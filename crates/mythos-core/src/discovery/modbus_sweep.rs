//! Heuristic Modbus RTU/TCP Sweep Discoverer.
//!
//! Implements 3-Tier Heuristic Sweep (Tier 1 Fast Probe, Tier 2 Expanding Sweep,
//! Tier 3 Full Blind Sweep) and catalog fingerprinting according to SYS-REQ-074 and
//! MYTHOS-CAT-001.

use super::traits::{
    DiscoveredDevice, DiscoveryContext, DiscoveryError, DiscoveryMode, ProtocolDiscoverer,
};
use crate::catalog::CatalogFingerprintEngine;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Abstraction for sending a Modbus probe request to a slave on an interface.
#[async_trait]
pub trait ModbusProbeTransport: Send + Sync {
    /// Probe a single slave ID by reading a holding register (FC 0x03).
    async fn probe_holding_register(
        &self,
        slave_id: u8,
        register_address: u16,
        timeout_duration: Duration,
    ) -> Result<u16, String>;
}

/// Simulated in-memory Modbus transport for unit testing and offline discovery tests.
#[derive(Default, Clone)]
pub struct MockModbusTransport {
    /// Mapping: (slave_id, register_address) -> register_value
    registers: HashMap<(u8, u16), u16>,
}

impl MockModbusTransport {
    pub fn new() -> Self {
        Self {
            registers: HashMap::new(),
        }
    }

    pub fn with_device(mut self, slave_id: u8, registers: &[(u16, u16)]) -> Self {
        for &(reg, val) in registers {
            self.registers.insert((slave_id, reg), val);
        }
        self
    }
}

#[async_trait]
impl ModbusProbeTransport for MockModbusTransport {
    async fn probe_holding_register(
        &self,
        slave_id: u8,
        register_address: u16,
        _timeout_duration: Duration,
    ) -> Result<u16, String> {
        if let Some(&val) = self.registers.get(&(slave_id, register_address)) {
            Ok(val)
        } else {
            Err("No response / timeout".to_string())
        }
    }
}

/// Modbus RTU / TCP Sweep Discoverer.
pub struct ModbusSweepDiscoverer {
    transport: Arc<dyn ModbusProbeTransport>,
    priority_baudrates: Vec<u32>,
    priority_parities: Vec<String>,
}

impl ModbusSweepDiscoverer {
    pub fn new(transport: Arc<dyn ModbusProbeTransport>) -> Self {
        Self {
            transport,
            priority_baudrates: vec![9600, 19200, 4800, 115200],
            priority_parities: vec!["None".to_string(), "Even".to_string()],
        }
    }

    pub fn with_priority_baudrates(mut self, bauds: Vec<u32>) -> Self {
        self.priority_baudrates = bauds;
        self
    }

    pub fn with_priority_parities(mut self, parities: Vec<String>) -> Self {
        self.priority_parities = parities;
        self
    }

    /// Execute heuristic sweep across candidate slave IDs.
    async fn sweep_candidates(
        &self,
        ctx: &DiscoveryContext,
        candidate_ids: &[u8],
        probe_reg: u16,
    ) -> Result<Vec<DiscoveredDevice>, DiscoveryError> {
        let mut discovered = Vec::new();

        for &slave_id in candidate_ids {
            // Check cooperative cancellation
            if ctx.cancel_token.is_cancelled() {
                return Err(DiscoveryError::Cancelled);
            }

            // Probe single register with fast probe timeout
            let probe_result = self
                .transport
                .probe_holding_register(slave_id, probe_reg, ctx.timeout_bounds.fast_probe_timeout)
                .await;

            if let Ok(reg_value) = probe_result {
                // Device responded!
                let mut device = DiscoveredDevice {
                    device_id: format!("modbus-{}-slave-{}", ctx.target_interface, slave_id),
                    protocol: "modbus-rtu".to_string(),
                    station_name: Some(format!("Modbus_Node_{}", slave_id)),
                    mac_address: None,
                    ip_address: None,
                    modbus_slave_id: Some(slave_id),
                    fingerprint_model: None,
                    vendor: None,
                    confidence: 0.6,
                    properties: HashMap::new(),
                };

                device
                    .properties
                    .insert("probed_register".to_string(), probe_reg.to_string());
                device
                    .properties
                    .insert("probed_value".to_string(), reg_value.to_string());

                // If catalog is available in context, fingerprint against catalog
                if let Some(ref catalog) = ctx.catalog {
                    let fp_engine = CatalogFingerprintEngine::new(catalog);
                    let matches = fp_engine.match_probe_response(slave_id, probe_reg, reg_value);
                    if let Some(best) = matches.first() {
                        device.fingerprint_model = Some(best.model.clone());
                        device.vendor = Some(best.vendor.clone());
                        device.confidence = (device.confidence + best.confidence) / 2.0;
                        device
                            .properties
                            .insert("catalog_manifest_id".to_string(), best.manifest_id.clone());
                    }
                }

                discovered.push(device);
            }
        }

        Ok(discovered)
    }
}

#[async_trait]
impl ProtocolDiscoverer for ModbusSweepDiscoverer {
    fn protocol_id(&self) -> &str {
        "modbus-rtu"
    }

    async fn discover(
        &self,
        ctx: &DiscoveryContext,
    ) -> Result<Vec<DiscoveredDevice>, DiscoveryError> {
        // Enforce total scan timeout constraint
        let scan_fut = async {
            match ctx.mode {
                DiscoveryMode::GuidedDiscovery => {
                    // Tier 1 Fast Probe (IDs 1..16, probe register 0 / 40001)
                    let tier1_candidates: Vec<u8> = (1..=16).collect();
                    let tier1_results = self.sweep_candidates(ctx, &tier1_candidates, 0).await?;

                    if !tier1_results.is_empty() {
                        return Ok(tier1_results);
                    }

                    // Tier 2 Expanding Sweep (IDs 17..32)
                    let tier2_candidates: Vec<u8> = (17..=32).collect();
                    self.sweep_candidates(ctx, &tier2_candidates, 0).await
                }
                DiscoveryMode::BlindBusSweep => {
                    // Full scan candidate IDs 1..247
                    let full_candidates: Vec<u8> = (1..=247).collect();
                    self.sweep_candidates(ctx, &full_candidates, 0).await
                }
            }
        };

        match timeout(ctx.timeout_bounds.total_scan_timeout, scan_fut).await {
            Ok(res) => res,
            Err(_) => Err(DiscoveryError::Timeout(
                ctx.timeout_bounds.total_scan_timeout,
            )),
        }
    }
}
