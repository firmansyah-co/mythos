//! What the Function Block Diagram front-end refuses, and where each diagnostic says it is:
//! the element by its `localId` and position, or the declaration by its name (ADR-013 §2.16).

mod support;

use mythos_logic_graphic::{lower, Source};
use mythos_logic_ir::Diagnostic;
use support::tc6::{project, In, Pou};

fn refused(pous: &[Pou]) -> Vec<Diagnostic> {
    let xml = project(pous);
    match lower(
        &[Source {
            name: "main.xml",
            text: &xml,
        }],
        None,
    ) {
        Ok(_) => panic!("lowered, but should not have:\n{xml}"),
        Err(d) => d,
    }
}

fn lowers(pous: &[Pou]) {
    let xml = project(pous);
    if let Err(d) = lower(
        &[Source {
            name: "main.xml",
            text: &xml,
        }],
        None,
    ) {
        panic!("{d:#?}\n{xml}");
    }
}

/// The one diagnostic, with its element's id.
fn one(d: Vec<Diagnostic>) -> (Option<u64>, String) {
    assert_eq!(d.len(), 1, "one diagnostic expected: {d:#?}");
    let d = d.into_iter().next().unwrap();
    (d.element.as_ref().and_then(|e| e.local_id), d.message)
}

/// A program with a good network, to which each test adds what is wrong.
fn base() -> Pou {
    Pou::program("Main")
        .fbd()
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("q", "BOOL"),
            ("x", "INT"),
            ("s", "INT"),
        ])
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
        .outvar(4, 240, 0, "q", &["3"])
}

#[test]
fn the_base_lowers() {
    lowers(&[base()]);
}

#[test]
fn rails_contacts_and_coils_are_refused_by_name_in_fbd() {
    let d = refused(&[base()
        .left(10, 100)
        .contact(11, 100, 100, support::tc6::Contact::No, "a", &["10"])
        .coil(12, 200, 100, support::tc6::Coil::Plain, "q", &["11"])
        .right(13, 100, &["12"])]);
    let said: Vec<(Option<u64>, &str)> = d
        .iter()
        .map(|d| {
            (
                d.element.as_ref().and_then(|e| e.local_id),
                d.message.as_str(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            (Some(10), "leftPowerRail 10 is an element of a Ladder Diagram; a Function Block Diagram has no power rails, contacts or coils (IEC 61131-3 ed. 3 §8.3): use a variable and the blocks AND, OR, NOT, SR, RS"),
            (Some(11), "contact 11 is an element of a Ladder Diagram; a Function Block Diagram has no power rails, contacts or coils (IEC 61131-3 ed. 3 §8.3): use a variable and the blocks AND, OR, NOT, SR, RS"),
            (Some(12), "coil 12 is an element of a Ladder Diagram; a Function Block Diagram has no power rails, contacts or coils (IEC 61131-3 ed. 3 §8.3): use a variable and the blocks AND, OR, NOT, SR, RS"),
            (Some(13), "rightPowerRail 13 is an element of a Ladder Diagram; a Function Block Diagram has no power rails, contacts or coils (IEC 61131-3 ed. 3 §8.3): use a variable and the blocks AND, OR, NOT, SR, RS"),
        ]
    );
}

#[test]
fn an_element_fbd_does_not_read_is_refused_by_name() {
    let p = base().raw("<actionBlock localId=\"10\"><position x=\"0\" y=\"100\"/></actionBlock>");
    assert_eq!(
        one(refused(&[p])),
        (
            Some(10),
            "actionBlock 10: this element is not read in a Function Block Diagram yet".into()
        )
    );
}

#[test]
fn two_lines_into_one_input_are_refused_there_is_no_wired_or() {
    let p = base().outvar(10, 240, 60, "q", &["1", "2"]);
    assert_eq!(
        one(refused(&[p])),
        (
            Some(10),
            "an output variable 10 has 2 lines into its input; a Function Block Diagram joins no lines (no wired-OR, IEC 61131-3 ed. 3 §8.3.2): join them with an OR block".into()
        )
    );
    let p = base().block(
        10,
        120,
        100,
        "OR",
        None,
        &[("IN1", In::From(&["1", "2"])), ("IN2", In::Expr("FALSE"))],
        &["OUT"],
    );
    assert_eq!(
        one(refused(&[p])).1,
        "a block 10 has 2 lines into its input IN1; a Function Block Diagram joins no lines (no wired-OR, IEC 61131-3 ed. 3 §8.3.2): join them with an OR block"
    );
}

#[test]
fn a_continuation_without_its_connector_is_refused() {
    let p = base()
        .continuation(10, 0, 100, "NOPE")
        .outvar(11, 120, 100, "q", &["10"]);
    assert_eq!(
        one(refused(&[p])),
        (
            Some(10),
            "continuation 10 goes on with the line NOPE, which no connector of the body has".into()
        )
    );
}

#[test]
fn two_connectors_of_one_name_are_refused() {
    let p = base()
        .connector(10, 300, 0, "L", &["3"])
        .connector(11, 300, 40, "L", &["1"]);
    assert_eq!(
        one(refused(&[p])),
        (
            Some(11),
            "connectors 10 and 11 have the name L; a line goes on from one connector of a name"
                .into()
        )
    );
}

#[test]
fn a_connector_named_as_a_variable_the_pou_or_a_label_is_refused() {
    // §8.1.3: it shall be an error if a connector's label is another named element's.
    // Its continuation follows from that, and is not told again.
    let p = base()
        .connector(10, 300, 0, "a", &["3"])
        .continuation(11, 0, 100, "a")
        .outvar(12, 120, 100, "q", &["11"]);
    assert_eq!(
        one(refused(&[p])),
        (
            Some(10),
            "connector 10: A is also the name of a variable of the POU; a connector's name is its own (IEC 61131-3 ed. 3 §8.1.3)".into()
        )
    );
    let p = base().connector(10, 300, 0, "Main", &["3"]);
    assert!(one(refused(&[p])).1.contains("is also the name of the POU"));
    let p = base()
        .label(9, 200, "NEXT")
        .connector(10, 300, 0, "next", &["3"]);
    assert!(one(refused(&[p]))
        .1
        .contains("is also the name of a label of the body"));
}

#[test]
fn a_connector_needs_a_line_and_takes_no_expression() {
    let p = base()
        .raw("<connector localId=\"10\" name=\"L\"><position x=\"300\" y=\"0\"/></connector>");
    assert_eq!(
        one(refused(&[p])),
        (
            Some(10),
            "a connector 10 has no input: connect it to a variable or to a block's output".into()
        )
    );
    let p = base().raw("<connector localId=\"10\" name=\"L\"><position x=\"300\" y=\"0\"/><connectionPointIn><expression>a</expression></connectionPointIn></connector>");
    assert_eq!(
        one(refused(&[p])),
        (
            Some(10),
            "connector 10 has an expression; a connector ends a line from an output".into()
        )
    );
}

#[test]
fn connectors_that_only_continue_one_another_are_refused() {
    let p = base()
        .continuation(10, 0, 100, "LP")
        .connector(11, 120, 100, "LQ", &["10"])
        .continuation(12, 0, 140, "LQ")
        .connector(13, 120, 140, "LP", &["12"])
        .outvar(14, 240, 100, "q", &["10"]);
    let d = refused(&[p]);
    assert!(
        d.iter()
            .all(|d| d.message.contains("only continue one another")),
        "{d:#?}"
    );
    assert!(!d.is_empty());
}

#[test]
fn a_line_drawn_back_from_a_variable_is_not_a_feedback_path() {
    // The loop OR -> in-out q -> OR: the line back runs from the in-out variable, not a block.
    let p = Pou::program("Main")
        .fbd()
        .vars(&[("a", "BOOL"), ("q", "BOOL")])
        .invar(1, 0, 0, "a")
        .block(
            2,
            120,
            0,
            "OR",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::From(&["3"]))],
            &["OUT"],
        )
        .inoutvar(3, 60, 60, "q", &["2"]);
    assert_eq!(
        one(refused(&[p])),
        (
            Some(3),
            "the line from block 2 back to an in-out variable 3 closes a loop; a feedback path runs from a block's output back to a block's input (IEC 61131-3 ed. 3 §8.1.5.2): write the value to a variable and read it".replace("block 2", "a block 2")
        )
    );
}

#[test]
fn a_feedback_path_from_an_output_of_unknown_type_is_refused() {
    // ADD's result is typed only once the loop is checked; the line back needs it first.
    let p = Pou::program("Main")
        .fbd()
        .vars(&[("s", "INT")])
        .block(
            1,
            120,
            0,
            "ADD",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(2, 240, 0, "s", &["1"]);
    assert_eq!(
        one(refused(&[p])),
        (
            Some(1),
            "the output OUT of block 1 (ADD) goes back to block 1 by a feedback path, and its type is known only once the loop is checked: write it to a variable and read the variable in its place (an implicit loop, IEC 61131-3 ed. 3 §8.1.5.2 b)".into()
        )
    );
}

#[test]
fn a_feedback_path_of_known_type_is_taken() {
    // XOR of its own output toggles: the type of XOR's result is BOOL before checking.
    lowers(&[Pou::program("Main")
        .fbd()
        .vars(&[("t", "BOOL"), ("q", "BOOL")])
        .invar(1, 0, 0, "t")
        .block(
            2,
            120,
            0,
            "XOR",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::From(&["2"]))],
            &["OUT"],
        )
        .outvar(3, 240, 0, "q", &["2"])]);
}

#[test]
fn a_loop_with_no_line_drawn_back_is_refused() {
    // Both lines of the loop run to the right: positions that cannot be (equal y, but the
    // elements given a NaN x), so nothing says where the loop starts.
    let p = Pou::program("Main")
        .fbd()
        .vars(&[("q", "BOOL")])
        .raw("<block localId=\"1\" typeName=\"NOT\"><position x=\"NaN\" y=\"0\"/><inputVariables><variable formalParameter=\"IN\"><connectionPointIn><connection refLocalId=\"2\"/></connectionPointIn></variable></inputVariables><inOutVariables/><outputVariables><variable formalParameter=\"OUT\"><connectionPointOut/></variable></outputVariables></block>")
        .raw("<block localId=\"2\" typeName=\"NOT\"><position x=\"NaN\" y=\"40\"/><inputVariables><variable formalParameter=\"IN\"><connectionPointIn><connection refLocalId=\"1\"/></connectionPointIn></variable></inputVariables><inOutVariables/><outputVariables><variable formalParameter=\"OUT\"><connectionPointOut/></variable></outputVariables></block>")
        .outvar(3, 240, 0, "q", &["1"]);
    let (id, msg) = one(refused(&[p]));
    assert_eq!(id, Some(1));
    assert!(
        msg.starts_with("the lines go round in a loop through the elements 1, 2 with none drawn back from right to left"),
        "{msg}"
    );
}

#[test]
fn what_fbd_shares_with_ld_is_refused_as_in_ld() {
    // A function block without an instance, an instance called twice in a network, a jump to
    // no label.
    let p = base().block(
        10,
        120,
        100,
        "TON",
        None,
        &[("IN", In::From(&["1"]))],
        &["Q"],
    );
    assert_eq!(
        one(refused(&[p])).1,
        "block 10 calls the function block type TON without an instance: give it an instanceName"
    );
    let p = base()
        .vars(&[("t", "TON")])
        .block(
            10,
            240,
            100,
            "TON",
            Some("t"),
            &[("IN", In::From(&["1"])), ("PT", In::Expr("T#1s"))],
            &["Q"],
        )
        .block(
            11,
            240,
            200,
            "TON",
            Some("t"),
            &[("IN", In::From(&["2"])), ("PT", In::Expr("T#1s"))],
            &["Q"],
        );
    assert_eq!(
        one(refused(&[p])).1,
        "blocks 10 and 11 of one network both call the instance t; a network calls an instance once"
    );
    let p = base().jump(10, 360, 0, "NOWHERE", &["3"]);
    assert_eq!(
        one(refused(&[p])).1,
        "jump 10 goes to the label NOWHERE, which the body does not have"
    );
}

#[test]
fn a_type_error_is_told_at_the_element_by_the_checker() {
    // ADD of two INTs into a BOOL output variable.
    let p = Pou::program("Main")
        .fbd()
        .vars(&[("x", "INT"), ("q", "BOOL")])
        .invar(1, 0, 0, "x")
        .block(
            2,
            120,
            0,
            "ADD",
            None,
            &[("IN1", In::From(&["1"])), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(3, 240, 0, "q", &["2"]);
    let (id, msg) = one(refused(&[p]));
    assert_eq!(id, Some(3), "{msg}");
}
