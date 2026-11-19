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

mod ir;
mod keywords;

pub use ir::*;
pub use keywords::{is_keyword, KEYWORDS, TYPE_KEYWORDS};

/// The version of the IR's shape. A backend given another version refuses it.
///
/// 6: [`Var::hidden`], a variable a language made for itself (the edge memory of a Ladder
/// Diagram contact), which the host may read and never write.
/// 5: no standard function blocks of its own: TON, CTU and the rest are function block types
/// of the unit, written in Structured Text (ADR-013 L4); [`Intrinsic`] for what they need
/// that no language expresses.
/// 4: each variable's [`Retention`].
/// 3: the numeric and selection functions of the standard library in [`StdFn`].
/// 2: STRING, string literals, and the functions of the standard library ([`StdFn`]).
/// 1: a unit of several program organization units (functions, function block types, one
/// program), structures and arrays; places with a path. 0 was one program of scalar variables.
pub const IR_VERSION: u32 = 6;

/// The length of a STRING declared without one. IEC 61131-3 ed. 3 gives no default:
/// Table 10 footnote k and §6.4.2.2 leave the maximum length to the implementer, and the
/// declaration may state one (`STRING[10]`). 254 is what STruC++ (the differential oracle,
/// ADR-013 §2.12) and most implementations take, so a program moved between them keeps it.
pub const DEFAULT_STRING_LENGTH: u16 = 254;

/// The longest STRING there may be: what a declaration may state, and the longest value a
/// string function may give (IEC 61131-3 ed. 3 §6.6.2.5.11: producing a longer one is an
/// error). Implementer specific (Table 10 footnote k); chosen so that `LEN`, which gives an
/// INT, can always say the length.
pub const MAX_STRING_LENGTH: u16 = i16::MAX as u16;

/// How deep a front-end lets constructs of a body nest within one another: statements within
/// statements, and within one statement, operations, calls, subscripts and parentheses. An
/// operation counts once for each operator of a chain (`a + b + c` is two deep), because the
/// tree a chain makes is that deep. A source nested deeper is refused with a diagnostic.
///
/// Every pass over a unit (checking, lowering, a backend's walk) is recursive, and a compiler
/// runs inside the logic module's service process: a source must never be able to exhaust
/// the stack it is compiled on. The smallest one is 2 MiB (a tokio worker thread, a Rust test
/// thread). `deep_sources_are_refused.rs` compiles the deepest source of each kind allowed on
/// a thread of half that, in a debug build, where frames are largest.
pub const MAX_NESTING: usize = 100;

/// How deep a front-end lets types contain one another: a structure, an array or a function
/// block instance within another. A type nested deeper is refused with a diagnostic, for the
/// same reason as [`MAX_NESTING`].
pub const MAX_TYPE_NESTING: usize = 100;

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
///
/// A graphical language (Ladder Diagram, ADR-013 §2.15) stores a POU as a PLCopen TC6 XML
/// document, whose lines say little about the diagram. A diagnostic about such a POU says
/// which part of it, in [`Element`]: the element of the body by its `localId` and position,
/// or the declaration of the interface by its name. Its `line` and `col` are then where that
/// element's tag starts in the document, so a text editor can still point at it. Both
/// fields are absent for Structured Text, whose JSON is what it was.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub message: String,
    /// The source `line` and `col` are in, by the name the caller gave it, when a program is
    /// compiled from several (a Structured Text source and Ladder Diagram documents). Absent
    /// when it is compiled from one, or the message is about the whole program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The part of a graphical POU the message is about. Absent for a textual language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<Box<Element>>,
}

/// Where in a graphical POU a diagnostic is: an element of its body, or a declaration of its
/// interface.
///
/// ```json
/// {"pou": "Main", "local_id": 7, "x": 120, "y": 40,
///  "text": {"field": "variable", "col": 1, "end_col": 6}}
/// {"pou": "Main", "variable": "count", "text": {"field": "initialValue", "col": 1, "end_col": 4}}
/// ```
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Element {
    /// The POU, by its name.
    pub pou: String,
    /// The element of the body, by its `localId` (TC6 `localId`, unique in the body). Absent
    /// for a declaration of the interface and for the body as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_id: Option<u64>,
    /// The element's position in the diagram, as the document gives it (TC6 `position`, its
    /// `x` and `y`, the element's top left corner). Absent when there is no element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    /// The variable of the interface the message is about, by its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
    /// The text of the element or declaration the message is about, and which characters of
    /// it. Absent when it is about the element as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextRange>,
}

/// Characters of one text of an element: `field` names the text by the TC6 element or
/// attribute that holds it (`variable` of a contact or a coil, `expression` of a variable,
/// `instanceName` and `typeName` of a block, `type` and `initialValue` of a declaration, a
/// block's input by its formal parameter). `col` and `end_col` count characters from 1, the
/// end exclusive, as [`Diagnostic`]'s columns do.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextRange {
    pub field: String,
    pub col: u32,
    pub end_col: u32,
}

impl Diagnostic {
    pub fn at(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            line: span.line,
            col: span.col,
            end_line: span.end_line,
            end_col: span.end_col,
            message: message.into(),
            source: None,
            element: None,
        }
    }

    /// About the program as a whole, not a place in it.
    pub fn whole(message: impl Into<String>) -> Self {
        Diagnostic::at(Span::default(), message)
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(source) = &self.source {
            write!(f, "{source}:")?;
        }
        write!(f, "{}:{}: ", self.line, self.col)?;
        if let Some(e) = &self.element {
            write!(f, "{}", e.pou)?;
            if let Some(id) = e.local_id {
                write!(f, " element {id}")?;
            }
            if let Some(v) = &e.variable {
                write!(f, " variable {v}")?;
            }
            if let Some(t) = &e.text {
                write!(f, " {} {}..{}", t.field, t.col, t.end_col)?;
            }
            write!(f, ": ")?;
        }
        write!(f, "{}", self.message)
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

/// The retentive qualifier of the section a variable is declared in (IEC 61131-3 ed. 3
/// §6.5.6, Figure 7): what a warm restart does to it.
///
/// A RETAIN variable keeps the value it had when the program stopped; a NON_RETAIN one starts
/// at its initial value. The standard leaves a variable of an unqualified section to the
/// implementer (§6.5.6.1 rule 3); MythOS initializes it, as NON_RETAIN. The two differ only
/// for a function block instance: the members of an instance declared in an unqualified
/// section follow their type's own declarations ([`Unit::member_retention`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    #[default]
    Unqualified,
    Retain,
    NonRetain,
}

impl Retention {
    /// Whether a variable of this retention keeps its value across a warm restart.
    pub fn retained(self) -> bool {
        self == Retention::Retain
    }
}
