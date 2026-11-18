//! RETAIN and NON_RETAIN (IEC 61131-3 ed. 3 §6.5.6; Table 40 features 4a–5b, Table 47
//! features 4a–5b): which variables the symbol table says keep their value across a warm
//! restart. The runtime that keeps them is not this crate's; what it is told is.

mod support;
use support::*;

use mythos_logic_compiler::{compile, TargetProfile};

/// Every symbol of a program, with whether it is retained.
fn retained(src: &str) -> Vec<(String, bool)> {
    let c = compile(src, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}\n{src}"));
    c.symbols.into_iter().map(|s| (s.path, s.retain)).collect()
}

fn pairs(expected: &[(&str, bool)]) -> Vec<(String, bool)> {
    expected.iter().map(|(p, r)| (p.to_string(), *r)).collect()
}

/// A function block type with a member of each kind of declaration.
const INNER: &str = "
    FUNCTION_BLOCK Inner
    VAR_INPUT x : INT; END_VAR
    VAR RETAIN kept : INT; END_VAR
    VAR NON_RETAIN lost : INT; END_VAR
    VAR plain : INT; t : TON; END_VAR
    END_FUNCTION_BLOCK";

#[test]
fn each_static_section_of_a_program_takes_retain_or_non_retain() {
    assert_eq!(
        retained(
            "PROGRAM Main
             VAR_INPUT RETAIN ri : INT; END_VAR
             VAR_INPUT NON_RETAIN ni : INT; END_VAR
             VAR_INPUT ui : INT; END_VAR
             VAR_OUTPUT RETAIN ro : INT; END_VAR
             VAR_OUTPUT NON_RETAIN no : INT; END_VAR
             VAR_OUTPUT uo : INT; END_VAR
             VAR RETAIN rv : INT; END_VAR
             VAR NON_RETAIN nv : INT; END_VAR
             VAR uv : INT; END_VAR
             END_PROGRAM"
        ),
        pairs(&[
            ("ri", true),
            ("ni", false),
            ("ui", false),
            ("ro", true),
            ("no", false),
            ("uo", false),
            ("rv", true),
            ("nv", false),
            ("uv", false),
        ])
    );
}

#[test]
fn each_static_section_of_a_function_block_takes_retain_or_non_retain() {
    // An instance in an unqualified section: its members follow the type's declarations.
    assert_eq!(
        retained(
            "FUNCTION_BLOCK B
             VAR_INPUT RETAIN ri : INT; END_VAR
             VAR_INPUT NON_RETAIN ni : INT; END_VAR
             VAR_OUTPUT RETAIN ro : INT; END_VAR
             VAR_OUTPUT NON_RETAIN no : INT; END_VAR
             VAR RETAIN rv : INT; END_VAR
             VAR NON_RETAIN nv : INT; END_VAR
             VAR uv : INT; END_VAR
             END_FUNCTION_BLOCK
             PROGRAM Main VAR b : B; END_VAR END_PROGRAM"
        ),
        pairs(&[
            ("b.ri", true),
            ("b.ni", false),
            ("b.ro", true),
            ("b.no", false),
            ("b.rv", true),
            ("b.nv", false),
            ("b.uv", false),
        ])
    );
}

#[test]
fn a_retain_instance_retains_its_members_but_not_one_declared_non_retain_nor_an_instance_inside() {
    assert_eq!(
        retained(&format!(
            "{INNER}
             PROGRAM Main VAR RETAIN r : Inner; END_VAR END_PROGRAM"
        )),
        pairs(&[
            ("r.x", true),
            ("r.kept", true),
            // (a) declared NON_RETAIN in the type: keeps its own.
            ("r.lost", false),
            ("r.plain", true),
            // (b) itself an instance: follows its own type, and TON declares no retention.
            ("r.t.IN", false),
            ("r.t.PT", false),
            ("r.t.Q", false),
            ("r.t.ET", false),
            ("r.t.RUNNING", false),
            ("r.t.START", false),
            ("r.t.CLOCK", false),
        ])
    );
}

#[test]
fn a_non_retain_instance_initializes_its_members_but_keeps_one_declared_retain() {
    assert_eq!(
        retained(&format!(
            "{INNER}
             PROGRAM Main VAR NON_RETAIN n : Inner; END_VAR END_PROGRAM"
        )),
        pairs(&[
            ("n.x", false),
            ("n.kept", true),
            ("n.lost", false),
            ("n.plain", false),
            ("n.t.IN", false),
            ("n.t.PT", false),
            ("n.t.Q", false),
            ("n.t.ET", false),
            ("n.t.RUNNING", false),
            ("n.t.START", false),
            ("n.t.CLOCK", false),
        ])
    );
}

#[test]
fn an_unqualified_instance_follows_what_its_type_declares_retain() {
    assert_eq!(
        retained(&format!(
            "{INNER}
             PROGRAM Main VAR u : Inner; END_VAR END_PROGRAM"
        )),
        pairs(&[
            ("u.x", false),
            ("u.kept", true),
            ("u.lost", false),
            ("u.plain", false),
            ("u.t.IN", false),
            ("u.t.PT", false),
            ("u.t.Q", false),
            ("u.t.ET", false),
            ("u.t.RUNNING", false),
            ("u.t.START", false),
            ("u.t.CLOCK", false),
        ])
    );
}

#[test]
fn an_instance_inside_a_retain_instance_follows_its_own_type_unless_declared_retain_itself() {
    assert_eq!(
        retained(&format!(
            "{INNER}
             FUNCTION_BLOCK Outer
             VAR n : INT; inner : Inner; END_VAR
             VAR RETAIN declared : Inner; END_VAR
             END_FUNCTION_BLOCK
             PROGRAM Main VAR RETAIN o : Outer; END_VAR END_PROGRAM"
        )),
        pairs(&[
            ("o.n", true),
            // (b): `inner` is an instance, so Inner's own declarations apply, as if unqualified.
            ("o.inner.x", false),
            ("o.inner.kept", true),
            ("o.inner.lost", false),
            ("o.inner.plain", false),
            ("o.inner.t.IN", false),
            ("o.inner.t.PT", false),
            ("o.inner.t.Q", false),
            ("o.inner.t.ET", false),
            ("o.inner.t.RUNNING", false),
            ("o.inner.t.START", false),
            ("o.inner.t.CLOCK", false),
            // (a): Outer declares `declared` RETAIN itself, so it is a RETAIN instance.
            ("o.declared.x", true),
            ("o.declared.kept", true),
            ("o.declared.lost", false),
            ("o.declared.plain", true),
            ("o.declared.t.IN", false),
            ("o.declared.t.PT", false),
            ("o.declared.t.Q", false),
            ("o.declared.t.ET", false),
            ("o.declared.t.RUNNING", false),
            ("o.declared.t.START", false),
            ("o.declared.t.CLOCK", false),
        ])
    );
}

#[test]
fn a_standard_block_in_a_retain_section_retains_its_members() {
    // Table 40 feature 5a: VAR RETAIN TMR1 : TON; END_VAR. Its internal state is variables
    // of the block since L4, and takes the qualifier as the rest does.
    assert_eq!(
        retained(
            "PROGRAM Main
             VAR RETAIN tmr : TON; END_VAR
             VAR NON_RETAIN up : CTU; END_VAR
             END_PROGRAM"
        ),
        pairs(&[
            ("tmr.IN", true),
            ("tmr.PT", true),
            ("tmr.Q", true),
            ("tmr.ET", true),
            ("tmr.RUNNING", true),
            ("tmr.START", true),
            ("tmr.CLOCK", true),
            ("up.CU", false),
            ("up.R", false),
            ("up.PV", false),
            ("up.Q", false),
            ("up.CV", false),
            ("up.CU_M", false),
        ])
    );
}

#[test]
fn every_element_of_a_structure_in_a_qualified_section_takes_the_qualifier_nested_ones_too() {
    assert_eq!(
        retained(
            "TYPE
               Pair : STRUCT a : INT; b : REAL; END_STRUCT;
               Nest : STRUCT p : Pair; flag : BOOL; END_STRUCT;
             END_TYPE
             FUNCTION_BLOCK Holder VAR inside : Nest; END_VAR END_FUNCTION_BLOCK
             PROGRAM Main
             VAR RETAIN r : Nest; END_VAR
             VAR NON_RETAIN n : Nest; END_VAR
             VAR u : Nest; END_VAR
             VAR RETAIN h : Holder; END_VAR
             END_PROGRAM"
        ),
        pairs(&[
            ("r.p.a", true),
            ("r.p.b", true),
            ("r.flag", true),
            ("n.p.a", false),
            ("n.p.b", false),
            ("n.flag", false),
            ("u.p.a", false),
            ("u.p.b", false),
            ("u.flag", false),
            // A structure inside a RETAIN instance takes the instance's qualifier.
            ("h.inside.p.a", true),
            ("h.inside.p.b", true),
            ("h.inside.flag", true),
        ])
    );
}

#[test]
fn an_array_is_retained_or_not_as_a_whole() {
    assert_eq!(
        retained(
            "TYPE Pair : STRUCT a : INT; b : REAL; END_STRUCT; END_TYPE
             FUNCTION_BLOCK Plain VAR v : INT; END_VAR END_FUNCTION_BLOCK
             FUNCTION_BLOCK Kept VAR RETAIN v : INT; END_VAR END_FUNCTION_BLOCK
             PROGRAM Main
             VAR RETAIN ints : ARRAY [1..3] OF INT; pairs : ARRAY [0..1] OF Pair;
               timers : ARRAY [1..2] OF TON; plains : ARRAY [1..2] OF Plain; END_VAR
             VAR NON_RETAIN nints : ARRAY [1..3] OF INT; END_VAR
             VAR uints : ARRAY [1..3] OF INT; kepts : ARRAY [1..2] OF Kept; END_VAR
             END_PROGRAM"
        ),
        pairs(&[
            ("ints", true),
            ("pairs", true),
            ("timers", true),
            ("plains", true),
            ("nints", false),
            ("uints", false),
            // Unqualified, but every element's only member is declared RETAIN by its type.
            ("kepts", true),
        ])
    );
}

#[test]
fn an_array_that_would_be_retained_in_part_is_refused_where_it_is_declared() {
    // Each element is an unqualified instance of Inner: `kept` retained, the rest not.
    assert_eq!(
        refusal(&format!(
            "{INNER}
PROGRAM Main VAR many : ARRAY [1..2] OF Inner; END_VAR END_PROGRAM"
        )),
        "8:18: many would be retained in part: some members of its elements are retained across a warm restart and some are not (IEC 61131-3 ed. 3 §6.5.6.2); an array retained in part is not supported yet"
    );
    // Inside an instance: told at the array's declaration in the block, by its path. Holder
    // declares the array RETAIN; each Cell then retains `v`, but its TON follows its own type.
    assert_eq!(
        refusal(
            "FUNCTION_BLOCK Cell VAR v : INT; t : TON; END_VAR END_FUNCTION_BLOCK
FUNCTION_BLOCK Holder VAR RETAIN cells : ARRAY [1..2] OF Cell; END_VAR END_FUNCTION_BLOCK
PROGRAM Main VAR h : Holder; END_VAR END_PROGRAM"
        ),
        "2:34: h.cells would be retained in part: some members of its elements are retained across a warm restart and some are not (IEC 61131-3 ed. 3 §6.5.6.2); an array retained in part is not supported yet"
    );
    // The same array in an unqualified section of Holder: an array of instances is
    // instances, so (b) applies to each element, which follows Cell's own declarations.
    assert_eq!(
        retained(
            "FUNCTION_BLOCK Cell VAR v : INT; t : TON; END_VAR END_FUNCTION_BLOCK
             FUNCTION_BLOCK Holder VAR cells : ARRAY [1..2] OF Cell; END_VAR END_FUNCTION_BLOCK
             PROGRAM Main VAR RETAIN h : Holder; END_VAR END_PROGRAM"
        ),
        pairs(&[("h.cells", false)]),
    );
}

#[test]
fn retain_is_refused_where_nothing_can_be_retained() {
    for (src, said) in [
        (
            "FUNCTION_BLOCK B VAR_IN_OUT RETAIN x : INT; END_VAR END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "1:29: RETAIN is not allowed on VAR_IN_OUT: the variable is the caller's, and has no storage of its own to retain (IEC 61131-3 ed. 3 §6.5.6.2)",
        ),
        (
            "PROGRAM Main VAR_TEMP NON_RETAIN x : INT; END_VAR END_PROGRAM",
            "1:23: NON_RETAIN is not allowed on VAR_TEMP: the variable starts afresh at each call, so there is nothing to retain (IEC 61131-3 ed. 3 Annex A, Temp_Var_Decls)",
        ),
        (
            "PROGRAM Main VAR_EXTERNAL RETAIN x : INT; END_VAR END_PROGRAM",
            "1:27: RETAIN is not allowed on VAR_EXTERNAL: the variable is retained or not where VAR_GLOBAL declares it (IEC 61131-3 ed. 3 Annex A, External_Var_Decls)",
        ),
        (
            "FUNCTION F : INT VAR RETAIN x : INT; END_VAR F := 1; END_FUNCTION PROGRAM Main END_PROGRAM",
            "1:22: RETAIN is not allowed in a FUNCTION: a function keeps nothing from one call to the next, so there is nothing to retain (IEC 61131-3 ed. 3 §6.5.6.1, §6.6.2.1)",
        ),
        (
            "FUNCTION F : INT VAR_INPUT NON_RETAIN x : INT; END_VAR F := x; END_FUNCTION PROGRAM Main END_PROGRAM",
            "1:28: NON_RETAIN is not allowed in a FUNCTION: a function keeps nothing from one call to the next, so there is nothing to retain (IEC 61131-3 ed. 3 §6.5.6.1, §6.6.2.1)",
        ),
        (
            "FUNCTION F : INT VAR_OUTPUT RETAIN x : INT; END_VAR F := 1; END_FUNCTION PROGRAM Main END_PROGRAM",
            "1:29: RETAIN is not allowed in a FUNCTION: a function keeps nothing from one call to the next, so there is nothing to retain (IEC 61131-3 ed. 3 §6.5.6.1, §6.6.2.1)",
        ),
    ] {
        assert_eq!(refusal(src), said, "{src}");
    }
}

#[test]
fn constant_does_not_combine_with_retain_or_non_retain() {
    for (src, said) in [
        (
            "PROGRAM Main VAR RETAIN CONSTANT x : INT := 1; END_VAR END_PROGRAM",
            "1:25: CONSTANT does not combine with RETAIN or NON_RETAIN: a section takes one of the three (IEC 61131-3 ed. 3 Annex A)",
        ),
        (
            "PROGRAM Main VAR CONSTANT NON_RETAIN x : INT := 1; END_VAR END_PROGRAM",
            "1:18: CONSTANT does not combine with RETAIN or NON_RETAIN: a section takes one of the three (IEC 61131-3 ed. 3 Annex A)",
        ),
    ] {
        assert_eq!(refusal(src), said, "{src}");
    }
    // CONSTANT alone is not supported yet, and is still told as such.
    assert!(
        refusal("PROGRAM Main VAR CONSTANT x : INT := 1; END_VAR END_PROGRAM")
            .ends_with("CONSTANT is not supported yet")
    );
}

#[test]
fn a_retained_variable_runs_like_any_other() {
    let mut r = Run::new(
        "PROGRAM Main
         VAR RETAIN n : INT := 5; END_VAR
         VAR NON_RETAIN m : INT := 7; END_VAR
           n := n + 1;
           m := m + 1;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("n"), 6);
    assert_eq!(r.int("m"), 8);
}

/// The symbol table as the runtime reads it: `"retain": true` on a retained symbol, and no
/// `retain` key at all on one that is not. A round trip through the same types cannot catch
/// a wrong key or a value written where none should be, so the JSON is written out here.
#[test]
fn the_symbol_table_says_retain_true_and_says_nothing_otherwise() {
    let c = compile(
        "FUNCTION_BLOCK B VAR RETAIN k : INT; END_VAR VAR p : BOOL; END_VAR END_FUNCTION_BLOCK
         PROGRAM Main
         VAR_INPUT RETAIN set : REAL; END_VAR
         VAR NON_RETAIN count : DINT; END_VAR
         VAR RETAIN hist : ARRAY [1..2] OF INT; END_VAR
         VAR b : B; END_VAR
         END_PROGRAM",
        &TargetProfile::pc(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&c.symbols).unwrap(),
        serde_json::json!([
            {"path": "set", "type": "REAL", "section": "input", "offset": 0, "size": 4,
             "writable": true, "retain": true},
            {"path": "count", "type": "DINT", "section": "var", "offset": 4, "size": 4,
             "writable": false},
            {"path": "hist", "type": "ARRAY [1..2] OF INT", "section": "var", "offset": 8,
             "size": 4, "writable": false,
             "array": {"element_type": "INT", "element_size": 2, "bounds": [[1, 2]]},
             "retain": true},
            {"path": "b.k", "type": "INT", "section": "var", "offset": 12, "size": 2,
             "writable": false, "retain": true},
            {"path": "b.p", "type": "BOOL", "section": "var", "offset": 14, "size": 1,
             "writable": false},
        ])
    );
    // A symbol without the key reads back as not retained.
    let back: Vec<mythos_logic_compiler::Symbol> =
        serde_json::from_value(serde_json::to_value(&c.symbols).unwrap()).unwrap();
    assert_eq!(back, c.symbols);
}
