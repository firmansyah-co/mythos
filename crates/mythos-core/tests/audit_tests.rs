//! Merkle-Chained Audit Ledger Integrity and Tamper Evidence Test Suite.
//!
//! Complies with TSK-505, SYS-REQ-067, and ADR-005.

use mythos_core::auth::{AuditEntry, AuditIntegrityError, MerkleAuditLedger};
use serde_json::json;
use std::fs;
use tempfile::NamedTempFile;

#[test]
fn test_sequential_append_sha256_hash_chaining() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    let ledger = MerkleAuditLedger::open_or_create(&path).unwrap();

    // Entry 0 (Genesis)
    let entry0 = ledger
        .append(
            "agent:ai-01",
            "Operator",
            "write_tag",
            "external.Boiler.SP101.pressure.sp",
            json!({"previous": 10.0, "new": 12.5, "intent": "ramp up boiler"}),
        )
        .unwrap();

    assert_eq!(entry0.index, 0);
    assert_eq!(entry0.prev_hash, AuditEntry::GENESIS_PREV_HASH);
    assert!(entry0.verify_hash());

    // Entry 1
    let entry1 = ledger
        .append(
            "operator:dewa_bagus",
            "Supervisor",
            "ack_alarm",
            "ALM-PRESS-101",
            json!({"reason": "steam valve opened manually"}),
        )
        .unwrap();

    assert_eq!(entry1.index, 1);
    assert_eq!(entry1.prev_hash, entry0.hash);
    assert!(entry1.verify_hash());

    // Entry 2
    let entry2 = ledger
        .append(
            "engineer:fadhil",
            "Engineer",
            "deploy_logic",
            "MainTask",
            json!({"hash": "c0ffee1234", "source": "ST"}),
        )
        .unwrap();

    assert_eq!(entry2.index, 2);
    assert_eq!(entry2.prev_hash, entry1.hash);
    assert!(entry2.verify_hash());

    // Verify integrity across the entire ledger
    assert!(ledger.verify_integrity().is_ok());
    assert_eq!(ledger.entry_count(), 3);
}

#[test]
fn test_full_chain_integrity_verification_and_restore() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    {
        let ledger = MerkleAuditLedger::open_or_create(&path).unwrap();
        for i in 0..10 {
            ledger
                .append(
                    format!("caller:{}", i),
                    "Operator",
                    "action_tick",
                    format!("target:{}", i),
                    json!({"tick": i}),
                )
                .unwrap();
        }
        assert_eq!(ledger.entry_count(), 10);
        assert!(ledger.verify_integrity().is_ok());
    }

    // Reopen ledger from disk: must verify and reconstruct chain state
    let reopened = MerkleAuditLedger::open_or_create(&path).unwrap();
    assert_eq!(reopened.entry_count(), 10);
    assert!(reopened.verify_integrity().is_ok());

    let entries = reopened.read_entries().unwrap();
    assert_eq!(entries.len(), 10);
    for (idx, entry) in entries.iter().enumerate() {
        assert_eq!(entry.index, idx as u64);
        assert!(entry.verify_hash());
    }
}

#[test]
fn test_tamper_evidence_single_character_payload_modification() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    let ledger = MerkleAuditLedger::open_or_create(&path).unwrap();

    ledger
        .append(
            "agent:ai-01",
            "Operator",
            "write_tag",
            "external.Boiler.SP101.pressure.sp",
            json!({"setpoint": 10.0}),
        )
        .unwrap();
    ledger
        .append(
            "agent:ai-02",
            "Operator",
            "write_tag",
            "external.Boiler.SP102.pressure.sp",
            json!({"setpoint": 20.0}),
        )
        .unwrap();

    // Ledger is initially cryptographically valid
    assert!(ledger.verify_integrity().is_ok());

    // Simulate tampering: modify one character in payload (e.g., setpoint 10.0 -> 90.0)
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("10.0"));
    let tampered_content = content.replacen("10.0", "90.0", 1);
    fs::write(&path, tampered_content).unwrap();

    // verify_integrity() MUST detect tampering and return TamperedRecordDetected
    let verify_res = MerkleAuditLedger::verify_file_integrity(&path);
    assert!(verify_res.is_err());
    match verify_res.unwrap_err() {
        AuditIntegrityError::TamperedRecordDetected { index, .. } => {
            assert_eq!(index, 0);
        }
        other => panic!("Expected TamperedRecordDetected error, got {:?}", other),
    }
}

#[test]
fn test_tamper_evidence_timestamp_modification() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    let ledger = MerkleAuditLedger::open_or_create(&path).unwrap();

    ledger
        .append(
            "agent:ai-01",
            "Operator",
            "write_tag",
            "tag:1",
            json!({"val": 1}),
        )
        .unwrap();

    let content = fs::read_to_string(&path).unwrap();
    // Tamper with timestamp_ns digit
    let tampered_content = content.replacen(r#""timestamp_ns":"#, r#""timestamp_ns":9"#, 1);
    fs::write(&path, tampered_content).unwrap();

    let verify_res = MerkleAuditLedger::verify_file_integrity(&path);
    assert!(verify_res.is_err());
}
