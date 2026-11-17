//! The instructions on strings (format 2): IEC 61131-3 ed. 3 §6.4.2.2, §6.6.1.2.2 and
//! §6.6.2.5.11 with Table 34.
//!
//! A `STRING[n]` is `2 + n` bytes of the program's memory: its length now (u16, little-endian),
//! then room for `n` characters of one byte each. Nothing here allocates: every result is
//! written into a string the program declared (or the compiler set aside), and every byte
//! touched is reached through `get`/`get_mut`, so a damaged program stops with a [`Fault`] or
//! gives a wrong string, and never reaches outside the memory or panics.

use crate::vm::{copy, Fault};

/// A string the instruction names: where its `2 + cap` bytes start, and its length `cap`.
/// The caller has checked that the `2 + cap` bytes lie inside the memory.
#[derive(Clone, Copy)]
pub(crate) struct Str {
    pub cap: u16,
    pub at: u32,
}

impl Str {
    fn data(self) -> usize {
        self.at as usize + 2
    }
}

/// The length a string holds now. One greater than its declared length was not written by
/// the program's own instructions: it is never followed.
fn len(memory: &[u8], s: Str, pc: u32) -> Result<usize, Fault> {
    let a = s.at as usize;
    let b = memory.get(a..a + 2).ok_or(Fault::BadCode(pc))?;
    let n = u16::from_le_bytes([b[0], b[1]]);
    if n > s.cap {
        return Err(Fault::BadString(pc));
    }
    Ok(n as usize)
}

/// The characters of `s`, as a range of the memory.
fn chars(memory: &[u8], s: Str, pc: u32) -> Result<core::ops::Range<usize>, Fault> {
    let n = len(memory, s, pc)?;
    Ok(s.data()..s.data() + n)
}

fn set_len(memory: &mut [u8], s: Str, n: usize, pc: u32) -> Result<(), Fault> {
    let a = s.at as usize;
    memory
        .get_mut(a..a + 2)
        .ok_or(Fault::BadCode(pc))?
        .copy_from_slice(&(n as u16).to_le_bytes());
    Ok(())
}

/// Write the pieces of memory `parts`, one after another, as the string `dst`. Longer than
/// `dst` holds is `StringTooLong`: the string a function gives is never cut (§6.6.2.5.11:
/// producing one longer than the maximum is an error); assignment, which cuts, is [`assign`].
fn compose(
    memory: &mut [u8],
    dst: Str,
    parts: &[core::ops::Range<usize>],
    pc: u32,
) -> Result<(), Fault> {
    let total: usize = parts.iter().map(|r| r.len()).sum();
    if total > dst.cap as usize {
        return Err(Fault::StringTooLong(pc));
    }
    let mut at = dst.data();
    for part in parts {
        let n = part.len();
        copy(memory, part.clone(), at..at + n).ok_or(Fault::BadCode(pc))?;
        at += n;
    }
    set_len(memory, dst, total, pc)
}

/// `dst := src`, cut to `dst`'s length.
pub(crate) fn assign(memory: &mut [u8], dst: Str, src: Str, pc: u32) -> Result<(), Fault> {
    let from = chars(memory, src, pc)?;
    let n = from.len().min(dst.cap as usize);
    let to = dst.data()..dst.data() + n;
    copy(memory, from.start..from.start + n, to).ok_or(Fault::BadCode(pc))?;
    set_len(memory, dst, n, pc)
}

/// How `a` compares with `b`: -1, 0 or 1. The shorter is taken as extended on the right with
/// characters of code zero, and characters compare by their codes (§6.6.2.5.11), so 'Z' is
/// greater than 'AZ', and 'A' equals 'A$00'.
pub(crate) fn compare(memory: &[u8], a: Str, b: Str, pc: u32) -> Result<i32, Fault> {
    let (ra, rb) = (chars(memory, a, pc)?, chars(memory, b, pc)?);
    let at = |r: &core::ops::Range<usize>, k: usize| -> u8 {
        if k < r.len() {
            memory.get(r.start + k).copied().unwrap_or(0)
        } else {
            0
        }
    };
    for k in 0..ra.len().max(rb.len()) {
        let (x, y) = (at(&ra, k), at(&rb, k));
        if x != y {
            return Ok(if x < y { -1 } else { 1 });
        }
    }
    Ok(0)
}

pub(crate) fn length(memory: &[u8], a: Str, pc: u32) -> Result<i32, Fault> {
    len(memory, a, pc).map(|n| n as i32)
}

/// A length or a count: not below zero (§6.6.2.5.11).
fn count(n: i64, pc: u32) -> Result<usize, Fault> {
    usize::try_from(n).map_err(|_| Fault::StringArgument(pc))
}

/// The stretch of `n` characters from position `p` (the first is 1) of a string of `len`
/// characters, as indices from its start: every position in it must exist. With `n` zero,
/// `p` may be one past the end, which names no character.
fn stretch(len: usize, n: i64, p: i64, pc: u32) -> Result<core::ops::Range<usize>, Fault> {
    let n = count(n, pc)?;
    if p < 1 {
        return Err(Fault::StringArgument(pc));
    }
    let from = count(p - 1, pc)?;
    let to = from.checked_add(n).ok_or(Fault::StringArgument(pc))?;
    if to > len {
        return Err(Fault::StringArgument(pc));
    }
    Ok(from..to)
}

fn shift(r: &core::ops::Range<usize>, from: usize, to: usize) -> core::ops::Range<usize> {
    r.start + from..r.start + to
}

/// `CONCAT(a, b)`
pub(crate) fn concat(memory: &mut [u8], dst: Str, a: Str, b: Str, pc: u32) -> Result<(), Fault> {
    let (ra, rb) = (chars(memory, a, pc)?, chars(memory, b, pc)?);
    compose(memory, dst, &[ra, rb], pc)
}

/// `LEFT(a, n)` and `RIGHT(a, n)`.
pub(crate) fn side(
    memory: &mut [u8],
    dst: Str,
    a: Str,
    n: i64,
    left: bool,
    pc: u32,
) -> Result<(), Fault> {
    let r = chars(memory, a, pc)?;
    let n = count(n, pc)?;
    if n > r.len() {
        return Err(Fault::StringArgument(pc));
    }
    let part = if left {
        shift(&r, 0, n)
    } else {
        shift(&r, r.len() - n, r.len())
    };
    compose(memory, dst, &[part], pc)
}

/// `MID(a, n, p)`
pub(crate) fn mid(
    memory: &mut [u8],
    dst: Str,
    a: Str,
    n: i64,
    p: i64,
    pc: u32,
) -> Result<(), Fault> {
    let r = chars(memory, a, pc)?;
    let s = stretch(r.len(), n, p, pc)?;
    compose(memory, dst, &[shift(&r, s.start, s.end)], pc)
}

/// `DELETE(a, n, p)`
pub(crate) fn delete(
    memory: &mut [u8],
    dst: Str,
    a: Str,
    n: i64,
    p: i64,
    pc: u32,
) -> Result<(), Fault> {
    let r = chars(memory, a, pc)?;
    let s = stretch(r.len(), n, p, pc)?;
    let parts = [shift(&r, 0, s.start), shift(&r, s.end, r.len())];
    compose(memory, dst, &parts, pc)
}

/// `INSERT(a, b, p)`: `b` after the `p`-th character of `a`; 0 puts it first, `LEN(a)` last.
pub(crate) fn insert(
    memory: &mut [u8],
    dst: Str,
    a: Str,
    b: Str,
    p: i64,
    pc: u32,
) -> Result<(), Fault> {
    let (ra, rb) = (chars(memory, a, pc)?, chars(memory, b, pc)?);
    let p = count(p, pc)?;
    if p > ra.len() {
        return Err(Fault::StringArgument(pc));
    }
    let parts = [shift(&ra, 0, p), rb, shift(&ra, p, ra.len())];
    compose(memory, dst, &parts, pc)
}

/// `REPLACE(a, b, n, p)`
#[allow(clippy::too_many_arguments)]
pub(crate) fn replace(
    memory: &mut [u8],
    dst: Str,
    a: Str,
    b: Str,
    n: i64,
    p: i64,
    pc: u32,
) -> Result<(), Fault> {
    let (ra, rb) = (chars(memory, a, pc)?, chars(memory, b, pc)?);
    let s = stretch(ra.len(), n, p, pc)?;
    let parts = [shift(&ra, 0, s.start), rb, shift(&ra, s.end, ra.len())];
    compose(memory, dst, &parts, pc)
}

/// `FIND(a, b)`: the first position `p` (from 1) at which the characters of `b` stand in `a`,
/// or 0. An empty `b` stands at position 1.
pub(crate) fn find(memory: &[u8], a: Str, b: Str, pc: u32) -> Result<i32, Fault> {
    let (ra, rb) = (chars(memory, a, pc)?, chars(memory, b, pc)?);
    let (Some(ha), Some(hb)) = (memory.get(ra), memory.get(rb)) else {
        return Err(Fault::BadCode(pc));
    };
    if hb.len() > ha.len() {
        return Ok(0);
    }
    for p in 0..=ha.len() - hb.len() {
        if ha.get(p..p + hb.len()) == Some(hb) {
            return Ok(p as i32 + 1);
        }
    }
    Ok(0)
}

/// The decimal text of `v`.
pub(crate) fn from_int(memory: &mut [u8], dst: Str, v: i64, pc: u32) -> Result<(), Fault> {
    // i64::MIN has 19 digits and a sign.
    let mut text = [0u8; 20];
    let mut at = text.len();
    let mut rest = v.unsigned_abs();
    loop {
        at -= 1;
        text[at] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    if v < 0 {
        at -= 1;
        text[at] = b'-';
    }
    let digits = &text[at..];
    if digits.len() > dst.cap as usize {
        return Err(Fault::StringTooLong(pc));
    }
    memory
        .get_mut(dst.data()..dst.data() + digits.len())
        .ok_or(Fault::BadCode(pc))?
        .copy_from_slice(digits);
    set_len(memory, dst, digits.len(), pc)
}

/// The integer `a` writes as an integer literal (§6.3.2, Table 5): `[+|-]` decimal digits, or
/// `2#`, `8#`, `16#` and digits of that base (no sign), underscores single and between
/// digits as Annex A allows them. Nothing else is taken: not a space, not a type prefix.
pub(crate) fn to_int(memory: &[u8], a: Str, pc: u32) -> Result<i64, Fault> {
    let r = chars(memory, a, pc)?;
    let text = memory.get(r).ok_or(Fault::BadCode(pc))?;
    parse_int(text).ok_or(Fault::BadNumber(pc))
}

fn parse_int(text: &[u8]) -> Option<i64> {
    let (negative, body) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    // `16#…`: unsigned, and the underscore may also stand right after the `#`.
    let (radix, digits, lead) = match body.iter().position(|c| *c == b'#') {
        Some(hash) => {
            let radix = match &body[..hash] {
                b"2" => 2,
                b"8" => 8,
                b"16" => 16,
                _ => return None,
            };
            if negative || text.first() == Some(&b'+') {
                return None;
            }
            (radix, &body[hash + 1..], true)
        }
        None => (10, body, false),
    };
    // Digits, an underscore only between two of them (or, after `#`, before the first).
    let mut value: u64 = 0;
    let mut seen = 0usize;
    let mut underscore = false;
    for &c in digits {
        if c == b'_' {
            if underscore || (seen == 0 && !lead) {
                return None;
            }
            underscore = true;
            continue;
        }
        let d = (c as char).to_digit(radix)?;
        value = value.checked_mul(radix as u64)?.checked_add(d as u64)?;
        seen += 1;
        underscore = false;
    }
    if seen == 0 || digits.last() == Some(&b'_') {
        return None;
    }
    if negative {
        // -9223372036854775808 is the one value whose magnitude is past i64::MAX.
        0i64.checked_sub_unsigned(value)
    } else {
        i64::try_from(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_int;

    #[test]
    fn an_integer_is_read_only_in_the_form_of_a_literal() {
        let ok = |t: &str| parse_int(t.as_bytes());
        assert_eq!(ok("42"), Some(42));
        assert_eq!(ok("-42"), Some(-42));
        assert_eq!(ok("+7"), Some(7));
        assert_eq!(ok("1_000"), Some(1000));
        assert_eq!(ok("16#FF"), Some(255));
        assert_eq!(ok("16#_ff"), Some(255));
        assert_eq!(ok("2#1010_1010"), Some(170));
        assert_eq!(ok("8#17"), Some(15));
        assert_eq!(ok("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(ok("9223372036854775807"), Some(i64::MAX));
        for bad in [
            "",
            "-",
            " 42",
            "42 ",
            "4 2",
            "1__0",
            "_1",
            "1_",
            "16#",
            "16#_",
            "16#__F",
            "-16#FF",
            "+16#FF",
            "10#5",
            "16#FG",
            "2#2",
            "INT#5",
            "1.5",
            "9223372036854775808",
            "-9223372036854775809",
        ] {
            assert_eq!(ok(bad), None, "{bad:?}");
        }
    }
}
