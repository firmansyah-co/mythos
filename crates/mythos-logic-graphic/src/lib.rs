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
//!
//! **Power flow for an editor.** Every output of an element that carries power (a contact, a
//! coil, a block's BOOL output and ENO, a BOOL variable read) and every junction of links is
//! kept in a hidden BOOL of the POU, `__LD_<localId>` or `__LD_<localId>_<pin>`, assigned when
//! the element is evaluated in the scan; each network keeps `__NET_<localId>`, TRUE once it
//! runs, and, where a jump or a return can skip it, FALSE when it does not. This is so in
//! every build (the owner, 2026-11-18): there is no build for monitoring. Only a value is
//! stored, never a guess: the lowering evaluates each element once, and the variable is
//! assigned at that moment, so storing it changes nothing the program does.
//! [`lower_with_probes`] says, per output, junction and network, which variable holds its
//! power ([`Probe`]).

mod fbd;
mod graph;
mod ld;
mod read;
mod sfc;
pub mod xml;

pub use sfc::MAX_SITUATIONS;

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
    /// The action or transition of the POU this place is in (a Sequential Function Chart's).
    pub owner: Option<Owner>,
    /// The text of the element this place is, by the TC6 name of what holds it; `None` for
    /// the element as a whole.
    pub field: Option<String>,
    /// How many characters that text has (of this line of it, for a text of several).
    pub len: u32,
    /// The line of the text, from 1, for a text of several lines.
    pub text_line: Option<u32>,
    /// Where the element's tag is in its document.
    pub tag: Span,
}

/// A part of a POU that a Sequential Function Chart declares by name (ADR-013 §2.17).
#[derive(Debug, Clone)]
pub(crate) enum Owner {
    /// `actions/action`, by its name.
    Action(String),
    /// `transitions/transition`, by its name.
    Transition(String),
    /// The inline body of an element of the chart, by its `localId`: an action of an action
    /// block, or a transition's condition.
    Inline(u64),
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

    /// The place itself as the place of a text of several lines (the Structured Text body of
    /// an action or a transition): a line of the syntax tree for each line of the text, one
    /// after the other. The line that stands for its first line.
    pub fn lines(&mut self, line: u32, field: &str, text: &str) -> u32 {
        let base = self.list[(line - FIRST_LINE) as usize].clone();
        let mut first = None;
        for (k, part) in text.split('\n').enumerate() {
            let mut place = base.clone();
            place.field = Some(field.to_string());
            place.len = part.trim_end_matches('\r').chars().count() as u32;
            place.text_line = Some(k as u32 + 1);
            let at = self.add(place);
            first.get_or_insert(at);
        }
        first.unwrap_or(line)
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

/// Where the power of one point of a diagram is kept (ADR-013 §2.15): of an output of an
/// element, of a junction of links, or whether a network ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// The POU the element is in.
    pub pou: String,
    /// The element; for a network, its first element in the order of evaluation.
    pub local_id: u64,
    pub at: At,
    pub holds: Holds,
    /// The body of a Sequential Function Chart's action or condition the element is in, when
    /// it is one written in LD or FBD (ADR-013 §2.17); `None` for an element of the POU's own
    /// body (its `localId` is the body's).
    pub body: Option<Body>,
}

/// A body in LD or FBD of a Sequential Function Chart's action or transition condition: its
/// elements' `localId`s are its own, so a [`Probe`] of one says which body it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub of: BodyOf,
    /// Whether the body is a Ladder Diagram (its probes are power, §2.15) or a Function Block
    /// Diagram (values, §2.16).
    pub ladder: bool,
}

/// Whose a [`Body`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyOf {
    /// An action of the POU (`actions/action`), by its name.
    Action(String),
    /// A transition of the POU (`transitions/transition`), by its name.
    Transition(String),
    /// The inline body (`inline`) of an action of an action block or of a transition, by that
    /// element's `localId`.
    Inline(u64),
    /// A network drawn in the chart's body itself that gives the condition of the transition of
    /// this `localId` (Table 55 features 2a–6a, TC6 `condition/connectionPointIn`); its
    /// elements' `localId`s are the chart's.
    Wired(u64),
}

/// The point of an element a [`Probe`] is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum At {
    /// An output: a block's formal parameter in upper case; empty for the one output of any
    /// other element.
    Output(String),
    /// An input where several links join, a vertical link (§8.2.3): the OR of them. A
    /// block's formal parameter in upper case; empty for the one input of any other element.
    Input(String),
    /// The network of these elements, by `localId` in the order they are evaluated: whether it
    /// ran in the last scan.
    Network(Vec<u64>),
    /// A step of a Sequential Function Chart: whether it is active, its flag `***.X`
    /// (IEC 61131-3 ed. 3 Table 54).
    Step,
    /// A step: its elapsed time `***.T`, a TIME.
    StepTime,
    /// A transition: its condition as it was last evaluated, which is only while the
    /// transition is enabled ([`Holds::Guarded`] by whether it was).
    Transition,
    /// An action of an action block (the association, by its `localId`): the state Q of the
    /// action it names, its action flag (§6.7.4.6).
    Action,
}

/// What holds a point's power.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holds {
    /// ON whenever its network runs: the left rail, and what it reaches through nothing that
    /// can switch it off (§8.2.1).
    On,
    /// The value of a variable, by its path from the POU (for the PROGRAM, the path of its
    /// symbol), negated when `negated`.
    Variable { path: String, negated: bool },
    /// The value of the variable `path`, computed in a scan only where the BOOL variable `ran`
    /// is TRUE: a function's result in a Function Block Diagram, where its EN, or the ENO of a
    /// function whose result it reads, may be FALSE (ADR-013 §2.16). Where `ran` is FALSE the
    /// function did not run, and `path` holds a value of an earlier scan, not this one's.
    Guarded { path: String, ran: String },
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
    lower_with_probes(ld, st).map(|(unit, _)| unit)
}

/// [`lower`], with the [`Probe`] of every output, junction and network of every LD POU.
pub fn lower_with_probes(
    ld: &[Source],
    st: Option<&Source>,
) -> Result<(Unit, Vec<Probe>), Vec<Diagnostic>> {
    lower_project_with_probes(ld, st, &mythos_logic_st::Project::default())
}

/// [`lower_with_probes`], in `project` (ADR-014, slice D1): the POUs of the documents and of
/// `st` may use the types, functions and function blocks of the project's library sources,
/// which are checked with them whole ([`mythos_logic_st::lower_tree_in`]). With library
/// sources there is more than one source, and each diagnostic with a line names its own.
pub fn lower_project_with_probes(
    ld: &[Source],
    st: Option<&Source>,
    project: &mythos_logic_st::Project,
) -> Result<(Unit, Vec<Probe>), Vec<Diagnostic>> {
    let several = ld.len() + st.is_some() as usize + project.library.len() > 1;
    let (mut tree, mut diags) = match st {
        Some(st) => parser::Parser::new(lexer::tokens(st.text)).source(),
        None => (ast::Source::default(), Vec::new()),
    };
    // The library's POUs before the documents' interfaces are read: a block of a diagram
    // may call one of them.
    mythos_logic_st::project::add_library(&mut tree, &mut diags, project);
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
    let mut probes = Vec::new();
    let mut charts = false;
    for pou in &pous {
        let lowered = match &pou.body {
            Some((graph::Lang::Fbd, _)) => {
                fbd::pou(pou, &known, &mut places, &mut diags, &mut probes)
            }
            Some((graph::Lang::Sfc, _)) => {
                charts = true;
                sfc::pou(pou, &known, &mut places, &mut diags, &mut probes)
            }
            _ => ld::pou(pou, &known, &mut places, &mut diags, &mut probes),
        };
        tree.pous.push(lowered);
    }
    if charts {
        // The structures of a chart's steps and actions (§2.17), declared once; no program
        // can name them (two underscores, §6.1.2).
        tree.types.extend(sfc::types(Span::default()));
    }
    let mut out = match mythos_logic_st::lower_tree_in(tree, diags, project) {
        Ok(unit) if said_about_documents.is_empty() => return Ok((unit, probes)),
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
        // One of a library source already names it (`mythos_logic_st::project`).
        if d.source.is_some() {
            return d;
        }
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
        line: place.text_line,
    });
    let (action, transition, inline) = match &place.owner {
        Some(Owner::Action(a)) => (Some(a.clone()), None, None),
        Some(Owner::Transition(t)) => (None, Some(t.clone()), None),
        Some(Owner::Inline(id)) => (None, None, Some(*id)),
        None => (None, None, None),
    };
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
            action,
            transition,
            inline,
            text,
        })),
    }
}
