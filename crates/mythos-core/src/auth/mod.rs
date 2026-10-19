//! Authentication, Authorization and Audit Subsystems.
//!
//! Complies with TSK-504, TSK-505, SYS-REQ-067, SYS-REQ-073, and ADR-005.

pub mod audit;
pub mod mcp_auth;

pub use audit::{AuditEntry, AuditIntegrityError, MerkleAuditLedger};
pub use mcp_auth::{scopes, AgentAuthToken, AgentClaims, AuthError, Ed25519TokenManager};
