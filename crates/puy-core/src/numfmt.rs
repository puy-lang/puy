//! Text forms and conversions of reals and `Currency` (`docs/spec/types.md` 8.4,
//! `docs/spec/semantics.md` 6.1, 5.3). Shared by constant folding, the dumps and the VM, so that
//! a folded constant and the value computed at run time give the same text.

/// `Currency` values are the value times this.
pub const CURR_SCALE: i64 = 10000;

/// The default text of a `Double` (`docs/spec/semantics.md` 6.1): the shortest digits that read
/// back to the same value, in fixed notation when the decimal exponent is in −5..14, otherwise as
/// `digitsEexp`.
pub fn f64_to_str(v: f64) -> String {
    if v.is_nan() {
        return "NAN".into();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-INF" } else { "INF" }.into();
    }
    layout_sci(&format!("{v:e}"))
}

/// The default text of a `Single`: the shortest digits that read back as binary32.
pub fn f32_to_str(v: f32) -> String {
    if v.is_nan() {
        return "NAN".into();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-INF" } else { "INF" }.into();
    }
    layout_sci(&format!("{v:e}"))
}

/// Lays out the digits of a Rust `{:e}` form (`-1.5e-7`) by the rules of 6.1 (the digits as
/// given: `Format`'s `g` passes digits rounded to a precision).
pub(crate) fn layout_sci(sci: &str) -> String {
    let (mantissa, exp) = sci.split_once('e').expect("`{:e}` has an exponent");
    let exp: i32 = exp.parse().expect("`{:e}` has a decimal exponent");
    let (neg, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if (-5..15).contains(&exp) {
        if exp < 0 {
            out.push_str("0.");
            for _ in 0..(-exp - 1) {
                out.push('0');
            }
            out.push_str(&digits);
        } else {
            let int_len = exp as usize + 1;
            if digits.len() <= int_len {
                out.push_str(&digits);
                for _ in digits.len()..int_len {
                    out.push('0');
                }
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('E');
        out.push_str(&exp.to_string());
    }
    out
}

/// `X:W:D` of a real (`docs/spec/semantics.md` 6.1): the exact decimal value of the binary value
/// rounded to `d` decimals, ties to even (`d <= 0`: no decimal point). INF and NaN use the default
/// text. `None` if the text would exceed the string length limit.
pub fn f64_fixed(v: f64, d: i64) -> Option<String> {
    if !v.is_finite() {
        return Some(f64_to_str(v));
    }
    let d = d.max(0);
    // The integer part has at most 309 digits
    if d > crate::sema::types::MAX_STRING_LEN as i64 - 400 {
        return None;
    }
    // Rust's fixed-precision formatting rounds the exact binary value to nearest, ties to even
    // (`fixed_text_rounds_the_exact_value` checks the ties)
    Some(format!("{v:.*}", d as usize))
}

/// The default text of a `Currency`: fixed notation without trailing zeros of the fraction, and
/// without `.` when there is no fraction (`1.5`, `100`, `0.0002`).
pub fn curr_to_str(c: i64) -> String {
    curr_text_wide(c as i128)
}

/// The decimal text of a scaled `Currency` value that may be out of range (for E0714):
/// `curr_to_str` of the wide value.
pub fn curr_text_wide(v: i128) -> String {
    let neg = v < 0;
    let a = v.unsigned_abs();
    let int = a / CURR_SCALE as u128;
    let frac = a % CURR_SCALE as u128;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&int.to_string());
    if frac != 0 {
        let f = format!("{frac:04}");
        out.push('.');
        out.push_str(f.trim_end_matches('0'));
    }
    out
}

/// `X:W:D` of a `Currency`: the value rounded to `d` decimals, ties to even.
pub fn curr_fixed(c: i64, d: i64) -> Option<String> {
    let d = d.max(0);
    if d > crate::sema::types::MAX_STRING_LEN as i64 - 40 {
        return None;
    }
    let neg = c < 0;
    let a = c.unsigned_abs() as u128;
    let (int, frac) = if d >= 4 {
        // Exact: pad the four decimals with zeros
        let s = CURR_SCALE as u128;
        (a / s, format!("{:04}{}", a % s, "0".repeat(d as usize - 4)))
    } else {
        // Round to `d` decimals: the value in units of 10^-d
        let unit = 10u128.pow(4 - d as u32);
        let q = div_round_even(a, unit);
        let p = 10u128.pow(d as u32);
        let f = if d == 0 {
            String::new()
        } else {
            format!("{:0width$}", q % p, width = d as usize)
        };
        (q / p, f)
    };
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&int.to_string());
    if d > 0 {
        out.push('.');
        out.push_str(&frac);
    }
    Some(out)
}

/// `a / b` rounded to the nearest integer, ties to even (`b > 0`).
fn div_round_even(a: u128, b: u128) -> u128 {
    let q = a / b;
    let r = a % b;
    if r * 2 > b || (r * 2 == b && q % 2 == 1) {
        q + 1
    } else {
        q
    }
}

/// `a / b` rounded to the nearest integer, ties to even (`b != 0`).
fn idiv_round_even(a: i128, b: i128) -> i128 {
    let neg = (a < 0) != (b < 0);
    let q = div_round_even(a.unsigned_abs(), b.unsigned_abs()) as i128;
    if neg { -q } else { q }
}

/// Whether the scaled value is in the range of `Currency` (that of `Int64`).
fn curr_in_range(v: i128) -> Option<i64> {
    i64::try_from(v).ok()
}

/// An integer as a `Currency` (`None`: out of range; `EOverflow`).
pub fn int_to_curr(v: i128) -> Option<i64> {
    curr_in_range(v.checked_mul(CURR_SCALE as i128)?)
}

pub fn curr_add(a: i64, b: i64) -> Option<i64> {
    a.checked_add(b)
}

pub fn curr_sub(a: i64, b: i64) -> Option<i64> {
    a.checked_sub(b)
}

/// The product, rounded to four decimals (ties to even).
pub fn curr_mul(a: i64, b: i64) -> Option<i64> {
    curr_in_range(idiv_round_even(a as i128 * b as i128, CURR_SCALE as i128))
}

/// The quotient, rounded to four decimals (ties to even). `Err(())` for a zero divisor
/// (`EZeroDivide`), `Ok(None)` when out of range (`EOverflow`).
#[allow(clippy::result_unit_err)]
pub fn curr_div(a: i64, b: i64) -> Result<Option<i64>, ()> {
    if b == 0 {
        return Err(());
    }
    Ok(curr_in_range(idiv_round_even(
        a as i128 * CURR_SCALE as i128,
        b as i128,
    )))
}

/// A real as a `Currency`: the exact binary value times 10000, rounded to an integer (ties to
/// even). `None` for NaN, an infinity or a value out of range (`EOverflow`).
pub fn f64_to_curr(v: f64) -> Option<i64> {
    if !v.is_finite() {
        return None;
    }
    if v == 0.0 {
        return Some(0);
    }
    let bits = v.to_bits();
    let neg = bits >> 63 != 0;
    let exp_bits = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    // v = m * 2^e
    let (m, e) = if exp_bits == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp_bits - 1075)
    };
    let scaled = m as u128 * CURR_SCALE as u128; // < 2^67
    let mag: u128 = if e >= 0 {
        if e > 63 {
            return None;
        }
        scaled.checked_shl(e as u32).filter(|x| x >> e == scaled)?
    } else if -e >= 100 {
        0
    } else {
        div_round_even(scaled, 1u128 << (-e) as u32)
    };
    let mag = i128::try_from(mag).ok()?;
    curr_in_range(if neg { -mag } else { mag })
}

/// The decimal text of a scaled `Currency` (`-12.3456`), exact.
fn curr_decimal(c: i64) -> String {
    let neg = c < 0;
    let a = c.unsigned_abs();
    format!(
        "{}{}.{:04}",
        if neg { "-" } else { "" },
        a / CURR_SCALE as u64,
        a % CURR_SCALE as u64
    )
}

/// A `Currency` as the nearest `Double`.
pub fn curr_to_f64(c: i64) -> f64 {
    if c.unsigned_abs() < 1 << 53 {
        // Both exact, so the one division rounds correctly
        c as f64 / CURR_SCALE as f64
    } else {
        curr_decimal(c)
            .parse()
            .expect("the decimal text of a Currency parses")
    }
}

/// A `Currency` as the nearest `Single` (one rounding).
pub fn curr_to_f32(c: i64) -> f32 {
    curr_decimal(c)
        .parse()
        .expect("the decimal text of a Currency parses")
}

/// An integer as the nearest `Double`.
pub fn int_to_f64(v: i128) -> f64 {
    v as f64
}

/// `Trunc` and `Round` (`docs/spec/semantics.md` 5.3): `None` for NaN, an infinity or a result
/// outside `Int64` (`EInvalidOp`).
pub fn f64_to_int(v: f64, round: bool) -> Option<i64> {
    let r = if round {
        v.round_ties_even()
    } else {
        v.trunc()
    };
    // -2^63 is exact; 2^63 is the first value out of range
    if r.is_nan() || !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&r) {
        return None;
    }
    Some(r as i64)
}

/// `Trunc` / `Round` of a `Currency` (always in range).
pub fn curr_to_int(c: i64, round: bool) -> i64 {
    if round {
        idiv_round_even(c as i128, CURR_SCALE as i128) as i64
    } else {
        c / CURR_SCALE
    }
}

/// `StrToFloat` (`docs/spec/semantics.md` 5.3): leading blanks, a sign, decimal digits, `.` and a
/// fraction, `e` / `E` and an exponent; or `INF`, `-INF`, `NAN` (any case). Nothing may follow.
pub fn parse_f64(units: &[u16]) -> Option<f64> {
    let mut i = 0;
    while i < units.len() && matches!(units[i], 32 | 9) {
        i += 1;
    }
    let s: String = char::decode_utf16(units[i..].iter().copied())
        .collect::<Result<_, _>>()
        .ok()?;
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, &s[..]),
    };
    if body.eq_ignore_ascii_case("inf") {
        return Some(if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    if body.eq_ignore_ascii_case("nan") {
        return Some(f64::NAN);
    }
    let b = body.as_bytes();
    let mut j = 0;
    let digits = |j: &mut usize| {
        let start = *j;
        while *j < b.len() && b[*j].is_ascii_digit() {
            *j += 1;
        }
        *j - start
    };
    let int_digits = digits(&mut j);
    let mut frac_digits = 0;
    if j < b.len() && b[j] == b'.' {
        j += 1;
        frac_digits = digits(&mut j);
    }
    if int_digits + frac_digits == 0 {
        return None;
    }
    if j < b.len() && (b[j] == b'e' || b[j] == b'E') {
        j += 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if digits(&mut j) == 0 {
            return None;
        }
    }
    if j != b.len() {
        return None;
    }
    let v: f64 = body.parse().ok()?;
    Some(if neg { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The default text follows the examples of semantics.md 6.1.
    #[test]
    fn default_text_of_doubles() {
        for (v, s) in [
            (1.5, "1.5"),
            (100.0, "100"),
            (0.0001, "0.0001"),
            (0.00001, "0.00001"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e20, "1E20"),
            (1.5e-7, "1.5E-7"),
            (1e15, "1E15"),
            (123456789012345.0, "123456789012345"),
            (-0.0, "-0"),
            (-2.5, "-2.5"),
            (f64::INFINITY, "INF"),
            (f64::NEG_INFINITY, "-INF"),
            (f64::NAN, "NAN"),
        ] {
            assert_eq!(f64_to_str(v), s, "{v}");
        }
    }

    // A Single is shown with the digits that read back as binary32.
    #[test]
    fn default_text_of_singles() {
        assert_eq!(f32_to_str(0.1), "0.1");
        assert_eq!(f32_to_str(16777217.0), "16777216");
    }

    // `X:W:D` rounds the exact binary value, ties to even.
    #[test]
    fn fixed_text_rounds_the_exact_value() {
        for (v, d, s) in [
            (0.125, 2, "0.12"),
            (0.375, 2, "0.38"),
            (2.5, 0, "2"),
            (2.675, 2, "2.67"),
            (1.0, -1, "1"),
            (f64::NAN, 2, "NAN"),
        ] {
            assert_eq!(f64_fixed(v, d).unwrap(), s, "{v}:{d}");
        }
    }

    // Currency text: trailing zeros dropped, and the fixed form rounded ties to even.
    #[test]
    fn currency_text() {
        assert_eq!(curr_to_str(15000), "1.5");
        assert_eq!(curr_to_str(1_000_000), "100");
        assert_eq!(curr_to_str(2), "0.0002");
        assert_eq!(curr_to_str(-15000), "-1.5");
        assert_eq!(curr_fixed(12500, 1).unwrap(), "1.2");
        assert_eq!(curr_fixed(13500, 0).unwrap(), "1");
        assert_eq!(curr_fixed(15000, 0).unwrap(), "2");
        assert_eq!(curr_fixed(-15, 2).unwrap(), "-0.00");
        assert_eq!(curr_fixed(12345, 6).unwrap(), "1.234500");
    }

    // Currency arithmetic rounds ties to even and reports overflow and a zero divisor.
    #[test]
    fn currency_arithmetic() {
        assert_eq!(curr_mul(15000, 15000), Some(22500));
        // 0.0001 * 0.5 = 0.00005 -> 0 (tie to even)
        assert_eq!(curr_mul(1, 5000), Some(0));
        assert_eq!(curr_mul(3, 5000), Some(2));
        assert_eq!(curr_div(10000, 30000), Ok(Some(3333)));
        assert_eq!(curr_div(1, 0), Err(()));
        assert_eq!(curr_mul(i64::MAX, 20000), None);
        assert_eq!(
            int_to_curr(922_337_203_685_477),
            Some(9_223_372_036_854_770_000)
        );
        assert_eq!(int_to_curr(922_337_203_685_478), None);
    }

    // A real converted to Currency uses the exact binary value.
    #[test]
    fn real_to_currency() {
        assert_eq!(f64_to_curr(0.1), Some(1000));
        // 0.00005 is a little above the tie in binary
        assert_eq!(f64_to_curr(0.00005), Some(1));
        // 1/32 and 3/32 are exact ties: 312.5 and 937.5 go to even
        assert_eq!(f64_to_curr(0.03125), Some(312));
        assert_eq!(f64_to_curr(0.09375), Some(938));
        assert_eq!(f64_to_curr(-1.5), Some(-15000));
        assert_eq!(f64_to_curr(f64::NAN), None);
        assert_eq!(f64_to_curr(1e30), None);
        assert_eq!(curr_to_f64(1000), 0.1);
    }

    // Trunc / Round fail outside Int64.
    #[test]
    fn real_to_integer() {
        assert_eq!(f64_to_int(2.5, true), Some(2));
        assert_eq!(f64_to_int(-2.7, false), Some(-2));
        assert_eq!(f64_to_int(9.3e18, false), None);
        assert_eq!(
            f64_to_int(-9_223_372_036_854_775_808.0, false),
            Some(i64::MIN)
        );
        assert_eq!(f64_to_int(f64::NAN, true), None);
    }

    // StrToFloat accepts the forms of 5.3 and nothing after them.
    #[test]
    fn parse_reals() {
        let p = |s: &str| parse_f64(&s.encode_utf16().collect::<Vec<_>>());
        assert_eq!(p("  1.5"), Some(1.5));
        assert_eq!(p("-2e3"), Some(-2000.0));
        assert_eq!(p(".5"), Some(0.5));
        assert_eq!(p("-inf"), Some(f64::NEG_INFINITY));
        assert!(p("NaN").unwrap().is_nan());
        assert_eq!(p("1.5 "), None);
        assert_eq!(p("1e"), None);
        assert_eq!(p("."), None);
        assert_eq!(p("1,5"), None);
        assert_eq!(p(""), None);
    }
}
