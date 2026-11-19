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
    /// This installation as a node (ADR-012): where to listen comes from its role.
    pub node: Arc<crate::node::Node>,
    pub io: Arc<IoManager<S>>,
    /// The service modules (ADR-011); stop them with [`ServiceHost::shutdown`] on exit.
    pub services: Arc<crate::services::ServiceHost>,
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
    storage_root: &std::path::Path,
    shm_name: &str,
) -> Result<Server<S>, StorageError> {
    bootstrap_as(storage_root, shm_name, crate::node::NodeRole::Edge).await
}

/// [`bootstrap`] for a node whose role, on its first start, is `first_role` (the desktop app is
/// a workstation; `mythos-server` an edge). `MYTHOS_ROLE` overrides it; `node.json` wins once it
/// exists.
pub async fn bootstrap_as<S: PlatformShm + 'static>(
    storage_root: &std::path::Path,
    shm_name: &str,
    first_role: crate::node::NodeRole,
) -> Result<Server<S>, StorageError> {
    // Packages installed in Settings → Modules live in the storage's system directory, which is
    // always searched, so an update of MythOS (a new version directory) keeps them.
    let mut dirs = module_dirs_from_env();
    let install_dir = ControllerStorage::new(storage_root)?
        .system_dir()
        .join("modules");
    std::fs::create_dir_all(&install_dir)?;
    if !dirs.contains(&install_dir) {
        dirs.push(install_dir);
    }
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
    let server = assemble(storage_root, shm_name, registry, Some(dirs), first_role).await?;
    server.io.spawn_module_watcher(MODULE_WATCH_INTERVAL);
    Ok(server)
}

/// Start logging: `RUST_LOG` when set, otherwise `info`. A bare `tracing_subscriber::fmt::init()`
/// logs only errors when `RUST_LOG` is unset, which hid every warning, the setup code among them.
pub fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

/// Print the setup code where the person starting the server sees it, whatever the log level:
/// without it nobody can create the first administrator.
pub fn announce_setup_code<S: PlatformShm + 'static>(server: &Server<S>) {
    if let Some(code) = server.state.auth.as_ref().and_then(|a| a.setup_code()) {
        eprintln!(
            "MythOS has no account yet. Open the workbench and create the first administrator \
             with setup code {code}"
        );
    }
}

/// How often the module directories are checked for a module added, removed or replaced.
pub const MODULE_WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// [`bootstrap`] with a fixed set of drivers and no module directories to watch.
pub async fn bootstrap_with<S: PlatformShm + 'static>(
    storage_root: &std::path::Path,
    shm_name: &str,
    registry: DriverRegistry,
) -> Result<Server<S>, StorageError> {
    assemble(
        storage_root,
        shm_name,
        registry,
        None,
        crate::node::NodeRole::Edge,
    )
    .await
}

async fn assemble<S: PlatformShm + 'static>(
    storage_root: &std::path::Path,
    shm_name: &str,
    mut registry: DriverRegistry,
    module_dirs: Option<Vec<std::path::PathBuf>>,
    first_role: crate::node::NodeRole,
) -> Result<Server<S>, StorageError> {
    let storage = Arc::new(ControllerStorage::new(storage_root)?);
    let node = Arc::new(
        crate::node::Node::open(&storage.system_dir(), first_role)
            .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?,
    );
    let identity = node.identity();
    info!(
        "This node is '{}' ({}), role {}",
        identity.name,
        identity.id,
        identity.role.as_str()
    );
    let module_settings = Arc::new(
        crate::modules::ModuleSettings::open(&storage.system_dir())
            .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?,
    );
    registry.apply_disabled(&module_settings.disabled());
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

    let io = match module_dirs {
        Some(dirs) => IoManager::<S>::with_module_dirs(
            Arc::clone(&storage),
            Arc::clone(&tag_catalog),
            shm_name,
            TAG_CAPACITY,
            registry,
            dirs,
        ),
        None => IoManager::<S>::new(
            Arc::clone(&storage),
            Arc::clone(&tag_catalog),
            shm_name,
            TAG_CAPACITY,
            registry,
        ),
    };
    io.use_module_settings(Arc::clone(&module_settings));
    // A node whose role (or own setting) reads no field devices starts none (ADR-012 §2.1).
    io.set_field_io_off(identity.field_io_off_reason());
    io.reload().await;

    // Accounts are not optional for a server: every route is checked against its policy.
    let accounts = crate::auth::Auth::open(&storage.system_dir())
        .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?;
    // A paired node's token acts with the role it was granted (ADR-012 §2.3).
    let pairings = Arc::new(
        crate::pairing::Pairings::open(&storage.system_dir())
            .map_err(|e| StorageError::Io(std::io::Error::other(e.to_string())))?,
    );
    {
        let pairings = Arc::clone(&pairings);
        accounts.set_pairing_lookup(Arc::new(move |token| pairings.identify(token)));
    }
    if let Some(code) = accounts.setup_code() {
        warn!(
            "No account exists yet. Open the workbench and create the first administrator with \
             setup code {code} (set {} to choose it).",
            crate::auth::SETUP_CODE_ENV
        );
    }

    let state = ApiState::new(storage, tag_catalog, shm_reader);
    // Service modules are found with the drivers, and follow every later reading of the module
    // directories (the watcher, or POST /api/v1/modules/rescan).
    let services = crate::services::ServiceHost::new(state.hmi_events.clone());
    {
        let node = Arc::clone(&node);
        services.set_node(Arc::new(move || Some(node.wire())));
    }
    services.sync(io.service_modules()).await;
    {
        let services = Arc::clone(&services);
        io.when_services_change(Arc::new(move |modules| {
            let services = Arc::clone(&services);
            tokio::spawn(async move { services.sync(modules).await });
        }));
    }
    let state = state
        .with_modules(module_settings)
        .with_services(Arc::clone(&services))
        .with_io_manager(Arc::clone(&io))
        .with_auth(Arc::new(accounts))
        .with_updates(Arc::new(crate::updates::Updates::detect()))
        .with_node(Arc::clone(&node))
        .with_pairings(pairings);

    Ok(Server {
        state,
        node,
        io,
        services,
    })
}
