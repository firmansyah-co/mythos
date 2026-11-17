//! The syntax tree of Structured Text, as written: names unresolved, types unchecked.

pub use mythos_logic_ir::Section;
use mythos_logic_ir::Span;

/// Everything one source declares, in the order written.
///
/// A source with mistakes still gives a tree: what could not be read is left out and marked
/// (`lost`, [`TypeDecl::partial`], [`TypeDeclKind::Unknown`], [`Pou::partial_vars`],
/// [`Pou::partial_body`]), so that the checker says nothing about what follows from the gap.
#[derive(Debug, Clone, Default)]
pub struct Source {
    pub types: Vec<TypeDecl>,
    pub pous: Vec<Pou>,
    /// Declarations whose name could not be read: the source declares something more of
    /// each kind listed, and the tree does not know what it is called.
    pub lost: Vec<Lost>,
}

/// A kind of declaration, of one the parser could not read the name of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lost {
    Program,
    Function,
    /// A function block type, a structure or another type.
    Type,
}

/// One declaration of a `TYPE … END_TYPE` block.
#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub span: Span,
    pub kind: TypeDeclKind,
    /// Part of the declaration could not be read: a structure may have more members.
    pub partial: bool,
}

#[derive(Debug, Clone)]
pub enum TypeDeclKind {
    /// `name : STRUCT … END_STRUCT;`
    Struct(Vec<VarDecl>),
    /// `name : ARRAY […] OF …;`: another name for that array type.
    Alias(TypeSpec),
    /// What the name stands for could not be read.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PouKind {
    Program,
    FunctionBlock,
    Function,
}

/// A program, a function block type or a function.
#[derive(Debug, Clone)]
pub struct Pou {
    pub kind: PouKind,
    pub name: String,
    pub span: Span,
    /// A function's result type.
    pub result: Option<TypeSpec>,
    pub vars: Vec<VarDecl>,
    pub body: Vec<Stmt>,
    /// Its head or one of its declarations could not be read: it may have a result type and
    /// variables that are not in `vars`.
    pub partial_vars: bool,
    /// One of its statements could not be read: `body` is not all it does.
    pub partial_body: bool,
}

/// A type as a declaration names it.
#[derive(Debug, Clone)]
pub struct TypeSpec {
    pub span: Span,
    pub kind: TypeSpecKind,
}

#[derive(Debug, Clone)]
pub enum TypeSpecKind {
    Named(String),
    /// `ARRAY [lo..hi, …] OF element`
    Array(Vec<(i128, i128)>, Box<TypeSpec>),
    /// `STRING`, or `STRING[n]` with the length as written and where it is.
    String(Option<(i128, Span)>),
}

/// What a declaration starts its variable with.
#[derive(Debug, Clone)]
pub enum Init {
    Value(Expr),
    /// `[1, 2, 3(0)]`
    Array(Vec<ArrayInit>, Span),
}

#[derive(Debug, Clone)]
pub enum ArrayInit {
    Value(Expr),
    /// `3(0, 1)`: the list, that many times.
    Repeat(i128, Vec<ArrayInit>, Span),
}

/// A variable of a POU, or a member of a structure.
#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub span: Span,
    pub section: Section,
    pub ty: TypeSpec,
    pub init: Option<Init>,
}

/// One step from a variable to a part of it.
#[derive(Debug, Clone)]
pub enum Selector {
    /// `.name`
    Member(String, Span),
    /// `[i, j]`
    Index(Vec<Expr>, Span),
}

/// A variable or a part of one: `name`, `inst.member`, `a[i].b`.
#[derive(Debug, Clone)]
pub struct Place {
    pub name: String,
    pub span: Span,
    pub path: Vec<Selector>,
}

impl Place {
    /// As written, for messages: `a[…].b`.
    pub fn text(&self) -> String {
        let mut s = self.name.clone();
        for sel in &self.path {
            match sel {
                Selector::Member(m, _) => {
                    s.push('.');
                    s.push_str(m);
                }
                Selector::Index(..) => s.push_str("[…]"),
            }
        }
        s
    }
}

/// An argument of a function block call.
#[derive(Debug, Clone)]
pub enum Arg {
    /// `name := value`
    In(String, Expr, Span),
    /// `name => target`
    Out(String, Place, Span),
    /// A value alone, by position.
    Value(Expr),
}

/// An argument of a function call.
#[derive(Debug, Clone)]
pub struct CallArg {
    /// The input it names (`a := 1`); none in a call by position.
    pub name: Option<(String, Span)>,
    pub value: Expr,
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
    Call(Place, Vec<Arg>),
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
    /// A character string: its characters, one byte each.
    Str(Vec<u8>),
    Var(Place),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// A function call in an expression: `INT_TO_REAL(x)`, `Scale(raw := x, gain := 2.0)`
    Call(String, Vec<CallArg>),
}
