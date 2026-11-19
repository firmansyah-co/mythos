//! Each Ladder Diagram program here has a twin written by hand in Structured Text, and the
//! two, compiled, run on the VM through the same random inputs: every variable they share is
//! equal after every scan (ADR-013 §2.15).
//!
//! The twins cover every element the LD front-end reads: contacts in series and in parallel,
//! negated, rising and falling contacts; plain, negated, set, reset, rising and falling coils;
//! a coil passing power on and a seal-in by its own variable; a TON and a CTU (with EN and
//! ENO); a function of the project written in ST, standard functions as blocks, a function
//! with EN; a forward jump, a backward jump and a return. The ST twins write the edges with
//! R_TRIG and F_TRIG (Table 44), so this also holds the LD edges to the standard's blocks.
//!
//! What is compared: every symbol of the LD program but its hidden ones (`__EDGE_*`, `__LD_*`,
//! `__NET_*`, `__JUMP`) is in the ST twin, and every one of the twin's but its edge detectors (`h_*`) is
//! in the LD program; each of those, read after each scan, is equal.

mod support;
#[path = "../../mythos-logic-graphic/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{compile, compile_sources, Sources, TargetProfile};
use mythos_logic_vm::Value;
use support::Run;
use tc6::{project, Coil, Contact, In, Pou};

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

/// An input: its name and how to draw a value of it.
enum Input {
    Bool(&'static str),
    Int(&'static str, i16, i16),
}

fn compare(name: &str, ld: impl Fn() -> Run, st: impl Fn() -> Run, inputs: &[Input]) {
    let (l, s) = (ld(), st());
    // What monitoring costs (ADR-013 §2.15): the variables keeping power and rung flags.
    let kept =
        l.c.symbols
            .iter()
            .filter(|sym| sym.path.starts_with("__LD_") || sym.path.starts_with("__NET_"))
            .count();
    eprintln!(
        "{name}: the LD program is {} bytes of code and {} of memory, {kept} variables keeping power",
        l.c.code_size, l.c.memory_size
    );
    let shared: Vec<String> =
        l.c.symbols
            .iter()
            .filter(|sym| !sym.path.starts_with("__") && !sym.path.contains(".__"))
            .map(|sym| sym.path.clone())
            .collect();
    for path in &shared {
        let twin =
            s.c.symbol(path)
                .unwrap_or_else(|| panic!("{name}: the ST twin has no {path}"));
        assert_eq!(twin.ty, l.c.symbol(path).unwrap().ty, "{name}: {path}");
    }
    for sym in &s.c.symbols {
        if !sym.path.starts_with("h_") {
            assert!(
                l.c.symbol(&sym.path).is_some(),
                "{name}: the LD program has no {}",
                sym.path
            );
        }
    }
    assert!(shared.len() >= 5, "{name}: something to compare");
    let mut values = 0usize;
    for seed in 1..=SEQUENCES {
        let (mut l, mut s) = (ld(), st());
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut now = 0i64;
        for scan in 0..SCANS {
            for input in inputs {
                let (path, v) = match input {
                    Input::Bool(p) => (*p, Value::Bool(rng.bool())),
                    Input::Int(p, lo, hi) => (*p, Value::Int(rng.int(*lo, *hi))),
                };
                l.set(path, v);
                s.set(path, v);
            }
            now += (rng.next() % 30) as i64;
            l.scan_at(now);
            s.scan_at(now);
            for path in &shared {
                assert_eq!(
                    l.get(path),
                    s.get(path),
                    "{name}: {path} differs after scan {} of sequence {seed}",
                    scan + 1
                );
                values += 1;
            }
        }
    }
    assert_eq!(values, SEQUENCES as usize * SCANS * shared.len());
    eprintln!("{name}: {} variables, {values} values equal", shared.len());
}

fn ld_run(pous: Vec<Pou>, st: Option<&'static str>) -> impl Fn() -> Run {
    let doc = project(&pous);
    move || {
        let ld = [("main.xml", doc.as_str())];
        Run::of(compile_sources(
            &Sources {
                st: st.map(|t| ("pous.st", t)),
                tc6: &ld,
            },
            &TargetProfile::pc(),
        ))
    }
}

fn st_run(src: &'static str) -> impl Fn() -> Run {
    move || Run::of(compile(src, &TargetProfile::pc()))
}

#[test]
fn contacts_and_coils_equal_their_st_twin() {
    let main = Pou::program("Main")
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("d", "BOOL"),
            ("e", "BOOL"),
            ("q1", "BOOL"),
            ("q2", "BOOL"),
            ("q3", "BOOL"),
            ("q4", "BOOL"),
            ("q5", "BOOL"),
            ("q6", "BOOL"),
            ("q7", "BOOL"),
            ("q8", "BOOL"),
            ("q9", "BOOL"),
            ("run", "BOOL"),
        ])
        .left(1, 0)
        // q1 := (a AND NOT b) OR (c AND d)
        .contact(10, 100, 0, Contact::No, "a", &["1"])
        .contact(11, 200, 0, Contact::Nc, "b", &["10"])
        .contact(12, 100, 20, Contact::No, "c", &["1"])
        .contact(13, 200, 20, Contact::No, "d", &["12"])
        .coil(14, 300, 0, Coil::Plain, "q1", &["11", "13"])
        // q2 := a AND rising(b)
        .contact(20, 100, 100, Contact::No, "a", &["1"])
        .contact(21, 200, 100, Contact::Rising, "b", &["20"])
        .coil(22, 300, 100, Coil::Plain, "q2", &["21"])
        // q3 := c AND falling(d)
        .contact(30, 100, 200, Contact::No, "c", &["1"])
        .contact(31, 200, 200, Contact::Falling, "d", &["30"])
        .coil(32, 300, 200, Coil::Plain, "q3", &["31"])
        // set q4 on a AND c, then reset it on b AND d
        .contact(40, 100, 300, Contact::No, "a", &["1"])
        .contact(41, 200, 300, Contact::No, "c", &["40"])
        .coil(42, 300, 300, Coil::Set, "q4", &["41"])
        .contact(43, 100, 400, Contact::No, "b", &["1"])
        .contact(44, 200, 400, Contact::No, "d", &["43"])
        .coil(45, 300, 400, Coil::Reset, "q4", &["44"])
        // q5 := NOT (e OR a)
        .contact(50, 100, 500, Contact::No, "e", &["1"])
        .contact(51, 100, 520, Contact::No, "a", &["1"])
        .coil(52, 300, 500, Coil::Negated, "q5", &["50", "51"])
        // q6 rising on a AND e; q7 falling on b OR c
        .contact(60, 100, 600, Contact::No, "a", &["1"])
        .contact(61, 200, 600, Contact::No, "e", &["60"])
        .coil(62, 300, 600, Coil::Rising, "q6", &["61"])
        .contact(70, 100, 700, Contact::No, "b", &["1"])
        .contact(71, 100, 720, Contact::No, "c", &["1"])
        .coil(72, 300, 700, Coil::Falling, "q7", &["70", "71"])
        // q8 := e; q9 := e AND a, through the coil q8
        .contact(80, 100, 800, Contact::No, "e", &["1"])
        .coil(81, 200, 800, Coil::Plain, "q8", &["80"])
        .contact(82, 300, 800, Contact::No, "a", &["81"])
        .coil(83, 400, 800, Coil::Plain, "q9", &["82"])
        // run := (a OR run) AND NOT b, a seal-in
        .contact(90, 100, 900, Contact::No, "a", &["1"])
        .contact(91, 100, 920, Contact::No, "run", &["1"])
        .contact(92, 200, 900, Contact::Nc, "b", &["90", "91"])
        .coil(93, 300, 900, Coil::Plain, "run", &["92"])
        .right(
            2,
            0,
            &["14", "22", "32", "42", "45", "52", "62", "72", "83", "93"],
        );
    let st = "PROGRAM Main
        VAR
            a, b, c, d, e : BOOL;
            q1, q2, q3, q4, q5, q6, q7, q8, q9, run : BOOL;
            h_b : R_TRIG; h_d : F_TRIG; h_q6 : R_TRIG; h_q7 : F_TRIG;
        END_VAR
        q1 := (a AND NOT b) OR (c AND d);
        h_b(CLK := b); q2 := a AND h_b.Q;
        h_d(CLK := d); q3 := c AND h_d.Q;
        IF a AND c THEN q4 := TRUE; END_IF;
        IF b AND d THEN q4 := FALSE; END_IF;
        q5 := NOT (e OR a);
        h_q6(CLK := a AND e); q6 := h_q6.Q;
        h_q7(CLK := b OR c); q7 := h_q7.Q;
        q8 := e; q9 := e AND a;
        run := (a OR run) AND NOT b;
    END_PROGRAM";
    compare(
        "contacts and coils",
        ld_run(vec![main], None),
        st_run(st),
        &[
            Input::Bool("a"),
            Input::Bool("b"),
            Input::Bool("c"),
            Input::Bool("d"),
            Input::Bool("e"),
        ],
    );
}

#[test]
fn a_ton_and_a_ctu_equal_their_st_twin() {
    let main = Pou::program("Main")
        .vars(&[
            ("start", "BOOL"),
            ("pulse", "BOOL"),
            ("reset", "BOOL"),
            ("en", "BOOL"),
            ("q", "BOOL"),
            ("done", "BOOL"),
            ("ok", "BOOL"),
            ("et", "TIME"),
            ("cv", "INT"),
            ("t", "TON"),
            ("c", "CTU"),
        ])
        .left(1, 0)
        .contact(10, 100, 0, Contact::No, "start", &["1"])
        .block(
            11,
            200,
            0,
            "TON",
            Some("t"),
            &[("IN", In::From(&["10"])), ("PT", In::Expr("T#40ms"))],
            &["Q", "ET"],
        )
        .coil(12, 400, 0, Coil::Plain, "q", &["11.Q"])
        .outvar(13, 400, 40, "et", &["11.ET"])
        .contact(20, 100, 100, Contact::No, "en", &["1"])
        .contact(21, 100, 140, Contact::No, "pulse", &["1"])
        .invar(22, 100, 160, "reset")
        .block(
            23,
            200,
            100,
            "CTU",
            Some("c"),
            &[
                ("EN", In::From(&["20"])),
                ("CU", In::From(&["21"])),
                ("R", In::From(&["22"])),
                ("PV", In::Expr("5")),
            ],
            &["ENO", "Q", "CV"],
        )
        .coil(24, 400, 100, Coil::Plain, "ok", &["23.ENO"])
        .coil(25, 400, 140, Coil::Plain, "done", &["23.Q"])
        .outvar(26, 400, 180, "cv", &["23.CV"])
        .right(2, 0, &["12", "24", "25"]);
    let st = "PROGRAM Main
        VAR
            start, pulse, reset, en, q, done, ok : BOOL; et : TIME; cv : INT;
            t : TON; c : CTU;
        END_VAR
        t(IN := start, PT := T#40ms); q := t.Q; et := t.ET;
        IF en THEN c(CU := pulse, R := reset, PV := 5); END_IF;
        ok := en; done := c.Q; cv := c.CV;
    END_PROGRAM";
    compare(
        "TON and CTU",
        ld_run(vec![main], None),
        st_run(st),
        &[
            Input::Bool("start"),
            Input::Bool("pulse"),
            Input::Bool("reset"),
            Input::Bool("en"),
        ],
    );
}

const SCALE: &str = "FUNCTION Scale : INT
    VAR_INPUT raw : INT; gain : INT; END_VAR
    Scale := raw * gain;
END_FUNCTION";

#[test]
fn functions_equal_their_st_twin() {
    let main = Pou::program("Main")
        .vars(&[
            ("a", "INT"),
            ("g", "INT"),
            ("en", "BOOL"),
            ("out", "INT"),
            ("diff", "INT"),
            ("big", "BOOL"),
            ("ok", "BOOL"),
        ])
        .left(1, 0)
        .invar(10, 0, 0, "a")
        .block(
            11,
            100,
            0,
            "ADD",
            None,
            &[("IN1", In::From(&["10"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .block(
            12,
            200,
            0,
            "Scale",
            None,
            &[("raw", In::From(&["11"])), ("gain", In::Expr("g"))],
            &["OUT"],
        )
        .outvar(13, 300, 0, "out", &["12"])
        .invar(20, 0, 100, "out")
        .block(
            21,
            100,
            100,
            "GT",
            None,
            &[("IN1", In::From(&["20"])), ("IN2", In::Expr("10"))],
            &["OUT"],
        )
        .coil(22, 300, 100, Coil::Plain, "big", &["21"])
        .contact(30, 100, 200, Contact::No, "en", &["1"])
        .block(
            31,
            200,
            200,
            "SUB",
            None,
            &[
                ("EN", In::From(&["30"])),
                ("IN1", In::Expr("a")),
                ("IN2", In::Expr("g")),
            ],
            &["ENO", "OUT"],
        )
        .coil(32, 400, 200, Coil::Plain, "ok", &["31.ENO"])
        .outvar(33, 400, 240, "diff", &["31.OUT"])
        .right(2, 0, &["22", "32"]);
    let st = "FUNCTION Scale : INT
            VAR_INPUT raw : INT; gain : INT; END_VAR
            Scale := raw * gain;
        END_FUNCTION
        PROGRAM Main
        VAR a, g, out, diff : INT; en, big, ok : BOOL; END_VAR
        out := Scale(raw := a + 1, gain := g);
        big := out > 10;
        IF en THEN diff := a - g; END_IF;
        ok := en;
    END_PROGRAM";
    compare(
        "functions",
        ld_run(vec![main], Some(SCALE)),
        st_run(st),
        &[
            Input::Int("a", -20, 20),
            Input::Int("g", -4, 4),
            Input::Bool("en"),
        ],
    );
}

#[test]
fn jumps_and_a_return_equal_their_st_twin() {
    let main = Pou::program("Main")
        .vars(&[
            ("skip", "BOOL"),
            ("stop", "BOOL"),
            ("a", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
            ("i", "INT"),
            ("n", "INT"),
        ])
        .left(1, 0)
        .contact(10, 100, 0, Contact::No, "skip", &["1"])
        .jump(11, 200, 0, "OVER", &["10"])
        .contact(20, 100, 100, Contact::No, "a", &["1"])
        .coil(21, 200, 100, Coil::Plain, "q", &["20"])
        .label(30, 150, "OVER")
        .invar(31, 0, 200, "0")
        .outvar(32, 100, 200, "i", &["31"])
        .label(40, 300, "LOOP")
        .invar(41, 0, 320, "i")
        .block(
            42,
            100,
            320,
            "ADD",
            None,
            &[("IN1", In::From(&["41"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(43, 200, 320, "i", &["42"])
        .block(
            50,
            100,
            400,
            "LT",
            None,
            &[("IN1", In::Expr("i")), ("IN2", In::Expr("n"))],
            &["OUT"],
        )
        .jump(51, 200, 400, "LOOP", &["50"])
        .contact(60, 100, 500, Contact::No, "stop", &["1"])
        .ret(61, 200, 500, &["60"])
        .contact(70, 100, 600, Contact::No, "a", &["1"])
        .coil(71, 200, 600, Coil::Negated, "r", &["70"])
        .right(2, 0, &["21", "71"]);
    let st = "PROGRAM Main
        VAR skip, stop, a, q, r : BOOL; i, n : INT; END_VAR
        IF NOT skip THEN q := a; END_IF;
        i := 0;
        REPEAT i := i + 1; UNTIL NOT (i < n) END_REPEAT;
        IF stop THEN RETURN; END_IF;
        r := NOT a;
    END_PROGRAM";
    compare(
        "jumps and a return",
        ld_run(vec![main], None),
        st_run(st),
        &[
            Input::Bool("skip"),
            Input::Bool("stop"),
            Input::Bool("a"),
            Input::Int("n", -2, 6),
        ],
    );
}
