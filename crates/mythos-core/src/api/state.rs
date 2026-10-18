//! Shared state for Axum API Gateway.

use crate::storage::ControllerStorage;
use crate::tag::TagCatalog;
use mythos_pal::PlatformShm;
use mythos_shm::ShmReaderHandle;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Application state shared across all Axum route handlers.
pub struct ApiState<S: PlatformShm + 'static = mythos_pal::DefaultShm> {
    pub storage: Arc<ControllerStorage>,
    pub tag_catalog: Arc<RwLock<TagCatalog>>,
    pub shm_reader: Option<Arc<RwLock<ShmReaderHandle<S>>>>,
}

impl<S: PlatformShm + 'static> Clone for ApiState<S> {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            tag_catalog: Arc::clone(&self.tag_catalog),
            shm_reader: self.shm_reader.clone(),
        }
    }
}

impl<S: PlatformShm + 'static> ApiState<S> {
    pub fn new(
        storage: Arc<ControllerStorage>,
        tag_catalog: Arc<RwLock<TagCatalog>>,
        shm_reader: Option<ShmReaderHandle<S>>,
    ) -> Self {
        Self {
            storage,
            tag_catalog,
            shm_reader: shm_reader.map(|r| Arc::new(RwLock::new(r))),
        }
    }
}
