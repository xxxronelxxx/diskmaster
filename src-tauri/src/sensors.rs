//! Sensor module: a small managed host (`DiskMasterSensors.exe`) backed by
//! LibreHardwareMonitor. It runs elevated on demand and writes snapshots that
//! the main app reads without needing any privileges itself.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use crate::elevated;
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorReading {
    pub kind: String,
    pub name: String,
    pub hardware: String,
    pub hardware_type: String,
    pub value: f64,
    pub identifier: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorsState {
    pub available: bool,
    pub running: bool,
    pub admin: bool,
    pub pawnio_installed: bool,
    pub pawnio_version: Option<String>,
    pub source: String,
    pub updated_at: Option<String>,
    pub age_seconds: Option<u64>,
    pub sensors: Vec<SensorReading>,
    pub error: Option<String>,
    pub needs_reboot: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    #[serde(default)]
    ok: bool,
    generated_at: Option<String>,
    #[serde(default)]
    admin: bool,
    #[serde(default)]
    pawnio: bool,
    #[serde(default)]
    sensors: Vec<SensorReading>,
    error: Option<String>,
    parent: Option<u32>,
}

pub fn exe_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.join("sensors").join("DiskMasterSensors.exe"),
        dir.join("resources")
            .join("sensors")
            .join("DiskMasterSensors.exe"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn setup_path() -> Option<PathBuf> {
    exe_path()
        .and_then(|path| path.parent().map(|dir| dir.join("PawnIO_setup.exe")))
        .filter(|path| path.exists())
}

fn data_dir() -> PathBuf {
    paths::storage_path(&["sensors"])
}

fn snapshot_path() -> PathBuf {
    data_dir().join("sensors.json")
}

fn stop_path() -> PathBuf {
    data_dir().join("sensors.stop")
}

fn install_result_path() -> PathBuf {
    data_dir().join("install-result.json")
}

fn read_snapshot() -> Option<(Snapshot, u64)> {
    let path = snapshot_path();
    let metadata = fs::metadata(&path).ok()?;
    let age = metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|elapsed| elapsed.as_secs())?;
    let raw = fs::read_to_string(&path).ok()?;
    let snapshot = serde_json::from_str::<Snapshot>(&raw).ok()?;
    Some((snapshot, age))
}

fn pawnio_version() -> Option<String> {
    let candidates = [
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PawnIO",
        "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PawnIO",
    ];
    for path in candidates {
        let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(path, KEY_READ)
        else {
            continue;
        };
        if let Ok(value) = key.get_value::<String, _>("DisplayVersion") {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn is_admin() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn set_last_error(message: Option<String>) {
    *LAST_ERROR.lock().unwrap() = message;
}

pub fn state() -> SensorsState {
    let available = exe_path().is_some();
    let pawnio = pawnio_version();
    let snapshot = read_snapshot();
    let fresh = snapshot
        .as_ref()
        .map(|(snapshot, age)| {
            *age < 12
                && snapshot
                    .parent
                    .map(|parent| parent == std::process::id())
                    .unwrap_or(true)
        })
        .unwrap_or(false);

    let mut sensors = Vec::new();
    let mut error = None;
    let mut updated_at = None;
    let mut age_seconds = None;
    let mut source = "none".to_string();
    let mut snapshot_admin = false;
    let mut snapshot_pawnio = false;

    if fresh {
        if let Some((snapshot, age)) = snapshot {
            if snapshot.ok {
                source = "service".into();
            } else {
                error = snapshot.error.clone();
            }
            snapshot_admin = snapshot.admin;
            snapshot_pawnio = snapshot.pawnio;
            sensors = snapshot.sensors;
            updated_at = snapshot.generated_at;
            age_seconds = Some(age);
        }
    }

    if !fresh {
        error = LAST_ERROR.lock().unwrap().clone();
    }

    SensorsState {
        available,
        running: fresh && error.is_none(),
        admin: if fresh { snapshot_admin } else { is_admin() },
        pawnio_installed: pawnio.is_some() || snapshot_pawnio,
        pawnio_version: pawnio,
        source,
        updated_at,
        age_seconds,
        sensors,
        error,
        needs_reboot: false,
    }
}

fn wait_for_snapshot(timeout: Duration) -> Option<SensorsState> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(250));
        let current = state();
        if current.running {
            return Some(current);
        }
        if let Some(error) = current.error.clone() {
            return Some(SensorsState {
                running: false,
                error: Some(error),
                ..current
            });
        }
    }
    None
}

pub fn start() -> Result<SensorsState, String> {
    let exe = exe_path().ok_or_else(|| {
        "Модуль датчиков не найден. Пересоберите приложение (pnpm build:portable)".to_string()
    })?;
    if state().running {
        return Ok(state());
    }

    fs::create_dir_all(data_dir()).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(stop_path());
    let _ = fs::remove_file(snapshot_path());
    set_last_error(None);

    let parameters = format!(
        "--serve --out \"{}\" --stop \"{}\" --parent {}",
        snapshot_path().display(),
        stop_path().display(),
        std::process::id()
    );
    if let Err(error) = elevated::launch_detached(&exe, &parameters) {
        set_last_error(Some(error.clone()));
        return Err(error);
    }

    match wait_for_snapshot(Duration::from_secs(15)) {
        Some(state) => {
            set_last_error(None);
            Ok(state)
        }
        None => {
            let message = "Датчики не запустились за 15 секунд".to_string();
            set_last_error(Some(message.clone()));
            Err(message)
        }
    }
}

/// Installs PawnIO from the bundled signed installer and immediately starts the
/// sensor service in the same elevated process (one UAC prompt).
pub fn install() -> Result<SensorsState, String> {
    let exe = exe_path().ok_or_else(|| "Модуль датчиков не найден".to_string())?;
    let setup = setup_path().ok_or_else(|| {
        "PawnIO_setup.exe не найден в сборке. Пересоберите приложение.".to_string()
    })?;

    fs::create_dir_all(data_dir()).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(stop_path());
    let _ = fs::remove_file(snapshot_path());
    let _ = fs::remove_file(install_result_path());
    set_last_error(None);

    let parameters = format!(
        "--install-pawnio --serve --setup \"{}\" --install-out \"{}\" --out \"{}\" --stop \"{}\" --parent {}",
        setup.display(),
        install_result_path().display(),
        snapshot_path().display(),
        stop_path().display(),
        std::process::id()
    );
    if let Err(error) = elevated::launch_detached(&exe, &parameters) {
        set_last_error(Some(error.clone()));
        return Err(error);
    }

    let result = match wait_for_snapshot(Duration::from_secs(180)) {
        Some(state) => state,
        None => {
            let message = "Установка не завершилась за 3 минуты".to_string();
            set_last_error(Some(message.clone()));
            return Err(message);
        }
    };

    let install = fs::read_to_string(install_result_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    if let Some(install) = install {
        let ok = install
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let reboot = install
            .get("reboot")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if !ok {
            let message = install
                .get("error")
                .and_then(|value| value.as_str())
                .unwrap_or("Не удалось установить PawnIO");
            set_last_error(Some(message.to_string()));
            return Err(message.to_string());
        }
        let mut state = result;
        state.needs_reboot = reboot;
        set_last_error(None);
        return Ok(state);
    }

    Ok(result)
}

pub fn stop() -> SensorsState {
    set_last_error(None);
    let path = stop_path();
    paths::ensure_parent(&path);
    let _ = fs::write(&path, b"stop");
    std::thread::sleep(Duration::from_millis(400));
    state()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_path_is_inside_storage() {
        let path = snapshot_path();
        assert!(path.ends_with("sensors\\sensors.json") || path.ends_with("sensors/sensors.json"));
    }

    #[test]
    fn missing_snapshot_is_not_running() {
        let state = state();
        assert!(!state.running || state.source == "service");
    }
}
