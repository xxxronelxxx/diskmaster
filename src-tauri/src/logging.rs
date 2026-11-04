use std::fs::OpenOptions;
use std::io::Write;

use crate::paths::storage_path;

/// Minimal append-only log. Rotation is intentionally not implemented so a
/// crash report is never lost; the file grows by a few kilobytes per session.
pub fn log(message: &str) {
    let path = storage_path(&["logs", "app.log"]);
    crate::paths::ensure_parent(&path);

    let line = format!(
        "{}  {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        message.replace('\n', " ")
    );

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}
