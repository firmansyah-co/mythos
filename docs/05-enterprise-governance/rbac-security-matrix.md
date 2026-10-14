# IEC 62443 Role-Based Access Control & Audit Security Matrix

## 1. Executive Summary & Purpose
This document specifies the security architecture, Role-Based Access Control (RBAC) model, AI Agent Model Context Protocol (MCP) authentication token policies, and tamper-evident append-only Audit Trail specification for MythOS.

MythOS operates in critical infrastructure environments governed by **IEC 62443-4-2** (Technical Security Requirements for IACS Components) and **IEC 62443-3-3** (System Security Requirements and Security Levels). The architecture enforces defense-in-depth, cryptographic token segregation, zero implicit privileges, and non-repudiable audit trails for both human operators and autonomous AI agents.

---

## 2. Normative References
- **IEC 62443-4-2:2019**: *Security for industrial automation and control systems - Technical security requirements for IACS components*.
- **IEC 62443-3-3:2013**: *System security requirements and security levels (SL-1 to SL-4)*.
- **NIST SP 800-63B**: *Digital Identity Guidelines: Authentication and Lifecycle Management*.
- **RFC 7519**: *JSON Web Token (JWT)*.
- **RFC 8725**: *JSON Web Token Best Current Practices*.

---

## 3. IEC 62443 Role Hierarchy & Permission Matrix

MythOS establishes five discrete human roles and dedicated machine/agent service accounts:

```
[ GUEST ] <--- [ OPERATOR ] <--- [ SUPERVISOR ] <--- [ ENGINEER ] <--- [ ADMIN ]
```

### 3.1 Permission Matrix by Operation Scope

| Functional Scope | Guest | Operator | Supervisor | Engineer | Admin | AI Agent (Scoped MCP) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **View Live HMI Graphics** | Yes | Yes | Yes | Yes | Yes | Yes (Read scope) |
| **View Alarm Summary** | Yes | Yes | Yes | Yes | Yes | Yes (Read scope) |
| **Acknowledge Alarms** | No | Yes | Yes | Yes | Yes | Conditional (`mcp:alarm:ack`) |
| **Shelve / Un-shelve Alarms** | No | Yes (<=2h) | Yes (<=24h) | Yes (<=7d) | Yes | Conditional (`mcp:alarm:shelve`) |
| **Set Alarm Out of Service** | No | No | No | Yes | Yes | Denied |
| **Toggle Manual Field Actuators** | No | Yes | Yes | Yes | Yes | Denied (Safety lock) |
| **Change Continuous Setpoints** | No | Yes (Bounded) | Yes (Full) | Yes (Full) | Yes | Conditional (`mcp:tag:write`) |
| **Execute / Hold Batch Recipe** | No | Yes | Yes | Yes | Yes | Conditional (`mcp:batch:cmd`) |
| **Edit / Sign Master Recipe** | No | No | Yes (Sign) | Yes (Edit) | Yes | Denied (21 CFR Part 11) |
| **Tune PID Parameters** | No | No | No | Yes | Yes | Conditional (`mcp:pid:tune`) |
| **Hot-Reload OpenPLC Logic** | No | No | No | Yes | Yes | Denied |
| **Configure Protocol Drivers** | No | No | No | Yes | Yes | Denied |
| **Export Historian CSV Data** | No | Yes | Yes | Yes | Yes | Yes (`mcp:historian:read`) |
| **User Account & Role Admin** | No | No | No | No | Yes | Denied |
| **Trigger Disaster Backup** | No | No | Yes | Yes | Yes | Yes (`mcp:backup:trigger`) |
| **Restore System from Backup**| No | No | No | No | Yes | Denied |

---

## 4. AI Agent Model Context Protocol (MCP) Security Architecture

MythOS exposes native Model Context Protocol (MCP) endpoints over stdio and authenticated SSE/TLS. Autonomous agents (e.g., Aria, Neo, Sage, Vulcan, Iris) interact with MythOS via these interfaces.

```
+-------------------------------------------------------------------------+
| AUTONOMOUS AI AGENT / MCP CLIENT (e.g. SAGE / NEO)                      |
+-------------------------------------------------------------------------+
                                     |
                                     | Request with Bearer ed25519 Token
                                     v
+-------------------------------------------------------------------------+
| MYTHOS MCP GATEWAY & SECURITY ENFORCER                                  |
| 1. Cryptographic Signature Verification (Ed25519)                       |
| 2. Scope & Capability Verification                                      |
| 3. Mandatory Safety Interlock Validation (E-STOP & Manual Inhibit)     |
+-------------------------------------------------------------------------+
                                     |
                       +-------------+-------------+
                       |                           |
                 (Authorized)                (Unauthorized)
                       |                           |
                       v                           v
+----------------------------------+     +-------------------------------+
| DISPATCH ACTION TO TAGBUS / PLC  |     | REJECT & WRITE TAMPER AUDIT   |
+----------------------------------+     +-------------------------------+
```

### 4.1 Token Specification & Scopes
AI Agent tokens are cryptographically signed using **Ed25519** public-key cryptography.

- **Token Structure (JWT RFC 7519)**:
  - `iss`: `"mythos-security-auth"`
  - `sub`: `"agent://mythos.rnd.sage"`
  - `aud`: `"mythos-runtime-ipc"`
  - `exp`: Monotonic expiration epoch (Maximum TTL: 3600 seconds; automatic rolling refresh).
  - `role`: `"service_agent"`
  - `scopes`: Array of granular URIs:
    - `"mcp:telemetry:read"`: Read access to `/dev/shm/mythos_tagbus`.
    - `"mcp:alarm:read"`: Read alarm states.
    - `"mcp:alarm:ack"`: Permitted to acknowledge non-critical alarms.
    - `"mcp:tag:write:setpoint"`: Permitted to write designated setpoint tags within safety limits.
    - `"mcp:historian:query"`: Execute time-series SQL/Parquet queries.
  - `safety_envelope`:
    - `max_setpoint_step_pct`: Maximum single-step setpoint modification (e.g., 5.0%).
    - `prohibited_tags`: List of safety-critical tags strictly forbidden from AI mutation (e.g., `safety.Estop.*`, `interlock.DoorRelease.*`).

---

## 5. Tamper-Evident Append-Only Audit Trail Schema

Compliance with IEC 62443 and 21 CFR Part 11 mandates that all administrative actions, alarm acknowledgments, recipe modifications, and setpoint writes are recorded in an immutable, cryptographically chained audit log.

### 5.1 Merkle Chaining / Hash-Linked Ledger
Every audit record contains the SHA-256 hash of the immediately preceding record:

$$\text{RecordHash}_k = \text{SHA256}(\text{RecordHash}_{k-1} \,\|\, \text{Timestamp}_k \,\|\, \text{Actor}_k \,\|\, \text{Action}_k \,\|\, \text{Payload}_k)$$

Any retroactive tampering, line deletion, or text manipulation in the log file invalidates the hash chain at index $k+1$, triggering an instant alarm alert on IPC startup.

### 5.2 Audit Record JSONL Schema

```json
{
  "sequence_index": 104291,
  "prev_hash_sha256": "4b92ec8401daec619d80315a6b0cfa09b3014e308b0c8d10b784a0d9f2a24bc1",
  "record_hash_sha256": "8f3b2049e701a9bd14238719bc45deca0198031ba0987fca401290bb0021c432",
  "timestamp_utc": "2026-03-29T14:32:01.408192Z",
  "actor": {
    "actor_type": "HUMAN_OPERATOR",
    "user_id": "operator.adrian",
    "role": "Supervisor",
    "client_ip": "192.168.1.104",
    "session_id": "sess-8a71b2"
  },
  "action": "SETPOINT_WRITE",
  "target": "external.Sterilizer01.TargetPressure",
  "details": {
    "old_value": 2.50,
    "new_value": 2.85,
    "engineering_unit": "barg",
    "reason_text": "Recipe step transition to Peak 3 holding condition"
  },
  "result": "SUCCESS"
}
```

---

## 6. Concrete Rust Security Model Implementation

```rust
//! mythos_security::rbac
//! IEC 62443 Role-Based Access Control and Cryptographic Audit Logger.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UserRole {
    Guest = 0,
    Operator = 1,
    Supervisor = 2,
    Engineer = 3,
    Admin = 4,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    ViewHmi,
    ViewAlarms,
    AckAlarms,
    ShelveAlarms,
    SetOutOfService,
    ToggleActuators,
    WriteSetpoints,
    ExecuteBatch,
    SignRecipe,
    EditRecipe,
    TunePid,
    HotReloadLogic,
    ConfigureDrivers,
    ExportHistorian,
    ManageUsers,
    TriggerBackup,
    RestoreBackup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPrincipal {
    pub principal_id: String,
    pub role: UserRole,
    pub is_service_agent: bool,
    pub granted_scopes: HashSet<String>,
}

impl SecurityPrincipal {
    pub fn has_permission(&self, permission: &Permission) -> bool {
        // Enforce role hierarchy rules
        match permission {
            Permission::ViewHmi | Permission::ViewAlarms => true,
            Permission::AckAlarms => self.role >= UserRole::Operator,
            Permission::ShelveAlarms => self.role >= UserRole::Operator,
            Permission::SetOutOfService => self.role >= UserRole::Engineer,
            Permission::ToggleActuators => self.role >= UserRole::Operator && !self.is_service_agent,
            Permission::WriteSetpoints => self.role >= UserRole::Operator,
            Permission::ExecuteBatch => self.role >= UserRole::Operator,
            Permission::SignRecipe => self.role >= UserRole::Supervisor && !self.is_service_agent,
            Permission::EditRecipe => self.role >= UserRole::Engineer && !self.is_service_agent,
            Permission::TunePid => self.role >= UserRole::Engineer,
            Permission::HotReloadLogic | Permission::ConfigureDrivers => self.role >= UserRole::Engineer && !self.is_service_agent,
            Permission::ExportHistorian => self.role >= UserRole::Operator,
            Permission::ManageUsers => self.role >= UserRole::Admin && !self.is_service_agent,
            Permission::TriggerBackup => self.role >= UserRole::Supervisor,
            Permission::RestoreBackup => self.role >= UserRole::Admin && !self.is_service_agent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub sequence_index: u64,
    pub prev_hash_sha256: String,
    pub record_hash_sha256: String,
    pub timestamp_utc: String,
    pub principal_id: String,
    pub role: UserRole,
    pub action: String,
    pub target: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub result: String,
}

pub struct AuditLedger {
    pub last_hash: String,
    pub current_sequence: u64,
}

impl AuditLedger {
    pub fn new(genesis_hash: &str, start_sequence: u64) -> Self {
        Self {
            last_hash: genesis_hash.to_string(),
            current_sequence: start_sequence,
        }
    }

    pub fn append_record(
        &mut self,
        timestamp_utc: &str,
        principal_id: &str,
        role: UserRole,
        action: &str,
        target: &str,
        old_val: Option<&str>,
        new_val: Option<&str>,
        result: &str,
    ) -> AuditRecord {
        self.current_sequence += 1;

        let mut hasher = Sha256::new();
        hasher.update(self.current_sequence.to_le_bytes());
        hasher.update(self.last_hash.as_bytes());
        hasher.update(timestamp_utc.as_bytes());
        hasher.update(principal_id.as_bytes());
        hasher.update(&[role as u8]);
        hasher.update(action.as_bytes());
        hasher.update(target.as_bytes());
        if let Some(ov) = old_val { hasher.update(ov.as_bytes()); }
        if let Some(nv) = new_val { hasher.update(nv.as_bytes()); }
        hasher.update(result.as_bytes());

        let record_hash = format!("{:x}", hasher.finalize());
        let record = AuditRecord {
            sequence_index: self.current_sequence,
            prev_hash_sha256: self.last_hash.clone(),
            record_hash_sha256: record_hash.clone(),
            timestamp_utc: timestamp_utc.to_string(),
            principal_id: principal_id.to_string(),
            role,
            action: action.to_string(),
            target: target.to_string(),
            old_value: old_val.map(|s| s.to_string()),
            new_value: new_val.map(|s| s.to_string()),
            result: result.to_string(),
        };

        self.last_hash = record_hash;
        record
    }
}
```

---

## 7. Verification & Acceptance Criteria
1. **Zero Privilege Escalation**: Service accounts cannot perform direct actuator toggling or administrative mutations, even if signed with highest-entropy credentials.
2. **Chain Integrity Verification**: A validation script running over 1,000,000 audit records verifies hash-chain integrity in $< 350\,\text{ms}$; single-bit alteration triggers immediate discrepancy detection.
3. **Defense Against Session Hijacking**: JWT tokens rotate every 15 minutes; revoked tokens are distributed instantly across shared memory block.
