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

/// Backstop for regime A (busy poll). Deliberately enormous: that population is dominated by how
/// long a spinning reader waits for the writer to publish, which is harness and scheduler
/// behaviour, not engine cost. It exists only to catch a hard hang or a catastrophic regression.
const BUSY_POLL_BACKSTOP_NS: u64 = 1_000_000;

fn percentile(sorted: &[u64], p: f64) -> u64 {
    debug_assert!(!sorted.is_empty());
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
    let _ = mythos_pal::DefaultScheduler::pin_to_core(READER_THREADS);
    let mut active_writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
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

    // Regime A is reported, never asserted here: it is dominated by how long a busy-polling reader
    // waits for the writer to publish, which is a property of the harness's polling pattern and of
    // host scheduling, not of the atomic operation. Gating on it would reproduce the flake this
    // sprint has been removing. Gross-breakage backstop only, and it says so when it fires.
    assert!(
        busy.p(0.99) < BUSY_POLL_BACKSTOP_NS,
        "regime A p99 ({} ns) exceeded the gross-breakage backstop ({} ns): the engine is \
         hung or catastrophically regressed. This is NOT the TSK-010 verdict.",
        busy.p(0.99),
        BUSY_POLL_BACKSTOP_NS
    );
}
