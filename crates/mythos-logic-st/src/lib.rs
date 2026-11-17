//! The Structured Text front-end (ADR-013 §2.1): IEC 61131-3 ST to the IR.
//!
//! What it takes today is a subset, and everything outside it is a diagnostic, never a guess:
//! in any order, TYPE blocks (STRUCT, and names for ARRAY types), functions, function block
//! types and one PROGRAM; VAR, VAR_INPUT and VAR_OUTPUT; BOOL, SINT, INT, DINT, LINT, REAL,
//! LREAL, TIME, structures and arrays of any of them, and instances of function blocks;
//! assignment (of a whole structure or array too), IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN;
//! arithmetic, comparison and BOOL logic; calls of functions, by position or by name; the
//! `*_TO_*` conversions between the elementary types; and calls of function block instances,
//! the standard ones and the source's own. Recursion is refused. A keyword of the standard is
//! refused as a name.
//!
//! It depends on `mythos-logic-ir` alone. Another language is another crate like this one.

pub mod ast;
pub mod check;
pub mod lexer;
pub mod parser;

use mythos_logic_ir::{Diagnostic, Unit};

/// Parse and check `source` into the IR, or say everything that is wrong with it.
///
/// The diagnostics are every independent mistake found in one pass, in the order of the
/// source (those about the program as a whole first). A mistake is told once: the lexer, the
/// parser and the checker each stay silent about what only follows from something already
/// told. A POU that could not be read whole has its declarations checked and its statements
/// not; every other POU is checked in full.
pub fn lower(source: &str) -> Result<Unit, Vec<Diagnostic>> {
    let (ast, mut diags) = parser::Parser::new(lexer::tokens(source)).source();
    match check::check(&ast) {
        Ok(unit) if diags.is_empty() => return Ok(unit),
        Ok(_) => {}
        Err(more) => diags.extend(more),
    }
    diags.sort_by_key(|d| (d.line, d.col));
    Err(diags)
}
