//! Differential test: each program in `tests/differential/` runs for a number of cycles on
//! STruC++ (Autonomy Logic's ST→C++ compiler, GPL-3.0, run as a separate program, never
//! linked or copied) and on our VM, and every variable must agree after every cycle.
//!
//! STruC++ is the oracle: two independent implementations of IEC 61131-3 agreeing is evidence
//! that ours means what the standard means. It needs Node 22+, g++ and STruC++, so it is not
//! in CI; run it by hand:
//!
//!   MYTHOS_STRUCPP_CLI=/path/to/strucpp/dist/node/cli.js \
//!     cargo test -p mythos-logic-compiler --test differential_strucpp -- --ignored --nocapture
//!
//! Its REPL advances a simulated clock by its cycle time (20 ms) per cycle, starting at 0, so
//! cycle k runs at (k-1) × 20 ms here too.

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_st::lexer::{lex, Tok};
use mythos_logic_vm::{cold_start, read, scan, ElemType, Frame, Program, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CYCLES: usize = 60;

/// Where we differ from STruC++ on purpose: in each case STruC++ departs from the text of
/// IEC 61131-3 ed. 3 and we follow it (Adrian, 2026-11-17: "mengikuti IEC"). A value listed
/// here is compared and reported, but does not fail the test. Keep this list short and argued.
const HALF_TO_EVEN: &str =
    "REAL to integer rounds a half to the even integer (Table 22 footnote b: \
     REAL_TO_INT(1.5) = 2, so 2.5 -> 2); STruC++ rounds a half away from zero";
const DELIBERATE: &[(&str, &str, &str)] = &[
    (
        "timers_and_counters.st",
        "UP",
        "CTU counts while CV < PVmax (Table 45); STruC++ stops at PV, as OpenPLC does",
    ),
    (
        "timers_and_counters.st",
        "DOWN",
        "CTD counts while CV > PVmin (Table 45); STruC++ stops at 0, as OpenPLC does",
    ),
    ("integers_and_control.st", "A", HALF_TO_EVEN),
    ("integers_and_control.st", "B", HALF_TO_EVEN),
    ("reals_and_conversions.st", "R1", HALF_TO_EVEN),
    ("reals_and_conversions.st", "R2", HALF_TO_EVEN),
    ("reals_and_conversions.st", "R3", HALF_TO_EVEN),
    ("reals_and_conversions.st", "D2", HALF_TO_EVEN),
];
const CYCLE_NS: i64 = 20_000_000;

fn elem(ty: &str) -> ElemType {
    match ty {
        "BOOL" => ElemType::Bool,
        "SINT" => ElemType::Sint,
        "INT" => ElemType::Int,
        "DINT" => ElemType::Dint,
        "LINT" => ElemType::Lint,
        "REAL" => ElemType::Real,
        "LREAL" => ElemType::Lreal,
        "TIME" => ElemType::Time,
        other => panic!("{other}"),
    }
}

/// A value as both sides can be compared: integers and times exactly, reals as f64.
#[derive(Debug, Clone, PartialEq)]
enum V {
    Int(i128),
    Real(f64),
}

fn ours(v: Value) -> V {
    match v {
        Value::Bool(b) => V::Int(b as i128),
        Value::Sint(x) => V::Int(x as i128),
        Value::Int(x) => V::Int(x as i128),
        Value::Dint(x) => V::Int(x as i128),
        Value::Lint(x) | Value::Time(x) => V::Int(x as i128),
        Value::Real(x) => V::Real(x as f64),
        Value::Lreal(x) => V::Real(x),
    }
}

fn theirs(ty: &str, text: &str) -> V {
    match ty {
        "BOOL" => V::Int((text == "TRUE") as i128),
        "REAL" | "LREAL" => V::Real(text.parse().unwrap_or_else(|_| panic!("{text}"))),
        "TIME" => match lex(text).unwrap()[0].tok {
            Tok::Time(ns) => V::Int(ns as i128),
            ref t => panic!("{text}: {t:?}"),
        },
        _ => V::Int(text.parse().unwrap_or_else(|_| panic!("{text}"))),
    }
}

/// The REPL prints reals rounded for display; agree to that precision.
fn agree(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Real(x), V::Real(y)) => (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0),
        _ => a == b,
    }
}

/// Run `st` on STruC++ for CYCLES cycles: per cycle, variable name -> (type, value text).
fn strucpp(cli: &Path, st: &Path, work: &Path) -> Vec<BTreeMap<String, (String, String)>> {
    std::fs::create_dir_all(work).unwrap();
    let src = work.join(st.file_name().unwrap());
    std::fs::copy(st, &src).unwrap();
    let cpp = src.with_extension("cpp");
    let out = Command::new("node")
        .arg(cli)
        .arg(&src)
        .arg("-o")
        .arg(&cpp)
        .arg("--build")
        .current_dir(work)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "STruC++ failed on {}: {}",
        st.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = src.with_extension("");
    // The REPL writes a history file where it runs: keep it in the work directory.
    let mut child = Command::new(&exe)
        .current_dir(work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut script = String::new();
    for _ in 0..CYCLES {
        script.push_str("run 1\nvars MAIN\n");
    }
    script.push_str("quit\n");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let text = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
    let mut cycles = Vec::new();
    for chunk in text.split("Executed 1 cycle(s).").skip(1) {
        let mut vars = BTreeMap::new();
        for line in chunk.lines() {
            // "MAIN.X : REAL = 2.5" (the last line of a chunk may run into the next message)
            let Some(rest) = line.trim().strip_prefix("MAIN.") else {
                continue;
            };
            let Some((name, rest)) = rest.split_once(" : ") else {
                continue;
            };
            let Some((ty, value)) = rest.split_once(" = ") else {
                continue;
            };
            let value = value.split("Goodbye").next().unwrap().trim();
            vars.insert(name.to_string(), (ty.to_string(), value.to_string()));
        }
        cycles.push(vars);
    }
    assert_eq!(
        cycles.len(),
        CYCLES,
        "STruC++ printed {} cycles",
        cycles.len()
    );
    cycles
}

#[test]
#[ignore = "needs STruC++, Node and g++: set MYTHOS_STRUCPP_CLI and run with --ignored"]
fn every_variable_agrees_with_strucpp_after_every_cycle() {
    let cli = PathBuf::from(
        std::env::var("MYTHOS_STRUCPP_CLI")
            .expect("set MYTHOS_STRUCPP_CLI to STruC++'s dist/node/cli.js"),
    );
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differential");
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("strucpp-differential");
    let mut programs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "st"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty());
    let mut disagreements = Vec::new();
    let mut deliberate: BTreeMap<String, usize> = BTreeMap::new();
    let mut compared = 0usize;
    for st in &programs {
        let src = std::fs::read_to_string(st).unwrap();
        let reference = strucpp(&cli, st, &work);
        let c = compile(&src, &TargetProfile::pc())
            .unwrap_or_else(|d| panic!("{}: {d:?}", st.display()));
        let p = Program::load(&c.bytecode).unwrap();
        let mut mem = vec![0u8; p.memory_size()];
        let mut stack = vec![0u64; p.stack_depth()];
        let mut calls = vec![Frame::default(); p.call_depth()];
        cold_start(&p, &mut mem).unwrap();
        for (k, expected) in reference.iter().enumerate() {
            scan(
                &p,
                &mut mem,
                &mut stack,
                &mut calls,
                k as i64 * CYCLE_NS,
                1_000_000,
            )
            .unwrap();
            for (name, (ty, text)) in expected {
                // Function block instances are listed by STruC++ as a whole; compare scalars.
                let Some(sym) = c.symbol(name) else { continue };
                // A structure or an array of MAIN is one line in STruC++; compare scalars.
                if sym.array.is_some() {
                    continue;
                }
                let mine = ours(read(&mem, sym.offset, elem(&sym.ty)).unwrap());
                let want = theirs(ty, text);
                compared += 1;
                if !agree(&mine, &want) {
                    let file = st.file_name().unwrap().to_string_lossy().to_string();
                    let line = format!(
                        "{file} cycle {}: {name} is {mine:?} here, {text} in STruC++",
                        k + 1
                    );
                    let listed = DELIBERATE.iter().find(|(f, v, _)| *f == file && *v == name);
                    // A rounding difference is exactly one apart; more is a real disagreement.
                    let one_apart =
                        matches!((&mine, &want), (V::Int(a), V::Int(b)) if (a - b).abs() == 1);
                    match listed {
                        Some((_, _, why)) if *why != HALF_TO_EVEN || one_apart => {
                            *deliberate
                                .entry(format!("{file} {name}: {why}"))
                                .or_insert(0usize) += 1;
                        }
                        _ => disagreements.push(line),
                    }
                }
            }
        }
    }
    println!(
        "{} programs, {CYCLES} cycles each, {compared} values compared",
        programs.len()
    );
    println!(
        "{} values differ on purpose:",
        deliberate.values().sum::<usize>()
    );
    for (what, times) in &deliberate {
        println!("  {times:>3}x {what}");
    }
    assert!(
        disagreements.is_empty(),
        "{} disagreements:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}
