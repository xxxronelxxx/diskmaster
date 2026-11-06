use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::paths;

const MAX_ENTRIES: usize = 500_000;
const MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Clone)]
pub struct CacheEntry {
    pub size: u64,
    pub modified: i64,
    pub hash: String,
    pub cached_at: i64,
}

pub type Snapshot = HashMap<String, CacheEntry>;

fn cache_file() -> PathBuf {
    paths::storage_path(&["duplicates-hashes.tsv"])
}

fn key(path: &Path) -> String {
    paths::normalize(path)
}

pub fn load() -> Snapshot {
    let Ok(raw) = fs::read_to_string(cache_file()) else {
        return Snapshot::new();
    };
    let now = chrono::Utc::now().timestamp_millis();
    let mut result = Snapshot::new();

    for line in raw.lines().take(MAX_ENTRIES) {
        let mut parts = line.split('|');
        let (Some(path), Some(size), Some(modified), Some(hash), Some(cached_at)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            continue;
        };
        let (Ok(size), Ok(modified), Ok(cached_at)) = (
            size.parse::<u64>(),
            modified.parse::<i64>(),
            cached_at.parse::<i64>(),
        ) else {
            continue;
        };
        if now - cached_at > MAX_AGE_MS {
            continue;
        }
        result.insert(
            path.to_string(),
            CacheEntry {
                size,
                modified,
                hash: hash.to_string(),
                cached_at,
            },
        );
    }

    result
}

pub fn lookup<'a>(
    snapshot: &'a Snapshot,
    path: &Path,
    size: u64,
    modified: i64,
) -> Option<&'a str> {
    let entry = snapshot.get(&key(path))?;
    if entry.size == size && entry.modified == modified {
        Some(entry.hash.as_str())
    } else {
        None
    }
}

/// Merges newly computed hashes into the on-disk cache (atomic replace) and
/// drops entries beyond the size limit.
pub fn merge_and_save(snapshot: &Snapshot, new_entries: Vec<(String, CacheEntry)>) {
    let now = chrono::Utc::now().timestamp_millis();
    let mut combined: Snapshot = snapshot.clone();

    for (path, mut entry) in new_entries {
        entry.cached_at = now;
        combined.insert(path, entry);
    }

    if combined.len() > MAX_ENTRIES {
        let excess = combined.len() - MAX_ENTRIES;
        let keys: Vec<String> = combined.keys().take(excess).cloned().collect();
        for key in keys {
            combined.remove(&key);
        }
    }

    let path = cache_file();
    paths::ensure_parent(&path);
    let temp = path.with_extension("tsv.tmp");

    let Ok(mut file) = fs::File::create(&temp) else {
        return;
    };
    for (path, entry) in &combined {
        let _ = writeln!(
            file,
            "{path}|{}|{}|{}|{}",
            entry.size, entry.modified, entry.hash, entry.cached_at
        );
    }
    let _ = file.flush();
    drop(file);
    let _ = fs::rename(temp, path);
}

pub fn make_entry(size: u64, modified: i64, hash: String) -> CacheEntry {
    CacheEntry {
        size,
        modified,
        hash,
        cached_at: chrono::Utc::now().timestamp_millis(),
    }
}
