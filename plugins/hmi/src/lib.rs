//! The MythOS HMI as a service module (ADR-011 §3 step 4): HMI screens and faceplates, served
//! at the URLs they always had (`/api/v1/projects/:id/hmi/…`), from the module's own data
//! directory (`<project>/hmi/`). Its workbench UI (screen editor, operator runtime) ships beside
//! it as `modules-ui/org.mythos.hmi/`.

pub mod faceplate;
pub mod screens;
pub mod udt;

use async_trait::async_trait;
use faceplate::{
    release_event_payload, FaceplateError, FaceplateStore, FaceplateType, InstanceBinding,
    ReadStage, UdtDefinition,
};
use mythos_module::protocol::{
    HttpCall, HttpReply, ModuleKind, ModuleManifest, RouteDecl, RouteScope, PROTOCOL_VERSION,
};
use mythos_module::{Events, Service};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

pub const MODULE_ID: &str = "org.mythos.hmi";
/// The module's name in URLs and its data directory: `/api/v1/projects/:id/hmi/…`.
pub const PROVIDES: &str = "hmi";
/// Sent on the telemetry stream when a project's screens are released.
pub const SCREENS_RELEASED_EVENT: &str = "SCREENS_RELEASED";
/// Sent when a faceplate type is released and its instances follow.
pub const FACEPLATE_PROPAGATED_EVENT: &str = "FACEPLATE_PROPAGATED";

const VIEW: (&str, &str) = (
    "view",
    "\"draft\" (default) for the version being edited, \"active\" for the released one",
);

fn route(
    method: &str,
    path: &str,
    role: &str,
    tool: &str,
    summary: &str,
    body: Option<Value>,
    query: &[(&str, &str)],
) -> RouteDecl {
    RouteDecl {
        method: method.into(),
        path: path.into(),
        scope: RouteScope::Project,
        role: Some(role.into()),
        tool: tool.into(),
        summary: summary.into(),
        body,
        query: query
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        needs: vec![],
    }
}

fn needing_tags(mut decl: RouteDecl) -> RouteDecl {
    decl.needs = vec!["tags".into()];
    decl
}

fn canvas_schema() -> Value {
    json!({
        "type": "object",
        "required": ["components"],
        "description": "The screen's whole canvas; it replaces the saved draft. Saving does not release.",
        "properties": {
            "title": { "type": "string" },
            "components": {
                "type": "array",
                "description": "What the workbench draws. It knows two kinds; any other is kept as it is and not drawn.",
                "items": {
                    "type": "object",
                    "required": ["id", "type", "x", "y", "width", "height"],
                    "properties": {
                        "id": { "type": "string" },
                        "type": { "type": "string", "enum": ["text", "io_field"] },
                        "x": { "type": "number" },
                        "y": { "type": "number" },
                        "width": { "type": "number" },
                        "height": { "type": "number" },
                        "text": { "type": "string", "description": "text: what it shows" },
                        "tag": { "type": "string", "description": "io_field: the tag URI it shows, \"\" for none" },
                        "label": { "type": "string", "description": "io_field" },
                        "unit": { "type": "string", "description": "io_field" }
                    }
                }
            }
        }
    })
}

fn udt_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name", "kind"],
        "properties": {
            "name": { "type": "string" },
            "kind": { "type": "string", "enum": ["STRUCT", "ENUM", "ALIAS"] },
            "members": {
                "type": "array",
                "description": "STRUCT: its fields",
                "items": {
                    "type": "object",
                    "required": ["name", "dataType"],
                    "properties": { "name": { "type": "string" }, "dataType": { "type": "string" } }
                }
            },
            "enumConstants": { "type": "array", "items": { "type": "string" }, "description": "ENUM: its values" },
            "baseType": { "type": "string", "description": "ALIAS: the type it names" }
        }
    })
}

fn binding_schema() -> Value {
    json!({
        "type": "object",
        "required": ["faceplateKey", "tagUri"],
        "properties": {
            "faceplateKey": { "type": "string" },
            "tagUri": { "type": "string", "description": "A registered tag URI, or \"\" for none" }
        }
    })
}

/// What `mythos-module-hmi --manifest` prints. The tool names are the ones these routes had as
/// core routes, so an MCP client written before the move keeps working.
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        protocol: PROTOCOL_VERSION,
        id: MODULE_ID.into(),
        name: "HMI".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        kind: ModuleKind::Service,
        provides: PROVIDES.into(),
        description: "HMI screens, faceplates and the operator runtime".into(),
        capabilities: vec![],
        config_schema: Value::Null,
        routes: vec![
            route("GET", "/screens", "viewer", "list_screens",
                "A project's HMI screens with their canvases.", None, &[VIEW]),
            route("POST", "/screens", "engineer", "create_screen",
                "Add an empty screen to a project's draft.",
                Some(json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string" },
                        "is_root": { "type": "boolean", "default": false }
                    }
                })), &[]),
            route("DELETE", "/screens/:screen", "engineer", "delete_screen",
                "Remove a screen from a project's draft.", None, &[]),
            route("POST", "/screens/:screen/subscreens", "engineer", "create_subscreen",
                "Add a sub-screen to a screen.",
                Some(json!({
                    "type": "object",
                    "required": ["name", "type"],
                    "properties": { "name": { "type": "string" }, "type": { "type": "string" } }
                })), &[]),
            route("PUT", "/screens/:screen/draft", "engineer", "save_screen_draft",
                "Replace a screen's draft canvas. Operators do not see it until release_screens.",
                Some(canvas_schema()), &[]),
            route("POST", "/commit", "engineer", "release_screens",
                "Release every screen's saved draft to the operator runtime, which shows it without a reload.",
                None, &[]),
            route("GET", "/faceplates/types", "viewer", "list_faceplate_types",
                "A project's faceplate types.", None, &[]),
            route("POST", "/faceplates/types", "engineer", "create_faceplate_type",
                "Add a faceplate type built on a UDT.",
                Some(json!({
                    "type": "object",
                    "required": ["name", "udt"],
                    "properties": { "name": { "type": "string" }, "udt": udt_schema() }
                })), &[]),
            route("GET", "/faceplates/types/:type", "viewer", "get_faceplate_type",
                "One faceplate type.", None,
                &[("view", "\"active\" (default) for the released definition, \"draft\" for the one being edited")]),
            route("PUT", "/faceplates/types/:type/draft", "engineer", "save_faceplate_type_draft",
                "Replace a faceplate type's draft.",
                Some(json!({ "type": "object", "description": "The whole faceplate type, as get_faceplate_type returns it, with the changes made" })),
                &[]),
            route("POST", "/faceplates/types/:type/release", "engineer", "release_faceplate_type",
                "Release a faceplate type's draft and propagate it to its instances.", None, &[]),
            route("GET", "/faceplates/instances", "viewer", "list_faceplate_instances",
                "A project's faceplate instances.", None, &[("stage", "Which stage to list")]),
            needing_tags(route("POST", "/faceplates/instances", "engineer", "create_faceplate_instance",
                "Place an instance of a faceplate type, bound to registered tags.",
                Some(json!({
                    "type": "object",
                    "required": ["name", "typeId"],
                    "properties": {
                        "name": { "type": "string" },
                        "typeId": { "type": "string" },
                        "typeVersion": { "type": "integer", "description": "A published version to freeze to; omit to follow the active one" },
                        "bindings": { "type": "array", "items": binding_schema() }
                    }
                })), &[])),
            needing_tags(route("PUT", "/faceplates/instances/:instance", "engineer", "update_faceplate_instance",
                "Change an instance's pinned type version or its bindings.",
                Some(json!({
                    "type": "object",
                    "properties": {
                        "typeVersion": {
                            "type": ["integer", "null"],
                            "description": "Absent: unchanged. null: follow the active version. A number: freeze to it."
                        },
                        "bindings": { "type": "array", "items": binding_schema() }
                    }
                })), &[])),
        ],
    }
}

/// Write through a temporary file and rename, so a crash never leaves a half-written file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Before the HMI was a module, the core kept its files in the project directory itself
/// (`<project>/screens/`, `<project>/faceplates/`). The first time the module serves a project
/// it copies them into its own directory (`<project>/hmi/`). They are copied, not moved: the
/// originals stay, so a server rolled back to a version before the module still finds them.
pub fn adopt_legacy_files(data: &Path) -> std::io::Result<()> {
    let Some(project) = data.parent() else {
        return Ok(());
    };
    for part in ["screens", "faceplates"] {
        let mine = data.join(part);
        let legacy = project.join(part);
        if !mine.exists() && legacy.is_dir() {
            copy_tree(&legacy, &mine)?;
        }
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    // Copy into a sibling first and rename, so a copy cut short is never taken as done.
    let partial = to.with_extension("adopting");
    if partial.exists() {
        std::fs::remove_dir_all(&partial)?;
    }
    copy_into(from, &partial)?;
    std::fs::rename(&partial, to)
}

fn copy_into(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_into(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn reply(result: Result<Value, (u16, Value)>, status: u16) -> HttpReply {
    match result {
        Ok(body) => HttpReply::json(status, body),
        Err((status, body)) => HttpReply::json(status, body),
    }
}

fn error(status: u16, message: impl ToString) -> (u16, Value) {
    (status, json!({ "error": message.to_string() }))
}

fn faceplate_error(e: FaceplateError) -> (u16, Value) {
    match e {
        FaceplateError::TypeNotFound(_) | FaceplateError::InstanceNotFound(_) => error(404, e),
        FaceplateError::Invalid(_) => error(400, e),
        FaceplateError::ProjectNotFound(_) => error(404, e),
        other => error(500, other),
    }
}

fn body<T: for<'de> Deserialize<'de>>(call: &HttpCall) -> Result<T, (u16, Value)> {
    serde_json::from_value(call.body.clone().unwrap_or(Value::Null))
        .map_err(|e| error(400, format!("the body is not what this route takes: {e}")))
}

fn param<'a>(call: &'a HttpCall, name: &str) -> &'a str {
    call.params.get(name).map(String::as_str).unwrap_or("")
}

#[derive(Deserialize)]
struct CreateScreen {
    name: String,
    #[serde(default)]
    is_root: bool,
}

#[derive(Deserialize)]
struct CreateSubScreen {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct CreateType {
    name: String,
    udt: UdtDefinition,
}

#[derive(Deserialize)]
struct CreateInstance {
    name: String,
    #[serde(rename = "typeId")]
    type_id: String,
    #[serde(rename = "typeVersion", default)]
    type_version: Option<u32>,
    #[serde(default)]
    bindings: Vec<InstanceBinding>,
}

/// `typeVersion` is tri-state: absent leaves the freeze state untouched, `null` unfreezes
/// (follow active), a number freezes to that published version.
#[derive(Deserialize)]
struct UpdateInstance {
    #[serde(rename = "typeVersion", default, deserialize_with = "double_option")]
    type_version: Option<Option<u32>>,
    #[serde(default)]
    bindings: Option<Vec<InstanceBinding>>,
}

fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Refuse any binding whose `tagUri` is not registered in the tag catalog the server sent.
///
/// An empty `tagUri` means **unassigned** and is allowed: an engineer authors a screen before
/// the device is commissioned. Only a supplied URI must exist. Every unknown one is reported at
/// once, in a list a client can highlight from.
fn require_registered_tags(
    call: &HttpCall,
    bindings: &[InstanceBinding],
) -> Result<(), (u16, Value)> {
    let Some(registered) = &call.tags else {
        // The manifest asks for the tags on these routes; without them nothing can be checked.
        return Err(error(
            503,
            "the server sent no tag catalog to check the bindings against",
        ));
    };
    let unknown: Vec<&str> = bindings
        .iter()
        .map(|b| b.tag_uri.trim())
        .filter(|uri| !uri.is_empty() && !registered.iter().any(|r| r == uri))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    Err((
        400,
        json!({
            "error": "One or more tagUri values are not registered in the tag catalog",
            "code": "UNREGISTERED_TAG_URI",
            "unknownTagUris": unknown,
            "hint": "Register the tags, or leave tagUri empty to save the binding as unassigned.",
        }),
    ))
}

/// The HMI service. Writes are serialized: each is a read-modify-write of one file, and the
/// server forwards requests concurrently.
#[derive(Default)]
pub struct Hmi {
    writes: Mutex<()>,
}

impl Hmi {
    fn handle_in(&self, call: &HttpCall, data: &Path, events: &Events) -> HttpReply {
        let project = call.project.clone().unwrap_or_default();
        match (call.method.as_str(), call.route.as_str()) {
            ("GET", "/screens") => {
                let stage =
                    match screens::Stage::from_view(call.query.get("view").map(String::as_str)) {
                        Ok(stage) => stage,
                        Err(e) => return HttpReply::error(400, e),
                    };
                reply(
                    screens::read(data, stage)
                        .map(|s| json!(s))
                        .map_err(refused),
                    200,
                )
            }
            ("POST", "/screens") => reply(
                body::<CreateScreen>(call).and_then(|b| {
                    screens::create(data, &b.name, b.is_root)
                        .map(|s| json!(s))
                        .map_err(refused)
                }),
                201,
            ),
            ("DELETE", "/screens/:screen") => reply(
                screens::delete(data, param(call, "screen"))
                    .map(|()| Value::Null)
                    .map_err(refused),
                204,
            ),
            ("POST", "/screens/:screen/subscreens") => reply(
                body::<CreateSubScreen>(call).and_then(|b| {
                    screens::create_subscreen(data, param(call, "screen"), &b.name, &b.kind)
                        .map(|s| json!(s))
                        .map_err(refused)
                }),
                201,
            ),
            ("PUT", "/screens/:screen/draft") => reply(
                screens::save_draft(
                    data,
                    param(call, "screen"),
                    call.body.clone().unwrap_or(Value::Null),
                )
                .map(|()| Value::Null)
                .map_err(refused),
                204,
            ),
            ("POST", "/commit") => match screens::release(data) {
                Ok(released) => {
                    // Every open station is told: an operator's runtime shows the released
                    // screens and must not go on showing the previous release until a reload.
                    events.send(
                        SCREENS_RELEASED_EVENT,
                        json!({ "project_id": project, "screens": released.len() }),
                    );
                    let by_id: serde_json::Map<String, Value> = released
                        .into_iter()
                        .map(|s| (s.id, s.canvas_data))
                        .collect();
                    HttpReply::json(200, Value::Object(by_id))
                }
                Err(e) => HttpReply::error(e.status, e.message),
            },
            _ => self.faceplates(call, data, events),
        }
    }

    fn faceplates(&self, call: &HttpCall, data: &Path, events: &Events) -> HttpReply {
        let store = FaceplateStore::new(data);
        match (call.method.as_str(), call.route.as_str()) {
            ("GET", "/faceplates/types") => reply(
                store
                    .type_summaries()
                    .map(|t| json!({ "types": t }))
                    .map_err(faceplate_error),
                200,
            ),
            ("POST", "/faceplates/types") => reply(
                body::<CreateType>(call).and_then(|b| {
                    store
                        .create_type(b.name, b.udt)
                        .map(|t| json!(t))
                        .map_err(faceplate_error)
                }),
                201,
            ),
            ("GET", "/faceplates/types/:type") => {
                let view = call
                    .query
                    .get("view")
                    .map(String::as_str)
                    .unwrap_or("active");
                reply(
                    store
                        .read_type(param(call, "type"), view)
                        .map(|t| json!(t))
                        .map_err(faceplate_error),
                    200,
                )
            }
            ("PUT", "/faceplates/types/:type/draft") => {
                let id = param(call, "type");
                reply(
                    body::<FaceplateType>(call).and_then(|mut def| {
                        // The path is authoritative; a different id in the body would write to
                        // another type than the one the caller addressed.
                        if !def.id.is_empty() && def.id != id {
                            return Err(error(
                                400,
                                format!(
                                    "Body type id '{}' does not match route type id '{id}'",
                                    def.id
                                ),
                            ));
                        }
                        def.id = id.to_owned();
                        store
                            .save_draft(def)
                            .map(|_| Value::Null)
                            .map_err(faceplate_error)
                    }),
                    204,
                )
            }
            ("POST", "/faceplates/types/:type/release") => match store.release(param(call, "type"))
            {
                Ok(result) => {
                    let at_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    events.send(
                        FACEPLATE_PROPAGATED_EVENT,
                        release_event_payload(&result, at_ms),
                    );
                    HttpReply::json(200, json!(result))
                }
                Err(e) => {
                    let (status, body) = faceplate_error(e);
                    HttpReply::json(status, body)
                }
            },
            ("GET", "/faceplates/instances") => {
                let stage = match call.query.get("stage") {
                    Some(raw) => match raw.parse::<ReadStage>() {
                        Ok(stage) => stage,
                        Err(e) => {
                            let (status, body) = faceplate_error(e);
                            return HttpReply::json(status, body);
                        }
                    },
                    None => ReadStage::default(),
                };
                reply(
                    store
                        .resolve_instances(stage)
                        .map(|i| json!({ "instances": i }))
                        .map_err(faceplate_error),
                    200,
                )
            }
            ("POST", "/faceplates/instances") => reply(
                body::<CreateInstance>(call).and_then(|b| {
                    require_registered_tags(call, &b.bindings)?;
                    store
                        .create_instance(b.name, b.type_id, b.type_version, b.bindings)
                        .map(|i| json!(i))
                        .map_err(faceplate_error)
                }),
                201,
            ),
            ("PUT", "/faceplates/instances/:instance") => reply(
                body::<UpdateInstance>(call).and_then(|b| {
                    if let Some(bindings) = &b.bindings {
                        require_registered_tags(call, bindings)?;
                    }
                    store
                        .update_instance(param(call, "instance"), b.type_version, b.bindings)
                        .map(|i| json!(i))
                        .map_err(faceplate_error)
                }),
                200,
            ),
            _ => HttpReply::error(404, "not a route of the HMI module"),
        }
    }
}

fn refused(e: screens::Refused) -> (u16, Value) {
    error(e.status, e.message)
}

#[async_trait]
impl Service for Hmi {
    async fn handle(&self, call: HttpCall, events: &Events) -> HttpReply {
        let Some(data) = call.data_dir.as_ref().map(PathBuf::from) else {
            return HttpReply::error(500, "the server gave the HMI no data directory");
        };
        let _guard = self.writes.lock().await;
        if let Err(e) = adopt_legacy_files(&data) {
            return HttpReply::error(
                500,
                format!("the project's earlier HMI files could not be taken over: {e}"),
            );
        }
        self.handle_in(&call, &data, events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earlier_files_are_copied_in_once_and_the_originals_stay() {
        let project = tempfile::tempdir().unwrap();
        let legacy = project.path().join("screens").join("draft");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(
            legacy.join("screens.json"),
            r#"[{"id":"s1","name":"Boiler","is_root":true,"canvasData":{"components":[]}}]"#,
        )
        .unwrap();
        let data = project.path().join("hmi");
        std::fs::create_dir_all(&data).unwrap();

        adopt_legacy_files(&data).unwrap();
        let read = screens::read(&data, screens::Stage::Draft).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "Boiler");
        assert!(
            legacy.join("screens.json").is_file(),
            "the original is kept"
        );

        // Once the module has its own files, the old ones are never read again.
        screens::delete(&data, "s1").unwrap();
        adopt_legacy_files(&data).unwrap();
        assert!(screens::read(&data, screens::Stage::Draft)
            .unwrap()
            .is_empty());
        assert!(!data.join("screens.adopting").exists());
    }

    #[test]
    fn every_route_is_for_a_project_and_names_the_least_role_it_needs() {
        for decl in manifest().routes {
            assert_eq!(decl.scope, RouteScope::Project, "{}", decl.path);
            let role = decl.role.as_deref().unwrap_or("admin");
            let reads = decl.method == "GET";
            assert_eq!(
                role,
                if reads { "viewer" } else { "engineer" },
                "{} {}",
                decl.method,
                decl.path
            );
        }
    }
}
