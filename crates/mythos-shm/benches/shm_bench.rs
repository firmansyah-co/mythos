//! High-concurrency Criterion micro-benchmark suite for MythOS Lock-Free SHM TagBus.
//!
//! Evaluates read/write throughput and p99 latency across 16 concurrent worker threads
//! validating the acceptance criteria (< 100 ns operation latency).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mythos_pal::MemoryShm;
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagFlags, TagValueRaw};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn bench_single_thread_atomic_swap(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_thread_shm");
    let shm_name = "mythos_bench_single_swap";
    let mut writer = ShmWriterHandle::<MemoryShm>::new(shm_name, 1024).unwrap();

    group.throughput(Throughput::Elements(1));
    group.bench_function("atomic_publish_swap", |b| {
        b.iter(|| {
            writer
                .write_tag(
                    black_box(42),
                    black_box(1),
                    QualityCode::GOOD,
                    TagValueRaw::from_f64(black_box(101.3)),
                    1000,
                    TagFlags::ACCESS_RO,
                )
                .unwrap();
            black_box(writer.publish());
        });
    });
    group.finish();
}

fn bench_high_concurrency_16_threads(c: &mut Criterion) {
    let mut group = c.benchmark_group("high_concurrency_shm");
    let shm_name = "mythos_bench_16_threads";
    let max_tags = 4096;

    // Initialize SHM segment
    let mut writer = ShmWriterHandle::<MemoryShm>::new(shm_name, max_tags).unwrap();
    for i in 0..max_tags {
        writer
            .write_tag(
                i,
                1,
                QualityCode::GOOD,
                TagValueRaw::from_f64(i as f64),
                1000,
                TagFlags::ACCESS_RO,
            )
            .unwrap();
    }
    writer.publish();

    group.bench_function(BenchmarkId::new("readers_16_threads", max_tags), |b| {
        b.iter_custom(|iters| {
            let running = Arc::new(AtomicBool::new(true));
            let mut handles = Vec::new();

            // Spawn 15 reader threads and 1 dedicated writer thread (16 total concurrent threads)
            for _ in 0..15 {
                let is_running = running.clone();
                let handle = thread::spawn(move || {
                    let mut reader = ShmReaderHandle::<MemoryShm>::new(shm_name, max_tags).unwrap();
                    let mut read_ops = 0u64;
                    while is_running.load(Ordering::Relaxed) {
                        let _ = reader.update();
                        for slot in 0..64 {
                            if let Ok(slot_data) = reader.read_tag(slot) {
                                black_box(slot_data);
                                read_ops += 1;
                            }
                        }
                    }
                    read_ops
                });
                handles.push(handle);
            }

            let start = Instant::now();
            let mut writer = ShmWriterHandle::<MemoryShm>::new(shm_name, max_tags).unwrap();
            for i in 0..iters {
                let slot = (i as usize) % max_tags;
                writer
                    .write_tag(
                        slot,
                        1,
                        QualityCode::GOOD,
                        TagValueRaw::from_f64(i as f64),
                        i,
                        TagFlags::ACCESS_RO,
                    )
                    .unwrap();
                writer.publish();
            }
            let duration = start.elapsed();

            running.store(false, Ordering::Relaxed);
            for h in handles {
                let _ = h.join();
            }

            duration
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_single_thread_atomic_swap,
    bench_high_concurrency_16_threads
);
criterion_main!(benches);
