//! Conversion between strings and integers (`docs/spec/semantics.md` 5.2, 6.2).

/// Parses with the `StrToInt` syntax: leading blanks (` `, tab), a sign, then decimal or `$` + hexadecimal.
/// With `allow_trailing_blank`, trailing blanks (` `, tab) are skipped (`ReadLn`).
/// Returns the signed value (`i128`), or `None` if the syntax does not match.
pub fn parse_int(units: &[u16], allow_trailing_blank: bool) -> Option<i128> {
    let mut i = 0;
    while i < units.len() && matches!(units[i], 32 | 9) {
        i += 1;
    }
    let mut end = units.len();
    if allow_trailing_blank {
        while end > i && matches!(units[end - 1], 32 | 9) {
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
    if digits.is_empty() {
        return None;
    }
    let mut value: i128 = 0;
    for &u in digits {
        let c = char::from_u32(u as u32)?;
        let d = c.to_digit(radix)? as i128;
        value = value * radix as i128 + d;
        if value > u64::MAX as i128 * 2 {
            return None;
        }
    }
    Some(if negative { -value } else { value })
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
