//! # MythOS Modbus TCP Protocol Driver
//!
//! Asynchronous Modbus TCP master implementing the MythOS protocol driver SPI. It polls the
//! mapped registers and returns one typed sample per mapping; the host writes them to the tag
//! bus. Modbus RTU is not implemented and an RTU configuration is rejected.

pub mod client;
pub mod config;
pub mod crc;
pub mod frame;
pub mod module;
#[cfg(feature = "sim")]
pub mod sim;

use async_trait::async_trait;
use client::ModbusClient;
use config::{ModbusDriverConfig, RegisterType, TagMapping};
use frame::{ModbusRequest, ModbusResponse};
use mythos_plugin_sdk::{
    MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata, PluginType,
    ProtocolDriver, SampleFault, TagDeclaration, TagSample,
};
use serde_json::Value as JsonValue;

/// The capability a host must grant before this driver opens a socket.
pub const CAPABILITY_NETWORK_CLIENT: &str = "network:client";

/// Modbus Protocol Driver state.
pub struct ModbusDriver {
    config: Option<ModbusDriverConfig>,
    client: Option<ModbusClient>,
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
            consecutive_errors: 0,
            is_running: false,
        }
    }
}

/// Translate a Modbus exception code into the fault it means for a tag.
///
/// Codes 1 to 3 say the request itself is wrong for this device (function, address or value):
/// the mapping is misconfigured and polling again will not help. Code 4 is the device reporting
/// its own failure. Everything else (acknowledge, busy, gateway path or target failures) is the
/// path to the device not delivering an answer.
fn fault_for_exception(function_code: u8, exception_code: u8) -> SampleFault {
    let text = format!(
        "Modbus exception 0x{:02X} on function 0x{:02X}",
        exception_code, function_code
    );
    match exception_code {
        0x01..=0x03 => SampleFault::ConfigError(text),
        0x04 => SampleFault::DeviceFailure(text),
        _ => SampleFault::CommFailure(text),
    }
}

/// The tags a configuration provides, each described by where its value comes from.
fn declarations(config: &ModbusDriverConfig) -> Vec<TagDeclaration> {
    let host = config.host.as_deref().unwrap_or("");
    config
        .mappings
        .iter()
        .map(|m| TagDeclaration {
            uri: m.uri.clone(),
            description: format!(
                "Modbus TCP {}:{} unit {} {} register {}",
                host,
                config.port,
                config.slave_id,
                if m.function_code == 4 {
                    "input"
                } else {
                    "holding"
                },
                m.address
            ),
            writable: writable(m),
        })
        .collect()
}

/// Whether `write_tag` can write a mapping: a holding register that fits one register. An
/// input register cannot be written at all, and a value spanning two registers would need a
/// multi-register write, which does not exist here. `write_tag` refuses exactly the others.
fn writable(mapping: &TagMapping) -> bool {
    mapping.function_code == 3 && matches!(mapping.data_type, RegisterType::U16 | RegisterType::I16)
}

fn read_request(slave_id: u8, mapping: &TagMapping) -> ModbusRequest {
    let quantity = mapping.data_type.register_count();
    if mapping.function_code == 4 {
        ModbusRequest::ReadInputRegisters {
            unit_id: slave_id,
            start_addr: mapping.address,
            quantity,
        }
    } else {
        ModbusRequest::ReadHoldingRegisters {
            unit_id: slave_id,
            start_addr: mapping.address,
            quantity,
        }
    }
}

#[async_trait]
impl MythosPlugin for ModbusDriver {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "org.mythos.driver.modbus".to_string(),
            name: "MythOS Modbus TCP Protocol Driver".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: "MythOS Team <adrian@firmansyah.co>".to_string(),
            description: "Modbus TCP master driver".to_string(),
            plugin_type: PluginType::ProtocolDriver,
            capabilities: vec![CAPABILITY_NETWORK_CLIENT.to_string()],
        }
    }

    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        ctx.check_capability(CAPABILITY_NETWORK_CLIENT)?;
        Ok(())
    }

    async fn start(&mut self, ctx: &PluginContext, config: &JsonValue) -> Result<(), PluginError> {
        ctx.check_capability(CAPABILITY_NETWORK_CLIENT)?;
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
                "Modbus communication failure ({} consecutive failed reads)",
                self.consecutive_errors
            )));
        } else if self.consecutive_errors > 0 {
            return Ok(PluginHealthReport::degraded(format!(
                "Modbus read failures ({} consecutive)",
                self.consecutive_errors
            )));
        }

        Ok(PluginHealthReport::healthy("Modbus polling nominal"))
    }
}

#[async_trait]
impl ProtocolDriver for ModbusDriver {
    async fn describe_config(
        &self,
        config: &JsonValue,
    ) -> Result<Vec<TagDeclaration>, PluginError> {
        let parsed: ModbusDriverConfig = serde_json::from_value(config.clone()).map_err(|e| {
            PluginError::InvalidConfiguration(format!("Invalid configuration schema: {}", e))
        })?;
        parsed.validate()?;
        Ok(declarations(&parsed))
    }

    fn declared_tags(&self) -> Vec<TagDeclaration> {
        self.config.as_ref().map(declarations).unwrap_or_default()
    }

    async fn poll_cycle(&mut self, ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        ctx.check_capability(CAPABILITY_NETWORK_CLIENT)?;

        let (config, client) = match (self.config.as_ref(), self.client.as_mut()) {
            (Some(c), Some(cl)) => (c, cl),
            _ => {
                return Err(PluginError::RuntimeError(
                    "Driver is not started or missing configuration".into(),
                ))
            }
        };

        let slave_id = config.slave_id;
        let mut samples = Vec::with_capacity(config.mappings.len());
        // Once the link itself fails, every later mapping in this cycle shares that outcome.
        // Trying each one would cost a full timeout per mapping and report the same thing.
        let mut link_down: Option<String> = None;

        for mapping in &config.mappings {
            if let Some(reason) = &link_down {
                samples.push(TagSample::fault(
                    mapping.uri.clone(),
                    SampleFault::CommFailure(reason.clone()),
                ));
                continue;
            }

            let reading = match client.execute(read_request(slave_id, mapping)).await {
                Ok(ModbusResponse::Registers(regs)) => match mapping.decode(&regs) {
                    Some(value) => Ok(value),
                    None => Err(SampleFault::CommFailure(format!(
                        "device returned {} register(s), {} needed",
                        regs.len(),
                        mapping.data_type.register_count()
                    ))),
                },
                Ok(ModbusResponse::Exception {
                    function_code,
                    exception_code,
                }) => Err(fault_for_exception(function_code, exception_code)),
                Ok(other) => Err(SampleFault::CommFailure(format!(
                    "unexpected response to a read: {:?}",
                    other
                ))),
                Err(e) => {
                    let reason = e.to_string();
                    link_down = Some(reason.clone());
                    Err(SampleFault::CommFailure(reason))
                }
            };

            samples.push(TagSample {
                uri: mapping.uri.clone(),
                reading,
            });
        }

        if samples.iter().any(|s| s.reading.is_err()) {
            self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        } else {
            self.consecutive_errors = 0;
        }

        Ok(samples)
    }

    async fn write_tag(
        &mut self,
        ctx: &PluginContext,
        tag_uri: &str,
        value: &JsonValue,
    ) -> Result<(), PluginError> {
        ctx.check_capability(CAPABILITY_NETWORK_CLIENT)?;

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

        if mapping.function_code != 3 {
            return Err(PluginError::InvalidTag(format!(
                "Tag '{}' is an input register and cannot be written",
                tag_uri
            )));
        }

        let num_val = value
            .as_f64()
            .or_else(|| value.as_i64().map(|v| v as f64))
            .ok_or_else(|| {
                PluginError::InvalidConfiguration("Value must be a valid number".into())
            })?;

        let raw = (num_val / mapping.scale).round();
        // Only single-register writes exist. A value that does not fit the register is refused:
        // a wrapped or clamped write would command something the caller did not ask for.
        let raw_val: u16 = match mapping.data_type {
            RegisterType::U16 if (0.0..=u16::MAX as f64).contains(&raw) => raw as u16,
            RegisterType::I16 if (i16::MIN as f64..=i16::MAX as f64).contains(&raw) => {
                (raw as i16) as u16
            }
            RegisterType::U16 | RegisterType::I16 => {
                return Err(PluginError::InvalidConfiguration(format!(
                    "Value {} does not fit the register of tag '{}'",
                    num_val, tag_uri
                )));
            }
            RegisterType::U32 | RegisterType::I32 | RegisterType::F32 => {
                return Err(PluginError::InvalidTag(format!(
                    "Tag '{}' spans two registers; multi-register writes are not implemented",
                    tag_uri
                )));
            }
        };

        let req = ModbusRequest::WriteSingleRegister {
            unit_id: config.slave_id,
            addr: mapping.address,
            value: raw_val,
        };

        match client.execute(req).await {
            Ok(ModbusResponse::WriteAcknowledged { .. }) => Ok(()),
            Ok(ModbusResponse::Exception {
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
