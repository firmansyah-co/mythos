//! A router whose HMI routes are served by the real HMI module process (ADR-011 step 4),
//! exactly as the server mounts it: the screens and faceplate tests run against it unchanged.

use axum::Router;
use mythos_core::api::{create_router, ApiState};
use mythos_core::services::ServiceHost;
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::TagCatalog;
use mythos_module::host::{read_manifest, Fingerprint};
use mythos_module::ModuleBinary;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// The HMI module, built from this package (see `src/bin/hmi-module-under-test.rs`).
pub const HMI: &str = env!("CARGO_BIN_EXE_hmi-module-under-test");

pub async fn hmi_router(
    storage: Arc<ControllerStorage>,
    catalog: Arc<RwLock<TagCatalog>>,
) -> Router {
    let path = Path::new(HMI);
    let module = Arc::new(ModuleBinary {
        path: path.to_path_buf(),
        manifest: read_manifest(path)
            .await
            .expect("the HMI module prints its manifest"),
        fingerprint: Fingerprint::of(path).unwrap(),
    });
    let state = ApiState::<mythos_pal::MemoryShm>::new(storage, catalog, None);
    let services = ServiceHost::new(state.hmi_events.clone());
    services.sync(vec![module]).await;
    create_router(state.with_services(services))
}
