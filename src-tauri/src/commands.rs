use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::clean_history::CleanHistoryEntry;
use crate::maintenance::{self, MaintenanceStatus};
use crate::settings::AppSettings;
use crate::state::AppState;
use crate::types::*;
use crate::{
    duplicates, folders, history, metrics, programs, scan, schedule, startup, system, widget,
    winget,
};

type CmdResult<T> = Result<T, String>;

/// Tab requested with `--tab=<id>` or `--settings` on the command line.
pub static INITIAL_TAB: Mutex<Option<String>> = Mutex::new(None);

#[tauri::command]
pub fn initial_tab() -> Option<String> {
    INITIAL_TAB.lock().unwrap().clone()
}

async fn blocking<T, F>(work: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())
}

/* --------------------------------- system --------------------------------- */

#[tauri::command]
pub fn portable_mode() -> bool {
    crate::paths::is_portable()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub portable: bool,
}

#[tauri::command]
pub fn app_info(app: tauri::AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        portable: crate::paths::is_portable(),
    }
}

const REPO_URL: &str = "https://github.com/xxxronelxxx/diskmaster";

fn open_url_internal(url: &str) -> Result<(), String> {
    std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_releases() -> Result<(), String> {
    open_url_internal(&format!("{REPO_URL}/releases/latest"))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with(REPO_URL) {
        return Err("Недопустимый адрес".into());
    }
    open_url_internal(&url)
}

#[tauri::command]
pub fn open_logs() -> Result<(), String> {
    let dir = crate::paths::storage_path(&["logs"]);
    let _ = std::fs::create_dir_all(&dir);
    std::process::Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn diagnostics_text(app: &tauri::AppHandle) -> String {
    let info = system::collect();
    let log = std::fs::read_to_string(crate::paths::storage_path(&["logs", "app.log"]))
        .unwrap_or_default();
    let tail: Vec<&str> = log
        .lines()
        .rev()
        .take(50)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    format!(
        "Диск-Мастер {}
portable={}
data={}
OS={} {} ({})
CPU={} / {} ядер, {} потоков
RAM={} / {}
admin={}

Журнал:
{}",
        app.package_info().version,
        crate::paths::is_portable(),
        crate::paths::storage_root().display(),
        info.os,
        info.version,
        info.build,
        info.cpu,
        info.cores,
        info.threads,
        info.ram_total,
        info.ram_free,
        info.admin,
        tail.join(
            "
"
        )
    )
}

#[tauri::command]
pub fn diagnostics(app: tauri::AppHandle) -> String {
    diagnostics_text(&app)
}

#[tauri::command]
pub fn copy_diagnostics(app: tauri::AppHandle) -> Result<(), String> {
    let text = diagnostics_text(&app);
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard.set_text(text).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn overview() -> CmdResult<SystemInfo> {
    blocking(system::collect).await
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetricsInput {
    #[serde(default)]
    pub processes: bool,
}

#[tauri::command]
pub async fn metrics(input: Option<MetricsInput>) -> CmdResult<MetricsState> {
    let with_processes = input.map(|value| value.processes).unwrap_or(false);
    blocking(move || {
        let processes = if with_processes {
            metrics::list_processes(12)
        } else {
            Vec::new()
        };
        metrics::state(processes)
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillInput {
    pub pid: u32,
}

#[derive(Serialize)]
pub struct OkResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[tauri::command]
pub async fn kill_process(input: KillInput) -> CmdResult<OkResult> {
    blocking(move || {
        let (ok, error) = metrics::kill_process(input.pid);
        OkResult { ok, error }
    })
    .await
}

/* ---------------------------------- scan ---------------------------------- */

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartScanInput {
    pub threshold_mb: Option<f64>,
}

#[tauri::command]
pub async fn start_scan(
    state: State<'_, AppState>,
    input: Option<StartScanInput>,
) -> CmdResult<ScanState> {
    let shared = state.inner().clone();
    let threshold = input.and_then(|value| value.threshold_mb);
    blocking(move || scan::start_scan(&shared, threshold)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatusInput {
    pub job_id: String,
}

#[tauri::command]
pub async fn scan_status(
    state: State<'_, AppState>,
    input: ScanStatusInput,
) -> CmdResult<Option<ScanState>> {
    let guard = state.scan.lock().unwrap();
    Ok(match guard.as_ref() {
        Some(job) if input.job_id.is_empty() || job.job_id == input.job_id => Some(job.clone()),
        Some(job) => Some(job.clone()),
        None => None,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanInput {
    pub categories: Vec<String>,
}

#[tauri::command]
pub async fn clean_junk(input: CleanInput) -> CmdResult<CleanResult> {
    blocking(move || {
        let result = scan::clean_categories(&input.categories);
        crate::clean_history::record(
            "clean",
            "manual",
            &input.categories,
            result.freed_bytes,
            result.deleted_files,
            &result.errors,
        );
        result
    })
    .await
}

#[tauri::command]
pub async fn preview_clean(input: CleanInput) -> CmdResult<CleanPreview> {
    blocking(move || CleanPreview {
        categories: scan::preview_categories(&input.categories, 200),
    })
    .await
}

#[tauri::command]
pub async fn clean_dry_run(input: CleanInput) -> CmdResult<CleanResult> {
    blocking(move || scan::dry_run_categories(&input.categories)).await
}

/* -------------------------------- settings -------------------------------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsState {
    pub settings: AppSettings,
    pub autostart: bool,
    pub portable: bool,
    pub hotkey_errors: Vec<String>,
}

fn settings_state_value(saved: AppSettings, app: &tauri::AppHandle) -> SettingsState {
    SettingsState {
        settings: saved,
        autostart: crate::autostart_enabled(),
        portable: crate::paths::is_portable(),
        hotkey_errors: crate::apply_hotkeys(app),
    }
}

#[tauri::command]
pub fn settings_state() -> SettingsState {
    SettingsState {
        settings: crate::settings::current(),
        autostart: crate::autostart_enabled(),
        portable: crate::paths::is_portable(),
        hotkey_errors: Vec::new(),
    }
}

#[tauri::command]
pub fn settings_save(input: AppSettings, app: tauri::AppHandle) -> Result<SettingsState, String> {
    let saved = crate::settings::save(&input)?;
    Ok(settings_state_value(saved, &app))
}

fn json_target(raw: &str) -> Result<std::path::PathBuf, String> {
    let target = std::path::PathBuf::from(raw);
    if !target.is_absolute() {
        return Err("Путь должен быть абсолютным".into());
    }
    let is_json = target
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if !is_json {
        return Err("Выберите файл с расширением .json".into());
    }
    Ok(target)
}

#[tauri::command]
pub fn settings_export(path: String) -> Result<(), String> {
    let target = json_target(&path)?;
    let json = crate::settings::export_json()?;
    crate::paths::ensure_parent(&target);
    std::fs::write(&target, json).map_err(|error| format!("Не удалось записать файл: {error}"))
}

#[tauri::command]
pub fn settings_import(path: String, app: tauri::AppHandle) -> Result<SettingsState, String> {
    let target = json_target(&path)?;
    let raw = std::fs::read_to_string(&target)
        .map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    let saved = crate::settings::import_json(&raw)?;
    Ok(settings_state_value(saved, &app))
}

#[derive(Deserialize)]
pub struct AutostartInput {
    pub enabled: bool,
}

#[tauri::command]
pub fn autostart_set(input: AutostartInput) -> OkResult {
    if crate::paths::is_portable() {
        return OkResult {
            ok: false,
            error: Some("Автозапуск недоступен в портативной версии".into()),
        };
    }
    crate::set_autostart(input.enabled);
    OkResult {
        ok: true,
        error: None,
    }
}

#[tauri::command]
pub fn notification_test(app: tauri::AppHandle) -> OkResult {
    use tauri_plugin_notification::NotificationExt;
    match app
        .notification()
        .builder()
        .title("Диск-Мастер")
        .body("Уведомления работают.")
        .show()
    {
        Ok(()) => OkResult {
            ok: true,
            error: None,
        },
        Err(error) => OkResult {
            ok: false,
            error: Some(error.to_string()),
        },
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorityInput {
    pub pid: u32,
    pub level: String,
}

#[tauri::command]
pub async fn process_priority(input: PriorityInput) -> CmdResult<OkResult> {
    blocking(move || {
        let (ok, error) = metrics::set_priority(input.pid, &input.level);
        OkResult { ok, error }
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuspendInput {
    pub pid: u32,
    pub suspend: bool,
}

#[tauri::command]
pub async fn process_suspend(input: SuspendInput) -> CmdResult<OkResult> {
    blocking(move || {
        let (ok, error) = metrics::set_suspended(input.pid, input.suspend);
        OkResult { ok, error }
    })
    .await
}

#[tauri::command]
pub fn clean_history() -> Vec<CleanHistoryEntry> {
    crate::clean_history::read(100)
}

#[tauri::command]
pub async fn maintenance_status() -> CmdResult<MaintenanceStatus> {
    blocking(maintenance::status).await
}

#[derive(Deserialize)]
pub struct SystemOpInput {
    pub op: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[tauri::command]
pub async fn system_operation(input: SystemOpInput) -> CmdResult<serde_json::Value> {
    blocking(move || maintenance::run(&input.op, input.payload)).await?
}

#[derive(Deserialize)]
pub struct WriteTextInput {
    pub path: String,
    pub content: String,
}

#[tauri::command]
pub async fn write_text_file(input: WriteTextInput) -> CmdResult<bool> {
    blocking(move || {
        let path = std::path::PathBuf::from(&input.path);
        let allowed = path
            .extension()
            .map(|value| {
                matches!(
                    value.to_string_lossy().to_lowercase().as_str(),
                    "csv" | "html" | "json" | "txt"
                )
            })
            .unwrap_or(false);
        if !allowed {
            return Err("Недопустимый тип файла для экспорта".to_string());
        }
        if input.content.len() > 20 * 1024 * 1024 {
            return Err("Файл слишком большой для экспорта".to_string());
        }
        crate::paths::ensure_parent(&path);
        std::fs::write(&path, input.content).map_err(|error| error.to_string())?;
        Ok::<bool, String>(true)
    })
    .await?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFilesInput {
    pub paths: Vec<String>,
}

#[tauri::command]
pub async fn delete_large_files(
    state: State<'_, AppState>,
    input: DeleteFilesInput,
) -> CmdResult<RecycleResult> {
    let shared = state.inner().clone();
    blocking(move || scan::delete_large_files(&shared, &input.paths)).await
}

#[derive(Deserialize)]
pub struct PathInput {
    pub path: String,
}

#[tauri::command]
pub async fn reveal(input: PathInput) -> CmdResult<OkResult> {
    blocking(move || {
        let (ok, error) = scan::reveal_path(&input.path);
        OkResult { ok, error }
    })
    .await
}

/* --------------------------------- folders -------------------------------- */

#[tauri::command]
pub async fn folder_roots() -> CmdResult<Vec<serde_json::Value>> {
    blocking(folders::roots).await
}

#[derive(Deserialize, Default)]
pub struct AnalyzeInput {
    pub path: Option<String>,
}

#[tauri::command]
pub async fn folder_analyze(
    state: State<'_, AppState>,
    input: Option<AnalyzeInput>,
) -> CmdResult<FolderAnalysis> {
    let shared = state.inner().clone();
    let path = input.and_then(|value| value.path);
    blocking(move || folders::analyze(&shared, path)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePathsInput {
    pub paths: Vec<String>,
    #[serde(default)]
    pub permanent: bool,
}

#[tauri::command]
pub async fn folder_delete(
    state: State<'_, AppState>,
    input: DeletePathsInput,
) -> CmdResult<DeleteResult> {
    let shared = state.inner().clone();
    blocking(move || {
        let result = folders::delete(&shared, &input.paths, input.permanent);
        crate::clean_history::record(
            if input.permanent {
                "delete-permanent"
            } else {
                "delete-to-recycle"
            },
            "manual",
            &[],
            result.bytes,
            result.files,
            &result.errors,
        );
        result
    })
    .await
}

/* -------------------------------- duplicates ------------------------------- */

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatesInput {
    pub root: Option<String>,
    pub min_size_kb: Option<f64>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[tauri::command]
pub async fn duplicates_scan(
    state: State<'_, AppState>,
    input: Option<DuplicatesInput>,
) -> CmdResult<DuplicateScan> {
    let shared = state.inner().clone();
    let input = input.unwrap_or_default();
    blocking(move || {
        duplicates::scan_duplicates(&shared, input.root, input.min_size_kb, input.exclude)
    })
    .await
}

#[tauri::command]
pub async fn duplicates_last(state: State<'_, AppState>) -> CmdResult<Option<DuplicateScan>> {
    Ok(duplicates::last_scan(state.inner()))
}

#[tauri::command]
pub fn duplicates_status(state: State<'_, AppState>) -> Option<DuplicateProgress> {
    duplicates::progress(state.inner())
}

#[tauri::command]
pub fn duplicates_cancel(state: State<'_, AppState>) {
    duplicates::cancel(state.inner());
}

/* -------------------------------- programs -------------------------------- */

#[derive(Deserialize, Default)]
pub struct RefreshInput {
    #[serde(default)]
    pub refresh: bool,
}

#[tauri::command]
pub async fn programs(
    state: State<'_, AppState>,
    input: Option<RefreshInput>,
) -> CmdResult<Vec<InstalledProgram>> {
    let shared = state.inner().clone();
    let force = input.map(|value| value.refresh).unwrap_or(false);
    blocking(move || programs::list(&shared, force)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallInput {
    pub key: String,
    #[serde(default)]
    pub quiet: bool,
}

#[tauri::command]
pub async fn uninstall(
    state: State<'_, AppState>,
    input: UninstallInput,
) -> CmdResult<UninstallResult> {
    let shared = state.inner().clone();
    blocking(move || programs::uninstall(&shared, &input.key, input.quiet)).await
}

/* --------------------------------- winget --------------------------------- */

#[tauri::command]
pub async fn updates(
    state: State<'_, AppState>,
    input: Option<RefreshInput>,
) -> CmdResult<WingetListing> {
    let shared = state.inner().clone();
    let force = input.map(|value| value.refresh).unwrap_or(false);
    blocking(move || winget::list(&shared, force)).await
}

#[derive(Deserialize)]
pub struct IdInput {
    pub id: String,
}

#[derive(Serialize)]
pub struct OutputResult {
    pub ok: bool,
    pub output: String,
}

#[tauri::command]
pub async fn upgrade(state: State<'_, AppState>, input: IdInput) -> CmdResult<OutputResult> {
    let shared = state.inner().clone();
    blocking(move || {
        let (ok, output) = winget::upgrade(&shared, &input.id);
        OutputResult { ok, output }
    })
    .await
}

#[tauri::command]
pub async fn upgrade_all(state: State<'_, AppState>) -> CmdResult<OutputResult> {
    let shared = state.inner().clone();
    blocking(move || {
        let (ok, output) = winget::upgrade_all(&shared);
        OutputResult { ok, output }
    })
    .await
}

/* --------------------------------- startup -------------------------------- */

#[tauri::command]
pub async fn startup_list(state: State<'_, AppState>) -> CmdResult<Vec<StartupItem>> {
    let shared = state.inner().clone();
    blocking(move || {
        let items = startup::list();
        let mut ids = shared.startup_ids.lock().unwrap();
        ids.clear();
        ids.extend(items.iter().map(|item| item.id.clone()));
        items
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleInput {
    pub id: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn startup_toggle(state: State<'_, AppState>, input: ToggleInput) -> CmdResult<OkResult> {
    if !state.startup_ids.lock().unwrap().contains(&input.id) {
        return Ok(OkResult {
            ok: false,
            error: Some("Элемент не найден в текущем списке автозагрузки".into()),
        });
    }
    blocking(move || {
        let (ok, error) = startup::toggle(&input.id, input.enabled);
        OkResult { ok, error }
    })
    .await
}

#[derive(Deserialize)]
pub struct StartupIdInput {
    pub id: String,
}

#[tauri::command]
pub async fn startup_delete(
    state: State<'_, AppState>,
    input: StartupIdInput,
) -> CmdResult<OkResult> {
    if !state.startup_ids.lock().unwrap().contains(&input.id) {
        return Ok(OkResult {
            ok: false,
            error: Some("Элемент не найден в текущем списке автозагрузки".into()),
        });
    }
    blocking(move || {
        let (ok, error) = startup::delete(&input.id);
        OkResult { ok, error }
    })
    .await
}

/* --------------------------------- schedule ------------------------------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSaveResult {
    pub config: ScheduleConfig,
    pub registered: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn schedule_get() -> CmdResult<ScheduleStatus> {
    blocking(schedule::status).await
}

#[tauri::command]
pub async fn schedule_save(input: ScheduleConfig) -> CmdResult<ScheduleSaveResult> {
    blocking(move || {
        let (config, registered, error) = schedule::save(&input);
        ScheduleSaveResult {
            config,
            registered,
            error,
        }
    })
    .await
}

#[tauri::command]
pub async fn schedule_history() -> CmdResult<Vec<ScheduleRun>> {
    blocking(|| schedule::read_history(30)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunNowResult {
    pub freed_bytes: u64,
    pub files: u64,
    pub categories: Vec<String>,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn schedule_run_now() -> CmdResult<RunNowResult> {
    blocking(|| {
        let result = schedule::run_now();
        RunNowResult {
            freed_bytes: result.freed_bytes,
            files: result.deleted_files,
            categories: result
                .categories
                .iter()
                .map(|category| category.id.clone())
                .collect(),
            errors: result.errors,
        }
    })
    .await
}

/* --------------------------------- history -------------------------------- */

#[tauri::command]
pub async fn history_get() -> CmdResult<HistorySummary> {
    blocking(history::summary).await
}

#[tauri::command]
pub async fn history_capture() -> CmdResult<HistoryCaptureResult> {
    blocking(history::capture_with_summary).await
}

/* --------------------------------- widget --------------------------------- */

#[tauri::command]
pub async fn widget_info(app: tauri::AppHandle) -> CmdResult<WidgetStatus> {
    Ok(widget::status(&app))
}

#[tauri::command]
pub async fn widget_start(
    app: tauri::AppHandle,
    input: Option<serde_json::Value>,
) -> CmdResult<OkResult> {
    let (ok, error) = widget::launch(&app, input);
    Ok(OkResult { ok, error })
}

#[tauri::command]
pub async fn widget_stop(app: tauri::AppHandle) -> CmdResult<OkResult> {
    widget::stop(&app);
    Ok(OkResult {
        ok: true,
        error: None,
    })
}

#[tauri::command]
pub async fn widget_hide(app: tauri::AppHandle) -> CmdResult<OkResult> {
    widget::stop(&app);
    Ok(OkResult {
        ok: true,
        error: None,
    })
}

#[tauri::command]
pub fn show_main(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub async fn widget_save(
    app: tauri::AppHandle,
    input: serde_json::Value,
) -> CmdResult<WidgetSettings> {
    let settings = widget::save_settings(&input);
    let geometry = ["x", "y", "width", "height"]
        .iter()
        .any(|key| input.get(*key).is_some());
    widget::apply(&app, &settings, geometry);
    Ok(settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetData {
    settings: WidgetSettings,
    latest: Option<MetricsSample>,
    drives: Vec<DriveInfo>,
    top: Option<ProcessInfo>,
}

#[tauri::command]
pub async fn widget_data() -> CmdResult<WidgetData> {
    blocking(|| {
        let settings = widget::read_settings();
        let (_, latest, _) = metrics::global().snapshot();
        let drives = system::drives_cached();
        let top = if settings.show_top_process {
            metrics::list_processes(1).into_iter().next()
        } else {
            None
        };
        WidgetData {
            settings,
            latest,
            drives,
            top,
        }
    })
    .await
}

#[derive(Deserialize)]
pub struct WidgetAutostartInput {
    pub enabled: bool,
}

#[tauri::command]
pub async fn widget_autostart(input: WidgetAutostartInput) -> CmdResult<OkResult> {
    if crate::paths::is_portable() {
        return Ok(OkResult {
            ok: false,
            error: Some("Автозапуск недоступен в портативной версии".into()),
        });
    }
    blocking(move || OkResult {
        ok: widget::set_autostart(input.enabled),
        error: None,
    })
    .await
}

/// Frontend diagnostics (update failures, unexpected errors) go to the same
/// append-only log as the backend messages.
#[tauri::command]
pub fn log_event(level: String, message: String) {
    let level = level.chars().take(16).collect::<String>();
    let message = message.chars().take(500).collect::<String>();
    crate::logging::log(&format!("FRONTEND {level}: {message}"));
}
