//! A controller keeps the bytecode it was given (a project's `runtimes.json`) across an
//! update of its VM. A VM reads every format it extends (ADR-013, "Format versions"), so a
//! program compiled before the update runs after it, scan for scan, as the same source
//! compiled now.
//!
//! `tests/fixtures/format1/` holds the six programs of `tests/differential/` as the format-1
//! compiler made them (`origin/dev` at f4637cd); its README says how. They are committed bytes,
//! never regenerated: regenerated, they would be what this compiler makes, and prove nothing.

use mythos_logic_compiler::{compile, Compiled, TargetProfile};
use mythos_logic_vm::{cold_start, scan, write, Frame, Program, Value, FORMAT_VERSION};

const CYCLES: i64 = 200;
const CYCLE_NS: i64 = 20_000_000;
const BUDGET: u32 = 1_000_000;

/// (name, bytes the format-1 compiler made, the source it made them from)
const FORMAT_1: &[(&str, &[u8], &str)] = &[
    (
        "functions_and_blocks",
        include_bytes!("fixtures/format1/functions_and_blocks.mlbc"),
        include_str!("differential/functions_and_blocks.st"),
    ),
    (
        "integers_and_control",
        include_bytes!("fixtures/format1/integers_and_control.mlbc"),
        include_str!("differential/integers_and_control.st"),
    ),
    (
        "reals_and_conversions",
        include_bytes!("fixtures/format1/reals_and_conversions.mlbc"),
        include_str!("differential/reals_and_conversions.st"),
    ),
    (
        "small_plant",
        include_bytes!("fixtures/format1/small_plant.mlbc"),
        include_str!("differential/small_plant.st"),
    ),
    (
        "structures_and_arrays",
        include_bytes!("fixtures/format1/structures_and_arrays.mlbc"),
        include_str!("differential/structures_and_arrays.st"),
    ),
    (
        "timers_and_counters",
        include_bytes!("fixtures/format1/timers_and_counters.mlbc"),
        include_str!("differential/timers_and_counters.st"),
    ),
];

fn version(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[4], bytes[5]])
}

/// A small generator of the inputs, the same on both sides and on every run.
struct Inputs(u64);

impl Inputs {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    /// A value for each input of an elementary type, as a host would write it between scans.
    /// Booleans change about every fourth scan, so that edges and held levels both happen.
    fn values(&mut self, c: &Compiled) -> Vec<(u32, Value)> {
        let mut out = Vec::new();
        for s in c.symbols.iter().filter(|s| s.writable && s.array.is_none()) {
            let r = self.next();
            let v = match s.ty.as_str() {
                "BOOL" => Value::Bool(r % 4 == 0),
                "SINT" => Value::Sint((r % 200) as i8),
                "INT" => Value::Int((r % 2_000) as i16 - 1_000),
                "DINT" => Value::Dint((r % 200_000) as i32 - 100_000),
                "LINT" => Value::Lint((r % 2_000_000) as i64 - 1_000_000),
                "REAL" => Value::Real((r % 20_000) as f32 / 100.0 - 50.0),
                "LREAL" => Value::Lreal((r % 20_000) as f64 / 100.0 - 50.0),
                "TIME" => Value::Time((r % 5_000) as i64 * 1_000_000),
                _ => continue,
            };
            out.push((s.offset, v));
        }
        out
    }
}

/// One side: a loaded program and what the host gives it.
struct Side<'a> {
    p: Program<'a>,
    memory: Vec<u8>,
    stack: Vec<u64>,
    calls: Vec<Frame>,
}

impl<'a> Side<'a> {
    fn new(bytes: &'a [u8]) -> Side<'a> {
        let p = Program::load(bytes).expect("it loads");
        let mut memory = vec![0; p.memory_size()];
        cold_start(&p, &mut memory).unwrap();
        Side {
            stack: vec![0; p.stack_depth()],
            calls: vec![Frame::default(); p.call_depth()],
            memory,
            p,
        }
    }
}

#[test]
fn format_1_bytecode_runs_to_the_same_scans_as_its_source_compiled_now() {
    // A newer compiler than the fixtures', or the comparison is of a format with itself.
    const { assert!(FORMAT_VERSION > 1) };
    for (name, old, source) in FORMAT_1 {
        // Committed format-1 bytes, intact: the loader checks the checksum too.
        assert_eq!(version(old), 1, "{name}: the fixture is format 1");
        let now =
            compile(source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{name}: {}", d[0]));
        assert_eq!(version(&now.bytecode), FORMAT_VERSION, "{name}");

        let mut a = Side::new(old);
        let mut b = Side::new(&now.bytecode);
        assert_eq!(a.p.format_version(), 1, "{name}");
        assert_eq!(b.p.format_version(), FORMAT_VERSION, "{name}");
        assert_eq!(a.memory, b.memory, "{name}: the same initial memory");
        assert_eq!(
            (a.p.stack_depth(), a.p.call_depth()),
            (b.p.stack_depth(), b.p.call_depth()),
            "{name}"
        );

        let mut inputs = Inputs(0x6d79_7468_6f73);
        let mut changed = 0;
        for k in 0..CYCLES {
            for (offset, v) in inputs.values(&now) {
                assert!(write(&mut a.memory, offset, v));
                assert!(write(&mut b.memory, offset, v));
            }
            let before = a.memory.clone();
            let t = k * CYCLE_NS;
            let ra = scan(&a.p, &mut a.memory, &mut a.stack, &mut a.calls, t, BUDGET);
            let rb = scan(&b.p, &mut b.memory, &mut b.stack, &mut b.calls, t, BUDGET);
            assert_eq!(ra, rb, "{name}: scan {k}");
            assert_eq!(a.memory, b.memory, "{name}: memory after scan {k}");
            if a.memory != before {
                changed += 1;
            }
        }
        // The comparison is of programs that did something, not of two memories at rest.
        assert!(
            changed > CYCLES / 2,
            "{name}: {changed} scans changed memory"
        );
    }
}

#[test]
fn every_fixture_is_one_of_the_differential_programs() {
    // A program added to tests/differential/ after format 1 has no format-1 bytes; one that
    // was there then must have them here.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/format1");
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|f| f.ends_with(".mlbc"))
        .collect();
    on_disk.sort();
    let listed: Vec<String> = FORMAT_1
        .iter()
        .map(|(n, _, _)| format!("{n}.mlbc"))
        .collect();
    assert_eq!(on_disk, listed);
}
