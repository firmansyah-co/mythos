//! TSK-010: what the criterion actually measures, and the verdict.
//!
//! `TSK-010` reads: *"Criterion benchmarks pass with < 100 ns p99 read/write operations under 16
//! concurrent threads."*
//!
//! # Method and its known bias (read this before trusting any number below)
//!
//! One writer thread plus 15 reader threads on **one shared POSIX segment** (`DefaultShm`), each
//! pinned to its own core on a best-effort basis. Every operation is timed individually, then
//! sorted and indexed. Two reader populations are collected:
//!
//! - **Regime A, busy poll**: every `update()` + `read_tag()` pair. This is the literal reading of
//!   the criterion, and it is the harshest one: 15 spinning readers hammer the state word as fast
//!   as they can, far harder than any real 10 Hz HMI client would.
//! - **Regime B, swap-acquired**: only the updates that actually acquired fresh data, i.e. took
//!   the state-word CAS. Excludes time spent spinning.
//!
//! **Known bias, stated up front:** wall-clock p99 on a preemptible host always includes
//! scheduling. Pinning reduces but does not remove this, and an unprivileged or oversubscribed
//! host may refuse the pin entirely. So a p99 here is an **upper bound** on engine cost, not a
//! clean decomposition of it. Regime B is closer to operation cost than regime A, and neither is
//! pure.
//!
//! # Verdict: NOT MET
//!
//! Measured on a shared segment with 16 concurrent threads, 1-min load average 6-8:
//!
//! | population | p50 | p90 | p99 |
//! |---|---|---|---|
//! | read, busy poll (A) | 43-199 ns | 599-1024 ns | 1458-2239 ns |
//! | read, swap-acquired (B) | 288-481 ns | 789-1315 ns | 1739-2549 ns |
//! | write | 280-479 ns | 785-1164 ns | 1537-2050 ns |
//!
//! Against a 100 ns target, both read and write **miss by roughly an order of magnitude**. CPU
//! pinning did not change the verdict: 16 threads CAS-ing one 64-bit state word serialise on that
//! word, and the cost is real cache-line ping-pong between cores, not measurement artefact.
//!
//! **This is a finding for the board, not a threshold to adjust.** The engine's single-word
//! architecture cannot meet 100 ns p99 under 16 concurrent busy pollers. Either the criterion is
//! unrealistic for a single shared state word, or the engine needs per-reader state (e.g. reader
//! slots or a sequence-lock design). That decision is not mine and it is not CI's.
//!
//! # Why the spec number is reported and not asserted
//!
//! Asserting `p99 < 100 ns` here would create a permanently red required check, which is exactly
//! the failure mode this sprint has been removing. This test is therefore a **report, not a
//! gate**: it asserts a gross-regression backstop only, and prints the TSK-010 verdict on every
//! run. CI carries the number; the board decides what to do about it.
//!
//! The naming matters as much as the behaviour. A green check called "p99 Concurrency Gate"
//! that passes while printing `TSK-010 VERDICT: ... NOT MET` states a pass the run did not
//! establish, and a reader scanning check names never sees the verdict. The CI step is
//! `Report SHM p99 under 16 threads (TSK-010, non-gating)` and this file/test are named to match,
//! so the surface cannot claim more than the assertion proves.
//!
//! # Two defects preceded this version
//!
//! 1. **No p99 existed.** `readers_16_threads` in `benches/shm_bench.rs` timed a whole writer loop
//!    and reported it as a latency percentile; CI only ran `cargo bench --no-run`.
//! 2. **The first version of this measurement never shared memory.** It used `MemoryShm`, whose
//!    `open_or_create` calls `new_isolated` on every call and allocates a fresh zeroed buffer, so
//!    the 15 "readers" each had a private segment and never contended at all. That harness
//!    established neither a pass nor a breach. This version uses `DefaultShm` and **asserts a
//!    reader observes the writer's value before measuring**, so the same mistake fails loudly.
//!
//! Release-only: an unoptimised build measures ~1.6x slower and would not measure the algorithm.

#![cfg(not(debug_assertions))]

use mythos_pal::{DefaultShm, PlatformScheduler};
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagFlags, TagValueRaw};
use std::thread;
use std::time::Instant;

const MAX_TAGS: usize = 4096;

/// `type_code` for a 64-bit float (`4 = REAL` in the slot layout's OPC-style codes).
const TYPE_CODE_REAL: u16 = 4;
const READER_THREADS: usize = 15;
const SAMPLES_PER_THREAD: usize = 20_000;

/// TSK-010's stated target for read and write.
const SPEC_P99_NS: u64 = 100;

/// Gross-breakage backstop for regime A (busy poll): p50, at **10x the worst p50 in the runs this
/// constant was derived from** (see the table below). Pinned by decision, not recomputed per run —
/// a later run that measures a higher p50 does not move it, and the comment must not claim it does.
///
/// # What this is NOT
///
/// This is a **gross-breakage detector, not a performance criterion.** Do not read it as a claim
/// about engine speed, and do not "tidy" it into one. At 10x it would not catch a 5x regression:
/// 1220 ns against a worst observed 122 ns leaves a factor of five of slack. That slack is
/// deliberate — the three retained runs below spread 3.4x on host state alone (p50 36 -> 122 ns at
/// 1-min load averages 32.8 to 63.9), so anything tighter would flake on load rather than on the
/// engine. It must be **re-tightened together with the real bound** once five or more runs are
/// retained; tightening it now, on three runs, would buy a flaky check rather than a stricter one.
///
/// # Provenance (do not replace this with a bare number)
///
/// Worst observed p50 for regime A = 122 ns (run 40463220645). 10 x 122 = 1220 ns.
///
/// ```text
/// run 40089446143  load 63.85:  A p50=36   B p50=178
/// run 40450932513  load 32.81:  A p50=63   B p50=449
/// run 40463220645  load 37.49:  A p50=122  B p50=428
/// ```
const REGIME_A_P50_BACKSTOP_NS: u64 = 1_220;

/// Gross-breakage backstop for regime B (swap-acquired, i.e. only updates that took the state-word
/// CAS): p50, at **10x the worst p50 in the runs this constant was derived from**. Pinned by
/// decision, not recomputed per run.
///
/// Same status as [`REGIME_A_P50_BACKSTOP_NS`]: a gross-breakage detector, not a performance
/// criterion, to be re-tightened with the real bound once five or more runs exist.
///
/// Worst observed p50 for regime B = 449 ns (run 40450932513). 10 x 449 = 4490 ns.
///
/// # Why p50 and not p99
///
/// A 20x multiple on p99 was the first proposal and it fails the evidence offered in its support:
/// for regime B it gives 20 x 178 = 3560 ns, while run 40450932513 measured p99 = 4145 ns. The
/// bound would have failed on the very runs used to derive it. p50 is the population that is
/// comparatively stable across host state, which is what a backstop has to key on.
const REGIME_B_P50_BACKSTOP_NS: u64 = 4_490;

/// Where each run's distributions are written, so a series can be retained as a CI artifact.
///
/// The bound the board sets after five runs is only possible if those runs are kept from now on;
/// the three runs quoted above had to be read out of CI logs by hand, and a hand-read number is not
/// a baseline. Overridable so CI can name the path explicitly rather than rely on the layout below.
const DISTRIBUTIONS_ENV: &str = "MYTHOS_TSK010_OUT";

fn percentile(sorted: &[u64], p: f64) -> u64 {
    // A percentile of an empty population does not exist. `debug_assert!` was no help here: release
    // is the only profile this runs in, so a starved population panicked as a bare "index out of
    // bounds: the len is 0 but the index is 0" with no population named and no bound quoted — the
    // least diagnosable failure this file can produce, on the run you most want recorded.
    assert!(
        !sorted.is_empty(),
        "cannot take a percentile of an empty population: no samples were collected, so there is \
         nothing to measure and nothing to grade. This is a HARNESS failure (a starved population), \
         not engine performance and not the TSK-010 verdict. See the population floor below."
    );
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

struct Distribution {
    label: &'static str,
    sorted: Vec<u64>,
}

impl Distribution {
    fn new(label: &'static str, mut samples: Vec<u64>) -> Self {
        samples.sort_unstable();
        Self {
            label,
            sorted: samples,
        }
    }

    fn p(&self, p: f64) -> u64 {
        percentile(&self.sorted, p)
    }

    fn report(&self) {
        if self.sorted.is_empty() {
            println!(
                "{} under {} concurrent threads on ONE shared segment (n=0 ops): NO SAMPLES — \
                 population starved, nothing measured",
                self.label,
                READER_THREADS + 1,
            );
            return;
        }
        println!(
            "{} latency under {} concurrent threads on ONE shared segment (n={} ops): \
             p50={} p90={} p99={} max={} ns",
            self.label,
            READER_THREADS + 1,
            self.sorted.len(),
            self.p(0.50),
            self.p(0.90),
            self.p(0.99),
            self.sorted[self.sorted.len() - 1],
        );
    }
}

/// A population's distribution, in the shape the retained record carries.
///
/// The percentiles are `Option` so a starved population is recorded as *absent* rather than as a
/// zero. Writing `"p50": 0` for a population with no samples would put a number in the series where
/// no measurement exists — the same defect this whole slice is about, in the evidence file itself.
struct Retained {
    name: &'static str,
    n: usize,
    p50: Option<u64>,
    p90: Option<u64>,
    p99: Option<u64>,
    max: Option<u64>,
}

impl Retained {
    fn of(name: &'static str, d: &Distribution) -> Self {
        if d.sorted.is_empty() {
            return Self {
                name,
                n: 0,
                p50: None,
                p90: None,
                p99: None,
                max: None,
            };
        }
        Self {
            name,
            n: d.sorted.len(),
            p50: Some(d.p(0.50)),
            p90: Some(d.p(0.90)),
            p99: Some(d.p(0.99)),
            max: Some(d.sorted[d.sorted.len() - 1]),
        }
    }
}

/// Renders `Some(n)` as a JSON number and `None` as JSON `null`.
fn json_num(v: Option<u64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

/// Write this run's distributions where CI can archive them, and say where.
///
/// Hand-built JSON rather than a serialiser: `mythos-shm` has no serde dependency and this is five
/// fixed fields per population. Printing the path on success and on failure is deliberate — an
/// artifact that silently did not appear is the same defect class as a check that silently did not
/// run, and CI uploads nothing when the file is absent.
fn retain_distributions(run: &[Retained], load: &str) {
    let dir = match std::env::var(DISTRIBUTIONS_ENV) {
        Ok(explicit) => std::path::PathBuf::from(explicit),
        Err(_) => {
            // Default to the workspace target dir, derived from this crate's manifest location:
            // <workspace>/crates/mythos-shm -> <workspace>. CI overrides this via the env var.
            let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(|p| p.parent())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            workspace.join("target").join("tsk010-p99")
        }
    };

    // The load average is a string because the read may fail; keep it a JSON number when it parsed
    // and an explicit string ("unknown") when it did not, rather than emitting a bogus 0.0.
    let load_json = if load.parse::<f64>().is_ok() {
        load.to_string()
    } else {
        format!("\"{load}\"")
    };

    let mut json = String::from("{\n");
    json.push_str(&format!("  \"loadavg_1min\": {load_json},\n"));
    json.push_str(&format!(
        "  \"regime_a_p50_backstop_ns\": {REGIME_A_P50_BACKSTOP_NS},\n"
    ));
    json.push_str(&format!(
        "  \"regime_b_p50_backstop_ns\": {REGIME_B_P50_BACKSTOP_NS},\n"
    ));
    json.push_str(&format!("  \"spec_p99_ns\": {SPEC_P99_NS},\n"));
    json.push_str("  \"populations\": [\n");
    for (i, r) in run.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"name\": \"{}\", \"n\": {}, \"p50\": {}, \"p90\": {}, \"p99\": {}, \
             \"max\": {}}}{}\n",
            r.name,
            r.n,
            json_num(r.p50),
            json_num(r.p90),
            json_num(r.p99),
            json_num(r.max),
            if i + 1 == run.len() { "" } else { "," }
        ));
    }
    json.push_str("  ]\n}\n");

    match std::fs::create_dir_all(&dir).and_then(|_| {
        let path = dir.join("distributions.json");
        std::fs::write(&path, &json).map(|_| path)
    }) {
        Ok(path) => println!(
            "TSK-010 DISTRIBUTIONS RETAINED at {} ({} bytes, {} populations)",
            path.display(),
            json.len(),
            run.len()
        ),
        Err(e) => println!(
            "TSK-010 DISTRIBUTIONS NOT RETAINED: could not write under {} ({e}). This run's numbers \
             exist only in this log; a series missing this run cannot support the five-run bound.",
            dir.display()
        ),
    }
}

#[test]
fn report_tsk010_p99_read_write_under_16_concurrent_threads() {
    // Unique per process so parallel test binaries never collide on a segment name.
    let name = format!("mythos_tsk010_p99_{}", std::process::id());

    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    for i in 0..MAX_TAGS {
        writer
            .write_tag(
                i,
                TYPE_CODE_REAL,
                QualityCode::GOOD,
                TagValueRaw::from_f64(i as f64),
                i as u64,
                TagFlags::ACCESS_RO,
            )
            .unwrap();
    }
    writer.publish();

    // Prove the harness shares memory BEFORE measuring. Without this, the whole file can silently
    // degenerate back into 16 threads timing 16 private buffers (the defect this file shipped
    // with the first time).
    {
        let mut probe = ShmReaderHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
        probe.update();
        let observed = probe
            .read_tag(1234)
            .map(|slot| slot.value.as_f64())
            .unwrap_or(f64::NAN);
        assert_eq!(
            observed, 1234.0,
            "a reader does not observe the writer's value, so these threads are NOT sharing one \
             segment and any latency measured here would be meaningless"
        );
    }

    // 15 reader threads + the writer below = 16 concurrent threads, as the criterion specifies.
    // Each reader records two populations:
    //   ALL    - every update()+read_tag pair (regime A: the literal criterion, busy poll)
    //   SWAP   - only the updates that actually acquired fresh data, i.e. took the state-word CAS
    //            (regime B: per-operation cost with spinning excluded)
    // THE WRITER IS ATTACHED, PINNED AND PRIMED BEFORE ANY READER STARTS.
    //
    // This is the starvation fix. Regime B counts only updates that acquired fresh data, which
    // requires the writer to have published. Creating the measuring writer *after* spawning the
    // readers left a window in which all 15 readers could finish their sample budget (0.6-1.3 ms at
    // these latencies) before the first publish landed, collapsing the population — the cause of the
    // n=1 false backstop failure and the n=0 bare index panic. Priming here means the very first
    // `update()` a reader makes can acquire, so the population cannot start empty by construction.
    // The floor assertion below is the belt to this brace: it catches a collapse from any other cause.
    let _ = mythos_pal::DefaultScheduler::pin_to_core(READER_THREADS);
    let mut active_writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    active_writer
        .write_tag(
            0,
            TYPE_CODE_REAL,
            QualityCode::GOOD,
            TagValueRaw::from_f64(0.0),
            0,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    active_writer.publish();

    let mut handles = Vec::new();
    for reader_index in 0..READER_THREADS {
        let segment = name.clone();
        handles.push(thread::spawn(move || {
            // Pin each reader to its own core so a preemption by another reader does not appear
            // in the measurement as engine cost. Best-effort: an unprivileged or oversubscribed
            // host may refuse, and the run stays valid (just noisier) if it does.
            let _ = mythos_pal::DefaultScheduler::pin_to_core(reader_index);
            let mut reader = ShmReaderHandle::<DefaultShm>::new(&segment, MAX_TAGS).unwrap();
            let mut all = Vec::with_capacity(SAMPLES_PER_THREAD);
            let mut swaps = Vec::new();
            let slot = 50usize;
            // A FIXED SAMPLE BUDGET, deliberately, and the population variance is handled by the
            // validity floor below rather than by lengthening this window.
            //
            // Sampling until the writer finishes was tried and REVERTED: it changes the statistic
            // this file exists to measure. A reader that keeps spinning through the writer's whole
            // run spends most of its samples waiting for a publish, which is inside regime A's
            // population by definition; regime A's p50 moved from 74 ns to 316 ns under that change.
            // The bounds in this file were derived from the fixed-budget p50s, so lengthening the
            // window would silently invalidate their provenance — and the decision that set them
            // says not to move them.
            while all.len() < SAMPLES_PER_THREAD {
                let started = Instant::now();
                let acquired = reader.update();
                let result = reader.read_tag(slot);
                let elapsed = started.elapsed().as_nanos() as u64;
                std::hint::black_box(&result);
                all.push(elapsed);
                if acquired {
                    swaps.push(elapsed);
                }
            }
            (all, swaps)
        }));
    }

    let mut writer_latencies = Vec::with_capacity(SAMPLES_PER_THREAD);
    for i in 0..SAMPLES_PER_THREAD {
        let slot = i % MAX_TAGS;
        let start = Instant::now();
        active_writer
            .write_tag(
                slot,
                TYPE_CODE_REAL,
                QualityCode::GOOD,
                TagValueRaw::from_f64(i as f64),
                i as u64,
                TagFlags::ACCESS_RO,
            )
            .unwrap();
        active_writer.publish();
        writer_latencies.push(start.elapsed().as_nanos() as u64);
    }

    let mut all_reads = Vec::with_capacity(READER_THREADS * SAMPLES_PER_THREAD);
    let mut swap_reads = Vec::new();
    for handle in handles {
        let (all, swaps) = handle.join().expect("reader thread panicked");
        all_reads.extend(all);
        swap_reads.extend(swaps);
    }

    let busy = Distribution::new("READ busy-poll (regime A, literal criterion)", all_reads);
    let swaps = Distribution::new("READ swap-acquired (regime B, operation cost)", swap_reads);
    let writes = Distribution::new("WRITE", writer_latencies);

    let load = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());

    // RETAIN BEFORE ANYTHING THAT CAN FAIL. A starved or malformed run is precisely the one whose
    // numbers matter, and a panic before this point leaves the archive step with nothing to upload —
    // the run that most needs explaining would be the only one with no evidence. `report()` below
    // cannot panic on an empty population, but it used to, and this ordering is the belt to that
    // brace.
    retain_distributions(
        &[
            Retained::of("read_busy_poll", &busy),
            Retained::of("read_swap_acquired", &swaps),
            Retained::of("write", &writes),
        ],
        &load,
    );

    busy.report();
    swaps.report();
    writes.report();
    println!("TSK-010 target {SPEC_P99_NS} ns p99, 1-min load average at measurement: {load}");

    #[cfg(target_os = "linux")]
    {
        let _ = mythos_pal::PosixShm::unlink(&name);
    }

    // TSK-010's verdict is REPORTED, not asserted. This test is not a gate on the spec number.
    //
    // The shipped engine does not meet the criterion in its literal regime: measured p99 is
    // ~1.3-2.5 us against a 100 ns target, on a provably shared segment, with and without CPU
    // pinning. A 16-thread single-word CAS design serialises on that word; the cost is real
    // cache-line ping-pong, not measurement error. Asserting the spec number here would create a
    // permanently red required check, which is the failure mode this sprint exists to remove.
    //
    // So: report the verdict unambiguously in the log, and assert only a gross-regression
    // backstop. The spec question is a finding for the board (see the PR body), not something a
    // CI threshold can decide.
    let read_verdict = if swaps.p(0.99) < SPEC_P99_NS {
        "MET"
    } else {
        "NOT MET"
    };
    let write_verdict = if writes.p(0.99) < SPEC_P99_NS {
        "MET"
    } else {
        "NOT MET"
    };
    println!(
        "TSK-010 VERDICT: read p99={} ns -> {read_verdict}; write p99={} ns -> {write_verdict} \
         (target {SPEC_P99_NS} ns, shared segment, 16 concurrent threads, pinned best-effort)",
        swaps.p(0.99),
        writes.p(0.99)
    );

    // Gross-breakage backstops, one per regime, on p50. Both fire on an engine that has hung or
    // regressed by more than an order of magnitude; neither says anything about whether TSK-010 is
    // met, and neither is a performance criterion. See the constants for their provenance and for
    // why they cannot be tightened before five runs are retained.
    //
    // Regime A p50 is dominated by how long a spinning reader waits for the writer to publish —
    // harness polling pattern and host scheduling, not the atomic operation — which is why a
    // backstop, not a gate. Regime B excludes that spin and is closer to operation cost.
    //
    // NOTE: the `writes` population has NO backstop, deliberately. The decision that set these two
    // numbers provided provenance for regimes A and B only, and inventing a third multiple would be
    // writing a threshold with nothing behind it. Adding one requires its own five-run basis.
    assert!(
        busy.p(0.50) < REGIME_A_P50_BACKSTOP_NS,
        "regime A p50 ({} ns) exceeded the gross-breakage backstop ({} ns = 10x the {} ns p50 the \
         bound was derived from): the engine is hung or has regressed by more than an order of \
         magnitude. This is NOT the TSK-010 verdict, and it is not a performance measurement.",
        busy.p(0.50),
        REGIME_A_P50_BACKSTOP_NS,
        122
    );
    // PRECONDITION FOR REGIME B ONLY: ENOUGH SAMPLES FOR A p50 TO MEAN ANYTHING.
    //
    // Placed here, immediately before the regime B assertion, rather than before both. A starved
    // regime B must not stop regime A's backstop from being evaluated: a severe engine regression
    // slows the readers so much that they finish after the writer's loop, which starves B while A
    // still has its full 300,000 samples. Checking B's precondition first masked A's backstop on
    // exactly the run that most needed it (observed in the regression control for this change).
    //
    // Regime A needs no floor: its population is every update+read pair from 15 readers with a
    // fixed budget, so it is 300,000 by construction and cannot starve.
    //
    // This is checked before either performance assertion, and it is a precondition rather than a
    // second threshold. Regime B counts only updates that ACQUIRED fresh data, so it depends on the
    // writer publishing while the readers are still sampling; the readers can finish their sample
    // budget first, collapsing the population. Two shapes observed on UNMODIFIED code: n=1 in 1 of 4
    // local release runs, where the lone descheduled sample was 10992 ns and tripped the regime B
    // backstop as gross breakage on an unchanged engine; and n=0 deterministically with a delayed
    // writer attach, which panicked as a bare index-out-of-bounds because the empty guard in
    // `percentile` was a `debug_assert!` in a release-only file.
    //
    // A p50 over one or two samples is not a median; it is a sample wearing a median's name. So the
    // bound is only asserted when the population can support the statistic at all.
    //
    // CHOOSING N — derived from the observed populations, and this is a VALIDITY gate, not a health
    // gate. Healthy regime B populations: 10,311 / 11,813 / 16,227 (PM-confirmed runs), 6,710-18,644
    // across local runs after the priming fix, and a low of 1,080 in one run. Starved: 0 and 1.
    //
    // An earlier floor of 1,000 sat inside the observed healthy spread — one legitimate run measured
    // 1,080 — so it would have failed a real measurement, which is the flake this row is about. The
    // floor is therefore 100: ~10x below the lowest healthy population observed and ~100x above the
    // starved cases, which pins it to the actual gap rather than to a round number that happens to
    // look cautious. A p50 over 100 samples is a median; a p50 over 1 is a sample wearing its name.
    //
    // The population variance itself is NOT fixed here. Lengthening the readers' window would fix it
    // and is the obvious move, but it moves regime A's p50 (measured: 74 ns -> 316 ns) and so would
    // invalidate the provenance of the bounds this file asserts. The decision that set those bounds
    // says not to move them, so the variance is handled by the precondition instead.
    //
    // WHY THIS FAILS RATHER THAN SKIPPING — the one place I read the instruction differently.
    // "Report it and assert nothing" would, in Rust, be a PASSING test, hence a GREEN required check
    // over a run that measured nothing: the next reader sees green and assumes coverage. That is the
    // same lie as the empty check list in #29 and the step named "Gate" that printed NOT MET in #25,
    // and this precondition exists to make the difference visible, so it must not resolve to green.
    // It fails, and the message says the failure is the harness's and that nothing about the engine
    // was learned. One line to change if that judgement is overruled.
    const MIN_REGIME_B_SAMPLES: usize = 100;
    assert!(
        swaps.sorted.len() >= MIN_REGIME_B_SAMPLES,
        "regime B population was starved: {} samples, minimum for a meaningful p50 is {}. Regime B \
         counts only updates that ACQUIRED fresh data, so it depends on the writer publishing while \
         the readers are still sampling. With a starved population NOTHING about the engine was \
         measured, so THIS RUN MUST NOT BE READ AS GREEN: it is not a pass, it is not gross \
         breakage, and it is not the TSK-010 verdict — the backstop was never evaluated. This is a \
         HARNESS failure. Check the priming write before the reader spawn rather than widening any \
         bound.",
        swaps.sorted.len(),
        MIN_REGIME_B_SAMPLES
    );

    assert!(
        swaps.p(0.50) < REGIME_B_P50_BACKSTOP_NS,
        "regime B p50 ({} ns) exceeded the gross-breakage backstop ({} ns = 10x the {} ns p50 the \
         bound was derived from): the engine is hung or has regressed by more than an order of \
         magnitude. This is NOT the TSK-010 verdict, and it is not a performance measurement.",
        swaps.p(0.50),
        REGIME_B_P50_BACKSTOP_NS,
        449
    );
}
