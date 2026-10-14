# ADR-005: Native Model Context Protocol (MCP) Server and Self-Registration

- **Status**: ACCEPTED
- **Date**: 2026-10-13
- **Author**: Neo (VP Software & Infrastructure)
- **Deciders**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Echo (VP Marketing), Scribe (VP Publication), Sage (VP R&D)

---

## 1. Context and Problem Statement

Modern industrial automation increasingly benefits from autonomous AI agents performing root-cause failure analysis, predictive maintenance optimization, energy efficiency tuning, and automated commissioning.

Existing SCADA systems either lack AI interfaces entirely or expose clumsy, unauthenticated REST/GraphQL endpoints that require AI agents to make multiple conversational hops, parse unstructured HTML/JSON, and guess schema parameters. Furthermore, allowing external AI models to write control parameters into physical machinery without deterministic safety boundaries creates severe physical hazards.

MythOS requires a native, standardized AI integration architecture that:
1. Conforms to the open Model Context Protocol (MCP) specification (JSON-RPC 2.0).
2. Supports dual transports: High-speed standard I/O (`stdio`) for local agents and Server-Sent Events (`SSE`) for distributed network agents.
3. Automatically generates tool schemas directly from Rust code at compile time, eliminating schema synchronization drift.
4. Enforces strict safety interlocks and role-based access control (RBAC) before dispatching commands.

---

## 2. Decision Drivers

- **Zero Schema Drift**: Tool manifests, descriptions, and JSON Schema argument specifications must derive directly from Rust source code.
- **Protocol Standardization**: Adhere strictly to the Anthropic Model Context Protocol (MCP) specification.
- **Full Operational Surface**: AI agents must be able to list tags, inspect live telemetry, acknowledge alarms, query historical time-series, and execute approved setpoint writes.
- **Hard Safety Barriers**: Disallow any AI action that violates engineering unit limits, physical interlocks, or operator access modes.

---

## 3. Considered Options

1. **Option 1: External Python/Node.js MCP Wrapper Daemon**
   - *Pros*: Fast prototyping using official Python/TypeScript MCP SDKs.
   - *Cons*: Additional runtime dependency (Python/Node.js); IPC serialization latency; cannot directly access lock-free SHM pointers; prone to schema drift when Rust core evolves.
2. **Option 2: Generic REST API with Swagger/OpenAPI Documentation**
   - *Pros*: Familiar web development pattern.
   - *Cons*: Not natively compatible with MCP AI client tooling (Claude Desktop, autonomous agents); lacks standard bidirectional tool-call semantics and SSE session state.
3. **Option 3: Pure Rust Native MCP Server with `#[mythos_mcp_tool]` Auto-Registration** (Selected)
   - *Pros*: Embedded directly inside `mythos-mcp` crate; zero external language runtimes; sub-millisecond access to `/dev/shm/mythos_tagbus`; compile-time schema derivation via procedural macro; integrated safety interlock guards.

---

## 4. Decision Outcome

**Chosen Option**: Option 3 (Pure Rust Native MCP Server with `#[mythos_mcp_tool]`).

### 4.1 Architecture Details
- **Procedural Macro**: Authors declare tools using `#[mythos_mcp_tool(name = "...", category = "...", safety_level = "...")]` over async Rust functions. The macro automatically derives JSON Schema parameter definitions via `schemars` and registers the tool into the static registry.
- **Dual Transports**:
  * `stdio`: Framed JSON-RPC over stdin/stdout for co-located autonomous agents.
  * `SSE`: Axum HTTP server hosting `/mcp/sse` and `/mcp/message` endpoints for network clients.
- **Tool Inventory**: Standard catalog includes `mythos_list_tags`, `mythos_read_tag`, `mythos_read_batch`, `mythos_write_tag`, `mythos_get_alarm_summary`, `mythos_ack_alarm`, `mythos_query_historian`, and `mythos_get_runtime_status`.
- **Safety Interlocks**: Every `mythos_write_tag` invocation undergoes static tag mode validation (`WO`/`RW`), engineering unit range checks, and mandatory audit log generation recording caller identity and operator intent.

---

## 5. Consequences

### 5.1 Positive Consequences
- **Instant AI Interoperability**: AI agents can connect immediately via Claude Desktop or CLI and inspect the plant state without custom integrations.
- **Zero Maintenance Overhead for Schemas**: Changing a Rust tool parameter automatically updates the MCP JSON Schema at compile time.
- **Deterministic Safety**: Prevents hallucinations from corrupting physical equipment through strict interlock guards and bounds checking.

### 5.2 Negative Consequences
- **Macro Complexity**: Maintaining procedural macro code generation requires rigorous unit tests in `mythos-mcp-macros`.
