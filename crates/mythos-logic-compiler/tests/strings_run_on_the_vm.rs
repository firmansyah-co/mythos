//! STRING (IEC 61131-3 ed. 3 §6.3.3, §6.4.2.2, §6.6.1.2.2, §6.6.2.5.11 and Table 34),
//! compiled and run on the VM.

mod support;
use support::*;

use mythos_logic_vm::{Fault, Value};

#[test]
fn a_string_has_its_declared_length_or_254_and_starts_empty_or_as_given() {
    let r = Run::new(
        "TYPE Name : STRING[8]; END_TYPE
         PROGRAM Main
         VAR a : STRING; b : STRING[10] := 'ABCD'; c : STRING(3) := ''; d : Name := 'pump';
             e : STRING[5] := STRING#'OK'; END_VAR
         END_PROGRAM",
    );
    // §6.4.2.2: the length is the declaration's; ed. 3 gives none for a STRING declared
    // without one, and 254 is the implementer's (here) choice.
    let sym = |p: &str| r.c.symbol(p).unwrap().clone();
    assert_eq!((sym("a").ty.as_str(), sym("a").size), ("STRING[254]", 256));
    assert_eq!((sym("b").ty.as_str(), sym("b").size), ("STRING[10]", 12));
    assert_eq!((sym("c").ty.as_str(), sym("c").size), ("STRING[3]", 5));
    assert_eq!(sym("d").ty, "STRING[8]");
    // Table 10: a STRING's default initial value is '' (empty).
    assert_eq!(r.text("a"), "");
    assert_eq!(r.text("b"), "ABCD");
    assert_eq!(r.text("c"), "");
    assert_eq!(r.text("d"), "pump");
    assert_eq!(r.text("e"), "OK");
}

#[test]
fn a_literal_is_its_characters_with_the_dollar_combinations_read() {
    // Table 6 and Table 7; $N is a line feed, the implementer's choice.
    let r = Run::new(
        "PROGRAM Main VAR a : STRING := 'it$'s $$5 $41$42 $L$N$R$T$P \"q\"'; END_VAR END_PROGRAM",
    );
    assert_eq!(r.text("a"), "it's $5 AB \n\n\r\t\u{c} \"q\"");
}

#[test]
fn assignment_cuts_a_string_longer_than_its_target() {
    // §6.6.1.2.2: a longer source is the implementer's; here it is cut to the target.
    let mut r = Run::new(
        "PROGRAM Main VAR long : STRING[10] := 'ABCDEFGHIJ'; short : STRING[4]; back : STRING[10];
           n : INT; END_VAR
           short := long;
           back := short;
           n := LEN(short);
           long := 'xy';
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.text("short"), "ABCD");
    assert_eq!(r.text("back"), "ABCD");
    assert_eq!(r.int("n"), 4);
    assert_eq!(
        r.text("long"),
        "xy",
        "a shorter one leaves nothing of what was there"
    );
}

#[test]
fn strings_compare_as_if_the_shorter_were_extended_with_zero_codes() {
    // §6.6.2.5.11 and its example: 'Z' > 'AZ', 'AZ' > 'ABC'. Extended with characters of
    // code zero, 'A' and 'A$00' are equal.
    let mut r = Run::new(
        "PROGRAM Main VAR z : STRING := 'Z'; az : STRING := 'AZ'; abc : STRING := 'ABC';
           a, b, c, d, e, f, g, h : BOOL; END_VAR
           a := z > az;
           b := az > abc;
           c := abc < az;
           d := 'A' = 'A$00';
           e := 'A' <> 'a';
           f := '' < 'A';
           g := abc >= 'ABC';
           h := abc <= 'AB';
         END_PROGRAM",
    );
    r.scan().unwrap();
    for (v, want) in [
        ("a", true),
        ("b", true),
        ("c", true),
        ("d", true),
        ("e", true),
        ("f", true),
        ("g", true),
        ("h", false),
    ] {
        assert_eq!(r.bool(v), want, "{v}");
    }
}

#[test]
fn every_example_of_table_34_gives_what_the_table_says() {
    let mut r = Run::new(
        "PROGRAM Main VAR n, f : INT; left, right, mid, cat, ins, del, rep : STRING; END_VAR
           n := LEN('ASTRING');
           left := LEFT(IN := 'ASTR', L := 3);
           right := RIGHT(IN := 'ASTR', L := 3);
           mid := MID(IN := 'ASTR', L := 2, P := 2);
           cat := CONCAT('AB', 'CD', 'E');
           ins := INSERT(IN1 := 'ABC', IN2 := 'XY', P := 2);
           del := DELETE(IN := 'ABXYC', L := 2, P := 3);
           rep := REPLACE(IN1 := 'ABCDE', IN2 := 'X', L := 2, P := 3);
           f := FIND(IN1 := 'ABCBC', IN2 := 'BC');
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("n"), 7);
    assert_eq!(r.text("left"), "AST");
    assert_eq!(r.text("right"), "STR");
    assert_eq!(r.text("mid"), "ST");
    assert_eq!(r.text("cat"), "ABCDE");
    assert_eq!(r.text("ins"), "ABXYC");
    assert_eq!(r.text("del"), "ABC");
    assert_eq!(r.text("rep"), "ABXE");
    assert_eq!(r.int("f"), 2);
}

#[test]
fn the_string_functions_at_their_edges() {
    let mut r = Run::new(
        "PROGRAM Main VAR s : STRING := 'ABC'; e : STRING;
           l0, l3, r0, m0, m4, d0, i0, i3, rp0, rpend : STRING;
           f0, f1, f2, f3 : INT; big : LINT := 2; END_VAR
           l0 := LEFT(s, 0);
           l3 := LEFT(s, big + 1);
           r0 := RIGHT(s, 0);
           m0 := MID(s, 0, 4);
           m4 := MID(s, 3, 1);
           d0 := DELETE(s, 0, 2);
           i0 := INSERT(s, 'xy', 0);
           i3 := INSERT(s, 'xy', 3);
           rp0 := REPLACE(s, 'xy', 0, 2);
           rpend := REPLACE(s, '', 2, 2);
           f0 := FIND(s, 'D');
           f1 := FIND(s, '');
           f2 := FIND(e, '');
           f3 := FIND(s, 'ABCD');
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.text("l0"), "");
    assert_eq!(r.text("l3"), "ABC", "a LINT length");
    assert_eq!(r.text("r0"), "");
    assert_eq!(
        r.text("m0"),
        "",
        "no characters from one past the end: no position named"
    );
    assert_eq!(r.text("m4"), "ABC");
    assert_eq!(r.text("d0"), "ABC");
    assert_eq!(r.text("i0"), "xyABC", "after the 0th: first");
    assert_eq!(r.text("i3"), "ABCxy", "after the last: last");
    assert_eq!(r.text("rp0"), "AxyBC", "replacing no characters inserts");
    assert_eq!(r.text("rpend"), "A");
    assert_eq!(r.int("f0"), 0, "not found");
    assert_eq!(r.int("f1"), 1, "an empty string stands at position 1");
    assert_eq!(r.int("f2"), 1);
    assert_eq!(r.int("f3"), 0);
}

/// Run `body` (statements of a program with `s : STRING := 'ABC'` and `n : INT`) and give
/// how its scan stopped.
fn fault_of(body: &str) -> Fault {
    let mut r = Run::new(&format!(
        "PROGRAM Main VAR s : STRING := 'ABC'; t : STRING; n : INT; x : DINT; END_VAR
           {body}
         END_PROGRAM"
    ));
    r.scan()
        .err()
        .unwrap_or_else(|| panic!("{body} ran to its end"))
}

#[test]
fn what_table_34_calls_an_error_stops_the_scan_with_a_named_fault() {
    // §6.6.2.5.11: an integer input below zero, a character position the string does not
    // have, a result longer than the longest string: each an error. Found when it happens,
    // the scan stops; nothing is cut, clamped or guessed.
    for body in [
        "n := -1; t := LEFT(s, n);",
        "n := 4; t := LEFT(s, n);",
        "n := 4; t := RIGHT(s, n);",
        "n := -1; t := RIGHT(s, n);",
        "n := 0; t := MID(s, 1, n);",
        "n := 3; t := MID(s, 2, n);",
        "n := -1; t := MID(s, n, 1);",
        "n := 5; t := MID(s, 0, n);",
        "n := 3; t := DELETE(s, n, 2);",
        "n := 0; t := DELETE(s, 1, n);",
        "n := 4; t := INSERT(s, 'x', n);",
        "n := -1; t := INSERT(s, 'x', n);",
        "n := 2; t := REPLACE(s, 'x', n, 3);",
        "n := -2; t := REPLACE(s, 'x', n, 1);",
    ] {
        assert!(
            matches!(fault_of(body), Fault::StringArgument(_)),
            "{body}: {:?}",
            fault_of(body)
        );
    }
    // A string function's result is never cut: past the longest STRING it is an error.
    let mut r = Run::new(
        "PROGRAM Main VAR a : STRING[32767]; n : INT; END_VAR
           a := 'x';
           FOR n := 1 TO 14 DO a := CONCAT(a, a); END_FOR;
           a := CONCAT(a, a);
         END_PROGRAM",
    );
    assert!(
        matches!(r.scan(), Err(Fault::StringTooLong(_))),
        "16384 + 16384 characters is one more than 32767"
    );
    assert_eq!(r.text("a").len(), 16384, "and nothing was written");
}

#[test]
fn a_literal_argument_below_zero_is_refused_at_compile_time() {
    assert!(
        refusal("PROGRAM Main VAR t : STRING; END_VAR t := LEFT('AB', -1); END_PROGRAM")
            .ends_with("L of LEFT may not be below zero")
    );
}

#[test]
fn integers_and_strings_convert_as_their_literals_write_them() {
    // Table 22 footnote e: the external representation of the value.
    let mut r = Run::new(
        "PROGRAM Main VAR a, b, c, d : STRING; i : INT; e : DINT; f : LINT; g : SINT;
           h : INT; k : DINT; m : LINT; END_VAR
           a := SINT_TO_STRING(-128);
           b := INT_TO_STRING(32767);
           c := DINT_TO_STRING(-2147483648);
           d := LINT_TO_STRING(-9223372036854775808);
           i := STRING_TO_INT('-42');
           e := STRING_TO_DINT('16#7FFF_FFFF');
           f := STRING_TO_LINT('+1_000_000');
           g := STRING_TO_SINT('2#0111_1111');
           h := STRING_TO_INT('40000');
           k := STRING_TO_DINT(INT_TO_STRING(i));
           m := STRING_TO_LINT('8#777');
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.text("a"), "-128");
    assert_eq!(r.text("b"), "32767");
    assert_eq!(r.text("c"), "-2147483648");
    assert_eq!(r.text("d"), "-9223372036854775808");
    assert_eq!(r.int("i"), -42);
    assert_eq!(r.int("e"), 2147483647);
    assert_eq!(r.int("f"), 1_000_000);
    assert_eq!(r.int("g"), 127);
    // A value its type cannot hold is wrapped, as every narrowing here is (ADR-013 §2.13).
    assert_eq!(r.int("h"), 40000 - 65536);
    assert_eq!(r.int("k"), -42);
    assert_eq!(r.int("m"), 511);
    // A conversion gives a string as long as its type's widest value: no longer.
    assert_eq!(
        refusal("PROGRAM Main VAR a : INT; END_VAR a := LEN(INT_TO_STRING(a)) + 'x'; END_PROGRAM"),
        "1:62: a INT and a STRING[1] cannot be combined without a conversion"
    );
    for text in ["' 42'", "'4 2'", "'42.0'", "''", "'INT#5'", "'-16#FF'"] {
        let body = format!("x := STRING_TO_DINT({text});");
        assert!(
            matches!(fault_of(&body), Fault::BadNumber(_)),
            "{text}: {:?}",
            fault_of(&body)
        );
    }
}

#[test]
fn strings_pass_through_functions_blocks_arrays_and_structures() {
    let mut r = Run::new(
        "TYPE Entry : STRUCT name : STRING[6] := 'none'; count : INT; END_STRUCT; END_TYPE
         FUNCTION Greet : STRING[12]
         VAR_INPUT who : STRING[5]; END_VAR
           Greet := CONCAT('hi ', who);
         END_FUNCTION
         FUNCTION_BLOCK Keep
         VAR_INPUT given : STRING[4]; END_VAR
         VAR_OUTPUT kept : STRING[8]; END_VAR
           kept := CONCAT(kept, given);
         END_FUNCTION_BLOCK
         PROGRAM Main
         VAR g1, g2 : STRING[12]; k : Keep; out : STRING[3]; i : INT;
             list : ARRAY [1..3] OF STRING[4] := ['ab', 2('cd')];
             e1, e2 : Entry; pair : BOOL; first : STRING[4]; END_VAR
           g1 := Greet('everybody');
           g2 := CONCAT(Greet('a'), Greet(who := 'b'));
           k(given := 'xyz', kept => out);
           i := 2;
           list[i + 1] := CONCAT(list[i], list[1]);
           first := list[i - 1];
           e1.name := 'pump';
           e1.count := 3;
           e2 := e1;
           pair := e2.name = 'pump' AND list[3] = 'cdab';
         END_PROGRAM",
    );
    r.scan().unwrap();
    // The input of Greet holds five characters: the argument is cut there.
    assert_eq!(r.text("g1"), "hi every");
    assert_eq!(
        r.text("g2"),
        "hi ahi b",
        "each call's result is kept apart before the next call"
    );
    assert_eq!(r.text("out"), "xyz");
    assert_eq!(r.text("list[2]"), "cd");
    assert_eq!(r.text("list[3]"), "cdab");
    assert_eq!(r.text("first"), "ab");
    assert_eq!(r.text("e2.name"), "pump");
    assert_eq!(r.int("e2.count"), 3);
    assert!(r.bool("pair"));
    r.scan().unwrap();
    // The block keeps its output from one call to the next; the target cuts it.
    assert_eq!(r.text("k.kept"), "xyzxyz");
    assert_eq!(r.text("out"), "xyz");
    // A subscript outside its range stops the scan, a string's as any other.
    let mut r = Run::new(
        "PROGRAM Main VAR list : ARRAY [1..2] OF STRING[4]; i : INT := 3; t : STRING; END_VAR
           t := list[i];
         END_PROGRAM",
    );
    assert!(matches!(r.scan(), Err(Fault::IndexOutOfRange(_))));
}

#[test]
fn a_stored_length_past_the_declared_one_is_never_followed() {
    // The host wrote it, or the memory is damaged: the program's own instructions never do.
    let mut r = Run::new(
        "PROGRAM Main VAR s : STRING[4]; t : STRING; n : INT; END_VAR
           n := LEN(s);
           t := s;
         END_PROGRAM",
    );
    let at = r.c.symbol("s").unwrap().offset as usize;
    r.memory[at] = 5;
    assert!(matches!(r.scan(), Err(Fault::BadString(_))));
    r.memory[at] = 4;
    r.scan().unwrap();
    assert_eq!(r.int("n"), 4);
    assert_eq!(r.get("n"), Value::Int(4));
}

#[test]
fn what_strings_do_not_allow_is_refused_with_the_reason() {
    let mut wrong = Vec::new();
    for (src, said) in [
        (
            "PROGRAM Main VAR s : STRING[0]; END_VAR END_PROGRAM",
            "1:29: a STRING holds 1 to 32767 characters, not 0",
        ),
        (
            "PROGRAM Main VAR s : STRING[40000]; END_VAR END_PROGRAM",
            "1:29: a STRING holds 1 to 32767 characters, not 40000",
        ),
        (
            "PROGRAM Main VAR s : STRING[3] := 'ABCD'; END_VAR END_PROGRAM",
            "1:35: the initial value has 4 characters; a STRING[3] holds 3",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := 'a' + 'b'; END_PROGRAM",
            "1:47: a STRING is not a number; join strings with CONCAT(…)",
        ),
        (
            "PROGRAM Main VAR s : STRING; n : INT; END_VAR n := s; END_PROGRAM",
            "1:52: a STRING[254] cannot be used as a INT without a conversion: write STRING_TO_INT(…)",
        ),
        (
            "PROGRAM Main VAR s : STRING; b : BOOL; END_VAR b := s = 1; END_PROGRAM",
            "1:55: a STRING[254] and a DINT cannot be combined without a conversion",
        ),
        (
            "PROGRAM Main VAR n : INT; END_VAR n := LEN(5); END_PROGRAM",
            "1:44: IN of LEN is a STRING; this is a DINT",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := LEFT('ab', 1.5); END_PROGRAM",
            "1:54: L of LEFT is an integer; this is a LREAL",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := MID('ab', 1); END_PROGRAM",
            "1:43: MID takes 3 argument(s), IN, L, P; 2 given",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := MID(IN := 'ab', L := 1); END_PROGRAM",
            "1:43: MID needs P as well",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := CONCAT('ab'); END_PROGRAM",
            "1:43: CONCAT takes two or more strings",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := CONCAT(IN1 := 'a', IN3 := 'b'); END_PROGRAM",
            "1:62: CONCAT has no input IN3",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR s := REAL_TO_STRING(1.5); END_PROGRAM",
            "1:43: REAL_TO_STRING is not supported yet",
        ),
        (
            "FUNCTION LEN : INT LEN := 1; END_FUNCTION PROGRAM Main END_PROGRAM",
            "1:10: LEN is a name of the standard; choose another",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR LEN(s); END_PROGRAM",
            "1:38: LEN is a function: use its result, as in x := LEN(…);",
        ),
        (
            "PROGRAM Main VAR s : STRING; END_VAR CASE s OF 1: ; END_CASE; END_PROGRAM",
            "1:43: CASE needs an integer selector; this is a STRING[254]",
        ),
        (
            "PROGRAM Main VAR s : ARRAY [1..2] OF STRING[2] := ['abc']; END_VAR END_PROGRAM",
            "1:52: the initial value has 3 characters; a STRING[2] holds 2",
        ),
        (
            "PROGRAM Main VAR a : ARRAY [1..2] OF STRING[2]; b : ARRAY [1..2] OF STRING[3]; END_VAR a := b; END_PROGRAM",
            "1:93: a ARRAY [1..2] OF STRING[3] cannot be used as a ARRAY [1..2] OF STRING[2]",
        ),
    ] {
        let got = refusal(src);
        if got != said {
            wrong.push(format!("{src}\n  wanted {said}\n  got    {got}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_variable_may_bear_the_name_of_a_string_function_and_both_are_used() {
    // The standard functions' names are a library's (ADR-013 §2.13): not keywords.
    let mut r = Run::new(
        "PROGRAM Main VAR len : INT; find : INT; s : STRING := 'abc'; END_VAR
           len := LEN(s);
           find := FIND(s, 'c') + len;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("len"), 3);
    assert_eq!(r.int("find"), 6);
}

#[test]
fn every_string_instruction_reads_an_element_of_an_array_where_it_is() {
    // An element's offset is computed at run time and popped by the instruction that names
    // it, after its integers, from its last string to its first. The stack the program
    // declares is exactly what the run is given: one offset too many left behind, or one
    // counted that is not there, shows here as a fault or as a deeper stack (each statement
    // with an element comes before the deepest, so a count that drifts raises it).
    let mut r = Run::new(
        "PROGRAM Main
         VAR w : ARRAY [1..4] OF STRING[6] := ['ab', 'cd', 'abcd', '42'];
             i, j, k, n, len, pos : INT; less : BOOL; num : DINT;
             cat, left, ins, rep, mid : STRING[20]; END_VAR
           i := 1; j := 2; k := 3; n := 1;
           less := w[j] < w[k];
           len := LEN(w[k]);
           cat := CONCAT(w[i], w[j], w[k]);
           left := LEFT(w[k], n + 1);
           mid := MID(w[k], n, j);
           ins := INSERT(w[i], w[j], n);
           rep := REPLACE(w[k], w[i], j, n + 2);
           pos := FIND(w[k], w[j]);
           num := STRING_TO_DINT(w[k + 1]);
           w[n] := w[j + 2];
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("len"), 4);
    assert_eq!(r.text("cat"), "abcdabcd");
    assert_eq!(r.text("left"), "ab");
    assert_eq!(r.text("mid"), "b");
    assert_eq!(r.text("ins"), "acdb");
    assert_eq!(r.text("rep"), "abab");
    assert_eq!(r.int("pos"), 3);
    assert!(!r.bool("less"), "'cd' > 'abcd'");
    assert_eq!(r.int("num"), 42);
    assert_eq!(r.text("w[1]"), "42");
    // The deepest is REPLACE: two offsets, j, then n and 2 while n + 2 is added.
    assert_eq!(r.c.stack_depth, 5);
}
