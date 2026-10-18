//! Device catalog fingerprinting and signature matching.
//!
//! Evaluates Modbus probe responses and register dumps against registered
//! device manifests in the hardware catalog.

use super::DeviceCatalog;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of fingerprint evaluation against catalog signatures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FingerprintMatch {
    pub manifest_id: String,
    pub vendor: String,
    pub model: String,
    pub confidence: f32,
    pub matched_registers: Vec<u16>,
    pub slave_id: u8,
}

/// Catalog Fingerprint Engine matching device signatures against registered manifests.
pub struct CatalogFingerprintEngine<'a> {
    catalog: &'a DeviceCatalog,
}

impl<'a> CatalogFingerprintEngine<'a> {
    /// Create a new fingerprint matcher with the provided catalog reference.
    pub fn new(catalog: &'a DeviceCatalog) -> Self {
        Self { catalog }
    }

    /// Match a single probed register reading to catalog manifests.
    ///
    /// For example, probing register 0 (40001) at slave_id with value `raw_val`.
    pub fn match_probe_response(
        &self,
        slave_id: u8,
        register_address: u16,
        register_value: u16,
    ) -> Vec<FingerprintMatch> {
        let mut matches = Vec::new();

        for manifest in self.catalog.list() {
            let mut score = 0.0f32;
            let mut matched_regs = Vec::new();

            if let Some(ref discovery) = manifest.discovery {
                if discovery.probe_register == register_address {
                    score += 0.5;
                    matched_regs.push(register_address);

                    // If probed register holds slave_id and matches
                    if register_value == slave_id as u16 {
                        score += 0.4;
                    }
                }
            }

            // Check if register exists in manifest register definitions
            if let Some(reg_def) = manifest
                .registers
                .iter()
                .find(|r| r.address_dec == register_address)
            {
                if !matched_regs.contains(&register_address) {
                    matched_regs.push(register_address);
                    score += 0.3;
                }

                // If register tag is SLAVE_ID and equals slave_id
                if (reg_def.tag_suffix.contains("SLAVE")
                    || reg_def.name.to_lowercase().contains("slave"))
                    && register_value == slave_id as u16
                {
                    score += 0.4;
                }
            }

            if score > 0.0 {
                let confidence = score.min(1.0);
                matches.push(FingerprintMatch {
                    manifest_id: manifest.metadata.id.clone(),
                    vendor: manifest.metadata.vendor.clone(),
                    model: manifest.metadata.model.clone(),
                    confidence,
                    matched_registers: matched_regs,
                    slave_id,
                });
            }
        }

        matches.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }

    /// Match a multi-register dump from an unknown device against catalog manifests.
    pub fn match_device_registers(
        &self,
        slave_id: u8,
        registers: &HashMap<u16, u16>,
    ) -> Vec<FingerprintMatch> {
        let mut matches = Vec::new();

        for manifest in self.catalog.list() {
            let mut matched_regs = Vec::new();
            let mut total_points = 0.0f32;
            let max_points = (manifest.registers.len() as f32).max(1.0);

            for reg_def in &manifest.registers {
                if let Some(&val) = registers.get(&reg_def.address_dec) {
                    matched_regs.push(reg_def.address_dec);
                    let mut reg_score = 1.0f32;

                    // If register is slave address, check match
                    if (reg_def.tag_suffix.contains("SLAVE")
                        || reg_def.name.to_lowercase().contains("slave"))
                        && val == slave_id as u16
                    {
                        reg_score += 1.0;
                    }

                    // If register has enum mapping, check if value exists in enum
                    if let Some(ref mapping) = reg_def.enum_mapping {
                        if mapping.contains_key(&val.to_string()) {
                            reg_score += 0.5;
                        }
                    }

                    total_points += reg_score;
                }
            }

            if !matched_regs.is_empty() {
                let confidence = (total_points / (max_points * 1.5)).min(1.0);
                matches.push(FingerprintMatch {
                    manifest_id: manifest.metadata.id.clone(),
                    vendor: manifest.metadata.vendor.clone(),
                    model: manifest.metadata.model.clone(),
                    confidence,
                    matched_registers: matched_regs,
                    slave_id,
                });
            }
        }

        matches.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }
}
