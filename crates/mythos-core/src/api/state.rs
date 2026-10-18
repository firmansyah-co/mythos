//! Shared state for Axum API Gateway.

use crate::collab::RoomManager;
use crate::storage::ControllerStorage;
use crate::tag::TagCatalog;
use mythos_pal::PlatformShm;
use mythos_shm::ShmReaderHandle;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Application state shared across all Axum route handlers.
pub struct ApiState<S: PlatformShm + 'static = mythos_pal::DefaultShm> {
    pub storage: Arc<ControllerStorage>,
    pub tag_catalog: Arc<RwLock<TagCatalog>>,
    pub shm_reader: Option<Arc<RwLock<ShmReaderHandle<S>>>>,
    pub static_dir: Option<PathBuf>,
    pub room_manager: Arc<RoomManager>,
}

impl<S: PlatformShm + 'static> Clone for ApiState<S> {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            tag_catalog: Arc::clone(&self.tag_catalog),
            shm_reader: self.shm_reader.clone(),
            static_dir: self.static_dir.clone(),
            room_manager: Arc::clone(&self.room_manager),
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
        }
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
