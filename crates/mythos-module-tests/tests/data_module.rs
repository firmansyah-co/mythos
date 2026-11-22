//! The data module (ADR-014 §2.8) as the server runs it: a real module process behind the
//! project routes. An engineer makes a project's user types and data blocks, a viewer reads
//! them, an agent does both over MCP, and a new build of the module takes over the files.
//!
//! Nothing here builds or loads a program, and no runtime runs: the module has neither.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use mythos_core::api::{create_router, ApiState};
use mythos_core::auth::{Auth, Role};
use mythos_core::services::ServiceHost;
use mythos_core::storage::{ControllerStorage, ProjectConfig};
use mythos_core::tag::TagCatalog;
use mythos_module::host::{read_manifest, Fingerprint};
use mythos_module::ModuleBinary;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::util::ServiceExt;

const DATA: &str = env!("CARGO_BIN_EXE_data-module-under-test");
const TYPES: &str = "/api/v1/projects/plant/data/types";
const BLOCKS: &str = "/api/v1/projects/plant/data/blocks";

const RECIPE: &str = "TYPE RecipeData : STRUCT
  speed : INT;
  temp : REAL;
END_STRUCT; END_TYPE
";

async fn module_at(path: &Path) -> Arc<ModuleBinary> {
    Arc::new(ModuleBinary {
        path: path.to_path_buf(),
        manifest: read_manifest(path)
            .await
            .expect("the data module prints its manifest"),
        fingerprint: Fingerprint::of(path).unwrap(),
    })
}

struct Rig {
    app: Router,
    services: Arc<ServiceHost>,
    engineer: String,
    operator: String,
    viewer: String,
    /// The storage root: every project is under it.
    root: PathBuf,
    project: PathBuf,
    _dir: tempfile::TempDir,
}

async fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("projects");
    let storage = Arc::new(ControllerStorage::new(&root).unwrap());
    for id in ["plant", "other"] {
        storage.create_project(&ProjectConfig::new(id, id)).unwrap();
    }
    let project = storage.project_dir("plant").unwrap();

    let auth = Arc::new(Auth::open(&storage.system_dir()).unwrap());
    let code = auth.setup_code().unwrap().to_string();
    auth.set_up(&code, "root", "", "root password 1").unwrap();
    for (name, role) in [
        ("erin", Role::Engineer),
        ("otto", Role::Operator),
        ("vera", Role::Viewer),
    ] {
        auth.create_user(name, "", role, "a good password").unwrap();
    }
    let session = |name: &str| auth.sign_in(name, "a good password").unwrap().1;
    let (engineer, operator, viewer) = (session("erin"), session("otto"), session("vera"));

    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    )
    .with_auth(auth);
    let services = ServiceHost::new(state.hmi_events.clone());
    services.sync(vec![module_at(Path::new(DATA)).await]).await;
    Rig {
        app: create_router(state.with_services(Arc::clone(&services))),
        services,
        engineer,
        operator,
        viewer,
        root,
        project,
        _dir: dir,
    }
}

impl Rig {
    async fn send_raw(
        &self,
        session: &str,
        method: &str,
        uri: &str,
        body: Option<String>,
    ) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("mythos_session={session}"));
        let request = match body {
            Some(b) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b)),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 22)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn send(
        &self,
        session: &str,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.send_raw(session, method, uri, body.map(|b| b.to_string()))
            .await
    }

    /// As the engineer, expecting `expected`.
    async fn eng(&self, method: &str, uri: &str, body: Option<Value>, expected: u16) -> Value {
        let (status, answer) = self.send(&self.engineer, method, uri, body).await;
        assert_eq!(status.as_u16(), expected, "{method} {uri}: {answer}");
        answer
    }

    async fn close(self) {
        self.services.shutdown().await;
    }
}

/// Every file and directory under `root`, relative to it.
fn listing(root: &Path) -> BTreeSet<PathBuf> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries {
            let path = e.unwrap().path();
            out.insert(path.strip_prefix(root).unwrap().to_path_buf());
            if path.is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

fn keys(v: &Value) -> BTreeSet<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn types_and_blocks_are_made_listed_and_deleted_through_the_server() {
    let rig = rig().await;
    let (status, listed) = rig.send(&rig.viewer, "GET", TYPES, None).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed, json!({ "types": [] }));

    // An operator and a viewer read, and write nothing.
    let recipe = format!("{TYPES}/RecipeData");
    for session in [&rig.operator, &rig.viewer] {
        let (status, said) = rig
            .send(session, "PUT", &recipe, Some(json!({ "source": RECIPE })))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{said}");
        let (status, said) = rig
            .send(
                session,
                "PUT",
                &format!("{BLOCKS}/Recipe"),
                Some(json!({ "type": "RecipeData", "retain": false })),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{said}");
    }
    assert!(!rig.project.join("data").join("types").exists());

    let saved = rig
        .eng("PUT", &recipe, Some(json!({ "source": RECIPE })), 200)
        .await;
    assert_eq!(
        saved,
        json!({ "name": "RecipeData", "created": true, "blocks": [], "changed": [] })
    );
    assert_eq!(
        std::fs::read_to_string(rig.project.join("data").join("types").join("RecipeData.st"))
            .unwrap(),
        RECIPE
    );
    let (status, read) = rig.send(&rig.viewer, "GET", &recipe, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read, json!({ "name": "RecipeData", "source": RECIPE }));

    let made = rig
        .eng(
            "PUT",
            &format!("{BLOCKS}/Recipe"),
            Some(json!({ "type": "RecipeData", "retain": true })),
            200,
        )
        .await;
    assert_eq!(
        keys(&made),
        BTreeSet::from(["created", "blocks", "changed"])
    );
    assert_eq!(made["created"], true);
    assert_eq!(made["changed"], json!(["Recipe"]));
    let block = &made["blocks"][0];
    assert_eq!(
        keys(block),
        BTreeSet::from(["name", "type", "retain", "layout", "size", "members"])
    );
    assert_eq!(block["retain"], true);

    let (status, blocks) = rig.send(&rig.viewer, "GET", BLOCKS, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        blocks,
        json!({ "ok": true, "diagnostics": [], "blocks": made["blocks"] })
    );
    let (status, checked) = rig
        .send(
            &rig.viewer,
            "POST",
            "/api/v1/projects/plant/data/check",
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{checked}");
    assert_eq!(checked, blocks);

    // Refusals come back with the module's status and words.
    let refused = rig.eng("DELETE", &recipe, None, 409).await;
    assert_eq!(
        refused,
        json!({ "error": "type 'RecipeData' is the type of data block 'Recipe': delete those blocks or give them another type first" })
    );
    let refused = rig
        .eng(
            "PUT",
            &format!("{BLOCKS}/Other"),
            Some(json!({ "type": "Nope", "retain": false })),
            400,
        )
        .await;
    assert_eq!(
        refused["error"],
        "data block 'Other' does not check and was not saved"
    );
    assert_eq!(
        refused["diagnostics"][0]["message"],
        "data block 'Other': there is no type Nope"
    );

    // A type source the project does not check with is refused too, and the stored one stays.
    let refused = rig
        .eng(
            "PUT",
            &recipe,
            Some(json!({ "source": RECIPE.replace("REAL", "Nope") })),
            400,
        )
        .await;
    assert_eq!(
        refused["error"],
        "with this source the project's types and data blocks do not check, and nothing was saved"
    );
    assert_eq!(
        std::fs::read_to_string(rig.project.join("data").join("types").join("RecipeData.st"))
            .unwrap(),
        RECIPE
    );

    // The other project has none of it.
    let (status, other) = rig
        .send(
            &rig.viewer,
            "GET",
            "/api/v1/projects/other/data/blocks",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        other,
        json!({ "ok": true, "diagnostics": [], "blocks": [] })
    );

    let deleted = rig
        .eng("DELETE", &format!("{BLOCKS}/recipe"), None, 200)
        .await;
    assert_eq!(deleted, json!({ "deleted": "Recipe", "blocks": [] }));
    let deleted = rig.eng("DELETE", &recipe, None, 200).await;
    assert_eq!(deleted, json!({ "deleted": "RecipeData", "blocks": [] }));
    let (_, listed) = rig.send(&rig.viewer, "GET", TYPES, None).await;
    assert_eq!(listed, json!({ "types": [] }));
    assert_eq!(
        std::fs::read_to_string(rig.project.join("data").join("blocks.json")).unwrap(),
        "{\n  \"blocks\": []\n}\n"
    );
    rig.close().await;
}

/// Every route is an MCP tool (ADR-010): an agent makes a type and a block with them.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_makes_a_type_and_a_block_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    let project = storage.project_dir("plant").unwrap();
    let state = ApiState::<mythos_pal::MemoryShm>::new(
        storage,
        Arc::new(RwLock::new(TagCatalog::new())),
        None,
    );
    let services = ServiceHost::new(state.hmi_events.clone());
    services.sync(vec![module_at(Path::new(DATA)).await]).await;
    let app = create_router(state.with_services(Arc::clone(&services)));

    let rpc = |method: &'static str, params: Value| {
        let app = app.clone();
        async move {
            // The Host header is what the MCP endpoint checks; nothing listens on a port here.
            let request = Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(header::HOST, "127.0.0.1")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ACCEPT, "application/json, text/event-stream")
                .body(Body::from(
                    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
                        .to_string(),
                ))
                .unwrap();
            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        }
    };
    let tool = |name: &'static str, arguments: Value| {
        let call = rpc("tools/call", json!({"name": name, "arguments": arguments}));
        async move {
            let answer = call.await;
            let result = &answer["result"];
            assert!(result.is_object(), "no result: {answer}");
            (
                result["isError"].as_bool().unwrap(),
                result["structuredContent"]["body"].clone(),
            )
        }
    };

    const EIGHT: [&str; 8] = [
        "list_data_types",
        "get_data_type",
        "save_data_type",
        "delete_data_type",
        "list_data_blocks",
        "save_data_block",
        "delete_data_block",
        "check_data",
    ];
    let listed = rpc("tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .filter(|n| EIGHT.contains(n))
        .collect();
    assert_eq!(names, EIGHT);
    // A path parameter `:type` is the argument `type_id`; `:block` is `block`.
    let schema =
        |name: &str| tools.iter().find(|t| t["name"] == name).unwrap()["inputSchema"].clone();
    let required = |name: &str| -> BTreeSet<String> {
        schema(name)["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        required("save_data_type"),
        BTreeSet::from(["project_id".into(), "type_id".into(), "body".into()])
    );
    assert_eq!(
        required("save_data_block"),
        BTreeSet::from(["project_id".into(), "block".into(), "body".into()])
    );

    let (error, saved) = tool(
        "save_data_type",
        json!({"project_id": "plant", "type_id": "RecipeData", "body": {"source": RECIPE}}),
    )
    .await;
    assert!(!error, "{saved}");
    assert_eq!(saved["created"], true, "{saved}");
    let (error, made) = tool(
        "save_data_block",
        json!({"project_id": "plant", "block": "Recipe", "body": {"type": "RecipeData", "retain": false}}),
    )
    .await;
    assert!(!error, "{made}");
    assert_eq!(made["changed"], json!(["Recipe"]), "{made}");

    // A draft checked, with the body MCP requires even when it says nothing.
    let grown = RECIPE.replace("temp : REAL;", "temp : REAL; batch : DINT;");
    let (error, checked) = tool(
        "check_data",
        json!({"project_id": "plant", "body": {"type": "RecipeData", "source": grown}}),
    )
    .await;
    assert!(!error, "{checked}");
    assert_eq!(checked["ok"], true, "{checked}");
    assert_ne!(checked["blocks"][0]["layout"], made["blocks"][0]["layout"]);
    let (error, stored) = tool("check_data", json!({"project_id": "plant", "body": {}})).await;
    assert!(!error, "{stored}");
    assert_eq!(stored["blocks"], made["blocks"]);

    let (error, listed) = tool("list_data_types", json!({"project_id": "plant"})).await;
    assert!(!error);
    assert_eq!(listed, json!({ "types": [{ "name": "RecipeData" }] }));
    let (error, got) = tool(
        "get_data_type",
        json!({"project_id": "plant", "type_id": "RecipeData"}),
    )
    .await;
    assert!(!error);
    assert_eq!(got["source"], RECIPE);
    let (error, refused) = tool(
        "delete_data_type",
        json!({"project_id": "plant", "type_id": "RecipeData"}),
    )
    .await;
    assert!(error, "{refused}");
    let (error, _) = tool(
        "delete_data_block",
        json!({"project_id": "plant", "block": "Recipe"}),
    )
    .await;
    assert!(!error);
    let (error, blocks) = tool("list_data_blocks", json!({"project_id": "plant"})).await;
    assert!(!error);
    assert_eq!(
        blocks,
        json!({ "ok": true, "diagnostics": [], "blocks": [] })
    );
    assert!(project
        .join("data")
        .join("types")
        .join("RecipeData.st")
        .is_file());
    services.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_name_that_leaves_the_directory_or_names_a_device_is_refused() {
    let rig = rig().await;
    // The data directory exists once a call has reached the module.
    rig.eng("GET", TYPES, None, 200).await;
    let before = listing(&rig.root);
    let body = |name: &str| {
        Some(json!({ "source": format!("TYPE {name} : STRUCT a : INT; END_STRUCT; END_TYPE") }))
    };
    for (uri, expected) in [
        // By the core: a `..` segment, and a decoded separator in a parameter.
        (format!("{TYPES}/.."), 400),
        (format!("{TYPES}/a%5Cb"), 400),
        // A decoded `/` makes the path one segment longer: no route of the module.
        (format!("{TYPES}/..%2F..%2Fescaped"), 400),
        (format!("{TYPES}/a%2Fb"), 404),
    ] {
        let (status, said) = rig.send(&rig.engineer, "PUT", &uri, body("a")).await;
        assert_eq!(status.as_u16(), expected, "{uri}: {said}");
    }
    // By the module: a device name of Windows, in any case.
    for name in ["CON", "nul", "Lpt1"] {
        let said = rig
            .eng("PUT", &format!("{TYPES}/{name}"), body(name), 400)
            .await;
        assert_eq!(
            said,
            json!({ "error": format!("'{name}' cannot be a type name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)") })
        );
    }
    let said = rig
        .eng("PUT", &format!("{TYPES}/a.b"), body("a"), 400)
        .await;
    assert!(said["error"]
        .as_str()
        .unwrap()
        .starts_with("'a.b' is not a type name"));
    assert_eq!(listing(&rig.root), before, "something was written");
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_body_larger_than_the_core_takes_is_refused_before_the_module() {
    let rig = rig().await;
    let before = listing(&rig.root);
    // A comment of 3 MiB in an otherwise good source.
    let source = format!("(* {} *)\n{RECIPE}", "x".repeat(3 << 20));
    let (status, said) = rig
        .send_raw(
            &rig.engineer,
            "PUT",
            &format!("{TYPES}/RecipeData"),
            Some(json!({ "source": source }).to_string()),
        )
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{said}");
    assert_eq!(listing(&rig.root), before, "something was written");
    rig.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_build_of_the_data_module_takes_over_and_the_files_stay() {
    let rig = rig().await;
    rig.eng(
        "PUT",
        &format!("{TYPES}/RecipeData"),
        Some(json!({ "source": RECIPE })),
        200,
    )
    .await;
    rig.eng(
        "PUT",
        &format!("{BLOCKS}/Recipe"),
        Some(json!({ "type": "RecipeData", "retain": true })),
        200,
    )
    .await;
    let types = rig.eng("GET", TYPES, None, 200).await;
    let blocks = rig.eng("GET", BLOCKS, None, 200).await;

    // The same module from another path is another build: the server swaps it in.
    let elsewhere = tempfile::tempdir().unwrap();
    let copy = elsewhere.path().join(format!(
        "mythos-module-data{}",
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::copy(DATA, &copy).unwrap();
    let old = rig.services.get("data").unwrap().module.path.clone();
    rig.services.sync(vec![module_at(&copy).await]).await;
    let now = rig.services.get("data").unwrap().module.path.clone();
    assert_eq!(now, copy);
    assert_ne!(now, old);

    assert_eq!(rig.eng("GET", TYPES, None, 200).await, types);
    assert_eq!(rig.eng("GET", BLOCKS, None, 200).await, blocks);
    // And the new process writes them as the old one did.
    let replaced = rig
        .eng(
            "PUT",
            &format!("{BLOCKS}/Recipe"),
            Some(json!({ "type": "RecipeData", "retain": false })),
            200,
        )
        .await;
    assert_eq!(replaced["created"], false);
    assert_eq!(replaced["changed"], json!([]));
    rig.close().await;
}
