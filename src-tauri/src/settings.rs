//! Persisted application settings: notifications, hotkeys and user-protected
//! paths. Stored next to the other app data as `settings.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::paths::{self, normalize};

fn default_low_disk_percent() -> u8 {
    10
}

fn default_cooldown_minutes() -> u32 {
    60
}

fn default_hotkey_show() -> String {
    "CmdOrCtrl+Alt+D".into()
}

fn default_hotkey_widget() -> String {
    "CmdOrCtrl+Alt+W".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub notify_low_disk: bool,
    pub low_disk_percent: u8,
    pub low_disk_cooldown_min: u32,
    pub quick_clean_notify: bool,
    pub minimize_to_tray: bool,
    pub always_admin: bool,
    pub protect_paths: Vec<String>,
    pub hotkey_show: String,
    pub hotkey_widget: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            notify_low_disk: true,
            low_disk_percent: default_low_disk_percent(),
            low_disk_cooldown_min: default_cooldown_minutes(),
            quick_clean_notify: true,
            minimize_to_tray: true,
            always_admin: false,
            protect_paths: Vec::new(),
            hotkey_show: default_hotkey_show(),
            hotkey_widget: default_hotkey_widget(),
        }
    }
}

impl AppSettings {
    /// Keeps persisted values in a sane range and normalizes the path list for
    /// storage (the original case is kept for display).
    pub fn sanitize(mut self) -> Self {
        self.low_disk_percent = self.low_disk_percent.clamp(1, 50);
        self.low_disk_cooldown_min = self.low_disk_cooldown_min.clamp(5, 1440);
        self.hotkey_show = self.hotkey_show.trim().to_string();
        self.hotkey_widget = self.hotkey_widget.trim().to_string();

        let mut seen = std::collections::HashSet::new();
        self.protect_paths = self
            .protect_paths
            .into_iter()
            .map(|value| value.trim().trim_end_matches('\\').to_string())
            .filter(|value| Path::new(value).is_absolute())
            .filter(|value| seen.insert(normalize(Path::new(value))))
            .collect();

        self
    }
}

pub fn settings_file() -> PathBuf {
    paths::storage_path(&["settings.json"])
}

pub fn load() -> AppSettings {
    fs::read_to_string(settings_file())
        .ok()
        .and_then(|raw| serde_json::from_str::<AppSettings>(&raw).ok())
        .map(AppSettings::sanitize)
        .unwrap_or_default()
}

pub fn save(settings: &AppSettings) -> Result<AppSettings, String> {
    let clean = settings.clone().sanitize();
    let path = settings_file();
    paths::ensure_parent(&path);
    let json = serde_json::to_string_pretty(&clean).map_err(|error| error.to_string())?;
    fs::write(&path, json).map_err(|error| format!("Не удалось сохранить настройки: {error}"))?;
    *CURRENT.lock().unwrap() = Some(clean.clone());
    Ok(clean)
}

static CURRENT: Mutex<Option<AppSettings>> = Mutex::new(None);

pub fn init() -> AppSettings {
    let loaded = load();
    *CURRENT.lock().unwrap() = Some(loaded.clone());
    loaded
}

pub fn current() -> AppSettings {
    let cached = CURRENT.lock().unwrap().clone();
    cached.unwrap_or_else(load)
}

/// Normalized user-protected prefixes, ready for path comparisons.
pub fn protected_paths() -> Vec<String> {
    current()
        .protect_paths
        .iter()
        .map(|value| normalize(Path::new(value)))
        .collect()
}

pub fn export_json() -> Result<String, String> {
    serde_json::to_string_pretty(&current()).map_err(|error| error.to_string())
}

pub fn import_json(raw: &str) -> Result<AppSettings, String> {
    let parsed: AppSettings =
        serde_json::from_str(raw).map_err(|error| format!("Некорректный JSON: {error}"))?;
    save(&parsed)
}

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn sanitize_clamps_thresholds() {
        let settings = AppSettings {
            low_disk_percent: 0,
            low_disk_cooldown_min: 1,
            ..Default::default()
        }
        .sanitize();
        assert_eq!(settings.low_disk_percent, 1);
        assert_eq!(settings.low_disk_cooldown_min, 5);
    }

    #[test]
    fn sanitize_drops_relative_and_duplicate_paths() {
        let settings = AppSettings {
            protect_paths: vec![
                "relative\\path".into(),
                "D:\\Photos\\".into(),
                "d:\\photos".into(),
            ],
            ..Default::default()
        }
        .sanitize();
        assert_eq!(settings.protect_paths, vec!["D:\\Photos".to_string()]);
    }

    #[test]
    fn default_hotkeys_are_set() {
        let settings = AppSettings::default();
        assert!(!settings.hotkey_show.is_empty());
        assert!(!settings.hotkey_widget.is_empty());
    }
}
