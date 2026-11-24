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
    check_data, check_tasks, implied_task, program_set_keys, same_program, same_program_set, Call,
    CarryReport, DataBlock, DataSupport, ExchangeReply, ForcedValue, NotCarried, OutputBinding,
    ProgramIdentity, ProgramVariables, ProgramVersion, RetainedValue, RuntimeMode, RuntimeProgram,
    RuntimeState, TaskConfig, TaskKind, TaskState, TaskSupport, WatchReply, WatchedValue,
    WireInput,
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
use sha2::{Digest, Sha256};
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
/// The most variables one watch may name, and the most a runtime is read for at one exchange:
/// the names of every watch of it, each once. Also the most watches one runtime keeps (a design
/// limit decided with C8d-core, not a figure of the protocol or of the VM).
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
    /// The data blocks its programs share (ADR-014): one master copy of each per runtime, each
    /// member written by one program at most. Left out, and absent from the file, when it has
    /// none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<DataBlock>,
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

    /// Which programs it runs, in program order: what a running build must also run for a new
    /// build to carry on from its variables, compared by [`same_program_set`].
    pub fn identities(&self) -> Vec<ProgramIdentity> {
        self.all_programs().iter().map(|p| p.identity()).collect()
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

    /// Each of its data blocks, with each member's writer, from the configuration alone: the
    /// program that declares the block and writes the member, or `None` when no program does.
    /// The same whether or not its module answers.
    pub fn data_status(&self) -> Vec<DataBlockStatus> {
        let programs = self.all_programs();
        self.data
            .iter()
            .map(|b| DataBlockStatus {
                name: b.name.clone(),
                ty: b.ty.clone(),
                layout: b.layout.clone(),
                retain: b.retain,
                members: b
                    .members
                    .iter()
                    .map(|m| DataMemberStatus {
                        name: m.name.clone(),
                        ty: m.ty.clone(),
                        writer: programs
                            .iter()
                            .find(|p| {
                                p.data.iter().any(|u| {
                                    u.name.eq_ignore_ascii_case(&b.name)
                                        && u.members.iter().any(|w| w.name == m.name && w.written)
                                })
                            })
                            .map(|p| p.name.clone()),
                    })
                    .collect(),
            })
            .collect()
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
        check_data(&programs, &self.data).map_err(|e| format!("runtime '{id}': {e}"))?;
        if self.tasks.is_empty() && programs.iter().any(|p| p.interval_ms == 0) {
            return Err(format!("runtime '{id}': interval_ms must be at least 1"));
        }
        // The project program each is a build of: never blank, and one build of it here.
        for (n, p) in programs.iter().enumerate() {
            let Some(from) = &p.built_from else {
                continue;
            };
            if from.trim().is_empty() {
                let at = if self.several() {
                    format!("runtime '{id}': program '{}'", p.name)
                } else {
                    format!("runtime '{id}'")
                };
                return Err(format!("{at}: built_from names no program of the project"));
            }
            for q in &programs[n + 1..] {
                if q.built_from
                    .as_ref()
                    .is_some_and(|f| f.eq_ignore_ascii_case(from))
                {
                    return Err(format!(
                        "runtime '{id}': programs '{}' and '{}' are both builds of program '{from}' of the project",
                        p.name, q.name
                    ));
                }
            }
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

/// The revision of a stored runtimes configuration: the lower hex SHA-256 of its bytes, as
/// they are on disk. No file is the revision of no bytes. Any byte changed, by a save or by
/// hand (whitespace too), gives another revision; an empty file has no bytes either, so it has
/// the revision of no file.
pub fn revision_of(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The bytes of the stored runtimes configuration at `path`; none when there is no file.
pub fn stored_bytes(path: &Path) -> Result<Vec<u8>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

impl RuntimesConfig {
    /// `Ok(None)` when the project has none.
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        Self::load_with_revision(path).map(|(config, _)| config)
    }

    /// [`Self::load`], with the [`revision_of`] the very bytes it parsed: one read, so the
    /// revision is of what is answered.
    pub fn load_with_revision(path: &Path) -> Result<(Option<Self>, String), String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((None, revision_of(&[])))
            }
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("{} is not a runtimes configuration: {e}", path.display()))?;
        config.validate()?;
        Ok((Some(config), revision_of(&bytes)))
    }

    /// What can be checked without asking a runtime module.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        let mut outputs: HashMap<&str, String> = HashMap::new();
        let mut published: HashMap<&str, String> = HashMap::new();
        // A data block's name, in lower case, and the runtime it is in.
        let mut blocks: HashMap<String, &str> = HashMap::new();
        // A project program's name, in lower case, with its spelling and the runtime it is in.
        let mut built: HashMap<String, (String, &str)> = HashMap::new();
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
            // One program of the project runs in one runtime, when both instances say which
            // project program theirs are builds of. An instance that does not say is not
            // compared: two project programs may declare one PROGRAM name.
            for p in i.all_programs() {
                if let Some(from) = &p.built_from {
                    if let Some((first, other)) =
                        built.insert(from.to_ascii_lowercase(), (from.clone(), i.id.as_str()))
                    {
                        return Err(format!(
                            "program '{}' of the project is in runtimes '{}' and '{}'; a program runs in one runtime: take it out of one of them in the same save",
                            first, other, i.id
                        ));
                    }
                }
            }
            // A data block is handed over inside one runtime; between runtimes is not built
            // yet (ADR-014 §5), and two independent copies of one name would mislead.
            for b in &i.data {
                if let Some(other) = blocks.insert(b.name.to_ascii_lowercase(), &i.id) {
                    return Err(format!(
                        "data block '{}' is in runtimes '{other}' and '{}'; a data block shared between runtimes is not built yet",
                        b.name, i.id
                    ));
                }
            }
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
    /// Its data blocks and each member's writer, from its configuration: known whether or not
    /// its module answers. Left out for a runtime that has none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<DataBlockStatus>,
}

/// One data block of a runtime, as its configuration says (ADR-014).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DataBlockStatus {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub layout: String,
    /// Whether its value is kept across a warm restart.
    pub retain: bool,
    pub members: Vec<DataMemberStatus>,
}

/// One top-level member of a data block, and the program that writes it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DataMemberStatus {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// The program that writes it; `None` (null, never left out) when no program writes it,
    /// so it keeps its initial or kept value.
    pub writer: Option<String>,
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
    /// The values kept under its id that it was not given, and why: kept for a program it does
    /// not run now, or for one that two of its programs could be.
    pub not_given: Vec<NotCarried>,
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
            not_given: Vec::new(),
        }
    }
}

/// What a new build asked of the running one: its variables, if it runs these programs in this
/// module.
struct PendingCarry {
    /// The project the new build is of: identities are names in one project.
    project: Option<String>,
    /// The new build's [`RuntimeInstance::identities`].
    program: Vec<ProgramIdentity>,
    module: String,
    reply: oneshot::Sender<Result<ProgramVariables, String>>,
}

/// A force or a release waiting for the instance's next exchange.
struct PendingForce {
    ask: ForceAsk,
    reply: oneshot::Sender<ForceAnswer>,
}

/// One watch of an instance: one list of variables (kept as the key it is found by).
struct Watch {
    /// When they were last asked for.
    asked: Instant,
    /// How many reads of the instance were kept when the list was first asked for: only a
    /// read kept after that can be its own.
    since: u64,
}

/// What a watch says of a name the module's answer has no value for. Never a value.
pub const NOT_GIVEN: &str = "the runtime's module gave no value for it";

/// The module's last read of an instance's watched variables, kept once for every list.
struct WatchRead {
    /// Its number among the reads of the instance, from 1.
    number: u64,
    /// When it was kept.
    at: Instant,
    /// The names it was asked for; `None` when the module did not answer, which fails every list.
    sent: Option<HashSet<String>>,
    /// The module's answer, with the place of each name among its values (the first, by the
    /// name as it was asked for), or why it gave none.
    answer: Result<(WatchReply, HashMap<String, usize>), String>,
}

/// The watches of one instance.
///
/// Its lock is the one every runtime of the server shares, so no step here does work for the
/// lists of other watchers: asking again with a list costs that list's names; the union is
/// kept as lists come and go, not worked out again from every list; and the module's answer is
/// kept once, each list taking its own names from it when it is read.
#[derive(Default)]
struct Watching {
    lists: HashMap<Vec<String>, Watch>,
    /// Every name of a list still watched, with how many lists name it: the union.
    named: BTreeMap<String, usize>,
    /// How many reads were kept.
    reads: u64,
    last: Option<WatchRead>,
}

/// The names of `list`, each once, in its order.
fn each_once(list: &[String]) -> impl Iterator<Item = &String> {
    let mut seen = HashSet::new();
    list.iter().filter(move |name| seen.insert(name.as_str()))
}

impl Watching {
    /// Let go of the lists nobody asked for within [`WATCH_KEPT`], and of their names.
    fn let_go(&mut self) {
        let now = Instant::now();
        let named = &mut self.named;
        self.lists.retain(|list, w| {
            let kept = now.saturating_duration_since(w.asked) <= WATCH_KEPT;
            if !kept {
                for name in each_once(list) {
                    if let Some(count) = named.get_mut(name) {
                        *count -= 1;
                        if *count == 0 {
                            named.remove(name);
                        }
                    }
                }
            }
            kept
        });
    }

    /// Watch a new list from now.
    fn add(&mut self, list: &[String]) {
        for name in each_once(list) {
            *self.named.entry(name.clone()).or_insert(0) += 1;
        }
        self.lists.insert(
            list.to_vec(),
            Watch {
                asked: Instant::now(),
                since: self.reads,
            },
        );
    }

    /// Keep a read of the module.
    fn keep(
        &mut self,
        sent: Option<HashSet<String>>,
        answer: Result<(WatchReply, HashMap<String, usize>), String>,
    ) {
        self.reads += 1;
        self.last = Some(WatchRead {
            number: self.reads,
            at: Instant::now(),
            sent,
            answer,
        });
    }
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
    /// What is being watched, by instance id: each list of names is a watch of its own.
    watches: HashMap<String, Watching>,
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
    /// from now. Each list of names (in its order, as spelt) is a watch of its own: asking with
    /// another list leaves the others as they are, and asking with the same one again only
    /// renews it. `Ok(false)` when there is no such instance.
    ///
    /// A new list is refused, with words, when the instance would then be read for more than
    /// [`WATCH_LIMIT`] names at each exchange, or keep more than [`WATCH_LIMIT`] lists; a list
    /// already watched never is. Lists nobody asked for within [`WATCH_KEPT`] are let go first.
    pub fn watch(&self, id: &str, variables: &[String]) -> Result<bool, String> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if !inner.statuses.contains_key(id) {
            return Ok(false);
        }
        let watching = inner.watches.entry(id.to_string()).or_default();
        watching.let_go(); // before the bounds
                           // Asked again: renewed, whatever the bounds, at the cost of its own names only.
        if let Some(watch) = watching.lists.get_mut(variables) {
            watch.asked = Instant::now();
            return Ok(true);
        }
        let other = watching.named.len();
        let more = each_once(variables)
            .filter(|name| !watching.named.contains_key(*name))
            .count();
        if other + more > WATCH_LIMIT {
            return Err(format!(
                "runtime '{id}' would be read for {n} variables at every exchange, and at most {WATCH_LIMIT} are: other watches name {other} of them; a watch nobody asks for again within {} s is let go",
                WATCH_KEPT.as_secs(),
                n = other + more,
            ));
        }
        if watching.lists.len() >= WATCH_LIMIT {
            return Err(format!(
                "runtime '{id}' has {n} watches already, and at most {WATCH_LIMIT} are kept: a watch nobody asks for again within {} s is let go",
                WATCH_KEPT.as_secs(),
                n = watching.lists.len(),
            ));
        }
        watching.add(variables);
        Ok(true)
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
    /// the programs `program` (a [`RuntimeInstance::identities`]) of `project` in `module` to
    /// carry on from. `None` when no instance of that id is loaded.
    fn ask_variables(
        &self,
        id: &str,
        project: Option<String>,
        program: Vec<ProgramIdentity>,
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
                project,
                program,
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

    /// What the instance's module last said of exactly these variables, in their order, and
    /// how long ago: each name's value found by the name as it was asked for, and a name the
    /// answer lacks with no value and saying so ([`NOT_GIVEN`]). `None`: nothing yet for this
    /// list, or it is let go, or the instance is gone. A list is given only a read made after it
    /// was first asked for, and only one that asked for every name of it: a list with a name
    /// that was not sent (watched since) waits for a read of its own. A module that did not
    /// answer fails every list.
    pub fn watched(
        &self,
        id: &str,
        variables: &[String],
    ) -> Option<(Duration, Result<WatchReply, String>)> {
        let inner = self.0.read().unwrap_or_else(|e| e.into_inner());
        let watching = inner.watches.get(id)?;
        let watch = watching.lists.get(variables)?;
        let read = watching
            .last
            .as_ref()
            .filter(|read| read.number > watch.since)?;
        if let Some(sent) = &read.sent {
            if !variables.iter().all(|name| sent.contains(name)) {
                return None;
            }
        }
        let answer = match &read.answer {
            Ok((reply, places)) => Ok(WatchReply {
                state: reply.state.clone(),
                values: variables
                    .iter()
                    .map(|name| match places.get(name) {
                        Some(&at) => reply.values[at].clone(),
                        None => WatchedValue {
                            variable: name.clone(),
                            ty: None,
                            value: None,
                            error: Some(NOT_GIVEN.to_string()),
                            forced: false,
                        },
                    })
                    .collect(),
            }),
            Err(why) => Err(why.clone()),
        };
        Some((read.at.elapsed(), answer))
    }

    /// Woken whenever a watch is answered: wait on it, then read [`RuntimeStates::watched`].
    pub fn answered(&self) -> Arc<Notify> {
        Arc::clone(&self.0.read().unwrap_or_else(|e| e.into_inner()).answered)
    }

    /// The variables to ask the instance's module for now, if anybody still watches: every
    /// name of every list still asked for, each once, in the order of the names, so one runtime
    /// is asked the same way every time. Lists nobody asked for within [`WATCH_KEPT`] are let go
    /// here.
    fn watch_wanted(&self, id: &str) -> Option<Vec<String>> {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        let watching = inner.watches.get_mut(id)?;
        watching.let_go(); // at the exchange
        if watching.lists.is_empty() {
            inner.watches.remove(id);
            return None;
        }
        Some(watching.named.keys().cloned().collect())
    }

    /// The module's answer to the names `sent`, kept once for every list of the instance: each
    /// list takes its own names from it when it is read ([`RuntimeStates::watched`]).
    fn watch_answer(&self, id: &str, sent: &[String], answer: Result<WatchReply, String>) {
        // Made ready before the lock is taken: under it, only what is kept is replaced.
        let sent: HashSet<String> = sent.iter().cloned().collect();
        let answer = answer.map(|reply| {
            let mut places = HashMap::with_capacity(reply.values.len());
            for (at, value) in reply.values.iter().enumerate() {
                places.entry(value.variable.clone()).or_insert(at);
            }
            (reply, places)
        });
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if let Some(watching) = inner.watches.get_mut(id) {
            watching.keep(Some(sent), answer);
        }
        inner.answered.notify_waiters();
    }

    /// The module does not answer: every list of the instance is told why, and none keeps a
    /// value.
    fn watch_failed(&self, id: &str, why: String) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if let Some(watching) = inner.watches.get_mut(id) {
            watching.keep(None, Err(why));
        }
        inner.answered.notify_waiters();
    }

    fn wanted(&self, id: &str) -> Option<bool> {
        let inner = self.0.read().unwrap_or_else(|e| e.into_inner());
        inner.wanted.get(id).copied()
    }

    /// What a driver says of its instance. A driver that has been replaced goes on reporting
    /// until it is stopped: its report is of the build before, and never takes the place of
    /// its replacement's (generations only grow, so the larger one is the newer driver).
    /// Without this the driver before would be the one listed again, and its stop would then
    /// forget the instance while its replacement runs.
    fn report(&self, generation: u64, status: RuntimeStatus) {
        let mut inner = self.0.write().unwrap_or_else(|e| e.into_inner());
        if inner
            .statuses
            .get(&status.id)
            .is_some_and(|(newer, _)| *newer > generation)
        {
            return;
        }
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
/// engineer's configuration and is not rewritten every few seconds. Each value is kept for the
/// program it is of ([`KeptFile`]), and given only to that program.
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
        Some(self.file_in(&self.project()?, id))
    }

    /// The active project; `None` when none is, or it cannot be read.
    pub fn project(&self) -> Option<String> {
        self.storage.get_active_project().ok().flatten()
    }

    /// The file of runtime `id` of `project`.
    pub fn file_in(&self, project: &str, id: &str) -> PathBuf {
        self.storage
            .system_dir()
            .join("retain")
            .join(project)
            .join(format!("{id}.json"))
    }

    /// What is kept in `path`: nothing when there is no such file. A file that cannot be read,
    /// or holds neither the values of before (an array) nor a [`KeptFile`], is said and taken
    /// as nothing, so the program starts from its initial values rather than from a guess.
    pub fn read(path: &Path) -> Kept {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Kept::Nothing,
            Err(e) => {
                tracing::error!(
                    "the RETAIN values in {} cannot be read: {e}",
                    path.display()
                );
                return Kept::Nothing;
            }
        };
        Kept::parse(&text).unwrap_or_else(|e| {
            tracing::error!("{} holds no RETAIN values: {e}", path.display());
            Kept::Nothing
        })
    }

    fn write(&self, path: &Path, file: &KeptFile) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(file).map_err(|e| e.to_string())?;
        self.storage
            .atomic_write_file(path, &bytes)
            .map_err(|e| e.to_string())
    }
}

/// What `<id>.json` holds since this slice. A file of before is a bare array (`Kept::Legacy`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptFile {
    /// The values of each program that had any, in the runtime's program order.
    pub programs: Vec<KeptProgram>,
    /// The data blocks' master copies (`layout` given, `path` the block's name), as the module
    /// gave them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<RetainedValue>,
}

/// The RETAIN values kept for one program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptProgram {
    /// The PROGRAM it was.
    pub name: String,
    /// The project program it was a build of, when its build said; never guessed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_from: Option<String>,
    /// The making of that project program its build said (`RuntimeProgram::created`); never
    /// guessed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// Its values, each `path` the variable's own, without the program's name.
    pub values: Vec<RetainedValue>,
}

/// What a retain file holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Kept {
    /// No file, or one that could not be read.
    Nothing,
    /// A file of before this slice: the values as the module gave them, of no program said.
    Legacy(Vec<RetainedValue>),
    /// The values per program.
    Programs(KeptFile),
}

impl Kept {
    /// A retain file's text: an array is a file of before, an object a [`KeptFile`].
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if value.is_array() {
            serde_json::from_value(value)
                .map(Kept::Legacy)
                .map_err(|e| e.to_string())
        } else {
            serde_json::from_value(value)
                .map(Kept::Programs)
                .map_err(|e| e.to_string())
        }
    }
}

/// The module's answer for RETAIN values as a retain file: each value kept for the program it
/// is of, with its path bare. With one program every value is its own; with several, a value is
/// of the program its name starts with (`<program>.`, without regard to case), as the module
/// names them. A data block's master is kept as given. A value of no program of the runtime is
/// not kept, and the second part says each such value.
pub fn kept_of(instance: &RuntimeInstance, values: &[RetainedValue]) -> (KeptFile, Vec<String>) {
    let programs = instance.all_programs();
    let several = programs.len() > 1;
    let mut kept: Vec<KeptProgram> = programs
        .iter()
        .map(|p| KeptProgram {
            name: p.name.clone(),
            built_from: p.built_from.clone(),
            created: p.created.clone(),
            values: Vec::new(),
        })
        .collect();
    let mut data = Vec::new();
    let mut dropped = Vec::new();
    for v in values {
        if v.layout.is_some() {
            data.push(v.clone());
            continue;
        }
        let found: Option<(usize, &str)> = if !several {
            (!programs.is_empty()).then_some((0, v.path.as_str()))
        } else {
            v.path.split_once('.').and_then(|(prefix, rest)| {
                programs
                    .iter()
                    .position(|p| p.name.eq_ignore_ascii_case(prefix.trim()))
                    .map(|i| (i, rest))
            })
        };
        match found {
            Some((i, path)) => kept[i].values.push(RetainedValue {
                path: path.to_string(),
                ..v.clone()
            }),
            None => dropped.push(format!(
                "a RETAIN value runtime '{}' gave names no program of it: {}",
                instance.id, v.path
            )),
        }
    }
    kept.retain(|k| !k.values.is_empty());
    (
        KeptFile {
            programs: kept,
            data,
        },
        dropped,
    )
}

/// What a runtime is given of what is kept under its id at a warm start, in its module's own
/// form: each program's values only to the program of the runtime that is the same program
/// ([`same_program`]), named `<program>.<path>` when it has several programs and bare when it
/// has one; every data block's master as kept. An entry two programs could be is given to
/// neither, and a program two entries could be is given neither: a variable is never given
/// two values kept for two programs. The second part is each value not given, and why; the
/// third whether the file is to be written again at the first keep: it is of before this
/// slice, or holds values not given.
pub fn given_of(
    instance: &RuntimeInstance,
    kept: &Kept,
) -> (Vec<RetainedValue>, Vec<NotCarried>, bool) {
    let file = match kept {
        Kept::Nothing => return (Vec::new(), Vec::new(), false),
        // Of no program said: given as before, and kept again in the new form.
        Kept::Legacy(values) => return (values.clone(), Vec::new(), true),
        Kept::Programs(file) => file,
    };
    let programs = instance.all_programs();
    let several = programs.len() > 1;
    let identity = |entry: &KeptProgram| ProgramIdentity {
        name: entry.name.clone(),
        built_from: entry.built_from.clone(),
        created: entry.created.clone(),
    };
    // The program an entry was kept for, as a message names it.
    let of = |entry: &KeptProgram| entry.built_from.clone().unwrap_or(entry.name.clone());
    // For each entry, the programs of the runtime it could be.
    let matching: Vec<Vec<usize>> = file
        .programs
        .iter()
        .map(|entry| {
            programs
                .iter()
                .enumerate()
                .filter(|(_, p)| same_program(&identity(entry), &p.identity()))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    let mut given = Vec::new();
    let mut not_given = Vec::new();
    for (entry, could_be) in file.programs.iter().zip(&matching) {
        let kept_for = of(entry);
        let withhold = |not_given: &mut Vec<NotCarried>, why: String| {
            not_given.extend(entry.values.iter().map(|v| NotCarried {
                variable: format!("{kept_for}.{}", v.path),
                why: why.clone(),
            }));
        };
        match could_be.as_slice() {
            [i] => {
                // The entries that could be only this program: more than one, and none is given.
                let claims: Vec<String> = file
                    .programs
                    .iter()
                    .zip(&matching)
                    .filter(|(_, m)| m.as_slice() == [*i])
                    .map(|(e, _)| format!("'{}'", of(e)))
                    .collect();
                if claims.len() > 1 {
                    withhold(
                        &mut not_given,
                        format!(
                            "program '{}' could be any of {}, each kept apart: it is given none of them",
                            programs[*i].name,
                            claims.join(" or ")
                        ),
                    );
                    continue;
                }
                let p = programs[*i];
                given.extend(entry.values.iter().map(|v| RetainedValue {
                    path: if several {
                        format!("{}.{}", p.name, v.path)
                    } else {
                        v.path.clone()
                    },
                    ..v.clone()
                }));
            }
            [] => {
                // Kept for a program of the same project name made apart from this one: deleted
                // and made again under its name, it is another program.
                let made_apart = entry.built_from.as_deref().is_some_and(|from| {
                    programs.iter().any(|p| {
                        p.built_from
                            .as_deref()
                            .is_some_and(|f| f.eq_ignore_ascii_case(from))
                    })
                });
                withhold(
                    &mut not_given,
                    if made_apart {
                        format!(
                            "kept for program '{kept_for}' as it was made before: runtime '{}' runs a program made again under that name, which is another program",
                            instance.id
                        )
                    } else {
                        format!(
                            "kept for program '{kept_for}', which runtime '{}' does not run now",
                            instance.id
                        )
                    },
                );
            }
            more => {
                let names: Vec<String> = more
                    .iter()
                    .map(|i| format!("'{}'", programs[*i].name))
                    .collect();
                withhold(
                    &mut not_given,
                    format!("it could be of {}", names.join(" or ")),
                );
            }
        }
    }
    given.extend(file.data.iter().cloned());
    let rewrite = !not_given.is_empty();
    (given, not_given, rewrite)
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
    /// The data blocks it says it hands over (`profile.data`): `None` for a module that hands
    /// none over.
    data_support: Option<DataSupport>,
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
    /// The project it was started for, when one was active: what it runs is named in that
    /// project, and a build of another project never carries on from its variables.
    project: Option<String>,
    /// Where its RETAIN values are kept, and what was kept last, in its module's own form:
    /// `None` when the next answer is to be written whatever it is.
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
            data_support: DataSupport::of(&module.manifest.profile),
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
            project: None,
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
        let Some(instance) = self.instance.as_ref() else {
            return;
        };
        let (file, dropped) = kept_of(instance, &values);
        for why in dropped {
            tracing::warn!("{why}");
        }
        match store.write(&path, &file) {
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
        project: Option<String>,
        instance: &RuntimeInstance,
    ) -> Result<(ProgramVariables, Instant), String> {
        let Some(answer) = self.env.states.ask_variables(
            &instance.id,
            project,
            instance.identities(),
            &self.module,
        ) else {
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
                not_given: Vec::new(),
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
    /// program of the same project in the same module, and only while this one is in RUN and
    /// nobody asked it to stop.
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
            .map(RuntimeInstance::identities)
            .unwrap_or_default();
        for ask in asks {
            let answer = if ask.project != self.project {
                // A program's identity is a name in its project: two projects' programs of one
                // name are two programs.
                let of = |p: &Option<String>| match p {
                    Some(p) => format!("project '{p}'"),
                    None => "no project known".to_string(),
                };
                Err(format!(
                    "the program running was of {}, not of {}",
                    of(&self.project),
                    of(&ask.project)
                ))
            } else if !same_program_set(&ask.program, &program) {
                // What was compared: the project names when both say them, else the names.
                let (running, asked, by_project) = program_set_keys(&program, &ask.program);
                let lower = |keys: &[String]| -> Vec<String> {
                    keys.iter().map(|k| k.to_ascii_lowercase()).collect()
                };
                if by_project && lower(&running) == lower(&asked) {
                    // The same names, another making of one of them.
                    Err(format!(
                        "the program running was '{}' as made before: a program made again under its name is another program",
                        running.join(",")
                    ))
                } else {
                    Err(format!(
                        "the program running was '{}', not '{}'",
                        running.join(","),
                        asked.join(",")
                    ))
                }
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

    /// Write the outputs the program changed since they were last confirmed. With tasks
    /// written out, an output the reply leaves out, of a program whose task has ended no run
    /// since RUN, is written its safe value: that can last without limit (a task's phase, an
    /// event that does not come), and nothing else writes an output the runtime claims. Given
    /// no tasks, its one program's outputs are left out only until its first scan ends, and are
    /// left as they were, as before there were tasks.
    async fn write_outputs(&mut self, reply: &ExchangeReply) {
        let Some(instance) = &self.instance else {
            return;
        };
        let owner = Self::owner(&instance.id);
        let reported: HashSet<&str> = reply.outputs.iter().map(|o| o.tag.as_str()).collect();
        let silent: Vec<&OutputBinding> = if instance.tasks.is_empty() {
            Vec::new()
        } else {
            instance
                .outputs()
                .into_iter()
                .map(|(_, o)| o)
                .filter(|o| !reported.contains(o.tag.as_str()))
                .collect()
        };
        self.safe.retain(|tag| silent.iter().any(|o| &o.tag == tag));
        for o in silent {
            if self.safe.contains(&o.tag) {
                continue;
            }
            if self.claim.is_some_and(|c| !self.env.io.holds(&o.tag, c)) {
                continue;
            }
            self.written.remove(&o.tag);
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
                data: instance.data_status(),
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
        let problem = data_problem(
            &self.module,
            &self.module_version,
            self.data_support.as_ref(),
            instance,
        )
        .or_else(|| {
            support_problem(
                &self.module,
                &self.module_version,
                self.support.as_ref(),
                instance,
            )
        });
        match problem {
            Some(problem) => Err(PluginError::InvalidConfiguration(problem)),
            None => Ok(()),
        }
    }
}

/// Why the runtime module `module` (version `version`), which says it hands over the data
/// blocks `support`, would not run `instance` as written (ADR-014); `None` when it would.
/// Checked before [`support_problem`]: a runtime that otherwise runs as today still is not run
/// by a module that does not say it hands data blocks over, because such a module ignores
/// `data` and every program would run on its own copy, never handed anything.
fn data_problem(
    module: &str,
    version: &str,
    support: Option<&DataSupport>,
    instance: &RuntimeInstance,
) -> Option<String> {
    if instance.data.is_empty() || support.is_some_and(|s| s.tiers.contains(&1)) {
        return None;
    }
    let names: Vec<&str> = instance.data.iter().map(|b| b.name.as_str()).collect();
    Some(format!(
        "runtime '{}' has data blocks ({}); the runtime module '{module}' (version {version}) does not say it hands data blocks over: install a newer build of it",
        instance.id,
        names.join(", ")
    ))
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
            if instance.program.is_none() {
                // Such a module reads its one program from `program`, and is not sent one.
                return Some(format!(
                    "runtime '{id}' gives its one program in programs; {module} reads it only from program and does not say it runs tasks: give it as program, or install a newer build of it"
                ));
            }
            return None;
        }
        let programs = instance.all_programs();
        let has = match (instance.tasks.first(), programs.as_slice()) {
            (Some(task), [program]) => {
                format!(
                    "task '{}' of kind {}{}",
                    task.name,
                    task.kind.name(),
                    not_as_today(task, program)
                )
            }
            _ => format!("{} programs", programs.len()),
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

/// What makes a cyclic task of one program not the program's implied task, as the words that
/// follow its kind in a refusal: empty for another kind, which its kind already says.
fn not_as_today(task: &TaskConfig, program: &RuntimeProgram) -> String {
    let TaskKind::Cyclic {
        interval_ms,
        phase_ms,
    } = task.kind
    else {
        return String::new();
    };
    let mut why = Vec::new();
    if interval_ms != program.interval_ms {
        why.push(format!(
            "interval_ms {interval_ms}, not its program's {}",
            program.interval_ms
        ));
    }
    if phase_ms != 0 {
        why.push(format!("phase_ms {phase_ms}"));
    }
    if let Some(ms) = task.watchdog_ms {
        why.push(format!("watchdog_ms {ms}"));
    }
    if why.is_empty() {
        String::new()
    } else {
        format!(" with {}", why.join(" and "))
    }
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
        // The project active now, once: the retain file is of it, and an online change only
        // from a build of it.
        let project = self.env.retain.as_ref().and_then(RetainStore::project);
        let retain_file = match (&self.env.retain, &project) {
            (Some(store), Some(project)) => Some(store.file_in(project, &instance.id)),
            _ => None,
        };
        // How it starts (ADR-013 §2.5). Cold when the load asks for it. Changed online when it
        // replaces a running build of the same program: it takes over that build's variables.
        // Otherwise as at a warm restart: the values its RETAIN variables had when it last ran,
        // kept by this server.
        let cold = self.env.states.cold_load();
        let kept = match (&retain_file, cold) {
            (Some(path), false) => RetainStore::read(path),
            _ => Kept::Nothing,
        };
        // Each program is given only the values kept for it; what is not given is said.
        let (given, not_given, rewrite) = given_of(&instance, &kept);
        if !not_given.is_empty() {
            tracing::warn!(
                "runtime '{}' was not given RETAIN values kept under its id: {}",
                instance.id,
                not_given
                    .iter()
                    .map(|n| format!("{} ({})", n.variable, n.why))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        let carried = if cold {
            Err((LoadStart::Cold, "a cold load was asked for".to_string()))
        } else if !instance.run {
            Err((
                LoadStart::Warm,
                "it is loaded in STOP, and starts as at a warm restart when it is put in RUN"
                    .to_string(),
            ))
        } else {
            self.running_variables(project.clone(), &instance)
                .await
                .map_err(|why| (LoadStart::Warm, why))
        };
        let mut sent = config.clone();
        if let Value::Object(map) = &mut sent {
            map.insert(
                "retained".to_string(),
                serde_json::to_value(&given).unwrap_or(Value::Null),
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
        let load = RuntimeLoad {
            not_given,
            ..match carried {
                Err((start, why)) => RuntimeLoad::not_online(&instance.id, start, why),
                Ok(_) => self.what_it_carried(&instance.id).await,
            }
        };
        self.project = project;
        self.retain_file = retain_file;
        // After a cold load nothing is taken as kept, so its first exchange keeps its values in
        // place of those of the build it replaced. The same after a start that read a file of
        // before or did not give all it read: the file then holds only its own programs' values,
        // in the new form, from its first keep.
        self.kept = if rewrite {
            None
        } else {
            (!cold).then_some(given)
        };
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
                self.env
                    .states
                    .watch_failed(&id, format!("the runtime's module does not answer: {e}"));
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

        // The variables somebody watches, as this scan left them: one call for every list
        // watched, each list answered from it by its own names. Reading changes nothing in the
        // program, and a module that cannot be watched says so.
        if let Some(union) = self.env.states.watch_wanted(&id) {
            let answer = self
                .process
                .call(
                    Call::Watch {
                        variables: union.clone(),
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
            self.env.states.watch_answer(&id, &union, answer);
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
                data: Vec::new(),
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

    /// A build replaces a running one: the new driver reports, then the driver before reports
    /// once more (it runs until it is stopped), then it is stopped. The instance stays the
    /// replacement's all along: it is still loaded, so a command reaches it, and the driver
    /// before knows it was replaced (so it does not keep its RETAIN values over the new ones).
    /// Seen in CI: without it the Run that follows a load was answered "no runtime is loaded".
    #[test]
    fn a_replaced_drivers_late_report_and_stop_leave_its_replacement_loaded() {
        let states = RuntimeStates::default();
        loaded(&states, "t", 1);
        loaded(&states, "t", 2);
        loaded(&states, "t", 1);
        assert!(
            states.replaced("t", 1),
            "the driver before is still replaced after its late report"
        );
        assert!(!states.replaced("t", 2));
        states.forget("t", 1);
        assert!(
            states.command("t", true),
            "the replacement is still loaded after the driver before stopped"
        );
        // The same driver reporting again, and a newer one, are taken as before.
        loaded(&states, "t", 2);
        loaded(&states, "t", 3);
        assert!(states.replaced("t", 2));
        states.forget("t", 3);
        assert!(!states.command("t", true), "its own stop forgets it");
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
        assert_eq!(states.watch("tank", &a), Ok(false), "nothing is loaded");
        assert_eq!(states.watch_wanted("tank"), None);

        loaded(&states, "tank", 1);
        assert_eq!(states.watch("tank", &a), Ok(true));
        assert_eq!(states.watch_wanted("tank"), Some(a.clone()));
        assert!(states.watched("tank", &a).is_none(), "asked, not answered");

        states.watch_answer("tank", &a, Ok(reply("a")));
        let (_, answer) = states.watched("tank", &a).expect("answered");
        assert_eq!(answer.unwrap().values[0].variable, "a");
        assert!(states.watched("tank", &b).is_none(), "not an answer for b");

        // Other variables are asked for: a watch of its own. What was answered for the first
        // stays the first's, and an answer to the first's names is not taken for the second.
        assert_eq!(states.watch("tank", &b), Ok(true));
        assert!(states.watched("tank", &b).is_none());
        states.watch_answer("tank", &a, Ok(reply("a")));
        assert!(states.watched("tank", &b).is_none(), "b was not read");
        let (_, answer) = states.watched("tank", &a).expect("a keeps its own answer");
        assert_eq!(answer.unwrap().values[0].variable, "a");
        states.watch_answer("tank", &b, Err("the module does not answer".into()));
        let (_, answer) = states.watched("tank", &b).expect("answered");
        assert_eq!(answer.unwrap_err(), "the module does not answer");
        assert!(
            states.watched("tank", &a).is_none(),
            "an answer to b's names is not a's, neither its values nor its failure: a waits"
        );
    }

    /// As if `list` of `id` was last asked for `ago` back.
    fn asked_back(states: &RuntimeStates, id: &str, list: &[String], ago: Duration) {
        let then = Instant::now()
            .checked_sub(ago)
            .expect("the clock has run that long");
        states
            .0
            .write()
            .unwrap()
            .watches
            .get_mut(id)
            .unwrap()
            .lists
            .get_mut(list)
            .unwrap()
            .asked = then;
    }

    /// Longer ago than a watch is kept.
    const LONG_AGO: Duration = Duration::from_secs(WATCH_KEPT.as_secs() + 1);

    #[test]
    fn a_watch_nobody_renews_stops_and_one_of_an_unloaded_instance_is_forgotten() {
        let states = RuntimeStates::default();
        let a = vec!["a".to_string()];
        loaded(&states, "tank", 1);
        assert_eq!(states.watch("tank", &a), Ok(true));
        asked_back(&states, "tank", &a, LONG_AGO);
        assert_eq!(states.watch_wanted("tank"), None, "nobody watches any more");
        assert!(states.watched("tank", &a).is_none(), "and it is gone");

        // Renewed, it is asked for again; unloaded, it is forgotten with the instance.
        assert_eq!(states.watch("tank", &a), Ok(true));
        assert_eq!(states.watch_wanted("tank"), Some(a.clone()));
        states.forget("tank", 1);
        assert_eq!(states.watch_wanted("tank"), None);
        assert_eq!(states.watch("tank", &a), Ok(false));
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    /// A module's answer giving each of `values` (name, value), in that order.
    fn replying(values: &[(&str, i64)]) -> WatchReply {
        let values: Vec<Value> = values
            .iter()
            .map(|(n, v)| json!({"variable": n, "type": "INT", "value": v}))
            .collect();
        serde_json::from_value(json!({
            "state": {"mode": "run", "program": "P", "version": "1", "scans": 1,
                      "last_scan_us": 1, "max_scan_us": 1, "overruns": 0},
            "values": values
        }))
        .unwrap()
    }

    /// What a list was answered: each name as given, with its value and its error.
    fn answered(
        states: &RuntimeStates,
        list: &[String],
    ) -> Vec<(String, Option<Value>, Option<String>)> {
        let (_, answer) = states.watched("tank", list).expect("answered");
        answer
            .expect("values")
            .values
            .into_iter()
            .map(|v| (v.variable, v.value, v.error))
            .collect()
    }

    fn given(name: &str, value: i64) -> (String, Option<Value>, Option<String>) {
        (name.to_string(), Some(json!(value)), None)
    }

    #[test]
    fn two_watchers_of_one_runtime_each_read_the_values_of_their_own_list() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        let (first, second) = (names(&["b", "a"]), names(&["c"]));
        assert_eq!(states.watch("tank", &first), Ok(true));
        assert_eq!(states.watch("tank", &second), Ok(true));
        let union = states.watch_wanted("tank").unwrap();
        states.watch_answer(
            "tank",
            &union,
            Ok(replying(&[("a", 1), ("b", 2), ("c", 3)])),
        );
        assert_eq!(
            answered(&states, &first),
            vec![given("b", 2), given("a", 1)],
            "its own names, in its own order"
        );
        assert_eq!(answered(&states, &second), vec![given("c", 3)]);

        // The same list asked again is the same watch: it keeps its answer.
        assert_eq!(states.watch("tank", &names(&["b", "a"])), Ok(true));
        assert_eq!(
            answered(&states, &first),
            vec![given("b", 2), given("a", 1)]
        );
        // The same names in another order are another list, not yet answered.
        assert_eq!(states.watch("tank", &names(&["a", "b"])), Ok(true));
        assert!(states.watched("tank", &names(&["a", "b"])).is_none());
    }

    #[test]
    fn the_runtime_is_asked_for_the_union_of_its_lists_in_one_call() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        for list in [&["c", "a", "d"][..], &["b", "c"], &["a", "b"], &["e"]] {
            assert_eq!(states.watch("tank", &names(list)), Ok(true));
        }
        asked_back(&states, "tank", &names(&["e"]), LONG_AGO);
        assert_eq!(
            states.watch_wanted("tank"),
            Some(names(&["a", "b", "c", "d"])),
            "every name of every live list, each once, none of the list let go"
        );
    }

    #[test]
    fn a_list_is_answered_from_the_unions_reply_by_the_names_as_asked() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        let list = names(&["x", "a", "A"]);
        assert_eq!(states.watch("tank", &list), Ok(true));
        let union = states.watch_wanted("tank").unwrap();
        // The module gives a value for "a" only: "A" is another name, and "x" has none.
        states.watch_answer("tank", &union, Ok(replying(&[("a", 7)])));
        let none = |name: &str| (name.to_string(), None, Some(NOT_GIVEN.to_string()));
        assert_eq!(
            answered(&states, &list),
            vec![none("x"), given("a", 7), none("A")]
        );
        let (_, answer) = states.watched("tank", &list).unwrap();
        let x = &answer.unwrap().values[0];
        assert_eq!(
            (&x.ty, x.forced),
            (&None, false),
            "no type, not forced: {x:?}"
        );
    }

    #[test]
    fn a_list_added_after_the_runtime_was_read_waits_for_a_read_of_its_own() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        let (a, ab) = (names(&["a"]), names(&["a", "b"]));
        assert_eq!(states.watch("tank", &a), Ok(true));
        let sent = states.watch_wanted("tank").unwrap();
        // Asked for while the runtime was read for the names sent.
        assert_eq!(states.watch("tank", &ab), Ok(true));
        states.watch_answer("tank", &sent, Ok(replying(&[("a", 1)])));
        assert_eq!(answered(&states, &a), vec![given("a", 1)]);
        assert!(
            states.watched("tank", &ab).is_none(),
            "b was not read: no answer, not a's value with b missing"
        );

        let sent = states.watch_wanted("tank").unwrap();
        assert_eq!(sent, ab);
        states.watch_answer("tank", &sent, Ok(replying(&[("a", 2), ("b", 3)])));
        assert_eq!(answered(&states, &ab), vec![given("a", 2), given("b", 3)]);
        assert_eq!(answered(&states, &a), vec![given("a", 2)]);
    }

    #[test]
    fn a_list_nobody_asks_for_again_within_5_s_stops_being_read_while_another_is_still_read() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        let (a, b) = (names(&["a"]), names(&["b"]));
        assert_eq!(states.watch("tank", &a), Ok(true));
        assert_eq!(states.watch("tank", &b), Ok(true));
        asked_back(&states, "tank", &a, LONG_AGO);
        assert_eq!(states.watch_wanted("tank"), Some(b.clone()));
        assert!(states.watched("tank", &a).is_none(), "a is let go");
        states.watch_answer("tank", &b, Ok(replying(&[("b", 1)])));
        assert_eq!(answered(&states, &b), vec![given("b", 1)]);
    }

    /// The names v{from} to v{to - 1}.
    fn numbered(from: usize, to: usize) -> Vec<String> {
        (from..to).map(|i| format!("v{i}")).collect()
    }

    #[test]
    fn a_watch_that_would_make_the_runtime_read_more_than_256_variables_is_refused_with_words() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        let (first, second) = (numbered(0, 200), numbered(200, 256));
        assert_eq!(states.watch("tank", &first), Ok(true));
        assert_eq!(states.watch("tank", &second), Ok(true));
        // Names already read cost nothing more.
        assert_eq!(states.watch("tank", &names(&["v255", "v0"])), Ok(true));

        let third = names(&["v0", "new"]);
        assert_eq!(
            states.watch("tank", &third),
            Err("runtime 'tank' would be read for 257 variables at every exchange, and at most 256 are: other watches name 256 of them; a watch nobody asks for again within 5 s is let go".to_string())
        );
        let wanted = states.watch_wanted("tank").unwrap();
        assert_eq!(wanted.len(), 256);
        assert!(
            !wanted.contains(&"new".to_string()),
            "nothing is registered"
        );
        assert!(!states.0.read().unwrap().watches["tank"]
            .lists
            .contains_key(&third));

        // A list let go is dropped before the bound is counted, by the watch itself.
        asked_back(&states, "tank", &second, LONG_AGO);
        assert_eq!(states.watch("tank", &third), Ok(true));
    }

    #[test]
    fn a_list_already_watched_is_never_refused_by_the_bound_when_it_is_asked_again() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        let (first, second) = (numbered(0, 200), numbered(200, 256));
        assert_eq!(states.watch("tank", &first), Ok(true));
        assert_eq!(states.watch("tank", &second), Ok(true));
        asked_back(&states, "tank", &second, Duration::from_secs(4));
        assert_eq!(states.watch("tank", &second), Ok(true), "the union is full");
        assert!(
            states.0.read().unwrap().watches["tank"].lists[&second]
                .asked
                .elapsed()
                < Duration::from_secs(1),
            "and it is renewed"
        );

        // Nor by the number of lists.
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        for i in 0..WATCH_LIMIT {
            assert_eq!(states.watch("tank", &numbered(i, i + 1)), Ok(true));
        }
        assert_eq!(states.watch("tank", &numbered(7, 8)), Ok(true), "256 lists");
    }

    #[test]
    fn the_number_of_lists_of_one_instance_is_bounded() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        for i in 0..WATCH_LIMIT {
            assert_eq!(states.watch("tank", &numbered(i, i + 1)), Ok(true));
        }
        // No name more to read, one list more to keep.
        let more = numbered(0, 2);
        assert_eq!(
            states.watch("tank", &more),
            Err("runtime 'tank' has 256 watches already, and at most 256 are kept: a watch nobody asks for again within 5 s is let go".to_string())
        );
        assert_eq!(states.0.read().unwrap().watches["tank"].lists.len(), 256);
        assert!(!states.0.read().unwrap().watches["tank"]
            .lists
            .contains_key(&more));

        asked_back(&states, "tank", &numbered(9, 10), LONG_AGO);
        assert_eq!(states.watch("tank", &more), Ok(true), "one was let go");
    }

    #[test]
    fn a_module_that_does_not_answer_fails_every_list() {
        let states = RuntimeStates::default();
        loaded(&states, "tank", 1);
        let (a, b) = (names(&["a"]), names(&["b"]));
        assert_eq!(states.watch("tank", &a), Ok(true));
        assert_eq!(states.watch("tank", &b), Ok(true));
        let union = states.watch_wanted("tank").unwrap();
        states.watch_answer("tank", &union, Ok(replying(&[("a", 1), ("b", 2)])));

        let why = "the runtime's module does not answer: gone";
        states.watch_failed("tank", why.to_string());
        for list in [&a, &b] {
            let (_, answer) = states.watched("tank", list).expect("answered");
            assert_eq!(answer.unwrap_err(), why, "{list:?} keeps no value");
        }

        // Unloaded, every list is dropped.
        states.forget("tank", 1);
        assert!(states.watched("tank", &a).is_none());
        assert!(states.watched("tank", &b).is_none());
        assert!(!states.0.read().unwrap().watches.contains_key("tank"));
    }

    /// The fastest of several runs of `work`: what the work costs, not what the machine did
    /// meanwhile.
    fn fastest(work: &dyn Fn()) -> Duration {
        (0..15)
            .map(|_| {
                let start = Instant::now();
                work();
                start.elapsed()
            })
            .min()
            .unwrap()
    }

    /// The lock of the runtime states is the one every runtime of the server shares: each
    /// exchange of every runtime takes it. A watcher asking again with its list, and an
    /// exchange keeping the module's answer, must not do work for the other watchers' lists
    /// under it. Both bounds at once: 256 lists of 256 names (each a rotation of the same 256,
    /// so the union is 256 names whatever the number of lists). Measured against the same work
    /// with one list, on this machine, now: the ratio, not a time.
    #[test]
    fn asking_again_and_keeping_an_answer_cost_as_much_with_256_lists_as_with_one() {
        let all = numbered(0, WATCH_LIMIT);
        let rotation = |k: usize| {
            let mut list = all.clone();
            list.rotate_left(k);
            list
        };
        let (one, many) = (RuntimeStates::default(), RuntimeStates::default());
        for states in [&one, &many] {
            loaded(states, "tank", 1);
        }
        assert_eq!(one.watch("tank", &rotation(0)), Ok(true));
        for k in 0..WATCH_LIMIT {
            assert_eq!(many.watch("tank", &rotation(k)), Ok(true), "list {k}");
        }
        let mut union = all.clone();
        union.sort();
        assert_eq!(one.watch_wanted("tank"), Some(union.clone()));
        assert_eq!(many.watch_wanted("tank"), Some(union.clone()));
        let values: Vec<(&str, i64)> = all.iter().map(|n| (n.as_str(), 1)).collect();
        let reply = replying(&values);

        let first = rotation(0);
        let renewal = |states: &RuntimeStates| {
            fastest(&|| assert_eq!(states.watch("tank", &first), Ok(true)))
        };
        let keeping = |states: &RuntimeStates| {
            fastest(&|| states.watch_answer("tank", &union, Ok(reply.clone())))
        };
        let (renew_one, renew_many) = (renewal(&one), renewal(&many));
        let (keep_one, keep_many) = (keeping(&one), keeping(&many));
        assert!(
            renew_many < renew_one * 4,
            "asking again with one list of 256 costs {renew_one:?} beside no other list, and {renew_many:?} beside 255 others"
        );
        assert!(
            keep_many < keep_one * 4,
            "keeping the module's answer costs {keep_one:?} for one list, and {keep_many:?} for 256"
        );
        // And each list still reads its own names, in its own order, from the one answer.
        let (_, answer) = many.watched("tank", &rotation(7)).expect("answered");
        let names: Vec<String> = answer
            .unwrap()
            .values
            .into_iter()
            .map(|v| v.variable)
            .collect();
        assert_eq!(names, rotation(7));
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

    fn named(name: &str) -> ProgramIdentity {
        ProgramIdentity {
            name: name.into(),
            built_from: None,
            created: None,
        }
    }

    #[test]
    fn the_program_key_is_the_sorted_names_and_one_program_s_is_its_name() {
        assert_eq!(parsed(instance("t")).identities(), [named("P")]);
        let two = parsed(several(&["b", "a"], json!([cyclic("f", 20, &["a", "b"])])));
        assert_eq!(two.identities(), [named("b"), named("a")]);
        // Compared as sorted names: the order of the programs does not matter.
        let swapped = parsed(several(&["a", "b"], json!([cyclic("f", 20, &["a", "b"])])));
        assert!(same_program_set(&two.identities(), &swapped.identities()));
        assert_eq!(
            program_set_keys(&two.identities(), &[]).0,
            ["a".to_string(), "b".to_string()]
        );
        // Every program naming its project program: compared by those names.
        let mut built = several(&["b", "a"], json!([cyclic("f", 20, &["a", "b"])]));
        built["programs"][0]["built_from"] = json!("B1");
        built["programs"][1]["built_from"] = json!("A1");
        let built = parsed(built);
        // One side without project names: the names, which are equal.
        assert!(same_program_set(&built.identities(), &two.identities()));
        assert_eq!(
            program_set_keys(&built.identities(), &built.identities()).0,
            ["A1".to_string(), "B1".to_string()]
        );
    }

    #[test]
    fn two_different_program_sets_have_different_program_keys() {
        // A program name may hold a ','; joined into one string, these three sets were one key.
        let one = parsed(several(
            &["a,b", "c"],
            json!([cyclic("x", 10, &["a,b", "c"])]),
        ));
        let other = parsed(several(
            &["a", "b,c"],
            json!([cyclic("x", 10, &["a", "b,c"])]),
        ));
        let mut single = instance("t");
        single["program"]["name"] = json!("a,b");
        let single = parsed(single);
        let pair = parsed(several(&["a", "b"], json!([cyclic("x", 10, &["a", "b"])])));
        for i in [&one, &other, &single, &pair] {
            assert_eq!(check_tasks(&i.all_programs(), &i.tasks), Ok(()));
        }
        assert!(!same_program_set(&one.identities(), &other.identities()));
        assert!(!same_program_set(&single.identities(), &pair.identities()));
        assert_eq!(single.identities(), [named("a,b")]);
        // The same with project names that hold a ','.
        let from = |mut i: Value, names: &[&str]| {
            for (n, name) in names.iter().enumerate() {
                i["programs"][n]["built_from"] = json!(name);
            }
            parsed(i)
        };
        let one = from(
            several(&["x", "y"], json!([cyclic("x", 10, &["x", "y"])])),
            &["a,b", "c"],
        );
        let other = from(
            several(&["x", "y"], json!([cyclic("x", 10, &["x", "y"])])),
            &["a", "b,c"],
        );
        assert!(!same_program_set(&one.identities(), &other.identities()));
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
    fn one_program_written_in_programs_is_refused_by_an_older_module_in_its_words() {
        // Such a module reads its program from `program`: sent `programs`, it could not read
        // it. Refused here, in words that say what to do, whether or not a task is written out.
        let mut alone = several(&["Tank"], json!([]));
        alone["tasks"] = json!([]);
        let mut with_task = several(&["Tank"], json!([cyclic("main", 20, &["Tank"])]));
        with_task["tasks"][0]["priority"] = json!(31);
        for i in [alone, with_task] {
            let i = parsed(i);
            assert!(i.runs_as_today());
            assert_eq!(i.check(), Ok(()));
            assert_eq!(
                support_problem("vm", "0.4.0", None, &i).as_deref(),
                Some("runtime 't' gives its one program in programs; the runtime module 'vm' (version 0.4.0) reads it only from program and does not say it runs tasks: give it as program, or install a newer build of it")
            );
            let tasks = TaskSupport {
                kinds: vec!["cyclic".into()],
            };
            assert_eq!(support_problem("vm", "0.5.0", Some(&tasks), &i), None);
        }
    }

    #[test]
    fn the_refusal_of_one_cyclic_task_names_what_makes_it_not_run_as_today() {
        let refused = |task: Value| {
            let mut i = instance("t");
            i["tasks"] = json!([task]);
            support_problem("vm", "0.4.0", None, &parsed(i)).unwrap()
        };
        let older = "; the runtime module 'vm' (version 0.4.0) runs one program in one cyclic task and does not say it runs tasks: install a newer build of it";
        let mut task = cyclic("main", 20, &["P"]);
        task["watchdog_ms"] = json!(500);
        assert_eq!(
            refused(task),
            format!("runtime 't' has task 'main' of kind cyclic with watchdog_ms 500{older}")
        );
        let mut task = cyclic("main", 20, &["P"]);
        task["kind"]["phase_ms"] = json!(5);
        assert_eq!(
            refused(task),
            format!("runtime 't' has task 'main' of kind cyclic with phase_ms 5{older}")
        );
        let mut task = cyclic("main", 50, &["P"]);
        task["kind"]["phase_ms"] = json!(5);
        task["watchdog_ms"] = json!(500);
        assert_eq!(
            refused(task),
            format!("runtime 't' has task 'main' of kind cyclic with interval_ms 50, not its program's 20 and phase_ms 5 and watchdog_ms 500{older}")
        );
    }

    // -------------------------------------------------------------------------
    // Data blocks (ADR-014, D2)
    // -------------------------------------------------------------------------

    const LAYOUT: &str = "0123456789abcdef";

    /// The runtime's block `name` of type T: x and y, two DINTs.
    fn block(name: &str) -> Value {
        json!({"name": name, "type": "T", "layout": LAYOUT, "size": 8, "members": [
            {"name": "x", "type": "DINT", "offset": 0, "size": 4},
            {"name": "y", "type": "DINT", "offset": 4, "size": 4}
        ]})
    }

    /// A program's use of block `name`, writing x and y as `writes` says.
    fn use_of(name: &str, writes: [bool; 2]) -> Value {
        json!({"name": name, "type": "T", "layout": LAYOUT, "offset": 16, "size": 8,
               "constant": false, "members": [
            {"name": "x", "type": "DINT", "offset": 0, "size": 4, "written": writes[0]},
            {"name": "y", "type": "DINT", "offset": 4, "size": 4, "written": writes[1]}
        ]})
    }

    /// The runtime `t` of programs a and b in two tasks, sharing block S: a writes x, b
    /// writes nothing.
    fn sharing() -> Value {
        let mut i = several(
            &["a", "b"],
            json!([cyclic("f", 10, &["a"]), cyclic("s", 50, &["b"])]),
        );
        i["programs"][0]["data"] = json!([use_of("S", [true, false])]);
        i["programs"][1]["data"] = json!([use_of("S", [false, false])]);
        i["data"] = json!([block("S")]);
        i
    }

    #[test]
    fn validate_refuses_each_data_mistake_with_its_message() {
        assert_eq!(check(vec![sharing()]), Ok(()));
        let with = |change: &dyn Fn(&mut Value)| {
            let mut i = sharing();
            change(&mut i);
            check(vec![i]).unwrap_err()
        };
        type Change = dyn Fn(&mut Value);
        let cases: Vec<(&Change, &str)> = vec![
            (&|i| i["data"][0]["name"] = json!("1S"),
             "runtime 't': '1S' is not a data block name: use a letter or '_' first, then letters, digits or '_'"),
            (&|i| i["data"] = json!([block("S"), block("s")]),
             "runtime 't': two data blocks are named 's'"),
            (&|i| i["data"] = json!([block("S"), block("A")]),
             "runtime 't': data block 'A' has the name of a program of this runtime"),
            (&|i| i["data"][0]["layout"] = json!("0123456789ABCDEF"),
             "runtime 't': data block 'S': its layout '0123456789ABCDEF' is not 16 lowercase hex digits"),
            (&|i| i["data"][0]["members"][1]["offset"] = json!(6),
             "runtime 't': data block 'S': member 'y' lies outside its 8 bytes"),
            (&|i| i["programs"][0]["data"] = json!([use_of("S", [true, false]), use_of("s", [false, false])]),
             "runtime 't': program 'a' declares data block 's' twice"),
            (&|i| i["data"][0]["name"] = json!("Q"),
             "runtime 't': program 'a' uses data block 'S', which is not in the runtime's data (it has: Q)"),
            (&|i| i["data"] = json!([]),
             "runtime 't': program 'a' uses data block 'S', which is not in the runtime's data (it has none)"),
            (&|i| i["programs"][0]["data"][0]["layout"] = json!("fedcba9876543210"),
             "runtime 't': program 'a' was built with data block 'S' laid out as fedcba9876543210 (8 bytes), and the runtime's 'S' is laid out as 0123456789abcdef (8 bytes): build 'a' again"),
            (&|i| i["programs"][0]["data"][0]["type"] = json!("U"),
             "runtime 't': program 'a' was built with data block 'S' of type U with other members than the runtime's T: build 'a' again"),
            (&|i| i["programs"][1]["data"][0]["members"][1]["type"] = json!("REAL"),
             "runtime 't': program 'b' was built with data block 'S' of type T with other members than the runtime's T: build 'b' again"),
            (&|i| i["programs"][0]["data"][0]["constant"] = json!(true),
             "runtime 't': program 'a' declares data block 'S' CONSTANT and writes its member 'x'"),
            (&|i| i["programs"][1]["data"][0]["members"][0]["written"] = json!(true),
             "runtime 't': data block 'S': member 'x' is written by programs 'a' and 'b'; a member has one writer"),
            (&|i| i["programs"][1]["inputs"] = json!([{"variable": "s.x", "tag": "external.A.B.c"}]),
             "runtime 't': program 'b': s.x is a member of data block 'S', which only a program writes: a tag cannot be read into it"),
            (&|i| i["programs"][1]["published"] = json!([{"variable": "S.y", "tag": "internal.A.b.q", "writable": true}]),
             "runtime 't': program 'b': S.y is a member of data block 'S', which only a program writes: it cannot be a setpoint"),
        ];
        for (change, message) in cases {
            assert_eq!(with(change), message);
        }
        // A member is published read-only, and output, as any variable (ADR-014 §2.8).
        let mut read_only = sharing();
        read_only["programs"][1]["published"] =
            json!([{"variable": "S.y", "tag": "internal.A.b.q"}]);
        read_only["programs"][1]["outputs"] =
            json!([{"variable": "S.x", "tag": "external.A.b.o", "safe": 0}]);
        assert_eq!(check(vec![read_only]), Ok(()));
        // A required field left out is no configuration: a missing `written` would hide a
        // writer from the one-writer check.
        assert!(with(&|i| {
            i["programs"][1]["data"][0]["members"][0]
                .as_object_mut()
                .unwrap()
                .remove("written");
        })
        .contains("missing field `written`"));
        assert!(with(&|i| {
            i["programs"][0]["data"][0]
                .as_object_mut()
                .unwrap()
                .remove("constant");
        })
        .contains("missing field `constant`"));
        assert!(with(&|i| i["data"][0]["kept"] = json!(true)).contains("unknown field `kept`"));

        // A block no program declares is accepted (it has no master).
        let mut unused = sharing();
        unused["data"] = json!([block("S"), block("Spare")]);
        assert_eq!(check(vec![unused]), Ok(()));

        // R16: one block, in any case, in two runtimes.
        let mut other = several(&["c"], json!([cyclic("f", 10, &["c"])]));
        other["id"] = json!("u");
        other["data"] = json!([block("s")]);
        assert_eq!(
            check(vec![sharing(), other.clone()]).unwrap_err(),
            "data block 's' is in runtimes 't' and 'u'; a data block shared between runtimes is not built yet"
        );
        other["data"] = json!([block("Other")]);
        assert_eq!(check(vec![sharing(), other]), Ok(()));
    }

    #[test]
    fn a_data_mistake_in_a_saved_file_is_refused_at_load_in_the_same_words() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(RUNTIMES_FILE);
        let mut i = sharing();
        i["programs"][1]["data"][0]["members"][0]["written"] = json!(true);
        std::fs::write(&path, json!({"instances": [i]}).to_string()).unwrap();
        assert_eq!(
            RuntimesConfig::load(&path).unwrap_err(),
            "runtime 't': data block 'S': member 'x' is written by programs 'a' and 'b'; a member has one writer"
        );
        std::fs::write(&path, json!({"instances": [sharing()]}).to_string()).unwrap();
        assert!(RuntimesConfig::load(&path).unwrap().is_some());
    }

    #[test]
    fn the_status_names_each_members_writer_from_the_configuration() {
        let i = parsed(sharing());
        let status = serde_json::to_value(i.data_status()).unwrap();
        assert_eq!(
            status,
            json!([{"name": "S", "type": "T", "layout": LAYOUT, "retain": false, "members": [
                {"name": "x", "type": "DINT", "writer": "a"},
                {"name": "y", "type": "DINT", "writer": null}
            ]}]),
            "null, not left out, for a member no program writes"
        );
        // Each member's own writer, not the first program that writes any member.
        let mut both = sharing();
        both["programs"][1]["data"][0]["members"][1]["written"] = json!(true);
        both["data"][0]["retain"] = json!(true);
        let both = parsed(both);
        assert_eq!(both.check(), Ok(()));
        let writers: Vec<Option<String>> = both.data_status()[0]
            .members
            .iter()
            .map(|m| m.writer.clone())
            .collect();
        assert_eq!(writers, vec![Some("a".into()), Some("b".into())]);
        assert!(both.data_status()[0].retain);
        // A block no program declares: every member unwritten.
        let mut unused = sharing();
        unused["data"] = json!([block("S"), block("Spare")]);
        let unused = parsed(unused).data_status();
        assert!(unused[1].members.iter().all(|m| m.writer.is_none()));
        // A runtime of today has none, and writes no `data` key.
        let today = parsed(instance("t"));
        assert!(today.data_status().is_empty());
        assert!(serde_json::to_value(&today).unwrap().get("data").is_none());
    }

    #[test]
    fn a_module_that_does_not_say_it_hands_data_blocks_over_is_given_none() {
        let refusal = "runtime 't' has data blocks (S); the runtime module 'vm' (version 0.5.0) does not say it hands data blocks over: install a newer build of it";
        let i = parsed(sharing());
        // It says it runs tasks, and the runtime's tasks are of its kinds: the data refuses.
        let tasks = TaskSupport {
            kinds: vec!["cyclic".into()],
        };
        assert_eq!(support_problem("vm", "0.5.0", Some(&tasks), &i), None);
        assert_eq!(
            data_problem("vm", "0.5.0", None, &i).as_deref(),
            Some(refusal)
        );
        let tier_two = DataSupport { tiers: vec![2] };
        assert_eq!(
            data_problem("vm", "0.5.0", Some(&tier_two), &i).as_deref(),
            Some(refusal)
        );
        let tier_one = DataSupport { tiers: vec![1] };
        assert_eq!(data_problem("vm", "0.6.0", Some(&tier_one), &i), None);
        // One program in its implied task runs as today for tasks, not for data.
        let mut one = instance("t");
        one["program"]["data"] = json!([use_of("S", [true, true])]);
        one["data"] = json!([block("S"), block("Spare")]);
        let one = parsed(one);
        assert_eq!(one.check(), Ok(()));
        assert!(one.runs_as_today());
        assert_eq!(
            data_problem("vm", "0.4.0", None, &one).as_deref(),
            Some("runtime 't' has data blocks (S, Spare); the runtime module 'vm' (version 0.4.0) does not say it hands data blocks over: install a newer build of it")
        );
        // A runtime of today needs nothing of a module.
        assert_eq!(
            data_problem("vm", "0.4.0", None, &parsed(instance("t"))),
            None
        );
        // The module's profile says it.
        assert_eq!(
            DataSupport::of(&json!({"data": {"tiers": [1]}})),
            Some(tier_one)
        );
        assert_eq!(DataSupport::of(&Value::Null), None);
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

    // -------------------------------------------------------------------------
    // Which program: the project program a build is of (C8c-core)
    // -------------------------------------------------------------------------

    /// Runtime `id` running program `name`, a build of the project program `from` when given,
    /// with an output and a published tag of its own.
    fn built(id: &str, name: &str, from: Option<&str>) -> Value {
        let mut i = instance(id);
        i["program"]["name"] = json!(name);
        i["program"]["outputs"][0]["tag"] = json!(format!("external.A.{id}.d"));
        if let Some(from) = from {
            i["program"]["built_from"] = json!(from);
        }
        i
    }

    /// Runtime `t` of programs `A` and `B`, builds of the project programs `from` when given.
    fn pair(from: [Option<&str>; 2]) -> Value {
        let mut i = several(&["A", "B"], json!([cyclic("f", 20, &["A", "B"])]));
        for (n, from) in from.iter().enumerate() {
            if let Some(from) = from {
                i["programs"][n]["built_from"] = json!(from);
            }
        }
        i
    }

    #[test]
    fn a_program_naming_no_project_program_or_one_named_twice_in_a_runtime_is_refused() {
        assert_eq!(check(vec![built("t", "P", Some("Tank"))]), Ok(()));
        for blank in ["", "  "] {
            assert_eq!(
                check(vec![built("t", "P", Some(blank))]),
                Err("runtime 't': built_from names no program of the project".to_string())
            );
            assert_eq!(
                check(vec![pair([Some("A1"), Some(blank)])]),
                Err(
                    "runtime 't': program 'B': built_from names no program of the project"
                        .to_string()
                )
            );
        }
        assert_eq!(
            check(vec![pair([Some("Tank"), Some("tank")])]),
            Err("runtime 't': programs 'A' and 'B' are both builds of program 'Tank' of the project".to_string())
        );
        assert_eq!(check(vec![pair([Some("A1"), Some("B1")])]), Ok(()));
        assert_eq!(check(vec![pair([Some("A1"), None])]), Ok(()));
        // Also where a connection's configuration is read, at a start.
        let refused = RuntimeDriver::parse(&built("t", "P", Some(""))).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("runtime 't': built_from names no program of the project"),
            "{refused}"
        );
    }

    #[test]
    fn one_program_of_the_project_in_two_instances_is_refused_naming_both() {
        assert_eq!(
            check(vec![
                built("a", "P", Some("Tank1")),
                built("b", "Q", Some("Tank2"))
            ]),
            Ok(())
        );
        let refused = "program 'Tank1' of the project is in runtimes 'a' and 'b'; a program runs in one runtime: take it out of one of them in the same save";
        assert_eq!(
            check(vec![
                built("a", "P", Some("Tank1")),
                built("b", "Q", Some("tank1"))
            ]),
            Err(refused.to_string())
        );
        // Refused for what it is, before the output tag the two builds share would be.
        let mut a = instance("a");
        a["program"]["built_from"] = json!("Tank1");
        let mut b = instance("b");
        b["program"]["built_from"] = json!("tank1");
        assert_eq!(check(vec![a.clone(), b]), Err(refused.to_string()));
        // In a runtime of several programs too.
        assert_eq!(
            check(vec![built("a", "P", Some("Tank1")), pair([Some("X"), Some("TANK1")])]),
            Err("program 'Tank1' of the project is in runtimes 'a' and 't'; a program runs in one runtime: take it out of one of them in the same save".to_string())
        );
    }

    #[test]
    fn programs_that_do_not_name_their_project_program_are_not_compared() {
        assert_eq!(
            check(vec![built("a", "P", None), built("b", "P", None)]),
            Ok(())
        );
        assert_eq!(
            check(vec![built("a", "P", Some("P")), built("b", "P", None)]),
            Ok(())
        );
        assert_eq!(
            check(vec![built("a", "P", None), built("b", "P", Some("P"))]),
            Ok(())
        );
    }

    #[test]
    fn two_project_programs_declaring_one_program_name_run_in_two_runtimes() {
        assert_eq!(
            check(vec![
                built("a", "Tank", Some("T1")),
                built("b", "Tank", Some("T2"))
            ]),
            Ok(())
        );
    }

    #[test]
    fn a_saved_file_with_one_program_in_two_runtimes_is_refused_at_load_in_the_same_words() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(RUNTIMES_FILE);
        let two = vec![built("a", "P", Some("Tank")), built("b", "Q", Some("TANK"))];
        std::fs::write(&path, json!({ "instances": two }).to_string()).unwrap();
        assert_eq!(
            RuntimesConfig::load(&path).unwrap_err(),
            "program 'Tank' of the project is in runtimes 'a' and 'b'; a program runs in one runtime: take it out of one of them in the same save"
        );
        let apart = vec![built("a", "P", Some("Tank")), built("b", "Q", Some("Pump"))];
        std::fs::write(&path, json!({ "instances": apart }).to_string()).unwrap();
        assert!(RuntimesConfig::load(&path).unwrap().is_some());
    }

    fn retained(path: &str, bytes: &str) -> RetainedValue {
        RetainedValue {
            path: path.into(),
            ty: "DINT".into(),
            bytes: bytes.into(),
            layout: None,
        }
    }

    fn master(name: &str) -> RetainedValue {
        RetainedValue {
            path: name.into(),
            ty: "T".into(),
            bytes: "00000000".into(),
            layout: Some(LAYOUT.into()),
        }
    }

    fn kept(programs: Vec<(&str, Option<&str>, Vec<RetainedValue>)>) -> Kept {
        Kept::Programs(KeptFile {
            programs: programs
                .into_iter()
                .map(|(name, from, values)| KeptProgram {
                    name: name.into(),
                    built_from: from.map(str::to_string),
                    created: None,
                    values,
                })
                .collect(),
            data: Vec::new(),
        })
    }

    #[test]
    fn the_kept_file_of_one_program_and_of_several_is_written_as_this_text() {
        let one = parsed(built("t", "P", None));
        let (file, dropped) = kept_of(&one, &[retained("total", "2a000000"), master("S")]);
        assert!(dropped.is_empty());
        assert_eq!(
            String::from_utf8(serde_json::to_vec_pretty(&file).unwrap()).unwrap(),
            r#"{
  "programs": [
    {
      "name": "P",
      "values": [
        {
          "path": "total",
          "type": "DINT",
          "bytes": "2a000000"
        }
      ]
    }
  ],
  "data": [
    {
      "path": "S",
      "type": "T",
      "bytes": "00000000",
      "layout": "0123456789abcdef"
    }
  ]
}"#
        );
        // Several: in the runtime's program order, the program's name taken off, the project
        // name only where the build gave it, and no data key when there is no block.
        let two = parsed(pair([Some("A1"), None]));
        let (file, dropped) = kept_of(
            &two,
            &[
                retained("b.count", "01000000"),
                retained("A.total", "2a000000"),
            ],
        );
        assert!(dropped.is_empty());
        assert_eq!(
            String::from_utf8(serde_json::to_vec_pretty(&file).unwrap()).unwrap(),
            r#"{
  "programs": [
    {
      "name": "A",
      "built_from": "A1",
      "values": [
        {
          "path": "total",
          "type": "DINT",
          "bytes": "2a000000"
        }
      ]
    },
    {
      "name": "B",
      "values": [
        {
          "path": "count",
          "type": "DINT",
          "bytes": "01000000"
        }
      ]
    }
  ]
}"#
        );
        // It reads back as what was written.
        let text = serde_json::to_string(&file).unwrap();
        assert_eq!(Kept::parse(&text), Ok(Kept::Programs(file)));
    }

    #[test]
    fn a_value_the_module_gives_is_kept_for_the_program_its_name_says() {
        // One program: every value without a layout is its, whatever its name.
        let one = parsed(built("t", "P", None));
        let (file, dropped) = kept_of(&one, &[retained("Other.total", "01000000")]);
        assert!(dropped.is_empty());
        assert_eq!(
            file.programs[0].values,
            [retained("Other.total", "01000000")]
        );
        // Several: by the part before the first '.', without regard to case; the rest kept.
        let two = parsed(pair([None, None]));
        let (file, dropped) = kept_of(
            &two,
            &[
                retained("a.s.x", "01000000"),
                retained(" B .y", "02000000"),
                retained("C.z", "03000000"),
                retained("nodot", "04000000"),
                master("S"),
            ],
        );
        assert_eq!(file.programs.len(), 2);
        assert_eq!(file.programs[0].name, "A");
        assert_eq!(file.programs[0].values, [retained("s.x", "01000000")]);
        assert_eq!(file.programs[1].name, "B");
        assert_eq!(file.programs[1].values, [retained("y", "02000000")]);
        assert_eq!(file.data, [master("S")]);
        assert_eq!(
            dropped,
            [
                "a RETAIN value runtime 't' gave names no program of it: C.z",
                "a RETAIN value runtime 't' gave names no program of it: nodot"
            ]
        );
        // A program with no value has no entry.
        let (file, _) = kept_of(&two, &[retained("B.y", "02000000")]);
        assert_eq!(file.programs.len(), 1);
        assert_eq!(file.programs[0].name, "B");
    }

    #[test]
    fn a_value_kept_for_one_program_is_given_only_to_it() {
        let total = || vec![retained("total", "2a000000")];
        let gone = |id: &str, of: &str| NotCarried {
            variable: format!("{of}.total"),
            why: format!("kept for program '{of}', which runtime '{id}' does not run now"),
        };

        // a) One program, the same program: given bare, nothing to write again.
        let tank = parsed(built("t", "Tank", None));
        assert_eq!(
            given_of(&tank, &kept(vec![("Tank", None, total())])),
            (total(), Vec::new(), false)
        );
        let t1 = parsed(built("t", "Tank", Some("T1")));
        assert_eq!(
            given_of(&t1, &kept(vec![("Tank", Some("t1"), total())])),
            (total(), Vec::new(), false)
        );
        // b) Another program, by name, and by project name with equal names: nothing given,
        // said, and the file written again.
        assert_eq!(
            given_of(&tank, &kept(vec![("Counter", None, total())])),
            (Vec::new(), vec![gone("t", "Counter")], true)
        );
        assert_eq!(
            given_of(&t1, &kept(vec![("Tank", Some("T2"), total())])),
            (Vec::new(), vec![gone("t", "T2")], true)
        );
        // c) Kept by a build that did not name its project program, given to one that does,
        // of the same PROGRAM name.
        assert_eq!(
            given_of(&t1, &kept(vec![("Tank", None, total())])),
            (total(), Vec::new(), false)
        );
        // d) One to several: renamed for the program; several to one: bare; A,B to A,C: A's.
        let ab = parsed(pair([None, None]));
        assert_eq!(
            given_of(&ab, &kept(vec![("A", None, total())])),
            (vec![retained("A.total", "2a000000")], Vec::new(), false)
        );
        let a = parsed(built("t", "A", None));
        assert_eq!(
            given_of(
                &a,
                &kept(vec![
                    ("A", None, total()),
                    ("B", None, vec![retained("count", "01000000")])
                ])
            ),
            (
                total(),
                vec![NotCarried {
                    variable: "B.count".into(),
                    why: "kept for program 'B', which runtime 't' does not run now".into()
                }],
                true
            )
        );
        let mut ac = several(&["A", "C"], json!([cyclic("f", 20, &["A", "C"])]));
        ac["id"] = json!("t");
        let ac = parsed(ac);
        assert_eq!(
            given_of(&ac, &kept(vec![("A", None, total()), ("B", None, total())])),
            (
                vec![retained("A.total", "2a000000")],
                vec![gone("t", "B")],
                true
            )
        );
        // e) An entry two programs could be: given to neither.
        let same = parsed(pair([Some("X"), Some("Y")]));
        let mut both = same.clone();
        both.programs[1].name = "A".into();
        assert_eq!(
            given_of(&both, &kept(vec![("A", None, total())])),
            (
                Vec::new(),
                vec![NotCarried {
                    variable: "A.total".into(),
                    why: "it could be of 'A' or 'A'".into()
                }],
                true
            )
        );
        // f) Data blocks' masters are given whatever the programs.
        let mut with_data = kept(vec![("Counter", None, total())]);
        if let Kept::Programs(file) = &mut with_data {
            file.data.push(master("S"));
        }
        let (given, not_given, rewrite) = given_of(&tank, &with_data);
        assert_eq!(given, [master("S")]);
        assert_eq!(not_given, [gone("t", "Counter")]);
        assert!(rewrite);
        let (given, _, _) = given_of(&ab, &with_data);
        assert_eq!(given, [master("S")]);
    }

    /// Review finding 2 of C8c-core: one entry matched the program by its project name, another
    /// (kept by a build that did not name its project program) by the PROGRAM name, and both
    /// were given under one path, the later winning whichever program kept it. Now the program
    /// is given neither, each value is said, and the file is written again.
    #[test]
    fn two_entries_kept_apart_are_never_both_given_to_one_program() {
        let t1 = parsed(built("t", "Tank", Some("T1")));
        let both = kept(vec![
            ("Other", Some("T1"), vec![retained("total", "01000000")]),
            ("Tank", None, vec![retained("total", "63000000")]),
        ]);
        let why = "program 'Tank' could be any of 'T1' or 'Tank', each kept apart: it is given none of them";
        assert_eq!(
            given_of(&t1, &both),
            (
                Vec::new(),
                vec![
                    NotCarried {
                        variable: "T1.total".into(),
                        why: why.into()
                    },
                    NotCarried {
                        variable: "Tank.total".into(),
                        why: why.into()
                    }
                ],
                true
            )
        );
        // In a runtime of several the other program is given its own as before.
        let mut ab = pair([Some("T1"), None]);
        ab["programs"][0]["name"] = json!("Tank");
        ab["tasks"] = json!([cyclic("f", 20, &["Tank", "B"])]);
        let ab = parsed(ab);
        let (given, not_given, rewrite) = given_of(
            &ab,
            &kept(vec![
                ("Other", Some("T1"), vec![retained("total", "01000000")]),
                ("Tank", None, vec![retained("total", "63000000")]),
                ("B", None, vec![retained("count", "02000000")]),
            ]),
        );
        assert_eq!(given, [retained("B.count", "02000000")]);
        assert_eq!(not_given.len(), 2, "{not_given:?}");
        assert!(rewrite);
    }

    /// Review finding 1 of C8c-core: a program deleted in the logic module while the core ran
    /// it, and made again under its name, had the same identity, and was given the RETAIN
    /// values kept for the one before. The logic module now marks each making (`created`), and
    /// a value kept for another making is not given, and said.
    #[test]
    fn a_program_made_again_under_its_name_is_given_none_of_the_values_kept_for_the_one_before() {
        let made = |created: Option<&str>| {
            let mut i = built("t", "T", Some("T"));
            if let Some(c) = created {
                i["program"]["created"] = json!(c);
            }
            parsed(i)
        };
        let kept_by = |created: Option<&str>| {
            Kept::Programs(KeptFile {
                programs: vec![KeptProgram {
                    name: "T".into(),
                    built_from: Some("T".into()),
                    created: created.map(str::to_string),
                    values: vec![retained("total", "65000000")],
                }],
                data: Vec::new(),
            })
        };
        let gone = NotCarried {
            variable: "T.total".into(),
            why: "kept for program 'T' as it was made before: runtime 't' runs a program made again under that name, which is another program".into(),
        };
        for (now, before) in [(Some("b2"), Some("a1")), (Some("b2"), None)] {
            assert_eq!(
                given_of(&made(now), &kept_by(before)),
                (Vec::new(), vec![gone.clone()], true),
                "{now:?} after {before:?}"
            );
        }
        // The same making: given.
        assert_eq!(
            given_of(&made(Some("a1")), &kept_by(Some("a1"))),
            (vec![retained("total", "65000000")], Vec::new(), false)
        );
        // The mark is kept with the values, and written only when the build gave one.
        let (file, _) = kept_of(&made(Some("a1")), &[retained("total", "65000000")]);
        assert_eq!(file.programs[0].created.as_deref(), Some("a1"));
        assert!(serde_json::to_string(&file)
            .unwrap()
            .contains(r#""built_from":"T","created":"a1","values""#));
        let (file, _) = kept_of(&made(None), &[retained("total", "65000000")]);
        assert!(!serde_json::to_string(&file).unwrap().contains("created"));
    }

    #[test]
    fn a_retain_file_of_today_is_given_unchanged_and_written_again_in_the_new_form() {
        let legacy = Kept::parse(r#"[{"path":"total","type":"DINT","bytes":"2a000000"}]"#).unwrap();
        assert_eq!(legacy, Kept::Legacy(vec![retained("total", "2a000000")]));
        // Whatever the runtime runs now, as before this slice; and the first keep writes.
        for i in [
            parsed(built("t", "Other", None)),
            parsed(pair([Some("A1"), Some("B1")])),
        ] {
            assert_eq!(
                given_of(&i, &legacy),
                (vec![retained("total", "2a000000")], Vec::new(), true)
            );
        }
        assert_eq!(
            given_of(&parsed(built("t", "P", None)), &Kept::Nothing),
            (Vec::new(), Vec::new(), false)
        );
    }

    #[test]
    fn a_retain_file_of_an_unknown_form_gives_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.json");
        assert_eq!(RetainStore::read(&path), Kept::Nothing, "no file");
        for text in [
            r#"{"programs": [], "x": 1}"#,
            "{}",
            r#""text""#,
            "[1, 2]",
            r#"{"programs": [{"name": "P", "values": [], "extra": true}]}"#,
            "not json",
        ] {
            std::fs::write(&path, text).unwrap();
            assert_eq!(RetainStore::read(&path), Kept::Nothing, "{text}");
            assert!(Kept::parse(text).is_err(), "{text}");
        }
        std::fs::write(&path, r#"{"programs": []}"#).unwrap();
        assert_eq!(
            RetainStore::read(&path),
            Kept::Programs(KeptFile {
                programs: Vec::new(),
                data: Vec::new()
            })
        );
    }
}
