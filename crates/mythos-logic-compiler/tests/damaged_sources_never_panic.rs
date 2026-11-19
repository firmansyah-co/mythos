//! The front-end's promise is that no source, however wrong, makes it panic, loop or say
//! nothing: a wrong source is answered with diagnostics. The recovery tests of
//! `mythos-logic-st` name each mistake; this one goes looking for the ones nobody thought of
//! (`damaged_programs_never_panic.rs` does the same to the VM with damaged bytecode).
//!
//! Every program of the differential corpus is damaged the way an editor's buffer is while
//! someone types: characters and stretches are deleted, pieces of syntax are inserted, lines
//! are repeated, swapped and dropped, the text is cut short. Thousands of damaged sources are
//! compiled. Each must compile or be refused, and a refusal must be usable by an editor:
//!
//! - at least one diagnostic;
//! - in the order of the source;
//! - each either about the whole program (all of line, col, end_line, end_col are 0) or with
//!   a range that lies inside the source and does not end before it starts;
//! - none said twice;
//! - and of what the lexer and the parser say, no two about the same place: a mistake of
//!   syntax must not bring a second message about the token it was found at (ADR-013 §2.8).
//!   (The checker may have something of its own to say about a name the parser refused: a
//!   keyword as a name that is also declared twice is two mistakes.)
//!
//! Each program is damaged as written (LF, which `.gitattributes` keeps on every system) and,
//! less often, with CRLF line ends, as an editor on Windows sends it: a CRLF source once
//! reached a structure holding itself that no LF one had, and overflowed the stack.
//!
//! The damage is drawn from a fixed seed: a failure here is reproducible.

use mythos_logic_compiler::{compile, Diagnostic, TargetProfile};
use mythos_logic_st::{lexer, parser::Parser};
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

/// Pieces of syntax to drop into a source: what a half-typed program is made of.
const PIECES: [&str; 44] = [
    ";",
    ":",
    ":=",
    ",",
    ".",
    "..",
    "(",
    ")",
    "[",
    "]",
    "'",
    "\"",
    "(*",
    "*)",
    "//",
    "#",
    "%",
    "§",
    "$",
    "\n",
    " 1 ",
    " 16#",
    " T#",
    " INT#",
    " 1e",
    " x ",
    " IF ",
    " THEN ",
    " ELSE ",
    " END_IF ",
    " CASE ",
    " OF ",
    " FOR ",
    " DO ",
    " WHILE ",
    " REPEAT ",
    " UNTIL ",
    " VAR ",
    " END_VAR ",
    " PROGRAM ",
    " END_PROGRAM ",
    " FUNCTION ",
    " TYPE ",
    " STRUCT ",
];

fn corpus() -> Vec<(String, String)> {
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
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect()
}

/// One piece of damage to `chars`.
fn damage(chars: &mut Vec<char>, rng: &mut Rng) {
    if chars.is_empty() {
        chars.extend(PIECES[rng.below(PIECES.len())].chars());
        return;
    }
    let at = rng.below(chars.len());
    match rng.below(7) {
        // A character gone, or a stretch of them.
        0 => {
            chars.remove(at);
        }
        1 => {
            let end = (at + 1 + rng.below(24)).min(chars.len());
            chars.drain(at..end);
        }
        // A piece of syntax where there was none.
        2 | 3 => {
            let piece: Vec<char> = PIECES[rng.below(PIECES.len())].chars().collect();
            chars.splice(at..at, piece);
        }
        // A character changed: to any printable one, or to one outside ASCII.
        4 => {
            chars[at] = match rng.below(8) {
                0 => 'é',
                1 => '\t',
                _ => (b' ' + rng.below(95) as u8) as char,
            };
        }
        // A line repeated, dropped or moved to another place.
        5 => {
            let text: String = chars.iter().collect();
            let mut lines: Vec<&str> = text.split('\n').collect();
            let line = rng.below(lines.len());
            match rng.below(3) {
                0 => lines.insert(line, lines[line]),
                1 => {
                    lines.remove(line);
                }
                _ => {
                    let moved = lines.remove(line);
                    lines.insert(rng.below(lines.len() + 1), moved);
                }
            }
            *chars = lines.join("\n").chars().collect();
        }
        // The rest of the source not typed yet.
        _ => chars.truncate(at),
    }
}

/// What an editor needs of the diagnostics of `source`, or what is wrong with them.
fn usable(source: &str, diags: &[Diagnostic]) -> Result<(), String> {
    if diags.is_empty() {
        return Err("refused without a diagnostic".into());
    }
    let lines: Vec<usize> = source.split('\n').map(|l| l.chars().count()).collect();
    // A position inside the source: up to the place after the last character of a line.
    let inside = |line: u32, col: u32| {
        line >= 1
            && col >= 1
            && lines
                .get(line as usize - 1)
                .is_some_and(|len| col as usize <= len + 1)
    };
    for d in diags {
        if d.message.is_empty() {
            return Err(format!("a diagnostic without a message at {d}"));
        }
        if (d.line, d.col, d.end_line, d.end_col) == (0, 0, 0, 0) {
            continue;
        }
        if !inside(d.line, d.col) {
            return Err(format!("starts outside the source: {d}"));
        }
        if !inside(d.end_line, d.end_col) {
            return Err(format!(
                "ends outside the source ({}:{}): {d}",
                d.end_line, d.end_col
            ));
        }
        if (d.end_line, d.end_col) < (d.line, d.col) {
            return Err(format!(
                "ends ({}:{}) before it starts: {d}",
                d.end_line, d.end_col
            ));
        }
    }
    for pair in diags.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if (a.line, a.col) > (b.line, b.col) {
            return Err(format!("out of order: {a} before {b}"));
        }
    }
    for (i, d) in diags.iter().enumerate() {
        if diags[..i].contains(d) {
            return Err(format!("said twice: {d}"));
        }
    }
    Ok(())
}

/// What the lexer and the parser say about `source`: no two messages about one place.
fn syntax_is_told_once(source: &str) -> Result<(), String> {
    let (_, mut diags) = Parser::new(lexer::tokens(source)).source();
    diags.sort_by_key(|d| (d.line, d.col));
    for pair in diags.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if (a.line, a.col) == (b.line, b.col) {
            return Err(format!("two about one place: {a} and {b}"));
        }
    }
    Ok(())
}

#[test]
fn a_damaged_source_compiles_or_is_refused_with_usable_diagnostics_and_never_panics() {
    let corpus = corpus();
    assert!(corpus.len() >= 4, "the corpus is where it is expected");
    let target = TargetProfile::pc();
    let mut rng = Rng(0x0005_EED0_FADA_A6ED);
    let (mut tried, mut refused, mut told, mut several) = (0u32, 0u32, 0u64, 0u32);
    let mut check = |source: &str| {
        // A panic would name no source: catch it to say which one it was.
        let outcome = std::panic::catch_unwind(|| compile(source, &target));
        let Ok(outcome) = outcome else {
            panic!("the front-end panicked on:\n{source}");
        };
        tried += 1;
        if let Err(wrong) = syntax_is_told_once(source) {
            panic!("{wrong}\nin the diagnostics of:\n{source}");
        }
        if let Err(diags) = outcome {
            if let Err(wrong) = usable(source, &diags) {
                panic!("{wrong}\nin the diagnostics of:\n{source}");
            }
            refused += 1;
            told += diags.len() as u64;
            several += (diags.len() > 1) as u32;
        }
    };
    for (name, good) in &corpus {
        assert!(compile(good, &target).is_ok(), "{name} as written compiles");
        let good: Vec<char> = good.chars().collect();
        for _ in 0..2_000 {
            let mut chars = good.clone();
            for _ in 0..1 + rng.below(4) {
                damage(&mut chars, &mut rng);
            }
            check(&chars.iter().collect::<String>());
        }
        // And cut short, at every third length: a program being typed.
        for len in (0..good.len()).step_by(3) {
            check(&good[..len].iter().collect::<String>());
        }
    }
    for (_, good) in &corpus {
        let crlf: Vec<char> = good.replace('\n', "\r\n").chars().collect();
        for _ in 0..500 {
            let mut chars = crlf.clone();
            for _ in 0..1 + rng.below(4) {
                damage(&mut chars, &mut rng);
            }
            check(&chars.iter().collect::<String>());
        }
    }
    println!(
        "{tried} damaged sources: {refused} refused with {told} diagnostics in all, {several} of them with more than one; {} compiled",
        tried - refused
    );
    // The damage must be neither nothing nor everything, and must reach recovery: sources
    // that still compile, sources that are refused, and refusals that tell several mistakes.
    assert!(refused > tried / 2, "too few refused: {refused} of {tried}");
    assert!(tried - refused > tried / 200, "too few still compile");
    assert!(several > tried / 20, "too few with several diagnostics");
}
