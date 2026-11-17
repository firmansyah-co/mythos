//! Function blocks written in the source (IEC 61131-3 ed. 3 §6.6.3), compiled and run on the
//! VM: a type whose body is emitted once and runs on each instance's own variables.

mod support;
use support::*;

use mythos_logic_vm::Value;

const COUNTER: &str = "
    FUNCTION_BLOCK UpCounter
    VAR_INPUT step : INT := 1; reset : BOOL; END_VAR
    VAR_OUTPUT total : DINT; full : BOOL; END_VAR
    VAR calls : INT; limit : DINT := 10; END_VAR
      calls := calls + 1;
      IF reset THEN total := 0; ELSE total := total + step; END_IF;
      full := total >= limit;
    END_FUNCTION_BLOCK";

#[test]
fn a_function_block_keeps_its_variables_from_one_call_to_the_next() {
    let mut r = Run::new(&format!(
        "{COUNTER}
         PROGRAM Main VAR c : UpCounter; t : DINT; f : BOOL; END_VAR
           c(step := 3);
           t := c.total;
           f := c.full;
         END_PROGRAM"
    ));
    for (scan, total, full) in [(1, 3, false), (2, 6, false), (3, 9, false), (4, 12, true)] {
        r.scan().unwrap();
        assert_eq!(r.int("t"), total, "scan {scan}");
        assert_eq!(r.bool("f"), full, "scan {scan}");
    }
    assert_eq!(
        r.int("c.calls"),
        4,
        "an internal variable, read by the host"
    );
}

#[test]
fn two_instances_of_one_type_each_have_their_own_variables() {
    let mut r = Run::new(&format!(
        "{COUNTER}
         PROGRAM Main VAR a, b : UpCounter; ta, tb : DINT; END_VAR
           a(step := 1);
           b(step := 100);
           a(step := 1);
           ta := a.total;
           tb := b.total;
         END_PROGRAM"
    ));
    r.scan().unwrap();
    r.scan().unwrap();
    assert_eq!(r.int("ta"), 4, "four calls of a");
    assert_eq!(r.int("tb"), 200, "two calls of b");
    assert_eq!(r.int("a.calls"), 4);
    assert_eq!(r.int("b.calls"), 2);
}

#[test]
fn outputs_are_copied_out_by_the_arrow_and_widen_as_an_assignment_would() {
    let mut r = Run::new(&format!(
        "{COUNTER}
         PROGRAM Main VAR c : UpCounter; wide : LINT; flag : BOOL; grid : ARRAY [1..2] OF DINT; i : INT := 2; END_VAR
           c(step := 5, total => wide);
           c(step := 5, total => grid[i], full => flag);
         END_PROGRAM"
    ));
    r.scan().unwrap();
    assert_eq!(r.get("wide"), Value::Lint(5), "a DINT output into a LINT");
    assert_eq!(r.int("grid[2]"), 10, "into an element chosen at run time");
    assert!(r.bool("flag"), "10 is the limit");

    let d = refusal(&format!(
        "{COUNTER} PROGRAM Main VAR c : UpCounter; small : INT; END_VAR c(total => small); END_PROGRAM"
    ));
    assert!(
        d.contains("total is a DINT, which does not fit in a INT"),
        "{d}"
    );
}

#[test]
fn an_input_not_given_keeps_its_value_and_may_be_set_apart_from_the_call() {
    // IEC 61131-3 ed. 3 §6.6.3.4.1 rule 3: an input may be assigned apart from the call, to
    // take effect with the next call (b), and an input not assigned keeps its initial value
    // or that of the latest call (c). Unlike a function's (§6.6.1.4.2).
    let mut r = Run::new(&format!(
        "{COUNTER}
         PROGRAM Main VAR c : UpCounter; t0, t1, t2, t3 : DINT; END_VAR
           c();
           t0 := c.total;
           c(step := 10);
           t1 := c.total;
           c();
           t2 := c.total;
           c.step := 100;
           c();
           t3 := c.total;
           c.step := 1;
         END_PROGRAM"
    ));
    r.scan().unwrap();
    assert_eq!(r.int("t0"), 1, "step starts at its declared 1");
    assert_eq!(r.int("t1"), 11);
    assert_eq!(r.int("t2"), 21, "step is still the 10 of the call before");
    assert_eq!(r.int("t3"), 121, "set apart from the call");
}

#[test]
fn every_instance_starts_at_the_initial_values_its_type_declares() {
    let r = Run::new(&format!(
        "{COUNTER}
         FUNCTION_BLOCK Holder VAR inner : UpCounter; gain : REAL := 2.5; END_VAR END_FUNCTION_BLOCK
         PROGRAM Main VAR a : UpCounter; h : Holder; many : ARRAY [1..3] OF UpCounter; END_VAR
         END_PROGRAM"
    ));
    assert_eq!(r.int("a.step"), 1);
    assert_eq!(r.int("a.limit"), 10);
    assert_eq!(r.int("h.inner.limit"), 10, "an instance inside an instance");
    assert_eq!(r.get("h.gain"), Value::Real(2.5));
    assert_eq!(
        r.int("many[3].limit"),
        10,
        "each element of an array of instances"
    );
    assert_eq!(r.int("many[3].step"), 1);
}

const STATION: &str = "
    FUNCTION_BLOCK Debounce
    VAR_INPUT raw : BOOL; hold : TIME := T#50ms; END_VAR
    VAR_OUTPUT q : BOOL; END_VAR
    VAR on_delay : TON; END_VAR
      on_delay(IN := raw, PT := hold);
      q := on_delay.Q;
    END_FUNCTION_BLOCK
    FUNCTION_BLOCK Station
    VAR_INPUT button : BOOL; END_VAR
    VAR_OUTPUT presses : INT; pressed : BOOL; END_VAR
    VAR clean : Debounce; edge : R_TRIG; count : CTU; END_VAR
      clean(raw := button);
      edge(CLK := clean.q);
      count(CU := edge.Q, PV := 1000);
      presses := count.CV;
      pressed := clean.q;
    END_FUNCTION_BLOCK";

#[test]
fn a_block_holds_instances_of_other_blocks_and_each_outer_instance_has_its_own() {
    let mut r = Run::new(&format!(
        "{STATION}
         PROGRAM Main
         VAR_INPUT b1, b2 : BOOL; END_VAR
         VAR s1, s2 : Station; n1, n2 : INT; deep : BOOL; elapsed : TIME; END_VAR
           s1(button := b1, presses => n1);
           s2(button := b2, presses => n2);
           deep := s1.clean.on_delay.Q;
           elapsed := s2.clean.on_delay.ET;
         END_PROGRAM"
    ));
    // b1 is pressed for 60 ms twice; b2 only bounces for 20 ms.
    let presses = |t: i64| (t < 60) || (100..160).contains(&t);
    let mut t = 0;
    while t <= 200 {
        r.set("b1", Value::Bool(presses(t)));
        r.set("b2", Value::Bool(t < 20));
        r.scan_at(t);
        if t == 50 {
            assert!(
                r.bool("deep"),
                "s1's timer, three instances deep, has run out"
            );
            assert!(r.bool("s1.clean.on_delay.Q"), "the same, read by the host");
            assert!(!r.bool("s2.clean.on_delay.Q"), "s2's own timer has not");
            assert_eq!(r.int("elapsed"), 0, "b2 was released at 20 ms");
        }
        if t == 10 {
            assert_eq!(r.get("elapsed"), Value::Time(10 * MS));
        }
        t += 10;
    }
    assert_eq!(r.int("n1"), 2, "two clean presses");
    assert_eq!(
        r.int("n2"),
        0,
        "a bounce shorter than the hold is not a press"
    );
    assert_eq!(r.int("s1.count.CV"), 2);
    assert_eq!(
        r.c.call_depth, 2,
        "Main calls Station, which calls Debounce"
    );
}

#[test]
fn a_block_is_used_before_it_is_declared_and_a_variable_may_be_named_as_its_type() {
    let mut r = Run::new(
        "PROGRAM Main VAR toggle : Toggle; out : BOOL; END_VAR
           toggle(q => out);
         END_PROGRAM
         FUNCTION_BLOCK Toggle VAR_OUTPUT q : BOOL; END_VAR
           q := NOT q;
         END_FUNCTION_BLOCK",
    );
    r.scan().unwrap();
    assert!(r.bool("out"));
    r.scan().unwrap();
    assert!(!r.bool("out"));
}

#[test]
fn return_leaves_the_block_and_the_program_goes_on() {
    let mut r = Run::new(
        "FUNCTION_BLOCK Gate
         VAR_INPUT open : BOOL; END_VAR
         VAR_OUTPUT passed : INT; END_VAR
           IF NOT open THEN RETURN; END_IF;
           passed := passed + 1;
         END_FUNCTION_BLOCK
         PROGRAM Main VAR g : Gate; after : INT; END_VAR
           g(open := FALSE);
           g(open := TRUE);
           g(open := FALSE);
           after := after + 1;
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("g.passed"), 1);
    assert_eq!(r.int("after"), 1, "RETURN left the block, not the scan");
}

#[test]
fn a_block_that_calls_a_function_still_runs_on_its_own_instance_afterwards() {
    // The function's body uses fixed addresses; when it returns, the block's body must find
    // the instance it was called for again.
    let mut r = Run::new(
        "FUNCTION Twice : INT VAR_INPUT v : INT; END_VAR Twice := v * 2; END_FUNCTION
         FUNCTION_BLOCK Doubler
         VAR_INPUT seed : INT; END_VAR
         VAR_OUTPUT value : INT; calls : INT; END_VAR
           IF calls = 0 THEN value := seed; END_IF;
           value := Twice(value);
           calls := calls + 1;
         END_FUNCTION_BLOCK
         FUNCTION_BLOCK Pair VAR a, b : Doubler; END_VAR VAR_OUTPUT sum : INT; END_VAR
           a(seed := 1);
           b(seed := 10);
           sum := Twice(a.value) + b.value;
         END_FUNCTION_BLOCK
         PROGRAM Main VAR p, q : Pair; x, y : INT; END_VAR
           p(sum => x);
           q();
           q(sum => y);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!(r.int("x"), 2 * 2 + 20);
    assert_eq!(r.int("y"), 2 * 4 + 40, "q was called twice");
    assert_eq!(r.int("p.a.calls"), 1);
    assert_eq!(r.int("q.b.calls"), 2);
    assert_eq!(
        r.c.call_depth, 3,
        "Main, Pair, Doubler, Twice: three calls in progress"
    );
}

#[test]
fn structures_and_arrays_inside_a_block_belong_to_the_instance() {
    let mut r = Run::new(
        "TYPE Sample : STRUCT slot : INT; value : REAL; END_STRUCT; END_TYPE
         FUNCTION_BLOCK Recorder
         VAR_INPUT value : REAL; END_VAR
         VAR_OUTPUT mean : REAL; last : Sample; END_VAR
         VAR log : ARRAY [0..3] OF Sample; next : INT; i : INT; sum : REAL; END_VAR
           log[next].slot := next;
           log[next].value := value;
           last := log[next];
           next := (next + 1) MOD 4;
           sum := 0.0;
           FOR i := 0 TO 3 DO sum := sum + log[i].value; END_FOR;
           mean := sum / 4.0;
         END_FUNCTION_BLOCK
         PROGRAM Main VAR a, b : Recorder; ma, mb : REAL; seen : Sample; k : INT; END_VAR
           k := k + 1;
           a(value := INT_TO_REAL(k), mean => ma);
           b(value := 100.0, mean => mb, last => seen);
         END_PROGRAM",
    );
    for _ in 0..4 {
        r.scan().unwrap();
    }
    assert_eq!(r.get("ma"), Value::Real(2.5), "(1 + 2 + 3 + 4) / 4");
    assert_eq!(r.get("mb"), Value::Real(100.0));
    assert_eq!(r.int("seen.slot"), 3, "a structure copied out by the arrow");
    assert_eq!(r.get("a.log[2].value"), Value::Real(3.0));
    assert_eq!(r.get("b.log[2].value"), Value::Real(100.0));
    r.scan().unwrap();
    assert_eq!(
        r.get("a.log[0].value"),
        Value::Real(5.0),
        "the oldest was overwritten"
    );
}

#[test]
fn a_block_passes_its_own_structures_and_arrays_to_functions_and_to_blocks_it_holds() {
    // Every way a copy can go: from and to an instance's own variables (relative to it),
    // a function's parameters (fixed), and elements chosen at run time.
    let mut r = Run::new(
        "TYPE Pair : STRUCT a, b : INT; END_STRUCT; Four : ARRAY [1..4] OF INT; END_TYPE
         FUNCTION Sum : INT VAR_INPUT v : Four; END_VAR VAR i : INT; END_VAR
           FOR i := 1 TO 4 DO Sum := Sum + v[i]; END_FOR;
         END_FUNCTION
         FUNCTION Gap : INT VAR_INPUT p : Pair; END_VAR Gap := p.b - p.a; END_FUNCTION
         FUNCTION_BLOCK Sink VAR_INPUT p : Pair; v : Four; END_VAR VAR_OUTPUT seen : INT; kept : Pair; END_VAR
           seen := Gap(p) + Sum(v);
           kept := p;
         END_FUNCTION_BLOCK
         FUNCTION_BLOCK Source
         VAR_INPUT pick : INT; END_VAR
         VAR_OUTPUT total, spread, through : INT; back : Pair; END_VAR
         VAR data : Four := [1, 2, 3, 4]; pairs : ARRAY [1..3] OF Pair; sink : Sink; sinks : ARRAY [0..1] OF Sink; n : INT; END_VAR
           n := n + 1;
           pairs[pick].a := n;
           pairs[pick].b := n * 10;
           total := Sum(data);
           spread := Gap(pairs[pick]);
           sink(p := pairs[pick], v := data);
           through := sink.seen;
           sinks[pick MOD 2](p := pairs[pick], v := data, kept => back);
           data[pick] := data[pick] + 1;
         END_FUNCTION_BLOCK
         PROGRAM Main VAR one, two : Source; t1, s1, x1, t2, s2 : INT; b : Pair; END_VAR
           one(pick := 2, total => t1, spread => s1, through => x1, back => b);
           two(pick := 3, total => t2, spread => s2);
         END_PROGRAM",
    );
    r.scan().unwrap();
    assert_eq!((r.int("t1"), r.int("s1"), r.int("x1")), (10, 9, 19));
    assert_eq!((r.int("b.a"), r.int("b.b")), (1, 10));
    r.scan().unwrap();
    assert_eq!((r.int("t1"), r.int("s1"), r.int("x1")), (11, 18, 29));
    assert_eq!(
        (r.int("t2"), r.int("s2")),
        (11, 18),
        "two has its own data and pairs"
    );
    assert_eq!(r.int("one.pairs[2].b"), 20);
    assert_eq!(r.int("two.pairs[3].b"), 20);
    assert_eq!(r.int("one.pairs[3].b"), 0);
    assert_eq!(
        r.int("two.sinks[1].kept.a"),
        2,
        "pick 3 is the sink at 3 MOD 2"
    );
    assert_eq!(
        r.int("two.sinks[0].kept.a"),
        0,
        "the other was never called"
    );
}

#[test]
fn an_array_of_blocks_each_holding_an_array_of_blocks() {
    let mut r = Run::new(
        "FUNCTION_BLOCK Tap VAR_INPUT open : BOOL; END_VAR VAR_OUTPUT count : INT; END_VAR
           IF open THEN count := count + 1; END_IF;
         END_FUNCTION_BLOCK
         FUNCTION_BLOCK Row VAR_INPUT which : INT; END_VAR VAR_OUTPUT opened : INT; END_VAR
         VAR taps : ARRAY [1..3] OF Tap; i : INT; END_VAR
           FOR i := 1 TO 3 DO taps[i](open := i = which); END_FOR;
           opened := taps[1].count + taps[2].count + taps[3].count;
         END_FUNCTION_BLOCK
         PROGRAM Main VAR rows : ARRAY [0..1] OF Row; k, a, b, c : INT; END_VAR
           k := k + 1;
           rows[0](which := 1);
           rows[k MOD 2](which := 3);
           a := rows[0].taps[1].count;
           b := rows[k MOD 2].taps[k MOD 2 + 2].count;
           c := rows[1].opened;
         END_PROGRAM",
    );
    for _ in 0..4 {
        r.scan().unwrap();
    }
    // k = 1..4: rows[0] gets which = 1 every scan, and which = 3 when k is even;
    // rows[1] gets which = 3 when k is odd.
    assert_eq!(r.int("a"), 4);
    assert_eq!(r.int("rows[0].taps[3].count"), 2);
    assert_eq!(r.int("rows[1].taps[3].count"), 2);
    assert_eq!(r.int("rows[1].taps[1].count"), 0);
    assert_eq!(r.int("b"), 0, "k is 4: rows[0].taps[2], never opened");
    assert_eq!(r.int("c"), 2);
    assert_eq!(r.c.call_depth, 2);
}

#[test]
fn a_block_that_would_hold_an_instance_of_itself_is_refused_naming_the_cycle() {
    let d = refusal(
        "FUNCTION_BLOCK Node VAR next : Node; END_VAR END_FUNCTION_BLOCK
         PROGRAM Main VAR n : Node; END_VAR END_PROGRAM",
    );
    assert_eq!(
        d,
        "1:16: recursion is not allowed: the function block Node would hold an instance of itself: Node holds itself"
    );

    let d = refusal(
        "FUNCTION_BLOCK A VAR b : B; END_VAR END_FUNCTION_BLOCK
         FUNCTION_BLOCK B VAR c : ARRAY [1..2] OF C; END_VAR END_FUNCTION_BLOCK
         FUNCTION_BLOCK C VAR a : A; END_VAR END_FUNCTION_BLOCK
         PROGRAM Main END_PROGRAM",
    );
    assert!(
        d.ends_with("A holds B, which holds C, which holds A"),
        "through an array too: {d}"
    );
}

#[test]
fn from_outside_an_instance_only_its_inputs_are_written_and_only_its_holder_calls_it() {
    let blocks = "
        FUNCTION_BLOCK Inner VAR_INPUT i : INT; END_VAR VAR_OUTPUT o : INT; END_VAR VAR m : INT; END_VAR
          m := i; o := m; END_FUNCTION_BLOCK
        FUNCTION_BLOCK Outer VAR_INPUT i : INT; END_VAR VAR_OUTPUT o : INT; END_VAR VAR inner : Inner; m : INT; END_VAR
          inner(i := i); o := inner.o; END_FUNCTION_BLOCK";
    let main = |body: &str| {
        format!("{blocks} PROGRAM Main VAR x : Outer; v : INT; END_VAR {body} END_PROGRAM")
    };
    // What is allowed: set an input, read an input and an output, look into an inner instance.
    let mut r = Run::new(&main("x.i := 4; x(); v := x.o + x.i + x.inner.o;"));
    r.scan().unwrap();
    assert_eq!(r.int("v"), 12);

    for (body, said) in [
        // Figure 13 rule 4: an output is assigned inside the block only.
        ("x.o := 1;", "x.o is an output of the block; it cannot be written"),
        // §6.6.3.2 rule 11: a VAR of a function block is private.
        (
            "v := x.m;",
            "m is an internal variable of Outer; only its inputs and outputs can be named from outside",
        ),
        (
            "x.m := 1;",
            "m is an internal variable of Outer; only its inputs and outputs can be named from outside",
        ),
        (
            "v := x.inner.m;",
            "m is an internal variable of Inner; only its inputs and outputs can be named from outside",
        ),
        (
            "x.inner.i := 1;",
            "x.inner is inside the function block Outer; only its body writes there",
        ),
        (
            "x.inner(i := 1);",
            "x.inner is inside another function block instance; only that block's body calls it",
        ),
        ("x := x;", "x is a function block instance; call it, or name one of its members"),
        ("v := x;", "x is a function block instance; name one of its inputs or outputs"),
        ("x(i := 1, m := 2);", "m is an internal variable of Outer, not a parameter"),
        ("x(o := 1);", "o is an output of Outer: read it with o => variable"),
        ("x(i => v);", "i is an input of Outer: set it with i := value"),
        ("x(zz := 1);", "Outer has no parameter zz"),
        ("x(i := 1, i := 2);", "i is given twice"),
        ("x(1);", "a call of a function block names each parameter"),
        ("v(i := 1);", "v is a INT, not a function block instance"),
        ("Outer(i := 1);", "Outer is a function block type; declare an instance of it and call that"),
        ("v := Outer(1);", "Outer is not a function; a function block is called as a statement"),
    ] {
        let d = diagnostics(&main(body));
        assert!(
            d.iter().any(|m| m.contains(said)),
            "{body}\n  wanted '{said}'\n  got {d:?}"
        );
    }
    // Inside the block, its own inputs are not assigned (Figure 13 rule 2a, Figure 7).
    let d = refusal(
        "FUNCTION_BLOCK B VAR_INPUT i : INT; END_VAR i := 1; END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
    );
    assert!(
        d.contains("i is an input of B; it cannot be written inside it"),
        "{d}"
    );
}

#[test]
fn what_a_function_block_does_not_support_yet_is_refused_by_name() {
    for (src, said) in [
        (
            "FUNCTION_BLOCK B VAR_IN_OUT x : INT; END_VAR END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "1:18: VAR_IN_OUT is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B VAR_TEMP x : INT; END_VAR END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "1:18: VAR_TEMP is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B EXTENDS A END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "1:18: EXTENDS is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B METHOD M END_METHOD END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "1:18: METHOD is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B VAR PUBLIC x : INT; END_VAR END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "1:22: PUBLIC is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B VAR_INPUT t : TON; END_VAR END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "a function block instance as an input or an output of a function block is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B VAR x : INT; END_VAR END_FUNCTION_BLOCK PROGRAM Main VAR b : B := (x := 1); END_VAR END_PROGRAM",
            "an initialiser list (name := value, …) for a structure or a function block instance is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B END_FUNCTION_BLOCK PROGRAM Main VAR b : B := 1; END_VAR END_PROGRAM",
            "a function block instance cannot be given an initial value",
        ),
        (
            "INTERFACE I END_INTERFACE PROGRAM Main END_PROGRAM",
            "1:1: INTERFACE is not supported yet",
        ),
        (
            "CLASS C END_CLASS PROGRAM Main END_PROGRAM",
            "1:1: CLASS is not supported yet",
        ),
        (
            "VAR_GLOBAL g : INT; END_VAR PROGRAM Main END_PROGRAM",
            "1:1: VAR_GLOBAL is not supported yet",
        ),
        (
            "FUNCTION_BLOCK B END_FUNCTION_BLOCK",
            "the source has no PROGRAM; it needs one, where a scan starts",
        ),
        (
            "PROGRAM One END_PROGRAM PROGRAM Two END_PROGRAM",
            "1:33: Two is a second PROGRAM; a source has one, where a scan starts",
        ),
        (
            "FUNCTION_BLOCK B END_FUNCTION_BLOCK FUNCTION_BLOCK b END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "b is declared twice",
        ),
        (
            "FUNCTION_BLOCK TON END_FUNCTION_BLOCK PROGRAM Main END_PROGRAM",
            "TON is a name of the standard; choose another",
        ),
        (
            "PROGRAM Main VAR x : Main; END_VAR END_PROGRAM",
            "Main is a program, not a type",
        ),
        (
            "PROGRAM Main VAR i : INT; END_VAR WHILE TRUE DO CONTINUE; END_WHILE; END_PROGRAM",
            "CONTINUE is not supported yet",
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
fn every_member_of_every_instance_is_a_symbol_by_its_path() {
    let r = Run::new(&format!(
        "{STATION} PROGRAM Main VAR_INPUT go : BOOL; END_VAR VAR s : Station; END_VAR s(button := go); END_PROGRAM"
    ));
    let paths: Vec<(&str, &str, u32, bool)> =
        r.c.symbols
            .iter()
            .map(|s| (s.path.as_str(), s.ty.as_str(), s.offset, s.writable))
            .collect();
    assert_eq!(
        paths,
        vec![
            ("go", "BOOL", 0, true),
            ("s.button", "BOOL", 1, false),
            ("s.presses", "INT", 2, false),
            ("s.pressed", "BOOL", 4, false),
            ("s.clean.raw", "BOOL", 5, false),
            ("s.clean.hold", "TIME", 6, false),
            ("s.clean.q", "BOOL", 14, false),
            ("s.clean.on_delay.IN", "BOOL", 15, false),
            ("s.clean.on_delay.PT", "TIME", 19, false),
            ("s.clean.on_delay.Q", "BOOL", 16, false),
            ("s.clean.on_delay.ET", "TIME", 27, false),
            ("s.edge.CLK", "BOOL", 43, false),
            ("s.edge.Q", "BOOL", 44, false),
            ("s.count.CU", "BOOL", 46, false),
            ("s.count.R", "BOOL", 47, false),
            ("s.count.PV", "INT", 50, false),
            ("s.count.Q", "BOOL", 48, false),
            ("s.count.CV", "INT", 52, false),
        ]
    );
    assert_eq!(
        r.c.memory_size, 54,
        "the program's variables and nothing else"
    );
}
