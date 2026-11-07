//! Windows driver package listing, backup and restore.
//!
//! The actual work is done by the in-box `pnputil.exe`, which is the supported
//! way to enumerate the DriverStore and export/import OEM driver packages.
//! Nothing is bundled into the executable; backups are ordinary folders.
//!
//! The module is shared by the main app and the elevated helper; each binary
//! uses a different subset, hence the module-wide dead-code allowance.
#![allow(dead_code)]

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_OEMCP};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_OUTPUT: usize = 4000;
const MAX_BACKUP_FILES: usize = 2000;
const MAX_RESTORE_ITEMS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DriverPackage {
    pub published_name: String,
    pub original_name: String,
    pub provider: String,
    pub class_name: String,
    pub class_guid: String,
    pub date: String,
    pub version: String,
    pub signer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverOpResult {
    pub name: String,
    pub ok: bool,
    pub code: Option<i32>,
    pub reboot: bool,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDriverFile {
    pub relative_path: String,
    pub size: u64,
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverBackupManifest {
    pub app_version: String,
    pub created_at: i64,
    pub packages: Vec<DriverPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverBackupScan {
    pub root: String,
    pub manifest: Option<DriverBackupManifest>,
    pub files: Vec<BackupDriverFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverExportResult {
    pub path: String,
    pub items: Vec<DriverOpResult>,
}

fn decode_oem(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let len = unsafe {
        MultiByteToWideChar(
            CP_OEMCP,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        )
    };
    if len <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut wide = vec![0u16; len as usize];
    let written = unsafe {
        MultiByteToWideChar(
            CP_OEMCP,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            wide.as_mut_ptr(),
            wide.len() as i32,
        )
    };
    if written <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    String::from_utf16_lossy(&wide[..written as usize])
}

/// Console output from pnputil is OEM-encoded; CSV data is usually plain ASCII.
fn decode_console(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.contains('\u{FFFD}') => text.to_string(),
        _ => decode_oem(bytes),
    }
}

/// Canonical paths on Windows use the `\\?\` prefix; pnputil and the UI
/// should see a normal drive path.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    text.to_string()
}

fn run_pnputil(args: &[&str]) -> Result<(i32, String), String> {
    let output = Command::new("pnputil")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("Не удалось запустить pnputil: {error}"))?;
    let mut text = decode_console(&output.stdout);
    text.push_str(&decode_console(&output.stderr));
    Ok((output.status.code().unwrap_or(-1), text.trim().to_string()))
}

fn limit_output(text: &str) -> String {
    if text.chars().count() <= MAX_OUTPUT {
        return text.to_string();
    }
    text.chars().take(MAX_OUTPUT).collect::<String>() + "\n…"
}

/// Runs pnputil and returns its raw CSV. Kept separate so the elevated helper
/// can use the same code when enumeration is denied to a normal process.
pub fn list_text() -> Result<String, String> {
    let (code, text) = run_pnputil(&["/enum-drivers", "/format", "csv"])?;
    if code != 0 {
        return Err(if text.is_empty() {
            format!("pnputil завершился с кодом {code}")
        } else {
            text
        });
    }
    Ok(text)
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '"' if quoted => {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            }
            '"' => quoted = true,
            ',' if !quoted => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(character),
        }
    }
    fields.push(current);
    fields
}

fn column(headers: &[String], name: &str) -> Option<usize> {
    headers.iter().position(|header| header == name)
}

fn cell(row: &[String], index: Option<usize>) -> String {
    index
        .and_then(|index| row.get(index))
        .cloned()
        .unwrap_or_default()
}

pub fn parse_list(text: &str) -> Result<Vec<DriverPackage>, String> {
    let mut lines = text.lines();
    let header_line = lines.next().ok_or("Пустой ответ pnputil")?;
    let headers = parse_csv_line(header_line.trim_start_matches('\u{feff}'));
    let published = column(&headers, "DriverName").ok_or("В CSV нет колонки DriverName")?;
    let original = column(&headers, "OriginalName");
    let provider = column(&headers, "ProviderName");
    let class_name = column(&headers, "ClassName");
    let class_guid = column(&headers, "ClassGuid");
    let driver_version = column(&headers, "DriverVersion");
    let signer = column(&headers, "SignerName");

    let mut packages = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let row = parse_csv_line(line);
        let published_name = cell(&row, Some(published));
        // pnputil may append a localized informational line after the CSV;
        // only real OEM package names are accepted.
        if !valid_inf_name(&published_name) {
            continue;
        }
        let (date, version) = cell(&row, driver_version)
            .split_once(' ')
            .map(|(date, version)| (date.to_string(), version.to_string()))
            .unwrap_or_else(|| (cell(&row, driver_version), String::new()));
        packages.push(DriverPackage {
            published_name,
            original_name: cell(&row, original),
            provider: cell(&row, provider),
            class_name: cell(&row, class_name),
            class_guid: cell(&row, class_guid),
            date,
            version,
            signer: cell(&row, signer),
        });
    }
    packages.sort_by(|left, right| {
        left.class_name
            .to_lowercase()
            .cmp(&right.class_name.to_lowercase())
            .then_with(|| {
                left.provider
                    .to_lowercase()
                    .cmp(&right.provider.to_lowercase())
            })
            .then_with(|| left.published_name.cmp(&right.published_name))
    });
    Ok(packages)
}

pub fn list() -> Result<Vec<DriverPackage>, String> {
    parse_list(&list_text()?)
}

fn forbidden_paths() -> Vec<String> {
    let mut roots = Vec::new();
    for value in [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ] {
        if let Ok(path) = std::env::var(value) {
            roots.push(
                path.trim_end_matches(['\\', '/'])
                    .replace('/', "\\")
                    .to_lowercase(),
            );
        }
    }
    roots
}

fn is_forbidden(path: &Path) -> bool {
    let display = display_path(path);
    if Path::new(&display).parent().is_none() {
        return true; // drive root
    }
    let text = display.replace('/', "\\").to_lowercase();
    forbidden_paths()
        .iter()
        .any(|root| !root.is_empty() && (text == *root || text.starts_with(&format!("{root}\\"))))
}

/// Creates `DiskMaster-Drivers-YYYY-MM-DD_HH-MM` inside the chosen parent.
pub fn prepare_backup_dir(parent: &str) -> Result<PathBuf, String> {
    let parent = PathBuf::from(parent.trim());
    if !parent.is_absolute() {
        return Err("Папка резервной копии должна быть абсолютным путём".into());
    }
    if is_forbidden(&parent) {
        return Err("Нельзя сохранять резервную копию в системную папку".into());
    }
    fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let parent = parent.canonicalize().map_err(|error| error.to_string())?;

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M");
    let mut target = parent.join(format!("DiskMaster-Drivers-{stamp}"));
    let mut suffix = 1;
    while target.exists() {
        target = parent.join(format!("DiskMaster-Drivers-{stamp}_{suffix}"));
        suffix += 1;
    }
    fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    Ok(target)
}

fn valid_inf_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if !(lower.starts_with("oem") && lower.ends_with(".inf")) {
        return false;
    }
    let middle = &lower[3..lower.len() - 4];
    !middle.is_empty()
        && middle.len() <= 12
        && middle.chars().all(|character| character.is_ascii_digit())
}

pub fn export(path: &Path, infs: &[String]) -> Result<Vec<DriverOpResult>, String> {
    if !path.is_absolute() {
        return Err("Папка экспорта должна быть абсолютным путём".into());
    }
    if is_forbidden(path) {
        return Err("Нельзя экспортировать в системную папку".into());
    }
    fs::create_dir_all(path).map_err(|error| error.to_string())?;
    let target = display_path(path);

    let names: Vec<String> = if infs.is_empty() {
        vec!["*".into()]
    } else {
        for name in infs {
            if !valid_inf_name(name) {
                return Err(format!("Некорректное имя драйвера: {name}"));
            }
        }
        infs.to_vec()
    };

    let mut results = Vec::new();
    for name in names {
        let (code, output) = run_pnputil(&["/export-driver", &name, &target])?;
        results.push(DriverOpResult {
            name,
            ok: code == 0,
            code: Some(code),
            reboot: false,
            output: limit_output(&output),
        });
    }
    Ok(results)
}

pub fn scan_backup(root: &str) -> Result<DriverBackupScan, String> {
    let root = PathBuf::from(root.trim());
    if !root.is_absolute() {
        return Err("Папка резервной копии должна быть абсолютным путём".into());
    }
    if !root.is_dir() {
        return Err("Папка резервной копии не найдена".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if is_forbidden(&root) {
        return Err("Системные папки не сканируются как резервная копия".into());
    }

    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if files.len() >= MAX_BACKUP_FILES {
                break;
            }
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if !metadata.is_file()
                || path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| !value.eq_ignore_ascii_case("inf"))
                    .unwrap_or(true)
            {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_millis() as i64);
            files.push(BackupDriverFile {
                relative_path: relative,
                size: metadata.len(),
                modified,
            });
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(DriverBackupScan {
        root: display_path(&root),
        manifest: read_manifest(&root),
        files,
    })
}

fn resolve_restore_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(relative);
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("Некорректный путь в резервной копии: {relative}"));
    }
    if candidate
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| !value.eq_ignore_ascii_case("inf"))
        .unwrap_or(true)
    {
        return Err(format!("Файл не является INF: {relative}"));
    }
    let path = root.join(candidate);
    let path = path
        .canonicalize()
        .map_err(|error| format!("{relative}: {error}"))?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(format!("Файл вне папки резервной копии: {relative}"));
    }
    Ok(path)
}

pub fn restore(root: &str, infs: &[String]) -> Result<Vec<DriverOpResult>, String> {
    let root = PathBuf::from(root.trim());
    if !root.is_absolute() || !root.is_dir() {
        return Err("Папка резервной копии не найдена".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if is_forbidden(&root) {
        return Err("Нельзя восстанавливать драйверы из системной папки".into());
    }
    if infs.is_empty() {
        return Err("Не выбраны INF-файлы для восстановления".into());
    }
    if infs.len() > MAX_RESTORE_ITEMS {
        return Err(format!(
            "Слишком много файлов: максимум {MAX_RESTORE_ITEMS}"
        ));
    }

    let mut results = Vec::new();
    for relative in infs {
        let path = match resolve_restore_file(&root, relative) {
            Ok(path) => path,
            Err(error) => {
                results.push(DriverOpResult {
                    name: relative.clone(),
                    ok: false,
                    code: None,
                    reboot: false,
                    output: error,
                });
                continue;
            }
        };
        let file = display_path(&path);
        let (code, output) = run_pnputil(&["/add-driver", &file, "/install"])?;
        let reboot = code == 3010 || code == 1641;
        results.push(DriverOpResult {
            name: relative.clone(),
            ok: code == 0 || reboot,
            code: Some(code),
            reboot,
            output: limit_output(&output),
        });
    }
    Ok(results)
}

pub fn write_manifest(dir: &Path, packages: Vec<DriverPackage>) -> Result<(), String> {
    let manifest = DriverBackupManifest {
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: chrono::Utc::now().timestamp_millis(),
        packages,
    };
    let raw = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    fs::write(dir.join("disk-master-drivers.json"), raw).map_err(|error| error.to_string())
}

pub fn read_manifest(dir: &Path) -> Option<DriverBackupManifest> {
    let raw = fs::read_to_string(dir.join("disk-master-drivers.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::{is_forbidden, parse_csv_line, parse_list, valid_inf_name};
    use std::path::Path;

    #[test]
    fn parses_quoted_csv() {
        assert_eq!(
            parse_csv_line(r#""oem1.inf","a,b.inf","ACME ""Ltd""",x"#),
            vec!["oem1.inf", "a,b.inf", "ACME \"Ltd\"", "x"]
        );
    }

    #[test]
    fn parses_pnputil_csv() {
        let text = "DriverName,OriginalName,ProviderName,ClassName,ClassGuid,ClassVersion,ExtensionId,DriverVersion,SignerName,CatalogAttributes,WhcpVersion\r\n\
\"oem1.inf\",\"video.inf\",\"ACME\",\"Display\",\"{guid}\",\"\",\"\",\"08/01/2024 1.2.3.4\",\"Microsoft Windows Hardware Compatibility Publisher\",\"Legacy\",\"Unknown\"\r\n";
        let packages = parse_list(text).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].published_name, "oem1.inf");
        assert_eq!(packages[0].date, "08/01/2024");
        assert_eq!(packages[0].version, "1.2.3.4");
    }

    #[test]
    fn ignores_non_driver_output_lines() {
        let text = concat!(
            "DriverName,OriginalName,ProviderName,ClassName,ClassGuid,ClassVersion,ExtensionId,DriverVersion,SignerName,CatalogAttributes,WhcpVersion\r\n",
            "Драйверы не найдены.\r\n"
        );
        assert!(parse_list(text).unwrap().is_empty());
    }

    #[test]
    fn validates_export_inf_names() {
        assert!(valid_inf_name("oem0.inf"));
        assert!(valid_inf_name("OEM123.INF"));
        assert!(!valid_inf_name("oem.inf"));
        assert!(!valid_inf_name("oem1.txt"));
        assert!(!valid_inf_name("..\\oem1.inf"));
        assert!(!valid_inf_name("oem1.inf\\x"));
    }

    #[test]
    fn rejects_drive_roots() {
        assert!(is_forbidden(Path::new("C:\\")));
    }

    #[test]
    fn strips_verbatim_prefix() {
        assert_eq!(
            super::display_path(Path::new(r"\\?\C:\Temp\x")),
            r"C:\Temp\x"
        );
    }

    #[test]
    fn scans_backup_and_rejects_traversal() {
        let root = std::env::temp_dir().join(format!("dm-drivers-test-{}", std::process::id()));
        let sub = root.join("oem1");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("oem1.inf"), b"test").unwrap();
        std::fs::write(sub.join("readme.txt"), b"x").unwrap();

        let scan = super::scan_backup(root.to_str().unwrap()).unwrap();
        assert_eq!(scan.files.len(), 1);
        assert_eq!(scan.files[0].relative_path, r"oem1\oem1.inf");

        let canonical = root.canonicalize().unwrap();
        assert!(super::resolve_restore_file(&canonical, r"oem1\oem1.inf").is_ok());
        assert!(super::resolve_restore_file(&canonical, r"..\oem1.inf").is_err());
        assert!(super::resolve_restore_file(&canonical, r"C:\oem1.inf").is_err());
        assert!(super::resolve_restore_file(&canonical, r"oem1\readme.txt").is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
