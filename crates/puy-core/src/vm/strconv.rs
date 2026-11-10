//! Conversion between strings and integers (`docs/spec/semantics.md` 5.2, 6.2).

/// A blank of the number syntax and of the input words: ` ` or tab (`docs/spec/semantics.md` 5.2,
/// 6.2).
pub fn is_blank(u: u16) -> bool {
    u == 32 || u == 9
}

/// The parts of a string in the `StrToInt` syntax: leading blanks (` `, tab), a sign, then decimal
/// or `$` + hexadecimal digits. With `allow_trailing_blank`, trailing blanks (` `, tab) are
/// skipped (`ReadLn`). Returns whether it is negative, the radix and the digits (all valid), or
/// `None` if the syntax does not match.
fn int_syntax(units: &[u16], allow_trailing_blank: bool) -> Option<(bool, u32, &[u16])> {
    let mut i = 0;
    while i < units.len() && is_blank(units[i]) {
        i += 1;
    }
    let mut end = units.len();
    if allow_trailing_blank {
        while end > i && is_blank(units[end - 1]) {
            end -= 1;
        }
    }
    let s = &units[i..end];
    if s.is_empty() {
        return None;
    }
    let mut j = 0;
    let negative = match s[0] {
        43 => {
            j = 1;
            false
        }
        45 => {
            j = 1;
            true
        }
        _ => false,
    };
    let (radix, digits) = if s.get(j) == Some(&36) {
        (16u32, &s[j + 1..])
    } else {
        (10u32, &s[j..])
    };
    let valid = digits
        .iter()
        .all(|&u| char::from_u32(u as u32).is_some_and(|c| c.is_digit(radix)));
    (!digits.is_empty() && valid).then_some((negative, radix, digits))
}

/// Parses with the `StrToInt` syntax ([`int_syntax`]). Returns the signed value (`i128`), or
/// `None` if the syntax does not match or the value is far beyond the 64-bit range.
pub fn parse_int(units: &[u16], allow_trailing_blank: bool) -> Option<i128> {
    let (negative, radix, digits) = int_syntax(units, allow_trailing_blank)?;
    let mut value: i128 = 0;
    for &u in digits {
        let d = char::from_u32(u as u32)?.to_digit(radix)? as i128;
        value = value * radix as i128 + d;
        if value > u64::MAX as i128 * 2 {
            return None;
        }
    }
    Some(if negative { -value } else { value })
}

/// Whether an integer in the `StrToInt` syntax is not 0, for any number of digits (`StrToBool`;
/// `docs/spec/semantics.md` 5.3). `None` if the syntax does not match.
pub fn int_is_nonzero(units: &[u16]) -> Option<bool> {
    let (_, _, digits) = int_syntax(units, false)?;
    Some(digits.iter().any(|&u| u != u16::from(b'0')))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn parse() {
        assert_eq!(parse_int(&u("42"), false), Some(42));
        assert_eq!(parse_int(&u("  -42"), false), Some(-42));
        assert_eq!(parse_int(&u("+$FF"), false), Some(255));
        assert_eq!(parse_int(&u("42 "), false), None);
        assert_eq!(parse_int(&u("42 \t"), true), Some(42));
        assert_eq!(parse_int(&u("12 abc"), true), None);
        assert_eq!(parse_int(&u("0x1F"), false), None);
        assert_eq!(parse_int(&u(""), false), None);
        assert_eq!(parse_int(&u("$"), false), None);
        assert_eq!(parse_int(&u("-"), false), None);
        assert_eq!(
            parse_int(&u("18446744073709551615"), false),
            Some(18446744073709551615)
        );
    }
}
