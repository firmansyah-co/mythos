//! Owns the running field I/O of a server and keeps it matching the configuration and the
//! installed modules.
//!
//! There is one runtime, for the active project. Saving that project's I/O configuration,
//! activating another project, or installing a new build of a driver module changes only the
//! connections it affects (see [`IoRuntime::apply`]).

use crate::io::config::{IoConfig, IO_CONFIG_FILE};
use crate::io::registry::{DriverInfo, DriverRegistry};
use crate::io::runtime::{ApplyReport, IoRuntime, IoStatusHandle};
use crate::storage::ControllerStorage;
use crate::tag::{TagCatalog, TagMetadata};
use mythos_module::ModuleProblem;
use mythos_pal::PlatformShm;
use mythos_shm::ShmWriterHandle;
use serde::Serialize;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// One reason a configuration cannot be run as written.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfigProblem {
    /// The connection the problem is in, or `null` when it concerns the file as a whole.
    pub connection_id: Option<String>,
    pub message: String,
}

/// What a rescan of the module directories found and changed.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RescanReport {
    pub drivers: Vec<DriverInfo>,
    pub problems: Vec<ModuleProblem>,
    /// What happened to the running connections, when field I/O is running.
    pub applied: Option<ApplyReport>,
}

struct Running {
    runtime: Option<IoRuntime>,
    /// The configuration last applied and the file it came from, so a module change can be
    /// applied to the same configuration.
    config: Option<(IoConfig, Option<String>)>,
}

pub struct IoManager<S: PlatformShm + 'static> {
    storage: Arc<ControllerStorage>,
    catalog: Arc<RwLock<TagCatalog>>,
    shm_name: String,
    capacity: usize,
    registry: StdRwLock<DriverRegistry>,
    /// Where modules are found, when this manager may look again.
    module_dirs: Option<Vec<PathBuf>>,
    status: IoStatusHandle,
    /// Held for the whole of a reload, so two reloads cannot interleave.
    running: Mutex<Running>,
    _shm: PhantomData<fn() -> S>,
}

impl<S: PlatformShm + 'static> IoManager<S> {
    /// A manager with a fixed set of drivers. [`Self::rescan`] leaves it as it is.
    pub fn new(
        storage: Arc<ControllerStorage>,
        catalog: Arc<RwLock<TagCatalog>>,
        shm_name: impl Into<String>,
        capacity: usize,
        registry: DriverRegistry,
    ) -> Arc<Self> {
        Self::build(storage, catalog, shm_name, capacity, registry, None)
    }

    /// A manager whose drivers are the modules in `dirs`, found again by [`Self::rescan`].
    pub fn with_module_dirs(
        storage: Arc<ControllerStorage>,
        catalog: Arc<RwLock<TagCatalog>>,
        shm_name: impl Into<String>,
        capacity: usize,
        registry: DriverRegistry,
        dirs: Vec<PathBuf>,
    ) -> Arc<Self> {
        Self::build(storage, catalog, shm_name, capacity, registry, Some(dirs))
    }

    fn build(
        storage: Arc<ControllerStorage>,
        catalog: Arc<RwLock<TagCatalog>>,
        shm_name: impl Into<String>,
        capacity: usize,
        registry: DriverRegistry,
        module_dirs: Option<Vec<PathBuf>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            storage,
            catalog,
            shm_name: shm_name.into(),
            capacity,
            registry: StdRwLock::new(registry),
            module_dirs,
            status: IoStatusHandle::not_running("field I/O has not been started"),
            running: Mutex::new(Running {
                runtime: None,
                config: None,
            }),
            _shm: PhantomData,
        })
    }

    /// The status of whatever is running. The same handle for the life of the manager.
    pub fn status(&self) -> IoStatusHandle {
        self.status.clone()
    }

    fn registry(&self) -> DriverRegistry {
        self.registry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Names of the drivers a configuration may use, sorted.
    pub fn driver_names(&self) -> Vec<String> {
        self.registry().names()
    }

    /// Every driver a configuration may use, and the module behind it.
    pub fn drivers(&self) -> Vec<DriverInfo> {
        self.registry().drivers()
    }

    /// Module executables that were found and could not be used.
    pub fn module_problems(&self) -> Vec<ModuleProblem> {
        self.registry().problems().to_vec()
    }

    /// Everything that would stop `config` from running as written. Empty means it can be saved.
    ///
    /// Nothing is started that stays running, and no device is contacted. The checks are the
    /// ones starting the configuration would make: the file's own rules, each driver's view of
    /// its connection, and that no tag is provided twice or has a name the catalog would refuse.
    pub async fn check(&self, config: &IoConfig) -> Vec<ConfigProblem> {
        Self::check_with(&self.registry(), config).await
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

    /// Make field I/O match the active project's configuration.
    ///
    /// Only the connections that differ from what runs are touched. Always leaves the status
    /// describing the result: the connections, or why there are none. Returns what changed, or
    /// `None` when no configuration is in effect.
    pub async fn reload(&self) -> Option<ApplyReport> {
        let mut running = self.running.lock().await;

        let active = match self.storage.get_active_project() {
            Ok(Some(name)) => name,
            Ok(None) => {
                return self
                    .idle(
                        &mut running,
                        "no project is active, so no I/O configuration is loaded".to_string(),
                    )
                    .await;
            }
            Err(e) => {
                return self
                    .idle(
                        &mut running,
                        format!("the active project could not be resolved: {e}"),
                    )
                    .await;
            }
        };

        let path = match self.storage.project_dir(&active) {
            Ok(dir) => dir.join(IO_CONFIG_FILE),
            Err(e) => {
                return self
                    .idle(
                        &mut running,
                        format!("the active project cannot be used: {e}"),
                    )
                    .await
            }
        };

        let config = match IoConfig::load(&path) {
            Ok(Some(config)) => config,
            Ok(None) => {
                return self
                    .idle(
                        &mut running,
                        format!("active project '{active}' has no {IO_CONFIG_FILE}"),
                    )
                    .await;
            }
            Err(e) => {
                error!("Field I/O configuration rejected: {}", e);
                return self.idle(&mut running, e.to_string()).await;
            }
        };

        let report = self
            .apply_locked(&mut running, config, Some(path.display().to_string()))
            .await?;
        info!(
            "Field I/O for project '{}': kept {:?}, swapped {:?}, restarted {:?}, started {:?}, stopped {:?}, failed {:?}",
            active,
            report.kept,
            report.swapped,
            report.restarted,
            report.started,
            report.stopped,
            report.failed
        );
        Some(report)
    }

    /// Look for modules again. Connections whose driver changed (a new build, a new version, a
    /// module that is gone) are swapped to what is installed now; the others are not touched.
    ///
    /// A manager built with [`Self::new`] has no directories to look in; it reports its
    /// drivers and changes nothing.
    pub async fn rescan(&self) -> RescanReport {
        let Some(dirs) = &self.module_dirs else {
            return RescanReport {
                drivers: self.drivers(),
                problems: self.module_problems(),
                applied: None,
            };
        };
        let registry = DriverRegistry::discover(dirs).await;
        for problem in registry.problems() {
            warn!(
                "Module {} cannot be used: {}",
                problem.path.display(),
                problem.message
            );
        }
        *self.registry.write().unwrap_or_else(|e| e.into_inner()) = registry;

        let mut running = self.running.lock().await;
        let applied = match running.config.clone() {
            Some((config, source)) if running.runtime.is_some() => {
                self.apply_locked(&mut running, config, source).await
            }
            _ => None,
        };
        drop(running);
        if let Some(report) = &applied {
            if !report.swapped.is_empty() || !report.restarted.is_empty() {
                info!(
                    "Modules changed: swapped {:?}, restarted {:?}",
                    report.swapped, report.restarted
                );
            }
        }
        RescanReport {
            drivers: self.drivers(),
            problems: self.module_problems(),
            applied,
        }
    }

    /// Watch the module directories and [`Self::rescan`] whenever a module executable is added,
    /// removed or replaced. Checking costs a directory listing and a `stat` per module.
    pub fn spawn_module_watcher(self: &Arc<Self>, every: Duration) -> Option<JoinHandle<()>> {
        let dirs = self.module_dirs.clone()?;
        let manager = Arc::downgrade(self);
        // The baseline is what is installed now, when the watch is asked for, not whenever the
        // task first gets to run: a module replaced in between would otherwise never be seen.
        let mut last = mythos_module::snapshot(&dirs);
        Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                let Some(manager) = manager.upgrade() else {
                    return;
                };
                let now = mythos_module::snapshot(&dirs);
                if now != last {
                    last = now;
                    info!("Module directories changed; looking for modules again");
                    manager.rescan().await;
                }
            }
        }))
    }

    /// Stop whatever is running, for process shutdown.
    pub async fn shutdown(&self) {
        let mut running = self.running.lock().await;
        if let Some(runtime) = running.runtime.take() {
            runtime.shutdown().await;
        }
        running.config = None;
        self.status.set_not_running("field I/O has been shut down");
    }

    async fn apply_locked(
        &self,
        running: &mut Running,
        config: IoConfig,
        source: Option<String>,
    ) -> Option<ApplyReport> {
        let registry = self.registry();
        let report = match running.runtime.as_mut() {
            Some(runtime) => {
                runtime
                    .apply(config.clone(), source.clone(), &registry)
                    .await
            }
            None => {
                let writer = match ShmWriterHandle::<S>::new(&self.shm_name, self.capacity) {
                    Ok(writer) => writer,
                    Err(e) => {
                        error!(
                            "Field I/O cannot open the tag bus '{}': {}",
                            self.shm_name, e
                        );
                        self.idle(
                            running,
                            format!("the tag bus '{}' is unavailable: {e}", self.shm_name),
                        )
                        .await;
                        return None;
                    }
                };
                let (runtime, report) = IoRuntime::start_with_report(
                    config.clone(),
                    source.clone(),
                    &registry,
                    Arc::clone(&self.catalog),
                    writer,
                    self.status.clone(),
                )
                .await;
                running.runtime = Some(runtime);
                report
            }
        };
        running.config = Some((config, source));
        Some(report)
    }

    async fn idle(&self, running: &mut Running, note: String) -> Option<ApplyReport> {
        if let Some(runtime) = running.runtime.take() {
            runtime.shutdown().await;
        }
        running.config = None;
        info!("Field I/O is not running: {}", note);
        self.status.set_not_running(note);
        None
    }
}
