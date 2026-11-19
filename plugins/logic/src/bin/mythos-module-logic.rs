//! The logic authoring module. Started by the MythOS server; see `mythos_module`.

fn main() -> std::process::ExitCode {
    mythos_module::run_service_module(mythos_logic::manifest(), mythos_logic::Logic::default())
}
