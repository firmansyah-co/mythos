//! What the LD front-end keeps for monitoring (ADR-013 §2.15): the power of every output of an
//! element and of every junction of links, each in a hidden variable assigned when it is
//! evaluated in the scan, and for every network a flag saying whether it ran. Each of these,
//! read after a scan, is the power the rung had in it, not one worked out from the variables
//! afterwards; and a network a jump or a return skipped reads as not run.

mod support;
#[path = "../../mythos-logic-ld/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{
    compile, compile_in, compile_sources, At, Holds, Language, Probe, Sources, TargetProfile,
};
use mythos_logic_vm::Value;
use support::Run;
use tc6::{project, Coil, Contact, In, Pou};

fn var(path: &str) -> Holds {
    Holds::Variable {
        path: path.into(),
        negated: false,
    }
}

fn out(local_id: u64, output: &str, holds: Holds) -> Probe {
    Probe {
        pou: "Main".into(),
        local_id,
        at: At::Output(output.into()),
        holds,
    }
}

fn junction(local_id: u64, input: &str, holds: Holds) -> Probe {
    Probe {
        pou: "Main".into(),
        local_id,
        at: At::Input(input.into()),
        holds,
    }
}

fn network(members: &[u64]) -> Probe {
    Probe {
        pou: "Main".into(),
        local_id: members[0],
        at: At::Network(members.to_vec()),
        holds: var(&format!("__NET_{}", members[0])),
    }
}

fn run(p: Pou) -> Run {
    Run::of(compile_in(
        &project(&[p]),
        Language::Ld,
        &TargetProfile::pc(),
    ))
}

fn set(r: &mut Run, path: &str, v: bool) {
    r.set(path, Value::Bool(v));
}

/// A branch point and a coil whose output goes on; a timer; two branches joined before a coil.
fn rungs() -> Pou {
    Pou::program("Main")
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
            ("s", "BOOL"),
            ("done", "BOOL"),
            ("o", "BOOL"),
            ("t", "TON"),
        ])
        .left(1, 0)
        // Contact 2 feeds two coils; coil 3's output goes on to coil 5.
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        .coil(3, 300, 20, Coil::Plain, "q", &["2"])
        .coil(4, 300, 60, Coil::Negated, "r", &["2"])
        .coil(5, 400, 20, Coil::Plain, "s", &["3"])
        // A timer's BOOL output is power; its TIME output is a value, which carries none.
        .contact(6, 100, 120, Contact::Nc, "b", &["1"])
        .block(
            7,
            200,
            120,
            "TON",
            Some("t"),
            &[("IN", In::From(&["6"])), ("PT", In::Expr("T#0ms"))],
            &["Q", "ET"],
        )
        .coil(8, 400, 120, Coil::Plain, "done", &["7.Q"])
        // Two branches joined at coil 12's input: a junction of its own.
        .contact(10, 100, 200, Contact::No, "a", &["1"])
        .contact(11, 100, 240, Contact::No, "b", &["1"])
        .coil(12, 300, 200, Coil::Plain, "o", &["10", "11"])
        .right(9, 0, &["4", "5", "8", "12"])
}

#[test]
fn every_output_junction_and_network_is_published_with_the_variable_keeping_it() {
    let compiled = compile_in(&project(&[rungs()]), Language::Ld, &TargetProfile::pc())
        .unwrap_or_else(|d| panic!("{d:?}"));
    assert_eq!(
        compiled.probes,
        vec![
            out(1, "", Holds::On),
            out(2, "", var("__LD_2")),
            network(&[2, 3, 5, 4]),
            out(3, "", var("__LD_3")),
            // A coil passes on the power that reaches it (Table 76), negated or not.
            out(4, "", var("__LD_4")),
            out(5, "", var("__LD_5")),
            out(6, "", var("__LD_6")),
            network(&[6, 7, 8]),
            out(7, "Q", var("__LD_7_Q")),
            out(8, "", var("__LD_8")),
            out(10, "", var("__LD_10")),
            network(&[10, 12, 11]),
            out(11, "", var("__LD_11")),
            out(12, "", var("__LD_12")),
            junction(12, "", var("__LD_12_IN")),
        ],
        "ET, a TIME, carries no power and has none"
    );
    // Each is a private symbol made by the front-end, which no tag writes.
    for p in &compiled.probes {
        if let Holds::Variable { path, .. } = &p.holds {
            let s = compiled
                .symbol(path)
                .unwrap_or_else(|| panic!("{path} is a symbol"));
            assert_eq!(s.ty, "BOOL", "{path}");
            assert!(s.private && !s.writable, "{path} is private");
        }
    }
}

#[test]
fn each_kept_power_after_a_scan_is_the_power_the_rung_had_in_it() {
    let mut r = run(rungs());
    for bits in 0..4u8 {
        let (a, b) = (bits & 1 != 0, bits & 2 != 0);
        set(&mut r, "a", a);
        set(&mut r, "b", b);
        r.scan().unwrap();
        let said = format!("a={a} b={b}");
        for (path, power) in [
            ("__LD_2", a),
            ("__LD_3", a),
            ("__LD_4", a),
            ("__LD_5", a),
            ("__LD_6", !b),
            // With PT 0 the timer's Q follows IN, which is NOT b.
            ("__LD_7_Q", !b),
            ("__LD_8", !b),
            ("__LD_10", a),
            ("__LD_11", b),
            ("__LD_12_IN", a || b),
            ("__LD_12", a || b),
            ("__NET_2", true),
            ("__NET_6", true),
            ("__NET_10", true),
        ] {
            assert_eq!(r.bool(path), power, "{path}, {said}");
        }
        assert_eq!(
            (r.bool("q"), r.bool("r"), r.bool("s")),
            (a, !a, a),
            "{said}"
        );
        assert_eq!((r.bool("done"), r.bool("o")), (!b, a || b), "{said}");
    }
}

/// Start and the motor's own contact in parallel, then Stop, then the motor's coil.
fn seal_in() -> Pou {
    Pou::program("Main")
        .vars(&[("start", "BOOL"), ("stop", "BOOL"), ("motor", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "start", &["1"])
        .contact(4, 100, 60, Contact::No, "motor", &["1"])
        .contact(3, 200, 20, Contact::Nc, "stop", &["2", "4"])
        .coil(5, 300, 20, Coil::Plain, "motor", &["3"])
        .right(9, 0, &["5"])
}

#[test]
fn a_seal_in_contact_keeps_what_it_read_before_its_coil_wrote() {
    let mut r = run(seal_in());
    let mut motor = false;
    let steps = [
        (false, false),
        (true, false),
        (false, false),
        (false, false),
        (false, true),
        (false, false),
        (true, true),
        (true, false),
    ];
    let mut differed = 0;
    for (scan, (start, stop)) in steps.into_iter().enumerate() {
        set(&mut r, "start", start);
        set(&mut r, "stop", stop);
        r.scan().unwrap();
        // The rung as the scan evaluated it: the seal contact read the motor before the coil.
        let seal = motor;
        let joined = start || seal;
        motor = joined && !stop;
        let said = format!("scan {scan}: start={start} stop={stop}");
        assert_eq!(r.bool("__LD_2"), start, "{said}");
        assert_eq!(r.bool("__LD_4"), seal, "{said}");
        assert_eq!(r.bool("__LD_3_IN"), joined, "{said}");
        assert_eq!(r.bool("__LD_3"), motor, "{said}");
        assert_eq!(r.bool("__LD_5"), motor, "{said}");
        assert_eq!(r.bool("motor"), motor, "{said}");
        differed += (r.bool("__LD_4") != r.bool("motor")) as usize;
    }
    // In those scans the seal contact's variable, read after the scan, would draw a guess.
    assert_eq!(differed, 3, "the motor changed in three scans: 1, 4 and 7");
}

#[test]
fn a_network_a_jump_skips_reads_as_not_run_and_keeps_what_it_had() {
    let p = Pou::program("Main")
        .vars(&[
            ("skip", "BOOL"),
            ("a", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
        ])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "skip", &["1"])
        .jump(3, 300, 20, "PAST", &["2"])
        .contact(4, 100, 60, Contact::No, "a", &["1"])
        .coil(5, 300, 60, Coil::Plain, "q", &["4"])
        .label(6, 100, "PAST")
        .contact(7, 100, 120, Contact::No, "a", &["1"])
        .coil(8, 300, 120, Coil::Plain, "r", &["7"])
        .right(9, 0, &["5", "8"]);
    let compiled = compile_in(
        &project(std::slice::from_ref(&p)),
        Language::Ld,
        &TargetProfile::pc(),
    )
    .unwrap_or_else(|d| panic!("{d:?}"));
    assert!(compiled.probes.contains(&network(&[4, 5])));
    let mut r = run(p);
    set(&mut r, "skip", false);
    set(&mut r, "a", true);
    r.scan().unwrap();
    assert!(r.bool("__NET_2") && r.bool("__NET_4") && r.bool("__NET_7"));
    assert!(r.bool("__LD_4") && r.bool("__LD_5") && r.bool("q"));
    // Skipped: its flag says so; what it kept is the last scan that ran it, not this one's.
    set(&mut r, "skip", true);
    set(&mut r, "a", false);
    r.scan().unwrap();
    assert!(r.bool("__NET_2") && r.bool("__LD_2"));
    assert!(
        !r.bool("__NET_4"),
        "the network the jump skipped did not run"
    );
    assert!(r.bool("__LD_4") && r.bool("q"), "kept from the scan before");
    assert!(r.bool("__NET_7") && !r.bool("__LD_7") && !r.bool("r"));
    // Run again: it reads as run, with this scan's power.
    set(&mut r, "skip", false);
    r.scan().unwrap();
    assert!(r.bool("__NET_4") && !r.bool("__LD_4") && !r.bool("q"));
}

#[test]
fn the_networks_after_a_return_read_as_not_run() {
    let p = Pou::program("Main")
        .vars(&[("stop", "BOOL"), ("a", "BOOL"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "stop", &["1"])
        .ret(3, 300, 20, &["2"])
        .contact(4, 100, 60, Contact::No, "a", &["1"])
        .coil(5, 300, 60, Coil::Plain, "q", &["4"])
        .right(9, 0, &["5"]);
    let mut r = run(p);
    set(&mut r, "a", true);
    for stop in [false, true, true, false, true] {
        set(&mut r, "stop", stop);
        r.scan().unwrap();
        assert!(r.bool("__NET_2"), "the return's own network ran");
        assert_eq!(r.bool("__LD_2"), stop);
        assert_eq!(r.bool("__NET_4"), !stop, "stop={stop}");
    }
}

#[test]
fn a_backward_jump_runs_its_networks_again_and_each_reads_as_run() {
    // Count to 3 in one scan: the jump goes back while n < 3.
    let p = Pou::program("Main")
        .vars(&[("n", "INT"), ("more", "BOOL")])
        .label(2, 0, "AGAIN")
        .left(1, 0)
        .invar(3, 100, 20, "n + 1")
        .outvar(4, 300, 20, "n", &["3"])
        .invar(5, 100, 60, "n < 3")
        .outvar(6, 300, 60, "more", &["5"])
        .contact(7, 100, 100, Contact::No, "more", &["1"])
        .jump(8, 300, 100, "AGAIN", &["7"])
        .right(9, 0, &[]);
    let mut r = run(p);
    r.scan().unwrap();
    assert_eq!(r.int("n"), 3);
    assert!(r.bool("__NET_3") && r.bool("__NET_5") && r.bool("__NET_7"));
    // The last time round, the jump's contact was off.
    assert!(!r.bool("__LD_7"));
}

#[test]
fn an_instance_called_by_two_blocks_keeps_each_calls_output() {
    // The end of the scan holds what the second call left; each block's Q holds its own.
    let p = Pou::program("Main")
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
            ("t", "TON"),
        ])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        .block(
            3,
            200,
            20,
            "TON",
            Some("t"),
            &[("IN", In::From(&["2"])), ("PT", In::Expr("T#0ms"))],
            &["Q"],
        )
        .coil(4, 400, 20, Coil::Plain, "q", &["3.Q"])
        .contact(5, 100, 100, Contact::No, "b", &["1"])
        .block(
            6,
            200,
            100,
            "TON",
            Some("t"),
            &[("IN", In::From(&["5"])), ("PT", In::Expr("T#0ms"))],
            &["Q"],
        )
        .coil(7, 400, 100, Coil::Plain, "r", &["6.Q"])
        .right(9, 0, &["4", "7"]);
    let mut r = run(p);
    set(&mut r, "a", true);
    set(&mut r, "b", false);
    r.scan().unwrap();
    assert!(r.bool("__LD_3_Q") && r.bool("q"));
    assert!(!r.bool("__LD_6_Q") && !r.bool("r"));
    assert!(!r.bool("t.Q"), "t.Q after the scan is the second call's");
}

#[test]
fn a_function_with_en_keeps_eno_and_its_result_as_power_false_where_it_did_not_run() {
    let p = Pou::program("Main")
        .vars(&[("en", "BOOL"), ("x", "INT"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "en", &["1"])
        .block(
            3,
            200,
            20,
            "GT",
            None,
            &[
                ("EN", In::From(&["2"])),
                ("IN1", In::Expr("x")),
                ("IN2", In::Expr("5")),
            ],
            &["ENO", "OUT"],
        )
        .coil(4, 400, 20, Coil::Plain, "q", &["3.OUT"])
        .right(9, 0, &["4"]);
    let compiled = compile_in(
        &project(std::slice::from_ref(&p)),
        Language::Ld,
        &TargetProfile::pc(),
    )
    .unwrap_or_else(|d| panic!("{d:?}"));
    assert!(compiled.probes.contains(&out(3, "ENO", var("__LD_3_ENO"))));
    assert!(compiled.probes.contains(&out(3, "OUT", var("__LD_3"))));
    let mut r = run(p);
    for (en, x, eno, gt) in [
        (false, 7, false, false),
        (true, 7, true, true),
        (true, 3, true, false),
        (false, 9, false, false),
    ] {
        set(&mut r, "en", en);
        r.set("x", Value::Int(x));
        r.scan().unwrap();
        assert_eq!(r.bool("__LD_3_ENO"), eno, "en={en} x={x}");
        assert_eq!(r.bool("__LD_3"), gt, "en={en} x={x}");
        assert_eq!(r.bool("q"), gt, "en={en} x={x}");
    }
}

#[test]
fn structured_text_alone_publishes_nothing_and_an_st_function_block_is_not_probed() {
    let st = "PROGRAM Main VAR a : BOOL; END_VAR a := NOT a; END_PROGRAM";
    assert_eq!(compile(st, &TargetProfile::pc()).unwrap().probes, vec![]);
    // An LD program beside an ST source: the probes are of the LD POUs only.
    let doc = project(&[rungs()]);
    let ld = [("main.xml", doc.as_str())];
    let compiled = compile_sources(
        &Sources {
            st: Some((
                "pous.st",
                "FUNCTION_BLOCK Twice VAR_INPUT x : BOOL; END_VAR VAR_OUTPUT y : BOOL; END_VAR y := x; END_FUNCTION_BLOCK",
            )),
            ld: &ld,
        },
        &TargetProfile::pc(),
    )
    .unwrap_or_else(|d| panic!("{d:?}"));
    assert!(compiled.probes.iter().all(|p| p.pou == "Main"));
    assert_eq!(compiled.probes.len(), 15, "{:?}", compiled.probes);
}
