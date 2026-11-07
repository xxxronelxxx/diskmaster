//! Read-only SMART reader for NVMe and ATA drives.
//!
//! Windows requires administrator rights to open `\\.\PhysicalDriveN` and issue
//! the SMART IOCTLs. The main app asks the elevated helper when it is not already
//! running as administrator.

use std::ffi::c_void;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
const IOCTL_STORAGE_PREDICT_FAILURE: u32 = 0x002D_1100;
const STORAGE_DEVICE_PROPERTY: u32 = 0;
const STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY: u32 = 50;
const PROTOCOL_TYPE_NVME: u32 = 3;
const NVME_DATA_TYPE_LOG_PAGE: u32 = 2;
const NVME_LOG_PAGE_HEALTH: u32 = 0x02;
const NVME_HEALTH_LOG_SIZE: usize = 512;
const ATA_SMART_SIZE: usize = 512;
const PREDICT_FAILURE_SIZE: usize = 4 + ATA_SMART_SIZE;
const BUS_TYPE_NVME: u32 = 17;
const BUS_TYPE_USB: u32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmartProtocol {
    Nvme,
    Ata,
}

impl SmartProtocol {
    pub fn key(self) -> &'static str {
        match self {
            SmartProtocol::Nvme => "nvme",
            SmartProtocol::Ata => "ata",
        }
    }
}

/// Warning conditions reported by the drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmartWarning {
    AvailableSpare,
    Temperature,
    Reliability,
    ReadOnly,
    VolatileMemory,
    PersistentMemory,
    /// ATA SMART predicts a drive failure (`PredictFailure` from Windows).
    PredictFailure,
}

impl SmartWarning {
    #[allow(dead_code)]
    pub fn key(self) -> &'static str {
        match self {
            SmartWarning::AvailableSpare => "availableSpare",
            SmartWarning::Temperature => "temperature",
            SmartWarning::Reliability => "reliability",
            SmartWarning::ReadOnly => "readOnly",
            SmartWarning::VolatileMemory => "volatileMemory",
            SmartWarning::PersistentMemory => "persistentMemory",
            SmartWarning::PredictFailure => "predictFailure",
        }
    }

    fn from_mask(mask: u8) -> Vec<Self> {
        const BITS: [(u8, SmartWarning); 6] = [
            (1 << 0, SmartWarning::AvailableSpare),
            (1 << 1, SmartWarning::Temperature),
            (1 << 2, SmartWarning::Reliability),
            (1 << 3, SmartWarning::ReadOnly),
            (1 << 4, SmartWarning::VolatileMemory),
            (1 << 5, SmartWarning::PersistentMemory),
        ];
        BITS.into_iter()
            .filter(|(bit, _)| mask & bit != 0)
            .map(|(_, warning)| warning)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtaAttribute {
    pub id: u8,
    pub value: u8,
    pub worst: u8,
    pub raw: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartDisk {
    pub index: u32,
    pub model: String,
    pub serial: String,
    pub bus_type: u32,
    pub protocol: SmartProtocol,
    pub history_key: String,
    pub critical_warning: u8,
    pub warnings: Vec<SmartWarning>,
    pub health_passed: Option<bool>,
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
    pub attributes: Vec<AtaAttribute>,
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

#[derive(Default, Clone)]
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

fn query_ata_smart(handle: HANDLE) -> Option<AtaSmart> {
    let mut output = vec![0u8; PREDICT_FAILURE_SIZE];
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_PREDICT_FAILURE,
            std::ptr::null(),
            0,
            output.as_mut_ptr() as *mut c_void,
            output.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || returned < PREDICT_FAILURE_SIZE as u32 {
        return None;
    }
    let failure_predicted = u32::from_le_bytes(output[0..4].try_into().ok()?) != 0;
    parse_ata_smart(&output[4..], failure_predicted)
}

fn le_u48(bytes: &[u8]) -> u64 {
    let mut value = [0u8; 8];
    value[..6].copy_from_slice(&bytes[..6]);
    u64::from_le_bytes(value)
}

fn ata_checksum_ok(data: &[u8]) -> bool {
    data.len() == ATA_SMART_SIZE && data.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn attribute_value(attributes: &[AtaAttribute], id: u8) -> Option<u64> {
    attributes
        .iter()
        .find(|attribute| attribute.id == id)
        .map(|attribute| attribute.raw)
}

fn parse_ata_smart(bytes: &[u8], failure_predicted: bool) -> Option<AtaSmart> {
    if bytes.len() < ATA_SMART_SIZE {
        return None;
    }
    let data = &bytes[..ATA_SMART_SIZE];
    let revision = u16::from_le_bytes([data[0], data[1]]);
    // Unsupported bridges often return zeros or 0xFF for the whole page.
    if revision == 0 || revision == 0xFFFF || !ata_checksum_ok(data) {
        return None;
    }

    let mut attributes = Vec::new();
    for index in 0..30 {
        let offset = 2 + index * 12;
        let id = data[offset];
        if id == 0 || id == 0xFF {
            continue;
        }
        attributes.push(AtaAttribute {
            id,
            value: data[offset + 3],
            worst: data[offset + 4],
            raw: le_u48(&data[offset + 5..offset + 11]),
        });
    }
    if attributes.is_empty() {
        return None;
    }
    attributes.sort_by_key(|attribute| attribute.id);

    let temperature = [194u8, 190u8].into_iter().find_map(|id| {
        let current = (attribute_value(&attributes, id)? & 0xFF) as u32;
        (1..=120).contains(&current).then_some(current)
    });
    // SATA SSDs expose remaining life under different vendor IDs; normalized
    // values are 100 (new) to 0 (worn out).
    let wear = [231u8, 233, 202, 177, 179].into_iter().find_map(|id| {
        let value = attributes
            .iter()
            .find(|attribute| attribute.id == id)?
            .value;
        (value <= 100).then(|| 100u32 - u32::from(value))
    });
    let available_spare = attributes
        .iter()
        .find(|attribute| attribute.id == 232)
        .map(|attribute| attribute.value)
        .filter(|value| *value <= 100);
    let power_on_hours = attribute_value(&attributes, 9);
    let unsafe_shutdowns = attribute_value(&attributes, 174);
    let media_errors =
        attribute_value(&attributes, 187).or_else(|| attribute_value(&attributes, 198));

    Some(AtaSmart {
        health_passed: !failure_predicted,
        temperature,
        available_spare,
        wear,
        power_on_hours,
        unsafe_shutdowns,
        media_errors,
        attributes,
    })
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
        warnings: SmartWarning::from_mask(critical_warning),
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
    warnings: Vec<SmartWarning>,
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

struct AtaSmart {
    health_passed: bool,
    temperature: Option<u32>,
    available_spare: Option<u8>,
    wear: Option<u32>,
    power_on_hours: Option<u64>,
    unsafe_shutdowns: Option<u64>,
    media_errors: Option<u64>,
    attributes: Vec<AtaAttribute>,
}

fn history_key(protocol: SmartProtocol, serial: &str, model: &str, index: u32) -> String {
    let identity = if !serial.is_empty() {
        serial
    } else if !model.is_empty() {
        model
    } else {
        return format!("{}:{index}", protocol.key());
    };
    format!("{}:{identity}", protocol.key())
}

fn nvme_disk(index: u32, descriptor: &Descriptor, parsed: SmartDiskFields) -> SmartDisk {
    SmartDisk {
        index,
        model: descriptor.model.clone(),
        serial: descriptor.serial.clone(),
        bus_type: descriptor.bus_type,
        protocol: SmartProtocol::Nvme,
        history_key: history_key(
            SmartProtocol::Nvme,
            &descriptor.serial,
            &descriptor.model,
            index,
        ),
        critical_warning: parsed.critical_warning,
        warnings: parsed.warnings,
        health_passed: None,
        temperature: parsed.temperature,
        available_spare: Some(parsed.available_spare),
        spare_threshold: Some(parsed.spare_threshold),
        wear: Some(u32::from(parsed.wear)),
        power_on_hours: parsed.power_on_hours,
        unsafe_shutdowns: parsed.unsafe_shutdowns,
        media_errors: parsed.media_errors,
        error_log_entries: parsed.error_log_entries,
        data_read: parsed.data_read,
        data_written: parsed.data_written,
        attributes: Vec::new(),
    }
}

fn ata_disk(index: u32, descriptor: &Descriptor, ata: AtaSmart) -> SmartDisk {
    SmartDisk {
        index,
        model: descriptor.model.clone(),
        serial: descriptor.serial.clone(),
        bus_type: descriptor.bus_type,
        protocol: SmartProtocol::Ata,
        history_key: history_key(
            SmartProtocol::Ata,
            &descriptor.serial,
            &descriptor.model,
            index,
        ),
        critical_warning: 0,
        warnings: if ata.health_passed {
            Vec::new()
        } else {
            vec![SmartWarning::PredictFailure]
        },
        health_passed: Some(ata.health_passed),
        temperature: ata.temperature,
        available_spare: ata.available_spare,
        spare_threshold: None,
        wear: ata.wear,
        power_on_hours: ata.power_on_hours,
        unsafe_shutdowns: ata.unsafe_shutdowns,
        media_errors: ata.media_errors,
        error_log_entries: None,
        data_read: None,
        data_written: None,
        attributes: ata.attributes,
    }
}

pub fn read_all() -> Vec<SmartDisk> {
    read_all_inner(false)
}

/// Background checks skip USB bridges: many of them do not pass SMART through,
/// and opening an external drive every hour can spin it up for nothing.
#[allow(dead_code)]
pub fn read_all_excluding_usb() -> Vec<SmartDisk> {
    read_all_inner(true)
}

fn read_all_inner(skip_usb: bool) -> Vec<SmartDisk> {
    if !is_elevated() {
        return Vec::new();
    }

    let mut disks = Vec::new();
    for index in 0..32 {
        let Some(handle) = open_drive(index) else {
            continue;
        };
        let descriptor = query_descriptor(handle).unwrap_or_default();
        if skip_usb && descriptor.bus_type == BUS_TYPE_USB {
            unsafe { CloseHandle(handle) };
            continue;
        }
        let mut smart = None;

        // NVMe health log pages, or an unknown bus where the descriptor failed.
        if descriptor.bus_type == BUS_TYPE_NVME || descriptor.bus_type == 0 {
            if let Some(bytes) = query_nvme_health(handle) {
                smart = Some(nvme_disk(index, &descriptor, parse_nvme_health(&bytes)));
            }
        }
        // ATA/SATA SMART data, including drives behind USB bridges when supported.
        if smart.is_none() && descriptor.bus_type != BUS_TYPE_NVME {
            if let Some(ata) = query_ata_smart(handle) {
                smart = Some(ata_disk(index, &descriptor, ata));
            }
        }

        if let Some(disk) = smart {
            disks.push(disk);
        }
        unsafe { CloseHandle(handle) };
    }
    disks
}

#[cfg(test)]
mod tests {
    use super::{parse_ata_smart, parse_nvme_health, to_u64, AtaAttribute, SmartWarning};

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

    fn put_ata_attribute(
        bytes: &mut [u8; 512],
        index: usize,
        id: u8,
        value: u8,
        worst: u8,
        raw: u64,
    ) {
        let offset = 2 + index * 12;
        bytes[offset] = id;
        bytes[offset + 3] = value;
        bytes[offset + 4] = worst;
        bytes[offset + 5..offset + 11].copy_from_slice(&raw.to_le_bytes()[..6]);
    }

    fn ata_sample() -> [u8; 512] {
        let mut bytes = [0u8; 512];
        bytes[0] = 0x00;
        bytes[1] = 0x01; // revision 0x0100
        put_ata_attribute(&mut bytes, 0, 5, 100, 100, 0);
        put_ata_attribute(&mut bytes, 1, 9, 90, 90, 12_000);
        put_ata_attribute(&mut bytes, 2, 174, 100, 100, 7);
        put_ata_attribute(&mut bytes, 3, 187, 100, 100, 3);
        put_ata_attribute(&mut bytes, 4, 194, 60, 50, 40);
        put_ata_attribute(&mut bytes, 5, 197, 100, 100, 2);
        put_ata_attribute(&mut bytes, 6, 198, 100, 100, 1);
        put_ata_attribute(&mut bytes, 7, 199, 100, 100, 9);
        put_ata_attribute(&mut bytes, 8, 231, 97, 97, 0);
        put_ata_attribute(&mut bytes, 9, 232, 100, 100, 0);
        put_ata_attribute(&mut bytes, 10, 241, 100, 100, 123_456);
        let sum = bytes[..511]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        bytes[511] = 0u8.wrapping_sub(sum);
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
    fn decodes_critical_warning_bits() {
        let mut bytes = sample();
        bytes[0] = 0b0010_1101;
        assert_eq!(
            parse_nvme_health(&bytes).warnings,
            vec![
                SmartWarning::AvailableSpare,
                SmartWarning::Reliability,
                SmartWarning::ReadOnly,
                SmartWarning::PersistentMemory,
            ]
        );
        bytes[0] = 0;
        assert!(parse_nvme_health(&bytes).warnings.is_empty());
    }

    #[test]
    fn serializes_warning_names_for_the_ui() {
        assert_eq!(
            serde_json::to_string(&SmartWarning::AvailableSpare).unwrap(),
            "\"availableSpare\""
        );
        assert_eq!(
            serde_json::to_string(&SmartWarning::ReadOnly).unwrap(),
            "\"readOnly\""
        );
        assert_eq!(
            serde_json::to_string(&SmartWarning::PersistentMemory).unwrap(),
            "\"persistentMemory\""
        );
        assert_eq!(
            serde_json::to_string(&SmartWarning::PredictFailure).unwrap(),
            "\"predictFailure\""
        );
    }

    #[test]
    fn missing_temperature_is_none() {
        let mut bytes = [0u8; 512];
        bytes[1] = 0xFF;
        bytes[2] = 0xFF;
        assert_eq!(parse_nvme_health(&bytes).temperature, None);
    }

    #[test]
    fn parses_ata_smart_attributes() {
        let parsed = parse_ata_smart(&ata_sample(), false).expect("valid ATA page");
        assert!(parsed.health_passed);
        assert_eq!(parsed.temperature, Some(40));
        assert_eq!(parsed.wear, Some(3));
        assert_eq!(parsed.available_spare, Some(100));
        assert_eq!(parsed.power_on_hours, Some(12_000));
        assert_eq!(parsed.unsafe_shutdowns, Some(7));
        assert_eq!(parsed.media_errors, Some(3));
        assert_eq!(parsed.attributes.len(), 11);
        assert_eq!(
            parsed.attributes[0],
            AtaAttribute {
                id: 5,
                value: 100,
                worst: 100,
                raw: 0,
            }
        );
    }

    #[test]
    fn ata_checksum_and_empty_pages_are_rejected() {
        let mut bytes = ata_sample();
        bytes[100] = bytes[100].wrapping_add(1);
        assert!(parse_ata_smart(&bytes, false).is_none());
        assert!(parse_ata_smart(&[0u8; 512], false).is_none());
        assert!(parse_ata_smart(&[0xFFu8; 512], false).is_none());
    }

    #[test]
    fn ata_failure_prediction_is_reported() {
        let parsed = parse_ata_smart(&ata_sample(), true).expect("valid ATA page");
        assert!(!parsed.health_passed);
    }
}
