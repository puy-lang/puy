//! `Format` (`docs/spec/semantics.md` 5.3): reads the format and lays out the arguments of the
//! `array of const`.

use crate::numfmt;
use crate::sema::types::MAX_STRING_LEN;

/// An element of the `array of const`, read from its register by its kind
/// (`docs/spec/bytecode.md` 2.16).
pub enum FmtArg {
    /// An integer and the bit width of its type (for `u` and `x` of a negative value).
    Int {
        value: i128,
        bits: u32,
    },
    Char(u16),
    Bool,
    Str(Vec<u16>),
    Single(f32),
    Double(f64),
    Curr(i64),
}

/// Why `Format` failed.
#[derive(Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The format and the arguments do not match (`EConvertError`).
    Invalid,
    /// The result would exceed the string length limit (`EOutOfMemory`).
    TooLong,
}

/// The default precision of `f` and `n`.
const FIXED_DEFAULT: i64 = 2;

/// The largest precision that can give a string within the limit (the integer part of a `Double`
/// has at most 309 digits; `numfmt::f64_fixed` uses the same margin).
const MAX_PRECISION: i64 = MAX_STRING_LEN as i64 - 400;

/// The precision `g` rounds to at most (more than the significant digits of any `Double`).
const G_MAX_DIGITS: i64 = 800;

/// Formats `args` by `fmt`. `room` is the most code units the result can have (`Heap::str_room`):
/// widths and precisions over it fail with `TooLong` before the text is built.
pub fn format(fmt: &[u16], args: &[FmtArg], room: usize) -> Result<Vec<u16>, FormatError> {
    let mut out: Vec<u16> = Vec::new();
    let mut i = 0;
    let mut next = 0usize;
    while i < fmt.len() {
        let u = fmt[i];
        i += 1;
        if u != u16::from(b'%') {
            out.push(u);
            continue;
        }
        if fmt.get(i) == Some(&u16::from(b'%')) {
            out.push(u);
            i += 1;
            continue;
        }
        let mut spec = Spec::default();
        let mut tok = read_number(fmt, &mut i, args, &mut next)?;
        if fmt.get(i) == Some(&u16::from(b':')) {
            // An index (0-based) moves the position of the next argument
            let Some(Num::Digits(n)) = tok else {
                return Err(FormatError::Invalid);
            };
            next = usize::try_from(n).map_err(|_| FormatError::Invalid)?;
            i += 1;
            tok = None;
        }
        if tok.is_none() {
            if fmt.get(i) == Some(&u16::from(b'-')) {
                spec.left = true;
                i += 1;
            }
            tok = read_number(fmt, &mut i, args, &mut next)?;
        }
        if let Some(w) = tok {
            let w = w.value();
            // A negative width from `*` aligns to the left
            if w < 0 {
                spec.left = true;
            }
            spec.width = w.unsigned_abs().min(u64::MAX as u128) as u64;
        }
        if fmt.get(i) == Some(&u16::from(b'.')) {
            i += 1;
            let p = read_number(fmt, &mut i, args, &mut next)?.map_or(0, Num::value);
            // A negative precision from `*` counts as none
            spec.precision = (p >= 0).then(|| p.min(i64::MAX as i128) as i64);
        }
        let Some(&kind) = fmt.get(i) else {
            return Err(FormatError::Invalid);
        };
        i += 1;
        let arg = args.get(next).ok_or(FormatError::Invalid)?;
        next += 1;
        let text = layout(kind, arg, spec.precision, room)?;
        if spec.width > room as u64 {
            return Err(FormatError::TooLong);
        }
        let pad = (spec.width as usize).saturating_sub(text.len());
        if !spec.left {
            out.extend(std::iter::repeat_n(u16::from(b' '), pad));
        }
        out.extend(&text);
        if spec.left {
            out.extend(std::iter::repeat_n(u16::from(b' '), pad));
        }
        if out.len() > room {
            return Err(FormatError::TooLong);
        }
    }
    Ok(out)
}

#[derive(Default)]
struct Spec {
    left: bool,
    width: u64,
    precision: Option<i64>,
}

/// A width, precision or index as written.
enum Num {
    Digits(u64),
    /// Taken from an argument by `*`.
    Arg(i64),
}

impl Num {
    fn value(self) -> i128 {
        match self {
            Num::Digits(n) => n as i128,
            Num::Arg(n) => n as i128,
        }
    }
}

/// Reads decimal digits or `*` (which takes the next argument, an integer).
fn read_number(
    fmt: &[u16],
    i: &mut usize,
    args: &[FmtArg],
    next: &mut usize,
) -> Result<Option<Num>, FormatError> {
    if fmt.get(*i) == Some(&u16::from(b'*')) {
        *i += 1;
        let Some(FmtArg::Int { value, .. }) = args.get(*next) else {
            return Err(FormatError::Invalid);
        };
        *next += 1;
        let v = (*value).clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        return Ok(Some(Num::Arg(v)));
    }
    let start = *i;
    let mut n: u64 = 0;
    while let Some(d) = fmt
        .get(*i)
        .and_then(|u| char::from_u32(u32::from(*u))?.to_digit(10))
    {
        n = n.saturating_mul(10).saturating_add(u64::from(d));
        *i += 1;
    }
    Ok((*i > start).then_some(Num::Digits(n)))
}

/// The text of one argument by the kind letter (case-insensitive).
fn layout(
    kind: u16,
    arg: &FmtArg,
    precision: Option<i64>,
    room: usize,
) -> Result<Vec<u16>, FormatError> {
    let kind = char::from_u32(u32::from(kind))
        .ok_or(FormatError::Invalid)?
        .to_ascii_lowercase();
    let text = match (kind, arg) {
        ('d', FmtArg::Int { value, .. }) => {
            check_precision(precision, room)?;
            min_digits(*value, &value.unsigned_abs().to_string(), precision)
        }
        ('u', FmtArg::Int { value, bits }) => {
            check_precision(precision, room)?;
            min_digits(0, &unsigned(*value, *bits).to_string(), precision)
        }
        ('x', FmtArg::Int { value, bits }) => {
            check_precision(precision, room)?;
            min_digits(0, &format!("{:X}", unsigned(*value, *bits)), precision)
        }
        ('s', FmtArg::Str(s)) => return Ok(truncated(s, precision)),
        ('s', FmtArg::Char(c)) => return Ok(truncated(&[*c], precision)),
        ('f' | 'n', FmtArg::Curr(c)) => {
            check_precision(precision, room)?;
            let t = numfmt::curr_fixed(*c, precision.unwrap_or(FIXED_DEFAULT))
                .ok_or(FormatError::TooLong)?;
            if kind == 'n' { grouped(&t) } else { t }
        }
        ('f' | 'n' | 'e' | 'g', _) => {
            let v = match arg {
                FmtArg::Double(v) => *v,
                FmtArg::Single(v) => f64::from(*v),
                FmtArg::Curr(c) => numfmt::curr_to_f64(*c),
                _ => return Err(FormatError::Invalid),
            };
            match kind {
                'f' | 'n' => {
                    if v.is_finite() {
                        check_precision(precision, room)?;
                    }
                    let t = numfmt::f64_fixed(v, precision.unwrap_or(FIXED_DEFAULT))
                        .ok_or(FormatError::TooLong)?;
                    if kind == 'n' && v.is_finite() {
                        grouped(&t)
                    } else {
                        t
                    }
                }
                'e' => {
                    check_precision(precision, room)?;
                    exponential(v, precision, matches!(arg, FmtArg::Single(_)))
                }
                _ => general(v, precision, matches!(arg, FmtArg::Single(_))),
            }
        }
        _ => return Err(FormatError::Invalid),
    };
    Ok(text.encode_utf16().collect())
}

/// `EOutOfMemory` for a precision that adds more digits than a string can hold or than `room`
/// allows (`d`, `u`, `x` and `e`, and `f` / `n` of a finite value, give at least `precision`
/// digits; the precision of `s` and `g` only shortens).
fn check_precision(precision: Option<i64>, room: usize) -> Result<(), FormatError> {
    match precision {
        Some(p) if p > MAX_PRECISION || p as u64 > room as u64 => Err(FormatError::TooLong),
        _ => Ok(()),
    }
}

/// A negative value as the unsigned value of its type's width (two's complement).
fn unsigned(value: i128, bits: u32) -> u128 {
    if value < 0 {
        (value + (1i128 << bits)) as u128
    } else {
        value as u128
    }
}

/// `digits` padded on the left with `0` to `precision` digits, after a `-` if `value` is negative.
fn min_digits(value: i128, digits: &str, precision: Option<i64>) -> String {
    let mut out = String::new();
    if value < 0 {
        out.push('-');
    }
    let p = precision.unwrap_or(0).max(0) as usize;
    for _ in digits.len()..p {
        out.push('0');
    }
    out.push_str(digits);
    out
}

/// A string cut to at most `precision` code units.
fn truncated(s: &[u16], precision: Option<i64>) -> Vec<u16> {
    match precision {
        Some(p) if (p.max(0) as usize) < s.len() => s[..p.max(0) as usize].to_vec(),
        _ => s.to_vec(),
    }
}

/// `n`: the fixed text with `,` between the groups of three digits of the integer part.
fn grouped(fixed: &str) -> String {
    let (sign, rest) = match fixed.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", fixed),
    };
    let (int, frac) = match rest.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let mut out = String::from(sign);
    for (k, c) in int.chars().enumerate() {
        if k > 0 && (int.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    out
}

/// `e`: `d.ddd…E+ddd` with `precision` significant digits (the shortest digits that read back
/// when omitted; a `Single` as binary32). INF and NaN use the default text.
fn exponential(v: f64, precision: Option<i64>, single: bool) -> String {
    if !v.is_finite() {
        return numfmt::f64_to_str(v);
    }
    let sci = match precision {
        Some(p) => format!("{v:.*e}", (p.max(1) - 1) as usize),
        None if single => format!("{:e}", v as f32),
        None => format!("{v:e}"),
    };
    let (mantissa, exp) = sci.split_once('e').expect("`{:e}` has an exponent");
    let exp: i32 = exp.parse().expect("`{:e}` has a decimal exponent");
    let sign = if exp < 0 { '-' } else { '+' };
    format!("{mantissa}E{sign}{:03}", exp.unsigned_abs())
}

/// `g`: the default text when `precision` is omitted, otherwise rounded to `precision`
/// significant digits and laid out by the same rule (fixed or exponent; no trailing zeros).
fn general(v: f64, precision: Option<i64>, single: bool) -> String {
    match precision {
        _ if !v.is_finite() => numfmt::f64_to_str(v),
        None if single => numfmt::f32_to_str(v as f32),
        None => numfmt::f64_to_str(v),
        Some(p) => {
            // The exact decimal value of a `Double` has at most 767 significant digits, so a
            // larger precision gives the same text (its trailing zeros are dropped)
            let p = p.clamp(1, G_MAX_DIGITS);
            let sci = format!("{v:.*e}", (p - 1) as usize);
            let (mantissa, exp) = sci.split_once('e').expect("`{:e}` has an exponent");
            let mantissa = if mantissa.contains('.') {
                mantissa.trim_end_matches('0').trim_end_matches('.')
            } else {
                mantissa
            };
            numfmt::layout_sci(&format!("{mantissa}e{exp}"))
        }
    }
}
