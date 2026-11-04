use std::fs;
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

/// Directory used by the old Electron build and by early Tauri builds. Also
/// happens to be the default install directory, which is why user data was
/// moved out of it.
fn legacy_dir() -> PathBuf {
    local_appdata().join("DiskMaster")
}

/// Persistent app data. Kept separate from the install directory so
/// uninstalling the app cannot remove history or widget settings.
pub fn data_dir() -> PathBuf {
    local_appdata().join("DiskMasterData")
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
                if fs::create_dir_all(&candidate).is_ok() {
                    // Verify the directory is actually writable.
                    let probe = candidate.join(".write-test");
                    if fs::write(&probe, b"ok").is_ok() {
                        let _ = fs::remove_file(&probe);
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

fn copy_dir(from: &Path, to: &Path) {
    if !from.is_dir() {
        return;
    }
    let _ = fs::create_dir_all(to);
    let Ok(entries) = fs::read_dir(from) else {
        return;
    };
    for entry in entries.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir(&source, &target);
        } else if !target.exists() {
            let _ = fs::copy(&source, &target);
        }
    }
}

/// Copies legacy data into the new directory once. Nothing is deleted: the old
/// folder is left untouched so a downgrade still works.
pub fn migrate_legacy_data() {
    let target = data_dir();
    if target.exists() {
        return;
    }

    let legacy = legacy_dir();
    let candidates = ["history", "schedule", "widget"];
    let has_data = candidates
        .iter()
        .any(|name| legacy.join(name).exists())
        || legacy.join("window-state.json").exists();
    if !has_data {
        return;
    }

    let _ = fs::create_dir_all(&target);
    for name in candidates {
        copy_dir(&legacy.join(name), &target.join(name));
    }
    let state = legacy.join("window-state.json");
    if state.exists() {
        let _ = fs::copy(&state, target.join("window-state.json"));
    }
}

pub fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
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
