//! How long a scan takes on this machine, for a program shaped like a small plant's: a
//! sequence (CASE), timers, counters, REAL arithmetic like a PI loop, and a FOR loop.
//!
//! Not a pass/fail test (the number depends on the machine), so it is ignored by default:
//!
//!   cargo test --release -p mythos-logic-compiler --test scan_time -- --ignored --nocapture

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::{cold_start, scan, write, Program, Value};
use std::time::Instant;

pub const PLANT: &str = "
PROGRAM Plant
VAR_INPUT
  start_pb, stop_pb, level_hi, level_lo : BOOL;
  pv : REAL;
END_VAR
VAR_OUTPUT
  pump, valve, alarm : BOOL;
  cv : REAL;
END_VAR
VAR
  step : INT;
  t_fill, t_drain, t_alarm, t1, t2, t3, t4, t5 : TON;
  batches : CTU;
  edge : R_TRIG;
  sp : REAL := 50.0;
  kp : REAL := 1.2;
  ki : REAL := 0.05;
  err, integral : REAL;
  i : INT;
  total : DINT;
END_VAR
  CASE step OF
    0: IF start_pb AND NOT stop_pb THEN step := 1; END_IF;
    1: pump := TRUE; valve := FALSE;
       t_fill(IN := TRUE, PT := T#30s);
       IF level_hi OR t_fill.Q THEN step := 2; t_fill(IN := FALSE); END_IF;
    2: pump := FALSE; valve := TRUE;
       t_drain(IN := TRUE, PT := T#20s);
       IF level_lo OR t_drain.Q THEN step := 0; valve := FALSE; t_drain(IN := FALSE); END_IF;
  ELSE
    step := 0;
  END_CASE;
  IF stop_pb THEN step := 0; pump := FALSE; valve := FALSE; END_IF;

  edge(CLK := step = 2);
  batches(CU := edge.Q, R := FALSE, PV := 1000);

  err := sp - pv;
  integral := integral + err * ki;
  IF integral > 100.0 THEN integral := 100.0; ELSIF integral < 0.0 THEN integral := 0.0; END_IF;
  cv := kp * err + integral;
  IF cv > 100.0 THEN cv := 100.0; ELSIF cv < 0.0 THEN cv := 0.0; END_IF;

  t_alarm(IN := pv > 90.0, PT := T#5s);
  alarm := t_alarm.Q;
  t1(IN := pump, PT := T#1s); t2(IN := valve, PT := T#2s); t3(IN := alarm, PT := T#3s);
  t4(IN := t1.Q, PT := T#4s); t5(IN := t2.Q, PT := T#5s);

  total := 0;
  FOR i := 1 TO 50 DO
    total := total + i * 2;
  END_FOR;
END_PROGRAM
";

#[test]
#[ignore = "a measurement, not a check: run with --ignored --nocapture"]
fn scan_time_of_a_small_plant_program() {
    let c = compile(PLANT, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    let p = Program::load(&c.bytecode).unwrap();
    let mut mem = vec![0u8; p.memory_size()];
    let mut stack = vec![0u64; p.stack_depth()];
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
        let r = scan(&p, &mut mem, &mut stack, now, 1_000_000).unwrap();
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
