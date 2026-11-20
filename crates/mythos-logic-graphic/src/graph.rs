//! The graph of a graphical body, as both languages read it (IEC 61131-3 ed. 3 §8.1): each
//! element with its `localId`, its position and its inputs, and the checks that need the whole
//! graph. A Ladder Diagram (`ld`) and a Function Block Diagram (`fbd`) share every element
//! but their own: the rails, contacts and coils are LD's; connectors and continuations are
//! read in FBD. What a language does not take is refused by name, with the element.
//!
//! The network order, the labels, the jumps and the returns are the same rule in both
//! languages (§8.1.4, §8.1.6, §8.2.7; for FBD, ADR-013 §2.13 on §8.3.3), and are here too:
//! [`body`] arranges the statements each language makes for a network.

use crate::read::{flag, name, GPou};
use crate::xml::Node;
use crate::{expression, At, Holds, Place, Places};
use mythos_logic_ir::{Diagnostic, Retention, Section, Span};
use mythos_logic_st::ast::{self, BinOp, Expr, ExprKind, Stmt, TypeSpec, TypeSpecKind, VarDecl};
use std::collections::BTreeMap;

/// The language of a graphical body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    /// Ladder Diagram (§8.2).
    Ld,
    /// Function Block Diagram (§8.3).
    Fbd,
    /// Sequential Function Chart (§6.7), lowered by `sfc`.
    Sfc,
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Lang::Ld => "a Ladder Diagram",
            Lang::Fbd => "a Function Block Diagram",
            Lang::Sfc => "a Sequential Function Chart",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    None,
    Rising,
    Falling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Storage {
    None,
    Set,
    Reset,
}

#[derive(Debug, Clone)]
pub(crate) enum Kind {
    LeftRail,
    RightRail,
    Contact {
        var: Expr,
        negated: bool,
        edge: Edge,
    },
    Coil {
        var: ast::Place,
        negated: bool,
        storage: Storage,
        edge: Edge,
    },
    Block {
        type_name: String,
        instance: Option<ast::Place>,
        /// Its outputs by formal parameter in upper case, each with whether it is negated.
        outputs: Vec<(String, bool)>,
    },
    InVar {
        expr: Expr,
        negated: bool,
    },
    OutVar {
        target: ast::Place,
        negated: bool,
    },
    InOutVar {
        target: ast::Place,
        negated_in: bool,
        negated_out: bool,
    },
    Label(String),
    Jump(String),
    Return,
    Comment,
    /// The end of a line continued elsewhere (§8.1.3): it takes the line, by name in upper
    /// case. Read in FBD.
    Connector(String),
    /// Where a line continues: it gives what the connector of its name takes.
    Continuation(String),
}

/// An input of an element: what reaches it.
#[derive(Debug, Clone)]
pub(crate) struct Pin {
    /// The formal parameter it sets, in upper case (a block's); `None` for the one input of
    /// any other element.
    pub formal: Option<String>,
    pub conns: Vec<Conn>,
    /// An expression written at the pin in place of a connection.
    pub expr: Option<Expr>,
    pub negated: bool,
    /// The place of the pin, for what is said about it.
    pub line: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct Conn {
    pub from: u64,
    pub formal: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Elem {
    pub id: u64,
    pub pos: (f64, f64),
    pub line: u32,
    pub kind: Kind,
    pub inputs: Vec<Pin>,
}

impl Elem {
    pub fn what(&self) -> &'static str {
        match self.kind {
            Kind::LeftRail => "the left power rail",
            Kind::RightRail => "the right power rail",
            Kind::Contact { .. } => "a contact",
            Kind::Coil { .. } => "a coil",
            Kind::Block { .. } => "a block",
            Kind::InVar { .. } => "an input variable",
            Kind::OutVar { .. } => "an output variable",
            Kind::InOutVar { .. } => "an in-out variable",
            Kind::Label(_) => "a label",
            Kind::Jump(_) => "a jump",
            Kind::Return => "a return",
            Kind::Comment => "a comment",
            Kind::Connector(_) => "a connector",
            Kind::Continuation(_) => "a continuation",
        }
    }

    /// The output a connection with `formal` takes from this element, in upper case; `None`
    /// when it gives no such output.
    pub fn output(&self, formal: Option<&str>) -> Option<String> {
        match &self.kind {
            Kind::LeftRail
            | Kind::Contact { .. }
            | Kind::Coil { .. }
            | Kind::InVar { .. }
            | Kind::InOutVar { .. }
            | Kind::Continuation(_) => Some(String::new()),
            Kind::Block { outputs, .. } => match formal {
                Some(f) => {
                    let f = f.to_ascii_uppercase();
                    outputs.iter().any(|(o, _)| *o == f).then_some(f)
                }
                // The schema: the first output that is not ENO.
                None => outputs
                    .iter()
                    .find(|(o, _)| o != "ENO")
                    .map(|(o, _)| o.clone()),
            },
            _ => None,
        }
    }
}

/// The elements of the body `body` of `p`, in language `lang`; what cannot be read is said.
pub(crate) fn read_body(
    body: &Node,
    lang: Lang,
    p: &GPou,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Elem> {
    let mut out = Vec::new();
    for node in &body.children {
        if matches!(node.name.as_str(), "addData" | "documentation") {
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
            text_line: None,
            tag: node.span,
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
        let single_input = |places: &mut Places, diags: &mut Vec<Diagnostic>| match node
            .child("connectionPointIn")
        {
            Some(cp) => vec![pin(cp, None, false, line, places, diags)],
            None => vec![Pin {
                formal: None,
                conns: Vec::new(),
                expr: None,
                negated: false,
                line,
            }],
        };
        let text =
            |places: &mut Places, diags: &mut Vec<Diagnostic>, field: &str| -> Option<Expr> {
                match node.child(field) {
                    Some(t) => expression(places, line, field, t.text.trim(), diags),
                    None => {
                        diags.push(places.diag(line, format!("{} {id} has no {field}", node.name)));
                        None
                    }
                }
            };
        // A coil or an output variable writes its text: it must name a variable.
        let place_of = |e: Expr, field: &str, diags: &mut Vec<Diagnostic>| match e.kind {
            ExprKind::Var(p) => Some(p),
            _ => {
                diags.push(Diagnostic::at(
                    e.span,
                    format!(
                        "the {field} of {} {id} names a variable to write; this is not one",
                        node.name
                    ),
                ));
                None
            }
        };
        let edge_of =
            |attr: &str, places: &Places, diags: &mut Vec<Diagnostic>| match node.attr(attr) {
                None | Some("none") => Edge::None,
                Some("rising") => Edge::Rising,
                Some("falling") => Edge::Falling,
                Some(other) => {
                    diags.push(places.diag(
                        line,
                        format!("{attr}=\"{other}\" is none, rising or falling"),
                    ));
                    Edge::None
                }
            };
        let storage_of = |attr: &str, places: &Places, diags: &mut Vec<Diagnostic>| match node
            .attr(attr)
        {
            None | Some("none") => Storage::None,
            Some("set") => Storage::Set,
            Some("reset") => Storage::Reset,
            Some(other) => {
                diags.push(places.diag(line, format!("{attr}=\"{other}\" is none, set or reset")));
                Storage::None
            }
        };
        if lang == Lang::Fbd
            && matches!(
                node.name.as_str(),
                "leftPowerRail" | "rightPowerRail" | "contact" | "coil"
            )
        {
            diags.push(places.diag(
                line,
                format!(
                    "{} {id} is an element of a Ladder Diagram; a Function Block Diagram has no power rails, contacts or coils (IEC 61131-3 ed. 3 §8.3): use a variable and the blocks AND, OR, NOT, SR, RS",
                    node.name
                ),
            ));
            continue;
        }
        let (kind, inputs) = match node.name.as_str() {
            "leftPowerRail" => (Kind::LeftRail, Vec::new()),
            "rightPowerRail" => {
                let pins = node
                    .children_named("connectionPointIn")
                    .map(|cp| pin(cp, None, false, line, places, diags))
                    .collect();
                (Kind::RightRail, pins)
            }
            "contact" => {
                let negated = flag(node, "negated", places, line, diags);
                let edge = edge_of("edge", places, diags);
                if storage_of("storage", places, diags) != Storage::None {
                    diags.push(places.diag(
                        line,
                        format!("contact {id} has a storage; only a coil sets or resets"),
                    ));
                }
                if negated && edge != Edge::None {
                    diags.push(places.diag(
                        line,
                        format!("contact {id} is negated and senses an edge; IEC 61131-3 Table 75 has no such contact"),
                    ));
                }
                let var = text(places, diags, "variable");
                let inputs = single_input(places, diags);
                match var {
                    Some(var) => (Kind::Contact { var, negated, edge }, inputs),
                    None => continue,
                }
            }
            "coil" => {
                let negated = flag(node, "negated", places, line, diags);
                let edge = edge_of("edge", places, diags);
                let storage = storage_of("storage", places, diags);
                let kinds =
                    negated as u8 + (edge != Edge::None) as u8 + (storage != Storage::None) as u8;
                if kinds > 1 {
                    diags.push(places.diag(
                        line,
                        format!("coil {id} combines negation, an edge and a storage; IEC 61131-3 Table 76 has a coil of each, not of two"),
                    ));
                }
                let var =
                    text(places, diags, "variable").and_then(|e| place_of(e, "variable", diags));
                let inputs = single_input(places, diags);
                match var {
                    Some(var) => (
                        Kind::Coil {
                            var,
                            negated,
                            storage,
                            edge,
                        },
                        inputs,
                    ),
                    None => continue,
                }
            }
            "inVariable" => {
                let negated = flag(node, "negated", places, line, diags);
                no_modifier(node, "edge", "storage", id, places, line, diags);
                match text(places, diags, "expression") {
                    Some(expr) => (Kind::InVar { expr, negated }, Vec::new()),
                    None => continue,
                }
            }
            "outVariable" => {
                let negated = flag(node, "negated", places, line, diags);
                no_modifier(node, "edge", "storage", id, places, line, diags);
                let target = text(places, diags, "expression")
                    .and_then(|e| place_of(e, "expression", diags));
                let inputs = single_input(places, diags);
                match target {
                    Some(target) => (Kind::OutVar { target, negated }, inputs),
                    None => continue,
                }
            }
            "inOutVariable" => {
                let negated_in = flag(node, "negatedIn", places, line, diags);
                let negated_out = flag(node, "negatedOut", places, line, diags);
                no_modifier(node, "edgeIn", "storageIn", id, places, line, diags);
                no_modifier(node, "edgeOut", "storageOut", id, places, line, diags);
                let target = text(places, diags, "expression")
                    .and_then(|e| place_of(e, "expression", diags));
                let inputs = single_input(places, diags);
                match target {
                    Some(target) => (
                        Kind::InOutVar {
                            target,
                            negated_in,
                            negated_out,
                        },
                        inputs,
                    ),
                    None => continue,
                }
            }
            "block" => match block(node, id, line, places, diags) {
                Some(b) => b,
                None => continue,
            },
            "label" | "jump" => {
                let label = node.attr("label").unwrap_or("");
                if !label_name(places, line, label, diags) {
                    continue;
                }
                if node.name == "label" {
                    (Kind::Label(label.to_ascii_uppercase()), Vec::new())
                } else {
                    (
                        Kind::Jump(label.to_ascii_uppercase()),
                        single_input(places, diags),
                    )
                }
            }
            "return" => (Kind::Return, single_input(places, diags)),
            "comment" => (Kind::Comment, Vec::new()),
            // A connector and its continuations are one line (§8.1.3): read in FBD, where the
            // FBD front-end joins them before anything else.
            "connector" | "continuation" if lang == Lang::Fbd => {
                let label = node.attr("name").unwrap_or("");
                if !name(places, line, "name", label, diags) {
                    continue;
                }
                let label = label.to_ascii_uppercase();
                if node.name == "connector" {
                    (Kind::Connector(label), single_input(places, diags))
                } else {
                    (Kind::Continuation(label), Vec::new())
                }
            }
            other => {
                diags.push(places.diag(
                    line,
                    format!(
                        "{other} {id}: this element is not read in {} yet",
                        lang.name()
                    ),
                ));
                continue;
            }
        };
        if diags.len() == before {
            out.push(Elem {
                id,
                pos,
                line,
                kind,
                inputs,
            });
        }
    }
    out
}

pub(crate) fn no_modifier(
    node: &Node,
    edge: &str,
    storage: &str,
    id: u64,
    places: &Places,
    line: u32,
    diags: &mut Vec<Diagnostic>,
) {
    for attr in [edge, storage] {
        if node.attr(attr).is_some_and(|v| v != "none") {
            diags.push(places.diag(
                line,
                format!(
                    "{} {id}: {attr} on a variable is not supported; use a contact or a coil",
                    node.name
                ),
            ));
        }
    }
}

/// A network label: an identifier or an unsigned decimal integer (§8.1.4).
pub(crate) fn label_name(
    places: &mut Places,
    at: u32,
    label: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if !label.is_empty() && label.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    name(places, at, "label", label, diags)
}

/// A `connectionPointIn`: its connections, or the expression written in their place.
pub(crate) fn pin(
    cp: &Node,
    formal: Option<String>,
    negated: bool,
    line: u32,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Pin {
    let mut conns = Vec::new();
    for c in cp.children_named("connection") {
        match c
            .attr("refLocalId")
            .and_then(|t| t.trim().parse::<u64>().ok())
        {
            Some(from) => conns.push(Conn {
                from,
                formal: c.attr("formalParameter").map(String::from),
            }),
            None => diags.push(places.diag(
                line,
                format!(
                    "a connection has no refLocalId that is a whole number ({})",
                    c.attr("refLocalId").unwrap_or("none")
                ),
            )),
        }
    }
    let field = formal.clone().unwrap_or_else(|| "expression".into());
    let expr = cp
        .child("expression")
        .and_then(|e| expression(places, line, &field, e.text.trim(), diags));
    Pin {
        formal,
        conns,
        expr,
        negated,
        line,
    }
}

pub(crate) type BlockParts = (Kind, Vec<Pin>);

pub(crate) fn block(
    node: &Node,
    id: u64,
    line: u32,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
) -> Option<BlockParts> {
    let type_name = node.attr("typeName").unwrap_or("").to_string();
    // The Boolean operators and MOD are keywords of ST and blocks of LD and FBD (Table 29,
    // Table 31 give them a graphical form).
    let operator = matches!(
        type_name.to_ascii_uppercase().as_str(),
        "AND" | "OR" | "XOR" | "NOT" | "MOD"
    );
    if !operator && !name(places, line, "typeName", &type_name, diags) {
        return None;
    }
    let instance = match node.attr("instanceName") {
        Some(text) if !text.trim().is_empty() => {
            match expression(places, line, "instanceName", text.trim(), diags)? {
                Expr {
                    kind: ExprKind::Var(p),
                    ..
                } => Some(p),
                e => {
                    diags.push(Diagnostic::at(
                        e.span,
                        format!(
                            "the instanceName of block {id} names an instance; this is not one"
                        ),
                    ));
                    return None;
                }
            }
        }
        _ => None,
    };
    let modifiers = |v: &Node, places: &Places, diags: &mut Vec<Diagnostic>| {
        for attr in ["edge", "storage"] {
            if v.attr(attr).is_some_and(|x| x != "none") {
                diags.push(places.diag(
                    line,
                    format!(
                        "block {id}: {attr} on its parameter {} is not supported yet",
                        v.attr("formalParameter").unwrap_or("")
                    ),
                ));
            }
        }
    };
    let mut inputs = Vec::new();
    for v in node
        .child("inputVariables")
        .into_iter()
        .flat_map(|n| n.children_named("variable"))
    {
        modifiers(v, places, diags);
        let formal = v.attr("formalParameter").unwrap_or("").to_ascii_uppercase();
        let negated = flag(v, "negated", places, line, diags);
        let pin_line = places.text(line, &formal, "");
        match v.child("connectionPointIn") {
            Some(cp) => inputs.push(pin(cp, Some(formal), negated, pin_line, places, diags)),
            None => inputs.push(Pin {
                formal: Some(formal),
                conns: Vec::new(),
                expr: None,
                negated,
                line: pin_line,
            }),
        }
    }
    if node
        .child("inOutVariables")
        .is_some_and(|n| n.children_named("variable").next().is_some())
    {
        diags.push(places.diag(
            line,
            format!("block {id}: in-out parameters (VAR_IN_OUT) are not supported yet"),
        ));
    }
    let mut outputs = Vec::new();
    for v in node
        .child("outputVariables")
        .into_iter()
        .flat_map(|n| n.children_named("variable"))
    {
        modifiers(v, places, diags);
        let formal = v.attr("formalParameter").unwrap_or("").to_ascii_uppercase();
        outputs.push((formal, flag(v, "negated", places, line, diags)));
    }
    Some((
        Kind::Block {
            type_name,
            instance,
            outputs,
        },
        inputs,
    ))
}

/// The checks of the graph that need all of it; `false` when one failed. A cycle of
/// connections is refused in LD (§8.1.5.2 rule a); in FBD the FBD front-end decides which
/// connections of a cycle are feedback paths.
pub(crate) fn check_graph(
    elems: &[Elem],
    lang: Lang,
    places: &Places,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let before = diags.len();
    let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
    for (k, e) in elems.iter().enumerate() {
        if let Some(first) = seen.insert(e.id, k) {
            diags.push(places.diag(
                e.line,
                format!(
                    "two elements have the localId {}: {} and {}; each element has its own",
                    e.id,
                    elems[first].what(),
                    e.what()
                ),
            ));
        }
    }
    if diags.len() != before {
        return false;
    }
    for e in elems {
        for p in &e.inputs {
            for c in &p.conns {
                let Some(&from) = seen.get(&c.from) else {
                    diags.push(places.diag(
                        p.line,
                        format!(
                            "{} {} is connected to localId {}, which no element of the body has",
                            e.what(),
                            e.id,
                            c.from
                        ),
                    ));
                    continue;
                };
                let src = &elems[from];
                if src.output(c.formal.as_deref()).is_none() {
                    diags.push(places.diag(
                        p.line,
                        match (&src.kind, &c.formal) {
                            (Kind::Block { .. }, Some(f)) => format!(
                                "{} {} is connected to the output {f} of block {}, which the block does not show",
                                e.what(),
                                e.id,
                                src.id
                            ),
                            (Kind::Block { .. }, None) => format!(
                                "{} {} is connected to block {}, which shows no output",
                                e.what(),
                                e.id,
                                src.id
                            ),
                            _ => format!(
                                "{} {} is connected to {} {}, which gives no output",
                                e.what(),
                                e.id,
                                src.what(),
                                src.id
                            ),
                        },
                    ));
                }
            }
        }
        let needs_input = matches!(
            e.kind,
            Kind::Contact { .. }
                | Kind::Coil { .. }
                | Kind::OutVar { .. }
                | Kind::InOutVar { .. }
                | Kind::Jump(_)
                | Kind::Return
                | Kind::Connector(_)
        );
        if needs_input
            && e.inputs
                .iter()
                .all(|p| p.conns.is_empty() && p.expr.is_none())
        {
            let to = match lang {
                Lang::Ld => "to the left power rail or to an element before it",
                Lang::Fbd | Lang::Sfc => "to a variable or to a block's output",
            };
            diags.push(places.diag(
                e.line,
                format!("{} {} has no input: connect it {to}", e.what(), e.id),
            ));
        }
    }
    if diags.len() != before {
        return false;
    }
    if lang == Lang::Fbd {
        return true;
    }
    // A cycle of connections, found without recursion: each element's state is 0 not seen,
    // 1 on the way, 2 done.
    let index = |id: u64| seen[&id];
    let mut state = vec![0u8; elems.len()];
    for root in 0..elems.len() {
        if state[root] != 0 {
            continue;
        }
        let ups = |k: usize| -> Vec<usize> {
            elems[k]
                .inputs
                .iter()
                .flat_map(|p| p.conns.iter().map(|c| index(c.from)))
                .collect()
        };
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
                    let from = trail.iter().position(|t| t.0 == next).unwrap_or(0);
                    let ids: Vec<String> = trail[from..]
                        .iter()
                        .map(|t| elems[t.0].id.to_string())
                        .collect();
                    diags.push(places.diag(
                        elems[next].line,
                        format!(
                            "the connections go round in a cycle through the elements {}; a Ladder Diagram has no feedback by wire: write the value to a variable and read it (IEC 61131-3 ed. 3 §8.1.5.2)",
                            ids.join(", ")
                        ),
                    ));
                    return false;
                }
                _ => {}
            }
        }
    }
    true
}

/// The hidden variables a body's lowering declares for itself (ADR-013 §2.15): each a
/// variable of the POU whose name begins with two underscores, which §6.1.2 forbids in every
/// name a program declares, marked hidden, so private.
#[derive(Debug, Default)]
pub(crate) struct Hidden {
    pub vars: Vec<VarDecl>,
}

/// The variable that says where a body with a jump or a return goes on: 0 on, -1 return, k to
/// the k-th label.
pub(crate) const JUMP: &str = "__JUMP";

impl Hidden {
    /// A hidden variable of the POU, declared once; the place it is.
    pub fn var(&mut self, name: String, ty: TypeSpec, span: Span) -> ast::Place {
        if !self.vars.iter().any(|v| v.name == name) {
            self.vars.push(VarDecl {
                name: name.clone(),
                span,
                section: Section::Var,
                retention: Retention::Unqualified,
                ty,
                init: None,
                hidden: true,
            });
        }
        ast::Place {
            name,
            span,
            path: Vec::new(),
        }
    }

    /// `__JUMP`, a DINT.
    pub fn jump(&mut self, span: Span) -> ast::Place {
        let ty = TypeSpec {
            span,
            kind: TypeSpecKind::Named("DINT".into()),
        };
        self.var(JUMP.into(), ty, span)
    }
}

pub(crate) fn bool_ty(span: Span) -> TypeSpec {
    TypeSpec {
        span,
        kind: TypeSpecKind::Named("BOOL".into()),
    }
}

pub(crate) fn bool_expr(value: bool, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Bool(value),
        span,
    }
}

fn int_expr(value: i128, span: Span) -> Expr {
    Expr {
        kind: if value < 0 {
            ExprKind::Neg(Box::new(Expr {
                kind: ExprKind::Int(-value),
                span,
            }))
        } else {
            ExprKind::Int(value)
        },
        span,
    }
}

/// `__JUMP = value`.
pub(crate) fn jump_is(jump: &ast::Place, value: i128, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Binary(
            BinOp::Eq,
            Box::new(Expr {
                kind: ExprKind::Var(jump.clone()),
                span,
            }),
            Box::new(int_expr(value, span)),
        ),
        span,
    }
}

/// `IF __JUMP = 0 THEN __JUMP := to; END_IF`: the first transfer of a network wins.
pub(crate) fn set_jump(jump: ast::Place, to: i128, span: Span) -> Vec<Stmt> {
    vec![Stmt::If(
        vec![(
            jump_is(&jump, 0, span),
            vec![Stmt::Assign(jump, int_expr(to, span))],
        )],
        None,
    )]
}

/// What a language's lowering gives [`body`].
pub(crate) trait Networks {
    fn elems(&self) -> &[Elem];
    fn index(&self) -> &BTreeMap<u64, usize>;
    fn span(&self, k: usize) -> Span;
    fn hidden(&mut self) -> &mut Hidden;
    /// The labels of the body in order, which [`body`] fills before any network is lowered.
    fn labels(&mut self) -> &mut Vec<String>;
    fn err(&mut self, k: usize, message: String);
    fn publish(&mut self, k: usize, at: At, holds: Holds);
    /// The statements of one network, whose elements are `members`, by position (top, then
    /// left).
    fn network(&mut self, members: &[usize]) -> Vec<Stmt>;
}

/// Whether element `k` is evaluated as part of a network: not a rail (§8.1.4), a comment, a
/// label, a connector or a continuation (a line, joined before, §8.1.3).
fn in_network(e: &Elem) -> bool {
    !matches!(
        e.kind,
        Kind::LeftRail
            | Kind::RightRail
            | Kind::Comment
            | Kind::Label(_)
            | Kind::Connector(_)
            | Kind::Continuation(_)
    )
}

/// The statements of a whole body: its networks in order, each lowered by the language
/// (`n.network`), with the labels, jumps, returns and network flags of §8.1.6 and ADR-013
/// §2.15.
///
/// **Networks**: a network is a maximal set of connected elements, the rails excluded
/// (§8.1.4); networks are evaluated top to bottom by the topmost element of each, then the
/// leftmost (§8.2.7 for LD; for FBD the implementer's, §8.3.3, and the same, ADR-013 §2.13).
/// **Jumps and returns** take effect when their network has been evaluated whole (§8.1.5.1
/// rule c), through `__JUMP`. **Flags**: each network sets `__NET_<localId>` of its first
/// element TRUE as it starts; in a body with a jump or a return every flag is set FALSE first
/// in the scan, so a network that did not run reads FALSE.
pub(crate) fn body<N: Networks>(n: &mut N) -> Vec<Stmt> {
    let elems = n.elems();
    let index = n.index();
    // Networks: connected elements, the rails excluded (§8.1.4).
    let mut parent: Vec<usize> = (0..elems.len()).collect();
    fn root(parent: &mut [usize], mut k: usize) -> usize {
        while parent[k] != k {
            parent[k] = parent[parent[k]];
            k = parent[k];
        }
        k
    }
    let rail = |k: usize| matches!(elems[k].kind, Kind::LeftRail | Kind::RightRail);
    for (k, e) in elems.iter().enumerate() {
        if rail(k) {
            continue;
        }
        for p in &e.inputs {
            for c in &p.conns {
                let from = index[&c.from];
                if !rail(from) {
                    let (a, b) = (root(&mut parent, k), root(&mut parent, from));
                    parent[a] = b;
                }
            }
        }
    }
    let mut networks: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut labels: Vec<usize> = Vec::new();
    for (k, e) in elems.iter().enumerate() {
        if let Kind::Label(_) = e.kind {
            labels.push(k);
        } else if in_network(e) {
            let r = root(&mut parent, k);
            networks.entry(r).or_default().push(k);
        }
    }
    let by_place = |a: &usize, b: &usize| {
        let (ea, eb) = (&elems[*a], &elems[*b]);
        ea.pos
            .1
            .total_cmp(&eb.pos.1)
            .then(ea.pos.0.total_cmp(&eb.pos.0))
            .then(ea.id.cmp(&eb.id))
    };
    // Each item: a label, or a network; by the topmost element, a label first.
    enum Item {
        Label(usize, String),
        Network(Vec<usize>),
    }
    let mut items: Vec<((f64, u8, f64, u64), Item)> = Vec::new();
    for (_, mut members) in networks {
        members.sort_by(by_place);
        let top =
            members
                .iter()
                .map(|k| elems[*k].pos)
                .fold((f64::INFINITY, f64::INFINITY), |a, p| {
                    if p.1 < a.1 || (p.1 == a.1 && p.0 < a.0) {
                        (p.0, p.1)
                    } else {
                        a
                    }
                });
        let first = elems[members[0]].id;
        items.push(((top.1, 1, top.0, first), Item::Network(members)));
    }
    for &k in &labels {
        let e = &elems[k];
        let Kind::Label(name) = &e.kind else {
            unreachable!()
        };
        items.push(((e.pos.1, 0, e.pos.0, e.id), Item::Label(k, name.clone())));
    }
    items.sort_by(|a, b| {
        a.0 .0
            .total_cmp(&b.0 .0)
            .then(a.0 .1.cmp(&b.0 .1))
            .then(a.0 .2.total_cmp(&b.0 .2))
            .then(a.0 .3.cmp(&b.0 .3))
    });
    // What the loop below needs of the elements, so that it may lower them.
    let jumps: Vec<(usize, String)> = elems
        .iter()
        .enumerate()
        .filter_map(|(k, e)| match &e.kind {
            Kind::Jump(l) => Some((k, l.clone())),
            _ => None,
        })
        .collect();
    let returns: Vec<usize> = (0..elems.len())
        .filter(|k| matches!(elems[*k].kind, Kind::Return))
        .collect();
    let ids: Vec<u64> = elems.iter().map(|e| e.id).collect();
    let transfers = !jumps.is_empty() || !returns.is_empty();
    // The labels, in order; two of one name are refused.
    for (_, item) in &items {
        if let Item::Label(k, name) = item {
            if n.labels().contains(name) {
                let id = ids[*k];
                n.err(
                    *k,
                    format!("label {id}: the body has two labels {name}; a jump goes to one"),
                );
            } else {
                n.labels().push(name.clone());
            }
        }
    }
    let mut backward = false;
    let mut seen_labels: Vec<String> = Vec::new();
    let mut flags: Vec<ast::Place> = Vec::new();
    let mut out: Vec<Stmt> = Vec::new();
    for (_, item) in &items {
        match item {
            Item::Label(k, name) => {
                seen_labels.push(name.clone());
                let targeted = jumps.iter().any(|(_, l)| l == name);
                if transfers && targeted {
                    let span = n.span(*k);
                    let at = n.labels().iter().position(|l| l == name).unwrap_or(0) as i128 + 1;
                    let jump = n.hidden().jump(span);
                    out.push(Stmt::If(
                        vec![(
                            jump_is(&jump, at, span),
                            vec![Stmt::Assign(
                                jump.clone(),
                                Expr {
                                    kind: ExprKind::Int(0),
                                    span,
                                },
                            )],
                        )],
                        None,
                    ));
                }
            }
            Item::Network(members) => {
                // The network's flag, `__NET_<localId>` of its first element: TRUE once it
                // runs in a scan.
                let first = members[0];
                let span = n.span(first);
                let flag = n
                    .hidden()
                    .var(format!("__NET_{}", ids[first]), bool_ty(span), span);
                n.publish(
                    first,
                    At::Network(members.iter().map(|k| ids[*k]).collect()),
                    Holds::Variable {
                        path: flag.text(),
                        negated: false,
                    },
                );
                let mut stmts = vec![Stmt::Assign(flag.clone(), bool_expr(true, span))];
                flags.push(flag);
                if jumps
                    .iter()
                    .any(|(k, l)| members.contains(k) && seen_labels.contains(l))
                {
                    backward = true;
                }
                stmts.extend(n.network(members));
                let returns_here = returns.iter().any(|k| members.contains(k));
                if transfers {
                    let jump = n.hidden().jump(span);
                    if returns_here {
                        stmts.push(Stmt::If(
                            vec![(jump_is(&jump, -1, span), vec![Stmt::Return(span)])],
                            None,
                        ));
                    }
                    out.push(Stmt::If(vec![(jump_is(&jump, 0, span), stmts)], None));
                } else {
                    out.extend(stmts);
                }
            }
        }
    }
    if !transfers {
        return out;
    }
    let span = n.span(0);
    let jump = n.hidden().jump(span);
    let mut all = vec![Stmt::Assign(
        jump.clone(),
        Expr {
            kind: ExprKind::Int(0),
            span,
        },
    )];
    all.extend(
        flags
            .into_iter()
            .map(|f| Stmt::Assign(f, bool_expr(false, span))),
    );
    if backward {
        all.push(Stmt::Repeat(out, jump_is(&jump, 0, span)));
    } else {
        all.extend(out);
    }
    all
}
