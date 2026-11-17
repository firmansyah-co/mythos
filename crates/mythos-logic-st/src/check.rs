//! From the syntax tree to the IR: resolve every name, type every expression, and refuse what
//! IEC 61131-3 does not allow or this compiler does not support yet. Nothing unsupported is
//! compiled into something else: it is a diagnostic.
//!
//! The order of a source does not matter: every type and POU is known by name before any
//! declaration is read, and every declaration before any body.

use crate::ast::{self, BinOp, CaseLabel, ExprKind as A};
use mythos_logic_ir::*;

pub fn check(src: &ast::Source) -> Result<Unit, Vec<Diagnostic>> {
    let mut c = Checker {
        unit: Unit::new(),
        names: Vec::new(),
        aliases: vec![Alias::Unread; src.types.len()],
        alias_specs: src
            .types
            .iter()
            .map(|t| match &t.kind {
                ast::TypeDeclKind::Alias(spec) => Some(spec.clone()),
                ast::TypeDeclKind::Struct(_) => None,
            })
            .collect(),
        struct_of: Vec::new(),
        pou_spans: src.pous.iter().map(|p| p.span).collect(),
        calls: Vec::new(),
        diags: Vec::new(),
        cur: 0,
        loops: 0,
        result_assigned: false,
    };
    c.names(src);
    c.declarations(src);
    let contained = c.containment();
    for (i, p) in src.pous.iter().enumerate() {
        c.cur = i;
        c.loops = 0;
        c.result_assigned = false;
        let body = c.stmts(&p.body);
        c.unit.pous[i].body = body;
        // IEC 61131-3 ed. 3 §7.3.3.2.3: a POU with a result assigns it at least once.
        if p.kind == ast::PouKind::Function && !c.result_assigned {
            c.err(
                p.span,
                format!(
                    "the function {} never assigns its result: write {} := …;",
                    p.name, p.name
                ),
            );
        }
    }
    if !contained {
        c.recursion();
    }
    if c.diags.is_empty() {
        Ok(c.unit)
    } else {
        Err(c.diags)
    }
}

/// What a name declared at the top of a source is.
#[derive(Clone, Copy)]
enum Named {
    Struct(usize),
    /// A name for an array type: an index into the source's type declarations.
    Alias(usize),
    Pou(usize),
}

/// How far a named array type has been worked out.
#[derive(Clone, Copy)]
enum Alias {
    Unread,
    Reading,
    Known(Option<Ty>),
}

/// How a function block's member is used from outside.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Input,
    Output,
    Internal,
}

/// A place a program named, resolved.
struct Target {
    place: Place,
    ty: Ty,
    /// Why it may not be written, when it may not.
    readonly: Option<String>,
    /// Why it may not be read, when it may not.
    unreadable: Option<String>,
    /// It lies inside a function block instance: named through the instance, from outside.
    inside_instance: bool,
}

struct Checker {
    unit: Unit,
    /// Every type and POU of the source, by its name in upper case.
    names: Vec<(String, Named)>,
    aliases: Vec<Alias>,
    /// For each type declaration of the source: the array type it names, if it names one.
    alias_specs: Vec<Option<ast::TypeSpec>>,
    /// For each type declaration of the source: its structure in the unit, if it is one.
    struct_of: Vec<Option<usize>>,
    pou_spans: Vec<Span>,
    /// Every function call: (the POU it is in, the function called, where).
    calls: Vec<(usize, usize, Span)>,
    diags: Vec<Diagnostic>,
    /// The POU whose declarations or body are being read.
    cur: usize,
    loops: usize,
    result_assigned: bool,
}

/// A literal's value before it knows its type.
fn fits(ty: Ty, v: i128) -> bool {
    ty.int_range().is_some_and(|(lo, hi)| v >= lo && v <= hi)
}

/// `A calls B, which calls A`, from the names round a cycle.
fn chain(names: &[String], verb: &str) -> String {
    let Some((first, rest)) = names.split_first() else {
        return String::new();
    };
    let Some((second, more)) = rest.split_first() else {
        return format!("{first} {verb} itself");
    };
    let mut s = format!("{first} {verb} {second}");
    for n in more.iter().chain(std::iter::once(first)) {
        s.push_str(&format!(", which {verb} {n}"));
    }
    s
}

/// The most elements an array may have. A backend's and a target's limits on memory are far
/// below it; this one only keeps the arithmetic on sizes inside 32 bits.
const MAX_ELEMENTS: i128 = u32::MAX as i128;

impl Checker {
    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::at(span, msg));
    }

    /// A type's name as the program writes it.
    fn n(&self, ty: Ty) -> String {
        self.unit.ty_name(ty)
    }

    fn named(&self, name: &str) -> Option<Named> {
        let upper = name.to_ascii_uppercase();
        self.names
            .iter()
            .find(|(n, _)| *n == upper)
            .map(|(_, x)| *x)
    }

    /// The function named `name`, if there is one.
    fn function(&self, name: &str) -> Option<usize> {
        match self.named(name) {
            Some(Named::Pou(p)) if matches!(self.unit.pous[p].kind, PouKind::Function { .. }) => {
                Some(p)
            }
            _ => None,
        }
    }

    /// First pass: every type and POU by name, so that a later one can be used earlier.
    fn names(&mut self, src: &ast::Source) {
        fn taken(c: &mut Checker, name: &str, span: Span, what: Named) {
            let upper = name.to_ascii_uppercase();
            let conversion = upper
                .split_once("_TO_")
                .is_some_and(|(a, b)| Ty::from_name(a).is_some() && Ty::from_name(b).is_some());
            if Ty::from_name(&upper).is_some() || conversion {
                c.err(
                    span,
                    format!("{name} is a name of the standard; choose another"),
                );
            } else if c.names.iter().any(|(n, _)| *n == upper) {
                c.err(span, format!("{name} is declared twice"));
            } else {
                c.names.push((upper, what));
            }
        }
        for (i, t) in src.types.iter().enumerate() {
            let what = match t.kind {
                ast::TypeDeclKind::Struct(_) => {
                    self.unit.structs.push(StructTy {
                        name: t.name.clone(),
                        fields: Vec::new(),
                    });
                    self.struct_of.push(Some(self.unit.structs.len() - 1));
                    Named::Struct(self.unit.structs.len() - 1)
                }
                ast::TypeDeclKind::Alias(_) => {
                    self.struct_of.push(None);
                    Named::Alias(i)
                }
            };
            taken(self, &t.name, t.span, what);
        }
        let mut programs = 0;
        for (i, p) in src.pous.iter().enumerate() {
            let kind = match p.kind {
                ast::PouKind::Program => {
                    programs += 1;
                    if programs == 1 {
                        self.unit.entry = i;
                    } else {
                        self.err(
                            p.span,
                            format!(
                                "{} is a second PROGRAM; a source has one, where a scan starts",
                                p.name
                            ),
                        );
                    }
                    PouKind::Program
                }
                ast::PouKind::FunctionBlock => PouKind::FunctionBlock,
                ast::PouKind::Function => PouKind::Function { result: 0 },
            };
            self.unit.pous.push(Pou {
                name: p.name.clone(),
                kind,
                vars: Vec::new(),
                body: Vec::new(),
            });
            taken(self, &p.name, p.span, Named::Pou(i));
        }
        if programs == 0 {
            self.diags.push(Diagnostic::whole(
                "the source has no PROGRAM; it needs one, where a scan starts",
            ));
        }
    }

    /// Second pass: the members of every structure and the variables of every POU.
    fn declarations(&mut self, src: &ast::Source) {
        for (k, t) in src.types.iter().enumerate() {
            let ast::TypeDeclKind::Struct(fields) = &t.kind else {
                // Read a named array type even when nothing uses it, so its mistakes are told.
                self.resolve(&ast::TypeSpec {
                    span: t.span,
                    kind: ast::TypeSpecKind::Named(t.name.clone()),
                });
                continue;
            };
            let Some(at) = self.struct_of[k] else {
                continue;
            };
            if fields.is_empty() {
                self.err(t.span, format!("the structure {} has no members", t.name));
            }
            for f in fields {
                if self.unit.structs[at]
                    .fields
                    .iter()
                    .any(|x| x.name.eq_ignore_ascii_case(&f.name))
                {
                    self.err(f.span, format!("{} is declared twice", f.name));
                    continue;
                }
                let Some(ty) = self.resolve(&f.ty) else {
                    continue;
                };
                if self.unit.holds_instance(ty) {
                    self.err(
                        f.ty.span,
                        "a function block instance inside a structure is not supported yet",
                    );
                    continue;
                }
                let init = f.init.as_ref().and_then(|i| self.init(i, ty));
                self.unit.structs[at].fields.push(Field {
                    name: f.name.clone(),
                    ty,
                    init,
                });
            }
        }
        for (i, p) in src.pous.iter().enumerate() {
            self.cur = i;
            if let Some(spec) = &p.result {
                // A function's result is its variable 0, named as the function.
                let ty = match self.resolve(spec) {
                    Some(t) if t.is_elementary() => t,
                    Some(t) => {
                        let what = self.n(t);
                        self.err(
                            spec.span,
                            format!(
                                "a function returning a {what} is not supported yet; a result is of an elementary type"
                            ),
                        );
                        Ty::Bool
                    }
                    None => Ty::Bool,
                };
                self.unit.pous[i].vars.push(Var {
                    name: p.name.clone(),
                    ty,
                    section: Section::Var,
                    init: None,
                });
            }
            for d in &p.vars {
                self.declare(d, p);
            }
        }
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.unit.pous[self.cur]
            .vars
            .iter()
            .position(|v| v.name.eq_ignore_ascii_case(name))
    }

    fn declare(&mut self, d: &ast::VarDecl, pou: &ast::Pou) {
        if self.find(&d.name).is_some() {
            self.err(d.span, format!("{} is declared twice", d.name));
            return;
        }
        // IEC 61131-3 ed. 3 §6.6.3.2 rule 18 advises against an instance named as a function;
        // here no variable may be, so `name(…)` is never two things. A variable named as a
        // type (`pump : Pump`) is no ambiguity: a type is named only in a declaration.
        if self.function(&d.name).is_some() {
            self.err(
                d.span,
                format!(
                    "{} is also the name of a function; a variable needs its own",
                    d.name
                ),
            );
            return;
        }
        let Some(ty) = self.resolve(&d.ty) else {
            return;
        };
        if self.unit.holds_instance(ty) {
            let instance = self.n(ty);
            match (pou.kind, d.section) {
                (ast::PouKind::Function, _) => {
                    // §6.6.2.1: a function does not store its state.
                    self.err(
                        d.ty.span,
                        format!(
                            "a function keeps nothing between calls, so {} cannot hold a {instance}",
                            pou.name
                        ),
                    );
                    return;
                }
                (ast::PouKind::FunctionBlock, Section::Input | Section::Output) => {
                    self.err(
                        d.ty.span,
                        "a function block instance as an input or an output of a function block is not supported yet",
                    );
                    return;
                }
                _ => {}
            }
        }
        let init = d.init.as_ref().and_then(|i| self.init(i, ty));
        self.unit.pous[self.cur].vars.push(Var {
            name: d.name.clone(),
            ty,
            section: d.section,
            init,
        });
    }

    /// The type a declaration names.
    fn resolve(&mut self, spec: &ast::TypeSpec) -> Option<Ty> {
        match &spec.kind {
            ast::TypeSpecKind::Named(name) => {
                if let Some(ty) = Ty::from_name(&name.to_ascii_uppercase()) {
                    return Some(ty);
                }
                match self.named(name) {
                    Some(Named::Struct(i)) => Some(Ty::Struct(i)),
                    Some(Named::Pou(p)) => match self.unit.pous[p].kind {
                        PouKind::FunctionBlock => Some(Ty::Instance(p)),
                        PouKind::Function { .. } => {
                            self.err(spec.span, format!("{name} is a function, not a type"));
                            None
                        }
                        PouKind::Program => {
                            self.err(spec.span, format!("{name} is a program, not a type"));
                            None
                        }
                    },
                    Some(Named::Alias(k)) => self.alias(k, name, spec.span),
                    None => {
                        self.err(
                            spec.span,
                            format!("{name} is not a type this compiler knows"),
                        );
                        None
                    }
                }
            }
            ast::TypeSpecKind::Array(dims, elem) => {
                let elem = self.resolve(elem);
                let mut ok = true;
                let mut count: i128 = 1;
                for (lo, hi) in dims {
                    if !fits(Ty::Dint, *lo) || !fits(Ty::Dint, *hi) {
                        self.err(
                            spec.span,
                            format!("the bounds {lo}..{hi} do not fit in a DINT"),
                        );
                        ok = false;
                    } else if hi < lo {
                        self.err(spec.span, format!("the range {lo}..{hi} is empty"));
                        ok = false;
                    } else {
                        count = count.saturating_mul(hi - lo + 1);
                    }
                }
                if ok && count > MAX_ELEMENTS {
                    self.err(
                        spec.span,
                        format!("an array of {count} elements is too large"),
                    );
                    ok = false;
                }
                let elem = elem?;
                if !ok {
                    return None;
                }
                let dims = dims
                    .iter()
                    .map(|(lo, hi)| (*lo as i64, *hi as i64))
                    .collect();
                Some(self.unit.array(dims, elem))
            }
        }
    }

    /// The array type a TYPE declaration gave a name to.
    fn alias(&mut self, k: usize, name: &str, span: Span) -> Option<Ty> {
        match self.aliases[k] {
            Alias::Known(ty) => ty,
            Alias::Reading => {
                self.err(span, format!("the type {name} is defined by itself"));
                None
            }
            Alias::Unread => {
                self.aliases[k] = Alias::Reading;
                let ty = self
                    .alias_specs
                    .get(k)
                    .cloned()
                    .flatten()
                    .and_then(|spec| self.resolve(&spec));
                self.aliases[k] = Alias::Known(ty);
                ty
            }
        }
    }

    /// An initial value for a variable or a member of type `ty`.
    fn init(&mut self, init: &ast::Init, ty: Ty) -> Option<Init> {
        match (init, ty) {
            (ast::Init::Value(e), ty) if ty.is_elementary() => {
                self.constant(e, ty).map(Init::Value)
            }
            (ast::Init::Value(e), Ty::Fb(_) | Ty::Instance(_)) => {
                self.err(
                    e.span,
                    "a function block instance cannot be given an initial value",
                );
                None
            }
            (ast::Init::Value(e), Ty::Array(_)) => {
                self.err(
                    e.span,
                    "an array starts from a list of values: [1, 2, 3] or [3(0)]",
                );
                None
            }
            (ast::Init::Value(e), _) => {
                self.err(
                    e.span,
                    "an initial value for a whole structure is not supported yet; give its members theirs in the TYPE",
                );
                None
            }
            (ast::Init::Array(items, span), Ty::Array(a)) => {
                let (elem, len) = (self.unit.arrays[a].elem, self.unit.arrays[a].len());
                if !elem.is_elementary() {
                    let what = self.n(elem);
                    self.err(
                        *span,
                        format!(
                            "a list of initial values for an array of {what} is not supported yet"
                        ),
                    );
                    return None;
                }
                let mut values = Vec::new();
                if !self.elements(items, elem, len, &mut values) {
                    return None;
                }
                // IEC 61131-3 ed. 3 §6.4.4.5.2 has values beyond the array ignored with a
                // warning. There is no warning here to give, so it is refused.
                if values.len() as u64 > len {
                    self.err(
                        *span,
                        format!("the list has more initial values than the array's {len} elements"),
                    );
                    return None;
                }
                Some(Init::Elements(values))
            }
            (ast::Init::Array(_, span), ty) => {
                let what = self.n(ty);
                self.err(
                    *span,
                    format!("a {what} is not an array: it does not start from a list"),
                );
                None
            }
        }
    }

    /// The values of an array initialiser, repetitions written out. Stops with `false` at a
    /// mistake; stops adding once there is one value more than the array holds.
    fn elements(
        &mut self,
        items: &[ast::ArrayInit],
        elem: Ty,
        len: u64,
        out: &mut Vec<Const>,
    ) -> bool {
        for item in items {
            match item {
                ast::ArrayInit::Value(e) => match self.constant(e, elem) {
                    Some(k) => {
                        if out.len() as u64 <= len {
                            out.push(k);
                        }
                    }
                    None => return false,
                },
                ast::ArrayInit::Repeat(times, inner, span) => {
                    if *times < 0 {
                        self.err(*span, format!("{times} is not a number of repetitions"));
                        return false;
                    }
                    let mut once = Vec::new();
                    if !self.elements(inner, elem, len, &mut once) {
                        return false;
                    }
                    if once.is_empty() {
                        self.err(*span, "a repetition of nothing");
                        return false;
                    }
                    for _ in 0..*times {
                        if out.len() as u64 > len {
                            break;
                        }
                        out.extend(once.iter().copied());
                    }
                }
            }
        }
        true
    }

    /// An initial value: a literal of the variable's type.
    fn constant(&mut self, e: &ast::Expr, ty: Ty) -> Option<Const> {
        if !matches!(e.kind, A::Int(_) | A::Real(_) | A::Bool(_) | A::Time(_)) {
            self.err(e.span, "an initial value must be a literal");
            return None;
        }
        let x = self.expr(e, Some(ty))?;
        match x.kind {
            ExprKind::Const(k) if x.ty == ty => Some(k),
            _ => {
                let (want, got) = (self.n(ty), self.n(x.ty));
                self.err(e.span, format!("a {want} cannot start as a {got}"));
                None
            }
        }
    }

    /// A type that contains itself has no size, and a function block that holds an instance
    /// of itself would be recursion. Returns whether one was found.
    fn containment(&mut self) -> bool {
        let mut found = false;
        if let Some(cycle) = self.unit.struct_cycle() {
            let names: Vec<String> = cycle
                .iter()
                .map(|s| self.unit.structs[*s].name.clone())
                .collect();
            self.diags.push(Diagnostic::whole(format!(
                "the structure {} contains itself: {}",
                names[0],
                chain(&names, "holds")
            )));
            found = true;
        }
        // No body has been read yet, so a cycle here is one of instances held.
        if let Err(cycle) = self.unit.order() {
            let names: Vec<String> = cycle
                .iter()
                .map(|p| self.unit.pous[*p].name.clone())
                .collect();
            self.err(
                self.pou_spans[cycle[0]],
                format!(
                    "recursion is not allowed: the function block {} would hold an instance of itself: {}",
                    names[0],
                    chain(&names, "holds")
                ),
            );
            found = true;
        }
        found
    }

    /// IEC 61131-3 ed. 3 §6.6.1.1 leaves recursion to the implementer. Here it is refused, so
    /// that every variable has one fixed place and a scan's memory is known before it runs.
    fn recursion(&mut self) {
        let Err(cycle) = self.unit.order() else {
            return;
        };
        let names: Vec<String> = cycle
            .iter()
            .map(|p| self.unit.pous[*p].name.clone())
            .collect();
        let next = cycle.get(1).copied().unwrap_or(cycle[0]);
        let span = self
            .calls
            .iter()
            .find(|(from, to, _)| *from == cycle[0] && *to == next)
            .map(|(_, _, span)| *span)
            .unwrap_or(self.pou_spans[cycle[0]]);
        self.err(
            span,
            format!("recursion is not allowed: {}", chain(&names, "calls")),
        );
    }

    /// The member `name` of a function block instance of type `ty`: its index, type and role.
    fn fb_member(&self, ty: Ty, name: &str) -> Option<(usize, Ty, Role)> {
        match ty {
            Ty::Fb(k) => {
                let i = k
                    .members()
                    .iter()
                    .position(|m| m.name.eq_ignore_ascii_case(name))?;
                let m = k.members()[i];
                let role = match m.direction {
                    Direction::Input => Role::Input,
                    Direction::Output => Role::Output,
                };
                Some((i, m.ty, role))
            }
            Ty::Instance(p) => {
                let vars = &self.unit.pous[p].vars;
                let i = vars
                    .iter()
                    .position(|v| v.name.eq_ignore_ascii_case(name))?;
                let role = match vars[i].section {
                    Section::Input => Role::Input,
                    Section::Output => Role::Output,
                    Section::Var => Role::Internal,
                };
                Some((i, vars[i].ty, role))
            }
            _ => None,
        }
    }

    /// Resolve a place: the variable, then each step of its path, with what may be done there.
    fn place(&mut self, p: &ast::Place) -> Option<Target> {
        let Some(var) = self.find(&p.name) else {
            self.err(p.span, format!("{} is not declared", p.name));
            return None;
        };
        let pou = &self.unit.pous[self.cur];
        let mut ty = pou.vars[var].ty;
        let mut readonly = None;
        let mut unreadable = None;
        // IEC 61131-3 ed. 3 Figure 7: VAR_INPUT is "not modifiable within the entity"
        // (Figure 13 rule 2a says the same of a function block's inputs). A PROGRAM's too: its
        // inputs are what the runtime binds to tags.
        if pou.vars[var].section == Section::Input {
            readonly = Some(format!(
                "{} is an input of {}; it cannot be written inside it",
                p.name, pou.name
            ));
        }
        let mut inside_instance = false;
        let mut path = Vec::new();
        let mut text = p.name.clone();
        for sel in &p.path {
            match sel {
                ast::Selector::Member(m, mspan) => match ty {
                    Ty::Struct(i) => {
                        let fields = &self.unit.structs[i].fields;
                        let Some(at) = fields.iter().position(|f| f.name.eq_ignore_ascii_case(m))
                        else {
                            let what = self.n(ty);
                            self.err(*mspan, format!("{what} has no member {m}"));
                            return None;
                        };
                        ty = fields[at].ty;
                        path.push(Sel::Member(at));
                    }
                    Ty::Fb(_) | Ty::Instance(_) => {
                        let block = self.n(ty);
                        let Some((at, mty, role)) = self.fb_member(ty, m) else {
                            self.err(p.span, format!("{block} has no member {m}"));
                            return None;
                        };
                        match role {
                            Role::Output => {
                                readonly.get_or_insert(format!(
                                    "{text}.{m} is an output of the block; it cannot be written"
                                ));
                            }
                            // Figure 13 rule 2b: an input may be set from outside, separately
                            // from the call. Only the POU that holds the instance does so.
                            Role::Input => {
                                // Figure 13 rule 1b: an input is not read from outside.
                                unreadable.get_or_insert(format!(
                                    "{text}.{m} is an input of the block; it is set from outside, not read there"
                                ));
                                if inside_instance {
                                    readonly.get_or_insert(format!(
                                        "{text} is inside another function block instance; only that block's body sets {m}"
                                    ));
                                }
                            }
                            // §6.6.3.2 rule 11: a VAR of a function block is private. An
                            // instance held there can still be looked into; nothing else.
                            Role::Internal => {
                                if !self.unit.holds_instance(mty) {
                                    self.err(
                                        *mspan,
                                        format!(
                                            "{m} is an internal variable of {block}; only its inputs and outputs can be named from outside"
                                        ),
                                    );
                                    return None;
                                }
                                readonly.get_or_insert(format!(
                                    "{text}.{m} is inside the function block {block}; only its body writes there"
                                ));
                            }
                        }
                        inside_instance = true;
                        ty = mty;
                        path.push(Sel::Member(at));
                    }
                    other => {
                        let what = self.n(other);
                        self.err(
                            p.span,
                            format!("{text} is a {what}, which has no member {m}"),
                        );
                        return None;
                    }
                },
                ast::Selector::Index(subscripts, ispan) => {
                    let Ty::Array(a) = ty else {
                        let what = self.n(ty);
                        self.err(*ispan, format!("{text} is a {what}, not an array"));
                        return None;
                    };
                    let array = self.unit.arrays[a].clone();
                    if subscripts.len() != array.dims.len() {
                        self.err(
                            *ispan,
                            format!(
                                "{text} has {} dimension(s); {} subscript(s) given",
                                array.dims.len(),
                                subscripts.len()
                            ),
                        );
                        return None;
                    }
                    let mut lowered = Vec::new();
                    for (e, (lo, hi)) in subscripts.iter().zip(&array.dims) {
                        // §6.4.4.5.1: a subscript is an expression of an integer type.
                        let x = self.expr(e, Some(Ty::Dint))?;
                        if !x.ty.is_int() {
                            let what = self.n(x.ty);
                            self.err(
                                e.span,
                                format!("a subscript must be an integer; this is a {what}"),
                            );
                            return None;
                        }
                        // The same section makes a subscript outside its range an error. A
                        // literal one is found here; a computed one stops the scan.
                        if let ExprKind::Const(Const::Int(v)) = x.kind {
                            if v < *lo || v > *hi {
                                self.err(
                                    e.span,
                                    format!("the subscript {v} is outside {lo}..{hi}"),
                                );
                                return None;
                            }
                        }
                        lowered.push(x);
                    }
                    ty = array.elem;
                    path.push(Sel::Index(lowered));
                }
            }
            match sel {
                ast::Selector::Member(m, _) => {
                    text.push('.');
                    text.push_str(m);
                }
                ast::Selector::Index(..) => text.push_str("[…]"),
            }
        }
        Some(Target {
            place: Place { var, path },
            ty,
            readonly,
            unreadable,
            inside_instance,
        })
    }

    /// `value` where `want` is needed: as is, widened, or a diagnostic.
    fn coerce(&mut self, value: Expr, want: Ty, span: Span) -> Option<Expr> {
        if value.ty == want {
            return Some(value);
        }
        if value.ty.widens_to(want) {
            if let ExprKind::Const(k) = value.kind {
                return Some(Expr {
                    ty: want,
                    kind: ExprKind::Const(match (k, want.is_real()) {
                        (Const::Int(v), true) => Const::Real(v as f64),
                        (k, _) => k,
                    }),
                });
            }
            return Some(Expr {
                ty: want,
                kind: ExprKind::Convert(Box::new(value)),
            });
        }
        let (from, to) = (self.n(value.ty), self.n(want));
        if value.ty.is_elementary() && want.is_elementary() {
            self.err(
                span,
                format!(
                    "a {from} cannot be used as a {to} without a conversion: write {from}_TO_{to}(…)"
                ),
            );
        } else {
            // IEC 61131-3 ed. 3 §7.3.3.2.1: multi-element variables are assigned only to
            // their own type.
            self.err(span, format!("a {from} cannot be used as a {to}"));
        }
        None
    }

    /// A place to be read or written as a value: not a whole function block instance.
    fn assign_to(&mut self, place: &ast::Place, writing: bool) -> Option<(Place, Ty)> {
        let t = self.place(place)?;
        if let Ty::Fb(_) | Ty::Instance(_) = t.ty {
            self.err(
                place.span,
                format!(
                    "{} is a function block instance; call it, or name one of its members",
                    place.text()
                ),
            );
            return None;
        }
        if self.unit.holds_instance(t.ty) {
            self.err(
                place.span,
                format!(
                    "{} holds function block instances; it cannot be assigned as a whole",
                    place.text()
                ),
            );
            return None;
        }
        if writing {
            if let Some(why) = t.readonly {
                self.err(place.span, why);
                return None;
            }
        }
        Some((t.place, t.ty))
    }

    fn stmts(&mut self, s: &[ast::Stmt]) -> Vec<Stmt> {
        s.iter().filter_map(|x| self.stmt(x)).collect()
    }

    fn stmt(&mut self, s: &ast::Stmt) -> Option<Stmt> {
        match s {
            ast::Stmt::Assign(place, value) => {
                let target = self.assign_to(place, true);
                let v = self.expr(value, target.as_ref().map(|t| t.1));
                let (p, ty) = target?;
                if p.var == 0 && matches!(self.unit.pous[self.cur].kind, PouKind::Function { .. }) {
                    self.result_assigned = true;
                }
                let v = self.coerce(v?, ty, value.span)?;
                Some(Stmt::Assign(p, v))
            }
            ast::Stmt::Call(called, args) => {
                let name = &called.name;
                if called.path.is_empty() && self.find(name).is_none() {
                    let msg = if self.function(name).is_some() {
                        // Table 72 lets a function be called as a statement, its result
                        // discarded; a function here has no other effect, so that is refused.
                        format!("{name} is a function: use its result, as in x := {name}(…);")
                    } else if matches!(self.named(name), Some(Named::Pou(_))) {
                        format!(
                            "{name} is a function block type; declare an instance of it and call that"
                        )
                    } else if name.to_ascii_uppercase().contains("_TO_") {
                        format!("{name}(…) gives a value; assign it: x := {name}(…);")
                    } else {
                        format!("{name} is not declared")
                    };
                    self.err(called.span, msg);
                    return None;
                }
                let t = self.place(called)?;
                let block = self.n(t.ty);
                if !matches!(t.ty, Ty::Fb(_) | Ty::Instance(_)) {
                    self.err(
                        called.span,
                        format!(
                            "{} is a {block}, not a function block instance",
                            called.text()
                        ),
                    );
                    return None;
                }
                if t.inside_instance {
                    self.err(
                        called.span,
                        format!(
                            "{} is inside another function block instance; only that block's body calls it",
                            called.text()
                        ),
                    );
                    return None;
                }
                let mut inputs: Vec<(usize, Expr)> = Vec::new();
                let mut outputs: Vec<(usize, Place)> = Vec::new();
                for a in args {
                    let (pname, pspan) = match a {
                        ast::Arg::In(n, _, s) | ast::Arg::Out(n, _, s) => (n, *s),
                        ast::Arg::Value(e) => {
                            self.err(
                                e.span,
                                "a call of a function block names each parameter: IN := …, or Q => …",
                            );
                            continue;
                        }
                    };
                    let Some((mi, mty, role)) = self.fb_member(t.ty, pname) else {
                        self.err(pspan, format!("{block} has no parameter {pname}"));
                        continue;
                    };
                    if role == Role::Internal {
                        self.err(
                            pspan,
                            format!("{pname} is an internal variable of {block}, not a parameter"),
                        );
                        continue;
                    }
                    match a {
                        ast::Arg::In(_, e, _) => {
                            if role != Role::Input {
                                self.err(pspan, format!("{pname} is an output of {block}: read it with {pname} => variable"));
                                continue;
                            }
                            if inputs.iter().any(|(m, _)| *m == mi) {
                                self.err(pspan, format!("{pname} is given twice"));
                                continue;
                            }
                            if let Some(v) = self
                                .expr(e, Some(mty))
                                .and_then(|v| self.coerce(v, mty, e.span))
                            {
                                inputs.push((mi, v));
                            }
                        }
                        ast::Arg::Value(_) => {}
                        ast::Arg::Out(_, target, _) => {
                            if role != Role::Output {
                                self.err(
                                    pspan,
                                    format!(
                                        "{pname} is an input of {block}: set it with {pname} := value"
                                    ),
                                );
                                continue;
                            }
                            if let Some((p, ty)) = self.assign_to(target, true) {
                                if !mty.widens_to(ty) {
                                    let (from, to) = (self.n(mty), self.n(ty));
                                    self.err(
                                        target.span,
                                        format!(
                                            "{pname} is a {from}, which does not fit in a {to}"
                                        ),
                                    );
                                    continue;
                                }
                                outputs.push((mi, p));
                            }
                        }
                    }
                }
                Some(Stmt::Call {
                    instance: t.place,
                    inputs,
                    outputs,
                })
            }
            ast::Stmt::If(branches, otherwise) => {
                let mut out = Vec::new();
                for (c, b) in branches {
                    let c = self.condition(c);
                    let b = self.stmts(b);
                    out.push((c?, b));
                }
                let e = otherwise
                    .as_ref()
                    .map(|b| self.stmts(b))
                    .unwrap_or_default();
                Some(Stmt::If(out, e))
            }
            ast::Stmt::Case(sel, arms, otherwise) => {
                let s = self.expr(sel, None);
                let sty = s.as_ref().map(|s| s.ty);
                if let Some(t) = sty {
                    if !t.is_int() {
                        let what = self.n(t);
                        self.err(
                            sel.span,
                            format!("CASE needs an integer selector; this is a {what}"),
                        );
                    }
                }
                let mut out = Vec::new();
                let mut seen: Vec<(i64, i64)> = Vec::new();
                for (labels, body) in arms {
                    let mut ranges = Vec::new();
                    for l in labels {
                        let (lo, hi, span) = match l {
                            CaseLabel::One(e) => (self.label(e, sty), None, e.span),
                            CaseLabel::Range(a, b) => {
                                (self.label(a, sty), self.label(b, sty), a.span)
                            }
                        };
                        let Some(lo) = lo else { continue };
                        let hi = match l {
                            CaseLabel::One(_) => lo,
                            CaseLabel::Range(..) => match hi {
                                Some(h) => h,
                                None => continue,
                            },
                        };
                        if hi < lo {
                            self.err(span, format!("the range {lo}..{hi} is empty"));
                            continue;
                        }
                        if seen.iter().any(|(a, b)| lo <= *b && *a <= hi) {
                            self.err(span, "this label is already covered by an earlier one");
                            continue;
                        }
                        seen.push((lo, hi));
                        ranges.push((lo, hi));
                    }
                    let b = self.stmts(body);
                    out.push((ranges, b));
                }
                let e = otherwise
                    .as_ref()
                    .map(|b| self.stmts(b))
                    .unwrap_or_default();
                Some(Stmt::Case(s?, out, e))
            }
            ast::Stmt::For {
                var,
                from,
                to,
                by,
                body,
            } => {
                let target = self.assign_to(var, true);
                if let Some((_, ty)) = &target {
                    if !ty.is_int() {
                        let what = self.n(*ty);
                        self.err(
                            var.span,
                            format!(
                                "a FOR variable must be an integer; {} is a {what}",
                                var.name
                            ),
                        );
                    }
                }
                let ty = target.as_ref().map(|t| t.1);
                let f = self
                    .expr(from, ty)
                    .and_then(|f| ty.and_then(|t| self.coerce(f, t, from.span)));
                let t = self
                    .expr(to, ty)
                    .and_then(|x| ty.and_then(|tt| self.coerce(x, tt, to.span)));
                let step = match by {
                    None => Some(1),
                    Some(e) => match self.expr(e, ty) {
                        Some(Expr {
                            kind: ExprKind::Const(Const::Int(0)),
                            ..
                        }) => {
                            self.err(e.span, "BY 0 never ends");
                            None
                        }
                        Some(Expr {
                            kind: ExprKind::Const(Const::Int(v)),
                            ..
                        }) => Some(v),
                        Some(_) => {
                            self.err(e.span, "BY must be a constant integer for now");
                            None
                        }
                        None => None,
                    },
                };
                self.loops += 1;
                let b = self.stmts(body);
                self.loops -= 1;
                Some(Stmt::For {
                    var: target?.0,
                    from: f?,
                    to: t?,
                    by: step?,
                    body: b,
                })
            }
            ast::Stmt::While(c, b) => {
                let c = self.condition(c);
                self.loops += 1;
                let b = self.stmts(b);
                self.loops -= 1;
                Some(Stmt::While(c?, b))
            }
            ast::Stmt::Repeat(b, c) => {
                self.loops += 1;
                let b = self.stmts(b);
                self.loops -= 1;
                let c = self.condition(c);
                Some(Stmt::Repeat(b, c?))
            }
            ast::Stmt::Exit(span) => {
                if self.loops == 0 {
                    self.err(*span, "EXIT outside a loop");
                    return None;
                }
                Some(Stmt::Exit)
            }
            ast::Stmt::Return(_) => Some(Stmt::Return),
        }
    }

    fn label(&mut self, e: &ast::Expr, sel: Option<Ty>) -> Option<i64> {
        match e.kind {
            A::Int(v) => {
                if let Some(t) = sel {
                    if t.is_int() && !fits(t, v) {
                        let what = self.n(t);
                        self.err(e.span, format!("{v} is outside the range of {what}"));
                        return None;
                    }
                }
                Some(v as i64)
            }
            _ => {
                self.err(e.span, "a CASE label must be an integer literal for now");
                None
            }
        }
    }

    fn condition(&mut self, e: &ast::Expr) -> Option<Expr> {
        let x = self.expr(e, Some(Ty::Bool))?;
        if x.ty != Ty::Bool {
            let what = self.n(x.ty);
            self.err(
                e.span,
                format!("a condition must be BOOL; this is a {what}"),
            );
            return None;
        }
        Some(x)
    }

    fn is_untyped_literal(e: &ast::Expr) -> bool {
        matches!(e.kind, A::Int(_) | A::Real(_))
    }

    /// Type an expression. `want` is the type its context needs; it decides what type a bare
    /// literal takes, and nothing else.
    fn expr(&mut self, e: &ast::Expr, want: Option<Ty>) -> Option<Expr> {
        let span = e.span;
        match &e.kind {
            A::Int(v) => {
                let v = *v;
                let ty = match want {
                    Some(t) if t.is_int() => {
                        if !fits(t, v) {
                            let what = self.n(t);
                            self.err(span, format!("{v} is outside the range of {what}"));
                            return None;
                        }
                        t
                    }
                    Some(t) if t.is_real() => {
                        return Some(Expr {
                            ty: t,
                            kind: ExprKind::Const(Const::Real(v as f64)),
                        })
                    }
                    _ if fits(Ty::Dint, v) => Ty::Dint,
                    _ if fits(Ty::Lint, v) => Ty::Lint,
                    _ => {
                        self.err(span, format!("{v} is too large for any integer type"));
                        return None;
                    }
                };
                Some(Expr {
                    ty,
                    kind: ExprKind::Const(Const::Int(v as i64)),
                })
            }
            A::Real(v) => {
                let ty = if want == Some(Ty::Real) {
                    Ty::Real
                } else {
                    Ty::Lreal
                };
                Some(Expr {
                    ty,
                    kind: ExprKind::Const(Const::Real(*v)),
                })
            }
            A::Bool(b) => Some(Expr {
                ty: Ty::Bool,
                kind: ExprKind::Const(Const::Bool(*b)),
            }),
            A::Time(t) => Some(Expr {
                ty: Ty::Time,
                kind: ExprKind::Const(Const::Time(*t)),
            }),
            A::Var(p) => {
                let t = self.place(p)?;
                if let Some(why) = t.unreadable {
                    self.err(span, why);
                    return None;
                }
                let text = p.text();
                match t.ty {
                    Ty::Fb(_) => {
                        self.err(
                            span,
                            format!(
                                "{text} is a function block instance; name one of its outputs, like {text}.Q"
                            ),
                        );
                        return None;
                    }
                    Ty::Instance(_) => {
                        self.err(
                            span,
                            format!(
                                "{text} is a function block instance; name one of its inputs or outputs"
                            ),
                        );
                        return None;
                    }
                    ty if self.unit.holds_instance(ty) => {
                        self.err(
                            span,
                            format!("{text} holds function block instances; it is not a value"),
                        );
                        return None;
                    }
                    _ => {}
                }
                Some(Expr {
                    ty: t.ty,
                    kind: ExprKind::Load(t.place),
                })
            }
            A::Not(inner) => {
                let x = self.expr(inner, Some(Ty::Bool))?;
                if x.ty != Ty::Bool {
                    let what = self.n(x.ty);
                    self.err(
                        span,
                        format!(
                            "NOT on a {what} (bit operations on integers are not supported yet)"
                        ),
                    );
                    return None;
                }
                Some(Expr {
                    ty: Ty::Bool,
                    kind: ExprKind::Not(Box::new(x)),
                })
            }
            A::Neg(inner) => {
                let x = self.expr(inner, want)?;
                if !(x.ty.is_int() || x.ty.is_real() || x.ty == Ty::Time) {
                    let what = self.n(x.ty);
                    self.err(span, format!("a {what} cannot be negated"));
                    return None;
                }
                Some(Expr {
                    ty: x.ty,
                    kind: ExprKind::Neg(Box::new(x)),
                })
            }
            A::Binary(op, l, r) => self.binary(*op, l, r, want, span),
            A::Call(name, args) => self.call(name, args, span),
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        l: &ast::Expr,
        r: &ast::Expr,
        want: Option<Ty>,
        span: Span,
    ) -> Option<Expr> {
        let comparison = matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        );
        let logical = matches!(op, BinOp::And | BinOp::Or | BinOp::Xor);
        // A bare literal takes the type of the other side; otherwise, outside a comparison,
        // the context's.
        let hint = if comparison || logical { None } else { want };
        let (a, b) = if Self::is_untyped_literal(l) && !Self::is_untyped_literal(r) {
            let b = self.expr(r, hint);
            let a = self.expr(l, b.as_ref().map(|b| b.ty).or(hint));
            (a, b)
        } else {
            let a = self.expr(l, if logical { Some(Ty::Bool) } else { hint });
            let rhint = match (&a, op) {
                (Some(a), BinOp::Mul | BinOp::Div) if a.ty == Ty::Time => Some(Ty::Lint),
                (Some(a), _) => Some(a.ty),
                (None, _) => hint,
            };
            let b = self.expr(r, rhint);
            (a, b)
        };
        let (a, b) = (a?, b?);
        if logical {
            if a.ty != Ty::Bool || b.ty != Ty::Bool {
                self.err(span, "AND, OR and XOR take BOOL operands (bit operations on integers are not supported yet)");
                return None;
            }
            let lop = match op {
                BinOp::And => Logic::And,
                BinOp::Or => Logic::Or,
                _ => Logic::Xor,
            };
            return Some(Expr {
                ty: Ty::Bool,
                kind: ExprKind::Logic(lop, Box::new(a), Box::new(b)),
            });
        }
        // TIME * integer, TIME / integer
        if a.ty == Ty::Time && matches!(op, BinOp::Mul | BinOp::Div) {
            if !b.ty.is_int() {
                let what = self.n(b.ty);
                self.err(
                    span,
                    format!("a TIME can be multiplied or divided by an integer only, not a {what}"),
                );
                return None;
            }
            let b = self.coerce(b, Ty::Lint, r.span)?;
            let aop = if op == BinOp::Mul {
                Arith::Mul
            } else {
                Arith::Div
            };
            return Some(Expr {
                ty: Ty::Time,
                kind: ExprKind::Arith(aop, Box::new(a), Box::new(b)),
            });
        }
        // widens_to is reflexive, so this also covers equal types.
        let common = if b.ty.widens_to(a.ty) {
            a.ty
        } else if a.ty.widens_to(b.ty) {
            b.ty
        } else if a.ty.widens_to(Ty::Lreal) && b.ty.widens_to(Ty::Lreal) {
            Ty::Lreal
        } else {
            let (x, y) = (self.n(a.ty), self.n(b.ty));
            self.err(
                span,
                format!("a {x} and a {y} cannot be combined without a conversion"),
            );
            return None;
        };
        let a = self.coerce(a, common, l.span)?;
        let b = self.coerce(b, common, r.span)?;
        if !common.is_elementary() {
            // IEC 61131-3 ed. 3 §7.3.3.2.2 compares multi-element variables of one type.
            let what = self.n(common);
            self.err(
                span,
                if comparison {
                    format!("comparing a whole {what} is not supported yet; compare its members")
                } else {
                    format!("a {what} is not a number")
                },
            );
            return None;
        }
        if comparison {
            if common == Ty::Bool && !matches!(op, BinOp::Eq | BinOp::Ne) {
                let what = self.n(common);
                self.err(span, format!("{what} values cannot be ordered"));
                return None;
            }
            let c = match op {
                BinOp::Eq => Cmp::Eq,
                BinOp::Ne => Cmp::Ne,
                BinOp::Lt => Cmp::Lt,
                BinOp::Le => Cmp::Le,
                BinOp::Gt => Cmp::Gt,
                _ => Cmp::Ge,
            };
            return Some(Expr {
                ty: Ty::Bool,
                kind: ExprKind::Cmp(c, Box::new(a), Box::new(b)),
            });
        }
        let aop = match op {
            BinOp::Add => Arith::Add,
            BinOp::Sub => Arith::Sub,
            BinOp::Mul => Arith::Mul,
            BinOp::Div => Arith::Div,
            _ => Arith::Mod,
        };
        let ok = match common {
            t if t.is_int() => true,
            t if t.is_real() => aop != Arith::Mod,
            Ty::Time => matches!(aop, Arith::Add | Arith::Sub),
            _ => false,
        };
        if !ok {
            let what = self.n(common);
            self.err(span, format!("{aop:?} is not defined on {what}"));
            return None;
        }
        Some(Expr {
            ty: common,
            kind: ExprKind::Arith(aop, Box::new(a), Box::new(b)),
        })
    }

    /// A function call in an expression: a function of the source, or one of the type
    /// conversions `<A>_TO_<B>` between the elementary numeric types and BOOL.
    fn call(&mut self, name: &str, args: &[ast::CallArg], span: Span) -> Option<Expr> {
        if let Some(f) = self.function(name) {
            return self.call_function(f, args, span);
        }
        if matches!(self.named(name), Some(Named::Pou(_))) {
            self.err(
                span,
                format!(
                    "{name} is not a function; a function block is called as a statement, through an instance"
                ),
            );
            return None;
        }
        if let Some(v) = self.find(name) {
            let ty = self.unit.pous[self.cur].vars[v].ty;
            let msg = if matches!(ty, Ty::Fb(_) | Ty::Instance(_)) {
                format!(
                    "{name} is a function block instance: call it as a statement, {name}(…); then read its outputs"
                )
            } else {
                format!("{name} is a variable, not a function")
            };
            self.err(span, msg);
            return None;
        }
        let upper = name.to_ascii_uppercase();
        let conv = upper
            .split_once("_TO_")
            .and_then(|(f, t)| Some((Ty::from_name(f)?, Ty::from_name(t)?)));
        let Some((from, to)) = conv else {
            self.err(
                span,
                format!(
                    "{name} is not a function this compiler knows yet (the *_TO_* conversions are, and the functions of this source)"
                ),
            );
            return None;
        };
        let numeric = |t: Ty| t.is_int() || t.is_real() || t == Ty::Bool;
        if !numeric(from) || !numeric(to) {
            self.err(span, format!("{upper} is not supported yet"));
            return None;
        }
        if from.is_real() && to == Ty::Bool {
            self.err(span, format!("{upper} is not supported yet"));
            return None;
        }
        if args.len() != 1 {
            self.err(span, format!("{upper} takes one argument"));
            return None;
        }
        if let Some((_, nspan)) = &args[0].name {
            self.err(*nspan, format!("{upper} takes its argument by position"));
            return None;
        }
        let x = self.expr(&args[0].value, Some(from))?;
        let x = self.coerce(x, from, args[0].value.span)?;
        if from == to {
            return Some(x);
        }
        Some(Expr {
            ty: to,
            kind: ExprKind::Convert(Box::new(x)),
        })
    }

    /// A call of the function `f` of this source (IEC 61131-3 ed. 3 §6.6.1.4.2, Table 20):
    /// by position, every input in the order declared; or by name, in any order, an input
    /// left out starting at its initial value.
    fn call_function(&mut self, f: usize, args: &[ast::CallArg], span: Span) -> Option<Expr> {
        let (name, result) = {
            let p = &self.unit.pous[f];
            (p.name.clone(), p.vars[0].ty)
        };
        let inputs: Vec<(usize, String, Ty)> = self.unit.pous[f]
            .vars
            .iter()
            .enumerate()
            .filter(|(_, v)| v.section == Section::Input)
            .map(|(i, v)| (i, v.name.clone(), v.ty))
            .collect();
        self.calls.push((self.cur, f, span));
        let named = args.iter().filter(|a| a.name.is_some()).count();
        if named != 0 && named != args.len() {
            self.err(
                span,
                format!("the arguments of {name} are either all named or all by position"),
            );
            return None;
        }
        let mut lowered: Vec<(usize, Expr)> = Vec::new();
        let mut ok = true;
        if named == 0 {
            if args.len() != inputs.len() {
                self.err(
                    span,
                    format!(
                        "{name} takes {} argument(s) by position; {} given",
                        inputs.len(),
                        args.len()
                    ),
                );
                return None;
            }
            for (a, (vi, _, ty)) in args.iter().zip(&inputs) {
                match self
                    .expr(&a.value, Some(*ty))
                    .and_then(|v| self.coerce(v, *ty, a.value.span))
                {
                    Some(v) => lowered.push((*vi, v)),
                    None => ok = false,
                }
            }
        } else {
            for a in args {
                let Some((pname, pspan)) = &a.name else {
                    continue;
                };
                let Some((vi, _, ty)) = inputs
                    .iter()
                    .find(|(_, n, _)| n.eq_ignore_ascii_case(pname))
                else {
                    self.err(*pspan, format!("{name} has no input {pname}"));
                    ok = false;
                    continue;
                };
                if lowered.iter().any(|(i, _)| i == vi) {
                    self.err(*pspan, format!("{pname} is given twice"));
                    ok = false;
                    continue;
                }
                match self
                    .expr(&a.value, Some(*ty))
                    .and_then(|v| self.coerce(v, *ty, a.value.span))
                {
                    Some(v) => lowered.push((*vi, v)),
                    None => ok = false,
                }
            }
        }
        if !ok {
            return None;
        }
        Some(Expr {
            ty: result,
            kind: ExprKind::Call {
                pou: f,
                args: lowered,
            },
        })
    }
}
