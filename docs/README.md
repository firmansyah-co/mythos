# MythOS Enterprise Documentation Suite

Welcome to the comprehensive technical documentation and system specification repository for **MythOS**, an industrial-grade Supervisory Control and Data Acquisition (SCADA) and soft-logic runtime platform engineered in pure Rust with the embedded Native STruC++ Runtime Engine.

---

## 1. Document Index & Navigation

### 1.1 Master Engineering Specifications (`specs/`)
- **[Master System Specification (`specs/spec.md`)](../specs/spec.md)**: ISO/IEC/IEEE 29148 compliant system specification written in ASD-STE100 technical English. Contains 73 functional requirements, non-functional latency bounds, and acceptance criteria.
- **[Work Breakdown Structure & Roadmap (`specs/tasks.md`)](../specs/tasks.md)**: 6-Sprint engineering delivery roadmap (Sprint 0 through Sprint 5) with concrete deliverables, acceptance gates, and RACI matrix.

### 1.2 Architectural Decision Records (`specs/adr/`)
- **[ADR-001: Pure Rust Microkernel Actor Architecture](../specs/adr/ADR-001-rust-microkernel-actors.md)**: Decoupled Tokio actor supervision replacing monolithic loops.
- **[ADR-002: External Namespace Tag Bus & Shared Memory](../specs/adr/ADR-002-external-namespace-tag-bus.md)**: Lock-free triple-buffered POSIX SHM (`/dev/shm/mythos_tagbus`) supporting 32,768 tags with sub-50ns reads.
- **[ADR-003: Native STruC++ Runtime Engine Architecture](../specs/adr/ADR-003-openplc-v4-runtime-bridge.md)**: In-process C-ABI FFI wrapper, STruC++ compiler pipeline, Linux `SCHED_FIFO` real-time thread scheduling, and dynamic hot-reload.
- **[ADR-004: Layer 2 DCP Discovery and Deterministic Fieldbus Topology](../specs/adr/ADR-004-layer2-dcp-discovery-topology.md)**: Subnet-agnostic PROFINET Layer 2 DCP broadcast frames, LED blink signalization, and RS485 bus timing constraints.
- **[ADR-005: Native Model Context Protocol (MCP) Self-Registration](../specs/adr/ADR-005-native-mcp-self-registration.md)**: Native Rust MCP server with procedural macros for autonomous AI agent inspection and tool execution.
- **[ADR-006: Server-Driven UI (SDUI) & Auto-Generated TypeScript Contract](../specs/adr/ADR-006-server-driven-ui-auto-types.md)**: Pure Dumb Frontend paradigm with build-time `ts-rs` type synchronization.
- **[ADR-007: ISA-18.2 Alarms, ISA-88 Recipes, Historian & Disaster Recovery](../specs/adr/ADR-007-isa18-2-isa88-historian-baseline.md)**: First-Out latching, 4-phase recipe handshakes, Swinging Door Compression in Parquet, and 1-click `.mythos-pkg` restoration.

### 1.3 Technical Architecture Handbooks (`docs/01-architecture/`)
- **[00-Cargo Workspace and Crates Design](01-architecture/00-cargo-workspace-and-crates-design.md)**: Modular crate layout, internal dependencies, trait interfaces, and compilation invariants.
- **[01-Microkernel and Shared Memory Architecture](01-architecture/01-microkernel-and-shm.md)**: Detailed memory layout, atomic pointer swap mechanics, and lock-free concurrency.
- **[02-Protocol Driver Service Provider Interface (SPI)](01-architecture/02-protocol-driver-spi.md)**: Dynamic driver trait contract, configuration schemas, and fault isolation mechanisms.
- **[03-Native STruC++ Runtime Engine & In-Process FFI Bridge](01-architecture/03-openplc-v4-ffi-bridge.md)**: Linux real-time thread priority, memory synchronization cycle, and STruC++ pipeline.
- **[04-Model Context Protocol (MCP) Subsystem](01-architecture/04-mcp-subsystem-spec.md)**: Stdio and SSE transports, procedural macros, and cryptographic security scopes for AI agents.
- **[05-HTTP, SSE, and WebSocket API](01-architecture/05-http-sse-and-websocket-api.md)**: OpenAPI 3.1 endpoints, 100ms SSE telemetry streaming, oscilloscope WebSocket protocol, and in-canvas logic deployment.

### 1.4 Data, Addressing & Memory Retention (`docs/02-data-and-addressing/`)
- **[Tag Namespace Rules](02-data-and-addressing/tag-namespace-rules.md)**: Semantic naming convention `external.<namespace>.<device>.<variable>` and validation rules.
- **[Access Mode Matrix](02-data-and-addressing/access-mode-matrix.md)**: ReadOnly, WriteOnly, and ReadWrite execution models and dirty-flag write synchronizations.
- **[UDT and Memory Retention](02-data-and-addressing/udt-and-memory-retention.md)**: User-Defined Types (UDT) layouts, VOLATILE, RETAIN, and PERSISTENT NVRAM WAL engine.
- **[Project File and Directory Structure](02-data-and-addressing/project-file-and-directory-structure.md)**: Target controller primary storage, multi-project hot-switching, atomic debounced autosave, WAL integrity, and JSON schema.

### 1.5 Network, Discovery & Device Catalog (`docs/03-network-and-catalog/`)
- **[Layer 2 DCP Protocol Specification](03-network-and-catalog/layer2-dcp-protocol-spec.md)**: Raw Ethernet broadcast frames (EtherType `0x8892`), service responses, and IP commissioning.
- **[Device Package Schema](03-network-and-catalog/device-package-schema.md)**: JSON/YAML Open Hardware Catalog schema, Modbus register mappings, and vector icon requirements.
- **Golden Reference Packages**:
  - `catalog/devices/supmea-p300/`: Supmea SUP-P300 Pressure Transmitter.
  - `catalog/devices/sentec-rainfall/`: SenTec Tipping Bucket Rainfall Sensor.

### 1.6 Human-Machine Interface & Visual Systems (`docs/04-hmi-and-ui/`)
- **[00-Frontend Application Architecture](04-hmi-and-ui/00-frontend-application-architecture.md)**: React 19 SPA, Dockview shell, Zustand stores, 100ms SSE telemetry consumer, in-canvas logic editor, and Yjs CRDT real-time collaboration.
- **[Design Tokens (W3C DTCG)](04-hmi-and-ui/design-tokens-dtcg.json)**: Dual-theme W3C DTCG tokens (SCADA Obsidian & Light Clean Slate) and calibrated ISA-101/ISA-18.2 semantic alarm colors.
- **[Network Topology View Specification](04-hmi-and-ui/network-topology-view-spec.md)**: xyflow canvas node types, gestures, and color-coded bus routing.
- **[Faceplate Binding Specification](04-hmi-and-ui/faceplate-binding-spec.md)**: 4-Mode presentation mapping (P&ID, Interactive, Popup, Sidebar), nested sub-canvases, asset importer (SVG/JPEG/GIF), and zero manual wire binding.
- **[Inspector Drawer Layout](04-hmi-and-ui/inspector-drawer-layout.md)**: Modular drag-and-drop inspector architecture with fixed 384px constant width, stackable card widgets (@dnd-kit), bounded grid canvas engine (1920x1080 / 4K), and asset import pipeline.

### 1.7 Industrial Process Engines & Governance (`docs/05-enterprise-governance/`)
- **[ISA-18.2 Alarm Engine Specification](05-enterprise-governance/isa-18-2-alarm-engine.md)**: Finite state machine, First-Out monotonic latching, deadband filters, and flood suppression.
- **[ANSI/ISA-88 Batch Recipe Model](05-enterprise-governance/isa-88-recipe-model.md)**: Formula and Procedure separation, PackML phase execution, and 4-phase memory handshake.
- **[Embedded Historian & Selective CSV Export](05-enterprise-governance/embedded-historian-and-csv-export.md)**: Boxcar-Swinging Door Compression (SDC), daily Apache Parquet files, and RFC 4180 streaming.
- **[IEC 62443 RBAC & Audit Matrix](05-enterprise-governance/rbac-security-matrix.md)**: Human operator roles, AI agent token scopes, and append-only Merkle-chained audit ledger.
- **[Disaster Recovery Package Specification](05-enterprise-governance/disaster-recovery-pkg.md)**: 1-Click encrypted `.mythos-pkg` archive and sub-30 second bare-metal restoration.

---

## 2. Editorial & Quality Standards

All specifications in this repository strictly adhere to:
1. **ASD-STE100 Technical English**: Short sentences under 20 words, active voice, concrete verbs, one fact or instruction per sentence, zero corporate ambiguity.
2. **Deterministic Architecture**: Structs, traits, byte layouts, error hierarchies, and serialization schemas are documented with zero placeholders or vague generalities.
3. **Traceability**: Every system requirement maps directly to an approved Architectural Decision Record (ADR) and a corresponding delivery task in the Work Breakdown Structure.
