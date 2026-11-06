//! Read-only NVMe SMART/health reader.
//!
//! Windows requires administrator rights to open `\\.\PhysicalDriveN` and issue
//! `IOCTL_STORAGE_QUERY_PROPERTY` with the NVMe health log page. The main app
//! asks the elevated helper when it is not already running as administrator.

use std::ffi::c_void;

use serde::Serialize;
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
const STORAGE_DEVICE_PROPERTY: u32 = 0;
const STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY: u32 = 50;
const PROTOCOL_TYPE_NVME: u32 = 3;
const NVME_DATA_TYPE_LOG_PAGE: u32 = 2;
const NVME_LOG_PAGE_HEALTH: u32 = 0x02;
const NVME_HEALTH_LOG_SIZE: usize = 512;
const BUS_TYPE_NVME: u32 = 17;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartDisk {
    pub index: u32,
    pub model: String,
    pub serial: String,
    pub bus_type: u32,
    pub critical_warning: u8,
    pub temperature: Option<u32>,
    pub available_spare: Option<u8>,
    pub spare_threshold: Option<u8>,
    pub wear: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub media_errors: Option<u64>,
    pub error_log_entries: Option<u64>,
    pub data_read: Option<u64>,
    pub data_written: Option<u64>,
}

pub fn is_elevated() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_drive(index: u32) -> Option<HANDLE> {
    let path = format!("\\\\.\\PhysicalDrive{index}");
    let wide_path = wide(&path);
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        None
    } else {
        Some(handle)
    }
}

fn read_c_string(buffer: &[u8], offset: u32) -> Option<String> {
    if offset == 0 || offset as usize >= buffer.len() {
        return None;
    }
    let start = offset as usize;
    let end = buffer[start..]
        .iter()
        .position(|&byte| byte == 0)
        .map(|index| start + index)
        .unwrap_or(buffer.len());
    Some(
        String::from_utf8_lossy(&buffer[start..end])
            .trim()
            .to_string(),
    )
}

#[derive(Default)]
struct Descriptor {
    model: String,
    serial: String,
    bus_type: u32,
}

fn query_descriptor(handle: HANDLE) -> Option<Descriptor> {
    #[repr(C)]
    struct Query {
        property_id: u32,
        query_type: u32,
        additional: [u8; 4],
    }

    let query = Query {
        property_id: STORAGE_DEVICE_PROPERTY,
        query_type: 0,
        additional: [0; 4],
    };
    let mut output = vec![0u8; 1024];
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const c_void,
            std::mem::size_of::<Query>() as u32,
            output.as_mut_ptr() as *mut c_void,
            output.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || returned < 36 {
        return None;
    }

    let vendor = read_c_string(&output, u32::from_le_bytes(output[12..16].try_into().ok()?));
    let product = read_c_string(&output, u32::from_le_bytes(output[16..20].try_into().ok()?));
    let serial = read_c_string(&output, u32::from_le_bytes(output[24..28].try_into().ok()?));
    let bus_type = u32::from_le_bytes(output[28..32].try_into().ok()?);

    let model = [vendor, product]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Some(Descriptor {
        model,
        serial: serial.unwrap_or_default(),
        bus_type,
    })
}

fn query_nvme_health(handle: HANDLE) -> Option<Vec<u8>> {
    // The driver expects the protocol data immediately after the 8-byte query
    // header, not after `sizeof(STORAGE_PROPERTY_QUERY)` (which is 12 due to
    // padding). Verified on Windows 11 with the PHISON NVMe drive.
    let protocol_offset = 8usize;
    let mut input = vec![0u8; protocol_offset + 40];
    let put = |buffer: &mut [u8], offset: usize, value: u32| {
        buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    };
    put(&mut input, 0, STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY);
    put(&mut input, 4, 0);
    put(&mut input, protocol_offset, PROTOCOL_TYPE_NVME);
    put(&mut input, protocol_offset + 4, NVME_DATA_TYPE_LOG_PAGE);
    put(&mut input, protocol_offset + 8, NVME_LOG_PAGE_HEALTH);
    put(&mut input, protocol_offset + 12, 0);
    put(&mut input, protocol_offset + 16, 40);
    put(
        &mut input,
        protocol_offset + 20,
        NVME_HEALTH_LOG_SIZE as u32,
    );

    let mut output = vec![0u8; 4096];
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            input.as_ptr() as *const c_void,
            input.len() as u32,
            output.as_mut_ptr() as *mut c_void,
            output.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || returned < 48 {
        return None;
    }

    let data_offset = u32::from_le_bytes(output[24..28].try_into().ok()?) as usize;
    let data_length = u32::from_le_bytes(output[28..32].try_into().ok()?) as usize;
    let start = 8 + data_offset;
    if data_length < NVME_HEALTH_LOG_SIZE || start + NVME_HEALTH_LOG_SIZE > returned as usize {
        return None;
    }
    Some(output[start..start + NVME_HEALTH_LOG_SIZE].to_vec())
}

fn le_u128(bytes: &[u8]) -> u128 {
    let mut value = [0u8; 16];
    value.copy_from_slice(&bytes[..16]);
    u128::from_le_bytes(value)
}

fn to_u64(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn parse_nvme_health(bytes: &[u8]) -> SmartDiskFields {
    let critical_warning = bytes[0];
    let kelvin = u16::from_le_bytes([bytes[1], bytes[2]]);
    let temperature = if kelvin == 0 || kelvin == 0xFFFF {
        None
    } else {
        Some((kelvin.saturating_sub(273)) as u32)
    };
    let available_spare = bytes[3];
    let spare_threshold = bytes[4];
    let wear = bytes[5];

    SmartDiskFields {
        critical_warning,
        temperature,
        available_spare,
        spare_threshold,
        wear,
        power_on_hours: Some(to_u64(le_u128(&bytes[128..144]))),
        unsafe_shutdowns: Some(to_u64(le_u128(&bytes[144..160]))),
        media_errors: Some(to_u64(le_u128(&bytes[160..176]))),
        error_log_entries: Some(to_u64(le_u128(&bytes[176..192]))),
        data_read: Some(to_u64(le_u128(&bytes[32..48]).saturating_mul(1000 * 512))),
        data_written: Some(to_u64(le_u128(&bytes[48..64]).saturating_mul(1000 * 512))),
    }
}

struct SmartDiskFields {
    critical_warning: u8,
    temperature: Option<u32>,
    available_spare: u8,
    spare_threshold: u8,
    wear: u8,
    power_on_hours: Option<u64>,
    unsafe_shutdowns: Option<u64>,
    media_errors: Option<u64>,
    error_log_entries: Option<u64>,
    data_read: Option<u64>,
    data_written: Option<u64>,
}

pub fn read_all() -> Vec<SmartDisk> {
    if !is_elevated() {
        return Vec::new();
    }

    let mut disks = Vec::new();
    for index in 0..32 {
        let Some(handle) = open_drive(index) else {
            continue;
        };
        let descriptor = query_descriptor(handle);
        // Health log pages exist only for NVMe; skip other buses instead of
        // issuing an IOCTL that cannot succeed. If the descriptor is
        // unavailable, try anyway rather than hiding a working drive.
        let is_nvme = descriptor
            .as_ref()
            .map(|item| item.bus_type == BUS_TYPE_NVME)
            .unwrap_or(true);
        let health = if is_nvme {
            query_nvme_health(handle)
        } else {
            None
        };
        let descriptor = descriptor.unwrap_or_default();
        if let Some(bytes) = health {
            let parsed = parse_nvme_health(&bytes);
            disks.push(SmartDisk {
                index,
                model: descriptor.model,
                serial: descriptor.serial,
                bus_type: descriptor.bus_type,
                critical_warning: parsed.critical_warning,
                temperature: parsed.temperature,
                available_spare: Some(parsed.available_spare),
                spare_threshold: Some(parsed.spare_threshold),
                wear: Some(parsed.wear as u32),
                power_on_hours: parsed.power_on_hours,
                unsafe_shutdowns: parsed.unsafe_shutdowns,
                media_errors: parsed.media_errors,
                error_log_entries: parsed.error_log_entries,
                data_read: parsed.data_read,
                data_written: parsed.data_written,
            });
        }
        unsafe { CloseHandle(handle) };
    }
    disks
}

#[cfg(test)]
mod tests {
    use super::{parse_nvme_health, to_u64};

    fn sample() -> [u8; 512] {
        let mut bytes = [0u8; 512];
        bytes[0] = 1; // critical warning
        bytes[1..3].copy_from_slice(&324u16.to_le_bytes()); // 51 C
        bytes[3] = 100; // spare
        bytes[4] = 50; // spare threshold
        bytes[5] = 3; // percentage used
        bytes[32..48].copy_from_slice(&1234u128.to_le_bytes());
        bytes[48..64].copy_from_slice(&5678u128.to_le_bytes());
        bytes[128..144].copy_from_slice(&12290u128.to_le_bytes());
        bytes[144..160].copy_from_slice(&39u128.to_le_bytes());
        bytes[160..176].copy_from_slice(&7u128.to_le_bytes());
        bytes[176..192].copy_from_slice(&2u128.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_nvme_health_page() {
        let parsed = parse_nvme_health(&sample());
        assert_eq!(parsed.critical_warning, 1);
        assert_eq!(parsed.temperature, Some(51));
        assert_eq!(parsed.available_spare, 100);
        assert_eq!(parsed.spare_threshold, 50);
        assert_eq!(parsed.wear, 3);
        assert_eq!(parsed.power_on_hours, Some(12290));
        assert_eq!(parsed.unsafe_shutdowns, Some(39));
        assert_eq!(parsed.media_errors, Some(7));
        assert_eq!(parsed.error_log_entries, Some(2));
        assert_eq!(parsed.data_read, Some(1234u64 * 1000 * 512));
        assert_eq!(parsed.data_written, Some(5678u64 * 1000 * 512));
    }

    #[test]
    fn converts_large_counters() {
        assert_eq!(to_u64(u128::MAX), u64::MAX);
        assert_eq!(to_u64(42), 42);
    }

    #[test]
    fn missing_temperature_is_none() {
        let mut bytes = [0u8; 512];
        bytes[1] = 0xFF;
        bytes[2] = 0xFF;
        assert_eq!(parse_nvme_health(&bytes).temperature, None);
    }
}
