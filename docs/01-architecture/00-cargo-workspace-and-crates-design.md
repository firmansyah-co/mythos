# MythOS Senior Programmer Design: Cargo Workspace and Crate Architecture

- **Document Identifier**: MYTHOS-ARCH-000
- **Version**: 1.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

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
    "crates/mythos-core",
    "crates/mythos-shm",
    "crates/mythos-plugin-sdk",
    "crates/mythos-mcp",
    "crates/mythos-mcp-macros",

    # Universal Backend Plugins ("Everything is a Plugin")
    "plugins/drivers/driver-modbus",
    "plugins/drivers/driver-mqtt",
    "plugins/engines/engine-strucpp",
    "plugins/engines/engine-alarms",
    "plugins/engines/engine-recipes",
    "plugins/engines/engine-historian",
    "plugins/engines/engine-discovery-dcp",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.82.0"
authors = ["adrianaryaputra <adrian@firmansyah.co>"]
license = "Proprietary"

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
│   mythos-strucpp   │   │     mythos-mcp     │   │ drivers/driver-*   │
│ (Native STruC++)   │   │   (MCP AI Server)  │   │ (Modbus, MQTT, ..) │
└──────────┬─────────┘   └──────────┬─────────┘   └──────────┬─────────┘
           │                        │                        │
           │                        ▼                        │
           │             ┌────────────────────┐              │
           │             │ mythos-mcp-macros  │              │
           │             └────────────────────┘              │
           │                        │                        │
           └────────────────────────┼────────────────────────┘
                                    │
           ┌────────────────────────┴────────────────────────┐
           ▼                                                 ▼
┌────────────────────┐                             ┌────────────────────┐
│     mythos-shm     │                             │ mythos-driver-api  │
│ (Triple-Buffer Bus)│                             │  (Driver SPI & CTX)│
└────────────────────┘                             └────────────────────┘
```

### 3.1 Crate Dependency Rules
1. **Zero Downward Pollution**: `mythos-shm` and `mythos-driver-api` are bottom-tier leaf crates. They depend only on standard library and minimal zero-cost dependencies (`bytemuck`, `serde`, `thiserror`). They never depend on `mythos-core`.
2. **Driver Isolation**: Specific protocol drivers (`drivers/driver-modbus`, `drivers/driver-mqtt`) depend solely on `mythos-driver-api` and `mythos-shm`. Drivers never depend on each other or on `mythos-core`.
3. **STruC++ Engine Isolation**: `crates/mythos-strucpp` interacts with the rest of the system strictly through `mythos-shm` memory maps and `mythos-driver-api` event abstractions without external PLC daemons.

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

### 4.2 `mythos-driver-api` (Driver Service Provider Interface)
- **Role**: Contract definitions for isolated industrial communication drivers.
- **Core Abstractions**:
  * `trait ProtocolDriver`: Asynchronous lifecycle interface (`init`, `start`, `stop`, `health_check`).
  * `trait DriverConfigValidator`: Schema validator for opaque JSON driver configurations.
  * `struct DriverContext`: Host runtime interface exposing logger, SHM write token, and diagnostics channel.
  * `enum DriverEvent`: State change notifications, connection lifecycle events, and telemetry counters.
  * `enum DriverError`: Structured, recoverable vs non-recoverable protocol errors.

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

### 4.6 `drivers/driver-modbus` (Modbus TCP/RTU Protocol Driver)
- **Role**: Industrial Modbus protocol client implementing `ProtocolDriver`.
- **Features**:
  * Modbus TCP over tokio socket channels with keep-alive and auto-reconnect.
  * Modbus RTU over RS-485 serial ports (`tokio-serial`) with inter-frame silent interval enforcement.
  * Cyclic polling scheduler with batch register aggregation (Function Codes 01, 02, 03, 04).
  * Direct SHM tag writing without kernel context switches.

### 4.7 `drivers/driver-mqtt` (MQTT / Sparkplug B Edge Driver)
- **Role**: Cloud and edge broker synchronizer.
- **Features**:
  * MQTT v3.1.1 / v5.0 client backed by `rumqttc`.
  * Sparkplug B protocol payload decoding and encoding (Google Protocol Buffers).
  * Store-and-forward SQLite buffer during broker connection dropouts.

---

## 5. Unified Error Hierarchy (`mythos-core::error`)

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
