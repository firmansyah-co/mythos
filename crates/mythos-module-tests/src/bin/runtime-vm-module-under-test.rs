//! Exactly the entry point of the shipped `mythos-module-runtime-vm`, built from this package so
//! its tests find it through `CARGO_BIN_EXE_*` (see the modbus one beside it for why the name
//! differs).

fn main() -> std::process::ExitCode {
    mythos_runtime_vm::main()
}
