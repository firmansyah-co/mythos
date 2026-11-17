//! From the syntax tree to the IR: resolve every name, type every expression, and refuse what
//! IEC 61131-3 does not allow or this compiler does not support yet. Nothing unsupported is
//! compiled into something else: it is a diagnostic.

use crate::ast::{self, BinOp, CaseLabel, ExprKind as A};
use mythos_logic_ir::*;

pub fn check(p: &ast::Program) -> Result<Program, Vec<Diagnostic>> {
    let mut c = Checker {
        vars: Vec::new(),
        diags: Vec::new(),
        loops: 0,
    };
    for d in &p.vars {
        c.declare(d);
    }
    let body = c.stmts(&p.body);
    if c.diags.is_empty() {
        Ok(Program {
            name: p.name.clone(),
            vars: c.vars,
            body,
        })
    } else {
        Err(c.diags)
    }
}

struct Checker {
    vars: Vec<Var>,
    diags: Vec<Diagnostic>,
    loops: usize,
}

/// A literal's value before it knows its type.
fn fits(ty: Ty, v: i128) -> bool {
    ty.int_range().is_some_and(|(lo, hi)| v >= lo && v <= hi)
}

impl Checker {
    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::at(span, msg));
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.vars
            .iter()
            .position(|v| v.name.eq_ignore_ascii_case(name))
    }

    fn declare(&mut self, d: &ast::VarDecl) {
        if self.find(&d.name).is_some() {
            self.err(d.span, format!("{} is declared twice", d.name));
            return;
        }
        let Some(ty) = Ty::from_name(&d.type_name.to_ascii_uppercase()) else {
            self.err(
                d.type_span,
                format!("{} is not a type this compiler knows", d.type_name),
            );
            return;
        };
        let init = match (&d.init, ty) {
            (None, _) => None,
            (Some(e), Ty::Fb(_)) => {
                self.err(
                    e.span,
                    "a function block instance cannot be given an initial value",
                );
                None
            }
            (Some(e), ty) => self.constant(e, ty),
        };
        self.vars.push(Var {
            name: d.name.clone(),
            ty,
            section: d.section,
            init,
        });
    }

    /// An initial value: a literal of the variable's type.
    fn constant(&mut self, e: &ast::Expr, ty: Ty) -> Option<Const> {
        let x = self.expr(e, Some(ty))?;
        match x.kind {
            ExprKind::Const(k) if x.ty == ty => Some(k),
            ExprKind::Const(_) => {
                self.err(
                    e.span,
                    format!("a {} cannot start as a {}", ty.name(), x.ty.name()),
                );
                None
            }
            _ => {
                self.err(e.span, "an initial value must be a literal");
                None
            }
        }
    }

    fn place(&mut self, p: &ast::Place) -> Option<Place> {
        let Some(var) = self.find(&p.name) else {
            self.err(p.span, format!("{} is not declared", p.name));
            return None;
        };
        match (self.vars[var].ty, &p.member) {
            (_, None) => Some(Place { var, member: None }),
            (Ty::Fb(k), Some(m)) => {
                let Some(i) = k
                    .members()
                    .iter()
                    .position(|x| x.name.eq_ignore_ascii_case(m))
                else {
                    self.err(p.span, format!("{} has no member {m}", k.name()));
                    return None;
                };
                Some(Place {
                    var,
                    member: Some(i),
                })
            }
            (ty, Some(m)) => {
                self.err(
                    p.span,
                    format!("{} is a {}, which has no member {m}", p.name, ty.name()),
                );
                None
            }
        }
    }

    fn place_ty(&self, p: Place) -> Ty {
        let v = &self.vars[p.var];
        match (v.ty, p.member) {
            (Ty::Fb(k), Some(m)) => k.members()[m].ty,
            (ty, _) => ty,
        }
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
        self.err(
            span,
            format!(
                "a {} cannot be used as a {} without a conversion: write {}_TO_{}(…)",
                value.ty.name(),
                want.name(),
                value.ty.name(),
                want.name()
            ),
        );
        None
    }

    fn assign_to(&mut self, place: &ast::Place, writing: bool) -> Option<(Place, Ty)> {
        let p = self.place(place)?;
        let ty = self.place_ty(p);
        if let Ty::Fb(_) = ty {
            self.err(
                place.span,
                format!(
                    "{} is a function block instance; call it, or name one of its members",
                    place.name
                ),
            );
            return None;
        }
        if writing {
            if let (Ty::Fb(k), Some(m)) = (self.vars[p.var].ty, p.member) {
                if k.members()[m].direction == Direction::Output {
                    self.err(
                        place.span,
                        format!(
                            "{}.{} is an output of the block; it cannot be written",
                            place.name,
                            k.members()[m].name
                        ),
                    );
                    return None;
                }
            }
        }
        Some((p, ty))
    }

    fn stmts(&mut self, s: &[ast::Stmt]) -> Vec<Stmt> {
        s.iter().filter_map(|x| self.stmt(x)).collect()
    }

    fn stmt(&mut self, s: &ast::Stmt) -> Option<Stmt> {
        match s {
            ast::Stmt::Assign(place, value) => {
                let target = self.assign_to(place, true);
                let v = self.expr(value, target.map(|t| t.1));
                let (p, ty) = target?;
                let v = self.coerce(v?, ty, value.span)?;
                Some(Stmt::Assign(p, v))
            }
            ast::Stmt::Call(name, args, span) => {
                let Some(var) = self.find(name) else {
                    if name.to_ascii_uppercase().contains("_TO_") {
                        self.err(
                            *span,
                            format!("{name}(…) gives a value; assign it: x := {name}(…);"),
                        );
                    } else {
                        self.err(*span, format!("{name} is not declared"));
                    }
                    return None;
                };
                let Ty::Fb(k) = self.vars[var].ty else {
                    self.err(
                        *span,
                        format!(
                            "{name} is a {}, not a function block instance",
                            self.vars[var].ty.name()
                        ),
                    );
                    return None;
                };
                let mut inputs = Vec::new();
                let mut outputs = Vec::new();
                for a in args {
                    let (pname, pspan) = match a {
                        ast::Arg::In(n, _, s) | ast::Arg::Out(n, _, s) => (n, *s),
                    };
                    let Some(mi) = k
                        .members()
                        .iter()
                        .position(|m| m.name.eq_ignore_ascii_case(pname))
                    else {
                        self.err(pspan, format!("{} has no parameter {pname}", k.name()));
                        continue;
                    };
                    let member = k.members()[mi];
                    let mty = member.ty;
                    match a {
                        ast::Arg::In(_, e, _) => {
                            if member.direction != Direction::Input {
                                self.err(pspan, format!("{pname} is an output of {}: read it with {pname} => variable", k.name()));
                                continue;
                            }
                            if let Some(v) = self
                                .expr(e, Some(mty))
                                .and_then(|v| self.coerce(v, mty, e.span))
                            {
                                inputs.push((mi, v));
                            }
                        }
                        ast::Arg::Out(_, target, _) => {
                            if member.direction != Direction::Output {
                                self.err(
                                    pspan,
                                    format!(
                                        "{pname} is an input of {}: set it with {pname} := value",
                                        k.name()
                                    ),
                                );
                                continue;
                            }
                            if let Some((p, ty)) = self.assign_to(target, true) {
                                if !mty.widens_to(ty) {
                                    self.err(
                                        target.span,
                                        format!(
                                            "{pname} is a {}, which does not fit in a {}",
                                            mty.name(),
                                            ty.name()
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
                    var,
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
                        self.err(
                            sel.span,
                            format!("CASE needs an integer selector; this is a {}", t.name()),
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
                if let Some((_, ty)) = target {
                    if !ty.is_int() {
                        self.err(
                            var.span,
                            format!(
                                "a FOR variable must be an integer; {} is a {}",
                                var.name,
                                ty.name()
                            ),
                        );
                    }
                }
                let ty = target.map(|t| t.1);
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
                        self.err(e.span, format!("{v} is outside the range of {}", t.name()));
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
            self.err(
                e.span,
                format!("a condition must be BOOL; this is a {}", x.ty.name()),
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
                            self.err(span, format!("{v} is outside the range of {}", t.name()));
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
                let place = self.place(p)?;
                let ty = self.place_ty(place);
                if let Ty::Fb(_) = ty {
                    self.err(
                        span,
                        format!(
                            "{} is a function block instance; name one of its outputs, like {}.Q",
                            p.name, p.name
                        ),
                    );
                    return None;
                }
                Some(Expr {
                    ty,
                    kind: ExprKind::Load(place),
                })
            }
            A::Not(inner) => {
                let x = self.expr(inner, Some(Ty::Bool))?;
                if x.ty != Ty::Bool {
                    self.err(
                        span,
                        format!(
                            "NOT on a {} (bit operations on integers are not supported yet)",
                            x.ty.name()
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
                    self.err(span, format!("a {} cannot be negated", x.ty.name()));
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
                self.err(
                    span,
                    format!(
                        "a TIME can be multiplied or divided by an integer only, not a {}",
                        b.ty.name()
                    ),
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
            self.err(
                span,
                format!(
                    "a {} and a {} cannot be combined without a conversion",
                    a.ty.name(),
                    b.ty.name()
                ),
            );
            return None;
        };
        let a = self.coerce(a, common, l.span)?;
        let b = self.coerce(b, common, r.span)?;
        if comparison {
            if matches!(common, Ty::Fb(_))
                || (common == Ty::Bool && !matches!(op, BinOp::Eq | BinOp::Ne))
            {
                self.err(span, format!("{} values cannot be ordered", common.name()));
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
            self.err(
                span,
                format!("{:?} is not defined on {}", aop, common.name()),
            );
            return None;
        }
        Some(Expr {
            ty: common,
            kind: ExprKind::Arith(aop, Box::new(a), Box::new(b)),
        })
    }

    /// Functions: the type conversions `<A>_TO_<B>` between the elementary numeric types and BOOL.
    fn call(&mut self, name: &str, args: &[ast::Expr], span: Span) -> Option<Expr> {
        let upper = name.to_ascii_uppercase();
        let conv = upper
            .split_once("_TO_")
            .and_then(|(f, t)| Some((Ty::from_name(f)?, Ty::from_name(t)?)));
        let Some((from, to)) = conv else {
            self.err(
                span,
                format!(
                    "{name} is not a function this compiler knows yet (the *_TO_* conversions are)"
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
        let x = self.expr(&args[0], Some(from))?;
        let x = self.coerce(x, from, args[0].span)?;
        if from == to {
            return Some(x);
        }
        Some(Expr {
            ty: to,
            kind: ExprKind::Convert(Box::new(x)),
        })
    }
}
