//! A controller keeps the bytecode it was given (a project's `runtimes.json`) across an
//! update of its VM. A VM reads every format it extends (ADR-013, "Format versions"), so a
//! program compiled before the update runs after it, scan for scan, as the same source
//! compiled now.
//!
//! `tests/fixtures/format1/` holds the six programs of `tests/differential/` as the format-1
//! compiler made them (`origin/dev` at f4637cd), `tests/fixtures/format2/` the seven there were
//! as the format-2 compiler made them (L2c-1's head, 78c84a7), and `tests/fixtures/format3/`
//! the eight as the format-3 compiler made them (C7b-1, the tree of ca35b59), with the symbols it
//! gave them; each README says how. They are committed bytes, never regenerated: regenerated,
//! they would be what this compiler makes, and prove nothing.
//!
//! Since format 4 the standard function blocks are written in Structured Text (ADR-013 L4):
//! an instance holds its internal state as variables of its own, so the source compiled now
//! lays memory out otherwise than the fixtures do. What must be equal is what the program
//! means: after every scan, every variable the two share by path and type, the members of
//! every standard block among them. The fixtures of formats 1 to 3 lay memory out alike (their
//! bytes differ in the version field and the checksum only, which a test holds), so the
//! format-3 symbols say where everything of each is.

use mythos_logic_compiler::{compile, Compiled, Symbol, TargetProfile};
use mythos_logic_vm::fb::FbKind;
use mythos_logic_vm::{
    cold_start, read, scan, write, ElemType, Frame, Program, Value, FORMAT_VERSION,
};

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

const FORMAT_3: &[Fixture] = &[
    fixture!("format3", "functions_and_blocks"),
    fixture!("format3", "integers_and_control"),
    fixture!("format3", "library"),
    fixture!("format3", "reals_and_conversions"),
    fixture!("format3", "small_plant"),
    fixture!("format3", "strings"),
    fixture!("format3", "structures_and_arrays"),
    fixture!("format3", "timers_and_counters"),
];

/// The symbols the format-3 compiler gave the program `name`.
fn format_3_symbols(name: &str) -> Vec<Symbol> {
    let path = format!(
        "{}/tests/fixtures/format3/{name}.symbols.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn format_3_bytes(name: &str) -> &'static [u8] {
    FORMAT_3
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, b, _)| *b)
        .unwrap_or_else(|| panic!("no format-3 fixture of {name}"))
}

fn version(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[4], bytes[5]])
}

fn elem(ty: &str) -> Option<ElemType> {
    Some(match ty {
        "BOOL" => ElemType::Bool,
        "SINT" => ElemType::Sint,
        "INT" => ElemType::Int,
        "DINT" => ElemType::Dint,
        "LINT" => ElemType::Lint,
        "REAL" => ElemType::Real,
        "LREAL" => ElemType::Lreal,
        "TIME" => ElemType::Time,
        _ => return None,
    })
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

    /// A value for each input of an elementary type, by its path, as a host would write it
    /// between scans. Booleans change about every fourth scan, so that edges and held levels
    /// both happen.
    fn values(&mut self, c: &Compiled) -> Vec<(String, Value)> {
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
            out.push((s.path.clone(), v));
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

/// Every variable of one value both programs have, by path and type: (path, type, offset in
/// the older program, offset in the newer one). An array is compared element by element.
fn shared(old: &[Symbol], new: &Compiled) -> Vec<(String, ElemType, u32, u32)> {
    let mut out = Vec::new();
    for o in old {
        let Some(n) = new.symbols.iter().find(|n| n.path == o.path) else {
            continue;
        };
        assert_eq!(n.ty, o.ty, "{}: the same type", o.path);
        // An array of standard blocks: their public members, element by element. The old
        // instance is laid out as the VM's native block; the new one as its type says.
        if let (Some(a), Some(b)) = (&o.array, &n.array) {
            if let Some(kind) = FbKind::ALL.into_iter().find(|k| k.name() == a.element_type) {
                let layout = new
                    .types
                    .iter()
                    .find(|t| t.name == a.element_type)
                    .expect("the layout of the library's block");
                let count: i64 = a.bounds.iter().map(|(lo, hi)| hi - lo + 1).product();
                for k in 0..count as u32 {
                    for m in kind.members() {
                        let at = layout.members.iter().find(|x| x.name == m.name).unwrap();
                        out.push((
                            format!("{}#{k}.{}", o.path, m.name),
                            m.ty,
                            o.offset + k * a.element_size + m.offset,
                            n.offset + k * b.element_size + at.offset,
                        ));
                    }
                }
                continue;
            }
        }
        assert_eq!(n.size, o.size, "{}: the same size", o.path);
        match (&o.array, elem(&o.ty)) {
            (None, Some(ty)) => out.push((o.path.clone(), ty, o.offset, n.offset)),
            (Some(a), _) => {
                let Some(ty) = elem(&a.element_type) else {
                    continue;
                };
                let count: i64 = a.bounds.iter().map(|(lo, hi)| hi - lo + 1).product();
                for k in 0..count as u32 {
                    let at = k * a.element_size;
                    out.push((format!("{}#{k}", o.path), ty, o.offset + at, n.offset + at));
                }
            }
            // A STRING: compared as its bytes, one SINT at a time.
            (None, None) => {
                for k in 0..o.size {
                    out.push((
                        format!("{}#{k}", o.path),
                        ElemType::Sint,
                        o.offset + k,
                        n.offset + k,
                    ));
                }
            }
        }
    }
    out
}

/// Each of `fixtures`, bytes of format `format`, runs beside its source compiled now: the
/// same inputs before every scan, and after it the same outcome and every variable the two
/// share equal.
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
        // Laid out as the format-3 one is: the same bytes but the version and the checksum.
        let three = format_3_bytes(name);
        assert_eq!(old.len(), three.len(), "{name}");
        let differ: Vec<usize> = (0..old.len()).filter(|k| old[*k] != three[*k]).collect();
        assert!(
            differ.iter().all(|k| *k == 4 || *k + 4 >= old.len()),
            "{name}: format {format} differs from format 3 at {differ:?}"
        );
        let symbols = format_3_symbols(name);
        let now =
            compile(source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{name}: {}", d[0]));
        assert_eq!(version(&now.bytecode), FORMAT_VERSION, "{name}");
        let shared = shared(&symbols, &now);
        // Every variable of the old program is in the new one: compiled now, a program has
        // every variable it had, and more only inside standard blocks.
        let leaves = symbols
            .iter()
            .filter(|s| now.symbol(&s.path).is_some())
            .count();
        assert_eq!(leaves, symbols.len(), "{name}: a variable went missing");

        let mut a = Side::new(old);
        let mut b = Side::new(&now.bytecode);
        assert_eq!(a.p.format_version(), format, "{name}");
        assert_eq!(b.p.format_version(), FORMAT_VERSION, "{name}");
        let equal = |a: &Side, b: &Side, when: &str| {
            for (path, ty, x, y) in &shared {
                assert_eq!(
                    read(&a.memory, *x, *ty),
                    read(&b.memory, *y, *ty),
                    "{name}: {path} {when}"
                );
            }
        };
        equal(&a, &b, "at the start");

        let mut inputs = Inputs(0x6d79_7468_6f73);
        let mut changed = 0;
        for k in 0..CYCLES {
            for (path, v) in inputs.values(&now) {
                let o = symbols.iter().find(|s| s.path == path).unwrap();
                let n = now.symbol(&path).unwrap();
                assert!(write(&mut a.memory, o.offset, v));
                assert!(write(&mut b.memory, n.offset, v));
            }
            let before = a.memory.clone();
            let t = k * CYCLE_NS;
            let ra = scan(&a.p, &mut a.memory, &mut a.stack, &mut a.calls, t, BUDGET);
            let rb = scan(&b.p, &mut b.memory, &mut b.stack, &mut b.calls, t, BUDGET);
            assert_eq!(ra.is_ok(), rb.is_ok(), "{name}: scan {k}: {ra:?} {rb:?}");
            equal(&a, &b, &format!("after scan {k}"));
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
fn format_3_bytecode_runs_to_the_same_scans_as_its_source_compiled_now() {
    runs_alike(3, FORMAT_3);
}

#[test]
fn every_fixture_is_one_of_the_differential_programs() {
    // A program added to tests/differential/ after a format has no bytes of it; one that was
    // there then must have them here.
    for (dir, fixtures) in [
        ("format1", FORMAT_1),
        ("format2", FORMAT_2),
        ("format3", FORMAT_3),
    ] {
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
