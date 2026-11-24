//! The Unicode properties of a code point (`CharClass`, `UnicodeCategory`, `CodePointClass`,
//! `NumericValue`; `docs/spec/bytecode.md` 2.22). The general categories and the numeric values
//! come from the tables of `unicode_tables.rs`; Alphabetic, Uppercase, Lowercase and White_Space
//! from Rust's standard library, whose data is of the same Unicode version (a unit test checks it).

use super::unicode_tables::{CATEGORY_RUNS, NUMERIC_FIRST, NUMERIC_RUNS};

/// The ordinals of some general categories in `TUnicodeCategory`
/// (`docs/spec/stdlib/System.Character/README.md`).
const CAT_CC: u32 = 0;
const CAT_CN: u32 = 2;
const CAT_ND: u32 = 13;
const CAT_NL: u32 = 14;
const CAT_NO: u32 = 15;

/// The largest code point.
const MAX_CODE_POINT: i64 = 0x10_FFFF;

/// The index of the run of `runs` (words of a code point shifted left by 5 and 5 bits of data)
/// that holds the code point `cp`: the last run whose code point is not above `cp`, if any.
fn run_of(runs: &[u32], cp: u32) -> Option<usize> {
    let key = cp << 5 | 0x1F;
    runs.partition_point(|&w| w <= key).checked_sub(1)
}

/// The general category of the code point `cp` as its ordinal in `TUnicodeCategory`
/// (`UnicodeCategory`, `I101`). A number outside `0..=$10FFFF` is unassigned (`Cn`).
pub(super) fn unicode_category(cp: i64) -> u32 {
    let Ok(cp) = u32::try_from(cp) else {
        return CAT_CN;
    };
    if i64::from(cp) > MAX_CODE_POINT {
        return CAT_CN;
    }
    let i = run_of(&CATEGORY_RUNS, cp).expect("the first run starts at U+0000");
    CATEGORY_RUNS[i] & 0x1F
}

/// The Unicode properties of the code point `cp` as a set of bits (`CodePointClass`, `I102`;
/// `CharClass`, `I105`, for a UTF-16 code unit): 1 Alphabetic, 2 Uppercase, 4 Lowercase,
/// 8 White_Space, 16 a control character (`Cc`), 32 a number (`Nd`, `Nl`, `No`), 64 a decimal
/// digit (`Nd`). A surrogate and a number outside `0..=$10FFFF` have none.
pub(super) fn code_point_class(cp: i64) -> i64 {
    let Some(ch) = u32::try_from(cp).ok().and_then(char::from_u32) else {
        return 0;
    };
    let cat = unicode_category(cp);
    let props = [
        ch.is_alphabetic(),
        ch.is_uppercase(),
        ch.is_lowercase(),
        ch.is_whitespace(),
        cat == CAT_CC,
        matches!(cat, CAT_ND | CAT_NL | CAT_NO),
        cat == CAT_ND,
    ];
    props
        .iter()
        .enumerate()
        .fold(0, |bits, (i, &p)| if p { bits | 1 << i } else { bits })
}

/// The numeric value of the code point `cp` (field 8 of `UnicodeData.txt`; a fraction as the
/// nearest `Double`), or -1 if it has none (`NumericValue`, `I103`).
pub(super) fn numeric_value(cp: i64) -> f64 {
    let Ok(cp) = u32::try_from(cp) else {
        return -1.0;
    };
    if i64::from(cp) > MAX_CODE_POINT {
        return -1.0;
    }
    let Some(i) = run_of(&NUMERIC_RUNS, cp) else {
        return -1.0;
    };
    let (start, last) = (NUMERIC_RUNS[i] >> 5, NUMERIC_RUNS[i] & 0x1F);
    if cp - start > last {
        return -1.0;
    }
    NUMERIC_FIRST[i] + f64::from(cp - start)
}

#[cfg(test)]
mod tests {
    use super::super::unicode_tables::UNICODE_VERSION;
    use super::*;

    // The tables and the properties of Rust's standard library must be of the same Unicode
    // version, or `IsLetter` and `GetUnicodeCategory` could disagree on a character.
    #[test]
    fn tables_are_of_the_unicode_version_of_the_standard_library() {
        assert_eq!(char::UNICODE_VERSION, UNICODE_VERSION);
    }

    // The bits of the general category agree with the standard library's `is_control` and
    // `is_numeric` on every code point, so `CharClass` keeps its meaning with the new table.
    #[test]
    fn category_bits_agree_with_the_standard_library() {
        for cp in 0..=0x10_FFFFu32 {
            let Some(ch) = char::from_u32(cp) else {
                continue;
            };
            let bits = code_point_class(i64::from(cp));
            assert_eq!(bits & 16 != 0, ch.is_control(), "{cp:X}");
            assert_eq!(bits & 32 != 0, ch.is_numeric(), "{cp:X}");
        }
    }

    // The ends of the code space and the runs: the first and last code points, a surrogate,
    // and numbers outside the code space.
    #[test]
    fn categories_at_the_ends() {
        for (cp, cat) in [
            (0x0000, 0),        // Cc
            (0x0041, 9),        // Lu
            (0x00BD, 15),       // No
            (0xD800, 4),        // Cs
            (0xDFFF, 4),        // Cs
            (0xE000, 3),        // Co
            (0x1_0000, 7),      // Lo
            (0x1_F600, 26),     // So
            (0x10_FFFD, 3),     // Co
            (0x10_FFFF, 2),     // Cn
            (0x11_0000, 2),     // outside
            (-1, 2),            // outside
            (0x1_0000_0041, 2), // outside, though its low bits are `A`
        ] {
            assert_eq!(unicode_category(cp), cat, "{cp:X}");
        }
    }

    // Values of the numeric table: the start, middle and end of runs, a fraction, a negative
    // fraction, the largest value and code points next to a run.
    #[test]
    fn numeric_values() {
        for (cp, v) in [
            (0x0030, 0.0),
            (0x0039, 9.0),
            (0x003A, -1.0),
            (0x002F, -1.0),
            (0x00BD, 0.5),
            (0x0F33, -0.5),
            (0x2163, 4.0),
            (0xFF17, 7.0),
            (0x1_D7CE, 0.0),
            (0x1_6B61, 1e12),
            (0x5146, -1.0), // a numeric value of Unihan only
            (0x0041, -1.0),
            (0x11_0030, -1.0),
            (-1, -1.0),
        ] {
            assert_eq!(numeric_value(cp), v, "{cp:X}");
        }
    }
}
