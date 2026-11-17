//! A controller keeps the bytecode it was given (a project's `runtimes.json`) across an
//! update of its VM. A VM reads every format it extends (ADR-013, "Format versions"), so a
//! program compiled before the update runs after it, scan for scan, as the same source
//! compiled now.
//!
//! `tests/fixtures/format1/` holds the six programs of `tests/differential/` as the format-1
//! compiler made them (`origin/dev` at f4637cd), `tests/fixtures/format2/` the seven there were
//! as the format-2 compiler made them (L2c-1's head, 78c84a7); each README says how. They are
//! committed bytes, never regenerated: regenerated, they would be what this compiler makes,
//! and prove nothing.

use mythos_logic_compiler::{compile, Compiled, TargetProfile};
use mythos_logic_vm::{cold_start, scan, write, Frame, Program, Value, FORMAT_VERSION};

const CYCLES: i64 = 200;
const CYCLE_NS: i64 = 20_000_000;
const BUDGET: u32 = 1_000_000;

/// (name, bytes an older compiler made, the source it made them from)
type Fixture = (&'static str, &'static [u8], &'static str);

macro_rules! fixture {
    ($dir:literal, $name:literal) => {
        (
            $name,
            include_bytes!(concat!("fixtures/", $dir, "/", $name, ".mlbc")),
            include_str!(concat!("differential/", $name, ".st")),
        )
    };
}

const FORMAT_1: &[Fixture] = &[
    fixture!("format1", "functions_and_blocks"),
    fixture!("format1", "integers_and_control"),
    fixture!("format1", "reals_and_conversions"),
    fixture!("format1", "small_plant"),
    fixture!("format1", "structures_and_arrays"),
    fixture!("format1", "timers_and_counters"),
];

const FORMAT_2: &[Fixture] = &[
    fixture!("format2", "functions_and_blocks"),
    fixture!("format2", "integers_and_control"),
    fixture!("format2", "reals_and_conversions"),
    fixture!("format2", "small_plant"),
    fixture!("format2", "strings"),
    fixture!("format2", "structures_and_arrays"),
    fixture!("format2", "timers_and_counters"),
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

/// Each of `fixtures`, bytes of format `format`, runs beside its source compiled now: the
/// same inputs before every scan, the same result and the same memory after it.
fn runs_alike(format: u16, fixtures: &[Fixture]) {
    // A newer compiler than the fixtures', or the comparison is of a format with itself.
    assert!(format < FORMAT_VERSION);
    for (name, old, source) in fixtures {
        // Committed bytes of that format, intact: the loader checks the checksum too.
        assert_eq!(
            version(old),
            format,
            "{name}: the fixture is format {format}"
        );
        let now =
            compile(source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{name}: {}", d[0]));
        assert_eq!(version(&now.bytecode), FORMAT_VERSION, "{name}");

        let mut a = Side::new(old);
        let mut b = Side::new(&now.bytecode);
        assert_eq!(a.p.format_version(), format, "{name}");
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
fn format_1_bytecode_runs_to_the_same_scans_as_its_source_compiled_now() {
    runs_alike(1, FORMAT_1);
}

#[test]
fn format_2_bytecode_runs_to_the_same_scans_as_its_source_compiled_now() {
    runs_alike(2, FORMAT_2);
}

#[test]
fn every_fixture_is_one_of_the_differential_programs() {
    // A program added to tests/differential/ after a format has no bytes of it; one that was
    // there then must have them here.
    for (dir, fixtures) in [("format1", FORMAT_1), ("format2", FORMAT_2)] {
        let path = format!("{}/tests/fixtures/{dir}", env!("CARGO_MANIFEST_DIR"));
        let mut on_disk: Vec<String> = std::fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f.ends_with(".mlbc"))
            .collect();
        on_disk.sort();
        let listed: Vec<String> = fixtures
            .iter()
            .map(|(n, _, _)| format!("{n}.mlbc"))
            .collect();
        assert_eq!(on_disk, listed, "{dir}");
    }
}
