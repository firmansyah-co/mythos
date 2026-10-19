//! Pre-built MCP Tools for MythOS Operations.
//!
//! Submodules:
//! - `tag_tools`: SCADA tag reading and writing.
//! - `alarm_tools`: ANSI/ISA-18.2 alarm queries, acknowledgments, and shelving.
//! - `control_tools`: Soft-logic compilation and cycle-synchronized hot-reload.
//! - `diag_tools`: Network discovery and RFC 4180 CSV telemetry export.

pub mod alarm_tools;
pub mod control_tools;
pub mod diag_tools;
pub mod tag_tools;

pub use alarm_tools::*;
pub use control_tools::*;
pub use diag_tools::*;
pub use tag_tools::*;

use crate::server::McpServer;

/// Register all standard MythOS MCP tools into the given server.
pub async fn register_all_tools(server: &McpServer) {
    tag_tools::register_mythos_read_tag(server).await;
    tag_tools::register_mythos_write_tag(server).await;
    alarm_tools::register_mythos_get_alarm_summary(server).await;
    alarm_tools::register_mythos_ack_alarm(server).await;
    alarm_tools::register_mythos_shelve_alarm(server).await;
    control_tools::register_mythos_compile_logic(server).await;
    control_tools::register_mythos_hot_reload_logic(server).await;
    diag_tools::register_mythos_discover_devices(server).await;
    diag_tools::register_mythos_export_telemetry_csv(server).await;
}
