//! Pascal names: the conversion from Rust's names, and the checks of the generated names.

use proc_macro2::Span;

use crate::reserved::RESERVED_WORDS;

/// `snake_case` (or `SCREAMING_SNAKE_CASE`) as `PascalCase` (requirements §6): each part
/// between `_` starts with a capital; a part written all in capitals is lowered after its first
/// letter (`MAX_COUNT` → `MaxCount`), any other keeps its letters (`utf8_len` → `Utf8Len`).
pub(crate) fn pascal_case(rust: &str) -> String {
    let rust = rust.strip_prefix("r#").unwrap_or(rust);
    let mut out = String::new();
    for part in rust.split('_').filter(|p| !p.is_empty()) {
        let upper = !part.chars().any(char::is_lowercase);
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            if upper {
                out.extend(chars.flat_map(char::to_lowercase));
            } else {
                out.extend(chars);
            }
        }
    }
    out
}

/// Checks that `name` can be a Pascal identifier the unit declares: not empty, of letters,
/// digits and `_` not starting with a digit, and not a reserved word.
pub(crate) fn check_ident(name: &str, span: Span) -> syn::Result<()> {
    let mut chars = name.chars();
    let ok = chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric());
    if !ok {
        return Err(syn::Error::new(
            span,
            format!("`{name}` is not a Pascal identifier (ASCII letters, digits and `_`)"),
        ));
    }
    let lower = name.to_ascii_lowercase();
    if RESERVED_WORDS.contains(&lower.as_str()) {
        return Err(syn::Error::new(
            span,
            format!("`{name}` is a reserved word of Pascal; give another name with `name = \"…\"`"),
        ));
    }
    Ok(())
}

/// Checks that a unit's name is identifiers joined by `.` (`Puy.Zip`).
pub(crate) fn check_unit_name(name: &str, span: Span) -> syn::Result<()> {
    for part in name.split('.') {
        check_ident(part, span)?;
    }
    Ok(())
}

/// Names declared in one scope, compared ignoring ASCII case as Pascal compares them.
#[derive(Default)]
pub(crate) struct Scope {
    names: Vec<String>,
}

impl Scope {
    /// Declares `name`: an error when the scope already has it.
    pub(crate) fn declare(&mut self, name: &str, what: &str, span: Span) -> syn::Result<()> {
        if self.names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            return Err(syn::Error::new(
                span,
                format!("{what} `{name}` is declared twice (Pascal names ignore case)"),
            ));
        }
        self.names.push(name.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // snake_case and SCREAMING_SNAKE_CASE become PascalCase; digits and raw names survive.
    #[test]
    fn converts_to_pascal_case() {
        for (rust, pascal) in [
            ("read_all", "ReadAll"),
            ("utf8_len", "Utf8Len"),
            ("add_int64", "AddInt64"),
            ("MAX_COUNT", "MaxCount"),
            ("_x", "X"),
            ("r#type", "Type"),
            ("x", "X"),
        ] {
            assert_eq!(pascal_case(rust), pascal, "{rust}");
        }
    }

    // A reserved word in any case is refused, and so is a name that is not an identifier.
    #[test]
    fn refuses_reserved_words() {
        let span = Span::call_site();
        assert!(check_ident("Begin", span).is_err());
        assert!(check_ident("STRING", span).is_err());
        assert!(check_ident("Value", span).is_ok());
        assert!(check_ident("1x", span).is_err());
        assert!(check_ident("Ünit", span).is_err());
        assert!(check_unit_name("Puy.Zip", span).is_ok());
        assert!(check_unit_name("Puy.Unit", span).is_err());
    }

    // A second name that differs only in case is an error.
    #[test]
    fn scope_ignores_case() {
        let span = Span::call_site();
        let mut s = Scope::default();
        assert!(s.declare("Add", "routine", span).is_ok());
        assert!(s.declare("ADD", "routine", span).is_err());
    }
}
