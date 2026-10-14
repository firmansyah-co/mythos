# ADR-002: External Namespace Tag Bus and Shared Memory Architecture

- **Status**: ACCEPTED
- **Date**: 2026-10-13
- **Author**: Neo (VP Software & Infrastructure)
- **Deciders**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Vulcan (VP Hardware), Iris (VP Design), Scribe (VP Publication)

---

## 1. Context and Problem Statement

A high-performance SCADA system connects numerous concurrent readers and writers:
- Field drivers writing 10,000 to 50,000 register updates per second.
- OpenPLC execution loops reading inputs and writing outputs every 1 to 10 milliseconds.
- Web clients streaming real-time dashboards via WebSockets / SSE at 30 to 60 FPS.
- Historian and alarm engines evaluating thresholds continuously.

Traditional approaches using database queries or mutex-protected in-memory hash maps suffer from severe lock contention. A reader iterating over 20,000 tags in a mutex locks out field bus drivers, resulting in communication timeouts and dropped frames.

Furthermore, arbitrary variable naming schemes across disparate protocols lead to configuration chaos.

---

## 2. Decision Drivers

- **Zero Lock Contention**: Writers must never wait for slow readers, and readers must never block writers.
- **Cache-Line Efficiency**: Prevent False Sharing across multi-core CPUs.
- **Deterministic Addressing**: Enforce a unified, globally consistent naming schema for all process data.
- **Zero Serialization Overhead**: Data exchanged across local processes must be zero-copy.

---

## 3. Considered Options

1. **Option 1: In-Memory Redis / Key-Value Store over Local Sockets**
   - *Pros*: Simple client libraries; built-in pub/sub.
   - *Cons*: TCP socket context switch overhead; serialization/deserialization penalty; non-deterministic latency spikes ($1-5 \text{ ms}$).
2. **Option 2: Mutex-Protected Global RwLock Hash Map**
   - *Pros*: Native in-process Rust data structures.
   - *Cons*: Reader-writer lock contention; lock starvation under high-frequency writes; cannot be mapped across separate OS processes without shared memory.
3. **Option 3: POSIX Lock-Free Triple-Buffered Shared Memory Bus with Dot-Separated Semantic Hierarchy** (Selected)
   - *Pros*: Sub-microsecond access latency ($< 1 \mu\text{s}$); zero lock contention via atomic pointer swap; cache-line aligned (64-byte) POD structs; accessible across separate processes via `/dev/shm/mythos_tagbus`.

---

## 4. Decision Outcome

**Chosen Option**: Option 3 (Lock-Free Triple-Buffered Shared Memory Bus).

### 4.1 Architecture Details
- **Memory Object**: Backed by POSIX shared memory file `/dev/shm/mythos_tagbus`.
- **Triple-Buffering**: Writer operates on Write Buffer, readers read from Read Buffer, and atomic Compare-And-Swap swaps Clean (Surface) Buffer pointers upon write completion.
- **Data Alignment**: All tag entries (`TagSlot`) are exactly 64 bytes (`#[repr(C, align(64))]`), matching x86_64 / ARMv8 cache line boundaries.
- **Naming Hierarchy & Plant Topology**: Process tags adhere strictly to the physical hierarchy:
  * External field tags: `external.<namespace>.<device>.<variable>[.<subfield>]`
  * Internal PLC logic variables: `<namespace>.<device>.<variable>[.<subfield>]` (omitting the `external` root prefix).
  * Microkernel diagnostic tags: `kernel.<subsystem>.<metric>`.
  * `external`: Root prefix strictly reserved for data originating from or dispatched to external field devices (e.g., Supmea P200 transmitters, Modicon PLCs, VFD motor drives).
  * `<namespace>`: Physical plant installation location, process area, or machine group (e.g., `BoilerRoom`, `Line1`, `Sterilizer01`, `PKS.Station2`). Transport protocol names (such as `modbus` or `profinet`) are decoupled and forbidden in tag URIs.
  * `<device>`: Physical instrument or machine tag name (e.g., `PT101`, `TT204`, `Supmea_P200`, `FeedPump01`).
  * `<variable>`: Measured physical metric or command register (e.g., `pressure`, `temperature`, `flow_rate`, `speed`).
  * Optional `.<subfield>`: UDT/struct member or operational sub-property (e.g., `.pv`, `.sp`, `.raw`, `.fault`).
  * Gold Standard Reference: `external.BoilerRoom.PT101.pressure.pv`.
- **Quality and Timestamps**: Every slot encapsulates an IEC 61850 quality code and a 64-bit nanosecond timestamp.

---

## 5. Consequences

### 5.1 Positive Consequences
- **Deterministic Latency**: Read and write latencies are bounded under $100 \text{ ns}$.
- **Zero Allocation in Loop**: Memory is pre-allocated at startup; zero heap allocations during runtime.
- **Crash Resilience**: If a reader crashes, the shared memory segment and active writers remain unaffected.

### 5.2 Negative Consequences
- **Fixed Tag Capacity**: Shared memory size is configured at startup (e.g., 65,536 tags per segment); dynamic resizing requires coordinated segment reallocation.
- **Unsafe Pointer Casts**: Requires audited unsafe Rust code (`bytemuck::Pod`) for casting raw memory to typed structs.
