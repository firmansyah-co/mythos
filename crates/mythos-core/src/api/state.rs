//! Shared state for Axum API Gateway.

use crate::collab::RoomManager;
use crate::io::{IoManager, IoStatusHandle};
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
    /// The SSE event name. A service module chooses its own (ADR-011 §2.2).
    pub event: String,
    pub data: serde_json::Value,
}

impl HmiEvent {
    pub fn new(event: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            event: event.into(),
            data,
        }
    }
}

/// Application state shared across all Axum route handlers.
pub struct ApiState<S: PlatformShm + 'static = mythos_pal::DefaultShm> {
    pub storage: Arc<ControllerStorage>,
    pub tag_catalog: Arc<RwLock<TagCatalog>>,
    pub shm_reader: Option<Arc<RwLock<ShmReaderHandle<S>>>>,
    pub static_dir: Option<PathBuf>,
    pub room_manager: Arc<RoomManager>,
    /// Definition-change notifications relayed to every open operator station.
    pub hmi_events: BroadcastSender<HmiEvent>,
    /// What the field I/O runtime is doing, or why none is running.
    pub io_status: IoStatusHandle,
    /// The manager of the field I/O runtime, when this server has one. Without it an I/O
    /// configuration can be read and saved but not applied.
    pub io_manager: Option<Arc<IoManager<S>>>,
    /// The controller's accounts. With them every route requires what `crate::auth::policy`
    /// says; without them (a state built for a test, or for embedding) nothing is checked. The
    /// server always has them: `crate::server` attaches them.
    pub auth: Option<Arc<crate::auth::Auth>>,
    /// The server's own version and installation, and the published releases.
    pub updates: Option<Arc<crate::updates::Updates>>,
    /// The service modules (ADR-011) whose routes this server forwards.
    pub services: Option<Arc<crate::services::ServiceHost>>,
    /// Which modules are switched off, and where packages are installed.
    pub modules: Option<Arc<crate::modules::ModuleSettings>>,
    /// This installation as a node: its id, name and role (ADR-012).
    pub node: Option<Arc<crate::node::Node>>,
}

impl<S: PlatformShm + 'static> Clone for ApiState<S> {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            tag_catalog: Arc::clone(&self.tag_catalog),
            shm_reader: self.shm_reader.clone(),
            static_dir: self.static_dir.clone(),
            room_manager: Arc::clone(&self.room_manager),
            hmi_events: self.hmi_events.clone(),
            io_status: self.io_status.clone(),
            io_manager: self.io_manager.clone(),
            auth: self.auth.clone(),
            updates: self.updates.clone(),
            services: self.services.clone(),
            modules: self.modules.clone(),
            node: self.node.clone(),
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
            hmi_events: BroadcastSender::new(256),
            io_status: IoStatusHandle::not_running(
                "no field I/O runtime is attached to this server",
            ),
            io_manager: None,
            auth: None,
            updates: None,
            services: None,
            modules: None,
            node: None,
        }
    }

    /// Publish a definition-change notification to every open operator station.
    ///
    /// A send with no subscribers is not an error: a release must succeed whether or not
    /// anyone is watching.
    pub fn notify_hmi(&self, event: HmiEvent) {
        let _ = self.hmi_events.send(event);
    }

    /// Attach the field I/O manager. Its status becomes the one reported at
    /// `GET /api/v1/io/status`.
    pub fn with_io_manager(mut self, manager: Arc<IoManager<S>>) -> Self {
        self.io_status = manager.status();
        self.io_manager = Some(manager);
        self
    }

    /// Attach the status of a field I/O runtime, reported at `GET /api/v1/io/status`.
    pub fn with_io_status(mut self, io_status: IoStatusHandle) -> Self {
        self.io_status = io_status;
        self
    }

    /// Configure custom static asset directory for Web HMI serving.
    /// Require accounts: every route is checked against `crate::auth::policy`.
    pub fn with_auth(mut self, auth: Arc<crate::auth::Auth>) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn with_modules(mut self, modules: Arc<crate::modules::ModuleSettings>) -> Self {
        self.modules = Some(modules);
        self
    }

    pub fn with_services(mut self, services: Arc<crate::services::ServiceHost>) -> Self {
        self.services = Some(services);
        self
    }

    pub fn with_node(mut self, node: Arc<crate::node::Node>) -> Self {
        self.node = Some(node);
        self
    }

    pub fn with_updates(mut self, updates: Arc<crate::updates::Updates>) -> Self {
        self.updates = Some(updates);
        self
    }

    pub fn with_static_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.static_dir = Some(path.into());
        self
    }

    /// Configure custom RoomManager instance for CRDT collaboration.
    pub fn with_room_manager(mut self, room_manager: Arc<RoomManager>) -> Self {
        self.room_manager = room_manager;
        self
    }
}
