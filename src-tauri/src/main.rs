//! # MythOS Industrial Workbench - Tauri v2 Desktop Wrapper
//!
//! Spawns MythOS microkernel daemon on an embedded local TCP port,
//! serves the local UI shell via native webview, and handles clean shutdown hooks.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    mythos_core::server::init_logging();
    mythos_desktop_lib::run()
}
