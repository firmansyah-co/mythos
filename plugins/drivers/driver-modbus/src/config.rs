//! Modbus driver configuration models and register mappings.

use mythos_plugin_sdk::{PluginError, SampleValue};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Transport type: TCP or RTU.
///
/// `Rtu` is accepted by the parser so that an RTU configuration is rejected with a clear
/// message rather than an "unknown variant" error. There is no serial transport: see
/// [`ModbusDriverConfig::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModbusTransport {
    Tcp,
    Rtu,
}

/// How a mapped value is laid out in the device's 16-bit registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RegisterType {
    /// One register, unsigned.
    #[default]
    U16,
    /// One register, two's complement.
    I16,
    /// Two registers, unsigned.
    U32,
    /// Two registers, two's complement.
    I32,
    /// Two registers, IEEE 754 single precision.
    F32,
}

impl RegisterType {
    /// Number of 16-bit registers the value occupies.
    pub const fn register_count(self) -> u16 {
        match self {
            Self::U16 | Self::I16 => 1,
            Self::U32 | Self::I32 | Self::F32 => 2,
        }
    }
}

/// Which register of a two-register value carries the high 16 bits.
///
/// Modbus defines the byte order inside a register (big-endian) and says nothing about the
/// order of the registers themselves. Devices differ, so a mapping must say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WordOrder {
    /// The first register is the high word (`ABCD`).
    #[default]
    HighFirst,
    /// The first register is the low word (`CDAB`).
    LowFirst,
}

/// Register mapping item binding a Modbus register to a canonical tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagMapping {
    /// Canonical tag URI (must begin with "external.")
    pub uri: String,
    /// Modbus register address (0-based)
    pub address: u16,
    /// Function code: 3 (Holding Registers) or 4 (Input Registers)
    pub function_code: u8,
    /// Register layout of the value.
    #[serde(default)]
    pub data_type: RegisterType,
    /// Register order of a two-register value. Ignored for one-register types.
    #[serde(default)]
    pub word_order: WordOrder,
    /// Scaling multiplier applied to the decoded value.
    #[serde(default = "default_scale")]
    pub scale: f64,
}

fn default_scale() -> f64 {
    1.0
}

impl TagMapping {
    /// Decode the registers a device returned for this mapping.
    ///
    /// Returns `None` when the device returned fewer registers than the type needs. With a
    /// scale of exactly `1.0` the value keeps the device's own type; any other scale produces a
    /// 64-bit float, because a scaled integer is no longer an integer.
    pub fn decode(&self, registers: &[u16]) -> Option<SampleValue> {
        let needed = self.data_type.register_count() as usize;
        if registers.len() < needed {
            return None;
        }

        let wide = |order: WordOrder| -> u32 {
            let (high, low) = match order {
                WordOrder::HighFirst => (registers[0], registers[1]),
                WordOrder::LowFirst => (registers[1], registers[0]),
            };
            ((high as u32) << 16) | (low as u32)
        };

        let native = match self.data_type {
            RegisterType::U16 => SampleValue::U16(registers[0]),
            RegisterType::I16 => SampleValue::I16(registers[0] as i16),
            RegisterType::U32 => SampleValue::U32(wide(self.word_order)),
            RegisterType::I32 => SampleValue::I32(wide(self.word_order) as i32),
            RegisterType::F32 => SampleValue::F32(f32::from_bits(wide(self.word_order))),
        };

        if self.scale == 1.0 {
            return Some(native);
        }

        let as_f64 = match native {
            SampleValue::U16(v) => v as f64,
            SampleValue::I16(v) => v as f64,
            SampleValue::U32(v) => v as f64,
            SampleValue::I32(v) => v as f64,
            SampleValue::F32(v) => v as f64,
            // `native` is built just above from the five register types only.
            other => return Some(other),
        };
        Some(SampleValue::F64(as_f64 * self.scale))
    }
}

/// Root configuration for the Modbus Driver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModbusDriverConfig {
    pub transport: ModbusTransport,
    /// Host IP/hostname of the Modbus TCP device or gateway. Required for TCP.
    #[serde(default)]
    pub host: Option<String>,
    /// Port (default 502 for TCP)
    #[serde(default = "default_port")]
    pub port: u16,
    /// Serial port device path (RTU only; RTU is not implemented)
    #[serde(default)]
    pub serial_port: Option<String>,
    /// Baud rate (RTU only; RTU is not implemented)
    #[serde(default = "default_baud")]
    pub baud_rate: u32,
    /// Unit/Slave ID (1..247)
    #[serde(default = "default_slave_id")]
    pub slave_id: u8,
    /// Per-request timeout in milliseconds (10..=10000)
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// List of tag mappings
    #[serde(default)]
    pub mappings: Vec<TagMapping>,
}

fn default_port() -> u16 {
    502
}

fn default_baud() -> u32 {
    9600
}

fn default_slave_id() -> u8 {
    1
}

fn default_timeout_ms() -> u64 {
    1000
}

impl ModbusDriverConfig {
    /// Validate configuration and tag namespace hygiene.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.slave_id == 0 || self.slave_id > 247 {
            return Err(PluginError::InvalidConfiguration(format!(
                "Slave ID {} is outside 1..=247",
                self.slave_id
            )));
        }

        if !(10..=10_000).contains(&self.timeout_ms) {
            return Err(PluginError::InvalidConfiguration(format!(
                "timeout_ms {} is outside 10..=10000",
                self.timeout_ms
            )));
        }

        match self.transport {
            ModbusTransport::Tcp => {
                if self.host.as_deref().map(str::trim).unwrap_or("").is_empty() {
                    return Err(PluginError::InvalidConfiguration(
                        "TCP transport requires 'host'".into(),
                    ));
                }
            }
            ModbusTransport::Rtu => {
                // The serial transport does not exist. Refusing here is the only honest
                // outcome: a driver that started would have to answer with something, and it
                // has nothing to answer with.
                return Err(PluginError::InvalidConfiguration(
                    "Modbus RTU transport is not implemented; only 'tcp' is available".into(),
                ));
            }
        }

        let mut seen = HashSet::new();
        for mapping in &self.mappings {
            if !mapping.uri.starts_with("external.") {
                return Err(PluginError::InvalidTag(format!(
                    "Tag URI '{}' violates physical namespace contract (must start with 'external.')",
                    mapping.uri
                )));
            }

            if !seen.insert(mapping.uri.as_str()) {
                return Err(PluginError::InvalidTag(format!(
                    "Tag URI '{}' is mapped more than once",
                    mapping.uri
                )));
            }

            // Prohibited transport names check
            let segments: Vec<&str> = mapping.uri.split('.').collect();
            if segments.len() < 4 {
                return Err(PluginError::InvalidTag(format!(
                    "Tag URI '{}' must have at least external.<namespace>.<device>.<variable>",
                    mapping.uri
                )));
            }

            let namespace = segments[1].to_lowercase();
            if namespace == "modbus" || namespace == "profinet" || namespace == "mqtt" {
                return Err(PluginError::InvalidTag(format!(
                    "Tag URI '{}' prohibited protocol name in namespace",
                    mapping.uri
                )));
            }

            if mapping.function_code != 3 && mapping.function_code != 4 {
                return Err(PluginError::InvalidConfiguration(format!(
                    "Unsupported function code {} for tag '{}' (3 = holding, 4 = input)",
                    mapping.function_code, mapping.uri
                )));
            }

            if !mapping.scale.is_finite() || mapping.scale == 0.0 {
                return Err(PluginError::InvalidConfiguration(format!(
                    "Scale {} for tag '{}' must be finite and non-zero",
                    mapping.scale, mapping.uri
                )));
            }

            let last = mapping.address as u32 + mapping.data_type.register_count() as u32 - 1;
            if last > u16::MAX as u32 {
                return Err(PluginError::InvalidConfiguration(format!(
                    "Tag '{}' at address {} runs past the last register",
                    mapping.uri, mapping.address
                )));
            }
        }

        Ok(())
    }
}
