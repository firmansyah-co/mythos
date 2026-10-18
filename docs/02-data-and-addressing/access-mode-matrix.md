# MythOS Data Specification: Access Mode Matrix and Shadow Buffer Pattern

- **Document Identifier**: MYTHOS-DATA-003
- **Version**: 2.0.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification defines the Access Mode enforcement matrix and the Command/State Shadow Buffer pattern implemented across MythOS.

In distributed automation systems, uncontrolled concurrent writes cause race conditions, feedback flutter, and dangerous command collisions. For example, if an operator HMI writes a motor setpoint to the same memory slot continuously updated by a field inverter, the values will overwrite each other unpredictably.

MythOS resolves this problem through:
1. **Strict Tri-State Access Mode Enforcement**: `RO` (Read-Only), `WO` (Write-Only / Command), and `RW` (Read-Write / Shadowed Parameter).
2. **Command / State Shadow Buffer Architecture**: Decoupling the commanded setpoint from the actual field feedback.
3. **Dirty-Flag Event Triggers**: Dispatching outbound network frames only when a commanded variable is mutated.
4. **Plant Hierarchy Tag Nomenclature**: Adhering strictly to `external.<namespace>.<device>.<variable>[.<subfield>]` for all field variables.

---

## 2. Access Mode Matrix

| Mode | Semantic Name | Permitted Writers | Permitted Readers | Industrial Use Case |
| :--- | :--- | :--- | :--- | :--- |
| **RO** | Read-Only | Field Protocol Driver, Native STruC++ Engine outputs | HMI, Historian, MCP AI Agents, Rule Engines | Process feedback, physical sensor readings, status bitmasks, fault codes, operating hours. |
| **WO** | Write-Only | HMI Operator actions, MCP AI Tools, Higher-level Batch scripts | Field Protocol Driver (outbound transmission queue) | Momentary commands: Start, Stop, Reset, E-Stop trip, jog pulse, calibration zero command. |
| **RW** | Read-Write | Native STruC++ Logic, HMI (via Shadow Buffer), MCP Agents | All subsystems | PID tuning setpoints (Kp, Ti), temperature limits, recipe setpoint arrays, operational modes. |

---

## 3. Command / State Shadow Buffer Pattern

The Command / State pattern separates what is *requested* from what is *confirmed* in the physical field device.

```
                  Operator HMI / MCP Agent
                             │
                             │ Command Write: target = 1500 RPM
                             ▼
+───────────────────────────────────────────────────────────────────────────+
| Command Shadow Buffer (`external.BoilerRoom.FeedPump01.speed.sp`)         |
|   - Marked as DIRTY                                                       |
|   - Access Mode: WO / RW                                                  |
+───────────────────────────────────────────────────────────────────────────+
                               │
                               │ Driver consumes dirty flag
                               ▼ Outbound Field Bus Write (Modbus FC16 / PROFINET)
+───────────────────────────────────────────────────────────────────────────+
| Physical Field Inverter (VFD - FeedPump01 in BoilerRoom)                  |
|   - Accelerates motor towards 1500 RPM                                    |
+───────────────────────────────────────────────────────────────────────────+
                               │
                               │ Inbound Field Bus Cyclic Read (Modbus FC03)
                               ▼
+───────────────────────────────────────────────────────────────────────────+
| State Feedback Buffer (`external.BoilerRoom.FeedPump01.speed.pv`)         |
|   - Actual measured speed: 1485 RPM                                       |
|   - Quality: GOOD (0x00C0)                                                |
|   - Access Mode: RO                                                       |
+───────────────────────────────────────────────────────────────────────────+
                               │
                               ▼
                      HMI / MCP displays live feedback
```

### 3.1 Tag Hierarchy & Separation Invariants
1. **Canonical URI Compliance**:
   - `external`: Variable is mapped to a physical external field inverter.
   - `BoilerRoom`: Plant process area / machine group namespace.
   - `FeedPump01`: Physical device identifier.
   - `speed`: Controlled metric.
   - `.sp` / `.pv`: Subfields separating command setpoint from confirmed process value.
2. **Zero Self-Echo**: An HMI write to `speed.sp` NEVER mutates `speed.pv`. `speed.pv` reflects only confirmed physical field device telemetry.
3. **Visual Consistency (ISA-101)**: The HMI renders both the commanded setpoint (e.g., target indicator line) and the actual process value (solid bar). If `speed.pv` fails to track `speed.sp` within a defined tolerance window, a deviation alarm triggers.

---

## 4. Dirty-Flag Write Trigger Architecture

To avoid flooding field buses (especially low-bandwidth RS-485 serial multidrop lines at 9600 baud) with redundant continuous write commands, MythOS implements an atomic dirty-flag mechanism.

### 4.1 Shadow Buffer State Structure

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use mythos_shm::{TagSlot, TagValueRaw};

pub struct ShadowBufferSlot {
    pub slot_id: u32,
    pub is_dirty: AtomicBool,
    pub pending_value: TagValueRaw,
    pub last_dispatched_value: TagValueRaw,
    pub last_command_time_ns: AtomicU64,
}

impl ShadowBufferSlot {
    /// Invoked by HMI / MCP / API Gateway to submit a new setpoint
    pub fn submit_command(&mut self, new_val: TagValueRaw, timestamp_ns: u64) {
        self.pending_value = new_val;
        self.last_command_time_ns.store(timestamp_ns, Ordering::Release);
        // Atomically mark dirty to trigger driver dispatch
        self.is_dirty.store(true, Ordering::Release);
    }

    /// Invoked by Protocol Driver cyclic task
    pub fn take_if_dirty(&mut self) -> Option<TagValueRaw> {
        if self.is_dirty.swap(false, Ordering::AcqRel) {
            self.last_dispatched_value = self.pending_value;
            Some(self.pending_value)
        } else {
            None
        }
    }
}
```

### 4.2 Outbound Driver Dispatch Loop

```rust
use std::time::Duration;
use tracing::info;

pub async fn run_outbound_dispatch_worker(
    mut slots: Vec<ShadowBufferSlot>,
    mut modbus_client: tokio_modbus::client::Context,
) {
    let poll_interval = Duration::from_millis(20);

    loop {
        for slot in slots.iter_mut() {
            if let Some(val_to_write) = slot.take_if_dirty() {
                // Execute hardware register write only when dirty flag was asserted
                let register_val = unsafe { val_to_write.u16_val };
                let address = slot.slot_id as u16;

                info!(
                    slot_id = slot.slot_id,
                    value = register_val,
                    "Dispatching dirty setpoint to field device"
                );

                if let Err(e) = modbus_client.write_single_register(address, register_val).await {
                    // Re-mark dirty for retry on transient comms error
                    slot.is_dirty.store(true, std::sync::atomic::Ordering::Release);
                }
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}
```

---

## 5. Security and Role-Based Access Enforcement

The Microkernel validates every write operation against the system access control matrix:

```rust
use crate::error::NamespaceError;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CallerRole {
    Viewer,
    Operator,
    Engineer,
    McpAgent,
    SystemAdmin,
}

pub fn check_write_permission(
    tag_mode: AccessMode,
    caller_role: CallerRole,
) -> Result<(), NamespaceError> {
    // 1. Enforce Tag Access Mode
    if tag_mode == AccessMode::ReadOnly {
        return Err(NamespaceError::ReadOnlyAccess(
            "Attempted write to READ_ONLY tag is prohibited".into(),
        ));
    }

    // 2. Enforce Role Permissions
    match caller_role {
        CallerRole::Viewer => Err(NamespaceError::ReadOnlyAccess(
            "Viewer role has read-only privileges across all namespaces".into(),
        )),
        CallerRole::Operator | CallerRole::Engineer | CallerRole::McpAgent | CallerRole::SystemAdmin => {
            Ok(())
        }
    }
}
```


---

## 6. Device Execution Modes & Zero-Silent-Fallback Invariant

### 6.1 Industrial Safety Context & The Adrian Rule
In mission-critical industrial automation, communication loss to an actuator, PLC, or field sensor represents an immediate physical safety hazard. If a physical communication cable is severed (e.g., RS-485 multidrop cut, Ethernet jack unplugged, or serial port missing), an automated control system MUST NEVER silently fall back to generating synthetic or simulated data. 

Generating synthetic data on communication failure masks physical process drift, blinds operators to loss of field observability, and violates IEC 61508 / ISO 13849 safety principles.

**Mandatory Invariant (Zero Silent Fallback)**:
1. In `PhysicalHardware` mode (production default), any physical failure (missing COM/tty port, socket failure, severed line, or protocol timeout) MUST immediately assert `QualityCode::BAD_COMM_FAILURE` (`0x0008` / `0x0018`) on all associated process variables.
2. Under ZERO circumstances shall a driver or the runtime silently switch from `PhysicalHardware` to simulation mode.
3. Simulation mode can ONLY be engaged through an **explicit, authenticated manual operator action** on a per-device basis.

---

### 6.2 Per-Device Execution Mode (`DeviceExecutionMode`)

Every external field device registered under the `external.<namespace>.<device>` hierarchy maintains an isolated, independent operational mode state machine:

```rust
use serde::{Deserialize, Serialize};

/// Operational execution mode configured per external field device
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum DeviceExecutionMode {
    /// Default Production Mode:
    /// - Strictly binds to physical interface (serial tty, TCP socket, CAN bus).
    /// - Port opening errors and frame timeouts emit hard BAD_COMM_FAILURE alarms.
    /// - Zero simulation or synthetic fallback permitted.
    #[default]
    PhysicalHardware = 0,

    /// Staging & Commissioning Simulation Mode:
    /// - Suppresses physical interface allocation and port opening errors.
    /// - Unlocks manual TagBus injection, Quick Input Slider, and Direct Variable Forcing.
    /// - Stamped with high-visibility amber warning indicator [🧪 SIMULATION ACTIVE].
    /// - Emits immutable audit log entry on transition.
    ManualSimulation = 1,
}
```

#### Operational Mode Comparison

| Operational Dimension | `PhysicalHardware` (Default Production) | `ManualSimulation` (Staging / Commissioning) |
| :--- | :--- | :--- |
| **Default State** | Mandatory factory default for all devices | Requires explicit operator toggling |
| **Physical I/O Binding** | Binds to physical COM/tty, raw socket, or IP | Hardware allocation bypassed / suppressed |
| **Port Missing / Broken Wire** | Hard alarm: `BAD_COMM_FAILURE` (`0x0008`/`0x0018`) | Errors suppressed; simulator engine handles values |
| **Tag Value Source** | Physical cyclic polling via protocol driver SPI | Interactive slider, variable forcing, or test script |
| **Quality Code Output** | `GOOD (0x00C0)` or `BAD_COMM_FAILURE (0x0018)` | `UNCERTAIN_SUBSTITUTED (0x0044)` with SIM flag |
| **UI Visual Banner** | Normal device status (Green / Gray / Red Alarm) | Amber Badge: `[🧪 SIMULATION ACTIVE]` |
| **Audit Trail Requirement** | Standard driver lifecycle telemetry | High-priority audit record with operator identity |

---

### 6.3 Quick Slider & Direct Variable Forcing TagBus Injection Interface

When a device is explicitly transitioned to `DeviceExecutionMode::ManualSimulation`, the TagBus unlocks manual injection mechanisms in the Tag Inspector and Developer Console.

```
                      OPERATOR DESKTOP / TOUCH PANEL
                                    │
                        [ 🧪 SIMULATION ACTIVE ]
            ┌───────────────────────┴───────────────────────┐
            ▼                                               ▼
   Quick Input Slider                            Direct Variable Forcing
 [─────●────────────] 42.5 bar                  Value: 1200 RPM [ Force Override ]
            │                                               │
            └───────────────────────┬───────────────────────┘
                                    │
                                    ▼
                +───────────────────────────────────────+
                | TagBus Simulated Injection Engine     |
                |   - Mode: ManualSimulation Verified   |
                |   - Quality: UNCERTAIN_SUBSTITUTED    |
                |   - Target: external.Area.Device.Tag  |
                +───────────────────────────────────────+
                                    │
                                    ▼
                +───────────────────────────────────────+
                | Lock-Free Triple-Buffered SHM Slot    |
                |   - Raw Value: Injected Float / Int   |
                |   - Quality: 0x0044                   |
                |   - Timestamp: CLOCK_REALTIME         |
                +───────────────────────────────────────+
                                    │
            ┌───────────────────────┴───────────────────────┐
            ▼                                               ▼
   Native STruC++ Engine                           SCADA Canvas Mimic Faceplate
   Reads simulated input                           Displays value with amber border
```

#### Invariant Enforcement Rules:
1. **Hardware Mode Lockout**: If `DeviceExecutionMode == PhysicalHardware`, all injection endpoints (`POST /api/v1/simulation/inject`, Quick Slider events, and Forcing hooks) REJECT incoming mutations with HTTP `403 Forbidden` (`ErrSimulationModeDisabled`).
2. **Quality Code Stamping**: All tags written via simulation injection are marked with OPC DA quality `UNCERTAIN_SUBSTITUTED` (`0x0044`), ensuring downstream historian systems, safety trip logic, and Native STruC++ scripts detect non-physical data.
3. **Audit Trail Logging**:
   Every state change and variable forcing action generates an immutable audit record:
   ```json
   {
     "timestamp_ns": 1772986400000000000,
     "operator_id": "Adrian",
     "action": "ENABLE_SIMULATION_MODE",
     "target_device": "external.BoilerRoom.PT101",
     "previous_mode": "PhysicalHardware",
     "new_mode": "ManualSimulation",
     "reason": "Cold loop commissioning prior to field wiring"
   }
   ```
