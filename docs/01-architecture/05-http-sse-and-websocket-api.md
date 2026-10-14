# MythOS Architecture Specification: HTTP, SSE, and WebSocket API

- **Document Identifier**: MYTHOS-ARCH-005
- **Version**: 2.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification defines the complete Hypertext Transfer Protocol (HTTP), Server-Sent Events (SSE), and WebSocket application programming interfaces (APIs) for the MythOS Supervisory Control and Data Acquisition (SCADA) platform.

The MythOS Microkernel hosts an asynchronous API Gateway implemented in pure Rust using `axum 0.7`, `tower 0.4`, `tower-http 0.5`, and `yrs 0.21` (pure Rust CRDT engine). This gateway provides the primary interface between the MythOS runtime host and connected engineering clients:
1. The integrated web-based engineering workbench and SCADA HMI.
2. The multi-user collaborative IDE with real-time text and canvas synchronization.
3. External industrial software components, MES/ERP bridges, edge gateways, and automated testing rigs.

The API architecture enforces four fundamental constraints:
- **Rate-Limited Telemetry Stream (100ms / 10 Hz Batched Ticks)**: While internal Rust driver loops poll field equipment at 10 ms to 50 ms, the egress SSE stream batches and throttles tag value distributions to 100 ms intervals (10 Hz). This prevents browser Document Object Model (DOM) churn and network buffer bloat.
- **Multi-Project Independent Hot-Switching (< 50ms Bumpless Swap)**: The controller stores multiple project packages simultaneously. Draft projects can be loaded, edited, and validated in IDE memory without disturbing the active production control loop. Activating a validated project triggers a bumpless dynamic library hot-swap in less than 50 ms during cycle dead-time.
- **Atomic Debounced Autosave (1000ms Idle WAL Flush)**: Client edits trigger debounced autosave events. Changes write atomically through a Write-Ahead Log (WAL) and temporary file atomic rename, preventing data loss during unexpected browser crashes or power interruptions.
- **Multi-User Real-Time Collaboration (CRDT / Yjs)**: Multiple engineers can simultaneously edit logic, schemas, and screens. Synchronized state updates flow over WebSocket through Conflict-Free Replicated Data Types (CRDT) powered by `yrs` on the backend and Yjs on the frontend, providing live presence avatars, multi-cursor Structured Text editing, and live canvas layout synchronization.

---

## 2. Communication Topology and Architecture

```
+----------------------------------------------------------------------------------------------------+
|                                      MYTHOS CLIENT WORKSPACE                                       |
|  - React 19 / TypeScript 5-Zone SCADA Workbench                                                    |
|  - Monaco Structured Text Collaborative Editor (y-monaco multi-cursor)                              |
|  - Bounded Grid Canvas & Interactive Topology View (y-websocket object sync)                        |
+---------------------+-----------------------------------+------------------------------------------+
                      |                                   |                                          |
     HTTP REST 1.1/2  |                  Server-Sent      |                  Dual WebSocket Channels |
     JSON Transaction |                  Events (SSE)     |                  (Port 8080)             |
     (Port 8080)      |                  (Port 8080)      |                  1. /collab/ws (CRDT)    |
                      |                  (100ms Batched)  |                  2. /telemetry/ws (1kHz) |
                      ▼                                   ▼                                          ▼
+---------------------+-----------------------------------+------------------------------------------+
|                           AXUM 0.7 ASYNCHRONOUS API GATEWAY (mythos-core)                          |
|  - Security Middleware: IEC 62443 RBAC Token Verification                                          |
|  - Rate-Limiting & Telemetry Batcher (100ms / 10 Hz Sliding Window)                                |
|  - Collaborative Session Manager (yrs CRDT Document Rooms & Presence Registry)                    |
|  - Project Manager: Multi-Project Repository & Active Symlink Router                              |
+---------------------+-----------------------------------+------------------------------------------+
                      |                                   |                                          |
         Memory Token |                      Lock-Free    |                             FFI Control  |
         Read / Write |                      Subscriber   |                             Hot-Reload   |
                      ▼                                   ▼                                          ▼
+------------------------------------+   +------------------------------------+   +------------------+
|       LOCK-FREE TAG BUS            |   |   ISA-18.2 ALARM & RECIPE ENGINE   |   |  OPENPLC V4 FFI  |
|   /dev/shm/mythos_tagbus (POSIX)   |   |   Deterministic Real-Time Workers  |   |  STruC++ Hot-Swap|
+------------------------------------+   +------------------------------------+   +------------------+
```

### 2.1 Transport Protocol Matrix

| Protocol | Path | Direction | Frequency / Timing | Purpose |
| :--- | :--- | :--- | :--- | :--- |
| **HTTP REST** | `/api/v1/*` | Client to Server | On demand (< 10 ms P99) | Configuration, CRUD, batch read/write, compilation, project lifecycle. |
| **SSE Stream** | `/api/v1/telemetry/stream` | Server to Client | Exactly 100 ms ticks (10 Hz) | Process tag value batches, alarm state transitions, scan progress, cycle metrics. |
| **WebSocket Collab** | `/api/v1/collab/ws` | Bidirectional | Real-time event-driven | Multi-user CRDT document sync (Yjs / `yrs`), cursor tracking, presence avatars. |
| **WebSocket Oscilloscope** | `/api/v1/telemetry/ws` | Bidirectional | Up to 1 kHz (1 ms ticks) | High-frequency oscilloscope traces, high-speed variable forcing. |

---

## 3. OpenAPI 3.1 REST API Specification

The base path for all REST interfaces is `/api/v1`.

### 3.1 Complete Endpoint Summary Table

| Category | Method | Path | Summary | Minimum RBAC Scope |
| :--- | :--- | :--- | :--- | :--- |
| **Projects** | `GET` | `/api/v1/projects` | List all projects residing on controller storage | `project:read` |
| **Projects** | `POST` | `/api/v1/projects` | Create a new project directory structure | `project:write` |
| **Projects** | `GET` | `/api/v1/projects/{project_id}` | Retrieve complete project metadata and schema | `project:read` |
| **Projects** | `DELETE` | `/api/v1/projects/{project_id}` | Delete project from storage (inactive projects only) | `project:admin` |
| **Projects** | `POST` | `/api/v1/projects/{project_id}/load` | Load project into memory/IDE as draft (starts CRDT room) | `project:read` |
| **Projects** | `POST` | `/api/v1/projects/{project_id}/unload` | Unload draft project from IDE memory | `project:read` |
| **Projects** | `POST` | `/api/v1/projects/{project_id}/activate` | Bumpless hot-swap project to active runtime (< 50ms) | `project:activate` |
| **Projects** | `POST` | `/api/v1/projects/{project_id}/autosave` | Commit debounced editor delta to project WAL | `project:write` |
| **Projects** | `GET` | `/api/v1/projects/{project_id}/export` | Export project archive (`.mythos-pkg`) | `project:export` |
| **Projects** | `POST` | `/api/v1/projects/import` | Import project package onto controller disk | `project:write` |
| **Projects** | `POST` | `/api/v1/projects/sync/pull` | Pull project file delta from controller to client | `project:read` |
| **Projects** | `POST` | `/api/v1/projects/sync/push` | Push project file delta from client to controller | `project:write` |
| **Tags** | `GET` | `/api/v1/tags` | List registered tags with filtering and pagination | `tags:read` |
| **Tags** | `GET` | `/api/v1/tags/{uri}` | Read single tag value, quality code, and timestamp | `tags:read` |
| **Tags** | `POST` | `/api/v1/tags/batch-read` | Atomic batch read of multiple tag URIs | `tags:read` |
| **Tags** | `POST` | `/api/v1/tags/batch-write` | Atomic batch write of multiple tag values | `tags:write` |
| **Tags** | `POST` | `/api/v1/tags/{uri}/force` | Force tag value override (IECVar forcing) | `tags:force` |
| **Tags** | `DELETE` | `/api/v1/tags/{uri}/force` | Release tag value force | `tags:force` |
| **Devices** | `GET` | `/api/v1/devices` | List configured field devices and driver links | `devices:read` |
| **Devices** | `POST` | `/api/v1/devices` | Register new field device configuration | `devices:write` |
| **Devices** | `GET` | `/api/v1/devices/{device_id}` | Read device configuration, diagnostics, and registers | `devices:read` |
| **Devices** | `PUT` | `/api/v1/devices/{device_id}` | Update device parameters | `devices:write` |
| **Devices** | `DELETE` | `/api/v1/devices/{device_id}` | Delete field device registration | `devices:write` |
| **Drivers** | `GET` | `/api/v1/drivers` | List loaded protocol drivers and capabilities | `system:read` |
| **Discovery** | `POST` | `/api/v1/discovery/scan` | Initiate heuristic fieldbus scan | `discovery:execute` |
| **Discovery** | `GET` | `/api/v1/discovery/scan/{scan_id}` | Poll discovery scan status and detected devices | `discovery:read` |
| **Discovery** | `DELETE` | `/api/v1/discovery/scan/{scan_id}` | Cancel active discovery scan | `discovery:execute` |
| **Control** | `GET` | `/api/v1/control/status` | Read soft-logic execution state and cycle metrics | `control:read` |
| **Control** | `POST` | `/api/v1/control/start` | Start soft-logic execution loop | `control:write` |
| **Control** | `POST` | `/api/v1/control/stop` | Stop soft-logic execution loop safely | `control:write` |
| **Control** | `POST` | `/api/v1/control/deploy` | Compile in-canvas Structured Text and hot-reload | `control:deploy` |
| **Control** | `GET` | `/api/v1/control/debug-map` | Retrieve compiled hierarchical symbol debug map | `control:read` |
| **Control Staging** | `GET` | `/api/v1/control/draft/status` | Check dirty state, diff between logic/tasks draft and active | `control:read` |
| **Control Staging** | `POST` | `/api/v1/control/draft/save` | Save uncommitted logic and task scheduler draft to staging buffer | `control:write` |
| **Control Staging** | `POST` | `/api/v1/control/draft/commit` | Promote draft to runtime (STruC++ compile, atomic task re-sched, hot-swap) | `control:deploy` |
| **Control Staging** | `POST` | `/api/v1/control/draft/discard` | Revert control logic and task drafts to active runtime state | `control:write` |
| **HMI Staging** | `GET` | `/api/v1/hmi/draft/status` | Check dirty state and diff between HMI draft and active | `hmi:read` |
| **HMI Staging** | `POST` | `/api/v1/hmi/draft/save` | Save draft HMI canvas layout & widget configs to staging buffer | `hmi:write` |
| **HMI Staging** | `POST` | `/api/v1/hmi/draft/commit` | Promote draft HMI layout to active production runtime via atomic exchange | `hmi:deploy` |
| **HMI Staging** | `POST` | `/api/v1/hmi/draft/discard` | Revert draft HMI canvas layout to active runtime state | `hmi:write` |
| **HMI Screens** | `GET` | `/api/v1/hmi/screens` | List all screens with nested sub-screens (draft and active status) | `hmi:read` |
| **HMI Screens** | `POST` | `/api/v1/hmi/screens` | Create new Screen in staging draft | `hmi:write` |
| **HMI Screens** | `GET` | `/api/v1/hmi/screens/{screen_id}` | Retrieve screen layout definition (supports `?view=draft` or `?view=active`) | `hmi:read` |
| **HMI Screens** | `PUT` | `/api/v1/hmi/screens/{screen_id}` | Update screen layout in staging draft | `hmi:write` |
| **HMI Screens** | `DELETE` | `/api/v1/hmi/screens/{screen_id}` | Delete screen from staging draft | `hmi:write` |
| **HMI Screens** | `POST` | `/api/v1/hmi/screens/{screen_id}/subscreens` | Create Sub-Screen / Popup Window in staging draft | `hmi:write` |
| **HMI Screens** | `GET` | `/api/v1/hmi/screens/{screen_id}/subscreens/{subscreen_id}` | Retrieve sub-screen layout definition (`?view=draft|active`) | `hmi:read` |
| **HMI Screens** | `PUT` | `/api/v1/hmi/screens/{screen_id}/subscreens/{subscreen_id}` | Update sub-screen layout in staging draft | `hmi:write` |
| **HMI Screens** | `DELETE` | `/api/v1/hmi/screens/{screen_id}/subscreens/{subscreen_id}` | Delete sub-screen from staging draft | `hmi:write` |
| **Faceplates** | `GET` | `/api/v1/hmi/faceplates` | List all master faceplate types with version info | `hmi:read` |
| **Faceplates** | `POST` | `/api/v1/hmi/faceplates` | Create new master faceplate type in staging draft | `hmi:write` |
| **Faceplates** | `GET` | `/api/v1/hmi/faceplates/{type_id}` | Retrieve master faceplate type definition (`?view=draft|active`) | `hmi:read` |
| **Faceplates** | `PUT` | `/api/v1/hmi/faceplates/{type_id}` | Update master faceplate draft definition | `hmi:write` |
| **Faceplates** | `DELETE` | `/api/v1/hmi/faceplates/{type_id}` | Delete master faceplate type from draft | `hmi:write` |
| **Faceplates** | `POST` | `/api/v1/hmi/faceplates/{type_id}/propagate` | Atomically trigger global version bump and instance migration | `hmi:deploy` |
| **Alarms** | `GET` | `/api/v1/alarms/active` | List active unacknowledged or active alarms | `alarms:read` |
| **Alarms** | `GET` | `/api/v1/alarms/summary` | Summary count of alarms by priority level | `alarms:read` |
| **Alarms** | `POST` | `/api/v1/alarms/{alarm_id}/ack` | Acknowledge active alarm | `alarms:ack` |
| **Alarms** | `POST` | `/api/v1/alarms/{alarm_id}/shelve` | Shelve alarm for defined duration | `alarms:shelve` |
| **Alarms** | `POST` | `/api/v1/alarms/{alarm_id}/unshelve` | Unshelve previously shelved alarm | `alarms:shelve` |
| **Recipes** | `GET` | `/api/v1/recipes` | List ISA-88 master recipes | `recipes:read` |
| **Recipes** | `POST` | `/api/v1/recipes` | Create new master recipe | `recipes:write` |
| **Recipes** | `GET` | `/api/v1/recipes/{recipe_id}` | Get recipe structure, phases, and parameters | `recipes:read` |
| **Recipes** | `PUT` | `/api/v1/recipes/{recipe_id}` | Update master recipe | `recipes:write` |
| **Recipes** | `POST` | `/api/v1/recipes/{recipe_id}/load` | Load recipe into active execution unit | `recipes:execute` |
| **Recipes** | `POST` | `/api/v1/recipes/active/start` | Start execution of loaded recipe | `recipes:execute` |
| **Recipes** | `POST` | `/api/v1/recipes/active/abort` | Abort active recipe execution | `recipes:execute` |
| **Recipes** | `GET` | `/api/v1/recipes/active/status` | Query active batch state and execution step | `recipes:read` |
| **Historian** | `GET` | `/api/v1/historian/query` | Query historical time-series process data | `historian:read` |
| **Historian** | `POST` | `/api/v1/historian/export/csv` | Request selective CSV export file generation | `historian:export` |
| **Historian** | `GET` | `/api/v1/historian/stats` | Historian partition size, record count, and health | `historian:read` |
| **Collab** | `GET` | `/api/v1/collab/sessions` | List active collaborative rooms and connected users | `project:read` |
| **System** | `GET` | `/api/v1/system/info` | Hardware architecture, OS version, runtime uptime | `system:read` |
| **System** | `GET` | `/api/v1/system/health` | Subsystem status (SHM, Drivers, FFI, Historian) | `system:read` |
| **System** | `GET` | `/api/v1/system/audit` | Query immutable tamper-evident audit trail log | `audit:read` |

---

## 4. Multi-Project Lifecycle and Hot-Switching Endpoints

The target controller acts as the primary repository of all plant automation projects. Multiple project configurations exist on disk concurrently under `/var/lib/mythos/projects/<project_id>/`.

### 4.1 Project Status States
- `ACTIVE`: The project currently executed by the OpenPLC soft-logic engine and driver SPI. Exactly one project is `ACTIVE` at any time. Symlinked at `/var/lib/mythos/projects/active`.
- `LOADED_DRAFT`: A project opened in memory by one or more engineering clients for editing and validation. Runs collaborative CRDT rooms and syntax verification without affecting the `ACTIVE` control loop.
- `IDLE_DRAFT`: A project stored on disk, not currently opened in memory.

### 4.2 Endpoint: `GET /api/v1/projects`
Returns an array of all project descriptors present on the controller:

```json
[
  {
    "project_id": "proj_line01_bottling",
    "name": "Line 01 Bottling & Capping",
    "version": "1.4.2",
    "status": "ACTIVE",
    "active_since": 1729000000000,
    "last_modified": 1729004200000,
    "connected_collaborators": 2,
    "path": "/var/lib/mythos/projects/proj_line01_bottling"
  },
  {
    "project_id": "proj_line02_pasteurizer",
    "name": "Line 02 Flash Pasteurization Unit",
    "version": "2.0.0-draft",
    "status": "LOADED_DRAFT",
    "active_since": null,
    "last_modified": 1729004812000,
    "connected_collaborators": 1,
    "path": "/var/lib/mythos/projects/proj_line02_pasteurizer"
  }
]
```

### 4.3 Endpoint: `POST /api/v1/projects/:id/load`
Loads a project into IDE memory, spins up the backend CRDT session room, and prepares in-memory AST buffers for engineering clients. The active production control runtime continues execution uninterrupted.

```json
// Request
{
  "client_session_id": "sess_89a1f4b"
}

// Response: HTTP 200 OK
{
  "project_id": "proj_line02_pasteurizer",
  "status": "LOADED_DRAFT",
  "collab_ws_endpoint": "/api/v1/collab/ws?project_id=proj_line02_pasteurizer",
  "document_count": 14,
  "loaded_at": 1729005000000
}
```

### 4.4 Endpoint: `POST /api/v1/projects/:id/activate` (Bumpless Hot-Swap < 50ms)
Transitions a draft project to `ACTIVE`.

#### Execution Sequence:
1. Verifies that all Structured Text logic compiles cleanly via STruC++ to `.so`.
2. Validates that hardware pin mappings and driver bus configs match physical I/O constraints.
3. Flushes current debounced autosave deltas to disk.
4. Waits for the PLC cyclic dead-time window.
5. Performs atomic pointer swap of the OpenPLC `ConfigurationInstance` and rebinds `/dev/shm/mythos_tagbus`.
6. Updates `/var/lib/mythos/projects/active` atomic symlink to point to the newly activated directory.
7. Total dead-time jitter is measured and guaranteed to remain strictly below 50 ms.

```json
// Request
{
  "force_recompile": false,
  "max_allowed_deadtime_ms": 50
}

// Response: HTTP 200 OK
{
  "success": true,
  "previous_active_project": "proj_line01_bottling",
  "new_active_project": "proj_line02_pasteurizer",
  "switch_duration_ms": 28,
  "hot_swap_deadtime_us": 480,
  "symlink_target": "/var/lib/mythos/projects/proj_line02_pasteurizer",
  "activated_at": 1729005120000
}
```

---

## 5. Telemetry SSE Stream (`GET /api/v1/telemetry/stream`)

### 5.1 Rate Limiting Contract (100ms / 10 Hz Batched Ticks)
Process values inside the pure Rust microkernel are polled from field hardware and OpenPLC shared memory at 10 ms to 50 ms cycles.

However, streaming updates at raw 50 Hz or 100 Hz directly to web browsers causes massive DOM thrashing, garbage collection pauses, and client UI freeze. 

The MythOS API Gateway implements an **Egress Telemetry Batcher**:
- The microkernel maintains a double-buffered ring of dirty tag entries.
- Every **100 ms (10 Hz)**, the batcher collects all modified tags, eliminates redundant intermediate writes, packages the net values into a single compressed `tag_update` batch, and flushes it down the SSE pipe.
- If an urgent event occurs (such as an ANSI/ISA-18.2 **Alarm State Transition**), the batcher bypasses the 100 ms window and dispatches the event immediately (< 1 ms latency).

### 5.2 SSE Wire Protocol Example

```http
GET /api/v1/telemetry/stream HTTP/1.1
Host: 192.168.1.50:8080
Accept: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
Authorization: Bearer <TOKEN>
Last-Event-ID: 1729005000000-00128
```

Server response:
```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
Transfer-Encoding: chunked

id: 1729005000100-00129
event: tag_update
data: {"timestamp":1729005000100,"batch_window_ms":100,"updates":[{"uri":"external.field.pump01.pv_speed","value":1480.2,"quality":"GOOD","access":"READ_ONLY"},{"uri":"external.field.tank01.pv_level_pct","value":78.4,"quality":"GOOD","access":"READ_ONLY"}]}

id: 1729005000115-00130
event: alarm_transition
data: {"timestamp":1729005000114,"alarm_id":"ALM_TANK01_HIGH","tag_uri":"external.field.tank01.pv_level_pct","old_state":"NORM","new_state":"UNACK","severity":"HIGH","current_value":78.4,"limit_value":75.0,"message":"Tank 01 high level threshold exceeded"}

:ping
```

### 5.3 Staging Lifecycle and Layout SSE Events

In addition to telemetry and alarms, the SSE pipe broadcasts control and HMI staging lifecycle events to keep multi-user engineering clients and operator displays synchronized in real time without polling.

#### Wire Protocol Example: Staging & Layout Events

```http
id: 1729005000200-00131
event: DRAFT_DIRTY
data: {"timestamp":1729005000200,"subsystem":"control","dirty_files":["tasks.draft.json","logic/draft/main.st"],"user":"engineer_adrian"}

id: 1729005000250-00132
event: DRAFT_COMMITTED
data: {"timestamp":1729005000250,"subsystem":"control","commit_hash":"sha256-a1b2c3d4e5f6...","promoted_files":["tasks.active.json","logic/active/build/libopenplc_prog.so"],"swap_dead_time_ms":12.4}

id: 1729005000280-00133
event: DRAFT_DISCARDED
data: {"timestamp":1729005000280,"subsystem":"hmi","reverted_files":["screens/draft/main.screen.json"],"user":"engineer_adrian"}

id: 1729005000300-00134
event: HMI_LAYOUT_UPDATED
data: {"timestamp":1729005000300,"action":"ATOMIC_COMMIT","affected_screens":["scr_bottling_overview","scr_pasteurizer"],"screens_version":"2.4.0","target_view":"active"}

id: 1729005000350-00135
event: FACEPLATE_PROPAGATED
data: {"timestamp":1729005000350,"type_id":"fp_vfd_motor_v2","from_version":"1.2.0","to_version":"2.0.0","migrated_instances_count":14,"affected_screens":["scr_bottling_overview","scr_cip_wash"]}
```

#### Event Catalog:
- **`DRAFT_DIRTY`**: Broadcast whenever uncommitted modifications are written to staging buffers (`tasks.draft.json`, `logic/draft/`, `screens/draft/`). Client IDEs illuminate amber "Draft Modified" indicators.
- **`DRAFT_COMMITTED`**: Broadcast upon successful atomic promotion of staged changes into the active production runtime.
- **`DRAFT_DISCARDED`**: Broadcast when an engineer cancels pending draft changes and rolls back the staging buffer to active state.
- **`HMI_LAYOUT_UPDATED`**: Broadcast to all running operator stations and wallboards. Triggers live React/SVG DOM reconciliation without browser reload or operational disruption.
- **`FACEPLATE_PROPAGATED`**: Broadcast when a master faceplate type is updated and propagated across dependent screen instances.

---

## 6. Multi-User Real-Time Collaborative IDE Protocol (`GET /api/v1/collab/ws`)

MythOS features a real-time collaborative editing environment inspired by Google Docs and VS Code Live Share. Multiple automation engineers, commissioning technicians, and control supervisors can open and edit the same project simultaneously across the local area network (LAN).

### 6.1 CRDT Foundation (`yrs` and `y-websocket`)
State synchronization is implemented using Conflict-Free Replicated Data Types (CRDT):
- **Server**: Backed by `yrs 0.21` (pure Rust implementation of the Yjs CRDT algorithm). Each loaded project hosts a thread-safe `Doc` instance wrapped in an actor channel.
- **Client**: Connects via `y-websocket` in the React 19 application.
- **Conflict Resolution**: Guaranteed convergence without server-side locking or merge conflicts.

### 6.2 Connection Upgrade & Handshake
`GET /api/v1/collab/ws?project_id=<id>&user_id=<user>&user_name=<name>&color=<hex>`

```http
GET /api/v1/collab/ws?project_id=proj_line02_pasteurizer&user_id=usr_neo&user_name=Neo&color=%2300ffaa HTTP/1.1
Host: 192.168.1.50:8080
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==
Sec-WebSocket-Version: 13
```

### 6.3 Collaborative Capabilities
1. **Multi-Cursor Monaco Structured Text Editor**:
   - `y-monaco` synchronizes the Monaco editor buffer with a shared `Y.Text` CRDT type (`logic/main.st`).
   - Remote cursors, text selections, and user avatar badges display in real time with distinctive user colors.
2. **Real-Time Bounded Grid Canvas Synchronization**:
   - Canvas objects, piping nodes, instrument symbols, and faceplates are represented in a shared `Y.Map` (`screens/main.screen.json`).
   - Moving or resizing a pump on Engineer A's screen updates instantly on Engineer B's screen without page refresh.
3. **Live Presence and Awareness**:
   - Ephemeral state (mouse cursor position, active document tab, selected element ID) synchronizes via the Yjs Awareness protocol.
   - The Top Industrial HUD displays live user avatar circles with ping latency indicators.

### 6.4 Wire Message Framing
The WebSocket binary stream transmits standard Yjs binary messages:
- `MessageSync (0)`: Handshake sync step 1, step 2, and incremental document updates.
- `MessageAwareness (1)`: User presence, active viewport, cursor coordinates, and client metadata.
- `MessageAuth (2)`: Security authentication challenge and token validation.

---

## 7. Atomic Debounced Autosave Pipeline

To guarantee zero loss of engineering work without generating disk I/O bottlenecks, MythOS enforces an **Atomic Debounced Autosave Engine**:

```
+───────────────────────────+
| Engineering Modification  | (Text stroke, Canvas drag, Parameter change)
+─────────────┬─────────────+
              │
              ▼
+───────────────────────────+
| Client Debounce (1000 ms) | (Resets on subsequent input within 1000 ms window)
+─────────────┬─────────────+
              │ POST /api/v1/projects/:id/autosave
              ▼
+───────────────────────────+
|   Server WAL Append       | (Appends incremental delta to project.wal with fsync)
+─────────────┬─────────────+
              │ Every 30 seconds OR on explicit manual Save
              ▼
+───────────────────────────+
|   Atomic Temporary Swap   | 1. Write clean file to /path/file.tmp
|   (Zero-Loss POSIX rename)| 2. rename(/path/file.tmp, /path/file.json)
+───────────────────────────+
```

### 7.1 Autosave Endpoint: `POST /api/v1/projects/{project_id}/autosave`

```json
// Request
{
  "client_id": "usr_neo",
  "document_path": "logic/main.st",
  "revision_id": 482,
  "payload": "PROGRAM MainProgram\n  VAR\n    Motor_Run AT %QX0.0 : BOOL;\n  END_VAR\nEND_PROGRAM",
  "checksum_sha256": "3a8b4c..."
}

// Response: HTTP 200 OK
{
  "status": "COMMITTED_WAL",
  "project_id": "proj_line02_pasteurizer",
  "document_path": "logic/main.st",
  "revision_id": 482,
  "wal_offset": 81920,
  "persisted_at": 1729005400000
}
```

---

## 8. High-Frequency WebSocket Protocol (`/api/v1/telemetry/ws`)

For sample rates exceeding 10 Hz (such as oscilloscope diagnostics at 100 Hz to 1 kHz):
- Clients establish a dedicated WebSocket connection: `GET /api/v1/telemetry/ws`.
- Telemetry frames transmit in compact Little-Endian binary format:

```
+-------------------------------------------------------------------------+
| OFFSET | TYPE    | FIELD DESCRIPTION                                    |
+--------+---------+------------------------------------------------------+
| 0x00   | u16     | Magic Identifier (0x4D59 = "MY")                     |
| 0x02   | u8      | Packet Type (0x01 = Oscilloscope Frame)              |
| 0x03   | u8      | Channel Count (N)                                    |
| 0x04   | u64     | Timestamp Microseconds (UNIX Epoch)                  |
| 0x0C   | f32 * N | IEEE 754 32-bit Floating Point Values [Channel 0..N] |
| END    | u32     | CRC32-C Checksum of Header and Payload                |
+-------------------------------------------------------------------------+
```

---

## 9. In-Canvas PLC Logic Compilation and Hot-Reload Endpoint

The integrated Monaco editor dispatches control logic directly to the target controller via `POST /api/v1/control/deploy`. There is no file upload form; the raw Structured Text string is submitted directly.

### 9.1 Request Contract (`POST /api/v1/control/deploy`)

```json
{
  "st_source": "PROGRAM MainProgram\n  VAR\n    Motor_Run AT %QX0.0 : BOOL;\n    Emergency_Stop AT %IX0.0 : BOOL;\n  END_VAR\n\n  IF NOT Emergency_Stop THEN\n    Motor_Run := TRUE;\n  ELSE\n    Motor_Run := FALSE;\n  END_IF;\nEND_PROGRAM",
  "target_program": "MainProgram",
  "compiler_options": {
    "optimization_level": "O2",
    "debug_symbols": true
  },
  "dry_run": false
}
```

### 9.2 Controller Execution Pipeline
1. Validates user token and checks `control:deploy` permission.
2. Writes code to temporary staging file: `/var/lib/mythos/projects/<active>/logic/st_staging.st`.
3. Invokes STruC++ (`strucpp`) compiler process in an isolated thread sandbox.
4. On compilation errors, returns HTTP `422 Unprocessable Entity` with exact file, line, and column coordinates.
5. On success, generates `.so` shared library and `debug-map.json`.
6. Dynamically loads `.so` via `dlopen`, binds `LocatedVar` pointers into `/dev/shm/mythos_tagbus`, executes atomic pointer hot-swap at scan dead-time (< 50 ms), and queues the old library for background disposal.

---

## 10. Decoupled Staging Lifecycle Architecture: Edit View vs Runtime View

To enforce strict operational integrity across industrial plants, MythOS decouples the **Engineering Edit View** (staging) from the **Live Production Runtime View** (operator execution).

Under this model:
- Plant operators interact exclusively with committed, production-hardened screens, faceplates, and logic (`screens/active/`, `faceplates/active/`, `logic/active/`, `tasks.active.json`).
- Control engineers work inside an isolated staging sandbox (`screens/draft/`, `faceplates/draft/`, `logic/draft/`, `tasks.draft.json`).
- Changes made in the engineering canvas or task configurator remain strictly unobservable to field operators and field controllers until an explicit, two-stage **Save -> Commit** sequence completes.

```
+─────────────────────────────────────────────────────────────────────────────+
|                         ENGINEERING WORKSTATION                             |
|                                                                             |
|   1. Engineer edits Canvas / Tasks / Logic in Web IDE                       |
|   2. POST /api/v1/hmi/draft/save  -->  Persists to screens/draft/           |
|   3. Simulation Preview Mode: Inspect draft with live or mock TagBus data   |
|   4. POST /api/v1/hmi/draft/commit                                          |
+──────────────────────────────────────┬──────────────────────────────────────+
                                       │
                                       │ Atomic Filesystem Swap (renameat2)
                                       │ & Real-Time SSE Dispatch
                                       ▼
+─────────────────────────────────────────────────────────────────────────────+
|                       MYTHOS RUNTIME & GATEWAY ENGINE                       |
|                                                                             |
|   - Promotes screens/draft/ -> screens/active/                              |
|   - OpenPLC pthreads update via cycle dead-time hot-swap (< 50ms)           |
|   - Broadcasts SSE: HMI_LAYOUT_UPDATED / FACEPLATE_PROPAGATED               |
+──────────────────────────────────────┬──────────────────────────────────────+
                                       │
                                       │ text/event-stream (Instant React VDOM Swap)
                                       ▼
+─────────────────────────────────────────────────────────────────────────────+
|                        OPERATOR INTERFACE (WALLBOARDS / HMIS)               |
|                                                                             |
|   Zero browser reload. Displays instantly re-render modified widgets        |
|   Continuous process control without interruption or visual flickering      |
+─────────────────────────────────────────────────────────────────────────────+
```

### 10.1 Control Logic & Task Staging Endpoints

#### 10.1.1 Query Staging Status (`GET /api/v1/control/draft/status`)
Checks whether control logic or task configuration contains uncommitted modifications.

- **Request**: `GET /api/v1/control/draft/status`
- **Response**: `200 OK`
```json
{
  "is_dirty": true,
  "last_modified": 1729005012400,
  "modified_by": "engineer_adrian",
  "active_hash": "sha256-e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "draft_hash": "sha256-82f5b8a96d11f879685a210d7a468d6ef77fbe684e2a875a901004a88f7c9e5e",
  "diff_summary": {
    "tasks_modified": true,
    "task_changes": [
      {
        "task_name": "Task_Fast",
        "field": "cycle_time_us",
        "active_value": 100000,
        "draft_value": 20000
      }
    ],
    "logic_modified": true,
    "logic_files": ["logic/draft/main.st"]
  }
}
```

#### 10.1.2 Save Staging Draft (`POST /api/v1/control/draft/save`)
Persists uncommitted logic code and task configurations into the staging directory without affecting the running engine.

- **Request**: `POST /api/v1/control/draft/save`
```json
{
  "tasks": [
    {
      "name": "Task_Fast",
      "cycle_time_us": 20000,
      "sched_priority": 90,
      "cpu_core_id": 2,
      "programs": ["MainProgram"]
    },
    {
      "name": "Task_Slow",
      "cycle_time_us": 500000,
      "sched_priority": 80,
      "cpu_core_id": 3,
      "programs": ["Diagnostics"]
    }
  ],
  "st_source_files": [
    {
      "path": "logic/draft/main.st",
      "content": "PROGRAM MainProgram\n  // Modified high-speed logic\nEND_PROGRAM"
    }
  ]
}
```
- **Response**: `200 OK`
```json
{
  "status": "SAVED_TO_DRAFT",
  "draft_hash": "sha256-82f5b8a96d11f879685a210d7a468d6ef77fbe684e2a875a901004a88f7c9e5e",
  "saved_at": 1729005015000
}
```

#### 10.1.3 Discard Staging Draft (`POST /api/v1/control/draft/discard`)
Reverts all uncommitted logic and task scheduler staging files back to match the active runtime state.

- **Request**: `POST /api/v1/control/draft/discard`
```json
{
  "confirm": true
}
```
- **Response**: `200 OK`
```json
{
  "status": "DISCARDED",
  "reverted_to_hash": "sha256-e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "timestamp": 1729005020000
}
```

#### 10.1.4 Commit Staging Draft to Runtime (`POST /api/v1/control/draft/commit`)
Validates draft task configurations, compiles Structured Text through STruC++, creates the updated shared library (`libopenplc_prog.so`), and executes an atomic hot-swap at scan dead-time.

- **Request**: `POST /api/v1/control/draft/commit`
```json
{
  "comment": "Promoted Task_Fast to 20ms cycle and updated pump interlock",
  "require_bumpless": true
}
```
- **Response**: `200 OK`
```json
{
  "status": "COMMITTED_AND_ACTIVE",
  "active_hash": "sha256-82f5b8a96d11f879685a210d7a468d6ef77fbe684e2a875a901004a88f7c9e5e",
  "compilation_time_ms": 340,
  "hot_swap_dead_time_ms": 14.8,
  "active_tasks_count": 2,
  "timestamp": 1729005025000
}
```

---

### 10.2 TIA Portal Style Screen & Sub-Screen CRUD Endpoints

Screens and sub-screens (popups, faceplate containers, diagnostic overlays) are organized in a hierarchical tree. All screen mutations default to the staging workspace (`view=draft`) and require explicit commit before appearing on production displays.

#### 10.2.1 Screen Hierarchy Tree (`GET /api/v1/hmi/screens`)
Retrieves the full screen catalog, including nested sub-screens, layout metadata, and draft dirty status.

- **Request**: `GET /api/v1/hmi/screens?view=draft`
- **Response**: `200 OK`
```json
{
  "view": "draft",
  "screens": [
    {
      "screen_id": "scr_bottling_overview",
      "title": "Bottling Line 01 Overview",
      "is_home": true,
      "status": "DIRTY",
      "resolution": { "width": 1920, "height": 1080 },
      "subscreens": [
        {
          "subscreen_id": "popup_valve_cv101",
          "title": "Control Valve CV-101 Detail",
          "window_type": "POPUP_MODAL",
          "dimensions": { "width": 480, "height": 560 }
        },
        {
          "subscreen_id": "dock_alarm_tray",
          "title": "Bottom Alarm Banner",
          "window_type": "DOCK_BOTTOM",
          "dimensions": { "width": 1920, "height": 160 }
        }
      ]
    },
    {
      "screen_id": "scr_cip_station",
      "title": "CIP Cleaning Station",
      "is_home": false,
      "status": "COMMITTED",
      "resolution": { "width": 1920, "height": 1080 },
      "subscreens": []
    }
  ]
}
```

#### 10.2.2 Create Screen (`POST /api/v1/hmi/screens`)
Creates a new screen in the staging workspace (`screens/draft/<screen_id>/screen.json`).

- **Request**: `POST /api/v1/hmi/screens`
```json
{
  "screen_id": "scr_pasteurizer",
  "title": "Flash Pasteurizer Unit",
  "is_home": false,
  "resolution": { "width": 1920, "height": 1080 },
  "background_color": "#121417",
  "grid": { "snap": true, "size": 16 }
}
```
- **Response**: `201 Created`

#### 10.2.3 Read, Update, Delete Screen
- `GET /api/v1/hmi/screens/{screen_id}?view=draft|active`: Fetches full widget layout JSON.
- `PUT /api/v1/hmi/screens/{screen_id}`: Updates widget placement, bindings, and layout in staging.
- `DELETE /api/v1/hmi/screens/{screen_id}`: Removes screen from staging.

#### 10.2.4 Sub-Screen / Popup Window Endpoints
- `POST /api/v1/hmi/screens/{screen_id}/subscreens`: Creates a new popup window or docked panel linked to a parent screen.
```json
{
  "subscreen_id": "popup_pid_tuning",
  "title": "PID Controller Loop Tuning",
  "window_type": "POPUP_DRAGGABLE",
  "dimensions": { "width": 640, "height": 480 },
  "parameters": [
    { "name": "$LOOP_TAG", "type": "UDT_PID_Loop", "required": true }
  ]
}
```
- `GET /api/v1/hmi/screens/{screen_id}/subscreens/{subscreen_id}?view=draft|active`
- `PUT /api/v1/hmi/screens/{screen_id}/subscreens/{subscreen_id}`
- `DELETE /api/v1/hmi/screens/{screen_id}/subscreens/{subscreen_id}`

---

### 10.3 HMI Staging Lifecycle Endpoints

#### 10.3.1 Check HMI Draft Status (`GET /api/v1/hmi/draft/status`)
- **Response**: `200 OK`
```json
{
  "is_dirty": true,
  "staged_screen_count": 3,
  "modified_screens": ["scr_bottling_overview"],
  "created_screens": ["scr_pasteurizer"],
  "deleted_screens": [],
  "last_saved": 1729005030000
}
```

#### 10.3.2 Save Draft HMI Layout (`POST /api/v1/hmi/draft/save`)
Flushes dirty canvas edits from browser state into `screens/draft/` and records deltas into WAL.

#### 10.3.3 Discard Draft HMI Layout (`POST /api/v1/hmi/draft/discard`)
Discards all staging modifications, purging `screens/draft/` and restoring a mirror copy of `screens/active/`.

#### 10.3.4 Commit Draft HMI Layout to Active (`POST /api/v1/hmi/draft/commit`)
Executes atomic filesystem directory promotion (`screens/draft/` -> `screens/active/`) and emits the `HMI_LAYOUT_UPDATED` SSE event to all connected runtime clients.

- **Request**: `POST /api/v1/hmi/draft/commit`
```json
{
  "version_tag": "v1.4.0",
  "change_description": "Added Pasteurizer screen and updated CV-101 popup"
}
```
- **Response**: `200 OK`
```json
{
  "status": "COMMITTED",
  "version": "v1.4.0",
  "promoted_screens_count": 3,
  "commit_timestamp": 1729005045000,
  "sse_event_id": "1729005045000-00136"
}
```

---

### 10.4 Faceplate Library Editor, Master Types & Global Propagation

Faceplates represent reusable, parameterized graphical components bound to IEC 61131-3 User-Defined Types (UDTs). MythOS provides a centralized Faceplate Master Type Library.

#### 10.4.1 Faceplate Master Type Endpoints
- **`GET /api/v1/hmi/faceplates`**: Lists all registered master faceplates with version numbers, UDT bindings, and instance counts across all screens.
- **`POST /api/v1/hmi/faceplates`**: Creates a new master faceplate definition in staging.
- **`GET /api/v1/hmi/faceplates/{type_id}?view=draft|active`**: Returns faceplate schema, SVG persona templates, and tag interface.
- **`PUT /api/v1/hmi/faceplates/{type_id}`**: Saves modified faceplate design into `faceplates/draft/{type_id}/`.
- **`DELETE /api/v1/hmi/faceplates/{type_id}`**: Removes faceplate definition if no screen instances depend on it.

#### 10.4.2 Global Atomic Propagation (`POST /api/v1/hmi/faceplates/{type_id}/propagate`)
When a control engineer modifies a master faceplate type (e.g., adding an interlock indicator badge to `FP_VFD_Motor`), all instantiations across all screens must update synchronously.

- **Request**: `POST /api/v1/hmi/faceplates/{type_id}/propagate`
```json
{
  "new_version": "2.1.0",
  "migration_strategy": "AUTO_UPGRADE",
  "default_parameter_bindings": {
    "$INTERLOCK_OK": ".safety_interlock"
  }
}
```
- **Execution Flow**:
  1. Inspects dependency tree: locates every screen referencing `{type_id}`.
  2. Runs compatibility check: verifies all existing tag parameter bindings remain valid under the new faceplate version interface.
  3. Updates faceplate definition in `faceplates/active/{type_id}/definition.json`.
  4. Automatically reconciles instance configurations in all dependent screen JSON files.
  5. Emits `FACEPLATE_PROPAGATED` SSE event to all connected clients.
- **Response**: `200 OK`
```json
{
  "status": "PROPAGATED",
  "type_id": "FP_VFD_Motor",
  "new_version": "2.1.0",
  "updated_screens": [
    "scr_bottling_overview",
    "scr_cip_station",
    "scr_pasteurizer"
  ],
  "instances_migrated": 18,
  "timestamp": 1729005060000
}
```

---

### 10.5 Zero-Refresh Live Client Synchronization

MythOS operator views utilize real-time reactive Virtual DOM diffing backed by the SSE stream:

1. When `HMI_LAYOUT_UPDATED` or `FACEPLATE_PROPAGATED` arrives via SSE, the web client receives the list of modified screen IDs and their content hashes.
2. If the operator's display currently presents an affected screen:
   - The frontend engine fetches the delta JSON in the background without refreshing the browser tab or disrupting active user interactions.
   - The layout engine applies React / SVG diffing to swap out only changed nodes (e.g. updated geometry, newly added faceplate widgets, revised color palettes).
   - In-flight operator actions (such as slider adjustments, setpoint dialog entries, or alarm shelf clicks) are preserved without loss of context.


---

## 11. Complete OpenAPI 3.1 JSON Schemas

### 11.1 Project Descriptor Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "ProjectDescriptor",
  "type": "object",
  "required": ["project_id", "name", "version", "status", "last_modified", "path"],
  "properties": {
    "project_id": { "type": "string" },
    "name": { "type": "string" },
    "version": { "type": "string" },
    "status": { "type": "string", "enum": ["ACTIVE", "LOADED_DRAFT", "IDLE_DRAFT"] },
    "active_since": { "type": ["integer", "null"] },
    "last_modified": { "type": "integer" },
    "connected_collaborators": { "type": "integer", "minimum": 0 },
    "path": { "type": "string" }
  }
}
```

### 11.2 Tag Data Transfer Object Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "TagDto",
  "type": "object",
  "required": ["uri", "value", "quality", "timestamp", "access_mode"],
  "properties": {
    "uri": {
      "type": "string",
      "pattern": "^external\\.[a-z0-9_]+\\.[a-z0-9_]+\\.[a-z0-9_]+(\\.[a-z0-9_]+)*$"
    },
    "value": {},
    "quality": {
      "type": "string",
      "enum": ["GOOD", "BAD_COMM_FAILURE", "BAD_SENSOR_FAILURE", "UNCERTAIN_INITIALIZING", "FORCED"]
    },
    "timestamp": { "type": "integer", "format": "int64" },
    "access_mode": {
      "type": "string",
      "enum": ["READ_ONLY", "READ_WRITE", "WRITE_ONLY", "CONSTANT"]
    },
    "forced": { "type": "boolean" }
  }
}
```

### 11.3 Deploy Request Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "DeployLogicRequest",
  "type": "object",
  "required": ["st_source"],
  "properties": {
    "st_source": { "type": "string" },
    "target_program": { "type": "string", "default": "MainProgram" },
    "compiler_options": {
      "type": "object",
      "properties": {
        "optimization_level": { "type": "string", "enum": ["O0", "O1", "O2", "O3"], "default": "O2" },
        "debug_symbols": { "type": "boolean", "default": true }
      }
    },
    "dry_run": { "type": "boolean", "default": false }
  }
}
```

### 11.4 ISA-18.2 Alarm State Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "AlarmStateDto",
  "type": "object",
  "required": ["alarm_id", "tag_uri", "state", "severity", "timestamp"],
  "properties": {
    "alarm_id": { "type": "string" },
    "tag_uri": { "type": "string" },
    "state": {
      "type": "string",
      "enum": ["NORM", "UNACK", "ACKED", "RTN_UNACK", "SHELVED", "SUPPRESSED", "OUT_OF_SERVICE"]
    },
    "severity": {
      "type": "string",
      "enum": ["CRITICAL", "HIGH", "MEDIUM", "LOW", "DIAGNOSTIC"]
    },
    "current_value": { "type": "number" },
    "limit_value": { "type": "number" },
    "timestamp": { "type": "integer", "format": "int64" },
    "shelve_expiration": { "type": ["integer", "null"], "format": "int64" }
  }
}
```

### 11.5 Control and HMI Staging Status Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "StagingStatusDto",
  "type": "object",
  "required": ["is_dirty", "active_hash", "draft_hash", "last_modified"],
  "properties": {
    "is_dirty": { "type": "boolean" },
    "active_hash": { "type": "string" },
    "draft_hash": { "type": "string" },
    "last_modified": { "type": "integer", "format": "int64" },
    "modified_by": { "type": "string" },
    "diff_summary": {
      "type": "object",
      "properties": {
        "tasks_modified": { "type": "boolean" },
        "logic_modified": { "type": "boolean" },
        "screens_modified": { "type": "boolean" },
        "faceplates_modified": { "type": "boolean" }
      }
    }
  }
}
```

### 11.6 Screen Hierarchy Descriptor Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "ScreenHierarchyDto",
  "type": "object",
  "required": ["screen_id", "title", "is_home", "status", "resolution"],
  "properties": {
    "screen_id": { "type": "string" },
    "title": { "type": "string" },
    "is_home": { "type": "boolean" },
    "status": { "type": "string", "enum": ["COMMITTED", "DIRTY", "NEW"] },
    "resolution": {
      "type": "object",
      "required": ["width", "height"],
      "properties": {
        "width": { "type": "integer" },
        "height": { "type": "integer" }
      }
    },
    "subscreens": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["subscreen_id", "title", "window_type"],
        "properties": {
          "subscreen_id": { "type": "string" },
          "title": { "type": "string" },
          "window_type": { "type": "string", "enum": ["POPUP_MODAL", "POPUP_DRAGGABLE", "DOCK_TOP", "DOCK_BOTTOM", "DOCK_LEFT", "DOCK_RIGHT"] }
        }
      }
    }
  }
}
```

### 11.7 Master Faceplate Type Definition Schema

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "MasterFaceplateDefinitionDto",
  "type": "object",
  "required": ["type_id", "name", "version", "bound_udt", "interface_parameters"],
  "properties": {
    "type_id": { "type": "string" },
    "name": { "type": "string" },
    "version": { "type": "string" },
    "bound_udt": { "type": "string" },
    "interface_parameters": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "data_type", "direction"],
        "properties": {
          "name": { "type": "string" },
          "data_type": { "type": "string" },
          "direction": { "type": "string", "enum": ["INPUT", "OUTPUT", "IN_OUT"] }
        }
      }
    },
    "graphic_personas": {
      "type": "object",
      "properties": {
        "compact": { "type": "string" },
        "standard": { "type": "string" },
        "detailed": { "type": "string" }
      }
    }
  }
}
```

---

## 12. Error Response Conventions

All error responses from the API Gateway adhere to RFC 7807 (*Problem Details for HTTP APIs*):

```json
{
  "type": "https://mythos.industrial/errors/tag-access-denied",
  "title": "Forbidden Tag Access",
  "status": 403,
  "detail": "Tag 'external.field.flow_meter_01.sensor_raw' is designated READ_ONLY. Direct write rejected.",
  "instance": "/api/v1/tags/batch-write",
  "timestamp": 1729000005120,
  "error_code": "ERR_TAG_READ_ONLY",
  "invalid_params": [
    {
      "name": "external.field.flow_meter_01.sensor_raw",
      "reason": "Physical input channel cannot be modified via software API"
    }
  ]
}
```
