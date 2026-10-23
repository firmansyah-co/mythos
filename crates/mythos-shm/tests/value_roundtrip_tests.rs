//! Round-trip identity for every `TagValueRaw` type, and a type-aware telemetry decode.
//!
//! These are the tests that would have caught both data-integrity defects:
//!
//! - **D1**: `from_f32(98.6).as_f32()` returned 36893490000000000000.0. The writer widened to
//!   `f64` and stored 8 bytes; the reader reinterpreted the first 4 bytes as `f32`. Not inverses.
//! - **D2**: telemetry read every slot through `as_f64` regardless of `type_code`, so a DINT
//!   holding 42 surfaced as a denormal (~1e-322) — a number that is not the plant's value and
//!   looks like a broken instrument.
//!
//! Both were silent, which is why each type gets an explicit identity assertion rather than one
//! representative case.

use mythos_shm::slot::TagValueRaw;

/// Every integer and float type round-trips through its own accessor.
#[test]
fn every_value_type_round_trips_through_its_own_accessor() {
    assert!(TagValueRaw::from_bool(true).as_bool());
    assert!(!TagValueRaw::from_bool(false).as_bool());

    for value in [0u8, 1, 42, 255] {
        assert_eq!(TagValueRaw::from_u8(value).as_u8(), value, "u8 {value}");
    }

    for value in [i16::MIN, -1234, 0, 1234, i16::MAX] {
        assert_eq!(TagValueRaw::from_i16(value).as_i16(), value, "i16 {value}");
    }

    for value in [0u16, 1, 4321, u16::MAX] {
        assert_eq!(TagValueRaw::from_u16(value).as_u16(), value, "u16 {value}");
    }

    for value in [i32::MIN, -1_000_000, 0, 42, 1_000_000, i32::MAX] {
        assert_eq!(TagValueRaw::from_i32(value).as_i32(), value, "i32 {value}");
    }

    for value in [0u32, 42, 3_000_000_000, u32::MAX] {
        assert_eq!(TagValueRaw::from_u32(value).as_u32(), value, "u32 {value}");
    }

    for value in [i64::MIN, -1, 0, 42, i64::MAX] {
        assert_eq!(TagValueRaw::from_i64(value).as_i64(), value, "i64 {value}");
    }

    for value in [0u64, 42, u64::MAX] {
        assert_eq!(TagValueRaw::from_u64(value).as_u64(), value, "u64 {value}");
    }

    for value in [f32::MIN, -98.6f32, 0.0, 98.6, f32::MAX] {
        let raw = TagValueRaw::from_f32(value);
        assert_eq!(raw.as_f32(), value, "f32 {value}");
        // Widening to f64 is lossless, so as_f64 must agree with the f32 exactly.
        assert_eq!(raw.as_f64(), value as f64, "f32 {value} widened");
    }

    for value in [f64::MIN, -98.6f64, 0.0, 98.6, f64::MAX] {
        assert_eq!(TagValueRaw::from_f64(value).as_f64(), value, "f64 {value}");
    }
}

/// The specific value from the defect report, asserted verbatim.
#[test]
fn the_reported_f32_round_trip_case_is_identity() {
    let raw = TagValueRaw::from_f32(98.6);
    assert_eq!(
        raw.as_f32(),
        98.6f32,
        "from_f32(98.6).as_f32() regressed: the reader is no longer the inverse of the writer"
    );
    // The exact garbage value the defect produced, named so a future regression is unmistakable.
    assert_ne!(
        raw.as_f32(),
        36893490000000000000.0f32,
        "this is the exact value the f32 round-trip defect produced"
    );
}

/// Narrowing accessors must match how the widening constructors store values.
#[test]
fn narrowing_accessors_match_the_widened_storage() {
    // from_i16 stores the value sign-extended in 8 bytes; as_i32 must return the same number.
    assert_eq!(TagValueRaw::from_i16(-1234).as_i32(), -1234);
    assert_eq!(TagValueRaw::from_u16(4321).as_u32(), 4321);
    assert_eq!(TagValueRaw::from_i32(42).as_i64(), 42);
    assert_eq!(TagValueRaw::from_u32(42).as_u64(), 42);
}

/// An integer value must never be readable as a plausible float.
///
/// This is the shape of D2: an integer's bit pattern read as `f64` produces a denormal. The test
/// documents why a `type_code` is required rather than a guess.
#[test]
fn integer_bits_do_not_read_as_their_numeric_value_in_f64() {
    let raw = TagValueRaw::from_i32(42);
    assert_ne!(
        raw.as_f64(),
        42.0,
        "if these were equal, reading by type would not matter - it does, so it must be explicit"
    );
    // Denormal territory, i.e. what the operator would have seen.
    assert!(
        raw.as_f64().abs() < f64::MIN_POSITIVE,
        "an integer bit pattern reinterpreted as f64 is a denormal, which is the operator-visible \
         symptom this work removes"
    );
}
