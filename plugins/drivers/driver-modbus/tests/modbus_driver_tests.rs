use driver_modbus::ModbusDriver;
use mythos_plugin_sdk::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Lightweight In-Memory Simulated Modbus TCP Slave Server
struct SimulatedModbusSlave {
    port: u16,
    registers: Arc<Mutex<HashMap<u16, u16>>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SimulatedModbusSlave {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let registers = Arc::new(Mutex::new(HashMap::new()));
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let regs_clone = registers.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    Ok((mut socket, _)) = listener.accept() => {
                        let regs = regs_clone.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 256];
                            loop {
                                let n = match socket.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(n) => n,
                                    Err(_) => break,
                                };

                                if n < 12 {
                                    continue;
                                }

                                let tx_id = [buf[0], buf[1]];
                                let unit_id = buf[6];
                                let func = buf[7];

                                match func {
                                    // Read Holding Registers (FC 03)
                                    0x03 => {
                                        let addr = u16::from_be_bytes([buf[8], buf[9]]);
                                        let count = u16::from_be_bytes([buf[10], buf[11]]);
                                        let reg_lock = regs.lock().await;

                                        let mut resp = Vec::new();
                                        resp.extend_from_slice(&tx_id);
                                        resp.extend_from_slice(&[0, 0]); // Proto ID
                                        let byte_count = (count * 2) as u8;
                                        let length = (byte_count as u16) + 3;
                                        resp.extend_from_slice(&length.to_be_bytes());
                                        resp.push(unit_id);
                                        resp.push(0x03);
                                        resp.push(byte_count);

                                        for i in 0..count {
                                            let val: u16 = *reg_lock.get(&(addr + i)).unwrap_or(&0);
                                            resp.extend_from_slice(&val.to_be_bytes());
                                        }

                                        let _ = socket.write_all(&resp).await;
                                    }
                                    // Write Single Register (FC 06)
                                    0x06 => {
                                        let addr = u16::from_be_bytes([buf[8], buf[9]]);
                                        let val = u16::from_be_bytes([buf[10], buf[11]]);
                                        {
                                            let mut reg_lock = regs.lock().await;
                                            reg_lock.insert(addr, val);
                                        }

                                        // Echo back
                                        let mut resp = Vec::new();
                                        resp.extend_from_slice(&tx_id);
                                        resp.extend_from_slice(&[0, 0]);
                                        resp.extend_from_slice(&6u16.to_be_bytes());
                                        resp.push(unit_id);
                                        resp.push(0x06);
                                        resp.extend_from_slice(&addr.to_be_bytes());
                                        resp.extend_from_slice(&val.to_be_bytes());

                                        let _ = socket.write_all(&resp).await;
                                    }
                                    _ => {}
                                }
                            }
                        });
                    }
                }
            }
        });

        Self {
            port,
            registers,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    async fn set_register(&self, addr: u16, val: u16) {
        let mut lock = self.registers.lock().await;
        lock.insert(addr, val);
    }

    async fn get_register(&self, addr: u16) -> u16 {
        let lock = self.registers.lock().await;
        *lock.get(&addr).unwrap_or(&0)
    }

    fn stop(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn test_modbus_driver_full_integration() {
    let slave = SimulatedModbusSlave::start().await;
    // Set Register 100 to 452 (representing 45.2 bar pressure with 0.1 scale)
    slave.set_register(100, 452).await;

    let mut caps = HashSet::new();
    caps.insert("tagbus:write".to_string());
    caps.insert("network:client".to_string());
    let ctx = PluginContext::new("org.mythos.driver.modbus", caps, "/tmp");

    let mut driver = ModbusDriver::new();
    assert!(driver.init(&ctx).await.is_ok());

    let config = json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": slave.port,
        "slave_id": 1,
        "timeout_ms": 1000,
        "mappings": [
            {
                "uri": "external.BoilerRoom.PT101.pressure.pv",
                "address": 100,
                "function_code": 3,
                "data_type": "u16",
                "scale": 0.1
            }
        ]
    });

    assert!(driver.start(&ctx, &config).await.is_ok());

    let health = driver.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);

    // Poll cycle
    let updated = driver.poll_cycle(&ctx).await.unwrap();
    assert_eq!(updated, 1);

    // Verify cached tag value: 452 * 0.1 = 45.2
    let val = driver
        .get_cached_value("external.BoilerRoom.PT101.pressure.pv")
        .unwrap();
    assert!((val - 45.2).abs() < 1e-4);

    // Test writing to tag (e.g. setpoint 50.0 bar -> register should receive 500)
    let write_res = driver
        .write_tag(&ctx, "external.BoilerRoom.PT101.pressure.pv", &json!(50.0))
        .await;
    assert!(write_res.is_ok());

    assert_eq!(slave.get_register(100).await, 500);

    // Test stop
    assert!(driver.stop(&ctx).await.is_ok());
    let health_stop = driver.health_check().await.unwrap();
    assert_eq!(health_stop.status, HealthStatus::Stopped);

    slave.stop();
}

#[tokio::test]
async fn test_modbus_driver_tag_namespace_validation() {
    let mut caps = HashSet::new();
    caps.insert("tagbus:write".to_string());
    let ctx = PluginContext::new("org.mythos.driver.modbus", caps, "/tmp");

    let mut driver = ModbusDriver::new();
    assert!(driver.init(&ctx).await.is_ok());

    // Violating physical hierarchy: missing 'external.' prefix
    let bad_config1 = json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": 502,
        "mappings": [
            {
                "uri": "internal.BoilerRoom.PT101.pv",
                "address": 1,
                "function_code": 3
            }
        ]
    });
    assert!(driver.start(&ctx, &bad_config1).await.is_err());

    // Prohibited transport name in namespace: 'external.modbus.device1.val'
    let bad_config2 = json!({
        "transport": "tcp",
        "host": "127.0.0.1",
        "port": 502,
        "mappings": [
            {
                "uri": "external.modbus.device1.val",
                "address": 1,
                "function_code": 3
            }
        ]
    });
    assert!(driver.start(&ctx, &bad_config2).await.is_err());
}
