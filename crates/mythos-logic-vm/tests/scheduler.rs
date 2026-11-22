//! The scheduler's rules (ADR-013 §2.19), one test each. Times are nanoseconds; nothing here
//! runs a program: a test plays the host, beginning, continuing and ending runs by hand.

use mythos_logic_vm::sched::{ended, next, start, trigger, wake, Kind, Next, Task};

fn task(kind: Kind, priority: u8) -> Task {
    Task::new(kind, priority).expect("a valid task")
}
fn cyclic(interval: i64, phase: i64, priority: u8) -> Task {
    task(Kind::Cyclic { interval, phase }, priority)
}
fn event(priority: u8) -> Task {
    task(Kind::Event, priority)
}

#[test]
fn a_priority_above_31_a_period_not_above_zero_or_a_negative_phase_is_refused() {
    assert!(Task::new(Kind::Event, 31).is_some());
    assert!(Task::new(Kind::Event, 32).is_none());
    assert!(Task::new(Kind::Startup, 255).is_none());
    assert!(Task::new(
        Kind::Cyclic {
            interval: 0,
            phase: 0
        },
        0
    )
    .is_none());
    assert!(Task::new(
        Kind::Cyclic {
            interval: -5,
            phase: 0
        },
        0
    )
    .is_none());
    assert!(Task::new(
        Kind::Cyclic {
            interval: 5,
            phase: -1
        },
        0
    )
    .is_none());
    assert!(Task::new(Kind::Cycle { min_cycle: -1 }, 0).is_none());
    assert!(Task::new(Kind::Cycle { min_cycle: 0 }, 0).is_some());
}

#[test]
fn a_startup_task_runs_once_at_the_start_before_any_other_task() {
    // The event and the cyclic task are of higher priority, and ready: they still wait.
    let mut t = [event(0), cyclic(10, 0, 0), task(Kind::Startup, 20)];
    start(&mut t, 0);
    assert!(trigger(&mut t, 0, 0));
    assert_eq!(next(&mut t, 0), Next::Begin(2));
    assert_eq!(
        next(&mut t, 5),
        Next::Resume(2),
        "not preempted, whatever the priority"
    );
    assert!(ended(&mut t, 2, 7));
    // The event was ready first (0) and the cyclic task at 0 too: equal times, earlier index.
    assert_eq!(next(&mut t, 7), Next::Begin(0));
    ended(&mut t, 0, 8);
    assert_eq!(next(&mut t, 8), Next::Begin(1));
    ended(&mut t, 1, 9);
    // Never again.
    assert_eq!(next(&mut t, 9), Next::Idle);
    assert_eq!(t[2].counters().runs, 1);
}

#[test]
fn the_ready_task_of_highest_priority_begins_and_0_is_the_highest() {
    let mut t = [event(5), event(0), event(31)];
    start(&mut t, 0);
    for k in [2, 0, 1] {
        trigger(&mut t, k, 0);
    }
    assert_eq!(next(&mut t, 0), Next::Begin(1));
    ended(&mut t, 1, 1);
    assert_eq!(next(&mut t, 1), Next::Begin(0));
    ended(&mut t, 0, 2);
    assert_eq!(next(&mut t, 2), Next::Begin(2));
}

#[test]
fn among_equal_priorities_the_task_ready_longest_begins_first() {
    let mut t = [event(3), event(3)];
    start(&mut t, 0);
    trigger(&mut t, 1, 10);
    trigger(&mut t, 0, 20);
    assert_eq!(next(&mut t, 30), Next::Begin(1));
}

#[test]
fn among_equal_priorities_ready_as_long_the_task_earlier_in_the_list_begins_first() {
    let mut t = [event(3), event(3)];
    start(&mut t, 0);
    trigger(&mut t, 1, 10);
    trigger(&mut t, 0, 10);
    assert_eq!(next(&mut t, 30), Next::Begin(0));
}

#[test]
fn a_task_in_progress_is_preempted_only_by_a_ready_task_of_strictly_higher_priority() {
    let mut t = [event(4), event(4), event(5), event(3)];
    start(&mut t, 0);
    trigger(&mut t, 0, 0);
    assert_eq!(next(&mut t, 0), Next::Begin(0));
    trigger(&mut t, 1, 1); // equal
    trigger(&mut t, 2, 1); // lower
    assert_eq!(next(&mut t, 2), Next::Resume(0));
    trigger(&mut t, 3, 3); // higher
    assert_eq!(next(&mut t, 4), Next::Begin(3));
    assert_eq!(t[0].counters().preemptions, 1);
    assert!(t[0].in_progress(), "preempted, not ended");
}

#[test]
fn a_preempted_task_is_continued_when_no_task_of_higher_priority_is_in_progress_or_ready() {
    let mut t = [event(4), event(1), event(4)];
    start(&mut t, 0);
    trigger(&mut t, 0, 0);
    assert_eq!(next(&mut t, 0), Next::Begin(0));
    trigger(&mut t, 1, 1);
    trigger(&mut t, 2, 1);
    assert_eq!(next(&mut t, 1), Next::Begin(1));
    assert_eq!(next(&mut t, 2), Next::Resume(1));
    ended(&mut t, 1, 3);
    // The equal-priority task ready since 1 does not take the place of the preempted one.
    assert_eq!(next(&mut t, 3), Next::Resume(0));
    ended(&mut t, 0, 4);
    assert_eq!(next(&mut t, 4), Next::Begin(2));
}

#[test]
fn a_cyclic_task_is_due_at_fixed_times_from_the_start_that_do_not_drift() {
    // Interval 100, phase 30, started at 1000: due at 1030, 1130, 1230, ... however long the
    // runs take and however late they begin.
    let mut t = [cyclic(100, 30, 2)];
    start(&mut t, 1000);
    assert_eq!(next(&mut t, 1029), Next::Idle);
    assert_eq!(wake(&t), Some(1030));
    assert_eq!(next(&mut t, 1030), Next::Begin(0));
    ended(&mut t, 0, 1090);
    assert_eq!(wake(&t), Some(1130));
    assert_eq!(next(&mut t, 1129), Next::Idle);
    // Begun late, at 1171: the next is still due at 1230.
    assert_eq!(next(&mut t, 1171), Next::Begin(0));
    ended(&mut t, 0, 1200);
    assert_eq!(wake(&t), Some(1230));
    assert_eq!(next(&mut t, 1230), Next::Begin(0));
    assert_eq!(t[0].counters().overruns, 0);
}

#[test]
fn a_cyclic_task_due_again_before_its_last_run_ended_makes_no_run_and_counts_an_overrun() {
    let mut t = [cyclic(100, 0, 2)];
    start(&mut t, 0);
    assert_eq!(next(&mut t, 0), Next::Begin(0));
    // Due at 100 and at 200 while the run begun at 0 goes on.
    assert_eq!(next(&mut t, 100), Next::Resume(0));
    assert_eq!(next(&mut t, 250), Next::Resume(0));
    assert_eq!(t[0].counters().overruns, 2);
    ended(&mut t, 0, 260);
    // No run is owed for 100 or 200: the next is at 300.
    assert_eq!(next(&mut t, 260), Next::Idle);
    assert_eq!(next(&mut t, 300), Next::Begin(0));

    // Due again while its run is ready but held back by a task of higher priority: the same.
    let mut t = [cyclic(100, 0, 2), event(0)];
    start(&mut t, 0);
    trigger(&mut t, 1, 0);
    assert_eq!(next(&mut t, 0), Next::Begin(1));
    assert_eq!(next(&mut t, 150), Next::Resume(1));
    assert_eq!(t[0].counters().overruns, 1);
    ended(&mut t, 1, 160);
    assert_eq!(next(&mut t, 160), Next::Begin(0));
}

#[test]
fn a_cycle_task_is_below_every_priority_and_begins_again_once_ended_no_sooner_than_its_minimum_cycle_time(
) {
    // Given priority 0, it still yields to an event task of priority 31.
    let mut t = [task(Kind::Cycle { min_cycle: 100 }, 0), event(31)];
    start(&mut t, 0);
    assert_eq!(next(&mut t, 0), Next::Begin(0));
    trigger(&mut t, 1, 10);
    assert_eq!(next(&mut t, 10), Next::Begin(1));
    assert_eq!(t[0].counters().preemptions, 1);
    ended(&mut t, 1, 20);
    assert_eq!(next(&mut t, 20), Next::Resume(0));
    ended(&mut t, 0, 40);
    // Begun at 0, minimum cycle 100: not again before 100.
    assert_eq!(next(&mut t, 99), Next::Idle);
    assert_eq!(wake(&t), Some(100));
    assert_eq!(next(&mut t, 100), Next::Begin(0));
    // A run longer than the minimum: it begins again as soon as it has ended.
    ended(&mut t, 0, 350);
    assert_eq!(next(&mut t, 350), Next::Begin(0));
}

#[test]
fn an_event_task_is_ready_when_the_host_reports_its_trigger_and_only_then() {
    let mut t = [event(7), cyclic(1000, 0, 7)];
    start(&mut t, 0);
    assert_eq!(next(&mut t, 0), Next::Begin(1));
    ended(&mut t, 1, 1);
    assert_eq!(next(&mut t, 500), Next::Idle);
    assert!(!trigger(&mut t, 1, 500), "a cyclic task has no trigger");
    assert!(!trigger(&mut t, 9, 500), "no such task");
    assert!(trigger(&mut t, 0, 500));
    assert!(t[0].is_ready());
    assert_eq!(next(&mut t, 500), Next::Begin(0));
    ended(&mut t, 0, 510);
    assert_eq!(next(&mut t, 520), Next::Idle);
}

#[test]
fn each_task_counts_its_runs_preemptions_and_last_and_longest_run_time() {
    let mut t = [event(5), event(1)];
    start(&mut t, 0);
    assert_eq!(t[0].counters().last_run, None, "not measured before a run");
    assert_eq!(t[0].counters().last_end, None);
    trigger(&mut t, 0, 0);
    assert_eq!(next(&mut t, 0), Next::Begin(0));
    trigger(&mut t, 1, 10);
    assert_eq!(next(&mut t, 10), Next::Begin(1));
    ended(&mut t, 1, 15);
    assert_eq!(next(&mut t, 15), Next::Resume(0));
    ended(&mut t, 0, 40);
    trigger(&mut t, 0, 50);
    assert_eq!(next(&mut t, 50), Next::Begin(0));
    ended(&mut t, 0, 60);
    let c = t[0].counters();
    assert_eq!(c.runs, 2);
    assert_eq!(c.preemptions, 1);
    assert_eq!(c.last_run, Some(10));
    assert_eq!(
        c.longest_run,
        Some(40),
        "from its beginning to its end, preemption included"
    );
    assert_eq!(c.last_end, Some(60));
    assert_eq!(t[1].counters().runs, 1);
    assert_eq!(t[1].counters().last_run, Some(5));
    assert!(!ended(&mut t, 0, 70), "no run in progress to end");
    assert_eq!(t[0].counters().runs, 2);
}
