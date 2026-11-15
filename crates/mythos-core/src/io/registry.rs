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
            problems: discovery.problems,
        };
        for module in discovery.modules {
            if module.manifest.kind == ModuleKind::Driver {
                registry.drivers.insert(
                    module.manifest.provides.clone(),
                    DriverSource::Module(Arc::new(module)),
                );
            }
        }
        registry
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
            DriverSource::Module(module) => format!(
                "module:{}:{}:{}:{}:{}",
                module.manifest.id,
                module.manifest.version,
                module.path.display(),
                module.fingerprint.len,
                module.fingerprint.modified_unix_ns
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
