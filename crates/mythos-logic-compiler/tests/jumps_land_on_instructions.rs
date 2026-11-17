//! The loader refuses a jump that does not land where an instruction of its body starts
//! (ADR-013 §2.3). It has no memory to note the starts in, so it proves a target by walking
//! to it from a start it already knows, and it keeps a few of those to walk from. This test
//! has the memory: it lists every start of every body the plain way and asks the loader about
//! every jump of every program of the corpus, pointed at every byte of its body in turn. The
//! loader must take the jump exactly when the byte is a start.
//!
//! The programs are compiled ones, so the jumps are the nests of IF, CASE and loops the
//! loader's shortcuts are made for, and each is tried with every target, near and far, ahead
//! and behind. One more program is written here to have every shape a shortcut is for: loops
//! one after another, loops nested three deep, and a CASE whose arms hold IF and ELSIF.

use mythos_logic_compiler::{compile, TargetProfile};
use mythos_logic_vm::format::{instruction_len, op};
use mythos_logic_vm::{crc32, LoadError, Program};
use std::path::Path;

/// Loops in a row, a CASE of arms with IF and ELSIF in them, loops nested three deep after a
/// stretch without any, and a function with a loop of its own (a second body).
const SHAPES: &str = "FUNCTION Sum : DINT VAR_INPUT n : DINT; END_VAR VAR k : DINT; END_VAR
  FOR k := 1 TO n DO IF k > 2 THEN Sum := Sum + k; END_IF; END_FOR;
END_FUNCTION
PROGRAM Shapes VAR i, j, a, sel : DINT; END_VAR
  FOR i := 1 TO 3 DO a := a + i; END_FOR;
  WHILE a > 100 DO a := a - 7; END_WHILE;
  REPEAT a := a + 1; UNTIL a > 3 END_REPEAT;
  CASE sel OF
    0: IF a > 1 THEN a := a + 1; ELSIF a < 0 THEN a := 0; ELSE a := a - 1; END_IF;
    1: IF a > 2 THEN a := a + 2; ELSIF a < -5 THEN a := 5; ELSE a := a - 2; END_IF;
    2, 3: a := Sum(a);
    4..9: IF a = 4 THEN sel := 0; END_IF;
  ELSE
    sel := 0;
  END_CASE;
  IF a > 10 THEN a := 10; ELSE a := a + 1; END_IF;
  FOR i := 1 TO 2 DO
    WHILE a > 9 DO
      FOR j := 1 TO 2 DO a := a - j; END_FOR;
    END_WHILE;
  END_FOR;
  IF a < 0 THEN a := 0; END_IF;
  FOR i := 1 TO 2 DO
    REPEAT
      FOR j := 1 TO 2 DO IF a > j THEN a := a - j; END_IF; END_FOR;
    UNTIL a < 5 END_REPEAT;
  END_FOR;
END_PROGRAM";

/// The parts of a program in the VM's format (its header is 20 bytes, then 8 per body).
struct Parts {
    /// Where the code starts in the file.
    code_at: usize,
    code_len: usize,
    /// Where each body starts in the code, and where the last one ends.
    bounds: Vec<usize>,
}

fn parts(bytes: &[u8]) -> Parts {
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let (memory, code_len, bodies) = (u32_at(8), u32_at(12), u16_at(18));
    let mut bounds: Vec<usize> = (0..bodies).map(|k| u32_at(20 + 8 * k)).collect();
    bounds.push(code_len);
    Parts {
        code_at: 20 + 8 * bodies + memory,
        code_len,
        bounds,
    }
}

/// `bytes` with the four bytes at `at` replaced and the checksum made right again.
fn retargeted(bytes: &[u8], at: usize, target: u32) -> Vec<u8> {
    let mut b = bytes.to_vec();
    b[at..at + 4].copy_from_slice(&target.to_le_bytes());
    let body = b.len() - 4;
    let crc = crc32(&b[..body]);
    b[body..].copy_from_slice(&crc.to_le_bytes());
    b
}

#[test]
fn the_loader_takes_a_jump_exactly_when_it_lands_where_an_instruction_of_its_body_starts() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/differential");
    let mut sources: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "st"))
        .collect();
    sources.sort();
    assert!(sources.len() >= 4, "the corpus is where it is expected");
    let mut programs: Vec<(String, String)> = sources
        .iter()
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().to_string(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect();
    programs.push(("the shapes written here".into(), SHAPES.into()));
    let (mut jumps, mut tried, mut taken) = (0u32, 0u32, 0u32);
    let (mut forward, mut backward) = (0u32, 0u32);
    for (name, source) in &programs {
        let good = compile(source, &TargetProfile::pc()).unwrap().bytecode;
        assert!(Program::load(&good).is_ok(), "{name} as compiled loads");
        let p = parts(&good);
        let code = &good[p.code_at..p.code_at + p.code_len];
        for body in p.bounds.windows(2) {
            let (start, end) = (body[0], body[1]);
            // Every start of an instruction of this body, the plain way.
            let mut starts = vec![false; end];
            let mut pc = start;
            while pc < end {
                starts[pc] = true;
                pc += instruction_len(code[pc]).unwrap();
            }
            for at in (start..end).filter(|at| starts[*at]) {
                if !matches!(code[at], op::JMP | op::JZ) {
                    continue;
                }
                jumps += 1;
                let written = u32::from_le_bytes(code[at + 1..at + 5].try_into().unwrap());
                forward += (written as usize > at) as u32;
                backward += (written as usize <= at) as u32;
                // Every byte of the body, and one before it and one after it.
                for target in start.saturating_sub(1)..=end {
                    let bytes = retargeted(&good, p.code_at + at + 1, target as u32);
                    // Outside the body there is no start of its: `starts` ends where it does.
                    let lands = target >= start && starts.get(target) == Some(&true);
                    let expected = if lands {
                        None
                    } else {
                        Some(LoadError::BadJump(at as u32))
                    };
                    tried += 1;
                    taken += expected.is_none() as u32;
                    assert_eq!(
                        Program::load(&bytes).err(),
                        expected,
                        "{name}: the jump at {at} (body {start}..{end}) pointed at {target}"
                    );
                }
            }
        }
    }
    println!(
        "{jumps} jumps ({forward} forward, {backward} back) pointed at {tried} places: {taken} taken, {} refused",
        tried - taken
    );
    // The programs must have what the shortcuts are for, or this proves little.
    assert!(
        forward >= 60 && backward >= 15,
        "{forward} forward, {backward} back"
    );
}
