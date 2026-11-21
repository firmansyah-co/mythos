//! What the workbench's Sequential Function Chart editor writes is what the compiler compiles and
//! the VM runs (ADR-013 §2.8, §2.17, L3d-3).
//!
//! The documents in `tests/sfc/editor_*.xml` are written by the editor's own code: the
//! workbench's test `sfcmodel.test.ts` builds them through the editor's edits and holds each
//! equal to its file here, so the two cannot drift apart. `editor_sequence.xml` is a chart built
//! from the logic module's template: steps and transitions put as pairs, an alternative branch
//! (on stop, to Abort and back by a jump), a simultaneous branch (Mix and Heat), actions of the
//! qualifiers N, P1 (statements), L, D, S and R. `editor_bottling_edited.xml` and
//! `editor_mixer_edited.xml` are the front-end's corpus charts, another tool's documents, after
//! the editor gave one step one more action: the chart is laid out anew, and everything else,
//! the bodies in LD and FBD and the networks drawn in the chart among it, runs as it did.

mod support;

use mythos_logic_compiler::{compile_in, Language, TargetProfile};
use mythos_logic_vm::Value;
use support::Run;

fn sfc(path: &str) -> Run {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Run::of(compile_in(&text, Language::Sfc, &TargetProfile::pc()))
}

fn editor(file: &str) -> Run {
    sfc(&format!("{}/tests/sfc/{file}", env!("CARGO_MANIFEST_DIR")))
}

fn corpus(file: &str) -> Run {
    sfc(&format!(
        "{}/../mythos-logic-graphic/tests/sfc/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

const STEPS: [&str; 6] = ["Idle", "Fill", "Mix", "Heat", "Drain", "Abort"];

/// The active steps, by name.
fn active(r: &Run) -> Vec<&'static str> {
    STEPS
        .iter()
        .copied()
        .filter(|n| r.bool(&format!("{n}.X")))
        .collect()
}

#[test]
fn the_sequence_built_in_the_editor_runs_through_its_simultaneous_branch_and_back() {
    let mut r = editor("editor_sequence.xml");
    r.scan_at(0);
    assert_eq!(active(&r), ["Idle"]);
    r.scan_at(10);
    assert_eq!(
        active(&r),
        ["Idle"],
        "a new transition's condition is what was written: start"
    );
    r.set("start", Value::Bool(true));
    r.scan_at(20);
    assert_eq!(active(&r), ["Fill"]);
    r.set("start", Value::Bool(false));
    // Fill's actions run from the next scan: N valve, and P1 counts the batch once.
    r.scan_at(30);
    assert!(r.bool("valve"));
    assert_eq!(r.int("batches"), 1);
    for t in [40, 50, 60] {
        r.scan_at(t);
        assert_eq!(active(&r), ["Fill"], "Fill lasts 50 ms ({t} ms)");
        assert_eq!(r.int("batches"), 1, "P1 pulses once on entry ({t} ms)");
    }
    // Fill.T >= T#50ms at 70 ms: both Mix and Heat at once.
    r.scan_at(70);
    assert_eq!(active(&r), ["Mix", "Heat"]);
    r.scan_at(80);
    assert!(!r.bool("valve"), "N valve ends with Fill");
    assert!(r.bool("mixer"), "L mixer starts with Mix");
    assert!(!r.bool("heater"), "D heater waits 20 ms");
    r.scan_at(100);
    assert!(
        r.bool("heater"),
        "D T#20ms heater, 20 ms after its step's first scan"
    );
    r.scan_at(110);
    assert!(
        !r.bool("mixer"),
        "L T#30ms mixer, ended 30 ms after it started"
    );
    // Both have lasted 40 ms at 110 ms; the convergence is passed when the transition after it clears.
    assert_eq!(active(&r), ["Drain"]);
    r.scan_at(120);
    assert!(!r.bool("heater"), "D heater ends with Heat");
    r.scan_at(130);
    assert_eq!(
        active(&r),
        ["Idle"],
        "Drain.T >= T#20ms, and the jump back to Idle"
    );
}

#[test]
fn the_alternative_branch_built_in_the_editor_aborts_on_stop_and_the_left_branch_goes_first() {
    let mut r = editor("editor_sequence.xml");
    r.scan_at(0);
    r.set("start", Value::Bool(true));
    r.scan_at(10);
    r.set("start", Value::Bool(false));
    assert_eq!(active(&r), ["Fill"]);
    r.set("stop", Value::Bool(true));
    r.scan_at(20);
    assert_eq!(active(&r), ["Abort"]);
    r.scan_at(30);
    assert!(r.bool("alarm"), "S alarm");
    r.scan_at(40);
    assert_eq!(active(&r), ["Abort"], "until stop is released");
    r.set("stop", Value::Bool(false));
    r.scan_at(50);
    assert_eq!(active(&r), ["Idle"], "NOT stop, and the jump back to Idle");
    r.scan_at(60);
    assert!(r.bool("alarm"), "a stored action stays on after its step");
    // Both branches' conditions at once: the left one, Fill.T >= T#50ms, is tested first.
    r.set("start", Value::Bool(true));
    r.scan_at(70);
    r.set("start", Value::Bool(false));
    assert_eq!(active(&r), ["Fill"]);
    for t in [80, 90, 100, 110] {
        r.scan_at(t);
    }
    r.set("stop", Value::Bool(true));
    r.scan_at(120);
    assert_eq!(
        active(&r),
        ["Mix", "Heat"],
        "Fill.T is 50 ms and stop is TRUE: the leftmost branch"
    );
    r.set("stop", Value::Bool(false));
    for t in [130, 140, 150, 160] {
        r.scan_at(t);
    }
    assert_eq!(active(&r), ["Drain"]);
    r.scan_at(170);
    assert!(!r.bool("alarm"), "R alarm in Drain");
    assert_eq!(r.int("batches"), 2);
}

/// Run the corpus chart and the editor's edit of it side by side, every value equal but the one
/// the edit added, which follows its step as an N action does: TRUE in the scan after the step is
/// entered, as long as it is active.
fn same_but_one(
    before: &str,
    after: &str,
    steps: &[&str],
    bools: &[&str],
    ints: &[&str],
    step: &str,
    added: &str,
) {
    let mut a = corpus(before);
    let mut b = editor(after);
    // An initial step is active before the first scan (§6.7.2).
    let mut was = b.bool(&format!("{step}.X"));
    for scan in 0..600 {
        let now = scan * 7;
        a.scan_at(now);
        b.scan_at(now);
        for s in steps {
            let x = format!("{s}.X");
            assert_eq!(a.bool(&x), b.bool(&x), "{x} at {now} ms");
            let t = format!("{s}.T");
            assert_eq!(a.int(&t), b.int(&t), "{t} at {now} ms");
        }
        for v in bools {
            assert_eq!(a.bool(v), b.bool(v), "{v} at {now} ms");
        }
        for v in ints {
            assert_eq!(a.int(v), b.int(v), "{v} at {now} ms");
        }
        assert_eq!(
            b.bool(added),
            was,
            "{added} at {now} ms follows {step} of the scan before"
        );
        was = b.bool(&format!("{step}.X"));
    }
}

#[test]
fn another_tools_chart_runs_the_same_after_the_editor_laid_it_out_and_added_an_action() {
    same_but_one(
        "bottling.xml",
        "editor_bottling_edited.xml",
        &[
            "Idle", "Convey", "Reject", "Fill", "FillDone", "Cap", "CapDone", "Release",
        ],
        &["conveyor", "valve", "capper", "alarm", "label_ok"],
        &["bottles", "rejected", "fill_time"],
        "Idle",
        "idle_lamp",
    );
}

#[test]
fn a_chart_with_bodies_in_ld_and_fbd_runs_the_same_after_the_editor_laid_it_out() {
    // Its inline bodies in LD and FBD are written as found, the rung drawn in the chart and the
    // FBD network reaching a transition through a connector moved with their transitions.
    same_but_one(
        "mixer.xml",
        "editor_mixer_edited.xml",
        &["Idle", "Fill", "Stir"],
        &["ready", "valve", "motor"],
        &["batches", "stirred"],
        "Fill",
        "filling",
    );
    let r = editor("editor_mixer_edited.xml");
    assert_eq!(r.int("Fill.T"), 0);
}

#[test]
fn a_step_renamed_in_the_editor_is_timed_by_its_condition_under_its_new_name() {
    // Found in review: a rename that left `Step1.T` in the condition, and a new step that then took
    // the name Step1, compiled with the condition timing the new step, so Fill never left. The
    // editor now writes the condition as `Fill.T >= T#50ms`.
    let mut r = editor("editor_renamed.xml");
    let active = |r: &Run| {
        ["Start", "Fill", "Step1"]
            .into_iter()
            .filter(|n| r.bool(&format!("{n}.X")))
            .collect::<Vec<_>>()
    };
    r.scan_at(0);
    r.set("go", Value::Bool(true));
    r.scan_at(10);
    r.set("go", Value::Bool(false));
    assert_eq!(active(&r), ["Fill"]);
    for t in [20, 30, 40, 50] {
        r.scan_at(t);
        assert_eq!(active(&r), ["Fill"], "{t} ms");
    }
    r.scan_at(60);
    assert_eq!(active(&r), ["Step1"], "Fill.T >= T#50ms, Fill's own time");
}

#[test]
fn a_selection_the_editor_laid_out_in_its_priority_order_takes_the_branch_its_priorities_chose() {
    // Found in review: `priorities.xml` tests its right branch first (priority 1, to C); the editor
    // writes it without priorities, that branch on the left, so position gives the same order.
    let first = |r: &mut Run| {
        r.scan_at(0);
        r.set("go", Value::Bool(true));
        r.scan_at(10);
        r.scan_at(20);
        (r.bool("B.X"), r.bool("C.X"))
    };
    let mut before = sfc(&format!(
        "{}/tests/sfc/priorities.xml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut after = editor("editor_priorities_edited.xml");
    assert_eq!(first(&mut before), (false, true));
    assert_eq!(first(&mut after), (false, true));
}
