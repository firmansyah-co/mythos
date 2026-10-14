# MythOS Architecture Specification: Universal Plugin Harness and Extensibility

- **Document Identifier**: MYTHOS-ARCH-006
- **Version**: 2.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Executive Summary & The "Everything is a Plugin" Paradigm

MythOS eliminates monolithic software decay in industrial control and SCADA systems by adopting the **Universal Plugin Harness** architecture, inspired by the DeepSeek Harness and the VSCode/Eclipse RCP extension runtime.

### 1.1 The Zero-Domain Microkernel Principle
In MythOS, the Core Microkernel contains **zero application-domain logic**. The microkernel does not know what a PLC is, does not understand Modbus registers, does not hardcode alarm thresholds, and does not contain user interface screens.

The **MythOS Core Microkernel** provides only four foundational primitives:
1. **Lock-Free TagBus Engine**: Ultra-fast, triple-buffered shared memory (`/dev/shm/mythos_tagbus`) capable of $> 100,000$ tag updates/sec with $< 50\,\text{ns}$ atomic pointer swaps.
2. **Plugin Host & Supervisor**: An asynchronous Tokio/WASI actor supervisor that loads, initializes, isolates, schedules, monitors, and hot-reloads plugin extensions.
3. **Capability-Based Security Barrier**: A zero-trust capability validator enforcing memory quotas, CPU execution timeouts, and strict TagBus write fencing.
4. **Unified MCP Aggregator**: A Model Context Protocol registry that dynamically exposes tools, prompts, and resources from all registered plugins to autonomous AI agents.

**EVERY OTHER CAPABILITY IN MYTHOS IS A PLUGIN.** Both backend execution engines and frontend graphical user interface (GUI) modules strictly implement standardized plugin extension contracts.

```
+----------------------------------------------------------------------------------------------------+
|                                    THIRD-PARTY & COMMUNITY PLUGINS                                 |
|   [Vendor Fieldbus Drivers]    [Custom AI Algorithms]    [OEM P&ID Widgets]    [Robotics Planners] |
+----------------------------------------------------------------------------------------------------+
|                                  FIRST-PARTY BUILT-IN CORE PLUGINS                                 |
|                                                                                                    |
|  +--------------------+  +--------------------+  +--------------------+  +--------------------+    |
|  |   plugin-openplc   |  |   plugin-alarms    |  |   plugin-recipes   |  |  plugin-historian  |    |
|  |  (IEC 61131-3 PLC) |  |  (ANSI/ISA-18.2)   |  |  (ISA-88 Batch)    |  |  (SDC & Parquet)   |    |
|  +--------------------+  +--------------------+  +--------------------+  +--------------------+    |
|  +--------------------+  +--------------------+  +--------------------+                            |
|  |plugin-discovery-dcp|  |   driver-modbus    |  |    driver-mqtt     |                            |
|  | (Layer 2 DCP Scan) |  | (Modbus RTU/TCP)   |  |  (Sparkplug B)     |                            |
|  +--------------------+  +--------------------+  +--------------------+                            |
+----------------------------------------------------------------------------------------------------+
|                                  FRONTEND GUI EXTENSION PLUGINS                                    |
|                                                                                                    |
|  +---------------------------+  +---------------------------+  +--------------------------------+  |
|  | ui-plugin-network-topology|  |   ui-plugin-logic-editor  |  |      ui-plugin-hmi-canvas      |  |
|  |    (xyflow Node Graph)    |  |  (Monaco ST & Ladder)     |  |   (ISA-101 SCADA Viewport)     |  |
|  +---------------------------+  +---------------------------+  +--------------------------------+  |
|  +---------------------------+  +---------------------------+  +--------------------------------+  |
|  |ui-plugin-faceplate-library|  |    ui-plugin-inspector    |  |    ui-plugin-alarm-banner      |  |
|  |    (Faceplate Master)     |  | (384px Modular Drawer)    |  |  (Global ISA-18.2 Banner)      |  |
|  +---------------------------+  +---------------------------+  +--------------------------------+  |
+----------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+----------------------------------------------------------------------------------------------------+
|                               CAPABILITY-BASED SECURITY & ISOLATION                                |
|   - Linear Memory Quota (32 MiB)          - TagBus Namespace Write Fencing                         |
|   - CPU Fuel & Watchdog Metering          - Ed25519 Cryptographic Verification                     |
+----------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+----------------------------------------------------------------------------------------------------+
|                                    MYTHOS PURE MICROKERNEL                                         |
|                                                                                                    |
|   +----------------------------+   +----------------------------+   +--------------------------+   |
|   |    Lock-Free SHM TagBus    |   |   Plugin Host Supervisor   |   |   Unified MCP Server     |   |
|   |  (/dev/shm/mythos_tagbus)  |   | (Dynamic Hot-Reloading)    |   |  (AI Tool Aggregator)    |   |
|   +----------------------------+   +----------------------------+   +--------------------------+   |
+----------------------------------------------------------------------------------------------------+
```

---

## 2. Universal Plugin Taxonomy: The 6 Plugin Categories

Every plugin package advertises one or more of the six universal plugin categories in its manifest (`plugin.yaml`).

```
                              +-----------------------+
                              |   Universal Plugin    |
                              +-----------+-----------+
                                          |
          +-------------------+-----------+-----------+-------------------+
          |                   |                       |                   |
          v                   v                       v                   v
+-------------------+ +-------------------+ +-------------------+ +-------------------+
| 1. ProtocolDriver | |  2. ProcessEngine | |3. NetworkDiscovery| |   4. HmiWidget    |
+-------------------+ +-------------------+ +-------------------+ +-------------------+
          |                                                               |
          +-------------------------------+-------------------------------+
                                          |
                                          v
                              +-----------------------+
                              |   5. LogicExtension   |
                              +-----------------------+
                                          |
                                          v
                              +-----------------------+
                              |   6. McpContributor   |
                              +-----------------------+
```

### 2.1 Category 1: `ProtocolDriver`
- **Role**: Fieldbus and industrial telemetry communications.
- **Contract**: Bidirectional asynchronous I/O with direct TagBus shared memory slot synchronization.
- **Built-in First-Party Plugins**:
  * `driver-modbus`: Modbus RTU (RS-485 serial) and Modbus TCP master/slave.
  * `driver-mqtt`: MQTT v3.1.1/v5.0 edge publisher with Sparkplug B encoding.
- **Community Extensions**: CANopen, EtherCAT Master, Siemens S7comm, BACnet/IP, OPC UA Client.

### 2.2 Category 2: `ProcessEngine`
- **Role**: Continuous cyclic or event-triggered operational algorithms and data processors.
- **Contract**: Evaluates process states during cyclic microkernel clock ticks (10ms - 1000ms) with zero memory allocations in the inner loop.
- **Built-in First-Party Plugins**:
  * `plugin-alarms`: ANSI/ISA-18.2 alarm state machines (Norm, Unack, Ack, RTN), First-Out latching, and dynamic flood suppression.
  * `plugin-recipes`: ANSI/ISA-88 batch recipe coordinator and PackML state machine synchronization.
  * `plugin-historian`: Real-time Boxcar-Swinging Door Compression (SDC) filter writing compressed time-series chunks into columnar Apache Parquet files.
- **Community Extensions**: FFT vibration spectrum condition monitoring, solar irradiance prediction, water chemical dosing PID autotuning.

### 2.3 Category 3: `NetworkDiscovery`
- **Role**: Autonomous physical layer scanning and heuristic fieldbus discovery.
- **Contract**: Non-blocking background worker with cooperative cancellation tokens and progressive event streaming.
- **Built-in First-Party Plugins**:
  * `plugin-discovery-dcp`: PROFINET Layer 2 DCP Ethernet broadcast daemon for identifying unconfigured decentralized I/O racks.
- **Community Extensions**: mDNS / DNS-SD discovery, BACnet Who-Is/I-Am sniffer, Modbus serial automated baud rate sweep.

### 2.4 Category 4: `HmiWidget` & UI Plugins
- **Role**: Graphical User Interface presentations, interactive node canvases, P&ID visual symbols, and diagnostic drawers.
- **Contract**: Web Components / React ES-Module bundles rendered into standardized Dockview shell extension slots.
- **Built-in First-Party Plugins**:
  * `ui-plugin-network-topology`: Interactive xyflow node-graph canvas displaying physical and logical plant networks.
  * `ui-plugin-logic-editor`: Dual-mode Monaco Structured Text editor and visual Ladder Diagram canvas.
  * `ui-plugin-hmi-canvas`: ANSI/ISA-101 high-performance SCADA screen and sub-screen viewport.
  * `ui-plugin-faceplate-library`: Faceplate Master Type visual designer and binding manager.
  * `ui-plugin-inspector`: 384px slide-over inspector drawer hosting modular diagnostic cards.
  * `ui-plugin-alarm-banner`: Persistent top/bottom ANSI/ISA-18.2 alarm banner with visual acknowledgement.
- **Community Extensions**: 3D Digital Twin Three.js viewer, GIS plant map viewer, custom OEM gauge faceplates.

### 2.5 Category 5: `LogicExtension`
- **Role**: Custom IEC 61131-3 Function Blocks and native math accelerators compiled into the OpenPLC STruC++ pipeline.
- **Contract**: Position-independent code (`-fPIC`) C-ABI dynamic library providing `strucpp` symbols.
- **Built-in First-Party Plugins**:
  * `plugin-openplc`: Core OpenPLC Runtime v4 execution harness bridging Structured Text C++ virtual dispatch with the TagBus.
- **Community Extensions**: Neural network inference Function Block (ONNX Runtime), matrix transformation blocks for 6-DOF robotics.

### 2.6 Category 6: `McpContributor`
- **Role**: Autonomous Artificial Intelligence (AI) tool, resource, and prompt provider for Model Context Protocol (MCP) clients.
- **INVARIANT**: **Every MythOS plugin automatically implements `McpContributor`**.
- **Contract**: When a plugin is loaded, it registers domain tools into the native MCP catalog, allowing AI agents to inspect and manipulate plugin capabilities dynamically.

---

## 3. First-Party Built-In Plugins Specification

All core features are developed as independent plugins residing in `plugins/` and `drivers/`. They compile against `crates/mythos-plugin-sdk` and do not possess privileged access to the microkernel.

### 3.1 First-Party Plugin Matrix

| Plugin Identifier | Primary Category | Secondary Categories | Package Path | Functionality |
|---|---|---|---|---|
| `plugin-openplc` | `LogicExtension` | `ProcessEngine`, `McpContributor` | `plugins/plugin-openplc` | IEC 61131-3 soft-logic execution engine, STruC++ compilation, and cyclic execution. |
| `plugin-alarms` | `ProcessEngine` | `HmiWidget`, `McpContributor` | `plugins/plugin-alarms` | ANSI/ISA-18.2 alarm lifecycle, first-out detection, shelving, and flood suppression. |
| `plugin-recipes` | `ProcessEngine` | `HmiWidget`, `McpContributor` | `plugins/plugin-recipes` | ANSI/ISA-88 batch recipe coordinator, parameter validation, and PackML sequencing. |
| `plugin-historian` | `ProcessEngine` | `McpContributor` | `plugins/plugin-historian` | Loss-minimized Boxcar-SDC compression, in-memory circular chunking, and Parquet persistence. |
| `plugin-discovery-dcp` | `NetworkDiscovery` | `McpContributor` | `plugins/plugin-discovery-dcp` | Raw Ethernet Layer 2 DCP broadcast scanner, IP setting, and LED flashing. |
| `driver-modbus` | `ProtocolDriver` | `NetworkDiscovery`, `McpContributor` | `drivers/driver-modbus` | Modbus RTU / TCP polling engine with auto-reconnect and TagBus DMA. |
| `driver-mqtt` | `ProtocolDriver` | `McpContributor` | `drivers/driver-mqtt` | MQTT / Sparkplug B edge publisher and dynamic topic subscriber. |

---

## 4. Frontend GUI Plugin Architecture (VSCode / Eclipse RCP Shell Model)

The MythOS Web Workbench is not a monolithic frontend. It is a **Modular Frontend Plugin Host Shell** built on React 19, TypeScript, and `@hexact/dockview-react`.

```
+----------------------------------------------------------------------------------------------------+
|                                    MYTHOS WORKBENCH PLUGIN HOST                                    |
+----------------------------------------------------------------------------------------------------+
|  Top Header Extension Slot (`slot-top-header`): Project Status, Active Workspace, Sync Status      |
+----------------------------------------------------------------------------------------------------+
|  Alarm Banner Extension Slot (`slot-alarm-banner`): Global ISA-18.2 Alarm Bar (`ui-plugin-alarm`)   |
+---------------------+--------------------------------------------------------+---------------------+
| Left Navigation     | Main Viewport Tab Extension Slot                       | Right Inspector     |
| Extension Slot      | (`slot-main-viewport`):                                | Drawer Slot         |
| (`slot-nav-tree`)   | - `ui-plugin-network-topology` (xyflow node graph)     | (`slot-inspector`)  |
|                     | - `ui-plugin-logic-editor` (Monaco ST & Ladder)        |                     |
| - Plant Tree View   | - `ui-plugin-hmi-canvas` (ISA-101 SCADA viewport)     | - 384px Fixed Width |
| - Device Catalog    | - `ui-plugin-faceplate-library` (Faceplate Editor)     | - Dynamic Widget    |
| - Plugin Store      |                                                        |   Hosting Card Deck |
+---------------------+--------------------------------------------------------+---------------------+
| Bottom Panel Extension Slot (`slot-bottom-panel`): Compile Diagnostics, MCP Agent Chat, Log Stream |
+----------------------------------------------------------------------------------------------------+
```

### 4.1 Frontend Extension Slots
The Workbench Shell exposes fixed layout extension slots where UI plugins mount dynamically:
1. `slot-top-header`: Application menu, project status indicator, hot-reload triggers.
2. `slot-alarm-banner`: Persistent ANSI/ISA-18.2 banner with unacknowledged flash latching.
3. `slot-nav-tree`: Hierarchical plant tree, hardware catalog tree, screen tree.
4. `slot-main-viewport`: Primary tabbed workspace for canvases, editors, and mimics.
5. `slot-inspector`: Right-hand 384px drawer rendering context-sensitive faceplates and widgets.
6. `slot-bottom-panel`: Tabbed console for compiler output, terminal, and MCP trace logs.

### 4.2 First-Party Frontend UI Plugins
Every workspace view is an isolated plugin bundle exposing standard lifecycle manifests:

```typescript
// Standard MythOS Frontend Plugin Interface
export interface MythosUiPlugin {
  readonly id: string;
  readonly name: string;
  readonly version: string;
  readonly slots: Array<'slot-main-viewport' | 'slot-inspector' | 'slot-alarm-banner' | 'slot-nav-tree'>;
  
  initialize(context: UiPluginContext): Promise<void>;
  renderComponent(slot: string, props: Record<string, unknown>): React.ReactNode;
  dispose(): void;
}
```

#### 4.2.1 `ui-plugin-network-topology`
- Mounts in: `slot-main-viewport`.
- Engine: `@xyflow/react`.
- Features: Visual layout of fieldbus nodes (Modbus, PROFINET, MQTT), cable link statuses, heuristic discovery node dropping.

#### 4.2.2 `ui-plugin-logic-editor`
- Mounts in: `slot-main-viewport`.
- Engine: `@monaco-editor/react` (Structured Text) + Canvas (Ladder Diagram).
- Features: In-canvas syntax highlighting, IEC 61131-3 live variable inspection tooltips, hot-reload compile triggering (`POST /api/v1/control/deploy`).

#### 4.2.3 `ui-plugin-hmi-canvas`
- Mounts in: `slot-main-viewport`.
- Features: ISA-101 high-performance grayscale mimic display, sub-canvas navigation, live tag value data-binding.

#### 4.2.4 `ui-plugin-faceplate-library`
- Mounts in: `slot-main-viewport` and `slot-inspector`.
- Features: Master faceplate designer, dynamic parameter binding, global propagation engine.

#### 4.2.5 `ui-plugin-inspector`
- Mounts in: `slot-inspector` (384px fixed width).
- Features: Hosts modular iOS-style widgets: live tag diagnostics, manual override toggles, calibration sliders, PID tuning curves.

#### 4.2.6 `ui-plugin-alarm-banner`
- Mounts in: `slot-alarm-banner`.
- Features: Real-time SSE listener rendering highest-priority unacknowledged alarm, silence buzzer toggle, bulk acknowledge button.

---

## 5. Plugin Manifest Specification (`plugin.yaml`)

Every plugin package contains a canonical manifest (`plugin.yaml`) adhering to JSON Schema Draft 2020-12.

### 5.1 Complete Manifest Example (Combined Driver, UI & MCP)

```yaml
schema_version: "1.0.0"
id: "org.mythos.driver.modbus"
name: "Industrial Modbus RTU/TCP Driver & Workbench Suite"
version: "2.1.0"
description: "Comprehensive Modbus communication suite with interactive diagnostic faceplates"
author: "MythOS Core Engineering <team@firmansyah.co>"
license: "Apache-2.0"
homepage: "https://github.com/firmansyahco/driver-modbus"

runtime:
  type: "native"                  # "native" (shared object actor) | "wasi" (sandboxed Wasm)
  entrypoint: "bin/libdriver_modbus.so"
  target_abi: "mythos-abi-v1"
  min_kernel_version: "0.1.0"

categories:
  - "ProtocolDriver"
  - "NetworkDiscovery"
  - "HmiWidget"
  - "McpContributor"

capabilities:
  - "net:listen"                 # Inbound Modbus TCP port 502
  - "net:connect"                # Outbound Modbus TCP connection
  - "serial:rw"                  # RS-485 serial ports (/dev/ttyUSB*, /dev/ttyS*)
  - "tagbus:read"                # Read from assigned TagBus namespace
  - "tagbus:write"               # Write to assigned TagBus namespace
  - "mcp:register"               # Dynamic AI tool registration

resources:
  max_memory_bytes: 33554432     # 32 MiB memory limit
  max_cpu_percent: 15            # 15% single-core CPU quota
  execution_timeout_ms: 1000     # 1000ms watchdog bound

configuration:
  schema: "schemas/config.schema.json"
  defaults:
    poll_interval_ms: 100
    timeout_ms: 500
    retry_count: 3

ui:
  entrypoint: "ui/bundle.js"
  style: "ui/bundle.css"
  slots:
    - name: "slot-inspector"
      component: "ModbusInspectorWidget"
    - name: "slot-main-viewport"
      component: "ModbusTopologyViewer"

mcp:
  tools:
    - name: "modbus_read_raw_registers"
      description: "Read raw 16-bit registers directly from remote slave"
      input_schema:
        type: "object"
        required: ["slave_id", "function_code", "address", "count"]
        properties:
          slave_id: { type: "integer", minimum: 1, maximum: 247 }
          function_code: { type: "integer", enum: [1, 2, 3, 4] }
          address: { type: "integer", minimum: 0, maximum: 65535 }
          count: { type: "integer", minimum: 1, maximum: 125 }
```

---

## 6. Rust Trait Contracts (`crates/mythos-plugin-sdk`)

The `mythos-plugin-sdk` crate exposes the standardized Rust trait contracts. Both first-party engines and third-party plugins compile against this crate.

### 6.1 The Master Trait: `MythosPlugin`

```rust
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use crate::context::PluginContext;
use crate::error::PluginError;
use crate::health::PluginHealthReport;

/// The primary lifecycle trait implemented by all MythOS plugins.
#[async_trait]
pub trait MythosPlugin: Send + Sync + 'static {
    /// Return the canonical reverse-DNS identifier (e.g., "org.mythos.driver.modbus").
    fn plugin_id(&self) -> &'static str;

    /// Return the semantic version of the plugin.
    fn version(&self) -> &'static str;

    /// Return the list of categories implemented by this plugin.
    fn categories(&self) -> &'static [&'static str];

    /// Initialize internal state and validate capability requirements.
    /// Executed before network ports or files are opened.
    async fn init(&mut self, ctx: &PluginContext) -> Result<(), PluginError>;

    /// Apply configuration and start execution tasks.
    async fn start(&mut self, ctx: &mut PluginContext, config: JsonValue) -> Result<(), PluginError>;

    /// Cyclic execution hook invoked by the Microkernel scheduler (e.g., 10ms - 1000ms).
    async fn tick(&mut self, ctx: &mut PluginContext, delta_ms: u32) -> Result<(), PluginError> {
        let _ = (ctx, delta_ms);
        Ok(())
    }

    /// Terminate background tasks, flush write buffers, and release OS resources.
    async fn stop(&mut self, ctx: &mut PluginContext) -> Result<(), PluginError>;

    /// Periodically queried by the Microkernel Supervisor to verify operational health.
    async fn health_check(&self) -> PluginHealthReport;
}
```

### 6.2 Category Extension Sub-Traits

```rust
/// Category 1: Protocol Driver Extension Contract
#[async_trait]
pub trait ProtocolDriverExtension: MythosPlugin {
    async fn poll_cycle(&mut self, ctx: &mut PluginContext) -> Result<u32, PluginError>;
    async fn handle_write_tag(
        &mut self,
        ctx: &mut PluginContext,
        tag_id: u32,
        value: crate::TagValueRaw,
    ) -> Result<(), PluginError>;
}

/// Category 2: Process Engine Extension Contract
#[async_trait]
pub trait ProcessEngineExtension: MythosPlugin {
    async fn evaluate_tags(
        &mut self,
        ctx: &mut PluginContext,
        changed_tags: &[crate::TagChangeEvent],
    ) -> Result<(), PluginError>;
    async fn flush_storage(&mut self, ctx: &mut PluginContext) -> Result<(), PluginError>;
}

/// Category 3: Network Discovery Extension Contract
#[async_trait]
pub trait NetworkDiscoveryExtension: MythosPlugin {
    async fn start_scan(
        &mut self,
        ctx: &mut PluginContext,
        scan_params: JsonValue,
        progress_tx: flume::Sender<crate::DiscoveryProgressEvent>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<crate::DiscoveredDevice>, PluginError>;
}

/// Category 5: Logic Extension Engine Contract
pub trait LogicExtensionEngine: MythosPlugin {
    fn export_function_blocks(&self) -> Vec<crate::FunctionBlockDescriptor>;
    fn resolve_symbol(&self, symbol_name: &str) -> Option<*const ()>;
}

/// Category 6: Model Context Protocol (MCP) Contributor Contract
#[async_trait]
pub trait McpContributorExtension: MythosPlugin {
    fn export_mcp_tools(&self) -> Vec<crate::McpToolDescriptor>;
    async fn execute_mcp_tool(
        &mut self,
        ctx: &PluginContext,
        tool_name: &str,
        params: JsonValue,
    ) -> Result<JsonValue, PluginError>;
}
```

---

## 7. Capability-Based Sandboxing & Safety Invariants

To guarantee that third-party plugins cannot compromise the industrial plant or crash the real-time PLC:

```
+----------------------------------------------------------------------------------------------------+
|                                    SANDBOXING GUARANTEE MATRIX                                     |
+-------------------------------------+--------------------------------------------------------------+
| Security Dimension                  | Enforcement Mechanism                                        |
+-------------------------------------+--------------------------------------------------------------+
| 1. Memory Safety & Quotas           | - WASI: Wasmtime 32 MiB linear memory virtual ceiling.       |
|                                     | - Native: OS cgroup / setrlimit RSS memory bounds.           |
+-------------------------------------+--------------------------------------------------------------+
| 2. TagBus Namespace Write Fencing   | Plugins may ONLY write to tags within their assigned         |
|                                     | namespace (e.g., `external.modbus.*`). Writing to internal   |
|                                     | OpenPLC tags (`internal.*`) results in instant SIGSEGV/Fault.|
+-------------------------------------+--------------------------------------------------------------+
| 3. CPU Time Quota & Fuel Metering   | - WASI: Deterministic instruction fuel consumption.          |
|                                     | - Native: Tokio 100ms watchdog timer aborts runaway loops.  |
+-------------------------------------+--------------------------------------------------------------+
| 4. Panic Isolation                  | Host wraps all plugin invocations in `catch_unwind`. A panic |
|                                     | in a plugin never crashes the microkernel or stops OpenPLC.  |
+-------------------------------------+--------------------------------------------------------------+
| 5. Cryptographic Packaging          | Ed25519 digital signatures verified against trusted roots.   |
+-------------------------------------+--------------------------------------------------------------+
```

---

## 8. Hot-Reloading & Lifecycle State Machine

Plugins support dynamic hot-reloading at runtime without restarting the MythOS daemon or dropping the cyclic control loop:

```
              +-------------------+
              |    Uninstalled    |
              +---------+---------+
                        | mythos plugin install <pkg>
                        v
              +-------------------+
              |     Installed     |
              +---------+---------+
                        | init() & verify capabilities
                        v
              +-------------------+
              |      Loaded       |
              +----+---------+----+
                   |         ^
      start(config)|         | stop()
                   v         |
              +-------------------+
              |      Active       | <---+
              +----+---------+----+     |
                   |         |          |
       panic caught|         | tick()   | hot_reload()
                   v         +----------+
              +-------------------+
              |  Fault Isolated   |
              +-------------------+
```

### 8.1 Bumpless Hot-Update Flow ($< 15\,\text{ms}$)
1. The new plugin binary and assets are staged into memory.
2. The new instance initializes via `init(&ctx)`.
3. During the dead-time between cyclic ticks:
   - The old plugin instance is paused and drained via `stop(&mut ctx)`.
   - The TagBus pointers are atomically remapped to the new instance.
   - The new instance starts via `start(&mut ctx, config)`.
4. Total switchover latency is $< 15\,\text{ms}$, ensuring uninterrupted real-time field control.

---

## 9. Dynamic AI Model Context Protocol (MCP) Aggregator

When any plugin (first-party or third-party) is loaded:
1. The microkernel inspects its declared MCP tools (`export_mcp_tools()`).
2. Tools are dynamically registered into the native `mythos-mcp` server.
3. Connected AI agents receive an automated `notifications/tools/list_changed` JSON-RPC broadcast.
4. AI agents can immediately execute the new tool via `execute_mcp_tool()`.
5. When a plugin is disabled or uninstalled, its tools are instantly revoked, preventing phantom tool executions.

---

## 10. Packaging & CLI Management (`.mythos-plugin`)

Plugins are distributed as compressed `.mythos-plugin` archives:

```bash
# Install a plugin
mythos plugin install org.mythos.driver.modbus-2.1.0.mythos-plugin

# List all loaded plugins (First-Party, Third-Party, and UI modules)
mythos plugin list

# Enable or disable a plugin dynamically
mythos plugin enable org.mythos.driver.modbus
mythos plugin disable org.mythos.driver.modbus

# Upgrade a plugin without restarting MythOS
mythos plugin update org.mythos.driver.modbus --from-package modbus-2.2.0.mythos-plugin

# Inspect capabilities, memory usage, and assigned TagBus slots
mythos plugin inspect org.mythos.driver.modbus
```

This completes the transformation of MythOS into an open, community-driven, ultra-hardened microkernel platform where **everything is a plugin**.


---

## 11. Decoupled Repositories, Central Index Registry & GitHub Releases CDN

To eliminate dependency bottlenecks and enable distributed third-party development without centralized binary hosting costs, MythOS implements the **Decoupled Git Repository & Central Index Architecture** (modeled after Rust's `crates.io` index and Homebrew taps).

```
+----------------------------------------------------------------------------------------------------+
|                      DECOUPLED REPOSITORIES & GITHUB RELEASES DISTRIBUTION PIPELINE                 |
|                                                                                                    |
|  1. INDEPENDENT GIT REPOS (OEMs/Community)     2. CENTRAL INDEX REGISTRY    3. TARGET CONTROLLER   |
|  +---------------------------------------+     +------------------------+   +--------------------+ |
|  | github.com/autonomy/compiler-strucpp  |     | github.com/firmansyahco|   | $ mythos plugin    | |
|  | - tag: v1.4.0                         |     |   /plugin-registry     |   |   update --all     | |
|  | - release artifact:                   | ==> |                        |==>|                    | |
|  |   strucpp-v1.4.0-arm64.mythos-plugin  |     | Metadata Index (JSON): |   | 1. Query index     | |
|  +---------------------------------------+     | /plugins/compiler/     |   | 2. Compare semver  | |
|                                                |   strucpp.json         |   | 3. Download release| |
|  +---------------------------------------+     | - download_url         |   |    binary via CDN  | |
|  | github.com/supmea/driver-supmea       |     | - sha256 checksum      |   | 4. Verify hash     | |
|  | - release: supmea-v2.0.mythos-plugin  |     | - supported_targets    |   | 5. Hot-swap live   | |
|  +---------------------------------------+     +------------------------+   +--------------------+ |
+----------------------------------------------------------------------------------------------------+
```

### 11.1 Independent Git Repositories
- **Decoupled Development**: Every hardware vendor (Supmea, Haiwell, SenTec), compiler developer (Autonomy Logic), or community team maintains independent Git repositories under their own GitHub/GitLab organizations.
- **CI/CD Automated Packaging**: Upon creating a Git release tag (e.g. `v1.2.0`), automated GitHub Actions build and sign `.mythos-plugin` archives for target architectures (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`) and upload them as GitHub Release Assets.

### 11.2 Central Index Registry (`github.com/firmansyahco/plugin-registry`)
- **Git-Backed Lightweight Metadata**: The central registry contains only tiny JSON metadata files organized by namespace:
  ```json
  {
    "id": "org.autonomy.compiler.strucpp",
    "name": "STruC++ IEC 61131-3 Compiler Plugin",
    "category": "SoftLogicCompiler",
    "versions": {
      "1.4.0": {
        "released_at": "2026-10-14T08:00:00Z",
        "download_url": "https://github.com/autonomy-logic/strucpp/releases/download/v1.4.0/strucpp-v1.4.0-aarch64.mythos-plugin",
        "sha256": "8f4e2b10a9c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0",
        "min_mythos_version": "1.0.0",
        "supported_languages": ["ST", "LD", "FBD", "SFC", "IL"]
      }
    }
  }
  ```

### 11.3 Automated Update Checking & In-IDE Package Manager
- **Non-Intrusive Background Checks**: When MythOS starts or every 24 hours, the Plugin Host queries the index via HTTPS.
- **In-IDE Storefront**: The Web Canvas features a dedicated **`[ 🧩 Extensions & Plugins ]`** tab:
  * Filter by category: `Compilers`, `Protocols`, `Visual Widgets`, `Algorithms`.
  * One-Click Install & Update: Clicking `[ Update ]` downloads the signed binary from GitHub Releases CDN, validates the SHA-256 digest, and executes a $< 15\,	ext{ms}$ hot-swap without stopping the soft-logic scan loop.

---

## 12. Soft-Logic Compiler Plugins & Full 5-Language IEC 61131-3 Support

MythOS supports all **5 standard IEC 61131-3 languages** through the pluggable `SoftLogicCompilerPlugin` SPI:
1. **ST (Structured Text)**: High-level textual language (Pascal-like algorithmic logic).
2. **LD (Ladder Diagram)**: Graphical relay ladder contact and coil networks.
3. **FBD (Function Block Diagram)**: Graphical interconnect networks of functional blocks.
4. **SFC (Sequential Function Chart)**: State chart phase transitions and step actions.
5. **IL (Instruction List)**: Low-level textual accumulator language (legacy support).

### 12.1 Compiler Plugin Contract (`trait SoftLogicCompilerPlugin`)
Compilers (including the default `compiler-strucpp`) implement the dedicated compiler SPI:

```rust
#[async_trait]
pub trait SoftLogicCompilerPlugin: MythosPlugin {
    /// Supported IEC 61131-3 language variants
    fn supported_languages(&self) -> Vec<IecLanguage>;

    /// Compile IEC source tree into a native shared object (.so)
    async fn compile(
        &self, 
        source: &IecSourceTree, 
        target: &CompilationTarget
    ) -> Result<CompiledArtifact, CompilerError>;
}
```

This guarantees complete technological independence: if future compiler toolchains emerge (such as LLVM-based or pure Rust IEC compilers), they can be installed as drop-in compiler plugins without altering the MythOS microkernel.
