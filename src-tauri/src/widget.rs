use std::fs;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::time::Duration;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

use crate::paths::{self, storage_path};
use crate::ps;
use crate::state::{AppState, WidgetRuntime};
use crate::types::{WidgetSettings, WidgetStatus};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "DiskMasterWidget";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn data_dir() -> std::path::PathBuf {
    storage_path(&["widget"])
}

fn settings_file() -> std::path::PathBuf {
    data_dir().join("widget-settings.json")
}

fn script_file() -> std::path::PathBuf {
    data_dir().join("monitor.ps1")
}

fn log_file() -> std::path::PathBuf {
    data_dir().join("widget.log")
}

fn ensure_dir() {
    let _ = fs::create_dir_all(data_dir());
}

pub fn read_settings() -> WidgetSettings {
    let Ok(raw) = fs::read_to_string(settings_file()) else {
        return WidgetSettings::default();
    };
    let cleaned = raw.trim_start_matches('\u{feff}');
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(cleaned) else {
        return WidgetSettings::default();
    };

    // Older builds stored the indicator bars under "sparklines".
    if value.get("bars").is_none() {
        if let Some(legacy) = value.get("sparklines").and_then(|v| v.as_bool()) {
            value["bars"] = serde_json::Value::Bool(legacy);
        }
    }

    clamp_settings(serde_json::from_value(value).unwrap_or_default())
}

fn valid_accent(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.len() == 7
        && bytes[0] == b'#'
        && bytes[1..].iter().all(|byte| byte.is_ascii_hexdigit())
}

/// Values arrive from the UI as JSON, so they are clamped before they reach the
/// WPF script. A bad width or opacity would otherwise break the widget window.
fn clamp_settings(mut settings: WidgetSettings) -> WidgetSettings {
    settings.width = settings.width.clamp(200, 1200);
    settings.height = settings.height.clamp(120, 900);
    settings.opacity = settings.opacity.clamp(0.2, 1.0);
    settings.radius = settings.radius.min(40);
    settings.font_scale = settings.font_scale.clamp(0.7, 2.0);
    settings.interval_ms = settings.interval_ms.clamp(500, 10_000);
    settings.title = settings.title.chars().take(40).collect();

    if !valid_accent(&settings.accent) {
        settings.accent = "#38e0a0".into();
    }
    if !matches!(settings.z_mode.as_str(), "normal" | "top" | "desktop") {
        settings.z_mode = "normal".into();
    }
    if settings.x < -1 || settings.x > 20_000 {
        settings.x = -1;
    }
    if settings.y < -1 || settings.y > 20_000 {
        settings.y = -1;
    }

    settings
}

pub fn save_settings(patch: &serde_json::Value) -> WidgetSettings {
    ensure_dir();
    let mut current = serde_json::to_value(read_settings()).unwrap_or_else(|_| serde_json::json!({}));
    if let (Some(base), Some(patch)) = (current.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            base.insert(key.clone(), value.clone());
        }
    }
    let merged = clamp_settings(serde_json::from_value(current).unwrap_or_default());
    let _ = fs::write(
        settings_file(),
        serde_json::to_string_pretty(&merged).unwrap_or_default(),
    );
    merged
}

pub fn ensure_script() -> std::path::PathBuf {
    ensure_dir();
    let source = include_str!("../scripts/monitor.ps1");
    let normalized = source.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let content = format!("\u{feff}{}", normalized.replace('\n', "\r\n"));
    let path = script_file();
    let needs_write = fs::read_to_string(&path)
        .map(|existing| existing != content)
        .unwrap_or(true);
    if needs_write {
        let _ = fs::write(&path, content);
    }
    path
}

fn is_child_alive(runtime: &mut WidgetRuntime) -> bool {
    match runtime.child.as_mut() {
        Some(child) => matches!(child.try_wait(), Ok(None)),
        None => false,
    }
}

pub fn is_running(shared: &AppState) -> bool {
    is_child_alive(&mut shared.widget.lock().unwrap())
}

fn path_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

pub fn launch(shared: &AppState, patch: Option<serde_json::Value>) -> (bool, Option<String>) {
    {
        let mut runtime = shared.widget.lock().unwrap();
        if is_child_alive(&mut runtime) {
            return (true, None);
        }
    }

    // Do not stack a second WPF widget on top of one left over from a previous
    // session (including the old Electron build).
    kill_orphans();

    let merged = match patch {
        Some(patch) => save_settings(&patch),
        None => read_settings(),
    };
    let script = ensure_script();
    let settings = settings_file();

    let child: Result<Child, _> = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            &script.to_string_lossy(),
            "-SettingsFile",
            &settings.to_string_lossy(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    let mut runtime = shared.widget.lock().unwrap();
    match child {
        Ok(mut process) => {
            std::thread::sleep(Duration::from_millis(1200));
            if matches!(process.try_wait(), Ok(None)) {
                runtime.child = Some(process);
                runtime.last_error = None;
                runtime.settings = Some(merged);
                (true, None)
            } else {
                let log = read_log();
                crate::logging::log(&format!(
                    "widget launch failed: {}",
                    log.clone().unwrap_or_else(|| "no output".into())
                ));
                runtime.child = None;
                runtime.last_error = log.clone();
                (
                    false,
                    Some(log.unwrap_or_else(|| "Виджет не запустился".into())),
                )
            }
        }
        Err(error) => {
            runtime.last_error = Some(error.to_string());
            (false, Some(error.to_string()))
        }
    }
}

pub fn stop(shared: &AppState) -> bool {
    crate::logging::log("widget stop");
    if let Some(mut child) = shared.widget.lock().unwrap().child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    kill_orphans();
    true
}

fn kill_orphans() {
    let _ = ps::run(
        "Get-CimInstance Win32_Process -Filter \"Name='powershell.exe'\" | \
         Where-Object { $_.CommandLine -like '*monitor.ps1*' } | \
         ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }",
        Duration::from_secs(20),
    );
}

pub fn read_log() -> Option<String> {
    let raw = fs::read_to_string(log_file()).ok()?;
    let lines: Vec<&str> = raw.lines().filter(|line| !line.is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    let fatal: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("fatal") || lower.contains("error")
        })
        .collect();
    if !fatal.is_empty() {
        Some(fatal.join("\n"))
    } else {
        Some(lines[lines.len().saturating_sub(4)..].join("\n"))
    }
}

fn autostart_command() -> String {
    let script = script_file();
    let settings = settings_file();
    format!(
        "powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File {} -SettingsFile {}",
        path_quote(&script.to_string_lossy()),
        path_quote(&settings.to_string_lossy())
    )
}

pub fn set_autostart(enabled: bool) -> bool {
    if paths::is_portable() {
        return false;
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE) else {
        return false;
    };
    if enabled {
        let _ = ensure_script();
        key.set_value(RUN_NAME, &autostart_command()).is_ok()
    } else {
        let _ = key.delete_value(RUN_NAME);
        true
    }
}

pub fn get_autostart() -> bool {
    if paths::is_portable() {
        return false;
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ) else {
        return false;
    };
    key.get_value::<String, _>(RUN_NAME).is_ok()
}

/// The old build stored the widget script under `%LOCALAPPDATA%\DiskMaster`.
/// Data migration copies it to the new directory; the Run entry still points to
/// the old path, so it is rewritten to the new one.
pub fn migrate_autostart() {
    if paths::is_portable() {
        return;
    }
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ) else {
        return;
    };
    let Ok(value) = key.get_value::<String, _>(RUN_NAME) else {
        return;
    };
    let legacy = paths::local_appdata().join("DiskMaster").join("widget");
    if value.contains(&legacy.to_string_lossy().to_string()) {
        let _ = ensure_script();
        let _ = set_autostart(true);
        crate::logging::log("widget autostart migrated to new data directory");
    }
}

pub fn status(shared: &AppState) -> WidgetStatus {
    let running = is_running(shared);
    WidgetStatus {
        running,
        autostart: get_autostart(),
        settings: read_settings(),
        log: read_log(),
    }
}

fn _child_type(_: &Child) {}
