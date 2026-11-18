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
    /// The variables held at a value by [`Call::Force`]. Empty from a module that cannot force.
    #[serde(default)]
    pub forced: Vec<ForcedValue>,
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

    #[test]
    fn plugin_errors_keep_their_kind_and_message_across_the_wire() {
        let original = PluginError::InvalidConfiguration("slave id 0".into());
        let wire = WireError::from(&original);
        assert_eq!(wire.kind, ErrorKind::InvalidConfiguration);
        assert_eq!(PluginError::from(wire), original);
    }
}
