//! # MythOS Server Daemon
//!
//! Hosts the HTTP/SSE API and the field I/O runtime that feeds the tag bus.

use mythos_core::api::create_router;
use mythos_core::server::{bootstrap, shm_name_from_env};
use mythos_core::storage::default_projects_dir;
use mythos_pal::DefaultShm;
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

/// The workbench of a release bundle: `web/` beside the `bin/` this executable is in. Used when
/// `MYTHOS_STATIC_DIR` is not set; `None` when there is none (a build from source).
fn installed_workbench() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let web = exe.parent()?.parent()?.join("web");
    web.join("index.html").is_file().then_some(web)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    mythos_core::server::init_logging();

    let storage_root = std::env::var_os("MYTHOS_STORAGE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_projects_dir);
    let shm_name = shm_name_from_env();

    let server = bootstrap::<DefaultShm>(&storage_root, &shm_name).await?;
    mythos_core::server::announce_setup_code(&server);
    let mut state = server.state;
    if std::env::var_os("MYTHOS_STATIC_DIR").is_none() {
        if let Some(web) = installed_workbench() {
            tracing::info!(
                "Serving the workbench installed beside the server: {}",
                web.display()
            );
            state = state.with_static_dir(web);
        }
    }
    let app = create_router(state);

    let port: u16 = std::env::var("MYTHOS_HTTP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    // Where to listen is the node's role (ADR-012 §2.1): an edge or a server on every interface,
    // a workstation on this computer only. MYTHOS_BIND overrides it.
    let addr = SocketAddr::new(server.node.listen_ip()?, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    server.node.set_listening(listener.local_addr()?);
    tracing::info!("MythOS server listening on http://{}", addr);

    // Telemetry streams never end on their own, so a graceful HTTP shutdown would wait for
    // every browser to close. Stop serving on the signal instead, then let the I/O runtime mark
    // its tags "not connected" before the process exits.
    tokio::select! {
        result = axum::serve(listener, app) => result?,
        _ = shutdown_signal() => tracing::info!("Shutdown requested"),
    }

    server.io.shutdown().await;
    server.services.shutdown().await;
    Ok(())
}
