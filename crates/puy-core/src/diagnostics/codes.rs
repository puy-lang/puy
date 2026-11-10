//! Registry of error codes (`docs/spec/diagnostics.md` chapters 4–5).
//!
//! Holds the same list as the tables in the spec and checks it against them in tests. Message templates are held by
//! the code that creates the diagnostics in each stage (`lexer`, `parser`, `sema`, etc.).

use super::Code;

/// One registry entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeInfo {
    pub code: Code,
    /// The milestone where it is first issued. `None` means reserved.
    pub milestone: Option<u8>,
    /// Name of the warning (only for `W`).
    pub name: Option<&'static str>,
}

const fn e(n: u16, m: u8) -> CodeInfo {
    CodeInfo {
        code: Code::E(n),
        milestone: Some(m),
        name: None,
    }
}

const fn reserved(n: u16) -> CodeInfo {
    CodeInfo {
        code: Code::E(n),
        milestone: None,
        name: None,
    }
}

const fn w(n: u16, m: u8, name: &'static str) -> CodeInfo {
    CodeInfo {
        code: Code::W(n),
        milestone: Some(m),
        name: Some(name),
    }
}

const fn f(n: u16) -> CodeInfo {
    CodeInfo {
        code: Code::F(n),
        milestone: Some(1),
        name: None,
    }
}

/// The registry.
pub const CODES: &[CodeInfo] = &[
    // 4.1 Lexical
    e(1, 1),
    e(2, 1),
    e(3, 1),
    e(4, 1),
    e(5, 1),
    e(6, 1),
    e(7, 1),
    e(8, 1),
    e(9, 1),
    // 4.2 Directives
    e(200, 1),
    e(201, 1),
    e(202, 1),
    e(203, 1),
    e(204, 1),
    e(205, 1),
    e(206, 1),
    e(207, 1),
    e(208, 1),
    e(209, 1),
    e(210, 1),
    e(211, 1),
    e(212, 1),
    e(213, 1),
    e(214, 1),
    e(215, 1),
    e(216, 1),
    // 4.3 Syntax
    e(400, 1),
    e(401, 1),
    e(402, 1),
    e(403, 1),
    e(404, 1),
    e(405, 1),
    e(406, 1),
    e(407, 1),
    e(408, 2),
    e(409, 1),
    e(410, 1),
    e(411, 1),
    reserved(412),
    e(413, 1),
    e(414, 1),
    e(415, 2),
    e(416, 2),
    e(417, 2),
    // 4.4 Semantic analysis
    e(700, 1),
    e(701, 1),
    e(702, 1),
    e(703, 1),
    e(704, 1),
    e(705, 1),
    e(706, 1),
    e(707, 1),
    e(708, 1),
    e(709, 2),
    e(710, 1),
    e(711, 1),
    e(712, 1),
    e(713, 1),
    e(714, 1),
    e(715, 1),
    e(716, 1),
    e(717, 1),
    e(718, 1),
    e(719, 1),
    e(720, 1),
    e(721, 1),
    e(722, 1),
    e(723, 1),
    e(724, 1),
    e(725, 1),
    e(726, 1),
    e(727, 2),
    e(728, 2),
    e(729, 2),
    e(730, 2),
    e(731, 2),
    e(732, 2),
    e(733, 2),
    e(734, 2),
    e(735, 2),
    e(736, 2),
    e(737, 2),
    e(738, 2),
    e(739, 2),
    e(740, 2),
    // 4.5 Control flow
    e(2000, 1),
    e(2001, 1),
    e(2002, 1),
    reserved(2003),
    e(2004, 1),
    // 4.6 Units and packages
    e(2500, 1),
    e(2501, 1),
    e(2502, 2),
    e(2503, 2),
    e(2504, 2),
    e(2505, 2),
    e(2506, 2),
    e(2507, 1),
    // 4.7 Removed features
    e(3000, 1),
    e(3001, 1),
    e(3002, 1),
    e(3003, 1),
    e(3004, 1),
    e(3005, 1),
    e(3006, 1),
    e(3007, 1),
    e(3008, 1),
    e(3009, 3),
    e(3010, 1),
    e(3011, 1),
    e(3012, 1),
    e(3013, 1),
    e(3014, 3),
    e(3015, 1),
    e(3016, 1),
    e(3017, 3),
    e(3018, 3),
    e(3019, 3),
    e(3020, 3),
    e(3021, 3),
    // 4.8 Unsupported features and code generation limits
    e(3200, 1),
    e(3400, 1),
    // Chapter 5 Warnings
    w(1, 1, "unused-variable"),
    w(2, 2, "unused-value"),
    w(3, 1, "unreachable-code"),
    w(4, 1, "ignored-directive"),
    w(5, 1, "directive-placement"),
    w(6, 1, "uses-in-clause"),
    w(7, 1, "platform-symbol"),
    w(8, 1, "user-message"),
    w(9, 1, "ignored-calling-convention"),
    w(10, 1, "ignored-hint-directive"),
    w(11, 3, "deprecated"),
    w(12, 3, "experimental"),
    w(13, 5, "source-extension"),
    w(14, 1, "unknown-warning-name"),
    w(15, 1, "misplaced-metadata"),
    w(16, 2, "ignored-packed"),
    w(17, 2, "unused-parameter"),
    w(18, 1, "empty-statement"),
    w(19, 2, "unreachable-handler"),
    w(20, 2, "legacy-unit-init"),
    // 4.9 Fatal errors
    f(1),
    f(2),
    f(3),
    f(4),
    f(5),
    f(6),
];

/// Warnings disabled by default (chapter 5).
pub const WARNINGS_OFF_BY_DEFAULT: &[Code] = &[Code::W(17)];

pub fn lookup(code: Code) -> Option<&'static CodeInfo> {
    CODES.iter().find(|c| c.code == code)
}

pub fn warning_name(code: Code) -> Option<&'static str> {
    lookup(code).and_then(|c| c.name)
}

/// Looks up a code by warning name (case-insensitive).
pub fn warning_by_name(name: &str) -> Option<Code> {
    CODES
        .iter()
        .find(|c| c.name.is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .map(|c| c.code)
}

/// Mapping from Delphi warning names (chapter 5).
pub fn delphi_warning_name(name: &str) -> Option<&'static str> {
    let upper = name.to_ascii_uppercase();
    Some(match upper.as_str() {
        "SYMBOL_DEPRECATED" | "UNIT_DEPRECATED" => "deprecated",
        "SYMBOL_EXPERIMENTAL" | "UNIT_EXPERIMENTAL" => "experimental",
        "SYMBOL_PLATFORM" | "UNIT_PLATFORM" | "SYMBOL_LIBRARY" | "UNIT_LIBRARY" => {
            "ignored-hint-directive"
        }
        _ => return None,
    })
}

/// All registered warning names (used for similar-name suggestions).
pub fn warning_names() -> impl Iterator<Item = &'static str> {
    CODES.iter().filter_map(|c| c.name)
}

/// Label vocabulary (2.2).
pub const LABELS: &[&str] = &[
    "",
    "declared here",
    "declared again here",
    "read here",
    "returns here",
    "assigned by this loop",
    "found here",
    "block starts here",
    "starts here",
    "never closed",
    "text before the closing quotes",
    "shallower than the closing quotes",
    "a constant",
    "the loop variable",
    "a `const` parameter",
    "not a variable",
    "included from here",
    "first `uses` here",
    "project manifest here",
];

/// Similar names (2.1): measures the case-insensitive edit distance and picks the smallest among those within 1/3 of the
/// name's length (at most 2). `None` if two or more candidates tie at the same distance.
pub fn similar_name<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let lower = name.to_ascii_lowercase();
    let limit = (lower.chars().count() / 3).min(2);
    let mut best: Option<(usize, &'a str)> = None;
    let mut tie = false;
    for c in candidates {
        let d = levenshtein(&lower, &c.to_ascii_lowercase());
        if d > limit {
            continue;
        }
        match best {
            Some((bd, _)) if d > bd => {}
            Some((bd, _)) if d == bd => tie = true,
            _ => {
                best = Some((d, c));
                tie = false;
            }
        }
    }
    match best {
        Some((_, c)) if !tie => Some(c),
        _ => None,
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicates_and_sorted() {
        for pair in CODES.windows(2) {
            assert!(pair[0].code < pair[1].code, "{:?} {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn warning_names_unique() {
        let names: Vec<_> = warning_names().collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len());
        assert_eq!(warning_by_name("Unused-Variable"), Some(Code::W(1)));
        assert_eq!(warning_by_name("unused_variable"), None);
    }

    #[test]
    fn similar() {
        assert_eq!(
            similar_name("unused_variable", warning_names()),
            Some("unused-variable")
        );
        assert_eq!(similar_name("xyz", warning_names()), None);
        assert_eq!(
            similar_name("RANGECHECK", ["RANGECHECKS", "OVERFLOWCHECKS"].into_iter()),
            Some("RANGECHECKS")
        );
    }

    #[test]
    fn registry_matches_spec_tables() {
        // Check against the tables in the spec (docs/spec/diagnostics.md) in both directions.
        // The spec is read at run time because the public repository does not ship `docs/`;
        // there the check is skipped.
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/diagnostics.md");
        let Ok(spec) = std::fs::read_to_string(&path) else {
            return;
        };
        let mut in_table = Vec::new();
        for line in spec.lines() {
            if let Some(rest) = line.strip_prefix("| ")
                && let Some(head) = rest.get(..5)
                && let Some(code) = Code::parse(head)
                && rest.as_bytes().get(5) == Some(&b' ')
            {
                in_table.push(code);
            }
        }
        in_table.sort();
        in_table.dedup();
        let registry: Vec<Code> = CODES.iter().map(|c| c.code).collect();
        assert_eq!(registry, in_table);
    }
}
