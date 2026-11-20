//! Structured Text to a syntax tree. Recursive descent; operator precedence as in IEC 61131-3
//! ed. 3 Table 71, lowest first: OR; XOR; AND, &; = <> < > <= >=; + -; * / MOD; unary - NOT;
//! call. Operators of one precedence apply left to right.
//!
//! **Depth.** Every pass over the tree is recursive, so the tree may not be deeper than
//! [`MAX_NESTING`]: a statement within statements, or a part of an expression within
//! operations, calls, subscripts and parentheses, deeper than that is refused where the limit
//! is reached, and the reading goes on after it as after any other mistake.
//!
//! **Recovery.** A rule that cannot go on says why once and gives up; whoever holds a list
//! (of statements, of declarations, of POUs) then moves to where the next item can start and
//! reads on, so one pass reports every independent mistake:
//!
//! - a statement is left at its `;`, or before the keyword that ends the list it is in;
//! - a statement with an `END_` of its own (IF, CASE, FOR, WHILE, REPEAT) whose head cannot
//!   be read is still read from the keyword that ends the head (THEN, OF, DO), so mistakes in
//!   the statements under it are found; if that keyword is not there, the statement is left
//!   at its own `END_`, the statements nested in it included;
//! - a declaration is left at its `;`, or before whatever ends its section;
//! - a POU whose head cannot be read is left at its `END_`.
//!
//! A mistake is told once. The token a rule stopped at is never complained about twice, and
//! text the lexer could not read ([`Tok::Error`]) is not complained about here at all. What
//! could not be read is marked in the tree (`partial`, `Unknown`, `lost`), so the checker
//! stays silent about what follows from the missing parts.

use crate::ast::*;
use crate::lexer::{Lexed, Tok, Token};
use mythos_logic_ir::{is_keyword, Diagnostic, Span, MAX_NESTING, TYPE_KEYWORDS};

pub struct Parser {
    toks: Vec<Token>,
    at: usize,
    /// The source ended inside something never closed, so nothing is said about what its
    /// end lacks.
    cut: bool,
    diags: Vec<Diagnostic>,
    /// How many times a rule gave up. A POU or a type during which it grew was not read
    /// whole.
    failures: usize,
    /// The token the last complaint was about: it is not complained about again.
    told_at: Option<usize>,
    /// The keywords the source declares as names, in upper case. Each is refused where it
    /// is declared; where it is used it stands as the name it was meant to be, so that the
    /// one mistake is told once.
    named_keywords: Vec<String>,
    /// How deep the construct being read is nested: the statements and the parts of an
    /// expression it is within (see [`MAX_NESTING`]).
    nest: usize,
    /// The height of the tree of the expression last read: 1 for a value alone, one more
    /// for each operation, call or subscript above it.
    height: usize,
}

/// What is said where a source nests deeper than [`MAX_NESTING`].
fn too_deep() -> String {
    format!(
        "nested more than {MAX_NESTING} deep: statements within statements, and operations, calls, subscripts, parentheses and arrays within one another, each count"
    )
}

/// A rule could not go on. What was wrong has been said.
#[derive(Debug)]
pub struct Failed;

type R<T> = Result<T, Failed>;

/// Keywords that begin a POU or a TYPE block: where the top level reads on.
const STARTS: [&str; 4] = ["PROGRAM", "FUNCTION", "FUNCTION_BLOCK", "TYPE"];

/// Keywords that end a POU.
const POU_ENDS: [&str; 3] = ["END_PROGRAM", "END_FUNCTION", "END_FUNCTION_BLOCK"];

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

/// The statements with an `END_` of their own: how each begins and ends.
const BLOCKS: [(&str, &str); 5] = [
    ("IF", "END_IF"),
    ("CASE", "END_CASE"),
    ("FOR", "END_FOR"),
    ("WHILE", "END_WHILE"),
    ("REPEAT", "END_REPEAT"),
];

/// What may stand at the top of a source and is not supported yet, with what ends it.
const UNSUPPORTED: [(&str, &str); 9] = [
    ("CONFIGURATION", "END_CONFIGURATION"),
    ("RESOURCE", "END_RESOURCE"),
    ("INTERFACE", "END_INTERFACE"),
    ("CLASS", "END_CLASS"),
    ("NAMESPACE", "END_NAMESPACE"),
    ("USING", ";"),
    ("VAR_GLOBAL", "END_VAR"),
    ("METHOD", "END_METHOD"),
    ("ACTION", "END_ACTION"),
];

/// A keyword that can follow the name of a POU, so that standing in the name's place it is
/// not a name: the name was left out.
/// Why a RETAIN or NON_RETAIN qualifier is not allowed on the section `keyword` of a POU of
/// `kind`; `None` where it may be (IEC 61131-3 ed. 3 §6.5.6.2 and Annex A: the static
/// VAR, VAR_INPUT and VAR_OUTPUT of programs and function blocks, and VAR_GLOBAL).
fn no_retention(kind: PouKind, keyword: &str) -> Option<&'static str> {
    Some(match keyword {
        "VAR_IN_OUT" => {
            "on VAR_IN_OUT: the variable is the caller's, and has no storage of its own to retain (IEC 61131-3 ed. 3 §6.5.6.2)"
        }
        "VAR_TEMP" => {
            "on VAR_TEMP: the variable starts afresh at each call, so there is nothing to retain (IEC 61131-3 ed. 3 Annex A, Temp_Var_Decls)"
        }
        "VAR_EXTERNAL" => {
            "on VAR_EXTERNAL: the variable is retained or not where VAR_GLOBAL declares it (IEC 61131-3 ed. 3 Annex A, External_Var_Decls)"
        }
        _ if kind == PouKind::Function => {
            "in a FUNCTION: a function keeps nothing from one call to the next, so there is nothing to retain (IEC 61131-3 ed. 3 §6.5.6.1, §6.6.2.1)"
        }
        _ => return None,
    })
}

fn follows_a_pou_name(upper: &str) -> bool {
    upper.starts_with("VAR")
        || upper.starts_with("END_")
        || STARTS.contains(&upper)
        || BLOCKS.iter().any(|(open, _)| upper == *open)
        || [
            "EXIT",
            "RETURN",
            "CONTINUE",
            "EXTENDS",
            "IMPLEMENTS",
            "METHOD",
            "ACTION",
            "USING",
        ]
        .contains(&upper)
}

/// The keywords `toks` declare as names: a keyword before `:` or `,` among declarations,
/// and one after PROGRAM, FUNCTION or FUNCTION_BLOCK that cannot follow a POU's name. This
/// looks at the whole source first because a name may be used before it is declared (a
/// function block's input, in a program written above the block).
fn named_keywords(toks: &[Token]) -> Vec<String> {
    let mut found = Vec::new();
    let mut declaring = false;
    for (i, t) in toks.iter().enumerate() {
        let Tok::Word(upper, _) = &t.tok else {
            continue;
        };
        let next = toks.get(i + 1).map(|t| &t.tok);
        if declaring && is_keyword(upper) && matches!(next, Some(Tok::Sym(":" | ","))) {
            found.push(upper.clone());
            continue;
        }
        match upper.as_str() {
            "TYPE" | "STRUCT" => declaring = true,
            "END_VAR" | "END_TYPE" => declaring = false,
            "PROGRAM" | "FUNCTION" | "FUNCTION_BLOCK" => {
                declaring = false;
                if let Some(Tok::Word(name, _)) = next {
                    if is_keyword(name) && !follows_a_pou_name(name) {
                        found.push(name.clone());
                    }
                }
            }
            section if section.starts_with("VAR") => declaring = true,
            _ => {}
        }
    }
    found
}

/// Where a name is being declared.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Declares {
    /// A POU: its name is followed by its sections or its statements.
    Pou,
    /// A variable, a member or a type: its name is followed by `:` or `,`.
    Item,
}

impl Parser {
    pub fn new(lexed: Lexed) -> Self {
        Parser {
            named_keywords: named_keywords(&lexed.toks),
            toks: lexed.toks,
            at: 0,
            cut: lexed.cut,
            diags: lexed.diags,
            failures: 0,
            told_at: None,
            nest: 0,
            height: 0,
        }
    }

    /// Go one level deeper to read what `read` reads, or refuse at the next token when
    /// that is deeper than a source may nest.
    fn deeper<T>(&mut self, read: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        if self.nest >= MAX_NESTING {
            return self.refuse(too_deep());
        }
        self.nest += 1;
        let read = read(self);
        self.nest -= 1;
        read
    }

    /// An expression whose tree is `height` high has been read at `span`: refused when the
    /// tree, from the statement it is in, is deeper than a source may nest.
    fn grew(&mut self, height: usize, span: Span) -> R<()> {
        // A value alone stands where `deeper` let it.
        if height > 1 && self.nest + height > MAX_NESTING {
            return self.fail(span, too_deep());
        }
        self.height = height;
        Ok(())
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.at].tok
    }
    /// The token after the next one.
    fn peek2(&self) -> Option<&Tok> {
        self.toks.get(self.at + 1).map(|t| &t.tok)
    }
    fn span(&self) -> Span {
        self.toks[self.at].span
    }
    /// From where `start` begins to the end of the last token taken.
    fn since(&self, start: Span) -> Span {
        let Some(last) = self.at.checked_sub(1).map(|i| self.toks[i].span) else {
            return start;
        };
        // Nothing taken since `start`: the last token ends where `start` begins, or before.
        if (last.end_line, last.end_col) <= (start.line, start.col) {
            return start;
        }
        start.to(last)
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
    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }
    fn one_of(&self, kws: &[&str]) -> bool {
        matches!(self.peek(), Tok::Word(u, _) if kws.contains(&u.as_str()))
    }
    /// At a keyword that begins a statement with an `END_` of its own.
    fn opens_block(&self) -> bool {
        matches!(self.peek(), Tok::Word(u, _) if BLOCKS.iter().any(|(open, _)| u == open))
    }
    /// Where a statement list ends: at the end of the source, at a keyword that ends a
    /// list, or where another POU begins.
    fn ends_stmts(&self) -> bool {
        self.at_eof() || self.one_of(&ENDS) || self.one_of(&STARTS)
    }
    /// At something no declaration begins with, so a list of declarations ends here.
    fn leaves_decls(&self) -> bool {
        self.at_eof()
            || self.one_of(&STARTS)
            || self.one_of(&ENDS)
            || self.opens_block()
            || self.one_of(&["END_VAR", "END_STRUCT", "END_TYPE", "EXIT", "RETURN"])
            || matches!(self.peek(), Tok::Word(u, _) if u == "VAR" || u.starts_with("VAR_"))
    }
    fn describe(&self) -> String {
        match self.peek() {
            Tok::Word(_, w) => format!("'{w}'"),
            Tok::Int(v) => format!("the number {v}"),
            Tok::Real(v) => format!("the number {v}"),
            Tok::Time(_) => "a duration".into(),
            Tok::Str(_) => "a string".into(),
            Tok::Sym(s) => format!("'{s}'"),
            Tok::Error => "text that cannot be read".into(),
            Tok::Eof => "the end of the source".into(),
        }
    }

    /// Say something and go on: the tree is still whole.
    fn say(&mut self, span: Span, message: impl Into<String>) {
        self.diags.push(Diagnostic::at(span, message));
    }
    /// Say what is wrong and give up.
    fn fail<T>(&mut self, span: Span, message: impl Into<String>) -> R<T> {
        self.failures += 1;
        self.say(span, message);
        Err(Failed)
    }
    /// Give up at the next token, saying `message` about it; unless something has been
    /// said about that token already (by the lexer, which could not read it, or by a rule
    /// that stopped here before), or the source was cut short before its end.
    fn refuse<T>(&mut self, message: String) -> R<T> {
        self.failures += 1;
        self.say_here(message);
        Err(Failed)
    }
    /// Say `message` about the next token, unless something has been said about it.
    fn say_here(&mut self, message: String) {
        let told = matches!(self.peek(), Tok::Error)
            || (self.cut && self.at_eof())
            || self.told_at == Some(self.at);
        if !told {
            self.say(self.span(), message);
        }
        self.told_at = Some(self.at);
    }
    /// Give up because the next token is not `wanted`.
    fn unexpected<T>(&mut self, wanted: impl std::fmt::Display) -> R<T> {
        self.refuse(format!("expected {wanted}, found {}", self.describe()))
    }
    fn not_yet<T>(&mut self, what: impl std::fmt::Display) -> R<T> {
        self.refuse(format!("{what} is not supported yet"))
    }
    /// The next token is a word that can stand as a name here: any word but a keyword,
    /// unless the source declares that keyword as a name (and was told so there).
    fn at_name(&self) -> bool {
        matches!(self.peek(), Tok::Word(u, _) if !is_keyword(u) || self.named_keywords.contains(u))
    }
    fn expect_kw(&mut self, kw: &str) -> R<()> {
        if self.is_kw(kw) {
            self.bump();
            Ok(())
        } else {
            self.unexpected(kw)
        }
    }
    fn expect_sym(&mut self, s: &str) -> R<()> {
        if self.is_sym(s) {
            self.bump();
            Ok(())
        } else {
            self.unexpected(format!("'{s}'"))
        }
    }
    /// A name where one is used.
    fn ident(&mut self) -> R<(String, Span)> {
        let span = self.span();
        match self.peek().clone() {
            Tok::Word(_, w) if self.at_name() => {
                self.bump();
                Ok((w, span))
            }
            _ => self.unexpected("a name"),
        }
    }
    /// The name of a type where one is named: a name, or the keyword of an elementary type.
    fn type_name(&mut self) -> R<(String, Span)> {
        let span = self.span();
        match self.peek().clone() {
            Tok::Word(u, w) if self.at_name() || TYPE_KEYWORDS.contains(&u.as_str()) => {
                self.bump();
                Ok((w, span))
            }
            _ => self.unexpected("a name"),
        }
    }

    /// A name where one is declared. A keyword of the standard written as a name is refused
    /// (IEC 61131-3 ed. 3 §6.1.3) and the declaration is read on under it, so the one
    /// mistake is told once. A keyword standing where a name was left out is not a name.
    fn declared(&mut self, what: Declares) -> R<(String, Span)> {
        let span = self.span();
        let Tok::Word(upper, w) = self.peek().clone() else {
            return self.unexpected("a name");
        };
        if is_keyword(&upper) {
            let meant_as_a_name = match what {
                Declares::Item => self.names_an_item(),
                Declares::Pou => !follows_a_pou_name(&upper),
            };
            if !meant_as_a_name {
                return self.unexpected("a name");
            }
            self.say_here(format!(
                "{w} is a keyword of IEC 61131-3 and cannot be used as a name"
            ));
            if !self.named_keywords.contains(&upper) {
                self.named_keywords.push(upper);
            }
        }
        self.bump();
        Ok((w, span))
    }

    /// At a word followed by `:` or `,`: it is written as the name of a variable, a member
    /// or a type, whatever the word is.
    fn names_an_item(&self) -> bool {
        matches!(self.peek(), Tok::Word(..)) && matches!(self.peek2(), Some(Tok::Sym(":" | ",")))
    }

    /// Step over tokens up to and including `end` (a keyword, or `;`), or to the end of
    /// the source.
    fn skip_past(&mut self, end: &str) {
        loop {
            if self.at_eof() {
                return;
            }
            let found = self.is_kw(end) || self.is_sym(end);
            self.bump();
            if found {
                return;
            }
        }
    }

    /// Keywords that a declaration outside this text named a variable with (and was told so
    /// there): where they stand as names here they are taken as those names, so that the one
    /// mistake is told once.
    pub fn with_named_keywords(mut self, upper: &[String]) -> Self {
        self.named_keywords.extend(upper.iter().cloned());
        self
    }

    /// One expression and nothing after it: the text of a graphical element (the variable of
    /// a contact, the expression of a variable, an initial value; ADR-013 §2.15). `None` when
    /// it could not be read; the diagnostics say why.
    pub fn expression_alone(mut self) -> (Option<Expr>, Vec<Diagnostic>) {
        if self.at_eof() {
            let _ = self.unexpected::<()>("an expression");
            return (None, self.diags);
        }
        let read = self.expr();
        let read = match read {
            Ok(e) if self.at_eof() => Some(e),
            Ok(_) => {
                let _ = self.unexpected::<()>("the end of the text");
                None
            }
            Err(_) => None,
        };
        (read, self.diags)
    }

    /// Statements and nothing after them: the Structured Text body of an action of a
    /// Sequential Function Chart (ADR-013 §2.17). The statements read, and whether all of the
    /// text was read (when not, the diagnostics say what was not).
    pub fn statements_alone(mut self) -> (Vec<Stmt>, bool, Vec<Diagnostic>) {
        let mut out = Vec::new();
        loop {
            out.append(&mut self.stmts());
            if self.at_eof() {
                break;
            }
            // A keyword that ends a statement which is not open (END_IF, ELSE, UNTIL, …), or
            // one that begins a declaration: neither belongs in a list of statements.
            let _ = self.unexpected::<()>("a statement");
            self.bump();
            if self.is_sym(";") {
                self.bump();
            }
        }
        let whole = self.failures == 0;
        (out, whole, self.diags)
    }

    /// A source: any number of TYPE blocks, functions and function block types, and the
    /// program, in any order; with everything that could not be read of it.
    pub fn source(mut self) -> (Source, Vec<Diagnostic>) {
        let mut src = Source::default();
        while !self.at_eof() {
            if self.is_kw("TYPE") {
                self.types(&mut src);
            } else if self.is_kw("PROGRAM") {
                self.pou(PouKind::Program, &mut src);
            } else if self.is_kw("FUNCTION_BLOCK") {
                self.pou(PouKind::FunctionBlock, &mut src);
            } else if self.is_kw("FUNCTION") {
                self.pou(PouKind::Function, &mut src);
            } else if let Some((kw, end)) = UNSUPPORTED.iter().find(|(kw, _)| self.is_kw(kw)) {
                let _ = self.not_yet::<()>(kw);
                // What it would have declared is not known to the checker.
                match *kw {
                    "NAMESPACE" => src.lost.extend([Lost::Function, Lost::Type]),
                    "CLASS" | "INTERFACE" => src.lost.push(Lost::Type),
                    _ => {}
                }
                self.bump();
                self.skip_past(end);
            } else {
                let _ = self.unexpected::<()>("PROGRAM, FUNCTION, FUNCTION_BLOCK or TYPE");
                self.bump();
                while !(self.at_eof()
                    || self.one_of(&STARTS)
                    || UNSUPPORTED.iter().any(|(kw, _)| self.is_kw(kw)))
                {
                    self.bump();
                }
            }
        }
        (src, self.diags)
    }

    /// `TYPE name : STRUCT … END_STRUCT; other : ARRAY […] OF …; END_TYPE`
    fn types(&mut self, src: &mut Source) {
        self.bump();
        loop {
            if !self.names_an_item() {
                if self.is_kw("END_TYPE") {
                    self.bump();
                    if self.is_sym(";") {
                        self.bump();
                    }
                    return;
                }
                if self.leaves_decls() {
                    let _ = self.unexpected::<()>("END_TYPE");
                    return;
                }
            }
            let from = self.at;
            let Ok((name, span)) = self.declared(Declares::Item) else {
                src.lost.push(Lost::Type);
                self.sync_decl(from);
                continue;
            };
            let before = self.failures;
            let read = self.type_kind();
            let partial = self.failures != before;
            src.types.push(TypeDecl {
                name,
                span,
                kind: read.as_ref().map_or(TypeDeclKind::Unknown, |k| k.clone()),
                partial,
            });
            if read.is_err() {
                self.sync_decl(from);
            }
        }
    }

    /// What a type's name stands for: from the `:` after the name to the `;`.
    fn type_kind(&mut self) -> R<TypeDeclKind> {
        self.expect_sym(":")?;
        let kind = if self.is_kw("STRUCT") {
            self.bump();
            let mut fields = Vec::new();
            self.decls("END_STRUCT", Section::Var, &mut fields)?;
            TypeDeclKind::Struct(fields)
        } else if self.is_kw("ARRAY") || self.is_kw("STRING") {
            TypeDeclKind::Alias(self.type_spec()?)
        } else if self.is_sym("(") {
            return self.not_yet("an enumeration");
        } else {
            return self.refuse(format!(
                "a type derived from {} is not supported yet; TYPE takes STRUCT, ARRAY and STRING",
                self.describe()
            ));
        };
        if self.is_sym(":=") {
            return self.not_yet("an initial value on a type");
        }
        self.expect_sym(";")?;
        Ok(kind)
    }

    /// The declarations of a section or a structure, up to `end`. One that cannot be read
    /// is left at its `;` and the next is read. Fails when the section is never closed.
    fn decls(&mut self, end: &str, section: Section, out: &mut Vec<VarDecl>) -> R<()> {
        loop {
            if !self.names_an_item() {
                if self.is_kw(end) {
                    self.bump();
                    return Ok(());
                }
                // A statement where a declaration should be: the section was not closed.
                let statement = matches!(self.peek(), Tok::Word(..))
                    && matches!(self.peek2(), Some(Tok::Sym(":=" | "(" | "." | "[")));
                if statement || self.leaves_decls() {
                    return self.unexpected(end);
                }
            }
            let from = self.at;
            if self.var_decls(section, out).is_err() {
                self.sync_decl(from);
            }
        }
    }

    /// Leave a declaration that could not be read: past its `;`, or before whatever ends
    /// the list it is in. `from` is where the declaration began.
    fn sync_decl(&mut self, from: usize) {
        if self.at == from {
            self.bump();
        }
        loop {
            if self.is_kw("STRUCT") {
                // A structure written out has declarations of its own: over it whole.
                loop {
                    let stop = self.at_eof()
                        || self.one_of(&STARTS)
                        || self.one_of(&POU_ENDS)
                        || self.is_kw("END_TYPE");
                    if stop {
                        break;
                    }
                    let found = self.is_kw("END_STRUCT");
                    self.bump();
                    if found {
                        break;
                    }
                }
                continue;
            }
            if self.is_sym(";") {
                self.bump();
                return;
            }
            if self.leaves_decls() {
                return;
            }
            self.bump();
        }
    }

    /// Step over a variable section that is not read: from its keyword to its END_VAR.
    fn skip_section(&mut self) {
        self.bump();
        loop {
            if self.is_kw("END_VAR") {
                self.bump();
                return;
            }
            let another =
                matches!(self.peek(), Tok::Word(u, _) if u == "VAR" || u.starts_with("VAR_"));
            if another || self.at_eof() || self.one_of(&STARTS) || self.one_of(&POU_ENDS) {
                return;
            }
            self.bump();
        }
    }

    /// A program, a function block type or a function, from its keyword to its END_.
    fn pou(&mut self, kind: PouKind, src: &mut Source) {
        let end = match kind {
            PouKind::Program => "END_PROGRAM",
            PouKind::FunctionBlock => "END_FUNCTION_BLOCK",
            PouKind::Function => "END_FUNCTION",
        };
        self.bump();
        let mut read = None;
        if self.pou_from_name(kind, end, &mut read).is_err() {
            // Its head could not be read, or it holds something not supported: leave it.
            if let Some(pou) = &mut read {
                pou.partial_vars = true;
                pou.partial_body = true;
            }
            loop {
                if self.at_eof() || self.one_of(&STARTS) {
                    break;
                }
                let found = self.one_of(&POU_ENDS);
                self.bump();
                if found {
                    break;
                }
            }
        }
        match read {
            Some(pou) => src.pous.push(pou),
            None => src.lost.push(match kind {
                PouKind::Program => Lost::Program,
                PouKind::FunctionBlock => Lost::Type,
                PouKind::Function => Lost::Function,
            }),
        }
    }

    /// A POU after its keyword. `read` holds it from the moment its name is known, so that a
    /// POU that cannot be read to its end is still known by name.
    fn pou_from_name(&mut self, kind: PouKind, end: &str, read: &mut Option<Pou>) -> R<()> {
        let before = self.failures;
        for kw in ["FINAL", "ABSTRACT"] {
            if self.is_kw(kw) {
                return self.not_yet(kw);
            }
        }
        let (name, span) = self.declared(Declares::Pou)?;
        let pou = read.insert(Pou {
            kind,
            name: name.clone(),
            span,
            result: None,
            vars: Vec::new(),
            body: Vec::new(),
            partial_vars: false,
            partial_body: false,
            library: false,
        });
        if kind == PouKind::Function {
            if self.is_sym(":") {
                self.bump();
                pou.result = Some(self.type_spec()?);
            } else {
                // The rest of the function can still be read. Nothing more is said about a
                // name that was refused as one.
                self.failures += 1;
                if self.told_at != Some(self.at - 1) {
                    self.say(
                        span,
                        format!(
                            "the function {name} has no result type; a function without a result is not supported yet"
                        ),
                    );
                }
            }
        }
        for kw in ["EXTENDS", "IMPLEMENTS"] {
            if self.is_kw(kw) {
                return self.not_yet(kw);
            }
        }
        while let Tok::Word(keyword, written) = self.peek().clone() {
            if keyword != "VAR" && !keyword.starts_with("VAR_") {
                break;
            }
            // A retentive qualifier where nothing can be retained is wrong whether or not the
            // section is supported, and is told as such.
            if let Some(Tok::Word(q, written)) = self.peek2().cloned() {
                if q == "RETAIN" || q == "NON_RETAIN" {
                    if let Some(why) = no_retention(kind, &keyword) {
                        self.bump();
                        let _ = self.refuse::<()>(format!("{written} is not allowed {why}"));
                        self.skip_section();
                        continue;
                    }
                }
            }
            let section = match keyword.as_str() {
                "VAR" => Section::Var,
                "VAR_INPUT" => Section::Input,
                "VAR_OUTPUT" if kind == PouKind::Function => {
                    let _ = self.not_yet::<()>("VAR_OUTPUT on a function");
                    self.skip_section();
                    continue;
                }
                "VAR_OUTPUT" => Section::Output,
                _ => {
                    let _ = self.not_yet::<()>(written);
                    self.skip_section();
                    continue;
                }
            };
            self.bump();
            let retention = if self.is_kw("RETAIN") {
                Retention::Retain
            } else if self.is_kw("NON_RETAIN") {
                Retention::NonRetain
            } else {
                Retention::Unqualified
            };
            let qualified = retention != Retention::Unqualified;
            if qualified {
                self.bump();
            }
            // IEC 61131-3 ed. 3 Annex A (Loc_Var_Decls, Global_Var_Decls): a section takes
            // one of CONSTANT, RETAIN and NON_RETAIN, not two. A constant has nothing to keep.
            let constant_then_retain = self.is_kw("CONSTANT")
                && matches!(self.peek2(), Some(Tok::Word(q, _)) if q == "RETAIN" || q == "NON_RETAIN");
            if constant_then_retain || (qualified && self.is_kw("CONSTANT")) {
                let _ = self.refuse::<()>(
                    "CONSTANT does not combine with RETAIN or NON_RETAIN: a section takes one of the three (IEC 61131-3 ed. 3 Annex A)".into(),
                );
                self.skip_section();
                continue;
            }
            let qualifier = [
                "CONSTANT",
                "RETAIN",
                "NON_RETAIN",
                "PERSISTENT",
                "PUBLIC",
                "PRIVATE",
                "PROTECTED",
                "INTERNAL",
            ]
            .into_iter()
            .find(|q| self.is_kw(q));
            if let Some(qualifier) = qualifier {
                let _ = self.not_yet::<()>(qualifier);
                self.skip_section();
                continue;
            }
            // A section never closed is told; what follows is read as the POU goes on.
            let from = pou.vars.len();
            let _ = self.decls("END_VAR", section, &mut pou.vars);
            for d in &mut pou.vars[from..] {
                d.retention = retention;
            }
        }
        pou.partial_vars = self.failures != before;
        for kw in ["METHOD", "ACTION", "PROPERTY"] {
            if self.is_kw(kw) {
                return self.not_yet(kw);
            }
        }
        let before = self.failures;
        loop {
            pou.body.append(&mut self.stmts());
            if self.is_kw(end) {
                self.bump();
                break;
            }
            let _ = self.unexpected::<()>(end);
            // The end of another kind of POU was meant as this one's. Where the source ends
            // or another POU begins, this one was never closed.
            if self.one_of(&POU_ENDS) {
                self.bump();
                break;
            }
            if self.at_eof() || self.one_of(&STARTS) {
                break;
            }
            // A keyword that ends a statement which is not open (END_IF, ELSE, UNTIL, …):
            // step over it and read the statements after it.
            self.bump();
            if self.is_sym(";") {
                self.bump();
            }
        }
        pou.partial_body = self.failures != before;
        Ok(())
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
            let elem = self.deeper(|p| p.type_spec())?;
            return Ok(TypeSpec {
                span: self.since(span),
                kind: TypeSpecKind::Array(dims, Box::new(elem)),
            });
        }
        if self.is_kw("STRUCT") {
            return self.refuse(
                "a STRUCT without a name is not supported yet: declare it in TYPE … END_TYPE and use its name".into(),
            );
        }
        if self.is_kw("STRING") {
            return self.string_spec();
        }
        if self.is_kw("WSTRING") || self.is_kw("POINTER") || self.is_kw("REF_TO") {
            return self.not_yet(self.describe());
        }
        let (name, _) = self.type_name()?;
        if self.is_sym("(") {
            return self.not_yet(format!("a subrange ({name} (lo..hi))"));
        }
        Ok(TypeSpec {
            span,
            kind: TypeSpecKind::Named(name),
        })
    }

    /// `STRING`, `STRING[n]` (Annex A, `S_Byte_Str_Spec`) or `STRING(n)` (§6.4.2.2 calls the
    /// length "parenthesized").
    fn string_spec(&mut self) -> R<TypeSpec> {
        let span = self.span();
        self.bump();
        let close = if self.is_sym("[") {
            "]"
        } else if self.is_sym("(") {
            ")"
        } else {
            return Ok(TypeSpec {
                span,
                kind: TypeSpecKind::String(None),
            });
        };
        self.bump();
        let at = self.span();
        let length = match self.peek().clone() {
            Tok::Int(v) => {
                self.bump();
                v
            }
            _ => {
                return self.refuse(format!(
                    "the length of a STRING is an integer literal, found {}",
                    self.describe()
                ))
            }
        };
        self.expect_sym(close)?;
        Ok(TypeSpec {
            span: self.since(span),
            kind: TypeSpecKind::String(Some((length, at))),
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
            _ => {
                let message = format!(
                    "an array bound must be an integer literal for now, found {}",
                    self.describe()
                );
                if negative {
                    // Told of the minus sign, where the bound begins.
                    self.fail(self.since(span), message)
                } else {
                    self.refuse(message)
                }
            }
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
            return Ok(Init::Array(items, self.since(span)));
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
                let inner = self.deeper(|p| p.array_items(")"))?;
                self.expect_sym(")")?;
                items.push(ArrayInit::Repeat(n, inner, self.since(span)));
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
        let mut names = vec![self.declared(Declares::Item)?];
        while self.is_sym(",") {
            self.bump();
            names.push(self.declared(Declares::Item)?);
        }
        if self.is_kw("AT") {
            return self.refuse(
                "AT %I/%Q addresses are not used: bind a variable to a tag in the variable table instead".into(),
            );
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
                retention: Retention::Unqualified,
                ty: ty.clone(),
                init: init.clone(),
                hidden: false,
            });
        }
        Ok(())
    }

    /// The steps after a name: `.member`, `[i, j]`, any number of them. Leaves in `height`
    /// the height of the highest subscript, 0 when there is none.
    fn selectors(&mut self) -> R<Vec<Selector>> {
        let mut path = Vec::new();
        let mut highest = 0;
        loop {
            let span = self.span();
            if self.is_sym(".") {
                self.bump();
                let (member, mspan) = self.ident()?;
                path.push(Selector::Member(member, mspan));
            } else if self.is_sym("[") {
                self.bump();
                let mut subscripts = vec![self.expr()?];
                highest = highest.max(self.height);
                while self.is_sym(",") {
                    self.bump();
                    subscripts.push(self.expr()?);
                    highest = highest.max(self.height);
                }
                self.expect_sym("]")?;
                path.push(Selector::Index(subscripts, self.since(span)));
            } else {
                self.height = highest;
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

    /// A list of statements, up to what ends it. A statement that cannot be read is left
    /// and the next is read.
    fn stmts(&mut self) -> Vec<Stmt> {
        self.stmt_list(false)
    }

    /// The statements of one CASE arm: up to the next label, ELSE or END_CASE.
    fn case_body(&mut self) -> Vec<Stmt> {
        self.stmt_list(true)
    }

    /// A list of statements is one level deeper than the statement it is in.
    fn stmt_list(&mut self, in_case: bool) -> Vec<Stmt> {
        self.nest += 1;
        let out = self.stmt_list_here(in_case);
        self.nest -= 1;
        out
    }

    fn stmt_list_here(&mut self, in_case: bool) -> Vec<Stmt> {
        let mut out = Vec::new();
        loop {
            if self.ends_stmts() {
                return out;
            }
            if self.is_sym(";") {
                self.bump();
                continue;
            }
            if in_case {
                // A label starts with a number, a minus sign or a name followed by ':' or
                // ',' or '..'.
                let label_like = matches!(self.peek(), Tok::Int(_) | Tok::Sym("-"))
                    || (matches!(self.peek(), Tok::Word(..))
                        && matches!(self.peek2(), Some(Tok::Sym(":" | "," | ".."))));
                if label_like {
                    return out;
                }
            }
            let from = self.at;
            match self.stmt() {
                Ok(Some(s)) => out.push(s),
                Ok(None) => {}
                Err(Failed) => self.sync_stmt(from),
            }
        }
    }

    /// Leave a statement that could not be read: past its `;`, or before the keyword that
    /// ends the list or begins the next statement. `from` is where the statement began.
    fn sync_stmt(&mut self, from: usize) {
        if self.at == from {
            self.bump();
        }
        loop {
            if self.is_sym(";") {
                self.bump();
                return;
            }
            if self.ends_stmts() || self.opens_block() {
                return;
            }
            self.bump();
        }
    }

    /// Leave a statement with an `END_` of its own that could not be followed: on to that
    /// `END_`, over the statements nested in it. Stops before the end of the POU.
    fn skip_block(&mut self) {
        let mut depth = 1;
        loop {
            if self.at_eof() || self.one_of(&STARTS) || self.one_of(&POU_ENDS) {
                return;
            }
            let closes = matches!(self.peek(), Tok::Word(u, _) if BLOCKS.iter().any(|(_, close)| u == close));
            if self.opens_block() {
                depth += 1;
            } else if closes {
                depth -= 1;
            }
            self.bump();
            if depth == 0 {
                if self.is_sym(";") {
                    self.bump();
                }
                return;
            }
        }
    }

    /// The head of a statement: what `parse` reads, then the keyword `kw` that ends the head.
    /// A head that cannot be read is given up (`None`), but `kw` is looked for before the
    /// statement's `;`, so that the statements under it are still read. Fails when it is
    /// not there.
    fn head<T>(&mut self, kw: &str, parse: impl FnOnce(&mut Self) -> R<T>) -> R<Option<T>> {
        let read = parse(self).and_then(|v| self.expect_kw(kw).map(|()| v));
        let failed = match read {
            Ok(v) => return Ok(Some(v)),
            Err(failed) => failed,
        };
        let mut k = self.at;
        loop {
            match &self.toks[k].tok {
                Tok::Word(u, _) if u == kw => {
                    self.at = k;
                    self.bump();
                    return Ok(None);
                }
                Tok::Eof | Tok::Sym(";") => return Err(failed),
                Tok::Word(u, _)
                    if ENDS.contains(&u.as_str())
                        || STARTS.contains(&u.as_str())
                        || BLOCKS.iter().any(|(open, _)| u == open) =>
                {
                    return Err(failed)
                }
                _ => k += 1,
            }
        }
    }

    /// The `END_` of a statement and the `;` that may follow it. When it is not there this
    /// says so and takes nothing: what is there belongs to an enclosing statement.
    fn closer(&mut self, kw: &str) -> bool {
        if self.expect_kw(kw).is_err() {
            return false;
        }
        if self.is_sym(";") {
            self.bump();
        }
        true
    }

    /// One statement. `Ok(None)`: it could not be read whole, has said why, and the parser
    /// stands after it. `Err`: it could not be read and the parser stands inside it.
    fn stmt(&mut self) -> R<Option<Stmt>> {
        if self.opens_block() {
            return Ok(self.block());
        }
        self.other_stmt()
    }

    /// A statement with an `END_` of its own. `None`: it could not be read whole, has said
    /// why, and the parser stands after it.
    fn block(&mut self) -> Option<Stmt> {
        if self.nest >= MAX_NESTING {
            // Said at its keyword; then left whole, with all that is nested in it.
            let _ = self.refuse::<()>(too_deep());
            self.bump();
            self.skip_block();
            return None;
        }
        // One call for every kind: a statement within statements keeps one result on the
        // stack, not one for each kind.
        let kind: fn(&mut Self) -> R<Option<Stmt>> = if self.is_kw("IF") {
            Self::if_stmt
        } else if self.is_kw("CASE") {
            Self::case_stmt
        } else if self.is_kw("FOR") {
            Self::for_stmt
        } else if self.is_kw("WHILE") {
            Self::while_stmt
        } else {
            Self::repeat_stmt
        };
        match kind(self) {
            Ok(read) => read,
            Err(Failed) => {
                self.skip_block();
                None
            }
        }
    }

    /// A statement without an `END_` of its own.
    fn other_stmt(&mut self) -> R<Option<Stmt>> {
        let span = self.span();
        if self.is_kw("EXIT") {
            self.bump();
            self.expect_sym(";")?;
            return Ok(Some(Stmt::Exit(span)));
        }
        if self.is_kw("RETURN") {
            self.bump();
            self.expect_sym(";")?;
            return Ok(Some(Stmt::Return(span)));
        }
        for kw in ["VAR", "VAR_TEMP"] {
            if self.is_kw(kw) {
                let _ = self.not_yet::<()>(format!("{kw} after the statements have begun"));
                self.skip_section();
                return Ok(None);
            }
        }
        for kw in ["CONTINUE", "METHOD", "ACTION"] {
            if self.is_kw(kw) {
                return self.not_yet(kw);
            }
        }
        self.simple_stmt().map(Some)
    }

    /// An assignment or a call of a function block instance.
    fn simple_stmt(&mut self) -> R<Stmt> {
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

    fn if_stmt(&mut self) -> R<Option<Stmt>> {
        self.bump();
        let mut whole = true;
        let mut branches = Vec::new();
        let mut otherwise = None;
        let cond = self.head("THEN", |p| p.expr())?;
        let body = self.stmts();
        match cond {
            Some(c) => branches.push((c, body)),
            None => whole = false,
        }
        loop {
            if self.is_kw("ELSIF") {
                self.bump();
                let cond = self.head("THEN", |p| p.expr())?;
                let body = self.stmts();
                match cond {
                    Some(c) => branches.push((c, body)),
                    None => whole = false,
                }
            } else if self.is_kw("ELSE") {
                self.bump();
                otherwise = Some(self.stmts());
            } else {
                break;
            }
        }
        if !self.closer("END_IF") {
            return Ok(None);
        }
        Ok(whole.then_some(Stmt::If(branches, otherwise)))
    }

    fn case_stmt(&mut self) -> R<Option<Stmt>> {
        self.bump();
        let sel = self.head("OF", |p| p.expr())?;
        let mut arms = Vec::new();
        let mut otherwise = None;
        loop {
            if self.is_kw("ELSE") {
                self.bump();
                otherwise = Some(self.case_body());
                break;
            }
            // END_CASE, or what shows it is missing.
            if self.ends_stmts() {
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
            arms.push((labels, self.case_body()));
        }
        if !self.closer("END_CASE") {
            return Ok(None);
        }
        Ok(sel.map(|sel| Stmt::Case(sel, arms, otherwise)))
    }

    fn for_stmt(&mut self) -> R<Option<Stmt>> {
        self.bump();
        let head = self.head("DO", |p| {
            let (name, vspan) = p.ident()?;
            if p.is_sym(".") || p.is_sym("[") {
                return p.not_yet("a FOR variable that is a member or an element (name it alone)");
            }
            p.expect_sym(":=")?;
            let from = p.expr()?;
            p.expect_kw("TO")?;
            let to = p.expr()?;
            let by = if p.is_kw("BY") {
                p.bump();
                Some(p.expr()?)
            } else {
                None
            };
            Ok((name, vspan, from, to, by))
        })?;
        let body = self.stmts();
        if !self.closer("END_FOR") {
            return Ok(None);
        }
        Ok(head.map(|(name, span, from, to, by)| Stmt::For {
            var: Place {
                name,
                span,
                path: Vec::new(),
            },
            from,
            to,
            by,
            body,
        }))
    }

    fn while_stmt(&mut self) -> R<Option<Stmt>> {
        self.bump();
        let cond = self.head("DO", |p| p.expr())?;
        let body = self.stmts();
        if !self.closer("END_WHILE") {
            return Ok(None);
        }
        Ok(cond.map(|c| Stmt::While(c, body)))
    }

    fn repeat_stmt(&mut self) -> R<Option<Stmt>> {
        self.bump();
        let body = self.stmts();
        if self.expect_kw("UNTIL").is_err() {
            return Ok(None);
        }
        let cond = self.head("END_REPEAT", |p| p.expr())?;
        if self.is_sym(";") {
            self.bump();
        }
        Ok(cond.map(|c| Stmt::Repeat(body, c)))
    }

    /// An expression. Its operations are taken by precedence (IEC 61131-3 ed. 3 Table 71),
    /// left to right within one, in a loop rather than a rule per precedence, so that a
    /// level of parentheses costs one call of this and not one for each precedence. Leaves
    /// the height of its tree in `height`.
    pub fn expr(&mut self) -> R<Expr> {
        // The operands read and the operators between them still waiting for their right
        // operand; each operator waiting has a lower precedence than the one after it.
        let mut operands = vec![self.operand()?];
        let mut waiting: Vec<(usize, BinOp, Span)> = Vec::new();
        loop {
            // A message about an operation is about its operator.
            let span = self.span();
            let Some((level, op)) = self.operator() else {
                break;
            };
            while waiting.last().is_some_and(|(before, ..)| *before >= level) {
                self.apply(&mut operands, &mut waiting)?;
            }
            self.bump();
            waiting.push((level, op, span));
            operands.push(self.operand()?);
        }
        while !waiting.is_empty() {
            self.apply(&mut operands, &mut waiting)?;
        }
        let Some((e, height)) = operands.pop() else {
            return self.unexpected("a value");
        };
        self.height = height;
        Ok(e)
    }

    /// An operand of an operation, with the height of its tree.
    fn operand(&mut self) -> R<(Expr, usize)> {
        let e = self.unary()?;
        Ok((e, self.height))
    }

    /// The operator at the next token: its precedence (0 binds least) and what it does.
    fn operator(&self) -> Option<(usize, BinOp)> {
        const LEVELS: [&[(&str, BinOp)]; 7] = [
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
            // Table 71 precedence 7: above * and below a unary minus, so -a**2 is (-a)**2.
            &[("**", BinOp::Pow)],
        ];
        let token = match self.peek() {
            Tok::Sym(s) => *s,
            Tok::Word(u, _) => u.as_str(),
            _ => return None,
        };
        LEVELS.iter().enumerate().find_map(|(level, ops)| {
            ops.iter()
                .find(|(t, _)| *t == token)
                .map(|(_, op)| (level, *op))
        })
    }

    /// Join the last two operands by the last operator waiting.
    fn apply(
        &mut self,
        operands: &mut Vec<(Expr, usize)>,
        waiting: &mut Vec<(usize, BinOp, Span)>,
    ) -> R<()> {
        let (Some((_, op, span)), Some((right, rh)), Some((left, lh))) =
            (waiting.pop(), operands.pop(), operands.pop())
        else {
            return Ok(());
        };
        let height = 1 + lh.max(rh);
        self.grew(height, span)?;
        operands.push((
            Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                span,
            },
            height,
        ));
        Ok(())
    }

    /// A unary operation or what it applies to: one level deeper than where it stands.
    fn unary(&mut self) -> R<Expr> {
        self.deeper(|p| p.unary_here())
    }

    fn unary_here(&mut self) -> R<Expr> {
        if self.is_sym("-") {
            return self.minus();
        }
        if self.is_sym("+") {
            self.bump();
            return self.unary();
        }
        if self.is_kw("NOT") {
            return self.not();
        }
        self.primary()
    }

    fn minus(&mut self) -> R<Expr> {
        let span = self.span();
        self.bump();
        let inner = self.unary()?;
        // Fold a negative literal, so INT's -32768 is a value, not -(32768).
        let kind = match inner.kind {
            ExprKind::Int(v) => ExprKind::Int(-v),
            ExprKind::Real(v) => ExprKind::Real(-v),
            ExprKind::Time(v) => ExprKind::Time(-v),
            k => {
                self.grew(self.height + 1, self.since(span))?;
                ExprKind::Neg(Box::new(Expr {
                    kind: k,
                    span: inner.span,
                }))
            }
        };
        Ok(Expr {
            kind,
            span: self.since(span),
        })
    }

    fn not(&mut self) -> R<Expr> {
        let span = self.span();
        self.bump();
        let inner = self.unary()?;
        self.grew(self.height + 1, self.since(span))?;
        Ok(Expr {
            kind: ExprKind::Not(Box::new(inner)),
            span: self.since(span),
        })
    }

    fn primary(&mut self) -> R<Expr> {
        let span = self.span();
        let kind = match self.peek() {
            Tok::Int(v) => ExprKind::Int(*v),
            Tok::Real(v) => ExprKind::Real(*v),
            Tok::Time(v) => ExprKind::Time(*v),
            Tok::Str(text) => ExprKind::Str(text.clone()),
            Tok::Word(u, _) if u == "TRUE" || u == "FALSE" => ExprKind::Bool(u == "TRUE"),
            Tok::Sym("(") => {
                self.bump();
                let e = self.expr()?;
                self.expect_sym(")")?;
                return Ok(e);
            }
            Tok::Word(..) if self.at_name() => return self.named(),
            _ => return self.unexpected("a value"),
        };
        self.bump();
        self.height = 1;
        Ok(Expr { kind, span })
    }

    /// A variable, a part of one, or a call of a function.
    fn named(&mut self) -> R<Expr> {
        let span = self.span();
        let (name, nspan) = self.ident()?;
        let kind = if self.is_sym("(") {
            let args = self.call_args()?;
            ExprKind::Call(name, args)
        } else {
            let path = self.selectors()?;
            ExprKind::Var(Place {
                name,
                span: nspan,
                path,
            })
        };
        self.grew(self.height + 1, self.since(span))?;
        Ok(Expr {
            kind,
            span: self.since(span),
        })
    }

    /// The arguments of a function called in an expression, from its `(` to its `)`. Leaves
    /// in `height` the height of the highest argument, 0 when there is none.
    fn call_args(&mut self) -> R<Vec<CallArg>> {
        self.bump();
        let mut args = Vec::new();
        let mut highest = 0;
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
            highest = highest.max(self.height);
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
        self.height = highest;
        Ok(args)
    }
}
