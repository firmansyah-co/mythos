# MythOS Architecture Specification: Model Context Protocol (MCP) Subsystem

- **Document Identifier**: MYTHOS-ARCH-004
- **Version**: 1.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification defines the native Model Context Protocol (MCP) subsystem integrated into the MythOS Pure Rust Microkernel.

The Model Context Protocol enables external autonomous Artificial Intelligence (AI) agents (such as Large Language Model assistants and autonomous enterprise division heads) to directly discover, inspect, diagnose, and supervise industrial SCADA operations.

Rather than relying on external web scrapers or unsecured REST endpoints, MythOS implements a **first-class, native MCP Server in Pure Rust**. The server supports:
1. Dual transports: High-speed standard input/output (`stdio`) and Server-Sent Events (`SSE`) over HTTP.
2. Compile-time procedural macro tool registration via `#[mythos_mcp_tool]`.
3. Strict industrial safety interlocks and Role-Based Access Control (RBAC) preventing unauthorized or dangerous physical state mutations.

---

## 2. MCP Server Architecture and Transports

```
┌─────────────────────────────────┐       ┌─────────────────────────────────┐
│     AI Agent (CLI / Local)      │       │     AI Agent (Network / Web)    │
└────────────────┬────────────────┘       └────────────────┬────────────────┘
                 │ stdio transport                         │ SSE + POST transport
                 ▼                                         ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                      MythOS MCP Server (`mythos-mcp`)                     │
│                                                                           │
│   ┌──────────────────────────┐             ┌──────────────────────────┐   │
│   │ Stdio Transport Handler  │             │   Axum SSE/HTTP Router   │   │
│   └────────────┬─────────────┘             └─────────────┬────────────┘   │
│                │                                         │                │
│                └────────────────────┬────────────────────┘                │
│                                     ▼                                     │
│                     ┌───────────────────────────────┐                     │
│                     │ JSON-RPC 2.0 Dispatch Router  │                     │
│                     └───────────────┬───────────────┘                     │
│                                     │                                     │
│     ┌───────────────────────────────┴───────────────────────────────┐     │
│     ▼                                                               ▼     │
│  [Discovery Service]                                      [Execution Engine]│
│  - tools/list                                             - tools/call      │
│  - resources/list                                                           │
│  - prompts/list                                                    │        │
│                                                                    ▼        │
│                                                    +──────────────────────+ │
│                                                    | Safety Interlock &   | │
│                                                    | RBAC Authorization   | │
│                                                    +──────────────┬───────+ │
│                                                                   │ Pass    │
│                                                                   ▼         │
│                                                    +──────────────────────+ │
│                                                    | Tool Dispatch Handler| │
│                                                    +──────────────────────+ │
└───────────────────────────────────────────────────────────────────┼───────┘
                                                                    │
                                   ┌────────────────────────────────┴───────┐
                                   ▼                                        ▼
                         +──────────────────+                     +──────────────────+
                         | MythOS Shm Bus   |                     | OpenPLC & Kernel |
                         +──────────────────+                     +──────────────────+
```

### 2.1 Transport Layer Details
1. **Stdio Transport**:
   - Primary transport for co-located AI processes (e.g., local Claude Desktop, headless command-line autonomous agents).
   - Frames JSON-RPC messages separated by standard newlines (`\n`).
   - Standard error (`stderr`) is reserved strictly for operational logs; standard output (`stdout`) contains pure JSON-RPC payloads.
2. **SSE Transport (HTTP)**:
   - Primary transport for distributed agents and network dashboards.
   - Endpoint `/mcp/sse`: Provides an active event stream emitting server messages and connection session IDs.
   - Endpoint `/mcp/message?session_id=<id>`: Receives incoming JSON-RPC POST requests and dispatches responses over the SSE connection.

---

## 3. Tool Auto-Registration Procedural Macro (`#[mythos_mcp_tool]`)

MythOS eliminates manual JSON schema synchronization by deriving tool definitions directly from Rust function signatures using `#[mythos_mcp_tool]`.

### 3.1 Macro Invariants and Code Generation
When applied to an asynchronous function, `#[mythos_mcp_tool]` generates:
1. An OpenAPI / JSON Schema compliant parameter definition struct via `schemars`.
2. An automatic registration hook into the global `ToolRegistry` using the `inventory` or `linkme` crate.
3. Serialization and deserialization wrappers enforcing strong type checking.

### 3.2 Concrete Tool Authoring Example

```rust
use mythos_mcp_macros::mythos_mcp_tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use crate::context::McpContext;
use crate::error::McpError;

#[derive(Deserialize, JsonSchema)]
pub struct ReadTagArgs {
    /// Canonical tag URI (e.g., "external.BoilerRoom.PT101.pressure.pv")
    pub uri: String,
}

#[derive(Serialize, JsonSchema)]
pub struct ReadTagResponse {
    pub uri: String,
    pub value: serde_json::Value,
    pub quality: String,
    pub timestamp_ns: u64,
}

/// Read real-time process variable from the MythOS lock-free Shared Memory bus.
#[mythos_mcp_tool(
    name = "mythos_read_tag",
    category = "scada_telemetry",
    safety_level = "read_only"
)]
pub async fn read_tag(
    ctx: McpContext,
    args: ReadTagArgs,
) -> Result<ReadTagResponse, McpError> {
    let slot = ctx.shm_reader.resolve_tag_by_uri(&args.uri)
        .ok_or_else(|| McpError::ToolNotFound(format!("Tag URI '{}' not found", args.uri)))?;

    Ok(ReadTagResponse {
        uri: args.uri,
        value: slot.value_as_json(),
        quality: format!("{:?}", slot.quality),
        timestamp_ns: slot.timestamp_ns,
    })
}

#[derive(Deserialize, JsonSchema)]
pub struct WriteTagArgs {
    /// Canonical tag URI
    pub uri: String,
    /// Numerical or boolean value to write
    pub value: f64,
    /// Mandatory reason string explaining why the AI agent is modifying plant state
    pub operator_intent: String,
}

#[derive(Serialize, JsonSchema)]
pub struct WriteTagResponse {
    pub success: bool,
    pub previous_value: f64,
    pub new_value: f64,
    pub audit_id: String,
}

/// Write a command setpoint or discrete state into a field device tag.
#[mythos_mcp_tool(
    name = "mythos_write_tag",
    category = "scada_control",
    safety_level = "safety_critical"
)]
pub async fn write_tag(
    ctx: McpContext,
    args: WriteTagArgs,
) -> Result<WriteTagResponse, McpError> {
    // 1. Verify safety interlocks
    ctx.safety_interlock.assert_write_permitted(&args.uri, args.value, &ctx.caller_role)?;

    // 2. Perform write via kernel shadow buffer
    let (prev, new, audit_id) = ctx.kernel.write_tag_with_audit(
        &args.uri,
        args.value,
        &args.operator_intent,
        &ctx.caller_identity,
    ).await?;

    Ok(WriteTagResponse {
        success: true,
        previous_value: prev,
        new_value: new,
        audit_id,
    })
}
```

---

## 4. Standard Surface Control Tool Catalog

The MythOS native MCP server provides complete operational visibility to AI agents through the following core tools:

| Tool Name | Safety Level | Description |
| :--- | :--- | :--- |
| `mythos_list_tags` | Read Only | Returns hierarchy of all registered tags filtered by namespace prefix. |
| `mythos_read_tag` | Read Only | Reads instantaneous value, quality code, and timestamp of a tag. |
| `mythos_read_batch` | Read Only | Efficiently reads up to 500 tags in a single atomic snapshot. |
| `mythos_write_tag` | Safety Critical | Modifies a writable tag setpoint with mandatory intent logging. |
| `mythos_get_alarm_summary` | Read Only | Returns list of active alarms, unacknowledged states, and Shelved alarms. |
| `mythos_ack_alarm` | Operator Control | Acknowledges an active industrial alarm per ISA-18.2 state machine. |
| `mythos_query_historian` | Read Only | Retrieves historical time-series points over a designated time window. |
| `mythos_get_runtime_status`| Read Only | Returns CPU usage, cycle times, overrun statistics, and driver health. |
| `mythos_reload_openplc` | Engineer Control| Compiles and hot-reloads Structured Text into the real-time core. |

---

## 5. Industrial Safety Interlocks and Authorization Guards

Industrial operations cannot permit unrestricted, unverified AI state changes. MythOS implements a **Deterministic Safety Interlock Guard**:

### 5.1 Interlock Verification Sequence
When an AI agent executes `mythos_write_tag`:
1. **Access Mode Verification**: The target tag must possess `WO` (Write-Only) or `RW` (Read-Write) permissions in the catalog. Writing to `RO` tags yields immediate rejection.
2. **Clamp and Rate Limiting**: The target value is checked against engineering units minimum (`EU_MIN`) and maximum (`EU_MAX`). Values out of range are rejected with an explicit clamp error.
3. **Physical Interlock Matrix**: If physical safety interlocks are tripped (e.g., E-Stop active, motor protection overload trip), command dispatch fails immediately.
4. **Mandatory Intent Audit Trail**: Every write operation logs the agent's identity, timestamp, previous value, requested value, and `operator_intent` reason directly into the immutable append-only audit log (`/var/log/mythos/audit.log`).
