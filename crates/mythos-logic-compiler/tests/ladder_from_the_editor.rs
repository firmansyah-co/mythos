//! What the workbench's Ladder Diagram editor writes is what the compiler compiles and the VM
//! runs (ADR-013 §2.15, L3b-2b).
//!
//! The documents in `tests/ladder/editor_*.xml` are written by the editor's own code: the
//! workbench's test `ldmodel.test.ts` builds them through the editor's edits and holds each
//! equal to its file here, so the two cannot drift apart. `foreign.xml` is a document of
//! another tool, laid out its own way (a rail pair per rung, wire points, a namespace prefix),
//! and `editor_foreign_edited.xml` is that document after the editor changed one rung of it,
//! moved a network it keeps as found, and added one.

mod support;

use mythos_logic_compiler::{
    compile_in, compile_sources, library_blocks, Language, LibraryBlock, Sources, TargetProfile,
};
use mythos_logic_vm::Value;
use support::{Run, MOTOR_LIBRARY, MS};

fn ladder(file: &str) -> Run {
    let path = format!("{}/tests/ladder/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Run::of(compile_in(&text, Language::Ld, &TargetProfile::pc()))
}

fn set(r: &mut Run, var: &str, v: bool) {
    r.set(var, Value::Bool(v));
}

#[test]
fn the_seal_in_rung_the_editor_wrote_latches_on_start_and_drops_on_stop() {
    let mut r = ladder("editor_seal_in.xml");
    r.scan_at(0);
    assert!(!r.bool("motor") && !r.bool("lamp"));
    // Start pressed: the motor runs, and keeps running through its own contact once it is let go.
    set(&mut r, "start", true);
    r.scan_at(10);
    assert!(r.bool("motor"));
    set(&mut r, "start", false);
    for t in [20, 30, 40] {
        r.scan_at(t);
        assert!(r.bool("motor"), "sealed in at {t} ms");
    }
    // Stop pressed: it drops, and stays off once stop is let go.
    set(&mut r, "stop", true);
    r.scan_at(50);
    assert!(!r.bool("motor"));
    set(&mut r, "stop", false);
    r.scan_at(60);
    assert!(!r.bool("motor"));
    // Stop held while start is pressed: the normally closed contact wins.
    set(&mut r, "stop", true);
    set(&mut r, "start", true);
    r.scan_at(70);
    assert!(!r.bool("motor"));
}

#[test]
fn the_timer_rung_the_editor_wrote_lights_the_lamp_50_ms_after_the_motor_starts() {
    let mut r = ladder("editor_seal_in.xml");
    r.scan_at(0);
    set(&mut r, "start", true);
    r.scan_at(100);
    set(&mut r, "start", false);
    // The timer's IN went on at 100 ms: its ET, written to `waited`, counts from there.
    r.scan_at(140);
    assert!(r.bool("motor") && !r.bool("lamp"));
    assert_eq!(r.int("waited"), 40 * MS);
    r.scan_at(150);
    assert!(r.bool("lamp"), "PT is T#50ms");
    assert_eq!(r.int("waited"), 50 * MS);
    // The motor stopped: IN goes off, and Q and ET with it.
    set(&mut r, "stop", true);
    r.scan_at(160);
    assert!(!r.bool("motor") && !r.bool("lamp"));
    assert_eq!(r.int("waited"), 0);
}

#[test]
fn another_tools_document_runs_the_same_after_the_editor_changed_one_rung_of_it() {
    let mut before = ladder("foreign.xml");
    let mut after = ladder("editor_foreign_edited.xml");
    // A fixed sequence of inputs, the same for both.
    let mut seed: u32 = 0x2545_f491;
    for scan in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let (a, b, c) = (seed & 1 != 0, seed & 2 != 0, seed & 4 != 0);
        let level = (seed >> 8) as i16 % 20;
        for r in [&mut before, &mut after] {
            set(r, "a", a);
            set(r, "b", b);
            set(r, "c", c);
            r.set("level", Value::Int(level));
            r.scan_at(scan * 7);
        }
        // The rung the editor changed: c put in series after a.
        assert_eq!(before.bool("q"), a && !b, "scan {scan}");
        assert_eq!(after.bool("q"), a && c && !b, "scan {scan}");
        // The network it added.
        assert_eq!(after.bool("extra"), a, "scan {scan}");
        // What it did not change does as it did: the timer rung, moved down, and the
        // network it keeps as found, moved up.
        for var in ["r", "high"] {
            assert_eq!(before.bool(var), after.bool(var), "{var} at scan {scan}");
        }
        assert_eq!(before.int("elapsed"), after.int("elapsed"), "scan {scan}");
        assert_eq!(after.bool("high"), level > 10, "scan {scan}");
    }
}

/// A Ladder Diagram compiled with the project's library, as every program is (ADR-014 §2.8).
fn ladder_with_library(file: &str) -> Run {
    let path = format!("{}/tests/ladder_library/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Run::of(compile_sources(
        &Sources {
            tc6: &[(file, &text)],
            library: &[MOTOR_LIBRARY],
            ..Default::default()
        },
        &TargetProfile::pc(),
    ))
}

#[test]
fn editor_library_blocks_are_what_the_workbench_tests_place() {
    // The pins the workbench's tests give the boxes are the answer `ANSWER` of
    // `libraryanswer.ts`, read here from that file: it is what `library_blocks` answers for
    // the library the editor's fixtures compile with, in the shape the logic module writes.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../frontend/packages/logic-ui/src/__tests__/libraryanswer.ts");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (_, after) = text
        .split_once("const ANSWER = `")
        .unwrap_or_else(|| panic!("{}: no `const ANSWER = `", path.display()));
    let (json, _) = after
        .split_once('`')
        .unwrap_or_else(|| panic!("{}: ANSWER is not closed", path.display()));
    let fixture: serde_json::Value =
        serde_json::from_str(json).unwrap_or_else(|e| panic!("{}: ANSWER: {e}", path.display()));
    let blocks = library_blocks(&Sources {
        library: &[MOTOR_LIBRARY],
        ..Default::default()
    })
    .unwrap_or_else(|d| panic!("{d:?}"));
    let pins = |list: &[(String, String)]| -> Vec<serde_json::Value> {
        list.iter()
            .map(|(name, ty)| serde_json::json!({ "name": name, "type": ty }))
            .collect()
    };
    let answered: Vec<serde_json::Value> = blocks
        .iter()
        .map(|b: &LibraryBlock| {
            serde_json::json!({
                "name": b.name,
                "kind": if b.function_block { "function_block" } else { "function" },
                "inputs": pins(&b.inputs),
                "outputs": pins(&b.outputs),
                "result": b.result,
            })
        })
        .collect();
    assert_eq!(serde_json::Value::Array(answered), fixture);
}

#[test]
fn the_rung_the_editor_wrote_with_a_library_block_runs_as_the_same_program_in_st() {
    // Written by the editor with the library's boxes: Motor's power at its BOOL input `run`,
    // written RUN as the editor writes a pin (the declared spelling is lower case), and Scale
    // under EN.
    let mut ld = ladder_with_library("editor_library.xml");
    let mut st = Run::of(compile_sources(
        &Sources {
            st: Some((
                "main.st",
                "PROGRAM Main
VAR_INPUT start : BOOL; enable : BOOL; speed : INT; END_VAR
VAR_OUTPUT running : BOOL; done : BOOL; rpm : INT; scaled : INT; END_VAR
VAR motor1 : Motor; END_VAR
motor1(run := start, speed := speed);
running := motor1.running;
rpm := motor1.rpm;
IF enable THEN scaled := Scale(raw := rpm, factor := 3); END_IF;
done := enable;
END_PROGRAM",
            )),
            library: &[MOTOR_LIBRARY],
            ..Default::default()
        },
        &TargetProfile::pc(),
    ));
    let mut seed: u32 = 0x9e37_79b9;
    let mut ran = (false, false);
    for scan in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let (start, enable) = (seed & 1 != 0, seed & 2 != 0);
        let speed = ((seed >> 8) % 100) as i16;
        for r in [&mut ld, &mut st] {
            set(r, "start", start);
            set(r, "enable", enable);
            r.set("speed", Value::Int(speed));
            r.scan_at(scan * 10);
        }
        for var in ["running", "done"] {
            assert_eq!(ld.bool(var), st.bool(var), "{var} at scan {scan}");
        }
        for var in ["rpm", "scaled"] {
            assert_eq!(ld.int(var), st.int(var), "{var} at scan {scan}");
        }
        // The library's code ran: rpm is ten times speed while started, and scaled three times that.
        if start {
            assert_eq!(ld.int("rpm"), i64::from(speed) * 10, "scan {scan}");
            ran.0 = true;
        }
        if enable {
            assert_eq!(ld.int("scaled"), ld.int("rpm") * 3, "scan {scan}");
            ran.1 = true;
        }
    }
    assert_eq!(ran, (true, true), "both blocks were called with power");
}
