//! A latched comm-loss must be observable from **another process**.
//!
//! `QualityGate::record_comm_loss` used to latch `BAD_COMM_FAILURE` into a process-local
//! `HashMap` and stop there. The failure was real inside the process that noticed it and
//! invisible to every other consumer — the HMI, an alarm path, any reader of slot quality — no
//! matter what they were wired to do.
//!
//! # Why this test forks a process instead of asserting in one
//!
//! A same-process assertion cannot distinguish "published to shared memory" from "still in the
//! map" — the map is right there. This is the trap the SHM p99 slice fell into with `MemoryShm`,
//! where all 15 "reader" threads had private buffers and 0 of them ever saw the writer's data, so
//! the harness established neither a pass nor a breach.
//!
//! So the reader here is a genuinely separate OS process:
//!
//! * the parent creates a `DefaultShm` (POSIX) segment, primes a slot, and drives the gate;
//! * the parent re-executes **this test binary** with `MYTHOS_QUALITY_READER_SEGMENT` set, which
//!   runs [`child_reads_slot_quality`];
//! * the child attaches its own `ShmReaderHandle` by name and prints what it observes;
//! * the parent asserts on the child's stdout, and runs the same child twice — once before the
//!   latch and once after — so the test proves it can tell the two states apart. A child that
//!   printed the same thing both times would make the "after" assertion meaningless.

#![cfg(target_os = "linux")]

use mythos_pal::DefaultShm;
use mythos_shm::{QualityCode, ShmReaderHandle, ShmWriterHandle, TagFlags, TagValueRaw};
use mythos_strucpp::{DeviceExecutionMode, QualityGate, QualityVisibility, ShmQualityPublisher};
use std::collections::HashMap;
use std::process::Command;

/// Env var the parent sets to put the child into reader mode, carrying the segment name.
const READER_ENV: &str = "MYTHOS_QUALITY_READER_SEGMENT";
/// Env var carrying the slot the child must read.
const SLOT_ENV: &str = "MYTHOS_QUALITY_READER_SLOT";
/// Env var carrying the segment capacity, so parent and child agree on the layout.
const MAX_TAGS_ENV: &str = "MYTHOS_QUALITY_READER_MAX_TAGS";

const TAG_URI: &str = "external.PumpStation.P101.flow";
const SLOT_ID: u32 = 7;
const MAX_TAGS: usize = 64;
const TYPE_CODE_REAL: u16 = 4;

/// Child-only body. Runs in a separate process; the parent asserts on its stdout.
///
/// Without the env var this is the no-op it looks like: the real assertion lives in the parent,
/// and the parent fails loudly if this child does not produce a line it can parse.
#[test]
fn child_reads_slot_quality() {
    let Ok(segment) = std::env::var(READER_ENV) else {
        eprintln!("no {READER_ENV}; this test is a helper invoked by the parent test, skipping");
        return;
    };
    let slot: usize = std::env::var(SLOT_ENV).unwrap().parse().unwrap();
    let max_tags: usize = std::env::var(MAX_TAGS_ENV).unwrap().parse().unwrap();

    let mut reader = ShmReaderHandle::<DefaultShm>::new(&segment, max_tags)
        .expect("child could not attach to the segment the parent created");
    reader.update();
    let observed = reader
        .read_tag(slot)
        .expect("child could not read the slot");

    println!(
        "OBSERVED quality={} value={} type_code={}",
        observed.quality.code(),
        observed.value.as_f64(),
        observed.type_code
    );
}

/// Run the child and return its `OBSERVED ...` line.
fn observe_from_another_process(segment: &str) -> String {
    let exe = std::env::current_exe().expect("test binary path");
    let output = Command::new(exe)
        .args(["child_reads_slot_quality", "--exact", "--nocapture"])
        .env(READER_ENV, segment)
        .env(SLOT_ENV, SLOT_ID.to_string())
        .env(MAX_TAGS_ENV, MAX_TAGS.to_string())
        .output()
        .expect("failed to spawn the reader process");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|l| l.contains("OBSERVED "))
        .unwrap_or_else(|| {
            panic!(
                "the child process produced no observation line.\n\
                 status: {:?}\nstdout:\n{stdout}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .to_string();
    assert!(
        output.status.success(),
        "the child process failed: {:?}\nstdout:\n{stdout}",
        output.status
    );
    line
}

fn segment_name(suffix: &str) -> String {
    // Unique per test process so parallel binaries cannot collide on a segment.
    format!("mythos_qgate_{}_{}", std::process::id(), suffix)
}

/// Prime the slot with a real value and GOOD quality, and publish it.
fn prime_slot(writer: &mut ShmWriterHandle<DefaultShm>) {
    writer
        .write_tag(
            SLOT_ID as usize,
            TYPE_CODE_REAL,
            QualityCode::GOOD,
            TagValueRaw::from_f64(42.5),
            1_700_000_000_000_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();
}

#[test]
fn comm_loss_is_visible_to_a_different_process() {
    let name = segment_name("commloss");
    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    prime_slot(&mut writer);

    // Control: before the latch, another process sees the primed GOOD quality. Without this the
    // "after" assertion below would also pass if the child simply printed a constant.
    let before = observe_from_another_process(&name);
    assert!(
        before.contains(&format!("quality={}", QualityCode::GOOD.code())),
        "control failed: a fresh reader should see the primed GOOD quality, got: {before}"
    );

    let mut bindings = HashMap::new();
    bindings.insert(TAG_URI.to_string(), SLOT_ID);
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, bindings));

    let latch = gate.record_comm_loss(TAG_URI);
    assert_eq!(latch.quality, QualityCode::BAD_COMM_FAILURE);
    assert_eq!(
        latch.visibility,
        QualityVisibility::ShmPublished { slot_id: SLOT_ID },
        "the latch must report that it reached shared memory"
    );

    let after = observe_from_another_process(&name);
    assert!(
        after.contains(&format!("quality={}", QualityCode::BAD_COMM_FAILURE.code())),
        "a different process must see BAD_COMM_FAILURE after the latch, got: {after}"
    );

    // The value the slot already held is preserved: a quality event is not a data event.
    assert!(
        after.contains("value=42.5"),
        "publishing a quality change must not invent or drop the slot's value, got: {after}"
    );

    let _ = mythos_pal::PosixShm::unlink(&name);
}

#[test]
fn recovery_is_visible_to_a_different_process() {
    let name = segment_name("recovery");
    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    prime_slot(&mut writer);

    let mut bindings = HashMap::new();
    bindings.insert(TAG_URI.to_string(), SLOT_ID);
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, bindings));

    gate.record_comm_loss(TAG_URI);
    let bad = observe_from_another_process(&name);
    assert!(bad.contains(&format!("quality={}", QualityCode::BAD_COMM_FAILURE.code())));

    let cleared = gate.clear_comm_loss(TAG_URI);
    assert_eq!(cleared.quality, QualityCode::GOOD);
    assert!(cleared.visibility.is_cross_process());

    let after = observe_from_another_process(&name);
    assert!(
        after.contains(&format!("quality={}", QualityCode::GOOD.code())),
        "a recovery must reach other processes too, or readers stay bad forever, got: {after}"
    );

    let _ = mythos_pal::PosixShm::unlink(&name);
}

#[test]
fn without_a_publication_path_the_latch_says_it_is_process_local() {
    // No publisher configured: the old behaviour, but now it is *reported* rather than silent.
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware);
    let latch = gate.record_comm_loss(TAG_URI);

    assert_eq!(latch.quality, QualityCode::BAD_COMM_FAILURE);
    assert!(
        !latch.visibility.is_cross_process(),
        "with no shared-memory path the latch cannot claim cross-process visibility"
    );
    match &latch.visibility {
        QualityVisibility::ProcessLocalOnly { reason } => assert!(
            reason.contains("no shared-memory publication path"),
            "the reason must say why nothing else can see this: {reason}"
        ),
        other => panic!("expected ProcessLocalOnly, got {other:?}"),
    }
}

#[test]
fn an_unbound_tag_reports_that_it_could_not_be_published() {
    // Publisher present, but this tag has no slot: it must not silently pretend to publish.
    let name = segment_name("unbound");
    let writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, HashMap::new()));

    let latch = gate.record_comm_loss(TAG_URI);
    assert_eq!(latch.quality, QualityCode::BAD_COMM_FAILURE);
    match &latch.visibility {
        QualityVisibility::ProcessLocalOnly { reason } => assert!(
            reason.contains("not bound to a TagBus slot"),
            "the reason must name the cause: {reason}"
        ),
        other => panic!("expected ProcessLocalOnly, got {other:?}"),
    }

    let _ = mythos_pal::PosixShm::unlink(&name);
}
