//! Target profiles: what a target can run. A program that does not fit a target is refused at
//! compile time, naming what does not fit, so nothing is found out on the controller.
//!
//! The memory and code limits of the microcontroller profiles are budgets this project chose
//! for a program, leaving the rest of the chip to the firmware (network stack, I/O). They are
//! not the chips' sizes: an RP2040 has 264 KB of SRAM, an ESP32 520 KB.

use crate::ir::Ty;

#[derive(Debug, Clone)]
pub struct TargetProfile {
    pub name: &'static str,
    /// Bytes of variable memory a program may use.
    pub max_memory: u32,
    /// Bytes of bytecode a program may have.
    pub max_code: u32,
    /// Types the target does not run.
    pub refuses: &'static [Ty],
}

impl TargetProfile {
    /// A PC runner: Windows, macOS or Linux.
    pub const PC: TargetProfile = TargetProfile {
        name: "pc",
        max_memory: 64 * 1024 * 1024,
        max_code: 64 * 1024 * 1024,
        refuses: &[],
    };
    /// RP2040: two Cortex-M0+ cores, no floating-point unit (REAL and LREAL run in software).
    pub const RP2040: TargetProfile = TargetProfile {
        name: "rp2040",
        max_memory: 64 * 1024,
        max_code: 512 * 1024,
        refuses: &[],
    };
    /// ESP32 family.
    pub const ESP32: TargetProfile = TargetProfile {
        name: "esp32",
        max_memory: 128 * 1024,
        max_code: 1024 * 1024,
        refuses: &[],
    };

    pub fn by_name(name: &str) -> Option<TargetProfile> {
        [Self::PC, Self::RP2040, Self::ESP32]
            .into_iter()
            .find(|p| p.name == name)
    }
}
