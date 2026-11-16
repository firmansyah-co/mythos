//! Updating a MythOS installation: what the release feed publishes, installing a checked bundle
//! into `<home>/versions/<version>`, and moving `<home>/current` between installed versions.
//!
//! The `mythos` CLI and the server's Settings → Updates both use this, so the two cannot
//! disagree about an installation.

pub mod config;
pub mod install;
pub mod release;
pub mod rollback;
pub mod update;
pub mod versions;
