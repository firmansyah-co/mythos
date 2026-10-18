//! Asynchronous non-blocking Modbus client transport (TCP & simulated RTU).

use crate::config::{ModbusDriverConfig, ModbusTransport};
use crate::frame::{ModbusRequest, ModbusResponse};
use mythos_plugin_sdk::PluginError;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Modbus Client maintaining connection state and executing queries.
pub struct ModbusClient {
    config: ModbusDriverConfig,
    tcp_stream: Option<TcpStream>,
    transaction_id: AtomicU16,
}

impl ModbusClient {
    pub fn new(config: ModbusDriverConfig) -> Self {
        Self {
            config,
            tcp_stream: None,
            transaction_id: AtomicU16::new(1),
        }
    }

    /// Connect to remote device (or establish connection pool).
    pub async fn connect(&mut self) -> Result<(), PluginError> {
        match self.config.transport {
            ModbusTransport::Tcp => {
                let host = self.config.host.as_deref().unwrap_or("127.0.0.1");
                let addr = format!("{}:{}", host, self.config.port);
                let duration = Duration::from_millis(self.config.timeout_ms);

                let stream = match timeout(duration, TcpStream::connect(&addr)).await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        return Err(PluginError::CommunicationError(format!(
                            "Failed to connect to Modbus TCP target {}: {}",
                            addr, e
                        )));
                    }
                    Err(_) => {
                        return Err(PluginError::Timeout(format!(
                            "Connection timeout to Modbus TCP target {}",
                            addr
                        )));
                    }
                };

                self.tcp_stream = Some(stream);
                Ok(())
            }
            ModbusTransport::Rtu => {
                // Serial RTU transport stub / simulated
                Ok(())
            }
        }
    }

    /// Disconnect transport.
    pub async fn disconnect(&mut self) {
        if let Some(mut stream) = self.tcp_stream.take() {
            let _ = stream.shutdown().await;
        }
    }

    /// Check if transport is currently connected.
    pub fn is_connected(&self) -> bool {
        match self.config.transport {
            ModbusTransport::Tcp => self.tcp_stream.is_some(),
            ModbusTransport::Rtu => true,
        }
    }

    /// Execute a Modbus request and await response with timeout guard.
    pub async fn execute(&mut self, req: ModbusRequest) -> Result<ModbusResponse, PluginError> {
        let duration = Duration::from_millis(self.config.timeout_ms);
        timeout(duration, self.execute_inner(req))
            .await
            .map_err(|_| PluginError::Timeout("Modbus query timeout exceeded".into()))?
    }

    async fn execute_inner(&mut self, req: ModbusRequest) -> Result<ModbusResponse, PluginError> {
        match self.config.transport {
            ModbusTransport::Tcp => {
                if self.tcp_stream.is_none() {
                    self.connect().await?;
                }

                let stream = self.tcp_stream.as_mut().ok_or_else(|| {
                    PluginError::CommunicationError("Modbus TCP stream disconnected".into())
                })?;

                let tx_id = self.transaction_id.fetch_add(1, Ordering::SeqCst);
                let frame = req.encode_tcp(tx_id);

                if let Err(e) = stream.write_all(&frame).await {
                    self.tcp_stream = None;
                    return Err(PluginError::CommunicationError(format!(
                        "Modbus TCP write failed: {}",
                        e
                    )));
                }

                // Read MBAP header (7 bytes)
                let mut header = [0u8; 7];
                if let Err(e) = stream.read_exact(&mut header).await {
                    self.tcp_stream = None;
                    return Err(PluginError::CommunicationError(format!(
                        "Modbus TCP header read failed: {}",
                        e
                    )));
                }

                let length = u16::from_be_bytes([header[4], header[5]]) as usize;
                if length == 0 || length > 256 {
                    self.tcp_stream = None;
                    return Err(PluginError::CommunicationError(format!(
                        "Invalid Modbus TCP frame length {}",
                        length
                    )));
                }

                // Read remaining bytes (Unit ID already in length, so length - 1 bytes of PDU)
                let pdu_len = length.saturating_sub(1);
                let mut pdu = vec![0u8; pdu_len];
                if let Err(e) = stream.read_exact(&mut pdu).await {
                    self.tcp_stream = None;
                    return Err(PluginError::CommunicationError(format!(
                        "Modbus TCP PDU read failed: {}",
                        e
                    )));
                }

                let mut full_frame = Vec::with_capacity(7 + pdu_len);
                full_frame.extend_from_slice(&header);
                full_frame.extend_from_slice(&pdu);

                ModbusResponse::parse_tcp(&full_frame, tx_id)
            }
            ModbusTransport::Rtu => {
                // For RTU testing, return empty or mock
                Ok(ModbusResponse::Registers(vec![0]))
            }
        }
    }
}
