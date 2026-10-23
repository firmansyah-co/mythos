//! MCP Tools for ANSI/ISA-18.2 Alarm Subsystem.
//!
//! Exposes:
//! - `mythos_get_alarm_summary`: Queries active, unacknowledged, and shelved alarms.
//! - `mythos_ack_alarm`: Acknowledges an active industrial alarm per ISA-18.2 state machine.
//! - `mythos_shelve_alarm`: Shelves an alarm with duration and operator reason.

use mythos_core::alarm::{AlarmEventTrigger, AlarmState};
use mythos_core::auth::scopes;
use mythos_mcp_macros::mythos_mcp_tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::McpContext;
use crate::protocol::CallToolResult;
use crate::McpError;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct GetAlarmSummaryArgs {
    /// Optional filter by state ("ACTIVE", "UNACK", "SHELVED", "ALL")
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AckAlarmArgs {
    /// Numerical Alarm ID to acknowledge
    pub alarm_id: u32,
    /// Reason or commentary for acknowledgment
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShelveAlarmArgs {
    /// Numerical Alarm ID to shelve
    pub alarm_id: u32,
    /// Shelve duration in seconds (max 86400)
    pub duration_seconds: u32,
    /// Reason explaining why the alarm is shelved
    pub reason: String,
}

/// Query summary of active, unacknowledged, and shelved alarms across the plant.
#[mythos_mcp_tool(
    name = "mythos_get_alarm_summary",
    category = "scada_alarm",
    safety_level = "read_only"
)]
pub async fn mythos_get_alarm_summary(
    ctx: McpContext,
    args: GetAlarmSummaryArgs,
) -> Result<CallToolResult, McpError> {
    ctx.check_permission(scopes::TAG_READ)?;

    let registry = ctx.alarm_registry.read().await;
    let filter = args.filter.as_deref().unwrap_or("ALL").to_uppercase();

    let mut alarms_list = Vec::new();
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    for (id, state) in &registry.states {
        let def = registry.definitions.get(id);

        let matches_filter = match filter.as_str() {
            "ACTIVE" => state.is_active_condition,
            "UNACK" => {
                state.current_state == AlarmState::UnackAlarm
                    || state.current_state == AlarmState::UnackRtn
            }
            "SHELVED" => state.current_state == AlarmState::Shelved,
            _ => true,
        };

        if matches_filter {
            alarms_list.push(serde_json::json!({
                "alarm_id": id,
                "state": state.current_state.to_string(),
                "is_active_condition": state.is_active_condition,
                "severity": def.map(|d| d.severity.to_string()).unwrap_or_else(|| "UNKNOWN".to_string()),
                "tag_name": def.map(|d| d.tag_name.clone()).unwrap_or_default(),
                "description": def.map(|d| d.description.clone()).unwrap_or_default(),
                "last_state_change_ns": state.last_state_change_ns,
                "shelve_expiration_ns": state.shelve_expiration_ns,
            }));
        }
    }

    let result = serde_json::json!({
        "timestamp_ns": now_ns,
        "total_alarms": alarms_list.len(),
        "filter_applied": filter,
        "alarms": alarms_list,
    });

    CallToolResult::ok_json(&result).map_err(|e| McpError::InternalError(e.to_string()))
}

/// Acknowledge an active industrial alarm conforming to ANSI/ISA-18.2 state machine.
#[mythos_mcp_tool(
    name = "mythos_ack_alarm",
    category = "scada_alarm",
    safety_level = "operator_control"
)]
pub async fn mythos_ack_alarm(
    ctx: McpContext,
    args: AckAlarmArgs,
) -> Result<CallToolResult, McpError> {
    ctx.check_permission(scopes::ALARM_ACK)?;

    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    let mut registry = ctx.alarm_registry.write().await;
    let def = registry
        .definitions
        .get(&args.alarm_id)
        .cloned()
        .ok_or_else(|| {
            McpError::ToolNotFound(format!(
                "Alarm definition for ID {} not found",
                args.alarm_id
            ))
        })?;

    let state = registry.states.get_mut(&args.alarm_id).ok_or_else(|| {
        McpError::ToolNotFound(format!(
            "Alarm runtime state for ID {} not found",
            args.alarm_id
        ))
    })?;

    let prev_state = state.current_state;
    let transition = state.step(
        &def,
        AlarmEventTrigger::OperatorAcknowledge {
            user_id: ctx.caller_id().to_string(),
            timestamp_ns: now_ns,
        },
    );

    let new_state = state.current_state;

    // Audit logging
    if let Some(ledger) = ctx.audit_ledger() {
        let _ = ledger.append(
            ctx.caller_id(),
            ctx.role(),
            "ack_alarm",
            format!("ALM-{}", args.alarm_id),
            serde_json::json!({
                "alarm_id": args.alarm_id,
                "previous_state": prev_state.to_string(),
                "new_state": new_state.to_string(),
                "reason": args.reason.unwrap_or_else(|| "Acknowledged by AI agent".to_string()),
            }),
        );
    }

    let result = serde_json::json!({
        "success": true,
        "alarm_id": args.alarm_id,
        "previous_state": prev_state.to_string(),
        "new_state": new_state.to_string(),
        "transitioned": transition.is_some(),
    });

    CallToolResult::ok_json(&result).map_err(|e| McpError::InternalError(e.to_string()))
}

/// Shelve an alarm with a defined duration and mandatory justification.
#[mythos_mcp_tool(
    name = "mythos_shelve_alarm",
    category = "scada_alarm",
    safety_level = "supervisor_control"
)]
pub async fn mythos_shelve_alarm(
    ctx: McpContext,
    args: ShelveAlarmArgs,
) -> Result<CallToolResult, McpError> {
    ctx.check_permission(scopes::ALARM_SHELVE)?;

    if args.duration_seconds == 0 || args.duration_seconds > 86400 {
        return Err(McpError::InvalidArguments(
            "Shelve duration must be between 1 and 86400 seconds (24 hours)".to_string(),
        ));
    }

    if args.reason.trim().is_empty() {
        return Err(McpError::InvalidArguments(
            "Shelve reason is mandatory under ANSI/ISA-18.2".to_string(),
        ));
    }

    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    let mut registry = ctx.alarm_registry.write().await;
    let def = registry
        .definitions
        .get(&args.alarm_id)
        .cloned()
        .ok_or_else(|| {
            McpError::ToolNotFound(format!(
                "Alarm definition for ID {} not found",
                args.alarm_id
            ))
        })?;

    let state = registry.states.get_mut(&args.alarm_id).ok_or_else(|| {
        McpError::ToolNotFound(format!(
            "Alarm runtime state for ID {} not found",
            args.alarm_id
        ))
    })?;

    let prev_state = state.current_state;
    let transition = state.step(
        &def,
        AlarmEventTrigger::OperatorShelve {
            user_id: ctx.caller_id().to_string(),
            duration_sec: args.duration_seconds,
            timestamp_ns: now_ns,
        },
    );

    let new_state = state.current_state;

    // Audit logging
    if let Some(ledger) = ctx.audit_ledger() {
        let _ = ledger.append(
            ctx.caller_id(),
            ctx.role(),
            "shelve_alarm",
            format!("ALM-{}", args.alarm_id),
            serde_json::json!({
                "alarm_id": args.alarm_id,
                "previous_state": prev_state.to_string(),
                "new_state": new_state.to_string(),
                "duration_seconds": args.duration_seconds,
                "reason": args.reason,
            }),
        );
    }

    let result = serde_json::json!({
        "success": true,
        "alarm_id": args.alarm_id,
        "previous_state": prev_state.to_string(),
        "new_state": new_state.to_string(),
        "shelved_until_ns": state.shelve_expiration_ns,
        "transitioned": transition.is_some(),
    });

    CallToolResult::ok_json(&result).map_err(|e| McpError::InternalError(e.to_string()))
}
