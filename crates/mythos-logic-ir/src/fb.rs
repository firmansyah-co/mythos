//! The standard function blocks of IEC 61131-3, as a program sees them: a name and members
//! with a type and a direction. How an instance is stored and how the block runs is a
//! backend's business; this catalog says only what a front-end may check a program against.
//!
//! When user function blocks exist, these become a library written in the IEC languages and
//! this catalog goes away (ADR-013 §2.1).

use crate::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StdFb {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy)]
pub struct FbMember {
    pub name: &'static str,
    pub ty: Ty,
    pub direction: Direction,
}

const fn m(name: &'static str, ty: Ty, direction: Direction) -> FbMember {
    FbMember {
        name,
        ty,
        direction,
    }
}
use Direction::{Input as I, Output as O};

const TIMER: &[FbMember] = &[
    m("IN", Ty::Bool, I),
    m("PT", Ty::Time, I),
    m("Q", Ty::Bool, O),
    m("ET", Ty::Time, O),
];
const CTU: &[FbMember] = &[
    m("CU", Ty::Bool, I),
    m("R", Ty::Bool, I),
    m("PV", Ty::Int, I),
    m("Q", Ty::Bool, O),
    m("CV", Ty::Int, O),
];
const CTD: &[FbMember] = &[
    m("CD", Ty::Bool, I),
    m("LD", Ty::Bool, I),
    m("PV", Ty::Int, I),
    m("Q", Ty::Bool, O),
    m("CV", Ty::Int, O),
];
const TRIG: &[FbMember] = &[m("CLK", Ty::Bool, I), m("Q", Ty::Bool, O)];
const SR: &[FbMember] = &[
    m("S1", Ty::Bool, I),
    m("R", Ty::Bool, I),
    m("Q1", Ty::Bool, O),
];
const RS: &[FbMember] = &[
    m("S", Ty::Bool, I),
    m("R1", Ty::Bool, I),
    m("Q1", Ty::Bool, O),
];

impl StdFb {
    pub const ALL: [StdFb; 9] = [
        StdFb::Ton,
        StdFb::Tof,
        StdFb::Tp,
        StdFb::Ctu,
        StdFb::Ctd,
        StdFb::RTrig,
        StdFb::FTrig,
        StdFb::Sr,
        StdFb::Rs,
    ];

    /// The name a program declares it by.
    pub const fn name(self) -> &'static str {
        match self {
            StdFb::Ton => "TON",
            StdFb::Tof => "TOF",
            StdFb::Tp => "TP",
            StdFb::Ctu => "CTU",
            StdFb::Ctd => "CTD",
            StdFb::RTrig => "R_TRIG",
            StdFb::FTrig => "F_TRIG",
            StdFb::Sr => "SR",
            StdFb::Rs => "RS",
        }
    }

    /// The members a program can name.
    pub const fn members(self) -> &'static [FbMember] {
        match self {
            StdFb::Ton | StdFb::Tof | StdFb::Tp => TIMER,
            StdFb::Ctu => CTU,
            StdFb::Ctd => CTD,
            StdFb::RTrig | StdFb::FTrig => TRIG,
            StdFb::Sr => SR,
            StdFb::Rs => RS,
        }
    }
}
