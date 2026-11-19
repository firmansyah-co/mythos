//! `mythos-module-hmi`: the HMI service module (ADR-011). The server finds it beside itself.

use std::process::ExitCode;

fn main() -> ExitCode {
    mythos_module::run_service_module(mythos_hmi::manifest(), mythos_hmi::Hmi::default())
}
