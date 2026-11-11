//! `DeviceExecutionMode` has exactly one definition, and it is `mythos_core`'s.
//!
//! This crate used to declare its own `DeviceExecutionMode` next to `mythos_core`'s. The two had
//! identical variants, so every Rust-level comparison succeeded — and the wire forms disagreed,
//! because core's enum carries `#[serde(rename_all = "snake_case")]` and this crate's did not.
//! A value serialised on one side could not be read on the other, and nothing noticed because
//! nothing asserted the wire form at all.
//!
//! These tests fail if the duplicate comes back, if the wire form drifts, or if `Default` stops
//! being the safe mode. They deliberately go through the `mythos_strucpp::quality_gate` path that
//! existing consumers use, not through `mythos_core` directly: a test that only exercises core
//! would stay green while this crate grew a second definition again.

use mythos_strucpp::quality_gate::{DeviceExecutionMode, QualityGate};

/// Compile-time proof that the re-exported path and the owning crate denote one type.
///
/// If `mythos_strucpp` ever re-introduces its own enum — or a wrapper, newtype or alias standing
/// in for one — the two paths stop being interchangeable and this coercion stops compiling. That
/// is a stronger statement than "the variants match", which is exactly what the old duplicate
/// satisfied.
const _: fn(mythos_core::DeviceExecutionMode) -> DeviceExecutionMode = std::convert::identity;

/// The wire form of both variants, pinned as strings.
///
/// `snake_case` is the whole point of adopting core's definition: `"PhysicalHardware"` and
/// `"physical_hardware"` were the two forms in circulation, and only one of them can be the
/// contract. Pinning both variants means a rename or a dropped `rename_all` goes red here rather
/// than at a process boundary in the field.
#[test]
fn wire_form_is_snake_case_for_both_variants() {
    assert_eq!(
        serde_json::to_string(&DeviceExecutionMode::PhysicalHardware).unwrap(),
        "\"physical_hardware\"",
        "PhysicalHardware must serialise as snake_case"
    );
    assert_eq!(
        serde_json::to_string(&DeviceExecutionMode::ManualSimulation).unwrap(),
        "\"manual_simulation\"",
        "ManualSimulation must serialise as snake_case"
    );
}

/// The wire form is readable back through this crate's path, in both directions.
///
/// The original defect was asymmetric: core wrote `"physical_hardware"` and this crate expected
/// `"PhysicalHardware"`. Round-tripping through the re-export catches a divergence in either
/// direction, which a one-way serialisation assertion would not.
#[test]
fn wire_form_round_trips_through_the_strucpp_path() {
    for mode in [
        DeviceExecutionMode::PhysicalHardware,
        DeviceExecutionMode::ManualSimulation,
    ] {
        let json = serde_json::to_string(&mode).unwrap();
        let back: DeviceExecutionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode, "round trip changed the value: {json}");
    }

    // A core-written value is readable here with no adapter — this is the cross-crate case the
    // duplicate broke.
    let from_core =
        serde_json::to_string(&mythos_core::DeviceExecutionMode::ManualSimulation).unwrap();
    let read_here: DeviceExecutionMode = serde_json::from_str(&from_core).unwrap();
    assert_eq!(read_here, DeviceExecutionMode::ManualSimulation);
}

/// One type: a `mythos_core` value goes into `QualityGate` with no conversion.
///
/// This is the condition a shim would fake. An alias or a `From` impl would let this line be
/// written with a conversion call in it; here there is nothing between the core value and the
/// gate's constructor, so the gate's field and core's enum are the same type by construction.
#[test]
fn core_value_enters_quality_gate_with_no_conversion() {
    let core_mode = mythos_core::DeviceExecutionMode::ManualSimulation;
    let gate = QualityGate::new(core_mode);

    // Compared against a value built from the core path, not this crate's.
    assert_eq!(
        gate.current_mode(),
        mythos_core::DeviceExecutionMode::ManualSimulation
    );
    assert!(
        gate.is_simulation(),
        "gate must see core's ManualSimulation as simulation"
    );

    // And the reverse direction: set through this crate's path, read as a core value.
    gate.set_mode(mythos_core::DeviceExecutionMode::PhysicalHardware);
    assert_eq!(gate.current_mode(), DeviceExecutionMode::PhysicalHardware);
    assert!(!gate.is_simulation());
}

/// The default is the safe mode, and it is core's default that supplies it.
///
/// Safety-relevant state must not default to the permissive branch. Since this crate no longer
/// defines the type, `Default` now comes from `mythos_core` — this asserts that adopting it did
/// not quietly change which mode a fresh gate lands in.
#[test]
fn default_remains_physical_hardware() {
    assert_eq!(
        DeviceExecutionMode::default(),
        DeviceExecutionMode::PhysicalHardware
    );
    assert_eq!(
        mythos_core::DeviceExecutionMode::default(),
        DeviceExecutionMode::PhysicalHardware
    );
    assert!(
        !DeviceExecutionMode::default().eq(&DeviceExecutionMode::ManualSimulation),
        "defaulting to simulation would unlock forcing on an unconfigured gate"
    );
}
