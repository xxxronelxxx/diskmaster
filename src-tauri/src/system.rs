use std::time::Duration;

use chrono::{DateTime, Utc};
use sysinfo::{Disks, System};
use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::UI::Shell::IsUserAnAdmin;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use crate::types::{DriveInfo, SystemInfo};

fn reg_string(path: &str, name: &str) -> Option<String> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?;
    key.get_value::<String, _>(name).ok()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Volume label and file system for a drive root like `C:\`.
fn volume_info(root: &str) -> (String, String) {
    let root = wide(root);
    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    let mut serial: u32 = 0;
    let mut max_component: u32 = 0;
    let mut flags: u32 = 0;

    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            &mut max_component,
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };

    if ok == 0 {
        return (String::new(), String::new());
    }

    let read = |buffer: &[u16]| {
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(0);
        String::from_utf16_lossy(&buffer[..len])
    };
    (read(&label), read(&fs))
}

fn os_name() -> (String, String, String) {
    const KEY: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let mut product = reg_string(KEY, "ProductName").unwrap_or_else(|| "Windows".into());
    let version = reg_string(KEY, "DisplayVersion").unwrap_or_default();
    let build = reg_string(KEY, "CurrentBuild").unwrap_or_default();
    let ubr = reg_string(KEY, "UBR").unwrap_or_default();

    // Windows 11 still reports "Windows 10" in ProductName on some builds.
    if build
        .parse::<u32>()
        .map(|value| value >= 22000)
        .unwrap_or(false)
    {
        product = product.replace("Windows 10", "Windows 11");
    }

    let build = if ubr.is_empty() {
        build
    } else {
        format!("{build}.{ubr}")
    };
    (product, version, build)
}

pub fn drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut result = Vec::new();

    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        let normalized_mount = mount.replace('/', "\\");
        if normalized_mount.len() != 3 || !normalized_mount.ends_with(":\\") {
            continue;
        }
        let total = disk.total_space();
        if total == 0 {
            continue;
        }
        let free = disk.available_space();
        let (label, file_system) = volume_info(&normalized_mount);
        let file_system = if file_system.is_empty() {
            disk.file_system().to_string_lossy().to_string()
        } else {
            file_system
        };
        result.push(DriveInfo {
            letter: normalized_mount.trim_end_matches('\\').to_string(),
            label,
            file_system,
            total,
            free,
            used: total.saturating_sub(free),
        });
    }

    result.sort_by(|a, b| a.letter.cmp(&b.letter));
    result
}

pub fn collect() -> SystemInfo {
    let mut sys = System::new();
    sys.refresh_memory();

    let threads = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(0);
    let cores = sys
        .physical_core_count()
        .unwrap_or_else(|| (threads / 2).max(1));

    let (os, version, build) = os_name();
    let cpu = reg_string(
        "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
        "ProcessorNameString",
    )
    .unwrap_or_else(|| "Неизвестный процессор".into())
    .trim()
    .to_string();

    let uptime_ms = unsafe { GetTickCount64() };
    let last_boot: Option<String> = std::time::SystemTime::now()
        .checked_sub(Duration::from_millis(uptime_ms))
        .map(|time| {
            let date: DateTime<Utc> = time.into();
            date.to_rfc3339()
        });

    SystemInfo {
        machine: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "—".into()),
        user: std::env::var("USERNAME").unwrap_or_else(|_| "—".into()),
        os,
        build,
        version,
        last_boot,
        cpu,
        cores,
        threads,
        ram_total: sys.total_memory(),
        ram_free: sys.available_memory(),
        admin: unsafe { IsUserAnAdmin() } != 0,
        drives: drives(),
    }
}
