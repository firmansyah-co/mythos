use mythos_core::recipe::*;
use std::collections::BTreeMap;

#[test]
fn test_recipe_model_constraints_validation_and_digest() {
    let mut formula = BTreeMap::new();
    formula.insert(
        "TargetTemperature".to_string(),
        ParameterValue::Float(142.5),
    );
    formula.insert(
        "DeaerationTimeSec".to_string(),
        ParameterValue::Integer(300),
    );
    formula.insert(
        "HoldingPressureBarg".to_string(),
        ParameterValue::Float(2.90),
    );

    let constraints = vec![
        ParameterConstraint {
            name: "TargetTemperature".to_string(),
            description: "Cooker Steam Temperature".to_string(),
            unit: "degC".to_string(),
            min_value: Some(100.0),
            max_value: Some(160.0),
            default_value: ParameterValue::Float(140.0),
        },
        ParameterConstraint {
            name: "HoldingPressureBarg".to_string(),
            description: "Peak Cook Pressure".to_string(),
            unit: "barg".to_string(),
            min_value: Some(1.0),
            max_value: Some(3.5),
            default_value: ParameterValue::Float(2.8),
        },
    ];

    let steps = vec![
        RecipeStep {
            step_index: 0,
            step_name: "DeaerationPurge".to_string(),
            phase_identifier: "PHASE_PURGE_01".to_string(),
            step_timeout_seconds: 600,
            step_parameters: BTreeMap::new(),
            transition_condition: StepTransitionCondition::TimerComplete,
        },
        RecipeStep {
            step_index: 1,
            step_name: "SteamRampCook".to_string(),
            phase_identifier: "PHASE_COOK_01".to_string(),
            step_timeout_seconds: 3600,
            step_parameters: BTreeMap::new(),
            transition_condition: StepTransitionCondition::PlcPhaseCompleteSignal,
        },
    ];

    let recipe = MasterRecipe {
        recipe_id: "RCP-PALM-OIL-001".to_string(),
        revision: "1.0.0".to_string(),
        title: "Palm Fruit Sterilization Standard Recipe".to_string(),
        author_user_id: "eng_adrian".to_string(),
        state: RecipeLifecycleState::Approved,
        created_at_utc: "2026-10-19T00:00:00Z".to_string(),
        formula,
        steps,
        digital_signature_sha256: None,
    };

    // Valid constraints
    assert!(recipe.validate_constraints(&constraints).is_ok());

    // Invalid constraint test: boundary violation
    let invalid_constraints = vec![ParameterConstraint {
        name: "TargetTemperature".to_string(),
        description: "Cooker Steam Temperature".to_string(),
        unit: "degC".to_string(),
        min_value: Some(100.0),
        max_value: Some(140.0), // 142.5 > 140.0
        default_value: ParameterValue::Float(135.0),
    }];
    let err = recipe
        .validate_constraints(&invalid_constraints)
        .unwrap_err();
    assert!(err.contains("value 142.5 > max 140"));

    // Digest computation is deterministic
    let digest1 = recipe.compute_content_digest();
    let digest2 = recipe.compute_content_digest();
    assert_eq!(digest1, digest2);
    assert_eq!(digest1.len(), 64);
}

#[test]
fn test_recipe_4_phase_handshake_synchronization_success() {
    let coordinator = RecipeSyncCoordinator::new(2000);

    let mut formula = BTreeMap::new();
    formula.insert("MixAgitatorRPM".to_string(), ParameterValue::Integer(120));
    formula.insert("DosingLiters".to_string(), ParameterValue::Float(450.0));

    let recipe = MasterRecipe {
        recipe_id: "RCP-BATCH-MIX-42".to_string(),
        revision: "2.1.0".to_string(),
        title: "Chemical Batch Mix".to_string(),
        author_user_id: "eng_fadhil".to_string(),
        state: RecipeLifecycleState::Released,
        created_at_utc: "2026-10-19T00:00:00Z".to_string(),
        formula,
        steps: vec![],
        digital_signature_sha256: None,
    };

    let mut bus = RecipeBusRegisters::default();

    // 1. SCADA Phase 2: WRITE BUS
    let digest = coordinator
        .write_bus(&mut bus, "BATCH-2026-0001", &recipe)
        .unwrap();
    assert!(bus.command_trigger_handshake);
    assert_eq!(bus.command_code, 1); // LOAD
    assert_eq!(bus.command_checksum_sha256, digest);

    // 2. STruC++ PLC scan cycle handles handshake
    coordinator.plc_scan_cycle_transfer(&mut bus);
    assert!(bus.status_ack_handshake);
    assert!(bus.status_parameter_valid);

    // 3. SCADA Phase 3: VERIFY
    assert!(coordinator.verify_ack(&mut bus, &digest).is_ok());
    assert!(!bus.command_trigger_handshake);

    // 4. SCADA Phase 4: START BATCH
    let batch = coordinator
        .start_batch(
            &mut bus,
            "BATCH-2026-0001",
            &recipe,
            "op_dewabagus",
            1_000_000_000,
        )
        .unwrap();

    assert_eq!(batch.batch_id, "BATCH-2026-0001");
    assert_eq!(batch.packml_state, PackMlState::Starting);
    assert_eq!(batch.status, BatchRunStatus::Running);
    assert_eq!(bus.command_code, 2); // START
    assert_eq!(bus.status_execution_state, 1); // RUNNING
}

#[test]
fn test_recipe_handshake_rejects_transfer_while_running() {
    let coordinator = RecipeSyncCoordinator::new(2000);
    let recipe = MasterRecipe {
        recipe_id: "RCP-ACTIVE".to_string(),
        revision: "1.0".to_string(),
        title: "Active".to_string(),
        author_user_id: "user".to_string(),
        state: RecipeLifecycleState::Released,
        created_at_utc: "2026-10-19".to_string(),
        formula: BTreeMap::new(),
        steps: vec![],
        digital_signature_sha256: None,
    };

    let mut bus = RecipeBusRegisters {
        status_execution_state: 1,
        ..Default::default()
    };

    let err = coordinator
        .write_bus(&mut bus, "BATCH-999", &recipe)
        .unwrap_err();
    assert_eq!(
        err,
        RecipeSyncError::InvalidExecutionState {
            state: "RUNNING".to_string()
        }
    );
}
