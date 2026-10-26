//! MCP Tools for Network Discovery and Telemetry Historian Export.
//!
//! Exposes:
//! - `mythos_discover_devices`: Executes Layer 2 DCP and Modbus sweeps.
//! - `mythos_export_telemetry_csv`: Generates RFC 4180 CSV historical reports.

use mythos_core::auth::scopes;
use mythos_core::discovery::{DiscoveryContext, DiscoveryMode, DiscoveryTimeoutBounds};
use mythos_core::historian::{CsvExportRequest, CsvExporter, ResampleStrategy};
use mythos_mcp_macros::mythos_mcp_tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::context::McpContext;
use crate::protocol::CallToolResult;
use crate::McpError;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct DiscoverDevicesArgs {
    /// Target network interface name (e.g. "eth0", "enp3s0")
    pub interface: Option<String>,
    /// Optional protocol filter ("profinet-dcp", "modbus-rtu", "modbus-tcp", "all")
    pub protocol: Option<String>,
    /// Sweep mode ("guided", "blind")
    pub sweep_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExportTelemetryCsvArgs {
    /// List of tag URIs to export
    pub tags: Vec<String>,
    /// Start timestamp in nanoseconds since UNIX epoch
    pub start_time_ns: u64,
    /// End timestamp in nanoseconds since UNIX epoch
    pub end_time_ns: u64,
    /// Uniform sample interval in milliseconds (e.g. 1000 for 1s intervals)
    pub interval_ms: u64,
    /// Resample strategy ("RAW", "FORWARD_FILL", "LINEAR_INTERPOLATION", "TIME_WEIGHTED_AVERAGE")
    pub resample_strategy: Option<String>,
}

/// Discover industrial devices across physical Ethernet Layer 2 and serial field networks.
#[mythos_mcp_tool(
    name = "mythos_discover_devices",
    category = "network_discovery",
    safety_level = "operator_control"
)]
pub async fn mythos_discover_devices(
    ctx: McpContext,
    args: DiscoverDevicesArgs,
) -> Result<CallToolResult, McpError> {
    ctx.check_permission(scopes::DEVICE_DISCOVER)?;

    let iface = args.interface.unwrap_or_else(|| "eth0".to_string());
    let proto_filter = args.protocol.as_deref().unwrap_or("all");

    let mode = match args
        .sweep_mode
        .as_deref()
        .unwrap_or("guided")
        .to_lowercase()
        .as_str()
    {
        "blind" => DiscoveryMode::BlindBusSweep,
        _ => DiscoveryMode::GuidedDiscovery,
    };

    let disc_ctx = DiscoveryContext {
        mode,
        target_interface: iface.clone(),
        timeout_bounds: DiscoveryTimeoutBounds::default(),
        cancel_token: CancellationToken::new(),
        catalog: None,
    };

    // No engine attached means no scan was performed. That is NOT "the network is
    // empty", so it must not be reported as a successful zero-device result: a
    // caller reading `devices: []` cannot tell the two apart. It is reported as an
    // error (`is_error: true`) whose body carries the reason, and never as a device.
    //
    // This branch used to return an invented Siemens ET200SP with real-looking MAC,
    // IP, article number and confidence 1.0. It is deleted outright rather than
    // gated behind a cfg/feature flag, because a gate is how such a payload comes
    // back: the honest answer to "no discovery engine" does not depend on a build
    // profile.
    let Some(engine) = &ctx.discovery_engine else {
        let result = serde_json::json!({
            "status": "unavailable",
            "reason": "no discovery engine is attached to this MCP context; no scan was performed",
            "interface": iface,
            "protocol": proto_filter,
            "count": 0,
            "devices": [],
        });
        return CallToolResult::error_json(&result)
            .map_err(|e| McpError::InternalError(e.to_string()));
    };

    let filter = if proto_filter == "all" {
        None
    } else {
        Some(proto_filter)
    };

    // A failed scan is also not an empty network. `unwrap_or_default()` used to turn
    // every DiscoveryError (cancelled, bad interface, unregistered protocol) into an
    // empty device list, which reads exactly like a completed sweep that found nothing.
    let discovered_devices = match engine.run_discovery(&disc_ctx, filter).await {
        Ok(devices) => devices,
        Err(e) => {
            let result = serde_json::json!({
                "status": "failed",
                "reason": format!("discovery scan failed: {e}"),
                "interface": iface,
                "protocol": proto_filter,
                "count": 0,
                "devices": [],
            });
            return CallToolResult::error_json(&result)
                .map_err(|err| McpError::InternalError(err.to_string()));
        }
    };

    let result = serde_json::json!({
        "status": "complete",
        "interface": iface,
        "protocol": proto_filter,
        "count": discovered_devices.len(),
        "devices": discovered_devices,
    });

    CallToolResult::ok_json(&result).map_err(|e| McpError::InternalError(e.to_string()))
}

/// Export historical process telemetry for specified tags to standard RFC 4180 CSV.
#[mythos_mcp_tool(
    name = "mythos_export_telemetry_csv",
    category = "historian",
    safety_level = "read_only"
)]
pub async fn mythos_export_telemetry_csv(
    ctx: McpContext,
    args: ExportTelemetryCsvArgs,
) -> Result<CallToolResult, McpError> {
    ctx.check_permission(scopes::TELEMETRY_EXPORT)?;

    let strategy = match args
        .resample_strategy
        .as_deref()
        .unwrap_or("FORWARD_FILL")
        .to_uppercase()
        .as_str()
    {
        "RAW" => ResampleStrategy::Raw,
        "LINEAR_INTERPOLATION" => ResampleStrategy::LinearInterpolation,
        "TIME_WEIGHTED_AVERAGE" => ResampleStrategy::TimeWeightedAverage,
        _ => ResampleStrategy::ForwardFill,
    };

    let req = CsvExportRequest {
        start_time_ns: args.start_time_ns,
        end_time_ns: args.end_time_ns,
        tag_names: args.tags.clone(),
        sample_interval_ms: args.interval_ms.max(10),
        resample_strategy: strategy,
        include_headers: true,
        delimiter: ',',
    };

    let buffer = ctx.historian_buffer.read().await;
    let mut out_bytes = Vec::new();

    let btree_series: std::collections::BTreeMap<
        String,
        Vec<mythos_core::historian::TimeSeriesPoint>,
    > = buffer
        .series
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    CsvExporter::export_csv(&mut out_bytes, &req, &btree_series)
        .map_err(|e| McpError::ToolExecutionFailed(format!("CSV export failed: {}", e)))?;

    let csv_string = String::from_utf8(out_bytes)
        .map_err(|e| McpError::InternalError(format!("Invalid UTF-8 in CSV: {}", e)))?;

    Ok(CallToolResult::ok_text(csv_string))
}
