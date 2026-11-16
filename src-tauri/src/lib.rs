//! # MythOS Industrial Workbench - Desktop Core Library
//!
//! Entry point for the Tauri v2 desktop application, which embeds the MythOS server.

use mythos_core::api::create_router;
use mythos_core::server::{bootstrap, shm_name_from_env};
use mythos_core::storage::default_projects_dir;
use mythos_pal::DefaultShm;
use std::net::SocketAddr;
use tauri::{AppHandle, Manager, RunEvent};
use tokio::sync::oneshot;
use tracing::{error, info};

/// Spawn the embedded MythOS server in a background tokio task.
pub fn spawn_microkernel(shutdown_rx: oneshot::Receiver<()>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let storage_root = std::env::var_os("MYTHOS_STORAGE_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(default_projects_dir);

        let server = match bootstrap::<DefaultShm>(&storage_root, &shm_name_from_env()).await {
            Ok(server) => server,
            Err(e) => {
                error!(
                    "Embedded server failed to initialize storage at {}: {}",
                    storage_root.display(),
                    e
                );
                return;
            }
        };
        mythos_core::server::announce_setup_code(&server);
        let io = server.io;
        let app = create_router(server.state);

        let port: u16 = std::env::var("MYTHOS_HTTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(9090);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        info!("Embedded MythOS server listening on http://{}", addr);

        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind embedded server listener to {}: {}", addr, e);
                return;
            }
        };

        // Telemetry streams never end on their own, so stop serving on the signal rather than
        // waiting for every stream to close.
        tokio::select! {
            result = axum::serve(listener, app) => {
                if let Err(e) = result {
                    error!("Embedded server terminated with error: {}", e);
                }
            }
            _ = shutdown_rx => info!("Received shutdown signal for embedded server."),
        }

        io.shutdown().await;
        info!("Embedded server terminated cleanly.");
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
