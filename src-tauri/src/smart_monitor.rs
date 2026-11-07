//! Hourly SMART checks while the app is running.
//!
//! This only works when the process is elevated; otherwise Windows refuses to
//! open `\\.\PhysicalDriveN`. The explicit button in the UI remains available
//! for the non-elevated case and goes through the UAC helper.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use chrono::TimeZone;
use tauri::AppHandle;

use crate::smart::{self, SmartDisk};

const START_DELAY: Duration = Duration::from_secs(20);
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

static STARTED: AtomicBool = AtomicBool::new(false);
static ELEVATION_WARNED: AtomicBool = AtomicBool::new(false);
static SUMMARY: Mutex<Option<SmartSummary>> = Mutex::new(None);

#[derive(Debug, Clone, Copy)]
pub struct SmartSummary {
    pub checked_at: i64,
    pub drives: usize,
    pub warnings: usize,
}

fn should_check(background_enabled: bool, elevated: bool) -> bool {
    background_enabled && elevated
}

fn warning_count(disks: &[SmartDisk]) -> usize {
    disks.iter().map(|disk| disk.warnings.len()).sum()
}

pub fn update(disks: &[SmartDisk]) {
    let summary = SmartSummary {
        checked_at: chrono::Utc::now().timestamp_millis(),
        drives: disks.len(),
        warnings: warning_count(disks),
    };
    *SUMMARY.lock().unwrap_or_else(|error| error.into_inner()) = Some(summary);
}

pub fn summary() -> Option<SmartSummary> {
    *SUMMARY.lock().unwrap_or_else(|error| error.into_inner())
}

fn run_once(app: &AppHandle) {
    let settings = crate::settings::current();
    if !should_check(settings.smart_background, smart::is_elevated()) {
        if !smart::is_elevated() && !ELEVATION_WARNED.swap(true, Ordering::SeqCst) {
            crate::logging::log("smart: background check skipped (not elevated)");
        }
        return;
    }

    let disks = smart::read_all_excluding_usb();
    crate::smart_history::record(&disks);
    crate::smart_alerts::check_smart(app, &disks);
    update(&disks);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || crate::refresh_tray_menu(&handle));
    crate::logging::log(&format!(
        "smart: background check: {} drive(s), {} warning(s)",
        disks.len(),
        warning_count(&disks)
    ));
}

pub fn start(app: AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(START_DELAY);
        loop {
            run_once(&app);
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

/// Menu label for the tray; kept here so the tray builder only renders it.
pub fn tray_label() -> String {
    if !smart::is_elevated() {
        return crate::locale::tr(
            "Здоровье: нужны права администратора",
            "Health: administrator rights required",
        )
        .into();
    }
    match summary() {
        None => crate::locale::tr("Здоровье: нет данных", "Health: no data").into(),
        Some(summary) if summary.drives == 0 => {
            crate::locale::tr("Здоровье: нет данных", "Health: no data").into()
        }
        Some(summary) if summary.warnings > 0 => format!(
            "{}: {} ({})",
            crate::locale::tr("Здоровье", "Health"),
            crate::locale::tr("внимание", "attention"),
            summary.warnings
        ),
        Some(_) => crate::locale::tr("Здоровье: OK", "Health: OK").into(),
    }
}

pub fn diagnostics_line() -> String {
    if !smart::is_elevated() {
        return "uac".into();
    }
    let Some(summary) = summary() else {
        return "direct, no check yet".into();
    };
    let checked = chrono::Local
        .timestamp_millis_opt(summary.checked_at)
        .single()
        .map(|value| value.format("%d.%m %H:%M").to_string())
        .unwrap_or_else(|| "?".into());
    format!(
        "direct, drives={}, warnings={}, checked={checked}",
        summary.drives, summary.warnings
    )
}

#[cfg(test)]
mod tests {
    use super::should_check;

    #[test]
    fn background_check_needs_both_switch_and_elevation() {
        assert!(should_check(true, true));
        assert!(!should_check(false, true));
        assert!(!should_check(true, false));
        assert!(!should_check(false, false));
    }
}
