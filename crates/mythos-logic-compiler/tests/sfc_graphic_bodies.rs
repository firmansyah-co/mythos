//! A Sequential Function Chart's actions and transition conditions written in LD or FBD,
//! compiled from PLCopen TC6 XML and run on the VM (ADR-013 §2.17): each rule taken from
//! IEC 61131-3 ed. 3 §6.7.3 (Table 55) and §6.7.4 (Tables 56, 57), one test each, scan by scan,
//! and where each body's values are kept for a view to draw it live.

mod support;
#[path = "../../mythos-logic-graphic/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{compile_in, At, Body, BodyOf, Holds, Language, Probe, TargetProfile};
use mythos_logic_vm::{cold_start, read, write, Fault, Program, Value};
use support::{Run, MS};
use tc6::{project, Act, Coil, Contact, In, Pou};

fn run(pous: &[Pou]) -> Run {
    Run::of(compile_in(
        &project(pous),
        Language::Sfc,
        &TargetProfile::pc(),
    ))
}

/// Idle (initial) --go--> Work --stop--> Idle, both in ST, the actions added by each test.
fn chart() -> Pou {
    Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("stop", "BOOL"),
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("lamp", "BOOL"),
            ("n", "INT"),
        ])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"])
}

/// An LD body: the coil `lamp` takes `a` AND NOT `b` (one rung, elements 11–14).
fn ld_lamp() -> Pou {
    Pou::program("_")
        .left(10, 20)
        .contact(11, 100, 20, Contact::No, "a", &["10"])
        .contact(12, 200, 20, Contact::Nc, "b", &["11"])
        .coil(13, 300, 20, Coil::Plain, "lamp", &["12"])
        .right(14, 20, &["13"])
}

/// An FBD body: `n` := `n` + 1 (elements 21–24).
fn fbd_count() -> Pou {
    Pou::program("_")
        .fbd()
        .invar(21, 0, 0, "n")
        .invar(22, 0, 40, "1")
        .block(
            23,
            100,
            0,
            "ADD",
            None,
            &[("IN1", In::From(&["21"])), ("IN2", In::From(&["22"]))],
            &["OUT"],
        )
        .outvar(24, 200, 0, "n", &["23"])
}

fn probe<'a>(r: &'a Run, of: &BodyOf, local_id: u64, at: At) -> &'a Probe {
    r.c.probes
        .iter()
        .find(|p| {
            p.local_id == local_id && p.at == at && p.body.as_ref().map(|b| &b.of) == Some(of)
        })
        .unwrap_or_else(|| panic!("no probe of {of:?} {local_id} {at:?}: {:#?}", r.c.probes))
}

fn path(p: &Probe) -> String {
    match &p.holds {
        Holds::Variable { path, .. } | Holds::Guarded { path, .. } => path.clone(),
        Holds::On => panic!("{p:?} is ON"),
    }
}

/// The flag of the network of `of` whose first element is `first`.
fn network_flag(r: &Run, of: &BodyOf, first: u64) -> String {
    let p =
        r.c.probes
            .iter()
            .find(|p| {
                p.local_id == first
                    && matches!(p.at, At::Network(_))
                    && p.body.as_ref().map(|b| &b.of) == Some(of)
            })
            .unwrap_or_else(|| panic!("no network {first} of {of:?}: {:#?}", r.c.probes));
    path(p)
}

#[test]
fn an_action_in_ld_runs_only_while_its_q_is_true_and_without_a_final_scan() {
    // Table 56 feature 2l, §6.7.4.6: the action runs while Q; without final scan (ADR-013
    // §2.17), so what it wrote stays when its step is left (the standard's note to Table 56
    // writes S8.X into the rung for that).
    let mut r = run(&[chart().action_body("Light", &ld_lamp()).actions_of(
        5,
        300,
        80,
        "3",
        &[(6, "N", None, Act::Name("Light"))],
    )]);
    r.set("a", Value::Bool(true));
    r.scan_at(0);
    assert!(!r.bool("lamp"), "Idle: Light does not run");
    r.set("go", Value::Bool(true));
    r.scan_at(10);
    assert!(
        r.bool("Work.X") && !r.bool("lamp"),
        "activated this scan, runs from the next"
    );
    r.scan_at(20);
    assert!(r.bool("Light.Q") && r.bool("lamp"), "Work: the rung runs");
    r.set("b", Value::Bool(true));
    r.scan_at(30);
    assert!(!r.bool("lamp"), "the contact on b opens the rung");
    r.set("b", Value::Bool(false));
    r.scan_at(40);
    assert!(r.bool("lamp"));
    r.set("go", Value::Bool(false));
    r.set("stop", Value::Bool(true));
    r.scan_at(50);
    assert!(r.bool("Idle.X"));
    r.set("a", Value::Bool(false));
    r.scan_at(60);
    assert!(!r.bool("Light.Q"));
    assert!(
        r.bool("lamp"),
        "no final scan: the coil was not evaluated again"
    );
}

#[test]
fn a_network_of_an_action_that_did_not_run_reads_as_not_run() {
    // ADR-013 §2.15 network flags: TRUE where the network ran in the scan; a body that did not
    // run (its action's Q FALSE) shows none of its networks as run.
    let mut r = run(&[chart().action_body("Light", &ld_lamp()).actions_of(
        5,
        300,
        80,
        "3",
        &[(6, "N", None, Act::Name("Light"))],
    )]);
    let of = BodyOf::Action("Light".into());
    let flag = network_flag(&r, &of, 11);
    assert_eq!(flag, "__SFC_LIGHT__NET_11");
    r.set("a", Value::Bool(true));
    r.scan_at(0);
    assert!(!r.bool(&flag));
    r.set("go", Value::Bool(true));
    r.scan_at(10);
    r.scan_at(20);
    assert!(r.bool(&flag), "Work: the rung ran");
    let contact = path(probe(&r, &of, 11, At::Output(String::new())));
    assert!(r.bool(&contact), "the contact on a passes power");
    r.set("go", Value::Bool(false));
    r.set("stop", Value::Bool(true));
    r.scan_at(30);
    r.scan_at(40);
    assert!(!r.bool(&flag), "Idle: the rung did not run");
    assert!(
        r.bool(&contact),
        "its contact keeps the power of the scan it last ran, which the flag says is not this one's"
    );
}

#[test]
fn an_action_in_fbd_written_in_its_action_block_runs_while_its_q_is_true() {
    // Table 57 feature 4a with Table 58 feature 4f: the action's body is in the block.
    let mut r =
        run(&[chart().actions_of(5, 300, 80, "3", &[(6, "N", None, Act::Body(&fbd_count()))])]);
    r.scan_at(0);
    assert_eq!(r.int("n"), 0);
    r.set("go", Value::Bool(true));
    r.scan_at(10);
    for k in 1..=3 {
        r.scan_at(10 + 10 * k);
        assert_eq!(r.int("n"), k);
    }
    r.set("go", Value::Bool(false));
    r.set("stop", Value::Bool(true));
    r.scan_at(100);
    let n = r.int("n");
    r.scan_at(110);
    r.scan_at(120);
    assert_eq!(r.int("n"), n, "Idle: the body does not run");
    let of = BodyOf::Inline(6);
    let p = probe(&r, &of, 23, At::Output("OUT".into()));
    assert_eq!(path(p), "__SFC_6__FBD_23");
    assert_eq!(p.body, Some(Body { of, ladder: false }));
}

#[test]
fn an_action_with_a_qualifier_runs_its_ld_body_under_the_action_control() {
    // §6.7.4.6: the qualifier decides Q, Q decides the body. P1: one scan, as the step is
    // entered (R_TRIG of the step's flag).
    let rung = Pou::program("_")
        .left(10, 20)
        .block(
            11,
            100,
            20,
            "ADD",
            None,
            &[
                ("EN", In::From(&["10"])),
                ("IN1", In::Expr("n")),
                ("IN2", In::Expr("10")),
            ],
            &["ENO", "OUT"],
        )
        .outvar(12, 200, 20, "n", &["11.OUT"]);
    let mut r = run(&[chart().action_body("Ten", &rung).actions_of(
        5,
        300,
        80,
        "3",
        &[(6, "P1", None, Act::Name("Ten"))],
    )]);
    r.set("go", Value::Bool(true));
    for k in 0..5 {
        r.scan_at(10 * k);
    }
    assert_eq!(r.int("n"), 10, "once, as Work was entered");
}

#[test]
fn two_bodies_with_the_same_local_ids_keep_their_values_apart() {
    // Each body's localIds are its own (TC6: one body each); its hidden variables are in its
    // own namespace, so online change carries each by a path naming the action and the element.
    let r = run(&[chart()
        .action_body("Light", &ld_lamp())
        .action_body(
            "Dark",
            &ld_lamp().coil(15, 300, 60, Coil::Negated, "b", &["10"]),
        )
        .actions_of(
            5,
            300,
            80,
            "3",
            &[
                (6, "N", None, Act::Name("Light")),
                (7, "N", None, Act::Body(&ld_lamp())),
            ],
        )
        .actions_of(8, 300, 0, "1", &[(9, "N", None, Act::Name("Dark"))])]);
    for name in [
        "__SFC_LIGHT__NET_11",
        "__SFC_DARK__NET_11",
        "__SFC_7__NET_11",
        "__SFC_DARK__NET_15",
    ] {
        let s = r.c.symbol(name).unwrap_or_else(|| panic!("no {name}"));
        assert!(s.private, "{name} is private");
    }
}

#[test]
fn a_condition_in_ld_is_the_power_reaching_the_coil_of_the_transitions_name() {
    // Table 55 feature 10: TRANSITION T FROM … TO …, a rung whose coil is the transition's name.
    let cond = Pou::program("_")
        .left(30, 20)
        .contact(31, 100, 20, Contact::No, "a", &["30"])
        .contact(32, 200, 20, Contact::Nc, "b", &["31"])
        .coil(33, 300, 20, Coil::Plain, "Ready", &["32"])
        .right(34, 20, &["33"]);
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("a", "BOOL"), ("b", "BOOL"), ("stop", "BOOL")])
        .transition_body("Ready", &cond)
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans_ref(2, 100, 40, "Ready", &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"]);
    let mut r = run(&[p]);
    r.set("a", Value::Bool(true));
    r.set("b", Value::Bool(true));
    r.scan_at(0);
    assert!(
        r.bool("Idle.X") && !r.bool("__SFC_2"),
        "a AND NOT b is FALSE"
    );
    r.set("b", Value::Bool(false));
    r.scan_at(10);
    assert!(
        r.bool("__SFC_2") && r.bool("Work.X"),
        "a AND NOT b: cleared"
    );
    let of = BodyOf::Transition("Ready".into());
    assert_eq!(
        path(probe(&r, &of, 31, At::Output(String::new()))),
        "__SFC_READY__LD_31"
    );
}

#[test]
fn a_condition_in_fbd_written_in_the_transition_writes_the_name_its_inline_body_gives() {
    // Table 55 feature 3a: an FBD network at the transition; TC6 writes it `inline` with a
    // name, which the network's output variable is (§6.7.3 g).
    let cond = Pou::program("_")
        .fbd()
        .invar(30, 0, 0, "a")
        .invar(31, 0, 40, "b")
        .block(
            32,
            100,
            0,
            "AND",
            None,
            &[("IN1", In::From(&["30"])), ("IN2", In::From(&["31"]))],
            &["OUT"],
        )
        .outvar(33, 200, 0, "Both", &["32"]);
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("a", "BOOL"), ("b", "BOOL"), ("stop", "BOOL")])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans_body(2, 100, 40, "Both", &cond, &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"]);
    let mut r = run(&[p]);
    r.set("a", Value::Bool(true));
    r.scan_at(0);
    assert!(r.bool("Idle.X"));
    r.set("b", Value::Bool(true));
    r.scan_at(10);
    assert!(r.bool("Work.X"));
    let of = BodyOf::Inline(2);
    assert_eq!(
        path(probe(&r, &of, 32, At::Output("OUT".into()))),
        "__SFC_2__FBD_32"
    );
}

/// Idle --(a network drawn in the chart)--> Work --stop--> Idle.
fn wired(network: impl FnOnce(Pou) -> Pou, cond: &[&str], negated: bool) -> Pou {
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("a", "BOOL"), ("b", "BOOL"), ("stop", "BOOL"), ("d", "INT")])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans_wired(2, 100, 40, cond, negated, &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"]);
    network(p)
}

#[test]
fn a_rung_drawn_in_the_chart_gives_the_transition_it_reaches_its_condition() {
    // Table 55 feature 2a: the power reaching the transition is its condition.
    let mut r = run(&[wired(
        |p| {
            p.left(40, 40)
                .contact(41, -100, 40, Contact::No, "a", &["40"])
                .contact(42, -50, 40, Contact::No, "b", &["41"])
        },
        &["42"],
        false,
    )]);
    r.set("a", Value::Bool(true));
    r.scan_at(0);
    assert!(r.bool("Idle.X"));
    r.set("b", Value::Bool(true));
    r.scan_at(10);
    assert!(r.bool("Work.X"));
    // The probes of the rung, said to be of the transition's network; none of the element the
    // chart added where the rung reaches the transition.
    let of = BodyOf::Wired(2);
    assert_eq!(
        probe(&r, &of, 41, At::Network(vec![41, 42])).body,
        Some(Body { of, ladder: true })
    );
    assert!(
        !r.c.probes
            .iter()
            .any(|p| p.body.is_some() && p.local_id == 2),
        "{:#?}",
        r.c.probes
    );
}

#[test]
fn an_fbd_network_drawn_in_the_chart_reaches_its_transition_through_a_connector() {
    // Table 55 features 4a and 6a: the network ends at a connector, its continuation is at the
    // transition (§8.1.3: one line). Negated: the transition clears where the network is FALSE.
    let mut r = run(&[wired(
        |p| {
            p.invar(40, -300, 40, "a")
                .invar(41, -300, 80, "b")
                .block(
                    42,
                    -200,
                    40,
                    "OR",
                    None,
                    &[("IN1", In::From(&["40"])), ("IN2", In::From(&["41"]))],
                    &["OUT"],
                )
                .connector(43, -100, 40, "TRANX", &["42"])
                .continuation(44, 50, 40, "TRANX")
        },
        &["44"],
        true,
    )]);
    r.set("a", Value::Bool(true));
    r.scan_at(0);
    assert!(r.bool("Idle.X"), "a OR b is TRUE, negated FALSE");
    r.set("a", Value::Bool(false));
    r.scan_at(10);
    assert!(r.bool("Work.X"), "NOT (a OR b)");
}

#[test]
fn a_condition_network_is_evaluated_only_where_its_transition_is_enabled() {
    // ADR-013 §2.17: a condition has no effect but a fault, and is not consulted where its
    // transition is not enabled. 100 / d with d = 0 faults where it is evaluated.
    let net = |p: Pou| {
        p.invar(40, -400, 40, "100")
            .invar(41, -400, 80, "d")
            .block(
                42,
                -300,
                40,
                "DIV",
                None,
                &[("IN1", In::From(&["40"])), ("IN2", In::From(&["41"]))],
                &["OUT"],
            )
            .invar(43, -300, 80, "5")
            .block(
                44,
                -200,
                40,
                "GT",
                None,
                &[("IN1", In::From(&["42"])), ("IN2", In::From(&["43"]))],
                &["OUT"],
            )
    };
    // Enabled from the first scan: d = 0 faults.
    let mut r = run(&[wired(net, &["44"], false)]);
    r.now = 0;
    assert!(matches!(r.scan(), Err(Fault::DivideByZero(_))));
    // The same network on the transition out of Work, never enabled while Idle is active.
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL"), ("d", "INT")])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans_wired(4, 100, 120, &["44"], false, &["3"]);
    let mut r = run(&[net(p)]);
    let flag = network_flag(&r, &BodyOf::Wired(4), 40);
    for k in 0..5 {
        r.now = k * 10 * MS;
        r.scan().expect("not enabled, not evaluated: no fault");
        assert!(!r.bool("__SFC_4_EN") && !r.bool(&flag));
    }
    r.set("d", Value::Int(10));
    r.set("go", Value::Bool(true));
    r.scan_at(100);
    r.scan_at(110);
    assert!(r.bool("__SFC_4_EN") && r.bool(&flag), "enabled: evaluated");
    assert!(r.bool("__SFC_4"), "100 / 10 > 5");
}

#[test]
fn a_negated_transition_negates_its_ld_condition() {
    let cond = Pou::program("_")
        .left(30, 20)
        .contact(31, 100, 20, Contact::No, "a", &["30"])
        .coil(32, 300, 20, Coil::Plain, "T", &["31"]);
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("a", "BOOL"), ("stop", "BOOL")])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans_body(2, 100, 40, "T", &cond, &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"]);
    let xml = project(&[p]).replace("<condition>", "<condition negated=\"true\">");
    let mut r = Run::of(compile_in(&xml, Language::Sfc, &TargetProfile::pc()));
    r.set("a", Value::Bool(true));
    r.scan_at(0);
    assert!(r.bool("Idle.X"));
    r.set("a", Value::Bool(false));
    r.scan_at(10);
    assert!(r.bool("Work.X"));
}

#[test]
fn an_edge_in_an_ld_action_senses_from_the_last_scan_the_action_ran() {
    // Table 75 feature 3 in an action: its memory (`__SFC_<action>__EDGE_<localId>`) is set only
    // where the action runs, as every statement of the body is.
    let rung = Pou::program("_")
        .left(10, 20)
        .contact(11, 100, 20, Contact::Rising, "a", &["10"])
        .block(
            12,
            200,
            20,
            "ADD",
            None,
            &[
                ("EN", In::From(&["11"])),
                ("IN1", In::Expr("n")),
                ("IN2", In::Expr("1")),
            ],
            &["ENO", "OUT"],
        )
        .outvar(13, 300, 20, "n", &["12.OUT"]);
    let mut r = run(&[chart().action_body("Count", &rung).actions_of(
        5,
        300,
        80,
        "3",
        &[(6, "N", None, Act::Name("Count"))],
    )]);
    assert!(r.c.symbol("__SFC_COUNT__EDGE_11").is_some());
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    r.scan_at(10);
    r.set("a", Value::Bool(true));
    r.scan_at(20);
    assert_eq!(r.int("n"), 1, "a rose");
    r.scan_at(30);
    assert_eq!(r.int("n"), 1);
    // a falls and rises while Work is not active: the action does not see it.
    r.set("go", Value::Bool(false));
    r.set("stop", Value::Bool(true));
    r.scan_at(40);
    r.set("a", Value::Bool(false));
    r.scan_at(50);
    r.set("a", Value::Bool(true));
    r.scan_at(60);
    r.set("stop", Value::Bool(false));
    r.set("go", Value::Bool(true));
    r.scan_at(70);
    r.scan_at(80);
    assert_eq!(
        r.int("n"),
        1,
        "the contact last sensed a TRUE, and a is TRUE: no edge"
    );
}

/// Load `after` in place of `before` as an online change does: every variable of the same
/// path, type and size carried over, the rest from its initial value.
fn take_over(before: &Run, after: &str) -> Run {
    let mut r = Run::of(compile_in(after, Language::Sfc, &TargetProfile::pc()));
    let p = Program::load(&r.c.bytecode).unwrap();
    let mut memory = vec![0; p.memory_size()];
    cold_start(&p, &mut memory).unwrap();
    for s in &r.c.symbols {
        if s.array.is_some() {
            continue;
        }
        if let Some(old) = before
            .c
            .symbols
            .iter()
            .find(|o| o.path.eq_ignore_ascii_case(&s.path) && o.ty == s.ty && o.array.is_none())
        {
            let ty = support::elem(&s.ty);
            let v = read(&before.memory, old.offset, ty).unwrap();
            assert!(write(&mut memory, s.offset, v));
        }
    }
    r.memory = memory;
    r.now = before.now;
    r
}

#[test]
fn an_online_change_carries_an_ld_actions_edge_memory_by_its_path() {
    // The edge's memory is `__SFC_COUNT__EDGE_11`: a build that keeps the action's name and the
    // contact's localId keeps it, though the rung changed around it.
    let rung = |extra: bool| {
        let p = Pou::program("_")
            .left(10, 20)
            .contact(11, 100, 20, Contact::Rising, "a", &["10"]);
        let p = if extra {
            p.contact(14, 150, 20, Contact::No, "TRUE", &["11"])
        } else {
            p
        };
        p.block(
            12,
            200,
            20,
            "ADD",
            None,
            &[
                ("EN", In::From(&[if extra { "14" } else { "11" }])),
                ("IN1", In::Expr("n")),
                ("IN2", In::Expr("1")),
            ],
            &["ENO", "OUT"],
        )
        .outvar(13, 300, 20, "n", &["12.OUT"])
    };
    let doc = |extra: bool| {
        project(&[chart().action_body("Count", &rung(extra)).actions_of(
            5,
            300,
            80,
            "3",
            &[(6, "N", None, Act::Name("Count"))],
        )])
    };
    let mut r = Run::of(compile_in(&doc(false), Language::Sfc, &TargetProfile::pc()));
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    r.set("a", Value::Bool(true));
    r.scan_at(10);
    r.scan_at(20);
    assert_eq!(r.int("n"), 1);
    let mut r = take_over(&r, &doc(true));
    r.scan_at(30);
    assert_eq!(
        r.int("n"),
        1,
        "the edge's memory was carried: a is still TRUE, no edge"
    );
}

#[test]
fn the_mixer_chart_the_emulators_run_does_what_its_bodies_say() {
    // `tests/sfc/mixer.xml` over the scans `scripts/logic-vm-on-emulators.sh` runs: 60, 20 ms
    // apart, from the clock's zero. Idle lasts until its FBD action sets ready (40 ms); Fill
    // until its FBD condition sees 30 ms; Stir until the FBD network drawn in the chart sees
    // 70 ms: a cycle of 8 scans, Fill entered at scans 2, 10, …, 58.
    let doc = include_str!("../../mythos-logic-graphic/tests/sfc/mixer.xml");
    let mut r = Run::of(compile_in(doc, Language::Sfc, &TargetProfile::rp2040()));
    eprintln!(
        "{} bytes of code, {} of memory, {} private symbols",
        r.c.code_size,
        r.c.memory_size,
        r.c.symbols.iter().filter(|s| s.private).count()
    );
    let steps = ["Idle", "Fill", "Stir"];
    let mut trace = Vec::new();
    for k in 0..60 {
        r.scan_at(k * 20);
        let active: Vec<&str> = steps
            .iter()
            .copied()
            .filter(|s| r.bool(&format!("{s}.X")))
            .collect();
        assert_eq!(active.len(), 1, "scan {k}: one step at a time, {active:?}");
        // Once Stir has run its actions (from the scan after it was entered), the LD action in
        // its block has reset the valve that Open set.
        if active[0] == "Stir" && trace.last() == Some(&"Stir") {
            assert!(!r.bool("valve"), "scan {k}");
        }
        if active[0] == "Fill" && trace.last() == Some(&"Fill") {
            assert!(r.bool("valve"), "scan {k}");
        }
        trace.push(active[0]);
    }
    let entered = |name: &str| {
        (1..trace.len())
            .filter(|k| trace[*k] == name && trace[*k - 1] != name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        entered("Fill"),
        [2, 10, 18, 26, 34, 42, 50, 58],
        "{trace:?}"
    );
    assert_eq!(entered("Stir"), [4, 12, 20, 28, 36, 44, 52], "{trace:?}");
    // P1 Count once per Fill; motor (L 50 ms) on for three scans of each Stir, stirred + 1 each.
    assert_eq!(r.int("batches"), 8);
    assert_eq!(r.int("stirred"), 21);
    assert!(r.bool("valve"), "Fill entered at 58 opened it at 59");
}

#[test]
fn an_ld_condition_ending_at_a_coil_takes_no_power_where_its_function_did_not_run() {
    // A coil's power is defined where a function under EN did not run: OFF (ADR-013 §2.15), so
    // the condition is computed in every evaluation and never an earlier one's (review of PR
    // #163: an output variable in its place was refused, §2.17).
    let cond = Pou::program("_")
        .left(30, 20)
        .contact(31, 100, 20, Contact::No, "en", &["30"])
        .block(
            32,
            200,
            20,
            "AND",
            None,
            &[
                ("EN", In::From(&["31"])),
                ("IN1", In::Expr("a")),
                ("IN2", In::Expr("b")),
            ],
            &["ENO", "OUT"],
        )
        .coil(33, 300, 20, Coil::Plain, "T", &["32.OUT"]);
    let p = Pou::program("Main")
        .sfc()
        .vars(&[
            ("en", "BOOL"),
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("stop", "BOOL"),
        ])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans_body(2, 100, 40, "T", &cond, &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"]);
    let mut r = run(&[p]);
    for v in ["en", "a", "b"] {
        r.set(v, Value::Bool(true));
    }
    r.scan_at(0);
    assert!(r.bool("Work.X"), "en, a AND b: cleared");
    r.set("stop", Value::Bool(true));
    r.scan_at(10);
    assert!(r.bool("Idle.X"));
    r.set("stop", Value::Bool(false));
    // a AND b would be TRUE, but EN is FALSE: the AND does not run, its power is OFF.
    r.set("en", Value::Bool(false));
    for k in 0..3 {
        r.scan_at(20 + 10 * k);
        assert!(r.bool("Idle.X") && r.bool("__SFC_2_EN") && !r.bool("__SFC_2"));
    }
    r.set("en", Value::Bool(true));
    r.scan_at(60);
    assert!(r.bool("Work.X"));
}

#[test]
fn a_function_of_an_fbd_action_that_did_not_run_is_not_shown_as_computed() {
    // ADR-013 §2.16: a guarded value's `ran` says where it was computed in this scan. In a body
    // that did not run (its action's Q FALSE) it reads FALSE, as the network's flag does (found
    // in review: `ran` stayed TRUE once the action stopped).
    let body = Pou::program("_")
        .fbd()
        .invar(30, 0, 0, "go")
        .invar(31, 0, 40, "a")
        .invar(32, 0, 80, "b")
        .block(
            33,
            100,
            0,
            "AND",
            None,
            &[
                ("EN", In::From(&["30"])),
                ("IN1", In::From(&["31"])),
                ("IN2", In::From(&["32"])),
            ],
            &["ENO", "OUT"],
        )
        .outvar(34, 200, 0, "lamp", &["33.OUT"]);
    let mut r = run(&[chart().action_body("Light", &body).actions_of(
        5,
        300,
        80,
        "3",
        &[(6, "N", None, Act::Name("Light"))],
    )]);
    let of = BodyOf::Action("Light".into());
    let (value, ran) = match &probe(&r, &of, 33, At::Output("OUT".into())).holds {
        Holds::Guarded { path, ran } => (path.clone(), ran.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(ran, "__SFC_LIGHT__FBD_33_ENO");
    let flag = network_flag(&r, &of, 30);
    for v in ["go", "a", "b"] {
        r.set(v, Value::Bool(true));
    }
    r.scan_at(0);
    r.scan_at(10);
    assert!(r.bool(&flag) && r.bool(&ran) && r.bool(&value), "Work: ran");
    r.set("go", Value::Bool(false));
    r.set("stop", Value::Bool(true));
    r.scan_at(20);
    r.scan_at(30);
    assert!(r.bool("Idle.X"));
    assert!(!r.bool(&flag), "the network did not run");
    assert!(!r.bool(&ran), "so the AND was not computed in this scan");
    assert!(
        r.bool(&value),
        "its kept value is an earlier scan's, which ran says"
    );
}

#[test]
fn an_edge_in_an_ld_action_sees_a_rise_made_while_its_action_did_not_run() {
    // The memory of an edge in an action is set only where the action runs: a falls while the
    // action runs and rises while it does not, so at the next run the contact senses the rise.
    // A memory set every scan would have followed a while the action was idle, and seen none.
    let rung = Pou::program("_")
        .left(10, 20)
        .contact(11, 100, 20, Contact::Rising, "a", &["10"])
        .block(
            12,
            200,
            20,
            "ADD",
            None,
            &[
                ("EN", In::From(&["11"])),
                ("IN1", In::Expr("n")),
                ("IN2", In::Expr("1")),
            ],
            &["ENO", "OUT"],
        )
        .outvar(13, 300, 20, "n", &["12.OUT"]);
    let mut r = run(&[chart().action_body("Count", &rung).actions_of(
        5,
        300,
        80,
        "3",
        &[(6, "N", None, Act::Name("Count"))],
    )]);
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    r.scan_at(10);
    assert_eq!(r.int("n"), 0, "Work runs Count with a FALSE");
    r.set("go", Value::Bool(false));
    r.set("stop", Value::Bool(true));
    r.scan_at(20);
    assert!(r.bool("Idle.X"));
    // a rises while Count does not run.
    r.set("a", Value::Bool(true));
    r.scan_at(30);
    r.scan_at(40);
    assert_eq!(r.int("n"), 0, "Idle: Count does not run");
    r.set("stop", Value::Bool(false));
    r.set("go", Value::Bool(true));
    r.scan_at(50);
    r.scan_at(60);
    assert_eq!(
        r.int("n"),
        1,
        "the contact last sensed a FALSE, in Work: it senses the rise at its next run"
    );
}

#[test]
fn the_probes_of_networks_drawn_in_the_chart_name_variables_the_program_has() {
    // Review of PR #163: transitions drawn above their networks (the element the chart adds is
    // first by position), two networks from one left rail, one transition negated.
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("a", "BOOL"), ("b", "BOOL")])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans_wired(2, 100, 40, &["41"], false, &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans_wired(4, 100, 120, &["42"], true, &["3"])
        .left(40, 400)
        .contact(41, 300, 400, Contact::No, "a", &["40"])
        .contact(42, 300, 440, Contact::No, "b", &["40"]);
    let r = run(&[p]);
    let bodies: Vec<&Probe> = r.c.probes.iter().filter(|p| p.body.is_some()).collect();
    assert!(bodies.len() >= 4, "{bodies:#?}");
    for p in bodies {
        assert!(
            p.local_id != 2 && p.local_id != 4,
            "no probe of the added element: {p:?}"
        );
        if let At::Network(members) = &p.at {
            assert!(!members.contains(&2) && !members.contains(&4), "{p:?}");
        }
        if !matches!(p.holds, Holds::On) {
            let kept = path(p);
            assert!(r.c.symbol(&kept).is_some(), "no symbol {kept}");
        }
    }
}
