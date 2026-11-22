//! Data blocks handed over between the programs of one runtime (ADR-014, slice D2), driven
//! through the engine directly. A task run hands over as a whole: nothing reaches a block's
//! master before the task's run has ended, a later program of the task takes an earlier one's
//! members from that program's working memory, and a task run that faults hands nothing over.
//!
//! Every claim here is about what was computed (counts, values, "never unequal"), never about
//! when something happened.

use mythos_logic_compiler::{
    check_project, compile_sources, Compiled, DataBlock as Declared, Sources, TargetProfile,
};
use mythos_module::protocol::{
    CarryReport, DataSupport, DataUse, RetainedValue, RuntimeMode, TaskState, WireInput,
};
use mythos_module::RuntimeEngine;
use mythos_plugin_sdk::{MythosPlugin, PluginContext};
use mythos_runtime_vm::VmEngine;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// These tests run schedulers that preempt: one at a time, so that they do not share the
/// machine's cores with each other.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    static ONE: Mutex<()> = Mutex::new(());
    ONE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Drive a future of the engine to its end: its calls never wait on anything.
fn finish<T>(future: impl std::future::Future<Output = T>) -> T {
    struct Nothing;
    impl std::task::Wake for Nothing {
        fn wake(self: Arc<Self>) {}
    }
    let waker = std::task::Waker::from(Arc::new(Nothing));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

fn ctx() -> PluginContext {
    PluginContext::new("io.runtime:d", HashSet::new(), String::new())
}

const BLOCK: Declared = Declared { name: "S", ty: "T" };

/// A program compiled with the library `types` and the one data block `S` of type `T`.
fn compiled(types: &str, source: &str) -> Compiled {
    compile_sources(
        &Sources {
            st: Some(("main.st", source)),
            library: &[("types.st", types)],
            data: &[BLOCK],
            ..Default::default()
        },
        &TargetProfile::pc(),
    )
    .unwrap_or_else(|d| panic!("{d:?}"))
}

/// A runtime program of `source`, with what `Compiled.data` says as its `data`.
fn program(types: &str, source: &str) -> Value {
    let c = compiled(types, source);
    json!({
        "name": c.name,
        "version": format!("{}-1", c.name),
        "interval_ms": 10,
        "code": hex::encode(&c.bytecode),
        "symbols": c.symbols,
        "data": c.data,
    })
}

/// The runtime's `data`: block `S` as the project lays it out.
fn data(types: &str, retain: bool) -> Value {
    let layouts = check_project(&Sources {
        library: &[("types.st", types)],
        data: &[BLOCK],
        ..Default::default()
    })
    .unwrap_or_else(|d| panic!("{d:?}"));
    Value::Array(
        layouts
            .into_iter()
            .map(|l| {
                json!({
                    "name": l.name, "type": l.ty, "layout": l.layout, "size": l.size,
                    "retain": retain,
                    "members": l.members.iter().map(|m| json!({
                        "name": m.name, "type": m.ty, "offset": m.offset, "size": m.size,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn cyclic(name: &str, interval_ms: u64, priority: u8, programs: &[&str]) -> Value {
    json!({"name": name, "kind": {"type": "cyclic", "interval_ms": interval_ms},
           "priority": priority, "programs": programs})
}

fn cycle(name: &str, priority: u8, programs: &[&str]) -> Value {
    json!({"name": name, "kind": {"type": "cycle"}, "priority": priority, "programs": programs})
}

fn start(config: Value) -> VmEngine {
    let mut engine = VmEngine::default();
    finish(engine.start(&ctx(), &config)).unwrap_or_else(|e| panic!("{e}"));
    engine
}

fn refused(config: Value) -> String {
    let mut engine = VmEngine::default();
    match finish(engine.start(&ctx(), &config)) {
        Ok(()) => panic!("started: {config}"),
        Err(e) => e.to_string(),
    }
}

/// An exchange with no inputs: it feeds the runtime, so its tasks run.
fn feed(engine: &mut VmEngine) -> mythos_module::protocol::ExchangeReply {
    finish(engine.exchange(&ctx(), Vec::<WireInput>::new())).unwrap()
}

fn task<'a>(state: &'a mythos_module::protocol::RuntimeState, name: &str) -> &'a TaskState {
    state
        .tasks
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no task {name}: {state:?}"))
}

/// The values of `names`, read in one watch (under one lock), each as an integer.
fn ints(engine: &mut VmEngine, names: &[&str]) -> Vec<i64> {
    finish(engine.watch(&ctx(), names.iter().map(|s| s.to_string()).collect()))
        .unwrap()
        .values
        .into_iter()
        .map(|v| {
            v.value
                .as_ref()
                .and_then(Value::as_i64)
                .unwrap_or_else(|| panic!("{}: {:?}", v.variable, v.error))
        })
        .collect()
}

fn int(engine: &mut VmEngine, name: &str) -> i64 {
    ints(engine, &[name])[0]
}

/// Feed every few ms until `done` holds of the values of `names`, or panic after `secs`.
fn until(engine: &mut VmEngine, names: &[&str], secs: u64, done: impl Fn(&[i64]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let reply = feed(engine);
        assert_ne!(
            reply.state.mode,
            RuntimeMode::Fault,
            "{:?}",
            reply.state.fault
        );
        let values = ints(engine, names);
        if done(&values) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "never: {names:?} = {values:?}; {:?}",
            reply.state
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn retained(engine: &mut VmEngine) -> Vec<RetainedValue> {
    finish(engine.retained(&ctx())).unwrap()
}

/// The kept value of data block `S`.
fn kept_block(engine: &mut VmEngine) -> RetainedValue {
    retained(engine)
        .into_iter()
        .find(|v| v.layout.is_some() && v.path == "S")
        .expect("the block's master is kept")
}

fn dint_hex(v: i32) -> String {
    hex::encode(v.to_le_bytes())
}

// -----------------------------------------------------------------------------
// A preempting task, and the task run handed over whole
// -----------------------------------------------------------------------------

const PAIR: &str = "TYPE T : STRUCT a : DINT; b : DINT; END_STRUCT; END_TYPE";

const LOW: &str = "PROGRAM L
  VAR n : DINT; i : DINT; acc : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  n := n + 1;
  S.a := n;
  FOR i := 1 TO 20000 DO
    acc := acc + (i MOD 7);
  END_FOR;
  S.b := n;
END_PROGRAM";

const HIGH: &str = "PROGRAM H
  VAR bad : DINT; changes : DINT; last : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  IF S.a <> S.b THEN bad := bad + 1; END_IF;
  IF S.a <> last THEN changes := changes + 1; last := S.a; END_IF;
END_PROGRAM";

#[test]
fn a_preempting_task_never_sees_a_half_written_block() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true, "data": data(PAIR, false),
        "programs": [program(PAIR, LOW), program(PAIR, HIGH)],
        "tasks": [cycle("low", 20, &["L"]), cyclic("high", 1, 0, &["H"])]}));
    // Until the low task has been preempted and the high one has seen ten new values: then
    // the count of unequal pairs it saw is not vacuous.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let reply = feed(&mut engine);
        assert_eq!(
            reply.state.mode,
            RuntimeMode::Run,
            "{:?}",
            reply.state.fault
        );
        let changes = int(&mut engine, "H.changes");
        if changes >= 10 && task(&reply.state, "low").preemptions > 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{changes} changes: {:?}",
            reply.state
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        int(&mut engine, "H.bad"),
        0,
        "a half-written block was seen"
    );
}

const FIRST: &str = "PROGRAM A
  VAR cnt : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  cnt := cnt + 1;
  S.a := cnt;
END_PROGRAM";

const LONG: &str = "PROGRAM LONG
  VAR i : DINT; acc : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  FOR i := 1 TO 20000 DO
    acc := acc + (i MOD 7);
  END_FOR;
  S.b := S.a;
END_PROGRAM";

#[test]
fn a_preempting_task_sees_nothing_of_an_unfinished_task_run() {
    let _one = one_at_a_time();
    // Task `low` runs A, which writes S.a, then LONG, which takes S.a from A's working memory
    // and writes it into S.b at its end. Had A's S.a reached the master when A ended, the high
    // task would see S.a one ahead of S.b while LONG runs.
    let mut engine = start(json!({"run": true, "data": data(PAIR, false),
        "programs": [program(PAIR, FIRST), program(PAIR, LONG), program(PAIR, HIGH)],
        "tasks": [cycle("low", 20, &["A", "LONG"]), cyclic("high", 1, 0, &["H"])]}));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let reply = feed(&mut engine);
        assert_eq!(
            reply.state.mode,
            RuntimeMode::Run,
            "{:?}",
            reply.state.fault
        );
        let changes = int(&mut engine, "H.changes");
        if changes >= 10 && task(&reply.state, "low").preemptions > 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{changes} changes: {:?}",
            reply.state
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        int(&mut engine, "H.bad"),
        0,
        "the high task saw a member of a task run that had not ended"
    );
    // LONG took A's S.a of the same run: in their images, at the task's end, they agree.
    let [a, b, cnt] = ints(&mut engine, &["A.S.a", "LONG.S.b", "A.cnt"])[..] else {
        unreachable!()
    };
    assert_eq!((a, b), (cnt, cnt));
}

const ONE: &str = "TYPE T : STRUCT n : DINT; END_STRUCT; END_TYPE";

const COUNTER: &str = "PROGRAM A
  VAR cnt : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  cnt := cnt + 1;
  S.n := cnt;
END_PROGRAM";

const TAKER: &str = "PROGRAM B
  VAR got : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  got := S.n;
END_PROGRAM";

#[test]
fn two_programs_of_one_task_see_each_other_in_order() {
    let _one = one_at_a_time();
    for (order, behind) in [(["A", "B"], 0), (["B", "A"], 1)] {
        let mut engine = start(json!({"run": true, "data": data(ONE, false),
            "programs": [program(ONE, COUNTER), program(ONE, TAKER)],
            "tasks": [cyclic("t", 2, 5, &order)]}));
        until(&mut engine, &["A.cnt"], 10, |v| v[0] >= 5);
        for _ in 0..20 {
            feed(&mut engine);
            let [got, cnt] = ints(&mut engine, &["B.got", "A.cnt"])[..] else {
                unreachable!()
            };
            assert_eq!(got, cnt - behind, "order {order:?}");
            std::thread::sleep(Duration::from_millis(3));
        }
    }
}

const SHARED: &str = "TYPE
  Pos : STRUCT x : DINT; y : DINT; END_STRUCT;
  T : STRUCT speed : INT; pos : Pos; back : INT; END_STRUCT;
END_TYPE";

const WRITER: &str = "PROGRAM A
  VAR a1 : INT := 11; seen : INT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  S.speed := 40 + a1;
  S.pos.x := 7;
  S.pos.y := 8;
  seen := S.back * 2;
END_PROGRAM";

const READER: &str = "PROGRAM B
  VAR b0 : LREAL; b1 : BOOL; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  VAR out : DINT; END_VAR
  out := INT_TO_DINT(S.speed) + S.pos.x * 100 + S.pos.y * 1000;
  S.back := 21;
END_PROGRAM";

#[test]
fn a_member_reaches_a_program_of_another_task_by_the_writers_completed_run() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true, "data": data(SHARED, false),
        "programs": [program(SHARED, WRITER), program(SHARED, READER)],
        "tasks": [cyclic("w", 3, 4, &["A"]), cyclic("r", 5, 6, &["B"])]}));
    until(&mut engine, &["B.out", "A.seen"], 10, |v| {
        v == [51 + 700 + 8000, 42]
    });
    // Each copy holds the other's member, and its own.
    let copies = ints(
        &mut engine,
        &["A.S.back", "B.S.speed", "B.S.pos.y", "A.S.speed"],
    );
    assert_eq!(copies, [21, 51, 8, 51]);
}

// -----------------------------------------------------------------------------
// Faults
// -----------------------------------------------------------------------------

const V: &str = "TYPE T : STRUCT v : DINT; END_STRUCT; END_TYPE";

const FAULTS_AT_THREE: &str = "PROGRAM A
  VAR zero : DINT; x : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  S.v := S.v + 1;
  IF S.v = 3 THEN x := 1 / zero; END_IF;
END_PROGRAM";

fn until_fault(engine: &mut VmEngine) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let reply = feed(engine);
        if reply.state.mode == RuntimeMode::Fault {
            return reply.state.fault.unwrap_or_default();
        }
        assert!(
            Instant::now() < deadline,
            "never faulted: {:?}",
            reply.state
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_faulted_run_hands_nothing_over() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true, "data": data(V, true),
        "program": program(V, FAULTS_AT_THREE)}));
    let fault = until_fault(&mut engine);
    assert!(fault.contains("division by zero"), "{fault}");
    let kept = kept_block(&mut engine);
    assert_eq!(
        kept.bytes,
        dint_hex(2),
        "the faulted run's 3 is not in the master"
    );
    assert_eq!(kept.ty, "T");
    assert_eq!(int(&mut engine, "S.v"), 2, "nor in the image");
}

const LATER_FAULTS: &str = "PROGRAM F
  VAR m : DINT; zero : DINT; x : DINT; END_VAR
  m := m + 1;
  IF m = 3 THEN x := 1 / zero; END_IF;
END_PROGRAM";

const SEER: &str = "PROGRAM R
  VAR seen : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  seen := S.n;
END_PROGRAM";

#[test]
fn a_task_whose_later_program_faults_hands_nothing_over() {
    let _one = one_at_a_time();
    // Task `t` runs A, which counts into S.n, then F, which faults in its third run. A has
    // ended its third run when F faults; nothing of it may reach the master.
    let mut engine = start(json!({"run": true, "data": data(ONE, true),
        "programs": [program(ONE, COUNTER), program(ONE, LATER_FAULTS), program(ONE, SEER)],
        "tasks": [cyclic("t", 5, 5, &["A", "F"]), cyclic("r", 1, 1, &["R"])]}));
    let fault = until_fault(&mut engine);
    assert!(fault.contains("program 'F' run 3 stopped"), "{fault}");
    assert_eq!(kept_block(&mut engine).bytes, dint_hex(2));
    let [cnt, an, seen] = ints(&mut engine, &["A.cnt", "A.S.n", "R.seen"])[..] else {
        unreachable!()
    };
    assert_eq!(
        (cnt, an),
        (2, 2),
        "A's image is of its last completed task run"
    );
    assert!(seen <= 2, "the reader of another task saw {seen}");
}

// -----------------------------------------------------------------------------
// Forces
// -----------------------------------------------------------------------------

const X: &str = "TYPE T : STRUCT x : DINT; END_STRUCT; END_TYPE";

const SETS_X: &str = "PROGRAM A
  VAR_EXTERNAL S : T; END_VAR
  S.x := 5;
END_PROGRAM";

const READS_X1: &str = "PROGRAM R1
  VAR r : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  r := S.x;
END_PROGRAM";

const READS_X2: &str = "PROGRAM R2
  VAR r : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  r := S.x;
END_PROGRAM";

fn force(engine: &mut VmEngine, variable: &str, value: i64) {
    finish(engine.force(&ctx(), variable.into(), json!(value))).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_force_on_a_member_wins_over_the_hand_over() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true, "data": data(X, true),
        "programs": [program(X, SETS_X), program(X, READS_X1), program(X, READS_X2)],
        "tasks": [cyclic("a", 2, 3, &["A"]), cyclic("r", 2, 4, &["R1", "R2"])]}));
    until(&mut engine, &["R1.r", "R2.r"], 10, |v| v == [5, 5]);

    // Forced in its writer: what the writer hands over, so every reader takes it.
    force(&mut engine, "A.S.x", 9);
    until(&mut engine, &["R1.r", "R2.r"], 10, |v| v == [9, 9]);
    assert_eq!(kept_block(&mut engine).bytes, dint_hex(9));
    finish(engine.unforce(&ctx(), None)).unwrap();
    until(&mut engine, &["R1.r", "R2.r"], 10, |v| v == [5, 5]);

    // Forced in a reader: that reader reads it, and neither the master nor the other reader
    // (a later program of the same task) changes.
    force(&mut engine, "R1.S.x", 7);
    until(&mut engine, &["R1.r"], 10, |v| v == [7]);
    for _ in 0..10 {
        feed(&mut engine);
        assert_eq!(ints(&mut engine, &["R1.r", "R2.r"]), [7, 5]);
        assert_eq!(kept_block(&mut engine).bytes, dint_hex(5));
        std::thread::sleep(Duration::from_millis(3));
    }
}

// -----------------------------------------------------------------------------
// Initial values, RETAIN, a runtime loaded in STOP
// -----------------------------------------------------------------------------

const INIT: &str = "TYPE T : STRUCT k : DINT := 77; w : DINT; END_STRUCT; END_TYPE";

const WRITES_W: &str = "PROGRAM A
  VAR_EXTERNAL S : T; END_VAR
  S.w := S.w + 1;
END_PROGRAM";

const READS_K: &str = "PROGRAM B
  VAR k : DINT; w : DINT; END_VAR
  VAR_EXTERNAL S : T; END_VAR
  k := S.k;
  w := S.w;
END_PROGRAM";

#[test]
fn a_member_no_program_writes_keeps_its_initial_value_in_every_copy() {
    let _one = one_at_a_time();
    let mut engine = start(json!({"run": true, "data": data(INIT, true),
        "programs": [program(INIT, WRITES_W), program(INIT, READS_K)],
        "tasks": [cyclic("a", 2, 3, &["A"]), cyclic("b", 2, 4, &["B"])]}));
    until(&mut engine, &["B.w"], 10, |v| v[0] >= 3);
    assert_eq!(ints(&mut engine, &["A.S.k", "B.S.k", "B.k"]), [77, 77, 77]);
    let kept = hex::decode(kept_block(&mut engine).bytes).unwrap();
    assert_eq!(&kept[..4], &77i32.to_le_bytes(), "the master holds it too");
}

const COUNTS: &str = "TYPE T : STRUCT v : DINT := 5; END_STRUCT; END_TYPE";

const COUNTS_ON: &str = "PROGRAM A
  VAR_EXTERNAL S : T; END_VAR
  S.v := S.v + 1;
END_PROGRAM";

const SEES_V: &str = "PROGRAM B
  VAR_EXTERNAL S : T; END_VAR
  VAR got : DINT; END_VAR
  got := S.v;
END_PROGRAM";

fn counting(retain: bool, extra: Value) -> Value {
    let mut config = json!({"run": true, "data": data(COUNTS, retain),
        "programs": [program(COUNTS, COUNTS_ON), program(COUNTS, SEES_V)],
        "tasks": [cyclic("a", 2, 3, &["A"]), cyclic("b", 2, 4, &["B"])]});
    if let Value::Object(e) = extra {
        config.as_object_mut().unwrap().extend(e);
    }
    config
}

fn set_mode(engine: &mut VmEngine, run: bool, cold: bool) {
    finish(engine.set_mode(&ctx(), run, cold)).unwrap();
}

#[test]
fn a_retained_block_survives_a_warm_restart_and_not_a_cold_one() {
    let _one = one_at_a_time();
    let mut engine = start(counting(true, json!({})));
    until(&mut engine, &["A.S.v"], 10, |v| v[0] >= 10);
    set_mode(&mut engine, false, false);
    let kept = kept_block(&mut engine);
    let at_stop = i32::from_le_bytes(hex::decode(&kept.bytes).unwrap().try_into().unwrap());
    assert!(at_stop >= 10);
    assert_eq!(int(&mut engine, "A.S.v"), i64::from(at_stop));

    // Warm: every copy holds the kept value before any run (none runs before an exchange),
    // and the writer counts on from it.
    set_mode(&mut engine, true, false);
    assert_eq!(
        ints(&mut engine, &["A.S.v", "B.S.v"]),
        [i64::from(at_stop); 2]
    );
    until(&mut engine, &["A.S.v", "B.got"], 10, |v| {
        v[0] > i64::from(at_stop) && v[1] > i64::from(at_stop)
    });

    // Cold: the type's initial value, in every copy and in the master.
    set_mode(&mut engine, true, true);
    assert_eq!(ints(&mut engine, &["A.S.v", "B.S.v"]), [5, 5]);
    assert_eq!(kept_block(&mut engine).bytes, dint_hex(5));

    // A block that is not RETAIN starts from its initial value at a warm restart too, and is
    // not kept.
    let mut plain = start(counting(false, json!({})));
    until(&mut plain, &["A.S.v"], 10, |v| v[0] >= 8);
    set_mode(&mut plain, false, false);
    assert!(retained(&mut plain).iter().all(|v| v.layout.is_none()));
    set_mode(&mut plain, true, false);
    assert_eq!(ints(&mut plain, &["A.S.v", "B.S.v"]), [5, 5]);
}

fn kept_value(layout: &str, bytes: &str) -> Value {
    json!({"path": "S", "type": "T", "bytes": bytes, "layout": layout})
}

#[test]
fn a_retained_block_of_another_layout_is_not_taken() {
    let _one = one_at_a_time();
    let layout = data(COUNTS, true)[0]["layout"]
        .as_str()
        .unwrap()
        .to_string();
    // Taken: the block's layout and size; and the writer counts on from it.
    let mut taken = start(counting(
        true,
        json!({"retained": [kept_value(&layout, &dint_hex(1000))]}),
    ));
    assert_eq!(ints(&mut taken, &["A.S.v", "B.S.v"]), [1000, 1000]);
    until(&mut taken, &["B.got"], 10, |v| v[0] > 1000);

    // Not taken: another layout, or bytes not the block's size. The first image stands, the
    // type's initial value, not zeros. The warning goes only to the module's log, which this
    // test does not capture.
    for (other, bytes) in [
        ("0123456789abcdef", dint_hex(1000)),
        (layout.as_str(), "e803".to_string()),
    ] {
        let mut engine = start(counting(
            true,
            json!({"retained": [kept_value(other, &bytes)]}),
        ));
        assert_eq!(
            ints(&mut engine, &["A.S.v", "B.S.v"]),
            [5, 5],
            "{other} {bytes}"
        );
        assert_eq!(kept_block(&mut engine).bytes, dint_hex(5));
    }
}

#[test]
fn a_block_in_a_runtime_loaded_in_stop_gives_back_the_kept_value() {
    let _one = one_at_a_time();
    let layout = data(COUNTS, true)[0]["layout"]
        .as_str()
        .unwrap()
        .to_string();
    let given = vec![
        kept_value(&layout, &dint_hex(1234)),
        // A value of a block the runtime does not have.
        json!({"path": "Gone", "type": "T", "bytes": "01000000", "layout": layout}),
    ];
    let mut engine = start(counting(true, json!({"run": false, "retained": given})));
    let back: Vec<Value> = retained(&mut engine)
        .iter()
        .filter(|v| v.layout.is_some())
        .map(|v| serde_json::to_value(v).unwrap())
        .collect();
    assert_eq!(back, given, "given back as given, never zeros");
    // Put in RUN: a warm start from it.
    set_mode(&mut engine, true, false);
    assert_eq!(ints(&mut engine, &["A.S.v", "B.S.v"]), [1234, 1234]);
    let after: Vec<RetainedValue> = retained(&mut engine)
        .into_iter()
        .filter(|v| v.layout.is_some())
        .collect();
    assert_eq!(
        after.len(),
        1,
        "the value of no block is dropped at RUN: {after:?}"
    );
    assert_eq!(after[0].bytes, dint_hex(1234));
}

// -----------------------------------------------------------------------------
// Online change
// -----------------------------------------------------------------------------

const V1: &str = "TYPE T : STRUCT n : DINT; r : INT; END_STRUCT; END_TYPE";
const V2: &str = "TYPE T : STRUCT n : DINT; r : DINT := 9; added : INT := 4; END_STRUCT; END_TYPE";

const BUMPS: &str = "PROGRAM A
  VAR_EXTERNAL S : T; END_VAR
  S.n := S.n + 1;
  S.r := 3;
END_PROGRAM";

fn online(types: &str, carried: Option<&mythos_module::protocol::ProgramVariables>) -> Value {
    let mut config = json!({"run": true, "data": data(types, false),
        "programs": [program(types, BUMPS), program(types, TAKER)],
        "tasks": [cyclic("a", 2, 3, &["A"]), cyclic("b", 2, 4, &["B"])]});
    if let Some(c) = carried {
        config["carried"] = serde_json::to_value(c).unwrap();
    }
    config
}

fn report(engine: &mut VmEngine) -> CarryReport {
    finish(engine.carried(&ctx())).unwrap()
}

#[test]
fn an_online_change_carries_the_block_whole_when_its_layout_is_the_same_and_by_leaf_when_not() {
    let _one = one_at_a_time();
    let mut old = start(online(V1, None));
    until(&mut old, &["A.S.n", "B.got"], 10, |v| {
        v[0] >= 10 && v[1] >= 10
    });
    let carried_running = finish(old.variables(&ctx())).unwrap();
    drop(old);
    let path_n = |c: &mythos_module::protocol::ProgramVariables| {
        let v = c.values.iter().find(|v| v.path == "A.S.n").unwrap();
        i32::from_le_bytes(hex::decode(&v.bytes).unwrap().try_into().unwrap())
    };
    let n = i64::from(path_n(&carried_running));
    assert!(n >= 10);

    // The same layout: the writer's counter goes on, and every copy agrees with it.
    let mut same = start(online(V1, Some(&carried_running)));
    assert_eq!(
        ints(&mut same, &["A.S.n", "B.S.n", "A.S.r", "B.S.r"]),
        [n, n, 3, 3]
    );
    let r = report(&mut same);
    assert!(r.initialized.is_empty(), "{r:?}");
    until(&mut same, &["B.got"], 10, |v| v[0] > n);

    // A member added, one retyped: the untouched member carries, the changed ones take their
    // first image, in every copy.
    let mut changed = start(online(V2, Some(&carried_running)));
    assert_eq!(
        ints(
            &mut changed,
            &["A.S.n", "B.S.n", "A.S.r", "B.S.r", "A.S.added", "B.S.added"]
        ),
        [n, n, 9, 9, 4, 4]
    );
    let r = report(&mut changed);
    let initialized: Vec<(&str, &str)> = r
        .initialized
        .iter()
        .map(|i| (i.variable.as_str(), i.why.as_str()))
        .collect();
    for expected in [
        ("A.S.r", "its type is DINT now, not INT"),
        (
            "A.S.added",
            "the running program had no variable of that name",
        ),
        ("B.S.r", "its type is DINT now, not INT"),
        (
            "B.S.added",
            "the running program had no variable of that name",
        ),
    ] {
        assert!(
            initialized.contains(&expected),
            "{expected:?} in {initialized:?}"
        );
    }
    assert!(!initialized.iter().any(|(v, _)| v.ends_with("S.n")));
    until(&mut changed, &["B.got"], 10, |v| v[0] > n);
}

// -----------------------------------------------------------------------------
// A runtime of today
// -----------------------------------------------------------------------------

const TODAY: &str = "PROGRAM P
  VAR RETAIN total : DINT := 3; END_VAR
  VAR n : DINT; END_VAR
  n := n + 1;
END_PROGRAM";

#[test]
fn a_runtime_without_data_runs_on_the_same_path() {
    let _one = one_at_a_time();
    let c = mythos_logic_compiler::compile(TODAY, &TargetProfile::pc()).unwrap();
    let p = json!({"name": "P", "version": "1", "interval_ms": 10,
        "code": hex::encode(&c.bytecode), "symbols": c.symbols});
    assert!(
        serde_json::to_value(&c.data).unwrap() == json!([]),
        "a program of today declares no block"
    );
    let without = json!({"run": true, "program": p});
    let mut with = without.clone();
    with["data"] = json!([]);
    let mut a = start(without);
    let mut b = start(with);
    // Not yet fed: nothing has run, so both are exactly as they were set.
    let ra = serde_json::to_value(retained(&mut a)).unwrap();
    assert_eq!(
        ra,
        json!([{"path": "total", "type": "DINT", "bytes": "03000000"}])
    );
    assert_eq!(ra, serde_json::to_value(retained(&mut b)).unwrap());
    let va = finish(a.variables(&ctx())).unwrap();
    let vb = finish(b.variables(&ctx())).unwrap();
    assert_eq!(va.values, vb.values);
    let wire = serde_json::to_value(&va).unwrap();
    assert!(wire.get("data").is_none());
    assert!(wire["values"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v.get("layout").is_none()));
    // Running, the state has the keys of today.
    let state = |e: &mut VmEngine| {
        let s = serde_json::to_value(feed(e).state).unwrap();
        let mut keys: Vec<String> = s.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(state(&mut a), state(&mut b));
    assert_eq!(
        state(&mut a),
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
}

// -----------------------------------------------------------------------------
// The wire and what the module refuses
// -----------------------------------------------------------------------------

#[test]
fn the_compilers_data_json_is_the_protocols() {
    let c = compiled(SHARED, WRITER);
    for d in &c.data {
        let wire = serde_json::to_value(d).unwrap();
        let read: DataUse = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(&read).unwrap(), wire);
        let mut no_written = wire.clone();
        no_written["members"][0]
            .as_object_mut()
            .unwrap()
            .remove("written");
        assert!(serde_json::from_value::<DataUse>(no_written).is_err());
    }
    assert!(!c.data.is_empty());
}

#[test]
fn the_module_refuses_a_copy_outside_its_memory() {
    let config = |change: &dyn Fn(&mut Value)| {
        let mut a = program(SHARED, WRITER);
        change(&mut a["data"][0]);
        json!({"data": data(SHARED, false), "programs": [a, program(SHARED, READER)],
               "tasks": [cyclic("t", 5, 5, &["A", "B"])]})
    };
    // R14.
    let e = refused(config(&|d| d["offset"] = json!(4_000_000_000u32)));
    assert!(
        e.contains("program 'A': its copy of data block 'S' lies outside the program's memory"),
        "{e}"
    );
    // R15: inside the memory (two bytes before the copy: A's `a1` and `seen` lie there), and
    // not where the symbols say.
    let e = refused(config(&|d| {
        let at = d["offset"].as_u64().unwrap();
        assert!(at >= 2, "{at}");
        d["offset"] = json!(at - 2);
    }));
    assert!(
        e.contains("program 'A': its symbols do not place data block 'S' where its data says"),
        "{e}"
    );
    let e = refused(config(&|d| d["members"][1]["offset"] = json!(0)));
    assert!(
        e.contains("other members than the runtime's T"),
        "R9 before R15: {e}"
    );
    // R15 from the symbols' side, each way on its own: a member whose first leaf is missing,
    // so its leaves begin past its offset; and one whose leaf lies past its end.
    let symbols = |change: &dyn Fn(&mut Vec<Value>)| {
        let mut a = program(SHARED, WRITER);
        let mut list = a["symbols"].as_array().unwrap().clone();
        change(&mut list);
        a["symbols"] = Value::Array(list);
        json!({"data": data(SHARED, false), "programs": [a, program(SHARED, READER)],
               "tasks": [cyclic("t", 5, 5, &["A", "B"])]})
    };
    let at = |list: &[Value], path: &str| {
        list.iter()
            .position(|s| s["path"] == path)
            .unwrap_or_else(|| panic!("no symbol {path}"))
    };
    for change in [
        &(|list: &mut Vec<Value>| {
            let x = at(list, "S.pos.x");
            list.remove(x);
        }) as &dyn Fn(&mut Vec<Value>),
        &|list: &mut Vec<Value>| {
            let (y, back) = (at(list, "S.pos.y"), at(list, "S.back"));
            list[y]["offset"] = list[back]["offset"].clone();
        },
    ] {
        let e = refused(symbols(change));
        assert!(
            e.contains("program 'A': its symbols do not place data block 'S' where its data says"),
            "{e}"
        );
    }
    // And the shared check, through the module: R11.
    let e = refused(config(&|d| d["members"][2]["written"] = json!(true)));
    assert!(
        e.contains("data block 'S': member 'back' is written by programs 'A' and 'B'; a member has one writer"),
        "{e}"
    );
    // The control: as compiled, it is taken.
    let mut engine = VmEngine::default();
    finish(engine.start(&ctx(), &config(&|_| {}))).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn the_manifest_says_it_hands_data_blocks_over() {
    let m = mythos_runtime_vm::manifest();
    assert_eq!(
        DataSupport::of(&m.profile).expect("profile.data").tiers,
        [1]
    );
    assert_eq!(m.version, "0.6.0");
}
