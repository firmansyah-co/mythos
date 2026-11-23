//! The task configuration's storage and its four routes (C8c stage 1), served by [`Logic`] as
//! the server calls it.

use super::*;
use crate::{Event, Logic, LOCK_FILE, PROGRAMS_CHANGED_EVENT, RUNTIMES_CHANGED_EVENT};
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
    use crate::temporary_path;
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
            "POST /runtimes/:runtime/check check_logic_runtime viewer []",
            "POST /runtimes/:runtime/build build_logic_runtime engineer [\"tags\"]",
        ]
    );
    // After the library's, last.
    let tools: Vec<&str> = manifest.routes.iter().map(|r| r.tool.as_str()).collect();
    assert_eq!(
        &tools[tools.len() - 7..],
        [
            "check_logic_library",
            "list_logic_runtimes",
            "get_logic_runtime",
            "save_logic_runtime",
            "delete_logic_runtime",
            "check_logic_runtime",
            "build_logic_runtime"
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
    // The check takes the same runtime, or none: nothing in it is required.
    let check = manifest
        .routes
        .iter()
        .find(|r| r.tool == "check_logic_runtime")
        .unwrap();
    let schema = check.body.as_ref().unwrap();
    assert!(schema.get("required").is_none(), "{schema}");
    assert_eq!(
        schema["properties"],
        save.body.as_ref().unwrap()["properties"]
    );
    // The program's build no longer tells a caller to compose an instance by hand.
    let build = manifest
        .routes
        .iter()
        .find(|r| r.tool == "build_logic_program")
        .unwrap();
    assert!(!build
        .summary
        .contains("until a runtime of several programs"));
    assert!(
        build.summary.contains("build_logic_runtime"),
        "{}",
        build.summary
    );
}

// -----------------------------------------------------------------------------
// Checking and building a runtime (C8c stage 2)
// -----------------------------------------------------------------------------

const SHARED: &str = "TYPE Shared : STRUCT level : INT; spare : INT := 5; END_STRUCT; END_TYPE\n";

/// Writes `S.level` from its input; nothing else of S.
const WRITER: &str = "PROGRAM Writer
VAR_INPUT level : INT; END_VAR
VAR_EXTERNAL S : Shared; END_VAR
  S.level := level;
END_PROGRAM";

/// Drives the speed from S, which it only reads.
const READER: &str = "PROGRAM Reader
VAR_OUTPUT speed : INT; END_VAR
VAR_EXTERNAL S : Shared; END_VAR
  speed := S.level * 10 + S.spare;
END_PROGRAM";

const LEVEL: &str = "external.P.LT1.level";
const SPEED: &str = "external.P.P1.speed";

impl Project {
    /// The data module's files: the type `Shared` and `blocks`, each `(name, retain)` of it.
    fn data(&self, blocks: &[(&str, bool)]) {
        let data = self.root.path().join("data");
        std::fs::create_dir_all(data.join("types")).unwrap();
        std::fs::write(data.join("types").join("Shared.st"), SHARED).unwrap();
        let blocks: Vec<Value> = blocks
            .iter()
            .map(|(n, r)| json!({ "name": n, "type": "Shared", "retain": r }))
            .collect();
        std::fs::write(
            data.join("blocks.json"),
            json!({ "blocks": blocks }).to_string(),
        )
        .unwrap();
    }
}

/// Program `name` with `source` and `settings` (merged over interval 20, vm, pc, run).
async fn written(logic: &Logic, p: &Project, name: &str, source: &str, settings: Value) {
    program(logic, p, name, 20).await;
    let mut all = json!({ "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true });
    for (k, v) in settings.as_object().unwrap() {
        all[k] = v.clone();
    }
    let n = [("program", name)];
    ok(&send(
        logic,
        p,
        "PUT",
        "/programs/:program/source",
        &n,
        Some(json!({ "source": source })),
    )
    .await);
    ok(&send(
        logic,
        p,
        "PUT",
        "/programs/:program/settings",
        &n,
        Some(all),
    )
    .await);
}

/// A program of Structured Text named `name` that counts in `x`.
fn counter(name: &str) -> String {
    format!("PROGRAM {name}\nVAR_OUTPUT x : INT; END_VAR\n  x := x + 1;\nEND_PROGRAM")
}

async fn writer_and_reader(logic: &Logic, p: &Project) {
    p.data(&[("S", false)]);
    written(
        logic,
        p,
        "Writer",
        WRITER,
        json!({ "inputs": [{ "variable": "level", "tag": LEVEL }] }),
    )
    .await;
    written(
        logic,
        p,
        "Reader",
        READER,
        json!({ "outputs": [{ "variable": "speed", "tag": SPEED, "safe": 0 }] }),
    )
    .await;
}

/// Runtime `Line`: Writer in `fast`, Reader in `slow`.
fn line() -> Value {
    json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "fast", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["Writer"] },
        { "name": "slow", "kind": { "type": "cyclic", "interval_ms": 50 }, "priority": 5, "programs": ["Reader"] }
    ]})
}

async fn build_of(logic: &Logic, p: &Project, id: &str, tags: Option<&[&str]>) -> Answer {
    let mut call: HttpCall = serde_json::from_value(json!({
        "method": "POST", "route": "/runtimes/:runtime/build", "params": { "runtime": id },
        "project": "plant", "data_dir": p.logic.display().to_string(),
    }))
    .unwrap();
    call.tags = tags.map(|t| t.iter().map(|s| (*s).to_string()).collect());
    let (reply, event) = logic.serve(&call).await;
    Answer {
        status: reply.status,
        body: reply.body,
        event,
    }
}

async fn built(logic: &Logic, p: &Project, id: &str) -> Value {
    let a = build_of(logic, p, id, None).await;
    assert!(a.event.is_none());
    ok(&a).clone()
}

async fn check_of(logic: &Logic, p: &Project, id: &str, draft: Option<Value>) -> Value {
    let a = send(
        logic,
        p,
        "POST",
        "/runtimes/:runtime/check",
        &[("runtime", id)],
        draft,
    )
    .await;
    assert!(a.event.is_none());
    ok(&a).clone()
}

fn sorted_keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort_unstable();
    k
}

fn names(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect()
}

/// The block `S` of `Shared` as a runtime's data writes it, laid out by the compiler.
fn block_s(retain: bool) -> Value {
    let layouts = mythos_logic_compiler::check_project(&mythos_logic_compiler::Sources {
        library: &[("data/types/Shared.st", SHARED)],
        data: &[mythos_logic_compiler::DataBlock {
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

#[tokio::test]
async fn a_runtime_of_several_programs_is_built_with_its_tasks_in_order() {
    let (logic, p) = (Logic::default(), project());
    for name in ["A", "B"] {
        written(&logic, &p, name, &counter(name), json!({})).await;
    }
    // Its PROGRAM spelled otherwise, which is one name without regard to case.
    written(
        &logic,
        &p,
        "Pump",
        &counter("PUMP"),
        json!({ "interval_ms": 70 }),
    )
    .await;
    let tasks = json!([
        { "name": "fast", "kind": { "type": "cyclic", "interval_ms": 10, "phase_ms": 0 }, "priority": 1, "programs": ["B"] },
        { "name": "slow", "kind": { "type": "cycle", "min_cycle_ms": 0 }, "priority": 9, "watchdog_ms": 4000, "programs": ["pump", "A"] }
    ]);
    let saved = put(
        &logic,
        &p,
        "Line",
        json!({ "module": "vm", "target": "pc", "run": false, "tasks": tasks }),
    )
    .await;
    ok(&saved);
    let before = p.files();

    let b = built(&logic, &p, "line").await;
    assert_eq!(
        sorted_keys(&b),
        ["instance", "programs", "replaces", "version", "warnings"],
        "{b}"
    );
    let instance = &b["instance"];
    assert_eq!(
        sorted_keys(instance),
        ["id", "module", "programs", "run", "tasks"]
    );
    assert_eq!(instance["id"], "Line", "the stored spelling");
    assert_eq!(
        (&instance["module"], &instance["run"]),
        (&json!("vm"), &json!(false))
    );
    // In task order, then in each task's order; named by the compiled PROGRAM.
    assert_eq!(names(&instance["programs"]), ["B", "PUMP", "A"]);
    let mut emitted = tasks.clone();
    emitted[1]["programs"] = json!(["PUMP", "A"]);
    assert_eq!(instance["tasks"], emitted);
    // The stored interval, which a runtime with tasks does not use; never one made up.
    assert_eq!(instance["programs"][1]["interval_ms"], 70);
    assert_eq!(names(&b["programs"]), ["B", "Pump", "A"]);
    for (i, program) in b["programs"].as_array().unwrap().iter().enumerate() {
        assert_eq!(program["version"], instance["programs"][i]["version"]);
        assert_eq!(
            sorted_keys(program),
            ["code_size", "memory_size", "name", "version"]
        );
    }
    assert_eq!(b["warnings"], json!([]));
    // A build and a check write nothing.
    check_of(&logic, &p, "Line", None).await;
    assert_eq!(p.files(), before);
}

#[tokio::test]
async fn a_configured_runtime_of_one_program_gives_it_in_program_with_its_tasks() {
    let (logic, p) = (Logic::default(), project());
    // Alone in a runtime, a program may name its PROGRAM otherwise: nothing is prefixed.
    written(&logic, &p, "A", &counter("Other"), json!({})).await;
    let task = json!({ "name": "t", "kind": { "type": "cyclic", "interval_ms": 10, "phase_ms": 0 }, "priority": 1, "programs": ["A"] });
    ok(&put(
        &logic,
        &p,
        "R",
        json!({ "module": "vm", "target": "pc", "run": true, "tasks": [task] }),
    )
    .await);
    let b = built(&logic, &p, "R").await;
    let instance = &b["instance"];
    assert_eq!(
        sorted_keys(instance),
        ["id", "module", "program", "run", "tasks"],
        "{b}"
    );
    assert_eq!(instance["program"]["name"], "Other");
    let mut emitted = task.clone();
    emitted["programs"] = json!(["Other"]);
    assert_eq!(instance["tasks"], json!([emitted]));
    assert_eq!(b["replaces"], json!(["A"]));
    // Its version is its one program's, which is what the core reports of the instance.
    assert_eq!(b["version"], instance["program"]["version"], "{b}");
}

#[tokio::test]
async fn two_programs_sharing_a_block_build_into_one_instance_with_the_block_once() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    ok(&put(&logic, &p, "Line", line()).await);
    let b = built(&logic, &p, "Line").await;
    assert_eq!(b["instance"]["data"], json!([block_s(false)]), "{b}");
    for program in b["instance"]["programs"].as_array().unwrap() {
        assert_eq!(program["data"][0]["name"], "S", "{program}");
    }
    assert_eq!(names(&b["instance"]["programs"]), ["Writer", "Reader"]);
    // Retained as blocks.json says.
    p.data(&[("S", true)]);
    let b = built(&logic, &p, "Line").await;
    assert_eq!(b["instance"]["data"], json!([block_s(true)]));
}

#[tokio::test]
async fn what_the_core_refuses_of_a_runtime_is_not_built() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    p.data(&[("S", false), ("Recipe", false)]);
    // R11: a second writer of S.level.
    written(
        &logic,
        &p,
        "Other",
        &WRITER.replace("PROGRAM Writer", "PROGRAM Other"),
        json!({ "inputs": [{ "variable": "level", "tag": LEVEL }] }),
    )
    .await;
    let two = json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "t", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["Writer", "Other"] }
    ]});
    ok(&put(&logic, &p, "Two", two).await);
    let said = "runtime 'Two': data block 'S': member 'level' is written by programs 'Writer' and 'Other'; a member has one writer";
    refused(&build_of(&logic, &p, "Two", None).await, 400, said);
    let checked = check_of(&logic, &p, "Two", None).await;
    assert_eq!(checked["ok"], false);
    assert_eq!(checked["problems"], json!([said]), "{checked}");
    assert!(checked.get("version").is_none());

    // R3: a block named like a program of the runtime.
    written(&logic, &p, "Recipe", &counter("Recipe"), json!({})).await;
    written(
        &logic,
        &p,
        "Mixer",
        "PROGRAM Mixer\nVAR_EXTERNAL Recipe : Shared; END_VAR\n  Recipe.level := 1;\nEND_PROGRAM",
        json!({}),
    )
    .await;
    let mixing = json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "t", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["Mixer", "Recipe"] }
    ]});
    ok(&put(&logic, &p, "Mixing", mixing).await);
    refused(
        &build_of(&logic, &p, "Mixing", None).await,
        400,
        "runtime 'Mixing': data block 'Recipe' has the name of a program of this runtime",
    );
    // Each alone builds: it is the runtime that is refused.
    ok(&delete(&logic, &p, "Mixing").await);
    for name in ["Mixer", "Recipe"] {
        let alone = send(
            &logic,
            &p,
            "POST",
            "/programs/:program/build",
            &[("program", name)],
            None,
        )
        .await;
        ok(&alone);
    }
}

#[tokio::test]
async fn two_programs_of_a_runtime_cannot_bind_one_output_nor_publish_one_tag() {
    let (logic, p) = (Logic::default(), project());
    let output = json!({ "outputs": [{ "variable": "x", "tag": SPEED, "safe": 0 }] });
    written(&logic, &p, "A", &counter("A"), output.clone()).await;
    written(&logic, &p, "B", &counter("B"), output).await;
    let both = json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "t", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["A", "B"] }
    ]});
    ok(&put(&logic, &p, "Line", both.clone()).await);
    let said = format!("runtime 'Line': {SPEED} is an output of both 'Line/A' and 'Line/B': one tag has one writer");
    refused(&build_of(&logic, &p, "Line", None).await, 400, &said);
    assert_eq!(
        check_of(&logic, &p, "Line", None).await["problems"],
        json!([said])
    );

    let published =
        json!({ "outputs": [], "published": [{ "variable": "x", "tag": "internal.P.count" }] });
    for name in ["A", "B"] {
        let mut s = json!({ "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true });
        s["published"] = published["published"].clone();
        ok(&send(
            &logic,
            &p,
            "PUT",
            "/programs/:program/settings",
            &[("program", name)],
            Some(s),
        )
        .await);
    }
    refused(
        &build_of(&logic, &p, "Line", None).await,
        400,
        "runtime 'Line': internal.P.count is published by both 'Line/A' and 'Line/B'",
    );
}

#[tokio::test]
async fn an_event_task_whose_tag_no_program_reads_into_a_bool_is_not_built() {
    let (logic, p) = (Logic::default(), project());
    let source = "PROGRAM A\nVAR_INPUT go : BOOL; n : INT; END_VAR\nVAR_OUTPUT x : INT; END_VAR\n  IF go THEN x := x + n; END_IF;\nEND_PROGRAM";
    written(&logic, &p, "A", source, json!({})).await;
    written(&logic, &p, "B", &counter("B"), json!({})).await;
    let tag = "external.P.go";
    ok(&put(&logic, &p, "Line", json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "edge", "kind": { "type": "event", "tag": tag }, "priority": 0, "programs": ["A"] },
        { "name": "t", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["B"] }
    ]})).await);
    let said = format!("runtime 'Line': task 'edge': its tag {tag} is not read into a BOOL variable by any program of this runtime");
    for inputs in [
        json!([]),
        json!([{ "variable": "n", "tag": tag }]),
        json!([{ "variable": "go", "tag": tag, "of": "good" }]),
    ] {
        let s = json!({ "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true, "inputs": inputs });
        ok(&send(
            &logic,
            &p,
            "PUT",
            "/programs/:program/settings",
            &[("program", "A")],
            Some(s),
        )
        .await);
        refused(&build_of(&logic, &p, "Line", None).await, 400, &said);
    }
    // Read as a value into a BOOL: built.
    let s = json!({ "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true,
                    "inputs": [{ "variable": "GO", "tag": tag }] });
    ok(&send(
        &logic,
        &p,
        "PUT",
        "/programs/:program/settings",
        &[("program", "A")],
        Some(s),
    )
    .await);
    let b = built(&logic, &p, "Line").await;
    assert_eq!(
        b["instance"]["tasks"][0]["kind"],
        json!({ "type": "event", "tag": tag })
    );
}

/// A program whose variables take 80 000 bytes: more than the rp2040 profile's 64 KiB.
const BIG: &str =
    "PROGRAM Big\nVAR big : ARRAY [1..40000] OF INT; END_VAR\n  big[1] := big[1] + 1;\nEND_PROGRAM";

#[tokio::test]
async fn a_program_in_a_runtime_is_compiled_for_the_runtimes_target() {
    let (logic, p) = (Logic::default(), project());
    // Its own target says rp2040, where it does not fit: alone it is not built.
    written(&logic, &p, "Big", BIG, json!({ "target": "rp2040" })).await;
    let alone = send(
        &logic,
        &p,
        "POST",
        "/programs/:program/build",
        &[("program", "Big")],
        None,
    )
    .await;
    assert_eq!(alone.status, 400, "{}", alone.body);
    let task = |target: &str| {
        json!({ "module": "vm", "target": target, "run": true, "tasks": [
            { "name": "t", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["Big"] }
        ]})
    };
    ok(&put(&logic, &p, "OnPc", task("pc")).await);
    let b = built(&logic, &p, "OnPc").await;
    assert!(
        b["programs"][0]["memory_size"].as_u64().unwrap() > 64 * 1024,
        "{b}"
    );
    // Its own target pc, the runtime's rp2040: refused, whatever its own says.
    ok(&delete(&logic, &p, "OnPc").await);
    let s = json!({ "interval_ms": 20, "runtime": "vm", "target": "pc", "run": true });
    ok(&send(
        &logic,
        &p,
        "PUT",
        "/programs/:program/settings",
        &[("program", "Big")],
        Some(s),
    )
    .await);
    ok(&put(&logic, &p, "OnChip", task("rp2040")).await);
    let a = build_of(&logic, &p, "OnChip", None).await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        a.body["error"],
        "program 'Big' does not compile and runtime 'OnChip' was not built"
    );
    assert_eq!(a.body["program"], "Big");
    let said = a.body["diagnostics"][0]["message"].as_str().unwrap();
    assert!(said.ends_with("the target rp2040 allows 65536"), "{said}");
    let checked = check_of(&logic, &p, "OnChip", None).await;
    assert_eq!(checked["ok"], false);
    assert_eq!(checked["programs"][0]["diagnostics"], a.body["diagnostics"]);
}

#[tokio::test]
async fn a_program_named_otherwise_in_its_source_is_not_built_into_a_runtime_of_several() {
    let (logic, p) = (Logic::default(), project());
    written(&logic, &p, "A", &counter("A"), json!({})).await;
    written(&logic, &p, "B", &counter("Other"), json!({})).await;
    ok(&put(&logic, &p, "Line", json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
        { "name": "t", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": ["A", "B"] }
    ]})).await);
    let said = "runtime 'Line': program 'B' declares PROGRAM Other in its source; in a runtime of several programs its variables are named B.<variable>, so its PROGRAM must be named B";
    refused(&build_of(&logic, &p, "Line", None).await, 400, said);
    let checked = check_of(&logic, &p, "Line", None).await;
    assert_eq!(checked["problems"], json!([said]), "{checked}");
    assert_eq!(
        checked["programs"][1]["ok"], true,
        "the program itself compiles"
    );
}

#[tokio::test]
async fn a_programs_version_in_a_runtime_follows_its_task_and_not_its_unused_interval() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    ok(&put(&logic, &p, "Line", line()).await);
    let versions = |b: &Value| -> Vec<String> {
        b["programs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["version"].as_str().unwrap().to_string())
            .collect()
    };
    let first = versions(&built(&logic, &p, "Line").await);
    assert_eq!(first.len(), 2);
    // The program's own check gives that version: its version in the runtime.
    let alone = |name: &'static str| {
        let (logic, p) = (&logic, &p);
        async move {
            ok(&send(
                logic,
                p,
                "POST",
                "/programs/:program/check",
                &[("program", name)],
                None,
            )
            .await)["version"]
                .clone()
        }
    };
    let writer_in_runtime = alone("Writer").await;
    assert_eq!(
        writer_in_runtime,
        json!(first[0]),
        "the check gives the version in the runtime"
    );
    // The unused stored interval: the same versions.
    let s = json!({ "interval_ms": 999, "runtime": "vm", "target": "pc", "run": true,
                    "inputs": [{ "variable": "level", "tag": LEVEL }] });
    ok(&send(
        &logic,
        &p,
        "PUT",
        "/programs/:program/settings",
        &[("program", "Writer")],
        Some(s),
    )
    .await);
    let b = built(&logic, &p, "Line").await;
    assert_eq!(versions(&b), first);
    assert_eq!(b["instance"]["programs"][0]["interval_ms"], 999);
    // Its task's priority, watchdog, kind: Writer's version, not Reader's.
    for change in [
        json!({ "priority": 2 }),
        json!({ "watchdog_ms": 500 }),
        json!({ "kind": { "type": "cyclic", "interval_ms": 30 } }),
        json!({ "kind": { "type": "cyclic", "interval_ms": 20, "phase_ms": 5 } }),
    ] {
        let mut l = line();
        for (k, v) in change.as_object().unwrap() {
            l["tasks"][0][k] = v.clone();
        }
        ok(&put(&logic, &p, "Line", l).await);
        let now = versions(&built(&logic, &p, "Line").await);
        assert_ne!(now[0], first[0], "{change}");
        assert_eq!(now[1], first[1], "{change}");
    }
    // Both in one task, in either order: each order is another task.
    let one = |order: [&str; 2]| {
        json!({ "module": "vm", "target": "pc", "run": true, "tasks": [
            { "name": "fast", "kind": { "type": "cyclic", "interval_ms": 20 }, "priority": 1, "programs": order }
        ]})
    };
    ok(&put(&logic, &p, "Line", one(["Writer", "Reader"])).await);
    let wr = built(&logic, &p, "Line").await;
    ok(&put(&logic, &p, "Line", one(["Reader", "Writer"])).await);
    let rw = built(&logic, &p, "Line").await;
    assert_eq!(names(&rw["programs"]), ["Reader", "Writer"]);
    assert_ne!(wr["programs"][0]["version"], rw["programs"][1]["version"]);
}

#[tokio::test]
async fn the_runtimes_version_ignores_run_and_retain_and_follows_its_tasks() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    ok(&put(&logic, &p, "Line", line()).await);
    let first = built(&logic, &p, "Line").await;
    let mut stopped = line();
    stopped["run"] = json!(false);
    ok(&put(&logic, &p, "Line", stopped).await);
    let b = built(&logic, &p, "Line").await;
    assert_eq!(b["instance"]["run"], false);
    assert_eq!(b["version"], first["version"], "run is not in the version");
    p.data(&[("S", true)]);
    let b = built(&logic, &p, "Line").await;
    assert_eq!(b["instance"]["data"][0]["retain"], true);
    assert_eq!(
        b["version"], first["version"],
        "retain is not in the version"
    );
    let mut slower = line();
    slower["tasks"][1]["kind"]["interval_ms"] = json!(60);
    ok(&put(&logic, &p, "Line", slower).await);
    let b = built(&logic, &p, "Line").await;
    assert_ne!(b["version"], first["version"]);
    // It is the version the core reports of the instance once loaded (`RuntimeStatus.version`,
    // its programs' versions joined by ", " in the instance's order), so a caller comparing the
    // two finds them equal: never a hash the core does not report.
    for b in [&first, &b] {
        let reported: Vec<&str> = b["instance"]["programs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["version"].as_str().unwrap())
            .collect();
        assert_eq!(b["version"], json!(reported.join(", ")), "{b}");
    }
    // And a check that is ok gives the build's version.
    ok(&put(&logic, &p, "Line", line()).await);
    let checked = check_of(&logic, &p, "Line", None).await;
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["version"], first["version"]);
}

#[tokio::test]
async fn a_program_in_a_configured_runtime_is_built_only_with_its_runtime() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    ok(&put(&logic, &p, "Line", line()).await);
    for name in ["Writer", "Reader"] {
        let a = send(
            &logic,
            &p,
            "POST",
            "/programs/:program/build",
            &[("program", name)],
            None,
        )
        .await;
        assert_eq!(a.status, 409, "{}", a.body);
        let task = if name == "Writer" { "fast" } else { "slow" };
        assert_eq!(
            a.body,
            json!({
                "error": format!("program '{name}' is in task '{task}' of runtime 'Line': build that runtime with build_logic_runtime"),
                "runtime": "Line",
            })
        );
    }
    // Out of the runtime, it builds alone again.
    ok(&delete(&logic, &p, "Line").await);
    ok(&send(
        &logic,
        &p,
        "POST",
        "/programs/:program/build",
        &[("program", "Writer")],
        None,
    )
    .await);
}

#[tokio::test]
async fn a_program_in_a_configured_runtime_is_checked_for_its_runtime() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    // Alone, its own target: rp2040, where it does not fit.
    written(&logic, &p, "Big", BIG, json!({ "target": "rp2040" })).await;
    let check = |name: &'static str| {
        let (logic, p) = (&logic, &p);
        async move {
            ok(&send(
                logic,
                p,
                "POST",
                "/programs/:program/check",
                &[("program", name)],
                None,
            )
            .await)
            .clone()
        }
    };
    let today = check("Writer").await;
    assert_eq!(check("Big").await["ok"], false);
    let mut l = line();
    l["tasks"][1]["programs"] = json!(["Reader", "Big"]);
    ok(&put(&logic, &p, "Line", l).await);
    let b = built(&logic, &p, "Line").await;
    for (i, name) in ["Writer", "Reader", "Big"].into_iter().enumerate() {
        let checked = check(name).await;
        assert_eq!(checked["ok"], true, "{name}: {checked}");
        assert_eq!(checked["version"], b["programs"][i]["version"], "{name}");
    }
    // Today's keys, today's answer but for the version.
    let mut writer = check("Writer").await;
    assert_eq!(sorted_keys(&writer), sorted_keys(&today));
    assert_ne!(writer["version"], today["version"]);
    writer["version"] = today["version"].clone();
    assert_eq!(writer, today);
    // A draft's text is checked the same way.
    let draft = send(
        &logic,
        &p,
        "POST",
        "/programs/:program/check",
        &[("program", "Writer")],
        Some(json!({ "source": WRITER })),
    )
    .await;
    assert_eq!(ok(&draft)["version"], b["programs"][0]["version"]);
}

#[tokio::test]
async fn building_a_program_running_alone_through_the_runtime_route_answers_the_programs_build() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    let tags = [LEVEL];
    let mut call: HttpCall = serde_json::from_value(json!({
        "method": "POST", "route": "/programs/:program/build", "params": { "program": "Reader" },
        "project": "plant", "data_dir": p.logic.display().to_string(),
    }))
    .unwrap();
    call.tags = Some(vec![LEVEL.to_string()]);
    let (program, _) = logic.serve(&call).await;
    assert_eq!(program.status, 200);
    for id in ["Reader", "reader"] {
        let runtime = build_of(&logic, &p, id, Some(&tags)).await;
        assert_eq!(runtime.status, 200);
        assert_eq!(
            serde_json::to_vec(&runtime.body).unwrap(),
            serde_json::to_vec(&program.body).unwrap(),
            "{id}"
        );
    }
    assert!(
        program.body["warnings"].as_array().unwrap().len() == 1,
        "{}",
        program.body
    );
    // Its check, as a runtime of its own.
    let checked = check_of(&logic, &p, "Reader", None).await;
    let alone = ok(&send(
        &logic,
        &p,
        "POST",
        "/programs/:program/check",
        &[("program", "Reader")],
        None,
    )
    .await)
    .clone();
    let mut expected = alone.clone();
    expected["name"] = json!("Reader");
    assert_eq!(
        checked,
        json!({ "ok": true, "runtime": "Reader", "version": alone["version"], "programs": [expected], "problems": [] })
    );
    refused(
        &build_of(&logic, &p, "Mixer", None).await,
        404,
        "the project has no runtime 'Mixer'",
    );
    let a = send(
        &logic,
        &p,
        "POST",
        "/runtimes/:runtime/check",
        &[("runtime", "Mixer")],
        None,
    )
    .await;
    refused(&a, 404, "the project has no runtime 'Mixer'");
}

#[tokio::test]
async fn a_runtime_build_names_what_it_replaces_and_warns_of_retained_values_not_taken() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    written(
        &logic,
        &p,
        "Keeper",
        "PROGRAM Keeper\nVAR RETAIN n : INT; END_VAR\n  n := n + 1;\nEND_PROGRAM",
        json!({}),
    )
    .await;
    let mut l = line();
    l["tasks"][0]["programs"] = json!(["Keeper", "Writer"]);
    ok(&put(&logic, &p, "Line", l).await);
    let a = build_of(&logic, &p, "Line", Some(&[LEVEL])).await;
    let b = ok(&a);
    assert_eq!(b["replaces"], json!(["Keeper", "Writer", "Reader"]));
    assert_eq!(
        b["warnings"],
        json!([
            "program 'Keeper': RETAIN values are kept per runtime id and per program: a warm load of runtime 'Line' gives a RETAIN variable of 'Keeper' only a value kept under runtime 'Line' for 'Keeper' (or, when the build that kept it did not name its project program, for a program of the same PROGRAM name), of the same name, type and size; values kept under runtime 'Line' for another program are not given, and none of those kept while 'Keeper' ran in another runtime, alone as runtime 'Keeper' included; a cold load starts every RETAIN variable from its initial value",
            format!("program 'Reader': output {SPEED} is not a tag of this controller now: writes to it will fail"),
        ])
    );
    // Nothing known of the tags: only the RETAIN warning.
    let b = built(&logic, &p, "Line").await;
    assert_eq!(b["warnings"].as_array().unwrap().len(), 1, "{b}");
}

#[tokio::test]
async fn a_check_of_a_runtime_gives_each_programs_diagnostics_and_the_runtimes_problems() {
    let (logic, p) = (Logic::default(), project());
    writer_and_reader(&logic, &p).await;
    written(
        &logic,
        &p,
        "Broken",
        "PROGRAM Broken\n  y := 1;\nEND_PROGRAM",
        json!({}),
    )
    .await;
    let before = p.files();
    // A draft, never saved.
    let mut draft = line();
    draft["tasks"][1]["programs"] = json!(["reader", "Broken"]);
    let checked = check_of(&logic, &p, "Line", Some(draft.clone())).await;
    assert_eq!(p.files(), before, "a check writes nothing");
    assert_eq!(checked["ok"], false);
    assert_eq!(checked["runtime"], "Line");
    assert_eq!(names(&checked["programs"]), ["Writer", "Reader", "Broken"]);
    let broken = &checked["programs"][2];
    assert_eq!(broken["ok"], false);
    assert_eq!(
        broken["diagnostics"][0]["message"], "y is not declared",
        "{broken}"
    );
    assert!(broken.get("version").is_none());
    for good in &checked["programs"].as_array().unwrap()[..2] {
        assert_eq!(good["ok"], true, "{good}");
        assert_eq!(good["binding_problems"], json!([]));
        assert!(good["version"].is_string());
    }
    assert_eq!(checked["problems"], json!([]));
    assert_eq!(checked["data"], json!([block_s(false)]));
    assert!(checked.get("version").is_none());
    // A draft that breaks the configuration's rules says so; still nothing written.
    let mut ghost = line();
    ghost["tasks"][1]["programs"] = json!(["Reader", "Ghost"]);
    let checked = check_of(&logic, &p, "Line", Some(ghost)).await;
    assert_eq!(
        checked["problems"],
        json!(["runtime 'Line': task 'slow' runs 'Ghost': the project has no program 'Ghost'"])
    );
    let checked = check_of(&logic, &p, "Writer", Some(line())).await;
    assert_eq!(
        checked["problems"][0],
        "'Writer' is a program of the project; a runtime is not named like a program"
    );
    // A draft body is a whole runtime.
    let a = send(
        &logic,
        &p,
        "POST",
        "/runtimes/:runtime/check",
        &[("runtime", "Line")],
        Some(json!({ "module": "vm" })),
    )
    .await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(p.files(), before);
    // The draft, saved and good, checks as the build builds.
    ok(&put(&logic, &p, "Line", line()).await);
    let stored = check_of(&logic, &p, "Line", None).await;
    let draft = check_of(&logic, &p, "Line", Some(line())).await;
    assert_eq!(stored, draft);
    assert_eq!(stored["ok"], true, "{stored}");
    let b = built(&logic, &p, "Line").await;
    assert_eq!(stored["version"], b["version"]);
    assert_eq!(
        stored["programs"][1]["version"],
        b["programs"][1]["version"]
    );
    // An empty body is the stored runtime.
    assert_eq!(check_of(&logic, &p, "Line", Some(json!({}))).await, stored);
}

#[tokio::test]
async fn a_task_configuration_that_cannot_be_read_refuses_the_program_check_and_build() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    std::fs::write(p.tasks_file(), "{ not json").unwrap();
    let before = p.files();
    for (route, params) in [
        ("/programs/:program/check", [("program", "A")]),
        ("/programs/:program/build", [("program", "A")]),
        ("/runtimes/:runtime/check", [("runtime", "A")]),
        ("/runtimes/:runtime/build", [("runtime", "A")]),
    ] {
        let a = send(&logic, &p, "POST", route, &params, None).await;
        assert_eq!(a.status, 500, "{route}: {}", a.body);
        assert!(
            a.body["error"]
                .as_str()
                .unwrap()
                .starts_with("logic/tasks.json cannot be read"),
            "{route}"
        );
    }
    assert_eq!(p.files(), before);
}

// -----------------------------------------------------------------------------
// Review findings of C8c, each a test of its own
// -----------------------------------------------------------------------------

/// A hand edit can store a runtime under an id that is not a runtime id ("Line 1"). Its
/// programs are then built only with it, and it cannot be built: the program's build says to
/// delete it, and the delete takes it, by that id. An id stored under none is still refused.
#[tokio::test]
async fn a_runtime_stored_under_an_id_that_is_not_one_can_be_deleted_by_it() {
    let (logic, p) = (Logic::default(), project());
    program(&logic, &p, "A", 20).await;
    let mut stored = one_task(&["A"]);
    stored["id"] = json!("Line 1");
    std::fs::write(p.tasks_file(), json!({ "runtimes": [stored] }).to_string()).unwrap();
    let words = "'Line 1' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'";
    assert_eq!(list(&logic, &p).await["problems"], json!([words]));
    let a = [("program", "A")];
    let built = send(&logic, &p, "POST", "/programs/:program/build", &a, None).await;
    assert_eq!(built.status, 409, "{}", built.body);
    assert_eq!(
        built.body,
        json!({
            "error": format!("program 'A' is in task 't' of runtime 'Line 1', which cannot be built: {words}; take it out with delete_logic_runtime"),
            "runtime": "Line 1",
        })
    );
    // Building it, or saving under that id, stays refused.
    refused(&build_of(&logic, &p, "Line 1", None).await, 400, words);
    refused(
        &put(&logic, &p, "Line 1", one_task(&["A"])).await,
        400,
        words,
    );
    // A 65-character id, never stored, is refused by the delete too.
    let long = "L".repeat(65);
    refused(
        &delete(&logic, &p, &long).await,
        400,
        &format!("'{long}' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'"),
    );
    let gone = delete(&logic, &p, "Line 1").await;
    assert_eq!(ok(&gone), &json!({ "deleted": "Line 1" }));
    let (name, data) = gone.event.unwrap();
    assert_eq!(name, RUNTIMES_CHANGED_EVENT);
    assert_eq!(
        data,
        json!({ "project": "plant", "runtime": "Line 1", "what": "deleted" })
    );
    assert_eq!(
        std::fs::read_to_string(p.tasks_file()).unwrap(),
        "{\n  \"runtimes\": []\n}\n"
    );
    // A is free again: built alone, and it can be put in a runtime.
    ok(&send(&logic, &p, "POST", "/programs/:program/build", &a, None).await);
    ok(&put(&logic, &p, "Line1", one_task(&["A"])).await);
}

/// The implied runtime listed for a program running alone is the one the core derives for the
/// instance its build answers ([`implied_task`] of that instance's program), whose program is
/// named by the PROGRAM its source declares, not by the program's name in the project.
#[tokio::test]
async fn the_implied_task_listed_is_the_one_the_core_derives_from_the_programs_build() {
    let (logic, p) = (Logic::default(), project());
    written(
        &logic,
        &p,
        "Tank",
        &counter("Other"),
        json!({ "interval_ms": 20 }),
    )
    .await;
    let a = [("program", "Tank")];
    let built = ok(&send(&logic, &p, "POST", "/programs/:program/build", &a, None).await).clone();
    let program: RuntimeProgram =
        serde_json::from_value(built["instance"]["program"].clone()).unwrap();
    assert_eq!(program.name, "Other");
    let derived = serde_json::to_value(implied_task(&program)).unwrap();
    let listed = list(&logic, &p).await;
    assert_eq!(listed["implied"][0]["id"], "Tank", "{listed}");
    assert_eq!(listed["implied"][0]["tasks"], json!([derived]), "{listed}");
    let one = ok(&get(&logic, &p, "Tank").await).clone();
    assert_eq!(one["runtime"], listed["implied"][0]);
    // A program whose saved source does not compile has no build to derive a task from: its
    // task is named by its name in the project, as the tool's summary says.
    written(
        &logic,
        &p,
        "Broken",
        "PROGRAM Elsewhere\n  x := ;\nEND_PROGRAM",
        json!({}),
    )
    .await;
    let listed = list(&logic, &p).await;
    assert_eq!(
        listed["implied"][1],
        implied_of("Broken", 20, "vm", "pc", true)
    );
}

/// A project with no module directory: a write route that is refused makes nothing, as before
/// the lock between processes. One that writes makes the directory, its file and the lock.
#[tokio::test]
async fn a_refused_write_in_a_project_without_the_modules_directory_makes_no_file() {
    let logic = Logic::default();
    let root = tempfile::tempdir().unwrap();
    let p = Project {
        logic: root.path().join("logic"),
        root,
    };
    let refusals = [
        (delete(&logic, &p, "R").await, 404),
        (
            send(
                &logic,
                &p,
                "PUT",
                "/programs/:program/source",
                &[("program", "Ghost")],
                Some(json!({ "source": "x" })),
            )
            .await,
            404,
        ),
        (put(&logic, &p, "R", json!({ "module": "vm" })).await, 400),
        (put(&logic, &p, "R", one_task(&["Ghost"])).await, 400),
        (
            send(
                &logic,
                &p,
                "POST",
                "/programs",
                &[],
                Some(json!({ "name": "9x" })),
            )
            .await,
            400,
        ),
        (
            send(
                &logic,
                &p,
                "DELETE",
                "/library/:name",
                &[("name", "Lib")],
                None,
            )
            .await,
            404,
        ),
    ];
    for (a, status) in &refusals {
        assert_eq!(a.status, *status, "{}", a.body);
        assert!(a.event.is_none(), "{}", a.body);
    }
    assert!(!p.logic.exists(), "a refused write made {:?}", p.files());
    // The answers are the ones a project with the directory gets, word for word.
    assert_eq!(
        refusals[0].0.body,
        json!({ "error": "the project has no runtime 'R'" })
    );
    assert_eq!(
        refusals[1].0.body,
        json!({ "error": "the project has no program 'Ghost'" })
    );
    // A write that is made: once, with its event, under the lock.
    let made = send(
        &logic,
        &p,
        "POST",
        "/programs",
        &[],
        Some(json!({ "name": "A" })),
    )
    .await;
    assert_eq!(made.status, 201, "{}", made.body);
    let (name, data) = made.event.unwrap();
    assert_eq!(name, PROGRAMS_CHANGED_EVENT);
    assert_eq!(data["what"], "created");
    assert!(p.logic.join(LOCK_FILE).is_file());
    let listed = ok(&send(&logic, &p, "GET", "/programs", &[], None).await).clone();
    assert_eq!(listed["programs"].as_array().unwrap().len(), 1, "{listed}");
}

/// What this module cannot know (which instances the core runs) its tools say to the caller:
/// `replaces` names only the ids its programs run under alone, so a configured runtime a program
/// left still runs it until it is taken out or rebuilt; a deleted runtime runs on until its
/// instance is taken out; and the core does not refuse one program in two runtimes.
#[test]
fn the_tools_say_that_a_runtime_a_program_left_still_runs_it_until_it_is_taken_out() {
    let manifest = crate::manifest();
    let summary = |tool: &str| {
        manifest
            .routes
            .iter()
            .find(|r| r.tool == tool)
            .unwrap()
            .summary
            .clone()
    };
    let build = summary("build_logic_runtime");
    for said in [
        "'replaces' does not name another configured runtime a program of this one was in before",
        "or the program runs in both",
        "its programs' versions joined by ', '",
        "a warm load gives a program only the values kept under this runtime's id",
    ] {
        assert!(build.contains(said), "{said}: {build}");
    }
    let delete = summary("delete_logic_runtime");
    assert!(
        delete.contains("its instance (the runtime's id) runs on, with all its programs, until it is taken out with save_runtimes_config"),
        "{delete}"
    );
    assert!(!delete.contains("its programs run alone again"), "{delete}");
    let alone = summary("build_logic_program");
    assert!(
        alone.contains("take out any instance of a configured runtime the program was in before"),
        "{alone}"
    );
    let save = summary("save_logic_runtime");
    assert!(
        save.contains("a program taken out of it included, until it is built again"),
        "{save}"
    );
    let list = summary("list_logic_runtimes");
    assert!(
        list.contains("named by the PROGRAM the saved source declares"),
        "{list}"
    );
}
