//! Functions (IEC 61131-3 ed. 3 §6.6.2), compiled and run on the VM: called by position and
//! by name, keeping nothing between calls, and never recursive.

mod support;
use support::*;

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::Value;

#[test]
fn a_function_is_called_by_position_and_by_name_in_any_order() {
    let mut r = Run::new(
        "FUNCTION Scale : REAL
         VAR_INPUT raw : INT; gain : REAL; offset : REAL; END_VAR
           Scale := INT_TO_REAL(raw) * gain + offset;
         END_FUNCTION
         PROGRAM Main VAR a, b, c : REAL; END_VAR
           a := Scale(10, 2.0, 1.0);
           b := Scale(raw := 10, gain := 2.0, offset := 1.0);
           c := Scale(offset := 1.0, raw := 10, gain := 2.0);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("a"), Value::Real(21.0));
    assert_eq!(r.get("b"), Value::Real(21.0));
    assert_eq!(r.get("c"), Value::Real(21.0));
}

#[test]
fn an_input_left_out_of_a_call_by_name_has_its_initial_value_every_time() {
    // IEC 61131-3 ed. 3 §6.6.1.4.2 and Table 20 feature 1b: a variable given no value in a
    // formal call has the initial value of its declaration, or its type's default.
    let mut r = Run::new(
        "FUNCTION Bound : INT
         VAR_INPUT x : INT; lo : INT; hi : INT := 100; END_VAR
           IF x > hi THEN Bound := hi; ELSIF x < lo THEN Bound := lo; ELSE Bound := x; END_IF;
         END_FUNCTION
         PROGRAM Main VAR a, b, c, d : INT; END_VAR
           a := Bound(x := 500);
           b := Bound(x := 500, hi := 20);
           c := Bound(x := 500);
           d := Bound(x := -5);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("a"), 100, "hi starts at 100");
    assert_eq!(r.int("b"), 20);
    assert_eq!(
        r.int("c"),
        100,
        "the 20 given to the call before is not kept: hi is 100 again"
    );
    assert_eq!(r.int("d"), 0, "lo has INT's default, 0");
}

#[test]
fn a_functions_variables_and_result_start_afresh_at_every_call() {
    // §6.6.2.1: a function does not store its inputs, internals or result; Table 19
    // feature 2e: its VAR is temporary, unlike a function block's.
    let mut r = Run::new(
        "FUNCTION Count : INT
         VAR_INPUT inc : INT; END_VAR
         VAR n : INT := 10; seen : BOOL; END_VAR
           IF seen THEN n := 1000; END_IF;
           seen := TRUE;
           n := n + inc;
           Count := n;
         END_FUNCTION
         FUNCTION OnlyWhenPositive : INT
         VAR_INPUT x : INT; END_VAR
           OnlyWhenPositive := 0;
           IF x > 0 THEN OnlyWhenPositive := x; END_IF;
         END_FUNCTION
         FUNCTION Unset : INT
         VAR_INPUT x : INT; END_VAR
           IF x > 0 THEN Unset := x; END_IF;
         END_FUNCTION
         PROGRAM Main VAR a, b, c, p, q, u1, u2 : INT; END_VAR
           a := Count(1);
           b := Count(2);
           c := Count(3);
           p := OnlyWhenPositive(7);
           q := OnlyWhenPositive(-7);
           u1 := Unset(9);
           u2 := Unset(-9);
         END_PROGRAM",
    );
    for scan in 1..=3 {
        r.scan().unwrap();
        assert_eq!(r.int("a"), 11, "scan {scan}");
        assert_eq!(r.int("b"), 12, "scan {scan}: n is 10 again, not 11");
        assert_eq!(r.int("c"), 13, "scan {scan}");
        assert_eq!(r.int("p"), 7);
        assert_eq!(r.int("q"), 0);
        assert_eq!(r.int("u1"), 9);
        assert_eq!(
            r.int("u2"),
            0,
            "scan {scan}: a result not assigned in this call is the type's default, not the 9 of the call before"
        );
    }
}

#[test]
fn a_function_is_used_before_it_is_declared_and_calls_another() {
    let mut r = Run::new(
        "PROGRAM Main VAR a : DINT; END_VAR
           a := Hypot2(3, 4);
         END_PROGRAM
         FUNCTION Hypot2 : DINT
         VAR_INPUT x, y : DINT; END_VAR
           Hypot2 := Square(x) + Square(y);
         END_FUNCTION
         FUNCTION Square : DINT
         VAR_INPUT v : DINT; END_VAR
           Square := v * v;
         END_FUNCTION",
    );
    r.scan().unwrap();
    assert_eq!(r.int("a"), 25);
    assert_eq!(r.c.call_depth, 2, "Main calls Hypot2, which calls Square");
}

#[test]
fn an_argument_may_be_a_call_of_the_same_function() {
    // A function's parameters are one fixed place. Every argument is worked out before any
    // is put there, so the inner calls do not overwrite the outer call's.
    let mut r = Run::new(
        "FUNCTION Diff : DINT
         VAR_INPUT a, b : DINT; END_VAR
         VAR t : DINT; END_VAR
           t := a - b;
           Diff := t;
         END_FUNCTION
         PROGRAM Main VAR x, y, z : DINT; END_VAR
           x := Diff(100, Diff(30, 5));
           y := Diff(Diff(100, 30), 5);
           z := Diff(Diff(50, Diff(9, 4)), Diff(Diff(8, 2), 1));
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("x"), 75, "100 - (30 - 5)");
    assert_eq!(r.int("y"), 65, "(100 - 30) - 5");
    assert_eq!(r.int("z"), 40, "(50 - 5) - (6 - 1)");
    assert_eq!(r.c.call_depth, 1, "one call is in progress at a time");
}

#[test]
fn arguments_widen_as_assignments_do_and_narrowing_needs_a_conversion() {
    let mut r = Run::new(
        "FUNCTION Half : LREAL
         VAR_INPUT v : LREAL; END_VAR
           Half := v / 2.0;
         END_FUNCTION
         PROGRAM Main VAR i : INT := 7; x : LREAL; y : LREAL; END_VAR
           x := Half(i);
           y := Half(5);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(
        r.get("x"),
        Value::Lreal(3.5),
        "an INT widens to LREAL (Figure 11)"
    );
    assert_eq!(r.get("y"), Value::Lreal(2.5));

    let d = refusal(
        "FUNCTION F : INT VAR_INPUT v : INT; END_VAR F := v; END_FUNCTION
         PROGRAM Main VAR d : DINT; r : INT; END_VAR r := F(d); END_PROGRAM",
    );
    assert!(
        d.contains("a DINT cannot be used as a INT without a conversion: write DINT_TO_INT"),
        "{d}"
    );
    let d = refusal(
        "FUNCTION F : DINT VAR_INPUT v : INT; END_VAR F := v; END_FUNCTION
         PROGRAM Main VAR r : INT; END_VAR r := F(1); END_PROGRAM",
    );
    assert!(d.contains("a DINT cannot be used as a INT"), "{d}");
}

#[test]
fn a_function_takes_a_structure_and_an_array_as_its_own_copy() {
    let mut r = Run::new(
        "TYPE Range : STRUCT lo, hi : INT; END_STRUCT; END_TYPE
         FUNCTION Width : INT
         VAR_INPUT r : Range; END_VAR
           Width := r.hi - r.lo;
         END_FUNCTION
         FUNCTION Total : DINT
         VAR_INPUT v : ARRAY [1..4] OF INT; scale : INT; END_VAR
         VAR i : INT; END_VAR
           FOR i := 1 TO 4 DO Total := Total + v[i] * scale; END_FOR;
         END_FUNCTION
         PROGRAM Main
         VAR lim : Range; many : ARRAY [0..1] OF Range; data : ARRAY [1..4] OF INT := [1, 2, 3, 4];
             k : INT := 1; w1, w2 : INT; t1, t2 : DINT; END_VAR
           lim.lo := 3; lim.hi := 10;
           many[1].lo := 20; many[1].hi := 50;
           w1 := Width(lim);
           w2 := Width(r := many[k]);
           t1 := Total(data, 10);
           t2 := Total(scale := Width(many[k]), v := data);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("w1"), 7);
    assert_eq!(
        r.int("w2"),
        30,
        "an element reached by a computed subscript"
    );
    assert_eq!(r.int("t1"), 100);
    assert_eq!(r.int("t2"), 300);
}

#[test]
fn return_leaves_the_function_with_the_result_assigned_so_far() {
    let mut r = Run::new(
        "FUNCTION First : INT
         VAR_INPUT v : ARRAY [1..5] OF INT; wanted : INT; END_VAR
         VAR i : INT; END_VAR
           First := -1;
           FOR i := 1 TO 5 DO
             IF v[i] = wanted THEN First := i; RETURN; END_IF;
           END_FOR;
           First := 0;
         END_FUNCTION
         PROGRAM Main VAR data : ARRAY [1..5] OF INT := [4, 8, 15, 16, 23]; a, b, after : INT; END_VAR
           a := First(data, 15);
           b := First(data, 99);
           after := 1;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("a"), 3);
    assert_eq!(r.int("b"), 0);
    assert_eq!(r.int("after"), 1, "RETURN left the function, not the scan");
}

#[test]
fn recursion_is_refused_at_compile_time_naming_the_cycle() {
    // IEC 61131-3 ed. 3 §6.6.1.1 leaves recursive calls to the implementer. Here they are
    // refused, which is what lets every variable have one place.
    let d = refusal(
        "FUNCTION Fact : DINT\nVAR_INPUT n : DINT; END_VAR\n  IF n <= 1 THEN Fact := 1; ELSE Fact := n * Fact(n - 1); END_IF;\nEND_FUNCTION\nPROGRAM Main VAR x : DINT; END_VAR x := Fact(5); END_PROGRAM",
    );
    assert_eq!(d, "3:46: recursion is not allowed: Fact calls itself");

    let d = refusal(
        "FUNCTION Even : BOOL VAR_INPUT n : INT; END_VAR
           IF n = 0 THEN Even := TRUE; ELSE Even := Odd(n - 1); END_IF;
         END_FUNCTION
         FUNCTION Odd : BOOL VAR_INPUT n : INT; END_VAR
           IF n = 0 THEN Odd := FALSE; ELSE Odd := Even(n - 1); END_IF;
         END_FUNCTION
         PROGRAM Main VAR x : BOOL; END_VAR x := Even(4); END_PROGRAM",
    );
    assert!(
        d.ends_with("recursion is not allowed: Even calls Odd, which calls Even"),
        "{d}"
    );
    assert!(
        d.starts_with("2:53:"),
        "at the call of Odd inside Even: {d}"
    );

    // A cycle of three, none of them called by the program: still refused.
    let d = refusal(
        "FUNCTION A : INT VAR_INPUT n : INT; END_VAR A := B(n); END_FUNCTION
         FUNCTION B : INT VAR_INPUT n : INT; END_VAR B := C(n); END_FUNCTION
         FUNCTION C : INT VAR_INPUT n : INT; END_VAR C := A(n); END_FUNCTION
         PROGRAM Main VAR x : INT; END_VAR x := 1; END_PROGRAM",
    );
    assert!(
        d.ends_with("recursion is not allowed: A calls B, which calls C, which calls A"),
        "{d}"
    );

    // Through an argument is recursion too.
    let d = refusal(
        "FUNCTION F : INT VAR_INPUT n : INT; END_VAR F := G(G(n)); END_FUNCTION
         FUNCTION G : INT VAR_INPUT n : INT; END_VAR G := n + F(0); END_FUNCTION
         PROGRAM Main VAR x : INT; END_VAR x := F(1); END_PROGRAM",
    );
    assert!(
        d.contains("recursion is not allowed: F calls G, which calls F"),
        "{d}"
    );
}

#[test]
fn the_program_declares_how_deep_its_calls_nest_and_runs_in_exactly_that() {
    let depth = |src: &str| compile(src, &TargetProfile::pc()).unwrap().call_depth;
    assert_eq!(
        depth("PROGRAM Main VAR x : INT; END_VAR x := 1; END_PROGRAM"),
        0
    );
    let chain = "FUNCTION F1 : INT VAR_INPUT n : INT; END_VAR F1 := n + 1; END_FUNCTION
                 FUNCTION F2 : INT VAR_INPUT n : INT; END_VAR F2 := F1(n) + F1(n); END_FUNCTION
                 FUNCTION F3 : INT VAR_INPUT n : INT; END_VAR F3 := F2(F1(n)); END_FUNCTION
                 FUNCTION_BLOCK B VAR_OUTPUT o : INT; END_VAR o := F3(o); END_FUNCTION_BLOCK
                 FUNCTION_BLOCK Outer VAR b : B; END_VAR VAR_OUTPUT o : INT; END_VAR b(); o := b.o; END_FUNCTION_BLOCK";
    assert_eq!(
        depth(&format!(
            "{chain} PROGRAM Main VAR x : INT; END_VAR x := F1(1); END_PROGRAM"
        )),
        1,
        "what is declared but not called from the program does not count"
    );
    assert_eq!(
        depth(&format!(
            "{chain} PROGRAM Main VAR x : INT; END_VAR x := F3(1); END_PROGRAM"
        )),
        3
    );
    // Main -> Outer -> B -> F3 -> F2 -> F1. `Run` gives the VM exactly the frames declared.
    let mut r = Run::new(&format!(
        "{chain} PROGRAM Main VAR o : Outer; x : INT; END_VAR o(); x := o.o; END_PROGRAM"
    ));
    assert_eq!(r.c.call_depth, 5);
    r.scan().unwrap();
    assert_eq!(r.int("x"), 4, "F3(0) = F2(1) = 2 + 2");
    r.scan().unwrap();
    assert_eq!(r.int("x"), 12, "F3(4) = F2(5) = 6 + 6");
}

#[test]
fn what_a_function_may_not_be_or_do_is_refused_by_name() {
    let main = "PROGRAM Main VAR x : INT; t : TON; END_VAR";
    for (src, said) in [
        (
            "FUNCTION F : INT VAR_OUTPUT o : INT; END_VAR F := 1; END_FUNCTION PROGRAM Main END_PROGRAM".to_string(),
            "1:18: VAR_OUTPUT on a function is not supported yet",
        ),
        (
            "FUNCTION F : INT VAR_IN_OUT o : INT; END_VAR F := 1; END_FUNCTION PROGRAM Main END_PROGRAM".to_string(),
            "1:18: VAR_IN_OUT is not supported yet",
        ),
        (
            "FUNCTION F VAR_INPUT a : INT; END_VAR END_FUNCTION PROGRAM Main END_PROGRAM".to_string(),
            "1:10: the function F has no result type; a function without a result is not supported yet",
        ),
        (
            "TYPE S : STRUCT a : INT; END_STRUCT; END_TYPE FUNCTION F : S F.a := 1; END_FUNCTION PROGRAM Main END_PROGRAM".to_string(),
            "a function returning a S is not supported yet",
        ),
        (
            "FUNCTION F : INT VAR t : TON; END_VAR F := 1; END_FUNCTION PROGRAM Main END_PROGRAM".to_string(),
            "a function keeps nothing between calls, so F cannot hold a TON",
        ),
        (
            "FUNCTION F : INT VAR_INPUT a : INT; END_VAR a := 2; F := a; END_FUNCTION PROGRAM Main END_PROGRAM".to_string(),
            "a is an input of F; it cannot be written inside it",
        ),
        (
            "FUNCTION F : INT VAR_INPUT a : INT; END_VAR END_FUNCTION PROGRAM Main END_PROGRAM".to_string(),
            "1:10: the function F never assigns its result: write F := …;",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a : INT; END_VAR F := a; END_FUNCTION {main} F(1); END_PROGRAM"),
            "F is a function: use its result, as in x := F(…);",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a : INT; END_VAR F := a; END_FUNCTION {main} x := F(a := 1, o => x); END_PROGRAM"),
            "an output of a function (name => variable) is not supported yet",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a, b : INT; END_VAR F := a; END_FUNCTION {main} x := F(1); END_PROGRAM"),
            "F takes 2 argument(s) by position; 1 given",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a, b : INT; END_VAR F := a; END_FUNCTION {main} x := F(1, b := 2); END_PROGRAM"),
            "the arguments of F are either all named or all by position",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a, b : INT; END_VAR F := a; END_FUNCTION {main} x := F(a := 1, c := 2); END_PROGRAM"),
            "F has no input c",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a, b : INT; END_VAR F := a; END_FUNCTION {main} x := F(a := 1, a := 2); END_PROGRAM"),
            "a is given twice",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a : INT; END_VAR VAR loc : INT; END_VAR F := a; END_FUNCTION {main} x := F(loc := 1); END_PROGRAM"),
            "F has no input loc",
        ),
        (
            format!("FUNCTION F : INT VAR_INPUT a : INT; END_VAR F := a; END_FUNCTION {main} x := F(1).a; END_PROGRAM"),
            "a member or an element of a function's result is not supported yet",
        ),
        (
            format!("{main} x := t(IN := TRUE); END_PROGRAM"),
            "t is a function block instance: call it as a statement, t(…); then read its outputs",
        ),
        (
            format!("FUNCTION F : INT F := 1; END_FUNCTION {main} VAR y : F; END_VAR END_PROGRAM"),
            "F is a function, not a type",
        ),
        (
            format!("FUNCTION F : INT F := 1; END_FUNCTION {main} VAR F : INT; END_VAR END_PROGRAM"),
            "F is also the name of a function; a variable needs its own",
        ),
        (
            format!("FUNCTION F : INT F := 1; END_FUNCTION FUNCTION f : INT f := 1; END_FUNCTION {main} END_PROGRAM"),
            "f is declared twice",
        ),
        (
            format!("FUNCTION INT_TO_REAL : REAL INT_TO_REAL := 1.0; END_FUNCTION {main} END_PROGRAM"),
            "INT_TO_REAL is a name of the standard; choose another",
        ),
        (
            format!("{main} x := Missing(1); END_PROGRAM"),
            "Missing is not a function this compiler knows yet",
        ),
    ] {
        let d = diagnostics(&src);
        assert!(
            d.iter().any(|m| m.contains(said)),
            "{src}\n  wanted '{said}'\n  got {d:?}"
        );
    }
}
