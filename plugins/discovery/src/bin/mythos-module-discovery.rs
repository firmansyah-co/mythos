//! `mythos-module-discovery`: announces this node and lists others over mDNS (ADR-012 §2.2).

use std::process::ExitCode;

fn main() -> ExitCode {
    mythos_module::run_service_module(
        mythos_discovery::manifest(),
        mythos_discovery::Discovery::default(),
    )
}
