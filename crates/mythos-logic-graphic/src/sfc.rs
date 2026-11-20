//! A Sequential Function Chart to statements of the Structured Text syntax tree (IEC 61131-3
//! ed. 3 §6.7; ADR-013 §2.17).
//!
//! **What is read.** A POU whose body is `SFC` (a program or a function block, §6.7.1), with:
//! `step` (`name`, `initialStep`), `transition` (its condition `inline` in ST, or a
//! `reference` to a transition of the POU's `transitions`, `negated`; `priority`),
//! `selectionDivergence`, `selectionConvergence`, `simultaneousDivergence`,
//! `simultaneousConvergence`, `jumpStep` (`targetName`), `actionBlock` (each `action` with its
//! `qualifier`, `duration`, and a `reference` to an action of the POU's `actions` or to a BOOL
//! variable, or an `inline` body in ST) and `comment`. The POU's `actions` (`action` with a body
//! in ST: statements) and `transitions` (`transition` with a body in ST: one Boolean
//! expression). Everything else is refused by its name, with the element: an action or a
//! transition in LD, FBD, IL or SFC, a condition drawn as a network (`connectionPointIn`), a
//! `macroStep` (not in ed. 3), the qualifiers SD, SL and P0 (below) and DL (not in Table 59), an
//! action block's indicator variable (deprecated, §6.7.4.4), a negated step or action block.
//!
//! **The chart as a net.** Each transition is resolved to the steps before it and the steps
//! after it, through the divergences and convergences (Table 61) and jumps: a transition
//! is *enabled* when every step before it is active, and *cleared* when it is enabled and its
//! condition is TRUE (§6.7.5). The alternation step, transition, step is checked, naming the
//! elements: two steps or two transitions linked directly are refused. Each chart (a connected
//! set of steps and transitions) has exactly one initial step (§6.7.2 error 1). Transitions
//! of one selection divergence take the left-to-right priority of Table 61 feature 2a, or
//! feature 2b where every one of them gives a `priority` (the lowest first); TC6 cannot draw
//! feature 2c's mutual exclusion, so every divergence here has a priority.
//!
//! **Unsafe and unreachable charts** (§6.7.5, Figure 26: "shall treat the existence of such
//! conditions as errors"). Every situation the chart can reach is explored, as if any condition
//! could be TRUE or FALSE at any scan: a chart is refused when, in one of them, clearing a
//! transition would activate a step that is already active and that it does not deactivate
//! (unsafe), or when a step can never be active or a transition never be enabled (unreachable).
//! So a chart that only its conditions keep safe is refused too: the structure is judged, as
//! Figure 26 judges it. The exploration stops at [`MAX_SITUATIONS`] and refuses what it could
//! not check.
//!
//! **A scan** (§6.7.5): the steps' elapsed times grow by the scan clock; every action runs
//! under its action control with the steps as they are; every enabled transition's condition
//! is evaluated; then every transition that clears does so at once (each deactivates the steps
//! before it, then activates those after it, its elapsed time from `T#0s`), the first of a
//! divergence by priority. So a step activated in this scan runs its actions in the next one
//! before any transition after it is tested, as the rule on propagation requires, the initial
//! step included.
//!
//! **Action control** (§6.7.4.6): the functional equivalent of ACTION_CONTROL *without final
//! scan* (Table 60 feature 2, Figure 23 b), built from the standard library's RS, TON and
//! R_TRIG, only the parts an action uses (rule e). The feature with final scan is not taken:
//! there, A is the OR of Q and an F_TRIG of Q, and an F_TRIG is TRUE at its first execution
//! after a cold start when its input is FALSE (Table 44 and its note), so every action given as
//! statements would run once at a cold start, whatever its step. The T input is the duration of
//! the active time-related association, `T#0s` when there is none (rule c). SD, SL and P0 are
//! refused for the owner to decide (ADR-013 §2.17): by rule c an SD action whose step is left
//! before its delay starts at once, an SL action ends at once; P0's F_TRIG makes it run at a
//! cold start. Two time-related associations of one action whose steps can be active together
//! are refused (rule d).
//!
//! **What is kept** (ADR-013 §2.17, the owner's decision for monitoring): each step is a
//! variable of the POU by the step's name, a structure of `X` (its flag) and `T` (its elapsed
//! time), read by the program as `S.X` and `S.T` (Table 54) and written only by the chart
//! (Table 54 footnote a); each action given by name is a variable by its name with `Q`; an
//! inline action keeps `Q` in `__SFC_<localId>_Q`; each transition keeps its condition in
//! `__SFC_<localId>`, evaluated only while it is enabled, and whether it was in
//! `__SFC_<localId>_EN`; the action control's blocks are `__SFC_<action>_<part>`. All are
//! private symbols, retained or carried as any hidden variable. `__SFC_SHAPE` holds a hash of
//! the chart's steps and transitions: a build of another chart that takes over the variables
//! of a running one (online change) starts the chart again from its initial step, so steps
//! carried by name never make a situation the new chart cannot be in.

use crate::graph::{bool_expr, bool_ty, Hidden, Lang};
use crate::read::{flag, name, GPou};
use crate::xml::Node;
use crate::{At, Holds, Owner, Place, Places, Probe};
use mythos_logic_ir::{Diagnostic, Intrinsic, Retention, Section, Span};
use mythos_logic_st::ast::{
    self, Arg, BinOp, Expr, ExprKind, Init, Selector, Stmt, TypeDecl, TypeDeclKind, TypeSpec,
    TypeSpecKind, VarDecl,
};
use mythos_logic_st::{lexer, parser};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

/// The structure of a step that is not initial: `X` and `T` (Table 54).
pub(crate) const STEP_TYPE: &str = "__SFC_STEP";
/// The structure of an initial step: its `X` starts TRUE (§6.7.2).
pub(crate) const INITIAL_STEP_TYPE: &str = "__SFC_INITIAL_STEP";
/// The structure of an action given by name: `Q`, its action flag (§6.7.4.6 rule a).
pub(crate) const ACTION_TYPE: &str = "__SFC_ACTION";

/// How many situations (sets of active steps) a chart may reach and still be checked for
/// safety and reachability.
pub const MAX_SITUATIONS: usize = 100_000;

/// The types a unit with a chart declares, once.
pub(crate) fn types(span: Span) -> Vec<TypeDecl> {
    let member = |name: &str, ty: &str, init: Option<bool>| VarDecl {
        name: name.into(),
        span,
        section: Section::Var,
        retention: Retention::Unqualified,
        ty: TypeSpec {
            span,
            kind: TypeSpecKind::Named(ty.into()),
        },
        init: init.map(|b| Init::Value(bool_expr(b, span))),
        hidden: true,
    };
    let step = |name: &str, initial: bool| TypeDecl {
        name: name.into(),
        span,
        kind: TypeDeclKind::Struct(vec![
            member("X", "BOOL", Some(initial)),
            member("T", "TIME", None),
        ]),
        partial: false,
    };
    vec![
        step(STEP_TYPE, false),
        step(INITIAL_STEP_TYPE, true),
        TypeDecl {
            name: ACTION_TYPE.into(),
            span,
            kind: TypeDeclKind::Struct(vec![member("Q", "BOOL", None)]),
            partial: false,
        },
    ]
}

/// An action qualifier taken (Table 59).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Qual {
    N,
    R,
    S,
    L,
    D,
    P,
    P1,
    Ds,
}

impl Qual {
    /// A time-related qualifier, which has a duration (§6.7.4.5).
    fn timed(self) -> bool {
        matches!(self, Qual::L | Qual::D | Qual::Ds)
    }
    fn name(self) -> &'static str {
        match self {
            Qual::N => "N",
            Qual::R => "R",
            Qual::S => "S",
            Qual::L => "L",
            Qual::D => "D",
            Qual::P => "P",
            Qual::P1 => "P1",
            Qual::Ds => "DS",
        }
    }
}

#[derive(Debug, Clone)]
enum Target {
    /// An action of the POU or a BOOL variable, by name as written.
    Name(String),
    /// Statements written in the action block (Table 57 feature 4).
    Inline(Vec<Stmt>),
}

/// One action of an action block: an association of an action with a step (§6.7.4.3).
#[derive(Debug, Clone)]
struct Assoc {
    id: u64,
    line: u32,
    qual: Qual,
    duration: Option<Expr>,
    target: Target,
}

#[derive(Debug, Clone)]
enum Kind {
    Step { name: String, initial: bool },
    Jump { target: String },
    Transition { cond: Cond, priority: Option<u64> },
    SelDiv,
    SelConv,
    SimDiv,
    SimConv,
    Block { assocs: Vec<Assoc> },
}

#[derive(Debug, Clone)]
enum Cond {
    Expr(Expr, bool),
    Named(String, bool),
}

impl Kind {
    fn what(&self) -> &'static str {
        match self {
            Kind::Step { .. } => "step",
            Kind::Jump { .. } => "jump step",
            Kind::Transition { .. } => "transition",
            Kind::SelDiv => "selection divergence",
            Kind::SelConv => "selection convergence",
            Kind::SimDiv => "simultaneous divergence",
            Kind::SimConv => "simultaneous convergence",
            Kind::Block { .. } => "action block",
        }
    }
    /// What comes after a step: a transition, or what leads to transitions.
    fn after_step(&self) -> bool {
        matches!(self, Kind::Step { .. } | Kind::SelDiv | Kind::SimConv)
    }
    /// What comes after a transition: a step, or what leads to steps.
    fn after_transition(&self) -> bool {
        matches!(self, Kind::Transition { .. } | Kind::SelConv | Kind::SimDiv)
    }
}

#[derive(Debug, Clone)]
struct El {
    id: u64,
    pos: (f64, f64),
    line: u32,
    kind: Kind,
    ins: Vec<u64>,
}

/// An action of the POU (`actions/action`), its body read.
struct NamedAction {
    name: String,
    body: Vec<Stmt>,
}

/// A transition of the POU (`transitions/transition`), its condition read.
struct NamedTransition {
    name: String,
    cond: Expr,
}

/// A Sequential Function Chart POU with its statements, and the hidden variables they use; or,
/// when it cannot be lowered, marked as not read whole, so the checker says nothing about it.
pub(crate) fn pou(
    p: &GPou,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
    probes: &mut Vec<Probe>,
) -> ast::Pou {
    let mut tree = p.tree.clone();
    let Some((Lang::Sfc, body)) = &p.body else {
        return tree;
    };
    let before = diags.len();
    places.keywords_as_names = p.keywords_as_names.clone();
    let at = places.add(Place {
        doc: p.doc,
        pou: tree.name.clone(),
        local_id: None,
        position: None,
        variable: None,
        owner: None,
        field: None,
        len: 0,
        tag: body.span,
        text_line: None,
    });
    let actions = named_actions(p, places, diags);
    let transitions = named_transitions(p, places, diags);
    let elems = elements(body, p, places, diags);
    if diags.len() == before {
        if let Some(chart) = Chart::of(&elems, &tree, &actions, &transitions, at, places, diags) {
            if diags.len() == before && chart.explore(places, diags) {
                let (stmts, vars, found) = chart.lower(&tree.name, &actions);
                tree.body = stmts;
                tree.vars.extend(vars);
                probes.extend(found);
                return tree;
            }
        }
    }
    tree.partial_body = true;
    tree
}

/// The Structured Text of a `body`, or the language it is in when that is another.
fn st_of(body: &Node) -> Result<String, String> {
    let lang = body
        .children
        .iter()
        .find(|c| matches!(c.name.as_str(), "IL" | "ST" | "FBD" | "LD" | "SFC"));
    match lang {
        Some(l) if l.name == "ST" => Ok(text_of(l)),
        Some(l) => Err(l.name.clone()),
        None => Err(String::new()),
    }
}

/// The character data of a formatted text: its own, or its XHTML children's.
fn text_of(n: &Node) -> String {
    let mut s = n.text.clone();
    for c in &n.children {
        s.push_str(&text_of(c));
    }
    s
}

/// Statements of a body of several lines, read as ST reads them; `None` when not all of it
/// could be read.
fn statements(
    places: &mut Places,
    at: u32,
    text: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<Vec<Stmt>> {
    let first = places.lines(at, "body", text);
    let (stmts, whole, said) = parser::Parser::new(lexer::tokens_at(text, first))
        .with_named_keywords(&places.keywords_as_names)
        .statements_alone();
    diags.extend(said);
    whole.then_some(stmts)
}

/// One expression, of one line or several, read as ST reads it.
fn condition(
    places: &mut Places,
    at: u32,
    field: &str,
    text: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let first = places.lines(at, field, text);
    let (expr, said) = parser::Parser::new(lexer::tokens_at(text, first))
        .with_named_keywords(&places.keywords_as_names)
        .expression_alone();
    diags.extend(said);
    expr
}

fn place_of(p: &GPou, places: &mut Places, node: &Node, owner: Option<Owner>) -> u32 {
    places.add(Place {
        doc: p.doc,
        pou: p.tree.name.clone(),
        local_id: None,
        position: None,
        variable: None,
        owner,
        field: None,
        len: 0,
        tag: node.span,
        text_line: None,
    })
}

fn named_actions(p: &GPou, places: &mut Places, diags: &mut Vec<Diagnostic>) -> Vec<NamedAction> {
    let mut out: Vec<NamedAction> = Vec::new();
    for a in p.actions.iter().flat_map(|n| n.children_named("action")) {
        let action = a.attr("name").unwrap_or("").to_string();
        let at = place_of(p, places, a, Some(Owner::Action(action.clone())));
        if !name(places, at, "name", &action, diags) {
            continue;
        }
        if out.iter().any(|o| o.name.eq_ignore_ascii_case(&action)) {
            diags.push(places.diag(
                at,
                format!("the POU has two actions {action}; an action has one name"),
            ));
            continue;
        }
        let Some(body) = a.child("body") else {
            diags.push(places.diag(at, format!("action {action} has no body")));
            continue;
        };
        match st_of(body) {
            Ok(text) => {
                if let Some(body) = statements(places, at, &text, diags) {
                    out.push(NamedAction { name: action, body });
                }
            }
            Err(lang) if lang.is_empty() => {
                diags.push(places.diag(at, format!("action {action} has an empty body")))
            }
            Err(lang) => diags.push(places.diag(
                at,
                format!("action {action} is in {lang}; an action in a Sequential Function Chart is read in ST only, yet"),
            )),
        }
    }
    out
}

fn named_transitions(
    p: &GPou,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Vec<NamedTransition> {
    let mut out: Vec<NamedTransition> = Vec::new();
    for t in p
        .transitions
        .iter()
        .flat_map(|n| n.children_named("transition"))
    {
        let tname = t.attr("name").unwrap_or("").to_string();
        let at = place_of(p, places, t, Some(Owner::Transition(tname.clone())));
        if !name(places, at, "name", &tname, diags) {
            continue;
        }
        if out.iter().any(|o| o.name.eq_ignore_ascii_case(&tname)) {
            diags.push(places.diag(
                at,
                format!("the POU has two transitions {tname}; a transition has one name"),
            ));
            continue;
        }
        let Some(body) = t.child("body") else {
            diags.push(places.diag(at, format!("transition {tname} has no body")));
            continue;
        };
        match st_of(body) {
            Ok(text) => {
                if let Some(cond) = condition(places, at, "body", &text, diags) {
                    out.push(NamedTransition { name: tname, cond });
                }
            }
            Err(lang) if lang.is_empty() => {
                diags.push(places.diag(at, format!("transition {tname} has an empty body")))
            }
            Err(lang) => diags.push(places.diag(
                at,
                format!("transition {tname} is in {lang}; a transition's condition is read in ST only, yet: one Boolean expression"),
            )),
        }
    }
    out
}

/// The connections of a `connectionPointIn`, by the `localId`s they name.
fn ins_of(cp: Option<&Node>, places: &Places, line: u32, diags: &mut Vec<Diagnostic>) -> Vec<u64> {
    let mut out = Vec::new();
    for c in cp
        .into_iter()
        .flat_map(|cp| cp.children_named("connection"))
    {
        match c
            .attr("refLocalId")
            .and_then(|t| t.trim().parse::<u64>().ok())
        {
            Some(id) => out.push(id),
            None => diags.push(places.diag(
                line,
                format!(
                    "a connection has no refLocalId that is a whole number ({})",
                    c.attr("refLocalId").unwrap_or("none")
                ),
            )),
        }
    }
    out
}

/// The elements of a chart's body; what cannot be read is said.
fn elements(body: &Node, p: &GPou, places: &mut Places, diags: &mut Vec<Diagnostic>) -> Vec<El> {
    let mut out = Vec::new();
    for node in &body.children {
        if matches!(node.name.as_str(), "addData" | "documentation" | "comment") {
            continue;
        }
        let id_text = node.attr("localId");
        let id = id_text.and_then(|t| t.trim().parse::<u64>().ok());
        let pos = node.child("position").and_then(|n| {
            Some((
                n.attr("x")?.trim().parse::<f64>().ok()?,
                n.attr("y")?.trim().parse::<f64>().ok()?,
            ))
        });
        let line = places.add(Place {
            doc: p.doc,
            pou: p.tree.name.clone(),
            local_id: id,
            position: pos,
            variable: None,
            owner: None,
            field: None,
            len: 0,
            tag: node.span,
            text_line: None,
        });
        let Some(id) = id else {
            diags.push(places.diag(
                line,
                match id_text {
                    Some(t) => format!("{}: its localId '{t}' is not a whole number", node.name),
                    None => format!("{}: it has no localId", node.name),
                },
            ));
            continue;
        };
        let Some(pos) = pos else {
            diags.push(places.diag(line, format!("{} {id} has no position (x, y)", node.name)));
            continue;
        };
        let before = diags.len();
        let one_in = |places: &Places, diags: &mut Vec<Diagnostic>| {
            ins_of(node.child("connectionPointIn"), places, line, diags)
        };
        let all_ins = |places: &Places, diags: &mut Vec<Diagnostic>| {
            let mut v = Vec::new();
            for cp in node.children_named("connectionPointIn") {
                v.extend(ins_of(Some(cp), places, line, diags));
            }
            v
        };
        let (kind, ins) = match node.name.as_str() {
            "step" => {
                let step = node.attr("name").unwrap_or("").to_string();
                if !name(places, line, "name", &step, diags) {
                    continue;
                }
                if flag(node, "negated", places, line, diags) {
                    diags.push(places.diag(
                        line,
                        format!("step {step} is negated; a step of IEC 61131-3 is not"),
                    ));
                }
                let initial = flag(node, "initialStep", places, line, diags);
                (
                    Kind::Step {
                        name: step,
                        initial,
                    },
                    one_in(places, diags),
                )
            }
            "jumpStep" => {
                let target = node.attr("targetName").unwrap_or("").to_string();
                if !name(places, line, "targetName", &target, diags) {
                    continue;
                }
                (Kind::Jump { target }, one_in(places, diags))
            }
            "transition" => {
                let priority = match node.attr("priority") {
                    None => None,
                    Some(t) => match t.trim().parse::<u64>() {
                        Ok(n) => Some(n),
                        Err(_) => {
                            diags.push(places.diag(
                                line,
                                format!(
                                    "transition {id}: its priority '{t}' is not a whole number"
                                ),
                            ));
                            None
                        }
                    },
                };
                let Some(cond) = transition_condition(node, id, line, places, diags) else {
                    continue;
                };
                (Kind::Transition { cond, priority }, one_in(places, diags))
            }
            "selectionDivergence" => (Kind::SelDiv, one_in(places, diags)),
            "selectionConvergence" => (Kind::SelConv, all_ins(places, diags)),
            "simultaneousDivergence" => (Kind::SimDiv, one_in(places, diags)),
            "simultaneousConvergence" => (Kind::SimConv, all_ins(places, diags)),
            "actionBlock" => {
                if flag(node, "negated", places, line, diags) {
                    diags.push(places.diag(
                        line,
                        format!("action block {id} is negated; an action block of IEC 61131-3 is not (Table 58)"),
                    ));
                }
                let assocs = associations(node, id, pos, p, places, diags);
                (Kind::Block { assocs }, one_in(places, diags))
            }
            "macroStep" => {
                diags.push(places.diag(
                    line,
                    format!("macroStep {id}: a macro step is not an element of IEC 61131-3 ed. 3; write its steps in the chart"),
                ));
                continue;
            }
            other => {
                diags.push(places.diag(
                    line,
                    format!(
                        "{other} {id}: this element is not read in a Sequential Function Chart yet (a transition's condition is written in ST, inline or as a transition of the POU)"
                    ),
                ));
                continue;
            }
        };
        if diags.len() == before {
            out.push(El {
                id,
                pos,
                line,
                kind,
                ins,
            });
        }
    }
    out
}

fn transition_condition(
    node: &Node,
    id: u64,
    line: u32,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<Cond> {
    let Some(c) = node.child("condition") else {
        diags.push(places.diag(
            line,
            format!("transition {id} has no condition; one that is always TRUE is written TRUE (§6.7.3)"),
        ));
        return None;
    };
    let negated = flag(c, "negated", places, line, diags);
    if let Some(r) = c.child("reference") {
        let n = r.attr("name").unwrap_or("");
        if !name(places, line, "reference", n, diags) {
            return None;
        }
        return Some(Cond::Named(n.to_string(), negated));
    }
    if let Some(inline) = c.child("inline") {
        return match st_of(inline) {
            Ok(text) => {
                condition(places, line, "condition", &text, diags).map(|e| Cond::Expr(e, negated))
            }
            Err(lang) if lang.is_empty() => {
                diags.push(places.diag(line, format!("transition {id} has an empty condition")));
                None
            }
            Err(lang) => {
                diags.push(places.diag(
                    line,
                    format!("transition {id}: a condition in {lang} is not read yet; write it in ST, one Boolean expression"),
                ));
                None
            }
        };
    }
    if c.child("connectionPointIn").is_some() {
        diags.push(places.diag(
            line,
            format!("transition {id}: a condition drawn as a network connected to the transition is not read yet; write it in ST, inline or as a transition of the POU"),
        ));
        return None;
    }
    diags.push(places.diag(line, format!("transition {id} has an empty condition")));
    None
}

fn associations(
    node: &Node,
    block: u64,
    pos: (f64, f64),
    p: &GPou,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Assoc> {
    let mut out = Vec::new();
    for a in node.children_named("action") {
        let id = a.attr("localId").and_then(|t| t.trim().parse::<u64>().ok());
        let line = places.add(Place {
            doc: p.doc,
            pou: p.tree.name.clone(),
            local_id: id.or(Some(block)),
            position: Some(pos),
            variable: None,
            owner: None,
            field: None,
            len: 0,
            tag: a.span,
            text_line: None,
        });
        let Some(id) = id else {
            diags.push(places.diag(
                line,
                format!("an action of action block {block} has no localId that is a whole number"),
            ));
            continue;
        };
        let qual = match a.attr("qualifier").unwrap_or("N") {
            "N" => Qual::N,
            "R" => Qual::R,
            "S" => Qual::S,
            "L" => Qual::L,
            "D" => Qual::D,
            "P" => Qual::P,
            "P1" => Qual::P1,
            "DS" => Qual::Ds,
            "SD" => {
                diags.push(places.diag(line, format!(
                    "action {id}: the qualifier SD is not taken yet: by the action control of IEC 61131-3 ed. 3 §6.7.4.6 rule c its delay is T#0s once its step is no longer active, so an SD action whose step is left before the delay starts at once (ADR-013 §2.17)"
                )));
                continue;
            }
            "SL" => {
                diags.push(places.diag(line, format!(
                    "action {id}: the qualifier SL is not taken yet: by the action control of IEC 61131-3 ed. 3 §6.7.4.6 rule c its limit is T#0s once its step is no longer active, so an SL action whose step is left before the limit ends at once (ADR-013 §2.17)"
                )));
                continue;
            }
            "P0" => {
                diags.push(places.diag(line, format!(
                    "action {id}: the qualifier P0 is not taken yet: its pulse is an F_TRIG (§6.7.4.6, Figure 23), which is TRUE at its first execution after a cold start when its input is FALSE (Table 44), so a P0 action would run at a cold start whatever its step (ADR-013 §2.17)"
                )));
                continue;
            }
            other => {
                diags.push(places.diag(
                    line,
                    format!("action {id}: '{other}' is not an action qualifier of IEC 61131-3 ed. 3 (Table 59: N, R, S, L, D, P, SD, DS, SL, P1, P0)"),
                ));
                continue;
            }
        };
        if a.attr("indicator").is_some_and(|i| !i.trim().is_empty()) {
            diags.push(places.diag(
                line,
                format!("action {id}: an indicator variable is not taken; its use is deprecated (IEC 61131-3 ed. 3 §6.7.4.4)"),
            ));
            continue;
        }
        let duration = match a.attr("duration").map(str::trim).filter(|d| !d.is_empty()) {
            Some(text) if qual.timed() => {
                match crate::expression(places, line, "duration", text, diags) {
                    Some(e) => Some(e),
                    None => continue,
                }
            }
            Some(_) => {
                diags.push(places.diag(
                    line,
                    format!("action {id}: the qualifier {} has no duration; only L, D, SD, DS and SL have one (§6.7.4.5)", qual.name()),
                ));
                continue;
            }
            None if qual.timed() => {
                diags.push(places.diag(
                    line,
                    format!(
                        "action {id}: the qualifier {} needs a duration, a TIME (§6.7.4.5)",
                        qual.name()
                    ),
                ));
                continue;
            }
            None => None,
        };
        let target = match (a.child("reference"), a.child("inline")) {
            (Some(r), None) => {
                let n = r.attr("name").unwrap_or("");
                if !name(places, line, "reference", n, diags) {
                    continue;
                }
                Target::Name(n.to_string())
            }
            (None, Some(body)) => match st_of(body) {
                Ok(text) => match statements(places, line, &text, diags) {
                    Some(stmts) => Target::Inline(stmts),
                    None => continue,
                },
                Err(lang) if lang.is_empty() => {
                    diags.push(places.diag(line, format!("action {id} has an empty body")));
                    continue;
                }
                Err(lang) => {
                    diags.push(places.diag(
                        line,
                        format!("action {id} is in {lang}; an action in a Sequential Function Chart is read in ST only, yet"),
                    ));
                    continue;
                }
            },
            (Some(_), Some(_)) => {
                diags.push(places.diag(
                    line,
                    format!("action {id} both names an action and has a body; it does one"),
                ));
                continue;
            }
            (None, None) => {
                diags.push(
                    places.diag(line, format!("action {id} names no action and has no body")),
                );
                continue;
            }
        };
        out.push(Assoc {
            id,
            line,
            qual,
            duration,
            target,
        });
    }
    out
}

/// What an action is.
#[derive(Debug, Clone)]
enum ActKind {
    /// A BOOL variable of the POU (Table 56 feature 1), by its declared name.
    Var(String),
    /// An action of the POU, by index into the named actions, and its name.
    Named(usize, String),
    /// Statements in an action block, by the association's `localId`.
    Inline(u64, Vec<Stmt>),
}

#[derive(Debug, Clone)]
struct Action {
    kind: ActKind,
    /// Its associations: the step (by index into `steps`), the qualifier, the duration and the
    /// association's `localId`.
    assocs: Vec<(usize, Qual, Option<Expr>, u64)>,
}

/// A chart resolved: its steps and transitions as a net, its actions, ready to explore and
/// lower.
struct Chart {
    /// Steps by position: name as declared, initial, element index.
    steps: Vec<(String, bool, usize)>,
    elems: Vec<El>,
    /// Transitions by position: element index, the steps before, the steps after (indices
    /// into `steps`), the condition, and the higher-priority transitions of its divergence
    /// (indices into `trans`).
    trans: Vec<Trans>,
    actions: Vec<Action>,
    span: Span,
}

struct Trans {
    el: usize,
    pre: Vec<usize>,
    post: Vec<usize>,
    cond: Expr,
    before: Vec<usize>,
}

impl Chart {
    fn of(
        elems: &[El],
        pou: &ast::Pou,
        actions: &[NamedAction],
        transitions: &[NamedTransition],
        at: u32,
        places: &Places,
        diags: &mut Vec<Diagnostic>,
    ) -> Option<Chart> {
        let before = diags.len();
        let span = places.span(at);
        // Ids: each element and each action of a block has its own.
        let mut index: BTreeMap<u64, usize> = BTreeMap::new();
        let mut ids: BTreeMap<u64, u32> = BTreeMap::new();
        for (k, e) in elems.iter().enumerate() {
            let mut own = vec![(e.id, e.line)];
            if let Kind::Block { assocs } = &e.kind {
                own.extend(assocs.iter().map(|a| (a.id, a.line)));
            }
            for (id, line) in own {
                if ids.insert(id, line).is_some() {
                    diags.push(places.diag(
                        line,
                        format!("two elements have the localId {id}; each element has its own"),
                    ));
                }
            }
            index.insert(e.id, k);
        }
        if diags.len() != before {
            return None;
        }
        // Names: steps, actions and transitions are names of the POU (§6.7.2, §6.7.3,
        // §6.7.4.1), each its own.
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        let mut claim = |upper: String, what: String, line: u32, diags: &mut Vec<Diagnostic>| {
            if let Some(first) = names.get(&upper) {
                diags.push(places.diag(
                    line,
                    format!("{what} has the name of {first}; each step, action and transition of a POU has a name of its own, apart from its variables"),
                ));
            } else {
                names.insert(upper, what);
            }
        };
        claim(
            pou.name.to_ascii_uppercase(),
            format!("the POU {}", pou.name),
            at,
            diags,
        );
        for v in &pou.vars {
            claim(
                v.name.to_ascii_uppercase(),
                format!("the variable {}", v.name),
                at,
                diags,
            );
        }
        let before_names = diags.len();
        for e in elems {
            if let Kind::Step { name, .. } = &e.kind {
                claim(
                    name.to_ascii_uppercase(),
                    format!("step {name}"),
                    e.line,
                    diags,
                );
            }
        }
        for a in actions {
            claim(
                a.name.to_ascii_uppercase(),
                format!("action {}", a.name),
                at,
                diags,
            );
        }
        for t in transitions {
            claim(
                t.name.to_ascii_uppercase(),
                format!("transition {}", t.name),
                at,
                diags,
            );
        }
        if diags.len() != before_names {
            return None;
        }
        // Steps, by position.
        let by_place = |a: &usize, b: &usize| {
            let (ea, eb) = (&elems[*a], &elems[*b]);
            ea.pos
                .1
                .total_cmp(&eb.pos.1)
                .then(ea.pos.0.total_cmp(&eb.pos.0))
                .then(ea.id.cmp(&eb.id))
        };
        let mut step_els: Vec<usize> = (0..elems.len())
            .filter(|k| matches!(elems[*k].kind, Kind::Step { .. }))
            .collect();
        step_els.sort_by(by_place);
        let steps: Vec<(String, bool, usize)> = step_els
            .iter()
            .map(|k| match &elems[*k].kind {
                Kind::Step { name, initial } => (name.clone(), *initial, *k),
                _ => unreachable!(),
            })
            .collect();
        let step_of_el: BTreeMap<usize, usize> =
            steps.iter().enumerate().map(|(s, st)| (st.2, s)).collect();
        // Links: what each element reads, its kind allowed, and how many read each.
        let mut users: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        for (k, e) in elems.iter().enumerate() {
            let count_ok = match e.kind {
                Kind::Step { .. } => e.ins.len() <= 1,
                Kind::SelConv | Kind::SimConv => !e.ins.is_empty(),
                _ => e.ins.len() == 1,
            };
            if !count_ok {
                let msg = match e.kind {
                    Kind::Step { ref name, .. } => format!(
                        "step {name} has {} links into it; links into one step are joined by a selection convergence (Table 61 feature 3)",
                        e.ins.len()
                    ),
                    Kind::SelConv | Kind::SimConv => {
                        format!("{} {} has no link into it", e.kind.what(), e.id)
                    }
                    _ if e.ins.is_empty() => {
                        format!("{} {} has no link into it", e.kind.what(), e.id)
                    }
                    _ => format!(
                        "{} {} has {} links into it; it has one",
                        e.kind.what(),
                        e.id,
                        e.ins.len()
                    ),
                };
                diags.push(places.diag(e.line, msg));
                continue;
            }
            for &from in &e.ins {
                let Some(&f) = index.get(&from) else {
                    diags.push(places.diag(
                        e.line,
                        format!(
                            "{} {} is connected to localId {from}, which no element of the body has",
                            e.kind.what(),
                            e.id
                        ),
                    ));
                    continue;
                };
                let src = &elems[f].kind;
                let ok = match e.kind {
                    Kind::Step { .. } | Kind::Jump { .. } => src.after_transition(),
                    Kind::Transition { .. } => src.after_step(),
                    Kind::SelDiv => matches!(src, Kind::Step { .. } | Kind::SimConv),
                    Kind::SelConv => matches!(src, Kind::Transition { .. } | Kind::SelConv),
                    Kind::SimDiv => matches!(src, Kind::Transition { .. } | Kind::SelConv),
                    Kind::SimConv => matches!(src, Kind::Step { .. } | Kind::SimConv),
                    Kind::Block { .. } => matches!(src, Kind::Step { .. }),
                };
                if !ok {
                    let reason = if matches!(e.kind, Kind::Block { .. }) {
                        "an action block is connected to a step".to_string()
                    } else if src.after_step() && e.kind.kind_wants_transition() {
                        "two steps are never linked directly: a transition is between them (IEC 61131-3 ed. 3 §6.7.5)".to_string()
                    } else if src.after_transition() && e.kind.kind_wants_step() {
                        "two transitions are never linked directly: a step is between them (IEC 61131-3 ed. 3 §6.7.5)".to_string()
                    } else {
                        format!(
                            "a {} does not follow a {} (Table 61)",
                            e.kind.what(),
                            src.what()
                        )
                    };
                    diags.push(places.diag(
                        e.line,
                        format!(
                            "{} {} is linked from {} {}; {reason}",
                            e.kind.what(),
                            e.id,
                            src.what(),
                            elems[f].id
                        ),
                    ));
                    continue;
                }
                if !matches!(e.kind, Kind::Block { .. }) {
                    users.entry(from).or_default().push(k);
                }
            }
        }
        if diags.len() != before {
            return None;
        }
        for (k, e) in elems.iter().enumerate() {
            let n = users.get(&e.id).map_or(0, Vec::len);
            let ok = match e.kind {
                Kind::Step { .. } => n <= 1,
                Kind::Transition { .. } | Kind::SelConv | Kind::SimConv => n == 1,
                Kind::SelDiv | Kind::SimDiv => n >= 1,
                Kind::Jump { .. } | Kind::Block { .. } => true,
            };
            if !ok {
                let msg = match (&e.kind, n) {
                    (Kind::Step { name, .. }, _) => format!(
                        "step {name} is followed by {n} elements; the transitions after a step hang from a selection divergence (Table 61 feature 2)"
                    ),
                    (Kind::Transition { .. }, 0) => {
                        format!("transition {} leads to no step", e.id)
                    }
                    (_, 0) => format!("{} {} leads nowhere", e.kind.what(), e.id),
                    _ => format!(
                        "{} {} is followed by {n} elements; it has one after it",
                        e.kind.what(),
                        e.id
                    ),
                };
                diags.push(places.diag(elems[k].line, msg));
            }
        }
        // Jumps go to a step of the body.
        let step_named = |n: &str| steps.iter().position(|s| s.0.eq_ignore_ascii_case(n));
        for e in elems {
            if let Kind::Jump { target } = &e.kind {
                if step_named(target).is_none() {
                    diags.push(places.diag(
                        e.line,
                        format!(
                            "jump step {} goes to {target}, which no step of the body is named",
                            e.id
                        ),
                    ));
                }
            }
        }
        if diags.len() != before {
            return None;
        }
        // The net: the steps before and after each transition.
        let mut trans_els: Vec<usize> = (0..elems.len())
            .filter(|k| matches!(elems[*k].kind, Kind::Transition { .. }))
            .collect();
        trans_els.sort_by(by_place);
        let t_of_el: BTreeMap<usize, usize> =
            trans_els.iter().enumerate().map(|(t, &k)| (k, t)).collect();
        let resolve = |start: usize, step_side: bool| -> Vec<usize> {
            // Up (step_side): the steps an element after steps stands for; down: the
            // transitions an element after transitions stands for. Without recursion, each
            // element once.
            let mut out = BTreeSet::new();
            let mut seen = BTreeSet::new();
            let mut todo = vec![start];
            while let Some(k) = todo.pop() {
                if !seen.insert(k) {
                    continue;
                }
                let e = &elems[k];
                match (&e.kind, step_side) {
                    (Kind::Step { .. }, true) => {
                        out.insert(step_of_el[&k]);
                    }
                    (Kind::Transition { .. }, false) => {
                        out.insert(t_of_el[&k]);
                    }
                    _ => todo.extend(e.ins.iter().map(|id| index[id])),
                }
            }
            out.into_iter().collect()
        };
        let mut trans: Vec<Trans> = Vec::new();
        for &k in &trans_els {
            let pre = resolve(index[&elems[k].ins[0]], true);
            let cond = match &elems[k].kind {
                Kind::Transition { cond, .. } => match cond {
                    Cond::Expr(e, negated) => negate(e.clone(), *negated),
                    Cond::Named(n, negated) => {
                        match transitions.iter().find(|t| t.name.eq_ignore_ascii_case(n)) {
                            Some(t) => negate(t.cond.clone(), *negated),
                            None => {
                                diags.push(places.diag(
                                    elems[k].line,
                                    format!("transition {} names the transition {n}, which the POU's transitions do not have", elems[k].id),
                                ));
                                bool_expr(false, span)
                            }
                        }
                    }
                },
                _ => unreachable!(),
            };
            trans.push(Trans {
                el: k,
                pre,
                post: Vec::new(),
                cond,
                before: Vec::new(),
            });
        }
        for (k, e) in elems.iter().enumerate() {
            let step = match &e.kind {
                Kind::Step { .. } => step_of_el[&k],
                Kind::Jump { target } => match step_named(target) {
                    Some(s) => s,
                    None => continue,
                },
                _ => continue,
            };
            for &from in &e.ins {
                for t in resolve(index[&from], false) {
                    if !trans[t].post.contains(&step) {
                        trans[t].post.push(step);
                    }
                }
            }
        }
        // Links that go round among convergences reach no step.
        for tr in &trans {
            let e = &elems[tr.el];
            if tr.pre.is_empty() {
                diags.push(places.diag(e.line, format!("transition {} follows no step", e.id)));
            } else if tr.post.is_empty() {
                diags.push(places.diag(e.line, format!("transition {} leads to no step", e.id)));
            }
        }
        if diags.len() != before {
            return None;
        }
        // Priorities within a selection divergence (Table 61 features 2a, 2b).
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (t, tr) in trans.iter().enumerate() {
            let input = index[&elems[tr.el].ins[0]];
            if matches!(elems[input].kind, Kind::SelDiv) {
                groups.entry(input).or_default().push(t);
            }
        }
        for (div, mut members) in groups {
            let prio = |t: usize| match elems[trans[t].el].kind {
                Kind::Transition { priority, .. } => priority,
                _ => None,
            };
            let given = members.iter().filter(|t| prio(**t).is_some()).count();
            if given == members.len() {
                members.sort_by_key(|t| (prio(*t), *t));
                for w in members.windows(2) {
                    if prio(w[0]) == prio(w[1]) {
                        diags.push(places.diag(
                            elems[trans[w[1]].el].line,
                            format!(
                                "transitions {} and {} of selection divergence {} have the priority {}; each has its own (Table 61 feature 2b)",
                                elems[trans[w[0]].el].id,
                                elems[trans[w[1]].el].id,
                                elems[div].id,
                                prio(w[0]).unwrap_or(0)
                            ),
                        ));
                    }
                }
            } else if given > 0 {
                diags.push(places.diag(
                    elems[div].line,
                    format!("selection divergence {}: some of its transitions give a priority and some do not; all do (Table 61 feature 2b) or none, and then the leftmost goes first (feature 2a)", elems[div].id),
                ));
            } else {
                members.sort_by(|a, b| {
                    let (ea, eb) = (&elems[trans[*a].el], &elems[trans[*b].el]);
                    ea.pos
                        .0
                        .total_cmp(&eb.pos.0)
                        .then(ea.pos.1.total_cmp(&eb.pos.1))
                        .then(ea.id.cmp(&eb.id))
                });
            }
            for (k, &t) in members.iter().enumerate() {
                trans[t].before = members[..k].to_vec();
            }
        }
        // Each chart, a connected set of steps and transitions, has one initial step (§6.7.2
        // error 1).
        let mut parent: Vec<usize> = (0..steps.len()).collect();
        fn root(parent: &mut [usize], mut k: usize) -> usize {
            while parent[k] != k {
                parent[k] = parent[parent[k]];
                k = parent[k];
            }
            k
        }
        for t in &trans {
            let all: Vec<usize> = t.pre.iter().chain(&t.post).copied().collect();
            for w in all.windows(2) {
                let (a, b) = (root(&mut parent, w[0]), root(&mut parent, w[1]));
                parent[a] = b;
            }
        }
        let mut charts: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for s in 0..steps.len() {
            let r = root(&mut parent, s);
            charts.entry(r).or_default().push(s);
        }
        for members in charts.values() {
            let initial: Vec<&str> = members
                .iter()
                .filter(|s| steps[**s].1)
                .map(|s| steps[*s].0.as_str())
                .collect();
            let names: Vec<&str> = members.iter().map(|s| steps[*s].0.as_str()).collect();
            if initial.len() != 1 {
                let first = elems[steps[members[0]].2].line;
                diags.push(places.diag(
                    first,
                    if initial.is_empty() {
                        format!("the chart of the steps {} has no initial step; each chart has exactly one (IEC 61131-3 ed. 3 §6.7.2)", names.join(", "))
                    } else {
                        format!("the chart of the steps {} has {} initial steps ({}); each chart has exactly one (IEC 61131-3 ed. 3 §6.7.2)", names.join(", "), initial.len(), initial.join(", "))
                    },
                ));
            }
        }
        if steps.is_empty() {
            diags.push(places.diag(
                at,
                "the chart has no step; it has exactly one initial step (IEC 61131-3 ed. 3 §6.7.2)",
            ));
        }
        // Actions: in the order of the steps, then of their blocks, top to bottom.
        let mut blocks: Vec<usize> = (0..elems.len())
            .filter(|k| matches!(elems[*k].kind, Kind::Block { .. }))
            .collect();
        blocks.sort_by(|a, b| {
            let (sa, sb) = (
                step_of_el[&index[&elems[*a].ins[0]]],
                step_of_el[&index[&elems[*b].ins[0]]],
            );
            sa.cmp(&sb).then(by_place(a, b))
        });
        let mut acts: Vec<Action> = Vec::new();
        let mut by_key: BTreeMap<String, usize> = BTreeMap::new();
        for &b in &blocks {
            let step = step_of_el[&index[&elems[b].ins[0]]];
            let Kind::Block { assocs } = &elems[b].kind else {
                unreachable!()
            };
            for a in assocs {
                let (key, kind) = match &a.target {
                    Target::Inline(stmts) => {
                        (format!("#{}", a.id), ActKind::Inline(a.id, stmts.clone()))
                    }
                    Target::Name(n) => {
                        if let Some(k) = actions.iter().position(|x| x.name.eq_ignore_ascii_case(n))
                        {
                            (
                                n.to_ascii_uppercase(),
                                ActKind::Named(k, actions[k].name.clone()),
                            )
                        } else if let Some(v) =
                            pou.vars.iter().find(|v| v.name.eq_ignore_ascii_case(n))
                        {
                            let is_bool = matches!(&v.ty.kind, TypeSpecKind::Named(t) if t.eq_ignore_ascii_case("BOOL"));
                            if !is_bool || !matches!(v.section, Section::Var | Section::Output) {
                                diags.push(places.diag(
                                    a.line,
                                    format!("action {}: {n} is a variable that is not a BOOL of VAR or VAR_OUTPUT; a variable that is an action is one (Table 56 feature 1)", a.id),
                                ));
                                continue;
                            }
                            (n.to_ascii_uppercase(), ActKind::Var(v.name.clone()))
                        } else {
                            diags.push(places.diag(
                                a.line,
                                format!("action {}: {n} names no action of the POU and no BOOL variable", a.id),
                            ));
                            continue;
                        }
                    }
                };
                let at = *by_key.entry(key).or_insert_with(|| {
                    acts.push(Action {
                        kind,
                        assocs: Vec::new(),
                    });
                    acts.len() - 1
                });
                acts[at]
                    .assocs
                    .push((step, a.qual, a.duration.clone(), a.id));
            }
        }
        // Writes the chart owns: a step's flag and time, an action's flag, a variable that is
        // an action (Table 54 footnote a; §6.7.4.1; §6.7.4.6 rule a). And RETURN, which would
        // leave the whole chart's scan.
        let mut owned: Vec<(String, String)> = Vec::new();
        for s in &steps {
            owned.push((
                s.0.to_ascii_uppercase(),
                format!("{} is step {}: only the chart writes its flag and time (IEC 61131-3 ed. 3 §6.7.2, Table 54 footnote a)", s.0, s.0),
            ));
        }
        for a in &acts {
            match &a.kind {
                ActKind::Var(v) => owned.push((
                    v.to_ascii_uppercase(),
                    format!("{v} is an action of the chart: only the chart writes it (IEC 61131-3 ed. 3 §6.7.4.1)"),
                )),
                ActKind::Named(k, _) => owned.push((
                    actions[*k].name.to_ascii_uppercase(),
                    format!("{} is an action of the chart: its flag is read-only (IEC 61131-3 ed. 3 §6.7.4.6 rule a)", actions[*k].name),
                )),
                ActKind::Inline(..) => {}
            }
        }
        for a in &acts {
            let body = match &a.kind {
                ActKind::Named(k, _) => &actions[*k].body,
                ActKind::Inline(_, b) => b,
                ActKind::Var(_) => continue,
            };
            writes(body, &owned, diags);
        }
        for a in actions {
            // An action no step uses is not run; what it writes is still checked.
            if !acts
                .iter()
                .any(|x| matches!(&x.kind, ActKind::Named(_, n) if *n == a.name))
            {
                writes(&a.body, &owned, diags);
            }
        }
        if diags.len() != before {
            return None;
        }
        Some(Chart {
            steps,
            elems: elems.to_vec(),
            trans,
            actions: acts,
            span,
        })
    }

    /// Explore every situation the chart can reach, as if every condition could be either:
    /// refuse an unsafe or unreachable chart, or two time-related associations of one action
    /// active together. `false` when one was found.
    fn explore(&self, places: &Places, diags: &mut Vec<Diagnostic>) -> bool {
        let n = self.steps.len();
        let words = n.div_ceil(64).max(1);
        let has = |m: &[u64], s: usize| m[s / 64] >> (s % 64) & 1 == 1;
        let set = |m: &mut [u64], s: usize, on: bool| {
            if on {
                m[s / 64] |= 1 << (s % 64);
            } else {
                m[s / 64] &= !(1 << (s % 64));
            }
        };
        let mut start = vec![0u64; words];
        for (s, st) in self.steps.iter().enumerate() {
            if st.1 {
                set(&mut start, s, true);
            }
        }
        let names = |m: &[u64]| -> String {
            (0..n)
                .filter(|s| has(m, *s))
                .map(|s| self.steps[s].0.clone())
                .collect::<Vec<_>>()
                .join(", ")
        };
        // The pairs of steps holding time-related associations of one action (rule d).
        let mut timed: Vec<(usize, usize, usize)> = Vec::new();
        for (k, a) in self.actions.iter().enumerate() {
            let ts: Vec<usize> = a
                .assocs
                .iter()
                .filter(|x| x.1.timed())
                .map(|x| x.0)
                .collect();
            for i in 0..ts.len() {
                for j in i + 1..ts.len() {
                    timed.push((k, ts[i], ts[j]));
                }
            }
        }
        let mut seen: HashSet<Vec<u64>> = HashSet::new();
        let mut active = vec![false; n];
        let mut enabled = vec![false; self.trans.len()];
        let mut todo = VecDeque::new();
        seen.insert(start.clone());
        todo.push_back(start);
        let mut told_timed = vec![false; self.actions.len()];
        let before = diags.len();
        while let Some(m) = todo.pop_front() {
            for (s, a) in active.iter_mut().enumerate() {
                *a |= has(&m, s);
            }
            for &(k, a, b) in &timed {
                if has(&m, a) && has(&m, b) && !told_timed[k] {
                    told_timed[k] = true;
                    let line = self.elems[self.steps[b].2].line;
                    diags.push(places.diag(
                        line,
                        format!(
                            "{} has time-related associations in steps {} and {}, which can be active together; one action has at most one active time-related association (IEC 61131-3 ed. 3 §6.7.4.6 rule d)",
                            self.action_name(k),
                            self.steps[a].0,
                            self.steps[b].0
                        ),
                    ));
                }
            }
            for (t, tr) in self.trans.iter().enumerate() {
                if !tr.pre.iter().all(|s| has(&m, *s)) {
                    continue;
                }
                enabled[t] = true;
                let mut next = m.clone();
                for &s in &tr.pre {
                    set(&mut next, s, false);
                }
                for &s in &tr.post {
                    if has(&next, s) {
                        let id = self.elems[tr.el].id;
                        diags.push(places.diag(
                            self.elems[tr.el].line,
                            format!(
                                "the chart is unsafe: where the steps {} are active, clearing transition {id} activates step {}, which is active already (IEC 61131-3 ed. 3 §6.7.5, Figure 26 a)",
                                names(&m),
                                self.steps[s].0
                            ),
                        ));
                        return false;
                    }
                    set(&mut next, s, true);
                }
                if seen.contains(&next) {
                    continue;
                }
                if seen.len() >= MAX_SITUATIONS {
                    diags.push(places.diag(
                        self.elems[tr.el].line,
                        format!(
                            "the chart can be in more than {MAX_SITUATIONS} situations (sets of active steps); this compiler checks each for safety and reachability (IEC 61131-3 ed. 3 §6.7.5) and does not check this one: split it into charts with fewer simultaneous sequences"
                        ),
                    ));
                    return false;
                }
                if seen.insert(next.clone()) {
                    todo.push_back(next);
                }
            }
        }
        for (s, st) in self.steps.iter().enumerate() {
            if !active[s] {
                diags.push(places.diag(
                    self.elems[st.2].line,
                    format!(
                        "step {} can never be active: no situation the chart reaches activates it (IEC 61131-3 ed. 3 §6.7.5, an unreachable chart)",
                        st.0
                    ),
                ));
            }
        }
        for (t, tr) in self.trans.iter().enumerate() {
            if !enabled[t] && tr.pre.iter().all(|s| active[*s]) {
                let pre: Vec<&str> = tr.pre.iter().map(|s| self.steps[*s].0.as_str()).collect();
                diags.push(places.diag(
                    self.elems[tr.el].line,
                    format!(
                        "transition {} can never be cleared: the steps before it, {}, are never active together (IEC 61131-3 ed. 3 §6.7.5, Figure 26 b)",
                        self.elems[tr.el].id,
                        pre.join(", ")
                    ),
                ));
            }
        }
        diags.len() == before
    }

    fn action_name(&self, k: usize) -> String {
        match &self.actions[k].kind {
            ActKind::Var(v) => format!("the action {v}"),
            ActKind::Named(_, n) => format!("the action {n}"),
            ActKind::Inline(id, _) => format!("action {id}"),
        }
    }

    /// The statements of a scan, the variables they keep, and the probes for monitoring.
    fn lower(&self, pou: &str, named: &[NamedAction]) -> (Vec<Stmt>, Vec<VarDecl>, Vec<Probe>) {
        let span = self.span;
        let mut hidden = Hidden::default();
        let mut vars: Vec<VarDecl> = Vec::new();
        let mut probes: Vec<Probe> = Vec::new();
        let mut out: Vec<Stmt> = Vec::new();
        let named_ty = |ty: &str| TypeSpec {
            span,
            kind: TypeSpecKind::Named(ty.into()),
        };
        let time_ty = || named_ty("TIME");
        // Steps: a variable by its name.
        for (name, initial, el) in &self.steps {
            vars.push(VarDecl {
                name: name.clone(),
                span,
                section: Section::Var,
                retention: Retention::Unqualified,
                ty: named_ty(if *initial {
                    INITIAL_STEP_TYPE
                } else {
                    STEP_TYPE
                }),
                init: None,
                hidden: true,
            });
            let id = self.elems[*el].id;
            probes.push(Probe {
                pou: pou.into(),
                local_id: id,
                at: At::Step,
                holds: Holds::Variable {
                    path: format!("{name}.X"),
                    negated: false,
                },
            });
            probes.push(Probe {
                pou: pou.into(),
                local_id: id,
                at: At::StepTime,
                holds: Holds::Variable {
                    path: format!("{name}.T"),
                    negated: false,
                },
            });
        }
        // The shape of the chart, for an online change.
        let shape = hidden.var("__SFC_SHAPE".into(), named_ty("DINT"), span);
        let hash = self.shape_hash() as i128;
        if let Some(v) = hidden.vars.iter_mut().find(|v| v.name == "__SFC_SHAPE") {
            v.init = Some(Init::Value(int(hash, span)));
        }
        let clock = hidden.var("__SFC_CLOCK".into(), time_ty(), span);
        let dt = hidden.var("__SFC_DT".into(), time_ty(), span);
        // Whether this instance has been scanned: an instance first called after the runtime's
        // clock has run (added by an online change, or first called from a step entered late)
        // takes the clock at its first scan, so its initial step's time starts at zero there
        // and not at the runtime's uptime.
        let started = hidden.var("__SFC_STARTED".into(), named_ty("BOOL"), span);
        // What starts the chart again: every step as it starts, every action control idle.
        let mut reset: Vec<Stmt> = Vec::new();
        for (name, initial, _) in &self.steps {
            reset.push(Stmt::Assign(
                member(name, "X", span),
                bool_expr(*initial, span),
            ));
            reset.push(Stmt::Assign(member(name, "T", span), time0(span)));
        }
        let mut control: Vec<Stmt> = Vec::new();
        for a in &self.actions {
            let (key, q) = match &a.kind {
                ActKind::Var(v) => (v.to_ascii_uppercase(), place(v, span)),
                ActKind::Named(k, _) => {
                    let n = &named[*k].name;
                    vars.push(VarDecl {
                        name: n.clone(),
                        span,
                        section: Section::Var,
                        retention: Retention::Unqualified,
                        ty: named_ty(ACTION_TYPE),
                        init: None,
                        hidden: true,
                    });
                    (n.to_ascii_uppercase(), member(n, "Q", span))
                }
                ActKind::Inline(id, _) => {
                    let key = id.to_string();
                    let q = hidden.var(format!("__SFC_{key}_Q"), bool_ty(span), span);
                    (key, q)
                }
            };
            for (_, _, _, id) in &a.assocs {
                probes.push(Probe {
                    pou: pou.into(),
                    local_id: *id,
                    at: At::Action,
                    holds: Holds::Variable {
                        path: q.text(),
                        negated: false,
                    },
                });
            }
            let input = |qual: Qual| -> Option<Expr> {
                let xs: Vec<Expr> = a
                    .assocs
                    .iter()
                    .filter(|x| x.1 == qual)
                    .map(|x| var(member(&self.steps[x.0].0, "X", span)))
                    .collect();
                (!xs.is_empty()).then(|| or_all(xs, span))
            };
            let has = |qual: Qual| a.assocs.iter().any(|x| x.1 == qual);
            let part = |hidden: &mut Hidden, what: &str, ty: &str| {
                hidden.var(format!("__SFC_{key}_{what}"), named_ty(ty), span)
            };
            let call = |inst: &ast::Place, args: Vec<(&str, Expr)>| {
                Stmt::Call(
                    inst.clone(),
                    args.into_iter()
                        .map(|(n, e)| Arg::In(n.into(), e, span))
                        .collect(),
                )
            };
            let r = input(Qual::R).unwrap_or_else(|| bool_expr(false, span));
            // T: the duration of the active time-related association, T#0s if none (rule c).
            let timer_pt = if a.assocs.iter().any(|x| x.1.timed()) {
                let t = hidden.var(format!("__SFC_{key}_T"), time_ty(), span);
                control.push(Stmt::Assign(t.clone(), time0(span)));
                for x in a.assocs.iter().filter(|x| x.1.timed()) {
                    control.push(Stmt::If(
                        vec![(
                            var(member(&self.steps[x.0].0, "X", span)),
                            vec![Stmt::Assign(
                                t.clone(),
                                x.2.clone().unwrap_or_else(|| time0(span)),
                            )],
                        )],
                        None,
                    ));
                }
                Some(var(t))
            } else {
                None
            };
            let pt = || timer_pt.clone().unwrap_or_else(|| time0(span));
            let mut terms: Vec<Expr> = Vec::new();
            if let Some(n) = input(Qual::N) {
                terms.push(n);
            }
            if has(Qual::S) {
                let ff = part(&mut hidden, "S_FF", "RS");
                control.push(call(
                    &ff,
                    vec![("S", input(Qual::S).unwrap()), ("R1", r.clone())],
                ));
                reset.push(call(
                    &ff,
                    vec![("S", bool_expr(false, span)), ("R1", bool_expr(true, span))],
                ));
                terms.push(var(sel(&ff, "Q1", span)));
            }
            if has(Qual::L) {
                let tmr = part(&mut hidden, "L_TMR", "TON");
                let l = input(Qual::L).unwrap();
                control.push(call(&tmr, vec![("IN", l.clone()), ("PT", pt())]));
                reset.push(call(
                    &tmr,
                    vec![("IN", bool_expr(false, span)), ("PT", time0(span))],
                ));
                terms.push(and(l, not(var(sel(&tmr, "Q", span)), span), span));
            }
            if has(Qual::D) {
                let tmr = part(&mut hidden, "D_TMR", "TON");
                control.push(call(
                    &tmr,
                    vec![("IN", input(Qual::D).unwrap()), ("PT", pt())],
                ));
                reset.push(call(
                    &tmr,
                    vec![("IN", bool_expr(false, span)), ("PT", time0(span))],
                ));
                terms.push(var(sel(&tmr, "Q", span)));
            }
            if has(Qual::P) {
                let trig = part(&mut hidden, "P_TRIG", "R_TRIG");
                control.push(call(&trig, vec![("CLK", input(Qual::P).unwrap())]));
                reset.push(call(&trig, vec![("CLK", bool_expr(false, span))]));
                terms.push(var(sel(&trig, "Q", span)));
            }
            if has(Qual::Ds) {
                let tmr = part(&mut hidden, "DS_TMR", "TON");
                let ff = part(&mut hidden, "DS_FF", "RS");
                control.push(call(
                    &tmr,
                    vec![("IN", input(Qual::Ds).unwrap()), ("PT", pt())],
                ));
                control.push(call(
                    &ff,
                    vec![("S", var(sel(&tmr, "Q", span))), ("R1", r.clone())],
                ));
                reset.push(call(
                    &tmr,
                    vec![("IN", bool_expr(false, span)), ("PT", time0(span))],
                ));
                reset.push(call(
                    &ff,
                    vec![("S", bool_expr(false, span)), ("R1", bool_expr(true, span))],
                ));
                terms.push(var(sel(&ff, "Q1", span)));
            }
            if has(Qual::P1) {
                let trig = part(&mut hidden, "P1_TRIG", "R_TRIG");
                control.push(call(&trig, vec![("CLK", input(Qual::P1).unwrap())]));
                reset.push(call(&trig, vec![("CLK", bool_expr(false, span))]));
                terms.push(var(sel(&trig, "Q", span)));
            }
            let mut qx = or_all(terms, span);
            if let Some(r) = input(Qual::R) {
                qx = and(not(r, span), qx, span);
            }
            control.push(Stmt::Assign(q.clone(), qx));
            match &a.kind {
                ActKind::Var(_) => {}
                ActKind::Named(k, _) => control.push(Stmt::If(
                    vec![(var(q.clone()), named[*k].body.clone())],
                    None,
                )),
                ActKind::Inline(_, body) => {
                    control.push(Stmt::If(vec![(var(q.clone()), body.clone())], None))
                }
            }
        }
        // 0. A build of another chart that took over this one's variables starts it again.
        reset.push(Stmt::Assign(shape.clone(), int(hash, span)));
        out.push(Stmt::If(
            vec![(
                Expr {
                    kind: ExprKind::Binary(
                        BinOp::Ne,
                        Box::new(var(shape)),
                        Box::new(int(hash, span)),
                    ),
                    span,
                },
                reset,
            )],
            None,
        ));
        // 1. The scan clock: the steps' elapsed times grow by the time since the last scan,
        // none across a restart (the clock starts again from zero; as the timers, §2.13).
        let now = Expr {
            kind: ExprKind::Intrinsic(Intrinsic::Now),
            span,
        };
        out.push(Stmt::If(
            vec![(
                Expr {
                    kind: ExprKind::Binary(
                        BinOp::Eq,
                        Box::new(var(started.clone())),
                        Box::new(bool_expr(false, span)),
                    ),
                    span,
                },
                vec![
                    Stmt::Assign(clock.clone(), now.clone()),
                    Stmt::Assign(started, bool_expr(true, span)),
                ],
            )],
            None,
        ));
        out.push(Stmt::Assign(
            dt.clone(),
            Expr {
                kind: ExprKind::Binary(
                    BinOp::Sub,
                    Box::new(now.clone()),
                    Box::new(var(clock.clone())),
                ),
                span,
            },
        ));
        out.push(Stmt::If(
            vec![(
                Expr {
                    kind: ExprKind::Binary(
                        BinOp::Lt,
                        Box::new(var(dt.clone())),
                        Box::new(time0(span)),
                    ),
                    span,
                },
                vec![Stmt::Assign(dt.clone(), time0(span))],
            )],
            None,
        ));
        out.push(Stmt::Assign(clock, now));
        for (name, _, _) in &self.steps {
            out.push(Stmt::If(
                vec![(
                    var(member(name, "X", span)),
                    vec![Stmt::Assign(
                        member(name, "T", span),
                        Expr {
                            kind: ExprKind::Binary(
                                BinOp::Add,
                                Box::new(var(member(name, "T", span))),
                                Box::new(var(dt.clone())),
                            ),
                            span,
                        },
                    )],
                )],
                None,
            ));
        }
        // 2. The actions.
        out.extend(control);
        // 3. Each enabled transition's condition.
        let mut cond_vars: Vec<(ast::Place, ast::Place)> = Vec::new();
        for tr in &self.trans {
            let id = self.elems[tr.el].id;
            let c = hidden.var(format!("__SFC_{id}"), bool_ty(span), span);
            let en = hidden.var(format!("__SFC_{id}_EN"), bool_ty(span), span);
            let pre: Vec<Expr> = tr
                .pre
                .iter()
                .map(|s| var(member(&self.steps[*s].0, "X", span)))
                .collect();
            out.push(Stmt::Assign(en.clone(), and_all(pre, span)));
            out.push(Stmt::If(
                vec![(
                    var(en.clone()),
                    vec![Stmt::Assign(c.clone(), tr.cond.clone())],
                )],
                None,
            ));
            probes.push(Probe {
                pou: pou.into(),
                local_id: id,
                at: At::Transition,
                holds: Holds::Guarded {
                    path: c.text(),
                    ran: en.text(),
                },
            });
            cond_vars.push((c, en));
        }
        // 4. Every transition that clears, at once: the first of a divergence by priority.
        let fires = |t: usize| -> Expr {
            let (c, en) = &cond_vars[t];
            let mut e = and(var(en.clone()), var(c.clone()), span);
            for &h in &self.trans[t].before {
                e = and(e, not(var(cond_vars[h].0.clone()), span), span);
            }
            e
        };
        for (s, (name, _, _)) in self.steps.iter().enumerate() {
            let ins: Vec<Expr> = (0..self.trans.len())
                .filter(|t| self.trans[*t].post.contains(&s))
                .map(fires)
                .collect();
            let outs: Vec<Expr> = (0..self.trans.len())
                .filter(|t| self.trans[*t].pre.contains(&s))
                .map(fires)
                .collect();
            let mut arms = Vec::new();
            if !ins.is_empty() {
                arms.push((
                    or_all(ins, span),
                    vec![
                        Stmt::Assign(member(name, "X", span), bool_expr(true, span)),
                        Stmt::Assign(member(name, "T", span), time0(span)),
                    ],
                ));
            }
            if !outs.is_empty() {
                arms.push((
                    or_all(outs, span),
                    vec![Stmt::Assign(
                        member(name, "X", span),
                        bool_expr(false, span),
                    )],
                ));
            }
            if !arms.is_empty() {
                out.push(Stmt::If(arms, None));
            }
        }
        vars.extend(hidden.vars);
        (out, vars, probes)
    }

    /// A hash of the chart's steps and transitions by name (FNV-1a, 31 bits), as a DINT.
    fn shape_hash(&self) -> i32 {
        let mut text = String::new();
        let mut steps: Vec<String> = self
            .steps
            .iter()
            .map(|(n, i, _)| format!("{}{}", n.to_ascii_uppercase(), if *i { "*" } else { "" }))
            .collect();
        steps.sort();
        text.push_str(&steps.join(","));
        let mut trans: Vec<String> = self
            .trans
            .iter()
            .map(|t| {
                let side = |v: &[usize]| {
                    let mut n: Vec<String> = v
                        .iter()
                        .map(|s| self.steps[*s].0.to_ascii_uppercase())
                        .collect();
                    n.sort();
                    n.join(",")
                };
                format!("{}>{}", side(&t.pre), side(&t.post))
            })
            .collect();
        trans.sort();
        text.push('|');
        text.push_str(&trans.join(";"));
        let mut h: u32 = 0x811c_9dc5;
        for b in text.bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
        (h & 0x7fff_ffff) as i32
    }
}

impl Kind {
    fn kind_wants_transition(&self) -> bool {
        matches!(
            self,
            Kind::Step { .. } | Kind::Jump { .. } | Kind::SelConv | Kind::SimDiv
        )
    }
    fn kind_wants_step(&self) -> bool {
        matches!(self, Kind::Transition { .. } | Kind::SelDiv | Kind::SimConv)
    }
}

/// Refuse a write to what the chart owns, and RETURN, in the statements of an action.
fn writes(stmts: &[Stmt], owned: &[(String, String)], diags: &mut Vec<Diagnostic>) {
    let check = |p: &ast::Place, diags: &mut Vec<Diagnostic>| {
        if let Some((_, why)) = owned.iter().find(|(n, _)| p.name.eq_ignore_ascii_case(n)) {
            diags.push(Diagnostic::at(p.span, why.clone()));
        }
    };
    for s in stmts {
        match s {
            Stmt::Assign(p, _) => check(p, diags),
            Stmt::Call(p, args) => {
                check(p, diags);
                for a in args {
                    if let Arg::Out(_, target, _) = a {
                        check(target, diags);
                    }
                }
            }
            Stmt::If(arms, otherwise) => {
                for (_, body) in arms {
                    writes(body, owned, diags);
                }
                if let Some(b) = otherwise {
                    writes(b, owned, diags);
                }
            }
            Stmt::Case(_, arms, otherwise) => {
                for (_, body) in arms {
                    writes(body, owned, diags);
                }
                if let Some(b) = otherwise {
                    writes(b, owned, diags);
                }
            }
            Stmt::For { var, body, .. } => {
                check(var, diags);
                writes(body, owned, diags);
            }
            Stmt::While(_, body) | Stmt::Repeat(body, _) => writes(body, owned, diags),
            Stmt::Exit(_) => {}
            Stmt::Return(span) => diags.push(Diagnostic::at(
                *span,
                "RETURN in an action would end the scan of the whole chart before its transitions; an action has no RETURN of its own (write IF … END_IF around what it skips)",
            )),
        }
    }
}

fn negate(e: Expr, negated: bool) -> Expr {
    if negated {
        let span = e.span;
        not(e, span)
    } else {
        e
    }
}

fn place(name: &str, span: Span) -> ast::Place {
    ast::Place {
        name: name.into(),
        span,
        path: Vec::new(),
    }
}

fn member(name: &str, m: &str, span: Span) -> ast::Place {
    ast::Place {
        name: name.into(),
        span,
        path: vec![Selector::Member(m.into(), span)],
    }
}

fn sel(p: &ast::Place, m: &str, span: Span) -> ast::Place {
    let mut p = p.clone();
    p.path.push(Selector::Member(m.into(), span));
    p
}

fn var(p: ast::Place) -> Expr {
    let span = p.span;
    Expr {
        kind: ExprKind::Var(p),
        span,
    }
}

fn not(e: Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Not(Box::new(e)),
        span,
    }
}

fn and(a: Expr, b: Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Binary(BinOp::And, Box::new(a), Box::new(b)),
        span,
    }
}

fn or_all(mut xs: Vec<Expr>, span: Span) -> Expr {
    if xs.is_empty() {
        return bool_expr(false, span);
    }
    let first = xs.remove(0);
    xs.into_iter().fold(first, |a, b| Expr {
        kind: ExprKind::Binary(BinOp::Or, Box::new(a), Box::new(b)),
        span,
    })
}

fn and_all(mut xs: Vec<Expr>, span: Span) -> Expr {
    if xs.is_empty() {
        return bool_expr(true, span);
    }
    let first = xs.remove(0);
    xs.into_iter().fold(first, |a, b| and(a, b, span))
}

fn time0(span: Span) -> Expr {
    Expr {
        kind: ExprKind::Time(0),
        span,
    }
}

fn int(value: i128, span: Span) -> Expr {
    if value < 0 {
        Expr {
            kind: ExprKind::Neg(Box::new(Expr {
                kind: ExprKind::Int(-value),
                span,
            })),
            span,
        }
    } else {
        Expr {
            kind: ExprKind::Int(value),
            span,
        }
    }
}
