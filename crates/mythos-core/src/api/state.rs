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
    /// Definition-change notifications relayed to every open operator station.
    pub hmi_events: BroadcastSender<HmiEvent>,
    /// What the field I/O runtime is doing, or why none is running.
    pub io_status: IoStatusHandle,
    /// The manager of the field I/O runtime, when this server has one. Without it an I/O
    /// configuration can be read and saved but not applied.
    pub io_manager: Option<Arc<IoManager<S>>>,
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
