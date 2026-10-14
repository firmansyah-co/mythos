# ADR-001: Pure Rust Microkernel Actor Architecture

- **Status**: ACCEPTED
- **Date**: 2026-10-13
- **Author**: Neo (VP Software & Infrastructure)
- **Deciders**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Vulcan (VP Hardware), Scribe (VP Publication)

---

## 1. Context and Problem Statement

Industrial SCADA and soft-logic runtime platforms must execute concurrently across multiple real-time and non-real-time subsystems:
- High-priority cyclic PLC scan loops (IEC 61131-3 logic).
- Asynchronous industrial network protocol drivers (Modbus, MQTT, PROFINET).
- Lock-free shared memory tag bus distribution.
- High-throughput Server-Driven UI (SDUI) and WebSocket streams.
- Autonomous AI agent tool execution via Model Context Protocol (MCP).

In legacy architectures (such as previous monolithic Python or Node.js implementations), a slow I/O call or an uncaught exception in one protocol driver would block the entire runtime, causing PLC cycle slips and field bus communication dropouts.

MythOS requires an architectural foundation that delivers:
1. Zero cross-subsystem blocking.
2. Complete panic containment (fault isolation).
3. Deterministic memory safety without garbage-collection pauses.
4. Clean asynchronous concurrency for networking combined with preemptive real-time OS threads for control loops.

---

## 2. Decision Drivers

- **Zero Garbage Collection (GC)**: Control loops must never pause for memory compaction.
- **Fault Isolation**: A crashing driver or malformed network frame must not compromise the core kernel.
- **Performance**: Capable of processing $> 100,000$ tag updates per second with $< 100 \mu\text{s}$ latency.
- **Type Safety**: Strong compile-time enforcement of data types, memory alignment, and concurrency boundaries (`Send + Sync`).

---

## 3. Considered Options

1. **Option 1: Monolithic Multi-Threaded C/C++ Runtime**
   - *Pros*: Maximum raw execution speed; direct POSIX API integration.
   - *Cons*: Memory safety vulnerabilities (use-after-free, buffer overflows); high probability of segmentation faults crashing the entire industrial plant; manual concurrency synchronization.
2. **Option 2: Go Microservices with gRPC**
   - *Pros*: Fast concurrency with goroutines; built-in networking.
   - *Cons*: Non-deterministic Garbage Collection pauses (1-10 ms stop-the-world); high memory overhead per service; serialization overhead of gRPC/Protobuf over local sockets.
3. **Option 3: Pure Rust Microkernel Actor Architecture with Tokio and POSIX Real-Time Threads** (Selected)
   - *Pros*: Zero-cost abstractions; compile-time borrow checker guarantees zero data races; memory safety without GC pauses; clean separation between async Tokio tasks (networking/MCP) and dedicated `SCHED_FIFO` OS threads (OpenPLC logic).

---

## 4. Decision Outcome

**Chosen Option**: Option 3 (Pure Rust Microkernel Actor Architecture).

### 4.1 Architecture Details
- The core runtime (`mythos-core`) functions as a lightweight supervisor.
- Communication protocol drivers and external service connectors execute as isolated asynchronous actors communicating via bounded MPSC channels (`flume` / `tokio::sync::mpsc`).
- Real-time PLC logic runs inside a dedicated OS thread configured with Linux `SCHED_FIFO` scheduling policy.
- Inter-actor state telemetry is exchanged through a zero-copy lock-free Shared Memory bus (`mythos-shm`), eliminating serialization overhead.
- All actor tasks are wrapped with `std::panic::catch_unwind`, isolating faults and triggering automated exponential backoff recovery without process termination.

---

## 5. Consequences

### 5.1 Positive Consequences
- **Rock-Solid Stability**: Faults in individual protocol drivers are strictly isolated.
- **Sub-Millisecond Determinism**: PLC scan cycles remain jitter-free regardless of heavy HMI or MCP load.
- **Low Footprint**: Minimal memory footprint ($< 25 \text{ MiB}$ resident set size), suitable for industrial edge devices.

### 5.2 Negative Consequences
- **Strict Borrow Checker Discipline**: Requires strict lifetime and type management across asynchronous boundaries.
- **C/C++ FFI Boundaries**: Integrating OpenPLC requires maintaining explicit `extern "C"` bindings.
