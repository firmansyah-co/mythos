//! Field I/O: drivers poll devices, one scanner writes what they report to the tag bus.
//!
//! ```text
//!   device ──▶ driver task ──▶ samples ──▶ scanner task ──▶ tag bus ──▶ telemetry stream
//!              (one per                    (the only
//!               connection)                 writer)
//! ```
//!
//! A driver returns a sample for every tag it declared: a typed value, or the fault that
//! prevented a reading. The scanner turns that into a slot write. Nothing on this path
//! substitutes a value: a tag that could not be read keeps whatever was last read from the
//! device and carries a bad quality, and a tag that was never read has no type and no value.

pub mod config;
pub mod manager;
pub mod runtime;

pub use config::{ConnectionConfig, IoConfig, IoConfigError, IO_CONFIG_FILE};
pub use manager::{ConfigProblem, IoManager};
pub use runtime::{
    ConnectionState, ConnectionStatus, DriverRegistry, IoRuntime, IoStatus, IoStatusHandle,
    TagFaultStatus,
};
