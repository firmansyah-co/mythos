//! The wire protocol between the host and a module process.
//!
//! One JSON object per line. The host writes [`Request`]s to the module's stdin; the module
//! writes one [`Response`] per request, with the same `id`, to its stdout. stdout carries
//! nothing else: a module logs to stderr, which the host forwards to its own log.
//!
//! Values cross the wire with their type. A float is sent as its bit pattern, so a NaN or an
//! infinity read from a device arrives as exactly that, and a 64-bit integer is never rounded
//! through a JSON double.

use mythos_plugin_sdk::{PluginError, SampleFault, SampleValue, TagDeclaration, TagSample};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The protocol version this crate speaks. A module that answers `hello` with another one is
/// refused.
pub const PROTOCOL_VERSION: u32 = 1;

/// Asks a module executable to print its manifest to stdout and exit.
pub const MANIFEST_FLAG: &str = "--manifest";

/// Asks a module executable to serve the protocol on stdin and stdout.
pub const SERVE_FLAG: &str = "--serve";

/// Every module executable's file name starts with this.
pub const MODULE_FILE_PREFIX: &str = "mythos-module-";

/// What a module is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    /// Reads field devices: a protocol driver. `provides` is the driver name `io.json` uses.
    Driver,
    /// Serves routes of the API, with data of its own (ADR-011): the HMI, logic, …
    Service,
    /// Executes a program continuously against the tags (ADR-013 §2.4): it is given the tags
    /// it reads, publishes tags of its own, and asks for writes to field tags. `provides` is
    /// the name a runtime instance's `module` field uses.
    Runtime,
}

/// What a module says about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleManifest {
    /// The protocol version the module speaks.
    pub protocol: u32,
    /// Stable identifier, for example `org.mythos.driver.modbus`.
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: ModuleKind,
    /// The name the module is referred to by in a configuration. For a driver, the value of a
    /// connection's `driver` field.
    pub provides: String,
    pub description: String,
    /// The capabilities the module needs the host to grant.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// JSON Schema of the configuration the module accepts. A workbench draws its form from it;
    /// the module itself remains the authority on what is valid. A service has none.
    #[serde(default)]
    pub config_schema: Value,
    /// The routes a service serves (ADR-011 §2.2). A driver has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<RouteDecl>,
    /// A service that runs for as long as the server does (it announces something, or watches
    /// the network), not only when one of its routes is asked for: the server starts it when it
    /// finds it and starts it again when it stops.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub always_on: bool,
    /// A runtime module's target profile (ADR-013 §2.2): what it can run, as data a compiler
    /// checks a program against. Other kinds have none.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub profile: Value,
}

/// The node a service module runs on (ADR-012 §2.1), sent as [`Call::Node`] after `hello` and
/// again whenever it changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireNode {
    pub id: String,
    pub name: String,
    /// `workstation`, `edge` or `server`.
    pub role: String,
    #[serde(default)]
    pub site: String,
    /// Whether the node announces itself on the network (its role's default or its own setting).
    pub announce: bool,
    /// The port its server answers plain HTTP on, once it does.
    #[serde(default)]
    pub port: Option<u16>,
    /// The port its server answers HTTPS on, once it does: what other nodes use (ADR-012 §2.3).
    #[serde(default)]
    pub https_port: Option<u16>,
    /// SHA-256 of its certificate, lowercase hex: what another node pins when it pairs.
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// The server's version.
    pub version: String,
    /// The module protocol the server speaks.
    pub protocol: u32,
}

/// Where a service's route is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteScope {
    /// `/api/v1/projects/:id/<provides><path>`: the route works on one project's data.
    Project,
    /// `/api/v1/m/<provides><path>`.
    Global,
}

/// One route a service module serves. The server mounts it, enforces `role`, makes it an MCP
/// tool named `tool`, and forwards each request as a [`Call::Http`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteDecl {
    /// `GET`, `POST`, `PUT` or `DELETE`.
    pub method: String,
    /// Below the module's mount point, starting with `/`; `:name` is a path parameter.
    pub path: String,
    pub scope: RouteScope,
    /// The least role that may call it: `viewer`, `operator`, `engineer` or `admin`. Absent is
    /// administrators only.
    #[serde(default)]
    pub role: Option<String>,
    /// Its MCP tool name, unique across the server.
    pub tool: String,
    /// What it does, for the tool's description.
    pub summary: String,
    /// JSON Schema of its request body, when it takes one.
    #[serde(default)]
    pub body: Option<Value>,
    /// Query parameters it reads, and what each means.
    #[serde(default)]
    pub query: Vec<(String, String)>,
    /// What of the server's state the module needs with each call. `"tags"`: the URIs of the
    /// tags registered in the tag catalog now, in [`HttpCall::tags`], so a module can refuse a
    /// binding to a tag that does not exist. Anything else is refused when the module is found.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
}

/// What a route may ask for in [`RouteDecl::needs`].
pub const ROUTE_NEEDS: &[&str] = &["tags"];

/// Who a forwarded request is from, as the server established it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireIdentity {
    pub username: String,
    pub role: String,
}

/// A request forwarded to a service module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpCall {
    pub method: String,
    /// The route as declared (`/screens/:screen`), so the module needs no router of its own.
    pub route: String,
    /// The path parameters of `route`, by name.
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub query: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<Value>,
    /// `None` when the server has no accounts (a test).
    #[serde(default)]
    pub identity: Option<WireIdentity>,
    /// For a project route: the project's id, and the directory of this module's data for it.
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub data_dir: Option<String>,
    /// The registered tag URIs, when the route declares that it needs them (`needs: ["tags"]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// A service module's answer to an [`HttpCall`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpReply {
    pub status: u16,
    #[serde(default)]
    pub body: Value,
}

impl HttpReply {
    pub fn json(status: u16, body: Value) -> Self {
        Self { status, body }
    }

    pub fn error(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            body: serde_json::json!({ "error": message.into() }),
        }
    }
}

/// Something a service module tells the server without being asked, written as its own line:
/// `{"event": {"name": "...", "data": ...}}`. The server relays it on the telemetry stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireEvent {
    pub name: String,
    #[serde(default)]
    pub data: Value,
}

/// A line a module writes: an answer to a request, or an event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Outgoing {
    Event { event: WireEvent },
    Response(Response),
}

/// Identity and permissions the host grants one module instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireContext {
    pub plugin_id: String,
    pub capabilities: Vec<String>,
}

/// A request from the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub call: Call,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Call {
    /// Initialise with this context. Answers with the module's [`ModuleManifest`].
    Hello { context: WireContext },
    /// The node a service runs on, after `hello` and whenever it changes. Answers with `null`.
    Node { node: WireNode },
    /// Check a configuration and list the tags it would provide. Starts nothing, needs no
    /// `hello`. Answers with a list of [`WireDeclaration`].
    Describe { config: Value },
    /// Start with a configuration. Answers with the tags it provides, as [`WireDeclaration`]s.
    Start { config: Value },
    /// Run one poll cycle. Answers with one [`WireSample`] per declared tag.
    Poll,
    /// Write a value to a tag. Answers with `null`.
    Write { uri: String, value: Value },
    /// Report health. Answers with a `PluginHealthReport`.
    Health,
    /// Stop. Answers with `null`; the module may then be closed.
    Stop,
    /// A request to one of a service module's routes. Answers with an [`HttpReply`].
    Http(HttpCall),
    /// One exchange with a runtime module: the tags it reads as they are now, in; what it
    /// publishes and what it wants written, out. Answers with an [`ExchangeReply`].
    Exchange { inputs: Vec<WireInput> },
    /// Put a runtime module's program in RUN or in STOP. From STOP or FAULT, RUN is a warm
    /// restart: its RETAIN variables keep the values they had, the others take their initial
    /// values; with `cold`, every variable takes its initial value (IEC 61131-3 §6.5.6).
    /// Answers with its [`RuntimeState`].
    Mode {
        run: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cold: bool,
    },
    /// Read variables of a runtime module's program as its last scan left them. It changes
    /// nothing. Answers with a [`WatchReply`].
    Watch { variables: Vec<String> },
    /// Hold a variable of a running program at a value: whatever the inputs and the program
    /// say, the variable reads and is written as `value` until it is released, the program
    /// leaves RUN, or another program is loaded. Answers with its [`RuntimeState`].
    Force { variable: String, value: Value },
    /// Release a forced variable, or with `None` every one. Answers with its [`RuntimeState`].
    Unforce {
        #[serde(default)]
        variable: Option<String>,
    },
    /// The values of the program's RETAIN variables now, for the host to keep across a restart
    /// of the module. Answers with a list of [`RetainedValue`]: empty when it has none.
    Retained,
    /// The values of every variable of the program now, and its clock, for the host to carry
    /// into a new build of the same program that replaces this one (an online change; `carried`
    /// beside `program` in the start configuration). Only a program in RUN gives them. It
    /// changes nothing. Answers with [`ProgramVariables`].
    Variables,
    /// What the program took of the values it was started with (`carried`). Answers with a
    /// [`CarryReport`].
    Carried,
}

/// The variables of a running program, as [`Call::Variables`] gives them and a new build of
/// the program is started from (`carried` in its start configuration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramVariables {
    /// The program's clock when they were read: nanoseconds since it was put in RUN. A program
    /// started from them goes on from this clock, so its timers neither lose nor gain the time
    /// the swap took.
    pub clock_ns: i64,
    /// Every variable, each in the same form as a RETAIN value: path, type and bytes.
    pub values: Vec<RetainedValue>,
}

/// What a program did with the values it was started with (ADR-013 §2.5, online change).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryReport {
    /// Whether it was given any. A program given none carried none, and says so.
    pub given: bool,
    /// How many of its variables took the running program's value.
    pub carried: u64,
    /// The running program's variables it did not take, and why: it has none of that name, or
    /// its own is of another type or size.
    #[serde(default)]
    pub dropped: Vec<NotCarried>,
    /// Its variables that took no value from the running program, and why: the running one
    /// had none of that name, or one of another type or size. Each started from its initial
    /// value, or a RETAIN one from the value kept for it, as at a warm restart.
    #[serde(default)]
    pub initialized: Vec<NotCarried>,
}

/// A variable that did not carry over, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotCarried {
    pub variable: String,
    pub why: String,
}

/// The value of one RETAIN variable, as the runtime module keeps it: what the host stores and
/// gives back when it starts the program again (`retained` beside `program` in the start
/// configuration), which takes it only into a variable of the same path, type and size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedValue {
    pub path: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// The variable's bytes in the runtime module's own form, as lowercase hex.
    pub bytes: String,
    /// Only for a data block's master copy (ADR-014): the block's layout hash. `path` is then
    /// the block's name, `ty` its type and `bytes` the whole master. `None` for a variable of a
    /// program, and absent from the JSON, so a value kept before data blocks reads unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
}

// -----------------------------------------------------------------------------
// Runtime modules (ADR-013 §2.4)
// -----------------------------------------------------------------------------

/// What a runtime module is started with: a program and how it is tied to the tags.
///
/// The core reads the bindings (it owns the tags); `code` and `symbols` are the runtime
/// module's own and opaque to the core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeProgram {
    pub name: String,
    /// What identifies this build of the program: shown as the version that runs.
    pub version: String,
    /// The interval of its cyclic task.
    pub interval_ms: u64,
    /// The program in the runtime module's own form, as lowercase hex.
    pub code: String,
    /// Where its variables are, in the runtime module's own form.
    #[serde(default)]
    pub symbols: Value,
    /// Tags read into variables at the start of each scan.
    #[serde(default)]
    pub inputs: Vec<InputBinding>,
    /// Variables written to field tags after a scan, and what each tag is set to when the
    /// program is not running.
    #[serde(default)]
    pub outputs: Vec<OutputBinding>,
    /// Variables published as tags of the runtime's own.
    #[serde(default)]
    pub published: Vec<PublishedBinding>,
    /// The data blocks it declares in `VAR_EXTERNAL`, as the compiler says (ADR-014). Empty,
    /// and absent from the JSON, for a program that declares none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<DataUse>,
}

// -----------------------------------------------------------------------------
// Data blocks (ADR-014)
// -----------------------------------------------------------------------------

/// One data block a program declares in `VAR_EXTERNAL`: the compiler's `DataUse`, in the same
/// JSON. This crate does not depend on the compiler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataUse {
    /// As the project declares it.
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// The layout hash: 16 lowercase hex digits.
    pub layout: String,
    /// Of the program's copy, in the program's memory.
    pub offset: u32,
    pub size: u32,
    /// Declared `VAR_EXTERNAL CONSTANT`. Required: no default.
    pub constant: bool,
    /// The top-level members, in declaration order.
    pub members: Vec<DataMember>,
}

/// A top-level member of a data block as a program declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataMember {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Inside the block.
    pub offset: u32,
    pub size: u32,
    /// The program writes it. Required: a missing `written` read as false would hide a writer
    /// from the one-writer check, and the member would never be handed over.
    pub written: bool,
}

/// One data block of a runtime: the compiler's `DataLayout` without `written`, and whether
/// its value is kept across a warm restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataBlock {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub layout: String,
    pub size: u32,
    pub members: Vec<DataBlockMember>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub retain: bool,
}

/// A top-level member of a data block of a runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataBlockMember {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub offset: u32,
    pub size: u32,
}

/// What a runtime module says, in its manifest's `profile.data`, about the data blocks it hands
/// over between programs. A module that says nothing hands none over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSupport {
    /// The tiers it hands blocks over in: 1, between the programs of one runtime.
    pub tiers: Vec<u8>,
}

impl DataSupport {
    /// The data support a runtime module's profile declares; `None` when it declares none, or
    /// declares it in a form this crate cannot read.
    pub fn of(profile: &Value) -> Option<DataSupport> {
        serde_json::from_value(profile.get("data")?.clone()).ok()
    }
}

/// A data block's name: an IEC identifier, a letter or `_` first, then letters, digits or `_`.
fn valid_block_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 16 lowercase hex digits.
fn valid_layout(layout: &str) -> bool {
    layout.len() == 16
        && layout
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// The first segment of a variable's name: before its first `.` or `[`.
fn first_segment(variable: &str) -> &str {
    let end = variable.find(['.', '[']).unwrap_or(variable.len());
    variable[..end].trim()
}

/// Check the data blocks of one runtime against its programs (ADR-014): every name valid,
/// every block a program declares in the runtime's `data` and laid out as it was built with,
/// each member written by one program at most, and no tag written into a member. The same
/// check, word for word, in the core and in a runtime module. Block names are compared without
/// regard to case.
pub fn check_data(programs: &[&RuntimeProgram], data: &[DataBlock]) -> Result<(), String> {
    for (i, b) in data.iter().enumerate() {
        let name = &b.name;
        if !valid_block_name(name) {
            return Err(format!(
                "'{name}' is not a data block name: use a letter or '_' first, then letters, digits or '_'"
            ));
        }
        if data[..i].iter().any(|c| c.name.eq_ignore_ascii_case(name)) {
            return Err(format!("two data blocks are named '{name}'"));
        }
        if programs.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
            return Err(format!(
                "data block '{name}' has the name of a program of this runtime"
            ));
        }
        if !valid_layout(&b.layout) {
            return Err(format!(
                "data block '{name}': its layout '{}' is not 16 lowercase hex digits",
                b.layout
            ));
        }
        for (j, m) in b.members.iter().enumerate() {
            let end = u64::from(m.offset) + u64::from(m.size);
            if end > u64::from(b.size) {
                return Err(format!(
                    "data block '{name}': member '{}' lies outside its {} bytes",
                    m.name, b.size
                ));
            }
            // The hand-over copies each written member whole: two members that share a byte
            // would let two programs write it.
            if let Some(o) = b.members[..j].iter().find(|o| {
                u64::from(o.offset) < end
                    && u64::from(m.offset) < u64::from(o.offset) + u64::from(o.size)
            }) {
                return Err(format!(
                    "data block '{name}': members '{}' and '{}' overlap",
                    o.name, m.name
                ));
            }
        }
    }
    let names = if data.is_empty() {
        "it has none".to_string()
    } else {
        format!(
            "it has: {}",
            data.iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    for p in programs {
        let pn = &p.name;
        for (j, u) in p.data.iter().enumerate() {
            let un = &u.name;
            if p.data[..j].iter().any(|v| v.name.eq_ignore_ascii_case(un)) {
                return Err(format!("program '{pn}' declares data block '{un}' twice"));
            }
            let Some(b) = data.iter().find(|b| b.name.eq_ignore_ascii_case(un)) else {
                return Err(format!(
                    "program '{pn}' uses data block '{un}', which is not in the runtime's data ({names})"
                ));
            };
            if u.layout != b.layout || u.size != b.size {
                return Err(format!(
                    "program '{pn}' was built with data block '{un}' laid out as {} ({} bytes), and the runtime's '{}' is laid out as {} ({} bytes): build '{pn}' again",
                    u.layout, u.size, b.name, b.layout, b.size
                ));
            }
            let same_members = u.members.len() == b.members.len()
                && u.members.iter().zip(&b.members).all(|(m, n)| {
                    m.name == n.name && m.ty == n.ty && m.offset == n.offset && m.size == n.size
                });
            if u.ty != b.ty || !same_members {
                return Err(format!(
                    "program '{pn}' was built with data block '{un}' of type {} with other members than the runtime's {}: build '{pn}' again",
                    u.ty, b.ty
                ));
            }
            if u.constant {
                if let Some(m) = u.members.iter().find(|m| m.written) {
                    return Err(format!(
                        "program '{pn}' declares data block '{un}' CONSTANT and writes its member '{}'",
                        m.name
                    ));
                }
            }
        }
    }
    for b in data {
        for (i, m) in b.members.iter().enumerate() {
            let mut writer: Option<&str> = None;
            for p in programs {
                let writes = p
                    .data
                    .iter()
                    .find(|u| u.name.eq_ignore_ascii_case(&b.name))
                    .is_some_and(|u| u.members[i].written);
                if !writes {
                    continue;
                }
                if let Some(a) = writer {
                    return Err(format!(
                        "data block '{}': member '{}' is written by programs '{a}' and '{}'; a member has one writer",
                        b.name, m.name, p.name
                    ));
                }
                writer = Some(&p.name);
            }
        }
    }
    for p in programs {
        let block_of = |variable: &str| {
            let first = first_segment(variable);
            p.data
                .iter()
                .find(|u| u.name.eq_ignore_ascii_case(first))
                .map(|u| u.name.as_str())
        };
        for i in &p.inputs {
            if let Some(b) = block_of(&i.variable) {
                return Err(format!(
                    "program '{}': {} is a member of data block '{b}', which only a program writes: a tag cannot be read into it",
                    p.name, i.variable
                ));
            }
        }
        for s in p.published.iter().filter(|s| s.writable) {
            if let Some(b) = block_of(&s.variable) {
                return Err(format!(
                    "program '{}': {} is a member of data block '{b}', which only a program writes: it cannot be a setpoint",
                    p.name, s.variable
                ));
            }
        }
    }
    Ok(())
}

/// What of a tag a variable takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputOf {
    /// Its value. While the tag is not GOOD the variable keeps what it last took.
    #[default]
    Value,
    /// Whether it is GOOD: a BOOL, so a program can act on a value having gone stale.
    Good,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputBinding {
    pub variable: String,
    pub tag: String,
    #[serde(default)]
    pub of: InputOf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBinding {
    pub variable: String,
    pub tag: String,
    /// What the tag is written to whenever the program is not running. Required: there is no
    /// default, because no value is safe for every plant.
    pub safe: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedBinding {
    pub variable: String,
    pub tag: String,
    /// Whether an operator may write the variable through the tag (a setpoint).
    #[serde(default)]
    pub writable: bool,
}

/// A tag a runtime reads, as it is at this exchange.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireInput {
    pub tag: String,
    /// `None`: the tag has never been read, or does not exist.
    #[serde(default)]
    pub value: Option<Value>,
    /// Whether the value is current (quality GOOD).
    pub good: bool,
}

/// A value a runtime wants written to a field tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireOutput {
    pub tag: String,
    pub value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Run,
    Stop,
    /// The program stopped itself: a scan faulted. `RuntimeState::fault` says why.
    Fault,
}

/// What a runtime module's program is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeState {
    pub mode: RuntimeMode,
    #[serde(default)]
    pub fault: Option<String>,
    pub program: String,
    pub version: String,
    /// Scans completed since the program was last put in RUN.
    pub scans: u64,
    pub last_scan_us: u64,
    pub max_scan_us: u64,
    /// Scans that started late because the one before had not finished.
    pub overruns: u64,
    /// How long before this answer the program last completed a scan, in µs: how old the values
    /// a scan left are. `None` until a scan has completed since the program was put in RUN, and
    /// from a module that does not say (ADR-013 §2.8: a view then draws nothing as current).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_age_us: Option<u64>,
    /// The variables held at a value by [`Call::Force`]. Empty from a module that cannot force.
    #[serde(default)]
    pub forced: Vec<ForcedValue>,
    /// Each task's state, from a runtime started with tasks written out (ADR-013 §2.19). Empty
    /// for a runtime of one program in its implied task, and from a module that runs no tasks:
    /// the fields above then say all there is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<TaskState>,
    /// Each program and the version of it that runs, in the order the runtime was given them;
    /// empty when `tasks` is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<ProgramVersion>,
}

// -----------------------------------------------------------------------------
// Tasks (ADR-013 §2.19)
// -----------------------------------------------------------------------------

/// One task of a runtime: when it runs, at which priority, and the programs it runs, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskConfig {
    /// 1 to 64 letters, digits, `-` or `_`.
    pub name: String,
    pub kind: TaskKind,
    /// 0 is the highest, 31 the lowest (IEC 61131-3). Required: no priority is right for every
    /// task.
    pub priority: u8,
    /// How long the task may owe a run before the runtime is stopped. `None`: ten intervals and
    /// never less than 2 s for a cyclic or cycle task, 2 s for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watchdog_ms: Option<u64>,
    /// Names of the runtime's programs ([`RuntimeProgram::name`]), in the order they run.
    pub programs: Vec<String>,
}

/// When a task runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskKind {
    /// Again as soon as its run has ended, no sooner than `min_cycle_ms` after the last began;
    /// below every priority.
    Cycle {
        #[serde(default)]
        min_cycle_ms: u64,
    },
    /// Every `interval_ms`, the first time `phase_ms` after the change to RUN.
    Cyclic {
        interval_ms: u64,
        #[serde(default)]
        phase_ms: u64,
    },
    /// Once at every change to RUN, before any other task. Written with braces, as a variant
    /// with fields, so that serde refuses a field it does not know here too: a unit variant of
    /// an internally tagged enum ignores them, `deny_unknown_fields` or not.
    Startup {},
    /// At each rising edge of the BOOL tag `tag`, as the exchanges see it.
    Event { tag: String },
}

impl TaskKind {
    /// The name of the kind, as [`TaskSupport::kinds`] lists it.
    pub fn name(&self) -> &'static str {
        match self {
            TaskKind::Cycle { .. } => "cycle",
            TaskKind::Cyclic { .. } => "cyclic",
            TaskKind::Startup {} => "startup",
            TaskKind::Event { .. } => "event",
        }
    }
}

/// What a runtime module says, in its manifest's `profile.tasks`, about the tasks it runs. A
/// module that says nothing runs one program in one cyclic task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSupport {
    /// The kinds of task it runs: `cycle`, `cyclic`, `startup`, `event`.
    pub kinds: Vec<String>,
}

impl TaskSupport {
    /// The task support a runtime module's profile declares; `None` when it declares none, or
    /// declares it in a form this crate cannot read.
    pub fn of(profile: &Value) -> Option<TaskSupport> {
        serde_json::from_value(profile.get("tasks")?.clone()).ok()
    }
}

/// A program of a runtime and the version of it that runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramVersion {
    pub name: String,
    pub version: String,
}

/// What one task of a runtime is doing. A time not measured is `None`, never 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskState {
    pub name: String,
    pub kind: TaskKind,
    pub priority: u8,
    pub programs: Vec<String>,
    /// The watchdog's limit in force, in ms: the one set, or the default.
    pub watchdog_ms: u64,
    /// Runs ended since the runtime was last put in RUN.
    pub runs: u64,
    /// Runs not made because the task was due or triggered again before its last run ended.
    pub overruns: u64,
    /// Times a task of higher priority began while this one was in progress.
    pub preemptions: u64,
    /// From the beginning to the end of its last run, preemptions included.
    pub last_run_us: Option<u64>,
    pub longest_run_us: Option<u64>,
    /// How long before this answer its last run ended; `None` before one has.
    pub run_age_us: Option<u64>,
}

/// The task a runtime given no tasks runs its one program in: cyclic at the program's
/// interval, from the change to RUN, at the lowest priority, with the default watchdog.
pub fn implied_task(program: &RuntimeProgram) -> TaskConfig {
    TaskConfig {
        name: program.name.clone(),
        kind: TaskKind::Cyclic {
            interval_ms: program.interval_ms,
            phase_ms: 0,
        },
        priority: LOWEST_TASK_PRIORITY,
        watchdog_ms: None,
        programs: vec![program.name.clone()],
    }
}

/// The lowest priority a task can have; 0 is the highest.
pub const LOWEST_TASK_PRIORITY: u8 = 31;

/// A task's name: 1 to 64 letters, digits, `-` or `_`.
fn valid_task_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A program's name in a runtime of several programs, where a variable is named
/// `<program>.<path>`: none of `.`, `[`, `]` or white space, and not empty.
fn valid_program_name(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c == '.' || c == '[' || c == ']' || c.is_whitespace())
}

/// Check the programs and tasks of one runtime (ADR-013 §2.19): each program in exactly one
/// task, every name valid and every time representable. The same check, word for word, in the
/// core and in a runtime module. Program names are compared without regard to case, as a
/// variable's program is named; task names exactly. With no tasks, the runtime must have one
/// program, which runs in its [`implied_task`].
pub fn check_tasks(programs: &[&RuntimeProgram], tasks: &[TaskConfig]) -> Result<(), String> {
    if programs.is_empty() {
        return Err("it runs no program".to_string());
    }
    if programs.len() > 1 && tasks.is_empty() {
        return Err(
            "it runs several programs and names no task: each program needs a task".to_string(),
        );
    }
    for (i, p) in programs.iter().enumerate() {
        if programs[..i]
            .iter()
            .any(|q| q.name.eq_ignore_ascii_case(&p.name))
        {
            return Err(format!("two programs are named '{}'", p.name));
        }
        if programs.len() > 1 && !valid_program_name(&p.name) {
            return Err(format!(
                "'{}' is not a program name: no '.', '[', ']' or spaces",
                p.name
            ));
        }
    }
    let names = programs
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    // For each program, the task it is in.
    let mut owner: Vec<Option<&str>> = vec![None; programs.len()];
    for (i, t) in tasks.iter().enumerate() {
        let name = &t.name;
        if !valid_task_name(name) {
            return Err(format!(
                "'{name}' is not a task name: use 1 to 64 letters, digits, '-' or '_'"
            ));
        }
        if tasks[..i].iter().any(|u| u.name == *name) {
            return Err(format!("task name '{name}' is used more than once"));
        }
        if t.priority > LOWEST_TASK_PRIORITY {
            return Err(format!(
                "task '{name}' has priority {}; priorities are 0 (highest) to {LOWEST_TASK_PRIORITY}",
                t.priority
            ));
        }
        let mut times: Vec<(&str, u64)> = Vec::new();
        match &t.kind {
            TaskKind::Cyclic {
                interval_ms,
                phase_ms,
            } => {
                if *interval_ms == 0 {
                    return Err(format!(
                        "task '{name}' has interval_ms 0; it must be at least 1"
                    ));
                }
                times.push(("interval_ms", *interval_ms));
                times.push(("phase_ms", *phase_ms));
            }
            TaskKind::Cycle { min_cycle_ms } => times.push(("min_cycle_ms", *min_cycle_ms)),
            TaskKind::Event { tag } if tag.trim().is_empty() => {
                return Err(format!("event task '{name}' names no tag"));
            }
            TaskKind::Event { .. } | TaskKind::Startup {} => {}
        }
        if let Some(w) = t.watchdog_ms {
            if w == 0 {
                return Err(format!("task '{name}': watchdog_ms must be at least 1"));
            }
            times.push(("watchdog_ms", w));
        }
        for (field, ms) in times {
            // The scheduler counts in nanoseconds of an i64.
            if ms
                .checked_mul(1_000_000)
                .is_none_or(|ns| ns > i64::MAX as u64)
            {
                return Err(format!("task '{name}': {field} is too long"));
            }
        }
        if t.programs.is_empty() {
            return Err(format!("task '{name}' runs no program"));
        }
        for (j, p) in t.programs.iter().enumerate() {
            let Some(k) = programs.iter().position(|q| q.name.eq_ignore_ascii_case(p)) else {
                return Err(format!(
                    "task '{name}' runs '{p}', which is not a program of this runtime (it has {names})"
                ));
            };
            if t.programs[..j].iter().any(|q| q.eq_ignore_ascii_case(p)) {
                return Err(format!("task '{name}' names '{p}' twice"));
            }
            if let Some(other) = owner[k] {
                return Err(format!(
                    "program '{}' is in tasks '{other}' and '{name}'; a program is in one task",
                    programs[k].name
                ));
            }
            owner[k] = Some(name);
        }
    }
    if !tasks.is_empty() {
        if let Some(k) = owner.iter().position(Option::is_none) {
            return Err(format!(
                "program '{}' is in no task, so it would never run",
                programs[k].name
            ));
        }
    }
    Ok(())
}

/// A variable held at a value by [`Call::Force`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForcedValue {
    /// The name as it was forced.
    pub variable: String,
    pub value: Value,
}

/// A runtime module's answer to [`Call::Exchange`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExchangeReply {
    pub state: RuntimeState,
    /// One sample per published tag: its value while the program runs, a fault otherwise.
    pub samples: Vec<WireSample>,
    /// The outputs as the last scan left them. Empty unless the program is running.
    pub outputs: Vec<WireOutput>,
}

/// One variable of a [`WatchReply`]: its value, or why it has none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchedValue {
    /// The name as it was asked for.
    pub variable: String,
    /// Its type as the program declares it, when the program has it.
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
    /// `None`: there is no value to give, and `error` says why.
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
    /// Whether the value is held there by [`Call::Force`] rather than by the program.
    #[serde(default)]
    pub forced: bool,
}

/// A runtime module's answer to [`Call::Watch`]: the variables as the last scan left them,
/// and the state they were read in. A value read while the program is not in RUN is what it
/// was when it stopped, or its initial value: `state` says which program and whether it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchReply {
    pub state: RuntimeState,
    pub values: Vec<WatchedValue>,
}

/// The module's answer to one request: `result` or `error`, never both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WireError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, error: WireError) -> Self {
        Self {
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    InitializationFailed,
    StartFailed,
    RuntimeError,
    InvalidConfiguration,
    CapabilityViolation,
    Timeout,
    InvalidTag,
    CommunicationError,
    /// The request itself could not be understood.
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
    pub kind: ErrorKind,
    pub message: String,
}

impl From<&PluginError> for WireError {
    fn from(error: &PluginError) -> Self {
        let (kind, message) = match error {
            PluginError::InitializationFailed(m) => (ErrorKind::InitializationFailed, m),
            PluginError::StartFailed(m) => (ErrorKind::StartFailed, m),
            PluginError::RuntimeError(m) => (ErrorKind::RuntimeError, m),
            PluginError::InvalidConfiguration(m) => (ErrorKind::InvalidConfiguration, m),
            PluginError::CapabilityViolation(m) => (ErrorKind::CapabilityViolation, m),
            PluginError::Timeout(m) => (ErrorKind::Timeout, m),
            PluginError::InvalidTag(m) => (ErrorKind::InvalidTag, m),
            PluginError::CommunicationError(m) => (ErrorKind::CommunicationError, m),
        };
        Self {
            kind,
            message: message.clone(),
        }
    }
}

impl From<WireError> for PluginError {
    fn from(error: WireError) -> Self {
        let m = error.message;
        match error.kind {
            ErrorKind::InitializationFailed => PluginError::InitializationFailed(m),
            ErrorKind::StartFailed => PluginError::StartFailed(m),
            ErrorKind::RuntimeError => PluginError::RuntimeError(m),
            ErrorKind::InvalidConfiguration => PluginError::InvalidConfiguration(m),
            ErrorKind::CapabilityViolation => PluginError::CapabilityViolation(m),
            ErrorKind::Timeout => PluginError::Timeout(m),
            ErrorKind::InvalidTag => PluginError::InvalidTag(m),
            ErrorKind::CommunicationError => PluginError::CommunicationError(m),
            ErrorKind::Protocol => PluginError::RuntimeError(format!("module protocol error: {m}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireDeclaration {
    pub uri: String,
    pub description: String,
    /// Whether the module can write the tag. A module built before this field existed does
    /// not say, and is taken at its word: read-only.
    #[serde(default)]
    pub writable: bool,
}

impl From<&TagDeclaration> for WireDeclaration {
    fn from(d: &TagDeclaration) -> Self {
        Self {
            uri: d.uri.clone(),
            description: d.description.clone(),
            writable: d.writable,
        }
    }
}

impl From<WireDeclaration> for TagDeclaration {
    fn from(d: WireDeclaration) -> Self {
        Self {
            uri: d.uri,
            description: d.description,
            writable: d.writable,
        }
    }
}

/// A typed value. Floats travel as their bit pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireValue {
    Bool(bool),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32Bits(u32),
    F64Bits(u64),
}

impl From<SampleValue> for WireValue {
    fn from(v: SampleValue) -> Self {
        match v {
            SampleValue::Bool(v) => Self::Bool(v),
            SampleValue::U8(v) => Self::U8(v),
            SampleValue::I16(v) => Self::I16(v),
            SampleValue::U16(v) => Self::U16(v),
            SampleValue::I32(v) => Self::I32(v),
            SampleValue::U32(v) => Self::U32(v),
            SampleValue::I64(v) => Self::I64(v),
            SampleValue::U64(v) => Self::U64(v),
            SampleValue::F32(v) => Self::F32Bits(v.to_bits()),
            SampleValue::F64(v) => Self::F64Bits(v.to_bits()),
        }
    }
}

impl From<WireValue> for SampleValue {
    fn from(v: WireValue) -> Self {
        match v {
            WireValue::Bool(v) => Self::Bool(v),
            WireValue::U8(v) => Self::U8(v),
            WireValue::I16(v) => Self::I16(v),
            WireValue::U16(v) => Self::U16(v),
            WireValue::I32(v) => Self::I32(v),
            WireValue::U32(v) => Self::U32(v),
            WireValue::I64(v) => Self::I64(v),
            WireValue::U64(v) => Self::U64(v),
            WireValue::F32Bits(v) => Self::F32(f32::from_bits(v)),
            WireValue::F64Bits(v) => Self::F64(f64::from_bits(v)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireFault {
    Comm(String),
    Device(String),
    Config(String),
}

impl From<&SampleFault> for WireFault {
    fn from(f: &SampleFault) -> Self {
        match f {
            SampleFault::CommFailure(m) => Self::Comm(m.clone()),
            SampleFault::DeviceFailure(m) => Self::Device(m.clone()),
            SampleFault::ConfigError(m) => Self::Config(m.clone()),
        }
    }
}

impl From<WireFault> for SampleFault {
    fn from(f: WireFault) -> Self {
        match f {
            WireFault::Comm(m) => Self::CommFailure(m),
            WireFault::Device(m) => Self::DeviceFailure(m),
            WireFault::Config(m) => Self::ConfigError(m),
        }
    }
}

/// One tag's outcome: exactly one of `value` and `fault`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireSample {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<WireValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<WireFault>,
}

impl From<&TagSample> for WireSample {
    fn from(s: &TagSample) -> Self {
        match &s.reading {
            Ok(v) => Self {
                uri: s.uri.clone(),
                value: Some((*v).into()),
                fault: None,
            },
            Err(f) => Self {
                uri: s.uri.clone(),
                value: None,
                fault: Some(f.into()),
            },
        }
    }
}

impl From<WireSample> for TagSample {
    /// A sample that carries both a value and a fault, or neither, is malformed. It becomes a
    /// communication failure: a value the module did not clearly report is not a reading.
    fn from(s: WireSample) -> Self {
        match (s.value, s.fault) {
            (Some(v), None) => TagSample::value(s.uri, v.into()),
            (None, Some(f)) => TagSample::fault(s.uri, f.into()),
            _ => TagSample::fault(
                s.uri,
                SampleFault::CommFailure(
                    "the module sent a sample with neither or both of a value and a fault"
                        .to_string(),
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_request_has_a_fixed_shape_on_the_wire() {
        // Pinned literally: a symmetric round trip would accept any consistent encoding,
        // including one a module written in another language could not produce.
        let poll = Request {
            id: 7,
            call: Call::Poll,
        };
        assert_eq!(
            serde_json::to_value(&poll).unwrap(),
            json!({"id": 7, "call": {"method": "poll"}})
        );

        let start = Request {
            id: 8,
            call: Call::Start {
                config: json!({"host": "h"}),
            },
        };
        assert_eq!(
            serde_json::to_value(&start).unwrap(),
            json!({"id": 8, "call": {"method": "start", "params": {"config": {"host": "h"}}}})
        );

        let parsed: Request = serde_json::from_str(r#"{"id":1,"call":{"method":"stop"}}"#).unwrap();
        assert_eq!(parsed.call, Call::Stop);
    }

    #[test]
    fn a_sample_has_a_fixed_shape_on_the_wire() {
        let value = WireSample::from(&TagSample::value("external.a.b.c", SampleValue::U16(5)));
        assert_eq!(
            serde_json::to_value(&value).unwrap(),
            json!({"uri": "external.a.b.c", "value": {"u16": 5}})
        );
        let fault = WireSample::from(&TagSample::fault(
            "external.a.b.c",
            SampleFault::DeviceFailure("x".into()),
        ));
        assert_eq!(
            serde_json::to_value(&fault).unwrap(),
            json!({"uri": "external.a.b.c", "fault": {"device": "x"}})
        );
    }

    #[test]
    fn floats_survive_the_wire_exactly_including_nan_and_infinity() {
        for v in [f32::NAN, f32::INFINITY, -0.0f32, 1.1f32] {
            let wire = serde_json::to_string(&WireValue::from(SampleValue::F32(v))).unwrap();
            let back: SampleValue = serde_json::from_str::<WireValue>(&wire).unwrap().into();
            match back {
                SampleValue::F32(b) => assert_eq!(b.to_bits(), v.to_bits()),
                other => panic!("type changed: {other:?}"),
            }
        }
        let big = u64::MAX - 1;
        let wire = serde_json::to_string(&WireValue::from(SampleValue::U64(big))).unwrap();
        assert_eq!(
            SampleValue::from(serde_json::from_str::<WireValue>(&wire).unwrap()),
            SampleValue::U64(big)
        );
    }

    #[test]
    fn a_sample_with_neither_or_both_is_a_fault_not_a_value() {
        let neither = WireSample {
            uri: "external.a.b.c".into(),
            value: None,
            fault: None,
        };
        assert!(TagSample::from(neither).reading.is_err());
        let both = WireSample {
            uri: "external.a.b.c".into(),
            value: Some(WireValue::U16(1)),
            fault: Some(WireFault::Comm("x".into())),
        };
        assert!(TagSample::from(both).reading.is_err());
    }

    fn program(name: &str) -> RuntimeProgram {
        RuntimeProgram {
            name: name.into(),
            version: "1".into(),
            interval_ms: 100,
            code: String::new(),
            symbols: Value::Null,
            inputs: Vec::new(),
            outputs: Vec::new(),
            published: Vec::new(),
            data: Vec::new(),
        }
    }

    const HASH: &str = "0123456789abcdef";

    /// Block `S` of type `T`: members `a` (INT at 0) and `b` (DINT at 4), 8 bytes.
    fn block(name: &str) -> DataBlock {
        DataBlock {
            name: name.into(),
            ty: "T".into(),
            layout: HASH.into(),
            size: 8,
            members: vec![
                DataBlockMember {
                    name: "a".into(),
                    ty: "INT".into(),
                    offset: 0,
                    size: 2,
                },
                DataBlockMember {
                    name: "b".into(),
                    ty: "DINT".into(),
                    offset: 4,
                    size: 4,
                },
            ],
            retain: false,
        }
    }

    /// Program `name` declaring `block` (as [`block`] lays it out) at offset 16, writing the
    /// members whose names are in `writes`.
    fn user(name: &str, block_name: &str, writes: &[&str]) -> RuntimeProgram {
        let b = block(block_name);
        let mut p = program(name);
        p.data.push(DataUse {
            name: block_name.into(),
            ty: b.ty.clone(),
            layout: b.layout.clone(),
            offset: 16,
            size: b.size,
            constant: false,
            members: b
                .members
                .iter()
                .map(|m| DataMember {
                    name: m.name.clone(),
                    ty: m.ty.clone(),
                    offset: m.offset,
                    size: m.size,
                    written: writes.contains(&m.name.as_str()),
                })
                .collect(),
        });
        p
    }

    #[test]
    fn check_data_refuses_each_thing_it_can_get_wrong() {
        let check = |programs: &[&RuntimeProgram], data: &[DataBlock]| {
            check_data(programs, data).err().unwrap_or_default()
        };
        // What is right: one writer of `a`, one of `b`, a block no program declares, and the
        // runtime of today.
        let (w, r) = (user("w", "S", &["a"]), user("r", "s", &["b"]));
        assert_eq!(
            check_data(&[&w, &r], &[block("S"), block("Unused")]),
            Ok(())
        );
        assert_eq!(check_data(&[&program("p")], &[]), Ok(()));

        // R1.
        for bad in ["", "1x", "a.b", "a b", "x-y", "é"] {
            assert_eq!(
                check(&[&w], &[block(bad)]),
                format!("'{bad}' is not a data block name: use a letter or '_' first, then letters, digits or '_'")
            );
        }
        assert_eq!(check_data(&[], &[block("_x9")]), Ok(()));
        // R2.
        assert_eq!(
            check(&[&w], &[block("S"), block("s")]),
            "two data blocks are named 's'"
        );
        // R3.
        assert_eq!(
            check(&[&w], &[block("W")]),
            "data block 'W' has the name of a program of this runtime"
        );
        // R4.
        for bad in [
            "0123456789ABCDEF",
            "0123456789abcde",
            "0123456789abcdef0",
            "0123456789abcdeg",
        ] {
            let mut b = block("S");
            b.layout = bad.into();
            assert_eq!(
                check(&[&w], &[b]),
                format!("data block 'S': its layout '{bad}' is not 16 lowercase hex digits")
            );
        }
        // R5, without overflowing.
        let mut b = block("S");
        b.members[1].offset = 5;
        assert_eq!(
            check(&[&w], &[b.clone()]),
            "data block 'S': member 'b' lies outside its 8 bytes"
        );
        b.members[1].offset = u32::MAX;
        assert_eq!(
            check(&[&w], &[b]),
            "data block 'S': member 'b' lies outside its 8 bytes"
        );
        // R5a, members that overlap: `a` widened over `b`, either way round, and checked before
        // any program's use (whose members must equal the runtime's anyway).
        let mut b = block("S");
        b.members[0].size = 8;
        assert_eq!(
            check(&[&w], &[b.clone()]),
            "data block 'S': members 'a' and 'b' overlap"
        );
        b.members.swap(0, 1);
        assert_eq!(
            check(&[&w], &[b]),
            "data block 'S': members 'b' and 'a' overlap"
        );
        // Touching is not overlapping; a member of no bytes overlaps none.
        let mut b = block("S");
        b.members[0].size = 4;
        b.members.push(DataBlockMember {
            name: "empty".into(),
            ty: "T0".into(),
            offset: 4,
            size: 0,
        });
        assert_eq!(check_data(&[], &[b]), Ok(()));
        // R6.
        let mut twice = user("w", "S", &[]);
        let mut again = twice.data[0].clone();
        again.name = "s".into();
        twice.data.push(again);
        assert_eq!(
            check(&[&twice], &[block("S")]),
            "program 'w' declares data block 's' twice"
        );
        // R7.
        assert_eq!(
            check(&[&w], &[block("Other"), block("More")]),
            "program 'w' uses data block 'S', which is not in the runtime's data (it has: Other, More)"
        );
        assert_eq!(
            check(&[&w], &[]),
            "program 'w' uses data block 'S', which is not in the runtime's data (it has none)"
        );
        // R8: layout, then size.
        let mut other = block("S");
        other.layout = "fedcba9876543210".into();
        assert_eq!(
            check(&[&w], &[other]),
            "program 'w' was built with data block 'S' laid out as 0123456789abcdef (8 bytes), and the runtime's 'S' is laid out as fedcba9876543210 (8 bytes): build 'w' again"
        );
        let mut bigger = block("S");
        bigger.size = 12;
        assert_eq!(
            check(&[&w], &[bigger]),
            "program 'w' was built with data block 'S' laid out as 0123456789abcdef (8 bytes), and the runtime's 'S' is laid out as 0123456789abcdef (12 bytes): build 'w' again"
        );
        // R9: the same hash and size, and another type, or another member's name, type,
        // offset or size, or one member fewer.
        let r9 = "program 'w' was built with data block 'S' of type T with other members than the runtime's";
        let mut renamed = block("S");
        renamed.ty = "U".into();
        assert_eq!(check(&[&w], &[renamed]), format!("{r9} U: build 'w' again"));
        for change in 0..5 {
            let mut b = block("S");
            match change {
                0 => b.members[1].name = "c".into(),
                1 => b.members[1].ty = "REAL".into(),
                2 => b.members[1].offset = 2,
                3 => b.members[1].size = 2,
                _ => {
                    b.members.pop();
                }
            }
            assert_eq!(
                check(&[&w], &[b]),
                format!("{r9} T: build 'w' again"),
                "{change}"
            );
        }
        // R10.
        let mut constant = user("w", "S", &["b"]);
        constant.data[0].constant = true;
        assert_eq!(
            check(&[&constant], &[block("S")]),
            "program 'w' declares data block 'S' CONSTANT and writes its member 'b'"
        );
        let mut reads = user("w", "S", &[]);
        reads.data[0].constant = true;
        assert_eq!(check_data(&[&reads], &[block("S")]), Ok(()));
        // R11.
        let (a, b2) = (user("a", "S", &["b"]), user("b2", "S", &["a", "b"]));
        assert_eq!(
            check(&[&a, &b2], &[block("S")]),
            "data block 'S': member 'b' is written by programs 'a' and 'b2'; a member has one writer"
        );
        // R12 and R13: a member's first segment, without regard to case; an output and a
        // read-only published binding are allowed.
        let mut bound = user("r", "S", &[]);
        bound.outputs.push(OutputBinding {
            variable: "S.a".into(),
            tag: "external.A.B.o".into(),
            safe: Value::from(0),
        });
        bound.published.push(PublishedBinding {
            variable: "S.b".into(),
            tag: "internal.R.b".into(),
            writable: false,
        });
        bound.inputs.push(InputBinding {
            variable: "Sx".into(),
            tag: "external.A.B.c".into(),
            of: InputOf::Value,
        });
        assert_eq!(check_data(&[&bound], &[block("S")]), Ok(()));
        let mut input = bound.clone();
        input.inputs.push(InputBinding {
            variable: "s.a".into(),
            tag: "external.A.B.c".into(),
            of: InputOf::Value,
        });
        assert_eq!(
            check(&[&input], &[block("S")]),
            "program 'r': s.a is a member of data block 'S', which only a program writes: a tag cannot be read into it"
        );
        let mut setpoint = bound.clone();
        setpoint.published.push(PublishedBinding {
            variable: "S[0]".into(),
            tag: "internal.R.x".into(),
            writable: true,
        });
        assert_eq!(
            check(&[&setpoint], &[block("S")]),
            "program 'r': S[0] is a member of data block 'S', which only a program writes: it cannot be a setpoint"
        );
    }

    #[test]
    fn data_has_a_fixed_shape_and_is_absent_from_a_configuration_of_today() {
        // A program of today, and a RETAIN value of today: no `data`, no `layout` key.
        let p = serde_json::to_value(program("p")).unwrap();
        assert!(p.get("data").is_none(), "{p}");
        let kept = RetainedValue {
            path: "n".into(),
            ty: "DINT".into(),
            bytes: "01000000".into(),
            layout: None,
        };
        assert_eq!(
            serde_json::to_value(&kept).unwrap(),
            json!({"path": "n", "type": "DINT", "bytes": "01000000"})
        );
        let read: RetainedValue =
            serde_json::from_value(json!({"path": "n", "type": "DINT", "bytes": "01000000"}))
                .unwrap();
        assert_eq!(read, kept);

        // Pinned literally.
        let w = user("w", "S", &["a"]);
        assert_eq!(
            serde_json::to_value(&w.data[0]).unwrap(),
            json!({"name": "S", "type": "T", "layout": HASH, "offset": 16, "size": 8,
                   "constant": false, "members": [
                       {"name": "a", "type": "INT", "offset": 0, "size": 2, "written": true},
                       {"name": "b", "type": "DINT", "offset": 4, "size": 4, "written": false}]})
        );
        assert_eq!(
            serde_json::to_value(block("S")).unwrap(),
            json!({"name": "S", "type": "T", "layout": HASH, "size": 8, "members": [
                {"name": "a", "type": "INT", "offset": 0, "size": 2},
                {"name": "b", "type": "DINT", "offset": 4, "size": 4}]})
        );
        let mut kept_block = block("S");
        kept_block.retain = true;
        assert_eq!(
            serde_json::to_value(kept_block).unwrap()["retain"],
            json!(true)
        );

        // `written` and `constant` are required; an unknown field is refused.
        let member = json!({"name": "a", "type": "INT", "offset": 0, "size": 2});
        assert!(serde_json::from_value::<DataMember>(member).is_err());
        let mut use_ = serde_json::to_value(&w.data[0]).unwrap();
        use_.as_object_mut().unwrap().remove("constant");
        assert!(serde_json::from_value::<DataUse>(use_).is_err());
        let mut extra = serde_json::to_value(block("S")).unwrap();
        extra["colour"] = json!("red");
        assert!(serde_json::from_value::<DataBlock>(extra).is_err());
    }

    #[test]
    fn a_module_says_it_hands_data_blocks_over_in_its_profile_or_does_not() {
        assert_eq!(
            DataSupport::of(&json!({"name": "pc", "data": {"tiers": [1]}})),
            Some(DataSupport { tiers: vec![1] })
        );
        assert_eq!(DataSupport::of(&json!({"name": "pc"})), None);
        assert_eq!(DataSupport::of(&Value::Null), None);
        assert_eq!(DataSupport::of(&json!({"data": true})), None);
    }

    fn task(name: &str, kind: TaskKind, programs: &[&str]) -> TaskConfig {
        TaskConfig {
            name: name.into(),
            kind,
            priority: 5,
            watchdog_ms: None,
            programs: programs.iter().map(|p| p.to_string()).collect(),
        }
    }

    fn cyclic(ms: u64) -> TaskKind {
        TaskKind::Cyclic {
            interval_ms: ms,
            phase_ms: 0,
        }
    }

    #[test]
    fn check_tasks_refuses_each_thing_it_can_get_wrong() {
        let (a, b, c) = (program("a"), program("b"), program("c"));
        let two = [&a, &b];
        let check = |programs: &[&RuntimeProgram], tasks: &[TaskConfig]| {
            check_tasks(programs, tasks).err().unwrap_or_default()
        };
        // What is right.
        assert_eq!(check_tasks(&[&a], &[]), Ok(()), "one program, implied task");
        assert_eq!(
            check_tasks(
                &two,
                &[
                    task("fast", cyclic(10), &["a"]),
                    task("boot", TaskKind::Startup {}, &["B"]),
                ]
            ),
            Ok(()),
            "a program is named without regard to case"
        );
        assert_eq!(
            check_tasks(
                &[&a, &b, &c],
                &[
                    task("s1", TaskKind::Startup {}, &["a"]),
                    task("s2", TaskKind::Startup {}, &["b"]),
                    task("free", TaskKind::Cycle { min_cycle_ms: 0 }, &["c"]),
                ]
            ),
            Ok(()),
            "several startup tasks are allowed"
        );

        assert_eq!(check(&[], &[]), "it runs no program");
        assert_eq!(
            check(&two, &[]),
            "it runs several programs and names no task: each program needs a task"
        );
        let a2 = program("A");
        assert_eq!(
            check(&[&a, &a2], &[task("t", cyclic(1), &["a"])]),
            "two programs are named 'A'"
        );
        let dotted = program("x.y");
        assert_eq!(
            check(&[&a, &dotted], &[task("t", cyclic(1), &["a", "x.y"])]),
            "'x.y' is not a program name: no '.', '[', ']' or spaces"
        );
        for bad in ["", "p q", "p[1]", "p]"] {
            let p = program(bad);
            assert!(
                check(&[&a, &p], &[task("t", cyclic(1), &["a"])]).contains("is not a program name"),
                "{bad:?}"
            );
        }
        // One program keeps whatever name it had: no variable is prefixed with it.
        assert_eq!(
            check_tasks(&[&dotted], &[task("t", cyclic(1), &["x.y"])]),
            Ok(())
        );
        for bad in ["", "a b", "a.b", &"x".repeat(65)] {
            assert_eq!(
                check(&[&a], &[task(bad, cyclic(1), &["a"])]),
                format!("'{bad}' is not a task name: use 1 to 64 letters, digits, '-' or '_'")
            );
        }
        assert_eq!(
            check(
                &two,
                &[task("t", cyclic(1), &["a"]), task("t", cyclic(1), &["b"])]
            ),
            "task name 't' is used more than once"
        );
        let mut high = task("t", cyclic(1), &["a"]);
        high.priority = 31;
        assert_eq!(
            check_tasks(&[&a], std::slice::from_ref(&high)),
            Ok(()),
            "the lowest"
        );
        high.priority = 32;
        assert_eq!(
            check(&[&a], &[high]),
            "task 't' has priority 32; priorities are 0 (highest) to 31"
        );
        assert_eq!(
            check(&[&a], &[task("t", cyclic(0), &["a"])]),
            "task 't' has interval_ms 0; it must be at least 1"
        );
        let longest = (i64::MAX as u64) / 1_000_000;
        assert_eq!(
            check_tasks(&[&a], &[task("t", cyclic(longest), &["a"])]),
            Ok(())
        );
        for (kind, field) in [
            (cyclic(longest + 1), "interval_ms"),
            (
                TaskKind::Cyclic {
                    interval_ms: 1,
                    phase_ms: u64::MAX,
                },
                "phase_ms",
            ),
            (
                TaskKind::Cycle {
                    min_cycle_ms: longest + 1,
                },
                "min_cycle_ms",
            ),
        ] {
            assert_eq!(
                check(&[&a], &[task("t", kind, &["a"])]),
                format!("task 't': {field} is too long")
            );
        }
        let mut watched = task("t", TaskKind::Startup {}, &["a"]);
        watched.watchdog_ms = Some(longest + 1);
        assert_eq!(
            check(&[&a], std::slice::from_ref(&watched)),
            "task 't': watchdog_ms is too long"
        );
        watched.watchdog_ms = Some(0);
        assert_eq!(
            check(&[&a], &[watched]),
            "task 't': watchdog_ms must be at least 1"
        );
        assert_eq!(
            check(
                &[&a],
                &[task("t", TaskKind::Event { tag: " ".into() }, &["a"])]
            ),
            "event task 't' names no tag"
        );
        assert_eq!(
            check(&[&a], &[task("t", cyclic(1), &[])]),
            "task 't' runs no program"
        );
        assert_eq!(
            check(&two, &[task("t", cyclic(1), &["a", "z"])]),
            "task 't' runs 'z', which is not a program of this runtime (it has a, b)"
        );
        assert_eq!(
            check(&[&a], &[task("t", cyclic(1), &["a", "A"])]),
            "task 't' names 'A' twice"
        );
        assert_eq!(
            check(
                &two,
                &[
                    task("t", cyclic(1), &["a", "b"]),
                    task("u", cyclic(1), &["b"])
                ]
            ),
            "program 'b' is in tasks 't' and 'u'; a program is in one task"
        );
        assert_eq!(
            check(&two, &[task("t", cyclic(1), &["a"])]),
            "program 'b' is in no task, so it would never run"
        );
    }

    #[test]
    fn a_task_has_a_fixed_shape_and_refuses_a_field_it_does_not_know() {
        let t = TaskConfig {
            name: "fast".into(),
            kind: cyclic(10),
            priority: 0,
            watchdog_ms: None,
            programs: vec!["a".into()],
        };
        assert_eq!(
            serde_json::to_value(&t).unwrap(),
            json!({"name": "fast", "kind": {"type": "cyclic", "interval_ms": 10, "phase_ms": 0},
                   "priority": 0, "programs": ["a"]})
        );
        let parsed: TaskConfig = serde_json::from_value(json!({
            "name": "e", "kind": {"type": "event", "tag": "external.A.B.c"},
            "priority": 3, "watchdog_ms": 50, "programs": ["a"]
        }))
        .unwrap();
        assert_eq!(
            parsed.kind,
            TaskKind::Event {
                tag: "external.A.B.c".into()
            }
        );
        assert_eq!(parsed.watchdog_ms, Some(50));
        assert_eq!(
            serde_json::to_value(TaskKind::Startup {}).unwrap(),
            json!({"type": "startup"})
        );
        assert_eq!(
            serde_json::from_value::<TaskKind>(json!({"type": "startup"})).unwrap(),
            TaskKind::Startup {}
        );
        let cycle: TaskKind = serde_json::from_value(json!({"type": "cycle"})).unwrap();
        assert_eq!(cycle, TaskKind::Cycle { min_cycle_ms: 0 });

        // A misspelt field is refused, not ignored: in the task, and in each kind.
        for wrong in [
            json!({"name": "t", "kind": {"type": "startup"}, "priority": 1, "programs": [],
                   "watchdog": 5}),
            json!({"name": "t", "kind": {"type": "cyclic", "interval_ms": 1, "phase": 5},
                   "priority": 1, "programs": []}),
            json!({"name": "t", "kind": {"type": "cycle", "min_cycle": 5},
                   "priority": 1, "programs": []}),
            json!({"name": "t", "kind": {"type": "event", "tag": "x", "edge": "falling"},
                   "priority": 1, "programs": []}),
            json!({"name": "t", "kind": {"type": "startup", "cold": true},
                   "priority": 1, "programs": []}),
            json!({"name": "t", "kind": {"type": "cyclic", "interval_ms": 1}, "programs": []}),
        ] {
            assert!(
                serde_json::from_value::<TaskConfig>(wrong.clone()).is_err(),
                "{wrong}"
            );
        }
    }

    #[test]
    fn a_runtime_state_of_today_has_exactly_todays_keys() {
        let state = RuntimeState {
            mode: RuntimeMode::Run,
            fault: None,
            program: "p".into(),
            version: "1".into(),
            scans: 1,
            last_scan_us: 2,
            max_scan_us: 3,
            overruns: 0,
            scan_age_us: Some(4),
            forced: Vec::new(),
            tasks: Vec::new(),
            programs: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(&state).unwrap(),
            json!({"mode": "run", "fault": null, "program": "p", "version": "1", "scans": 1,
                   "last_scan_us": 2, "max_scan_us": 3, "overruns": 0, "scan_age_us": 4,
                   "forced": []})
        );
        // What a module of today sends reads with no tasks.
        let old: RuntimeState = serde_json::from_value(json!({"mode": "stop", "program": "p",
            "version": "1", "scans": 0, "last_scan_us": 0, "max_scan_us": 0, "overruns": 0}))
        .unwrap();
        assert!(old.tasks.is_empty() && old.programs.is_empty());
    }

    #[test]
    fn a_module_says_it_runs_tasks_in_its_profile_or_does_not() {
        assert_eq!(
            TaskSupport::of(&json!({"name": "pc", "tasks": {"kinds": ["cyclic", "event"]}})),
            Some(TaskSupport {
                kinds: vec!["cyclic".into(), "event".into()]
            })
        );
        assert_eq!(TaskSupport::of(&json!({"name": "pc"})), None);
        assert_eq!(TaskSupport::of(&Value::Null), None);
        assert_eq!(TaskSupport::of(&json!({"tasks": "all"})), None);
        let p = program("p");
        assert_eq!(
            implied_task(&p),
            TaskConfig {
                name: "p".into(),
                kind: cyclic(100),
                priority: 31,
                watchdog_ms: None,
                programs: vec!["p".into()],
            }
        );
    }

    #[test]
    fn plugin_errors_keep_their_kind_and_message_across_the_wire() {
        let original = PluginError::InvalidConfiguration("slave id 0".into());
        let wire = WireError::from(&original);
        assert_eq!(wire.kind, ErrorKind::InvalidConfiguration);
        assert_eq!(PluginError::from(wire), original);
    }
}
