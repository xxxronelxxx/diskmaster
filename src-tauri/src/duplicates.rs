use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::paths::home_dir;
use crate::state::AppState;
use crate::types::{DuplicateFile, DuplicateGroup, DuplicateScan};

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

#[derive(Default)]
pub struct DuplicateState {
    pub last: Option<DuplicateScan>,
}

#[derive(Clone, Deserialize)]
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
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash).ok()?;
    Some(format!("{:x}", hash.finalize()))
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

    for entry in walker.filter_map(Result::ok) {
        if Instant::now() > deadline {
            scan.truncated = true;
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        scan.scanned_files += 1;
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

    let candidates: Vec<Candidate> = by_size
        .into_iter()
        .filter(|(_, items)| items.len() > 1)
        .flat_map(|(_, items)| items)
        .collect();
    scan.candidate_files = candidates.len() as u64;

    let hashed: Vec<(Candidate, String)> = candidates
        .par_iter()
        .filter_map(|candidate| hash_file(&candidate.path).map(|hash| (candidate.clone(), hash)))
        .collect();

    let mut grouped: HashMap<(u64, String), Vec<Candidate>> = HashMap::new();
    for (candidate, hash) in hashed {
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

    groups.sort_by(|a, b| b.wasted.cmp(&a.wasted));
    scan.groups = groups;
    scan.elapsed_ms = started.elapsed().as_millis() as u64;

    {
        let mut allow = shared.allow.lock().unwrap();
        for group in &scan.groups {
            for file in &group.files {
                allow.register(Path::new(&file.path));
            }
        }
    }
    shared.duplicates.lock().unwrap().last = Some(scan.clone());
    scan
}

pub fn last_scan(shared: &AppState) -> Option<DuplicateScan> {
    shared.duplicates.lock().unwrap().last.clone()
}
