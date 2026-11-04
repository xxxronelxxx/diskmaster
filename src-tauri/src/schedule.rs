use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::paths::storage_path;
use crate::ps;
use crate::scan::clean_categories;
use crate::types::{CleanResult, ScheduleConfig, ScheduleRun, ScheduleStatus};

const TASK_NAME: &str = "DiskMasterAutoClean";
const TASK_PATH: &str = "\\DiskMaster\\";

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

fn register_task(config: &ScheduleConfig) -> (bool, Option<String>) {
    let start_time = if config.time.len() == 5 && config.time.as_bytes().get(2) == Some(&b':') {
        config.time.clone()
    } else {
        "12:00".into()
    };
    let categories = config.categories.join(",");
    let args = format!(
        "--run-scheduled-clean --categories={categories} --threshold={}",
        config.threshold_mb
    );
    let exe = runner();
    let work_dir = exe
        .parent()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    let log = log_file();
    let command = format!(
        "\"{}\" {} >> \"{}\" 2>&1",
        exe.to_string_lossy(),
        args,
        log.to_string_lossy()
    );

    let days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let trigger = match config.frequency.as_str() {
        "daily" => format!("New-ScheduledTaskTrigger -Daily -At '{}'", start_time),
        "monthly" => format!("New-ScheduledTaskTrigger -Monthly -DaysOfMonth 1 -At '{}'", start_time),
        "logon" => "New-ScheduledTaskTrigger -AtLogOn".to_string(),
        _ => {
            let day = days[(config.day_of_week as usize).min(6)];
            format!(
                "New-ScheduledTaskTrigger -Weekly -DaysOfWeek {} -At '{}'",
                day, start_time
            )
        }
    };

    let script = format!(
        "$ErrorActionPreference = 'Stop'; try {{ \
         $action = New-ScheduledTaskAction -Execute 'cmd.exe' -Argument {} -WorkingDirectory {}; \
         $trigger = {trigger}; \
         $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries \
         -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Hours 2); \
         Register-ScheduledTask -TaskName '{}' -TaskPath '{}' -Action $action -Trigger $trigger \
         -Settings $settings -Description 'Автоматическая очистка диска (Диск-Мастер)' -Force | Out-Null; \
         Write-Output 'ok' }} catch {{ Write-Output ('ERR ' + $_.Exception.Message) }}",
        ps::quote(&format!("/c {command}")),
        ps::quote(&work_dir),
        TASK_NAME,
        TASK_PATH
    );

    let output = ps::run(&script, Duration::from_secs(60));
    if output.stdout.contains("ok") {
        (true, None)
    } else {
        let message = output.stdout.replace("ERR", "").trim().to_string();
        let lower = message.to_lowercase();
        let friendly = if lower.contains("denied") || message.contains("отказано") {
            "Для создания задания в планировщике нужны права администратора".to_string()
        } else if message.is_empty() {
            "Не удалось создать задание в планировщике".to_string()
        } else {
            message.chars().take(300).collect()
        };
        (false, Some(friendly))
    }
}

fn unregister_task() {
    let script = format!(
        "Unregister-ScheduledTask -TaskName '{TASK_NAME}' -TaskPath '{}' -Confirm:$false -ErrorAction SilentlyContinue",
        TASK_PATH
    );
    ps::run(&script, Duration::from_secs(30));
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
    (next, registered, error)
}

pub fn status() -> ScheduleStatus {
    let config = read_config();
    let script = format!(
        "$task = Get-ScheduledTask -TaskName '{TASK_NAME}' -TaskPath '{}' -ErrorAction SilentlyContinue; \
         if ($task) {{ $info = Get-ScheduledTaskInfo -TaskName '{TASK_NAME}' -TaskPath '{}' -ErrorAction SilentlyContinue; \
         if ($info -and $info.NextRunTime) {{ Write-Output ('yes|' + $info.NextRunTime.ToString('o')) }} else {{ Write-Output 'yes|' }} }} \
         else {{ Write-Output 'no|' }}",
        TASK_PATH, TASK_PATH
    );
    let output = ps::run(&script, Duration::from_secs(30));
    let text = output.stdout.trim();
    let (flag, next) = text.split_once('|').unwrap_or(("no", ""));
    ScheduleStatus {
        config,
        registered: flag == "yes",
        next_run: if next.is_empty() { None } else { Some(next.to_string()) },
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
pub fn scheduled_clean(input: &[String]) -> CleanResult {
    let config = read_config();
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

    let result = clean_categories(&categories);
    append_run(&ScheduleRun {
        at: chrono::Utc::now().timestamp_millis(),
        freed_bytes: result.freed_bytes,
        files: result.deleted_files,
        categories,
        trigger: "scheduled".into(),
    });
    result
}

