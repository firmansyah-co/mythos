//! Several programs in several tasks in one runtime (ADR-013 §2.19, C8b), driven through the
//! engine directly: the scheduler, the process images, the state of each task, and that a
//! runtime of today reports exactly what it reported before there were tasks.

use mythos_logic_compiler::{compile, Compiled, TargetProfile};
use mythos_logic_vm::{cold_start, scan, Frame, Program};
use mythos_module::protocol::{
    ExchangeReply, RuntimeMode, RuntimeState, TaskState, WireInput, WireValue,
};
use mythos_module::RuntimeEngine;
use mythos_plugin_sdk::{MythosPlugin, PluginContext, ProtocolDriver};
use mythos_runtime_vm::VmEngine;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// These tests measure what a scheduler does in time: one at a time, so that they do not
/// share the machine's cores with each other.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    static ONE: Mutex<()> = Mutex::new(());
    ONE.lock().unwrap_or_else(|e| e.into_inner())
}

/// How the machine itself keeps time while a test runs: a thread that does nothing but sleep
/// `wait` over and over, and the latest it came back from one of those sleeps. A machine may
/// return a thread late by more the longer it asked to sleep (GitHub's macOS runner returned a
/// sleep of 10 ms up to 80 ms late while one of 1 ms was at most 20 ms late), so `wait` is the
/// wait of what is being judged: for the scheduler, how long it waits for a due time.
struct Timekeeper {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<(Duration, u32)>,
}

impl Timekeeper {
    fn start(wait: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let (mut latest, mut sleeps) = (Duration::ZERO, 0u32);
            while !seen.load(Ordering::SeqCst) {
                let before = Instant::now();
                std::thread::sleep(wait);
                latest = latest.max(before.elapsed().saturating_sub(wait));
                sleeps += 1;
            }
            (latest, sleeps)
        });
        Timekeeper { stop, thread }
    }

    /// The latest the thread came back from a sleep, and how many sleeps it made.
    fn latest(self) -> (Duration, u32) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.join().expect("the timekeeper does not panic")
    }
}

/// Drive a future of the engine to its end: its calls never wait on anything.
fn finish<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

fn ctx() -> PluginContext {
    PluginContext::new("io.runtime:t", HashSet::new(), String::new())
}

fn compiled(source: &str) -> Compiled {
    compile(source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"))
}

/// A runtime program of `source`, with its bindings (`inputs`, `outputs`, `published`).
fn program(source: &str, interval_ms: u64, bindings: Value) -> Value {
    let c = compiled(source);
    let mut p = json!({
        "name": c.name,
        "version": format!("{}-1", c.name),
        "interval_ms": interval_ms,
        "code": hex::encode(&c.bytecode),
        "symbols": c.symbols,
    });
    if let Value::Object(b) = bindings {
        p.as_object_mut().unwrap().extend(b);
    }
    p
}

fn cyclic(name: &str, interval_ms: u64, priority: u8, programs: &[&str]) -> Value {
    json!({"name": name, "kind": {"type": "cyclic", "interval_ms": interval_ms},
           "priority": priority, "programs": programs})
}

fn start(config: Value) -> VmEngine {
    let mut engine = VmEngine::default();
    finish(engine.start(&ctx(), &config)).unwrap_or_else(|e| panic!("{e}"));
    engine
}

fn exchange(engine: &mut VmEngine, inputs: Vec<WireInput>) -> ExchangeReply {
    finish(engine.exchange(&ctx(), inputs)).unwrap()
}

fn input(tag: &str, value: Option<Value>, good: bool) -> WireInput {
    WireInput {
        tag: tag.into(),
        value,
        good,
    }
}

fn task<'a>(state: &'a RuntimeState, name: &str) -> &'a TaskState {
    state
        .tasks
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no task {name}: {state:?}"))
}

/// Exchange (with no inputs) every few ms until `done`, or panic after ten seconds.
fn until(
    engine: &mut VmEngine,
    what: &str,
    done: impl Fn(&ExchangeReply) -> bool,
) -> ExchangeReply {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let reply = exchange(engine, Vec::new());
        if done(&reply) {
            return reply;
        }
        assert!(
            Instant::now() < deadline,
            "never: {what}; last {:?}",
            reply.state
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Exchange for `long`, every `every`, giving each exchange the inputs `feed` makes from its
/// count, and handing each reply to `check`.
fn exchange_for(
    engine: &mut VmEngine,
    long: Duration,
    every: Duration,
    feed: impl Fn(u64) -> Vec<WireInput>,
    mut check: impl FnMut(&ExchangeReply),
) -> ExchangeReply {
    let end = Instant::now() + long;
    let mut n = 0;
    loop {
        let reply = exchange(engine, feed(n));
        check(&reply);
        if Instant::now() >= end {
            return reply;
        }
        n += 1;
        std::thread::sleep(every);
    }
}

fn watch(engine: &mut VmEngine, names: &[&str]) -> Vec<(Option<Value>, Option<String>, bool)> {
    finish(engine.watch(&ctx(), names.iter().map(|s| s.to_string()).collect()))
        .unwrap()
        .values
        .into_iter()
        .map(|v| (v.value, v.error, v.forced))
        .collect()
}

fn watch_one(engine: &mut VmEngine, name: &str) -> Value {
    let (value, error, _) = watch(engine, &[name]).remove(0);
    value.unwrap_or_else(|| panic!("{name}: {error:?}"))
}

/// Iterations of the loop of the long programs: a run of a few million instructions, inside
/// the budget of 5,000,000 (`the_long_programs_fit_their_budget` says how many).
const LOOP: u32 = 100_000;

/// A run that takes long: tens of pieces.
fn slow_source(name: &str) -> String {
    format!(
        "
PROGRAM {name}
VAR i : DINT; n : DINT; acc : DINT; END_VAR
  n := n + 1;
  FOR i := 1 TO {LOOP} DO
    acc := acc + (i MOD 7) - n;
  END_FOR;
END_PROGRAM
"
    )
}

/// A run of a few instructions.
fn quick_source(name: &str) -> String {
    format!(
        "
PROGRAM {name}
VAR n : DINT; x : INT; END_VAR
  n := n + 1;
END_PROGRAM
"
    )
}

/// One run of the slow program, alone: its instructions and how long it took.
fn slow_run() -> (u32, Duration) {
    let c = compiled(&slow_source("slow"));
    let program = Program::load(&c.bytecode).unwrap();
    let mut memory = vec![0u8; program.memory_size()];
    cold_start(&program, &mut memory).unwrap();
    let mut stack = vec![0u64; program.stack_depth().max(1)];
    let mut calls = vec![Frame::default(); program.call_depth()];
    let began = Instant::now();
    let report = scan(&program, &mut memory, &mut stack, &mut calls, 0, 5_000_000).unwrap();
    (report.instructions, began.elapsed())
}

fn slow_run_alone() -> Duration {
    slow_run().1
}

#[test]
fn the_long_programs_fit_their_budget() {
    let _one = one_at_a_time();
    let (instructions, took) = slow_run();
    // Printed, not asserted: how long it takes depends on the build and the machine.
    eprintln!("a run of the slow program: {instructions} instructions in {took:?}");
    assert!(
        instructions > 1_000_000 && instructions < 5_000_000,
        "{instructions}"
    );
}

#[test]
fn a_configuration_of_today_reports_the_state_of_today() {
    let _one = one_at_a_time();
    let today = |state: &RuntimeState| {
        let mut keys: Vec<String> = serde_json::to_value(state)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    let mut engine = start(
        json!({"run": true, "program": program(&quick_source("Counter"), 10,
        json!({"published": [{"variable": "n", "tag": "internal.T.n"}]}))}),
    );
    let first = exchange(&mut engine, Vec::new());
    assert_eq!(
        today(&first.state),
        [
            "fault",
            "forced",
            "last_scan_us",
            "max_scan_us",
            "mode",
            "overruns",
            "program",
            "scans",
            "version"
        ],
        "before its first scan: no age"
    );
    assert_eq!(
        first.samples[0].fault,
        Some(mythos_module::protocol::WireFault::Device(
            "the program has not completed a scan yet".into()
        ))
    );
    let scanned = until(&mut engine, "it scans", |r| r.state.scans > 0);
    assert_eq!(
        today(&scanned.state),
        [
            "fault",
            "forced",
            "last_scan_us",
            "max_scan_us",
            "mode",
            "overruns",
            "program",
            "scan_age_us",
            "scans",
            "version"
        ]
    );
    assert_eq!(
        (
            scanned.state.program.as_str(),
            scanned.state.version.as_str()
        ),
        ("Counter", "Counter-1")
    );
    assert!(matches!(scanned.samples[0].value, Some(WireValue::I32(n)) if n > 0));

    // The same program in one task written out says its task, and its figures are the task's.
    let mut written = start(json!({"run": true,
        "programs": [program(&quick_source("Counter"), 10, json!({}))],
        "tasks": [cyclic("main", 10, 0, &["Counter"])]}));
    let r = until(&mut written, "it runs", |r| r.state.scans > 0);
    let main = task(&r.state, "main");
    assert_eq!(r.state.program, "Counter");
    assert!(main.runs >= r.state.scans && main.runs > 0);
    assert_eq!(
        main.watchdog_ms, 2_000,
        "ten intervals, never less than 2 s"
    );
    assert!(r.state.scan_age_us.is_some(), "one task: its age");
    assert_eq!(r.state.programs.len(), 1);
}

#[test]
fn a_higher_priority_task_preempts_a_long_lower_one() {
    let _one = one_at_a_time();
    // The high interval: a fifth of how long a low run takes alone on this machine and build,
    // so a low run outlasts it; never below 10 ms, so that neither a piece nor the machine's
    // other work makes the high task miss a due time.
    let alone = slow_run_alone();
    let high_ms = (alone.as_millis() as u64 / 5).clamp(10, 100);
    let mut engine = start(json!({"run": true,
        "programs": [
            program(&slow_source("slow"), 10, json!({})),
            program(&quick_source("quick"), 10, json!({})),
        ],
        "tasks": [cyclic("low", 1000, 10, &["slow"]), cyclic("high", high_ms, 1, &["quick"])]}));
    // With nothing to run the scheduler waits for the next due time, 20 ms at most.
    let wait = Duration::from_millis(high_ms.min(20));
    let keeper = Timekeeper::start(wait);
    // What each exchange saw, for the message of a failure: when, then the runs, preemptions
    // and age of the last run of the low task, then the runs, overruns and age of the high.
    let began = Instant::now();
    let mut trace = String::new();
    let r = loop {
        use std::fmt::Write;
        let asked = began.elapsed();
        let r = exchange(&mut engine, Vec::new());
        let answered = began.elapsed();
        let (l, h) = (task(&r.state, "low"), task(&r.state, "high"));
        let _ = writeln!(
            trace,
            "{:>7.1} ms (+{:>5.2})  low {} runs {} preempted age {:?} us  high {} runs {} overruns age {:?} us",
            asked.as_secs_f64() * 1e3,
            (answered - asked).as_secs_f64() * 1e3,
            l.runs,
            l.preemptions,
            l.run_age_us,
            h.runs,
            h.overruns,
            h.run_age_us
        );
        if answered >= Duration::from_millis(2_000) {
            break r;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (late, sleeps) = keeper.latest();
    let (low, high) = (task(&r.state, "low"), task(&r.state, "high"));
    assert_eq!(r.state.mode, RuntimeMode::Run, "{:?}", r.state.fault);
    assert!(low.runs > 0, "{low:?}");
    assert!(
        low.longest_run_us.is_some_and(|us| us > high_ms * 1_000),
        "a low run outlasts the high interval of {high_ms} ms (alone: {alone:?}): {low:?}"
    );
    assert!(low.preemptions > 0, "{low:?}");
    // The high task misses a due time when the scheduler comes back from a wait one of its
    // intervals late. That it never does is claimed only where a thread that merely sleeps as
    // long as the scheduler waits came back less than half an interval late; elsewhere the
    // figures are written and nothing is claimed.
    if late < Duration::from_millis(high_ms / 2) {
        assert_eq!(
            high.overruns, 0,
            "a thread sleeping {wait:?} was at most {late:?} late in {sleeps} sleeps, a low run \
             alone took {alone:?}: {high:?}\n{low:?}\n{trace}"
        );
    } else {
        // Written past the capture of a passing test's output: a claim not made is to be seen
        // in the log of the run that did not make it.
        let said = format!(
            "NOT SHOWN on this machine: that the high task misses no due time. A thread \
             sleeping {wait:?} came back up to {late:?} late in {sleeps} sleeps, the high \
             interval is {high_ms} ms, and the high task counted {} overruns in {} runs.\n",
            high.overruns, high.runs
        );
        let _ = std::io::Write::write_all(&mut std::io::stderr(), said.as_bytes());
    }
    assert!(high.runs > 5, "{high:?}");
    assert_eq!(high.preemptions, 0, "nothing is above it: {high:?}");
    // Several tasks: sums, and no one age.
    assert_eq!(r.state.scans, low.runs + high.runs);
    assert_eq!(r.state.scan_age_us, None);
    assert_eq!(r.state.program, "slow, quick");
}

#[test]
fn a_preempted_program_computes_what_it_computes_alone() {
    let _one = one_at_a_time();
    let sources = [slow_source("slow"), quick_source("quick")];
    let mut engine = start(json!({"run": true,
        "programs": [program(&sources[0], 10, json!({})), program(&sources[1], 10, json!({}))],
        "tasks": [cyclic("low", 50, 10, &["slow"]), cyclic("high", 5, 1, &["quick"])]}));
    let r = exchange_for(
        &mut engine,
        Duration::from_millis(2_000),
        Duration::from_millis(10),
        |_| Vec::new(),
        |_| {},
    );
    assert!(task(&r.state, "low").preemptions > 0, "{:?}", r.state);
    let now = finish(engine.variables(&ctx())).unwrap();
    for (name, source) in [("slow", &sources[0]), ("quick", &sources[1])] {
        let c = compiled(source);
        let n = now
            .values
            .iter()
            .find(|v| v.path == format!("{name}.n"))
            .map(|v| {
                let b = hex::decode(&v.bytes).unwrap();
                i32::from_le_bytes([b[0], b[1], b[2], b[3]])
            })
            .unwrap();
        assert!(n > 1, "{name} ran {n} times");
        // The same program, run alone n times from its initial values, with no clock.
        let program = Program::load(&c.bytecode).unwrap();
        let mut memory = vec![0u8; program.memory_size()];
        cold_start(&program, &mut memory).unwrap();
        let mut stack = vec![0u64; program.stack_depth().max(1)];
        let mut calls = vec![Frame::default(); program.call_depth()];
        for _ in 0..n {
            scan(&program, &mut memory, &mut stack, &mut calls, 0, 5_000_000).unwrap();
        }
        let mut compared = 0;
        for s in &c.symbols {
            let path = format!("{name}.{}", s.path);
            let given = now.values.iter().find(|v| v.path == path).unwrap();
            let alone = hex::encode(&memory[s.offset as usize..(s.offset + s.size) as usize]);
            assert_eq!(given.bytes, alone, "{path} after {n} runs");
            compared += 1;
        }
        assert!(compared >= 2, "{name}: {compared}");
    }
}

/// `out` is -1 from the start of a run to its end, `k` after.
const MARKER: &str = "
PROGRAM marker
VAR i : DINT; k : DINT; out : DINT; END_VAR
  out := -1;
  FOR i := 1 TO 30000 DO
    k := k + (i MOD 2) - (i MOD 2);
  END_FOR;
  k := k + 1;
  out := k;
END_PROGRAM
";

#[test]
fn an_exchange_during_a_long_run_reads_the_last_completed_run() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true,
        "programs": [program(MARKER, 10, json!({
            "published": [{"variable": "out", "tag": "internal.T.out"}],
            "outputs": [{"variable": "out", "tag": "external.A.B.out", "safe": 0}]}))],
        "tasks": [{"name": "free", "kind": {"type": "cycle"}, "priority": 31,
                   "programs": ["marker"]}]}));
    let mut seen = Vec::new();
    exchange_for(
        &mut engine,
        Duration::from_millis(1_500),
        Duration::from_millis(1),
        |_| Vec::new(),
        |r| {
            if let Some(WireValue::I32(v)) = r.samples[0].value {
                assert_ne!(v, -1, "a sample half-way through a run");
                assert_eq!(
                    r.outputs[0].value,
                    json!(v),
                    "the output, from the same image"
                );
                if seen.last() != Some(&v) {
                    seen.push(v);
                }
            } else {
                assert!(r.outputs.is_empty(), "no output before a run completed");
            }
        },
    );
    assert!(seen.len() >= 3, "it ran on while exchanged with: {seen:?}");
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
}

const LATCH: &str = "
PROGRAM latch
VAR inp : DINT; first : DINT; last : DINT; bad : DINT; runs : DINT; i : DINT; END_VAR
  first := inp;
  FOR i := 1 TO 30000 DO
    runs := runs + (i MOD 2) - (i MOD 2);
  END_FOR;
  last := inp;
  IF first <> last THEN
    bad := bad + 1;
  END_IF;
  runs := runs + 1;
END_PROGRAM
";

#[test]
fn a_tasks_inputs_are_fixed_for_its_run() {
    let _one = one_at_a_time();
    const TAG: &str = "external.A.B.x";
    let mut engine = start(json!({"run": true,
        "programs": [program(LATCH, 10, json!({"inputs": [{"variable": "inp", "tag": TAG}]}))],
        "tasks": [{"name": "free", "kind": {"type": "cycle"}, "priority": 31,
                   "programs": ["latch"]}]}));
    // The tag changes at every exchange, every millisecond: many times in each run.
    let r = exchange_for(
        &mut engine,
        Duration::from_millis(1_500),
        Duration::from_millis(1),
        |n| vec![input(TAG, Some(json!(n)), true)],
        |_| {},
    );
    let runs = watch_one(&mut engine, "runs").as_i64().unwrap();
    assert!(runs >= 3, "{runs} runs; {:?}", r.state);
    let longest = task(&r.state, "free").longest_run_us.unwrap();
    assert!(
        longest > 1_000,
        "a run spans several exchanges: {longest} µs"
    );
    assert_eq!(watch_one(&mut engine, "bad"), json!(0), "after {runs} runs");
    let first = watch_one(&mut engine, "first").as_i64().unwrap();
    assert!(
        first > runs,
        "the input it last took, {first}, is a count of exchanges"
    );
}

#[test]
fn each_task_counts_its_own_overruns() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true,
        "programs": [
            program(&slow_source("slow"), 10, json!({})),
            program(&quick_source("quick"), 10, json!({})),
        ],
        "tasks": [cyclic("tight", 2, 10, &["slow"]), cyclic("roomy", 300, 1, &["quick"])]}));
    let r = exchange_for(
        &mut engine,
        Duration::from_millis(1_500),
        Duration::from_millis(10),
        |_| Vec::new(),
        |_| {},
    );
    let (tight, roomy) = (task(&r.state, "tight"), task(&r.state, "roomy"));
    assert!(
        tight.longest_run_us.is_some_and(|us| us > 2_000),
        "its run outlasts its interval: {tight:?}"
    );
    assert!(tight.overruns > 0, "{tight:?}");
    assert_eq!(roomy.overruns, 0, "{roomy:?}");
    assert!(roomy.runs >= 2, "{roomy:?}");
    assert_eq!(r.state.overruns, tight.overruns, "the runtime's: their sum");
}

const FAULTY: &str = "
PROGRAM faulty
VAR n : DINT; z : DINT; x : DINT; END_VAR
  n := n + 1;
  IF n >= 5 THEN
    x := 1 / z;
  END_IF;
END_PROGRAM
";

#[test]
fn a_fault_in_one_program_stops_every_task() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true,
        "programs": [
            program(&quick_source("quick"), 10, json!({
                "outputs": [{"variable": "n", "tag": "external.A.B.n", "safe": 0}],
                "published": [{"variable": "n", "tag": "internal.T.n"}]})),
            program(FAULTY, 10, json!({})),
        ],
        "tasks": [cyclic("a", 5, 2, &["quick"]), cyclic("b", 10, 3, &["faulty"])]}));
    let r = until(&mut engine, "it faults", |r| {
        r.state.mode == RuntimeMode::Fault
    });
    let fault = r.state.fault.clone().unwrap();
    assert!(
        fault.starts_with("task 'b' program 'faulty' run 5 stopped: division by zero"),
        "{fault}"
    );
    assert!(
        r.outputs.is_empty(),
        "no output: the core writes the safe values"
    );
    assert!(
        matches!(&r.samples[0].fault,
            Some(mythos_module::protocol::WireFault::Device(m)) if m.starts_with("the program faulted: task 'b'")),
        "{:?}",
        r.samples
    );
    let runs = task(&r.state, "a").runs;
    std::thread::sleep(Duration::from_millis(100));
    let later = exchange(&mut engine, Vec::new());
    assert_eq!(task(&later.state, "a").runs, runs, "task a ran no more");
    assert_eq!(later.state.mode, RuntimeMode::Fault);
}

/// A RETAIN count of the times it ran; its run is long.
const BOOT: &str = "
PROGRAM boot
VAR RETAIN boots : DINT; END_VAR
VAR i : DINT; END_VAR
  boots := boots + 1;
  FOR i := 1 TO 100000 DO
    boots := boots + (i MOD 2) - (i MOD 2);
  END_FOR;
END_PROGRAM
";

fn startup_runtime(extra: Value) -> Value {
    let mut config = json!({"run": true,
    "programs": [program(BOOT, 10, json!({})), program(&quick_source("quick"), 10, json!({}))],
    "tasks": [
        {"name": "init", "kind": {"type": "startup"}, "priority": 31, "programs": ["boot"]},
        cyclic("fast", 2, 0, &["quick"]),
    ]});
    if let Value::Object(e) = extra {
        config.as_object_mut().unwrap().extend(e);
    }
    config
}

#[test]
fn a_startup_task_runs_before_any_other_at_warm_and_cold_start() {
    let _one = one_at_a_time();
    let mut engine = start(startup_runtime(json!({})));
    let started = |engine: &mut VmEngine| {
        let r = until(
            engine,
            "the startup task ran and the cyclic one after it",
            |r| task(&r.state, "init").runs == 1 && task(&r.state, "fast").runs > 0,
        );
        let (init, fast) = (task(&r.state, "init"), task(&r.state, "fast"));
        // The cyclic task is of the highest priority and was due every 2 ms through the whole
        // startup run: it began none of its runs during it.
        assert!(init.longest_run_us.is_some_and(|us| us > 2_000), "{init:?}");
        assert_eq!(init.preemptions, 0, "{init:?} {fast:?}");
        r
    };
    started(&mut engine);
    assert_eq!(watch_one(&mut engine, "boot.boots"), json!(1));

    for (cold, boots) in [(false, 2), (true, 1), (false, 2)] {
        finish(engine.set_mode(&ctx(), false, false)).unwrap();
        let state = finish(engine.set_mode(&ctx(), true, cold)).unwrap();
        assert_eq!(
            task(&state, "init").runs,
            0,
            "counted from the change to RUN"
        );
        started(&mut engine);
        assert_eq!(
            watch_one(&mut engine, "boot.boots"),
            json!(boots),
            "cold: {cold}"
        );
    }
}

#[test]
fn an_online_change_runs_no_startup_task() {
    let _one = one_at_a_time();
    let mut engine = start(startup_runtime(json!({
    "carried": {"clock_ns": 0, "values": [
        {"path": "boot.boots", "type": "DINT", "bytes": "05000000"},
        {"path": "gone.x", "type": "DINT", "bytes": "05000000"},
    ]}})));
    let r = until(&mut engine, "the cyclic task runs", |r| {
        task(&r.state, "fast").runs > 20
    });
    assert_eq!(task(&r.state, "init").runs, 0, "{:?}", r.state);
    assert_eq!(watch_one(&mut engine, "boot.boots"), json!(5), "carried");
    let report = finish(engine.carried(&ctx())).unwrap();
    assert_eq!(report.carried, 1);
    assert_eq!(report.dropped[0].variable, "gone.x");
    assert_eq!(
        report.dropped[0].why,
        "the runtime has no program gone; it runs boot, quick"
    );

    // The next change to RUN is a restart, and runs it.
    finish(engine.set_mode(&ctx(), false, false)).unwrap();
    finish(engine.set_mode(&ctx(), true, false)).unwrap();
    until(&mut engine, "the startup task runs", |r| {
        task(&r.state, "init").runs == 1
    });
    assert_eq!(watch_one(&mut engine, "boot.boots"), json!(6));
}

#[test]
fn an_event_task_runs_once_per_rising_edge() {
    let _one = one_at_a_time();
    const TAG: &str = "external.A.B.go";
    const EV: &str = "
PROGRAM ev
VAR trig : BOOL; n : DINT; END_VAR
  n := n + 1;
END_PROGRAM
";
    let mut engine = start(json!({"run": true,
        "programs": [program(EV, 10, json!({"inputs": [{"variable": "trig", "tag": TAG}]}))],
        "tasks": [{"name": "edge", "kind": {"type": "event", "tag": TAG}, "priority": 1,
                   "programs": ["ev"]}]}));
    let level = |b: bool| vec![input(TAG, Some(json!(b)), true)];
    let not_good = vec![input(TAG, Some(json!(true)), false)];
    let mut runs = Vec::new();
    // T at the first exchange is no edge; F,T is; T,T is not; not GOOD then T is not.
    for inputs in [
        level(true),
        level(true),
        level(false),
        level(true),
        level(true),
        level(false),
        level(true),
        not_good,
        level(true),
        vec![],
        level(true),
    ] {
        exchange(&mut engine, inputs.clone());
        std::thread::sleep(Duration::from_millis(60));
        // Read with the same inputs: an exchange that leaves the tag out would itself end any
        // edge across it.
        runs.push(task(&exchange(&mut engine, inputs).state, "edge").runs);
    }
    assert_eq!(runs, [0, 0, 0, 1, 1, 1, 2, 2, 2, 2, 2]);
    assert_eq!(watch_one(&mut engine, "n"), json!(2));
    let r = exchange(&mut engine, Vec::new());
    let edge = task(&r.state, "edge");
    assert_eq!((edge.overruns, edge.watchdog_ms), (0, 2_000));
    assert_eq!(
        r.state.mode,
        RuntimeMode::Run,
        "an event task owes no run while untriggered"
    );
}

const GO: &str = "external.A.B.go";

/// One event task `edge`, running a program that counts its runs and reads GO into a BOOL.
fn event_runtime() -> VmEngine {
    const EV: &str = "
PROGRAM ev
VAR trig : BOOL; n : DINT; END_VAR
  n := n + 1;
END_PROGRAM
";
    start(json!({"run": true,
        "programs": [program(EV, 10, json!({"inputs": [{"variable": "trig", "tag": GO}]}))],
        "tasks": [{"name": "edge", "kind": {"type": "event", "tag": GO}, "priority": 1,
                   "programs": ["ev"]}]}))
}

/// The runs of `edge` after these exchanges, each followed by a pause long enough for a run.
fn edge_runs_after(engine: &mut VmEngine, sequence: Vec<Vec<WireInput>>) -> u64 {
    let mut last = Vec::new();
    for inputs in sequence {
        exchange(engine, inputs.clone());
        std::thread::sleep(Duration::from_millis(60));
        last = inputs;
    }
    task(&exchange(engine, last).state, "edge").runs
}

fn go(level: bool) -> Vec<WireInput> {
    vec![input(GO, Some(json!(level)), true)]
}

#[test]
fn false_then_not_good_then_true_is_no_edge() {
    let _one = one_at_a_time();
    let mut engine = event_runtime();
    // A GOOD FALSE before the tag was not GOOD: the TRUE after it is no edge, whatever the
    // value that was not GOOD.
    let runs = edge_runs_after(
        &mut engine,
        vec![
            go(false),
            vec![input(GO, Some(json!(true)), false)],
            go(true),
        ],
    );
    assert_eq!(runs, 0);
    let runs = edge_runs_after(
        &mut engine,
        vec![
            go(false),
            vec![input(GO, Some(json!(false)), false)],
            go(true),
        ],
    );
    assert_eq!(runs, 0);
    let runs = edge_runs_after(
        &mut engine,
        vec![go(false), vec![input(GO, None, true)], go(true)],
    );
    assert_eq!(runs, 0, "GOOD with no value");
    // The control: GOOD FALSE then GOOD TRUE is one.
    assert_eq!(edge_runs_after(&mut engine, vec![go(false), go(true)]), 1);
}

#[test]
fn an_exchange_that_omits_the_event_tag_makes_no_edge_across_it() {
    let _one = one_at_a_time();
    let mut engine = event_runtime();
    let other = vec![input("external.A.B.other", Some(json!(1)), true)];
    let runs = edge_runs_after(&mut engine, vec![go(false), vec![], go(true)]);
    assert_eq!(runs, 0, "FALSE, (no inputs), TRUE");
    let runs = edge_runs_after(&mut engine, vec![go(false), other, go(true)]);
    assert_eq!(runs, 0, "FALSE, (another tag only), TRUE");
    assert_eq!(edge_runs_after(&mut engine, vec![go(false), go(true)]), 1);
}

#[test]
fn every_program_of_a_task_sees_the_inputs_of_its_beginning() {
    let _one = one_at_a_time();
    // `first` takes the input, then runs long; `second` takes it after `first` has ended. Both
    // are latched when the task begins, so both take the same value though it changes at every
    // exchange, many times in `first`'s run.
    const FIRST: &str = "
PROGRAM first
VAR inp : DINT; seen : DINT; i : DINT; k : DINT; END_VAR
  seen := inp;
  FOR i := 1 TO 30000 DO
    k := k + (i MOD 2) - (i MOD 2);
  END_FOR;
END_PROGRAM
";
    const SECOND: &str = "
PROGRAM second
VAR inp : DINT; seen : DINT; END_VAR
  seen := inp;
END_PROGRAM
";
    const X: &str = "external.A.B.x";
    const SEEN_FIRST: &str = "internal.T.first";
    const SEEN_SECOND: &str = "internal.T.second";
    let mut engine = start(json!({"run": true,
        "programs": [
            program(FIRST, 10, json!({"inputs": [{"variable": "inp", "tag": X}],
                "published": [{"variable": "seen", "tag": SEEN_FIRST}]})),
            program(SECOND, 10, json!({"inputs": [{"variable": "inp", "tag": X}],
                "published": [{"variable": "seen", "tag": SEEN_SECOND}]})),
        ],
        "tasks": [{"name": "free", "kind": {"type": "cycle"}, "priority": 31,
                   "programs": ["first", "second"]}]}));
    let sample = |r: &ExchangeReply, uri: &str| {
        r.samples
            .iter()
            .find(|s| s.uri == uri)
            .and_then(|s| s.value)
    };
    let end = Instant::now() + Duration::from_millis(1_500);
    let mut n = 0i64;
    let mut compared = 0;
    let mut last = None;
    while Instant::now() < end {
        let r = exchange(&mut engine, vec![input(X, Some(json!(n)), true)]);
        if let (Some(a), Some(b)) = (sample(&r, SEEN_FIRST), sample(&r, SEEN_SECOND)) {
            assert_eq!(a, b, "the two programs of one run took different inputs");
            compared += 1;
        }
        last = Some(r);
        n += 1;
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(compared > 100, "{compared} exchanges compared");
    let last = last.unwrap();
    let longest = task(&last.state, "free").longest_run_us.unwrap();
    assert!(
        longest > 1_000,
        "a run spans several exchanges: {longest} µs"
    );
}

#[test]
fn each_task_has_its_own_watchdog() {
    let _one = one_at_a_time();
    // `long`'s one run takes longer than its watchdog allows; `quick`, of the highest
    // priority, ends a run every 10 ms and is within its own.
    let mut engine = start(json!({"run": true,
    "programs": [
        program(&slow_source("slow"), 10, json!({})),
        program(&quick_source("quick"), 10, json!({})),
    ],
    "tasks": [
        cyclic("quick", 10, 0, &["quick"]),
        {"name": "long", "kind": {"type": "cyclic", "interval_ms": 1000}, "priority": 5,
         "watchdog_ms": 5, "programs": ["slow"]},
    ]}));
    let r = until(&mut engine, "the watchdog stops it", |r| {
        r.state.mode == RuntimeMode::Fault
    });
    let fault = r.state.fault.clone().unwrap();
    assert!(
        fault.starts_with("watchdog: task 'long' has ended no run for ")
            && fault.ends_with(" ms, past its watchdog's 5 ms"),
        "{fault}"
    );
    assert!(
        task(&r.state, "quick").runs > 0,
        "the other task ran: {:?}",
        r.state
    );
    assert_eq!(task(&r.state, "long").runs, 0);
    assert_eq!(task(&r.state, "long").watchdog_ms, 5);
    assert_eq!(task(&r.state, "quick").watchdog_ms, 2_000);
    assert!(r.outputs.is_empty());
}

#[test]
fn a_watchdog_shorter_than_its_interval_faults_no_task_waiting_for_its_time() {
    let _one = one_at_a_time();
    // Each run takes far less than 100 ms, and the task waits 400 ms between them.
    let mut engine = start(json!({"run": true,
        "programs": [program(&quick_source("quick"), 10, json!({}))],
        "tasks": [{"name": "slowly", "kind": {"type": "cyclic", "interval_ms": 400,
                   "phase_ms": 100}, "priority": 1, "watchdog_ms": 100, "programs": ["quick"]}]}));
    let r = exchange_for(
        &mut engine,
        Duration::from_millis(1_500),
        Duration::from_millis(10),
        |_| Vec::new(),
        |r| assert_eq!(r.state.mode, RuntimeMode::Run, "{:?}", r.state.fault),
    );
    assert!(task(&r.state, "slowly").runs >= 3, "{:?}", r.state);
}

#[test]
fn a_variable_of_a_runtime_with_several_programs_is_named_by_its_program() {
    let _one = one_at_a_time();
    const A: &str = "
PROGRAM a
VAR RETAIN total : DINT; END_VAR
VAR x : INT; END_VAR
  total := total + 1;
  x := 1;
END_PROGRAM
";
    const B: &str = "
PROGRAM b
VAR x : INT; END_VAR
  x := 2;
END_PROGRAM
";
    let mut engine = start(json!({"run": true,
        "programs": [program(A, 10, json!({})), program(B, 10, json!({}))],
        "tasks": [cyclic("ta", 10, 1, &["a"]), cyclic("tb", 10, 2, &["b"])],
        "retained": [{"path": "a.total", "type": "DINT", "bytes": "64000000"}]}));
    until(&mut engine, "both ran", |r| {
        task(&r.state, "ta").runs > 0 && task(&r.state, "tb").runs > 0
    });
    let values = watch(&mut engine, &["a.x", "B.X", "x", "z.x", "a.nothing"]);
    assert_eq!(values[0], (Some(json!(1)), None, false));
    assert_eq!(
        values[1],
        (Some(json!(2)), None, false),
        "without regard to case"
    );
    assert_eq!(
        values[2].1.as_deref(),
        Some("name the program first, as in a.x (this runtime runs a, b)")
    );
    assert_eq!(
        values[3].1.as_deref(),
        Some("the runtime has no program z; it runs a, b")
    );
    assert_eq!(
        values[4].1.as_deref(),
        Some("the program has no variable nothing")
    );

    let state = finish(engine.force(&ctx(), "b.x".into(), json!(7))).unwrap();
    assert_eq!(state.forced[0].variable, "b.x");
    assert_eq!(
        watch(&mut engine, &["b.x"])[0],
        (Some(json!(7)), None, true)
    );
    assert_eq!(watch_one(&mut engine, "a.x"), json!(1), "a's x is another");
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        watch_one(&mut engine, "b.x"),
        json!(7),
        "held through b's runs"
    );
    assert!(finish(engine.force(&ctx(), "x".into(), json!(7))).is_err());
    finish(engine.unforce(&ctx(), Some("B.x".into()))).unwrap();
    until(&mut engine, "b writes its x again", |_| true);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(watch_one(&mut engine, "b.x"), json!(2));

    let kept = finish(engine.retained(&ctx())).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].path, "a.total");
    let total = hex::decode(&kept[0].bytes).unwrap();
    assert!(
        i32::from_le_bytes([total[0], total[1], total[2], total[3]]) > 100,
        "it went on from the value kept for it"
    );
    let all = finish(engine.variables(&ctx())).unwrap();
    let paths: HashSet<&str> = all.values.iter().map(|v| v.path.as_str()).collect();
    assert!(
        ["a.total", "a.x", "b.x"].iter().all(|p| paths.contains(p)),
        "{paths:?}"
    );
}

#[test]
fn the_module_refuses_what_only_it_can_check() {
    let _one = one_at_a_time();
    let refused = |config: Value| {
        finish(VmEngine::default().describe_config(&config))
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
    };
    let p = program(&quick_source("p"), 10, json!({}));
    assert!(
        refused(json!({"program": p.clone(), "programs": [p.clone()]}))
            .ends_with("give program or programs, not both")
    );
    assert!(refused(json!({"run": true})).ends_with("it runs no program"));
    // A rule of check_tasks, word for word.
    assert!(
        refused(json!({"programs": [p.clone(), program(&quick_source("q"), 10, json!({}))]}))
            .ends_with("it runs several programs and names no task: each program needs a task")
    );
    // The event's tag must be read into a BOOL by a program of the runtime.
    let ev = |bindings: Value| {
        refused(json!({
            "programs": [program(&quick_source("p"), 10, bindings)],
            "tasks": [{"name": "e", "kind": {"type": "event", "tag": "external.A.B.go"},
                       "priority": 1, "programs": ["p"]}]}))
    };
    let not_bool =
        "task 'e': its tag external.A.B.go is not read into a BOOL variable by any program of this runtime";
    assert!(ev(json!({})).ends_with(not_bool), "{}", ev(json!({})));
    assert!(
        ev(json!({"inputs": [{"variable": "n", "tag": "external.A.B.go"}]})).ends_with(not_bool),
        "a DINT"
    );
    // A misspelt field of a task is refused, not ignored.
    let typo = refused(json!({"programs": [p.clone()],
        "tasks": [{"name": "t", "kind": {"type": "cyclic", "interval_ms": 10, "phase": 3},
                   "priority": 1, "programs": ["p"]}]}));
    assert!(typo.contains("unknown field `phase`"), "{typo}");
    // Two programs of one runtime do not publish one tag.
    let published = json!({"published": [{"variable": "n", "tag": "internal.T.n"}]});
    let twice = refused(json!({
        "programs": [program(&quick_source("p"), 10, published.clone()),
                     program(&quick_source("q"), 10, published)],
        "tasks": [cyclic("t", 10, 1, &["p", "q"])]}));
    assert!(
        twice.ends_with("two variables are published as internal.T.n"),
        "{twice}"
    );
    // A program's own mistake names the program, when there are several.
    let missing = refused(json!({
        "programs": [program(&quick_source("p"), 10,
                        json!({"inputs": [{"variable": "nope", "tag": "external.A.B.c"}]})),
                     program(&quick_source("q"), 10, json!({}))],
        "tasks": [cyclic("t", 10, 1, &["p", "q"])]}));
    assert!(
        missing.ends_with("program 'p': the program has no variable nope"),
        "{missing}"
    );
}
