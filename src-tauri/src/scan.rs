use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::guard;
use crate::paths::{home_dir, local_appdata, system_root};
use crate::state::AppState;
use crate::types::{
    CleanCategoryResult, CleanResult, LargeFile, PreviewCategory, PreviewEntry, RecycleResult,
    ScanCategoryResult, ScanState,
};

const MB: u64 = 1024 * 1024;
const LARGE_FILE_LIMIT: usize = 150;
const LARGE_FILE_THRESHOLD_MIN: u64 = 50 * MB;

/* -------------------------------------------------------------------------- */
/*                              junk categories                               */
/* -------------------------------------------------------------------------- */

fn browser_cache_paths() -> Vec<PathBuf> {
    let local = local_appdata();
    let bases = [
        local.join("Google").join("Chrome").join("User Data"),
        local.join("Microsoft").join("Edge").join("User Data"),
        local
            .join("BraveSoftware")
            .join("Brave-Browser")
            .join("User Data"),
        local.join("Yandex").join("YandexBrowser").join("User Data"),
        local.join("Opera Software").join("Opera Stable"),
    ];
    let profiles = ["Default", "Profile 1", "Profile 2", "Profile 3"];
    let sub_dirs = ["Cache", "Code Cache", "GPUCache"];
    let mut result = Vec::new();
    for base in bases {
        for profile in profiles {
            for sub in sub_dirs {
                result.push(base.join(profile).join(sub));
            }
        }
    }
    result
}

struct CategoryDef {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    paths: Vec<PathBuf>,
    kind: &'static str,
    risk: &'static str,
    needs_admin: bool,
    note: Option<&'static str>,
}

fn categories() -> Vec<CategoryDef> {
    let local = local_appdata();
    let system = system_root();
    let home = home_dir();
    let roaming = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Roaming"));
    vec![
        CategoryDef {
            id: "user-temp",
            name: "Временные файлы",
            description: "Кэш установщиков, распакованные архивы и мусор приложений в %TEMP%",
            paths: vec![local.join("Temp")],
            kind: "contents",
            risk: "safe",
            needs_admin: false,
            note: Some("Закройте приложения перед очисткой — занятые файлы будут пропущены"),
        },
        CategoryDef {
            id: "browser-cache",
            name: "Кэш браузеров",
            description: "Chrome, Edge, Brave, Yandex и Opera: кэш страниц и шейдеров",
            paths: browser_cache_paths(),
            kind: "contents",
            risk: "safe",
            needs_admin: false,
            note: Some("Браузеры должны быть закрыты, иначе кэш не удалится"),
        },
        CategoryDef {
            id: "win-temp",
            name: "Temp Windows",
            description: "Системная папка временных файлов C:\\Windows\\Temp",
            paths: vec![system.join("Temp")],
            kind: "contents",
            risk: "safe",
            needs_admin: true,
            note: None,
        },
        CategoryDef {
            id: "win-update",
            name: "Кэш обновлений Windows",
            description: "Скачанные пакеты обновлений, которые уже установлены",
            paths: vec![system.join("SoftwareDistribution").join("Download")],
            kind: "contents",
            risk: "safe",
            needs_admin: true,
            note: None,
        },
        CategoryDef {
            id: "delivery-opt",
            name: "Оптимизация доставки",
            description: "Кэш файлов, розданных другим компьютерам через Windows Update",
            paths: vec![system
                .join("ServiceProfiles")
                .join("NetworkService")
                .join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("Windows")
                .join("DeliveryOptimization")],
            kind: "contents",
            risk: "safe",
            needs_admin: true,
            note: None,
        },
        CategoryDef {
            id: "prefetch",
            name: "Prefetch",
            description: "Кэш предзагрузки приложений — Windows создаст его заново",
            paths: vec![system.join("Prefetch")],
            kind: "contents",
            risk: "caution",
            needs_admin: true,
            note: None,
        },
        CategoryDef {
            id: "crash-dumps",
            name: "Дампы и отчёты о сбоях",
            description: "Файлы аварийных завершений программ и системы",
            paths: vec![system.join("Minidump"), local.join("CrashDumps")],
            kind: "contents",
            risk: "safe",
            needs_admin: false,
            note: None,
        },
        CategoryDef {
            id: "thumbnails",
            name: "Кэш эскизов и значков",
            description: "thumbcache и iconcache — проводник пересоздаст их автоматически",
            paths: vec![local.join("Microsoft").join("Windows").join("Explorer")],
            kind: "files",
            risk: "safe",
            needs_admin: false,
            note: None,
        },
        CategoryDef {
            id: "inet-cache",
            name: "Кэш интернет-компонентов",
            description: "INetCache — временные файлы веб-компонентов Windows",
            paths: vec![local.join("Microsoft").join("Windows").join("INetCache")],
            kind: "contents",
            risk: "safe",
            needs_admin: false,
            note: None,
        },
        CategoryDef {
            id: "dx-cache",
            name: "Кэш шейдеров DirectX",
            description: "D3DSCache — ускоряет игры, но безопасно очищается",
            paths: vec![local.join("D3DSCache")],
            kind: "contents",
            risk: "safe",
            needs_admin: false,
            note: None,
        },
        CategoryDef {
            id: "win-logs",
            name: "Логи Windows",
            description: "Журналы установки и диагностики C:\\Windows\\Logs",
            paths: vec![system.join("Logs")],
            kind: "contents",
            risk: "caution",
            needs_admin: true,
            note: None,
        },
        CategoryDef {
            id: "npm-cache",
            name: "Кэш npm",
            description: "Скачанные пакеты npm — пересоздадутся при следующей установке",
            paths: vec![local.join("npm-cache"), roaming.join("npm-cache")],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: Some("Первая установка после очистки будет дольше"),
        },
        CategoryDef {
            id: "pnpm-store",
            name: "Хранилище pnpm",
            description: "Глобальное хранилище пакетов pnpm",
            paths: vec![local.join("pnpm").join("store"), local.join("pnpm-cache")],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: Some("Закройте проекты и редакторы перед очисткой"),
        },
        CategoryDef {
            id: "yarn-cache",
            name: "Кэш yarn",
            description: "Кэш пакетов Yarn (Classic и Berry)",
            paths: vec![
                local.join("Yarn").join("Cache"),
                local.join("Yarn").join("Berry").join("cache"),
            ],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: Some("Первая установка после очистки будет дольше"),
        },
        CategoryDef {
            id: "pip-cache",
            name: "Кэш pip",
            description: "Скачанные Python-пакеты pip",
            paths: vec![local.join("pip").join("Cache")],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: None,
        },
        CategoryDef {
            id: "cargo-cache",
            name: "Кэш cargo",
            description: "Скачанные крейты и git-репозитории Cargo",
            paths: vec![
                home.join(".cargo").join("registry").join("cache"),
                home.join(".cargo").join("git").join("db"),
            ],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: Some("Проекты будут пересобираться дольше"),
        },
        CategoryDef {
            id: "gradle-cache",
            name: "Кэш Gradle",
            description: "Зависимости и артефакты сборки Gradle",
            paths: vec![home.join(".gradle").join("caches")],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: Some("Первая сборка после очистки будет значительно дольше"),
        },
        CategoryDef {
            id: "maven-cache",
            name: "Кэш Maven",
            description: "Локальный репозиторий Maven (~/.m2)",
            paths: vec![home.join(".m2").join("repository")],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: Some("Зависимости скачаются заново при следующей сборке"),
        },
        CategoryDef {
            id: "nuget-cache",
            name: "Кэш NuGet",
            description: "Глобальный кэш пакетов NuGet (~/.nuget)",
            paths: vec![home.join(".nuget").join("packages")],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: Some("Проекты .NET восстановят пакеты при сборке"),
        },
        CategoryDef {
            id: "go-cache",
            name: "Кэш Go",
            description: "Кэш сборки и скачанные модули Go",
            paths: vec![
                local.join("go-build"),
                home.join("go").join("pkg").join("mod").join("cache"),
            ],
            kind: "contents",
            risk: "caution",
            needs_admin: false,
            note: None,
        },
        CategoryDef {
            id: "recycle-bin",
            name: "Корзина",
            description: "Файлы, удалённые в корзину. Освобождаются безвозвратно",
            paths: Vec::new(),
            kind: "recycle-bin",
            risk: "safe",
            needs_admin: false,
            note: None,
        },
    ]
}

fn thumbnails_pattern(name: &str) -> bool {
    let lower = name.to_lowercase();
    (lower.starts_with("thumbcache") || lower.starts_with("iconcache"))
        && (lower.ends_with(".db") || lower.ends_with(".dat"))
}

fn format_category(def: &CategoryDef) -> ScanCategoryResult {
    ScanCategoryResult {
        id: def.id.into(),
        name: def.name.into(),
        description: def.description.into(),
        paths: def
            .paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        kind: def.kind.into(),
        risk: def.risk.into(),
        needs_admin: def.needs_admin,
        note: def.note.map(str::to_string),
        bytes: 0,
        files: 0,
        status: "pending".into(),
        error: None,
    }
}

/* -------------------------------------------------------------------------- */
/*                                  walker                                    */
/* -------------------------------------------------------------------------- */

struct WalkResult {
    files: u64,
    bytes: u64,
    timed_out: bool,
    denied: u64,
}

fn walk(
    root: &Path,
    max_depth: Option<usize>,
    budget: Duration,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    mut on_file: impl FnMut(&crate::walk::WalkEntry),
    mut on_progress: impl FnMut(u64, u64, &Path),
) -> WalkResult {
    let stats = crate::walk::walk(
        root,
        max_depth,
        budget,
        cancel,
        &mut on_file,
        &mut on_progress,
    );
    WalkResult {
        files: stats.files,
        bytes: stats.bytes,
        timed_out: stats.truncated,
        denied: stats.denied,
    }
}

/* -------------------------------------------------------------------------- */
/*                                   jobs                                     */
/* -------------------------------------------------------------------------- */

pub fn start_scan(shared: &AppState, threshold_mb: Option<f64>) -> ScanState {
    {
        let guard = shared.scan.lock().unwrap();
        if let Some(job) = guard.as_ref() {
            if job.finished_at.is_none() {
                return job.clone();
            }
        }
    }

    // A new scan invalidates paths from the previous one.
    shared.allow.lock().unwrap().clear();
    shared
        .scan_cancel
        .store(false, std::sync::atomic::Ordering::SeqCst);

    let threshold = ((threshold_mb.unwrap_or(200.0) as u64) * MB).max(LARGE_FILE_THRESHOLD_MIN);
    let definitions = categories();
    let mut categories_out = Vec::with_capacity(definitions.len());
    for def in &definitions {
        categories_out.push(format_category(def));
    }

    let job = ScanState {
        job_id: new_id(),
        started_at: chrono::Utc::now().timestamp_millis(),
        finished_at: None,
        canceled: false,
        phase: "starting".into(),
        percent: 0,
        scan_path: String::new(),
        files_scanned: 0,
        bytes_scanned: 0,
        categories: categories_out,
        large_files: Vec::new(),
        large_files_truncated: false,
        large_file_threshold: threshold,
        total_junk_bytes: 0,
        total_junk_files: 0,
        error: None,
    };

    {
        let mut guard = shared.scan.lock().unwrap();
        *guard = Some(job.clone());
    }

    crate::logging::log(&format!("scan start threshold={threshold}"));
    let shared_clone = shared.clone();
    std::thread::spawn(move || run_scan(shared_clone, threshold));
    job
}

fn mark_canceled(shared: &AppState) {
    let mut guard = shared.scan.lock().unwrap();
    if let Some(job) = guard.as_mut() {
        job.phase = "canceled".into();
        job.canceled = true;
        job.finished_at = Some(chrono::Utc::now().timestamp_millis());
        job.scan_path.clear();
        crate::logging::log("scan canceled");
    }
}

fn is_canceled(shared: &AppState) -> bool {
    shared
        .scan_cancel
        .load(std::sync::atomic::Ordering::Relaxed)
}

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

fn run_scan(shared: AppState, threshold: u64) {
    let scan_started = Instant::now();
    {
        if let Some(job) = shared.scan.lock().unwrap().as_mut() {
            job.phase = "junk".into();
        }
    }

    let definitions = categories();
    let mut files_done = 0u64;
    let mut bytes_done = 0u64;

    for (index, def) in definitions.iter().enumerate() {
        {
            let mut guard = shared.scan.lock().unwrap();
            let Some(job) = guard.as_mut() else { return };
            if let Some(category) = job.categories.get_mut(index) {
                category.status = "scanning".into();
            }
            job.scan_path = def
                .paths
                .first()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|| def.name.into());
        }

        let (bytes, files, status, error) = measure_category(
            shared.clone(),
            def,
            index,
            definitions.len(),
            files_done,
            bytes_done,
        );
        files_done += files;
        bytes_done += bytes;

        let mut guard = shared.scan.lock().unwrap();
        let Some(job) = guard.as_mut() else { return };
        if let Some(category) = job.categories.get_mut(index) {
            category.bytes = bytes;
            category.files = files;
            category.status = status;
            category.error = error;
        }
        job.files_scanned = files_done;
        job.bytes_scanned = bytes_done;
        job.total_junk_bytes = bytes_done;
        job.total_junk_files = files_done;
        job.percent = ((index + 1) as f64 / definitions.len() as f64 * 60.0).round() as u32;
    }

    crate::logging::log(&format!(
        "scan junk phase: {} ms",
        scan_started.elapsed().as_millis()
    ));

    if is_canceled(&shared) {
        mark_canceled(&shared);
        return;
    }

    // Announce the large-file phase before starting the (slower) walk.
    {
        let mut guard = shared.scan.lock().unwrap();
        if let Some(job) = guard.as_mut() {
            job.phase = "large-files".into();
            job.scan_path = "Поиск больших файлов…".into();
        }
    }

    let drives = fixed_drive_letters();
    let home = home_dir();
    let mut roots: Vec<PathBuf> = vec![home.clone()];
    for letter in drives {
        if !home
            .to_string_lossy()
            .to_lowercase()
            .starts_with(&letter.to_lowercase())
        {
            roots.push(PathBuf::from(format!("{letter}\\")));
        }
    }

    let collected: Arc<std::sync::Mutex<Vec<LargeFile>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let progress_counter = Arc::new(AtomicU64::new(0));

    for (root_index, root) in roots.iter().enumerate() {
        let is_home = *root == home;
        let budget = Duration::from_millis(if is_home { 45_000 } else { 25_000 });
        let max_depth = if is_home { 14 } else { 3 };
        let root_started = Instant::now();
        let collected_ref = Arc::clone(&collected);
        let shared_for_progress = shared.clone();
        let roots_len = roots.len();
        let limit = threshold;

        walk(
            root,
            Some(max_depth),
            budget,
            Some(&shared.scan_cancel),
            |entry| {
                if entry.size < limit {
                    return;
                }
                if is_skipped_large_file(&entry.path, is_home, root) {
                    return;
                }
                collected_ref.lock().unwrap().push(LargeFile {
                    path: entry.path.to_string_lossy().to_string(),
                    name: entry
                        .path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    dir: entry
                        .path
                        .parent()
                        .map(|parent| parent.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    size: entry.size,
                    modified: entry.modified,
                });
            },
            |files, bytes, current| {
                progress_counter.fetch_add(files, Ordering::Relaxed);
                let elapsed = root_started.elapsed().as_secs_f64() / budget.as_secs_f64();
                let percent = 60.0
                    + ((root_index as f64 + elapsed.min(0.95)) / roots_len as f64 * 39.0).round();
                let mut guard = shared_for_progress.scan.lock().unwrap();
                if let Some(job) = guard.as_mut() {
                    job.files_scanned = files_done + files;
                    job.bytes_scanned = bytes_done + bytes;
                    job.scan_path = current.to_string_lossy().to_string();
                    job.percent = percent.min(99.0) as u32;
                    let mut sorted = collected_ref.lock().unwrap().clone();
                    sorted.sort_by_key(|file| std::cmp::Reverse(file.size));
                    sorted.truncate(LARGE_FILE_LIMIT);
                    job.large_files = sorted;
                }
            },
        );

        {
            let mut guard = shared.scan.lock().unwrap();
            if let Some(job) = guard.as_mut() {
                job.percent =
                    (60.0 + ((root_index + 1) as f64 / roots_len as f64 * 39.0)).min(99.0) as u32;
            }
        }
        if is_canceled(&shared) {
            mark_canceled(&shared);
            return;
        }
    }

    let mut all = collected.lock().unwrap().clone();
    all.sort_by_key(|file| std::cmp::Reverse(file.size));
    let truncated = all.len() > LARGE_FILE_LIMIT;
    all.truncate(LARGE_FILE_LIMIT);

    // Register large files so the delete command can only touch what the user saw.
    {
        let mut allow = shared.allow.lock().unwrap();
        for file in &all {
            allow.register(Path::new(&file.path));
        }
    }

    let mut guard = shared.scan.lock().unwrap();
    if let Some(job) = guard.as_mut() {
        job.large_files = all;
        job.large_files_truncated = truncated;
        job.percent = 100;
        job.phase = "done".into();
        job.scan_path = String::new();
        job.finished_at = Some(chrono::Utc::now().timestamp_millis());
        crate::logging::log(&format!(
            "scan done junk_bytes={} junk_files={} large={} total={} ms",
            job.total_junk_bytes,
            job.total_junk_files,
            job.large_files.len(),
            scan_started.elapsed().as_millis()
        ));
    }
}

fn is_skipped_large_file(path: &Path, is_home: bool, root: &Path) -> bool {
    let lower = path.to_string_lossy().to_lowercase();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // System files that are always in use or belong to Windows itself.
    if matches!(
        name.as_str(),
        "pagefile.sys" | "hiberfil.sys" | "swapfile.sys"
    ) {
        return true;
    }
    if lower.contains("\\$recycle.bin\\") || lower.contains("\\system volume information\\") {
        return true;
    }

    if !is_home {
        if let Ok(relative) = path.strip_prefix(root) {
            if let Some(first) = relative.components().next() {
                let top = first.as_os_str().to_string_lossy().to_lowercase();
                if matches!(
                    top.as_str(),
                    "windows"
                        | "program files"
                        | "program files (x86)"
                        | "programdata"
                        | "recovery"
                        | "perflogs"
                        | "msocache"
                ) {
                    return true;
                }
            }
        }
    }

    false
}

fn category_percent(index: usize, total: usize) -> u32 {
    (((index as f64 + 0.5) / total.max(1) as f64) * 60.0).min(59.0) as u32
}

fn measure_category(
    shared: AppState,
    def: &CategoryDef,
    index: usize,
    total_categories: usize,
    files_done: u64,
    bytes_done: u64,
) -> (u64, u64, String, Option<String>) {
    if def.kind == "recycle-bin" {
        let info = recycle_bin_info();
        return (
            info.map(|(_, size)| size).unwrap_or(0),
            info.map(|(count, _)| count).unwrap_or(0),
            "done".into(),
            None,
        );
    }

    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut denied = 0u64;
    let mut status = "done".to_string();
    let mut error: Option<String> = None;

    for target in &def.paths {
        let metadata = match fs::metadata(target) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if !metadata.is_dir() {
            continue;
        }

        if def.kind == "files" {
            if let Ok(entries) = fs::read_dir(target) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if def.id == "thumbnails" && !thumbnails_pattern(&name) {
                        continue;
                    }
                    if let Ok(meta) = entry.metadata() {
                        if meta.is_file() {
                            bytes += meta.len();
                            files += 1;
                        }
                    }
                }
            }
            continue;
        }

        let total_entries = def.paths.len().max(1);
        let budget = Duration::from_millis(25_000 / total_entries as u64);
        let shared_for_progress = shared.clone();
        let result = walk(
            target,
            None,
            budget,
            Some(&shared.scan_cancel),
            |_entry| {},
            |_files, _bytes, current| {
                let mut guard = shared_for_progress.scan.lock().unwrap();
                if let Some(job) = guard.as_mut() {
                    job.scan_path = current.to_string_lossy().to_string();
                    let percent = category_percent(index, total_categories);
                    job.percent = job.percent.max(percent);
                }
            },
        );
        bytes += result.bytes;
        files += result.files;
        denied += result.denied;
        if result.timed_out {
            status = "partial".into();
        }
    }

    if bytes == 0 && files == 0 && denied > 0 {
        status = "error".into();
        error = Some(if def.needs_admin {
            "Нужны права администратора".into()
        } else {
            "Нет доступа к папке".into()
        });
    }

    let _ = (files_done, bytes_done);
    (bytes, files, status, error)
}

/* -------------------------------------------------------------------------- */
/*                                  cleanup                                   */
/* -------------------------------------------------------------------------- */

fn recycle_bin_info() -> Option<(u64, u64)> {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };

    let result = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if result < 0 {
        return None;
    }

    Some((info.i64NumItems.max(0) as u64, info.i64Size.max(0) as u64))
}

fn empty_recycle_bin() -> bool {
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };

    let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
    let result = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), flags) };
    result >= 0
}

fn dir_size(path: &Path) -> u64 {
    walk(
        path,
        None,
        Duration::from_secs(8),
        None,
        |_entry| {},
        |_files, _bytes, _current| {},
    )
    .bytes
}

pub fn clean_categories(ids: &[String]) -> CleanResult {
    let definitions = categories();
    let protected = crate::settings::protected_paths();
    let mut result = CleanResult {
        freed_bytes: 0,
        deleted_files: 0,
        categories: Vec::new(),
        errors: Vec::new(),
    };

    for def in definitions
        .iter()
        .filter(|def| ids.iter().any(|id| id == def.id))
    {
        let mut category = CleanCategoryResult {
            id: def.id.into(),
            name: def.name.into(),
            bytes: 0,
            files: 0,
            errors: Vec::new(),
        };

        if def.kind == "recycle-bin" {
            let info = recycle_bin_info();
            if empty_recycle_bin() {
                if let Some((count, size)) = info {
                    category.bytes = size;
                    category.files = count;
                }
            } else {
                category.errors.push("Нет прав для очистки корзины".into());
            }
        } else if def.kind == "files" {
            for target in &def.paths {
                let Ok(entries) = fs::read_dir(target) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if def.id == "thumbnails" && !thumbnails_pattern(&name) {
                        continue;
                    }
                    let path = entry.path();
                    if crate::guard::matches_protected(&path, &protected) {
                        continue;
                    }
                    let Ok(meta) = entry.metadata() else { continue };
                    if !meta.is_file() {
                        continue;
                    }
                    match fs::remove_file(&path) {
                        Ok(()) => {
                            category.bytes += meta.len();
                            category.files += 1;
                        }
                        Err(_) => {
                            if category.errors.len() < 5 {
                                category.errors.push(format!("Занят или защищён: {name}"));
                            }
                        }
                    }
                }
            }
        } else {
            for target in &def.paths {
                let Ok(entries) = fs::read_dir(target) else {
                    continue;
                };
                for entry in entries.flatten() {
                    if entry
                        .file_type()
                        .map(|kind| kind.is_symlink())
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let path = entry.path();
                    if crate::guard::matches_protected(&path, &protected) {
                        continue;
                    }
                    let Ok(meta) = entry.metadata() else { continue };
                    if meta.is_dir() {
                        let size = dir_size(&path);
                        if fs::remove_dir_all(&path).is_ok() {
                            category.bytes += size;
                            category.files += 1;
                        }
                    } else if meta.is_file() {
                        if fs::remove_file(&path).is_ok() {
                            category.bytes += meta.len();
                            category.files += 1;
                        } else if category.errors.len() < 5 {
                            category.errors.push(format!(
                                "Не удалось удалить: {}",
                                entry.file_name().to_string_lossy()
                            ));
                        }
                    }
                }
            }
        }

        result.freed_bytes += category.bytes;
        result.deleted_files += category.files;
        for message in &category.errors {
            result.errors.push(format!("{}: {message}", def.name));
        }
        result.categories.push(category);
    }

    result
}

/// Simulates a cleanup: the same shape as `clean_categories`, but nothing is
/// deleted. Used by the "пробный запуск" button.
pub fn dry_run_categories(ids: &[String]) -> CleanResult {
    let preview = preview_categories(ids, 0);
    let mut result = CleanResult {
        freed_bytes: 0,
        deleted_files: 0,
        categories: Vec::new(),
        errors: Vec::new(),
    };

    for category in preview {
        result.freed_bytes += category.total_bytes;
        result.deleted_files += category.total_files;
        result.categories.push(CleanCategoryResult {
            id: category.id,
            name: category.name,
            bytes: category.total_bytes,
            files: category.total_files,
            errors: Vec::new(),
        });
    }

    result
}

/* -------------------------------------------------------------------------- */
/*                            recycle bin deletion                            */
/* -------------------------------------------------------------------------- */
pub fn move_to_recycle_bin(paths: &[String]) -> RecycleResult {
    let mut result = RecycleResult {
        removed: 0,
        bytes: 0,
        errors: Vec::new(),
    };

    for raw in paths {
        let path = PathBuf::from(raw);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            result
                .errors
                .push(format!("Не найдено: {}", path.display()));
            continue;
        };
        let size = if metadata.is_dir() {
            dir_size(&path)
        } else {
            metadata.len()
        };

        match trash::delete(&path) {
            Ok(()) => {
                result.removed += 1;
                result.bytes += size;
            }
            Err(error) => {
                if result.errors.len() < 5 {
                    result.errors.push(format!("{}: {error}", path.display()));
                }
            }
        }
    }

    result
}

/// Deletes the large files previously returned by a scan. The allow-list makes
/// sure the UI cannot ask for arbitrary paths.
pub fn delete_large_files(shared: &AppState, paths: &[String]) -> RecycleResult {
    let mut valid = Vec::new();
    let mut result = RecycleResult {
        removed: 0,
        bytes: 0,
        errors: Vec::new(),
    };

    {
        let allow = shared.allow.lock().unwrap();
        for raw in paths {
            let path = PathBuf::from(raw);
            match guard::assert_deletable(&path, &allow) {
                Ok(path) => valid.push(path.to_string_lossy().to_string()),
                Err(error) => result.errors.push(error),
            }
        }
    }

    if valid.is_empty() {
        if result.errors.is_empty() {
            result
                .errors
                .push("Файлы не найдены в результатах сканирования".into());
        }
        return result;
    }

    let recycled = move_to_recycle_bin(&valid);
    result.removed = recycled.removed;
    result.bytes = recycled.bytes;
    result.errors.extend(recycled.errors);
    result
}

/* -------------------------------------------------------------------------- */
/*                              explorer helper                               */
/* -------------------------------------------------------------------------- */

pub fn reveal_path(target: &str) -> (bool, Option<String>) {
    let path = PathBuf::from(target);
    if !path.exists() {
        return (false, Some("Файл больше не существует".into()));
    }
    let mut command = std::process::Command::new("explorer.exe");
    command.arg(format!("/select,{}", path.to_string_lossy()));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    match command.spawn() {
        Ok(_) => (true, None),
        Err(error) => (false, Some(error.to_string())),
    }
}

/* -------------------------------------------------------------------------- */
/*                                   drives                                   */
/* -------------------------------------------------------------------------- */

fn fixed_drive_letters() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;
    let mask = unsafe { GetLogicalDrives() };
    let mut result = Vec::new();
    for index in 0..26 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = (b'A' + index as u8) as char;
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if drive_type == DRIVE_FIXED {
            result.push(format!("{letter}:"));
        }
    }
    result
}

/// Read-only list of the largest items a cleanup would remove.
#[allow(clippy::needless_late_init)]
pub fn preview_categories(ids: &[String], limit: usize) -> Vec<PreviewCategory> {
    let definitions = categories();
    let protected = crate::settings::protected_paths();
    let mut result = Vec::new();

    for def in definitions
        .iter()
        .filter(|def| ids.iter().any(|id| id == def.id))
    {
        let mut entries: Vec<PreviewEntry> = Vec::new();
        let mut total_bytes = 0u64;
        let mut total_files = 0u64;
        let mut truncated_by_budget = false;

        if def.kind == "recycle-bin" {
            if let Some((count, size)) = recycle_bin_info() {
                total_files = count;
                total_bytes = size;
            }
        } else if def.kind == "files" {
            for target in &def.paths {
                let Ok(entries_iter) = fs::read_dir(target) else {
                    continue;
                };
                for entry in entries_iter.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if def.id == "thumbnails" && !thumbnails_pattern(&name) {
                        continue;
                    }
                    if crate::guard::matches_protected(&entry.path(), &protected) {
                        continue;
                    }
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            total_bytes += metadata.len();
                            total_files += 1;
                            entries.push(PreviewEntry {
                                path: entry.path().to_string_lossy().to_string(),
                                size: metadata.len(),
                            });
                        }
                    }
                }
            }
        } else {
            let mut timed_out = false;
            for target in &def.paths {
                if !target.is_dir() || crate::guard::matches_protected(target, &protected) {
                    continue;
                }
                let measured = walk(
                    target,
                    None,
                    Duration::from_secs(8),
                    None,
                    |entry| {
                        if crate::guard::matches_protected(&entry.path, &protected) {
                            return;
                        }
                        entries.push(PreviewEntry {
                            path: entry.path.to_string_lossy().to_string(),
                            size: entry.size,
                        });
                    },
                    |_files, _bytes, _current| {},
                );
                total_bytes += measured.bytes;
                total_files += measured.files;
                if measured.timed_out {
                    timed_out = true;
                }
            }
            truncated_by_budget = timed_out;
        }

        entries.sort_by_key(|entry| std::cmp::Reverse(entry.size));
        let truncated = truncated_by_budget || entries.len() > limit;
        entries.truncate(limit);

        result.push(PreviewCategory {
            id: def.id.into(),
            name: def.name.into(),
            total_bytes,
            total_files,
            entries,
            truncated,
        });
    }

    result
}

pub fn category_ids() -> Vec<String> {
    categories().iter().map(|def| def.id.to_string()).collect()
}

/// Read-only size estimate used by the scheduled task before cleaning, so a
/// threshold of "at least N MB" does not start a pointless run.
pub fn estimate_bytes(ids: &[String]) -> u64 {
    let definitions = categories();
    let mut total = 0u64;

    for def in definitions
        .iter()
        .filter(|def| ids.iter().any(|id| id == def.id))
    {
        if def.kind == "recycle-bin" {
            if let Some((_, size)) = recycle_bin_info() {
                total = total.saturating_add(size);
            }
            continue;
        }
        for target in &def.paths {
            if !target.is_dir() {
                continue;
            }
            let result = walk(
                target,
                None,
                Duration::from_secs(4),
                None,
                |_entry| {},
                |_files, _bytes, _current| {},
            );
            total = total.saturating_add(result.bytes);
        }
    }

    total
}

pub fn _quiet_rayon() {
    // Keep the rayon dependency used so the walker can be parallelised later
    // without touching Cargo.toml.
    let _ = rayon::current_num_threads();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_cache_categories_are_registered() {
        let ids = category_ids();
        for id in [
            "npm-cache",
            "pnpm-store",
            "yarn-cache",
            "pip-cache",
            "cargo-cache",
            "gradle-cache",
            "maven-cache",
            "nuget-cache",
            "go-cache",
        ] {
            assert!(ids.iter().any(|value| value == id), "missing {id}");
        }
    }

    #[test]
    fn recycle_bin_query_is_available() {
        assert!(recycle_bin_info().is_some());
    }

    #[test]
    fn large_file_skips_system_entries() {
        assert!(is_skipped_large_file(
            Path::new(r"C:\Windows\System32\pagefile.sys"),
            false,
            Path::new(r"C:\")
        ));
        assert!(is_skipped_large_file(
            Path::new(r"D:\$RECYCLE.BIN\file.bin"),
            false,
            Path::new(r"D:\")
        ));
        assert!(!is_skipped_large_file(
            Path::new(r"C:\Users\me\video.mp4"),
            true,
            Path::new(r"C:\Users\me")
        ));
    }

    #[test]
    fn category_progress_stays_below_sixty() {
        for index in 0..12 {
            assert!(super::category_percent(index, 12) <= 59);
        }
        assert!(super::category_percent(99, 0) <= 59);
    }
}
