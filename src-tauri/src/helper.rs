//! Small elevated helper. It is launched with `runas` by the main app and
//! performs only a fixed set of operations; it never accepts shell commands.

use std::ffi::OsStr;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

mod smart;
mod wmi_time;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Input {
    #[serde(default)]
    payload: Value,
}

#[derive(Serialize)]
struct Output {
    ok: bool,
    message: String,
    data: Value,
}

fn arg(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    std::env::args().find_map(|value| value.strip_prefix(&prefix).map(str::to_string))
}

fn main() {
    let operation = arg("op").unwrap_or_default();
    let input_path = arg("input");
    let output_path = arg("output");

    let payload = input_path
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str::<Input>(&raw).ok())
        .map(|input| input.payload)
        .unwrap_or(Value::Null);

    let result = match operation.as_str() {
        "windows_old" => windows_old(),
        "dism_cleanup" => dism_cleanup(),
        "restore_point_create" => restore_point_create(),
        "restore_points_list" => restore_points_list(),
        "restore_point_delete" => restore_point_delete(&payload),
        "clean_system" => clean_system(&payload),
        "smart" => smart_result(),
        _ => Output {
            ok: false,
            message: format!("Неизвестная операция: {operation}"),
            data: Value::Null,
        },
    };

    let text = serde_json::to_string(&result).unwrap_or_else(|_| "{}".into());
    if let Some(path) = output_path {
        let _ = fs::write(path, text);
    }

    // Exit explicitly: COM/WMI teardown during process shutdown can block after
    // a successful query, which would leave the main app waiting forever.
    std::process::exit(if result.ok { 0 } else { 1 });
}

fn smart_result() -> Output {
    let disks = smart::read_all();
    ok(
        format!("SMART: дисков — {}", disks.len()),
        serde_json::to_value(disks).unwrap_or(Value::Null),
    )
}

fn ok(message: impl Into<String>, data: Value) -> Output {
    Output {
        ok: true,
        message: message.into(),
        data,
    }
}

fn fail(message: impl Into<String>) -> Output {
    Output {
        ok: false,
        message: message.into(),
        data: Value::Null,
    }
}

fn size_of(path: &Path) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                files += 1;
                bytes += metadata.len();
            }
        }
    }
    (files, bytes)
}

fn windows_old() -> Output {
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    // `SystemRoot` is `C:\Windows`; Windows.old lives in the drive root.
    let target = root
        .parent()
        .map(|drive| drive.join("Windows.old"))
        .unwrap_or_else(|| root.join("Windows.old"));

    if !target.exists() {
        return ok(
            "Папка Windows.old не найдена",
            json!({ "bytes": 0, "files": 0 }),
        );
    }

    let (files, bytes) = size_of(&target);
    match fs::remove_dir_all(&target) {
        Ok(()) => ok(
            format!("Windows.old удалена: {} МБ", bytes / 1024 / 1024),
            json!({ "bytes": bytes, "files": files }),
        ),
        Err(error) => fail(format!("Не удалось удалить Windows.old: {error}")),
    }
}

/// DISM writes its console output in the OEM code page, not UTF-8.
fn decode_oem(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_OEMCP};

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
            len,
        )
    };
    if written <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    wide.truncate(written as usize);
    String::from_utf16_lossy(&wide)
}

fn dism_cleanup() -> Output {
    let system = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let output = Command::new(system.join("System32").join("dism.exe"))
        .args(["/Online", "/Cleanup-Image", "/StartComponentCleanup"])
        .creation_flags(0x0800_0000)
        .output();

    match output {
        Ok(result) => {
            let text = format!(
                "{}{}",
                decode_oem(&result.stdout),
                decode_oem(&result.stderr)
            );
            let normalized = text.replace('\r', "\n");
            let tail: String = normalized
                .lines()
                .filter(|line| !line.trim().is_empty())
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            if result.status.success() {
                ok("Очистка WinSxS завершена", json!({ "output": tail }))
            } else {
                fail(format!("DISM завершился с ошибкой. {tail}"))
            }
        }
        Err(error) => fail(format!("Не удалось запустить DISM: {error}")),
    }
}

fn restore_point_create() -> Output {
    use windows_sys::Win32::System::Restore::{
        SRSetRestorePointW, BEGIN_SYSTEM_CHANGE, MODIFY_SETTINGS, RESTOREPOINTINFOW, STATEMGRSTATUS,
    };

    let mut description = [0u16; 256];
    for (index, value) in "Диск-Мастер: обслуживание системы"
        .encode_utf16()
        .take(255)
        .enumerate()
    {
        description[index] = value;
    }

    let info = RESTOREPOINTINFOW {
        dwEventType: BEGIN_SYSTEM_CHANGE,
        dwRestorePtType: MODIFY_SETTINGS,
        llSequenceNumber: 0,
        szDescription: description,
    };
    let mut status: STATEMGRSTATUS = unsafe { std::mem::zeroed() };

    let result = unsafe { SRSetRestorePointW(&info, &mut status) };
    let code = unsafe { std::ptr::addr_of!(status.nStatus).read_unaligned() };
    let sequence = unsafe { std::ptr::addr_of!(status.llSequenceNumber).read_unaligned() };
    if result != 0 && code == 0 {
        ok(
            "Точка восстановления создана",
            json!({ "sequence": sequence }),
        )
    } else {
        fail(format!(
            "Не удалось создать точку восстановления (код 0x{code:X})"
        ))
    }
}

fn wmi_connection(namespace: &str) -> Result<wmi::WMIConnection, String> {
    wmi::WMIConnection::with_namespace_path(namespace).map_err(|error| format!("WMI: {error}"))
}

fn restore_points_list() -> Output {
    #[derive(Deserialize)]
    struct Point {
        #[serde(rename = "SequenceNumber")]
        sequence: u32,
        #[serde(rename = "Description")]
        description: Option<String>,
        #[serde(rename = "CreationTime")]
        creation: Option<String>,
        #[serde(rename = "RestorePointType")]
        kind: Option<u32>,
    }

    let connection = match wmi_connection("root\\default") {
        Ok(connection) => connection,
        Err(error) => return fail(error),
    };

    let points: Vec<Point> = match connection.raw_query(
        "SELECT SequenceNumber, Description, CreationTime, RestorePointType FROM SystemRestore",
    ) {
        Ok(points) => points,
        Err(error) => {
            return fail(format!(
                "Не удалось прочитать точки восстановления: {error}"
            ))
        }
    };

    let data: Vec<Value> = points
        .into_iter()
        .map(|point| {
            json!({
                "sequence": point.sequence,
                "description": point.description,
                "createdAt": point.creation.map(|raw| wmi_time::to_iso(&raw).unwrap_or(raw)),
                "kind": point.kind,
            })
        })
        .collect();

    ok("Список точек восстановления получен", json!(data))
}

fn restore_point_delete(payload: &Value) -> Output {
    use windows_sys::Win32::System::Restore::SRRemoveRestorePoint;

    let Some(sequence) = payload.get("sequence").and_then(|value| value.as_u64()) else {
        return fail("Не указан номер точки восстановления");
    };

    let result = unsafe { SRRemoveRestorePoint(sequence as u32) };
    if result == 0 {
        ok(
            "Точка восстановления удалена",
            json!({ "sequence": sequence }),
        )
    } else {
        fail(format!(
            "Не удалось удалить точку восстановления (код {result})"
        ))
    }
}

fn clean_system(payload: &Value) -> Output {
    let categories: Vec<String> = payload
        .get("categories")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let system = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"));

    let mut targets: Vec<(&str, &str, Vec<PathBuf>)> = Vec::new();
    for id in &categories {
        match id.as_str() {
            "win-temp" => targets.push(("win-temp", "Temp Windows", vec![system.join("Temp")])),
            "win-update" => targets.push((
                "win-update",
                "Кэш обновлений",
                vec![system.join("SoftwareDistribution").join("Download")],
            )),
            "delivery-opt" => targets.push((
                "delivery-opt",
                "Оптимизация доставки",
                vec![system
                    .join("ServiceProfiles")
                    .join("NetworkService")
                    .join("AppData")
                    .join("Local")
                    .join("Microsoft")
                    .join("Windows")
                    .join("DeliveryOptimization")],
            )),
            "prefetch" => targets.push(("prefetch", "Prefetch", vec![system.join("Prefetch")])),
            "win-logs" => targets.push(("win-logs", "Логи Windows", vec![system.join("Logs")])),
            _ => {}
        }
    }

    let mut results = Vec::new();
    for (id, name, paths) in targets {
        let mut bytes = 0u64;
        let mut files = 0u64;
        let mut errors = Vec::new();

        for path in paths {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let full = entry.path();
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if metadata.is_dir() {
                    let (count, size) = size_of(&full);
                    if fs::remove_dir_all(&full).is_ok() {
                        bytes += size;
                        files += count;
                    }
                } else if fs::remove_file(&full).is_ok() {
                    bytes += metadata.len();
                    files += 1;
                } else if errors.len() < 5 {
                    errors.push(entry.file_name().to_string_lossy().to_string());
                }
            }
        }

        results.push(json!({
            "id": id,
            "name": name,
            "bytes": bytes,
            "files": files,
            "errors": errors,
        }));
    }

    ok("Системные категории очищены", json!(results))
}

/// Keeps the helper from being treated as a console app.
fn _hide_console() {
    let _ = OsStr::new("");
    let _ = Instant::now();
    let _ = Duration::from_secs(0);
}

#[cfg(test)]
mod tests {
    use super::decode_oem;

    #[test]
    fn decodes_ascii() {
        assert_eq!(decode_oem(b"DISM OK"), "DISM OK");
    }

    #[test]
    fn decodes_single_byte_characters() {
        // 0x8E is a single character in every OEM code page (cp866, cp437, ...).
        assert_eq!(decode_oem(&[0x8E]).chars().count(), 1);
    }

    #[test]
    fn falls_back_on_empty_input() {
        assert_eq!(decode_oem(&[]), "");
    }
}
