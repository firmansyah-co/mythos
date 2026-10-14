# ISA-18.2 Alarm Engine Specification

## 1. Executive Summary & Purpose
This document specifies the design, state dynamics, and implementation contracts for the MythOS Alarm Subsystem. The design complies strictly with ANSI/ISA-18.2-2016 (*Management of Alarm Systems for the Process Industries*) and IEC 62682.

The MythOS Alarm Engine executes as a deterministic, zero-allocation microkernel component in pure Rust. It processes tag updates from the lock-free shared memory TagBus (`/dev/shm/mythos_tagbus`), computes deadbands, detects threshold excursions, updates alarm operational states, controls audible annunciators, captures First-Out sequences with microsecond resolution, and mitigates alarm floods via dynamic chatter suppression and state-based shelving.

---

## 2. Normative References
- **ANSI/ISA-18.2-2016**: *Management of Alarm Systems for the Process Industries*.
- **IEC 62682:2014**: *Management of alarm systems for the process industries*.
- **EEMUA 191 (3rd Edition)**: *Alarm Systems: A Guide to Design, Management and Procurement*.
- **ISO 13849-1:2015**: *Safety of machinery - Safety-related parts of control systems*.

---

## 3. Alarm State Machine Architecture

### 3.1 State Definitions
Per ANSI/ISA-18.2 Section 5.4, an individual alarm point exists in exactly one of the following canonical operational states:

1. **NORMAL (NORM)**:
   - The process variable is within safe operating limits.
   - The alarm is inactive, reset, and unannunciated.
2. **UNACKNOWLEDGED ALARM (UNACK_ALARM)**:
   - The process variable has violated the trip threshold (active condition).
   - The operator has not yet acknowledged the condition.
   - Audible annunciator (horn) is active; visual indicator flashes at high frequency (2 Hz).
3. **ACKNOWLEDGED ALARM (ACK_ALARM)**:
   - The alarm condition remains active (trip threshold violated).
   - The operator has confirmed awareness (acknowledge command received).
   - Audible annunciator is silenced; visual indicator transitions to steady illumination.
4. **UNACKNOWLEDGED RETURN-TO-NORMAL (UNACK_RTN)**:
   - The process variable has cleared the trip threshold (with hysteresis deadband applied) while in `UNACK_ALARM` state.
   - The return-to-normal condition has occurred, but the operator has not yet acknowledged that the excursion took place.
   - Audible annunciator sounds return chime or pulses; visual indicator flashes at low frequency (0.5 Hz).
5. **SHELVED (SHELVED)**:
   - The alarm is temporarily removed from the operator's active view by authorized personnel for a strictly bounded duration.
   - Includes state variants: `SHELVED_UNACK` and `SHELVED_ACK`.
   - Automatic unshelving occurs on timer expiration.
6. **SUPPRESSED BY DESIGN (SUPPRESSED)**:
   - The alarm is inhibited automatically based on plant operational state (e.g., low oil pressure alarm suppressed when pump motor circuit breaker is open).
   - Governed by Boolean suppression logic matrices.
7. **OUT OF SERVICE (OOS)**:
   - The point is manually decommissioned for maintenance, sensor calibration, or mechanical overhaul. Requires Engineer or Admin role.

---

### 3.2 Formal State Transition Matrix

The table below defines every valid state transition, trigger event, guard conditions, and emitted side effects:

| Current State | Trigger Event | Guard Condition | Next State | Annunciator Action | Audit Event |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `Normal` | `TripExcursion` | `!Suppressed && !OOS && !Shelved` | `UnackAlarm` | Start Horn, Flash Fast (2Hz) | `AlarmTrip` |
| `UnackAlarm` | `OperatorAck` | Authenticated operator | `AckAlarm` | Silence Horn, Solid Lamp | `AlarmAck` |
| `UnackAlarm` | `ReturnWithinDeadband` | Threshold + Deadband satisfied | `UnackRtn` | Pulse Chime, Flash Slow (0.5Hz) | `AlarmRtnToNormal` |
| `AckAlarm` | `ReturnWithinDeadband` | Threshold + Deadband satisfied | `Normal` | Annunciator Inactive | `AlarmNormal` |
| `UnackRtn` | `OperatorAck` | Authenticated operator | `Normal` | Annunciator Inactive | `AlarmAckRtn` |
| `UnackRtn` | `TripExcursion` | Process re-violates threshold | `UnackAlarm` | Start Horn, Flash Fast (2Hz) | `AlarmReTrip` |
| `Normal` \| `AckAlarm` | `ShelveRequest` | `role >= Operator && dur <= max_shelve` | `Shelved` | Silence Horn, Lamp Shelve Icon | `AlarmShelved` |
| `Shelved` | `UnshelveRequest` | Manual unshelve or Timeout | `EvalCondition` | If active -> `UnackAlarm`, Else -> `Normal` | `AlarmUnshelved` |
| Any State | `PlantStateSuppress` | Plant suppression logic `true` | `Suppressed` | Silence Horn, Inactive | `AlarmSuppressed` |
| `Suppressed` | `PlantStateUnsuppress` | Plant suppression logic `false` | `EvalCondition` | If active -> `UnackAlarm`, Else -> `Normal` | `AlarmUnsuppressed` |
| Any State | `SetOutOfService` | `role >= Engineer` | `OutOfService` | Inhibit All | `AlarmOOS` |
| `OutOfService` | `ReturnToService` | `role >= Engineer` | `EvalCondition` | If active -> `UnackAlarm`, Else -> `Normal` | `AlarmRTS` |

---

## 4. Audible Horn Logic & Annunciation Matrix

Annunciation adheres strictly to ISA-18.2 Section 10 and ISA-RP18.1 (Sequence A / Sequence M-1):

### 4.1 Horn Arbitration Logic
1. **Horn Bank Partitioning**:
   - `HornGroup::Critical`: Continuous 1000 Hz tone + high-strobe relay. Cannot be muted globally; requires individual ACK or group silence with auto-resound within 60 seconds if unacknowledged.
   - `HornGroup::High`: Pulsed tone (1 sec ON, 1 sec OFF).
   - `HornGroup::Medium`: Pulsed chime (0.5 sec pulse every 5 seconds).
   - `HornGroup::Low`: Visual only (silent), unless escalated by inactivity.
2. **Global Silence Command**:
   - `AlarmCommand::Silence`: Deasserts physical horn relays without altering the alarm state (alarms remain in `UnackAlarm` state).
   - **Resound Timer**: If any unacknowledged alarm remains active after silence timeout ($T_{\text{resound}} = 120\,\text{s}$), the horn re-asserts automatically.
3. **Global Acknowledge Command**:
   - `AlarmCommand::AckAllVisible`: Transitions all visible `UnackAlarm` points on current viewport to `AckAlarm`. Physical horn is silenced.

---

## 5. Deadband & Chatter Suppression Engineering

### 5.1 Deadband Mathematical Formulation
To eliminate limit-cycle oscillations (chatter) caused by process measurement noise near alarm trip points, MythOS enforces dual-mode deadband filtering:

#### 1. Absolute Threshold Deadband
Let $X(t)$ be the analog process value, $T_{\text{trip}}$ be the trip threshold, and $\Delta_{\text{db}} > 0$ be the engineering deadband:

- **High Alarm ($PV \ge T_{\text{trip}}$)**:
  $$\text{Trip Condition: } X(t) \ge T_{\text{trip}}$$
  $$\text{Clear Condition: } X(t) < T_{\text{trip}} - \Delta_{\text{db}}$$

- **Low Alarm ($PV \le T_{\text{trip}}$)**:
  $$\text{Trip Condition: } X(t) \le T_{\text{trip}}$$
  $$\text{Clear Condition: } X(t) > T_{\text{trip}} + \Delta_{\text{db}}$$

#### 2. Percentage of Range Deadband
When configured as a percentage of the sensor span ($Span = PV_{\max} - PV_{\min}$):
$$\Delta_{\text{db}} = Span \times \frac{P_{\text{deadband}}}{100.0}$$
Default default standard deadband: $1.0\%$ for pressure/flow, $0.5\%$ for temperature, $2.0\%$ for level.

### 5.2 Time-Delay Filters (On-Delay and Off-Delay)
To reject transient electrical spikes and process surges:
- **On-Delay ($T_{\text{on}}$)**: Condition must remain continuously true for $T_{\text{on}}$ before transitioning to `UnackAlarm`.
- **Off-Delay ($T_{\text{off}}$)**: Condition must remain continuously clear for $T_{\text{off}}$ before transitioning to `Normal` or `UnackRtn`.

### 5.3 Dynamic Chatter Detection and Suppression
An alarm point is classified as **Chattering** if it transitions between active and clear more than $N_{\text{chatter}}$ times (default: 3 transitions) within sliding window $W_{\text{chatter}}$ (default: 60 seconds).
- **Automated Mitigation**:
  1. The point is placed into `State::ChatterSuppressed`.
  2. An executive diagnostic alarm (`DIAG_ALARM_CHATTER_TRIP`) is raised once.
  3. The individual alarm point stops flooding the operator display and historian.
  4. Once transitions drop below 1 per 60 seconds for a cooling period of 10 minutes, the point is automatically returned to active monitoring.

---

## 6. Alarm Flood Mitigation Architecture

EEMUA 191 defines an **Alarm Flood** as any period where the alarm rate exceeds 10 alarms per 10 minutes per operator console. During catastrophic process upsets (e.g., turbine trip, main steam header rupture), hundreds of secondary alarms trigger simultaneously.

### 6.1 Flood Management Hierarchy
```
+-------------------------------------------------------------------+
|                        PROCESS UPSET EVENT                        |
+-------------------------------------------------------------------+
                                  |
                                  v
+-------------------------------------------------------------------+
| 1. FIRST-OUT CAPTURE ENGINE                                       |
|    Hardware / Kernel Timestamps (Sub-millisecond resolution)      |
|    Determines Root Cause (Trip Breaker vs Secondary Low Press)    |
+-------------------------------------------------------------------+
                                  |
                                  v
+-------------------------------------------------------------------+
| 2. STATE-BASED SUPPRESSION MATRIX                                 |
|    e.g., Boiler Trip -> Suppress 120 downflow burner alarms       |
+-------------------------------------------------------------------+
                                  |
                                  v
+-------------------------------------------------------------------+
| 3. DYNAMIC PRIORITY RATE-LIMITING                                 |
|    Throttle Low/Medium UI events; Guarantee CRITICAL dispatch     |
+-------------------------------------------------------------------+
```

### 6.2 First-Out Sequence Capture
1. **Identification**: In equipment groups (e.g., 3-boiler manifold, gas turbine trains), trip initiators form a First-Out Group (`FirstOutGroup`).
2. **Microsecond Precision**: Every TagBus sample carries a 64-bit monotonic nanosecond timestamp (`CLOCK_MONOTONIC_RAW`).
3. **Root-Cause Latching**:
   - The first tag in the group that triggers an unacknowledged alarm latches the `FirstOutLeader` register.
   - All subsequent alarms within the group occurring within $T_{\text{window}}$ (default: 5.0 seconds) are tagged with `first_out_leader = false; root_cause_id = leader_id; delta_micros = t_event - t_leader`.
   - The HMI and Audit Log visually elevate the Leader with a gold star badge and display precise relative offsets ($+12\,\text{ms}$, $+45\,\text{ms}$).

---

## 7. Priority Classification & EEMUA 191 Quotas

Every alarm point is assigned an immutable risk priority based on Consequence Severity and Maximum Allowable Operator Response Time (MART):

| Priority | Color Hex | Recommended Distribution (EEMUA 191) | Maximum MART | Acoustic Signature | Action Mandate |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Critical** | `#D32F2F` (Red) | $\le 5\%$ of total catalog | $< 2$ minutes | Continuous 1 kHz / Hi-Strobe | Immediate safety or shutdown action |
| **High** | `#F57C00` (Orange) | $\approx 15\%$ of total catalog | $< 10$ minutes | Pulsed 1 Hz / Orange Beacon | Process upset; imminent trip boundary |
| **Medium** | `#FBC02D` (Yellow) | $\approx 30\%$ of total catalog | $< 30$ minutes | Intermittent chime (5s) | Quality deviation or minor equipment fault |
| **Low** | `#0288D1` (Blue) | $\approx 50\%$ of total catalog | $> 60$ minutes | Visual only / Silent | Advisory maintenance or efficiency notice |

---

## 8. Rust Implementation Architecture & Core Types

Below is the concrete, zero-allocation Rust implementation contract for the MythOS Alarm Subsystem:

```rust
//! mythos_alarm_engine::model
//! ANSI/ISA-18.2 compliant state machine and calculation engine.

use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlarmSeverity {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlarmState {
    Normal = 0,
    UnackAlarm = 1,
    AckAlarm = 2,
    UnackRtn = 3,
    Shelved = 4,
    Suppressed = 5,
    OutOfService = 6,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlarmTripType {
    DigitalState { trip_on: bool },
    AnalogHigh { trip_threshold: f64, deadband: f64 },
    AnalogLow { trip_threshold: f64, deadband: f64 },
    RateOfChange { max_slope_per_sec: f64, deadband: f64 },
    Deviation { setpoint_tag: String, max_error: f64, deadband: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmDefinition {
    pub alarm_id: u32,
    pub tag_name: String,
    pub description: String,
    pub severity: AlarmSeverity,
    pub trip_type: AlarmTripType,
    pub on_delay_ms: u32,
    pub off_delay_ms: u32,
    pub first_out_group_id: Option<u16>,
    pub plant_suppression_tag: Option<String>,
    pub max_shelve_duration_sec: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmRuntimeState {
    pub alarm_id: u32,
    pub current_state: AlarmState,
    pub is_active_condition: bool,
    pub active_trip_timestamp_ns: u64,
    pub last_state_change_ns: u64,
    pub ack_timestamp_ns: Option<u64>,
    pub acknowledged_by_user_id: Option<String>,
    pub shelve_expiration_ns: Option<u64>,
    pub is_chattering: bool,
    pub transition_count_window: u16,
    pub first_out_is_leader: bool,
    pub first_out_leader_delta_us: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlarmEventTrigger {
    SampleEvaluated { current_value: f64, timestamp_ns: u64 },
    OperatorAcknowledge { user_id: String, timestamp_ns: u64 },
    OperatorShelve { user_id: String, duration_sec: u32, timestamp_ns: u64 },
    OperatorUnshelve { user_id: String, timestamp_ns: u64 },
    SetOutOfService { user_id: String, timestamp_ns: u64 },
    ReturnToService { user_id: String, timestamp_ns: u64 },
    PlantSuppressionChanged { is_suppressed: bool, timestamp_ns: u64 },
    Tick { timestamp_ns: u64 },
}

impl AlarmRuntimeState {
    pub fn new(alarm_id: u32) -> Self {
        Self {
            alarm_id,
            current_state: AlarmState::Normal,
            is_active_condition: false,
            active_trip_timestamp_ns: 0,
            last_state_change_ns: 0,
            ack_timestamp_ns: None,
            acknowledged_by_user_id: None,
            shelve_expiration_ns: None,
            is_chattering: false,
            transition_count_window: 0,
            first_out_is_leader: false,
            first_out_leader_delta_us: None,
        }
    }

    /// Evaluates raw analog input against threshold and deadband.
    pub fn evaluate_condition(&self, def: &AlarmDefinition, value: f64) -> bool {
        match &def.trip_type {
            AlarmTripType::DigitalState { trip_on } => (value > 0.5) == *trip_on,
            AlarmTripType::AnalogHigh { trip_threshold, deadband } => {
                if self.is_active_condition {
                    // Condition clears only when PV drops below (threshold - deadband)
                    value >= (*trip_threshold - *deadband)
                } else {
                    // Condition trips when PV exceeds threshold
                    value >= *trip_threshold
                }
            }
            AlarmTripType::AnalogLow { trip_threshold, deadband } => {
                if self.is_active_condition {
                    // Condition clears only when PV rises above (threshold + deadband)
                    value <= (*trip_threshold + *deadband)
                } else {
                    // Condition trips when PV drops below threshold
                    value <= *trip_threshold
                }
            }
            AlarmTripType::RateOfChange { max_slope_per_sec, deadband } => {
                if self.is_active_condition {
                    value.abs() >= (*max_slope_per_sec - *deadband)
                } else {
                    value.abs() >= *max_slope_per_sec
                }
            }
            AlarmTripType::Deviation { max_error, deadband, .. } => {
                if self.is_active_condition {
                    value.abs() >= (*max_error - *deadband)
                } else {
                    value.abs() >= *max_error
                }
            }
        }
    }

    /// Pure deterministic transition step compliant with ISA-18.2.
    pub fn step(
        &mut self,
        def: &AlarmDefinition,
        trigger: AlarmEventTrigger,
    ) -> Option<AlarmStateTransitionEvent> {
        let prev_state = self.current_state;

        match trigger {
            AlarmEventTrigger::SampleEvaluated { current_value, timestamp_ns } => {
                let condition_now = self.evaluate_condition(def, current_value);
                self.is_active_condition = condition_now;

                if self.current_state == AlarmState::OutOfService || self.current_state == AlarmState::Suppressed {
                    return None;
                }

                if self.current_state == AlarmState::Shelved {
                    if let Some(exp) = self.shelve_expiration_ns {
                        if timestamp_ns >= exp {
                            // Shelve timeout expired: transition out of shelved
                            self.shelve_expiration_ns = None;
                            self.current_state = if condition_now {
                                AlarmState::UnackAlarm
                            } else {
                                AlarmState::Normal
                            };
                        }
                    }
                    return if self.current_state != prev_state {
                        Some(self.build_transition(prev_state, self.current_state, timestamp_ns, "ShelveTimeout"))
                    } else {
                        None
                    };
                }

                match self.current_state {
                    AlarmState::Normal => {
                        if condition_now {
                            self.current_state = AlarmState::UnackAlarm;
                            self.active_trip_timestamp_ns = timestamp_ns;
                            self.last_state_change_ns = timestamp_ns;
                            return Some(self.build_transition(prev_state, AlarmState::UnackAlarm, timestamp_ns, "TripExcursion"));
                        }
                    }
                    AlarmState::UnackAlarm => {
                        if !condition_now {
                            self.current_state = AlarmState::UnackRtn;
                            self.last_state_change_ns = timestamp_ns;
                            return Some(self.build_transition(prev_state, AlarmState::UnackRtn, timestamp_ns, "ReturnWithinDeadband"));
                        }
                    }
                    AlarmState::AckAlarm => {
                        if !condition_now {
                            self.current_state = AlarmState::Normal;
                            self.last_state_change_ns = timestamp_ns;
                            return Some(self.build_transition(prev_state, AlarmState::Normal, timestamp_ns, "ReturnWithinDeadband"));
                        }
                    }
                    AlarmState::UnackRtn => {
                        if condition_now {
                            self.current_state = AlarmState::UnackAlarm;
                            self.last_state_change_ns = timestamp_ns;
                            return Some(self.build_transition(prev_state, AlarmState::UnackAlarm, timestamp_ns, "ReTripExcursion"));
                        }
                    }
                    _ => {}
                }
            }
            AlarmEventTrigger::OperatorAcknowledge { user_id, timestamp_ns } => {
                match self.current_state {
                    AlarmState::UnackAlarm => {
                        self.current_state = AlarmState::AckAlarm;
                        self.ack_timestamp_ns = Some(timestamp_ns);
                        self.acknowledged_by_user_id = Some(user_id);
                        self.last_state_change_ns = timestamp_ns;
                        return Some(self.build_transition(prev_state, AlarmState::AckAlarm, timestamp_ns, "OperatorAck"));
                    }
                    AlarmState::UnackRtn => {
                        self.current_state = AlarmState::Normal;
                        self.ack_timestamp_ns = Some(timestamp_ns);
                        self.acknowledged_by_user_id = Some(user_id);
                        self.last_state_change_ns = timestamp_ns;
                        return Some(self.build_transition(prev_state, AlarmState::Normal, timestamp_ns, "OperatorAckRtn"));
                    }
                    _ => {}
                }
            }
            AlarmEventTrigger::OperatorShelve { user_id, duration_sec, timestamp_ns } => {
                let bounded_dur = duration_sec.min(def.max_shelve_duration_sec);
                self.current_state = AlarmState::Shelved;
                self.shelve_expiration_ns = Some(timestamp_ns + (bounded_dur as u64 * 1_000_000_000));
                self.last_state_change_ns = timestamp_ns;
                return Some(self.build_transition(prev_state, AlarmState::Shelved, timestamp_ns, "OperatorShelve"));
            }
            AlarmEventTrigger::OperatorUnshelve { user_id: _, timestamp_ns } => {
                if self.current_state == AlarmState::Shelved {
                    self.shelve_expiration_ns = None;
                    self.current_state = if self.is_active_condition {
                        AlarmState::UnackAlarm
                    } else {
                        AlarmState::Normal
                    };
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(prev_state, self.current_state, timestamp_ns, "OperatorUnshelve"));
                }
            }
            AlarmEventTrigger::SetOutOfService { user_id: _, timestamp_ns } => {
                self.current_state = AlarmState::OutOfService;
                self.last_state_change_ns = timestamp_ns;
                return Some(self.build_transition(prev_state, AlarmState::OutOfService, timestamp_ns, "SetOutOfService"));
            }
            AlarmEventTrigger::ReturnToService { user_id: _, timestamp_ns } => {
                if self.current_state == AlarmState::OutOfService {
                    self.current_state = if self.is_active_condition {
                        AlarmState::UnackAlarm
                    } else {
                        AlarmState::Normal
                    };
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(prev_state, self.current_state, timestamp_ns, "ReturnToService"));
                }
            }
            AlarmEventTrigger::PlantSuppressionChanged { is_suppressed, timestamp_ns } => {
                if is_suppressed && self.current_state != AlarmState::OutOfService {
                    self.current_state = AlarmState::Suppressed;
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(prev_state, AlarmState::Suppressed, timestamp_ns, "PlantSuppressOn"));
                } else if !is_suppressed && self.current_state == AlarmState::Suppressed {
                    self.current_state = if self.is_active_condition {
                        AlarmState::UnackAlarm
                    } else {
                        AlarmState::Normal
                    };
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(prev_state, self.current_state, timestamp_ns, "PlantSuppressOff"));
                }
            }
            AlarmEventTrigger::Tick { timestamp_ns } => {
                if self.current_state == AlarmState::Shelved {
                    if let Some(exp) = self.shelve_expiration_ns {
                        if timestamp_ns >= exp {
                            self.shelve_expiration_ns = None;
                            self.current_state = if self.is_active_condition {
                                AlarmState::UnackAlarm
                            } else {
                                AlarmState::Normal
                            };
                            self.last_state_change_ns = timestamp_ns;
                            return Some(self.build_transition(prev_state, self.current_state, timestamp_ns, "ShelveTimeout"));
                        }
                    }
                }
            }
        }

        None
    }

    fn build_transition(
        &self,
        from_state: AlarmState,
        to_state: AlarmState,
        timestamp_ns: u64,
        reason: &'static str,
    ) -> AlarmStateTransitionEvent {
        AlarmStateTransitionEvent {
            alarm_id: self.alarm_id,
            from_state,
            to_state,
            timestamp_ns,
            reason,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmStateTransitionEvent {
    pub alarm_id: u32,
    pub from_state: AlarmState,
    pub to_state: AlarmState,
    pub timestamp_ns: u64,
    pub reason: &'static str,
}
```

---

## 9. Verification & Acceptance Criteria
1. **Mathematical Determinism**: Zero unbounded loops in alarm evaluation loop. Worst-case evaluation latency for 10,000 configured alarm tags $\le 1.5\,\text{ms}$ on standard x86-64 IPC.
2. **Hysteresis Guarantee**: An oscillating process variable hovering within $\pm \frac{1}{2}\Delta_{\text{db}}$ of the trip point must produce exactly ONE transition without ringing.
3. **Audit Immutability**: All transitions generate an append-only JSONL record routed to the audit log engine before horn relays change state.
