//! Target profiles: what a target can run. A program that does not fit a target is refused at
//! compile time, naming what does not fit, so nothing is found out on the controller.
//!
//! A profile is data (`serde`): a runtime module will declare its own in its manifest
//! (ADR-013 §2.2), and the three here are what stands in until those modules exist. The memory
//! and code limits of the microcontroller profiles are budgets this project chose for a
//! program, leaving the rest of the chip to the firmware (network stack, I/O). They are not the
//! chips' sizes: an RP2040 has 264 KB of SRAM, an ESP32 520 KB.

use mythos_logic_ir::Ty;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TargetProfile {
    pub name: String,
    /// The bytecode format version the target's VM reads.
    pub format_version: u16,
    /// Bytes of variable memory a program may use.
    pub max_memory: u32,
    /// Bytes of bytecode a program may have.
    pub max_code: u32,
    /// Types the target does not run.
    #[serde(default)]
    pub refuses: Vec<Ty>,
}

impl TargetProfile {
    fn built_in(name: &str, max_memory: u32, max_code: u32) -> TargetProfile {
        TargetProfile {
            name: name.into(),
            format_version: mythos_logic_backend_vm::FORMAT_VERSION,
            max_memory,
            max_code,
            refuses: Vec::new(),
        }
    }

    /// A PC runner: Windows, macOS or Linux.
    pub fn pc() -> TargetProfile {
        Self::built_in("pc", 64 * 1024 * 1024, 64 * 1024 * 1024)
    }
    /// RP2040: two Cortex-M0+ cores, no floating-point unit (REAL and LREAL run in software).
    pub fn rp2040() -> TargetProfile {
        Self::built_in("rp2040", 64 * 1024, 512 * 1024)
    }
    /// ESP32 family.
    pub fn esp32() -> TargetProfile {
        Self::built_in("esp32", 128 * 1024, 1024 * 1024)
    }

    pub fn by_name(name: &str) -> Option<TargetProfile> {
        [Self::pc(), Self::rp2040(), Self::esp32()]
            .into_iter()
            .find(|p| p.name == name)
    }
}
