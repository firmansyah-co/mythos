//! The numeric and selection functions of IEC 61131-3 ed. 3 (Table 28, EXPT of Table 29,
//! TRUNC of Table 22, Table 32, and the operators of Table 29 and Table 33 called by name),
//! compiled and run on the VM.

mod support;
use support::*;

use mythos_logic_vm::{math, Fault, Value};

#[test]
fn the_functions_of_one_real_give_the_vms_own_values_in_the_type_of_their_input() {
    let mut r = Run::new(
        "PROGRAM Main
         VAR x : LREAL := 0.7; y : REAL := 0.7;
             a, b, c, d, e, f, g, h, i, j : LREAL;
             fa, fb : REAL; END_VAR
           a := SQRT(x); b := LN(x); c := LOG(x); d := EXP(x); e := SIN(x);
           f := COS(x); g := TAN(x); h := ASIN(x); i := ACOS(x); j := ATAN(x);
           fa := SQRT(y); fb := SIN(y);
         END_PROGRAM",
    );
    r.scan().unwrap();
    let x = 0.7f64;
    for (v, want) in [
        ("a", math::sqrt(x)),
        ("b", math::ln(x)),
        ("c", math::log10(x)),
        ("d", math::exp(x)),
        ("e", math::sin(x)),
        ("f", math::cos(x)),
        ("g", math::tan(x)),
        ("h", math::asin(x)),
        ("i", math::acos(x)),
        ("j", math::atan(x)),
    ] {
        assert_eq!(r.get(v), Value::Lreal(want), "{v}");
    }
    // A REAL's is computed in LREAL and rounded once.
    let y = 0.7f32 as f64;
    assert_eq!(r.get("fa"), Value::Real(math::sqrt(y) as f32));
    assert_eq!(r.get("fb"), Value::Real(math::sin(y) as f32));
}

#[test]
fn what_is_not_a_real_number_is_what_iec_60559_gives_and_the_scan_goes_on() {
    // §6.6.2.5.8 calls a result outside the output's range an error, and Table 10 footnotes
    // e and f leave infinity and NaN to the implementer: here they are IEC 60559's values,
    // as REAL division by zero gives, and the scan does not stop.
    let mut r = Run::new(
        "PROGRAM Main VAR a, b, c, d : LREAL; n : LREAL := -1.0; z : LREAL; END_VAR
           a := SQRT(n); b := LN(z); c := EXP(1000.0); d := ASIN(2.0);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert!(r.real("a").is_nan());
    assert_eq!(r.real("b"), f64::NEG_INFINITY);
    assert_eq!(r.real("c"), f64::INFINITY);
    assert!(r.real("d").is_nan());
}

#[test]
fn abs_expt_trunc_and_atan2_as_the_tables_say() {
    let mut r = Run::new(
        "PROGRAM Main
         VAR a : INT; b : DINT; c : LREAL; d : SINT; e, f, g, h, k : LREAL; m : REAL;
             t1, t2, t3, t4 : INT; t5 : LINT; at1, at2 : LREAL; n : INT := 3; five : DINT := -5;
             half : REAL := 1.5; ms : SINT := -128; p : LREAL; END_VAR
           a := ABS(-7); b := ABS(five); c := ABS(-2.5); d := ABS(ms);
           e := EXPT(2.0, 10); f := 2.0 ** 0.5; g := EXPT(-2.0, n); h := 2.0 ** -2;
           k := -2.0 ** 2; m := EXPT(half, 2.0); p := 2.0 * 3.0 ** 2;
           t1 := TRUNC_INT(1.6); t2 := TRUNC_INT(-1.6); t3 := REAL_TRUNC_INT(2.5);
           t4 := LREAL_TRUNC_INT(-2.5); t5 := TRUNC_LINT(1.0E15 + 0.75);
           at1 := ATAN2(1.0, -1.0); at2 := ATAN2(Y := -1.0, X := 0.0);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("a"), 7);
    assert_eq!(r.int("b"), 5);
    assert_eq!(r.real("c"), 2.5);
    // ABS of SINT's most negative value does not fit: it wraps, as every result outside its
    // type does here (ADR-013 §2.13).
    assert_eq!(r.get("d"), Value::Sint(-128));
    assert_eq!(r.real("e"), 1024.0);
    assert_eq!(r.real("f"), std::f64::consts::SQRT_2);
    assert_eq!(r.real("g"), -8.0, "an integral power of a negative base");
    assert_eq!(r.real("h"), 0.25);
    // Table 71: the minus binds before **.
    assert_eq!(r.real("k"), 4.0);
    // And ** before *: 2 * (3 ** 2).
    assert_eq!(r.real("p"), 18.0);
    assert_eq!(r.get("m"), Value::Real(2.25));
    // Table 22 footnote c's examples: TRUNC_INT(1.6) is 1, TRUNC_INT(-1.6) is -1.
    assert_eq!(r.int("t1"), 1);
    assert_eq!(r.int("t2"), -1);
    assert_eq!(r.int("t3"), 2);
    assert_eq!(r.int("t4"), -2);
    assert_eq!(r.int("t5"), 1_000_000_000_000_000);
    assert_eq!(r.real("at1"), 3.0 * std::f64::consts::FRAC_PI_4);
    assert_eq!(r.real("at2"), -std::f64::consts::FRAC_PI_2);
}

#[test]
fn selection_works_as_table_32_writes_it() {
    let mut r = Run::new(
        "PROGRAM Main
         VAR x : INT := 7; k : INT := 1; g : BOOL;
             s0, s1, mx, mn, l1, l2, l3, m0, m2 : INT; ml : LREAL; mt : TIME; mb : BOOL;
             named : INT; END_VAR
           (* Table 32's examples: SEL(G := 0, IN0 := X, IN1 := 5), MAX(B, C, D), MIN, LIMIT,
              MUX(0, B, C, D). *)
           s0 := SEL(G := g, IN0 := x, IN1 := 5);
           s1 := SEL(NOT g, x, 5);
           mx := MAX(3, x, -2);
           mn := MIN(3, x, -2);
           l1 := LIMIT(IN := x, MN := 0, MX := 5);
           l2 := LIMIT(0, -3, 5);
           l3 := LIMIT(0, 4, 5);
           m0 := MUX(0, x, 20, 30);
           m2 := MUX(k + 1, x, 20, 30);
           ml := MAX(1, 2.5);
           mt := MIN(T#2s, T#1s500ms);
           mb := MAX(FALSE, TRUE);
           named := MUX(K := k, IN0 := 1, IN1 := 2);
         END_PROGRAM",
    );
    r.scan().unwrap();
    for (v, want) in [
        ("s0", 7),
        ("s1", 5),
        ("mx", 7),
        ("mn", -2),
        ("l1", 5),
        ("l2", 0),
        ("l3", 4),
        ("m0", 7),
        ("m2", 30),
        ("mt", 1_500_000_000),
        ("named", 2),
    ] {
        assert_eq!(r.int(v), want, "{v}");
    }
    assert_eq!(r.real("ml"), 2.5, "an INT and an LREAL: chosen as LREAL");
    assert!(r.bool("mb"));
}

#[test]
fn mux_given_a_k_with_no_input_stops_the_scan() {
    // Table 32 footnote e: an error. A literal K is refused at compile time; a computed one
    // stops the scan, as an array subscript out of its range does.
    for k in [-1, 3] {
        let mut r = Run::new(&format!(
            "PROGRAM Main VAR k : INT := {k}; m : INT; END_VAR m := MUX(k, 1, 2, 3); END_PROGRAM"
        ));
        assert!(
            matches!(r.scan(), Err(Fault::SelectorOutOfRange(_))),
            "K = {k}"
        );
    }
    assert!(
        refusal("PROGRAM Main VAR m : INT; END_VAR m := MUX(3, 1, 2, 3); END_PROGRAM")
            .ends_with("K of MUX is 3; this MUX has inputs IN0 to IN2")
    );
}

#[test]
fn only_the_input_sel_and_mux_choose_is_worked_out() {
    // A function has no side effect a program could see, but a fault is one: an input not
    // chosen is not evaluated, so an array subscript out of range there stops nothing.
    let mut r = Run::new(
        "PROGRAM Main VAR a : ARRAY [1..2] OF INT := [5, 6]; i : INT := 9; s, m : INT; END_VAR
           s := SEL(FALSE, 1, a[i]);
           m := MUX(1, a[i], 2, a[i]);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("s"), 1);
    assert_eq!(r.int("m"), 2);
}

#[test]
fn the_operators_are_functions_by_name_too() {
    // Table 29 (ADD, MUL extensible; SUB, DIV) and Table 33 (GT, GE, EQ, LE, LT extensible
    // as a chain of pairs; NE), and MOVE.
    let mut r = Run::new(
        "PROGRAM Main VAR a, b, c, d, m : INT; g1, g2, e1, n1 : BOOL; END_VAR
           a := ADD(1, 2, 3, 4); b := MUL(IN1 := 2, IN2 := 3, IN3 := 4); c := SUB(10, 4);
           d := DIV(-7, 2); m := MOVE(42);
           g1 := GT(5, 3, 1); g2 := GT(5, 3, 4); e1 := EQ(2, 2, 2); n1 := NE(1, 2);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("a"), 10);
    assert_eq!(r.int("b"), 24);
    assert_eq!(r.int("c"), 6);
    assert_eq!(
        r.int("d"),
        -3,
        "truncated toward zero (Table 29 footnote d)"
    );
    assert_eq!(r.int("m"), 42);
    assert!(r.bool("g1"));
    assert!(!r.bool("g2"), "5 > 3 but not 3 > 4");
    assert!(r.bool("e1"));
    assert!(r.bool("n1"));
}

#[test]
fn what_the_library_does_not_allow_is_refused_with_the_reason() {
    let mut wrong = Vec::new();
    for (src, said) in [
        (
            "PROGRAM Main VAR a : LREAL; t : TIME; END_VAR a := SQRT(t); END_PROGRAM",
            "IN of SQRT is a REAL or an LREAL; this is a TIME",
        ),
        (
            "PROGRAM Main VAR a : LREAL; n : LINT; END_VAR a := SIN(n); END_PROGRAM",
            "IN of SIN is a REAL or an LREAL; this is a LINT",
        ),
        (
            // Figure 11: a DINT widens to LREAL, not to REAL, so its root is an LREAL.
            "PROGRAM Main VAR r : REAL; d : DINT; END_VAR r := SQRT(d); END_PROGRAM",
            "a LREAL cannot be used as a REAL without a conversion: write LREAL_TO_REAL(…)",
        ),
        (
            "PROGRAM Main VAR a : INT; b : BOOL; END_VAR a := ABS(b); END_PROGRAM",
            "IN of ABS is a number; this is a BOOL",
        ),
        (
            "PROGRAM Main VAR a : INT; END_VAR a := MAX(1); END_PROGRAM",
            "MAX takes two or more inputs",
        ),
        (
            "PROGRAM Main VAR a : INT; t : TIME; END_VAR a := MAX(1, t); END_PROGRAM",
            "the inputs of MAX are a TIME and a DINT, which have no type in common",
        ),
        (
            "PROGRAM Main VAR a : INT; END_VAR a := MUX(0, 1); END_PROGRAM",
            "MUX takes K and two or more inputs",
        ),
        (
            "PROGRAM Main VAR a : INT; END_VAR a := SEL(1, 2, 3); END_PROGRAM",
            "a condition must be BOOL; this is a DINT",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := SEL(TRUE, 'a', 'b'); END_PROGRAM",
            "SEL of strings is not supported yet",
        ),
        (
            "PROGRAM Main VAR a : INT; END_VAR a := TRUNC_INT(5); END_PROGRAM",
            "IN of TRUNC_INT is a REAL or an LREAL; this is a DINT",
        ),
        (
            "PROGRAM Main VAR a : LREAL; t : TIME; END_VAR a := 2.0 ** t; END_PROGRAM",
            "IN2 of EXPT is a number; this is a TIME",
        ),
        (
            "PROGRAM Main VAR a : INT; END_VAR a := SUB(1, 2, 3); END_PROGRAM",
            "SUB takes two inputs",
        ),
        (
            "FUNCTION Max : INT Max := 1; END_FUNCTION PROGRAM Main END_PROGRAM",
            "Max is a name of the standard; choose another",
        ),
        (
            "PROGRAM Main VAR a : REAL; END_VAR a := SQRT(2.0) + LN(2); END_PROGRAM",
            "",
        ),
    ] {
        let d = diagnostics_or_none(src);
        let ok = if said.is_empty() {
            d.is_empty()
        } else {
            d.len() == 1 && d[0].ends_with(said)
        };
        if !ok {
            wrong.push(format!("{src}\n  wanted {said:?}\n  got    {d:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The diagnostics of `src`, none when it compiles.
fn diagnostics_or_none(src: &str) -> Vec<String> {
    match mythos_logic_compiler::compile(src, &mythos_logic_compiler::TargetProfile::pc()) {
        Ok(_) => Vec::new(),
        Err(d) => d.iter().map(|d| d.to_string()).collect(),
    }
}
