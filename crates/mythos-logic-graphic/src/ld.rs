//! An LD body to statements of the Structured Text syntax tree (IEC 61131-3 ed. 3 §8.1, §8.2).
//!
//! **The graph.** Each element is read with its `localId`, its position and its inputs: a
//! `connectionPointIn` holds the connections that reach it, each naming the element it comes
//! from (`refLocalId`) and, from a block, which output (`formalParameter`). Refused, each
//! naming the element: two elements with one `localId`; a connection to an id no element has
//! (dangling), or to an element that gives no output; a contact, coil, output variable, jump
//! or return with no input; a cycle of connections (LD has no feedback by wire: §8.1.5.2
//! rule a gives explicit loops to FBD alone, and a feedback variable is a variable).
//!
//! **Power flow** (§8.2.3). The left rail is ON. A connection carries the state of the link
//! it starts from; several connections into one input are a vertical link, the OR of them; a
//! contact is the AND of its input and its variable (Table 75); a coil passes its input on
//! (Table 76). So elements in series are AND and branches in parallel are OR.
//!
//! **Order.** A network is a maximal set of connected elements, the rails excluded (§8.1.4).
//! Networks are evaluated top to bottom (§8.2.7): by the topmost element of each, then the
//! leftmost. Within one, no element is evaluated before its inputs (§8.1.5.1 rule a), and
//! every element is evaluated (rule c), in the order of their positions (top, then left)
//! where the wiring leaves it free. A value that more than one statement needs (a branch
//! point, a coil whose output goes on, an edge's output) is kept in a hidden variable, so it
//! is evaluated once, as an element is.
//!
//! **Edges** (Table 75 features 3–4, Table 76 features 8–9). A transition-sensing contact or
//! coil keeps the state it last sensed in a hidden BOOL of the POU, `__EDGE_<localId>`, and
//! senses as R_TRIG and F_TRIG do (Table 44): rising `Q := CLK AND NOT M; M := CLK`, falling
//! `Q := NOT CLK AND NOT M; M := NOT CLK`, M starting FALSE; for a contact CLK is its
//! variable and its output is its input AND Q, for a coil CLK is its input and Q is written
//! to its variable.
//!
//! **Blocks** (§8.2.6, §6.6.1.5). A function block is called with the inputs connected to it;
//! one not connected keeps its value (§6.6.3.4.1). Its outputs are read from the instance.
//! EN, when connected, is evaluated outside the POU (§6.6.1.5, Example 2): when it is FALSE
//! the block is not called, its inputs are not set and ENO is FALSE; otherwise ENO is TRUE.
//! A function gives its result as OUT. A value computed from a function that did not run
//! (its EN, or an EN before it, was FALSE) is not written: an output variable fed by it keeps
//! its value, and as power flow it is OFF (§6.6.1.5 rule 4 leaves both to the implementer).
//!
//! **Jumps and returns** (§8.1.6, Table 73). A jump to a label goes on at the first network
//! at or below the label; a return leaves the POU. Each takes effect when the network it is
//! in has been evaluated whole (§8.1.5.1 rule c); in a network with several, the first in
//! evaluation order whose condition is TRUE wins. They are lowered with a hidden DINT,
//! `__JUMP` (0: go on; -1: return; k: to the k-th label), each network guarded by
//! `__JUMP = 0`, each label clearing it, and the whole in a REPEAT when a jump goes back.
//!
//! **Monitoring** (ADR-013 §2.15, the owner's decision of 2026-11-18). Every output that carries
//! power (a contact's, a coil's, a block's BOOL output and ENO, a BOOL variable read) and every
//! junction of links is kept in a hidden BOOL, `__LD_<localId>` or `__LD_<localId>_<pin>`,
//! in every build. Each is assigned where the lowering evaluates the value: at once where it
//! keeps a value (a branch point, a coil), and otherwise by a statement of its own just before
//! the first statement that reads it, where the expression it stands for was evaluated before.
//! Only these hidden variables are written between, so every element reads what it read
//! without them and the program does what it did. An output nothing reads is stored when its
//! network ends. A power read inside an IF (a block's input under its EN) is stored before the
//! IF, so it is the element's power whether or not the block ran; a function's result is
//! stored only where the function runs, and FALSE where it does not. So an element that feeds
//! nothing, or feeds a block whose EN is FALSE, is now evaluated, as §8.1.5.1 rule c says
//! every element is: what it reads is read, and an index out of range there faults where
//! before it was never read. Each network sets
//! `__NET_<localId>` (its first element's) TRUE as it starts; in a body with a jump or a return
//! every flag is set FALSE first in the scan, so a network that did not run reads FALSE.

use crate::graph::{
    self, bool_expr, bool_ty, check_graph, read_body, set_jump, Conn, Edge, Elem, Hidden, Kind,
    Lang, Storage,
};
use crate::read::{GPou, Known};
use crate::{At, Holds, Places, Probe};
use mythos_logic_ir::{Diagnostic, Span};
use mythos_logic_st::ast::{
    self, Arg, BinOp, CallArg, Expr, ExprKind, PouKind, Stmt, TypeSpec, TypeSpecKind,
};
use std::collections::BTreeMap;

/// A Boolean tree of power flow, or a value at its leaf.
#[derive(Debug, Clone)]
enum Tree {
    On,
    Leaf(Expr),
    Not(Box<Tree>),
    And(Vec<Tree>),
    Or(Vec<Tree>),
    /// A call of a function by its formal parameters, each argument a tree.
    Call(String, Vec<(String, Tree, Span)>, Span),
    /// A Boolean or MOD block: its operator over its inputs, left to right.
    Op(BinOp, Vec<Tree>, Span),
    /// The power of an output or of a junction, kept in a hidden variable: the k-th of
    /// [`Lowering::marks`], stored by the first statement that reads it.
    Mark(usize),
}

impl Tree {
    fn and(items: Vec<Tree>) -> Tree {
        let mut flat = Vec::new();
        for t in items {
            match t {
                Tree::On => {}
                Tree::And(inner) => flat.extend(inner),
                t => flat.push(t),
            }
        }
        match flat.len() {
            0 => Tree::On,
            1 => flat.pop().expect("one item"),
            _ => Tree::And(flat),
        }
    }
    fn or(items: Vec<Tree>) -> Tree {
        let mut flat = Vec::new();
        for t in items {
            match t {
                Tree::On => return Tree::On,
                Tree::Or(inner) => flat.extend(inner),
                t => flat.push(t),
            }
        }
        match flat.len() {
            1 => flat.pop().expect("one item"),
            _ => Tree::Or(flat),
        }
    }
    fn not(t: Tree) -> Tree {
        match t {
            Tree::Not(inner) => *inner,
            t => Tree::Not(Box::new(t)),
        }
    }
}

/// A series or a parallel of many, balanced, so that its depth grows with the logarithm of
/// its length (`mythos_logic_ir::MAX_NESTING`).
fn balanced(op: BinOp, mut items: Vec<Expr>, span: Span) -> Expr {
    while items.len() > 1 {
        let mut next = Vec::with_capacity(items.len().div_ceil(2));
        let mut it = items.into_iter();
        while let Some(a) = it.next() {
            match it.next() {
                Some(b) => next.push(Expr {
                    kind: ExprKind::Binary(op, Box::new(a), Box::new(b)),
                    span,
                }),
                None => next.push(a),
            }
        }
        items = next;
    }
    items.pop().expect("at least one item")
}

/// The power of an output or of a junction, kept in a hidden BOOL (ADR-013 §2.15).
#[derive(Debug)]
struct Mark {
    place: ast::Place,
    /// What is stored, until a statement has stored it: the power, and the power under which
    /// it is defined (a function's EN and those before it; FALSE is stored where it is not).
    pending: Option<(Tree, Option<Tree>)>,
}

/// The marks `tree` reads directly, in the order it reads them; not those a mark reads.
fn marks_in(tree: &Tree, out: &mut Vec<usize>) {
    match tree {
        Tree::Mark(m) => out.push(*m),
        Tree::Not(t) => marks_in(t, out),
        Tree::And(items) | Tree::Or(items) | Tree::Op(_, items, _) => {
            items.iter().for_each(|t| marks_in(t, out))
        }
        Tree::Call(_, args, _) => args.iter().for_each(|(_, t, _)| marks_in(t, out)),
        Tree::On | Tree::Leaf(_) => {}
    }
}

fn is_bool(ty: &TypeSpec) -> bool {
    matches!(&ty.kind, TypeSpecKind::Named(n) if n.eq_ignore_ascii_case("BOOL"))
}

/// What an output gives: a value, and the power under which it is defined (`None`: always).
#[derive(Debug, Clone)]
struct Val {
    tree: Tree,
    guard: Option<Tree>,
}

impl Val {
    fn plain(tree: Tree) -> Val {
        Val { tree, guard: None }
    }
    /// As power flow: OFF where it is not defined.
    fn power(self) -> Tree {
        match self.guard {
            Some(g) => Tree::and(vec![g, self.tree]),
            None => self.tree,
        }
    }
}

/// An LD POU with its statements, and the hidden variables they use; or, when its body
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
    let elems = read_body(body, Lang::Ld, p, places, diags);
    if diags.len() == before && check_graph(&elems, Lang::Ld, places, diags) {
        let mut l = Lowering::new(&elems, &tree, known, places);
        let stmts = graph::body(&mut l);
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
    tree.partial_body = true;
    tree
}

/// The state of lowering one body.
struct Lowering<'a> {
    elems: &'a [Elem],
    pou: &'a ast::Pou,
    known: &'a Known,
    places: &'a mut Places,
    index: BTreeMap<u64, usize>,
    /// How many connections take each output (element, output in upper case).
    uses: BTreeMap<(usize, String), usize>,
    /// The value of each output already worked out: kept in a hidden variable, or one that
    /// is the same wherever it is used.
    values: BTreeMap<(usize, String), Val>,
    /// Elements whose statements are emitted.
    done: Vec<bool>,
    stmts: Vec<Stmt>,
    hidden: Hidden,
    diags: Vec<Diagnostic>,
    /// The labels, in order; a jump to the k-th sets `__JUMP` to k + 1.
    labels: Vec<String>,
    /// The instances called by a block of the network being lowered.
    called: Vec<(String, u64)>,
    /// Every power kept in a hidden variable, by [`Tree::Mark`].
    marks: Vec<Mark>,
    /// The power of each junction (element, input), once worked out.
    junctions: BTreeMap<(usize, usize), Tree>,
    /// Each function's call, once worked out; `None` when it was refused.
    functions: BTreeMap<usize, Option<Val>>,
    /// What holds the power of each output, junction and network: what is published.
    published: BTreeMap<(u64, At), Holds>,
}

impl<'a> Lowering<'a> {
    fn new(elems: &'a [Elem], pou: &'a ast::Pou, known: &'a Known, places: &'a mut Places) -> Self {
        let index: BTreeMap<u64, usize> =
            elems.iter().enumerate().map(|(k, e)| (e.id, k)).collect();
        let mut uses: BTreeMap<(usize, String), usize> = BTreeMap::new();
        // The right rail takes nothing: what reaches it is the end of a rung (§8.2.3).
        for e in elems.iter().filter(|e| !matches!(e.kind, Kind::RightRail)) {
            for p in &e.inputs {
                for c in &p.conns {
                    let from = index[&c.from];
                    let out = elems[from].output(c.formal.as_deref()).unwrap_or_default();
                    *uses.entry((from, out)).or_default() += 1;
                }
            }
        }
        Lowering {
            elems,
            pou,
            known,
            places,
            index,
            uses,
            values: BTreeMap::new(),
            done: vec![false; elems.len()],
            stmts: Vec::new(),
            hidden: Hidden::default(),
            diags: Vec::new(),
            labels: Vec::new(),
            called: Vec::new(),
            marks: Vec::new(),
            junctions: BTreeMap::new(),
            functions: BTreeMap::new(),
            published: BTreeMap::new(),
        }
    }

    fn span(&self, k: usize) -> Span {
        self.places.span(self.elems[k].line)
    }

    fn uses(&self, k: usize, out: &str) -> usize {
        self.uses.get(&(k, out.to_string())).copied().unwrap_or(0)
    }

    fn err(&mut self, k: usize, message: String) {
        let d = self.places.diag(self.elems[k].line, message);
        self.diags.push(d);
    }

    /// A hidden variable of the POU, declared once; the place it is.
    fn hidden_var(&mut self, k: usize, name: String, ty: TypeSpec) -> ast::Place {
        let span = self.span(k);
        self.hidden.var(name, ty, span)
    }

    fn load(place: &ast::Place) -> Tree {
        Tree::Leaf(Expr {
            kind: ExprKind::Var(place.clone()),
            span: place.span,
        })
    }

    /// The tree as an expression, each power it reads stored first: every [`Tree::Mark`] in
    /// it not stored yet is assigned to its variable by a statement of its own, emitted now,
    /// so before the statement this expression goes into, and read from there. The statements
    /// are emitted where the expression is built, so a power read inside an IF is stored
    /// before that IF, whatever its condition.
    fn expr(&mut self, tree: Tree, span: Span) -> Expr {
        match tree {
            Tree::On => bool_expr(true, span),
            Tree::Leaf(e) => e,
            Tree::Not(t) => Expr {
                kind: ExprKind::Not(Box::new(self.expr(*t, span))),
                span,
            },
            Tree::And(items) => {
                let items = items.into_iter().map(|t| self.expr(t, span)).collect();
                balanced(BinOp::And, items, span)
            }
            Tree::Or(items) => {
                let items = items.into_iter().map(|t| self.expr(t, span)).collect();
                balanced(BinOp::Or, items, span)
            }
            Tree::Call(name, args, at) => {
                let args = args
                    .into_iter()
                    .map(|(formal, t, pin_span)| CallArg {
                        name: Some((formal, pin_span)),
                        value: self.expr(t, pin_span),
                    })
                    .collect();
                Expr {
                    kind: ExprKind::Call(name, args),
                    span: at,
                }
            }
            Tree::Op(op, items, at) => {
                let mut it = items.into_iter();
                let first = it.next().map(|t| self.expr(t, at));
                let rest: Vec<Expr> = it.map(|t| self.expr(t, at)).collect();
                let first = first.unwrap_or_else(|| bool_expr(false, at));
                rest.into_iter().fold(first, |a, b| Expr {
                    kind: ExprKind::Binary(op, Box::new(a), Box::new(b)),
                    span: at,
                })
            }
            Tree::Mark(m) => {
                self.store_mark(m);
                let place = &self.marks[m].place;
                Expr {
                    kind: ExprKind::Var(place.clone()),
                    span: place.span,
                }
            }
        }
    }

    /// Emit the statement storing mark `m`, after those of every mark it reads that is not
    /// stored yet. Without recursion: a series of contacts is a chain of marks as long as the
    /// series.
    fn store_mark(&mut self, m: usize) {
        let mut stack = vec![(m, false)];
        while let Some((m, ready)) = stack.pop() {
            if self.marks[m].pending.is_none() {
                continue;
            }
            if !ready {
                stack.push((m, true));
                let mut read = Vec::new();
                if let Some((value, guard)) = &self.marks[m].pending {
                    marks_in(value, &mut read);
                    if let Some(g) = guard {
                        marks_in(g, &mut read);
                    }
                }
                // The first read is stored first.
                stack.extend(read.into_iter().rev().map(|r| (r, false)));
                continue;
            }
            let Some((value, guard)) = self.marks[m].pending.take() else {
                continue;
            };
            let place = self.marks[m].place.clone();
            let at = place.span;
            let value = self.expr(value, at);
            let assign = Stmt::Assign(place.clone(), value);
            let stmt = match guard {
                None => assign,
                Some(g) => {
                    let g = self.expr(g, at);
                    Stmt::If(
                        vec![(g, vec![assign])],
                        Some(vec![Stmt::Assign(place, bool_expr(false, at))]),
                    )
                }
            };
            self.stmts.push(stmt);
        }
    }

    /// Store now every power `tree` reads that is not stored yet.
    fn settle(&mut self, tree: Tree, span: Span) {
        let _ = self.expr(tree, span);
    }

    fn assign(&mut self, place: &ast::Place, tree: Tree) -> Stmt {
        let span = place.span;
        Stmt::Assign(place.clone(), self.expr(tree, span))
    }

    fn when(&mut self, cond: Tree, span: Span, then: Vec<Stmt>) -> Vec<Stmt> {
        match cond {
            Tree::On => then,
            c => vec![Stmt::If(vec![(self.expr(c, span), then)], None)],
        }
    }

    fn publish(&mut self, k: usize, at: At, holds: Holds) {
        self.published.insert((self.elems[k].id, at), holds);
    }

    /// The power `tree` of `at` of element `k`, kept in the hidden BOOL `name`, which is
    /// published as holding it (ADR-013 §2.15). Stored now (`now`), where the lowering keeps a
    /// value at once, or else by the first statement that reads it, which is where the
    /// lowering evaluates a value it does not keep: either way when the element is evaluated,
    /// so storing it changes nothing the program does. With a `guard`, FALSE is stored where
    /// the guard is FALSE. ON is published as ON and stored nowhere.
    fn store(
        &mut self,
        k: usize,
        at: At,
        name: String,
        tree: Tree,
        guard: Option<Tree>,
        now: bool,
    ) -> Tree {
        if guard.is_none() && matches!(tree, Tree::On) {
            self.publish(k, at, Holds::On);
            return Tree::On;
        }
        let span = self.span(k);
        let place = self.hidden_var(k, name, bool_ty(span));
        self.publish(
            k,
            at,
            Holds::Variable {
                path: place.text(),
                negated: false,
            },
        );
        self.marks.push(Mark {
            place,
            pending: Some((tree, guard)),
        });
        let mark = Tree::Mark(self.marks.len() - 1);
        if now {
            self.settle(mark.clone(), span);
        }
        mark
    }

    /// The name of the variable keeping the power of the one output of element `k`, or a
    /// function's OUT: `__LD_<localId>`; of a block's other output or input `formal`:
    /// `__LD_<localId>_<formal>`; of the one input of another element: `__LD_<localId>_IN`.
    fn power_name(&self, k: usize, formal: &str) -> String {
        let id = self.elems[k].id;
        if formal.is_empty() {
            format!("__LD_{id}")
        } else {
            format!("__LD_{id}_{formal}")
        }
    }

    /// The power reaching input `pin` of element `k`: the OR of what is connected to it.
    /// Where several links join (a vertical link, §8.2.3), the OR is a junction of its own,
    /// kept in `__LD_<localId>_IN`, or `__LD_<localId>_<formal>` at a block's input.
    fn power_in(&mut self, k: usize, pin: usize) -> Tree {
        if let Some(t) = self.junctions.get(&(k, pin)) {
            return t.clone();
        }
        let p = &self.elems[k].inputs[pin];
        let conns = p.conns.clone();
        let mut items = Vec::new();
        for c in &conns {
            items.push(self.value_of(c).power());
        }
        if let Some(e) = self.elems[k].inputs[pin].expr.clone() {
            items.push(Tree::Leaf(e));
        }
        if items.is_empty() {
            // An input with nothing connected: the checks before refuse it where it is needed.
            let span = self.span(k);
            return Tree::Leaf(bool_expr(false, span));
        }
        if items.len() == 1 || matches!(self.elems[k].kind, Kind::RightRail) {
            return Tree::or(items);
        }
        let formal = self.elems[k].inputs[pin].formal.clone();
        let name = self.power_name(k, formal.as_deref().unwrap_or("IN"));
        let at = At::Input(formal.unwrap_or_default());
        let t = self.store(k, at, name, Tree::or(items), None, false);
        self.junctions.insert((k, pin), t.clone());
        t
    }

    /// What reaches input `pin` of element `k` as a value: one connection's value, the OR of
    /// several (a vertical link), or the expression written there; `None` when nothing does.
    fn value_in(&mut self, k: usize, pin: usize) -> Option<Val> {
        let p = self.elems[k].inputs[pin].clone();
        let v = match (p.conns.as_slice(), p.expr) {
            ([], None) => return None,
            ([], Some(e)) => Val::plain(Tree::Leaf(e)),
            ([c], None) => self.value_of(c),
            _ => Val::plain(self.power_in(k, pin)),
        };
        Some(if p.negated {
            Val::plain(Tree::not(v.power()))
        } else {
            v
        })
    }

    fn value_of(&mut self, c: &Conn) -> Val {
        let k = self.index[&c.from];
        let out = self.elems[k]
            .output(c.formal.as_deref())
            .unwrap_or_default();
        self.output(k, &out)
    }

    /// The value of output `out` of element `k`, its statements emitted first if it has any.
    fn output(&mut self, k: usize, out: &str) -> Val {
        if let Some(v) = self.values.get(&(k, out.to_string())) {
            return v.clone();
        }
        self.emit(k);
        if let Some(v) = self.values.get(&(k, out.to_string())) {
            return v.clone();
        }
        let span = self.span(k);
        let here = At::Output(String::new());
        let v = match self.elems[k].kind.clone() {
            Kind::LeftRail => {
                self.publish(k, here, Holds::On);
                Val::plain(Tree::On)
            }
            Kind::Contact { var, negated, .. } => {
                self.inputs_first(k);
                let input = self.power_in(k, 0);
                let var = Tree::Leaf(var);
                let tree = Tree::and(vec![input, if negated { Tree::not(var) } else { var }]);
                // Kept at once when more than one input reads it, as a branch point is;
                // otherwise where the one input that reads it does.
                let now = self.uses(k, "") > 1;
                let name = self.power_name(k, "");
                Val::plain(self.store(k, here, name, tree, None, now))
            }
            Kind::InVar { expr, negated } => {
                let literal = matches!(
                    expr.kind,
                    ExprKind::Int(_)
                        | ExprKind::Real(_)
                        | ExprKind::Bool(_)
                        | ExprKind::Time(_)
                        | ExprKind::Str(_)
                );
                let tree = Tree::Leaf(expr.clone());
                let tree = if negated { Tree::not(tree) } else { tree };
                let ty = self.type_of_read(&expr);
                let boolean = negated || ty.as_ref().is_some_and(is_bool);
                let name = self.power_name(k, "");
                if literal {
                    Val::plain(tree)
                } else if boolean {
                    // A BOOL read is power: kept, at once when more than one input reads it.
                    let now = self.uses(k, "") > 1;
                    Val::plain(self.store(k, here, name, tree, None, now))
                } else if self.uses(k, "") > 1 {
                    match ty {
                        Some(ty) => {
                            let place = self.hidden_var(k, name, ty);
                            let s = self.assign(&place, tree);
                            self.stmts.push(s);
                            Val::plain(Self::load(&place))
                        }
                        None => {
                            self.err(
                                k,
                                format!(
                                    "input variable {} feeds {} inputs, and its type is not known here: name one variable, or give each input its own input variable",
                                    self.elems[k].id,
                                    self.uses(k, "")
                                ),
                            );
                            Val::plain(tree)
                        }
                    }
                } else {
                    Val::plain(tree)
                }
            }
            // A call that was refused: what was wrong has been said.
            Kind::Block {
                instance: Some(_), ..
            } => Val::plain(Tree::Leaf(bool_expr(false, span))),
            Kind::Block { .. } => self.function_output(k, out),
            _ => Val::plain(Tree::Leaf(bool_expr(false, span))),
        };
        self.values.insert((k, out.to_string()), v.clone());
        v
    }

    /// Whether working out the output of element `k` only works out its inputs, each in a
    /// fixed order, and then it: a contact that senses no edge, and a function evaluated
    /// where it is read (its result taken once, its ENO not taken).
    fn reads_inputs_only(&self, k: usize) -> bool {
        match &self.elems[k].kind {
            Kind::Contact {
                edge: Edge::None, ..
            } => true,
            // One showing another output is refused before its inputs are read.
            Kind::Block {
                type_name,
                instance: None,
                outputs,
            } => {
                !self.known.is_block(&type_name.to_ascii_uppercase())
                    && outputs.iter().all(|(o, _)| o == "OUT" || o == "ENO")
                    && self.uses(k, "OUT") <= 1
                    && self.uses(k, "ENO") == 0
            }
            _ => false,
        }
    }

    /// The connections into element `k`, in the order working out its output reads them: a
    /// function's EN first, then each other input (§6.6.1.5).
    fn conns_in_order(&self, k: usize) -> Vec<Conn> {
        let inputs = &self.elems[k].inputs;
        let en = inputs.iter().filter(|p| p.formal.as_deref() == Some("EN"));
        let rest = inputs.iter().filter(|p| p.formal.as_deref() != Some("EN"));
        en.chain(rest)
            .flat_map(|p| p.conns.iter().cloned())
            .collect()
    }

    /// Work out what element `k` reads before it, one element at a time and the first first,
    /// in the order [`Lowering::output`] would reach them through its inputs, when every
    /// element before it reads its inputs only ([`Lowering::reads_inputs_only`]), is the left
    /// rail, or is worked out already: so that a series of contacts or of functions takes no
    /// stack for its length. Anything else before it is left to `output`, which works it out
    /// as it comes.
    fn inputs_first(&mut self, k: usize) {
        let mut order: Vec<(usize, String)> = Vec::new();
        let mut seen = vec![false; self.elems.len()];
        // Each element on the way, its connections, and how many of them are followed.
        let mut trail = vec![(k, String::new(), self.conns_in_order(k), 0usize)];
        seen[k] = true;
        while let Some(top) = trail.last_mut() {
            let Some(c) = top.2.get(top.3).cloned() else {
                let (at, out, _, _) = trail.pop().expect("a step");
                order.push((at, out));
                continue;
            };
            top.3 += 1;
            let from = self.index[&c.from];
            let out = self.elems[from]
                .output(c.formal.as_deref())
                .unwrap_or_default();
            if seen[from] || self.values.contains_key(&(from, out.clone())) {
                continue;
            }
            if matches!(self.elems[from].kind, Kind::LeftRail) {
                continue;
            }
            if !self.reads_inputs_only(from) {
                return;
            }
            seen[from] = true;
            let conns = self.conns_in_order(from);
            trail.push((from, out, conns, 0));
        }
        order.pop();
        for (at, out) in order {
            self.output(at, &out);
        }
    }

    /// The declared type of what `expr` reads, when it is a variable of the POU by its name.
    fn type_of_read(&self, expr: &Expr) -> Option<TypeSpec> {
        match &expr.kind {
            ExprKind::Var(p) if p.path.is_empty() => self
                .pou
                .vars
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(&p.name))
                .map(|v| v.ty.clone()),
            _ => None,
        }
    }

    /// Emit the statements of element `k`, once. Its outputs are recorded in `values` when
    /// they are kept; otherwise [`Lowering::output`] works them out where they are used.
    fn emit(&mut self, k: usize) {
        if self.done[k] {
            return;
        }
        self.done[k] = true;
        let span = self.span(k);
        let id = self.elems[k].id;
        let here = At::Output(String::new());
        match self.elems[k].kind.clone() {
            Kind::Contact { var, edge, .. } if edge != Edge::None => {
                if self.pou.kind == PouKind::Function {
                    self.err(k, format!("contact {id} senses an edge, which needs a memory; a function keeps nothing from one call to the next (IEC 61131-3 ed. 3 §6.6.2.1)"));
                    return;
                }
                let input = self.power_in(k, 0);
                let memory = self.hidden_var(k, format!("__EDGE_{id}"), bool_ty(span));
                let clk = match edge {
                    Edge::Rising => Tree::Leaf(var),
                    _ => Tree::not(Tree::Leaf(var)),
                };
                let q = Tree::and(vec![input, clk.clone(), Tree::not(Self::load(&memory))]);
                let name = self.power_name(k, "");
                let out = self.store(k, here, name, q, None, true);
                let s = self.assign(&memory, clk);
                self.stmts.push(s);
                self.values.insert((k, String::new()), Val::plain(out));
            }
            Kind::Coil {
                var,
                negated,
                storage,
                edge,
            } => {
                let input = self.power_in(k, 0);
                // A coil passes on the power that reaches it (Table 76): kept, read by the
                // coil's own statement and by what follows it.
                let name = self.power_name(k, "");
                let input = self.store(k, here, name, input, None, true);
                self.values
                    .insert((k, String::new()), Val::plain(input.clone()));
                let s = match (negated, storage, edge) {
                    (_, Storage::Set, _) => {
                        let set = vec![Stmt::Assign(var.clone(), bool_expr(true, span))];
                        self.when(input, span, set)
                    }
                    (_, Storage::Reset, _) => {
                        let reset = vec![Stmt::Assign(var.clone(), bool_expr(false, span))];
                        self.when(input, span, reset)
                    }
                    (_, _, Edge::Rising | Edge::Falling) => {
                        if self.pou.kind == PouKind::Function {
                            self.err(k, format!("coil {id} senses an edge, which needs a memory; a function keeps nothing from one call to the next (IEC 61131-3 ed. 3 §6.6.2.1)"));
                            return;
                        }
                        let memory = self.hidden_var(k, format!("__EDGE_{id}"), bool_ty(span));
                        let clk = if edge == Edge::Rising {
                            input
                        } else {
                            Tree::not(input)
                        };
                        vec![
                            self.assign(
                                &var,
                                Tree::and(vec![clk.clone(), Tree::not(Self::load(&memory))]),
                            ),
                            self.assign(&memory, clk),
                        ]
                    }
                    (true, _, _) => vec![self.assign(&var, Tree::not(input))],
                    (false, _, _) => vec![self.assign(&var, input)],
                };
                self.stmts.extend(s);
            }
            Kind::OutVar { target, negated } => {
                let Some(v) = self.value_in(k, 0) else { return };
                let v = if negated {
                    Val::plain(Tree::not(v.power()))
                } else {
                    v
                };
                let s = vec![self.assign(&target, v.tree)];
                let s = match v.guard {
                    Some(g) => self.when(g, span, s),
                    None => s,
                };
                self.stmts.extend(s);
            }
            Kind::InOutVar {
                target,
                negated_in,
                negated_out,
            } => {
                if let Some(v) = self.value_in(k, 0) {
                    let v = if negated_in {
                        Val::plain(Tree::not(v.power()))
                    } else {
                        v
                    };
                    let s = vec![self.assign(&target, v.tree)];
                    let s = match v.guard {
                        Some(g) => self.when(g, span, s),
                        None => s,
                    };
                    self.stmts.extend(s);
                }
                let read = Self::load(&target);
                let read = if negated_out { Tree::not(read) } else { read };
                let ty = self.type_of_read(&Expr {
                    kind: ExprKind::Var(target.clone()),
                    span,
                });
                let name = self.power_name(k, "");
                // Read after it is written, wherever it is used: kept when used more than once,
                // and kept as power when it is a BOOL.
                let read = if negated_out || ty.as_ref().is_some_and(is_bool) {
                    let now = self.uses(k, "") > 1 && !negated_out;
                    self.store(k, here, name, read, None, now)
                } else if self.uses(k, "") > 1 {
                    match ty {
                        Some(ty) => {
                            let place = self.hidden_var(k, name, ty);
                            let s = self.assign(&place, read);
                            self.stmts.push(s);
                            Self::load(&place)
                        }
                        None => read,
                    }
                } else {
                    read
                };
                self.values.insert((k, String::new()), Val::plain(read));
            }
            Kind::Block {
                type_name,
                instance: Some(instance),
                outputs,
            } => {
                self.call_block(k, &type_name, &instance, &outputs);
            }
            Kind::Block {
                type_name,
                instance: None,
                ..
            } => {
                let upper = type_name.to_ascii_uppercase();
                if self.known.is_block(&upper) {
                    self.err(k, format!("block {id} calls the function block type {type_name} without an instance: give it an instanceName"));
                    return;
                }
                // A function whose result more than one input takes, or whose ENO is used:
                // worked out once, here.
                if self.uses(k, "OUT") > 1 || self.uses(k, "ENO") > 0 {
                    self.keep_function(k, &type_name);
                }
            }
            Kind::Jump(label) => {
                let cond = self.power_in(k, 0);
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
                let s = self.when(cond, span, s);
                self.stmts.extend(s);
            }
            Kind::Return => {
                let cond = self.power_in(k, 0);
                let jump = self.hidden.jump(span);
                let s = set_jump(jump, -1, span);
                let s = self.when(cond, span, s);
                self.stmts.extend(s);
            }
            Kind::Contact { .. }
            | Kind::InVar { .. }
            | Kind::LeftRail
            | Kind::RightRail
            | Kind::Label(_)
            | Kind::Comment
            | Kind::Connector(_)
            | Kind::Continuation(_) => {}
        }
    }

    /// Store, once the statements of the network are emitted, every power of element `k`
    /// that nothing read: an output connected to nothing but the right rail, a junction into
    /// a function whose result nothing takes. Each element is evaluated (§8.1.5.1 rule c),
    /// and these, read by nothing, are evaluated last.
    fn settle_element(&mut self, k: usize) {
        let span = self.span(k);
        match self.elems[k].kind.clone() {
            Kind::Contact { .. } | Kind::InVar { .. } | Kind::InOutVar { .. } => {
                let v = self.output(k, "");
                self.settle(v.tree, span);
            }
            Kind::Block {
                instance: Some(_),
                outputs,
                ..
            } => {
                for (formal, _) in outputs {
                    if let Some(v) = self.values.get(&(k, formal)).cloned() {
                        self.settle(v.tree, span);
                    }
                }
            }
            Kind::Block {
                type_name,
                instance: None,
                outputs,
            } => {
                if self.known.is_block(&type_name.to_ascii_uppercase()) {
                    return;
                }
                let boolean = self
                    .result_type(&type_name, span)
                    .is_some_and(|t| is_bool(&t));
                for (formal, _) in outputs {
                    if formal == "ENO" || (formal == "OUT" && boolean) {
                        let v = self.output(k, &formal);
                        self.settle(v.tree, span);
                    }
                }
            }
            _ => {}
        }
        if matches!(self.elems[k].kind, Kind::RightRail) {
            return;
        }
        for pin in 0..self.elems[k].inputs.len() {
            let p = &self.elems[k].inputs[pin];
            if p.conns.len() + p.expr.is_some() as usize > 1 {
                let t = self.power_in(k, pin);
                self.settle(t, span);
            }
        }
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

    /// The power at EN of block `k`, when it shows one that is connected.
    fn enable(&mut self, k: usize) -> Option<Tree> {
        let pin = self.elems[k]
            .inputs
            .iter()
            .position(|p| p.formal.as_deref() == Some("EN"))?;
        Some(self.value_in(k, pin)?.power())
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
        let shows_eno = outputs.iter().any(|(o, _)| o == "ENO");
        let mut enable = self.enable(k);
        if shows_eno {
            // ENO is the power at EN (§6.6.1.5): kept at once when something takes it, as
            // before; otherwise where the call's condition reads it.
            if let Some(en) = enable.take() {
                let name = self.power_name(k, "ENO");
                let now = self.uses(k, "ENO") > 0;
                enable = Some(self.store(k, At::Output("ENO".into()), name, en, None, now));
            }
        }
        let inputs = self.block_inputs(k);
        let mut inner = Vec::new();
        let mut args = Vec::new();
        for (formal, v, pin_span) in inputs {
            let value = self.expr(v.tree, pin_span);
            match v.guard {
                // Set apart from the call, only where it is defined.
                Some(g) => {
                    let mut member = instance.clone();
                    member.path.push(ast::Selector::Member(formal, pin_span));
                    let s = self.when(g, pin_span, vec![Stmt::Assign(member, value)]);
                    inner.extend(s);
                }
                None => args.push(Arg::In(formal, value, pin_span)),
            }
        }
        inner.push(Stmt::Call(instance.clone(), args));
        let s = match &enable {
            Some(en) => self.when(en.clone(), span, inner),
            None => inner,
        };
        self.stmts.extend(s);
        for (formal, negated) in outputs {
            let v = if formal == "ENO" {
                match &enable {
                    Some(en) => en.clone(),
                    None => {
                        self.publish(k, At::Output("ENO".into()), Holds::On);
                        Tree::On
                    }
                }
            } else {
                let mut member = instance.clone();
                member
                    .path
                    .push(ast::Selector::Member(formal.clone(), span));
                let read = Self::load(&member);
                let read = if *negated { Tree::not(read) } else { read };
                // A BOOL output is power: kept where it is first read after the call.
                if *negated || self.known.output_is_bool(&upper, formal) {
                    let name = self.power_name(k, formal);
                    self.store(k, At::Output(formal.clone()), name, read, None, false)
                } else {
                    read
                }
            };
            self.values.insert((k, formal.clone()), Val::plain(v));
        }
    }

    /// The call of function `k` as a tree, with the power under which it runs; worked out
    /// once.
    fn function_call(&mut self, k: usize) -> Option<Val> {
        if let Some(v) = self.functions.get(&k) {
            return v.clone();
        }
        let v = self.function_call_once(k);
        self.functions.insert(k, v.clone());
        v
    }

    fn function_call_once(&mut self, k: usize) -> Option<Val> {
        let span = self.span(k);
        let id = self.elems[k].id;
        let Kind::Block {
            type_name, outputs, ..
        } = self.elems[k].kind.clone()
        else {
            return None;
        };
        if let Some((o, _)) = outputs.iter().find(|(o, _)| o != "OUT" && o != "ENO") {
            self.err(k, format!("block {id} calls the function {type_name}, which gives one output, OUT; it shows {o}"));
            return None;
        }
        let enable = self.enable(k);
        let inputs = self.block_inputs(k);
        let mut guards: Vec<Tree> = enable.into_iter().collect();
        let mut values = Vec::new();
        for (formal, v, pin_span) in inputs {
            if let Some(g) = v.guard {
                guards.push(g);
            }
            values.push((formal, v.tree, pin_span));
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
                self.err(
                    k,
                    format!(
                        "block {id} ({type_name}) takes {} inputs; it has {n} connected",
                        if op == BinOp::Mod {
                            "two"
                        } else {
                            "two or more"
                        }
                    ),
                );
                return None;
            }
            Tree::Op(op, values.into_iter().map(|(_, t, _)| t).collect(), span)
        } else if upper == "NOT" {
            if values.len() != 1 {
                self.err(
                    k,
                    format!(
                        "block {id} (NOT) takes one input, IN; it has {} connected",
                        values.len()
                    ),
                );
                return None;
            }
            let (_, t, _) = values.pop().expect("one input");
            Tree::Not(Box::new(t))
        } else {
            Tree::Call(type_name.clone(), values, span)
        };
        let mut guard = (!guards.is_empty()).then(|| Tree::and(guards));
        if outputs.iter().any(|(o, _)| o == "ENO") {
            // ENO is the power under which the function runs (§6.6.1.5).
            guard = match guard {
                Some(g) => {
                    let name = self.power_name(k, "ENO");
                    Some(self.store(k, At::Output("ENO".into()), name, g, None, false))
                }
                None => {
                    self.publish(k, At::Output("ENO".into()), Holds::On);
                    None
                }
            };
        }
        Some(Val { tree: call, guard })
    }

    /// Whether function `k`'s result is a BOOL, as far as it is known before checking.
    fn bool_result(&self, k: usize) -> bool {
        match &self.elems[k].kind {
            Kind::Block { type_name, .. } => self
                .result_type(type_name, self.span(k))
                .is_some_and(|t| is_bool(&t)),
            _ => false,
        }
    }

    fn function_output(&mut self, k: usize, out: &str) -> Val {
        let span = self.span(k);
        self.inputs_first(k);
        let Some(v) = self.function_call(k) else {
            return Val::plain(Tree::Leaf(bool_expr(false, span)));
        };
        if out == "ENO" {
            return Val::plain(v.guard.unwrap_or(Tree::On));
        }
        if self.bool_result(k) {
            // A BOOL result is power: kept where it is first read, FALSE where the function
            // does not run.
            let name = self.power_name(k, "");
            let tree = self.store(
                k,
                At::Output(out.to_string()),
                name,
                v.tree,
                v.guard.clone(),
                false,
            );
            return Val {
                tree,
                guard: v.guard,
            };
        }
        v
    }

    /// The type of the result of function `type_name`, where it is known without checking:
    /// a function of the project, a comparison or Boolean operator, a conversion.
    fn result_type(&self, type_name: &str, span: Span) -> Option<TypeSpec> {
        let upper = type_name.to_ascii_uppercase();
        if let Some(t) = self.known.result_of(&upper) {
            return Some(t.clone());
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

    /// Function `k` worked out once, its result and ENO kept in hidden variables.
    fn keep_function(&mut self, k: usize, type_name: &str) {
        let span = self.span(k);
        let id = self.elems[k].id;
        let Some(v) = self.function_call(k) else {
            return;
        };
        // The power under which it runs is kept at once in `__LD_<localId>_ENO`.
        let guard = match v.guard {
            Some(g @ Tree::Mark(_)) => {
                self.settle(g.clone(), span);
                Some(g)
            }
            Some(g) => {
                let name = self.power_name(k, "ENO");
                Some(self.store(k, At::Output("ENO".into()), name, g, None, true))
            }
            None => None,
        };
        self.values.insert(
            (k, "ENO".into()),
            Val::plain(guard.clone().unwrap_or(Tree::On)),
        );
        let boolean = self.bool_result(k);
        if self.uses(k, "OUT") <= 1 {
            let tree = if boolean {
                let name = self.power_name(k, "");
                self.store(
                    k,
                    At::Output("OUT".into()),
                    name,
                    v.tree,
                    guard.clone(),
                    false,
                )
            } else {
                v.tree
            };
            self.values.insert((k, "OUT".into()), Val { tree, guard });
            return;
        }
        let Some(ty) = self.result_type(type_name, span) else {
            self.err(
                k,
                format!(
                    "the output OUT of block {id} ({type_name}) feeds {} inputs, and its type is not known here: connect it to one input, or write it to a variable and read that",
                    self.uses(k, "OUT")
                ),
            );
            return;
        };
        let name = self.power_name(k, "");
        if boolean {
            let tree = self.store(
                k,
                At::Output("OUT".into()),
                name,
                v.tree,
                guard.clone(),
                true,
            );
            self.values.insert((k, "OUT".into()), Val { tree, guard });
            return;
        }
        let place = self.hidden_var(k, name, ty);
        let s = vec![self.assign(&place, v.tree)];
        let s = match &guard {
            Some(g) => self.when(g.clone(), span, s),
            None => s,
        };
        self.stmts.extend(s);
        self.values.insert(
            (k, "OUT".into()),
            Val {
                tree: Self::load(&place),
                guard,
            },
        );
    }

    /// What holds the power of every output, junction and network of the body: each a
    /// hidden variable assigned when it is evaluated, or ON (ADR-013 §2.15).
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
    /// Each element emitted in the order of its position, after what it reads (§8.1.5.1 rule
    /// a); then every power nothing read (rule c).
    fn network(&mut self, members: &[usize]) -> Vec<Stmt> {
        self.called.clear();
        self.stmts.clear();
        for &k in members {
            self.emit(k);
        }
        for &k in members {
            self.settle_element(k);
        }
        std::mem::take(&mut self.stmts)
    }
}
