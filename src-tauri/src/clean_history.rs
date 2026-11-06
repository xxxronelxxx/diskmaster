use std::fs;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::paths::{ensure_parent, storage_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanHistoryEntry {
    pub at: i64,
    pub action: String,
    pub trigger: String,
    pub categories: Vec<String>,
    pub bytes: u64,
    pub files: u64,
    pub errors: Vec<String>,
}

fn file() -> std::path::PathBuf {
    storage_path(&["clean-history.log"])
}

pub fn record(
    action: &str,
    trigger: &str,
    categories: &[String],
    bytes: u64,
    files: u64,
    errors: &[String],
) {
    let entry = CleanHistoryEntry {
        at: chrono::Utc::now().timestamp_millis(),
        action: action.into(),
        trigger: trigger.into(),
        categories: categories.to_vec(),
        bytes,
        files,
        errors: errors.iter().take(20).cloned().collect(),
    };

    let path = file();
    ensure_parent(&path);
    if let Ok(line) = serde_json::to_string(&entry) {
        if let Ok(mut handle) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(handle, "{line}");
        }
    }
}

pub fn read(limit: usize) -> Vec<CleanHistoryEntry> {
    let Ok(raw) = fs::read_to_string(file()) else {
        return Vec::new();
    };
    let mut entries: Vec<CleanHistoryEntry> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<CleanHistoryEntry>(line).ok())
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}
