//! What the Sequential Function Chart front-end refuses, and where each diagnostic says it is
//! (ADR-013 §2.17): the element by its `localId` and position, the action or transition of the
//! POU by its name, and the characters of a text, its line too when the text has several.

mod support;

use mythos_logic_graphic::{lower, Source, MAX_SITUATIONS};
use mythos_logic_ir::Diagnostic;
use support::tc6::{project, Act, Pou};

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
fn what_a_chart_does_not_read_yet_is_refused_by_name() {
    let macro_step =
        base().raw("<macroStep localId=\"9\" name=\"M\"><position x=\"0\" y=\"300\"/></macroStep>");
    let (id, msg, _) = one(refused(&[macro_step]));
    assert_eq!(id, Some(9));
    assert!(
        msg.starts_with("macroStep 9: a macro step is not an element of IEC 61131-3 ed. 3"),
        "{msg}"
    );

    let block = base().raw("<inVariable localId=\"9\"><position x=\"0\" y=\"300\"/><connectionPointOut/><expression>go</expression></inVariable>");
    let (_, msg, _) = one(refused(&[block]));
    assert!(
        msg.starts_with(
            "inVariable 9: this element is not read in a Sequential Function Chart yet"
        ),
        "{msg}"
    );

    let network = base().raw("<transition localId=\"9\"><position x=\"0\" y=\"300\"/><connectionPointIn><connection refLocalId=\"3\"/></connectionPointIn><condition><connectionPointIn><connection refLocalId=\"1\"/></connectionPointIn></condition></transition>");
    let (_, msg, _) = one(refused(&[network]));
    assert!(
        msg.contains("a condition drawn as a network connected to the transition is not read yet"),
        "{msg}"
    );

    let ld_action =
        base()
            .raw("")
            .actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("Blink"))]);
    let xml = project(&[ld_action]).replace(
        "<body><SFC>",
        "<actions><action name=\"Blink\"><body><LD></LD></body></action></actions><body><SFC>",
    );
    let d = lower(
        &[Source {
            name: "m.xml",
            text: &xml,
        }],
        None,
    )
    .unwrap_err();
    assert!(
        d.iter().any(|d| d.message == "action Blink is in LD; an action in a Sequential Function Chart is read in ST only, yet"
            && d.element.as_ref().unwrap().action.as_deref() == Some("Blink")),
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
