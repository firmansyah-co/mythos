//! # MythOS Universal Plugin SDK
//!
//! Trait definitions, capability guards, and lifecycle hooks for first-party and third-party plugins.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error types occurring during plugin lifecycle management.
#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin initialization error: {0}")]
    InitializationFailed(String),

    #[error("Plugin runtime failure: {0}")]
    RuntimeError(String),

    #[error("Capability violation: {0}")]
    CapabilityViolation(String),
}

/// Metadata identifying a loaded plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
}

/// Universal plugin lifecycle contract for all MythOS extensions.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Retrieve manifest metadata describing this plugin.
    fn metadata(&self) -> PluginMetadata;

    /// Initialize plugin state before execution.
    async fn on_load(&mut self) -> Result<(), PluginError>;

    /// Invoked periodically or upon scheduled tick.
    async fn on_tick(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    /// Graceful unload and cleanup handler.
    async fn on_unload(&mut self) -> Result<(), PluginError>;
}
