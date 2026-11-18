//! The module binary as the server finds it beside itself: it says what it is when asked.
//!
//! This test also makes `cargo test --workspace` build the binary, which the server's own tests
//! start (`pairing_tests.rs` downloads a program to a node that runs it in this module).

use std::process::Command;

#[test]
fn the_binary_says_it_is_a_runtime_that_runs_vm_programs() {
    let output = Command::new(env!("CARGO_BIN_EXE_mythos-module-runtime-vm"))
        .arg(mythos_module::protocol::MANIFEST_FLAG)
        .output()
        .expect("the module binary runs");
    assert!(output.status.success(), "{output:?}");
    let manifest: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the manifest is JSON");
    assert_eq!(manifest["kind"], "runtime", "{manifest}");
    assert_eq!(manifest["provides"], "vm", "{manifest}");
    assert_eq!(manifest["id"], mythos_runtime_vm::MODULE_ID, "{manifest}");
}
