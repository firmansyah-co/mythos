//! # MythOS Server Microkernel Daemon
//!
//! Production daemon entry point hosting the Axum REST/SSE API Gateway,
//! lock-free TagBus supervisor, and Universal Plugin Host.

use mythos_core::api::{create_router, ApiState};
use mythos_core::storage::{ControllerStorage, DEFAULT_PROJECTS_DIR};
use mythos_core::tag::TagCatalog;
use mythos_core::PluginHost;
use mythos_pal::PosixShm;
use mythos_shm::ShmReaderHandle;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let storage_root =
        std::env::var("MYTHOS_STORAGE_ROOT").unwrap_or_else(|_| DEFAULT_PROJECTS_DIR.to_string());
    let storage = Arc::new(ControllerStorage::new(&storage_root)?);

    let tag_catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let plugin_host = PluginHost::new();

    // Attach to or initialize lock-free TagBus in shared memory
    let shm_name = "mythos_tagbus";
    let shm_reader = match ShmReaderHandle::<PosixShm>::new(shm_name, 32768) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!(
                "Could not attach to POSIX SHM ({}). Running without SHM backing: {}",
                shm_name,
                e
            );
            None
        }
    };

    let state = ApiState::new(storage, tag_catalog, shm_reader);
    let app = create_router(state);

    let port: u16 = std::env::var("MYTHOS_HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("MythOS Microkernel Daemon listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    let _ = plugin_host;
    Ok(())
}
