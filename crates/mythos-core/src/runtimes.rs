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
use crate::tag::TagCatalog;
use mythos_module::protocol::{
    Call, ExchangeReply, RuntimeMode, RuntimeProgram, RuntimeState, WireInput,
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
use std::path::Path;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

/// The project file that says which runtimes run, beside `io.json`.
pub const RUNTIMES_FILE: &str = "runtimes.json";
/// A runtime instance `x` is the connection `runtime:x` of the field I/O runtime, and its
/// driver is `runtime:<module>`.
pub const CONNECTION_PREFIX: &str = "runtime:";
/// How long a runtime module may take to answer one exchange. Past it the program is taken to
/// be not running and its outputs are made safe. Well inside the connection's cycle deadline,
/// so that making them safe happens in this cycle and is not cancelled with it.
const EXCHANGE_DEADLINE: Duration = Duration::from_millis(1_000);
/// The connection's cycle deadline: one exchange and the writes it leads to.
const CYCLE_TIMEOUT_MS: u64 = 10_000;

pub fn is_runtime_connection(id: &str) -> bool {
    id.starts_with(CONNECTION_PREFIX)
}

// -----------------------------------------------------------------------------
// Configuration
// -----------------------------------------------------------------------------

/// One program in one runtime module.
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
    pub program: RuntimeProgram,
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
        let mut outputs: HashMap<&str, &str> = HashMap::new();
        let mut published: HashMap<&str, &str> = HashMap::new();
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
            if i.program.interval_ms == 0 {
                return Err(format!(
                    "runtime '{}': interval_ms must be at least 1",
                    i.id
                ));
            }
            for o in &i.program.outputs {
                if o.tag.contains(':') {
                    return Err(format!(
                        "runtime '{}': {} is another device's tag; a runtime writes this node's field tags",
                        i.id, o.tag
                    ));
                }
                if !(o.safe.is_number() || o.safe.is_boolean()) {
                    return Err(format!(
                        "runtime '{}': the safe value of {} must be a number or a boolean",
                        i.id, o.tag
                    ));
                }
                if let Some(other) = outputs.insert(o.tag.as_str(), i.id.as_str()) {
                    return Err(format!(
                        "{} is an output of both '{}' and '{}': one tag has one writer",
                        o.tag, other, i.id
                    ));
                }
            }
            for p in &i.program.published {
                // `external.` is the field: only a driver's tags are named so (tag namespace
                // rules §2.1). What a program computes is `internal.`.
                if !p.tag.starts_with("internal.") {
                    return Err(format!(
                        "runtime '{}': a published tag must start with 'internal.', which {} does not",
                        i.id, p.tag
                    ));
                }
                if let Some(other) = published.insert(p.tag.as_str(), i.id.as_str()) {
                    return Err(format!(
                        "{} is published by both '{}' and '{}'",
                        p.tag, other, i.id
                    ));
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
                // How often it is exchanged with: as often as it scans, within bounds that
                // keep an operator's view fresh and the exchange cheap.
                poll_interval_ms: i.program.interval_ms.clamp(20, 200),
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
    pub program: String,
    pub version: String,
    pub interval_ms: u64,
    /// `run`, `stop`, `fault`, or `unreachable` when its module does not answer.
    pub mode: String,
    /// Why it is in `fault`, or why it is `unreachable`.
    pub reason: Option<String>,
    pub scans: u64,
    pub last_scan_us: u64,
    pub max_scan_us: u64,
    pub overruns: u64,
    /// Whether every output is confirmed at its safe value. Meaningful while not in `run`.
    pub outputs_safe: bool,
    pub outputs: Vec<OutputStatus>,
    pub pid: Option<u32>,
    pub restarts: u64,
}

#[derive(Default)]
struct StatesInner {
    /// By instance id, with the generation of the driver that reports it.
    statuses: BTreeMap<String, (u64, RuntimeStatus)>,
    /// What an engineer asked of an instance since it was loaded: run or stop.
    wanted: HashMap<String, bool>,
    /// Why the project's runtimes are not as configured, when they are not.
    problem: Option<String>,
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
        }
    }
}

/// What a runtime driver needs of the server.
#[derive(Clone)]
pub struct RuntimeEnv {
    pub tags: Arc<dyn TagSource>,
    pub io: IoStatusHandle,
    pub states: RuntimeStates,
}

// -----------------------------------------------------------------------------
// The driver
// -----------------------------------------------------------------------------

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A runtime instance, as a driver of the field I/O runtime.
pub struct RuntimeDriver {
    process: ProcessDriver,
    module: String,
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
}

impl RuntimeDriver {
    pub fn new(module: Arc<ModuleBinary>, env: RuntimeEnv) -> Self {
        Self {
            module: module.manifest.provides.clone(),
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
        for o in &instance.program.outputs {
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
        let outputs: Vec<OutputStatus> = instance
            .program
            .outputs
            .iter()
            .map(|o| OutputStatus {
                tag: o.tag.clone(),
                variable: o.variable.clone(),
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
                program: instance.program.name.clone(),
                version: instance.program.version.clone(),
                interval_ms: instance.program.interval_ms,
                mode: mode.to_string(),
                reason,
                scans: state.map_or(0, |s| s.scans),
                last_scan_us: state.map_or(0, |s| s.last_scan_us),
                max_scan_us: state.map_or(0, |s| s.max_scan_us),
                overruns: state.map_or(0, |s| s.overruns),
                outputs_safe: instance
                    .program
                    .outputs
                    .iter()
                    .all(|o| self.safe.contains(&o.tag)),
                outputs,
                pid: self.process.pid(),
                restarts: self.process.restarts(),
            },
        );
    }

    fn parse(config: &Value) -> Result<RuntimeInstance, PluginError> {
        serde_json::from_value(config.clone())
            .map_err(|e| PluginError::InvalidConfiguration(format!("not a runtime instance: {e}")))
    }
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
        self.process.start(ctx, config).await?;
        let outputs: Vec<String> = instance
            .program
            .outputs
            .iter()
            .map(|o| o.tag.clone())
            .collect();
        match self.env.io.claim(&Self::owner(&instance.id), &outputs) {
            Ok(claim) => self.claim = Some(claim),
            Err(why) => {
                let _ = self.process.stop(ctx).await;
                return Err(PluginError::StartFailed(why));
            }
        }
        let mut reads: Vec<String> = instance
            .program
            .inputs
            .iter()
            .map(|i| i.tag.clone())
            .collect();
        reads.sort();
        reads.dedup();
        self.reads = reads;
        self.instance = Some(instance);
        self.report("stop", Some("loaded; no exchange yet".to_string()));
        Ok(())
    }

    /// Unload. The outputs still ours go to their safe values first; then the claim is given
    /// up and the process stopped.
    async fn stop(&mut self, ctx: &PluginContext) -> Result<(), PluginError> {
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
        Self::parse(config)?;
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
        if let Some(run) = wanted {
            if run != (reply.state.mode == RuntimeMode::Run) {
                match self
                    .process
                    .call(Call::Mode { run }, EXCHANGE_DEADLINE)
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
                    Err(e) => tracing::warn!("{}: mode change not taken: {e}", Self::owner(&id)),
                }
            }
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
}
