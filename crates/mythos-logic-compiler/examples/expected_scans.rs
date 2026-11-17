//! What a program gives on a PC, in the form the `on_target` firmware prints it, for
//! `scripts/logic-vm-on-emulators.sh` to compare with what an emulated microcontroller gives.
//!
//!   expected_scans <program.st> <profile> <bytecode out>
//!
//! Compiles the program for the profile, writes its bytecode, and prints, for 60 scans 20 ms
//! apart, `scan <n> <crc32 of the whole memory> <instructions>`, then `done`.

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::{cold_start, crc32, scan, Frame, Program};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, source, profile, out] = args.as_slice() else {
        eprintln!("usage: expected_scans <program.st> <profile> <bytecode out>");
        return ExitCode::from(2);
    };
    let Some(profile) = TargetProfile::by_name(profile) else {
        eprintln!("no target profile named {profile}");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{source}: {e}");
            return ExitCode::from(2);
        }
    };
    let compiled = match compile(&text, &profile) {
        Ok(c) => c,
        Err(diags) => {
            for d in diags {
                eprintln!("{source}:{d}");
            }
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::write(out, &compiled.bytecode) {
        eprintln!("{out}: {e}");
        return ExitCode::from(2);
    }
    let program = Program::load(&compiled.bytecode).expect("the VM loads what the compiler made");
    let mut memory = vec![0u8; program.memory_size()];
    let mut stack = vec![0u64; program.stack_depth()];
    let mut calls = vec![Frame::default(); program.call_depth()];
    cold_start(&program, &mut memory).expect("sizes agree");
    for k in 0..60u32 {
        match scan(
            &program,
            &mut memory,
            &mut stack,
            &mut calls,
            k as i64 * 20_000_000,
            1_000_000,
        ) {
            Ok(report) => println!(
                "scan {} {:08x} {}",
                k + 1,
                crc32(&memory),
                report.instructions
            ),
            Err(fault) => {
                println!("fault");
                eprintln!("{source}: scan {} stopped: {fault:?}", k + 1);
                return ExitCode::FAILURE;
            }
        }
    }
    println!("done");
    ExitCode::SUCCESS
}
