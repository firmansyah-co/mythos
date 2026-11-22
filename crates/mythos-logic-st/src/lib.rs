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
pub mod project;

use mythos_logic_ir::{Diagnostic, Span, Unit};
pub use project::{DataBlock, Project};

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

/// [`lower`], in `project` (ADR-014, slice D1): the program's Structured Text source `st`,
/// `(name, text)`, with the project's library sources and data blocks.
///
/// With neither, this is [`lower`]. With library sources, each diagnostic with a line names
/// its source: the library source it is in, or `st`.
pub fn lower_project(st: Option<(&str, &str)>, project: &Project) -> Result<Unit, Vec<Diagnostic>> {
    let (name, text) = st.unwrap_or(("", ""));
    if project.is_empty() {
        return lower(text);
    }
    let (mut ast, mut diags) = parser::Parser::new(lexer::tokens(text)).source();
    project::add_library(&mut ast, &mut diags, project);
    let several = !project.library.is_empty();
    lower_tree_in(ast, diags, project).map_err(|diags| {
        diags
            .into_iter()
            .map(|mut d| {
                if several && d.source.is_none() && d.line != 0 {
                    d.source = Some(name.to_string());
                }
                d
            })
            .collect()
    })
}

/// [`lower_tree`], in `project`: `ast` holds the program's POUs and types and those of the
/// project's library sources ([`project::add_library`]).
///
/// Everything in `ast` is checked, so a mistake in a part of the library the program does not
/// use is told; what is lowered is the program with only what it uses of the library
/// ([`project::used_only`]), and with only the data blocks it declares. A diagnostic on a
/// line of a library source is given that source's line and name ([`project::name_sources`]).
/// With no library and no data block, this is [`lower_tree`].
pub fn lower_tree_in(
    ast: ast::Source,
    diags: Vec<Diagnostic>,
    project: &Project,
) -> Result<Unit, Vec<Diagnostic>> {
    if project.is_empty() {
        return lower_tree(ast, diags);
    }
    let finish = |diags: Vec<Diagnostic>| {
        let mut diags = library::as_the_compilers(diags);
        diags.sort_by_key(|d| (d.line, d.col));
        project::name_sources(diags, project)
    };
    // Everything, checked.
    let mut whole = ast.clone();
    let mut diags = diags;
    library::add_used(&mut whole, &mut diags);
    match check::check_in(&whole, project) {
        Ok(_) if diags.is_empty() => {}
        Ok(_) => return Err(finish(diags)),
        Err(more) => {
            diags.extend(more);
            return Err(finish(diags));
        }
    }
    // What the program uses, lowered.
    let mut used = project::used_only(ast);
    let data = project::declared(&used, project.data);
    let mut diags = Vec::new();
    library::add_used(&mut used, &mut diags);
    let within = Project {
        library: project.library,
        data: &data,
    };
    match check::check_in(&used, &within) {
        Ok(unit) if diags.is_empty() => Ok(unit),
        Ok(_) => Err(finish(diags)),
        Err(more) => {
            diags.extend(more);
            Err(finish(diags))
        }
    }
}

/// The name of the PROGRAM [`check_project`] checks the project in. It begins with two
/// underscores, which no name of a source may (IEC 61131-3 ed. 3 §6.1.2).
pub const PROJECT_PROGRAM: &str = "__PROJECT";

/// Check the library sources and the data blocks of `project` alone, with no program of its
/// own: as a PROGRAM that declares every data block in `VAR_EXTERNAL` and does nothing, so
/// that each block is laid out as a program's copy of it is.
pub fn check_project(project: &Project) -> Result<Unit, Vec<Diagnostic>> {
    let mut ast = ast::Source::default();
    let mut diags = Vec::new();
    project::add_library(&mut ast, &mut diags, project);
    let mut vars: Vec<ast::VarDecl> = Vec::new();
    for b in project.data {
        // A name declared twice is told once, of the project's declarations.
        if vars.iter().any(|v| v.name.eq_ignore_ascii_case(b.name)) {
            continue;
        }
        vars.push(ast::VarDecl {
            name: b.name.to_string(),
            span: Span::default(),
            section: ast::Section::External,
            retention: ast::Retention::Unqualified,
            ty: ast::TypeSpec {
                span: Span::default(),
                kind: ast::TypeSpecKind::Named(b.ty.to_string()),
            },
            init: None,
            hidden: false,
            constant: false,
        });
    }
    ast.pous.push(ast::Pou {
        kind: ast::PouKind::Program,
        name: PROJECT_PROGRAM.to_string(),
        span: Span::default(),
        result: None,
        vars,
        body: Vec::new(),
        partial_vars: false,
        partial_body: false,
        library: false,
    });
    lower_tree_in(ast, diags, project)
}
