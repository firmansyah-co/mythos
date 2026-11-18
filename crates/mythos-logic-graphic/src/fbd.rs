//! An FBD body to statements of the Structured Text syntax tree (IEC 61131-3 ed. 3 §8.1, §8.3).
//!
//! **The graph** is LD's (`graph`): blocks calling functions and function blocks (EN, ENO, a
//! negated input or output), input, output and in-out variables, labels, jumps, returns and
//! comments, each connection naming the element and output it comes from. A Function Block
//! Diagram has no power rails, contacts or coils; each is refused by name.
//!
//! **Lines** (§8.1.3). A connector ends a line and each continuation of its name takes it up
//! again: before anything else, every connection from a continuation is made a connection
//! from what the connector's line comes from, so a connector and its continuations are one
//! line and join their networks. A connector's name is its own: one that is also a
//! variable's, the POU's or a label's is refused, as are two connectors of one name, a
//! continuation no connector names, and connectors that only continue one another.
//!
//! **No wired-OR** (§8.3.2). An input takes one line: two or more into one input are refused,
//! as is a line and an expression at one input; an OR block joins signals.
//!
//! **Feedback** (§8.1.5.2). A line that closes a loop of connections and runs from right to
//! left (the element it reaches stands at or left of the one it comes from) is a feedback
//! path: that is the implementer's means of rule b by which the user decides the order in an
//! explicit loop, by where the line back is drawn. It carries the output's value from the last
//! time its block was evaluated, its initial value (its type's default) at the first (rules
//! c and d). Refused, naming the elements: a line back that does not run from a block's
//! output to a block's input (the definition of a feedback path in §8.1.5.2); one from an
//! output whose type is known only once the loop is checked (ADD, SEL, MAX, …: write it to a
//! variable and read the variable instead, an implicit loop); a loop with no line drawn back.
//!
//! **Order.** Networks are LD's (`graph::body`): connected elements, top to bottom by the
//! topmost, then the leftmost (§8.3.3 leaves it to the implementer; ADR-013 §2.13). TC6's
//! `executionOrderId` is not read: the order is the standard's, not a tool's numbering (a CFC
//! mode that orders by it is a later slice). Within a network an element is evaluated once
//! every element it reads is (§8.1.5.1 rule a; a feedback path is read before its source is
//! evaluated again); of the elements ready, the topmost, then the leftmost, first; every
//! element is evaluated (rule c).
//!
//! **Values kept** (ADR-013 §2.16, the owner's decision of 2026-11-18). Every output an
//! element gives is evaluated once, when the element is, into a hidden variable of the POU:
//! `__FBD_<localId>` for a function's result, a variable read and an in-out variable;
//! `__FBD_<localId>_<formal>` for a function block's output; `__FBD_<localId>_ENO` for ENO
//! where it is not always TRUE. Every element that reads it reads the variable, so the value
//! a line carries is the value kept, and a negated output keeps its negation. Its type is the
//! value's: the ST checker types the variable by the value first assigned to it
//! (`ast::TypeSpecKind::OfValue`), so ADD of two INTs keeps an INT and of two REALs a REAL,
//! by the checker's one rule; a feedback path's source is declared with its type, which is
//! why it must be known first. A literal read is a constant, kept nowhere: its line carries
//! the literal written on the element.
//!
//! **EN and ENO** (§6.6.1.5), as in LD: EN is evaluated outside the block; when FALSE a
//! function is not called and a function block is not called and its inputs are not set. A
//! function's result is computed only where it runs, so it is not defined where it does not:
//! what reads it is not evaluated there (an output variable keeps its value, a function is
//! not called, a function block's input is not set), and ENO says where it ran: ENO is kept
//! for every function that may not run, the AND of its EN and the ENO of each function whose
//! result it reads. (§6.6.1.5 rule 4 leaves a block's outputs to the implementer where it
//! does not run.)

use crate::graph::{
    self, bool_expr, bool_ty, check_graph, read_body, set_jump, Conn, Elem, Hidden, Kind, Lang,
};
use crate::read::{GPou, Known};
use crate::{At, Holds, Places, Probe};
use mythos_logic_ir::{Diagnostic, Span};
use mythos_logic_st::ast::{
    self, Arg, BinOp, CallArg, Expr, ExprKind, Stmt, TypeSpec, TypeSpecKind,
};
use std::collections::{BTreeMap, BTreeSet};

/// An FBD POU with its statements, and the hidden variables they use; or, when its body
/// cannot be lowered, marked as not read whole, so the checker says nothing about it.
pub(crate) fn pou(
    p: &GPou,
    known: &Known,
    places: &mut Places,
    diags: &mut Vec<Diagnostic>,
    probes: &mut Vec<Probe>,
) -> ast::Pou {
    let mut tree = p.tree.clone();
    let Some((_, body)) = &p.body else {
        return tree;
    };
    let before = diags.len();
    places.keywords_as_names = p.keywords_as_names.clone();
    let mut elems = read_body(body, Lang::Fbd, p, places, diags);
    if diags.len() == before && check_graph(&elems, Lang::Fbd, places, diags) {
        let continued = join_lines(&mut elems, &tree, places, diags);
        if diags.len() == before {
            one_line_each(&elems, places, diags);
        }
        if diags.len() == before {
            let back = feedback(&elems, known, places, diags);
            if diags.len() == before {
                let mut l = Lowering::new(&elems, &tree, known, places, back);
                let stmts = graph::body(&mut l);
                l.continuations(&continued);
                let found = l.probes(&tree.name);
                let (hidden, said) = (l.hidden.vars, l.diags);
                diags.extend(said);
                if diags.len() == before {
                    probes.extend(found);
                    tree.body = stmts;
                    tree.vars.extend(hidden);
                    return tree;
                }
            }
        }
    }
    tree.partial_body = true;
    tree
}

/// Join each connector's line to its continuations (§8.1.3): every connection from a
/// continuation becomes one from what reaches its connector. The continuations, each with the
/// connection it now stands for.
fn join_lines(
    elems: &mut [Elem],
    pou: &ast::Pou,
    places: &Places,
    diags: &mut Vec<Diagnostic>,
) -> Vec<(usize, Conn)> {
    let mut connectors: BTreeMap<String, usize> = BTreeMap::new();
    // The names of connectors refused: their continuations follow from that, and are not told.
    let mut refused: Vec<String> = Vec::new();
    let labels: Vec<&str> = elems
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Label(l) => Some(l.as_str()),
            _ => None,
        })
        .collect();
    for (k, e) in elems.iter().enumerate() {
        let Kind::Connector(name) = &e.kind else {
            continue;
        };
        if let Some(&first) = connectors.get(name) {
            diags.push(places.diag(
                e.line,
                format!(
                    "connectors {} and {} have the name {name}; a line goes on from one connector of a name",
                    elems[first].id, e.id
                ),
            ));
            continue;
        }
        // §8.1.3: no storage is associated with a connector, so its name may not be another
        // named element's.
        let other = if pou.vars.iter().any(|v| v.name.eq_ignore_ascii_case(name)) {
            Some("a variable of the POU")
        } else if pou.name.eq_ignore_ascii_case(name) {
            Some("the POU")
        } else if labels.contains(&name.as_str()) {
            Some("a label of the body")
        } else {
            None
        };
        if let Some(other) = other {
            diags.push(places.diag(
                e.line,
                format!(
                    "connector {}: {name} is also the name of {other}; a connector's name is its own (IEC 61131-3 ed. 3 §8.1.3)",
                    e.id
                ),
            ));
            refused.push(name.clone());
            continue;
        }
        let pin = &e.inputs[0];
        if pin.expr.is_some() {
            diags.push(places.diag(
                pin.line,
                format!(
                    "connector {} has an expression; a connector ends a line from an output",
                    e.id
                ),
            ));
            refused.push(name.clone());
            continue;
        }
        connectors.insert(name.clone(), k);
    }
    let by_id: BTreeMap<u64, usize> = elems.iter().enumerate().map(|(k, e)| (e.id, k)).collect();
    let mut continued = Vec::new();
    for (k, e) in elems.iter().enumerate() {
        let Kind::Continuation(name) = &e.kind else {
            continue;
        };
        let Some(&first) = connectors.get(name) else {
            if refused.contains(name) {
                continue;
            }
            diags.push(places.diag(
                e.line,
                format!(
                    "continuation {} goes on with the line {name}, which no connector of the body has",
                    e.id
                ),
            ));
            continue;
        };
        // Follow connectors that are fed by continuations, to the output the line starts at.
        let mut at = first;
        let mut seen = vec![first];
        while let Some(c) = elems[at].inputs[0].conns.first() {
            let from = by_id[&c.from];
            let Kind::Continuation(next) = &elems[from].kind else {
                continued.push((k, c.clone()));
                break;
            };
            match connectors.get(next) {
                Some(&n) if !seen.contains(&n) => {
                    seen.push(n);
                    at = n;
                }
                Some(_) => {
                    let names: Vec<String> = seen
                        .iter()
                        .filter_map(|s| match &elems[*s].kind {
                            Kind::Connector(n) => Some(n.clone()),
                            _ => None,
                        })
                        .collect();
                    diags.push(places.diag(
                        e.line,
                        format!(
                            "continuation {}: the connectors {} only continue one another; a line starts at an output",
                            e.id,
                            names.join(", ")
                        ),
                    ));
                    break;
                }
                // Said where that continuation is.
                None => break,
            }
        }
    }
    if !diags.is_empty() {
        return continued;
    }
    let to: BTreeMap<u64, Conn> = continued
        .iter()
        .map(|(k, c)| (elems[*k].id, c.clone()))
        .collect();
    for e in elems.iter_mut() {
        for pin in &mut e.inputs {
            for c in &mut pin.conns {
                if let Some(line) = to.get(&c.from) {
                    *c = line.clone();
                }
            }
        }
    }
    continued
}

/// No wired-OR (§8.3.2): an input takes one line, or an expression, not both.
fn one_line_each(elems: &[Elem], places: &Places, diags: &mut Vec<Diagnostic>) {
    for e in elems {
        for pin in &e.inputs {
            let at = match &pin.formal {
                Some(f) => format!("its input {f}"),
                None => "its input".to_string(),
            };
            if pin.conns.len() > 1 {
                diags.push(places.diag(
                    pin.line,
                    format!(
                        "{} {} has {} lines into {at}; a Function Block Diagram joins no lines (no wired-OR, IEC 61131-3 ed. 3 §8.3.2): join them with an OR block",
                        e.what(),
                        e.id,
                        pin.conns.len()
                    ),
                ));
            } else if !pin.conns.is_empty() && pin.expr.is_some() {
                diags.push(places.diag(
                    pin.line,
                    format!(
                        "{} {} has a line and an expression at {at}; an input takes one",
                        e.what(),
                        e.id
                    ),
                ));
            }
        }
    }
}

/// The feedback paths of a body (§8.1.5.2): each by the element it reaches and its input, with
/// the type of the output it comes from, by the source element and output.
struct Feedback {
    back: BTreeSet<(usize, usize)>,
    types: BTreeMap<(usize, String), TypeSpec>,
}

/// The source of input `pin` of element `k`: the element and output in upper case.
fn source(elems: &[Elem], index: &BTreeMap<u64, usize>, c: &Conn) -> (usize, String) {
    let from = index[&c.from];
    let out = elems[from].output(c.formal.as_deref()).unwrap_or_default();
    (from, out)
}

/// The strongly connected parts of the graph, each element's part by number; found without
/// recursion (Kosaraju).
fn parts(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut prev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        next[a].push(b);
        prev[b].push(a);
    }
    let mut seen = vec![false; n];
    let mut finished = Vec::with_capacity(n);
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((k, i)) = stack.pop() {
            if let Some(&m) = next[k].get(i) {
                stack.push((k, i + 1));
                if !seen[m] {
                    seen[m] = true;
                    stack.push((m, 0));
                }
            } else {
                finished.push(k);
            }
        }
    }
    let mut part = vec![usize::MAX; n];
    let mut count = 0;
    for &root in finished.iter().rev() {
        if part[root] != usize::MAX {
            continue;
        }
        part[root] = count;
        let mut stack = vec![root];
        while let Some(k) = stack.pop() {
            for &m in &prev[k] {
                if part[m] == usize::MAX {
                    part[m] = count;
                    stack.push(m);
                }
            }
        }
        count += 1;
    }
    part
}

/// Find the feedback paths, refusing the loops the standard does not allow.
fn feedback(
    elems: &[Elem],
    known: &Known,
    places: &Places,
    diags: &mut Vec<Diagnostic>,
) -> Feedback {
    let index: BTreeMap<u64, usize> = elems.iter().enumerate().map(|(k, e)| (e.id, k)).collect();
    let mut lines = Vec::new();
    for (k, e) in elems.iter().enumerate() {
        for (pin, p) in e.inputs.iter().enumerate() {
            for c in &p.conns {
                lines.push((source(elems, &index, c), k, pin));
            }
        }
    }
    let edges: Vec<(usize, usize)> = lines.iter().map(|((s, _), t, _)| (*s, *t)).collect();
    let part = parts(elems.len(), &edges);
    let mut found = Feedback {
        back: BTreeSet::new(),
        types: BTreeMap::new(),
    };
    for ((s, out), t, pin) in &lines {
        let (s, t) = (*s, *t);
        // In a loop, and drawn back from right to left: the element it reaches stands at or
        // left of the one it comes from (a position that is not a number stands nowhere).
        let back = matches!(
            elems[t].pos.0.partial_cmp(&elems[s].pos.0),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        );
        if part[s] != part[t] || !back {
            continue;
        }
        let (src, dst) = (&elems[s], &elems[t]);
        if !matches!(src.kind, Kind::Block { .. }) || !matches!(dst.kind, Kind::Block { .. }) {
            diags.push(places.diag(
                dst.inputs[*pin].line,
                format!(
                    "the line from {} {} back to {} {} closes a loop; a feedback path runs from a block's output back to a block's input (IEC 61131-3 ed. 3 §8.1.5.2): write the value to a variable and read it",
                    src.what(),
                    src.id,
                    dst.what(),
                    dst.id
                ),
            ));
            continue;
        }
        let Some(ty) = output_type(src, out, known, dst.inputs[*pin].line, places) else {
            let Kind::Block { type_name, .. } = &src.kind else {
                unreachable!()
            };
            diags.push(places.diag(
                dst.inputs[*pin].line,
                format!(
                    "the output {out} of block {} ({type_name}) goes back to block {} by a feedback path, and its type is known only once the loop is checked: write it to a variable and read the variable in its place (an implicit loop, IEC 61131-3 ed. 3 §8.1.5.2 b)",
                    src.id, dst.id
                ),
            ));
            continue;
        };
        found.back.insert((t, *pin));
        found.types.insert((s, out.clone()), ty);
    }
    if !diags.is_empty() {
        return found;
    }
    // Without the feedback paths, nothing goes round: a loop that remains has no line drawn
    // back (positions the document gives no order to).
    let mut waiting: Vec<usize> = vec![0; elems.len()];
    let mut readers: Vec<Vec<usize>> = vec![Vec::new(); elems.len()];
    for ((s, _), t, pin) in &lines {
        if !found.back.contains(&(*t, *pin)) {
            waiting[*t] += 1;
            readers[*s].push(*t);
        }
    }
    let mut ready: Vec<usize> = (0..elems.len()).filter(|k| waiting[*k] == 0).collect();
    while let Some(k) = ready.pop() {
        for &r in &readers[k] {
            waiting[r] -= 1;
            if waiting[r] == 0 {
                ready.push(r);
            }
        }
    }
    // Of what never became ready, the elements on a loop, not those after one.
    let mut size = vec![0usize; elems.len()];
    for p in &part {
        size[*p] += 1;
    }
    let stuck: Vec<usize> = (0..elems.len())
        .filter(|k| waiting[*k] > 0 && size[part[*k]] > 1)
        .collect();
    if let Some(&first) = stuck.first() {
        let ids: Vec<String> = stuck.iter().map(|k| elems[*k].id.to_string()).collect();
        diags.push(places.diag(
            elems[first].line,
            format!(
                "the lines go round in a loop through the elements {} with none drawn back from right to left, so nothing says where the loop starts: draw the line back to the left (a feedback path, IEC 61131-3 ed. 3 §8.1.5.2)",
                ids.join(", ")
            ),
        ));
    }
    found
}

/// The type of output `out` of block `e`, where it is known before checking: a function block
/// output's declaration, the result of a function of the project, a comparison, a Boolean
/// operator or a conversion.
fn output_type(e: &Elem, out: &str, known: &Known, line: u32, places: &Places) -> Option<TypeSpec> {
    let Kind::Block {
        type_name,
        instance,
        outputs,
    } = &e.kind
    else {
        return None;
    };
    let span = places.span(line);
    let negated = outputs.iter().any(|(o, n)| o == out && *n);
    let upper = type_name.to_ascii_uppercase();
    if out == "ENO" || negated {
        return Some(bool_ty(span));
    }
    if instance.is_some() {
        return known.output_type(&upper, out).map(|mut t| {
            t.span = span;
            t
        });
    }
    result_type(known, type_name, span)
}

/// The type of the result of function `type_name`, where it is known without checking: a
/// function of the project, a comparison or Boolean operator, a conversion.
fn result_type(known: &Known, type_name: &str, span: Span) -> Option<TypeSpec> {
    let upper = type_name.to_ascii_uppercase();
    if let Some(t) = known.result_of(&upper) {
        return Some(TypeSpec {
            span,
            kind: t.kind.clone(),
        });
    }
    let named = |n: &str| TypeSpec {
        span,
        kind: TypeSpecKind::Named(n.into()),
    };
    if matches!(
        upper.as_str(),
        "GT" | "GE" | "EQ" | "LE" | "LT" | "NE" | "AND" | "OR" | "XOR" | "NOT"
    ) {
        return Some(named("BOOL"));
    }
    let (_, to) = upper.split_once("_TO_")?;
    matches!(
        to,
        "BOOL" | "SINT" | "INT" | "DINT" | "LINT" | "REAL" | "LREAL" | "TIME"
    )
    .then(|| named(to))
}

/// What an output gives: its value, and the ENO of the function under which it is defined
/// (`None`: always).
#[derive(Debug, Clone)]
struct Val {
    value: Expr,
    ran: Option<ast::Place>,
}

fn load(place: &ast::Place) -> Expr {
    Expr {
        kind: ExprKind::Var(place.clone()),
        span: place.span,
    }
}

fn not(e: Expr) -> Expr {
    let span = e.span;
    Expr {
        kind: ExprKind::Not(Box::new(e)),
        span,
    }
}

fn and(items: Vec<Expr>, span: Span) -> Expr {
    let mut it = items.into_iter();
    let first = it.next().unwrap_or_else(|| bool_expr(true, span));
    it.fold(first, |a, b| Expr {
        kind: ExprKind::Binary(BinOp::And, Box::new(a), Box::new(b)),
        span,
    })
}

fn is_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_)
        | ExprKind::Real(_)
        | ExprKind::Bool(_)
        | ExprKind::Time(_)
        | ExprKind::Str(_) => true,
        ExprKind::Neg(inner) => matches!(inner.kind, ExprKind::Int(_) | ExprKind::Real(_)),
        _ => false,
    }
}

/// The state of lowering one body.
struct Lowering<'a> {
    elems: &'a [Elem],
    pou: &'a ast::Pou,
    known: &'a Known,
    places: &'a mut Places,
    index: BTreeMap<u64, usize>,
    back: Feedback,
    /// The value of each output evaluated, by element and output in upper case.
    values: BTreeMap<(usize, String), Val>,
    stmts: Vec<Stmt>,
    hidden: Hidden,
    diags: Vec<Diagnostic>,
    labels: Vec<String>,
    /// The instances called by a block of the network being lowered.
    called: Vec<(String, u64)>,
    /// What holds each output and network: what is published.
    published: BTreeMap<(u64, At), Holds>,
}

impl<'a> Lowering<'a> {
    fn new(
        elems: &'a [Elem],
        pou: &'a ast::Pou,
        known: &'a Known,
        places: &'a mut Places,
        back: Feedback,
    ) -> Self {
        Lowering {
            elems,
            pou,
            known,
            places,
            index: elems.iter().enumerate().map(|(k, e)| (e.id, k)).collect(),
            back,
            values: BTreeMap::new(),
            stmts: Vec::new(),
            hidden: Hidden::default(),
            diags: Vec::new(),
            labels: Vec::new(),
            called: Vec::new(),
            published: BTreeMap::new(),
        }
    }

    fn span(&self, k: usize) -> Span {
        self.places.span(self.elems[k].line)
    }

    fn err(&mut self, k: usize, message: String) {
        let d = self.places.diag(self.elems[k].line, message);
        self.diags.push(d);
    }

    fn publish(&mut self, k: usize, at: At, holds: Holds) {
        self.published.insert((self.elems[k].id, at), holds);
    }

    /// The hidden variable keeping output `out` of element `k`: `__FBD_<localId>` for the one
    /// output of a variable and for a function's result, `__FBD_<localId>_<formal>` for any
    /// other. Typed by its value, or with the type a feedback path needs known.
    fn out_var(&mut self, k: usize, out: &str) -> ast::Place {
        let id = self.elems[k].id;
        let function = matches!(&self.elems[k].kind, Kind::Block { instance: None, .. });
        let name = if out.is_empty() || (function && out == "OUT") {
            format!("__FBD_{id}")
        } else {
            format!("__FBD_{id}_{out}")
        };
        let span = self.span(k);
        let ty = match self.back.types.get(&(k, out.to_string())) {
            Some(t) => t.clone(),
            None if out == "ENO" => bool_ty(span),
            None => TypeSpec {
                span,
                kind: TypeSpecKind::OfValue,
            },
        };
        self.hidden.var(name, ty, span)
    }

    /// Keep `value` of output `out` of element `k`: assigned now, or where `ran` is TRUE;
    /// published as what holds it.
    fn keep(&mut self, k: usize, out: &str, value: Expr, ran: Option<ast::Place>) -> Val {
        let place = self.out_var(k, out);
        let assign = Stmt::Assign(place.clone(), value);
        let holds = match &ran {
            Some(r) => {
                self.stmts
                    .push(Stmt::If(vec![(load(r), vec![assign])], None));
                Holds::Guarded {
                    path: place.text(),
                    ran: r.text(),
                }
            }
            None => {
                self.stmts.push(assign);
                Holds::Variable {
                    path: place.text(),
                    negated: false,
                }
            }
        };
        self.publish(k, At::Output(out.to_string()), holds);
        let v = Val {
            value: load(&place),
            ran,
        };
        self.values.insert((k, out.to_string()), v.clone());
        v
    }

    /// What reaches input `pin` of element `k`: the line's value, or the expression written
    /// there; `None` when nothing does. A negated input is the negation of what reaches it.
    fn value_in(&mut self, k: usize, pin: usize) -> Option<Val> {
        let p = self.elems[k].inputs[pin].clone();
        // What a line brings is told at the input it reaches, as an expression written there.
        let at = self.places.span(p.line);
        let v = match (p.conns.first(), p.expr) {
            (Some(c), _) => {
                let (from, out) = source(self.elems, &self.index, c);
                if self.back.back.contains(&(k, pin)) {
                    // A feedback path: what its source kept when it was last evaluated.
                    let place = self.out_var(from, &out);
                    Val {
                        value: load(&place),
                        ran: None,
                    }
                } else {
                    // None where its source was refused, and why was said.
                    self.values.get(&(from, out.clone()))?.clone()
                }
            }
            (None, Some(e)) => Val {
                value: e,
                ran: None,
            },
            (None, None) => return None,
        };
        let v = if p.conns.is_empty() {
            v
        } else {
            Val {
                value: Expr {
                    span: at,
                    ..v.value
                },
                ran: v.ran,
            }
        };
        Some(if p.negated {
            Val {
                value: not(v.value),
                ran: v.ran,
            }
        } else {
            v
        })
    }

    /// Evaluate element `k`: its statements, and the value of each of its outputs kept.
    fn eval(&mut self, k: usize) {
        let span = self.span(k);
        let id = self.elems[k].id;
        match self.elems[k].kind.clone() {
            Kind::InVar { expr, negated } => {
                let value = if negated { not(expr) } else { expr };
                if is_literal(&value) || matches!(&value.kind, ExprKind::Not(e) if is_literal(e)) {
                    self.values
                        .insert((k, String::new()), Val { value, ran: None });
                } else {
                    self.keep(k, "", value, None);
                }
            }
            Kind::OutVar { target, negated } => {
                let Some(v) = self.value_in(k, 0) else { return };
                let value = if negated { not(v.value) } else { v.value };
                let s = Stmt::Assign(target, value);
                self.stmts.push(match v.ran {
                    Some(r) => Stmt::If(vec![(load(&r), vec![s])], None),
                    None => s,
                });
            }
            Kind::InOutVar {
                target,
                negated_in,
                negated_out,
            } => {
                if let Some(v) = self.value_in(k, 0) {
                    let value = if negated_in { not(v.value) } else { v.value };
                    let s = Stmt::Assign(target.clone(), value);
                    self.stmts.push(match v.ran {
                        Some(r) => Stmt::If(vec![(load(&r), vec![s])], None),
                        None => s,
                    });
                }
                let read = load(&target);
                let read = if negated_out { not(read) } else { read };
                self.keep(k, "", read, None);
            }
            Kind::Block {
                type_name,
                instance: Some(instance),
                outputs,
            } => self.call_block(k, &type_name, &instance, &outputs),
            Kind::Block {
                type_name,
                instance: None,
                outputs,
            } => {
                if self.known.is_block(&type_name.to_ascii_uppercase()) {
                    self.err(k, format!("block {id} calls the function block type {type_name} without an instance: give it an instanceName"));
                    return;
                }
                self.call_function(k, &type_name, &outputs);
            }
            Kind::Jump(label) => {
                let Some(cond) = self.condition(k) else {
                    return;
                };
                let Some(at) = self.labels.iter().position(|l| *l == label) else {
                    self.err(
                        k,
                        format!(
                            "jump {id} goes to the label {label}, which the body does not have"
                        ),
                    );
                    return;
                };
                let jump = self.hidden.jump(span);
                let s = set_jump(jump, (at + 1) as i128, span);
                self.stmts.push(Stmt::If(vec![(cond, s)], None));
            }
            Kind::Return => {
                let Some(cond) = self.condition(k) else {
                    return;
                };
                let jump = self.hidden.jump(span);
                let s = set_jump(jump, -1, span);
                self.stmts.push(Stmt::If(vec![(cond, s)], None));
            }
            Kind::LeftRail
            | Kind::RightRail
            | Kind::Contact { .. }
            | Kind::Coil { .. }
            | Kind::Label(_)
            | Kind::Comment
            | Kind::Connector(_)
            | Kind::Continuation(_) => {}
        }
    }

    /// The condition of a jump or a return: its input, where it is defined.
    fn condition(&mut self, k: usize) -> Option<Expr> {
        let span = self.span(k);
        let v = self.value_in(k, 0)?;
        Some(match v.ran {
            Some(r) => and(vec![load(&r), v.value], span),
            None => v.value,
        })
    }

    /// The inputs of block `k` but EN: each formal parameter with its value.
    fn block_inputs(&mut self, k: usize) -> Vec<(String, Val, Span)> {
        let mut out = Vec::new();
        for pin in 0..self.elems[k].inputs.len() {
            let p = self.elems[k].inputs[pin].clone();
            let formal = p.formal.clone().unwrap_or_default();
            if formal == "EN" {
                continue;
            }
            if let Some(v) = self.value_in(k, pin) {
                out.push((formal, v, self.places.span(p.line)));
            }
        }
        out
    }

    /// EN of block `k`, where it shows one that something reaches: its value, where defined.
    fn enable(&mut self, k: usize) -> Option<Expr> {
        let span = self.span(k);
        let pin = self.elems[k]
            .inputs
            .iter()
            .position(|p| p.formal.as_deref() == Some("EN"))?;
        let v = self.value_in(k, pin)?;
        Some(match v.ran {
            Some(r) => and(vec![load(&r), v.value], span),
            None => v.value,
        })
    }

    /// ENO of block `k`: kept where the block may not run (`condition`, the AND of its EN and
    /// the ENO of each function it reads), shown or not, since what reads a function's result
    /// is defined where it is TRUE; TRUE where nothing can stop the block. The variable kept.
    fn keep_eno(&mut self, k: usize, condition: Option<Expr>) -> Option<ast::Place> {
        let negated = match &self.elems[k].kind {
            Kind::Block { outputs, .. } => outputs.iter().any(|(o, n)| o == "ENO" && *n),
            _ => false,
        };
        let shown = match &self.elems[k].kind {
            Kind::Block { outputs, .. } => outputs.iter().any(|(o, _)| o == "ENO"),
            _ => false,
        };
        let span = self.span(k);
        let Some(condition) = condition else {
            if shown {
                // Nothing reaches EN and nothing it reads may be undefined: always TRUE.
                self.publish(k, At::Output("ENO".into()), Holds::On);
                let value = bool_expr(!negated, span);
                self.values
                    .insert((k, "ENO".into()), Val { value, ran: None });
            }
            return None;
        };
        let place = self.out_var(k, "ENO");
        self.stmts.push(Stmt::Assign(place.clone(), condition));
        self.publish(
            k,
            At::Output("ENO".into()),
            Holds::Variable {
                path: place.text(),
                negated,
            },
        );
        let value = if negated {
            not(load(&place))
        } else {
            load(&place)
        };
        self.values
            .insert((k, "ENO".into()), Val { value, ran: None });
        Some(place)
    }

    fn call_function(&mut self, k: usize, type_name: &str, outputs: &[(String, bool)]) {
        let span = self.span(k);
        let id = self.elems[k].id;
        if let Some((o, _)) = outputs.iter().find(|(o, _)| o != "OUT" && o != "ENO") {
            self.err(k, format!("block {id} calls the function {type_name}, which gives one output, OUT; it shows {o}"));
            return;
        }
        let enable = self.enable(k);
        let inputs = self.block_inputs(k);
        let mut guards: Vec<Expr> = enable.into_iter().collect();
        let mut values = Vec::new();
        for (formal, v, pin_span) in inputs {
            if let Some(r) = v.ran {
                guards.push(load(&r));
            }
            values.push((formal, v.value, pin_span));
        }
        let upper = type_name.to_ascii_uppercase();
        // AND, OR, XOR, NOT and MOD are operators of ST, not functions (ADR-013 §2.13); as
        // blocks they take IN1, IN2, … (NOT takes IN).
        let operator = match upper.as_str() {
            "AND" => Some(BinOp::And),
            "OR" => Some(BinOp::Or),
            "XOR" => Some(BinOp::Xor),
            "MOD" => Some(BinOp::Mod),
            _ => None,
        };
        let call = if let Some(op) = operator {
            let n = values.len();
            if n < 2 || (op == BinOp::Mod && n != 2) {
                let takes = if op == BinOp::Mod {
                    "two"
                } else {
                    "two or more"
                };
                self.err(
                    k,
                    format!("block {id} ({type_name}) takes {takes} inputs; it has {n} connected"),
                );
                return;
            }
            let mut it = values.into_iter().map(|(_, e, _)| e);
            let first = it.next().expect("two inputs");
            it.fold(first, |a, b| Expr {
                kind: ExprKind::Binary(op, Box::new(a), Box::new(b)),
                span,
            })
        } else if upper == "NOT" {
            if values.len() != 1 {
                self.err(
                    k,
                    format!(
                        "block {id} (NOT) takes one input, IN; it has {} connected",
                        values.len()
                    ),
                );
                return;
            }
            let (_, e, _) = values.pop().expect("one input");
            not(e)
        } else {
            // A conversion takes its one input by position (Table 22: its pin is IN, which ST
            // does not name); every other function by its formal parameters.
            let positional =
                values.len() == 1 && (upper.contains("_TO_") || upper.contains("TRUNC_"));
            let args = values
                .into_iter()
                .map(|(formal, value, pin_span)| CallArg {
                    name: (!positional).then_some((formal, pin_span)),
                    value,
                })
                .collect();
            Expr {
                kind: ExprKind::Call(type_name.to_string(), args),
                span,
            }
        };
        let condition = (!guards.is_empty()).then(|| and(guards, span));
        // ENO, kept wherever the function may not run, shown or not: what reads its result is
        // defined where it is TRUE.
        let ran = self.keep_eno(k, condition);
        let negated = outputs.iter().any(|(o, n)| o == "OUT" && *n);
        let call = if negated { not(call) } else { call };
        self.keep(k, "OUT", call, ran);
    }

    fn call_block(
        &mut self,
        k: usize,
        type_name: &str,
        instance: &ast::Place,
        outputs: &[(String, bool)],
    ) {
        let span = self.span(k);
        let id = self.elems[k].id;
        let upper = type_name.to_ascii_uppercase();
        if !self.known.is_block(&upper) {
            self.err(
                k,
                format!(
                    "block {id} has the instance {} but {type_name} is not a function block type",
                    instance.text()
                ),
            );
            return;
        }
        if instance.path.is_empty() {
            let declared = self
                .pou
                .vars
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(&instance.name));
            if let Some(TypeSpec {
                kind: TypeSpecKind::Named(n),
                ..
            }) = declared.map(|v| &v.ty)
            {
                if !n.eq_ignore_ascii_case(type_name) {
                    self.err(
                        k,
                        format!(
                            "block {id} is a {type_name}, but its instance {} is a {n}",
                            instance.name
                        ),
                    );
                    return;
                }
            }
        }
        let key = instance.text().to_ascii_uppercase();
        if let Some((_, other)) = self.called.iter().find(|(i, _)| *i == key) {
            let other = *other;
            self.err(k, format!("blocks {other} and {id} of one network both call the instance {}; a network calls an instance once", instance.text()));
            return;
        }
        self.called.push((key, id));
        let enable = self.enable(k);
        let ran = self.keep_eno(k, enable);
        let inputs = self.block_inputs(k);
        let mut inner = Vec::new();
        let mut args = Vec::new();
        for (formal, v, pin_span) in inputs {
            match v.ran {
                // Set apart from the call, only where it is defined.
                Some(r) => {
                    let mut member = instance.clone();
                    member.path.push(ast::Selector::Member(formal, pin_span));
                    inner.push(Stmt::If(
                        vec![(load(&r), vec![Stmt::Assign(member, v.value)])],
                        None,
                    ));
                }
                None => args.push(Arg::In(formal, v.value, pin_span)),
            }
        }
        inner.push(Stmt::Call(instance.clone(), args));
        match &ran {
            Some(r) => self.stmts.push(Stmt::If(vec![(load(r), inner)], None)),
            None => self.stmts.extend(inner),
        }
        // Each output read from the instance once it is called, or not: an instance's outputs
        // keep their values where it is not called.
        for (formal, negated) in outputs {
            if formal == "ENO" {
                continue;
            }
            let mut member = instance.clone();
            member
                .path
                .push(ast::Selector::Member(formal.clone(), span));
            let read = load(&member);
            let read = if *negated { not(read) } else { read };
            self.keep(k, formal, read, None);
        }
    }

    /// Each continuation holds what the line it goes on with comes from.
    fn continuations(&mut self, continued: &[(usize, Conn)]) {
        for (k, c) in continued {
            let (from, out) = source(self.elems, &self.index, c);
            let id = self.elems[from].id;
            if let Some(holds) = self.published.get(&(id, At::Output(out))).cloned() {
                self.publish(*k, At::Output(String::new()), holds);
            }
        }
    }

    /// What holds every output and network of the body: each a hidden variable assigned when
    /// it is evaluated, or ON (ADR-013 §2.16).
    fn probes(&self, pou: &str) -> Vec<Probe> {
        self.published
            .iter()
            .map(|((local_id, at), holds)| Probe {
                pou: pou.to_string(),
                local_id: *local_id,
                at: at.clone(),
                holds: holds.clone(),
            })
            .collect()
    }

    /// The elements of a network in the order they are evaluated: an element once every
    /// element it reads by a line that is not a feedback path is; of those ready, the topmost,
    /// then the leftmost (`members` is in that order).
    fn order(&self, members: &[usize]) -> Vec<usize> {
        let rank: BTreeMap<usize, usize> =
            members.iter().enumerate().map(|(r, k)| (*k, r)).collect();
        let mut waiting: BTreeMap<usize, usize> = members.iter().map(|k| (*k, 0)).collect();
        let mut readers: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &k in members {
            for (pin, p) in self.elems[k].inputs.iter().enumerate() {
                if self.back.back.contains(&(k, pin)) {
                    continue;
                }
                for c in &p.conns {
                    let (from, _) = source(self.elems, &self.index, c);
                    if rank.contains_key(&from) {
                        *waiting.get_mut(&k).expect("a member") += 1;
                        readers.entry(from).or_default().push(k);
                    }
                }
            }
        }
        let mut ready: BTreeSet<usize> = members
            .iter()
            .filter(|k| waiting[k] == 0)
            .map(|k| rank[k])
            .collect();
        let mut out = Vec::with_capacity(members.len());
        while let Some(r) = ready.pop_first() {
            let k = members[r];
            out.push(k);
            for &reader in readers.get(&k).map(Vec::as_slice).unwrap_or(&[]) {
                let w = waiting.get_mut(&reader).expect("a member");
                *w -= 1;
                if *w == 0 {
                    ready.insert(rank[&reader]);
                }
            }
        }
        out
    }
}

impl graph::Networks for Lowering<'_> {
    fn elems(&self) -> &[Elem] {
        self.elems
    }
    fn index(&self) -> &BTreeMap<u64, usize> {
        &self.index
    }
    fn span(&self, k: usize) -> Span {
        Lowering::span(self, k)
    }
    fn hidden(&mut self) -> &mut Hidden {
        &mut self.hidden
    }
    fn labels(&mut self) -> &mut Vec<String> {
        &mut self.labels
    }
    fn err(&mut self, k: usize, message: String) {
        Lowering::err(self, k, message)
    }
    fn publish(&mut self, k: usize, at: At, holds: Holds) {
        Lowering::publish(self, k, at, holds)
    }
    fn network(&mut self, members: &[usize]) -> Vec<Stmt> {
        self.called.clear();
        self.stmts.clear();
        for k in self.order(members) {
            self.eval(k);
        }
        std::mem::take(&mut self.stmts)
    }
}
