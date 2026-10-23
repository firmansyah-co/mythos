# MythOS Data and Storage Specification: Project File and Directory Structure

- **Document Identifier**: MYTHOS-DATA-004
- **Version**: 2.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification defines the canonical on-disk directory layout, file naming conventions, storage hierarchy, Write-Ahead Logging (WAL) mechanics, and JSON schema definitions for automation projects in MythOS.

In legacy SCADA systems, project files are either stored in proprietary opaque binary blobs or scattered across disparate registry paths. This legacy pattern creates:
1. Complete dependency on vendor-specific engineering suites.
2. Inability to perform version control using modern Git workflows.
3. Vulnerability to project corruption during unexpected power outages or controller hard resets.
4. Rigid 1:1 coupling between engineering workstations and field controllers.

MythOS establishes an **Open, Transparent, and Robust Storage Architecture**:
- **Primary Source of Truth on Target Controller**: The industrial controller (such as an Industrial PC or Raspberry Pi Compute Module 4 mounted inside the control cabinet) hosts the primary project filesystem under `/var/lib/mythos/projects/`. The controller boots independently, executes soft-logic 24/7, and serves HMI/API clients without requiring a connected engineering laptop.
- **Multi-Project Repository and Active Symlink**: The controller can store multiple independent projects simultaneously. Exactly one project is designated `ACTIVE` at runtime via an atomic filesystem symlink (`/var/lib/mythos/projects/active`). Draft projects can be loaded, modified, and validated concurrently in memory without disturbing the active control cycle.
- **Bumpless Hot-Swap (< 50ms Dead-Time)**: Switching the active project executes an atomic symlink swap and Native STruC++ dynamic library hot-swap during cyclic dead-time in less than 50 ms.
- **Atomic Debounced Autosave with WAL Protection**: Client modifications commit immediately to an append-only Write-Ahead Log (`project.wal`), followed by debounced atomic file replacement using temporary files and POSIX `rename`. This guarantees zero data loss and prevents partial file corruption.
- **Bidirectional Client Synchronization & 1-Click Export**: Engineering laptops can operate in offline mode and perform differential Git-style synchronizations (`push`/`pull`) or export/import self-contained, encrypted `.mythos-pkg` archives.

---

## 2. Canonical On-Disk Directory Layout

### 2.1 Multi-Project Filesystem Layout

On the target controller, the root data directory is `/var/lib/mythos/`:

```
/var/lib/mythos/
├── config/
│   ├── system.json                    -- Controller hardware & network daemon configuration
│   ├── security.json                  -- IEC 62443 RBAC policies, TLS keys, & MCP token registry
│   └── drivers.json                   -- Protocol driver initialization registry
├── projects/
│   ├── active -> proj_line01_bottling -- POSIX Atomic Symlink pointing to currently executing project
│   └── proj_line01_bottling/          -- Production Project Directory
│       ├── project.active.json        -- Live master project descriptor, network topology, layout presets
│       ├── project.draft.json         -- Staged master project descriptor under active edit
│       ├── tasks.active.json          -- Live IEC task definitions (intervals, priorities, core affinities)
│       ├── tasks.draft.json           -- Staged IEC task definitions under active configuration
│       ├── project.wal                -- Write-Ahead Log for active editor deltas
│       ├── logic/
│       │   ├── active/                -- Committed soft-logic executed by Native STruC++ runtime
│       │   │   ├── main.st            -- Active IEC 61131-3 Structured Text primary source
│       │   │   ├── udt_definitions.st -- Active User-Defined Data Types (UDT) declarations
│       │   │   ├── function_blocks/   -- Active Function Block sources (*.st)
│       │   │   └── build/             -- Pre-compiled dynamic shared object library & debug map
│       │   │       ├── generated.cpp  -- STruC++ emitted C++17 translation
│       │   │       ├── generated.hpp  -- STruC++ emitted class definitions
│       │   │       ├── debug-map.json -- Hierarchical symbol debug address map
│       │   │       └── libstrucpp_prog.so -- Currently executing dynamic shared object library
│       │   └── draft/                 -- Staged soft-logic sandbox under engineering edit
│       │       ├── main.st            -- Staged IEC 61131-3 Structured Text
│       │       ├── udt_definitions.st -- Staged UDT declarations
│       │       ├── function_blocks/   -- Staged Function Block sources
│       │       └── build/             -- Staged compilation test sandbox
│       │           ├── generated.cpp
│       │           ├── generated.hpp
│       │           ├── debug-map.json
│       │           └── libstrucpp_prog.so.staged -- Compiled staging binary ready for commit
│       ├── screens/
│       │   ├── active/                -- Production HMI screens rendered on operator terminals
│       │   │   ├── scr_bottling_overview/
│       │   │   │   ├── screen.json    -- Active primary screen layout & widget bindings
│       │   │   │   └── subscreens/    -- Active sub-screens and popup windows
│       │   │   │       ├── popup_valve_cv101.json
│       │   │   │       └── dock_alarm_tray.json
│       │   │   └── scr_cip_station/
│       │   │       ├── screen.json
│       │   │       └── subscreens/
│       │   └── draft/                 -- Staged HMI screens under engineering design & simulation
│       │       ├── scr_bottling_overview/
│       │       │   ├── screen.json    -- Staged primary screen layout
│       │       │   └── subscreens/    -- Staged sub-screens & popups
│       │       │       ├── popup_valve_cv101.json
│       │       │       └── dock_alarm_tray.json
│       │       ├── scr_cip_station/
│       │       │   ├── screen.json
│       │       │   └── subscreens/
│       │       └── scr_pasteurizer/   -- Staged new screen under creation
│       │           ├── screen.json
│       │           └── subscreens/
│       ├── faceplates/
│       │   ├── active/                -- Committed master faceplate types referenced by screens
│       │   │   ├── fp_vfd_motor/
│       │   │   │   └── definition.json -- Committed master faceplate schema & SVG templates
│       │   │   └── fp_analog_valve/
│       │   │       └── definition.json
│       │   └── draft/                 -- Staged master faceplate types under edit & testing
│       │       ├── fp_vfd_motor/
│       │       │   └── definition.json -- Staged master faceplate schema
│       │       └── fp_analog_valve/
│       │           └── definition.json
│       ├── subcanvases/               -- Reusable P&ID/Interactive sub-canvas modules
│       │   ├── pump_skid.subcanvas.json
│       │   ├── valve_manifold.subcanvas.json
│       │   └── reactor_vessel.subcanvas.json
│       ├── recipes/                   -- ISA-88 recipe files
│       │   ├── recipe_catalog.json
│       │   ├── beer_lager_v1.json
│       │   └── active_batch.json
│       └── assets/                    -- Media assets
│           ├── icons/
│           ├── backgrounds/
│           └── animations/
└── historian/                         -- Apache Parquet daily partitioned time-series storage
    ├── 2026-10-15/
    │   ├── chunk_0000.parquet
    │   └── chunk_0001.parquet
    └── catalog_manifest.json
```

---

### 2.2 Dual-Buffer Staging Architecture and Isolation Semantics

MythOS establishes an air-gapped filesystem model separating uncommitted engineering artifacts (`*.draft.json`, `*/draft/`) from actively executing production assets (`*.active.json`, `*/active/`).

| Subsystem | Staging Sandbox (Edit View) | Active Production (Runtime View) | Commit Promotion Trigger | Runtime Disruption |
| :--- | :--- | :--- | :--- | :--- |
| **Project Master** | `project.draft.json` | `project.active.json` | `POST /api/v1/control/draft/commit` | None |
| **Task Scheduler** | `tasks.draft.json` | `tasks.active.json` | `POST /api/v1/control/draft/commit` | Zero glitch (< 50ms dead-time) |
| **Logic & UDTs** | `logic/draft/` | `logic/active/` | `POST /api/v1/control/draft/commit` | Zero glitch (< 50ms dead-time) |
| **HMI Screens** | `screens/draft/` | `screens/active/` | `POST /api/v1/hmi/draft/commit` | Zero browser refresh (SSE React VDOM) |
| **Faceplates** | `faceplates/draft/` | `faceplates/active/` | `POST /api/v1/hmi/faceplates/{id}/propagate` | Zero browser refresh (SSE React VDOM) |

#### 2.2.1 Transaction Semantics and Atomic Swapping
Filesystem mutations follow ACID transactional properties on Linux filesystems (ext4, XFS, Btrfs):

1. **Atomic File Replacement**:
   - Single files (`tasks.draft.json` -> `tasks.active.json`) are promoted using `renameat2(AT_FDCWD, tmp_path, AT_FDCWD, target_path, 0)` or POSIX `rename(2)` which guarantees atomic directory entry substitution.
   - For transactional rollback safety, directory-level exchanges use `renameat2(AT_FDCWD, "screens/draft", AT_FDCWD, "screens/active", RENAME_EXCHANGE)` when supported by the underlying filesystem kernel, swapping draft and active trees in a single uninterruptible operation.
   - On legacy filesystems without `RENAME_EXCHANGE`, promotion employs a 3-step atomic transaction:
     ```
     1. cp -al screens/draft screens/active.next
     2. rename("screens/active", "screens/active.prev")
     3. rename("screens/active.next", "screens/active")
     4. rm -rf screens/active.prev
     ```
2. **Cryptographic Checksum Verification (`sha256`)**:
   - Every committed file and directory tree generates a corresponding SHA-256 digest recorded in `project.active.json` under `.manifest`.
   - Before executing dynamic library loading (`dlopen`) or hot-swapping task configurations, `mythos-core` calculates and verifies checksums. Any hash mismatch aborts the promotion transaction immediately.
3. **Automated Rollback Safeguard**:
   - If the STruC++ compiler fails, if dynamic symbol resolution encounters missing functions, or if memory allocation fails during Task Host initialization, the transaction triggers an automatic rollback:
     - The staged files are reverted to the pre-transaction active state.
     - The active running runtime remains undisturbed.
     - HTTP `422 Unprocessable Entity` or `500 Internal Error` is returned to the client with exact diagnostics.

---
## 3. Storage Hierarchy and Multi-Project Lifecycle

```
+───────────────────────────────────────────────────────────────────────────────────+
|                         CONTROLLER STORAGE REPOSITORY                             |
|                        (/var/lib/mythos/projects/)                                |
+───────────────────────────────────────────────────────────────────────────────────+
       ▲                                                             │
       │ Symlink: /projects/active                                   │
       ▼                                                             ▼
+──────────────────────────+                                  +─────────────────────+
|   ACTIVE PRODUCTION      |                                  |   LOADED DRAFT      |
|   PROJECT DIRECTORY      |                                  |   PROJECT DIRECTORY |
|   (proj_line01_bottling) |                                  |   (proj_line02_pst) |
+─────────────┬────────────+                                  +──────────┬──────────+
              │                                                          │
              │ STruC++ Compiled Binary                                  │ Monaco ST Editor
              ▼                                                          ▼
+──────────────────────────+                                  +─────────────────────+
|  Native STruC++ Engine   |                                  | Multi-User Collab   |
|  Lock-Free TagBus SHM    |                                  | Session Room (CRDT) |
|  24/7 Field Execution    |                                  | Offline Validation  |
+──────────────────────────+                                  +─────────────────────+
              ▲                                                          │
              │                  BUMPLESS HOT-SWAP                       │
              └─────────────────── (< 50ms Dead-Time) ◄──────────────────┘
```

### 3.1 Controller Primary Source of Truth
The controller is completely autonomous:
1. On system boot, the `mythos-core` daemon inspects `/var/lib/mythos/projects/active`.
2. It resolves the canonical project path, loads `project.json`, initializes drivers, and attaches `/dev/shm/mythos_tagbus`.
3. It loads `logic/build/libstrucpp_prog.so` and begins the cyclic real-time execution loop.
4. If an engineer disconnects their laptop or loses network connectivity, the controller continues uninhibited plant control.

### 3.2 Secondary Client Sync (Push/Pull/Export)
- **Local Laptop Execution**: Engineers can run the identical `mythos` binary locally on their workstation using a local directory `~/.mythos/projects/<project_id>/`.
- **Differential Push (`POST /api/v1/projects/sync/push`)**: Computes SHA-256 hashes of all project files and transfers modified files to the remote controller over HTTP/REST.
- **Differential Pull (`POST /api/v1/projects/sync/pull`)**: Downloads updated project files from the controller to the local workstation.
- **1-Click Packaging (`GET /api/v1/projects/{project_id}/export`)**: Compresses the entire project directory into an encrypted, tamper-evident `.mythos-pkg` archive for off-site archiving or disaster recovery.

---

## 4. Atomic Debounced Autosave and WAL Mechanics

To prevent disk wear on solid-state controller media (eMMC/NVMe) while eliminating data loss during browser crashes, MythOS implements a **Two-Tier Autosave Pipeline**:

```
[ Engineering Modification ] (Text edit in Monaco, Canvas drag, Tag bind)
            │
            ▼
[ Client-Side Debounce Timer ] (1000 ms idle window)
            │
            ├─► Triggered within 1000 ms? ──► Reset debounce timer
            │
            ▼ (1000 ms quiet window expires)
[ HTTP POST /api/v1/projects/:id/autosave ]
            │
            ▼
[ Tier 1: Write-Ahead Log (WAL) Append ]
  - Opens /var/lib/mythos/projects/<id>/project.wal
  - Appends binary delta record: [Header: CRC32, Timestamp, DocID, Length] + [Payload]
  - Calls fdatasync() -> Returns HTTP 200 OK to client in < 5 ms
            │
            ▼
[ Tier 2: Checkpoint Flush ] (Every 30 seconds OR on explicit Ctrl+S / Deploy)
  1. Parse latest WAL entries into consolidated file representations.
  2. Write target file to atomic sibling temporary file:
     /var/lib/mythos/projects/<id>/screens/main.screen.json.tmp_89f1a
  3. Flush buffer to media via fsync().
  4. Execute POSIX rename():
     rename("main.screen.json.tmp_89f1a", "main.screen.json")
  5. Truncate project.wal to zero bytes.
```

### 4.1 Write-Ahead Log Binary Record Format

| Field | Offset | Type | Description |
| :--- | :--- | :--- | :--- |
| `MAGIC` | `0x00` | `u32` | Magic bytes `0x4D595741` (`"MYWA"`) |
| `RECORD_CRC32` | `0x04` | `u32` | CRC32-C checksum over header and payload |
| `TIMESTAMP_US` | `0x08` | `u64` | UNIX Epoch timestamp in microseconds |
| `DOC_PATH_LEN` | `0x10` | `u16` | Length of relative document path string |
| `PAYLOAD_LEN` | `0x12` | `u32` | Length of document payload bytes |
| `DOC_PATH` | `0x16` | `UTF-8` | Relative path (e.g. `"logic/main.st"`) |
| `PAYLOAD` | `...` | `Bytes` | UTF-8 text delta or serialized JSON content |

### 4.2 Power-Loss Recovery Routine
If power cuts out before Tier 2 checkpointing:
1. Upon startup, `mythos-core` checks if `project.wal` has a size greater than zero.
2. It replays each valid binary record sequentially, verifying the CRC32 checksum.
3. Corrupt trailing records caused by mid-write power loss are automatically discarded.
4. The recovered document state is written to the destination file, guaranteeing zero data loss.

---

## 5. Master Project Schema (`project.active.json` & `project.draft.json`)

The `project.active.json` (live production) and `project.draft.json` (staging buffer) files reside in the root of each project directory and define project metadata, network topology, display defaults, and active operational parameters.

### 5.1 JSON Schema Definition (`project.schema.json`)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "MythosProjectConfiguration",
  "description": "Definitive schema for MythOS project configuration files (project.json)",
  "type": "object",
  "required": [
    "schema_version",
    "project_id",
    "metadata",
    "network_topology",
    "logic_config",
    "hmi_config",
    "alarm_config"
  ],
  "properties": {
    "schema_version": {
      "type": "string",
      "const": "1.0.0"
    },
    "project_id": {
      "type": "string",
      "pattern": "^[a-z0-9_]{3,32}$",
      "description": "Unique alphanumeric identifier for project directory"
    },
    "metadata": {
      "type": "object",
      "required": ["name", "version", "author", "created_at", "last_modified"],
      "properties": {
        "name": { "type": "string", "minLength": 1, "maxLength": 128 },
        "description": { "type": "string" },
        "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+(-[a-z0-9]+)?$" },
        "author": { "type": "string" },
        "facility": { "type": "string" },
        "area": { "type": "string" },
        "created_at": { "type": "integer", "format": "int64" },
        "last_modified": { "type": "integer", "format": "int64" }
      }
    },
    "network_topology": {
      "type": "object",
      "required": ["drivers", "devices"],
      "properties": {
        "drivers": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["driver_id", "driver_type", "enabled"],
            "properties": {
              "driver_id": { "type": "string" },
              "driver_type": { "type": "string", "enum": ["modbus_tcp", "modbus_rtu", "profinet", "mqtt_sparkplug", "opcua", "ethernet_ip"] },
              "enabled": { "type": "boolean" },
              "poll_interval_ms": { "type": "integer", "minimum": 1, "default": 20 },
              "parameters": { "type": "object" }
            }
          }
        },
        "devices": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["device_id", "driver_id", "station_address", "channels"],
            "properties": {
              "device_id": { "type": "string" },
              "driver_id": { "type": "string" },
              "station_address": { "type": "string" },
              "catalog_profile": { "type": "string" },
              "channels": {
                "type": "array",
                "items": {
                  "type": "object",
                  "required": ["channel_name", "tag_uri", "data_type", "access_mode"],
                  "properties": {
                    "channel_name": { "type": "string" },
                    "tag_uri": { "type": "string" },
                    "data_type": { "type": "string" },
                    "access_mode": { "type": "string", "enum": ["READ_ONLY", "READ_WRITE", "WRITE_ONLY"] },
                    "register_offset": { "type": "integer" }
                  }
                }
              }
            }
          }
        }
      }
    },
    "logic_config": {
      "type": "object",
      "required": ["primary_st_file", "task_cycle_time_us", "compiler_target"],
      "properties": {
        "primary_st_file": { "type": "string", "default": "logic/main.st" },
        "task_cycle_time_us": { "type": "integer", "minimum": 250, "default": 1000 },
        "cpu_affinity": { "type": "integer", "default": 1 },
        "compiler_target": { "type": "string", "default": "strucpp_cxx17" },
        "retain_storage": {
          "type": "object",
          "properties": {
            "wal_path": { "type": "string", "default": "nvram.wal" },
            "flush_interval_ms": { "type": "integer", "default": 100 }
          }
        }
      }
    },
    "hmi_config": {
      "type": "object",
      "required": ["default_screen", "canvas_dimensions", "theme"],
      "properties": {
        "default_screen": { "type": "string", "default": "screens/main.screen.json" },
        "canvas_dimensions": {
          "type": "object",
          "required": ["width", "height", "grid_size_px"],
          "properties": {
            "width": { "type": "integer", "default": 1920 },
            "height": { "type": "integer", "default": 1080 },
            "grid_size_px": { "type": "integer", "default": 16 }
          }
        },
        "theme": { "type": "string", "enum": ["industrial_dark", "industrial_light"], "default": "industrial_dark" },
        "layout_presets": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "preset_id": { "type": "string" },
              "name": { "type": "string" },
              "dockview_state": { "type": "object" }
            }
          }
        }
      }
    },
    "alarm_config": {
      "type": "object",
      "required": ["chatter_suppression", "default_deadband_pct"],
      "properties": {
        "chatter_suppression": {
          "type": "object",
          "properties": {
            "max_transitions_per_minute": { "type": "integer", "default": 5 },
            "auto_shelve_duration_sec": { "type": "integer", "default": 300 }
          }
        },
        "default_deadband_pct": { "type": "number", "default": 1.5 }
      }
    }
  }
}
```

### 5.2 Canonical `project.json` Example Instance

```json
{
  "schema_version": "1.0.0",
  "project_id": "proj_line01_bottling",
  "metadata": {
    "name": "Line 01 Bottling & Capping Skid",
    "description": "High-speed rotary bottling line with pneumatic capping and inspection",
    "version": "1.4.2",
    "author": "adrianaryaputra (firmansyahco)",
    "facility": "Plant 04 North",
    "area": "Bottling Skid A",
    "created_at": 1729000000000,
    "last_modified": 1729004200000
  },
  "network_topology": {
    "drivers": [
      {
        "driver_id": "drv_modbus_tcp_01",
        "driver_type": "modbus_tcp",
        "enabled": true,
        "poll_interval_ms": 20,
        "parameters": {
          "remote_ip": "192.168.1.101",
          "remote_port": 502,
          "timeout_ms": 500
        }
      },
      {
        "driver_id": "drv_modbus_rtu_01",
        "driver_type": "modbus_rtu",
        "enabled": true,
        "poll_interval_ms": 50,
        "parameters": {
          "serial_port": "/dev/ttyUSB0",
          "baud_rate": 19200,
          "data_bits": 8,
          "parity": "E",
          "stop_bits": 1
        }
      }
    ],
    "devices": [
      {
        "device_id": "dev_schneider_tm3",
        "driver_id": "drv_modbus_tcp_01",
        "station_address": "1",
        "catalog_profile": "schneider_tm3bceip_v1",
        "channels": [
          {
            "channel_name": "DI_EmergencyStop",
            "tag_uri": "external.field.bottling.di_estop",
            "data_type": "BOOL",
            "access_mode": "READ_ONLY",
            "register_offset": 0
          },
          {
            "channel_name": "DO_ConveyorRun",
            "tag_uri": "external.field.bottling.cmd_conveyor_run",
            "data_type": "BOOL",
            "access_mode": "READ_WRITE",
            "register_offset": 100
          }
        ]
      }
    ]
  },
  "logic_config": {
    "primary_st_file": "logic/main.st",
    "task_cycle_time_us": 1000,
    "cpu_affinity": 1,
    "compiler_target": "strucpp_cxx17",
    "retain_storage": {
      "wal_path": "nvram.wal",
      "flush_interval_ms": 100
    }
  },
  "hmi_config": {
    "default_screen": "screens/main.screen.json",
    "canvas_dimensions": {
      "width": 1920,
      "height": 1080,
      "grid_size_px": 16
    },
    "theme": "industrial_dark",
    "layout_presets": [
      {
        "preset_id": "operator_default",
        "name": "Standard Operator Console",
        "dockview_state": {}
      }
    ]
  },
  "alarm_config": {
    "chatter_suppression": {
      "max_transitions_per_minute": 5,
      "auto_shelve_duration_sec": 300
    },
    "default_deadband_pct": 1.5
  }
}
```

---

### 5.3 Task Scheduler Configuration Schema (`tasks.schema.json`)

The `tasks.active.json` and `tasks.draft.json` files define IEC 61131-3 task execution periods, priority rankings, CPU core affinities, and assigned Program Organization Units (POUs):

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "TaskSchedulerConfiguration",
  "description": "Schema for tasks.active.json and tasks.draft.json",
  "type": "object",
  "required": ["version", "tasks"],
  "properties": {
    "version": { "type": "string", "const": "1.0.0" },
    "last_modified": { "type": "integer", "format": "int64" },
    "modified_by": { "type": "string" },
    "tasks": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "cycle_time_us", "sched_priority", "cpu_core_id", "programs"],
        "properties": {
          "name": { "type": "string", "pattern": "^[A-Za-z0-9_]{3,32}$" },
          "cycle_time_us": { "type": "integer", "minimum": 1000, "maximum": 10000000 },
          "sched_priority": { "type": "integer", "minimum": 1, "maximum": 99 },
          "cpu_core_id": { "type": "integer", "minimum": 0, "maximum": 255 },
          "watchdog_timeout_us": { "type": "integer", "minimum": 1000 },
          "programs": {
            "type": "array",
            "items": { "type": "string" }
          }
        }
      }
    }
  }
}
```

#### Canonical `tasks.active.json` Example:
```json
{
  "version": "1.0.0",
  "last_modified": 1729005025000,
  "modified_by": "engineer_adrian",
  "tasks": [
    {
      "name": "Task_Fast",
      "cycle_time_us": 20000,
      "sched_priority": 90,
      "cpu_core_id": 2,
      "watchdog_timeout_us": 16000,
      "programs": ["MainProgram"]
    },
    {
      "name": "Task_Slow",
      "cycle_time_us": 500000,
      "sched_priority": 80,
      "cpu_core_id": 3,
      "watchdog_timeout_us": 400000,
      "programs": ["Diagnostics", "HistorianCollector"]
    }
  ]
}
```

---

## 6. Screen, Sub-Screen, and Master Faceplate Specifications

### 6.1 Screen and Sub-Screen Layout Model (`screens/`)
Following the TIA Portal hierarchical paradigm, screens are partitioned into top-level display contexts and associated sub-screens (popups, docked trays, modal setpoint panels):

```
screens/
├── active/
│   └── scr_bottling_overview/
│       ├── screen.json                -- Primary screen geometry, layers, and embedded widgets
│       └── subscreens/
│           ├── popup_valve_cv101.json -- Parameterized popup modal window
│           └── dock_alarm_tray.json   -- Docked status overlay
└── draft/
    └── scr_bottling_overview/
        ├── screen.json                -- Staged primary screen under edit
        └── subscreens/
            ├── popup_valve_cv101.json -- Staged popup modal
            └── dock_alarm_tray.json
```

#### 6.1.1 Screen Descriptor Schema (`screen.json`)
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "screen_id": "scr_bottling_overview",
  "title": "Bottling Line 01 Overview",
  "version": "1.2.0",
  "resolution": { "width": 1920, "height": 1080 },
  "background": {
    "color": "#121417",
    "grid_visible": false,
    "grid_snap": true,
    "grid_size": 16
  },
  "layers": [
    { "layer_id": "background_pipes", "locked": true, "visible": true },
    { "layer_id": "dynamic_equipment", "locked": false, "visible": true },
    { "layer_id": "operator_controls", "locked": false, "visible": true }
  ],
  "widgets": [
    {
      "widget_id": "vfd_feed_pump",
      "type": "faceplate_instance",
      "faceplate_type": "fp_vfd_motor",
      "version": "2.1.0",
      "layer": "dynamic_equipment",
      "transform": { "x": 320, "y": 240, "scale": 1.0, "rotation": 0 },
      "bindings": {
        "$MOTOR_TAG": "external.field.feed_pump01"
      },
      "interactions": {
        "on_click": {
          "action": "OPEN_SUBSCREEN",
          "target_subscreen_id": "popup_valve_cv101"
        }
      }
    }
  ]
}
```

#### 6.1.2 Sub-Screen Descriptor Schema (`subscreens/*.json`)
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "subscreen_id": "popup_valve_cv101",
  "parent_screen_id": "scr_bottling_overview",
  "title": "Control Valve CV-101 Diagnostics",
  "window_type": "POPUP_MODAL",
  "dimensions": { "width": 520, "height": 640 },
  "modal_overlay": true,
  "close_on_backdrop_click": false,
  "interface_parameters": [
    { "name": "$VALVE_TAG", "type": "UDT_Analog_Valve", "required": true }
  ],
  "widgets": [
    {
      "widget_id": "setpoint_slider",
      "type": "analog_slider",
      "transform": { "x": 40, "y": 120, "width": 440, "height": 48 },
      "bindings": {
        "value": "$VALVE_TAG.setpoint_percent",
        "feedback": "$VALVE_TAG.actual_position"
      }
    }
  ]
}
```

---

### 6.2 Master Faceplate Type Library (`faceplates/`)

Faceplates establish reusable, strongly-typed HMI components bound directly to PLC User-Defined Types (UDTs). Master faceplate definitions are stored centrally in `faceplates/draft/` and promoted to `faceplates/active/`:

```
faceplates/
├── active/
│   └── fp_vfd_motor/
│       └── definition.json
└── draft/
    └── fp_vfd_motor/
        └── definition.json
```

#### 6.2.1 Master Faceplate Schema (`definition.json`)
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type_id": "fp_vfd_motor",
  "name": "Variable Frequency Drive Motor Faceplate",
  "version": "2.1.0",
  "bound_udt": "UDT_Motor_VFD",
  "interface_parameters": [
    { "name": "$PV_SPEED", "type": "REAL", "direction": "INPUT", "description": "Actual motor RPM" },
    { "name": "$SP_SPEED", "type": "REAL", "direction": "IN_OUT", "description": "Commanded speed setpoint" },
    { "name": "$STATUS_WORD", "type": "WORD", "direction": "INPUT", "description": "Drive status word" },
    { "name": "$CMD_START", "type": "BOOL", "direction": "OUTPUT", "description": "Start contactor command" }
  ],
  "graphic_personas": {
    "compact": "<svg viewBox='0 0 64 64'>...</svg>",
    "standard": "<svg viewBox='0 0 160 120'>...</svg>",
    "diagnostic": "<svg viewBox='0 0 320 280'>...</svg>"
  },
  "color_palette_binding": {
    "running": "#00E676",
    "stopped": "#9E9E9E",
    "fault": "#FF1744",
    "interlocked": "#FFD600"
  }
}
```

#### 6.2.2 Global Version Propagation Mechanics
When an engineer triggers `POST /api/v1/hmi/faceplates/{type_id}/propagate`:
1. **Dependency Analysis**: The controller scans all screen files in `screens/active/` and `screens/draft/` matching `"faceplate_type": "{type_id}"`.
2. **Schema Compatibility**: Verifies that any new mandatory parameters have default fallback bindings to avoid broken tag references.
3. **Atomic File Write**: Copies `faceplates/draft/{type_id}/definition.json` to `faceplates/active/{type_id}/definition.json` using atomic temporary replace.
4. **Instance Upgrade**: Updates the `version` attribute in every dependent `screen.json` instance.
5. **SSE Notification**: Emits `FACEPLATE_PROPAGATED` so that active browser DOM nodes hot-swap their SVG persona graphics without reloading the page.

---

### 6.3 Sub-Canvas Schema (`subcanvases/*.subcanvas.json`)
Reusable assemblies (such as pump skids or PID valve loops) are encapsulated in `.subcanvas.json` files. They declare parameter interface aliases (`$PUMP_TAG`, `$RUN_FEEDBACK`) which resolve when placed onto a parent screen.

```json
{
  "subcanvas_id": "sub_centrifugal_pump",
  "name": "Centrifugal Feed Pump Skid",
  "version": "1.0.0",
  "dimensions": { "width": 320, "height": 240 },
  "interface_parameters": [
    { "name": "$PUMP_TAG", "type": "UDT_Motor_VFD", "required": true, "description": "Motor VFD composite UDT tag" },
    { "name": "$ALARM_TAG", "type": "BOOL", "required": false, "description": "High temperature interlock" }
  ],
  "graphic_elements": [
    {
      "element_id": "pump_icon",
      "type": "svg_symbol",
      "asset_path": "assets/icons/iso1219_centrifugal_pump.svg",
      "transform": { "x": 16, "y": 16, "scale": 1.0, "rotation": 0 },
      "bindings": {
        "status_color": "$PUMP_TAG.status_color",
        "rotation_speed": "$PUMP_TAG.pv_speed"
      }
    }
  ]
}
```

---

## 7. Cryptographic Packaging (`.mythos-pkg`)

When an engineer clicks **[Export Project Package]** or invokes `GET /api/v1/projects/{project_id}/export`, the controller bundles the project directory into a single encrypted file:

```
+-------------------------------------------------------------------------+
| .mythos-pkg CONTAINER FILE                                              |
| Magic: "MYTHOS_PKG" (11 Bytes) | Version: 1 | Key Deriv: Argon2id     |
+-------------------------------------------------------------------------+
| ENCRYPTED ZIP ARCHIVE (AES-256-GCM / ChaCha20-Poly1305)                 |
|                                                                         |
| ├── manifest.json                  -- Checksum manifest & signature     |
| ├── project.json                   -- Project configuration & topology  |
| ├── logic/                                                              |
| │   ├── main.st                    -- Structured Text source code       |
| │   └── build/libstrucpp_prog.so   -- Pre-compiled executable library   |
| ├── screens/                       -- All screen JSON definitions       |
| ├── subcanvases/                   -- Reusable component definitions    |
| ├── recipes/                       -- ISA-88 recipe files               |
| └── assets/                        -- SVGs, PNGs, and animation assets  |
+-------------------------------------------------------------------------+
| POLY1305 AUTHENTICATION TAG (16 Bytes)                                  |
+-------------------------------------------------------------------------+
```
