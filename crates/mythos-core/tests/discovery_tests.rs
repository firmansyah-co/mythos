//! Unit and integration tests for Universal Discovery Engine & Modbus Heuristic Sweep (TSK-208 & TSK-209).

use mythos_core::catalog::DeviceCatalog;
use mythos_core::discovery::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_modbus_guided_discovery_and_catalog_fingerprint() {
    // 1. Load catalog manifests
    let catalog_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog/devices");
    let mut catalog = DeviceCatalog::new();
    catalog.load_directory(&catalog_root).unwrap();
    let catalog_arc = Arc::new(catalog);

    // 2. Set up mock Modbus transport simulating a Supmea P300 at slave ID 1
    // Probed register 0 (40001) returning value 1 (slave address)
    let transport =
        Arc::new(MockModbusTransport::new().with_device(1, &[(0, 1), (1, 3), (2, 3), (4, 2500)]));

    let discoverer = Arc::new(ModbusSweepDiscoverer::new(transport));
    let mut engine = DiscoveryEngine::new();
    engine.register(discoverer);

    // 3. Run Guided Discovery
    let ctx = DiscoveryContext::new("ttyUSB0", DiscoveryMode::GuidedDiscovery)
        .with_catalog(catalog_arc)
        .with_timeouts(Duration::from_millis(100), Duration::from_millis(2000));

    let results = engine
        .run_discovery(&ctx, Some("modbus-rtu"))
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    let dev = &results[0];
    assert_eq!(dev.modbus_slave_id, Some(1));
    assert_eq!(dev.protocol, "modbus-rtu");
    assert_eq!(dev.vendor.as_deref(), Some("Supmea Automation Co., Ltd."));
    assert_eq!(dev.fingerprint_model.as_deref(), Some("SUP-P300"));
    assert!(dev.confidence > 0.5);
}

#[tokio::test]
async fn test_discovery_cooperative_cancellation() {
    let transport = Arc::new(MockModbusTransport::new());
    let discoverer = Arc::new(ModbusSweepDiscoverer::new(transport));
    let mut engine = DiscoveryEngine::new();
    engine.register(discoverer);

    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();

    // Trigger cancellation before/during sweep
    cancel_token_clone.cancel();

    let ctx = DiscoveryContext::new("ttyUSB0", DiscoveryMode::BlindBusSweep)
        .with_cancellation_token(cancel_token)
        .with_timeouts(Duration::from_millis(50), Duration::from_millis(2000));

    let res = engine.run_discovery(&ctx, Some("modbus-rtu")).await;
    assert_eq!(res, Err(DiscoveryError::Cancelled));
}

#[tokio::test]
async fn test_discovery_timeout_enforcement() {
    // Transport that delays beyond total scan timeout
    struct SlowTransport;
    #[async_trait::async_trait]
    impl ModbusProbeTransport for SlowTransport {
        async fn probe_holding_register(
            &self,
            _slave_id: u8,
            _register_address: u16,
            _timeout_duration: Duration,
        ) -> Result<u16, String> {
            tokio::time::sleep(Duration::from_millis(500)).await;
            Err("Timeout".into())
        }
    }

    let transport = Arc::new(SlowTransport);
    let discoverer = Arc::new(ModbusSweepDiscoverer::new(transport));
    let mut engine = DiscoveryEngine::new();
    engine.register(discoverer);

    // Total timeout set to 50ms, smaller than single probe
    let ctx = DiscoveryContext::new("ttyUSB0", DiscoveryMode::GuidedDiscovery)
        .with_timeouts(Duration::from_millis(100), Duration::from_millis(50));

    let res = engine.run_discovery(&ctx, Some("modbus-rtu")).await;
    match res {
        Err(DiscoveryError::Timeout(_)) => {}
        other => panic!("Expected DiscoveryError::Timeout, got {:?}", other),
    }
}
