//! A project's runtimes and their tasks, as authored (ADR-013 §2.19): `<project>/logic/tasks.json`.
//!
//! A program that no configured runtime names runs alone, exactly as before there was a task
//! configuration: its own runtime, whose id is the program's name, in one cyclic task at its own
//! interval ([`implied_task`]). Nothing is written for it; such a runtime is *implied*, derived
//! from `programs.json` on every read. A *configured* runtime is a record of this file: its id,
//! the runtime module that runs it, the target its programs are compiled for, whether it is put
//! in RUN when loaded, and its tasks, each naming programs of the project.
//!
//! Every write is checked by the rules of [`problems_of`]; a write that breaks one is refused
//! and writes nothing. The files may still be edited by hand, so every read checks them again and
//! lists what is wrong in `problems`. A project with no `tasks.json` has no configured runtime,
//! and no read makes the file.

use super::{
    body, build as build_program, check as check_program, check_bindings, check_entry,
    compile_entry, data_uses, device_name, error, find, instance_data, load as load_programs,
    read_project, read_source, reply, unregistered, version_in_runtime, with_problems,
    write_synced, ProgramEntry, Programs, Project, Refusal,
};
use mythos_logic_compiler::{Compiled, Diagnostic, Symbol, TargetProfile};
use mythos_module::protocol::{
    check_data, check_tasks, implied_task, DataBlock as RuntimeDataBlock, HttpCall, HttpReply,
    InputOf, RuntimeProgram, TaskConfig, TaskKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

/// The file, in the module's directory. Not `runtimes.json`: the core's `<project>/runtimes.json`
/// is what is loaded, this is what is authored.
pub(crate) const TASKS_FILE: &str = "tasks.json";

/// `tasks.json`. Unknown keys are refused, so a file written by a newer module is not read
/// as if it said less than it does.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskFile {
    pub(crate) runtimes: Vec<RuntimeEntry>,
}

/// One runtime: every key required, none has a default (whether it runs when loaded is a
/// decision, and a runtime without a task runs nothing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeEntry {
    /// The core's runtime id: 1 to 64 letters, digits, `-` or `_`.
    pub(crate) id: String,
    /// The runtime module that runs it: a module's `provides`.
    pub(crate) module: String,
    /// The target profile its programs are compiled for.
    pub(crate) target: String,
    /// Whether it is put in RUN when it is loaded.
    pub(crate) run: bool,
    /// Its tasks; each names programs of the project, in the spelling of `programs.json`.
    pub(crate) tasks: Vec<TaskConfig>,
}

/// What `PUT /runtimes/:runtime` takes: a runtime without its id, which is the path's.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBody {
    module: String,
    target: String,
    run: bool,
    tasks: Vec<TaskConfig>,
}

fn unreadable(e: impl std::fmt::Display) -> Refusal {
    error(
        500,
        format!("logic/{TASKS_FILE} cannot be read as the project's runtimes: {e}"),
    )
}

/// The project's configured runtimes. A missing file is none; it is never made by a read.
pub(crate) fn load(data: &Path) -> Result<TaskFile, Refusal> {
    match std::fs::read(data.join(TASKS_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(unreadable),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TaskFile::default()),
        Err(e) => Err(unreadable(e)),
    }
}

fn save(data: &Path, file: &TaskFile) -> Result<(), Refusal> {
    let mut bytes = serde_json::to_vec_pretty(file).map_err(|e| error(500, e))?;
    bytes.push(b'\n');
    write_synced(&data.join(TASKS_FILE), &bytes)
}

/// The core's runtime id, word for word.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What is wrong with `id` as a runtime id, in the core's words; `None` when it is one.
pub(crate) fn id_problem(id: &str) -> Option<String> {
    (!valid_id(id))
        .then(|| format!("'{id}' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'"))
}

/// The configured runtime and the task of it that run program `name` (without regard to case),
/// if one does.
pub(crate) fn assignment<'a>(
    file: &'a TaskFile,
    name: &str,
) -> Option<(&'a RuntimeEntry, &'a TaskConfig)> {
    file.runtimes.iter().find_map(|r| {
        r.tasks
            .iter()
            .find(|t| t.programs.iter().any(|p| p.eq_ignore_ascii_case(name)))
            .map(|t| (r, t))
    })
}

/// [`assignment`], by the runtime's id and the task's name.
pub(crate) fn assigned<'a>(file: &'a TaskFile, name: &str) -> Option<(&'a str, &'a str)> {
    assignment(file, name).map(|(r, t)| (r.id.as_str(), t.name.as_str()))
}

/// A program as [`check_tasks`] and [`implied_task`] read it: only its name and interval.
/// Never answered.
fn stub(name: &str, interval_ms: u64) -> RuntimeProgram {
    RuntimeProgram {
        name: name.to_string(),
        version: String::new(),
        interval_ms,
        code: String::new(),
        symbols: Value::Null,
        inputs: vec![],
        outputs: vec![],
        published: vec![],
        data: vec![],
    }
}

/// Which other runtimes the rules between runtimes (one id without case, a program in one
/// runtime) compare runtime `i` with: every other, for a write; only those before it, for a
/// listing, so that each problem is said once.
#[derive(Clone, Copy, PartialEq)]
enum Others {
    All,
    Before,
}

/// The rules runtime `i` of `runtimes` breaks, in order (the C8c contract §3.3, T1 to T9), each
/// with the status a write that breaks it is refused with.
fn problems_of(
    programs: &Programs,
    runtimes: &[RuntimeEntry],
    i: usize,
    others: Others,
) -> Vec<(u16, String)> {
    let r = &runtimes[i];
    let id = r.id.as_str();
    let compared = |j: usize| j != i && (others == Others::All || j < i);
    let mut found = Vec::new();
    // T1
    if let Some(problem) = id_problem(id) {
        found.push((400, problem));
    }
    // T2: the core keeps a runtime's retained values in a file named for it.
    if device_name(id) {
        found.push((400, format!(
            "'{id}' cannot be a runtime id: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"
        )));
    }
    // T3: and that file is one file for two spellings on Windows and macOS.
    if let Some(other) = (0..runtimes.len())
        .filter(|&j| compared(j))
        .map(|j| &runtimes[j])
        .find(|o| o.id.eq_ignore_ascii_case(id))
    {
        found.push((
            409,
            format!(
            "the project already has a runtime '{}'; names that differ only in case are one name",
            other.id
        ),
        ));
    }
    // T4: a program running alone is a runtime of its own name.
    if programs
        .programs
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(id))
    {
        found.push((
            409,
            format!("'{id}' is a program of the project; a runtime is not named like a program"),
        ));
    }
    // T5
    if r.module.trim().is_empty() {
        found.push((400, format!("runtime '{id}' names no module")));
    }
    // T6
    if TargetProfile::by_name(&r.target).is_none() {
        found.push((400, format!("'{}' is not a target profile", r.target)));
    }
    // T7a
    if r.tasks.is_empty() {
        found.push((400, format!("runtime '{id}' has no task")));
    } else {
        // T8, before T7, so that a program the project lacks is said to be lacking.
        let mut named: Vec<RuntimeProgram> = Vec::new();
        for t in &r.tasks {
            for p in &t.programs {
                let stored = programs
                    .programs
                    .iter()
                    .find(|e| e.name.eq_ignore_ascii_case(p));
                if stored.is_none() {
                    found.push((
                        400,
                        format!(
                        "runtime '{id}': task '{}' runs '{p}': the project has no program '{p}'",
                        t.name
                    ),
                    ));
                }
                if !named.iter().any(|n| n.name.eq_ignore_ascii_case(p)) {
                    named.push(match stored {
                        Some(e) => stub(&e.name, e.interval_ms),
                        // Refused above (T8). `check_tasks` reads no interval, and this
                        // stub is neither answered nor built.
                        None => stub(p, 0),
                    });
                }
            }
        }
        // T7: the protocol's own check, on every program the tasks name.
        let refs: Vec<&RuntimeProgram> = named.iter().collect();
        if let Err(m) = check_tasks(&refs, &r.tasks) {
            found.push((400, format!("runtime '{id}': {m}")));
        }
        // T9
        for p in &named {
            if let Some(other) = (0..runtimes.len())
                .filter(|&j| compared(j))
                .map(|j| &runtimes[j])
                .find(|o| {
                    o.tasks
                        .iter()
                        .any(|t| t.programs.iter().any(|q| q.eq_ignore_ascii_case(&p.name)))
                })
            {
                found.push((
                    409,
                    format!(
                        "program '{}' is in runtime '{}' already; a program is in one runtime",
                        p.name, other.id
                    ),
                ));
            }
        }
    }
    found
}

/// Every rule the stored files break, runtime by runtime: what a hand edit got wrong.
pub(crate) fn configuration_problems(programs: &Programs, file: &TaskFile) -> Vec<String> {
    (0..file.runtimes.len())
        .flat_map(|i| problems_of(programs, &file.runtimes, i, Others::Before))
        .map(|(_, m)| m)
        .collect()
}

/// The runtime each program named by no configured runtime runs alone in, in the order of
/// `programs.json`: derived, never written.
fn implied(programs: &Programs, file: &TaskFile) -> Vec<RuntimeEntry> {
    programs
        .programs
        .iter()
        .filter(|p| assigned(file, &p.name).is_none())
        .map(|p| RuntimeEntry {
            id: p.name.clone(),
            module: p.runtime.clone(),
            target: p.target.clone(),
            run: p.run,
            tasks: vec![implied_task(&stub(&p.name, p.interval_ms))],
        })
        .collect()
}

/// An implied runtime as the routes that list runtimes answer it: its task is the one the core
/// derives from the instance the program's build answers ([`implied_task`] of its program),
/// whose program is named by the PROGRAM its saved source declares, which need not be the
/// program's name in the project. So the program is compiled alone, as its build compiles it.
/// When it does not compile (or its source cannot be read), it has no build to derive a task
/// from, and the task stays named by the program's name in the project, as the tool's summary
/// says.
fn listed(
    mut runtime: RuntimeEntry,
    programs: &Programs,
    data: &Path,
    project: Option<&Project>,
) -> RuntimeEntry {
    let Some(entry) = programs.programs.iter().find(|p| p.name == runtime.id) else {
        return runtime;
    };
    if let Some(declared) = declared_name(entry, data, project) {
        runtime.tasks = vec![implied_task(&stub(&declared, entry.interval_ms))];
    }
    runtime
}

/// The name of the PROGRAM that `entry`'s saved source declares, as its build compiles it;
/// `None` when it would not compile.
fn declared_name(entry: &ProgramEntry, data: &Path, project: Option<&Project>) -> Option<String> {
    check_entry(entry).ok()?;
    let target = TargetProfile::by_name(&entry.target)?;
    let source = read_source(data, entry).ok()?;
    compile_entry(entry, &source, &target, project?)
        .compiled
        .ok()
        .map(|c| c.name)
}

/// The runtime of `id` among `runtimes`: the exact spelling, else one that differs only in case.
fn position(runtimes: &[RuntimeEntry], id: &str) -> Option<usize> {
    runtimes
        .iter()
        .position(|r| r.id == id)
        .or_else(|| runtimes.iter().position(|r| r.id.eq_ignore_ascii_case(id)))
}

fn no_runtime(id: &str) -> Refusal {
    error(404, format!("the project has no runtime '{id}'"))
}

/// The routes of the project's runtimes: the reply, and the data of the event to send when a
/// runtime was saved or deleted.
pub(crate) fn runtimes(call: &HttpCall, data: &Path) -> (HttpReply, Option<Value>) {
    let project = call.project.clone().unwrap_or_default();
    let id = call.params.get("runtime").map(String::as_str).unwrap_or("");
    let mut event = None;
    let result = match (call.method.as_str(), call.route.as_str()) {
        ("GET", "/runtimes") => (|| {
            let programs = load_programs(data)?;
            let file = load(data)?;
            let alone = implied(&programs, &file);
            let project = if alone.is_empty() {
                None
            } else {
                read_project(data).ok()
            };
            let alone: Vec<RuntimeEntry> = alone
                .into_iter()
                .map(|r| listed(r, &programs, data, project.as_ref()))
                .collect();
            Ok(json!({
                "runtimes": file.runtimes,
                "implied": alone,
                "problems": configuration_problems(&programs, &file),
            }))
        })(),
        ("GET", "/runtimes/:runtime") => (|| {
            let programs = load_programs(data)?;
            let file = load(data)?;
            if let Some(at) = position(&file.runtimes, id) {
                return Ok(json!({ "runtime": file.runtimes[at], "implied": false }));
            }
            let mut alone = implied(&programs, &file);
            let at = position(&alone, id).ok_or_else(|| no_runtime(id))?;
            let runtime = listed(
                alone.swap_remove(at),
                &programs,
                data,
                read_project(data).ok().as_ref(),
            );
            Ok(json!({ "runtime": runtime, "implied": true }))
        })(),
        ("PUT", "/runtimes/:runtime") => body::<RuntimeBody>(call).and_then(|b| {
            let programs = load_programs(data)?;
            let mut file = load(data)?;
            let (at, created) = put_in(&mut file, &programs, id, b);
            let entry = file.runtimes[at].clone();
            if let Some((status, problem)) = problems_of(&programs, &file.runtimes, at, Others::All)
                .into_iter()
                .next()
            {
                return Err(error(status, problem));
            }
            save(data, &file)?;
            event = Some(json!({ "project": project, "runtime": id, "what": "saved" }));
            Ok(json!({ "runtime": entry, "created": created }))
        }),
        ("DELETE", "/runtimes/:runtime") => (|| {
            let mut file = load(data)?;
            // An id that is not a runtime id is taken only when a runtime is stored under it
            // (a hand edit), so that the runtime can be taken out (`Logic::serve`).
            let at = position(&file.runtimes, id).ok_or_else(|| match id_problem(id) {
                Some(problem) => error(400, problem),
                None => no_runtime(id),
            })?;
            let gone = file.runtimes.remove(at);
            save(data, &file)?;
            event = Some(json!({ "project": project, "runtime": gone.id, "what": "deleted" }));
            Ok(json!({ "deleted": gone.id }))
        })(),
        ("POST", "/runtimes/:runtime/check") => (|| {
            let programs = load_programs(data)?;
            let mut file = load(data)?;
            let draft = match &call.body {
                None | Some(Value::Null) => None,
                Some(Value::Object(o)) if o.is_empty() => None,
                Some(_) => Some(body::<RuntimeBody>(call)?),
            };
            let at = match draft {
                Some(b) => Some(put_in(&mut file, &programs, id, b).0),
                None => position(&file.runtimes, id),
            };
            match at {
                Some(at) => check_runtime(&programs, &file, at, data),
                None => {
                    let alone = implied(&programs, &file);
                    let at = position(&alone, id).ok_or_else(|| no_runtime(id))?;
                    check_alone(find(&programs, &alone[at].id)?, data)
                }
            }
        })(),
        ("POST", "/runtimes/:runtime/build") => (|| {
            let programs = load_programs(data)?;
            let file = load(data)?;
            let tags = call.tags.as_deref();
            match position(&file.runtimes, id) {
                Some(at) => build_runtime(&programs, &file, at, data, tags),
                None => {
                    // A program running alone: exactly its own build.
                    let alone = implied(&programs, &file);
                    let at = position(&alone, id).ok_or_else(|| no_runtime(id))?;
                    let entry = find(&programs, &alone[at].id)?;
                    build_program(
                        entry,
                        &read_source(data, entry)?,
                        tags,
                        &read_project(data)?,
                    )
                }
            }
        })(),
        (method, route) => Err(error(
            404,
            format!("the logic module has no route {method} {route}"),
        )),
    };
    (reply(result, 200), event)
}

/// Put runtime `id` of `body` into `file`, in place of the one of exactly that id or after the
/// others, with each task naming a program in the spelling it is stored in: where it is, and
/// whether it is new. Nothing is checked or written.
fn put_in(file: &mut TaskFile, programs: &Programs, id: &str, body: RuntimeBody) -> (usize, bool) {
    let mut tasks = body.tasks;
    for t in &mut tasks {
        for p in &mut t.programs {
            if let Some(e) = programs
                .programs
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(p))
            {
                p.clone_from(&e.name);
            }
        }
    }
    let entry = RuntimeEntry {
        id: id.to_string(),
        module: body.module,
        target: body.target,
        run: body.run,
        tasks,
    };
    match file.runtimes.iter().position(|r| r.id == id) {
        Some(at) => {
            file.runtimes[at] = entry;
            (at, false)
        }
        None => {
            file.runtimes.push(entry);
            (file.runtimes.len() - 1, true)
        }
    }
}

// -----------------------------------------------------------------------------
// Checking and building a configured runtime (the C8c contract §3.5, §3.6)
// -----------------------------------------------------------------------------

/// The programs of `runtime`, in the order its tasks run them (task by task, each task's in its
/// order), each once, with the task it is in. A name the project lacks is left out: that is a
/// problem of the configuration (T8).
fn members<'a>(
    runtime: &'a RuntimeEntry,
    programs: &'a Programs,
) -> Vec<(&'a ProgramEntry, &'a TaskConfig)> {
    let mut found: Vec<(&ProgramEntry, &TaskConfig)> = Vec::new();
    for t in &runtime.tasks {
        for p in &t.programs {
            let Some(e) = programs
                .programs
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(p))
            else {
                continue;
            };
            if !found.iter().any(|(f, _)| f.name == e.name) {
                found.push((e, t));
            }
        }
    }
    found
}

/// The entry as a runtime builds it: with the runtime's target. Its own module, run and
/// interval are not used by the runtime (its module, its run and its tasks are).
fn as_built(entry: &ProgramEntry, runtime: &RuntimeEntry) -> ProgramEntry {
    let mut e = entry.clone();
    e.target.clone_from(&runtime.target);
    e
}

/// In a runtime of several programs a variable is named `<program>.<variable>` by its
/// compiled PROGRAM's name, so that name must be the program's in the project: what is wrong,
/// when it is not.
fn naming_problem(
    id: &str,
    entry: &ProgramEntry,
    compiled: &Compiled,
    several: bool,
) -> Option<String> {
    (several && !compiled.name.eq_ignore_ascii_case(&entry.name)).then(|| {
        format!(
            "runtime '{id}': program '{p}' declares PROGRAM {q} in its source; in a runtime of several programs its variables are named {p}.<variable>, so its PROGRAM must be named {p}",
            p = entry.name,
            q = compiled.name
        )
    })
}

/// A program of a runtime, compiled, as the instance carries it.
struct Built<'a> {
    entry: &'a ProgramEntry,
    compiled: Compiled,
    program: RuntimeProgram,
}

/// The runtime's tasks as the instance gives them: each program named by its compiled
/// PROGRAM's name, which is what the instance's programs are named.
fn emitted_tasks(runtime: &RuntimeEntry, built: &[Built]) -> Vec<TaskConfig> {
    runtime
        .tasks
        .iter()
        .map(|t| TaskConfig {
            programs: t
                .programs
                .iter()
                .map(|p| {
                    built
                        .iter()
                        .find(|b| b.entry.name.eq_ignore_ascii_case(p))
                        .map_or_else(|| p.clone(), |b| b.compiled.name.clone())
                })
                .collect(),
            ..t.clone()
        })
        .collect()
}

/// The data blocks of the runtime: each block a program declares, once (by name, without
/// regard to case), in the order the programs first declare them.
fn union_of_data(built: &[Built], project: &Project) -> Result<Vec<RuntimeDataBlock>, Refusal> {
    let mut data: Vec<RuntimeDataBlock> = Vec::new();
    for b in built {
        for block in instance_data(&b.compiled, &project.blocks).map_err(|e| error(500, e))? {
            if !data
                .iter()
                .any(|d| d.name.eq_ignore_ascii_case(&block.name))
            {
                data.push(block);
            }
        }
    }
    Ok(data)
}

/// What the core and the runtime module would refuse of the runtime as a whole, each said as
/// they say it and starting `runtime '<id>': `: its tasks, its data, an output tag or a
/// published tag of two of its programs, and an event task whose tag no program reads into a
/// BOOL.
fn runtime_problems(
    id: &str,
    built: &[Built],
    tasks: &[TaskConfig],
    data: &[RuntimeDataBlock],
) -> Vec<String> {
    let prefixed = |m: String| format!("runtime '{id}': {m}");
    let refs: Vec<&RuntimeProgram> = built.iter().map(|b| &b.program).collect();
    let several = built.len() > 1;
    let owner = |p: &RuntimeProgram| {
        if several {
            format!("{id}/{}", p.name)
        } else {
            id.to_string()
        }
    };
    let mut found = Vec::new();
    if let Err(m) = check_tasks(&refs, tasks) {
        found.push(prefixed(m));
    }
    if let Err(m) = check_data(&refs, data) {
        found.push(prefixed(m));
    }
    // One tag has one writer, and one publisher (the core's words).
    let mut outputs: Vec<(&str, String)> = Vec::new();
    let mut published: Vec<(&str, String)> = Vec::new();
    for p in &refs {
        for o in &p.outputs {
            if let Some((_, a)) = outputs.iter().find(|(t, _)| *t == o.tag) {
                found.push(prefixed(format!(
                    "{} is an output of both '{a}' and '{}': one tag has one writer",
                    o.tag,
                    owner(p)
                )));
            } else {
                outputs.push((&o.tag, owner(p)));
            }
        }
        for s in &p.published {
            if let Some((_, a)) = published.iter().find(|(t, _)| *t == s.tag) {
                found.push(prefixed(format!(
                    "{} is published by both '{a}' and '{}'",
                    s.tag,
                    owner(p)
                )));
            } else {
                published.push((&s.tag, owner(p)));
            }
        }
    }
    // An event task sees an edge in a BOOL a program reads its tag into (the runtime module's
    // words).
    for t in tasks {
        let TaskKind::Event { tag } = &t.kind else {
            continue;
        };
        let bound = built.iter().any(|b| {
            b.entry.inputs.iter().any(|i| {
                i.tag == *tag
                    && i.of == InputOf::Value
                    && symbol(&b.compiled.symbols, &i.variable).is_some_and(|s| s.ty == "BOOL")
            })
        });
        if !bound {
            found.push(prefixed(format!(
                "task '{}': its tag {tag} is not read into a BOOL variable by any program of this runtime",
                t.name
            )));
        }
    }
    found
}

fn symbol<'a>(symbols: &'a [Symbol], variable: &str) -> Option<&'a Symbol> {
    symbols
        .iter()
        .find(|s| s.path.eq_ignore_ascii_case(variable))
}

/// The runtime's version, as the core reports it of the instance (`RuntimeStatus.version`): its
/// one program's version, or every program's joined by `, ` in the instance's order. Each is the
/// program's version in the runtime, which follows its task, its code, its bindings and its data
/// blocks' layouts; not whether the runtime is put in RUN, nor whether a block is retained.
fn runtime_version(built: &[Built]) -> String {
    built
        .iter()
        .map(|b| b.program.version.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a load does with the RETAIN values of program `p` in runtime `id` (ADR-013 §2.5): the
/// core keeps them per runtime id, so a warm load gives `p` what is kept under `id`, whichever
/// program ran under that id before, and nothing of what was kept while `p` ran alone.
fn retain_warning(p: &str, id: &str) -> String {
    format!(
        "program '{p}': RETAIN values are kept per runtime id, not per program: a warm load of runtime '{id}' puts into each RETAIN variable of '{p}' the value kept under runtime '{id}' for a variable of the same name, type and size, whichever program ran in runtime '{id}' before, and none of the values kept while '{p}' ran alone as runtime '{p}'; a cold load starts every RETAIN variable from its initial value"
    )
}

/// The program of `entry` as the instance carries it.
fn runtime_program(
    entry: &ProgramEntry,
    compiled: &Compiled,
    task: &TaskConfig,
) -> Result<RuntimeProgram, Refusal> {
    Ok(RuntimeProgram {
        name: compiled.name.clone(),
        version: version_in_runtime(entry, compiled, task),
        // Stored, and not used by a runtime given tasks; never a value made up here.
        interval_ms: entry.interval_ms,
        code: hex::encode(&compiled.bytecode),
        symbols: serde_json::to_value(&compiled.symbols).map_err(|e| error(500, e))?,
        inputs: entry.inputs.clone(),
        outputs: entry.outputs.clone(),
        published: entry.published.clone(),
        data: data_uses(compiled),
    })
}

/// Build configured runtime `at` of `file`: every program from its saved source, in one
/// snapshot of the project, for the runtime's target; the first thing wrong refuses it.
fn build_runtime(
    programs: &Programs,
    file: &TaskFile,
    at: usize,
    data: &Path,
    registered: Option<&[String]>,
) -> Result<Value, Refusal> {
    let runtime = &file.runtimes[at];
    let id = runtime.id.as_str();
    if let Some((status, problem)) = problems_of(programs, &file.runtimes, at, Others::All)
        .into_iter()
        .next()
    {
        return Err(error(status, problem));
    }
    let target = TargetProfile::by_name(&runtime.target)
        .ok_or_else(|| error(400, format!("'{}' is not a target profile", runtime.target)))?;
    let project = read_project(data)?;
    let members = members(runtime, programs);
    let several = members.len() > 1;
    let mut project_problems: Option<Vec<Diagnostic>> = None;
    let mut built: Vec<Built> = Vec::new();
    for (entry, task) in members {
        let p = entry.name.as_str();
        let e = as_built(entry, runtime);
        check_entry(&e).map_err(|m| error(400, format!("runtime '{id}': program '{p}': {m}")))?;
        let compiling = compile_entry(&e, &read_source(data, entry)?, &target, &project);
        project_problems = compiling.project_problems;
        let compiled = compiling.compiled.map_err(|diagnostics| {
            (
                400,
                with_problems(
                    json!({
                        "error": format!("program '{p}' does not compile and runtime '{id}' was not built"),
                        "program": p,
                        "diagnostics": diagnostics,
                    }),
                    &project_problems,
                ),
            )
        })?;
        let binding_problems = check_bindings(&e, &compiled.symbols);
        if !binding_problems.is_empty() {
            return Err((
                400,
                with_problems(
                    json!({
                        "error": format!("program '{p}''s bindings do not match its variables and runtime '{id}' was not built"),
                        "program": p,
                        "binding_problems": binding_problems,
                    }),
                    &project_problems,
                ),
            ));
        }
        if let Some(problem) = naming_problem(id, entry, &compiled, several) {
            return Err((
                400,
                with_problems(json!({ "error": problem }), &project_problems),
            ));
        }
        let program = runtime_program(entry, &compiled, task)?;
        built.push(Built {
            entry,
            compiled,
            program,
        });
    }
    let tasks = emitted_tasks(runtime, &built);
    let data_blocks = union_of_data(&built, &project)?;
    if let Some(problem) = runtime_problems(id, &built, &tasks, &data_blocks)
        .into_iter()
        .next()
    {
        return Err((
            400,
            with_problems(json!({ "error": problem }), &project_problems),
        ));
    }

    let mut warnings = Vec::new();
    for b in &built {
        let p = &b.entry.name;
        warnings.extend(
            unregistered(b.entry, registered)
                .into_iter()
                .map(|w| format!("program '{p}': {w}")),
        );
        // The core keeps RETAIN values per runtime id, not per program, and the runtime module
        // puts each into the RETAIN variable of the same name, type and size, whichever program
        // wrote it. This module cannot know what the core keeps under the id (it never calls
        // the core), so it says what happens in either case.
        if b.compiled.symbols.iter().any(|s| s.retain) {
            warnings.push(retain_warning(p, id));
        }
    }
    let mut instance = json!({
        "id": id,
        "module": runtime.module,
        "run": runtime.run,
        "tasks": tasks,
    });
    // One program in `program`, as a runtime of today: a module that runs one program in one
    // task then runs it, when its task is the one it runs.
    if let [one] = built.as_slice() {
        instance["program"] = json!(one.program);
    } else {
        instance["programs"] = json!(built.iter().map(|b| &b.program).collect::<Vec<_>>());
    }
    if !data_blocks.is_empty() {
        instance["data"] = json!(data_blocks);
    }
    Ok(with_problems(
        json!({
            "version": runtime_version(&built),
            "warnings": warnings,
            "programs": built.iter().map(|b| json!({
                "name": b.entry.name,
                "version": b.program.version,
                "code_size": b.compiled.code_size,
                "memory_size": b.compiled.memory_size,
            })).collect::<Vec<_>>(),
            // The ids its programs run under alone: the caller takes them out of the core's
            // runtimes in the save that puts this one in.
            "replaces": built.iter().map(|b| &b.entry.name).collect::<Vec<_>>(),
            "instance": instance,
        }),
        &project_problems,
    ))
}

/// Check configured runtime `at` of `file` (stored, or a draft put in it): every program
/// compiled as the build would, never refused for what it finds.
fn check_runtime(
    programs: &Programs,
    file: &TaskFile,
    at: usize,
    data: &Path,
) -> Result<Value, Refusal> {
    let runtime = &file.runtimes[at];
    let id = runtime.id.as_str();
    let mut problems: Vec<String> = problems_of(programs, &file.runtimes, at, Others::All)
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    let configured = problems.is_empty();
    let target = TargetProfile::by_name(&runtime.target);
    let project = read_project(data)?;
    let members = members(runtime, programs);
    let several = members.len() > 1;
    let mut project_problems: Option<Vec<Diagnostic>> = None;
    let mut answers = Vec::new();
    let mut built: Vec<Built> = Vec::new();
    let mut all_ok = true;
    for (entry, task) in members {
        let p = entry.name.as_str();
        let e = as_built(entry, runtime);
        if let Err(m) = check_entry(&e) {
            // Its target is the runtime's, which the configuration's problems already name.
            if target.is_some() {
                problems.push(format!("runtime '{id}': program '{p}': {m}"));
            }
        }
        let source = read_source(data, entry)?;
        let mut answer =
            json!({ "name": p, "ok": false, "diagnostics": [], "binding_problems": [] });
        let Some(target) = &target else {
            answer["diagnostics"] = json!([Diagnostic::whole(format!(
                "'{}' is not a target profile",
                runtime.target
            ))]);
            all_ok = false;
            answers.push(answer);
            continue;
        };
        let compiling = compile_entry(&e, &source, target, &project);
        project_problems = compiling.project_problems;
        match compiling.compiled {
            Err(diagnostics) => {
                answer["diagnostics"] = json!(diagnostics);
                all_ok = false;
            }
            Ok(compiled) => {
                let binding_problems = check_bindings(&e, &compiled.symbols);
                let program = runtime_program(entry, &compiled, task)?;
                answer["ok"] = json!(binding_problems.is_empty());
                answer["binding_problems"] = json!(binding_problems);
                answer["version"] = json!(program.version);
                answer["code_size"] = json!(compiled.code_size);
                answer["memory_size"] = json!(compiled.memory_size);
                if !binding_problems.is_empty() {
                    all_ok = false;
                }
                if let Some(problem) = naming_problem(id, entry, &compiled, several) {
                    problems.push(problem);
                }
                built.push(Built {
                    entry,
                    compiled,
                    program,
                });
            }
        }
        answers.push(answer);
    }
    let tasks = emitted_tasks(runtime, &built);
    let data_blocks = union_of_data(&built, &project)?;
    // The runtime as a whole, once its configuration holds and every program built: before,
    // the tasks' own check would say again what the configuration's problems say.
    if configured && all_ok && problems.is_empty() {
        problems.extend(runtime_problems(id, &built, &tasks, &data_blocks));
    }
    let ok = all_ok && problems.is_empty();
    let mut answer = json!({
        "ok": ok,
        "runtime": id,
        "programs": answers,
        "problems": problems,
    });
    if ok {
        answer["version"] = json!(runtime_version(&built));
    }
    if !data_blocks.is_empty() {
        answer["data"] = json!(data_blocks);
    }
    Ok(with_problems(answer, &project_problems))
}

/// The check of a program running alone, as a runtime of its own: its program's check.
fn check_alone(entry: &ProgramEntry, data: &Path) -> Result<Value, Refusal> {
    let mut checked = check_program(entry, &read_source(data, entry)?, &read_project(data)?);
    let ok = checked["ok"] == true
        && checked["binding_problems"]
            .as_array()
            .is_some_and(Vec::is_empty);
    let project_problems = checked.get("project_problems").cloned();
    let version = checked.get("version").cloned();
    checked["name"] = json!(entry.name);
    let mut answer = json!({
        "ok": ok,
        "runtime": entry.name,
        "programs": [checked],
        "problems": [],
    });
    if let (true, Some(version)) = (ok, version) {
        answer["version"] = version;
    }
    if let Some(problems) = project_problems {
        answer["project_problems"] = problems;
    }
    Ok(answer)
}

#[cfg(test)]
mod tests;
