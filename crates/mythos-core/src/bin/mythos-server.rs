//! # MythOS Server Daemon
//!
//! Hosts the HTTP/SSE API and the field I/O runtime that feeds the tag bus.

use mythos_core::api::create_router;
use mythos_core::server::{bootstrap, shm_name_from_env};
use mythos_core::storage::DEFAULT_PROJECTS_DIR;
use mythos_pal::PosixShm;
use std::net::SocketAddr;

/// Resolves when the process is asked to stop (Ctrl-C, or SIGTERM on Unix).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let storage_root =
        std::env::var("MYTHOS_STORAGE_ROOT").unwrap_or_else(|_| DEFAULT_PROJECTS_DIR.to_string());
    let shm_name = shm_name_from_env();

    let server = bootstrap::<PosixShm>(&storage_root, &shm_name).await?;
    let app = create_router(server.state);

    let port: u16 = std::env::var("MYTHOS_HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("MythOS server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // Telemetry streams never end on their own, so a graceful HTTP shutdown would wait for
    // every browser to close. Stop serving on the signal instead, then let the I/O runtime mark
    // its tags "not connected" before the process exits.
    tokio::select! {
        result = axum::serve(listener, app) => result?,
        _ = shutdown_signal() => tracing::info!("Shutdown requested"),
    }

    if let Some(io) = server.io {
        io.shutdown().await;
    }
    Ok(())
}
