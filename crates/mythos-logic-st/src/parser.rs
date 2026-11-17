//! Structured Text to a syntax tree. Recursive descent; operator precedence as in IEC 61131-3
//! ed. 3 Table 71, lowest first: OR; XOR; AND, &; = <> < > <= >=; + -; * / MOD; unary - NOT;
//! call. Operators of one precedence apply left to right.

use crate::ast::*;
use crate::lexer::{Tok, Token};
use mythos_logic_ir::{Diagnostic, Span};

pub struct Parser {
    toks: Vec<Token>,
    at: usize,
}

type R<T> = Result<T, Diagnostic>;

/// Statement keywords that end a statement list.
const ENDS: [&str; 11] = [
    "END_PROGRAM",
    "END_FUNCTION",
    "END_IF",
    "ELSIF",
    "ELSE",
    "END_CASE",
    "END_FOR",
    "END_WHILE",
    "UNTIL",
    "END_REPEAT",
    "END_FUNCTION_BLOCK",
];

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, at: 0 }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.at].tok
    }
    fn span(&self) -> Span {
        self.toks[self.at].span
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.at].clone();
        if self.at + 1 < self.toks.len() {
            self.at += 1;
        }
        t
    }
    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Word(u, _) if u == kw)
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if *x == s)
    }
    fn describe(&self) -> String {
        match self.peek() {
            Tok::Word(_, w) => format!("'{w}'"),
            Tok::Int(v) => format!("the number {v}"),
            Tok::Real(v) => format!("the number {v}"),
            Tok::Time(_) => "a duration".into(),
            Tok::Sym(s) => format!("'{s}'"),
            Tok::Eof => "the end of the source".into(),
        }
    }
    fn expect_kw(&mut self, kw: &str) -> R<()> {
        if self.is_kw(kw) {
            self.bump();
            Ok(())
        } else {
            Err(Diagnostic::at(
                self.span(),
                format!("expected {kw}, found {}", self.describe()),
            ))
        }
    }
    fn expect_sym(&mut self, s: &str) -> R<()> {
        if self.is_sym(s) {
            self.bump();
            Ok(())
        } else {
            Err(Diagnostic::at(
                self.span(),
                format!("expected '{s}', found {}", self.describe()),
            ))
        }
    }
    fn ident(&mut self) -> R<(String, Span)> {
        let span = self.span();
        match self.peek().clone() {
            Tok::Word(_, w) => {
                self.bump();
                Ok((w, span))
            }
            _ => Err(Diagnostic::at(
                span,
                format!("expected a name, found {}", self.describe()),
            )),
        }
    }

    fn not_yet<T>(&self, what: impl std::fmt::Display) -> R<T> {
        Err(Diagnostic::at(
            self.span(),
            format!("{what} is not supported yet"),
        ))
    }
    /// The token after the next one.
    fn peek2(&self) -> Option<&Tok> {
        self.toks.get(self.at + 1).map(|t| &t.tok)
    }

    /// A source: any number of TYPE blocks, functions and function block types, and the
    /// program, in any order.
    pub fn source(&mut self) -> R<Source> {
        let mut src = Source::default();
        loop {
            if matches!(self.peek(), Tok::Eof) {
                return Ok(src);
            }
            if self.is_kw("TYPE") {
                self.types(&mut src.types)?;
            } else if self.is_kw("PROGRAM") {
                src.pous.push(self.pou(PouKind::Program)?);
            } else if self.is_kw("FUNCTION_BLOCK") {
                src.pous.push(self.pou(PouKind::FunctionBlock)?);
            } else if self.is_kw("FUNCTION") {
                src.pous.push(self.pou(PouKind::Function)?);
            } else {
                for kw in [
                    "CONFIGURATION",
                    "RESOURCE",
                    "INTERFACE",
                    "CLASS",
                    "NAMESPACE",
                    "USING",
                    "VAR_GLOBAL",
                    "METHOD",
                    "ACTION",
                ] {
                    if self.is_kw(kw) {
                        return self.not_yet(kw);
                    }
                }
                return Err(Diagnostic::at(
                    self.span(),
                    format!(
                        "expected PROGRAM, FUNCTION, FUNCTION_BLOCK or TYPE, found {}",
                        self.describe()
                    ),
                ));
            }
        }
    }

    /// `TYPE name : STRUCT … END_STRUCT; other : ARRAY […] OF …; END_TYPE`
    fn types(&mut self, out: &mut Vec<TypeDecl>) -> R<()> {
        self.expect_kw("TYPE")?;
        while !self.is_kw("END_TYPE") {
            let (name, span) = self.ident()?;
            self.expect_sym(":")?;
            let kind = if self.is_kw("STRUCT") {
                self.bump();
                let mut fields = Vec::new();
                while !self.is_kw("END_STRUCT") {
                    if matches!(self.peek(), Tok::Eof) {
                        self.expect_kw("END_STRUCT")?;
                    }
                    self.var_decls(Section::Var, &mut fields)?;
                }
                self.bump();
                TypeDeclKind::Struct(fields)
            } else if self.is_kw("ARRAY") {
                TypeDeclKind::Alias(self.type_spec()?)
            } else if self.is_sym("(") {
                return self.not_yet("an enumeration");
            } else {
                return Err(Diagnostic::at(
                    self.span(),
                    format!(
                        "a type derived from {} is not supported yet; TYPE takes STRUCT and ARRAY",
                        self.describe()
                    ),
                ));
            };
            if self.is_sym(":=") {
                return self.not_yet("an initial value on a type");
            }
            self.expect_sym(";")?;
            out.push(TypeDecl { name, span, kind });
        }
        self.bump();
        if self.is_sym(";") {
            self.bump();
        }
        Ok(())
    }

    /// A program, a function block type or a function, from its keyword to its END_.
    fn pou(&mut self, kind: PouKind) -> R<Pou> {
        let (keyword, end) = match kind {
            PouKind::Program => ("PROGRAM", "END_PROGRAM"),
            PouKind::FunctionBlock => ("FUNCTION_BLOCK", "END_FUNCTION_BLOCK"),
            PouKind::Function => ("FUNCTION", "END_FUNCTION"),
        };
        self.expect_kw(keyword)?;
        for kw in ["FINAL", "ABSTRACT"] {
            if self.is_kw(kw) {
                return self.not_yet(kw);
            }
        }
        let (name, span) = self.ident()?;
        let result = if kind == PouKind::Function {
            if !self.is_sym(":") {
                return Err(Diagnostic::at(
                    span,
                    format!(
                        "the function {name} has no result type; a function without a result is not supported yet"
                    ),
                ));
            }
            self.bump();
            Some(self.type_spec()?)
        } else {
            None
        };
        for kw in ["EXTENDS", "IMPLEMENTS"] {
            if self.is_kw(kw) {
                return self.not_yet(kw);
            }
        }
        let mut vars = Vec::new();
        loop {
            let section = if self.is_kw("VAR") {
                Section::Var
            } else if self.is_kw("VAR_INPUT") {
                Section::Input
            } else if self.is_kw("VAR_OUTPUT") {
                if kind == PouKind::Function {
                    return self.not_yet("VAR_OUTPUT on a function");
                }
                Section::Output
            } else if let Tok::Word(u, w) = self.peek() {
                if u.starts_with("VAR_") {
                    return self.not_yet(w);
                }
                break;
            } else {
                break;
            };
            self.bump();
            for qual in [
                "CONSTANT",
                "RETAIN",
                "NON_RETAIN",
                "PERSISTENT",
                "PUBLIC",
                "PRIVATE",
                "PROTECTED",
                "INTERNAL",
            ] {
                if self.is_kw(qual) {
                    return self.not_yet(qual);
                }
            }
            while !self.is_kw("END_VAR") {
                if matches!(self.peek(), Tok::Eof) {
                    self.expect_kw("END_VAR")?;
                }
                self.var_decls(section, &mut vars)?;
            }
            self.bump();
        }
        for kw in ["METHOD", "ACTION", "PROPERTY"] {
            if self.is_kw(kw) {
                return self.not_yet(kw);
            }
        }
        let body = self.stmts()?;
        self.expect_kw(end)?;
        Ok(Pou {
            kind,
            name,
            span,
            result,
            vars,
            body,
        })
    }

    /// A type where a declaration names one: a name, or `ARRAY [lo..hi, …] OF type`.
    fn type_spec(&mut self) -> R<TypeSpec> {
        let span = self.span();
        if self.is_kw("ARRAY") {
            self.bump();
            self.expect_sym("[")?;
            let mut dims = Vec::new();
            loop {
                if self.is_sym("*") {
                    return self.not_yet("an array of variable length (ARRAY [*])");
                }
                let lo = self.bound()?;
                self.expect_sym("..")?;
                let hi = self.bound()?;
                dims.push((lo, hi));
                if self.is_sym(",") {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect_sym("]")?;
            self.expect_kw("OF")?;
            let elem = self.type_spec()?;
            return Ok(TypeSpec {
                span,
                kind: TypeSpecKind::Array(dims, Box::new(elem)),
            });
        }
        if self.is_kw("STRUCT") {
            return Err(Diagnostic::at(
                span,
                "a STRUCT without a name is not supported yet: declare it in TYPE … END_TYPE and use its name",
            ));
        }
        if self.is_kw("STRING")
            || self.is_kw("WSTRING")
            || self.is_kw("POINTER")
            || self.is_kw("REF_TO")
        {
            return self.not_yet(self.describe());
        }
        let (name, _) = self.ident()?;
        if self.is_sym("(") {
            return self.not_yet(format!("a subrange ({name} (lo..hi))"));
        }
        Ok(TypeSpec {
            span,
            kind: TypeSpecKind::Named(name),
        })
    }

    /// A bound of an array's subscript range: an integer literal, which may be negative.
    fn bound(&mut self) -> R<i128> {
        let span = self.span();
        let negative = self.is_sym("-");
        if negative {
            self.bump();
        }
        match self.peek().clone() {
            Tok::Int(v) => {
                self.bump();
                Ok(if negative { -v } else { v })
            }
            _ => Err(Diagnostic::at(
                span,
                format!(
                    "an array bound must be an integer literal for now, found {}",
                    self.describe()
                ),
            )),
        }
    }

    /// `(name := …`: the start of an initialiser list of a structure or an instance.
    fn at_named_list(&self) -> bool {
        self.is_sym("(")
            && matches!(self.peek2(), Some(Tok::Word(..)))
            && matches!(
                self.toks.get(self.at + 2).map(|t| &t.tok),
                Some(Tok::Sym(":="))
            )
    }

    /// What follows `:=` in a declaration.
    fn init(&mut self) -> R<Init> {
        let span = self.span();
        if self.is_sym("[") {
            self.bump();
            let items = self.array_items("]")?;
            self.expect_sym("]")?;
            return Ok(Init::Array(items, span));
        }
        if self.at_named_list() {
            return self.not_yet(
                "an initialiser list (name := value, …) for a structure or a function block instance",
            );
        }
        Ok(Init::Value(self.expr()?))
    }

    /// The values of an array initialiser up to `close`: `1, 2, 3(0)`.
    fn array_items(&mut self, close: &str) -> R<Vec<ArrayInit>> {
        let mut items = Vec::new();
        while !self.is_sym(close) {
            let span = self.span();
            if let (Tok::Int(n), Some(Tok::Sym("("))) = (self.peek().clone(), self.peek2()) {
                self.bump();
                self.bump();
                let inner = self.array_items(")")?;
                self.expect_sym(")")?;
                items.push(ArrayInit::Repeat(n, inner, span));
            } else if self.is_sym("[") {
                return self.not_yet("an array initialiser inside another");
            } else if self.at_named_list() {
                return self.not_yet("an initialiser list (name := value, …) for a structure");
            } else {
                items.push(ArrayInit::Value(self.expr()?));
            }
            if self.is_sym(",") {
                self.bump();
            } else {
                break;
            }
        }
        Ok(items)
    }

    /// `a, b : INT := 5;`
    fn var_decls(&mut self, section: Section, out: &mut Vec<VarDecl>) -> R<()> {
        let mut names = vec![self.ident()?];
        while self.is_sym(",") {
            self.bump();
            names.push(self.ident()?);
        }
        if self.is_kw("AT") {
            return Err(Diagnostic::at(self.span(), "AT %I/%Q addresses are not used: bind a variable to a tag in the variable table instead"));
        }
        self.expect_sym(":")?;
        let ty = self.type_spec()?;
        let init = if self.is_sym(":=") {
            self.bump();
            Some(self.init()?)
        } else {
            None
        };
        self.expect_sym(";")?;
        for (name, span) in names {
            out.push(VarDecl {
                name,
                span,
                section,
                ty: ty.clone(),
                init: init.clone(),
            });
        }
        Ok(())
    }

    /// The steps after a name: `.member`, `[i, j]`, any number of them.
    fn selectors(&mut self) -> R<Vec<Selector>> {
        let mut path = Vec::new();
        loop {
            let span = self.span();
            if self.is_sym(".") {
                self.bump();
                let (member, mspan) = self.ident()?;
                path.push(Selector::Member(member, mspan));
            } else if self.is_sym("[") {
                self.bump();
                let mut subscripts = vec![self.expr()?];
                while self.is_sym(",") {
                    self.bump();
                    subscripts.push(self.expr()?);
                }
                self.expect_sym("]")?;
                path.push(Selector::Index(subscripts, span));
            } else {
                return Ok(path);
            }
        }
    }

    /// A variable or a part of one.
    fn place(&mut self) -> R<Place> {
        let (name, span) = self.ident()?;
        let path = self.selectors()?;
        Ok(Place { name, span, path })
    }

    fn stmts(&mut self) -> R<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            if matches!(self.peek(), Tok::Eof) || ENDS.iter().any(|e| self.is_kw(e)) {
                return Ok(out);
            }
            if self.is_sym(";") {
                self.bump();
                continue;
            }
            out.push(self.stmt()?);
        }
    }

    fn end_with(&mut self, kw: &str) -> R<()> {
        self.expect_kw(kw)?;
        if self.is_sym(";") {
            self.bump();
        }
        Ok(())
    }

    fn stmt(&mut self) -> R<Stmt> {
        let span = self.span();
        if self.is_kw("IF") {
            self.bump();
            let mut branches = Vec::new();
            let cond = self.expr()?;
            self.expect_kw("THEN")?;
            branches.push((cond, self.stmts()?));
            let mut otherwise = None;
            loop {
                if self.is_kw("ELSIF") {
                    self.bump();
                    let c = self.expr()?;
                    self.expect_kw("THEN")?;
                    branches.push((c, self.stmts()?));
                } else if self.is_kw("ELSE") {
                    self.bump();
                    otherwise = Some(self.stmts()?);
                } else {
                    break;
                }
            }
            self.end_with("END_IF")?;
            return Ok(Stmt::If(branches, otherwise));
        }
        if self.is_kw("CASE") {
            self.bump();
            let sel = self.expr()?;
            self.expect_kw("OF")?;
            let mut arms = Vec::new();
            let mut otherwise = None;
            loop {
                if self.is_kw("ELSE") {
                    self.bump();
                    otherwise = Some(self.case_body()?);
                    break;
                }
                if self.is_kw("END_CASE") {
                    break;
                }
                let mut labels = Vec::new();
                loop {
                    let lo = self.expr()?;
                    if self.is_sym("..") {
                        self.bump();
                        labels.push(CaseLabel::Range(lo, self.expr()?));
                    } else {
                        labels.push(CaseLabel::One(lo));
                    }
                    if self.is_sym(",") {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect_sym(":")?;
                arms.push((labels, self.case_body()?));
            }
            self.end_with("END_CASE")?;
            return Ok(Stmt::Case(sel, arms, otherwise));
        }
        if self.is_kw("FOR") {
            self.bump();
            let (name, vspan) = self.ident()?;
            if self.is_sym(".") || self.is_sym("[") {
                return self
                    .not_yet("a FOR variable that is a member or an element (name it alone)");
            }
            self.expect_sym(":=")?;
            let from = self.expr()?;
            self.expect_kw("TO")?;
            let to = self.expr()?;
            let by = if self.is_kw("BY") {
                self.bump();
                Some(self.expr()?)
            } else {
                None
            };
            self.expect_kw("DO")?;
            let body = self.stmts()?;
            self.end_with("END_FOR")?;
            return Ok(Stmt::For {
                var: Place {
                    name,
                    span: vspan,
                    path: Vec::new(),
                },
                from,
                to,
                by,
                body,
            });
        }
        if self.is_kw("WHILE") {
            self.bump();
            let c = self.expr()?;
            self.expect_kw("DO")?;
            let body = self.stmts()?;
            self.end_with("END_WHILE")?;
            return Ok(Stmt::While(c, body));
        }
        if self.is_kw("REPEAT") {
            self.bump();
            let body = self.stmts()?;
            self.expect_kw("UNTIL")?;
            let c = self.expr()?;
            self.end_with("END_REPEAT")?;
            return Ok(Stmt::Repeat(body, c));
        }
        if self.is_kw("EXIT") {
            self.bump();
            self.expect_sym(";")?;
            return Ok(Stmt::Exit(span));
        }
        if self.is_kw("RETURN") {
            self.bump();
            self.expect_sym(";")?;
            return Ok(Stmt::Return(span));
        }
        for kw in ["CONTINUE", "METHOD", "ACTION", "VAR", "VAR_TEMP"] {
            if self.is_kw(kw) {
                return self.not_yet(if kw.starts_with("VAR") {
                    format!("{kw} after the statements have begun")
                } else {
                    kw.to_string()
                });
            }
        }
        let target = self.place()?;
        if self.is_sym("(") {
            self.bump();
            let mut args = Vec::new();
            while !self.is_sym(")") {
                match (self.peek(), self.peek2()) {
                    (Tok::Word(..), Some(Tok::Sym(":="))) => {
                        let (pname, pspan) = self.ident()?;
                        self.bump();
                        args.push(Arg::In(pname, self.expr()?, pspan));
                    }
                    (Tok::Word(..), Some(Tok::Sym("=>"))) => {
                        let (pname, pspan) = self.ident()?;
                        self.bump();
                        args.push(Arg::Out(pname, self.place()?, pspan));
                    }
                    // By position: the checker says what is wrong with it, knowing what
                    // is being called.
                    _ => args.push(Arg::Value(self.expr()?)),
                }
                if self.is_sym(",") {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect_sym(")")?;
            self.expect_sym(";")?;
            return Ok(Stmt::Call(target, args));
        }
        self.expect_sym(":=")?;
        let value = self.expr()?;
        self.expect_sym(";")?;
        Ok(Stmt::Assign(target, value))
    }

    /// The statements of one CASE arm: up to the next label, ELSE or END_CASE.
    fn case_body(&mut self) -> R<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            if self.is_kw("ELSE") || self.is_kw("END_CASE") || matches!(self.peek(), Tok::Eof) {
                return Ok(out);
            }
            if self.is_sym(";") {
                self.bump();
                continue;
            }
            // A label starts with a number, a minus sign or a name followed by ':' or ',' or '..'.
            let next = self.toks.get(self.at + 1).map(|t| t.tok.clone());
            let label_like = matches!(self.peek(), Tok::Int(_) | Tok::Sym("-"))
                || (matches!(self.peek(), Tok::Word(..))
                    && matches!(
                        next,
                        Some(Tok::Sym(":")) | Some(Tok::Sym(",")) | Some(Tok::Sym(".."))
                    ));
            if label_like {
                return Ok(out);
            }
            out.push(self.stmt()?);
        }
    }

    pub fn expr(&mut self) -> R<Expr> {
        self.binary(0)
    }

    fn binary(&mut self, level: usize) -> R<Expr> {
        // (operators of this level, by token)
        const LEVELS: [&[(&str, BinOp)]; 6] = [
            &[("OR", BinOp::Or)],
            &[("XOR", BinOp::Xor)],
            &[("AND", BinOp::And)],
            // Equality and comparison share one precedence (IEC 61131-3 ed. 3 Table 71,
            // precedence 4), applied left to right: `a < b = c` is `(a < b) = c`.
            &[
                ("=", BinOp::Eq),
                ("<>", BinOp::Ne),
                ("<", BinOp::Lt),
                (">", BinOp::Gt),
                ("<=", BinOp::Le),
                (">=", BinOp::Ge),
            ],
            &[("+", BinOp::Add), ("-", BinOp::Sub)],
            &[("*", BinOp::Mul), ("/", BinOp::Div), ("MOD", BinOp::Mod)],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        loop {
            let span = self.span();
            let op = LEVELS[level].iter().find(|(t, _)| match self.peek() {
                Tok::Sym(s) => s == t,
                Tok::Word(u, _) => u == t,
                _ => false,
            });
            let Some((_, op)) = op else { break };
            self.bump();
            let right = self.binary(level + 1)?;
            left = Expr {
                kind: ExprKind::Binary(*op, Box::new(left), Box::new(right)),
                span,
            };
        }
        if self.is_sym("**") {
            return Err(Diagnostic::at(
                self.span(),
                "** (power) is not supported yet",
            ));
        }
        Ok(left)
    }

    fn unary(&mut self) -> R<Expr> {
        let span = self.span();
        if self.is_sym("-") {
            self.bump();
            let inner = self.unary()?;
            // Fold a negative literal, so INT's -32768 is a value, not -(32768).
            let kind = match inner.kind {
                ExprKind::Int(v) => ExprKind::Int(-v),
                ExprKind::Real(v) => ExprKind::Real(-v),
                ExprKind::Time(v) => ExprKind::Time(-v),
                k => ExprKind::Neg(Box::new(Expr {
                    kind: k,
                    span: inner.span,
                })),
            };
            return Ok(Expr { kind, span });
        }
        if self.is_sym("+") {
            self.bump();
            return self.unary();
        }
        if self.is_kw("NOT") {
            self.bump();
            let inner = self.unary()?;
            return Ok(Expr {
                kind: ExprKind::Not(Box::new(inner)),
                span,
            });
        }
        self.primary()
    }

    fn primary(&mut self) -> R<Expr> {
        let span = self.span();
        let kind = match self.peek().clone() {
            Tok::Int(v) => {
                self.bump();
                ExprKind::Int(v)
            }
            Tok::Real(v) => {
                self.bump();
                ExprKind::Real(v)
            }
            Tok::Time(v) => {
                self.bump();
                ExprKind::Time(v)
            }
            Tok::Sym("(") => {
                self.bump();
                let e = self.expr()?;
                self.expect_sym(")")?;
                return Ok(e);
            }
            Tok::Word(u, _) if u == "TRUE" || u == "FALSE" => {
                self.bump();
                ExprKind::Bool(u == "TRUE")
            }
            Tok::Word(..) => {
                let (name, nspan) = self.ident()?;
                if self.is_sym("(") {
                    self.bump();
                    let mut args = Vec::new();
                    while !self.is_sym(")") {
                        // `name := value` names the input; a value alone goes by position.
                        let named = match (self.peek(), self.peek2()) {
                            (Tok::Word(..), Some(Tok::Sym(":="))) => true,
                            (Tok::Word(..), Some(Tok::Sym("=>"))) => {
                                return self.not_yet("an output of a function (name => variable)")
                            }
                            _ => false,
                        };
                        let name = if named {
                            let n = self.ident()?;
                            self.bump();
                            Some(n)
                        } else {
                            None
                        };
                        args.push(CallArg {
                            name,
                            value: self.expr()?,
                        });
                        if self.is_sym(",") {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    self.expect_sym(")")?;
                    if self.is_sym(".") || self.is_sym("[") {
                        return self.not_yet("a member or an element of a function's result");
                    }
                    ExprKind::Call(name, args)
                } else {
                    let path = self.selectors()?;
                    ExprKind::Var(Place {
                        name,
                        span: nspan,
                        path,
                    })
                }
            }
            _ => {
                return Err(Diagnostic::at(
                    span,
                    format!("expected a value, found {}", self.describe()),
                ))
            }
        };
        Ok(Expr { kind, span })
    }
}
