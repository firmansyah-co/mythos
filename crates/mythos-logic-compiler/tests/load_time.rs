//! What loading a program costs (ADR-013 §2.3). The loader walks every body twice: once to
//! check every instruction, and once to check that every jump lands where an instruction
//! starts, which without memory to note the starts in means walking to each target. This
//! measures it on a program far larger than the corpus, with the shapes that cost most: many
//! loops (a jump back is walked from the start of its body) and a long CASE (every arm jumps
//! forward to its end).
//!
//! It is a measurement, not a test: run it by hand, in release.
//!
//!   cargo test --release -p mythos-logic-compiler --test load_time -- --ignored --nocapture

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::format::{instruction_len, op};
use mythos_logic_vm::Program;
use std::time::Instant;

/// A program of `loops` FOR loops, a CASE of `arms` arms, each with an IF and an ELSIF in it,
/// `ifs` IF statements, and then `loops` loops with a loop nested in each: at the end, where
/// the head of a loop is farthest from the start of the body.
fn source(loops: usize, arms: usize, ifs: usize) -> String {
    let mut s = String::from("PROGRAM Big VAR i, j, a, sel : DINT; END_VAR\n");
    for _ in 0..loops {
        s.push_str("FOR i := 1 TO 3 DO a := a + i; END_FOR;\n");
    }
    s.push_str("CASE sel OF\n");
    for k in 0..arms {
        s.push_str(&format!(
            "{k}: IF a > {k} THEN a := a + 1; ELSIF a < 0 THEN a := 0; ELSE a := a - 1; END_IF;\n"
        ));
    }
    s.push_str("END_CASE;\n");
    for k in 0..ifs {
        s.push_str(&format!(
            "IF a > {k} THEN a := a - 1; ELSE a := a + 1; END_IF;\n"
        ));
    }
    for _ in 0..loops {
        s.push_str(
            "FOR i := 1 TO 2 DO WHILE a > 9 DO FOR j := 1 TO 2 DO a := a - j; END_FOR; END_WHILE; END_FOR;\n",
        );
    }
    s.push_str("END_PROGRAM\n");
    s
}

#[test]
#[ignore = "a measurement: run by hand, in release"]
fn loading_a_large_program() {
    for (loops, arms, ifs) in [(10, 20, 50), (300, 2000, 2000), (1500, 8000, 8000)] {
        let c = compile(&source(loops, arms, ifs), &TargetProfile::pc()).unwrap();
        let p = Program::load(&c.bytecode).unwrap();
        // Count the jumps the loader has to follow, by walking the code as it does.
        let code = &c.bytecode[c.bytecode.len() - 4 - p.code_size()..c.bytecode.len() - 4];
        let (mut pc, mut jumps, mut back, mut instructions) = (0usize, 0u32, 0u32, 0u32);
        while pc < code.len() {
            if matches!(code[pc], op::JMP | op::JZ) {
                jumps += 1;
                let target = u32::from_le_bytes(code[pc + 1..pc + 5].try_into().unwrap());
                back += (target as usize <= pc) as u32;
            }
            instructions += 1;
            pc += instruction_len(code[pc]).unwrap();
        }
        let runs = 20;
        let mut best = u128::MAX;
        let mut total = 0u128;
        for _ in 0..runs {
            let t = Instant::now();
            let loaded = Program::load(std::hint::black_box(&c.bytecode));
            let ns = t.elapsed().as_nanos();
            assert!(loaded.is_ok());
            best = best.min(ns);
            total += ns;
        }
        println!(
            "{} bytes of code, {instructions} instructions, {jumps} jumps ({back} of them back): load best {:.3} ms, mean {:.3} ms over {runs}",
            p.code_size(),
            best as f64 / 1e6,
            total as f64 / runs as f64 / 1e6
        );
    }
}
