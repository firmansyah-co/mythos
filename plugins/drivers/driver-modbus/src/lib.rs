//! # MythOS Reference Modbus Protocol Driver
//!
//! High-performance asynchronous Modbus RTU & TCP master protocol driver
//! adhering to the MythOS Universal Plugin SDK and Protocol Driver SPI.

pub mod client;
pub mod config;
pub mod crc;
pub mod frame;

use async_trait::async_trait;
use client::ModbusClient;
use config::ModbusDriverConfig;
use frame::ModbusRequest;
use mythos_plugin_sdk::{
    MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata, PluginType,
    ProtocolDriver,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Modbus Protocol Driver state and runtime actor.
pub struct ModbusDriver {
    config: Option<ModbusDriverConfig>,
    client: Option<ModbusClient>,
    cached_tag_values: HashMap<String, f64>,
    consecutive_errors: u32,
    is_running: bool,
}

impl Default for ModbusDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl ModbusDriver {
    pub fn new() -> Self {
        Self {
            config: None,
            client: None,
            cached_tag_values: HashMap::new(),
            consecutive_errors: 0,
            is_running: false,
        }
    }

    /// Read the currently cached process value for a tag.
    pub fn get_cached_value(&self, tag_uri: &str) -> Option<f64> {
        self.cached_tag_values.get(tag_uri).copied()
    }
}

#[async_trait]
impl MythosPlugin for ModbusDriver {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "org.mythos.driver.modbus".to_string(),
            name: "MythOS Reference Modbus Protocol Driver".to_string(),
            version: "0.1.0".to_string(),
            author: "MythOS Team <adrian@firmansyah.co>".to_string(),
            description:
                "High-performance Modbus RTU/TCP master driver with canonical SHM tag mapping"
                    .to_string(),
            plugin_type: PluginType::ProtocolDriver,
            capabilities: vec!["tagbus:write".to_string(), "network:client".to_string()],
        }
    }

    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        ctx.check_capability("tagbus:write")?;
        Ok(())
    }

    async fn start(&mut self, ctx: &PluginContext, config: &JsonValue) -> Result<(), PluginError> {
        ctx.check_capability("tagbus:write")?;
        let parsed: ModbusDriverConfig = serde_json::from_value(config.clone()).map_err(|e| {
            PluginError::InvalidConfiguration(format!("Failed to parse Modbus config: {}", e))
        })?;

        parsed.validate()?;
        self.client = Some(ModbusClient::new(parsed.clone()));
        self.config = Some(parsed);
        self.is_running = true;
        self.consecutive_errors = 0;

        Ok(())
    }

    async fn stop(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        if let Some(client) = self.client.as_mut() {
            client.disconnect().await;
        }
        self.is_running = false;
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
        if !self.is_running {
            return Ok(PluginHealthReport::stopped("Driver is stopped"));
        }

        if self.consecutive_errors > 3 {
            return Ok(PluginHealthReport::unhealthy(format!(
                "Modbus communication failure ({} consecutive timeouts)",
                self.consecutive_errors
            )));
        } else if self.consecutive_errors > 0 {
            return Ok(PluginHealthReport::degraded(format!(
                "Modbus comm retry in progress ({} warnings)",
                self.consecutive_errors
            )));
        }

        Ok(PluginHealthReport::healthy("Modbus polling nominal"))
    }
}

#[async_trait]
impl ProtocolDriver for ModbusDriver {
    fn validate_config(&self, config: &JsonValue) -> Result<(), PluginError> {
        let parsed: ModbusDriverConfig = serde_json::from_value(config.clone()).map_err(|e| {
            PluginError::InvalidConfiguration(format!("Invalid configuration schema: {}", e))
        })?;
        parsed.validate()
    }

    async fn poll_cycle(&mut self, ctx: &PluginContext) -> Result<u32, PluginError> {
        ctx.check_capability("tagbus:write")?;

        let (config, client) = match (self.config.as_ref(), self.client.as_mut()) {
            (Some(c), Some(cl)) => (c, cl),
            _ => {
                return Err(PluginError::RuntimeError(
                    "Driver is not started or missing configuration".into(),
                ))
            }
        };

        let mut updated_tags = 0;
        let slave_id = config.slave_id;

        for mapping in &config.mappings {
            let req = if mapping.function_code == 4 {
                ModbusRequest::ReadInputRegisters {
                    unit_id: slave_id,
                    start_addr: mapping.address,
                    quantity: 1,
                }
            } else {
                ModbusRequest::ReadHoldingRegisters {
                    unit_id: slave_id,
                    start_addr: mapping.address,
                    quantity: 1,
                }
            };

            match client.execute(req).await {
                Ok(frame::ModbusResponse::Registers(regs)) => {
                    if let Some(&first) = regs.first() {
                        let float_val = (first as f64) * mapping.scale;
                        self.cached_tag_values
                            .insert(mapping.uri.clone(), float_val);
                        updated_tags += 1;
                        self.consecutive_errors = 0;
                    }
                }
                Ok(frame::ModbusResponse::Exception {
                    function_code,
                    exception_code,
                }) => {
                    self.consecutive_errors += 1;
                    tracing::warn!(
                        "Modbus slave returned exception 0x{:02X} for FC 0x{:02X}",
                        exception_code,
                        function_code
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    self.consecutive_errors += 1;
                    tracing::error!("Modbus poll error for tag '{}': {}", mapping.uri, e);
                    // Bounded timeout invariant: do not abort loop, proceed to next
                }
            }
        }

        Ok(updated_tags)
    }

    async fn write_tag(
        &mut self,
        ctx: &PluginContext,
        tag_uri: &str,
        value: &JsonValue,
    ) -> Result<(), PluginError> {
        ctx.check_capability("tagbus:write")?;

        let (config, client) = match (self.config.as_ref(), self.client.as_mut()) {
            (Some(c), Some(cl)) => (c, cl),
            _ => {
                return Err(PluginError::RuntimeError(
                    "Driver is not started or missing configuration".into(),
                ))
            }
        };

        let mapping = config
            .mappings
            .iter()
            .find(|m| m.uri == tag_uri)
            .ok_or_else(|| {
                PluginError::InvalidTag(format!("Tag '{}' not mapped in Modbus driver", tag_uri))
            })?;

        let num_val = value
            .as_f64()
            .or_else(|| value.as_i64().map(|v| v as f64))
            .ok_or_else(|| {
                PluginError::InvalidConfiguration("Value must be a valid number".into())
            })?;

        let raw_val = (num_val / mapping.scale).round() as u16;

        let req = ModbusRequest::WriteSingleRegister {
            unit_id: config.slave_id,
            addr: mapping.address,
            value: raw_val,
        };

        match client.execute(req).await {
            Ok(frame::ModbusResponse::WriteAcknowledged { .. }) => {
                self.cached_tag_values.insert(tag_uri.to_string(), num_val);
                Ok(())
            }
            Ok(frame::ModbusResponse::Exception {
                function_code,
                exception_code,
            }) => Err(PluginError::CommunicationError(format!(
                "Modbus write exception 0x{:02X} on FC 0x{:02X}",
                exception_code, function_code
            ))),
            Ok(_) => Err(PluginError::CommunicationError(
                "Unexpected response".into(),
            )),
            Err(e) => Err(e),
        }
    }
}
