# ANSI/ISA-88 Batch Control & Recipe Engine Specification

## 1. Executive Summary & Purpose
This document specifies the software architecture, data structures, and deterministic PLC synchronization contracts for the MythOS Recipe Management Subsystem. The design complies strictly with ANSI/ISA-88.01-2010 (*Batch Control Part 1: Models and Terminology*) and integrates with ISA-TR88.00.02 (PackML) procedural state execution.

The MythOS Recipe Engine bridges high-level process manufacturing recipes (formulations, step sequences, phase parameters) to physical runtime execution in pure Rust and OpenPLC Runtime v4. It guarantees two-way atomic synchronization over the lock-free TagBus (`/dev/shm/mythos_tagbus`), preventing partial parameter writes or race conditions during batch execution.

---

## 2. Normative References
- **ANSI/ISA-88.01-2010**: *Batch Control Models and Terminology*.
- **ANSI/ISA-88.00.02-2001**: *Batch Control Part 2: Data Structures and Guidelines for Languages*.
- **ISA-TR88.00.02-2015**: *Machine and Unit States: An Implementation Example of ANSI/ISA-88.01 (PackML)*.
- **IEC 61512-1**: *Batch control - Part 1: Models and terminology*.
- **21 CFR Part 11**: *Electronic Records; Electronic Signatures (FDA batch integrity)*.

---

## 3. ISA-88 Structural Hierarchy

MythOS models batch production using the four-tier ISA-88 procedural hierarchy:

```
+-------------------------------------------------------------------------+
| PROCEDURE (Master Recipe)                                               |
| High-level strategy to manufacture a complete batch entity              |
+-------------------------------------------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| UNIT PROCEDURE                                                          |
| Contiguous sequence of operations conducted within a single unit        |
| (e.g., Sterilizer Vessel 1, Reaction Tank A, Mix Blender B)             |
+-------------------------------------------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| OPERATION                                                               |
| Independent processing activity causing chemical/physical change        |
| (e.g., Deaeration, Heating Ramp, Holding Soak, Steam Exhaust)            |
+-------------------------------------------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
| PHASE (PLC Equipment Phase)                                             |
| Smallest procedural element directly commanding field actuators         |
| Executed inside OpenPLC v4 with bidirectional TagBus handshakes         |
+-------------------------------------------------------------------------+
```

---

## 4. Recipe Model: Formula vs. Procedure

In MythOS, a Recipe comprises two distinct components: **Formula (Parameters)** and **Procedure (Logic Sequence)**.

```
+-----------------------------------------------------------------------------+
|                                MASTER RECIPE                                |
|  ID: "RCP-CPO-STERIL-003"              Version: "2.1.0"                     |
|  Product: "High-FFA Palm Fruit Sterilization"                               |
+--------------------------------------+--------------------------------------+
| 1. FORMULA (Setpoints & Tolerances)  | 2. PROCEDURE (Sequential Steps)      |
|--------------------------------------|--------------------------------------|
| - Target Temp:       142.5 degC      | Step 1: Door Interlock & Seal Check  |
| - Deaeration Time:   300 s           | Step 2: Vacuum Purge Phase           |
| - Peak 1 Pressure:   1.50 barg       | Step 3: Steam Ramp Phase             |
| - Peak 2 Pressure:   2.00 barg       | Step 4: Triple-Peak Cook Phase       |
| - Holding Pressure:  2.90 barg       | Step 5: Exhaust to Atmospheric       |
| - Soak Duration:     2700 s          | Step 6: Condensate Blowdown          |
| - Exhaust Rate:      0.15 barg/min   | Step 7: Batch Completion & Unload    |
+--------------------------------------+--------------------------------------+
```

### 4.1 Formula Definition (Parameters & Constraints)
The formula encapsulates target values, allowable tolerances, and validation rules. Formula parameters cannot be altered once a batch is released to production.

- **Formula Parameter Types**:
  - `Float`: Engineering values (temperature, pressure, flow rate, dosing mass).
  - `Integer`: Discrete counts (pump pulse target, agitator speed RPM).
  - `Boolean`: Binary flags (nitrogen blanket required, inline filter bypass).
  - `Duration`: Time intervals with millisecond precision.
- **Validation Envelope**:
  Every parameter defines `min_allowed`, `max_allowed`, `nominal_default`, and `engineering_unit`. Any recipe upload violating these bounds is rejected before admission.

### 4.2 Procedural Step Model (PackML State Mapping)
Each Recipe Step invokes an equipment Phase mapped to an ISA-TR88 PackML execution machine:

```
                  +--------------+
                  |   STOPPED    |
                  +-------+------+
                          | Reset
                          v
                  +--------------+
                  |     IDLE     |
                  +-------+------+
                          | Start
                          v
                  +--------------+
                  |   STARTING   |
                  +-------+------+
                          |
                          v
+-------------+   +--------------+   +-------------+
|  HOLDING    |<--|   EXECUTE    |-->| SUSPENDING  |
+------+------+   +-------+------+   +------+------+
       |                  |                 |
       v                  v                 v
+-------------+   +--------------+   +-------------+
|    HELD     |   |  COMPLETING  |   |  SUSPENDED  |
+------+------+   +-------+------+   +------+------+
       |                  |                 |
       v                  v                 v
+-------------+   +--------------+   +-------------+
| UN-HOLDING  |   |   COMPLETE   |   | UNSUSPENDING|
+-------------+   +--------------+   +-------------+
```

---

## 5. Two-Way PLC Synchronization Contract

To ensure absolute safety and deterministic execution, recipe parameters are never transferred directly into live PLC control variables while equipment is moving.

### 5.1 TagBus Memory Layout: `external.Recipe.*`

MythOS establishes a dedicated memory namespace for PLC handshake and parameter exchange:

```
external.Recipe.
├── Command/
│   ├── BatchID           : STRING[32]  -- Unique batch execution identifier
│   ├── RecipeID          : STRING[32]  -- Master recipe identifier
│   ├── StepIndex         : UINT16      -- Active step index commanded by SCADA
│   ├── CommandCode       : UINT16      -- 1=LOAD, 2=START, 3=HOLD, 4=RESUME, 5=ABORT
│   ├── ChecksumSHA256    : STRING[64]  -- SHA-256 hash of parameter buffer
│   └── TriggerHandshake  : BOOL        -- Rising edge triggers PLC transfer
│
├── Parameters/
│   ├── Param01_Real      : REAL        -- Formula parameter slots
│   ├── Param02_Real      : REAL
│   ├── Param03_DInt      : DINT
│   ├── Param04_TimeSec   : UDINT
│   └── ... (up to 128 bounded slots)
│
├── Status/
│   ├── AckHandshake      : BOOL        -- PLC echoes true when transfer committed
│   ├── ExecutionState    : UINT16      -- 0=IDLE, 1=RUNNING, 2=HELD, 3=COMPLETE, 4=ERROR
│   ├── ActiveStepIndex   : UINT16      -- Currently executing step in PLC ladder
│   ├── StepElapsedSec    : UDINT       -- Timer elapsed for current phase
│   ├── FaultCode         : UINT32      -- Equipment interlock fault code
│   └── ParameterValid    : BOOL        -- True if PLC verified checksum matches
│
└── Telemetry/
    ├── ActualBatchWeight : REAL        -- Measured dosing weight
    ├── ActualPeakTemp    : REAL        -- Maximum reached process temperature
    └── ActualHoldTimeSec : UDINT       -- Exact hold duration confirmed by PLC
```

### 5.2 Atomic Handshake Protocol Sequence
The two-way exchange adheres to a non-blocking 4-phase synchronization handshake:

```
      SCADA / MythOS Recipe Engine                       OpenPLC Runtime v4
                   |                                             |
   (1) PREPARE:    |                                             |
       Serialize formula parameters                              |
       Compute SHA-256 checksum                                  |
                   |                                             |
   (2) WRITE BUS:  |                                             |
       Write all parameters to TagBus                            |
       Write ChecksumSHA256 & BatchID                            |
       Set Command.TriggerHandshake = TRUE                       |
                   |-------------------------------------------->| (Scan cycle begins)
                   |                                             | Read TriggerHandshake == TRUE
                   |                                             | Compute local checksum of slots
                   |                                             | Validate values within limits
                   |                                             | If OK:
                   |                                             |   Copy slots to Internal.RecipeBuf
                   |                                             |   Set Status.ParameterValid = TRUE
                   |                                             |   Set Status.AckHandshake = TRUE
                   |<--------------------------------------------|
   (3) VERIFY:     |                                             |
       Read Status.AckHandshake == TRUE                          |
       Assert Status.ParameterValid == TRUE                      |
       Set Command.TriggerHandshake = FALSE                      |
                   |-------------------------------------------->|
                   |                                             | Read TriggerHandshake == FALSE
                   |                                             | Set Status.AckHandshake = FALSE
                   |<--------------------------------------------|
   (4) START BATCH:|                                             |
       Set Command.CommandCode = 2 (START)                       |
       PLC begins Phase execution                                |
```

**Timeout & Abort Policy**:
- If `Status.AckHandshake` does not respond within $T_{\text{sync\_timeout}} = 2.0\,\text{seconds}$, the transaction aborts with `ERR_RECIPE_SYNC_TIMEOUT`.
- If `Status.ParameterValid == FALSE`, the transaction aborts with `ERR_RECIPE_CHECKSUM_MISMATCH` or `ERR_PARAM_LIMIT_EXCEEDED`. No motion occurs.

---

## 6. Recipe Lifecycle & Audit Trail (21 CFR Part 11)

A recipe transitions through strict administrative gates:

```
[ DRAFT ] ---> [ IN_REVIEW ] ---> [ APPROVED ] ---> [ RELEASED ] ---> [ ARCHIVED ]
                   |                     |
                   +-- Reject to DRAFT --+
```

1. **DRAFT**: Editable only by users with `Engineer` or `Admin` role.
2. **IN_REVIEW**: Formula frozen. Awaiting electronic signature from designated Process Authority.
3. **APPROVED**: Cryptographically signed. Ready for scheduling.
4. **RELEASED (In-Production)**: Actively loaded into batch queues. Cannot be modified. Any parameter update requires incrementing `version_minor` or `version_major`.
5. **ARCHIVED**: Superseded by newer revisions. Read-only historical retention.

**Electronic Signatures**:
Per 21 CFR Part 11, each recipe release transition requires dual credentials (Username + Cryptographic Token) and records a non-repudiable audit event containing:
- `user_id`, `role`, `timestamp_utc`, `recipe_id`, `version`, `reason_for_change`, and `sha256_digest`.

---

## 7. Concrete Rust Model Implementation

```rust
//! mythos_recipe_engine::model
//! ANSI/ISA-88 compliant batch recipe and synchronization engine.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecipeLifecycleState {
    Draft,
    InReview,
    Approved,
    Released,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    Float(f64),
    Integer(i64),
    Boolean(bool),
    DurationSeconds(u64),
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterConstraint {
    pub name: String,
    pub description: String,
    pub unit: String,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: ParameterValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeStep {
    pub step_index: u16,
    pub step_name: String,
    pub phase_identifier: String,
    pub step_timeout_seconds: u32,
    pub step_parameters: HashMap<String, ParameterValue>,
    pub transition_condition: StepTransitionCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepTransitionCondition {
    TimerComplete,
    ProcessTagThreshold { tag_name: String, target_value: f64, operator: String },
    OperatorConfirmationPrompt { prompt_text: String },
    PlcPhaseCompleteSignal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterRecipe {
    pub recipe_id: String,
    pub revision: String,
    pub title: String,
    pub author_user_id: String,
    pub state: RecipeLifecycleState,
    pub created_at_utc: String,
    pub formula: HashMap<String, ParameterValue>,
    pub steps: Vec<RecipeStep>,
    pub digital_signature_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchExecutionInstance {
    pub batch_id: String,
    pub recipe_id: String,
    pub recipe_revision: String,
    pub equipment_unit_id: String,
    pub started_by_user_id: String,
    pub start_timestamp_ns: u64,
    pub active_step_index: u16,
    pub packml_state: String,
    pub status: BatchRunStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatchRunStatus {
    PendingSync,
    Synchronizing,
    Running,
    Held,
    Completing,
    Completed,
    Aborted,
    Failed,
}

impl MasterRecipe {
    /// Validates all formula entries against strict boundary constraints.
    pub fn validate_constraints(&self, constraints: &[ParameterConstraint]) -> Result<(), String> {
        for c in constraints {
            if let Some(val) = self.formula.get(&c.name) {
                if let ParameterValue::Float(num) = val {
                    if let Some(min) = c.min_value {
                        if *num < min {
                            return Err(format!("Parameter {} value {} < min {}", c.name, num, min));
                        }
                    }
                    if let Some(max) = c.max_value {
                        if *num > max {
                            return Err(format!("Parameter {} value {} > max {}", c.name, num, max));
                        }
                    }
                }
            } else {
                return Err(format!("Missing mandatory parameter: {}", c.name));
            }
        }
        Ok(())
    }

    /// Calculates deterministic SHA-256 fingerprint for recipe verification.
    pub fn compute_content_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.recipe_id.as_bytes());
        hasher.update(self.revision.as_bytes());
        for (k, v) in &self.formula {
            hasher.update(k.as_bytes());
            hasher.update(format!("{:?}", v).as_bytes());
        }
        for s in &self.steps {
            hasher.update(s.step_name.as_bytes());
            hasher.update(s.phase_identifier.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}
```

---

## 8. Verification & Acceptance Criteria
1. **Zero Corrupted Writes**: A simulated communication break during step parameter loading must immediately abort the transfer, restore previous safe parameters, and raise `ERR_RECIPE_SYNC_ABORT`.
2. **Deterministic Step Sequence**: Sequence stepping occurs strictly when transition criteria are satisfied; unexpected equipment alarms automatically pause the recipe in PackML `HELD` state.
3. **Audit Completeness**: All batch runs produce an immutable log tying Batch ID, Recipe Revision, Step Trajectories, and Operator Interventions for 10-year regulatory retention.
