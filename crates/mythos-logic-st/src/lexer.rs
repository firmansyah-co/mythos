//! Structured Text tokens. Keywords and identifiers are case-insensitive (IEC 61131-3 §6.1.2);
//! a word keeps its spelling for messages and its upper-case form for matching.

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
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

const SYMS2: [&str; 7] = [":=", "=>", "<>", "<=", ">=", "**", ".."];
const SYMS1: [&str; 15] = [
    "=", "<", ">", "+", "-", "*", "/", "(", ")", ";", ":", ",", ".", "[", "]",
];

pub fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    macro_rules! advance {
        ($n:expr) => {
            for _ in 0..$n {
                if chars[i] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                i += 1;
            }
        };
    }
    while i < chars.len() {
        let c = chars[i];
        let span = Span { line, col };
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            advance!(1);
            continue;
        }
        // Comments: (* *), /* */, //
        if (c == '(' && next == Some('*')) || (c == '/' && next == Some('*')) {
            let close = if c == '(' { ')' } else { '/' };
            advance!(2);
            loop {
                if i + 1 >= chars.len() {
                    return Err(Diagnostic::at(span, "a comment that is never closed"));
                }
                if chars[i] == '*' && chars[i + 1] == close {
                    advance!(2);
                    break;
                }
                advance!(1);
            }
            continue;
        }
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                advance!(1);
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                advance!(1);
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_ascii_uppercase();
            if i < chars.len() && chars[i] == '#' {
                advance!(1);
                if upper == "T" || upper == "TIME" {
                    let (ns, used) = duration(&chars[i..]).map_err(|m| Diagnostic::at(span, m))?;
                    advance!(used);
                    out.push(Token {
                        tok: Tok::Time(ns),
                        span,
                    });
                    continue;
                }
                return Err(Diagnostic::at(
                    span,
                    format!("the typed literal {word}#… is not supported yet; only T# and TIME#"),
                ));
            }
            out.push(Token {
                tok: Tok::Word(upper, word),
                span,
            });
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                advance!(1);
            }
            let digits: String = chars[start..i].iter().filter(|c| **c != '_').collect();
            // Based integer: 16#FF, 8#17, 2#1010
            if i < chars.len() && chars[i] == '#' {
                let base: u32 = digits.parse().unwrap_or(0);
                if ![2, 8, 16].contains(&base) {
                    return Err(Diagnostic::at(
                        span,
                        format!("{base}# is not a base; use 2#, 8# or 16#"),
                    ));
                }
                advance!(1);
                let s = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    advance!(1);
                }
                let body: String = chars[s..i].iter().filter(|c| **c != '_').collect();
                let v = i128::from_str_radix(&body, base).map_err(|_| {
                    Diagnostic::at(span, format!("{body} is not a base-{base} number"))
                })?;
                out.push(Token {
                    tok: Tok::Int(v),
                    span,
                });
                continue;
            }
            // Real: digits '.' digits [E [+-] digits]; a '..' after an integer is a range.
            let is_real = i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit();
            let has_exp = |j: usize| j < chars.len() && (chars[j] == 'e' || chars[j] == 'E');
            if is_real || has_exp(i) {
                let mut text = digits;
                if is_real {
                    advance!(1);
                    text.push('.');
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                        if chars[i] != '_' {
                            text.push(chars[i]);
                        }
                        advance!(1);
                    }
                }
                if has_exp(i) {
                    text.push('e');
                    advance!(1);
                    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                        text.push(chars[i]);
                        advance!(1);
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        text.push(chars[i]);
                        advance!(1);
                    }
                }
                let v: f64 = text
                    .parse()
                    .map_err(|_| Diagnostic::at(span, format!("{text} is not a number")))?;
                out.push(Token {
                    tok: Tok::Real(v),
                    span,
                });
                continue;
            }
            let v: i128 = digits
                .parse()
                .map_err(|_| Diagnostic::at(span, format!("{digits} is too large")))?;
            out.push(Token {
                tok: Tok::Int(v),
                span,
            });
            continue;
        }
        // A direct address (%IX0.0): read whole, so the parser can say what to do instead.
        if c == '%' {
            let start = i;
            advance!(1);
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '*')
            {
                advance!(1);
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Token {
                tok: Tok::Word(text.to_ascii_uppercase(), text),
                span,
            });
            continue;
        }
        if c == '\'' || c == '"' {
            return Err(Diagnostic::at(span, "strings are not supported yet"));
        }
        if let Some(n) = next {
            let two: String = [c, n].iter().collect();
            if let Some(s) = SYMS2.iter().find(|s| **s == two) {
                advance!(2);
                out.push(Token {
                    tok: Tok::Sym(s),
                    span,
                });
                continue;
            }
        }
        let one = c.to_string();
        if let Some(s) = SYMS1.iter().find(|s| **s == one) {
            advance!(1);
            out.push(Token {
                tok: Tok::Sym(s),
                span,
            });
            continue;
        }
        if c == '&' {
            advance!(1);
            out.push(Token {
                tok: Tok::Word("AND".into(), "&".into()),
                span,
            });
            continue;
        }
        return Err(Diagnostic::at(
            span,
            format!("'{c}' is not part of Structured Text"),
        ));
    }
    out.push(Token {
        tok: Tok::Eof,
        span: Span { line, col },
    });
    Ok(out)
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
        assert_eq!(toks[1].span, Span { line: 2, col: 2 });
    }
}
