# ADR-007: Baseline Architecture for ISA-18.2 Alarms, ISA-88 Recipes, Embedded Historian, and Disaster Recovery

## Status
**ACCEPTED**

## Context
Industrial control systems require high-reliability alarm management, deterministic batch recipe coordination, loss-minimized telemetry archiving, and instantaneous disaster recovery. Previous monolithic architectures (such as LycanEdge) suffered from several failure modes:
1. Alarm floods crashing the user interface due to unconstrained event storms and lack of first-out capture.
2. Incomplete or non-atomic PLC synchronization when updating recipe parameters, introducing severe physical collision or process contamination hazards.
3. High write amplification and flash memory exhaustion caused by dumping raw uncompressed time-series data to SQLite or flat JSON files.
4. Protracted hardware recovery procedures taking multiple hours of manual configuration when an on-site IPC fails.

MythOS is engineered in Pure Rust to establish an enterprise-grade, deterministic microkernel platform integrating OpenPLC Runtime v4. We must define the architectural decisions and standards for process governance, alarms, recipes, historization, and system restoration.

---

## Decision Drivers
- **Safety & Standards Compliance**: Full alignment with ANSI/ISA-18.2-2016, ANSI/ISA-88.01-2010, ISA-TR88.00.02 (PackML), IEC 62443-4-2, and 21 CFR Part 11.
- **Determinism & Real-Time Performance**: Zero memory allocation in inner evaluation loops, bounded execution time, and zero cross-thread blocking between telemetry historian and cyclic control tasks.
- **Storage Durability**: Solid-state storage protection on industrial eMMC/NVMe flash media through dual-stage compression.
- **Operational Resilience**: Rapid mean-time-to-recovery (MTTR $< 30$ seconds) on replacement hardware with automated package verification.
- **AI Agent Security**: Strict privilege separation for AI agent Model Context Protocol (MCP) clients using Ed25519 cryptographic tokens and immutable Merkle audit logging.

---

## Architectural Decisions

### 1. ISA-18.2 Alarm State Engine
- We implement a state machine comprising the canonical states: `Normal`, `UnackAlarm`, `AckAlarm`, `UnackRtn`, `Shelved`, `Suppressed`, and `OutOfService`.
- Hysteresis is enforced through explicit absolute or span-percentage deadbands ($\Delta_{\text{db}}$), and noise rejection via on-delay and off-delay filters.
- First-Out sequences are latched using monotonic 64-bit nanosecond timestamps (`CLOCK_MONOTONIC_RAW`), elevating the initiating fault and tagging subsequent secondary excursions.
- Audible annunciator banks are partitioned into `Critical`, `High`, `Medium`, and `Low` horn groups with deterministic silence and auto-resound timers.

### 2. ISA-88 Recipe Synchronization
- Recipes are decoupled into **Formula** (bounded engineering parameters) and **Procedure** (sequential step transitions mapped to PackML states).
- Synchronization between the SCADA layer and OpenPLC Runtime v4 executes over the lock-free TagBus (`/dev/shm/mythos_tagbus`) using an atomic 4-phase handshake protocol (`external.Recipe.*`).
- Every recipe transfer includes a SHA-256 parameter digest verified by the PLC prior to arming execution. Parameter mutations while a phase is in PackML `Execute` state are strictly rejected.

### 3. Embedded Historian & Compression Strategy
- Raw telemetry from the TagBus passes through a dual-stage real-time compression pipeline:
  1. **Deadband Filter**: Immediate rejection of stationary or noise fluctuations.
  2. **Boxcar-Swinging Door Compression (SDC)**: Bristol algorithm with configurable error tolerance $\epsilon_{\text{sdc}}$, yielding $> 92\%$ compression ratio.
- Data is chunked in memory using Apache Arrow record batches and flushed to disk in daily Hive-partitioned **Apache Parquet** files (`year=YYYY/month=MM/day=DD/`) compressed with **Zstandard Level 3**.
- A zero-copy **Selective CSV Exporter API** provides resampled streams (Raw, Forward Fill, Linear Interpolation, Time-Weighted Average) complying with RFC 4180.

### 4. IEC 62443 RBAC & Audit Trail
- Five human roles (`Guest`, `Operator`, `Supervisor`, `Engineer`, `Admin`) govern access.
- AI Agent MCP tokens are issued via Ed25519 signed JWTs with explicit, restricted scopes (`mcp:telemetry:read`, `mcp:alarm:ack`, `mcp:tag:write:setpoint`). Direct physical actuator toggling by AI agents is forbidden.
- An append-only Merkle-chained JSONL audit ledger records all setpoint modifications, alarm acknowledgments, and recipe sign-offs.

### 5. Disaster Recovery (`.mythos-pkg`)
- Complete IPC system configuration is packaged into an encrypted single-file container (`.mythos-pkg`) using ChaCha20-Poly1305 or AES-256-GCM authenticated encryption.
- The package bundles OpenPLC Structured Text source, pre-compiled `.so` binary, hardware I/O maps, tag catalogs, master recipes, and credentials.
- A hot-restore workflow achieves bare-metal restoration on replacement IPC hardware in $\le 25\,\text{seconds}$, verified by SHA-256 manifest checks and automated TagBus spin-up.

---

## Consequences

### Positive
- **Guaranteed Standards Compliance**: Directly satisfies audit requirements for high-risk industrial facilities (e.g., palm oil mills, chemical reactors, steam boilers).
- **Extreme Flash Longevity**: SDC compression combined with Parquet columnar encoding reduces flash write cycles by $> 90\%$, dramatically extending IPC drive lifespan.
- **Microsecond Diagnostics**: First-Out capture pinpoints the exact physical root cause during multi-boiler or turbine cascade trips.
- **Fail-Safe Restoration**: Technicians can replace a destroyed IPC in the field within minutes without specialist programming tools.

### Negative / Trade-Offs
- **Lossy Telemetry Bounds**: Swinging Door Compression is lossy; downstream analytics must account for linear interpolation between archived inflection points.
- **Synchronization Latency**: The 4-phase recipe handshake introduces a $10\,\text{ms} - 50\,\text{ms}$ parameter verification delay before batch execution starts. This is a deliberate design trade-off prioritizing safety over instantaneous command response.
- **Key Management Overhead**: Hot restoration requires access to facility decryption keys, demanding secure on-site custody of USB recovery tokens.

---

## Compliance & Verification
- Unit and integration tests in pure Rust validate the ISA-18.2 transition table against every permutation of inputs.
- Numerical simulation validates SDC reconstruction error bounds ($\le \epsilon_{\text{sdc}}$) across 100,000 synthetic test points.
- Audit trail integrity checker validates Merkle hash linkage under fuzz testing and detects corrupted records instantaneously.
