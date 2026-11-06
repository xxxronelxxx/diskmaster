use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::paths::storage_path;

/// Minimal append-only log. Rotation is intentionally not implemented so a
/// crash report is never lost; the file grows by a few kilobytes per session.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

fn rotate_if_needed(path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() < MAX_LOG_BYTES {
        return;
    }

    let old = path.with_file_name("app.old.log");
    let _ = fs::remove_file(&old);
    let _ = fs::rename(path, old);
}

pub fn log(message: &str) {
    let path = storage_path(&["logs", "app.log"]);
    crate::paths::ensure_parent(&path);
    rotate_if_needed(&path);

    let line = format!(
        "{}  {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        message.replace('\n', " ")
    );

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}
