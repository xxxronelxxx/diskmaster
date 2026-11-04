use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::paths::{home_dir, local_appdata, normalize, system_root};

/// Directories that must never be deleted from, even inside an allow-listed
/// tree. Compared case-insensitively with a trailing separator boundary.
fn protected_prefixes() -> Vec<String> {
    let list = vec![
        system_root(),
        PathBuf::from("C:\\Program Files"),
        PathBuf::from("C:\\Program Files (x86)"),
        PathBuf::from("C:\\ProgramData"),
        PathBuf::from("C:\\$Recycle.Bin"),
        PathBuf::from("C:\\System Volume Information"),
        PathBuf::from("C:\\Recovery"),
        PathBuf::from("C:\\PerfLogs"),
        PathBuf::from("C:\\MSOCache"),
        local_appdata().join("Microsoft"),
    ];
    list.into_iter().map(|path| normalize(&path)).collect()
}

fn depth(path: &Path) -> usize {
    path.components().count()
}

/// A path is deletable when it is absolute, not a drive/profile root, not in a
/// protected system location and not a reparse point (symlink/junction).
pub fn check_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Путь должен быть абсолютным".into());
    }
    let resolved = path.to_path_buf();

    if depth(&resolved) <= 2 {
        return Err("Защищённый путь: корень диска".into());
    }
    if resolved == home_dir() {
        return Err("Защищённый путь: папка пользователя".into());
    }
    if resolved == local_appdata() {
        return Err("Защищённый путь: AppData".into());
    }

    let normalized = normalize(&resolved);
    for prefix in protected_prefixes() {
        if normalized == prefix || normalized.starts_with(&format!("{prefix}\\")) {
            return Err(format!("Защищённый системный путь: {}", resolved.display()));
        }
    }

    let metadata = std::fs::symlink_metadata(&resolved)
        .map_err(|_| format!("Не найдено: {}", resolved.display()))?;
    if metadata.file_type().is_symlink() {
        return Err("Символические ссылки и junction-папки защищены".into());
    }

    Ok(resolved)
}

/// Paths the user has explicitly seen in scan/analysis results. Deleting
/// anything outside this set is rejected by the RPC layer.
#[derive(Default)]
pub struct AllowList {
    paths: HashSet<String>,
    updated_at: i64,
}

#[allow(dead_code)]
impl AllowList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, path: &Path) {
        self.paths.insert(normalize(path));
        self.touch();
    }

    pub fn register_many<'a, I: IntoIterator<Item = &'a PathBuf>>(&mut self, paths: I) {
        for path in paths {
            self.paths.insert(normalize(path));
        }
        self.touch();
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Only the exact paths returned by a scan/analysis are deletable. A
    /// previously registered parent directory does not implicitly allow every
    /// descendant.
    pub fn allows(&self, path: &Path) -> bool {
        self.paths.contains(&normalize(path))
    }

    /// The allow-list is rebuilt on every scan; it is pointless after a long
    /// idle period and would otherwise grow forever.
    pub fn is_stale(&self, max_age_ms: i64) -> bool {
        self.updated_at == 0 || chrono::Utc::now().timestamp_millis() - self.updated_at > max_age_ms
    }
}

/// Validates a destructive request: the path must be safe on its own *and*
/// must have been surfaced to the user by a scan/analysis.
pub fn assert_deletable(path: &Path, allow: &AllowList) -> Result<PathBuf, String> {
    let resolved = check_path(path)?;
    if !allow.allows(&resolved) {
        return Err(format!(
            "Путь не из результатов анализа: {}",
            resolved.display()
        ));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_system_locations() {
        assert!(check_path(Path::new("C:\\Windows\\System32")).is_err());
        assert!(check_path(Path::new("C:\\Program Files\\Something")).is_err());
        assert!(check_path(Path::new("C:\\ProgramData\\App")).is_err());
        assert!(check_path(Path::new("C:\\")).is_err());
    }

    #[test]
    fn allow_list_only_accepts_registered_paths() {
        let mut allow = AllowList::new();
        let allowed = std::env::temp_dir().join("disk-master-allowed");
        let other = std::env::temp_dir().join("disk-master-other");

        assert!(assert_deletable(&allowed, &allow).is_err());
        allow.register(&allowed);
        assert!(allow.allows(&allowed));
        assert!(!allow.allows(&other));
        assert!(!allow.allows(&allowed.join("child.txt")));
    }

    #[test]
    fn recycle_bin_api_accepts_registered_temp_file() {
        let mut allow = AllowList::new();
        let path = std::env::temp_dir().join(format!(
            "disk-master-test-{}.tmp",
            std::process::id()
        ));
        std::fs::write(&path, b"test").unwrap();
        allow.register(&path);
        let resolved = assert_deletable(&path, &allow).unwrap();
        trash::delete(&resolved).unwrap();
        assert!(!path.exists());
    }
}
