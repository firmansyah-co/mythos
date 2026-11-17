//! Target profiles: what a target can run. A program that does not fit a target is refused at
//! compile time, naming what does not fit, so nothing is found out on the controller.
//!
//! A profile is data (`serde`): a runtime module will declare its own in its manifest
//! (ADR-013 §2.2), and the three here are what stands in until those modules exist. The memory
//! and code limits of the microcontroller profiles are budgets this project chose for a
//! program, leaving the rest of the chip to the firmware (network stack, I/O). They are not the
//! chips' sizes: an RP2040 has 264 KB of SRAM, an ESP32 520 KB. Their stack and call depths
//! are what the firmware this repository builds for those instruction sets has
//! (`mythos-logic-vm/examples/on_target`, which the emulators run): a real board's firmware
//! will declare its own.

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
    /// How deep a program's evaluation stack may be, in values: the stack the target's
    /// firmware has. `None`, which is what a profile that leaves the field out declares: the
    /// target states no limit (a PC runner gives each program the stack it declares). It is
    /// not zero: a limit of 0 refuses every program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_stack_depth: Option<u16>,
    /// How deep a program's calls may nest: the call frames the target's firmware has.
    /// `None`, or the field left out: the target states no limit. A limit of 0 allows a
    /// program that calls nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_call_depth: Option<u16>,
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
            max_stack_depth: None,
            max_call_depth: None,
            refuses: Vec::new(),
        }
    }

    /// The stack and call depths of the firmware built for the microcontroller targets.
    fn on_firmware(self) -> TargetProfile {
        TargetProfile {
            max_stack_depth: Some(64),
            max_call_depth: Some(16),
            ..self
        }
    }

    /// A PC runner: Windows, macOS or Linux. It gives each program the stack and the call
    /// frames it declares, so it states no limit on either.
    pub fn pc() -> TargetProfile {
        Self::built_in("pc", 64 * 1024 * 1024, 64 * 1024 * 1024)
    }
    /// RP2040: two Cortex-M0+ cores, no floating-point unit (REAL and LREAL run in software).
    pub fn rp2040() -> TargetProfile {
        Self::built_in("rp2040", 64 * 1024, 512 * 1024).on_firmware()
    }
    /// ESP32 family.
    pub fn esp32() -> TargetProfile {
        Self::built_in("esp32", 128 * 1024, 1024 * 1024).on_firmware()
    }

    pub fn by_name(name: &str) -> Option<TargetProfile> {
        [Self::pc(), Self::rp2040(), Self::esp32()]
            .into_iter()
            .find(|p| p.name == name)
    }
}
