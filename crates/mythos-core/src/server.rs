//! Server bootstrap shared by the daemon binary and the desktop shell.
//!
//! Builds the API state and starts the field I/O manager, which runs the active project's
//! `io.json` and feeds the tag bus the API reads from.

use crate::api::ApiState;
use crate::io::{module_dirs_from_env, DriverRegistry, IoManager};
use crate::storage::{ControllerStorage, StorageError};
use crate::tag::TagCatalog;
use mythos_pal::PlatformShm;
use mythos_shm::ShmReaderHandle;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Name of the shared-memory tag bus segment, unless `MYTHOS_SHM_NAME` says otherwise.
pub const DEFAULT_SHM_NAME: &str = "mythos_tagbus";

/// Number of tag slots in the tag bus segment.
pub const TAG_CAPACITY: usize = 32768;

/// A bootstrapped server: the state for the router, and the manager of its field I/O.
pub struct Server<S: PlatformShm + 'static> {
    pub state: ApiState<S>,
    pub io: Arc<IoManager<S>>,
}

/// The tag bus segment name from `MYTHOS_SHM_NAME`, or the default.
///
/// Two servers must not share a segment: the tag bus has exactly one writer.
pub fn shm_name_from_env() -> String {
    std::env::var("MYTHOS_SHM_NAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SHM_NAME.to_string())
}

/// Open storage and the tag bus, find the modules, and start field I/O for the active project.
///
/// Modules are looked for where [`module_dirs_from_env`] says.
pub async fn bootstrap<S: PlatformShm + 'static>(
    storage_root: &str,
    shm_name: &str,
) -> Result<Server<S>, StorageError> {
    let dirs = module_dirs_from_env();
    let registry = DriverRegistry::discover(&dirs).await;
    for driver in registry.drivers() {
        if let Some(module) = &driver.module {
            info!(
                "Module '{}' {} provides driver '{}' ({})",
                module.manifest.id,
                module.manifest.version,
                driver.name,
                module.path.display()
            );
        }
    }
    for problem in registry.problems() {
        warn!(
            "Module {} cannot be used: {}",
            problem.path.display(),
            problem.message
        );
    }
    if registry.names().is_empty() {
        warn!(
            "No driver modules found in {:?}; set MYTHOS_MODULE_PATH to the directory that holds them",
            dirs
        );
    }
    bootstrap_with(storage_root, shm_name, registry).await
}

/// [`bootstrap`] with a given set of drivers.
pub async fn bootstrap_with<S: PlatformShm + 'static>(
    storage_root: &str,
    shm_name: &str,
    registry: DriverRegistry,
) -> Result<Server<S>, StorageError> {
    let storage = Arc::new(ControllerStorage::new(storage_root)?);
    let tag_catalog = Arc::new(RwLock::new(TagCatalog::new()));

    let shm_reader = match ShmReaderHandle::<S>::new(shm_name, TAG_CAPACITY) {
        Ok(reader) => Some(reader),
        Err(e) => {
            warn!(
                "Could not attach to the tag bus '{}'; telemetry will be empty: {}",
                shm_name, e
            );
            None
        }
    };

    let io = IoManager::<S>::new(
        Arc::clone(&storage),
        Arc::clone(&tag_catalog),
        shm_name,
        TAG_CAPACITY,
        registry,
    );
    io.reload().await;

    let state = ApiState::new(storage, tag_catalog, shm_reader).with_io_manager(Arc::clone(&io));

    Ok(Server { state, io })
}
