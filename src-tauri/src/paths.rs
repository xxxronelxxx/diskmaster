use std::path::{Path, PathBuf};

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\Default"))
}

pub fn local_appdata() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData").join("Local"))
}

pub fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
}

/// `%LOCALAPPDATA%\DiskMaster` — shared by the installed and portable builds.
pub fn data_dir() -> PathBuf {
    local_appdata().join("DiskMaster")
}

/// Portable builds place a `portable.marker` next to the executable. The whole
/// folder can then be copied around, and data lives beside the app.
pub fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("portable.marker").exists()))
        .unwrap_or(false)
}

/// Portable builds keep their data next to the executable so the whole folder
/// can be copied around. Falls back to `%LOCALAPPDATA%` when the folder is
/// read-only (e.g. copied into Program Files).
pub fn storage_root() -> PathBuf {
    if is_portable() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join("disk-master-data");
                if std::fs::create_dir_all(&candidate).is_ok() {
                    // Verify the directory is actually writable.
                    let probe = candidate.join(".write-test");
                    if std::fs::write(&probe, b"ok").is_ok() {
                        let _ = std::fs::remove_file(&probe);
                        return candidate;
                    }
                }
            }
        }
    }
    data_dir()
}

pub fn storage_path(parts: &[&str]) -> PathBuf {
    let mut path = storage_root();
    for part in parts {
        path.push(part);
    }
    path
}

pub fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}

pub fn normalize(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let trimmed = text.trim_end_matches('\\');
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        format!("{trimmed}\\")
    } else {
        trimmed.to_lowercase()
    }
}
