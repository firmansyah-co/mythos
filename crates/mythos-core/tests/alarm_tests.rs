use mythos_core::alarm::*;

#[test]
fn test_alarm_fsm_canonical_cycle_and_ack() {
    let def = AlarmDefinition {
        alarm_id: 101,
        tag_name: "external.Boiler01.SteamPressure".to_string(),
        description: "High Steam Pressure Trip".to_string(),
        severity: AlarmSeverity::Critical,
        trip_type: AlarmTripType::AnalogHigh {
            trip_threshold: 10.0,
            deadband: 0.5,
        },
        on_delay_ms: 0,
        off_delay_ms: 0,
        first_out_group_id: Some(1),
        plant_suppression_tag: None,
        max_shelve_duration_sec: 3600,
    };

    let mut state = AlarmRuntimeState::new(101);
    assert_eq!(state.current_state, AlarmState::Normal);

    // 1. PV below threshold -> Normal
    let ev1 = state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 9.8,
            timestamp_ns: 1_000_000_000,
        },
    );
    assert!(ev1.is_none());
    assert_eq!(state.current_state, AlarmState::Normal);

    // 2. PV exceeds threshold -> UnackAlarm
    let ev2 = state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 10.2,
            timestamp_ns: 1_000_100_000,
        },
    );
    assert!(ev2.is_some());
    assert_eq!(state.current_state, AlarmState::UnackAlarm);

    // 3. Operator acknowledges -> AckAlarm
    let ev3 = state.step(
        &def,
        AlarmEventTrigger::OperatorAcknowledge {
            user_id: "op_adrian".to_string(),
            timestamp_ns: 1_000_500_000,
        },
    );
    assert!(ev3.is_some());
    assert_eq!(state.current_state, AlarmState::AckAlarm);
    assert_eq!(state.acknowledged_by_user_id.as_deref(), Some("op_adrian"));

    // 4. PV returns within deadband (drops below threshold - deadband = 9.5) -> Normal
    // Test that 9.6 does NOT clear condition (within hysteresis)
    let ev4 = state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 9.6,
            timestamp_ns: 1_001_000_000,
        },
    );
    assert!(ev4.is_none());
    assert_eq!(state.current_state, AlarmState::AckAlarm);

    // Drops to 9.4 -> Clears to Normal
    let ev5 = state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 9.4,
            timestamp_ns: 1_001_100_000,
        },
    );
    assert!(ev5.is_some());
    assert_eq!(state.current_state, AlarmState::Normal);
}

#[test]
fn test_alarm_fsm_return_before_ack_and_retrip() {
    let def = AlarmDefinition {
        alarm_id: 102,
        tag_name: "external.Boiler01.Temp".to_string(),
        description: "High Temp".to_string(),
        severity: AlarmSeverity::High,
        trip_type: AlarmTripType::AnalogHigh {
            trip_threshold: 150.0,
            deadband: 2.0,
        },
        on_delay_ms: 0,
        off_delay_ms: 0,
        first_out_group_id: None,
        plant_suppression_tag: None,
        max_shelve_duration_sec: 1800,
    };

    let mut state = AlarmRuntimeState::new(102);

    // Trip -> UnackAlarm
    state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 151.0,
            timestamp_ns: 1_000_000_000,
        },
    );
    assert_eq!(state.current_state, AlarmState::UnackAlarm);

    // Returns to normal without operator ack -> UnackRtn
    let ev = state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 147.0,
            timestamp_ns: 1_001_000_000,
        },
    );
    assert!(ev.is_some());
    assert_eq!(state.current_state, AlarmState::UnackRtn);

    // Re-trip before operator ack -> back to UnackAlarm
    let ev_retrip = state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 152.0,
            timestamp_ns: 1_002_000_000,
        },
    );
    assert!(ev_retrip.is_some());
    assert_eq!(state.current_state, AlarmState::UnackAlarm);

    // Return to normal again -> UnackRtn
    state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 147.0,
            timestamp_ns: 1_003_000_000,
        },
    );
    assert_eq!(state.current_state, AlarmState::UnackRtn);

    // Operator Ack -> Normal
    let ev_ack = state.step(
        &def,
        AlarmEventTrigger::OperatorAcknowledge {
            user_id: "op_dewabagus".to_string(),
            timestamp_ns: 1_004_000_000,
        },
    );
    assert!(ev_ack.is_some());
    assert_eq!(state.current_state, AlarmState::Normal);
}

#[test]
fn test_alarm_shelve_and_auto_timeout_unshelve() {
    let def = AlarmDefinition {
        alarm_id: 103,
        tag_name: "external.Pump01.Vibration".to_string(),
        description: "Vibration High".to_string(),
        severity: AlarmSeverity::Medium,
        trip_type: AlarmTripType::AnalogHigh {
            trip_threshold: 5.0,
            deadband: 0.1,
        },
        on_delay_ms: 0,
        off_delay_ms: 0,
        first_out_group_id: None,
        plant_suppression_tag: None,
        max_shelve_duration_sec: 120,
    };

    let mut state = AlarmRuntimeState::new(103);

    // Trip -> UnackAlarm
    state.step(
        &def,
        AlarmEventTrigger::SampleEvaluated {
            current_value: 6.0,
            timestamp_ns: 1_000_000_000,
        },
    );
    assert_eq!(state.current_state, AlarmState::UnackAlarm);

    // Shelve for 60 seconds
    state.step(
        &def,
        AlarmEventTrigger::OperatorShelve {
            user_id: "eng_fadhil".to_string(),
            duration_sec: 60,
            timestamp_ns: 1_000_000_000,
        },
    );
    assert_eq!(state.current_state, AlarmState::Shelved);

    // Tick at 30 seconds -> still Shelved
    let ev_mid = state.step(
        &def,
        AlarmEventTrigger::Tick {
            timestamp_ns: 31_000_000_000,
        },
    );
    assert!(ev_mid.is_none());
    assert_eq!(state.current_state, AlarmState::Shelved);

    // Tick at 61 seconds (1s past expiration) -> automatically UnackAlarm (condition still active)
    let ev_exp = state.step(
        &def,
        AlarmEventTrigger::Tick {
            timestamp_ns: 62_000_000_000,
        },
    );
    assert!(ev_exp.is_some());
    assert_eq!(state.current_state, AlarmState::UnackAlarm);
}

#[test]
fn test_first_out_monotonic_latching_root_cause_detection() {
    let mut manager = FirstOutManager::new();
    manager.register_group(1, "BoilerTurbineTripGroup", 5.0);

    let base_raw_ns = get_monotonic_raw_ns();
    assert!(base_raw_ns > 0);

    // Cascade sequence of 10 alarms
    // First alarm is the true initiating root-cause event
    let res1 = manager.record_trip(1, 201, base_raw_ns).unwrap();
    assert!(res1.is_leader);
    assert_eq!(res1.alarm_id, 201);
    assert_eq!(res1.leader_alarm_id, 201);
    assert_eq!(res1.delta_micros, 0);

    // Subsequent 9 alarms arriving in rapid microsecond intervals
    for i in 2..=10 {
        let alarm_id = 200 + i;
        let ts_offset_ns = base_raw_ns + ((i as u64) * 125_000); // 125 micros apart
        let res = manager.record_trip(1, alarm_id, ts_offset_ns).unwrap();

        assert!(!res.is_leader);
        assert_eq!(res.alarm_id, alarm_id);
        assert_eq!(res.leader_alarm_id, 201);
        assert_eq!(res.delta_micros, (i as u64) * 125);
    }

    // Reset group
    assert!(manager.reset_group(1));
    let status = manager.get_group_status(1).unwrap();
    assert!(!status.is_latched);
}

#[test]
fn test_deadband_hysteresis_and_filter_timer() {
    let deadband_abs = DeadbandConfig::Absolute(2.5);
    assert_eq!(deadband_abs.delta(), 2.5);

    let deadband_span = DeadbandConfig::SpanPercent {
        span: 200.0,
        percent: 1.5,
    };
    assert_eq!(deadband_span.delta(), 3.0);

    // Test filter timer: 50ms on-delay, 100ms off-delay
    let mut timer = AlarmFilterTimer::new(50, 100);

    // Spike at t=0 for only 20ms -> does not trigger
    assert!(!timer.update(true, 0));
    assert!(!timer.update(true, 20_000_000));
    assert!(!timer.update(false, 30_000_000));
    assert!(!timer.is_filtered_active());

    // Sustained excursion for 60ms -> triggers!
    assert!(!timer.update(true, 100_000_000));
    assert!(!timer.update(true, 130_000_000));
    assert!(timer.update(true, 160_000_000));
    assert!(timer.is_filtered_active());
}

#[test]
fn test_chatter_detector_suppression() {
    let mut detector = ChatterDetector::new(3, 60, 600);
    let base_ns = 1_000_000_000_000u64;

    // 3 transitions within 30s
    assert!(!detector.record_transition(base_ns));
    assert!(!detector.record_transition(base_ns + 10_000_000_000));
    assert!(!detector.record_transition(base_ns + 20_000_000_000));
    assert!(!detector.is_chatter_suppressed());

    // 4th transition within window -> triggers chatter suppression!
    let suppressed = detector.record_transition(base_ns + 25_000_000_000);
    assert!(suppressed);
    assert!(detector.is_chatter_suppressed());

    // Tick before cooling timeout (300s into cooling) -> still suppressed
    assert!(!detector.tick(base_ns + 325_000_000_000));
    assert!(detector.is_chatter_suppressed());

    // Tick past cooling duration (601s into cooling) -> un-suppressed!
    assert!(detector.tick(base_ns + 630_000_000_000));
    assert!(!detector.is_chatter_suppressed());
}
