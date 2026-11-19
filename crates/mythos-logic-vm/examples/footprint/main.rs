//! The VM linked into bare-metal firmware, to prove it builds with `core` alone and to measure
//! what it costs in flash. CI builds it for an RP2040 (thumbv6m-none-eabi) and a RISC-V ESP32
//! (riscv32imc-unknown-none-elf); see `.github/workflows/ci.yml`, job `logic-vm-bare-metal`.
//!
//! It is not a boot image (no vector table, no clocks): it links the loader and the scan loop
//! as real firmware would, around a program slot in flash the optimiser cannot see into. On a
//! PC it is an empty program, so `cargo test --workspace` builds it too.
#![cfg_attr(target_os = "none", no_std, no_main)]

#[cfg(target_os = "none")]
mod bare {
    use core::hint::black_box;
    use core::panic::PanicInfo;
    use mythos_logic_vm::{cold_start, scan, Frame, Limits, Program};

    /// Where a downloaded program would sit in flash.
    #[no_mangle]
    static PROGRAM: [u8; 4096] = [0; 4096];

    #[no_mangle]
    pub extern "C" fn _start() -> ! {
        let bytes: &[u8] = black_box(&PROGRAM[..]);
        let host = Limits {
            memory: 2048,
            stack_depth: 64,
            call_depth: 16,
        };
        let mut memory = [0u8; 2048];
        let mut stack = [0u64; 64];
        let mut calls = [Frame::default(); 16];
        if let Ok(p) = Program::load_within(bytes, &host) {
            let n = p.memory_size();
            if cold_start(&p, &mut memory[..n]).is_ok() {
                let mut now = 0i64;
                loop {
                    let _ = black_box(scan(
                        &p,
                        &mut memory[..n],
                        &mut stack,
                        &mut calls,
                        now,
                        100_000,
                    ));
                    now = black_box(now + 1_000_000);
                }
            }
        }
        #[allow(clippy::empty_loop)]
        loop {}
    }

    #[panic_handler]
    fn panic(_: &PanicInfo) -> ! {
        #[allow(clippy::empty_loop)]
        loop {}
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
