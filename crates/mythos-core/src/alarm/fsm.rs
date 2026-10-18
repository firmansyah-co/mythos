//! ANSI/ISA-18.2 compliant alarm state machine and evaluation engine.

use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlarmSeverity {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl std::fmt::Display for AlarmSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
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

impl std::fmt::Display for AlarmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "NORMAL"),
            Self::UnackAlarm => write!(f, "UNACK_ALARM"),
            Self::AckAlarm => write!(f, "ACK_ALARM"),
            Self::UnackRtn => write!(f, "UNACK_RTN"),
            Self::Shelved => write!(f, "SHELVED"),
            Self::Suppressed => write!(f, "SUPPRESSED"),
            Self::OutOfService => write!(f, "OUT_OF_SERVICE"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlarmTripType {
    DigitalState {
        trip_on: bool,
    },
    AnalogHigh {
        trip_threshold: f64,
        deadband: f64,
    },
    AnalogLow {
        trip_threshold: f64,
        deadband: f64,
    },
    RateOfChange {
        max_slope_per_sec: f64,
        deadband: f64,
    },
    Deviation {
        setpoint_tag: String,
        max_error: f64,
        deadband: f64,
    },
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
    SampleEvaluated {
        current_value: f64,
        timestamp_ns: u64,
    },
    OperatorAcknowledge {
        user_id: String,
        timestamp_ns: u64,
    },
    OperatorShelve {
        user_id: String,
        duration_sec: u32,
        timestamp_ns: u64,
    },
    OperatorUnshelve {
        user_id: String,
        timestamp_ns: u64,
    },
    SetOutOfService {
        user_id: String,
        timestamp_ns: u64,
    },
    ReturnToService {
        user_id: String,
        timestamp_ns: u64,
    },
    PlantSuppressionChanged {
        is_suppressed: bool,
        timestamp_ns: u64,
    },
    Tick {
        timestamp_ns: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlarmStateTransitionEvent {
    pub alarm_id: u32,
    pub from_state: AlarmState,
    pub to_state: AlarmState,
    pub timestamp_ns: u64,
    pub reason: &'static str,
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

    /// Evaluates raw analog or digital input against threshold and deadband.
    pub fn evaluate_condition(&self, def: &AlarmDefinition, value: f64) -> bool {
        match &def.trip_type {
            AlarmTripType::DigitalState { trip_on } => (value > 0.5) == *trip_on,
            AlarmTripType::AnalogHigh {
                trip_threshold,
                deadband,
            } => {
                if self.is_active_condition {
                    // Condition clears only when PV drops below (threshold - deadband)
                    value >= (*trip_threshold - *deadband)
                } else {
                    // Condition trips when PV exceeds threshold
                    value >= *trip_threshold
                }
            }
            AlarmTripType::AnalogLow {
                trip_threshold,
                deadband,
            } => {
                if self.is_active_condition {
                    // Condition clears only when PV rises above (threshold + deadband)
                    value <= (*trip_threshold + *deadband)
                } else {
                    // Condition trips when PV drops below threshold
                    value <= *trip_threshold
                }
            }
            AlarmTripType::RateOfChange {
                max_slope_per_sec,
                deadband,
            } => {
                if self.is_active_condition {
                    value.abs() >= (*max_slope_per_sec - *deadband)
                } else {
                    value.abs() >= *max_slope_per_sec
                }
            }
            AlarmTripType::Deviation {
                max_error,
                deadband,
                ..
            } => {
                if self.is_active_condition {
                    value.abs() >= (*max_error - *deadband)
                } else {
                    value.abs() >= *max_error
                }
            }
        }
    }

    /// Pure deterministic transition step compliant with ANSI/ISA-18.2.
    pub fn step(
        &mut self,
        def: &AlarmDefinition,
        trigger: AlarmEventTrigger,
    ) -> Option<AlarmStateTransitionEvent> {
        let prev_state = self.current_state;

        match trigger {
            AlarmEventTrigger::SampleEvaluated {
                current_value,
                timestamp_ns,
            } => {
                let condition_now = self.evaluate_condition(def, current_value);
                self.is_active_condition = condition_now;

                if self.current_state == AlarmState::OutOfService
                    || self.current_state == AlarmState::Suppressed
                {
                    return None;
                }

                if self.current_state == AlarmState::Shelved {
                    if let Some(exp) = self.shelve_expiration_ns {
                        if timestamp_ns >= exp {
                            self.shelve_expiration_ns = None;
                            self.current_state = if condition_now {
                                AlarmState::UnackAlarm
                            } else {
                                AlarmState::Normal
                            };
                            self.last_state_change_ns = timestamp_ns;
                            return Some(self.build_transition(
                                prev_state,
                                self.current_state,
                                timestamp_ns,
                                "ShelveTimeout",
                            ));
                        }
                    }
                    return None;
                }

                match self.current_state {
                    AlarmState::Normal if condition_now => {
                        self.current_state = AlarmState::UnackAlarm;
                        self.active_trip_timestamp_ns = timestamp_ns;
                        self.last_state_change_ns = timestamp_ns;
                        return Some(self.build_transition(
                            prev_state,
                            AlarmState::UnackAlarm,
                            timestamp_ns,
                            "TripExcursion",
                        ));
                    }
                    AlarmState::UnackAlarm if !condition_now => {
                        self.current_state = AlarmState::UnackRtn;
                        self.last_state_change_ns = timestamp_ns;
                        return Some(self.build_transition(
                            prev_state,
                            AlarmState::UnackRtn,
                            timestamp_ns,
                            "ReturnWithinDeadband",
                        ));
                    }
                    AlarmState::AckAlarm if !condition_now => {
                        self.current_state = AlarmState::Normal;
                        self.last_state_change_ns = timestamp_ns;
                        return Some(self.build_transition(
                            prev_state,
                            AlarmState::Normal,
                            timestamp_ns,
                            "ReturnWithinDeadband",
                        ));
                    }
                    AlarmState::UnackRtn if condition_now => {
                        self.current_state = AlarmState::UnackAlarm;
                        self.last_state_change_ns = timestamp_ns;
                        return Some(self.build_transition(
                            prev_state,
                            AlarmState::UnackAlarm,
                            timestamp_ns,
                            "ReTripExcursion",
                        ));
                    }
                    _ => {}
                }
            }
            AlarmEventTrigger::OperatorAcknowledge {
                user_id,
                timestamp_ns,
            } => match self.current_state {
                AlarmState::UnackAlarm => {
                    self.current_state = AlarmState::AckAlarm;
                    self.ack_timestamp_ns = Some(timestamp_ns);
                    self.acknowledged_by_user_id = Some(user_id);
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(
                        prev_state,
                        AlarmState::AckAlarm,
                        timestamp_ns,
                        "OperatorAck",
                    ));
                }
                AlarmState::UnackRtn => {
                    self.current_state = AlarmState::Normal;
                    self.ack_timestamp_ns = Some(timestamp_ns);
                    self.acknowledged_by_user_id = Some(user_id);
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(
                        prev_state,
                        AlarmState::Normal,
                        timestamp_ns,
                        "OperatorAckRtn",
                    ));
                }
                _ => {}
            },
            AlarmEventTrigger::OperatorShelve {
                user_id: _,
                duration_sec,
                timestamp_ns,
            } => {
                let bounded_dur = duration_sec.min(def.max_shelve_duration_sec);
                self.current_state = AlarmState::Shelved;
                self.shelve_expiration_ns =
                    Some(timestamp_ns + (bounded_dur as u64 * 1_000_000_000));
                self.last_state_change_ns = timestamp_ns;
                return Some(self.build_transition(
                    prev_state,
                    AlarmState::Shelved,
                    timestamp_ns,
                    "OperatorShelve",
                ));
            }
            AlarmEventTrigger::OperatorUnshelve {
                user_id: _,
                timestamp_ns,
            } => {
                if self.current_state == AlarmState::Shelved {
                    self.shelve_expiration_ns = None;
                    self.current_state = if self.is_active_condition {
                        AlarmState::UnackAlarm
                    } else {
                        AlarmState::Normal
                    };
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(
                        prev_state,
                        self.current_state,
                        timestamp_ns,
                        "OperatorUnshelve",
                    ));
                }
            }
            AlarmEventTrigger::SetOutOfService {
                user_id: _,
                timestamp_ns,
            } => {
                self.current_state = AlarmState::OutOfService;
                self.last_state_change_ns = timestamp_ns;
                return Some(self.build_transition(
                    prev_state,
                    AlarmState::OutOfService,
                    timestamp_ns,
                    "SetOutOfService",
                ));
            }
            AlarmEventTrigger::ReturnToService {
                user_id: _,
                timestamp_ns,
            } => {
                if self.current_state == AlarmState::OutOfService {
                    self.current_state = if self.is_active_condition {
                        AlarmState::UnackAlarm
                    } else {
                        AlarmState::Normal
                    };
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(
                        prev_state,
                        self.current_state,
                        timestamp_ns,
                        "ReturnToService",
                    ));
                }
            }
            AlarmEventTrigger::PlantSuppressionChanged {
                is_suppressed,
                timestamp_ns,
            } => {
                if is_suppressed && self.current_state != AlarmState::OutOfService {
                    self.current_state = AlarmState::Suppressed;
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(
                        prev_state,
                        AlarmState::Suppressed,
                        timestamp_ns,
                        "PlantSuppressOn",
                    ));
                } else if !is_suppressed && self.current_state == AlarmState::Suppressed {
                    self.current_state = if self.is_active_condition {
                        AlarmState::UnackAlarm
                    } else {
                        AlarmState::Normal
                    };
                    self.last_state_change_ns = timestamp_ns;
                    return Some(self.build_transition(
                        prev_state,
                        self.current_state,
                        timestamp_ns,
                        "PlantSuppressOff",
                    ));
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
                            return Some(self.build_transition(
                                prev_state,
                                self.current_state,
                                timestamp_ns,
                                "ShelveTimeout",
                            ));
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
