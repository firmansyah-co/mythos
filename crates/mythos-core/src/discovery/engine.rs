//! Universal Discovery Engine Coordinator.
//!
//! Orchestrates protocol discoverers across Ethernet Layer 2 DCP, Modbus serial sweep,
//! and mDNS fallbacks.

use super::traits::{DiscoveredDevice, DiscoveryContext, DiscoveryError, ProtocolDiscoverer};
use std::collections::HashMap;
use std::sync::Arc;

/// Universal protocol discovery coordinator engine.
pub struct DiscoveryEngine {
    discoverers: HashMap<String, Arc<dyn ProtocolDiscoverer>>,
}

impl Default for DiscoveryEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoveryEngine {
    /// Create a new discovery coordinator engine.
    pub fn new() -> Self {
        Self {
            discoverers: HashMap::new(),
        }
    }

    /// Register a protocol discoverer.
    pub fn register(&mut self, discoverer: Arc<dyn ProtocolDiscoverer>) {
        self.discoverers
            .insert(discoverer.protocol_id().to_string(), discoverer);
    }

    /// Retrieve a registered discoverer by protocol ID.
    pub fn get(&self, protocol_id: &str) -> Option<&Arc<dyn ProtocolDiscoverer>> {
        self.discoverers.get(protocol_id)
    }

    /// List registered protocol IDs.
    pub fn registered_protocols(&self) -> Vec<String> {
        self.discoverers.keys().cloned().collect()
    }

    /// Execute discovery across all registered protocols (or specific protocol if specified).
    pub async fn run_discovery(
        &self,
        ctx: &DiscoveryContext,
        protocol_filter: Option<&str>,
    ) -> Result<Vec<DiscoveredDevice>, DiscoveryError> {
        let mut results = Vec::new();

        if let Some(target_protocol) = protocol_filter {
            if let Some(discoverer) = self.discoverers.get(target_protocol) {
                let mut found = discoverer.discover(ctx).await?;
                results.append(&mut found);
            } else {
                return Err(DiscoveryError::ProtocolError(format!(
                    "Discoverer for protocol '{}' not registered",
                    target_protocol
                )));
            }
        } else {
            // Run all discoverers sequentially or concurrently
            for discoverer in self.discoverers.values() {
                if ctx.cancel_token.is_cancelled() {
                    return Err(DiscoveryError::Cancelled);
                }
                match discoverer.discover(ctx).await {
                    Ok(mut found) => results.append(&mut found),
                    Err(DiscoveryError::Cancelled) => return Err(DiscoveryError::Cancelled),
                    Err(e) => {
                        tracing::warn!("Discoverer {} failed: {}", discoverer.protocol_id(), e);
                    }
                }
            }
        }

        Ok(results)
    }
}
