//! A scan run in pieces, suspended and resumed between them (ADR-013 §2.19, slice C8a),
//! computes exactly what the same scan computes uninterrupted: after every scan the whole
//! memory, the report and the instruction count are equal, and a fault is the same fault at the
//! same point. And two tasks on one thread, scheduled by `mythos_logic_vm::sched` on a clock
//! that advances with the instructions executed, each compute what they compute alone.

use mythos_logic_compiler::{compile, compile_in, Compiled, Language, TargetProfile};
use mythos_logic_vm::sched::{self, Kind, Next, Task};
use mythos_logic_vm::{cold_start, scan, Fault, Frame, Piece, Program, Run, ScanReport};
use std::path::{Path, PathBuf};

const BUDGET: u32 = 1_000_000;
const MS: i64 = 1_000_000;

/// Every program the other end-to-end tests compare with STruC++ or with Structured Text, and
/// that the emulators run: Structured Text, Ladder Diagram, Function Block Diagram and
/// Sequential Function Chart. A file added to one of these folders is compared here too.
fn programs() -> Vec<(PathBuf, Compiled)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let folders = [
        root.join("tests/differential"),
        root.join("tests/ladder"),
        root.join("tests/fbd"),
        root.join("../mythos-logic-graphic/tests/sfc"),
    ];
    let mut found = Vec::new();
    for folder in folders {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
            .unwrap_or_else(|e| panic!("{}: {e}", folder.display()))
            .map(|e| e.unwrap().path())
            .filter(|p| matches!(p.extension().and_then(|x| x.to_str()), Some("st" | "xml")))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no programs in {}", folder.display());
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            // As `examples/expected_scans.rs` compiles them: one reader takes LD, FBD and SFC.
            let language = if path.extension().unwrap() == "xml" {
                Language::Ld
            } else {
                Language::St
            };
            let c = compile_in(&text, language, &TargetProfile::pc())
                .unwrap_or_else(|d| panic!("{}: {d:?}", path.display()));
            found.push((path, c));
        }
    }
    found
}

/// A seeded generator, written here: the same lengths on every run and every machine.
struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// The working space of one program: memory, stack and call frames, as a host gives them.
struct Space {
    memory: Vec<u8>,
    stack: Vec<u64>,
    calls: Vec<Frame>,
}
impl Space {
    fn new(p: &Program) -> Space {
        let mut memory = vec![0; p.memory_size()];
        cold_start(p, &mut memory).unwrap();
        Space {
            memory,
            stack: vec![0; p.stack_depth()],
            calls: vec![Frame::default(); p.call_depth()],
        }
    }
    fn scan(&mut self, p: &Program, now: i64, budget: u32) -> Result<ScanReport, Fault> {
        scan(
            p,
            &mut self.memory,
            &mut self.stack,
            &mut self.calls,
            now,
            budget,
        )
    }
    /// One scan in pieces whose lengths `piece` gives: how it ended, the run, and how many
    /// pieces it took.
    fn pieces(
        &mut self,
        p: &Program,
        now: i64,
        budget: u32,
        piece: &mut dyn FnMut() -> u32,
    ) -> (Result<ScanReport, Fault>, Run, u32) {
        let mut run = Run::begin(p, now, budget);
        let mut calls = 0u32;
        loop {
            calls += 1;
            match run.resume(
                p,
                &mut self.memory,
                &mut self.stack,
                &mut self.calls,
                piece(),
            ) {
                Ok(Piece::Suspended) => assert!(!run.ended()),
                Ok(Piece::Ended(report)) => return (Ok(report), run, calls),
                Err(fault) => return (Err(fault), run, calls),
            }
        }
    }
}

/// How a program is cut: a fixed length, or pseudo-random lengths from 1 to 40.
#[derive(Debug, Clone, Copy)]
enum Cut {
    Fixed(u32),
    Random(u64),
}

/// `scans` scans of `p` uninterrupted, and the same from the same start in pieces cut as `cut`
/// says, each at the clock `clock` gives for it; every scan compared. Returns how many
/// instructions were executed in all.
fn compare(name: &str, p: &Program, cut: Cut, scans: u32, clock: impl Fn(u32) -> i64) -> u64 {
    let mut whole = Space::new(p);
    let mut cut_up = Space::new(p);
    let mut rng = XorShift(match cut {
        Cut::Random(seed) => seed,
        Cut::Fixed(_) => 1,
    });
    let mut length = || match cut {
        Cut::Fixed(n) => n,
        Cut::Random(_) => (rng.next() % 40) as u32 + 1,
    };
    let mut executed = 0u64;
    for k in 0..scans {
        let now = clock(k);
        let expected = whole.scan(p, now, BUDGET);
        let (got, run, pieces) = cut_up.pieces(p, now, BUDGET, &mut length);
        assert_eq!(got, expected, "{name}, {cut:?}, scan {k}: the outcome");
        assert!(
            whole.memory == cut_up.memory,
            "{name}, {cut:?}, scan {k}: the memory differs"
        );
        assert_eq!(
            run.now(),
            now,
            "{name}: the run keeps the clock it began with"
        );
        let report = expected.unwrap_or_else(|f| panic!("{name}, scan {k}: {f:?}"));
        assert_eq!(run.instructions(), report.instructions, "{name}, scan {k}");
        if let Cut::Fixed(n) = cut {
            // Every piece but the last ran exactly n instructions: the run was suspended
            // where it should have been, and resumed from there.
            assert_eq!(
                pieces,
                report.instructions.div_ceil(n),
                "{name}, {cut:?}, scan {k}: the number of pieces"
            );
        }
        executed += report.instructions as u64;
    }
    executed
}

#[test]
fn every_program_computes_in_pieces_exactly_what_it_computes_uninterrupted() {
    let programs = programs();
    // Eight Structured Text programs, four Ladder, three FBD, three SFC at the time of writing.
    assert!(programs.len() >= 18, "{} programs", programs.len());
    // Uneven scan intervals, so timers and step times see more than one spacing.
    let clock = |k: u32| k as i64 * 20 * MS + ((k * k) % 7) as i64 * MS;
    for (path, c) in &programs {
        let name = path.file_name().unwrap().to_str().unwrap();
        let p = Program::load(&c.bytecode).unwrap();
        let cuts = [
            Cut::Fixed(1),
            Cut::Fixed(2),
            Cut::Fixed(3),
            Cut::Fixed(7),
            Cut::Fixed(64),
            Cut::Random(0x9E37_79B9_7F4A_7C15),
            Cut::Random(0xD1B5_4A32_D192_ED03),
        ];
        for cut in cuts {
            let executed = compare(name, &p, cut, 120, clock);
            assert!(executed > 120, "{name}: the scans did something");
        }
    }
}

fn load(src: &str) -> (Compiled, Vec<u8>) {
    let c = compile(src, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
    let bytes = c.bytecode.clone();
    (c, bytes)
}

/// The cuts every fault test runs with.
fn fault_cuts() -> [Cut; 6] {
    [
        Cut::Fixed(1),
        Cut::Fixed(2),
        Cut::Fixed(3),
        Cut::Fixed(7),
        Cut::Fixed(64),
        Cut::Random(42),
    ]
}

/// One scan of `src` at budget `budget`, uninterrupted and in pieces, for every cut: the same
/// outcome and the same memory. Returns the outcome.
fn same_end(src: &str, budget: u32) -> Result<ScanReport, Fault> {
    let (_, bytes) = load(src);
    let p = Program::load(&bytes).unwrap();
    let mut outcome = None;
    for cut in fault_cuts() {
        let mut whole = Space::new(&p);
        let mut cut_up = Space::new(&p);
        let mut rng = XorShift(7);
        let mut length = || match cut {
            Cut::Fixed(n) => n,
            Cut::Random(_) => (rng.next() % 40) as u32 + 1,
        };
        let expected = whole.scan(&p, 0, budget);
        let (got, run, _) = cut_up.pieces(&p, 0, budget, &mut length);
        assert_eq!(got, expected, "{cut:?}");
        assert!(whole.memory == cut_up.memory, "{cut:?}: the memory differs");
        if got == Err(Fault::BudgetExceeded) {
            assert_eq!(
                run.instructions(),
                budget,
                "{cut:?}: the whole budget, no more"
            );
        }
        outcome = Some(expected);
    }
    outcome.unwrap()
}

#[test]
fn the_budget_counts_the_instructions_of_the_whole_run_across_its_pieces() {
    let endless =
        "PROGRAM P VAR a : DINT; END_VAR WHILE TRUE DO a := a + 1; END_WHILE; END_PROGRAM";
    assert_eq!(same_end(endless, 1000), Err(Fault::BudgetExceeded));
    assert_eq!(same_end(endless, 1), Err(Fault::BudgetExceeded));
    assert_eq!(same_end(endless, 0), Err(Fault::BudgetExceeded));

    // A scan of exactly n instructions ends with a budget of n and not with n - 1, cut or not.
    let finite =
        "PROGRAM P VAR i, a : DINT; END_VAR FOR i := 1 TO 30 DO a := a + i; END_FOR; END_PROGRAM";
    let n = same_end(finite, BUDGET).unwrap().instructions;
    assert!(n > 64, "long enough to be cut by every cut: {n}");
    assert_eq!(same_end(finite, n), Ok(ScanReport { instructions: n }));
    assert_eq!(same_end(finite, n - 1), Err(Fault::BudgetExceeded));
}

#[test]
fn a_fault_inside_a_piece_is_the_same_fault_at_the_same_point() {
    let divide = "PROGRAM P VAR i, a : DINT; b : DINT := 0; c : DINT; END_VAR
        FOR i := 1 TO 25 DO a := a + i; END_FOR;
        c := a / b;
      END_PROGRAM";
    assert!(matches!(
        same_end(divide, BUDGET),
        Err(Fault::DivideByZero(_))
    ));

    let index = "PROGRAM P VAR i : INT; x : ARRAY[1..10] OF INT; END_VAR
        FOR i := 1 TO 20 DO x[i] := i; END_FOR;
      END_PROGRAM";
    assert!(matches!(
        same_end(index, BUDGET),
        Err(Fault::IndexOutOfRange(_))
    ));
}

#[test]
fn a_run_that_has_ended_answers_its_end_again_and_executes_nothing() {
    let (_, bytes) = load("PROGRAM P VAR a : DINT; END_VAR a := a + 1; END_PROGRAM");
    let p = Program::load(&bytes).unwrap();
    let mut s = Space::new(&p);
    let mut run = Run::begin(&p, 0, BUDGET);
    let first = run.resume(&p, &mut s.memory, &mut s.stack, &mut s.calls, 1000);
    let Ok(Piece::Ended(report)) = first else {
        panic!("{first:?}")
    };
    let memory = s.memory.clone();
    let again = run.resume(&p, &mut s.memory, &mut s.stack, &mut s.calls, 1000);
    assert_eq!(again, Ok(Piece::Ended(report)));
    assert_eq!(run.instructions(), report.instructions);
    assert!(s.memory == memory, "a := a + 1 ran once");

    let (_, bytes) = load("PROGRAM P VAR a, b : INT; END_VAR a := a / b; END_PROGRAM");
    let p = Program::load(&bytes).unwrap();
    let mut s = Space::new(&p);
    let mut run = Run::begin(&p, 0, BUDGET);
    let fault = run.resume(&p, &mut s.memory, &mut s.stack, &mut s.calls, 1000);
    assert!(matches!(fault, Err(Fault::DivideByZero(_))), "{fault:?}");
    assert!(run.ended());
    let again = run.resume(&p, &mut s.memory, &mut s.stack, &mut s.calls, 1000);
    assert_eq!(again, fault);
}

#[test]
fn a_piece_of_zero_instructions_is_a_piece_of_one() {
    let (_, bytes) = load(
        "PROGRAM P VAR i, a : DINT; END_VAR FOR i := 1 TO 3 DO a := a + i; END_FOR; END_PROGRAM",
    );
    let p = Program::load(&bytes).unwrap();
    let mut whole = Space::new(&p);
    let n = whole.scan(&p, 0, BUDGET).unwrap().instructions;
    let mut s = Space::new(&p);
    let mut run = Run::begin(&p, 0, BUDGET);
    assert_eq!(run.instructions(), 0);
    for k in 1..n {
        let r = run.resume(&p, &mut s.memory, &mut s.stack, &mut s.calls, 0);
        assert_eq!(r, Ok(Piece::Suspended));
        assert_eq!(run.instructions(), k, "one instruction per piece");
    }
    let r = run.resume(&p, &mut s.memory, &mut s.stack, &mut s.calls, 0);
    assert_eq!(r, Ok(Piece::Ended(ScanReport { instructions: n })));
    assert!(whole.memory == s.memory);
}

#[test]
fn a_run_resumed_with_a_program_of_a_shallower_stack_faults_and_does_not_panic() {
    // A holds up to four values on its stack, B two: at some suspensions of A, more values are
    // on the stack than B's stack has room for.
    let (_, a_bytes) = load(
        "PROGRAM A VAR a, b, c, d, e, f, g, h, i : DINT; END_VAR
           a := ((b + c) * (d + e)) + ((f + g) * (h + i));
           a := ((b + c) * (d + e)) + ((f + g) * (h + i));
         END_PROGRAM",
    );
    let (_, b_bytes) = load(
        "PROGRAM B VAR x, y, z : DINT; END_VAR
           x := y + z; x := y + z; x := y + z; x := y + z; x := y + z; x := y + z;
         END_PROGRAM",
    );
    let a = Program::load(&a_bytes).unwrap();
    let b = Program::load(&b_bytes).unwrap();
    assert!(a.stack_depth() > b.stack_depth(), "the premise of the test");

    let mut whole = Space::new(&a);
    let expected = whole.scan(&a, 0, BUDGET);
    let mut s = Space::new(&a);
    let mut run = Run::begin(&a, 0, BUDGET);
    let mut refused = 0;
    loop {
        // From this suspension of A, a copy of the run resumed with B and B's own space.
        let mut wrong = run;
        let mut other = Space::new(&b);
        let r = wrong.resume(&b, &mut other.memory, &mut other.stack, &mut other.calls, 1);
        if r == Err(Fault::WrongSizes) {
            refused += 1;
        }
        match run.resume(&a, &mut s.memory, &mut s.stack, &mut s.calls, 1) {
            Ok(Piece::Suspended) => {}
            end => {
                assert_eq!(end.map(|_| expected.unwrap()), expected, "A itself");
                break;
            }
        }
    }
    assert!(
        whole.memory == s.memory,
        "A itself computes what it computes"
    );
    assert!(
        refused > 0,
        "a run holding more stack than B has is refused, not followed"
    );
}

#[test]
fn a_suspended_run_reads_the_clock_it_began_with_when_it_is_resumed_later() {
    // The timer has run since the first scan, at 0: in the second scan, begun at 50 ms, its
    // elapsed time is 50 ms, however much later the pieces of that scan are executed.
    let src = "PROGRAM P VAR t : TON; et : TIME; q : BOOL; END_VAR
        t(IN := TRUE, PT := T#1h, Q => q, ET => et);
      END_PROGRAM";
    let (c, bytes) = load(src);
    let p = Program::load(&bytes).unwrap();
    let et = c.symbol("et").unwrap().offset as usize;
    let mut s = Space::new(&p);
    s.scan(&p, 0, BUDGET).unwrap();

    // Between every two pieces of the scan begun at 50 ms another scan runs, complete, at a
    // clock that keeps moving: the next run, handed a different clock, while this one waits.
    let mut other = Space::new(&p);
    let mut later = 10_000 * MS;
    let mut run = Run::begin(&p, 50 * MS, BUDGET);
    let mut pieces = 0;
    loop {
        other.scan(&p, later, BUDGET).unwrap();
        later += 7 * MS;
        pieces += 1;
        match run.resume(&p, &mut s.memory, &mut s.stack, &mut s.calls, 1) {
            Ok(Piece::Suspended) => assert_eq!(run.now(), 50 * MS),
            Ok(Piece::Ended(_)) => break,
            Err(f) => panic!("{f:?}"),
        }
    }
    assert!(pieces > 3, "the scan was suspended: {pieces} pieces");
    let elapsed = i64::from_le_bytes(s.memory[et..et + 8].try_into().unwrap());
    assert_eq!(elapsed, 50 * MS);

    // The next run is handed the next clock, and reads that one.
    s.scan(&p, 80 * MS, BUDGET).unwrap();
    let elapsed = i64::from_le_bytes(s.memory[et..et + 8].try_into().unwrap());
    assert_eq!(elapsed, 80 * MS);
}

/// One begin or end in a simulation, at a simulated time in nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Begin(usize, i64),
    End(usize, i64),
}

/// What a simulation of two tasks did: its events, and the scheduler's tasks at the end.
struct Simulation {
    events: Vec<Event>,
    tasks: Vec<Task>,
    runs_checked: u32,
}

/// Simulated nanoseconds per instruction, and instructions per piece.
const NS_PER_INSTRUCTION: i64 = 10;
const PIECE: u32 = 50;
const HIGH_INTERVAL: i64 = 5_000;
const LOW_INTERVAL: i64 = 200_000;
const SIMULATED: i64 = 1_000_000;

/// A long program at priority 10, every 200 µs, and a short one at priority 1, every 5 µs, on
/// one thread: the scheduler says what to run, a piece of at most 50 instructions runs, and
/// the clock advances 10 ns per instruction executed. After every run, the program's memory is
/// compared with the same run alone: the memory it began with, scanned uninterrupted at the
/// clock the run began with.
fn simulate() -> Simulation {
    let low = "PROGRAM Low VAR i, n, acc : DINT; t : TON; et : TIME; END_VAR
        n := n + 1;
        FOR i := 1 TO 300 DO acc := acc + i * n; END_FOR;
        t(IN := TRUE, PT := T#1h, ET => et);
      END_PROGRAM";
    let high = "PROGRAM High VAR c : DINT; t : TON; et : TIME; END_VAR
        c := c + 1;
        t(IN := TRUE, PT := T#1h, ET => et);
      END_PROGRAM";
    let bytes = [load(high).1, load(low).1];
    let programs: Vec<Program> = bytes.iter().map(|b| Program::load(b).unwrap()).collect();
    let mut spaces: Vec<Space> = programs.iter().map(Space::new).collect();
    let mut tasks = vec![
        Task::new(
            Kind::Cyclic {
                interval: HIGH_INTERVAL,
                phase: 0,
            },
            1,
        )
        .unwrap(),
        Task::new(
            Kind::Cyclic {
                interval: LOW_INTERVAL,
                phase: 0,
            },
            10,
        )
        .unwrap(),
    ];
    sched::start(&mut tasks, 0);
    let mut runs: [Option<(Run, Vec<u8>)>; 2] = [None, None];
    let mut events = Vec::new();
    let mut runs_checked = 0;
    let mut t = 0i64;
    while t < SIMULATED {
        let k = match sched::next(&mut tasks, t) {
            Next::Idle => {
                t = sched::wake(&tasks).expect("cyclic tasks are always due again");
                continue;
            }
            Next::Begin(k) => {
                assert!(runs[k].is_none(), "task {k} begun twice");
                runs[k] = Some((
                    Run::begin(&programs[k], t, BUDGET),
                    spaces[k].memory.clone(),
                ));
                events.push(Event::Begin(k, t));
                k
            }
            Next::Resume(k) => k,
        };
        let (run, before) = runs[k].as_mut().expect("a run in progress");
        let s = &mut spaces[k];
        let done = run.instructions();
        let r = run.resume(
            &programs[k],
            &mut s.memory,
            &mut s.stack,
            &mut s.calls,
            PIECE,
        );
        t += (run.instructions() - done) as i64 * NS_PER_INSTRUCTION;
        match r {
            Ok(Piece::Suspended) => {}
            Ok(Piece::Ended(report)) => {
                assert!(sched::ended(&mut tasks, k, t));
                events.push(Event::End(k, t));
                // The same run alone: same memory to begin with, same clock.
                let mut alone = Space {
                    memory: before.clone(),
                    stack: vec![0; programs[k].stack_depth()],
                    calls: vec![Frame::default(); programs[k].call_depth()],
                };
                let expected = alone.scan(&programs[k], run.now(), BUDGET);
                assert_eq!(Ok(report), expected, "task {k}, run at {}", run.now());
                assert!(
                    alone.memory == s.memory,
                    "task {k}: memory of run at {}",
                    run.now()
                );
                runs_checked += 1;
                runs[k] = None;
            }
            Err(f) => panic!("task {k}: {f:?}"),
        }
    }
    Simulation {
        events,
        tasks,
        runs_checked,
    }
}

#[test]
fn two_tasks_on_one_thread_interleave_as_the_rules_say_and_each_computes_what_it_computes_alone() {
    let sim = simulate();
    let again = simulate();
    assert_eq!(sim.events, again.events, "the same interleaving every time");

    let high_begins: Vec<i64> = sim
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Begin(0, at) => Some(*at),
            _ => None,
        })
        .collect();
    // The high task begins once per due time, within one piece of it: each due time k * 5 µs.
    let piece_ns = PIECE as i64 * NS_PER_INSTRUCTION;
    for (k, at) in high_begins.iter().enumerate() {
        let due = k as i64 * HIGH_INTERVAL;
        assert!(
            *at >= due && *at < due + piece_ns,
            "high run {k} began at {at}, due at {due}"
        );
    }
    assert!(high_begins.len() as i64 >= SIMULATED / HIGH_INTERVAL - 1);

    // The high task's runs are short: never preempted, never overrun. Every begin of it is
    // followed by its end before anything else begins.
    let high = sim.tasks[0].counters();
    assert_eq!(high.overruns, 0);
    assert_eq!(high.preemptions, 0);
    for w in sim.events.windows(2) {
        if let Event::Begin(0, _) = w[0] {
            assert!(matches!(w[1], Event::End(0, _)), "{w:?}");
        }
    }

    // The low task is long: preempted by every high run that began while it was in progress,
    // and resumed after; it began only when no high run was in progress.
    let mut low_running = false;
    let mut preemptions = 0;
    let mut low_runs = 0;
    for e in &sim.events {
        match e {
            Event::Begin(1, _) => low_running = true,
            Event::End(1, _) => {
                low_running = false;
                low_runs += 1;
            }
            Event::Begin(0, _) if low_running => preemptions += 1,
            _ => {}
        }
    }
    let low = sim.tasks[1].counters();
    assert!(low_runs >= 4, "{low_runs} low runs");
    assert!(preemptions >= low_runs, "each low run was preempted");
    assert_eq!(low.preemptions, preemptions);
    assert_eq!(low.runs, low_runs);
    assert_eq!(low.overruns, 0);
    let longest = low.longest_run.unwrap();
    assert!(
        longest > HIGH_INTERVAL,
        "a low run spans several high runs: {longest} ns"
    );

    assert_eq!(u64::from(sim.runs_checked), high.runs + low.runs);
}
