//! What the front-end says about a source with mistakes in it (ADR-013 §2.8): every
//! independent mistake of one compile, each once, with where it starts and ends; and nothing
//! about what only follows from a mistake already told.
//!
//! Each test gives the exact list. A cascade (a second message caused by the first) or a
//! mistake that went untold both make a list differ.

use mythos_logic_ir::{Diagnostic, KEYWORDS};
use mythos_logic_st::lower;

/// Every diagnostic as `line:col-end_line:end_col message`.
fn told(src: &str) -> Vec<String> {
    match lower(src) {
        Ok(_) => Vec::new(),
        Err(diags) => diags.iter().map(ranged).collect(),
    }
}

fn ranged(d: &Diagnostic) -> String {
    format!(
        "{}:{}-{}:{} {}",
        d.line, d.col, d.end_line, d.end_col, d.message
    )
}

#[test]
fn one_compile_tells_every_independent_mistake_once() {
    let src = "PROGRAM Main
VAR
  a, b : INT;
  at : INT;
  s : STRING;
  c : BOOL
  d : DINT;
END_VAR
  a := 'text';
  IF a > THEN
    b := ;
    d := 5 +* 2;
  END_IF;
  WHILE c DO
    a := a + 1
  END_WHILE;
END_PROGRAM
FUNCTION Twice : INT
VAR_INPUT x : INT; END_VAR
  Twice := x * 2.5;
  Twice := nope;
END_FUNCTION";
    assert_eq!(
        told(src),
        vec![
            // A keyword as a name; a type not supported; a `;` left out of a declaration.
            "4:3-4:5 at is a keyword of IEC 61131-3 and cannot be used as a name",
            "5:7-5:13 'STRING' is not supported yet",
            "7:3-7:4 expected ';', found 'd'",
            // What the lexer cannot read.
            "9:8-9:14 strings are not supported yet",
            // A head that cannot be read, and two statements under it.
            "10:10-10:14 expected a value, found 'THEN'",
            "11:10-11:11 expected a value, found ';'",
            "12:13-12:14 expected a value, found '*'",
            // A `;` left out before the end of a list.
            "16:3-16:12 expected ';', found 'END_WHILE'",
            // The function was read whole, so it is checked as well.
            "20:14-20:15 a LREAL cannot be used as a INT without a conversion: write LREAL_TO_INT(…)",
            "21:12-21:16 nope is not declared",
        ]
    );
}

#[test]
fn a_statement_that_cannot_be_read_is_left_at_its_semicolon_and_the_next_is_read() {
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := ; a := 1 2; a := (3; a := 4; END_PROGRAM"),
        vec![
            "1:37-1:38 expected a value, found ';'",
            "1:46-1:47 expected ';', found the number 2",
            "1:56-1:57 expected ')', found ';'",
        ]
    );
    // With no `;` to stop at, it is left before the keyword that ends the list: that keyword
    // is not eaten, so nothing more is missed.
    assert_eq!(
        told("PROGRAM P VAR a : INT; c : BOOL; END_VAR IF c THEN a := 1 + END_IF; a := ; END_PROGRAM"),
        vec![
            "1:61-1:67 expected a value, found 'END_IF'",
            "1:74-1:75 expected a value, found ';'",
        ]
    );
}

#[test]
fn a_head_that_cannot_be_read_does_not_hide_the_statements_under_it() {
    assert_eq!(
        told(
            "PROGRAM P VAR a, i : INT; c : BOOL; END_VAR
IF a + THEN a := ; ELSIF c c THEN a := ; ELSE a := ; END_IF;
CASE a + OF 1: a := ; ELSE a := ); END_CASE;
FOR i := 1 TO DO a := ; END_FOR;
WHILE c + DO a := ; END_WHILE;
REPEAT a := ; UNTIL c + END_REPEAT;
END_PROGRAM"
        ),
        vec![
            "2:8-2:12 expected a value, found 'THEN'",
            "2:18-2:19 expected a value, found ';'",
            "2:28-2:29 expected THEN, found 'c'",
            "2:40-2:41 expected a value, found ';'",
            "2:52-2:53 expected a value, found ';'",
            "3:10-3:12 expected a value, found 'OF'",
            "3:21-3:22 expected a value, found ';'",
            "3:33-3:34 expected a value, found ')'",
            "4:15-4:17 expected a value, found 'DO'",
            "4:23-4:24 expected a value, found ';'",
            "5:11-5:13 expected a value, found 'DO'",
            "5:19-5:20 expected a value, found ';'",
            "6:13-6:14 expected a value, found ';'",
            "6:25-6:35 expected a value, found 'END_REPEAT'",
        ]
    );
}

#[test]
fn a_statement_whose_head_is_lost_is_left_at_its_own_end_with_what_is_nested_in_it() {
    // No THEN to read on from. The IF is left at its own END_IF: the END_IF of the IF nested
    // in it is not taken for it, and the statements in between are not told as if they
    // stood alone.
    assert_eq!(
        told(
            "PROGRAM P VAR a : INT; c : BOOL; END_VAR
IF a > ;
  IF c THEN a := 1; END_IF;
  a := 2;
END_IF;
a := ;
END_PROGRAM"
        ),
        vec![
            "2:8-2:9 expected a value, found ';'",
            "6:6-6:7 expected a value, found ';'",
        ]
    );
    // The same for a FOR without DO and a CASE with a label that is not one.
    assert_eq!(
        told(
            "PROGRAM P VAR a, i : INT; END_VAR
FOR i := 1 TO 3 a := 1; END_FOR;
CASE a OF 1: a := 1; 2 a := 2; 3: a := 3; END_CASE;
a := ;
END_PROGRAM"
        ),
        vec![
            "2:17-2:18 expected DO, found 'a'",
            "3:24-3:25 expected ':', found 'a'",
            "4:6-4:7 expected a value, found ';'",
        ]
    );
}

#[test]
fn a_missing_end_is_told_once_where_it_is_missed() {
    // END_IF is missing: END_WHILE is not taken for it, so the WHILE still closes.
    assert_eq!(
        told("PROGRAM P VAR a : INT; c : BOOL; END_VAR WHILE c DO IF c THEN a := 1; END_WHILE; END_PROGRAM"),
        vec!["1:71-1:80 expected END_IF, found 'END_WHILE'"]
    );
    // END_PROGRAM is missing where the source ends: a point, with nothing to underline.
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := 1;"),
        vec!["1:39-1:39 expected END_PROGRAM, found the end of the source"]
    );
    // And where the next POU begins: that POU is read all the same.
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := 1; FUNCTION F : INT F := ; END_FUNCTION"),
        vec![
            "1:40-1:48 expected END_PROGRAM, found 'FUNCTION'",
            "1:62-1:63 expected a value, found ';'",
        ]
    );
    // The end of another kind of POU is taken for the one that was meant.
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := 1; END_FUNCTION FUNCTION F : INT F := ; END_FUNCTION"),
        vec![
            "1:40-1:52 expected END_PROGRAM, found 'END_FUNCTION'",
            "1:75-1:76 expected a value, found ';'",
        ]
    );
}

#[test]
fn a_keyword_that_ends_nothing_is_told_and_the_statements_after_it_are_read() {
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := 1; END_IF; a := ; END_PROGRAM"),
        vec![
            "1:40-1:46 expected END_PROGRAM, found 'END_IF'",
            "1:53-1:54 expected a value, found ';'",
        ]
    );
}

#[test]
fn a_declaration_that_cannot_be_read_is_left_at_its_semicolon_and_the_next_is_read() {
    assert_eq!(
        told(
            "PROGRAM P
VAR
  a : ;
  b INT;
  c : ARRAY [1..] OF INT;
  d : STRUCT x : INT; y : INT; END_STRUCT;
  e : INT := ;
END_VAR
END_PROGRAM"
        ),
        vec![
            "3:7-3:8 expected a name, found ';'",
            "4:5-4:8 expected ':', found 'INT'",
            "5:17-5:18 an array bound must be an integer literal for now, found ']'",
            // The structure is stepped over whole: its members are not read as variables.
            "6:7-6:13 a STRUCT without a name is not supported yet: declare it in TYPE … END_TYPE and use its name",
            "7:14-7:15 expected a value, found ';'",
        ]
    );
    // The same among the members of a structure and the declarations of a TYPE block.
    assert_eq!(
        told(
            "TYPE
  A : STRUCT x : ; y : INT; END_STRUCT;
  B : (Red, Green);
  C : STRUCT x INT; END_STRUCT;
END_TYPE
PROGRAM P END_PROGRAM"
        ),
        vec![
            "2:18-2:19 expected a name, found ';'",
            "3:7-3:8 an enumeration is not supported yet",
            "4:16-4:19 expected ':', found 'INT'",
        ]
    );
}

#[test]
fn the_statements_of_a_pou_that_was_not_read_whole_are_not_checked() {
    // The declaration of `b` could not be read, so `b` is not known: "b is not declared"
    // would follow from the `;` left out and tell nothing new. What else the statements get
    // wrong is found once the POU can be read.
    assert_eq!(
        told("PROGRAM P VAR a : INT; b : BOOL END_VAR a := b; a := nope; END_PROGRAM"),
        vec!["1:33-1:40 expected ';', found 'END_VAR'"]
    );
    // The same when it is a statement that could not be read.
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := ; a := nope; END_PROGRAM"),
        vec!["1:37-1:38 expected a value, found ';'"]
    );
    // A declaration that was read and refused leaves the statements checked.
    assert_eq!(
        told("PROGRAM P VAR a : INT; b : Nope; END_VAR a := b; a := nope; END_PROGRAM"),
        vec![
            "1:28-1:32 Nope is not a type this compiler knows",
            "1:55-1:59 nope is not declared",
        ]
    );
    // One mistake in a declaration of several names is one mistake.
    assert_eq!(
        told("PROGRAM P VAR a, b, c : Nope; END_VAR END_PROGRAM"),
        vec!["1:25-1:29 Nope is not a type this compiler knows"]
    );
}

#[test]
fn a_section_never_closed_is_told_once_and_the_statements_are_not_read_as_declarations() {
    assert_eq!(
        told("PROGRAM P VAR a : INT; a := 1; a := 2; a := ; END_PROGRAM"),
        vec![
            "1:24-1:25 expected END_VAR, found 'a'",
            "1:45-1:46 expected a value, found ';'",
        ]
    );
    assert_eq!(
        told("PROGRAM P VAR a : INT; IF a > 0 THEN a := ; END_IF; END_PROGRAM"),
        vec![
            "1:24-1:26 expected END_VAR, found 'IF'",
            "1:43-1:44 expected a value, found ';'",
        ]
    );
    assert_eq!(
        told("TYPE S : STRUCT x : INT; PROGRAM P END_PROGRAM"),
        vec!["1:26-1:33 expected END_STRUCT, found 'PROGRAM'"]
    );
}

#[test]
fn a_section_that_is_not_supported_is_stepped_over_whole() {
    assert_eq!(
        told(
            "PROGRAM P
VAR CONSTANT k : INT := 1; END_VAR
VAR_IN_OUT z : INT; END_VAR
VAR a : INT; END_VAR
  a := 1;
  VAR b : INT; c : INT; END_VAR
  a := ;
END_PROGRAM
FUNCTION F : INT
VAR_OUTPUT o : INT; END_VAR
  F := ;
END_FUNCTION"
        ),
        vec![
            "2:5-2:13 CONSTANT is not supported yet",
            "3:1-3:11 VAR_IN_OUT is not supported yet",
            "6:3-6:6 VAR after the statements have begun is not supported yet",
            "7:8-7:9 expected a value, found ';'",
            "10:1-10:11 VAR_OUTPUT on a function is not supported yet",
            "11:8-11:9 expected a value, found ';'",
        ]
    );
}

#[test]
fn a_pou_that_cannot_be_read_does_not_stop_the_others_being_read_and_checked() {
    // The head of B is not supported, so B is left at its END_: nothing inside it is told.
    // P is read whole and checked. What P does with B is not told, because what B has is
    // not known; what P gets wrong by itself is.
    assert_eq!(
        told(
            "FUNCTION_BLOCK B EXTENDS C
VAR_INPUT x : INT; END_VAR
  x := ;
END_FUNCTION_BLOCK
PROGRAM P
VAR b : B; q : INT; END_VAR
  b(x := 1, y := 2);
  q := b.z;
  q := nope;
END_PROGRAM"
        ),
        vec![
            "1:18-1:25 EXTENDS is not supported yet",
            "9:8-9:12 nope is not declared",
        ]
    );
    // A syntax mistake in one POU; the mistakes of meaning in another are found as well.
    assert_eq!(
        told(
            "FUNCTION F : INT
VAR_INPUT x : INT; END_VAR
  F := x +;
END_FUNCTION
PROGRAM P
VAR q : INT; b : BOOL; END_VAR
  q := b;
  b := F(1);
END_PROGRAM"
        ),
        vec![
            "3:11-3:12 expected a value, found ';'",
            "7:8-7:9 a BOOL cannot be used as a INT without a conversion: write BOOL_TO_INT(…)",
            "8:8-8:12 a INT cannot be used as a BOOL without a conversion: write INT_TO_BOOL(…)",
        ]
    );
}

#[test]
fn what_is_not_supported_at_the_top_of_a_source_is_stepped_over_whole() {
    // The PROGRAM line inside the configuration is not the start of a POU.
    assert_eq!(
        told(
            "CONFIGURATION C
  RESOURCE R ON PLC
    TASK t(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM p WITH t : Main;
  END_RESOURCE
END_CONFIGURATION
PROGRAM Main VAR a : INT; END_VAR a := ; END_PROGRAM"
        ),
        vec![
            "1:1-1:14 CONFIGURATION is not supported yet",
            "7:40-7:41 expected a value, found ';'",
        ]
    );
    // What is no declaration at all is told once, up to the next thing that is one.
    assert_eq!(
        told("hello world; x := 1; PROGRAM P END_PROGRAM END_IF"),
        vec![
            "1:1-1:6 expected PROGRAM, FUNCTION, FUNCTION_BLOCK or TYPE, found 'hello'",
            "1:44-1:50 expected PROGRAM, FUNCTION, FUNCTION_BLOCK or TYPE, found 'END_IF'",
        ]
    );
}

#[test]
fn text_the_lexer_cannot_read_is_told_by_the_lexer_and_by_nothing_else() {
    // Each is where a value should be; the parser says nothing about not finding one.
    assert_eq!(
        told(
            "PROGRAM P VAR a : INT; END_VAR
  a := 'text';
  a := INT#5 + 1;
  a := 3 § 4;
  a := ;
END_PROGRAM"
        ),
        vec![
            "2:8-2:14 strings are not supported yet",
            "3:8-3:13 the typed literal INT#… is not supported yet; only T# and TIME#",
            "4:10-4:11 '§' is not part of Structured Text",
            "5:8-5:9 expected a value, found ';'",
        ]
    );
    // A comment never closed takes the rest of the source: the END_PROGRAM that is not
    // there may be inside it, so it is not told as missing.
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := 1; (* never closed\n a := ; END_PROGRAM"),
        vec!["1:40-1:42 a comment that is never closed"]
    );
}

#[test]
fn the_use_of_what_a_refused_declaration_declared_is_not_a_second_mistake() {
    // A variable of a type that does not exist: told where it is declared, and not as
    // "not declared" at each use.
    assert_eq!(
        told("PROGRAM P VAR a : Nope; b : INT; END_VAR a := 1; b := a + 1; a(x := 1); b := a(1); b := zz; END_PROGRAM"),
        vec![
            "1:19-1:23 Nope is not a type this compiler knows",
            "1:89-1:91 zz is not declared",
        ]
    );
    // It is still a name that is taken.
    assert_eq!(
        told("PROGRAM P VAR a : Nope; a : INT; END_VAR END_PROGRAM"),
        vec![
            "1:19-1:23 Nope is not a type this compiler knows",
            "1:25-1:26 a is declared twice",
        ]
    );
    // A type the parser could not read, and a structure with a member it could not read:
    // a variable of the first is not of an unknown type, and a member missing from the
    // second may be the one not read. A member that is there is still checked.
    assert_eq!(
        told(
            "TYPE Thing : (A, B); Other : STRUCT x : INT; y : 'no'; END_STRUCT; END_TYPE
PROGRAM P VAR a : Thing; o : Other; b : BOOL; END_VAR
  b := a; b := o.y; b := o.z; b := o.x;
END_PROGRAM"
        ),
        vec![
            "1:14-1:15 an enumeration is not supported yet",
            "1:50-1:54 strings are not supported yet",
            "3:36-3:39 a INT cannot be used as a BOOL without a conversion: write INT_TO_BOOL(…)",
        ]
    );
    // A function block with a declaration that was refused: its other members are known.
    assert_eq!(
        told(
            "FUNCTION_BLOCK B VAR_INPUT x : INT; y : Nope; END_VAR END_FUNCTION_BLOCK
PROGRAM P VAR b : B; END_VAR b(x := TRUE, y := 1, z := 2); END_PROGRAM"
        ),
        vec![
            "1:41-1:45 Nope is not a type this compiler knows",
            "2:37-2:41 a BOOL cannot be used as a INT without a conversion: write BOOL_TO_INT(…)",
        ]
    );
    // A function with an input that was refused: how many it takes, and which, is not known.
    assert_eq!(
        told(
            "FUNCTION F : INT VAR_INPUT x : INT; y : Nope; END_VAR F := x; END_FUNCTION
PROGRAM P VAR a : INT; END_VAR a := F(1, 2); a := F(x := 1, z := 2); a := F(x := TRUE); END_PROGRAM"
        ),
        vec![
            "1:41-1:45 Nope is not a type this compiler knows",
            "2:82-2:86 a BOOL cannot be used as a INT without a conversion: write BOOL_TO_INT(…)",
        ]
    );
    // A function whose result type is not known: no call of it is typed, as a BOOL or as
    // anything else.
    assert_eq!(
        told("FUNCTION F VAR_INPUT x : INT; END_VAR F := x; END_FUNCTION PROGRAM P VAR b : BOOL; i : INT; END_VAR b := F(1); i := F(1); i := F(1, 2); END_PROGRAM"),
        vec!["1:10-1:11 the function F has no result type; a function without a result is not supported yet"]
    );
}

#[test]
fn a_declaration_whose_name_cannot_be_read_does_not_make_the_source_lack_what_it_declares() {
    // The PROGRAM is there; it has no name.
    assert_eq!(
        told("PROGRAM 5 VAR a : INT; END_VAR a := 1; END_PROGRAM"),
        vec!["1:9-1:10 expected a name, found the number 5"]
    );
    // The type and the function used below may be the ones without a name.
    assert_eq!(
        told(
            "TYPE 5 : STRUCT x : INT; END_STRUCT; END_TYPE
FUNCTION 6 : INT 6 := 1; END_FUNCTION
PROGRAM P VAR a : Thing; b : INT; END_VAR b := a.x; b := G(1); END_PROGRAM"
        ),
        vec![
            "1:6-1:7 expected a name, found the number 5",
            "2:10-2:11 expected a name, found the number 6",
        ]
    );
    // With every name read, each of the three is told.
    assert_eq!(
        told("FUNCTION F : INT F := 1; END_FUNCTION FUNCTION_BLOCK B VAR a : Thing; b : INT; END_VAR b := G(1); END_FUNCTION_BLOCK"),
        vec![
            "0:0-0:0 the source has no PROGRAM; it needs one, where a scan starts",
            "1:64-1:69 Thing is not a type this compiler knows",
            "1:93-1:97 G is not a function this compiler knows yet (the *_TO_* conversions are, and the functions of this source)",
        ]
    );
}

#[test]
fn a_diagnostic_says_where_it_starts_and_where_it_ends() {
    assert_eq!(
        told(
            "PROGRAM P VAR a : INT; b : BOOL; arr : ARRAY [1..3] OF INT; t : TON; END_VAR
  a := b;
  a := arr[1, 2];
  b := a + b;
  a := NOT a;
  a := Nope(1,
            2);
  arr[9] := 0;
  a := t.Q;
  a := -b;
  t(IN := 1);
  a := 70000;
END_PROGRAM"
        ),
        vec![
            // A variable: its name.
            "2:8-2:9 a BOOL cannot be used as a INT without a conversion: write BOOL_TO_INT(…)",
            // Subscripts: from `[` to `]`.
            "3:11-3:17 arr has 1 dimension(s); 2 subscript(s) given",
            // An operation: its operator.
            "4:10-4:11 a INT and a BOOL cannot be combined without a conversion",
            // NOT and what it is applied to.
            "5:8-5:13 NOT on a INT (bit operations on integers are not supported yet)",
            // A call, to its `)`: on the next line here.
            "6:8-7:15 Nope is not a function this compiler knows yet (the *_TO_* conversions are, and the functions of this source)",
            // One subscript.
            "8:7-8:8 the subscript 9 is outside 1..3",
            // A member of an instance: the whole path.
            "9:8-9:11 a BOOL cannot be used as a INT without a conversion: write BOOL_TO_INT(…)",
            // A minus and what it negates.
            "10:8-10:10 a BOOL cannot be negated",
            // A literal.
            "11:11-11:12 a DINT cannot be used as a BOOL without a conversion: write DINT_TO_BOOL(…)",
            "12:8-12:13 70000 is outside the range of INT",
        ]
    );
    // About the program as a whole: no place, all four are 0.
    assert_eq!(
        told(""),
        vec!["0:0-0:0 the source has no PROGRAM; it needs one, where a scan starts"]
    );
    // A column counts characters, not bytes: `é` is one.
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR (* é *) a := 'é' + b; END_PROGRAM"),
        vec!["1:45-1:48 strings are not supported yet"]
    );
}

#[test]
fn every_keyword_of_the_standard_is_refused_as_a_name_and_named() {
    // IEC 61131-3 ed. 3 §6.1.3. As a variable, in upper and in lower case: one message,
    // naming the word, and nothing else about the declaration.
    for keyword in KEYWORDS {
        for word in [keyword.to_string(), keyword.to_ascii_lowercase()] {
            let src = format!("PROGRAM P\nVAR\n  ok : INT;\n  {word} : INT;\nEND_VAR\nEND_PROGRAM");
            let end = 3 + word.len();
            assert_eq!(
                told(&src),
                vec![format!(
                    "4:3-4:{end} {word} is a keyword of IEC 61131-3 and cannot be used as a name"
                )],
                "{word}"
            );
        }
    }
    // The example of ADR-013: `at : INT` compiled.
    assert_eq!(
        told("PROGRAM P VAR at : INT; END_VAR END_PROGRAM"),
        vec!["1:15-1:17 at is a keyword of IEC 61131-3 and cannot be used as a name"]
    );
}

#[test]
fn a_keyword_is_refused_as_the_name_of_anything_a_source_declares() {
    // A type, a member, a function block, an input, a function, a program; and one of two
    // names in a list.
    assert_eq!(
        told(
            "TYPE on : STRUCT of : INT; END_STRUCT; END_TYPE
FUNCTION_BLOCK task VAR_INPUT step, x : INT; END_VAR END_FUNCTION_BLOCK
FUNCTION real : INT real := 1; END_FUNCTION
PROGRAM with VAR a, to : INT; END_VAR END_PROGRAM"
        ),
        vec![
            "1:6-1:8 on is a keyword of IEC 61131-3 and cannot be used as a name",
            "1:18-1:20 of is a keyword of IEC 61131-3 and cannot be used as a name",
            "2:16-2:20 task is a keyword of IEC 61131-3 and cannot be used as a name",
            "2:31-2:35 step is a keyword of IEC 61131-3 and cannot be used as a name",
            "3:10-3:14 real is a keyword of IEC 61131-3 and cannot be used as a name",
            "4:9-4:13 with is a keyword of IEC 61131-3 and cannot be used as a name",
            "4:21-4:23 to is a keyword of IEC 61131-3 and cannot be used as a name",
        ]
    );
    // What else is wrong with the head of a POU named by a keyword is told when it has a
    // name: the name is one token, told once.
    assert_eq!(
        told("FUNCTION step VAR_INPUT x : INT; END_VAR END_FUNCTION PROGRAM P END_PROGRAM"),
        vec!["1:10-1:14 step is a keyword of IEC 61131-3 and cannot be used as a name"]
    );
    // A keyword that can follow a POU's name is not one: the name was left out.
    assert_eq!(
        told("PROGRAM VAR a : INT; END_VAR a := 1; END_PROGRAM"),
        vec!["1:9-1:12 expected a name, found 'VAR'"]
    );
}

#[test]
fn a_keyword_declared_as_a_name_is_told_where_it_is_declared_and_not_where_it_is_used() {
    // Used before and after its declaration, as a variable, a type, a member and an input.
    assert_eq!(
        told(
            "PROGRAM P
VAR t : task; o : on; step : INT; END_VAR
  t(to := step);
  o.of := step + 1;
  step := t.from;
END_PROGRAM
TYPE on : STRUCT of : INT; END_STRUCT; END_TYPE
FUNCTION_BLOCK task VAR_INPUT to : INT; END_VAR VAR_OUTPUT from : INT; END_VAR END_FUNCTION_BLOCK"
        ),
        vec![
            "2:23-2:27 step is a keyword of IEC 61131-3 and cannot be used as a name",
            "7:6-7:8 on is a keyword of IEC 61131-3 and cannot be used as a name",
            "7:18-7:20 of is a keyword of IEC 61131-3 and cannot be used as a name",
            "8:16-8:20 task is a keyword of IEC 61131-3 and cannot be used as a name",
            "8:31-8:33 to is a keyword of IEC 61131-3 and cannot be used as a name",
            "8:60-8:64 from is a keyword of IEC 61131-3 and cannot be used as a name",
        ]
    );
    // A keyword nobody declared is not a name where one is used.
    assert_eq!(
        told("PROGRAM P VAR a : INT; END_VAR a := step; a := a + of; with := 1; END_PROGRAM"),
        vec![
            "1:37-1:41 expected a value, found 'step'",
            "1:52-1:54 expected a value, found 'of'",
            "1:56-1:60 expected a name, found 'with'",
        ]
    );
}

#[test]
fn a_name_the_standard_gives_its_own_variables_and_blocks_is_not_a_keyword() {
    // The inputs and outputs of the standard function blocks (Tables 43 to 46), the formal
    // parameters of standard functions (Tables 30 and 34), EN and ENO (§6.6.1.5), the
    // inputs of a TASK (Table 63) and the action qualifiers (Table 59) are names a program
    // may use. So is an instance named as its function block type.
    let src = "PROGRAM P
VAR
  s, r, s1, r1, q, q1, clk, cu, cd, ld, pv, cv, pt, et : INT;
  n, l, p, d, sd, ds, sl, en, eno, single, interval, priority : INT;
  st, jmp, cal, ret, add, eq : INT;
  ton : TON;
END_VAR
  s := r + ld + n + interval + st;
  ton(IN := TRUE, PT := T#1s);
END_PROGRAM";
    assert_eq!(told(src), Vec::<String>::new());
}

#[test]
fn a_type_or_pou_named_as_a_conversion_of_the_standard_is_still_refused_as_before() {
    // Not a keyword, so the parser says nothing; a keyword, so the checker says nothing.
    assert_eq!(
        told("FUNCTION_BLOCK INT END_FUNCTION_BLOCK FUNCTION INT_TO_REAL : INT INT_TO_REAL := 1; END_FUNCTION PROGRAM P END_PROGRAM"),
        vec![
            "1:16-1:19 INT is a keyword of IEC 61131-3 and cannot be used as a name",
            "1:48-1:59 INT_TO_REAL is a name of the standard; choose another",
        ]
    );
}
