//! Which task runs next (ADR-013 §2.19, IEC 61131-3 ed. 3 §6.8.2 tasks).
//!
//! One thread runs every task of a runtime: a program's run is a [`crate::Run`], continued in
//! pieces, and between two pieces the host asks [`next`] which task to continue or begin. This
//! module only decides. It runs no program and reads no clock: the host passes the time in, in
//! nanoseconds of a monotonic clock, and owns the tasks in a slice; nothing is allocated.
//!
//! The rules:
//! - Priority 0 is the highest, 31 the lowest. A [`Kind::Cycle`] task is below every priority.
//! - Every [`Kind::Startup`] task runs once, at the start, before any other task begins.
//! - The task to begin is the ready one of highest priority; among equals the one ready
//!   longest, then the one earlier in the slice.
//! - A task in progress is preempted only by a ready task of strictly higher priority. A
//!   preempted task is continued when no task of higher priority is in progress or ready.
//! - A [`Kind::Cyclic`] task is due at `start + phase + k * interval`, fixed times that do not
//!   drift with how long its runs take. Due again while its last run has not ended (or not
//!   begun), that run is not made and an overrun is counted, whenever the host asks [`next`];
//!   a due time at the very end of the run is not an overrun.

/// The lowest priority a task can have; 0 is the highest.
pub const LOWEST_PRIORITY: u8 = 31;

/// When a task becomes ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Ready again as soon as its run has ended, but not before `min_cycle` nanoseconds after
    /// its last run began; below every priority, whatever priority it is given.
    Cycle { min_cycle: i64 },
    /// Due every `interval` nanoseconds, the first time `phase` nanoseconds after the start.
    Cyclic { interval: i64, phase: i64 },
    /// Once, at the start, before any other task.
    Startup,
    /// Ready when the host reports its trigger with [`trigger`].
    Event,
}

/// What a task has done. A time is in nanoseconds; one not yet measured is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    /// Runs that ended.
    pub runs: u64,
    /// Runs of a cyclic task not made because it was due again before its last run ended.
    pub overruns: u64,
    /// Times a task of higher priority began while this one was the one in progress.
    pub preemptions: u64,
    /// From the beginning to the end of the last run, preemptions included.
    pub last_run: Option<i64>,
    /// The longest of those.
    pub longest_run: Option<i64>,
    /// When the last run ended.
    pub last_end: Option<i64>,
}

/// One task: its configuration and its state. The host keeps the tasks in a slice and gives
/// the same slice, in the same order, to every function of this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Task {
    kind: Kind,
    priority: u8,
    /// Ready since then, and not yet begun.
    ready: Option<i64>,
    /// Begun then, and not yet ended.
    began: Option<i64>,
    /// A cyclic task's next due time; a cycle task's earliest next beginning.
    due: i64,
    counters: Counters,
}

/// What the host does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Nothing is ready and nothing is in progress.
    Idle,
    /// Begin a run of this task (its index in the slice).
    Begin(usize),
    /// Continue the run of this task, which is in progress.
    Resume(usize),
}

impl Task {
    /// A task of `kind` at `priority`; `None` when the priority is above
    /// [`LOWEST_PRIORITY`], an interval is not above zero, or a phase or a minimum cycle time
    /// is below zero.
    pub fn new(kind: Kind, priority: u8) -> Option<Task> {
        let valid = match kind {
            Kind::Cycle { min_cycle } => min_cycle >= 0,
            Kind::Cyclic { interval, phase } => interval > 0 && phase >= 0,
            Kind::Startup | Kind::Event => true,
        };
        (valid && priority <= LOWEST_PRIORITY).then_some(Task {
            kind,
            priority,
            ready: None,
            began: None,
            due: 0,
            counters: Counters::default(),
        })
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn priority(&self) -> u8 {
        self.priority
    }
    pub fn counters(&self) -> Counters {
        self.counters
    }
    /// Ready, and not yet begun.
    pub fn is_ready(&self) -> bool {
        self.ready.is_some()
    }
    /// Begun and not ended: running, or preempted.
    pub fn in_progress(&self) -> bool {
        self.began.is_some()
    }
    /// Lower is more urgent; a cycle task is below every priority.
    fn rank(&self) -> u8 {
        match self.kind {
            Kind::Cycle { .. } => LOWEST_PRIORITY + 1,
            _ => self.priority,
        }
    }
}

/// The start of the runtime at `now`: the startup and cycle tasks are ready, the cyclic tasks
/// count their due times from here, the event tasks wait for their triggers.
pub fn start(tasks: &mut [Task], now: i64) {
    for t in tasks.iter_mut() {
        t.began = None;
        t.counters = Counters::default();
        t.ready = None;
        t.due = now;
        match t.kind {
            Kind::Startup | Kind::Cycle { .. } => t.ready = Some(now),
            Kind::Cyclic { phase, .. } => t.due = now.saturating_add(phase),
            Kind::Event => {}
        }
    }
}

/// The trigger of the event task at `task` happened at `now`: it is ready, unless it already
/// is. `false`, changing nothing, when there is no such task or it is not an event task.
pub fn trigger(tasks: &mut [Task], task: usize, now: i64) -> bool {
    match tasks.get_mut(task) {
        Some(t) if t.kind == Kind::Event => {
            if t.ready.is_none() {
                t.ready = Some(now);
            }
            true
        }
        _ => false,
    }
}

/// The run of the task at `task` ended at `now` (with its report or a fault). `false`,
/// changing nothing, when that task had no run in progress.
pub fn ended(tasks: &mut [Task], task: usize, now: i64) -> bool {
    let Some(t) = tasks.get_mut(task) else {
        return false;
    };
    let Some(began) = t.began.take() else {
        return false;
    };
    let took = now.saturating_sub(began);
    let c = &mut t.counters;
    c.runs = c.runs.saturating_add(1);
    c.last_run = Some(took);
    c.longest_run = Some(c.longest_run.map_or(took, |l| l.max(took)));
    c.last_end = Some(now);
    match t.kind {
        // Ready again at its end, or at its minimum cycle time if that is later: that is when it
        // became ready, so it does not pass a cycle task ready since before its end.
        Kind::Cycle { min_cycle } => t.due = began.saturating_add(min_cycle).max(now),
        // The due times that passed while it was in progress make no run, whether or not the
        // host asked [`next`] between them and the end.
        Kind::Cyclic { interval, .. } if now > t.due => {
            let passed = due_times(now.abs_diff(t.due) - 1, interval);
            skip(t, passed, passed, interval);
        }
        _ => {}
    }
    true
}

/// Which task to run at `now`: makes ready the cyclic and cycle tasks whose time has come
/// (counting overruns), then applies the rules of the module. A task it answers with
/// [`Next::Begin`] is from then in progress, until the host reports with [`ended`].
pub fn next(tasks: &mut [Task], now: i64) -> Next {
    release(tasks, now);
    // A startup task not yet ended holds back every other task.
    let starting = tasks
        .iter()
        .any(|t| t.kind == Kind::Startup && (t.ready.is_some() || t.began.is_some()));
    let current = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.began.is_some())
        .min_by_key(|(k, t)| (t.rank(), *k))
        .map(|(k, _)| k);
    let candidate = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| !starting || t.kind == Kind::Startup)
        .filter_map(|(k, t)| t.ready.map(|since| (t.rank(), since, k)))
        .min()
        .map(|(_, _, k)| k);
    match (current, candidate) {
        (Some(c), Some(k)) if tasks[k].rank() < tasks[c].rank() => {
            let p = &mut tasks[c].counters.preemptions;
            *p = p.saturating_add(1);
            begin(tasks, k, now)
        }
        (Some(c), _) => Next::Resume(c),
        (None, Some(k)) => begin(tasks, k, now),
        (None, None) => Next::Idle,
    }
}

/// The earliest time a cyclic task is due or a cycle task may begin again, for a host with
/// nothing ready to sleep until; `None` when no such time is set.
pub fn wake(tasks: &[Task]) -> Option<i64> {
    tasks
        .iter()
        .filter_map(|t| match t.kind {
            Kind::Cyclic { .. } => Some(t.due),
            Kind::Cycle { .. } if t.ready.is_none() && t.began.is_none() => Some(t.due),
            _ => None,
        })
        .min()
}

fn begin(tasks: &mut [Task], k: usize, now: i64) -> Next {
    let t = &mut tasks[k];
    t.ready = None;
    t.began = Some(now);
    Next::Begin(k)
}

fn release(tasks: &mut [Task], now: i64) {
    for t in tasks.iter_mut() {
        match t.kind {
            Kind::Cyclic { interval, .. } if now >= t.due => {
                // How many due times have passed: the first may make a run, the others not.
                let passed = due_times(now.abs_diff(t.due), interval);
                let missed = if t.ready.is_none() && t.began.is_none() {
                    t.ready = Some(t.due);
                    passed - 1
                } else {
                    passed
                };
                skip(t, passed, missed, interval);
            }
            Kind::Cycle { .. } if t.ready.is_none() && t.began.is_none() && now >= t.due => {
                t.ready = Some(t.due);
            }
            _ => {}
        }
    }
}

/// How many due times, `interval` apart, lie from a due time to `span` nanoseconds after it,
/// both included. In `u64`, which holds the distance between any two times: no overflow.
fn due_times(span: u64, interval: i64) -> u64 {
    // `interval` is above zero (`Task::new`).
    (span / interval.unsigned_abs()).saturating_add(1)
}

/// Moves a cyclic task's due time past `passed` due times, counting `missed` overruns; both
/// saturate at the end of time.
fn skip(t: &mut Task, passed: u64, missed: u64, interval: i64) {
    let o = &mut t.counters.overruns;
    *o = o.saturating_add(missed);
    t.due = t
        .due
        .saturating_add_unsigned(passed.saturating_mul(interval.unsigned_abs()));
}
