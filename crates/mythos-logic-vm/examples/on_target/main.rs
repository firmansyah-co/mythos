//! The VM running a real program on a microcontroller's instruction set, in an emulator, to
//! prove a program gives there exactly what it gives on a PC (ADR-013 §2.9).
//!
//! `scripts/logic-vm-on-emulators.sh` compiles a program, builds this firmware around its
//! bytecode, runs it in QEMU and compares its output with `expected_scans` run on the host.
//! After each scan the firmware prints `scan <n> <crc32 of the whole memory> <instructions>`:
//! the format lays memory out the same on every target, so equal checksums mean every variable
//! and every function block's inner state is equal, byte for byte.
//!
//! Two machines, chosen for their instruction sets, not as models of a board:
//! - Cortex-M0 (QEMU `microbit`, thumbv6m-none-eabi): the RP2040's instruction set, with no
//!   hardware divide and no floating point, so all of that runs in software as it will there;
//! - RV32IMC (QEMU `virt`, riscv32imc-unknown-none-elf): the ESP32-C3's instruction set.
//!
//! On a PC this is an empty program, so `cargo test --workspace` builds it too.
#![cfg_attr(target_os = "none", no_std, no_main)]

#[cfg(target_os = "none")]
mod firmware {
    use core::panic::PanicInfo;
    use mythos_logic_vm::{cold_start, crc32, Frame, Limits, Piece, Program, Run};

    /// The program, as it would sit in flash after a download.
    static PROGRAM: &[u8] = include_bytes!(env!("MYTHOS_LOGIC_PROGRAM"));
    /// As the host's `expected_scans` and STruC++'s REPL: 60 cycles of 20 ms from 0.
    const SCANS: u32 = 60;
    const CYCLE_NS: i64 = 20_000_000;
    const BUDGET: u32 = 1_000_000;
    /// Each scan runs in pieces of this many instructions, suspended and resumed between them
    /// as a scheduler would to let a task of higher priority run (ADR-013 §2.19). Odd, so the
    /// cuts fall at every kind of instruction; `expected_scans` scans uninterrupted, so equal
    /// output proves a run in pieces computes what a whole scan computes, on this instruction
    /// set too.
    const PIECE: u32 = 97;
    /// What this firmware has for a program. The compiler's `rp2040` and `esp32` profiles
    /// declare the same stack and call depths, so a program compiled for them fits.
    const HOST: Limits = Limits {
        memory: 4096,
        stack_depth: 64,
        call_depth: 16,
    };

    #[cfg(target_arch = "arm")]
    mod machine {
        //! Arm semihosting: the debugger (here QEMU) serves the calls.
        use core::arch::asm;

        fn call(op: usize, arg: usize) -> usize {
            let result: usize;
            // SAFETY: the semihosting trap. QEMU handles it and returns in r0; no memory of
            // ours is touched beyond what `arg` points to, which the callers keep alive.
            unsafe {
                asm!("bkpt #0xAB", inout("r0") op => result, in("r1") arg, options(nostack));
            }
            result
        }
        pub fn put(byte: u8) {
            call(0x03, &byte as *const u8 as usize); // SYS_WRITEC
        }
        pub fn exit(ok: bool) -> ! {
            // SYS_EXIT with ADP_Stopped_ApplicationExit, or ADP_Stopped_RunTimeErrorUnknown.
            call(0x18, if ok { 0x20026 } else { 0x20023 });
            #[allow(clippy::empty_loop)]
            loop {}
        }

        /// The reset vector; the linker script puts the initial stack pointer before it.
        #[link_section = ".vector_table.reset"]
        #[no_mangle]
        pub static RESET_VECTOR: extern "C" fn() -> ! = reset;

        #[no_mangle]
        pub extern "C" fn reset() -> ! {
            super::run()
        }
    }

    #[cfg(target_arch = "riscv32")]
    mod machine {
        //! QEMU's RISC-V `virt` machine: a 16550 UART and the SiFive test finisher.
        use core::arch::global_asm;
        use core::ptr::write_volatile;

        global_asm!(
            ".section .text.entry",
            ".globl _start",
            "_start:",
            "la sp, _stack_top",
            "call rust_main",
        );

        pub fn put(byte: u8) {
            // SAFETY: the UART's transmit register on this machine.
            unsafe { write_volatile(0x1000_0000 as *mut u8, byte) }
        }
        pub fn exit(ok: bool) -> ! {
            // SAFETY: the test finisher on this machine; 0x5555 passes, 0x3333 fails.
            unsafe { write_volatile(0x10_0000 as *mut u32, if ok { 0x5555 } else { 0x1_3333 }) }
            #[allow(clippy::empty_loop)]
            loop {}
        }

        #[no_mangle]
        pub extern "C" fn rust_main() -> ! {
            super::run()
        }
    }

    fn text(s: &str) {
        for b in s.bytes() {
            machine::put(b);
        }
    }
    fn decimal(mut v: u32) {
        let mut digits = [0u8; 10];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (v % 10) as u8;
            v /= 10;
            n += 1;
            if v == 0 {
                break;
            }
        }
        while n > 0 {
            n -= 1;
            machine::put(digits[n]);
        }
    }
    fn hex(v: u32) {
        for shift in (0..8).rev() {
            machine::put(b"0123456789abcdef"[((v >> (shift * 4)) & 0xF) as usize]);
        }
    }

    fn run() -> ! {
        let mut memory = [0u8; HOST.memory];
        let mut stack = [0u64; HOST.stack_depth];
        let mut calls = [Frame::default(); HOST.call_depth];
        // A program that needs more than this firmware has is refused here, with the rest
        // of what the loader refuses.
        let Ok(program) = Program::load_within(PROGRAM, &HOST) else {
            text("the program did not load\n");
            machine::exit(false)
        };
        let size = program.memory_size();
        if cold_start(&program, &mut memory[..size]).is_err() {
            text("the program did not start\n");
            machine::exit(false)
        }
        for k in 0..SCANS {
            let mut run = Run::begin(&program, k as i64 * CYCLE_NS, BUDGET);
            let outcome = loop {
                match run.resume(&program, &mut memory[..size], &mut stack, &mut calls, PIECE) {
                    Ok(Piece::Suspended) => continue,
                    Ok(Piece::Ended(report)) => break Ok(report),
                    Err(fault) => break Err(fault),
                }
            };
            match outcome {
                Ok(report) => {
                    text("scan ");
                    decimal(k + 1);
                    text(" ");
                    hex(crc32(&memory[..size]));
                    text(" ");
                    decimal(report.instructions);
                    text("\n");
                }
                Err(_) => {
                    text("fault\n");
                    machine::exit(false)
                }
            }
        }
        text("done\n");
        machine::exit(true)
    }

    #[panic_handler]
    fn panic(_: &PanicInfo) -> ! {
        text("panic\n");
        machine::exit(false)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
