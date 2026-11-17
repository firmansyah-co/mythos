//! Exactly the entry point of the shipped `mythos-module-logic`, built from this package so its
//! tests find it through `CARGO_BIN_EXE_*` (see the modbus one beside it for why the name differs).

use std::process::ExitCode;

fn main() -> ExitCode {
    mythos_module::run_service_module(mythos_logic::manifest(), mythos_logic::Logic::default())
}
