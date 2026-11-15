//! The Modbus TCP driver module. Started by the MythOS server; see `mythos_module`.

fn main() -> std::process::ExitCode {
    driver_modbus::module::main()
}
