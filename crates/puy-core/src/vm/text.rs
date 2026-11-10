//! The string routines of `System.SysUtils` (`docs/spec/semantics.md` 5.3) on UTF-16 code units.

use std::cmp::Ordering;

/// The range of `units` without the leading (`left`) or trailing (`right`) characters whose
/// code unit is at most `' '` (`Trim`, `TrimLeft`, `TrimRight`).
pub fn trimmed(units: &[u16], left: bool, right: bool) -> (usize, usize) {
    let mut start = 0;
    let mut end = units.len();
    if left {
        while start < end && units[start] <= 32 {
            start += 1;
        }
    }
    if right {
        while end > start && units[end - 1] <= 32 {
            end -= 1;
        }
    }
    (start, end)
}

/// An ASCII lowercase letter as uppercase (the folding of `SameText` / `CompareText`).
fn upper_ascii(u: u16) -> u16 {
    if (97..=122).contains(&u) { u - 32 } else { u }
}

/// `CompareText`: the code point order (§5.11) after folding ASCII letters to uppercase.
pub fn compare_text(a: &[u16], b: &[u16]) -> Ordering {
    let fa: Vec<u16> = a.iter().map(|u| upper_ascii(*u)).collect();
    let fb: Vec<u16> = b.iter().map(|u| upper_ascii(*u)).collect();
    crate::sema::compare_strings(&fa, &fb)
}

/// `SameText`: equal after folding ASCII letters.
pub fn same_text(a: &[u16], b: &[u16]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| upper_ascii(*x) == upper_ascii(*y))
}

/// `QuotedStr`: in `'`, with each `'` doubled.
pub fn quoted(units: &[u16]) -> Vec<u16> {
    let quote = u16::from(b'\'');
    let mut out = Vec::with_capacity(units.len() + 2);
    out.push(quote);
    for &u in units {
        out.push(u);
        if u == quote {
            out.push(quote);
        }
    }
    out.push(quote);
    out
}

/// `StrToBool`: `True` / `False` in any case, or an integer in the `StrToInt` syntax of any size
/// (0 is `False`). `None` otherwise.
pub fn parse_bool(units: &[u16]) -> Option<bool> {
    let text = String::from_utf16(units).ok()?;
    if text.eq_ignore_ascii_case("true") {
        return Some(true);
    }
    if text.eq_ignore_ascii_case("false") {
        return Some(false);
    }
    super::strconv::int_is_nonzero(units)
}

/// `StringReplace`: replaces the first (or with `all`, every) occurrence of `old` in `s` by
/// `new`. With `ignore_case`, characters match by their lowercase forms (§5.11: the Unicode case
/// mapping table, no language rules). An empty `old` replaces nothing.
pub fn replace(s: &[u16], old: &[u16], new: &[u16], all: bool, ignore_case: bool) -> Vec<u16> {
    if old.is_empty() {
        return s.to_vec();
    }
    // The characters of `s` with the code unit offset each starts at (folded when ignoring case;
    // a lone surrogate matches only itself)
    let key = |units: &[u16]| -> Vec<(usize, u32)> {
        let mut out = Vec::new();
        let mut i = 0;
        for c in chars(units) {
            let len = c.map_or(1, char::len_utf16);
            let code = match c {
                Ok(c) if !ignore_case => c as u32,
                _ => fold(c, false),
            };
            out.push((i, code));
            i += len;
        }
        out
    };
    let hay = key(s);
    let needle: Vec<u32> = key(old).into_iter().map(|(_, c)| c).collect();
    let mut out = Vec::with_capacity(s.len());
    let mut copied = 0; // code units of `s` already in `out`
    let mut k = 0; // the character of `hay` to try next
    while k + needle.len() <= hay.len() {
        if hay[k..k + needle.len()]
            .iter()
            .map(|(_, c)| *c)
            .eq(needle.iter().copied())
        {
            let start = hay[k].0;
            let end = hay.get(k + needle.len()).map_or(s.len(), |(i, _)| *i);
            out.extend_from_slice(&s[copied..start]);
            out.extend_from_slice(new);
            copied = end;
            k += needle.len();
            if !all {
                break;
            }
        } else {
            k += 1;
        }
    }
    out.extend_from_slice(&s[copied..]);
    out
}

/// The characters of `units` (a lone surrogate as itself).
fn chars(units: &[u16]) -> impl Iterator<Item = Result<char, u16>> + '_ {
    char::decode_utf16(units.iter().copied()).map(|c| c.map_err(|e| e.unpaired_surrogate()))
}

/// The single-character case mapping of `c` (§5.11: the Unicode table, no language or context
/// rules): its uppercase (`upper`) or lowercase form, or `None` when that is more than one
/// character (`ß`, `İ`).
fn map_single(c: char, upper: bool) -> Option<char> {
    fn single(mut it: impl Iterator<Item = char>) -> Option<char> {
        match (it.next(), it.next()) {
            (Some(m), None) => Some(m),
            _ => None,
        }
    }
    if upper {
        single(c.to_uppercase())
    } else {
        single(c.to_lowercase())
    }
}

/// The case mapping of `c` into `out`. Without `full`, a character whose mapping is more than
/// one character stays as it is ([`map_single`]).
fn map_case(c: char, upper: bool, full: bool, out: &mut Vec<char>) {
    if !full {
        out.push(map_single(c, upper).unwrap_or(c));
    } else if upper {
        out.extend(c.to_uppercase());
    } else {
        out.extend(c.to_lowercase());
    }
}

/// `AnsiUpperCase` / `AnsiLowerCase`: each character mapped by [`map_case`] (lone surrogates
/// unchanged).
pub fn convert_case(units: &[u16], upper: bool, full: bool) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut mapped = Vec::new();
    let mut buf = [0u16; 2];
    for c in chars(units) {
        match c {
            Ok(c) => {
                mapped.clear();
                map_case(c, upper, full, &mut mapped);
                for m in &mapped {
                    out.extend_from_slice(m.encode_utf16(&mut buf));
                }
            }
            Err(u) => out.push(u),
        }
    }
    out
}

/// The form a character is compared by when ignoring case: its single-character lowercase (or
/// with `upper`, uppercase) mapping. A lone surrogate is itself.
fn fold(c: Result<char, u16>, upper: bool) -> u32 {
    match c {
        Ok(c) => map_single(c, upper).unwrap_or(c) as u32,
        Err(u) => u32::from(u),
    }
}

/// `AnsiCompareText` (`AnsiSameText` is `Equal`): the code point order (§5.11) of the characters
/// folded by [`fold`].
pub fn compare_folded(a: &[u16], b: &[u16], upper: bool) -> Ordering {
    let fa = chars(a).map(|c| fold(c, upper));
    let fb = chars(b).map(|c| fold(c, upper));
    fa.cmp(fb)
}
