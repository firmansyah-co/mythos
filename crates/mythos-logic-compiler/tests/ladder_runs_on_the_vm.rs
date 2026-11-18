//! Ladder Diagram programs, compiled from TC6 XML, run on the VM: each element of IEC
//! 61131-3 ed. 3 §8.2 does what its table says, scan by scan (ADR-013 §2.15).

mod support;
#[path = "../../mythos-logic-graphic/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{compile_in, compile_sources, Language, Sources, TargetProfile};
use mythos_logic_vm::Value;
use support::Run;
use tc6::{project, Coil, Contact, In, Pou};

fn ld(pous: &[Pou]) -> Run {
    Run::of(compile_in(
        &project(pous),
        Language::Ld,
        &TargetProfile::pc(),
    ))
}

fn with_st(pous: &[Pou], st: &str) -> Run {
    let doc = project(pous);
    let ld = [("main.xml", doc.as_str())];
    Run::of(compile_sources(
        &Sources {
            st: Some(("pous.st", st)),
            tc6: &ld,
        },
        &TargetProfile::pc(),
    ))
}

fn set(r: &mut Run, var: &str, v: bool) {
    r.set(var, Value::Bool(v));
}

/// One rung: the left rail, a contact of `kind` on `x`, a coil on `q`.
fn one_contact(kind: Contact) -> Pou {
    Pou::program("Main")
        .vars(&[("x", "BOOL"), ("en", "BOOL"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "en", &["1"])
        .contact(3, 200, 20, kind, "x", &["2"])
        .coil(4, 300, 20, Coil::Plain, "q", &["3"])
        .right(5, 0, &["4"])
}

#[test]
fn series_is_and_and_parallel_is_or() {
    // q := (a AND b) OR c: the vertical link before the coil is the OR (§8.2.3).
    let p = Pou::program("Main")
        .vars(&[("a", "BOOL"), ("b", "BOOL"), ("c", "BOOL"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        .contact(3, 200, 20, Contact::No, "b", &["2"])
        .contact(4, 100, 60, Contact::No, "c", &["1"])
        .coil(5, 300, 20, Coil::Plain, "q", &["3", "4"])
        .right(6, 0, &["5"]);
    let mut r = ld(&[p]);
    for bits in 0..8u8 {
        let (a, b, c) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
        set(&mut r, "a", a);
        set(&mut r, "b", b);
        set(&mut r, "c", c);
        r.scan().unwrap();
        assert_eq!(r.bool("q"), (a && b) || c, "a={a} b={b} c={c}");
    }
}

#[test]
fn a_long_series_and_a_wide_parallel_compile_and_run() {
    // 300 contacts in series and 300 in parallel: deeper than a source may nest
    // (MAX_NESTING) if built as a chain, so built balanced.
    let mut p = Pou::program("Main").vars(&[("q", "BOOL"), ("o", "BOOL")]);
    let mut vars = Vec::new();
    for k in 0..300 {
        vars.push((format!("s{k}"), "BOOL"));
    }
    let names: Vec<(&str, &str)> = vars.iter().map(|(n, t)| (n.as_str(), *t)).collect();
    p = p.vars(&names).left(1, 0);
    let mut prev = "1".to_string();
    for k in 0..300u64 {
        p = p.contact(
            10 + k,
            10 * k as i32,
            0,
            Contact::No,
            &format!("s{k}"),
            &[&prev],
        );
        prev = (10 + k).to_string();
    }
    p = p.coil(5, 4000, 0, Coil::Plain, "q", &[&prev]);
    let mut ors = Vec::new();
    for k in 0..300u64 {
        p = p.contact(
            1000 + k,
            10,
            100 + k as i32,
            Contact::No,
            &format!("s{k}"),
            &["1"],
        );
        ors.push((1000 + k).to_string());
    }
    let refs: Vec<&str> = ors.iter().map(String::as_str).collect();
    p = p.coil(6, 4000, 100, Coil::Plain, "o", &refs);
    let mut r = ld(&[p]);
    r.scan().unwrap();
    assert!(!r.bool("q") && !r.bool("o"));
    for k in 0..300 {
        set(&mut r, &format!("s{k}"), true);
    }
    r.scan().unwrap();
    assert!(r.bool("q") && r.bool("o"));
    set(&mut r, "s150", false);
    r.scan().unwrap();
    assert!(!r.bool("q") && r.bool("o"));
}

#[test]
fn a_long_series_of_functions_compiles_and_runs() {
    // 300 AND blocks, each taking the one before: each result is kept for monitoring (§2.15),
    // and is worked out one block at a time, so the series takes no stack for its length.
    let mut p = Pou::program("Main")
        .vars(&[("x", "BOOL"), ("y", "BOOL"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 0, 0, Contact::No, "x", &["1"]);
    let mut prev = "2".to_string();
    for k in 0..300u64 {
        p = p.block(
            10 + k,
            20 + 10 * k as i32,
            0,
            "AND",
            None,
            &[("IN1", In::From(&[&prev])), ("IN2", In::Expr("y"))],
            &["OUT"],
        );
        prev = (10 + k).to_string();
    }
    let mut r = ld(&[p
        .coil(5, 9999, 0, Coil::Plain, "q", &[&prev])
        .right(3, 0, &["5"])]);
    for (x, y) in [(true, true), (true, false), (false, true), (true, true)] {
        set(&mut r, "x", x);
        set(&mut r, "y", y);
        r.scan().unwrap();
        assert_eq!(r.bool("q"), x && y, "x={x} y={y}");
        assert_eq!(r.bool("__LD_309"), x && y, "the last block's result, kept");
    }
}

#[test]
fn a_normally_closed_contact_passes_power_when_its_variable_is_off() {
    let mut r = ld(&[one_contact(Contact::Nc)]);
    for (en, x) in [(true, false), (true, true), (false, false), (false, true)] {
        set(&mut r, "en", en);
        set(&mut r, "x", x);
        r.scan().unwrap();
        assert_eq!(r.bool("q"), en && !x, "en={en} x={x}");
    }
}

#[test]
fn a_rising_contact_is_on_for_one_scan_after_its_variable_goes_on() {
    let mut r = ld(&[one_contact(Contact::Rising)]);
    set(&mut r, "en", true);
    let mut seen = Vec::new();
    for x in [false, true, true, false, true, false] {
        set(&mut r, "x", x);
        r.scan().unwrap();
        seen.push(r.bool("q"));
    }
    assert_eq!(seen, [false, true, false, false, true, false]);
}

#[test]
fn a_rising_contact_senses_its_variable_while_its_input_is_off() {
    // Table 44: M := CLK at every evaluation. The variable goes on while the left link is
    // OFF; when the link comes on the variable has not changed since: no edge.
    let mut r = ld(&[one_contact(Contact::Rising)]);
    set(&mut r, "en", false);
    set(&mut r, "x", true);
    r.scan().unwrap();
    assert!(!r.bool("q"));
    set(&mut r, "en", true);
    r.scan().unwrap();
    assert!(
        !r.bool("q"),
        "the transition was sensed while the link was OFF"
    );
}

#[test]
fn a_rising_contact_is_on_at_the_first_scan_when_its_variable_is_on_as_r_trig() {
    // Table 44 note: R_TRIG's Q is TRUE after its first execution with CLK TRUE.
    let mut r = ld(&[one_contact(Contact::Rising)]);
    set(&mut r, "en", true);
    set(&mut r, "x", true);
    r.scan().unwrap();
    assert!(r.bool("q"));
}

#[test]
fn a_falling_contact_is_on_for_one_scan_after_its_variable_goes_off() {
    let mut r = ld(&[one_contact(Contact::Falling)]);
    set(&mut r, "en", true);
    let mut seen = Vec::new();
    // As F_TRIG (Table 44 and its note): with the variable OFF at the first evaluation
    // after a cold start, the contact is ON for that evaluation.
    for x in [false, false, true, true, false, false, true, false] {
        set(&mut r, "x", x);
        r.scan().unwrap();
        seen.push(r.bool("q"));
    }
    assert_eq!(seen, [true, false, false, false, true, false, false, true]);
}

#[test]
fn the_memory_of_an_edge_is_a_private_hidden_variable_named_for_its_element() {
    let r = ld(&[one_contact(Contact::Rising)]);
    let m = r.c.symbol("__EDGE_3").expect("the edge memory is a symbol");
    assert_eq!(m.ty, "BOOL");
    assert!(m.private, "only the program writes it");
    assert!(!m.writable);
    assert!(
        !m.retain,
        "an unqualified variable of a program is not retained"
    );
    // Nothing else is hidden in this program but what keeps power for monitoring (§2.15):
    // the rung's flag and the output of each contact and of the coil.
    let hidden: Vec<&str> =
        r.c.symbols
            .iter()
            .filter(|s| s.path.starts_with("__"))
            .map(|s| s.path.as_str())
            .collect();
    assert_eq!(
        hidden,
        ["__NET_2", "__LD_2", "__EDGE_3", "__LD_3", "__LD_4"]
    );
}

fn one_coil(kind: Coil) -> Pou {
    Pou::program("Main")
        .vars(&[("x", "BOOL"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "x", &["1"])
        .coil(3, 300, 20, kind, "q", &["2"])
        .right(5, 0, &["3"])
}

fn run_coil(kind: Coil, xs: &[bool], q0: bool) -> Vec<bool> {
    let mut r = ld(&[one_coil(kind)]);
    set(&mut r, "q", q0);
    xs.iter()
        .map(|x| {
            set(&mut r, "x", *x);
            r.scan().unwrap();
            r.bool("q")
        })
        .collect()
}

#[test]
fn each_coil_writes_its_variable_as_table_76_says() {
    let xs = [false, true, true, false, true, false];
    assert_eq!(run_coil(Coil::Plain, &xs, false), xs);
    assert_eq!(
        run_coil(Coil::Negated, &xs, false),
        xs.map(|x| !x),
        "the inverse of the left link"
    );
    assert_eq!(
        run_coil(Coil::Set, &xs, false),
        [false, true, true, true, true, true],
        "set while the link is on, and stays set"
    );
    assert_eq!(
        run_coil(Coil::Reset, &xs, true),
        [true, false, false, false, false, false],
        "reset while the link is on, and stays reset"
    );
    assert_eq!(
        run_coil(Coil::Rising, &xs, false),
        [false, true, false, false, true, false],
        "on for one evaluation after the link goes on"
    );
    assert_eq!(
        run_coil(
            Coil::Falling,
            &[true, true, false, false, true, false],
            false
        ),
        [false, false, true, false, false, true],
        "on for one evaluation after the link goes off"
    );
}

#[test]
fn a_coil_passes_its_input_on_as_in_the_example_of_8_2_5() {
    // |   a    b       c    d    |
    // +--( )--| |--+--( )---( )--+
    // |            |      e      |
    // |            +-----( )-----+
    // a is always TRUE; c, d and e equal b.
    let p = Pou::program("Main")
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("d", "BOOL"),
            ("e", "BOOL"),
        ])
        .left(1, 0)
        .coil(2, 50, 20, Coil::Plain, "a", &["1"])
        .contact(3, 100, 20, Contact::No, "b", &["2"])
        .coil(4, 200, 20, Coil::Plain, "c", &["3"])
        .coil(5, 300, 20, Coil::Plain, "d", &["4"])
        .coil(6, 250, 60, Coil::Plain, "e", &["3"])
        .right(7, 0, &["5", "6"]);
    let mut r = ld(&[p]);
    for b in [false, true, false] {
        set(&mut r, "b", b);
        r.scan().unwrap();
        assert!(r.bool("a"));
        assert_eq!((r.bool("c"), r.bool("d"), r.bool("e")), (b, b, b));
    }
}

#[test]
fn power_through_a_coil_is_the_power_before_it_was_written() {
    // |  a     a      d  |
    // +--| |--(R)----( )-+   a is reset by the rung, and d still gets the power that
    //                        reached the coil, sensed before the reset (one evaluation).
    let p = Pou::program("Main")
        .vars(&[("a", "BOOL"), ("d", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        .coil(3, 200, 20, Coil::Reset, "a", &["2"])
        .coil(4, 300, 20, Coil::Plain, "d", &["3"])
        .right(5, 0, &["4"]);
    let mut r = ld(&[p]);
    set(&mut r, "a", true);
    r.scan().unwrap();
    assert!(!r.bool("a"));
    assert!(r.bool("d"), "the power at the coil was ON");
    r.scan().unwrap();
    assert!(!r.bool("d"));
}

#[test]
fn a_branch_point_is_evaluated_once_for_every_branch() {
    // a feeds a reset of a and a coil d on two branches: both see the same power.
    let p = Pou::program("Main")
        .vars(&[("a", "BOOL"), ("d", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        .coil(3, 200, 20, Coil::Reset, "a", &["2"])
        .coil(4, 200, 60, Coil::Plain, "d", &["2"])
        .right(5, 0, &["3", "4"]);
    let mut r = ld(&[p]);
    set(&mut r, "a", true);
    r.scan().unwrap();
    assert!(!r.bool("a") && r.bool("d"));
}

#[test]
fn networks_run_top_to_bottom_whatever_the_order_of_the_document() {
    // The rung below copies b to c; the rung above copies a to b. Written bottom first.
    let p = Pou::program("Main")
        .vars(&[("a", "BOOL"), ("b", "BOOL"), ("c", "BOOL")])
        .left(1, 0)
        .contact(20, 100, 200, Contact::No, "b", &["1"])
        .coil(21, 200, 200, Coil::Plain, "c", &["20"])
        .contact(10, 100, 100, Contact::No, "a", &["1"])
        .coil(11, 200, 100, Coil::Plain, "b", &["10"])
        .right(2, 0, &["11", "21"]);
    let mut r = ld(&[p]);
    set(&mut r, "a", true);
    r.scan().unwrap();
    assert!(r.bool("b") && r.bool("c"), "b was written before c read it");
}

#[test]
fn a_ton_and_a_ctu_run_as_blocks_with_en_and_eno() {
    // |  start     t (TON)          q   |
    // +---| |----+-|IN    Q|-------( )--+
    //            T#50ms-|PT  ET|- et
    // |  pulse     c (CTU)     done  |
    // +---|P|-----EN|CU   Q|---( )---+    (EN from the left rail, CU from the pulse)
    //      reset -|R    CV|- cv
    //          3 -|PV      |
    let p = Pou::program("Main")
        .vars(&[
            ("start", "BOOL"),
            ("q", "BOOL"),
            ("et", "TIME"),
            ("pulse", "BOOL"),
            ("reset", "BOOL"),
            ("done", "BOOL"),
            ("cv", "INT"),
            ("enabled", "BOOL"),
            ("t", "TON"),
            ("c", "CTU"),
        ])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "start", &["1"])
        .block(
            3,
            200,
            20,
            "TON",
            Some("t"),
            &[("IN", In::From(&["2"])), ("PT", In::Expr("T#50ms"))],
            &["Q", "ET"],
        )
        .coil(4, 400, 20, Coil::Plain, "q", &["3.Q"])
        .outvar(5, 400, 60, "et", &["3.ET"])
        .contact(6, 100, 120, Contact::No, "pulse", &["1"])
        .invar(7, 100, 160, "reset")
        .invar(8, 100, 180, "3")
        .contact(9, 50, 120, Contact::No, "enabled", &["1"])
        .block(
            10,
            200,
            120,
            "CTU",
            Some("c"),
            &[
                ("EN", In::From(&["9"])),
                ("CU", In::From(&["6"])),
                ("R", In::From(&["7"])),
                ("PV", In::From(&["8"])),
            ],
            &["ENO", "Q", "CV"],
        )
        .coil(11, 400, 120, Coil::Plain, "done", &["10.Q"])
        .outvar(12, 400, 160, "cv", &["10.CV"])
        .right(13, 0, &["4", "11"]);
    let mut r = ld(&[p]);
    set(&mut r, "start", true);
    r.scan_at(0);
    r.scan_at(30);
    assert!(!r.bool("q"));
    assert_eq!(r.int("et"), 30_000_000);
    r.scan_at(60);
    assert!(r.bool("q"));
    assert_eq!(r.int("et"), 50_000_000);

    // CTU counts only while EN is on (the block is not called otherwise, §6.6.1.5).
    for (enabled, pulse) in [
        (false, true),
        (false, false),
        (true, true),
        (true, false),
        (true, true),
        (true, false),
        (true, true),
    ] {
        set(&mut r, "enabled", enabled);
        set(&mut r, "pulse", pulse);
        r.scan_at(100);
    }
    assert_eq!(r.int("cv"), 3);
    assert!(r.bool("done"));
    set(&mut r, "reset", true);
    r.scan_at(100);
    assert_eq!(r.int("cv"), 0);
}

#[test]
fn a_block_not_enabled_keeps_its_outputs_and_eno_is_off() {
    // |  en      c (CTU)  ENO    ok  |
    // +--| |----EN    ENO|------( )--+
    //   1 -|PV  CU|<- the left rail;  CV -> cv
    let p = Pou::program("Main")
        .vars(&[("en", "BOOL"), ("ok", "BOOL"), ("cv", "INT"), ("c", "CTU")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "en", &["1"])
        .block(
            3,
            200,
            20,
            "CTU",
            Some("c"),
            &[
                ("EN", In::From(&["2"])),
                ("CU", In::From(&["1"])),
                ("PV", In::Expr("1")),
            ],
            &["ENO", "CV"],
        )
        .coil(4, 400, 20, Coil::Plain, "ok", &["3.ENO"])
        .outvar(5, 400, 60, "cv", &["3.CV"])
        .right(6, 0, &["4"]);
    let mut r = ld(&[p]);
    set(&mut r, "en", true);
    r.scan().unwrap();
    assert!(r.bool("ok"));
    assert_eq!(
        r.int("cv"),
        1,
        "CU is ON from the first call: an edge (Table 45)"
    );
    set(&mut r, "en", false);
    r.scan().unwrap();
    assert!(!r.bool("ok"), "ENO is OFF when EN is");
    assert_eq!(r.int("cv"), 1, "not called: its output keeps its value");
}

#[test]
fn an_st_function_and_standard_functions_are_called_from_ld() {
    let st = "FUNCTION Scale : INT
        VAR_INPUT raw : INT; gain : INT; END_VAR
        Scale := raw * gain;
    END_FUNCTION";
    // out := Scale(raw := ADD(a, 1), gain := 3); big := GT(out, 10) as a coil.
    let p = Pou::program("Main")
        .vars(&[("a", "INT"), ("out", "INT"), ("big", "BOOL")])
        .left(1, 0)
        .invar(2, 0, 20, "a")
        .invar(3, 0, 40, "1")
        .block(
            4,
            100,
            20,
            "ADD",
            None,
            &[("IN1", In::From(&["2"])), ("IN2", In::From(&["3"]))],
            &["OUT"],
        )
        .block(
            5,
            200,
            20,
            "Scale",
            None,
            &[("raw", In::From(&["4"])), ("gain", In::Expr("3"))],
            &["OUT"],
        )
        .outvar(6, 300, 20, "out", &["5"])
        .invar(7, 300, 80, "out")
        .block(
            8,
            400,
            80,
            "GT",
            None,
            &[("IN1", In::From(&["7"])), ("IN2", In::Expr("10"))],
            &["OUT"],
        )
        .coil(9, 500, 80, Coil::Plain, "big", &["8"])
        .right(10, 0, &["9"]);
    let mut r = with_st(&[p], st);
    for (a, out) in [(0, 3), (2, 9), (3, 12), (-5, -12)] {
        r.set("a", Value::Int(a));
        r.scan().unwrap();
        assert_eq!(r.int("out"), out);
        assert_eq!(r.bool("big"), out > 10);
    }
}

#[test]
fn a_function_with_en_writes_nothing_when_it_does_not_run() {
    // | en      ADD          |
    // +--| |--EN   ENO|--( ok )
    //     a -|IN1  OUT|-> sum
    //     1 -|IN2     |
    let p = Pou::program("Main")
        .vars(&[("en", "BOOL"), ("a", "INT"), ("sum", "INT"), ("ok", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "en", &["1"])
        .invar(3, 100, 60, "a")
        .block(
            4,
            200,
            20,
            "ADD",
            None,
            &[
                ("EN", In::From(&["2"])),
                ("IN1", In::From(&["3"])),
                ("IN2", In::Expr("1")),
            ],
            &["ENO", "OUT"],
        )
        .coil(5, 300, 20, Coil::Plain, "ok", &["4.ENO"])
        .outvar(6, 300, 60, "sum", &["4.OUT"])
        .right(7, 0, &["5"]);
    let mut r = ld(&[p]);
    set(&mut r, "en", true);
    r.set("a", Value::Int(4));
    r.scan().unwrap();
    assert_eq!(r.int("sum"), 5);
    assert!(r.bool("ok"));
    set(&mut r, "en", false);
    r.set("a", Value::Int(40));
    r.scan().unwrap();
    assert_eq!(r.int("sum"), 5, "not run: sum keeps its value");
    assert!(!r.bool("ok"));
}

#[test]
fn and_or_xor_not_and_mod_blocks_are_the_operators() {
    let p = Pou::program("Main")
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("all", "BOOL"),
            ("any", "BOOL"),
            ("odd", "BOOL"),
            ("none", "BOOL"),
            ("n", "INT"),
            ("m", "INT"),
        ])
        .invar(1, 0, 0, "a")
        .invar(2, 0, 20, "b")
        .invar(3, 0, 40, "c")
        .block(
            4,
            100,
            0,
            "AND",
            None,
            &[
                ("IN1", In::From(&["1"])),
                ("IN2", In::From(&["2"])),
                ("IN3", In::From(&["3"])),
            ],
            &["OUT"],
        )
        .outvar(5, 200, 0, "all", &["4"])
        .block(
            6,
            100,
            100,
            "OR",
            None,
            &[
                ("IN1", In::Expr("a")),
                ("IN2", In::Expr("b")),
                ("IN3", In::Expr("c")),
            ],
            &["OUT"],
        )
        .outvar(7, 200, 100, "any", &["6"])
        .block(
            8,
            100,
            200,
            "XOR",
            None,
            &[("IN1", In::Expr("a")), ("IN2", In::Expr("b"))],
            &["OUT"],
        )
        .outvar(9, 200, 200, "odd", &["8"])
        .block(
            10,
            100,
            300,
            "NOT",
            None,
            &[("IN", In::Expr("a OR b"))],
            &["OUT"],
        )
        .outvar(11, 200, 300, "none", &["10"])
        .block(
            12,
            100,
            400,
            "MOD",
            None,
            &[("IN1", In::Expr("n")), ("IN2", In::Expr("3"))],
            &["OUT"],
        )
        .outvar(13, 200, 400, "m", &["12"]);
    let mut r = ld(&[p]);
    for bits in 0..8u8 {
        let (a, b, c) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
        set(&mut r, "a", a);
        set(&mut r, "b", b);
        set(&mut r, "c", c);
        r.set("n", Value::Int(bits as i16 + 2));
        r.scan().unwrap();
        assert_eq!(r.bool("all"), a && b && c);
        assert_eq!(r.bool("any"), a || b || c);
        assert_eq!(r.bool("odd"), a ^ b);
        assert_eq!(r.bool("none"), !(a || b));
        assert_eq!(r.int("m"), (bits as i64 + 2) % 3);
    }
}

#[test]
fn a_function_result_feeding_two_inputs_is_computed_once() {
    // GT's result feeds two coils; a comparison is a BOOL, so it is kept.
    let p = Pou::program("Main")
        .vars(&[("n", "INT"), ("x", "BOOL"), ("y", "BOOL")])
        .left(1, 0)
        .block(
            2,
            100,
            20,
            "GT",
            None,
            &[("IN1", In::Expr("n")), ("IN2", In::Expr("0"))],
            &["OUT"],
        )
        .coil(3, 200, 20, Coil::Plain, "x", &["2"])
        .coil(4, 200, 60, Coil::Plain, "y", &["2"])
        .right(5, 0, &["3", "4"]);
    let mut r = ld(&[p]);
    r.set("n", Value::Int(4));
    r.scan().unwrap();
    assert!(r.bool("x") && r.bool("y"));
    assert!(r.c.symbol("__LD_2").is_some(), "kept in a hidden variable");
}

#[test]
fn a_forward_jump_skips_the_networks_up_to_its_label() {
    // | skip            |
    // +--| |----->>OVER |
    // | a        q      |
    // +--| |----( )-----+   (skipped while skip is on: q keeps its value)
    // OVER:
    // | a        r      |
    // +--| |----( )-----+
    let p = Pou::program("Main")
        .vars(&[
            ("skip", "BOOL"),
            ("a", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
        ])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "skip", &["1"])
        .jump(3, 200, 20, "OVER", &["2"])
        .contact(4, 100, 100, Contact::No, "a", &["1"])
        .coil(5, 200, 100, Coil::Plain, "q", &["4"])
        .label(6, 150, "OVER")
        .contact(7, 100, 200, Contact::No, "a", &["1"])
        .coil(8, 200, 200, Coil::Plain, "r", &["7"])
        .right(9, 0, &["5", "8"]);
    let mut r = ld(&[p]);
    set(&mut r, "a", true);
    r.scan().unwrap();
    assert!(r.bool("q") && r.bool("r"));
    set(&mut r, "skip", true);
    set(&mut r, "a", false);
    r.scan().unwrap();
    assert!(r.bool("q"), "skipped: q keeps its value");
    assert!(!r.bool("r"), "after the label: evaluated");
    let j = r.c.symbol("__JUMP").expect("the jump variable is a symbol");
    assert!(j.private);
}

#[test]
fn a_backward_jump_repeats_the_networks_from_its_label() {
    // AGAIN:
    // i := ADD(i, 1)
    // LT(i, n) ---->>AGAIN
    let p = Pou::program("Main")
        .vars(&[("i", "INT"), ("n", "INT")])
        .label(1, 0, "AGAIN")
        .invar(2, 0, 20, "i")
        .block(
            3,
            100,
            20,
            "ADD",
            None,
            &[("IN1", In::From(&["2"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(4, 200, 20, "i", &["3"])
        .block(
            5,
            100,
            100,
            "LT",
            None,
            &[("IN1", In::Expr("i")), ("IN2", In::Expr("n"))],
            &["OUT"],
        )
        .jump(6, 200, 100, "AGAIN", &["5"]);
    let mut r = ld(&[p]);
    r.set("n", Value::Int(10));
    r.scan().unwrap();
    assert_eq!(r.int("i"), 10);
    r.scan().unwrap();
    assert_eq!(r.int("i"), 11, "at least once, as REPEAT");
}

#[test]
fn a_return_ends_the_scan_after_its_network() {
    // | stop      | x  ( q )         the coil in the network of the return is still written
    // +--| |--<RETURN>               (§8.1.5.1 rule c), the network below is not.
    // | x     r   |
    let p = Pou::program("Main")
        .vars(&[
            ("stop", "BOOL"),
            ("x", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
        ])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "stop", &["1"])
        .ret(3, 200, 20, &["2"])
        .contact(4, 100, 40, Contact::No, "x", &["2"])
        .coil(5, 200, 40, Coil::Plain, "q", &["4"])
        .contact(6, 100, 200, Contact::No, "x", &["1"])
        .coil(7, 200, 200, Coil::Plain, "r", &["6"])
        .right(8, 0, &["5", "7"]);
    let mut r = ld(&[p]);
    set(&mut r, "x", true);
    set(&mut r, "stop", true);
    r.scan().unwrap();
    assert!(r.bool("q"), "its own network is evaluated whole");
    assert!(!r.bool("r"), "the networks after it are not");
    set(&mut r, "stop", false);
    r.scan().unwrap();
    assert!(r.bool("r"));
}

#[test]
fn an_ld_function_block_and_an_ld_function_are_called_from_an_ld_program() {
    // A latch written in LD: q := (set OR q) AND NOT reset; and a function And2.
    let latch = Pou::function_block("Latch")
        .inputs(&[("set", "BOOL"), ("reset", "BOOL")])
        .outputs(&[("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "set", &["1"])
        .contact(3, 100, 60, Contact::No, "q", &["1"])
        .contact(4, 200, 20, Contact::Nc, "reset", &["2", "3"])
        .coil(5, 300, 20, Coil::Plain, "q", &["4"])
        .right(6, 0, &["5"]);
    let and2 = Pou::function("And2", "BOOL")
        .inputs(&[("a", "BOOL"), ("b", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        .contact(3, 200, 20, Contact::No, "b", &["2"])
        .coil(4, 300, 20, Coil::Plain, "And2", &["3"])
        .right(5, 0, &["4"]);
    let main = Pou::program("Main")
        .vars(&[
            ("s", "BOOL"),
            ("rs", "BOOL"),
            ("out", "BOOL"),
            ("both", "BOOL"),
            ("l", "Latch"),
        ])
        .block(
            1,
            100,
            20,
            "Latch",
            Some("l"),
            &[("set", In::Expr("s")), ("reset", In::Expr("rs"))],
            &["q"],
        )
        .outvar(2, 200, 20, "out", &["1.q"])
        .block(
            3,
            100,
            100,
            "And2",
            None,
            &[("a", In::Expr("s")), ("b", In::Expr("out"))],
            &["OUT"],
        )
        .outvar(4, 200, 100, "both", &["3"]);
    let mut r = ld(&[latch, and2, main]);
    set(&mut r, "s", true);
    r.scan().unwrap();
    assert!(r.bool("out") && r.bool("both"));
    set(&mut r, "s", false);
    r.scan().unwrap();
    assert!(r.bool("out") && !r.bool("both"), "latched");
    set(&mut r, "rs", true);
    r.scan().unwrap();
    assert!(!r.bool("out"));
}

#[test]
fn retain_in_the_interface_and_the_edge_memory_of_a_retained_instance_are_retained() {
    let block = Pou::function_block("Pulse")
        .inputs(&[("x", "BOOL")])
        .outputs(&[("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::Rising, "x", &["1"])
        .coil(3, 200, 20, Coil::Plain, "q", &["2"])
        .right(4, 0, &["3"]);
    let main = Pou::program("Main")
        .section(
            "localVars retain=\"true\"",
            &[("count", "INT", Some("5")), ("p", "Pulse", None)],
        )
        .section("localVars nonretain=\"true\"", &[("other", "INT", None)])
        .vars(&[("plain", "BOOL")])
        .left(1, 0)
        .block(
            2,
            100,
            20,
            "Pulse",
            Some("p"),
            &[("x", In::From(&["1"]))],
            &["q"],
        )
        .coil(3, 200, 20, Coil::Plain, "plain", &["2.q"])
        .right(4, 0, &["3"]);
    let r = ld(&[block, main]);
    let retained = |path: &str| r.c.symbol(path).unwrap_or_else(|| panic!("{path}")).retain;
    assert!(retained("count"));
    assert_eq!(r.int("count"), 5, "the initial value from simpleValue");
    assert!(
        retained("p.__EDGE_2"),
        "the edge memory takes the instance's RETAIN (§6.5.6.2)"
    );
    assert!(r.c.symbol("p.__EDGE_2").unwrap().private);
    assert!(!retained("other"));
    assert!(!retained("plain"));
}

#[test]
fn comments_are_not_part_of_any_network() {
    let p = one_contact(Contact::No).comment(99, 10, 10, "the pump starts when x is on");
    let mut r = ld(&[p]);
    set(&mut r, "en", true);
    set(&mut r, "x", true);
    r.scan().unwrap();
    assert!(r.bool("q"));
}

#[test]
fn the_oscillator_the_emulators_run_does_what_its_rungs_say() {
    let xml = include_str!("ladder/oscillator.xml");
    let mut r = Run::of(compile_in(xml, Language::Ld, &TargetProfile::rp2040()));
    let kept =
        r.c.symbols
            .iter()
            .filter(|s| s.path.starts_with("__LD_") || s.path.starts_with("__NET_"))
            .count();
    eprintln!(
        "the oscillator is {} bytes of code and {} of memory, {kept} variables keeping power",
        r.c.code_size, r.c.memory_size
    );
    let (mut rises, mut falls, mut last, mut latched_at) = (0, 0, false, None);
    let mut steps = Vec::new();
    for k in 0..60 {
        r.scan_at(k * 20);
        let blink = r.bool("blink");
        rises += (blink && !last) as i32;
        last = blink;
        falls += r.bool("fell") as i32;
        if r.bool("latched") && latched_at.is_none() {
            latched_at = Some(k);
        }
        steps.push(r.int("steps"));
        assert!(r.int("count") <= 5);
    }
    assert!(rises >= 5, "blink oscillates: {rises} rising edges");
    assert!(falls >= 4, "fell is on once for each fall: {falls}");
    let at = latched_at.expect("latched after five rising edges") as usize;
    assert_eq!(
        steps[at - 1],
        100 + at as i64,
        "steps counted every scan before"
    );
    assert!(
        steps[at..].iter().all(|s| *s == steps[at]),
        "and none after: the jump skips it"
    );
}
