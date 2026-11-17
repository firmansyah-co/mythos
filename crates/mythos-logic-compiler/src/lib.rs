//! The MythOS logic compiler (ADR-013).
//!
//! ```text
//! Structured Text ──lex/parse──► syntax tree ──check──► IR ──backend──► VM bytecode
//!  (LD, FBD, SFC, IL later)                             │   (C later)
//!                                              target profile check
//! ```
//!
//! What it takes today is a subset of IEC 61131-3 ed. 3, and everything outside it is a
//! diagnostic, never a guess: one PROGRAM with VAR, VAR_INPUT and VAR_OUTPUT; BOOL, SINT,
//! INT, DINT, LINT, REAL, LREAL, TIME; assignment, IF, CASE, FOR, WHILE, REPEAT, EXIT,
//! RETURN; arithmetic, comparison and BOOL logic; the `*_TO_*` conversions between those
//! types; and the standard function blocks TON, TOF, TP, CTU, CTD, R_TRIG, F_TRIG, SR, RS.

pub mod ast;
pub mod bytecode;
pub mod check;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod profile;

pub use bytecode::Symbol;
pub use lexer::Span;
pub use profile::TargetProfile;

/// A message about the source, at a line and column.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

/// A compiled program.
pub struct Compiled {
    pub name: String,
    /// The program in the VM's format (`mythos_logic_vm::Program::load` reads it).
    pub bytecode: Vec<u8>,
    /// Where each variable is, for binding, monitoring and forcing.
    pub symbols: Vec<Symbol>,
    pub memory_size: u32,
    pub code_size: u32,
    pub stack_depth: u16,
}

impl Compiled {
    pub fn symbol(&self, path: &str) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|s| s.path.eq_ignore_ascii_case(path))
    }
}

/// Parse and check `source` into the IR.
pub fn lower(source: &str) -> Result<ir::Program, Vec<Diagnostic>> {
    let toks = lexer::lex(source).map_err(|d| vec![d])?;
    let ast = parser::Parser::new(toks).program().map_err(|d| vec![d])?;
    check::check(&ast)
}

/// Compile `source` for `target`.
pub fn compile(source: &str, target: &TargetProfile) -> Result<Compiled, Vec<Diagnostic>> {
    let program = lower(source)?;
    let mut diags = Vec::new();
    for t in program.types_used() {
        if target.refuses.contains(&t) {
            diags.push(Diagnostic {
                line: 0,
                col: 0,
                message: format!(
                    "{} is not supported on the target {}",
                    t.name(),
                    target.name
                ),
            });
        }
    }
    let out = bytecode::emit(&program);
    if out.memory_size > target.max_memory {
        diags.push(Diagnostic {
            line: 0,
            col: 0,
            message: format!(
                "the program needs {} bytes of variable memory; the target {} allows {}",
                out.memory_size, target.name, target.max_memory
            ),
        });
    }
    if out.code_size > target.max_code {
        diags.push(Diagnostic {
            line: 0,
            col: 0,
            message: format!(
                "the program is {} bytes of code; the target {} allows {}",
                out.code_size, target.name, target.max_code
            ),
        });
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(Compiled {
        name: program.name,
        bytecode: out.bytes,
        symbols: out.symbols,
        memory_size: out.memory_size,
        code_size: out.code_size,
        stack_depth: out.stack_depth,
    })
}
