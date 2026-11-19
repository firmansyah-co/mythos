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
//! The standard function blocks are a library written in ST ([`library`]); a program is
//! compiled with the POUs of it that it uses.
//!
//! It depends on `mythos-logic-ir` alone. Another language is another crate like this one.

pub mod ast;
pub mod check;
pub mod lexer;
pub mod library;
pub mod parser;

use mythos_logic_ir::{Diagnostic, Unit};

/// Parse and check `source` into the IR, or say everything that is wrong with it.
///
/// The diagnostics are every independent mistake found in one pass, in the order of the
/// source (those about the program as a whole first). A mistake is told once: the lexer, the
/// parser and the checker each stay silent about what only follows from something already
/// told. A POU that could not be read whole has its declarations checked and its statements
/// not; every other POU is checked in full.
///
/// The POUs of the standard library the source uses are compiled with it. A diagnostic is
/// never about a place in the library: one would be the compiler's fault, and says so about
/// the whole program ([`library::as_the_compilers`]).
pub fn lower(source: &str) -> Result<Unit, Vec<Diagnostic>> {
    let (ast, diags) = parser::Parser::new(lexer::tokens(source)).source();
    lower_tree(ast, diags)
}

/// Check a syntax tree into the IR: `ast`, with what reading it said in `diags`.
///
/// Another front-end that builds trees of POUs (Ladder Diagram, ADR-013 §2.15) adds them to
/// the tree of a Structured Text source and lowers both here, so that the POUs of the two
/// languages call one another, the standard library is added for both, and every rule of
/// declarations, types and calls is the one rule ST has. Its places are lines it chose,
/// apart from any of a source (from `2^31`) and of the library; it maps them back.
pub fn lower_tree(
    mut ast: ast::Source,
    mut diags: Vec<Diagnostic>,
) -> Result<Unit, Vec<Diagnostic>> {
    library::add_used(&mut ast, &mut diags);
    match check::check(&ast) {
        Ok(unit) if diags.is_empty() => return Ok(unit),
        Ok(_) => {}
        Err(more) => diags.extend(more),
    }
    let mut diags = library::as_the_compilers(diags);
    diags.sort_by_key(|d| (d.line, d.col));
    Err(diags)
}
