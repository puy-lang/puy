//! The Pascal sources of the standard library units (`docs/spec/collections.md` 1.2).
//!
//! The sources are embedded with `include_str!`; this crate only maps a unit name to its source
//! and does no I/O, so it builds for `wasm32-wasip2` and `wasm32-unknown-unknown` like `puy-core`.

/// A unit of the standard library written in Pascal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    /// The unit name as written in its heading (`System.Generics.Collections`).
    pub name: &'static str,
    /// The source of the unit (UTF-8).
    pub source: &'static str,
}

/// The units, in the order of their names.
pub const UNITS: &[Unit] = &[
    Unit {
        name: "System.Generics.Collections",
        source: include_str!("../units/System.Generics.Collections.pas"),
    },
    Unit {
        name: "System.Generics.Defaults",
        source: include_str!("../units/System.Generics.Defaults.pas"),
    },
    Unit {
        name: "System.SysUtils",
        source: include_str!("../units/System.SysUtils.pas"),
    },
];

/// The unit of that name (compared ignoring ASCII case, as unit names are).
pub fn find(name: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each unit's heading names the unit, so the table and the sources cannot drift apart.
    #[test]
    fn headings_match_names() {
        for u in UNITS {
            let heading = format!("unit {};", u.name);
            assert!(u.source.contains(&heading), "{}", u.name);
        }
    }

    // Lookup ignores ASCII case, as `uses` does (`docs/spec/units.md` 2.2).
    #[test]
    fn find_ignores_case() {
        assert_eq!(
            find("system.generics.collections").map(|u| u.name),
            Some("System.Generics.Collections")
        );
        assert_eq!(find("System.Generics"), None);
    }
}
