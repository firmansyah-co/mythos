//! The project a program is compiled in (ADR-014, slice D1): its **library**, sources of
//! Structured Text holding the project's types, functions and function block types, and its
//! **data blocks**, named instances of a structure type of the library that a PROGRAM
//! declares in `VAR_EXTERNAL`.
//!
//! **One namespace.** The POUs and types of every library source are added to the program's
//! syntax tree and checked with it by the one rule of names the checker has: a program may
//! use what any library source declares, a library source what any other declares, in any
//! order, and a name declared twice is refused wherever the two are. **The library stands
//! alone:** it is first checked with no program ([`crate::check_project`]), so a library
//! source never uses a name only a program declares, and a data block is laid out alike in
//! every program.
//!
//! **Everything checked, only what is used compiled.** A program is checked with every
//! library source whole, so a mistake in a part no program uses is still told. It is then
//! compiled from its own POUs and types with only those of the library it uses, directly or
//! through one another ([`used_only`]): a library source the program does not use changes
//! nothing of what is compiled.
//!
//! **Places.** The lines of library source `k` are counted from [`first_line`]`(k)`: from
//! `2^30` up, `2^20` lines a source, past the program's source (from 1) and the files of the
//! standard library (from `2^20`, [`crate::library::first_line`]) and short of the places of a
//! TC6 document (from `2^31`). A diagnostic on such a line is given back the line of its
//! source and the source's name ([`name_sources`]), the way a program of several sources names
//! the source of each diagnostic (`mythos_logic_ir::Diagnostic::source`).

use crate::ast::{self, Source};
use crate::{lexer, library, parser};
use mythos_logic_ir::Diagnostic;

/// A data block of the project: a named instance of a structure type of the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataBlock<'a> {
    pub name: &'a str,
    /// The name of its type.
    pub ty: &'a str,
}

/// What a program is compiled with besides its own sources.
#[derive(Debug, Clone, Copy, Default)]
pub struct Project<'a> {
    /// `(name, text)` of each library source: TYPE blocks, functions and function block
    /// types, no PROGRAM.
    pub library: &'a [(&'a str, &'a str)],
    pub data: &'a [DataBlock<'a>],
}

impl Project<'_> {
    /// No library and no data block: a program is compiled as it always was.
    pub fn is_empty(&self) -> bool {
        self.library.is_empty() && self.data.is_empty()
    }
}

/// The line the first line of library source `0` is counted as.
pub const FIRST_LINE: u32 = 1 << 30;

/// How many lines each library source has room for.
pub const SOURCE_LINES: u32 = 1 << 20;

/// How many library sources a project may have: as many as fit between [`FIRST_LINE`] and
/// the first line of a TC6 document's places (`2^31`).
pub const MAX_SOURCES: usize = ((1u64 << 31) - FIRST_LINE as u64) as usize / SOURCE_LINES as usize;

/// The line library source `k`'s first line is counted as.
pub fn first_line(k: usize) -> u32 {
    FIRST_LINE + k as u32 * SOURCE_LINES
}

/// The library source a line counted by [`first_line`] is in.
pub fn source_of(line: u32) -> Option<usize> {
    let k = line.checked_sub(FIRST_LINE)? / SOURCE_LINES;
    (k < MAX_SOURCES as u32).then_some(k as usize)
}

/// Read every library source of `project` and add its types and POUs to `tree`; what reading
/// them says goes to `diags`. A PROGRAM in a library source is refused and left out.
pub fn add_library(tree: &mut Source, diags: &mut Vec<Diagnostic>, project: &Project) {
    if project.library.len() > MAX_SOURCES {
        diags.push(Diagnostic::whole(format!(
            "the project has {} library sources; at most {MAX_SOURCES} are taken",
            project.library.len()
        )));
        // None is read, so what they declare is not known: nothing is said about its
        // absence, as for a source too long to read below.
        tree.lost.extend([ast::Lost::Function, ast::Lost::Type]);
        return;
    }
    for (k, (name, text)) in project.library.iter().enumerate() {
        let lines = text.split('\n').count();
        if lines > SOURCE_LINES as usize {
            diags.push(Diagnostic::whole(format!(
                "the library source '{name}' has {lines} lines; at most {SOURCE_LINES} are taken"
            )));
            // What it declares is not known: nothing is said about its absence.
            tree.lost.extend([ast::Lost::Function, ast::Lost::Type]);
            continue;
        }
        let (read, said) = parser::Parser::new(lexer::tokens_at(text, first_line(k))).source();
        diags.extend(said);
        for p in read.pous {
            if p.kind == ast::PouKind::Program {
                diags.push(Diagnostic::at(
                    p.span,
                    format!(
                        "{} is a PROGRAM in the library source '{name}'; a library holds types, functions and function blocks",
                        p.name
                    ),
                ));
                continue;
            }
            tree.pous.push(p);
        }
        tree.types.extend(read.types);
        // A PROGRAM whose name could not be read is not the program's.
        tree.lost
            .extend(read.lost.into_iter().filter(|l| *l != ast::Lost::Program));
    }
}

/// `tree` with only what the program uses of the library: every type and POU that is not of
/// a library source, and each of a library source that one of those uses, directly or through
/// others, by name (a type named in a declaration, a function called).
pub fn used_only(tree: Source) -> Source {
    let Source { types, pous, lost } = tree;
    let (mut out_types, mut lib_types): (Vec<_>, Vec<_>) = types
        .into_iter()
        .partition(|t| source_of(t.span.line).is_none());
    let (mut out_pous, mut lib_pous): (Vec<_>, Vec<_>) = pous
        .into_iter()
        .partition(|p| source_of(p.span.line).is_none());
    let mut wanted = Vec::new();
    out_types
        .iter()
        .for_each(|t| library::type_names_used(t, &mut wanted));
    out_pous
        .iter()
        .for_each(|p| library::pou_names_used(p, &mut wanted));
    let mut seen = 0;
    // Each name looked at once; a type or POU taken adds the names it uses.
    while seen < wanted.len() {
        let name = wanted[seen].clone();
        seen += 1;
        while let Some(k) = lib_types
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(&name))
        {
            let t = lib_types.remove(k);
            library::type_names_used(&t, &mut wanted);
            out_types.push(t);
        }
        while let Some(k) = lib_pous
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(&name))
        {
            let p = lib_pous.remove(k);
            library::pou_names_used(&p, &mut wanted);
            out_pous.push(p);
        }
    }
    Source {
        types: out_types,
        pous: out_pous,
        lost,
    }
}

/// The data blocks of `data` that a PROGRAM of `tree` declares in `VAR_EXTERNAL`.
pub fn declared<'a>(tree: &Source, data: &[DataBlock<'a>]) -> Vec<DataBlock<'a>> {
    data.iter()
        .filter(|b| {
            tree.pous.iter().any(|p| {
                p.kind == ast::PouKind::Program
                    && p.vars.iter().any(|v| {
                        v.section == ast::Section::External && v.name.eq_ignore_ascii_case(b.name)
                    })
            })
        })
        .copied()
        .collect()
}

/// Each diagnostic on a line of a library source given that source's name and its own line.
pub fn name_sources(diags: Vec<Diagnostic>, project: &Project) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .map(|mut d| {
            let Some(k) = source_of(d.line) else {
                return d;
            };
            let Some((name, _)) = project.library.get(k) else {
                return d;
            };
            let base = first_line(k) - 1;
            d.line -= base;
            if source_of(d.end_line) == Some(k) {
                d.end_line -= base;
            } else {
                d.end_line = d.line;
                d.end_col = d.col;
            }
            d.source = Some(name.to_string());
            d
        })
        .collect()
}
