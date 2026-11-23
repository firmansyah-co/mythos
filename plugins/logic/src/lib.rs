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
//! document beside its PROGRAM, in Structured Text before it.
//!
//! **The project's library and data (ADR-014 §2.7, §2.8).** Beside the programs, the module
//! keeps the project's library: Structured Text sources of functions and function blocks,
//! `library/<name>.st`, with no PROGRAM and no TYPE, compiled into every program. And it
//! *reads*, never writes, the data module's directory beside its own (`<project>/data/`): the
//! project's user types (`types/<Name>.st`) and data blocks (`blocks.json`). Every check and
//! build compiles the program with the types and the library, and with the data blocks; when
//! the stored types and blocks do not check by themselves, the program is compiled without
//! them, so a program that uses none of them is never refused for them, and the answer says
//! why in `project_problems`. A project with neither a library nor data compiles exactly as it
//! did before there were either.
//!
//! It never talks to a runtime, and it does not load what it builds: a module cannot call the
//! core. The workbench, or an agent over MCP, takes the built instance and saves it into the
//! project's runtimes.

use async_trait::async_trait;
use mythos_logic_compiler::{
    check_project, compile_sources, At, BodyOf, DataBlock, Diagnostic, Holds, Sources, Symbol,
    TargetProfile,
};
use mythos_logic_st::ast::PouKind;
use mythos_module::protocol::{
    check_data, DataBlock as RuntimeDataBlock, DataBlockMember, DataMember, DataUse, HttpCall,
    HttpReply, InputBinding, InputOf, ModuleKind, ModuleManifest, OutputBinding, PublishedBinding,
    RouteDecl, RouteScope, RuntimeProgram, TaskConfig, PROTOCOL_VERSION,
};
use mythos_module::{Events, Service};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

mod runtimes;

pub const MODULE_ID: &str = "org.mythos.logic";
/// The module's name in URLs and its data directory: `/api/v1/projects/:id/logic/…`.
pub const PROVIDES: &str = "logic";
/// Sent on the telemetry stream when a project's programs change, so open editors follow.
pub const PROGRAMS_CHANGED_EVENT: &str = "LOGIC_PROGRAMS_CHANGED";
/// Sent when a source of the project's library is saved or deleted: `{"project", "name",
/// "what": "saved" | "deleted"}`. Not [`PROGRAMS_CHANGED_EVENT`], whose `program` the
/// workbench reads.
pub const LIBRARY_CHANGED_EVENT: &str = "LOGIC_LIBRARY_CHANGED";
/// Sent when a runtime of the project's task configuration is saved or deleted: `{"project",
/// "runtime", "what": "saved" | "deleted"}`. Not [`PROGRAMS_CHANGED_EVENT`], whose `program`
/// and `what` the workbench acts on.
pub const RUNTIMES_CHANGED_EVENT: &str = "LOGIC_RUNTIMES_CHANGED";
/// In the module's directory, the file whose lock every write holds, so that two processes
/// serving one project (a hot swap starts the new one before the old one stops) never read and
/// write at the same time: the task configuration and the programs must agree, and they are two
/// files. It ends in `.tmp`, so a device's plant slice leaves it out. It is made by the first
/// write and never removed: removing a lock file lets two writers each lock a different file.
pub const LOCK_FILE: &str = ".lock.tmp";

const PROGRAMS_FILE: &str = "programs.json";
const SOURCES_DIR: &str = "programs";
/// The project's library, in the module's own directory: `library/<name>.st`.
const LIBRARY_DIR: &str = "library";
/// The data module's directory beside this one (ADR-014 §2.8), read and never written.
const DATA_DIR: &str = "data";
/// In the data directory: one file a user type, and the data blocks.
const TYPES_DIR: &str = "types";
const BLOCKS_FILE: &str = "blocks.json";
/// The extension of a library source or a type file, exactly.
const SOURCE_EXTENSION: &str = ".st";
/// Names that are a device on Windows whatever follows them: `CON.st` is the console there.
/// Compared without regard to case, on every OS.
const DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

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
    /// A mark given when the program is made, never changed, and carried by each of its
    /// builds (`RuntimeProgram::created`): a program deleted and made again under this name
    /// has another, so the core takes it for another program. A program made before there was
    /// a mark has none, and is never given one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
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

/// The mark of a program made now as `name` (see [`ProgramEntry::created`]): what tells it
/// from any other program made under that name, before or after, in this or another process.
fn making_mark(name: &str, now: std::time::SystemTime) -> String {
    static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let since = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut hash = Sha256::new();
    hash.update(name.as_bytes());
    hash.update(since.to_le_bytes());
    hash.update(std::process::id().to_le_bytes());
    hash.update(
        MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .to_le_bytes(),
    );
    hex::encode(&hash.finalize()[..8])
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

fn save(data: &Path, programs: &Programs) -> Result<(), Refusal> {
    let bytes = serde_json::to_vec_pretty(programs).map_err(|e| error(500, e))?;
    write_synced(&data.join(PROGRAMS_FILE), &bytes)
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

// -----------------------------------------------------------------------------
// The project's library, types and data blocks
// -----------------------------------------------------------------------------

fn device_name(name: &str) -> bool {
    DEVICE_NAMES.iter().any(|d| d.eq_ignore_ascii_case(name))
}

/// What is wrong with `name` as a library source's name, as the message of the refusal;
/// `None` when it is one: a program's name (it becomes a file name) that is not a device name
/// of Windows.
fn library_name_problem(name: &str) -> Option<String> {
    if !valid_name(name) {
        return Some(format!(
            "'{name}' is not a library source name: a letter, then letters, digits or _, at most 64"
        ));
    }
    if device_name(name) {
        return Some(format!(
            "'{name}' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"
        ));
    }
    None
}

/// Whether `name` is a type name as the data module takes one (ADR-014 §2.8, the D3 contract
/// §1.2), word for word: an identifier (ASCII letters, digits and `_`, not beginning with a
/// digit, no `__`, not ending in `_`) of at most 64 characters, and never a device name of
/// Windows. The two modules may not depend on each other, so the rule is written twice; a test
/// of each pins them together.
fn type_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.contains("__")
        && !name.ends_with('_')
        && name.len() <= 64
        && !device_name(name)
}

/// The stem of a file named `<stem>.st` whose stem `takes`, exactly the lowercase extension;
/// `None` for any other name.
fn source_stem(file_name: &str, takes: fn(&str) -> bool) -> Option<&str> {
    let stem = file_name.strip_suffix(SOURCE_EXTENSION)?;
    takes(stem).then_some(stem)
}

/// The data module's `blocks.json`: unknown keys refused, `retain` required, as it writes it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlocksFile {
    blocks: Vec<BlockEntry>,
}

/// One data block of the project, in declaration order.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockEntry {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    retain: bool,
}

/// What a program of the project is compiled with besides its own source, as read.
#[derive(Debug, Clone, Default)]
struct Project {
    /// `(source name, text)` of each user type: `data/types/X.st`, by file name in byte order.
    types: Vec<(String, String)>,
    /// `(source name, text)` of each library source: `logic/library/X.st`, by file name.
    library: Vec<(String, String)>,
    blocks: Vec<BlockEntry>,
    /// Why the project's data cannot be read as it is: `blocks.json` is not the project's
    /// data blocks (or not a file), `data/types` is not a directory, a type file is not text.
    /// The stored data is then taken as not checking, and no program is refused for it.
    problems: Vec<Diagnostic>,
    /// Library sources that cannot be read as text, each a diagnostic naming its source: a
    /// mistake of the library, which refuses every program as any other does.
    unreadable_library: Vec<Diagnostic>,
}

impl Project {
    /// A project of before the library and the data: compiled exactly as then.
    fn is_empty(&self) -> bool {
        self.types.is_empty()
            && self.library.is_empty()
            && self.blocks.is_empty()
            && self.problems.is_empty()
            && self.unreadable_library.is_empty()
    }
}

/// The library's file rule: a library source's name.
fn library_file(stem: &str) -> bool {
    library_name_problem(stem).is_none()
}

/// Every file in `dir` by the file rule: a regular file directly in it (a link or a directory
/// is not one), named `<stem>.st` with a stem that `takes`; anything else is ignored. Each as
/// `(stem, path)`, sorted by file name in byte order: every name is its stem followed by
/// `.st`, and `.` sorts before every character a stem may hold, so sorting the stems sorts the
/// names. A missing directory holds none. Nothing is read but the directory.
fn list_sources(
    dir: &Path,
    takes: fn(&str) -> bool,
) -> Result<Vec<(String, PathBuf)>, (PathBuf, std::io::Error)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err((dir.to_path_buf(), e)),
    };
    let mut sources = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| (dir.to_path_buf(), e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| (path.clone(), e))?;
        if !kind.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(stem) = file_name.to_str().and_then(|n| source_stem(n, takes)) else {
            continue;
        };
        sources.push((stem.to_string(), path));
    }
    sources.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
    Ok(sources)
}

/// The sources of [`list_sources`], read: `(stem, text)` of each, and `(stem, why)` of each
/// that cannot be read as text. A file removed since it was listed is not there.
struct Read {
    texts: Vec<(String, String)>,
    unreadable: Vec<(String, String)>,
}

fn read_sources(dir: &Path, takes: fn(&str) -> bool) -> Result<Read, (PathBuf, std::io::Error)> {
    let mut read = Read {
        texts: Vec::new(),
        unreadable: Vec::new(),
    };
    for (stem, path) in list_sources(dir, takes)? {
        match std::fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => read.texts.push((stem, text)),
                Err(e) => read
                    .unreadable
                    .push((stem, format!("it is not UTF-8 text ({})", e.utf8_error()))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => read.unreadable.push((stem, e.to_string())),
        }
    }
    Ok(read)
}

/// `blocks.json` as read.
#[derive(PartialEq)]
enum BlocksRead {
    Missing,
    Bytes(Vec<u8>),
    /// It cannot be read (a directory, say): why.
    Failed(String),
}

fn read_blocks(path: &Path) -> BlocksRead {
    match std::fs::read(path) {
        Ok(bytes) => BlocksRead::Bytes(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BlocksRead::Missing,
        Err(e) => BlocksRead::Failed(e.to_string()),
    }
}

/// A diagnostic about a whole source, naming it.
fn of_source(name: String, message: String) -> Diagnostic {
    let mut d = Diagnostic::whole(message);
    d.source = Some(name);
    d
}

/// The project's data directory, beside the module's own `logic` (ADR-014 §2.8).
fn data_dir_of(logic: &Path) -> Result<PathBuf, Refusal> {
    logic.parent().map(|p| p.join(DATA_DIR)).ok_or_else(|| {
        error(
            500,
            format!(
                "{}: the module's data directory has no parent, so the project's data cannot be found",
                logic.display()
            ),
        )
    })
}

/// The project's library, types and data blocks, read and never written. `blocks.json` is
/// read before and after the types: when another process wrote it between, everything is read
/// once more (a write between two reads can still give one compile a state that never was; the
/// data module's lock is taken by its writes only, and this module writes nothing there).
///
/// Data that cannot be read as it is becomes a problem of the project, never a refusal of the
/// route: a program that uses none of it still compiles. A library directory that cannot be
/// listed is a 500 naming it.
fn read_project(logic: &Path) -> Result<Project, Refusal> {
    let data = data_dir_of(logic)?;
    let blocks_path = data.join(BLOCKS_FILE);
    let types_dir = data.join(TYPES_DIR);
    let library = read_sources(&logic.join(LIBRARY_DIR), library_file)
        .map_err(|(path, e)| error(500, format!("{}: {e}", path.display())))?;
    let first = read_blocks(&blocks_path);
    let mut types = read_sources(&types_dir, type_name);
    let mut blocks = read_blocks(&blocks_path);
    if blocks != first {
        types = read_sources(&types_dir, type_name);
        blocks = read_blocks(&blocks_path);
    }
    let mut problems = Vec::new();
    let unreadable_blocks = |why: String| {
        Diagnostic::whole(format!(
            "data/blocks.json cannot be read as the project's data blocks: {why}"
        ))
    };
    let blocks = match blocks {
        BlocksRead::Missing => Vec::new(),
        BlocksRead::Bytes(bytes) => match serde_json::from_slice::<BlocksFile>(&bytes) {
            Ok(file) => file.blocks,
            Err(e) => {
                problems.push(unreadable_blocks(e.to_string()));
                Vec::new()
            }
        },
        BlocksRead::Failed(why) => {
            problems.push(unreadable_blocks(why));
            Vec::new()
        }
    };
    let named = |prefix: &str, stem: &str| format!("{prefix}/{stem}{SOURCE_EXTENSION}");
    let types = match types {
        Ok(read) => {
            for (stem, why) in read.unreadable {
                let name = named("data/types", &stem);
                let message = format!("{name} cannot be read as a type's source: {why}");
                problems.push(of_source(name, message));
            }
            read.texts
                .into_iter()
                .map(|(stem, text)| (named("data/types", &stem), text))
                .collect()
        }
        Err((_, e)) => {
            problems.push(Diagnostic::whole(format!(
                "data/types cannot be read as the project's types: {e}"
            )));
            Vec::new()
        }
    };
    let unreadable_library = library
        .unreadable
        .into_iter()
        .map(|(stem, why)| {
            let name = named("logic/library", &stem);
            let message = format!("{name} cannot be read as a library source: {why}");
            of_source(name, message)
        })
        .collect();
    Ok(Project {
        types,
        library: library
            .texts
            .into_iter()
            .map(|(stem, text)| (named("logic/library", &stem), text))
            .collect(),
        blocks,
        problems,
        unreadable_library,
    })
}

fn pairs(sources: &[(String, String)]) -> Vec<(&str, &str)> {
    sources
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect()
}

fn data_blocks(blocks: &[BlockEntry]) -> Vec<DataBlock<'_>> {
    blocks
        .iter()
        .map(|b| DataBlock {
            name: &b.name,
            ty: &b.ty,
        })
        .collect()
}

/// The project's types and data blocks checked by themselves: `Err` with why when they do
/// not check, and then a program is compiled without them.
fn check_stored_data(project: &Project) -> Result<(), Vec<Diagnostic>> {
    if !project.problems.is_empty() {
        return Err(project.problems.clone());
    }
    if project.types.is_empty() && project.blocks.is_empty() {
        return Ok(());
    }
    let types = pairs(&project.types);
    let blocks = data_blocks(&project.blocks);
    check_project(&Sources {
        library: &types,
        data: &blocks,
        ..Default::default()
    })
    .map(|_| ())
}

/// What refuses every program before it is compiled, each with where it is: a library source
/// that cannot be read as text; a TYPE declared in a library source, since the project's types
/// are the data module's (ADR-014 §2.8); a PROGRAM in one, said in the module's words, since
/// the compiler's own would say a library may hold types.
fn library_refusals(project: &Project) -> Vec<Diagnostic> {
    let mut found = project.unreadable_library.clone();
    for (name, text) in &project.library {
        let (read, _) =
            mythos_logic_st::parser::Parser::new(mythos_logic_st::lexer::tokens(text)).source();
        for t in &read.types {
            let mut d = Diagnostic::at(
                t.span,
                format!(
                    "{} is a TYPE in the library source '{name}'; the project's types are made in the data module",
                    t.name
                ),
            );
            d.source = Some(name.clone());
            found.push(d);
        }
        for p in read.pous.iter().filter(|p| p.kind == PouKind::Program) {
            let mut d = Diagnostic::at(
                p.span,
                format!(
                    "{} is a PROGRAM in the library source '{name}'; a library holds functions and function blocks",
                    p.name
                ),
            );
            d.source = Some(name.clone());
            found.push(d);
        }
    }
    found
}

/// What a program's compile is given of the project: the library sources (the types first
/// when the stored data checks) and the data blocks, or none of the blocks, with why. When the
/// stored data does not check, the types still come first if they check by themselves: a
/// library source that uses one then still compiles, and no program is refused for blocks it
/// does not use.
struct Given<'a> {
    library: Vec<(&'a str, &'a str)>,
    data: Vec<DataBlock<'a>>,
    problems: Option<Vec<Diagnostic>>,
}

fn given(project: &Project) -> Given<'_> {
    match check_stored_data(project) {
        Ok(()) => {
            let mut library = pairs(&project.types);
            library.extend(pairs(&project.library));
            Given {
                library,
                data: data_blocks(&project.blocks),
                problems: None,
            }
        }
        Err(problems) => {
            let types = pairs(&project.types);
            let typed = !types.is_empty()
                && check_project(&Sources {
                    library: &types,
                    ..Default::default()
                })
                .is_ok();
            let mut library = if typed { types } else { Vec::new() };
            library.extend(pairs(&project.library));
            Given {
                library,
                data: Vec::new(),
                problems: Some(problems),
            }
        }
    }
}

/// Write a file whole or not at all: a temporary beside it, named for this process (during a
/// hot swap two processes may write) and ending in `.tmp` (left out of a device's plant
/// slice), synced, then renamed over the file.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), Refusal> {
    about_to_write(path)?;
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

/// Remove a file of the module: a write, so gated as [`write_synced`] is.
fn remove_written(path: &Path) -> Result<(), Refusal> {
    about_to_write(path)?;
    std::fs::remove_file(path).map_err(|e| error(500, format!("{}: {e}", path.display())))
}

std::thread_local! {
    /// Set while a write route runs in a project with no module directory yet, without the
    /// lock (see [`Logic::serve`]): a write it reaches is not made, and [`WANTED`] is set.
    static DRY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Whether a write was reached while [`DRY`] was set.
    static WANTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Every write of the module passes here first: while [`DRY`] is set it is not made.
fn about_to_write(path: &Path) -> Result<(), Refusal> {
    if DRY.get() {
        WANTED.set(true);
        return Err(error(
            500,
            format!(
                "{}: not written while the call is tried without the lock",
                path.display()
            ),
        ));
    }
    Ok(())
}

/// Run `route` with every write held back ([`DRY`]): its answer when it reached no write,
/// `None` when it reached one (and wrote nothing).
fn without_writes<T>(route: impl FnOnce() -> T) -> Option<T> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            DRY.set(false);
        }
    }
    WANTED.set(false);
    DRY.set(true);
    let reset = Reset;
    let answer = route();
    drop(reset);
    (!WANTED.replace(false)).then_some(answer)
}

/// Hold the lock of [`LOCK_FILE`] in `data` until the file returned is dropped. The lock is
/// the operating system's, so it holds between processes; it waits while another holds it.
/// The data module holds its own the same way; the two may not depend on each other.
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

/// Whether a call only reads: it writes nothing, so it takes no lock and makes no file.
fn only_reads(call: &HttpCall) -> bool {
    call.method == "GET"
        || (call.method == "POST"
            && (call.route.ends_with("/check") || call.route.ends_with("/build")))
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

/// The library sources' names, by file name; nothing is read but the directory.
fn library_names(data: &Path) -> Result<Vec<String>, Refusal> {
    Ok(list_sources(&data.join(LIBRARY_DIR), library_file)
        .map_err(|(path, e)| error(500, format!("{}: {e}", path.display())))?
        .into_iter()
        .map(|(stem, _)| stem)
        .collect())
}

/// The stored spelling of library source `name`: the exact one when it is stored (two
/// spellings can only be there by hand, on a file system that tells case apart), otherwise one
/// that differs only in case, which is one name.
fn stored_library_name(data: &Path, name: &str) -> Result<Option<String>, Refusal> {
    let names = library_names(data)?;
    Ok(names
        .iter()
        .find(|s| *s == name)
        .or_else(|| names.iter().find(|s| s.eq_ignore_ascii_case(name)))
        .cloned())
}

fn library_path(data: &Path, stem: &str) -> PathBuf {
    data.join(LIBRARY_DIR)
        .join(format!("{stem}{SOURCE_EXTENSION}"))
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

/// What compiling a program gave: the program or the compiler's diagnostics, and, when the
/// stored types and data blocks do not check, why (they were left out).
struct Compiling {
    compiled: Result<mythos_logic_compiler::Compiled, Vec<Diagnostic>>,
    project_problems: Option<Vec<Diagnostic>>,
}

/// Compile the program's `source`, written in its language, for `target`: from that one
/// source (see the top of this file), with the project's types, library and data blocks.
fn compile_entry(
    entry: &ProgramEntry,
    source: &str,
    target: &TargetProfile,
    project: &Project,
) -> Compiling {
    let file = |extension: &str| format!("{}.{extension}", entry.name);
    if project.is_empty() {
        // A project of before the library and the data: exactly as then.
        let compiled = if is_tc6(&entry.language) {
            let name = file("xml");
            compile_sources(
                &Sources {
                    st: None,
                    tc6: &[(name.as_str(), source)],
                    ..Default::default()
                },
                target,
            )
        } else {
            let name = file("st");
            compile_sources(
                &Sources {
                    st: Some((name.as_str(), source)),
                    tc6: &[],
                    ..Default::default()
                },
                target,
            )
        };
        return Compiling {
            compiled,
            project_problems: None,
        };
    }
    // The stored data first, by itself: data that does not check is left out, so it refuses
    // only a program that uses it (the compiler would refuse every program for it).
    let given = given(project);
    let project_problems = given.problems.clone();
    let refused = library_refusals(project);
    if !refused.is_empty() {
        return Compiling {
            compiled: Err(refused),
            project_problems,
        };
    }
    let name = file(if is_tc6(&entry.language) { "xml" } else { "st" });
    let own = [(name.as_str(), source)];
    let sources = if is_tc6(&entry.language) {
        Sources {
            st: None,
            tc6: &own,
            library: &given.library,
            data: &given.data,
        }
    } else {
        Sources {
            st: Some(own[0]),
            tc6: &[],
            library: &given.library,
            data: &given.data,
        }
    };
    Compiling {
        compiled: compile_sources(&sources, target),
        project_problems,
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

/// What identifies a build: its code and everything that ties it to the plant, and the data
/// blocks it was built with, laid out (a renamed member can leave the code alike, and the core
/// refuses a program built with another layout). A check gives the same, so an editor knows
/// whether the text it shows is what a runtime runs. A program without data blocks hashes
/// what it did before there were any. Whether a block is retained is the runtime's, not the
/// program's: it is not in the version.
fn version_of(entry: &ProgramEntry, compiled: &mythos_logic_compiler::Compiled) -> String {
    let mut hash = Sha256::new();
    hash.update(&compiled.bytecode);
    hash.update(entry.interval_ms.to_le_bytes());
    hash.update(
        serde_json::to_vec(&(&entry.inputs, &entry.outputs, &entry.published)).unwrap_or_default(),
    );
    if !compiled.data.is_empty() {
        hash.update(serde_json::to_vec(&compiled.data).unwrap_or_default());
    }
    hex::encode(&hash.finalize()[..6])
}

/// What identifies a build of a program in a configured runtime (ADR-013 §2.19): as
/// [`version_of`], with the task the runtime runs it in, as configured, in place of the
/// program's own interval, which such a runtime does not use. A change of the task (its kind,
/// priority, watchdog, or the programs it runs and their order) changes the versions of its
/// programs; a change of the unused interval does not.
fn version_in_runtime(
    entry: &ProgramEntry,
    compiled: &mythos_logic_compiler::Compiled,
    task: &TaskConfig,
) -> String {
    let mut hash = Sha256::new();
    hash.update(&compiled.bytecode);
    hash.update(serde_json::to_vec(task).unwrap_or_default());
    hash.update(
        serde_json::to_vec(&(&entry.inputs, &entry.outputs, &entry.published)).unwrap_or_default(),
    );
    if !compiled.data.is_empty() {
        hash.update(serde_json::to_vec(&compiled.data).unwrap_or_default());
    }
    hex::encode(&hash.finalize()[..6])
}

/// The compiler's data blocks of a program in the protocol's form, field by field (the two
/// have the same JSON; a field added to one and not the other does not compile here).
fn data_uses(compiled: &mythos_logic_compiler::Compiled) -> Vec<DataUse> {
    compiled
        .data
        .iter()
        .map(|d| DataUse {
            name: d.name.clone(),
            ty: d.ty.clone(),
            layout: d.layout.clone(),
            offset: d.offset,
            size: d.size,
            constant: d.constant,
            members: d
                .members
                .iter()
                .map(|m| DataMember {
                    name: m.name.clone(),
                    ty: m.ty.clone(),
                    offset: m.offset,
                    size: m.size,
                    written: m.written,
                })
                .collect(),
        })
        .collect()
}

/// The runtime's data blocks for a program: one for each block it declares, laid out as it
/// was built with, retained as `blocks.json` says of the block of that name (the compiler
/// gives the project's spelling).
fn instance_data(
    compiled: &mythos_logic_compiler::Compiled,
    blocks: &[BlockEntry],
) -> Result<Vec<RuntimeDataBlock>, String> {
    compiled
        .data
        .iter()
        .map(|d| {
            let entry = blocks
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(&d.name))
                .ok_or_else(|| {
                    format!(
                        "data block '{}' was compiled in but is not in data/blocks.json",
                        d.name
                    )
                })?;
            Ok(RuntimeDataBlock {
                name: d.name.clone(),
                ty: d.ty.clone(),
                layout: d.layout.clone(),
                size: d.size,
                members: d
                    .members
                    .iter()
                    .map(|m| DataBlockMember {
                        name: m.name.clone(),
                        ty: m.ty.clone(),
                        offset: m.offset,
                        size: m.size,
                    })
                    .collect(),
                retain: entry.retain,
            })
        })
        .collect()
}

/// Compile `source` for the entry's target: the answer of `check`.
fn check(entry: &ProgramEntry, source: &str, project: &Project) -> Value {
    check_in(entry, source, project, None)
}

/// [`check`], of a program that a configured runtime runs in `task` when one is given: its
/// version is then its version in that runtime ([`version_in_runtime`]).
fn check_in(
    entry: &ProgramEntry,
    source: &str,
    project: &Project,
    task: Option<&TaskConfig>,
) -> Value {
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
    let compiling = compile_entry(entry, source, &target, project);
    let mut answer = match compiling.compiled {
        Ok(compiled) => {
            let mut problems = check_bindings(entry, &compiled.symbols);
            if !compiled.data.is_empty() {
                // What the core refuses of the program's data, said as its build would say it.
                // Only the fields `check_data` reads: this program is never answered.
                let program = RuntimeProgram {
                    name: compiled.name.clone(),
                    built_from: None,
                    created: None,
                    version: String::new(),
                    interval_ms: entry.interval_ms,
                    code: String::new(),
                    symbols: Value::Null,
                    inputs: entry.inputs.clone(),
                    outputs: entry.outputs.clone(),
                    published: entry.published.clone(),
                    data: data_uses(&compiled),
                };
                let refused = instance_data(&compiled, &project.blocks)
                    .and_then(|data| check_data(&[&program], &data));
                if let Err(m) = refused {
                    problems.push(m);
                }
            }
            let mut answer = json!({
                "ok": true,
                "diagnostics": [],
                "program": compiled.name,
                // The version a build of this text with these settings would have.
                "version": match task {
                    Some(task) => version_in_runtime(entry, &compiled, task),
                    None => version_of(entry, &compiled),
                },
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
            if !compiled.data.is_empty() {
                answer["data"] = json!(compiled.data);
            }
            answer
        }
        Err(diagnostics) => json!({ "ok": false, "diagnostics": diagnostics }),
    };
    if let Some(problems) = compiling.project_problems {
        answer["project_problems"] = json!(problems);
    }
    answer
}

/// `body` with the project's problems, when there are any.
fn with_problems(mut body: Value, problems: &Option<Vec<Diagnostic>>) -> Value {
    if let Some(problems) = problems {
        body["project_problems"] = json!(problems);
    }
    body
}

/// The bindings of `entry` to tags nothing provides now, each said as a warning. Said, not
/// refused: the connection that provides a tag may be configured later, and a program reading
/// it sees it as not GOOD until then. Nothing is said when nothing is known of the tags.
fn unregistered(entry: &ProgramEntry, registered: Option<&[String]>) -> Vec<String> {
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
    warnings
}

/// Compile the saved source and pack it as the runtime instance the core loads.
fn build(
    entry: &ProgramEntry,
    source: &str,
    registered: Option<&[String]>,
    project: &Project,
) -> Result<Value, Refusal> {
    check_entry(entry).map_err(|e| error(400, e))?;
    let target = TargetProfile::by_name(&entry.target)
        .ok_or_else(|| error(400, format!("'{}' is not a target profile", entry.target)))?;
    let compiling = compile_entry(entry, source, &target, project);
    let problems_of_project = compiling.project_problems;
    let compiled = compiling.compiled.map_err(|diagnostics| {
        (
            400,
            with_problems(
                json!({
                    "error": "the program does not compile and was not built",
                    "diagnostics": diagnostics,
                }),
                &problems_of_project,
            ),
        )
    })?;
    let problems = check_bindings(entry, &compiled.symbols);
    if !problems.is_empty() {
        return Err((
            400,
            with_problems(
                json!({
                    "error": "the program's bindings do not match its variables and it was not built",
                    "binding_problems": problems,
                }),
                &problems_of_project,
            ),
        ));
    }

    let version = version_of(entry, &compiled);

    let program = RuntimeProgram {
        name: compiled.name.clone(),
        // The program's name in the project, never the PROGRAM its source declares.
        built_from: Some(entry.name.clone()),
        created: entry.created.clone(),
        version: version.clone(),
        interval_ms: entry.interval_ms,
        code: hex::encode(&compiled.bytecode),
        symbols: serde_json::to_value(&compiled.symbols).map_err(|e| error(500, e))?,
        inputs: entry.inputs.clone(),
        outputs: entry.outputs.clone(),
        published: entry.published.clone(),
        data: data_uses(&compiled),
    };
    let data = instance_data(&compiled, &project.blocks).map_err(|e| error(500, e))?;
    // What the core would refuse of the instance's data is not built: a block named like the
    // program, a tag read into a member, and whatever else the core's own check says.
    check_data(&[&program], &data).map_err(|m| {
        (
            400,
            with_problems(json!({ "error": m }), &problems_of_project),
        )
    })?;

    let warnings = unregistered(entry, registered);

    let mut instance = json!({
        "id": entry.name,
        "module": entry.runtime,
        "run": entry.run,
        "program": program,
    });
    // Only when there is one: an instance of today has no `data` key.
    if !data.is_empty() {
        instance["data"] = json!(data);
    }
    Ok(with_problems(
        json!({
            "version": version,
            "warnings": warnings,
            "code_size": compiled.code_size,
            "memory_size": compiled.memory_size,
            "instance": instance,
        }),
        &problems_of_project,
    ))
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

/// The body `save_logic_runtime` takes: a runtime without its id, which is the path's.
fn runtime_schema() -> Value {
    json!({
        "type": "object",
        "required": ["module", "target", "run", "tasks"],
        "properties": {
            "module": { "type": "string", "description": "The runtime module that runs it, for example vm" },
            "target": { "type": "string", "description": "The target profile its programs are compiled for, for example pc" },
            "run": { "type": "boolean", "description": "Whether it is put in RUN when it is loaded" },
            "tasks": { "type": "array", "minItems": 1, "description": "Its tasks; every program of the runtime is in exactly one",
                "items": {
                    "type": "object",
                    "required": ["name", "kind", "priority", "programs"],
                    "properties": {
                        "name": { "type": "string", "description": "1 to 64 letters, digits, - or _" },
                        "kind": { "type": "object", "required": ["type"],
                            "description": "cyclic {interval_ms, phase_ms (default 0)}, cycle {min_cycle_ms (default 0)}, startup {}, or event {tag}: at each rising edge of a BOOL tag",
                            "properties": {
                                "type": { "type": "string", "enum": ["cyclic", "cycle", "startup", "event"] },
                                "interval_ms": { "type": "integer", "minimum": 1 },
                                "phase_ms": { "type": "integer", "minimum": 0 },
                                "min_cycle_ms": { "type": "integer", "minimum": 0 },
                                "tag": { "type": "string" }
                            } },
                        "priority": { "type": "integer", "minimum": 0, "maximum": 31, "description": "0 is the highest, 31 the lowest" },
                        "watchdog_ms": { "type": "integer", "minimum": 1, "description": "How long the task may owe a run before the runtime is stopped; absent: the runtime's default" },
                        "programs": { "type": "array", "items": { "type": "string" }, "description": "Programs of the project, in the order they run" }
                    }
                } }
        }
    })
}

/// The body `check_logic_runtime` takes: none, or a whole runtime to check in place of the
/// stored one.
fn runtime_check_schema() -> Value {
    let mut schema = runtime_schema();
    if let Some(o) = schema.as_object_mut() {
        o.remove("required");
        o.insert(
            "description".into(),
            json!("Optional: a configuration to check in place of the stored one, with all four of module, target, run and tasks; it is not saved"),
        );
    }
    schema
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
        description: "Control programs in Structured Text, Ladder Diagram, Function Block Diagram or Sequential Function Chart: their source, their bindings to tags, check and build; and the project's library of functions and function blocks".into(),
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
                "Remove a program and its source from a project. A runtime that already runs it is not touched. A program made again under its name is another program to the core (each program is marked when it is made, and its builds carry the mark since 0.8.0): it is not changed online from the deleted one's running build, nor given the RETAIN values kept for it.",
                None, &[]),
            route("POST", "/programs/:program/check", "viewer", "check_logic_program",
                "Compile a program, Structured Text, Ladder Diagram, Function Block Diagram or Sequential Function Chart, without building it: every mistake in it as a diagnostic with the range it is about (line, col, end_line, end_col, counted from 1, the end exclusive; all 0 when it is about the whole program; in a diagram the range is the element's tag in the document and 'element' names it: pou, local_id, x, y, or the interface's variable, or a chart's action or transition by name, or ('inline') the local_id of the action or transition of the chart whose inline body in LD or FBD the element is in, and the characters of its text, with the line of the text when it has several), or its variables (hidden: made by the diagram front-end for itself), what its bindings get wrong and the version a build of it would have. A Ladder Diagram that compiles also gives 'power', to watch for power flow: for each output of an element that carries power ('output') and each input where links join ('input'), the hidden variable assigned when it is evaluated in the scan (or on: ON whenever its network runs); for each network ('network': its elements' local_ids), the variable that is TRUE when it ran in the last scan. A Function Block Diagram that compiles gives 'values' of the same shape: for every output of every element (a block's by its formal parameter, ENO among them; '' for a variable's), the hidden variable holding the value it gave when it was evaluated in the scan, of the output's own type (or on: ENO always TRUE), with 'ran' where the value is computed only where that BOOL is TRUE (a function under EN); for each network the variable that is TRUE when it ran. A Sequential Function Chart that compiles gives 'chart' of the same shape: for each step ('step': 'X') the variable that is TRUE while it is active and ('step': 'T') its elapsed time, a TIME; for each transition ('transition': true) the variable holding its condition, with 'ran', the variable that is TRUE when the transition was enabled and its condition evaluated in the last scan; for each action of an action block ('action': 'Q', by the action's local_id) the variable holding the state of the action it names. A chart also gives 'bodies': for every action and transition condition written in LD or FBD, the entries of its 'power' (LD) or 'values' (FBD), each with 'body', the body it is in: {'action': name} (an action of the POU), {'transition': name} (a transition of the POU), {'inline': local_id} (written in the action of an action block or in the transition of that local_id), or {'wired': local_id} (a network drawn in the chart that gives that transition its condition), with 'language' ld or fbd; a body's elements' local_ids are its own, and its networks' variables are FALSE where the body did not run in the scan (an action whose Q was FALSE, a transition not enabled). With 'source' it checks that text instead of the saved one.",
                Some(json!({
                    "type": "object",
                    "properties": { "source": { "type": "string" } }
                })), &[]),
            route("POST", "/programs/:program/build", "engineer", "build_logic_program",
                "Compile the saved source, Structured Text, Ladder Diagram, Function Block Diagram or Sequential Function Chart, and pack it as a runtime instance. The answer's 'instance' is what save_runtimes_config takes in its 'instances' (replace the one with the same id, and take out any instance of a configured runtime the program was in before, which the core still runs until it is taken out or rebuilt without the program: the core refuses the save while that instance holds the program, when that instance was built by this module since 0.8.0; one built before is not compared, and runs the program too until it is taken out); nothing is loaded by this call. The program is compiled with the project's library and its user types and data blocks (the data module's); when those types and blocks do not check, it is compiled without them and 'project_problems' says why. When the program declares data blocks, the instance carries them in 'data'. A data block in two runtimes is refused when saved. A program that a configured runtime of the project runs (list_logic_runtimes) is not built alone: it is refused with 409 naming the runtime, which build_logic_runtime builds with all its programs.",
                None, &["tags"]),
            route("GET", "/library", "viewer", "list_logic_library",
                "The project's library: Structured Text sources of functions and function blocks shared by every program, by name. Reads only; nothing is written, compiled or loaded.", None, &[]),
            route("GET", "/library/:name", "viewer", "get_logic_library_source",
                "A source of the project's library, by name (without regard to case; answered with its stored spelling): Structured Text holding functions and function blocks. Reads only.", None, &[]),
            route("PUT", "/library/:name", "engineer", "save_logic_library_source",
                "Make or replace a source of the project's library: Structured Text holding FUNCTIONs and FUNCTION_BLOCKs, no PROGRAM and no TYPE (the project's types are made in the data module). Every program is compiled with every library source, and a mistake in one refuses every program, so check it first with check_logic_library. Nothing is compiled or loaded by this call; a name that differs only in case from a stored one is refused.",
                Some(json!({
                    "type": "object",
                    "required": ["source"],
                    "properties": { "source": { "type": "string", "description": "Structured Text: FUNCTION … END_FUNCTION and FUNCTION_BLOCK … END_FUNCTION_BLOCK" } }
                })), &[]),
            route("DELETE", "/library/:name", "engineer", "delete_logic_library_source",
                "Remove a source from the project's library. Every program is then compiled without it: one that uses what it held no longer compiles. Nothing is built or loaded.", None, &[]),
            route("POST", "/library/check", "viewer", "check_logic_library",
                "Check the project's library with the compiler, with the project's user types and data blocks, without saving or building anything: the stored sources, or with 'name' and 'source' (both or neither) that one source's text put in, new or replacing. Answers 'ok' and the diagnostics, each with 'source' naming its file (logic/library/<name>.st or data/types/<Name>.st); when the project's types and data blocks do not check, the library is checked without them and 'project_problems' says why.",
                Some(json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "A library source's name; give source with it" },
                        "source": { "type": "string", "description": "That source's text; give name with it" }
                    }
                })), &[]),
            route("GET", "/runtimes", "viewer", "list_logic_runtimes",
                "The project's runtimes as authored in the logic module (logic/tasks.json): 'runtimes', each configured one with its module, target, run and tasks; 'implied', the runtime each program named by no configured runtime runs alone in (its id the program's name, one cyclic task at the program's interval, priority 31, the task and its program named by the PROGRAM the saved source declares, as the core names them, or by the program's name when that source does not compile), derived and never written; and 'problems', what the stored files break of the rules (after a hand edit). Reads only; nothing is written, built or loaded.", None, &[]),
            route("GET", "/runtimes/:runtime", "viewer", "get_logic_runtime",
                "One runtime of the project, configured or implied (a program running alone, by its name), with 'implied' saying which. Ids are compared without regard to case; the answer has the stored spelling. Reads only; nothing is written, built or loaded.", None, &[]),
            route("PUT", "/runtimes/:runtime", "engineer", "save_logic_runtime",
                "Make or replace a configured runtime of the project: the runtime module that runs it, the target profile its programs are compiled for, whether it is put in RUN when loaded, and its tasks, each naming programs of the project in the order they run (stored in the spelling of the program list). Refused, and nothing saved, when the id is a device name on Windows, differs only in case from another runtime's, or is a program's name; when a task names a program the project lacks or a program of another runtime; or when the tasks do not check as the core checks them. Nothing is built or loaded by this call: a runtime the core runs keeps running what it was built with, a program taken out of it included, until it is built again and saved with save_runtimes_config; a save of another runtime's build that holds such a program is refused by the core until then, when both were built by this module since 0.8.0.",
                Some(runtime_schema()), &[]),
            route("DELETE", "/runtimes/:runtime", "engineer", "delete_logic_runtime",
                "Remove a configured runtime from the project's configuration. Nothing is loaded, and nothing that runs is touched: if the core runs the runtime, its instance (the runtime's id) runs on, with all its programs, until it is taken out with save_runtimes_config. Each of its programs is then built alone again (build_logic_program) and runs alone only once that build is saved, after the runtime's instance is taken out (the core refuses that build beside the runtime's instance when both were built by this module since 0.8.0). An id that is not a runtime id is taken when a runtime is stored under it (a hand edit of logic/tasks.json).", None, &[]),
            route("POST", "/runtimes/:runtime/check", "viewer", "check_logic_runtime",
                "Check a runtime of the project without building or saving anything: each of its programs compiled from its saved source for the runtime's target, with its diagnostics, binding problems and its version in the runtime ('programs'); and 'problems', what the configuration breaks and what the core would refuse of the runtime as a whole (its tasks, its data blocks, an output or a published tag of two programs, an event task's tag no program reads into a BOOL). 'ok' when it would build; then 'version', the runtime's. 'data' is the data blocks its programs declare, each once. With a body (module, target, run and tasks, all four) that configuration is checked in place of the stored one, and nothing is saved. A program running alone is checked by its name as a runtime of its own. Nothing is loaded.",
                Some(runtime_check_schema()), &[]),
            route("POST", "/runtimes/:runtime/build", "engineer", "build_logic_runtime",
                "Build a runtime of the project: every program of it compiled from its saved source, in one snapshot of the project, for the runtime's target, packed as one runtime instance with its tasks and the data blocks its programs declare, each once. Answers 'instance', 'version' (the runtime's, as the core reports it once loaded: its programs' versions joined by ', '), 'programs' (each program's version in the runtime, code_size and memory_size), 'warnings', and 'replaces': the ids those programs run under alone. Nothing is loaded by this call: save 'instance' with save_runtimes_config in its 'instances', in place of the one with the same id, and take out the instances named in 'replaces' in the same save (the core refuses a data block or an output tag in two runtimes). 'replaces' does not name another configured runtime a program of this one was in before, which this module does not know to be loaded: if the core still runs one, take it out or build it again without the program and save it in the same save. The core refuses a save in which one program of the project is in two runtimes, when both instances name it as built by this module (every build since 0.8.0 does; an instance built before does not, and the program then runs in both until it is taken out). The core keeps RETAIN values per runtime id and per program: a warm load gives a program only the values kept under this runtime's id for it, of variables of the same name, type and size, never those kept for another program (a program deleted and made again under its name is another program), nor those kept while it ran in another runtime; the one exception is values kept before the core kept them per program, given once, as they are, to whatever the runtime runs; a cold load starts them from their initial values. A program running alone, by its name, builds exactly as build_logic_program builds it.",
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

/// The name and data of an event a call sends when it changed something.
type Event = (&'static str, Value);

impl Logic {
    /// Answer `call`, with the event to send when it changed something. [`Service::handle`]
    /// is this, with the event sent.
    pub async fn serve(&self, call: &HttpCall) -> (HttpReply, Option<Event>) {
        let Some(data) = call.data_dir.as_ref().map(PathBuf::from) else {
            return (
                HttpReply::error(500, "the server gave the logic module no data directory"),
                None,
            );
        };
        // A program's name becomes a file name: nothing but a valid name gets that far.
        if let Some(name) = call.params.get("program") {
            if !valid_name(name) {
                return (
                    HttpReply::error(400, format!("'{name}' is not a program name")),
                    None,
                );
            }
        }
        // So does a library source's.
        if let Some(name) = call.params.get("name") {
            if let Some(problem) = library_name_problem(name) {
                return (HttpReply::error(400, problem), None);
            }
        }
        // A runtime's id is the core's runtime id. It names no file of this module, so a
        // runtime stored under another id (a hand edit) can still be deleted by it: the route
        // refuses the id when no runtime is stored under it.
        if let Some(id) = call.params.get("runtime") {
            if let Some(problem) = runtimes::id_problem(id) {
                if !(call.method == "DELETE" && call.route == "/runtimes/:runtime") {
                    return (HttpReply::error(400, problem), None);
                }
            }
        }
        let _guard = self.writes.lock().await;
        // A write also holds the lock between processes (see [`LOCK_FILE`]). A read does not:
        // it writes nothing, and makes no file.
        let _lock = if only_reads(call) {
            None
        } else {
            // In a project with no module directory yet, the lock would make the directory and
            // the lock file even for a write that is refused. So the call is first tried with
            // every write held back: one that reaches no write answers as it is (it read only
            // files that do not exist, which is what a read sees); one that reaches a write
            // wrote nothing and is made again below, under the lock, from the start.
            if matches!(std::fs::symlink_metadata(&data), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
            {
                if let Some(answer) = without_writes(|| self.handle_in(call, &data)) {
                    return answer;
                }
            }
            match lock_writes(&data) {
                Ok(file) => Some(file),
                Err((status, body)) => return (HttpReply::json(status, body), None),
            }
        };
        self.handle_in(call, &data)
    }

    fn handle_in(&self, call: &HttpCall, data: &Path) -> (HttpReply, Option<Event>) {
        let project = call.project.clone().unwrap_or_default();
        let name = call.params.get("program").map(String::as_str).unwrap_or("");
        let event = std::cell::RefCell::new(None);
        let changed = |what: &str, name: &str| {
            *event.borrow_mut() = Some((
                PROGRAMS_CHANGED_EVENT,
                json!({ "project": project, "program": name, "what": what }),
            ));
        };
        let reply = match (call.method.as_str(), call.route.as_str()) {
            ("GET", "/programs") => reply(load(data).map(|p| json!({ "programs": p.programs })), 200),
            ("POST", "/programs") => reply(
                body::<Create>(call).and_then(|b| {
                    if !valid_name(&b.name) {
                        return Err(error(
                            400,
                            format!("'{}' is not a program name: a letter, then letters, digits or _, at most 64", b.name),
                        ));
                    }
                    // A new name only: a program that already has such a name still reads,
                    // checks, builds and deletes.
                    if device_name(&b.name) {
                        return Err(error(
                            400,
                            format!("'{}' cannot be a program name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)", b.name),
                        ));
                    }
                    let mut programs = load(data)?;
                    if programs.programs.iter().any(|p| p.name.eq_ignore_ascii_case(&b.name)) {
                        return Err(error(409, format!("the project already has a program '{}'", b.name)));
                    }
                    let configured = runtimes::load(data)?;
                    if let Some(r) = configured.runtimes.iter().find(|r| r.id.eq_ignore_ascii_case(&b.name)) {
                        return Err(error(
                            409,
                            format!("the project has a runtime '{}'; a program is not named like a runtime", r.id),
                        ));
                    }
                    let now = std::time::SystemTime::now();
                    let entry = ProgramEntry {
                        name: b.name.clone(),
                        created: Some(making_mark(&b.name, now)),
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
                    let source = template(&b.name, &entry.language, now);
                    write_synced(&source_path(data, &entry), source.as_bytes())?;
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
                    write_synced(&source_path(data, &entry), b.source.as_bytes())?;
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
                        created: programs.programs[at].created.clone(),
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
                    if let Some((runtime, task)) = runtimes::assigned(&runtimes::load(data)?, name) {
                        return Err(error(
                            409,
                            format!("program '{name}' is in task '{task}' of runtime '{runtime}': take it out of the runtime first"),
                        ));
                    }
                    programs.programs.retain(|p| p.name != name);
                    save(data, &programs)?;
                    let _ = remove_written(&source_path(data, &entry));
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
                    let configured = runtimes::load(data)?;
                    let project = read_project(data)?;
                    Ok(match runtimes::assignment(&configured, name) {
                        // In a configured runtime: compiled for its target, and the version is
                        // the one that runtime's build gives the program.
                        Some((runtime, task)) => {
                            let mut in_runtime = entry.clone();
                            in_runtime.target.clone_from(&runtime.target);
                            check_in(&in_runtime, &source, &project, Some(task))
                        }
                        None => check(entry, &source, &project),
                    })
                }),
                200,
            ),
            ("POST", "/programs/:program/build") => reply(
                load(data).and_then(|p| {
                    let entry = find(&p, name)?;
                    // A program of a configured runtime is built only with it: built alone, it
                    // would be an instance the caller saves beside the runtime that runs it.
                    if let Some((runtime, task)) = runtimes::assignment(&runtimes::load(data)?, name) {
                        return Err((
                            409,
                            json!({
                                "error": match runtimes::id_problem(&runtime.id) {
                                    None => format!(
                                        "program '{name}' is in task '{}' of runtime '{}': build that runtime with build_logic_runtime",
                                        task.name, runtime.id
                                    ),
                                    // A hand edit: that runtime cannot be built, only deleted.
                                    Some(problem) => format!(
                                        "program '{name}' is in task '{}' of runtime '{}', which cannot be built: {problem}; take it out with delete_logic_runtime",
                                        task.name, runtime.id
                                    ),
                                },
                                "runtime": runtime.id,
                            }),
                        ));
                    }
                    build(
                        entry,
                        &read_source(data, entry)?,
                        call.tags.as_deref(),
                        &read_project(data)?,
                    )
                }),
                200,
            ),
            (_, route) if route == "/library" || route.starts_with("/library/") => {
                let (reply, sent) = library(call, data);
                return (reply, sent.map(|e| (LIBRARY_CHANGED_EVENT, e)));
            }
            (_, route) if route == "/runtimes" || route.starts_with("/runtimes/") => {
                let (reply, sent) = runtimes::runtimes(call, data);
                return (reply, sent.map(|e| (RUNTIMES_CHANGED_EVENT, e)));
            }
            (method, route) => HttpReply::error(404, format!("the logic module has no route {method} {route}")),
        };
        (reply, event.into_inner())
    }
}

#[derive(Deserialize, Default)]
struct LibraryCheckBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

/// The routes of the project's library: the reply, and the data of the event to send when a
/// source was saved or deleted.
fn library(call: &HttpCall, data: &Path) -> (HttpReply, Option<Value>) {
    let project = call.project.clone().unwrap_or_default();
    let name = call.params.get("name").map(String::as_str).unwrap_or("");
    let mut event = None;
    let result = match (call.method.as_str(), call.route.as_str()) {
        ("GET", "/library") => library_names(data).map(|names| {
            json!({ "sources": names.iter().map(|stem| json!({ "name": stem })).collect::<Vec<_>>() })
        }),
        ("GET", "/library/:name") => (|| {
            let stem = stored_library_name(data, name)?.ok_or_else(|| no_library_source(name))?;
            let path = library_path(data, &stem);
            let source = std::fs::read_to_string(&path)
                .map_err(|e| error(500, format!("{}: {e}", path.display())))?;
            Ok(json!({ "name": stem, "source": source }))
        })(),
        ("PUT", "/library/:name") => body::<SourceBody>(call).and_then(|b| {
            let stored = stored_library_name(data, name)?;
            if let Some(other) = stored.as_deref().filter(|s| *s != name) {
                return Err(other_spelling(other));
            }
            write_synced(&library_path(data, name), b.source.as_bytes())?;
            event = Some(json!({ "project": project, "name": name, "what": "saved" }));
            Ok(json!({ "name": name, "created": stored.is_none() }))
        }),
        ("DELETE", "/library/:name") => (|| {
            let stem = stored_library_name(data, name)?.ok_or_else(|| no_library_source(name))?;
            // The listed spelling's file, never the request's: on Linux another spelling is
            // another file.
            let path = library_path(data, &stem);
            remove_written(&path)?;
            event = Some(json!({ "project": project, "name": stem, "what": "deleted" }));
            Ok(json!({ "deleted": stem }))
        })(),
        ("POST", "/library/check") => (|| {
            let given = match &call.body {
                None | Some(Value::Null) => LibraryCheckBody::default(),
                Some(_) => body::<LibraryCheckBody>(call)?,
            };
            let draft = match (given.name, given.source) {
                (None, None) => None,
                (Some(n), Some(t)) => {
                    if let Some(problem) = library_name_problem(&n) {
                        return Err(error(400, problem));
                    }
                    if let Some(other) =
                        stored_library_name(data, &n)?.filter(|s| *s != n)
                    {
                        return Err(other_spelling(&other));
                    }
                    Some((n, t))
                }
                _ => return Err(error(400, "give both name and source, or neither")),
            };
            Ok(check_library(read_project(data)?, draft))
        })(),
        (method, route) => Err(error(
            404,
            format!("the logic module has no route {method} {route}"),
        )),
    };
    (reply(result, 200), event)
}

fn no_library_source(name: &str) -> Refusal {
    error(404, format!("the project has no library source '{name}'"))
}

fn other_spelling(existing: &str) -> Refusal {
    error(
        409,
        format!(
            "the project already has a library source '{existing}'; names that differ only in case are one name"
        ),
    )
}

/// The project's library checked with the compiler, with `draft` (a source's name and text)
/// put in, new or replacing: what every program would be refused for.
fn check_library(mut project: Project, draft: Option<(String, String)>) -> Value {
    if let Some((name, text)) = draft {
        let source = format!("logic/library/{name}{SOURCE_EXTENSION}");
        // A stored file that is not text is replaced like any other.
        project
            .unreadable_library
            .retain(|d| d.source.as_deref() != Some(source.as_str()));
        match project.library.iter_mut().find(|(n, _)| *n == source) {
            Some(stored) => stored.1 = text,
            None => {
                project.library.push((source, text));
                project
                    .library
                    .sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
            }
        }
    }
    let given = given(&project);
    let mut diagnostics = library_refusals(&project);
    if diagnostics.is_empty() {
        if let Err(found) = check_project(&Sources {
            library: &given.library,
            data: &given.data,
            ..Default::default()
        }) {
            diagnostics = found;
        }
    }
    with_problems(
        json!({ "ok": diagnostics.is_empty(), "diagnostics": diagnostics }),
        &given.problems,
    )
}

#[async_trait]
impl Service for Logic {
    async fn handle(&self, call: HttpCall, events: &Events) -> HttpReply {
        let (reply, event) = self.serve(&call).await;
        if let Some((name, data)) = event {
            events.send(name, data);
        }
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A check of a project of today: no library, no types, no data blocks.
    fn check(entry: &ProgramEntry, source: &str) -> Value {
        super::check(entry, source, &Project::default())
    }

    /// A build in a project of today.
    fn build(
        entry: &ProgramEntry,
        source: &str,
        registered: Option<&[String]>,
    ) -> Result<Value, Refusal> {
        super::build(entry, source, registered, &Project::default())
    }

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

    // -------------------------------------------------------------------------
    // The project's library, types and data blocks (D3)
    // -------------------------------------------------------------------------

    const SHARED: &str =
        "TYPE Shared : STRUCT level : INT; seen : INT; spare : INT := 5; END_STRUCT; END_TYPE\n";

    /// Writes `S.level` from its input; nothing else of S.
    const WRITER: &str = "PROGRAM Writer
VAR_INPUT level : INT; END_VAR
VAR_EXTERNAL S : Shared; END_VAR
  S.level := level;
END_PROGRAM";

    const MOTORS: &str =
        "FUNCTION Twice : INT\nVAR_INPUT x : INT; END_VAR\n  Twice := x * 2;\nEND_FUNCTION\n";

    fn writer_entry() -> ProgramEntry {
        serde_json::from_value(json!({
            "name": "Writer", "language": "st", "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true,
            "inputs": [{"variable": "level", "tag": "external.P.LT1.level"}]
        }))
        .unwrap()
    }

    /// A project's directory as the core makes it: the module's own `logic`, and beside it
    /// the data module's `data`.
    struct Dir {
        root: tempfile::TempDir,
        logic: PathBuf,
    }

    fn project_dir() -> Dir {
        let root = tempfile::tempdir().unwrap();
        let logic = root.path().join("logic");
        std::fs::create_dir_all(&logic).unwrap();
        Dir { root, logic }
    }

    impl Dir {
        fn put(&self, path: &[&str], text: &str) {
            let file = path
                .iter()
                .fold(self.root.path().to_path_buf(), |p, s| p.join(s));
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }

        fn types(&self, text: &str) {
            self.put(&["data", "types", "Shared.st"], text);
        }

        fn blocks(&self, blocks: &[(&str, &str, bool)]) {
            let blocks: Vec<Value> = blocks
                .iter()
                .map(|(n, t, r)| json!({"name": n, "type": t, "retain": r}))
                .collect();
            self.put(
                &["data", "blocks.json"],
                &json!({ "blocks": blocks }).to_string(),
            );
        }

        fn library(&self, name: &str, text: &str) {
            self.put(&["logic", "library", &format!("{name}.st")], text);
        }

        fn project(&self) -> Project {
            read_project(&self.logic).unwrap()
        }
    }

    fn keys(v: &Value) -> Vec<&str> {
        let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        k.sort_unstable();
        k
    }

    /// The block `S` of type `Shared` as the data module answers it, with `retain` as a
    /// runtime's data block writes it (absent when false).
    fn block_s(types: &str, retain: bool) -> Value {
        let layouts = check_project(&Sources {
            library: &[("data/types/Shared.st", types)],
            data: &[DataBlock {
                name: "S",
                ty: "Shared",
            }],
            ..Default::default()
        })
        .unwrap();
        let l = &layouts[0];
        let mut b = json!({
            "name": l.name, "type": l.ty, "layout": l.layout, "size": l.size,
            "members": l.members.iter().map(|m| json!({
                "name": m.name, "type": m.ty, "offset": m.offset, "size": m.size
            })).collect::<Vec<_>>(),
        });
        if retain {
            b["retain"] = json!(true);
        }
        b
    }

    fn call(
        method: &str,
        route: &str,
        name: Option<&str>,
        body: Option<Value>,
        logic: &Path,
    ) -> HttpCall {
        serde_json::from_value(json!({
            "method": method, "route": route,
            "params": name.map_or(json!({}), |n| json!({"name": n})),
            "body": body, "project": "plant", "data_dir": logic.display().to_string(),
        }))
        .unwrap()
    }

    #[test]
    fn a_project_of_today_checks_and_builds_with_exactly_todays_keys() {
        let dir = project_dir();
        // What a project gets once any call reaches the data module (an empty `data`), and an
        // empty library: neither is anything.
        std::fs::create_dir_all(dir.root.path().join("data").join("types")).unwrap();
        std::fs::create_dir_all(dir.logic.join("library")).unwrap();
        let project = dir.project();
        assert!(project.is_empty(), "{project:?}");

        let checked = super::check(&entry(), TANK, &project);
        assert_eq!(
            keys(&checked),
            [
                "binding_problems",
                "code_size",
                "diagnostics",
                "memory_size",
                "ok",
                "program",
                "variables",
                "version"
            ],
            "{checked}"
        );
        let built = super::build(&entry(), TANK, None, &project).unwrap();
        assert_eq!(
            keys(&built),
            [
                "code_size",
                "instance",
                "memory_size",
                "version",
                "warnings"
            ]
        );
        assert_eq!(keys(&built["instance"]), ["id", "module", "program", "run"]);
        assert_eq!(
            keys(&built["instance"]["program"]),
            [
                "built_from",
                "code",
                "inputs",
                "interval_ms",
                "name",
                "outputs",
                "published",
                "symbols",
                "version"
            ]
        );
        // The versions these programs had before D3, read from the tree at 4bd8f15.
        assert_eq!(built["version"], "d3adaba64d45");
        assert_eq!(checked["version"], "d3adaba64d45");
        for (e, source, version) in [
            (ld_entry(), ladder("run"), "603a39003bec"),
            (fbd_entry(), diagram("level"), "018750a43b66"),
            (sfc_entry(), chart("90"), "a1226c2936b9"),
        ] {
            assert_eq!(
                super::build(&e, &source, None, &project).unwrap()["version"],
                version,
                "{}",
                e.language
            );
        }
    }

    /// A project of today, with no task configuration, through the routes as the server calls
    /// them: each of the four programs whose versions are pinned above builds to that version
    /// with today's keys through the program's route and, byte for byte the same, through the
    /// runtime route of its implied runtime; its check is today's; and no call makes a file.
    #[tokio::test]
    async fn a_project_without_runtimes_builds_every_program_exactly_as_before() {
        fn files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
            let mut out = Vec::new();
            for e in std::fs::read_dir(dir).unwrap() {
                let path = e.unwrap().path();
                if path.is_dir() {
                    out.extend(files(&path));
                } else {
                    let bytes = std::fs::read(&path).unwrap();
                    out.push((path, bytes));
                }
            }
            out.sort();
            out
        }
        for (e, source, version) in [
            (entry(), TANK.to_string(), "d3adaba64d45"),
            (ld_entry(), ladder("run"), "603a39003bec"),
            (fbd_entry(), diagram("level"), "018750a43b66"),
            (sfc_entry(), chart("90"), "a1226c2936b9"),
        ] {
            let dir = project_dir();
            dir.put(
                &["logic", "programs.json"],
                &json!({ "programs": [e] }).to_string(),
            );
            let file = format!("Tank.{}", if e.language == "st" { "st" } else { "xml" });
            dir.put(&["logic", "programs", &file], &source);
            let before = files(dir.root.path());
            let logic = Logic::default();
            let serve = |route: &str, param: &str| {
                let call: HttpCall = serde_json::from_value(json!({
                    "method": "POST", "route": route, "params": { param: "Tank" },
                    "project": "plant", "data_dir": dir.logic.display().to_string(),
                }))
                .unwrap();
                let logic = &logic;
                async move { logic.serve(&call).await }
            };
            let (program, event) = serve("/programs/:program/build", "program").await;
            assert!(event.is_none());
            assert_eq!(program.status, 200, "{}", program.body);
            assert_eq!(program.body["version"], version, "{}", e.language);
            assert_eq!(
                keys(&program.body),
                [
                    "code_size",
                    "instance",
                    "memory_size",
                    "version",
                    "warnings"
                ]
            );
            assert_eq!(
                keys(&program.body["instance"]),
                ["id", "module", "program", "run"]
            );
            assert_eq!(
                program.body,
                build(&e, &source, None).unwrap(),
                "{}",
                e.language
            );
            let (runtime, _) = serve("/runtimes/:runtime/build", "runtime").await;
            assert_eq!(runtime.status, 200);
            assert_eq!(
                serde_json::to_vec(&runtime.body).unwrap(),
                serde_json::to_vec(&program.body).unwrap(),
                "{}",
                e.language
            );
            let (checked, _) = serve("/programs/:program/check", "program").await;
            assert_eq!(checked.body, check(&e, &source), "{}", e.language);
            assert_eq!(checked.body["version"], version);
            let (runtime_check, _) = serve("/runtimes/:runtime/check", "runtime").await;
            assert_eq!(runtime_check.body["version"], version);
            assert_eq!(
                files(dir.root.path()),
                before,
                "{}: a file was made",
                e.language
            );
        }
    }

    #[test]
    fn a_program_without_var_external_builds_the_same_bytes_and_version_beside_types_blocks_and_a_library(
    ) {
        let dir = project_dir();
        dir.types(SHARED);
        dir.blocks(&[("S", "Shared", true)]);
        dir.library("Motors", MOTORS);
        let project = dir.project();
        assert!(!project.is_empty());
        for (e, source) in [(entry(), TANK.to_string()), (ld_entry(), ladder("run"))] {
            let today = build(&e, &source, None).unwrap();
            let now = super::build(&e, &source, None, &project).unwrap();
            assert_eq!(now, today, "{}", e.language);
            let checked = super::check(&e, &source, &project);
            assert_eq!(checked, check(&e, &source), "{}", e.language);
        }
    }

    #[test]
    fn a_program_with_var_external_is_built_with_its_blocks_layout_and_retain() {
        let dir = project_dir();
        dir.types(SHARED);
        // T first and not retained: S's retain is S's own.
        dir.blocks(&[("T", "Shared", false), ("S", "Shared", true)]);
        let project = dir.project();
        let built = super::build(&writer_entry(), WRITER, None, &project).unwrap();
        assert!(built.get("project_problems").is_none(), "{built}");
        assert_eq!(
            built["instance"]["data"],
            json!([block_s(SHARED, true)]),
            "{built}"
        );
        let checked = super::check(&writer_entry(), WRITER, &project);
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["binding_problems"], json!([]), "{checked}");
        // The program carries the compiler's data, which the check shows: S.level written.
        assert_eq!(built["instance"]["program"]["data"], checked["data"]);
        let members = checked["data"][0]["members"].as_array().unwrap();
        let written: Vec<(&str, bool)> = members
            .iter()
            .map(|m| (m["name"].as_str().unwrap(), m["written"].as_bool().unwrap()))
            .collect();
        assert_eq!(
            written,
            [("level", true), ("seen", false), ("spare", false)]
        );
        assert_eq!(checked["version"], built["version"]);

        // Not retained: the runtime's block says nothing of retain.
        dir.blocks(&[("T", "Shared", true), ("S", "Shared", false)]);
        let built = super::build(&writer_entry(), WRITER, None, &dir.project()).unwrap();
        assert_eq!(built["instance"]["data"], json!([block_s(SHARED, false)]));
    }

    #[test]
    fn what_check_data_refuses_is_not_built() {
        let dir = project_dir();
        dir.types(SHARED);
        dir.blocks(&[("Recipe", "Shared", false), ("S", "Shared", false)]);
        let project = dir.project();
        // R3: a block named like the program.
        let recipe = "PROGRAM Recipe\nVAR_EXTERNAL Recipe : Shared; END_VAR\n  Recipe.level := 1;\nEND_PROGRAM";
        let mut e = writer_entry();
        e.name = "Recipe".into();
        e.inputs.clear();
        let said = "data block 'Recipe' has the name of a program of this runtime";
        let (status, refused) = super::build(&e, recipe, None, &project).unwrap_err();
        assert_eq!((status, refused), (400, json!({ "error": said })));
        let checked = super::check(&e, recipe, &project);
        assert_eq!(checked["binding_problems"], json!([said]), "{checked}");

        // R12: a tag read into a member.
        let mut e = writer_entry();
        e.inputs.push(
            serde_json::from_value(json!({"variable": "S.seen", "tag": "external.P.X.seen"}))
                .unwrap(),
        );
        let said = "program 'Writer': S.seen is a member of data block 'S', which only a program writes: a tag cannot be read into it";
        let (status, refused) = super::build(&e, WRITER, None, &project).unwrap_err();
        assert_eq!((status, refused), (400, json!({ "error": said })));
        let checked = super::check(&e, WRITER, &project);
        assert_eq!(checked["binding_problems"], json!([said]), "{checked}");
    }

    #[test]
    fn when_the_data_does_not_check_a_program_without_blocks_builds_and_one_with_blocks_says_why() {
        let dir = project_dir();
        dir.types(SHARED);
        dir.blocks(&[("S", "Missing", false)]);
        let project = dir.project();
        let problems = check_stored_data(&project).unwrap_err();
        assert!(
            problems.iter().any(|d| d.message.contains("Missing")),
            "{problems:?}"
        );
        let problems = json!(problems);

        let built = super::build(&entry(), TANK, None, &project).unwrap();
        assert_eq!(built["project_problems"], problems, "{built}");
        assert_eq!(built["version"], "d3adaba64d45");
        assert!(built["instance"].get("data").is_none());
        let checked = super::check(&entry(), TANK, &project);
        assert_eq!(checked["ok"], true);
        assert_eq!(checked["project_problems"], problems);

        let (status, refused) = super::build(&writer_entry(), WRITER, None, &project).unwrap_err();
        assert_eq!(status, 400);
        assert_eq!(refused["project_problems"], problems, "{refused}");
        let messages: Vec<&str> = refused["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["message"].as_str().unwrap())
            .collect();
        assert!(
            messages.contains(&"there is no data block S in the project"),
            "{refused}"
        );
        let checked = super::check(&writer_entry(), WRITER, &project);
        assert_eq!(checked["ok"], false);
        assert_eq!(checked["project_problems"], problems);

        // A blocks.json that is not the project's data blocks (L7), with no types at all.
        for (text, why) in [
            ("{\"blocks\": [", "EOF while parsing"),
            ("{\"blocks\": [], \"x\": 1}", "unknown field `x`"),
            (
                "{\"blocks\": [{\"name\": \"S\", \"type\": \"Shared\"}]}",
                "missing field `retain`",
            ),
        ] {
            let dir = project_dir();
            dir.put(&["data", "blocks.json"], text);
            let project = dir.project();
            let built = super::build(&entry(), TANK, None, &project).unwrap();
            assert_eq!(built["version"], "d3adaba64d45");
            let p = &built["project_problems"];
            assert_eq!(p.as_array().unwrap().len(), 1, "{built}");
            let message = p[0]["message"].as_str().unwrap();
            assert!(
                message
                    .starts_with("data/blocks.json cannot be read as the project's data blocks: ")
                    && message.contains(why),
                "{message}"
            );
            assert_eq!(p[0]["line"], 0);
            assert!(p[0].get("source").is_none());
        }
    }

    #[test]
    fn the_version_follows_the_data_layout() {
        let dir = project_dir();
        dir.types(SHARED);
        dir.blocks(&[("S", "Shared", false)]);
        let before = super::build(&writer_entry(), WRITER, None, &dir.project()).unwrap();
        // A member renamed, its type and place kept: the code may be alike, the block is not.
        dir.types(&SHARED.replace("seen", "watched"));
        let after = super::build(&writer_entry(), WRITER, None, &dir.project()).unwrap();
        assert_ne!(
            before["instance"]["data"][0]["layout"],
            after["instance"]["data"][0]["layout"]
        );
        assert_ne!(before["version"], after["version"]);
        // Retain is the runtime's: the version does not follow it.
        dir.blocks(&[("S", "Shared", true)]);
        let retained = super::build(&writer_entry(), WRITER, None, &dir.project()).unwrap();
        assert_eq!(retained["version"], after["version"]);
        assert_ne!(retained["instance"]["data"], after["instance"]["data"]);
    }

    #[test]
    fn a_library_source_is_compiled_into_every_program_and_its_mistakes_name_it() {
        let dir = project_dir();
        dir.library("Motors", MOTORS);
        let twice = TANK.replace("(setpoint - level) * 10", "Twice(setpoint - level) * 5");
        let built = super::build(&entry(), &twice, None, &dir.project()).unwrap();
        assert!(built.get("project_problems").is_none(), "{built}");
        // Without the library, it does not compile.
        let (status, _) = build(&entry(), &twice, None).unwrap_err();
        assert_eq!(status, 400);
        // The program's own mistake names its source once there are several.
        let wrong = "PROGRAM Tank\nVAR a : INT; END_VAR\n  a := b;\nEND_PROGRAM";
        let checked = super::check(&entry(), wrong, &dir.project());
        assert_eq!(checked["diagnostics"][0]["source"], "Tank.st", "{checked}");

        // A mistake in the library refuses a program that does not use it, and names its place.
        dir.library("Motors", &MOTORS.replace("x * 2", "y * 2"));
        let checked = super::check(&entry(), TANK, &dir.project());
        assert_eq!(checked["ok"], false);
        let d = &checked["diagnostics"][0];
        assert_eq!(d["source"], "logic/library/Motors.st", "{checked}");
        assert_eq!(d["line"], 3, "{checked}");
        assert_eq!(d["message"], "y is not declared", "{checked}");
        let (status, refused) = super::build(&entry(), TANK, None, &dir.project()).unwrap_err();
        assert_eq!(status, 400);
        assert_eq!(refused["diagnostics"][0], *d);

        // With only the data module's types, the program's own source is named too.
        let dir = project_dir();
        dir.types(SHARED);
        let checked = super::check(&entry(), wrong, &dir.project());
        assert_eq!(checked["diagnostics"][0]["source"], "Tank.st", "{checked}");
    }

    #[test]
    fn a_type_in_the_library_is_refused_with_its_place() {
        let dir = project_dir();
        dir.library(
            "Motors",
            &format!("{MOTORS}TYPE Speed : STRUCT rpm : INT; END_STRUCT; END_TYPE\n"),
        );
        let said = "Speed is a TYPE in the library source 'logic/library/Motors.st'; the project's types are made in the data module";
        let checked = super::check(&entry(), TANK, &dir.project());
        assert_eq!(checked["ok"], false);
        let d = &checked["diagnostics"];
        assert_eq!(d.as_array().unwrap().len(), 1, "{checked}");
        assert_eq!(d[0]["message"], said);
        assert_eq!(d[0]["source"], "logic/library/Motors.st");
        assert_eq!(
            (d[0]["line"].clone(), d[0]["col"].clone()),
            (json!(5), json!(6)),
            "{checked}"
        );
        let (status, refused) = super::build(&entry(), TANK, None, &dir.project()).unwrap_err();
        assert_eq!(status, 400);
        assert_eq!(refused["diagnostics"], *d);
        let library = check_library(dir.project(), None);
        assert_eq!(library["ok"], false);
        assert_eq!(library["diagnostics"], *d);

        // A TYPE in the program's own source is the program's, as before.
        let dir = project_dir();
        dir.library("Motors", MOTORS);
        let own = format!("TYPE Pair : STRUCT a : INT; b : INT; END_STRUCT; END_TYPE\n{TANK}");
        let checked = super::check(&entry(), &own, &dir.project());
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(check(&entry(), &own)["ok"], true);
    }

    #[test]
    fn the_logic_module_ignores_what_the_data_module_ignores() {
        // The two copies of the file rule, name by name: the data module's own table and more.
        let long = "x".repeat(65);
        let names = [
            "CON",
            "con",
            "Com1",
            "LPT9",
            "nul",
            "PrN",
            "aUx",
            "COM0",
            "lpt0",
            "a.b",
            "a__b",
            "a_",
            "1a",
            &long,
            "é",
            "",
            "Recipe",
            "_Tmp",
            "Con1",
            "CONSOLE",
            "RecipeData",
            "x9",
            &long[..64],
        ];
        for name in names {
            assert_eq!(
                type_name(name),
                mythos_data::type_name_problem(name).is_none(),
                "{name}"
            );
        }
        let files = [
            "RecipeData.st",
            "X.tmp",
            ".blocks.json.9.tmp",
            "notes.txt",
            "Bad-Name.st",
            "Y.ST",
            "CON.st",
            "a__b.st",
            "Z.st.tmp",
            ".st",
            "_T.st",
            "Recipe.St",
        ];
        for file in files {
            assert_eq!(
                source_stem(file, type_name),
                mythos_data::type_file_stem(file),
                "{file}"
            );
        }

        let dir = project_dir();
        dir.types(SHARED);
        for file in files {
            dir.put(
                &["data", "types", file],
                "TYPE Junk : STRUCT a : INT; END_STRUCT; END_TYPE\n",
            );
        }
        std::fs::create_dir_all(dir.root.path().join("data").join("types").join("Dir.st")).unwrap();
        let read: Vec<String> = dir.project().types.into_iter().map(|(n, _)| n).collect();
        // `RecipeData.st` and `_T.st` are type files (and hold a Junk each); the rest is not.
        assert_eq!(
            read,
            [
                "data/types/RecipeData.st",
                "data/types/Shared.st",
                "data/types/_T.st"
            ]
        );
    }

    #[test]
    fn a_library_name_is_one_name_without_regard_to_case_and_never_a_device() {
        for bad in ["cOn", "Prn", "aux", "NuL", "Com7", "lpT0"] {
            assert_eq!(
                library_name_problem(bad),
                Some(format!("'{bad}' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"))
            );
        }
        for bad in ["", "_x", "1a", "a-b", "a.st", &"x".repeat(65)] {
            assert_eq!(
                library_name_problem(bad),
                Some(format!("'{bad}' is not a library source name: a letter, then letters, digits or _, at most 64"))
            );
        }
        for good in ["Motors", "Con1", "CONSOLE", "x9", "COM10"] {
            assert_eq!(library_name_problem(good), None, "{good}");
        }

        let dir = project_dir();
        let logic = &dir.logic;
        let (reply, event) = library(
            &call(
                "PUT",
                "/library/:name",
                Some("Motors"),
                Some(json!({"source": MOTORS})),
                logic,
            ),
            logic,
        );
        assert_eq!(
            (reply.status, reply.body),
            (200, json!({"name": "Motors", "created": true}))
        );
        assert_eq!(
            event,
            Some(json!({"project": "plant", "name": "Motors", "what": "saved"}))
        );
        assert_eq!(
            std::fs::read_to_string(logic.join("library").join("Motors.st")).unwrap(),
            MOTORS
        );
        let (reply, _) = library(
            &call(
                "PUT",
                "/library/:name",
                Some("Motors"),
                Some(json!({"source": MOTORS})),
                logic,
            ),
            logic,
        );
        assert_eq!(reply.body["created"], false);
        // Another spelling of a stored name is refused, and nothing is written.
        let (reply, event) = library(
            &call(
                "PUT",
                "/library/:name",
                Some("MOTORS"),
                Some(json!({"source": "x"})),
                logic,
            ),
            logic,
        );
        assert_eq!(
            (reply.status, reply.body),
            (
                409,
                json!({"error": "the project already has a library source 'Motors'; names that differ only in case are one name"})
            )
        );
        assert_eq!(event, None);
        let listed: Vec<String> = std::fs::read_dir(logic.join("library"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(listed, ["Motors.st"], "no temporary is left");
        let (reply, _) = library(&call("GET", "/library", None, None, logic), logic);
        assert_eq!(reply.body, json!({"sources": [{"name": "Motors"}]}));
        let (reply, _) = library(
            &call("GET", "/library/:name", Some("motors"), None, logic),
            logic,
        );
        assert_eq!(reply.body, json!({"name": "Motors", "source": MOTORS}));
        // A draft of another spelling is refused as a save would be; both or neither.
        let (reply, _) = library(
            &call(
                "POST",
                "/library/check",
                None,
                Some(json!({"name": "motors", "source": "x"})),
                logic,
            ),
            logic,
        );
        assert_eq!(reply.status, 409);
        let (reply, _) = library(
            &call(
                "POST",
                "/library/check",
                None,
                Some(json!({"name": "Motors"})),
                logic,
            ),
            logic,
        );
        assert_eq!(
            (reply.status, reply.body),
            (
                400,
                json!({"error": "give both name and source, or neither"})
            )
        );
        let (reply, _) = library(
            &call(
                "POST",
                "/library/check",
                None,
                Some(json!({"name": "Motors", "source": MOTORS.replace("x * 2", "y * 2")})),
                logic,
            ),
            logic,
        );
        assert_eq!(reply.body["ok"], false, "{}", reply.body);
        assert_eq!(
            reply.body["diagnostics"][0]["source"],
            "logic/library/Motors.st"
        );
        let (reply, _) = library(&call("POST", "/library/check", None, None, logic), logic);
        assert_eq!(reply.body, json!({"ok": true, "diagnostics": []}));
        // Deleted by another spelling: the stored file goes.
        let (reply, event) = library(
            &call("DELETE", "/library/:name", Some("motors"), None, logic),
            logic,
        );
        assert_eq!(reply.body, json!({"deleted": "Motors"}));
        assert_eq!(
            event,
            Some(json!({"project": "plant", "name": "Motors", "what": "deleted"}))
        );
        assert!(!logic.join("library").join("Motors.st").exists());
        let (reply, _) = library(
            &call("GET", "/library/:name", Some("Motors"), None, logic),
            logic,
        );
        assert_eq!(
            (reply.status, reply.body),
            (
                404,
                json!({"error": "the project has no library source 'Motors'"})
            )
        );
    }

    #[test]
    fn the_logic_module_writes_nothing_under_the_data_directory() {
        let dir = project_dir();
        dir.types(SHARED);
        dir.blocks(&[("S", "Shared", false)]);
        let data = dir.root.path().join("data");
        let snapshot = || {
            let mut files: Vec<(String, Vec<u8>)> = Vec::new();
            for sub in [data.clone(), data.join("types")] {
                for e in std::fs::read_dir(&sub).unwrap() {
                    let path = e.unwrap().path();
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    files.push((path.display().to_string(), bytes));
                }
            }
            files.sort();
            files
        };
        let before = snapshot();
        super::check(&writer_entry(), WRITER, &dir.project());
        super::build(&writer_entry(), WRITER, None, &dir.project()).unwrap();
        check_library(dir.project(), Some(("Motors".into(), MOTORS.into())));
        assert_eq!(snapshot(), before);
        // A project whose data directory is not there is not given one.
        let bare = project_dir();
        bare.project();
        assert!(!bare.root.path().join("data").exists());
    }

    // -------------------------------------------------------------------------
    // Review of D3: files that are not text, odd shapes, links, the library's words
    // -------------------------------------------------------------------------

    /// Bytes a hand copy saved in Latin-1 holds: a comment with an e-acute.
    fn latin1(text: &str) -> Vec<u8> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.extend_from_slice(b"(* caf\xe9 *)\n");
        bytes
    }

    fn put_bytes(dir: &Dir, path: &[&str], bytes: &[u8]) {
        let file = path
            .iter()
            .fold(dir.root.path().to_path_buf(), |p, s| p.join(s));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }

    /// TANK uses no type, block or library function: it builds as it does in a project of
    /// today, whatever is wrong with the data, and the answer says what is.
    fn builds_as_today_with_problems(project: &Project) -> Value {
        let built = super::build(&entry(), TANK, None, project).unwrap();
        assert_eq!(built["version"], "d3adaba64d45", "{built}");
        let checked = super::check(&entry(), TANK, project);
        assert_eq!(checked["ok"], true, "{checked}");
        assert_eq!(checked["project_problems"], built["project_problems"]);
        built["project_problems"].clone()
    }

    #[test]
    fn a_type_file_that_is_not_text_is_a_problem_of_the_project_not_a_refusal() {
        let dir = project_dir();
        put_bytes(&dir, &["data", "types", "RecipeData.st"], &latin1(SHARED));
        let problems = builds_as_today_with_problems(&dir.project());
        assert_eq!(problems.as_array().unwrap().len(), 1, "{problems}");
        assert_eq!(problems[0]["source"], "data/types/RecipeData.st");
        assert!(
            problems[0]["message"].as_str().unwrap().starts_with(
                "data/types/RecipeData.st cannot be read as a type's source: it is not UTF-8 text"
            ),
            "{problems}"
        );
    }

    #[test]
    fn data_that_cannot_be_read_as_files_is_a_problem_of_the_project_not_a_refusal() {
        // blocks.json a directory.
        let dir = project_dir();
        std::fs::create_dir_all(dir.root.path().join("data").join("blocks.json")).unwrap();
        let problems = builds_as_today_with_problems(&dir.project());
        let message = problems[0]["message"].as_str().unwrap();
        assert!(
            message.starts_with("data/blocks.json cannot be read as the project's data blocks: "),
            "{problems}"
        );
        // data/types a file.
        let dir = project_dir();
        dir.put(&["data", "types"], "x");
        let problems = builds_as_today_with_problems(&dir.project());
        let message = problems[0]["message"].as_str().unwrap();
        assert!(
            message.starts_with("data/types cannot be read as the project's types: "),
            "{problems}"
        );
        // The library is the module's own: a library that is a file is said as a 500 naming
        // it, as any other error of reading the module's own directory.
        let dir = project_dir();
        dir.put(&["logic", "library"], "x");
        let (status, body) = read_project(&dir.logic).unwrap_err();
        assert_eq!(status, 500);
        assert!(
            body["error"].as_str().unwrap().contains("library"),
            "{body}"
        );
    }

    #[test]
    fn a_library_source_that_is_not_text_refuses_programs_and_its_own_routes_mend_it() {
        let dir = project_dir();
        let logic = &dir.logic;
        dir.library("Motors", MOTORS);
        let old = "FUNCTION Old : INT\n  Old := 1;\nEND_FUNCTION\n";
        put_bytes(&dir, &["logic", "library", "Old.st"], &latin1(old));
        let route = |m: &str, r: &str, n: Option<&str>, b: Option<Value>| {
            let (reply, _) = library(&call(m, r, n, b, logic), logic);
            (reply.status, reply.body)
        };
        assert_eq!(
            route("GET", "/library", None, None),
            (
                200,
                json!({"sources": [{"name": "Motors"}, {"name": "Old"}]})
            )
        );
        assert_eq!(route("GET", "/library/:name", Some("Motors"), None).0, 200);
        let (status, body) = route("GET", "/library/:name", Some("Old"), None);
        assert_eq!(status, 500, "{body}");
        // Every program is refused, with the file named, as for any library mistake.
        let checked = super::check(&entry(), TANK, &dir.project());
        assert_eq!(checked["ok"], false, "{checked}");
        let d = &checked["diagnostics"][0];
        assert_eq!(d["source"], "logic/library/Old.st", "{checked}");
        assert!(
            d["message"].as_str().unwrap().starts_with(
                "logic/library/Old.st cannot be read as a library source: it is not UTF-8 text"
            ),
            "{checked}"
        );
        let (status, refused) = super::build(&entry(), TANK, None, &dir.project()).unwrap_err();
        assert_eq!(status, 400);
        assert_eq!(refused["diagnostics"][0], *d);
        let (status, body) = route("POST", "/library/check", None, None);
        assert_eq!((status, &body["ok"]), (200, &json!(false)), "{body}");
        // A draft that replaces it checks.
        let (_, body) = route(
            "POST",
            "/library/check",
            None,
            Some(json!({"name": "Old", "source": old})),
        );
        assert_eq!(body, json!({"ok": true, "diagnostics": []}));
        // Its neighbour can be saved, and it can be replaced or deleted by its route.
        assert_eq!(
            route(
                "PUT",
                "/library/:name",
                Some("Motors"),
                Some(json!({"source": MOTORS}))
            )
            .0,
            200
        );
        assert_eq!(
            route(
                "PUT",
                "/library/:name",
                Some("Old"),
                Some(json!({"source": old}))
            ),
            (200, json!({"name": "Old", "created": false}))
        );
        assert_eq!(super::check(&entry(), TANK, &dir.project())["ok"], true);
        put_bytes(&dir, &["logic", "library", "Old.st"], &latin1(old));
        assert_eq!(
            route("DELETE", "/library/:name", Some("Old"), None),
            (200, json!({"deleted": "Old"}))
        );
        assert_eq!(super::check(&entry(), TANK, &dir.project())["ok"], true);
    }

    #[test]
    fn a_library_source_using_a_type_compiles_when_only_the_blocks_do_not_check() {
        let util = "FUNCTION LevelOf : INT\nVAR_INPUT s : Shared; END_VAR\n  LevelOf := s.level;\nEND_FUNCTION\n";
        for blocks in [
            "{\"blocks\": [",
            "{\"blocks\": [{\"name\": \"S\", \"type\": \"Missing\", \"retain\": false}]}",
        ] {
            let dir = project_dir();
            dir.types(SHARED);
            dir.library("Util", util);
            dir.put(&["data", "blocks.json"], blocks);
            let problems = builds_as_today_with_problems(&dir.project());
            assert_eq!(problems.as_array().unwrap().len(), 1, "{problems}");
            // A program that calls it, with a variable of the type, compiles.
            let probe = "PROGRAM Probe\nVAR_OUTPUT o : INT; END_VAR\nVAR s : Shared; END_VAR\n  s.level := 3;\n  o := LevelOf(s);\nEND_PROGRAM";
            let mut e = writer_entry();
            e.name = "Probe".into();
            e.inputs.clear();
            let checked = super::check(&e, probe, &dir.project());
            assert_eq!(checked["ok"], true, "{checked}");
            assert_eq!(checked["project_problems"], problems);
            assert_eq!(check_library(dir.project(), None)["ok"], true);
        }
        // When the types themselves do not check, they are left out, and the library source
        // that uses one is a library mistake, said with the project's problems.
        let dir = project_dir();
        dir.types("TYPE Shared : STRUCT level : NoSuchType; END_STRUCT; END_TYPE\n");
        dir.library("Util", util);
        let checked = super::check(&entry(), TANK, &dir.project());
        assert_eq!(checked["ok"], false, "{checked}");
        assert_eq!(checked["diagnostics"][0]["source"], "logic/library/Util.st");
        assert!(checked.get("project_problems").is_some(), "{checked}");
    }

    #[test]
    fn a_program_in_the_library_is_refused_in_the_modules_words() {
        let dir = project_dir();
        dir.library(
            "Motors",
            "PROGRAM P\nVAR a : INT; END_VAR\n a := 1;\nEND_PROGRAM\n",
        );
        let checked = super::check(&entry(), TANK, &dir.project());
        assert_eq!(checked["ok"], false);
        let d = &checked["diagnostics"];
        assert_eq!(d.as_array().unwrap().len(), 1, "{checked}");
        assert_eq!(
            d[0]["message"],
            "P is a PROGRAM in the library source 'logic/library/Motors.st'; a library holds functions and function blocks"
        );
        assert_eq!(d[0]["source"], "logic/library/Motors.st");
        assert_eq!(
            (d[0]["line"].clone(), d[0]["col"].clone()),
            (json!(1), json!(9))
        );
        let library = check_library(dir.project(), None);
        assert_eq!(library["diagnostics"], *d);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_at_the_temporarys_name_is_not_written_through() {
        let dir = project_dir();
        let logic = &dir.logic;
        let lib = logic.join("library");
        std::fs::create_dir_all(&lib).unwrap();
        let outside = dir.root.path().join("outside.txt");
        std::fs::write(&outside, "untouched").unwrap();
        let file = lib.join("Motors.st");
        std::os::unix::fs::symlink(&outside, temporary_path(&file).unwrap()).unwrap();
        let (reply, _) = library(
            &call(
                "PUT",
                "/library/:name",
                Some("Motors"),
                Some(json!({"source": MOTORS})),
                logic,
            ),
            logic,
        );
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "untouched");
        assert!(std::fs::symlink_metadata(&file)
            .unwrap()
            .file_type()
            .is_file());
        assert!(std::fs::symlink_metadata(temporary_path(&file).unwrap()).is_err());
        let (reply, _) = library(&call("GET", "/library", None, None, logic), logic);
        assert_eq!(reply.body, json!({"sources": [{"name": "Motors"}]}));
    }

    #[test]
    fn the_temporary_is_named_for_the_process() {
        let file = Path::new("library").join("Motors.st");
        assert_eq!(
            temporary_path(&file),
            Some(Path::new("library").join(format!(".Motors.st.{}.tmp", std::process::id())))
        );
    }

    /// A link in `data/types/` or `logic/library/` is not a source, whatever it points at,
    /// as the data module's rule has it.
    #[cfg(unix)]
    #[test]
    fn a_link_is_not_a_type_file_nor_a_library_source() {
        let dir = project_dir();
        dir.types(SHARED);
        dir.library("Motors", MOTORS);
        let outside = dir.root.path().join("Other.st");
        std::fs::write(
            &outside,
            "TYPE Other : STRUCT a : INT; END_STRUCT; END_TYPE\n",
        )
        .unwrap();
        let data_types = dir.root.path().join("data").join("types");
        std::os::unix::fs::symlink(&outside, data_types.join("Other.st")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.logic.join("library").join("Other.st")).unwrap();
        let project = dir.project();
        let names = |s: &[(String, String)]| s.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
        assert_eq!(names(&project.types), ["data/types/Shared.st"]);
        assert_eq!(names(&project.library), ["logic/library/Motors.st"]);
        let (reply, _) = library(&call("GET", "/library", None, None, &dir.logic), &dir.logic);
        assert_eq!(reply.body, json!({"sources": [{"name": "Motors"}]}));
    }

    /// Two spellings of one library source, made by hand on a file system that tells case
    /// apart: a request that names one exactly reaches that one.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_exact_library_spelling_is_found_before_another() {
        let dir = project_dir();
        let logic = &dir.logic;
        dir.library("Motors", MOTORS);
        let lower = MOTORS.replace("Twice", "Thrice");
        dir.library("motors", &lower);
        let (reply, _) = library(
            &call("GET", "/library/:name", Some("motors"), None, logic),
            logic,
        );
        assert_eq!(reply.body, json!({"name": "motors", "source": lower}));
        let (reply, _) = library(
            &call("DELETE", "/library/:name", Some("motors"), None, logic),
            logic,
        );
        assert_eq!(reply.body, json!({"deleted": "motors"}));
        assert!(logic.join("library").join("Motors.st").exists());
    }
}
