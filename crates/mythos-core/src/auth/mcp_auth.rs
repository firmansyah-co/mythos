//! Ed25519 Cryptographic Token Authentication and RBAC Verification.
//!
//! Complies with TSK-504, SYS-REQ-067, SYS-REQ-073, and ADR-005.
//! Provides AgentClaims, AgentAuthToken, Ed25519TokenManager, and capability scope enforcement.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Authentication and authorization errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("Token format invalid: {0}")]
    InvalidTokenFormat(String),

    #[error("Cryptographic signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    #[error("Token expired: current_time_ns={current_time_ns}, expires_at_ns={expires_at_ns}")]
    TokenExpired {
        current_time_ns: u64,
        expires_at_ns: u64,
    },

    #[error("Token not yet valid: current_time_ns={current_time_ns}, issued_at_ns={issued_at_ns}")]
    TokenNotYetValid {
        current_time_ns: u64,
        issued_at_ns: u64,
    },

    #[error("Unauthorized: missing required scope '{required_scope}' (has: {assigned_scopes:?})")]
    MissingScope {
        required_scope: String,
        assigned_scopes: Vec<String>,
    },

    #[error("Key format invalid: {0}")]
    InvalidKey(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Standard capability scopes for MythOS AI agents and operators.
pub mod scopes {
    /// Read tags and real-time process variables.
    pub const TAG_READ: &str = "tag:read";
    /// Write setpoints and modify actuator states.
    pub const TAG_WRITE: &str = "tag:write";
    /// Deploy and reload IEC 61131-3 logic programs.
    pub const CONTROL_DEPLOY: &str = "control:deploy";
    /// Acknowledge alarms per ANSI/ISA-18.2.
    pub const ALARM_ACK: &str = "alarm:ack";
    /// Shelve alarms with duration and reason.
    pub const ALARM_SHELVE: &str = "alarm:shelve";
    /// Discover physical and virtual field network devices.
    pub const DEVICE_DISCOVER: &str = "device:discover";
    /// Export historical telemetry data.
    pub const TELEMETRY_EXPORT: &str = "telemetry:export";
    /// Full administrative superuser scope.
    pub const ADMIN: &str = "*";
}

/// Token payload claims representing an AI Agent or Operator identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClaims {
    /// Unique identifier of the agent or operator (e.g., "agent:echo-01", "operator:adrian").
    pub agent_id: String,
    /// System role (e.g., "Guest", "Operator", "Supervisor", "Engineer", "Admin").
    pub role: String,
    /// Explicit capability scopes granted to this token.
    pub scopes: Vec<String>,
    /// Token creation timestamp in nanoseconds since UNIX epoch.
    pub issued_at_ns: u64,
    /// Token expiration timestamp in nanoseconds since UNIX epoch.
    pub expires_at_ns: u64,
}

impl AgentClaims {
    /// Create new claims with a duration in seconds.
    pub fn new(
        agent_id: impl Into<String>,
        role: impl Into<String>,
        scopes: Vec<String>,
        duration_secs: u64,
    ) -> Self {
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        Self {
            agent_id: agent_id.into(),
            role: role.into(),
            scopes,
            issued_at_ns: now_ns,
            expires_at_ns: now_ns.saturating_add(duration_secs.saturating_mul(1_000_000_000)),
        }
    }

    /// Check if these claims include the required scope (supports wildcard `*`).
    pub fn has_scope(&self, required_scope: &str) -> bool {
        self.scopes.iter().any(|s| s == "*" || s == required_scope)
    }

    /// Verify if the claims are currently valid given the timestamp in nanoseconds.
    pub fn validate_time(&self, current_time_ns: u64) -> Result<(), AuthError> {
        if current_time_ns > self.expires_at_ns {
            return Err(AuthError::TokenExpired {
                current_time_ns,
                expires_at_ns: self.expires_at_ns,
            });
        }
        if current_time_ns + 10_000_000_000 < self.issued_at_ns {
            // Allow 10s clock skew
            return Err(AuthError::TokenNotYetValid {
                current_time_ns,
                issued_at_ns: self.issued_at_ns,
            });
        }
        Ok(())
    }
}

/// Compact representation of an authenticated agent token.
/// Serialized format: `<payload_base64_or_hex>.<signature_hex>`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAuthToken {
    pub claims: AgentClaims,
    pub signature_hex: String,
}

/// Ed25519 Token Manager for cryptographic signing and verification.
#[derive(Clone)]
pub struct Ed25519TokenManager {
    signing_key: Option<SigningKey>,
    verifying_key: VerifyingKey,
}

impl Ed25519TokenManager {
    /// Generate a new ephemeral token manager with a random Ed25519 keypair.
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key: Some(signing_key),
            verifying_key,
        }
    }

    /// Create a token manager from a 32-byte signing key (private key).
    pub fn from_signing_key_bytes(bytes: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(bytes);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key: Some(signing_key),
            verifying_key,
        }
    }

    /// Create a verification-only token manager from a 32-byte verifying key (public key).
    pub fn from_verifying_key(verifying_key: VerifyingKey) -> Self {
        Self {
            signing_key: None,
            verifying_key,
        }
    }

    /// Create a verification-only token manager from 32-byte public key slice.
    pub fn from_verifying_key_bytes(bytes: &[u8; 32]) -> Result<Self, AuthError> {
        let verifying_key =
            VerifyingKey::from_bytes(bytes).map_err(|e| AuthError::InvalidKey(e.to_string()))?;
        Ok(Self {
            signing_key: None,
            verifying_key,
        })
    }

    /// Get verifying key bytes (public key).
    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Get verifying key hex string.
    pub fn verifying_key_hex(&self) -> String {
        hex::encode(self.verifying_key.to_bytes())
    }

    /// Issue and cryptographically sign a token with claims.
    pub fn issue_token(&self, claims: &AgentClaims) -> Result<String, AuthError> {
        let signer = self.signing_key.as_ref().ok_or_else(|| {
            AuthError::InvalidKey(
                "Cannot issue token: signing key not available (verification-only manager)"
                    .to_string(),
            )
        })?;

        let payload_json =
            serde_json::to_vec(claims).map_err(|e| AuthError::SerializationError(e.to_string()))?;
        let payload_hex = hex::encode(&payload_json);

        let signature: Signature = signer.sign(&payload_json);
        let signature_hex = hex::encode(signature.to_bytes());

        // Token format: payload_hex.signature_hex
        Ok(format!("{}.{}", payload_hex, signature_hex))
    }

    /// Decode and cryptographically verify a token string.
    pub fn verify_token(&self, token_str: &str) -> Result<AgentClaims, AuthError> {
        let parts: Vec<&str> = token_str.trim().split('.').collect();
        if parts.len() != 2 {
            return Err(AuthError::InvalidTokenFormat(
                "Expected token format '<payload_hex>.<signature_hex>'".to_string(),
            ));
        }

        let payload_bytes = hex::decode(parts[0])
            .map_err(|e| AuthError::InvalidTokenFormat(format!("Invalid payload hex: {}", e)))?;
        let sig_bytes = hex::decode(parts[1])
            .map_err(|e| AuthError::InvalidTokenFormat(format!("Invalid signature hex: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(AuthError::InvalidTokenFormat(format!(
                "Invalid signature length: expected 64 bytes, got {}",
                sig_bytes.len()
            )));
        }

        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_arr);

        // Verify cryptographic signature
        self.verifying_key
            .verify(&payload_bytes, &signature)
            .map_err(|e| AuthError::SignatureVerificationFailed(e.to_string()))?;

        // Deserialize claims
        let claims: AgentClaims = serde_json::from_slice(&payload_bytes)
            .map_err(|e| AuthError::SerializationError(format!("Failed to parse claims: {}", e)))?;

        // Verify expiration
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        claims.validate_time(now_ns)?;

        Ok(claims)
    }

    /// Verify token cryptographic validity and assert a specific capability scope.
    ///
    /// CRITICAL INVARIANT (SYS-REQ-073):
    /// Any agent missing the required scope (e.g., `tag:write`, `control:deploy`)
    /// MUST be hard-rejected before modifying plant state.
    pub fn verify_token_scope(
        &self,
        token_str: &str,
        required_scope: &str,
    ) -> Result<AgentClaims, AuthError> {
        let claims = self.verify_token(token_str)?;
        if !claims.has_scope(required_scope) {
            return Err(AuthError::MissingScope {
                required_scope: required_scope.to_string(),
                assigned_scopes: claims.scopes.clone(),
            });
        }
        Ok(claims)
    }
}
