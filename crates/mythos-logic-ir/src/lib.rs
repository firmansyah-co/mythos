//! The intermediate representation of MythOS control logic (ADR-013 §2.1).
//!
//! ```text
//!  front-ends (one crate per language)        backends (one crate per target kind)
//!  mythos-logic-st ─┐                      ┌─► mythos-logic-backend-vm ─► logic VM bytecode
//!  (LD, FBD, SFC    ┼──► mythos-logic-ir ──┤
//!   later)         ─┘                      └─► (C later)
//! ```
//!
//! This crate is the seam. It knows no language, no target and no VM: a front-end depends on
//! it and on nothing downstream, a backend on it and on nothing upstream. The IR is `serde`
//! data with a version, so a front-end and a backend need not live in one process.

mod fb;
mod ir;
mod keywords;

pub use fb::{Direction, FbMember, StdFb};
pub use ir::*;
pub use keywords::{is_keyword, KEYWORDS, TYPE_KEYWORDS};

/// The version of the IR's shape. A backend given another version refuses it.
///
/// 1: a unit of several program organization units (functions, function block types, one
/// program), structures and arrays; places with a path. 0 was one program of scalar variables.
pub const IR_VERSION: u32 = 1;

/// A stretch of a source text: from `line`:`col`, where it starts, up to `end_line`:`end_col`,
/// the first position after it. Lines and columns count from 1; a column counts characters
/// (Unicode scalar values), not bytes, and a tab is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Span {
    /// From where this one starts to where `last` ends.
    pub fn to(self, last: Span) -> Span {
        Span {
            end_line: last.end_line,
            end_col: last.end_col,
            ..self
        }
    }
}

/// A message about the source, and the stretch of it the message is about.
///
/// `line` and `col` say where that stretch starts, `end_line` and `end_col` where it ends:
/// all four count from 1, and the end is exclusive, so `3:8` to `3:9` is the one character at
/// line 3, column 8 (an editor underlines from the start up to, not including, the end). A
/// column counts characters, not bytes. The end equals the start when the message is about a
/// point with nothing to underline, such as the end of the source. All four are 0 when the
/// message is about the whole program and no place in it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub message: String,
}

impl Diagnostic {
    pub fn at(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            line: span.line,
            col: span.col,
            end_line: span.end_line,
            end_col: span.end_col,
            message: message.into(),
        }
    }

    /// About the program as a whole, not a place in it.
    pub fn whole(message: impl Into<String>) -> Self {
        Diagnostic {
            line: 0,
            col: 0,
            end_line: 0,
            end_col: 0,
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
