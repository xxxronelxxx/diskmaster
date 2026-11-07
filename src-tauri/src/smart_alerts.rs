//! SMART alerts: one system notification per drive per day, or immediately
//! when the set of warning conditions changes.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::maintenance::DiskHealth;
use crate::paths;
use crate::smart::{SmartDisk, SmartWarning};

const ALERT_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct AlertState {
    last_at: i64,
    signature: String,
}

type AlertMap = BTreeMap<String, AlertState>;

fn file() -> std::path::PathBuf {
    paths::storage_path(&["history", "smart-alerts.json"])
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

fn load() -> AlertMap {
    fs::read_to_string(file())
        .ok()
        .and_then(|raw| serde_json::from_str::<AlertMap>(&raw).ok())
        .unwrap_or_default()
}

fn save(state: &AlertMap) {
    let path = file();
    paths::ensure_parent(&path);
    if let Ok(raw) = serde_json::to_string(state) {
        let _ = fs::write(path, raw);
    }
}

fn signature(warnings: &[SmartWarning]) -> String {
    let mut keys: Vec<&str> = warnings.iter().map(|warning| warning.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    keys.join(",")
}

fn should_notify(previous: Option<&AlertState>, signature: &str, now: i64) -> bool {
    if signature.is_empty() {
        return false;
    }
    match previous {
        None => true,
        Some(previous) => {
            previous.signature != signature
                || now.saturating_sub(previous.last_at) >= ALERT_INTERVAL_MS
        }
    }
}

fn warning_text(warning: SmartWarning) -> &'static str {
    match warning {
        SmartWarning::AvailableSpare => {
            crate::locale::tr("резерв ниже порога", "spare below threshold")
        }
        SmartWarning::Temperature => {
            crate::locale::tr("температура вне нормы", "temperature out of range")
        }
        SmartWarning::Reliability => {
            crate::locale::tr("надёжность снижена", "reliability degraded")
        }
        SmartWarning::ReadOnly => crate::locale::tr("диск только для чтения", "drive is read-only"),
        SmartWarning::VolatileMemory => {
            crate::locale::tr("сбой резервного питания", "volatile memory backup failed")
        }
        SmartWarning::PersistentMemory => crate::locale::tr(
            "энергонезависимая память только для чтения",
            "persistent memory is read-only",
        ),
        SmartWarning::PredictFailure => {
            crate::locale::tr("возможен отказ диска", "drive failure predicted")
        }
    }
}

fn notify(app: &AppHandle, name: &str, warnings: &[SmartWarning]) {
    use tauri_plugin_notification::NotificationExt;

    let list = warnings
        .iter()
        .map(|warning| warning_text(*warning))
        .collect::<Vec<_>>()
        .join(", ");
    let body = if name.is_empty() {
        list
    } else {
        format!("{name}: {list}")
    };
    if let Err(error) = app
        .notification()
        .builder()
        .title(crate::locale::tr("Диск-Мастер", "Disk Master"))
        .body(body)
        .extra("tab", "maintenance")
        .show()
    {
        crate::logging::log(&format!("smart alert notification failed: {error}"));
    }
}

fn check_entries(app: &AppHandle, entries: Vec<(String, String, Vec<SmartWarning>)>) {
    if entries.is_empty() || !crate::settings::current().smart_notify {
        return;
    }

    let _guard = lock();
    let mut state = load();
    let now = chrono::Utc::now().timestamp_millis();
    let mut changed = false;

    for (key, name, warnings) in entries {
        if key.is_empty() {
            continue;
        }
        let next_signature = signature(&warnings);
        let previous = state.get(&key).cloned();

        if next_signature.is_empty() {
            if let Some(previous) = state.get_mut(&key) {
                if !previous.signature.is_empty() {
                    previous.signature.clear();
                    changed = true;
                }
            }
            continue;
        }

        if should_notify(previous.as_ref(), &next_signature, now) {
            notify(app, &name, &warnings);
            state.insert(
                key,
                AlertState {
                    last_at: now,
                    signature: next_signature,
                },
            );
            changed = true;
        }
    }

    if changed {
        save(&state);
    }
}

pub fn check_smart(app: &AppHandle, disks: &[SmartDisk]) {
    let entries = disks
        .iter()
        .map(|disk| {
            (
                disk.history_key.clone(),
                disk.model.clone(),
                disk.warnings.clone(),
            )
        })
        .collect();
    check_entries(app, entries);
}

pub fn check_health(app: &AppHandle, disks: &[DiskHealth]) {
    let entries = disks
        .iter()
        .filter_map(|disk| {
            Some((
                disk.history_key.clone()?,
                disk.name.clone(),
                disk.warnings.clone(),
            ))
        })
        .collect();
    check_entries(app, entries);
}

#[cfg(test)]
mod tests {
    use super::{should_notify, AlertState};

    fn state(last_at: i64, signature: &str) -> AlertState {
        AlertState {
            last_at,
            signature: signature.into(),
        }
    }

    #[test]
    fn first_alert_is_sent() {
        assert!(should_notify(None, "reliability", 1_000));
    }

    #[test]
    fn repeated_alert_waits_a_day() {
        let previous = state(1_000, "reliability");
        assert!(!should_notify(Some(&previous), "reliability", 2_000));
        assert!(should_notify(
            Some(&previous),
            "reliability",
            1_000 + 24 * 60 * 60 * 1000
        ));
    }

    #[test]
    fn new_warning_is_sent_immediately() {
        let previous = state(1_000, "reliability");
        assert!(should_notify(
            Some(&previous),
            "readOnly,reliability",
            2_000
        ));
    }

    #[test]
    fn healthy_state_does_not_notify() {
        assert!(!should_notify(None, "", 1_000));
    }
}
