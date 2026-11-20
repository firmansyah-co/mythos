//! The MythOS logic compiler (ADR-013): the one call that takes source to something a target
//! runs. It owns no language and no target; it joins the crates that do.
//!
//! ```text
//! mythos-logic-st ──────► mythos-logic-ir ──► target profile check ──► mythos-logic-backend-vm
//! mythos-logic-graphic ─┘   (the seam)         (this crate)             (a backend per
//!  (a front-end per language; LD and FBD                                  target kind)
//!   share one, as they share the TC6 document)
//! ```
//!
//! A program is compiled from one source ([`compile`], [`compile_in`]) or from several in
//! different languages ([`compile_sources`]): a Structured Text source with types, functions
//! and function blocks, and PLCopen TC6 documents whose POUs are Ladder Diagrams or Function
//! Block Diagrams, one POU of all of them the PROGRAM. Each POU may call those of the others
//! and the standard library (ADR-013 §2.15, §2.16).

pub mod profile;

pub use mythos_logic_backend_vm::{ArrayInfo, MemberLayout, Symbol, TypeLayout};
pub use mythos_logic_graphic::{At, Holds, Probe};
use mythos_logic_ir::Ty;
pub use mythos_logic_ir::{Diagnostic, Element, Span, TextRange};
pub use profile::TargetProfile;

/// The languages a program may be written in. One front-end crate each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Structured Text.
    St,
    /// Ladder Diagram, as a PLCopen TC6 XML v2.01 document (ADR-013 §2.15).
    Ld,
    /// Function Block Diagram, as a PLCopen TC6 XML v2.01 document (ADR-013 §2.16).
    Fbd,
    /// Sequential Function Chart, as a PLCopen TC6 XML v2.01 document (ADR-013 §2.17).
    Sfc,
}

/// The sources of one program: at most one Structured Text source, and PLCopen TC6
/// documents. Each is named for the diagnostics, which say which source a line is in when
/// there is more than one.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    /// `(name, text)` of the Structured Text source.
    pub st: Option<(&'a str, &'a str)>,
    /// `(name, text)` of each TC6 document: its POUs' bodies are in LD, FBD or SFC, one
    /// document may hold all three, and a POU of any may call one of the others.
    pub tc6: &'a [(&'a str, &'a str)],
}

/// A compiled program.
pub struct Compiled {
    pub name: String,
    /// The program in the VM's format (`mythos_logic_vm::Program::load` reads it).
    pub bytecode: Vec<u8>,
    /// Where each variable is, for binding, monitoring and forcing.
    pub symbols: Vec<Symbol>,
    /// How each structure and function block type is laid out, for reaching into the
    /// elements of an array of them.
    pub types: Vec<TypeLayout>,
    pub memory_size: u32,
    pub code_size: u32,
    pub stack_depth: u16,
    /// How deep the program's calls nest; the VM is given that many call frames.
    pub call_depth: u16,
    /// For each output, junction and network of a Ladder Diagram, the hidden variable that
    /// keeps its power, and for each output and network of a Function Block Diagram, the one
    /// that keeps its value, each assigned when it is evaluated in the scan (ADR-013 §2.15,
    /// §2.16), and for each step, transition and action of a Sequential Function Chart, the
    /// variable keeping its state (§2.17): what an editor shows power flow and values from.
    /// Empty for a program of Structured Text alone.
    pub probes: Vec<Probe>,
}

impl Compiled {
    pub fn symbol(&self, path: &str) -> Option<&Symbol> {
        self.symbols
            .iter()
            .find(|s| s.path.eq_ignore_ascii_case(path))
    }
}

/// Parse and check `source`, written in `language`, into the IR.
pub fn lower(source: &str, language: Language) -> Result<mythos_logic_ir::Unit, Vec<Diagnostic>> {
    match language {
        Language::St => mythos_logic_st::lower(source),
        Language::Ld | Language::Fbd | Language::Sfc => mythos_logic_graphic::lower(
            &[mythos_logic_graphic::Source {
                name: "",
                text: source,
            }],
            None,
        ),
    }
}

/// Parse and check `sources` into one unit of the IR.
pub fn lower_sources(sources: &Sources) -> Result<mythos_logic_ir::Unit, Vec<Diagnostic>> {
    lower_sources_with_probes(sources).map(|(unit, _)| unit)
}

/// [`lower_sources`], with the probes of the graphical POUs
/// ([`mythos_logic_graphic::lower_with_probes`]).
fn lower_sources_with_probes(
    sources: &Sources,
) -> Result<(mythos_logic_ir::Unit, Vec<Probe>), Vec<Diagnostic>> {
    let ld: Vec<mythos_logic_graphic::Source> = sources
        .tc6
        .iter()
        .map(|(name, text)| mythos_logic_graphic::Source { name, text })
        .collect();
    let st = sources
        .st
        .map(|(name, text)| mythos_logic_graphic::Source { name, text });
    if ld.is_empty() {
        let (_, text) = sources.st.unwrap_or(("", ""));
        return mythos_logic_st::lower(text).map(|unit| (unit, Vec::new()));
    }
    mythos_logic_graphic::lower_with_probes(&ld, st.as_ref())
}

/// Compile a program from `sources` for `target`.
pub fn compile_sources(
    sources: &Sources,
    target: &TargetProfile,
) -> Result<Compiled, Vec<Diagnostic>> {
    let (unit, probes) = lower_sources_with_probes(sources)?;
    let mut compiled = compile_unit(unit, target)?;
    compiled.probes = probes;
    Ok(compiled)
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
    match language {
        Language::St => compile_unit(lower(source, language)?, target),
        Language::Ld | Language::Fbd | Language::Sfc => compile_sources(
            &Sources {
                st: None,
                tc6: &[("", source)],
            },
            target,
        ),
    }
}

/// Compile a unit of the IR for `target`.
fn compile_unit(
    unit: mythos_logic_ir::Unit,
    target: &TargetProfile,
) -> Result<Compiled, Vec<Diagnostic>> {
    let mut diags = Vec::new();
    if target.format_version != mythos_logic_backend_vm::FORMAT_VERSION {
        diags.push(Diagnostic::whole(format!(
            "the target {} runs bytecode format {}; this compiler makes format {}",
            target.name,
            target.format_version,
            mythos_logic_backend_vm::FORMAT_VERSION
        )));
    }
    for t in unit.types_used() {
        if target.refuses.iter().any(|r| r.same_kind(t)) {
            // A kind of type: every length of a STRING is refused with it.
            let what = match t {
                Ty::String(_) => "STRING".to_string(),
                t => unit.ty_name(t),
            };
            diags.push(Diagnostic::whole(format!(
                "{what} is not supported on the target {}",
                target.name
            )));
        }
    }
    let too_much = |needed: u64| {
        Diagnostic::whole(format!(
            "the program needs {needed} bytes of variable memory; the target {} allows {}",
            target.name, target.max_memory
        ))
    };
    // Before anything is emitted: an array of a million elements is one line of source, and
    // its memory image should not be built only to be refused.
    match mythos_logic_backend_vm::variable_memory(&unit) {
        Ok(needed) if needed > target.max_memory as u64 => {
            diags.push(too_much(needed));
            return Err(diags);
        }
        Ok(_) => {}
        Err(d) => {
            diags.push(d);
            return Err(diags);
        }
    }
    let out = match mythos_logic_backend_vm::emit(&unit) {
        Ok(out) => out,
        Err(d) => {
            diags.push(d);
            return Err(diags);
        }
    };
    if out.memory_size > target.max_memory {
        diags.push(too_much(out.memory_size as u64));
    }
    if out.code_size > target.max_code {
        diags.push(Diagnostic::whole(format!(
            "the program is {} bytes of code; the target {} allows {}",
            out.code_size, target.name, target.max_code
        )));
    }
    // What the VM would find at the program's start on that target, found here instead.
    if let Some(max) = target.max_stack_depth.filter(|max| out.stack_depth > *max) {
        diags.push(Diagnostic::whole(format!(
            "the program needs a stack {} values deep; the target {} allows {max}",
            out.stack_depth, target.name
        )));
    }
    if let Some(max) = target.max_call_depth.filter(|max| out.call_depth > *max) {
        diags.push(Diagnostic::whole(format!(
            "the program nests its calls {} deep; the target {} allows {max}",
            out.call_depth, target.name
        )));
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(Compiled {
        name: unit.program().name.clone(),
        bytecode: out.bytes,
        symbols: out.symbols,
        types: out.types,
        memory_size: out.memory_size,
        code_size: out.code_size,
        stack_depth: out.stack_depth,
        call_depth: out.call_depth,
        probes: Vec::new(),
    })
}
