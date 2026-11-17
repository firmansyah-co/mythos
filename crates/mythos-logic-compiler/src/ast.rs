//! The syntax tree of Structured Text, as written: names unresolved, types unchecked.

use crate::lexer::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub name: String,
    pub span: Span,
    pub vars: Vec<VarDecl>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Var,
    Input,
    Output,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub span: Span,
    pub section: Section,
    pub type_name: String,
    pub type_span: Span,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct Place {
    pub name: String,
    pub member: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Arg {
    /// `name := value`
    In(String, Expr, Span),
    /// `name => target`
    Out(String, Place, Span),
}

#[derive(Debug, Clone)]
pub enum CaseLabel {
    One(Expr),
    Range(Expr, Expr),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Assign(Place, Expr),
    /// A function block instance called as a statement: `t1(IN := x, PT := T#1s);`
    Call(String, Vec<Arg>, Span),
    If(Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>),
    Case(Expr, Vec<(Vec<CaseLabel>, Vec<Stmt>)>, Option<Vec<Stmt>>),
    For {
        var: Place,
        from: Expr,
        to: Expr,
        by: Option<Expr>,
        body: Vec<Stmt>,
    },
    While(Expr, Vec<Stmt>),
    Repeat(Vec<Stmt>, Expr),
    Exit(Span),
    Return(Span),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    Xor,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i128),
    Real(f64),
    Bool(bool),
    Time(i64),
    Var(Place),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// A function call in an expression: `INT_TO_REAL(x)`
    Call(String, Vec<Expr>),
}
