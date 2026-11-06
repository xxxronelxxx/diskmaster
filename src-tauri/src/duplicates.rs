use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::paths::home_dir;
use crate::state::AppState;
use crate::types::{DuplicateFile, DuplicateGroup, DuplicateProgress, DuplicateScan};

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "appdata",
    ".cache",
    "venv",
    ".venv",
    "__pycache__",
    "dist",
    "build",
    "target",
    "bin",
    "obj",
];

const QUICK_HASH_THRESHOLD: u64 = 256 * 1024;
const QUICK_HASH_CHUNK: u64 = 64 * 1024;

pub struct DuplicateState {
    pub last: Option<DuplicateScan>,
    pub progress: Option<DuplicateProgress>,
    pub cancel: Arc<AtomicBool>,
}

impl Default for DuplicateState {
    fn default() -> Self {
        Self {
            last: None,
            progress: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Clone)]
struct Candidate {
    path: PathBuf,
    size: u64,
    modified: i64,
}

fn skip_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name.to_lowercase().as_str())
}

fn hash_file(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    // io::copy uses a small 8 KiB buffer; 1 MiB keeps NVMe drives busy.
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hasher.finalize().to_hex().to_string())
}

/// Cheap first pass for large files: hash the first and last 64 KiB. Two files
/// with different heads/tails cannot be identical, so the expensive full read
/// only happens for groups that survive this check.
fn quick_hash(path: &Path, size: u64) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; QUICK_HASH_CHUNK as usize];

    let read = file.read(&mut buffer).ok()?;
    hasher.update(&buffer[..read]);

    if size > QUICK_HASH_CHUNK * 2 {
        file.seek(SeekFrom::End(-(QUICK_HASH_CHUNK as i64))).ok()?;
        let read = file.read(&mut buffer).ok()?;
        hasher.update(&buffer[..read]);
    }

    Some(hasher.finalize().to_hex().to_string())
}

fn publish_progress(
    shared: &AppState,
    phase: &str,
    scanned: u64,
    hashed: u64,
    total: u64,
    percent: u32,
    cancelled: bool,
) {
    if let Some(progress) = shared.duplicates.lock().unwrap().progress.as_mut() {
        progress.phase = phase.into();
        progress.scanned = scanned;
        progress.hashed = hashed;
        progress.total = total;
        progress.percent = percent;
        progress.cancelled = cancelled;
    }
}

pub fn progress(shared: &AppState) -> Option<DuplicateProgress> {
    shared.duplicates.lock().unwrap().progress.clone()
}

pub fn cancel(shared: &AppState) {
    let state = shared.duplicates.lock().unwrap();
    state.cancel.store(true, Ordering::SeqCst);
}

fn wide_path(path: &Path) -> Vec<u16> {
    let text = path.to_string_lossy();
    let prefixed = if text.len() > 240 && !text.starts_with("\\\\?\\") {
        format!("\\\\?\\{text}")
    } else {
        text.to_string()
    };
    prefixed.encode_utf16().chain(std::iter::once(0)).collect()
}

fn filetime_to_millis(high: u32, low: u32) -> i64 {
    let ticks = ((high as u64) << 32 | low as u64) as i64;
    ((ticks - 116_444_736_000_000_000) / 10_000).max(0)
}

/// Enumerates a directory tree with `FindFirstFileEx`, which returns the size
/// and timestamp together with the name. Per-file `stat` calls were the main
/// reason the old walker took 25–40 seconds on a 300k-file profile.
fn collect_candidates(
    shared: &AppState,
    root: &Path,
    min_size: u64,
    started: Instant,
    deadline: Instant,
    cancel: &AtomicBool,
) -> (HashMap<u64, Vec<Candidate>>, u64, bool) {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, WIN32_FIND_DATAW,
    };

    let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut scanned = 0u64;
    let mut truncated = false;

    while let Some((directory, depth)) = stack.pop() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() > deadline {
            truncated = true;
            break;
        }

        let pattern = directory.join("*");
        let wide = wide_path(&pattern);
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        let handle = unsafe {
            FindFirstFileExW(
                wide.as_ptr(),
                FindExInfoBasic,
                &mut data as *mut _ as *mut c_void,
                FindExSearchNameMatch,
                std::ptr::null(),
                0,
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            continue;
        }

        loop {
            if cancel.load(Ordering::SeqCst) || Instant::now() > deadline {
                truncated = true;
                break;
            }

            let end = data
                .cFileName
                .iter()
                .position(|&value| value == 0)
                .unwrap_or(data.cFileName.len());
            let name = String::from_utf16_lossy(&data.cFileName[..end]);

            if name != "." && name != ".." {
                let attributes = data.dwFileAttributes;
                if attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                    if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                        if depth < 12 && !skip_dir(&name) {
                            stack.push((directory.join(&name), depth + 1));
                        }
                    } else {
                        scanned += 1;
                        if scanned % 2000 == 0 {
                            let percent =
                                ((started.elapsed().as_secs_f64() / 45.0) * 30.0).min(30.0) as u32;
                            publish_progress(shared, "walking", scanned, 0, 0, percent, false);
                        }

                        let size = ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64;
                        if size >= min_size {
                            by_size.entry(size).or_default().push(Candidate {
                                path: directory.join(&name),
                                size,
                                modified: filetime_to_millis(
                                    data.ftLastWriteTime.dwHighDateTime,
                                    data.ftLastWriteTime.dwLowDateTime,
                                ),
                            });
                        }
                    }
                }
            }

            let mut next: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
            if unsafe { FindNextFileW(handle, &mut next) } == 0 {
                break;
            }
            data = next;
        }

        unsafe {
            FindClose(handle);
        }
    }

    (by_size, scanned, truncated)
}

pub fn scan_duplicates(
    shared: &AppState,
    root: Option<String>,
    min_size_kb: Option<f64>,
) -> DuplicateScan {
    let started = Instant::now();
    let root = root
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(home_dir);
    let min_size = ((min_size_kb.unwrap_or(64.0).max(1.0) as u64) * 1024).max(1024);
    let deadline = started + Duration::from_secs(45);

    // Paths from the previous scan must not stay deletable.
    shared.allow.lock().unwrap().clear();

    let cancel = {
        let mut state = shared.duplicates.lock().unwrap();
        state.cancel.store(false, Ordering::SeqCst);
        state.progress = Some(DuplicateProgress {
            phase: "walking".into(),
            scanned: 0,
            hashed: 0,
            total: 0,
            percent: 2,
            cancelled: false,
        });
        Arc::clone(&state.cancel)
    };

    let mut scan = DuplicateScan {
        id: format!("{}", chrono::Utc::now().timestamp_millis()),
        root: root.to_string_lossy().to_string(),
        min_size,
        scanned_files: 0,
        candidate_files: 0,
        groups: Vec::new(),
        elapsed_ms: 0,
        truncated: false,
        error: None,
    };

    let (by_size, scanned, truncated) =
        collect_candidates(shared, &root, min_size, started, deadline, &cancel);
    scan.scanned_files = scanned;
    scan.truncated = truncated;
    let walk_ms = started.elapsed().as_millis();

    if cancel.load(Ordering::SeqCst) {
        scan.error = Some("Сканирование отменено".into());
        scan.elapsed_ms = started.elapsed().as_millis() as u64;
        let mut state = shared.duplicates.lock().unwrap();
        state.progress = None;
        state.last = Some(scan.clone());
        return scan;
    }

    let candidates: Vec<Candidate> = by_size
        .into_iter()
        .filter(|(_, items)| items.len() > 1)
        .flat_map(|(_, items)| items)
        .collect();
    scan.candidate_files = candidates.len() as u64;
    let total = candidates.len() as u64;
    publish_progress(shared, "hashing", scanned, 0, total, 30, false);

    let quick_started = Instant::now();
    let processed = AtomicU64::new(0);

    // Pass 1: full hash for small files, cheap head+tail hash for large ones.
    let quick: Vec<(Candidate, String)> = candidates
        .par_iter()
        .filter_map(|candidate| {
            if cancel.load(Ordering::SeqCst) {
                return None;
            }
            let hash = if candidate.size > QUICK_HASH_THRESHOLD {
                quick_hash(&candidate.path, candidate.size)?
            } else {
                hash_file(&candidate.path)?
            };
            let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 500 == 0 {
                let percent = 30 + ((done as f64 / total.max(1) as f64) * 40.0) as u32;
                publish_progress(
                    shared,
                    "quick",
                    scanned,
                    done,
                    total,
                    percent.min(69),
                    false,
                );
            }
            Some((candidate.clone(), hash))
        })
        .collect();
    let quick_ms = quick_started.elapsed().as_millis();

    let mut quick_groups: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (candidate, hash) in quick {
        quick_groups
            .entry((candidate.size, hash))
            .or_default()
            .push(candidate);
    }

    let mut final_results: Vec<(Candidate, String)> = Vec::new();
    let mut full_candidates: Vec<Candidate> = Vec::new();

    for ((size, hash), items) in quick_groups {
        if items.len() < 2 {
            continue;
        }
        if size > QUICK_HASH_THRESHOLD {
            full_candidates.extend(items);
        } else {
            // Already a full hash for small files.
            for item in items {
                final_results.push((item, hash.clone()));
            }
        }
    }

    let full_total = full_candidates.len() as u64;
    if full_total > 0 {
        publish_progress(shared, "hashing", scanned, 0, full_total, 70, false);
    }

    let full_started = Instant::now();
    let full_hashed = AtomicU64::new(0);
    let full: Vec<(Candidate, String)> = full_candidates
        .par_iter()
        .filter_map(|candidate| {
            if cancel.load(Ordering::SeqCst) {
                return None;
            }
            let hash = hash_file(&candidate.path)?;
            let done = full_hashed.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 200 == 0 {
                let percent = 70 + ((done as f64 / full_total.max(1) as f64) * 29.0) as u32;
                publish_progress(
                    shared,
                    "hashing",
                    scanned,
                    done,
                    full_total,
                    percent.min(99),
                    false,
                );
            }
            Some((candidate.clone(), hash))
        })
        .collect();
    let full_ms = full_started.elapsed().as_millis();
    final_results.extend(full);

    let cancelled = cancel.load(Ordering::SeqCst);

    let mut grouped: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (candidate, hash) in final_results {
        grouped
            .entry((candidate.size, hash))
            .or_default()
            .push(candidate);
    }

    let mut groups: Vec<DuplicateGroup> = grouped
        .into_iter()
        .filter(|(_, items)| items.len() > 1)
        .map(|((size, hash), items)| {
            let mut files: Vec<DuplicateFile> = items
                .iter()
                .map(|candidate| DuplicateFile {
                    path: candidate.path.to_string_lossy().to_string(),
                    name: candidate
                        .path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    dir: candidate
                        .path
                        .parent()
                        .map(|parent| parent.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    size,
                    modified: candidate.modified,
                })
                .collect();
            files.sort_by(|a, b| a.path.cmp(&b.path));
            DuplicateGroup {
                hash,
                size,
                count: files.len() as u64,
                wasted: size.saturating_mul(files.len().saturating_sub(1) as u64),
                files,
            }
        })
        .collect();

    groups.sort_by_key(|group| std::cmp::Reverse(group.wasted));
    scan.groups = groups;
    scan.elapsed_ms = started.elapsed().as_millis() as u64;

    if cancelled {
        scan.error = Some("Сканирование отменено".into());
        scan.truncated = true;
    }

    {
        let mut allow = shared.allow.lock().unwrap();
        for group in &scan.groups {
            for file in &group.files {
                allow.register(Path::new(&file.path));
            }
        }
    }

    crate::logging::log(&format!(
        "duplicates scan: walk={walk_ms}ms quick={quick_ms}ms full={full_ms}ms total={}ms ({} files, {} candidates)",
        started.elapsed().as_millis(),
        scan.scanned_files,
        scan.candidate_files
    ));

    {
        let mut state = shared.duplicates.lock().unwrap();
        state.progress = None;
        state.last = Some(scan.clone());
    }
    scan
}

pub fn last_scan(shared: &AppState) -> Option<DuplicateScan> {
    shared.duplicates.lock().unwrap().last.clone()
}
