//! What the workbench's Function Block Diagram editor writes is what the compiler compiles and
//! the VM runs (ADR-013 §2.8, §2.16, L3c-2).
//!
//! The documents in `tests/fbd/editor_*.xml` are written by the editor's own code: the
//! workbench's test `fbdmodel.test.ts` builds them through the editor's edits and holds each
//! equal to its file here, so the two cannot drift apart. `editor_mixer.xml` is a program built
//! from the editor's template; `editor_blinker_edited.xml` is `blinker.xml`, another tool's
//! document, after the editor assigned one output of one network to a new variable and titled
//! it, keeping the networks it does not draw (a feedback path, a connector) as found.

mod support;

use mythos_logic_compiler::{compile_in, Language, TargetProfile};
use mythos_logic_vm::Value;
use support::{Run, MS};

fn fbd(file: &str) -> Run {
    let path = format!("{}/tests/fbd/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Run::of(compile_in(&text, Language::Fbd, &TargetProfile::pc()))
}

fn set(r: &mut Run, var: &str, v: bool) {
    r.set(var, Value::Bool(v));
}

#[test]
fn the_seal_in_the_editor_wrote_latches_on_start_and_drops_on_stop() {
    let mut r = fbd("editor_mixer.xml");
    r.scan_at(0);
    assert!(!r.bool("run"));
    // run := (start OR run) AND NOT stop: the OR the editor joined in where a second line into
    // one input would have gone, and the negated input.
    set(&mut r, "start", true);
    r.scan_at(10);
    assert!(r.bool("run"));
    set(&mut r, "start", false);
    for t in [20, 30, 40] {
        r.scan_at(t);
        assert!(r.bool("run"), "sealed in at {t} ms");
    }
    set(&mut r, "stop", true);
    r.scan_at(50);
    assert!(!r.bool("run"));
    set(&mut r, "stop", false);
    r.scan_at(60);
    assert!(!r.bool("run"));
    set(&mut r, "start", true);
    set(&mut r, "stop", true);
    r.scan_at(70);
    assert!(!r.bool("run"), "stop held wins over start");
}

#[test]
fn the_timer_the_editor_wrote_lights_the_lamp_50_ms_after_run_with_both_outputs_assigned() {
    let mut r = fbd("editor_mixer.xml");
    r.scan_at(0);
    set(&mut r, "start", true);
    r.scan_at(100);
    set(&mut r, "start", false);
    r.scan_at(140);
    assert!(r.bool("run") && !r.bool("lamp"));
    assert_eq!(r.int("waited"), 40 * MS);
    r.scan_at(150);
    assert!(r.bool("lamp"), "PT is T#50ms");
    assert_eq!(r.int("waited"), 50 * MS);
    set(&mut r, "stop", true);
    r.scan_at(160);
    assert!(!r.bool("run") && !r.bool("lamp"));
    assert_eq!(r.int("waited"), 0);
}

#[test]
fn the_nested_boxes_and_the_jump_the_editor_wrote_compute_and_skip_as_drawn() {
    let mut r = fbd("editor_mixer.xml");
    // Over 100 the jump to LATE skips `low := TRUE`; the network at LATE runs.
    r.set("level", Value::Int(150));
    r.scan_at(0);
    assert_eq!(r.int("total"), 301, "ADD(MUL(level, 2), 1)");
    assert!(!r.bool("low"), "skipped while level > 100");
    assert_eq!(r.int("seen"), 150);
    r.set("level", Value::Int(-7));
    r.scan_at(10);
    assert_eq!(r.int("total"), -13);
    assert!(r.bool("low"), "written once level is not over 100");
    assert_eq!(r.int("seen"), -7);
}

#[test]
fn another_tools_diagram_runs_the_same_after_the_editor_changed_one_network_of_it() {
    let mut before = fbd("blinker.xml");
    let mut after = fbd("editor_blinker_edited.xml");
    for scan in 0..400 {
        for r in [&mut before, &mut after] {
            r.scan_at(scan * 7);
        }
        for var in ["blink", "done", "toggle", "held"] {
            assert_eq!(before.bool(var), after.bool(var), "{var} at scan {scan}");
        }
        for var in ["count", "steps"] {
            assert_eq!(before.int(var), after.int(var), "{var} at scan {scan}");
        }
        assert_eq!(
            before.real("level"),
            after.real("level"),
            "level at scan {scan}"
        );
        // The output the editor assigned: t2's Q, written to pulse.
        assert_eq!(
            after.bool("pulse"),
            after.bool("t2.Q"),
            "pulse at scan {scan}"
        );
    }
}
