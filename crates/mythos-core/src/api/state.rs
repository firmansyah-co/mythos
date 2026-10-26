//! Shared state for Axum API Gateway.

use crate::api::mcp::McpSseTransport;
use crate::auth::{Ed25519TokenManager, MerkleAuditLedger};
use crate::collab::RoomManager;
use crate::storage::ControllerStorage;
use crate::tag::TagCatalog;
use mythos_pal::PlatformShm;
use mythos_shm::ShmReaderHandle;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast::Sender as BroadcastSender, RwLock};

/// A named HMI notification delivered on `/api/v1/telemetry/stream`.
///
/// This is how the UI learns that a definition changed without polling: a release on one
/// screen reaches every open operator station through the stream that is already open.
#[derive(Debug, Clone, PartialEq)]
pub struct HmiEvent {
    pub event: &'static str,
    pub data: serde_json::Value,
}

impl HmiEvent {
    pub fn new(event: &'static str, data: serde_json::Value) -> Self {
        Self { event, data }
    }
}

/// Application state shared across all Axum route handlers.
pub struct ApiState<S: PlatformShm + 'static = mythos_pal::DefaultShm> {
    pub storage: Arc<ControllerStorage>,
    pub tag_catalog: Arc<RwLock<TagCatalog>>,
    pub shm_reader: Option<Arc<RwLock<ShmReaderHandle<S>>>>,
    pub static_dir: Option<PathBuf>,
    pub room_manager: Arc<RoomManager>,
    pub mcp_transport: Arc<McpSseTransport>,
    pub token_manager: Option<Arc<Ed25519TokenManager>>,
    pub audit_ledger: Option<Arc<MerkleAuditLedger>>,
    /// Definition-change notifications relayed to every open operator station.
    pub hmi_events: BroadcastSender<HmiEvent>,
}

impl<S: PlatformShm + 'static> Clone for ApiState<S> {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            tag_catalog: Arc::clone(&self.tag_catalog),
            shm_reader: self.shm_reader.clone(),
            static_dir: self.static_dir.clone(),
            room_manager: Arc::clone(&self.room_manager),
            mcp_transport: Arc::clone(&self.mcp_transport),
            token_manager: self.token_manager.clone(),
            audit_ledger: self.audit_ledger.clone(),
            hmi_events: self.hmi_events.clone(),
        }
    }
}

impl<S: PlatformShm + 'static> ApiState<S> {
    pub fn new(
        storage: Arc<ControllerStorage>,
        tag_catalog: Arc<RwLock<TagCatalog>>,
        shm_reader: Option<ShmReaderHandle<S>>,
    ) -> Self {
        let static_dir = std::env::var("MYTHOS_STATIC_DIR")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from);

        Self {
            storage,
            tag_catalog,
            shm_reader: shm_reader.map(|r| Arc::new(RwLock::new(r))),
            static_dir,
            room_manager: Arc::new(RoomManager::new()),
            mcp_transport: Arc::new(McpSseTransport::new()),
            token_manager: None,
            audit_ledger: None,
            hmi_events: BroadcastSender::new(256),
        }
    }

    /// Publish a definition-change notification to every open operator station.
    ///
    /// A send with no subscribers is not an error: a release must succeed whether or not
    /// anyone is watching.
    pub fn notify_hmi(&self, event: HmiEvent) {
        let _ = self.hmi_events.send(event);
    }

    /// Configure custom static asset directory for Web HMI serving.
    pub fn with_static_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.static_dir = Some(path.into());
        self
    }

    /// Configure custom RoomManager instance for CRDT collaboration.
    pub fn with_room_manager(mut self, room_manager: Arc<RoomManager>) -> Self {
        self.room_manager = room_manager;
        self
    }

    /// Configure Ed25519 Token Manager for cryptographic AI token authentication.
    pub fn with_token_manager(mut self, token_manager: Arc<Ed25519TokenManager>) -> Self {
        self.token_manager = Some(token_manager);
        self
    }

    /// Configure Merkle Audit Ledger for audit tracking.
    pub fn with_audit_ledger(mut self, audit_ledger: Arc<MerkleAuditLedger>) -> Self {
        self.audit_ledger = Some(audit_ledger);
        self
    }
}
