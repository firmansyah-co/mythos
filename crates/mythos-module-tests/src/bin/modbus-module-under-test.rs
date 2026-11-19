//! The Modbus module, built inside this package so its tests can locate the executable.

fn main() -> std::process::ExitCode {
    driver_modbus::module::main()
}
