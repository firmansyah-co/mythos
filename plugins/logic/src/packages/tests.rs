//! The project's library packages (ADR-015 step 1, stage P1b: the P1 contract R10 to R20, R27,
//! R28), served by [`Logic`] as the server calls it. Every package file is built here in memory
//! with the zip writer; each project is a temporary directory.

use super::*;
use crate::{Event, Logic, LIBRARY_DIR, LOCK_FILE, PACKAGES_CHANGED_EVENT};
use mythos_logic_package::MAX_PACKAGE_BASE64_LEN;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

// -----------------------------------------------------------------------------
// Projects and calls
// -----------------------------------------------------------------------------

/// A project's directory as the core makes it; `logic` is the module's, made only by a write.
struct Project {
    root: tempfile::TempDir,
    logic: PathBuf,
}

/// A project whose module directory is not made yet.
fn bare() -> Project {
    let root = tempfile::tempdir().unwrap();
    let logic = root.path().join("logic");
    Project { root, logic }
}

/// A project with its module directory.
fn project() -> Project {
    let p = bare();
    std::fs::create_dir_all(&p.logic).unwrap();
    p
}

impl Project {
    /// Every file under the project, with its bytes; not the lock file, which every write
    /// route makes when it takes the lock and which holds nothing (see [`LOCK_FILE`]).
    fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let path = e.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if !path.ends_with(LOCK_FILE) {
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

    fn stored(&self, dir: &str, file: &str) -> PathBuf {
        self.logic.join("packages").join(dir).join(file)
    }

    fn list_file(&self) -> PathBuf {
        self.logic.join("packages.json")
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

fn encoded(file: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(file)
}

async fn add(logic: &Logic, p: &Project, file: &[u8], file_name: Option<&str>) -> Answer {
    let mut body = json!({ "file": encoded(file) });
    if let Some(name) = file_name {
        body["file_name"] = json!(name);
    }
    send(logic, p, "POST", "/packages", &[], Some(body)).await
}

async fn check_file(logic: &Logic, p: &Project, file: &[u8]) -> Answer {
    send(
        logic,
        p,
        "POST",
        "/packages/check",
        &[],
        Some(json!({ "file": encoded(file) })),
    )
    .await
}

async fn get_one(logic: &Logic, p: &Project, id: &str) -> Answer {
    send(
        logic,
        p,
        "GET",
        "/packages/:package",
        &[("package", id)],
        None,
    )
    .await
}

async fn remove_one(logic: &Logic, p: &Project, id: &str) -> Answer {
    send(
        logic,
        p,
        "DELETE",
        "/packages/:package",
        &[("package", id)],
        None,
    )
    .await
}

async fn listed(logic: &Logic, p: &Project) -> Value {
    let a = send(logic, p, "GET", "/packages", &[], None).await;
    assert_eq!(a.status, 200, "{}", a.body);
    a.body
}

fn added(a: &Answer) -> &Value {
    assert_eq!(a.status, 201, "{}", a.body);
    &a.body
}

fn words(a: &Answer) -> &str {
    a.body["error"]
        .as_str()
        .unwrap_or_else(|| panic!("no error: {}", a.body))
}

// -----------------------------------------------------------------------------
// Package files
// -----------------------------------------------------------------------------

const BASE: &str =
    "FUNCTION Twice : INT\nVAR_INPUT x : INT; END_VAR\nTwice := x * 2;\nEND_FUNCTION\n";
const MOTOR: &str = "FUNCTION_BLOCK Motor\nVAR_INPUT start : BOOL; END_VAR\nVAR_OUTPUT speed : INT; END_VAR\nIF start THEN speed := Twice(x := 5); ELSE speed := 0; END_IF;\nEND_FUNCTION_BLOCK\n";

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// A package file of this id, version, dependencies and sources (`(stem, text)`), its
/// checksums right; no optional manifest field.
fn package(id: &str, version: &str, depends: &[(&str, &str)], sources: &[(&str, &str)]) -> Vec<u8> {
    let files: Vec<String> = sources.iter().map(|(s, _)| format!("src/{s}.st")).collect();
    let manifest = json!({
        "format": 1, "id": id, "name": format!("{id} blocks"), "version": version,
        "form": "source", "languages": ["st"], "files": files,
        "depends": depends.iter().map(|(i, v)| json!({ "id": i, "version": v })).collect::<Vec<_>>(),
        "license": "MIT",
    });
    let mut entries: Vec<(String, Vec<u8>)> = vec![(
        "manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    )];
    for (stem, text) in sources {
        entries.push((format!("src/{stem}.st"), text.as_bytes().to_vec()));
    }
    let checksums: serde_json::Map<String, Value> = entries
        .iter()
        .map(|(name, bytes)| (name.clone(), json!(sha(bytes))))
        .collect();
    entries.push((
        "checksums.json".into(),
        serde_json::to_vec(&Value::Object(checksums)).unwrap(),
    ));
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in &entries {
        writer.start_file(name.as_str(), options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn base() -> Vec<u8> {
    package("org.example.base", "1.0.0", &[], &[("base", BASE)])
}

fn motors() -> Vec<u8> {
    package(
        "org.example.motors",
        "1.2.0",
        &[("org.example.base", "1.0.0")],
        &[("motor", MOTOR)],
    )
}

// -----------------------------------------------------------------------------
// Add, list, read, remove
// -----------------------------------------------------------------------------

#[tokio::test]
async fn a_package_is_added_listed_read_and_removed() {
    let logic = Logic::default();
    // No module directory yet: the add is made under the lock all the same.
    let p = bare();
    let file = base();
    let a = add(&logic, &p, &file, Some("base-1.0.0.zip")).await;
    let body = added(&a);
    let entry = json!({ "id": "org.example.base", "version": "1.0.0", "sha256": sha(&file),
        "origin": { "kind": "file", "file_name": "base-1.0.0.zip" } });
    assert_eq!(body["added"], entry);
    assert_eq!(body["versions_checked"], false);
    assert_eq!(
        body["blocks"],
        json!([{ "name": "Twice", "kind": "function", "inputs": [{ "name": "x", "type": "INT" }],
            "outputs": [], "result": "INT" }])
    );
    assert_eq!(
        a.event,
        Some((
            PACKAGES_CHANGED_EVENT,
            json!({ "project": "plant", "package": "org.example.base", "version": "1.0.0", "what": "added" })
        ))
    );
    // Kept whole, and listed.
    assert_eq!(
        std::fs::read(p.stored("org.example.base", "1.0.0.zip")).unwrap(),
        file
    );
    let list: Value = serde_json::from_slice(&std::fs::read(p.list_file()).unwrap()).unwrap();
    assert_eq!(list, json!({ "packages": [entry] }));

    // An origin's file name only when one was given.
    let motors_file = motors();
    let body = added(&add(&logic, &p, &motors_file, None).await).clone();
    assert_eq!(body["added"]["origin"], json!({ "kind": "file" }));
    assert_eq!(body["blocks"][0]["name"], "Motor");

    let all = listed(&logic, &p).await;
    let ids: Vec<&str> = all["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["org.example.base", "org.example.motors"]);
    for e in all["packages"].as_array().unwrap() {
        assert_eq!(e["license"], "MIT", "{e}");
        assert_eq!(e["format"], 1, "{e}");
        assert!(e.get("problem").is_none(), "{e}");
    }
    assert_eq!(all["packages"][1]["name"], "org.example.motors blocks");

    let got = get_one(&logic, &p, "org.example.motors").await;
    assert_eq!(got.status, 200, "{}", got.body);
    assert_eq!(got.body["entry"]["sha256"], sha(&motors_file));
    assert_eq!(
        got.body["manifest"]["depends"],
        json!([{ "id": "org.example.base", "version": "1.0.0" }])
    );
    // An optional field the manifest does not have is answered absent.
    assert!(
        got.body["manifest"].get("description").is_none(),
        "{}",
        got.body
    );
    assert_eq!(
        got.body["sources"],
        json!([{ "path": "src/motor.st", "source": MOTOR }])
    );
    assert_eq!(got.body["blocks"][0]["name"], "Motor");
    assert_eq!(got.body["dependents"], json!([]));
    assert!(got.event.is_none());
    let got = get_one(&logic, &p, "org.example.base").await;
    assert_eq!(got.body["dependents"], json!(["org.example.motors"]));

    let gone = remove_one(&logic, &p, "org.example.motors").await;
    assert_eq!(gone.status, 200, "{}", gone.body);
    assert_eq!(
        gone.body,
        json!({ "removed": { "id": "org.example.motors", "version": "1.2.0" } })
    );
    assert_eq!(
        gone.event,
        Some((
            PACKAGES_CHANGED_EVENT,
            json!({ "project": "plant", "package": "org.example.motors", "version": "1.2.0", "what": "removed" })
        ))
    );
    assert!(!p.logic.join("packages/org.example.motors").exists());
    let gone = remove_one(&logic, &p, "org.example.base").await;
    assert_eq!(gone.status, 200, "{}", gone.body);
    let list: Value = serde_json::from_slice(&std::fs::read(p.list_file()).unwrap()).unwrap();
    assert_eq!(list, json!({ "packages": [] }));
    assert!(!p.logic.join("packages/org.example.base").exists());
}

#[tokio::test]
async fn a_package_whose_id_differs_only_in_case_from_one_in_the_project_is_refused() {
    let logic = Logic::default();
    let p = project();
    added(&add(&logic, &p, &base(), None).await);
    let before = p.files();
    for id in ["Org.Example.BASE", "org.example.base"] {
        let other = package(
            id,
            "2.0.0",
            &[],
            &[("other", "FUNCTION Other : INT\nOther := 1;\nEND_FUNCTION\n")],
        );
        let a = add(&logic, &p, &other, None).await;
        assert_eq!(a.status, 409, "{}", a.body);
        assert_eq!(
            words(&a),
            "the project already has package 'org.example.base' 1.0.0; remove it first (an update is ADR-015 step 3)"
        );
        assert!(a.event.is_none());
        let checked = check_file(&logic, &p, &other).await;
        assert_eq!(checked.body["ok"], false);
        assert_eq!(
            checked.body["present"],
            json!({ "version": "1.0.0", "sha256": sha(&base()) })
        );
    }
    assert_eq!(p.files(), before);
}

#[tokio::test]
async fn a_package_whose_dependency_the_project_lacks_is_refused_naming_it() {
    let logic = Logic::default();
    let p = bare();
    let a = add(&logic, &p, &motors(), None).await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        words(&a),
        "org.example.motors 1.2.0 depends on org.example.base, which the project does not have: add it first"
    );
    assert_eq!(
        a.body["missing"],
        json!([{ "id": "org.example.base", "version": "1.0.0" }])
    );
    assert_eq!(a.body["versions_checked"], false);
    assert!(a.event.is_none());
    // Nothing written: not even the module's directory.
    assert!(!p.logic.exists());
}

#[tokio::test]
async fn a_package_declaring_a_block_the_projects_library_declares_is_refused_naming_both() {
    let logic = Logic::default();
    let p = project();
    std::fs::create_dir_all(p.logic.join(LIBRARY_DIR)).unwrap();
    // Compared without regard to case.
    std::fs::write(
        p.logic.join(LIBRARY_DIR).join("twice.st"),
        "FUNCTION TWICE : INT\nVAR_INPUT x : INT; END_VAR\nTWICE := x;\nEND_FUNCTION\n",
    )
    .unwrap();
    let before = p.files();
    let a = add(&logic, &p, &base(), None).await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        words(&a),
        "Twice is declared by org.example.base 1.0.0 in src/base.st and by the library source 'logic/library/twice.st'"
    );
    assert_eq!(
        a.body["clashes"],
        json!([{ "name": "Twice", "package_source": "src/base.st", "other": "the library source 'logic/library/twice.st'" }])
    );
    assert_eq!(p.files(), before);
}

#[tokio::test]
async fn a_package_declaring_a_block_another_package_declares_is_refused_naming_both() {
    let logic = Logic::default();
    let p = project();
    added(&add(&logic, &p, &base(), None).await);
    let before = p.files();
    let other = package(
        "org.example.other",
        "1.0.0",
        &[],
        &[(
            "other",
            "FUNCTION twice : INT\nVAR_INPUT x : INT; END_VAR\ntwice := x;\nEND_FUNCTION\n",
        )],
    );
    let a = add(&logic, &p, &other, None).await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        words(&a),
        "twice is declared by org.example.other 1.0.0 in src/other.st and by org.example.base 1.0.0 in src/base.st"
    );
    assert_eq!(p.files(), before);
}

#[tokio::test]
async fn a_package_declaring_a_block_named_like_a_user_type_is_refused_naming_both() {
    let logic = Logic::default();
    let p = project();
    let types = p.root.path().join("data").join("types");
    std::fs::create_dir_all(&types).unwrap();
    std::fs::write(
        types.join("Twice.st"),
        "TYPE Twice : STRUCT a : INT; END_STRUCT; END_TYPE\n",
    )
    .unwrap();
    let before = p.files();
    let a = add(&logic, &p, &base(), None).await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        words(&a),
        "Twice is declared by org.example.base 1.0.0 in src/base.st and by the type 'data/types/Twice.st'"
    );
    assert_eq!(p.files(), before);
}

#[tokio::test]
async fn a_type_in_a_package_is_refused() {
    let logic = Logic::default();
    let p = bare();
    let typed = package(
        "org.example.typed",
        "1.0.0",
        &[],
        &[("speed", "TYPE Speed : INT; END_TYPE\n")],
    );
    let a = add(&logic, &p, &typed, None).await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        words(&a),
        "Speed is a TYPE in org.example.typed's src/speed.st; types in a package come with ADR-015 step 2"
    );
    let program = package(
        "org.example.program",
        "1.0.0",
        &[],
        &[("main", "PROGRAM Main\nEND_PROGRAM\n")],
    );
    let a = add(&logic, &p, &program, None).await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(
        words(&a),
        "Main is a PROGRAM in org.example.program's src/main.st; a package holds functions and function blocks"
    );
    assert!(!p.logic.exists());
}

#[tokio::test]
async fn a_package_that_uses_a_block_of_a_package_it_does_not_depend_on_is_refused() {
    let logic = Logic::default();
    let p = project();
    added(&add(&logic, &p, &base(), None).await);
    // A block of the project's library is not taken either: nothing else of the project.
    std::fs::create_dir_all(p.logic.join(LIBRARY_DIR)).unwrap();
    std::fs::write(
        p.logic.join(LIBRARY_DIR).join("helper.st"),
        "FUNCTION Helper : INT\nHelper := 1;\nEND_FUNCTION\n",
    )
    .unwrap();
    let before = p.files();
    for (name, uses) in [("loose", "Twice(x := 5)"), ("helped", "Helper()")] {
        let text = format!(
            "FUNCTION_BLOCK Loose\nVAR_OUTPUT speed : INT; END_VAR\nspeed := {uses};\nEND_FUNCTION_BLOCK\n"
        );
        let file = package(
            &format!("org.example.{name}"),
            "1.0.0",
            &[],
            &[("loose", &text)],
        );
        let a = add(&logic, &p, &file, None).await;
        assert_eq!(a.status, 400, "{}", a.body);
        assert_eq!(
            words(&a),
            format!("org.example.{name} 1.0.0 does not check")
        );
        assert!(
            !a.body["diagnostics"].as_array().unwrap().is_empty(),
            "{}",
            a.body
        );
        assert_eq!(a.body["versions_checked"], false);
    }
    assert_eq!(p.files(), before);
    // The same use with the dependency declared is added.
    added(&add(&logic, &p, &motors(), None).await);
}

#[tokio::test]
async fn removing_a_package_another_depends_on_is_refused_naming_it() {
    let logic = Logic::default();
    let p = project();
    added(&add(&logic, &p, &base(), None).await);
    added(&add(&logic, &p, &motors(), None).await);
    let before = p.files();
    for id in ["org.example.base", "ORG.EXAMPLE.BASE"] {
        let a = remove_one(&logic, &p, id).await;
        assert_eq!(a.status, 409, "{}", a.body);
        assert_eq!(
            words(&a),
            "package 'org.example.base' is a dependency of org.example.motors: remove those first"
        );
        assert_eq!(a.body["dependents"], json!(["org.example.motors"]));
        assert!(a.event.is_none());
    }
    assert_eq!(p.files(), before);
}

#[tokio::test]
async fn an_absent_package_is_404_and_a_parameter_that_is_not_an_id_is_400_in_the_readers_words() {
    let logic = Logic::default();
    let p = project();
    let a = get_one(&logic, &p, "org.example.base").await;
    assert_eq!(a.status, 404, "{}", a.body);
    assert_eq!(words(&a), "the project has no package 'org.example.base'");
    let a = remove_one(&logic, &p, "org.example.base").await;
    assert_eq!(a.status, 404, "{}", a.body);
    assert!(a.event.is_none());
    for bad in ["motors", "org.CON", "org..x", "org.example.motörs"] {
        for a in [
            get_one(&logic, &p, bad).await,
            remove_one(&logic, &p, bad).await,
        ] {
            assert_eq!(a.status, 400, "{bad}: {}", a.body);
            assert_eq!(words(&a), mythos_logic_package::id_problem(bad).unwrap());
        }
    }
    assert!(!p.list_file().exists());
}

// -----------------------------------------------------------------------------
// The file as given
// -----------------------------------------------------------------------------

#[tokio::test]
async fn a_file_over_the_base64_bound_is_refused_before_it_is_decoded() {
    let logic = Logic::default();
    let p = bare();
    // Not base64 at all: a decode first would say so; the bound is said instead.
    let over = "!".repeat(MAX_PACKAGE_BASE64_LEN + 1);
    for route in ["/packages", "/packages/check"] {
        let a = send(
            &logic,
            &p,
            "POST",
            route,
            &[],
            Some(json!({ "file": over })),
        )
        .await;
        assert_eq!(a.status, 400, "{}", a.body);
        assert_eq!(
            words(&a),
            format!(
                "the file is {} characters of base64; at most {MAX_PACKAGE_BASE64_LEN} are taken, the base64 of {} bytes",
                MAX_PACKAGE_BASE64_LEN + 1,
                mythos_logic_package::MAX_PACKAGE_BYTES
            )
        );
        // At the bound it is decoded, and refused as what it is.
        let at = "!".repeat(MAX_PACKAGE_BASE64_LEN);
        let a = send(&logic, &p, "POST", route, &[], Some(json!({ "file": at }))).await;
        assert_eq!(a.status, 400, "{}", a.body);
        assert_eq!(
            words(&a),
            "the file is not base64 of the standard alphabet with padding"
        );
    }
    assert!(!p.logic.exists());
}

#[tokio::test]
async fn a_file_that_is_not_a_package_is_refused_in_the_readers_words_and_nothing_written() {
    let logic = Logic::default();
    let p = bare();
    let not_a_zip = b"not a zip file at all";
    let said = mythos_logic_package::read(not_a_zip).unwrap_err();
    let a = add(&logic, &p, not_a_zip, Some("x.zip")).await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(words(&a), said);
    let a = check_file(&logic, &p, not_a_zip).await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert_eq!(words(&a), said);
    // The file name: as given, within its bound, no control character.
    let long = "a".repeat(MAX_FILE_NAME_LEN + 1);
    for (name, said) in [
        (
            long.as_str(),
            format!(
                "file_name is at most {MAX_FILE_NAME_LEN} characters; this one is {}",
                MAX_FILE_NAME_LEN + 1
            ),
        ),
        (
            "a\nb.zip",
            "file_name holds a control character".to_string(),
        ),
        (
            "",
            "file_name is empty: leave it out when there is none".to_string(),
        ),
    ] {
        let a = add(&logic, &p, &base(), Some(name)).await;
        assert_eq!(a.status, 400, "{}", a.body);
        assert_eq!(words(&a), said);
    }
    let a = send(
        &logic,
        &p,
        "POST",
        "/packages",
        &[],
        Some(json!({ "file": encoded(&base()), "url": "x" })),
    )
    .await;
    assert_eq!(a.status, 400, "{}", a.body);
    assert!(!p.logic.exists());
}

// -----------------------------------------------------------------------------
// The check
// -----------------------------------------------------------------------------

#[tokio::test]
async fn the_check_writes_nothing_takes_no_lock_and_its_ok_is_true_exactly_when_the_add_would_succeed(
) {
    let logic = Logic::default();
    // In a project with no module directory, the check makes none (and so no lock file).
    let p = bare();
    let a = check_file(&logic, &p, &base()).await;
    assert_eq!(a.status, 200, "{}", a.body);
    assert_eq!(a.body["ok"], true, "{}", a.body);
    assert_eq!(a.body["sha256"], sha(&base()));
    assert_eq!(a.body["size"], base().len());
    assert_eq!(a.body["package"]["id"], "org.example.base");
    assert_eq!(a.body["blocks"][0]["name"], "Twice");
    assert_eq!(a.body["present"], Value::Null);
    assert!(a.event.is_none());
    assert!(!p.logic.exists());

    // Each case: the check's ok, then the add's answer, on the same project.
    let p = project();
    let loose = package(
        "org.example.loose",
        "1.0.0",
        &[],
        &[("loose", "FUNCTION_BLOCK Loose\nVAR_OUTPUT s : INT; END_VAR\ns := Twice(x := 1);\nEND_FUNCTION_BLOCK\n")],
    );
    let clash = package("org.example.clash", "1.0.0", &[], &[("c", BASE)]);
    let typed = package(
        "org.example.typed",
        "1.0.0",
        &[],
        &[("t", "TYPE T : INT; END_TYPE\n")],
    );
    let cases: Vec<(&str, Vec<u8>, bool)> = vec![
        ("missing", motors(), false),
        ("fine", base(), true),
        ("present", base(), false),
        ("does not check", loose, false),
        ("clash", clash, false),
        ("type", typed, false),
        ("fine with its dependency", motors(), true),
    ];
    for (case, file, adds) in cases {
        let before = p.files();
        let checked = check_file(&logic, &p, &file).await;
        assert_eq!(checked.status, 200, "{case}: {}", checked.body);
        assert!(checked.event.is_none(), "{case}");
        assert_eq!(p.files(), before, "{case}: the check wrote");
        assert!(
            !p.logic.join(LOCK_FILE).exists() || case != "missing",
            "{case}"
        );
        let a = add(&logic, &p, &file, None).await;
        assert_eq!(
            checked.body["ok"],
            json!(a.status == 201),
            "{case}: {} / {}",
            checked.body,
            a.body
        );
        assert_eq!(a.status == 201, adds, "{case}: {}", a.body);
        if !adds {
            assert_eq!(checked.body["refusal"]["error"], a.body["error"], "{case}");
            assert_eq!(checked.body["refusal"]["status"], a.status, "{case}");
        }
    }
}

// -----------------------------------------------------------------------------
// What is stored, read again
// -----------------------------------------------------------------------------

#[tokio::test]
async fn a_listed_package_whose_file_is_missing_is_answered_with_a_problem_and_no_manifest_fields()
{
    let logic = Logic::default();
    let p = project();
    added(&add(&logic, &p, &base(), None).await);
    std::fs::remove_file(p.stored("org.example.base", "1.0.0.zip")).unwrap();
    let all = listed(&logic, &p).await;
    let e = &all["packages"][0];
    assert_eq!(
        e["problem"],
        "logic/packages/org.example.base/1.0.0.zip is missing: logic/packages.json lists the package and its file is not there"
    );
    for key in ["name", "license", "format"] {
        assert!(e.get(key).is_none(), "{key}: {e}");
    }
    let got = get_one(&logic, &p, "org.example.base").await;
    assert_eq!(got.status, 200, "{}", got.body);
    assert_eq!(got.body.as_object().unwrap().len(), 2, "{}", got.body);
    assert_eq!(got.body["problem"], e["problem"]);
    // It can be removed.
    let gone = remove_one(&logic, &p, "org.example.base").await;
    assert_eq!(gone.status, 200, "{}", gone.body);
    assert_eq!(listed(&logic, &p).await, json!({ "packages": [] }));
}

#[tokio::test]
async fn a_listed_package_whose_file_was_changed_is_answered_with_a_problem_and_can_be_removed() {
    let logic = Logic::default();
    let p = project();
    added(&add(&logic, &p, &base(), None).await);
    let stored = p.stored("org.example.base", "1.0.0.zip");
    // Another valid package of the same id and version: it reads, and is not the one added.
    let changed = package(
        "org.example.base",
        "1.0.0",
        &[],
        &[(
            "base",
            "FUNCTION Twice : INT\nVAR_INPUT x : INT; END_VAR\nTwice := x * 3;\nEND_FUNCTION\n",
        )],
    );
    for (file, said) in [
        (changed, "logic/packages/org.example.base/1.0.0.zip does not have the SHA-256 logic/packages.json records; it was changed after it was added".to_string()),
        (b"garbage".to_vec(), format!(
            "logic/packages/org.example.base/1.0.0.zip no longer reads as a package: {}",
            mythos_logic_package::read(b"garbage").unwrap_err()
        )),
    ] {
        std::fs::write(&stored, &file).unwrap();
        let all = listed(&logic, &p).await;
        assert_eq!(all["packages"][0]["problem"], json!(said));
        assert!(all["packages"][0].get("name").is_none());
        let got = get_one(&logic, &p, "org.example.base").await;
        assert_eq!(got.body["problem"], json!(said));
        assert!(got.body.get("manifest").is_none() && got.body.get("sources").is_none());
    }
    // A package added after it cannot be checked against what it declares: said, not guessed.
    let other = package(
        "org.example.other",
        "1.0.0",
        &[],
        &[("o", "FUNCTION O : INT\nO := 1;\nEND_FUNCTION\n")],
    );
    let a = add(&logic, &p, &other, None).await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert!(
        words(&a).contains("no longer reads as a package"),
        "{}",
        a.body
    );
    let gone = remove_one(&logic, &p, "org.example.base").await;
    assert_eq!(gone.status, 200, "{}", gone.body);
    assert!(!stored.exists());
}

#[tokio::test]
async fn a_stored_file_packages_json_does_not_list_is_not_a_package_and_an_add_writes_over_it() {
    let logic = Logic::default();
    let p = project();
    let stored = p.stored("org.example.base", "1.0.0.zip");
    std::fs::create_dir_all(stored.parent().unwrap()).unwrap();
    std::fs::write(&stored, base()).unwrap();
    assert_eq!(listed(&logic, &p).await, json!({ "packages": [] }));
    assert_eq!(get_one(&logic, &p, "org.example.base").await.status, 404);
    // A list made by no read.
    assert!(!p.list_file().exists());
    std::fs::write(&stored, b"left by a stop between two writes").unwrap();
    added(&add(&logic, &p, &base(), None).await);
    assert_eq!(std::fs::read(&stored).unwrap(), base());
}

#[tokio::test]
async fn an_unreadable_packages_json_is_a_500_naming_it_and_is_not_written_over() {
    let logic = Logic::default();
    let p = project();
    let entry = |id: &str, version: &str| json!({ "id": id, "version": version, "sha256": sha(b"x"), "origin": { "kind": "file" } });
    for (text, said) in [
        ("{ not json".to_string(), "key must be a string"),
        (json!({ "packages": [], "more": 1 }).to_string(), "unknown field `more`"),
        (json!({ "packages": [entry("org.example.base", "../../x")] }).to_string(), "is not a package version"),
        (json!({ "packages": [entry("org..base", "1.0.0")] }).to_string(), "is not a package id"),
        (json!({ "packages": [entry("org.example.a", "1.0.0"), entry("ORG.example.A", "1.0.0")] }).to_string(), "one id"),
        (json!({ "packages": [{ "id": "org.example.a", "version": "1.0.0", "sha256": "AB", "origin": { "kind": "file" } }] }).to_string(), "64 lowercase"),
        (json!({ "packages": [{ "id": "org.example.a", "version": "1.0.0", "sha256": sha(b"x"), "origin": { "kind": "url" } }] }).to_string(), "unknown variant"),
    ] {
        std::fs::write(p.list_file(), &text).unwrap();
        let a = send(&logic, &p, "GET", "/packages", &[], None).await;
        assert_eq!(a.status, 500, "{}", a.body);
        let w = words(&a);
        assert!(w.starts_with("logic/packages.json cannot be read as the project's packages: "), "{w}");
        assert!(w.contains(said), "{said}: {w}");
        let a = add(&logic, &p, &base(), None).await;
        assert_eq!(a.status, 500, "{}", a.body);
        assert_eq!(std::fs::read_to_string(p.list_file()).unwrap(), text);
        assert!(!p.logic.join("packages").exists());
    }
}

#[tokio::test]
async fn the_add_writes_the_file_before_the_list() {
    let logic = Logic::default();
    let p = project();
    // The file cannot be written: a directory that holds a file is where it goes.
    let stored = p.stored("org.example.base", "1.0.0.zip");
    std::fs::create_dir_all(&stored).unwrap();
    std::fs::write(stored.join("in-the-way"), b"").unwrap();
    let a = add(&logic, &p, &base(), None).await;
    assert_eq!(a.status, 500, "{}", a.body);
    assert!(a.event.is_none());
    // So the list was never written: no package without its file.
    assert!(!p.list_file().exists());
}

#[tokio::test]
async fn the_remove_writes_the_list_before_it_removes_the_file() {
    let logic = Logic::default();
    let p = project();
    added(&add(&logic, &p, &base(), None).await);
    let stored = p.stored("org.example.base", "1.0.0.zip");
    // The list cannot be written: its temporary's name is a directory that holds a file.
    let tmp = p
        .logic
        .join(format!(".packages.json.{}.tmp", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("in-the-way"), b"").unwrap();
    let a = remove_one(&logic, &p, "org.example.base").await;
    assert_eq!(a.status, 500, "{}", a.body);
    assert!(a.event.is_none());
    // So the file was not removed: the package is still listed and still reads.
    assert_eq!(std::fs::read(&stored).unwrap(), base());
    let all = listed(&logic, &p).await;
    assert_eq!(all["packages"][0]["id"], "org.example.base");
    assert!(all["packages"][0].get("problem").is_none(), "{all}");
    std::fs::remove_dir_all(&tmp).unwrap();
    assert_eq!(remove_one(&logic, &p, "org.example.base").await.status, 200);
    assert!(!stored.exists());
}

#[tokio::test]
async fn ids_that_differ_only_in_case_are_one_id_and_one_directory() {
    let logic = Logic::default();
    let p = project();
    let file = package("Org.Example.Base", "1.0.0-RC1", &[], &[("base", BASE)]);
    let body = added(&add(&logic, &p, &file, None).await).clone();
    // The id answered is the manifest's own spelling; the stored path is lower-cased.
    assert_eq!(body["added"]["id"], "Org.Example.Base");
    assert_eq!(
        std::fs::read(p.stored("org.example.base", "1.0.0-rc1.zip")).unwrap(),
        file
    );
    let got = get_one(&logic, &p, "ORG.EXAMPLE.BASE").await;
    assert_eq!(got.status, 200, "{}", got.body);
    assert_eq!(got.body["entry"]["id"], "Org.Example.Base");
    assert_eq!(got.body["manifest"]["version"], "1.0.0-RC1");
    let gone = remove_one(&logic, &p, "org.example.base").await;
    assert_eq!(gone.status, 200, "{}", gone.body);
    assert_eq!(gone.body["removed"]["id"], "Org.Example.Base");
    assert!(!p.logic.join("packages/org.example.base").exists());
}

#[tokio::test]
async fn an_add_past_the_source_bound_is_refused_stating_it() {
    let logic = Logic::default();
    let p = project();
    let library = p.logic.join(LIBRARY_DIR);
    std::fs::create_dir_all(&library).unwrap();
    for k in 0..MAX_SOURCES {
        std::fs::write(library.join(format!("L{k}.st")), b"").unwrap();
    }
    let before = p.files();
    let a = add(&logic, &p, &base(), None).await;
    assert_eq!(a.status, 409, "{}", a.body);
    assert_eq!(
        words(&a),
        format!(
            "with org.example.base 1.0.0 the project would have {} library sources; at most {MAX_SOURCES} are taken",
            MAX_SOURCES + 1
        )
    );
    assert_eq!(p.files(), before);
}

#[test]
fn the_manifest_declares_the_five_package_routes_each_a_tool_that_says_programs_are_not_compiled_with_it(
) {
    let manifest = crate::manifest();
    let routes: Vec<String> = manifest
        .routes
        .iter()
        .filter(|r| r.path.starts_with("/packages"))
        .map(|r| {
            format!(
                "{} {} {} {}",
                r.method,
                r.path,
                r.tool,
                r.role.as_deref().unwrap_or("-")
            )
        })
        .collect();
    assert_eq!(
        routes,
        [
            "GET /packages list_logic_packages viewer",
            "GET /packages/:package get_logic_package viewer",
            "POST /packages/check check_logic_package viewer",
            "POST /packages add_logic_package engineer",
            "DELETE /packages/:package remove_logic_package engineer",
        ]
    );
    for r in manifest
        .routes
        .iter()
        .filter(|r| r.path.starts_with("/packages"))
    {
        assert!(
            r.summary
                .contains("A package is kept and listed; programs are not compiled with it yet."),
            "{}: {}",
            r.tool,
            r.summary
        );
        if r.method == "POST" {
            for said in [
                format!("at most {MAX_PACKAGE_BASE64_LEN} characters"),
                format!("at most {} bytes", mythos_logic_package::MAX_PACKAGE_BYTES),
                "base64".to_string(),
            ] {
                assert!(r.summary.contains(&said), "{}: {said}", r.tool);
            }
        }
    }
}
