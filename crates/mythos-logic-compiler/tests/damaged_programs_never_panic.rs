//! The VM's promise is that no program, however wrong, makes it read or write outside what it
//! was given or panic: a wrong program is refused at load or stops with a `Fault`. The loader's
//! tests name each refusal; this one goes looking for the ones nobody thought of.
//!
//! Every program of the differential corpus is compiled, then damaged: bytes are changed at
//! random and the checksum made right again, so the damage reaches the loader and, when the
//! loader lets it through, the scan. Thousands of damaged programs are loaded and scanned. A
//! panic anywhere fails the test; so would an out-of-bounds access, which in safe Rust is one.
//!
//! The damage is drawn from a fixed seed: a failure here is reproducible.

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::{cold_start, crc32, scan, Frame, Program};
use std::path::Path;

/// xorshift64*: small, deterministic, good enough to scatter damage.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differential");
    let mut programs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "st"))
        .collect();
    programs.sort();
    programs
        .into_iter()
        .map(|p| {
            let source = std::fs::read_to_string(&p).unwrap();
            let compiled =
                compile(&source, &TargetProfile::pc()).unwrap_or_else(|d| panic!("{d:?}"));
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                compiled.bytecode,
            )
        })
        .collect()
}

/// Load `bytes` and, if the loader takes it, scan it a few times. Returns whether it loaded.
fn exercise(bytes: &[u8]) -> bool {
    let Ok(program) = Program::load(bytes) else {
        return false;
    };
    // The host gives what the program declares, as a real host does; a damaged header may
    // declare anything, so bound what this test is willing to allocate.
    if program.memory_size() > 1 << 20 {
        return true;
    }
    let mut memory = vec![0u8; program.memory_size()];
    let mut stack = vec![0u64; program.stack_depth()];
    let mut calls = vec![Frame::default(); program.call_depth()];
    if cold_start(&program, &mut memory).is_err() {
        return true;
    }
    for k in 0..4i64 {
        // A budget small enough that a damaged loop ends quickly, as a fault.
        let _ = scan(
            &program,
            &mut memory,
            &mut stack,
            &mut calls,
            k * 20_000_000,
            20_000,
        );
    }
    true
}

#[test]
fn a_damaged_program_is_refused_or_faults_and_never_panics() {
    let corpus = corpus();
    assert!(corpus.len() >= 4, "the corpus is where it is expected");
    let mut rng = Rng(0x00C0_FFEE_1234_5678);
    let (mut tried, mut loaded) = (0u32, 0u32);
    for (name, good) in &corpus {
        assert!(exercise(good), "{name} as compiled loads");
        let body = good.len() - 4;
        for round in 0..4_000 {
            let mut bytes = good.clone();
            // One to four bytes changed; half the time only within the code and header's
            // sizes, where a change is most likely to pass the loader.
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(body);
                bytes[at] = match round % 3 {
                    0 => rng.next() as u8,
                    1 => bytes[at] ^ (1 << rng.below(8)),
                    // An opcode-sized nudge: lands on neighbouring instructions.
                    _ => bytes[at].wrapping_add(1 + rng.below(3) as u8),
                };
            }
            let crc = crc32(&bytes[..body]);
            bytes[body..].copy_from_slice(&crc.to_le_bytes());
            tried += 1;
            loaded += exercise(&bytes) as u32;
        }
        // And cut short at every length: a download that stopped half way.
        for len in 0..good.len().min(600) {
            let _ = exercise(&good[..len]);
        }
    }
    println!("{tried} damaged programs, {loaded} of them passed the loader and were scanned");
    // The damage must actually reach the scan, or this proves only the loader.
    assert!(
        loaded > tried / 20,
        "too few damaged programs load: {loaded} of {tried}"
    );
}
