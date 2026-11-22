//! The MythOS logic runtime for a PC (ADR-013 §2.4): a runtime module that runs a program
//! compiled to logic VM bytecode.
//!
//! One process is one runtime: one program, or several programs in tasks (ADR-013 §2.19). It
//! runs them on its own clock, in its own thread, whatever the server is doing; the server
//! exchanges with it (`exchange`): the tags the programs read go in, what they publish and what
//! they want written come out. A runtime given one program and no tasks runs it in one cyclic
//! task at its interval, and says exactly what it said before there were tasks.
//!
//! What it promises:
//!
//! - **A run sees one set of inputs.** The inputs of a task's programs are taken when the task
//!   begins a run, from the last exchange, and do not change during it.
//! - **An exchange never sees a program half-way.** What is reported is each program's image:
//!   its memory as its last completed run left it ([`tasks`]).
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

mod tasks;

use async_trait::async_trait;
use mythos_logic_vm::{
    read, read_string, string_size, write, ElemType, Fault, Program, Value as VmValue,
};
use mythos_module::protocol::{
    check_data, check_tasks, implied_task, CarryReport, DataBlock, ExchangeReply, InputOf,
    ModuleKind, ModuleManifest, NotCarried, ProgramVariables, RetainedValue, RuntimeMode,
    RuntimeProgram, RuntimeState, TaskConfig, WatchReply, WatchedValue, WireInput, WireOutput,
    WireSample, PROTOCOL_VERSION,
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
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tasks::{Control, Forced, Layout, Shared};

pub const MODULE_ID: &str = "org.mythos.runtime.vm";
/// The name a runtime instance's `module` field uses.
pub const RUNTIME_NAME: &str = "vm";
/// Instructions one run of a program may execute. A run that needs more is taken for a loop
/// that never ends and stops the runtime.
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
            "max_code": 64 * 1024 * 1024,
            // ADR-013 §2.19: it runs several programs in tasks of every kind.
            "tasks": {"kinds": ["cycle", "cyclic", "startup", "event"]},
            // ADR-014: it hands data blocks over between the programs of one runtime.
            "data": {"tiers": [1]}
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
    /// The one program of a runtime of today; or `programs`, never both.
    #[serde(default)]
    program: Option<RuntimeProgram>,
    #[serde(default)]
    programs: Vec<RuntimeProgram>,
    /// The tasks that run them; none for one program in its implied task.
    #[serde(default)]
    tasks: Vec<TaskConfig>,
    /// The data blocks its programs share (ADR-014); none for a runtime of today.
    #[serde(default)]
    data: Vec<DataBlock>,
    /// Whether to put it in RUN at once. Otherwise it is loaded and waits in STOP.
    #[serde(default)]
    run: bool,
    /// The values its RETAIN variables had when it last ran, as the host kept them: taken at
    /// its first start into RUN, a warm restart.
    #[serde(default)]
    retained: Vec<RetainedValue>,
    /// The variables of the program this one replaces, as it was running (an online change):
    /// taken after the initial and the RETAIN values, before the first scan.
    #[serde(default)]
    carried: Option<ProgramVariables>,
}

/// A checked configuration: what to load, and how to start it.
struct Loaded {
    layout: Layout,
    run: bool,
    retained: Vec<RetainedValue>,
    carried: Option<ProgramVariables>,
}

/// One entry of the compiler's symbol table; only what binding and watching need.
#[derive(Deserialize, Clone)]
struct SymbolEntry {
    path: String,
    #[serde(rename = "type")]
    ty: String,
    offset: u32,
    /// Its size in bytes; a whole array's for an array.
    #[serde(default)]
    size: u32,
    #[serde(default)]
    array: Option<ArrayEntry>,
    /// Whether it keeps its value across a warm restart (IEC 61131-3 §6.5.6).
    #[serde(default)]
    retain: bool,
    /// Inside a function block instance and not one of its inputs: only the block writes it.
    #[serde(default)]
    private: bool,
    /// How it is declared: `external` for a program's copy of a data block (ADR-014), or a
    /// part of one. Absent from symbols made before data blocks.
    #[serde(default)]
    section: Option<String>,
}

/// What an array symbol holds: enough to find one element.
#[derive(Deserialize, Clone)]
struct ArrayEntry {
    element_type: String,
    element_size: u32,
    /// Each subscript's range, inclusive.
    bounds: Vec<(i64, i64)>,
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
    /// Every symbol of the program, by its path in lower case: what can be watched.
    symbols: HashMap<String, SymbolEntry>,
    /// Its RETAIN variables, each as one span of memory: `(path, type, offset, size)`.
    retain: Vec<(String, String, u32, u32)>,
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

/// Why a variable of the type `name` cannot be bound, beyond its not being a number: the tag
/// bus carries numbers and BOOLs (`SampleValue`), and no text.
fn not_on_the_bus(name: &str) -> &'static str {
    if string_length(name).is_some() {
        " (a tag carries a number, never text)"
    } else {
        ""
    }
}

/// What a watched variable is in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// One value.
    Value(ElemType),
    /// A `STRING[n]`: `2 + n` bytes.
    Text(u16),
}

impl Shape {
    fn size(self) -> u32 {
        match self {
            Shape::Value(ty) => ty.size(),
            Shape::Text(n) => string_size(n),
        }
    }
}

/// The length of a `STRING[n]`, by the name the compiler gives its type.
fn string_length(name: &str) -> Option<u16> {
    name.strip_prefix("STRING[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// A type whose value can be read and shown: what can be bound, TIME and STRING.
fn watchable(name: &str) -> Option<Shape> {
    match name {
        "TIME" => Some(Shape::Value(ElemType::Time)),
        other => elem_of(other)
            .map(Shape::Value)
            .or_else(|| string_length(other).map(Shape::Text)),
    }
}

/// `name[2]` or `name[1, 3]` as the name and its subscripts; a plain name has none.
fn subscripts(variable: &str) -> Result<(&str, Vec<i64>), String> {
    let Some(open) = variable.find('[') else {
        return Ok((variable, Vec::new()));
    };
    let inner = variable[open + 1..]
        .strip_suffix(']')
        .ok_or_else(|| format!("{variable} is not a variable or an element of an array"))?;
    let indices = inner
        .split(',')
        .map(|part| part.trim().parse::<i64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("the subscripts of {variable} are not whole numbers"))?;
    Ok((&variable[..open], indices))
}

/// Where a watched variable is and what it is, or why it cannot be read.
fn locate(plan: &Plan, variable: &str) -> Result<(u32, Shape, String), String> {
    let (name, indices) = subscripts(variable)?;
    let entry = plan
        .symbols
        .get(&name.trim().to_ascii_lowercase())
        .ok_or_else(|| format!("the program has no variable {name}"))?;
    let (type_name, offset) = match (&entry.array, indices.is_empty()) {
        (None, true) => (entry.ty.clone(), u64::from(entry.offset)),
        (None, false) => return Err(format!("{name} is a {}, not an array", entry.ty)),
        (Some(_), true) => {
            return Err(format!(
                "{name} is an {}; name one element, as in {name}[…]",
                entry.ty
            ))
        }
        (Some(array), false) => {
            if indices.len() != array.bounds.len() {
                return Err(format!(
                    "{name} has {} subscripts, not {}",
                    array.bounds.len(),
                    indices.len()
                ));
            }
            // The rightmost subscript varies fastest.
            let mut at: u64 = 0;
            for (index, (lo, hi)) in indices.iter().zip(&array.bounds) {
                if index < lo || index > hi {
                    return Err(format!("subscript {index} of {name} is outside {lo}..{hi}"));
                }
                at = at * (hi - lo + 1) as u64 + (index - lo) as u64;
            }
            (
                array.element_type.clone(),
                u64::from(entry.offset) + at * u64::from(array.element_size),
            )
        }
    };
    let shape = watchable(&type_name).ok_or_else(|| {
        format!(
            "{variable} is a {type_name}; only a BOOL, a number, a TIME or a STRING has a value to show"
        )
    })?;
    if offset + u64::from(shape.size()) > plan.memory_size as u64 {
        return Err(format!("{variable} lies outside the program's memory"));
    }
    Ok((offset as u32, shape, type_name))
}

/// Check a configuration and work out where everything is. Starts nothing.
fn plan(config: &Value) -> Result<Loaded, PluginError> {
    let StartConfig {
        program,
        programs,
        tasks,
        data,
        run,
        retained,
        carried,
    } = serde_json::from_value(config.clone())
        .map_err(|e| invalid(format!("not a runtime program: {e}")))?;
    let programs = match (program, programs.is_empty()) {
        (Some(_), false) => return Err(invalid("give program or programs, not both")),
        (Some(program), true) => vec![program],
        (None, _) => programs,
    };
    check_tasks(&programs.iter().collect::<Vec<_>>(), &tasks).map_err(invalid)?;
    check_data(&programs.iter().collect::<Vec<_>>(), &data).map_err(invalid)?;
    let implied = tasks.is_empty();
    if implied && programs[0].interval_ms == 0 {
        return Err(invalid("interval_ms must be at least 1"));
    }
    let several = programs.len() > 1;
    let mut plans = Vec::new();
    for program in programs {
        let name = program.name.clone();
        plans.push(plan_program(program).map_err(|e| match e {
            PluginError::InvalidConfiguration(m) if several => {
                invalid(format!("program '{name}': {m}"))
            }
            other => other,
        })?);
    }
    // Every copy of a data block inside its program's memory, where its symbols put it: the
    // hand-over copies by these offsets, so none may lead outside a memory.
    for plan in &plans {
        check_copies(plan).map_err(invalid)?;
    }
    // Inside one runtime, as inside one program: one variable to a tag.
    let mut outputs = HashSet::new();
    let mut published = HashSet::new();
    for plan in &plans {
        for (_, tag) in &plan.outputs {
            if !outputs.insert(tag.as_str()) {
                return Err(invalid(format!(
                    "two variables are bound to the output {tag}"
                )));
            }
        }
        for (_, tag, _) in &plan.published {
            if !published.insert(tag.as_str()) {
                return Err(invalid(format!("two variables are published as {tag}")));
            }
        }
    }
    let tasks = if implied {
        vec![implied_task(&plans[0].program)]
    } else {
        tasks
    };
    Ok(Loaded {
        layout: Layout::new(plans, tasks, implied, data).map_err(invalid)?,
        run,
        retained,
        carried,
    })
}

/// Check that each data block a program declares lies inside its memory (R14), and that its
/// symbols put each member where its data says (R15): the member's own symbol, or the leaves
/// under it, starting at the member's offset and none outside it.
fn check_copies(plan: &Plan) -> Result<(), String> {
    let p = &plan.program.name;
    for u in &plan.program.data {
        let b = &u.name;
        if u64::from(u.offset) + u64::from(u.size) > plan.memory_size as u64 {
            return Err(format!(
                "program '{p}': its copy of data block '{b}' lies outside the program's memory"
            ));
        }
        let misplaced = || {
            format!("program '{p}': its symbols do not place data block '{b}' where its data says")
        };
        for m in &u.members {
            let at = u64::from(u.offset) + u64::from(m.offset);
            let end = at + u64::from(m.size);
            let own = format!("{b}.{}", m.name).to_ascii_lowercase();
            let under = |path: &str| {
                path == own
                    || path
                        .strip_prefix(own.as_str())
                        .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('['))
            };
            let leaves: Vec<&SymbolEntry> = plan
                .symbols
                .iter()
                .filter(|(path, _)| under(path))
                .map(|(_, s)| s)
                .collect();
            let first = leaves.iter().map(|s| u64::from(s.offset)).min();
            let inside = leaves.iter().all(|s| {
                u64::from(s.offset) >= at && u64::from(s.offset) + u64::from(s.size) <= end
            });
            if first != Some(at) || !inside {
                return Err(misplaced());
            }
        }
    }
    // The other way round: every part of a data block's copy the code has (a symbol declared
    // `external`) is of a block the program's data declares, and inside a member it lists. A
    // block left out of the data, or a member left out of a block, would never be handed over,
    // and the program would run on a copy nothing updates.
    let mut externals: Vec<&SymbolEntry> = plan
        .symbols
        .values()
        .filter(|s| s.section.as_deref() == Some("external"))
        .collect();
    externals.sort_by(|a, b| a.path.cmp(&b.path));
    for s in externals {
        let block = first_segment(&s.path);
        let Some(u) = plan
            .program
            .data
            .iter()
            .find(|u| u.name.eq_ignore_ascii_case(block))
        else {
            return Err(format!(
                "program '{p}': its code declares data block '{block}' ({}), and its data does not",
                s.path
            ));
        };
        let Some(rest) = s.path.get(block.len()..).filter(|r| !r.is_empty()) else {
            continue;
        };
        let member = rest
            .strip_prefix('.')
            .map(first_segment)
            .unwrap_or_default();
        if !u
            .members
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(member))
        {
            return Err(format!(
                "program '{p}': its code has {} in data block '{}', and its data lists no member '{member}' of it",
                s.path, u.name
            ));
        }
    }
    Ok(())
}

/// The first segment of a path: before its first `.` or `[`.
fn first_segment(path: &str) -> &str {
    &path[..path.find(['.', '[']).unwrap_or(path.len())]
}

/// Check one program and work out where its variables are.
fn plan_program(program: RuntimeProgram) -> Result<Plan, PluginError> {
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
                "{variable} is a {}; only BOOL, integer and real variables can be bound to a tag{}",
                entry.ty,
                not_on_the_bus(&entry.ty)
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

    // A tag written into a variable: never one only its function block writes.
    let writable = |variable: &str, what: &str| -> Result<(), PluginError> {
        match symbols.get(&variable.to_ascii_lowercase()) {
            Some(entry) if entry.private => Err(invalid(format!(
                "{variable} is inside a function block instance and is not one of its inputs, so only the block writes it; it cannot be {what}"
            ))),
            _ => Ok(()),
        }
    };
    let mut inputs = Vec::new();
    for b in &program.inputs {
        let var = resolve(&b.variable)?;
        writable(&b.variable, "read from a tag")?;
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
        let var = resolve(&b.variable)?;
        if b.writable {
            writable(&b.variable, "a setpoint")?;
        }
        published.push((var, b.tag.clone(), b.writable));
    }
    let mut retain = Vec::new();
    for s in entries.iter().filter(|s| s.retain) {
        if s.size == 0 || s.offset as usize + s.size as usize > memory_size {
            return Err(invalid(format!(
                "the RETAIN variable {} lies outside the program's memory",
                s.path
            )));
        }
        retain.push((s.path.clone(), s.ty.clone(), s.offset, s.size));
    }
    Ok(Plan {
        program,
        bytes,
        memory_size,
        inputs,
        outputs,
        published,
        symbols: entries
            .iter()
            .map(|s| (s.path.to_ascii_lowercase(), s.clone()))
            .collect(),
        retain,
    })
}

/// The values of the program's RETAIN variables in `memory`.
fn snapshot(plan: &Plan, memory: &[u8]) -> Vec<RetainedValue> {
    plan.retain
        .iter()
        .map(|(path, ty, offset, size)| RetainedValue {
            path: path.clone(),
            ty: ty.clone(),
            bytes: hex::encode(&memory[*offset as usize..(*offset + *size) as usize]),
            layout: None,
        })
        .collect()
}

/// Put retained values into the program's RETAIN variables: each only into a variable of the
/// same path, type and size, which a program built again from changed text may no longer have.
/// Returns the paths of the values not taken, and why.
fn restore(plan: &Plan, memory: &mut [u8], values: &[RetainedValue]) -> Vec<String> {
    let mut refused = Vec::new();
    for value in values {
        let found = plan
            .retain
            .iter()
            .find(|(path, ..)| path.eq_ignore_ascii_case(&value.path));
        let Some((_, ty, offset, size)) = found else {
            refused.push(format!(
                "{}: the program has no RETAIN variable of that name",
                value.path
            ));
            continue;
        };
        if *ty != value.ty {
            refused.push(format!(
                "{}: its type is {ty} now, not {}",
                value.path, value.ty
            ));
            continue;
        }
        match hex::decode(&value.bytes) {
            Ok(bytes) if bytes.len() == *size as usize => {
                memory[*offset as usize..(*offset + *size) as usize].copy_from_slice(&bytes);
            }
            _ => refused.push(format!(
                "{}: the value kept is not {size} bytes of a {ty}",
                value.path
            )),
        }
    }
    refused
}

/// The program's variables in the order they lie in memory: every symbol whose bytes are
/// inside it.
fn variables_in_memory(plan: &Plan) -> Vec<&SymbolEntry> {
    let mut all: Vec<&SymbolEntry> = plan
        .symbols
        .values()
        .filter(|s| s.size > 0 && s.offset as usize + s.size as usize <= plan.memory_size)
        .collect();
    all.sort_by(|a, b| (a.offset, &a.path).cmp(&(b.offset, &b.path)));
    all
}

/// The values of every variable in `memory`, as an online change carries them.
fn all_values(plan: &Plan, memory: &[u8]) -> Vec<RetainedValue> {
    variables_in_memory(plan)
        .into_iter()
        .map(|s| RetainedValue {
            path: s.path.clone(),
            ty: s.ty.clone(),
            bytes: hex::encode(&memory[s.offset as usize..(s.offset + s.size) as usize]),
            layout: None,
        })
        .collect()
}

/// Whether a variable's value carries over as its bytes: one value, a STRING, or an array of
/// those. An array of structures or of function block instances does not: its type names its
/// elements but not their layout, which a new build may have changed under the same name.
fn carries_as_bytes(entry: &SymbolEntry) -> bool {
    match &entry.array {
        None => watchable(&entry.ty).is_some(),
        Some(array) => watchable(&array.element_type).is_some(),
    }
}

/// Why the running program's `value` cannot be taken into `entry`, this build's variable of
/// the same path; `None` when it can.
fn not_carried_because(
    entry: &SymbolEntry,
    value: &RetainedValue,
    memory_size: usize,
) -> Option<String> {
    if entry.ty != value.ty {
        return Some(format!("its type is {} now, not {}", entry.ty, value.ty));
    }
    if !carries_as_bytes(entry) {
        return Some(format!(
            "an {} is not carried: the layout of its elements is not compared, and may have changed under the same name",
            entry.ty
        ));
    }
    if entry.size == 0 || entry.offset as usize + entry.size as usize > memory_size {
        return Some("it lies outside the program's memory".to_string());
    }
    match hex::decode(&value.bytes) {
        Ok(bytes) if bytes.len() == entry.size as usize => None,
        _ => Some(format!(
            "the value given is not the {} bytes of a {}",
            entry.size, entry.ty
        )),
    }
}

/// Put the running program's values into this build's variables, over their initial and RETAIN
/// values (an online change): each only into a variable of the same path, type and size, and
/// what is not taken is said, never guessed at. Every variable of this build that takes no
/// value keeps the one it has.
fn carry(plan: &Plan, memory: &mut [u8], values: &[RetainedValue]) -> CarryReport {
    let mut report = CarryReport {
        given: true,
        ..CarryReport::default()
    };
    // Why each path of the running program was not taken, by path in lower case.
    let mut refused: HashMap<String, String> = HashMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    for value in values {
        let key = value.path.to_ascii_lowercase();
        let why = if taken.contains(&key) || refused.contains_key(&key) {
            Some("it was given more than once".to_string())
        } else {
            match plan.symbols.get(&key) {
                None => Some("this build has no variable of that name".to_string()),
                Some(entry) => not_carried_because(entry, value, plan.memory_size),
            }
        };
        match why {
            None => {
                let entry = &plan.symbols[&key];
                let bytes = hex::decode(&value.bytes).unwrap_or_default();
                let at = entry.offset as usize;
                memory[at..at + bytes.len()].copy_from_slice(&bytes);
                report.carried += 1;
                taken.insert(key);
            }
            Some(why) => {
                refused.entry(key).or_insert_with(|| why.clone());
                report.dropped.push(NotCarried {
                    variable: value.path.clone(),
                    why,
                });
            }
        }
    }
    for entry in variables_in_memory(plan) {
        let key = entry.path.to_ascii_lowercase();
        if taken.contains(&key) {
            continue;
        }
        report.initialized.push(NotCarried {
            variable: entry.path.clone(),
            why: refused
                .remove(&key)
                .unwrap_or_else(|| "the running program had no variable of that name".to_string()),
        });
    }
    report
}

/// After an online change's carry, the leaves of block copies that took a running value and
/// lose it: each member of a master is set from one copy (its writer's, or the first declaring
/// program's for a member no program writes) and then spread into every copy, so a copy's leaf
/// keeps its carried value only when that copy's leaf carried too. `initialized[p]` is the
/// paths, in lower case, of program `p`'s variables that took no running value. Gives
/// `(program, path, why)` for each leaf that carried and is set back.
fn set_from_another_copy(
    layout: &Layout,
    initialized: &[HashSet<String>],
) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for spec in &layout.blocks {
        let Some(first) = spec.first else {
            continue;
        };
        for (i, writer) in spec.writer.iter().enumerate() {
            let (s, sj) = writer.unwrap_or(first);
            let source = &layout.plans[s];
            let src_prefix = format!(
                "{}.{}",
                source.program.data[sj].name, source.program.data[sj].members[i].name
            )
            .to_ascii_lowercase();
            for &(p, j) in &spec.uses {
                if p == s {
                    continue;
                }
                let plan = &layout.plans[p];
                let u = &plan.program.data[j];
                let prefix = format!("{}.{}", u.name, u.members[i].name).to_ascii_lowercase();
                for entry in variables_in_memory(plan) {
                    let key = entry.path.to_ascii_lowercase();
                    let Some(rest) = key.strip_prefix(prefix.as_str()) else {
                        continue;
                    };
                    if !(rest.is_empty() || rest.starts_with('.') || rest.starts_with('[')) {
                        continue;
                    }
                    if initialized[p].contains(&key) {
                        continue;
                    }
                    let from = format!("{src_prefix}{rest}");
                    let carried_there =
                        source.symbols.contains_key(&from) && !initialized[s].contains(&from);
                    if !carried_there {
                        let shown = source
                            .symbols
                            .get(&from)
                            .map(|e| e.path.clone())
                            .unwrap_or(from);
                        out.push((
                            p,
                            entry.path.clone(),
                            format!(
                                "every copy of data block '{}' is set from {}, which took no running value",
                                spec.config.name,
                                layout.full_name(s, &shown)
                            ),
                        ));
                    }
                }
            }
        }
    }
    out
}

fn declarations(layout: &Layout) -> Vec<TagDeclaration> {
    layout
        .plans
        .iter()
        .flat_map(|plan| {
            plan.program.published.iter().map(|b| TagDeclaration {
                uri: b.tag.clone(),
                description: format!("variable {} of program {}", b.variable, plan.program.name),
                writable: b.writable,
            })
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

/// A watched variable as it is sent: its value, or why there is none to send.
fn watched(read: Option<VmValue>) -> (Option<Value>, Option<String>) {
    match read {
        // JSON has no NaN and no infinity, and `json!` turns them into null: such a value is
        // said, not sent as if the variable had none.
        Some(VmValue::Real(v)) if !v.is_finite() => {
            (None, Some(format!("it is {v}, not a finite number")))
        }
        Some(VmValue::Lreal(v)) if !v.is_finite() => {
            (None, Some(format!("it is {v}, not a finite number")))
        }
        Some(v) => (Some(to_json(v)), None),
        None => (
            None,
            Some("it lies outside the program's memory".to_string()),
        ),
    }
}

/// A watched STRING as it is sent: its characters, each byte the character of that code
/// (IEC 61131-3 ed. 3 Table 10 footnote l: ISO/IEC 10646, whose first 256 codes are those a
/// single-byte string writes with `$XX`). A stored length its type cannot hold is said.
fn watched_text(memory: &[u8], offset: u32, n: u16) -> (Option<Value>, Option<String>) {
    match read_string(memory, offset, n) {
        Some(bytes) => (
            Some(Value::String(bytes.iter().map(|b| *b as char).collect())),
            None,
        ),
        None => (
            None,
            Some(format!(
                "its stored length is more than the {n} characters a STRING[{n}] holds"
            )),
        ),
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
        Fault::IndexOutOfRange(at) => {
            format!("an array subscript outside its bounds (code offset {at})")
        }
        Fault::CallDepthExceeded(at) => {
            format!("calls nested deeper than the program declares (code offset {at})")
        }
        Fault::StringTooLong(at) => {
            format!("a string function gave a string longer than it may be (code offset {at})")
        }
        Fault::StringArgument(at) => format!(
            "a string function was given a length or position below zero or past its string (code offset {at})"
        ),
        Fault::BadString(at) => {
            format!("a string holds more characters than it is declared with (code offset {at})")
        }
        Fault::BadNumber(at) => {
            format!("a string converted to an integer does not write one (code offset {at})")
        }
        Fault::SelectorOutOfRange(at) => {
            format!("MUX was given a K with no input to choose (code offset {at})")
        }
    }
}

// -----------------------------------------------------------------------------
// The running runtime
// -----------------------------------------------------------------------------

/// The watchdog's limit for a task that has no interval to scale it by, and its least for
/// every task.
const DEFAULT_WATCHDOG: Duration = Duration::from_secs(2);

/// How long a cyclic task may owe a run before the watchdog stops the runtime: ten intervals,
/// and never less than two seconds, so that a host busy for a moment (a CI runner shares its
/// cores with builds) does not fault a healthy program. A run is bounded by [`BUDGET`]
/// instructions, so a program that runs at all completes one well inside this; what the
/// watchdog catches is a task that no longer ends runs, or a scheduler thread that has ended.
fn watchdog_limit(interval_ms: u64) -> Duration {
    Duration::from_millis(interval_ms.saturating_mul(10)).max(DEFAULT_WATCHDOG)
}

/// The most variables one program may have forced at once.
const FORCE_LIMIT: usize = 64;

struct Running {
    layout: Arc<Layout>,
    control: Arc<Control>,
    quit: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
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
            running.control.notify();
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

/// The running values of an online change, each for the program its path names, and what was
/// not taken because it names no program of this runtime.
fn split_carried(
    layout: &Layout,
    values: &[RetainedValue],
) -> (Vec<Vec<RetainedValue>>, Vec<NotCarried>) {
    let mut each = vec![Vec::new(); layout.plans.len()];
    let mut dropped = Vec::new();
    for value in values {
        match layout.split(&value.path) {
            Ok((p, path)) => each[p].push(RetainedValue {
                path: path.to_string(),
                ..value.clone()
            }),
            Err(why) => dropped.push(NotCarried {
                variable: value.path.clone(),
                why,
            }),
        }
    }
    (each, dropped)
}

/// A path of program `p` named as the runtime names it, in a list of values.
fn named(layout: &Layout, p: usize, values: Vec<RetainedValue>) -> Vec<RetainedValue> {
    values
        .into_iter()
        .map(|v| RetainedValue {
            path: layout.full_name(p, &v.path),
            ..v
        })
        .collect()
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

    /// Load the programs, and put the runtime in RUN if the configuration says so.
    async fn start(&mut self, _ctx: &PluginContext, config: &Value) -> Result<(), PluginError> {
        self.halt();
        let Loaded {
            layout,
            run,
            retained,
            carried,
        } = plan(config)?;
        let mut shared = Shared::new(&layout, retained);
        if run {
            shared.restart(&layout, false)?;
            // An online change: the running programs' values over the initial and RETAIN ones,
            // the clock going on from where it was, before the first run; no startup task runs.
            if let Some(carried) = &carried {
                let (each, dropped) = split_carried(&layout, &carried.values);
                let mut report = CarryReport {
                    given: true,
                    dropped,
                    ..CarryReport::default()
                };
                let mut initialized = Vec::new();
                for (p, values) in each.iter().enumerate() {
                    let r = carry(&layout.plans[p], &mut shared.programs[p].image, values);
                    initialized.push(
                        r.initialized
                            .iter()
                            .map(|n| n.variable.to_ascii_lowercase())
                            .collect::<HashSet<String>>(),
                    );
                    report.carried += r.carried;
                    let full = |n: NotCarried| NotCarried {
                        variable: layout.full_name(p, &n.variable),
                        why: n.why,
                    };
                    report.dropped.extend(r.dropped.into_iter().map(full));
                    report
                        .initialized
                        .extend(r.initialized.into_iter().map(full));
                }
                // A copy that carried a member whose master is set from a copy that did not:
                // spread below gives it that copy's value, so it did not carry.
                for (p, path, why) in set_from_another_copy(&layout, &initialized) {
                    report.carried = report.carried.saturating_sub(1);
                    report.initialized.push(NotCarried {
                        variable: layout.full_name(p, &path),
                        why,
                    });
                }
                shared.carry = report;
                // Each data block's master from the carried copies of its writers (with every
                // task run handing over whole, a master's member is its writer's copy), then
                // every copy from its master.
                shared.compose(&layout);
                shared.spread(&layout);
                let clock = Duration::from_nanos(carried.clock_ns.max(0) as u64);
                if let Some(since) = Instant::now().checked_sub(clock) {
                    shared.since = since;
                }
                shared.begin_tasks(&layout, true);
            }
        } else if let Some(carried) = &carried {
            // A runtime loaded in STOP is not changed online: it starts when it is put in RUN,
            // from its initial values and its RETAIN ones.
            shared.carry = CarryReport {
                given: true,
                carried: 0,
                dropped: carried
                    .values
                    .iter()
                    .map(|v| NotCarried {
                        variable: v.path.clone(),
                        why: "the program was loaded in STOP".to_string(),
                    })
                    .collect(),
                initialized: Vec::new(),
            };
        }
        let layout = Arc::new(layout);
        let control = Arc::new(Control::new(shared));
        let quit = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("logic-scan".to_string())
            .spawn({
                let (layout, control, quit) =
                    (Arc::clone(&layout), Arc::clone(&control), Arc::clone(&quit));
                move || tasks::schedule(layout, control, quit)
            })
            .map_err(|e| PluginError::StartFailed(format!("the scan thread did not start: {e}")))?;
        self.running = Some(Running {
            layout,
            control,
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
            Some(r) => format!("{:?}", r.control.lock().mode),
            None => "no program is loaded".to_string(),
        }))
    }
}

#[async_trait]
impl ProtocolDriver for VmEngine {
    async fn describe_config(&self, config: &Value) -> Result<Vec<TagDeclaration>, PluginError> {
        Ok(declarations(&plan(config)?.layout))
    }

    fn declared_tags(&self) -> Vec<TagDeclaration> {
        self.running
            .as_ref()
            .map(|r| declarations(&r.layout))
            .unwrap_or_default()
    }

    async fn poll_cycle(&mut self, _ctx: &PluginContext) -> Result<Vec<TagSample>, PluginError> {
        Err(PluginError::RuntimeError(
            "a runtime module is exchanged with, not polled".to_string(),
        ))
    }

    /// An operator writes a setpoint: a published variable marked writable. It is in the
    /// program's image at once, and its program takes it when its task next begins a run.
    async fn write_tag(
        &mut self,
        _ctx: &PluginContext,
        tag_uri: &str,
        value: &Value,
    ) -> Result<(), PluginError> {
        let running = self.running()?;
        let (p, (var, _, writable)) = running
            .layout
            .plans
            .iter()
            .enumerate()
            .find_map(|(p, plan)| {
                plan.published
                    .iter()
                    .find(|(_, tag, _)| tag == tag_uri)
                    .map(|b| (p, b))
            })
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
        let mut g = running.control.lock();
        if g.mode != RuntimeMode::Run {
            return Err(PluginError::InvalidTag(
                "the program is not running, so the setpoint was not taken".to_string(),
            ));
        }
        let live = &mut g.programs[p];
        if live.is_forced(var.offset) {
            return Err(PluginError::InvalidTag(format!(
                "{tag_uri} is forced by an engineer, so the setpoint was not taken"
            )));
        }
        live.setpoints.retain(|(offset, _)| *offset != var.offset);
        live.setpoints.push((var.offset, v));
        write(&mut live.image, var.offset, v);
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
        let layout = &running.layout;
        let mut g = running.control.lock();
        g.feed(
            layout,
            inputs
                .into_iter()
                .map(|i| (i.tag, i.value, i.good))
                .collect(),
        );
        // Checked before anything is reported: a runtime the watchdog stops reports no value
        // and asks for no write in this very exchange.
        let ended = running.thread.as_ref().is_none_or(JoinHandle::is_finished);
        g.watchdog(layout, ended, Instant::now());
        running.control.notify();

        let mut samples = Vec::new();
        let mut outputs = Vec::new();
        for (p, plan) in layout.plans.iter().enumerate() {
            let silent = g.silent_because(layout, p);
            let image = &g.programs[p].image;
            samples.extend(plan.published.iter().map(|(var, tag, _)| {
                let sample = match (&silent, read(image, var.offset, var.ty)) {
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
                };
                WireSample::from(&sample)
            }));
            if silent.is_none() {
                outputs.extend(plan.outputs.iter().filter_map(|(var, tag)| {
                    read(image, var.offset, var.ty).map(|v| WireOutput {
                        tag: tag.clone(),
                        value: to_json(v),
                    })
                }));
            }
        }
        Ok(ExchangeReply {
            state: g.state(layout),
            samples,
            outputs,
        })
    }

    async fn set_mode(
        &mut self,
        _ctx: &PluginContext,
        run: bool,
        cold: bool,
    ) -> Result<RuntimeState, PluginError> {
        let running = self.running()?;
        let mut g = running.control.lock();
        match (run, g.mode) {
            (true, RuntimeMode::Run) if !cold => {}
            (true, _) => g.restart(&running.layout, cold)?,
            (false, _) => g.stop(),
        }
        running.control.notify();
        Ok(g.state(&running.layout))
    }

    async fn watch(
        &mut self,
        _ctx: &PluginContext,
        variables: Vec<String>,
    ) -> Result<WatchReply, PluginError> {
        let running = self.running()?;
        let layout = &running.layout;
        // Images, under one lock: every value as its program's last completed run left it.
        let g = running.control.lock();
        let values = variables
            .into_iter()
            .map(|variable| {
                let found = layout.split(&variable).and_then(|(p, path)| {
                    locate(&layout.plans[p], path).map(|(offset, shape, ty)| (p, offset, shape, ty))
                });
                match found {
                    Err(why) => WatchedValue {
                        variable,
                        ty: None,
                        value: None,
                        error: Some(why),
                        forced: false,
                    },
                    Ok((p, offset, shape, ty)) => {
                        let live = &g.programs[p];
                        let (value, error) = match shape {
                            Shape::Value(elem) => watched(read(&live.image, offset, elem)),
                            Shape::Text(n) => watched_text(&live.image, offset, n),
                        };
                        WatchedValue {
                            variable,
                            ty: Some(ty),
                            value,
                            error,
                            forced: matches!(shape, Shape::Value(_)) && live.is_forced(offset),
                        }
                    }
                }
            })
            .collect();
        Ok(WatchReply {
            state: g.state(layout),
            values,
        })
    }

    /// The values of its RETAIN variables now; for a program that has not run, the values the
    /// host gave, so that a runtime loaded in STOP never has its kept values replaced by zeros.
    /// Then each RETAIN data block's master, by the block's name and with its layout.
    async fn retained(&mut self, _ctx: &PluginContext) -> Result<Vec<RetainedValue>, PluginError> {
        let running = self.running()?;
        let layout = &running.layout;
        let g = running.control.lock();
        let mut values = Vec::new();
        for (p, live) in g.programs.iter().enumerate() {
            let mine = if live.ran {
                snapshot(&layout.plans[p], &live.image)
            } else {
                live.pending.clone()
            };
            values.extend(named(layout, p, mine));
        }
        values.extend(g.kept_blocks(layout));
        values.extend(g.unclaimed.iter().cloned());
        Ok(values)
    }

    /// Every variable's value now, and the clock, from the images: each program as its last
    /// completed run left it. Only in RUN: a runtime that is not running has no running state
    /// to carry on.
    async fn variables(&mut self, _ctx: &PluginContext) -> Result<ProgramVariables, PluginError> {
        let running = self.running()?;
        let layout = &running.layout;
        let g = running.control.lock();
        if g.mode != RuntimeMode::Run {
            return Err(PluginError::RuntimeError(
                "the program is not in RUN, so it has no running values to carry on".to_string(),
            ));
        }
        let mut values = Vec::new();
        for (p, live) in g.programs.iter().enumerate() {
            values.extend(named(layout, p, all_values(&layout.plans[p], &live.image)));
        }
        Ok(ProgramVariables {
            clock_ns: g.since.elapsed().as_nanos().min(i64::MAX as u128) as i64,
            values,
        })
    }

    /// What it did with the running values it was started with.
    async fn carried(&mut self, _ctx: &PluginContext) -> Result<CarryReport, PluginError> {
        Ok(self.running()?.control.lock().carry.clone())
    }

    /// Hold a variable at a value. Only in RUN: a runtime that is not running has nothing to
    /// hold, and leaving RUN releases every force, so none outlives the run it was set in.
    async fn force(
        &mut self,
        _ctx: &PluginContext,
        variable: String,
        value: Value,
    ) -> Result<RuntimeState, PluginError> {
        let running = self.running()?;
        let layout = &running.layout;
        let mut g = running.control.lock();
        if g.mode != RuntimeMode::Run {
            return Err(PluginError::RuntimeError(
                "the program is not running: a variable is forced only while it runs, and \
                 stopping it releases every force"
                    .to_string(),
            ));
        }
        let (p, path) = layout.split(&variable).map_err(PluginError::InvalidTag)?;
        let (offset, shape, ty) =
            locate(&layout.plans[p], path).map_err(PluginError::InvalidTag)?;
        let elem = match shape {
            Shape::Value(ElemType::Time) | Shape::Text(_) => {
                return Err(PluginError::InvalidTag(format!(
                    "{variable} is a {ty}; only a BOOL or a number can be forced"
                )))
            }
            Shape::Value(elem) => elem,
        };
        let held = to_vm(elem, &value).ok_or_else(|| {
            PluginError::InvalidTag(format!(
                "{value} is not a value of {variable}, a {ty}: nothing was forced"
            ))
        })?;
        let live = &mut g.programs[p];
        let count = live.forces.len();
        match live.forces.iter_mut().find(|f| f.offset == offset) {
            Some(f) => {
                f.variable = variable;
                f.value = held;
            }
            None if count >= FORCE_LIMIT => {
                return Err(PluginError::InvalidTag(format!(
                    "{FORCE_LIMIT} variables are forced already, the most one program may have"
                )))
            }
            None => live.forces.push(Forced {
                variable,
                offset,
                value: held,
            }),
        }
        // Into the image at once, so an exchange before the next run already gives it; the
        // program's working memory takes it when its task next begins a run.
        write(&mut live.image, offset, held);
        Ok(g.state(layout))
    }

    /// Release one forced variable, or every one. The program decides its value again from its
    /// next run.
    async fn unforce(
        &mut self,
        _ctx: &PluginContext,
        variable: Option<String>,
    ) -> Result<RuntimeState, PluginError> {
        let running = self.running()?;
        let layout = &running.layout;
        let mut g = running.control.lock();
        match variable {
            None => g.programs.iter_mut().for_each(|p| p.forces.clear()),
            Some(variable) => {
                let (p, path) = layout.split(&variable).map_err(PluginError::InvalidTag)?;
                let (offset, _, _) =
                    locate(&layout.plans[p], path).map_err(PluginError::InvalidTag)?;
                let forces = &mut g.programs[p].forces;
                let before = forces.len();
                forces.retain(|f| f.offset != offset);
                if forces.len() == before {
                    return Err(PluginError::InvalidTag(format!("{variable} is not forced")));
                }
            }
        }
        Ok(g.state(layout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a future of this engine to its end. Its calls never wait on anything, so polling
    /// is enough; there is no async runtime in this crate to borrow.
    fn finish<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let mut future = std::pin::pin!(future);
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
        }
    }

    fn retaining(symbols: Value) -> Result<Plan, PluginError> {
        let bytes = include_bytes!(
            "../../../crates/mythos-logic-compiler/tests/fixtures/format1/integers_and_control.mlbc"
        );
        plan(&json!({ "program": {
            "name": "P", "version": "1", "interval_ms": 10,
            "code": hex::encode(bytes), "symbols": symbols,
        }}))
        .map(|loaded| loaded.layout.plans.into_iter().next().unwrap())
    }

    #[test]
    fn a_tag_is_never_bound_to_write_what_only_a_function_block_writes() {
        let bytes = include_bytes!(
            "../../../crates/mythos-logic-compiler/tests/fixtures/format1/integers_and_control.mlbc"
        );
        let symbols = json!([
            {"path": "t.IN", "type": "BOOL", "offset": 0, "size": 1},
            {"path": "t.Q", "type": "BOOL", "offset": 1, "size": 1, "private": true},
        ]);
        let with = |inputs: Value, published: Value| {
            plan(&json!({ "program": {
                "name": "P", "version": "1", "interval_ms": 10,
                "code": hex::encode(bytes), "symbols": symbols,
                "inputs": inputs, "published": published,
            }}))
            .map(|_| ())
            .map_err(|e| e.to_string())
        };
        assert_eq!(
            with(
                json!([{"variable": "t.IN", "tag": "external.A.B.c"}]),
                json!([])
            ),
            Ok(())
        );
        assert_eq!(
            with(
                json!([]),
                json!([{"variable": "t.Q", "tag": "internal.A.q"}])
            ),
            Ok(()),
            "read, it is published"
        );
        let refused = with(
            json!([{"variable": "t.Q", "tag": "external.A.B.c"}]),
            json!([]),
        );
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.contains("t.Q is inside a function block instance")
                    && e.ends_with("it cannot be read from a tag")),
            "{refused:?}"
        );
        let refused = with(
            json!([]),
            json!([{"variable": "t.Q", "tag": "internal.A.q", "writable": true}]),
        );
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.ends_with("it cannot be a setpoint")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_retained_value_is_taken_only_into_a_variable_of_the_same_name_type_and_size() {
        let plan = retaining(json!([
            {"path": "Total", "type": "DINT", "offset": 0, "size": 4, "retain": true},
            {"path": "n", "type": "DINT", "offset": 4, "size": 4},
        ]))
        .unwrap();
        assert_eq!(plan.retain.len(), 1, "only what is RETAIN");
        let mut memory = vec![0u8; plan.memory_size];
        memory[..4].copy_from_slice(&1234i32.to_le_bytes());
        let kept = snapshot(&plan, &memory);
        assert_eq!(
            serde_json::to_value(&kept).unwrap(),
            json!([{"path": "Total", "type": "DINT", "bytes": "d2040000"}])
        );

        let value = |path: &str, ty: &str, bytes: &str| RetainedValue {
            path: path.into(),
            ty: ty.into(),
            bytes: bytes.into(),
            layout: None,
        };
        let mut fresh = vec![0u8; plan.memory_size];
        let refused = restore(
            &plan,
            &mut fresh,
            &[
                value("total", "DINT", "d2040000"),
                value("gone", "DINT", "01000000"),
                value("Total", "INT", "0100"),
                value("Total", "DINT", "0100"),
            ],
        );
        assert_eq!(
            &fresh[..4],
            &1234i32.to_le_bytes(),
            "the name's case does not count"
        );
        assert_eq!(
            refused,
            vec![
                "gone: the program has no RETAIN variable of that name".to_string(),
                "Total: its type is DINT now, not INT".to_string(),
                "Total: the value kept is not 4 bytes of a DINT".to_string(),
            ]
        );

        // A RETAIN variable the program's memory cannot hold is refused when it is loaded.
        let outside = retaining(json!([
            {"path": "far", "type": "DINT", "offset": 1_000_000, "size": 4, "retain": true},
        ]));
        assert!(outside.err().is_some_and(|e| e
            .to_string()
            .contains("the RETAIN variable far lies outside")));
    }

    #[test]
    fn the_watchdog_allows_ten_intervals_and_never_less_than_two_seconds() {
        assert_eq!(watchdog_limit(20), Duration::from_secs(2));
        assert_eq!(watchdog_limit(200), Duration::from_secs(2));
        assert_eq!(watchdog_limit(500), Duration::from_secs(5));
        assert_eq!(watchdog_limit(u64::MAX), Duration::from_millis(u64::MAX));
    }

    #[test]
    fn a_program_whose_scan_thread_ends_is_faulted_at_the_next_exchange_and_reports_nothing() {
        let bytes = include_bytes!(
            "../../../crates/mythos-logic-compiler/tests/fixtures/format1/integers_and_control.mlbc"
        );
        let config = json!({ "run": true, "program": {
            "name": "P", "version": "1", "interval_ms": 10,
            "code": hex::encode(bytes), "symbols": [],
        }});
        let ctx = PluginContext::new("t", HashSet::new(), ".");
        let mut engine = VmEngine::default();
        finish(engine.start(&ctx, &config)).unwrap();
        let first = finish(engine.exchange(&ctx, Vec::new())).unwrap();
        assert_eq!(first.state.mode, RuntimeMode::Run);
        let deadline = Instant::now() + Duration::from_secs(10);
        while finish(engine.exchange(&ctx, Vec::new()))
            .unwrap()
            .state
            .scans
            == 0
        {
            assert!(Instant::now() < deadline, "the program never scanned");
            std::thread::sleep(Duration::from_millis(10));
        }

        // A healthy program runs past the watchdog's limit and is not stopped by it.
        let past = Instant::now() + watchdog_limit(10) + Duration::from_millis(500);
        while Instant::now() < past {
            let healthy = finish(engine.exchange(&ctx, Vec::new())).unwrap();
            assert_eq!(
                healthy.state.mode,
                RuntimeMode::Run,
                "{:?}",
                healthy.state.fault
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        // The scan thread ends, as a panic would end it.
        let running = engine.running.as_mut().unwrap();
        running.quit.store(true, Ordering::SeqCst);
        running.thread.take().unwrap().join().unwrap();

        let after = finish(engine.exchange(&ctx, Vec::new())).unwrap();
        assert_eq!(after.state.mode, RuntimeMode::Fault);
        assert_eq!(
            after.state.fault.as_deref(),
            Some("watchdog: the scan thread has ended; load the program again")
        );
        assert!(after.outputs.is_empty(), "nothing is asked to be written");
    }

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
    fn a_watched_value_that_json_cannot_carry_is_said_not_sent_as_null() {
        assert_eq!(watched(Some(VmValue::Real(1.5))), (Some(json!(1.5)), None));
        assert_eq!(
            watched(Some(VmValue::Bool(false))),
            (Some(json!(false)), None)
        );
        for (value, word) in [
            (VmValue::Real(f32::NAN), "NaN"),
            (VmValue::Real(f32::INFINITY), "inf"),
            (VmValue::Lreal(f64::NEG_INFINITY), "-inf"),
        ] {
            let (sent, why) = watched(Some(value));
            assert_eq!(sent, None, "{word}");
            assert_eq!(
                why.as_deref(),
                Some(format!("it is {word}, not a finite number").as_str())
            );
        }
        let (sent, why) = watched(None);
        assert_eq!(sent, None);
        assert!(why.unwrap().contains("outside the program's memory"));
    }

    #[test]
    fn a_watched_string_is_its_characters_or_why_it_has_none() {
        assert_eq!(watchable("STRING[10]"), Some(Shape::Text(10)));
        assert_eq!(watchable("STRING[10"), None);
        assert_eq!(
            watchable("STRING"),
            None,
            "the compiler always names the length"
        );
        assert_eq!(Shape::Text(10).size(), 12);
        // Length 3, room for 4: 'A', 'é' written $E9, a line feed.
        let memory = [3, 0, b'A', 0xE9, b'\n', 0];
        assert_eq!(
            watched_text(&memory, 0, 4),
            (Some(json!("A\u{e9}\n")), None)
        );
        // A stored length past what the type holds is never followed.
        let memory = [5, 0, b'A', b'B', b'C', b'D'];
        let (sent, why) = watched_text(&memory, 0, 4);
        assert_eq!(sent, None);
        assert!(why.unwrap().contains("more than the 4 characters"));
    }

    #[test]
    fn a_name_with_subscripts_is_the_name_and_whole_numbers() {
        assert_eq!(subscripts("level"), Ok(("level", vec![])));
        assert_eq!(subscripts("hist[2]"), Ok(("hist", vec![2])));
        assert_eq!(subscripts("grid[ -1 , 3 ]"), Ok(("grid", vec![-1, 3])));
        assert!(subscripts("hist[2").is_err());
        assert!(subscripts("hist[]").is_err());
        assert!(subscripts("hist[1.5]").is_err());
        assert!(subscripts("hist[2]x").is_err());
    }

    fn value(path: &str, ty: &str, bytes: &str) -> RetainedValue {
        RetainedValue {
            path: path.into(),
            ty: ty.into(),
            bytes: bytes.into(),
            layout: None,
        }
    }

    fn not_carried(variable: &str, why: &str) -> NotCarried {
        NotCarried {
            variable: variable.into(),
            why: why.into(),
        }
    }

    /// A new build of a program, as an online change finds it: a RETAIN and a plain counter
    /// kept, one variable added, one whose type changed, an array of numbers, an array of
    /// instances, and one variable the old build sends too few bytes for.
    fn new_build() -> Plan {
        let plan = retaining(json!([
            {"path": "Total", "type": "DINT", "offset": 0, "size": 4, "retain": true},
            {"path": "n", "type": "DINT", "offset": 4, "size": 4},
            {"path": "added", "type": "INT", "offset": 8, "size": 2},
            {"path": "kind", "type": "INT", "offset": 10, "size": 2},
            {"path": "pair", "type": "ARRAY [0..1] OF INT", "offset": 12, "size": 4,
             "array": {"element_type": "INT", "element_size": 2, "bounds": [[0, 1]]}},
            {"path": "blocks", "type": "ARRAY [0..0] OF Pump", "offset": 16, "size": 4,
             "array": {"element_type": "Pump", "element_size": 4, "bounds": [[0, 0]]}},
            {"path": "w", "type": "INT", "offset": 20, "size": 2},
        ]))
        .unwrap();
        assert!(plan.memory_size >= 22, "the fixture's memory holds them");
        plan
    }

    #[test]
    fn a_running_value_is_carried_only_into_a_variable_of_the_same_name_type_and_size() {
        let plan = new_build();
        let mut memory = vec![0xAAu8; plan.memory_size];
        let report = carry(
            &plan,
            &mut memory,
            &[
                value("total", "DINT", "d2040000"),
                value("n", "DINT", "07000000"),
                value("removed", "DINT", "01000000"),
                value("kind", "DINT", "05000000"),
                value("pair", "ARRAY [0..1] OF INT", "01000200"),
                value("blocks", "ARRAY [0..0] OF Pump", "09090909"),
                value("w", "INT", "01"),
            ],
        );
        // Taken: the RETAIN counter and the plain one alike, and the array of numbers. The
        // name's case does not count.
        assert_eq!(&memory[0..4], &1234i32.to_le_bytes());
        assert_eq!(&memory[4..8], &7i32.to_le_bytes());
        assert_eq!(&memory[12..16], &[1, 0, 2, 0]);
        // Not taken: what this build had before the values came, untouched.
        assert_eq!(&memory[8..12], &[0xAA; 4], "added and kind keep theirs");
        assert_eq!(&memory[16..22], &[0xAA; 6], "blocks and w keep theirs");
        assert_eq!(report.carried, 3);
        assert!(report.given);
        let blocks = "an ARRAY [0..0] OF Pump is not carried: the layout of its elements is \
                      not compared, and may have changed under the same name";
        assert_eq!(
            report.dropped,
            vec![
                not_carried("removed", "this build has no variable of that name"),
                not_carried("kind", "its type is INT now, not DINT"),
                not_carried("blocks", blocks),
                not_carried("w", "the value given is not the 2 bytes of a INT"),
            ]
        );
        // In the order they lie in memory, each with why.
        assert_eq!(
            report.initialized,
            vec![
                not_carried("added", "the running program had no variable of that name"),
                not_carried("kind", "its type is INT now, not DINT"),
                not_carried("blocks", blocks),
                not_carried("w", "the value given is not the 2 bytes of a INT"),
            ]
        );
    }

    #[test]
    fn a_value_given_twice_is_taken_once() {
        let plan = new_build();
        let mut memory = vec![0u8; plan.memory_size];
        let report = carry(
            &plan,
            &mut memory,
            &[
                value("n", "DINT", "07000000"),
                value("N", "DINT", "08000000"),
            ],
        );
        assert_eq!(&memory[4..8], &7i32.to_le_bytes());
        assert_eq!(report.carried, 1);
        assert_eq!(
            report.dropped,
            vec![not_carried("N", "it was given more than once")]
        );
    }

    #[test]
    fn every_variable_is_given_in_the_order_it_lies_in_memory() {
        let plan = new_build();
        let mut memory = vec![0u8; plan.memory_size];
        memory[4..8].copy_from_slice(&7i32.to_le_bytes());
        let values = all_values(&plan, &memory);
        let paths: Vec<&str> = values.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(
            paths,
            ["Total", "n", "added", "kind", "pair", "blocks", "w"]
        );
        assert_eq!(values[1], value("n", "DINT", "07000000"));
        // What one build gives, the same build takes whole.
        let mut fresh = vec![0xFFu8; plan.memory_size];
        let report = carry(&plan, &mut fresh, &values);
        assert_eq!(report.carried, 6, "all but the array of instances");
        assert_eq!(&fresh[..16], &memory[..16]);
    }

    /// The program of [`new_build`] in the engine: started into RUN with the running one's
    /// values, it holds them before any scan (it is not fed, so none runs), its clock goes on
    /// from the running one's, and it says what it took. Loaded in STOP, it takes none.
    #[test]
    fn a_program_started_with_running_values_takes_them_before_its_first_scan() {
        let bytes = include_bytes!(
            "../../../crates/mythos-logic-compiler/tests/fixtures/format1/integers_and_control.mlbc"
        );
        let symbols = json!([
            {"path": "Total", "type": "DINT", "offset": 0, "size": 4, "retain": true},
            {"path": "n", "type": "DINT", "offset": 4, "size": 4},
        ]);
        let config = |run: bool| {
            json!({
                "program": {
                    "name": "P", "version": "2", "interval_ms": 10,
                    "code": hex::encode(bytes), "symbols": symbols,
                },
                "run": run,
                "retained": [value("Total", "DINT", "01000000")],
                "carried": {
                    "clock_ns": 3_600_000_000_000i64,
                    "values": [value("Total", "DINT", "d2040000"), value("n", "DINT", "07000000")],
                },
            })
        };
        let ctx = PluginContext::new("io.runtime:p", HashSet::new(), String::new());

        let mut engine = VmEngine::default();
        finish(engine.start(&ctx, &config(true))).unwrap();
        let now = finish(engine.variables(&ctx)).unwrap();
        assert_eq!(
            &now.values[..2],
            &[
                value("Total", "DINT", "d2040000"),
                value("n", "DINT", "07000000")
            ],
            "the running values, over the RETAIN one kept"
        );
        assert!(
            now.clock_ns >= 3_600_000_000_000,
            "the clock goes on from an hour, not from zero: {}",
            now.clock_ns
        );
        let report = finish(engine.carried(&ctx)).unwrap();
        assert_eq!((report.given, report.carried), (true, 2));
        assert!(report.dropped.is_empty() && report.initialized.is_empty());

        let mut stopped = VmEngine::default();
        finish(stopped.start(&ctx, &config(false))).unwrap();
        assert!(
            finish(stopped.variables(&ctx)).is_err(),
            "a program in STOP gives no running values"
        );
        let report = finish(stopped.carried(&ctx)).unwrap();
        assert_eq!(report.carried, 0);
        assert_eq!(
            report.dropped,
            vec![
                not_carried("Total", "the program was loaded in STOP"),
                not_carried("n", "the program was loaded in STOP"),
            ]
        );
        // Put in RUN, it is a warm restart: the RETAIN value kept, nothing carried.
        finish(stopped.set_mode(&ctx, true, false)).unwrap();
        let now = finish(stopped.variables(&ctx)).unwrap();
        assert_eq!(now.values[0], value("Total", "DINT", "01000000"));
        assert!(now.clock_ns < 3_600_000_000_000);
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
        // It says it runs tasks, of every kind, in the form the core reads.
        assert_eq!(
            mythos_module::protocol::TaskSupport::of(&m.profile)
                .expect("the profile says it runs tasks")
                .kinds,
            ["cycle", "cyclic", "startup", "event"]
        );
    }

    #[test]
    fn a_program_built_before_an_update_of_the_vm_is_still_taken() {
        // Bytes the format-1 and the format-2 compiler made, as a project's runtimes.json
        // holds them from before an update: the module takes them as it took them then
        // (ADR-013 §2.3, "Format versions"), rather than leave the program stopped until it
        // is built again.
        let older: [(u16, &[u8]); 2] = [
            (
                1,
                include_bytes!(
                    "../../../crates/mythos-logic-compiler/tests/fixtures/format1/integers_and_control.mlbc"
                ),
            ),
            (
                2,
                include_bytes!(
                    "../../../crates/mythos-logic-compiler/tests/fixtures/format2/strings.mlbc"
                ),
            ),
        ];
        for (format, bytes) in older {
            assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), format);
            let config = json!({ "program": {
                "name": "older",
                "version": format!("format-{format}"),
                "interval_ms": 10,
                "code": hex::encode(bytes),
                "symbols": [],
            }});
            let taken = plan(&config);
            assert!(
                taken.is_ok(),
                "format {format}: {:?}",
                taken.err().map(|e| e.to_string())
            );
        }
        let bytes = older[0].1;
        // A version newer than this VM's is refused, and the reason says so.
        let mut newer = bytes.to_vec();
        newer[4..6].copy_from_slice(&(mythos_logic_vm::FORMAT_VERSION + 1).to_le_bytes());
        let config = json!({ "program": {
            "name": "newer",
            "version": "newer",
            "interval_ms": 10,
            "code": hex::encode(newer),
            "symbols": [],
        }});
        let refused = plan(&config)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(refused.contains("UnsupportedVersion"), "{refused}");
    }
}
