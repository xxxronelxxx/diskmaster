use std::process::Command;

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use crate::state::{AppState, ProgramCache};
use crate::types::{InstalledProgram, UninstallResult};

const CACHE_TTL_MS: u128 = 30_000;

const HIVES: [(&str, &str); 3] = [
    (
        "HKLM64",
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ),
    (
        "HKLM32",
        "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ),
    (
        "HKCU",
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ),
];

fn read_string(key: &RegKey, name: &str) -> Option<String> {
    key.get_value::<String, _>(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_dword(key: &RegKey, name: &str) -> u32 {
    key.get_value::<u32, _>(name).unwrap_or(0)
}

fn read_size(key: &RegKey) -> u64 {
    if let Ok(value) = key.get_value::<u32, _>("EstimatedSize") {
        return value as u64 * 1024;
    }
    if let Ok(value) = key.get_value::<u64, _>("EstimatedSize") {
        return value * 1024;
    }
    0
}

fn extract_exe(value: &str) -> Option<&str> {
    if let Some(start) = value.find('"') {
        let rest = &value[start + 1..];
        if let Some(end) = rest.find('"') {
            let inner = &rest[..end];
            if inner.to_lowercase().ends_with(".exe") {
                return Some(inner);
            }
        }
    }

    let lower = value.to_lowercase();
    let index = lower.find(".exe")?;
    let end = index + 4;
    let start = value[..end]
        .rfind([':', '\\'])
        .map(|pos| {
            if value.as_bytes().get(pos + 1) == Some(&b'\\') {
                pos.saturating_sub(1)
            } else {
                pos + 1
            }
        })
        .unwrap_or(0);
    let candidate = value[start..end].trim();
    if candidate.contains(':') {
        Some(candidate)
    } else {
        None
    }
}

/// Many MSI entries leave `InstallLocation` empty, so fall back to the folder
/// of the uninstaller/icon executable.
fn derive_location(
    registered: Option<&str>,
    display_icon: Option<&str>,
    uninstall: Option<&str>,
    quiet_uninstall: Option<&str>,
) -> Option<String> {
    if let Some(location) = registered {
        let trimmed = location
            .trim()
            .trim_matches('"')
            .trim()
            .trim_end_matches(['\\', '/']);
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    for candidate in [display_icon, uninstall, quiet_uninstall]
        .into_iter()
        .flatten()
    {
        let cleaned = candidate
            .trim()
            .trim_end_matches(|c: char| c.is_ascii_digit() || c == ',' || c == ' ')
            .trim();
        let Some(executable) = extract_exe(cleaned) else {
            continue;
        };
        let normalized = executable.replace('/', "\\");
        let Some(index) = normalized.rfind('\\') else {
            continue;
        };
        if index <= 2 {
            continue;
        }
        let directory = &normalized[..index];
        if directory.to_lowercase().starts_with("c:\\windows") {
            continue;
        }
        return Some(directory.to_string());
    }

    None
}

fn read_hive(root: &RegKey, prefix: &str, path: &str, out: &mut Vec<InstalledProgram>) {
    let Ok(uninstall) = root.open_subkey_with_flags(path, KEY_READ) else {
        return;
    };

    for name in uninstall.enum_keys().flatten() {
        let Ok(key) = uninstall.open_subkey_with_flags(&name, KEY_READ) else {
            continue;
        };
        let Some(display_name) = read_string(&key, "DisplayName") else {
            continue;
        };
        if read_dword(&key, "SystemComponent") == 1 {
            continue;
        }
        if read_string(&key, "ParentKeyName").is_some() {
            continue;
        }
        let release = read_string(&key, "ReleaseType")
            .unwrap_or_default()
            .to_lowercase();
        if release.contains("update") || release.contains("hotfix") || release.contains("security")
        {
            continue;
        }

        let location_raw = read_string(&key, "InstallLocation");
        let uninstall_string = read_string(&key, "UninstallString");
        let quiet_uninstall_string = read_string(&key, "QuietUninstallString");
        let display_icon = read_string(&key, "DisplayIcon");

        out.push(InstalledProgram {
            key: format!("{prefix}|{name}"),
            name: display_name,
            version: read_string(&key, "DisplayVersion"),
            publisher: read_string(&key, "Publisher"),
            install_date: read_string(&key, "InstallDate"),
            size_bytes: read_size(&key),
            location: derive_location(
                location_raw.as_deref(),
                display_icon.as_deref(),
                uninstall_string.as_deref(),
                quiet_uninstall_string.as_deref(),
            ),
            uninstall_string,
            quiet_uninstall_string,
            no_remove: read_dword(&key, "NoRemove") == 1,
            display_icon,
        });
    }
}

fn load() -> Vec<InstalledProgram> {
    let mut items = Vec::new();
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    let user = RegKey::predef(HKEY_CURRENT_USER);

    for (prefix, path) in HIVES {
        let root = if prefix.starts_with("HKLM") {
            &machine
        } else {
            &user
        };
        read_hive(root, prefix, path, &mut items);
    }

    items.sort_by_key(|item| item.name.to_lowercase());
    items
}

pub fn list(shared: &AppState, force: bool) -> Vec<InstalledProgram> {
    if !force {
        let guard = shared.programs.lock().unwrap();
        if let Some(cache) = guard.as_ref() {
            if cache.at.elapsed().as_millis() < CACHE_TTL_MS {
                return cache.items.clone();
            }
        }
    }

    let started = std::time::Instant::now();
    let items = load();
    let elapsed = started.elapsed().as_millis();
    if elapsed > 400 {
        crate::logging::log(&format!(
            "programs list took {elapsed} ms ({} items)",
            items.len()
        ));
    }

    let mut guard = shared.programs.lock().unwrap();
    *guard = Some(ProgramCache {
        at: std::time::Instant::now(),
        items: items.clone(),
    });
    items
}

fn find_guid(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut start = None;
    for (index, ch) in bytes.iter().enumerate() {
        if *ch == b'{' {
            start = Some(index);
        } else if *ch == b'}' {
            if let Some(begin) = start {
                let candidate = &value[begin..=index];
                if candidate.len() == 38
                    && candidate[1..]
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() || c == '-')
                {
                    return Some(candidate.to_string());
                }
            }
            start = None;
        }
    }
    None
}

impl InstalledProgram {
    fn uninstall_command(&self, quiet: bool) -> Option<String> {
        let uninstall = self
            .uninstall_string
            .clone()
            .or_else(|| self.quiet_uninstall_string.clone())?;
        let guid = find_guid(&uninstall)
            .or_else(|| self.quiet_uninstall_string.as_deref().and_then(find_guid));

        if quiet {
            if let Some(quiet_string) = self.quiet_uninstall_string.clone() {
                return Some(quiet_string);
            }
            if let Some(guid) = guid {
                return Some(format!("msiexec.exe /x {guid} /qn /norestart"));
            }
            return Some(format!("{uninstall} /S"));
        }

        if let Some(guid) = guid {
            if uninstall.to_lowercase().contains("msiexec") {
                return Some(format!("msiexec.exe /x {guid} /qb /norestart"));
            }
        }
        Some(uninstall)
    }
}

pub fn uninstall(shared: &AppState, key: &str, quiet: bool) -> UninstallResult {
    let programs = list(shared, true);
    let Some(program) = programs.into_iter().find(|item| item.key == key) else {
        return UninstallResult {
            launched: false,
            command: None,
            error: Some("Программа не найдена в списке установленных".into()),
        };
    };
    if program.no_remove {
        return UninstallResult {
            launched: false,
            command: None,
            error: Some("Издатель запретил удаление через реестр".into()),
        };
    }
    let Some(command) = program.uninstall_command(quiet) else {
        return UninstallResult {
            launched: false,
            command: None,
            error: Some("У программы нет команды удаления".into()),
        };
    };

    let mut child = Command::new("cmd.exe");
    child.args(["/c", &command]);
    match child.spawn() {
        Ok(_) => {
            *shared.programs.lock().unwrap() = None;
            UninstallResult {
                launched: true,
                command: Some(command),
                error: None,
            }
        }
        Err(error) => UninstallResult {
            launched: false,
            command: Some(command),
            error: Some(error.to_string()),
        },
    }
}
