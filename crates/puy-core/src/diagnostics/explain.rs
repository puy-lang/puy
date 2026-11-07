//! Explanations for `puy explain` (`docs/spec/diagnostics.md` chapter 1). Embeds `docs/explain/<code>.md` at build time.

use super::Code;

include!(concat!(env!("OUT_DIR"), "/explain_table.rs"));

/// The explanation for the code (Markdown body).
pub fn lookup(code: Code) -> Option<&'static str> {
    let key = code.to_string();
    EXPLANATIONS
        .binary_search_by(|(c, _)| c.cmp(&key.as_str()))
        .ok()
        .map(|i| EXPLANATIONS[i].1)
}

/// Whether an explanation exists.
pub fn has(code: Code) -> bool {
    lookup(code).is_some()
}

/// List of codes that have explanations (ascending).
pub fn all() -> impl Iterator<Item = (Code, &'static str)> {
    EXPLANATIONS
        .iter()
        .filter_map(|(c, text)| Code::parse(c).map(|code| (code, *text)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::codes;

    /// Explanation file names are registry codes (not reserved ones), and the first line starts with `# <code>: `.
    #[test]
    fn explanations_match_registry() {
        for (name, text) in EXPLANATIONS {
            let code =
                Code::parse(name).unwrap_or_else(|| panic!("docs/explain/{name}.md: not a code"));
            let info = codes::lookup(code)
                .unwrap_or_else(|| panic!("docs/explain/{name}.md: {name} is not registered"));
            assert!(
                info.milestone.is_some(),
                "docs/explain/{name}.md: {name} is reserved"
            );
            let first = text.lines().next().unwrap_or("");
            assert!(
                first.starts_with(&format!("# {name}: ")),
                "docs/explain/{name}.md: the first line must be `# {name}: <summary>`, found `{first}`"
            );
            assert!(
                text.ends_with('\n'),
                "docs/explain/{name}.md: must end with a newline"
            );
        }
    }
}
