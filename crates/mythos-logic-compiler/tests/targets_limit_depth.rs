//! A target says how deep a program's evaluation stack and its calls may go (ADR-013 §2.2),
//! and a program that goes deeper is refused when it is compiled, naming the target, the limit
//! and what the program needs: not when the controller starts it.

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::{cold_start, scan, Fault, Frame, Limits, LoadError, Program};
use std::path::Path;

fn profile(json: &str) -> TargetProfile {
    serde_json::from_str(json).expect("a target profile")
}

/// A profile as a runtime module would declare it, with these depths.
fn with_depths(name: &str, stack: Option<u16>, calls: Option<u16>) -> TargetProfile {
    let mut json =
        format!(r#"{{"name":"{name}","format_version":4,"max_memory":65536,"max_code":65536"#);
    if let Some(stack) = stack {
        json.push_str(&format!(r#","max_stack_depth":{stack}"#));
    }
    if let Some(calls) = calls {
        json.push_str(&format!(r#","max_call_depth":{calls}"#));
    }
    json.push('}');
    profile(&json)
}

fn refusal(src: &str, target: &TargetProfile) -> Vec<String> {
    match compile(src, target) {
        Ok(_) => panic!("compiled, but should not have"),
        Err(d) => d.iter().map(|d| d.to_string()).collect(),
    }
}

/// Each `(` leaves one more value on the stack while what is inside it is worked out.
const DEEP: &str = "PROGRAM P VAR a : DINT; END_VAR
  a := 1 + (2 + (3 + (4 + (5 + (6 + a)))));
END_PROGRAM";

/// The program calls Outer, which calls Middle, which calls Inner: three calls in progress.
const NESTED: &str =
    "FUNCTION Inner : DINT VAR_INPUT x : DINT; END_VAR Inner := x + 1; END_FUNCTION
FUNCTION Middle : DINT VAR_INPUT x : DINT; END_VAR Middle := Inner(x) + 1; END_FUNCTION
FUNCTION Outer : DINT VAR_INPUT x : DINT; END_VAR Outer := Middle(x) + 1; END_FUNCTION
PROGRAM P VAR a : DINT; END_VAR a := Outer(a); END_PROGRAM";

#[test]
fn a_program_whose_stack_is_deeper_than_the_target_allows_is_refused_at_compile_time() {
    let unlimited = with_depths("any", None, None);
    let need = compile(DEEP, &unlimited).unwrap().stack_depth;
    assert_eq!(
        need, 7,
        "the six numbers and `a`, all held before the first addition"
    );

    // Exactly what it needs is enough; one less is not.
    assert!(compile(DEEP, &with_depths("board", Some(need), None)).is_ok());
    assert_eq!(
        refusal(DEEP, &with_depths("board", Some(need - 1), None)),
        vec![format!(
            "0:0: the program needs a stack {need} values deep; the target board allows {}",
            need - 1
        )]
    );
    // A shallower program fits the same target.
    assert!(compile(
        "PROGRAM P VAR a : DINT; END_VAR a := a + 1; END_PROGRAM",
        &with_depths("board", Some(need - 1), None)
    )
    .is_ok());
}

#[test]
fn a_program_whose_calls_nest_deeper_than_the_target_allows_is_refused_at_compile_time() {
    let unlimited = with_depths("any", None, None);
    let need = compile(NESTED, &unlimited).unwrap().call_depth;
    assert_eq!(need, 3, "Outer, Middle and Inner are in progress at once");

    assert!(compile(NESTED, &with_depths("board", None, Some(3))).is_ok());
    assert_eq!(
        refusal(NESTED, &with_depths("board", None, Some(2))),
        vec!["0:0: the program nests its calls 3 deep; the target board allows 2"]
    );
    // Both limits broken: both told.
    let stack = compile(NESTED, &unlimited).unwrap().stack_depth;
    assert_eq!(
        refusal(NESTED, &with_depths("tiny", Some(stack - 1), Some(1))),
        vec![
            format!(
                "0:0: the program needs a stack {stack} values deep; the target tiny allows {}",
                stack - 1
            ),
            "0:0: the program nests its calls 3 deep; the target tiny allows 1".to_string(),
        ]
    );
}

#[test]
fn a_profile_without_the_depths_states_no_limit_and_zero_is_a_limit() {
    // A profile written before the depths existed: it still loads, and it limits neither.
    let old = profile(r#"{"name":"old","format_version":4,"max_memory":65536,"max_code":65536}"#);
    assert_eq!((old.max_stack_depth, old.max_call_depth), (None, None));
    assert!(compile(DEEP, &old).is_ok());
    assert!(compile(NESTED, &old).is_ok());

    // Leaving a depth out is not declaring 0. A limit of 0 calls allows a program that
    // calls nothing and refuses one that calls; a limit of 0 on the stack refuses every
    // program, since every program needs a stack.
    let no_calls = with_depths("no-calls", None, Some(0));
    assert!(compile(DEEP, &no_calls).is_ok());
    assert_eq!(
        refusal(NESTED, &no_calls),
        vec!["0:0: the program nests its calls 3 deep; the target no-calls allows 0"]
    );
    let no_stack = with_depths("no-stack", Some(0), None);
    assert_eq!(
        refusal(
            "PROGRAM P VAR a : BOOL; END_VAR a := TRUE; END_PROGRAM",
            &no_stack
        ),
        vec!["0:0: the program needs a stack 1 values deep; the target no-stack allows 0"]
    );
}

#[test]
fn a_profile_says_its_depths_only_when_it_has_them() {
    // No limit is written as no field, so what reads a profile never sees a made-up number.
    let pc = serde_json::to_value(TargetProfile::pc()).unwrap();
    assert!(pc.get("max_stack_depth").is_none(), "{pc}");
    assert!(pc.get("max_call_depth").is_none(), "{pc}");
    assert_eq!(
        (
            TargetProfile::pc().max_stack_depth,
            TargetProfile::pc().max_call_depth
        ),
        (None, None),
        "a PC runner gives a program what it declares"
    );
    for board in [TargetProfile::rp2040(), TargetProfile::esp32()] {
        let json = serde_json::to_value(&board).unwrap();
        assert_eq!(json["max_stack_depth"], 64, "{json}");
        assert_eq!(json["max_call_depth"], 16, "{json}");
        let back: TargetProfile = serde_json::from_value(json).unwrap();
        assert_eq!(back, board, "and it reads back as it was");
    }
}

/// The number after `field: ` in the `Limits` a firmware source declares.
fn declared(source: &str, field: &str) -> u16 {
    let limits = source
        .split("Limits {")
        .nth(1)
        .unwrap_or_else(|| panic!("the firmware declares no Limits"));
    let after = limits
        .split(&format!("{field}: "))
        .nth(1)
        .unwrap_or_else(|| panic!("the firmware's Limits have no {field}"));
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("{field} is not a number"))
}

#[test]
fn the_microcontroller_profiles_declare_the_depths_the_firmware_has() {
    // The profiles' depths are the firmware's, so a program the compiler lets through for
    // `rp2040` or `esp32` is one that firmware loads. The firmware says what it has in the
    // `Limits` it loads a program within; this reads them from its source.
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mythos-logic-vm/examples");
    for firmware in ["on_target/main.rs", "footprint/main.rs"] {
        let source = std::fs::read_to_string(examples.join(firmware)).unwrap();
        assert!(
            source.contains("Program::load_within("),
            "{firmware} loads a program within its limits"
        );
        for board in [TargetProfile::rp2040(), TargetProfile::esp32()] {
            assert_eq!(
                board.max_stack_depth,
                Some(declared(&source, "stack_depth")),
                "{firmware} and the profile {}",
                board.name
            );
            assert_eq!(
                board.max_call_depth,
                Some(declared(&source, "call_depth")),
                "{firmware} and the profile {}",
                board.name
            );
        }
    }
}

#[test]
fn what_the_compiler_refuses_for_a_target_is_what_its_firmware_would_refuse_to_load() {
    // The compile-time refusal and the loader's are the same fact, told earlier.
    let c = compile(DEEP, &with_depths("any", None, None)).unwrap();
    let room = Limits {
        memory: 4096,
        stack_depth: c.stack_depth as usize,
        call_depth: 16,
    };
    assert!(Program::load_within(&c.bytecode, &room).is_ok());
    assert_eq!(
        Program::load_within(
            &c.bytecode,
            &Limits {
                stack_depth: c.stack_depth as usize - 1,
                ..room
            }
        )
        .err(),
        Some(LoadError::StackTooDeep(c.stack_depth))
    );
    let nested = compile(NESTED, &with_depths("any", None, None)).unwrap();
    assert_eq!(
        Program::load_within(
            &nested.bytecode,
            &Limits {
                call_depth: 2,
                ..room
            }
        )
        .err(),
        Some(LoadError::CallsTooDeep(3))
    );
}

#[test]
fn the_depth_a_program_declares_is_the_depth_it_reaches_and_a_larger_stack_changes_nothing() {
    // The compiler declares exactly the depth the program reaches: it runs in a stack of
    // that depth, and in one of a slot less it is refused before it runs. In a host's
    // larger stack it gives the same.
    for src in [DEEP, NESTED] {
        let c = compile(src, &TargetProfile::pc()).unwrap();
        let p = Program::load(&c.bytecode).unwrap();
        let run = |slots: usize| {
            let mut memory = vec![0u8; p.memory_size()];
            cold_start(&p, &mut memory).unwrap();
            let mut stack = vec![0u64; slots];
            let mut calls = vec![Frame::default(); p.call_depth()];
            scan(&p, &mut memory, &mut stack, &mut calls, 0, 10_000).map(|_| memory)
        };
        let exact = run(p.stack_depth()).expect("it runs in what it declares");
        assert_eq!(run(p.stack_depth() + 100).as_ref(), Ok(&exact));
        assert_eq!(run(p.stack_depth() - 1), Err(Fault::WrongSizes));
    }
}
