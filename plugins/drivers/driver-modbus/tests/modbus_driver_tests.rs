//! The driver against a simulated Modbus TCP device: real sockets, real frames.

use driver_modbus::sim::ModbusSimServer;
use driver_modbus::ModbusDriver;
use mythos_plugin_sdk::*;
use serde_json::json;
use std::collections::HashSet;

fn ctx() -> PluginContext {
    let mut caps = HashSet::new();
    caps.insert("network:client".to_string());
    PluginContext::new("org.mythos.driver.modbus", caps, "/tmp")
}

fn reading<'a>(samples: &'a [TagSample], uri: &str) -> &'a Result<SampleValue, SampleFault> {
    &samples
        .iter()
        .find(|s| s.uri == uri)
        .unwrap_or_else(|| panic!("no sample for {uri}"))
        .reading
}

async fn started(config: serde_json::Value) -> ModbusDriver {
    let mut driver = ModbusDriver::new();
    driver.init(&ctx()).await.unwrap();
    driver.start(&ctx(), &config).await.unwrap();
    driver
}

#[tokio::test]
async fn reads_each_register_type_and_reports_one_sample_per_declared_tag() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(100, 452).await; // u16, scaled by 0.1
    sim.set_holding(101, (-7i16) as u16).await; // i16
    let bits = 21.5f32.to_bits();
    sim.set_holding(102, (bits >> 16) as u16).await; // f32, high word first
    sim.set_holding(103, (bits & 0xFFFF) as u16).await;
    sim.set_holding(104, 0x5678).await; // u32, low word first: 0x1234_5678
    sim.set_holding(105, 0x1234).await;
    sim.set_input(7, 65000).await; // input register, u16

    let mut driver = started(json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": sim.port(),
        "slave_id": 1,
        "timeout_ms": 1000,
        "mappings": [
            {"uri": "external.BoilerRoom.PT101.pressure", "address": 100, "function_code": 3, "scale": 0.1},
            {"uri": "external.BoilerRoom.TT101.offset", "address": 101, "function_code": 3, "data_type": "i16"},
            {"uri": "external.BoilerRoom.TT101.temperature", "address": 102, "function_code": 3, "data_type": "f32"},
            {"uri": "external.BoilerRoom.FQ101.total", "address": 104, "function_code": 3, "data_type": "u32", "word_order": "low_first"},
            {"uri": "external.BoilerRoom.LT101.level", "address": 7, "function_code": 4}
        ]
    }))
    .await;

    let declared = driver.declared_tags();
    let samples = driver.poll_cycle(&ctx()).await.unwrap();

    let declared_uris: Vec<&str> = declared.iter().map(|d| d.uri.as_str()).collect();
    let sample_uris: Vec<&str> = samples.iter().map(|s| s.uri.as_str()).collect();
    assert_eq!(sample_uris, declared_uris);

    match reading(&samples, "external.BoilerRoom.PT101.pressure") {
        Ok(SampleValue::F64(v)) => assert!((v - 45.2).abs() < 1e-9, "got {v}"),
        other => panic!("scaled u16 must be an f64, got {other:?}"),
    }
    assert_eq!(
        reading(&samples, "external.BoilerRoom.TT101.offset"),
        &Ok(SampleValue::I16(-7))
    );
    assert_eq!(
        reading(&samples, "external.BoilerRoom.TT101.temperature"),
        &Ok(SampleValue::F32(21.5))
    );
    assert_eq!(
        reading(&samples, "external.BoilerRoom.FQ101.total"),
        &Ok(SampleValue::U32(0x1234_5678))
    );
    assert_eq!(
        reading(&samples, "external.BoilerRoom.LT101.level"),
        &Ok(SampleValue::U16(65000))
    );

    assert_eq!(
        driver.health_check().await.unwrap().status,
        HealthStatus::Healthy
    );
}

#[tokio::test]
async fn an_unmapped_register_is_a_config_fault_and_does_not_poison_its_neighbours() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(1, 11).await;
    sim.set_holding(3, 33).await;
    // Address 2 is never set: the simulated device answers exception 2 for it.

    let mut driver = started(json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": sim.port(),
        "mappings": [
            {"uri": "external.Plant.Dev.a", "address": 1, "function_code": 3},
            {"uri": "external.Plant.Dev.b", "address": 2, "function_code": 3},
            {"uri": "external.Plant.Dev.c", "address": 3, "function_code": 3}
        ]
    }))
    .await;

    let samples = driver.poll_cycle(&ctx()).await.unwrap();

    assert_eq!(
        reading(&samples, "external.Plant.Dev.a"),
        &Ok(SampleValue::U16(11))
    );
    assert!(
        matches!(
            reading(&samples, "external.Plant.Dev.b"),
            Err(SampleFault::ConfigError(_))
        ),
        "got {:?}",
        reading(&samples, "external.Plant.Dev.b")
    );
    assert_eq!(
        reading(&samples, "external.Plant.Dev.c"),
        &Ok(SampleValue::U16(33))
    );
}

#[tokio::test]
async fn a_dead_link_faults_every_tag_and_never_returns_a_number() {
    // Bind a port, then release it, so nothing is listening there.
    let port = {
        let probe = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
        let port = probe.port();
        probe.stop();
        port
    };
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut driver = started(json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": port,
        "timeout_ms": 300,
        "mappings": [
            {"uri": "external.Plant.Dev.a", "address": 1, "function_code": 3},
            {"uri": "external.Plant.Dev.b", "address": 2, "function_code": 3}
        ]
    }))
    .await;

    let samples = driver.poll_cycle(&ctx()).await.unwrap();
    assert_eq!(samples.len(), 2);
    for sample in &samples {
        assert!(
            matches!(sample.reading, Err(SampleFault::CommFailure(_))),
            "{} must be a comm failure, got {:?}",
            sample.uri,
            sample.reading
        );
    }
    assert_ne!(
        driver.health_check().await.unwrap().status,
        HealthStatus::Healthy
    );
}

#[tokio::test]
async fn a_link_that_drops_after_a_good_read_turns_into_a_fault_then_recovers() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    let port = sim.port();
    sim.set_holding(5, 500).await;

    let mut driver = started(json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": port,
        "timeout_ms": 300,
        "mappings": [{"uri": "external.Plant.Dev.a", "address": 5, "function_code": 3}]
    }))
    .await;

    let first = driver.poll_cycle(&ctx()).await.unwrap();
    assert_eq!(
        reading(&first, "external.Plant.Dev.a"),
        &Ok(SampleValue::U16(500))
    );

    sim.stop();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let second = driver.poll_cycle(&ctx()).await.unwrap();
    assert!(
        matches!(
            reading(&second, "external.Plant.Dev.a"),
            Err(SampleFault::CommFailure(_))
        ),
        "the value read before the drop must not be repeated, got {:?}",
        reading(&second, "external.Plant.Dev.a")
    );

    // A device back on the same port is read again without restarting the driver.
    let sim2 = ModbusSimServer::start(&format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    sim2.set_holding(5, 777).await;
    let third = driver.poll_cycle(&ctx()).await.unwrap();
    assert_eq!(
        reading(&third, "external.Plant.Dev.a"),
        &Ok(SampleValue::U16(777))
    );
}

#[tokio::test]
async fn writes_a_holding_register_and_refuses_what_it_cannot_write_exactly() {
    let sim = ModbusSimServer::start("127.0.0.1:0").await.unwrap();
    sim.set_holding(100, 452).await;
    sim.set_input(7, 1).await;

    let mut driver = started(json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": sim.port(),
        "mappings": [
            {"uri": "external.BoilerRoom.PT101.setpoint", "address": 100, "function_code": 3, "scale": 0.1},
            {"uri": "external.BoilerRoom.LT101.level", "address": 7, "function_code": 4}
        ]
    }))
    .await;

    // 50.0 with a 0.1 scale is register value 500.
    driver
        .write_tag(&ctx(), "external.BoilerRoom.PT101.setpoint", &json!(50.0))
        .await
        .unwrap();
    assert_eq!(sim.holding(100).await, Some(500));

    // Out of range for a u16 register: refused, and the register keeps its value.
    assert!(driver
        .write_tag(&ctx(), "external.BoilerRoom.PT101.setpoint", &json!(9000.0))
        .await
        .is_err());
    assert!(driver
        .write_tag(&ctx(), "external.BoilerRoom.PT101.setpoint", &json!(-1.0))
        .await
        .is_err());
    assert_eq!(sim.holding(100).await, Some(500));

    // An input register is read-only.
    assert!(driver
        .write_tag(&ctx(), "external.BoilerRoom.LT101.level", &json!(1))
        .await
        .is_err());
    // An unmapped tag.
    assert!(driver
        .write_tag(&ctx(), "external.BoilerRoom.Nope.x", &json!(1))
        .await
        .is_err());
}

#[tokio::test]
async fn rejects_configurations_it_cannot_serve_honestly() {
    let mut driver = ModbusDriver::new();
    driver.init(&ctx()).await.unwrap();

    let cases = [
        (
            "RTU has no transport",
            json!({"transport": "rtu", "serial_port": "/dev/ttyUSB0", "mappings": []}),
        ),
        (
            "TCP without a host",
            json!({"transport": "tcp", "mappings": []}),
        ),
        (
            "tag outside the external namespace",
            json!({"transport": "tcp", "host": "127.0.0.1", "mappings": [
                {"uri": "internal.BoilerRoom.PT101.pv", "address": 1, "function_code": 3}]}),
        ),
        (
            "protocol name used as a namespace",
            json!({"transport": "tcp", "host": "127.0.0.1", "mappings": [
                {"uri": "external.modbus.device1.val", "address": 1, "function_code": 3}]}),
        ),
        (
            "the same tag mapped twice",
            json!({"transport": "tcp", "host": "127.0.0.1", "mappings": [
                {"uri": "external.Plant.Dev.a", "address": 1, "function_code": 3},
                {"uri": "external.Plant.Dev.a", "address": 2, "function_code": 3}]}),
        ),
        (
            "a write function code on a read mapping",
            json!({"transport": "tcp", "host": "127.0.0.1", "mappings": [
                {"uri": "external.Plant.Dev.a", "address": 1, "function_code": 6}]}),
        ),
        (
            "an unknown register type",
            json!({"transport": "tcp", "host": "127.0.0.1", "mappings": [
                {"uri": "external.Plant.Dev.a", "address": 1, "function_code": 3, "data_type": "f64"}]}),
        ),
        (
            "a zero scale",
            json!({"transport": "tcp", "host": "127.0.0.1", "mappings": [
                {"uri": "external.Plant.Dev.a", "address": 1, "function_code": 3, "scale": 0.0}]}),
        ),
        (
            "slave id 0",
            json!({"transport": "tcp", "host": "127.0.0.1", "slave_id": 0, "mappings": []}),
        ),
    ];

    for (what, config) in cases {
        assert!(
            driver.start(&ctx(), &config).await.is_err(),
            "must reject: {what}"
        );
        assert!(
            driver.declared_tags().is_empty(),
            "a rejected configuration must declare no tags: {what}"
        );
    }
}

#[tokio::test]
async fn requires_the_network_capability() {
    let no_caps = PluginContext::new("org.mythos.driver.modbus", HashSet::new(), "/tmp");
    let mut driver = ModbusDriver::new();
    assert!(driver.init(&no_caps).await.is_err());
}
