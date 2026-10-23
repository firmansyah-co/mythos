//! TSK-010: real per-operation p99 latency under 16 concurrent threads.
//!
//! `TSK-010`'s acceptance criterion reads: *"Criterion benchmarks pass with < 100 ns p99
//! read/write operations under 16 concurrent threads."* Until this file existed **nothing in the
//! repository computed a p99.** The only related code, `readers_16_threads` in
//! `benches/shm_bench.rs`, measured `start.elapsed()` for a whole writer loop and reported it as
//! if it were a latency percentile; `ci.yml` additionally ran `cargo bench --no-run`, so even that
//! was compiled and never executed. The criterion had therefore never been measured at all.
//!
//! This gate measures it: one writer thread and 15 reader threads on a real shared-memory
//! segment (16 concurrent threads, as specified), per-operation wall-clock latency, sorted and
//! indexed.
//!
//! # Measured results on this host (16 cores, i9-11980HK, shared with the CI runner and the
//! agent fleet — see `shm_engine_tests.rs` for that fact)
//!
//! 10 release-mode runs, per-operation p99:
//!
//! | operation | p99 range | verdict against the 100 ns target |
//! |-----------|-----------|-----------------------------------|
//! | read  (`update` + `read_tag`) | 36-47 ns (78 ns with 16 extra busy cores) | **met**, with margin |
//! | write (`write_tag` + `publish`) | 68-135 ns | **NOT reliably met** — breached in 4 of 10 runs |
//!
//! This is a finding, not a threshold to adjust. The write path pays for CAS contention against
//! 15 concurrent readers on the triple-buffer state word, which is exactly the scenario the
//! criterion describes, so the measurement is fair. What it shows is that the **write** half of
//! TSK-010's criterion cannot be certified on a host that is also running a build fleet.
//!
//! # Gate design, and why the write bound is not 100 ns
//!
//! - **Reads are gated at the specified 100 ns.** They are stable there (36-78 ns across every
//!   run taken), so this is the real acceptance assertion.
//! - **Writes are gated at a regression bound, not at the spec target.** Gating writes at 100 ns
//!   would produce exactly the flaky required check that `fix/shm-latency-regime` just removed —
//!   it failed 4 times in 10 correct runs. The bound below is set from measured data with headroom
//!   for a loaded runner, and it exists to catch a genuine regression (a lock, an allocation, an
//!   extra synchronisation point), not to certify the spec number.
//!   **This is a spec gap reported upward, not a threshold quietly softened.** If a future change
//!   makes writes pass 100 ns reliably, tighten this to 100 — and say so in the PR.
//!
//! The measurement is release-only: an unoptimised CAS loop measures ~3x slower and would not
//! measure the algorithm (same reasoning as `shm_engine_tests.rs`).

#![cfg(not(debug_assertions))]

use mythos_pal::MemoryShm;
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagFlags, TagValueRaw};
use std::thread;
use std::time::Instant;

const MAX_TAGS: usize = 4096;
const READER_THREADS: usize = 15;
const SAMPLES_PER_THREAD: usize = 20_000;

/// TSK-010's stated target for read and write.
const SPEC_P99_NS: u64 = 100;

/// Regression bound for writes, derived from measurement (see the header). Not the spec target.
const WRITE_P99_REGRESSION_NS: u64 = 200;

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
            "{} latency under {} concurrent threads (n={} ops): p50={} p90={} p99={} max={} ns",
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
fn tsk010_p99_read_write_under_16_concurrent_threads() {
    let name = "mythos_tsk010_p99_gate";
    let mut writer = ShmWriterHandle::<MemoryShm>::new(name, MAX_TAGS).unwrap();
    for i in 0..MAX_TAGS {
        writer
            .write_tag(
                i,
                1,
                QualityCode::GOOD,
                TagValueRaw::from_f64(i as f64),
                i as u64,
                TagFlags::ACCESS_RO,
            )
            .unwrap();
    }
    writer.publish();

    // 15 reader threads + the writer below = 16 concurrent threads, as the criterion specifies.
    let mut handles = Vec::new();
    for _ in 0..READER_THREADS {
        handles.push(thread::spawn(move || {
            let mut reader = ShmReaderHandle::<MemoryShm>::new(name, MAX_TAGS).unwrap();
            let mut latencies = Vec::with_capacity(SAMPLES_PER_THREAD);
            let slot = 50usize;
            while latencies.len() < SAMPLES_PER_THREAD {
                let start = Instant::now();
                reader.update();
                let result = reader.read_tag(slot);
                latencies.push(start.elapsed().as_nanos() as u64);
                std::hint::black_box(&result);
            }
            latencies
        }));
    }

    let mut writer_latencies = Vec::with_capacity(SAMPLES_PER_THREAD);
    let mut active_writer = ShmWriterHandle::<MemoryShm>::new(name, MAX_TAGS).unwrap();
    for i in 0..SAMPLES_PER_THREAD {
        let slot = i % MAX_TAGS;
        let start = Instant::now();
        active_writer
            .write_tag(
                slot,
                1,
                QualityCode::GOOD,
                TagValueRaw::from_f64(i as f64),
                i as u64,
                TagFlags::ACCESS_RO,
            )
            .unwrap();
        active_writer.publish();
        writer_latencies.push(start.elapsed().as_nanos() as u64);
    }

    let mut read_latencies = Vec::with_capacity(READER_THREADS * SAMPLES_PER_THREAD);
    for handle in handles {
        read_latencies.extend(handle.join().expect("reader thread panicked"));
    }

    let reads = Distribution::new("READ", read_latencies);
    let writes = Distribution::new("WRITE", writer_latencies);
    reads.report();
    writes.report();

    // The real acceptance assertion: TSK-010's stated target, on the operation where it holds.
    assert!(
        reads.p(0.99) < SPEC_P99_NS,
        "TSK-010: read p99 is {} ns, over the {} ns target. This is a release build under 16 \
         concurrent threads, i.e. the criterion's own regime.",
        reads.p(0.99),
        SPEC_P99_NS
    );

    // Writes: regression gate with a measured bound, NOT the spec target. See the header.
    assert!(
        writes.p(0.99) < WRITE_P99_REGRESSION_NS,
        "write p99 is {} ns, over the {} ns regression bound. Optimised writes measure 68-135 ns \
         on this host, so this indicates a real regression (a lock, an allocation, an added \
         synchronisation point). Note the {} ns TSK-010 target is separate and is reported above, \
         not asserted here - see the header of this file for why.",
        writes.p(0.99),
        WRITE_P99_REGRESSION_NS,
        SPEC_P99_NS
    );
}
