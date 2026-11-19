//! The data types' TypeScript definitions, generated with `ts-rs` into the workbench's source.

use mythos_core::datatype::{DataTypeEntry, DataTypeSummary};
use mythos_core::datatype::{UdtDefinition, UdtKind, UdtMember};
use std::fs;
use std::path::PathBuf;
use ts_rs::TS;

#[test]
fn the_data_types_are_exported_as_typescript() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Workspace root must exist");
    let target_dir = workspace_root.join("frontend/packages/ui-shell/src/types/generated");
    fs::create_dir_all(&target_dir).expect("Failed to create target generated types directory");

    // The UDT shape shared by data types and the HMI module's faceplate types
    UdtKind::export_all_to(&target_dir).expect("Failed to export UdtKind");
    UdtMember::export_all_to(&target_dir).expect("Failed to export UdtMember");
    UdtDefinition::export_all_to(&target_dir).expect("Failed to export UdtDefinition");
    DataTypeEntry::export_all_to(&target_dir).expect("Failed to export DataTypeEntry");
    DataTypeSummary::export_all_to(&target_dir).expect("Failed to export DataTypeSummary");

    let barrel_content = r#"// Auto-generated TypeScript definitions from MythOS Rust microkernel (TSK-302).
// Generated via `ts-rs` during `cargo test`. Do not edit manually.

export * from './UdtKind';
export * from './UdtMember';
export * from './UdtDefinition';
export * from './DataTypeEntry';
export * from './DataTypeSummary';
"#;
    fs::write(target_dir.join("index.ts"), barrel_content)
        .expect("Failed to write index.ts barrel");

    for name in [
        "UdtKind.ts",
        "UdtMember.ts",
        "UdtDefinition.ts",
        "DataTypeEntry.ts",
        "DataTypeSummary.ts",
        "index.ts",
    ] {
        assert!(target_dir.join(name).exists(), "{name}");
    }
}
