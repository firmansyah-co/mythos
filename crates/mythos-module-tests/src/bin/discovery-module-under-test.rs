//! Exactly the entry point of the shipped `mythos-module-discovery`, built from this package so
//! its tests find it through `CARGO_BIN_EXE_*`.

use std::process::ExitCode;

fn main() -> ExitCode {
    mythos_module::run_service_module(
        mythos_discovery::manifest(),
        mythos_discovery::Discovery::default(),
    )
}
