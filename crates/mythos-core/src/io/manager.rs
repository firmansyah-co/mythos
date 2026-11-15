//! Owns the running field I/O of a server and replaces it when the configuration changes.
//!
//! There is one runtime at a time, for the active project. Saving that project's I/O
//! configuration, or activating another project, stops the runtime and starts a new one. The
//! tags of the old runtime are removed from the catalog first, so a reader never finds a tag
//! that nothing is feeding any more.

use crate::io::config::{IoConfig, IO_CONFIG_FILE};
use crate::io::registry::{DriverInfo, DriverRegistry};
use crate::io::runtime::{ConnectionState, IoRuntime, IoStatusHandle};
use crate::storage::ControllerStorage;
use crate::tag::{TagCatalog, TagMetadata};
use mythos_module::ModuleProblem;
use mythos_pal::PlatformShm;
use mythos_shm::ShmWriterHandle;
use serde::Serialize;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};

/// One reason a configuration cannot be run as written.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfigProblem {
    /// The connection the problem is in, or `null` when it concerns the file as a whole.
    pub connection_id: Option<String>,
    pub message: String,
}

struct Running {
    runtime: Option<IoRuntime>,
    /// URIs the current runtime registered, to be removed when it stops.
    tags: Vec<String>,
}

pub struct IoManager<S: PlatformShm + 'static> {
    storage: Arc<ControllerStorage>,
    catalog: Arc<RwLock<TagCatalog>>,
    shm_name: String,
    capacity: usize,
    registry: DriverRegistry,
    status: IoStatusHandle,
    /// Held for the whole of a reload, so two reloads cannot interleave.
    running: Mutex<Running>,
    _shm: PhantomData<fn() -> S>,
}

impl<S: PlatformShm + 'static> IoManager<S> {
    pub fn new(
        storage: Arc<ControllerStorage>,
        catalog: Arc<RwLock<TagCatalog>>,
        shm_name: impl Into<String>,
        capacity: usize,
        registry: DriverRegistry,
    ) -> Arc<Self> {
        Arc::new(Self {
            storage,
            catalog,
            shm_name: shm_name.into(),
            capacity,
            registry,
            status: IoStatusHandle::not_running("field I/O has not been started"),
            running: Mutex::new(Running {
                runtime: None,
                tags: Vec::new(),
            }),
            _shm: PhantomData,
        })
    }

    /// The status of whatever is running. The same handle for the life of the manager.
    pub fn status(&self) -> IoStatusHandle {
        self.status.clone()
    }

    /// Names of the drivers a configuration may use, sorted.
    pub fn driver_names(&self) -> Vec<String> {
        self.registry.names()
    }

    /// Every driver a configuration may use, and the module behind it.
    pub fn drivers(&self) -> Vec<DriverInfo> {
        self.registry.drivers()
    }

    /// Module executables that were found and could not be used.
    pub fn module_problems(&self) -> Vec<ModuleProblem> {
        self.registry.problems().to_vec()
    }

    /// Everything that would stop `config` from running as written. Empty means it can be saved.
    ///
    /// Nothing is started and no device is contacted. The checks are the ones starting the
    /// configuration would make: the file's own rules, each driver's view of its connection,
    /// and that no tag is provided twice or has a name the catalog would refuse.
    pub async fn check(&self, config: &IoConfig) -> Vec<ConfigProblem> {
        Self::check_with(&self.registry, config).await
    }

    /// [`Self::check`] against a given set of drivers, for a server with no running manager.
    pub async fn check_with(registry: &DriverRegistry, config: &IoConfig) -> Vec<ConfigProblem> {
        let mut problems = Vec::new();

        if let Err(e) = config.validate() {
            problems.push(ConfigProblem {
                connection_id: None,
                message: e.to_string(),
            });
        }

        let mut provided_by: HashMap<String, String> = HashMap::new();
        for connection in &config.connections {
            let problem = |message: String| ConfigProblem {
                connection_id: Some(connection.id.clone()),
                message,
            };

            let Some(driver) = registry.create(&connection.driver) else {
                problems.push(problem(format!(
                    "unknown driver '{}' (available: {})",
                    connection.driver,
                    registry.names().join(", ")
                )));
                continue;
            };

            let declarations = match driver.describe_config(&connection.config).await {
                Ok(declarations) => declarations,
                Err(e) => {
                    problems.push(problem(e.to_string()));
                    continue;
                }
            };

            if declarations.is_empty() {
                problems.push(problem("the connection declares no tags".to_string()));
            }

            for declaration in declarations {
                if let Err(e) = TagMetadata::new(declaration.uri.clone(), 0) {
                    problems.push(problem(format!(
                        "tag '{}' is not valid: {}",
                        declaration.uri, e
                    )));
                    continue;
                }
                if let Some(other) =
                    provided_by.insert(declaration.uri.clone(), connection.id.clone())
                {
                    problems.push(problem(format!(
                        "tag '{}' is already provided by connection '{}'",
                        declaration.uri, other
                    )));
                }
            }
        }

        problems
    }

    /// Stop whatever is running and start the active project's configuration.
    ///
    /// Always leaves the status describing the result: the new connections, or why there are
    /// none.
    pub async fn reload(&self) {
        let mut running = self.running.lock().await;

        self.stop_locked(&mut running).await;

        let active = match self.storage.get_active_project() {
            Ok(Some(name)) => name,
            Ok(None) => {
                return self
                    .idle("no project is active, so no I/O configuration is loaded".to_string());
            }
            Err(e) => {
                return self.idle(format!("the active project could not be resolved: {e}"));
            }
        };

        let path = match self.storage.project_dir(&active) {
            Ok(dir) => dir.join(IO_CONFIG_FILE),
            Err(e) => return self.idle(format!("the active project cannot be used: {e}")),
        };

        let config = match IoConfig::load(&path) {
            Ok(Some(config)) => config,
            Ok(None) => {
                return self.idle(format!("active project '{active}' has no {IO_CONFIG_FILE}"));
            }
            Err(e) => {
                error!("Field I/O configuration rejected: {}", e);
                return self.idle(e.to_string());
            }
        };

        let writer = match ShmWriterHandle::<S>::new(&self.shm_name, self.capacity) {
            Ok(writer) => writer,
            Err(e) => {
                error!(
                    "Field I/O cannot open the tag bus '{}': {}",
                    self.shm_name, e
                );
                return self.idle(format!(
                    "the tag bus '{}' is unavailable: {e}",
                    self.shm_name
                ));
            }
        };

        let runtime = IoRuntime::start(
            config,
            Some(path.display().to_string()),
            &self.registry,
            Arc::clone(&self.catalog),
            writer,
            self.status.clone(),
        )
        .await;

        running.tags = self
            .status
            .snapshot()
            .connections
            .into_iter()
            .filter(|connection| connection.state == ConnectionState::Running)
            .flat_map(|connection| connection.tags)
            .collect();
        running.runtime = Some(runtime);
        info!(
            "Field I/O started for project '{}' with {} tag(s)",
            active,
            running.tags.len()
        );
    }

    /// Stop whatever is running, for process shutdown.
    pub async fn shutdown(&self) {
        let mut running = self.running.lock().await;
        self.stop_locked(&mut running).await;
        self.status.set_not_running("field I/O has been shut down");
    }

    /// Stop the current runtime and remove its tags from the catalog.
    ///
    /// The runtime marks every slot "not connected" as it ends. The tags are unregistered
    /// after that, so there is no moment at which a registered tag reads as GOOD with nothing
    /// feeding it.
    async fn stop_locked(&self, running: &mut Running) {
        if let Some(runtime) = running.runtime.take() {
            runtime.shutdown().await;
        }
        if !running.tags.is_empty() {
            let mut catalog = self.catalog.write().await;
            for uri in running.tags.drain(..) {
                catalog.unregister(&uri);
            }
        }
    }

    fn idle(&self, note: String) {
        info!("Field I/O is not running: {}", note);
        self.status.set_not_running(note);
    }
}
