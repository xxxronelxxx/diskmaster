use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use crate::paths::storage_path;
use crate::scan::clean_categories;
use crate::types::{CleanResult, ScheduleConfig, ScheduleRun, ScheduleStatus};

const TASK_NAME: &str = "DiskMasterAutoClean";
const TASK_PATH: &str = "\\DiskMaster\\";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn config_file() -> PathBuf {
    storage_path(&["schedule", "config.json"])
}

fn log_file() -> PathBuf {
    storage_path(&["schedule", "history.log"])
}

pub fn read_config() -> ScheduleConfig {
    let path = config_file();
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => ScheduleConfig::default(),
    }
}

fn write_config(config: &ScheduleConfig) -> std::io::Result<()> {
    let path = config_file();
    crate::paths::ensure_parent(&path);
    fs::write(path, serde_json::to_string_pretty(config)?)
}

pub fn read_history(limit: usize) -> Vec<ScheduleRun> {
    let Ok(raw) = fs::read_to_string(log_file()) else {
        return Vec::new();
    };
    let mut runs: Vec<ScheduleRun> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<ScheduleRun>(line).ok())
        .collect();
    runs.reverse();
    runs.truncate(limit);
    runs
}

pub fn append_run(run: &ScheduleRun) {
    let path = log_file();
    crate::paths::ensure_parent(&path);
    if let Ok(line) = serde_json::to_string(run) {
        use std::io::Write;
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

fn valid_categories(ids: &[String]) -> Vec<String> {
    let known: Vec<String> = crate::scan::category_ids();
    ids.iter()
        .filter(|id| known.iter().any(|known| known == *id))
        .cloned()
        .collect()
}

fn runner() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("DiskMaster.exe"))
}

fn task_full_name() -> String {
    format!("{TASK_PATH}{TASK_NAME}")
}

fn schtasks_run(args: &[&str]) -> (bool, String) {
    let mut command = Command::new("schtasks.exe");
    command.args(args).creation_flags(CREATE_NO_WINDOW);
    match command.output() {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            (output.status.success(), text)
        }
        Err(error) => (false, error.to_string()),
    }
}

fn register_task(config: &ScheduleConfig) -> (bool, Option<String>) {
    let start_time = if config.time.len() == 5 && config.time.as_bytes().get(2) == Some(&b':') {
        config.time.clone()
    } else {
        "12:00".into()
    };
    let categories = config.categories.join(",");
    let task_command = format!(
        "\"{}\" --run-scheduled-clean --categories={categories} --threshold={}",
        runner().to_string_lossy(),
        config.threshold_mb
    );

    let days = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    let mut args: Vec<String> = vec![
        "/create".into(),
        "/tn".into(),
        task_full_name(),
        "/tr".into(),
        task_command,
        "/f".into(),
    ];

    match config.frequency.as_str() {
        "daily" => {
            args.extend(["/sc".into(), "DAILY".into(), "/st".into(), start_time]);
        }
        "monthly" => {
            args.extend([
                "/sc".into(),
                "MONTHLY".into(),
                "/d".into(),
                "1".into(),
                "/st".into(),
                start_time,
            ]);
        }
        "logon" => {
            args.extend(["/sc".into(), "ONLOGON".into()]);
        }
        _ => {
            let day = days[(config.day_of_week as usize).min(6)];
            args.extend([
                "/sc".into(),
                "WEEKLY".into(),
                "/d".into(),
                day.into(),
                "/st".into(),
                start_time,
            ]);
        }
    }

    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (ok, output) = schtasks_run(&refs);
    if ok {
        return (true, None);
    }

    let lower = output.to_lowercase();
    let message = if lower.contains("denied")
        || lower.contains("access is denied")
        || output.contains("отказано")
    {
        "Для создания задания в планировщике нужны права администратора".to_string()
    } else {
        "Не удалось создать задание в планировщике".to_string()
    };
    (false, Some(message))
}

fn unregister_task() {
    let full = task_full_name();
    let _ = schtasks_run(&["/delete", "/tn", &full, "/f"]);
}

pub fn save(config: &ScheduleConfig) -> (ScheduleConfig, bool, Option<String>) {
    let next = ScheduleConfig {
        categories: valid_categories(&config.categories),
        ..config.clone()
    };
    let _ = write_config(&next);

    if !next.enabled {
        unregister_task();
        return (next, false, None);
    }

    let (registered, error) = register_task(&next);
    crate::logging::log(&format!(
        "schedule save enabled={} registered={} error={:?}",
        next.enabled, registered, error
    ));
    (next, registered, error)
}

pub fn status() -> ScheduleStatus {
    let config = read_config();
    let full = task_full_name();
    let (ok, text) = schtasks_run(&["/query", "/tn", &full, "/fo", "CSV", "/v"]);

    if !ok {
        return ScheduleStatus {
            config,
            registered: false,
            next_run: None,
        };
    }

    let row = text.lines().nth(1).map(crate::startup::parse_csv_line);
    let next_run = row.as_ref().and_then(|fields| {
        fields
            .get(2)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value != "N/A")
    });

    ScheduleStatus {
        config,
        registered: true,
        next_run,
    }
}

pub fn run_now() -> CleanResult {
    let config = read_config();
    let categories = valid_categories(&config.categories);
    if categories.is_empty() {
        return CleanResult {
            freed_bytes: 0,
            deleted_files: 0,
            categories: Vec::new(),
            errors: vec!["Не выбраны категории для очистки".into()],
        };
    }

    let result = clean_categories(&categories);
    append_run(&ScheduleRun {
        at: chrono::Utc::now().timestamp_millis(),
        freed_bytes: result.freed_bytes,
        files: result.deleted_files,
        categories: categories.clone(),
        trigger: "manual".into(),
    });

    let mut next = config;
    next.last_run_at = Some(chrono::Utc::now().timestamp_millis());
    next.last_freed_bytes = Some(result.freed_bytes);
    let _ = write_config(&next);
    result
}

/// Headless entry point used by `--run-scheduled-clean` and by the scheduled
/// task itself.
pub fn scheduled_clean(input: &[String], threshold_arg: Option<u64>) -> CleanResult {
    let mut config = read_config();
    let mut categories = valid_categories(input);
    if categories.is_empty() {
        categories = valid_categories(&config.categories);
    }
    if categories.is_empty() {
        return CleanResult {
            freed_bytes: 0,
            deleted_files: 0,
            categories: Vec::new(),
            errors: vec!["Не выбраны категории".into()],
        };
    }

    let threshold_mb = threshold_arg.unwrap_or(config.threshold_mb);
    let threshold_bytes = threshold_mb.saturating_mul(1024 * 1024);
    if threshold_bytes > 0 {
        let available = crate::scan::estimate_bytes(&categories);
        if available < threshold_bytes {
            let at = chrono::Utc::now().timestamp_millis();
            append_run(&ScheduleRun {
                at,
                freed_bytes: 0,
                files: 0,
                categories: categories.clone(),
                trigger: "scheduled".into(),
            });
            config.last_run_at = Some(at);
            config.last_freed_bytes = Some(0);
            let _ = write_config(&config);
            crate::logging::log(&format!(
                "scheduled clean skipped: {available} bytes below threshold {threshold_bytes}"
            ));
            return CleanResult {
                freed_bytes: 0,
                deleted_files: 0,
                categories: Vec::new(),
                errors: vec![format!(
                    "Порог не достигнут: найдено {} МБ из {} МБ",
                    available / (1024 * 1024),
                    threshold_mb
                )],
            };
        }
    }

    let result = clean_categories(&categories);
    let at = chrono::Utc::now().timestamp_millis();
    append_run(&ScheduleRun {
        at,
        freed_bytes: result.freed_bytes,
        files: result.deleted_files,
        categories,
        trigger: "scheduled".into(),
    });

    config.last_run_at = Some(at);
    config.last_freed_bytes = Some(result.freed_bytes);
    let _ = write_config(&config);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_schedule_never_cleans_the_recycle_bin() {
        assert!(!ScheduleConfig::default()
            .categories
            .iter()
            .any(|id| id == "recycle-bin"));
    }
}
