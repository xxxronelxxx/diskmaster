use std::time::Duration;

use crate::ps;
use crate::types::StartupItem;

const LIST_SCRIPT: &str = include_str!("../scripts/startup.ps1");

pub fn list() -> Vec<StartupItem> {
    let data = ps::run_json::<Vec<StartupItem>>(LIST_SCRIPT, Duration::from_secs(90)).unwrap_or_default();
    let mut items: Vec<StartupItem> = data.into_iter().filter(|item| !item.id.is_empty()).collect();
    items.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    items
}

fn split_id(id: &str) -> (String, Vec<String>) {
    let mut parts = id.split('|');
    let kind = parts.next().unwrap_or_default().to_string();
    let rest = parts.map(str::to_string).collect();
    (kind, rest)
}

pub fn toggle(id: &str, enabled: bool) -> (bool, Option<String>) {
    let (kind, rest) = split_id(id);

    match kind.as_str() {
        "reg" => {
            let Some(key_path) = rest.first() else {
                return (false, Some("Некорректный элемент".into()));
            };
            let name = rest[1..].join("|");
            if name.is_empty() {
                return (false, Some("Некорректный элемент".into()));
            }
            let approved = if key_path.to_uppercase().starts_with("HKCU") {
                "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run"
            } else {
                "HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run"
            };
            let bytes = if enabled {
                "[byte[]](2,0,0,0,0,0,0,0,0,0,0,0)"
            } else {
                "[byte[]](3,0,0,0,0,0,0,0,0,0,0,0)"
            };
            let script = format!(
                "$approved = {}; if (-not (Test-Path $approved)) {{ New-Item -Path $approved -Force | Out-Null }}; \
                 $bytes = {bytes}; New-ItemProperty -Path $approved -Name {} -Value $bytes -PropertyType Binary -Force | Out-Null; \
                 Write-Output 'ok'",
                ps::quote(approved),
                ps::quote(&name)
            );
            finish(ps::run(&script, Duration::from_secs(30)))
        }
        "task" => {
            let task_path = rest.first().cloned().unwrap_or_else(|| "\\".into());
            let task_name = rest[1..].join("|");
            let verb = if enabled { "Enable-ScheduledTask" } else { "Disable-ScheduledTask" };
            let script = format!(
                "try {{ {verb} -TaskName {} -TaskPath {} | Out-Null; Write-Output 'ok' }} \
                 catch {{ Write-Output ('ERR ' + $_.Exception.Message) }}",
                ps::quote(&task_name),
                ps::quote(&task_path)
            );
            finish(ps::run(&script, Duration::from_secs(30)))
        }
        "folder" => {
            let target = rest.join("|");
            let file_name = target.rsplit(['\\', '/']).next().unwrap_or_default().to_string();
            let approved = "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder";
            let bytes = if enabled {
                "[byte[]](2,0,0,0,0,0,0,0,0,0,0,0)"
            } else {
                "[byte[]](3,0,0,0,0,0,0,0,0,0,0,0)"
            };
            let script = format!(
                "$approved = {}; if (-not (Test-Path $approved)) {{ New-Item -Path $approved -Force | Out-Null }}; \
                 $bytes = {bytes}; New-ItemProperty -Path $approved -Name {} -Value $bytes -PropertyType Binary -Force | Out-Null; \
                 Write-Output 'ok'",
                ps::quote(approved),
                ps::quote(&file_name)
            );
            finish(ps::run(&script, Duration::from_secs(30)))
        }
        _ => (false, Some("Неизвестный тип элемента".into())),
    }
}

pub fn delete(id: &str) -> (bool, Option<String>) {
    let (kind, rest) = split_id(id);

    match kind.as_str() {
        "reg" => {
            let Some(key_path) = rest.first() else {
                return (false, Some("Некорректный элемент".into()));
            };
            let name = rest[1..].join("|");
            let script = format!(
                "try {{ Remove-ItemProperty -Path {} -Name {} -ErrorAction Stop; Write-Output 'ok' }} \
                 catch {{ Write-Output ('ERR ' + $_.Exception.Message) }}",
                ps::quote(key_path),
                ps::quote(&name)
            );
            finish(ps::run(&script, Duration::from_secs(30)))
        }
        "folder" => {
            let target = rest.join("|");
            let script = format!(
                "try {{ Remove-Item -LiteralPath {} -Force -ErrorAction Stop; Write-Output 'ok' }} \
                 catch {{ Write-Output ('ERR ' + $_.Exception.Message) }}",
                ps::quote(&target)
            );
            finish(ps::run(&script, Duration::from_secs(30)))
        }
        "task" => {
            let task_path = rest.first().cloned().unwrap_or_else(|| "\\".into());
            let task_name = rest[1..].join("|");
            let script = format!(
                "try {{ Unregister-ScheduledTask -TaskName {} -TaskPath {} -Confirm:$false -ErrorAction Stop; \
                 Write-Output 'ok' }} catch {{ Write-Output ('ERR ' + $_.Exception.Message) }}",
                ps::quote(&task_name),
                ps::quote(&task_path)
            );
            finish(ps::run(&script, Duration::from_secs(30)))
        }
        _ => (false, Some("Неизвестный тип элемента".into())),
    }
}

fn finish(output: crate::ps::PsOutput) -> (bool, Option<String>) {
    if output.stdout.contains("ok") {
        return (true, None);
    }
    let message = output
        .stdout
        .replace("ERR", "")
        .trim()
        .to_string()
        .or_empty()
        .unwrap_or_else(|| output.stderr.trim().to_string());
    let lower = message.to_lowercase();
    let friendly = if lower.contains("denied") || message.contains("отказано") || lower.contains("администратор") {
        "Нужны права администратора".to_string()
    } else if message.is_empty() {
        "Не удалось изменить состояние".to_string()
    } else {
        message.chars().take(200).collect()
    };
    (false, Some(friendly))
}

trait OrEmpty {
    fn or_empty(self) -> Option<String>;
}

impl OrEmpty for String {
    fn or_empty(self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}
