use serde::{Deserialize, Serialize};

/* --------------------------------- system --------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub letter: String,
    pub label: String,
    pub file_system: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub machine: String,
    pub user: String,
    pub os: String,
    pub build: String,
    pub version: String,
    pub last_boot: Option<String>,
    pub cpu: String,
    pub cores: usize,
    pub threads: usize,
    pub ram_total: u64,
    pub ram_free: u64,
    pub admin: bool,
    pub drives: Vec<DriveInfo>,
}

/* --------------------------------- metrics -------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSample {
    pub t: i64,
    pub cpu: f64,
    pub cores: Vec<f64>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub disk_read: f64,
    pub disk_write: f64,
    pub net_in: f64,
    pub net_out: f64,
    pub net_adapter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsState {
    pub history: Vec<MetricsSample>,
    pub latest: Option<MetricsSample>,
    pub interval_ms: u64,
    pub error: Option<String>,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub memory: u64,
    pub cpu_percent: f64,
    pub path: Option<String>,
    pub suspended: bool,
}

/* ---------------------------------- scan ---------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCategoryResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub paths: Vec<String>,
    pub kind: String,
    pub risk: String,
    pub needs_admin: bool,
    pub note: Option<String>,
    pub bytes: u64,
    pub files: u64,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub path: String,
    pub name: String,
    pub dir: String,
    pub size: u64,
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanState {
    pub job_id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub canceled: bool,
    pub phase: String,
    pub percent: u32,
    pub scan_path: String,
    pub files_scanned: u64,
    pub bytes_scanned: u64,
    pub categories: Vec<ScanCategoryResult>,
    pub large_files: Vec<LargeFile>,
    pub large_files_truncated: bool,
    pub large_file_threshold: u64,
    pub total_junk_bytes: u64,
    pub total_junk_files: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanCategoryResult {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub freed_bytes: u64,
    pub deleted_files: u64,
    pub categories: Vec<CleanCategoryResult>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEntry {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCategory {
    pub id: String,
    pub name: String,
    pub total_bytes: u64,
    pub total_files: u64,
    pub entries: Vec<PreviewEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPreview {
    pub categories: Vec<PreviewCategory>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleResult {
    pub removed: u64,
    pub bytes: u64,
    pub errors: Vec<String>,
}

/* -------------------------------- folders --------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub files: u64,
    pub modified: i64,
    pub children: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderAnalysis {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub files: u64,
    pub entries: Vec<FolderEntry>,
    pub parent: Option<String>,
    pub elapsed_ms: u64,
    pub truncated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub ok: bool,
    pub bytes: u64,
    pub files: u64,
    pub errors: Vec<String>,
}

/* ------------------------------- duplicates ------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFile {
    pub path: String,
    pub name: String,
    pub dir: String,
    pub size: u64,
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub hash: String,
    pub size: u64,
    pub count: u64,
    pub wasted: u64,
    pub files: Vec<DuplicateFile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateProgress {
    pub phase: String,
    pub scanned: u64,
    pub hashed: u64,
    pub total: u64,
    pub percent: u32,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateScan {
    pub id: String,
    pub root: String,
    pub min_size: u64,
    pub scanned_files: u64,
    pub candidate_files: u64,
    pub groups: Vec<DuplicateGroup>,
    pub elapsed_ms: u64,
    pub truncated: bool,
    pub error: Option<String>,
}

/* -------------------------------- programs -------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledProgram {
    pub key: String,
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
    pub size_bytes: u64,
    pub location: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub no_remove: bool,
    pub display_icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub launched: bool,
    pub command: Option<String>,
    pub error: Option<String>,
}

/* --------------------------------- winget --------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WingetUpdate {
    pub name: String,
    pub id: String,
    pub current: String,
    pub available: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WingetListing {
    pub available: bool,
    pub updates: Vec<WingetUpdate>,
    pub error: Option<String>,
    pub raw: Option<String>,
    pub refreshed_at: i64,
}

/* -------------------------------- startup --------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub id: String,
    pub name: String,
    pub command: String,
    pub location: String,
    pub source: String,
    pub enabled: bool,
    pub publisher: Option<String>,
    pub executable: Option<String>,
    pub exists: bool,
    pub system: bool,
}

/* ------------------------------- schedule --------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleConfig {
    pub enabled: bool,
    pub frequency: String,
    pub time: String,
    pub day_of_week: u32,
    pub categories: Vec<String>,
    pub threshold_mb: u64,
    pub last_run_at: Option<i64>,
    pub last_freed_bytes: Option<u64>,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency: "weekly".into(),
            time: "12:00".into(),
            day_of_week: 1,
            categories: vec![
                "user-temp".into(),
                "browser-cache".into(),
                "thumbnails".into(),
                "inet-cache".into(),
                "dx-cache".into(),
            ],
            threshold_mb: 0,
            last_run_at: None,
            last_freed_bytes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRun {
    pub at: i64,
    pub freed_bytes: u64,
    pub files: u64,
    pub categories: Vec<String>,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStatus {
    pub config: ScheduleConfig,
    pub registered: bool,
    pub next_run: Option<String>,
}

/* -------------------------------- history --------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDrive {
    pub letter: String,
    pub used: u64,
    pub free: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveSnapshot {
    pub t: i64,
    pub drives: Vec<SnapshotDrive>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub points: Vec<DriveSnapshot>,
    pub letters: Vec<String>,
    pub growth_per_day: f64,
    pub first_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCaptureResult {
    pub snapshot: Option<DriveSnapshot>,
    pub points: Vec<DriveSnapshot>,
    pub letters: Vec<String>,
    pub growth_per_day: f64,
    pub first_at: Option<i64>,
}

/* --------------------------------- widget --------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetSettings {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub opacity: f64,
    pub accent: String,
    pub radius: u32,
    pub font_scale: f64,
    pub show_cpu: bool,
    pub show_ram: bool,
    pub show_disk: bool,
    pub show_net: bool,
    pub show_drives: bool,
    pub show_top_process: bool,
    pub show_temperatures: bool,
    pub bars: bool,
    pub z_mode: String,
    pub click_through: bool,
    pub interval_ms: u64,
    pub show_title: bool,
    pub title: String,
}

impl Default for WidgetSettings {
    fn default() -> Self {
        Self {
            x: -1,
            y: -1,
            width: 320,
            height: 200,
            opacity: 0.82,
            accent: "#38e0a0".into(),
            radius: 14,
            font_scale: 1.0,
            show_cpu: true,
            show_ram: true,
            show_disk: true,
            show_net: true,
            show_drives: true,
            show_top_process: false,
            show_temperatures: false,
            bars: true,
            z_mode: "normal".into(),
            click_through: false,
            interval_ms: 1200,
            show_title: true,
            title: "ДИСК-МАСТЕР".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetStatus {
    pub running: bool,
    pub autostart: bool,
    pub settings: WidgetSettings,
    pub log: Option<String>,
}
