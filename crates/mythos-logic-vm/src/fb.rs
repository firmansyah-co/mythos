//! The standard function blocks of IEC 61131-3 (§6.6.3.5), built into the VM.
//!
//! An instance is a run of bytes in the program's memory. Its layout is defined here once and
//! read by the compiler (`FbKind::members`), so a program and the VM cannot disagree on it.
//! Members a program may not touch (an edge memory, a start time) come after the public ones.

use crate::format::ElemType;

/// The standard function blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FbKind {
    Ton,
    Tof,
    Tp,
    Ctu,
    Ctd,
    RTrig,
    FTrig,
    Sr,
    Rs,
}

/// Whether a program sets a member before the call or reads it after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

/// A member of a function block instance a program can name.
#[derive(Debug, Clone, Copy)]
pub struct Member {
    pub name: &'static str,
    pub offset: u32,
    pub ty: ElemType,
    pub direction: Direction,
}

const fn m(name: &'static str, offset: u32, ty: ElemType, direction: Direction) -> Member {
    Member {
        name,
        offset,
        ty,
        direction,
    }
}
use Direction::{Input as I, Output as O};
use ElemType::{Bool, Int, Time};

// Timers: IN@0 Q@1 running@2 last_in@3 PT@4 ET@12 start@20 => 28 bytes.
const TIMER: &[Member] = &[
    m("IN", 0, Bool, I),
    m("PT", 4, Time, I),
    m("Q", 1, Bool, O),
    m("ET", 12, Time, O),
];
// CTU: CU@0 R@1 Q@2 last@3 PV@4 CV@6 => 8. CTD: CD@0 LD@1 Q@2 last@3 PV@4 CV@6 => 8.
const CTU: &[Member] = &[
    m("CU", 0, Bool, I),
    m("R", 1, Bool, I),
    m("PV", 4, Int, I),
    m("Q", 2, Bool, O),
    m("CV", 6, Int, O),
];
const CTD: &[Member] = &[
    m("CD", 0, Bool, I),
    m("LD", 1, Bool, I),
    m("PV", 4, Int, I),
    m("Q", 2, Bool, O),
    m("CV", 6, Int, O),
];
// Edges: CLK@0 Q@1 M@2 => 3.
const TRIG: &[Member] = &[m("CLK", 0, Bool, I), m("Q", 1, Bool, O)];
// Bistables: S1/S@0 R/R1@1 Q1@2 => 3.
const SR: &[Member] = &[m("S1", 0, Bool, I), m("R", 1, Bool, I), m("Q1", 2, Bool, O)];
const RS: &[Member] = &[m("S", 0, Bool, I), m("R1", 1, Bool, I), m("Q1", 2, Bool, O)];

impl FbKind {
    pub const ALL: [FbKind; 9] = [
        FbKind::Ton,
        FbKind::Tof,
        FbKind::Tp,
        FbKind::Ctu,
        FbKind::Ctd,
        FbKind::RTrig,
        FbKind::FTrig,
        FbKind::Sr,
        FbKind::Rs,
    ];

    /// The name a program declares it by.
    pub const fn name(self) -> &'static str {
        match self {
            FbKind::Ton => "TON",
            FbKind::Tof => "TOF",
            FbKind::Tp => "TP",
            FbKind::Ctu => "CTU",
            FbKind::Ctd => "CTD",
            FbKind::RTrig => "R_TRIG",
            FbKind::FTrig => "F_TRIG",
            FbKind::Sr => "SR",
            FbKind::Rs => "RS",
        }
    }

    /// Its code in an `FB` instruction.
    pub const fn code(self) -> u8 {
        match self {
            FbKind::Ton => 1,
            FbKind::Tof => 2,
            FbKind::Tp => 3,
            FbKind::Ctu => 4,
            FbKind::Ctd => 5,
            FbKind::RTrig => 6,
            FbKind::FTrig => 7,
            FbKind::Sr => 8,
            FbKind::Rs => 9,
        }
    }

    pub fn from_code(code: u8) -> Option<FbKind> {
        FbKind::ALL.into_iter().find(|k| k.code() == code)
    }

    /// Bytes an instance takes.
    pub const fn size(self) -> u32 {
        match self {
            FbKind::Ton | FbKind::Tof | FbKind::Tp => 28,
            FbKind::Ctu | FbKind::Ctd => 8,
            FbKind::RTrig | FbKind::FTrig | FbKind::Sr | FbKind::Rs => 3,
        }
    }

    /// The members a program can name.
    pub const fn members(self) -> &'static [Member] {
        match self {
            FbKind::Ton | FbKind::Tof | FbKind::Tp => TIMER,
            FbKind::Ctu => CTU,
            FbKind::Ctd => CTD,
            FbKind::RTrig | FbKind::FTrig => TRIG,
            FbKind::Sr => SR,
            FbKind::Rs => RS,
        }
    }
}

fn get_i64(b: &[u8], at: usize) -> i64 {
    let mut x = [0u8; 8];
    x.copy_from_slice(&b[at..at + 8]);
    i64::from_le_bytes(x)
}
fn put_i64(b: &mut [u8], at: usize, v: i64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn get_i16(b: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([b[at], b[at + 1]])
}
fn put_i16(b: &mut [u8], at: usize, v: i16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

/// Run one call of `kind` on its instance `x` (exactly `kind.size()` bytes) at time `now`
/// (nanoseconds of a clock that never goes back).
pub(crate) fn run(kind: FbKind, x: &mut [u8], now: i64) {
    let b = |x: &[u8], at: usize| x[at] != 0;
    match kind {
        FbKind::Ton => {
            let (input, pt) = (b(x, 0), get_i64(x, 4));
            if !input {
                x[1] = 0;
                x[2] = 0;
                put_i64(x, 12, 0);
            } else {
                if !b(x, 2) {
                    x[2] = 1;
                    put_i64(x, 20, now);
                }
                let et = now.saturating_sub(get_i64(x, 20)).min(pt).max(0);
                put_i64(x, 12, et);
                x[1] = (et >= pt) as u8;
            }
            x[3] = input as u8;
        }
        FbKind::Tof => {
            let (input, pt) = (b(x, 0), get_i64(x, 4));
            if input {
                x[1] = 1;
                x[2] = 0;
                put_i64(x, 12, 0);
            } else {
                if b(x, 3) {
                    // falling edge of IN: the delay starts
                    x[2] = 1;
                    put_i64(x, 20, now);
                }
                if b(x, 2) {
                    let et = now.saturating_sub(get_i64(x, 20)).min(pt).max(0);
                    put_i64(x, 12, et);
                    if et >= pt {
                        x[1] = 0;
                        x[2] = 0;
                    }
                }
            }
            x[3] = input as u8;
        }
        FbKind::Tp => {
            let (input, pt) = (b(x, 0), get_i64(x, 4));
            if !b(x, 2) && input && !b(x, 3) {
                // rising edge while no pulse runs: a pulse starts
                x[2] = 1;
                x[1] = 1;
                put_i64(x, 20, now);
            }
            if b(x, 2) {
                let et = now.saturating_sub(get_i64(x, 20)).min(pt).max(0);
                put_i64(x, 12, et);
                if et >= pt {
                    x[1] = 0;
                    x[2] = 0;
                }
            }
            // ET holds PT after the pulse while IN is TRUE, and is 0 once both are over: also
            // in the call that ends the pulse, when IN is already FALSE.
            if !b(x, 2) && !input {
                put_i64(x, 12, 0);
            }
            x[3] = input as u8;
        }
        FbKind::Ctu => {
            let (cu, r, pv) = (b(x, 0), b(x, 1), get_i16(x, 4));
            let mut cv = get_i16(x, 6);
            if r {
                cv = 0;
            } else if cu && !b(x, 3) && cv < i16::MAX {
                cv += 1;
            }
            put_i16(x, 6, cv);
            x[2] = (cv >= pv) as u8;
            x[3] = cu as u8;
        }
        FbKind::Ctd => {
            let (cd, ld, pv) = (b(x, 0), b(x, 1), get_i16(x, 4));
            let mut cv = get_i16(x, 6);
            if ld {
                cv = pv;
            } else if cd && !b(x, 3) && cv > i16::MIN {
                cv -= 1;
            }
            put_i16(x, 6, cv);
            x[2] = (cv <= 0) as u8;
            x[3] = cd as u8;
        }
        FbKind::RTrig => {
            let clk = b(x, 0);
            x[1] = (clk && !b(x, 2)) as u8;
            x[2] = clk as u8;
        }
        FbKind::FTrig => {
            let clk = b(x, 0);
            x[1] = (!clk && b(x, 2)) as u8;
            x[2] = clk as u8;
        }
        FbKind::Sr => {
            let q = b(x, 0) || (!b(x, 1) && b(x, 2));
            x[2] = q as u8;
        }
        FbKind::Rs => {
            let q = !b(x, 1) && (b(x, 0) || b(x, 2));
            x[2] = q as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: i64 = 1_000_000;

    fn timer(kind: FbKind) -> [u8; 28] {
        let mut x = [0u8; 28];
        x[4..12].copy_from_slice(&(100 * MS).to_le_bytes());
        let _ = kind;
        x
    }
    fn q(x: &[u8]) -> bool {
        x[1] != 0
    }
    fn et(x: &[u8]) -> i64 {
        get_i64(x, 12)
    }

    #[test]
    fn ton_turns_on_once_in_has_been_true_for_pt_and_off_when_in_falls() {
        let mut x = timer(FbKind::Ton);
        x[0] = 1;
        run(FbKind::Ton, &mut x, 0);
        assert!(!q(&x));
        assert_eq!(et(&x), 0);
        run(FbKind::Ton, &mut x, 99 * MS);
        assert!(!q(&x));
        assert_eq!(et(&x), 99 * MS);
        run(FbKind::Ton, &mut x, 100 * MS);
        assert!(q(&x));
        run(FbKind::Ton, &mut x, 500 * MS);
        assert_eq!(et(&x), 100 * MS, "ET stops at PT");
        x[0] = 0;
        run(FbKind::Ton, &mut x, 501 * MS);
        assert!(!q(&x));
        assert_eq!(et(&x), 0);
    }

    #[test]
    fn tof_stays_on_for_pt_after_in_falls() {
        let mut x = timer(FbKind::Tof);
        run(FbKind::Tof, &mut x, 0);
        assert!(!q(&x), "never on before IN was");
        x[0] = 1;
        run(FbKind::Tof, &mut x, 10 * MS);
        assert!(q(&x));
        x[0] = 0;
        run(FbKind::Tof, &mut x, 20 * MS);
        assert!(q(&x));
        run(FbKind::Tof, &mut x, 119 * MS);
        assert!(q(&x));
        run(FbKind::Tof, &mut x, 120 * MS);
        assert!(!q(&x));
    }

    #[test]
    fn tp_gives_one_pulse_of_pt_whatever_in_does_meanwhile() {
        let mut x = timer(FbKind::Tp);
        x[0] = 1;
        run(FbKind::Tp, &mut x, 0);
        assert!(q(&x));
        x[0] = 0;
        run(FbKind::Tp, &mut x, 50 * MS);
        assert!(q(&x), "IN falling does not end the pulse");
        x[0] = 1;
        run(FbKind::Tp, &mut x, 60 * MS);
        assert!(q(&x), "a new edge does not restart it");
        run(FbKind::Tp, &mut x, 100 * MS);
        assert!(!q(&x));
        run(FbKind::Tp, &mut x, 200 * MS);
        assert!(!q(&x), "IN held TRUE makes no second pulse");
        assert_eq!(et(&x), 100 * MS, "ET holds PT while IN stays TRUE");
        x[0] = 0;
        run(FbKind::Tp, &mut x, 210 * MS);
        assert_eq!(et(&x), 0);
    }

    #[test]
    fn tp_et_is_zero_in_the_call_that_ends_the_pulse_when_in_is_already_false() {
        // Found by the differential test against STruC++ (timers_and_counters.st, ET_TP).
        let mut x = timer(FbKind::Tp);
        x[0] = 1;
        run(FbKind::Tp, &mut x, 0);
        x[0] = 0;
        run(FbKind::Tp, &mut x, 20 * MS);
        run(FbKind::Tp, &mut x, 100 * MS);
        assert!(!q(&x));
        assert_eq!(et(&x), 0);
    }

    #[test]
    fn ctu_counts_on_past_pv_up_to_the_types_limit() {
        // STruC++ (as OpenPLC/MatIEC) stops at PV instead. Which the standard says has not
        // been checked against its text: an open question in ADR-013 §2.13.
        let mut x = [0u8; 8];
        put_i16(&mut x, 4, 2);
        for k in 0..10 {
            x[0] = (k % 2 == 0) as u8;
            run(FbKind::Ctu, &mut x, 0);
        }
        assert_eq!(get_i16(&x, 6), 5);
        assert!(x[2] != 0);
        put_i16(&mut x, 6, i16::MAX);
        x[0] = 0;
        run(FbKind::Ctu, &mut x, 0);
        x[0] = 1;
        run(FbKind::Ctu, &mut x, 0);
        assert_eq!(get_i16(&x, 6), i16::MAX, "never wraps");
    }

    #[test]
    fn ctd_counts_on_below_zero_down_to_the_types_limit() {
        let mut x = [0u8; 8];
        for k in 0..6 {
            x[0] = (k % 2 == 0) as u8;
            run(FbKind::Ctd, &mut x, 0);
        }
        assert_eq!(get_i16(&x, 6), -3);
        assert!(x[2] != 0, "Q is CV <= 0");
    }

    #[test]
    fn ctu_counts_rising_edges_only_and_resets() {
        let mut x = [0u8; 8];
        put_i16(&mut x, 4, 2);
        for cu in [1, 1, 0, 1] {
            x[0] = cu;
            run(FbKind::Ctu, &mut x, 0);
        }
        assert_eq!(get_i16(&x, 6), 2);
        assert!(x[2] != 0);
        x[1] = 1;
        run(FbKind::Ctu, &mut x, 0);
        assert_eq!(get_i16(&x, 6), 0);
        assert!(x[2] == 0);
    }

    #[test]
    fn r_trig_is_true_for_one_call_per_rising_edge() {
        let mut x = [0u8; 3];
        let mut seen = [false; 5];
        for (i, clk) in [0, 1, 1, 0, 1].into_iter().enumerate() {
            x[0] = clk;
            run(FbKind::RTrig, &mut x, 0);
            seen[i] = x[1] != 0;
        }
        assert_eq!(seen, [false, true, false, false, true]);
    }

    #[test]
    fn every_member_lies_inside_its_instance() {
        for k in FbKind::ALL {
            for mem in k.members() {
                assert!(
                    mem.offset + mem.ty.size() <= k.size(),
                    "{} {}",
                    k.name(),
                    mem.name
                );
            }
            assert_eq!(FbKind::from_code(k.code()), Some(k));
        }
    }
}
