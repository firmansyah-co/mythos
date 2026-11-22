//! The data module: a project's user types and data blocks. Started by the MythOS server; see
//! `mythos_module`.

fn main() -> std::process::ExitCode {
    mythos_module::run_service_module(mythos_data::manifest(), mythos_data::Data::default())
}
