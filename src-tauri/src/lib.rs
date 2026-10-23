//! # MythOS Industrial Workbench - Desktop Core Library
//!
//! Provides the core entry point and microkernel supervisor for the Tauri v2 desktop application.

use mythos_core::api::{create_router, ApiState};
use mythos_core::storage::{ControllerStorage, DEFAULT_PROJECTS_DIR};
use mythos_core::tag::TagCatalog;
use mythos_pal::PosixShm;
use mythos_shm::ShmReaderHandle;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Manager, RunEvent};
use tokio::sync::{oneshot, RwLock};
use tracing::{error, info, warn};

/// Spawn the MythOS microkernel embedded service in a background tokio task.
pub fn spawn_microkernel(shutdown_rx: oneshot::Receiver<()>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let storage_root = std::env::var("MYTHOS_STORAGE_ROOT")
            .unwrap_or_else(|_| DEFAULT_PROJECTS_DIR.to_string());

        let storage = match ControllerStorage::new(&storage_root) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                error!(
                    "Desktop microkernel failed to initialize storage at {}: {}",
                    storage_root, e
                );
                return;
            }
        };

        let tag_catalog = Arc::new(RwLock::new(TagCatalog::new()));

        let shm_name = "mythos_tagbus";
        let shm_reader = match ShmReaderHandle::<PosixShm>::new(shm_name, 32768) {
            Ok(r) => Some(r),
            Err(e) => {
                warn!(
                    "Embedded microkernel running without SHM backing ({}): {}",
                    shm_name, e
                );
                None
            }
        };

        let state = ApiState::new(storage, tag_catalog, shm_reader);
        let app = create_router(state);

        let port: u16 = std::env::var("MYTHOS_HTTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(9090);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        info!("Embedded MythOS Microkernel listening on http://{}", addr);

        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind microkernel listener to {}: {}", addr, e);
                return;
            }
        };

        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
            info!("Received shutdown signal for embedded microkernel.");
        });

        if let Err(e) = server.await {
            error!("Embedded microkernel server terminated with error: {}", e);
        } else {
            info!("Embedded microkernel server terminated cleanly.");
        }
    })
}

/// Run the Tauri desktop application.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let kernel_handle = spawn_microkernel(shutdown_rx);

    let mut shutdown_sender = Some(shutdown_tx);

    let app = tauri::Builder::default()
        .setup(|app| {
            info!("Tauri desktop application setup complete. Window initialized.");
            #[cfg(debug_assertions)]
            {
                if let Some(main_window) = app.get_webview_window("main") {
                    let _ = main_window;
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Failed to build Tauri desktop application context");

    app.run(
        move |_app_handle: &AppHandle, event: RunEvent| match event {
            RunEvent::ExitRequested { .. } | RunEvent::Exit => {
                info!("Desktop window exit requested. Performing clean shutdown...");
                if let Some(tx) = shutdown_sender.take() {
                    let _ = tx.send(());
                }
            }
            _ => {}
        },
    );

    drop(kernel_handle);
    info!("MythOS Desktop App exiting cleanly.");

    Ok(())
}
