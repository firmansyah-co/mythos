//! Structures and arrays (IEC 61131-3 ed. 3 §6.4.4.5, §6.4.4.6), compiled and run on the VM.

mod support;
use support::*;

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::{Fault, Value};

#[test]
fn a_structure_has_members_to_any_depth_starting_at_the_values_its_type_gives() {
    let mut r = Run::new(
        "TYPE
           Limits : STRUCT lo : REAL := -1.5; hi : REAL := 1.5; END_STRUCT;
           Channel : STRUCT range : Limits; raw : INT; ok : BOOL := TRUE; gain : LREAL := 2.0; END_STRUCT;
           Card : STRUCT first, second : Channel; slot : SINT := 3; END_STRUCT;
         END_TYPE
         PROGRAM Main VAR card : Card; width : REAL; scaled : LREAL; END_VAR
           card.first.raw := 21;
           card.second.range.hi := 10.0;
           width := card.second.range.hi - card.second.range.lo;
           scaled := INT_TO_LREAL(card.first.raw) * card.first.gain;
         END_PROGRAM",
    );
    assert_eq!(r.get("card.first.range.lo"), Value::Real(-1.5));
    assert_eq!(r.get("card.second.range.hi"), Value::Real(1.5));
    assert!(r.bool("card.second.ok"));
    assert_eq!(r.get("card.slot"), Value::Sint(3));
    r.scan().unwrap();
    assert_eq!(r.get("width"), Value::Real(11.5));
    assert_eq!(r.get("scaled"), Value::Lreal(42.0));
    assert_eq!(
        r.get("card.first.range.hi"),
        Value::Real(1.5),
        "the other channel's is its own"
    );
}

#[test]
fn a_whole_structure_is_copied_by_assignment_and_only_to_its_own_type() {
    let mut r = Run::new(
        "TYPE Point : STRUCT x, y : INT; END_STRUCT;
              Line : STRUCT head, tail : Point; END_STRUCT; END_TYPE
         PROGRAM Main VAR a, b : Point; l, m : Line; END_VAR
           a.x := 3; a.y := 4;
           b := a;
           a.x := 99;
           l.head := b;
           l.tail := a;
           m := l;
           l.tail.y := -1;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(
        (r.int("b.x"), r.int("b.y")),
        (3, 4),
        "a copy: not changed with a"
    );
    assert_eq!(
        (r.int("m.head.x"), r.int("m.tail.x"), r.int("m.tail.y")),
        (3, 99, 4)
    );

    // §6.4.4.6.1: two structured variables are assigned only when they are of one data
    // type. Both here hold two INTs, and are still two types.
    let d = refusal(
        "TYPE Point : STRUCT x, y : INT; END_STRUCT; Size : STRUCT w, h : INT; END_STRUCT; END_TYPE
         PROGRAM Main VAR p : Point; s : Size; END_VAR p := s; END_PROGRAM",
    );
    assert!(d.contains("a Size cannot be used as a Point"), "{d}");
    let d = refusal(
        "TYPE Point : STRUCT x, y : INT; END_STRUCT; END_TYPE
         PROGRAM Main VAR p : Point; i : INT; END_VAR p := i; END_PROGRAM",
    );
    assert!(d.contains("a INT cannot be used as a Point"), "{d}");
}

#[test]
fn an_array_is_read_and_written_by_a_subscript_worked_out_at_run_time() {
    let mut r = Run::new(
        "PROGRAM Main
         VAR v : ARRAY [1..5] OF INT; around : ARRAY [-2..2] OF DINT; i : INT; sum : DINT; big : LINT := 2; s : SINT := -2; END_VAR
           FOR i := 1 TO 5 DO v[i] := i * i; END_FOR;
           sum := 0;
           FOR i := 5 TO 1 BY -1 DO sum := sum + v[i]; END_FOR;
           FOR i := -2 TO 2 DO around[i] := i * 10; END_FOR;
           around[s] := around[s] - 1;
           around[big] := around[big - 1] + around[i - 5];
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("sum"), 55);
    assert_eq!(r.int("v[1]"), 1);
    assert_eq!(r.int("v[5]"), 25);
    assert_eq!(
        r.int("around[-2]"),
        -21,
        "a lower bound below zero, a SINT subscript"
    );
    assert_eq!(r.int("around[0]"), 0);
    assert_eq!(
        r.int("around[2]"),
        10 - 21,
        "a LINT subscript; i is 3 after the loop, so around[i - 5] is around[-2]"
    );
}

#[test]
fn in_an_array_of_several_dimensions_the_rightmost_subscript_varies_fastest() {
    // §6.4.4.5.2 says so of initial values; the host finds elements the same way.
    let mut r = Run::new(
        "PROGRAM Main
         VAR m : ARRAY [1..2, -1..1] OF INT := [10, 11, 12, 20, 21, 22];
             cube : ARRAY [0..1, 0..2, 0..3] OF DINT; i, j, k : INT; picked : INT; corner : DINT; END_VAR
           i := 2; j := 0;
           picked := m[i, j];
           m[i - 1, j + 1] := -7;
           FOR i := 0 TO 1 DO FOR j := 0 TO 2 DO FOR k := 0 TO 3 DO
             cube[i, j, k] := i * 100 + j * 10 + k;
           END_FOR; END_FOR; END_FOR;
           corner := cube[1, 2, 3];
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("picked"), 21);
    let m = r.c.symbol("m").unwrap().clone();
    let raw: Vec<i16> = (0..6)
        .map(|k| {
            let at = (m.offset + k * 2) as usize;
            i16::from_le_bytes([r.memory[at], r.memory[at + 1]])
        })
        .collect();
    assert_eq!(
        raw,
        [10, 11, -7, 20, 21, 22],
        "m[1, 1] is the third element in memory"
    );
    assert_eq!(r.int("corner"), 123);
    assert_eq!(r.int("cube[1, 0, 2]"), 102);
    assert_eq!(r.int("cube[0, 2, 0]"), 20);
}

#[test]
fn arrays_of_structures_structures_holding_arrays_and_arrays_of_arrays() {
    let mut r = Run::new(
        "TYPE
           Reading : STRUCT taken : TIME; values : ARRAY [1..3] OF REAL; valid : BOOL; END_STRUCT;
           Row : ARRAY [1..3] OF INT;
         END_TYPE
         PROGRAM Main
         VAR log : ARRAY [0..2] OF Reading; grid : ARRAY [1..2] OF Row; one : Row; keep : Reading;
             n, c : INT; total : REAL; cell : INT; END_VAR
           FOR n := 0 TO 2 DO
             log[n].taken := T#1s * n;
             log[n].valid := n <> 1;
             FOR c := 1 TO 3 DO log[n].values[c] := INT_TO_REAL(n * 10 + c); END_FOR;
           END_FOR;
           n := 2; c := 3;
           total := log[n].values[c] + log[n - 1].values[c - 2];
           keep := log[n];
           log[0] := log[n];
           one[2] := 5;
           grid[n] := one;
           grid[1][c] := 9;
           cell := grid[2][2] + grid[n - 1][c];
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("total"), Value::Real(23.0 + 11.0));
    assert_eq!(
        r.get("keep.taken"),
        Value::Time(2_000 * MS),
        "an element copied to a variable"
    );
    assert!(r.bool("keep.valid"));
    assert_eq!(
        r.get("log[0].taken"),
        Value::Time(2_000 * MS),
        "an element copied to an element"
    );
    assert_eq!(r.get("log[1].taken"), Value::Time(1_000 * MS));
    assert!(!r.bool("log[1].valid"));
    assert_eq!(
        r.int("cell"),
        14,
        "a row copied whole into an array of rows"
    );
}

#[test]
fn a_whole_array_is_copied_by_assignment_between_variables_of_the_same_type() {
    let mut r = Run::new(
        "TYPE Triple : ARRAY [1..3] OF INT; END_TYPE
         PROGRAM Main
         VAR a : ARRAY [1..3] OF INT := [1, 2, 3]; b : ARRAY [1..3] OF INT; t : Triple;
             m, n : ARRAY [0..1, 0..1] OF BOOL; END_VAR
           b := a;
           a[1] := 100;
           t := b;
           m[1, 0] := TRUE;
           n := m;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!((r.int("b[1]"), r.int("b[2]"), r.int("b[3]")), (1, 2, 3));
    assert_eq!(
        r.int("t[3]"),
        3,
        "a name for the same array type is that type"
    );
    assert!(r.bool("n[1, 0]"));
    assert!(!r.bool("n[0, 1]"));

    for (decl, said) in [
        (
            "b : ARRAY [0..2] OF INT;",
            "a ARRAY [0..2] OF INT cannot be used as a ARRAY [1..3] OF INT",
        ),
        (
            "b : ARRAY [1..3] OF DINT;",
            "a ARRAY [1..3] OF DINT cannot be used as a ARRAY [1..3] OF INT",
        ),
        (
            "b : ARRAY [1..4] OF INT;",
            "a ARRAY [1..4] OF INT cannot be used as a ARRAY [1..3] OF INT",
        ),
    ] {
        let d = refusal(&format!(
            "PROGRAM Main VAR a : ARRAY [1..3] OF INT; {decl} END_VAR a := b; END_PROGRAM"
        ));
        assert!(d.contains(said), "{d}");
    }
}

#[test]
fn an_array_starts_from_its_list_of_values_with_repetitions_written_out() {
    // §6.4.4.5.2: a repetition n(…) stands for its list n times (the section's own example
    // is `a` below), and entries the list does not reach have the type's default.
    let r = Run::new(
        "TYPE Config : STRUCT gains : ARRAY [1..4] OF REAL := [2(0.5), 1.5]; END_STRUCT; END_TYPE
         PROGRAM Main
         VAR a : ARRAY [0..5] OF INT := [2(1, 2, 3)];
             b : ARRAY [1..5] OF INT := [3(0), 7, -1];
             c : ARRAY [1..4] OF DINT := [1, 2];
             d : ARRAY [1..2, 1..2] OF BOOL := [TRUE, FALSE, 2(TRUE)];
             e : ARRAY [1..8] OF INT := [9, 2(1, 2(5)), 4];
             t : ARRAY [1..2] OF TIME := [T#1s, T#250ms];
             cfg : Config;
         END_VAR
         END_PROGRAM",
    );
    let ints = |name: &str, lo: i64, hi: i64| -> Vec<i64> {
        (lo..=hi).map(|i| r.int(&format!("{name}[{i}]"))).collect()
    };
    assert_eq!(ints("a", 0, 5), [1, 2, 3, 1, 2, 3]);
    assert_eq!(ints("b", 1, 5), [0, 0, 0, 7, -1]);
    assert_eq!(ints("c", 1, 4), [1, 2, 0, 0], "the rest is DINT's default");
    assert_eq!(
        ints("e", 1, 8),
        [9, 1, 5, 5, 1, 5, 5, 4],
        "a repetition inside a repetition"
    );
    assert!(r.bool("d[1, 1]") && !r.bool("d[1, 2]") && r.bool("d[2, 1]") && r.bool("d[2, 2]"));
    assert_eq!(r.get("t[2]"), Value::Time(250 * MS));
    assert_eq!(r.get("cfg.gains[2]"), Value::Real(0.5));
    assert_eq!(r.get("cfg.gains[3]"), Value::Real(1.5));
    assert_eq!(r.get("cfg.gains[4]"), Value::Real(0.0));
}

#[test]
fn an_array_of_function_block_instances_gives_each_element_its_own_state() {
    let mut r = Run::new(
        "FUNCTION_BLOCK Tally VAR_INPUT add : INT; END_VAR VAR_OUTPUT sum : INT; END_VAR
           sum := sum + add;
         END_FUNCTION_BLOCK
         PROGRAM Main
         VAR_INPUT run : BOOL; END_VAR
         VAR tallies : ARRAY [1..3] OF Tally; timers : ARRAY [0..1] OF TON; pulses : ARRAY [1..2, 1..2] OF CTU;
             i : INT; got : INT; done0, done1 : BOOL; count : INT; END_VAR
           FOR i := 1 TO 3 DO tallies[i](add := i); END_FOR;
           i := 3;
           tallies[i](add := 100, sum => got);
           timers[0](IN := run, PT := T#30ms, Q => done0);
           timers[i - 2](IN := run, PT := T#60ms);
           done1 := timers[1].Q;
           pulses[2, i - 2](CU := run AND NOT done0, PV := 5);
           count := pulses[2, 1].CV;
         END_PROGRAM",
    );
    r.set("run", Value::Bool(true));
    r.scan_at(0);
    assert_eq!(r.int("tallies[1].sum"), 1);
    assert_eq!(r.int("tallies[2].sum"), 2);
    assert_eq!(r.int("got"), 103);
    assert_eq!(r.int("count"), 1, "one rising edge of CU");
    r.scan_at(40);
    assert_eq!(r.int("tallies[2].sum"), 4);
    assert_eq!(r.int("got"), 206);
    assert!(
        r.bool("done0") && !r.bool("done1"),
        "each timer has its own time"
    );
    assert_eq!(r.get("timers[1].ET"), Value::Time(40 * MS));
    r.scan_at(60);
    assert!(r.bool("done1"));
    assert_eq!(
        r.int("pulses[1, 1].CV"),
        0,
        "the counters never called did not count"
    );
    assert_eq!(r.int("count"), 1);
}

/// A program with one statement that takes its subscript from the input `i`.
fn indexing(statement: &str) -> Run {
    Run::new(&format!(
        "TYPE Pair : STRUCT a, b : INT; END_STRUCT; END_TYPE
         PROGRAM Main
         VAR_INPUT i : INT; j : INT; wide : LINT; END_VAR
         VAR before : DINT := 16#5A5A5A5A;
             v : ARRAY [1..4] OF INT := [1, 2, 3, 4];
             m : ARRAY [1..3, 1..3] OF INT := [11, 12, 13, 21, 22, 23, 31, 32, 33];
             low : ARRAY [-3..-1] OF INT := [-3, -2, -1];
             pairs : ARRAY [1..2] OF Pair;
             after : DINT := 16#5A5A5A5A;
             x : INT; reached : BOOL; END_VAR
           {statement}
           reached := TRUE;
         END_PROGRAM"
    ))
}

#[test]
fn a_subscript_outside_the_declared_bounds_stops_the_scan_with_a_fault() {
    // §6.4.4.5.1: a subscript outside the range the array was declared with is an error,
    // which for a computed subscript can only be found at run time.
    for (statement, i, j) in [
        ("x := v[i];", 0, 0),
        ("x := v[i];", 5, 0),
        ("x := v[i];", -1, 0),
        ("x := v[i];", 32767, 0),
        ("v[i] := 7;", 0, 0),
        ("v[i] := 7;", 5, 0),
        ("v[i + j] := 7;", 3, 2),
        ("x := low[i];", 0, 0),
        ("x := low[i];", -4, 0),
        ("x := low[i];", 1, 0),
        ("pairs[i].b := 7;", 3, 0),
        ("x := pairs[i].a;", 0, 0),
        // Each subscript against its own range, not the position against the whole array:
        // m[1, 4] would be the place of m[2, 1], and m[2, 0] that of m[1, 3].
        ("x := m[i, j];", 1, 4),
        ("x := m[i, j];", 2, 0),
        ("m[i, j] := 7;", 1, 4),
        ("m[i, j] := 7;", 0, 2),
        ("m[i, j] := 7;", 4, 1),
        ("pairs[1] := pairs[i];", 3, 0),
        ("pairs[i] := pairs[2];", 0, 0),
    ] {
        let mut r = indexing(statement);
        r.set("i", Value::Int(i));
        r.set("j", Value::Int(j));
        let inputs = r.memory.clone();
        let result = r.scan();
        assert!(
            matches!(result, Err(Fault::IndexOutOfRange(_))),
            "{statement} with i = {i}, j = {j}: {result:?}"
        );
        assert_eq!(r.memory, inputs, "{statement}: nothing was written");
        assert!(!r.bool("reached"), "the scan stopped there");
    }
    // A LINT subscript is checked as a LINT: 2^32 + 2 is not element 2.
    let mut r = indexing("x := v[wide];");
    r.set("wide", Value::Lint((1 << 32) + 2));
    assert!(matches!(r.scan(), Err(Fault::IndexOutOfRange(_))));
    r.set("wide", Value::Lint(2));
    r.scan().unwrap();
    assert_eq!(r.int("x"), 2);

    // And every subscript inside its bounds is its element, at both ends.
    for (statement, i, j, x) in [
        ("x := v[i];", 1, 0, 1),
        ("x := v[i];", 4, 0, 4),
        ("x := low[i];", -3, 0, -3),
        ("x := low[i];", -1, 0, -1),
        ("x := m[i, j];", 1, 3, 13),
        ("x := m[i, j];", 2, 1, 21),
        ("x := m[i, j];", 3, 3, 33),
    ] {
        let mut r = indexing(statement);
        r.set("i", Value::Int(i));
        r.set("j", Value::Int(j));
        r.scan().unwrap();
        assert_eq!(r.int("x"), x, "{statement} with i = {i}, j = {j}");
        assert!(r.bool("reached"));
    }
}

#[test]
fn a_subscript_out_of_range_inside_a_function_block_or_a_function_is_the_same_fault() {
    let mut r = Run::new(
        "FUNCTION Pick : INT VAR_INPUT v : ARRAY [1..3] OF INT; pos : INT; END_VAR
           Pick := v[pos];
         END_FUNCTION
         FUNCTION_BLOCK Ring VAR_INPUT pos : INT; END_VAR VAR_OUTPUT got : INT; END_VAR
         VAR slots : ARRAY [0..3] OF INT := [5, 6, 7, 8]; END_VAR
           got := slots[pos];
         END_FUNCTION_BLOCK
         PROGRAM Main
         VAR_INPUT a, b : INT; END_VAR
         VAR first : Ring; guard : DINT := 16#7F7F7F7F; second : Ring; data : ARRAY [1..3] OF INT := [1, 2, 3]; x, y : INT; END_VAR
           first(pos := a, got => x);
           second(pos := a);
           y := Pick(data, b);
         END_PROGRAM",
    );
    r.set("a", Value::Int(3));
    r.set("b", Value::Int(3));
    r.scan().unwrap();
    assert_eq!((r.int("x"), r.int("y")), (8, 3));
    // One past the end of first.slots is, in memory, the guard after the instance.
    r.set("a", Value::Int(4));
    assert!(matches!(r.scan(), Err(Fault::IndexOutOfRange(_))));
    r.set("a", Value::Int(-1));
    assert!(matches!(r.scan(), Err(Fault::IndexOutOfRange(_))));
    r.set("a", Value::Int(0));
    r.set("b", Value::Int(0));
    assert!(matches!(r.scan(), Err(Fault::IndexOutOfRange(_))));
    assert_eq!(r.int("guard"), 0x7F7F7F7F);
}

#[test]
fn a_literal_subscript_outside_the_bounds_is_refused_when_compiled() {
    let d = refusal(
        "PROGRAM Main VAR v : ARRAY [1..4] OF INT; x : INT; END_VAR\n  x := v[5];\nEND_PROGRAM",
    );
    assert_eq!(d, "2:10: the subscript 5 is outside 1..4");
    let d = refusal(
        "PROGRAM Main VAR m : ARRAY [0..1, -2..2] OF INT; END_VAR m[1, -3] := 1; END_PROGRAM",
    );
    assert!(d.contains("the subscript -3 is outside -2..2"), "{d}");
    // Inside them, a literal subscript needs no check at run time: it is part of the address.
    let c = compile(
        "PROGRAM Main VAR v : ARRAY [1..4] OF INT; x : INT; END_VAR x := v[4]; END_PROGRAM",
        &TargetProfile::pc(),
    )
    .unwrap();
    assert_eq!(c.code_size, 11, "a load, a store and the end");
}

#[test]
fn what_is_wrong_with_a_subscript_or_a_member_is_said_where_it_is() {
    let main = |body: &str| {
        format!(
            "TYPE P : STRUCT x : INT; END_STRUCT; END_TYPE
             PROGRAM Main VAR v : ARRAY [1..4] OF INT; m : ARRAY [1..2, 1..2] OF INT; p : P; i : INT; r : REAL; END_VAR {body} END_PROGRAM"
        )
    };
    for (body, said) in [
        (
            "i := v[r];",
            "a subscript must be an integer; this is a REAL",
        ),
        (
            "i := v[TRUE];",
            "a subscript must be an integer; this is a BOOL",
        ),
        ("i := m[1];", "m has 2 dimension(s); 1 subscript(s) given"),
        (
            "i := v[1, 2];",
            "v has 1 dimension(s); 2 subscript(s) given",
        ),
        (
            "i := m[1][2];",
            "m has 2 dimension(s); 1 subscript(s) given",
        ),
        ("i := i[1];", "i is a INT, not an array"),
        ("i := p[1];", "p is a P, not an array"),
        (
            "i := v.x;",
            "v is a ARRAY [1..4] OF INT, which has no member x",
        ),
        ("i := p.y;", "P has no member y"),
        ("i := p.x.z;", "p.x is a INT, which has no member z"),
        ("i := v;", "a ARRAY [1..4] OF INT cannot be used as a INT"),
        ("i := p;", "a P cannot be used as a INT"),
        (
            "i := v[1] + p;",
            "a INT and a P cannot be combined without a conversion",
        ),
        (
            "IF p = p THEN i := 1; END_IF;",
            "comparing a whole P is not supported yet; compare its members",
        ),
        (
            "IF v THEN i := 1; END_IF;",
            "a condition must be BOOL; this is a ARRAY [1..4] OF INT",
        ),
        (
            "FOR v[1] := 1 TO 2 DO i := 1; END_FOR;",
            "a FOR variable that is a member or an element",
        ),
        (
            "FOR p.x := 1 TO 2 DO i := 1; END_FOR;",
            "a FOR variable that is a member or an element",
        ),
    ] {
        let d = diagnostics(&main(body));
        assert!(
            d.iter().any(|m| m.contains(said)),
            "{body}\n  wanted '{said}'\n  got {d:?}"
        );
    }
}

#[test]
fn what_types_do_not_support_yet_is_refused_by_name() {
    for (src, said) in [
        (
            "TYPE Colour : (RED, GREEN); END_TYPE PROGRAM Main END_PROGRAM",
            "1:15: an enumeration is not supported yet",
        ),
        (
            "TYPE Level : INT (0..100); END_TYPE PROGRAM Main END_PROGRAM",
            "a type derived from 'INT' is not supported yet; TYPE takes STRUCT, ARRAY and STRING",
        ),
        (
            "TYPE Speed : REAL; END_TYPE PROGRAM Main END_PROGRAM",
            "a type derived from 'REAL' is not supported yet; TYPE takes STRUCT, ARRAY and STRING",
        ),
        (
            "PROGRAM Main VAR x : INT (0..10); END_VAR END_PROGRAM",
            "a subrange (INT (lo..hi)) is not supported yet",
        ),
        (
            "PROGRAM Main VAR s : STRUCT a : INT; END_STRUCT; END_VAR END_PROGRAM",
            "a STRUCT without a name is not supported yet",
        ),
        (
            "TYPE P : STRUCT x : INT; END_STRUCT; END_TYPE PROGRAM Main VAR p : P := (x := 1); END_VAR END_PROGRAM",
            "an initialiser list (name := value, …) for a structure or a function block instance is not supported yet",
        ),
        (
            "TYPE P : STRUCT x : INT; END_STRUCT := (x := 1); END_TYPE PROGRAM Main END_PROGRAM",
            "an initial value on a type is not supported yet",
        ),
        (
            "TYPE P : STRUCT x : INT; END_STRUCT; END_TYPE PROGRAM Main VAR a : ARRAY [1..2] OF P := [(x := 1)]; END_VAR END_PROGRAM",
            "an initialiser list (name := value, …) for a structure is not supported yet",
        ),
        (
            "TYPE P : STRUCT x : INT; END_STRUCT; END_TYPE PROGRAM Main VAR a : ARRAY [1..2] OF P := [1, 2]; END_VAR END_PROGRAM",
            "a list of initial values for an array of P is not supported yet",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..2] OF ARRAY [1..2] OF INT := [[1, 2], [3, 4]]; END_VAR END_PROGRAM",
            "an array initialiser inside another is not supported yet",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..3] OF INT := [1, 2, 3, 4]; END_VAR END_PROGRAM",
            "1:45: the list has more initial values than the array's 3 elements",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..3] OF INT := [1000000000(1)]; END_VAR END_PROGRAM",
            "the list has more initial values than the array's 3 elements",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..3] OF INT := 5; END_VAR END_PROGRAM",
            "an array starts from a list of values: [1, 2, 3] or [3(0)]",
        ),
        (
            "PROGRAM Main VAR a : INT := [1, 2]; END_VAR END_PROGRAM",
            "a INT is not an array: it does not start from a list",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..3] OF INT := [1, 2.5]; END_VAR END_PROGRAM",
            "a INT cannot start as a LREAL",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..3] OF SINT := [1, 300]; END_VAR END_PROGRAM",
            "300 is outside the range of SINT",
        ),
        (
            "PROGRAM Main VAR n : INT; a : ARRAY [1..n] OF INT; END_VAR END_PROGRAM",
            "an array bound must be an integer literal for now, found 'n'",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [*] OF INT; END_VAR END_PROGRAM",
            "an array of variable length (ARRAY [*]) is not supported yet",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [5..1] OF INT; END_VAR END_PROGRAM",
            "the range 5..1 is empty",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [0..3000000000] OF INT; END_VAR END_PROGRAM",
            "the bounds 0..3000000000 do not fit in a DINT",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..2] OF WSTRING; END_VAR END_PROGRAM",
            "'WSTRING' is not supported yet",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..2] OF WORD; END_VAR END_PROGRAM",
            "WORD is not a type this compiler knows",
        ),
        (
            "TYPE P : STRUCT t : TON; END_STRUCT; END_TYPE PROGRAM Main END_PROGRAM",
            "a function block instance inside a structure is not supported yet",
        ),
        (
            "TYPE P : STRUCT END_STRUCT; END_TYPE PROGRAM Main END_PROGRAM",
            "the structure P has no members",
        ),
        (
            "TYPE P : STRUCT x : INT; X : REAL; END_STRUCT; END_TYPE PROGRAM Main END_PROGRAM",
            "X is declared twice",
        ),
        (
            "TYPE P : STRUCT x : INT; END_STRUCT; p : STRUCT y : INT; END_STRUCT; END_TYPE PROGRAM Main END_PROGRAM",
            "p is declared twice",
        ),
        (
            "TYPE A : STRUCT b : B; END_STRUCT; B : STRUCT a : ARRAY [1..2] OF A; END_STRUCT; END_TYPE PROGRAM Main END_PROGRAM",
            "the structure A contains itself: A holds B, which holds A",
        ),
        (
            "TYPE Loop : ARRAY [1..2] OF Loop; END_TYPE PROGRAM Main END_PROGRAM",
            "the type Loop is defined by itself",
        ),
        (
            "FUNCTION_BLOCK B END_FUNCTION_BLOCK PROGRAM Main VAR a, c : ARRAY [1..2] OF B; END_VAR a := c; END_PROGRAM",
            "a holds function block instances; it cannot be assigned as a whole",
        ),
    ] {
        let d = diagnostics(src);
        assert!(
            d.iter().any(|m| m.contains(said)),
            "{src}\n  wanted '{said}'\n  got {d:?}"
        );
    }
}

#[test]
fn the_order_of_a_source_does_not_matter() {
    let parts = [
        "TYPE Grid : ARRAY [1..2] OF Cell; END_TYPE",
        "TYPE Cell : STRUCT level : Level; n : INT; END_STRUCT; Level : STRUCT v : REAL := 1.5; END_STRUCT; END_TYPE",
        "FUNCTION Top : REAL VAR_INPUT g : Grid; END_VAR Top := Bigger(g[1].level.v, g[2].level.v); END_FUNCTION",
        "FUNCTION Bigger : REAL VAR_INPUT a, b : REAL; END_VAR IF a > b THEN Bigger := a; ELSE Bigger := b; END_IF; END_FUNCTION",
        "FUNCTION_BLOCK Keeper VAR_INPUT g : Grid; END_VAR VAR_OUTPUT best : REAL; END_VAR VAR w : Watch; END_VAR
           w(v := Top(g)); best := w.max; END_FUNCTION_BLOCK",
        "FUNCTION_BLOCK Watch VAR_INPUT v : REAL; END_VAR VAR_OUTPUT max : REAL; END_VAR max := Bigger(max, v); END_FUNCTION_BLOCK",
        "PROGRAM Main VAR g : Grid; k : Keeper; best : REAL; END_VAR
           g[2].level.v := g[2].level.v + 1.0; k(g := g, best => best); END_PROGRAM",
    ];
    let forwards = parts.join("\n");
    let backwards = parts.iter().rev().copied().collect::<Vec<_>>().join("\n");
    for src in [forwards, backwards] {
        let mut r = Run::new(&src);
        r.scan().unwrap();
        r.scan().unwrap();
        assert_eq!(r.get("best"), Value::Real(3.5));
    }
}

#[test]
fn an_array_is_one_symbol_with_its_element_type_size_and_bounds() {
    let c = compile(
        "TYPE Point : STRUCT x : INT; y : LREAL; END_STRUCT; END_TYPE
         FUNCTION_BLOCK Cell VAR_INPUT in : BOOL; END_VAR VAR hits : DINT; END_VAR END_FUNCTION_BLOCK
         PROGRAM Main
         VAR_INPUT set : ARRAY [1..3] OF REAL; origin : Point; END_VAR
         VAR m : ARRAY [0..1, -1..1] OF INT; path : ARRAY [1..4] OF Point; cells : ARRAY [1..2] OF Cell; END_VAR
         END_PROGRAM",
        &TargetProfile::pc(),
    )
    .unwrap();
    let json = serde_json::to_value(&c.symbols).unwrap();
    assert_eq!(
        json,
        serde_json::json!([
            {"path": "set", "type": "ARRAY [1..3] OF REAL", "section": "input", "offset": 0, "size": 12, "writable": true,
             "array": {"element_type": "REAL", "element_size": 4, "bounds": [[1, 3]]}},
            {"path": "origin.x", "type": "INT", "section": "input", "offset": 12, "size": 2, "writable": true},
            {"path": "origin.y", "type": "LREAL", "section": "input", "offset": 14, "size": 8, "writable": true},
            {"path": "m", "type": "ARRAY [0..1, -1..1] OF INT", "section": "var", "offset": 22, "size": 12, "writable": false,
             "array": {"element_type": "INT", "element_size": 2, "bounds": [[0, 1], [-1, 1]]}},
            {"path": "path", "type": "ARRAY [1..4] OF Point", "section": "var", "offset": 34, "size": 40, "writable": false,
             "array": {"element_type": "Point", "element_size": 10, "bounds": [[1, 4]]}},
            {"path": "cells", "type": "ARRAY [1..2] OF Cell", "section": "var", "offset": 74, "size": 10, "writable": false,
             "array": {"element_type": "Cell", "element_size": 5, "bounds": [[1, 2]]}},
        ])
    );
    // What an element is made of, for reaching `path[3].y` or `cells[2].hits`.
    let types = serde_json::to_value(&c.types).unwrap();
    assert_eq!(
        types,
        serde_json::json!([
            {"name": "Point", "size": 10, "members": [
                {"name": "x", "type": "INT", "offset": 0, "size": 2},
                {"name": "y", "type": "LREAL", "offset": 2, "size": 8}]},
            {"name": "Cell", "size": 5, "members": [
                {"name": "in", "type": "BOOL", "offset": 0, "size": 1},
                {"name": "hits", "type": "DINT", "offset": 1, "size": 4}]},
        ])
    );
    // And the table reads back: a runner gets it as data.
    let back: Vec<mythos_logic_compiler::Symbol> = serde_json::from_value(json).unwrap();
    assert_eq!(back, c.symbols);
}

#[test]
fn a_target_refuses_a_type_wherever_it_is_used_and_a_program_too_large_for_it() {
    let no_lreal: TargetProfile = serde_json::from_str(
        r#"{"name":"no-lreal","format_version":2,"max_memory":65536,"max_code":65536,"refuses":["Lreal"]}"#,
    )
    .unwrap();
    for src in [
        "TYPE P : STRUCT v : LREAL; END_STRUCT; END_TYPE PROGRAM Main VAR p : P; END_VAR END_PROGRAM",
        "PROGRAM Main VAR a : ARRAY [1..2] OF LREAL; END_VAR END_PROGRAM",
        "FUNCTION F : LREAL F := 1.0; END_FUNCTION PROGRAM Main END_PROGRAM",
        "FUNCTION_BLOCK B VAR v : LREAL; END_VAR END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
    ] {
        let err = compile(src, &no_lreal).err().unwrap();
        assert_eq!(
            err[0].message, "LREAL is not supported on the target no-lreal",
            "{src}"
        );
    }
    let small: TargetProfile = serde_json::from_str(
        r#"{"name":"small","format_version":2,"max_memory":100,"max_code":65536}"#,
    )
    .unwrap();
    let err = compile(
        "PROGRAM Main VAR a : ARRAY [1..50] OF DINT; END_VAR END_PROGRAM",
        &small,
    )
    .err()
    .unwrap();
    assert_eq!(
        err[0].message,
        "the program needs 200 bytes of variable memory; the target small allows 100"
    );
    // An array far larger than any target is refused as quickly, by its size alone.
    let started = std::time::Instant::now();
    let err = compile(
        "TYPE Big : STRUCT a : ARRAY [0..999999] OF LREAL; END_STRUCT; END_TYPE
         PROGRAM Main VAR a : ARRAY [1..200] OF Big; END_VAR END_PROGRAM",
        &TargetProfile::pc(),
    )
    .err()
    .unwrap();
    assert_eq!(
        err[0].message,
        "the program needs 1600000000 bytes of variable memory; the target pc allows 67108864"
    );
    assert!(started.elapsed().as_secs() < 5, "without building 1.6 GB");
    let err = compile(
        "PROGRAM Main VAR a : ARRAY [0..2000000000] OF LINT; END_VAR END_PROGRAM",
        &TargetProfile::pc(),
    )
    .err()
    .unwrap();
    assert_eq!(
        err[0].message,
        "16000000008 bytes of variables is more than the bytecode format can hold"
    );
}
