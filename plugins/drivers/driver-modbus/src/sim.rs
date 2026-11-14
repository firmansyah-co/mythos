//! A Modbus TCP device simulator, for tests and for development without hardware.
//!
//! It is a separate network endpoint that speaks the real protocol. Nothing in the driver or the
//! host falls back to it: it produces values only for a connection that was explicitly pointed
//! at its address. Compiled only with the `sim` feature.
//!
//! Serves function codes 3 (holding), 4 (input) and 6 (write single holding register). A read
//! that touches an address that was never set answers exception 2 (illegal data address), and
//! an unsupported function answers exception 1, as a real device does.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Mutex};

#[derive(Default)]
struct Registers {
    holding: HashMap<u16, u16>,
    input: HashMap<u16, u16>,
}

/// A running simulated Modbus TCP device.
pub struct ModbusSimServer {
    addr: SocketAddr,
    registers: Arc<Mutex<Registers>>,
    shutdown: watch::Sender<bool>,
}

impl ModbusSimServer {
    /// Bind and start serving. Use port `0` to let the OS pick a free port.
    pub async fn start(bind: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(bind).await?;
        let addr = listener.local_addr()?;
        let registers = Arc::new(Mutex::new(Registers::default()));
        let (shutdown, shutdown_rx) = watch::channel(false);

        let regs = Arc::clone(&registers);
        tokio::spawn(async move {
            let mut accept_shutdown = shutdown_rx.clone();
            loop {
                tokio::select! {
                    _ = accept_shutdown.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((socket, _)) = accepted else { continue };
                        tokio::spawn(serve_connection(
                            socket,
                            Arc::clone(&regs),
                            shutdown_rx.clone(),
                        ));
                    }
                }
            }
        });

        Ok(Self {
            addr,
            registers,
            shutdown,
        })
    }

    /// The address the simulator is listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub async fn set_holding(&self, addr: u16, value: u16) {
        self.registers.lock().await.holding.insert(addr, value);
    }

    pub async fn set_input(&self, addr: u16, value: u16) {
        self.registers.lock().await.input.insert(addr, value);
    }

    pub async fn holding(&self, addr: u16) -> Option<u16> {
        self.registers.lock().await.holding.get(&addr).copied()
    }

    /// Stop listening and close every open connection.
    pub fn stop(&self) {
        let _ = self.shutdown.send(true);
    }
}

impl Drop for ModbusSimServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn serve_connection(
    mut socket: TcpStream,
    registers: Arc<Mutex<Registers>>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let mut header = [0u8; 7];
        tokio::select! {
            _ = shutdown.changed() => return,
            read = socket.read_exact(&mut header) => {
                if read.is_err() {
                    return;
                }
            }
        }

        let length = u16::from_be_bytes([header[4], header[5]]) as usize;
        if !(2..=254).contains(&length) {
            return;
        }
        let mut pdu = vec![0u8; length - 1];
        if socket.read_exact(&mut pdu).await.is_err() {
            return;
        }

        let response_pdu = handle_pdu(&pdu, &registers).await;

        let mut frame = Vec::with_capacity(7 + response_pdu.len());
        frame.extend_from_slice(&header[0..2]); // transaction id
        frame.extend_from_slice(&[0, 0]); // protocol id
        frame.extend_from_slice(&((response_pdu.len() as u16) + 1).to_be_bytes());
        frame.push(header[6]); // unit id
        frame.extend_from_slice(&response_pdu);

        if socket.write_all(&frame).await.is_err() {
            return;
        }
    }
}

fn exception(function: u8, code: u8) -> Vec<u8> {
    vec![function | 0x80, code]
}

async fn handle_pdu(pdu: &[u8], registers: &Arc<Mutex<Registers>>) -> Vec<u8> {
    let Some(&function) = pdu.first() else {
        return exception(0, 0x01);
    };

    match function {
        0x03 | 0x04 => {
            if pdu.len() < 5 {
                return exception(function, 0x03);
            }
            let start = u16::from_be_bytes([pdu[1], pdu[2]]);
            let count = u16::from_be_bytes([pdu[3], pdu[4]]);
            if count == 0 || count > 125 {
                return exception(function, 0x03);
            }

            let regs = registers.lock().await;
            let table = if function == 0x03 {
                &regs.holding
            } else {
                &regs.input
            };

            let mut out = vec![function, (count * 2) as u8];
            for offset in 0..count {
                let Some(addr) = start.checked_add(offset) else {
                    return exception(function, 0x02);
                };
                match table.get(&addr) {
                    Some(value) => out.extend_from_slice(&value.to_be_bytes()),
                    None => return exception(function, 0x02),
                }
            }
            out
        }
        0x06 => {
            if pdu.len() < 5 {
                return exception(function, 0x03);
            }
            let addr = u16::from_be_bytes([pdu[1], pdu[2]]);
            let value = u16::from_be_bytes([pdu[3], pdu[4]]);
            registers.lock().await.holding.insert(addr, value);
            pdu[..5].to_vec()
        }
        other => exception(other, 0x01),
    }
}
