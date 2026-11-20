//! The standard library, written in Structured Text (ADR-013 L4).
//!
//! Adrian, 2026-11-18: every standard function block is implemented on top of ST, with what
//! ST cannot express exposed as an intrinsic; OSCAT is to follow as the standard library. So
//! a block of the library is source text like a program's, compiled with the program that
//! uses it, and adding one needs no new firmware.
//!
//! **Layout.** One file per block or per family of blocks, each embedded here, and an index
//! of every POU the library declares with the file that holds it ([`POUS`]). A test holds the
//! index equal to what the files declare.
//!
//! **What a program gets.** Only the POUs it uses, and those they use in turn: a type named
//! in a declaration, a function called in an expression ([`add_used`]).
//!
//! **Names.** Every name the library declares is refused as the name of a POU or a type of a
//! program, used or not, so a program never changes what a name of the library means. Inside
//! the library, names are resolved among the library's alone (`check`), so nothing a program
//! declares reaches into it. A name beginning with two underscores is an intrinsic: IEC
//! 61131-3 ed. 3 §6.1.2 forbids it in an identifier, so the lexer refuses it in a program and
//! takes it only here.
//!
//! **Places.** The lines of file `k` are counted from `(k + 1) × 2^20`, never a line of a
//! program's source. A diagnostic about a place in the library would be a fault of the
//! compiler; [`as_the_compilers`] makes it one about the whole program that says so, never one
//! that points into the program's text.

use crate::ast::{self, PouKind, Source};
use crate::{lexer, parser};
use mythos_logic_ir::Diagnostic;

/// A file of the library.
pub struct File {
    pub name: &'static str,
    pub text: &'static str,
}

/// Every file of the library.
pub const FILES: &[File] = &[
    File {
        name: "bistables.st",
        text: include_str!("bistables.st"),
    },
    File {
        name: "edges.st",
        text: include_str!("edges.st"),
    },
    File {
        name: "counters.st",
        text: include_str!("counters.st"),
    },
    File {
        name: "timers.st",
        text: include_str!("timers.st"),
    },
];

/// Every POU of the library: its name in upper case, its kind, and the file (an index into
/// [`FILES`]) that declares it.
pub const POUS: &[(&str, PouKind, usize)] = &[
    ("SR", PouKind::FunctionBlock, 0),
    ("RS", PouKind::FunctionBlock, 0),
    ("R_TRIG", PouKind::FunctionBlock, 1),
    ("F_TRIG", PouKind::FunctionBlock, 1),
    ("CTU", PouKind::FunctionBlock, 2),
    ("CTU_INT", PouKind::FunctionBlock, 2),
    ("CTU_DINT", PouKind::FunctionBlock, 2),
    ("CTU_LINT", PouKind::FunctionBlock, 2),
    ("CTD", PouKind::FunctionBlock, 2),
    ("CTD_INT", PouKind::FunctionBlock, 2),
    ("CTD_DINT", PouKind::FunctionBlock, 2),
    ("CTD_LINT", PouKind::FunctionBlock, 2),
    ("CTUD", PouKind::FunctionBlock, 2),
    ("CTUD_INT", PouKind::FunctionBlock, 2),
    ("CTUD_DINT", PouKind::FunctionBlock, 2),
    ("CTUD_LINT", PouKind::FunctionBlock, 2),
    ("TP", PouKind::FunctionBlock, 3),
    ("TP_TIME", PouKind::FunctionBlock, 3),
    ("TON", PouKind::FunctionBlock, 3),
    ("TON_TIME", PouKind::FunctionBlock, 3),
    ("TOF", PouKind::FunctionBlock, 3),
    ("TOF_TIME", PouKind::FunctionBlock, 3),
];

/// Lines of the library are counted from here and up: file `k` from `(k + 1) * FILE_LINES`.
pub const FILE_LINES: u32 = 1 << 20;

/// The line file `k`'s first line is counted as.
pub fn first_line(k: usize) -> u32 {
    (k as u32 + 1) * FILE_LINES
}

/// The POU of the library named `upper` (a name in upper case): its kind and its file.
pub fn find(upper: &str) -> Option<(PouKind, usize)> {
    POUS.iter()
        .find(|(name, _, _)| *name == upper)
        .map(|(_, kind, file)| (*kind, *file))
}

/// Whether the library declares a POU named `upper`.
pub fn declares(upper: &str) -> bool {
    find(upper).is_some()
}

/// The POUs of file `k`, read. Each is marked as the library's.
pub fn read(k: usize) -> (Source, Vec<Diagnostic>) {
    let (mut src, diags) =
        parser::Parser::new(lexer::library_tokens(FILES[k].text, first_line(k))).source();
    for p in &mut src.pous {
        p.library = true;
    }
    (src, diags)
}

/// Add to `src` every POU of the library it uses, and every one those use, each once; what
/// reading a file of the library says goes to `diags` (nothing, unless the library is
/// broken). A POU of the source that bears a library name does not stand in for the
/// library's: the checker refuses it.
pub fn add_used(src: &mut Source, diags: &mut Vec<Diagnostic>) {
    let mut wanted: Vec<String> = Vec::new();
    names_used(src, &mut wanted);
    let mut files: Vec<Option<Source>> = (0..FILES.len()).map(|_| None).collect();
    let mut taken: Vec<String> = Vec::new();
    while let Some(name) = wanted.pop() {
        let Some((_, k)) = find(&name) else { continue };
        if taken.contains(&name) {
            continue;
        }
        taken.push(name.clone());
        let file = files[k].get_or_insert_with(|| {
            let (read, said) = read(k);
            diags.extend(said);
            read
        });
        let Some(pou) = file
            .pous
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
            .cloned()
        else {
            continue;
        };
        let mut more = Vec::new();
        pou_names_used(&pou, &mut more);
        wanted.extend(more);
        src.pous.push(pou);
    }
}

/// Every name `src` uses that could be a POU of the library, in upper case: each type named
/// in a declaration, and each function called in an expression.
fn names_used(src: &Source, out: &mut Vec<String>) {
    for t in &src.types {
        match &t.kind {
            ast::TypeDeclKind::Struct(fields) => {
                fields.iter().for_each(|f| spec_names(&f.ty, out));
            }
            ast::TypeDeclKind::Alias(spec) => spec_names(spec, out),
            ast::TypeDeclKind::Unknown => {}
        }
    }
    for p in &src.pous {
        pou_names_used(p, out);
    }
}

fn pou_names_used(p: &ast::Pou, out: &mut Vec<String>) {
    if let Some(spec) = &p.result {
        spec_names(spec, out);
    }
    for v in &p.vars {
        spec_names(&v.ty, out);
        if let Some(init) = &v.init {
            init_names(init, out);
        }
    }
    stmts_names(&p.body, out);
}

fn add(name: &str, out: &mut Vec<String>) {
    let upper = name.to_ascii_uppercase();
    if declares(&upper) && !out.contains(&upper) {
        out.push(upper);
    }
}

fn spec_names(spec: &ast::TypeSpec, out: &mut Vec<String>) {
    let mut spec = spec;
    // A loop through the element types of arrays: a named type is at the end of the chain.
    loop {
        match &spec.kind {
            ast::TypeSpecKind::Named(name) => return add(name, out),
            ast::TypeSpecKind::Array(_, elem) => spec = elem,
            ast::TypeSpecKind::String(_) | ast::TypeSpecKind::OfValue => return,
        }
    }
}

fn init_names(init: &ast::Init, out: &mut Vec<String>) {
    fn items(list: &[ast::ArrayInit], out: &mut Vec<String>) {
        for item in list {
            match item {
                ast::ArrayInit::Value(e) => expr_names(e, out),
                ast::ArrayInit::Repeat(_, inner, _) => items(inner, out),
            }
        }
    }
    match init {
        ast::Init::Value(e) => expr_names(e, out),
        ast::Init::Array(list, _) => items(list, out),
    }
}

fn place_names(p: &ast::Place, out: &mut Vec<String>) {
    for sel in &p.path {
        if let ast::Selector::Index(subscripts, _) = sel {
            subscripts.iter().for_each(|e| expr_names(e, out));
        }
    }
}

fn expr_names(e: &ast::Expr, out: &mut Vec<String>) {
    match &e.kind {
        ast::ExprKind::Call(name, args) => {
            add(name, out);
            args.iter().for_each(|a| expr_names(&a.value, out));
        }
        ast::ExprKind::Var(p) => place_names(p, out),
        ast::ExprKind::Neg(a) | ast::ExprKind::Not(a) => expr_names(a, out),
        ast::ExprKind::Binary(_, a, b) => {
            expr_names(a, out);
            expr_names(b, out);
        }
        ast::ExprKind::Int(_)
        | ast::ExprKind::Real(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::Time(_)
        | ast::ExprKind::Str(_)
        | ast::ExprKind::Intrinsic(_) => {}
    }
}

fn stmts_names(stmts: &[ast::Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            ast::Stmt::Assign(p, e) => {
                place_names(p, out);
                expr_names(e, out);
            }
            ast::Stmt::Call(p, args) => {
                place_names(p, out);
                for a in args {
                    match a {
                        ast::Arg::In(_, e, _) | ast::Arg::Value(e) => expr_names(e, out),
                        ast::Arg::Out(_, target, _) => place_names(target, out),
                    }
                }
            }
            ast::Stmt::If(branches, otherwise) => {
                for (c, body) in branches {
                    expr_names(c, out);
                    stmts_names(body, out);
                }
                if let Some(body) = otherwise {
                    stmts_names(body, out);
                }
            }
            ast::Stmt::Case(sel, arms, otherwise) => {
                expr_names(sel, out);
                for (labels, body) in arms {
                    for l in labels {
                        match l {
                            ast::CaseLabel::One(e) => expr_names(e, out),
                            ast::CaseLabel::Range(a, b) => {
                                expr_names(a, out);
                                expr_names(b, out);
                            }
                        }
                    }
                    stmts_names(body, out);
                }
                if let Some(body) = otherwise {
                    stmts_names(body, out);
                }
            }
            ast::Stmt::For {
                var,
                from,
                to,
                by,
                body,
            } => {
                place_names(var, out);
                expr_names(from, out);
                expr_names(to, out);
                if let Some(by) = by {
                    expr_names(by, out);
                }
                stmts_names(body, out);
            }
            ast::Stmt::While(c, body) | ast::Stmt::Repeat(body, c) => {
                expr_names(c, out);
                stmts_names(body, out);
            }
            ast::Stmt::Exit(_) | ast::Stmt::Return(_) => {}
        }
    }
}

/// Where in the library a line counted as [`first_line`] says is: the file and its own line.
pub fn place_of(line: u32) -> Option<(&'static str, u32)> {
    let k = (line / FILE_LINES).checked_sub(1)? as usize;
    let file = FILES.get(k)?;
    Some((file.name, line - first_line(k) + 1))
}

/// Each diagnostic about a place in the library made one about the program as a whole, which
/// says where in the library it is and that the fault is the compiler's. The program's text
/// is never pointed into for it.
pub fn as_the_compilers(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .map(|d| match place_of(d.line) {
            Some((file, line)) => Diagnostic::whole(format!(
                "the standard library does not compile ({file} {line}:{}: {}); this is a fault of the compiler, not of the program",
                d.col, d.message
            )),
            None => d,
        })
        .collect()
}
