//! The Structured Text front-end (ADR-013 §2.1): IEC 61131-3 ST to the IR.
//!
//! What it takes today is a subset, and everything outside it is a diagnostic, never a guess:
//! one PROGRAM with VAR, VAR_INPUT and VAR_OUTPUT; BOOL, SINT, INT, DINT, LINT, REAL, LREAL,
//! TIME; assignment, IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN; arithmetic, comparison and
//! BOOL logic; the `*_TO_*` conversions between those types; and calls of the standard
//! function blocks.
//!
//! It depends on `mythos-logic-ir` alone. Another language is another crate like this one.

pub mod ast;
pub mod check;
pub mod lexer;
pub mod parser;

use mythos_logic_ir::{Diagnostic, Program};

/// Parse and check `source` into the IR.
pub fn lower(source: &str) -> Result<Program, Vec<Diagnostic>> {
    let toks = lexer::lex(source).map_err(|d| vec![d])?;
    let ast = parser::Parser::new(toks).program().map_err(|d| vec![d])?;
    check::check(&ast)
}
