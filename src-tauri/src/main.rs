#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clean_history;
mod commands;
mod duplicates;
mod elevated;
mod folders;
mod guard;
mod hash_cache;
mod history;
mod logging;
mod maintenance;
mod metrics;
mod paths;
mod programs;
mod scan;
mod schedule;
mod sensors;
mod settings;
mod startup;
mod state;
mod system;
mod types;
mod widget;
mod win_text;
mod winget;
mod wmi_time;

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

use crate::state::AppState;

static QUITTING: AtomicBool = AtomicBool::new(false);
static LAST_STATE_SAVE: AtomicI64 = AtomicI64::new(0);
static WIDGET_STATE_SAVE: AtomicI64 = AtomicI64::new(0);
const AUTOSTART_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const AUTOSTART_NAME: &str = "DiskMaster";

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

const AUTOSTART_TASK: &str = "DiskMasterAutostart";

fn is_elevated() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

fn quote_arg(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn schtasks_path() -> std::path::PathBuf {
    paths::system_root().join("System32").join("schtasks.exe")
}

fn task_exists() -> bool {
    Command::new(schtasks_path())
        .args(["/Query", "/TN", AUTOSTART_TASK])
        .creation_flags(0x0800_0000)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn task_command() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    format!("\"{}\" --hidden", exe.to_string_lossy())
}

fn run_schtasks(args: &[String]) -> Result<(), String> {
    let path = schtasks_path();
    if is_elevated() {
        let status = Command::new(&path)
            .args(args)
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|error| format!("schtasks: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("schtasks завершился с кодом {:?}", status.code()))
        }
    } else {
        let line = args
            .iter()
            .map(|arg| {
                if arg.contains(' ') || arg.contains('"') {
                    quote_arg(arg)
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let code = elevated::launch(&path, &line)?;
        if code == 0 {
            Ok(())
        } else {
            Err(format!("schtasks завершился с кодом {code}"))
        }
    }
}

fn create_task(always_admin: bool) -> Result<(), String> {
    let mut args = vec![
        "/Create".to_string(),
        "/TN".into(),
        AUTOSTART_TASK.into(),
        "/TR".into(),
        task_command(),
        "/SC".into(),
        "ONLOGON".into(),
        "/F".into(),
    ];
    if always_admin {
        args.push("/RL".into());
        args.push("HIGHEST".into());
    }
    run_schtasks(&args)
}

fn delete_task() -> Result<(), String> {
    if !task_exists() {
        return Ok(());
    }
    run_schtasks(&[
        "/Delete".to_string(),
        "/TN".into(),
        AUTOSTART_TASK.into(),
        "/F".into(),
    ])
}

fn write_run_key() {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(AUTOSTART_KEY, KEY_READ | KEY_WRITE)
    else {
        return;
    };
    let exe = std::env::current_exe().unwrap_or_default();
    let command = format!("\"{}\" --hidden", exe.to_string_lossy());
    let _ = key.set_value(AUTOSTART_NAME, &command);
}

fn remove_run_key() {
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(AUTOSTART_KEY, KEY_READ | KEY_WRITE)
    {
        let _ = key.delete_value(AUTOSTART_NAME);
    }
}

pub fn autostart_enabled() -> bool {
    let run_key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(AUTOSTART_KEY, KEY_READ)
        .ok()
        .and_then(|key| key.get_value::<String, _>(AUTOSTART_NAME).ok())
        .is_some();
    run_key || task_exists()
}

/// Applies the startup policy: a plain Run key for normal autostart, or a
/// highest-privilege scheduled task when the user wants the app to start as
/// administrator without a UAC prompt at logon.
pub fn apply_autostart(enabled: bool, always_admin: bool) -> Result<(), String> {
    if always_admin {
        remove_run_key();
        if enabled {
            create_task(true)
        } else {
            delete_task()
        }
    } else if enabled {
        delete_task()?;
        write_run_key();
        Ok(())
    } else {
        remove_run_key();
        delete_task()
    }
}

fn message_box(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "Диск-Мастер"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn format_gb(bytes: u64) -> String {
    format!("{:.1} ГБ", bytes as f64 / 1024f64.powi(3))
}

fn notify(app: &AppHandle, title: &str, body: &str, tab: &str) {
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .extra("tab", tab)
        .show();
}

fn toggle_widget(app: &AppHandle) {
    if widget::is_running(app) {
        widget::stop(app);
    } else {
        let _ = widget::launch(app, None);
    }
}

/// Re-registers the global hotkeys from the current settings. Returns the
/// shortcuts that could not be registered.
pub fn apply_hotkeys(app: &AppHandle) -> Vec<String> {
    let settings = settings::current();
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let mut errors = Vec::new();

    if !settings.hotkey_show.is_empty() {
        let accelerator = settings.hotkey_show.clone();
        if let Err(error) = shortcuts.on_shortcut(accelerator.as_str(), |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                logging::log("hotkey: show");
                show_main(app);
            }
        }) {
            errors.push(format!("{accelerator}: {error}"));
        }
    }

    if !settings.hotkey_widget.is_empty() {
        let accelerator = settings.hotkey_widget.clone();
        if let Err(error) = shortcuts.on_shortcut(accelerator.as_str(), |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                logging::log("hotkey: widget");
                toggle_widget(app);
            }
        }) {
            errors.push(format!("{accelerator}: {error}"));
        }
    }

    errors
}

/// Watches free space on fixed drives and raises a toast when a drive drops
/// below the configured threshold. Cooldown prevents repeated toasts.
fn start_disk_watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(180));
            let settings = settings::current();
            if !settings.notify_low_disk {
                continue;
            }
            let now = chrono::Utc::now().timestamp();
            let mut low = false;
            for drive in system::drives() {
                if drive.total == 0 {
                    continue;
                }
                let free_percent = drive.free as f64 / drive.total as f64 * 100.0;
                if free_percent >= settings.low_disk_percent as f64 {
                    continue;
                }
                low = true;
                if let Some(tray) = app.tray_by_id("main-tray") {
                    let _ = tray.set_tooltip(Some(format!(
                        "Мало места на {}: свободно {}",
                        drive.letter,
                        format_gb(drive.free)
                    )));
                }
                let cooldown = (settings.low_disk_cooldown_min.max(5) as i64) * 60;
                let previous = last.get(&drive.letter).copied().unwrap_or(0);
                if now - previous < cooldown {
                    continue;
                }
                last.insert(drive.letter.clone(), now);
                notify(
                    &app,
                    "Мало свободного места",
                    &format!(
                        "{}: свободно {} из {} ({}%). Откройте «Очистку», чтобы освободить место.",
                        drive.letter,
                        format_gb(drive.free),
                        format_gb(drive.total),
                        free_percent.round() as u32
                    ),
                    "cleanup",
                );
            }
            if !low {
                if let Some(tray) = app.tray_by_id("main-tray") {
                    let _ = tray.set_tooltip(Some("Диск-Мастер — очистка и мониторинг"));
                }
            }
        }
    });
}

/* -------------------------------------------------------------------------- */
/*                              window geometry                               */
/* -------------------------------------------------------------------------- */

#[derive(Serialize, Deserialize, Clone)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

fn window_state_file() -> std::path::PathBuf {
    paths::storage_path(&["window-state.json"])
}

fn load_window_state() -> Option<WindowState> {
    let raw = std::fs::read_to_string(window_state_file()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_window_state(
    position: tauri::PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
    maximized: bool,
    force: bool,
) {
    let now = chrono::Utc::now().timestamp_millis();
    if !force && now - LAST_STATE_SAVE.load(Ordering::Relaxed) < 800 {
        return;
    }
    LAST_STATE_SAVE.store(now, Ordering::Relaxed);

    let state = WindowState {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized,
    };
    let path = window_state_file();
    paths::ensure_parent(&path);
    if let Ok(raw) = serde_json::to_string(&state) {
        let _ = std::fs::write(path, raw);
    }
}

fn save_window_state(window: &tauri::Window, force: bool) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    write_window_state(
        position,
        size,
        window.is_maximized().unwrap_or(false),
        force,
    );
}

/* -------------------------------------------------------------------------- */
/*                                   tray                                     */
/* -------------------------------------------------------------------------- */

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let portable = paths::is_portable();
    let open = MenuItem::with_id(app, "open", "Открыть Диск-Мастер", true, None::<&str>)?;
    let widget = MenuItem::with_id(app, "widget", "Показать/скрыть виджет", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick-clean", "Быстрая очистка", true, None::<&str>)?;
    let snapshot = MenuItem::with_id(
        app,
        "snapshot",
        "Сделать снимок свободного места",
        true,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "Настройки", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        if portable {
            "Автозапуск недоступен в портативной версии"
        } else {
            "Запускать при входе в Windows"
        },
        !portable,
        !portable && autostart_enabled(),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[
            &open,
            &separator,
            &widget,
            &quick,
            &snapshot,
            &separator2,
            &settings,
            &autostart,
            &quit,
        ],
    )
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Диск-Мастер — очистка и мониторинг")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "widget" => toggle_widget(app),
            "quick-clean" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let ids: Vec<String> = [
                        "user-temp",
                        "thumbnails",
                        "inet-cache",
                        "dx-cache",
                        "crash-dumps",
                    ]
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect();
                    let result = scan::clean_categories(&ids);
                    crate::clean_history::record(
                        "clean",
                        "quick",
                        &ids,
                        result.freed_bytes,
                        result.deleted_files,
                        &result.errors,
                    );
                    if settings::current().quick_clean_notify {
                        notify(
                            &app,
                            "Быстрая очистка завершена",
                            &format!(
                                "Освобождено {} · {} файлов",
                                format_gb(result.freed_bytes),
                                result.deleted_files
                            ),
                            "cleanup",
                        );
                    }
                });
            }
            "settings" => {
                show_main(app);
                let _ = app.emit("open-settings", ());
            }
            "snapshot" => {
                let _ = history::capture();
                show_main(app);
            }
            "autostart" => {
                if paths::is_portable() {
                    return;
                }
                let next = !autostart_enabled();
                match apply_autostart(next, settings::current().always_admin) {
                    Ok(()) => logging::log(&format!("autostart -> {next}")),
                    Err(error) => logging::log(&format!("autostart failed: {error}")),
                }
                if let Some(tray) = app.tray_by_id("main-tray") {
                    if let Ok(menu) = build_menu(app) {
                        let _ = tray.set_menu(Some(menu));
                    }
                }
            }
            "quit" => {
                QUITTING.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/* -------------------------------------------------------------------------- */
/*                              headless clean                                */
/* -------------------------------------------------------------------------- */

fn arg_value(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    std::env::args().find_map(|value| value.strip_prefix(&prefix).map(str::to_string))
}

fn run_headless_clean() {
    let categories: Vec<String> = arg_value("categories")
        .unwrap_or_default()
        .split(',')
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    let threshold = arg_value("threshold").and_then(|value| value.parse::<u64>().ok());

    let _ = schedule::scheduled_clean(&categories, threshold);
}

/* -------------------------------------------------------------------------- */
/*                                   main                                     */
/* -------------------------------------------------------------------------- */

fn main() {
    if !paths::is_portable() {
        paths::migrate_legacy_data();
    }
    logging::log(&format!(
        "start portable={} argv={:?}",
        paths::is_portable(),
        std::env::args().collect::<Vec<_>>()
    ));

    if std::env::args().any(|value| value == "--run-scheduled-clean") {
        run_headless_clean();
        return;
    }

    let loaded_settings = settings::init();
    if loaded_settings.always_admin
        && !is_elevated()
        && !std::env::args().any(|value| value == "--elevated")
    {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut parameters: Vec<String> = std::env::args().skip(1).collect();
        parameters.push("--elevated".into());
        let line = parameters
            .iter()
            .map(|argument| quote_arg(argument))
            .collect::<Vec<_>>()
            .join(" ");
        match elevated::launch_detached(&exe, &line) {
            Ok(()) => return,
            Err(error) => {
                message_box(&format!(
                    "Для запуска с правами администратора нужно подтвердить запрос UAC.

{error}"
                ));
                return;
            }
        }
    }

    let initial_tab = arg_value("tab").or_else(|| {
        std::env::args()
            .any(|value| value == "--settings")
            .then(|| "settings".to_string())
    });
    if let Some(tab) = initial_tab {
        *commands::INITIAL_TAB.lock().unwrap() = Some(tab);
    }

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let wants_widget = argv.iter().any(|value| value == "--widget");
            let hidden = argv.iter().any(|value| value == "--hidden");

            if wants_widget {
                let _ = widget::launch(app, None);
            } else if !hidden {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::portable_mode,
            commands::app_info,
            commands::open_releases,
            commands::open_url,
            commands::open_logs,
            commands::diagnostics,
            commands::copy_diagnostics,
            commands::overview,
            commands::metrics,
            commands::kill_process,
            commands::start_scan,
            commands::scan_status,
            commands::clean_junk,
            commands::clean_dry_run,
            commands::preview_clean,
            commands::clean_history,
            commands::write_text_file,
            commands::settings_state,
            commands::settings_save,
            commands::settings_export,
            commands::settings_import,
            commands::autostart_set,
            commands::notification_test,
            commands::process_priority,
            commands::process_suspend,
            commands::sensors_state,
            commands::sensors_start,
            commands::sensors_stop,
            commands::sensors_install,
            commands::initial_tab,
            commands::maintenance_status,
            commands::system_operation,
            commands::delete_large_files,
            commands::reveal,
            commands::folder_roots,
            commands::folder_analyze,
            commands::folder_delete,
            commands::duplicates_scan,
            commands::duplicates_last,
            commands::duplicates_status,
            commands::duplicates_cancel,
            commands::programs,
            commands::uninstall,
            commands::updates,
            commands::upgrade,
            commands::upgrade_all,
            commands::startup_list,
            commands::startup_toggle,
            commands::startup_delete,
            commands::schedule_get,
            commands::schedule_save,
            commands::schedule_history,
            commands::schedule_run_now,
            commands::history_get,
            commands::history_capture,
            commands::widget_info,
            commands::widget_start,
            commands::widget_stop,
            commands::widget_save,
            commands::widget_autostart,
            commands::widget_data,
            commands::widget_hide,
            commands::show_main,
            commands::log_event,
        ])
        .setup(|app| {
            let hidden = std::env::args().any(|value| value == "--hidden");

            if let Some(window) = app.get_webview_window("main") {
                let monitor = window.current_monitor().ok().flatten();
                if let Some(state) = load_window_state() {
                    let (mut width, mut height, mut x, mut y) =
                        (state.width, state.height, state.x, state.y);

                    if let Some(monitor) = monitor.as_ref() {
                        let area = monitor.size();
                        let origin = monitor.position();
                        let max_width = area.width.saturating_sub(40);
                        let max_height = area.height.saturating_sub(120);
                        width = width.min(max_width).max(960);
                        height = height.min(max_height).max(620);

                        let max_x = origin.x + area.width as i32 - width as i32;
                        let max_y = origin.y + area.height as i32 - height as i32;
                        if x < origin.x || x > max_x {
                            x = origin.x + 40;
                        }
                        if y < origin.y || y > max_y {
                            y = origin.y + 40;
                        }
                    }

                    let _ = window.set_size(tauri::PhysicalSize::new(width, height));
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                    if state.maximized {
                        let _ = window.maximize();
                    }
                } else if let Some(monitor) = monitor.as_ref() {
                    // No saved geometry: make sure the default window fits the
                    // current monitor even at 125–150% DPI, then centre it.
                    let area = monitor.size();
                    let origin = monitor.position();
                    if let Ok(current) = window.inner_size() {
                        let width = current.width.min(area.width.saturating_sub(40));
                        let height = current.height.min(area.height.saturating_sub(120));
                        let _ = window.set_size(tauri::PhysicalSize::new(width, height));
                        let x = origin.x + ((area.width as i32 - width as i32) / 2).max(0);
                        let y = origin.y + ((area.height as i32 - height as i32) / 2).max(0);
                        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                    }
                }
                if !hidden {
                    let _ = window.show();
                }
            }

            build_tray(app.handle())?;
            widget::migrate_autostart();

            settings::init();

            // Clicks on our toasts carry the target tab in the notification
            // payload and open the matching section in the running window.
            let action_app = app.handle().clone();
            let _ = app.notification().on_action(move |action| {
                if action.action_id() != "tap" {
                    return;
                }
                let tab = action
                    .notification()
                    .and_then(|notification| notification.extra().get("tab"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("overview")
                    .to_string();
                show_main(&action_app);
                let _ = action_app.emit("navigate", tab);
            });

            let hotkey_errors = apply_hotkeys(app.handle());
            if hotkey_errors.is_empty() {
                let current = settings::current();
                logging::log(&format!(
                    "hotkeys registered: {} / {}",
                    current.hotkey_show, current.hotkey_widget
                ));
            } else {
                logging::log(&format!("hotkeys failed: {}", hotkey_errors.join("; ")));
            }
            start_disk_watch(app.handle().clone());

            // Sensors are started automatically on a normal launch. A hidden
            // autostart run stays silent unless the app already runs elevated
            // (the always-administrator task), where no UAC is involved.
            if !hidden || is_elevated() {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let state = sensors::state();
                    if state.available && state.pawnio_installed && !state.running {
                        match sensors::start() {
                            Ok(_) => logging::log("sensors: auto-started"),
                            Err(error) => {
                                logging::log(&format!("sensors: auto-start failed: {error}"))
                            }
                        }
                    }
                });
            }

            if std::env::args().any(|value| value == "--widget") {
                let _ = widget::launch(app.handle(), None);
            }
            history::start_sampler();
            logging::log("window setup complete");

            // Warm up the metrics sampler after the first paint so the
            // monitoring tab has data immediately without slowing startup.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let _ = metrics::global();
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "widget" {
                match event {
                    WindowEvent::CloseRequested { .. } => {
                        if let (Ok(position), Ok(size)) =
                            (window.outer_position(), window.inner_size())
                        {
                            widget::remember_geometry(position, size);
                        }
                        // Let the window close and destroy its WebView; the
                        // widget is recreated on the next launch.
                    }
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                        let now = chrono::Utc::now().timestamp_millis();
                        if now - WIDGET_STATE_SAVE.load(Ordering::Relaxed) > 800 {
                            WIDGET_STATE_SAVE.store(now, Ordering::Relaxed);
                            if let (Ok(position), Ok(size)) =
                                (window.outer_position(), window.inner_size())
                            {
                                widget::remember_geometry(position, size);
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }

            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if QUITTING.load(Ordering::SeqCst) {
                        return;
                    }
                    if settings::current().minimize_to_tray {
                        api.prevent_close();
                        save_window_state(window, true);
                        let _ = window.hide();
                    } else {
                        QUITTING.store(true, Ordering::SeqCst);
                        window.app_handle().exit(0);
                    }
                }
                WindowEvent::Moved(_) => {
                    save_window_state(window, false);
                }
                WindowEvent::Resized(_) => {
                    save_window_state(window, false);
                    if window.is_minimized().unwrap_or(false) {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        });

    let app = match builder.build(tauri::generate_context!()) {
        Ok(app) => app,
        Err(error) => {
            // A broken WebView2 runtime is the most common cause here; make it
            // visible instead of showing nothing at all.
            eprintln!("Диск-Мастер: не удалось запустить приложение: {error}");
            return;
        }
    };

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            if let Some(window) = app.get_webview_window("main") {
                if let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) {
                    write_window_state(
                        position,
                        size,
                        window.is_maximized().unwrap_or(false),
                        true,
                    );
                }
            }
            metrics::stop_global();
            if sensors::state().running {
                sensors::stop();
            }
            logging::log("exit");
        }
    });
}
