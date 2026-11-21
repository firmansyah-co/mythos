//! What the Sequential Function Chart front-end refuses, and where each diagnostic says it is
//! (ADR-013 §2.17): the element by its `localId` and position, the action or transition of the
//! POU by its name, and the characters of a text, its line too when the text has several.

mod support;

use mythos_logic_graphic::{lower, Source, MAX_SITUATIONS};
use mythos_logic_ir::Diagnostic;
use support::tc6::{project, Act, Coil, Contact, In, Pou};

fn lowered(pous: &[Pou]) -> Result<(), Vec<Diagnostic>> {
    let xml = project(pous);
    lower(
        &[Source {
            name: "main.xml",
            text: &xml,
        }],
        None,
    )
    .map(|_| ())
}

fn refused(pous: &[Pou]) -> Vec<Diagnostic> {
    match lowered(pous) {
        Ok(()) => panic!("lowered, but should not have:\n{}", project(pous)),
        Err(d) => d,
    }
}

fn lowers(pous: &[Pou]) {
    if let Err(d) = lowered(pous) {
        panic!("{d:#?}\n{}", project(pous));
    }
}

/// The one diagnostic, with its element's id.
fn one(d: Vec<Diagnostic>) -> (Option<u64>, String, Diagnostic) {
    assert_eq!(d.len(), 1, "one diagnostic expected: {d:#?}");
    let d = d.into_iter().next().unwrap();
    (
        d.element.as_ref().and_then(|e| e.local_id),
        d.message.clone(),
        d,
    )
}

/// Idle (initial) --go--> Run --stop--> Idle, to which each test adds what is wrong.
fn base() -> Pou {
    Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("stop", "BOOL"),
            ("lamp", "BOOL"),
            ("n", "INT"),
        ])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "Run", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"])
}

#[test]
fn the_base_lowers() {
    lowers(&[base()]);
    lowers(&[base().actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("lamp"))])]);
}

#[test]
fn a_chart_without_an_initial_step_is_refused() {
    // §6.7.2 error 1: each SFC network has exactly one initial step.
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL")])
        .step(1, 100, 0, "A", false, &["4"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "B", false, &["2"])
        .trans(4, 100, 120, "go", &["3"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(1));
    assert_eq!(
        msg,
        "the chart of the steps A, B has no initial step; each chart has exactly one (IEC 61131-3 ed. 3 §6.7.2)"
    );
}

#[test]
fn a_chart_with_two_initial_steps_is_refused() {
    let p = base().raw("").step(9, 300, 0, "Other", true, &[]);
    // A chart of its own, with its one initial step: taken.
    lowers(&[p]);
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL")])
        .step(1, 100, 0, "A", true, &["4"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "B", true, &["2"])
        .trans(4, 100, 120, "go", &["3"]);
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "the chart of the steps A, B has 2 initial steps (A, B); each chart has exactly one (IEC 61131-3 ed. 3 §6.7.2)"
    );
}

#[test]
fn two_steps_linked_without_a_transition_between_them_are_refused() {
    // §6.7.5: two steps shall never be directly linked.
    let p = base().step(5, 300, 80, "Next", false, &["3"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(5));
    assert_eq!(
        msg,
        "step 5 is linked from step 3; two steps are never linked directly: a transition is between them (IEC 61131-3 ed. 3 §6.7.5)"
    );
}

#[test]
fn two_transitions_linked_without_a_step_between_them_are_refused() {
    // §6.7.5: two transitions shall never be directly linked.
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL")])
        .step(1, 100, 0, "A", true, &["5"])
        .trans(2, 100, 40, "go", &["1"])
        .trans(5, 100, 80, "go", &["2"]);
    let d = refused(&[p]);
    assert!(
        d.iter().any(|d| d.message
            == "transition 5 is linked from transition 2; two transitions are never linked directly: a step is between them (IEC 61131-3 ed. 3 §6.7.5)"),
        "{d:#?}"
    );
}

/// Figure 26: A, then B and C at once; C chooses D or E; B and D join to F; then either
/// back to A (a, unsafe) or, with G, a join of F and G (b, unreachable).
fn figure_26(unsafe_: bool) -> Pou {
    let p = Pou::program("Main")
        .sfc()
        .vars(&[
            ("t1", "BOOL"),
            ("t2", "BOOL"),
            ("t3", "BOOL"),
            ("t4", "BOOL"),
            ("t5", "BOOL"),
            ("t6", "BOOL"),
            ("t7", "BOOL"),
        ])
        .step(1, 200, 0, "A", true, &["30"])
        .trans(2, 200, 40, "t1", &["1"])
        .sim_div(3, 200, 60, &["2"])
        .step(4, 100, 80, "B", false, &["3"])
        .step(5, 300, 80, "C", false, &["3"])
        .sel_div(6, 300, 110, &["5"])
        .trans(7, 300, 130, "t2", &["6"])
        .trans(8, 400, 130, "t3", &["6"])
        .step(9, 300, 160, "D", false, &["7"])
        .step(10, 400, 160, "E", false, &["8"])
        .sim_conv(11, 200, 190, &["4", "9"])
        .trans(12, 200, 210, "t4", &["11"])
        .trans(13, 400, 210, "t5", &["10"])
        .step(14, 200, 240, "F", false, &["12"])
        .step(15, 400, 240, "G", false, &["13"]);
    if unsafe_ {
        p.trans(16, 200, 270, "t6", &["14"])
            .trans(17, 400, 270, "t7", &["15"])
            .sel_conv(30, 300, 300, &["16", "17"])
    } else {
        p.sim_conv(18, 300, 270, &["14", "15"])
            .trans(30, 300, 290, "t6", &["18"])
    }
}

#[test]
fn an_unsafe_chart_is_refused_naming_the_situation_and_the_step() {
    // §6.7.5, Figure 26 a): taking E and G back to A while B still waits activates B twice.
    let (id, msg, _) = one(refused(&[figure_26(true)]));
    assert_eq!(id, Some(2));
    assert_eq!(
        msg,
        "the chart is unsafe: where the steps A, B are active, clearing transition 2 activates step B, which is active already (IEC 61131-3 ed. 3 §6.7.5, Figure 26 a)"
    );
}

#[test]
fn an_unreachable_chart_is_refused_naming_the_transition_never_cleared() {
    // §6.7.5, Figure 26 b): F and G are never active together, so t6 never clears.
    let (id, msg, _) = one(refused(&[figure_26(false)]));
    assert_eq!(id, Some(30));
    assert_eq!(
        msg,
        "transition 30 can never be cleared: the steps before it, F, G, are never active together (IEC 61131-3 ed. 3 §6.7.5, Figure 26 b)"
    );
}

#[test]
fn a_step_no_transition_leads_to_is_refused_as_unreachable() {
    let p = base().step(9, 300, 0, "Lost", false, &[]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(9));
    assert!(
        msg.starts_with("the chart of the steps Lost has no initial step"),
        "{msg}"
    );
}

#[test]
fn a_chart_too_large_to_check_is_refused_saying_so() {
    // 18 sequences of 2 steps side by side: 3^18 situations, far past what is explored.
    let mut p = Pou::program("Main")
        .sfc()
        .vars(&[("t", "BOOL")])
        .step(1, 0, 0, "Start", true, &[])
        .trans(2, 0, 20, "t", &["1"]);
    let mut ends = Vec::new();
    let mut div = vec![];
    for k in 0..18u64 {
        let base = 100 + k * 10;
        p = p
            .step(base, (k * 50) as i32, 60, &format!("A{k}"), false, &["3"])
            .trans(base + 1, (k * 50) as i32, 80, "t", &[&base.to_string()])
            .step(
                base + 2,
                (k * 50) as i32,
                100,
                &format!("B{k}"),
                false,
                &[&(base + 1).to_string()],
            );
        ends.push((base + 2).to_string());
        div.push(base);
    }
    p = p.sim_div(3, 0, 40, &["2"]);
    let refs: Vec<&str> = ends.iter().map(String::as_str).collect();
    p = p
        .sim_conv(5, 0, 200, &refs)
        .trans(6, 0, 220, "t", &["5"])
        .jump_step(7, 0, 240, "Start", &["6"]);
    let d = refused(&[p]);
    assert!(
        d.iter().any(|d| d.message.starts_with(&format!(
            "the chart can be in more than {MAX_SITUATIONS} situations"
        ))),
        "{d:#?}"
    );
}

#[test]
fn a_program_writing_a_steps_flag_or_time_is_refused() {
    // Table 54 footnote a: S4.X := 1; S4.T := t#100ms; are errors.
    let p = base().actions_of(
        5,
        200,
        80,
        "3",
        &[(6, "N", None, Act::St("Idle.X := TRUE;\nRun.T := T#100ms;"))],
    );
    let d = refused(&[p]);
    assert_eq!(d.len(), 2, "{d:#?}");
    assert_eq!(
        d[0].message,
        "Idle is step Idle: only the chart writes its flag and time (IEC 61131-3 ed. 3 §6.7.2, Table 54 footnote a)"
    );
    // Where it is: the action of block 5, its body's first line, then its second.
    let e = d[0].element.as_ref().unwrap();
    assert_eq!(e.local_id, Some(6));
    let t = e.text.as_ref().unwrap();
    assert_eq!(
        (t.field.as_str(), t.line, t.col, t.end_col),
        ("body", Some(1), 1, 5)
    );
    let t = d[1].element.as_ref().unwrap().text.as_ref().unwrap();
    assert_eq!((t.line, t.col), (Some(2), 1));
}

#[test]
fn a_variable_that_is_an_action_written_elsewhere_is_refused() {
    // §6.7.4.1: a BOOL variable used as an action is modified only as that action.
    let p = base()
        .actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("lamp"))])
        .actions_of(7, 200, 0, "1", &[(8, "N", None, Act::St("lamp := TRUE;"))]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(8));
    assert_eq!(
        msg,
        "lamp is an action of the chart: only the chart writes it (IEC 61131-3 ed. 3 §6.7.4.1)"
    );
}

#[test]
fn return_in_an_action_is_refused() {
    let p = base().actions_of(5, 200, 80, "3", &[(6, "N", None, Act::St("RETURN;"))]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(6));
    assert!(
        msg.starts_with("RETURN in an action would end the scan of the whole chart"),
        "{msg}"
    );
}

#[test]
fn the_qualifiers_sd_sl_and_p0_are_refused_with_the_reason() {
    for (q, d, reason) in [
        (
            "SD",
            Some("T#1s"),
            "an SD action whose step is left before the delay starts at once",
        ),
        (
            "SL",
            Some("T#1s"),
            "an SL action whose step is left before the limit ends at once",
        ),
        (
            "P0",
            None,
            "a P0 action would run at a cold start whatever its step",
        ),
    ] {
        let p = base().actions_of(5, 200, 80, "3", &[(6, q, d, Act::Name("lamp"))]);
        let (id, msg, _) = one(refused(&[p]));
        assert_eq!(id, Some(6));
        assert!(
            msg.contains(reason) && msg.contains("ADR-013 §2.17"),
            "{q}: {msg}"
        );
    }
}

#[test]
fn a_qualifier_not_in_table_59_is_refused() {
    let p = base().actions_of(
        5,
        200,
        80,
        "3",
        &[(6, "DL", Some("T#1s"), Act::Name("lamp"))],
    );
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "action 6: 'DL' is not an action qualifier of IEC 61131-3 ed. 3 (Table 59: N, R, S, L, D, P, SD, DS, SL, P1, P0)"
    );
}

#[test]
fn a_duration_is_given_exactly_with_a_time_related_qualifier() {
    let p = base().actions_of(5, 200, 80, "3", &[(6, "L", None, Act::Name("lamp"))]);
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "action 6: the qualifier L needs a duration, a TIME (§6.7.4.5)"
    );
    let p = base().actions_of(
        5,
        200,
        80,
        "3",
        &[(6, "N", Some("T#1s"), Act::Name("lamp"))],
    );
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "action 6: the qualifier N has no duration; only L, D, SD, DS and SL have one (§6.7.4.5)"
    );
    // A duration that is not a TIME is the checker's, at the duration.
    let p = base().actions_of(5, 200, 80, "3", &[(6, "D", Some("n"), Act::Name("lamp"))]);
    let (id, _, d) = one(refused(&[p]));
    assert_eq!(id, Some(6));
    assert_eq!(d.element.unwrap().text.unwrap().field, "duration");
}

#[test]
fn two_time_related_associations_of_one_action_active_together_are_refused() {
    // §6.7.4.6 rule d.
    let p = base().actions_of(
        5,
        200,
        80,
        "3",
        &[
            (6, "L", Some("T#1s"), Act::Name("lamp")),
            (7, "D", Some("T#2s"), Act::Name("lamp")),
        ],
    );
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "the action lamp has time-related associations in steps Run and Run, which can be active together; one action has at most one active time-related association (IEC 61131-3 ed. 3 §6.7.4.6 rule d)"
    );
    // In two steps never active together: taken.
    lowers(&[base()
        .actions_of(
            5,
            200,
            80,
            "3",
            &[(6, "L", Some("T#1s"), Act::Name("lamp"))],
        )
        .actions_of(7, 200, 0, "1", &[(8, "D", Some("T#2s"), Act::Name("lamp"))])]);
}

#[test]
fn an_indicator_variable_is_refused_as_deprecated() {
    let p = base().raw("<actionBlock localId=\"5\"><position x=\"200\" y=\"80\"/><connectionPointIn><connection refLocalId=\"3\"/></connectionPointIn><action localId=\"6\" qualifier=\"N\" indicator=\"done\"><relPosition x=\"0\" y=\"0\"/><reference name=\"lamp\"/></action></actionBlock>");
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "action 6: an indicator variable is not taken; its use is deprecated (IEC 61131-3 ed. 3 §6.7.4.4)"
    );
}

#[test]
fn what_a_chart_does_not_read_is_refused_by_name() {
    let macro_step =
        base().raw("<macroStep localId=\"9\" name=\"M\"><position x=\"0\" y=\"300\"/></macroStep>");
    let (id, msg, _) = one(refused(&[macro_step]));
    assert_eq!(id, Some(9));
    assert!(
        msg.starts_with("macroStep 9: a macro step is not an element of IEC 61131-3 ed. 3"),
        "{msg}"
    );
    let vendor =
        base().raw("<vendorElement localId=\"9\"><position x=\"0\" y=\"300\"/></vendorElement>");
    let (id, msg, _) = one(refused(&[vendor]));
    assert_eq!(id, Some(9));
    assert_eq!(
        msg,
        "vendorElement 9: this element is not read in a Sequential Function Chart"
    );
}

/// `base()` with the action `Blink` of `body` (TC6 XML of a `body`'s content) on Run.
fn with_action_body(body: &str) -> Vec<Diagnostic> {
    let p = base().actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("Blink"))]);
    let xml = project(&[p]).replace(
        "<body><SFC>",
        &format!(
            "<actions><action name=\"Blink\"><body>{body}</body></action></actions><body><SFC>"
        ),
    );
    lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err()
}

#[test]
fn an_action_in_il_or_sfc_is_refused_with_the_reason() {
    let d = with_action_body("<IL><xhtml:p>LD go</xhtml:p></IL>");
    assert!(
        d.iter().any(|d| d.message == "action Blink is in IL; this compiler has no Instruction List front-end: write the action in ST, LD or FBD"
            && d.element.as_ref().unwrap().action.as_deref() == Some("Blink")),
        "{d:#?}"
    );
    // Table 56 feature 2s: the standard does not say what becomes of the chart's steps when
    // the action stops.
    let d = with_action_body("<SFC></SFC>");
    assert!(
        d.iter().any(|d| d.message.starts_with("action Blink is a chart (SFC); an action that is a chart (IEC 61131-3 ed. 3 Table 56 feature 2s) is not taken: the standard does not say what becomes of its steps when the action's Q goes FALSE")
            && d.message.ends_with("write the chart as a function block with an SFC body and call it from the action")),
        "{d:#?}"
    );
    // In an action block, the same, named by the action's localId.
    let p = base().raw("");
    let xml = project(&[p]).replace(
        "</SFC>",
        "<actionBlock localId=\"5\"><position x=\"200\" y=\"80\"/><connectionPointIn><connection refLocalId=\"3\"/></connectionPointIn><action localId=\"6\" qualifier=\"N\"><relPosition x=\"0\" y=\"0\"/><inline><IL><xhtml:p>LD go</xhtml:p></IL></inline></action></actionBlock></SFC>",
    );
    let d = lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err();
    let (id, msg, _) = one(d);
    assert_eq!(id, Some(6));
    assert!(msg.starts_with("action 6 is in IL"), "{msg}");
}

#[test]
fn a_condition_in_il_or_sfc_is_refused() {
    let p = base().transition("Ready", "go");
    let xml = project(&[p
        .raw("<transition localId=\"9\"><position x=\"300\" y=\"40\"/><connectionPointIn><connection refLocalId=\"1\"/></connectionPointIn><condition><reference name=\"Ready\"/></condition></transition>")])
    .replace("<ST><xhtml:p xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">go</xhtml:p></ST>", "<IL><xhtml:p>LD go</xhtml:p></IL>");
    let d = lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err();
    assert!(
        d.iter().any(|d| d.message == "transition Ready: a condition in IL is not read; this compiler has no Instruction List front-end: write it in ST, LD or FBD"),
        "{d:#?}"
    );
}

/// A rung for a condition: left 30, contact `a` 31, the coil `T` 32, and `more` after.
fn rung(more: impl FnOnce(Pou) -> Pou) -> Pou {
    more(
        Pou::program("_")
            .left(30, 20)
            .contact(31, 100, 20, Contact::No, "go", &["30"])
            .coil(32, 300, 20, Coil::Plain, "T", &["31"]),
    )
}

/// `base()` whose transition out of Idle is the condition `cond`, inline, of the name `T`.
fn condition(cond: &Pou) -> Pou {
    Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("stop", "BOOL"),
            ("lamp", "BOOL"),
            ("n", "INT"),
            ("trig", "R_TRIG"),
        ])
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans_body(2, 100, 40, "T", cond, &["1"])
        .step(3, 100, 80, "Run", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"])
}

#[test]
fn the_condition_rung_lowers() {
    lowers(&[condition(&rung(|p| p))]);
}

#[test]
fn a_condition_network_that_writes_anything_but_its_transitions_name_is_refused() {
    // §6.7.3: it shall be an error if any side effect occurs during the evaluation of a
    // transition condition.
    let p = condition(&rung(|p| p.coil(33, 300, 60, Coil::Plain, "lamp", &["31"])));
    let (id, msg, d) = one(refused(&[p]));
    assert_eq!(id, Some(33));
    assert_eq!(
        msg,
        "coil 33 writes lamp; a condition's network writes nothing but its transition's name (IEC 61131-3 ed. 3 §6.7.3: a side effect is an error)"
    );
    assert_eq!(
        d.element.unwrap().inline,
        Some(2),
        "the transition whose body it is in"
    );
}

#[test]
fn a_condition_network_that_calls_a_function_block_or_senses_an_edge_is_refused() {
    let p = condition(&rung(|p| {
        p.block(
            33,
            200,
            60,
            "R_TRIG",
            Some("trig"),
            &[("CLK", In::From(&["30"]))],
            &["Q"],
        )
        .contact(34, 250, 60, Contact::No, "go", &["33.Q"])
        .coil(35, 300, 60, Coil::Plain, "T", &["34"])
    }));
    let d = refused(&[p]);
    assert!(
        d.iter().any(|d| d.message.starts_with(
            "block 33 calls the function block trig (R_TRIG); a call changes the instance"
        )),
        "{d:#?}"
    );
    let p = condition(
        &Pou::program("_")
            .left(30, 20)
            .contact(31, 100, 20, Contact::Rising, "go", &["30"])
            .coil(32, 300, 20, Coil::Plain, "T", &["31"]),
    );
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(31));
    assert!(
        msg.starts_with("contact 31 senses an edge; it keeps what it last sensed"),
        "{msg}"
    );
}

#[test]
fn a_condition_network_with_no_coil_or_two_coils_of_its_name_is_refused() {
    let none = condition(
        &Pou::program("_")
            .left(30, 20)
            .contact(31, 100, 20, Contact::No, "go", &["30"])
            .right(32, 20, &["31"]),
    );
    let d = refused(&[none]);
    assert!(
        d.iter().any(|d| d.message == "the condition's network writes no T: a coil of the transition's name, T, takes the condition (IEC 61131-3 ed. 3 §6.7.3 g, Table 55 features 10, 11)"),
        "{d:#?}"
    );
    let two = condition(&rung(|p| p.coil(33, 300, 60, Coil::Plain, "T", &["30"])));
    let (id, msg, _) = one(refused(&[two]));
    assert_eq!(id, Some(33));
    assert!(msg.starts_with("a coil 33 writes T too"), "{msg}");
    let set = condition(
        &Pou::program("_")
            .left(30, 20)
            .contact(31, 100, 20, Contact::No, "go", &["30"])
            .coil(32, 300, 20, Coil::Set, "T", &["31"]),
    );
    let (id, msg, _) = one(refused(&[set]));
    assert_eq!(id, Some(32));
    assert!(
        msg.starts_with("coil 32 sets, resets or senses an edge"),
        "{msg}"
    );
}

#[test]
fn a_condition_network_with_a_jump_or_an_element_apart_is_refused() {
    let jump = condition(&rung(|p| {
        p.jump(33, 200, 60, "L", &["31"]).label(34, 100, "L")
    }));
    let d = refused(&[jump]);
    assert!(
        d.iter().any(|d| d.message == "a jump 33: a condition is one network giving one Boolean value (IEC 61131-3 ed. 3 §6.7.3); it has no labels, jumps or returns"),
        "{d:#?}"
    );
    let apart = condition(&rung(|p| {
        p.contact(33, 100, 60, Contact::No, "stop", &["30"])
            .right(34, 60, &["33"])
    }));
    let (id, msg, _) = one(refused(&[apart]));
    assert_eq!(id, Some(33));
    assert_eq!(
        msg,
        "a contact 33 does not lead to the condition; a condition is one network giving one Boolean value (IEC 61131-3 ed. 3 §6.7.3)"
    );
}

#[test]
fn an_inline_condition_in_ld_without_a_name_or_with_a_variables_name_is_refused() {
    let p = condition(&rung(|p| p));
    let xml = project(&[p]).replace("<inline name=\"T\">", "<inline name=\"\">");
    let d = lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err();
    let (id, msg, _) = one(d);
    assert_eq!(id, Some(2));
    assert!(
        msg.starts_with("transition 2: its condition in LD has no name"),
        "{msg}"
    );
    let p = condition(
        &Pou::program("_")
            .left(30, 20)
            .contact(31, 100, 20, Contact::No, "go", &["30"])
            .coil(32, 300, 20, Coil::Plain, "lamp", &["31"]),
    );
    let xml = project(&[p]).replace("<inline name=\"T\">", "<inline name=\"lamp\">");
    let d = lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err();
    let (id, msg, _) = one(d);
    assert_eq!(id, Some(2));
    assert_eq!(
        msg,
        "the condition of transition 2, lamp, has the name of the variable lamp; each step, action and transition of a POU has a name of its own, apart from its variables"
    );
}

#[test]
fn a_transition_of_the_pou_in_ld_named_by_two_transitions_is_refused() {
    // Table 55 feature 9a: a transition name refers to TRANSITION … FROM … TO …, one transition.
    let cond = rung(|p| p);
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL")])
        .transition_body("T", &cond)
        .step(1, 100, 0, "A", true, &["4"])
        .trans_ref(2, 100, 40, "T", &["1"])
        .step(3, 100, 80, "B", false, &["2"])
        .trans_ref(4, 100, 120, "T", &["3"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(4));
    assert!(
        msg.starts_with(
            "transitions 2 and 4 both name the transition T, whose condition is a network of LD"
        ),
        "{msg}"
    );
}

#[test]
fn an_ld_action_that_returns_or_writes_a_steps_flag_is_refused() {
    let ret = Pou::program("_")
        .left(10, 20)
        .contact(11, 100, 20, Contact::No, "go", &["10"])
        .ret(12, 200, 20, &["11"]);
    let p = base().action_body("Blink", &ret).actions_of(
        5,
        200,
        80,
        "3",
        &[(6, "N", None, Act::Name("Blink"))],
    );
    let (id, msg, d) = one(refused(&[p]));
    assert_eq!(id, Some(12));
    assert!(
        msg.starts_with("return 12: a return in an action would end the scan of the whole chart"),
        "{msg}"
    );
    assert_eq!(d.element.unwrap().action.as_deref(), Some("Blink"));
    // Table 54 footnote a: the chart alone writes a step's flag.
    let coil = Pou::program("_")
        .left(10, 20)
        .contact(11, 100, 20, Contact::No, "go", &["10"])
        .coil(12, 200, 20, Coil::Plain, "Idle.X", &["11"]);
    let p = base().actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Body(&coil))]);
    let (id, msg, d) = one(refused(&[p]));
    assert_eq!(id, Some(12));
    assert_eq!(
        msg,
        "Idle is step Idle: only the chart writes its flag and time (IEC 61131-3 ed. 3 §6.7.2, Table 54 footnote a)"
    );
    let e = d.element.unwrap();
    assert_eq!(e.inline, Some(6));
    assert_eq!(e.text.unwrap().field, "variable");
}

#[test]
fn a_network_drawn_in_the_chart_that_leads_to_no_transition_or_to_two_is_refused() {
    let stray = base()
        .left(40, 300)
        .contact(41, 0, 300, Contact::No, "go", &["40"]);
    let (id, msg, _) = one(refused(&[stray]));
    assert_eq!(id, Some(41));
    assert!(
        msg.starts_with(
            "a contact 41 is drawn in the chart and leads to no transition's condition"
        ),
        "{msg}"
    );
    let rail = base().left(40, 300);
    let (id, msg, _) = one(refused(&[rail]));
    assert_eq!(id, Some(40));
    assert!(
        msg.starts_with(
            "the left power rail 40 is drawn in the chart and leads to no transition's condition"
        ),
        "{msg}"
    );
    let two = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL")])
        .left(40, 0)
        .contact(41, 0, 40, Contact::No, "go", &["40"])
        .step(1, 100, 0, "A", true, &["4"])
        .trans_wired(2, 100, 40, &["41"], false, &["1"])
        .step(3, 100, 80, "B", false, &["2"])
        .trans_wired(4, 100, 120, &["41"], true, &["3"]);
    let (id, msg, _) = one(refused(&[two]));
    assert_eq!(id, Some(41));
    assert!(
        msg.starts_with("a contact 41 leads to the conditions of transitions 2 and 4"),
        "{msg}"
    );
}

#[test]
fn a_network_drawn_in_the_chart_connected_to_a_step_is_refused() {
    // Table 54 feature 3b, a connection from the step's flag: not read; S.X is (feature 3a).
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL")])
        .step(1, 100, 0, "A", true, &["4"])
        .contact(41, 0, 40, Contact::No, "go", &["1"])
        .trans_wired(2, 100, 40, &["41"], false, &["1"])
        .step(3, 100, 80, "B", false, &["2"])
        .trans(4, 100, 120, "go", &["3"]);
    let d = refused(&[p]);
    assert!(
        d.iter().any(|d| d.message == "a contact 41 is connected to step 1; a network reads a step's flag by its name, S.X (IEC 61131-3 ed. 3 Table 54 feature 3a): a connection from the step (feature 3b) is not read"),
        "{d:#?}"
    );
}

#[test]
fn a_chart_in_a_function_and_actions_in_another_body_are_refused() {
    // §6.7.1: only a program or a function block is a chart.
    let f = Pou::function("F", "BOOL")
        .sfc()
        .step(1, 0, 0, "S", true, &[]);
    let d = refused(&[f]);
    assert!(
        d.iter()
            .any(|d| d.message.starts_with("the function F has a body in SFC")),
        "{d:#?}"
    );
    let xml = project(&[Pou::program("Main").fbd()]).replace(
        "<body><FBD>",
        "<actions><action name=\"A\"><body><ST><xhtml:p>x := 1;</xhtml:p></ST></body></action></actions><body><FBD>",
    );
    let d = lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err();
    assert_eq!(
        d[0].message,
        "Main declares actions; actions and transitions belong to a POU whose body is a Sequential Function Chart (IEC 61131-3 ed. 3 §6.7)"
    );
}

#[test]
fn a_name_that_is_also_another_names_is_refused() {
    // §6.7.2, §6.7.3, §6.7.4: step, transition and action names are local to the POU.
    let p = base().step(9, 300, 0, "lamp", false, &[]);
    let d = refused(&[p]);
    assert_eq!(
        d[0].message,
        "step lamp has the name of the variable lamp; each step, action and transition of a POU has a name of its own, apart from its variables"
    );
}

#[test]
fn an_action_naming_nothing_or_a_variable_that_is_not_a_bool_is_refused() {
    let p = base().actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("nothing"))]);
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "action 6: nothing names no action of the POU and no BOOL variable"
    );
    let p = base().actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("n"))]);
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "action 6: n is a variable that is not a BOOL of VAR or VAR_OUTPUT; a variable that is an action is one (Table 56 feature 1)"
    );
}

#[test]
fn a_mistake_in_a_named_transition_is_told_at_the_transition_by_its_name() {
    let p = base()
        .transition("Ready", "go AND")
        .raw("<transition localId=\"9\"><position x=\"300\" y=\"40\"/><connectionPointIn><connection refLocalId=\"1\"/></connectionPointIn><condition><reference name=\"Ready\"/></condition></transition>");
    let d = refused(&[p]);
    let e = d[0].element.as_ref().unwrap();
    assert_eq!(e.transition.as_deref(), Some("Ready"), "{d:#?}");
    assert_eq!(e.text.as_ref().unwrap().field, "body");
    // A condition that is not a BOOL is the checker's, told at the condition's text.
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("n", "INT")])
        .step(1, 100, 0, "A", true, &["4"])
        .trans(2, 100, 40, "n + 1", &["1"])
        .step(3, 100, 80, "B", false, &["2"])
        .trans(4, 100, 120, "TRUE", &["3"]);
    let (id, _, d) = one(refused(&[p]));
    assert_eq!(id, Some(2));
    assert_eq!(d.element.unwrap().text.unwrap().field, "condition");
}

#[test]
fn a_diagnostic_in_a_body_of_several_lines_is_json_with_its_line() {
    let p = base()
        .action("Blink", "lamp := NOT lamp;\nn := nope;")
        .actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("Blink"))]);
    let (_, _, d) = one(refused(&[p]));
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["element"]["action"], "Blink", "{json}");
    assert_eq!(json["element"]["text"]["line"], 2, "{json}");
    assert_eq!(json["element"]["text"]["field"], "body");
    assert_eq!(json["element"]["text"]["col"], 6);
}

#[test]
fn a_step_after_a_transition_that_never_clears_is_refused_as_never_active() {
    // §6.7.5, an unreachable chart: Figure 26 b), but t6 leads to a step H of its own (and H
    // back to A by a jump step), so H is never active.
    let p = Pou::program("Main")
        .sfc()
        .vars(&[
            ("t1", "BOOL"),
            ("t2", "BOOL"),
            ("t3", "BOOL"),
            ("t4", "BOOL"),
            ("t5", "BOOL"),
            ("t6", "BOOL"),
            ("t7", "BOOL"),
        ])
        .step(1, 200, 0, "A", true, &[])
        .trans(2, 200, 40, "t1", &["1"])
        .sim_div(3, 200, 60, &["2"])
        .step(4, 100, 80, "B", false, &["3"])
        .step(5, 300, 80, "C", false, &["3"])
        .sel_div(6, 300, 110, &["5"])
        .trans(7, 300, 130, "t2", &["6"])
        .trans(8, 400, 130, "t3", &["6"])
        .step(9, 300, 160, "D", false, &["7"])
        .step(10, 400, 160, "E", false, &["8"])
        .sim_conv(11, 200, 190, &["4", "9"])
        .trans(12, 200, 210, "t4", &["11"])
        .trans(13, 400, 210, "t5", &["10"])
        .step(14, 200, 240, "F", false, &["12"])
        .step(15, 400, 240, "G", false, &["13"])
        .sim_conv(18, 300, 270, &["14", "15"])
        .trans(30, 300, 290, "t6", &["18"])
        .step(31, 300, 320, "H", false, &["30"])
        .trans(32, 300, 350, "t7", &["31"])
        .jump_step(33, 300, 380, "A", &["32"]);
    let d = refused(&[p]);
    let h = d
        .iter()
        .find(|d| d.message.starts_with("step H can never be active"))
        .unwrap_or_else(|| panic!("no diagnostic for step H: {d:#?}"));
    assert_eq!(h.element.as_ref().and_then(|e| e.local_id), Some(31));
    assert_eq!(
        h.message,
        "step H can never be active: no situation the chart reaches activates it (IEC 61131-3 ed. 3 §6.7.5, an unreachable chart)"
    );
}

#[test]
fn two_time_related_associations_in_two_steps_active_together_are_refused() {
    // §6.7.4.6 rule d, across two steps of simultaneous sequences.
    let p = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL"), ("stop", "BOOL"), ("lamp", "BOOL")])
        .step(1, 100, 0, "Idle", true, &[])
        .trans(2, 100, 40, "go", &["1"])
        .sim_div(3, 100, 60, &["2"])
        .step(4, 0, 80, "A", false, &["3"])
        .step(5, 200, 80, "B", false, &["3"])
        .sim_conv(6, 100, 110, &["4", "5"])
        .trans(7, 100, 130, "stop", &["6"])
        .jump_step(8, 100, 160, "Idle", &["7"])
        .actions_of(
            9,
            300,
            80,
            "4",
            &[(10, "L", Some("T#1s"), Act::Name("lamp"))],
        )
        .actions_of(
            11,
            400,
            80,
            "5",
            &[(12, "D", Some("T#2s"), Act::Name("lamp"))],
        );
    let d = refused(&[p]);
    assert!(
        d.iter().any(|d| d.message
            == "the action lamp has time-related associations in steps A and B, which can be active together; one action has at most one active time-related association (IEC 61131-3 ed. 3 §6.7.4.6 rule d)"),
        "{d:#?}"
    );
}

/// From an initial step, simultaneous loops of `sizes` steps each: 1 + the product of the
/// sizes situations.
fn loops(sizes: &[usize]) -> Pou {
    let mut p = Pou::program("Main")
        .sfc()
        .vars(&[("c", "BOOL")])
        .step(1, 0, 0, "I", true, &[])
        .trans(2, 0, 10, "c", &["1"])
        .sim_div(3, 0, 20, &["2"]);
    let mut id = 10u64;
    for (k, &n) in sizes.iter().enumerate() {
        let x = 100 * k as i32;
        let mut prev = "3".to_string();
        for i in 0..n {
            let sid = id;
            id += 1;
            p = p.step(
                sid,
                x,
                100 + 20 * i as i32,
                &format!("L{k}_{i}"),
                false,
                &[&prev],
            );
            let tid = id;
            id += 1;
            p = p.trans(tid, x, 110 + 20 * i as i32, "c", &[&sid.to_string()]);
            prev = tid.to_string();
        }
        let jid = id;
        id += 1;
        p = p.jump_step(jid, x, 9000, &format!("L{k}_0"), &[&prev]);
    }
    p
}

#[test]
fn a_chart_of_exactly_the_most_situations_checked_is_taken() {
    // 1 + 9 × 41 × 271 = 100,000 situations: exactly MAX_SITUATIONS, so checked and taken.
    assert_eq!(MAX_SITUATIONS, 100_000);
    lowers(&[loops(&[9, 41, 271])]);
    // One loop step more: 1 + 9 × 41 × 272 = 100,369, refused.
    let d = refused(&[loops(&[9, 41, 272])]);
    assert!(
        d.iter().any(|d| d.message.starts_with(&format!(
            "the chart can be in more than {MAX_SITUATIONS} situations"
        ))),
        "{d:#?}"
    );
}

/// An FBD condition `T` of the AND of `a` and `b` under the EN `en` (review of PR #163).
fn and_under_en() -> Pou {
    Pou::program("_")
        .fbd()
        .invar(30, 0, 0, "en")
        .invar(31, 0, 40, "go")
        .invar(32, 0, 80, "stop")
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
        .outvar(34, 200, 0, "T", &["33.OUT"])
}

#[test]
fn a_condition_whose_result_is_computed_only_under_an_en_is_refused() {
    // §6.7.3: a condition is the result of evaluating one Boolean expression each time; where
    // EN is FALSE the function does not run (§6.6.1.5) and nothing is computed. Found in review:
    // the transition cleared on the value of an earlier enabling, and a negated one on the
    // initial FALSE of a result never computed.
    let with_en = || condition(&and_under_en()).vars(&[("en", "BOOL")]);
    let why = "takes a value computed only where a function's EN is TRUE";
    let (id, msg, d) = one(refused(&[with_en()]));
    assert_eq!(id, Some(34), "at the output variable that takes it");
    assert!(
        msg.starts_with(&format!("an output variable 34 {why}")),
        "{msg}"
    );
    assert_eq!(d.element.unwrap().inline, Some(2));
    // Negated: refused the same, before any value is taken.
    let xml = project(&[with_en()]).replace("<condition>", "<condition negated=\"true\">");
    let (id, msg, _) = one(lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err());
    assert_eq!(id, Some(34));
    assert!(msg.contains(why), "{msg}");
    // LD, an output variable from the function's OUT.
    let ld = Pou::program("_")
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
                ("IN1", In::Expr("go")),
                ("IN2", In::Expr("stop")),
            ],
            &["ENO", "OUT"],
        )
        .outvar(33, 300, 20, "T", &["32.OUT"]);
    let (id, msg, _) = one(refused(&[condition(&ld).vars(&[("en", "BOOL")])]));
    assert_eq!(id, Some(33));
    assert!(msg.contains(why), "{msg}");
    // A network drawn in the chart: said at the transition, where the chart adds what takes it.
    let drawn = Pou::program("Main")
        .sfc()
        .vars(&[("en", "BOOL"), ("go", "BOOL"), ("stop", "BOOL")])
        .step(1, 100, 0, "Idle", true, &["4"])
        .invar(30, -400, 40, "en")
        .invar(31, -400, 80, "go")
        .invar(32, -400, 120, "stop")
        .block(
            33,
            -300,
            40,
            "AND",
            None,
            &[
                ("EN", In::From(&["30"])),
                ("IN1", In::From(&["31"])),
                ("IN2", In::From(&["32"])),
            ],
            &["ENO", "OUT"],
        )
        .trans_wired(2, 100, 40, &["33.OUT"], false, &["1"])
        .step(3, 100, 80, "Run", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"]);
    let (id, msg, _) = one(refused(&[drawn]));
    assert_eq!(id, Some(2));
    assert!(msg.contains(why), "{msg}");
}

#[test]
fn a_condition_network_with_a_feedback_path_is_refused() {
    // §8.1.5.2 lets FBD draw a line back; in a condition it carries a value from one evaluation
    // to the next, state as an edge's memory is, between evaluations made only while the
    // transition is enabled (found in review: XOR of its own output toggled per evaluation).
    let back = Pou::program("_")
        .fbd()
        .invar(30, 0, 0, "go")
        .block(
            31,
            120,
            0,
            "XOR",
            None,
            &[("IN1", In::From(&["30"])), ("IN2", In::From(&["31"]))],
            &["OUT"],
        )
        .outvar(32, 240, 0, "T", &["31"]);
    let (id, msg, _) = one(refused(&[condition(&back)]));
    assert_eq!(id, Some(31));
    assert!(
        msg.starts_with(
            "a block 31 reads back its own output (a feedback path, IEC 61131-3 ed. 3 §8.1.5.2)"
        ),
        "{msg}"
    );
}
