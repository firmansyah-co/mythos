//! The Ladder Diagram front-end (ADR-013 §2.1, §2.15): IEC 61131-3 ed. 3 §8.2 LD, stored as
//! PLCopen TC6 XML v2.01, to the IR.
//!
//! **What it reads.** A TC6 `project` document; of it, the POUs (`types/pous/pou`): a
//! PROGRAM, FUNCTION_BLOCK or FUNCTION with an interface (VAR_INPUT, VAR_OUTPUT, VAR, with
//! RETAIN or NON_RETAIN where Structured Text takes them; elementary types, STRING, arrays
//! and named types; initial values) and one body in LD: left and right power rails,
//! contacts (normally open, negated, rising, falling), coils (plain, negated, set, reset,
//! rising, falling), blocks calling a function or a function block (the standard library's,
//! or the project's, in LD or in ST), input, output and in-out variables, labels, jumps,
//! returns and comments. Everything else of the schema is refused by its name, with the
//! element it is in; nothing is skipped silently.
//!
//! **How it lowers.** The graph is read into an element per `localId`, checked (every
//! connection reaches an element that gives an output, no element has two `localId`s in
//! common, no cycle), split into networks, and each network lowered to statements of the
//! Structured Text front-end's syntax tree: never to ST text. The text inside elements (a
//! contact's variable, a variable's expression, an initial value) is read by the ST lexer
//! and parser, and the POUs are checked by the ST checker together with the project's ST
//! POUs and the standard library ([`mythos_logic_st::lower_tree`]): one rule for every
//! declaration, type and call, whichever language wrote it. The result is the IR, which the
//! backend, the VM, RETAIN, watching, forcing and online change take as they take ST.
//!
//! **Places.** A diagnostic about an LD POU names the element ([`mythos_logic_ir::Element`]):
//! its `localId` and position, or the variable of the interface, and the characters of its
//! text when it is about them. The syntax tree carries this as lines of its own: each place
//! of a document is a line from [`FIRST_LINE`] up, which no source and no file of the library
//! reaches, and a diagnostic on such a line is mapped back to its place.

mod lower;
mod read;
pub mod xml;

use mythos_logic_ir::{Diagnostic, Element, Span, TextRange, Unit};
use mythos_logic_st::{ast, lexer, parser};

/// The namespace of PLCopen TC6 XML v2.01, the only version read.
pub const TC6_NAMESPACE: &str = "http://www.plcopen.org/xml/tc6_0201";

/// The first line given to a place of an LD document in the syntax tree. Structured Text
/// sources count lines from 1 and the files of the standard library from `2^20`, each file
/// `2^20` lines apart (`mythos_logic_st::library::first_line`); this is far past both.
pub const FIRST_LINE: u32 = 1 << 31;

/// A source a program is compiled from, with the name its diagnostics give it.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

/// A place of an LD document: what a diagnostic about it says.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub doc: usize,
    pub pou: String,
    pub local_id: Option<u64>,
    pub position: Option<(f64, f64)>,
    pub variable: Option<String>,
    /// The text of the element this place is, by the TC6 name of what holds it; `None` for
    /// the element as a whole.
    pub field: Option<String>,
    /// How many characters that text has.
    pub len: u32,
    /// Where the element's tag is in its document.
    pub tag: Span,
}

/// Every place of the documents being lowered, each a line of the syntax tree.
#[derive(Debug, Default)]
pub(crate) struct Places {
    list: Vec<Place>,
    /// The keywords the POU being read declared as names (refused there, once): where its
    /// texts use them they are those names.
    pub keywords_as_names: Vec<String>,
}

impl Places {
    /// A new place; the line that stands for it.
    pub fn add(&mut self, place: Place) -> u32 {
        self.list.push(place);
        FIRST_LINE + (self.list.len() as u32 - 1)
    }

    /// The place itself as the place of the line `line`: a text of it as a field of its own.
    pub fn text(&mut self, line: u32, field: &str, text: &str) -> u32 {
        let mut place = self.list[(line - FIRST_LINE) as usize].clone();
        place.field = Some(field.to_string());
        place.len = text.chars().count() as u32;
        self.add(place)
    }

    /// The span of the whole of the place `line` stands for: all of its text, or, for an
    /// element, a mark at its start.
    pub fn span(&self, line: u32) -> Span {
        let len = self.list[(line - FIRST_LINE) as usize].len;
        Span {
            line,
            col: 1,
            end_line: line,
            end_col: len + 1,
        }
    }

    /// A diagnostic about the place `line` stands for.
    pub fn diag(&self, line: u32, message: impl Into<String>) -> Diagnostic {
        Diagnostic::at(self.span(line), message)
    }
}

/// What reading the text of an element gives: a tree of the expression, or nothing when it
/// could not be read (the diagnostics say why).
pub(crate) fn expression(
    places: &mut Places,
    at: u32,
    field: &str,
    text: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<ast::Expr> {
    let line = places.text(at, field, text);
    if text.contains(['\n', '\r']) {
        diags.push(places.diag(
            line,
            format!("the {field} is on more than one line; write it on one"),
        ));
        return None;
    }
    let (expr, said) = parser::Parser::new(lexer::tokens_at(text, line))
        .with_named_keywords(&places.keywords_as_names)
        .expression_alone();
    diags.extend(said);
    expr
}

/// Lower the Ladder Diagram POUs of the documents `ld`, with the POUs of the Structured Text
/// source `st`, into one unit: one of all of them is the PROGRAM, and each may call the
/// functions and function blocks of the others and of the standard library.
///
/// The diagnostics are every independent mistake found, as the ST front-end tells them
/// (`mythos_logic_st::lower`); one about an LD POU names its element
/// ([`mythos_logic_ir::Element`]). When there is more than one source, each diagnostic with
/// a line names its source.
pub fn lower(ld: &[Source], st: Option<&Source>) -> Result<Unit, Vec<Diagnostic>> {
    let several = ld.len() + st.is_some() as usize > 1;
    let (mut tree, mut diags) = match st {
        Some(st) => parser::Parser::new(lexer::tokens(st.text)).source(),
        None => (ast::Source::default(), Vec::new()),
    };
    let mut places = Places::default();
    // Every document's interfaces first, so that a body may call a POU of any of them.
    let mut pous = Vec::new();
    let mut said_about_documents: Vec<(usize, Diagnostic)> = Vec::new();
    for (k, doc) in ld.iter().enumerate() {
        let read = match xml::parse(doc.text) {
            Ok((root, namespace)) => {
                read::document(&root, namespace.as_deref(), k, &mut places, &mut diags)
            }
            Err(d) => {
                said_about_documents.push((k, d));
                None
            }
        };
        match read {
            Some(read) => pous.extend(read),
            // What the document declares is not known: nothing is said about its absence.
            None => tree
                .lost
                .extend([ast::Lost::Program, ast::Lost::Function, ast::Lost::Type]),
        }
    }
    let known = read::Known::of(&tree, pous.iter());
    for pou in &pous {
        let lowered = lower::pou(pou, &known, &mut places, &mut diags);
        tree.pous.push(lowered);
    }
    let mut out = match mythos_logic_st::lower_tree(tree, diags) {
        Ok(unit) if said_about_documents.is_empty() => return Ok(unit),
        Ok(_) => Vec::new(),
        Err(diags) => diags,
    };
    let name = |doc: usize| several.then(|| ld[doc].name.to_string());
    let mut mapped: Vec<Diagnostic> = said_about_documents
        .into_iter()
        .map(|(k, mut d)| {
            d.source = name(k);
            d
        })
        .collect();
    for d in out.drain(..) {
        mapped.push(map(
            d,
            &places,
            &name,
            several.then_some(st.map(|s| s.name)).flatten(),
        ));
    }
    Err(mapped)
}

/// A diagnostic of the syntax tree as its source says it: one on a line of [`Places`] about
/// that element of its document, one of the ST source naming it when there are several.
fn map(
    d: Diagnostic,
    places: &Places,
    doc_name: &dyn Fn(usize) -> Option<String>,
    st_name: Option<&str>,
) -> Diagnostic {
    if d.line < FIRST_LINE {
        let source = (d.line != 0).then(|| st_name.map(String::from)).flatten();
        return Diagnostic { source, ..d };
    }
    let Some(place) = places.list.get((d.line - FIRST_LINE) as usize) else {
        return d;
    };
    let text = place.field.as_ref().map(|field| TextRange {
        field: field.clone(),
        col: d.col,
        end_col: if d.end_line == d.line {
            d.end_col
        } else {
            place.len + 1
        },
    });
    Diagnostic {
        line: place.tag.line,
        col: place.tag.col,
        end_line: place.tag.end_line,
        end_col: place.tag.end_col,
        message: d.message,
        source: doc_name(place.doc),
        element: Some(Box::new(Element {
            pou: place.pou.clone(),
            local_id: place.local_id,
            x: place.position.map(|p| p.0),
            y: place.position.map(|p| p.1),
            variable: place.variable.clone(),
            text,
        })),
    }
}
