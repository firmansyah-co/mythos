//! ANSI/ISA-18.2 Alarm Subsystem Module.
//!
//! Provides finite state machine evaluation, First-Out sequence latching,
//! deadband hysteresis, filter timers, and chatter suppression.

pub mod first_out;
pub mod fsm;
pub mod hysteresis;

pub use first_out::{
    get_monotonic_raw_ns, FirstOutCaptureResult, FirstOutGroupStatus, FirstOutManager,
};
pub use fsm::{
    AlarmDefinition, AlarmEventTrigger, AlarmRuntimeState, AlarmSeverity, AlarmState,
    AlarmStateTransitionEvent, AlarmTripType,
};
pub use hysteresis::{AlarmFilterTimer, ChatterDetector, DeadbandConfig};
