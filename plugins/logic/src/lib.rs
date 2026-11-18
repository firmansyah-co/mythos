//! MythOS control logic authoring, as a service module (ADR-013 §2.6).
//!
//! A project's programs live in the module's data directory (`<project>/logic/`): the list
//! with each program's settings and its bindings to tags in `programs.json`, and each
//! program's source in `programs/<name>.st`. The module checks a program (the compiler's
//! diagnostics, each with the range of the source it is about, and its variables for the
//! binding table) and builds it:
//! compiled for its target, bindings checked against its variables, packed as the runtime
//! instance the core loads (`PUT /api/v1/projects/:id/runtimes`).
//!
//! It never talks to a runtime, and it does not load what it builds: a module cannot call the
//! core. The workbench, or an agent over MCP, takes the built instance and saves it into the
//! project's runtimes.

use async_trait::async_trait;
use mythos_logic_compiler::{compile, Diagnostic, Symbol, TargetProfile};
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
    /// The language of its source. `st` is the one there is.
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

fn source_path(data: &Path, name: &str) -> PathBuf {
    data.join(SOURCES_DIR).join(format!("{name}.st"))
}

fn read_source(data: &Path, name: &str) -> Result<String, Refusal> {
    let path = source_path(data, name);
    std::fs::read_to_string(&path)
        .map_err(|e| error(500, format!("the source of {name} cannot be read: {e}")))
}

fn find<'a>(programs: &'a Programs, name: &str) -> Result<&'a ProgramEntry, Refusal> {
    programs
        .programs
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| error(404, format!("the project has no program '{name}'")))
}

/// What a new program starts as: every section a program may have, empty.
fn template(name: &str) -> String {
    format!(
        "PROGRAM {name}\nVAR_INPUT\nEND_VAR\nVAR_OUTPUT\nEND_VAR\nVAR\nEND_VAR\n\nEND_PROGRAM\n"
    )
}

// -----------------------------------------------------------------------------
// Checking settings and bindings
// -----------------------------------------------------------------------------

/// What can be said of an entry without compiling its source.
fn check_entry(entry: &ProgramEntry) -> Result<(), String> {
    if entry.language != "st" {
        return Err(format!(
            "'{}' is not a language this module compiles; 'st' is",
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
            })
        })
        .collect()
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
    match compile(source, &target) {
        Ok(compiled) => {
            let problems = check_bindings(entry, &compiled.symbols);
            json!({
                "ok": true,
                "diagnostics": [],
                "program": compiled.name,
                "variables": variables(&compiled.symbols),
                "code_size": compiled.code_size,
                "memory_size": compiled.memory_size,
                "binding_problems": problems,
            })
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
    let compiled = compile(source, &target).map_err(|diagnostics| {
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

    // What identifies this build: the code and everything that ties it to the plant.
    let mut hash = Sha256::new();
    hash.update(&compiled.bytecode);
    hash.update(entry.interval_ms.to_le_bytes());
    hash.update(
        serde_json::to_vec(&(&entry.inputs, &entry.outputs, &entry.published)).unwrap_or_default(),
    );
    let version = hex::encode(&hash.finalize()[..6]);

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
        description: "Control programs: their source, their bindings to tags, check and build".into(),
        capabilities: vec![],
        config_schema: Value::Null,
        always_on: false,
        profile: Value::Null,
        routes: vec![
            route("GET", "/programs", "viewer", "list_logic_programs",
                "A project's control programs with their settings and bindings.", None, &[]),
            route("POST", "/programs", "engineer", "create_logic_program",
                "Add a program to a project: an empty PROGRAM, not run when loaded until its settings say so.",
                Some(json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "A letter, then letters, digits or _; at most 64" },
                        "interval_ms": { "type": "integer", "minimum": 1, "default": 100 }
                    }
                })), &[]),
            route("GET", "/programs/:program", "viewer", "get_logic_program",
                "A program's settings, bindings and source.", None, &[]),
            route("PUT", "/programs/:program/source", "engineer", "save_logic_source",
                "Replace a program's source. Nothing is compiled or loaded: check_logic_program and build_logic_program do that.",
                Some(json!({
                    "type": "object",
                    "required": ["source"],
                    "properties": { "source": { "type": "string", "description": "Structured Text" } }
                })), &[]),
            route("PUT", "/programs/:program/settings", "engineer", "save_logic_settings",
                "Replace a program's settings and its bindings to tags.",
                Some(settings_schema()), &[]),
            route("DELETE", "/programs/:program", "engineer", "delete_logic_program",
                "Remove a program and its source from a project. A runtime that already runs it is not touched.",
                None, &[]),
            route("POST", "/programs/:program/check", "viewer", "check_logic_program",
                "Compile a program without building it: every mistake in it as a diagnostic with the range it is about (line, col, end_line, end_col, counted from 1, the end exclusive; all 0 when it is about the whole program), or its variables and what its bindings get wrong. With 'source' it checks that text instead of the saved one.",
                Some(json!({
                    "type": "object",
                    "properties": { "source": { "type": "string" } }
                })), &[]),
            route("POST", "/programs/:program/build", "engineer", "build_logic_program",
                "Compile the saved source and pack it as a runtime instance. The answer's 'instance' is what save_runtimes_config takes in its 'instances' (replace the one with the same id); nothing is loaded by this call.",
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
                        language: "st".into(),
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
                    write_atomic(&source_path(data, &b.name), template(&b.name).as_bytes())?;
                    programs.programs.push(entry.clone());
                    save(data, &programs)?;
                    changed("created", &b.name);
                    Ok(json!({ "program": entry, "source": template(&b.name) }))
                }),
                201,
            ),
            ("GET", "/programs/:program") => reply(
                load(data).and_then(|p| {
                    let entry = find(&p, name)?.clone();
                    Ok(json!({ "program": entry, "source": read_source(data, name)? }))
                }),
                200,
            ),
            ("PUT", "/programs/:program/source") => reply(
                body::<SourceBody>(call).and_then(|b| {
                    find(&load(data)?, name)?;
                    write_atomic(&source_path(data, name), b.source.as_bytes())?;
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
                    find(&programs, name)?;
                    programs.programs.retain(|p| p.name != name);
                    save(data, &programs)?;
                    let _ = std::fs::remove_file(source_path(data, name));
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
                        None => read_source(data, name)?,
                    };
                    Ok(check(entry, &source))
                }),
                200,
            ),
            ("POST", "/programs/:program/build") => reply(
                load(data).and_then(|p| {
                    let entry = find(&p, name)?;
                    build(entry, &read_source(data, name)?, call.tags.as_deref())
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
        assert!(with(&|e| e.language = "ld".into()).contains("is not a language"));
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
