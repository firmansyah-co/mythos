//! The HMI module's routes as MCP tools (ADR-011 §2.2): generated from its manifest, under the
//! names they had when they were core routes, with the bodies their schemas document.

mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use mythos_core::storage::{ControllerStorage, ProjectConfig};
use mythos_core::tag::TagCatalog;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const PRESSURE: &str = "external.BoilerRoom.PT101.pressure";

struct Rig {
    router: Router,
    _dir: tempfile::TempDir,
}

async fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(dir.path()).unwrap());
    storage
        .create_project(&ProjectConfig::new("plant", "Plant"))
        .unwrap();
    let router = common::hmi_router(storage, Arc::new(RwLock::new(TagCatalog::new()))).await;
    Rig { router, _dir: dir }
}

impl Rig {
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, "127.0.0.1:8080")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .body(Body::from(
                json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string(),
            ))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Call a tool; returns (isError, its body).
    async fn tool(&self, name: &str, arguments: Value) -> (bool, Value) {
        let answer = self
            .rpc("tools/call", json!({"name": name, "arguments": arguments}))
            .await;
        let result = &answer["result"];
        assert!(result.is_object(), "no result: {answer}");
        (
            result["isError"].as_bool().unwrap(),
            result["structuredContent"]["body"].clone(),
        )
    }

    async fn body_schema(&self, tool: &str) -> Value {
        let listed = self.rpc("tools/list", json!({})).await;
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == tool)
            .unwrap_or_else(|| panic!("no tool {tool}"))["inputSchema"]["properties"]["body"]
            .clone()
    }
}

fn required(schema: &Value) -> Vec<String> {
    schema["required"]
        .as_array()
        .map(|r| r.iter().map(|v| v.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_can_publish_a_screen() {
    let rig = rig().await;
    let (error, screen) = rig
        .tool(
            "create_screen",
            json!({"project_id": "plant", "body": {"name": "Overview", "is_root": true}}),
        )
        .await;
    assert!(!error, "{screen}");
    let screen_id = screen["id"].as_str().unwrap().to_string();
    let canvas = json!({"title": "Overview", "components": [{"id": "e1", "type": "io_field",
        "x": 0, "y": 0, "width": 160, "height": 32, "tag": PRESSURE, "label": "Pressure", "unit": "bar"}]});
    let (error, saved) = rig
        .tool(
            "save_screen_draft",
            json!({"project_id": "plant", "screen_id": screen_id, "body": canvas}),
        )
        .await;
    assert!(!error, "{saved}");
    let (error, released) = rig
        .tool("release_screens", json!({"project_id": "plant"}))
        .await;
    assert!(!error, "{released}");
    let (_, screens) = rig
        .tool(
            "list_screens",
            json!({"project_id": "plant", "view": "active"}),
        )
        .await;
    assert_eq!(
        screens[0]["canvasData"]["components"][0]["tag"], PRESSURE,
        "{screens}"
    );
}

/// The field names every documented body uses, filled in from the tool's own schema, sent to
/// the module. A schema that drifts from what the module reads fails here.
#[tokio::test(flavor = "multi_thread")]
async fn the_bodies_the_faceplate_tools_document_are_what_the_module_accepts() {
    let rig = rig().await;
    let udt_schema = rig.body_schema("create_faceplate_type").await["properties"]["udt"].clone();
    assert_eq!(required(&udt_schema), vec!["name", "kind"]);
    let member = &udt_schema["properties"]["members"]["items"];
    assert_eq!(required(member), vec!["name", "dataType"]);
    let udt = json!({"name": "Motor", "kind": "STRUCT",
        "members": [{"name": "running", "dataType": "BOOL"}, {"name": "speed", "dataType": "REAL"}]});

    let (error, faceplate) = rig
        .tool(
            "create_faceplate_type",
            json!({"project_id": "plant", "body": {"name": "MotorFp", "udt": udt}}),
        )
        .await;
    assert!(!error, "{faceplate}");
    let type_id = faceplate["id"]
        .as_str()
        .expect("a faceplate type id")
        .to_string();

    let instance_schema = rig.body_schema("create_faceplate_instance").await;
    assert_eq!(required(&instance_schema), vec!["name", "typeId"]);
    let binding = &instance_schema["properties"]["bindings"]["items"];
    assert_eq!(required(binding), vec!["faceplateKey", "tagUri"]);
    // A fresh type declares no binding keys, so the module refuses this one. That it names the
    // key proves it read the binding with the field names documented.
    let (error, refused) = rig
        .tool(
            "create_faceplate_instance",
            json!({"project_id": "plant", "body": {"name": "M0", "typeId": type_id,
                "bindings": [{"faceplateKey": "running", "tagUri": ""}]}}),
        )
        .await;
    assert!(error);
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("binding 'running'"),
        "{refused}"
    );
    let (error, instance) = rig
        .tool(
            "create_faceplate_instance",
            json!({"project_id": "plant", "body": {"name": "M1", "typeId": type_id, "bindings": []}}),
        )
        .await;
    assert!(!error, "{instance}");
    let instance_id = instance["id"].as_str().expect("an instance id").to_string();
    let (error, updated) = rig
        .tool(
            "update_faceplate_instance",
            json!({"project_id": "plant", "instance_id": instance_id, "body": {"typeVersion": null}}),
        )
        .await;
    assert!(!error, "{updated}");
}
