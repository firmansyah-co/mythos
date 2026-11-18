//! What the LD front-end tells an editor about power flow (ADR-013 §2.15): for each output of
//! an element, the variable that holds its state once a scan is over, where one holds it
//! exactly; and that variable, read after a scan, is the power the output carried in it.

mod support;
#[path = "../../mythos-logic-ld/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{
    compile, compile_in, compile_sources, Holds, Language, Probe, Sources, TargetProfile,
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

fn probe(local_id: u64, output: &str, holds: Holds) -> Probe {
    Probe {
        pou: "Main".into(),
        local_id,
        output: output.into(),
        holds,
    }
}

/// A branch point, a coil whose output goes on, a timer and a contact used once.
fn rungs() -> Pou {
    Pou::program("Main")
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("q", "BOOL"),
            ("r", "BOOL"),
            ("s", "BOOL"),
            ("done", "BOOL"),
            ("t", "TON"),
        ])
        .left(1, 0)
        // Contact 2 feeds two coils: its output is kept in __LD_2.
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        // Coil 3's output goes on to coil 5: the power reaching it is kept in __LD_3.
        .coil(3, 300, 20, Coil::Plain, "q", &["2"])
        .coil(4, 300, 60, Coil::Negated, "r", &["2"])
        .coil(5, 400, 20, Coil::Plain, "s", &["3"])
        // Contact 6 is used once: its output is an expression kept nowhere.
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
        .right(9, 0, &["4", "5", "8"])
}

#[test]
fn every_output_a_variable_holds_exactly_is_published_and_nothing_else() {
    let compiled = compile_in(&project(&[rungs()]), Language::Ld, &TargetProfile::pc())
        .unwrap_or_else(|d| panic!("{d:?}"));
    assert_eq!(
        compiled.probes,
        vec![
            probe(1, "", Holds::On),
            probe(2, "", var("__LD_2")),
            probe(3, "", var("__LD_3")),
            // A coil passes on the power that reaches it (Table 76), negated or not.
            probe(4, "", var("__LD_2")),
            probe(5, "", var("__LD_3")),
            probe(7, "ET", var("t.ET")),
            probe(7, "Q", var("t.Q")),
        ],
        "contact 6 (used once) and coil 8 (fed by an expression kept nowhere: t.Q read here, \
         which only the block's own output is published as) have none"
    );
    // Each published variable is a symbol the runtime can be asked for.
    for p in &compiled.probes {
        if let Holds::Variable { path, .. } = &p.holds {
            assert!(compiled.symbol(path).is_some(), "{path} is a symbol");
        }
    }
}

#[test]
fn a_published_variable_after_a_scan_is_the_power_its_output_carried_in_it() {
    let doc = project(&[rungs()]);
    let mut r = Run::of(compile_in(&doc, Language::Ld, &TargetProfile::pc()));
    for bits in 0..4u8 {
        let (a, b) = (bits & 1 != 0, bits & 2 != 0);
        r.set("a", Value::Bool(a));
        r.set("b", Value::Bool(b));
        r.scan().unwrap();
        // The power out of contact 2 and into coil 3 is a; out of coil 3 the same.
        assert_eq!(r.bool("__LD_2"), a, "a={a}");
        assert_eq!(r.bool("__LD_3"), a, "a={a}");
        assert_eq!(r.bool("q"), a);
        assert_eq!(r.bool("r"), !a);
        assert_eq!(r.bool("s"), a);
        // With PT 0 the timer's Q follows IN, which is NOT b; done is what t.Q carried.
        assert_eq!(r.bool("t.Q"), !b, "b={b}");
        assert_eq!(r.bool("done"), r.bool("t.Q"));
    }
}

#[test]
fn a_body_that_jumps_or_returns_publishes_nothing() {
    for transfer in ["jump", "return"] {
        let mut p = Pou::program("Main")
            .vars(&[("a", "BOOL"), ("q", "BOOL"), ("r", "BOOL")])
            .left(1, 0)
            .contact(2, 100, 20, Contact::No, "a", &["1"])
            .coil(3, 300, 20, Coil::Plain, "q", &["2"])
            .coil(4, 300, 40, Coil::Plain, "r", &["2"]);
        p = if transfer == "jump" {
            p.jump(5, 300, 60, "END", &["2"]).label(6, 100, "END")
        } else {
            p.ret(5, 300, 60, &["2"])
        };
        let compiled = compile_in(
            &project(&[p.right(9, 0, &["3", "4"])]),
            Language::Ld,
            &TargetProfile::pc(),
        )
        .unwrap_or_else(|d| panic!("{transfer}: {d:?}"));
        assert_eq!(compiled.probes, vec![], "{transfer}");
    }
}

#[test]
fn an_instance_called_by_two_blocks_publishes_no_output() {
    // The end of the scan holds what the second call left, not what the first block gave.
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
    let compiled = compile_in(&project(&[p]), Language::Ld, &TargetProfile::pc())
        .unwrap_or_else(|d| panic!("{d:?}"));
    assert_eq!(compiled.probes, vec![probe(1, "", Holds::On)]);
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
    assert_eq!(compiled.probes.len(), 7, "{:?}", compiled.probes);
}
