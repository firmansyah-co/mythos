//! The compiler runs inside the logic module's service process, on a thread of a tokio
//! runtime, and every pass over a program is recursive. A source an engineer or an agent sends
//! must never be able to exhaust that thread's stack: the process would abort, and every
//! request with it. So a source nested deeper than [`MAX_NESTING`] (statements, operations,
//! calls, subscripts, parentheses) or with types nested deeper than [`MAX_TYPE_NESTING`] is
//! refused with a diagnostic where the limit is reached, and a structure that contains
//! itself is told before anything follows its members round.
//!
//! The smallest stack the compiler runs on is 2 MiB (tokio's worker threads, Rust's test
//! threads). The deepest source of each kind that is allowed is compiled here on a thread of
//! half that, in the debug build `cargo test` makes, whose frames are the largest: on every
//! system CI runs this on, it is the measurement that the limit leaves room.

use mythos_logic_compiler::{compile, Diagnostic, TargetProfile};
use mythos_logic_ir::{MAX_NESTING, MAX_TYPE_NESTING};

/// Half the smallest stack the compiler runs on.
const HALF_THE_SMALLEST_STACK: usize = 1 << 20;

/// Far beyond either limit.
const THOUSANDS: usize = 5_000;

/// A program around `body`, with what the bodies below use.
fn program(body: &str) -> String {
    format!(
        "FUNCTION F : INT\nVAR_INPUT a : INT; END_VAR\nF := a;\nEND_FUNCTION\n\
         PROGRAM Main\nVAR x : INT; b : BOOL; a : ARRAY [0..1] OF INT; END_VAR\n{body}\nEND_PROGRAM\n"
    )
}

/// A source of something `n` deep.
type Source = fn(usize) -> String;

/// One kind of nesting of a body: its name, what opens one level of it, and a source with
/// `n` levels.
struct Kind {
    name: &'static str,
    opens: &'static str,
    source: Source,
}

fn kinds() -> Vec<Kind> {
    vec![
        Kind {
            name: "parentheses",
            opens: "(",
            source: |n| program(&format!("x := {}x{};", "(".repeat(n), ")".repeat(n))),
        },
        Kind {
            name: "negations",
            opens: "- ",
            source: |n| program(&format!("x := {}x;", "- ".repeat(n))),
        },
        Kind {
            name: "NOT",
            opens: "NOT ",
            source: |n| program(&format!("b := {}b;", "NOT ".repeat(n))),
        },
        Kind {
            name: "a chain of additions",
            opens: " + x",
            source: |n| program(&format!("x := x{};", " + x".repeat(n))),
        },
        Kind {
            name: "a chain of OR between comparisons",
            opens: " OR ",
            source: |n| program(&format!("b := x > 0{};", " OR x > 0".repeat(n))),
        },
        Kind {
            name: "IF",
            opens: "IF b THEN ",
            source: |n| {
                program(&format!(
                    "{}x := 1;{}",
                    "IF b THEN ".repeat(n),
                    " END_IF;".repeat(n)
                ))
            },
        },
        Kind {
            name: "CASE",
            opens: "CASE x OF 1: ",
            source: |n| {
                program(&format!(
                    "{}x := 1;{}",
                    "CASE x OF 1: ".repeat(n),
                    " END_CASE;".repeat(n)
                ))
            },
        },
        Kind {
            name: "FOR",
            opens: "FOR x := 1 TO 2 DO ",
            source: |n| {
                program(&format!(
                    "{}b := TRUE;{}",
                    "FOR x := 1 TO 2 DO ".repeat(n),
                    " END_FOR;".repeat(n)
                ))
            },
        },
        Kind {
            name: "WHILE",
            opens: "WHILE b DO ",
            source: |n| {
                program(&format!(
                    "{}b := FALSE;{}",
                    "WHILE b DO ".repeat(n),
                    " END_WHILE;".repeat(n)
                ))
            },
        },
        Kind {
            name: "REPEAT",
            opens: "REPEAT ",
            source: |n| {
                program(&format!(
                    "{}b := TRUE;{}",
                    "REPEAT ".repeat(n),
                    " UNTIL b END_REPEAT;".repeat(n)
                ))
            },
        },
        Kind {
            name: "calls",
            opens: "F(",
            source: |n| program(&format!("x := {}x{};", "F(".repeat(n), ")".repeat(n))),
        },
        Kind {
            name: "subscripts",
            opens: "a[",
            source: |n| program(&format!("x := {}0{};", "a[".repeat(n), "]".repeat(n))),
        },
        Kind {
            name: "ARRAY OF in a declaration",
            opens: "ARRAY [0..0] OF ",
            source: |n| {
                format!(
                    "PROGRAM Main\nVAR v : {}INT; END_VAR\nEND_PROGRAM\n",
                    "ARRAY [0..0] OF ".repeat(n)
                )
            },
        },
        Kind {
            name: "repeated initial values",
            opens: "1(",
            source: |n| {
                format!(
                    "PROGRAM Main\nVAR v : ARRAY [0..0] OF INT := [{}0{}]; END_VAR\nEND_PROGRAM\n",
                    "1(".repeat(n),
                    ")".repeat(n)
                )
            },
        },
    ]
}

/// Types each holding the one before, `n` of them, named `{prefix}0` to `{prefix}{n}`; the
/// program holds the last. `reversed` declares the outermost first.
fn structures(n: usize, reversed: bool) -> String {
    let mut decls: Vec<String> = (1..=n)
        .map(|i| format!("S{i} : STRUCT s : S{}; END_STRUCT;", i - 1))
        .collect();
    decls.insert(0, "S0 : STRUCT a : INT; END_STRUCT;".into());
    if reversed {
        decls.reverse();
    }
    format!(
        "TYPE\n{}\nEND_TYPE\nPROGRAM Main\nVAR x : S{n}; END_VAR\nEND_PROGRAM\n",
        decls.join("\n")
    )
}

fn array_types(n: usize, reversed: bool) -> String {
    let mut decls: Vec<String> = (1..=n)
        .map(|i| format!("A{i} : ARRAY [0..0] OF A{};", i - 1))
        .collect();
    decls.insert(0, "A0 : ARRAY [0..0] OF INT;".into());
    if reversed {
        decls.reverse();
    }
    format!(
        "TYPE\n{}\nEND_TYPE\nPROGRAM Main\nVAR x : A{n}; END_VAR\nEND_PROGRAM\n",
        decls.join("\n")
    )
}

fn function_blocks(n: usize) -> String {
    let mut s =
        String::from("FUNCTION_BLOCK B0\nVAR q : INT; END_VAR\nq := 1;\nEND_FUNCTION_BLOCK\n");
    for i in 1..=n {
        s += &format!(
            "FUNCTION_BLOCK B{i}\nVAR b : B{}; END_VAR\nb();\nEND_FUNCTION_BLOCK\n",
            i - 1
        );
    }
    s + &format!("PROGRAM Main\nVAR x : B{n}; END_VAR\nx();\nEND_PROGRAM\n")
}

/// Both limits at once: a variable that is an array, `MAX_TYPE_NESTING - 1` deep, of the
/// deepest structure allowed.
fn arrays_of_the_deepest_structure(n: usize) -> String {
    let deepest = structures(MAX_TYPE_NESTING - 1, false);
    let (types, _) = deepest.split_once("PROGRAM").unwrap();
    format!(
        "{types}PROGRAM Main\nVAR v : {}S{}; END_VAR\nEND_PROGRAM\n",
        "ARRAY [0..0] OF ".repeat(n),
        MAX_TYPE_NESTING - 1
    )
}

fn compile_on(stack: usize, source: String) -> Result<(), Vec<Diagnostic>> {
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || compile(&source, &TargetProfile::pc()).map(|_| ()))
        .unwrap()
        .join()
        .unwrap()
}

fn says_too_deep(d: &Diagnostic) -> bool {
    d.message
        .contains(&format!("nested more than {MAX_NESTING} deep"))
        || d.message
            .contains(&format!("nested more than {MAX_TYPE_NESTING} deep"))
}

/// The text of `source` before where `d` starts.
fn before<'s>(source: &'s str, d: &Diagnostic) -> &'s str {
    let mut at = 0;
    for (i, line) in source.split('\n').enumerate() {
        if i + 1 == d.line as usize {
            let col: usize = line
                .chars()
                .take(d.col as usize - 1)
                .map(char::len_utf8)
                .sum();
            return &source[..at + col];
        }
        at += line.len() + 1;
    }
    source
}

/// The deepest `source(n)` that compiles, looked for from just above `limit` down.
fn deepest_allowed(limit: usize, source: impl Fn(usize) -> String) -> usize {
    (1..=limit + 1)
        .rev()
        .find(|n| compile(&source(*n), &TargetProfile::pc()).is_ok())
        .unwrap()
}

#[test]
fn a_body_nested_thousands_deep_is_refused_once_where_the_limit_is_reached() {
    for kind in kinds() {
        let source = (kind.source)(THOUSANDS);
        let diags = compile_on(HALF_THE_SMALLEST_STACK, source.clone()).expect_err(kind.name);
        assert_eq!(diags.len(), 1, "{}: {diags:?}", kind.name);
        let d = &diags[0];
        assert!(says_too_deep(d), "{}: {d}", kind.name);
        // Where it was reached: as many levels opened before it as the limit allows, give
        // or take what the statement around it counts.
        let opened = before(&source, d).matches(kind.opens).count();
        assert!(
            (MAX_NESTING - 4..=MAX_NESTING + 1).contains(&opened),
            "{}: told after {opened} levels: {d}",
            kind.name
        );
    }
}

#[test]
fn the_deepest_body_allowed_compiles_on_half_the_smallest_stack() {
    for kind in kinds() {
        let n = deepest_allowed(MAX_NESTING, kind.source);
        // A realistic program never meets the limit; the limit is what the docs say.
        assert!(
            n >= MAX_NESTING - 3,
            "{}: only {n} deep compiles",
            kind.name
        );
        let deeper = compile_on(HALF_THE_SMALLEST_STACK, (kind.source)(n + 1)).unwrap_err();
        assert!(
            deeper.iter().any(says_too_deep),
            "{}: {deeper:?}",
            kind.name
        );
        assert_eq!(
            compile_on(HALF_THE_SMALLEST_STACK, (kind.source)(n)),
            Ok(()),
            "{} {n} deep",
            kind.name
        );
    }
}

#[test]
fn types_nested_thousands_deep_are_refused_and_told_once_per_chain() {
    let refused = |source: String| compile_on(HALF_THE_SMALLEST_STACK, source).unwrap_err();
    let one = |diags: Vec<Diagnostic>| {
        assert_eq!(diags.len(), 1, "{diags:?}");
        diags[0].message.clone()
    };
    let deepest = format!("S{MAX_TYPE_NESTING}");
    assert!(
        one(refused(structures(THOUSANDS, false))).starts_with(&format!(
            "the structure {deepest} is nested more than {MAX_TYPE_NESTING} deep"
        ))
    );
    assert!(one(refused(structures(THOUSANDS, true)))
        .starts_with(&format!("the structure {deepest} is nested more than")));
    assert!(
        one(refused(function_blocks(THOUSANDS))).starts_with(&format!(
            "the function block B{MAX_TYPE_NESTING} is nested more than {MAX_TYPE_NESTING} deep"
        ))
    );
    assert!(one(refused(array_types(THOUSANDS, false)))
        .starts_with("this array type is nested more than"));
    // Declared outermost first, each named type is worked out from the outside in: every
    // one too deep is told where the limit is reached from it, and all of them are refused.
    let diags = refused(array_types(THOUSANDS, true));
    assert!(diags.iter().all(says_too_deep), "{diags:?}");
    assert!(
        diags.len() <= THOUSANDS / MAX_TYPE_NESTING + 1,
        "{}",
        diags.len()
    );
}

#[test]
fn the_deepest_types_allowed_compile_on_half_the_smallest_stack() {
    let cases: [(&str, Source); 4] = [
        ("structures", |n| structures(n, false)),
        ("structures declared outermost first", |n| {
            structures(n, true)
        }),
        ("named array types", |n| array_types(n, false)),
        ("function blocks", function_blocks),
    ];
    for (name, source) in cases {
        let n = deepest_allowed(MAX_TYPE_NESTING, source);
        assert!(n >= MAX_TYPE_NESTING - 2, "{name}: only {n} deep compiles");
        assert_eq!(
            compile_on(HALF_THE_SMALLEST_STACK, source(n)),
            Ok(()),
            "{name} {n} deep"
        );
    }
    // Both limits at once: an array as deep as allowed of a structure as deep as allowed.
    let n = deepest_allowed(MAX_TYPE_NESTING, arrays_of_the_deepest_structure);
    assert!(n >= MAX_TYPE_NESTING - 2, "only {n} arrays deep compiles");
    assert_eq!(
        compile_on(HALF_THE_SMALLEST_STACK, arrays_of_the_deepest_structure(n)),
        Ok(())
    );
}

#[test]
fn a_structure_that_contains_itself_is_told_and_nothing_follows_it_round() {
    for (source, told) in [
        (
            "TYPE L : STRUCT a : INT; r : L; END_STRUCT; END_TYPE\n",
            "the structure L contains itself: L holds itself",
        ),
        (
            "TYPE A : STRUCT b : B; END_STRUCT; B : STRUCT x : INT; c : A; d : A; END_STRUCT; END_TYPE\n",
            "the structure A contains itself: A holds B, which holds A",
        ),
        (
            "TYPE L : STRUCT a : INT; r : ARRAY [0..1] OF L; END_STRUCT; END_TYPE\n",
            "the structure L contains itself: L holds itself",
        ),
    ] {
        // Its uses say nothing more: what follows from the one mistake is not another.
        let first = source.split_whitespace().nth(1).unwrap();
        let source = format!(
            "{source}PROGRAM Main\nVAR x : {first}; y : ARRAY [0..1] OF {first}; END_VAR\nx.a := 1;\nEND_PROGRAM\n"
        );
        let diags = compile_on(HALF_THE_SMALLEST_STACK, source).unwrap_err();
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].message, told);
        let d = &diags[0];
        assert_eq!((d.line, d.col, d.end_line, d.end_col), (0, 0, 0, 0), "{d}");
    }
}

#[test]
fn a_chain_of_thousands_of_functions_each_calling_the_next_compiles() {
    // Calls of one function by the next do not nest in the tree, so nothing refuses them;
    // the order the POUs are emitted in is found without recursion. (The recursion it
    // replaced did not overflow at this length either, nor at 10,000 on a 1 MiB thread:
    // this guards that a long chain keeps compiling; it is not a proof of that change.)
    let mut s =
        String::from("FUNCTION G0 : INT\nVAR_INPUT a : INT; END_VAR\nG0 := a;\nEND_FUNCTION\n");
    for i in 1..=THOUSANDS {
        s += &format!(
            "FUNCTION G{i} : INT\nVAR_INPUT a : INT; END_VAR\nG{i} := G{}(a);\nEND_FUNCTION\n",
            i - 1
        );
    }
    s += &format!("PROGRAM Main\nVAR x : INT; END_VAR\nx := G{THOUSANDS}(x);\nEND_PROGRAM\n");
    assert_eq!(compile_on(HALF_THE_SMALLEST_STACK, s), Ok(()));
}
