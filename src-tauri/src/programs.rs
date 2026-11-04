use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::ps;
use crate::state::{AppState, ProgramCache};
use crate::types::{InstalledProgram, UninstallResult};

const LIST_SCRIPT: &str = r#"
$items = New-Object System.Collections.ArrayList
$sources = @(
  @{ Path='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'; Hive='HKLM64' },
  @{ Path='HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'; Hive='HKLM32' },
  @{ Path='HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'; Hive='HKCU' }
)
foreach ($s in $sources) {
  $props = Get-ItemProperty -Path $s.Path -ErrorAction SilentlyContinue
  foreach ($p in $props) {
    if (-not $p.DisplayName) { continue }
    if ($p.SystemComponent -eq 1) { continue }
    if ($p.ParentKeyName) { continue }
    if ($p.ReleaseType -match 'Update|Hotfix|Security') { continue }
    $size = 0
    if ($p.EstimatedSize) { $size = [double]$p.EstimatedSize * 1024 }
    $noRemove = $false
    if ($p.NoRemove -eq 1) { $noRemove = $true }
    $location = $null
    if ($p.InstallLocation) { $location = [string]$p.InstallLocation }
    $null = $items.Add([pscustomobject]@{
      key = ($s.Hive + '|' + $p.PSChildName)
      name = [string]$p.DisplayName
      version = if ($p.DisplayVersion) { [string]$p.DisplayVersion } else { $null }
      publisher = if ($p.Publisher) { [string]$p.Publisher } else { $null }
      installDate = if ($p.InstallDate) { [string]$p.InstallDate } else { $null }
      sizeBytes = $size
      location = $location
      uninstallString = if ($p.UninstallString) { [string]$p.UninstallString } else { $null }
      quietUninstallString = if ($p.QuietUninstallString) { [string]$p.QuietUninstallString } else { $null }
      noRemove = $noRemove
      displayIcon = if ($p.DisplayIcon) { [string]$p.DisplayIcon } else { $null }
    })
  }
}
ConvertTo-Json -InputObject @($items) -Depth 4 -Compress
"#;

const CACHE_TTL_MS: u128 = 30_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProgram {
    key: String,
    name: String,
    version: Option<String>,
    publisher: Option<String>,
    install_date: Option<String>,
    #[serde(default)]
    size_bytes: f64,
    location: Option<String>,
    uninstall_string: Option<String>,
    quiet_uninstall_string: Option<String>,
    #[serde(default)]
    no_remove: bool,
    display_icon: Option<String>,
}

/// Many MSI entries leave `InstallLocation` empty, so fall back to the folder
/// of the uninstaller/icon executable.
fn derive_location(program: &RawProgram) -> Option<String> {
    if let Some(location) = program.location.as_ref() {
        let trimmed = location.trim().trim_matches('"').trim().trim_end_matches(['\\', '/']);
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let candidates = [
        program.display_icon.as_deref(),
        program.uninstall_string.as_deref(),
        program.quiet_uninstall_string.as_deref(),
    ];

    for candidate in candidates.into_iter().flatten() {
        let cleaned = candidate
            .trim()
            .trim_end_matches(|c: char| c.is_ascii_digit() || c == ',' || c == ' ')
            .trim();
        let executable = extract_exe(cleaned)?;
        let normalized = executable.replace('/', "\\");
        if let Some(index) = normalized.rfind('\\') {
            if index <= 2 {
                let _ = index;
                continue;
            }
            let directory = &normalized[..index];
            if directory.to_lowercase().starts_with("c:\\windows") {
                continue;
            }
            return Some(directory.to_string());
        }
    }

    None
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
    if let Some(index) = lower.find(".exe") {
        let end = index + 4;
        let start = value[..end]
            .rfind(|c: char| c == ':' || c == '\\')
            .map(|pos| {
                // Keep the drive letter (e.g. `C:\`) intact.
                if value.as_bytes().get(pos + 1) == Some(&b'\\') {
                    pos.saturating_sub(1)
                } else {
                    pos + 1
                }
            })
            .unwrap_or(0);
        let candidate = value[start..end].trim();
        if candidate.contains(':') {
            return Some(candidate);
        }
    }
    None
}

fn load() -> Vec<InstalledProgram> {
    let data = ps::run_json::<Vec<RawProgram>>(LIST_SCRIPT, Duration::from_secs(90)).unwrap_or_default();
    let mut items: Vec<InstalledProgram> = data
        .into_iter()
        .filter(|item| !item.name.is_empty())
        .map(|item| InstalledProgram {
            key: item.key.clone(),
            name: item.name.clone(),
            version: item.version.clone(),
            publisher: item.publisher.clone(),
            install_date: item.install_date.clone(),
            size_bytes: item.size_bytes.max(0.0) as u64,
            location: derive_location(&item),
            uninstall_string: item.uninstall_string.clone(),
            quiet_uninstall_string: item.quiet_uninstall_string.clone(),
            no_remove: item.no_remove,
            display_icon: item.display_icon.clone(),
        })
        .collect();
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
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

    let items = load();
    let mut guard = shared.programs.lock().unwrap();
    *guard = Some(ProgramCache {
        at: std::time::Instant::now(),
        items: items.clone(),
    });
    items
}

fn find_guid(value: &str) -> Option<String> {
    let lower = value.to_lowercase();
    let bytes = lower.as_bytes();
    let mut start = None;
    for (index, ch) in bytes.iter().enumerate() {
        if *ch == b'{' {
            start = Some(index);
        } else if *ch == b'}' {
            if let Some(begin) = start {
                let candidate = &value[begin..=index];
                if candidate.len() == 38
                    && candidate[1..].chars().all(|c| c.is_ascii_hexdigit() || c == '-')
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
        let uninstall = self.uninstall_string.clone().or_else(|| self.quiet_uninstall_string.clone())?;
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        child.creation_flags(0);
    }
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

pub fn _touch(path: &Path) {
    let _ = path;
}
