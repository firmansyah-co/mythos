//! The intermediate representation of MythOS control logic (ADR-013 §2.1).
//!
//! ```text
//!  front-ends (one crate per language)        backends (one crate per target kind)
//!  mythos-logic-st ─┐                      ┌─► mythos-logic-backend-vm ─► logic VM bytecode
//!  (LD, FBD, SFC,   ┼──► mythos-logic-ir ──┤
//!   IL later)      ─┘                      └─► (C later)
//! ```
//!
//! This crate is the seam. It knows no language, no target and no VM: a front-end depends on
//! it and on nothing downstream, a backend on it and on nothing upstream. The IR is `serde`
//! data with a version, so a front-end and a backend need not live in one process.

mod fb;
mod ir;

pub use fb::{Direction, FbMember, StdFb};
pub use ir::*;

/// The version of the IR's shape. A backend given another version refuses it.
pub const IR_VERSION: u32 = 0;

/// A place in a source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

/// A message about the source, at a line and column (0:0 when it is about the whole program).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub line: u32,
    pub col: u32,
    pub message: String,
}

impl Diagnostic {
    pub fn at(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            line: span.line,
            col: span.col,
            message: message.into(),
        }
    }

    /// About the program as a whole, not a place in it.
    pub fn whole(message: impl Into<String>) -> Self {
        Diagnostic {
            line: 0,
            col: 0,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

/// How a variable is declared: what the outside may do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Var,
    Input,
    Output,
}
