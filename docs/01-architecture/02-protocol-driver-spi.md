# MythOS Architecture Specification: Protocol Driver SPI

- **Document Identifier**: MYTHOS-ARCH-002
- **Version**: 1.3.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

> **Revision 1.3.0 (2026-10-24)** — Sections 4 to 7 are rewritten against the source. Revision 1.2.0 of those sections described the retired `mythos-driver-api` contract and named eight symbols that this workspace does not define: `DriverError`, `DriverContext`, `driver_id`, `config_schema`, `DriverConfigValidator`, `DriverHealthReport`, `DriverStatus`, and `DiscoveryCapableDriver`. All eight are removed. Section 3 was corrected in revision 1.2.0. This revision changes one sentence in section 3.4, which said that the host performs the shared-memory write. No such path exists.
>
> Each section now separates three things that the previous text merged: the contract that the code implements, the behaviour that the code does not yet implement, and the requirement that remains unmet. Read the status line under each heading before you build against it.

---

## Contract Status Summary

| Section | Subject | Implemented in | Status |
| :--- | :--- | :--- | :--- |
| 3 | Plugin and driver trait contracts | `crates/mythos-plugin-sdk/src/lib.rs` | Implemented |
| 4 | Opaque JSON configuration validation | `plugins/drivers/driver-modbus/src/config.rs` | Implemented, with four known gaps |
| 5 | Plugin registry and lifecycle | `crates/mythos-core/src/plugin/mod.rs` | Implemented |
| 5 | Panic containment, per-driver task, restart, cycle timeout | — | **Not implemented** |
| 6 | Discovery trait, coordinator, cancellation, timeouts | `crates/mythos-core/src/discovery/` | Implemented |
| 6 | Discovery progress channel and backpressure | — | **Not implemented** |
| 6 | Modbus TCP, EtherCAT, CANopen, and MQTT Sparkplug B discoverers | — | **Not implemented** |
| 7 | Device execution mode and zero-silent-fallback gate | `crates/mythos-core/src/device/mode.rs`, `crates/mythos-strucpp/src/quality_gate.rs` | Implemented as library types |
| 7 | Operator path to change device mode | — | **Not implemented** |

Every "Not implemented" row is a gap between this specification and the code. It is recorded here so that no reader mistakes a design target for a shipped contract.

---

## 1. Scope and Objective

This specification defines the Service Provider Interface (SPI) for communication protocol drivers in MythOS. 

Industrial environments contain diverse field communication protocols, including Modbus TCP/RTU, PROFINET, MQTT, Ethernet/IP, CANopen, BACnet, EtherCAT, and OPC UA. In legacy monolithic architectures, a malformed packet or slow serial transaction in one driver blocks the entire runtime. 

MythOS defines an **Isolated Protocol Driver SPI**. The design intent is that every protocol driver runs as an isolated actor, that no driver shares memory space or an execution thread with another driver, and that a driver reaches the Microkernel only through strongly typed asynchronous traits, opaque configuration, and a bounded shared-memory write token.

The trait contract in section 3 is implemented. The runtime isolation is not. Section 5 states which isolation mechanisms exist today and which remain open. Do not read this page as evidence that a driver fault is contained.

Furthermore, this specification defines the **Universal Protocol Auto-Discovery SPI** (`ProtocolDiscoverer`, section 6). A protocol module implements a non-blocking heuristic scan with cooperative cancellation and a bounded time budget. This capability allows commissioning technicians and autonomous AI agents to enumerate field devices without manual parameter guessing. Discovery is a `mythos-core` concern, not a plugin SDK concern, and the discovery trait is not an extension of `ProtocolDriver`.

---

## 2. Driver Lifecycle State Machine

```
              ┌────────────────────────────────────────────────┐
              │                                                │
              ▼                                                │
      +---------------+   validate_config() passes             │
      │ Uninitialized │ ─────────────────────────────────┐     │
      +---------------+                                  │     │
              ▲                                          ▼     │
              │ stop()                           +---------------+
              │                                  │  Configured   │
              │                                  +---------------+
              │                                          │
              │                                          │ start()
              │                                          ▼
      +---------------+   health_check() fails   +---------------+
      │   Degraded    │ <─────────────────────── │    Running    │
      +---------------+                          +---------------+
              │                                          │
              │ Unrecoverable error                      │ Panic / Fatal
              ▼                                          ▼
      +---------------+                          +---------------+
      │    Faulted    │ <─────────────────────── │   Restarting  │
      +---------------+                          +---------------+
```

**Status of this model.** `PluginHost` stores one boolean, `is_started`, and no state enumeration. Four of the six states are observable in the code:

- `Uninitialized` and `Configured` are owned by `register`, `initialize_plugin`, and `start_plugin`.
- `Running` means `is_started == true`.
- `Degraded` is observable through `health_check`. The Modbus driver counts consecutive poll errors and returns `Degraded` above zero and `Unhealthy` above three.
- `Faulted` and `Restarting` are not implemented. No supervisor detects a dropout, marks a driver faulted, or performs a backoff restart.

The model is also incomplete in one direction: `Running` says "tags updating into SHM", but no component writes driver values into the shared memory TagBus yet (sections 4 and 5.4). Read the diagram as the target lifecycle. Section 5.4 lists the gap between the target and the code.

### 2.1 State Definitions
1. **Uninitialized**: Driver struct created; no network sockets or serial ports opened.
2. **Configured**: Driver validated JSON configuration; internal buffers allocated; ready to connect.
3. **Running**: Field bus connected; cyclic polling active; tags updating into SHM.
4. **Degraded**: Frame dropouts or timeout rate exceeds threshold; driver actively retrying connection.
5. **Faulted**: Irrecoverable communication failure or configuration mismatch; driver stopped; diagnostic alert emitted.
6. **Restarting**: Supervisor executing exponential backoff reset.

---

## 3. Rust Trait Contracts (`mythos-plugin-sdk`)

The driver SPI is defined once, in `crates/mythos-plugin-sdk/src/lib.rs`. A driver implements the universal plugin lifecycle plus the driver-specific methods. The SDK exports no separate configuration-validator trait; validation is a method on `ProtocolDriver`.

### 3.1 Universal Plugin Lifecycle (`MythosPlugin`)

```rust
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use mythos_plugin_sdk::{
    PluginContext, PluginError, PluginHealthReport, PluginMetadata,
};

#[async_trait]
pub trait MythosPlugin: Send + Sync {
    /// Return the plugin manifest: id, name, version, author, type, and capabilities.
    fn metadata(&self) -> PluginMetadata;

    /// Initialize plugin state and validate dependencies before starting.
    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;

    /// Apply runtime configuration and commence background tasks.
    async fn start(&mut self, ctx: &PluginContext, config: &JsonValue) -> Result<(), PluginError>;

    /// Optional cyclic tick hook invoked by the microkernel host scheduler.
    async fn tick(&mut self, ctx: &PluginContext, delta_ms: u32) -> Result<(), PluginError> {
        Ok(())
    }

    /// Terminate execution tasks and release resources gracefully.
    async fn stop(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;

    /// Check operational health and report diagnostic status.
    async fn health_check(&self) -> Result<PluginHealthReport, PluginError>;
}
```

### 3.2 Protocol Driver SPI (`ProtocolDriver`)

```rust
#[async_trait]
pub trait ProtocolDriver: MythosPlugin {
    /// Parse and validate the driver-specific JSON configuration schema.
    fn validate_config(&self, config: &JsonValue) -> Result<(), PluginError>;

    /// Execute one bounded I/O polling cycle against configured target devices.
    /// Returns the number of field tags successfully ingested or updated.
    async fn poll_cycle(&mut self, ctx: &PluginContext) -> Result<u32, PluginError>;

    /// Write an output value to a field device target tag.
    async fn write_tag(
        &mut self,
        ctx: &PluginContext,
        tag_uri: &str,
        value: &JsonValue,
    ) -> Result<(), PluginError>;
}
```

### 3.3 Driver Health and Diagnostics

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginHealthReport {
    pub status: HealthStatus,
    pub message: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Stopped,
}
```

### 3.4 Plugin Context (`PluginContext`)

`PluginContext` is the only host handle a driver receives. It carries no logger, SHM writer, or event channel. The host grants capability strings, and the driver checks them before using a privileged resource:

```rust
#[derive(Debug, Clone)]
pub struct PluginContext {
    pub plugin_id: String,
    pub allowed_capabilities: Arc<HashSet<String>>,
    pub working_directory: String,
}

impl PluginContext {
    /// Return true when the capability scope is permitted for this plugin.
    pub fn has_capability(&self, capability: &str) -> bool;

    /// Return PluginError::CapabilityViolation when the scope is missing.
    pub fn check_capability(&self, capability: &str) -> Result<(), PluginError>;
}
```

A driver that publishes field values declares the `tagbus:write` capability in its manifest and calls `ctx.check_capability("tagbus:write")` during `init`, `start`, and each poll cycle. Drivers do not link `mythos-shm` directly. No shared-memory publish path is implemented yet: the reference driver caches values in a private map and returns a count from `poll_cycle` (see section 5.4).

---

## 4. Opaque JSON Configuration Validation

**Status: implemented.** The reference driver validates its own configuration. No JSON Schema artifact is generated.

### 4.1 The contract

Two SDK methods touch configuration:

```rust
// crates/mythos-plugin-sdk/src/lib.rs:222
fn validate_config(&self, config: &JsonValue) -> Result<(), PluginError>;

// crates/mythos-plugin-sdk/src/lib.rs:197
async fn start(&mut self, ctx: &PluginContext, config: &JsonValue) -> Result<(), PluginError>;
```

`validate_config` takes `&self`. The host can therefore test a candidate configuration without first stopping a running driver. `start` takes `&mut self`, and it owns the transition to the running state.

The `mythos-plugin-sdk` crate does not depend on `schemars`. It declares no `config_schema()` method, no `DriverConfigValidator` trait, and no `DriverError` type. Parsing uses `serde_json`, and semantic rules are methods on the driver's own configuration struct.

### 4.2 Validation stages

```text
                Raw JSON payload (serde_json::Value)
                              │
                              ▼
        ┌───────────────────────────────────────────┐
        │  Stage 1 — structural                     │
        │  serde_json::from_value::<DriverConfig>() │
        │    missing required field ────────────────┼──► PluginError::InvalidConfiguration
        │    wrong type or unknown enum value ──────┼──►
        └───────────────────┬───────────────────────┘
                            │ Ok(typed config)
                            ▼
        ┌───────────────────────────────────────────┐
        │  Stage 2 — semantic                       │
        │  DriverConfig::validate(&self)            │
        │    range, cross-field, and transport ─────┼──► PluginError::InvalidConfiguration
        │    tag namespace rules ───────────────────┼──► PluginError::InvalidTag
        └───────────────────┬───────────────────────┘
                            │ Ok(())
                            ▼
               driver stores its own typed config
```

Stage 1 rejects a structurally wrong payload. Stage 2 rejects a well-formed payload that is semantically unsafe. Both stages return `PluginError`. No driver opens a socket or a serial port before both stages pass.

| Failure | Error variant | Raised by |
| :--- | :--- | :--- |
| Field missing, wrong type, unknown enum value | `PluginError::InvalidConfiguration` | `serde_json::from_value` |
| Value out of range or inconsistent with a peer field | `PluginError::InvalidConfiguration` | `Config::validate` |
| Tag URI violates the namespace contract | `PluginError::InvalidTag` | `Config::validate` |
| Required capability not granted | `PluginError::CapabilityViolation` | `PluginContext::check_capability` |

### 4.3 Reference example: the Modbus driver

`plugins/drivers/driver-modbus/src/config.rs` is the worked example. The configuration is two nested structs. `serde` supplies a default for every field except `transport`, so the smallest valid payload names only the transport.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModbusTransport {
    Tcp,
    Rtu,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagMapping {
    /// Canonical tag URI (must begin with "external.")
    pub uri: String,
    /// Modbus register address (0-based)
    pub address: u16,
    /// Function code: 3 (Holding Registers) or 4 (Input Registers)
    pub function_code: u8,
    /// Data type: "u16", "i16", "f32" (spanning 2 registers)
    #[serde(default = "default_datatype")]
    pub data_type: String,
    /// Scaling multiplier applied to raw integer
    #[serde(default = "default_scale")]
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModbusDriverConfig {
    pub transport: ModbusTransport,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub serial_port: Option<String>,
    #[serde(default = "default_baud")]
    pub baud_rate: u32,
    #[serde(default = "default_slave_id")]
    pub slave_id: u8,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub mappings: Vec<TagMapping>,
}
```

`ModbusDriverConfig::validate` (`config.rs:84`) enforces these semantic rules:

| Rule | Rejection |
| :--- | :--- |
| `slave_id` must not be 0 | `InvalidConfiguration` |
| TCP transport requires a non-empty `host` | `InvalidConfiguration` |
| RTU transport requires a non-empty `serial_port` | `InvalidConfiguration` |
| Each mapping URI must start with `external.` | `InvalidTag` |
| Each mapping URI must have at least four dot-separated segments | `InvalidTag` |
| The second segment must not be `modbus`, `profinet`, or `mqtt` | `InvalidTag` |
| `function_code` must be 3, 4, or 6 | `InvalidConfiguration` |

The driver calls the same function from both entry points. `start` parses, validates, constructs the client, and only then sets `is_running = true`. `validate_config` parses and validates without mutating state.

### 4.4 The isolation guarantee

Configuration isolation is achieved by type, not by process:

1. Each driver declares its own configuration struct in its own crate.
2. The SDK passes configuration as `&JsonValue`. There is no shared configuration registry and no shared schema.
3. A parse or validation failure returns `PluginError` before the driver allocates a transport.
4. The driver stores the typed configuration in a private field. No other driver can read it.

This satisfies the type-safety part of SYS-REQ-011. It does not provide the runtime isolation that SYS-REQ-011 also implies. All drivers share one address space and one `PluginHost` lock, as section 5 describes.

### 4.5 Known gaps

| # | Gap | Effect |
| :--- | :--- | :--- |
| 4.5.1 | `TagMapping.data_type` is declared (`config.rs:25`) and never read anywhere in the workspace. | The field documents `u16`, `i16`, and `f32` with `f32` spanning two registers. `poll_cycle` always reads `quantity: 1` and treats the register as unsigned. An `f32` mapping silently returns the first register as an integer. |
| 4.5.2 | `validate` accepts `function_code == 6`, but `poll_cycle` sends a holding-register read (FC 3) for every code except 4. | A mapping declared with FC 6 is polled with FC 3. The configuration is accepted and then ignored. |
| 4.5.3 | `write_tag` always emits `WriteSingleRegister` (FC 6), whatever the mapping declares. | A read-only mapping (FC 3 or FC 4) can be written if a caller names its URI. |
| 4.5.4 | No driver publishes a machine-readable schema. | A UI panel or an AI agent cannot discover a driver's configuration shape. The previous revision of this document showed a `config_schema()` call. That method does not exist. |

---

## 5. Plugin Registry, Lifecycle, and Fault Isolation

**Status: the registry is implemented. Per-driver tasks, panic containment, automatic restart, and enforced cycle timeouts are not implemented.**

### 5.1 The registry

`crates/mythos-core/src/plugin/mod.rs` defines `PluginHost`. It is the only lifecycle owner in the workspace.

```rust
struct ManagedPlugin {
    plugin: Box<dyn MythosPlugin>,
    context: PluginContext,
    is_started: bool,
}

pub struct PluginHost {
    plugins: Arc<RwLock<HashMap<String, ManagedPlugin>>>,
}
```

| Method | Effect |
| :--- | :--- |
| `register(plugin, context)` | Inserts a plugin under `metadata().id`. Rejects a duplicate id with `InitializationFailed`. |
| `initialize_plugin(id)` | Calls `plugin.init(&context)`. |
| `start_plugin(id, config)` | Calls `plugin.start(&context, config)` and sets `is_started = true`. |
| `stop_plugin(id)` | Calls `plugin.stop(&context)` and clears `is_started`. |
| `health_check(id)` | Calls `plugin.health_check()`. |
| `health_check_all()` | Calls `health_check()` on every plugin. Converts an error into `Unhealthy`. |
| `list_plugins()` | Returns `metadata()` for every plugin. |
| `tick_all(delta_ms)` | Calls `tick()` on every started plugin. Logs and continues on error. |
| `hotswap_plugin(id, new, config)` | Stops the old plugin, initializes and starts the new one, then swaps it into the slot. |
| `install_from_package(path, dir, caps)` | Verifies a `.mythos-plugin` package and its capability claims. |

### 5.2 Concurrency behaviour

Every method takes the `RwLock` and holds the guard across the plugin `.await`:

```rust
let mut lock = self.plugins.write().await;
let managed = lock.get_mut(plugin_id).ok_or(...)?;
managed.plugin.start(&managed.context, config).await?;   // guard still held
```

This is a registry serialization point, not an isolation boundary. Three consequences follow:

1. The lock is shared by all plugins. This contradicts the "no shared locks between drivers" rule that revision 1.2.0 stated.
2. One slow `tick` blocks every other plugin, and blocks `health_check` and `list_plugins` too. A driver polled through the host would block in the same way. The host has no `poll_cycle` entry point today.
3. A panic inside a plugin invocation unwinds through the guard. `std::sync::PoisonError` does not apply because the lock is `tokio::sync::RwLock`, which does not poison. The panic unwinds into whichever task called `tick_all`, and the plugin is left with a stale `is_started` flag.

### 5.3 What the code does enforce

| Mechanism | Where | Effect |
| :--- | :--- | :--- |
| Capability gate | `PluginContext::check_capability` | A driver cannot use a privileged resource unless the host granted the capability string. |
| Typed error partition | `PluginError` (8 variants) | Every failure carries a category that a caller can match. |
| Per-plugin health report | `PluginHealthReport` | `health_check` returns `Healthy`, `Degraded`, `Unhealthy`, or `Stopped`, with a message and a timestamp. |
| Bounded-cycle convention | `ProtocolDriver::poll_cycle` | The method is defined as one bounded cycle. The driver returns a count, not a stream. |
| Hot swap | `PluginHost::hotswap_plugin` | A plugin can be replaced without restarting the host process. |
| Graceful stop | `PluginHost::stop_plugin` | The host can stop one plugin without touching the others. |

### 5.4 What is missing

The word "supervisor" in the module comment describes intent, not behaviour. The workspace contains no `catch_unwind` call, no per-plugin task, no restart counter, and no backoff.

| Required behaviour | Requirement | State |
| :--- | :--- | :--- |
| Restart a panicking actor within 100 ms without disturbing adjacent actors | SYS-REQ-001 | **Not implemented.** A panic unwinds into the calling task. No restart path exists. |
| Isolate a driver fault to that driver | SYS-REQ-010 acceptance test | **Not implemented.** All plugins share one process and one lock. |
| Enforce a per-cycle polling timeout between 10 ms and 10 000 ms, then mark tags `BAD_COMM_FAILURE` and proceed | SYS-REQ-012 | **Not implemented.** No timeout wraps `poll_cycle`. `PluginError::Timeout` exists and only the Modbus client raises it, for its own socket operation. |
| Deliver a dedicated task or pinned thread per driver | Section 1 of this document | **Not implemented.** `Box<dyn MythosPlugin>` is called inline by the host. |
| Use bounded MPSC channels so a slow consumer drops telemetry without blocking the network loop | Revision 1.2.0 of this document | **Not implemented.** No channel exists. Drivers return values from a synchronous call. |
| Restrict each driver to the tag slots in its own validated configuration | Revision 1.2.0 of this document, section 5.2 | **Not implemented.** No production component holds a `ShmWriterHandle`. Drivers cache values in a private `HashMap` and publish nothing. |

A reader who needs SYS-REQ-001, SYS-REQ-010, or SYS-REQ-012 to hold must treat this section as an open engineering item. The acceptance test for SYS-REQ-010 injects a driver crash and asserts that the core stays healthy. That test cannot pass against the current host.

### 5.5 The device-fault gate that does exist

One isolation boundary is implemented, and it is not in the plugin host. It is in the soft-logic runtime.

`crates/mythos-strucpp/src/quality_gate.rs` defines `QualityGate`. It owns the device execution mode, a map of latched quality codes keyed by tag URI, and an atomic simulation flag.

```rust
pub fn record_comm_loss(&self, tag_uri: &str) -> QualityCode   // latches BAD_COMM_FAILURE
pub fn clear_comm_loss(&self, tag_uri: &str)                   // releases the latch
pub fn get_latched_quality(&self, tag_uri: &str) -> Option<QualityCode>
pub fn validate_forcing_permitted(&self, tag_uri: &str) -> Result<(), StrucppError>
```

`TagSynchronizer::sync_inputs` (`mythos-strucpp/src/sync.rs:73`) consults the gate before it writes to located memory. When a tag carries a latched bad quality, the write is skipped and the PLC keeps its previous input value:

```rust
if let Some(latched) = quality_gate.get_latched_quality(&binding.tag_uri) {
    if !latched.is_good() {
        // skip: do not copy a stale value into %IX
        continue;
    }
}
```

This isolates a faulting field device from the control scan. It does not isolate a faulting driver from the host. Section 7 covers the gate in full.

---

## 6. Universal Protocol Auto-Discovery (`ProtocolDiscoverer`)

**Status: the trait, the coordinator, cancellation, and timeouts are implemented in `crates/mythos-core/src/discovery/`. The progress channel, the backpressure rule, and four of the six protocols that SYS-REQ-074 names are not implemented.**

### 6.1 Architectural rationale

Field commissioning in a brownfield plant starts with undocumented fieldbus configuration. Instruments arrive with unknown baud rate, parity, and slave address, or on an unknown IP subnet. A technician normally resolves this with a proprietary desktop tool or by reading DIP switches.

MythOS provides a discovery engine so that a technician or an AI agent can enumerate the field network without guessing parameters. SYS-REQ-074 defines two scanning modes:

- **Guided Discovery** probes a small high-probability candidate set first, and stops as soon as it finds devices.
- **Blind Bus Sweep** probes the full candidate range.

Every sweep is read-only. A discovery probe must never actuate a field device.

### 6.2 Trait contract

The discovery trait is defined in `crates/mythos-core/src/discovery/traits.rs`. It is **not** in `mythos-plugin-sdk`, and it is **not** called `DiscoveryCapableDriver`.

```rust
#[async_trait]
pub trait ProtocolDiscoverer: Send + Sync {
    /// Unique identifier of the protocol, e.g. "modbus-rtu", "profinet-dcp", "mdns".
    fn protocol_id(&self) -> &str;

    /// Execute the discovery probe on the configured target interface.
    async fn discover(
        &self,
        ctx: &DiscoveryContext,
    ) -> Result<Vec<DiscoveredDevice>, DiscoveryError>;
}
```

Two properties of this signature are load-bearing:

1. `discover` takes `&self`, not `&mut self`. A discoverer is shared across concurrent callers through `Arc<dyn ProtocolDiscoverer>`, and it cannot keep per-scan state in its own fields. Per-scan state belongs in `DiscoveryContext`.
2. `discover` returns a `Vec` when the sweep ends. It does not stream. Section 6.7 describes the consequence.

### 6.3 Data structures

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMode {
    GuidedDiscovery,
    BlindBusSweep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryTimeoutBounds {
    /// Single fast probe timeout. SYS-REQ-074 requires <= 1.0 s.
    pub fast_probe_timeout: Duration,
    /// Total scan timeout. SYS-REQ-074 requires <= 4.0 s for a guided sweep.
    pub total_scan_timeout: Duration,
}

impl Default for DiscoveryTimeoutBounds {
    fn default() -> Self {
        Self {
            fast_probe_timeout: Duration::from_millis(1000),
            total_scan_timeout: Duration::from_millis(4000),
        }
    }
}

#[derive(Clone)]
pub struct DiscoveryContext {
    pub mode: DiscoveryMode,
    pub target_interface: String,
    pub timeout_bounds: DiscoveryTimeoutBounds,
    pub cancel_token: CancellationToken,
    pub catalog: Option<Arc<DeviceCatalog>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    pub device_id: String,
    pub protocol: String,
    pub station_name: Option<String>,
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
    pub modbus_slave_id: Option<u8>,
    pub fingerprint_model: Option<String>,
    pub vendor: Option<String>,
    pub confidence: f32,
    pub properties: HashMap<String, String>,
}
```

`DiscoveredDevice` has no `suggested_config` field, no `raw_attributes` field, no `serial_number` field, and no `matched_catalog_package` field. Revision 1.2.0 documented all four. Open vendor and protocol facts belong in `properties`. A catalog match is written into `properties["catalog_manifest_id"]`.

The builder methods on `DiscoveryContext` are `with_catalog`, `with_timeouts`, and `with_cancellation_token`.

### 6.4 The coordinator

`DiscoveryEngine` (`discovery/engine.rs`) holds `HashMap<String, Arc<dyn ProtocolDiscoverer>>`.

```rust
pub fn register(&mut self, discoverer: Arc<dyn ProtocolDiscoverer>);
pub fn get(&self, protocol_id: &str) -> Option<&Arc<dyn ProtocolDiscoverer>>;
pub fn registered_protocols(&self) -> Vec<String>;
pub async fn run_discovery(
    &self,
    ctx: &DiscoveryContext,
    protocol_filter: Option<&str>,
) -> Result<Vec<DiscoveredDevice>, DiscoveryError>;
```

Behaviour of `run_discovery`:

| Filter | Behaviour |
| :--- | :--- |
| `Some(id)` where `id` is registered | Runs that discoverer. Propagates its error. |
| `Some(id)` where `id` is not registered | Returns `DiscoveryError::ProtocolError`. |
| `None` | Runs every registered discoverer **sequentially**, in `HashMap` iteration order. Skips a failing discoverer with a `tracing::warn!` and continues. Returns `DiscoveryError::Cancelled` immediately if the token is cancelled. |

Two consequences:

1. With `None`, one hanging discoverer delays every discoverer after it. The per-probe timeout applies inside a discoverer, not between them.
2. With `None`, a total failure of every discoverer still returns `Ok(vec![])`. An empty vector therefore means either "no devices" or "every protocol failed". A caller cannot distinguish the two. This is a reporting gap, not a safety defect: no false device is reported.

### 6.5 Implemented discoverers

| `protocol_id` | Type | Interface class | Status |
| :--- | :--- | :--- | :--- |
| `modbus-rtu` | `ModbusSweepDiscoverer` | Serial RS-485 | Implemented. Slave-ID axis only; see 6.9. |
| `profinet-dcp` | `DcpDiscoverer` | Ethernet Layer 2, EtherType `0x8892` | Implemented. |
| `mdns` | `MdnsDiscoverer` | IP multicast UDP `5353` | Implemented. |

`DcpDiscoverer::new(source_mac, transport)` wraps `DcpClient::identify_sweep`. `MdnsDiscoverer::new(transport)` queries `_mythos._tcp.local` and `_opcua-tcp._tcp.local` and assigns a fixed confidence of `0.9`. Both adapters map their protocol error type into `DiscoveryError`.

The protocol coverage in SYS-REQ-074 that is **not** implemented is listed in section 6.8.

### 6.6 Cancellation and time budgets

Cancellation is cooperative and real:

- `DiscoveryContext::cancel_token` is a `tokio_util::sync::CancellationToken`.
- `ModbusSweepDiscoverer` checks `ctx.cancel_token.is_cancelled()` before each slave-ID probe, and returns `DiscoveryError::Cancelled`.
- `MdnsDiscoverer` checks the token before each service-type query.
- `DiscoveryEngine` checks the token between discoverers when no filter is set.
- `DcpDiscoverer` relies on its internal `identify_sweep` timeout. It does not poll the token between frames.

Time budgets are real for the Modbus sweep. `ModbusSweepDiscoverer::discover` wraps the whole tier sequence in `tokio::time::timeout(ctx.timeout_bounds.total_scan_timeout, …)` and maps expiry to `DiscoveryError::Timeout`. Each individual probe receives `fast_probe_timeout`.

The default bounds are 1 000 ms per probe and 4 000 ms per scan. Both satisfy SYS-REQ-074. `DiscoveryTimeoutBounds` is a plain struct with public fields, so a caller can set a value that violates the requirement. No code rejects it.

Revision 1.2.0 claimed a teardown latency bound of 50 ms on cancellation. No code measures teardown latency. The claim is removed.

### 6.7 Progress reporting and backpressure

**Status: not implemented.** `ProtocolDiscoverer::discover` has no progress channel parameter. A caller learns nothing until the sweep returns.

Revision 1.2.0 specified a bounded `mpsc::Sender<ScanProgress>` with `try_send` for intermediate frames and `send().await` for completion frames, plus a `ScanProgress` struct carrying `progress_percent`, `devices_found`, and `is_complete`. None of that exists in `crates/mythos-core/src/discovery/`.

A UI that shows a live scan must therefore either poll the coordinator from a separate task or wait for the final `Vec`. The transport contract in `ModbusProbeTransport` also takes no progress sink, so adding a channel later requires a change to the trait and to every implementor.

### 6.8 Protocol coverage not implemented

SYS-REQ-074 names six protocols. Two are implemented: Modbus RTU and PROFINET. A third discoverer, mDNS, implements the routed-subnet fallback in SYS-REQ-043, which is a separate requirement.

| Protocol | Required behaviour | State |
| :--- | :--- | :--- |
| Modbus TCP | Subnet socket sweep with unit-identifier verification | **Not implemented.** No `modbus-tcp` discoverer is registered, and `ModbusSweepDiscoverer` reports `protocol_id = "modbus-rtu"`. Note the distinction: Modbus TCP *polling* works, because `ModbusClient` opens a `TcpStream` for `ModbusTransport::Tcp`. Modbus TCP *discovery* has no code path. |
| EtherCAT | ESC ring traversal and SII EEPROM reads | **Not implemented.** No EtherCAT module exists in the workspace. `ethercat` appears only as a reserved namespace word in `mythos-core/src/namespace.rs`. |
| CANopen | NMT heartbeat and node-guard monitoring | **Not implemented.** No CAN module exists in the workspace. |
| MQTT Sparkplug B | `NBIRTH` and `DBIRTH` state topic capture | **Not implemented.** The workspace has no MQTT client and no Sparkplug code. `mqtt` appears only as a reserved namespace word and as a prohibited tag-namespace token. |

BACnet IP and MSTP are not named by SYS-REQ-074. Revision 1.2.0 documented a BACnet strategy, and section 1 of this document lists BACnet among the protocols the platform addresses. No BACnet module exists; `bacnet` appears only as a reserved namespace word.

Revision 1.2.0 presented detailed heuristic strategies for CANopen, BACnet, EtherCAT, and Modbus TCP, including LSS Fastscan, SII EEPROM word offsets, and BACnet APDU handling. Those strategies are design notes. They are not contracts, and no code implements them.

### 6.9 Modbus sweep behaviour

`ModbusSweepDiscoverer::discover` implements tiered sweeps over a single axis, the slave ID:

| Mode | Tier 1 | Tier 2 | Tier 3 |
| :--- | :--- | :--- | :--- |
| `GuidedDiscovery` | Slave IDs 1–16, register 0 | If tier 1 is empty, slave IDs 17–32 | — |
| `BlindBusSweep` | Slave IDs 1–247, register 0 | — | — |

A slave counts as discovered when `probe_holding_register` returns `Ok`. The probe is a single FC 3 read of one register. A transport error, a timeout, and a Modbus exception are indistinguishable at this layer: all three arrive as `Err(String)` and the slave is skipped.

`ModbusSweepDiscoverer` also exposes `with_priority_baudrates` and `with_priority_parities`, and defaults them to `[9600, 19200, 4800, 115200]` and `["None", "Even"]`. **These fields are never read.** `ModbusProbeTransport::probe_holding_register(slave_id, register_address, timeout)` takes no serial parameters, so the discoverer has no way to apply them.

The practical result: the current engine sweeps slave IDs on whatever serial configuration the transport already has. It does not sweep baud rate or parity. SYS-REQ-074 requires "serial baud rate, parity, and slave address heuristic scanning". Two of the three axes are declared and not swept.

### 6.10 Catalog fingerprinting

`CatalogFingerprintEngine` (`crates/mythos-core/src/catalog/fingerprint.rs`) scores a probe response against manifests in `catalog/devices/`.

```rust
pub fn match_probe_response(
    &self,
    slave_id: u8,
    register_address: u16,
    register_value: u16,
) -> Vec<FingerprintMatch>;
```

The engine runs only when `DiscoveryContext.catalog` is `Some`. When a match exists, `ModbusSweepDiscoverer` copies `manifest_id`, `vendor`, and `model` out of the best match, stores the manifest id in `properties["catalog_manifest_id"]`, and averages the discovery confidence with the fingerprint confidence:

```rust
device.confidence = (device.confidence + best.confidence) / 2.0;
```

No code path provisions a device from a catalog match. The "single-click commissioning" flow in revision 1.2.0, which allocates tag slots and pre-maps register layouts, does not exist.

### 6.11 Wiring status and one open defect

`DiscoveryEngine` is constructed in two places: integration tests, and `McpContext::with_discovery_engine`. `crates/mythos-core/src/bin/mythos-server.rs` does not build an engine and registers no discoverer, so the production daemon has no discovery capability.

**Open defect.** The MCP tool `mythos_discover_devices` (`crates/mythos-mcp/src/tools/diag_tools.rs:89`) handles a missing engine by inventing a device:

```rust
} else {
    // Mock fallback discovered device when physical interface is unprivileged in container/test
    vec![mythos_core::discovery::DiscoveredDevice {
        device_id: "siemens-et200sp-01".to_string(),
        protocol: "profinet-dcp".to_string(),
        mac_address: Some("00:1B:1B:3A:4F:90".to_string()),
        ip_address: Some("192.168.1.10".to_string()),
        fingerprint_model: Some("6ES7155-6AU01-0BN0".to_string()),
        vendor: Some("Siemens AG".to_string()),
        confidence: 1.0,
        ..
    }]
}
```

An AI agent that calls this tool on a host with no discovery engine receives a Siemens ET200SP at a specific MAC and IP, with confidence 1.0, and no indication that the result is fabricated. This is the same defect class as SYS-REQ-096: a plausible value reported where no measurement exists. It is recorded here because this document is the trace for discovery behaviour. It requires a code change outside this document's scope.

---

## 7. Per-Device Simulation Mode and the Zero-Silent-Fallback Invariant

**Status: the mode types, the controller, and the quality gate are implemented and unit-tested. No operator path reaches them, and no driver consumes them.**

### 7.1 The invariant

In industrial control, a silent fallback corrupts the operator's picture of the plant. A severed RS-485 cable on a boiler must produce an alarm. It must never produce a synthetic temperature that looks plausible.

SYS-REQ-096 defines the rule:

1. **Default mode is physical.** Every device starts in `PhysicalHardware`.
2. **Failure is asserted, not hidden.** A missing port, a refused socket, or a frame timeout must mark the affected tags bad immediately.
3. **No automatic fallback.** No driver, gate, or host may move a device from physical to simulation on its own.
4. **Manual transition only.** A device enters simulation only when an authorized operator changes its mode explicitly.

The code enforces rules 1 and 3. It implements the mechanism for rule 2. Rule 4 has no operator surface yet (section 7.8).

### 7.2 The mode contract in `mythos-core`

`crates/mythos-core/src/device/mode.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceExecutionMode {
    /// Normal production state. Reads and writes bind to physical field hardware.
    /// Timeouts and communication loss raise an alarm. The device never falls back
    /// to synthetic values automatically.
    #[default]
    PhysicalHardware,

    /// Engineering and commissioning state. Manual injection supplies synthetic values.
    ManualSimulation,
}
```

The enum is `snake_case` on the wire. It is **not** `#[repr(u8)]`, and its discriminants are not stable. Revision 1.2.0 documented `#[repr(u8)]` with explicit values 0 and 1, and UPPERCASE serde names. Both were wrong.

```rust
pub enum DeviceModeError {
    PhysicalCommunicationFailure { device_id: String, reason: String },
    SimulationInjectionDisallowed(String),
    UnauthorizedTransition(String),
}

pub struct SimulationAuditEvent {
    pub event_id: u64,
    pub timestamp_ms: u64,
    pub device_id: String,
    pub previous_mode: DeviceExecutionMode,
    pub new_mode: DeviceExecutionMode,
    pub operator: String,
    pub reason: String,
}

pub struct DeviceModeController {
    device_id: String,
    mode: DeviceExecutionMode,
    audit_trail: Vec<SimulationAuditEvent>,
    mock_values: HashMap<String, f64>,
}
```

| Method | Behaviour |
| :--- | :--- |
| `new(device_id)` | Starts in `PhysicalHardware` with an empty audit trail. |
| `mode()` / `is_simulation()` | Reads the current mode. |
| `set_mode(target, operator, reason)` | Sets the mode unconditionally and appends a `SimulationAuditEvent` with a process-global `AtomicU64` id. Returns the new event. |
| `audit_trail()` | Returns the in-memory event slice. |
| `handle_comm_failure(reason)` | In `PhysicalHardware`, returns `Err(PhysicalCommunicationFailure)` and leaves the mode unchanged. In `ManualSimulation`, returns `Ok(())`. |
| `inject_mock_value(suffix, value)` | Rejects with `SimulationInjectionDisallowed` unless the mode is `ManualSimulation`. Otherwise stores the value. |
| `get_mock_value(suffix)` | Returns the injected value only in `ManualSimulation`. |
| `clear_mock_values()` | Empties the injection map. |

Note that `set_mode` does not validate the transition, and it does not reject a transition to the mode the device is already in. `DeviceModeError::UnauthorizedTransition` is declared and never constructed.

### 7.3 The second gate, and a duplicated enum

`crates/mythos-strucpp/src/quality_gate.rs` declares **its own** `DeviceExecutionMode` with the same two variant names. It has no serde attributes, no `Default`, and no relation to the `mythos-core` type.

Two independent public types with the same name and the same meaning are a drift risk. A value from one cannot be passed to the other, and a future change to one will not reach the other. `mythos-strucpp` does not depend on `mythos-core`, so neither type can absorb the other without a dependency decision.

`QualityGate` is the runtime gate used by the control scan:

| Method | Behaviour |
| :--- | :--- |
| `new(initial_mode)` | Creates the gate with an empty latch map. |
| `current_mode()` / `is_simulation()` | Reads mode and the atomic simulation flag. |
| `set_mode(new_mode)` | Sets mode and the simulation flag. Records no audit event. |
| `record_comm_loss(tag_uri)` | Latches `QualityCode::BAD_COMM_FAILURE` (`0x0018`) for that URI and returns the code. |
| `clear_comm_loss(tag_uri)` | Releases the latch. |
| `get_latched_quality(tag_uri)` | Returns the latched code, if any. |
| `validate_forcing_permitted(tag_uri)` | Returns `Err(StrucppError::ManualForcingRejected)` in `PhysicalHardware`, `Ok(())` in `ManualSimulation`. |

The gate is consumed in two places:

- `TagSynchronizer::sync_inputs` skips the copy into `%IX` for a tag with a latched bad quality (section 5.5).
- `TagSynchronizer::sync_outputs` stamps `UNCERTAIN_SUBSTITUTE` on output slots when the gate reports `ManualSimulation`, and `GOOD` otherwise.

Crucially, `record_comm_loss` does not write the quality word into the shared-memory slot. The code is latched in the gate's own map. The slot's `quality` field keeps whatever value the last writer gave it. An HMI that reads slot quality directly will not see the failure; only the control scan will.

### 7.4 Quality codes

`crates/mythos-shm/src/slot.rs` defines the word:

| Constant | Value | Meaning |
| :--- | :--- | :--- |
| `QualityCode::BAD_NON_SPECIFIC` | `0x0000` | Unspecified failure |
| `QualityCode::BAD_CONFIG_ERROR` | `0x0004` | Configuration rejected |
| `QualityCode::BAD_NOT_CONNECTED` | `0x0008` | No connection to the device |
| `QualityCode::BAD_DEVICE_FAILURE` | `0x000C` | Device reported a failure |
| `QualityCode::BAD_SENSOR_FAILURE` | `0x0010` | Sensor failure |
| `QualityCode::BAD_COMM_FAILURE` | `0x0018` | Communication failure |
| `QualityCode::UNCERTAIN_SUBSTITUTE` | `0x0044` | Value is a substitute, not measured |
| `QualityCode::UNCERTAIN_INITIAL_VALUE` | `0x0048` | No value acquired yet |
| `QualityCode::GOOD` | `0x00C0` | Measured value |

The constant is named `UNCERTAIN_SUBSTITUTE`, not `UNCERTAIN_SUBSTITUTED`. Revision 1.2.0 used the longer spelling, which does not compile.

`TagFlags::SIMULATION_ACTIVE` is defined as `1 << 8` in the same file. **No code sets this flag.** SYS-REQ-096 requires the simulation bit to be active on injected tags. It is not.

### 7.5 Behaviour matrix

| Condition | `PhysicalHardware` | `ManualSimulation` |
| :--- | :--- | :--- |
| Default at construction | Yes | No |
| Interface missing | `DeviceModeController::handle_comm_failure` returns `PhysicalCommunicationFailure`. | `Ok(())` — the failure is ignored. |
| Cable severed or poll timeout | `QualityGate::record_comm_loss` latches `BAD_COMM_FAILURE` (`0x0018`). `sync_inputs` stops forwarding the tag. | No caller invokes `record_comm_loss` in this mode. The method itself does not check the mode, so a call would latch the same code. |
| Quality word in the shared-memory slot | **Not written by the gate.** The latch is in-process. | `sync_outputs` stamps `UNCERTAIN_SUBSTITUTE` (`0x0044`) on output slots. |
| Manual injection | `inject_mock_value` returns `SimulationInjectionDisallowed`. `validate_forcing_permitted` returns `ManualForcingRejected`. | Accepted. |
| Automatic transition to simulation | Impossible. No call site exists. | — |
| Amber `[SIMULATION ACTIVE]` badge in the UI | **Not implemented.** No frontend code reads the mode. | **Not implemented.** |
| Audit record | A `set_mode` call appends an event. | Same. |

Rows marked "Not implemented" are required by SYS-REQ-096 and absent from the workspace.

### 7.6 The injection contract

Revision 1.2.0 documented `DriverContext::inject_simulated_tag(tag_id, TagValueRaw, device_mode)`, which wrote to shared memory through `self.shm_writer`. No such type and no such method exist.

The real injection surface is `DeviceModeController::inject_mock_value(tag_suffix: String, value: f64)`. It stores the value in a private `HashMap<String, f64>`. It does not touch shared memory and it does not notify subscribers. It accepts only `f64`, so an integer tag survives the round trip up to 2^53 and a `BOOL` has no representation at all.

A caller that wants an injected value to reach the control scan must read `get_mock_value` and write the slot itself. No code performs that step.

### 7.7 The audit trail

The trail is an in-memory `Vec<SimulationAuditEvent>` owned by a `DeviceModeController`. It has these properties:

- It is append-only for the life of the controller.
- It is lost when the process exits or the controller is dropped.
- It records every `set_mode` call, including a transition to the current mode.
- It is not chained, signed, or replicated.
- `event_id` comes from a process-global `AtomicU64`, so ids are unique within a process and not stable across restarts.

Revision 1.2.0 specified an append-only SQLite table `audit_events` with columns for operator, target URI, previous state, new state, and metadata JSON. **No SQLite database exists in the workspace.** No `audit_events` table is created anywhere.

`mythos-core::auth::MerkleAuditLedger` exists and is a real append-only Merkle-chained ledger. It is not connected to device-mode changes. The SYS-REQ-096 line in `specs/spec.md` that requires "mandatory Merkle audit logging" for variable forcing therefore has no implementation path today.

### 7.8 Wiring status

| Component | Reached from |
| :--- | :--- |
| `DeviceModeController` | Exported from `mythos_core`, unit-tested in `crates/mythos-core/tests/device_mode_tests.rs`. No other caller. |
| `QualityGate` | `TagSynchronizer::sync_inputs` and `sync_outputs`. Unit-tested in `crates/mythos-strucpp/tests/sync_and_gating_tests.rs`. |
| Any HTTP route | None. The API router has 9 routes, and none addresses device mode or simulation. |
| Any MCP tool | None. |
| Any driver | None. `ModbusDriver` has no mode field, and `mythos-plugin-sdk` does not expose one. |

The consequence is precise and worth stating plainly: SYS-REQ-096 requires "each external field device shall expose an independent, manual operational mode switch within its configuration inspector". The types for that switch exist, are tested, and are correct. The switch does not exist. An operator cannot change a device mode through the product today.

### 7.9 Requirement trace

| Requirement | Test evidence | Status |
| :--- | :--- | :--- |
| SYS-REQ-096: default is `PhysicalHardware` | `device_mode_tests.rs:6` | Satisfied for the library type |
| SYS-REQ-096: communication failure raises an error and does not change mode | `device_mode_tests.rs:13` | Satisfied for the library type |
| SYS-REQ-096: injection rejected in physical mode | `device_mode_tests.rs:33` | Satisfied for the library type |
| SYS-REQ-096: mode transition records an audit event and unlocks injection | `device_mode_tests.rs:43` | Satisfied for the library type |
| SYS-REQ-096: bad quality latches `BAD_COMM_FAILURE` and blocks the PLC write | `sync_and_gating_tests.rs:85` | Satisfied |
| SYS-REQ-096: per-device mode switch in the configuration inspector | — | **Not implemented** |
| SYS-REQ-096: amber simulation badge on Network View, Tag Explorer, and faceplates | — | **Not implemented** |
| SYS-REQ-096: `SIMULATION_ACTIVE` flag active on injected tags | — | **Not implemented** |
| SYS-REQ-096: Merkle-chained audit record for forcing | — | **Not implemented** |
| SYS-REQ-096: immediate `BAD_COMM_FAILURE` visible to an HMI reader | — | **Not implemented.** The latch is in-process; the slot quality word is unchanged. |

The acceptance test recorded in `specs/spec.md` for SYS-REQ-096 requires an operator to sever a connection and observe an immediate alarm. That test cannot pass until the mode switch, the slot-quality write, and the UI badge exist.
