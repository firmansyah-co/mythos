use mythos_shm::slot::{QualityCode, TagSlot, TagValueRaw};
use mythos_strucpp::ffi::{LocatedVarDescriptor, LocatedVarRaw};
use mythos_strucpp::quality_gate::{DeviceExecutionMode, QualityGate};
use mythos_strucpp::sync::TagSynchronizer;
use std::ffi::c_void;

#[test]
fn test_tagbus_sync_and_isolation_sys_req_024_025() {
    let mut synchronizer = TagSynchronizer::new();
    let quality_gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware);

    let mut in_memory: u8 = 0;
    let mut out_memory: u8 = 0;

    let in_raw = LocatedVarRaw {
        area: 0, // Input
        size: 0, // Bit
        byte_index: 0,
        bit_index: 0,
        _reserved: [0; 3],
        pointer: &mut in_memory as *mut u8 as *mut c_void,
    };

    let out_raw = LocatedVarRaw {
        area: 1, // Output
        size: 0, // Bit
        byte_index: 0,
        bit_index: 1,
        _reserved: [0; 3],
        pointer: &mut out_memory as *mut u8 as *mut c_void,
    };

    let in_desc = LocatedVarDescriptor::from_raw(&in_raw).unwrap();
    let out_desc = LocatedVarDescriptor::from_raw(&out_raw).unwrap();

    // 1. SYS-REQ-025: Prohibit non-external / non-internal URIs
    let invalid_reg = synchronizer.register_binding("bad_prefix.tag", 0, in_desc.clone());
    assert!(invalid_reg.is_err());

    // Register valid bindings
    synchronizer.register_binding("external.Line1.Sensor1.pv", 0, in_desc).unwrap();
    synchronizer.register_binding("external.Line1.Actuator1.sp", 1, out_desc).unwrap();

    // Setup dummy TagBus slots
    let slot0 = TagSlot {
        value: TagValueRaw::from_bool(true),
        quality: QualityCode::GOOD,
        ..Default::default()
    };

    let slot1 = TagSlot {
        value: TagValueRaw::from_bool(false),
        quality: QualityCode::GOOD,
        ..Default::default()
    };

    let mut slots = [slot0, slot1];

    // Scan Start: Sync inputs -> %IX
    unsafe {
        synchronizer.sync_inputs(&slots, &quality_gate);
    }
    assert_eq!(in_memory, 1); // Value copied into PLC located input

    // Simulate PLC logic execution writing output
    unsafe { std::ptr::write_volatile(&mut out_memory, 1); }

    // Scan End: Sync outputs -> TagBus %QX
    unsafe {
        synchronizer.sync_outputs(&mut slots, &quality_gate);
    }
    unsafe {
        assert!(slots[1].value.boolean);
        assert_eq!(slots[1].quality, QualityCode::GOOD);
    }
}

#[test]
fn test_quality_gate_comm_loss_latching_sys_req_096() {
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware);
    let tag = "external.PumpStation.P101.flow";

    // In PhysicalHardware mode, forcing must be strictly rejected
    let force_res = gate.validate_forcing_permitted(tag);
    assert!(force_res.is_err());

    // Record comm loss -> immediately latches BAD_COMM_FAILURE
    let code = gate.record_comm_loss(tag);
    assert_eq!(code, QualityCode::BAD_COMM_FAILURE);
    assert_eq!(gate.get_latched_quality(tag), Some(QualityCode::BAD_COMM_FAILURE));

    // Clear alarm upon reconnect
    gate.clear_comm_loss(tag);
    assert_eq!(gate.get_latched_quality(tag), None);

    // Switch to ManualSimulation mode
    gate.set_mode(DeviceExecutionMode::ManualSimulation);
    assert!(gate.is_simulation());

    // In ManualSimulation mode, forcing is unlocked
    let sim_force_res = gate.validate_forcing_permitted(tag);
    assert!(sim_force_res.is_ok());
}
