//! The Structured Text front-end (ADR-013 §2.1): IEC 61131-3 ST to the IR.
//!
//! What it takes today is a subset, and everything outside it is a diagnostic, never a guess:
//! in any order, TYPE blocks (STRUCT, and names for ARRAY types), functions, function block
//! types and one PROGRAM; VAR, VAR_INPUT and VAR_OUTPUT; BOOL, SINT, INT, DINT, LINT, REAL,
//! LREAL, TIME, structures and arrays of any of them, and instances of function blocks;
//! assignment (of a whole structure or array too), IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN;
//! arithmetic, comparison and BOOL logic; calls of functions, by position or by name; the
//! `*_TO_*` conversions between the elementary types; and calls of function block instances,
//! the standard ones and the source's own. Recursion is refused.
//!
//! It depends on `mythos-logic-ir` alone. Another language is another crate like this one.

pub mod ast;
pub mod check;
pub mod lexer;
pub mod parser;

use mythos_logic_ir::{Diagnostic, Unit};

/// Parse and check `source` into the IR.
pub fn lower(source: &str) -> Result<Unit, Vec<Diagnostic>> {
    let toks = lexer::lex(source).map_err(|d| vec![d])?;
    let ast = parser::Parser::new(toks).source().map_err(|d| vec![d])?;
    check::check(&ast)
}
