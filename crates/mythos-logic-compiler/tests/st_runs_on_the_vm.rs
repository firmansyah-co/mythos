//! Structured Text compiled and run on the VM, scan by scan, the way a runner does it.

use mythos_logic_compiler::{compile, Compiled, TargetProfile};
use mythos_logic_vm::{cold_start, read, scan, write, Fault, Frame, Program, Value};

const MS: i64 = 1_000_000;
const BUDGET: u32 = 1_000_000;

struct Run {
    c: Compiled,
    memory: Vec<u8>,
    stack: Vec<u64>,
    calls: Vec<Frame>,
    now: i64,
}

impl Run {
    fn new(src: &str) -> Run {
        let c = compile(src, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
        let p = Program::load(&c.bytecode).expect("the VM loads what the compiler made");
        let mut memory = vec![0; p.memory_size()];
        cold_start(&p, &mut memory).unwrap();
        let stack = vec![0; p.stack_depth()];
        let calls = vec![Frame::default(); p.call_depth()];
        Run {
            c,
            memory,
            stack,
            calls,
            now: 0,
        }
    }
    fn scan(&mut self) -> Result<(), Fault> {
        let p = Program::load(&self.c.bytecode).unwrap();
        scan(
            &p,
            &mut self.memory,
            &mut self.stack,
            &mut self.calls,
            self.now,
            BUDGET,
        )
        .map(|_| ())
    }
    fn scan_at(&mut self, now_ms: i64) {
        self.now = now_ms * MS;
        self.scan().unwrap();
    }
    fn get(&self, path: &str) -> Value {
        let s = self
            .c
            .symbol(path)
            .unwrap_or_else(|| panic!("no symbol {path}"));
        read(&self.memory, s.offset, elem(&s.ty)).unwrap()
    }
    fn set(&mut self, path: &str, v: Value) {
        let s = self.c.symbol(path).unwrap();
        assert!(write(&mut self.memory, s.offset, v));
    }
}

fn elem(ty: &str) -> mythos_logic_vm::ElemType {
    use mythos_logic_vm::ElemType as E;
    match ty {
        "BOOL" => E::Bool,
        "SINT" => E::Sint,
        "INT" => E::Int,
        "DINT" => E::Dint,
        "LINT" => E::Lint,
        "REAL" => E::Real,
        "LREAL" => E::Lreal,
        "TIME" => E::Time,
        other => panic!("{other}"),
    }
}

fn diagnostics(src: &str) -> Vec<String> {
    match compile(src, &TargetProfile::pc()) {
        Ok(_) => panic!("compiled, but should not have"),
        Err(d) => d.iter().map(|d| d.to_string()).collect(),
    }
}

#[test]
fn a_motor_starts_after_its_delay_and_stops_when_asked() {
    let mut r = Run::new(
        "PROGRAM Pump
         VAR_INPUT start_pb, stop_pb : BOOL; END_VAR
         VAR_OUTPUT motor : BOOL; END_VAR
         VAR running : BOOL; delay : TON; starts : CTU; edge : R_TRIG; END_VAR
           IF stop_pb THEN running := FALSE;
           ELSIF start_pb THEN running := TRUE;
           END_IF;
           delay(IN := running, PT := T#2s);
           motor := delay.Q;
           edge(CLK := motor);
           starts(CU := edge.Q, PV := 100);
         END_PROGRAM",
    );
    r.set("start_pb", Value::Bool(true));
    r.scan_at(0);
    assert_eq!(r.get("motor"), Value::Bool(false));
    r.set("start_pb", Value::Bool(false));
    r.scan_at(1999);
    assert_eq!(r.get("motor"), Value::Bool(false));
    assert_eq!(r.get("delay.ET"), Value::Time(1999 * MS));
    r.scan_at(2000);
    assert_eq!(r.get("motor"), Value::Bool(true));
    r.scan_at(2100);
    assert_eq!(
        r.get("starts.CV"),
        Value::Int(1),
        "one rising edge, one count"
    );
    r.set("stop_pb", Value::Bool(true));
    r.scan_at(2200);
    assert_eq!(r.get("motor"), Value::Bool(false));
}

#[test]
fn integer_arithmetic_wraps_at_the_width_of_its_type() {
    let mut r = Run::new(
        "PROGRAM P VAR i : INT := 32767; s : SINT := -128; d : DINT; big, inside : BOOL; END_VAR
           inside := i + 1 < 0;
           i := i + 1;
           s := s - 1;
           d := 2147483647; d := d + 1;
           big := INT_TO_DINT(i) + 1 > 0;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("i"), Value::Int(-32768));
    assert_eq!(r.get("s"), Value::Sint(127));
    assert_eq!(r.get("d"), Value::Dint(i32::MIN));
    assert_eq!(r.get("big"), Value::Bool(false), "-32768 + 1 is negative");
    assert_eq!(
        r.get("inside"),
        Value::Bool(true),
        "INT arithmetic wraps inside an expression too, not only when stored"
    );
}

#[test]
fn conversions_round_a_half_to_the_even_integer_and_narrowing_wraps() {
    // IEC 61131-3 ed. 3 Table 22 footnote b, with its own examples: 1.6 -> 2, -1.6 -> -2,
    // 1.5 -> 2, -1.5 -> -2; and so 2.5 -> 2, where rounding a half away from zero gives 3.
    let mut r = Run::new(
        "PROGRAM P VAR a, b, c, n, e1, e2, e3, e4, e5 : INT; d : DINT; x : REAL := 2.5; y : LREAL; f : BOOL; k : DINT := 70000; END_VAR
           a := REAL_TO_INT(x);
           b := REAL_TO_INT(-x);
           c := REAL_TO_INT(2.4);
           e1 := REAL_TO_INT(1.6);
           e2 := REAL_TO_INT(-1.6);
           e3 := REAL_TO_INT(1.5);
           e4 := REAL_TO_INT(-1.5);
           e5 := REAL_TO_INT(0.5);
           d := LREAL_TO_DINT(1000000.5);
           n := DINT_TO_INT(k);
           y := x;
           f := INT_TO_BOOL(a);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(
        r.get("a"),
        Value::Int(2),
        "2.5 is as near 2 as 3: the even one"
    );
    assert_eq!(r.get("b"), Value::Int(-2));
    assert_eq!(r.get("c"), Value::Int(2));
    assert_eq!(r.get("e1"), Value::Int(2));
    assert_eq!(r.get("e2"), Value::Int(-2));
    assert_eq!(r.get("e3"), Value::Int(2));
    assert_eq!(r.get("e4"), Value::Int(-2));
    assert_eq!(r.get("e5"), Value::Int(0));
    assert_eq!(r.get("d"), Value::Dint(1_000_000));
    assert_eq!(r.get("n"), Value::Int(4464), "70000 mod 65536");
    assert_eq!(
        r.get("y"),
        Value::Lreal(2.5),
        "REAL widens to LREAL without a conversion"
    );
    assert_eq!(r.get("f"), Value::Bool(true));
}

#[test]
fn case_for_while_repeat_and_exit_run_as_written() {
    let mut r = Run::new(
        "PROGRAM P
         VAR sel : INT; which : INT; sum : DINT; i : INT; n : INT; m : INT; END_VAR
           CASE sel OF
             1, 2: which := 10;
             3..5: which := 20;
           ELSE which := 99;
           END_CASE;
           sum := 0;
           FOR i := 1 TO 10 DO
             IF i = 7 THEN EXIT; END_IF;
             sum := sum + i;
           END_FOR;
           n := 0;
           WHILE n < 5 DO n := n + 2; END_WHILE;
           m := 0;
           REPEAT m := m + 1; UNTIL m >= 3 END_REPEAT;
         END_PROGRAM",
    );
    for (sel, which) in [(1, 10), (2, 10), (4, 20), (5, 20), (6, 99), (-1, 99)] {
        r.set("sel", Value::Int(sel));
        r.scan().unwrap();
        assert_eq!(r.get("which"), Value::Int(which), "sel {sel}");
    }
    assert_eq!(r.get("sum"), Value::Dint(21), "1..6, EXIT at 7");
    assert_eq!(r.get("n"), Value::Int(6));
    assert_eq!(r.get("m"), Value::Int(3));
}

#[test]
fn a_for_loop_counts_down_with_a_negative_step() {
    let mut r = Run::new(
        "PROGRAM P VAR i : DINT; last : DINT; count : DINT; END_VAR
           count := 0;
           FOR i := 10 TO 1 BY -3 DO last := i; count := count + 1; END_FOR;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("count"), Value::Dint(4), "10, 7, 4, 1");
    assert_eq!(r.get("last"), Value::Dint(1));
}

#[test]
fn return_ends_the_scan_there() {
    let mut r = Run::new(
        "PROGRAM P VAR a, b : INT; END_VAR
           a := 1;
           IF a = 1 THEN RETURN; END_IF;
           b := 1;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("b"), Value::Int(0));
}

#[test]
fn division_by_zero_stops_the_scan_with_a_fault_instead_of_a_value() {
    let mut r = Run::new("PROGRAM P VAR a, b : INT := 0; c : INT; END_VAR c := a / b; END_PROGRAM");
    assert!(matches!(r.scan(), Err(Fault::DivideByZero(_))));
}

#[test]
fn mod_by_zero_is_zero_and_division_truncates_toward_zero() {
    // IEC 61131-3 ed. 3 Table 29: footnote d (7/3 = 2, (-7)/3 = -2) and footnote e (MOD is
    // IN1 - (IN1/IN2)*IN2, and 0 when IN2 is 0).
    let mut r = Run::new(
        "PROGRAM P VAR z : INT; q1, q2, m1, m2, m3 : INT; big : LINT := 9; lz : LINT; lm : LINT := 5; END_VAR
           q1 := 7 / 3;
           q2 := -7 / 3;
           m1 := 7 MOD 3;
           m2 := -7 MOD 3;
           m3 := 7 MOD z;
           lm := big MOD lz;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("q1"), Value::Int(2));
    assert_eq!(r.get("q2"), Value::Int(-2));
    assert_eq!(r.get("m1"), Value::Int(1));
    assert_eq!(r.get("m2"), Value::Int(-1));
    assert_eq!(r.get("m3"), Value::Int(0), "MOD by zero is 0, not a fault");
    assert_eq!(r.get("lm"), Value::Lint(0));
}

#[test]
fn equality_and_comparison_share_one_precedence_applied_left_to_right() {
    // IEC 61131-3 ed. 3 Table 71: =, <>, <, >, <=, >= are all precedence 4.
    let mut r = Run::new(
        "PROGRAM P VAR a : INT := 1; b : INT := 2; flag : BOOL := TRUE; x, y : BOOL; END_VAR
           x := a < b = flag;
           y := a > b <> flag;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("x"), Value::Bool(true), "(a < b) = flag");
    assert_eq!(r.get("y"), Value::Bool(true), "(a > b) <> flag");

    // Written the other way round it is (flag = a) < b: a BOOL compared with an INT.
    let d = diagnostics(
        "PROGRAM P VAR a, b : INT; flag, x : BOOL; END_VAR x := flag = a < b; END_PROGRAM",
    );
    assert!(
        d[0].contains("a BOOL and a INT cannot be combined"),
        "{d:?}"
    );
}

#[test]
fn a_loop_that_never_ends_is_stopped_by_the_budget() {
    let mut r = Run::new(
        "PROGRAM P VAR a : DINT; END_VAR WHILE TRUE DO a := a + 1; END_WHILE; END_PROGRAM",
    );
    assert_eq!(r.scan(), Err(Fault::BudgetExceeded));
}

#[test]
fn time_arithmetic_and_comparison() {
    let mut r = Run::new(
        "PROGRAM P VAR t : TIME := T#1m; u : TIME; long : BOOL; END_VAR
           u := t * 2 + T#500ms;
           long := u > T#2m;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.get("u"), Value::Time(120_500 * MS));
    assert_eq!(r.get("long"), Value::Bool(true));
}

#[test]
fn mistakes_are_named_with_their_line_and_column() {
    let d = diagnostics("PROGRAM P\nVAR a : INT; END_VAR\n  a := b;\nEND_PROGRAM");
    assert_eq!(d, vec!["3:8: b is not declared"]);

    let d = diagnostics("PROGRAM P VAR a : INT; d : DINT; END_VAR a := d; END_PROGRAM");
    assert_eq!(d.len(), 1);
    assert!(
        d[0].contains("a DINT cannot be used as a INT without a conversion: write DINT_TO_INT"),
        "{d:?}"
    );

    let d = diagnostics("PROGRAM P VAR a : INT := 40000; END_VAR END_PROGRAM");
    assert!(d[0].contains("40000 is outside the range of INT"), "{d:?}");

    let d = diagnostics("PROGRAM P VAR t : TON; END_VAR t.Q := TRUE; END_PROGRAM");
    assert!(d[0].contains("t.Q is an output of the block"), "{d:?}");

    let d = diagnostics("PROGRAM P VAR a : BOOL; END_VAR IF 1 THEN a := TRUE; END_IF; END_PROGRAM");
    assert!(d[0].contains("a condition must be BOOL"), "{d:?}");

    let d = diagnostics("PROGRAM P VAR a : INT; END_VAR EXIT; END_PROGRAM");
    assert!(d[0].contains("EXIT outside a loop"), "{d:?}");
}

#[test]
fn what_is_not_supported_yet_is_refused_by_name_never_compiled_as_something_else() {
    for (src, said) in [
        (
            "PROGRAM P VAR a : STRING; END_VAR END_PROGRAM",
            "'STRING' is not supported yet",
        ),
        (
            "PROGRAM P VAR a AT %IX0.0 : BOOL; END_VAR END_PROGRAM",
            "bind a variable to a tag",
        ),
        (
            "PROGRAM P VAR RETAIN a : INT; END_VAR END_PROGRAM",
            "RETAIN is not supported yet",
        ),
        (
            "PROGRAM P VAR a : REAL; END_VAR a := 2.0 ** 2.0; END_PROGRAM",
            "** (power) is not supported yet",
        ),
        (
            "PROGRAM P VAR a : INT; END_VAR a := ABS(a); END_PROGRAM",
            "ABS is not a function this compiler knows yet",
        ),
        (
            "PROGRAM P VAR a : WORD; END_VAR END_PROGRAM",
            "WORD is not a type this compiler knows",
        ),
    ] {
        let d = diagnostics(src);
        assert!(
            d.iter().any(|m| m.contains(said)),
            "{src}: wanted '{said}', got {d:?}"
        );
    }
}

/// A profile as a runtime module's manifest will carry it (ADR-013 §2.2): data, not code.
fn profile(json: &str) -> TargetProfile {
    serde_json::from_str(json).unwrap()
}

#[test]
fn a_target_that_cannot_run_a_type_refuses_the_program_at_compile_time() {
    let no_lreal = profile(
        r#"{"name":"no-lreal","format_version":1,"max_memory":65536,"max_code":65536,"refuses":["Lreal"]}"#,
    );
    let err = compile("PROGRAM P VAR x : LREAL; END_VAR END_PROGRAM", &no_lreal)
        .err()
        .unwrap();
    assert_eq!(
        err[0].message,
        "LREAL is not supported on the target no-lreal"
    );
    assert!(
        compile("PROGRAM P VAR x : REAL; END_VAR END_PROGRAM", &no_lreal).is_ok(),
        "and what it can run compiles"
    );

    let tiny = profile(r#"{"name":"tiny","format_version":1,"max_memory":8,"max_code":65536}"#);
    let err = compile("PROGRAM P VAR t : TON; END_VAR END_PROGRAM", &tiny)
        .err()
        .unwrap();
    assert_eq!(
        err[0].message,
        "the program needs 28 bytes of variable memory; the target tiny allows 8"
    );
}

#[test]
fn a_target_whose_vm_reads_another_bytecode_format_is_refused_at_compile_time() {
    // Otherwise the refusal would come from the controller, at download or at a hot swap.
    let newer =
        profile(r#"{"name":"board-x","format_version":7,"max_memory":65536,"max_code":65536}"#);
    let err = compile("PROGRAM P VAR x : INT; END_VAR END_PROGRAM", &newer)
        .err()
        .unwrap();
    assert_eq!(
        err[0].message,
        "the target board-x runs bytecode format 7; this compiler makes format 1"
    );
}

#[test]
fn the_built_in_profiles_are_found_by_name_and_read_this_compilers_format() {
    for name in ["pc", "rp2040", "esp32"] {
        let p = TargetProfile::by_name(name).unwrap();
        assert!(
            compile("PROGRAM P VAR x : LREAL; END_VAR END_PROGRAM", &p).is_ok(),
            "{name}"
        );
    }
    assert!(TargetProfile::by_name("plc-9000").is_none());
}

#[test]
fn the_symbol_table_says_where_every_variable_and_member_is_and_which_the_host_may_set() {
    let c = compile(
        "PROGRAM P VAR_INPUT go : BOOL; END_VAR VAR_OUTPUT out : INT; END_VAR VAR t : TON; END_VAR END_PROGRAM",
        &TargetProfile::pc(),
    )
    .unwrap();
    let paths: Vec<(&str, &str, u32, bool)> = c
        .symbols
        .iter()
        .map(|s| (s.path.as_str(), s.ty.as_str(), s.offset, s.writable))
        .collect();
    assert_eq!(
        paths,
        vec![
            ("go", "BOOL", 0, true),
            ("out", "INT", 1, false),
            ("t.IN", "BOOL", 3, false),
            ("t.PT", "TIME", 7, false),
            ("t.Q", "BOOL", 4, false),
            ("t.ET", "TIME", 15, false),
        ]
    );
}
