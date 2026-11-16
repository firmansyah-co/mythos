//! # MythOS Server Daemon
//!
//! Hosts the HTTP/SSE API and the field I/O runtime that feeds the tag bus.

use mythos_core::api::create_router;
use mythos_core::server::{bootstrap, shm_name_from_env};
use mythos_core::storage::default_projects_dir;
use mythos_pal::DefaultShm;
use std::net::SocketAddr;

/// `--storage-root <dir>` and `--port <n>`, which override `MYTHOS_STORAGE_ROOT` and
/// `MYTHOS_HTTP_PORT`: a service of the operating system starts the server with them
/// (`mythos service install`), so no wrapper process stands between it and the service manager.
struct Args {
    storage_root: Option<std::path::PathBuf>,
    port: Option<u16>,
    https_port: Option<u16>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        storage_root: None,
        port: None,
        https_port: None,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--storage-root" => {
                parsed.storage_root = Some(
                    args.next()
                        .ok_or("--storage-root needs a directory")?
                        .into(),
                )
            }
            "--port" => {
                let text = args.next().ok_or("--port needs a number")?;
                parsed.port = Some(
                    text.parse()
                        .map_err(|_| format!("--port {text} is not a port number"))?,
                );
            }
            "--https-port" => {
                let text = args.next().ok_or("--https-port needs a number")?;
                parsed.https_port = Some(
                    text.parse()
                        .map_err(|_| format!("--https-port {text} is not a port number"))?,
                );
            }
            other => {
                return Err(format!(
                    "unknown argument '{other}' (mythos-server takes --storage-root <dir>, --port <n> and --https-port <n>)"
                ))
            }
        }
    }
    Ok(parsed)
}

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
    let args = parse_args(std::env::args().skip(1))?;

    let storage_root = args
        .storage_root
        .clone()
        .or_else(|| std::env::var_os("MYTHOS_STORAGE_ROOT").map(std::path::PathBuf::from))
        .unwrap_or_else(default_projects_dir);
    let shm_name = shm_name_from_env();

    let server = bootstrap::<DefaultShm>(&storage_root, &shm_name).await?;
    mythos_core::server::announce_setup_code(&server);
    let system_dir = server.state.storage.system_dir();
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
    let pairings = state.pairings.clone();
    let app = create_router(state);
    // ADR-012 §2.6: call home to every paired node this one is told to.
    if let Some(pairings) = pairings {
        mythos_core::callhome::spawn_callers(pairings, app.clone());
    }

    let port: u16 = args
        .port
        .or_else(|| {
            std::env::var("MYTHOS_HTTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
        })
        .unwrap_or(8080);

    // Where to listen is the node's role (ADR-012 §2.1): an edge or a server on every interface,
    // a workstation on this computer only. MYTHOS_BIND overrides it.
    // ADR-012 §2.3: the network uses HTTPS, with this node's own certificate, on the role's
    // address (an edge's or a server's every interface, a workstation's loopback). Plain HTTP is
    // answered on this computer only, unless the node says `http_on_network`.
    let https_port: u16 = args
        .https_port
        .or_else(|| {
            std::env::var("MYTHOS_HTTPS_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
        })
        .unwrap_or(8443);
    let identity = server.node.identity();
    let short: String = identity.id.chars().take(8).collect();
    let cert = mythos_core::tls::NodeCert::load_or_create(
        &system_dir,
        &[format!("mythos-{short}.local")],
    )?;

    let addr = SocketAddr::new(server.node.http_ip()?, port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    server.node.set_listening(listener.local_addr()?);
    let tls_addr = SocketAddr::new(server.node.listen_ip()?, https_port);
    let tls_listener = tokio::net::TcpListener::bind(&tls_addr).await?;
    let tls_bound = tls_listener.local_addr()?;
    server
        .node
        .set_listening_tls(tls_bound, cert.fingerprint.clone());
    // Now that the ports are known, the service modules are told (discovery announces them).
    server.services.node_changed().await;
    tracing::info!("MythOS server listening on http://{}", addr);
    tracing::info!(
        "MythOS server listening on https://{} with certificate {}",
        tls_bound,
        mythos_core::tls::spoken(&cert.fingerprint)
    );
    let tls_server = tokio::spawn(mythos_core::tls::serve(
        tls_listener,
        cert.server_config()?,
        app.clone(),
    ));

    // Telemetry streams never end on their own, so a graceful HTTP shutdown would wait for
    // every browser to close. Stop serving on the signal instead, then let the I/O runtime mark
    // its tags "not connected" before the process exits.
    tokio::select! {
        result = axum::serve(listener, app) => result?,
        _ = shutdown_signal() => tracing::info!("Shutdown requested"),
    }

    tls_server.abort();
    server.io.shutdown().await;
    server.services.shutdown().await;
    Ok(())
}
