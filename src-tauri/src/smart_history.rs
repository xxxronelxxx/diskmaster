//! Local SMART history: one sample per hour per drive, kept for 90 days.
//!
//! The file lives next to the other app data and never leaves the machine.
//! Recording is serialized with a mutex because the maintenance query and the
//! explicit “Refresh SMART” button can finish at the same time.

use std::fs;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::smart::{SmartDisk, SmartProtocol};

const VERSION: u32 = 1;
const SAMPLE_INTERVAL_MS: i64 = 60 * 60 * 1000;
const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;
const MAX_SAMPLES: usize = 2200;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SmartSample {
    pub t: i64,
    pub temperature: Option<u32>,
    pub wear: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub media_errors: Option<u64>,
    pub critical_warning: u8,
    pub health_passed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartHistoryDrive {
    pub key: String,
    pub protocol: SmartProtocol,
    pub model: String,
    pub samples: Vec<SmartSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct HistoryFile {
    version: u32,
    drives: Vec<SmartHistoryDrive>,
}

fn file() -> std::path::PathBuf {
    paths::storage_path(&["history", "smart.json"])
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn load() -> HistoryFile {
    fs::read_to_string(file())
        .ok()
        .and_then(|raw| serde_json::from_str::<HistoryFile>(&raw).ok())
        .unwrap_or_default()
}

fn save(history: &HistoryFile) {
    let path = file();
    paths::ensure_parent(&path);
    if let Ok(raw) = serde_json::to_string(history) {
        let _ = fs::write(path, raw);
    }
}

fn sample_from(disk: &SmartDisk, now: i64) -> SmartSample {
    SmartSample {
        t: now,
        temperature: disk.temperature,
        wear: disk.wear,
        power_on_hours: disk.power_on_hours,
        media_errors: disk.media_errors,
        critical_warning: disk.critical_warning,
        health_passed: disk.health_passed,
    }
}

fn needs_sample(last: Option<&SmartSample>, sample: &SmartSample, now: i64) -> bool {
    let Some(last) = last else {
        return true;
    };
    let warning_changed = last.critical_warning != sample.critical_warning
        || last.health_passed != sample.health_passed;
    warning_changed || now.saturating_sub(last.t) >= SAMPLE_INTERVAL_MS
}

fn trim(samples: &mut Vec<SmartSample>, now: i64) {
    let cutoff = now.saturating_sub(RETENTION_MS);
    samples.retain(|sample| sample.t >= cutoff);
    if samples.len() > MAX_SAMPLES {
        samples.drain(..samples.len() - MAX_SAMPLES);
    }
}

/// Persists samples for every drive that returned SMART data.
pub fn record(disks: &[SmartDisk]) {
    if disks.is_empty() {
        return;
    }
    let _guard = lock();
    let mut history = load();
    let now = now_ms();

    for disk in disks {
        if disk.history_key.is_empty() {
            continue;
        }
        let index = match history
            .drives
            .iter()
            .position(|drive| drive.key == disk.history_key)
        {
            Some(index) => index,
            None => {
                history.drives.push(SmartHistoryDrive {
                    key: disk.history_key.clone(),
                    protocol: disk.protocol,
                    model: disk.model.clone(),
                    samples: Vec::new(),
                });
                history.drives.len() - 1
            }
        };
        let drive = &mut history.drives[index];
        drive.model = disk.model.clone();
        drive.protocol = disk.protocol;

        let sample = sample_from(disk, now);
        if needs_sample(drive.samples.last(), &sample, now) {
            drive.samples.push(sample);
        }
        trim(&mut drive.samples, now);
    }

    history.drives.retain(|drive| !drive.samples.is_empty());
    history.version = VERSION;
    save(&history);
}

/// Returns the local history, newest drive metadata included.
pub fn read() -> Vec<SmartHistoryDrive> {
    let _guard = lock();
    let mut history = load();
    let now = now_ms();
    for drive in &mut history.drives {
        trim(&mut drive.samples, now);
    }
    history.drives.retain(|drive| !drive.samples.is_empty());
    history.drives
}

#[cfg(test)]
mod tests {
    use super::{needs_sample, trim, SmartSample};

    fn sample(t: i64) -> SmartSample {
        SmartSample {
            t,
            temperature: Some(50),
            wear: Some(3),
            power_on_hours: Some(100),
            media_errors: Some(0),
            critical_warning: 0,
            health_passed: None,
        }
    }

    #[test]
    fn first_sample_and_hourly_interval() {
        let first = sample(1_000);
        assert!(needs_sample(None, &first, 1_000));
        assert!(!needs_sample(Some(&first), &sample(1_500), 1_500));
        assert!(needs_sample(
            Some(&first),
            &sample(1_000 + 60 * 60 * 1000),
            1_000 + 60 * 60 * 1000
        ));
    }

    #[test]
    fn warning_change_is_recorded_immediately() {
        let first = sample(1_000);
        let mut warning = sample(1_100);
        warning.critical_warning = 4;
        assert!(needs_sample(Some(&first), &warning, 1_100));
    }

    #[test]
    fn trims_old_points_and_caps_the_list() {
        let now = 100 * 24 * 60 * 60 * 1000;
        let mut samples: Vec<SmartSample> = (0..2500)
            .map(|index| sample(now - index * 60 * 60 * 1000))
            .collect();
        trim(&mut samples, now);
        assert!(samples.len() <= 2200);
        assert!(samples
            .iter()
            .all(|sample| sample.t >= now - 90 * 24 * 60 * 60 * 1000));
    }
}
