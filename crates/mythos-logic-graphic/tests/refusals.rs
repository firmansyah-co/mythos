//! What the Ladder Diagram front-end refuses, and where each diagnostic says it is: the
//! element by its `localId` and position, or the declaration by its name, and the characters
//! of its text (ADR-013 §2.15).

mod support;

use mythos_logic_graphic::{lower, Source};
use mythos_logic_ir::Diagnostic;
use support::tc6::{project, Coil, Contact, In, Pou};

fn refused(pous: &[Pou]) -> Vec<Diagnostic> {
    refused_xml(&project(pous))
}

fn refused_xml(xml: &str) -> Vec<Diagnostic> {
    match lower(
        &[Source {
            name: "main.xml",
            text: xml,
        }],
        None,
    ) {
        Ok(_) => panic!("lowered, but should not have:\n{xml}"),
        Err(d) => d,
    }
}

/// The one diagnostic, with its element's id.
fn one(d: Vec<Diagnostic>) -> (Option<u64>, String, Diagnostic) {
    assert_eq!(d.len(), 1, "one diagnostic expected: {d:#?}");
    let d = d.into_iter().next().unwrap();
    let id = d.element.as_ref().and_then(|e| e.local_id);
    (id, d.message.clone(), d)
}

/// A program with a good rung, to which each test adds what is wrong.
fn base() -> Pou {
    Pou::program("Main")
        .vars(&[("a", "BOOL"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "a", &["1"])
        .coil(3, 200, 20, Coil::Plain, "q", &["2"])
        .right(4, 0, &["3"])
}

#[test]
fn a_dangling_connection_is_refused_naming_its_element_and_the_missing_id() {
    let (id, msg, d) = one(refused(&[base().coil(
        10,
        200,
        60,
        Coil::Plain,
        "q",
        &["77"],
    )]));
    assert_eq!(id, Some(10));
    assert_eq!(
        msg,
        "a coil 10 is connected to localId 77, which no element of the body has"
    );
    let e = d.element.unwrap();
    assert_eq!(
        (e.pou.as_str(), e.x, e.y),
        ("Main", Some(200.0), Some(60.0))
    );
    assert!(d.line > 0, "and where its tag is in the document");
}

#[test]
fn a_connection_from_an_element_without_an_output_is_refused() {
    let p = base()
        .outvar(10, 300, 60, "q", &["2"])
        .coil(11, 400, 60, Coil::Plain, "q", &["10"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(11));
    assert_eq!(
        msg,
        "a coil 11 is connected to an output variable 10, which gives no output"
    );
}

#[test]
fn a_connection_to_an_output_a_block_does_not_show_is_refused() {
    let p = base()
        .section("localVars", &[("t", "TON", None)])
        .block(
            10,
            100,
            100,
            "TON",
            Some("t"),
            &[("IN", In::From(&["1"]))],
            &["Q"],
        )
        .coil(11, 300, 100, Coil::Plain, "q", &["10.ET"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(11));
    assert_eq!(
        msg,
        "a coil 11 is connected to the output ET of block 10, which the block does not show"
    );
}

#[test]
fn a_coil_with_no_input_is_refused() {
    let p =
        base().raw("<coil localId=\"10\"><position x=\"5\" y=\"7\"/><variable>q</variable></coil>");
    let (id, msg, d) = one(refused(&[p]));
    assert_eq!(id, Some(10));
    assert_eq!(
        msg,
        "a coil 10 has no input: connect it to the left power rail or to an element before it"
    );
    assert_eq!(d.element.unwrap().y, Some(7.0));
}

#[test]
fn each_element_needing_an_input_is_refused_without_one() {
    for (xml, what) in [
        ("<contact localId=\"10\"><position x=\"0\" y=\"0\"/><variable>a</variable></contact>", "a contact 10"),
        ("<outVariable localId=\"10\"><position x=\"0\" y=\"0\"/><expression>q</expression></outVariable>", "an output variable 10"),
        ("<jump localId=\"10\" label=\"L\"><position x=\"0\" y=\"0\"/></jump>", "a jump 10"),
        ("<return localId=\"10\"><position x=\"0\" y=\"0\"/></return>", "a return 10"),
    ] {
        let (_, msg, _) = one(refused(&[base().raw(xml)]));
        assert!(msg.starts_with(&format!("{what} has no input")), "{msg}");
    }
}

#[test]
fn two_elements_with_one_local_id_are_refused() {
    let (id, msg, _) = one(refused(&[base().contact(
        2,
        100,
        60,
        Contact::No,
        "a",
        &["1"],
    )]));
    assert_eq!(id, Some(2));
    assert_eq!(
        msg,
        "two elements have the localId 2: a contact and a contact; each element has its own"
    );
}

#[test]
fn a_cycle_of_connections_is_refused_naming_the_elements_on_it() {
    // 10 takes from 11, 11 from 10: a loop by wire, with no feedback variable.
    let p = base()
        .contact(10, 100, 100, Contact::No, "a", &["1", "11"])
        .contact(11, 200, 100, Contact::No, "a", &["10"]);
    let (id, msg, _) = one(refused(&[p]));
    assert!(id == Some(10) || id == Some(11), "{id:?}");
    assert!(
        msg.starts_with("the connections go round in a cycle through the elements "),
        "{msg}"
    );
    assert!(msg.contains("10") && msg.contains("11"), "{msg}");
    assert!(msg.contains("§8.1.5.2"), "{msg}");
}

#[test]
fn a_cycle_through_a_variable_is_feedback_and_is_taken() {
    // The implicit loop of §8.1.5.2 b): a seal-in through the variable run, not a wire.
    let p = Pou::program("Main")
        .vars(&[("start", "BOOL"), ("stop", "BOOL"), ("run", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "start", &["1"])
        .contact(3, 100, 60, Contact::No, "run", &["1"])
        .contact(4, 200, 20, Contact::Nc, "stop", &["2", "3"])
        .coil(5, 300, 20, Coil::Plain, "run", &["4"])
        .right(6, 0, &["5"]);
    lower(
        &[Source {
            name: "x",
            text: &project(&[p]),
        }],
        None,
    )
    .expect("a seal-in compiles");
}

#[test]
fn an_undeclared_variable_is_told_at_the_characters_of_the_contacts_text() {
    let p = base()
        .contact(10, 100, 60, Contact::No, "s.missing", &["1"])
        .coil(11, 200, 60, Coil::Plain, "q", &["10"]);
    let (id, msg, d) = one(refused(&[p]));
    assert_eq!(id, Some(10));
    assert!(msg.contains("s"), "{msg}");
    let t = d.element.unwrap().text.expect("about its text");
    assert_eq!(t.field, "variable");
    assert_eq!(
        (t.col, t.end_col),
        (1, 2),
        "the name s, the first character"
    );
}

#[test]
fn text_that_does_not_read_is_told_where_it_stops() {
    let p = base().contact(10, 100, 60, Contact::No, "a +", &["1"]);
    let (id, msg, d) = one(refused(&[p]));
    assert_eq!(id, Some(10));
    assert!(msg.starts_with("expected"), "{msg}");
    assert_eq!(d.element.unwrap().text.unwrap().field, "variable");
}

#[test]
fn a_coil_on_what_is_not_a_variable_is_refused() {
    let p = base().coil(10, 200, 60, Coil::Plain, "a AND q", &["2"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(10));
    assert_eq!(
        msg,
        "the variable of coil 10 names a variable to write; this is not one"
    );
}

#[test]
fn a_coil_on_an_input_of_the_program_is_refused_by_the_checker_at_the_coil() {
    let p = Pou::program("Main")
        .inputs(&[("x", "BOOL")])
        .left(1, 0)
        .coil(2, 100, 20, Coil::Plain, "x", &["1"])
        .right(3, 0, &["2"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(2));
    assert!(msg.contains("input"), "{msg}");
}

#[test]
fn an_element_of_another_language_or_not_read_yet_is_refused_by_name() {
    let p =
        base().raw("<connector localId=\"10\" name=\"C\"><position x=\"0\" y=\"0\"/></connector>");
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(10));
    assert_eq!(
        msg,
        "connector 10: this element is not read in a Ladder Diagram yet"
    );
}

#[test]
fn contradictory_attributes_of_a_contact_and_a_coil_are_refused() {
    let p = base().raw("<contact localId=\"10\" negated=\"true\" edge=\"rising\"><position x=\"0\" y=\"0\"/>
        <connectionPointIn><connection refLocalId=\"1\"/></connectionPointIn><variable>a</variable></contact>");
    let (_, msg, _) = one(refused(&[p]));
    assert!(msg.contains("Table 75 has no such contact"), "{msg}");
    let p = base().raw("<coil localId=\"10\" storage=\"set\" edge=\"rising\"><position x=\"0\" y=\"0\"/>
        <connectionPointIn><connection refLocalId=\"1\"/></connectionPointIn><variable>q</variable></coil>");
    let (_, msg, _) = one(refused(&[p]));
    assert!(msg.contains("Table 76 has a coil of each"), "{msg}");
}

#[test]
fn an_edge_in_a_function_is_refused() {
    let f = Pou::function("F", "BOOL")
        .inputs(&[("x", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::Rising, "x", &["1"])
        .coil(3, 200, 20, Coil::Plain, "F", &["2"])
        .right(4, 0, &["3"]);
    let (id, msg, _) = one(refused(&[base(), f]));
    assert_eq!(id, Some(2));
    assert!(msg.contains("a function keeps nothing"), "{msg}");
}

#[test]
fn blocks_are_refused_where_they_cannot_be_called() {
    let ton = |p: Pou| p.section("localVars", &[("t", "TON", None), ("c", "CTU", None)]);
    let (id, msg, _) = one(refused(&[ton(base()).block(
        10,
        0,
        100,
        "TON",
        None,
        &[("IN", In::From(&["1"]))],
        &["Q"],
    )]));
    assert_eq!(id, Some(10));
    assert_eq!(
        msg,
        "block 10 calls the function block type TON without an instance: give it an instanceName"
    );

    let (_, msg, _) = one(refused(&[ton(base()).block(
        10,
        0,
        100,
        "TON",
        Some("c"),
        &[("IN", In::From(&["1"]))],
        &["Q"],
    )]));
    assert_eq!(msg, "block 10 is a TON, but its instance c is a CTU");

    let (_, msg, _) = one(refused(&[ton(base()).block(
        10,
        0,
        100,
        "ADD",
        Some("t"),
        &[],
        &["OUT"],
    )]));
    assert_eq!(
        msg,
        "block 10 has the instance t but ADD is not a function block type"
    );

    let twice = ton(base())
        .block(
            10,
            0,
            100,
            "TON",
            Some("t"),
            &[("IN", In::From(&["1"]))],
            &["Q"],
        )
        .block(
            11,
            0,
            140,
            "TON",
            Some("t"),
            &[("IN", In::From(&["10.Q"]))],
            &["Q"],
        );
    let (id, msg, _) = one(refused(&[twice]));
    assert_eq!(id, Some(11));
    assert_eq!(msg, "blocks 10 and 11 of one network both call the instance t; a network calls an instance once");
}

#[test]
fn a_function_result_of_unknown_type_feeding_two_inputs_is_refused() {
    let p = base()
        .vars(&[("n", "INT"), ("m", "INT")])
        .block(
            10,
            100,
            100,
            "ADD",
            None,
            &[("IN1", In::Expr("n")), ("IN2", In::Expr("1"))],
            &["OUT"],
        )
        .outvar(11, 200, 100, "n", &["10"])
        .outvar(12, 200, 140, "m", &["10"]);
    let (id, msg, _) = one(refused(&[p]));
    assert_eq!(id, Some(10));
    assert!(
        msg.starts_with(
            "the output OUT of block 10 (ADD) feeds 2 inputs, and its type is not known here"
        ),
        "{msg}"
    );
}

#[test]
fn a_jump_to_a_missing_label_and_two_labels_of_one_name_are_refused() {
    let (id, msg, _) = one(refused(&[base().jump(10, 100, 100, "NOWHERE", &["1"])]));
    assert_eq!(id, Some(10));
    assert_eq!(
        msg,
        "jump 10 goes to the label NOWHERE, which the body does not have"
    );
    let (id, msg, _) = one(refused(&[base().label(10, 50, "L").label(11, 60, "L")]));
    assert_eq!(id, Some(11));
    assert_eq!(
        msg,
        "label 11: the body has two labels L; a jump goes to one"
    );
}

#[test]
fn an_element_without_a_local_id_or_a_position_is_refused() {
    let (_, msg, d) =
        one(refused(&[base().raw(
            "<contact><position x=\"0\" y=\"0\"/><variable>a</variable></contact>",
        )]));
    assert_eq!(msg, "contact: it has no localId");
    assert!(d.line > 0);
    let (id, msg, _) =
        one(refused(&[base().raw(
            "<contact localId=\"10\"><variable>a</variable></contact>",
        )]));
    assert_eq!(id, Some(10));
    assert_eq!(msg, "contact 10 has no position (x, y)");
}

#[test]
fn what_an_interface_cannot_declare_is_refused_naming_the_variable() {
    let var = |d: &Diagnostic| d.element.as_ref().and_then(|e| e.variable.clone());
    let p = base().interface_xml(
        "<localVars><variable name=\"__x\"><type><BOOL/></type></variable></localVars>",
    );
    let (_, msg, d) = one(refused(&[p]));
    assert!(msg.contains("is not a name"), "{msg}");
    assert_eq!(var(&d).as_deref(), Some("__x"));

    let p = base().interface_xml(
        "<localVars><variable name=\"step\"><type><BOOL/></type></variable></localVars>",
    );
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(
        msg,
        "step is a keyword of IEC 61131-3 and cannot be used as a name"
    );

    let p = base().interface_xml("<localVars><variable name=\"x\" address=\"%IX0.0\"><type><BOOL/></type></variable></localVars>");
    let (_, msg, _) = one(refused(&[p]));
    assert!(msg.starts_with("AT %I/%Q addresses are not used"), "{msg}");

    let p = base()
        .interface_xml("<tempVars><variable name=\"x\"><type><BOOL/></type></variable></tempVars>");
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(msg, "tempVars (VAR_TEMP) is not supported yet");

    let p = base().interface_xml("<localVars><variable name=\"x\"><type><enum><values><value name=\"A\"/></values></enum></type></variable></localVars>");
    let (_, msg, _) = one(refused(&[p]));
    assert!(
        msg.starts_with("a type declared in place (enum) is not supported"),
        "{msg}"
    );

    let p = base().interface_xml("<localVars retain=\"true\" nonretain=\"true\"><variable name=\"x\"><type><BOOL/></type></variable></localVars>");
    let (_, msg, _) = one(refused(&[p]));
    assert_eq!(msg, "a section is RETAIN or NON_RETAIN, not both");

    let f = Pou::function("F", "BOOL")
        .interface_xml("<inputVars retain=\"true\"><variable name=\"x\"><type><BOOL/></type></variable></inputVars>")
        .left(1, 0)
        .coil(2, 0, 0, Coil::Plain, "F", &["1"]);
    let (_, msg, _) = one(refused(&[base(), f]));
    assert!(
        msg.starts_with("RETAIN and NON_RETAIN are not allowed in a FUNCTION"),
        "{msg}"
    );
}

#[test]
fn an_initial_value_that_does_not_fit_is_told_at_its_text() {
    let p = base().section("localVars", &[("n", "SINT", Some("300"))]);
    let (_, msg, d) = one(refused(&[p]));
    assert!(msg.contains("300"), "{msg}");
    let e = d.element.unwrap();
    assert_eq!(e.variable.as_deref(), Some("n"));
    assert_eq!(e.text.unwrap().field, "initialValue");
}

#[test]
fn a_document_that_is_not_a_tc6_v201_project_is_refused() {
    let d = refused_xml("<project xmlns=\"http://www.plcopen.org/xml/tc6.xsd\"/>");
    assert_eq!(d.len(), 1, "{d:#?}");
    assert!(
        d[0].message
            .starts_with("the document is not a PLCopen TC6 XML v2.01 project"),
        "{}",
        d[0].message
    );
}

#[test]
fn a_document_that_is_not_well_formed_is_told_at_its_line_and_nothing_follows_from_it() {
    let d =
        refused_xml("<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\">\n<types>\n</project>");
    assert_eq!(
        d.len(),
        1,
        "no 'the source has no PROGRAM' after it: {d:#?}"
    );
    assert!(d[0].message.contains("not well-formed"), "{}", d[0].message);
    assert_eq!(d[0].line, 3);
}

#[test]
fn a_body_in_another_language_is_refused() {
    let xml = project(&[]).replace(
        "<pous>",
        "<pous><pou name=\"Main\" pouType=\"program\"><body><ST><xhtml:p>x := 1;</xhtml:p></ST></body></pou>",
    );
    let d = refused_xml(&xml);
    assert!(
        d.iter().any(|d| d.message
            == "Main has a body in ST; this front-end reads LD and FBD (write ST in a .st source)"),
        "{d:#?}"
    );
}

#[test]
fn a_diagnostic_about_an_element_is_json_an_editor_can_place() {
    let p = base()
        .contact(10, 120, 40, Contact::No, "nope", &["1"])
        .coil(11, 200, 40, Coil::Plain, "q", &["10"]);
    let (_, _, d) = one(refused(&[p]));
    let json = serde_json::to_value(&d).unwrap();
    let e = &json["element"];
    assert_eq!(e["pou"], "Main");
    assert_eq!(e["local_id"], 10);
    assert_eq!(
        (e["x"].as_f64(), e["y"].as_f64()),
        (Some(120.0), Some(40.0))
    );
    assert_eq!(e["text"]["field"], "variable");
    assert_eq!(
        (e["text"]["col"].as_u64(), e["text"]["end_col"].as_u64()),
        (Some(1), Some(5))
    );
    assert!(json.get("source").is_none(), "one source: not named");
    // ST's diagnostics keep their shape: no element, no source.
    let st =
        mythos_logic_st::lower("PROGRAM P VAR x : INT; END_VAR x := y; END_PROGRAM").unwrap_err();
    let json = serde_json::to_value(&st[0]).unwrap();
    assert_eq!(
        json.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        ["col", "end_col", "end_line", "line", "message"]
    );
}

#[test]
fn with_several_sources_each_diagnostic_names_its_source() {
    let st = "FUNCTION F : INT\n  VAR_INPUT x : INT; END_VAR\n  F := y;\nEND_FUNCTION";
    let p = base()
        .contact(10, 100, 60, Contact::No, "nope", &["1"])
        .coil(11, 200, 60, Coil::Plain, "q", &["10"]);
    let doc = project(&[p]);
    let d = lower(
        &[Source {
            name: "main.xml",
            text: &doc,
        }],
        Some(&Source {
            name: "lib.st",
            text: st,
        }),
    )
    .unwrap_err();
    assert_eq!(d.len(), 2, "{d:#?}");
    assert_eq!((d[0].source.as_deref(), d[0].line), (Some("lib.st"), 3));
    assert!(d[0].element.is_none());
    assert_eq!(d[1].source.as_deref(), Some("main.xml"));
    assert_eq!(d[1].element.as_ref().unwrap().local_id, Some(10));
}

#[test]
fn a_program_in_ld_with_one_in_st_is_two_programs() {
    let st = "PROGRAM Other VAR x : INT; END_VAR x := 1; END_PROGRAM";
    let doc = project(&[base()]);
    let d = lower(
        &[Source {
            name: "main.xml",
            text: &doc,
        }],
        Some(&Source {
            name: "p.st",
            text: st,
        }),
    )
    .unwrap_err();
    assert_eq!(d.len(), 1, "{d:#?}");
    assert!(d[0].message.contains("second PROGRAM"), "{}", d[0].message);
}

#[test]
fn a_keyword_declared_as_a_name_is_told_once_not_again_where_it_is_used() {
    let p = Pou::program("Main")
        .vars(&[("on", "BOOL"), ("q", "BOOL")])
        .left(1, 0)
        .contact(2, 100, 20, Contact::No, "on", &["1"])
        .coil(3, 200, 20, Coil::Plain, "on", &["2"])
        .right(4, 0, &["3"]);
    let (_, msg, d) = one(refused(&[p]));
    assert_eq!(
        msg,
        "on is a keyword of IEC 61131-3 and cannot be used as a name"
    );
    assert_eq!(d.element.unwrap().variable.as_deref(), Some("on"));
}
