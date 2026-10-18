//! Microkernel plugin host and supervisor.
//!
//! Provides dynamic registration, capability isolation, and lifecycle management
//! for first-party engines and third-party extensions ("Everything is a Plugin" - ADR-008).

pub mod packager;

pub use packager::{PluginPackageManifest, PluginPackager, VerifiedPluginPackage};

use mythos_plugin_sdk::{
    MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata,
};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Managed plugin instance holding metadata, context, and state.
struct ManagedPlugin {
    plugin: Box<dyn MythosPlugin>,
    context: PluginContext,
    is_started: bool,
}

/// The microkernel plugin supervisor and host.
#[derive(Clone)]
pub struct PluginHost {
    plugins: Arc<RwLock<HashMap<String, ManagedPlugin>>>,
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    /// Create a new empty plugin host.
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new plugin with its context.
    pub async fn register(
        &self,
        plugin: Box<dyn MythosPlugin>,
        context: PluginContext,
    ) -> Result<PluginMetadata, PluginError> {
        let meta = plugin.metadata();
        let id = meta.id.clone();

        let mut lock = self.plugins.write().await;
        if lock.contains_key(&id) {
            return Err(PluginError::InitializationFailed(format!(
                "Plugin '{}' is already registered",
                id
            )));
        }

        let managed = ManagedPlugin {
            plugin,
            context,
            is_started: false,
        };
        lock.insert(id, managed);
        Ok(meta)
    }

    /// Initialize a registered plugin.
    pub async fn initialize_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let mut lock = self.plugins.write().await;
        let managed = lock.get_mut(plugin_id).ok_or_else(|| {
            PluginError::InitializationFailed(format!("Plugin '{}' not found", plugin_id))
        })?;

        managed.plugin.init(&managed.context).await
    }

    /// Start a registered plugin with the specified configuration.
    pub async fn start_plugin(
        &self,
        plugin_id: &str,
        config: &JsonValue,
    ) -> Result<(), PluginError> {
        let mut lock = self.plugins.write().await;
        let managed = lock
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::StartFailed(format!("Plugin '{}' not found", plugin_id)))?;

        managed.plugin.start(&managed.context, config).await?;
        managed.is_started = true;
        Ok(())
    }

    /// Stop a running plugin.
    pub async fn stop_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        let mut lock = self.plugins.write().await;
        let managed = lock.get_mut(plugin_id).ok_or_else(|| {
            PluginError::RuntimeError(format!("Plugin '{}' not found", plugin_id))
        })?;

        managed.plugin.stop(&managed.context).await?;
        managed.is_started = false;
        Ok(())
    }

    /// Perform operational health check on a specific plugin.
    pub async fn health_check(&self, plugin_id: &str) -> Result<PluginHealthReport, PluginError> {
        let lock = self.plugins.read().await;
        let managed = lock.get(plugin_id).ok_or_else(|| {
            PluginError::RuntimeError(format!("Plugin '{}' not found", plugin_id))
        })?;

        managed.plugin.health_check().await
    }

    /// Perform health check across all registered plugins.
    pub async fn health_check_all(&self) -> HashMap<String, PluginHealthReport> {
        let lock = self.plugins.read().await;
        let mut reports = HashMap::new();

        for (id, managed) in lock.iter() {
            let report = managed.plugin.health_check().await.unwrap_or_else(|e| {
                PluginHealthReport::unhealthy(format!("Health check error: {}", e))
            });
            reports.insert(id.clone(), report);
        }

        reports
    }

    /// Retrieve metadata for all registered plugins.
    pub async fn list_plugins(&self) -> Vec<PluginMetadata> {
        let lock = self.plugins.read().await;
        lock.values().map(|m| m.plugin.metadata()).collect()
    }

    /// Cyclic tick hook invoking all running plugins.
    pub async fn tick_all(&self, delta_ms: u32) -> Result<(), PluginError> {
        let mut lock = self.plugins.write().await;
        for (id, managed) in lock.iter_mut() {
            if managed.is_started {
                if let Err(e) = managed.plugin.tick(&managed.context, delta_ms).await {
                    tracing::error!("Error ticking plugin '{}': {}", id, e);
                }
            }
        }
        Ok(())
    }

    /// Install a plugin package (.mythos-plugin) into the plugins directory,
    /// verifying its integrity and capabilities.
    pub fn install_from_package(
        &self,
        package_path: &Path,
        plugins_dir: &Path,
        allowed_caps: HashSet<String>,
    ) -> Result<PluginMetadata, PluginError> {
        let verified = PluginPackager::extract_and_verify(package_path, plugins_dir)?;
        PluginPackager::verify_capabilities(&verified.manifest, &allowed_caps)?;
        Ok(verified.manifest.metadata)
    }

    /// Gracefully hot-swap a running or registered plugin with a new implementation.
    ///
    /// Stops the existing plugin (if started), replaces it with `new_plugin`,
    /// initializes it with the existing PluginContext, and starts it with `config`.
    /// The TagBus and underlying microkernel state remain uninterrupted.
    pub async fn hotswap_plugin(
        &self,
        plugin_id: &str,
        mut new_plugin: Box<dyn MythosPlugin>,
        config: &JsonValue,
    ) -> Result<(), PluginError> {
        let mut lock = self.plugins.write().await;
        let managed = lock.get_mut(plugin_id).ok_or_else(|| {
            PluginError::RuntimeError(format!("Cannot hotswap nonexistent plugin '{}'", plugin_id))
        })?;

        // 1. Gracefully stop existing plugin if started
        if managed.is_started {
            managed.plugin.stop(&managed.context).await?;
            managed.is_started = false;
        }

        // 2. Initialize new plugin instance
        new_plugin.init(&managed.context).await?;

        // 3. Start new plugin instance with config
        new_plugin.start(&managed.context, config).await?;

        // 4. Swap new plugin into managed slot
        managed.plugin = new_plugin;
        managed.is_started = true;

        Ok(())
    }
}
