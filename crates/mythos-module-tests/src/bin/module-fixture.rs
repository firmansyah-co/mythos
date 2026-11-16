//! A stand-in for a module that misbehaves, for tests that need one on every platform.
//!
//! It does what a file beside it says: `<its own file name>.fixture.json`, holding
//! `{"manifest": "<text to print for --manifest>", "manifest_exit": <code>, "serve_exit": <code>}`.
//! A test copies it under a module's name and writes that file next to the copy. Shell scripts
//! did this job before, and Windows cannot run them.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let exe = std::env::current_exe().expect("own path");
    let mut sidecar = exe.clone().into_os_string();
    sidecar.push(".fixture.json");
    let text = std::fs::read_to_string(&sidecar)
        .unwrap_or_else(|e| panic!("{}: {e}", std::path::Path::new(&sidecar).display()));
    let script: serde_json::Value = serde_json::from_str(&text).expect("fixture file is JSON");
    let code = |key: &str| script[key].as_u64().unwrap_or(0) as u8;

    if std::env::args().nth(1).as_deref() == Some("--manifest") {
        if let Some(manifest) = script["manifest"].as_str() {
            let mut out = std::io::stdout();
            let _ = out.write_all(manifest.as_bytes());
            let _ = out.flush();
        }
        return ExitCode::from(code("manifest_exit"));
    }
    ExitCode::from(code("serve_exit"))
}
