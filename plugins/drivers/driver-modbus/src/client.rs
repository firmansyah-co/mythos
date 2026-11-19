//! Asynchronous Modbus TCP client transport.

use crate::config::ModbusDriverConfig;
use crate::frame::{ModbusRequest, ModbusResponse};
use mythos_plugin_sdk::PluginError;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Modbus TCP client maintaining connection state and executing queries.
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

    /// Connect to the configured device.
    pub async fn connect(&mut self) -> Result<(), PluginError> {
        // `validate` requires a host for TCP. A missing one here is a configuration that was
        // never validated, and connecting to a made-up address would read someone else's device.
        let host = self.config.host.as_deref().ok_or_else(|| {
            PluginError::InvalidConfiguration("Modbus TCP target has no 'host'".into())
        })?;
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

    /// Disconnect transport.
    pub async fn disconnect(&mut self) {
        if let Some(mut stream) = self.tcp_stream.take() {
            let _ = stream.shutdown().await;
        }
    }

    /// Check if transport is currently connected.
    pub fn is_connected(&self) -> bool {
        self.tcp_stream.is_some()
    }

    /// Execute a Modbus request and await the response, bounded by the configured timeout.
    ///
    /// A timeout drops the connection: the late answer to this request would otherwise be read
    /// as the answer to the next one.
    pub async fn execute(&mut self, req: ModbusRequest) -> Result<ModbusResponse, PluginError> {
        let duration = Duration::from_millis(self.config.timeout_ms);
        match timeout(duration, self.execute_inner(req)).await {
            Ok(result) => result,
            Err(_) => {
                self.tcp_stream = None;
                Err(PluginError::Timeout("Modbus query timeout exceeded".into()))
            }
        }
    }

    async fn execute_inner(&mut self, req: ModbusRequest) -> Result<ModbusResponse, PluginError> {
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

        let parsed = ModbusResponse::parse_tcp(&full_frame, tx_id);
        if parsed.is_err() {
            // A frame that does not parse leaves the stream in an unknown position.
            self.tcp_stream = None;
        }
        parsed
    }
}
