//! The MythOS logic runtime for a PC (ADR-013 §2.4): a runtime module that runs a program
//! compiled to logic VM bytecode.
//!
//! One process runs one program. It scans on its own clock, in its own thread, whatever the
//! server is doing; the server exchanges with it (`exchange`): the tags the program reads go
//! in, what it publishes and what it wants written come out.
//!
//! What it promises:
//!
//! - **A scan sees one set of inputs.** Inputs are taken at the start of a scan from the last
//!   exchange and do not change during it.
//! - **No scan before the first inputs.** After RUN nothing is scanned until one exchange has
//!   delivered the tags, and no output or published value is reported until one scan has
//!   completed: nothing computed from initial values alone reaches the plant.
//! - **A tag that is not GOOD does not feed the program a value.** Its variable keeps what it
//!   last took; a binding `of: good` gives the program a BOOL to act on.
//! - **A scan that faults stops the program** (FAULT, with the reason): division by zero, a
//!   loop that never ends (the instruction budget), anything the VM refuses. RUN after that
//!   is a cold start.
//! - **Stopped means it says nothing.** In STOP or FAULT every published tag is a fault with
//!   the reason and no output is reported; the server then writes the outputs' safe values.

use async_trait::async_trait;
use mythos_logic_vm::{cold_start, read, scan, write, ElemType, Fault, Program, Value as VmValue};
use mythos_module::protocol::{
    ExchangeReply, InputOf, ModuleKind, ModuleManifest, RuntimeMode, RuntimeProgram, RuntimeState,
    WireInput, WireOutput, WireSample, PROTOCOL_VERSION,
};
use mythos_module::RuntimeEngine;
use mythos_plugin_sdk::{
    MythosPlugin, PluginContext, PluginError, PluginHealthReport, PluginMetadata, PluginType,
    ProtocolDriver, SampleFault, SampleValue, TagDeclaration, TagSample,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const MODULE_ID: &str = "org.mythos.runtime.vm";
/// The name a runtime instance's `module` field uses.
pub const RUNTIME_NAME: &str = "vm";
/// Instructions one scan may execute. A scan that needs more is taken for a loop that never
/// ends and stops the program.
const BUDGET: u32 = 5_000_000;

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        protocol: PROTOCOL_VERSION,
        id: MODULE_ID.to_string(),
        name: "Logic runtime (VM)".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: ModuleKind::Runtime,
        provides: RUNTIME_NAME.to_string(),
        description: "Runs IEC 61131-3 programs compiled to MythOS logic VM bytecode.".to_string(),
        capabilities: Vec::new(),
        config_schema: Value::Null,
        routes: Vec::new(),
        always_on: false,
        profile: json!({
            "name": "pc",
            "format_version": mythos_logic_vm::FORMAT_VERSION,
            "max_memory": 64 * 1024 * 1024,
            "max_code": 64 * 1024 * 1024
        }),
    }
}

/// Entry point of the `mythos-module-runtime-vm` executable.
pub fn main() -> ExitCode {
    mythos_module::run_runtime_module(manifest(), VmEngine::default)
}

// -----------------------------------------------------------------------------
// What a program is started with
// -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct StartConfig {
    program: RuntimeProgram,
    /// Whether to put it in RUN at once. Otherwise it is loaded and waits in STOP.
    #[serde(default)]
    run: bool,
}

/// One entry of the compiler's symbol table; only what binding needs.
#[derive(Deserialize)]
struct SymbolEntry {
    path: String,
    #[serde(rename = "type")]
    ty: String,
    offset: u32,
}

#[derive(Clone, Copy)]
struct Var {
    offset: u32,
    ty: ElemType,
}

struct Plan {
    program: RuntimeProgram,
    bytes: Vec<u8>,
    memory_size: usize,
    inputs: Vec<(Var, String, InputOf)>,
    outputs: Vec<(Var, String)>,
    published: Vec<(Var, String, bool)>,
}

fn invalid(message: impl Into<String>) -> PluginError {
    PluginError::InvalidConfiguration(message.into())
}

fn elem_of(name: &str) -> Option<ElemType> {
    Some(match name {
        "BOOL" => ElemType::Bool,
        "SINT" => ElemType::Sint,
        "INT" => ElemType::Int,
        "DINT" => ElemType::Dint,
        "LINT" => ElemType::Lint,
        "REAL" => ElemType::Real,
        "LREAL" => ElemType::Lreal,
        _ => return None,
    })
}

/// Check a configuration and work out where everything is. Starts nothing.
fn plan(config: &Value) -> Result<(Plan, bool), PluginError> {
    let StartConfig { program, run } = serde_json::from_value(config.clone())
        .map_err(|e| invalid(format!("not a runtime program: {e}")))?;
    if program.interval_ms == 0 {
        return Err(invalid("interval_ms must be at least 1"));
    }
    let bytes = hex::decode(&program.code)
        .map_err(|e| invalid(format!("the program's code is not hex: {e}")))?;
    let memory_size = Program::load(&bytes)
        .map_err(|e| invalid(format!("the program does not load: {e:?}")))?
        .memory_size();

    let entries: Vec<SymbolEntry> = serde_json::from_value(program.symbols.clone())
        .map_err(|e| invalid(format!("the program's symbols cannot be read: {e}")))?;
    let symbols: HashMap<String, &SymbolEntry> = entries
        .iter()
        .map(|s| (s.path.to_ascii_lowercase(), s))
        .collect();
    let resolve = |variable: &str| -> Result<Var, PluginError> {
        let entry = symbols
            .get(&variable.to_ascii_lowercase())
            .ok_or_else(|| invalid(format!("the program has no variable {variable}")))?;
        let ty = elem_of(&entry.ty).ok_or_else(|| {
            invalid(format!(
                "{variable} is a {}; only BOOL, integer and real variables can be bound to a tag",
                entry.ty
            ))
        })?;
        if entry.offset as usize + ty.size() as usize > memory_size {
            return Err(invalid(format!(
                "{variable} lies outside the program's memory"
            )));
        }
        Ok(Var {
            offset: entry.offset,
            ty,
        })
    };

    let mut inputs = Vec::new();
    for b in &program.inputs {
        let var = resolve(&b.variable)?;
        if b.of == InputOf::Good && var.ty != ElemType::Bool {
            return Err(invalid(format!(
                "{} takes whether {} is GOOD, so it must be a BOOL",
                b.variable, b.tag
            )));
        }
        inputs.push((var, b.tag.clone(), b.of));
    }
    let mut seen = HashSet::new();
    let mut outputs = Vec::new();
    for b in &program.outputs {
        if !seen.insert(b.tag.as_str()) {
            return Err(invalid(format!(
                "two variables are bound to the output {}",
                b.tag
            )));
        }
        outputs.push((resolve(&b.variable)?, b.tag.clone()));
    }
    let mut seen = HashSet::new();
    let mut published = Vec::new();
    for b in &program.published {
        if !seen.insert(b.tag.as_str()) {
            return Err(invalid(format!("two variables are published as {}", b.tag)));
        }
        published.push((resolve(&b.variable)?, b.tag.clone(), b.writable));
    }
    Ok((
        Plan {
            program,
            bytes,
            memory_size,
            inputs,
            outputs,
            published,
        },
        run,
    ))
}

fn declarations(plan: &Plan) -> Vec<TagDeclaration> {
    plan.program
        .published
        .iter()
        .map(|b| TagDeclaration {
            uri: b.tag.clone(),
            description: format!("variable {} of program {}", b.variable, plan.program.name),
            writable: b.writable,
        })
        .collect()
}

// -----------------------------------------------------------------------------
// Values in and out of the VM's memory
// -----------------------------------------------------------------------------

/// Round to nearest, a half to the even integer, as the VM's own conversions do.
fn round_half_even(x: f64) -> f64 {
    let floor = x.floor();
    let diff = x - floor;
    if diff > 0.5 || (diff == 0.5 && floor % 2.0 != 0.0) {
        floor + 1.0
    } else {
        floor
    }
}

/// A tag's value as a variable of type `ty`, or `None` when it is not one: the wrong kind of
/// value, or a number outside the type's range. Never wrapped or clamped.
fn to_vm(ty: ElemType, value: &Value) -> Option<VmValue> {
    let integer = |lo: i64, hi: i64| -> Option<i64> {
        let n = match value {
            Value::Number(n) => match n.as_i64() {
                Some(i) => i,
                None => {
                    let f = round_half_even(n.as_f64()?);
                    if !f.is_finite() || f < i64::MIN as f64 || f > i64::MAX as f64 {
                        return None;
                    }
                    f as i64
                }
            },
            _ => return None,
        };
        (lo..=hi).contains(&n).then_some(n)
    };
    Some(match ty {
        ElemType::Bool => match value {
            Value::Bool(b) => VmValue::Bool(*b),
            Value::Number(n) if n.as_i64() == Some(0) => VmValue::Bool(false),
            Value::Number(n) if n.as_i64() == Some(1) => VmValue::Bool(true),
            _ => return None,
        },
        ElemType::Sint => VmValue::Sint(integer(i8::MIN as i64, i8::MAX as i64)? as i8),
        ElemType::Int => VmValue::Int(integer(i16::MIN as i64, i16::MAX as i64)? as i16),
        ElemType::Dint => VmValue::Dint(integer(i32::MIN as i64, i32::MAX as i64)? as i32),
        ElemType::Lint => VmValue::Lint(integer(i64::MIN, i64::MAX)?),
        ElemType::Real => VmValue::Real(value.as_f64()? as f32),
        ElemType::Lreal => VmValue::Lreal(value.as_f64()?),
        ElemType::Time => return None,
    })
}

fn to_json(value: VmValue) -> Value {
    match value {
        VmValue::Bool(b) => json!(b),
        VmValue::Sint(v) => json!(v),
        VmValue::Int(v) => json!(v),
        VmValue::Dint(v) => json!(v),
        VmValue::Lint(v) | VmValue::Time(v) => json!(v),
        VmValue::Real(v) => json!(v),
        VmValue::Lreal(v) => json!(v),
    }
}

fn to_sample(value: VmValue) -> SampleValue {
    match value {
        VmValue::Bool(b) => SampleValue::Bool(b),
        VmValue::Sint(v) => SampleValue::I16(v as i16),
        VmValue::Int(v) => SampleValue::I16(v),
        VmValue::Dint(v) => SampleValue::I32(v),
        VmValue::Lint(v) | VmValue::Time(v) => SampleValue::I64(v),
        VmValue::Real(v) => SampleValue::F32(v),
        VmValue::Lreal(v) => SampleValue::F64(v),
    }
}

fn describe(fault: Fault) -> String {
    match fault {
        Fault::WrongSizes => "the memory or stack does not match the program".to_string(),
        Fault::BudgetExceeded => format!(
            "it executed more than {BUDGET} instructions: most likely a loop that never ends"
        ),
        Fault::DivideByZero(at) => format!("division by zero (code offset {at})"),
        Fault::StackOverflow(at) => format!("stack overflow (code offset {at})"),
        Fault::StackUnderflow(at) => format!("stack underflow (code offset {at})"),
        Fault::BadCode(at) => format!("not an instruction (code offset {at})"),
    }
}

// -----------------------------------------------------------------------------
// The running program
// -----------------------------------------------------------------------------

struct Live {
    memory: Vec<u8>,
    mode: RuntimeMode,
    fault: Option<String>,
    scans: u64,
    last_scan_us: u64,
    max_scan_us: u64,
    overruns: u64,
    /// The tags of the last exchange, by URI.
    inputs: HashMap<String, (Option<Value>, bool)>,
    /// Whether an exchange has delivered the tags since the program was put in RUN.
    fed: bool,
    /// When the program was put in RUN: the zero of its clock.
    since: Instant,
}

impl Live {
    fn state(&self, plan: &Plan) -> RuntimeState {
        RuntimeState {
            mode: self.mode,
            fault: self.fault.clone(),
            program: plan.program.name.clone(),
            version: plan.program.version.clone(),
            scans: self.scans,
            last_scan_us: self.last_scan_us,
            max_scan_us: self.max_scan_us,
            overruns: self.overruns,
        }
    }

    /// From its initial values, clock at zero, waiting for its first inputs.
    fn cold_run(&mut self, plan: &Plan) -> Result<(), PluginError> {
        let program = Program::load(&plan.bytes)
            .map_err(|e| PluginError::RuntimeError(format!("the program does not load: {e:?}")))?;
        cold_start(&program, &mut self.memory)
            .map_err(|e| PluginError::RuntimeError(describe(e)))?;
        self.mode = RuntimeMode::Run;
        self.fault = None;
        self.scans = 0;
        self.last_scan_us = 0;
        self.max_scan_us = 0;
        self.overruns = 0;
        self.fed = false;
        self.since = Instant::now();
        Ok(())
    }

    /// Why nothing is reported, when nothing is.
    fn silent_because(&self) -> Option<String> {
        match self.mode {
            RuntimeMode::Stop => Some("the program is stopped".to_string()),
            RuntimeMode::Fault => Some(format!(
                "the program faulted: {}",
                self.fault.as_deref().unwrap_or("no reason recorded")
            )),
            RuntimeMode::Run if self.scans == 0 => {
                Some("the program has not completed a scan yet".to_string())
            }
            RuntimeMode::Run => None,
        }
    }
}

struct Running {
    plan: Arc<Plan>,
    live: Arc<Mutex<Live>>,
    quit: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn lock(live: &Mutex<Live>) -> std::sync::MutexGuard<'_, Live> {
    live.lock().unwrap_or_else(|e| e.into_inner())
}

/// The scan thread: one scan every interval while the program is in RUN and has been fed.
fn scan_loop(plan: Arc<Plan>, live: Arc<Mutex<Live>>, quit: Arc<AtomicBool>) {
    let Ok(program) = Program::load(&plan.bytes) else {
        let mut l = lock(&live);
        l.mode = RuntimeMode::Fault;
        l.fault = Some("the program does not load".to_string());
        return;
    };
    let mut stack = vec![0u64; program.stack_depth().max(1)];
    let interval = Duration::from_millis(plan.program.interval_ms);
    let mut next = Instant::now();
    while !quit.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now < next {
            // Short sleeps, so the thread ends promptly when asked to.
            std::thread::sleep((next - now).min(Duration::from_millis(20)));
            continue;
        }
        let late = now.duration_since(next) >= interval;
        next = if late {
            now + interval
        } else {
            next + interval
        };

        let mut guard = lock(&live);
        let l = &mut *guard;
        if l.mode != RuntimeMode::Run || !l.fed {
            continue;
        }
        if late {
            l.overruns += 1;
        }
        // The inputs of this scan: what the last exchange delivered.
        for (var, tag, of) in &plan.inputs {
            let Some((value, good)) = l.inputs.get(tag) else {
                continue;
            };
            match of {
                InputOf::Good => {
                    write(&mut l.memory, var.offset, VmValue::Bool(*good));
                }
                InputOf::Value => {
                    if let (true, Some(v)) = (*good, value.as_ref().and_then(|v| to_vm(var.ty, v)))
                    {
                        write(&mut l.memory, var.offset, v);
                    }
                }
            }
        }
        let started = Instant::now();
        let now_ns = l.since.elapsed().as_nanos().min(i64::MAX as u128) as i64;
        match scan(&program, &mut l.memory, &mut stack, now_ns, BUDGET) {
            Ok(_) => {
                let us = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
                l.scans += 1;
                l.last_scan_us = us;
                l.max_scan_us = l.max_scan_us.max(us);
            }
            Err(fault) => {
                l.mode = RuntimeMode::Fault;
                l.fault = Some(format!("scan {} stopped: {}", l.scans + 1, describe(fault)));
                tracing::error!("{}", l.fault.as_deref().unwrap_or_default());
            }
        }
    }
}

/// The engine of the runtime module.
#[derive(Default)]
pub struct VmEngine {
    running: Option<Running>,
}

impl VmEngine {
    fn running(&self) -> Result<&Running, PluginError> {
        self.running
            .as_ref()
            .ok_or_else(|| PluginError::RuntimeError("no program is loaded".to_string()))
    }

    fn halt(&mut self) {
        if let Some(mut running) = self.running.take() {
            running.quit.store(true, Ordering::SeqCst);
            if let Some(thread) = running.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for VmEngine {
    fn drop(&mut self) {
        self.halt();
    }
}

#[async_trait]
impl MythosPlugin for VmEngine {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: MODULE_ID.to_string(),
            name: "MythOS logic runtime (VM)".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: "MythOS Team <adrian@firmansyah.co>".to_string(),
            description: "Runs programs compiled to logic VM bytecode".to_string(),
            plugin_type: PluginType::ProcessEngine,
            capabilities: Vec::new(),
        }
    }

    async fn init(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        Ok(())
    }

    /// Load the program, and put it in RUN if the configuration says so.
    async fn start(&mut self, _ctx: &PluginContext, config: &Value) -> Result<(), PluginError> {
        self.halt();
        let (plan, run) = plan(config)?;
        let mut live = Live {
            memory: vec![0u8; plan.memory_size],
            mode: RuntimeMode::Stop,
            fault: None,
            scans: 0,
            last_scan_us: 0,
            max_scan_us: 0,
            overruns: 0,
            inputs: HashMap::new(),
            fed: false,
            since: Instant::now(),
        };
        if run {
            live.cold_run(&plan)?;
        }
        let plan = Arc::new(plan);
        let live = Arc::new(Mutex::new(live));
        let quit = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("logic-scan".to_string())
            .spawn({
                let (plan, live, quit) = (Arc::clone(&plan), Arc::clone(&live), Arc::clone(&quit));
                move || scan_loop(plan, live, quit)
            })
            .map_err(|e| PluginError::StartFailed(format!("the scan thread did not start: {e}")))?;
        self.running = Some(Running {
            plan,
            live,
            quit,
            thread: Some(thread),
        });
        Ok(())
    }

    async fn stop(&mut self, _ctx: &PluginContext) -> Result<(), PluginError> {
        self.halt();
        Ok(())
    }

    async fn health_check(&self) -> Result<PluginHealthReport, PluginError> {
        Ok(PluginHealthReport::healthy(match &self.running {
            Some(r) => format!("{:?}", lock(&r.live).mode),
            None => "no program is loaded".to_string(),
        }))
    }
}

#[async_trait]
impl ProtocolDriver for VmEngine {
    async fn describe_config(&self, config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        Ok(declarations(&plan(config)?.0))
    }

    fn declared_tags(&self) -> Vec<TagDeclaration> {
        self.running
            .as_ref()
            .map(|r| declarations(&r.plan))
            .unwrap_or_default()
    }

    async fn poll_cycle(&mut self, _ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        Err(PluginError::RuntimeError(
            "a runtime module is exchanged with, not polled".to_string(),
        ))
    }

    /// An operator writes a setpoint: a published variable marked writable. It takes effect
    /// at the next scan.
    async fn write_tag(
        &mut self,
        _ctx: &PluginContext,
        tag_uri: &str,
        value: &Value,
    ) -> Result<(), PluginError> {
        let running = self.running()?;
        let (var, _, writable) = running
            .plan
            .published
            .iter()
            .find(|(_, tag, _)| tag == tag_uri)
            .ok_or_else(|| {
                PluginError::InvalidTag(format!("the program publishes no tag {tag_uri}"))
            })?;
        if !writable {
            return Err(PluginError::InvalidTag(format!(
                "{tag_uri} is not a setpoint: its variable is published read-only"
            )));
        }
        let v = to_vm(var.ty, value).ok_or_else(|| {
            invalid(format!(
                "{value} does not fit the variable behind {tag_uri}"
            ))
        })?;
        let mut l = lock(&running.live);
        if l.mode != RuntimeMode::Run {
            return Err(PluginError::InvalidTag(
                "the program is not running, so the setpoint was not taken".to_string(),
            ));
        }
        write(&mut l.memory, var.offset, v);
        Ok(())
    }
}

#[async_trait]
impl RuntimeEngine for VmEngine {
    async fn exchange(
        &mut self,
        _ctx: &PluginContext,
        inputs: Vec<WireInput>,
    ) -> Result<ExchangeReply, PluginError> {
        let running = self.running()?;
        let plan = &running.plan;
        let mut l = lock(&running.live);
        for input in inputs {
            l.inputs.insert(input.tag, (input.value, input.good));
        }
        l.fed = true;

        let silent = l.silent_because();
        let samples = plan
            .published
            .iter()
            .map(
                |(var, tag, _)| match (&silent, read(&l.memory, var.offset, var.ty)) {
                    (None, Some(v)) => TagSample::value(tag.clone(), to_sample(v)),
                    (Some(why), _) => {
                        TagSample::fault(tag.clone(), SampleFault::DeviceFailure(why.clone()))
                    }
                    (None, None) => TagSample::fault(
                        tag.clone(),
                        SampleFault::ConfigError(
                            "the variable lies outside the program's memory".into(),
                        ),
                    ),
                },
            )
            .map(|s| WireSample::from(&s))
            .collect();
        let outputs = match silent {
            Some(_) => Vec::new(),
            None => plan
                .outputs
                .iter()
                .filter_map(|(var, tag)| {
                    read(&l.memory, var.offset, var.ty).map(|v| WireOutput {
                        tag: tag.clone(),
                        value: to_json(v),
                    })
                })
                .collect(),
        };
        Ok(ExchangeReply {
            state: l.state(plan),
            samples,
            outputs,
        })
    }

    async fn set_mode(
        &mut self,
        _ctx: &PluginContext,
        run: bool,
    ) -> Result<RuntimeState, PluginError> {
        let running = self.running()?;
        let mut l = lock(&running.live);
        match (run, l.mode) {
            (true, RuntimeMode::Run) => {}
            (true, _) => l.cold_run(&running.plan)?,
            (false, _) => {
                l.mode = RuntimeMode::Stop;
                l.fault = None;
            }
        }
        Ok(l.state(&running.plan))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_value_becomes_a_variable_only_when_it_is_one_never_wrapped_or_clamped() {
        assert_eq!(to_vm(ElemType::Int, &json!(42)), Some(VmValue::Int(42)));
        assert_eq!(to_vm(ElemType::Int, &json!(40000)), None, "outside INT");
        assert_eq!(
            to_vm(ElemType::Int, &json!(2.5)),
            Some(VmValue::Int(2)),
            "a half to even"
        );
        assert_eq!(to_vm(ElemType::Int, &json!(3.5)), Some(VmValue::Int(4)));
        assert_eq!(to_vm(ElemType::Int, &json!("42")), None);
        assert_eq!(to_vm(ElemType::Int, &json!(true)), None);
        assert_eq!(to_vm(ElemType::Bool, &json!(1)), Some(VmValue::Bool(true)));
        assert_eq!(to_vm(ElemType::Bool, &json!(2)), None);
        assert_eq!(
            to_vm(ElemType::Real, &json!(21.5)),
            Some(VmValue::Real(21.5))
        );
        assert_eq!(to_vm(ElemType::Sint, &json!(-129)), None);
        assert_eq!(to_vm(ElemType::Time, &json!(1)), None);
    }

    #[test]
    fn the_manifest_says_it_is_a_runtime_and_what_it_can_run() {
        let m = manifest();
        assert_eq!(m.kind, ModuleKind::Runtime);
        assert_eq!(m.provides, "vm");
        assert_eq!(
            m.profile["format_version"],
            json!(mythos_logic_vm::FORMAT_VERSION)
        );
    }
}
