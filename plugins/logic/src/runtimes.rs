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
    body, device_name, error, load as load_programs, reply, write_synced, Programs, Refusal,
};
use mythos_logic_compiler::TargetProfile;
use mythos_module::protocol::{
    check_tasks, implied_task, HttpCall, HttpReply, RuntimeProgram, TaskConfig,
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
pub(crate) fn assigned<'a>(file: &'a TaskFile, name: &str) -> Option<(&'a str, &'a str)> {
    file.runtimes.iter().find_map(|r| {
        r.tasks
            .iter()
            .find(|t| t.programs.iter().any(|p| p.eq_ignore_ascii_case(name)))
            .map(|t| (r.id.as_str(), t.name.as_str()))
    })
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
            Ok(json!({
                "runtimes": file.runtimes,
                "implied": implied(&programs, &file),
                "problems": configuration_problems(&programs, &file),
            }))
        })(),
        ("GET", "/runtimes/:runtime") => (|| {
            let programs = load_programs(data)?;
            let file = load(data)?;
            if let Some(at) = position(&file.runtimes, id) {
                return Ok(json!({ "runtime": file.runtimes[at], "implied": false }));
            }
            let alone = implied(&programs, &file);
            let at = position(&alone, id).ok_or_else(|| no_runtime(id))?;
            Ok(json!({ "runtime": alone[at], "implied": true }))
        })(),
        ("PUT", "/runtimes/:runtime") => body::<RuntimeBody>(call).and_then(|b| {
            let programs = load_programs(data)?;
            let mut file = load(data)?;
            // A task names a program in the spelling it is stored in.
            let mut tasks = b.tasks;
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
                module: b.module,
                target: b.target,
                run: b.run,
                tasks,
            };
            let at = file.runtimes.iter().position(|r| r.id == id);
            let created = at.is_none();
            let at = match at {
                Some(at) => {
                    file.runtimes[at] = entry.clone();
                    at
                }
                None => {
                    file.runtimes.push(entry.clone());
                    file.runtimes.len() - 1
                }
            };
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
            let at = position(&file.runtimes, id).ok_or_else(|| no_runtime(id))?;
            let gone = file.runtimes.remove(at);
            save(data, &file)?;
            event = Some(json!({ "project": project, "runtime": gone.id, "what": "deleted" }));
            Ok(json!({ "deleted": gone.id }))
        })(),
        (method, route) => Err(error(
            404,
            format!("the logic module has no route {method} {route}"),
        )),
    };
    (reply(result, 200), event)
}

#[cfg(test)]
mod tests;
