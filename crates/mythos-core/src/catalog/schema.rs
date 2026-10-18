//! Device manifest schema definitions for MythOS Open Hardware Catalog.
//!
//! Conforms to MYTHOS-CAT-001 (JSON Schema Draft 2020-12).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level device manifest representing an open hardware device package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceManifest {
    pub schema_version: String,
    pub metadata: DeviceMetadata,
    pub physical_interface: PhysicalInterface,
    pub protocol: ProtocolConfig,
    #[serde(default)]
    pub discovery: Option<DiscoveryConfig>,
    pub registers: Vec<RegisterDefinition>,
    pub ui: UiConfig,
}

/// Metadata describing hardware identity, vendor, and revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMetadata {
    pub id: String,
    pub vendor: String,
    #[serde(default)]
    pub vendor_id: Option<String>,
    pub model: String,
    pub category: DeviceCategory,
    pub description: String,
    pub revision: String,
    #[serde(default)]
    pub certifications: Vec<String>,
}

/// Device classification categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceCategory {
    Sensor,
    Actuator,
    Drive,
    RemoteIo,
    Gateway,
    PowerMeter,
    Other,
}

/// Physical interface characteristics and terminal bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalInterface {
    pub connector_type: String,
    pub signaling: String,
    pub supply_voltage: SupplyVoltage,
    #[serde(default)]
    pub pinout: Vec<PinoutEntry>,
}

/// Operating supply voltage requirements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyVoltage {
    pub nominal_vdc: f64,
    pub min_vdc: f64,
    pub max_vdc: f64,
    pub power_watts: f64,
}

/// Physical connector pinout definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinoutEntry {
    pub pin: String,
    pub label: String,
    pub function: String,
}

/// Communication protocol specifications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolConfig {
    #[serde(rename = "type")]
    pub protocol_type: String,
    #[serde(default)]
    pub modbus_rtu: Option<ModbusRtuConfig>,
}

/// Modbus RTU communication settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModbusRtuConfig {
    pub default_slave_id: u8,
    pub default_baud_rate: u32,
    #[serde(default)]
    pub supported_baud_rates: Vec<u32>,
    pub default_framing: String,
    pub endianness: String,
    #[serde(default = "default_min_poll_interval_ms")]
    pub min_poll_interval_ms: u32,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u32,
}

fn default_min_poll_interval_ms() -> u32 {
    50
}

fn default_timeout_ms() -> u32 {
    150
}

/// Heuristic discovery and active bus fingerprinting parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Probe register address (can be number e.g. 40001 or hex string "0x0000")
    #[serde(deserialize_with = "deserialize_probe_register")]
    pub probe_register: u16,
    #[serde(default = "default_expected_fc")]
    pub expected_function_code: String,
    #[serde(default)]
    pub expected_fc: Option<String>,
    #[serde(default)]
    pub priority_baudrates: Vec<u32>,
    #[serde(default)]
    pub priority_parities: Vec<String>,
    #[serde(default = "default_id_range")]
    pub default_id_range: [u8; 2],
    #[serde(default = "default_probe_timeout_ms")]
    pub probe_timeout_ms: u64,
}

fn default_expected_fc() -> String {
    "0x03".to_string()
}

fn default_id_range() -> [u8; 2] {
    [1, 32]
}

fn default_probe_timeout_ms() -> u64 {
    150
}

fn deserialize_probe_register<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(num) => {
            if let Some(val) = num.as_u64() {
                // If 1-based PLC address e.g. 40001
                if (40001..=49999).contains(&val) {
                    Ok((val - 40001) as u16)
                } else if val <= 65535 {
                    Ok(val as u16)
                } else {
                    Err(D::Error::custom(format!("Register out of range: {}", val)))
                }
            } else {
                Err(D::Error::custom("Invalid number format for probe_register"))
            }
        }
        serde_json::Value::String(s) => {
            let s = s.trim();
            if let Some(stripped) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                u16::from_str_radix(stripped, 16)
                    .map_err(|e| D::Error::custom(format!("Invalid hex probe_register: {}", e)))
            } else {
                let val: u64 = s.parse().map_err(|e| {
                    D::Error::custom(format!("Invalid decimal string probe_register: {}", e))
                })?;
                if (40001..=49999).contains(&val) {
                    Ok((val - 40001) as u16)
                } else if val <= 65535 {
                    Ok(val as u16)
                } else {
                    Err(D::Error::custom(format!("Register out of range: {}", val)))
                }
            }
        }
        other => Err(D::Error::custom(format!(
            "Unexpected probe_register value: {:?}",
            other
        ))),
    }
}

/// Single Modbus register definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDefinition {
    pub name: String,
    pub tag_suffix: String,
    pub address_hex: String,
    pub address_dec: u16,
    pub function_code: String,
    pub data_type: DataType,
    pub access: AccessType,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub scaling: ScalingConfig,
    #[serde(default)]
    pub enum_mapping: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Industrial register data types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DataType {
    Bool,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float32,
    Bcd16,
    #[serde(rename = "RAW_BYTES")]
    RawBytes,
}

/// Access permission on registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AccessType {
    #[serde(rename = "RO")]
    ReadOnly,
    #[serde(rename = "WO")]
    WriteOnly,
    #[serde(rename = "RW")]
    ReadWrite,
}

/// Register value scaling and engineering transformation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ScalingConfig {
    #[default]
    None,
    Divisor {
        divisor: f64,
        #[serde(default)]
        offset: f64,
    },
    Linear {
        #[serde(default = "default_multiplier")]
        multiplier: f64,
        #[serde(default)]
        offset: f64,
        #[serde(default)]
        raw_min: Option<f64>,
        #[serde(default)]
        raw_max: Option<f64>,
        #[serde(default)]
        scaled_min: Option<f64>,
        #[serde(default)]
        scaled_max: Option<f64>,
    },
    Lookup,
}

fn default_multiplier() -> f64 {
    1.0
}

/// UI presentation attributes for topology canvas and inspector drawer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub icon_path: String,
    pub color_accent: String,
    pub rack_width_units: u32,
}
