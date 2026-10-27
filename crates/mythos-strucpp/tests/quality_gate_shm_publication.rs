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
use mythos_strucpp::{
    ClearOutcome, DeviceExecutionMode, QualityGate, QualityVisibility, ShmQualityPublisher,
};
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
    match &cleared {
        ClearOutcome::Cleared {
            restored,
            visibility,
        } => {
            assert_eq!(
                *restored,
                Some(QualityCode::GOOD),
                "the slot was GOOD before the comm loss, so GOOD is what a clear restores"
            );
            assert!(visibility.is_cross_process());
        }
        other => panic!("expected a clear, got {other:?}"),
    }

    let after = observe_from_another_process(&name);
    assert!(
        after.contains(&format!("quality={}", QualityCode::GOOD.code())),
        "a recovery must reach other processes too, or readers stay bad forever, got: {after}"
    );

    let _ = mythos_pal::PosixShm::unlink(&name);
}

/// The close miss: a recovery for a tag this process never saw fail.
///
/// `clear_comm_loss` used to publish GOOD unconditionally, so this erased a real
/// BAD_SENSOR_FAILURE set by another path — cross-process — and replaced it with GOOD. A true BAD
/// destroyed is worse than the invented device #32 removed: the consumer was told a sensor was
/// fine when it was not.
#[test]
fn a_clear_with_no_latch_leaves_another_paths_failure_alone() {
    let name = segment_name("closemiss");
    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();

    // Another path reports a sensor failure on the same slot and publishes it.
    writer
        .write_tag(
            SLOT_ID as usize,
            TYPE_CODE_REAL,
            QualityCode::BAD_SENSOR_FAILURE,
            TagValueRaw::from_f64(42.5),
            1_700_000_000_000_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let before = observe_from_another_process(&name);
    assert!(
        before.contains(&format!(
            "quality={}",
            QualityCode::BAD_SENSOR_FAILURE.code()
        )),
        "control: the sensor failure must be visible before the clear, got: {before}"
    );

    let mut bindings = HashMap::new();
    bindings.insert(TAG_URI.to_string(), SLOT_ID);
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, bindings));

    // This gate never saw a comm loss for this tag.
    let outcome = gate.clear_comm_loss(TAG_URI);

    // The cross-process consequence is asserted first, because it is the harm: whatever the API
    // returns, what another process can see is the part that matters. Asserting the return value
    // first would let a passing API shape hide a slot that was rewritten anyway.
    let after = observe_from_another_process(&name);
    assert!(
        after.contains(&format!(
            "quality={}",
            QualityCode::BAD_SENSOR_FAILURE.code()
        )),
        "the other path's real sensor failure must survive a bogus recovery, got: {after}"
    );
    assert!(
        !after.contains(&format!("quality={}", QualityCode::GOOD.code())),
        "the bogus recovery must not have re-stamped GOOD: {after}"
    );

    assert_eq!(
        outcome,
        ClearOutcome::NothingToClear,
        "a clear with no latch must say so rather than claim a recovery"
    );

    let _ = mythos_pal::PosixShm::unlink(&name);
}

/// A comm loss on top of an existing failure, then a recovery: the underlying failure comes back.
///
/// Restoring a blanket GOOD here would report a failed sensor as healthy the moment the link
/// returned, which is a fabricated pass rather than a fabricated value.
#[test]
fn a_clear_restores_the_quality_that_preceded_the_comm_loss() {
    let name = segment_name("restore");
    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();

    writer
        .write_tag(
            SLOT_ID as usize,
            TYPE_CODE_REAL,
            QualityCode::BAD_SENSOR_FAILURE,
            TagValueRaw::from_f64(42.5),
            1_700_000_000_000_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let mut bindings = HashMap::new();
    bindings.insert(TAG_URI.to_string(), SLOT_ID);
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, bindings));

    // The link drops on a tag whose sensor was already failing.
    gate.record_comm_loss(TAG_URI);
    let during = observe_from_another_process(&name);
    assert!(
        during.contains(&format!("quality={}", QualityCode::BAD_COMM_FAILURE.code())),
        "the comm loss must be visible, got: {during}"
    );

    let outcome = gate.clear_comm_loss(TAG_URI);
    match outcome {
        ClearOutcome::Cleared {
            restored,
            visibility,
        } => {
            assert_eq!(
                restored,
                Some(QualityCode::BAD_SENSOR_FAILURE),
                "the sensor failure that was live underneath must be restored, not GOOD"
            );
            assert!(visibility.is_cross_process());
        }
        other => panic!("expected a clear, got {other:?}"),
    }

    let after = observe_from_another_process(&name);
    assert!(
        after.contains(&format!(
            "quality={}",
            QualityCode::BAD_SENSOR_FAILURE.code()
        )),
        "a recovered link over a failed sensor is still a failed sensor, got: {after}"
    );

    let _ = mythos_pal::PosixShm::unlink(&name);
}

/// A repeated comm loss must not overwrite what a clear has to restore.
#[test]
fn a_second_comm_loss_does_not_forget_the_original_quality() {
    let name = segment_name("repeat");
    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    writer
        .write_tag(
            SLOT_ID as usize,
            TYPE_CODE_REAL,
            QualityCode::UNCERTAIN_SUBSTITUTE,
            TagValueRaw::from_f64(1.0),
            1_700_000_000_000_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let mut bindings = HashMap::new();
    bindings.insert(TAG_URI.to_string(), SLOT_ID);
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, bindings));

    gate.record_comm_loss(TAG_URI);
    gate.record_comm_loss(TAG_URI); // flapped twice before recovering

    match gate.clear_comm_loss(TAG_URI) {
        ClearOutcome::Cleared { restored, .. } => assert_eq!(
            restored,
            Some(QualityCode::UNCERTAIN_SUBSTITUTE),
            "the second latch must not have replaced the quality a clear restores"
        ),
        other => panic!("expected a clear, got {other:?}"),
    }

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

/// Compare-and-restore: a clear must undo only its own overwrite.
///
/// The restore in `clear_comm_loss` was unconditional, so if another path published a real
/// measurement while the latch was held, the clear reverted that measurement's quality. Reachable,
/// not hypothetical: `TagSynchronizer::sync_outputs` rewrites slot quality on every scan.
///
/// The replay that found this (pod_backend.reviewer, #43 re-review):
///
/// ```text
/// 1) code=68 (UNCERTAIN_SUBSTITUTE) value=42.5
/// 2) code=24 (BAD_COMM_FAILURE)     value=42.5   <- the latch
/// 3) code=192 (GOOD)                value=77.25  <- another path's real measurement
/// 4) code=68 (UNCERTAIN_SUBSTITUTE) value=77.25  <- unconditional restore
/// ```
///
/// Step 4 labels a measured value as a substitute — the inverse of the lie this slice removes.
#[test]
fn a_clear_leaves_quality_another_path_published_after_the_latch() {
    let name = segment_name("takenover");
    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();

    // 1) The slot starts as a substitute value.
    writer
        .write_tag(
            SLOT_ID as usize,
            TYPE_CODE_REAL,
            QualityCode::UNCERTAIN_SUBSTITUTE,
            TagValueRaw::from_f64(42.5),
            1_700_000_000_000_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let mut bindings = HashMap::new();
    bindings.insert(TAG_URI.to_string(), SLOT_ID);
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, bindings));

    // 2) The link drops; the gate latches BAD_COMM_FAILURE.
    gate.record_comm_loss(TAG_URI);
    let latched = observe_from_another_process(&name);
    assert!(
        latched.contains(&format!("quality={}", QualityCode::BAD_COMM_FAILURE.code())),
        "control: the latch must be visible before the third path writes, got: {latched}"
    );

    // 3) Another path publishes a real measurement on the same slot while the latch is held.
    //
    // NOTE ON THE SECOND WRITER HANDLE: the TagBus assumes ONE writer per segment, and a second
    // handle whose bank state has fallen behind trips `Producer write bank mismatch`
    // (buffer.rs:70, an implementation assertion the reviewer hit in their own harness). This
    // sequence is safe only because of its ordering: `other` is opened *after* the latch published,
    // so it starts from the current write bank, and the gate never publishes again once `other` has
    // written. Do not reorder this as "gate latch, other publish, gate clear publishes, other
    // publishes again" — that trips the assertion. The real caller must share one handle between
    // the scan and the gate; see the follow-up row for that constraint.
    //
    // This is also why the negative control for this test fails on `buffer.rs:70` rather than on the
    // assertion below: with the restore unconditional, the clear republishes after `other` moved the
    // bank, so the engine's own assertion fires first. The test is still a valid detector under
    // either failure mode — an engine assertion today, a reverted quality if that assertion ever
    // becomes a real mismatch — but the RED reason is the engine's, and saying otherwise would be
    // describing a control I did not observe.
    let mut other = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    other
        .write_tag(
            SLOT_ID as usize,
            TYPE_CODE_REAL,
            QualityCode::GOOD,
            TagValueRaw::from_f64(77.25),
            1_700_000_000_001_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    other.publish();

    let taken = observe_from_another_process(&name);
    assert!(
        taken.contains(&format!("quality={}", QualityCode::GOOD.code())),
        "control: the third path's GOOD measurement must be visible, got: {taken}"
    );

    // 4) The link recovers. The clear must NOT overwrite the third path's quality.
    let outcome = gate.clear_comm_loss(TAG_URI);

    let after = observe_from_another_process(&name);
    assert!(
        after.contains(&format!("quality={}", QualityCode::GOOD.code())),
        "a clear must not revert a quality another path published after the latch, got: {after}"
    );
    assert!(
        after.contains("value=77.25"),
        "and must not disturb the measured value either, got: {after}"
    );
    match &outcome {
        ClearOutcome::Cleared {
            restored,
            visibility,
        } => {
            assert_eq!(
                *restored, None,
                "nothing was restored, and the outcome must say so rather than claim a restore"
            );
            assert!(
                !visibility.is_cross_process(),
                "no write happened, so the visibility must not claim one"
            );
            match visibility {
                QualityVisibility::ProcessLocalOnly { reason } => assert!(
                    reason.contains("another path owns the quality"),
                    "the reason must name the cause: {reason}"
                ),
                other => panic!("expected ProcessLocalOnly, got {other:?}"),
            }
        }
        other => panic!("expected a clear, got {other:?}"),
    }

    let _ = mythos_pal::PosixShm::unlink(&name);
}

/// The compare half: while the slot still carries this gate's own word, the restore still fires.
#[test]
fn a_clear_still_restores_when_the_gate_still_owns_the_word() {
    let name = segment_name("stillowns");
    let mut writer = ShmWriterHandle::<DefaultShm>::new(&name, MAX_TAGS).unwrap();
    writer
        .write_tag(
            SLOT_ID as usize,
            TYPE_CODE_REAL,
            QualityCode::BAD_SENSOR_FAILURE,
            TagValueRaw::from_f64(42.5),
            1_700_000_000_000_000,
            TagFlags::ACCESS_RO,
        )
        .unwrap();
    writer.publish();

    let mut bindings = HashMap::new();
    bindings.insert(TAG_URI.to_string(), SLOT_ID);
    let gate = QualityGate::new(DeviceExecutionMode::PhysicalHardware)
        .with_shm_publication(ShmQualityPublisher::new(writer, bindings));

    gate.record_comm_loss(TAG_URI);
    match gate.clear_comm_loss(TAG_URI) {
        ClearOutcome::Cleared {
            restored,
            visibility,
        } => {
            assert_eq!(
                restored,
                Some(QualityCode::BAD_SENSOR_FAILURE),
                "nobody else touched the slot, so the restore must still happen"
            );
            assert!(visibility.is_cross_process());
        }
        other => panic!("expected a clear, got {other:?}"),
    }

    let after = observe_from_another_process(&name);
    assert!(
        after.contains(&format!(
            "quality={}",
            QualityCode::BAD_SENSOR_FAILURE.code()
        )),
        "got: {after}"
    );

    let _ = mythos_pal::PosixShm::unlink(&name);
}
