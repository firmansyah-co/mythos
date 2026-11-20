//! MythOS control logic authoring, as a service module (ADR-013 §2.6).
//!
//! A project's programs live in the module's data directory (`<project>/logic/`): the list
//! with each program's settings and its bindings to tags in `programs.json`, and each
//! program's source beside it: `programs/<name>.st` for Structured Text, `programs/<name>.xml`
//! for a Ladder Diagram or a Function Block Diagram (a PLCopen TC6 XML v2.01 document holding
//! its POUs, ADR-013 §2.15, §2.16). The module checks a program (the compiler's diagnostics,
//! each with the range of the source it is about or, in a diagram, the element; its variables
//! for the binding table) and
//! builds it: compiled for its target, bindings checked against its variables, packed as the
//! runtime instance the core loads (`PUT /api/v1/projects/:id/runtimes`).
//!
//! **Each program is compiled from its own source alone** (`compile_sources` with that one
//! source). Every source of a project holds a PROGRAM, and a program is compiled from sources
//! of which exactly one POU is a PROGRAM, so another program's functions and function blocks
//! cannot be compiled in without taking its PROGRAM out of its file; and a program built with
//! another's POUs would change, and take a new version, when that other program is edited.
//! A function or function block a program needs is in its own source: in a Ladder Diagram
//! document beside its PROGRAM, in Structured Text before it. A library of the project's own,
//! sources without a PROGRAM shared by every program, is not built.
//!
//! It never talks to a runtime, and it does not load what it builds: a module cannot call the
//! core. The workbench, or an agent over MCP, takes the built instance and saves it into the
//! project's runtimes.

use async_trait::async_trait;
use mythos_logic_compiler::{
    compile_sources, At, BodyOf, Diagnostic, Holds, Sources, Symbol, TargetProfile,
};
use mythos_module::protocol::{
    HttpCall, HttpReply, InputBinding, InputOf, ModuleKind, ModuleManifest, OutputBinding,
    PublishedBinding, RouteDecl, RouteScope, RuntimeProgram, PROTOCOL_VERSION,
};
use mythos_module::{Events, Service};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

pub const MODULE_ID: &str = "org.mythos.logic";
/// The module's name in URLs and its data directory: `/api/v1/projects/:id/logic/…`.
pub const PROVIDES: &str = "logic";
/// Sent on the telemetry stream when a project's programs change, so open editors follow.
pub const PROGRAMS_CHANGED_EVENT: &str = "LOGIC_PROGRAMS_CHANGED";

const PROGRAMS_FILE: &str = "programs.json";
const SOURCES_DIR: &str = "programs";

// -----------------------------------------------------------------------------
// What is stored
// -----------------------------------------------------------------------------

/// One program of a project: how it runs and how it is tied to the tags. Its source is a file
/// of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramEntry {
    /// Its name in the project, and the id of the runtime instance it becomes.
    pub name: String,
    /// The language of its source: `st` (Structured Text), `ld` (Ladder Diagram), `fbd`
    /// (Function Block Diagram) or `sfc` (Sequential Function Chart), each diagram a TC6
    /// document. A `programs.json` written before
    /// there was a choice has none, which is `st`.
    #[serde(default = "structured_text")]
    pub language: String,
    /// The interval of its cyclic task.
    pub interval_ms: u64,
    /// The runtime module that runs it: a module's `provides`.
    pub runtime: String,
    /// The target profile it is compiled for.
    pub target: String,
    /// Whether it is put in RUN when it is loaded.
    pub run: bool,
    #[serde(default)]
    pub inputs: Vec<InputBinding>,
    #[serde(default)]
    pub outputs: Vec<OutputBinding>,
    #[serde(default)]
    pub published: Vec<PublishedBinding>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Programs {
    #[serde(default)]
    programs: Vec<ProgramEntry>,
}

fn structured_text() -> String {
    "st".into()
}

/// The languages a program may be written in, each with the extension of its file.
const LANGUAGES: &[(&str, &str)] = &[("st", "st"), ("ld", "xml"), ("fbd", "xml"), ("sfc", "xml")];

/// Whether a program in `language` is a PLCopen TC6 document.
fn is_tc6(language: &str) -> bool {
    matches!(language, "ld" | "fbd" | "sfc")
}

type Refusal = (u16, Value);

fn error(status: u16, message: impl ToString) -> Refusal {
    (status, json!({ "error": message.to_string() }))
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn load(data: &Path) -> Result<Programs, Refusal> {
    let path = data.join(PROGRAMS_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            error(
                500,
                format!(
                    "{} cannot be read as the project's programs: {e}",
                    path.display()
                ),
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Programs::default()),
        Err(e) => Err(error(500, format!("{}: {e}", path.display()))),
    }
}

/// Write a file whole or not at all: a temporary beside it, then a rename.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Refusal> {
    let fail = |e: std::io::Error| error(500, format!("{}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(fail)?;
    std::fs::rename(&tmp, path).map_err(fail)
}

fn save(data: &Path, programs: &Programs) -> Result<(), Refusal> {
    let bytes = serde_json::to_vec_pretty(programs).map_err(|e| error(500, e))?;
    write_atomic(&data.join(PROGRAMS_FILE), &bytes)
}

/// The file of a program's source: `.st` or, for a diagram, `.xml`.
fn source_path(data: &Path, entry: &ProgramEntry) -> PathBuf {
    let extension = LANGUAGES
        .iter()
        .find(|(l, _)| *l == entry.language)
        .map_or("st", |(_, e)| e);
    data.join(SOURCES_DIR)
        .join(format!("{}.{extension}", entry.name))
}

fn read_source(data: &Path, entry: &ProgramEntry) -> Result<String, Refusal> {
    let path = source_path(data, entry);
    std::fs::read_to_string(&path).map_err(|e| {
        error(
            500,
            format!("the source of {} cannot be read: {e}", entry.name),
        )
    })
}

fn find<'a>(programs: &'a Programs, name: &str) -> Result<&'a ProgramEntry, Refusal> {
    programs
        .programs
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| error(404, format!("the project has no program '{name}'")))
}

/// What a new program starts as. In Structured Text, every section a program may have, empty.
/// In a Ladder Diagram, a TC6 project holding the PROGRAM with no variable and a body of the
/// two power rails and no rung between them; in a Function Block Diagram, the PROGRAM with no
/// variable and an empty FBD body; in a Sequential Function Chart, the PROGRAM with no
/// variable and one initial step, `Start`, with no action and no transition (a chart has
/// exactly one initial step, IEC 61131-3 ed. 3 §6.7.2). Each compiles to a program that does
/// nothing.
fn template(name: &str, language: &str, now: std::time::SystemTime) -> String {
    if !is_tc6(language) {
        return format!(
            "PROGRAM {name}\nVAR_INPUT\nEND_VAR\nVAR_OUTPUT\nEND_VAR\nVAR\nEND_VAR\n\nEND_PROGRAM\n"
        );
    }
    let body = if language == "fbd" {
        "          <FBD/>\n"
    } else if language == "sfc" {
        r#"          <SFC>
            <step localId="1" name="Start" initialStep="true">
              <position x="0" y="0"/>
              <connectionPointOut formalParameter=""/>
            </step>
          </SFC>
"#
    } else {
        r#"          <LD>
            <leftPowerRail localId="1">
              <position x="0" y="0"/>
              <connectionPointOut formalParameter=""/>
            </leftPowerRail>
            <rightPowerRail localId="2">
              <position x="400" y="0"/>
            </rightPowerRail>
          </LD>
"#
    };
    // The schema requires a file header: who wrote it is not known, so it is left empty; the
    // product, its version and the time are this module's own.
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <fileHeader companyName="" productName="MythOS logic" productVersion="{version}" creationDateTime="{created}"/>
  <contentHeader name="{name}">
    <coordinateInfo>
      <fbd><scaling x="1" y="1"/></fbd>
      <ld><scaling x="1" y="1"/></ld>
      <sfc><scaling x="1" y="1"/></sfc>
    </coordinateInfo>
  </contentHeader>
  <types>
    <dataTypes/>
    <pous>
      <pou name="{name}" pouType="program">
        <interface/>
        <body>
{body}        </body>
      </pou>
    </pous>
  </types>
  <instances>
    <configurations/>
  </instances>
</project>
"#,
        version = env!("CARGO_PKG_VERSION"),
        created = date_time(now),
    )
}

/// `time` as an `xsd:dateTime` in UTC, to the second: `2026-11-18T09:30:00Z`.
fn date_time(time: std::time::SystemTime) -> String {
    let secs = time
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // The civil date of a day count from 1970-01-01 (Howard Hinnant's days_from_civil, inverted).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

// -----------------------------------------------------------------------------
// Checking settings and bindings
// -----------------------------------------------------------------------------

/// What can be said of an entry without compiling its source.
fn check_entry(entry: &ProgramEntry) -> Result<(), String> {
    if !LANGUAGES.iter().any(|(l, _)| *l == entry.language) {
        return Err(format!(
            "'{}' is not a language this module compiles; 'st' (Structured Text), 'ld' (Ladder Diagram), 'fbd' (Function Block Diagram) and 'sfc' (Sequential Function Chart) are",
            entry.language
        ));
    }
    if entry.interval_ms == 0 {
        return Err("interval_ms must be at least 1".to_string());
    }
    if entry.runtime.trim().is_empty() {
        return Err("the program names no runtime module".to_string());
    }
    if TargetProfile::by_name(&entry.target).is_none() {
        return Err(format!("'{}' is not a target profile", entry.target));
    }
    for o in &entry.outputs {
        if !(o.safe.is_number() || o.safe.is_boolean()) {
            return Err(format!(
                "the safe value of {} must be a number or a boolean",
                o.tag
            ));
        }
        if o.tag.contains(':') {
            return Err(format!(
                "{} is another device's tag; a program writes its own node's",
                o.tag
            ));
        }
    }
    for p in &entry.published {
        if !p.tag.starts_with("internal.") {
            return Err(format!(
                "a published tag must start with 'internal.', which {} does not",
                p.tag
            ));
        }
    }
    Ok(())
}

/// A variable a tag can be bound to: a BOOL, an integer or a real. It is decided by the type's
/// name, as the runtime module decides it: an array, a structure, an instance, a TIME and a
/// STRING each have another. A STRING is not bindable because a tag holds no text
/// (`mythos_plugin_sdk::SampleValue` is a BOOL or a number, and so is a tag's slot).
fn bindable(symbol: &Symbol) -> bool {
    matches!(
        symbol.ty.as_str(),
        "BOOL" | "SINT" | "INT" | "DINT" | "LINT" | "REAL" | "LREAL"
    )
}

/// Every binding against the compiled program's variables. Each problem names its binding.
fn check_bindings(entry: &ProgramEntry, symbols: &[Symbol]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut look = |kind: &str, variable: &str, tag: &str| -> Option<Symbol> {
        match symbols
            .iter()
            .find(|s| s.path.eq_ignore_ascii_case(variable))
        {
            None => {
                problems.push(format!(
                    "{kind} {tag}: the program has no variable {variable}"
                ));
                None
            }
            Some(s) if !bindable(s) => {
                // The tag bus carries numbers and BOOLs (`SampleValue`): a STRING has no tag
                // to be bound to, as an array or a structure has none.
                let text = if s.ty.starts_with("STRING[") {
                    " (a tag carries a number, never text)"
                } else {
                    ""
                };
                problems.push(format!(
                    "{kind} {tag}: {variable} is of type {}; only BOOL, integer and real variables can be bound to a tag{text}",
                    s.ty
                ));
                None
            }
            Some(s) => Some(s.clone()),
        }
    };
    let mut good_not_bool = Vec::new();
    let mut private = Vec::new();
    // A tag written into a variable: only one the program owns, or an input of a block.
    let mut written = |kind: &str, s: &Symbol, tag: &str| {
        if s.private {
            private.push(format!(
                "{kind} {tag}: {} is inside a function block instance and is not one of its inputs, so only the block writes it",
                s.path
            ));
        }
    };
    for b in &entry.inputs {
        if let Some(s) = look("input", &b.variable, &b.tag) {
            written("input", &s, &b.tag);
            if b.of == InputOf::Good && s.ty != "BOOL" {
                good_not_bool.push(format!(
                    "input {}: {} takes whether the tag is GOOD, so it must be a BOOL",
                    b.tag, b.variable
                ));
            }
        }
    }
    for b in &entry.outputs {
        look("output", &b.variable, &b.tag);
    }
    for b in &entry.published {
        if let Some(s) = look("published", &b.variable, &b.tag) {
            if b.writable {
                written("setpoint", &s, &b.tag);
            }
        }
    }
    problems.extend(good_not_bool);
    problems.extend(private);
    let mut seen = std::collections::BTreeSet::new();
    for o in &entry.outputs {
        if !seen.insert(o.tag.as_str()) {
            problems.push(format!("output {}: two variables are bound to it", o.tag));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for p in &entry.published {
        if !seen.insert(p.tag.as_str()) {
            problems.push(format!(
                "published {}: two variables are published as it",
                p.tag
            ));
        }
    }
    problems
}

/// A variable the front-end made for itself (ADR-013 §2.15, §2.16: an LD edge's memory, a
/// power or a value kept for monitoring, a network's flag, a jump), or a member of one: a part of its path begins with two underscores,
/// which IEC 61131-3 ed. 3 §6.1.2 forbids in every name a program declares.
fn hidden(path: &str) -> bool {
    path.split('.').any(|part| part.starts_with("__"))
}

/// What the workbench shows of a variable in the binding table.
fn variables(symbols: &[Symbol]) -> Vec<Value> {
    symbols
        .iter()
        .map(|s| {
            json!({
                "path": s.path,
                "type": s.ty,
                "section": s.section,
                "array": s.array,
                "bindable": bindable(s),
                // Bound only to be read (an output, a published tag that is not a setpoint):
                // only its function block writes it.
                "private": s.private,
                "hidden": hidden(&s.path),
            })
        })
        .collect()
}

/// Compile the program's `source`, written in its language, for `target`: from that one
/// source alone (see the top of this file).
fn compile_entry(
    entry: &ProgramEntry,
    source: &str,
    target: &TargetProfile,
) -> Result<mythos_logic_compiler::Compiled, Vec<Diagnostic>> {
    let file = |extension: &str| format!("{}.{extension}", entry.name);
    if is_tc6(&entry.language) {
        let name = file("xml");
        compile_sources(
            &Sources {
                st: None,
                tc6: &[(name.as_str(), source)],
            },
            target,
        )
    } else {
        let name = file("st");
        compile_sources(
            &Sources {
                st: Some((name.as_str(), source)),
                tc6: &[],
            },
            target,
        )
    }
}

/// Where the editor reads the PROGRAM's diagram live (`mythos_logic_compiler::Probe`): for
/// each output of an element and each junction of links, the hidden variable assigned when it
/// is evaluated in the scan (or on: always ON while its network runs), and for each network the
/// variable that says whether it ran in the last scan. In a Ladder Diagram that is power: what
/// is not listed carries none (a value that is not a BOOL), and is shown without a state. In a
/// Function Block Diagram it is every output's value, of whatever type; `ran`, where given,
/// names the BOOL that is TRUE where the value was computed in the scan (ADR-013 §2.16).
///
/// A Sequential Function Chart's are in two lists, so that what reads the first (`chart`, its
/// steps, transitions and actions) reads it as it was before LD and FBD bodies (ADR-013 §2.17):
/// `bodies` gives the probes of its actions and conditions written in LD or FBD, each in LD's or
/// FBD's shape with `body`, which body it is in.
fn probes(compiled: &mythos_logic_compiler::Compiled, bodies: bool) -> Vec<Value> {
    compiled
        .probes
        .iter()
        .filter(|p| p.pou.eq_ignore_ascii_case(&compiled.name))
        .filter(|p| p.body.is_some() == bodies)
        .map(|p| {
            let mut entry = match &p.at {
                At::Output(output) => json!({"local_id": p.local_id, "output": output}),
                At::Input(input) => json!({"local_id": p.local_id, "input": input}),
                At::Network(members) => json!({"local_id": p.local_id, "network": members}),
                At::Step => json!({"local_id": p.local_id, "step": "X"}),
                At::StepTime => json!({"local_id": p.local_id, "step": "T"}),
                At::Transition => json!({"local_id": p.local_id, "transition": true}),
                At::Action => json!({"local_id": p.local_id, "action": "Q"}),
            };
            match &p.holds {
                Holds::On => entry["on"] = json!(true),
                Holds::Variable { path, negated } => {
                    entry["variable"] = json!(path);
                    entry["negated"] = json!(negated);
                }
                Holds::Guarded { path, ran } => {
                    entry["variable"] = json!(path);
                    entry["negated"] = json!(false);
                    entry["ran"] = json!(ran);
                }
            }
            if let Some(body) = &p.body {
                let mut of = match &body.of {
                    BodyOf::Action(name) => json!({"action": name}),
                    BodyOf::Transition(name) => json!({"transition": name}),
                    BodyOf::Inline(id) => json!({"inline": id}),
                    BodyOf::Wired(id) => json!({"wired": id}),
                };
                of["language"] = json!(if body.ladder { "ld" } else { "fbd" });
                entry["body"] = of;
            }
            entry
        })
        .collect()
}

/// What identifies a build: its code and everything that ties it to the plant. A check gives
/// the same, so an editor knows whether the text it shows is what a runtime runs.
fn version_of(entry: &ProgramEntry, compiled: &mythos_logic_compiler::Compiled) -> String {
    let mut hash = Sha256::new();
    hash.update(&compiled.bytecode);
    hash.update(entry.interval_ms.to_le_bytes());
    hash.update(
        serde_json::to_vec(&(&entry.inputs, &entry.outputs, &entry.published)).unwrap_or_default(),
    );
    hex::encode(&hash.finalize()[..6])
}

/// Compile `source` for the entry's target: the answer of `check`.
fn check(entry: &ProgramEntry, source: &str) -> Value {
    let target = match TargetProfile::by_name(&entry.target) {
        Some(target) => target,
        None => {
            // Not about a place in the source: in the shape of the compiler's own, so
            // whatever reads diagnostics reads one kind.
            return json!({
                "ok": false,
                "diagnostics": [Diagnostic::whole(format!(
                    "'{}' is not a target profile",
                    entry.target
                ))],
            });
        }
    };
    match compile_entry(entry, source, &target) {
        Ok(compiled) => {
            let problems = check_bindings(entry, &compiled.symbols);
            let mut answer = json!({
                "ok": true,
                "diagnostics": [],
                "program": compiled.name,
                // The version a build of this text with these settings would have.
                "version": version_of(entry, &compiled),
                "variables": variables(&compiled.symbols),
                "code_size": compiled.code_size,
                "memory_size": compiled.memory_size,
                "binding_problems": problems,
            });
            match entry.language.as_str() {
                "ld" => answer["power"] = Value::Array(probes(&compiled, false)),
                "fbd" => answer["values"] = Value::Array(probes(&compiled, false)),
                "sfc" => {
                    answer["chart"] = Value::Array(probes(&compiled, false));
                    answer["bodies"] = Value::Array(probes(&compiled, true));
                }
                _ => {}
            }
            answer
        }
        Err(diagnostics) => json!({ "ok": false, "diagnostics": diagnostics }),
    }
}

/// Compile the saved source and pack it as the runtime instance the core loads.
fn build(
    entry: &ProgramEntry,
    source: &str,
    registered: Option<&[String]>,
) -> Result<Value, Refusal> {
    check_entry(entry).map_err(|e| error(400, e))?;
    let target = TargetProfile::by_name(&entry.target)
        .ok_or_else(|| error(400, format!("'{}' is not a target profile", entry.target)))?;
    let compiled = compile_entry(entry, source, &target).map_err(|diagnostics| {
        (
            400,
            json!({
                "error": "the program does not compile and was not built",
                "diagnostics": diagnostics,
            }),
        )
    })?;
    let problems = check_bindings(entry, &compiled.symbols);
    if !problems.is_empty() {
        return Err((
            400,
            json!({
                "error": "the program's bindings do not match its variables and it was not built",
                "binding_problems": problems,
            }),
        ));
    }

    let version = version_of(entry, &compiled);

    let program = RuntimeProgram {
        name: compiled.name.clone(),
        version: version.clone(),
        interval_ms: entry.interval_ms,
        code: hex::encode(&compiled.bytecode),
        symbols: serde_json::to_value(&compiled.symbols).map_err(|e| error(500, e))?,
        inputs: entry.inputs.clone(),
        outputs: entry.outputs.clone(),
        published: entry.published.clone(),
    };

    // A tag nothing provides now is said, not refused: the connection that provides it may
    // be configured later, and a program reading it sees it as not GOOD until then.
    let mut warnings = Vec::new();
    if let Some(registered) = registered {
        let known = |tag: &str| registered.iter().any(|r| r == tag);
        for b in &entry.inputs {
            if !known(&b.tag) {
                warnings.push(format!(
                    "input {} is not a tag of this controller now: {} will not take a value",
                    b.tag, b.variable
                ));
            }
        }
        for b in &entry.outputs {
            if !known(&b.tag) {
                warnings.push(format!(
                    "output {} is not a tag of this controller now: writes to it will fail",
                    b.tag
                ));
            }
        }
    }

    Ok(json!({
        "version": version,
        "warnings": warnings,
        "code_size": compiled.code_size,
        "memory_size": compiled.memory_size,
        "instance": {
            "id": entry.name,
            "module": entry.runtime,
            "run": entry.run,
            "program": program,
        },
    }))
}

// -----------------------------------------------------------------------------
// Routes
// -----------------------------------------------------------------------------

fn route(
    method: &str,
    path: &str,
    role: &str,
    tool: &str,
    summary: &str,
    body: Option<Value>,
    needs: &[&str],
) -> RouteDecl {
    RouteDecl {
        method: method.into(),
        path: path.into(),
        scope: RouteScope::Project,
        role: Some(role.into()),
        tool: tool.into(),
        summary: summary.into(),
        body,
        query: Default::default(),
        needs: needs.iter().map(|n| (*n).to_string()).collect(),
    }
}

fn settings_schema() -> Value {
    let binding = |extra: Value| {
        let mut properties = json!({
            "variable": { "type": "string", "description": "A variable of the program" },
            "tag": { "type": "string", "description": "A tag URI" }
        });
        if let (Some(p), Some(e)) = (properties.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        json!({ "type": "object", "required": ["variable", "tag"], "properties": properties })
    };
    json!({
        "type": "object",
        "required": ["interval_ms", "runtime", "target", "run"],
        "properties": {
            "interval_ms": { "type": "integer", "minimum": 1, "description": "The interval of its cyclic task" },
            "runtime": { "type": "string", "description": "The runtime module that runs it, for example vm" },
            "target": { "type": "string", "description": "The target profile it is compiled for, for example pc" },
            "run": { "type": "boolean", "description": "Whether it is put in RUN when it is loaded" },
            "inputs": { "type": "array", "description": "Tags read into variables at the start of each scan",
                "items": binding(json!({"of": {"type": "string", "enum": ["value", "good"],
                    "description": "value (default), or good: a BOOL that says whether the tag is current"}})) },
            "outputs": { "type": "array", "description": "Variables written to field tags",
                "items": binding(json!({"safe": {"type": ["number", "boolean"],
                    "description": "What the tag is written whenever the program is not running. Required"}})) },
            "published": { "type": "array", "description": "Variables published as tags starting with internal.",
                "items": binding(json!({"writable": {"type": "boolean",
                    "description": "Whether an operator may write it: a setpoint"}})) }
        }
    })
}

/// What `mythos-module-logic --manifest` prints.
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        protocol: PROTOCOL_VERSION,
        id: MODULE_ID.into(),
        name: "Logic".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        kind: ModuleKind::Service,
        provides: PROVIDES.into(),
        description: "Control programs in Structured Text, Ladder Diagram, Function Block Diagram or Sequential Function Chart: their source, their bindings to tags, check and build".into(),
        capabilities: vec![],
        config_schema: Value::Null,
        always_on: false,
        profile: Value::Null,
        routes: vec![
            route("GET", "/programs", "viewer", "list_logic_programs",
                "A project's control programs with their settings and bindings, each with its language: st (Structured Text), ld (Ladder Diagram), fbd (Function Block Diagram) or sfc (Sequential Function Chart).", None, &[]),
            route("POST", "/programs", "engineer", "create_logic_program",
                "Add a program to a project, in Structured Text (st, the default), Ladder Diagram (ld), Function Block Diagram (fbd) or Sequential Function Chart (sfc): an empty PROGRAM, not run when loaded until its settings say so. A Ladder Diagram starts as a PLCopen TC6 XML v2.01 document holding the PROGRAM, with the two power rails and no rung; a Function Block Diagram as one holding the PROGRAM with an empty FBD body; a Sequential Function Chart as one holding the PROGRAM with one initial step, Start, and nothing else.",
                Some(json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "A letter, then letters, digits or _; at most 64" },
                        "language": { "type": "string", "enum": ["st", "ld", "fbd", "sfc"], "default": "st",
                            "description": "st: Structured Text. ld: Ladder Diagram. fbd: Function Block Diagram. sfc: Sequential Function Chart. Each diagram is stored as a PLCopen TC6 XML v2.01 document. It cannot be changed later" },
                        "interval_ms": { "type": "integer", "minimum": 1, "default": 100 }
                    }
                })), &[]),
            route("GET", "/programs/:program", "viewer", "get_logic_program",
                "A program's settings, bindings and source: Structured Text, or for a Ladder Diagram (language ld), a Function Block Diagram (fbd) or a Sequential Function Chart (sfc) the PLCopen TC6 XML v2.01 document.", None, &[]),
            route("PUT", "/programs/:program/source", "engineer", "save_logic_source",
                "Replace a program's source, in its language: Structured Text, or for a Ladder Diagram (language ld), a Function Block Diagram (fbd) or a Sequential Function Chart (sfc) a PLCopen TC6 XML v2.01 document whose POUs include the PROGRAM (its body LD, FBD or SFC; one document may hold POUs of all three; a chart's actions in ST, LD or FBD, in its POU's actions or in an action block; its transitions' conditions in ST, LD or FBD, written in the transition, in the POU's transitions, or as an LD or FBD network drawn in the chart connected to the transition; a condition in LD or FBD writes the transition's name, by a coil or an output variable). Nothing is compiled or loaded: check_logic_program and build_logic_program do that.",
                Some(json!({
                    "type": "object",
                    "required": ["source"],
                    "properties": { "source": { "type": "string", "description": "Structured Text, or for a Ladder Diagram, a Function Block Diagram or a Sequential Function Chart the TC6 XML document" } }
                })), &[]),
            route("PUT", "/programs/:program/settings", "engineer", "save_logic_settings",
                "Replace a program's settings and its bindings to tags.",
                Some(settings_schema()), &[]),
            route("DELETE", "/programs/:program", "engineer", "delete_logic_program",
                "Remove a program and its source from a project. A runtime that already runs it is not touched.",
                None, &[]),
            route("POST", "/programs/:program/check", "viewer", "check_logic_program",
                "Compile a program, Structured Text, Ladder Diagram, Function Block Diagram or Sequential Function Chart, without building it: every mistake in it as a diagnostic with the range it is about (line, col, end_line, end_col, counted from 1, the end exclusive; all 0 when it is about the whole program; in a diagram the range is the element's tag in the document and 'element' names it: pou, local_id, x, y, or the interface's variable, or a chart's action or transition by name, or ('inline') the local_id of the action or transition of the chart whose inline body in LD or FBD the element is in, and the characters of its text, with the line of the text when it has several), or its variables (hidden: made by the diagram front-end for itself), what its bindings get wrong and the version a build of it would have. A Ladder Diagram that compiles also gives 'power', to watch for power flow: for each output of an element that carries power ('output') and each input where links join ('input'), the hidden variable assigned when it is evaluated in the scan (or on: ON whenever its network runs); for each network ('network': its elements' local_ids), the variable that is TRUE when it ran in the last scan. A Function Block Diagram that compiles gives 'values' of the same shape: for every output of every element (a block's by its formal parameter, ENO among them; '' for a variable's), the hidden variable holding the value it gave when it was evaluated in the scan, of the output's own type (or on: ENO always TRUE), with 'ran' where the value is computed only where that BOOL is TRUE (a function under EN); for each network the variable that is TRUE when it ran. A Sequential Function Chart that compiles gives 'chart' of the same shape: for each step ('step': 'X') the variable that is TRUE while it is active and ('step': 'T') its elapsed time, a TIME; for each transition ('transition': true) the variable holding its condition, with 'ran', the variable that is TRUE when the transition was enabled and its condition evaluated in the last scan; for each action of an action block ('action': 'Q', by the action's local_id) the variable holding the state of the action it names. A chart also gives 'bodies': for every action and transition condition written in LD or FBD, the entries of its 'power' (LD) or 'values' (FBD), each with 'body', the body it is in: {'action': name} (an action of the POU), {'transition': name} (a transition of the POU), {'inline': local_id} (written in the action of an action block or in the transition of that local_id), or {'wired': local_id} (a network drawn in the chart that gives that transition its condition), with 'language' ld or fbd; a body's elements' local_ids are its own, and its networks' variables are FALSE where the body did not run in the scan (an action whose Q was FALSE, a transition not enabled). With 'source' it checks that text instead of the saved one.",
                Some(json!({
                    "type": "object",
                    "properties": { "source": { "type": "string" } }
                })), &[]),
            route("POST", "/programs/:program/build", "engineer", "build_logic_program",
                "Compile the saved source, Structured Text, Ladder Diagram, Function Block Diagram or Sequential Function Chart, and pack it as a runtime instance. The answer's 'instance' is what save_runtimes_config takes in its 'instances' (replace the one with the same id); nothing is loaded by this call.",
                None, &["tags"]),
        ],
    }
}

fn reply(result: Result<Value, Refusal>, status: u16) -> HttpReply {
    match result {
        Ok(body) => HttpReply::json(status, body),
        Err((status, body)) => HttpReply::json(status, body),
    }
}

fn body<T: for<'de> Deserialize<'de>>(call: &HttpCall) -> Result<T, Refusal> {
    serde_json::from_value(call.body.clone().unwrap_or(Value::Null))
        .map_err(|e| error(400, format!("the body is not what this route takes: {e}")))
}

#[derive(Deserialize)]
struct Create {
    name: String,
    #[serde(default = "structured_text")]
    language: String,
    #[serde(default)]
    interval_ms: Option<u64>,
}

#[derive(Deserialize)]
struct SourceBody {
    source: String,
}

#[derive(Deserialize, Default)]
struct CheckBody {
    #[serde(default)]
    source: Option<String>,
}

/// The settings a client sends: an entry without what it cannot change.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    interval_ms: u64,
    runtime: String,
    target: String,
    run: bool,
    #[serde(default)]
    inputs: Vec<InputBinding>,
    #[serde(default)]
    outputs: Vec<OutputBinding>,
    #[serde(default)]
    published: Vec<PublishedBinding>,
}

/// The logic service. Writes are serialized: each is a read-modify-write of one file, and the
/// server forwards requests concurrently.
#[derive(Default)]
pub struct Logic {
    writes: Mutex<()>,
}

impl Logic {
    fn handle_in(&self, call: &HttpCall, data: &Path, events: &Events) -> HttpReply {
        let project = call.project.clone().unwrap_or_default();
        let name = call.params.get("program").map(String::as_str).unwrap_or("");
        let changed = |what: &str, name: &str| {
            events.send(
                PROGRAMS_CHANGED_EVENT,
                json!({ "project": project, "program": name, "what": what }),
            );
        };
        match (call.method.as_str(), call.route.as_str()) {
            ("GET", "/programs") => reply(load(data).map(|p| json!({ "programs": p.programs })), 200),
            ("POST", "/programs") => reply(
                body::<Create>(call).and_then(|b| {
                    if !valid_name(&b.name) {
                        return Err(error(
                            400,
                            format!("'{}' is not a program name: a letter, then letters, digits or _, at most 64", b.name),
                        ));
                    }
                    let mut programs = load(data)?;
                    if programs.programs.iter().any(|p| p.name.eq_ignore_ascii_case(&b.name)) {
                        return Err(error(409, format!("the project already has a program '{}'", b.name)));
                    }
                    let entry = ProgramEntry {
                        name: b.name.clone(),
                        language: b.language.clone(),
                        interval_ms: b.interval_ms.unwrap_or(100),
                        runtime: "vm".into(),
                        target: "pc".into(),
                        // A new program does not start by itself: that is said in its settings.
                        run: false,
                        inputs: vec![],
                        outputs: vec![],
                        published: vec![],
                    };
                    check_entry(&entry).map_err(|e| error(400, e))?;
                    let source = template(&b.name, &entry.language, std::time::SystemTime::now());
                    write_atomic(&source_path(data, &entry), source.as_bytes())?;
                    programs.programs.push(entry.clone());
                    save(data, &programs)?;
                    changed("created", &b.name);
                    Ok(json!({ "program": entry, "source": source }))
                }),
                201,
            ),
            ("GET", "/programs/:program") => reply(
                load(data).and_then(|p| {
                    let entry = find(&p, name)?.clone();
                    let source = read_source(data, &entry)?;
                    Ok(json!({ "program": entry, "source": source }))
                }),
                200,
            ),
            ("PUT", "/programs/:program/source") => reply(
                body::<SourceBody>(call).and_then(|b| {
                    let entry = find(&load(data)?, name)?.clone();
                    write_atomic(&source_path(data, &entry), b.source.as_bytes())?;
                    changed("source", name);
                    Ok(json!({ "saved": true }))
                }),
                200,
            ),
            ("PUT", "/programs/:program/settings") => reply(
                body::<Settings>(call).and_then(|s| {
                    let mut programs = load(data)?;
                    let at = programs
                        .programs
                        .iter()
                        .position(|p| p.name == name)
                        .ok_or_else(|| error(404, format!("the project has no program '{name}'")))?;
                    let entry = ProgramEntry {
                        name: name.to_string(),
                        language: programs.programs[at].language.clone(),
                        interval_ms: s.interval_ms,
                        runtime: s.runtime,
                        target: s.target,
                        run: s.run,
                        inputs: s.inputs,
                        outputs: s.outputs,
                        published: s.published,
                    };
                    check_entry(&entry).map_err(|e| error(400, e))?;
                    programs.programs[at] = entry.clone();
                    save(data, &programs)?;
                    changed("settings", name);
                    Ok(json!({ "program": entry }))
                }),
                200,
            ),
            ("DELETE", "/programs/:program") => reply(
                load(data).and_then(|mut programs| {
                    let entry = find(&programs, name)?.clone();
                    programs.programs.retain(|p| p.name != name);
                    save(data, &programs)?;
                    let _ = std::fs::remove_file(source_path(data, &entry));
                    changed("deleted", name);
                    Ok(json!({ "deleted": name }))
                }),
                200,
            ),
            ("POST", "/programs/:program/check") => reply(
                load(data).and_then(|p| {
                    let entry = find(&p, name)?;
                    let given = match &call.body {
                        None | Some(Value::Null) => CheckBody::default(),
                        Some(_) => body::<CheckBody>(call)?,
                    };
                    let source = match given.source {
                        Some(source) => source,
                        None => read_source(data, entry)?,
                    };
                    Ok(check(entry, &source))
                }),
                200,
            ),
            ("POST", "/programs/:program/build") => reply(
                load(data).and_then(|p| {
                    let entry = find(&p, name)?;
                    build(entry, &read_source(data, entry)?, call.tags.as_deref())
                }),
                200,
            ),
            (method, route) => HttpReply::error(404, format!("the logic module has no route {method} {route}")),
        }
    }
}

#[async_trait]
impl Service for Logic {
    async fn handle(&self, call: HttpCall, events: &Events) -> HttpReply {
        let Some(data) = call.data_dir.as_ref().map(PathBuf::from) else {
            return HttpReply::error(500, "the server gave the logic module no data directory");
        };
        // A program's name becomes a file name: nothing but a valid name gets that far.
        if let Some(name) = call.params.get("program") {
            if !valid_name(name) {
                return HttpReply::error(400, format!("'{name}' is not a program name"));
            }
        }
        let _guard = self.writes.lock().await;
        self.handle_in(&call, &data, events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> ProgramEntry {
        serde_json::from_value(json!({
            "name": "Tank", "language": "st", "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true,
            "inputs": [{"variable": "level", "tag": "external.P.LT1.level"},
                       {"variable": "level_ok", "tag": "external.P.LT1.level", "of": "good"}],
            "outputs": [{"variable": "speed", "tag": "external.P.P1.speed", "safe": 0}],
            "published": [{"variable": "setpoint", "tag": "internal.P.Tank.setpoint", "writable": true}]
        }))
        .unwrap()
    }

    const TANK: &str = "PROGRAM Tank
        VAR_INPUT level : INT; level_ok : BOOL; setpoint : INT := 50; END_VAR
        VAR_OUTPUT speed : INT; END_VAR
        VAR t : TON; hist : ARRAY [1..4] OF INT; note : STRING[8]; END_VAR
        IF level_ok AND (level < setpoint) THEN speed := (setpoint - level) * 10; ELSE speed := 0; END_IF;
        END_PROGRAM";

    /// A Ladder Diagram of the tank (a TC6 document, ADR-013 §2.15): `run` through a contact
    /// that feeds two coils (a branch point, kept in `__LD_10`), and `speed := level * 10`
    /// through a MUL block.
    fn ladder(contact_variable: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Tank" pouType="program">
      <interface>
        <inputVars>
          <variable name="level"><type><INT/></type></variable>
          <variable name="run"><type><BOOL/></type></variable>
        </inputVars>
        <outputVars>
          <variable name="speed"><type><INT/></type></variable>
          <variable name="motor"><type><BOOL/></type></variable>
          <variable name="lamp"><type><BOOL/></type></variable>
        </outputVars>
      </interface>
      <body><LD>
        <leftPowerRail localId="1"><position x="0" y="0"/><connectionPointOut formalParameter=""/></leftPowerRail>
        <contact localId="10"><position x="40" y="20"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>{contact_variable}</variable></contact>
        <coil localId="11"><position x="200" y="20"/><connectionPointIn><connection refLocalId="10"/></connectionPointIn><connectionPointOut/><variable>motor</variable></coil>
        <coil localId="12" negated="true"><position x="200" y="50"/><connectionPointIn><connection refLocalId="10"/></connectionPointIn><connectionPointOut/><variable>lamp</variable></coil>
        <inVariable localId="20"><position x="40" y="100"/><connectionPointOut/><expression>level</expression></inVariable>
        <block localId="21" typeName="MUL"><position x="120" y="100"/>
          <inputVariables>
            <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="20"/></connectionPointIn></variable>
            <variable formalParameter="IN2"><connectionPointIn><expression>10</expression></connectionPointIn></variable>
          </inputVariables>
          <inOutVariables/>
          <outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables>
        </block>
        <outVariable localId="22"><position x="200" y="100"/><connectionPointIn><connection refLocalId="21"/></connectionPointIn><expression>speed</expression></outVariable>
        <rightPowerRail localId="2"><position x="400" y="0"/>
          <connectionPointIn><connection refLocalId="11"/></connectionPointIn>
          <connectionPointIn><connection refLocalId="12"/></connectionPointIn>
        </rightPowerRail>
      </LD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
"#
        )
    }

    fn ld_entry() -> ProgramEntry {
        serde_json::from_value(json!({
            "name": "Tank", "language": "ld", "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true,
            "inputs": [{"variable": "level", "tag": "external.P.LT1.level"}],
            "outputs": [{"variable": "speed", "tag": "external.P.P1.speed", "safe": 0}]
        }))
        .unwrap()
    }

    #[test]
    fn a_programs_list_written_before_languages_is_structured_text() {
        // The shape C3 wrote: no language. It is read, and every program in it is ST.
        let old: Programs = serde_json::from_value(json!({"programs": [{
            "name": "Tank", "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true
        }]}))
        .expect("a list without a language is read");
        assert_eq!(old.programs[0].language, "st");
        assert_eq!(
            source_path(Path::new("/d"), &old.programs[0]),
            Path::new("/d/programs/Tank.st")
        );
        assert_eq!(
            source_path(Path::new("/d"), &ld_entry()),
            Path::new("/d/programs/Tank.xml")
        );
        // Written back, it says so.
        assert_eq!(
            serde_json::to_value(&old).unwrap()["programs"][0]["language"],
            "st"
        );
    }

    #[test]
    fn a_new_ladder_diagram_is_a_tc6_document_that_compiles_to_a_program_doing_nothing() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_795_000_000);
        assert_eq!(date_time(at), "2026-11-18T11:06:40Z");
        let source = template("Tank", "ld", at);
        assert!(
            source.contains(r#"<pou name="Tank" pouType="program">"#),
            "{source}"
        );
        assert!(source.contains(r#"creationDateTime="2026-11-18T11:06:40Z""#));
        let checked = check(&ld_entry(), &source);
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["program"], "Tank");
        assert_eq!(checked["variables"], json!([]));
        // Nothing connects to the rails: nothing carries power.
        assert_eq!(checked["power"], json!([]));
        // An ST program's check says nothing of power.
        assert!(check(&entry(), TANK).get("power").is_none());
        assert_eq!(template("Tank", "st", at), template("Tank", "xx", at));
    }

    #[test]
    fn a_ladder_diagram_is_checked_with_its_variables_and_where_its_power_is_held() {
        let checked = check(&ld_entry(), &ladder("run"));
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["binding_problems"], json!([]), "{checked}");
        let vars = checked["variables"].as_array().unwrap();
        let var = |path: &str| {
            vars.iter()
                .find(|v| v["path"] == path)
                .unwrap_or_else(|| panic!("{path} in {vars:?}"))
        };
        assert_eq!(var("level")["hidden"], false);
        assert_eq!(var("motor")["bindable"], true);
        // The contact's power and the rung's flag, kept by the front-end: hidden, and private.
        for kept in ["__LD_10", "__NET_10"] {
            assert_eq!(var(kept)["hidden"], true);
            assert_eq!(var(kept)["private"], true);
        }
        // Each element's power, and each network's flag; the MUL rung carries values, no power.
        assert_eq!(
            checked["power"],
            json!([
                {"local_id": 1, "output": "", "on": true},
                {"local_id": 10, "output": "", "variable": "__LD_10", "negated": false},
                {"local_id": 10, "network": [10, 11, 12], "variable": "__NET_10", "negated": false},
                {"local_id": 11, "output": "", "variable": "__LD_11", "negated": false},
                {"local_id": 12, "output": "", "variable": "__LD_12", "negated": false},
                {"local_id": 20, "network": [20, 21, 22], "variable": "__NET_20", "negated": false},
            ])
        );
    }

    #[test]
    fn a_ladder_diagnostic_names_its_element_and_where_it_is_drawn() {
        let checked = check(&ld_entry(), &ladder("runn"));
        assert_eq!(checked["ok"], false, "{checked}");
        let d = &checked["diagnostics"][0];
        assert_eq!(d["message"], "runn is not declared", "{checked}");
        assert_eq!(
            d["element"],
            json!({"pou": "Tank", "local_id": 10, "x": 40.0, "y": 20.0,
                   "text": {"field": "variable", "col": 1, "end_col": 5}}),
            "{checked}"
        );
        // Its line is the element's tag in the document; one source names none.
        assert_eq!(d["line"], 18, "{checked}");
        assert!(d.get("source").is_none(), "{checked}");
        // Not compiled as Structured Text: that would say something of line 1.
        assert_eq!(checked["diagnostics"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_ladder_diagram_is_built_into_a_runtime_instance_like_any_program() {
        let built = build(&ld_entry(), &ladder("run"), None).unwrap();
        assert_eq!(built["instance"]["id"], "Tank");
        assert_eq!(built["instance"]["program"]["name"], "Tank");
        let symbols = built["instance"]["program"]["symbols"].as_array().unwrap();
        assert!(symbols.iter().any(|s| s["path"] == "__LD_10"));
        // A check of the same text gives the version its build has.
        assert_eq!(
            check(&ld_entry(), &ladder("run"))["version"],
            built["version"]
        );
        // The same program in Structured Text is another build.
        let st = build(&entry(), TANK, None).unwrap();
        assert_ne!(st["version"], built["version"]);
        let (status, body) = build(&ld_entry(), &ladder("runn"), None).unwrap_err();
        assert_eq!(status, 400);
        assert_eq!(body["diagnostics"][0]["element"]["local_id"], 10);
        // An ST source given to an LD program is read as XML, and is not XML.
        let (_, body) = build(&ld_entry(), TANK, None).unwrap_err();
        assert_eq!(
            body["diagnostics"],
            json!([{"line": 1, "col": 1, "end_line": 5, "end_col": 32,
                    "message": "text outside the root element"}]),
            "{body}"
        );
    }

    /// The tank as a Function Block Diagram (ADR-013 §2.16): `speed := level * 10` through a
    /// MUL block, and `motor := run AND NOT stop`, the NOT a negated input of the AND.
    fn diagram(level_variable: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Tank" pouType="program">
      <interface>
        <inputVars>
          <variable name="level"><type><INT/></type></variable>
          <variable name="run"><type><BOOL/></type></variable>
          <variable name="stop"><type><BOOL/></type></variable>
        </inputVars>
        <outputVars>
          <variable name="speed"><type><INT/></type></variable>
          <variable name="motor"><type><BOOL/></type></variable>
        </outputVars>
      </interface>
      <body><FBD>
        <inVariable localId="1"><position x="0" y="0"/><connectionPointOut/><expression>{level_variable}</expression></inVariable>
        <block localId="2" typeName="MUL"><position x="120" y="0"/>
          <inputVariables>
            <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="1"/></connectionPointIn></variable>
            <variable formalParameter="IN2"><connectionPointIn><expression>10</expression></connectionPointIn></variable>
          </inputVariables>
          <inOutVariables/>
          <outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables>
        </block>
        <outVariable localId="3"><position x="240" y="0"/><connectionPointIn><connection refLocalId="2"/></connectionPointIn><expression>speed</expression></outVariable>
        <inVariable localId="10"><position x="0" y="100"/><connectionPointOut/><expression>run</expression></inVariable>
        <inVariable localId="11"><position x="0" y="140"/><connectionPointOut/><expression>stop</expression></inVariable>
        <block localId="12" typeName="AND"><position x="120" y="100"/>
          <inputVariables>
            <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="10"/></connectionPointIn></variable>
            <variable formalParameter="IN2" negated="true"><connectionPointIn><connection refLocalId="11"/></connectionPointIn></variable>
          </inputVariables>
          <inOutVariables/>
          <outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables>
        </block>
        <outVariable localId="13"><position x="240" y="100"/><connectionPointIn><connection refLocalId="12"/></connectionPointIn><expression>motor</expression></outVariable>
      </FBD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
"#
        )
    }

    fn fbd_entry() -> ProgramEntry {
        let mut e = ld_entry();
        e.language = "fbd".into();
        e
    }

    #[test]
    fn a_new_function_block_diagram_is_a_tc6_document_that_compiles_to_a_program_doing_nothing() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_795_000_000);
        let source = template("Tank", "fbd", at);
        assert!(source.contains("<FBD/>"), "{source}");
        assert!(!source.contains("PowerRail"), "{source}");
        assert!(source.contains(r#"creationDateTime="2026-11-18T11:06:40Z""#));
        let checked = check(&fbd_entry(), &source);
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["program"], "Tank");
        assert_eq!(checked["variables"], json!([]));
        assert_eq!(checked["values"], json!([]));
        // An FBD check says nothing of power, an LD one nothing of values.
        assert!(checked.get("power").is_none());
        assert!(check(&ld_entry(), &template("Tank", "ld", at))
            .get("values")
            .is_none());
        assert_eq!(
            source_path(Path::new("/d"), &fbd_entry()),
            Path::new("/d/programs/Tank.xml")
        );
    }

    #[test]
    fn a_function_block_diagram_is_checked_with_where_each_value_is_kept() {
        let checked = check(&fbd_entry(), &diagram("level"));
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["binding_problems"], json!([]), "{checked}");
        let vars = checked["variables"].as_array().unwrap();
        let var = |path: &str| {
            vars.iter()
                .find(|v| v["path"] == path)
                .unwrap_or_else(|| panic!("{path} in {vars:?}"))
        };
        // Each value kept in its own type, hidden and private: a number too.
        assert_eq!(var("__FBD_2")["type"], "INT");
        assert_eq!(var("__FBD_12")["type"], "BOOL");
        for kept in [
            "__FBD_1", "__FBD_2", "__FBD_10", "__FBD_11", "__FBD_12", "__NET_1", "__NET_10",
        ] {
            assert_eq!(var(kept)["hidden"], true, "{kept}");
            assert_eq!(var(kept)["private"], true, "{kept}");
        }
        assert_eq!(
            checked["values"],
            json!([
                {"local_id": 1, "output": "", "variable": "__FBD_1", "negated": false},
                {"local_id": 1, "network": [1, 2, 3], "variable": "__NET_1", "negated": false},
                {"local_id": 2, "output": "OUT", "variable": "__FBD_2", "negated": false},
                {"local_id": 10, "output": "", "variable": "__FBD_10", "negated": false},
                {"local_id": 10, "network": [10, 12, 13, 11], "variable": "__NET_10", "negated": false},
                {"local_id": 11, "output": "", "variable": "__FBD_11", "negated": false},
                {"local_id": 12, "output": "OUT", "variable": "__FBD_12", "negated": false},
            ])
        );
        assert!(checked.get("power").is_none());
    }

    #[test]
    fn a_function_block_diagram_diagnostic_names_its_element_and_its_build_runs_like_any() {
        let checked = check(&fbd_entry(), &diagram("levle"));
        assert_eq!(checked["ok"], false, "{checked}");
        let d = &checked["diagnostics"][0];
        assert_eq!(d["message"], "levle is not declared", "{checked}");
        assert_eq!(
            d["element"],
            json!({"pou": "Tank", "local_id": 1, "x": 0.0, "y": 0.0,
                   "text": {"field": "expression", "col": 1, "end_col": 6}}),
            "{checked}"
        );
        let built = build(&fbd_entry(), &diagram("level"), None).unwrap();
        assert_eq!(built["instance"]["program"]["name"], "Tank");
        let symbols = built["instance"]["program"]["symbols"].as_array().unwrap();
        assert!(symbols.iter().any(|s| s["path"] == "__FBD_2"));
        assert_eq!(
            check(&fbd_entry(), &diagram("level"))["version"],
            built["version"]
        );
        // The same document as an LD program compiles the same: the reader is one.
        assert_eq!(
            build(&ld_entry(), &diagram("level"), None).unwrap()["version"],
            built["version"]
        );
    }

    fn sfc_entry() -> ProgramEntry {
        let mut e = ld_entry();
        e.language = "sfc".into();
        e
    }

    /// Tank as a chart: Filling while the level is below `limit`, then Pumping, the pump's speed
    /// the action of a step of statements; back when it is low.
    fn chart(limit: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
  <types><dataTypes/><pous>
    <pou name="Tank" pouType="program">
      <interface>
        <inputVars><variable name="level"><type><INT/></type></variable></inputVars>
        <outputVars>
          <variable name="speed"><type><INT/></type></variable>
          <variable name="valve"><type><BOOL/></type></variable>
        </outputVars>
      </interface>
      <actions>
        <action name="Pump"><body><ST><xhtml:p>speed := 50;</xhtml:p></ST></body></action>
      </actions>
      <body><SFC>
        <step localId="1" name="Filling" initialStep="true"><position x="100" y="0"/></step>
        <transition localId="2"><position x="100" y="40"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn>
          <condition><inline name=""><ST><xhtml:p>level &gt;= {limit}</xhtml:p></ST></inline></condition></transition>
        <step localId="3" name="Pumping"><position x="100" y="80"/><connectionPointIn><connection refLocalId="2"/></connectionPointIn></step>
        <transition localId="4"><position x="100" y="120"/><connectionPointIn><connection refLocalId="3"/></connectionPointIn>
          <condition><inline name=""><ST><xhtml:p>level &lt; 10</xhtml:p></ST></inline></condition></transition>
        <jumpStep localId="5" targetName="Filling"><position x="100" y="160"/><connectionPointIn><connection refLocalId="4"/></connectionPointIn></jumpStep>
        <actionBlock localId="6"><position x="200" y="0"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn>
          <action localId="7" qualifier="N"><relPosition x="0" y="0"/><reference name="valve"/></action></actionBlock>
        <actionBlock localId="8"><position x="200" y="80"/><connectionPointIn><connection refLocalId="3"/></connectionPointIn>
          <action localId="9" qualifier="N"><relPosition x="0" y="0"/><reference name="Pump"/></action></actionBlock>
      </SFC></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
"#
        )
    }

    #[test]
    fn a_new_sequential_function_chart_is_a_tc6_document_with_one_initial_step_doing_nothing() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_795_000_000);
        let source = template("Tank", "sfc", at);
        assert!(
            source.contains(r#"<step localId="1" name="Start" initialStep="true">"#),
            "{source}"
        );
        let checked = check(&sfc_entry(), &source);
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["program"], "Tank");
        // Its one step, a variable of the program by its name, private: only the chart writes it.
        let vars = checked["variables"].as_array().unwrap();
        let start = vars.iter().find(|v| v["path"] == "Start.X").unwrap();
        assert_eq!(start["private"], true, "{checked}");
        assert_eq!(
            checked["chart"],
            json!([
                {"local_id": 1, "step": "X", "variable": "Start.X", "negated": false},
                {"local_id": 1, "step": "T", "variable": "Start.T", "negated": false},
            ])
        );
        assert!(checked.get("power").is_none() && checked.get("values").is_none());
        assert_eq!(
            source_path(Path::new("/d"), &sfc_entry()),
            Path::new("/d/programs/Tank.xml")
        );
    }

    #[test]
    fn a_sequential_function_chart_is_checked_with_where_each_step_transition_and_action_is_kept() {
        let checked = check(&sfc_entry(), &chart("90"));
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["binding_problems"], json!([]), "{checked}");
        let chart = checked["chart"].as_array().unwrap();
        let has = |want: Value| assert!(chart.contains(&want), "{want} in {chart:?}");
        has(json!({"local_id": 3, "step": "X", "variable": "Pumping.X", "negated": false}));
        has(
            json!({"local_id": 2, "transition": true, "variable": "__SFC_2", "negated": false, "ran": "__SFC_2_EN"}),
        );
        has(json!({"local_id": 7, "action": "Q", "variable": "valve", "negated": false}));
        has(json!({"local_id": 9, "action": "Q", "variable": "Pump.Q", "negated": false}));
        let vars = checked["variables"].as_array().unwrap();
        let var = |path: &str| {
            vars.iter()
                .find(|v| v["path"] == path)
                .unwrap_or_else(|| panic!("{path}"))
        };
        assert_eq!(var("__SFC_2")["hidden"], true);
        assert_eq!(var("Pumping.T")["type"], "TIME");
        assert_eq!(var("Pumping.X")["private"], true);
    }

    #[test]
    fn a_chart_with_bodies_in_ld_and_fbd_gives_their_values_apart_from_its_steps() {
        // Pump in LD (its coil the valve), the condition out of Pumping in FBD written in the
        // transition, the condition out of Filling a rung drawn in the chart (ADR-013 §2.17).
        let source = chart("90")
            .replace(
                r#"<action name="Pump"><body><ST><xhtml:p>speed := 50;</xhtml:p></ST></body></action>"#,
                r#"<action name="Pump"><body><LD>
          <leftPowerRail localId="1"><position x="0" y="0"/><connectionPointOut/></leftPowerRail>
          <contact localId="2"><position x="100" y="0"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>Pumping.X</variable></contact>
          <coil localId="3"><position x="200" y="0"/><connectionPointIn><connection refLocalId="2"/></connectionPointIn><connectionPointOut/><variable>valve</variable></coil>
        </LD></body></action>"#,
            )
            .replace(
                r#"<condition><inline name=""><ST><xhtml:p>level &lt; 10</xhtml:p></ST></inline></condition>"#,
                r#"<condition><inline name="Low"><FBD>
          <inVariable localId="1"><position x="0" y="0"/><connectionPointOut/><expression>level &lt; 10</expression></inVariable>
          <outVariable localId="2"><position x="100" y="0"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><expression>Low</expression></outVariable>
        </FBD></inline></condition>"#,
            )
            .replace(
                r#"<condition><inline name=""><ST><xhtml:p>level &gt;= 90</xhtml:p></ST></inline></condition>"#,
                r#"<condition><connectionPointIn><connection refLocalId="21"/></connectionPointIn></condition>"#,
            )
            .replace(
                r#"<step localId="1" name="Filling" initialStep="true"><position x="100" y="0"/></step>"#,
                r#"<step localId="1" name="Filling" initialStep="true"><position x="100" y="0"/></step>
        <leftPowerRail localId="20"><position x="-100" y="40"/><connectionPointOut/></leftPowerRail>
        <contact localId="21"><position x="0" y="40"/><connectionPointIn><connection refLocalId="20"/></connectionPointIn><connectionPointOut/><variable>level &gt;= 90</variable></contact>"#,
            )
            .replace(
                r#"<reference name="valve"/>"#,
                r#"<reference name="Pump"/>"#,
            );
        let source = source.replacen(
            r#"<reference name="Pump"/>"#,
            r#"<inline><ST><xhtml:p>speed := 0;</xhtml:p></ST></inline>"#,
            1,
        );
        let checked = check(&sfc_entry(), &source);
        assert_eq!(checked["ok"], true, "{checked}");
        // The chart's own list as before: steps, transitions and actions only.
        let chart = checked["chart"].as_array().unwrap();
        assert!(chart.iter().all(|p| p.get("body").is_none()), "{checked}");
        let bodies = checked["bodies"].as_array().unwrap();
        let has = |want: Value| assert!(bodies.contains(&want), "{want} in {bodies:#?}");
        has(
            json!({"local_id": 2, "output": "", "variable": "__SFC_PUMP__LD_2", "negated": false,
                   "body": {"action": "Pump", "language": "ld"}}),
        );
        has(
            json!({"local_id": 2, "network": [2, 3], "variable": "__SFC_PUMP__NET_2", "negated": false,
                   "body": {"action": "Pump", "language": "ld"}}),
        );
        has(
            json!({"local_id": 1, "output": "", "variable": "__SFC_4__FBD_1", "negated": false,
                   "body": {"inline": 4, "language": "fbd"}}),
        );
        has(
            json!({"local_id": 21, "output": "", "variable": "__SFC_2__LD_21", "negated": false,
                   "body": {"wired": 2, "language": "ld"}}),
        );
        let vars = checked["variables"].as_array().unwrap();
        let hidden = vars
            .iter()
            .find(|v| v["path"] == "__SFC_PUMP__LD_2")
            .unwrap_or_else(|| panic!("{checked}"));
        assert_eq!(hidden["hidden"], true);
    }

    #[test]
    fn a_sequential_function_chart_diagnostic_names_its_element_and_its_build_runs_like_any() {
        let checked = check(&sfc_entry(), &chart("levle"));
        assert_eq!(checked["ok"], false, "{checked}");
        let d = &checked["diagnostics"][0];
        assert_eq!(d["message"], "levle is not declared", "{checked}");
        assert_eq!(
            d["element"],
            json!({"pou": "Tank", "local_id": 2, "x": 100.0, "y": 40.0,
                   "text": {"field": "condition", "col": 10, "end_col": 15, "line": 1}}),
            "{checked}"
        );
        let built = build(&sfc_entry(), &chart("90"), None).unwrap();
        assert_eq!(built["instance"]["program"]["name"], "Tank");
        assert_eq!(
            check(&sfc_entry(), &chart("90"))["version"],
            built["version"]
        );
    }

    #[test]
    fn a_name_is_a_letter_then_letters_digits_or_underscores() {
        for ok in ["Tank", "a", "Pump_2", "x9"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            "2pump",
            "_x",
            "tank 1",
            "a-b",
            "../x",
            "a/b",
            "a.st",
            &"x".repeat(65),
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn settings_are_refused_for_each_thing_they_can_get_wrong() {
        assert_eq!(check_entry(&entry()), Ok(()));
        let with = |change: &dyn Fn(&mut ProgramEntry)| {
            let mut e = entry();
            change(&mut e);
            check_entry(&e).unwrap_err()
        };
        assert!(with(&|e| e.language = "il".into()).contains("is not a language"));
        for language in ["ld", "fbd", "sfc"] {
            let mut e = entry();
            e.language = language.into();
            assert_eq!(check_entry(&e), Ok(()), "{language}");
        }
        assert!(with(&|e| e.interval_ms = 0).contains("at least 1"));
        assert!(with(&|e| e.runtime = " ".into()).contains("names no runtime module"));
        assert!(with(&|e| e.target = "plc-9000".into()).contains("is not a target profile"));
        assert!(
            with(&|e| e.outputs[0].safe = json!("off")).contains("must be a number or a boolean")
        );
        assert!(
            with(&|e| e.outputs[0].tag = "edge1:external.P.P1.speed".into())
                .contains("another device's tag")
        );
        assert!(
            with(&|e| e.published[0].tag = "external.P.Tank.setpoint".into())
                .contains("must start with 'internal.'")
        );
    }

    #[test]
    fn a_check_gives_the_compilers_diagnostics_or_the_variables_and_binding_problems() {
        let bad = check(
            &entry(),
            "PROGRAM Tank\nVAR a : INT; END_VAR\n  a := b;\nEND_PROGRAM",
        );
        assert_eq!(bad["ok"], false);
        // The whole list: one mistake, with where it starts and where it ends.
        assert_eq!(
            bad["diagnostics"],
            json!([{"line": 3, "col": 8, "end_line": 3, "end_col": 9,
                "message": "b is not declared"}])
        );
        // A target that does not exist is told in the same shape, about no place.
        let mut nowhere = entry();
        nowhere.target = "plc-9000".into();
        assert_eq!(
            check(&nowhere, TANK)["diagnostics"],
            json!([{"line": 0, "col": 0, "end_line": 0, "end_col": 0,
                "message": "'plc-9000' is not a target profile"}])
        );

        let good = check(&entry(), TANK);
        assert_eq!(good["ok"], true, "{good}");
        assert_eq!(good["binding_problems"], json!([]));
        let vars = good["variables"].as_array().unwrap();
        let var = |path: &str| {
            vars.iter()
                .find(|v| v["path"] == path)
                .unwrap_or_else(|| panic!("{path} in {vars:?}"))
        };
        assert_eq!(var("level")["type"], "INT");
        assert_eq!(var("level")["section"], "input");
        assert_eq!(var("level")["bindable"], true);
        assert_eq!(
            var("hist")["bindable"],
            false,
            "an array is not bound to one tag"
        );
        assert_eq!(var("t.ET")["bindable"], false, "nor a TIME");
        assert_eq!(var("note")["type"], "STRING[8]");
        assert_eq!(
            var("note")["bindable"],
            false,
            "nor a STRING: a tag holds no text"
        );

        let mut e = entry();
        e.inputs[0].variable = "flow".into();
        e.inputs[1].variable = "level".into();
        e.outputs[0].variable = "hist".into();
        e.published.push(e.published[0].clone());
        let wrong = check(&e, TANK);
        assert_eq!(
            wrong["binding_problems"],
            json!([
                "input external.P.LT1.level: the program has no variable flow",
                "output external.P.P1.speed: hist is of type ARRAY [1..4] OF INT; only BOOL, integer and real variables can be bound to a tag",
                "input external.P.LT1.level: level takes whether the tag is GOOD, so it must be a BOOL",
                "published internal.P.Tank.setpoint: two variables are published as it"
            ])
        );
        let mut e = entry();
        e.outputs[0].variable = "note".into();
        assert_eq!(
            check(&e, TANK)["binding_problems"],
            json!([
                "output external.P.P1.speed: note is of type STRING[8]; only BOOL, integer and real variables can be bound to a tag (a tag carries a number, never text)"
            ])
        );
    }

    #[test]
    fn a_tag_never_writes_what_only_a_function_block_writes() {
        let good = check(&entry(), TANK);
        let vars = good["variables"].as_array().unwrap();
        let private = |path: &str| {
            vars.iter()
                .find(|v| v["path"] == path)
                .unwrap_or_else(|| panic!("{path} in {vars:?}"))["private"]
                .clone()
        };
        // An input of the block is written from outside it; its outputs and its own variables
        // are not.
        assert_eq!(private("t.IN"), false);
        assert_eq!(private("t.Q"), true);
        assert_eq!(private("t.RUNNING"), true);
        assert_eq!(private("level"), false);

        let mut e = entry();
        e.inputs[0].variable = "t.RUNNING".into();
        e.inputs.push(
            serde_json::from_value(json!({"variable": "t.IN", "tag": "external.P.X.run"})).unwrap(),
        );
        e.published[0].variable = "t.Q".into();
        e.outputs[0].variable = "t.Q".into();
        assert_eq!(
            check(&e, TANK)["binding_problems"],
            json!([
                "input external.P.LT1.level: t.RUNNING is inside a function block instance and is not one of its inputs, so only the block writes it",
                "setpoint internal.P.Tank.setpoint: t.Q is inside a function block instance and is not one of its inputs, so only the block writes it"
            ]),
            "read, as an output or a published tag that is not a setpoint, it is bound"
        );
        e.published[0].writable = false;
        assert_eq!(check(&e, TANK)["binding_problems"], json!([
            "input external.P.LT1.level: t.RUNNING is inside a function block instance and is not one of its inputs, so only the block writes it"
        ]));
    }

    #[test]
    fn a_build_is_the_runtime_instance_and_its_version_follows_what_was_built() {
        let registered = vec!["external.P.LT1.level".to_string()];
        let built = build(&entry(), TANK, Some(&registered)).unwrap();
        let instance = &built["instance"];
        assert_eq!(instance["id"], "Tank");
        assert_eq!(instance["module"], "vm");
        assert_eq!(instance["run"], true);
        assert_eq!(instance["program"]["name"], "Tank");
        assert_eq!(instance["program"]["interval_ms"], 20);
        assert_eq!(instance["program"]["outputs"][0]["safe"], 0);
        assert!(!instance["program"]["code"].as_str().unwrap().is_empty());
        // The output's tag is not a tag of the controller now: said, not refused.
        assert_eq!(
            built["warnings"],
            json!(["output external.P.P1.speed is not a tag of this controller now: writes to it will fail"])
        );

        // The same again is the same version; a changed source or binding is another.
        let again = build(&entry(), TANK, None).unwrap();
        assert_eq!(again["version"], built["version"]);
        assert_eq!(
            again["warnings"],
            json!([]),
            "nothing is known of the tags: nothing is said"
        );
        let other_source = build(&entry(), &TANK.replace("* 10", "* 20"), None).unwrap();
        assert_ne!(other_source["version"], built["version"]);
        let mut e = entry();
        e.outputs[0].safe = json!(7);
        assert_ne!(build(&e, TANK, None).unwrap()["version"], built["version"]);

        // What does not compile, or binds what is not there, is not built.
        let (status, body) = build(&entry(), "PROGRAM Tank x := 1; END_PROGRAM", None).unwrap_err();
        assert_eq!(status, 400);
        assert_eq!(body["diagnostics"][0]["message"], "x is not declared");
        let mut e = entry();
        e.outputs[0].variable = "flow".into();
        let (status, body) = build(&e, TANK, None).unwrap_err();
        assert_eq!(status, 400);
        assert_eq!(
            body["binding_problems"],
            json!(["output external.P.P1.speed: the program has no variable flow"])
        );
    }
}
