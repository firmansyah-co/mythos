//! Runtimes (ADR-013 §2.4, §2.5): the programs this node executes continuously against its
//! tags, each in a runtime module process.
//!
//! A runtime instance is hosted as a connection of the field I/O runtime. A [`RuntimeDriver`]
//! wraps the module process and gives the I/O runtime what it gives any driver: the tags the
//! program publishes are its declared tags, and its poll cycle is one *exchange* with the
//! module. So everything the read path promises holds for a runtime's tags without being
//! written twice: slots and the catalog, a deadline per cycle, `BAD_COMM_FAILURE` when the
//! process dies, a restart with the same program, and replacement in place when a new build
//! or program publishes the same tags.
//!
//! What a runtime adds to a driver:
//!
//! - **It reads tags.** Each exchange carries the tags its program is bound to, as they are.
//! - **It owns outputs.** The field tags its program writes are claimed for it: nobody else,
//!   an operator included, writes them while it runs them.
//! - **Safe values.** Whenever the program is not running — stopped, faulted, its process
//!   dead or silent past the exchange deadline, or the instance removed — every output is
//!   written its safe value, and written again each cycle until the device confirms.
//!
//! This is not a safety system (ADR-013 §2.11): if this server itself stops, nothing here
//! writes anything.

use crate::io::{ConnectionConfig, IoStatusHandle, WriteError};
use crate::storage::ControllerStorage;
use crate::tag::TagCatalog;
use mythos_module::protocol::{
    check_tasks, implied_task, Call, CarryReport, ExchangeReply, ForcedValue, NotCarried,
    OutputBinding, ProgramVariables, ProgramVersion, RetainedValue, RuntimeMode, RuntimeProgram,
    RuntimeState, TaskConfig, TaskKind, TaskState, TaskSupport, WatchReply, WireInput,
};
use mythos_module::{ModuleBinary, ProcessDriver};
use mythos_pal::PlatformShm;
use mythos_plugin_sdk::{
    async_trait, MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata,
    ProtocolDriver, TagDeclaration, TagSample,
};
use mythos_shm::{QualityCode, ShmReaderHandle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Mutex, Notify, RwLock};

/// The project file that says which runtimes run, beside `io.json`.
pub const RUNTIMES_FILE: &str = "runtimes.json";
/// A runtime instance `x` is the connection `runtime:x` of the field I/O runtime, and its
/// driver is `runtime:<module>`.
pub const CONNECTION_PREFIX: &str = "runtime:";
/// How long a runtime module may take to answer one exchange. Past it the program is taken to
/// be not running and its outputs are made safe. Well inside the connection's cycle deadline,
/// so that making them safe happens in this cycle and is not cancelled with it.
const EXCHANGE_DEADLINE: Duration = Duration::from_millis(1_000);
/// How long a runtime goes on being asked for the variables somebody watched, after the last
/// time they were asked for. A watch nobody renews costs the runtime nothing.
pub const WATCH_KEPT: Duration = Duration::from_secs(5);
/// The most variables one watch may name.
pub const WATCH_LIMIT: usize = 256;
/// How often a program's RETAIN values are asked for and, when they changed, kept on disk. A
/// restart of the server or the module takes the values kept last: at most this old.
const RETAIN_EVERY: Duration = Duration::from_secs(2);
/// The connection's cycle deadline: one exchange and the writes it leads to.
const CYCLE_TIMEOUT_MS: u64 = 10_000;
/// How long a new build waits for the running one to give its variables, which it does at its
/// next exchange (every 200 ms at the most).
const CARRY_WAIT: Duration = Duration::from_millis(2_500);

pub fn is_runtime_connection(id: &str) -> bool {
    id.starts_with(CONNECTION_PREFIX)
}

// -----------------------------------------------------------------------------
// Configuration
// -----------------------------------------------------------------------------

/// The programs of one runtime module process, and the tasks that run them (ADR-013 §2.19).
///
/// A runtime of today has one `program` and no `tasks`: it runs in its implied task, cyclic at
/// the program's interval. A runtime of several programs gives them in `programs` and says in
/// `tasks` which task runs each. A file of today reads and writes back unchanged, because the
/// fields it does not use are left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeInstance {
    /// Its name, unique within the project: letters, digits, `-` and `_`.
    pub id: String,
    /// Which runtime module runs it: a module's `provides`, for example `vm`.
    pub module: String,
    /// Whether it is put in RUN when it is loaded. Required: whether a program starts by
    /// itself is a decision, not a default.
    pub run: bool,
    /// Its one program; or `programs`, never both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<RuntimeProgram>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<RuntimeProgram>,
    /// The tasks that run its programs; none for one program in its implied task. When tasks
    /// are given, a program's `interval_ms` is not used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<TaskConfig>,
}

impl RuntimeInstance {
    /// Its programs: `program`, else `programs`.
    pub fn all_programs(&self) -> Vec<&RuntimeProgram> {
        match &self.program {
            Some(program) => vec![program],
            None => self.programs.iter().collect(),
        }
    }

    /// The tasks it runs: those written out, or the implied one of its one program.
    pub fn effective_tasks(&self) -> Vec<TaskConfig> {
        if !self.tasks.is_empty() {
            return self.tasks.clone();
        }
        match self.all_programs().as_slice() {
            [program] => vec![implied_task(program)],
            _ => Vec::new(),
        }
    }

    /// Whether a runtime module that runs one program in one cyclic task runs it exactly as
    /// written: one program, and no task, or one cyclic task at the program's interval with no
    /// phase, the default watchdog and only that program. Any priority: with one task it does
    /// not matter.
    pub fn runs_as_today(&self) -> bool {
        let programs = self.all_programs();
        let [program] = programs.as_slice() else {
            return false;
        };
        match self.tasks.as_slice() {
            [] => true,
            [task] => {
                matches!(task.kind, TaskKind::Cyclic { interval_ms, phase_ms: 0 }
                    if interval_ms == program.interval_ms)
                    && task.watchdog_ms.is_none()
                    && matches!(task.programs.as_slice(), [name] if name.eq_ignore_ascii_case(&program.name))
            }
            _ => false,
        }
    }

    /// What a running build must also run for a new build to carry on from its variables:
    /// the names of its programs, sorted, joined by `,`. For one program, its name.
    pub fn program_key(&self) -> String {
        let mut names: Vec<&str> = self
            .all_programs()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        names.sort_unstable();
        names.join(",")
    }

    /// Whether its variables are named `<program>.<path>`: it has two or more programs.
    fn several(&self) -> bool {
        self.all_programs().len() > 1
    }

    /// Who owns its outputs and published tags, in a message: `'<id>'`, or `'<id>/<program>'`
    /// when it has several programs.
    fn owner_of(&self, program: &RuntimeProgram) -> String {
        if self.several() {
            format!("{}/{}", self.id, program.name)
        } else {
            self.id.clone()
        }
    }

    /// The name of a variable of `program` as the runtime's module names it.
    fn variable_name(&self, program: &RuntimeProgram, variable: &str) -> String {
        if self.several() {
            format!("{}.{variable}", program.name)
        } else {
            variable.to_string()
        }
    }

    /// Every output of every program, with the program it is of.
    fn outputs(&self) -> Vec<(&RuntimeProgram, &OutputBinding)> {
        self.all_programs()
            .into_iter()
            .flat_map(|p| p.outputs.iter().map(move |o| (p, o)))
            .collect()
    }

    /// Its program and version as one line each: the one program's, or every program's
    /// joined by `, ` in program order.
    fn names_and_versions(&self) -> (String, String) {
        let programs = self.all_programs();
        let join = |f: &dyn Fn(&RuntimeProgram) -> &str| {
            programs.iter().map(|p| f(p)).collect::<Vec<_>>().join(", ")
        };
        (join(&|p| &p.name), join(&|p| &p.version))
    }

    /// The interval it reports: its one program's in its implied task; otherwise the smallest
    /// interval of its cyclic tasks, and none when it has no cyclic task.
    fn interval_ms(&self) -> Option<u64> {
        if self.tasks.is_empty() {
            return self.all_programs().first().map(|p| p.interval_ms);
        }
        self.tasks
            .iter()
            .filter_map(|t| match t.kind {
                TaskKind::Cyclic { interval_ms, .. } => Some(interval_ms),
                _ => None,
            })
            .min()
    }

    /// How often it is exchanged with. As often as it scans, within bounds that keep an
    /// operator's view fresh and the exchange cheap: for a runtime that runs as today its
    /// program's interval; 20 ms when it has an event task, whose edges are seen only at an
    /// exchange; otherwise its smallest cyclic interval; 200 ms when it has none.
    fn exchange_ms(&self) -> u64 {
        let interval = if self.runs_as_today() {
            self.all_programs().first().map(|p| p.interval_ms)
        } else if self
            .tasks
            .iter()
            .any(|t| matches!(t.kind, TaskKind::Event { .. }))
        {
            Some(20)
        } else {
            self.interval_ms()
        };
        interval.unwrap_or(200).clamp(20, 200)
    }

    /// What can be checked of this runtime alone, without asking a runtime module. Each
    /// message starts `runtime '<id>'`.
    fn check(&self) -> Result<(), String> {
        let id = &self.id;
        if self.program.is_some() && !self.programs.is_empty() {
            return Err(format!(
                "runtime '{id}': give program or programs, not both"
            ));
        }
        let programs = self.all_programs();
        check_tasks(&programs, &self.tasks).map_err(|e| format!("runtime '{id}': {e}"))?;
        if self.tasks.is_empty() && programs.iter().any(|p| p.interval_ms == 0) {
            return Err(format!("runtime '{id}': interval_ms must be at least 1"));
        }
        for p in &programs {
            // A runtime of several programs names the program a binding is of.
            let at = if self.several() {
                format!("runtime '{id}': program '{}'", p.name)
            } else {
                format!("runtime '{id}'")
            };
            for o in &p.outputs {
                if o.tag.contains(':') {
                    return Err(format!(
                        "{at}: {} is another device's tag; a runtime writes this node's field tags",
                        o.tag
                    ));
                }
                if !(o.safe.is_number() || o.safe.is_boolean()) {
                    return Err(format!(
                        "{at}: the safe value of {} must be a number or a boolean",
                        o.tag
                    ));
                }
            }
            for b in &p.published {
                // `external.` is the field: only a driver's tags are named so (tag namespace
                // rules §2.1). What a program computes is `internal.`.
                if !b.tag.starts_with("internal.") {
                    return Err(format!(
                        "{at}: a published tag must start with 'internal.', which {} does not",
                        b.tag
                    ));
                }
            }
        }
        Ok(())
    }
}

/// The runtimes of a project (`runtimes.json`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimesConfig {
    #[serde(default)]
    pub instances: Vec<RuntimeInstance>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl RuntimesConfig {
    /// `Ok(None)` when the project has none.
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        let config: Self = serde_json::from_str(&text)
            .map_err(|e| format!("{} is not a runtimes configuration: {e}", path.display()))?;
        config.validate()?;
        Ok(Some(config))
    }

    /// What can be checked without asking a runtime module.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        let mut outputs: HashMap<&str, String> = HashMap::new();
        let mut published: HashMap<&str, String> = HashMap::new();
        for i in &self.instances {
            if !valid_id(&i.id) {
                return Err(format!(
                    "'{}' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'",
                    i.id
                ));
            }
            if !ids.insert(i.id.as_str()) {
                return Err(format!("runtime id '{}' is used more than once", i.id));
            }
            if i.module.trim().is_empty() {
                return Err(format!("runtime '{}' names no module", i.id));
            }
            i.check()?;
            // One tag has one writer, and one publisher: across runtimes, and across the
            // programs of one runtime.
            for p in i.all_programs() {
                let owner = i.owner_of(p);
                for o in &p.outputs {
                    if let Some(other) = outputs.insert(o.tag.as_str(), owner.clone()) {
                        return Err(format!(
                            "{} is an output of both '{}' and '{}': one tag has one writer",
                            o.tag, other, owner
                        ));
                    }
                }
                for b in &p.published {
                    if let Some(other) = published.insert(b.tag.as_str(), owner.clone()) {
                        return Err(format!(
                            "{} is published by both '{}' and '{}'",
                            b.tag, other, owner
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// The instances as connections of the field I/O runtime.
    pub fn connections(&self) -> Vec<ConnectionConfig> {
        self.instances
            .iter()
            .map(|i| ConnectionConfig {
                id: format!("{CONNECTION_PREFIX}{}", i.id),
                driver: format!("{CONNECTION_PREFIX}{}", i.module),
                poll_interval_ms: i.exchange_ms(),
                cycle_timeout_ms: Some(CYCLE_TIMEOUT_MS),
                config: serde_json::to_value(i).unwrap_or(Value::Null),
            })
            .collect()
    }
}

// -----------------------------------------------------------------------------
// The tags a runtime reads
// -----------------------------------------------------------------------------

/// Where a runtime's inputs come from.
#[async_trait]
pub trait TagSource: Send + Sync {
    /// Each tag as it is now. A tag that does not exist, or was never read, has no value and
    /// is not good.
    async fn read(&self, tags: &[String]) -> Vec<WireInput>;
}

/// The tag bus of this server.
pub struct BusTags<S: PlatformShm + 'static> {
    catalog: Arc<RwLock<TagCatalog>>,
    shm_name: String,
    capacity: usize,
    /// Opened on first use: the segment exists once field I/O has started.
    reader: Mutex<Option<ShmReaderHandle<S>>>,
}

impl<S: PlatformShm + 'static> BusTags<S> {
    pub fn new(catalog: Arc<RwLock<TagCatalog>>, shm_name: &str, capacity: usize) -> Self {
        Self {
            catalog,
            shm_name: shm_name.to_string(),
            capacity,
            reader: Mutex::new(None),
        }
    }
}

#[async_trait]
impl<S: PlatformShm + 'static> TagSource for BusTags<S> {
    async fn read(&self, tags: &[String]) -> Vec<WireInput> {
        let slots: Vec<Option<u32>> = {
            let catalog = self.catalog.read().await;
            tags.iter()
                .map(|t| catalog.get_by_uri(t).map(|m| m.slot_id))
                .collect()
        };
        let mut guard = self.reader.lock().await;
        if guard.is_none() {
            *guard = ShmReaderHandle::<S>::new(&self.shm_name, self.capacity).ok();
        }
        if let Some(reader) = guard.as_mut() {
            let _ = reader.update();
        }
        let reader = guard.as_ref();
        tags.iter()
            .zip(slots)
            .map(|(tag, slot)| {
                let read = slot
                    .zip(reader)
                    .and_then(|(slot, reader)| reader.read_tag(slot as usize).ok());
                match read {
                    Some(slot) => {
                        let (_, value) =
                            crate::api::telemetry::decode_slot_value(slot.type_code, &slot.value);
                        WireInput {
                            tag: tag.clone(),
                            value: (!value.is_null()).then_some(value),
                            good: slot.quality == QualityCode::GOOD,
                        }
                    }
                    None => WireInput {
                        tag: tag.clone(),
                        value: None,
                        good: false,
                    },
                }
            })
            .collect()
    }
}

// -----------------------------------------------------------------------------
// Status and commands
// -----------------------------------------------------------------------------

/// One output of a runtime, as last handled.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutputStatus {
    pub tag: String,
    pub variable: String,
    pub safe: Value,
    /// What the device last confirmed, written by this runtime: the program's value, or the
    /// safe value. `None`: nothing has been confirmed yet.
    pub written: Option<Value>,
    /// Why the last write was not confirmed, when it was not. It is tried again each cycle.
    pub error: Option<String>,
}

/// What a runtime instance is doing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuntimeStatus {
    pub id: String,
    pub module: String,
    /// Its program, or for a runtime of several programs their names joined by `, `.
    pub program: String,
    pub version: String,
    /// Its program's interval in its implied task; otherwise the smallest interval of its
    /// cyclic tasks. `None` when it has no cyclic task: it runs at no one interval.
    pub interval_ms: Option<u64>,
    /// `run`, `stop`, `fault`, or `unreachable` when its module does not answer.
    pub mode: String,
    /// Why it is in `fault`, or why it is `unreachable`.
    pub reason: Option<String>,
    /// Scans since the program was last put in RUN, as its module last said. `None`: nothing
    /// is known, because its module has not answered (yet, or any more). Not zero: zero is
    /// what a program in STOP that never ran has done, and that is a different statement.
    pub scans: Option<u64>,
    /// The last and the longest scan time. `None` while nothing is known, and until a scan has
    /// run: a module sends 0 before its first scan, and 0 µs was never measured.
    pub last_scan_us: Option<u64>,
    pub max_scan_us: Option<u64>,
    /// Scans that started late. `None` while nothing is known.
    pub overruns: Option<u64>,
    /// Whether every output is confirmed at its safe value. Meaningful while not in `run`.
    pub outputs_safe: bool,
    pub outputs: Vec<OutputStatus>,
    pub pid: Option<u32>,
    pub restarts: u64,
    /// The variables an engineer holds at a value, as its module last said. `None`: nothing
    /// is known, because its module has not answered (yet, or any more).
    pub forced: Option<Vec<ForcedValue>>,
    /// Every tag bound to a forced variable: an input the program no longer takes, an output
    /// written the forced value, a published tag giving it. What the HMI marks as forced.
    pub forced_tags: Vec<String>,
    /// Each program and the version that runs, and each task's state, as its module last said:
    /// from a runtime with tasks written out. Left out for a runtime of one program in its
    /// implied task, and while nothing is known.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<ProgramVersion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<TaskState>,
}

/// A force or a release an engineer asked of an instance.
#[derive(Debug, Clone, PartialEq)]
pub enum ForceAsk {
    Force {
        variable: String,
        value: Value,
    },
    /// One variable, or with `None` every one.
    Release {
        variable: Option<String>,
    },
}

/// Why a force or a release was not done.
#[derive(Debug, Clone, PartialEq)]
pub enum ForceRefusal {
    /// The module refused it: the program is not running, it has no such variable, ...
    Refused(String),
    /// The module did not answer.
    Unreachable(String),
    /// The instance was unloaded before its next exchange.
    Unloaded,
}

/// What a force or a release came to.
pub type ForceAnswer = Result<RuntimeState, ForceRefusal>;

/// How the variables of a program a load started began (ADR-013 §2.5, online change).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadStart {
    /// Changed online: it took over the variables of the running build it replaced and scans
    /// on from them, without a restart.
    Online,
    /// As at a warm restart: its RETAIN variables took the values kept for them, every other
    /// variable its initial value.
    Warm,
    /// Cold: every variable took its initial value, RETAIN ones included.
    Cold,
    /// Its module was given the running build's variables and does not say what it took.
    Unknown,
}

/// What a load did to one runtime: how it started, and what did not carry over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeLoad {
    pub runtime: String,
    pub start: LoadStart,
    /// Why it was not changed online, when it was not.
    pub why: Option<String>,
    /// Changed online: how many of its variables took the running build's value.
    pub carried: Option<u64>,
    /// Changed online: the running build's variables it did not take, and why (gone, or of
    /// another type or size).
    pub dropped: Vec<NotCarried>,
    /// Changed online: its variables that took no value of the running build, and why (new,
    /// or of another type or size): each its initial value, or a RETAIN one its kept value.
    pub initialized: Vec<NotCarried>,
}

impl RuntimeLoad {
    fn not_online(runtime: &str, start: LoadStart, why: String) -> Self {
        Self {
            runtime: runtime.to_string(),
            start,
            why: Some(why),
            carried: None,
            dropped: Vec::new(),
            initialized: Vec::new(),
        }
    }
}

/// What a new build asked of the running one: its variables, if it runs these programs in this
/// module.
struct PendingCarry {
    /// The new build's [`RuntimeInstance::program_key`].
    program: String,
    module: String,
    reply: oneshot::Sender<Result<ProgramVariables, String>>,
}

/// A force or a release waiting for the instance's next exchange.
struct PendingForce {
    ask: ForceAsk,
    reply: oneshot::Sender<ForceAnswer>,
}

/// The variables being watched of one instance, and what its module last said of them.
struct Watch {
    variables: Vec<String>,
    /// When they were last asked for.
    asked: Instant,
    /// The module's answer for exactly these variables, or why it gave none, and when.
    answer: Option<(Instant, Result<WatchReply, String>)>,
}

#[derive(Default)]
struct StatesInner {
    /// By instance id, with the generation of the driver that reports it.
    statuses: BTreeMap<String, (u64, RuntimeStatus)>,
    /// What an engineer asked of an instance since it was loaded: run or stop.
    wanted: HashMap<String, bool>,
    /// Instances an engineer asked to start cold, not yet told.
    cold: HashSet<String>,
    /// Why the project's runtimes are not as configured, when they are not.
    problem: Option<String>,
    /// What is being watched, by instance id.
    watches: HashMap<String, Watch>,
    /// Forces and releases waiting for the instance's next exchange, by instance id.
    forcing: HashMap<String, Vec<PendingForce>>,
    /// Asks of a new build for the variables of the running one, by instance id.
    carrying: HashMap<String, Vec<PendingCarry>>,
    /// Whether the load under way starts its programs cold.
    cold_load: bool,
    /// What the load under way did to each runtime it started.
    loads: Vec<RuntimeLoad>,
    /// Woken whenever a watch is answered.
    answered: Arc<Notify>,
}

/// Shared view of the runtimes of this server, and the way to command them.
#[derive(Clone, Default)]
pub struct RuntimeStates(Arc<StdRwLock<StatesInner>>);

impl RuntimeStates {
    pub fn snapshot(&self) -> Vec<RuntimeStatus> {
        let inner = self.0.read().unwrap_or_else(|e| e.into_inner());
        inner.statuses.values().map(|(_, s)| s.clone()).collect()
    }

    pub fn problem(&self) -> Option<String> {
        self.0
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .problem
            .clone()
    }

    pub(crate) fn set_problem(&self, problem: Option<String>) {
        self.0.write().unwrap_or_else(|e| e.into_inner()).problem = problem;
    }

    /// Ask a loaded instance to run or to stop. `false` when there is no such instance. It
    /// takes effect at its next exchange.
    pub fn command(&self, id: &str, run: bool) -> bool {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if !inner.statuses.contains_key(id) {
            return false;
        }
        inner.wanted.insert(id.to_string(), run);
        true
    }

    /// Ask a loaded instance to start again cold, from its initial values, RETAIN variables
    /// included, and run. `false` when there is no such instance. It takes effect at its next
    /// exchange.
    pub fn cold_start(&self, id: &str) -> bool {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if !inner.statuses.contains_key(id) {
            return false;
        }
        inner.wanted.insert(id.to_string(), true);
        inner.cold.insert(id.to_string());
        true
    }

    /// Whether a cold start was asked of the instance, and not yet taken.
    fn take_cold(&self, id: &str) -> bool {
        self.0
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .cold
            .remove(id)
    }

    fn put_back_cold(&self, id: &str) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if inner.statuses.contains_key(id) {
            inner.cold.insert(id.to_string());
        }
    }

    /// Ask a loaded instance for these variables at its coming exchanges, for [`WATCH_KEPT`]
    /// from now. `false` when there is no such instance. Asking for other variables than the
    /// last time drops what was answered for those.
    pub fn watch(&self, id: &str, variables: &[String]) -> bool {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if !inner.statuses.contains_key(id) {
            return false;
        }
        let now = Instant::now();
        match inner.watches.get_mut(id) {
            Some(watch) if watch.variables == variables => watch.asked = now,
            _ => {
                inner.watches.insert(
                    id.to_string(),
                    Watch {
                        variables: variables.to_vec(),
                        asked: now,
                        answer: None,
                    },
                );
            }
        }
        true
    }

    /// Ask a loaded instance to force or release a variable at its next exchange. `None` when
    /// there is no such instance; otherwise the answer comes on the receiver. Whoever stops
    /// waiting drops the receiver, and an ask nobody waits for any more is not carried out.
    pub fn force(&self, id: &str, ask: ForceAsk) -> Option<oneshot::Receiver<ForceAnswer>> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if !inner.statuses.contains_key(id) {
            return None;
        }
        let (reply, answer) = oneshot::channel();
        inner
            .forcing
            .entry(id.to_string())
            .or_default()
            .push(PendingForce { ask, reply });
        Some(answer)
    }

    /// The forces and releases asked of the instance, in the order they were asked, and still
    /// waited for.
    fn forcing_wanted(&self, id: &str) -> Vec<PendingForce> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        inner
            .forcing
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.reply.is_closed())
            .collect()
    }

    /// Ask the running instance `id` for its variables at its next exchange, for a new build of
    /// the programs `program` (a [`RuntimeInstance::program_key`]) in `module` to carry on from. `None` when no instance of that id is loaded.
    fn ask_variables(
        &self,
        id: &str,
        program: &str,
        module: &str,
    ) -> Option<oneshot::Receiver<Result<ProgramVariables, String>>> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if !inner.statuses.contains_key(id) {
            return None;
        }
        let (reply, answer) = oneshot::channel();
        inner
            .carrying
            .entry(id.to_string())
            .or_default()
            .push(PendingCarry {
                program: program.to_string(),
                module: module.to_string(),
                reply,
            });
        Some(answer)
    }

    /// The asks for the instance's variables still waited for.
    fn carrying_wanted(&self, id: &str) -> Vec<PendingCarry> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        inner
            .carrying
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.reply.is_closed())
            .collect()
    }

    /// A load begins: whether it starts its programs cold, and nothing recorded yet.
    pub(crate) fn begin_loads(&self, cold: bool) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        inner.cold_load = cold;
        inner.loads.clear();
    }

    /// A load ended: what it did to each runtime it started.
    pub(crate) fn end_loads(&self) -> Vec<RuntimeLoad> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        inner.cold_load = false;
        std::mem::take(&mut inner.loads)
    }

    fn cold_load(&self) -> bool {
        self.0.read().unwrap_or_else(|e| e.into_inner()).cold_load
    }

    fn record_load(&self, load: RuntimeLoad) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        inner.loads.retain(|l| l.runtime != load.runtime);
        inner.loads.push(load);
    }

    /// Whether a newer driver reports for the instance: a replacement has taken it over.
    fn replaced(&self, id: &str, generation: u64) -> bool {
        self.0
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .statuses
            .get(id)
            .is_some_and(|(g, _)| *g != generation)
    }

    /// What the instance's module last said of exactly these variables, and how long ago.
    /// `None`: nothing yet, or the instance is gone.
    pub fn watched(
        &self,
        id: &str,
        variables: &[String],
    ) -> Option<(Duration, Result<WatchReply, String>)> {
        let inner = self.0.read().unwrap_or_else(|e| e.into_inner());
        let watch = inner.watches.get(id)?;
        if watch.variables != variables {
            return None;
        }
        let (at, answer) = watch.answer.as_ref()?;
        Some((at.elapsed(), answer.clone()))
    }

    /// Woken whenever a watch is answered: wait on it, then read [`RuntimeStates::watched`].
    pub fn answered(&self) -> Arc<Notify> {
        Arc::clone(&self.0.read().unwrap_or_else(|e| e.into_inner()).answered)
    }

    /// The variables to ask the instance's module for now, if anybody still watches.
    fn watch_wanted(&self, id: &str) -> Option<Vec<String>> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        let watch = inner.watches.get(id)?;
        if watch.asked.elapsed() > WATCH_KEPT {
            inner.watches.remove(id);
            return None;
        }
        Some(watch.variables.clone())
    }

    /// The module's answer for `variables`. Dropped when something else is watched by now.
    fn watch_answer(&self, id: &str, variables: &[String], answer: Result<WatchReply, String>) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if let Some(watch) = inner.watches.get_mut(id) {
            if watch.variables == variables {
                watch.answer = Some((Instant::now(), answer));
            }
        }
        inner.answered.notify_waiters();
    }

    fn wanted(&self, id: &str) -> Option<bool> {
        let inner = self.0.read().unwrap_or_else(|e| e.into_inner());
        inner.wanted.get(id).copied()
    }

    fn report(&self, generation: u64, status: RuntimeStatus) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        inner
            .statuses
            .insert(status.id.clone(), (generation, status));
    }

    /// Forget an instance, unless a newer driver has since reported for it (its replacement).
    fn forget(&self, id: &str, generation: u64) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if inner
            .statuses
            .get(id)
            .is_some_and(|(g, _)| *g == generation)
        {
            inner.statuses.remove(id);
            inner.wanted.remove(id);
            inner.cold.remove(id);
            inner.watches.remove(id);
            for pending in inner.forcing.remove(id).unwrap_or_default() {
                let _ = pending.reply.send(Err(ForceRefusal::Unloaded));
            }
            for pending in inner.carrying.remove(id).unwrap_or_default() {
                let _ = pending
                    .reply
                    .send(Err("the running build was unloaded".to_string()));
            }
            // Whoever waits for its values is told now, not at their deadline.
            inner.answered.notify_waiters();
        }
    }
}

/// What a runtime driver needs of the server.
#[derive(Clone)]
pub struct RuntimeEnv {
    pub tags: Arc<dyn TagSource>,
    pub io: IoStatusHandle,
    pub states: RuntimeStates,
    /// Where RETAIN values are kept; `None` keeps none, and every start is cold.
    pub retain: Option<RetainStore>,
}

/// Where the RETAIN values of the active project's runtimes are kept, so that a restart of the
/// module process or of the server is a warm restart (IEC 61131-3 §6.5.6): in the controller's
/// own directory (`<system>/retain/<project>/<runtime>.json`), not in the project, which is the
/// engineer's configuration and is not rewritten every few seconds.
#[derive(Clone)]
pub struct RetainStore {
    storage: Arc<ControllerStorage>,
}

impl RetainStore {
    pub fn new(storage: Arc<ControllerStorage>) -> Self {
        Self { storage }
    }

    /// The file of the active project's runtime `id`; `None` when no project is active.
    pub fn file(&self, id: &str) -> Option<PathBuf> {
        let project = self.storage.get_active_project().ok().flatten()?;
        Some(
            self.storage
                .system_dir()
                .join("retain")
                .join(project)
                .join(format!("{id}.json")),
        )
    }

    /// What is kept in `path`: nothing when there is no such file. A file that cannot be read
    /// is said and taken as nothing, so the program starts cold rather than from a guess.
    pub fn read(path: &Path) -> Vec<RetainedValue> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                tracing::error!(
                    "the RETAIN values in {} cannot be read: {e}",
                    path.display()
                );
                return Vec::new();
            }
        };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::error!("{} holds no RETAIN values: {e}", path.display());
            Vec::new()
        })
    }

    fn write(&self, path: &Path, values: &[RetainedValue]) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(values).map_err(|e| e.to_string())?;
        self.storage
            .atomic_write_file(path, &bytes)
            .map_err(|e| e.to_string())
    }
}

// -----------------------------------------------------------------------------
// The driver
// -----------------------------------------------------------------------------

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A runtime instance, as a driver of the field I/O runtime.
pub struct RuntimeDriver {
    process: ProcessDriver,
    module: String,
    /// Its module's version, and the tasks it says it runs (`profile.tasks`): `None` for a
    /// module that runs one program in one cyclic task.
    module_version: String,
    support: Option<TaskSupport>,
    env: RuntimeEnv,
    generation: u64,
    instance: Option<RuntimeInstance>,
    /// The tags the program reads, once each.
    reads: Vec<String>,
    /// Our claim on the outputs, while we hold one.
    claim: Option<u64>,
    /// What each output was last confirmed written to.
    written: HashMap<String, Value>,
    /// Outputs confirmed at their safe value since the program last ran.
    safe: BTreeSet<String>,
    errors: HashMap<String, String>,
    last: Option<RuntimeState>,
    /// Where its RETAIN values are kept, and what was kept last.
    retain_file: Option<PathBuf>,
    kept: Option<Vec<RetainedValue>>,
    retain_due: Instant,
    /// Its module said it keeps no RETAIN values: it is not asked again.
    retain_unsupported: bool,
    /// The configuration its module was started with, the kept values in it.
    sent: Option<Value>,
}

impl RuntimeDriver {
    pub fn new(module: Arc<ModuleBinary>, env: RuntimeEnv) -> Self {
        Self {
            module: module.manifest.provides.clone(),
            module_version: module.manifest.version.clone(),
            support: TaskSupport::of(&module.manifest.profile),
            process: ProcessDriver::new(module),
            env,
            generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1,
            instance: None,
            reads: Vec::new(),
            claim: None,
            written: HashMap::new(),
            safe: BTreeSet::new(),
            errors: HashMap::new(),
            last: None,
            retain_file: None,
            kept: None,
            retain_due: Instant::now(),
            retain_unsupported: false,
            sent: None,
        }
    }

    /// Ask the module for the RETAIN values, every [`RETAIN_EVERY`] or `now`, and keep them when
    /// they changed. A module started again after a crash is given the values kept last.
    async fn keep_retained(&mut self, now: bool) {
        let Some(path) = self.retain_file.clone() else {
            return;
        };
        let Some(store) = self.env.retain.clone() else {
            return;
        };
        if self.retain_unsupported || (!now && Instant::now() < self.retain_due) {
            return;
        }
        self.retain_due = Instant::now() + RETAIN_EVERY;
        let values = match self.process.call(Call::Retained, EXCHANGE_DEADLINE).await {
            Ok(v) => match serde_json::from_value::<Vec<RetainedValue>>(v) {
                Ok(values) => values,
                Err(e) => {
                    tracing::warn!(
                        "the module answered for RETAIN values with something else: {e}"
                    );
                    return;
                }
            },
            // Not answering now is the exchange's to report; it is asked again later.
            Err(PluginError::CommunicationError(_) | PluginError::Timeout(_)) => return,
            Err(e) => {
                tracing::warn!("{}: no RETAIN values are kept: {e}", self.module);
                self.retain_unsupported = true;
                return;
            }
        };
        if self.kept.as_ref() == Some(&values) {
            return;
        }
        match store.write(&path, &values) {
            Ok(()) => {
                if let Some(Value::Object(map)) = self.sent.as_mut() {
                    map.insert(
                        "retained".to_string(),
                        serde_json::to_value(&values).unwrap_or(Value::Null),
                    );
                }
                if let Some(sent) = self.sent.clone() {
                    self.process.restart_with(sent);
                }
                self.kept = Some(values);
            }
            Err(e) => tracing::error!("the RETAIN values were not kept in {}: {e}", path.display()),
        }
    }

    /// The variables of the running build of `instance`, for it to carry on from (an online
    /// change), or why there are none: no build of it is loaded, the running one is another
    /// program or in another module, it is not in RUN, or it does not answer. Asked at the
    /// running one's next exchange.
    async fn running_variables(
        &self,
        instance: &RuntimeInstance,
    ) -> Result<(ProgramVariables, Instant), String> {
        let Some(answer) =
            self.env
                .states
                .ask_variables(&instance.id, &instance.program_key(), &self.module)
        else {
            return Err("no build of it was loaded".to_string());
        };
        match tokio::time::timeout(CARRY_WAIT, answer).await {
            Ok(Ok(Ok(variables))) => Ok((variables, Instant::now())),
            Ok(Ok(Err(why))) => Err(why),
            Ok(Err(_)) => Err("the running build was unloaded before it answered".to_string()),
            Err(_) => Err(format!(
                "the running build did not give its variables within {} ms",
                CARRY_WAIT.as_millis()
            )),
        }
    }

    /// What the module says it took of the running build's variables it was started with. A
    /// module that does not say is not claimed to have changed online.
    async fn what_it_carried(&mut self, id: &str) -> RuntimeLoad {
        let answer = self
            .process
            .call(Call::Carried, EXCHANGE_DEADLINE)
            .await
            .map_err(module_reason)
            .and_then(|v| {
                serde_json::from_value::<CarryReport>(v).map_err(|e| {
                    format!("its module answered for what it carried with something else: {e}")
                })
            });
        match answer {
            Ok(report) if report.given => RuntimeLoad {
                runtime: id.to_string(),
                start: LoadStart::Online,
                why: None,
                carried: Some(report.carried),
                dropped: report.dropped,
                initialized: report.initialized,
            },
            Ok(_) => RuntimeLoad::not_online(
                id,
                LoadStart::Unknown,
                "its module was given the running build's variables and says it was given none"
                    .to_string(),
            ),
            Err(why) => RuntimeLoad::not_online(
                id,
                LoadStart::Unknown,
                format!(
                    "its module does not say what it took of the running build's variables: {why}"
                ),
            ),
        }
    }

    /// Answer a new build's asks for this one's variables: only for a build of the same
    /// program in the same module, and only while this one is in RUN and nobody asked it to
    /// stop.
    async fn give_variables(&mut self, id: &str) {
        let asks = self.env.states.carrying_wanted(id);
        if asks.is_empty() {
            return;
        }
        let running = self
            .last
            .as_ref()
            .is_some_and(|s| s.mode == RuntimeMode::Run)
            && self.env.states.wanted(id) != Some(false);
        let program = self
            .instance
            .as_ref()
            .map(RuntimeInstance::program_key)
            .unwrap_or_default();
        for ask in asks {
            let answer = if ask.program != program {
                Err(format!(
                    "the program running was '{program}', not '{}'",
                    ask.program
                ))
            } else if ask.module != self.module {
                Err(format!(
                    "it ran in the runtime module '{}', not '{}'",
                    self.module, ask.module
                ))
            } else if !running {
                Err("the running build was not in RUN".to_string())
            } else {
                match self.process.call(Call::Variables, EXCHANGE_DEADLINE).await {
                    Ok(v) => serde_json::from_value::<ProgramVariables>(v).map_err(|e| {
                        format!(
                            "the running build answered for its variables with something else: {e}"
                        )
                    }),
                    Err(e) => Err(format!(
                        "the running build did not give its variables: {}",
                        module_reason(e)
                    )),
                }
            };
            let _ = ask.reply.send(answer);
        }
    }

    fn owner(id: &str) -> String {
        format!("runtime '{id}'")
    }

    /// Write every output its safe value, unless the device already confirmed it. An output
    /// a newer claim has taken over (our replacement is running it) is left alone.
    async fn make_safe(&mut self) {
        let Some(instance) = &self.instance else {
            return;
        };
        let owner = Self::owner(&instance.id);
        self.written.clear();
        for (_, o) in instance.outputs() {
            if self.safe.contains(&o.tag) {
                continue;
            }
            if self.claim.is_some_and(|c| !self.env.io.holds(&o.tag, c)) {
                continue;
            }
            match self
                .env
                .io
                .write_tag_as(&o.tag, o.safe.clone(), Some(&owner))
                .await
            {
                Ok(()) => {
                    self.safe.insert(o.tag.clone());
                    self.errors.remove(&o.tag);
                }
                Err(e) => {
                    tracing::error!(
                        "{owner}: the safe value of {} was not confirmed: {e}",
                        o.tag
                    );
                    self.errors.insert(o.tag.clone(), e.to_string());
                }
            }
        }
    }

    /// Write the outputs the program changed since they were last confirmed.
    async fn write_outputs(&mut self, reply: &ExchangeReply) {
        let Some(instance) = &self.instance else {
            return;
        };
        let owner = Self::owner(&instance.id);
        self.safe.clear();
        for o in &reply.outputs {
            if self.written.get(&o.tag) == Some(&o.value) {
                continue;
            }
            match self
                .env
                .io
                .write_tag_as(&o.tag, o.value.clone(), Some(&owner))
                .await
            {
                Ok(()) => {
                    self.written.insert(o.tag.clone(), o.value.clone());
                    self.errors.remove(&o.tag);
                }
                // Not confirmed: it is not in `written`, so it is tried again next cycle.
                Err(e) => {
                    if !matches!(e, WriteError::Refused(_)) || !self.errors.contains_key(&o.tag) {
                        tracing::warn!("{owner}: output {} was not confirmed: {e}", o.tag);
                    }
                    self.errors.insert(o.tag.clone(), e.to_string());
                }
            }
        }
    }

    fn report(&self, mode: &str, reason: Option<String>) {
        let Some(instance) = &self.instance else {
            return;
        };
        let state = self.last.as_ref();
        let (program, version) = instance.names_and_versions();
        let outputs: Vec<OutputStatus> = instance
            .outputs()
            .into_iter()
            .map(|(p, o)| OutputStatus {
                tag: o.tag.clone(),
                variable: instance.variable_name(p, &o.variable),
                safe: o.safe.clone(),
                written: if self.safe.contains(&o.tag) {
                    Some(o.safe.clone())
                } else {
                    self.written.get(&o.tag).cloned()
                },
                error: self.errors.get(&o.tag).cloned(),
            })
            .collect();
        self.env.states.report(
            self.generation,
            RuntimeStatus {
                id: instance.id.clone(),
                module: self.module.clone(),
                program,
                version,
                interval_ms: instance.interval_ms(),
                mode: mode.to_string(),
                reason,
                // What the module last said, and only that: a module that does not answer
                // reports no figures, never zeros that nothing measured.
                scans: state.map(|s| s.scans),
                last_scan_us: state.filter(|s| s.scans > 0).map(|s| s.last_scan_us),
                max_scan_us: state.filter(|s| s.scans > 0).map(|s| s.max_scan_us),
                overruns: state.map(|s| s.overruns),
                outputs_safe: instance
                    .outputs()
                    .iter()
                    .all(|(_, o)| self.safe.contains(&o.tag)),
                outputs,
                pid: self.process.pid(),
                restarts: self.process.restarts(),
                forced: state.map(|s| s.forced.clone()),
                forced_tags: state
                    .map(|s| instance_forced_tags(instance, &s.forced))
                    .unwrap_or_default(),
                programs: state.map(|s| s.programs.clone()).unwrap_or_default(),
                tasks: state.map(|s| s.tasks.clone()).unwrap_or_default(),
            },
        );
    }

    fn parse(config: &Value) -> Result<RuntimeInstance, PluginError> {
        let instance: RuntimeInstance = serde_json::from_value(config.clone()).map_err(|e| {
            PluginError::InvalidConfiguration(format!("not a runtime instance: {e}"))
        })?;
        instance
            .check()
            .map_err(PluginError::InvalidConfiguration)?;
        Ok(instance)
    }

    /// Whether its module runs `instance` as written: see [`support_problem`].
    fn check_support(&self, instance: &RuntimeInstance) -> Result<(), PluginError> {
        match support_problem(
            &self.module,
            &self.module_version,
            self.support.as_ref(),
            instance,
        ) {
            Some(problem) => Err(PluginError::InvalidConfiguration(problem)),
            None => Ok(()),
        }
    }
}

/// Why the runtime module `module` (version `version`), which says it runs the tasks
/// `support`, would not run `instance` as written (ADR-013 §2.19); `None` when it would. A
/// module that does not say it runs tasks is given only a runtime that runs as today, and a
/// module that does only the kinds of task it lists. Checked before the module is asked,
/// because a module that does not know tasks ignores them and would run something else than
/// was written.
fn support_problem(
    module: &str,
    version: &str,
    support: Option<&TaskSupport>,
    instance: &RuntimeInstance,
) -> Option<String> {
    let module = format!("the runtime module '{module}' (version {version})");
    let id = &instance.id;
    let Some(support) = support else {
        if instance.runs_as_today() {
            return None;
        }
        let count = instance.all_programs().len();
        let has = match instance.tasks.first() {
            Some(task) if count <= 1 => {
                format!("task '{}' of kind {}", task.name, task.kind.name())
            }
            _ => format!("{count} programs"),
        };
        return Some(format!(
            "runtime '{id}' has {has}; {module} runs one program in one cyclic task and does not say it runs tasks: install a newer build of it"
        ));
    };
    let task = instance
        .tasks
        .iter()
        .find(|t| !support.kinds.iter().any(|k| k == t.kind.name()))?;
    let runs = if support.kinds.is_empty() {
        "none".to_string()
    } else {
        support.kinds.join(", ")
    };
    Some(format!(
        "runtime '{id}': task '{}' is a {} task, which {module} does not run (it runs: {runs})",
        task.name,
        task.kind.name()
    ))
}

/// What a module said when it refused, without the error kind's prefix.
fn module_reason(e: PluginError) -> String {
    match e {
        PluginError::InitializationFailed(m)
        | PluginError::StartFailed(m)
        | PluginError::RuntimeError(m)
        | PluginError::InvalidConfiguration(m)
        | PluginError::CapabilityViolation(m)
        | PluginError::Timeout(m)
        | PluginError::InvalidTag(m)
        | PluginError::CommunicationError(m) => m,
    }
}

/// A variable's name as a program compares it: case and spaces do not count.
fn variable_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Every tag bound to one of the forced variables, once each.
fn forced_tags(program: &RuntimeProgram, forced: &[ForcedValue]) -> Vec<String> {
    let keys: HashSet<String> = forced.iter().map(|f| variable_key(&f.variable)).collect();
    let bound = program
        .inputs
        .iter()
        .map(|b| (&b.variable, &b.tag))
        .chain(program.outputs.iter().map(|b| (&b.variable, &b.tag)))
        .chain(program.published.iter().map(|b| (&b.variable, &b.tag)));
    let tags: BTreeSet<String> = bound
        .filter(|(variable, _)| keys.contains(&variable_key(variable)))
        .map(|(_, tag)| tag.clone())
        .collect();
    tags.into_iter().collect()
}

/// Every tag bound to one of the forced variables of a runtime. With one program a variable is
/// named by its path; with several by `<program>.<path>`, the program in any case, and a forced
/// `a.x` is bound where program `a` binds `x`.
fn instance_forced_tags(instance: &RuntimeInstance, forced: &[ForcedValue]) -> Vec<String> {
    let programs = instance.all_programs();
    if let [program] = programs.as_slice() {
        return forced_tags(program, forced);
    }
    let mut tags = BTreeSet::new();
    for program in programs {
        let of_program: Vec<ForcedValue> = forced
            .iter()
            .filter_map(|f| {
                let (name, path) = f.variable.split_once('.')?;
                name.trim()
                    .eq_ignore_ascii_case(&program.name)
                    .then(|| ForcedValue {
                        variable: path.to_string(),
                        value: f.value.clone(),
                    })
            })
            .collect();
        tags.extend(forced_tags(program, &of_program));
    }
    tags.into_iter().collect()
}

fn mode_name(mode: RuntimeMode) -> &'static str {
    match mode {
        RuntimeMode::Run => "run",
        RuntimeMode::Stop => "stop",
        RuntimeMode::Fault => "fault",
    }
}

#[async_trait]
impl MythosPlugin for RuntimeDriver {
    fn metadata(&self) -> PluginMetadata {
        self.process.metadata()
    }

    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        self.process.init(ctx).await
    }

    /// Load the program into a module process, then claim its outputs.
    async fn start(&mut self, ctx: &PluginContext, config: &Value) -> Result<(), PluginError> {
        let instance = Self::parse(config)?;
        self.check_support(&instance)?;
        let retain_file = self.env.retain.as_ref().and_then(|r| r.file(&instance.id));
        // How it starts (ADR-013 §2.5). Cold when the load asks for it. Changed online when it
        // replaces a running build of the same program: it takes over that build's variables.
        // Otherwise as at a warm restart: the values its RETAIN variables had when it last ran,
        // kept by this server.
        let cold = self.env.states.cold_load();
        let kept = match (&retain_file, cold) {
            (Some(path), false) => RetainStore::read(path),
            _ => Vec::new(),
        };
        let carried = if cold {
            Err((LoadStart::Cold, "a cold load was asked for".to_string()))
        } else if !instance.run {
            Err((
                LoadStart::Warm,
                "it is loaded in STOP, and starts as at a warm restart when it is put in RUN"
                    .to_string(),
            ))
        } else {
            self.running_variables(&instance)
                .await
                .map_err(|why| (LoadStart::Warm, why))
        };
        let mut sent = config.clone();
        if let Value::Object(map) = &mut sent {
            map.insert(
                "retained".to_string(),
                serde_json::to_value(&kept).unwrap_or(Value::Null),
            );
        }
        let mut first = sent.clone();
        if let (Ok((variables, read_at)), Value::Object(map)) = (&carried, &mut first) {
            // Its clock goes on from the running build's, and the time since it was read.
            let since = read_at.elapsed().as_nanos().min(i64::MAX as u128) as i64;
            let carried = ProgramVariables {
                clock_ns: variables.clock_ns.saturating_add(since),
                values: variables.values.clone(),
            };
            map.insert(
                "carried".to_string(),
                serde_json::to_value(&carried).unwrap_or(Value::Null),
            );
        }
        self.process.start(ctx, &first).await?;
        // A process started again after a crash is a warm restart: it is never given the
        // running build's values again, which would by then be old.
        self.process.restart_with(sent.clone());
        let load = match carried {
            Err((start, why)) => RuntimeLoad::not_online(&instance.id, start, why),
            Ok(_) => self.what_it_carried(&instance.id).await,
        };
        self.retain_file = retain_file;
        // After a cold load nothing is taken as kept, so its first exchange keeps its values in
        // place of those of the build it replaced.
        self.kept = (!cold).then_some(kept);
        self.sent = Some(sent);
        self.retain_due = Instant::now();
        let outputs: Vec<String> = instance
            .outputs()
            .iter()
            .map(|(_, o)| o.tag.clone())
            .collect();
        match self.env.io.claim(&Self::owner(&instance.id), &outputs) {
            Ok(claim) => self.claim = Some(claim),
            Err(why) => {
                let _ = self.process.stop(ctx).await;
                return Err(PluginError::StartFailed(why));
            }
        }
        let mut reads: Vec<String> = instance
            .all_programs()
            .iter()
            .flat_map(|p| p.inputs.iter().map(|i| i.tag.clone()))
            .collect();
        reads.sort();
        reads.dedup();
        self.reads = reads;
        self.instance = Some(instance);
        self.report("stop", Some("loaded; no exchange yet".to_string()));
        self.env.states.record_load(load);
        Ok(())
    }

    /// Unload. The outputs still ours go to their safe values first; then the claim is given
    /// up and the process stopped.
    async fn stop(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
        // What its RETAIN variables hold now is kept before the process goes, unless a new
        // build has replaced it: the kept values are then that build's to write.
        let replaced = self
            .instance
            .as_ref()
            .is_some_and(|i| self.env.states.replaced(&i.id, self.generation));
        if !replaced {
            self.keep_retained(true).await;
        }
        if self.instance.is_some() {
            self.safe.clear();
            self.make_safe().await;
        }
        if let Some(claim) = self.claim.take() {
            self.env.io.release(claim);
        }
        if let Some(instance) = &self.instance {
            self.env.states.forget(&instance.id, self.generation);
        }
        self.process.stop(ctx).await
    }

    async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
        self.process.health_check().await
    }
}

#[async_trait]
impl ProtocolDriver for RuntimeDriver {
    async fn describe_config(&self, config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        let instance = Self::parse(config)?;
        self.check_support(&instance)?;
        self.process.describe_config(config).await
    }

    fn declared_tags(&self) -> Vec<TagDeclaration> {
        self.process.declared_tags()
    }

    /// One exchange: the tags in; published samples back to the I/O runtime; outputs written,
    /// or made safe.
    async fn poll_cycle(&mut self, _ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        let Some(id) = self.instance.as_ref().map(|i| i.id.clone()) else {
            return Err(PluginError::RuntimeError(
                "no program is loaded".to_string(),
            ));
        };
        let inputs = self.env.tags.read(&self.reads).await;
        let answer = match tokio::time::timeout(
            EXCHANGE_DEADLINE,
            self.process
                .call(Call::Exchange { inputs }, EXCHANGE_DEADLINE),
        )
        .await
        {
            Ok(Ok(value)) => serde_json::from_value::<ExchangeReply>(value).map_err(|e| {
                PluginError::CommunicationError(format!(
                    "the module answered an exchange with something else: {e}"
                ))
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(PluginError::Timeout(format!(
                "the module did not answer an exchange within {} ms",
                EXCHANGE_DEADLINE.as_millis()
            ))),
        };
        let reply = match answer {
            Ok(reply) => reply,
            Err(e) => {
                // Not answering is not running: whatever the program last asked for no longer
                // stands.
                self.last = None;
                self.make_safe().await;
                self.report("unreachable", Some(e.to_string()));
                for pending in self.env.states.forcing_wanted(&id) {
                    let _ = pending.reply.send(Err(ForceRefusal::Unreachable(format!(
                        "the runtime's module does not answer: {e}"
                    ))));
                }
                for pending in self.env.states.carrying_wanted(&id) {
                    let _ = pending.reply.send(Err(format!(
                        "the running build's module does not answer: {e}"
                    )));
                }
                if let Some(variables) = self.env.states.watch_wanted(&id) {
                    self.env.states.watch_answer(
                        &id,
                        &variables,
                        Err(format!("the runtime's module does not answer: {e}")),
                    );
                }
                return Err(e);
            }
        };

        // A program an engineer stopped does not get to write, even for the one exchange after
        // its module was restarted with a configuration that says run.
        let wanted = self.env.states.wanted(&id);
        if reply.state.mode == RuntimeMode::Run && wanted != Some(false) {
            self.write_outputs(&reply).await;
        } else {
            self.make_safe().await;
        }
        self.last = Some(reply.state.clone());
        self.report(mode_name(reply.state.mode), reply.state.fault.clone());

        // What an engineer asked for, if it is not what the program is doing. It takes effect
        // by the next exchange.
        let mut changed = false;
        if let Some(run) = wanted {
            let cold = run && self.env.states.take_cold(&id);
            if cold || run != (reply.state.mode == RuntimeMode::Run) {
                changed = true;
                match self
                    .process
                    .call(Call::Mode { run, cold }, EXCHANGE_DEADLINE)
                    .await
                    .and_then(|v| {
                        serde_json::from_value::<RuntimeState>(v)
                            .map_err(|e| PluginError::CommunicationError(e.to_string()))
                    }) {
                    // What it says it now does is what is reported, at once.
                    Ok(state) => {
                        self.report(mode_name(state.mode), state.fault.clone());
                        self.last = Some(state);
                    }
                    Err(e) => {
                        tracing::warn!("{}: mode change not taken: {e}", Self::owner(&id));
                        if cold {
                            self.env.states.put_back_cold(&id);
                        }
                    }
                }
            }
        }

        // Its RETAIN values, kept when they changed: at once after a change of mode, so that a
        // stop keeps what it stopped with.
        self.keep_retained(changed).await;

        // Its variables, for a new build of it about to replace it: after a mode change, so a
        // program an engineer has just stopped gives none.
        self.give_variables(&id).await;

        // What an engineer asked to force or release, in the order asked: after a mode change,
        // so a force asked after a stop is refused by a program that no longer runs.
        for pending in self.env.states.forcing_wanted(&id) {
            let call = match pending.ask {
                ForceAsk::Force { variable, value } => Call::Force { variable, value },
                ForceAsk::Release { variable } => Call::Unforce { variable },
            };
            let answer = match self.process.call(call, EXCHANGE_DEADLINE).await {
                Ok(v) => serde_json::from_value::<RuntimeState>(v).map_err(|e| {
                    ForceRefusal::Unreachable(format!(
                        "the module answered a force with something else: {e}"
                    ))
                }),
                Err(PluginError::CommunicationError(e) | PluginError::Timeout(e)) => {
                    Err(ForceRefusal::Unreachable(e))
                }
                Err(e) => Err(ForceRefusal::Refused(module_reason(e))),
            };
            if let Ok(state) = &answer {
                // What it now holds is reported at once, not at the next exchange.
                self.last = Some(state.clone());
                self.report(mode_name(state.mode), state.fault.clone());
            }
            let _ = pending.reply.send(answer);
        }

        // The variables somebody watches, as this scan left them. Reading changes nothing in
        // the program, and a module that cannot be watched says so.
        if let Some(variables) = self.env.states.watch_wanted(&id) {
            let answer = self
                .process
                .call(
                    Call::Watch {
                        variables: variables.clone(),
                    },
                    EXCHANGE_DEADLINE,
                )
                .await
                .and_then(|v| {
                    serde_json::from_value::<WatchReply>(v).map_err(|e| {
                        PluginError::CommunicationError(format!(
                            "the module answered a watch with something else: {e}"
                        ))
                    })
                })
                .map_err(|e| e.to_string());
            self.env.states.watch_answer(&id, &variables, answer);
        }

        Ok(reply.samples.into_iter().map(Into::into).collect())
    }

    /// An operator writes a setpoint: a published variable.
    async fn write_tag(
        &mut self,
        ctx: &PluginContext,
        tag_uri: &str,
        value: &Value,
    ) -> Result<(), PluginError> {
        self.process.write_tag(ctx, tag_uri, value).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn instance(id: &str) -> Value {
        json!({
            "id": id, "module": "vm", "run": true,
            "program": {
                "name": "P", "version": "1", "interval_ms": 20, "code": "00",
                "inputs": [{"variable": "a", "tag": "external.A.B.c"}],
                "outputs": [{"variable": "b", "tag": "external.A.B.d", "safe": 0}],
                "published": [{"variable": "c", "tag": format!("internal.A.{id}.c")}]
            }
        })
    }

    fn check(instances: Vec<Value>) -> Result<(), String> {
        let config: RuntimesConfig =
            serde_json::from_value(json!({ "instances": instances })).map_err(|e| e.to_string())?;
        config.validate()
    }

    #[test]
    fn a_configuration_is_refused_for_each_thing_it_can_get_wrong() {
        assert_eq!(check(vec![instance("tank")]), Ok(()));

        let with = |change: &dyn Fn(&mut Value)| {
            let mut i = instance("tank");
            change(&mut i);
            check(vec![i]).unwrap_err()
        };
        assert!(with(&|i| i["id"] = json!("tank 1")).contains("is not a runtime id"));
        assert!(with(&|i| i["id"] = json!("")).contains("is not a runtime id"));
        assert!(with(&|i| i["module"] = json!(" ")).contains("names no module"));
        assert!(with(&|i| i["program"]["interval_ms"] = json!(0)).contains("at least 1"));
        assert!(
            with(&|i| i["program"]["outputs"][0]["tag"] = json!("edge1:external.A.B.d"))
                .contains("another device's tag")
        );
        assert!(with(&|i| i["program"]["outputs"][0]["safe"] = json!("off"))
            .contains("must be a number or a boolean"));
        assert!(
            with(&|i| i["program"]["published"][0]["tag"] = json!("external.A.B.c"))
                .contains("must start with 'internal.'")
        );
        // No default: an output without a safe value, or an instance that does not say
        // whether it runs, is not a configuration.
        assert!(with(&|i| {
            i["program"]["outputs"][0]
                .as_object_mut()
                .unwrap()
                .remove("safe");
        })
        .contains("missing field `safe`"));
        assert!(with(&|i| {
            i.as_object_mut().unwrap().remove("run");
        })
        .contains("missing field `run`"));
    }

    #[test]
    fn two_instances_cannot_share_an_id_an_output_or_a_published_tag() {
        assert!(check(vec![instance("a"), instance("a")])
            .unwrap_err()
            .contains("used more than once"));
        // Different ids, the same output tag: one tag has one writer.
        assert!(check(vec![instance("a"), instance("b")])
            .unwrap_err()
            .contains("is an output of both 'a' and 'b'"));
        let mut b = instance("b");
        b["program"]["outputs"][0]["tag"] = json!("external.A.B.e");
        b["program"]["published"][0]["tag"] = json!("internal.A.a.c");
        assert!(check(vec![instance("a"), b])
            .unwrap_err()
            .contains("is published by both 'a' and 'b'"));
    }

    fn loaded(states: &RuntimeStates, id: &str, generation: u64) {
        states.report(
            generation,
            RuntimeStatus {
                id: id.to_string(),
                module: "vm".into(),
                program: "P".into(),
                version: "1".into(),
                interval_ms: Some(20),
                mode: "run".into(),
                reason: None,
                scans: Some(1),
                last_scan_us: Some(1),
                max_scan_us: Some(1),
                overruns: Some(0),
                outputs_safe: false,
                outputs: Vec::new(),
                pid: None,
                restarts: 0,
                forced: Some(Vec::new()),
                forced_tags: Vec::new(),
                programs: Vec::new(),
                tasks: Vec::new(),
            },
        );
    }

    #[test]
    fn the_tags_of_a_forced_variable_are_every_tag_bound_to_it_whatever_its_case() {
        let instance: RuntimeInstance = serde_json::from_value(instance("t")).unwrap();
        let mut program = instance.program.unwrap();
        program.inputs.push(mythos_module::protocol::InputBinding {
            variable: "B".into(),
            tag: "external.A.B.e".into(),
            of: Default::default(),
        });
        let forced = |names: &[&str]| -> Vec<ForcedValue> {
            names
                .iter()
                .map(|n| ForcedValue {
                    variable: n.to_string(),
                    value: json!(1),
                })
                .collect()
        };
        assert_eq!(forced_tags(&program, &forced(&[])), Vec::<String>::new());
        assert_eq!(
            forced_tags(&program, &forced(&[" b "])),
            vec!["external.A.B.d".to_string(), "external.A.B.e".to_string()]
        );
        assert_eq!(
            forced_tags(&program, &forced(&["A", "c", "zz"])),
            vec!["external.A.B.c".to_string(), "internal.A.t.c".to_string()]
        );
    }

    #[tokio::test]
    async fn a_force_waits_for_its_instance_and_one_nobody_waits_for_is_dropped() {
        let states = RuntimeStates::default();
        let ask = || ForceAsk::Force {
            variable: "a".into(),
            value: json!(1),
        };
        assert!(states.force("t", ask()).is_none(), "no such instance");
        loaded(&states, "t", 1);
        let waited = states.force("t", ask()).unwrap();
        let abandoned = states.force("t", ask()).unwrap();
        drop(abandoned);
        assert_eq!(
            states.forcing_wanted("t").len(),
            1,
            "an ask whose caller stopped waiting is not carried out"
        );
        assert!(states.forcing_wanted("t").is_empty(), "taken once");
        drop(waited);

        // Unloading answers whoever waits, at once.
        let waiting = states.force("t", ask()).unwrap();
        states.forget("t", 1);
        assert_eq!(waiting.await.unwrap(), Err(ForceRefusal::Unloaded));
    }

    fn reply(variable: &str) -> WatchReply {
        serde_json::from_value(json!({
            "state": {"mode": "run", "program": "P", "version": "1", "scans": 1,
                      "last_scan_us": 1, "max_scan_us": 1, "overruns": 0},
            "values": [{"variable": variable, "type": "INT", "value": 1}]
        }))
        .unwrap()
    }

    #[test]
    fn a_watch_is_of_a_loaded_instance_and_its_answer_is_of_the_variables_asked() {
        let states = RuntimeStates::default();
        let (a, b) = (vec!["a".to_string()], vec!["b".to_string()]);
        assert!(!states.watch("tank", &a), "nothing is loaded");
        assert_eq!(states.watch_wanted("tank"), None);

        loaded(&states, "tank", 1);
        assert!(states.watch("tank", &a));
        assert_eq!(states.watch_wanted("tank"), Some(a.clone()));
        assert!(states.watched("tank", &a).is_none(), "asked, not answered");

        states.watch_answer("tank", &a, Ok(reply("a")));
        let (_, answer) = states.watched("tank", &a).expect("answered");
        assert_eq!(answer.unwrap().values[0].variable, "a");
        assert!(states.watched("tank", &b).is_none(), "not an answer for b");

        // Other variables are asked for: what was answered for the first is dropped, and an
        // answer for the first that arrives now is not taken for one of the second.
        assert!(states.watch("tank", &b));
        assert!(states.watched("tank", &b).is_none());
        states.watch_answer("tank", &a, Ok(reply("a")));
        assert!(states.watched("tank", &b).is_none());
        assert!(states.watched("tank", &a).is_none());
        states.watch_answer("tank", &b, Err("the module does not answer".into()));
        let (_, answer) = states.watched("tank", &b).expect("answered");
        assert_eq!(answer.unwrap_err(), "the module does not answer");
    }

    #[test]
    fn a_watch_nobody_renews_stops_and_one_of_an_unloaded_instance_is_forgotten() {
        let states = RuntimeStates::default();
        let a = vec!["a".to_string()];
        loaded(&states, "tank", 1);
        assert!(states.watch("tank", &a));
        // As if it was last asked for longer ago than a watch is kept.
        let long_ago = Instant::now()
            .checked_sub(WATCH_KEPT + Duration::from_secs(1))
            .expect("the clock has run that long");
        states
            .0
            .write()
            .unwrap()
            .watches
            .get_mut("tank")
            .unwrap()
            .asked = long_ago;
        assert_eq!(states.watch_wanted("tank"), None, "nobody watches any more");
        assert!(states.watched("tank", &a).is_none(), "and it is gone");

        // Renewed, it is asked for again; unloaded, it is forgotten with the instance.
        assert!(states.watch("tank", &a));
        assert_eq!(states.watch_wanted("tank"), Some(a.clone()));
        states.forget("tank", 1);
        assert_eq!(states.watch_wanted("tank"), None);
        assert!(!states.watch("tank", &a));
    }

    #[test]
    fn an_instance_becomes_a_connection_named_for_what_it_is() {
        let config: RuntimesConfig =
            serde_json::from_value(json!({ "instances": [instance("tank")] })).unwrap();
        let c = &config.connections()[0];
        assert_eq!(
            (c.id.as_str(), c.driver.as_str()),
            ("runtime:tank", "runtime:vm")
        );
        assert!(is_runtime_connection(&c.id) && !is_runtime_connection("plc1"));
        assert_eq!(c.config["program"]["name"], "P");
    }

    #[test]
    fn an_output_has_one_owner_and_a_replacement_takes_it_over_without_a_gap() {
        let io = IoStatusHandle::default();
        let tags = vec!["external.A.B.d".to_string(), "external.A.B.e".to_string()];
        let first = io.claim("runtime 'a'", &tags).unwrap();
        assert_eq!(
            io.claim("runtime 'b'", &tags[..1]),
            Err("external.A.B.d is already an output of runtime 'a'".to_string())
        );
        assert_eq!(io.owner_of(&tags[0]).as_deref(), Some("runtime 'a'"));

        // The replacement of 'a' claims one of the two before the old one stops.
        let second = io.claim("runtime 'a'", &tags[..1]).unwrap();
        assert!(!io.holds(&tags[0], first) && io.holds(&tags[0], second));
        assert!(
            io.holds(&tags[1], first),
            "what the replacement did not take is still the old one's"
        );
        // The old one stopping gives up only what is still its own.
        io.release(first);
        assert_eq!(io.owner_of(&tags[0]).as_deref(), Some("runtime 'a'"));
        assert_eq!(io.owner_of(&tags[1]), None);
        io.release(second);
        assert_eq!(io.owner_of(&tags[0]), None);
    }

    /// A runtime `t` of the programs `names`, each with one output and one published tag of
    /// its own, run by `tasks`.
    fn several(names: &[&str], tasks: Value) -> Value {
        let programs: Vec<Value> = names
            .iter()
            .map(|n| {
                json!({
                    "name": n, "version": "1", "interval_ms": 20, "code": "00",
                    "outputs": [{"variable": "o", "tag": format!("external.A.{n}.o"), "safe": 0}],
                    "published": [{"variable": "p", "tag": format!("internal.A.{n}.p")}]
                })
            })
            .collect();
        json!({"id": "t", "module": "vm", "run": true, "programs": programs, "tasks": tasks})
    }

    fn cyclic(name: &str, ms: u64, programs: &[&str]) -> Value {
        json!({"name": name, "kind": {"type": "cyclic", "interval_ms": ms},
               "priority": 5, "programs": programs})
    }

    fn parsed(i: Value) -> RuntimeInstance {
        serde_json::from_value(i).unwrap()
    }

    #[test]
    fn validate_refuses_each_task_mistake_with_its_message() {
        let two = || {
            several(
                &["a", "b"],
                json!([cyclic("f", 10, &["a"]), cyclic("s", 50, &["b"])]),
            )
        };
        assert_eq!(check(vec![two()]), Ok(()));
        let with = |change: &dyn Fn(&mut Value)| {
            let mut i = two();
            change(&mut i);
            check(vec![i]).unwrap_err()
        };
        let both = with(&|i| i["program"] = i["programs"][0].clone());
        assert_eq!(both, "runtime 't': give program or programs, not both");
        assert_eq!(
            with(&|i| i["programs"] = json!([])),
            "runtime 't': it runs no program"
        );
        assert_eq!(
            with(&|i| i["tasks"] = json!([])),
            "runtime 't': it runs several programs and names no task: each program needs a task"
        );
        assert_eq!(
            with(&|i| i["programs"][1]["name"] = json!("A")),
            "runtime 't': two programs are named 'A'"
        );
        assert_eq!(
            with(&|i| i["programs"][1]["name"] = json!("b.c")),
            "runtime 't': 'b.c' is not a program name: no '.', '[', ']' or spaces"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["name"] = json!("f 1")),
            "runtime 't': 'f 1' is not a task name: use 1 to 64 letters, digits, '-' or '_'"
        );
        assert_eq!(
            with(&|i| i["tasks"][1]["name"] = json!("f")),
            "runtime 't': task name 'f' is used more than once"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["priority"] = json!(32)),
            "runtime 't': task 'f' has priority 32; priorities are 0 (highest) to 31"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["kind"]["interval_ms"] = json!(0)),
            "runtime 't': task 'f' has interval_ms 0; it must be at least 1"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["kind"]["phase_ms"] = json!(u64::MAX / 1_000)),
            "runtime 't': task 'f': phase_ms is too long"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["watchdog_ms"] = json!(0)),
            "runtime 't': task 'f': watchdog_ms must be at least 1"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["kind"] = json!({"type": "event", "tag": " "})),
            "runtime 't': event task 'f' names no tag"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["programs"] = json!([])),
            "runtime 't': task 'f' runs no program"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["programs"] = json!(["a", "z"])),
            "runtime 't': task 'f' runs 'z', which is not a program of this runtime (it has a, b)"
        );
        assert_eq!(
            with(&|i| i["tasks"][0]["programs"] = json!(["a", "A"])),
            "runtime 't': task 'f' names 'A' twice"
        );
        assert_eq!(
            with(&|i| i["tasks"][1]["programs"] = json!(["b", "a"])),
            "runtime 't': program 'a' is in tasks 'f' and 's'; a program is in one task"
        );
        assert_eq!(
            with(&|i| i["tasks"] = json!([cyclic("f", 10, &["a"])])),
            "runtime 't': program 'b' is in no task, so it would never run"
        );
        // A task, and a kind, serde does not know is no configuration.
        assert!(with(&|i| i["tasks"][0]["phase"] = json!(1)).contains("unknown field `phase`"));
        assert!(with(&|i| i["tasks"][0]["kind"]["type"] = json!("hourly")).contains("hourly"));
        assert!(with(&|i| {
            i["tasks"][0].as_object_mut().unwrap().remove("priority");
        })
        .contains("missing field `priority`"));

        // A binding of one program of several names the program.
        assert_eq!(
            with(&|i| i["programs"][1]["outputs"][0]["safe"] = json!("off")),
            "runtime 't': program 'b': the safe value of external.A.b.o must be a number or a boolean"
        );
        // One tag has one writer, and one publisher, inside one runtime too.
        assert_eq!(
            with(&|i| i["programs"][1]["outputs"][0]["tag"] = json!("external.A.a.o")),
            "external.A.a.o is an output of both 't/a' and 't/b': one tag has one writer"
        );
        assert_eq!(
            with(&|i| i["programs"][1]["published"][0]["tag"] = json!("internal.A.a.p")),
            "internal.A.a.p is published by both 't/a' and 't/b'"
        );
        // With tasks written out a program's interval is not used, so 0 is no mistake.
        let mut ignored = two();
        ignored["programs"][0]["interval_ms"] = json!(0);
        assert_eq!(check(vec![ignored]), Ok(()));

        // One program keeps today's words, and any name, with tasks written out or not.
        let mut one = instance("tank");
        one["program"]["name"] = json!("P.1");
        assert_eq!(check(vec![one.clone()]), Ok(()));
        one["tasks"] = json!([cyclic("main", 10, &["p.1"])]);
        assert_eq!(check(vec![one.clone()]), Ok(()));
        one["program"]["outputs"][0]["safe"] = json!("off");
        assert_eq!(
            check(vec![one]).unwrap_err(),
            "runtime 'tank': the safe value of external.A.B.d must be a number or a boolean"
        );
        let mut two_runtimes = instance("a");
        two_runtimes["program"]["outputs"][0]["tag"] = json!("external.A.a.o");
        assert_eq!(
            check(vec![two_runtimes, two()]).unwrap_err(),
            "external.A.a.o is an output of both 'a' and 't/a': one tag has one writer"
        );
    }

    #[test]
    fn a_runtime_runs_as_today_only_with_one_program_in_one_cyclic_task_at_its_interval() {
        let today = parsed(instance("t"));
        assert!(today.runs_as_today());
        assert_eq!(
            today.effective_tasks(),
            vec![implied_task(today.program.as_ref().unwrap())]
        );
        let with_task = |task: Value| {
            let mut i = instance("t");
            i["tasks"] = json!([task]);
            parsed(i).runs_as_today()
        };
        // Written out as the implied task, at any priority and in any case.
        let mut same = cyclic("main", 20, &["p"]);
        same["priority"] = json!(0);
        assert!(with_task(same.clone()));
        assert!(!with_task(cyclic("main", 50, &["P"])), "another interval");
        let mut phased = same.clone();
        phased["kind"]["phase_ms"] = json!(5);
        assert!(!with_task(phased), "a phase");
        let mut guarded = same.clone();
        guarded["watchdog_ms"] = json!(100);
        assert!(!with_task(guarded), "a watchdog");
        let mut startup = same.clone();
        startup["kind"] = json!({"type": "startup"});
        assert!(!with_task(startup), "another kind");
        let mut two_tasks = instance("t");
        two_tasks["tasks"] = json!([same.clone(), same]);
        assert!(!parsed(two_tasks).runs_as_today());
        let two = parsed(several(&["b", "a"], json!([cyclic("f", 20, &["a", "b"])])));
        assert!(!two.runs_as_today(), "two programs");
        assert_eq!(two.effective_tasks(), two.tasks);
    }

    #[test]
    fn the_program_key_is_the_sorted_names_and_one_program_s_is_its_name() {
        assert_eq!(parsed(instance("t")).program_key(), "P");
        let two = parsed(several(&["b", "a"], json!([cyclic("f", 20, &["a", "b"])])));
        assert_eq!(two.program_key(), "a,b");
    }

    #[test]
    fn the_exchange_period_and_the_interval_follow_the_tasks() {
        let today = parsed(instance("t"));
        assert_eq!((today.exchange_ms(), today.interval_ms()), (20, Some(20)));
        let mut slow = instance("t");
        slow["program"]["interval_ms"] = json!(1_000);
        assert_eq!(parsed(slow).exchange_ms(), 200, "as today: clamped");
        let tasks = |tasks: Value| parsed(several(&["a", "b"], tasks));
        let cyc = tasks(json!([cyclic("f", 30, &["a"]), cyclic("s", 100, &["b"])]));
        assert_eq!((cyc.exchange_ms(), cyc.interval_ms()), (30, Some(30)));
        let event = tasks(json!([
            cyclic("s", 100, &["b"]),
            {"name": "e", "kind": {"type": "event", "tag": "external.A.B.c"}, "priority": 1, "programs": ["a"]}
        ]));
        assert_eq!((event.exchange_ms(), event.interval_ms()), (20, Some(100)));
        let none = tasks(json!([
            {"name": "c", "kind": {"type": "cycle"}, "priority": 31, "programs": ["a"]},
            {"name": "s", "kind": {"type": "startup"}, "priority": 0, "programs": ["b"]}
        ]));
        assert_eq!(
            (none.exchange_ms(), none.interval_ms()),
            (200, None),
            "no cyclic task: no interval is reported"
        );
        // One program in a written-out task of another interval: the task's.
        let mut one = instance("t");
        one["tasks"] = json!([cyclic("main", 50, &["P"])]);
        let one = parsed(one);
        assert_eq!((one.exchange_ms(), one.interval_ms()), (50, Some(50)));
    }

    #[test]
    fn an_older_module_is_given_only_what_runs_as_today_and_a_newer_one_only_its_kinds() {
        let problem = |support: Option<&TaskSupport>, i: Value| {
            support_problem("vm", "0.4.0", support, &parsed(i))
        };
        assert_eq!(problem(None, instance("t")), None);
        let mut as_today = instance("t");
        as_today["tasks"] = json!([cyclic("main", 20, &["P"])]);
        assert_eq!(problem(None, as_today), None);
        let two = several(
            &["a", "b"],
            json!([cyclic("f", 20, &["a"]), cyclic("s", 50, &["b"])]),
        );
        assert_eq!(
            problem(None, two.clone()).as_deref(),
            Some("runtime 't' has 2 programs; the runtime module 'vm' (version 0.4.0) runs one program in one cyclic task and does not say it runs tasks: install a newer build of it")
        );
        let mut startup = instance("t");
        startup["tasks"] = json!([{"name": "boot", "kind": {"type": "startup"}, "priority": 0, "programs": ["P"]}]);
        assert_eq!(
            problem(None, startup.clone()).as_deref(),
            Some("runtime 't' has task 'boot' of kind startup; the runtime module 'vm' (version 0.4.0) runs one program in one cyclic task and does not say it runs tasks: install a newer build of it")
        );
        let cyclic_only = TaskSupport {
            kinds: vec!["cyclic".into(), "cycle".into()],
        };
        assert_eq!(problem(Some(&cyclic_only), two), None);
        assert_eq!(
            problem(Some(&cyclic_only), startup.clone()).as_deref(),
            Some("runtime 't': task 'boot' is a startup task, which the runtime module 'vm' (version 0.4.0) does not run (it runs: cyclic, cycle)")
        );
        let nothing = TaskSupport { kinds: vec![] };
        assert!(problem(Some(&nothing), startup)
            .unwrap()
            .ends_with("(it runs: none)"));
        assert_eq!(
            problem(Some(&nothing), instance("t")),
            None,
            "the implied task needs no kind"
        );
    }

    #[test]
    fn a_forced_variable_of_a_runtime_of_several_programs_is_bound_in_its_own_program() {
        let mut i = several(&["a", "b"], json!([cyclic("f", 20, &["a", "b"])]));
        // Both programs have a variable `o`; only program a's is forced.
        i["programs"][0]["inputs"] = json!([{"variable": "O", "tag": "external.A.in"}]);
        let i = parsed(i);
        let forced = |names: &[&str]| -> Vec<ForcedValue> {
            names
                .iter()
                .map(|n| ForcedValue {
                    variable: n.to_string(),
                    value: json!(1),
                })
                .collect()
        };
        assert_eq!(
            instance_forced_tags(&i, &forced(&["A.o"])),
            vec!["external.A.a.o".to_string(), "external.A.in".to_string()]
        );
        assert_eq!(
            instance_forced_tags(&i, &forced(&["b.p", "o", "c.o"])),
            vec!["internal.A.b.p".to_string()],
            "an unprefixed name, or another program's, binds nothing"
        );
    }
}
