use serde::{Deserialize, Serialize};

use crate::elevated;
use crate::smart::{AtaAttribute, SmartProtocol, SmartWarning};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealth {
    pub device_id: String,
    pub name: String,
    pub media_type: Option<u16>,
    pub size: u64,
    pub health: Option<u16>,
    pub operational: Vec<u16>,
    pub serial: Option<String>,
    pub temperature: Option<u32>,
    pub wear: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub read_errors: Option<u64>,
    pub write_errors: Option<u64>,
    pub smart: bool,
    pub protocol: Option<SmartProtocol>,
    pub history_key: Option<String>,
    pub health_passed: Option<bool>,
    pub attributes: Vec<AtaAttribute>,
    pub bus_type: Option<u16>,
    pub warnings: Vec<SmartWarning>,
    pub data_read: Option<u64>,
    pub data_written: Option<u64>,
    pub media_errors: Option<u64>,
    pub error_log_entries: Option<u64>,
    pub critical_warning: Option<u8>,
    pub available_spare: Option<u8>,
    pub spare_threshold: Option<u8>,
    pub unsafe_shutdowns: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorInfo {
    pub name: String,
    pub sensor_type: String,
    pub value: f64,
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePoint {
    pub sequence: u32,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub kind: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceStatus {
    pub disks: Vec<DiskHealth>,
    pub sensors: Vec<SensorInfo>,
    pub restore_points: Vec<RestorePoint>,
    pub helper_available: bool,
    pub elevated: bool,
}

#[derive(Deserialize)]
struct RawDisk {
    #[serde(rename = "DeviceId")]
    device_id: Option<String>,
    #[serde(rename = "FriendlyName")]
    name: Option<String>,
    #[serde(rename = "MediaType")]
    media_type: Option<u16>,
    #[serde(rename = "Size")]
    size: Option<u64>,
    #[serde(rename = "HealthStatus")]
    health: Option<u16>,
    #[serde(rename = "OperationalStatus", default)]
    operational: Option<Vec<u16>>,
    #[serde(rename = "SerialNumber")]
    serial: Option<String>,
    #[serde(rename = "BusType")]
    bus_type: Option<u16>,
}

#[derive(Deserialize)]
struct RawReliability {
    #[serde(rename = "DeviceId")]
    device_id: Option<String>,
    #[serde(rename = "Temperature")]
    temperature: Option<u32>,
    #[serde(rename = "Wear")]
    wear: Option<u32>,
    #[serde(rename = "PowerOnHours")]
    power_on_hours: Option<u64>,
    #[serde(rename = "ReadErrorsTotal")]
    read_errors: Option<u64>,
    #[serde(rename = "WriteErrorsTotal")]
    write_errors: Option<u64>,
}

#[derive(Deserialize)]
struct RawSensor {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "SensorType")]
    sensor_type: String,
    #[serde(rename = "Value")]
    value: Option<f64>,
    #[serde(rename = "Identifier")]
    identifier: Option<String>,
}

#[derive(Deserialize)]
struct RawRestorePoint {
    #[serde(rename = "SequenceNumber")]
    sequence: u32,
    #[serde(rename = "Description")]
    description: Option<String>,
    #[serde(rename = "CreationTime")]
    created_at: Option<String>,
    #[serde(rename = "RestorePointType")]
    kind: Option<u32>,
}

fn connection(namespace: &str) -> Option<wmi::WMIConnection> {
    wmi::WMIConnection::with_namespace_path(namespace).ok()
}

fn disks() -> Vec<DiskHealth> {
    let smart_disks = crate::smart::read_all();
    crate::smart_history::record(&smart_disks);
    let Some(wmi) = connection("ROOT\\Microsoft\\Windows\\Storage") else {
        return Vec::new();
    };

    let raw: Vec<RawDisk> = wmi
        .raw_query(
            "SELECT DeviceId, FriendlyName, MediaType, Size, HealthStatus, OperationalStatus, SerialNumber, BusType FROM MSFT_PhysicalDisk",
        )
        .unwrap_or_default();

    let reliability: Vec<RawReliability> = wmi
        .raw_query(
            "SELECT DeviceId, Temperature, Wear, PowerOnHours, ReadErrorsTotal, WriteErrorsTotal FROM MSFT_StorageReliabilityCounter",
        )
        .unwrap_or_default();

    raw.into_iter()
        .map(|disk| {
            let device_id = disk.device_id.unwrap_or_default();
            let counter = reliability
                .iter()
                .find(|item| item.device_id.as_deref() == Some(device_id.as_str()));
            let smart = smart_disks.iter().find(|item| {
                item.index.to_string() == device_id
                    || (!item.serial.is_empty()
                        && disk.serial.as_deref() == Some(item.serial.as_str()))
            });

            DiskHealth {
                device_id,
                name: disk.name.unwrap_or_else(|| "Диск".into()),
                media_type: disk.media_type,
                size: disk.size.unwrap_or(0),
                health: disk.health,
                operational: disk.operational.unwrap_or_default(),
                serial: disk.serial,
                temperature: counter
                    .and_then(|item| item.temperature)
                    .or_else(|| smart.and_then(|item| item.temperature)),
                wear: counter
                    .and_then(|item| item.wear)
                    .or_else(|| smart.and_then(|item| item.wear)),
                power_on_hours: counter
                    .and_then(|item| item.power_on_hours)
                    .or_else(|| smart.and_then(|item| item.power_on_hours)),
                read_errors: counter.and_then(|item| item.read_errors),
                write_errors: counter.and_then(|item| item.write_errors),
                smart: smart.is_some(),
                protocol: smart.map(|item| item.protocol),
                history_key: smart.map(|item| item.history_key.clone()),
                health_passed: smart.and_then(|item| item.health_passed),
                attributes: smart
                    .map(|item| item.attributes.clone())
                    .unwrap_or_default(),
                bus_type: disk.bus_type,
                warnings: smart.map(|item| item.warnings.clone()).unwrap_or_default(),
                data_read: smart.and_then(|item| item.data_read),
                data_written: smart.and_then(|item| item.data_written),
                media_errors: smart.and_then(|item| item.media_errors),
                error_log_entries: smart.and_then(|item| item.error_log_entries),
                critical_warning: smart.map(|item| item.critical_warning),
                available_spare: smart.and_then(|item| item.available_spare),
                spare_threshold: smart.and_then(|item| item.spare_threshold),
                unsafe_shutdowns: smart.and_then(|item| item.unsafe_shutdowns),
            }
        })
        .collect()
}

fn sensors() -> Vec<SensorInfo> {
    let Some(wmi) = connection("ROOT\\LibreHardwareMonitor") else {
        return Vec::new();
    };

    let raw: Vec<RawSensor> = wmi
        .raw_query("SELECT Name, SensorType, Value, Identifier FROM Sensor")
        .unwrap_or_default();

    raw.into_iter()
        .filter(|sensor| {
            sensor.sensor_type.eq_ignore_ascii_case("Temperature")
                || sensor.sensor_type.eq_ignore_ascii_case("Fan")
        })
        .filter_map(|sensor| {
            Some(SensorInfo {
                name: sensor.name,
                sensor_type: sensor.sensor_type,
                value: sensor.value?,
                identifier: sensor.identifier,
            })
        })
        .collect()
}

fn restore_points() -> Vec<RestorePoint> {
    let Some(wmi) = connection("root\\default") else {
        return Vec::new();
    };

    let raw: Vec<RawRestorePoint> = wmi
        .raw_query(
            "SELECT SequenceNumber, Description, CreationTime, RestorePointType FROM SystemRestore",
        )
        .unwrap_or_default();

    raw.into_iter()
        .map(|point| RestorePoint {
            sequence: point.sequence,
            description: point.description,
            created_at: point
                .created_at
                .map(|raw| crate::wmi_time::to_iso(&raw).unwrap_or(raw)),
            kind: point.kind,
        })
        .collect()
}

pub fn status() -> MaintenanceStatus {
    MaintenanceStatus {
        disks: disks(),
        sensors: sensors(),
        restore_points: restore_points(),
        helper_available: elevated::helper_path().is_some(),
        elevated: crate::smart::is_elevated(),
    }
}

const ALLOWED_OPS: &[&str] = &[
    "windows_old",
    "dism_cleanup",
    "restore_point_create",
    "restore_points_list",
    "restore_point_delete",
    "clean_system",
];

pub fn run(operation: &str, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    if !ALLOWED_OPS.contains(&operation) {
        return Err("Недопустимая системная операция".into());
    }
    elevated::run(operation, payload)
}
