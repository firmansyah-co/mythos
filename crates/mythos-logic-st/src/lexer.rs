//! Structured Text tokens. Keywords and identifiers are case-insensitive (IEC 61131-3 §6.1.2);
//! a word keeps its spelling for messages and its upper-case form for matching.
//!
//! The lexer does not stop at what it cannot read: it says so, puts a [`Tok::Error`] where
//! the unreadable text was, and goes on, so one pass finds every such place.

pub use mythos_logic_ir::Span;
use mythos_logic_ir::{Diagnostic, MAX_STRING_LENGTH};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// An identifier or keyword: (upper case, as written).
    Word(String, String),
    Int(i128),
    Real(f64),
    /// A duration in nanoseconds.
    Time(i64),
    /// A single-byte character string (`'…'` or `STRING#'…'`): its characters, `$`
    /// combinations read (IEC 61131-3 ed. 3 Table 6, Table 7).
    Str(Vec<u8>),
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
    lex_as(src, 1, false)
}

/// The tokens of a text written inside another language's source, its lines counted from
/// `first_line`: the variable of a Ladder Diagram contact, the expression of a variable, an
/// initial value (ADR-013 §2.15). The rules are a program's: a name may not begin with two
/// underscores. The line says which text it is, so a diagnostic about it finds its element.
pub fn tokens_at(src: &str, first_line: u32) -> Lexed {
    lex_as(src, first_line, false)
}

/// The tokens of a file of the standard library, its lines counted from `first_line`, so that
/// a place in it can never be taken for a place in a program's source (ADR-013 L4). Only
/// here may a name begin with two underscores: those are the intrinsics, which a program
/// cannot name.
pub(crate) fn library_tokens(src: &str, first_line: u32) -> Lexed {
    lex_as(src, first_line, true)
}

/// Whether `word` cannot be a name: IEC 61131-3 ed. 3 §6.1.2 allows neither two
/// underscores together nor one at the end (`__LIM_SW5`, `LIM__SW5`, `LIM_SW5_`). With
/// `intrinsic`, a name may begin with two underscores and a letter, which is how the
/// library names an intrinsic; the rest of it follows the rule.
fn misnamed(word: &str, intrinsic: bool) -> bool {
    let rest = word
        .strip_prefix("__")
        .filter(|rest| intrinsic && rest.starts_with(|x: char| x.is_ascii_alphabetic()))
        .unwrap_or(word);
    rest.contains("__") || rest.ends_with('_')
}

fn lex_as(src: &str, first_line: u32, library: bool) -> Lexed {
    let chars: Vec<char> = src.chars().collect();
    let mut c = Cursor {
        chars: &chars,
        i: 0,
        line: first_line,
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
                // Table 6 feature 3a: a typed string, STRING#'OK'.
                if upper == "STRING" && c.peek(0) == Some('\'') {
                    match string_literal(&mut c) {
                        Ok(text) => token!(Tok::Str(text)),
                        Err(m) => unreadable!(m),
                    }
                }
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
            if misnamed(&word, library) {
                unreadable!(format!(
                    "{word} is not a name: a name has no two underscores together and none at its end"
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
        if ch == '\'' {
            match string_literal(&mut c) {
                Ok(text) => token!(Tok::Str(text)),
                Err(m) => unreadable!(m),
            }
        }
        if ch == '"' {
            string_body(&mut c);
            unreadable!("a double-byte string (\"…\", WSTRING) is not supported yet");
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

/// A single-byte character string from its opening quote to its closing one (IEC 61131-3
/// ed. 3 §6.3.3, Table 6, Table 7, Annex A `S_Byte_Char_Str`): its characters, each a byte.
/// Written as itself, a character is a printable one of ISO/IEC 646 (`' '` to `'~'`) other
/// than `$` and `'`; any other is written `$` and two hexadecimal digits, its eight-bit code.
/// `$$`, `$'`, `$L`, `$N`, `$P`, `$R`, `$T` (either case) are the combinations of Table 7;
/// `$N`, the newline, is a line feed (`$0A`), which the standard leaves to the implementer.
/// The whole literal is stepped over even when it is wrong, so it is one thing told once.
fn string_literal(c: &mut Cursor) -> Result<Vec<u8>, String> {
    let mut text = Vec::new();
    let mut wrong: Option<String> = None;
    let mut fail = |m: String| {
        wrong.get_or_insert(m);
    };
    c.step();
    loop {
        let Some(x) = c.peek(0) else {
            fail("a string that is never closed".into());
            break;
        };
        if x == '\n' {
            fail("a string that is not closed on its line".into());
            break;
        }
        c.step();
        match x {
            '\'' => break,
            '$' => {
                let Some(n) = c.peek(0).filter(|n| *n != '\n') else {
                    fail("a $ with nothing after it".into());
                    continue;
                };
                c.step();
                let byte = match n.to_ascii_uppercase() {
                    '$' => b'$',
                    '\'' => b'\'',
                    'L' | 'N' => b'\n',
                    'P' => 0x0C,
                    'R' => b'\r',
                    'T' => b'\t',
                    '"' => {
                        fail(
                            "$\" belongs in a double-byte string; here a \" stands for itself"
                                .into(),
                        );
                        continue;
                    }
                    h if h.is_ascii_hexdigit() => {
                        match c.peek(0).filter(|l| l.is_ascii_hexdigit()) {
                            Some(l) => {
                                c.step();
                                // Two hexadecimal digits: always a byte.
                                u8::from_str_radix(&format!("{h}{l}"), 16).unwrap_or(0)
                            }
                            None => {
                                fail(format!(
                                    "${n} is not a character: write $ and two hexadecimal digits"
                                ));
                                continue;
                            }
                        }
                    }
                    _ => {
                        fail(format!(
                            "${n} is not a character: $ is followed by $, ', L, N, P, R, T or two hexadecimal digits"
                        ));
                        continue;
                    }
                };
                text.push(byte);
            }
            x if (' '..='~').contains(&x) => text.push(x as u8),
            x => {
                let code = x as u32;
                fail(if code <= 0xFF {
                    format!("the character {x:?} is written ${code:02X} in a string")
                } else {
                    format!(
                        "{x:?} is not a single-byte character; a STRING holds codes up to $FF (WSTRING is not supported yet)"
                    )
                });
            }
        }
    }
    if text.len() > MAX_STRING_LENGTH as usize {
        fail(format!(
            "a string of {} characters is longer than the {MAX_STRING_LENGTH} a STRING may hold",
            text.len()
        ));
    }
    match wrong {
        Some(m) => Err(m),
        None => Ok(text),
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
            "a := \"it$\"s\" + INT#16#FF + §§ + 9#1;
b := DT#1970-01-01-00:00:00;",
        );
        let said: Vec<String> = lexed.diags.iter().map(|d| d.to_string()).collect();
        assert_eq!(
            said,
            vec![
                "1:6: a double-byte string (\"…\", WSTRING) is not supported yet",
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
    fn a_name_with_two_underscores_together_or_one_at_its_end_is_refused() {
        // IEC 61131-3 ed. 3 §6.1.2, with its own examples.
        let lexed = tokens("__LIM_SW5 LIM__SW5 LIM_SW5_ _ LIM_SW5 _LIM_SW5 __NOW");
        let said: Vec<String> = lexed.diags.iter().map(|d| d.to_string()).collect();
        let why = "is not a name: a name has no two underscores together and none at its end";
        assert_eq!(
            said,
            vec![
                format!("1:1: __LIM_SW5 {why}"),
                format!("1:11: LIM__SW5 {why}"),
                format!("1:20: LIM_SW5_ {why}"),
                format!("1:29: _ {why}"),
                format!("1:48: __NOW {why}"),
            ]
        );
        let words: Vec<&str> = lexed
            .toks
            .iter()
            .filter_map(|t| match &t.tok {
                Tok::Word(_, w) => Some(w.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            words,
            ["LIM_SW5", "_LIM_SW5"],
            "one leading underscore is a name"
        );

        // The library names an intrinsic so; nothing else breaks the rule there either, and
        // its places are counted from the line it is given.
        let lexed = library_tokens("__NOW LIM__SW5 __LIM_ ___X", 1 << 20);
        let said: Vec<String> = lexed.diags.iter().map(|d| d.to_string()).collect();
        assert_eq!(
            said,
            vec![
                format!("1048576:7: LIM__SW5 {why}"),
                format!("1048576:16: __LIM_ {why}"),
                format!("1048576:23: ___X {why}"),
            ]
        );
        assert_eq!(lexed.toks[0].tok, Tok::Word("__NOW".into(), "__NOW".into()));
    }

    #[test]
    fn a_string_is_its_characters_with_the_dollar_combinations_read() {
        let lexed = tokens("'' 'A' '$'' '\"' '$$1$L$l$N$P$R$T$0a$FF' STRING#'OK' 'two words'");
        assert!(lexed.diags.is_empty(), "{:?}", lexed.diags);
        let texts: Vec<Vec<u8>> = lexed
            .toks
            .iter()
            .filter_map(|t| match &t.tok {
                Tok::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec![
                b"".to_vec(),
                b"A".to_vec(),
                b"'".to_vec(),
                b"\"".to_vec(),
                vec![b'$', b'1', 10, 10, 10, 12, 13, 9, 10, 0xFF],
                b"OK".to_vec(),
                b"two words".to_vec(),
            ]
        );
    }

    #[test]
    fn a_string_that_cannot_be_read_is_one_thing_told_once() {
        let lexed = tokens("'$Q' '$4' '$\"' 'é' 'tab\there' '→' 'open\nx := 1;");
        let said: Vec<String> = lexed.diags.iter().map(|d| d.to_string()).collect();
        assert_eq!(
            said,
            vec![
                "1:1: $Q is not a character: $ is followed by $, ', L, N, P, R, T or two hexadecimal digits",
                "1:6: $4 is not a character: write $ and two hexadecimal digits",
                "1:11: $\" belongs in a double-byte string; here a \" stands for itself",
                "1:16: the character 'é' is written $E9 in a string",
                "1:20: the character '\\t' is written $09 in a string",
                "1:31: '→' is not a single-byte character; a STRING holds codes up to $FF (WSTRING is not supported yet)",
                "1:35: a string that is not closed on its line",
            ]
        );
        let errors = lexed.toks.iter().filter(|t| t.tok == Tok::Error).count();
        assert_eq!(errors, 7);
        // What follows on the next line is read.
        assert!(lexed.toks.iter().any(|t| t.tok == Tok::Sym(":=")));
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
