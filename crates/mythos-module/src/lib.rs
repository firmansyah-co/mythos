//! # MythOS modules
//!
//! MythOS is assembled from modules. A module is a separate executable that the server starts,
//! talks to over a line-oriented JSON protocol on its standard streams, and can stop, restart
//! or replace while everything else keeps running.
//!
//! Running each module in its own process is what makes it replaceable and contained: a module
//! that crashes, hangs or is upgraded affects only the work it does. Rust has no stable ABI, so
//! a module loaded into the server's address space could neither be unloaded safely nor be
//! built by anything but the exact compiler that built the server.
//!
//! - [`protocol`]: the messages, pinned on the wire.
//! - [`serve`]: the module side. A driver crate turns its [`ProtocolDriver`] into a module
//!   executable with [`run_driver_module`].
//! - [`host`]: the server side. [`discover`] finds module executables and reads their
//!   manifests; [`ProcessDriver`] drives one as an ordinary [`ProtocolDriver`].
//!
//! [`ProtocolDriver`]: mythos_plugin_sdk::ProtocolDriver

pub mod host;
pub mod protocol;
pub mod serve;
pub mod settle;

pub use host::{
    discover, read_manifest, snapshot, Discovery, Fingerprint, ModuleBinary, ModuleProblem,
    ProcessDriver,
};
pub use protocol::{ModuleKind, ModuleManifest, MODULE_FILE_PREFIX, PROTOCOL_VERSION};
pub use serve::{run_driver_module, serve_driver};
pub use settle::Settle;
