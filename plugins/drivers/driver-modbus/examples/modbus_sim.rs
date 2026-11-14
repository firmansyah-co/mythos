//! Standalone Modbus TCP device simulator for development without hardware.
//!
//! ```text
//! cargo run -p driver-modbus --features sim --example modbus_sim -- 127.0.0.1:1502
//! ```
//!
//! THE VALUES ARE SYNTHETIC. This process is a stand-in for a field device and exists so the
//! data path can be exercised end to end. Point a connection at it deliberately, and never at a
//! commissioning or production project.
//!
//! Holding registers, updated once per second:
//!
//! | Address | Type | Meaning                                             |
//! | ------- | ---- | --------------------------------------------------- |
//! | 0       | u16  | Seconds since start, wrapping                       |
//! | 1       | i16  | Triangle wave, -1000..=1000                          |
//! | 2..=3   | f32  | Sine wave 20.0..=80.0, high word first              |
//! | 10      | u16  | Writable register, initially 0                      |

use driver_modbus::sim::ModbusSimServer;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = std::env::args()
        .nth(1)
        .ok_or("usage: modbus_sim <listen address>, for example 127.0.0.1:1502")?;

    let server = ModbusSimServer::start(&bind).await?;
    println!(
        "Modbus TCP SIMULATOR listening on {} — every value it serves is synthetic",
        server.local_addr()
    );
    server.set_holding(10, 0).await;

    let mut seconds: u32 = 0;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tick.tick().await;

        let phase = (seconds % 40) as i32; // 0..40
        let triangle = if phase < 20 {
            -1000 + phase * 100
        } else {
            1000 - (phase - 20) * 100
        };
        let sine = 50.0_f32 + 30.0 * ((seconds as f32) * std::f32::consts::TAU / 60.0).sin();
        let bits = sine.to_bits();

        server.set_holding(0, (seconds & 0xFFFF) as u16).await;
        server.set_holding(1, (triangle as i16) as u16).await;
        server.set_holding(2, (bits >> 16) as u16).await;
        server.set_holding(3, (bits & 0xFFFF) as u16).await;

        seconds = seconds.wrapping_add(1);
    }
}
