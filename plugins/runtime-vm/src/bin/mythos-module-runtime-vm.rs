//! The logic runtime for a PC. Started by the MythOS server; see `mythos_module`.

fn main() -> std::process::ExitCode {
    mythos_runtime_vm::main()
}
