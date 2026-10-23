//! Tests for Ed25519 Token Authentication and Merkle-Chained JSONL Audit Ledger.
//!
//! Validates TSK-504 and TSK-505 requirements.

use mythos_core::auth::{
    scopes, AgentClaims, AuditEntry, AuditIntegrityError, AuthError, Ed25519TokenManager,
    MerkleAuditLedger,
};
use serde_json::json;
use tempfile::NamedTempFile;

#[test]
fn test_ed25519_token_generation_and_verification() {
    let manager = Ed25519TokenManager::generate();
    let claims = AgentClaims::new(
        "agent:test-01",
        "Engineer",
        vec![scopes::TAG_READ.to_string(), scopes::TAG_WRITE.to_string()],
        3600, // 1 hour
    );

    let token = manager
        .issue_token(&claims)
        .expect("Token issuance should succeed");
    assert!(!token.is_empty());
    assert!(token.contains('.'));

    // Verify token
    let verified_claims = manager
        .verify_token(&token)
        .expect("Token verification should succeed");
    assert_eq!(verified_claims.agent_id, "agent:test-01");
    assert_eq!(verified_claims.role, "Engineer");
    assert!(verified_claims.has_scope(scopes::TAG_READ));
    assert!(verified_claims.has_scope(scopes::TAG_WRITE));
    assert!(!verified_claims.has_scope(scopes::CONTROL_DEPLOY));
}

#[test]
fn test_token_scope_enforcement_and_rejection() {
    let manager = Ed25519TokenManager::generate();
    // Token with ONLY tag:read
    let claims = AgentClaims::new(
        "agent:read-only",
        "Operator",
        vec![scopes::TAG_READ.to_string()],
        3600,
    );
    let token = manager.issue_token(&claims).unwrap();

    // Verification for tag:read must pass
    assert!(manager.verify_token_scope(&token, scopes::TAG_READ).is_ok());

    // CRITICAL SAFETY INVARIANT: Rejection of tag:write and control:deploy
    let write_err = manager
        .verify_token_scope(&token, scopes::TAG_WRITE)
        .unwrap_err();
    match write_err {
        AuthError::MissingScope { required_scope, .. } => {
            assert_eq!(required_scope, scopes::TAG_WRITE);
        }
        other => panic!("Expected MissingScope error, got {:?}", other),
    }

    let deploy_err = manager
        .verify_token_scope(&token, scopes::CONTROL_DEPLOY)
        .unwrap_err();
    match deploy_err {
        AuthError::MissingScope { required_scope, .. } => {
            assert_eq!(required_scope, scopes::CONTROL_DEPLOY);
        }
        other => panic!("Expected MissingScope error, got {:?}", other),
    }
}

#[test]
fn test_tampered_token_signature_rejection() {
    let manager = Ed25519TokenManager::generate();
    let claims = AgentClaims::new("agent:007", "Admin", vec![scopes::ADMIN.to_string()], 3600);
    let token = manager.issue_token(&claims).unwrap();

    let parts: Vec<&str> = token.split('.').collect();
    // Tamper with payload
    let mut tampered_payload = parts[0].to_string();
    if tampered_payload.ends_with('0') {
        tampered_payload.pop();
        tampered_payload.push('1');
    } else {
        tampered_payload.pop();
        tampered_payload.push('0');
    }
    let tampered_token = format!("{}.{}", tampered_payload, parts[1]);

    let res = manager.verify_token(&tampered_token);
    assert!(res.is_err());
}

#[test]
fn test_expired_token_rejection() {
    let manager = Ed25519TokenManager::generate();
    let mut claims = AgentClaims::new(
        "agent:expired",
        "Operator",
        vec![scopes::TAG_READ.to_string()],
        0,
    );
    // Force expiration in the past
    claims.issued_at_ns = 1_000_000_000;
    claims.expires_at_ns = 2_000_000_000;

    let token = manager.issue_token(&claims).unwrap();
    let res = manager.verify_token(&token);
    assert!(matches!(res, Err(AuthError::TokenExpired { .. })));
}

#[test]
fn test_merkle_audit_ledger_integrity() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    let ledger = MerkleAuditLedger::open_or_create(&path).unwrap();

    // Write 5 entries
    let entry0 = ledger
        .append(
            "agent:ai-1",
            "Operator",
            "write_tag",
            "ns=1;s=Boiler.Temp.SP",
            json!({"previous": 60.0, "new": 75.0, "operator_intent": "preheat cycle"}),
        )
        .unwrap();

    assert_eq!(entry0.index, 0);
    assert_eq!(entry0.prev_hash, AuditEntry::GENESIS_PREV_HASH);
    assert!(entry0.verify_hash());

    let entry1 = ledger
        .append(
            "operator:adrian",
            "Supervisor",
            "ack_alarm",
            "ALM-PRESS-01",
            json!({"reason": "pressure stabilized"}),
        )
        .unwrap();

    assert_eq!(entry1.index, 1);
    assert_eq!(entry1.prev_hash, entry0.hash);
    assert!(entry1.verify_hash());

    let entry2 = ledger
        .append(
            "engineer:neo",
            "Engineer",
            "deploy_logic",
            "MainTask",
            json!({"hash": "deadbeef", "language": "ST"}),
        )
        .unwrap();

    assert_eq!(entry2.index, 2);
    assert_eq!(entry2.prev_hash, entry1.hash);
    assert!(entry2.verify_hash());

    // Integrity check must pass
    assert!(ledger.verify_integrity().is_ok());

    // Reopen ledger and verify state restoration
    let reopened = MerkleAuditLedger::open_or_create(&path).unwrap();
    assert_eq!(reopened.entry_count(), 3);
    assert!(reopened.verify_integrity().is_ok());

    let entries = reopened.read_entries().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].action, "write_tag");
    assert_eq!(entries[1].action, "ack_alarm");
    assert_eq!(entries[2].action, "deploy_logic");
}

#[test]
fn test_merkle_audit_ledger_tamper_detection() {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path().to_path_buf();

    let ledger = MerkleAuditLedger::open_or_create(&path).unwrap();

    ledger
        .append(
            "agent:ai-1",
            "Operator",
            "write_tag",
            "tag:1",
            json!({"val": 10}),
        )
        .unwrap();
    ledger
        .append(
            "agent:ai-2",
            "Operator",
            "write_tag",
            "tag:2",
            json!({"val": 20}),
        )
        .unwrap();

    // Verify initially valid
    assert!(ledger.verify_integrity().is_ok());

    // Tamper with file directly on disk: alter line 1 (val 10 -> val 99)
    let content = std::fs::read_to_string(&path).unwrap();
    let tampered_content = content.replace("10", "99");
    std::fs::write(&path, tampered_content).unwrap();

    // Verification must detect tampering
    let verify_res = MerkleAuditLedger::verify_file_integrity(&path);
    assert!(verify_res.is_err());
    assert!(matches!(
        verify_res.unwrap_err(),
        AuditIntegrityError::TamperedRecordDetected { .. }
    ));
}
