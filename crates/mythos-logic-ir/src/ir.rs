//! The intermediate representation: a checked program, every name resolved and every
//! expression typed, and nothing yet decided about how a target stores or runs it.
//!
//! This is the seam of ADR-013. Every language (ST now; LD, FBD, SFC and IL later) is lowered
//! to it, and every backend (VM bytecode now; C later) starts from it. So it holds no memory
//! addresses, no instruction choices and no word sizes: variables are indices, and a backend
//! decides where they live.

use crate::{Section, StdFb};

/// The types the IR knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Ty {
    Bool,
    Sint,
    Int,
    Dint,
    Lint,
    Real,
    Lreal,
    Time,
    Fb(StdFb),
}

impl Ty {
    pub fn from_name(upper: &str) -> Option<Ty> {
        Some(match upper {
            "BOOL" => Ty::Bool,
            "SINT" => Ty::Sint,
            "INT" => Ty::Int,
            "DINT" => Ty::Dint,
            "LINT" => Ty::Lint,
            "REAL" => Ty::Real,
            "LREAL" => Ty::Lreal,
            "TIME" => Ty::Time,
            _ => {
                return StdFb::ALL
                    .into_iter()
                    .find(|k| k.name() == upper)
                    .map(Ty::Fb)
            }
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Ty::Bool => "BOOL",
            Ty::Sint => "SINT",
            Ty::Int => "INT",
            Ty::Dint => "DINT",
            Ty::Lint => "LINT",
            Ty::Real => "REAL",
            Ty::Lreal => "LREAL",
            Ty::Time => "TIME",
            Ty::Fb(k) => k.name(),
        }
    }

    pub fn is_int(self) -> bool {
        matches!(self, Ty::Sint | Ty::Int | Ty::Dint | Ty::Lint)
    }
    pub fn is_real(self) -> bool {
        matches!(self, Ty::Real | Ty::Lreal)
    }

    /// The range of an integer type.
    pub fn int_range(self) -> Option<(i128, i128)> {
        Some(match self {
            Ty::Sint => (i8::MIN as i128, i8::MAX as i128),
            Ty::Int => (i16::MIN as i128, i16::MAX as i128),
            Ty::Dint => (i32::MIN as i128, i32::MAX as i128),
            Ty::Lint => (i64::MIN as i128, i64::MAX as i128),
            _ => return None,
        })
    }

    /// Whether a value of `self` may be used where `to` is wanted without a conversion
    /// function: the lossless conversions. Not yet checked against the standard (ADR-013 §2.13).
    pub fn widens_to(self, to: Ty) -> bool {
        use Ty::*;
        self == to
            || matches!(
                (self, to),
                (Sint, Int | Dint | Lint | Real | Lreal)
                    | (Int, Dint | Lint | Real | Lreal)
                    | (Dint, Lint | Lreal)
                    | (Real, Lreal)
            )
    }
}

/// A constant value.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Const {
    Bool(bool),
    Int(i64),
    Real(f64),
    Time(i64),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Var {
    pub name: String,
    pub ty: Ty,
    pub section: Section,
    pub init: Option<Const>,
}

/// Where a value is read or written: a variable, or a member of a function block instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Place {
    pub var: usize,
    /// Index into the instance's `StdFb::members()`.
    pub member: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Logic {
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Expr {
    pub ty: Ty,
    pub kind: ExprKind,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExprKind {
    Const(Const),
    Load(Place),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    /// Both operands have the expression's type, except TIME * or / an integer (right is LINT).
    Arith(Arith, Box<Expr>, Box<Expr>),
    /// Both operands have the same type; the result is BOOL.
    Cmp(Cmp, Box<Expr>, Box<Expr>),
    Logic(Logic, Box<Expr>, Box<Expr>),
    /// The operand's value as the expression's type.
    Convert(Box<Expr>),
}

/// The labels of one CASE arm, as inclusive ranges of the selector's (integer) type, and its
/// statements.
pub type CaseArm = (Vec<(i64, i64)>, Vec<Stmt>);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Stmt {
    Assign(Place, Expr),
    /// Set the inputs, run the block, copy the outputs out.
    Call {
        var: usize,
        inputs: Vec<(usize, Expr)>,
        outputs: Vec<(usize, Place)>,
    },
    If(Vec<(Expr, Vec<Stmt>)>, Vec<Stmt>),
    /// Arms, then the ELSE statements.
    Case(Expr, Vec<CaseArm>, Vec<Stmt>),
    For {
        var: Place,
        from: Expr,
        to: Expr,
        by: i64,
        body: Vec<Stmt>,
    },
    While(Expr, Vec<Stmt>),
    Repeat(Vec<Stmt>, Expr),
    Exit,
    Return,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Program {
    pub name: String,
    pub vars: Vec<Var>,
    pub body: Vec<Stmt>,
}

impl Program {
    /// The type of what `place` names.
    pub fn place_ty(&self, place: Place) -> Ty {
        let var = &self.vars[place.var];
        match (var.ty, place.member) {
            (Ty::Fb(k), Some(m)) => k.members()[m].ty,
            (ty, _) => ty,
        }
    }

    /// Every type the program uses, for checking it against a target's profile.
    pub fn types_used(&self) -> Vec<Ty> {
        let mut out: Vec<Ty> = Vec::new();
        let mut add = |t: Ty| {
            if !out.contains(&t) {
                out.push(t)
            }
        };
        for v in &self.vars {
            add(v.ty);
            if let Ty::Fb(k) = v.ty {
                for m in k.members() {
                    add(m.ty);
                }
            }
        }
        fn walk_e(e: &Expr, add: &mut dyn FnMut(Ty)) {
            add(e.ty);
            match &e.kind {
                ExprKind::Neg(a) | ExprKind::Not(a) | ExprKind::Convert(a) => walk_e(a, add),
                ExprKind::Arith(_, a, b) | ExprKind::Cmp(_, a, b) | ExprKind::Logic(_, a, b) => {
                    walk_e(a, add);
                    walk_e(b, add);
                }
                ExprKind::Const(_) | ExprKind::Load(_) => {}
            }
        }
        fn walk_s(s: &[Stmt], add: &mut dyn FnMut(Ty)) {
            for st in s {
                match st {
                    Stmt::Assign(_, e) => walk_e(e, add),
                    Stmt::Call { inputs, .. } => inputs.iter().for_each(|(_, e)| walk_e(e, add)),
                    Stmt::If(br, el) => {
                        for (c, b) in br {
                            walk_e(c, add);
                            walk_s(b, add);
                        }
                        walk_s(el, add);
                    }
                    Stmt::Case(sel, arms, el) => {
                        walk_e(sel, add);
                        arms.iter().for_each(|(_, b)| walk_s(b, add));
                        walk_s(el, add);
                    }
                    Stmt::For { from, to, body, .. } => {
                        walk_e(from, add);
                        walk_e(to, add);
                        walk_s(body, add);
                    }
                    Stmt::While(c, b) | Stmt::Repeat(b, c) => {
                        walk_e(c, add);
                        walk_s(b, add);
                    }
                    Stmt::Exit | Stmt::Return => {}
                }
            }
        }
        walk_s(&self.body, &mut add);
        out
    }
}
