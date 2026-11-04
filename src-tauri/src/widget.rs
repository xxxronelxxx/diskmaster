use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

use crate::paths;
use crate::types::{WidgetSettings, WidgetStatus};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_NAME: &str = "DiskMasterWidget";

fn data_dir() -> PathBuf {
    paths::storage_path(&["widget"])
}

fn settings_file() -> PathBuf {
    data_dir().join("widget-settings.json")
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
    value.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|byte| byte.is_ascii_hexdigit())
}

/// Values arrive from the UI as JSON, so they are clamped before they reach the
/// native window. A bad width or opacity would otherwise break the widget.
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

pub fn save_settings(patch: &Value) -> WidgetSettings {
    ensure_dir();
    let mut current =
        serde_json::to_value(read_settings()).unwrap_or_else(|_| serde_json::json!({}));
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

pub fn is_running(app: &AppHandle) -> bool {
    app.get_webview_window("widget")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

fn apply_native_style(window: &tauri::WebviewWindow, settings: &WidgetSettings) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_BOTTOM,
            HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_LAYERED,
            WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
        };

        let Ok(raw) = window.hwnd() else {
            return;
        };
        let hwnd = raw.0 as HWND;

        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let mut style = style as u32;
        style |= WS_EX_TOOLWINDOW;
        if settings.click_through {
            style |= WS_EX_LAYERED | WS_EX_TRANSPARENT;
        } else {
            style &= !WS_EX_TRANSPARENT;
        }
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        }

        let after = match settings.z_mode.as_str() {
            "top" => HWND_TOPMOST,
            "desktop" => HWND_BOTTOM,
            _ => HWND_NOTOPMOST,
        };
        unsafe {
            SetWindowPos(
                hwnd,
                after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

fn default_position(app: &AppHandle, settings: &WidgetSettings) -> PhysicalPosition<i32> {
    if settings.x >= 0 && settings.y >= 0 {
        return PhysicalPosition::new(settings.x, settings.y);
    }
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let area = monitor.size();
        let origin = monitor.position();
        return PhysicalPosition::new(
            origin.x + area.width as i32 - settings.width as i32 - 24,
            origin.y + 24,
        );
    }
    PhysicalPosition::new(24, 24)
}

pub fn apply(app: &AppHandle, settings: &WidgetSettings) {
    let Some(window) = app.get_webview_window("widget") else {
        return;
    };
    let _ = window.set_size(PhysicalSize::new(settings.width, settings.height));
    let position = if settings.x >= 0 && settings.y >= 0 {
        PhysicalPosition::new(settings.x, settings.y)
    } else {
        default_position(app, settings)
    };
    let _ = window.set_position(position);
    let _ = window.set_always_on_top(settings.z_mode == "top");
    apply_native_style(&window, settings);
}

pub fn launch(app: &AppHandle, patch: Option<Value>) -> (bool, Option<String>) {
    let settings = match patch {
        Some(patch) => save_settings(&patch),
        None => read_settings(),
    };

    if let Some(window) = app.get_webview_window("widget") {
        let _ = window.show();
        apply(app, &settings);
        return (true, None);
    }

    let builder = WebviewWindowBuilder::new(app, "widget", WebviewUrl::App("widget.html".into()))
        .title("Диск-Мастер — виджет")
        .inner_size(settings.width as f64, settings.height as f64)
        .decorations(false)
        .transparent(true)
        .resizable(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .always_on_top(settings.z_mode == "top");

    let window = match builder.build() {
        Ok(window) => window,
        Err(error) => return (false, Some(error.to_string())),
    };

    let _ = window.set_size(PhysicalSize::new(settings.width, settings.height));
    let _ = window.set_position(default_position(app, &settings));
    apply_native_style(&window, &settings);
    let _ = window.show();

    (true, None)
}

pub fn stop(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("widget") {
        let _ = window.hide();
    }
}

pub fn remember_geometry(position: PhysicalPosition<i32>, size: PhysicalSize<u32>) {
    let mut settings = read_settings();
    settings.x = position.x;
    settings.y = position.y;
    settings.width = size.width.clamp(200, 1200);
    settings.height = size.height.clamp(120, 900);
    ensure_dir();
    let _ = fs::write(
        settings_file(),
        serde_json::to_string_pretty(&settings).unwrap_or_default(),
    );
}

pub fn status(app: &AppHandle) -> WidgetStatus {
    WidgetStatus {
        running: is_running(app),
        autostart: get_autostart(),
        settings: read_settings(),
        log: None,
    }
}

/* -------------------------------------------------------------------------- */
/*                                 autostart                                  */
/* -------------------------------------------------------------------------- */

fn autostart_command() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    format!("\"{}\" --hidden --widget", exe.to_string_lossy())
}

pub fn set_autostart(enabled: bool) -> bool {
    if paths::is_portable() {
        return false;
    }
    let Ok(key) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)
    else {
        return false;
    };
    if enabled {
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
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .ok()
        .and_then(|key| key.get_value::<String, _>(RUN_NAME).ok())
        .map(|value| value.contains("--widget"))
        .unwrap_or(false)
}

/// The old build stored a PowerShell script and pointed the Run entry at it.
/// Rewrite that entry to the native widget as soon as the new version starts.
pub fn migrate_autostart() {
    if paths::is_portable() {
        return;
    }
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ)
    else {
        return;
    };
    let Ok(value) = key.get_value::<String, _>(RUN_NAME) else {
        return;
    };
    let expected = autostart_command();
    if value != expected {
        let _ = set_autostart(true);
        crate::logging::log("widget autostart migrated to the current executable");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_out_of_range_settings() {
        let settings = WidgetSettings {
            width: 99_999,
            height: 1,
            opacity: 5.0,
            radius: 500,
            font_scale: 0.1,
            interval_ms: 50,
            accent: "red".into(),
            z_mode: "hacked".into(),
            x: -50_000,
            ..WidgetSettings::default()
        };
        let clamped = clamp_settings(settings);
        assert_eq!(clamped.width, 1200);
        assert_eq!(clamped.height, 120);
        assert_eq!(clamped.opacity, 1.0);
        assert_eq!(clamped.radius, 40);
        assert_eq!(clamped.font_scale, 0.7);
        assert_eq!(clamped.interval_ms, 500);
        assert_eq!(clamped.accent, "#38e0a0");
        assert_eq!(clamped.z_mode, "normal");
        assert_eq!(clamped.x, -1);
    }

    #[test]
    fn accepts_valid_accent() {
        assert!(valid_accent("#38e0a0"));
        assert!(!valid_accent("#38e0a"));
        assert!(!valid_accent("38e0a0"));
    }
}
