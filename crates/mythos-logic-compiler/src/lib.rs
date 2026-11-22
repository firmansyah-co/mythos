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
pub use mythos_logic_graphic::{At, Body, BodyOf, Holds, Probe};
use mythos_logic_ir::Ty;
pub use mythos_logic_ir::{Diagnostic, Element, Span, TextRange};
pub use mythos_logic_st::DataBlock;
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
/// documents; and of its project, the library sources and the data blocks (ADR-014, slice
/// D1). Each source is named for the diagnostics, which say which source a line is in when
/// there is more than one.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    /// `(name, text)` of the Structured Text source.
    pub st: Option<(&'a str, &'a str)>,
    /// `(name, text)` of each TC6 document: its POUs' bodies are in LD, FBD or SFC, one
    /// document may hold all three, and a POU of any may call one of the others.
    pub tc6: &'a [(&'a str, &'a str)],
    /// `(name, text)` of each Structured Text source of the project's types and library:
    /// TYPE blocks, functions and function block types, no PROGRAM. Every one is checked
    /// whole; what the program does not use of them adds nothing to what is compiled.
    pub library: &'a [(&'a str, &'a str)],
    /// The data blocks of the project.
    pub data: &'a [DataBlock<'a>],
}

impl<'a> Sources<'a> {
    fn project(&self) -> mythos_logic_st::Project<'a> {
        mythos_logic_st::Project {
            library: self.library,
            data: self.data,
        }
    }
}

/// One data block a program declares in `VAR_EXTERNAL`: where the program's own copy of it
/// is, how it is laid out, and which of its members the program writes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataUse {
    /// As the project declares it (not as the program spells it).
    pub name: String,
    /// Its type's name, as the project declares it.
    #[serde(rename = "type")]
    pub ty: String,
    /// The layout hash: 16 lowercase hex digits (`mythos_logic_backend_vm::layout_hash`).
    pub layout: String,
    /// Of the program's copy, in the program's memory.
    pub offset: u32,
    pub size: u32,
    /// Declared in `VAR_EXTERNAL CONSTANT`.
    pub constant: bool,
    /// The top-level members, in declaration order.
    pub members: Vec<DataMember>,
}

/// A top-level member of a data block.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataMember {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Inside the block.
    pub offset: u32,
    pub size: u32,
    /// The program stores into it or into a part of it: decided from the IR
    /// ([`mythos_logic_ir::Unit::stores`]), so alike for every language. A store that cannot
    /// be told to be within one member makes every member written.
    pub written: bool,
}

/// One data block of the project as laid out, with no program.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataLayout {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub layout: String,
    pub size: u32,
    /// Each `written` is false.
    pub members: Vec<DataMember>,
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
    /// Each data block the program declares in `VAR_EXTERNAL`, in the order of the project's
    /// declaration ([`Sources::data`]). Empty for a program that declares none.
    pub data: Vec<DataUse>,
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
    let project = sources.project();
    if ld.is_empty() {
        if project.is_empty() {
            let (_, text) = sources.st.unwrap_or(("", ""));
            return mythos_logic_st::lower(text).map(|unit| (unit, Vec::new()));
        }
        return mythos_logic_st::lower_project(sources.st, &project).map(|unit| (unit, Vec::new()));
    }
    if project.is_empty() {
        return mythos_logic_graphic::lower_with_probes(&ld, st.as_ref());
    }
    mythos_logic_graphic::lower_project_with_probes(&ld, st.as_ref(), &project)
}

/// Check the library sources and the data block declarations of `sources` alone (its `st`
/// and `tc6` are not looked at), and lay the blocks out, each as a program's copy of it is
/// laid out, in the order of [`Sources::data`].
///
/// Every type and POU of the library is checked and laid out whether a block uses it or not,
/// so one that no program could ever hold is refused here; each block is refused only for
/// what it is alone ([`mythos_logic_backend_vm::data_layouts`]). A program compiled in the
/// project is refused for whatever this refuses ([`compile_sources`]).
pub fn check_project(sources: &Sources) -> Result<Vec<DataLayout>, Vec<Diagnostic>> {
    let unit = mythos_logic_st::check_project(&sources.project())?;
    let externals = mythos_logic_backend_vm::data_layouts(&unit).map_err(|d| vec![d])?;
    let data = data_uses(&unit, &externals, sources.data);
    Ok(data
        .into_iter()
        .map(|d| DataLayout {
            name: d.name,
            ty: d.ty,
            layout: d.layout,
            size: d.size,
            members: d
                .members
                .into_iter()
                .map(|m| DataMember {
                    written: false,
                    ..m
                })
                .collect(),
        })
        .collect())
}

/// Each data block of `data` the program of `unit` declares, as `externals` lays it out, with
/// which top-level members the program writes.
fn data_uses(
    unit: &mythos_logic_ir::Unit,
    externals: &[mythos_logic_backend_vm::ExternalLayout],
    data: &[DataBlock],
) -> Vec<DataUse> {
    let stores = unit.stores(unit.entry);
    let vars = &unit.program().vars;
    let mut out = Vec::new();
    for block in data {
        let Some(e) = externals
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(block.name))
        else {
            continue;
        };
        // Within the block, every store is within one member or may reach all of them.
        let mut written = vec![false; e.members.len()];
        for s in stores.iter().filter(|s| s.var == e.var) {
            match s.member {
                Some(m) if m < written.len() => written[m] = true,
                Some(_) | None => written.iter_mut().for_each(|w| *w = true),
            }
        }
        out.push(DataUse {
            name: block.name.to_string(),
            ty: block.ty.to_string(),
            layout: e.layout.clone(),
            offset: e.offset,
            size: e.size,
            constant: vars[e.var].constant,
            members: e
                .members
                .iter()
                .zip(written)
                .map(|(m, written)| DataMember {
                    name: m.name.clone(),
                    ty: m.ty.clone(),
                    offset: m.offset,
                    size: m.size,
                    written,
                })
                .collect(),
        });
    }
    out
}

/// Compile a program from `sources` for `target`.
pub fn compile_sources(
    sources: &Sources,
    target: &TargetProfile,
) -> Result<Compiled, Vec<Diagnostic>> {
    let (unit, probes) = lower_sources_with_probes(sources)?;
    if !sources.project().is_empty() {
        // The project alone, laid out: a library type no program could hold is refused in
        // every program, used or not, as one in the program's own source is.
        check_project(sources)?;
    }
    let mut compiled = compile_unit(unit, target, sources.data)?;
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
        Language::St => compile_unit(lower(source, language)?, target, &[]),
        Language::Ld | Language::Fbd | Language::Sfc => compile_sources(
            &Sources {
                tc6: &[("", source)],
                ..Default::default()
            },
            target,
        ),
    }
}

/// Compile a unit of the IR for `target`.
fn compile_unit(
    unit: mythos_logic_ir::Unit,
    target: &TargetProfile,
    data: &[DataBlock],
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
    let data = data_uses(&unit, &out.externals, data);
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
        data,
    })
}
