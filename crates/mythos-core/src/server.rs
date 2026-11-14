//! Server bootstrap shared by the daemon binary and the desktop shell.
//!
//! Builds the API state and, when the active project has an `io.json`, starts the field I/O
//! runtime that feeds the tag bus the API reads from.

use crate::api::ApiState;
use crate::io::{DriverRegistry, IoConfig, IoRuntime, IoStatusHandle, IO_CONFIG_FILE};
use crate::storage::{ControllerStorage, StorageError};
use crate::tag::TagCatalog;
use mythos_pal::PlatformShm;
use mythos_shm::{ShmReaderHandle, ShmWriterHandle};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Name of the shared-memory tag bus segment, unless `MYTHOS_SHM_NAME` says otherwise.
pub const DEFAULT_SHM_NAME: &str = "mythos_tagbus";

/// Number of tag slots in the tag bus segment.
pub const TAG_CAPACITY: usize = 32768;

/// A bootstrapped server: the state for the router, and the I/O runtime when one is running.
pub struct Server<S: PlatformShm + 'static> {
    pub state: ApiState<S>,
    pub io: Option<IoRuntime>,
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

/// Open storage and the tag bus, and start field I/O for the active project.
pub async fn bootstrap<S: PlatformShm + 'static>(
    storage_root: &str,
    shm_name: &str,
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

    let (io, io_status) = start_io::<S>(&storage, shm_name, Arc::clone(&tag_catalog)).await;
    let state = ApiState::new(storage, tag_catalog, shm_reader).with_io_status(io_status);

    Ok(Server { state, io })
}

async fn start_io<S: PlatformShm + 'static>(
    storage: &ControllerStorage,
    shm_name: &str,
    tag_catalog: Arc<RwLock<TagCatalog>>,
) -> (Option<IoRuntime>, IoStatusHandle) {
    let not_running = |note: String| {
        info!("Field I/O is not running: {}", note);
        (None, IoStatusHandle::not_running(note))
    };

    let active = match storage.get_active_project() {
        Ok(Some(name)) => name,
        Ok(None) => {
            return not_running(
                "no project is active, so no I/O configuration is loaded".to_string(),
            )
        }
        Err(e) => return not_running(format!("the active project could not be resolved: {e}")),
    };

    let path = match storage.project_dir(&active) {
        Ok(dir) => dir.join(IO_CONFIG_FILE),
        Err(e) => return not_running(format!("the active project cannot be used: {e}")),
    };
    let config = match IoConfig::load(&path) {
        Ok(Some(config)) => config,
        Ok(None) => {
            return not_running(format!("active project '{active}' has no {IO_CONFIG_FILE}"))
        }
        Err(e) => {
            error!("Field I/O configuration rejected: {}", e);
            return (None, IoStatusHandle::not_running(e.to_string()));
        }
    };

    let writer = match ShmWriterHandle::<S>::new(shm_name, TAG_CAPACITY) {
        Ok(writer) => writer,
        Err(e) => {
            error!("Field I/O cannot open the tag bus '{}': {}", shm_name, e);
            return (
                None,
                IoStatusHandle::not_running(format!(
                    "the tag bus '{shm_name}' is unavailable: {e}"
                )),
            );
        }
    };

    let status = IoStatusHandle::default();
    let runtime = IoRuntime::start(
        config,
        Some(path.display().to_string()),
        &DriverRegistry::builtin(),
        tag_catalog,
        writer,
        status.clone(),
    )
    .await;
    (Some(runtime), status)
}
