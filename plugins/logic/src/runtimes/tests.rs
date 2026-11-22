//! The task configuration's storage and its four routes (C8c stage 1), served by [`Logic`] as
//! the server calls it.

use super::*;
use crate::{
    temporary_path, Event, Logic, LOCK_FILE, PROGRAMS_CHANGED_EVENT, RUNTIMES_CHANGED_EVENT,
};
use std::path::PathBuf;

/// A project's directory as the core makes it, with the module's own `logic` in it.
struct Project {
    root: tempfile::TempDir,
    logic: PathBuf,
}

fn project() -> Project {
    let root = tempfile::tempdir().unwrap();
    let logic = root.path().join("logic");
    std::fs::create_dir_all(&logic).unwrap();
    Project { root, logic }
}

impl Project {
    fn tasks_file(&self) -> PathBuf {
        self.logic.join(TASKS_FILE)
    }

    /// Every file under the project, with its bytes.
    fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let path = e.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    let bytes = std::fs::read(&path).unwrap();
                    out.push((path, bytes));
                }
            }
        }
        let mut out = Vec::new();
        walk(self.root.path(), &mut out);
        out.sort();
        out
    }
}

struct Answer {
    status: u16,
    body: Value,
    event: Option<Event>,
}

async fn send(
    logic: &Logic,
    p: &Project,
    method: &str,
    route: &str,
    params: &[(&str, &str)],
    body: Option<Value>,
) -> Answer {
    let params: serde_json::Map<String, Value> = params
        .iter()
        .map(|(k, v)| ((*k).to_string(), json!(v)))
        .collect();
    let call: HttpCall = serde_json::from_value(json!({
        "method": method, "route": route, "params": params, "body": body,
        "project": "plant", "data_dir": p.logic.display().to_string(),
    }))
    .unwrap();
    let (reply, event) = logic.serve(&call).await;
    Answer {
        status: reply.status,
        body: reply.body,
        event,
    }
}

fn ok(a: &Answer) -> &Value {
    assert_eq!(a.status, 200, "{}", a.body);
    &a.body
}

async fn program(logic: &Logic, p: &Project, name: &str, interval_ms: u64) {
    let a = send(
        logic,
        p,
        "POST",
        "/programs",
        &[],
        Some(json!({ "name": name, "interval_ms": interval_ms })),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.body);
}

async fn put(logic: &Logic, p: &Project, id: &str, body: Value) -> Answer {
    send(
        logic,
        p,
        "PUT",
        "/runtimes/:runtime",
        &[("runtime", id)],
        Some(body),
    )
    .await
}

async fn get(logic: &Logic, p: &Project, id: &str) -> Answer {
    send(
        logic,
        p,
        "GET",
        "/runtimes/:runtime",
        &[("runtime", id)],
        None,
    )
    .await
}

async fn delete(logic: &Logic, p: &Project, id: &str) -> Answer {
    send(
        logic,
        p,
        "DELETE",
        "/runtimes/:runtime",
        &[("runtime", id)],
        None,
    )
    .await
}

async fn list(logic: &Logic, p: &Project) -> Value {
    ok(&send(logic, p, "GET", "/runtimes", &[], None).await).clone()
}

/// A runtime of one cyclic task `t` at 10 ms running `programs`.
fn one_task(programs: &[&str]) -> Value {
    json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "t", "kind": { "type": "cyclic", "interval_ms": 10 }, "priority": 1, "programs": programs }
    ]})
}

fn refused(a: &Answer, status: u16, words: &str) {
    assert_eq!(a.status, status, "{}", a.body);
    assert_eq!(a.body, json!({ "error": words }));
    assert!(a.event.is_none());
}

/// The implied runtime of a program as the protocol makes it.
fn implied_of(name: &str, interval_ms: u64, module: &str, target: &str, run: bool) -> Value {
    json!({ "id": name, "module": module, "target": target, "run": run,
            "tasks": [implied_task(&stub(name, interval_ms))] })
}

/// §3.2 of the C8c contract, as `to_vec_pretty` writes it, with a newline.
const LITERAL: &str = r#"{
  "runtimes": [
    {
      "id": "Line1",
      "module": "vm",
      "target": "pc",
      "run": true,
      "tasks": [
        {
          "name": "fast",
          "kind": {
            "type": "cyclic",
            "interval_ms": 10,
            "phase_ms": 0
          },
          "priority": 1,
          "programs": [
            "Writer"
          ]
        },
        {
          "name": "slow",
          "kind": {
            "type": "cyclic",
            "interval_ms": 100,
            "phase_ms": 0
          },
          "priority": 10,
          "watchdog_ms": 3000,
          "programs": [
            "Reader"
          ]
        }
      ]
    }
  ]
}
"#;

#[tokio::test]
async fn a_project_without_a_task_configuration_has_no_runtimes_and_each_program_runs_alone() {
    let (logic, p) = (Logic::default(), project());
    // A project of today, as written before there were runtimes: no lock file, no tasks.json.
    std::fs::create_dir_all(p.logic.join("programs")).unwrap();
    std::fs::write(
        p.logic.join("programs.json"),
        json!({ "programs": [
            { "name": "Tank", "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true },
            { "name": "Pump", "language": "st", "interval_ms": 50, "runtime": "other", "target": "rp2040", "run": false }
        ]})
        .to_string(),
    )
    .unwrap();
    for name in ["Tank", "Pump"] {
        std::fs::write(
            p.logic.join("programs").join(format!("{name}.st")),
            format!("PROGRAM {name}\nEND_PROGRAM\n"),
        )
        .unwrap();
    }
    let before = p.files();
    let listed = list(&logic, &p).await;
    assert_eq!(
        listed,
        json!({
            "runtimes": [],
            "implied": [implied_of("Tank", 20, "vm", "pc", true), implied_of("Pump", 50, "other", "rp2040", false)],
            "problems": [],
        })
    );
    // Pinned, so a change of the protocol's implied task is seen here.
    assert_eq!(
        listed["implied"][0]["tasks"],
        json!([{ "name": "Tank", "kind": { "type": "cyclic", "interval_ms": 20, "phase_ms": 0 },
                 "priority": 31, "programs": ["Tank"] }])
    );
    let one = get(&logic, &p, "Tank").await;
    assert_eq!(
        *ok(&one),
        json!({ "runtime": implied_of("Tank", 20, "vm", "pc", true), "implied": true })
    );
    assert_eq!(ok(&get(&logic, &p, "pump").await)["runtime"]["id"], "Pump");
    refused(
        &get(&logic, &p, "Mixer").await,
        404,
        "the project has no runtime 'Mixer'",
    );
    // The program routes that read answer as they did.
    let programs = send(&logic, &p, "GET", "/programs", &[], None).await;
    assert_eq!(ok(&programs)["programs"][0]["name"], "Tank");
    let checked = send(
        &logic,
        &p,
        "POST",
        "/programs/:program/check",
        &[("program", "Tank")],
        None,
    )
    .await;
    assert_eq!(ok(&checked)["ok"], true, "{}", checked.body);
    // No read made a file: no tasks.json, no lock file.
    assert_eq!(p.files(), before);
    // An implied runtime is not deleted: it is not configured. Nothing is written.
    refused(
        &delete(&logic, &p, "Tank").await,
        404,
        "the project has no runtime 'Tank'",
    );
    assert!(!p.tasks_file().exists());
}

#[tokio::test]
async fn a_runtime_is_stored_as_written_and_read_back() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "Writer", 20).await;
    program(&logic, &p, "Reader", 50).await;
    // Another spelling of each program, and a phase left out.
    let body = json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "fast", "kind": { "type": "cyclic", "interval_ms": 10 }, "priority": 1, "programs": ["writer"] },
        { "name": "slow", "kind": { "type": "cyclic", "interval_ms": 100, "phase_ms": 0 }, "priority": 10,
          "watchdog_ms": 3000, "programs": ["READER"] }
    ]});
    let saved = put(&logic, &p, "Line1", body.clone()).await;
    let literal: Value = serde_json::from_str(LITERAL).unwrap();
    assert_eq!(
        *ok(&saved),
        json!({ "runtime": literal["runtimes"][0], "created": true })
    );
    assert_eq!(
        saved.event,
        Some((
            RUNTIMES_CHANGED_EVENT,
            json!({ "project": "plant", "runtime": "Line1", "what": "saved" })
        ))
    );
    assert_eq!(std::fs::read_to_string(p.tasks_file()).unwrap(), LITERAL);
    let again = put(&logic, &p, "Line1", body).await;
    assert_eq!(ok(&again)["created"], false);
    assert_eq!(std::fs::read_to_string(p.tasks_file()).unwrap(), LITERAL);
    assert_eq!(
        list(&logic, &p).await,
        json!({ "runtimes": literal["runtimes"], "implied": [], "problems": [] })
    );
    assert_eq!(
        *ok(&get(&logic, &p, "line1").await),
        json!({ "runtime": literal["runtimes"][0], "implied": false })
    );
    // Only the module's own temporaries end in .tmp: none is left.
    let left: Vec<_> = std::fs::read_dir(&p.logic)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp") && n != LOCK_FILE)
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[tokio::test]
async fn a_runtime_is_refused_for_each_thing_its_tasks_get_wrong() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    program(&logic, &p, "B", 20).await;
    let task = |name: &str, kind: Value, priority: u64, programs: &[&str]| json!({ "name": name, "kind": kind, "priority": priority, "programs": programs });
    let cyclic = json!({ "type": "cyclic", "interval_ms": 10 });
    let runtime =
        |tasks: Vec<Value>| json!({ "module": "vm", "target": "pc", "run": true, "tasks": tasks });
    let rows: Vec<(Value, &str)> = vec![
        (runtime(vec![]), "runtime 'R' has no task"),
        (
            runtime(vec![task("t", cyclic.clone(), 1, &[])]),
            "runtime 'R': it runs no program",
        ),
        (
            runtime(vec![
                task("t", cyclic.clone(), 1, &["A"]),
                task("u", cyclic.clone(), 1, &[]),
            ]),
            "runtime 'R': task 'u' runs no program",
        ),
        (
            runtime(vec![
                task("t", cyclic.clone(), 1, &["A"]),
                task("u", cyclic.clone(), 2, &["a"]),
            ]),
            "runtime 'R': program 'A' is in tasks 't' and 'u'; a program is in one task",
        ),
        (
            runtime(vec![task("t", cyclic.clone(), 1, &["A", "a"])]),
            "runtime 'R': task 't' names 'A' twice",
        ),
        (
            runtime(vec![task("a b", cyclic.clone(), 1, &["A"])]),
            "runtime 'R': 'a b' is not a task name: use 1 to 64 letters, digits, '-' or '_'",
        ),
        (
            runtime(vec![
                task("t", cyclic.clone(), 1, &["A"]),
                task("t", cyclic.clone(), 2, &["B"]),
            ]),
            "runtime 'R': task name 't' is used more than once",
        ),
        (
            runtime(vec![task("t", cyclic.clone(), 32, &["A"])]),
            "runtime 'R': task 't' has priority 32; priorities are 0 (highest) to 31",
        ),
        (
            runtime(vec![task(
                "t",
                json!({ "type": "cyclic", "interval_ms": 0 }),
                1,
                &["A"],
            )]),
            "runtime 'R': task 't' has interval_ms 0; it must be at least 1",
        ),
        (
            runtime(vec![task(
                "t",
                json!({ "type": "cyclic", "interval_ms": u64::MAX / 1000 }),
                1,
                &["A"],
            )]),
            "runtime 'R': task 't': interval_ms is too long",
        ),
        (
            runtime(vec![task(
                "t",
                json!({ "type": "event", "tag": " " }),
                1,
                &["A"],
            )]),
            "runtime 'R': event task 't' names no tag",
        ),
        (
            runtime(vec![
                json!({ "name": "t", "kind": cyclic, "priority": 1, "watchdog_ms": 0, "programs": ["A"] }),
            ]),
            "runtime 'R': task 't': watchdog_ms must be at least 1",
        ),
        (
            json!({ "module": " ", "target": "pc", "run": true, "tasks": [task("t", cyclic.clone(), 1, &["A"])] }),
            "runtime 'R' names no module",
        ),
        (
            json!({ "module": "vm", "target": "z80", "run": true, "tasks": [task("t", cyclic.clone(), 1, &["A"])] }),
            "'z80' is not a target profile",
        ),
    ];
    for (body, words) in rows {
        refused(&put(&logic, &p, "R", body).await, 400, words);
        assert!(!p.tasks_file().exists(), "{words}: written");
    }
    // What serde refuses: a missing priority, a field a task does not have, a field a kind
    // does not have, a missing key of the runtime.
    for body in [
        runtime(vec![
            json!({ "name": "t", "kind": cyclic, "programs": ["A"] }),
        ]),
        runtime(vec![
            json!({ "name": "t", "kind": cyclic, "priority": 1, "programs": ["A"], "phase": 0 }),
        ]),
        runtime(vec![task(
            "t",
            json!({ "type": "cyclic", "interval_ms": 10, "offset_ms": 5 }),
            1,
            &["A"],
        )]),
        runtime(vec![task(
            "t",
            json!({ "type": "startup", "tag": "x" }),
            1,
            &["A"],
        )]),
        json!({ "module": "vm", "target": "pc", "tasks": [task("t", cyclic.clone(), 1, &["A"])] }),
        json!({ "id": "R", "module": "vm", "target": "pc", "run": true, "tasks": [task("t", cyclic.clone(), 1, &["A"])] }),
    ] {
        let a = put(&logic, &p, "R", body.clone()).await;
        assert_eq!(a.status, 400, "{body}: {}", a.body);
        assert!(
            a.body["error"]
                .as_str()
                .unwrap()
                .starts_with("the body is not what this route takes: "),
            "{}",
            a.body
        );
        assert!(!p.tasks_file().exists(), "{body}: written");
    }
    // A refusal of a replacement leaves the stored runtime as it was.
    ok(&put(&logic, &p, "R", one_task(&["A"])).await);
    let stored = std::fs::read(p.tasks_file()).unwrap();
    refused(
        &put(
            &logic,
            &p,
            "R",
            runtime(vec![task("t", cyclic.clone(), 40, &["A"])]),
        )
        .await,
        400,
        "runtime 'R': task 't' has priority 40; priorities are 0 (highest) to 31",
    );
    assert_eq!(std::fs::read(p.tasks_file()).unwrap(), stored);
}

#[tokio::test]
async fn a_task_names_only_programs_of_the_project() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    refused(
        &put(&logic, &p, "R", one_task(&["A", "Ghost"])).await,
        400,
        "runtime 'R': task 't' runs 'Ghost': the project has no program 'Ghost'",
    );
    // Said before what the tasks get wrong among themselves.
    let twice = json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "t", "kind": { "type": "startup" }, "priority": 1, "programs": ["Ghost"] },
        { "name": "u", "kind": { "type": "startup" }, "priority": 1, "programs": ["Ghost"] }
    ]});
    refused(
        &put(&logic, &p, "R", twice).await,
        400,
        "runtime 'R': task 't' runs 'Ghost': the project has no program 'Ghost'",
    );
    assert!(!p.tasks_file().exists());
}

#[tokio::test]
async fn a_program_is_in_one_runtime_of_the_project() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    program(&logic, &p, "B", 20).await;
    ok(&put(&logic, &p, "R1", one_task(&["A"])).await);
    let stored = std::fs::read(p.tasks_file()).unwrap();
    refused(
        &put(&logic, &p, "R2", one_task(&["B", "a"])).await,
        409,
        "program 'A' is in runtime 'R1' already; a program is in one runtime",
    );
    assert_eq!(std::fs::read(p.tasks_file()).unwrap(), stored);
    // A runtime is not compared with itself.
    ok(&put(&logic, &p, "R1", one_task(&["A"])).await);
    ok(&put(&logic, &p, "R2", one_task(&["B"])).await);
    let listed = list(&logic, &p).await;
    assert_eq!(listed["implied"], json!([]));
    assert_eq!(listed["problems"], json!([]));
    // Moved: out of R1 first, then into R2.
    refused(
        &put(&logic, &p, "R2", one_task(&["B", "A"])).await,
        409,
        "program 'A' is in runtime 'R1' already; a program is in one runtime",
    );
}

#[tokio::test]
async fn a_runtime_is_never_named_like_a_program_nor_a_program_like_a_runtime() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "Tank", 20).await;
    program(&logic, &p, "A", 20).await;
    refused(
        &put(&logic, &p, "tank", one_task(&["A"])).await,
        409,
        "'tank' is a program of the project; a runtime is not named like a program",
    );
    assert!(!p.tasks_file().exists());
    ok(&put(&logic, &p, "Line", one_task(&["A"])).await);
    let before = p.files();
    let a = send(
        &logic,
        &p,
        "POST",
        "/programs",
        &[],
        Some(json!({ "name": "LINE" })),
    )
    .await;
    refused(
        &a,
        409,
        "the project has a runtime 'Line'; a program is not named like a runtime",
    );
    assert_eq!(p.files(), before, "nothing written");
}

#[tokio::test]
async fn a_runtime_id_is_one_name_without_regard_to_case_and_never_a_device() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    program(&logic, &p, "B", 20).await;
    program(&logic, &p, "C", 20).await;
    for id in ["CON", "nul", "Lpt1"] {
        refused(
            &put(&logic, &p, id, one_task(&["A"])).await,
            400,
            &format!("'{id}' cannot be a runtime id: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"),
        );
    }
    let long = "a".repeat(65);
    for id in ["a.b", long.as_str(), "", "a b"] {
        let words = format!("'{id}' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'");
        refused(&put(&logic, &p, id, one_task(&["A"])).await, 400, &words);
        // Before anything is looked up, on every route that names a runtime.
        refused(&get(&logic, &p, id).await, 400, &words);
        refused(&delete(&logic, &p, id).await, 400, &words);
    }
    assert!(!p.tasks_file().exists());
    ok(&put(&logic, &p, "Con1", one_task(&["A"])).await);
    ok(&put(&logic, &p, "line-1", one_task(&["B"])).await);
    refused(
        &put(&logic, &p, "LINE-1", one_task(&["C"])).await,
        409,
        "the project already has a runtime 'line-1'; names that differ only in case are one name",
    );
    assert_eq!(
        list(&logic, &p).await["runtimes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].clone())
            .collect::<Vec<_>>(),
        [json!("Con1"), json!("line-1")]
    );
}

#[tokio::test]
async fn a_new_program_is_never_named_like_a_device_and_one_already_so_named_still_works() {
    let (logic, p) = (Logic::default(), project());
    for name in ["Aux", "com1", "LPT9"] {
        refused(
            &send(&logic, &p, "POST", "/programs", &[], Some(json!({ "name": name }))).await,
            400,
            &format!("'{name}' cannot be a program name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"),
        );
    }
    assert!(!p.logic.join("programs.json").exists());
    // One made before the rule.
    std::fs::create_dir_all(p.logic.join("programs")).unwrap();
    std::fs::write(
        p.logic.join("programs.json"),
        json!({ "programs": [{ "name": "Con", "interval_ms": 20, "runtime": "vm", "target": "pc", "run": false }] })
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        p.logic.join("programs").join("Con.st"),
        "PROGRAM Con\nVAR x : INT; END_VAR\nx := x + 1;\nEND_PROGRAM\n",
    )
    .unwrap();
    let con = [("program", "Con")];
    ok(&send(&logic, &p, "GET", "/programs/:program", &con, None).await);
    let checked = send(&logic, &p, "POST", "/programs/:program/check", &con, None).await;
    assert_eq!(ok(&checked)["ok"], true, "{}", checked.body);
    let built = send(&logic, &p, "POST", "/programs/:program/build", &con, None).await;
    assert_eq!(ok(&built)["instance"]["id"], "Con");
    assert_eq!(ok(&get(&logic, &p, "Con").await)["implied"], true);
    let deleted = send(&logic, &p, "DELETE", "/programs/:program", &con, None).await;
    assert_eq!(*ok(&deleted), json!({ "deleted": "Con" }));
    assert_eq!(
        deleted.event,
        Some((
            PROGRAMS_CHANGED_EVENT,
            json!({ "project": "plant", "program": "Con", "what": "deleted" })
        ))
    );
}

#[tokio::test]
async fn a_program_in_a_runtime_cannot_be_deleted_until_it_is_taken_out() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    program(&logic, &p, "B", 20).await;
    let body = json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "fast", "kind": { "type": "cyclic", "interval_ms": 10 }, "priority": 1, "programs": ["A"] },
        { "name": "slow", "kind": { "type": "cycle" }, "priority": 5, "programs": ["B"] }
    ]});
    ok(&put(&logic, &p, "R", body).await);
    let before = p.files();
    let a = [("program", "A")];
    refused(
        &send(&logic, &p, "DELETE", "/programs/:program", &a, None).await,
        409,
        "program 'A' is in task 'fast' of runtime 'R': take it out of the runtime first",
    );
    assert_eq!(p.files(), before, "nothing written or removed");
    ok(&put(&logic, &p, "R", one_task(&["B"])).await);
    let deleted = send(&logic, &p, "DELETE", "/programs/:program", &a, None).await;
    assert_eq!(*ok(&deleted), json!({ "deleted": "A" }));
    assert!(!p.logic.join("programs").join("A.st").exists());
}

#[tokio::test]
async fn deleting_a_runtime_lets_its_programs_run_alone_again() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    program(&logic, &p, "B", 30).await;
    ok(&put(&logic, &p, "R", one_task(&["A", "B"])).await);
    assert_eq!(list(&logic, &p).await["implied"], json!([]));
    let deleted = delete(&logic, &p, "r").await;
    assert_eq!(*ok(&deleted), json!({ "deleted": "R" }));
    assert_eq!(
        deleted.event,
        Some((
            RUNTIMES_CHANGED_EVENT,
            json!({ "project": "plant", "runtime": "R", "what": "deleted" })
        ))
    );
    // The file stays, with no runtime in it.
    assert_eq!(
        std::fs::read_to_string(p.tasks_file()).unwrap(),
        "{\n  \"runtimes\": []\n}\n"
    );
    assert_eq!(
        list(&logic, &p).await,
        json!({
            "runtimes": [],
            "implied": [implied_of("A", 20, "vm", "pc", false), implied_of("B", 30, "vm", "pc", false)],
            "problems": [],
        })
    );
    refused(
        &delete(&logic, &p, "R").await,
        404,
        "the project has no runtime 'R'",
    );
}

#[tokio::test]
async fn a_task_configuration_that_cannot_be_read_refuses_every_route_that_needs_it_and_names_the_file(
) {
    for text in [
        "{ not json",
        r#"{"runtimes": [], "more": 1}"#,
        r#"{"runtimes": [{"id": "R", "module": "vm", "target": "pc", "tasks": []}]}"#,
    ] {
        let (logic, p) = (Logic::default(), project());
        program(&logic, &p, "A", 20).await;
        std::fs::write(p.tasks_file(), text).unwrap();
        let before = p.files();
        let a = [("program", "A")];
        let r = [("runtime", "R")];
        for (method, route, params, body) in [
            ("GET", "/runtimes", &[][..], None),
            ("GET", "/runtimes/:runtime", &r[..], None),
            ("PUT", "/runtimes/:runtime", &r[..], Some(one_task(&["A"]))),
            ("DELETE", "/runtimes/:runtime", &r[..], None),
            ("POST", "/programs", &[][..], Some(json!({ "name": "B" }))),
            ("DELETE", "/programs/:program", &a[..], None),
        ] {
            let answer = send(&logic, &p, method, route, params, body).await;
            assert_eq!(answer.status, 500, "{method} {route}: {}", answer.body);
            let said = answer.body["error"].as_str().unwrap();
            assert!(
                said.starts_with("logic/tasks.json cannot be read as the project's runtimes: "),
                "{method} {route}: {said}"
            );
            assert!(answer.event.is_none());
        }
        assert_eq!(p.files(), before, "{text}: nothing written");
        // What does not need it answers.
        ok(&send(&logic, &p, "GET", "/programs", &[], None).await);
        ok(&send(&logic, &p, "GET", "/programs/:program", &a, None).await);
    }
}

#[tokio::test]
async fn a_hand_edited_configuration_is_listed_with_its_problems() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    program(&logic, &p, "B", 20).await;
    let task = |name: &str, programs: &[&str]| json!({ "name": name, "kind": { "type": "startup" }, "priority": 3, "programs": programs });
    let file = json!({ "runtimes": [
        { "id": "A", "module": "", "target": "z80", "run": false, "tasks": [task("t", &["Ghost"])] },
        { "id": "line", "module": "vm", "target": "pc", "run": true, "tasks": [task("t", &["B"])] },
        { "id": "LINE", "module": "vm", "target": "pc", "run": true, "tasks": [task("u", &["b"])] },
        { "id": "nul", "module": "vm", "target": "pc", "run": true, "tasks": [] }
    ]});
    std::fs::write(p.tasks_file(), file.to_string()).unwrap();
    let before = p.files();
    let listed = list(&logic, &p).await;
    assert_eq!(listed["runtimes"], file["runtimes"]);
    assert_eq!(
        listed["implied"],
        json!([implied_of("A", 20, "vm", "pc", false)])
    );
    assert_eq!(
        listed["problems"],
        json!([
            "'A' is a program of the project; a runtime is not named like a program",
            "runtime 'A' names no module",
            "'z80' is not a target profile",
            "runtime 'A': task 't' runs 'Ghost': the project has no program 'Ghost'",
            "the project already has a runtime 'line'; names that differ only in case are one name",
            "program 'B' is in runtime 'line' already; a program is in one runtime",
            "'nul' cannot be a runtime id: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)",
            "runtime 'nul' has no task",
        ])
    );
    // The configured one is found first, then the exact spelling.
    assert_eq!(ok(&get(&logic, &p, "A").await)["implied"], false);
    assert_eq!(ok(&get(&logic, &p, "LINE").await)["runtime"]["id"], "LINE");
    assert_eq!(p.files(), before, "a read writes nothing");
    // A write of one runtime is checked against the others as they are.
    refused(
        &put(&logic, &p, "line", one_task(&["B"])).await,
        409,
        "the project already has a runtime 'LINE'; names that differ only in case are one name",
    );
    assert_eq!(
        configuration_problems(&load_programs(&p.logic).unwrap(), &load(&p.logic).unwrap()).len(),
        8
    );
}

/// One round of a race on a project holding program `P` and no runtime: one call deletes `P`,
/// the other saves runtime `R` running `P`, at the same moment, each served by a `Logic` of its
/// own (each its own mutex, as two processes during a hot swap). The statuses, and the problems
/// of the files afterwards.
fn race() -> (u16, u16, Vec<String>) {
    let p = project();
    let rt = || {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    };
    let (first, second) = (Logic::default(), Logic::default());
    rt().block_on(program(&first, &p, "P", 20));
    let barrier = std::sync::Barrier::new(2);
    let (deleted, saved) = std::thread::scope(|s| {
        let a = s.spawn(|| {
            let rt = rt();
            barrier.wait();
            rt.block_on(send(
                &first,
                &p,
                "DELETE",
                "/programs/:program",
                &[("program", "P")],
                None,
            ))
            .status
        });
        let b = s.spawn(|| {
            let rt = rt();
            barrier.wait();
            rt.block_on(put(&second, &p, "R", one_task(&["P"]))).status
        });
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(
        p.logic.join(LOCK_FILE).is_file(),
        "a write makes the lock file"
    );
    let problems =
        configuration_problems(&load_programs(&p.logic).unwrap(), &load(&p.logic).unwrap());
    (deleted, saved, problems)
}

#[test]
fn a_runtime_write_holds_a_lock_another_process_waits_on() {
    for round in 0..100 {
        let (deleted, saved, problems) = race();
        assert!(problems.is_empty(), "round {round}: {problems:?}");
        assert!(
            matches!((deleted, saved), (200, 400) | (409, 200)),
            "round {round}: delete {deleted}, save {saved}: exactly one may be done"
        );
    }
}

/// A link planted at the temporary's name (the pid is predictable) is not written through:
/// the save stays inside the module's directory and `tasks.json` is a file.
#[cfg(unix)]
#[tokio::test]
async fn a_link_at_the_task_configurations_temporary_is_not_written_through() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    let outside = p.root.path().join("outside.txt");
    std::fs::write(&outside, "precious\n").unwrap();
    let tmp = temporary_path(&p.tasks_file()).unwrap();
    std::os::unix::fs::symlink(&outside, &tmp).unwrap();
    ok(&put(&logic, &p, "R", one_task(&["A"])).await);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "precious\n");
    assert!(std::fs::symlink_metadata(p.tasks_file())
        .unwrap()
        .file_type()
        .is_file());
    assert!(std::fs::symlink_metadata(&tmp).is_err(), "the link is gone");
    // programs.json goes the same way now.
    let programs = p.logic.join("programs.json");
    std::os::unix::fs::symlink(&outside, temporary_path(&programs).unwrap()).unwrap();
    program(&logic, &p, "B", 20).await;
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "precious\n");
    assert!(std::fs::symlink_metadata(&programs)
        .unwrap()
        .file_type()
        .is_file());
}

#[test]
fn the_manifest_declares_the_runtime_routes_each_a_tool_of_its_own() {
    let manifest = crate::manifest();
    // Method, path, tool, least role, and what the server must give it.
    let routes: Vec<String> = manifest
        .routes
        .iter()
        .filter(|r| r.path.starts_with("/runtimes"))
        .map(|r| {
            format!(
                "{} {} {} {} {:?}",
                r.method,
                r.path,
                r.tool,
                r.role.as_deref().unwrap_or("-"),
                r.needs
            )
        })
        .collect();
    assert_eq!(
        routes,
        [
            "GET /runtimes list_logic_runtimes viewer []",
            "GET /runtimes/:runtime get_logic_runtime viewer []",
            "PUT /runtimes/:runtime save_logic_runtime engineer []",
            "DELETE /runtimes/:runtime delete_logic_runtime engineer []",
        ]
    );
    // After the library's, last.
    let tools: Vec<&str> = manifest.routes.iter().map(|r| r.tool.as_str()).collect();
    assert_eq!(
        &tools[tools.len() - 5..],
        [
            "check_logic_library",
            "list_logic_runtimes",
            "get_logic_runtime",
            "save_logic_runtime",
            "delete_logic_runtime"
        ]
    );
    // Each a tool of its own, and every summary says nothing is loaded.
    let mut unique = tools.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), tools.len());
    for r in manifest
        .routes
        .iter()
        .filter(|r| r.path.starts_with("/runtimes"))
    {
        assert!(r.summary.contains("loaded"), "{}: {}", r.tool, r.summary);
        assert!(matches!(
            r.scope,
            mythos_module::protocol::RouteScope::Project
        ));
    }
    // The body the save tool documents is the one the route takes.
    let save = manifest
        .routes
        .iter()
        .find(|r| r.tool == "save_logic_runtime")
        .unwrap();
    assert_eq!(
        save.body.as_ref().unwrap()["required"],
        json!(["module", "target", "run", "tasks"])
    );
    assert_eq!(
        save.body.as_ref().unwrap()["properties"]["tasks"]["items"]["required"],
        json!(["name", "kind", "priority", "programs"])
    );
}
