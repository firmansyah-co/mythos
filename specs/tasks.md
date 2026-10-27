# MythOS Work Breakdown Structure & Sprint Roadmap

- **Document Identifier**: MYTHOS-PLAN-001
- **Controlled Language**: ASD-STE100 Simplified Technical English
- **Standard Reference**: ISO/IEC/IEEE 29148 & Agile Scrum Enterprise Delivery
- **Version**: 1.4.0
- **Status**: APPROVED / MASTER ROADMAP
- **Author**: Scribe (VP of Publication & Technical Documentation)
- **Approved by**: Adrian (Board of Commissioners), Aria (Chief Executive Officer)

---

## 1. Executive Summary & Delivery Strategy

This document establishes the official engineering roadmap and Work Breakdown Structure (WBS) for MythOS. 
The development lifecycle is divided into six sequential sprints (Sprint 0 through Sprint 5). 
Each sprint lasts two weeks and delivers fully verified, tested, and documented capability increments.

```
+-----------------------------------------------------------------------------------------------+
|                                    MYTHOS SPRINT ROADMAP                                      |
+-----------------------------------------------------------------------------------------------+
|                                                                                               |
|  [ SPRINT 0 ]  Core Foundations, SHM TagBus, Driver SPI & Universal Plugin SDK Harness      |
|         |                                                                                     |
|         v                                                                                     |
|  [ SPRINT 1 ]  Native STruC++ Runtime Engine & In-Canvas Logic Compilation                    |
|         |                                                                                     |
|         v                                                                                     |
|  [ SPRINT 2 ]  Layer 2 PROFINET DCP Discovery, Hardware Catalog & Dynamic Plugin Packaging    |
|         |                                                                                     |
|         v                                                                                     |
|  [ SPRINT 3 ]  High-Performance ISA-101 HMI, SDUI, xyflow Network Canvas & Faceplates        |
|         |                                                                                     |
|         v                                                                                     |
|  [ SPRINT 4 ]  ISA-18.2 Alarm Engine, ISA-88 Recipe Synchronization & Parquet Historian      |
|         |                                                                                     |
|         v                                                                                     |
|  [ SPRINT 5 ]  Native MCP AI Agent Subsystem, Disaster Recovery (.mythos-pkg) & Acceptance   |
|                                                                                               |
+-----------------------------------------------------------------------------------------------+
```

---

## 2. Sprint Breakdown

### Sprint 0: Core Foundations, Lock-Free SHM TagBus, Driver SPI & Universal Plugin SDK
- **Sprint Goal**: Deliver the pure Rust microkernel workspace foundation, triple-buffered lock-free shared memory tag bus, target controller storage layout, isolated protocol driver SPI, and the universal plugin harness trait contract.
- **Duration**: Weeks 1 - 2
- **Lead Division**: Software (`head-software`), Hardware (`head-hardware`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-001** | Initialize Cargo Workspace and Crate Structure | Software (`head-software`) | `Cargo.toml`, `crates/mythos-pal/`, `crates/mythos-core/`, `crates/mythos-shm/`, `crates/mythos-plugin-sdk/`, `crates/mythos-cli/` | Zero compilation errors. Cargo tree resolves cleanly with strict compiler flags (`-D warnings`). |
| **TSK-002** | Implement Triple-Buffered Lock-Free SHM Engine | Software (`head-software`) | `crates/mythos-shm/src/lib.rs`, `buffer.rs`, `slot.rs` | 32,768 tag slots allocated in `/dev/shm/mythos_tagbus`. 64-byte alignment verified. Lock-free atomic swap latency $< 50\,\text{ns}$. |
| **TSK-003** | Define Canonical Plant Hierarchy Tag Engine | Software (`head-software`) | `crates/mythos-core/src/tag.rs`, `namespace.rs` | Validates `external.<namespace>.<device>.<variable>` hierarchy. Scopes namespace to plant locations (e.g. `BoilerRoom`). Omits `external` for internal PLC tags. |
| **TSK-004** | Develop Protocol Driver Service Provider Interface | Software (`head-software`) | `crates/mythos-plugin-sdk/src/lib.rs` | Asynchronous `ProtocolDriver` trait implemented with isolated JSON schema parsing. |
| **TSK-005** | Implement Reference Modbus RTU/TCP Driver Crate | Software (`head-software`), Hardware (`head-hardware`) | `plugins/drivers/driver-modbus/` | Successful register polling with simulated slave device. Zero thread blocking on serial timeouts. |
| **TSK-006** | Develop NVRAM WAL Persistence Engine | Software (`head-software`) | `crates/mythos-core/src/nvram.rs` | RETAIN and PERSISTENT memory tiers backed by `/var/lib/mythos/nvram.dat` with CRC32 integrity checks. |
| **TSK-007** | Implement Target Controller Storage & WAL Engine | Software (`head-software`) | `crates/mythos-core/src/storage/`, `project.rs` | Primary target controller storage layout under `/var/lib/mythos/projects/`. Multi-project repository, atomic `active` symlink, debounced autosave (1,000ms), and crash-proof append-only `project.wal`. |
| **TSK-008** | Implement Axum API Gateway & 100ms SSE Stream | Software (`head-software`) | `crates/mythos-core/src/api/`, `telemetry.rs` | Axum 0.7 REST endpoints and `GET /api/v1/telemetry/stream` SSE stream with locked 100ms (10 Hz) batch throttling. |
| **TSK-009** | Universal Plugin SDK & Crate Architecture | Software (`head-software`) | `crates/mythos-plugin-api/`, `crates/mythos-plugin-host/` | Establishes `MythosPlugin` trait and microkernel plugin host. Refactors engines and drivers into isolated first-party plugins. Zero domain logic in microkernel. |
| **TSK-010** | Author Sprint 0 Unit & Micro-Benchmark Suite | Publication / QA | `crates/mythos-shm/benches/shm_bench.rs` | The original $< 100\,\text{ns}$ p99-under-16-threads criterion is **UNDER REVISION**; the measured result is recorded in the TSK-010 STATUS note under Sprint 0. It is not a merge gate. A replacement bound is a board decision; this document does not select one. **CI state**: PR #25 (merged at `333d199`) reports the 16-thread p99 via `Report SHM p99 under 16 threads (TSK-010, non-gating)`, a report-only step, and runs the criterion benchmark through `Run SHM Micro-Benchmark (criterion, executed)`. The p99 is measured and reported on every run; it does not gate. |
| **TSK-011** | Implement Platform Abstraction Layer (PAL) & OS Agnosticism | Software (`head-software`) | `crates/mythos-pal/`, `crates/mythos-shm/src/pal.rs`, `crates/mythos-core/src/pal.rs` | Encapsulates POSIX shared memory, Windows Named SHM, macOS mmap, real-time thread priority, and dynamic library loading behind unified traits. Builds cleanly across Linux, Windows 11, and macOS. |
| **TSK-012** | Engineer Multi-Architecture Container & Compose Suite | Software (`head-software`) | `Dockerfile`, `compose.yaml`, `.dockerignore` | Builds official multi-stage Docker images for `linux/amd64` and `linux/arm64` with `--ipc=host` and `--net=host` optimization. Boots daemon and web UI within container in $< 3\,\text{s}$. |
> **TSK-010 STATUS (1.4.0): the p99 criterion is UNDER REVISION. It is not met, and the measurement is recorded here.**
>
> The Sprint 0 acceptance criterion "$< 100\,\text{ns}$ p99 read/write under 16 concurrent threads" is withdrawn as a merge gate. It is not recorded as met, and it is not recorded as a simple failure. The criterion itself does not survive contact with the measured regime.
>
> **Measurement 1.** PR #25, CI run 40089446143, 1-minute load average 63.85. Latency in nanoseconds:
>
> | Case | n | p50 | p90 | p99 |
> |---|---|---|---|---|
> | READ, busy-poll | 300,000 | 36 | 247 | 909 |
> | READ, swap-acquire | 10,311 | 178 | 373 | 1,209 |
> | WRITE | 20,000 | 145 | 437 | 1,297 |
>
> Against the 100 ns p99 target: read NOT MET, write NOT MET.
>
> **Measurement 2.** An independent run at local load average 86.21 agreed in direction: read swap-acquire p99 4,145 ns, write p99 1,614 ns.
>
> **Why no p99 number replaces it.** The 16-thread case CAS-es one 64-bit state word. The measured tail is contention on that word, not the cost of a tag access. A p99 under N-thread contention measures the scheduler and the co-tenant load as much as the architecture: read swap-acquire p99 moved from 1,209 ns to 4,145 ns (3.4x) between the two runs above, at different co-tenant load. No fixed p99 bound is defensible as a merge gate, and this amendment does not invent one.
>
> **Recommended replacement, FOR BOARD SIGN-OFF, not adopted here.** Gate on a best-batch bound in the regime that TSK-002 already uses, which measures uncontended atomic swap cost ($< 50\,\text{ns}$). Retain the 16-thread p99 as a report-only CI measurement so that a real regression stays visible without a gate that reddens on unrelated changes. Selecting the exact bound is a board decision.
>
> The measurement harness introduced by PR #25 is retained. Only its gate status changes.
>
> **CI wiring, shipped as of `333d199`.** PR #25 merged the report-only step (`Report SHM p99 under 16 threads (TSK-010, non-gating)`) and replaced the former compile-only step with an executed criterion benchmark (`Run SHM Micro-Benchmark (criterion, executed)`). Both run on every CI build; the p99 number is reported and logged, never gated.

---

### Sprint 1: Native STruC++ Runtime Engine & In-Canvas Logic Compilation
- **Sprint Goal**: Embed the Native STruC++ Runtime Engine via an `extern "C"` Foreign Function Interface, establish the STruC++ compiler pipeline with in-canvas deploy, and verify cyclic execution with real-time Linux priority without external PLC daemons.
- **Duration**: Weeks 3 - 4
- **Lead Division**: Software (`head-software`), Hardware (`head-hardware`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-101** | Native STruC++ C-ABI Bridge & Symbol Loader | Software (`head-software`) | `crates/mythos-strucpp/src/ffi.rs`, `symbol.rs`, `glue.h` | Safe Rust wrapper exposing native runtime initialization, cycle step, and memory buffer pointers without external PLC daemons. |
| **TSK-102** | STruC++ Compiler Engine & In-Canvas Deploy | Software (`head-software`) | `crates/mythos-strucpp/src/compiler.rs`, `deploy.rs` | Compiles `.st` logic into native C++17 `.so` dynamic libraries using `strucpp compile` directly on the target controller via `POST /api/v1/control/deploy`. Manual file uploads eliminated. |
| **TSK-103** | Native Cyclic Scheduling & Real-Time Linux Scan Thread | Hardware (`head-hardware`), Software (`head-software`) | `crates/mythos-strucpp/src/runtime.rs` | Attaches cyclic scan loop to `SCHED_FIFO` with real-time priority 80. Scan jitter within $\pm 100\,\mu\text{s}$. |
| **TSK-104** | Dynamic Hot-Reload Subsystem | Software (`head-software`) | `crates/mythos-strucpp/src/hot_reload.rs` | Dynamic library swap using `libloading` completed in $< 50\,\text{ms}$ at end-of-scan boundary without zeroing outputs. |
| **TSK-105** | TagBus Bidirectional Memory Synchronization | Software (`head-software`) | `crates/mythos-strucpp/src/sync.rs` | Copies inputs into STruC++ `%IX` memory and extracts outputs from `%QX` memory into `/dev/shm/mythos_tagbus` per scan cycle. |
| **TSK-106** | IEC 61131-3 Standard Test Program Suite | Hardware (`head-hardware`) | `crates/mythos-strucpp/tests/programs/` | Automated test suites for Timer (TON), Counter (CTU), and State Machine logic verify expected execution. |
| **TSK-107** | Task Scheduler Staging Isolation | Software (`head-software`) | `crates/mythos-strucpp/src/scheduler.rs` | Decouples draft task configuration edits from the running `SCHED_FIFO` loop until atomic project activation. |
| **TSK-108** | Tag Quality Alarm Propagation & Simulation Input Gating | Software (`head-software`) | `crates/mythos-strucpp/src/quality_gate.rs`, `sync.rs` | Enforces `BAD_COMM_FAILURE` (0x0008/0x0018) quality latching on physical field communication loss (SYS-REQ-096). Restricts TagBus manual slider injection and direct variable forcing strictly to devices configured in `ManualSimulation` mode. |

---

### Sprint 2: Layer 2 PROFINET DCP Discovery, Open Hardware Catalog & Dynamic Plugin Packaging
- **Sprint Goal**: Implement raw socket Ethernet broadcast discovery for unassigned industrial devices, establish Open Hardware Catalog ingestion, engineer the Universal Protocol Auto-Discovery Engine, and deliver the `.mythos-plugin` packaging and dynamic hot-reload supervisor.
- **Duration**: Weeks 5 - 6
- **Lead Division**: Hardware (`head-hardware`), Software (`head-software`), Design (`head-design`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-201** | Implement Raw Socket Layer 2 Ethernet Frame Engine | Hardware (`head-hardware`) | `crates/mythos-core/src/discovery/raw_socket.rs` | Transmits and parses IEEE 802.3 frames with EtherType `0x8892` on specified network interfaces (`eth0`, `eth1`). |
| **TSK-202** | Implement PROFINET DCP Discovery Primitives | Hardware (`head-hardware`) | `crates/mythos-core/src/discovery/dcp.rs` | `DCP-Identify`, `DCP-Set-IP`, and `DCP-Signal-LED` commands execute across subnets. Identifies devices within $2.5\,\text{s}$. |
| **TSK-203** | Implement mDNS / Zeroconf Fallback Discovery | Hardware (`head-hardware`) | `crates/mythos-core/src/discovery/mdns.rs` | Discovers registered `_mythos._tcp.local.` services on routed IP networks. |
| **TSK-204** | Build Open Hardware Catalog Parser Engine | Hardware (`head-hardware`), Software (`head-software`) | `crates/mythos-core/src/catalog/mod.rs`, `schema.rs` | Validates device manifests against JSON/YAML schemas. Parses register maps, endianness, and scale ranges. |
| **TSK-205** | Ingest Golden Reference Device Packages | Hardware (`head-hardware`) | `catalog/devices/supmea-p300/`, `catalog/devices/sentec-rainfall/` | Supmea P300 and SenTec Rainfall manifests load cleanly with SVG icons and register mappings verified. |
| **TSK-206** | Develop Device Network Topology Data Model | Software (`head-software`), Hardware (`head-hardware`) | `crates/mythos-core/src/topology.rs` | Constructs graph of discovered devices, parent racks, bus topologies, and assignable IP blocks. |
| **TSK-207** | Implement Per-Device Operational Mode Toggle & Zero-Silent-Fallback Guard | Software (`head-software`), Hardware (`head-hardware`), Design (`head-design`) | `crates/mythos-core/src/device/mode.rs`, `frontend/src/features/inspector/DeviceModeToggle.tsx` | Implements `DeviceExecutionMode` (`PhysicalHardware` vs `ManualSimulation`, SYS-REQ-096). Strictly forbids automatic simulation fallback upon port/cable disconnect. Provides manual inspector toggle, Quick Input Slider, amber `[🧪 SIMULATION ACTIVE]` indicator, and append-only audit trail logging. |
| **TSK-208** | Universal Protocol Discovery Engine & Trait Implementation | Software (`head-software`) | `crates/mythos-core/src/discovery/engine.rs`, `traits.rs` | Implements `ProtocolDiscoverer` async trait. Coordinates Guided Discovery and Blind Bus Sweep. Supports cancellation tokens and enforces fast probe ($\le 1.0\,	ext{s}$) timeout bounds. |
| **TSK-209** | Device Catalog Fingerprinting & Modbus Heuristic Sweep | Hardware (`head-hardware`) | `crates/mythos-core/src/discovery/modbus_sweep.rs`, `crates/mythos-core/src/catalog/fingerprint.rs` | Sweeps baud rates (9600 to 115200), parity, and slave IDs (1 to 247). Matches response registers against catalog signatures. Completes guided sweep in $\le 4.0\,\text{s}$. |
| **TSK-210** | Network View Guided Discovery UI & Bus Scanner Progress Modal | Design (`head-design`) | `frontend/src/features/discovery/DiscoveryModal.tsx`, `BusScannerProgress.tsx` | Dark Obsidian modal displays scan progress, live discovered devices, and cancellation button. Complies with ISA-101 contrast and box-model invariants. |
| **TSK-211** | Plugin Packaging, CLI Toolchain & Dynamic Host Manager | Software (`head-software`) | `crates/mythos-core/src/plugin/`, `crates/mythos-cli/src/plugin.rs` | Builds single-file `.mythos-plugin` packager, capability verifier, and dynamic hot-reload engine without process restarts. |
| **TSK-212** | Universal Bootstrap Scripts (POSIX Bash & Windows PowerShell) | Software (`head-software`), Publication (`head-publication`) | `scripts/install.sh`, `scripts/install.ps1` | Develops one-line bootstrap scripts for `https://get.mythos.firmansyah.co`. Supports `--channel <release|rc|dev>` and `--version <semver>` flags, environment variables, OS/arch detection (`x86_64`, `aarch64`, `armv7l`), SHA-256 verification, and systemd service configuration. |
| **TSK-213** | Multi-Channel CLI Lifecycle Management & Instant Rollback | Software (`head-software`) | `crates/mythos-cli/src/channel.rs`, `update.rs`, `rollback.rs` | Implements `mythos channel [switch <channel>]`, `mythos versions --remote`, `mythos update [--version <semver>]`, and sub-second (`< 1.0 s`) atomic binary rollback via cached versions. |
| **TSK-214** | Web HMI Asset Embedding & Desktop Launcher Registration | Software (`head-software`), Design (`head-design`) | `crates/mythos-server/src/web/`, `scripts/register-launcher.sh` | Embeds production Web HMI static assets into the daemon microkernel binary (served on `http://localhost:8080`). Automatically registers macOS `.app`, Windows shortcuts, and Linux `.desktop` entries during installation. |

---

### Sprint 3: High-Performance ISA-101 HMI, SDUI, Collaborative IDE & In-Canvas Logic Editor
- **Sprint Goal**: Deliver the responsive SCADA web workbench, integrated in-canvas Monaco/Ladder logic editor, TIA Portal-style screen tree, Faceplate Library designer, 3-stage HMI lifecycle (Edit, Preview, Deploy), immersive fullscreen runtime kiosk, and multi-user CRDT collaboration.
- **Duration**: Weeks 7 - 8
- **Lead Division**: Design (`head-design`), Software (`head-software`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-301** | Establish Design Token Architecture (W3C DTCG) | Design (`head-design`) | `docs/04-hmi-and-ui/design-tokens-dtcg.json`, `tokens.css` | Muted Dark Obsidian palette loaded. ISA-101 semantic alarm colors defined (#DC2626, #EA580C, #CA8A04). |
| **TSK-302** | Configure Server-Driven UI Type Generation (`ts-rs`) | Software (`head-software`) | `crates/mythos-core/src/sdui/`, `frontend/src/types/` | Automatically emits TypeScript definitions from Rust backend structs during `cargo test`. Zero manual type sync. |
| **TSK-303** | Develop xyflow Interactive Network Canvas | Design (`head-design`) | `frontend/src/components/NetworkCanvas.tsx` | Drag-and-drop node placement for PLC racks, remote I/O, and sensors. Color-coded bus cable routing (Green/Yellow/Blue/Red). |
| **TSK-304** | Build 3-Tier Visual Faceplate Component Library | Design (`head-design`) | `frontend/src/components/faceplates/` | Tier 1 P&ID Symbol, Tier 2 Summary Card, and Tier 3 Diagnostic Faceplate render dynamically from tag data. |
| **TSK-305** | Implement Function Block Drag-and-Drop Auto-Binding | Design (`head-design`), Software (`head-software`) | `frontend/src/features/binding/` | Dragging FB instance onto canvas binds all faceplate controls to `external.*` tags without manual wiring. |
| **TSK-306** | Develop Slide-Over Inspector Drawer Component | Design (`head-design`) | `frontend/src/components/InspectorDrawer.tsx` | 384px constant non-modal slide-over drawer with 4 tabs: Manual Control, Interlock Matrix Checklist (`[✓]` / `[✗]`), Mini Trend, Maintenance. |
| **TSK-307** | Build Integrated In-Canvas Monaco Logic Editor | Software (`head-software`), Design (`head-design`) | `frontend/src/features/logic/` | Monaco editor for Structured Text and visual Ladder Diagram canvas. Clicking [Deploy to Controller] invokes `POST /api/v1/control/deploy`. Manual file uploads eliminated. |
| **TSK-308** | Implement TIA Portal Screen Hierarchy & 3-Stage Lifecycle | Design (`head-design`), Software (`head-software`) | `frontend/src/features/hmi/`, `useHmiStore.ts` | Screen and Sub-Screen tree in Zone 2. 3-stage lifecycle: Edit Canvas, Non-Actuating Preview Mode (blue banner), and Production Deploy via SSE broadcast (`HMI_LAYOUT_UPDATED`). |
| **TSK-309** | Build Faceplate Library Designer & Global Propagation | Design (`head-design`), Software (`head-software`) | `frontend/src/features/faceplates/` | Dedicated Faceplate Library in Zone 2 tree. Master Faceplate Type designer in Zone 3 with formal UDT bindings. Releasing updates propagates instantly to all instances across screens. |
| **TSK-310** | Implement Immersive Fullscreen Runtime Mode & Kiosk Exit | Software (`head-software`), Design (`head-design`) | `frontend/src/features/runtime/` | Fullscreen kiosk expands canvas and hides all IDE chrome. Discrete exit button enforces IEC 62443 password challenge (`Engineer` / `Admin`) to restore IDE. |
| **TSK-311** | Engineer Real-Time CRDT Collaboration Engine | Software (`head-software`) | `crates/mythos-core/src/collab/`, `frontend/src/features/collab/` | Yjs + `yrs` WebSocket room server. Multi-cursor position rendering and real-time conflict-free synchronized editing over LAN. |
| **TSK-312** | Package One-Click Desktop Installers Suite (Tauri v2) | Software (`head-software`), Publication / QA | `src-tauri/`, `tauri.conf.json`, `scripts/build-installers.sh` | Generates single-click `MythOS-Setup-x64.exe` (with bundled Clang toolchain), `MythOS-Universal.dmg`, and `MythOS-x86_64.AppImage` with zero manual dependencies. |
| **TSK-313** | Execute Deterministic Visual Box-Model Audit | Design (`head-design`) | Box-model test results and UI screenshot validation | Verifies alignment, contrast ratios ($> 7:1$), and zero non-alarm red/yellow color elements. |

---

### Sprint 4: ISA-18.2 Alarms, ISA-88 Recipes & Parquet Historian
- **Sprint Goal**: Deliver the ANSI/ISA-18.2 alarm state engine with First-Out latching, ANSI/ISA-88 batch recipe synchronization, and the Swinging Door Compressed Apache Parquet historian.
- **Duration**: Weeks 9 - 10
- **Lead Division**: R&D (`head-rnd`), Software (`head-software`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-401** | Implement ISA-18.2 Finite State Machine Engine | R&D (`head-rnd`), Software (`head-software`) | `crates/mythos-core/src/alarm/fsm.rs` | Full compliance with ISA-18.2 states (`Normal`, `UnackAlarm`, `AckAlarm`, `UnackRtn`, `Shelved`, `Suppressed`). |
| **TSK-402** | Implement First-Out Monotonic Latching Subsystem | R&D (`head-rnd`) | `crates/mythos-core/src/alarm/first_out.rs` | Microsecond-accurate root-cause fault identification using `CLOCK_MONOTONIC_RAW` nanosecond timestamps. |
| **TSK-403** | Build Deadband Hysteresis & Filter Delay Timers | R&D (`head-rnd`) | `crates/mythos-core/src/alarm/hysteresis.rs` | Eliminates tag chattering. State transitions trigger only when value exceeds span deadband for configured delay. |
| **TSK-404** | Implement ANSI/ISA-88 Batch Recipe Engine | R&D (`head-rnd`), Software (`head-software`) | `crates/mythos-core/src/recipe/` | Decouples Formula from Procedure. Manages PackML phases (`Idle`, `Starting`, `Execute`, `Completing`). |
| **TSK-405** | Build 4-Phase Recipe Memory Synchronization | R&D (`head-rnd`), Software (`head-software`) | `crates/mythos-core/src/recipe/sync.rs` | Atomic parameter transfer over `external.Recipe.*` with SHA-256 digest validation prior to batch start. |
| **TSK-406** | Develop Boxcar-Swinging Door Compression Engine | R&D (`head-rnd`) | `crates/mythos-core/src/historian/sdc.rs` | Bristol SDC algorithm compresses real-time tag streams. Achieves $> 90\%$ compression ratio within error bound $\epsilon$. |
| **TSK-407** | Implement Apache Parquet Chunking & Hive Storage | Software (`head-software`) | `crates/mythos-core/src/historian/parquet.rs` | Batches telemetry in Apache Arrow record batches. Writes daily Parquet chunks with Zstandard Level 3 compression. |
| **TSK-408** | Implement RFC 4180 Selective CSV Exporter API | Software (`head-software`) | `crates/mythos-core/src/historian/csv.rs` | Streams resampled CSV data (Raw, Forward-Fill, Linear Interpolation, Time-Weighted Average). |

---

### Sprint 5: Native MCP Subsystem, Disaster Recovery & Commissioning
- **Sprint Goal**: Embed the native Model Context Protocol server for full AI agent control, package the 1-click encrypted backup container (`.mythos-pkg`), and conduct end-to-end acceptance testing.
- **Duration**: Weeks 11 - 12
- **Lead Division**: Software (`head-software`), Publication (`head-publication`), Enterprise Board

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-501** | Construct Native Rust MCP Server Engine | Software (`head-software`) | `crates/mythos-mcp/src/server.rs`, `transport.rs` | Supports `stdio` and Server-Sent Events (`SSE`) transports adhering to MCP Specification 2024-11-05. |
| **TSK-502** | Develop `#[mythos_mcp_tool]` Procedural Macro | Software (`head-software`) | `crates/mythos-mcp-macros/` | Macro automatically registers Rust functions as MCP tools with JSON schema parameters during startup. |
| **TSK-503** | Register Comprehensive AI Platform Tools | Software (`head-software`) | `crates/mythos-mcp/src/tools/` | Exposes tools for tag reading/writing, alarm acknowledgment, logic compilation, and device discovery. |
| **TSK-504** | Implement Ed25519 Cryptographic Token Authentication | Software (`head-software`) | `crates/mythos-core/src/auth/mcp_auth.rs` | AI agents authenticate using scoped cryptographic tokens. Direct physical actuator toggling is blocked without authorization. |
| **TSK-505** | Build Merkle-Chained JSONL Audit Ledger | Software (`head-software`) | `crates/mythos-core/src/auth/audit.rs` | Append-only audit trail records all human and AI agent actions with SHA-256 hash chains. |
| **TSK-506** | Develop 1-Click Encrypted Disaster Recovery Engine | Software (`head-software`), Publication (`head-publication`) | `crates/mythos-core/src/recovery/` | Packs logic, configs, recipes, and users into `.mythos-pkg` using ChaCha20-Poly1305. Hot-restore completes in $< 25\,\text{s}$. |
| **TSK-507** | Conduct End-to-End System Commissioning & Audit | Publication (`head-publication`), All Kadivs | Full test reports, operator handbook, SOP manuals | 100% test pass rate across all verification criteria in `specs/spec.md`. Board approval signed off. |

---
---

### Sprint 6: Siemens TIA Portal View Launchpad, Clean Empty Project Lifecycle & Full Dynamic Workbench Wiring
- **Sprint Goal**: Deliver the dual-mode Siemens TIA Portal workspace with Portal View (Welcome Screen and Task Launchpad) and Project View (5-Zone Workbench), implement clean empty project initialization with zero hardcoded mocks, wire end-to-end interactive document opening into Dockview tabs, provide dynamic block and screen creation modals, and bind active selection context to the constant 384px inspector drawer.
- **Duration**: Weeks 13 - 14
- **Lead Division**: Software (`head-software`), Design (`head-design`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-601** | Siemens TIA Portal "Portal View" Welcome Screen & Bottom-Left Dual-Mode Switcher (`[ 🌐 Portal View ]` <---> `[ 🎛️ Project View ]`) | Software (`head-software`), Design (`head-design`) | `frontend/packages/ui-shell/src/components/portal/PortalView.tsx`, `frontend/packages/ui-shell/src/App.tsx`, `frontend/packages/ui-shell/src/components/portal/PortalNav.tsx` | Bottom-left persistent switcher toggles between Portal View and Project View instantly. Portal View presents 6 canonical navigation domains: Start, Devices & Networks, PLC Programming, Motion & Technology, Visualization, and Online & Diagnostics. Action triggers seamless contextual transition to Project View. |
| **TSK-602** | Zero-Mock Clean Empty Project State & Lifecycle Manager (Create, Open, Import .mythos-pkg via `/api/v1/projects`) | Software (`head-software`) | `frontend/packages/ui-shell/src/stores/useProjectStore.ts`, `frontend/packages/ui-shell/src/components/workbench/Zone2ProjectTree.tsx`, `frontend/packages/ui-shell/src/components/portal/ProjectLifecycleModal.tsx` | Completely eliminates hardcoded mock projects and phantom blocks. Displays clean empty states in project tree and central canvas when no objects exist. REST API integration provides live project creation, opening, and `.mythos-pkg` archive import via `/api/v1/projects`. |
| **TSK-603** | Full Project Tree Dynamic Wiring & Dynamic Block/Screen Creation | Software (`head-software`) | `frontend/packages/ui-shell/src/components/workbench/DockviewWorkspace.tsx`, `frontend/packages/ui-shell/src/components/modals/CreateBlockModal.tsx`, `frontend/packages/ui-shell/src/components/modals/CreateScreenModal.tsx` | Real interactive document router in Dockview opens and focuses tabs for ST logic blocks, HMI screens, and Devices/Topology views. Dynamic creation modals allow engineers to instantiate OB, FB, FC, and DB blocks, and HMI screens with immediate tree update and tab opening. |
| **TSK-604** | Active Selection Context Binding to Constant 384px Inspector Drawer | Software (`head-software`), Design (`head-design`) | `frontend/packages/ui-shell/src/components/workbench/Zone4TaskCards.tsx`, `frontend/plugins/ui-inspector/src/InspectorDrawer.tsx` | Inspector drawer maintains strict 384px layout token width. Zone 4 and bottom inspector dynamically synchronize to the actively selected tree node, logic block, or canvas element, rendering real properties, tag bindings, and diagnostics without static mockups. |

---

### Sprint 7: Universal IEC 61131-3 Logic IDE Parity (OpenPLC Editor v4 Complete Replication)
- **Sprint Goal**: Deliver complete IDE parity with OpenPLC Editor v4 and Siemens STEP 7 across all five IEC 61131-3 languages (ST, IL, LD, FBD, SFC), construct the variable table grid with user-defined data type (DUT/UDT) definitions, and implement live online debugging with power-flow animation and variable forcing.
- **Duration**: Weeks 15 - 16
- **Lead Division**: Software (`head-software`), Hardware (`head-hardware`), R&D (`head-rnd`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-701** | Monaco Multi-Language Textual Editor Suite (Structured Text ST & Instruction List IL) | Software (`head-software`) | `frontend/plugins/ui-logic-editor/src/MonacoStEditor.tsx`, `frontend/plugins/ui-logic-editor/src/ilLanguage.ts`, `frontend/plugins/ui-logic-editor/src/stLanguage.ts` | Monaco editor provides full syntax highlighting, keyword autocompletion, bracket matching, linting, and hover tooltips for Structured Text (ST) and Instruction List (IL). Integrates with backend compiler diagnostics. |
| **TSK-702** | Full-Featured Graphical Ladder Diagram (LD) Canvas (Power rails, contacts, coils, branches, function blocks) | Software (`head-software`), Design (`head-design`) | `frontend/plugins/ui-logic-editor/src/LadderCanvas.tsx`, `frontend/plugins/ui-logic-editor/src/ladder/RungRenderer.tsx`, `frontend/plugins/ui-logic-editor/src/ladder/LadderElements.tsx` | Renders dual power rails, normally open (NO) and normally closed (NC) contacts, pulse transitions (P/N), standard/set/reset coils, parallel branch circuits, and inline function block invocations (TON, TOF, TP, CTU, CTD). Supports drag-and-drop from Zone 4 task cards. |
| **TSK-703** | Function Block Diagram (FBD) & Sequential Function Chart (SFC) Visual Canvas | Software (`head-software`), Design (`head-design`) | `frontend/plugins/ui-logic-editor/src/fbd/FbdCanvas.tsx`, `frontend/plugins/ui-logic-editor/src/sfc/SfcCanvas.tsx` | FBD canvas renders modular interconnected logic gates and mathematical blocks with typed input/output pins and negation flags. SFC canvas renders initial/step states, directed links, transition conditions, and action qualifiers (N, S, R, P, L, D). |
| **TSK-704** | Variable Table Grid & Data Type Editor (POU VAR table, GVL, DUT/UDT Structs/Enums) | Software (`head-software`) | `frontend/plugins/ui-logic-editor/src/variables/VariableTableGrid.tsx`, `frontend/plugins/ui-logic-editor/src/variables/DataTypeEditor.tsx` | Provides spreadsheet-style declaration grid for POU scoped variables (`VAR`, `VAR_INPUT`, `VAR_OUTPUT`, `VAR_IN_OUT`, `VAR_TEMP`), Global Variable Lists (`GVL`), and User-Defined Data Types (`DUT/UDT` structures and enumerations) with IEC memory addresses (`%IX`, `%QX`, `%MX`). |
| **TSK-705** | Live Online Debugging, Power-Flow Animation & Variable Forcing (SYS-REQ-096 compliant) | Software (`head-software`), Hardware (`head-hardware`), R&D (`head-rnd`) | `frontend/plugins/ui-logic-editor/src/debug/OnlineDebugEngine.ts`, `frontend/plugins/ui-logic-editor/src/debug/PowerFlowOverlay.tsx` | Real-time telemetry stream animates LD power rails and rungs in vibrant green (`#22C55E`) when active. ST editor displays inline variable values. Variable force table enables manual override with audit logging in compliance with SYS-REQ-096 safety invariants. |
---

### Sprint 8: Live Telemetry Cutover & Device-Mode Completion (next slice)
- **Sprint Goal**: Prove that one real TagBus value reaches one HMI faceplate, and complete the SYS-REQ-096 device-mode path that the Sprint 0 honesty work deferred.
- **Status**: PRE-COMMITTED PREREQUISITE. Not started. Recorded before sprint planning so the gap cannot be lost again. This is a next-slice prerequisite, not a retroactive defect against the merged HMI honesty work.
- **Precondition**: The canonical `DeviceExecutionMode` ownership decision must be resolved before implementation begins. See the ACCEPTED DUPLICATION note below.
- **Lead Division**: Software (`head-software`), Design (`head-design`)

| Task ID | Task Description | Assigned Division | Deliverables & Code Artifacts | Acceptance Criteria |
|---|---|---|---|---|
| **TSK-801** | HMI Live-Telemetry Consumer & SYS-REQ-096 Device-Mode End-to-End Path | Software (`head-software`), Design (`head-design`) | Consumer for the existing `GET /api/v1/telemetry/stream` producer (`crates/mythos-core/src/api/telemetry.rs`); quality input on `frontend/plugins/ui-faceplate-lib/src/components/`; a new device-mode route under `crates/mythos-core/src/api/`; operator path into `crates/mythos-core/src/device/mode.rs` | **Telemetry**: consumes `GET /api/v1/telemetry/stream`; resolves one commissioned tag to one HMI faceplate; updates the UI at the defined 100 ms (10 Hz) cadence; maps `QualityCode` to faceplate quality state; survives reconnect with bounded backoff; ships a live-path test. **Device mode**: operator mode control, `SIMULATION_ACTIVE` propagation, quality-latch visibility to the HMI, and the amber `[SIMULATION ACTIVE]` badge. Acceptance test: sever communication, observe the alarm, toggle to simulation, observe the badge. **Two known blockers for quality visibility**, both verified in code and neither solvable on the UI alone: (1) `QualityGate::record_comm_loss` latches in a process-local map and does not write `TagSlot.quality`, so the failure never reaches shared memory (`crates/mythos-strucpp/src/quality_gate.rs`); (2) the SSE payload collapses quality to a three-state string `GOOD`/`BAD`/`UNCERTAIN` (`crates/mythos-core/src/api/telemetry.rs`, `build_telemetry_event`), so a faceplate cannot distinguish `BAD_COMM_FAILURE` from any other bad state unless the payload carries the raw quality word. |

**Scope limit.** This slice proves one real tag-to-faceplate path. Broader subscription and binding architecture is follow-up. Do not expand the slice past that proof.

> **ACCEPTED DUPLICATION (1.4.0): `DeviceExecutionMode` is defined twice, deliberately and temporarily.**
>
> - `crates/mythos-core/src/device/mode.rs:30`
> - `crates/mythos-strucpp/src/quality_gate.rs:13`
>
> `mythos-strucpp` already depends on `mythos-core` (`crates/mythos-strucpp/Cargo.toml:16`, `mythos-core.workspace = true`), and there is no reverse dependency, so collapsing the two enums would create no cycle. The two definitions are not collapsed in Sprint 0; the duplication is an accepted, recorded choice, not an oversight. Its real cost is a wire-format divergence, verified empirically: `mythos-core`'s enum carries `#[serde(rename_all = "snake_case")]` and `Default`, `mythos-strucpp`'s carries neither, so the first variant serialises as `"physical_hardware"` in one crate and `"PhysicalHardware"` in the other.
>
> **Precondition on TSK-801.** The canonical ownership decision must be resolved before the slice is implemented. Because `mythos-strucpp` already depends on `mythos-core`, the natural resolution is for `mythos-strucpp` to adopt the `mythos-core` enum, but doing so changes the wire name of any serialised value and requires a migration. This document does not pick the resolution, because choosing one is an architecture decision, not a documentation edit.

## 3. Milestones & Quality Gates

```
+---------------------------------------------------------------------------------------------------+
| GATE 0: Architecture & Foundations Sign-Off (End of Sprint 0)                                     |
| Criteria: Cargo workspace compiles cleanly; SHM benchmark < 50 ns; target storage & WAL verified;  |
|           Axum REST Gateway operational; Universal Plugin SDK trait & host harness validated.      |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+---------------------------------------------------------------------------------------------------+
| GATE 1: Logic Engine & Control Loop Sign-Off (End of Sprint 1)                                    |
| Criteria: Native STruC++ compiles ST logic via STruC++; in-canvas POST /deploy hot-reloads < 50 ms;|
|           Linux real-time thread runs with < 100 us jitter bounds; scheduler staging isolated.    |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+---------------------------------------------------------------------------------------------------+
| GATE 2: Discovery, Catalog, Distribution & Plugin Packaging Sign-Off (End of Sprint 2)            |
| Criteria: Layer 2 DCP identifies unassigned devices; golden reference device packages ingested;  |
|           universal sweep runs <= 4.0 s; .mythos-plugin packager & dynamic hot-reload verified;   |
|           universal bootstrap scripts, multi-channel CLI, and instant rollback validated.          |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+---------------------------------------------------------------------------------------------------+
| GATE 3: Visual Presentation, In-Canvas IDE & Runtime Kiosk Sign-Off (End of Sprint 3)               |
| Criteria: Dark Obsidian UI validated; Monaco ST logic editor deployed atomically; TIA Portal       |
|           Screen tree and 3-stage lifecycle verified; Faceplate master types propagate globally;   |
|           Fullscreen runtime kiosk & IEC 62443 exit challenge pass; CRDT collaboration syncs.     |
| STATUS (1.3.0): REOPENED - NOT SIGNED OFF. TSK-309 is unmet. No Master Faceplate Type designer    |
|           and no release-time global propagation exist in code on dev; the criterion "Faceplate    |
|           master types propagate globally" above was recorded as passed without mechanical        |
|           evidence (.openrig/evidence/dod-spot-audit.md). Gate 3 closes only on evidence for an   |
|           authoring designer with formal UDT bindings and a propagation test covering every       |
|           instance. All other Gate 3 criteria remain as previously recorded.                      |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+---------------------------------------------------------------------------------------------------+
| GATE 4: Process Engines & Storage Sign-Off (End of Sprint 4)                                      |
| Criteria: ISA-18.2 First-Out captures initiating fault; SDC Parquet achieves > 90% compression.   |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+---------------------------------------------------------------------------------------------------+
| GATE 5: AI Agent & Enterprise Production Sign-Off (End of Sprint 5)                               |
| Criteria: MCP tools pass end-to-end AI control; .mythos-pkg restores replacement IPC in < 25 s.   |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+---------------------------------------------------------------------------------------------------+
| GATE 6: Siemens TIA Portal Workspace & Dynamic Lifecycle Sign-Off (End of Sprint 6)               |
| Criteria: Dual-mode Portal/Project View switcher verified; clean zero-mock project lifecycle;      |
|           dynamic Dockview tab wiring operational; dynamic block/screen creation validated;       |
|           constant 384px inspector drawer context-sensitive binding active.                       |
+---------------------------------------------------------------------------------------------------+
                                                  |
                                                  v
+---------------------------------------------------------------------------------------------------+
| GATE 7: Universal IEC 61131-3 Logic IDE & Online Debugging Sign-Off (End of Sprint 7)             |
| Criteria: Full 5-language parity (ST, IL, LD, FBD, SFC) verified against OpenPLC Editor v4;       |
|           Spreadsheet variable table & DUT/UDT editor validated; live power-flow animation and    |
|           variable forcing operational with SYS-REQ-096 safety compliance.                        |
+---------------------------------------------------------------------------------------------------+
```

---

## 4. Responsibility Assignment Matrix (RACI)

| Discipline / Subsystem | Neo (Software) | Vulcan (Hardware) | Iris (Design) | Sage (R&D) | Scribe (Publication) | Adrian / Aria (Governance) |
|---|---|---|---|---|---|---|
| **Cargo Workspace & Microkernel** | **Accountable** | Consulted | Informed | Consulted | Responsible (Specs) | Approver |
| **SHM Lock-Free TagBus** | **Accountable** | Consulted | Informed | Consulted | Responsible (Specs) | Approver |
| **Native STruC++ Runtime Engine** | **Accountable** | Responsible | Informed | Informed | Responsible (Specs) | Approver |
| **Layer 2 DCP Discovery** | Responsible | **Accountable** | Informed | Informed | Responsible (Specs) | Approver |
| **Universal Protocol Auto-Discovery** | **Accountable** | Responsible | Responsible | Informed | Responsible (Specs) | Approver |
| **Device Catalog Packages** | Consulted | **Accountable** | Informed | Informed | Responsible (Specs) | Approver |
| **ISA-101 HMI & Design Tokens** | Responsible | Informed | **Accountable** | Informed | Responsible (Specs) | Approver |
| **xyflow Network Canvas** | Responsible | Consulted | **Accountable** | Informed | Responsible (Specs) | Approver |
| **ISA-18.2 Alarm Engine** | Responsible | Informed | Consulted | **Accountable** | Responsible (Specs) | Approver |
| **ISA-88 Batch Recipes** | Responsible | Informed | Consulted | **Accountable** | Responsible (Specs) | Approver |
| **Parquet Historian & SDC** | Responsible | Informed | Informed | **Accountable** | Responsible (Specs) | Approver |
| **Native MCP AI Subsystem** | **Accountable** | Informed | Informed | Consulted | Responsible (Specs) | Approver |
| **Universal Plugin Harness & SDK** | **Accountable** | Consulted | Responsible | Consulted | Responsible (Specs) | Approver |
| **Plugin Dynamic Host & Packaging** | **Accountable** | Consulted | Informed | Consulted | Responsible (Specs) | Approver |
| **Universal Bootstrap & Multi-Channel CLI** | **Accountable** | Consulted | Consulted | Informed | Responsible (Specs) | Approver |
| **Modular Frontend Plugin Shell** | Responsible | Informed | **Accountable** | Informed | Responsible (Specs) | Approver |
| **Target Controller Storage & WAL**| **Accountable** | Consulted | Informed | Informed | Responsible (Specs) | Approver |
| **In-Canvas Logic IDE & Deploy** | **Accountable** | Responsible | Responsible | Informed | Responsible (Specs) | Approver |
| **TIA Portal HMI & Screen Tree** | Responsible | Informed | **Accountable** | Informed | Responsible (Specs) | Approver |
| **Faceplate Library Designer** | Responsible | Informed | **Accountable** | Informed | Responsible (Specs) | Approver |
| **CRDT Real-Time Collaboration** | **Accountable** | Informed | Responsible | Informed | Responsible (Specs) | Approver |
| **Disaster Recovery (.mythos-pkg)**| **Accountable** | Consulted | Informed | Informed | Responsible (Specs) | Approver |
| **Portal View & Dynamic Lifecycle**| **Accountable** | Informed | Responsible | Informed | Responsible (Specs) | Approver |
| **Universal IEC 61131-3 Logic IDE**| **Accountable** | Responsible | Responsible | Consulted | Responsible (Specs) | Approver |
| **Variable Table Grid & UDT Editor**| **Accountable** | Consulted | Responsible | Informed | Responsible (Specs) | Approver |
| **Online Logic Debugging & Forcing**| **Accountable** | Responsible | Responsible | Consulted | Responsible (Specs) | Approver |
| **Master Documentation & SOP** | Consulted | Consulted | Consulted | Consulted | **Accountable** | Approver |

---

## 5. Traceability to System Specifications & ADRs

| Sprint | Direct Requirement Trace | Architectural Decisions & Specifications Governed |
|---|---|---|
| **Sprint 0** | SYS-REQ-001, 002, 003, 004, 005, 010, 011, 012, 013, 016, 017, 018, 019, 030, 031, 032, 033, 034, 075, 076, 077, 086, 087 | ADR-001, ADR-002, ADR-008, ARCH-005, ARCH-006, DATA-004 |
| **Sprint 1** | SYS-REQ-020, 021, 022, 023, 024, 025, 080, 096 | ADR-003, ARCH-005, ARCH-002, DATA-003 |
| **Sprint 2** | SYS-REQ-040, 041, 042, 043, 044, 045, 074, 088, 089, 095, 096 | ADR-004, ADR-008, ARCH-006, ARCH-002, ARCH-008, HMI-000 |
| **Sprint 3** | SYS-REQ-050, 051, 052, 053, 054, 055, 056, 057, 078, 079, 081, 082, 083, 084, 085, 090 | ADR-006, ADR-008, HMI-000, ARCH-005, ARCH-006 |
| **Sprint 4** | SYS-REQ-060, 061, 062, 063, 064, 065, 066 | ADR-007 |
| **Sprint 5** | SYS-REQ-067, 068, 070, 071, 072, 073 | ADR-005, ADR-007 |
| **Sprint 6** | SYS-REQ-097, 098 | ADR-006, ADR-008, HMI-000, ARCH-005 |
| **Sprint 7** | SYS-REQ-021, 023, 096, 099 | ADR-003, ADR-006, HMI-000, ARCH-005 |
| **Sprint 8** | SYS-REQ-054, 055, 096 | ADR-006, ADR-008, ARCH-002, HMI-000 |

---

## 6. Revision History

| Version | Date | Author | Change |
|---|---|---|---|
| 1.2.0 | — | Scribe (VP Publication & Documentation) | Baseline approved by Adrian (Board of Commissioners) and Aria (CEO). |
| 1.3.0 | 2026-10-23 | Scribe (VP Publication & Documentation) | ADR-008 propagation. TSK-001 and TSK-004 now name `crates/mythos-plugin-sdk` and the real workspace members. TSK-005 now names `plugins/drivers/driver-modbus/`. GATE 3 status recorded as REOPENED: TSK-309 was not delivered and had no mechanical evidence. |
| 1.4.0 | 2026-10-26 | Scribe (VP Publication & Technical Documentation) | TSK-010 criterion revised: the $< 100\,\text{ns}$ p99-under-16-threads acceptance criterion is withdrawn as a merge gate and recorded as not met, with the measurement preserved (PR #25, run 40089446143, merged at `333d199`) and the 16-thread step reduced to report-only. No replacement bound is selected; that is a board decision. New Sprint 8 records TSK-801 as a pre-committed prerequisite: the live-telemetry consumer plus the SYS-REQ-096 device-mode end-to-end path. `DeviceExecutionMode` duplication recorded as accepted and temporary; its real cost is a serde wire-name divergence between the two definitions, with the canonical ownership decision as a precondition on TSK-801. |
