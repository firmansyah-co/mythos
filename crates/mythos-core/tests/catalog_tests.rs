//! Unit and integration tests for Open Hardware Catalog Parser Engine (TSK-204).

use mythos_core::catalog::*;
use std::path::PathBuf;

#[test]
fn test_parse_supmea_p300_manifest() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../catalog/devices/supmea-p300/manifest.json");
    let manifest = DeviceCatalog::load_manifest_file(&manifest_path)
        .expect("Failed to load Supmea P300 manifest");

    assert_eq!(manifest.schema_version, "1.0.0");
    assert_eq!(manifest.metadata.id, "supmea-p300");
    assert_eq!(manifest.metadata.vendor, "Supmea Automation Co., Ltd.");
    assert_eq!(manifest.metadata.category, DeviceCategory::Sensor);
    assert_eq!(manifest.registers.len(), 7);

    // Verify first register (Slave Address)
    let reg0 = &manifest.registers[0];
    assert_eq!(reg0.name, "Slave Address");
    assert_eq!(reg0.tag_suffix, "SLAVE_ID");
    assert_eq!(reg0.address_dec, 0);
    assert_eq!(reg0.access, AccessType::ReadWrite);
    assert_eq!(reg0.data_type, DataType::Uint16);

    // Verify pressure register with divisor scaling
    let pressure_reg = manifest
        .registers
        .iter()
        .find(|r| r.tag_suffix == "PRESSURE")
        .expect("PRESSURE register not found");
    assert_eq!(pressure_reg.unit, "bar");
    match pressure_reg.scaling {
        ScalingConfig::Divisor { divisor, offset } => {
            assert_eq!(divisor, 1000.0);
            assert_eq!(offset, 0.0);
        }
        _ => panic!("Expected Divisor scaling for pressure register"),
    }

    // Verify discovery config
    let discovery = manifest
        .discovery
        .as_ref()
        .expect("Missing discovery block");
    assert_eq!(discovery.probe_register, 0); // 40001 converts to 0
    assert_eq!(discovery.expected_function_code, "0x03");
    assert_eq!(discovery.priority_baudrates, vec![9600, 19200, 4800]);
}

#[test]
fn test_parse_sentec_rainfall_manifest() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../catalog/devices/sentec-rainfall/manifest.json");
    let manifest = DeviceCatalog::load_manifest_file(&manifest_path)
        .expect("Failed to load SenTec Rainfall manifest");

    assert_eq!(manifest.schema_version, "1.0.0");
    assert_eq!(manifest.metadata.id, "sentec-rainfall");
    assert_eq!(manifest.metadata.model, "ST-YL1");
    assert_eq!(manifest.registers.len(), 9);

    let rain_today = &manifest.registers[0];
    assert_eq!(rain_today.tag_suffix, "RAIN_TODAY");
    assert_eq!(rain_today.unit, "mm");
    assert_eq!(rain_today.access, AccessType::ReadOnly);
}

#[test]
fn test_load_catalog_directory() {
    let catalog_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/devices");
    let mut catalog = DeviceCatalog::new();
    let loaded = catalog
        .load_directory(&catalog_root)
        .expect("Failed to load catalog directory");

    assert!(loaded >= 2, "Expected at least 2 device manifests loaded");
    assert!(catalog.get("supmea-p300").is_some());
    assert!(catalog.get("sentec-rainfall").is_some());
}

#[test]
fn test_manifest_validation_rejects_zero_divisor() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../catalog/devices/supmea-p300/manifest.json");
    let mut manifest = DeviceCatalog::load_manifest_file(&manifest_path).unwrap();

    // Corrupt scaling divisor
    manifest.registers[4].scaling = ScalingConfig::Divisor {
        divisor: 0.0,
        offset: 0.0,
    };

    let mut catalog = DeviceCatalog::new();
    assert!(catalog.register(manifest).is_err());
}
