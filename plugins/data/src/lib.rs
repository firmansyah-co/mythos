//! MythOS user types and data blocks of a project, as a service module (ADR-014 §2.1, §2.7,
//! §2.8).
//!
//! A project's types and blocks live in the module's data directory (`<project>/data/`):
//!
//! ```text
//! types/RecipeData.st     one TYPE … END_TYPE declaring RecipeData and nothing else
//! blocks.json             {"blocks": [{"name": "Recipe", "type": "RecipeData", "retain": false}]}
//! ```
//!
//! **The files on disk always check.** This module is their only writer, and every write is
//! checked first with the compiler ([`mythos_logic_compiler::check_project`]) over the whole
//! state it would leave: one that does not check is refused and nothing is written. Each
//! write writes or removes exactly one file, through a temporary beside it that is synced and
//! then renamed, so no crash leaves the types and the blocks disagreeing. Someone may still
//! edit the files by hand: a state that does not check is then answered without a layout,
//! size or members for any block, never with made-up ones.
//!
//! **The file rule.** A type file is a regular file directly in `types/`, named `<stem>.st`
//! with exactly the lowercase extension `.st`, whose stem is a type name ([`type_name_problem`]
//! finds nothing wrong with it). Everything else in `types/` is ignored: a temporary
//! (`*.tmp`), `notes.txt`, `Bad-Name.st`, `X.ST`. The logic module applies the same rule.
//!
//! **Names** are compared without regard to case, as IEC 61131-3 compares them and as the
//! file systems of Windows and macOS do: an existing type or block is looked up by its listed
//! spelling before any file is opened, and another spelling of an existing name is refused.
//!
//! It builds nothing, loads nothing and never talks to a runtime. Its answers give each
//! block's layout hash, so a caller sees when a change of a type makes the programs built with
//! it out of date; the logic module builds programs from these files (ADR-014 §2.8).

use async_trait::async_trait;
use mythos_logic_compiler::{check_project, DataBlock, DataLayout, Diagnostic, Sources};
use mythos_logic_st::ast::PouKind;
use mythos_module::protocol::{
    HttpCall, HttpReply, ModuleKind, ModuleManifest, RouteDecl, RouteScope, PROTOCOL_VERSION,
};
use mythos_module::{Events, Service};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

pub const MODULE_ID: &str = "org.mythos.data";
/// The module's name in URLs and its data directory: `/api/v1/projects/:id/data/…`.
pub const PROVIDES: &str = "data";
/// Sent on the telemetry stream after each write, so open editors follow.
pub const DATA_CHANGED_EVENT: &str = "DATA_CHANGED";

const TYPES_DIR: &str = "types";
const BLOCKS_FILE: &str = "blocks.json";
/// The extension of a type file, exactly.
const TYPE_EXTENSION: &str = ".st";
/// In the data directory, the file whose lock every write holds, so that two processes serving
/// one project (a hot swap starts the new one before the old one stops) never check and write
/// at the same time: each would check the state it read, write a different file, and leave
/// files that do not check. It ends in `.tmp`, so a device's plant slice leaves it out. It is
/// made by the first write and never removed: removing a lock file lets two writers each lock
/// a different file.
pub const LOCK_FILE: &str = ".lock.tmp";

/// Names that are a device on Windows whatever follows them: `CON.st` is the console there.
/// Compared without regard to case, on every OS.
const DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The longest type name: it becomes a file name.
const MAX_NAME: usize = 64;

// -----------------------------------------------------------------------------
// What is stored
// -----------------------------------------------------------------------------

/// `blocks.json`. Unknown keys are refused, so a file written by a newer module is not read
/// as less than it says.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlocksFile {
    pub blocks: Vec<BlockEntry>,
}

/// One data block of the project, in declaration order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockEntry {
    pub name: String,
    /// The name of its type: a structure type of the project.
    #[serde(rename = "type")]
    pub ty: String,
    /// Whether the runtime keeps its value across a restart.
    pub retain: bool,
}

/// A project's types and blocks as read: each type as `(stem, text)`, sorted by file name in
/// byte order.
#[derive(Debug, Clone, Default)]
struct State {
    types: Vec<(String, String)>,
    /// Type files by the file rule whose bytes are not UTF-8 text (a copy saved by hand in
    /// another encoding), each as `(stem, why)`. Such a state does not check; the file can
    /// still be replaced or deleted by its route.
    unreadable: Vec<(String, String)>,
    blocks: Vec<BlockEntry>,
}

impl State {
    /// The stem of every type file, readable or not, by file name in byte order.
    fn stems(&self) -> Vec<&str> {
        let mut all: Vec<&str> = self
            .types
            .iter()
            .chain(&self.unreadable)
            .map(|(s, _)| s.as_str())
            .collect();
        all.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        all
    }

    /// The stored stem of type `name`: the exact spelling when one is stored (two spellings
    /// can only be there by hand, on a file system that tells case apart), otherwise the one
    /// that differs only in case.
    fn find_stem(&self, name: &str) -> Option<String> {
        let all = self.stems();
        all.iter()
            .find(|s| **s == name)
            .or_else(|| all.iter().find(|s| s.eq_ignore_ascii_case(name)))
            .map(|s| s.to_string())
    }
}

type Refusal = (u16, Value);

fn error(status: u16, message: impl ToString) -> Refusal {
    (status, json!({ "error": message.to_string() }))
}

fn refused_with(status: u16, message: impl ToString, diagnostics: &[Diagnostic]) -> Refusal {
    (
        status,
        json!({ "error": message.to_string(), "diagnostics": diagnostics }),
    )
}

/// What is wrong with `name` as a type name, as the message of the refusal; `None` when it is
/// one. A type name is an identifier (ASCII letters, digits and `_`, not beginning with a
/// digit, no `__`, not ending in `_`) of at most 64 characters, and never a device name of
/// Windows.
pub fn type_name_problem(name: &str) -> Option<String> {
    let mut chars = name.chars();
    let identifier = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.contains("__")
        && !name.ends_with('_')
        && name.len() <= MAX_NAME;
    if !identifier {
        return Some(format!(
            "'{name}' is not a type name: a letter or '_' first, then letters, digits and single underscores, not ending in '_', at most 64"
        ));
    }
    if DEVICE_NAMES.iter().any(|d| d.eq_ignore_ascii_case(name)) {
        return Some(format!(
            "'{name}' cannot be a type name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"
        ));
    }
    None
}

/// The type a file in `types/` holds, by the file rule: its stem when the file is named
/// `<type name>.st`, `None` for any other name.
pub fn type_file_stem(file_name: &str) -> Option<&str> {
    let stem = file_name.strip_suffix(TYPE_EXTENSION)?;
    type_name_problem(stem).is_none().then_some(stem)
}

/// The name a type's source has in diagnostics: relative to the project.
fn source_name(stem: &str) -> String {
    format!("data/types/{stem}{TYPE_EXTENSION}")
}

/// The type files: `(readable, unreadable)`, see [`State`].
type TypeFiles = (Vec<(String, String)>, Vec<(String, String)>);

fn read_types(data: &Path) -> Result<TypeFiles, Refusal> {
    let dir = data.join(TYPES_DIR);
    let fail = |path: &Path, e: std::io::Error| error(500, format!("{}: {e}", path.display()));
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), Vec::new())),
        Err(e) => return Err(fail(&dir, e)),
    };
    let mut types = Vec::new();
    let mut unreadable = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| fail(&dir, e))?;
        let path = entry.path();
        // Not followed: a link is not a regular file directly in `types/`.
        let kind = entry.file_type().map_err(|e| fail(&path, e))?;
        if !kind.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(stem) = file_name.to_str().and_then(type_file_stem) else {
            continue;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            // Removed since it was listed: it is not there.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(fail(&path, e)),
        };
        match String::from_utf8(bytes) {
            Ok(text) => types.push((stem.to_string(), text)),
            Err(e) => unreadable.push((
                stem.to_string(),
                format!("it is not UTF-8 text ({})", e.utf8_error()),
            )),
        }
    }
    sort_types(&mut types);
    sort_types(&mut unreadable);
    Ok((types, unreadable))
}

fn read_blocks(data: &Path) -> Result<Vec<BlockEntry>, Refusal> {
    let path = data.join(BLOCKS_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<BlocksFile>(&bytes)
            .map(|f| f.blocks)
            .map_err(|e| {
                error(
                    500,
                    format!(
                        "{} cannot be read as the project's data blocks: {e}",
                        path.display()
                    ),
                )
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(error(500, format!("{}: {e}", path.display()))),
    }
}

fn read_state(data: &Path) -> Result<State, Refusal> {
    let blocks = read_blocks(data)?;
    let (types, unreadable) = read_types(data)?;
    Ok(State {
        types,
        unreadable,
        blocks,
    })
}

/// Sort types by file name in byte order. The stems are compared: every file name is its stem
/// followed by `.st`, and `.` sorts before every character a stem may hold, so the two orders
/// are one.
fn sort_types(types: &mut [(String, String)]) {
    types.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
}

/// Check `state` with the compiler, as the logic module compiles programs in it: its types as
/// the library, its blocks as the data. Each block laid out, in declaration order.
fn check(state: &State) -> Result<Vec<DataLayout>, Vec<Diagnostic>> {
    if !state.unreadable.is_empty() {
        return Err(state
            .unreadable
            .iter()
            .map(|(stem, why)| {
                let name = source_name(stem);
                let mut d =
                    Diagnostic::whole(format!("{name} cannot be read as a type's source: {why}"));
                d.source = Some(name);
                d
            })
            .collect());
    }
    let names: Vec<String> = state.types.iter().map(|(s, _)| source_name(s)).collect();
    let library: Vec<(&str, &str)> = names
        .iter()
        .zip(&state.types)
        .map(|(n, (_, text))| (n.as_str(), text.as_str()))
        .collect();
    let data: Vec<DataBlock> = state
        .blocks
        .iter()
        .map(|b| DataBlock {
            name: &b.name,
            ty: &b.ty,
        })
        .collect();
    check_project(&Sources {
        library: &library,
        data: &data,
        ..Default::default()
    })
}

/// Each block of `blocks` as an answer gives it: with its layout, size and members when the
/// state checked (`layouts`), with only its name, type and retain when it did not.
fn blocks_json(blocks: &[BlockEntry], layouts: Option<&[DataLayout]>) -> Vec<Value> {
    blocks
        .iter()
        .map(|b| {
            let mut out = json!({ "name": b.name, "type": b.ty, "retain": b.retain });
            let laid = layouts.and_then(|l| l.iter().find(|d| d.name.eq_ignore_ascii_case(&b.name)));
            if let (Some(d), Some(fields)) = (laid, out.as_object_mut()) {
                fields.insert("layout".into(), json!(d.layout));
                fields.insert("size".into(), json!(d.size));
                fields.insert(
                    "members".into(),
                    Value::Array(
                        d.members
                            .iter()
                            .map(|m| {
                                json!({ "name": m.name, "type": m.ty, "offset": m.offset, "size": m.size })
                            })
                            .collect(),
                    ),
                );
            }
            out
        })
        .collect()
}

/// The blocks of `after` whose layout is not what it was `before` the write, new ones among
/// them; `null` when the state before did not check, since what it was is not known.
fn changed(before: Option<&[DataLayout]>, after: &[DataLayout]) -> Value {
    let Some(before) = before else {
        return Value::Null;
    };
    Value::Array(
        after
            .iter()
            .filter(|a| {
                !before
                    .iter()
                    .any(|b| b.name.eq_ignore_ascii_case(&a.name) && b.layout == a.layout)
            })
            .map(|a| json!(a.name))
            .collect(),
    )
}

/// What `text`, the source of type `name`, declares besides the type `name` itself, as the
/// message of the refusal; `None` when it declares that one type and nothing else.
fn declaration_problem(name: &str, text: &str) -> Option<String> {
    let (read, _) =
        mythos_logic_st::parser::Parser::new(mythos_logic_st::lexer::tokens(text)).source();
    let mut items: Vec<((u32, u32), String)> = read
        .types
        .iter()
        .map(|t| ((t.span.line, t.span.col), format!("type {}", t.name)))
        .collect();
    items.extend(read.pous.iter().map(|p| {
        let kind = match p.kind {
            PouKind::Function => "function",
            PouKind::FunctionBlock => "function block",
            PouKind::Program => "program",
        };
        ((p.span.line, p.span.col), format!("{kind} {}", p.name))
    }));
    items.sort_by_key(|(at, _)| *at);
    let items: Vec<String> = items.into_iter().map(|(_, i)| i).collect();
    if items == [format!("type {name}")] && read.lost.is_empty() {
        return None;
    }
    let said = if items.is_empty() {
        "nothing".to_string()
    } else {
        items.join(", ")
    };
    Some(format!(
        "the source of type '{name}' must declare the type {name} and nothing else; it declares {said}"
    ))
}

/// Write a file whole or not at all: a temporary beside it, named for this process (during a
/// hot swap two processes may write) and ending in `.tmp` (left out of a device's plant
/// slice), synced, then renamed over the file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Refusal> {
    let fail = |p: &Path, e: std::io::Error| error(500, format!("{}: {e}", p.display()));
    let Some(tmp) = temporary_path(path) else {
        return Err(error(500, format!("{}: not a file's path", path.display())));
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| fail(dir, e))?;
    }
    // A temporary of this name left by an earlier process of the same pid is removed, and the
    // temporary is then made new: never opened where it is, since whatever is at that name (a
    // link, say) would be written through, outside this directory.
    match std::fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(fail(&tmp, e)),
    }
    let mut made = false;
    let written = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        made = true;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, path)) {
        let mut message = format!("{}: {e}", path.display());
        if made {
            if let Err(r) = std::fs::remove_file(&tmp) {
                message.push_str(&format!("; {} is left: {r}", tmp.display()));
            }
        }
        return Err(error(500, message));
    }
    Ok(())
}

/// The temporary a write of `path` goes through: beside it, `.<file>.<pid>.tmp`. The pid keeps
/// two processes (during a hot swap) from writing one temporary; the `.tmp` ending keeps it
/// out of a device's plant slice.
fn temporary_path(path: &Path) -> Option<PathBuf> {
    let (dir, file) = (path.parent()?, path.file_name()?);
    Some(dir.join(format!(
        ".{}.{}.tmp",
        file.to_string_lossy(),
        std::process::id()
    )))
}

/// Hold the lock of [`LOCK_FILE`] in `data` until the file returned is dropped. The lock is
/// the operating system's, so it holds between processes; it waits while another holds it.
fn lock_writes(data: &Path) -> Result<std::fs::File, Refusal> {
    let path = data.join(LOCK_FILE);
    let fail = |e: std::io::Error| error(500, format!("{}: {e}", path.display()));
    std::fs::create_dir_all(data).map_err(|e| error(500, format!("{}: {e}", data.display())))?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(fail)?;
    file.lock().map_err(fail)?;
    Ok(file)
}

/// `blocks.json` as the documented literal has it (ADR-014 §2.8, the D3 contract §1.1): one
/// block a line, in declaration order.
///
/// ```json
/// {
///   "blocks": [
///     { "name": "Recipe", "type": "RecipeData", "retain": false },
///     { "name": "Line", "type": "LineState", "retain": true }
///   ]
/// }
/// ```
pub fn blocks_file_text(blocks: &[BlockEntry]) -> String {
    if blocks.is_empty() {
        return "{\n  \"blocks\": []\n}\n".to_string();
    }
    let lines: Vec<String> = blocks
        .iter()
        .map(|b| {
            format!(
                "    {{ \"name\": {}, \"type\": {}, \"retain\": {} }}",
                Value::from(b.name.as_str()),
                Value::from(b.ty.as_str()),
                b.retain
            )
        })
        .collect();
    format!("{{\n  \"blocks\": [\n{}\n  ]\n}}\n", lines.join(",\n"))
}

fn save_blocks(data: &Path, blocks: &[BlockEntry]) -> Result<(), Refusal> {
    write_atomic(&data.join(BLOCKS_FILE), blocks_file_text(blocks).as_bytes())
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
        needs: Vec::new(),
    }
}

/// What `mythos-module-data --manifest` prints.
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        protocol: PROTOCOL_VERSION,
        id: MODULE_ID.into(),
        name: "Data".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        kind: ModuleKind::Service,
        provides: PROVIDES.into(),
        description: "User types and data blocks of a project: stored, checked with the compiler, laid out; the logic module builds programs with them".into(),
        capabilities: vec![],
        config_schema: Value::Null,
        always_on: false,
        profile: Value::Null,
        routes: vec![
            route("GET", "/types", "viewer", "list_data_types",
                "The project's user types, by name, in the order of their file names. Reads only; nothing is written, built or loaded.", None),
            route("GET", "/types/:type", "viewer", "get_data_type",
                "A user type's Structured Text source: one TYPE … END_TYPE declaring that type. Reads only; nothing is written, built or loaded.", None),
            route("PUT", "/types/:type", "engineer", "save_data_type",
                "Make or replace a user type: writes its source, one TYPE … END_TYPE declaring the type of the path's name exactly and nothing else (a structure, or a name for an array type), to its own file. The project's types and data blocks are checked with the compiler first, with this source in; if they do not check, nothing is written and the answer gives the diagnostics. Answers each data block with its layout, and 'changed': the blocks whose layout this changed (null when the stored state did not check). Nothing is built or loaded: programs built with an old layout must be built again in the logic module.",
                Some(json!({
                    "type": "object",
                    "required": ["source"],
                    "properties": { "source": { "type": "string", "description": "Structured Text: TYPE <name> : STRUCT … END_STRUCT; END_TYPE" } }
                }))),
            route("DELETE", "/types/:type", "engineer", "delete_data_type",
                "Remove a user type's file. Refused while a data block has the type, or when the project's types and data blocks would not check without it; then nothing is removed. The data module cannot see programs: one that declares a variable of the type no longer compiles. Nothing is built or loaded.", None),
            route("GET", "/blocks", "viewer", "list_data_blocks",
                "The project's data blocks in declaration order, and whether the project's types and data blocks check ('ok', with the compiler's diagnostics). When they check, each block has its layout hash, size and top-level members (name, type, offset, size); when they do not, a block has only its name, type and retain. Reads only; nothing is written, built or loaded.", None),
            route("PUT", "/blocks/:block", "engineer", "save_data_block",
                "Make or replace a data block: a named instance of a structure type of the project, shared by the programs that declare it in VAR_EXTERNAL, and whether the runtime retains it. Writes blocks.json, a new block appended, a replaced one in its place. The project's types and data blocks are checked with the compiler first; if they do not check, nothing is written and the answer gives the diagnostics. Answers each data block with its layout, and 'changed': the blocks whose layout this changed or that are new (null when the stored state did not check). Nothing is built or loaded.",
                Some(json!({
                    "type": "object",
                    "required": ["type", "retain"],
                    "additionalProperties": false,
                    "properties": {
                        "type": { "type": "string", "description": "A structure type of the project" },
                        "retain": { "type": "boolean", "description": "Whether the runtime keeps its value across a restart" }
                    }
                }))),
            route("DELETE", "/blocks/:block", "engineer", "delete_data_block",
                "Remove a data block from blocks.json. Refused, with nothing written, when the project's types and data blocks would not check without it. A program that declares it in VAR_EXTERNAL no longer compiles. Nothing is built or loaded.", None),
            route("POST", "/check", "viewer", "check_data",
                "Check the project's types and data blocks with the compiler, without saving anything: the stored state, or with 'type' and 'source' (both or neither) that one type's source put in, new or replacing. Answers 'ok', the diagnostics, and the blocks (with their layouts when it checks). Nothing is written, built or loaded.",
                Some(json!({
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "description": "A type's name; give source with it" },
                        "source": { "type": "string", "description": "That type's source; give type with it" }
                    }
                }))),
        ],
    }
}

fn body<T: for<'de> Deserialize<'de>>(call: &HttpCall) -> Result<T, Refusal> {
    serde_json::from_value(call.body.clone().unwrap_or(Value::Null))
        .map_err(|e| error(400, format!("the body is not what this route takes: {e}")))
}

#[derive(Deserialize)]
struct SourceBody {
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockBody {
    #[serde(rename = "type")]
    ty: String,
    retain: bool,
}

#[derive(Deserialize, Default)]
struct CheckBody {
    #[serde(default, rename = "type")]
    ty: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

/// What one call answers, and the event it sends.
type Served = (Result<Value, Refusal>, Option<Value>);

/// The data service. Every call is serialized: each write is a read-check-write of the whole
/// state, and the server forwards requests concurrently. A write also holds the lock of
/// [`LOCK_FILE`], which another process serving the same directory waits on.
#[derive(Default)]
pub struct Data {
    writes: Mutex<()>,
}

impl Data {
    /// Answer `call`, with the data of the [`DATA_CHANGED_EVENT`] to send when it changed
    /// something. [`Service::handle`] is this, with the event sent.
    pub async fn serve(&self, call: &HttpCall) -> (HttpReply, Option<Value>) {
        let Some(data) = call.data_dir.as_ref().map(PathBuf::from) else {
            return (
                HttpReply::error(500, "the server gave the data module no data directory"),
                None,
            );
        };
        // A type's name becomes a file name: nothing but a type name gets that far.
        if let Some(name) = call.params.get("type") {
            if let Some(problem) = type_name_problem(name) {
                return (HttpReply::error(400, problem), None);
            }
        }
        let _guard = self.writes.lock().await;
        // A write also holds the lock between processes (see [`LOCK_FILE`]). A read does not:
        // it writes nothing, and makes no file.
        let reads = call.method == "GET" || (call.method == "POST" && call.route == "/check");
        let _lock = if reads {
            None
        } else {
            match lock_writes(&data) {
                Ok(file) => Some(file),
                Err((status, body)) => return (HttpReply::json(status, body), None),
            }
        };
        let (result, event) = self.handle_in(call, &data);
        let reply = match result {
            Ok(body) => HttpReply::json(200, body),
            Err((status, body)) => HttpReply::json(status, body),
        };
        (reply, event)
    }

    fn handle_in(&self, call: &HttpCall, data: &Path) -> Served {
        let project = call.project.clone().unwrap_or_default();
        let event = |what: &str, name: &str, change: &str| {
            Some(json!({ "project": project, "what": what, "name": name, "change": change }))
        };
        let param = |p: &str| call.params.get(p).map(String::as_str).unwrap_or("");
        match (call.method.as_str(), call.route.as_str()) {
            // Every route reads the whole state first, so a blocks.json that cannot be read is
            // said by every one of them, never passed over.
            ("GET", "/types") => (
                read_state(data).map(|state| {
                    json!({ "types": state.stems().iter().map(|n| json!({ "name": n })).collect::<Vec<_>>() })
                }),
                None,
            ),
            ("GET", "/types/:type") => (
                read_state(data).and_then(|state| {
                    let stem = find_type(&state, param("type"))?;
                    match state.types.iter().find(|(s, _)| *s == stem) {
                        Some((_, text)) => Ok(json!({ "name": stem, "source": text })),
                        // Its bytes are not text: there is no source to give.
                        None => {
                            let path = data.join(TYPES_DIR).join(format!("{stem}{TYPE_EXTENSION}"));
                            let why = state
                                .unreadable
                                .iter()
                                .find(|(s, _)| *s == stem)
                                .map_or("", |(_, why)| why.as_str());
                            Err(error(500, format!("{}: {why}", path.display())))
                        }
                    }
                }),
                None,
            ),
            ("PUT", "/types/:type") => {
                let name = param("type");
                match save_type(call, data, name) {
                    Ok(answer) => (Ok(answer), event("type", name, "saved")),
                    Err(r) => (Err(r), None),
                }
            }
            ("DELETE", "/types/:type") => match delete_type(data, param("type")) {
                Ok((stem, answer)) => (Ok(answer), event("type", &stem, "deleted")),
                Err(r) => (Err(r), None),
            },
            ("GET", "/blocks") => (
                read_state(data).map(|state| {
                    let (ok, diagnostics, layouts) = match check(&state) {
                        Ok(l) => (true, Vec::new(), Some(l)),
                        Err(d) => (false, d, None),
                    };
                    json!({ "ok": ok, "diagnostics": diagnostics,
                            "blocks": blocks_json(&state.blocks, layouts.as_deref()) })
                }),
                None,
            ),
            ("PUT", "/blocks/:block") => {
                let name = param("block");
                match save_block(call, data, name) {
                    Ok(answer) => (Ok(answer), event("block", name, "saved")),
                    Err(r) => (Err(r), None),
                }
            }
            ("DELETE", "/blocks/:block") => match delete_block(data, param("block")) {
                Ok((name, answer)) => (Ok(answer), event("block", &name, "deleted")),
                Err(r) => (Err(r), None),
            },
            ("POST", "/check") => (check_route(call, data), None),
            (method, route) => (
                Err(error(404, format!("the data module has no route {method} {route}"))),
                None,
            ),
        }
    }
}

/// The stored stem of type `name` ([`State::find_stem`]), or the refusal that there is none.
fn find_type(state: &State, name: &str) -> Result<String, Refusal> {
    state
        .find_stem(name)
        .ok_or_else(|| error(404, format!("the project has no type '{name}'")))
}

fn same_spelling_or_refuse(existing: &str, name: &str, what: &str) -> Result<(), Refusal> {
    if existing == name {
        return Ok(());
    }
    Err(error(
        409,
        format!("the project already has a {what} '{existing}'; names that differ only in case are one name"),
    ))
}

/// `state` with type `name`'s source `text` put in, new or replacing the one of the same
/// spelling (another spelling is refused); and whether it is new.
/// A file that is not text is replaced like any other.
fn with_type(mut state: State, name: &str, text: String) -> Result<(State, bool), Refusal> {
    let stored = state.find_stem(name);
    if let Some(existing) = &stored {
        same_spelling_or_refuse(existing, name, "type")?;
    }
    state.unreadable.retain(|(s, _)| s != name);
    match state.types.iter_mut().find(|(s, _)| s == name) {
        Some(stored) => stored.1 = text,
        None => {
            state.types.push((name.to_string(), text));
            sort_types(&mut state.types);
        }
    }
    Ok((state, stored.is_none()))
}

fn save_type(call: &HttpCall, data: &Path, name: &str) -> Result<Value, Refusal> {
    let given = body::<SourceBody>(call)?;
    let old = read_state(data)?;
    let before = check(&old).ok();
    let (new, created) = with_type(old, name, given.source.clone())?;
    let after = check(&new).map_err(|d| {
        refused_with(
            400,
            "with this source the project's types and data blocks do not check, and nothing was saved",
            &d,
        )
    })?;
    if let Some(problem) = declaration_problem(name, &given.source) {
        return Err(error(400, problem));
    }
    write_atomic(
        &data.join(TYPES_DIR).join(format!("{name}{TYPE_EXTENSION}")),
        given.source.as_bytes(),
    )?;
    Ok(json!({
        "name": name,
        "created": created,
        "blocks": blocks_json(&new.blocks, Some(&after)),
        "changed": changed(before.as_deref(), &after),
    }))
}

fn delete_type(data: &Path, name: &str) -> Result<(String, Value), Refusal> {
    let mut state = read_state(data)?;
    let stem = find_type(&state, name)?;
    let users: Vec<String> = state
        .blocks
        .iter()
        .filter(|b| b.ty.eq_ignore_ascii_case(&stem))
        .map(|b| format!("'{}'", b.name))
        .collect();
    if !users.is_empty() {
        return Err(error(
            409,
            format!(
                "type '{stem}' is the type of data block {}: delete those blocks or give them another type first",
                users.join(", ")
            ),
        ));
    }
    state.types.retain(|(s, _)| *s != stem);
    state.unreadable.retain(|(s, _)| *s != stem);
    let after = check(&state).map_err(|d| {
        refused_with(
            400,
            format!("without type '{stem}' the project's types and data blocks do not check, and it was not deleted"),
            &d,
        )
    })?;
    // The stored spelling, from the listing: never the request's, which on Linux could name
    // another file.
    let path = data.join(TYPES_DIR).join(format!("{stem}{TYPE_EXTENSION}"));
    std::fs::remove_file(&path).map_err(|e| error(500, format!("{}: {e}", path.display())))?;
    let answer = json!({ "deleted": stem, "blocks": blocks_json(&state.blocks, Some(&after)) });
    Ok((stem, answer))
}

fn save_block(call: &HttpCall, data: &Path, name: &str) -> Result<Value, Refusal> {
    let given = body::<BlockBody>(call)?;
    let mut state = read_state(data)?;
    let before = check(&state).ok();
    let entry = BlockEntry {
        name: name.to_string(),
        ty: given.ty,
        retain: given.retain,
    };
    let created = match state
        .blocks
        .iter()
        .position(|b| b.name.eq_ignore_ascii_case(name))
    {
        Some(k) => {
            same_spelling_or_refuse(&state.blocks[k].name, name, "data block")?;
            state.blocks[k] = entry;
            false
        }
        None => {
            state.blocks.push(entry);
            true
        }
    };
    let after = check(&state).map_err(|d| {
        refused_with(
            400,
            format!("data block '{name}' does not check and was not saved"),
            &d,
        )
    })?;
    save_blocks(data, &state.blocks)?;
    Ok(json!({
        "created": created,
        "blocks": blocks_json(&state.blocks, Some(&after)),
        "changed": changed(before.as_deref(), &after),
    }))
}

fn delete_block(data: &Path, name: &str) -> Result<(String, Value), Refusal> {
    let mut state = read_state(data)?;
    let Some(k) = state
        .blocks
        .iter()
        .position(|b| b.name.eq_ignore_ascii_case(name))
    else {
        return Err(error(
            404,
            format!("the project has no data block '{name}'"),
        ));
    };
    let stored = state.blocks.remove(k).name;
    let after = check(&state).map_err(|d| {
        refused_with(
            400,
            format!("without data block '{stored}' the project's types and data blocks do not check, and it was not deleted"),
            &d,
        )
    })?;
    save_blocks(data, &state.blocks)?;
    let answer = json!({ "deleted": stored, "blocks": blocks_json(&state.blocks, Some(&after)) });
    Ok((stored, answer))
}

fn check_route(call: &HttpCall, data: &Path) -> Result<Value, Refusal> {
    let given = match &call.body {
        None | Some(Value::Null) => CheckBody::default(),
        Some(_) => body::<CheckBody>(call)?,
    };
    let mut state = read_state(data)?;
    let mut extra = None;
    match (given.ty, given.source) {
        (None, None) => {}
        (Some(name), Some(source)) => {
            if let Some(problem) = type_name_problem(&name) {
                return Err(error(400, problem));
            }
            extra = declaration_problem(&name, &source);
            state = with_type(state, &name, source)?.0;
        }
        _ => return Err(error(400, "give both type and source, or neither")),
    }
    let (diagnostics, layouts) = match check(&state) {
        // The compiler's diagnostics win: the rule of one declaration is said only of a
        // source that checks, as a save says it.
        Ok(l) => match extra {
            Some(problem) => (vec![Diagnostic::whole(problem)], None),
            None => (Vec::new(), Some(l)),
        },
        Err(d) => (d, None),
    };
    Ok(json!({
        "ok": layouts.is_some(),
        "diagnostics": diagnostics,
        "blocks": blocks_json(&state.blocks, layouts.as_deref()),
    }))
}

#[async_trait]
impl Service for Data {
    async fn handle(&self, call: HttpCall, events: &Events) -> HttpReply {
        let (reply, event) = self.serve(&call).await;
        if let Some(data) = event {
            events.send(DATA_CHANGED_EVENT, data);
        }
        reply
    }
}

#[cfg(test)]
mod tests;
