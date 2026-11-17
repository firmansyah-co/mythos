//! The MythOS logic compiler (ADR-013): the one call that takes source to something a target
//! runs. It owns no language and no target; it joins the crates that do.
//!
//! ```text
//! mythos-logic-st ──► mythos-logic-ir ──► target profile check ──► mythos-logic-backend-vm
//!  (a front-end           (the seam)        (this crate)             (a backend per
//!   per language)                                                      target kind)
//! ```

pub mod profile;

pub use mythos_logic_backend_vm::Symbol;
pub use mythos_logic_ir::{Diagnostic, Span};
pub use profile::TargetProfile;

/// The languages a program may be written in. One front-end crate each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Structured Text.
    St,
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

/// Parse and check `source`, written in `language`, into the IR.
pub fn lower(
    source: &str,
    language: Language,
) -> Result<mythos_logic_ir::Program, Vec<Diagnostic>> {
    match language {
        Language::St => mythos_logic_st::lower(source),
    }
}

/// Compile Structured Text `source` for `target`.
pub fn compile(source: &str, target: &TargetProfile) -> Result<Compiled, Vec<Diagnostic>> {
    compile_in(source, Language::St, target)
}

/// Compile `source`, written in `language`, for `target`.
pub fn compile_in(
    source: &str,
    language: Language,
    target: &TargetProfile,
) -> Result<Compiled, Vec<Diagnostic>> {
    let program = lower(source, language)?;
    let mut diags = Vec::new();
    if target.format_version != mythos_logic_backend_vm::FORMAT_VERSION {
        diags.push(Diagnostic::whole(format!(
            "the target {} runs bytecode format {}; this compiler makes format {}",
            target.name,
            target.format_version,
            mythos_logic_backend_vm::FORMAT_VERSION
        )));
    }
    for t in program.types_used() {
        if target.refuses.contains(&t) {
            diags.push(Diagnostic::whole(format!(
                "{} is not supported on the target {}",
                t.name(),
                target.name
            )));
        }
    }
    let out = mythos_logic_backend_vm::emit(&program);
    if out.memory_size > target.max_memory {
        diags.push(Diagnostic::whole(format!(
            "the program needs {} bytes of variable memory; the target {} allows {}",
            out.memory_size, target.name, target.max_memory
        )));
    }
    if out.code_size > target.max_code {
        diags.push(Diagnostic::whole(format!(
            "the program is {} bytes of code; the target {} allows {}",
            out.code_size, target.name, target.max_code
        )));
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
