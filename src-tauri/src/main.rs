#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clean_history;
mod commands;
mod duplicates;
mod folders;
mod guard;
mod hash_cache;
mod history;
mod logging;
mod metrics;
mod paths;
mod programs;
mod scan;
mod schedule;
mod startup;
mod state;
mod system;
mod types;
mod widget;
mod win_text;
mod winget;

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
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

fn autostart_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(AUTOSTART_KEY, KEY_READ)
        .ok()
        .and_then(|key| key.get_value::<String, _>(AUTOSTART_NAME).ok())
        .is_some()
}

fn set_autostart(enabled: bool) {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(AUTOSTART_KEY, KEY_READ | KEY_WRITE)
    else {
        return;
    };
    if enabled {
        let exe = std::env::current_exe().unwrap_or_default();
        let command = format!("\"{}\" --hidden", exe.to_string_lossy());
        let _ = key.set_value(AUTOSTART_NAME, &command);
    } else {
        let _ = key.delete_value(AUTOSTART_NAME);
    }
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
    let snapshot = MenuItem::with_id(
        app,
        "snapshot",
        "Сделать снимок свободного места",
        true,
        None::<&str>,
    )?;
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
            &snapshot,
            &separator2,
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
            "widget" => {
                if widget::is_running(app) {
                    widget::stop(app);
                } else {
                    let _ = widget::launch(app, None);
                }
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
                set_autostart(next);
                logging::log(&format!("autostart -> {next}"));
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
            commands::preview_clean,
            commands::clean_history,
            commands::write_text_file,
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
                    api.prevent_close();
                    save_window_state(window, true);
                    let _ = window.hide();
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
            logging::log("exit");
        }
    });
}
