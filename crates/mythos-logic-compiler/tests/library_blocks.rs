//! The interface of the project library's functions and function blocks, as the compiler
//! checked it (`library_blocks`): what an editor's palette offers and what a box's pins are.

use mythos_logic_compiler::{check_project, library_blocks, LibraryBlock, Sources};

fn pins(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect()
}

fn blocks(library: &[(&str, &str)]) -> Vec<LibraryBlock> {
    library_blocks(&Sources {
        library,
        ..Default::default()
    })
    .unwrap_or_else(|d| panic!("{d:?}"))
}

#[test]
fn a_library_that_checks_answers_each_block_with_its_inputs_and_outputs_in_declaration_order() {
    // Two sources, as the module gives them (sorted by file name); in each, blocks and pins
    // declared in an order that is not their names' order, and sections interleaved.
    let valves = (
        "logic/library/A_Valves.st",
        "FUNCTION_BLOCK Zvalve
  VAR_INPUT open : BOOL; END_VAR
  VAR_OUTPUT position : REAL; END_VAR
  VAR travel : REAL; END_VAR
  VAR_INPUT limit : REAL; label : STRING[12]; END_VAR
  VAR_OUTPUT failed : BOOL; at_end : BOOL; END_VAR
  IF open THEN travel := travel + 1.0; END_IF;
  position := travel;
  failed := travel > limit;
  at_end := position >= limit;
END_FUNCTION_BLOCK
FUNCTION_BLOCK Mvalve
  VAR_INPUT run : BOOL; END_VAR
  VAR_OUTPUT done : BOOL; END_VAR
  done := run;
END_FUNCTION_BLOCK",
    );
    let pumps = (
        "logic/library/B_Pumps.st",
        "FUNCTION Apump : INT
  VAR_INPUT speed : INT; boost : INT; END_VAR
  Apump := speed + boost;
END_FUNCTION",
    );
    assert_eq!(
        blocks(&[valves, pumps]),
        vec![
            LibraryBlock {
                name: "Zvalve".into(),
                function_block: true,
                inputs: pins(&[("open", "BOOL"), ("limit", "REAL"), ("label", "STRING[12]")]),
                outputs: pins(&[("position", "REAL"), ("failed", "BOOL"), ("at_end", "BOOL")]),
                result: None,
            },
            LibraryBlock {
                name: "Mvalve".into(),
                function_block: true,
                inputs: pins(&[("run", "BOOL")]),
                outputs: pins(&[("done", "BOOL")]),
                result: None,
            },
            LibraryBlock {
                name: "Apump".into(),
                function_block: false,
                inputs: pins(&[("speed", "INT"), ("boost", "INT")]),
                outputs: vec![],
                result: Some("INT".into()),
            },
        ]
    );
}

#[test]
fn a_function_answers_its_result_type_and_no_outputs() {
    let scale = (
        "logic/library/Scale.st",
        "FUNCTION Scale : REAL
  VAR_INPUT raw : INT; gain : REAL; END_VAR
  VAR scratch : REAL; END_VAR
  scratch := INT_TO_REAL(raw);
  Scale := scratch * gain;
END_FUNCTION",
    );
    assert_eq!(
        blocks(&[scale]),
        vec![LibraryBlock {
            name: "Scale".into(),
            function_block: false,
            inputs: pins(&[("raw", "INT"), ("gain", "REAL")]),
            outputs: vec![],
            result: Some("REAL".into()),
        }]
    );
}

#[test]
fn a_block_of_the_standard_library_a_library_fb_uses_is_not_answered() {
    let debounce = (
        "logic/library/Debounce.st",
        "FUNCTION_BLOCK Debounce
  VAR_INPUT raw : BOOL; END_VAR
  VAR_OUTPUT steady : BOOL; END_VAR
  VAR wait : TON; edge : R_TRIG; END_VAR
  wait(IN := raw, PT := T#50ms);
  edge(CLK := wait.Q);
  steady := wait.Q;
END_FUNCTION_BLOCK",
    );
    // The checked unit does hold the standard blocks the library block uses: the answer
    // leaves them out, it does not merely lack them.
    let unit = mythos_logic_st::check_project(&mythos_logic_st::Project {
        library: &[debounce],
        data: &[],
    })
    .unwrap();
    for standard in ["TON", "R_TRIG"] {
        assert!(
            unit.pous
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(standard)),
            "{standard} is in the checked unit"
        );
    }
    assert_eq!(
        blocks(&[debounce]),
        vec![LibraryBlock {
            name: "Debounce".into(),
            function_block: true,
            inputs: pins(&[("raw", "BOOL")]),
            outputs: pins(&[("steady", "BOOL")]),
            result: None,
        }]
    );
}

#[test]
fn a_library_that_does_not_check_answers_its_diagnostics_and_no_block() {
    let broken = [
        (
            "logic/library/Good.st",
            "FUNCTION Good : INT VAR_INPUT x : INT; END_VAR Good := x; END_FUNCTION",
        ),
        (
            "logic/library/Bad.st",
            "FUNCTION Bad : INT VAR_INPUT x : INT; END_VAR Bad := y; END_FUNCTION",
        ),
    ];
    let sources = Sources {
        library: &broken,
        ..Default::default()
    };
    let refused = library_blocks(&sources).unwrap_err();
    assert!(!refused.is_empty());
    // Exactly what the check of the project refuses it for.
    assert_eq!(refused, check_project(&sources).unwrap_err());
}

#[test]
fn a_library_of_types_alone_answers_an_empty_list() {
    let types = (
        "data/types/Point.st",
        "TYPE Point : STRUCT x : INT; y : INT; END_STRUCT; END_TYPE",
    );
    assert_eq!(blocks(&[types]), vec![]);
    assert_eq!(blocks(&[]), vec![]);
}
