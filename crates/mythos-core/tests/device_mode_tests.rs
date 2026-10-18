//! Unit and integration tests for Per-Device Operational Mode Toggle & Zero-Silent-Fallback Guard (TSK-207).

use mythos_core::device::*;

#[test]
fn test_default_production_mode_is_physical() {
    let dev = DeviceModeController::new("p300-sensor");
    assert_eq!(dev.mode(), DeviceExecutionMode::PhysicalHardware);
    assert!(!dev.is_simulation());
}

#[test]
fn test_zero_silent_fallback_invariant_sys_req_096() {
    let dev = DeviceModeController::new("p300-sensor");

    // In PhysicalHardware mode, comm loss MUST return an error and NEVER fallback silently
    let comm_err = dev.handle_comm_failure("RS485 transceiver timeout after 150ms");
    assert!(comm_err.is_err());

    match comm_err.unwrap_err() {
        DeviceModeError::PhysicalCommunicationFailure { device_id, reason } => {
            assert_eq!(device_id, "p300-sensor");
            assert!(reason.contains("150ms"));
        }
        _ => panic!("Expected PhysicalCommunicationFailure error"),
    }

    // Verify state remained strictly in PhysicalHardware mode
    assert_eq!(dev.mode(), DeviceExecutionMode::PhysicalHardware);
}

#[test]
fn test_simulation_injection_disallowed_in_physical_mode() {
    let mut dev = DeviceModeController::new("p300-sensor");

    // Attempting to inject mock slider value in physical mode must fail
    let res = dev.inject_mock_value("PRESSURE", 4.25);
    assert!(res.is_err());
    assert_eq!(dev.get_mock_value("PRESSURE"), None);
}

#[test]
fn test_mode_transition_requires_audit_trail_and_enables_simulation() {
    let mut dev = DeviceModeController::new("p300-sensor");

    // Operator switches to ManualSimulation
    let event = dev.set_mode(
        DeviceExecutionMode::ManualSimulation,
        "operator-adrian",
        "Calibrating tank high pressure alarm setpoint",
    );

    assert_eq!(event.previous_mode, DeviceExecutionMode::PhysicalHardware);
    assert_eq!(event.new_mode, DeviceExecutionMode::ManualSimulation);
    assert_eq!(event.operator, "operator-adrian");
    assert!(dev.is_simulation());

    // In simulation mode, comm failure is non-fatal
    assert!(dev.handle_comm_failure("Cable disconnected").is_ok());

    // Mock injection is now permitted
    dev.inject_mock_value("PRESSURE", 7.82).unwrap();
    assert_eq!(dev.get_mock_value("PRESSURE"), Some(7.82));

    // Audit trail has recorded the event
    assert_eq!(dev.audit_trail().len(), 1);
}
