//! Structured Text tokens. Keywords and identifiers are case-insensitive (IEC 61131-3 §6.1.2);
//! a word keeps its spelling for messages and its upper-case form for matching.
//!
//! The lexer does not stop at what it cannot read: it says so, puts a [`Tok::Error`] where
//! the unreadable text was, and goes on, so one pass finds every such place.

use mythos_logic_ir::Diagnostic;
pub use mythos_logic_ir::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// An identifier or keyword: (upper case, as written).
    Word(String, String),
    Int(i128),
    Real(f64),
    /// A duration in nanoseconds.
    Time(i64),
    /// Punctuation and operators: `:=`, `=>`, `<>`, `<=`, `>=`, `**`, `..`, and single characters.
    Sym(&'static str),
    /// Text the lexer could not read and has already given a diagnostic for. The parser
    /// accepts it nowhere, and says nothing more about it.
    Error,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    /// From the token's first character to the position after its last.
    pub span: Span,
}

/// What the lexer made of a source.
pub struct Lexed {
    /// The tokens, ending with [`Tok::Eof`].
    pub toks: Vec<Token>,
    /// What could not be read, in the order found.
    pub diags: Vec<Diagnostic>,
    /// The source ended inside something never closed (a comment), so whatever the end of
    /// the source seems to lack may be in the part that was not read.
    pub cut: bool,
}

const SYMS2: [&str; 7] = [":=", "=>", "<>", "<=", ">=", "**", ".."];
const SYMS1: [&str; 15] = [
    "=", "<", ">", "+", "-", "*", "/", "(", ")", ";", ":", ",", ".", "[", "]",
];

/// The tokens of `src`, or the first thing in it that cannot be read.
pub fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut lexed = tokens(src);
    if lexed.diags.is_empty() {
        Ok(lexed.toks)
    } else {
        Err(lexed.diags.remove(0))
    }
}

/// Where the lexer is in the source.
struct Cursor<'a> {
    chars: &'a [char],
    i: usize,
    line: u32,
    col: u32,
}

impl Cursor<'_> {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.i + ahead).copied()
    }
    fn step(&mut self) {
        if self.chars[self.i] == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        self.i += 1;
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }
    /// Step while `keep` holds; the characters stepped over.
    fn run(&mut self, keep: impl Fn(char) -> bool) -> String {
        let start = self.i;
        while self.peek(0).is_some_and(&keep) {
            self.step();
        }
        self.chars[start..self.i].iter().collect()
    }
    /// The empty stretch at the cursor.
    fn here(&self) -> Span {
        Span {
            line: self.line,
            col: self.col,
            end_line: self.line,
            end_col: self.col,
        }
    }
    /// From where `start` begins to the cursor.
    fn since(&self, start: Span) -> Span {
        start.to(self.here())
    }
}

/// A character that can begin a token, a comment or white space; any other is not part of
/// the language.
fn known(c: char) -> bool {
    c.is_whitespace()
        || c.is_ascii_alphanumeric()
        || matches!(c, '_' | '%' | '\'' | '"' | '&')
        || SYMS1.iter().any(|s| s.starts_with(c))
}

/// Every token of `src`, and every place that could not be read.
pub fn tokens(src: &str) -> Lexed {
    let chars: Vec<char> = src.chars().collect();
    let mut c = Cursor {
        chars: &chars,
        i: 0,
        line: 1,
        col: 1,
    };
    let mut toks = Vec::new();
    let mut diags = Vec::new();
    let mut cut = false;
    while let Some(ch) = c.peek(0) {
        let start = c.here();
        let next = c.peek(1);
        if ch.is_whitespace() {
            c.step();
            continue;
        }
        // Comments: (* *), /* */, //
        if (ch == '(' && next == Some('*')) || (ch == '/' && next == Some('*')) {
            let close = if ch == '(' { ')' } else { '/' };
            c.steps(2);
            let opener = c.since(start);
            loop {
                if c.peek(1).is_none() {
                    // The rest of the source is inside the comment: nothing more to read.
                    diags.push(Diagnostic::at(opener, "a comment that is never closed"));
                    c.steps(c.chars.len() - c.i);
                    cut = true;
                    break;
                }
                if c.peek(0) == Some('*') && c.peek(1) == Some(close) {
                    c.steps(2);
                    break;
                }
                c.step();
            }
            continue;
        }
        if ch == '/' && next == Some('/') {
            c.run(|x| x != '\n');
            continue;
        }
        // What a failed literal leaves behind: the diagnostic, and a token no rule accepts.
        macro_rules! unreadable {
            ($msg:expr) => {{
                let span = c.since(start);
                diags.push(Diagnostic::at(span, $msg));
                toks.push(Token {
                    tok: Tok::Error,
                    span,
                });
                continue;
            }};
        }
        macro_rules! token {
            ($tok:expr) => {{
                toks.push(Token {
                    tok: $tok,
                    span: c.since(start),
                });
                continue;
            }};
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let word = c.run(|x| x.is_ascii_alphanumeric() || x == '_');
            let upper = word.to_ascii_uppercase();
            if c.peek(0) == Some('#') {
                c.step();
                if upper == "T" || upper == "TIME" {
                    match duration(&chars[c.i..]) {
                        Ok((ns, used)) => {
                            c.steps(used);
                            token!(Tok::Time(ns));
                        }
                        Err(m) => {
                            typed_literal_body(&mut c, false);
                            unreadable!(m);
                        }
                    }
                }
                let dated = matches!(
                    upper.as_str(),
                    "D" | "LD"
                        | "DATE"
                        | "LDATE"
                        | "DT"
                        | "LDT"
                        | "DATE_AND_TIME"
                        | "LDATE_AND_TIME"
                        | "TOD"
                        | "LTOD"
                        | "TIME_OF_DAY"
                        | "LTIME_OF_DAY"
                );
                typed_literal_body(&mut c, dated);
                unreadable!(format!(
                    "the typed literal {word}#… is not supported yet; only T# and TIME#"
                ));
            }
            token!(Tok::Word(upper, word));
        }
        if ch.is_ascii_digit() {
            let digits: String = c
                .run(|x| x.is_ascii_digit() || x == '_')
                .chars()
                .filter(|x| *x != '_')
                .collect();
            // Based integer: 16#FF, 8#17, 2#1010
            if c.peek(0) == Some('#') {
                let base: u32 = digits.parse().unwrap_or(0);
                c.step();
                let body: String = c
                    .run(|x| x.is_ascii_alphanumeric() || x == '_')
                    .chars()
                    .filter(|x| *x != '_')
                    .collect();
                if ![2, 8, 16].contains(&base) {
                    unreadable!(format!("{base}# is not a base; use 2#, 8# or 16#"));
                }
                match i128::from_str_radix(&body, base) {
                    Ok(v) => token!(Tok::Int(v)),
                    Err(_) => unreadable!(format!("{body} is not a base-{base} number")),
                }
            }
            // Real: digits '.' digits [E [+-] digits]; a '..' after an integer is a range.
            let is_real = c.peek(0) == Some('.') && c.peek(1).is_some_and(|x| x.is_ascii_digit());
            let has_exp = |c: &Cursor| matches!(c.peek(0), Some('e' | 'E'));
            if is_real || has_exp(&c) {
                let mut text = digits;
                if is_real {
                    c.step();
                    text.push('.');
                    text.extend(
                        c.run(|x| x.is_ascii_digit() || x == '_')
                            .chars()
                            .filter(|x| *x != '_'),
                    );
                }
                if has_exp(&c) {
                    text.push('e');
                    c.step();
                    if let Some(sign @ ('+' | '-')) = c.peek(0) {
                        text.push(sign);
                        c.step();
                    }
                    text.push_str(&c.run(|x| x.is_ascii_digit()));
                }
                match text.parse::<f64>() {
                    Ok(v) => token!(Tok::Real(v)),
                    Err(_) => unreadable!(format!("{text} is not a number")),
                }
            }
            match digits.parse::<i128>() {
                Ok(v) => token!(Tok::Int(v)),
                Err(_) => unreadable!(format!("{digits} is too large")),
            }
        }
        // A direct address (%IX0.0): read whole, so the parser can say what to do instead.
        if ch == '%' {
            c.step();
            let rest = c.run(|x| x.is_ascii_alphanumeric() || x == '.' || x == '*');
            let text = format!("%{rest}");
            token!(Tok::Word(text.to_ascii_uppercase(), text));
        }
        if ch == '\'' || ch == '"' {
            string_body(&mut c);
            unreadable!("strings are not supported yet");
        }
        if let Some(n) = next {
            let two: String = [ch, n].iter().collect();
            if let Some(s) = SYMS2.iter().find(|s| **s == two) {
                c.steps(2);
                token!(Tok::Sym(s));
            }
        }
        let one = ch.to_string();
        if let Some(s) = SYMS1.iter().find(|s| **s == one) {
            c.step();
            token!(Tok::Sym(s));
        }
        if ch == '&' {
            c.step();
            token!(Tok::Word("AND".into(), "&".into()));
        }
        // Characters of no token, however many stand together: said once.
        c.run(|x| !known(x));
        unreadable!(format!("'{ch}' is not part of Structured Text"));
    }
    toks.push(Token {
        tok: Tok::Eof,
        span: c.here(),
    });
    Lexed { toks, diags, cut }
}

/// Step over what follows the `#` of a typed literal the lexer does not read, so that it is
/// one unreadable thing and not several: `16#FF`, `1.5`, `'text'`, and for the date and time
/// of day types `1970-01-01-00:00:00`. A `..` is a range and ends it.
fn typed_literal_body(c: &mut Cursor, dated: bool) {
    if let Some('\'' | '"') = c.peek(0) {
        string_body(c);
        return;
    }
    if let Some('+' | '-') = c.peek(0) {
        c.step();
    }
    while let Some(x) = c.peek(0) {
        let digit_next = c.peek(1).is_some_and(|n| n.is_ascii_digit());
        let part = x.is_ascii_alphanumeric()
            || x == '_'
            || x == '#'
            || (x == '.' && digit_next)
            || (dated && (x == '-' || x == ':') && digit_next);
        if !part {
            break;
        }
        c.step();
    }
}

/// Step over a character string from its opening quote to its closing one, or to the end of
/// the line if it is never closed. `$` takes the next character with it (`$'` is a quote
/// inside the string: IEC 61131-3 ed. 3 Table 6).
fn string_body(c: &mut Cursor) {
    let Some(quote) = c.peek(0) else { return };
    c.step();
    while let Some(x) = c.peek(0) {
        if x == '\n' {
            return;
        }
        c.step();
        if x == quote {
            return;
        }
        if x == '$' && c.peek(0).is_some_and(|n| n != '\n') {
            c.step();
        }
    }
}

/// A duration after `T#`: `[-]` then parts like `1d2h3m4s5ms6us7ns`, underscores allowed, the
/// last part may have a fraction (`1.5s`). Returns nanoseconds and the characters used.
fn duration(c: &[char]) -> Result<(i64, usize), String> {
    let mut i = 0;
    let negative = c.first() == Some(&'-');
    if negative {
        i += 1;
    }
    let mut total: f64 = 0.0;
    let mut parts = 0;
    loop {
        while i < c.len() && c[i] == '_' {
            i += 1;
        }
        let s = i;
        while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
            i += 1;
        }
        if s == i {
            break;
        }
        let number: f64 = c[s..i]
            .iter()
            .collect::<String>()
            .parse()
            .map_err(|_| "a malformed duration".to_string())?;
        let u = i;
        while i < c.len() && c[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit: String = c[u..i].iter().collect::<String>().to_ascii_lowercase();
        let scale = match unit.as_str() {
            "d" => 86_400e9,
            "h" => 3_600e9,
            "m" => 60e9,
            "s" => 1e9,
            "ms" => 1e6,
            "us" => 1e3,
            "ns" => 1.0,
            _ => {
                return Err(format!(
                    "'{unit}' is not a unit of time (d, h, m, s, ms, us, ns)"
                ))
            }
        };
        total += number * scale;
        parts += 1;
    }
    if parts == 0 {
        return Err("T# needs a duration, like T#500ms".into());
    }
    if total > i64::MAX as f64 {
        return Err("the duration is too long".into());
    }
    let ns = total.round() as i64;
    Ok((if negative { -ns } else { ns }, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_nanoseconds() {
        let t = |s: &str| match lex(s).unwrap()[0].tok {
            Tok::Time(ns) => ns,
            ref other => panic!("{other:?}"),
        };
        assert_eq!(t("T#500ms"), 500_000_000);
        assert_eq!(t("TIME#1h_30m"), 5_400_000_000_000);
        assert_eq!(t("t#1.5s"), 1_500_000_000);
        assert_eq!(t("T#-2s"), -2_000_000_000);
    }

    #[test]
    fn a_range_after_an_integer_is_not_a_real() {
        let toks: Vec<Tok> = lex("1..5").unwrap().into_iter().map(|t| t.tok).collect();
        assert_eq!(
            toks,
            vec![Tok::Int(1), Tok::Sym(".."), Tok::Int(5), Tok::Eof]
        );
    }

    #[test]
    fn comments_of_every_kind_are_skipped() {
        let toks = lex("(* a *) x /* b */ // c\n y").unwrap();
        assert_eq!(toks.len(), 3);
        assert_eq!(
            toks[1].span,
            Span {
                line: 2,
                col: 2,
                end_line: 2,
                end_col: 3
            }
        );
    }

    #[test]
    fn a_token_ends_at_the_position_after_its_last_character() {
        let lexed = tokens(
            "ab := 16#FF;
  T#1s",
        );
        let spans: Vec<(u32, u32, u32, u32)> = lexed
            .toks
            .iter()
            .map(|t| (t.span.line, t.span.col, t.span.end_line, t.span.end_col))
            .collect();
        assert_eq!(
            spans,
            vec![
                (1, 1, 1, 3),   // ab
                (1, 4, 1, 6),   // :=
                (1, 7, 1, 12),  // 16#FF
                (1, 12, 1, 13), // ;
                (2, 3, 2, 7),   // T#1s
                (2, 7, 2, 7),   // the end of the source: nothing to underline
            ]
        );
    }

    #[test]
    fn what_cannot_be_read_is_said_once_and_the_rest_is_still_read() {
        let lexed = tokens(
            "a := 'it$'s' + INT#16#FF + §§ + 9#1;
b := DT#1970-01-01-00:00:00;",
        );
        let said: Vec<String> = lexed.diags.iter().map(|d| d.to_string()).collect();
        assert_eq!(
            said,
            vec![
                "1:6: strings are not supported yet",
                "1:16: the typed literal INT#… is not supported yet; only T# and TIME#",
                "1:28: '§' is not part of Structured Text",
                "1:33: 9# is not a base; use 2#, 8# or 16#",
                "2:6: the typed literal DT#… is not supported yet; only T# and TIME#",
            ]
        );
        // Each is one token, as long as what could not be read.
        let ends: Vec<(u32, u32)> = lexed
            .diags
            .iter()
            .map(|d| (d.end_line, d.end_col))
            .collect();
        assert_eq!(ends, vec![(1, 13), (1, 25), (1, 30), (1, 36), (2, 28)]);
        let errors = lexed.toks.iter().filter(|t| t.tok == Tok::Error).count();
        assert_eq!(errors, 5);
        assert!(!lexed.cut);
        assert!(matches!(lexed.toks.last().unwrap().tok, Tok::Eof));
    }

    #[test]
    fn a_comment_never_closed_takes_the_rest_of_the_source_and_says_so_once() {
        let lexed = tokens(
            "x := 1;
(* and so on
 y := 'a';",
        );
        let said: Vec<String> = lexed.diags.iter().map(|d| d.to_string()).collect();
        assert_eq!(said, vec!["2:1: a comment that is never closed"]);
        assert_eq!((lexed.diags[0].end_line, lexed.diags[0].end_col), (2, 3));
        assert!(lexed.cut);
        assert_eq!(lexed.toks.len(), 5, "x := 1 ; and the end");
    }
}
