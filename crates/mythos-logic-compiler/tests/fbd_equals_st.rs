//! Each Function Block Diagram program here has a twin written by hand in Structured Text, and
//! the two, compiled, run on the VM through the same random inputs: every variable they share is
//! equal after every scan, and so is every value the FBD front-end keeps for monitoring (ADR-013
//! §2.16).
//!
//! The twins cover every element the FBD front-end reads: input variables (plain, negated, a
//! literal), output variables (plain, negated), in-out variables; blocks of the Boolean
//! operators, arithmetic, comparison, selection and conversion functions, with negated inputs
//! and outputs and inputs written as expressions; the standard function blocks TON, CTU,
//! R_TRIG and SR, with EN and ENO and a negated output; a chain of functions under EN; a
//! connector with two continuations; a forward jump, a backward jump and a return, with a
//! label and a comment; an explicit loop, a function block's output drawn back, a block's own
//! output drawn back, and an implicit loop through a variable; an FBD function and function
//! block and an LD function block called from an FBD program. Networks are drawn out of the
//! order of the document.
//!
//! **What is compared.** Every symbol of the FBD program but its hidden ones is in the twin, and
//! every one of the twin's but its `w_*` and `n_*` is in the FBD program; each, read after each
//! scan, is equal. The twin names the value of each line `w_<localId>` (`w_<localId>_<output>`
//! for a function block's output and ENO; a continuation's line its own `w_<localId>`) and each
//! network's flag `n_<localId>`, assigned where the twin computes them. Every probe of the
//! PROGRAM, by the element and output it is about (not by the variable it names), must have its
//! `w_…` or `n_…` in the twin, and the variable it names must equal it after every scan; every
//! `w_` and `n_` of the twin must be a probe's. So what is kept for each line is what was
//! computed for that line.

mod support;
#[path = "../../mythos-logic-graphic/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{compile, compile_sources, At, Holds, Sources, TargetProfile};
use mythos_logic_vm::Value;
use std::collections::BTreeSet;
use support::Run;
use tc6::{project, In, Pou};

const SEQUENCES: u64 = 200;
const SCANS: usize = 100;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
    fn int(&mut self, lo: i16, hi: i16) -> i16 {
        lo + (self.next() % (hi - lo + 1) as u64) as i16
    }
}

enum Input {
    Bool(&'static str),
    Int(&'static str, i16, i16),
}

/// The twin's name for what a probe is about, by the element and the output it names, never by
/// the variable the probe gives: `n_<localId>` for a network, `w_<localId>` for a variable's or a
/// continuation's one output and a function's result, `w_<localId>_<output>` for any other output.
fn twin_of(p: &mythos_logic_compiler::Probe) -> String {
    match &p.at {
        At::Network(_) => format!("n_{}", p.local_id),
        At::Output(o) if o.is_empty() || o == "OUT" => format!("w_{}", p.local_id),
        At::Output(o) => format!("w_{}_{o}", p.local_id),
        At::Input(i) => panic!("an FBD probe at an input ({i}) of {}", p.local_id),
        other => panic!("an FBD probe of a chart ({other:?}) at {}", p.local_id),
    }
}

/// Whether a path of the twin is one of its names of a kept value.
fn kept_by_twin(path: &str) -> bool {
    path.split('.')
        .any(|p| p.starts_with("w_") || p.starts_with("n_"))
}

fn compare(name: &str, fbd: impl Fn() -> Run, st: impl Fn() -> Run, inputs: &[Input]) {
    let (f, s) = (fbd(), st());
    let kept =
        f.c.symbols
            .iter()
            .filter(|sym| sym.path.starts_with("__FBD_") || sym.path.starts_with("__NET_"))
            .count();
    eprintln!(
        "{name}: the FBD program is {} bytes of code and {} of memory, {kept} variables kept for monitoring",
        f.c.code_size, f.c.memory_size
    );
    let shared: Vec<String> =
        f.c.symbols
            .iter()
            .filter(|sym| !sym.path.starts_with("__") && !sym.path.contains(".__"))
            .map(|sym| sym.path.clone())
            .collect();
    for path in &shared {
        let twin =
            s.c.symbol(path)
                .unwrap_or_else(|| panic!("{name}: the ST twin has no {path}"));
        assert_eq!(twin.ty, f.c.symbol(path).unwrap().ty, "{name}: {path}");
    }
    for sym in &s.c.symbols {
        if !kept_by_twin(&sym.path) {
            assert!(
                f.c.symbol(&sym.path).is_some(),
                "{name}: the FBD program has no {}",
                sym.path
            );
        }
    }
    // Every value the PROGRAM keeps, with the twin's name of the element and output it is about.
    let mut probes: Vec<(String, String)> = Vec::new();
    for p in f.c.probes.iter().filter(|p| p.pou == "Main") {
        let path = match &p.holds {
            Holds::On => continue,
            Holds::Variable { path, negated } => {
                assert!(!negated, "{name}: no negated ENO here");
                path
            }
            Holds::Guarded { path, ran } => {
                assert!(
                    f.c.probes.iter().any(|q| q.local_id == p.local_id
                        && matches!(&q.holds, Holds::Variable { path, .. } if path == ran)),
                    "{name}: {ran} is the ENO of block {}",
                    p.local_id
                );
                path
            }
        };
        let twin = twin_of(p);
        let ty = &f.c.symbol(path).unwrap().ty;
        let tw =
            s.c.symbol(&twin)
                .unwrap_or_else(|| panic!("{name}: the ST twin has no {twin} for {path}"));
        assert_eq!(&tw.ty, ty, "{name}: {path} is kept as a {ty}");
        if !probes.iter().any(|(p, t)| p == path && *t == twin) {
            probes.push((path.clone(), twin));
        }
    }
    let twins: BTreeSet<String> = probes.iter().map(|(_, t)| t.to_lowercase()).collect();
    for sym in s.c.symbols.iter().filter(|s| kept_by_twin(&s.path)) {
        if !sym.path.contains('.') {
            assert!(
                twins.contains(&sym.path.to_lowercase()),
                "{name}: the twin's {} is no probe's",
                sym.path
            );
        }
    }
    assert!(shared.len() >= 5, "{name}: something to compare");
    let mut values = 0usize;
    for seed in 1..=SEQUENCES {
        let (mut f, mut s) = (fbd(), st());
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut now = 0i64;
        for scan in 0..SCANS {
            for input in inputs {
                let (path, v) = match input {
                    Input::Bool(p) => (*p, Value::Bool(rng.bool())),
                    Input::Int(p, lo, hi) => (*p, Value::Int(rng.int(*lo, *hi))),
                };
                f.set(path, v);
                s.set(path, v);
            }
            now += (rng.next() % 30) as i64;
            f.scan_at(now);
            s.scan_at(now);
            for path in &shared {
                assert_eq!(
                    f.get(path),
                    s.get(path),
                    "{name}: {path} differs after scan {} of sequence {seed}",
                    scan + 1
                );
                values += 1;
            }
            for (path, twin) in &probes {
                assert_eq!(
                    f.get(path),
                    s.get(twin),
                    "{name}: the kept {path} differs from {twin} after scan {} of sequence {seed}",
                    scan + 1
                );
                values += 1;
            }
        }
    }
    assert_eq!(
        values,
        SEQUENCES as usize * SCANS * (shared.len() + probes.len())
    );
    eprintln!(
        "{name}: {} variables and {} kept values, {values} values equal",
        shared.len(),
        probes.len()
    );
}

fn fbd_run(pous: Vec<Pou>) -> impl Fn() -> Run {
    let doc = project(&pous);
    move || {
        Run::of(compile_sources(
            &Sources {
                st: None,
                tc6: &[("main.xml", doc.as_str())],
            },
            &TargetProfile::pc(),
        ))
    }
}

fn st_run(src: &'static str) -> impl Fn() -> Run {
    move || Run::of(compile(src, &TargetProfile::pc()))
}

#[test]
fn functions_negation_and_numbers_equal_their_st_twin() {
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("x", "INT"),
            ("y", "INT"),
            ("q1", "BOOL"),
            ("q2", "BOOL"),
            ("q3", "BOOL"),
            ("s1", "INT"),
            ("s2", "INT"),
            ("s3", "INT"),
            ("t1", "REAL"),
        ])
        // Numbers, drawn first in the document, below the logic.
        .invar(10, 0, 200, "x")
        .invar(11, 0, 240, "y")
        .block(
            12,
            120,
            220,
            "ADD",
            None,
            &[("IN1", In::From(&["10"])), ("IN2", In::From(&["11"]))],
            &["OUT"],
        )
        .block(
            13,
            240,
            200,
            "MUL",
            None,
            &[("IN1", In::From(&["12"])), ("IN2", In::Expr("3"))],
            &["OUT"],
        )
        .invar(14, 120, 300, "7")
        .block(
            15,
            360,
            200,
            "SUB",
            None,
            &[("IN1", In::From(&["13"])), ("IN2", In::From(&["14"]))],
            &["OUT"],
        )
        .outvar(16, 480, 200, "s1", &["15"])
        .block(
            17,
            360,
            280,
            "GT",
            None,
            &[("IN1", In::From(&["12"])), ("IN2", In::From(&["14"]))],
            &["OUT"],
        )
        .outvar(18, 480, 280, "q3", &["17"])
        .block(
            19,
            360,
            340,
            "SEL",
            None,
            &[
                ("G", In::From(&["17"])),
                ("IN0", In::From(&["12"])),
                ("IN1", In::From(&["13"])),
            ],
            &["OUT"],
        )
        .outvar(20, 480, 340, "s2", &["19"])
        .block(
            21,
            360,
            400,
            "MAX",
            None,
            &[
                ("IN1", In::From(&["10"])),
                ("IN2", In::From(&["11"])),
                ("IN3", In::Expr("0")),
            ],
            &["OUT"],
        )
        .outvar(22, 480, 400, "s3", &["21"])
        .block(
            23,
            240,
            460,
            "INT_TO_REAL",
            None,
            &[("IN", In::From(&["15"]))],
            &["OUT"],
        )
        .block(
            24,
            360,
            460,
            "DIV",
            None,
            &[("IN1", In::From(&["23"])), ("IN2", In::Expr("2.0"))],
            &["OUT"],
        )
        .outvar(25, 480, 460, "t1", &["24"])
        // Logic.
        .invar(1, 0, 0, "a")
        .invar_neg(2, 0, 40, "b")
        .block_neg(
            3,
            120,
            0,
            "AND",
            None,
            &[
                ("IN1", In::From(&["1"]), false),
                ("IN2", In::From(&["2"]), true),
            ],
            &[("OUT", false)],
        )
        .invar(5, 120, 60, "c")
        .block_neg(
            4,
            240,
            0,
            "OR",
            None,
            &[
                ("IN1", In::From(&["3"]), false),
                ("IN2", In::From(&["5"]), false),
            ],
            &[("OUT", true)],
        )
        .outvar(7, 360, 0, "q1", &["4"])
        .block(
            8,
            240,
            60,
            "XOR",
            None,
            &[("IN1", In::From(&["5"])), ("IN2", In::Expr("TRUE"))],
            &["OUT"],
        )
        .outvar_neg(9, 360, 60, "q2", &["8"]);
    let twin = "PROGRAM Main
VAR a, b, c : BOOL; x, y : INT; q1, q2, q3 : BOOL; s1, s2, s3 : INT; t1 : REAL;
    n_1, n_10 : BOOL;
    w_1, w_2, w_3, w_4, w_5, w_8, w_17 : BOOL;
    w_10, w_11, w_12, w_13, w_15, w_19, w_21 : INT;
    w_23, w_24 : REAL;
END_VAR
n_1 := TRUE;
w_1 := a; w_2 := NOT b; w_3 := w_1 AND NOT w_2; w_5 := c; w_4 := NOT (w_3 OR w_5);
q1 := w_4; w_8 := w_5 XOR TRUE; q2 := NOT w_8;
n_10 := TRUE;
w_10 := x; w_11 := y; w_12 := w_10 + w_11; w_13 := w_12 * 3; w_15 := w_13 - 7; s1 := w_15;
w_17 := w_12 > 7; q3 := w_17; w_19 := SEL(w_17, w_12, w_13); s2 := w_19;
w_21 := MAX(w_10, w_11, 0); s3 := w_21; w_23 := INT_TO_REAL(w_15); w_24 := w_23 / 2.0; t1 := w_24;
END_PROGRAM";
    compare(
        "functions",
        fbd_run(vec![main]),
        st_run(twin),
        &[
            Input::Bool("a"),
            Input::Bool("b"),
            Input::Bool("c"),
            Input::Int("x", -1000, 1000),
            Input::Int("y", -1000, 1000),
        ],
    );
}

#[test]
fn function_blocks_en_and_eno_equal_their_st_twin() {
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("en", "BOOL"),
            ("r", "BOOL"),
            ("x", "INT"),
            ("t", "TON"),
            ("c", "CTU"),
            ("tr", "R_TRIG"),
            ("l", "SR"),
            ("q_t", "BOOL"),
            ("q_c", "BOOL"),
            ("q_r", "BOOL"),
            ("q_l", "BOOL"),
            ("eno_c", "BOOL"),
            ("e2", "BOOL"),
            ("et", "TIME"),
            ("cv", "INT"),
            ("s", "INT"),
        ])
        // A timer.
        .invar(1, 0, 0, "a")
        .block(
            2,
            120,
            0,
            "TON",
            Some("t"),
            &[("IN", In::From(&["1"])), ("PT", In::Expr("T#50ms"))],
            &["Q", "ET"],
        )
        .outvar(3, 240, 0, "q_t", &["2.Q"])
        .outvar(4, 240, 40, "et", &["2.ET"])
        // A counter under EN, its Q negated.
        .invar(10, 0, 100, "x")
        .block(
            11,
            120,
            100,
            "GT",
            None,
            &[("IN1", In::From(&["10"])), ("IN2", In::Expr("0"))],
            &["OUT"],
        )
        .invar(12, 0, 160, "b")
        .invar(13, 0, 200, "r")
        .block_neg(
            14,
            240,
            100,
            "CTU",
            Some("c"),
            &[
                ("EN", In::From(&["11"]), false),
                ("CU", In::From(&["12"]), false),
                ("R", In::From(&["13"]), false),
                ("PV", In::Expr("5"), false),
            ],
            &[("ENO", false), ("Q", true), ("CV", false)],
        )
        .outvar(15, 360, 100, "q_c", &["14.Q"])
        .outvar(16, 360, 140, "cv", &["14.CV"])
        .outvar(17, 360, 180, "eno_c", &["14.ENO"])
        // An edge and a bistable.
        .invar(20, 0, 300, "a")
        .block(
            21,
            120,
            300,
            "R_TRIG",
            Some("tr"),
            &[("CLK", In::From(&["20"]))],
            &["Q"],
        )
        .invar(22, 0, 360, "b")
        .block(
            23,
            240,
            300,
            "SR",
            Some("l"),
            &[("S1", In::From(&["21.Q"])), ("R", In::From(&["22"]))],
            &["Q1"],
        )
        .outvar(24, 360, 300, "q_l", &["23.Q1"])
        .outvar(25, 360, 340, "q_r", &["21.Q"])
        // A chain of functions under EN.
        .invar(30, 0, 500, "en")
        .invar(31, 0, 540, "x")
        .block(
            32,
            120,
            500,
            "ADD",
            None,
            &[
                ("EN", In::From(&["30"])),
                ("IN1", In::From(&["31"])),
                ("IN2", In::Expr("1")),
            ],
            &["ENO", "OUT"],
        )
        .block(
            33,
            240,
            500,
            "MUL",
            None,
            &[
                ("EN", In::From(&["32.ENO"])),
                ("IN1", In::From(&["32.OUT"])),
                ("IN2", In::Expr("2")),
            ],
            &["ENO", "OUT"],
        )
        .outvar(34, 360, 500, "s", &["33.OUT"])
        .outvar(35, 360, 540, "e2", &["33.ENO"]);
    let twin = "PROGRAM Main
VAR a, b, en, r : BOOL; x : INT; t : TON; c : CTU; tr : R_TRIG; l : SR;
    q_t, q_c, q_r, q_l, eno_c, e2 : BOOL; et : TIME; cv, s : INT;
    n_1, n_10, n_20, n_30 : BOOL;
    w_1, w_2_Q, w_11, w_12, w_13, w_14_ENO, w_14_Q, w_20, w_21_Q, w_22, w_23_Q1 : BOOL;
    w_30, w_32_ENO, w_33_ENO : BOOL;
    w_2_ET : TIME;
    w_10, w_14_CV, w_31, w_32, w_33 : INT;
END_VAR
n_1 := TRUE;
w_1 := a; t(IN := w_1, PT := T#50ms); w_2_Q := t.Q; w_2_ET := t.ET; q_t := w_2_Q; et := w_2_ET;
n_10 := TRUE;
w_10 := x; w_11 := w_10 > 0; w_12 := b; w_13 := r; w_14_ENO := w_11;
IF w_14_ENO THEN c(CU := w_12, R := w_13, PV := 5); END_IF;
w_14_Q := NOT c.Q; w_14_CV := c.CV; q_c := w_14_Q; cv := w_14_CV; eno_c := w_14_ENO;
n_20 := TRUE;
w_20 := a; tr(CLK := w_20); w_21_Q := tr.Q; q_r := w_21_Q; w_22 := b;
l(S1 := w_21_Q, R := w_22); w_23_Q1 := l.Q1; q_l := w_23_Q1;
n_30 := TRUE;
w_30 := en; w_31 := x; w_32_ENO := w_30;
IF w_32_ENO THEN w_32 := w_31 + 1; END_IF;
w_33_ENO := w_32_ENO AND w_32_ENO;
IF w_33_ENO THEN w_33 := w_32 * 2; END_IF;
IF w_33_ENO THEN s := w_33; END_IF;
e2 := w_33_ENO;
END_PROGRAM";
    compare(
        "blocks",
        fbd_run(vec![main]),
        st_run(twin),
        &[
            Input::Bool("a"),
            Input::Bool("b"),
            Input::Bool("en"),
            Input::Bool("r"),
            Input::Int("x", -3, 3),
        ],
    );
}

#[test]
fn lines_jumps_and_a_return_equal_their_st_twin() {
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("skip", "BOOL"),
            ("stop", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
            ("n", "INT"),
            ("k", "INT"),
            ("m", "INT"),
            ("last", "INT"),
        ])
        // After the return: drawn first in the document.
        .invar(27, 0, 480, "m")
        .outvar(28, 120, 480, "last", &["27"])
        .comment(29, 300, 480, "after the return")
        // The loop: m counts up to n, at least once.
        .label(16, 330, "AGAIN")
        .invar(17, 0, 340, "m")
        .block(
            18,
            120,
            340,
            "ADD",
            None,
            &[("IN1", In::From(&["17"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .inoutvar(19, 240, 340, "m", &["18"])
        .block(
            20,
            360,
            340,
            "LT",
            None,
            &[("IN1", In::From(&["19"])), ("IN2", In::Expr("n"))],
            &["OUT"],
        )
        .jump(24, 480, 340, "AGAIN", &["20"])
        .invar(25, 0, 420, "stop")
        .ret(26, 120, 420, &["25"])
        // A line continued.
        .invar(1, 0, 0, "a")
        .invar(2, 0, 40, "b")
        .block(
            3,
            120,
            0,
            "AND",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::From(&["2"]))],
            &["OUT"],
        )
        .connector(4, 240, 0, "L", &["3"])
        .continuation(5, 0, 100, "L")
        .outvar(6, 120, 100, "q", &["5"])
        .continuation(7, 0, 140, "L")
        .block(
            8,
            120,
            140,
            "NOT",
            None,
            &[("IN", In::From(&["7"]))],
            &["OUT"],
        )
        .outvar(9, 240, 140, "r", &["8"])
        // A jump forward over k.
        .invar(10, 0, 200, "skip")
        .jump(11, 120, 200, "OVER", &["10"])
        .invar(12, 0, 260, "k")
        .block(
            13,
            120,
            260,
            "ADD",
            None,
            &[("IN1", In::From(&["12"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(14, 240, 260, "k", &["13"])
        .label(15, 290, "OVER")
        .invar(22, 0, 300, "0")
        .outvar(23, 120, 300, "m", &["22"]);
    let twin = "PROGRAM Main
VAR a, b, skip, stop, q, r : BOOL; n, k, m, last : INT;
    n_1, n_10, n_12, n_22, n_17, n_25, n_27 : BOOL;
    w_1, w_2, w_3, w_5, w_7, w_8, w_10, w_20, w_25 : BOOL;
    w_12, w_13, w_17, w_18, w_19, w_27 : INT;
END_VAR
n_1 := FALSE; n_10 := FALSE; n_12 := FALSE; n_22 := FALSE; n_17 := FALSE; n_25 := FALSE; n_27 := FALSE;
n_1 := TRUE;
w_1 := a; w_2 := b; w_3 := w_1 AND w_2; w_5 := w_3; w_7 := w_3; q := w_5; w_8 := NOT w_7; r := w_8;
n_10 := TRUE;
w_10 := skip;
IF NOT w_10 THEN
    n_12 := TRUE;
    w_12 := k; w_13 := w_12 + 1; k := w_13;
END_IF;
n_22 := TRUE;
m := 0;
REPEAT
    n_17 := TRUE;
    w_17 := m; w_18 := w_17 + 1; m := w_18; w_19 := m; w_20 := w_19 < n;
UNTIL NOT w_20 END_REPEAT;
n_25 := TRUE;
w_25 := stop;
IF w_25 THEN RETURN; END_IF;
n_27 := TRUE;
w_27 := m; last := w_27;
END_PROGRAM";
    compare(
        "lines and control",
        fbd_run(vec![main]),
        st_run(twin),
        &[
            Input::Bool("a"),
            Input::Bool("b"),
            Input::Bool("skip"),
            Input::Bool("stop"),
            Input::Int("n", 0, 5),
        ],
    );
}

#[test]
fn feedback_paths_equal_their_st_twin() {
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("enable", "BOOL"),
            ("start1", "BOOL"),
            ("start2", "BOOL"),
            ("run", "BOOL"),
            ("a", "BOOL"),
            ("rst", "BOOL"),
            ("c", "CTU"),
            ("q", "BOOL"),
            ("pv", "INT"),
            ("t", "BOOL"),
            ("tog", "BOOL"),
            ("run2", "BOOL"),
        ])
        // §8.1.5.2 example a: an explicit loop.
        .invar(1, 0, 0, "enable")
        .invar(2, 0, 60, "start1")
        .invar(3, 0, 100, "start2")
        .block(
            4,
            120,
            60,
            "OR",
            None,
            &[
                ("IN1", In::From(&["2"])),
                ("IN2", In::From(&["3"])),
                ("IN3", In::From(&["5"])),
            ],
            &["OUT"],
        )
        .block(
            5,
            240,
            0,
            "AND",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::From(&["4"]))],
            &["OUT"],
        )
        .outvar(6, 360, 0, "run", &["5"])
        // A function block's output drawn back.
        .invar(10, 0, 200, "a")
        .block(
            11,
            120,
            240,
            "ADD",
            None,
            &[("IN1", In::From(&["12.CV"])), ("IN2", In::Expr("2"))],
            &["OUT"],
        )
        .block(
            12,
            240,
            200,
            "CTU",
            Some("c"),
            &[
                ("CU", In::From(&["10"])),
                ("R", In::From(&["13"])),
                ("PV", In::From(&["11"])),
            ],
            &["Q", "CV"],
        )
        .invar(13, 0, 260, "rst")
        .outvar(14, 360, 200, "q", &["12.Q"])
        .outvar(15, 360, 240, "pv", &["11"])
        // A block's own output drawn back: a toggle.
        .invar(20, 0, 400, "t")
        .block(
            21,
            120,
            400,
            "XOR",
            None,
            &[("IN1", In::From(&["20"])), ("IN2", In::From(&["21"]))],
            &["OUT"],
        )
        .outvar(22, 240, 400, "tog", &["21"])
        // §8.1.5.2 example b: an implicit loop, through the variable.
        .invar(30, 0, 500, "enable")
        .invar(31, 0, 540, "start1")
        .invar(32, 0, 580, "run2")
        .block(
            33,
            120,
            540,
            "OR",
            None,
            &[("IN1", In::From(&["31"])), ("IN2", In::From(&["32"]))],
            &["OUT"],
        )
        .block(
            34,
            240,
            500,
            "AND",
            None,
            &[("IN1", In::From(&["30"])), ("IN2", In::From(&["33"]))],
            &["OUT"],
        )
        .outvar(35, 360, 500, "run2", &["34"]);
    let twin = "PROGRAM Main
VAR enable, start1, start2, run, a, rst, q, t, tog, run2 : BOOL; c : CTU; pv : INT;
    n_1, n_10, n_20, n_30 : BOOL;
    w_1, w_2, w_3, w_4, w_5, w_10, w_12_Q, w_13, w_20, w_21, w_30, w_31, w_32, w_33, w_34 : BOOL;
    w_11, w_12_CV : INT;
END_VAR
n_1 := TRUE;
w_1 := enable; w_2 := start1; w_3 := start2; w_4 := w_2 OR w_3 OR w_5; w_5 := w_1 AND w_4;
run := w_5;
n_10 := TRUE;
w_10 := a; w_11 := w_12_CV + 2; w_13 := rst; c(CU := w_10, R := w_13, PV := w_11);
w_12_Q := c.Q; w_12_CV := c.CV; q := w_12_Q; pv := w_11;
n_20 := TRUE;
w_20 := t; w_21 := w_20 XOR w_21; tog := w_21;
n_30 := TRUE;
w_30 := enable; w_31 := start1; w_32 := run2; w_33 := w_31 OR w_32; w_34 := w_30 AND w_33;
run2 := w_34;
END_PROGRAM";
    compare(
        "feedback",
        fbd_run(vec![main]),
        st_run(twin),
        &[
            Input::Bool("enable"),
            Input::Bool("start1"),
            Input::Bool("start2"),
            Input::Bool("a"),
            Input::Bool("rst"),
            Input::Bool("t"),
        ],
    );
}

#[test]
fn fbd_and_ld_pous_called_from_fbd_equal_their_st_twin() {
    let twice = Pou::function("Twice", "INT")
        .fbd()
        .inputs(&[("v", "INT")])
        .invar(1, 0, 0, "v")
        .block(
            2,
            120,
            0,
            "MUL",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::Expr("2"))],
            &["OUT"],
        )
        .outvar(3, 240, 0, "Twice", &["2"]);
    let latch = Pou::function_block("Latch")
        .fbd()
        .inputs(&[("s", "BOOL"), ("r", "BOOL")])
        .outputs(&[("q", "BOOL")])
        .vars(&[("m", "SR")])
        .invar(1, 0, 0, "s")
        .invar(2, 0, 40, "r")
        .block(
            3,
            120,
            0,
            "SR",
            Some("m"),
            &[("S1", In::From(&["1"])), ("R", In::From(&["2"]))],
            &["Q1"],
        )
        .outvar(4, 240, 0, "q", &["3.Q1"]);
    let pass = Pou::function_block("Pass")
        .inputs(&[("i", "BOOL")])
        .outputs(&[("o", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 0, tc6::Contact::No, "i", &["1"])
        .coil(3, 200, 0, tc6::Coil::Plain, "o", &["2"])
        .right(4, 0, &["3"]);
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("x", "INT"),
            ("y", "INT"),
            ("s", "BOOL"),
            ("r", "BOOL"),
            ("q", "BOOL"),
            ("l", "Latch"),
            ("p", "Pass"),
        ])
        .invar(1, 0, 0, "x")
        .block(
            2,
            120,
            0,
            "Twice",
            None,
            &[("v", In::From(&["1"]))],
            &["OUT"],
        )
        .outvar(3, 240, 0, "y", &["2"])
        .invar(4, 0, 100, "s")
        .invar(5, 0, 140, "r")
        .block(
            6,
            120,
            100,
            "Latch",
            Some("l"),
            &[("s", In::From(&["4"])), ("r", In::From(&["5"]))],
            &["q"],
        )
        .block(
            7,
            240,
            100,
            "Pass",
            Some("p"),
            &[("i", In::From(&["6.q"]))],
            &["o"],
        )
        .outvar(8, 360, 100, "q", &["7.o"]);
    let twin = "FUNCTION Twice : INT VAR_INPUT v : INT; END_VAR Twice := v * 2; END_FUNCTION
FUNCTION_BLOCK Latch
VAR_INPUT s, r : BOOL; END_VAR VAR_OUTPUT q : BOOL; END_VAR VAR m : SR; END_VAR
m(S1 := s, R := r); q := m.Q1;
END_FUNCTION_BLOCK
FUNCTION_BLOCK Pass
VAR_INPUT i : BOOL; END_VAR VAR_OUTPUT o : BOOL; END_VAR
o := i;
END_FUNCTION_BLOCK
PROGRAM Main
VAR x, y : INT; s, r, q : BOOL; l : Latch; p : Pass;
    n_1, n_4 : BOOL;
    w_4, w_5, w_6_q, w_7_o : BOOL;
    w_1, w_2 : INT;
END_VAR
n_1 := TRUE;
w_1 := x; w_2 := Twice(v := w_1); y := w_2;
n_4 := TRUE;
w_4 := s; w_5 := r; l(s := w_4, r := w_5); w_6_q := l.q; p(i := w_6_q); w_7_o := p.o; q := w_7_o;
END_PROGRAM";
    compare(
        "pous",
        fbd_run(vec![twice, latch, pass, main]),
        st_run(twin),
        &[
            Input::Int("x", -1000, 1000),
            Input::Bool("s"),
            Input::Bool("r"),
        ],
    );
}
