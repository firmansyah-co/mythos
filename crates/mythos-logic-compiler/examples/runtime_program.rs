//! A program compiled for a runtime module, in the form a project's `runtimes.json` holds it,
//! for `scripts/e2e.sh` to load into a runtime over the API.
//!
//!   runtime_program <program.st> <profile>
//!
//! Compiles the program for the profile and prints
//! `{"name": "<program>", "code": "<bytecode, lowercase hex>", "symbols": [...]}`.

use mythos_logic_compiler::{compile, TargetProfile};
use std::fmt::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, source, profile] = args.as_slice() else {
        eprintln!("usage: runtime_program <program.st> <profile>");
        return ExitCode::from(2);
    };
    let Some(profile) = TargetProfile::by_name(profile) else {
        eprintln!("no target profile named {profile}");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{source}: {e}");
            return ExitCode::from(2);
        }
    };
    let compiled = match compile(&text, &profile) {
        Ok(c) => c,
        Err(diags) => {
            for d in diags {
                eprintln!("{source}:{d}");
            }
            return ExitCode::FAILURE;
        }
    };
    let mut code = String::with_capacity(compiled.bytecode.len() * 2);
    for byte in &compiled.bytecode {
        let _ = write!(code, "{byte:02x}");
    }
    println!(
        "{}",
        serde_json::json!({ "name": compiled.name, "code": code, "symbols": compiled.symbols })
    );
    ExitCode::SUCCESS
}
