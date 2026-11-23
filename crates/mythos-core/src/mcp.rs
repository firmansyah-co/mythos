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

/// Why the routes that forward to service modules are not tools themselves.
const MODULE_ROUTES: &str =
    "forwards to a service module; each route a module declares is its own tool, listed by tools/list";

/// Routes that are not tools.
pub const NOT_TOOLS: &[NotATool] = &[
    NotATool {
        method: "PUT",
        path: "/api/v1/slices/:project",
        reason: "called by the engineering node downloading a device here; download_device is the tool",
    },
    NotATool {
        method: "GET",
        path: "/api/v1/slices/:project",
        reason: "called by the engineering node comparing a device; compare_device is the tool",
    },
    NotATool {
        method: "POST",
        path: "/api/v1/callhome",
        reason: "called by a paired node calling this one home, as itself; list_fleet shows which do",
    },
    NotATool {
        method: "POST",
        path: "/api/v1/pairing/accept",
        reason: "called by another node pairing with this one, with the code it was given; pair_with_node is the tool",
    },
    NotATool {
        method: "DELETE",
        path: "/api/v1/pairing/peers/:node",
        reason: "called by a paired node to say it unpaired; unpair_node is the tool",
    },
    NotATool {
        method: "GET",
        path: "/api/v1/telemetry/stream",
        reason:
            "an endless Server-Sent Events stream; read_tag_values returns the same reading once",
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
        path: "/api/v1/projects/:id/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "POST",
        path: "/api/v1/projects/:id/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "PUT",
        path: "/api/v1/projects/:id/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "DELETE",
        path: "/api/v1/projects/:id/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "GET",
        path: "/api/v1/m/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "POST",
        path: "/api/v1/m/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "PUT",
        path: "/api/v1/m/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "DELETE",
        path: "/api/v1/m/:module/*rest",
        reason: MODULE_ROUTES,
    },
    NotATool {
        method: "POST",
        path: "/api/v1/modules/install",
        reason: "takes the package file itself as the request body; install_module_from_url installs one from where it is published",
    },
    NotATool {
        method: "GET",
        path: "/modules/:module/ui/*path",
        reason: "the files of a module's workbench UI, loaded by the browser",
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
    ("module", "module_id"),
    ("runtime", "runtime_id"),
    ("node", "node_id"),
    ("device", "device_id"),
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

fn plant_device_body() -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": { "type": "string" },
            "node": { "type": ["string", "null"], "description": "This node's id or a paired node's id; null for none yet" }
        }
    })
}

fn confirm_body() -> Value {
    json!({
        "type": "object",
        "required": ["confirm"],
        "properties": { "confirm": { "type": "boolean", "description": "true: this changes what a running controller does, and it is meant" } }
    })
}

fn pairing_code_body() -> Value {
    json!({
        "type": "object",
        "properties": {
            "role": { "type": "string", "enum": ROLES, "description": "What the node that pairs may do here; engineer when absent" }
        }
    })
}

fn pair_body() -> Value {
    json!({
        "type": "object",
        "required": ["address", "fingerprint", "code"],
        "properties": {
            "address": { "type": "string", "description": "host:port where the node answers HTTPS" },
            "fingerprint": { "type": "string", "description": "SHA-256 of its certificate, 64 hex characters (spaces allowed)" },
            "code": { "type": "string", "description": "The pairing code made on that node" },
            "grant": { "type": "string", "enum": ROLES, "description": "What that node may do here; viewer when absent" },
            "call_home": { "type": "boolean", "description": "Call that node home: this node opens the connection and is reached over it, for when that node cannot reach this one (another subnet, NAT)" }
        }
    })
}

fn call_home_body() -> Value {
    json!({
        "type": "object",
        "required": ["call_home"],
        "properties": {
            "call_home": { "type": "boolean", "description": "Call that node home (this node opens the connection and is reached over it), or stop" }
        }
    })
}

fn node_body() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "maxLength": 64 },
            "site": { "type": "string", "maxLength": 128 },
            "role": { "type": "string", "enum": ["workstation", "edge", "server"] },
            "field_io": { "type": ["boolean", "null"], "description": "Read field devices whatever the role says; null follows the role (a server reads none). Applies at once." },
            "announce": { "type": ["boolean", "null"], "description": "Announce this node on the network whatever the role says; null follows the role (only an edge does)." },
            "http_on_network": { "type": "boolean", "description": "Also answer plain HTTP on the network (off: HTTP on this computer only, HTTPS on the network). Applies when the server next starts." }
        }
    })
}

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

fn write_tag_body() -> Value {
    json!({
        "type": "object",
        "required": ["value"],
        "properties": {
            "value": {
                "type": ["number", "boolean"],
                "description": "The value in engineering units, as the tag is read"
            }
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

fn follow_body() -> Value {
    json!({
        "type": "object",
        "required": ["channel"],
        "properties": {
            "channel": { "type": "string", "enum": ["release", "rc", "dev"],
                         "description": "release: releases only; rc: also release candidates; dev: everything" },
            "pinned_version": { "type": ["string", "null"],
                                "description": "Stay on this version instead of following the channel; null to follow it" }
        }
    })
}

fn install_body() -> Value {
    json!({
        "type": "object",
        "properties": {
            "version": { "type": "string",
                         "description": "The version to install; absent installs the pinned version, or the newest on the channel" }
        }
    })
}

fn module_enabled_body() -> Value {
    json!({
        "type": "object",
        "required": ["enabled"],
        "properties": { "enabled": { "type": "boolean" } }
    })
}

fn install_url_body() -> Value {
    json!({
        "type": "object",
        "required": ["url", "sha256"],
        "properties": {
            "url": { "type": "string", "description": "Where the package (a zip) is published" },
            "sha256": { "type": "string", "description": "The SHA-256 the package must have" }
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

/// The data type routes take the definition itself as the body.
fn datatype_body() -> Value {
    udt_schema()
}

fn watch_body() -> Value {
    json!({
        "type": "object",
        "required": ["variables"],
        "properties": {
            "variables": {
                "type": "array",
                "minItems": 1,
                "maxItems": crate::runtimes::WATCH_LIMIT,
                "items": { "type": "string" },
                "description": "Variable names as the program declares them: level, pump.running, hist[2]. In a runtime of several programs, the program first: tank.level, tank.hist[2]"
            }
        }
    })
}

fn force_body() -> Value {
    json!({
        "type": "object",
        "required": ["variable", "value"],
        "properties": {
            "variable": {
                "type": "string",
                "description": "A variable as the program declares it: level, pump.running, hist[2]. In a runtime of several programs, the program first: tank.level"
            },
            "value": {
                "type": ["number", "boolean"],
                "description": "What to hold it at: a boolean or 0/1 for a BOOL, a number in the variable's range otherwise"
            }
        }
    })
}

fn release_body() -> Value {
    json!({
        "type": "object",
        "properties": {
            "variable": {
                "type": "string",
                "description": "The forced variable to release, named as it was forced; leave it out to release every one"
            }
        }
    })
}

/// A task's kind: one object per type, each with the fields that type takes and no other.
fn task_kind() -> Value {
    let kind = |name: &str, required: &[&str], fields: Value| {
        let mut properties = json!({ "type": { "type": "string", "const": name } });
        if let (Some(p), Value::Object(f)) = (properties.as_object_mut(), fields) {
            p.extend(f);
        }
        let mut required: Vec<&str> = required.to_vec();
        required.insert(0, "type");
        json!({
            "type": "object",
            "required": required,
            "additionalProperties": false,
            "properties": properties
        })
    };
    let ms = |minimum: u64| json!({ "type": "integer", "minimum": minimum });
    json!({
        "type": "object",
        "description": "cyclic: every interval_ms, first phase_ms after RUN; cycle: again as soon as it ends, no sooner than min_cycle_ms after it last began; startup: once at every change to RUN, before any other task; event: at each rising edge of the BOOL tag, which a program of the runtime reads",
        "oneOf": [
            kind("cyclic", &["interval_ms"], json!({ "interval_ms": ms(1), "phase_ms": ms(0) })),
            kind("cycle", &[], json!({ "min_cycle_ms": ms(0) })),
            kind("startup", &[], json!({})),
            kind("event", &["tag"], json!({ "tag": { "type": "string", "minLength": 1 } }))
        ]
    })
}

/// A data block as a program uses it (`program.data[]`), and as a runtime holds it
/// (`data[]`): the compiler's JSON, with no field the route does not take.
fn data_schemas() -> (Value, Value) {
    let member = |written: bool| {
        let mut properties = json!({
            "name": { "type": "string" },
            "type": { "type": "string" },
            "offset": { "type": "integer", "minimum": 0, "description": "Inside the block, in bytes" },
            "size": { "type": "integer", "minimum": 0 }
        });
        let mut required = vec!["name", "type", "offset", "size"];
        if written {
            properties["written"] = json!({ "type": "boolean", "description": "Whether the program's code writes it, as the compiler says; a member has one writer in a runtime. Never change it: a program whose code writes a member its data says it does not write is faulted at the end of its first task run" });
            required.push("written");
        }
        json!({ "type": "object", "required": required, "additionalProperties": false, "properties": properties })
    };
    let layout = json!({ "type": "string", "pattern": "^[0-9a-f]{16}$", "description": "The layout hash the compiler gives: 16 lowercase hex digits" });
    let used = json!({
        "type": "object",
        "required": ["name", "type", "layout", "offset", "size", "constant", "members"],
        "additionalProperties": false,
        "description": "A data block the program declares in VAR_EXTERNAL, exactly as the logic compiler gives it in the compiled program's data; no route of this server returns it. Required for every block the code declares: the runtime module refuses a program whose symbols have a data block, or a member of one, that its data leaves out",
        "properties": {
            "name": { "type": "string", "description": "A block of the runtime's data" },
            "type": { "type": "string" },
            "layout": layout.clone(),
            "offset": { "type": "integer", "minimum": 0, "description": "Of the program's copy, in its memory" },
            "size": { "type": "integer", "minimum": 0 },
            "constant": { "type": "boolean", "description": "Declared VAR_EXTERNAL CONSTANT: it writes no member" },
            "members": { "type": "array", "items": member(true) }
        }
    });
    let held = json!({
        "type": "object",
        "required": ["name", "type", "layout", "size", "members"],
        "additionalProperties": false,
        "description": "A data block the runtime's programs share: one master copy, each member handed from its one writer to the programs that read it",
        "properties": {
            "name": { "type": "string", "description": "A letter or _ first, then letters, digits or _; not a program's name; in one runtime only" },
            "type": { "type": "string" },
            "layout": layout,
            "size": { "type": "integer", "minimum": 0 },
            "members": { "type": "array", "items": member(false), "description": "Its top-level members: the members of each program's data for this block without their written, which this list does not take; no two may share a byte" },
            "retain": { "type": "boolean", "description": "Whether its value is kept across a warm restart; left out, it is not" }
        }
    });
    (used, held)
}

fn runtimes_config_body() -> Value {
    let (used, held) = data_schemas();
    let program = json!({
        "type": "object",
        "required": ["name", "version", "interval_ms", "code"],
        "properties": {
            "name": { "type": "string", "description": "In a runtime of several programs, no '.', '[', ']' or spaces" },
            "built_from": { "type": "string", "description": "The project program this is a build of, as the logic module names it in its builds; left out of an instance written by hand. Not blank, and one program of one built_from per runtime" },
            "created": { "type": "string", "description": "The making of that project program, a mark the logic module gives it when it is made; left out of an instance written by hand. A program deleted and made again under its name has another, and is another program" },
            "version": { "type": "string" },
            "interval_ms": { "type": "integer", "minimum": 0, "description": "The interval of its implied cyclic task, at least 1 when no tasks are given; not used when tasks are given" },
            "code": { "type": "string", "description": "The compiled program, hex" },
            "symbols": { "description": "The compiler's symbol table" },
            "inputs": { "type": "array", "items": { "type": "object", "required": ["variable", "tag"], "properties": {
                "variable": { "type": "string" }, "tag": { "type": "string" },
                "of": { "type": "string", "enum": ["value", "good"], "description": "value (default), or good: a BOOL that says whether the tag is current" } } } },
            "outputs": { "type": "array", "items": { "type": "object", "required": ["variable", "tag", "safe"], "properties": {
                "variable": { "type": "string" }, "tag": { "type": "string" },
                "safe": { "type": ["number", "boolean"], "description": "What the tag is written whenever the program is not running" } } } },
            "published": { "type": "array", "items": { "type": "object", "required": ["variable", "tag"], "properties": {
                "variable": { "type": "string" }, "tag": { "type": "string", "description": "Starts with internal." },
                "writable": { "type": "boolean" } } } },
            "data": { "type": "array", "items": used, "description": "The data blocks it declares in VAR_EXTERNAL; left out, none" }
        }
    });
    let task = json!({
        "type": "object",
        "required": ["name", "kind", "priority", "programs"],
        "additionalProperties": false,
        "properties": {
            "name": { "type": "string", "description": "Letters, digits, - and _" },
            "kind": task_kind(),
            "priority": { "type": "integer", "minimum": 0, "maximum": 31, "description": "0 is the highest" },
            "watchdog_ms": { "type": "integer", "minimum": 1, "description": "How long it may owe a run before the runtime is put in fault; left out, ten intervals and at least 2 s for a cyclic or cycle task, 2 s for the others" },
            "programs": { "type": "array", "items": { "type": "string" }, "description": "Names of the runtime's programs, in the order they run; each program is in exactly one task" }
        }
    });
    json!({
        "type": "object",
        "required": ["instances"],
        "description": "The whole runtimes configuration; it replaces the saved one.",
        "properties": {
            "instances": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["id", "module", "run"],
                    "description": "One runtime: program, or programs with tasks; never both; data: the data blocks its programs share.",
                    "properties": {
                        "id": { "type": "string", "description": "Letters, digits, - and _" },
                        "module": { "type": "string", "description": "A runtime module's 'provides', for example vm" },
                        "run": { "type": "boolean", "description": "Whether it is put in RUN when loaded" },
                        "program": program,
                        "programs": { "type": "array", "items": program, "description": "Several programs in one runtime module process" },
                        "tasks": { "type": "array", "items": task, "description": "The tasks that run the programs; left out, one program runs cyclic at its interval_ms" },
                        "data": { "type": "array", "items": held, "description": "The data blocks its programs share; left out, none" }
                    }
                }
            }
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
        name: "get_node",
        title: "This node",
        description: "This MythOS installation as a node: its id (never changes), name, site, role (workstation, edge or server), version, the module protocol it speaks, where it listens, and whether a restart is needed for the role's listening address to apply.",
        method: "GET",
        path: "/api/v1/node",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "update_node",
        title: "Rename this node or change its role",
        description: "Change this node's name, site, role, or whether it reads field devices or announces itself. A workstation listens on its own computer only, an edge or a server on every interface; a new listening address applies when the server next starts. A server reads no field devices unless field_io is set.",
        method: "PUT",
        path: "/api/v1/node",
        query: &[],
        body: Some(node_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "get_plant",
        title: "The devices of a plant",
        description: "A plant project's devices: each one's name, the project holding its configuration, and the node it is assigned to. A project with no plant of its own is one device, this controller (implicit: true).",
        method: "GET",
        path: "/api/v1/projects/:id/plant",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "set_plant_device",
        title: "Add or change a device",
        description: "Add a device to a plant (it gets a project of its own, <plant>--<device>), or rename it, or assign it to a node: this node's id or a paired node's (list_pairings), or null.",
        method: "PUT",
        path: "/api/v1/projects/:id/plant/devices/:device",
        query: &[],
        body: Some(plant_device_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "remove_plant_device",
        title: "Remove a device",
        description: "Take a device out of a plant. Its project is kept.",
        method: "DELETE",
        path: "/api/v1/projects/:id/plant/devices/:device",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "compare_device",
        title: "Compare a device with its node",
        description: "Whether the node a device is assigned to holds and runs exactly what the project says (same), something else (differs), holds it without running it (not_running), never got it (never_downloaded), or cannot be reached (unreachable, never reported as the same).",
        method: "GET",
        path: "/api/v1/projects/:id/plant/devices/:device/compare",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "download_device",
        title: "Download a device to its node",
        description: "Send a device's configuration to the node it is assigned to and make it what that node runs; on failure the node runs what it ran before. Changes what a running controller reads and shows: confirm must be true.",
        method: "POST",
        path: "/api/v1/projects/:id/plant/devices/:device/download",
        query: &[],
        body: Some(confirm_body),
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "download_plant",
        title: "Download every device",
        description: "Download every assigned device of a plant, each reported: done, refused (and why), unreachable, or skipped (assigned to no node). confirm must be true.",
        method: "POST",
        path: "/api/v1/projects/:id/plant/download",
        query: &[],
        body: Some(confirm_body),
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "create_pairing_code",
        title: "Make a pairing code",
        description: "Make a one-use code, valid ten minutes, that another node pairs with this one by; the node that uses it may act here with the role given (an engineer when absent). Give the code to whoever pairs, with this node's certificate fingerprint (get_node) to compare.",
        method: "POST",
        path: "/api/v1/pairing/code",
        query: &[],
        body: Some(pairing_code_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "list_pairings",
        title: "Paired nodes",
        description: "The nodes this one is paired with: id, name, role, address, the certificate fingerprint pinned, the role each may act with here. Never a token.",
        method: "GET",
        path: "/api/v1/pairings",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "pair_with_node",
        title: "Pair with a node",
        description: "Pair with the node at an address (host:https-port), which must present the certificate with the fingerprint given (compare it with the one that node shows; a node heard by list_network_nodes announces it, but heard is not trusted), using a pairing code made on it.",
        method: "POST",
        path: "/api/v1/pairings",
        query: &[],
        body: Some(pair_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "set_call_home",
        title: "Call a paired node home",
        description: "Call a paired node home, or stop: this node opens the connection to it and is reached over that, for when that node cannot reach this one (another subnet, NAT). Set on the node that cannot be reached.",
        method: "PUT",
        path: "/api/v1/pairings/:node",
        query: &[],
        body: Some(call_home_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "list_fleet",
        title: "List the fleet",
        description: "Every paired node, asked now: how it is reached (direct, or calling home), whether it answered and why not, its name, role, site and version, and what it runs (project and slice hash).",
        method: "GET",
        path: "/api/v1/fleet",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "paired_node_status",
        title: "Reach a paired node",
        description: "Ask a paired node who it is, over its pinned certificate and as this node: whether it can be reached, and what it says.",
        method: "GET",
        path: "/api/v1/pairings/:node/status",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "unpair_node",
        title: "Unpair a node",
        description: "Forget a paired node (its token is refused at once) and tell it, so it forgets this node too.",
        method: "DELETE",
        path: "/api/v1/pairings/:node",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "get_update_status",
        title: "Version and installation",
        description: "The version this server runs, the installation it can update (installed versions, the current and previous one, the channel followed or the pinned version), or why it has none.",
        method: "GET",
        path: "/api/v1/updates",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "list_releases",
        title: "Published releases",
        description: "Every release the release feed publishes: version, channel, title, release notes, when published, whether it has a bundle for this platform, whether it is installed or current, and which one the followed channel or pin would install. A feed that cannot be read is an error, never a list.",
        method: "GET",
        path: "/api/v1/updates/releases",
        query: &[("refresh", "true to ask the feed again instead of using the last reading (kept 5 minutes)")],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "set_update_follow",
        title: "Follow a channel or a version",
        description: "Choose what the installation follows: a channel (release, rc, dev), or one pinned version. Administrators only.",
        method: "PUT",
        path: "/api/v1/updates/follow",
        query: &[],
        body: Some(follow_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "install_update",
        title: "Install a release",
        description: "Download a release's bundle for this platform, check its SHA-256, check that it runs, and make it the current version. The running server keeps its version until restarted ('restart_required'). Administrators only.",
        method: "POST",
        path: "/api/v1/updates/install",
        query: &[],
        body: Some(install_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "rollback_update",
        title: "Roll back",
        description: "Make the previously current version current again; it is still installed, so nothing is downloaded. Administrators only.",
        method: "POST",
        path: "/api/v1/updates/rollback",
        query: &[],
        body: None,
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "set_module_enabled",
        title: "Switch a module on or off",
        description: "Disable or enable a module by its manifest id. A disabled driver's connections stop with CONFIG_ERROR naming the reason; a disabled service answers nothing. Administrators only.",
        method: "PUT",
        path: "/api/v1/modules/:module/enabled",
        query: &[],
        body: Some(module_enabled_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "install_module_from_url",
        title: "Install a module package",
        description: "Download a module package from a URL, check that its SHA-256 is the one given, check every file against its checksums and that its executable for this platform runs and is the module its manifest says, then install it and start using it. Administrators only.",
        method: "POST",
        path: "/api/v1/modules/install-url",
        query: &[],
        body: Some(install_url_body),
        read_only: false,
        destructive: false,
    },
    Tool {
        name: "remove_module",
        title: "Remove an installed module",
        description: "Stop a module installed from a package and remove its executable and UI. A module that is part of the MythOS installation cannot be removed, only disabled. Administrators only.",
        method: "DELETE",
        path: "/api/v1/modules/:module",
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
        name: "list_remote_devices",
        title: "List the plant's other devices",
        description: "The other devices of the plant this node runs whose tags are read here over the pairing, each with its link: connected, connecting or lost (and why). Their tags appear in read_tag_values named with the device, as edge1:external.Boiler.U1.p; while a link is lost they are BAD_COMM_FAILURE with no value.",
        method: "GET",
        path: "/api/v1/remote",
        query: &[],
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
        name: "write_tag",
        title: "Write a value to a field device",
        description: "Write a value to the field device behind a tag, as an operator. Only a tag whose access_mode is READ_WRITE can be written. The answer's 'written' is true when the device confirmed, false when nothing reached it, and \"unknown\" when a request went out and no confirmation came back. The tag's value does not change with the write: read it again after the next poll to see what the device now holds.",
        method: "PUT",
        path: "/api/v1/tags/:uri/value",
        query: &[],
        body: Some(write_tag_body),
        read_only: false,
        destructive: true,
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
    Tool {
        name: "list_runtimes",
        title: "List the runtimes of this node",
        description: "Every program this node executes: its mode (run, stop, fault, or unreachable when its module does not answer) and why, scan count and times (null when nothing is known: the module does not answer, or no scan has run; never 0 in their place), and each output with what the device last confirmed and whether it is at its safe value. A runtime with tasks written out also lists its programs with their versions and each task with its runs, overruns, preemptions and run times; its interval_ms is that of its fastest cyclic task, null when it has none. A runtime with data blocks lists them in data, each member with its writer: the program that writes it, null when none does. Also the runtime modules found, each with the target profile a program is compiled for.",
        method: "GET",
        path: "/api/v1/runtimes",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "run_runtime",
        title: "Put a program in RUN",
        description: "Put a loaded program in RUN. From stop or fault this is a warm restart: its RETAIN variables keep the values they had, the others take their initial values (cold_start_runtime starts from initial values only). It takes effect at the runtime's next exchange; read list_runtimes to see.",
        method: "POST",
        path: "/api/v1/runtimes/:runtime/run",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "cold_start_runtime",
        title: "Start a program again from its initial values",
        description: "A cold restart: every variable of the program takes its initial value, RETAIN variables included (the values kept for them are replaced), and the program runs. A program in RUN is restarted too, so its outputs may change at once. It takes effect at the runtime's next exchange; read list_runtimes to see.",
        method: "POST",
        path: "/api/v1/runtimes/:runtime/cold-start",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "stop_runtime",
        title: "Put a program in STOP",
        description: "Stop a program. Its outputs are written their safe values and its published tags stop being current.",
        method: "POST",
        path: "/api/v1/runtimes/:runtime/stop",
        query: &[],
        body: None,
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "watch_runtime",
        title: "Read a program's variables",
        description: "The values of a running program's variables as its last scan left them: any variable, bound to a tag or not, and one element of an array as name[2] or name[1,3]. It changes nothing in the program. Each value comes with its type; a variable the program does not have, or one with no single value (a whole array, a structure), has an error instead. 'state' says which program and version answered and whether it is in run: values read from a stopped or faulted program are what it stopped with. 'age_ms' is how long ago the runtime was read, and 'state.scan_age_us' how long before that the program last completed a scan (absent before its first scan, from a module that does not say, and from a runtime of several tasks, whose state.tasks give each task's run_age_us instead): a program that has stopped scanning still answers in run, with its last values and a scan age that grows, so values are current only while the scan age is within two of the program's intervals (list_runtimes: interval_ms) plus its longest scan (state.max_scan_us). 'fresh_ms' is how old an answer may be and still be given at once. The first call waits for the runtime's next exchange; calls within a few seconds of each other answer at once.",
        method: "POST",
        path: "/api/v1/runtimes/:runtime/watch",
        query: &[],
        body: Some(watch_body),
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "force_runtime_variable",
        title: "Force a variable of a running program",
        description: "Hold a variable of a running program at a value (a BOOL or a number; one element of an array as name[2]). Whatever its inputs and the program say, it reads and is written as that value: an output bound to it is written it to the device, a published tag gives it, and an operator's setpoint to it is refused. It holds until released (release_runtime_forces), until the program leaves run (stopped or faulted), or until a program is loaded again. A program that is not in run is refused. The answer's state.forced lists every forced variable; list_runtimes shows them too, with forced_tags: every tag bound to a forced variable. It is carried out at the runtime's next exchange; an answer of 504 means it was not carried out.",
        method: "POST",
        path: "/api/v1/runtimes/:runtime/force",
        query: &[],
        body: Some(force_body),
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "release_runtime_forces",
        title: "Release forced variables of a program",
        description: "Release one forced variable (body.variable), or every forced variable of the program (no variable). The program decides the value again from its next scan, so an output bound to it may change at once. A variable that is not forced is refused.",
        method: "POST",
        path: "/api/v1/runtimes/:runtime/release",
        query: &[],
        body: Some(release_body),
        read_only: false,
        destructive: true,
    },
    Tool {
        name: "get_runtimes_config",
        title: "Get a project's runtimes",
        description: "The programs a project is configured to run, with their bindings to tags, or null.",
        method: "GET",
        path: "/api/v1/projects/:id/runtimes",
        query: &[],
        body: None,
        read_only: true,
        destructive: false,
    },
    Tool {
        name: "save_runtimes_config",
        title: "Save a project's runtimes",
        description: "Check and save the programs a project runs. Each instance names a runtime module (list_runtimes shows them), whether to run at once, and a program: its code and symbols as the logic compiler made them, its data (each data block its code declares in VAR_EXTERNAL, as the compiler gave it; required when it declares any), the tags read into variables (inputs), the variables written to field tags with the safe value each tag takes whenever the program is not running (outputs), and the variables published as tags starting with internal. (published; writable makes one a setpoint an operator may write). A runtime of several programs gives them in programs instead of program, and tasks that say which task runs each (cyclic, cycle, startup or event, at a priority from 0, the highest, to 31); its variables are then named program.variable. A runtime module that does not say it runs tasks is refused such a runtime. An instance's data lists the data blocks its programs share: each block in one runtime only, each member written by one program at most, no tag read into a member and no member made a setpoint; a runtime module that does not say it hands data blocks over is refused a runtime with data. The layout hash, offsets and written flags of a block come only from the logic compiler: no route of this server returns them. The module loads each program to check it; anything that does not load is refused and nothing is saved. For the active project the runtimes are then made to match: 'applied' true says the configuration was put in force, not that every program started. 'changes.failed' lists each instance that did not start, with why; read list_runtimes for what runs. A new build of the same programs of the same project in run replaces it online (the same programs: by built_from and created when every program of both gives built_from, else by PROGRAM name, so a running instance that names no project program is replaced online by a build of any program of the same PROGRAM name; never a build of a project made active since the running one was started): it takes over every variable of the same name, type and size (RETAIN or not) and scans on, its outputs never going to their safe values; start=cold starts every program it starts or replaces from its initial values instead, RETAIN ones included. 'changes.runtimes' says for each how it started: start online (with carried, the count, and dropped and initialized, the variables that did not carry over and why), warm (RETAIN values kept, the rest initial; why says why not online) or cold. Forces are released by a load either way. A program may name the project program it is a build of (built_from, given by the logic module's builds): two instances holding programs of one built_from are refused, and nothing is saved. RETAIN values are kept per runtime id and per program: a start gives a program only the values kept for it (by built_from and created when both name one, else by program name; a program made again under its name is another program), and none when values kept for two programs could be it; 'changes.runtimes' lists in not_given each kept value it did not give, and why. The one exception is a retain file written before values were kept per program (a JSON array): it is given once, as it is, to whatever the runtime runs at its next warm start, not_given then empty, and written again per program.",
        method: "PUT",
        path: "/api/v1/projects/:id/runtimes",
        query: &[(
            "start",
            "cold to start every program this starts or replaces from its initial values, RETAIN variables included; leave it out to change running programs online",
        )],
        body: Some(runtimes_config_body),
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
    services: Option<Arc<crate::services::ServiceHost>>,
}

/// A route a service module declares, as a tool (ADR-011 §2.2): its arguments are the project
/// for a project route, the route's path parameters, its query parameters and its body.
struct ModuleTool {
    provides: String,
    version: String,
    decl: mythos_module::protocol::RouteDecl,
}

impl ModuleTool {
    fn all(services: &crate::services::ServiceHost) -> Vec<ModuleTool> {
        let mut tools = Vec::new();
        for service in services.all() {
            let manifest = &service.module.manifest;
            for decl in &manifest.routes {
                if TOOLS.iter().any(|t| t.name == decl.tool) {
                    tracing::warn!(
                        "module '{}' declares tool '{}', which a core route already has; it is left out",
                        manifest.provides,
                        decl.tool
                    );
                    continue;
                }
                tools.push(ModuleTool {
                    provides: manifest.provides.clone(),
                    version: manifest.version.clone(),
                    decl: decl.clone(),
                });
            }
        }
        tools
    }

    fn project_scoped(&self) -> bool {
        self.decl.scope == mythos_module::protocol::RouteScope::Project
    }

    fn describe(&self) -> Value {
        let mut properties = Map::new();
        let mut required: Vec<Value> = Vec::new();
        if self.project_scoped() {
            properties.insert("project_id".into(), json!({ "type": "string" }));
            required.push(json!("project_id"));
        }
        // Path parameters are named as the core's tools name them (`:screen` is `screen_id`), so
        // a route that moves from the core into a module keeps its tool's arguments.
        for param in path_params(&self.decl.path) {
            let name = argument_name(param);
            properties.insert(name.to_string(), json!({ "type": "string" }));
            required.push(json!(name));
        }
        for (name, description) in &self.decl.query {
            properties.insert(
                name.clone(),
                json!({ "type": "string", "description": description }),
            );
        }
        if let Some(body) = &self.decl.body {
            properties.insert("body".into(), body.clone());
            required.push(json!("body"));
        }
        let role = crate::services::declared_role(&self.decl);
        let read_only = self.decl.method.eq_ignore_ascii_case("GET");
        json!({
            "name": self.decl.tool,
            "title": self.decl.summary,
            "description": format!(
                "{} (module '{}' {}; needs the {role} role)",
                self.decl.summary, self.provides, self.version
            ),
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            },
            "annotations": {
                "title": self.decl.summary,
                "readOnlyHint": read_only,
                "destructiveHint": self.decl.method.eq_ignore_ascii_case("DELETE"),
                "idempotentHint": read_only || self.decl.method.eq_ignore_ascii_case("PUT"),
                "openWorldHint": false
            }
        })
    }

    fn request(&self, arguments: &Map<String, Value>) -> Result<Request<Body>, String> {
        let text = |name: &str| -> Result<String, String> {
            match arguments.get(name) {
                Some(Value::String(s)) => Ok(s.clone()),
                Some(other) => Ok(other.to_string()),
                None => Err(format!("'{name}' is required")),
            }
        };
        let mut path = if self.project_scoped() {
            format!(
                "/api/v1/projects/{}/{}",
                percent_encode(&text("project_id")?),
                self.provides
            )
        } else {
            format!("/api/v1/m/{}", self.provides)
        };
        for segment in self.decl.path.split('/').skip(1) {
            path.push('/');
            match segment.strip_prefix(':') {
                Some(param) => path.push_str(&percent_encode(&text(argument_name(param))?)),
                None => path.push_str(segment),
            }
        }
        let query: Vec<String> = self
            .decl
            .query
            .iter()
            .filter_map(|(name, _)| {
                arguments.get(name).map(|v| {
                    let v = v
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string());
                    format!("{}={}", percent_encode(name), percent_encode(&v))
                })
            })
            .collect();
        if !query.is_empty() {
            path.push('?');
            path.push_str(&query.join("&"));
        }
        let known = |key: &str| {
            key == "body"
                || (key == "project_id" && self.project_scoped())
                || self.decl.query.iter().any(|(q, _)| q == key)
                || path_params(&self.decl.path)
                    .iter()
                    .any(|param| argument_name(param) == key)
        };
        if let Some(unknown) = arguments.keys().find(|k| !known(k)) {
            return Err(format!(
                "'{unknown}' is not an argument of {}",
                self.decl.tool
            ));
        }
        let method = Method::from_bytes(self.decl.method.to_ascii_uppercase().as_bytes())
            .map_err(|e| e.to_string())?;
        let builder = Request::builder().method(method).uri(path);
        let request = match &self.decl.body {
            Some(_) => {
                let body = arguments.get("body").ok_or("'body' is required")?;
                builder
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
            }
            None => builder.body(Body::empty()),
        };
        request.map_err(|e| e.to_string())
    }
}

/// The `/mcp` route, dispatching tools to `api`. With `auth`, a client must send an access
/// token (`Authorization: Bearer ...`) or a session cookie, and each tool runs with that
/// account's role: the credentials are passed on to the route the tool calls.
pub fn router(
    api: Router,
    auth: Option<Arc<crate::auth::Auth>>,
    services: Option<Arc<crate::services::ServiceHost>>,
) -> Router {
    Router::new()
        .route("/mcp", post(handle).get(no_stream).delete(no_stream))
        .with_state(McpState {
            api,
            auth,
            services,
        })
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
                        configurations, data types and the data of service modules such as the HMI's screens; one project is \
                        active and its connections feed the tags. Only a tag whose quality_name is \
                        GOOD has a current value: never report a value with any other quality as \
                        a measurement. Saving an I/O configuration or activating a project changes \
                        what the controller reads at once."
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => {
            let mut tools: Vec<Value> = TOOLS.iter().map(Tool::describe).collect();
            if let Some(services) = &state.services {
                tools.extend(ModuleTool::all(services).iter().map(ModuleTool::describe));
            }
            rpc_result(id, json!({ "tools": tools }))
        }
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "tools/call needs a tool 'name'"));
            };
            let arguments = match params.get("arguments") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(map)) => map.clone(),
                Some(_) => return Some(rpc_error(id, -32602, "'arguments' must be an object")),
            };
            let request = if let Some(tool) = TOOLS.iter().find(|t| t.name == name) {
                tool.request(&arguments)
            } else if let Some(tool) = state
                .services
                .as_deref()
                .map(ModuleTool::all)
                .unwrap_or_default()
                .into_iter()
                .find(|t| t.decl.tool == name)
            {
                tool.request(&arguments)
            } else {
                return Some(rpc_error(id, -32602, format!("unknown tool '{name}'")));
            };
            rpc_result(id, call(state, credentials, request).await)
        }
        other => rpc_error(id, -32601, format!("method '{other}' is not supported")),
    })
}

/// Run a tool: send its request through the API router and report what the route answered.
async fn call(
    state: &McpState,
    credentials: &[(header::HeaderName, header::HeaderValue)],
    request: Result<Request<Body>, String>,
) -> Value {
    let mut request = match request {
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
    fn the_runtimes_body_schema_takes_what_the_route_takes_and_refuses_what_it_refuses() {
        use crate::runtimes::RuntimesConfig;
        use mythos_module::protocol::TaskKind;
        let schema = runtimes_config_body();
        let instance = &schema["properties"]["instances"]["items"]["properties"];
        let kind = &instance["tasks"]["items"]["properties"]["kind"];
        let variants = kind["oneOf"].as_array().expect("one object per kind");
        let mut types: Vec<&str> = Vec::new();
        for v in variants {
            assert_eq!(v["additionalProperties"], json!(false), "{v}");
            let name = v["properties"]["type"]["const"].as_str().unwrap();
            types.push(name);
            let sample = |field: &str| match field {
                "type" => json!(name),
                "tag" => json!("external.A.B.c"),
                _ => json!(1),
            };
            let required: Vec<&str> = v["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap())
                .collect();
            let least: serde_json::Map<String, Value> = required
                .iter()
                .map(|f| (f.to_string(), sample(f)))
                .collect();
            let parse = |o: &serde_json::Map<String, Value>| {
                serde_json::from_value::<TaskKind>(Value::Object(o.clone()))
            };
            assert!(parse(&least).is_ok(), "{name}: its required fields");
            // Each field the schema lists, the route takes.
            for field in v["properties"].as_object().unwrap().keys() {
                let mut with = least.clone();
                with.insert(field.clone(), sample(field));
                assert!(parse(&with).is_ok(), "{name}: {field}");
            }
            // A field it does not list, and a required field left out, the route refuses.
            let mut extra = least.clone();
            extra.insert("nope".into(), json!(1));
            assert!(parse(&extra).is_err(), "{name}: an unknown field");
            for field in required.iter().filter(|f| **f != "type") {
                let mut missing = least.clone();
                missing.remove(*field);
                assert!(parse(&missing).is_err(), "{name}: without {field}");
            }
        }
        types.sort_unstable();
        assert_eq!(types, ["cycle", "cyclic", "event", "startup"]);

        // The making of it: listed for program and programs.
        for at in [
            &instance["program"]["properties"]["created"],
            &instance["programs"]["items"]["properties"]["created"],
        ] {
            assert_eq!(at["type"], json!("string"));
            assert!(at["description"]
                .as_str()
                .unwrap()
                .contains("A program deleted and made again under its name has another"));
        }
        // The project program a build is of: listed for program and programs, and taken.
        for at in [
            &instance["program"]["properties"]["built_from"],
            &instance["programs"]["items"]["properties"]["built_from"],
        ] {
            assert_eq!(at["type"], json!("string"));
            assert!(at["description"]
                .as_str()
                .unwrap()
                .contains("The project program this is a build of, as the logic module names it"));
        }
        let built: RuntimesConfig = serde_json::from_value(json!({"instances": [{
            "id": "t", "module": "vm", "run": true,
            "program": {"name": "a", "built_from": "A", "version": "1", "interval_ms": 20, "code": "00"}
        }]}))
        .unwrap();
        assert_eq!(built.validate(), Ok(()));
        assert_eq!(
            built.instances[0]
                .program
                .as_ref()
                .unwrap()
                .built_from
                .as_deref(),
            Some("A")
        );

        // A program's interval_ms: 0 is taken when tasks are given, so the schema allows it.
        let program = &instance["programs"]["items"]["properties"]["interval_ms"];
        assert_eq!(program["minimum"], json!(0));
        let p = |name: &str, interval_ms: u64| json!({"name": name, "version": "1", "interval_ms": interval_ms, "code": "00"});
        let config: RuntimesConfig = serde_json::from_value(json!({"instances": [{
            "id": "t", "module": "vm", "run": true, "programs": [p("a", 0), p("b", 0)],
            "tasks": [{"name": "f", "kind": {"type": "cyclic", "interval_ms": 20},
                       "priority": 1, "programs": ["a", "b"]}]
        }]}))
        .unwrap();
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn the_mcp_schema_describes_data() {
        use crate::runtimes::RuntimesConfig;
        let schema = runtimes_config_body();
        let instance = &schema["properties"]["instances"]["items"];
        assert!(instance["description"]
            .as_str()
            .unwrap()
            .ends_with("data: the data blocks its programs share."));
        let held = &instance["properties"]["data"]["items"];
        let used = &instance["properties"]["programs"]["items"]["properties"]["data"]["items"];
        assert_eq!(
            used, &instance["properties"]["program"]["properties"]["data"]["items"],
            "the same for program and programs"
        );
        let names = |v: &Value| -> Vec<String> {
            let mut n: Vec<String> = v
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap().to_string())
                .collect();
            n.sort_unstable();
            n
        };
        let keys = |v: &Value| -> Vec<String> {
            let mut n: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            n.sort_unstable();
            n
        };
        for v in [
            held,
            used,
            &held["properties"]["members"]["items"],
            &used["properties"]["members"]["items"],
        ] {
            assert_eq!(v["additionalProperties"], json!(false), "{v}");
        }
        // Required as the route requires: `constant` and `written` have no default; `retain` has.
        assert_eq!(
            names(&used["required"]),
            ["constant", "layout", "members", "name", "offset", "size", "type"]
        );
        assert_eq!(
            names(&used["properties"]["members"]["items"]["required"]),
            ["name", "offset", "size", "type", "written"]
        );
        assert_eq!(
            names(&held["required"]),
            ["layout", "members", "name", "size", "type"]
        );
        assert_eq!(
            keys(&held["properties"]),
            ["layout", "members", "name", "retain", "size", "type"]
        );
        assert_eq!(
            names(&held["properties"]["members"]["items"]["required"]),
            keys(&held["properties"]["members"]["items"]["properties"])
        );

        // A body written as the schema says validates; one field more, the route refuses.
        let member = json!({"name": "x", "type": "DINT", "offset": 0, "size": 4});
        let mut written = member.clone();
        written["written"] = json!(true);
        let body = json!({"instances": [{
            "id": "t", "module": "vm", "run": true,
            "program": {"name": "a", "version": "1", "interval_ms": 20, "code": "00",
                "data": [{"name": "S", "type": "T", "layout": "0123456789abcdef", "offset": 0,
                          "size": 4, "constant": false, "members": [written]}]},
            "data": [{"name": "S", "type": "T", "layout": "0123456789abcdef", "size": 4,
                      "retain": true, "members": [member]}]
        }]});
        let config: RuntimesConfig = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(config.validate(), Ok(()));
        let mut extra = body;
        extra["instances"][0]["data"][0]["members"][0]["written"] = json!(false);
        assert!(serde_json::from_value::<RuntimesConfig>(extra)
            .unwrap_err()
            .to_string()
            .contains("unknown field `written`"));
    }

    #[test]
    fn the_runtime_tools_say_what_they_take_and_report_of_data_blocks() {
        let tool = |name: &str| {
            TOOLS
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("no tool {name}"))
                .description
        };
        let save = tool("save_runtimes_config");
        for said in [
            "its data (each data block its code declares in VAR_EXTERNAL, as the compiler gave it; required when it declares any)",
            "a runtime module that does not say it hands data blocks over is refused a runtime with data",
            "each block in one runtime only",
            "no route of this server returns them",
            "two instances holding programs of one built_from are refused, and nothing is saved",
            "a start gives a program only the values kept for it (by built_from and created when both name one, else by program name; a program made again under its name is another program), and none when values kept for two programs could be it",
            "The one exception is a retain file written before values were kept per program (a JSON array): it is given once, as it is, to whatever the runtime runs at its next warm start, not_given then empty, and written again per program.",
            "never a build of a project made active since the running one was started",
            "so a running instance that names no project program is replaced online by a build of any program of the same PROGRAM name",
            "'changes.runtimes' lists in not_given each kept value it did not give, and why",
        ] {
            assert!(save.contains(said), "save_runtimes_config: {said}");
        }
        assert!(tool("list_runtimes")
            .contains("lists them in data, each member with its writer: the program that writes it, null when none does"));
        let (used, held) = data_schemas();
        assert!(used["description"]
            .as_str()
            .unwrap()
            .contains("Required for every block the code declares"));
        assert!(held["properties"]["members"]["description"]
            .as_str()
            .unwrap()
            .contains("without their written, which this list does not take"));
        assert!(
            used["properties"]["members"]["items"]["properties"]["written"]["description"]
                .as_str()
                .unwrap()
                .contains("Never change it")
        );
    }

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
