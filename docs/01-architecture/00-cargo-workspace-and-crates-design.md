# MythOS Senior Programmer Design: Cargo Workspace and Crate Architecture

- **Document Identifier**: MYTHOS-ARCH-000
- **Version**: 1.1.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English
- **Change Authority**: ADR-008 (Universal Plugin Harness), revision recorded in Section 8.

---

## 1. Executive Summary

MythOS is an industrial-grade Supervisory Control and Data Acquisition (SCADA) and soft-logic runtime written in pure Rust. MythOS integrates the embedded Native STruC++ Runtime Engine (`crates/mythos-strucpp`), provides sub-millisecond lock-free shared memory tag distribution, hosts an isolated protocol driver SPI, and exposes a native Model Context Protocol (MCP) server for autonomous AI agent inspection and control.

To guarantee hard fault isolation, zero circular dependencies, fast incremental compilation, and deterministic memory safety, the codebase is structured as a modular Cargo Workspace.

---

## 2. Cargo Workspace Topology

### 2.1 Workspace Root Manifest (`Cargo.toml`)

```toml
[workspace]
resolver = "2"
members = [
    # Hardened Microkernel Core & SDK
    "crates/mythos-pal",
    "crates/mythos-core",
    "crates/mythos-shm",
    "crates/mythos-strucpp",
    "crates/mythos-plugin-sdk",
    "crates/mythos-mcp",
    "crates/mythos-mcp-macros",
    "crates/mythos-cli",

    # Universal Backend Plugins ("Everything is a Plugin")
    "plugins/drivers/driver-modbus",

    # Desktop Shell
    "src-tauri",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.82.0"
authors = ["adrianaryaputra <adrian@firmansyah.co>"]
license = "MPL-2.0"

[workspace.dependencies]
# Concurrency and Async
tokio = { version = "1.40", features = ["full"] }
tokio-stream = "0.1"
async-trait = "0.1"
flume = "0.11"
parking_lot = "0.12"

# Web, Networking and API Gateway
axum = { version = "0.7", features = ["macros", "ws"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["fs", "cors", "trace"] }

# Serialization, CRDT and Collaboration
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bytemuck = { version = "1.18", features = ["derive"] }
schemars = "0.8"
specta = "2.0"
ts-rs = "10.0"
yrs = { version = "0.21", features = ["sync"] }

# Logging and Diagnostics
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
thiserror = "1.0"

# Low-level and Systems
libc = "0.2"
memmap2 = "0.9"
libloading = "0.8"
crc32fast = "1.4"
```

---

## 3. Crate Dependency DAG (Directed Acyclic Graph)

```
                       ┌─────────────────────────┐
                       │     mythos-core         │
                       │   (Microkernel Host)    │
                       └────────────┬────────────┘
                                    │
           ┌────────────────────────┼────────────────────────┐
           ▼                        ▼                        ▼
┌────────────────────┐   ┌────────────────────┐   ┌────────────────────┐
│   mythos-strucpp   │   │     mythos-mcp     │   │  mythos-plugin-sdk │
│ (Native STruC++)   │   │   (MCP AI Server)  │   │ (Plugin & Driver)  │
└──────────┬─────────┘   └──────────┬─────────┘   └────────────────────┘
           │                        │                     leaf crate
           │                        ▼
           │             ┌────────────────────┐
           │             │ mythos-mcp-macros  │
           │             └────────────────────┘
           │                        │
           └────────────────────────┤
                                    │
           ┌────────────────────────┴────────────────────────┐
           ▼                                                 ▼
┌────────────────────┐                             ┌────────────────────┐
│     mythos-shm     │                             │     mythos-pal     │
│ (Triple-Buffer Bus)│                             │ (OS Abstraction)   │
└──────────┬─────────┘                             └────────────────────┘
           │                                                 ▲
           └─────────────────────────────────────────────────┘
```

Universal plugins depend on the SDK only. `plugins/drivers/driver-modbus` declares
`mythos-plugin-sdk` and no kernel crate. The `mythos-cli` lifecycle binary depends on
`mythos-core` and `mythos-plugin-sdk`. The `src-tauri` desktop shell depends on `mythos-core`,
`mythos-pal`, and `mythos-shm`.

### 3.1 Crate Dependency Rules
1. **Zero Downward Pollution**: `mythos-shm`, `mythos-pal`, and `mythos-plugin-sdk` are bottom-tier leaf crates. `mythos-plugin-sdk` and `mythos-pal` depend only on standard library and minimal zero-cost dependencies (`bytemuck`, `serde`, `thiserror`, `libc`, `memmap2`). They never depend on `mythos-core`.
2. **Driver Isolation**: Specific protocol drivers (`plugins/drivers/driver-modbus`, `plugins/drivers/driver-mqtt`) depend solely on `mythos-plugin-sdk`. Drivers never depend on each other or on `mythos-core`. The SDK exposes the `PluginContext` capability handle in place of a kernel-provided driver context type.
3. **STruC++ Engine Isolation**: `crates/mythos-strucpp` interacts with the rest of the system strictly through `mythos-shm` memory maps and `mythos-pal` platform traits without external PLC daemons.

---

## 4. Crate Specifications and Roles

### 4.1 `mythos-shm` (Lock-Free Shared Memory Bus)
- **Role**: High-performance, zero-copy, lock-free inter-process shared memory bus.
- **Backing Medium**: POSIX shared memory object (`/dev/shm/mythos_tagbus`).
- **Core Abstractions**:
  * `TripleBuffer<T>`: Single-writer, multi-reader lock-free atomic pointer swap mechanism.
  * `TagSlot`: 64-byte cache-line aligned Plain Old Data (POD) slot storing tag values, quality codes, and 64-bit nanosecond timestamps.
  * `ShmMaster`: Lifecycle manager that creates, sizes, and zeroes the shared memory segment.
  * `ShmReader`: Unprivileged, read-only consumer client mapping the bus into user space.
  * `RingBuffer`: Lock-free single-producer multi-consumer diagnostic and event stream buffer.

### 4.2 `mythos-plugin-sdk` (Universal Plugin Contract & Driver SPI)
- **Role**: Single contract crate for every MythOS extension. It defines the universal plugin lifecycle and the protocol driver SPI. All error types are `PluginError`.
- **Core Abstractions**:
  * `trait MythosPlugin: Send + Sync`: Universal plugin lifecycle. Methods are `metadata`, `init`, `start`, `tick`, `stop`, and `health_check`.
  * `trait ProtocolDriver: MythosPlugin`: Driver SPI. Methods are `validate_config(&self, &JsonValue)`, `poll_cycle(&mut self, &PluginContext) -> Result<u32, PluginError>`, and `write_tag(&mut self, &PluginContext, tag_uri, value)`.
  * `trait Plugin: MythosPlugin`: Blanket-implemented trait alias for backward compatibility.
  * `struct PluginContext`: Host runtime interface carrying `plugin_id`, `allowed_capabilities`, and `working_directory`. Methods are `has_capability` and `check_capability`.
  * `enum PluginError`: Structured lifecycle, configuration, capability, timeout, tag, and communication errors.
  * `enum PluginType`, `enum HealthStatus`, `struct PluginHealthReport`, `struct PluginMetadata`: Plugin classification, health reporting, and manifest identity.

Opaque JSON configuration validation is a responsibility of `ProtocolDriver::validate_config`. The SDK defines no separate validator trait.

### 4.3 `crates/mythos-strucpp` (Native STruC++ Runtime Engine)
- **Role**: Structured Text compilation pipeline and dynamic in-process runtime execution harness based on the STruC++ architecture, executing soft-logic without external PLC daemons.
- **Core Abstractions**:
  * `CompilerPipeline`: Automates compilation of IEC 61131-3 Structured Text to C++17 via STruC++ (`strucpp compile`), generating `generated.cpp`, `generated.hpp`, `generated_debug.cpp`, and `debug-map.json`, compiled to a dynamic shared object (`.so`) with Clang/G++.
  * `DynamicRuntimeLoader`: Safe dynamic library loader (`libloading::Library`) resolving the C-linkage configuration entrypoint (`strucpp_get_config`), lock plumbing (`strucpp_set_locks`), and hierarchical debug exports (`strucpp_debug_*`), enabling atomic hot-reloading during cyclic dead-time.
  * `VirtualDispatchHost`: Traverses `ConfigurationInstance` -> `ResourceInstance` -> `TaskInstance` -> `ProgramBase` via native C++ virtual dispatch to invoke cyclic execution.
  * `RealtimeTaskHost`: Configures thread-per-task execution under Linux `SCHED_FIFO` (priority 80-99) and user-declared CPU core pinning (`pthread_setaffinity_np`).
  * `IoBufferBridge`: Direct memory bridge between STruC++ `strucpp::locatedVars[]` (`LocatedVar`) pointers and MythOS POSIX shared memory bus (`/dev/shm/mythos_tagbus`).
  * `HierarchicalDebugEngine`: Bridges `debug_dispatch.hpp` PDU handlers and `IECVar<T>` forcing APIs with MythOS diagnostic and MCP inspection tools.

### 4.4 `mythos-mcp` and `mythos-mcp-macros` (Model Context Protocol Subsystem)
- **Role**: Native AI integration engine exposing real-time SCADA state and control tools to autonomous LLM agents.
- **Transports**: Stdio (process standard input/output) and Server-Sent Events (SSE) over HTTP.
- **Core Abstractions**:
  * `McpServer`: JSON-RPC 2.0 router and session state manager.
  * `#[mythos_mcp_tool]`: Procedural macro generating runtime tool manifests, JSON Schema parameter definitions, and dispatch hooks.
  * `ToolRegistry`: In-memory thread-safe inventory of registered diagnostic, control, and topology tools.
  * `SafetyInterlockGuard`: Authorization barrier validating role-based access control (RBAC), tag write boundaries, and simulation modes before control dispatch.

### 4.5 `mythos-core` (Microkernel Host Daemon)
- **Role**: Process supervisor, actor coordinator, configuration engine, and REST/WebSocket edge interface.
- **Core Abstractions**:
  * `KernelSupervisor`: Spawns and monitors driver worker threads, Native STruC++ real-time host, and MCP server.
  * `TagRegistry`: Semantic namespace resolver mapping `external.<namespace>.<device>.<variable>` string URIs to numeric SHM slot indices.
  * `NvramWalEngine`: Persistence engine writing `RETAIN` and `PERSISTENT` tags to `/var/lib/mythos/nvram.dat`.
  * `ApiGateway`: Axum-based HTTP/WebSocket server exposing telemetry and Server-Driven UI (SDUI) contracts.

### 4.6 `plugins/drivers/driver-modbus` (Modbus TCP/RTU Protocol Driver)
- **Role**: Industrial Modbus protocol client implementing `ProtocolDriver` from `mythos-plugin-sdk`.
- **Crate Dependencies**: `mythos-plugin-sdk` only. The driver never depends on `mythos-core`, `mythos-shm`, or another driver.
- **Features**:
  * Modbus TCP over tokio socket channels with keep-alive and auto-reconnect.
  * Modbus RTU over RS-485 serial ports (`tokio-serial`) with inter-frame silent interval enforcement.
  * Cyclic polling scheduler with batch register aggregation (Function Codes 01, 02, 03, 04).
  * Tag publication requested through the `tagbus:write` capability; the host performs the SHM write.

### 4.7 `plugins/drivers/driver-mqtt` (MQTT / Sparkplug B Edge Driver — Planned)
- **Status**: Directory scaffolded with a `.gitkeep` placeholder. No crate manifest exists yet, so the workspace does not build it.
- **Role**: Cloud and edge broker synchronizer.
- **Features**:
  * MQTT v3.1.1 / v5.0 client backed by `rumqttc`.
  * Sparkplug B protocol payload decoding and encoding (Google Protocol Buffers).
  * Store-and-forward SQLite buffer during broker connection dropouts.

Add `plugins/drivers/driver-mqtt` to `workspace.members` when its crate manifest lands.

### 4.8 `mythos-pal` (Platform Abstraction Layer)
- **Role**: Operating-system agnosticism. Every OS-specific call is hidden behind compile-time `#[cfg(target_os = "...")]` branches and safe Rust traits.
- **Crate Dependencies**: Standard library, `libc`, `memmap2`, `cfg-if`, `libloading`, `crc32fast`, `thiserror`. Leaf crate.
- **Core Abstractions**: Platform error type (`error`), dynamic library loading (`library`), OS scheduling and affinity (`scheduler`), shared-memory primitives (`shm`), and the unified platform traits (`traits`).
- **Consumers**: `mythos-shm`, `mythos-core`, `mythos-strucpp`, and `mythos-mcp`.

### 4.9 `mythos-cli` (Lifecycle, Plugin, and Distribution Toolchain)
- **Role**: Operator-facing binary named `mythos`. It manages releases, plugin lifecycle commands, update channels, and rollback.
- **Crate Dependencies**: `mythos-core`, `mythos-plugin-sdk`, `clap`.
- **Core Modules**: `channel` (release channels), `config`, `plugin_cmd`, `rollback`, `update`, and `versions`.

---

## 5. Unified Error Hierarchy (`mythos-core::error`)

> **Implementation note (2026-10-23)**: The aggregating `MythosError` type and the `mythos-core::error` module below are a design target, not current code. Error types currently live per crate: `ShmError` in `crates/mythos-shm/src/error.rs`, `StrucppError` in `crates/mythos-strucpp/src/error.rs`, `McpError` in `crates/mythos-mcp/src/lib.rs`, `NvramError` in `crates/mythos-core/src/nvram.rs`, and `PluginError` in `crates/mythos-plugin-sdk/src/lib.rs`. `DriverError` is superseded by `PluginError`. Do not write code against `MythosError` until the aggregation module lands.

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MythosError {
    #[error("Shared memory error: {0}")]
    ShmError(#[from] ShmError),

    #[error("Driver error [{driver_id}]: {source}")]
    DriverError {
        driver_id: String,
        #[source]
        source: DriverError,
    },

    #[error("Native STruC++ runtime error: {0}")]
    StrucppError(#[from] StrucppError),

    #[error("MCP protocol error: {0}")]
    McpError(#[from] McpError),

    #[error("Namespace addressing error: {0}")]
    NamespaceError(#[from] NamespaceError),

    #[error("NVRAM WAL persistence error: {0}")]
    NvramError(#[from] NvramError),

    #[error("Security or Safety Interlock violation: {0}")]
    SecurityError(String),

    #[error("System I/O failure: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum ShmError {
    #[error("Failed to create POSIX SHM object '{name}': {source}")]
    CreationFailed { name: String, source: std::io::Error },

    #[error("Failed to map memory: {0}")]
    MmapFailed(#[source] std::io::Error),

    #[error("Slot index {0} is out of bounds (max: {1})")]
    SlotOutOfBounds(usize, usize),

    #[error("SHM version mismatch: header={found}, expected={expected}")]
    VersionMismatch { found: u32, expected: u32 },

    #[error("Corrupt SHM header detected: magic mismatch")]
    CorruptedHeader,
}

#[derive(Error, Debug)]
pub enum DriverError {
    #[error("Configuration validation failed: {0}")]
    ConfigValidationFailed(String),

    #[error("Connection timeout after {0:?}")]
    ConnectionTimeout(std::time::Duration),

    #[error("Protocol parsing failure: {0}")]
    ProtocolViolation(String),

    #[error("Device communication error: {0}")]
    DeviceFailure(String),

    #[error("Driver watchdog timeout: driver did not heartbeat within {0:?}")]
    WatchdogExpired(std::time::Duration),
}

#[derive(Error, Debug)]
pub enum StrucppError {
    #[error("Structured Text STruC++ compilation failed: {0}")]
    CompilationFailed(String),

    #[error("C++ compilation failed: {0}")]
    CompilationFailed(String),

    #[error("Dynamic library load failed: {0}")]
    DlOpenFailed(String),

    #[error("Symbol resolution failed for '{symbol}': {source}")]
    SymbolNotFound { symbol: String, source: String },

    #[error("Cycle overrun detected: target={target_us}us, actual={actual_us}us")]
    CycleOverrun { target_us: u64, actual_us: u64 },

    #[error("Realtime thread scheduling failure: {0}")]
    SchedFifoFailed(String),
}

#[derive(Error, Debug)]
pub enum McpError {
    #[error("Tool '{0}' is not registered")]
    ToolNotFound(String),

    #[error("Invalid tool arguments: {0}")]
    InvalidArguments(String),

    #[error("Execution rejected by Safety Interlock: {0}")]
    InterlockViolation(String),

    #[error("JSON-RPC parsing error: {0}")]
    JsonRpc(String),
}

#[derive(Error, Debug)]
pub enum NamespaceError {
    #[error("Invalid tag URI format '{0}': must follow 'external.<namespace>.<device>.<variable>'")]
    InvalidFormat(String),

    #[error("Tag path exceeded maximum depth of {0}")]
    MaxDepthExceeded(usize),

    #[error("Tag '{0}' is not registered in the system dictionary")]
    TagNotFound(String),

    #[error("Tag '{0}' has read-only access mode")]
    ReadOnlyAccess(String),
}

#[derive(Error, Debug)]
pub enum NvramError {
    #[error("WAL file open error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("WAL CRC32 checksum mismatch at epoch {epoch}, offset {offset}")]
    ChecksumMismatch { epoch: u64, offset: u64 },

    #[error("NVRAM partition full (capacity: {0} bytes)")]
    PartitionFull(usize),
}
```

---

## 6. Concurrency and Safety Guarantees

1. **Send + Sync Bounds**: All inter-thread message payloads and shared driver contexts implement `Send + Sync + 'static`.
2. **Zero Dynamic Allocation in Hard Real-Time Loops**:
   - The Native STruC++ execution loop and the SHM cyclic update loops pre-allocate all buffers during initialization.
   - Zero `Vec` resizing, zero `String` formatting, and zero `Box::new` calls occur during the cyclic scanning window.
3. **Panic Containment**:
   - Protocol drivers and MCP tool calls execute within caught unwinding boundaries (`std::panic::catch_unwind`).
   - A driver panic isolates the faulty driver actor, triggers a clean diagnostic log, and restarts the driver via exponential backoff without crashing `mythos-core`.

---

## 7. Legacy Name Cross-Reference

Earlier revisions of this suite, and ADRs dated before 2026-10-14, use the names in the left column. Use the right column in new code and new documents. ADRs keep their original wording plus a superseded-by note, because an ADR records what was decided at the time.

| Legacy name | Current name | Current location | Notes |
|---|---|---|---|
| `crates/mythos-driver-api` | `crates/mythos-plugin-sdk` | `Cargo.toml` members | The driver SPI became part of the universal plugin SDK under ADR-008. |
| `drivers/driver-modbus` | `plugins/drivers/driver-modbus` | workspace member | Plugin code lives under `plugins/`. |
| `drivers/driver-mqtt` | `plugins/drivers/driver-mqtt` | scaffold only (`.gitkeep`) | Not a workspace member until its manifest lands. |
| `mythos-net-dcp` | `plugins/engines/engine-discovery-dcp` | scaffold only (`.gitkeep`) | Planned DCP discovery engine. |
| `struct DriverContext` | `struct PluginContext` | `crates/mythos-plugin-sdk/src/lib.rs` | Capability-based context. It carries no logger, SHM writer, or event channel. |
| `enum DriverError` | `enum PluginError` | `crates/mythos-plugin-sdk/src/lib.rs` | |
| `struct DriverHealthReport`, `enum DriverStatus` | `struct PluginHealthReport`, `enum HealthStatus` | `crates/mythos-plugin-sdk/src/lib.rs` | `HealthStatus` adds `Unhealthy` and `Stopped` and drops `Faulted`. |
| `trait DriverConfigValidator` | *(none)* | — | This trait never existed in code. Configuration validation is `ProtocolDriver::validate_config`. It is deleted from Section 4.2, not renamed. |

---

## 8. Revision History

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | — | Neo (VP Software & Infrastructure) | Initial approved specification. |
| 1.1.0 | 2026-10-23 | Scribe (VP Publication & Documentation) | ADR-008 propagation. Corrected workspace members and crate/plugin paths. Replaced the fictional driver-api contract list with the real `mythos-plugin-sdk` contracts. Added Sections 4.8 and 4.9 for `mythos-pal` and `mythos-cli`. Flagged Section 5 as a design target. Added Section 7 cross-reference. |
