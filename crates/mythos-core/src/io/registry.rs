//! The drivers a server can instantiate, by the name a connection uses.
//!
//! In a running server every driver is a module: an executable found in the module
//! directories, run as a separate process (see `mythos_module`). Nothing is compiled into the
//! server. A driver can also be registered in-process, which tests and embedders use to supply
//! one without building an executable.

use mythos_module::{Discovery, ModuleBinary, ModuleKind, ModuleProblem, ProcessDriver};
use mythos_plugin_sdk::ProtocolDriver;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Constructs a fresh, unconfigured in-process driver.
pub type DriverFactory = fn() -> Box<dyn ProtocolDriver>;

#[derive(Clone)]
enum DriverSource {
    InProcess(DriverFactory),
    Module(Arc<ModuleBinary>),
}

/// One entry of the registry, as the API reports it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DriverInfo {
    /// The name a connection's `driver` field uses.
    pub name: String,
    /// `module` for a module executable, `in_process` for one registered in code.
    pub source: &'static str,
    /// The module, when it is one.
    pub module: Option<ModuleBinary>,
}

#[derive(Clone, Default)]
pub struct DriverRegistry {
    drivers: BTreeMap<String, DriverSource>,
    /// Service modules found beside the drivers (ADR-011); `crate::services` runs them.
    services: Vec<Arc<ModuleBinary>>,
    /// Modules found but switched off in Settings → Modules (`crate::modules`).
    disabled: Vec<Arc<ModuleBinary>>,
    problems: Vec<ModuleProblem>,
}

impl DriverRegistry {
    /// A registry with no drivers.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The driver modules found in `dirs`. See [`mythos_module::discover`].
    pub async fn discover(dirs: &[PathBuf]) -> Self {
        Self::from_discovery(mythos_module::discover(dirs).await)
    }

    pub fn from_discovery(discovery: Discovery) -> Self {
        let mut registry = Self {
            drivers: BTreeMap::new(),
            services: Vec::new(),
            disabled: Vec::new(),
            problems: discovery.problems,
        };
        for module in discovery.modules {
            match module.manifest.kind {
                ModuleKind::Driver => {
                    registry.drivers.insert(
                        module.manifest.provides.clone(),
                        DriverSource::Module(Arc::new(module)),
                    );
                }
                ModuleKind::Service => registry.services.push(Arc::new(module)),
            }
        }
        registry
    }

    /// Set aside the modules whose manifest id is in `disabled`: they are listed, not used.
    pub fn apply_disabled(&mut self, disabled: &std::collections::BTreeSet<String>) {
        let mut off = Vec::new();
        self.drivers.retain(|_, source| match source {
            DriverSource::Module(m) if disabled.contains(&m.manifest.id) => {
                off.push(Arc::clone(m));
                false
            }
            _ => true,
        });
        self.services.retain(|m| {
            if disabled.contains(&m.manifest.id) {
                off.push(Arc::clone(m));
                false
            } else {
                true
            }
        });
        self.disabled.extend(off);
    }

    /// The modules found but switched off.
    pub fn disabled_modules(&self) -> &[Arc<ModuleBinary>] {
        &self.disabled
    }

    /// The disabled module that would provide driver `name`, if that is why it is missing.
    pub fn disabled_driver(&self, name: &str) -> Option<&ModuleBinary> {
        self.disabled
            .iter()
            .find(|m| m.manifest.kind == ModuleKind::Driver && m.manifest.provides == name)
            .map(|m| m.as_ref())
    }

    /// Every module found, used or not: its manifest and where it is.
    pub fn all_modules(&self) -> Vec<Arc<ModuleBinary>> {
        let mut all: Vec<Arc<ModuleBinary>> = self
            .drivers
            .values()
            .filter_map(|s| match s {
                DriverSource::Module(m) => Some(Arc::clone(m)),
                DriverSource::InProcess(_) => None,
            })
            .collect();
        all.extend(self.services.iter().cloned());
        all.extend(self.disabled.iter().cloned());
        all
    }

    /// The service modules found.
    pub fn services(&self) -> &[Arc<ModuleBinary>] {
        &self.services
    }

    /// Register an in-process driver under `name`, replacing any driver of that name.
    pub fn register(&mut self, name: impl Into<String>, factory: DriverFactory) {
        self.drivers
            .insert(name.into(), DriverSource::InProcess(factory));
    }

    pub fn create(&self, name: &str) -> Option<Box<dyn ProtocolDriver>> {
        self.drivers.get(name).map(|source| match source {
            DriverSource::InProcess(factory) => factory(),
            DriverSource::Module(module) => {
                Box::new(ProcessDriver::new(Arc::clone(module))) as Box<dyn ProtocolDriver>
            }
        })
    }

    /// Which build of which driver `name` currently is, or `None` when there is no such
    /// driver. Two calls return the same string exactly when the same code would run, so a
    /// change means the connections using the driver should be swapped to the new one.
    pub fn identity(&self, name: &str) -> Option<String> {
        self.drivers.get(name).map(|source| match source {
            DriverSource::InProcess(factory) => {
                format!("in_process:{name}:{:p}", *factory as *const ())
            }
            // The whole fingerprint, so a field added to it counts here without a change.
            DriverSource::Module(module) => format!(
                "module:{}:{}:{}:{:?}",
                module.manifest.id,
                module.manifest.version,
                module.path.display(),
                module.fingerprint
            ),
        })
    }

    /// Registered driver names, sorted.
    pub fn names(&self) -> Vec<String> {
        self.drivers.keys().cloned().collect()
    }

    /// Every registered driver, sorted by name.
    pub fn drivers(&self) -> Vec<DriverInfo> {
        self.drivers
            .iter()
            .map(|(name, source)| match source {
                DriverSource::InProcess(_) => DriverInfo {
                    name: name.clone(),
                    source: "in_process",
                    module: None,
                },
                DriverSource::Module(module) => DriverInfo {
                    name: name.clone(),
                    source: "module",
                    module: Some((**module).clone()),
                },
            })
            .collect()
    }

    /// Executables that looked like modules and could not be used.
    pub fn problems(&self) -> &[ModuleProblem] {
        &self.problems
    }
}

/// Where a server looks for module executables.
///
/// `MYTHOS_MODULE_PATH`, a list of directories separated like `PATH`, when it is set.
/// Otherwise the directory the server executable is in, which is where a build or an installer
/// puts the module executables beside it.
pub fn module_dirs_from_env() -> Vec<PathBuf> {
    if let Some(value) = std::env::var_os("MYTHOS_MODULE_PATH") {
        let dirs: Vec<PathBuf> = std::env::split_paths(&value)
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        if !dirs.is_empty() {
            return dirs;
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .into_iter()
        .collect()
}
