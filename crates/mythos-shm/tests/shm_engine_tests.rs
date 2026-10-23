use mythos_pal::MemoryShm;
use mythos_shm::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

#[test]
fn test_tag_slot_alignment_and_size() {
    assert_eq!(std::mem::size_of::<TagSlot>(), 64);
    assert_eq!(std::mem::align_of::<TagSlot>(), 64);
}

#[test]
fn test_superblock_alignment_and_size() {
    assert_eq!(std::mem::size_of::<ShmSuperblock>(), 4096);
    assert_eq!(std::mem::align_of::<ShmSuperblock>(), 64);
}

#[test]
fn test_triple_buffer_state_transitions() {
    let tb = TripleBufferState::new();
    let (w, c, r, has_new, gen) = TripleBufferState::unpack(tb.load_raw(Ordering::SeqCst));
    assert_eq!(w, 0);
    assert_eq!(c, 1);
    assert_eq!(r, 2);
    assert!(!has_new);
    assert_eq!(gen, 0);

    // Write cycle 1
    let next_w = tb.publish_write(w);
    assert_eq!(next_w, 1);
    let (w1, c1, r1, has_new1, gen1) = TripleBufferState::unpack(tb.load_raw(Ordering::SeqCst));
    assert_eq!(w1, 1);
    assert_eq!(c1, 0);
    assert_eq!(r1, 2);
    assert!(has_new1);
    assert_eq!(gen1, 1);

    // Read cycle 1
    let (read_bank, fresh) = tb.acquire_read();
    assert!(fresh);
    assert_eq!(read_bank, 0);
    let (w2, c2, r2, has_new2, gen2) = TripleBufferState::unpack(tb.load_raw(Ordering::SeqCst));
    assert_eq!(w2, 1);
    assert_eq!(c2, 2);
    assert_eq!(r2, 0);
    assert!(!has_new2);
    assert_eq!(gen2, 1);

    // Read cycle 2 without new write
    let (read_bank_again, fresh2) = tb.acquire_read();
    assert!(!fresh2);
    assert_eq!(read_bank_again, 0);
}

#[test]
fn test_engine_single_threaded_read_write() {
    let mut engine = TagBusEngine::<MemoryShm>::open_or_create("test_shm_single", 100).unwrap();
    assert_eq!(engine.max_tags(), 100);

    let val = TagValueRaw::from_f64(42.5);
    engine
        .write_tag(
            5,
            4, // REAL
            QualityCode::GOOD,
            val,
            1_700_000_000_000_000,
            TagFlags::ACCESS_RW | TagFlags::RETENTION_PERSISTENT,
        )
        .unwrap();

    engine.publish_cycle();

    let slot = engine.read_tag(5).unwrap();
    assert_eq!(slot.tag_id, 5);
    assert_eq!(slot.type_code, 4);
    assert_eq!(slot.quality, QualityCode::GOOD);
    assert_eq!(slot.value.as_f64(), 42.5);
    assert_eq!(slot.timestamp_ns, 1_700_000_000_000_000);
    assert_eq!(slot.sequence, 1);
}

#[test]
fn test_engine_32768_tag_slots() {
    let mut engine = TagBusEngine::<MemoryShm>::open_or_create("test_shm_32k", 32_768).unwrap();
    assert_eq!(engine.max_tags(), 32_768);

    // Verify first, middle, last slot
    engine
        .write_tag(
            0,
            1,
            QualityCode::GOOD,
            TagValueRaw::from_bool(true),
            10,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    engine
        .write_tag(
            16_384,
            2,
            QualityCode::GOOD,
            TagValueRaw::from_i32(12345),
            20,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    engine
        .write_tag(
            32_767,
            3,
            QualityCode::GOOD,
            TagValueRaw::from_f64(999.99),
            30,
            TagFlags::ACCESS_RO,
        )
        .unwrap();

    // Bounds check
    assert!(engine
        .write_tag(
            32_768,
            1,
            QualityCode::GOOD,
            TagValueRaw::from_bool(false),
            0,
            0
        )
        .is_err());

    engine.publish_cycle();

    assert!(engine.read_tag(0).unwrap().value.as_bool());
    assert_eq!(engine.read_tag(16_384).unwrap().value.as_i32(), 12345);
    assert_eq!(engine.read_tag(32_767).unwrap().value.as_f64(), 999.99);
}

#[test]
fn test_concurrent_writer_and_readers() {
    // Shared Memory segment for multi-threaded test
    let engine = Arc::new(std::sync::Mutex::new(
        TagBusEngine::<MemoryShm>::open_or_create("test_shm_concurrent", 1024).unwrap(),
    ));

    let engine_writer = Arc::clone(&engine);
    let writer_thread = thread::spawn(move || {
        for cycle in 1..=500 {
            {
                let mut eng = engine_writer.lock().unwrap();
                for i in 0..100 {
                    eng.write_tag(
                        i,
                        3,
                        QualityCode::GOOD,
                        TagValueRaw::from_i32(cycle * 1000 + i as i32),
                        cycle as u64,
                        TagFlags::ACCESS_RW,
                    )
                    .unwrap();
                }
                eng.publish_cycle();
            }
            thread::yield_now();
        }
    });

    let mut reader_handles = vec![];
    for _ in 0..4 {
        let engine_reader = Arc::clone(&engine);
        reader_handles.push(thread::spawn(move || {
            let mut read_count = 0;
            while read_count < 200 {
                let slot = {
                    let eng = engine_reader.lock().unwrap();
                    eng.read_tag(50).unwrap()
                };
                if slot.sequence > 0 {
                    let val = slot.value.as_i32();
                    // Must be consistent: cycle * 1000 + 50
                    assert_eq!(val % 1000, 50);
                    read_count += 1;
                }
                thread::yield_now();
            }
        }));
    }

    writer_thread.join().unwrap();
    for h in reader_handles {
        h.join().unwrap();
    }
}

#[test]
fn test_atomic_swap_latency_under_50ns() {
    let tb = TripleBufferState::new();
    let mut write_bank = 0;

    // Warm-up
    for _ in 0..1_000 {
        write_bank = tb.publish_write(write_bank);
        let _ = tb.acquire_read();
    }

    let iterations = 100_000;
    let start = Instant::now();
    for _ in 0..iterations {
        write_bank = tb.publish_write(write_bank);
        let _ = tb.acquire_read();
    }
    let elapsed = start.elapsed();
    let total_operations = iterations * 2;
    let ns_per_op = elapsed.as_nanos() as f64 / total_operations as f64;

    println!("Average atomic swap operation latency: {:.2} ns", ns_per_op);
    assert!(
        ns_per_op < 50.0,
        "Atomic swap operation latency ({:.2} ns) exceeded 50 ns threshold!",
        ns_per_op
    );
}
