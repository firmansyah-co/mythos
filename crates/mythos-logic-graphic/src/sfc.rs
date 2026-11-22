//! A Sequential Function Chart to statements of the Structured Text syntax tree (IEC 61131-3
//! ed. 3 §6.7; ADR-013 §2.17).
//!
//! **What is read.** A POU whose body is `SFC` (a program or a function block, §6.7.1), with:
//! `step` (`name`, `initialStep`), `transition` (its condition `inline` in ST, LD or FBD, a
//! `reference` to a transition of the POU's `transitions`, or a `connectionPointIn` from a
//! network of LD or FBD drawn in the chart; `negated`; `priority`), `selectionDivergence`,
//! `selectionConvergence`, `simultaneousDivergence`, `simultaneousConvergence`, `jumpStep`
//! (`targetName`), `actionBlock` (each `action` with its `qualifier`, `duration`, and a
//! `reference` to an action of the POU's `actions` or to a BOOL variable, or an `inline` body in
//! ST, LD or FBD), the elements of LD and FBD as networks giving transitions their conditions
//! (Table 55 features 2a–6a), and `comment`. The POU's `actions` (`action` with a body in ST:
//! statements; or in LD or FBD: networks, Table 56 features 2l, 2f) and `transitions`
//! (`transition` with a body in ST: one Boolean expression; or in LD or FBD: a network whose coil
//! or output variable of the transition's name takes the condition, Table 55 features 10, 11).
//! Everything else is refused by its name, with the element: an action or a condition in IL, an
//! action that is a chart (Table 56 feature 2s: what becomes of its steps when the action stops
//! is not said), a `macroStep` (not in ed. 3), the qualifier DL (not in Table 59), an action
//! block's indicator variable (deprecated, §6.7.4.4), a negated step or action block.
//!
//! **Bodies in LD and FBD** (ADR-013 §2.17, L3d-1b) are lowered by the LD and FBD front-ends
//! (`ld::lower`, `fbd::lower`) into the chart's POU, each in a namespace of its own
//! (`__SFC_<owner>__`, since its `localId`s are its own), an action's under `IF Q`, a
//! condition's under its transition's enable flag; the flags of a body's networks are set FALSE
//! before it may run, so a network of a body that did not run never reads as run. A condition's
//! network has no side effect (§6.7.3): it writes nothing but the transition's name, calls no
//! function block, senses no edge, is one network.
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
//! the active time-related association, `T#0s` when there is none (rule c). Two time-related
//! associations of one action whose steps can be active together are refused (rule d).
//!
//! **SD, SL and P0 do what Table 59 names, not what Figure 23 does at three edges** (ADR-013
//! §2.17, the owner's decision of 2026-11-22). By rule c the T of SD_TMR and SL_TMR is `T#0s`
//! once the step is left, so an SD action whose step is left before its delay would start at
//! once and an SL action would end at once; and P0's F_TRIG is TRUE at a cold start, so a P0
//! action would run whatever its step. Here each SD and each SL association is stored and
//! timed by itself, with its own duration kept once its step is left, so the delay and the
//! limit run in full. Where two associations of one action, or a step entered again, leave two
//! readings, the one with the action off is taken: an SD action starts when no association
//! that stored it is still in its delay (each counted from the last time its step was
//! entered), and stays started until R; an SL action ends at the first limit that is over
//! (each counted from when its step first stored it), and stays ended until R. "Entered" is
//! the step's flag rising: a step that loops to itself is not entered again for its action
//! control, whose input never falls, for these as for every qualifier. P0's edge
//! remembers FALSE at a start, so it pulses only in the scan after a step that was active is
//! left; several P0 associations are one input, their OR, as Figure 23 has P and P1. R ends
//! each at any time, and nothing is retained across a cold start: a delay that was running is
//! dropped.
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

use crate::graph::{
    self, bool_expr, bool_ty, check_graph, read_body, read_elem, Elem as GElem, Hidden,
    Kind as GKind, Lang, Lowered, Pin, Storage, NETWORK_ELEMENTS,
};
use crate::read::{flag, name, GPou, Known};
use crate::xml::Node;
use crate::{fbd, ld, At, Body, BodyOf, Holds, Owner, Place, Places, Probe};
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
    Sd,
    Sl,
    P0,
}

impl Qual {
    /// A time-related qualifier, which has a duration (§6.7.4.5).
    fn timed(self) -> bool {
        matches!(self, Qual::L | Qual::D | Qual::Ds | Qual::Sd | Qual::Sl)
    }
    /// A time-related qualifier whose timer runs only while its step is active, so the T of
    /// rule c is its duration. SD and SL run on after their step and keep their own.
    fn timed_while_active(self) -> bool {
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
            Qual::Sd => "SD",
            Qual::Sl => "SL",
            Qual::P0 => "P0",
        }
    }
}

#[derive(Debug, Clone)]
enum Target {
    /// An action of the POU or a BOOL variable, by name as written.
    Name(String),
    /// A body written in the action block (Table 57 feature 4, Table 58 features 4s, 4l, 4f).
    Inline(ActBody),
}

/// What an action does: statements of Structured Text (Table 56 feature 3s), or networks of
/// a Ladder Diagram or a Function Block Diagram (features 2l, 2f).
#[derive(Debug, Clone)]
enum ActBody {
    St(Vec<Stmt>),
    Graphic(Graphic),
}

/// A body in LD or FBD of an action or of a transition's condition, read and checked as a
/// graph (`graph::read_body`, `graph::check_graph`), lowered with the chart (ADR-013 §2.17).
#[derive(Debug, Clone)]
struct Graphic {
    lang: Lang,
    elems: Vec<GElem>,
    of: BodyOf,
}

impl Graphic {
    /// The namespace of its hidden variables: `__SFC_<owner>__`, the owner the action's or the
    /// transition's name in upper case, or the `localId` of the element whose body it is.
    fn prefix(&self) -> String {
        match &self.of {
            BodyOf::Action(n) | BodyOf::Transition(n) => {
                format!("__SFC_{}__", n.to_ascii_uppercase())
            }
            BodyOf::Inline(id) | BodyOf::Wired(id) => format!("__SFC_{id}__"),
        }
    }
    /// The hidden BOOL a condition's network writes its result to, in the place of the
    /// transition's name (§6.7.3 g).
    fn result(&self) -> String {
        format!("{}RESULT", self.prefix())
    }
    fn body(&self) -> Body {
        Body {
            of: self.of.clone(),
            ladder: self.lang == Lang::Ld,
        }
    }
    /// Lowered in the chart's POU, by its language's front-end.
    fn lower(
        &self,
        pou: &ast::Pou,
        known: &Known,
        places: &mut Places,
        diags: &mut Vec<Diagnostic>,
    ) -> Option<Lowered> {
        let prefix = self.prefix();
        match self.lang {
            Lang::Ld => ld::lower(
                &self.elems,
                pou,
                known,
                places,
                diags,
                &prefix,
                Some(self.body()),
            ),
            _ => {
                let mut elems = self.elems.clone();
                fbd::lower(
                    &mut elems,
                    pou,
                    known,
                    places,
                    diags,
                    &prefix,
                    Some(self.body()),
                )
            }
        }
    }
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
    /// A network in LD or FBD written in the transition (TC6 `condition/inline`), with the
    /// name it writes the condition to.
    Graphic(Graphic, String, bool),
    /// A network drawn in the chart's body, connected to the transition (TC6
    /// `condition/connectionPointIn`; Table 55 features 2a–6a).
    Wired(Pin, bool),
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
    body: ActBody,
}

/// A transition of the POU (`transitions/transition`), its condition read: an expression of
/// ST, or a network of LD or FBD that writes it to the transition's name.
struct NamedTransition {
    name: String,
    cond: CondBody,
}

enum CondBody {
    St(Expr),
    Graphic(Graphic),
}

/// A Sequential Function Chart POU with its statements, and the hidden variables they use; or,
/// when it cannot be lowered, marked as not read whole, so the checker says nothing about it.
pub(crate) fn pou(
    p: &GPou,
    known: &Known,
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
    let (elems, networks) = elements(body, p, places, diags);
    if diags.len() == before {
        if let Some(chart) = Chart::of(
            &elems,
            networks,
            &tree,
            &actions,
            &transitions,
            at,
            places,
            diags,
        ) {
            if diags.len() == before && chart.explore(places, diags) {
                if let Some((stmts, vars, found)) =
                    chart.lower(&tree, &actions, known, places, diags)
                {
                    tree.body = stmts;
                    tree.vars.extend(vars);
                    probes.extend(found);
                    return tree;
                }
            }
        }
    }
    tree.partial_body = true;
    tree
}

/// What a `body` is written in.
enum Written<'a> {
    St(String),
    /// LD or FBD: the element of that name, holding the network.
    Graphic(Lang, &'a Node),
    /// Another language by its TC6 name (IL, SFC); empty when the body holds none.
    Other(String),
}

fn written(body: &Node) -> Written<'_> {
    let lang = body
        .children
        .iter()
        .find(|c| matches!(c.name.as_str(), "IL" | "ST" | "FBD" | "LD" | "SFC"));
    match lang {
        Some(l) if l.name == "ST" => Written::St(text_of(l)),
        Some(l) if l.name == "LD" => Written::Graphic(Lang::Ld, l),
        Some(l) if l.name == "FBD" => Written::Graphic(Lang::Fbd, l),
        Some(l) => Written::Other(l.name.clone()),
        None => Written::Other(String::new()),
    }
}

/// Why an action in `lang` (IL, SFC) is not taken; `what` names it.
fn action_refused(what: &str, lang: &str) -> String {
    match lang {
        "SFC" => format!(
            "{what} is a chart (SFC); an action that is a chart (IEC 61131-3 ed. 3 Table 56 feature 2s) is not taken: the standard does not say what becomes of its steps when the action's Q goes FALSE, and with the action control without final scan (§6.7.4.6, ADR-013 §2.17) they would stay where they were and the chart go on from there at the action's next activation; write the chart as a function block with an SFC body and call it from the action"
        ),
        "IL" => format!(
            "{what} is in IL; this compiler has no Instruction List front-end: write the action in ST, LD or FBD"
        ),
        other => format!("{what} is in {other}; an action is written in ST, LD or FBD"),
    }
}

/// Why a condition in `lang` (IL, SFC) is not taken; `what` names the transition.
fn condition_refused(what: &str, lang: &str) -> String {
    match lang {
        "IL" => format!(
            "{what}: a condition in IL is not read; this compiler has no Instruction List front-end: write it in ST, LD or FBD"
        ),
        other => format!(
            "{what}: a condition in {other} is not one; a condition is one Boolean value (IEC 61131-3 ed. 3 §6.7.3), written in ST, LD or FBD"
        ),
    }
}

/// An action's body in LD or FBD, `node` the element of that language: its elements read and
/// checked as a graph. A return is refused: it would end the scan of the whole chart.
fn graphic_action(
    node: &Node,
    lang: Lang,
    of: BodyOf,
    p: &GPou,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<Graphic> {
    let before = diags.len();
    let owner = owner_of(&of);
    let elems = read_body(node, lang, p, Some(&owner), places, diags);
    if diags.len() != before || !check_graph(&elems, lang, places, diags) {
        return None;
    }
    for e in &elems {
        if matches!(e.kind, GKind::Return) {
            diags.push(places.diag(
                e.line,
                format!(
                    "return {}: a return in an action would end the scan of the whole chart before its transitions; an action has no return of its own (jump to a label at its end instead)",
                    e.id
                ),
            ));
        }
    }
    (diags.len() == before).then_some(Graphic { lang, elems, of })
}

/// A condition's body in LD or FBD (Table 55 features 10, 11; TC6 `inline` or a transition of
/// the POU): its elements read and checked, and its network checked to give one Boolean value,
/// written to `name`, the transition's, with no other effect (§6.7.3).
fn graphic_condition(
    node: &Node,
    lang: Lang,
    of: BodyOf,
    name: &str,
    p: &GPou,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<Graphic> {
    let before = diags.len();
    let owner = owner_of(&of);
    let elems = read_body(node, lang, p, Some(&owner), places, diags);
    if diags.len() != before || !check_graph(&elems, lang, places, diags) {
        return None;
    }
    let mut g = Graphic { lang, elems, of };
    let result = g.result();
    condition_network(
        &mut g.elems,
        lang,
        Sink::Named(name),
        &result,
        places,
        diags,
    )
    .then_some(g)
}

/// The place an element of a body of `of` names as its owner in a diagnostic.
fn owner_of(of: &BodyOf) -> Owner {
    match of {
        BodyOf::Action(n) => Owner::Action(n.clone()),
        BodyOf::Transition(n) => Owner::Transition(n.clone()),
        BodyOf::Inline(id) | BodyOf::Wired(id) => Owner::Inline(*id),
    }
}

/// What gives a condition's network its result.
enum Sink<'a> {
    /// The coil (LD) or output variable (FBD) that writes the transition's name.
    Named(&'a str),
    /// The element of this `localId`, which the chart added where the network reaches the
    /// transition.
    Element(u64),
}

/// Check that the network `elems` gives one Boolean value and does nothing else (§6.7.3: a
/// side effect is an error; Table 55 features 2a–6a, 10, 11), and make what gives it write
/// the hidden variable `result`. `false` when it does not, and the diagnostics say why.
///
/// Refused: a write to anything but the transition's name (a coil, an output or in-out
/// variable); a call of a function block (it changes the instance); an edge (it keeps what it
/// last sensed, between evaluations that happen only while the transition is enabled); a
/// label, a jump or a return (a condition is one network); an element that does not lead to
/// the result. A coil that gives the result neither sets, resets nor senses an edge: the
/// condition is a value of this scan.
fn condition_network(
    elems: &mut [GElem],
    lang: Lang,
    sink: Sink,
    result: &str,
    places: &Places,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let before = diags.len();
    let mut sinks: Vec<usize> = Vec::new();
    let named = |p: &ast::Place, n: &str| p.path.is_empty() && p.name.eq_ignore_ascii_case(n);
    let side = "a condition's network writes nothing but its transition's name (IEC 61131-3 ed. 3 §6.7.3: a side effect is an error)";
    for (k, e) in elems.iter().enumerate() {
        let is_sink = match (&sink, &e.kind) {
            (Sink::Element(id), _) => e.id == *id,
            (Sink::Named(n), GKind::Coil { var, .. }) => named(var, n),
            (Sink::Named(n), GKind::OutVar { target, .. }) => named(target, n),
            _ => false,
        };
        if is_sink {
            if let GKind::Coil { storage, edge, .. } = &e.kind {
                if *storage != Storage::None || *edge != graph::Edge::None {
                    diags.push(places.diag(
                        e.line,
                        format!(
                            "coil {} sets, resets or senses an edge; the coil of the transition's name takes the condition, a value of this scan (IEC 61131-3 ed. 3 §6.7.3, Table 55 feature 10)",
                            e.id
                        ),
                    ));
                }
            }
            sinks.push(k);
            continue;
        }
        let refusal = match &e.kind {
            GKind::Coil { var, .. } => Some(format!("coil {} writes {}; {side}", e.id, var.text())),
            GKind::OutVar { target, .. } | GKind::InOutVar { target, .. } => Some(format!(
                "{} {} writes {}; {side}",
                e.what(),
                e.id,
                target.text()
            )),
            GKind::Block {
                instance: Some(inst),
                type_name,
                ..
            } => Some(format!(
                "block {} calls the function block {} ({type_name}); a call changes the instance, and {side}: call it in an action and read its output",
                e.id,
                inst.text()
            )),
            GKind::Contact { edge, .. } if *edge != graph::Edge::None => Some(format!(
                "contact {} senses an edge; it keeps what it last sensed, and a condition is evaluated only while its transition is enabled, so it would sense an edge since then and not since the last scan: {side} (sense the edge in an action, with R_TRIG or F_TRIG)",
                e.id
            )),
            GKind::Label(_) | GKind::Jump(_) | GKind::Return => Some(format!(
                "{} {}: a condition is one network giving one Boolean value (IEC 61131-3 ed. 3 §6.7.3); it has no labels, jumps or returns",
                e.what(),
                e.id
            )),
            _ => None,
        };
        if let Some(r) = refusal {
            diags.push(places.diag(e.line, r));
        }
    }
    let gives = match lang {
        Lang::Ld => "a coil",
        _ => "an output variable",
    };
    match (&sink, sinks.as_slice()) {
        (_, [_]) => {}
        (Sink::Named(n), []) => {
            let at = elems.first().map(|e| e.line);
            let message = format!(
                "the condition's network writes no {n}: {gives} of the transition's name, {n}, takes the condition (IEC 61131-3 ed. 3 §6.7.3 g, Table 55 features 10, 11)"
            );
            match at {
                Some(line) => diags.push(places.diag(line, message)),
                None => diags.push(Diagnostic::whole(message)),
            }
        }
        (Sink::Named(n), [_, second, ..]) => diags.push(places.diag(
            elems[*second].line,
            format!(
                "{} {} writes {n} too; one coil or output variable of the transition's name takes the condition (IEC 61131-3 ed. 3 §6.7.3)",
                elems[*second].what(),
                elems[*second].id
            ),
        )),
        (Sink::Element(_), _) => unreachable!("the chart adds the element once"),
    }
    if diags.len() != before {
        return false;
    }
    // No connection goes round (a feedback path, §8.1.5.2, which FBD takes): a value carried
    // from one evaluation to the next is state, as an edge's memory is, and a condition is
    // evaluated only while its transition is enabled. Found without recursion: 0 not seen, 1 on
    // the way, 2 done.
    let ids: BTreeMap<u64, usize> = elems.iter().enumerate().map(|(k, e)| (e.id, k)).collect();
    let ups = |k: usize| -> Vec<usize> {
        elems[k]
            .inputs
            .iter()
            .flat_map(|p| p.conns.iter().filter_map(|c| ids.get(&c.from).copied()))
            .collect()
    };
    let mut state = vec![0u8; elems.len()];
    'roots: for root in 0..elems.len() {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut trail = vec![(root, ups(root), 0usize)];
        while let Some(top) = trail.last_mut() {
            let Some(&next) = top.1.get(top.2) else {
                state[top.0] = 2;
                trail.pop();
                continue;
            };
            top.2 += 1;
            match state[next] {
                0 => {
                    state[next] = 1;
                    trail.push((next, ups(next), 0));
                }
                1 => {
                    let e = &elems[next];
                    diags.push(places.diag(
                        e.line,
                        format!(
                            "{} {} reads back its own output (a feedback path, IEC 61131-3 ed. 3 §8.1.5.2): it keeps a value from one evaluation to the next, and a condition is evaluated only while its transition is enabled, so it would carry a value of an earlier enabling: {side} (keep the state in an action)",
                            e.what(),
                            e.id
                        ),
                    ));
                    break 'roots;
                }
                _ => {}
            }
        }
    }
    if diags.len() != before {
        return false;
    }
    // Every element of the network leads to the result: what reaches it, element by element.
    let sink = sinks[0];
    let index: BTreeMap<u64, usize> = elems.iter().enumerate().map(|(k, e)| (e.id, k)).collect();
    let mut reached = vec![false; elems.len()];
    let mut todo = vec![sink];
    while let Some(k) = todo.pop() {
        if std::mem::replace(&mut reached[k], true) {
            continue;
        }
        for p in &elems[k].inputs {
            todo.extend(p.conns.iter().filter_map(|c| index.get(&c.from).copied()));
        }
    }
    for (k, e) in elems.iter().enumerate() {
        let apart = matches!(e.kind, GKind::LeftRail | GKind::RightRail | GKind::Comment);
        if !reached[k] && !apart {
            diags.push(places.diag(
                e.line,
                format!(
                    "{} {} does not lead to the condition; a condition is one network giving one Boolean value (IEC 61131-3 ed. 3 §6.7.3)",
                    e.what(),
                    e.id
                ),
            ));
        }
    }
    if diags.len() != before {
        return false;
    }
    let span = places.span(elems[sink].line);
    let place = ast::Place {
        name: result.to_string(),
        span,
        path: Vec::new(),
    };
    match &mut elems[sink].kind {
        GKind::Coil { var, .. } => *var = place,
        GKind::OutVar { target, .. } => *target = place,
        _ => unreachable!("a sink writes"),
    }
    true
}

fn lang_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Ld => "LD",
        Lang::Fbd => "FBD",
        Lang::Sfc => "SFC",
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
        match written(body) {
            Written::St(text) => {
                if let Some(body) = statements(places, at, &text, diags) {
                    out.push(NamedAction {
                        name: action,
                        body: ActBody::St(body),
                    });
                }
            }
            Written::Graphic(lang, node) => {
                let of = BodyOf::Action(action.clone());
                if let Some(g) = graphic_action(node, lang, of, p, places, diags) {
                    out.push(NamedAction {
                        name: action,
                        body: ActBody::Graphic(g),
                    });
                }
            }
            Written::Other(lang) if lang.is_empty() => {
                diags.push(places.diag(at, format!("action {action} has an empty body")))
            }
            Written::Other(lang) => {
                diags.push(places.diag(at, action_refused(&format!("action {action}"), &lang)))
            }
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
        match written(body) {
            Written::St(text) => {
                if let Some(cond) = condition(places, at, "body", &text, diags) {
                    out.push(NamedTransition {
                        name: tname,
                        cond: CondBody::St(cond),
                    });
                }
            }
            Written::Graphic(lang, node) => {
                let of = BodyOf::Transition(tname.clone());
                if let Some(g) = graphic_condition(node, lang, of, &tname, p, places, diags) {
                    out.push(NamedTransition {
                        name: tname,
                        cond: CondBody::Graphic(g),
                    });
                }
            }
            Written::Other(lang) if lang.is_empty() => {
                diags.push(places.diag(at, format!("transition {tname} has an empty body")))
            }
            Written::Other(lang) => diags
                .push(places.diag(at, condition_refused(&format!("transition {tname}"), &lang))),
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

/// The elements of a chart's body, and those of the networks of LD or FBD drawn in it (which
/// give transitions their conditions, Table 55 features 2a–6a); what cannot be read is said.
fn elements(
    body: &Node,
    p: &GPou,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<El>, Vec<GElem>) {
    let mut out = Vec::new();
    let mut networks = Vec::new();
    for node in &body.children {
        if matches!(node.name.as_str(), "addData" | "documentation" | "comment") {
            continue;
        }
        if NETWORK_ELEMENTS.contains(&node.name.as_str()) {
            networks.extend(read_elem(node, Lang::Sfc, p, None, places, diags));
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
                let Some(cond) = transition_condition(node, id, line, p, places, diags) else {
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
                        "{other} {id}: this element is not read in a Sequential Function Chart"
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
    (out, networks)
}

fn transition_condition(
    node: &Node,
    id: u64,
    line: u32,
    p: &GPou,
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
        return match written(inline) {
            Written::St(text) => {
                condition(places, line, "condition", &text, diags).map(|e| Cond::Expr(e, negated))
            }
            Written::Graphic(lang, body) => {
                // The network writes the condition to the inline body's name, as one of the
                // POU's transitions writes it to the transition's (§6.7.3 g; TC6 gives the
                // inline body a name for it).
                let tname = inline.attr("name").unwrap_or("").trim();
                if tname.is_empty() {
                    diags.push(places.diag(
                        line,
                        format!("transition {id}: its condition in {} has no name; the network writes the condition to the name its inline element gives (IEC 61131-3 ed. 3 §6.7.3 g)", lang_name(lang)),
                    ));
                    return None;
                }
                if !name(places, line, "name", tname, diags) {
                    return None;
                }
                graphic_condition(body, lang, BodyOf::Inline(id), tname, p, places, diags)
                    .map(|g| Cond::Graphic(g, tname.to_string(), negated))
            }
            Written::Other(lang) if lang.is_empty() => {
                diags.push(places.diag(line, format!("transition {id} has an empty condition")));
                None
            }
            Written::Other(lang) => {
                diags
                    .push(places.diag(line, condition_refused(&format!("transition {id}"), &lang)));
                None
            }
        };
    }
    if let Some(cp) = c.child("connectionPointIn") {
        let pin = graph::pin(cp, None, false, line, places, diags);
        if pin.expr.is_some() {
            diags.push(places.diag(
                line,
                format!("transition {id}: its condition is an expression at a connection; write it inline in ST"),
            ));
            return None;
        }
        if pin.conns.is_empty() {
            diags.push(places.diag(line, format!("transition {id} has an empty condition")));
            return None;
        }
        return Some(Cond::Wired(pin, negated));
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
            "SD" => Qual::Sd,
            "SL" => Qual::Sl,
            "P0" => Qual::P0,
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
            (None, Some(body)) => match written(body) {
                Written::St(text) => match statements(places, line, &text, diags) {
                    Some(stmts) => Target::Inline(ActBody::St(stmts)),
                    None => continue,
                },
                Written::Graphic(lang, node) => {
                    match graphic_action(node, lang, BodyOf::Inline(id), p, places, diags) {
                        Some(g) => Target::Inline(ActBody::Graphic(g)),
                        None => continue,
                    }
                }
                Written::Other(lang) if lang.is_empty() => {
                    diags.push(places.diag(line, format!("action {id} has an empty body")));
                    continue;
                }
                Written::Other(lang) => {
                    diags.push(places.diag(line, action_refused(&format!("action {id}"), &lang)));
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
    /// A body in an action block, by the association's `localId`.
    Inline(u64, ActBody),
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
    /// The names an action's body may not write (Table 54 footnote a; §6.7.4.1; §6.7.4.6 rule
    /// a), each with why.
    owned: Vec<(String, String)>,
}

struct Trans {
    el: usize,
    pre: Vec<usize>,
    post: Vec<usize>,
    cond: TCond,
    before: Vec<usize>,
}

/// A transition's condition, resolved: an expression (negated already when the transition
/// is), or a network of LD or FBD writing its result, and whether the transition negates it.
enum TCond {
    St(Expr),
    Graphic(Graphic, bool),
}

impl Chart {
    #[allow(clippy::too_many_arguments)]
    fn of(
        elems: &[El],
        networks: Vec<GElem>,
        pou: &ast::Pou,
        actions: &[NamedAction],
        transitions: &[NamedTransition],
        at: u32,
        places: &Places,
        diags: &mut Vec<Diagnostic>,
    ) -> Option<Chart> {
        let before = diags.len();
        let span = places.span(at);
        // Ids: each element, each action of a block and each element of a network drawn in
        // the chart has its own.
        let mut index: BTreeMap<u64, usize> = BTreeMap::new();
        let mut ids: BTreeMap<u64, u32> = BTreeMap::new();
        let drawn = networks.iter().map(|e| vec![(e.id, e.line)]);
        for (k, own) in elems
            .iter()
            .map(|e| {
                let mut own = vec![(e.id, e.line)];
                if let Kind::Block { assocs } = &e.kind {
                    own.extend(assocs.iter().map(|a| (a.id, a.line)));
                }
                own
            })
            .chain(drawn)
            .enumerate()
        {
            if let Some(e) = elems.get(k) {
                index.insert(e.id, k);
            }
            for (id, line) in own {
                if ids.insert(id, line).is_some() {
                    diags.push(places.diag(
                        line,
                        format!("two elements have the localId {id}; each element has its own"),
                    ));
                }
            }
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
        // A condition in LD or FBD written in its transition names what its network writes
        // (§6.7.3 g): a transition's name, its own too.
        for e in elems {
            if let Kind::Transition {
                cond: Cond::Graphic(_, n, _),
                ..
            } = &e.kind
            {
                claim(
                    n.to_ascii_uppercase(),
                    format!("the condition of transition {}, {n},", e.id),
                    e.line,
                    diags,
                );
            }
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
        // A transition of the POU in LD or FBD, by the transition that names it: one only (a
        // transition name is a TRANSITION … FROM … TO … construct, Table 55 feature 9a).
        let mut named_by: BTreeMap<String, u64> = BTreeMap::new();
        let mut wired: Vec<(usize, Pin, bool)> = Vec::new();
        for &k in &trans_els {
            let pre = resolve(index[&elems[k].ins[0]], true);
            let id = elems[k].id;
            let cond = match &elems[k].kind {
                Kind::Transition { cond, .. } => match cond {
                    Cond::Expr(e, negated) => TCond::St(negate(e.clone(), *negated)),
                    Cond::Graphic(g, _, negated) => TCond::Graphic(g.clone(), *negated),
                    Cond::Wired(pin, negated) => {
                        wired.push((trans.len(), pin.clone(), *negated));
                        TCond::St(bool_expr(false, span))
                    }
                    Cond::Named(n, negated) => {
                        match transitions.iter().find(|t| t.name.eq_ignore_ascii_case(n)) {
                            Some(t) => match &t.cond {
                                CondBody::St(e) => TCond::St(negate(e.clone(), *negated)),
                                CondBody::Graphic(g) => {
                                    if let Some(first) =
                                        named_by.insert(t.name.to_ascii_uppercase(), id)
                                    {
                                        diags.push(places.diag(
                                            elems[k].line,
                                            format!("transitions {first} and {id} both name the transition {}, whose condition is a network of {}; a transition's name names one transition (IEC 61131-3 ed. 3 Table 55 features 9a–11: TRANSITION … FROM … TO …), and its network's values are kept once", t.name, lang_name(g.lang)),
                                        ));
                                    }
                                    TCond::Graphic(g.clone(), *negated)
                                }
                            },
                            None => {
                                diags.push(places.diag(
                                    elems[k].line,
                                    format!("transition {id} names the transition {n}, which the POU's transitions do not have"),
                                ));
                                TCond::St(bool_expr(false, span))
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
        // The networks drawn in the chart, each giving the condition of the transition it
        // reaches.
        let found = drawn_networks(networks, &wired, &trans, elems, pou, places, diags);
        for ((t, _, negated), g) in wired.iter().zip(found) {
            if let Some(g) = g {
                trans[*t].cond = TCond::Graphic(g, *negated);
            }
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
                    Target::Inline(body) => {
                        (format!("#{}", a.id), ActKind::Inline(a.id, body.clone()))
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
        // Those in ST here; those in LD or FBD once lowered, as statements.
        for a in &acts {
            let body = match &a.kind {
                ActKind::Named(k, _) => &actions[*k].body,
                ActKind::Inline(_, b) => b,
                ActKind::Var(_) => continue,
            };
            if let ActBody::St(body) = body {
                writes(body, &owned, diags);
            }
        }
        for a in actions {
            // An action no step uses is not run; what it writes is still checked.
            if let ActBody::St(body) = &a.body {
                if !acts
                    .iter()
                    .any(|x| matches!(&x.kind, ActKind::Named(_, n) if *n == a.name))
                {
                    writes(body, &owned, diags);
                }
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
            owned,
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

    /// The statements of a scan, the variables they keep, and the probes for monitoring; each
    /// action and condition in LD or FBD lowered by its language's front-end in its own
    /// namespace (ADR-013 §2.17). `None` when one could not be, and the diagnostics say why.
    fn lower(
        &self,
        tree: &ast::Pou,
        named: &[NamedAction],
        known: &Known,
        places: &mut Places,
        diags: &mut Vec<Diagnostic>,
    ) -> Option<(Vec<Stmt>, Vec<VarDecl>, Vec<Probe>)> {
        let pou = tree.name.as_str();
        let span = self.span;
        let before = diags.len();
        // The flags of a body's networks are set FALSE before it may run, so a network of a
        // body that did not run in the scan does not read as run.
        // So are the BOOLs that say where a function of an FBD body ran (its kept ENO, the
        // `ran` of a guarded probe): a value of a body that did not run is never shown as
        // computed in this scan. Within a run each is assigned before it is read, so setting
        // it FALSE first changes nothing the body does.
        let clear = |l: &Lowered| -> Vec<Stmt> {
            let mut rans: Vec<&str> = l
                .probes
                .iter()
                .filter_map(|p| match &p.holds {
                    Holds::Guarded { ran, .. } => Some(ran.as_str()),
                    _ => None,
                })
                .collect();
            rans.sort_unstable();
            rans.dedup();
            l.flags
                .iter()
                .cloned()
                .chain(rans.into_iter().map(|r| place(r, span)))
                .map(|f| Stmt::Assign(f, bool_expr(false, span)))
                .collect()
        };
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
                body: None,
            });
            probes.push(Probe {
                pou: pou.into(),
                local_id: id,
                at: At::StepTime,
                holds: Holds::Variable {
                    path: format!("{name}.T"),
                    negated: false,
                },
                body: None,
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
                    body: None,
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
            let timer_pt = if a.assocs.iter().any(|x| x.1.timed_while_active()) {
                let t = hidden.var(format!("__SFC_{key}_T"), time_ty(), span);
                control.push(Stmt::Assign(t.clone(), time0(span)));
                for x in a.assocs.iter().filter(|x| x.1.timed_while_active()) {
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
            for (qual, limited) in [(Qual::Sd, false), (Qual::Sl, true)] {
                if !has(qual) {
                    continue;
                }
                // Each SD or SL association is stored and timed by itself (ADR-013 §2.17): a
                // flip-flop set by its step and reset by R, and a timer of its own duration,
                // kept once its step is left. Where two associations or a step entered again
                // leave two readings, the one with the action off is taken: an SD action
                // starts when no association that stored it is still in its delay, each
                // counted from the last time its step was entered; an SL action ends when the
                // first limit is over, each counted from when its step first stored it. Once
                // started (SD) or ended (SL) it stays so until R.
                let name = qual.name();
                let mut stored: Vec<Expr> = Vec::new();
                let mut timed_out: Vec<Expr> = Vec::new();
                let mut waiting: Vec<Expr> = Vec::new();
                for x in a.assocs.iter().filter(|x| x.1 == qual) {
                    let id = x.3;
                    let step = || var(member(&self.steps[x.0].0, "X", span));
                    let t = hidden.var(format!("__SFC_{key}_{name}_{id}_KEPT"), time_ty(), span);
                    let ff =
                        hidden.var(format!("__SFC_{key}_{name}_{id}_FF"), named_ty("RS"), span);
                    let tmr = hidden.var(
                        format!("__SFC_{key}_{name}_{id}_TMR"),
                        named_ty("TON"),
                        span,
                    );
                    // The duration is read each scan the step is active, and kept after.
                    control.push(Stmt::If(
                        vec![(
                            step(),
                            vec![Stmt::Assign(
                                t.clone(),
                                x.2.clone().unwrap_or_else(|| time0(span)),
                            )],
                        )],
                        None,
                    ));
                    if !limited {
                        // A step entered again while its delay runs starts the delay again.
                        let was =
                            hidden.var(format!("__SFC_{key}_{name}_{id}_WAS"), bool_ty(span), span);
                        control.push(Stmt::If(
                            vec![(
                                and(
                                    and(step(), not(var(was.clone()), span), span),
                                    var(sel(&ff, "Q1", span)),
                                    span,
                                ),
                                vec![call(
                                    &tmr,
                                    vec![("IN", bool_expr(false, span)), ("PT", var(t.clone()))],
                                )],
                            )],
                            None,
                        ));
                        control.push(Stmt::Assign(was.clone(), step()));
                        reset.push(Stmt::Assign(was, bool_expr(false, span)));
                    }
                    control.push(call(&ff, vec![("S", step()), ("R1", r.clone())]));
                    control.push(call(
                        &tmr,
                        vec![("IN", var(sel(&ff, "Q1", span))), ("PT", var(t))],
                    ));
                    reset.push(call(
                        &ff,
                        vec![("S", bool_expr(false, span)), ("R1", bool_expr(true, span))],
                    ));
                    reset.push(call(
                        &tmr,
                        vec![("IN", bool_expr(false, span)), ("PT", time0(span))],
                    ));
                    stored.push(var(sel(&ff, "Q1", span)));
                    timed_out.push(var(sel(&tmr, "Q", span)));
                    waiting.push(and(
                        var(sel(&ff, "Q1", span)),
                        not(var(sel(&tmr, "Q", span)), span),
                        span,
                    ));
                }
                let stored = or_all(stored, span);
                let latch = part(
                    &mut hidden,
                    &format!("{name}_{}", if limited { "END" } else { "START" }),
                    "RS",
                );
                let set = if limited {
                    or_all(timed_out, span)
                } else {
                    and(stored.clone(), not(or_all(waiting, span), span), span)
                };
                control.push(call(&latch, vec![("S", set), ("R1", r.clone())]));
                reset.push(call(
                    &latch,
                    vec![("S", bool_expr(false, span)), ("R1", bool_expr(true, span))],
                ));
                terms.push(if limited {
                    and(stored, not(var(sel(&latch, "Q1", span)), span), span)
                } else {
                    var(sel(&latch, "Q1", span))
                });
            }
            if has(Qual::P0) {
                // The falling edge of P0, with what it remembers FALSE at a start: no step
                // was active before it, so none was left (Figure 23's F_TRIG is TRUE at its
                // first execution, Table 44; ADR-013 §2.17).
                let was = hidden.var(format!("__SFC_{key}_P0_WAS"), bool_ty(span), span);
                let pulse = hidden.var(format!("__SFC_{key}_P0_PULSE"), bool_ty(span), span);
                let p0 = input(Qual::P0).unwrap();
                control.push(Stmt::Assign(
                    pulse.clone(),
                    and(not(p0.clone(), span), var(was.clone()), span),
                ));
                control.push(Stmt::Assign(was.clone(), p0));
                reset.push(Stmt::Assign(was, bool_expr(false, span)));
                reset.push(Stmt::Assign(pulse.clone(), bool_expr(false, span)));
                terms.push(var(pulse));
            }
            let mut qx = or_all(terms, span);
            if let Some(r) = input(Qual::R) {
                qx = and(not(r, span), qx, span);
            }
            control.push(Stmt::Assign(q.clone(), qx));
            // The action's body runs while Q is TRUE (§6.7.4.6 rule a, without final scan).
            let body = match &a.kind {
                ActKind::Var(_) => None,
                ActKind::Named(k, _) => Some(&named[*k].body),
                ActKind::Inline(_, body) => Some(body),
            };
            match body {
                None => {}
                Some(ActBody::St(stmts)) => {
                    control.push(Stmt::If(vec![(var(q.clone()), stmts.clone())], None))
                }
                Some(ActBody::Graphic(g)) => {
                    if let Some(l) = g.lower(tree, known, places, diags) {
                        writes(&l.stmts, &self.owned, diags);
                        control.extend(clear(&l));
                        control.push(Stmt::If(vec![(var(q.clone()), l.stmts)], None));
                        vars.extend(l.vars);
                        probes.extend(l.probes);
                    }
                }
            }
        }
        // An action in LD or FBD no step uses is not run; it is lowered, so what it writes is
        // checked as an action's in ST is.
        for a in named {
            if let ActBody::Graphic(g) = &a.body {
                let used = self
                    .actions
                    .iter()
                    .any(|x| matches!(&x.kind, ActKind::Named(_, n) if *n == a.name));
                if !used {
                    if let Some(l) = g.lower(tree, known, places, diags) {
                        writes(&l.stmts, &self.owned, diags);
                    }
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
            match &tr.cond {
                TCond::St(cond) => out.push(Stmt::If(
                    vec![(var(en.clone()), vec![Stmt::Assign(c.clone(), cond.clone())])],
                    None,
                )),
                // The network runs where the transition is enabled, and only there; it writes
                // its result, which the transition takes (negated when it is).
                TCond::Graphic(g, negated) => {
                    let lowered = g.lower(tree, known, places, diags);
                    if let Some(l) = lowered.filter(|l| computed_each_time(g, l, places, diags)) {
                        let result = hidden.var(g.result(), bool_ty(span), span);
                        out.extend(clear(&l));
                        let mut body = l.stmts;
                        body.push(Stmt::Assign(c.clone(), negate(var(result), *negated)));
                        out.push(Stmt::If(vec![(var(en.clone()), body)], None));
                        vars.extend(l.vars);
                        probes.extend(drawn_probes(l.probes, &g.of));
                    }
                }
            }
            probes.push(Probe {
                pou: pou.into(),
                local_id: id,
                at: At::Transition,
                holds: Holds::Guarded {
                    path: c.text(),
                    ran: en.text(),
                },
                body: None,
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
        (diags.len() == before).then_some((out, vars, probes))
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

/// The networks of LD or FBD drawn in a chart's body (Table 55 features 2a–6a; TC6 lets a
/// chart's body hold LD and FBD elements, and a transition's `condition` be a
/// `connectionPointIn`): for each transition in `wired` (its index in `trans`, the connections
/// that reach its condition, whether negated), the network that reaches it, checked as a
/// condition's (`condition_network`), or `None` where it is refused.
///
/// A connector and its continuations are one line (§8.1.3, the FBD front-end's rule), so a
/// network may reach a transition through a continuation (features 4a–6a). Each element of a
/// network leads to the condition of one transition: one that leads to none, or to two, is
/// refused. A network with a power rail, a contact or a coil is a Ladder Diagram (§8.2), any
/// other a Function Block Diagram (§8.3). Where it reaches the transition the chart adds a
/// coil (LD) or an output variable (FBD) of the transition's `localId`, which takes the
/// condition.
fn drawn_networks(
    mut networks: Vec<GElem>,
    wired: &[(usize, Pin, bool)],
    trans: &[Trans],
    elems: &[El],
    pou: &ast::Pou,
    places: &Places,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Option<Graphic>> {
    let none = || wired.iter().map(|_| None).collect();
    if networks.is_empty() && wired.is_empty() {
        return Vec::new();
    }
    let before = diags.len();
    // Every connection of a network reaches an element of the chart's networks; one to a step
    // is told where the network is walked, below.
    let own: BTreeSet<u64> = networks.iter().map(|e| e.id).collect();
    for e in &networks {
        for p in &e.inputs {
            for c in &p.conns {
                if !own.contains(&c.from) && !elems.iter().any(|x| x.id == c.from) {
                    diags.push(places.diag(
                        p.line,
                        format!(
                            "{} {} is connected to localId {}, which no element of the body has",
                            e.what(),
                            e.id,
                            c.from
                        ),
                    ));
                }
            }
        }
    }
    if diags.len() != before {
        return none();
    }
    // Lines joined: a connection from a continuation is one from what reaches its connector.
    let continued = fbd::join_lines(&mut networks, pou, places, diags);
    if diags.len() != before {
        return none();
    }
    let lines: BTreeMap<u64, graph::Conn> = continued
        .iter()
        .map(|(k, c)| (networks[*k].id, c.clone()))
        .collect();
    networks.retain(|e| !matches!(e.kind, GKind::Connector(_) | GKind::Continuation(_)));
    let index: BTreeMap<u64, usize> = networks
        .iter()
        .enumerate()
        .map(|(k, e)| (e.id, k))
        .collect();
    // Each transition's network: what reaches its condition, element by element.
    let mut owner: Vec<Option<u64>> = vec![None; networks.len()];
    let mut members: Vec<Vec<usize>> = Vec::new();
    for (t, pin, _) in wired {
        let tid = elems[trans[*t].el].id;
        let mut set = Vec::new();
        let mut todo: Vec<u64> = Vec::new();
        for c in &pin.conns {
            let c = lines.get(&c.from).unwrap_or(c);
            match index.get(&c.from) {
                Some(_) => todo.push(c.from),
                None => diags.push(places.diag(
                    pin.line,
                    format!(
                        "the condition of transition {tid} is connected to localId {}, which no element of a network in the chart has",
                        c.from
                    ),
                )),
            }
        }
        let mut seen = BTreeSet::new();
        while let Some(id) = todo.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(&k) = index.get(&id) else {
                continue;
            };
            let e = &networks[k];
            // A power rail belongs to no network (§8.1.4): each network that reaches it has
            // its own.
            if !matches!(e.kind, GKind::LeftRail) {
                match owner[k] {
                    Some(other) if other != tid => diags.push(places.diag(
                        e.line,
                        format!(
                            "{} {} leads to the conditions of transitions {other} and {tid}; a network gives one transition its condition (IEC 61131-3 ed. 3 §6.7.3)",
                            e.what(),
                            e.id
                        ),
                    )),
                    _ => owner[k] = Some(tid),
                }
            }
            set.push(k);
            for p in &e.inputs {
                for c in &p.conns {
                    let from = lines.get(&c.from).unwrap_or(c).from;
                    if index.contains_key(&from) {
                        todo.push(from);
                    } else if let Some(x) = elems.iter().find(|x| x.id == from) {
                        diags.push(places.diag(
                            p.line,
                            format!(
                                "{} {} is connected to {} {from}; a network reads a step's flag by its name, S.X (IEC 61131-3 ed. 3 Table 54 feature 3a): a connection from the step (feature 3b) is not read",
                                e.what(),
                                e.id,
                                x.kind.what()
                            ),
                        ));
                    }
                }
            }
        }
        members.push(set);
    }
    // A left power rail is in every network that starts at it; one that nothing starts at is
    // drawn for nothing, and is told as any element that leads nowhere.
    let started: BTreeSet<u64> = networks
        .iter()
        .flat_map(|e| e.inputs.iter().flat_map(|p| p.conns.iter().map(|c| c.from)))
        .collect();
    for (k, e) in networks.iter().enumerate() {
        let unused = match e.kind {
            GKind::LeftRail => !started.contains(&e.id),
            // What reaches a right rail is the end of a rung (§8.2.3): it gives no condition.
            GKind::RightRail => true,
            _ => owner[k].is_none(),
        };
        if unused {
            diags.push(places.diag(
                e.line,
                format!(
                    "{} {} is drawn in the chart and leads to no transition's condition; a network in a chart gives a transition its condition (IEC 61131-3 ed. 3 §6.7.3, Table 55 features 2a–6a)",
                    e.what(),
                    e.id
                ),
            ));
        }
    }
    if diags.len() != before {
        return none();
    }
    let mut out = Vec::new();
    for ((t, pin, _), set) in wired.iter().zip(members) {
        let tr = &elems[trans[*t].el];
        let mut own: Vec<GElem> = set.iter().map(|k| networks[*k].clone()).collect();
        // Connections from continuations are made from what the line starts at.
        for e in &mut own {
            for p in &mut e.inputs {
                for c in &mut p.conns {
                    if let Some(line) = lines.get(&c.from) {
                        *c = line.clone();
                    }
                }
            }
        }
        let ladder = own.iter().any(|e| {
            matches!(
                e.kind,
                GKind::LeftRail | GKind::RightRail | GKind::Contact { .. } | GKind::Coil { .. }
            )
        });
        let lang = if ladder { Lang::Ld } else { Lang::Fbd };
        let mut input = pin.clone();
        for c in &mut input.conns {
            if let Some(line) = lines.get(&c.from) {
                *c = line.clone();
            }
        }
        let span = places.span(tr.line);
        let target = ast::Place {
            name: String::new(),
            span,
            path: Vec::new(),
        };
        own.push(GElem {
            id: tr.id,
            pos: tr.pos,
            line: tr.line,
            kind: match lang {
                Lang::Ld => GKind::Coil {
                    var: target,
                    negated: false,
                    storage: Storage::None,
                    edge: graph::Edge::None,
                },
                _ => GKind::OutVar {
                    target,
                    negated: false,
                },
            },
            inputs: vec![input],
        });
        let at = diags.len();
        let mut g = Graphic {
            lang,
            elems: own,
            of: BodyOf::Wired(tr.id),
        };
        let result = g.result();
        if check_graph(&g.elems, lang, places, diags)
            && condition_network(
                &mut g.elems,
                lang,
                Sink::Element(tr.id),
                &result,
                places,
                diags,
            )
            && diags.len() == at
        {
            out.push(Some(g));
        } else {
            out.push(None);
        }
    }
    out
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

/// Whether a condition's network assigns its result in every evaluation: at the top of its
/// statements, not only under a function's EN. Refused where not (said at the element that
/// takes the result): where the EN is FALSE the function does not run and the value is not
/// computed (IEC 61131-3 ed. 3 §6.6.1.5 rule 4; ADR-013 §2.16), and a condition is the result of
/// evaluating one Boolean expression each time its transition is enabled (§6.7.3). Taking an
/// earlier evaluation's value would clear the transition on a value of another scan, and
/// FALSE in its place would clear a negated transition on a value nobody computed. A coil's
/// power is defined where a function did not run (OFF, §2.15), so a coil always assigns it.
fn computed_each_time(
    g: &Graphic,
    l: &Lowered,
    places: &Places,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let result = g.result();
    let assigned = l
        .stmts
        .iter()
        .any(|s| matches!(s, Stmt::Assign(p, _) if p.path.is_empty() && p.name == result));
    if assigned {
        return true;
    }
    let sink = g.elems.iter().find(|e| match &e.kind {
        GKind::Coil { var, .. } => var.name == result,
        GKind::OutVar { target, .. } => target.name == result,
        _ => false,
    });
    let message = |what: &str| {
        format!(
            "{what} takes a value computed only where a function's EN is TRUE; where it is FALSE the function does not run (IEC 61131-3 ed. 3 §6.6.1.5) and the condition would not be computed, while a condition is the result of evaluating one Boolean expression each time its transition is enabled (§6.7.3): give the function no EN, or in LD end the rung at a coil of the transition's name (its power is OFF where the function did not run)"
        )
    };
    match sink {
        Some(e) => diags.push(places.diag(e.line, message(&format!("{} {}", e.what(), e.id)))),
        None => diags.push(Diagnostic::whole(message("the condition"))),
    }
    false
}

/// The probes of a condition's network: of one drawn in the chart, without the element the
/// chart added where it reaches the transition (whose condition is the transition's own probe).
fn drawn_probes(probes: Vec<Probe>, of: &BodyOf) -> Vec<Probe> {
    let BodyOf::Wired(tid) = of else {
        return probes;
    };
    probes
        .into_iter()
        .filter_map(|mut p| {
            if let At::Network(members) = &mut p.at {
                members.retain(|m| m != tid);
                if p.local_id == *tid {
                    p.local_id = *members.first()?;
                }
                return Some(p);
            }
            (p.local_id != *tid).then_some(p)
        })
        .collect()
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
