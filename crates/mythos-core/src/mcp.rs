//! # Model Context Protocol endpoint
//!
//! `POST /mcp` serves the [Model Context Protocol](https://modelcontextprotocol.io) over its
//! Streamable HTTP transport, answering every request with a single JSON response. An AI agent
//! connected to it can do everything the workbench can: every route of the HTTP API is a tool.
//!
//! A tool does not reimplement its route. It builds the HTTP request the route expects and sends
//! it through the same router, so an agent and the workbench cannot see different behaviour, and
//! a fix to a route is a fix to its tool.
//!
//! Coverage is enforced: [`TOOLS`] and [`NOT_TOOLS`] together must name every route the API
//! serves, and a test reads the router's source to check it. A new route without a tool, or
//! without a reason in [`NOT_TOOLS`], fails the build's tests.
//!
//! Authentication is the HTTP API's: an access token or a session, checked per tool by the
//! route it calls (see `router`). Requests carrying an `Origin` header from
//! anywhere but this host are refused, which stops a web page in a browser on the same machine
//! from driving the server (DNS rebinding).

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Map, Value};
use std::sync::Arc;
use tower::ServiceExt;

/// Protocol versions this endpoint speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// One MCP tool: an HTTP route, described for an agent.
#[derive(Debug, Clone, Copy)]
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub method: &'static str,
    /// The route, exactly as the router declares it (`:name` for a path parameter).
    pub path: &'static str,
    /// Query parameters the route reads.
    pub query: &'static [(&'static str, &'static str)],
    /// JSON Schema of the request body, when the route takes one.
    pub body: Option<fn() -> Value>,
    pub read_only: bool,
    pub destructive: bool,
}

/// A route that deliberately has no tool, and why.
#[derive(Debug, Clone, Copy)]
pub struct NotATool {
    pub method: &'static str,
    pub path: &'static str,
    pub reason: &'static str,
}

/// Routes that are not tools.
pub const NOT_TOOLS: &[NotATool] = &[
    NotATool {
        method: "GET",
        path: "/api/v1/telemetry/stream",
        reason:
            "an endless Server-Sent Events stream; read_tag_values returns the same reading once",
    },
    NotATool {
        method: "GET",
        path: "/api/v1/collab/ws/:room_id",
        reason: "a WebSocket for live co-editing between workbenches",
    },
    NotATool {
        method: "POST",
        path: "/api/v1/control/deploy",
        reason: "answers 501: there is no logic engine, so there is nothing to deploy",
    },
    NotATool {
        method: "POST",
        path: "/api/v1/auth/setup",
        reason:
            "creates the first administrator with the setup code from the controller's console; \
                 an MCP client already has an account, since it connects with that account's token",
    },
    NotATool {
        method: "POST",
        path: "/api/v1/auth/login",
        reason:
            "starts a browser session; an MCP client authenticates with an access token on its \
                 connection instead",
    },
    NotATool {
        method: "POST",
        path: "/api/v1/auth/logout",
        reason: "ends a browser session; an MCP connection has none (revoke_my_token ends a token)",
    },
    NotATool {
        method: "GET",
        path: "/",
        reason: "the workbench's web page",
    },
    NotATool {
        method: "GET",
        path: "/assets/*path",
        reason: "the workbench's static files",
    },
];

/// Path parameter names in the router, and the argument name a tool uses for each.
const PARAMS: &[(&str, &str)] = &[
    ("id", "project_id"),
    ("screen", "screen_id"),
    ("dut", "datatype_id"),
    ("type", "type_id"),
    ("instance", "instance_id"),
    ("uri", "uri"),
    ("token", "token_id"),
    ("username", "username"),
];

fn argument_name(param: &str) -> &str {
    PARAMS
        .iter()
        .find(|(p, _)| *p == param)
        .map(|(_, a)| *a)
        .unwrap_or(param)
}

fn path_params(path: &str) -> Vec<&str> {
    path.split('/')
        .filter_map(|segment| segment.strip_prefix(':'))
        .collect()
}

const VIEW: (&str, &str) = (
    "view",
    "\"draft\" (default) for the version being edited, \"active\" for the released one",
);

fn object(description: &str) -> Value {
    json!({ "type": "object", "description": description })
}

const ROLES: [&str; 4] = ["viewer", "operator", "engineer", "admin"];

fn password_change_body() -> Value {
    json!({
        "type": "object",
        "required": ["current_password", "new_password"],
        "properties": {
            "current_password": { "type": "string" },
            "new_password": { "type": "string", "minLength": crate::auth::MIN_PASSWORD_LEN }
        }
    })
}

fn new_token_body() -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": { "name": { "type": "string", "description": "What the token is for" } }
    })
}

fn new_user_body() -> Value {
    json!({
        "type": "object",
        "required": ["username", "role", "password"],
        "properties": {
            "username": { "type": "string", "description": "1 to 64 of a-z 0-9 . _ -" },
            "display_name": { "type": "string" },
            "role": { "type": "string", "enum": ROLES },
            "password": { "type": "string", "minLength": crate::auth::MIN_PASSWORD_LEN }
        }
    })
}

fn user_change_body() -> Value {
    json!({
        "type": "object",
        "description": "Only the fields given change",
        "properties": {
            "display_name": { "type": "string" },
            "role": { "type": "string", "enum": ROLES },
            "disabled": { "type": "boolean" },
            "password": { "type": "string", "minLength": crate::auth::MIN_PASSWORD_LEN }
        }
    })
}

fn create_project_body() -> Value {
    json!({
        "type": "object",
        "required": ["id", "name", "description", "author"],
        "properties": {
            "id": { "type": "string", "description": "1 to 128 of A-Z a-z 0-9 _ -; not 'active'" },
            "name": { "type": "string" },
            "description": { "type": "string" },
            "author": { "type": "string" }
        }
    })
}

fn create_screen_body() -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": { "type": "string" },
            "is_root": { "type": "boolean", "default": false }
        }
    })
}

fn create_subscreen_body() -> Value {
    json!({
        "type": "object",
        "required": ["name", "type"],
        "properties": { "name": { "type": "string" }, "type": { "type": "string" } }
    })
}

fn canvas_body() -> Value {
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

/// The data type routes take the definition itself as the body.
fn datatype_body() -> Value {
    udt_schema()
}

fn faceplate_type_create_body() -> Value {
    json!({
        "type": "object",
        "required": ["name", "udt"],
        "properties": { "name": { "type": "string" }, "udt": udt_schema() }
    })
}

fn faceplate_type_body() -> Value {
    object("The whole faceplate type, as get_faceplate_type returns it, with the changes made")
}

fn faceplate_instance_create_body() -> Value {
    json!({
        "type": "object",
        "required": ["name", "typeId"],
        "properties": {
            "name": { "type": "string" },
            "typeId": { "type": "string" },
            "typeVersion": { "type": "integer", "description": "A published version to freeze to; omit to follow the active one" },
            "bindings": { "type": "array", "items": binding_schema() }
        }
    })
}

fn faceplate_instance_update_body() -> Value {
    json!({
        "type": "object",
        "properties": {
            "typeVersion": {
                "type": ["integer", "null"],
                "description": "Absent: unchanged. null: follow the active version. A number: freeze to it."
            },
            "bindings": { "type": "array", "items": binding_schema() }
        }
    })
}

fn io_config_body() -> Value {
    json!({
        "type": "object",
        "required": ["connections"],
        "description": "The whole I/O configuration; it replaces the saved one.",
        "properties": {
            "connections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["id", "driver", "poll_interval_ms", "config"],
                    "properties": {
                        "id": { "type": "string" },
                        "driver": { "type": "string", "description": "A driver name from list_drivers" },
                        "poll_interval_ms": { "type": "integer", "minimum": 1 },
                        "cycle_timeout_ms": { "type": "integer", "minimum": 1 },
                        "config": object("The driver's configuration. Its JSON Schema is config_schema in list_modules.")
                    }
                }
            }
        }
    })
}

/// Every tool, one per route.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "get_auth_status",
        title: "Sign-in status",
        description: "Whether the controller still waits for its first administrator, and which account this connection acts as.",
        method: "GET",
        path: "/api/v1/auth/status",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "get_me",
        title: "My account",
        description: "The account this connection acts as: username, display name and role (viewer < operator < engineer < admin). A tool the role does not allow answers 403.",
        method: "GET",
        path: "/api/v1/auth/me",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "change_my_password",
        title: "Change my password",
        description: "Change this account's password. Every other browser session of the account ends.",
        method: "PUT",
        path: "/api/v1/auth/me/password",
        query: &[],
        body: Some(password_change_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "list_my_tokens",
        title: "List my access tokens",
        description: "This account's access tokens: id, name, when made and when last used. The tokens themselves are never shown again.",
        method: "GET",
        path: "/api/v1/auth/me/tokens",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "create_my_token",
        title: "Create an access token",
        description: "Make an access token for this account. The answer's 'secret' is the token, shown this once.",
        method: "POST",
        path: "/api/v1/auth/me/tokens",
        query: &[],
        body: Some(new_token_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "revoke_my_token",
        title: "Revoke an access token",
        description: "Revoke one of this account's access tokens by its id. A client using it is refused from then on.",
        method: "DELETE",
        path: "/api/v1/auth/me/tokens/:token",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "list_users",
        title: "List accounts",
        description: "Every account of the controller and its role. Administrators only.",
        method: "GET",
        path: "/api/v1/users",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "create_user",
        title: "Create an account",
        description: "Create an account with a role and an initial password. Administrators only.",
        method: "POST",
        path: "/api/v1/users",
        query: &[],
        body: Some(new_user_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "update_user",
        title: "Change an account",
        description: "Change an account's display name, role, disabled flag or password. Disabling it or setting its password ends its sessions. The last enabled administrator cannot be demoted or disabled. Administrators only.",
        method: "PUT",
        path: "/api/v1/users/:username",
        query: &[],
        body: Some(user_change_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "delete_user",
        title: "Delete an account",
        description: "Delete an account and its access tokens. The last enabled administrator cannot be deleted. Administrators only.",
        method: "DELETE",
        path: "/api/v1/users/:username",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "get_health",
        title: "Server health",
        description: "Whether the server is up.",
        method: "GET",
        path: "/api/v1/health",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "list_tags",
        title: "List tags",
        description: "Every tag that exists: its URI, tag bus slot, description and access mode. A tag exists because a running connection feeds it.",
        method: "GET",
        path: "/api/v1/tags",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "read_tag_values",
        title: "Read tag values",
        description: "Each tag's current value, type and quality, read from the tag bus now. Only a quality_name of GOOD means the value is current; anything else is stale or was never read.",
        method: "GET",
        path: "/api/v1/tags/values",
        query: &[("prefix", "Only tags whose URI starts with this")],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "get_tag",
        title: "Get a tag",
        description: "One tag's metadata.",
        method: "GET",
        path: "/api/v1/tags/:uri",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "list_projects",
        title: "List projects",
        description: "Every project stored on the controller.",
        method: "GET",
        path: "/api/v1/projects",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "create_project",
        title: "Create a project",
        description: "Create an empty project. It is not made active.",
        method: "POST",
        path: "/api/v1/projects",
        query: &[],
        body: Some(create_project_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "get_active_project",
        title: "Active project",
        description: "The project the controller is running, or null.",
        method: "GET",
        path: "/api/v1/projects/active",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "get_project",
        title: "Get a project",
        description: "One project's configuration.",
        method: "GET",
        path: "/api/v1/projects/:id",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "activate_project",
        title: "Activate a project",
        description: "Make this the project the controller runs. Field I/O switches to its configuration at once: the previous project's connections stop.",
        method: "POST",
        path: "/api/v1/projects/:id/activate",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "list_screens",
        title: "List HMI screens",
        description: "A project's HMI screens with their canvases.",
        method: "GET",
        path: "/api/v1/projects/:id/hmi/screens",
        query: &[VIEW],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "create_screen",
        title: "Create an HMI screen",
        description: "Add an empty screen to a project's draft.",
        method: "POST",
        path: "/api/v1/projects/:id/hmi/screens",
        query: &[],
        body: Some(create_screen_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "delete_screen",
        title: "Delete an HMI screen",
        description: "Remove a screen from a project's draft.",
        method: "DELETE",
        path: "/api/v1/projects/:id/hmi/screens/:screen",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "create_subscreen",
        title: "Create a sub-screen",
        description: "Add a sub-screen to a screen.",
        method: "POST",
        path: "/api/v1/projects/:id/hmi/screens/:screen/subscreens",
        query: &[],
        body: Some(create_subscreen_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "save_screen_draft",
        title: "Save a screen's canvas",
        description: "Replace a screen's draft canvas. Operators do not see it until release_screens.",
        method: "PUT",
        path: "/api/v1/projects/:id/hmi/screens/:screen/draft",
        query: &[],
        body: Some(canvas_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "release_screens",
        title: "Release HMI screens",
        description: "Release every screen's saved draft to the operator runtime, which shows it without a reload.",
        method: "POST",
        path: "/api/v1/projects/:id/hmi/commit",
        query: &[],
        body: None,
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "list_datatypes",
        title: "List data types",
        description: "A project's user data types (UDTs).",
        method: "GET",
        path: "/api/v1/projects/:id/datatypes",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "create_datatype",
        title: "Create a data type",
        description: "Add a user data type to a project.",
        method: "POST",
        path: "/api/v1/projects/:id/datatypes",
        query: &[],
        body: Some(datatype_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "get_datatype",
        title: "Get a data type",
        description: "One user data type.",
        method: "GET",
        path: "/api/v1/projects/:id/datatypes/:dut",
        query: &[VIEW],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "delete_datatype",
        title: "Delete a data type",
        description: "Remove a user data type.",
        method: "DELETE",
        path: "/api/v1/projects/:id/datatypes/:dut",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "save_datatype_draft",
        title: "Save a data type draft",
        description: "Replace a user data type's draft definition.",
        method: "PUT",
        path: "/api/v1/projects/:id/datatypes/:dut/draft",
        query: &[],
        body: Some(datatype_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "release_datatype",
        title: "Release a data type",
        description: "Release a user data type's draft.",
        method: "POST",
        path: "/api/v1/projects/:id/datatypes/:dut/release",
        query: &[],
        body: None,
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "list_faceplate_types",
        title: "List faceplate types",
        description: "A project's faceplate types.",
        method: "GET",
        path: "/api/v1/projects/:id/faceplates/types",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "create_faceplate_type",
        title: "Create a faceplate type",
        description: "Add a faceplate type built on a UDT.",
        method: "POST",
        path: "/api/v1/projects/:id/faceplates/types",
        query: &[],
        body: Some(faceplate_type_create_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "get_faceplate_type",
        title: "Get a faceplate type",
        description: "One faceplate type.",
        method: "GET",
        path: "/api/v1/projects/:id/faceplates/types/:type",
        query: &[VIEW],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "save_faceplate_type_draft",
        title: "Save a faceplate type draft",
        description: "Replace a faceplate type's draft.",
        method: "PUT",
        path: "/api/v1/projects/:id/faceplates/types/:type/draft",
        query: &[],
        body: Some(faceplate_type_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "release_faceplate_type",
        title: "Release a faceplate type",
        description: "Release a faceplate type's draft and propagate it to its instances.",
        method: "POST",
        path: "/api/v1/projects/:id/faceplates/types/:type/release",
        query: &[],
        body: None,
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "list_faceplate_instances",
        title: "List faceplate instances",
        description: "A project's faceplate instances.",
        method: "GET",
        path: "/api/v1/projects/:id/faceplates/instances",
        query: &[("stage", "Which stage to list")],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "create_faceplate_instance",
        title: "Create a faceplate instance",
        description: "Place an instance of a faceplate type, bound to registered tags.",
        method: "POST",
        path: "/api/v1/projects/:id/faceplates/instances",
        query: &[],
        body: Some(faceplate_instance_create_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "update_faceplate_instance",
        title: "Update a faceplate instance",
        description: "Change an instance's pinned type version or its bindings.",
        method: "PUT",
        path: "/api/v1/projects/:id/faceplates/instances/:instance",
        query: &[],
        body: Some(faceplate_instance_update_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "get_io_status",
        title: "Field I/O status",
        description: "Each running connection: its state, why it is not running when it is not, its tags, and how its last poll cycle went.",
        method: "GET",
        path: "/api/v1/io/status",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "list_drivers",
        title: "List drivers",
        description: "The driver names a connection may use.",
        method: "GET",
        path: "/api/v1/io/drivers",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "list_modules",
        title: "List modules",
        description: "The modules the server found, each with its manifest and the JSON Schema of the configuration it accepts, and the executables that could not be used as modules.",
        method: "GET",
        path: "/api/v1/modules",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "rescan_modules",
        title: "Look for modules again",
        description: "Find modules again now and swap every running connection whose module changed to the installed build. Reports what changed.",
        method: "POST",
        path: "/api/v1/modules/rescan",
        query: &[],
        body: None,
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "get_io_config",
        title: "Get a project's I/O configuration",
        description: "The connections a project is configured to read, or null.",
        method: "GET",
        path: "/api/v1/projects/:id/io",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "save_io_config",
        title: "Save a project's I/O configuration",
        description: "Check and save a project's whole I/O configuration. A configuration with any problem is refused whole, naming every problem. For the active project it is applied at once, changing only the connections that differ; the answer's changes says which.",
        method: "PUT",
        path: "/api/v1/projects/:id/io",
        query: &[],
        body: Some(io_config_body),
        read_only: false,
        destructive: true,
    },
];

impl Tool {
    /// The tool as `tools/list` describes it.
    pub fn describe(&self) -> Value {
        let mut properties = Map::new();
        let mut required: Vec<Value> = Vec::new();
        for param in path_params(self.path) {
            let name = argument_name(param);
            properties.insert(name.to_string(), json!({ "type": "string" }));
            required.push(json!(name));
        }
        for (name, description) in self.query {
            properties.insert(
                name.to_string(),
                json!({ "type": "string", "description": description }),
            );
        }
        if let Some(body) = self.body {
            properties.insert("body".to_string(), body());
            required.push(json!("body"));
        }
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            },
            "annotations": {
                "title": self.title,
                "readOnlyHint": self.read_only,
                "destructiveHint": self.destructive,
                "idempotentHint": self.read_only || self.method == "PUT",
                "openWorldHint": false
            }
        })
    }

    /// The HTTP request this tool's arguments mean, or why the arguments are wrong.
    fn request(&self, arguments: &Map<String, Value>) -> Result<Request<Body>, String> {
        let mut path = String::new();
        for segment in self.path.split('/').skip(1) {
            path.push('/');
            match segment.strip_prefix(':') {
                Some(param) => {
                    let name = argument_name(param);
                    let value = arguments
                        .get(name)
                        .and_then(Value::as_str)
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| format!("'{name}' is required and must be a string"))?;
                    path.push_str(&percent_encode(value));
                }
                None => path.push_str(segment),
            }
        }

        let mut query: Vec<String> = Vec::new();
        for (name, _) in self.query {
            if let Some(value) = arguments.get(*name) {
                let text = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                query.push(format!("{}={}", name, percent_encode(&text)));
            }
        }
        if !query.is_empty() {
            path.push('?');
            path.push_str(&query.join("&"));
        }

        let known = |key: &str| {
            key == "body"
                || self.query.iter().any(|(q, _)| *q == key)
                || path_params(self.path)
                    .iter()
                    .any(|p| argument_name(p) == key)
        };
        if let Some(unknown) = arguments.keys().find(|k| !known(k)) {
            return Err(format!("'{unknown}' is not an argument of {}", self.name));
        }

        let method = Method::from_bytes(self.method.as_bytes()).map_err(|e| e.to_string())?;
        let builder = Request::builder().method(method).uri(path);
        let request = match self.body {
            Some(_) => {
                let body = arguments
                    .get("body")
                    .ok_or_else(|| "'body' is required".to_string())?;
                builder
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
            }
            None => builder.body(Body::empty()),
        };
        request.map_err(|e| e.to_string())
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Endpoint
// -----------------------------------------------------------------------------

#[derive(Clone)]
struct McpState {
    api: Router,
    auth: Option<Arc<crate::auth::Auth>>,
}

/// The `/mcp` route, dispatching tools to `api`. With `auth`, a client must send an access
/// token (`Authorization: Bearer ...`) or a session cookie, and each tool runs with that
/// account's role: the credentials are passed on to the route the tool calls.
pub fn router(api: Router, auth: Option<Arc<crate::auth::Auth>>) -> Router {
    Router::new()
        .route("/mcp", post(handle).get(no_stream).delete(no_stream))
        .with_state(McpState { api, auth })
}

/// The headers of the MCP request that carry who sent it.
const CREDENTIAL_HEADERS: [header::HeaderName; 2] = [header::AUTHORIZATION, header::COOKIE];

/// This endpoint offers no server-initiated stream and keeps no sessions.
async fn no_stream() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}

/// Whether a request's `Origin`, if it has one, is this machine.
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let host_of = |authority: &str| -> String {
        let authority = authority.trim();
        if let Some(rest) = authority.strip_prefix('[') {
            return rest.split(']').next().unwrap_or("").to_ascii_lowercase();
        }
        authority
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(authority)
            .to_ascii_lowercase()
    };
    let origin_host = host_of(origin.split("://").nth(1).unwrap_or(origin));
    if matches!(origin_host.as_str(), "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|host| host_of(host) == origin_host)
        .unwrap_or(false)
}

fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

async fn handle(State(state): State<McpState>, headers: HeaderMap, body: String) -> Response {
    if !origin_allowed(&headers) {
        return (StatusCode::FORBIDDEN, "Origin not allowed").into_response();
    }
    if let Some(auth) = &state.auth {
        if crate::api::auth::identify(auth, &headers).is_none() {
            let mut response = (
                StatusCode::UNAUTHORIZED,
                "MythOS requires an access token: create one in Settings > Account and send it as \
                 'Authorization: Bearer <token>'",
            )
                .into_response();
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, "Bearer".parse().unwrap());
            return response;
        }
    }
    let credentials: Vec<(header::HeaderName, header::HeaderValue)> = CREDENTIAL_HEADERS
        .iter()
        .filter_map(|name| headers.get(name).map(|v| (name.clone(), v.clone())))
        .collect();
    let message: Value = match serde_json::from_str(&body) {
        Ok(message) => message,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(rpc_error(Value::Null, -32700, format!("parse error: {e}"))),
            )
                .into_response()
        }
    };

    match message {
        Value::Array(messages) => {
            let mut answers = Vec::new();
            for message in messages {
                if let Some(answer) = answer(&state, &credentials, message).await {
                    answers.push(answer);
                }
            }
            if answers.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                Json(Value::Array(answers)).into_response()
            }
        }
        message => match answer(&state, &credentials, message).await {
            Some(answer) => Json(answer).into_response(),
            // A notification or a response: accepted, nothing to say.
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

/// The answer to one JSON-RPC message, or `None` for a notification or a response.
async fn answer(
    state: &McpState,
    credentials: &[(header::HeaderName, header::HeaderValue)],
    message: Value,
) -> Option<Value> {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to something this server asked; it asks nothing.
        return None;
    };
    let id = message.get("id").cloned()?;
    let params = message.get("params").cloned().unwrap_or(json!({}));

    Some(match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str);
            let version = requested
                .filter(|v| PROTOCOL_VERSIONS.contains(v))
                .unwrap_or(PROTOCOL_VERSIONS[0]);
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": "mythos",
                        "title": "MythOS",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": "MythOS is a SCADA controller. Projects hold field I/O \
                        configurations, HMI screens, data types and faceplates; one project is \
                        active and its connections feed the tags. Only a tag whose quality_name is \
                        GOOD has a current value: never report a value with any other quality as \
                        a measurement. Saving an I/O configuration or activating a project changes \
                        what the controller reads at once."
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(
            id,
            json!({ "tools": TOOLS.iter().map(Tool::describe).collect::<Vec<_>>() }),
        ),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "tools/call needs a tool 'name'"));
            };
            let Some(tool) = TOOLS.iter().find(|t| t.name == name) else {
                return Some(rpc_error(id, -32602, format!("unknown tool '{name}'")));
            };
            let arguments = match params.get("arguments") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(map)) => map.clone(),
                Some(_) => return Some(rpc_error(id, -32602, "'arguments' must be an object")),
            };
            rpc_result(id, call(state, credentials, tool, &arguments).await)
        }
        other => rpc_error(id, -32601, format!("method '{other}' is not supported")),
    })
}

/// Run a tool: send its request through the API router and report what the route answered.
async fn call(
    state: &McpState,
    credentials: &[(header::HeaderName, header::HeaderValue)],
    tool: &Tool,
    arguments: &Map<String, Value>,
) -> Value {
    let mut request = match tool.request(arguments) {
        Ok(request) => request,
        Err(message) => {
            return json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true
            })
        }
    };
    for (name, value) in credentials {
        request.headers_mut().insert(name.clone(), value.clone());
    }
    let response = match state.api.clone().oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    let structured = json!({ "status": status.as_u16(), "body": body });
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&structured).unwrap_or_default()
        }],
        "structuredContent": structured,
        "isError": !status.is_success()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_unique_and_every_route_appears_once() {
        let mut names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "two tools share a name");

        let mut routes: Vec<(&str, &str)> = TOOLS
            .iter()
            .map(|t| (t.method, t.path))
            .chain(NOT_TOOLS.iter().map(|n| (n.method, n.path)))
            .collect();
        routes.sort();
        let before = routes.len();
        routes.dedup();
        assert_eq!(routes.len(), before, "a route is listed twice");
    }

    #[test]
    fn a_path_parameter_is_encoded_so_a_tag_uri_stays_one_segment() {
        let tool = TOOLS.iter().find(|t| t.name == "get_tag").unwrap();
        let mut arguments = Map::new();
        arguments.insert("uri".into(), json!("external.A.B.c/../x y"));
        let request = tool.request(&arguments).unwrap();
        assert_eq!(
            request.uri().to_string(),
            "/api/v1/tags/external.A.B.c%2F..%2Fx%20y"
        );
    }

    #[test]
    fn missing_and_unknown_arguments_are_refused() {
        let tool = TOOLS.iter().find(|t| t.name == "get_project").unwrap();
        assert!(tool
            .request(&Map::new())
            .unwrap_err()
            .contains("project_id"));
        let mut arguments = Map::new();
        arguments.insert("project_id".into(), json!("p"));
        arguments.insert("nonsense".into(), json!(1));
        assert!(tool.request(&arguments).unwrap_err().contains("nonsense"));
    }

    #[test]
    fn origins_other_than_this_host_are_refused() {
        let with = |origin: Option<&str>, host: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, host.parse().unwrap());
            if let Some(origin) = origin {
                headers.insert(header::ORIGIN, origin.parse().unwrap());
            }
            origin_allowed(&headers)
        };
        assert!(with(None, "10.0.0.5:8080"));
        assert!(with(Some("http://localhost:5173"), "127.0.0.1:8080"));
        assert!(with(Some("http://127.0.0.1:3000"), "127.0.0.1:8080"));
        assert!(with(Some("http://[::1]:3000"), "[::1]:8080"));
        assert!(with(Some("http://10.0.0.5:8080"), "10.0.0.5:8080"));
        assert!(!with(Some("http://evil.example"), "127.0.0.1:8080"));
        assert!(!with(Some("http://10.0.0.5.evil.example"), "10.0.0.5:8080"));
    }
}
