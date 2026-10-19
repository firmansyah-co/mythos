//! # MythOS Model Context Protocol Execution Context
//!
//! Provides execution context for MCP tool handlers, including access to
//! TagCatalog, ControllerStorage, Shared Memory handles, AlarmRegistry, HistorianBuffer,
//! MerkleAuditLedger, caller credentials, and safety interlock verification.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;

use mythos_core::alarm::{AlarmDefinition, AlarmRuntimeState};
use mythos_core::auth::MerkleAuditLedger;
use mythos_core::discovery::DiscoveryEngine;
use mythos_core::historian::TimeSeriesPoint;
use mythos_core::namespace::TagUri;
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::{AccessMode, TagCatalog, TagMetadata};

/// Alarm registry mapping IDs to definitions and runtime states.
#[derive(Debug, Default, Clone)]
pub struct AlarmRegistry {
    pub definitions: HashMap<u32, AlarmDefinition>,
    pub states: HashMap<u32, AlarmRuntimeState>,
}

/// In-memory historical buffer for time-series points.
#[derive(Debug, Default, Clone)]
pub struct HistorianBuffer {
    pub series: BTreeMap<String, Vec<TimeSeriesPoint>>,
}

/// Security credentials and caller metadata for authenticated MCP sessions.
#[derive(Debug, Clone)]
pub struct CallerAuth {
    pub caller_id: String,
    pub role: String,
    pub permissions: Vec<String>,
}

impl Default for CallerAuth {
    fn default() -> Self {
        Self {
            caller_id: "mcp-agent-local".to_string(),
            role: "operator".to_string(),
            permissions: vec![
                "tag:read".to_string(),
                "tag:write".to_string(),
                "alarm:read".to_string(),
                "alarm:ack".to_string(),
                "alarm:shelve".to_string(),
                "control:deploy".to_string(),
                "device:discover".to_string(),
                "telemetry:export".to_string(),
                "storage:read".to_string(),
            ],
        }
    }
}

impl CallerAuth {
    pub fn new(
        caller_id: impl Into<String>,
        role: impl Into<String>,
        permissions: Vec<String>,
    ) -> Self {
        Self {
            caller_id: caller_id.into(),
            role: role.into(),
            permissions,
        }
    }

    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == "*" || p == perm)
    }
}

/// Execution context passed to every MCP tool execution.
type TagValueRecord = (f64, u32, u64);

#[derive(Clone)]
pub struct McpContext {
    pub tag_catalog: Arc<RwLock<TagCatalog>>,
    pub storage: Option<Arc<ControllerStorage>>,
    pub alarm_registry: Arc<RwLock<AlarmRegistry>>,
    pub historian_buffer: Arc<RwLock<HistorianBuffer>>,
    pub audit_ledger: Option<Arc<MerkleAuditLedger>>,
    pub discovery_engine: Option<Arc<DiscoveryEngine>>,
    pub auth: CallerAuth,
    /// Simulation / Safety enforcement flag: when true, write attempts to ReadOnly tags are hard rejected
    pub enforce_safety_interlocks: bool,
    /// In-memory tag values store for simulated/local evaluation
    tag_values: Arc<RwLock<HashMap<String, TagValueRecord>>>,
}

impl McpContext {
    pub fn new(tag_catalog: Arc<RwLock<TagCatalog>>) -> Self {
        Self {
            tag_catalog,
            storage: None,
            alarm_registry: Arc::new(RwLock::new(AlarmRegistry::default())),
            historian_buffer: Arc::new(RwLock::new(HistorianBuffer::default())),
            audit_ledger: None,
            discovery_engine: None,
            auth: CallerAuth::default(),
            enforce_safety_interlocks: true,
            tag_values: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_storage(mut self, storage: Arc<ControllerStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn with_auth(mut self, auth: CallerAuth) -> Self {
        self.auth = auth;
        self
    }

    pub fn with_alarm_registry(mut self, registry: Arc<RwLock<AlarmRegistry>>) -> Self {
        self.alarm_registry = registry;
        self
    }

    pub fn with_historian_buffer(mut self, buffer: Arc<RwLock<HistorianBuffer>>) -> Self {
        self.historian_buffer = buffer;
        self
    }

    pub fn with_audit_ledger(mut self, ledger: Arc<MerkleAuditLedger>) -> Self {
        self.audit_ledger = Some(ledger);
        self
    }

    pub fn with_discovery_engine(mut self, engine: Arc<DiscoveryEngine>) -> Self {
        self.discovery_engine = Some(engine);
        self
    }

    pub fn with_safety_interlocks(mut self, enforce: bool) -> Self {
        self.enforce_safety_interlocks = enforce;
        self
    }

    pub fn caller_id(&self) -> &str {
        &self.auth.caller_id
    }

    pub fn role(&self) -> &str {
        &self.auth.role
    }

    pub fn tag_catalog(&self) -> &Arc<RwLock<TagCatalog>> {
        &self.tag_catalog
    }

    pub fn storage(&self) -> Option<&Arc<ControllerStorage>> {
        self.storage.as_ref()
    }

    pub fn audit_ledger(&self) -> Option<&Arc<MerkleAuditLedger>> {
        self.audit_ledger.as_ref()
    }

    /// Check if caller has required permission
    pub fn check_permission(&self, perm: &str) -> Result<(), crate::McpError> {
        if self.auth.has_permission(perm) {
            Ok(())
        } else {
            Err(crate::McpError::SafetyInterlockViolation(format!(
                "Caller '{}' with role '{}' lacks permission '{}'",
                self.auth.caller_id, self.auth.role, perm
            )))
        }
    }

    /// Check tag access safety before performing a write
    pub async fn validate_tag_write(&self, uri_str: &str) -> Result<TagMetadata, crate::McpError> {
        self.check_permission("tag:write")?;

        let _uri = TagUri::parse(uri_str).map_err(|e| {
            crate::McpError::ToolExecutionFailed(format!("Invalid tag URI '{}': {}", uri_str, e))
        })?;

        let catalog = self.tag_catalog.read().await;
        match catalog.get_by_uri(uri_str) {
            Some(meta) => {
                if self.enforce_safety_interlocks && meta.access_mode == AccessMode::ReadOnly {
                    return Err(crate::McpError::SafetyInterlockViolation(format!(
                        "Tag '{}' is designated READ-ONLY. AI writes forbidden by safety interlock.",
                        uri_str
                    )));
                }
                Ok(meta.clone())
            }
            None => Err(crate::McpError::ToolNotFound(format!(
                "Tag URI '{}' not found in catalog",
                uri_str
            ))),
        }
    }

    /// Execute a tag read returning (value, quality, timestamp_ns)
    pub async fn execute_tag_read(
        &self,
        uri_str: &str,
    ) -> Result<(f64, u32, u64), crate::McpError> {
        self.check_permission("tag:read")?;

        let catalog = self.tag_catalog.read().await;
        if catalog.get_by_uri(uri_str).is_none() {
            return Err(crate::McpError::ToolNotFound(format!(
                "Tag URI '{}' not found in catalog",
                uri_str
            )));
        }

        let values = self.tag_values.read().await;
        if let Some(&(val, quality, ts)) = values.get(uri_str) {
            Ok((val, quality, ts))
        } else {
            let now_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            // Default 0.0, Good quality (0xC0 / 192), current timestamp
            Ok((0.0, 192, now_ns))
        }
    }

    /// Execute a tag write returning (previous_value, new_value, audit_hash)
    pub async fn execute_tag_write(
        &self,
        uri_str: &str,
        value: f64,
        intent: &str,
    ) -> Result<(f64, f64, String), crate::McpError> {
        let meta = self.validate_tag_write(uri_str).await?;

        // Check bounds against engineering scale
        if self.enforce_safety_interlocks
            && meta.scale_max > meta.scale_min
            && (value < meta.scale_min || value > meta.scale_max)
        {
            return Err(crate::McpError::SafetyInterlockViolation(format!(
                "Setpoint value {} is outside engineering limits [{} .. {}] {} for tag '{}'",
                value, meta.scale_min, meta.scale_max, meta.engineering_units, uri_str
            )));
        }
        let clamped = value;

        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let mut values = self.tag_values.write().await;
        let prev_val = values.get(uri_str).map(|v| v.0).unwrap_or(0.0);
        values.insert(uri_str.to_string(), (clamped, 192, now_ns));

        // Audit log entry
        let audit_hash = if let Some(ledger) = &self.audit_ledger {
            let entry = ledger
                .append(
                    &self.auth.caller_id,
                    &self.auth.role,
                    "write_tag",
                    uri_str,
                    serde_json::json!({
                        "uri": uri_str,
                        "previous_value": prev_val,
                        "new_value": clamped,
                        "operator_intent": intent,
                        "timestamp_ns": now_ns,
                    }),
                )
                .map_err(|e| {
                    crate::McpError::InternalError(format!("Audit ledger error: {}", e))
                })?;
            entry.hash
        } else {
            format!("no_audit_{:x}", now_ns)
        };

        Ok((prev_val, clamped, audit_hash))
    }
}
