//! The standard function blocks written in Structured Text (ADR-013 L4) do what the VM's
//! native ones did, scan for scan.
//!
//! The native blocks still run: a program of formats 1 to 3 calls them with the `FB`
//! instruction, and the VM keeps it so those programs keep running (`mythos_logic_vm::fb`).
//! Here each native block runs in a format-3 program of one `FB` instruction, made the way
//! the format-3 compiler made it, and beside it the library's block runs in a program this
//! compiler makes. Both are given the same inputs, written by the host into the instance
//! before each scan, and the same clock, and every member a program could name (inputs and
//! outputs) must be equal after every scan.
//!
//! The inputs are random sequences, the same on every run: rising and falling edges, levels
//! held, pulses of IN shorter than PT, PT changed while a timer runs (and PT zero or below
//! zero), resets and loads while counting, counts pressed against the limits of INT, and a
//! clock that does not move between scans as well as one that jumps past PT.
//!
//! There is no intended difference: the clock-back rule of the timers (`timers.st`) acts only
//! when the clock goes back, which it never does within a run.

use mythos_logic_compiler::{compile, Compiled, TargetProfile};
use mythos_logic_vm::fb::{Direction, FbKind};
use mythos_logic_vm::format::op;
use mythos_logic_vm::{cold_start, crc32, read, scan, write, ElemType, Frame, Program, Value};

const BUDGET: u32 = 100_000;
const MS: i64 = 1_000_000;

/// A format-3 program whose scan is one call of the native block `kind` on the instance at 0.
fn native_program(kind: FbKind) -> Vec<u8> {
    let memory = kind.size();
    let code = [op::FB, kind.code(), 0, 0, 0, 0, op::END];
    let mut b = Vec::new();
    b.extend_from_slice(b"MLBC");
    b.extend_from_slice(&3u16.to_le_bytes()); // format 3
    b.extend_from_slice(&1u16.to_le_bytes()); // stack depth
    b.extend_from_slice(&memory.to_le_bytes());
    b.extend_from_slice(&(code.len() as u32).to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes()); // call depth
    b.extend_from_slice(&1u16.to_le_bytes()); // one body, the program's
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend(std::iter::repeat_n(0u8, memory as usize));
    b.extend_from_slice(&code);
    let crc = crc32(&b);
    b.extend_from_slice(&crc.to_le_bytes());
    b
}

struct Machine {
    bytes: Vec<u8>,
    memory: Vec<u8>,
    stack: Vec<u64>,
    calls: Vec<Frame>,
}

impl Machine {
    fn new(bytes: Vec<u8>) -> Machine {
        let p = Program::load(&bytes).expect("it loads");
        let mut memory = vec![0; p.memory_size()];
        cold_start(&p, &mut memory).unwrap();
        Machine {
            stack: vec![0; p.stack_depth()],
            calls: vec![Frame::default(); p.call_depth()],
            memory,
            bytes,
        }
    }
    fn scan(&mut self, now: i64) {
        let p = Program::load(&self.bytes).unwrap();
        scan(
            &p,
            &mut self.memory,
            &mut self.stack,
            &mut self.calls,
            now,
            BUDGET,
        )
        .expect("a scan of one block does not fault");
    }
}

/// The two sides of one block: where each member is in each memory.
struct Pair {
    name: &'static str,
    native: Machine,
    st: Machine,
    /// (member, its type, whether the host sets it, offset natively, offset in ST)
    members: Vec<(&'static str, ElemType, bool, u32, u32)>,
}

impl Pair {
    fn new(kind: FbKind) -> Pair {
        let name = kind.name();
        let c: Compiled = compile(
            &format!("PROGRAM Main VAR b : {name}; END_VAR b(); END_PROGRAM"),
            &TargetProfile::pc(),
        )
        .unwrap_or_else(|d| panic!("{name}: {}", d[0]));
        let members = kind
            .members()
            .iter()
            .map(|m| {
                let s = c
                    .symbol(&format!("b.{}", m.name))
                    .unwrap_or_else(|| panic!("the library's {name} has no member {}", m.name));
                assert_eq!(s.size, m.ty.size(), "{name}.{}", m.name);
                (
                    m.name,
                    m.ty,
                    m.direction == Direction::Input,
                    m.offset,
                    s.offset,
                )
            })
            .collect();
        // The library's block has every member the native one had, and none it had not
        // that a program can name.
        let named: Vec<&str> = c
            .symbols
            .iter()
            .filter_map(|s| s.path.strip_prefix("b."))
            .collect();
        for m in kind.members() {
            assert!(named.contains(&m.name), "{name}.{}", m.name);
        }
        Pair {
            name,
            native: Machine::new(native_program(kind)),
            st: Machine::new(c.bytecode),
            members,
        }
    }

    fn set(&mut self, member: &str, v: Value) {
        let (_, _, _, a, b) = *self
            .members
            .iter()
            .find(|m| m.0 == member)
            .unwrap_or_else(|| panic!("{}.{member}", self.name));
        assert!(write(&mut self.native.memory, a, v));
        assert!(write(&mut self.st.memory, b, v));
    }

    /// Scan both and compare; each output's value goes to `seen`.
    fn scan_and_compare(
        &mut self,
        now: i64,
        seen: &mut Vec<(&'static str, String)>,
        what: &dyn Fn() -> String,
    ) {
        self.native.scan(now);
        self.st.scan(now);
        for (m, ty, input, a, b) in &self.members {
            let x = read(&self.native.memory, *a, *ty).unwrap();
            let y = read(&self.st.memory, *b, *ty).unwrap();
            assert_eq!(x, y, "{}.{m} at {now} ns, {}", self.name, what());
            let v = (*m, format!("{x:?}"));
            if !input && seen.len() < 10_000 && !seen.contains(&v) {
                seen.push(v);
            }
        }
    }
}

/// A small generator, the same on every run.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

const SEQUENCES: usize = 400;
const STEPS: usize = 150;

/// Drive `kind` through random sequences; `set` gives each scan's inputs.
fn drive(kind: FbKind, seed: u64, mut set: impl FnMut(&mut Pair, &mut Rng, usize)) -> usize {
    let mut rng = Rng(seed);
    let mut scans = 0;
    let mut seen: Vec<(&'static str, String)> = Vec::new();
    for sequence in 0..SEQUENCES {
        let mut pair = Pair::new(kind);
        // A clock from 0, as after a start, or from far on.
        let mut now: i64 = if rng.chance(50) {
            0
        } else {
            rng.below(1 << 40) as i64
        };
        let mut log: Vec<String> = Vec::new();
        for step in 0..STEPS {
            set(&mut pair, &mut rng, step);
            // Time that does not move, a nanosecond, a few milliseconds, or a long jump.
            now += match rng.below(10) {
                0 | 1 => 0,
                2 => 1,
                3..=6 => rng.below(30) as i64 * MS,
                7 | 8 => rng.below(200) as i64 * MS,
                _ => rng.below(5_000) as i64 * MS,
            };
            let inputs: Vec<String> = pair
                .members
                .iter()
                .filter(|m| m.2)
                .map(|m| format!("{}={:?}", m.0, read(&pair.st.memory, m.4, m.1).unwrap()))
                .collect();
            log.push(format!("@{now} {}", inputs.join(" ")));
            let shown = || format!("sequence {sequence}, step {step}: {}", log.join(" | "));
            pair.scan_and_compare(now, &mut seen, &shown);
            scans += 1;
        }
    }
    // The sequences moved every output: each took more than one value (a timer's ET many).
    for m in kind
        .members()
        .iter()
        .filter(|m| m.direction == Direction::Output)
    {
        let values = seen.iter().filter(|(n, _)| *n == m.name).count();
        let enough = if m.ty == ElemType::Time { 50 } else { 2 };
        assert!(
            values >= enough,
            "{}.{}: {values} values",
            kind.name(),
            m.name
        );
    }
    scans
}

/// A BOOL that holds its level for a while: how long varies by sequence, so that pulses
/// shorter than a PT and levels held past it both happen.
fn level(rng: &mut Rng, was: &mut bool, percent: u64) -> Value {
    if rng.chance(percent) {
        *was = !*was;
    }
    Value::Bool(*was)
}

fn pt(rng: &mut Rng) -> Value {
    Value::Time(match rng.below(20) {
        0 => 0,
        1 => -(rng.below(100) as i64) * MS,
        2 => 1,
        3..=9 => rng.below(100) as i64 * MS,
        _ => rng.below(2_000) as i64 * MS,
    })
}

#[test]
fn the_timers_written_in_st_run_as_the_native_ones() {
    for (k, kind) in [FbKind::Ton, FbKind::Tof, FbKind::Tp]
        .into_iter()
        .enumerate()
    {
        let mut was = false;
        let mut flip = 30;
        let scans = drive(kind, 0x7469_6d65 + k as u64, |p, rng, step| {
            if step == 0 {
                flip = 5 + rng.below(60);
                p.set("PT", pt(rng));
            }
            p.set("IN", level(rng, &mut was, flip));
            // PT changed while it may be timing.
            if rng.chance(4) {
                p.set("PT", pt(rng));
            }
        });
        assert_eq!(scans, SEQUENCES * STEPS);
    }
}

#[test]
fn the_counters_written_in_st_run_as_the_native_ones() {
    for (k, kind) in [FbKind::Ctu, FbKind::Ctd].into_iter().enumerate() {
        let (pulse, load) = if kind == FbKind::Ctu {
            ("CU", "R")
        } else {
            ("CD", "LD")
        };
        let mut was = false;
        let mut flip = 50;
        drive(kind, 0x636f_756e + k as u64, |p, rng, step| {
            if step == 0 {
                flip = 10 + rng.below(80);
                p.set("PV", Value::Int(rng.below(20) as i16 - 5));
                // A count already near a limit of INT, as a long run leaves it.
                if rng.chance(30) {
                    let near = if rng.chance(50) { i16::MAX } else { i16::MIN };
                    p.set("CV", Value::Int(near.saturating_sub(rng.below(3) as i16)));
                }
            }
            p.set(pulse, level(rng, &mut was, flip));
            p.set(load, Value::Bool(rng.chance(5)));
            if rng.chance(3) {
                let pv = match rng.below(4) {
                    0 => i16::MAX,
                    1 => i16::MIN,
                    _ => rng.below(40) as i16 - 10,
                };
                p.set("PV", Value::Int(pv));
            }
        });
    }
}

#[test]
fn the_edges_and_bistables_written_in_st_run_as_the_native_ones() {
    for (k, kind) in [FbKind::RTrig, FbKind::FTrig].into_iter().enumerate() {
        let mut was = false;
        let mut flip = 50;
        drive(kind, 0x6564_6765 + k as u64, |p, rng, step| {
            if step == 0 {
                flip = 5 + rng.below(90);
                // Half the sequences start with CLK TRUE: the first call after a cold start.
                was = rng.chance(50);
            }
            p.set("CLK", level(rng, &mut was, flip));
        });
    }
    for (k, (kind, set, reset)) in [(FbKind::Sr, "S1", "R"), (FbKind::Rs, "S", "R1")]
        .into_iter()
        .enumerate()
    {
        let (mut s, mut r) = (false, false);
        drive(kind, 0x6269_7374 + k as u64, |p, rng, _| {
            p.set(set, level(rng, &mut s, 30));
            p.set(reset, level(rng, &mut r, 30));
        });
    }
}

#[test]
fn every_native_block_has_its_counterpart_in_the_library() {
    for kind in FbKind::ALL {
        Pair::new(kind);
    }
}
