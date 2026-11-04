use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_BINARY};
use winreg::{RegKey, RegValue};

use crate::types::StartupItem;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPROVED_RUN: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
const APPROVED_STARTUP_FOLDER: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    fn key(self) -> RegKey {
        match self {
            Hive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
            Hive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
        }
    }
}

fn expand_env(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            result.push_str(&std::env::var(name).unwrap_or_default());
            rest = &after[end + 1..];
        } else {
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

fn extract_executable(command: &str) -> Option<String> {
    let expanded = expand_env(command);
    if let Some(start) = expanded.find('"') {
        let rest = &expanded[start + 1..];
        if let Some(end) = rest.find('"') {
            let inner = rest[..end].to_string();
            if inner.to_lowercase().ends_with(".exe") {
                return Some(inner);
            }
        }
    }
    let lower = expanded.to_lowercase();
    if let Some(index) = lower.find(".exe") {
        let end = index + 4;
        let start = expanded[..end]
            .rfind([':', '\\'])
            .map(|pos| {
                if expanded.as_bytes().get(pos + 1) == Some(&b'\\') {
                    pos.saturating_sub(1)
                } else {
                    pos + 1
                }
            })
            .unwrap_or(0);
        let candidate = expanded[start..end].trim();
        if candidate.contains(':') {
            return Some(candidate.to_string());
        }
    }
    None
}

fn make_item(
    id: String,
    name: String,
    command: String,
    location: String,
    source: &str,
    enabled: bool,
    system: bool,
) -> StartupItem {
    let executable = extract_executable(&command);
    let exists = executable
        .as_deref()
        .map(|path| Path::new(path).exists())
        .unwrap_or(false);
    StartupItem {
        id,
        name,
        command,
        location,
        source: source.into(),
        enabled,
        publisher: None,
        executable,
        exists,
        system,
    }
}

fn approved_value(hive: Hive, section: &str, value_name: &str) -> Option<bool> {
    let root = hive.key();
    let key = root.open_subkey_with_flags(section, KEY_READ).ok()?;
    let raw = key.get_raw_value(value_name).ok()?;
    Some(raw.bytes.first().map(|byte| byte & 1 == 1).unwrap_or(false))
}

fn registry_items(hive: Hive, path: &str, label: &str, out: &mut Vec<StartupItem>) {
    let root = hive.key();
    let Ok(key) = root.open_subkey_with_flags(path, KEY_READ) else {
        return;
    };

    for value in key.enum_values().flatten() {
        let name = value.0;
        let command = value.1.to_string();
        if command.trim().is_empty() {
            continue;
        }
        let disabled = approved_value(hive, APPROVED_RUN, &name).unwrap_or(false);
        out.push(make_item(
            format!("reg|{path}|{name}"),
            name,
            command,
            label.into(),
            "registry",
            !disabled,
            false,
        ));
    }
}

fn startup_folder(hive: Hive, path: Option<PathBuf>, label: &str, out: &mut Vec<StartupItem>) {
    let Some(folder) = path else { return };
    let Ok(entries) = fs::read_dir(&folder) else {
        return;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.eq_ignore_ascii_case("desktop.ini")
            || file_name.to_lowercase().ends_with(".log")
        {
            continue;
        }
        let path = entry.path();
        let disabled = approved_value(hive, APPROVED_STARTUP_FOLDER, &file_name).unwrap_or(false);
        out.push(make_item(
            format!("folder|{}", path.to_string_lossy()),
            Path::new(&file_name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or(file_name.clone()),
            path.to_string_lossy().to_string(),
            label.into(),
            "startup-folder",
            !disabled,
            false,
        ));
    }
}

/* -------------------------------------------------------------------------- */
/*                          scheduled tasks (schtasks)                        */
/* -------------------------------------------------------------------------- */

pub(crate) fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == ',' {
            fields.push(current.trim().to_string());
            current = String::new();
        } else {
            current.push(ch);
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn schtasks_query() -> Vec<StartupItem> {
    let mut command = Command::new("cmd.exe");
    command.args(["/c", "chcp 65001>nul && schtasks /query /fo CSV /v"]);
    command.creation_flags(CREATE_NO_WINDOW);

    let Ok(output) = command.output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut items = Vec::new();

    for line in text.lines().skip(1) {
        let fields = parse_csv_line(line);
        if fields.len() < 19 {
            continue;
        }
        let full_name = fields[1].trim();
        if full_name.is_empty() || full_name.eq_ignore_ascii_case("TaskName") {
            continue;
        }
        let schedule_type = fields[18].to_lowercase();
        if !(schedule_type.contains("logon")
            || schedule_type.contains("system start")
            || schedule_type.contains("вход")
            || schedule_type.contains("запуск"))
        {
            continue;
        }

        let state = fields[11].to_lowercase();
        let status = fields[3].to_lowercase();
        let enabled = !(state.contains("disabled")
            || state.contains("отключ")
            || status.contains("disabled")
            || status.contains("отключ"));
        let command_line = fields[8].clone();

        let (path, name) = match full_name.rfind('\\') {
            Some(index) => (&full_name[..index], &full_name[index + 1..]),
            None => ("\\", full_name),
        };
        let system = path.to_lowercase().starts_with("\\microsoft\\windows");

        items.push(make_item(
            format!("task|{path}|{name}"),
            name.to_string(),
            command_line,
            path.to_string(),
            "task",
            enabled,
            system,
        ));
    }

    items
}

/* -------------------------------------------------------------------------- */
/*                                   API                                      */
/* -------------------------------------------------------------------------- */

pub fn list() -> Vec<StartupItem> {
    let mut items = Vec::new();

    registry_items(Hive::CurrentUser, RUN_KEY, "HKCU\\Run", &mut items);
    registry_items(Hive::LocalMachine, RUN_KEY, "HKLM\\Run", &mut items);
    registry_items(
        Hive::LocalMachine,
        "Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run",
        "HKLM\\Run (x86)",
        &mut items,
    );

    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let programdata = std::env::var_os("PROGRAMDATA").map(PathBuf::from);
    startup_folder(
        Hive::CurrentUser,
        appdata.map(|base| {
            base.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
        }),
        "Startup (пользователь)",
        &mut items,
    );
    startup_folder(
        Hive::LocalMachine,
        programdata.map(|base| {
            base.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
        }),
        "Startup (все)",
        &mut items,
    );

    items.extend(schtasks_query());
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

fn write_approved(hive: Hive, section: &str, name: &str, enabled: bool) -> bool {
    let root = hive.key();
    let Ok(key) = root.open_subkey_with_flags(section, KEY_READ | KEY_WRITE) else {
        return false;
    };
    let bytes = if enabled {
        vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    } else {
        vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    };
    key.set_raw_value(
        name,
        &RegValue {
            bytes,
            vtype: REG_BINARY,
        },
    )
    .is_ok()
}

fn run_schtasks(args: &[&str]) -> bool {
    let mut command = Command::new("schtasks.exe");
    command.args(args).creation_flags(CREATE_NO_WINDOW);
    matches!(command.output(), Ok(output) if output.status.success())
}

pub fn toggle(id: &str, enabled: bool) -> (bool, Option<String>) {
    let (kind, rest) = split_id(id);

    match kind.as_str() {
        "reg" => {
            let Some(path) = rest.first() else {
                return (false, Some("Некорректный элемент".into()));
            };
            let name = rest[1..].join("|");
            let hive = if path.to_uppercase().starts_with("HKCU") {
                Hive::CurrentUser
            } else {
                Hive::LocalMachine
            };
            if write_approved(hive, APPROVED_RUN, &name, enabled) {
                (true, None)
            } else {
                (
                    false,
                    Some(if enabled {
                        "Не удалось включить элемент автозагрузки".into()
                    } else {
                        "Не удалось отключить элемент автозагрузки".into()
                    }),
                )
            }
        }
        "folder" => {
            let target = rest.join("|");
            let file_name = target.rsplit(['\\', '/']).next().unwrap_or_default();
            let hive = if target.to_uppercase().starts_with("HKLM") {
                Hive::LocalMachine
            } else {
                Hive::CurrentUser
            };
            if write_approved(hive, APPROVED_STARTUP_FOLDER, file_name, enabled) {
                (true, None)
            } else {
                (false, Some("Не удалось изменить состояние".into()))
            }
        }
        "task" => {
            let path = rest.first().cloned().unwrap_or_else(|| "\\".into());
            let name = rest[1..].join("|");
            let full = format!("{path}\\{name}");
            let verb = if enabled { "/enable" } else { "/disable" };
            if run_schtasks(&["/change", "/tn", &full, verb]) {
                (true, None)
            } else {
                (
                    false,
                    Some("Не удалось изменить задачу (нужны права администратора)".into()),
                )
            }
        }
        _ => (false, Some("Неизвестный тип элемента".into())),
    }
}

pub fn delete(id: &str) -> (bool, Option<String>) {
    let (kind, rest) = split_id(id);

    match kind.as_str() {
        "reg" => {
            let Some(path) = rest.first() else {
                return (false, Some("Некорректный элемент".into()));
            };
            let name = rest[1..].join("|");
            let hive = if path.to_uppercase().starts_with("HKCU") {
                Hive::CurrentUser
            } else {
                Hive::LocalMachine
            };
            let root = hive.key();
            let Ok(key) = root.open_subkey_with_flags(path, KEY_READ | KEY_WRITE) else {
                return (false, Some("Ключ реестра не найден".into()));
            };
            match key.delete_value(&name) {
                Ok(()) => (true, None),
                Err(error) => (false, Some(error.to_string())),
            }
        }
        "folder" => {
            let target = rest.join("|");
            match fs::remove_file(&target) {
                Ok(()) => (true, None),
                Err(error) => (false, Some(error.to_string())),
            }
        }
        "task" => {
            let path = rest.first().cloned().unwrap_or_else(|| "\\".into());
            let name = rest[1..].join("|");
            let full = format!("{path}\\{name}");
            if run_schtasks(&["/delete", "/tn", &full, "/f"]) {
                (true, None)
            } else {
                (
                    false,
                    Some("Не удалось удалить задачу (нужны права администратора)".into()),
                )
            }
        }
        _ => (false, Some("Неизвестный тип элемента".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_csv_line;

    #[test]
    fn parses_quoted_csv() {
        let row = r#""SUPER","\Test\Task","N/A","Ready","Interactive only","N/A","0","Author","cmd.exe /c echo","N/A","","Enabled""#;
        let fields = parse_csv_line(row);
        assert_eq!(fields[1], r"\Test\Task");
        assert_eq!(fields[3], "Ready");
        assert_eq!(fields[8], "cmd.exe /c echo");
    }
}
