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

use mythos_logic_compiler::{compile_in, compile_sources, Language, Sources, TargetProfile};
use mythos_logic_vm::Value;
use support::{Run, MOTOR_LIBRARY, MS};

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

#[test]
fn the_network_the_editor_wrote_with_a_library_block_runs_as_the_same_program_in_st() {
    // Written by the editor with the library's boxes (no EN or ENO in FBD): Motor's pins written
    // RUN and SPEED as the editor writes a pin (declared in lower case), and Scale's OUT.
    let compiled = |sources: Sources| Run::of(compile_sources(&sources, &TargetProfile::pc()));
    let path = format!(
        "{}/tests/fbd_library/editor_library.xml",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut fbd = compiled(Sources {
        tc6: &[("editor_library.xml", &text)],
        library: &[MOTOR_LIBRARY],
        ..Default::default()
    });
    let mut st = compiled(Sources {
        st: Some((
            "main.st",
            "PROGRAM Main
VAR_INPUT start : BOOL; speed : INT; END_VAR
VAR_OUTPUT running : BOOL; rpm : INT; scaled : INT; END_VAR
VAR motor1 : Motor; END_VAR
motor1(run := start, speed := speed);
running := motor1.running;
rpm := motor1.rpm;
scaled := Scale(raw := rpm, factor := 3);
END_PROGRAM",
        )),
        library: &[MOTOR_LIBRARY],
        ..Default::default()
    });
    let mut seed: u32 = 0x85eb_ca6b;
    let mut started = false;
    for scan in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let start = seed & 1 != 0;
        let speed = ((seed >> 8) % 100) as i16;
        for r in [&mut fbd, &mut st] {
            set(r, "start", start);
            r.set("speed", Value::Int(speed));
            r.scan_at(scan * 10);
        }
        assert_eq!(fbd.bool("running"), st.bool("running"), "scan {scan}");
        for var in ["rpm", "scaled"] {
            assert_eq!(fbd.int(var), st.int(var), "{var} at scan {scan}");
        }
        // The library's code ran: rpm is ten times speed while started, and scaled three times that.
        assert_eq!(
            fbd.int("rpm"),
            if start { i64::from(speed) * 10 } else { 0 },
            "scan {scan}"
        );
        assert_eq!(fbd.int("scaled"), fbd.int("rpm") * 3, "scan {scan}");
        started |= start;
    }
    assert!(started, "Motor was called with run");
}
