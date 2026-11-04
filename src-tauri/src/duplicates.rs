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

const QUICK_HASH_THRESHOLD: u64 = 256 * 1024;
const QUICK_HASH_CHUNK: u64 = 64 * 1024;

fn hash_file(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
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

    let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
    let mut scanned: u64 = 0;

    let walker = jwalk::WalkDir::new(&root)
        .max_depth(12)
        .skip_hidden(false)
        .process_read_dir(|_depth, _path, _state, children| {
            children.retain(|entry| match entry {
                Ok(entry) => {
                    !entry.file_type().is_dir()
                        || !skip_dir(&entry.file_name().to_string_lossy())
                }
                Err(_) => true,
            });
        })
        .into_iter();

    for entry in walker {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        if Instant::now() > deadline {
            scan.truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        scanned += 1;
        if scanned % 2000 == 0 {
            let percent = ((started.elapsed().as_secs_f64() / 45.0) * 30.0).min(30.0) as u32;
            publish_progress(shared, "walking", scanned, 0, 0, percent, false);
        }
        if metadata.len() < min_size {
            continue;
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as i64)
            .unwrap_or(0);
        by_size.entry(metadata.len()).or_default().push(Candidate {
            path: entry.path(),
            size: metadata.len(),
            modified,
        });
    }

    scan.scanned_files = scanned;

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

    let processed = AtomicU64::new(0);
    let total = candidates.len() as u64;

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
                publish_progress(shared, "quick", scanned, done, total, percent.min(69), false);
            }
            Some((candidate.clone(), hash))
        })
        .collect();

    let mut quick_groups: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (candidate, hash) in quick {
        quick_groups
            .entry((candidate.size, hash))
            .or_default()
            .push(candidate);
    }

    let mut final_results: Vec<(Candidate, String)> = Vec::new();
    let mut full_candidates: Vec<Candidate> = Vec::new();

    for ((size, _hash), items) in quick_groups {
        if items.len() < 2 {
            continue;
        }
        if size > QUICK_HASH_THRESHOLD {
            full_candidates.extend(items);
        } else {
            // Already a full hash for small files; keep the group as is.
            for item in items {
                final_results.push((item.clone(), _hash.clone()));
            }
        }
    }

    let full_total = full_candidates.len() as u64;
    if full_total > 0 {
        publish_progress(shared, "hashing", scanned, 0, full_total, 70, false);
    }

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
                publish_progress(shared, "hashing", scanned, done, full_total, percent.min(99), false);
            }
            Some((candidate.clone(), hash))
        })
        .collect();
    final_results.extend(full);

    let results = final_results;

    let cancelled = cancel.load(Ordering::SeqCst);
    let mut grouped: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (candidate, hash) in results {
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
