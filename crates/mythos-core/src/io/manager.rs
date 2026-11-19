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
    /// Told the service modules of every new registry (`crate::services`).
    on_services: StdRwLock<Option<ServicesHook>>,
    /// Which modules are switched off (Settings → Modules); applied to every reading.
    module_settings: StdRwLock<Option<Arc<crate::modules::ModuleSettings>>>,
    /// Why this node reads no field devices, when it reads none (ADR-012 §2.1: a server's
    /// default). `None`: field I/O runs.
    field_io_off: StdRwLock<Option<String>>,
    /// What the runtime drivers of every registry are given (ADR-013 §2.4): this server's tag
    /// bus to read, its field I/O to write through, and where they report.
    runtime_env: crate::runtimes::RuntimeEnv,
    _shm: PhantomData<fn() -> S>,
}

/// Called with the service modules each time the module directories are read again.
pub type ServicesHook = Arc<dyn Fn(Vec<Arc<mythos_module::ModuleBinary>>) + Send + Sync>;

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
        mut registry: DriverRegistry,
        module_dirs: Option<Vec<PathBuf>>,
    ) -> Arc<Self> {
        let shm_name = shm_name.into();
        let status = IoStatusHandle::not_running("field I/O has not been started");
        let runtime_env = crate::runtimes::RuntimeEnv {
            tags: Arc::new(crate::runtimes::BusTags::<S>::new(
                Arc::clone(&catalog),
                &shm_name,
                capacity,
            )),
            io: status.clone(),
            states: crate::runtimes::RuntimeStates::default(),
            retain: Some(crate::runtimes::RetainStore::new(Arc::clone(&storage))),
        };
        registry.set_runtime_env(runtime_env.clone());
        Arc::new(Self {
            storage,
            catalog,
            shm_name,
            capacity,
            registry: StdRwLock::new(registry),
            module_dirs,
            status,
            runtime_env,
            running: Mutex::new(Running {
                runtime: None,
                config: None,
            }),
            on_services: StdRwLock::new(None),
            module_settings: StdRwLock::new(None),
            field_io_off: StdRwLock::new(None),
            _shm: PhantomData,
        })
    }

    /// Leave out, from every later reading of the module directories, the modules `settings`
    /// switches off.
    pub fn use_module_settings(&self, settings: Arc<crate::modules::ModuleSettings>) {
        *self
            .module_settings
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(settings);
    }

    /// Every module the last reading found, used or switched off.
    pub fn all_modules(&self) -> Vec<Arc<mythos_module::ModuleBinary>> {
        self.registry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .all_modules()
    }

    /// The modules the last reading found switched off.
    pub fn disabled_modules(&self) -> Vec<Arc<mythos_module::ModuleBinary>> {
        self.registry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .disabled_modules()
            .to_vec()
    }

    /// Tell `hook` the service modules found by every later rescan.
    pub fn when_services_change(&self, hook: ServicesHook) {
        *self.on_services.write().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    }

    /// The service modules the last reading of the module directories found.
    pub fn service_modules(&self) -> Vec<Arc<mythos_module::ModuleBinary>> {
        self.registry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .services()
            .to_vec()
    }

    /// The status of whatever is running. The same handle for the life of the manager.
    /// The runtimes of this server: what each is doing, and the way to command them.
    pub fn runtimes(&self) -> crate::runtimes::RuntimeStates {
        self.runtime_env.states.clone()
    }

    /// The runtime modules found, each with what it can run.
    pub fn runtime_modules(&self) -> Vec<Arc<mythos_module::ModuleBinary>> {
        self.registry
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .runtimes()
    }

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

    /// What is wrong with a project's runtimes, asked of the runtime module each names: the
    /// module loads the program without running it. An empty answer means every instance
    /// would start.
    pub async fn check_runtimes(
        &self,
        config: &crate::runtimes::RuntimesConfig,
    ) -> Vec<ConfigProblem> {
        let registry = self.registry();
        let mut problems = Vec::new();
        for connection in config.connections() {
            let problem = |message: String| ConfigProblem {
                connection_id: Some(connection.id.clone()),
                message,
            };
            let Some(driver) = registry.create(&connection.driver) else {
                let available: Vec<String> = registry
                    .runtimes()
                    .iter()
                    .map(|m| m.manifest.provides.clone())
                    .collect();
                problems.push(problem(format!(
                    "no runtime module provides '{}' (available: {})",
                    connection
                        .driver
                        .trim_start_matches(crate::runtimes::CONNECTION_PREFIX),
                    if available.is_empty() {
                        "none".to_string()
                    } else {
                        available.join(", ")
                    }
                )));
                continue;
            };
            match driver.describe_config(&connection.config).await {
                Ok(declarations) => {
                    for declaration in declarations {
                        if let Err(e) = TagMetadata::new(declaration.uri.clone(), 0) {
                            problems.push(problem(format!(
                                "tag '{}' is not valid: {}",
                                declaration.uri, e
                            )));
                        }
                    }
                }
                Err(e) => problems.push(problem(e.to_string())),
            }
        }
        problems
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
        self.reload_with(false).await
    }

    /// [`Self::reload`]; with `cold`, every runtime it starts or replaces starts cold: every
    /// variable its initial value, RETAIN ones included, and none carried from a running build.
    pub async fn reload_with(&self, cold: bool) -> Option<ApplyReport> {
        let mut running = self.running.lock().await;

        let off = self.field_io_off.read().unwrap().clone();
        if let Some(reason) = off {
            return self.idle(&mut running, reason).await;
        }

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

        let project_dir = self.storage.project_dir(&active);
        // The project's runtimes (ADR-013): each becomes a connection beside the field ones.
        // A file that cannot be used starts none of them and says why; field I/O still runs.
        let runtimes = match &project_dir {
            Ok(dir) => match crate::runtimes::RuntimesConfig::load(
                &dir.join(crate::runtimes::RUNTIMES_FILE),
            ) {
                Ok(config) => {
                    self.runtime_env.states.set_problem(None);
                    config.unwrap_or_default()
                }
                Err(e) => {
                    error!("Runtimes configuration rejected: {}", e);
                    self.runtime_env.states.set_problem(Some(e));
                    Default::default()
                }
            },
            Err(_) => Default::default(),
        };
        let path = match project_dir {
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
            Ok(Some(mut config)) => {
                config.connections.extend(runtimes.connections());
                config
            }
            // A project may run programs and read no field device.
            Ok(None) if !runtimes.instances.is_empty() => IoConfig {
                connections: runtimes.connections(),
            },
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
            .apply_locked(&mut running, config, Some(path.display().to_string()), cold)
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

    /// Switch field I/O off on this node, saying why (`Some`), or on again (`None`). Takes
    /// effect at the next [`Self::reload`]: off stops every connection; on starts the active
    /// project's.
    pub fn set_field_io_off(&self, reason: Option<String>) {
        *self.field_io_off.write().unwrap() = reason;
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
        let mut registry = DriverRegistry::discover(dirs).await;
        registry.set_runtime_env(self.runtime_env.clone());
        let settings = self
            .module_settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(settings) = settings {
            registry.apply_disabled(&settings.disabled());
        }
        for problem in registry.problems() {
            warn!(
                "Module {} cannot be used: {}",
                problem.path.display(),
                problem.message
            );
        }
        let services = registry.services().to_vec();
        *self.registry.write().unwrap_or_else(|e| e.into_inner()) = registry;
        let hook = self
            .on_services
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(hook) = hook {
            hook(services);
        }

        let mut running = self.running.lock().await;
        let applied = match running.config.clone() {
            Some((config, source)) if running.runtime.is_some() => {
                self.apply_locked(&mut running, config, source, false).await
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
    /// removed or replaced, once the change has held still for one look: a change takes effect
    /// one to two `every` after it is made. Checking costs a directory listing and a `stat` per
    /// module.
    pub fn spawn_module_watcher(self: &Arc<Self>, every: Duration) -> Option<JoinHandle<()>> {
        let dirs = self.module_dirs.clone()?;
        let manager = Arc::downgrade(self);
        // The baseline is what is installed now, when the watch is asked for, not whenever the
        // task first gets to run: a module replaced in between would otherwise never be seen.
        // A change is acted on once it has held still for one look (see `Settle`): replacing a
        // module on Windows leaves it missing for a moment, and that must not stop it.
        let mut settle = mythos_module::Settle::new(mythos_module::snapshot(&dirs));
        Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(every).await;
                let Some(manager) = manager.upgrade() else {
                    return;
                };
                if settle.observe(mythos_module::snapshot(&dirs)).is_some() {
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
        cold: bool,
    ) -> Option<ApplyReport> {
        let registry = self.registry();
        // How each runtime it starts begins is recorded as it starts, under this lock: no other
        // load runs beside it.
        self.runtime_env.states.begin_loads(cold);
        let report = self.apply_runtime(running, config, source, &registry).await;
        let loads = self.runtime_env.states.end_loads();
        report.map(|mut report| {
            report.runtimes = loads;
            report
        })
    }

    async fn apply_runtime(
        &self,
        running: &mut Running,
        config: IoConfig,
        source: Option<String>,
        registry: &DriverRegistry,
    ) -> Option<ApplyReport> {
        let report = match running.runtime.as_mut() {
            Some(runtime) => {
                runtime
                    .apply(config.clone(), source.clone(), registry)
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
                    registry,
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
