use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    duplicates, folders, history, metrics, programs, scan, schedule, startup, system, widget, winget,
};
use crate::state::AppState;
use crate::types::*;

type CmdResult<T> = Result<T, String>;

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

#[tauri::command]
pub fn open_releases() -> Result<(), String> {
    const URL: &str = "https://github.com/xxxronelxxx/diskmaster/releases/latest";
    std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", URL])
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
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
    blocking(move || scan::clean_categories(&input.categories)).await
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
    blocking(move || folders::delete(&shared, &input.paths, input.permanent)).await
}

/* -------------------------------- duplicates ------------------------------- */

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatesInput {
    pub root: Option<String>,
    pub min_size_kb: Option<f64>,
}

#[tauri::command]
pub async fn duplicates_scan(
    state: State<'_, AppState>,
    input: Option<DuplicatesInput>,
) -> CmdResult<DuplicateScan> {
    let shared = state.inner().clone();
    let input = input.unwrap_or_default();
    blocking(move || duplicates::scan_duplicates(&shared, input.root, input.min_size_kb)).await
}

#[tauri::command]
pub async fn duplicates_last(state: State<'_, AppState>) -> CmdResult<Option<DuplicateScan>> {
    Ok(duplicates::last_scan(state.inner()))
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
pub async fn startup_list() -> CmdResult<Vec<StartupItem>> {
    blocking(startup::list).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleInput {
    pub id: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn startup_toggle(input: ToggleInput) -> CmdResult<OkResult> {
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
pub async fn startup_delete(input: StartupIdInput) -> CmdResult<OkResult> {
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
pub async fn widget_info(state: State<'_, AppState>) -> CmdResult<WidgetStatus> {
    let shared = state.inner().clone();
    blocking(move || widget::status(&shared)).await
}

#[tauri::command]
pub async fn widget_start(
    state: State<'_, AppState>,
    input: Option<serde_json::Value>,
) -> CmdResult<OkResult> {
    let shared = state.inner().clone();
    blocking(move || {
        let (ok, error) = widget::launch(&shared, input);
        OkResult { ok, error }
    })
    .await
}

#[tauri::command]
pub async fn widget_stop(state: State<'_, AppState>) -> CmdResult<OkResult> {
    let shared = state.inner().clone();
    blocking(move || OkResult {
        ok: widget::stop(&shared),
        error: None,
    })
    .await
}

#[tauri::command]
pub async fn widget_save(
    state: State<'_, AppState>,
    input: serde_json::Value,
) -> CmdResult<WidgetSettings> {
    let _ = state;
    blocking(move || widget::save_settings(&input)).await
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
