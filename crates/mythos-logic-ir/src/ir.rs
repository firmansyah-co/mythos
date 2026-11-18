//! The intermediate representation: a checked unit, every name resolved and every expression
//! typed, and nothing yet decided about how a target stores or runs it.
//!
//! This is the seam of ADR-013. Every language (ST now; LD, FBD and SFC later) is lowered
//! to it, and every backend (VM bytecode now; C later) starts from it. So it holds no memory
//! addresses, no instruction choices and no word sizes: variables, members, types and program
//! organization units are indices, and a backend decides where they live.
//!
//! A [`Unit`] is everything one source declares: its structures, its array types, and its
//! program organization units (POUs): functions, function block types and one program, the
//! entry. Nothing in a unit is recursive (a function does not call itself, a type does not
//! contain itself), which [`Unit::order`] checks; that is what lets a backend give every
//! variable one fixed place.

use crate::{Retention, Section, StdFb, DEFAULT_STRING_LENGTH, IR_VERSION};

/// The types the IR knows. The user-defined ones are indices into the [`Unit`] that holds them.
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
    /// A single-byte character string of at most this many characters (IEC 61131-3 ed. 3
    /// §6.4.2.2, Table 10 feature 16a): `STRING[n]`, or [`DEFAULT_STRING_LENGTH`] when the
    /// declaration gives none. Two strings of different lengths are two types, but a value of
    /// one is assigned to the other (§6.6.1.2.2): truncated when it does not fit.
    String(u16),
    /// An instance of a standard function block.
    Fb(StdFb),
    /// A structure: an index into [`Unit::structs`].
    Struct(usize),
    /// An array: an index into [`Unit::arrays`]. Two arrays with the same bounds and element
    /// type have the same index, so they are one type.
    Array(usize),
    /// An instance of a user function block: the index of its type in [`Unit::pous`].
    Instance(usize),
}

impl Ty {
    /// An elementary type or a standard function block, by its name in upper case.
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
            "STRING" => Ty::String(DEFAULT_STRING_LENGTH),
            _ => {
                return StdFb::ALL
                    .into_iter()
                    .find(|k| k.name() == upper)
                    .map(Ty::Fb)
            }
        })
    }

    /// The name of an elementary type or a standard function block; `None` for a user-defined
    /// type, whose name its [`Unit`] knows ([`Unit::ty_name`]).
    pub fn builtin_name(self) -> Option<&'static str> {
        Some(match self {
            Ty::Bool => "BOOL",
            Ty::Sint => "SINT",
            Ty::Int => "INT",
            Ty::Dint => "DINT",
            Ty::Lint => "LINT",
            Ty::Real => "REAL",
            Ty::Lreal => "LREAL",
            Ty::Time => "TIME",
            Ty::String(_) => "STRING",
            Ty::Fb(k) => k.name(),
            Ty::Struct(_) | Ty::Array(_) | Ty::Instance(_) => return None,
        })
    }

    pub fn is_int(self) -> bool {
        matches!(self, Ty::Sint | Ty::Int | Ty::Dint | Ty::Lint)
    }
    pub fn is_real(self) -> bool {
        matches!(self, Ty::Real | Ty::Lreal)
    }
    /// One value: what an expression yields and a load or store moves.
    pub fn is_elementary(self) -> bool {
        matches!(
            self,
            Ty::Bool | Ty::Sint | Ty::Int | Ty::Dint | Ty::Lint | Ty::Real | Ty::Lreal | Ty::Time
        )
    }
    /// A structure or an array: assigned and passed whole, as a copy.
    pub fn is_aggregate(self) -> bool {
        matches!(self, Ty::Struct(_) | Ty::Array(_))
    }
    pub fn is_string(self) -> bool {
        matches!(self, Ty::String(_))
    }

    /// The same type, or, for strings, a string of any length: what a target refuses is a
    /// kind of type, not one length of it.
    pub fn same_kind(self, other: Ty) -> bool {
        self == other || (self.is_string() && other.is_string())
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
    /// function: the conversions marked "i" in IEC 61131-3 ed. 3 Figure 11. A structure or an
    /// array goes only where its own type is wanted (§6.4.4.6.1, §7.3.3.2.1).
    pub fn widens_to(self, to: Ty) -> bool {
        use Ty::*;
        self == to
            || matches!(
                (self, to),
                // §6.6.1.2.2: a STRING is assigned to a STRING, whatever their lengths.
                (String(_), String(_))
                    | (Sint, Int | Dint | Lint | Real | Lreal)
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

/// The value a variable or a structure's member starts with.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Init {
    /// Of an elementary variable.
    Value(Const),
    /// Of an array of elementary elements: its elements in order, the rightmost subscript
    /// varying fastest (IEC 61131-3 ed. 3 §6.4.4.5.2). Elements past the end of the list start
    /// at their type's default.
    Elements(Vec<Const>),
    /// Of a STRING: its characters, no more than its declared length.
    Str(Vec<u8>),
    /// Of an array of STRING: each element's characters, as `Elements`.
    Texts(Vec<Vec<u8>>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Var {
    pub name: String,
    pub ty: Ty,
    pub section: Section,
    /// The retentive qualifier of the section it is declared in.
    #[serde(default)]
    pub retention: Retention,
    pub init: Option<Init>,
}

/// A structure type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StructTy {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    /// What the member starts with in every variable of the structure.
    pub init: Option<Init>,
}

/// An array type: its subscript ranges, each inclusive, and what it holds. With several
/// ranges the rightmost subscript varies fastest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArrayTy {
    pub dims: Vec<(i64, i64)>,
    pub elem: Ty,
}

impl ArrayTy {
    /// How many elements it holds.
    pub fn len(&self) -> u64 {
        self.dims
            .iter()
            .map(|(lo, hi)| (hi - lo + 1) as u64)
            .product()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One step from a variable to a part of it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Sel {
    /// A member: an index into the structure's fields, the standard function block's
    /// `members()`, or the user function block's `vars`.
    Member(usize),
    /// An element: one subscript per range of the array, each an expression of an integer type.
    Index(Vec<Expr>),
}

/// Where a value is read or written: a variable of the POU the place is in, or a part of it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Place {
    /// An index into the `vars` of the POU.
    pub var: usize,
    pub path: Vec<Sel>,
}

impl Place {
    pub fn var(var: usize) -> Place {
        Place {
            var,
            path: Vec::new(),
        }
    }
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

/// A function of the standard library, called in an expression. Each takes its arguments in
/// the order the standard lists its inputs, every one given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StdFn {
    /// IEC 61131-3 ed. 3 Table 34, the character string functions. `L` and `P` are of an
    /// integer type; the result of each that gives a STRING is the expression's type, long
    /// enough for any result of its arguments, up to [`MAX_STRING_LENGTH`].
    /// `LEN(IN)`: an INT.
    Len,
    /// `LEFT(IN, L)`
    Left,
    /// `RIGHT(IN, L)`
    Right,
    /// `MID(IN, L, P)`
    Mid,
    /// `CONCAT(IN1, IN2, …)`: two or more.
    Concat,
    /// `INSERT(IN1, IN2, P)`
    Insert,
    /// `DELETE(IN, L, P)`
    Delete,
    /// `REPLACE(IN1, IN2, L, P)`
    Replace,
    /// `FIND(IN1, IN2)`: an INT.
    Find,
    // Table 28: of one number, the result of its type. ABS of any number; the others of a
    // REAL or an LREAL.
    Abs,
    Sqrt,
    Ln,
    Log,
    Exp,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    /// `ATAN2(Y, X)`, of one real type.
    Atan2,
    /// `EXPT(IN1, IN2)` (Table 29): IN1 a real, the result of its type; IN2 a real of that
    /// type, or a LINT.
    Expt,
    /// `TRUNC_<int>(IN)` (Table 22 feature 2c): a real toward zero, as the expression's
    /// integer type.
    Trunc,
    // Table 32: of one elementary type, the result of it.
    /// `SEL(G, IN0, IN1)`: G a BOOL.
    Sel,
    /// `MAX(IN1, IN2, …)`: two or more.
    Max,
    /// `MIN(IN1, IN2, …)`: two or more.
    Min,
    /// `LIMIT(MN, IN, MX)`: `MIN(MAX(IN, MN), MX)`.
    Limit,
    /// `MUX(K, IN0, IN1, …)`: K a LINT; two or more inputs.
    Mux,
}

impl StdFn {
    pub const ALL: [StdFn; 28] = [
        StdFn::Len,
        StdFn::Left,
        StdFn::Right,
        StdFn::Mid,
        StdFn::Concat,
        StdFn::Insert,
        StdFn::Delete,
        StdFn::Replace,
        StdFn::Find,
        StdFn::Abs,
        StdFn::Sqrt,
        StdFn::Ln,
        StdFn::Log,
        StdFn::Exp,
        StdFn::Sin,
        StdFn::Cos,
        StdFn::Tan,
        StdFn::Asin,
        StdFn::Acos,
        StdFn::Atan,
        StdFn::Atan2,
        StdFn::Expt,
        StdFn::Trunc,
        StdFn::Sel,
        StdFn::Max,
        StdFn::Min,
        StdFn::Limit,
        StdFn::Mux,
    ];

    pub fn name(self) -> &'static str {
        match self {
            StdFn::Len => "LEN",
            StdFn::Left => "LEFT",
            StdFn::Right => "RIGHT",
            StdFn::Mid => "MID",
            StdFn::Concat => "CONCAT",
            StdFn::Insert => "INSERT",
            StdFn::Delete => "DELETE",
            StdFn::Replace => "REPLACE",
            StdFn::Find => "FIND",
            StdFn::Abs => "ABS",
            StdFn::Sqrt => "SQRT",
            StdFn::Ln => "LN",
            StdFn::Log => "LOG",
            StdFn::Exp => "EXP",
            StdFn::Sin => "SIN",
            StdFn::Cos => "COS",
            StdFn::Tan => "TAN",
            StdFn::Asin => "ASIN",
            StdFn::Acos => "ACOS",
            StdFn::Atan => "ATAN",
            StdFn::Atan2 => "ATAN2",
            StdFn::Expt => "EXPT",
            StdFn::Trunc => "TRUNC",
            StdFn::Sel => "SEL",
            StdFn::Max => "MAX",
            StdFn::Min => "MIN",
            StdFn::Limit => "LIMIT",
            StdFn::Mux => "MUX",
        }
    }

    /// One of Table 34, on strings.
    pub fn is_string_function(self) -> bool {
        matches!(
            self,
            StdFn::Len
                | StdFn::Left
                | StdFn::Right
                | StdFn::Mid
                | StdFn::Concat
                | StdFn::Insert
                | StdFn::Delete
                | StdFn::Replace
                | StdFn::Find
        )
    }

    /// Takes as many inputs as it is given, two or more (CONCAT, MAX, MIN; MUX's after K).
    pub fn is_extensible(self) -> bool {
        matches!(self, StdFn::Concat | StdFn::Max | StdFn::Min | StdFn::Mux)
    }

    /// The names of its inputs, in order (Table 34). CONCAT's are IN1, IN2, … however many.
    pub fn inputs(self) -> &'static [&'static str] {
        match self {
            StdFn::Len => &["IN"],
            StdFn::Left | StdFn::Right => &["IN", "L"],
            StdFn::Mid | StdFn::Delete => &["IN", "L", "P"],
            StdFn::Concat => &["IN1", "IN2"],
            StdFn::Insert => &["IN1", "IN2", "P"],
            StdFn::Replace => &["IN1", "IN2", "L", "P"],
            StdFn::Find => &["IN1", "IN2"],
            StdFn::Abs
            | StdFn::Sqrt
            | StdFn::Ln
            | StdFn::Log
            | StdFn::Exp
            | StdFn::Sin
            | StdFn::Cos
            | StdFn::Tan
            | StdFn::Asin
            | StdFn::Acos
            | StdFn::Atan
            | StdFn::Trunc => &["IN"],
            StdFn::Atan2 => &["Y", "X"],
            StdFn::Expt | StdFn::Max | StdFn::Min => &["IN1", "IN2"],
            StdFn::Sel => &["G", "IN0", "IN1"],
            StdFn::Limit => &["MN", "IN", "MX"],
            StdFn::Mux => &["K", "IN0", "IN1"],
        }
    }

    /// By its name; TRUNC is named only with its result type (`TRUNC_INT`), not alone.
    pub fn by_name(upper: &str) -> Option<StdFn> {
        StdFn::ALL
            .into_iter()
            .find(|f| f.name() == upper && *f != StdFn::Trunc)
    }
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
    /// A character string literal: its characters, each one byte. Its type is a STRING of
    /// exactly that length.
    Str(Vec<u8>),
    /// The value at a place. An expression whose type is a structure or an array is always
    /// this: a whole variable or part, copied where it is assigned or passed.
    Load(Place),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    /// Both operands have the expression's type, except TIME * or / an integer (right is LINT).
    Arith(Arith, Box<Expr>, Box<Expr>),
    /// Both operands have the same type; the result is BOOL.
    Cmp(Cmp, Box<Expr>, Box<Expr>),
    Logic(Logic, Box<Expr>, Box<Expr>),
    /// The operand's value as the expression's type. Between an integer type and STRING it is
    /// the integer's decimal text (IEC 61131-3 ed. 3 Table 22 footnote e: the external
    /// representation of the value).
    Convert(Box<Expr>),
    /// A function of the standard library; see [`StdFn`] for what each takes.
    Std(StdFn, Vec<Expr>),
    /// A call of the function `pou`, giving its result. Each argument names the input it sets
    /// (an index into the function's `vars`); an input not named starts at its initial value
    /// (IEC 61131-3 ed. 3 §6.6.1.4.2). The function's other variables start at theirs at
    /// every call: a function keeps nothing between calls (§6.6.2.1).
    Call {
        pou: usize,
        args: Vec<(usize, Expr)>,
    },
}

/// The labels of one CASE arm, as inclusive ranges of the selector's (integer) type, and its
/// statements.
pub type CaseArm = (Vec<(i64, i64)>, Vec<Stmt>);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Stmt {
    /// The value has the place's type. For a structure or an array it is a `Load`, and every
    /// element is copied. For a STRING it is a STRING of any length, cut to the place's
    /// length if it is longer (§6.6.1.2.2 leaves that to the implementer).
    Assign(Place, Expr),
    /// Set the inputs given, run the function block instance at `instance`, copy the outputs
    /// asked for. The indices are into the block's members (as [`Sel::Member`]). An input not
    /// given keeps what it had (§6.6.3.4.1).
    Call {
        instance: Place,
        inputs: Vec<(usize, Expr)>,
        outputs: Vec<(usize, Place)>,
    },
    If(Vec<(Expr, Vec<Stmt>)>, Vec<Stmt>),
    /// Arms, then the ELSE statements.
    Case(Expr, Vec<CaseArm>, Vec<Stmt>),
    For {
        /// A variable or a member, never an array element.
        var: Place,
        from: Expr,
        to: Expr,
        by: i64,
        body: Vec<Stmt>,
    },
    While(Expr, Vec<Stmt>),
    Repeat(Vec<Stmt>, Expr),
    Exit,
    /// Leave the POU: in the program, the scan ends.
    Return,
}

/// What kind of program organization unit a [`Pou`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PouKind {
    /// The entry: its body is one scan.
    Program,
    /// A type; every variable declared with it is an instance, with its own copy of `vars`,
    /// kept from one call to the next.
    FunctionBlock,
    /// Called in expressions. `result` is the variable (an index into `vars`, named as the
    /// function) whose value the call gives. Its `vars` are not kept between calls.
    Function { result: usize },
}

/// A program organization unit: a program, a function block type or a function.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pou {
    pub name: String,
    pub kind: PouKind,
    pub vars: Vec<Var>,
    pub body: Vec<Stmt>,
}

/// Everything one source declares.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Unit {
    /// The [`IR_VERSION`] this was made for.
    pub ir_version: u32,
    pub structs: Vec<StructTy>,
    pub arrays: Vec<ArrayTy>,
    pub pous: Vec<Pou>,
    /// The program: an index into `pous`.
    pub entry: usize,
}

impl Unit {
    pub fn new() -> Unit {
        Unit {
            ir_version: IR_VERSION,
            structs: Vec::new(),
            arrays: Vec::new(),
            pous: Vec::new(),
            entry: 0,
        }
    }

    /// The program.
    pub fn program(&self) -> &Pou {
        &self.pous[self.entry]
    }

    /// The array type with these bounds and elements: the one already there, or a new one.
    pub fn array(&mut self, dims: Vec<(i64, i64)>, elem: Ty) -> Ty {
        let wanted = ArrayTy { dims, elem };
        let at = self
            .arrays
            .iter()
            .position(|a| *a == wanted)
            .unwrap_or_else(|| {
                self.arrays.push(wanted);
                self.arrays.len() - 1
            });
        Ty::Array(at)
    }

    /// A type's name as a program writes it.
    pub fn ty_name(&self, ty: Ty) -> String {
        match ty {
            Ty::Struct(i) => self.structs[i].name.clone(),
            Ty::Instance(p) => self.pous[p].name.clone(),
            Ty::Array(a) => {
                let a = &self.arrays[a];
                let dims: Vec<String> = a
                    .dims
                    .iter()
                    .map(|(lo, hi)| format!("{lo}..{hi}"))
                    .collect();
                format!("ARRAY [{}] OF {}", dims.join(", "), self.ty_name(a.elem))
            }
            Ty::String(n) => format!("STRING[{n}]"),
            other => other.builtin_name().unwrap_or("?").to_string(),
        }
    }

    /// The type of member `member` of a value of type `of`: a structure's field, or a member
    /// of a function block instance. `None` when `of` has no members or not that one.
    pub fn member_ty(&self, of: Ty, member: usize) -> Option<Ty> {
        match of {
            Ty::Struct(i) => self.structs.get(i)?.fields.get(member).map(|f| f.ty),
            Ty::Fb(k) => k.members().get(member).map(|m| m.ty),
            Ty::Instance(p) => self.pous.get(p)?.vars.get(member).map(|v| v.ty),
            _ => None,
        }
    }

    /// The type of what `place` names, in the POU `pou`.
    pub fn place_ty(&self, pou: usize, place: &Place) -> Ty {
        let mut ty = self.pous[pou].vars[place.var].ty;
        for sel in &place.path {
            ty = match (sel, ty) {
                (Sel::Member(m), of) => self
                    .member_ty(of, *m)
                    .expect("a place names a member its type has"),
                (Sel::Index(_), Ty::Array(a)) => self.arrays[a].elem,
                (Sel::Index(_), _) => panic!("a place subscripts what is not an array"),
            };
        }
        ty
    }

    /// The type within any arrays: `ty` itself when it is not an array.
    pub fn innermost(&self, ty: Ty) -> Ty {
        let mut ty = ty;
        while let Ty::Array(a) = ty {
            ty = self.arrays[a].elem;
        }
        ty
    }

    /// The retention of `member`, a variable of a function block type, in an instance of the
    /// type whose own retention is `instance`.
    ///
    /// IEC 61131-3 ed. 3 §6.5.6.2: every variable of a RETAIN or NON_RETAIN instance takes its
    /// qualifier, except one the type declares RETAIN or NON_RETAIN itself, and one that is a
    /// function block instance, which follows its own type's declarations. An array of
    /// instances is instances too: each element follows its type. In an unqualified instance,
    /// every member follows the type's declarations.
    pub fn member_retention(&self, instance: Retention, member: &Var) -> Retention {
        if member.retention != Retention::Unqualified {
            member.retention
        } else if matches!(self.innermost(member.ty), Ty::Fb(_) | Ty::Instance(_)) {
            Retention::Unqualified
        } else {
            instance
        }
    }

    /// Whether a value of `ty`, declared with `retention`, keeps its value across a warm
    /// restart (IEC 61131-3 ed. 3 §6.5.6): `Some` when all of it does or none of it does,
    /// `None` when only part of it does. Only an array of function block instances can be in
    /// part: one whose type declares a member RETAIN or NON_RETAIN, or holds an instance
    /// (which follows its own type, [`Unit::member_retention`]).
    pub fn retained(&self, ty: Ty, retention: Retention) -> Option<bool> {
        let mut seen = [false; 2];
        self.retained_parts(ty, retention, &mut seen);
        match seen {
            [true, true] => None,
            [false, true] => Some(true),
            [true, false] => Some(false),
            // Nothing in it: an instance of a block without variables.
            [false, false] => Some(retention.retained()),
        }
    }

    /// Marks in `seen` whether some elementary part of a value of `ty` is kept (`seen[1]`)
    /// and whether some part is not (`seen[0]`).
    fn retained_parts(&self, ty: Ty, retention: Retention, seen: &mut [bool; 2]) {
        match ty {
            Ty::Array(a) => self.retained_parts(self.arrays[a].elem, retention, seen),
            // §6.5.6.2: every element of a structure, nested ones too, takes its qualifier.
            Ty::Struct(i) => {
                for f in &self.structs[i].fields {
                    self.retained_parts(f.ty, retention, seen);
                }
            }
            Ty::Instance(p) => {
                for v in &self.pous[p].vars {
                    self.retained_parts(v.ty, self.member_retention(retention, v), seen);
                }
            }
            // A standard function block declares no retention of its own: its members take
            // the instance's.
            _ => seen[retention.retained() as usize] = true,
        }
    }

    /// Every array among the program's variables, at any depth, that would be kept across a
    /// warm restart in part ([`Unit::retained`] is `None`). An array is one symbol, kept or
    /// not as a whole, so such an array cannot be laid out. Each is given by its path from
    /// the program and where it is declared: a POU and the index of its variable.
    pub fn arrays_retained_in_part(&self) -> Vec<(String, usize, usize)> {
        let mut out = Vec::new();
        for (k, v) in self.program().vars.iter().enumerate() {
            self.find_retained_in_part(v.ty, v.retention, &v.name, (self.entry, k), &mut out);
        }
        out
    }

    fn find_retained_in_part(
        &self,
        ty: Ty,
        retention: Retention,
        path: &str,
        decl: (usize, usize),
        out: &mut Vec<(String, usize, usize)>,
    ) {
        match ty {
            Ty::Array(_) => {
                if self.retained(ty, retention).is_none() {
                    out.push((path.to_string(), decl.0, decl.1));
                }
            }
            Ty::Struct(i) => {
                for f in &self.structs[i].fields {
                    let path = format!("{path}.{}", f.name);
                    self.find_retained_in_part(f.ty, retention, &path, decl, out);
                }
            }
            Ty::Instance(p) => {
                for (k, v) in self.pous[p].vars.iter().enumerate() {
                    let path = format!("{path}.{}", v.name);
                    let retention = self.member_retention(retention, v);
                    self.find_retained_in_part(v.ty, retention, &path, (p, k), out);
                }
            }
            _ => {}
        }
    }

    /// Whether a value of `ty` holds a function block instance, at any depth.
    pub fn holds_instance(&self, ty: Ty) -> bool {
        match ty {
            Ty::Fb(_) | Ty::Instance(_) => true,
            Ty::Array(a) => self.holds_instance(self.arrays[a].elem),
            Ty::Struct(i) => self.structs[i]
                .fields
                .iter()
                .any(|f| self.holds_instance(f.ty)),
            _ => false,
        }
    }

    /// The POUs `pou` cannot exist or run without: the functions its body calls, and the
    /// function block types it holds an instance of (in its variables, at any depth).
    pub fn uses(&self, pou: usize) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        let mut add = |p: usize| {
            if !out.contains(&p) {
                out.push(p)
            }
        };
        fn in_ty(u: &Unit, ty: Ty, add: &mut dyn FnMut(usize)) {
            match ty {
                Ty::Instance(p) => add(p),
                Ty::Array(a) => in_ty(u, u.arrays[a].elem, add),
                Ty::Struct(i) => u.structs[i].fields.iter().for_each(|f| in_ty(u, f.ty, add)),
                _ => {}
            }
        }
        for v in &self.pous[pou].vars {
            in_ty(self, v.ty, &mut add);
        }
        walk_exprs(&self.pous[pou].body, &mut |e| {
            if let ExprKind::Call { pou, .. } = &e.kind {
                add(*pou)
            }
        });
        out
    }

    /// Every POU, each after all those it [`uses`](Unit::uses), the program included. `Err`
    /// is a cycle: POUs each using the next, and the last the first. Structures that contain
    /// themselves are a cycle too, reported by [`Unit::struct_cycle`].
    pub fn order(&self) -> Result<Vec<usize>, Vec<usize>> {
        let mut out = Vec::new();
        depth_first(self.pous.len(), |p| self.uses(p), |p| out.push(p))?;
        Ok(out)
    }

    /// A structure that contains itself, directly or through others: the structures on the
    /// way round. Such a type has no size.
    pub fn struct_cycle(&self) -> Option<Vec<usize>> {
        let held = |s: usize| {
            let mut next = Vec::new();
            for f in &self.structs[s].fields {
                // Through arrays, not through structures.
                if let Ty::Struct(i) = self.innermost(f.ty) {
                    next.push(i);
                }
            }
            next
        };
        depth_first(self.structs.len(), held, |_| {}).err()
    }

    /// Every elementary type and standard function block the unit uses, for checking it
    /// against a target's profile.
    pub fn types_used(&self) -> Vec<Ty> {
        let mut out: Vec<Ty> = Vec::new();
        let mut add = |t: Ty| {
            if t.builtin_name().is_some() && !out.contains(&t) {
                out.push(t)
            }
        };
        fn in_ty(u: &Unit, ty: Ty, add: &mut dyn FnMut(Ty)) {
            match ty {
                Ty::Fb(k) => {
                    add(ty);
                    k.members().iter().for_each(|m| add(m.ty));
                }
                Ty::Array(a) => in_ty(u, u.arrays[a].elem, add),
                Ty::Struct(_) | Ty::Instance(_) => {}
                // A kind of type: one entry for every length.
                Ty::String(_) => add(Ty::String(DEFAULT_STRING_LENGTH)),
                t => add(t),
            }
        }
        for s in &self.structs {
            for f in &s.fields {
                in_ty(self, f.ty, &mut add);
            }
        }
        for p in &self.pous {
            for v in &p.vars {
                in_ty(self, v.ty, &mut add);
            }
            walk_exprs(&p.body, &mut |e| in_ty(self, e.ty, &mut add));
        }
        out
    }
}

impl Default for Unit {
    fn default() -> Self {
        Unit::new()
    }
}

/// Visit the nodes `0..n` of a graph depth first, each after those `next` leads to from it,
/// calling `done` on each once all those are done. `Err` is a cycle: the nodes on the way
/// round, from the first reached twice. Iterative, so that a long chain of nodes (types each
/// holding the next, functions each calling the next) cannot exhaust the stack.
fn depth_first(
    n: usize,
    next: impl Fn(usize) -> Vec<usize>,
    mut done: impl FnMut(usize),
) -> Result<(), Vec<usize>> {
    // 0 not seen, 1 being visited, 2 done.
    let mut state = vec![0u8; n];
    for root in 0..n {
        if state[root] == 2 {
            continue;
        }
        state[root] = 1;
        // The way from `root`: each node, what it leads to, and how many of those are taken.
        let mut trail = vec![(root, next(root), 0usize)];
        while let Some(top) = trail.last_mut() {
            let Some(&q) = top.1.get(top.2) else {
                let p = top.0;
                trail.pop();
                state[p] = 2;
                done(p);
                continue;
            };
            top.2 += 1;
            match state[q] {
                2 => {}
                1 => {
                    let from = trail.iter().position(|t| t.0 == q).unwrap_or(0);
                    return Err(trail[from..].iter().map(|t| t.0).collect());
                }
                _ => {
                    state[q] = 1;
                    trail.push((q, next(q), 0));
                }
            }
        }
    }
    Ok(())
}

/// Call `f` on every expression in `stmts`, the ones inside others and inside places included.
pub fn walk_exprs(stmts: &[Stmt], f: &mut dyn FnMut(&Expr)) {
    fn in_place(p: &Place, f: &mut dyn FnMut(&Expr)) {
        for sel in &p.path {
            if let Sel::Index(subscripts) = sel {
                subscripts.iter().for_each(|e| in_expr(e, f));
            }
        }
    }
    fn in_expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
        f(e);
        match &e.kind {
            ExprKind::Neg(a) | ExprKind::Not(a) | ExprKind::Convert(a) => in_expr(a, f),
            ExprKind::Arith(_, a, b) | ExprKind::Cmp(_, a, b) | ExprKind::Logic(_, a, b) => {
                in_expr(a, f);
                in_expr(b, f);
            }
            ExprKind::Load(p) => in_place(p, f),
            ExprKind::Call { args, .. } => args.iter().for_each(|(_, a)| in_expr(a, f)),
            ExprKind::Std(_, args) => args.iter().for_each(|a| in_expr(a, f)),
            ExprKind::Const(_) | ExprKind::Str(_) => {}
        }
    }
    for st in stmts {
        match st {
            Stmt::Assign(p, e) => {
                in_place(p, f);
                in_expr(e, f);
            }
            Stmt::Call {
                instance,
                inputs,
                outputs,
            } => {
                in_place(instance, f);
                inputs.iter().for_each(|(_, e)| in_expr(e, f));
                outputs.iter().for_each(|(_, p)| in_place(p, f));
            }
            Stmt::If(branches, otherwise) => {
                for (c, b) in branches {
                    in_expr(c, f);
                    walk_exprs(b, f);
                }
                walk_exprs(otherwise, f);
            }
            Stmt::Case(sel, arms, otherwise) => {
                in_expr(sel, f);
                arms.iter().for_each(|(_, b)| walk_exprs(b, f));
                walk_exprs(otherwise, f);
            }
            Stmt::For {
                var,
                from,
                to,
                body,
                ..
            } => {
                in_place(var, f);
                in_expr(from, f);
                in_expr(to, f);
                walk_exprs(body, f);
            }
            Stmt::While(c, b) | Stmt::Repeat(b, c) => {
                in_expr(c, f);
                walk_exprs(b, f);
            }
            Stmt::Exit | Stmt::Return => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pou(name: &str, kind: PouKind, vars: Vec<Var>, body: Vec<Stmt>) -> Pou {
        Pou {
            name: name.into(),
            kind,
            vars,
            body,
        }
    }
    fn var(name: &str, ty: Ty) -> Var {
        Var {
            name: name.into(),
            ty,
            section: Section::Var,
            retention: Retention::Unqualified,
            init: None,
        }
    }
    fn call(pou: usize) -> Stmt {
        Stmt::Assign(
            Place::var(0),
            Expr {
                ty: Ty::Int,
                kind: ExprKind::Call {
                    pou,
                    args: Vec::new(),
                },
            },
        )
    }
    fn function(name: &str, calls: &[usize]) -> Pou {
        pou(
            name,
            PouKind::Function { result: 0 },
            vec![var(name, Ty::Int)],
            calls.iter().map(|c| call(*c)).collect(),
        )
    }

    #[test]
    fn the_order_puts_every_pou_after_those_it_uses() {
        let mut u = Unit::new();
        // main(0) calls f(1), which calls g(2); main also holds an instance of block(3).
        u.pous.push(pou(
            "main",
            PouKind::Program,
            vec![var("x", Ty::Int), var("b", Ty::Instance(3))],
            vec![call(1)],
        ));
        u.pous.push(function("f", &[2]));
        u.pous.push(function("g", &[]));
        u.pous
            .push(pou("block", PouKind::FunctionBlock, vec![], vec![]));
        let order = u.order().unwrap();
        let at = |p: usize| order.iter().position(|x| *x == p).unwrap();
        assert!(at(2) < at(1) && at(1) < at(0) && at(3) < at(0), "{order:?}");
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn a_function_that_reaches_itself_is_a_cycle_naming_every_pou_on_the_way() {
        let mut u = Unit::new();
        u.pous.push(function("a", &[1]));
        u.pous.push(function("b", &[2]));
        u.pous.push(function("c", &[0]));
        assert_eq!(u.order(), Err(vec![0, 1, 2]));

        let mut u = Unit::new();
        u.pous.push(function("alone", &[0]));
        assert_eq!(u.order(), Err(vec![0]));
    }

    #[test]
    fn a_block_holding_an_instance_of_itself_through_an_array_is_a_cycle() {
        let mut u = Unit::new();
        let inner = u.array(vec![(1, 2)], Ty::Instance(0));
        u.pous.push(pou(
            "block",
            PouKind::FunctionBlock,
            vec![var("more", inner)],
            vec![],
        ));
        assert_eq!(u.order(), Err(vec![0]));
    }

    #[test]
    fn a_structure_containing_itself_is_found() {
        let mut u = Unit::new();
        let field = |name: &str, ty: Ty| Field {
            name: name.into(),
            ty,
            init: None,
        };
        u.structs.push(StructTy {
            name: "A".into(),
            fields: vec![field("b", Ty::Struct(1))],
        });
        let again = u.array(vec![(0, 1)], Ty::Struct(0));
        u.structs.push(StructTy {
            name: "B".into(),
            fields: vec![field("n", Ty::Int), field("a", again)],
        });
        assert_eq!(u.struct_cycle(), Some(vec![0, 1]));
        u.structs[1].fields.pop();
        assert_eq!(u.struct_cycle(), None);
    }

    #[test]
    fn two_arrays_with_the_same_bounds_and_elements_are_one_type() {
        let mut u = Unit::new();
        let a = u.array(vec![(1, 3)], Ty::Int);
        let b = u.array(vec![(1, 3)], Ty::Int);
        let c = u.array(vec![(0, 2)], Ty::Int);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(u.ty_name(c), "ARRAY [0..2] OF INT");
        let m = u.array(vec![(1, 2), (-1, 1)], Ty::Real);
        assert_eq!(u.ty_name(m), "ARRAY [1..2, -1..1] OF REAL");
    }
}
