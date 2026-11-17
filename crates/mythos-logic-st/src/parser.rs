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
const ENDS: [&str; 10] = [
    "END_PROGRAM",
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

    /// A source of exactly one PROGRAM.
    pub fn program(&mut self) -> R<Program> {
        for kw in [
            "FUNCTION_BLOCK",
            "FUNCTION",
            "TYPE",
            "CONFIGURATION",
            "INTERFACE",
        ] {
            if self.is_kw(kw) {
                return Err(Diagnostic::at(
                    self.span(),
                    format!("{kw} is not supported yet; this compiler takes one PROGRAM"),
                ));
            }
        }
        let span = self.span();
        self.expect_kw("PROGRAM")?;
        let (name, _) = self.ident()?;
        let mut vars = Vec::new();
        loop {
            let section = if self.is_kw("VAR") {
                Section::Var
            } else if self.is_kw("VAR_INPUT") {
                Section::Input
            } else if self.is_kw("VAR_OUTPUT") {
                Section::Output
            } else if let Tok::Word(u, w) = self.peek() {
                if u.starts_with("VAR_") {
                    return Err(Diagnostic::at(
                        self.span(),
                        format!("{w} is not supported yet"),
                    ));
                }
                break;
            } else {
                break;
            };
            self.bump();
            for qual in ["CONSTANT", "RETAIN", "PERSISTENT"] {
                if self.is_kw(qual) {
                    return Err(Diagnostic::at(
                        self.span(),
                        format!("{qual} is not supported yet"),
                    ));
                }
            }
            while !self.is_kw("END_VAR") {
                self.var_decls(section, &mut vars)?;
            }
            self.bump();
        }
        let body = self.stmts()?;
        self.expect_kw("END_PROGRAM")?;
        if !matches!(self.peek(), Tok::Eof) {
            return Err(Diagnostic::at(
                self.span(),
                format!(
                    "{} after END_PROGRAM; one PROGRAM per source for now",
                    self.describe()
                ),
            ));
        }
        Ok(Program {
            name,
            span,
            vars,
            body,
        })
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
        if self.is_kw("ARRAY")
            || self.is_kw("STRING")
            || self.is_kw("POINTER")
            || self.is_kw("REF_TO")
        {
            return Err(Diagnostic::at(
                self.span(),
                format!("{} is not supported yet", self.describe()),
            ));
        }
        let (type_name, type_span) = self.ident()?;
        let init = if self.is_sym(":=") {
            self.bump();
            Some(self.expr()?)
        } else {
            None
        };
        self.expect_sym(";")?;
        for (name, span) in names {
            out.push(VarDecl {
                name,
                span,
                section,
                type_name: type_name.clone(),
                type_span,
                init: init.clone(),
            });
        }
        Ok(())
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
                    member: None,
                    span: vspan,
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
        let (name, nspan) = self.ident()?;
        if self.is_sym("(") {
            self.bump();
            let mut args = Vec::new();
            while !self.is_sym(")") {
                let (pname, pspan) = self.ident()?;
                if self.is_sym(":=") {
                    self.bump();
                    args.push(Arg::In(pname, self.expr()?, pspan));
                } else if self.is_sym("=>") {
                    self.bump();
                    let (t, tspan) = self.ident()?;
                    let member = self.member()?;
                    args.push(Arg::Out(
                        pname,
                        Place {
                            name: t,
                            member,
                            span: tspan,
                        },
                        pspan,
                    ));
                } else {
                    return Err(Diagnostic::at(
                        self.span(),
                        "a call names each parameter: IN := …, or Q => …",
                    ));
                }
                if self.is_sym(",") {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect_sym(")")?;
            self.expect_sym(";")?;
            return Ok(Stmt::Call(name, args, nspan));
        }
        let member = self.member()?;
        self.expect_sym(":=")?;
        let value = self.expr()?;
        self.expect_sym(";")?;
        Ok(Stmt::Assign(
            Place {
                name,
                member,
                span: nspan,
            },
            value,
        ))
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

    fn member(&mut self) -> R<Option<String>> {
        if self.is_sym(".") {
            self.bump();
            Ok(Some(self.ident()?.0))
        } else if self.is_sym("[") {
            Err(Diagnostic::at(self.span(), "arrays are not supported yet"))
        } else {
            Ok(None)
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
                        args.push(self.expr()?);
                        if self.is_sym(",") {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    self.expect_sym(")")?;
                    ExprKind::Call(name, args)
                } else {
                    let member = self.member()?;
                    ExprKind::Var(Place {
                        name,
                        member,
                        span: nspan,
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
