# ADR-008: Universal Plugin Harness Architecture ("Everything is a Plugin")

- **Status**: ACCEPTED
- **Date**: 2026-10-14
- **Author**: Neo (VP Software & Infrastructure)
- **Deciders**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Vulcan (VP Hardware), Iris (VP Design), Sage (VP R&D), Scribe (VP Publication)

---

## 1. Context and Problem Statement

Industrial SCADA and soft-logic runtime platforms historically suffer from severe architectural calcification. As new fieldbus communication standards emerge, new analytical routines are formulated, or specialized user interface components are created, monolithic architectures require direct modifications to the core codebase. This creates multiple acute engineering risks:
1. **Regression Risks**: Modifying core binaries to support a niche protocol can introduce timing jitter, memory leaks, or segmentation faults into real-time cyclic control loops.
2. **Vendor Lock-In and Barrier to Entry**: Third-party OEMs, hardware vendors, and system integrators cannot independently package, distribute, and monetize proprietary algorithms or drivers without fork-merging the entire platform.
3. **Monolithic Bloat**: Hardcoding all alarms, recipes, historians, discovery routines, and GUI screens into the primary application binary balloons the footprint, slows compilation, and compromises safety audits.
4. **AI Agent Tool Fragility**: When AI tools are statically compiled into the backend, newly attached hardware or algorithms cannot expose their unique diagnostic capabilities to autonomous AI agents without recompiling and redeploying the daemon.

Adrian (Board of Commissioners) mandated a foundational architectural pivot:
> *"pastikan everything is a plugin, seperti deepseek harness, aku mau mythos ini bisa dikembangkan oleh banyak orang kedepannya. bahkan GUI dan semua fitur yang kita buat pun berupa plugin kan."*

We must decide the architectural framework to transform MythOS into a pure zero-domain microkernel where **literally every feature—both first-party backend engines and frontend graphical modules—is an extensible plugin**.

---

## 2. Decision Drivers

- **Extensibility & Community Ecosystem**: Allow external developers, OEMs, and community contributors to publish and install plugins without recompiling or altering the core microkernel.
- **Zero-Domain Hardened Microkernel**: The core kernel must only host the lock-free TagBus (`/dev/shm/mythos_tagbus`), dynamic plugin lifecycle supervisor, capability-based security barrier, and unified MCP aggregator.
- **Universal Application**: The plugin contract must uniformly govern first-party built-in features (OpenPLC, Alarms, Recipes, Historian, DCP Discovery, Modbus, MQTT) and third-party extensions.
- **Modular Frontend GUI Architecture**: The frontend must function as a modular plugin host shell (similar to VSCode Extension Host or Eclipse RCP) where canvases, editors, faceplates, inspectors, and alarm bars mount as independent UI plugins.
- **Hard Real-Time Fault Isolation**: A buggy, panicking, or malicious plugin must never corrupt the shared memory bus or crash the cyclic IEC 61131-3 logic execution.
- **Autonomous AI Self-Registration**: Every plugin must automatically contribute its domain-specific tools into the native Model Context Protocol (MCP) server.

---

## 3. Considered Options

### Option 1: Monolithic Modular Crates with Compile-Time Cargo Feature Flags
- *Mechanism*: Features and drivers are separate Cargo crates statically linked into `mythos-core` using Cargo feature flags (`--features "modbus,openplc,alarms"`).
- *Pros*: Maximum compile-time optimization; zero dynamic dispatch overhead.
- *Cons*: Violates the core mandate. Third parties cannot distribute pre-compiled binary plugins. Adding a driver requires full codebase recompilation. Zero runtime hot-reloading.

### Option 2: Dynamic Shared Objects (`.so` / `dlopen`) with Raw C-ABI
- *Mechanism*: Plugins compile to shared libraries and are dynamically loaded at runtime using `dlopen` / `libloading`.
- *Pros*: Native performance; supports multiple programming languages (C, C++, Rust).
- *Cons*: Zero memory sandboxing. A segfault or memory corruption inside a third-party `.so` immediately terminates the entire host process. ABI instability across compiler versions.

### Option 3: Universal Dual-Runtime Plugin Harness (WASI Sandbox + Supervised Native Actor) [SELECTED]
- *Mechanism*: Adopt the DeepSeek Harness / VSCode Extension model:
  1. The Microkernel exposes a unified Service Provider Interface (`crates/mythos-plugin-sdk`).
  2. Untrusted community plugins run inside a hardware-isolated WebAssembly sandbox (Wasmtime 25.0) with strict 32 MiB memory quotas and CPU fuel metering.
  3. Certified high-throughput fieldbus drivers run as supervised native Tokio actors wrapped in `std::panic::catch_unwind` and monitored by the Microkernel Supervisor.
  4. The frontend runs a modular Dockview plugin shell where all GUI views are dynamically loaded UI plugin bundles.
  5. Every plugin implements `McpContributor`, dynamically exposing its capabilities to AI agents.
- *Pros*: Perfect fault isolation; zero-recompile extensibility; universal support for backend engines and frontend UI; native AI agent tool discovery; sub-15ms bumpless hot-reloading.

---

## 4. Decision Outcome

**Chosen Option**: Option 3 (Universal Dual-Runtime Plugin Harness).

### 4.1 Architectural Breakdown

#### 4.1.1 The Pure Microkernel (`crates/mythos-core`)
The kernel is stripped of all domain-specific logic. It contains solely:
- The lock-free POSIX Shared Memory TagBus (`/dev/shm/mythos_tagbus`).
- The `PluginManagerActor` handling lifecycle states (`init`, `start`, `tick`, `stop`, `health_check`).
- The capability-based security barrier (`tagbus:read`, `tagbus:write`, `net:listen`, `serial:rw`).
- The dynamic MCP tool aggregator.

#### 4.1.2 First-Party Built-In Features as Standard Plugins
All core features are converted to standalone first-party plugins that strictly adhere to the universal plugin contract:
- `plugin-openplc`: OpenPLC Runtime v4 soft-logic execution engine and STruC++ compiler bridge.
- `plugin-alarms`: ANSI/ISA-18.2 alarm lifecycle state machine and flood suppression.
- `plugin-recipes`: ANSI/ISA-88 batch recipe coordinator and PackML sequencer.
- `plugin-historian`: Boxcar-SDC compression engine and Apache Parquet columnar archiver.
- `plugin-discovery-dcp`: PROFINET Layer 2 DCP Ethernet broadcast daemon.
- `driver-modbus`: Modbus RTU / TCP communication driver.
- `driver-mqtt`: MQTT / Sparkplug B edge synchronizer.

#### 4.1.3 Modular Frontend GUI Plugin Shell
The web frontend application (`dockview-react` + Vite + React 19) becomes a modular plugin host shell:
- `slot-main-viewport`: Hosts `ui-plugin-network-topology` (xyflow), `ui-plugin-logic-editor` (Monaco ST & Ladder), `ui-plugin-hmi-canvas` (ISA-101 SCADA viewport), and `ui-plugin-faceplate-library`.
- `slot-inspector`: Hosts `ui-plugin-inspector` (384px drawer) rendering dynamic iOS-style modular widget cards.
- `slot-alarm-banner`: Hosts `ui-plugin-alarm-banner` rendering global ISA-18.2 alarm states.
- Third-party developers can mount custom React/Wasm UI widgets directly into these extension slots.

#### 4.1.4 Unified Packaging & Security Standard
- Plugins are distributed as `.mythos-plugin` ZIP containers containing manifest (`plugin.yaml`), WASI/native binaries, UI bundles, and JSON schemas.
- Packages are signed with Ed25519 cryptographic signatures and verified against root authorities before execution.

---

## 5. Consequences

### 5.1 Positive Consequences
- **Unlimited Extensibility**: Anyone can develop drivers, process algorithms, visual widgets, and AI tools without touching the core repository.
- **Ultra-Hardened Core**: The microkernel code surface is reduced, making it trivial to audit and verify for mission-critical reliability.
- **Fault Containment**: A crash in a third-party plugin is isolated by the supervisor; the real-time PLC scan loop continues running without interruption.
- **Instant AI Integration**: AI agents dynamically acquire new capabilities as soon as a plugin is installed.
- **Dynamic Hot-Swapping**: Bumpless live updates (< 15ms) allow updating drivers or algorithms without taking the industrial plant offline.

### 5.2 Negative Consequences and Mitigations
- **Dynamic Dispatch Overhead**: Trait objects and async channel dispatch introduce minor overhead compared to monomorphic static inlining. *Mitigation*: TagBus shared memory operations bypass dynamic dispatch using direct atomic pointer access.
- **Wasm/WASI Boundary Serialization**: Cross-boundary calls between WASI plugins and host memory require serialization. *Mitigation*: High-frequency telemetry uses shared memory ring buffers; WASI is reserved for algorithms, discovery, and UI.
