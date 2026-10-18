# MythOS Master System Specification (SyRS)

- **Document Identifier**: MYTHOS-SPEC-001
- **Standard Compliance**: ISO/IEC/IEEE 29148:2018 (Systems and Software Engineering — Life Cycle Processes — Requirements Engineering)
- **Controlled Language**: ASD-STE100 Simplified Technical English
- **Version**: 1.2.0
- **Status**: APPROVED / MASTER BASELINE
- **Author**: Scribe (VP of Publication & Technical Documentation)
- **Architectural Contributors**: Neo (VP Software), Vulcan (VP Hardware), Iris (VP Design), Sage (VP R&D)
- **Approved by**: Adrian (Board of Commissioners), Aria (Chief Executive Officer)

---

## 1. Introduction

### 1.1 Purpose
This document specifies the system requirements for MythOS. MythOS is an industrial-grade Supervisory Control and Data Acquisition (SCADA) and soft-logic runtime platform. MythOS executes in pure Rust. MythOS integrates the Native STruC++ Runtime Engine directly, eliminating external PLC daemons. This specification eliminates technical assumptions before implementation begins.

### 1.2 Scope
MythOS controls industrial manufacturing plants, utilities, palm oil mills, and municipal water facilities. MythOS provides:
1. Sub-millisecond cyclic control execution via the embedded Native STruC++ Runtime Engine.
2. High-throughput, lock-free, triple-buffered shared memory tag distribution (`/dev/shm/mythos_tagbus`).
3. Isolated, modular protocol driver execution via a strictly bounded Service Provider Interface (SPI).
4. Subnet-agnostic Layer 2 Ethernet device discovery via PROFINET Discovery and Configuration Protocol (DCP).
5. Open hardware device catalog with vendor-agnostic manifest ingestion.
6. ANSI/ISA-101 High-Performance HMI with interactive node-graph network topology views.
7. 100% integrated in-canvas IEC 61131-3 logic authoring (Monaco Structured Text + visual Ladder Canvas) with atomic zero-downtime hot-reloading (`POST /api/v1/control/deploy`), eliminating manual file uploads.
8. Target device-first primary storage on the industrial controller (`/var/lib/mythos/projects/<name>/`) with autonomous 24/7 standalone execution and workstation offline sync (`push`/`pull`).
9. Locked 100ms (10 Hz) standard SSE telemetry refresh stream preventing UI DOM thrashing.
10. Multi-project storage, background project editing, and atomic runtime hot-switching without process restarts.
11. Debounced atomic autosave with Write-Ahead Logging (`project.wal`) guaranteeing zero configuration loss.
12. Multi-user real-time collaborative editing using Conflict-Free Replicated Data Types (CRDT via Yjs / `yrs`) for Monaco multi-cursor and visual canvas synchronization over LAN.
13. Reusable Function Block Faceplate binding and slide-over Inspector Drawer diagnostics.
14. ANSI/ISA-18.2 compliant industrial alarm management with First-Out latching and flood suppression.
15. ANSI/ISA-88 Batch Control and PackML recipe state machine synchronization.
16. Embedded columnar time-series historian using Boxcar-Swinging Door Compression (SDC) and Apache Parquet storage.
17. Full-surface Model Context Protocol (MCP) server for autonomous AI agent inspection, ladder modification, and telemetry control.
18. 1-Click encrypted disaster recovery container (`.mythos-pkg`) enabling hot-swapping in under 30 seconds.

### 1.3 Definitions and Acronyms
- **ADR**: Architectural Decision Record.
- **ASD-STE100**: Aerospace and Defence Industries Association of Europe Simplified Technical English.
- **AST**: Abstract Syntax Tree.
- **CRDT**: Conflict-Free Replicated Data Type.
- **DCP**: Discovery and Configuration Protocol (PROFINET Layer 2).
- **FFI**: Foreign Function Interface.
- **HMI**: Human-Machine Interface.
- **IPC**: Industrial Personal Computer.
- **ISA**: International Society of Automation.
- **MCP**: Model Context Protocol (standard for AI agent tool integration).
- **MTTR**: Mean Time To Recovery.
- **NVRAM**: Non-Volatile Random Access Memory.
- **STruC++**: Structured Text to C++ compiler and native runtime execution framework for IEC 61131-3 control logic.
- **POD**: Plain Old Data (C-compatible, copyable memory layout).
- **RBAC**: Role-Based Access Control.
- **SDC**: Swinging Door Compression.
- **SDUI**: Server-Driven User Interface.
- **SHM**: Shared Memory.
- **SPI**: Service Provider Interface.
- **SSE**: Server-Sent Events.
- **STruC++**: Structured Text compiler generating native C++ / dynamic shared objects (`.so`).
- **UDT**: User-Defined Type (IEC 61131-3 `STRUCT`).
- **WAL**: Write-Ahead Log.
- **Yjs / yrs**: Real-time collaborative framework and CRDT engine in JavaScript and pure Rust.

### 1.4 References
1. ISO/IEC/IEEE 29148:2018: Systems and software engineering — Requirements engineering.
2. IEC 61131-3:2013: Programmable controllers — Part 3: Programming languages.
3. ANSI/ISA-101.01-2015: Human-Machine Interfaces for Process Automation Systems.
4. ANSI/ISA-18.2-2016: Management of Alarm Systems for the Process Industries.
5. ANSI/ISA-88.01-2010: Batch Control — Part 1: Models and Terminology.
6. IEC 62443-4-2: Security for industrial automation and control systems — Technical security requirements for IACS components.
7. RFC 4180: Common Format and MIME Type for Comma-Separated Values (CSV) Files.
8. OpenAPI Specification v3.1.0: Standard specification for machine-readable REST interfaces.
9. Yjs CRDT Specification: High-performance shared editing data model for collaborative workbenches.
10. `docs/01-architecture/05-http-sse-and-websocket-api.md`: HTTP, SSE, and WebSocket API Architecture.
11. `docs/02-data-and-addressing/project-file-and-directory-structure.md`: Project File and Storage Structure.
12. `docs/04-hmi-and-ui/00-frontend-application-architecture.md`: Frontend Application Architecture.

---

## 2. Overall Description

### 2.1 Product Perspective
MythOS replaces legacy monolithic SCADA runtimes. Legacy runtimes leak protocol configurations into core states. Alarm storms crash browser user interfaces. Hardware failure requires hours of manual reconfiguration.

MythOS adopts a microkernel actor architecture in pure Rust. All subsystems communicate through typed asynchronous message channels and a triple-buffered, lock-free Shared Memory bus. The system isolates protocol drivers into sandboxed dynamic crates. The system decouples visual presentation from state execution through Server-Driven UI (SDUI).

The target controller (Raspberry Pi Compute Module 4 or Industrial PC inside the cabinet) acts as the primary hardware host. The target controller stores projects persistently under `/var/lib/mythos/projects/` and runs soft-logic autonomously 24/7. Engineering laptops connect over LAN to edit logic in an integrated Monaco/Ladder IDE, receive 100ms throttled telemetry streams, collaborate in real time via CRDT, and perform atomic zero-downtime hot-deployments without manual file uploads.

```
+---------------------------------------------------------------------------------------------------+
|                                      MYTHOS TOPOLOGY & DATA FLOW                                  |
+---------------------------------------------------------------------------------------------------+
|                                                                                                   |
|  +------------------------------------------------------+  +-----------------------------------+  |
|  | INTEGRATED WEB WORKBENCH (React 19 + Dockview)       |  | AI AGENT SUBSYSTEM (Claude/Local) |  |
|  | - In-Canvas Logic Editor (Monaco ST + Ladder Canvas) |  | - Native MCP Client               |  |
|  | - xyflow Topology & ISA-101 Modular Faceplates       |  | - Automated Tag & Alarm Control   |  |
|  | - Real-Time Collaborative CRDT (Yjs Multi-Cursor)   |  | - Stdio / SSE Transport           |  |
|  +--------------------------+---------------------------+  +-----------------+-----------------+  |
|                             |                                                |                    |
|       100ms SSE Telemetry / | POST /api/v1/control/deploy                    | JSON-RPC (MCP)     |
|       CRDT WebSockets       | (Atomic STruC++ Compile & Hot-Reload)          |                    |
|                             |                                                |                    |
|  +--------------------------v------------------------------------------------v-----------------+  |
|  | TARGET INDUSTRIAL CONTROLLER (Primary Single Source of Truth: /var/lib/mythos/projects/)   |  |
|  |                                                                                             |  |
|  |  +-------------------------+  +---------------------------+  +---------------------------+  |  |
|  |  | Axum REST & SSE Gateway |  | Yrs CRDT Collaboration    |  | Model Context Protocol    |  |  |
|  |  | (100ms Throttled Stream)|  | (WebSocket Room Server)   |  | (#[mythos_mcp_tool] Macro)|  |  |
|  |  +------------+------------+  +-------------+-------------+  +-------------+-------------+  |  |
|  |               |                             |                              |                |  |
|  |  +------------v-----------------------------v------------------------------v-------------+  |  |
|  |  |                              ACTOR SUPERVISION BUS (Tokio)                            |  |  |
|  |  +------------+-----------------------------+------------------------------+-------------+  |  |
|  |               |                             |                              |                |  |
|  |  +------------v------------+  +-------------v-------------+  +-------------v-------------+  |  |
|  |  | ISA-18.2 Alarm Actor    |  | ISA-88 Batch Recipe Actor |  | Columnar Historian        |  |  |
|  |  | (First-Out Monotonic)   |  | (4-Phase Handshake)       |  | (Boxcar SDC + Parquet)    |  |  |
|  |  +------------+------------+  +-------------+-------------+  +-------------+-------------+  |  |
|  |               |                             |                              |                |  |
|  |  +------------v-----------------------------v------------------------------v-------------+  |  |
|  |  |         TRIPLE-BUFFERED LOCK-FREE SHARED MEMORY BUS (/dev/shm/mythos_tagbus)          |  |  |
|  |  |         Atomic Pointer Swap | 64-Byte Cache Aligned | 32,768 POD Tag Slots            |  |  |
|  |  +------------^-----------------------------^------------------------------^-------------+  |  |
|  |               |                             |                              |                |  |
|  |  +------------+------------+  +-------------+-------------+  +-------------+-------------+  |  |
|  |  | Native STruC++ Engine    |  | Isolated Driver SPI Crates|  | Layer 2 DCP Discovery     |  |  |
|  |  | (STruC++ C-ABI / .so)   |  | (Modbus TCP/RTU Actors)   |  | (Raw PROFINET Frames)     |  |  |
|  |  +-------------------------+  +---------------------------+  +---------------------------+  |  |
|  |                                                                                             |  |
|  |  +---------------------------------------------------------------------------------------+  |  |
|  |  | CONTROLLER STORAGE BACKING: /var/lib/mythos/projects/<active>/ (project.json, WAL)     |  |  |
|  |  +---------------------------------------------------------------------------------------+  |  |
|  +---------------------------------------------------------------------------------------------+  |
+---------------------------------------------------------------------------------------------------+
```

### 2.2 System Invariants
1. **Zero Monolithic Leakage**: Protocol drivers exist in isolated crates (`drivers/driver-*`). A driver crash or defect cannot terminate the core runtime or other drivers.
2. **Pure Rust Execution**: The core daemon, driver runtime, alarm engine, historian, and MCP server use safe Rust. The Native STruC++ Runtime Engine integrates exclusively through strict C-ABI Foreign Function Interfaces.
3. **Pure Dumb Frontend**: The user interface executes as a stateless consumer. The frontend never computes tag engineering scales, alarm thresholds, or interlock logic. The backend transmits Server-Driven UI definitions.
4. **Sub-Millisecond Tag Access**: All read operations on the TagBus complete in less than 50 nanoseconds. No mutexes or lock primitives guard the real-time scan cycle.
5. **AI Accessibility**: Every platform capability exposes an equivalent Model Context Protocol (MCP) tool. AI agents inspect and configure the platform with full cryptographic auditability.
6. **Physical Plant Hierarchy & Tag Isolation**: Tag addressing represents physical plant topology, not communication transport protocols. Transport protocol names never appear in tag URIs. External field tags strictly use the canonical hierarchy `external.<namespace>.<device>.<variable>[.<subfield>]` (such as `external.BoilerRoom.PT101.pressure`). The `external` prefix applies only to external field devices. Internal PLC variables, local calculations, and intermediate registers omit the `external` prefix.
7. **Universal Plugin Harness Architecture ("Everything is a Plugin")**: The core microkernel decouples strictly from domain logic. All protocol drivers, process engines, HMI widgets, logic extensions, and discovery scanners operate as independent plugins. Both first-party features and third-party modules implement the identical plugin contract. The platform enforces capability-based sandboxing, dynamic hot-reload, and zero crash propagation across the runtime boundary.
8. **Universal Cross-Platform & Container Parity**: The platform shall execute with identical functional behavior across Linux, Windows 11, macOS, and Docker containers via the Platform Abstraction Layer (PAL). Hardware-specific execution constraints adapt deterministically without application code divergence.
9. **Target Controller as Primary Source of Truth**: The controller in the cabinet hosts the master project filesystem (`/var/lib/mythos/projects/`). The controller operates autonomously without dependency on active workstation connections.
10. **Zero Manual File Upload**: Soft-logic deployment executes via an integrated in-canvas editor and an atomic REST compilation API (`POST /api/v1/control/deploy`). Manual binary or file uploads are forbidden.

---

## 3. System Capabilities and Functional Requirements

### 3.1 Software Architecture & Microkernel (`crates/mythos-core`, `crates/mythos-shm`)

- **SYS-REQ-001 [Actor Supervision]**: The microkernel shall supervise all operational actors using the Tokio asynchronous runtime. If an actor panics, the supervisor shall restart the actor within 100 milliseconds without interrupting adjacent actors.
- **SYS-REQ-002 [Lock-Free Shared Memory]**: The system shall allocate a POSIX shared memory block at `/dev/shm/mythos_tagbus`. The block holds exactly 32,768 Plain Old Data (POD) slots.
- **SYS-REQ-003 [Triple-Buffering]**: The TagBus shall implement a lock-free triple-buffer algorithm using an atomic 64-bit state descriptor (`AtomicU64`). The algorithm shall partition slots into `ReadBuffer`, `WriteBuffer`, and `StandbyBuffer`.
- **SYS-REQ-004 [Cache-Line Alignment]**: Every tag slot shall align to 64-byte hardware boundaries (`#[repr(align(64))]`). This alignment eliminates false sharing across CPU cores.
- **SYS-REQ-005 [Zero Allocation Loop]**: The core cyclic update loop shall perform zero heap allocations (`malloc`/`Box::new`) during steady-state execution.

### 3.2 Protocol Driver Service Provider Interface (`crates/mythos-driver-api`)

- **SYS-REQ-010 [Isolated Driver SPI]**: Every driver shall implement the asynchronous `ProtocolDriver` Rust trait (`init`, `start`, `poll_cycle`, `write_tag`, `health_check`, `stop`).
- **SYS-REQ-011 [Opaque Configuration Validation]**: Drivers shall validate configurations with isolated JSON schemas. Malformed parameters in one driver shall not affect other drivers.
- **SYS-REQ-012 [Bounded Cycle Execution]**: The supervisor shall enforce a maximum polling timeout per cycle (10 ms to 10,000 ms). On timeout, the driver shall mark tags with `Quality::BAD_COMM_FAILURE` and proceed.
- **SYS-REQ-013 [Dynamic Registration]**: The runtime shall support hot-registering drivers without stopping active PLC control tasks.
- **SYS-REQ-016 [Field Device Tag Ingestion]**: Protocol drivers shall ingest data strictly from physical external field devices. Drivers shall publish values using the `external.<namespace>.<device>.<variable>` schema. Drivers shall not embed transport protocol names (such as `modbus` or `profinet`) into tag URIs.
- **SYS-REQ-017 [Plant Namespace Scoping]**: Field drivers shall bind ingested tags to the physical plant location or machine area. Valid namespaces include process areas such as `BoilerRoom`, `Line1`, or `Sterilizer01`.
- **SYS-REQ-018 [Instrument Tag Mapping]**: Field drivers shall identify physical instruments using ISA-5.1 standards. Examples include `PT101` for pressure transmitters and `TT204` for temperature sensors.
- **SYS-REQ-019 [Physical Metric Registration]**: Field drivers shall register measured physical quantities as the variable segment. Permitted variables include `pressure`, `temperature`, and `flow_rate`, with standard subfields (`.pv`, `.raw`, `.fault`).

### 3.3 Native STruC++ Runtime Engine Integration (`crates/mythos-strucpp`)

- **SYS-REQ-020 [Native STruC++ C-ABI Bridge & Symbol Loader]**: MythOS shall interface with STruC++ compiled control logic through a standardized `extern "C"` Foreign Function Interface, consuming C++17 shared object libraries (`.so`) loaded via `libloading` without external PLC daemons.
- **SYS-REQ-021 [STruC++ Pipeline & In-Canvas Logic Deployment]**: The system shall compile Structured Text (`.st`) code into native `.so` dynamic libraries via STruC++ (`strucpp compile`) directly on the target controller. The IEC 61131-3 logic editor (Monaco ST editor and Ladder diagram canvas) shall be 100% integrated inside the MythOS web workbench. The platform shall eliminate manual file upload forms. Deploying logic shall execute as a single atomic user action ([Deploy to Controller] dispatching `POST /api/v1/control/deploy`), generating native binaries and hot-reloading without control interruption.
- **SYS-REQ-022 [Native Cyclic Scheduling]**: The Native STruC++ cyclic scan loop shall run on a dedicated real-time Linux thread (`SCHED_FIFO`, priority 80 to 99) with deterministic cycle timing.
- **SYS-REQ-023 [Dynamic Hot-Reload]**: The system shall hot-reload compiled logic binaries using `dlopen()` and `dlclose()`. Hot-reload shall verify checksums and pause execution at scan boundaries. It shall transfer retain buffers and resume execution within 50 milliseconds.
- **SYS-REQ-024 [Tag Synchronization]**: At scan start, the engine transfers inputs from TagBus to STruC++ located input memory (`%IX`). The engine shall resolve field inputs using `external.<namespace>.<device>.<variable>`. At scan end, the engine shall copy outputs (`%QX`) directly back to the TagBus.
- **SYS-REQ-025 [Internal PLC Variable Isolation]**: Internal PLC program variables and timers shall NOT use the `external` root prefix. Internal variables shall use the domain prefix `internal.<namespace>.<program>.<variable>`. The runtime shall isolate internal variables from fieldbus driver overwrites.

### 3.4 Data Tagging, Addressing & Retention (`docs/02-data-and-addressing/`)

- **SYS-REQ-030 [Canonical Addressing & Plant Hierarchy]**: The system shall enforce the universal semantic tag naming scheme:
  `external.<namespace>.<device>.<variable>[.<subfield>]`
  Where:
  - `external`: Root prefix strictly reserved for external field instruments and devices.
  - `<namespace>`: Physical plant installation location, process area, or machine group (such as `BoilerRoom`, `Line1`). Protocol names shall not be used as namespaces.
  - `<device>`: Physical instrument or machine tag (such as `PT101`, `Supmea_P200`).
  - `<variable>`: Measured physical metric or command register (such as `pressure`, `temperature`).
  - `<subfield>`: Optional UDT member or operational qualifier (such as `.pv`, `.sp`, `.raw`, `.fault`).
  Internal PLC variables and local calculations do not use `external` and shall use the `internal.<namespace>.<program>.<variable>` hierarchy.
  Gold standard canonical URI: `external.BoilerRoom.PT101.pressure`.
- **SYS-REQ-031 [Access Mode Matrix]**: Every tag shall support one of three access modes:
  - `ReadOnly` (`RO`): Driver or PLC writes data; UI and AI clients only read.
  - `WriteOnly` (`WO`): UI and AI clients dispatch write pulses; field device consumes.
  - `ReadWrite` (`RW`): Bidirectional synchronization utilizing shadow command buffers and dirty-flag write triggers.
- **SYS-REQ-032 [UDT Support]**: The platform shall support nested User-Defined Types (UDTs) with deterministic byte alignment and C-compatible memory representations.
- **SYS-REQ-033 [Retention Classes]**: The platform shall support three storage retention tiers:
  - `VOLATILE`: Reinitialized to default values upon process start.
  - `RETAIN`: Preserved across warm system restarts in shared memory.
  - `PERSISTENT`: Backed by an append-only Write-Ahead Log (WAL) written to `/var/lib/mythos/nvram.dat` with CRC32 integrity verification.
- **SYS-REQ-034 [Target Device Primary Storage & Workstation Synchronization]**: The target industrial controller (Raspberry Pi Compute Module 4 or IPC inside the cabinet) shall serve as the authoritative Primary Source of Truth. The controller shall store project files locally under `/var/lib/mythos/projects/<project_name>/`. The controller shall boot independently and execute control logic 24/7 without requiring an active engineering workstation connection. Engineering workstations running MythOS locally shall support offline development, bidirectional project synchronization (`Push to Controller` / `Pull from Controller`), and single-click `.mythos-pkg` archive export.

### 3.5 Network Discovery & Hardware Catalog (`docs/03-network-and-catalog/`, `specs/adr/ADR-004`)

- **SYS-REQ-040 [PROFINET DCP Discovery]**: The discovery daemon shall transmit IEEE 802.3 broadcast frames (EtherType `0x8892`).
- **SYS-REQ-041 [Subnet-Agnostic Operation]**: DCP discovery shall identify industrial devices without assigned IP addresses or on disparate IP subnets.
- **SYS-REQ-042 [DCP Commissioning Primitives]**: The system shall support:
  - `DCP-Identify`: Enumerates device Name, MAC, current IP, Subnet Mask, and Gateway.
  - `DCP-Set-IP`: Assigns IP address, subnet mask, and default gateway over Layer 2.
  - `DCP-Signal-LED`: Causes the target device physical LED to flash for optical field identification.
- **SYS-REQ-043 [mDNS Fallback]**: Over routed Layer 3 segments, the discovery engine queries standard mDNS (`_mythos._tcp.local.`) records.
- **SYS-REQ-044 [Open Hardware Catalog Schema]**: Field devices shall be defined using versioned JSON/YAML manifests conforming to `device-package-schema.md`.
- **SYS-REQ-045 [Catalog Validation]**: Manifests shall validate electrical ratings, register codes, word endianness, engineering ranges, and SVG icon vector standards.
- **SYS-REQ-074 [Universal Protocol & Device Auto-Discovery Engine]**: The system shall implement an automated discovery engine supporting two scanning modes:
  - `Guided Discovery`: The engine executes catalog-assisted fingerprinting using known device profile signatures.
  - `Blind Bus Sweep`: The engine sweeps communication parameters across unknown buses using progressive probing.
  - The discovery engine shall support multi-protocol identification:
    - `Modbus RTU`: Serial baud rate, parity, and slave address heuristic scanning.
    - `Modbus TCP`: Subnet socket sweep with unit identifier verification.
    - `PROFINET`: Layer 2 DCP identify broadcasts.
    - `EtherCAT`: EtherCAT Slave Controller (ESC) ring topology traversal.
    - `CANopen`: NMT heartbeat and node guard frame monitoring.
    - `MQTT Sparkplug B`: Subscribed `NBIRTH` and `DBIRTH` state topic capture.
  - The engine shall enforce deterministic execution timing bounds:
    - Single fast probe timeout: $\le 1.0\,\text{s}$.
    - Guided catalog sweep timeout: $\le 4.0\,\text{s}$.
    - Blind bus sweep operations shall accept an asynchronous cancellation token for immediate termination.

### 3.6 Human-Machine Interface & Visual Systems (`docs/04-hmi-and-ui/`)

- **SYS-REQ-050 [Dual-Theme W3C DTCG Tokens]**: The web interface shall implement W3C DTCG design tokens supporting dual visual themes: Dark Obsidian (`dark`, base `#0A0D12`) for control rooms, and Engineering Clean Slate (`light`, paper `#F8FAFC`, surface `#FFFFFF`) for daylight tablet readability.
- **SYS-REQ-051 [Semantic Alarm Color Invariant]**: The UI shall reserve bright saturated colors exclusively for anomalous process states across both themes:
  - Critical Alarm: `#DC2626` (Red).
  - Major Alarm: `#EA580C` (Dark) / `#D97706` (Light).
  - Minor Alarm: `#CA8A04` (Yellow).
  - Normal / Running: `#475569` (Dark) / `#64748B` (Light) (Muted Slate / Gray).
- **SYS-REQ-052 [Network Topology Canvas]**: The platform shall provide an interactive xyflow canvas displaying PLC racks, remote I/O islands, inverters, and field sensors with A* orthogonal cable routing.
- **SYS-REQ-053 [Color-Coded Bus Routing]**: Topology connections shall render color-coded cable vectors:
  - Modbus TCP / Industrial Ethernet: Green (`#16A34A`).
  - RS485 Serial (Modbus RTU): Blue (`#0284C7`) / Yellow (`#CA8A04`).
  - Safety Loop / Dual-Channel STO: Crimson Red (`#DC2626`).
- **SYS-REQ-054 [4-Mode Modular Faceplate Presentation & Screen Roles]**: Every industrial device and Function Block shall decouple its visual representations across distinct operational screens:
  - `P&ID Mode`: Hosted on dedicated **Engineering Screens** displaying full process schematics, piping flow vectors, and ISA-101 monochrome symbols on a bounded grid.
  - `Interactive Mode`: Hosted on dedicated **Operator Screens** presenting tactile mimic control decks, large Start/Stop pushbuttons, auto/manual toggles, and direct setpoint sliders.
  - `Popup Mode`: Contextual **Diagnostic Modal Windows** triggered by clicking a P&ID symbol or Interactive card setting gear for deep PID loop tuning and multi-monitor setups.
  - `Sidebar Mode`: Modular **iOS-Style Widget Deck** docked at a constant 384px width, hosting standard Size S (96px), Size M (192px), and Size L (384px) stackable, reorderable companion cards with smart-stack carousel support.
- **SYS-REQ-055 [Zero Manual Wire Binding]**: Dragging a PLC block or device onto the canvas binds Faceplate parameters automatically to corresponding tags (`external.<namespace>.<device>.*`).
- **SYS-REQ-056 [Standardized Fixed-Width Sidebar & iOS-Style Modular Widget Grid]**: The UI shall provide a docked slide-over Inspector Drawer constrained to a fixed constant width of `384px` across all plant screens. The sidebar interior shall be partitioned into a 2-column modular grid supporting standardized iOS-style widget form factors: `1/2-Width × 1-Height` (184px × 96px side-by-side tiles), `1-Width × 1-Height` (384px × 96px horizontal strips), `1/2-Width × 2-Height` (184px × 192px vertical bars), `1-Width × 2-Height` (384px × 192px actuator decks), and `1-Width × 4-Height` (384px × 384px full square interlock matrices), featuring smart-stack carousel support and drag-and-drop customization.
- **SYS-REQ-057 [Server-Driven UI Types]**: The system generates frontend TypeScript types from Rust structs using `ts-rs` during build time (`specs/adr/ADR-006`).
- **SYS-REQ-058 [Bounded Sub-Canvas Nesting & Multi-Format Asset Importer]**: The UI shall allow authoring bounded sub-canvases with native import support for vector SVGs, photographic JPEGs, and state-gated animated GIFs/WebP. Saved sub-canvases can be saved as named assets and dragged-and-dropped into any parent screen.
- **SYS-REQ-059 [Constant Grid Matrix Resolution Independence]**: The display canvas shall hold the number of logical grid cells constant (default: 120 columns × 67.5 rows @ 16px). Cell physical pixel dimensions shall scale adaptively to target display viewports, ensuring identical geometric composition and zero layout distortion across 4K video walls, laptops, tablets, and mobile devices.

### 3.7 Industrial Process Engines (`docs/05-enterprise-governance/`)

- **SYS-REQ-060 [Alarm Engine]**: The alarm actor shall implement the ANSI/ISA-18.2 alarm finite state machine.
- **SYS-REQ-061 [First-Out Latching]**: During trip events, the alarm engine latches root-cause faults using 64-bit nanosecond timestamps (`CLOCK_MONOTONIC_RAW`).
- **SYS-REQ-062 [Deadband & Alarm Hysteresis]**: Analog alarms shall enforce configurable deadbands ($\Delta_{\text{db}}$) and on/off-delay filter timers to eliminate chatter.
- **SYS-REQ-063 [ISA-88 Batch Recipe Engine]**: The recipe engine shall separate Formula (setpoints and parameters) from Procedure (PackML sequential phases).
- **SYS-REQ-064 [4-Phase Recipe Handshake]**: Recipe setpoint transfer to the Native STruC++ Engine executes through a 4-phase handshake over `external.Recipe.*`. Verification uses SHA-256 parameter digests.
- **SYS-REQ-065 [Columnar Historian Engine]**: Real-time telemetry shall pass through Boxcar-Swinging Door Compression (SDC). The engine writes daily Apache Parquet columnar files partitioned by date.
- **SYS-REQ-066 [CSV Exporter]**: The system shall expose an RFC 4180 streaming API supporting multiple resampled data streams.
- **SYS-REQ-067 [IEC 62443 RBAC & Audit Trail]**: Five hierarchical roles (`Guest`, `Operator`, `Supervisor`, `Engineer`, `Admin`) shall govern system operations. All setpoint mutations and alarm acknowledgments shall write to an append-only, SHA-256 Merkle-chained JSONL audit ledger.
- **SYS-REQ-068 [1-Click Disaster Recovery]**: The platform shall generate and ingest single-file encrypted archives (`.mythos-pkg`) using ChaCha20-Poly1305. A complete cold-replacement IPC shall restore and resume normal control within 25 seconds.

### 3.8 Model Context Protocol Subsystem (`docs/01-architecture/04-mcp-subsystem-spec.md`, `specs/adr/ADR-005`)

- **SYS-REQ-070 [Native MCP Server]**: MythOS shall embed a Model Context Protocol server supporting `stdio` and Server-Sent Events (`SSE`) transports.
- **SYS-REQ-071 [Macro Self-Registration]**: Subsystem actors shall declare AI agent tools using the `#[mythos_mcp_tool]` attribute procedural macro. Tools shall self-register during binary startup.
- **SYS-REQ-072 [Full Surface Agent Control]**: The MCP tool catalog shall expose operations for:
  - Reading and writing tag values (`mythos_read_tag`, `mythos_write_tag`).
  - Acknowledging and shelving alarms (`mythos_ack_alarm`, `mythos_shelve_alarm`).
  - Deploying and compiling IEC 61131-3 logic programs (`mythos_compile_logic`, `mythos_hot_reload_logic`).
  - Executing network discovery scans (`mythos_discover_devices`).
  - Activating batch recipes (`mythos_load_recipe`, `mythos_start_batch`).
- **SYS-REQ-073 [Cryptographic AI Agent Authentication]**: AI agent connections shall authenticate using Ed25519 cryptographic tokens. Tokens shall restrict privileges through explicit capability scopes. Direct physical actuator toggling by AI agents shall be rejected without operator authorization.

### 3.9 Telemetry, Project Staging & Collaborative IDE (`docs/01-architecture/05-http-sse-and-websocket-api.md`, `docs/02-data-and-addressing/project-file-and-directory-structure.md`, `docs/04-hmi-and-ui/00-frontend-application-architecture.md`)

- **SYS-REQ-075 [100ms Telemetry Throttle]**: The SSE stream (`GET /api/v1/telemetry/stream`) shall throttle tag updates to a standard 100-millisecond (10 Hz) interval. The microkernel batches process value changes during each 100ms window. This batching eliminates browser DOM thrashing while maintaining responsive visual instrumentation.
- **SYS-REQ-076 [Multi-Project Hot-Lifecycle]**: The target controller storage subsystem shall maintain multiple project trees concurrently under `/var/lib/mythos/projects/`. The runtime shall support opening, editing, syntax-checking, and validating draft projects in memory without interrupting active PLC scan loops. Switching the active project executes an atomic symlink swap (`/var/lib/mythos/projects/active`) and zero-downtime hot-reload without restarting the host process.
- **SYS-REQ-077 [Debounced Atomic Autosave]**: The engineering workbench and controller storage daemon shall implement debounced autosaving with an idle timeout of 1,000 milliseconds. Every autosave mutation commits append-only changes immediately to `project.wal`. The background storage actor then writes a temporary file and atomically replaces project artifacts via POSIX `rename`, guaranteeing zero data loss and crash resilience across power interruptions.
- **SYS-REQ-078 [Multi-User Real-Time Collaborative IDE via CRDT]**: The engineering IDE and canvas subsystem shall support concurrent multi-engineer collaboration across local area networks using Conflict-Free Replicated Data Types (CRDT). The backend utilizes `yrs` (Rust Yjs port) over WebSocket (`/api/v1/collab/ws`). The frontend integrates `yjs`, `y-monaco`, and `y-websocket` to deliver real-time multi-cursor position indicators, peer awareness presence avatars, and conflict-free concurrent editing of logic code and canvas topologies.
- **SYS-REQ-079 [Edit View vs Runtime View State Decoupling]**: The platform shall enforce strict two-phase separation between draft editing (Edit View) and active production execution (Runtime View) across HMI screens (Engineering & Operator) and soft-logic code. Modifications to canvas objects, piping vectors, faceplate cards, or logic code shall reside exclusively in an isolated draft staging buffer (`project.draft.json`). Modifications shall not mutate the live running plant until an explicit user commit action is executed.
- **SYS-REQ-080 [Task Scheduler Staging Isolation]**: Cyclic task configurations (scan intervals, execution priorities, CPU core affinities) within the Native STruC++ Engine shall adhere to Edit vs Runtime staging. Modifying a task schedule in Edit View shall update only draft project configurations. The live real-time `SCHED_FIFO` task thread continues executing existing cycle bounds until an atomic project activation is committed.
- **SYS-REQ-081 [Visual Mode Indicator & Promotion Handshake]**: The user interface shall display persistent status badges indicating operational mode (`[✏️ EDIT MODE — UNCOMMITTED]` versus `[🟢 RUNTIME MODE — LIVE]`). Committing shall promote draft files (`project.draft.json`) to active production (`project.active.json`), recompile Structured Text via STruC++, and trigger an atomic hot-reload in $< 50\,\text{ms}$ at scan dead-time.
- **SYS-REQ-082 [TIA Portal-Style Screen & Sub-Screen Hierarchy]**: The engineering workbench shall organize HMI screens in a hierarchical tree inside the primary navigation panel (Zone 2). The tree supports adding, renaming, and deleting top-level Screens (`Screen_01_PlantOverview`) and child Sub-Screens or Popup Windows (`SubScreen_FeedPumpSkid`). Double-clicking any Screen or Sub-Screen node opens its dedicated canvas tab in the multi-zone workspace (Zone 3) for independent graphic authoring.
- **SYS-REQ-083 [3-Stage HMI Lifecycle: Edit, Preview, Deploy]**: HMI displays shall adhere to a strict three-stage operational lifecycle:
  - `Edit View`: Drafting canvas with grid snapping, asset placement, and tag binding saved to `screens/draft/<screen_id>.json`.
  - `Preview Mode`: Non-actuating staging simulation with a blue banner (`[👁️ PREVIEW SIMULATION — NON-ACTUATING]`). Allows testing navigation, screen transitions, and popup behaviors without writing to physical actuators.
  - `Deploy to Runtime HMI`: Production release promoting draft screens to `screens/active/`. The backend broadcasts an SSE event (`HMI_LAYOUT_UPDATED`) to all active plant terminals and operator screens, executing an atomic hot-swap without browser reloads or PLC interruptions.
- **SYS-REQ-084 [Faceplate Library Editor & Global Propagation]**: The platform shall host a dedicated Faceplate Library in the Zone 2 Project Tree (`faceplates/library/`) for creating and managing master reusable component types (such as `FP_Motor_VFD`, `FP_Pneumatic_Valve`, and `FP_Analog_Transmitter`). Double-clicking a Faceplate Type opens the dedicated Faceplate Master Designer in Zone 3. The engineer designs graphic elements bound to formal UDT properties (`UDT.bRunning`, `UDT.bFault`, `UDT.rSpeedActual`) across three tabbed personas: P&ID Symbol, Interactive Card, and Diagnostic Popup. Dragging a Faceplate Type onto any plant screen binds it to a concrete PLC FB/UDT instance with zero manual wiring. Updating and releasing a new version of a master Faceplate Type in the Library shall atomically propagate visual and behavioral changes to all instantiated instances across all plant screens.
- **SYS-REQ-085 [Immersive Fullscreen Runtime Mode & Secure Kiosk Exit]**: Activating production runtime via `[▶️ Run / Enter Runtime]` shall expand the active HMI canvas to immersive fullscreen (`document.documentElement.requestFullscreen()`). The runtime shell shall hide all engineering IDE chrome, including the Zone 1 Menu Bar, 48px Activity Bar, Zone 2 Project Tree, and Zone 5 Console. The operator view shall render exclusively the plant process canvas, the global ISA-18.2 alarm banner, and the contextual Inspector Drawer upon interaction. Exiting runtime mode to the engineering IDE shall enforce an IEC 62443 credential challenge requiring `Engineer` or `Admin` privileges. Upon authorized exit confirmation, the system exits fullscreen (`document.exitFullscreen()`) and restores the 5-zone engineering workspace chrome.

### 3.10 Universal Plugin Harness & Extensibility Subsystem (`docs/01-architecture/06-universal-plugin-harness-and-extensibility.md`, `specs/adr/ADR-008`)

- **SYS-REQ-086 [Universal Plugin SPI & Lifecycle Engine]**: Every functional subsystem shall implement the standardized `MythosPlugin` trait. The trait shall mandate lifecycle hooks: `init`, `start`, `tick`, `stop`, and `health_check`. The core runtime manages plugin lifecycles deterministically. The microkernel contains zero domain-specific logic. All first-party features operate as built-in plugins under this unified interface:
  - `plugin-strucpp`: Native STruC++ IEC 61131-3 soft-logic runtime execution engine.
  - `plugin-alarms`: ANSI/ISA-18.2 alarm state machine and flood suppression.
  - `plugin-recipes`: ANSI/ISA-88 batch recipe control engine.
  - `plugin-historian`: Boxcar-Swinging Door telemetry compression and Parquet archiver.
  - `plugin-discovery-dcp`: PROFINET Layer 2 DCP Ethernet broadcast scanner.
  - `driver-modbus`: Modbus RTU and TCP communication driver.
  - `driver-mqtt`: MQTT Sparkplug B communication driver.
- **SYS-REQ-087 [Plugin Sandboxing & Capability-Based Security]**: Every plugin shall declare explicit permission scopes in its `plugin.yaml` manifest. The microkernel enforces a capability security proxy. The proxy prevents unauthorized actuator writes, raw disk access, and network egress. Sandboxed plugins execute within isolated memory boundaries. A crash in any plugin shall not terminate the core host daemon.
- **SYS-REQ-088 [Plugin Packaging & Dynamic Hot-Reload]**: The platform shall distribute plugins as single-file archives (`.mythos-plugin`). The package contains compiled binaries, frontend assets, configuration schemas, and cryptographic signatures. The plugin manager installs, activates, and updates plugins dynamically. Dynamic updates execute without restarting active soft-logic scan threads or interrupting TagBus operations.
- **SYS-REQ-089 [Automatic MCP AI Tool Contribution by Plugins]**: Installed plugins contribute domain tools into the native Model Context Protocol catalog automatically. The host discovers tool annotations and registers tools, resources, and prompt templates. AI agents inspect plugin health, trigger calibrations, and query state through registered tools.
- **SYS-REQ-090 [Modular Frontend GUI Plugin Shell]**: The web frontend shall operate as a modular extension shell. All screens, workspace panels, and inspector drawers shall execute as pluggable UI extensions. First-party modules and community packages implement identical UI extension slots:
  - `ui-plugin-network-topology`: Interactive xyflow node-graph canvas.
  - `ui-plugin-logic-editor`: Monaco Structured Text and Ladder logic canvas.
  - `ui-plugin-hmi-canvas`: ISA-101 high-performance SCADA process canvas.
  - `ui-plugin-faceplate-library`: Faceplate Master Type designer.
  - `ui-plugin-inspector`: Fixed-width slide-over drawer hosting contextual modular widgets.
  - `ui-plugin-alarm-banner`: Persistent ISA-18.2 top and bottom alarm banner.

### 3.11 Platform Abstraction Layer, Container Parity & Universal Distribution (`docs/01-architecture/07-cross-platform-hal-and-containerization.md`, `docs/01-architecture/08-one-click-installers-and-distribution.md`)

- **SYS-REQ-092 [Platform Abstraction Layer & OS Agnosticism]**: The microkernel shall encapsulate host operating system dependencies within the Platform Abstraction Layer (`mythos-pal`). Compile-time conditional compilation (`#[cfg(target_os)]`) shall map shared memory, thread priority, thread affinity, and dynamic library loading across Linux, Windows 11, macOS, and Docker containers. Shared memory shall bridge POSIX SHM (`/dev/shm`), Windows Named File Mapping (`Local\MythosTagBus`), and macOS Mach memory mappings. Real-time task scheduling shall map Linux `SCHED_FIFO`, Windows `THREAD_PRIORITY_TIME_CRITICAL`, and macOS `THREAD_TIME_CONSTRAINT_POLICY` under a unified real-time thread trait.
- **SYS-REQ-093 [Official Multi-Architecture Container Images]**: The platform shall publish official multi-stage Docker container images supporting `linux/amd64` and `linux/arm64` architectures. Container deployments shall support host IPC (`--ipc=host`) and host networking (`--net=host`) modes. Container runtimes shall mount host system time and hardware clock devices to maintain deterministic real-time execution.
- **SYS-REQ-094 [One-Click Native Desktop Installers]**: The platform shall distribute official one-click native desktop installers for Windows 10/11 (`MythOS-Setup-x64.exe`), macOS Universal (`MythOS-Universal.dmg`), and Linux (`MythOS-x86_64.AppImage`, `.deb`, `.rpm`). The Windows installer shall bundle standalone Clang compiler binaries and runtime service components with zero manual prerequisites. The macOS installer shall bundle Universal 2 binaries with native Cocoa WebViews and Apple notarization signatures. Linux AppImages shall execute directly without external package dependencies.
- **SYS-REQ-095 [Automated Universal Bootstrap & Multi-Channel CLI Distribution]**: The platform shall provide automated universal bootstrap scripts and multi-channel lifecycle management hosted at `https://get.mythos.firmansyah.co` and release feed `https://releases.mythos.firmansyah.co`.
  - **Linux & macOS Universal Bootstrap**: Single-command bash installer (`curl -fsSL https://get.mythos.firmansyah.co | bash`) supporting CLI flags `--channel <release|rc|dev>` and `--version <semver>` (or environment variables `CHANNEL=<release|rc|dev>` and `VERSION=<semver>`). The bootstrap installer shall automatically detect operating system and CPU architecture (`x86_64`, `aarch64`, `armv7l`), verify cryptographic SHA-256 checksums, install binary payloads to user/system paths, and configure operational directories (`/var/lib/mythos/` or `~/.mythos/`).
  - **Windows PowerShell Bootstrap**: Single-command PowerShell installer (`irm https://get.mythos.firmansyah.co/win | iex`) supporting parameters `-Channel <release|rc|dev>` and `-Version <semver>`.
  - **All-in-One Packaging & Embedded Web HMI GUI**: All distribution binaries shall bundle the complete embedded Web HMI GUI served on `http://localhost:8080` directly from the daemon microkernel. The bootstrap installer shall automatically register native desktop application launchers across target platforms (macOS `/Applications/MythOS.app`, Windows Start Menu and Desktop shortcuts, and Linux freedesktop `.desktop` entries). For headless controller environments, the installer shall register an active systemd service configured with real-time capability privileges (`CAP_SYS_NICE`, `CAP_NET_RAW`).
  - **CLI Lifecycle Management Subcommands (`crates/mythos-cli`)**: The `mythos` command-line utility shall provide first-class release channel, version switching, and recovery commands:
    - `mythos channel [switch <channel>]`: View the active channel or switch between `release` (stable), `rc` (release candidate), and `dev` (nightly development) channels.
    - `mythos versions --remote`: Query and list available release versions across official channels from `https://releases.mythos.firmansyah.co`.
    - `mythos update [--version <semver>]`: Update the local installation to the latest version in the active channel or pin to an explicit SemVer version.
    - `mythos rollback`: Immediately revert to the previous cached binary version with deterministic rollback time $< 1.0\,\text{s}$ without redownloading.

### 3.12 Industrial Safety Invariants & Explicit Device Simulation Control (`docs/01-architecture/02-protocol-driver-spi.md`, `docs/02-data-and-addressing/access-mode-matrix.md`, `docs/04-hmi-and-ui/inspector-drawer-layout.md`)

- **SYS-REQ-096 [Explicit Per-Device Simulation Mode & Zero-Silent-Fallback Invariant]**: The platform shall strictly enforce explicit per-device execution modes (`DeviceExecutionMode::PhysicalHardware` and `DeviceExecutionMode::ManualSimulation`).
  - **Zero Silent Fallback (Production Safety Invariant)**: In `PhysicalHardware` mode (default production setting), drivers shall bind strictly to physical communication interfaces. If a physical interface is missing, an RS-485 cable is severed, an Ethernet connection drops, or a communication timeout occurs, the driver shall immediately assert a hard `BAD_COMM_FAILURE` alarm (`0x0008` / `0x0018`). Under zero circumstances shall the driver or supervisor silently or automatically fall back to simulation mode or generate synthetic values.
  - **Explicit Per-Device Mode Toggle**: Each external field device shall expose an independent, manual operational mode switch within its configuration inspector:
    - `[ 🔌 Physical Hardware ]` (Production Default)
    - `[ 🧪 Simulation / Manual Test ]` (Staging & Commissioning Mode)
  - **Simulation Mode Behavioral Guarantees**: When an authorized operator explicitly switches a device to `ManualSimulation`:
    - The platform shall suppress physical interface allocation errors and port-missing warnings.
    - The platform shall unlock the interactive Quick Input Slider and Direct Variable Forcing injection interface in the Tag Inspector.
    - The platform shall stamp high-visibility amber badges `[🧪 SIMULATION ACTIVE]` across the Network View, Tag Explorer, and HMI faceplates.
    - Injected simulation tags shall carry OPC UA quality code `UNCERTAIN_SUBSTITUTED` (`0x0044`) with the simulation bit active.
    - The microkernel shall write an immutable append-only audit trail event recording operator identity, timestamp, target device path, and mode transition (`[OPERATOR: <id>] [ACTION: Enable Simulation Mode] [TARGET: external.<namespace>.<device>]`).

### 3.13 Siemens TIA Portal Workspace & Project Tree Parity (`docs/04-hmi-and-ui/`)

- **SYS-REQ-097 [Siemens TIA Portal Workspace & Project Tree Parity]**: The engineering workbench shall mirror canonical Siemens TIA Portal workspace paradigms:
  - **The 3 Canonical Hardware Engineering Views (Devices & Networks)**:
    - `Network View`: Displays interconnected devices, controllers, and distributed I/O over industrial fieldbuses. Cable vectors show explicit protocol colors (PROFINET `#16A34A` Green, RS-485 Modbus `#0284C7` Blue, Safety Loop / STO `#DC2626` Crimson Red). Supports subnet grouping and IP address configuration.
    - `Device View`: Displays physical rack chassis slot-by-slot (`Rack 0`, `Slot 0` = Power Supply, `Slot 1` = CPU / Soft-Logic Controller, `Slot 2..N` = Distributed I/O slices). Allows terminal pin inspection, channel configuration, and address filtering.
    - `Topology View`: Displays port-to-port physical cabling (`Port 1 [P1]` on Controller to `Port 2 [P2]` on Switch). Enables deterministic Layer 2 DCP device recognition and cable discrepancy detection without manual IP setup.
  - **Zone 2 Canonical Project Tree Taxonomy**:
    The primary navigation tree shall enforce the standardized hierarchical taxonomy:
    ```
    ▼ Project: [Project_Name]
      ▼ Devices & Networks
        ├── Network View (Bus overview & subnet mapping)
        ├── Device View (Rack & slot chassis configuration)
        └── Topology View (Port-to-port physical cabling)
      ▼ Controller [MythOS Soft-Logic / STruC++]
        ├── Device configuration & Network interfaces
        ├── Online & Diagnostics (Live CPU cycle time, buffer usage)
        ▼ Program Blocks
          ├── [➕ Add new block: OB / FB / FC / DB]
          ├── Main [OB1] (Cyclic main task)
          └── Function Blocks & Data Blocks
        ▼ PLC Tags
          ├── Show all tags
          └── Tag Tables (external.* vs internal.*)
        ▼ PLC Data Types (UDTs)
        └── Watch and Force Tables
      ▼ Visualization [MythOS Web HMI / WinCC Kiosk]
        ▼ HMI Screens
          ├── [➕ Add new screen]
          ├── Screen_01_PlantOverview
          └── Screen_02_BoilerArea (with nested Sub-Screens)
        ▼ Screen Management (Templates, Pop-up screens, Slide-in drawers)
        ▼ Faceplate Types (Project Library Master Templates)
        ├── HMI Tags & Alarm Management (ISA-18.2)
        └── Recipes (ISA-88)
    ```
  - **Zone 4 Far-Right Collapsible Task Cards**:
    The right dock shall provide collapsible vertical task cards for rapid asset selection:
    - `Instructions`: Standard IEC 61131-3 logic operations (Bit logic, Timers, Counters, Math, Comparators, Move/Convert).
    - `Hardware Catalog`: Searchable hierarchical tree of OEM modules, field sensors, motor drives, and I/O racks.
    - `Libraries`: Split view hosting Project Library (Master Faceplate Types, Project UDTs) and Global Vendor Libraries.
    - `Layout & Widgets`: Modular iOS-style 2-column widgets, ISA-101 process symbols, P&ID assets, and vector shapes.
  - **Zone 5 Context-Sensitive Bottom Properties & Inspector**:
    Selecting any block, device node, rack slot, or cable vector in Zone 3 shall drive the bottom tabbed inspector:
    - `General`: Element identification, author, semantic name, hardware release, and operational comments.
    - `IO Addresses / Variables`: Absolute hardware addresses (`%IX`, `%QX`, `%IW`, `%QW`), tag bindings (`external.*`), and process scaling.
    - `Diagnostics`: Real-time online communication health, CRC packet drops, cycle latency, and firmware status.
    - `Alarms`: Contextual ISA-18.2 alarm status banner, trip thresholds, deadband tolerances, and sequence-of-events history.

---

## 4. Non-Functional & Quality Requirements

### 4.1 Performance & Timing Bounds
- **NFR-001**: TagBus read latency under peak load shall not exceed 100 nanoseconds (target: $< 50\,\text{ns}$).
- **NFR-002**: Real-time Native STruC++ scan jitter shall not exceed $\pm 100\,\mu\text{s}$ when executing on a Linux kernel with `PREEMPT_RT` patches.
- **NFR-003**: Discovery scan over Layer 2 DCP shall return responses from all active physical devices within 2,500 milliseconds.
- **NFR-004**: Swinging Door Compression shall achieve over $90\%$ data reduction on telemetry while preserving peak and inflection accuracy.

### 4.2 Resource Utilization & Footprint
- **NFR-010**: Baseline resident memory (RSS) of the core daemon without historical cache shall not exceed 64 megabytes.
- **NFR-011**: Flash write wear shall not exceed 500 megabytes per day under 1,000-tag acquisition at 100 ms intervals.

### 4.3 Reliability & Availability
- **NFR-020**: Mean Time Between Failures (MTBF) of the core supervision actor loop shall exceed 50,000 continuous hours.
- **NFR-021**: Disaster recovery restoration from a valid `.mythos-pkg` file shall complete in less than 30 seconds on bare-metal hardware.

---

## 5. System Interfaces & Data Exchange

| Interface ID | Interface Name | Protocol / Mechanism | Data Format | Latency Bound |
|---|---|---|---|---|
| **IF-001** | Internal Tag Bus | POSIX SHM (`/dev/shm/mythos_tagbus`) | POD Structs (64-byte aligned), Plant Hierarchy URIs (`external.*` & `internal.*`) | $< 100\,\text{ns}$ |
| **IF-002** | Native STruC++ Engine Bridge | Linux FFI (`extern "C"`) | C-ABI Pointers, Dual-Zone Mapping (%IX field inputs / %QX, %MX internal logic) | $< 10\,\mu\text{s}$ |
| **IF-003** | Fieldbus Discovery | PROFINET Layer 2 DCP | Raw Ethernet Frames (`0x8892`) | $< 2.5\,\text{s}$ |
| **IF-004** | Serial Fieldbus | RS485 / Modbus RTU | Binary Modbus frames mapped to `external.<namespace>.<device>.<variable>` | $10\,\text{ms} - 500\,\text{ms}$ |
| **IF-005** | Web Client UI & Telemetry | HTTP/2, WebSocket & SSE | JSON / Typed Payloads with Plant Namespace Hierarchy, 100ms Telemetry Throttle | $< 100\,\text{ms}$ (stream tick), $< 10\,\text{ms}$ (REST) |
| **IF-006** | AI Agent Surface | Model Context Protocol (MCP) | JSON-RPC 2.0 (stdio / SSE) with Semantic Tag Navigation | $< 100\,\text{ms}$ |
| **IF-007** | Backup Container | Filesystem Archive | AES-256-GCM / ChaCha20 Zip | $< 25\,\text{s}$ |
| **IF-008** | Real-Time Collaborative IDE | WebSocket (`/api/v1/collab/ws`) | Yjs / `yrs` Binary CRDT Updates, Ephemeral Awareness State | $< 20\,\text{ms}$ |
| **IF-009** | Target Storage & Offline Sync | HTTP REST (`/api/v1/projects`) | JSON Project Schemas, Git-Style Push/Pull Diff, `.mythos-pkg` | $< 2\,\text{s}$ |
| **IF-010** | Universal Plugin SPI & Host Interface | In-Process Dynamic Loading / WASM Component | `trait MythosPlugin`, Capability Tokens, `plugin.yaml` | $< 10\,\mu\text{s}$ (native), $< 50\,\mu\text{s}$ (WASM) |

---

## 6. Verification & Acceptance Criteria Matrix

| Requirement ID | Specification Item | Verification Method | Acceptance Pass Criteria |
|---|---|---|---|
| **SYS-REQ-002** | Lock-Free Shared Memory | Benchmark Test | 1,000,000 tag reads complete with average latency $< 50\,\text{ns}$. Zero data corruption. |
| **SYS-REQ-010** | Protocol Driver SPI | Fault Injection Test | Simulated crash in Modbus driver terminates only that driver actor. Core and Native STruC++ Engine remain healthy. |
| **SYS-REQ-016** | Field Device Tag Ingestion | Tag Registry Verification | Confirms `external` prefix applies strictly to external field device telemetry. |
| **SYS-REQ-021** | In-Canvas Logic Deployment | End-to-End API Test | Logic deployed via `POST /api/v1/control/deploy` compiles on-device and reloads without manual file upload. |
| **SYS-REQ-023** | Native STruC++ Hot-Reload | Integration Test | Code reloaded in $< 50\,\text{ms}$ without drop in physical I/O output states. |
| **SYS-REQ-025** | Internal Variable Isolation | Memory Boundary Test | Confirms internal PLC variables and timers execute without the `external` prefix. |
| **SYS-REQ-030** | Canonical Plant Hierarchy | URI Parser Unit Test | Validates `external.<namespace>.<device>.<variable>` against plant topology; rejects protocol names in namespace. |
| **SYS-REQ-034** | Target Device Primary Storage | Autonomous Boot Test | Controller boots and executes project without connected laptop; laptop successfully pulls/pushes project changes over LAN. |
| **SYS-REQ-040** | Layer 2 DCP Discovery | Hardware Simulation | Identifies simulated PLC with unassigned IP ($0.0.0.0$) within $2.0\,\text{s}$. |
| **SYS-REQ-051** | ISA-101 Muted UI Theme | Box-Model Design Audit | Dark Obsidian tokens verified; zero non-alarm red/yellow elements on process displays. |
| **SYS-REQ-061** | ISA-18.2 First-Out Latching | Sequence Timing Test | First fault in 10-alarm cascade flagged with First-Out bit within $1\,\mu\text{s}$ resolution. |
| **SYS-REQ-065** | Parquet SDC Historian | Data Compression Test | Compression ratio exceeds $90\%$ on noisy sinusoidal signal with error within $\pm \epsilon_{\text{sdc}}$. |
| **SYS-REQ-068** | Disaster Recovery Package | Hot Restoration Test | Complete system restoration from `.mythos-pkg` archive completes in $\le 25\,\text{seconds}$. |
| **SYS-REQ-070** | Native MCP Subsystem | End-to-End AI Test | Automated AI agent reads tag, acknowledges alarm, and modifies setpoint through MCP tool calls. |
| **SYS-REQ-074** | Universal Auto-Discovery Engine | Protocol Sweep Benchmark | Guided sweep finishes in $\le 4.0\,\text{s}$; cancellation token halts blind scan within $100\,\text{ms}$. |
| **SYS-REQ-075** | 100ms Telemetry Throttle | Stream Rate Benchmark | Confirms SSE stream tick rate holds strictly at 100ms interval under 1,000 active tag mutations. |
| **SYS-REQ-076** | Multi-Project Hot-Lifecycle | Symlink Swap Test | Atomic switch from Project A to Project B executes with $< 50\,\text{ms}$ dead-time and zero process restarts. |
| **SYS-REQ-077** | Debounced Atomic Autosave | Power Loss Fault Injection | Simulates hard power cut during 1000ms debounce interval; verifies zero file corruption and complete recovery via `project.wal`. |
| **SYS-REQ-078** | Multi-User Collaborative IDE | Multi-Client Concurrency Test | Two concurrent clients edit logic file simultaneously; verifies deterministic CRDT convergence and multi-cursor rendering. |
| **SYS-REQ-079** | Edit vs Runtime View Decoupling | HMI Staging Isolation Test | Adding pump symbol to P&ID in Edit View causes zero visual change or tag subscription mutation in Operator Runtime View until explicit commit. |
| **SYS-REQ-080** | Task Scheduler Staging Isolation | Real-Time Thread Timing Test | Modifying task interval from 10ms to 50ms in Edit View leaves live `SCHED_FIFO` loop executing at 10ms until promotion handshake completes. |
| **SYS-REQ-081** | Visual Mode Indicator & Promotion | Promotion Handshake Test | Status badge transitions from `[✏️ EDIT MODE]` to `[🟢 RUNTIME MODE]`; Native STruC++ Engine hot-reloads within 50ms at scan boundary. |
| **SYS-REQ-082** | TIA Portal Screen Hierarchy | Navigation Tree Test | Double-clicking Screen or Sub-Screen in Zone 2 tree opens dedicated canvas tab in Zone 3 with independent layout state. |
| **SYS-REQ-083** | 3-Stage HMI Lifecycle | Staging & Simulation Test | Edit canvas changes persist to draft; Preview simulates navigation without actuator writes; Deploy broadcasts `HMI_LAYOUT_UPDATED` via SSE. |
| **SYS-REQ-084** | Faceplate Master Propagation | Multi-Instance Cascade Test | Modifying master Faceplate Type in Library propagates visual updates across 20+ instantiated screen symbols atomically. |
| **SYS-REQ-085** | Fullscreen Runtime & Kiosk Exit | Kiosk Boundary & Security Test | Activating runtime expands to fullscreen and hides IDE chrome; unauthorized exit attempts fail; valid admin credentials exit fullscreen safely. |
| **SYS-REQ-086** | Universal Plugin SPI & Lifecycle | Lifecycle State Machine Test | Microkernel initialises, ticks, and stops plugin instances deterministically. Panic in plugin restarts safely in $< 100\,\text{ms}$. |
| **SYS-REQ-087** | Plugin Sandboxing & Security | Capability Security Enforcement | Plugin without `actuator.write` permission fails tag write operation with `SecurityError`. Microkernel memory remains isolated. |
| **SYS-REQ-088** | Plugin Packaging & Hot-Reload | Dynamic Hot-Reload Test | `.mythos-plugin` package installs and upgrades during active execution. TagBus scan loop continues without zeroing outputs. |
| **SYS-REQ-089** | Plugin Dynamic MCP Tool Registration | AI Agent Tool Discovery Test | Installing a driver plugin automatically exposes corresponding diagnostic MCP tools. Agent queries device state successfully. |
| **SYS-REQ-090** | Modular Frontend GUI Plugin Shell | UI Extension Mount Test | Registers custom canvas view and inspector widget into host shell slots. Visual component renders cleanly within design token bounds. |
| **SYS-REQ-092** | Platform Abstraction Layer (PAL) | Cross-OS Verification Test | Executes tag bus, scheduling, and dynamic linking tests across Linux, Windows 11, macOS, and Docker environments. |
| **SYS-REQ-093** | Multi-Architecture Container Images | Container Parity Benchmark | Validates official `linux/amd64` and `linux/arm64` container builds with `--ipc=host` achieving $< 50\,	ext{ns}$ tag access latency. |
| **SYS-REQ-094** | One-Click Native Desktop Installers | Zero-Prerequisite Install Test | Executes automated installation of `.exe`, `.dmg`, and `.AppImage` packages on clean machines with zero manual toolchain setup. |
| **SYS-REQ-095** | Automated Universal Bootstrap & Multi-Channel CLI Distribution | Bootstrap & Channel Lifecycle Test | Validates bash and PowerShell bootstrap one-liners with `--channel` and `--version` flags; verifies embedded Web HMI on `:8080`, desktop launcher creation, systemd registration, `mythos channel switch`, `mythos versions --remote`, `mythos update --version`, and instant `mythos rollback` ($< 1.0\,\text{s}$). |
| **SYS-REQ-096** | Explicit Per-Device Simulation Mode & Zero-Silent-Fallback | Hardware Disconnect Fault Test | Sever RS-485 / Ethernet connection during PhysicalHardware mode; verify immediate `BAD_COMM_FAILURE` alarm generation with zero synthetic fallback. Toggle ManualSimulation; verify slider injection unlocked, amber simulation indicators active, and audit entry recorded. |
| **SYS-REQ-097** | Siemens TIA Portal Workspace & Project Tree Parity | Workspace Interaction & Navigation Test | Validates switching between Network, Device, and Topology views in Zone 3; verifies Zone 2 project tree taxonomy; confirms drag-and-drop from Zone 4 Task Cards (Instructions, Catalog, Libraries, Layout); verifies Zone 5 Context-Sensitive Bottom Inspector updates dynamically on node selection. |

---

## 7. Architectural Decisions Traceability

The requirements defined in this specification are backed by the approved Architectural Decision Records and technical architecture handbooks:
- **ADR-001**: Pure Rust Microkernel Actor Architecture (`specs/adr/ADR-001-rust-microkernel-actors.md`)
- **ADR-002**: External Namespace Tag Bus and Shared Memory Architecture (`specs/adr/ADR-002-external-namespace-tag-bus.md`)
- **ADR-003**: Native STruC++ Runtime Engine Architecture (`specs/adr/ADR-003-openplc-v4-runtime-bridge.md`)
- **ADR-004**: Layer 2 DCP Discovery and Deterministic Fieldbus Topology (`specs/adr/ADR-004-layer2-dcp-discovery-topology.md`)
- **ADR-005**: Native Model Context Protocol (MCP) Self-Registration (`specs/adr/ADR-005-native-mcp-self-registration.md`)
- **ADR-006**: Server-Driven UI (SDUI) and Auto-Generated TypeScript Contract (`specs/adr/ADR-006-server-driven-ui-auto-types.md`)
- **ADR-007**: ISA-18.2 Alarms, ISA-88 Recipes, Embedded Historian, and Disaster Recovery Baseline (`specs/adr/ADR-007-isa18-2-isa88-historian-baseline.md`)
- **ARCH-005**: HTTP, SSE, and WebSocket API Architecture (`docs/01-architecture/05-http-sse-and-websocket-api.md`)
- **DATA-004**: Project File and Directory Structure Specification (`docs/02-data-and-addressing/project-file-and-directory-structure.md`)
- **HMI-000**: Frontend Application Architecture Specification (`docs/04-hmi-and-ui/00-frontend-application-architecture.md`)
- **ADR-008**: Universal Plugin Harness Architecture ("Everything is a Plugin") (`specs/adr/ADR-008-universal-plugin-harness-architecture.md`)
- **ARCH-006**: Universal Plugin Harness and Extensibility Architecture (`docs/01-architecture/06-universal-plugin-harness-and-extensibility.md`)
- **ARCH-007**: Cross-Platform Hardware Abstraction Layer & Containerization (`docs/01-architecture/07-cross-platform-hal-and-containerization.md`)
- **ARCH-008**: One-Click Desktop Installers & Universal Distribution (`docs/01-architecture/08-one-click-installers-and-distribution.md`)