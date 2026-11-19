//! How long a scan takes on this machine, for a program shaped like a small plant's: a
//! sequence (CASE), timers, counters, REAL arithmetic like a PI loop, and a FOR loop.
//!
//! Not a pass/fail test (the number depends on the machine), so it is ignored by default:
//!
//!   cargo test --release -p mythos-logic-compiler --test scan_time -- --ignored --nocapture

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::{cold_start, scan, write, Frame, Program, Value};
use std::time::Instant;

/// Also one of the programs compared with STruC++ and run on the emulated microcontrollers.
pub const PLANT: &str = include_str!("differential/small_plant.st");

#[test]
#[ignore = "a measurement, not a check: run with --ignored --nocapture"]
fn scan_time_of_a_small_plant_program() {
    let c = compile(PLANT, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    let p = Program::load(&c.bytecode).unwrap();
    let mut mem = vec![0u8; p.memory_size()];
    let mut stack = vec![0u64; p.stack_depth()];
    let mut calls = vec![Frame::default(); p.call_depth()];
    cold_start(&p, &mut mem).unwrap();
    let start = c.symbol("start_pb").unwrap().offset;
    let pv = c.symbol("pv").unwrap().offset;
    write(&mut mem, start, Value::Bool(true));

    const N: usize = 200_000;
    let mut times = Vec::with_capacity(N);
    let mut instructions = 0;
    for k in 0..N {
        write(&mut mem, pv, Value::Real((k % 100) as f32));
        let now = (k as i64) * 1_000_000; // 1 ms per scan
        let t = Instant::now();
        let r = scan(&p, &mut mem, &mut stack, &mut calls, now, 1_000_000).unwrap();
        times.push(t.elapsed().as_nanos() as u64);
        instructions = r.instructions;
    }
    times.sort_unstable();
    let pct = |q: f64| times[((N as f64 - 1.0) * q) as usize];
    let mean = times.iter().sum::<u64>() / N as u64;
    println!(
        "program: {} bytes of code, {} bytes of memory, stack {}, {} instructions per scan",
        c.code_size, c.memory_size, c.stack_depth, instructions
    );
    println!(
        "scan over {N} scans: mean {mean} ns, p50 {} ns, p99 {} ns, p99.9 {} ns, max {} ns",
        pct(0.5),
        pct(0.99),
        pct(0.999),
        times[N - 1]
    );
}
