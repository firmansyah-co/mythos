//! A service module for the tests (ADR-011): notes kept per project.
//!
//! Its routes cover what a service may do: read and write its own data in the directory the
//! server gives it per project, tell the server about a change (an event), answer according to
//! the role the server checked, and die when asked so the server's restart can be seen.
//! Its name lacks the `mythos-module-` prefix for the same reason as the Modbus one.

use async_trait::async_trait;
use mythos_module::protocol::{
    HttpCall, HttpReply, ModuleKind, ModuleManifest, RouteDecl, RouteScope, PROTOCOL_VERSION,
};
use mythos_module::{run_service_module, Events, Service};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::ExitCode;

fn route(
    method: &str,
    path: &str,
    scope: RouteScope,
    role: Option<&str>,
    tool: &str,
    body: Option<Value>,
) -> RouteDecl {
    RouteDecl {
        method: method.into(),
        path: path.into(),
        scope,
        role: role.map(str::to_string),
        tool: tool.into(),
        summary: format!("{method} {path}"),
        body,
        query: vec![],
        needs: vec![],
    }
}

fn manifest() -> ModuleManifest {
    ModuleManifest {
        protocol: PROTOCOL_VERSION,
        id: "org.mythos.test.notes".into(),
        name: "Notes".into(),
        version: std::env::var("NOTES_VERSION").unwrap_or_else(|_| "1.0.0".into()),
        kind: ModuleKind::Service,
        provides: "notes".into(),
        description: "Notes per project, for the tests".into(),
        capabilities: vec![],
        config_schema: Value::Null,
        always_on: false,
        routes: vec![
            route(
                "GET",
                "/notes",
                RouteScope::Project,
                Some("viewer"),
                "list_notes",
                None,
            ),
            route(
                "GET",
                "/notes/:note",
                RouteScope::Project,
                Some("viewer"),
                "read_note",
                None,
            ),
            route(
                "PUT",
                "/notes/:note",
                RouteScope::Project,
                Some("engineer"),
                "write_note",
                Some(
                    json!({"type": "object", "required": ["text"], "properties": {"text": {"type": "string"}}}),
                ),
            ),
            route(
                "GET",
                "/whoami",
                RouteScope::Global,
                None,
                "notes_whoami",
                None,
            ),
            route(
                "POST",
                "/crash",
                RouteScope::Global,
                Some("viewer"),
                "notes_crash",
                None,
            ),
            route(
                "GET",
                "/pid",
                RouteScope::Global,
                Some("viewer"),
                "notes_pid",
                None,
            ),
        ],
    }
}

struct Notes;

fn dir(call: &HttpCall) -> Option<PathBuf> {
    call.data_dir.as_ref().map(PathBuf::from)
}

#[async_trait]
impl Service for Notes {
    async fn handle(&self, call: HttpCall, events: &Events) -> HttpReply {
        match (call.method.as_str(), call.route.as_str()) {
            ("GET", "/notes") => {
                let Some(dir) = dir(&call) else {
                    return HttpReply::error(500, "no data directory");
                };
                let mut names: Vec<String> = std::fs::read_dir(&dir)
                    .map(|entries| {
                        entries
                            .flatten()
                            .filter_map(|e| {
                                e.file_name()
                                    .to_str()?
                                    .strip_suffix(".txt")
                                    .map(str::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                names.sort();
                HttpReply::json(200, json!(names))
            }
            ("GET", "/notes/:note") => {
                let Some(dir) = dir(&call) else {
                    return HttpReply::error(500, "no data directory");
                };
                let note = &call.params["note"];
                match std::fs::read_to_string(dir.join(format!("{note}.txt"))) {
                    Ok(text) => HttpReply::json(200, json!({"note": note, "text": text})),
                    Err(_) => HttpReply::error(404, format!("there is no note '{note}'")),
                }
            }
            ("PUT", "/notes/:note") => {
                let Some(dir) = dir(&call) else {
                    return HttpReply::error(500, "no data directory");
                };
                let note = call.params["note"].clone();
                let Some(text) = call.body.as_ref().and_then(|b| b["text"].as_str()) else {
                    return HttpReply::error(422, "'text' is required");
                };
                if let Err(e) = std::fs::write(dir.join(format!("{note}.txt")), text) {
                    return HttpReply::error(500, e.to_string());
                }
                events.send(
                    "notes_changed",
                    json!({"project": call.project, "note": note}),
                );
                HttpReply::json(200, json!({"note": note, "text": text}))
            }
            ("GET", "/whoami") => HttpReply::json(200, json!(call.identity)),
            ("POST", "/crash") => std::process::exit(3),
            ("GET", "/pid") => HttpReply::json(200, json!(std::process::id())),
            _ => HttpReply::error(404, "not a route of this module"),
        }
    }
}

fn main() -> ExitCode {
    run_service_module(manifest(), Notes)
}
