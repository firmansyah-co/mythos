//! Function Block Diagram programs, compiled from PLCopen TC6 XML and run on the VM
//! (ADR-013 §2.16): each element, the order of evaluation (data flow within a network, networks
//! top to bottom), negation at a pin and on a variable, EN and ENO, lines continued by a
//! connector, feedback paths, jumps and returns, and FBD POUs called from FBD and from LD.

mod support;
#[path = "../../mythos-logic-graphic/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{compile_in, compile_sources, Holds, Language, Sources, TargetProfile};
use mythos_logic_vm::Value;
use support::Run;
use tc6::{project, In, Pou};

fn run(pous: &[Pou]) -> Run {
    let doc = project(pous);
    Run::of(compile_in(&doc, Language::Fbd, &TargetProfile::pc()))
}

fn b(v: bool) -> Value {
    Value::Bool(v)
}

#[test]
fn a_block_is_evaluated_after_what_it_reads_wherever_it_is_drawn() {
    // MUL is drawn above ADD and reads it; SUB above both. Each runs after its inputs.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[("x", "INT"), ("y", "INT"), ("s", "INT")])
        .invar(1, 0, 200, "x")
        .invar(2, 0, 240, "y")
        .block(
            3,
            120,
            220,
            "ADD",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::From(&["2"]))],
            &["OUT"],
        )
        .block(
            4,
            240,
            100,
            "MUL",
            None,
            &[("IN1", In::From(&["3"])), ("IN2", In::Expr("3"))],
            &["OUT"],
        )
        .block(
            5,
            360,
            0,
            "SUB",
            None,
            &[("IN1", In::From(&["4"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(6, 480, 0, "s", &["5"]);
    let mut r = run(&[main]);
    for (x, y) in [(1, 2), (-5, 3), (100, -1)] {
        r.set("x", Value::Int(x));
        r.set("y", Value::Int(y));
        r.scan().unwrap();
        assert_eq!(r.get("s"), Value::Int((x + y) * 3 - 1), "x {x}, y {y}");
        // Each output kept in its own type, as it was computed.
        assert_eq!(r.get("__FBD_3"), Value::Int(x + y));
        assert_eq!(r.get("__FBD_4"), Value::Int((x + y) * 3));
    }
    assert_eq!(r.c.symbol("__FBD_3").unwrap().ty, "INT");
}

#[test]
fn a_negated_input_and_a_negated_output_are_negated_outside_the_block() {
    // §6.6.1.4.3 rule 9: q := NOT (a AND NOT b); v := NOT c; w is written NOT of its line.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("q", "BOOL"),
            ("v", "BOOL"),
            ("w", "BOOL"),
        ])
        .invar(1, 0, 0, "a")
        .invar(2, 0, 40, "b")
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
            &[("OUT", true)],
        )
        .outvar(4, 240, 0, "q", &["3"])
        .invar_neg(5, 0, 100, "c")
        .outvar(6, 240, 100, "v", &["5"])
        .outvar_neg(7, 240, 140, "w", &["5"]);
    let mut r = run(&[main]);
    for (a, bb, c) in [
        (false, false, false),
        (true, false, true),
        (true, true, false),
        (false, true, true),
    ] {
        r.set("a", b(a));
        r.set("b", b(bb));
        r.set("c", b(c));
        r.scan().unwrap();
        assert_eq!(r.get("q"), b(!(a && !bb)), "a {a} b {bb}");
        // The line carries the negated output, and that is what is kept.
        assert_eq!(r.get("__FBD_3"), b(!(a && !bb)));
        assert_eq!(r.get("v"), b(!c));
        assert_eq!(r.get("w"), b(c));
    }
}

#[test]
fn a_function_with_en_writes_nothing_downstream_where_it_does_not_run() {
    // ADD under EN feeds MUL, which feeds s: where EN is FALSE neither runs and s keeps its
    // value; ENO of each says so, and t, fed by ENO, follows EN.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[("en", "BOOL"), ("x", "INT"), ("s", "INT"), ("t", "BOOL")])
        .invar(1, 0, 0, "en")
        .invar(2, 0, 40, "x")
        .block(
            3,
            120,
            0,
            "ADD",
            None,
            &[
                ("EN", In::From(&["1"])),
                ("IN1", In::From(&["2"])),
                ("IN2", In::Expr("10")),
            ],
            &["ENO", "OUT"],
        )
        .block(
            4,
            240,
            0,
            "MUL",
            None,
            &[("IN1", In::From(&["3.OUT"])), ("IN2", In::Expr("2"))],
            &["OUT"],
        )
        .outvar(5, 360, 0, "s", &["4"])
        .outvar(6, 360, 60, "t", &["3.ENO"]);
    let mut r = run(&[main]);
    r.set("en", b(true));
    r.set("x", Value::Int(1));
    r.scan().unwrap();
    assert_eq!(r.get("s"), Value::Int(22));
    assert_eq!(r.get("t"), b(true));
    r.set("en", b(false));
    r.set("x", Value::Int(5));
    r.scan().unwrap();
    assert_eq!(
        r.get("s"),
        Value::Int(22),
        "nothing downstream of a function not run"
    );
    assert_eq!(r.get("t"), b(false));
    assert_eq!(r.get("__FBD_3_ENO"), b(false));
    assert_eq!(r.get("__FBD_4_ENO"), b(false), "MUL did not run either");
    // What is kept is the value of the last scan it ran in, and the probe says so.
    assert_eq!(r.get("__FBD_3"), Value::Int(11));
    let probe =
        r.c.probes
            .iter()
            .find(|p| p.local_id == 4 && p.at == mythos_logic_compiler::At::Output("OUT".into()))
            .unwrap();
    assert_eq!(
        probe.holds,
        Holds::Guarded {
            path: "__FBD_4".into(),
            ran: "__FBD_4_ENO".into()
        }
    );
}

#[test]
fn a_function_block_is_called_with_its_inputs_and_its_outputs_kept_after_the_call() {
    // CTU counts the rising edges of a under EN; its CV and Q are kept, and ENO follows EN.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("a", "BOOL"),
            ("en", "BOOL"),
            ("c", "CTU"),
            ("cv", "INT"),
            ("q", "BOOL"),
        ])
        .invar(1, 0, 0, "en")
        .invar(2, 0, 40, "a")
        .block(
            3,
            120,
            0,
            "CTU",
            Some("c"),
            &[
                ("EN", In::From(&["1"])),
                ("CU", In::From(&["2"])),
                ("R", In::Expr("FALSE")),
                ("PV", In::Expr("2")),
            ],
            &["ENO", "Q", "CV"],
        )
        .outvar(4, 240, 0, "q", &["3.Q"])
        .outvar(5, 240, 40, "cv", &["3.CV"]);
    let mut r = run(&[main]);
    let mut count = 0;
    let mut prev = false;
    for (scan, (en, a)) in [
        (true, true),
        (true, false),
        (false, true),
        (false, false),
        (true, true),
        (true, false),
        (true, true),
    ]
    .into_iter()
    .enumerate()
    {
        r.set("en", b(en));
        r.set("a", b(a));
        r.scan().unwrap();
        if en {
            if a && !prev {
                count += 1;
            }
            prev = a;
        }
        assert_eq!(r.get("cv"), Value::Int(count), "scan {scan}");
        assert_eq!(r.get("__FBD_3_CV"), Value::Int(count));
        assert_eq!(r.get("q"), b(count >= 2));
        assert_eq!(r.get("__FBD_3_ENO"), b(en));
    }
}

#[test]
fn a_connector_and_its_continuations_are_one_line() {
    // a AND b goes to the connector L, and on at two continuations: q and NOT r.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[("a", "BOOL"), ("b", "BOOL"), ("q", "BOOL"), ("r", "BOOL")])
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
        .continuation(5, 0, 200, "L")
        .outvar(6, 120, 200, "q", &["5"])
        .continuation(7, 0, 260, "L")
        .outvar_neg(8, 120, 260, "r", &["7"]);
    let mut r = run(&[main]);
    for (a, bb) in [(true, true), (true, false), (false, true)] {
        r.set("a", b(a));
        r.set("b", b(bb));
        r.scan().unwrap();
        assert_eq!(r.get("q"), b(a && bb));
        assert_eq!(r.get("r"), b(!(a && bb)));
    }
    // One network: the connector joins what it ends to what its continuations reach.
    let networks: Vec<_> =
        r.c.probes
            .iter()
            .filter(|p| matches!(p.at, mythos_logic_compiler::At::Network(_)))
            .collect();
    assert_eq!(networks.len(), 1, "{networks:#?}");
    // A continuation holds what its line comes from.
    let held = |id: u64| {
        r.c.probes
            .iter()
            .find(|p| p.local_id == id && p.at == mythos_logic_compiler::At::Output(String::new()))
            .map(|p| p.holds.clone())
    };
    assert_eq!(
        held(5),
        Some(Holds::Variable {
            path: "__FBD_3".into(),
            negated: false
        })
    );
    assert_eq!(held(7), held(5));
}

#[test]
fn an_explicit_loop_drawn_back_reads_the_output_of_the_last_evaluation() {
    // §8.1.5.2 example a: RUN := ENABLE AND (START1 OR START2 OR RUN), the line from the AND's
    // output drawn back to the OR's third input. A seal-in.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("start1", "BOOL"),
            ("start2", "BOOL"),
            ("enable", "BOOL"),
            ("run", "BOOL"),
        ])
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
        .outvar(6, 360, 0, "run", &["5"]);
    let mut r = run(&[main]);
    let mut run_ = false;
    for (scan, (enable, s1, s2)) in [
        (true, false, false),
        (true, true, false),
        (true, false, false),
        (true, false, false),
        (false, false, false),
        (true, false, false),
        (true, false, true),
        (true, false, false),
    ]
    .into_iter()
    .enumerate()
    {
        r.set("enable", b(enable));
        r.set("start1", b(s1));
        r.set("start2", b(s2));
        r.scan().unwrap();
        run_ = enable && (s1 || s2 || run_);
        assert_eq!(r.get("run"), b(run_), "scan {scan}");
    }
}

#[test]
fn a_function_block_output_drawn_back_is_a_feedback_path() {
    // §8.1.5.2: "a feedback variable can also be an output element of a function block". CV of
    // the counter is drawn back to ADD, which sets its preset: PV is always CV + 2, read from
    // the last call, so Q never comes.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[("a", "BOOL"), ("c", "CTU"), ("q", "BOOL"), ("pv", "INT")])
        .invar(1, 0, 0, "a")
        .block(
            2,
            120,
            40,
            "ADD",
            None,
            &[("IN1", In::From(&["3.CV"])), ("IN2", In::Expr("2"))],
            &["OUT"],
        )
        .block(
            3,
            240,
            0,
            "CTU",
            Some("c"),
            &[
                ("CU", In::From(&["1"])),
                ("R", In::Expr("FALSE")),
                ("PV", In::From(&["2"])),
            ],
            &["Q", "CV"],
        )
        .outvar(4, 360, 0, "q", &["3.Q"])
        .outvar(5, 360, 40, "pv", &["2"]);
    let mut r = run(&[main]);
    for scan in 0..6 {
        r.set("a", b(scan % 2 == 0));
        r.scan().unwrap();
        let cv = match r.get("c.CV") {
            Value::Int(v) => v,
            v => panic!("{v:?}"),
        };
        assert_eq!(r.get("q"), b(false));
        // PV was computed from CV before this scan's count.
        let before = if scan % 2 == 0 { cv - 1 } else { cv };
        assert_eq!(r.get("pv"), Value::Int(before + 2), "scan {scan}");
    }
    assert_eq!(r.c.symbol("__FBD_3_CV").unwrap().ty, "INT");
}

#[test]
fn networks_run_top_to_bottom_whatever_the_order_of_the_document_or_its_execution_order_ids() {
    // Network 1 (top) copies x to m; network 2 (below) copies m to y. The document lists the
    // lower one first and numbers it first by executionOrderId, which is not read.
    let lower = "<inVariable localId=\"20\" executionOrderId=\"1\"><position x=\"0\" y=\"200\"/><connectionPointOut/><expression>m</expression></inVariable><outVariable localId=\"21\" executionOrderId=\"2\"><position x=\"200\" y=\"200\"/><connectionPointIn><connection refLocalId=\"20\"/></connectionPointIn><expression>y</expression></outVariable>";
    let upper = "<inVariable localId=\"10\" executionOrderId=\"3\"><position x=\"0\" y=\"0\"/><connectionPointOut/><expression>x</expression></inVariable><outVariable localId=\"11\" executionOrderId=\"4\"><position x=\"200\" y=\"0\"/><connectionPointIn><connection refLocalId=\"10\"/></connectionPointIn><expression>m</expression></outVariable>";
    let main = Pou::program("Main")
        .fbd()
        .vars(&[("x", "INT"), ("m", "INT"), ("y", "INT")])
        .raw(lower)
        .raw(upper);
    let mut r = run(&[main]);
    r.set("x", Value::Int(7));
    r.scan().unwrap();
    assert_eq!(r.get("y"), Value::Int(7), "the upper network ran first");
}

#[test]
fn jumps_returns_and_in_out_variables_take_effect_after_their_network() {
    // Network 1: n := n + 1 through an in-out variable read back as the jump's count; if
    // n < 3 the body jumps back to AGAIN (a loop of three per scan); network 3 returns when
    // stop, so network 4 (k := k + 1) is skipped.
    let main = Pou::program("Main")
        .fbd()
        .vars(&[("n", "INT"), ("k", "INT"), ("stop", "BOOL")])
        .label(1, -20, "AGAIN")
        .invar(2, 0, 0, "n")
        .block(
            3,
            120,
            0,
            "ADD",
            None,
            &[("IN1", In::From(&["2"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .inoutvar(4, 240, 0, "n", &["3"])
        .block(
            5,
            360,
            0,
            "LT",
            None,
            &[("IN1", In::From(&["4"])), ("IN2", In::Expr("3"))],
            &["OUT"],
        )
        .jump(6, 480, 0, "AGAIN", &["5"])
        .invar(7, 0, 100, "stop")
        .ret(8, 120, 100, &["7"])
        .invar(9, 0, 200, "k")
        .block(
            10,
            120,
            200,
            "ADD",
            None,
            &[("IN1", In::From(&["9"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(11, 240, 200, "k", &["10"]);
    let mut r = run(&[main]);
    r.scan().unwrap();
    assert_eq!(r.get("n"), Value::Int(3));
    assert_eq!(r.get("k"), Value::Int(1));
    r.set("n", Value::Int(0));
    r.set("stop", b(true));
    r.scan().unwrap();
    assert_eq!(r.get("n"), Value::Int(3));
    assert_eq!(
        r.get("k"),
        Value::Int(1),
        "the return skipped the last network"
    );
    assert_eq!(r.get("__NET_9"), b(false));
    assert_eq!(r.get("__NET_7"), b(true));
}

#[test]
fn fbd_pous_are_called_from_fbd_and_from_ld_and_ld_from_fbd() {
    // An FBD function (Twice) and an FBD function block (Latch, an SR of its own), called from
    // an FBD program, which also calls an LD function block (Pass) of the same document.
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
    let mut r = run(&[twice, latch, pass, main]);
    r.set("x", Value::Int(21));
    r.set("s", b(true));
    r.scan().unwrap();
    assert_eq!(r.get("y"), Value::Int(42));
    assert_eq!(r.get("q"), b(true));
    r.set("s", b(false));
    r.scan().unwrap();
    assert_eq!(r.get("q"), b(true), "latched");
    r.set("r", b(true));
    r.scan().unwrap();
    assert_eq!(r.get("q"), b(false));
}

#[test]
fn an_fbd_program_is_compiled_with_the_st_pous_of_its_project() {
    let st = "FUNCTION Half : INT VAR_INPUT v : INT; END_VAR Half := v / 2; END_FUNCTION";
    let main = Pou::program("Main")
        .fbd()
        .vars(&[("x", "INT"), ("y", "INT")])
        .invar(1, 0, 0, "x")
        .block(
            2,
            120,
            0,
            "Half",
            None,
            &[("v", In::From(&["1"]))],
            &["OUT"],
        )
        .outvar(3, 240, 0, "y", &["2"]);
    let doc = project(&[main]);
    let mut r = Run::of(compile_sources(
        &Sources {
            st: Some(("pous.st", st)),
            tc6: &[("main.xml", doc.as_str())],
            ..Default::default()
        },
        &TargetProfile::pc(),
    ));
    r.set("x", Value::Int(9));
    r.scan().unwrap();
    assert_eq!(r.get("y"), Value::Int(4));
}

#[test]
fn an_empty_fbd_body_compiles_to_a_program_that_does_nothing() {
    let main = Pou::program("Main").fbd().vars(&[("x", "INT")]);
    let mut r = run(&[main]);
    r.scan().unwrap();
    assert_eq!(r.get("x"), Value::Int(0));
    assert!(r.c.probes.is_empty());
}

#[test]
fn the_blinker_the_emulators_run_does_what_its_networks_say() {
    let xml = include_str!("fbd/blinker.xml");
    let mut r = Run::of(compile_in(xml, Language::Fbd, &TargetProfile::pc()));
    let kept =
        r.c.symbols
            .iter()
            .filter(|s| s.path.starts_with("__FBD_") || s.path.starts_with("__NET_"))
            .count();
    eprintln!(
        "the blinker is {} bytes of code and {} of memory, {kept} variables kept for monitoring",
        r.c.code_size, r.c.memory_size
    );
    let (mut changes, mut last_blink) = (0, false);
    let mut toggle = false;
    let mut steps = 100;
    let mut now = 0;
    for _ in 0..200 {
        now += 10;
        r.scan_at(now);
        let blink = r.get("blink") == b(true);
        if blink != last_blink {
            changes += 1;
        }
        last_blink = blink;
        let done = r.get("done") == b(true);
        // The XOR drawn back on itself: toggle flips while done holds.
        if done {
            toggle = !toggle;
        }
        assert_eq!(r.get("toggle"), b(toggle));
        let count = match r.get("count") {
            Value::Int(v) => v,
            v => panic!("{v:?}"),
        };
        assert_eq!(r.get("level"), Value::Real(f32::from(count) * 0.5));
        if !toggle && blink {
            steps += 1;
        }
        assert_eq!(r.get("steps"), Value::Int(steps));
        if !toggle {
            assert_eq!(r.get("held"), b(!blink));
        }
    }
    assert!(changes > 10, "blink goes on and off: {changes}");
}
