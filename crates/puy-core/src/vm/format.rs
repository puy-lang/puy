//! `Format` (`docs/spec/semantics.md` 5.3): reads the format and lays out the arguments of the
//! `array of const`.

use std::fmt::Write as _;

use super::heap::{OutOfMemory, TextBuf};
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
    /// The result would exceed the string length limit, or its buffer could not be allocated
    /// (`EOutOfMemory`).
    OutOfMemory,
}

impl From<OutOfMemory> for FormatError {
    fn from(_: OutOfMemory) -> Self {
        FormatError::OutOfMemory
    }
}

impl From<std::fmt::Error> for FormatError {
    /// A number written to a [`TextBuf`] fails only when the buffer is refused or the text is
    /// over the length limit.
    fn from(_: std::fmt::Error) -> Self {
        FormatError::OutOfMemory
    }
}

/// The default precision of `f` and `n`.
const FIXED_DEFAULT: i64 = 2;

/// The largest precision that can give a string within the limit (the integer part of a `Double`
/// has at most 309 digits; `numfmt::f64_fixed` uses the same margin).
const MAX_PRECISION: i64 = MAX_STRING_LEN as i64 - 400;

/// The precision `g` rounds to at most (more than the significant digits of any `Double`).
const G_MAX_DIGITS: i64 = 800;

/// Formats `args` by `fmt`. `room` is the most code units the result can have (`Heap::str_room`):
/// widths and precisions over it fail with `OutOfMemory` before the text is built.
pub fn format(fmt: &[u16], args: &[FmtArg], room: usize) -> Result<Vec<u16>, FormatError> {
    let mut out = TextBuf::default();
    let mut i = 0;
    let mut next = 0usize;
    while i < fmt.len() {
        let u = fmt[i];
        i += 1;
        if u != u16::from(b'%') {
            out.push_units(&[u])?;
            continue;
        }
        if fmt.get(i) == Some(&u16::from(b'%')) {
            out.push_units(&[u])?;
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
            return Err(FormatError::OutOfMemory);
        }
        let pad = (spec.width as usize).saturating_sub(text.len());
        if out.len() + pad + text.len() > room {
            return Err(FormatError::OutOfMemory);
        }
        if !spec.left {
            out.fill(u16::from(b' '), pad)?;
        }
        out.push_units(&text)?;
        if spec.left {
            out.fill(u16::from(b' '), pad)?;
        }
    }
    Ok(out.finish()?)
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
    // `d`, `u`, `x`, `e` and `f` / `n` of a finite value have at least `precision` digits: a
    // precision over the room fails first, otherwise the whole text is asked for before it is
    // written (the digits and what can come with them, so the buffer never grows)
    let digits = match (kind, arg) {
        ('d' | 'u' | 'x', FmtArg::Int { .. }) | ('e' | 'f' | 'n', FmtArg::Curr(_)) => true,
        ('e' | 'f' | 'n', FmtArg::Double(v)) => v.is_finite(),
        ('e' | 'f' | 'n', FmtArg::Single(v)) => v.is_finite(),
        _ => false,
    };
    let least = if digits {
        check_precision(precision, room)?;
        precision.map_or(0, |p| p.max(0) as usize)
    } else {
        0
    };
    let room_needed = if least > 0 {
        least.saturating_add(numfmt::DIGITS_EXTRA)
    } else {
        0
    };
    let mut t = TextBuf::with_room(room_needed)?;
    match (kind, arg) {
        ('d', FmtArg::Int { value, .. }) => {
            min_digits(&mut t, *value, &value.unsigned_abs().to_string(), precision)?;
        }
        ('u', FmtArg::Int { value, bits }) => {
            min_digits(&mut t, 0, &unsigned(*value, *bits).to_string(), precision)?;
        }
        ('x', FmtArg::Int { value, bits }) => {
            let digits = format!("{:X}", unsigned(*value, *bits));
            min_digits(&mut t, 0, &digits, precision)?;
        }
        ('s', FmtArg::Str(s)) => t.push_units(truncated(s, precision))?,
        ('s', FmtArg::Char(c)) => t.push_units(truncated(&[*c], precision))?,
        ('f' | 'n', FmtArg::Curr(c)) => {
            numfmt::curr_fixed(&mut t, *c, precision.unwrap_or(FIXED_DEFAULT))?;
            if kind == 'n' {
                grouped(t.units_mut())?;
            }
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
                    numfmt::f64_fixed(&mut t, v, precision.unwrap_or(FIXED_DEFAULT))?;
                    if kind == 'n' && v.is_finite() {
                        grouped(t.units_mut())?;
                    }
                }
                'e' => {
                    check_precision(precision, room)?;
                    exponential(&mut t, v, precision, matches!(arg, FmtArg::Single(_)))?;
                }
                _ => write!(
                    t,
                    "{}",
                    general(v, precision, matches!(arg, FmtArg::Single(_)))
                )?,
            }
        }
        _ => return Err(FormatError::Invalid),
    }
    Ok(t.finish()?)
}

/// `EOutOfMemory` for a precision that adds more digits than a string can hold or than `room`
/// allows (`d`, `u`, `x` and `e`, and `f` / `n` of a finite value, give at least `precision`
/// digits; the precision of `s` and `g` only shortens).
fn check_precision(precision: Option<i64>, room: usize) -> Result<(), FormatError> {
    match precision {
        Some(p) if p > MAX_PRECISION || p as u64 > room as u64 => Err(FormatError::OutOfMemory),
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
fn min_digits(
    t: &mut TextBuf,
    value: i128,
    digits: &str,
    precision: Option<i64>,
) -> Result<(), FormatError> {
    if value < 0 {
        t.write_char('-')?;
    }
    let p = precision.unwrap_or(0).max(0) as usize;
    numfmt::write_zeros(t, p.saturating_sub(digits.len()))?;
    t.write_str(digits)?;
    Ok(())
}

/// A string cut to at most `precision` code units.
fn truncated(s: &[u16], precision: Option<i64>) -> &[u16] {
    match precision {
        Some(p) if (p.max(0) as usize) < s.len() => &s[..p.max(0) as usize],
        _ => s,
    }
}

/// `n`: puts `,` between the groups of three digits of the integer part of the fixed text
/// `units` (in place: the fraction, which may be millions of digits, moves once).
fn grouped(units: &mut Vec<u16>) -> Result<(), OutOfMemory> {
    let start = usize::from(units.first() == Some(&u16::from(b'-')));
    let int_end = units
        .iter()
        .position(|u| *u == u16::from(b'.'))
        .unwrap_or(units.len());
    let commas = (int_end - start).saturating_sub(1) / 3;
    if commas == 0 {
        return Ok(());
    }
    let len = units.len();
    super::heap::try_reserve(units, commas)?;
    units.resize(len + commas, 0);
    units.copy_within(int_end..len, int_end + commas);
    // The integer part from its last digit, each written `commas` places further right until
    // the commas run out
    let mut dst = int_end + commas;
    for (k, src) in (start..int_end).rev().enumerate() {
        if k > 0 && k % 3 == 0 {
            dst -= 1;
            units[dst] = u16::from(b',');
        }
        dst -= 1;
        units[dst] = units[src];
    }
    Ok(())
}

/// `e`: `d.ddd…E+ddd` with `precision` significant digits (the shortest digits that read back
/// when omitted; a `Single` as binary32). INF and NaN use the default text.
fn exponential(
    t: &mut TextBuf,
    v: f64,
    precision: Option<i64>,
    single: bool,
) -> Result<(), FormatError> {
    if !v.is_finite() {
        t.write_str(&numfmt::f64_to_str(v))?;
        return Ok(());
    }
    // Rust's formatting takes at most 65535 digits; the significant digits of a `Double` past
    // its exact value are zeros, written before the exponent
    let digits = precision.map(|p| p.max(1) - 1);
    let exact = digits.map(|d| d.min(numfmt::EXACT_DIGITS));
    match exact {
        Some(d) => write!(t, "{v:.*e}", d as usize)?,
        None if single => write!(t, "{:e}", v as f32)?,
        None => write!(t, "{v:e}")?,
    }
    // Rust's exponent (`e-7`) ends the text; rewrite it as `E-007`
    let units = t.units_mut();
    let e = units
        .iter()
        .rposition(|u| *u == u16::from(b'e'))
        .expect("`{:e}` has an exponent");
    let exp: i32 = String::from_utf16_lossy(&units[e + 1..])
        .parse()
        .expect("`{:e}` has a decimal exponent");
    units.truncate(e);
    if let (Some(d), Some(x)) = (digits, exact) {
        numfmt::write_zeros(t, (d - x) as usize)?;
    }
    let sign = if exp < 0 { '-' } else { '+' };
    write!(t, "E{sign}{:03}", exp.unsigned_abs())?;
    Ok(())
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
