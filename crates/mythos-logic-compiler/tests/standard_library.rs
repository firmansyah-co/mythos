//! The standard library written in Structured Text (ADR-013 L4): what it declares, what a
//! program takes of it, the names it keeps to itself, the blocks the native ones did not
//! have, and a timer's state across a warm restart.

mod support;
use support::*;

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_st::ast::PouKind;
use mythos_logic_st::library::{self, FILES, FILE_LINES, POUS};
use mythos_logic_vm::Value;

/// The members of a block of the library as a program would declare them, by kind.
fn members(name: &str) -> &'static str {
    match name {
        "SR" => "S1 R Q1",
        "RS" => "S R1 Q1",
        "R_TRIG" | "F_TRIG" => "CLK Q",
        n if n.starts_with("CTUD") => "CU CD R LD PV QU QD CV",
        n if n.starts_with("CTU") => "CU R PV Q CV",
        n if n.starts_with("CTD") => "CD LD PV Q CV",
        _ => "IN PT Q ET",
    }
}

#[test]
fn the_index_is_what_the_files_declare_and_every_file_reads_cleanly() {
    let mut declared: Vec<(String, PouKind, usize)> = Vec::new();
    for (k, file) in FILES.iter().enumerate() {
        let (src, diags) = library::read(k);
        assert!(diags.is_empty(), "{}: {diags:?}", file.name);
        assert!(
            src.types.is_empty(),
            "{}: the library declares no TYPE",
            file.name
        );
        assert!(src.lost.is_empty(), "{}", file.name);
        // Lines of one file stay below the next file's first.
        assert!(
            (file.text.lines().count() as u32) < FILE_LINES,
            "{}",
            file.name
        );
        for p in &src.pous {
            assert_ne!(p.kind, PouKind::Program, "{}: {}", file.name, p.name);
            assert!(p.library);
            assert!(p.span.line >= library::first_line(k));
            declared.push((p.name.to_ascii_uppercase(), p.kind, k));
        }
    }
    let index: Vec<(String, PouKind, usize)> = POUS
        .iter()
        .map(|(n, kind, k)| (n.to_string(), *kind, *k))
        .collect();
    assert_eq!(declared, index);
    assert!(FILES.len() < (u32::MAX / FILE_LINES) as usize);
}

#[test]
fn every_block_of_the_library_compiles_and_runs_in_a_program() {
    for (name, _, _) in POUS {
        let names: Vec<&str> = members(name).split(' ').collect();
        let src = format!("PROGRAM Main VAR b : {name}; END_VAR b(); END_PROGRAM");
        let mut r = Run::new(&src);
        r.scan_at(0);
        r.scan_at(10);
        for m in names {
            r.get(&format!("b.{m}"));
        }
    }
}

#[test]
fn a_program_takes_only_the_blocks_it_uses() {
    let c = compile(
        "PROGRAM Main VAR t : TON; c : ARRAY [1..2] OF CTU_DINT; END_VAR t(IN := TRUE); END_PROGRAM",
        &TargetProfile::pc(),
    )
    .unwrap();
    let mut blocks: Vec<&str> = c.types.iter().map(|t| t.name.as_str()).collect();
    blocks.sort();
    assert_eq!(blocks, ["CTU_DINT", "TON"]);

    let bare = compile(
        "PROGRAM Main VAR x : INT; END_VAR x := 1; END_PROGRAM",
        &TargetProfile::pc(),
    )
    .unwrap();
    assert!(bare.types.is_empty());
}

#[test]
fn a_program_cannot_declare_a_pou_or_a_type_with_a_name_of_the_library() {
    // Each by its own range, used by the program or not.
    for (decl, at) in [
        (
            "FUNCTION_BLOCK TON VAR_INPUT IN : BOOL; END_VAR END_FUNCTION_BLOCK",
            16,
        ),
        ("FUNCTION ctud_lint : INT ctud_lint := 1; END_FUNCTION", 10),
        ("TYPE TOF_TIME : STRUCT a : INT; END_STRUCT; END_TYPE", 6),
    ] {
        let src = format!("{decl}\nPROGRAM Main VAR t : TON; END_VAR t(IN := TRUE); END_PROGRAM");
        let d = match compile(&src, &TargetProfile::pc()) {
            Ok(_) => panic!("compiled: {decl}"),
            Err(d) => d,
        };
        assert_eq!(d.len(), 1, "{d:?}");
        let name = decl.split_whitespace().nth(1).unwrap();
        assert_eq!(
            (d[0].line, d[0].col, d[0].end_line, d[0].end_col),
            (1, at, 1, at + name.len() as u32),
            "{decl}"
        );
        assert_eq!(
            d[0].message,
            format!("{name} is a name of the standard; choose another")
        );
    }
}

#[test]
fn nothing_a_program_declares_reaches_into_the_library() {
    // The names of the timers' and counters' own variables, as the program's function,
    // type and variables: the library still means what it says.
    let mut r = Run::new(
        "FUNCTION RUNNING : BOOL RUNNING := TRUE; END_FUNCTION
         FUNCTION CU_M : INT CU_M := 7; END_FUNCTION
         TYPE START : STRUCT x : INT; END_STRUCT; END_TYPE
         TYPE CLOCK : ARRAY [1..2] OF INT; END_TYPE
         PROGRAM Main
         VAR t : TON; c : CTU; m : INT; in_m : BOOL; s : START; k : CLOCK; END_VAR
           t(IN := TRUE, PT := T#50ms);
           c(CU := t.Q, PV := 2);
           m := CU_M();
           in_m := RUNNING();
         END_PROGRAM",
    );
    r.scan_at(0);
    assert!(!r.bool("t.Q"));
    r.scan_at(60);
    assert!(r.bool("t.Q"));
    assert_eq!(r.int("t.ET"), 50 * MS);
    assert_eq!(r.int("c.CV"), 1);
    assert_eq!(r.int("m"), 7);
    assert!(r.bool("in_m"));
}

#[test]
fn a_program_cannot_name_an_intrinsic() {
    // IEC 61131-3 ed. 3 §6.1.2: two underscores together are not a name. The diagnostic is
    // the program's own, at its own range.
    let d = match compile(
        "PROGRAM Main VAR t : TIME; END_VAR t := __NOW(); END_PROGRAM",
        &TargetProfile::pc(),
    ) {
        Ok(_) => panic!("a program read the clock"),
        Err(d) => d,
    };
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(
        (d[0].line, d[0].col, d[0].end_line, d[0].end_col),
        (1, 41, 1, 46)
    );
    assert!(
        d[0].message.starts_with("__NOW is not a name"),
        "{}",
        d[0].message
    );
}

#[test]
fn a_diagnostic_about_the_library_never_points_into_the_program() {
    let k = FILES.len() - 1;
    let about = mythos_logic_ir::Diagnostic {
        line: library::first_line(k) + 11,
        col: 5,
        end_line: library::first_line(k) + 11,
        end_col: 9,
        message: "something".into(),
    };
    let user = mythos_logic_ir::Diagnostic {
        line: 12,
        col: 5,
        end_line: 12,
        end_col: 9,
        message: "of the program".into(),
    };
    let out = library::as_the_compilers(vec![about, user.clone()]);
    assert_eq!(
        (out[0].line, out[0].col, out[0].end_line, out[0].end_col),
        (0, 0, 0, 0)
    );
    assert_eq!(
        out[0].message,
        format!(
            "the standard library does not compile ({} 12:5: something); this is a fault of the compiler, not of the program",
            FILES[k].name
        )
    );
    assert_eq!(out[1], user);
    assert_eq!(
        library::place_of(FILE_LINES - 1),
        None,
        "a line of a program"
    );
}

#[test]
fn every_member_of_a_standard_instance_is_a_symbol_and_takes_the_instances_retention() {
    let c = compile(
        "PROGRAM Main
         VAR RETAIN kept : TON; END_VAR
         VAR plain : TON; END_VAR
         VAR NON_RETAIN edge : R_TRIG; END_VAR
           kept(IN := TRUE); plain(IN := TRUE); edge(CLK := TRUE);
         END_PROGRAM",
        &TargetProfile::pc(),
    )
    .unwrap();
    let of = |prefix: &str| -> Vec<(String, String, bool)> {
        c.symbols
            .iter()
            .filter(|s| s.path.starts_with(prefix))
            .map(|s| (s.path.clone(), s.ty.clone(), s.retain))
            .collect()
    };
    let timer = |name: &str, retain: bool| -> Vec<(String, String, bool)> {
        [
            ("IN", "BOOL"),
            ("PT", "TIME"),
            ("Q", "BOOL"),
            ("ET", "TIME"),
            ("RUNNING", "BOOL"),
            ("START", "TIME"),
            ("CLOCK", "TIME"),
        ]
        .iter()
        .map(|(m, t)| (format!("{name}.{m}"), t.to_string(), retain))
        .collect()
    };
    assert_eq!(of("kept."), timer("kept", true));
    assert_eq!(of("plain."), timer("plain", false));
    assert_eq!(
        of("edge."),
        [
            ("edge.CLK".to_string(), "BOOL".to_string(), false),
            ("edge.Q".to_string(), "BOOL".to_string(), false),
            ("edge.M".to_string(), "BOOL".to_string(), false),
        ]
    );
    // None of them is the host's to set: they are the block's.
    assert!(c.symbols.iter().all(|s| !s.writable));
}

#[test]
fn the_internal_state_of_a_standard_block_is_private_as_a_user_blocks_is() {
    let d = refusal(
        "PROGRAM Main VAR t : TON; x : BOOL; END_VAR t(IN := TRUE); x := t.RUNNING; END_PROGRAM",
    );
    assert_eq!(
        d,
        "1:67: RUNNING is an internal variable of TON; only its inputs and outputs can be named from outside"
    );
    let d = refusal("PROGRAM Main VAR t : TON; x : BOOL; END_VAR x := t; END_PROGRAM");
    assert_eq!(
        d,
        "1:50: t is a function block instance; name one of its outputs, like t.Q"
    );
}

#[test]
fn the_typed_counters_count_to_the_limits_of_their_type() {
    // Table 45: CV < PVmax, CV > PVmin, the limits of CV's type.
    for (block, up, start, limit) in [
        ("CTU_INT", true, i16::MAX as i64 - 1, i16::MAX as i64),
        ("CTU_DINT", true, i32::MAX as i64 - 1, i32::MAX as i64),
        ("CTU_LINT", true, i64::MAX - 1, i64::MAX),
        ("CTD_INT", false, i16::MIN as i64 + 1, i16::MIN as i64),
        ("CTD_DINT", false, i32::MIN as i64 + 1, i32::MIN as i64),
        ("CTD_LINT", false, i64::MIN + 1, i64::MIN),
    ] {
        let (pulse, load) = if up { ("CU", "R") } else { ("CD", "LD") };
        let ty = block.split('_').nth(1).unwrap();
        let mut r = Run::new(&format!(
            "PROGRAM Main
             VAR_INPUT p : BOOL; pv : {ty}; l : BOOL; END_VAR
             VAR c : {block}; END_VAR
               c({pulse} := p, {load} := l, PV := pv);
             END_PROGRAM"
        ));
        let value = |v: i64| match ty {
            "INT" => Value::Int(v as i16),
            "DINT" => Value::Dint(v as i32),
            _ => Value::Lint(v),
        };
        // Past 32,767 for the wider types, where an INT counter would stop.
        if up {
            r.set("pv", value(start));
            r.set("c.CV", value(start));
        } else {
            r.set("pv", value(start));
            r.set("l", Value::Bool(true));
            r.scan_at(0);
            r.set("l", Value::Bool(false));
        }
        for k in 0..6 {
            r.set("p", Value::Bool(k % 2 == 0));
            r.scan_at(k);
        }
        assert_eq!(r.int("c.CV"), limit, "{block}");
        assert!(r.bool("c.Q"), "{block}");
    }
}

#[test]
fn ctud_counts_up_and_down_as_table_45_writes_it() {
    let mut r = Run::new(
        "PROGRAM Main
         VAR_INPUT cu, cd, r, ld : BOOL; END_VAR
         VAR c : CTUD; END_VAR
           c(CU := cu, CD := cd, R := r, LD := ld, PV := 2);
         END_PROGRAM",
    );
    // (CU, CD, R, LD) -> (CV, QU, QD)
    let steps = [
        ((false, false, false, false), (0, false, true)),
        ((true, false, false, false), (1, false, false)),
        ((true, false, false, false), (1, false, false)), // held: no edge
        ((false, false, false, false), (1, false, false)),
        ((true, false, false, false), (2, true, false)),
        ((false, true, false, false), (1, false, false)),
        ((false, false, false, false), (1, false, false)),
        ((true, true, false, false), (1, false, false)), // both edges at once: neither
        ((true, false, false, false), (1, false, false)), // CD fell, CU held: no edge
        ((false, false, false, false), (1, false, false)),
        ((false, true, false, false), (0, false, true)),
        ((false, false, false, false), (0, false, true)),
        ((false, true, false, false), (-1, false, true)), // below 0, to PVmin
        ((false, false, false, true), (2, true, false)),  // LD: CV := PV
        ((true, false, true, true), (0, false, true)),    // R before LD, and the edge spent
        ((false, false, false, false), (0, false, true)),
    ];
    for (k, ((cu, cd, reset, ld), (cv, qu, qd))) in steps.into_iter().enumerate() {
        r.set("cu", Value::Bool(cu));
        r.set("cd", Value::Bool(cd));
        r.set("r", Value::Bool(reset));
        r.set("ld", Value::Bool(ld));
        r.scan_at(k as i64);
        assert_eq!(
            (r.int("c.CV"), r.bool("c.QU"), r.bool("c.QD")),
            (cv, qu, qd),
            "step {k}"
        );
    }
}

#[test]
fn the_time_variants_are_the_timers_of_time() {
    for (variant, plain) in [("TP_TIME", "TP"), ("TON_TIME", "TON"), ("TOF_TIME", "TOF")] {
        let mut r = Run::new(&format!(
            "PROGRAM Main
             VAR_INPUT x : BOOL; END_VAR
             VAR a : {variant}; b : {plain}; END_VAR
               a(IN := x, PT := T#30ms);
               b(IN := x, PT := T#30ms);
             END_PROGRAM"
        ));
        for k in 0..40 {
            r.set("x", Value::Bool(k % 9 < 4));
            r.scan_at(k * 7);
            assert_eq!(r.bool("a.Q"), r.bool("b.Q"), "{variant} at {k}");
            assert_eq!(r.int("a.ET"), r.int("b.ET"), "{variant} at {k}");
        }
    }
}

/// A warm restart, as the runtime makes one: the instance's state is kept (RETAIN) and the
/// scan clock starts again from zero.
#[test]
fn a_retained_timer_carries_on_from_its_et_when_the_clock_starts_again() {
    let src = "PROGRAM Main
               VAR_INPUT x : BOOL; END_VAR
               VAR RETAIN ondelay : TON; off : TOF; pulse : TP; END_VAR
                 ondelay(IN := x, PT := T#1s);
                 off(IN := NOT x, PT := T#1s);
                 pulse(IN := x, PT := T#1s);
               END_PROGRAM";
    // The same program, a timer 400 ms into its PT: started again, the clock is 0.
    let mut r = Run::new(src);
    r.set("x", Value::Bool(true));
    r.scan_at(5_000);
    r.scan_at(5_400);
    assert_eq!(r.int("ondelay.ET"), 400 * MS);
    assert_eq!(r.int("pulse.ET"), 400 * MS);
    assert!(r.bool("pulse.Q"));
    // Warm restart: every variable kept, the clock from 0 again, IN still TRUE.
    for (at, et) in [
        (0, 400),
        (0, 400),
        (100, 500),
        (599, 999),
        (600, 1_000),
        (5_000, 1_000),
    ] {
        r.scan_at(at);
        let on = r.int("ondelay.ET");
        assert!(
            (0..=1_000 * MS).contains(&on),
            "never negative, never past PT: {on}"
        );
        assert_eq!(on, et * MS, "on at {at} ms after the restart");
        assert_eq!(r.bool("ondelay.Q"), et == 1_000, "on at {at}");
        // The pulse began with the timer: it ends with it, not a full PT after the restart.
        assert_eq!(r.int("pulse.ET"), et * MS, "pulse at {at}");
        assert_eq!(r.bool("pulse.Q"), et < 1_000, "pulse at {at}");
    }

    // TOF: IN fell 300 ms before the stop; started again, it holds Q for the 700 ms left.
    let mut r = Run::new(src);
    r.set("x", Value::Bool(false));
    r.scan_at(5_000); // off: IN (NOT x) TRUE
    r.set("x", Value::Bool(true));
    r.scan_at(7_000); // off: IN falls at 7.0 s
    r.scan_at(7_300);
    assert_eq!(r.int("off.ET"), 300 * MS);
    for (at, et, q) in [
        (0, 300, true),
        (50, 350, true),
        (699, 999, true),
        (700, 1_000, false),
    ] {
        r.scan_at(at);
        assert_eq!(r.int("off.ET"), et * MS, "off at {at}");
        assert_eq!(r.bool("off.Q"), q, "off at {at}");
    }

    // A restart whose clock is already past the last call's when the timer is first called
    // again cannot be told from running on: ET grows by the clock less the last call's
    // (here 50 ms), never below zero nor past PT.
    let mut r = Run::new(src);
    r.set("x", Value::Bool(true));
    r.scan_at(0);
    r.scan_at(200);
    r.scan_at(250);
    assert_eq!(r.int("ondelay.ET"), 250 * MS);
}

#[test]
fn a_timer_whose_start_is_more_than_a_time_can_hold_before_the_clock_is_done() {
    // ET is CLOCK - START, and a difference past what a TIME holds is past any PT, as a
    // saturating difference would be; never negative.
    let mut r =
        Run::new("PROGRAM Main VAR RETAIN t : TON; END_VAR t(IN := TRUE, PT := T#1s); END_PROGRAM");
    r.scan_at(0);
    r.set("t.START", Value::Time(i64::MIN + 5));
    r.scan_at(10);
    assert_eq!(r.int("t.ET"), 1_000 * MS);
    assert!(r.bool("t.Q"));
}
