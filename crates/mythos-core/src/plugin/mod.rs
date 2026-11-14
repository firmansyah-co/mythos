//! Plugin package format: build, verify and unpack `.mythos-plugin` archives.
//!
//! This is packaging only. Nothing loads a package at run time: drivers are compiled into the
//! server and listed in [`crate::io::DriverRegistry`].

pub mod packager;

pub use packager::{PluginPackageManifest, PluginPackager, VerifiedPluginPackage};
