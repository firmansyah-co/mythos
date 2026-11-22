//! Sequential Function Charts, compiled from PLCopen TC6 XML and run on the VM (ADR-013
//! §2.17): each rule of IEC 61131-3 ed. 3 §6.7 taken, one test each, scan by scan.

mod support;
#[path = "../../mythos-logic-graphic/tests/support/tc6.rs"]
mod tc6;

use mythos_logic_compiler::{
    compile_in, compile_sources, At, Holds, Language, Sources, TargetProfile,
};
use mythos_logic_vm::{cold_start, read, write, Program, Value};
use support::Run;
use tc6::{project, Act, Pou};

fn run(pous: &[Pou]) -> Run {
    Run::of(compile_in(
        &project(pous),
        Language::Sfc,
        &TargetProfile::pc(),
    ))
}

/// S1 (initial) --go--> S2 --back--> S1.
fn two_steps() -> Pou {
    Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL"), ("back", "BOOL")])
        .step(1, 100, 0, "S1", true, &["4"])
        .trans(2, 100, 50, "go", &["1"])
        .step(3, 100, 100, "S2", false, &["2"])
        .trans(4, 100, 150, "back", &["3"])
}

/// The active steps, by name, of `names`.
fn active(r: &Run, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter(|n| r.bool(&format!("{n}.X")))
        .map(|n| n.to_string())
        .collect()
}

#[test]
fn the_initial_step_is_active_after_a_cold_start_and_no_other() {
    // §6.7.2: the initial state is BOOL#1 for the initial step, BOOL#0 for the others, and an
    // elapsed time of t#0s.
    let r = run(&[two_steps()]);
    assert!(r.bool("S1.X"));
    assert!(!r.bool("S2.X"));
    assert_eq!(r.int("S1.T"), 0);
}

#[test]
fn a_transition_clears_only_when_the_step_before_it_is_active_and_its_condition_is_true() {
    // §6.7.5, Table 61 feature 1: from S1 to S2 if and only if S1 is active and go is TRUE.
    let mut r = run(&[two_steps()]);
    r.scan_at(0);
    assert_eq!(active(&r, &["S1", "S2"]), ["S1"]);
    // back is TRUE but S2 is not active: nothing moves.
    r.set("back", Value::Bool(true));
    r.scan_at(10);
    assert_eq!(active(&r, &["S1", "S2"]), ["S1"]);
    r.set("back", Value::Bool(false));
    r.set("go", Value::Bool(true));
    r.scan_at(20);
    assert_eq!(active(&r, &["S1", "S2"]), ["S2"]);
    // go stays TRUE: S1 is not active, so its transition does not clear again.
    r.scan_at(30);
    assert_eq!(active(&r, &["S1", "S2"]), ["S2"]);
    r.set("back", Value::Bool(true));
    r.scan_at(40);
    assert_eq!(active(&r, &["S1", "S2"]), ["S1"]);
}

#[test]
fn a_step_activated_in_a_scan_is_active_for_a_whole_scan_before_the_transition_after_it_is_tested()
{
    // §6.7.5: testing of the successor transition conditions of an active step is not
    // performed until the effects of its activation have propagated; a step's activity never
    // lasts zero. With go and back both TRUE the chart moves one step per scan, never two.
    let mut r = run(&[two_steps()
        .action("Count", "n := n + 1;")
        .vars(&[("n", "INT")])
        .actions_of(5, 200, 100, "3", &[(6, "N", None, Act::Name("Count"))])]);
    r.set("go", Value::Bool(true));
    r.set("back", Value::Bool(true));
    let mut seen = Vec::new();
    for k in 0..6 {
        r.scan_at(k * 10);
        seen.push(active(&r, &["S1", "S2"]).join(""));
    }
    assert_eq!(seen, ["S2", "S1", "S2", "S1", "S2", "S1"]);
    // S2 was active in three scans, and its action ran in each of them: after it was
    // activated, before its transition was tested.
    assert_eq!(r.int("n"), 3);

    // The effects of an activation are the actions it runs: a transition sees what the actions
    // of the step before it did in that scan, the initial step's in the first scan.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("ready", "BOOL"), ("done", "BOOL")])
        .step(1, 100, 0, "Prepare", true, &[])
        .trans(2, 100, 40, "ready", &["1"])
        .step(3, 100, 80, "Work", false, &["2"])
        .trans(4, 100, 120, "done", &["3"])
        .jump_step(5, 100, 160, "Prepare", &["4"])
        .actions_of(
            6,
            200,
            0,
            "1",
            &[(7, "N", None, Act::St("ready := TRUE; done := FALSE;"))],
        )
        .actions_of(
            8,
            200,
            80,
            "3",
            &[(9, "N", None, Act::St("done := TRUE; ready := FALSE;"))],
        );
    let mut r = run(&[main]);
    let mut seen = Vec::new();
    for k in 0..4 {
        r.scan_at(k * 10);
        seen.push(active(&r, &["Prepare", "Work"]).join(""));
    }
    assert_eq!(seen, ["Work", "Prepare", "Work", "Prepare"]);
}

#[test]
fn the_step_time_counts_from_t0_at_activation_and_holds_its_value_once_deactivated() {
    // §6.7.2: ***.T is the time since the step was initiated; reset to t#0s when it is
    // activated; it keeps the value it had when the step is deactivated.
    let mut r = run(&[two_steps()]);
    r.scan_at(0);
    assert_eq!(r.int("S1.T"), 0);
    r.scan_at(30);
    assert_eq!(r.int("S1.T"), 30 * support::MS);
    r.set("go", Value::Bool(true));
    r.scan_at(50);
    // S1 was deactivated at 50, after its time grew to 50 in this scan; S2 starts from 0.
    assert_eq!(r.int("S1.T"), 50 * support::MS);
    assert_eq!(r.int("S2.T"), 0);
    r.set("go", Value::Bool(false));
    r.scan_at(80);
    assert_eq!(r.int("S1.T"), 50 * support::MS, "held once S1 is inactive");
    assert_eq!(
        r.int("S2.T"),
        30 * support::MS,
        "the time since S2 was activated"
    );
}

#[test]
fn the_program_reads_a_steps_flag_and_time_in_a_condition_and_in_an_action() {
    // Table 54 features 3a and 4a: ***.X and ***.T are read as variables.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("was", "BOOL"), ("waited", "TIME")])
        .step(1, 100, 0, "Wait", true, &["4"])
        .trans(2, 100, 50, "Wait.T >= T#100ms", &["1"])
        .step(3, 100, 100, "Done", false, &["2"])
        .trans(4, 100, 150, "Done.T >= T#20ms", &["3"])
        .actions_of(
            5,
            200,
            100,
            "3",
            &[(6, "N", None, Act::St("was := Wait.X; waited := Wait.T;"))],
        );
    let mut r = run(&[main]);
    let mut t = 0;
    while !r.bool("Done.X") {
        r.scan_at(t);
        t += 10;
    }
    // Wait cleared at the first scan where its time reached 100 ms.
    assert_eq!(t - 10, 100);
    r.scan_at(t);
    assert!(!r.bool("was"));
    assert_eq!(r.int("waited"), 100 * support::MS);
}

#[test]
fn clearing_a_transition_deactivates_every_step_before_it_and_activates_every_step_after_it() {
    // §6.7.5, Table 61 features 4a and 4c: a simultaneous divergence activates both branches;
    // the convergence's transition is enabled only when both steps above it are active, and
    // clearing it deactivates both.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("join", "BOOL"),
        ])
        .step(1, 100, 0, "Start", true, &["12"])
        .trans(2, 100, 40, "go", &["1"])
        .sim_div(3, 100, 70, &["2"])
        .step(4, 50, 100, "A1", false, &["3"])
        .trans(5, 50, 140, "a", &["4"])
        .step(6, 50, 180, "A2", false, &["5"])
        .step(7, 150, 100, "B1", false, &["3"])
        .trans(8, 150, 140, "b", &["7"])
        .step(9, 150, 180, "B2", false, &["8"])
        .sim_conv(10, 100, 220, &["6", "9"])
        .trans(12, 100, 250, "join", &["10"]);
    let steps = ["Start", "A1", "A2", "B1", "B2"];
    let mut r = run(&[main]);
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    assert_eq!(active(&r, &steps), ["A1", "B1"]);
    r.set("join", Value::Bool(true));
    r.set("a", Value::Bool(true));
    r.scan_at(10);
    // The branches evolve independently; join waits for B2.
    assert_eq!(active(&r, &steps), ["A2", "B1"]);
    r.scan_at(20);
    assert_eq!(active(&r, &steps), ["A2", "B1"]);
    // The enabled flag kept for monitoring: join is not enabled while only A2 is active.
    assert!(!r.bool("__SFC_12_EN"));
    r.set("b", Value::Bool(true));
    r.scan_at(30);
    assert_eq!(active(&r, &steps), ["A2", "B2"]);
    r.set("go", Value::Bool(false));
    r.scan_at(40);
    assert_eq!(active(&r, &steps), ["Start"]);
    assert!(r.bool("__SFC_12_EN") && r.bool("__SFC_12"));
}

/// S0 with three transitions in a selection divergence, each to its own step.
fn divergence(prio: Option<[u64; 3]>) -> Pou {
    let mut p = Pou::program("Main")
        .sfc()
        .vars(&[
            ("c1", "BOOL"),
            ("c2", "BOOL"),
            ("c3", "BOOL"),
            ("home", "BOOL"),
        ])
        .step(1, 200, 0, "S0", true, &["20"])
        .sel_div(2, 200, 40, &["1"]);
    let xs = [100, 200, 300];
    for k in 0..3 {
        let (tid, sid, back) = (3 + k as u64 * 3, 4 + k as u64 * 3, 5 + k as u64 * 3);
        let cond = format!("c{}", k + 1);
        p = match prio {
            Some(ps) => p.trans_prio(tid, xs[k], 60, ps[k], &cond, &["2"]),
            None => p.trans(tid, xs[k], 60, &cond, &["2"]),
        };
        p = p
            .step(
                sid,
                xs[k],
                100,
                &format!("B{}", k + 1),
                false,
                &[&tid.to_string()],
            )
            .trans(back, xs[k], 140, "home", &[&sid.to_string()]);
    }
    p.sel_conv(20, 200, 170, &["5", "8", "11"])
}

#[test]
fn a_selection_divergence_clears_the_leftmost_transition_whose_condition_is_true() {
    // Table 61 feature 2a: left-to-right priority.
    let names = ["S0", "B1", "B2", "B3"];
    for (c, want) in [
        ([true, true, true], "B1"),
        ([false, true, true], "B2"),
        ([false, false, true], "B3"),
        ([true, false, true], "B1"),
    ] {
        let mut r = run(&[divergence(None)]);
        r.set("c1", Value::Bool(c[0]));
        r.set("c2", Value::Bool(c[1]));
        r.set("c3", Value::Bool(c[2]));
        r.scan_at(0);
        assert_eq!(active(&r, &names), [want], "{c:?}");
    }
}

#[test]
fn numbered_branches_clear_the_lowest_numbered_transition_whose_condition_is_true() {
    // Table 61 feature 2b: the lowest number has the highest priority. Here the rightmost
    // branch is 1 and the leftmost 3.
    let names = ["S0", "B1", "B2", "B3"];
    for (c, want) in [
        ([true, true, true], "B3"),
        ([true, true, false], "B2"),
        ([true, false, false], "B1"),
    ] {
        let mut r = run(&[divergence(Some([3, 2, 1]))]);
        r.set("c1", Value::Bool(c[0]));
        r.set("c2", Value::Bool(c[1]));
        r.set("c3", Value::Bool(c[2]));
        r.scan_at(0);
        assert_eq!(active(&r, &names), [want], "{c:?}");
    }
}

#[test]
fn a_selection_convergence_activates_the_step_after_it_from_any_branch() {
    // Table 61 feature 3.
    for branch in 0..3 {
        let mut r = run(&[divergence(None)]);
        r.set(&format!("c{}", branch + 1), Value::Bool(true));
        r.scan_at(0);
        r.set(&format!("c{}", branch + 1), Value::Bool(false));
        r.set("home", Value::Bool(true));
        r.scan_at(10);
        assert_eq!(
            active(&r, &["S0", "B1", "B2", "B3"]),
            ["S0"],
            "branch {branch}"
        );
    }
}

#[test]
fn transitions_that_can_clear_together_clear_in_the_same_scan() {
    // §6.7.5: several transitions which can be cleared simultaneously are cleared
    // simultaneously. Two parallel branches each move on in one scan.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL"), ("t", "BOOL"), ("done", "BOOL")])
        .step(1, 100, 0, "S", true, &["12"])
        .trans(2, 100, 40, "go", &["1"])
        .sim_div(3, 100, 70, &["2"])
        .step(4, 50, 100, "A1", false, &["3"])
        .trans(5, 50, 140, "t", &["4"])
        .step(6, 50, 180, "A2", false, &["5"])
        .step(7, 150, 100, "B1", false, &["3"])
        .trans(8, 150, 140, "t", &["7"])
        .step(9, 150, 180, "B2", false, &["8"])
        .sim_conv(10, 100, 220, &["6", "9"])
        .trans(12, 100, 250, "done", &["10"]);
    let mut r = run(&[main]);
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    r.set("t", Value::Bool(true));
    r.scan_at(10);
    assert_eq!(active(&r, &["A1", "A2", "B1", "B2"]), ["A2", "B2"]);
}

#[test]
fn a_jump_step_activates_the_step_it_names() {
    // Table 61 feature 6 (a sequence loop), drawn with a jump: S3 goes back to S1.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("t", "BOOL")])
        .step(1, 100, 0, "S1", true, &[])
        .trans(2, 100, 40, "t", &["1"])
        .step(3, 100, 80, "S2", false, &["2"])
        .trans(4, 100, 120, "t", &["3"])
        .step(5, 100, 160, "S3", false, &["4"])
        .trans(6, 100, 200, "t", &["5"])
        .jump_step(7, 100, 240, "S1", &["6"]);
    let mut r = run(&[main]);
    r.set("t", Value::Bool(true));
    let mut seen = Vec::new();
    for k in 0..4 {
        r.scan_at(k * 10);
        seen.push(active(&r, &["S1", "S2", "S3"]).join(""));
    }
    assert_eq!(seen, ["S2", "S3", "S1", "S2"]);
}

#[test]
fn a_sequence_skip_and_a_sequence_loop_follow_their_selections() {
    // Table 61 features 5a and 6a: S30 to S33 skipping S31, S32 when a is FALSE and d TRUE;
    // and from S32 back to S31 when c is FALSE and e TRUE.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("d", "BOOL"),
            ("e", "BOOL"),
        ])
        .step(1, 100, 0, "S30", true, &[])
        .sel_div(2, 100, 30, &["1"])
        .trans(3, 100, 50, "a", &["2"])
        .sel_conv(4, 100, 70, &["3", "13"])
        .step(5, 100, 90, "S31", false, &["4"])
        .trans(6, 100, 120, "b", &["5"])
        .step(7, 100, 150, "S32", false, &["6"])
        .sel_div(8, 100, 180, &["7"])
        .trans(9, 100, 200, "c", &["8"])
        .trans(10, 200, 50, "d", &["2"])
        .sel_conv(11, 100, 230, &["9", "10"])
        .step(12, 100, 260, "S33", false, &["11"])
        .trans(13, 160, 200, "e", &["8"]);
    let names = ["S30", "S31", "S32", "S33"];
    let mut r = run(&[main]);
    r.set("d", Value::Bool(true));
    r.scan_at(0);
    assert_eq!(active(&r, &names), ["S33"], "skipped");

    let main = Pou::program("Main")
        .sfc()
        .vars(&[
            ("a", "BOOL"),
            ("b", "BOOL"),
            ("c", "BOOL"),
            ("d", "BOOL"),
            ("e", "BOOL"),
        ])
        .step(1, 100, 0, "S30", true, &[])
        .sel_div(2, 100, 30, &["1"])
        .trans(3, 100, 50, "a", &["2"])
        .sel_conv(4, 100, 70, &["3", "13"])
        .step(5, 100, 90, "S31", false, &["4"])
        .trans(6, 100, 120, "b", &["5"])
        .step(7, 100, 150, "S32", false, &["6"])
        .sel_div(8, 100, 180, &["7"])
        .trans(9, 100, 200, "c", &["8"])
        .trans(10, 200, 50, "d", &["2"])
        .sel_conv(11, 100, 230, &["9", "10"])
        .step(12, 100, 260, "S33", false, &["11"])
        .trans(13, 160, 200, "e", &["8"]);
    let mut r = run(&[main]);
    r.set("a", Value::Bool(true));
    r.scan_at(0);
    r.set("a", Value::Bool(false));
    r.set("b", Value::Bool(true));
    r.scan_at(10);
    assert_eq!(active(&r, &names), ["S32"]);
    r.set("b", Value::Bool(false));
    r.set("e", Value::Bool(true));
    r.scan_at(20);
    assert_eq!(active(&r, &names), ["S31"], "looped back");
}

#[test]
fn a_named_transition_and_a_negated_condition_are_the_condition() {
    // Table 55 feature 9a: a transition name refers to a TRANSITION of the POU; TC6's negated
    // condition is its negation.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("level", "INT")])
        .transition("Full", "level >= 10")
        .step(1, 100, 0, "Filling", true, &["4"])
        .trans_ref(2, 100, 40, "Full", &["1"])
        .step(3, 100, 80, "Draining", false, &["2"])
        .raw("<transition localId=\"4\"><position x=\"100\" y=\"120\"/><connectionPointIn><connection refLocalId=\"3\"/></connectionPointIn><connectionPointOut/><condition negated=\"true\"><reference name=\"Low\"/></condition></transition>")
        .transition("Low", "level > 2");
    let mut r = run(&[main]);
    for (level, want) in [
        (5, "Filling"),
        (10, "Draining"),
        (4, "Draining"),
        (2, "Filling"),
    ] {
        r.set("level", Value::Int(level));
        r.scan_at(level as i64);
        assert_eq!(
            active(&r, &["Filling", "Draining"]),
            [want],
            "level {level}"
        );
    }
}

/// Idle (initial) --go--> S --stop--> Off --reset--> Idle: S has an action block of `qual`
/// (with `duration`) on the BOOL variable `act`, and Idle an R association of it.
fn qualified(qual: &str, duration: Option<&str>) -> Pou {
    qualified_reset_in(qual, duration, "1")
}

/// [`qualified`], the R association in the step `r_in` (`"1"` Idle, `"5"` Off).
fn qualified_reset_in(qual: &str, duration: Option<&str>, r_in: &str) -> Pou {
    Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("stop", "BOOL"),
            ("act", "BOOL"),
            ("reset", "BOOL"),
        ])
        .step(1, 100, 0, "Idle", true, &["8"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "S", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"])
        .step(5, 100, 160, "Off", false, &["4"])
        .trans(8, 100, 200, "reset", &["5"])
        .actions_of(6, 200, 80, "3", &[(7, qual, duration, Act::Name("act"))])
        .actions_of(9, 200, 0, r_in, &[(10, "R", None, Act::Name("act"))])
}

/// The value of `act` after each scan, 10 ms apart, with `go`, `stop` and `reset` at each.
fn trace(p: Pou, inputs: &[(bool, bool, bool)]) -> String {
    let mut r = run(&[p]);
    let mut out = String::new();
    for (k, (go, stop, reset)) in inputs.iter().enumerate() {
        r.set("go", Value::Bool(*go));
        r.set("stop", Value::Bool(*stop));
        r.set("reset", Value::Bool(*reset));
        r.scan_at(k as i64 * 10);
        out.push(if r.bool("act") { '1' } else { '0' });
    }
    out
}

const G: (bool, bool, bool) = (true, false, false);
const W: (bool, bool, bool) = (false, false, false);
const STOP: (bool, bool, bool) = (false, true, false);
const RESET: (bool, bool, bool) = (false, false, true);

#[test]
fn an_n_action_is_active_exactly_while_its_step_is() {
    // Table 59 feature 2; Figure 23 b): Q is N. S is entered in scan 0 and its actions run
    // from scan 1; it is left in scan 3.
    assert_eq!(
        trace(qualified("N", None), &[G, W, W, STOP, W, W]),
        "011100"
    );
}

#[test]
fn an_s_action_stays_active_after_its_step_until_an_r_association_resets_it() {
    // Table 59 features 3 and 4; Figure 23: S sets S_FF, R resets it and overrides Q. S is
    // left in scan 2; the action stays active in Off until Idle's R in scan 6.
    assert_eq!(
        trace(qualified("S", None), &[G, W, STOP, W, W, RESET, W]),
        "0111110"
    );
    // R in Off resets S_FF: once Off is left for Idle, with no R active, the action stays
    // inactive; it does not come back when R does not hold it down.
    assert_eq!(
        trace(
            qualified_reset_in("S", None, "5"),
            &[G, W, STOP, W, RESET, W, W]
        ),
        "0110000"
    );
}

#[test]
fn an_l_action_is_active_for_its_duration_at_most() {
    // Table 59 feature 5: Q is L AND NOT L_TMR.Q, a TON of the duration on L.
    assert_eq!(
        trace(qualified("L", Some("T#20ms")), &[G, W, W, W, W, STOP, W]),
        "0110000"
    );
    // Left before its duration: ends with the step.
    assert_eq!(
        trace(qualified("L", Some("T#100ms")), &[G, W, STOP, W]),
        "0110"
    );
}

#[test]
fn a_d_action_starts_after_its_duration_and_only_while_its_step_is_active() {
    // Table 59 feature 6: Q is D_TMR.Q, a TON of the duration on D.
    assert_eq!(
        trace(qualified("D", Some("T#20ms")), &[G, W, W, W, W, STOP, W]),
        "0001110"
    );
    // Left before its delay: never active.
    assert_eq!(
        trace(qualified("D", Some("T#100ms")), &[G, W, STOP, W, W]),
        "00000"
    );
}

#[test]
fn a_p_action_is_active_for_one_scan_when_its_step_becomes_active() {
    // Table 59 feature 7: Q is R_TRIG(P).Q.
    assert_eq!(trace(qualified("P", None), &[G, W, W, STOP, W]), "01000");
    // And P1 (feature 11), without final scan (Table 60 feature 2), the same.
    assert_eq!(trace(qualified("P1", None), &[G, W, W, STOP, W]), "01000");
}

#[test]
fn a_ds_action_is_stored_only_if_its_step_lasted_its_delay() {
    // Table 59 feature 9: DS_TMR on DS, DS_FF set by it and reset by R.
    assert_eq!(
        trace(
            qualified("DS", Some("T#20ms")),
            &[G, W, W, W, STOP, W, W, RESET, W]
        ),
        "000111110"
    );
    assert_eq!(
        trace(qualified("DS", Some("T#100ms")), &[G, W, STOP, W, W]),
        "00000"
    );
    // R resets DS_FF: not active again once R is no longer.
    assert_eq!(
        trace(
            qualified_reset_in("DS", Some("T#20ms"), "5"),
            &[G, W, W, W, STOP, W, RESET, W, W]
        ),
        "000110000"
    );
}

#[test]
fn an_sd_action_starts_its_full_delay_after_it_was_stored_whether_or_not_its_step_was_left() {
    // Table 59 feature 8 (stored and time delayed), ADR-013 §2.17. S is active from scan 1
    // (10 ms), which stores the action; it is left in scan 2, 10 ms into the 30 ms delay. The
    // action starts at 40 ms, the full delay: by Figure 23 with rule c's T#0s it would have
    // started in scan 3, at once. It stays active until Idle's R (scan 8).
    assert_eq!(
        trace(
            qualified("SD", Some("T#30ms")),
            &[G, W, STOP, W, W, W, W, RESET, W]
        ),
        "000011110"
    );
    // The same delay when its step is still active.
    assert_eq!(
        trace(
            qualified("SD", Some("T#30ms")),
            &[G, W, W, W, W, W, STOP, RESET, W]
        ),
        "000011110"
    );
}

#[test]
fn an_r_association_ends_an_sd_delay_and_the_next_one_runs_in_full() {
    // R in Off (scan 3) resets SD_FF while the delay runs: the action never starts. Stored
    // again from scan 6 (60 ms), it starts 30 ms later, in scan 9: nothing is left of the
    // first delay.
    assert_eq!(
        trace(
            qualified_reset_in("SD", Some("T#30ms"), "5"),
            &[G, W, STOP, W, RESET, G, W, W, W, W, W]
        ),
        "00000000011"
    );
}

#[test]
fn an_sl_action_runs_its_full_limit_whether_or_not_its_step_was_left_and_no_longer() {
    // Table 59 feature 10 (stored and time limited), ADR-013 §2.17. Stored in scan 1 (10 ms),
    // its step left in scan 2: it runs until 40 ms, the full 30 ms. By Figure 23 with rule
    // c's T#0s it would have ended in scan 3, at once.
    assert_eq!(
        trace(
            qualified("SL", Some("T#30ms")),
            &[G, W, STOP, W, W, W, RESET, W]
        ),
        "01110000"
    );
    // No longer while its step stays active.
    assert_eq!(
        trace(
            qualified("SL", Some("T#30ms")),
            &[G, W, W, W, W, W, STOP, W]
        ),
        "01110000"
    );
    // R ends it at once (Off, scan 3), 70 ms before its limit.
    assert_eq!(
        trace(
            qualified_reset_in("SL", Some("T#100ms"), "5"),
            &[G, W, STOP, W, W]
        ),
        "01100"
    );
}

/// Idle (initial) --go--> A --stop--> B --reset--> Idle, with no action block: no R on the way
/// round, unless a test puts one.
fn three_steps(more: &[(&str, &str)]) -> Pou {
    let mut vars = vec![("go", "BOOL"), ("stop", "BOOL"), ("reset", "BOOL")];
    vars.extend_from_slice(more);
    Pou::program("Main")
        .sfc()
        .vars(&vars)
        .step(1, 100, 0, "Idle", true, &["8"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "A", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"])
        .step(5, 100, 160, "B", false, &["4"])
        .trans(8, 100, 200, "reset", &["5"])
}

/// `qual` on `act` in A with `in_a`, and in B with `in_b`.
fn stored_twice(qual: &str, in_a: &str, in_b: &str) -> Pou {
    three_steps(&[("act", "BOOL")])
        .actions_of(6, 200, 80, "3", &[(7, qual, Some(in_a), Act::Name("act"))])
        .actions_of(
            9,
            200,
            160,
            "5",
            &[(10, qual, Some(in_b), Act::Name("act"))],
        )
}

#[test]
fn an_sd_action_two_steps_store_starts_only_when_the_delay_of_each_is_over() {
    // Found in review: with one duration per action, B's T#20ms replaced A's T#100ms while the
    // timer ran from A's store, and the action started 20 ms after A stored it. Each
    // association is timed by itself (ADR-013 §2.17): A stores at 10 ms, so not before 110 ms.
    assert_eq!(
        trace(
            stored_twice("SD", "T#100ms", "T#20ms"),
            &[G, W, STOP, W, W, W, W, W, W, W, W, W, W]
        ),
        "0000000000011"
    );
    // And B's own delay in full: A (T#20ms, over at 30 ms) is left in scan 1; B stores at
    // 20 ms with T#100ms, so not before 120 ms.
    assert_eq!(
        trace(
            stored_twice("SD", "T#20ms", "T#100ms"),
            &[G, STOP, W, W, W, W, W, W, W, W, W, W, W, W]
        ),
        "00000000000011"
    );
}

#[test]
fn an_sd_action_that_has_started_stays_active_until_r_whatever_stores_it_again() {
    // Found in review: A's T#20ms starts the action in scan 3; B, entered in scan 5 with
    // T#100ms, raised the one timer's PT above its elapsed time and the action stopped, with
    // no R. Started is kept until R.
    assert_eq!(
        trace(
            stored_twice("SD", "T#20ms", "T#100ms"),
            &[G, W, W, W, STOP, W, W, W, W, W, W, W, W]
        ),
        "0001111111111"
    );
}

#[test]
fn an_sd_step_entered_again_while_its_delay_runs_starts_the_delay_again() {
    // No R on the way round. A stores at 10 ms with T#60ms and is left; entered again at
    // 50 ms, the delay counts from there: 110 ms, not 70 ms. In doubt the action is off.
    let p = three_steps(&[("act", "BOOL")]).actions_of(
        6,
        200,
        80,
        "3",
        &[(7, "SD", Some("T#60ms"), Act::Name("act"))],
    );
    assert_eq!(
        trace(p, &[G, W, STOP, RESET, G, W, W, W, W, W, W, W, W]),
        "0000000000011"
    );
}

#[test]
fn an_sl_action_ends_at_the_first_limit_that_is_over_and_stays_ended_until_r() {
    // Each association's limit counts from when its step first stored the action; the first
    // one over ends it. A at 10 ms with T#100ms, B at 30 ms with T#20ms: ended at 50 ms.
    assert_eq!(
        trace(
            stored_twice("SL", "T#100ms", "T#20ms"),
            &[G, W, STOP, W, W, W, W, W]
        ),
        "01111000"
    );
    // A's T#20ms is over at 30 ms: B's longer limit does not start it again.
    assert_eq!(
        trace(
            stored_twice("SL", "T#20ms", "T#100ms"),
            &[G, W, STOP, W, W, W]
        ),
        "011000"
    );
    // Nor does its own step entered again, without an R between (scan 5).
    let p = three_steps(&[("act", "BOOL")]).actions_of(
        6,
        200,
        80,
        "3",
        &[(7, "SL", Some("T#30ms"), Act::Name("act"))],
    );
    assert_eq!(trace(p, &[G, W, STOP, RESET, G, W, W, W]), "01110000");
}

/// `act` over seven scans with `qual` on it in A, which stays active, its duration the variable
/// `d`: T#20ms until scan 3, a minute from scan 4.
fn with_a_duration_that_grows(qual: &str) -> String {
    let p = three_steps(&[("act", "BOOL"), ("d", "TIME")]).actions_of(
        6,
        200,
        80,
        "3",
        &[(7, qual, Some("d"), Act::Name("act"))],
    );
    let mut r = run(&[p]);
    let mut out = String::new();
    for k in 0..7 {
        let d = if k < 4 { 20 } else { 60_000 };
        r.set("d", Value::Time(d * support::MS));
        r.set("go", Value::Bool(k == 0));
        r.scan_at(k * 10);
        out.push(if r.bool("act") { '1' } else { '0' });
    }
    out
}

#[test]
fn an_sd_action_that_has_started_stays_so_when_its_duration_grows() {
    // The duration is read while the step is active. The T#20ms is over in scan 3 and the
    // action starts; from scan 4 the variable is a minute. A TON's Q falls when its PT passes
    // its elapsed time, which would stop the action with no R. Started is kept.
    assert_eq!(with_a_duration_that_grows("SD"), "0001111");
}

#[test]
fn an_sl_action_that_has_ended_stays_so_when_its_duration_grows() {
    // The limit is over in scan 3 and the action ends; the minute from scan 4 would make the
    // timer's Q fall and start the action again, with no R. Ended is kept.
    assert_eq!(with_a_duration_that_grows("SL"), "0110000");
}

#[test]
fn several_p0_associations_of_an_action_are_one_falling_edge_as_figure_23_has_them() {
    // A and B both pulse `act` on P0, and the chart goes from A straight into B: the input is
    // the OR of the two steps (Figure 23, as P and P1), which does not fall until B is left
    // (scan 4). In doubt the action is off.
    let p = three_steps(&[("act", "BOOL")])
        .actions_of(6, 200, 80, "3", &[(7, "P0", None, Act::Name("act"))])
        .actions_of(9, 200, 160, "5", &[(10, "P0", None, Act::Name("act"))]);
    assert_eq!(trace(p, &[G, W, STOP, W, RESET, W, W]), "0000010");
}

#[test]
fn an_action_named_as_a_part_of_another_action_s_control_shares_nothing_with_it() {
    // Found in review: `act`'s kept SD duration was `__SFC_ACT_SD_T`, which is also the T of
    // an action named `act_SD`; its T#0s, written every scan, ended `act`'s delay at once.
    for other in [
        "act_SD",
        "act_SD_7",
        "act_SD_7_KEPT",
        "act_SL",
        "act_P0",
        "other",
    ] {
        let p = three_steps(&[("act", "BOOL"), (other, "BOOL")])
            .actions_of(
                6,
                200,
                80,
                "3",
                &[(7, "SD", Some("T#100ms"), Act::Name("act"))],
            )
            .actions_of(
                9,
                200,
                0,
                "1",
                &[(10, "L", Some("T#1ms"), Act::Name(other))],
            );
        assert_eq!(
            trace(p, &[G, W, STOP, W, W, W, W, W, W, W, W, W, W]),
            "0000000000011",
            "{other}"
        );
    }
}

#[test]
fn an_sd_or_sl_action_keeps_the_duration_a_variable_had_while_its_step_was_active() {
    // The duration is read each scan its step is active and kept from then on: a variable
    // changed after the step was left changes nothing of a delay or a limit that is running.
    for (qual, expected) in [("SD", "0000111"), ("SL", "0111000")] {
        let p = Pou::program("Main")
            .sfc()
            .vars(&[
                ("go", "BOOL"),
                ("stop", "BOOL"),
                ("act", "BOOL"),
                ("reset", "BOOL"),
                ("d", "TIME"),
            ])
            .step(1, 100, 0, "Idle", true, &["8"])
            .trans(2, 100, 40, "go", &["1"])
            .step(3, 100, 80, "S", false, &["2"])
            .trans(4, 100, 120, "stop", &["3"])
            .step(5, 100, 160, "Off", false, &["4"])
            .trans(8, 100, 200, "reset", &["5"])
            .actions_of(6, 200, 80, "3", &[(7, qual, Some("d"), Act::Name("act"))]);
        let mut r = run(&[p]);
        let mut out = String::new();
        for (k, (go, stop)) in [
            (true, false),
            (false, false),
            (false, true),
            (false, false),
            (false, false),
            (false, false),
            (false, false),
        ]
        .iter()
        .enumerate()
        {
            // 30 ms while S is active (scans 1 and 2); zero, then a minute, once it is left.
            let d = match k {
                0..=2 => 30,
                3 => 0,
                _ => 60_000,
            };
            r.set("d", Value::Time(d * support::MS));
            r.set("go", Value::Bool(*go));
            r.set("stop", Value::Bool(*stop));
            r.scan_at(k as i64 * 10);
            out.push(if r.bool("act") { '1' } else { '0' });
        }
        assert_eq!(out, expected, "{qual}");
    }
}

#[test]
fn a_p0_action_is_active_for_one_scan_after_its_step_was_left_and_never_at_a_start() {
    // Table 59 feature 12 (pulse, falling edge), without final scan. S is left in scan 3; the
    // action is active in scan 4 only.
    assert_eq!(
        trace(qualified("P0", None), &[G, W, W, STOP, W, W]),
        "000010"
    );
    // At a cold start no step was left: nothing runs (the R association is in Off here, so
    // it is not what keeps the action off in Idle). Figure 23's F_TRIG is TRUE at its first
    // execution (Table 44), which would run the action in scan 0 whatever its step.
    assert_eq!(
        trace(qualified_reset_in("P0", None, "5"), &[W, W, W]),
        "000"
    );
    // And again each time its step is left.
    assert_eq!(
        trace(
            qualified_reset_in("P0", None, "1"),
            &[G, STOP, W, RESET, G, STOP, W, W]
        ),
        "00100010"
    );
}

#[test]
fn a_cold_start_drops_an_sd_delay_that_was_running_and_runs_no_p0_action() {
    // Nothing of the action control is retained (ADR-013 §2.17): after a cold start the chart
    // is at its initial step, the delay that was running is gone and the action never starts;
    // a step that was active before the cold start was not left by the chart, so P0 is silent.
    for qual in ["SD", "P0"] {
        let d = (qual == "SD").then_some("T#30ms");
        let mut r = run(&[qualified_reset_in(qual, d, "5")]);
        r.set("go", Value::Bool(true));
        r.scan_at(0);
        r.set("go", Value::Bool(false));
        r.scan_at(10);
        assert!(r.bool("S.X"), "{qual}");
        let p = Program::load(&r.c.bytecode).unwrap();
        cold_start(&p, &mut r.memory).unwrap();
        for k in 0..8 {
            r.scan_at(k * 10);
            assert!(r.bool("Idle.X") && !r.bool("S.X"), "{qual} scan {k}");
            assert!(!r.bool("act"), "{qual} scan {k}");
        }
    }
}

#[test]
fn an_action_of_statements_runs_while_it_is_active_and_its_flag_says_so() {
    // §6.7.4.6 rule a, Table 60 feature 2: the statements run while Q is TRUE, and Q is read
    // as Count.Q. Without final scan, they do not run once more after Q falls, nor at a cold
    // start.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("stop", "BOOL"),
            ("n", "INT"),
            ("q", "BOOL"),
        ])
        .action("Count", "n := n + 1; q := Count.Q;")
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "Run", false, &["2"])
        .trans(4, 100, 120, "stop", &["3"])
        .actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("Count"))]);
    let mut r = run(&[main]);
    r.scan_at(0);
    assert_eq!(r.int("n"), 0, "nothing at a cold start");
    r.set("go", Value::Bool(true));
    r.scan_at(10);
    r.set("go", Value::Bool(false));
    r.scan_at(20);
    r.scan_at(30);
    assert_eq!(r.int("n"), 2);
    assert!(r.bool("q") && r.bool("Count.Q"));
    r.set("stop", Value::Bool(true));
    r.scan_at(40);
    r.set("stop", Value::Bool(false));
    r.scan_at(50);
    r.scan_at(60);
    assert_eq!(r.int("n"), 3, "no final scan");
    assert!(!r.bool("Count.Q"));
}

#[test]
fn an_action_associated_with_several_steps_is_active_while_any_of_them_is() {
    // §6.7.4.6 rule b: an input is TRUE when at least one of its associations is active.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("t", "BOOL"), ("lamp", "BOOL")])
        .step(1, 100, 0, "A", true, &["6"])
        .trans(2, 100, 40, "t", &["1"])
        .step(3, 100, 80, "B", false, &["2"])
        .trans(4, 100, 120, "t", &["3"])
        .step(5, 100, 160, "C", false, &["4"])
        .trans(6, 100, 200, "t", &["5"])
        .actions_of(7, 200, 0, "1", &[(8, "N", None, Act::Name("lamp"))])
        .actions_of(9, 200, 160, "5", &[(10, "N", None, Act::Name("lamp"))]);
    let mut r = run(&[main]);
    r.set("t", Value::Bool(true));
    let mut seen = String::new();
    for k in 0..6 {
        r.scan_at(k * 10);
        seen.push(if r.bool("lamp") { '1' } else { '0' });
    }
    // Active in A (scan 0), not in B (scan 1), in C (scan 2), …
    assert_eq!(seen, "101101");
}

#[test]
fn a_chart_in_a_function_block_keeps_each_instance_s_steps() {
    // §6.7.1: a function block may be a chart; each instance has its own steps (§6.7.2: the
    // scope of step names is the POU).
    let seq = Pou::function_block("Seq")
        .sfc()
        .inputs(&[("t", "BOOL")])
        .outputs(&[("working", "BOOL")])
        .step(1, 100, 0, "Ready", true, &["4"])
        .trans(2, 100, 40, "t", &["1"])
        .step(3, 100, 80, "Busy", false, &["2"])
        .trans(4, 100, 120, "NOT t", &["3"])
        .actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("working"))]);
    let main = Pou::program("Main")
        .fbd()
        .vars(&[
            ("a", "Seq"),
            ("b", "Seq"),
            ("x", "BOOL"),
            ("y", "BOOL"),
            ("bx", "BOOL"),
            ("yb", "BOOL"),
        ])
        .invar(1, 0, 0, "x")
        .block(
            2,
            100,
            0,
            "Seq",
            Some("a"),
            &[("t", tc6::In::From(&["1"]))],
            &["working"],
        )
        .outvar(3, 200, 0, "bx", &["2.working"])
        .invar(4, 0, 100, "y")
        .block(
            5,
            100,
            100,
            "Seq",
            Some("b"),
            &[("t", tc6::In::From(&["4"]))],
            &["working"],
        )
        .outvar(6, 200, 100, "yb", &["5.working"]);
    let mut r = run(&[seq, main]);
    r.set("x", Value::Bool(true));
    r.scan_at(0);
    r.scan_at(10);
    assert!(r.bool("a.Busy.X") && !r.bool("b.Busy.X"));
    assert!(r.bool("bx") && !r.bool("yb"));
}

#[test]
fn the_step_times_do_not_count_the_time_across_a_restart() {
    // As the timers (ADR-013 §2.13): the scan clock starts again from zero at a restart; a
    // step's time carries on from what it was, the restart not counted.
    let mut r = run(&[two_steps()]);
    r.scan_at(0);
    r.scan_at(500);
    assert_eq!(r.int("S1.T"), 500 * support::MS);
    // A warm restart: the clock starts again at 0.
    r.scan_at(0);
    assert_eq!(r.int("S1.T"), 500 * support::MS);
    r.scan_at(20);
    assert_eq!(r.int("S1.T"), 520 * support::MS);
}

#[test]
fn every_step_transition_and_action_is_published_with_the_variable_keeping_it() {
    let main = qualified("S", None);
    let c = compile_in(&project(&[main]), Language::Sfc, &TargetProfile::pc()).unwrap();
    let find = |id: u64, at: At| {
        c.probes
            .iter()
            .find(|p| p.local_id == id && p.at == at)
            .unwrap_or_else(|| panic!("no probe {id} {at:?}: {:#?}", c.probes))
            .holds
            .clone()
    };
    let v = |p: &str| Holds::Variable {
        path: p.into(),
        negated: false,
    };
    assert_eq!(find(1, At::Step), v("Idle.X"));
    assert_eq!(find(3, At::StepTime), v("S.T"));
    assert_eq!(
        find(4, At::Transition),
        Holds::Guarded {
            path: "__SFC_4".into(),
            ran: "__SFC_4_EN".into()
        }
    );
    assert_eq!(find(7, At::Action), v("act"));
    assert_eq!(find(10, At::Action), v("act"));
    // Each is a private symbol: a step's flag cannot be written by a tag (Table 54 footnote a).
    for path in [
        "Idle.X",
        "S.T",
        "__SFC_4",
        "__SFC_4_EN",
        "__SFC_ACT_S_FF.Q1",
    ] {
        let s = c.symbol(path).unwrap_or_else(|| panic!("no symbol {path}"));
        assert!(s.private, "{path} is private");
    }
}

/// Load `after` in place of `before` as an online change does: every variable of the same
/// path, type and size carried over, the rest from its initial value.
fn take_over(before: &Run, after: &str) -> Run {
    let mut r = Run::of(compile_in(after, Language::Sfc, &TargetProfile::pc()));
    let p = Program::load(&r.c.bytecode).unwrap();
    let mut memory = vec![0; p.memory_size()];
    cold_start(&p, &mut memory).unwrap();
    for s in &r.c.symbols {
        if s.array.is_some() {
            continue;
        }
        if let Some(old) = before
            .c
            .symbols
            .iter()
            .find(|o| o.path.eq_ignore_ascii_case(&s.path) && o.ty == s.ty && o.array.is_none())
        {
            let ty = support::elem(&s.ty);
            let v = read(&before.memory, old.offset, ty).unwrap();
            assert!(write(&mut memory, s.offset, v));
        }
    }
    r.memory = memory;
    r.now = before.now;
    r
}

#[test]
fn a_build_of_the_same_chart_that_takes_over_the_variables_carries_on_where_it_was() {
    let mut r = run(&[two_steps()]);
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    assert!(r.bool("S2.X"));
    // The same steps and transitions; another condition.
    let after = project(&[Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL"), ("back", "BOOL")])
        .step(1, 100, 0, "S1", true, &["4"])
        .trans(2, 100, 50, "go AND NOT back", &["1"])
        .step(3, 100, 100, "S2", false, &["2"])
        .trans(4, 100, 150, "back", &["3"])]);
    let mut r = take_over(&r, &after);
    r.scan_at(10);
    assert_eq!(active(&r, &["S1", "S2"]), ["S2"]);
}

#[test]
fn a_build_of_another_chart_that_takes_over_the_variables_starts_it_again_from_its_initial_step() {
    // A new initial step S0 before S1: carried by name alone, S2 would stay active beside S0,
    // two steps of one sequence; the chart starts again instead.
    let mut r = run(&[two_steps()]);
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    assert!(r.bool("S2.X"));
    let after = project(&[Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL"), ("back", "BOOL")])
        .step(9, 100, -100, "S0", true, &["4"])
        .trans(8, 100, -50, "go", &["9"])
        .step(1, 100, 0, "S1", false, &["8"])
        .trans(2, 100, 50, "go", &["1"])
        .step(3, 100, 100, "S2", false, &["2"])
        .trans(4, 100, 150, "back", &["3"])]);
    let mut r = take_over(&r, &after);
    assert!(r.bool("S2.X"), "carried by its name");
    r.set("go", Value::Bool(false));
    r.scan_at(10);
    assert_eq!(active(&r, &["S0", "S1", "S2"]), ["S0"]);
}

#[test]
fn a_chart_started_again_by_a_build_of_another_chart_runs_no_p0_action_and_drops_an_sd_delay() {
    // An online change to another chart starts it again from its initial step (above): the
    // step that was active was not left by the chart, so P0 does not pulse, and what was
    // stored is not any more, so the SD delay that was running never starts the action.
    for qual in ["SD", "SL", "P0"] {
        let d = (qual != "P0").then_some("T#30ms");
        let mut r = run(&[qualified_reset_in(qual, d, "5")]);
        r.set("go", Value::Bool(true));
        r.scan_at(0);
        r.set("go", Value::Bool(false));
        r.scan_at(10);
        assert!(r.bool("S.X"), "{qual}");
        // A new initial step Zero before Idle.
        let after = project(&[Pou::program("Main")
            .sfc()
            .vars(&[
                ("go", "BOOL"),
                ("stop", "BOOL"),
                ("act", "BOOL"),
                ("reset", "BOOL"),
            ])
            .step(20, 100, -80, "Zero", true, &["8"])
            .trans(21, 100, -40, "go", &["20"])
            .step(1, 100, 0, "Idle", false, &["21"])
            .trans(2, 100, 40, "go", &["1"])
            .step(3, 100, 80, "S", false, &["2"])
            .trans(4, 100, 120, "stop", &["3"])
            .step(5, 100, 160, "Off", false, &["4"])
            .trans(8, 100, 200, "reset", &["5"])
            .actions_of(6, 200, 80, "3", &[(7, qual, d, Act::Name("act"))])
            .actions_of(9, 200, 0, "5", &[(10, "R", None, Act::Name("act"))])]);
        let mut r = take_over(&r, &after);
        assert!(r.bool("S.X"), "{qual}: carried by its name");
        for k in 2..10 {
            r.scan_at(k * 10);
            assert_eq!(
                active(&r, &["Zero", "Idle", "S", "Off"]),
                ["Zero"],
                "{qual} scan {k}"
            );
            assert!(!r.bool("act"), "{qual} scan {k}");
        }
    }
}

#[test]
fn a_chart_and_an_st_function_block_compile_together() {
    // A chart's action calls a function block of the project's ST source.
    let st = "FUNCTION_BLOCK Twice VAR_INPUT x : INT; END_VAR VAR_OUTPUT y : INT; END_VAR y := 2 * x; END_FUNCTION_BLOCK";
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("t", "Twice"), ("out", "INT")])
        .step(1, 100, 0, "Only", true, &[])
        .actions_of(
            2,
            200,
            0,
            "1",
            &[(3, "N", None, Act::St("t(x := 21); out := t.y;"))],
        );
    let doc = project(&[main]);
    let mut r = Run::of(compile_sources(
        &Sources {
            st: Some(("lib.st", st)),
            tc6: &[("main.xml", &doc)],
            ..Default::default()
        },
        &TargetProfile::pc(),
    ));
    r.scan_at(0);
    assert_eq!(r.int("out"), 42);
}

/// The scans `scripts/logic-vm-on-emulators.sh` runs: 60, 20 ms apart, from the clock's zero;
/// `each` sees each scan's run and the flags of `steps` as the scan before left them (what its
/// actions ran with: a scan runs the actions, then clears transitions).
fn emulator_scans(doc: &str, steps: &[&str], each: &mut dyn FnMut(usize, &Run, &[bool])) -> Run {
    let mut r = Run::of(compile_in(doc, Language::Sfc, &TargetProfile::rp2040()));
    eprintln!(
        "{} bytes of code, {} of memory, {} private symbols",
        r.c.code_size,
        r.c.memory_size,
        r.c.symbols.iter().filter(|s| s.private).count()
    );
    let mut before: Vec<bool> = steps.iter().map(|s| r.bool(&format!("{s}.X"))).collect();
    for k in 0..60 {
        r.scan_at(k * 20);
        each(k as usize, &r, &before);
        before = steps.iter().map(|s| r.bool(&format!("{s}.X"))).collect();
    }
    r
}

#[test]
fn the_bottling_chart_the_emulators_run_does_what_its_steps_say() {
    let doc = include_str!("../../mythos-logic-graphic/tests/sfc/bottling.xml");
    let steps = [
        "Idle", "Convey", "Reject", "Fill", "FillDone", "Cap", "CapDone", "Release",
    ];
    let at = |name: &str| steps.iter().position(|s| *s == name).unwrap();
    let mut trace: Vec<String> = Vec::new();
    let mut conveys = 0;
    let mut longest_fill = 0;
    let r = emulator_scans(doc, &steps, &mut |k, r, was| {
        // The inline action keeps Fill's elapsed time where Fill was active.
        if was[at("Fill")] {
            assert_eq!(r.int("fill_time"), r.int("Fill.T"), "scan {k}");
            longest_fill = longest_fill.max(r.int("fill_time"));
        }
        let now = active(r, &steps);
        // One sequence: Idle, Convey, Reject and Release alone; Fill or FillDone beside Cap or
        // CapDone (a simultaneous divergence, Table 61 feature 4a). Never none, never more.
        let parallel = (r.bool("Fill.X") || r.bool("FillDone.X")) as u8
            + (r.bool("Cap.X") || r.bool("CapDone.X")) as u8;
        assert!(
            (now.len() == 1 && parallel == 0) || (now.len() == 2 && parallel == 2),
            "scan {k}: {now:?}"
        );
        // N: the conveyor ran exactly where Convey was active.
        assert_eq!(r.bool("conveyor"), was[at("Convey")], "scan {k}");
        // L: the capper ran only in Cap, and stopped once Cap had been active 60 ms.
        if r.bool("capper") {
            assert!(was[at("Cap")], "scan {k}");
        }
        if was[at("Cap")] && r.int("Cap.T") >= 80 * support::MS {
            assert!(!r.bool("capper"), "scan {k}");
        }
        // S and R: the valve stays open from Fill until Release resets it.
        if was[at("FillDone")] || was[at("CapDone")] {
            assert!(r.bool("valve"), "scan {k}");
        }
        if was[at("Idle")] || was[at("Convey")] {
            assert!(!r.bool("valve") && !r.bool("label_ok"), "scan {k}");
        }
        if r.bool("Convey.X") && !was[at("Convey")] {
            conveys += 1;
        }
        trace.push(now.join("+"));
    });
    // P: one bottle counted each time Convey became active (but the last, if it did in the
    // last scan); every third rejected (the selection's leftmost transition first, Table 61
    // feature 2a).
    let counted = r.int("bottles");
    assert!(
        counted == conveys || counted + 1 == conveys,
        "{counted} {conveys} {trace:?}"
    );
    assert!(counted >= 3, "{trace:?}");
    assert_eq!(r.int("rejected"), counted / 3, "{trace:?}");
    assert!(trace.iter().any(|t| t == "Reject"), "{trace:?}");
    // Cap (70 ms) is done before Fill (100 ms); the convergence waits for both (4c).
    assert!(trace.iter().any(|t| t == "Fill+CapDone"), "{trace:?}");
    assert!(trace.iter().any(|t| t == "FillDone+CapDone"), "{trace:?}");
    // Fill cleared by the named transition Filled once its time reached 100 ms.
    assert_eq!(longest_fill, 100 * support::MS, "{trace:?}");
}

#[test]
fn the_doors_the_emulators_run_open_and_close_as_their_charts_say() {
    let doc = include_str!("../../mythos-logic-graphic/tests/sfc/doors.xml");
    let (mut opened, mut motor_scans) = (0, 0);
    let steps = ["front.Open", "back.Open"];
    let r = emulator_scans(doc, &steps, &mut |k, r, was| {
        // D: opened only where Open was active, once it had been for 30 ms.
        if r.bool("front_open") {
            assert!(
                was[0] && r.int("front.Open.T") >= 30 * support::MS,
                "scan {k}"
            );
            opened += 1;
        }
        // N: the back door's motor ran exactly where its Open was active.
        assert_eq!(r.bool("back_motor"), was[1], "scan {k}");
        motor_scans += r.bool("back_motor") as usize;
    });
    assert!(opened > 0 && motor_scans > 0, "{opened} {motor_scans}");
    // The two instances keep their own steps (§6.7.2: local to the POU instance).
    assert_ne!(
        (r.bool("front.Open.X"), r.int("front.Open.T")),
        (r.bool("back.Open.X"), r.int("back.Open.T"))
    );
}

/// A chart function block: Ready (initial) --Ready.T >= T#10s--> Busy (N working) --NOT t--> Ready.
fn seq_fb() -> Pou {
    Pou::function_block("Seq")
        .sfc()
        .inputs(&[("t", "BOOL")])
        .outputs(&[("working", "BOOL")])
        .step(1, 100, 0, "Ready", true, &["4"])
        .trans(2, 100, 40, "Ready.T >= T#10s", &["1"])
        .step(3, 100, 80, "Busy", false, &["2"])
        .trans(4, 100, 120, "NOT t", &["3"])
        .actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("working"))])
}

/// An FBD program calling `n` instances of `Seq` (a, b), each output to ya, yb.
fn seq_caller(n: usize) -> Pou {
    let mut m = Pou::program("Main").fbd();
    let mut vars = vec![("x", "BOOL")];
    let names = ["a", "b"];
    let outs = ["ya", "yb"];
    for k in 0..n {
        vars.push((names[k], "Seq"));
        vars.push((outs[k], "BOOL"));
    }
    m = m.vars(&vars).invar(1, 0, 0, "x");
    for k in 0..n {
        let id = 10 + 10 * k as u64;
        m = m
            .block(
                id,
                100,
                100 * k as i32,
                "Seq",
                Some(names[k]),
                &[("t", tc6::In::From(&["1"]))],
                &["working"],
            )
            .outvar(
                id + 1,
                200,
                100 * k as i32,
                outs[k],
                &[&format!("{id}.working")],
            );
    }
    m
}

#[test]
fn a_chart_instance_added_by_an_online_change_times_its_initial_step_from_its_first_scan() {
    // §6.7.2: the initial step's elapsed time starts at t#0s. An instance added to a runtime
    // whose clock has run an hour must not take the hour as its initial step's time.
    let mut r = run(&[seq_fb(), seq_caller(1)]);
    r.set("x", Value::Bool(true));
    for k in 0..=360 {
        r.scan_at(k * 10_000);
    }
    let mut r = take_over(&r, &project(&[seq_fb(), seq_caller(2)]));
    r.scan_at(3_600_010);
    assert!(r.bool("b.Ready.X"));
    assert_eq!(r.int("b.Ready.T"), 0, "b.Ready.T at b's first scan");
    r.scan_at(3_600_020);
    assert_eq!(r.int("b.Ready.T"), 10 * support::MS);
    assert!(
        !r.bool("b.Busy.X") && !r.bool("yb"),
        "Busy 10 ms after b's first scan"
    );
    // Ten seconds after its first scan, Busy.
    r.scan_at(3_610_010);
    r.scan_at(3_610_020);
    assert!(r.bool("b.Busy.X") && r.bool("yb"));
}

#[test]
fn a_chart_instance_first_called_late_times_its_initial_step_from_its_first_call() {
    let main = Pou::program("Main")
        .sfc()
        .vars(&[("go", "BOOL"), ("x", "BOOL"), ("y", "BOOL"), ("a", "Seq")])
        .action("Call", "a(t := x); y := a.working;")
        .step(1, 100, 0, "Idle", true, &["4"])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "Run", false, &["2"])
        .trans(4, 100, 120, "FALSE", &["3"])
        .actions_of(5, 200, 80, "3", &[(6, "N", None, Act::Name("Call"))]);
    let mut r = run(&[seq_fb(), main]);
    r.set("x", Value::Bool(true));
    for k in 0..=360 {
        r.scan_at(k * 10_000);
    }
    r.set("go", Value::Bool(true));
    r.scan_at(3_600_010); // Run activated
    r.scan_at(3_600_020); // Call runs: a's first scan
    assert_eq!(r.int("a.Ready.T"), 0);
    r.scan_at(3_600_030);
    r.scan_at(3_600_040);
    assert!(
        !r.bool("a.Busy.X") && !r.bool("y"),
        "Busy 20 ms after a's first call"
    );
}

#[test]
fn a_step_that_loops_to_itself_is_not_entered_again_for_an_sd_delay_or_a_p0_pulse() {
    // As the test below, for SD and P0: the action control sees S's flag, which stays TRUE
    // when S's transition leads back to S, so the SD delay is not started again (it would be
    // if S were left for a scan and entered again) and P0 has no falling edge. Found in the
    // second review: the rule was written as "a step entered again", which a loop to itself
    // is not, for any qualifier. S is stored in scan 1 (10 ms) with T#60ms and loops in scan
    // 3: the action starts at 70 ms.
    for (qual, duration, expected) in [
        ("SD", Some("T#60ms"), "000000011111"),
        ("P0", None, "000000000000"),
    ] {
        let p = Pou::program("Main")
            .sfc()
            .vars(&[("go", "BOOL"), ("again", "BOOL"), ("act", "BOOL")])
            .step(1, 100, 0, "Idle", true, &[])
            .trans(2, 100, 40, "go", &["1"])
            .step(3, 100, 80, "S", false, &["2"])
            .trans(4, 100, 120, "again", &["3"])
            .jump_step(5, 100, 160, "S", &["4"])
            .actions_of(6, 200, 80, "3", &[(7, qual, duration, Act::Name("act"))]);
        let mut r = run(&[p]);
        let mut out = String::new();
        let mut times = Vec::new();
        for k in 0..12 {
            r.set("go", Value::Bool(k == 0));
            r.set("again", Value::Bool(k == 3));
            r.scan_at(k * 10);
            times.push(r.int("S.T") / support::MS);
            out.push(if r.bool("act") { '1' } else { '0' });
        }
        // S's own time does start again in scan 3.
        assert_eq!(times[..6], [0, 10, 20, 0, 10, 20], "{qual}");
        assert_eq!(out, expected, "{qual}");
    }
}

#[test]
fn a_step_that_loops_to_itself_restarts_its_time_and_keeps_its_actions_as_figure_23_draws() {
    // §6.7.5 and Figure 23 b): clearing S's transition deactivates and activates S in one
    // scan, so S.T starts again; the action control sees S's flag, which stays TRUE, so a P
    // action does not pulse again and an L action's limit counts from the first activation.
    let main = Pou::program("Main")
        .sfc()
        .vars(&[
            ("go", "BOOL"),
            ("again", "BOOL"),
            ("n", "INT"),
            ("lamp", "BOOL"),
        ])
        .action("Count", "n := n + 1;")
        .step(1, 100, 0, "Idle", true, &[])
        .trans(2, 100, 40, "go", &["1"])
        .step(3, 100, 80, "S", false, &["2"])
        .trans(4, 100, 120, "again", &["3"])
        .jump_step(5, 100, 160, "S", &["4"])
        .actions_of(
            6,
            200,
            80,
            "3",
            &[
                (7, "P", None, Act::Name("Count")),
                (8, "L", Some("T#25ms"), Act::Name("lamp")),
            ],
        );
    let mut r = run(&[main]);
    r.set("go", Value::Bool(true));
    r.scan_at(0);
    r.set("again", Value::Bool(true));
    let mut lamp = String::new();
    for k in 1..8 {
        r.scan_at(k * 10);
        assert!(r.bool("S.X"));
        assert!(
            r.int("S.T") <= 10 * support::MS,
            "S.T starts again: {}",
            r.int("S.T")
        );
        lamp.push(if r.bool("lamp") { '1' } else { '0' });
    }
    assert_eq!(r.int("n"), 1, "P pulsed once, not at each re-entry");
    assert!(
        lamp.starts_with('1') && lamp.ends_with('0'),
        "L ended after its limit: {lamp}"
    );
}
