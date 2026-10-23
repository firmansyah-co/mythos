use mythos_core::recipe::{MasterRecipe, RecipeLifecycleState};
use mythos_core::recovery::*;
use mythos_core::storage::{ControllerStorage, ProjectConfig, TaskConfig};
use std::collections::BTreeMap;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_crypto_roundtrip_and_passphrase_failure() {
    let plaintext = b"CRITICAL_SCADA_SECURITY_PAYLOAD_CHA_CHA_20_POLY_1305";
    let passphrase = "SuperSecureFacilityPassphrase2026!";

    // Encrypt
    let (salt, nonce, ciphertext) = encrypt_payload(plaintext, passphrase).unwrap();

    // Decrypt with correct passphrase
    let decrypted = decrypt_payload(&ciphertext, passphrase, &salt, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);

    // Decrypt with incorrect passphrase must fail
    let bad_pass = "WrongPassphrase!";
    let result = decrypt_payload(&ciphertext, bad_pass, &salt, &nonce);
    assert!(result.is_err());
}

#[test]
fn test_package_header_serialization() {
    let salt = [0x42u8; 32];
    let nonce = [0x24u8; 12];
    let payload_len = 1048576u64;

    let header = PackageHeader::new(salt, nonce, payload_len);
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), HEADER_LEN);
    assert_eq!(&bytes[0..10], PACKAGE_MAGIC);

    let parsed = PackageHeader::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, header);
}

#[test]
fn test_pack_unpack_and_hot_restore_sla() {
    let temp_root = tempdir().unwrap();
    let storage = ControllerStorage::new(temp_root.path()).unwrap();

    // Create project
    let project_cfg = ProjectConfig::new("plant-delta-01", "Plant Delta Unit 1");
    let pdir = storage.create_project(&project_cfg).unwrap();

    // Populate tasks
    let tasks = vec![
        TaskConfig {
            name: "FastScan".to_string(),
            interval_ms: 10,
            priority: 90,
            core_affinity: Some(1),
        },
        TaskConfig {
            name: "SlowTelemetry".to_string(),
            interval_ms: 1000,
            priority: 20,
            core_affinity: None,
        },
    ];
    let tasks_json = serde_json::to_string_pretty(&tasks).unwrap();
    fs::write(pdir.join("tasks.active.json"), tasks_json).unwrap();

    // Populate logic .st
    let st_code = r#"PROGRAM MainControl
VAR
    TankLevel : REAL;
    PumpValve : BOOL;
END_VAR
IF TankLevel > 80.0 THEN
    PumpValve := FALSE;
END_IF;
END_PROGRAM"#;
    fs::write(pdir.join("logic/active/main.st"), st_code).unwrap();

    // Populate screens and faceplates
    fs::write(
        pdir.join("screens/active/overview.json"),
        r#"{"title": "Overview", "widgets": 4}"#,
    )
    .unwrap();
    fs::write(
        pdir.join("faceplates/active/valve_fp.json"),
        r#"{"component": "ValveFaceplate"}"#,
    )
    .unwrap();

    // Populate recipes
    let recipe = MasterRecipe {
        recipe_id: "REC-BATCH-501".to_string(),
        revision: "2.1.0".to_string(),
        title: "Polymer Synthesis 501".to_string(),
        author_user_id: "chief_chemist".to_string(),
        state: RecipeLifecycleState::Released,
        created_at_utc: "2026-10-15T00:00:00Z".to_string(),
        formula: BTreeMap::new(),
        steps: Vec::new(),
        digital_signature_sha256: None,
    };

    // Populate users
    let users = vec![
        UserRecord {
            username: "dewa_bagus".to_string(),
            role: "ot_specialist".to_string(),
            scopes: vec![
                "tag:read".to_string(),
                "tag:write".to_string(),
                "control:deploy".to_string(),
            ],
        },
        UserRecord {
            username: "fadhil".to_string(),
            role: "ot_technician".to_string(),
            scopes: vec!["tag:read".to_string(), "alarm:ack".to_string()],
        },
    ];

    let master_passphrase = "MasterOfflineRestoreSecret2026";

    // 1. Pack project to in-memory bytes
    let pkg_bytes = RecoveryEngine::pack(
        "plant-delta-01",
        &storage,
        master_passphrase,
        std::slice::from_ref(&recipe),
        &users,
    )
    .unwrap();

    assert!(pkg_bytes.len() > HEADER_LEN);
    assert_eq!(&pkg_bytes[0..10], PACKAGE_MAGIC);

    // 2. Inspect manifest without full restore
    let manifest = RecoveryEngine::inspect_manifest(&pkg_bytes, master_passphrase).unwrap();
    assert_eq!(manifest.project_id, "plant-delta-01");
    assert_eq!(manifest.recipe_count, 1);
    assert_eq!(manifest.user_count, 2);
    assert_eq!(manifest.logic_file_count, 1);

    // 3. Unpack into DisasterRecoveryBundle
    let bundle = RecoveryEngine::unpack(&pkg_bytes, master_passphrase).unwrap();
    assert_eq!(bundle.project.name, "Plant Delta Unit 1");
    assert_eq!(bundle.tasks.len(), 2);
    assert_eq!(bundle.recipes.len(), 1);
    assert_eq!(bundle.users.len(), 2);
    assert_eq!(bundle.logic_files.get("main.st").unwrap(), st_code);

    // 4. Hot restore into a fresh storage target (simulating bare-metal cold replacement IPC)
    let replacement_root = tempdir().unwrap();
    let replacement_storage = ControllerStorage::new(replacement_root.path()).unwrap();

    let report = RecoveryEngine::hot_restore(&bundle, &replacement_storage, None).unwrap();

    // Verify SLA < 25s
    assert!(report.meets_sla_25s);
    assert!(report.duration_ms < 25_000);
    assert!(report.restored_files_count >= 8);

    // Verify filesystem state in restored storage
    let active_project_id = replacement_storage.get_active_project().unwrap();
    assert_eq!(active_project_id, Some("plant-delta-01".to_string()));

    let restored_config = replacement_storage.get_project("plant-delta-01").unwrap();
    assert_eq!(restored_config.name, "Plant Delta Unit 1");

    let restored_st = fs::read_to_string(
        replacement_storage
            .project_dir("plant-delta-01")
            .join("logic/active/main.st"),
    )
    .unwrap();
    assert_eq!(restored_st, st_code);
}
