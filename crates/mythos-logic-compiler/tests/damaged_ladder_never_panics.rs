//! No Ladder Diagram or Function Block Diagram document, however damaged, makes the graphical
//! front-end panic or say nothing (ADR-013 §2.15, §2.16); `damaged_sources_never_panic.rs` does
//! the same for Structured Text.
//!
//! The hand-written TC6 projects of the emulator corpus (`tests/ladder/oscillator.xml`,
//! `tests/fbd/blinker.xml`) are
//! damaged the ways a file is, by a tool or by hand: characters and stretches deleted, pieces
//! of XML and of the schema inserted, lines repeated, swapped and dropped, attribute values and
//! ids changed, the text cut short. Each damaged document must compile or be refused with at
//! least one diagnostic, each naming a place: a line of the document, or an element of a POU.
//! The damage is drawn from a fixed seed: a failure here is reproducible.

use mythos_logic_compiler::{compile_in, Language, TargetProfile};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const PIECES: [&str; 29] = [
    "<",
    ">",
    "/>",
    "\"",
    "&",
    "&amp;",
    "<!--",
    "-->",
    "<![CDATA[",
    "]]>",
    "<connection refLocalId=\"12\"/>",
    "<connection refLocalId=\"999\"/>",
    "<connection refLocalId=\"13\" formalParameter=\"Q\"/>",
    "<contact localId=\"10\"><position x=\"1\" y=\"1\"/><variable>blink</variable></contact>",
    "<coil localId=\"500\"><position x=\"1\" y=\"1\"/><variable>x</variable></coil>",
    "<jump localId=\"501\" label=\"DONE\"><position x=\"1\" y=\"1\"/></jump>",
    "<label localId=\"502\" label=\"DONE\"><position x=\"1\" y=\"1\"/></label>",
    "negated=\"true\"",
    "edge=\"falling\"",
    "storage=\"reset\"",
    "localId=\"x\"",
    "<expression>a +</expression>",
    "<variable name=\"__x\"><type><BOOL/></type></variable>",
    "\u{0}",
    // Of a Function Block Diagram: lines continued, drawn back, joined.
    "<connector localId=\"503\" name=\"LVL\"><position x=\"1\" y=\"1\"/><connectionPointIn><connection refLocalId=\"41\"/></connectionPointIn></connector>",
    "<continuation localId=\"504\" name=\"LVL\"><position x=\"1\" y=\"1\"/><connectionPointOut/></continuation>",
    "<connection refLocalId=\"31\" formalParameter=\"CV\"/>",
    "<connection refLocalId=\"54\"/>",
    "executionOrderId=\"7\"",
];

fn damage(text: &str, rng: &mut Rng) -> String {
    let mut s: Vec<char> = text.chars().collect();
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(s.len() + 1).min(s.len());
        match rng.below(7) {
            0 => {
                let n = (1 + rng.below(40)).min(s.len() - at);
                s.drain(at..at + n);
            }
            1 => {
                let piece: Vec<char> = PIECES[rng.below(PIECES.len())].chars().collect();
                s.splice(at..at, piece);
            }
            2 | 3 => {
                let mut lines: Vec<String> = s
                    .iter()
                    .collect::<String>()
                    .lines()
                    .map(String::from)
                    .collect();
                let (a, b) = (rng.below(lines.len()), rng.below(lines.len()));
                match rng.below(3) {
                    0 => lines.swap(a, b),
                    1 => {
                        let l = lines[a].clone();
                        lines.insert(b, l);
                    }
                    _ => {
                        lines.remove(a);
                    }
                }
                s = lines.join("\n").chars().collect();
            }
            4 => {
                // A digit changed: an id, a position, a length.
                if let Some(k) = (at..s.len()).find(|k| s[*k].is_ascii_digit()) {
                    s[k] = char::from(b'0' + rng.below(10) as u8);
                }
            }
            5 => s.truncate(at),
            _ => {
                if at < s.len() {
                    s[at] = char::from(32 + rng.below(95) as u8);
                }
            }
        }
    }
    s.into_iter().collect()
}

#[test]
fn a_damaged_ladder_document_is_compiled_or_refused_with_places_never_a_panic() {
    damaged(
        include_str!("ladder/oscillator.xml"),
        Language::Ld,
        0x5EED_1AD0_0000_0001,
    );
}

#[test]
fn a_damaged_function_block_diagram_is_compiled_or_refused_with_places_never_a_panic() {
    damaged(
        include_str!("fbd/blinker.xml"),
        Language::Fbd,
        0x5EED_FBD0_0000_0001,
    );
}

fn damaged(original: &str, language: Language, seed: u64) {
    let mut rng = Rng(seed);
    let (mut refused, mut compiled) = (0, 0);
    for _ in 0..3000 {
        let text = damage(original, &mut rng);
        match compile_in(&text, language, &TargetProfile::pc()) {
            Ok(_) => compiled += 1,
            Err(diags) => {
                refused += 1;
                assert!(!diags.is_empty(), "refused with nothing said:\n{text}");
                let lines = text.lines().count() as u32 + 1;
                for d in &diags {
                    let whole = (d.line, d.col, d.end_line, d.end_col) == (0, 0, 0, 0);
                    assert!(
                        whole || d.element.is_some() || (d.line >= 1 && d.line <= lines),
                        "{d} has no place in a document of {lines} lines:\n{text}"
                    );
                    assert!(d.line <= lines, "{d} points past the document:\n{text}");
                }
            }
        }
    }
    eprintln!("{refused} refused, {compiled} compiled");
    assert!(
        refused > 1000 && compiled > 100,
        "{refused} refused, {compiled} compiled"
    );
}
