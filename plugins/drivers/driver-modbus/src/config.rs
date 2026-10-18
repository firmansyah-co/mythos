//! Modbus driver configuration models and register mappings.

use mythos_plugin_sdk::PluginError;
use serde::{Deserialize, Serialize};

/// Transport type: TCP or RTU.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModbusTransport {
    Tcp,
    Rtu,
}

/// Register mapping item binding a Modbus register to a canonical SHM tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagMapping {
    /// Canonical tag URI (must begin with "external.")
    pub uri: String,
    /// Modbus register address (0-based)
    pub address: u16,
    /// Function code: 3 (Holding Registers) or 4 (Input Registers)
    pub function_code: u8,
    /// Data type: "u16", "i16", "f32" (spanning 2 registers)
    #[serde(default = "default_datatype")]
    pub data_type: String,
    /// Scaling multiplier applied to raw integer
    #[serde(default = "default_scale")]
    pub scale: f64,
}

fn default_datatype() -> String {
    "u16".to_string()
}

fn default_scale() -> f64 {
    1.0
}

/// Root configuration for the Modbus Driver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModbusDriverConfig {
    pub transport: ModbusTransport,
    /// Host IP/hostname (for TCP, e.g. "127.0.0.1")
    #[serde(default)]
    pub host: Option<String>,
    /// Port (default 502 for TCP)
    #[serde(default = "default_port")]
    pub port: u16,
    /// Serial port device path (for RTU, e.g. "/dev/ttyUSB0")
    #[serde(default)]
    pub serial_port: Option<String>,
    /// Baud rate for RTU (default 9600)
    #[serde(default = "default_baud")]
    pub baud_rate: u32,
    /// Unit/Slave ID (1..247)
    #[serde(default = "default_slave_id")]
    pub slave_id: u8,
    /// Timeout in milliseconds (10..10000ms)
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
        if self.slave_id == 0 {
            return Err(PluginError::InvalidConfiguration(
                "Slave ID cannot be 0".into(),
            ));
        }

        match self.transport {
            ModbusTransport::Tcp => {
                if self.host.is_none() || self.host.as_deref() == Some("") {
                    return Err(PluginError::InvalidConfiguration(
                        "TCP transport requires 'host'".into(),
                    ));
                }
            }
            ModbusTransport::Rtu => {
                if self.serial_port.is_none() || self.serial_port.as_deref() == Some("") {
                    return Err(PluginError::InvalidConfiguration(
                        "RTU transport requires 'serial_port'".into(),
                    ));
                }
            }
        }

        for mapping in &self.mappings {
            if !mapping.uri.starts_with("external.") {
                return Err(PluginError::InvalidTag(format!(
                    "Tag URI '{}' violates physical namespace contract (must start with 'external.')",
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

            if mapping.function_code != 3
                && mapping.function_code != 4
                && mapping.function_code != 6
            {
                return Err(PluginError::InvalidConfiguration(format!(
                    "Unsupported function code {} for tag '{}'",
                    mapping.function_code, mapping.uri
                )));
            }
        }

        Ok(())
    }
}
