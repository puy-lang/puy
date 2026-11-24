//! The Pascal sources of the standard library units (`docs/spec/collections.md` 1.2).
//!
//! The sources are embedded with `include_str!`; this crate only maps a unit name to its source
//! and does no I/O, so it builds for `wasm32-wasip2` and `wasm32-unknown-unknown` like `puy-core`.
//!
//! A large unit is kept in one file per type: its own file names the others with `{$I}` lines,
//! which the build (`build.rs`) replaces with the pieces of the files that make up the source.
//! The table of units is made by the build from the files under `units/`. The files of a type
//! (a part) are read only when the program uses a name they declare, which the table lists.

use std::borrow::Cow;

/// A unit of the standard library written in Pascal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    /// The unit name as written in its heading (`System.Generics.Collections`).
    pub name: &'static str,
    /// The files of the unit: its own file first, then the files it includes in the order of
    /// the `{$I}` lines.
    pub files: &'static [File],
    /// The pieces of the files that make up the source, in order. Empty when the source is the
    /// whole of the first file (a unit that includes no file).
    pub pieces: &'static [Piece],
    /// The parts of the unit: the included files grouped by the name before their section
    /// (`System.SysUtils/TEncoding`), each read only when the program uses a name it declares
    /// (`docs/spec/collections.md` 1.2).
    pub parts: &'static [Part],
}

/// A part of a unit: the included files that share a name before the section
/// (`System.SysUtils/TEncoding.Interface.inc`, `….Implementation.inc`, `….Finalization.inc`),
/// read or left out together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// The path of its files without the section and the extension (`System.SysUtils/TEncoding`).
    pub name: &'static str,
    /// The names declared at the top level of its files, lowercase: a file that writes one of
    /// them reads the part.
    pub names: &'static [&'static str],
    /// The names of the other parts (of any unit) that its files write, lowercase: reading the
    /// part reads those too.
    pub refs: &'static [&'static str],
}

/// The members the part of `System` written in Pascal declares in a built-in helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperMembers {
    /// The name of the built-in helper (`TStringHelper`).
    pub helper: &'static str,
    /// The part of `System` that declares the helper (an index into its `parts`).
    pub part: u32,
    /// Its methods, properties and constants, in the order of their declarations.
    pub members: &'static [&'static str],
}

/// A file of a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct File {
    /// The path relative to the directory of the units, with `/` between the parts
    /// (`System.Generics.Collections/List.Interface.inc`).
    pub path: &'static str,
    /// The text of the file (UTF-8).
    pub text: &'static str,
}

/// A piece of the source of a unit: the bytes `start..end` of `files[file]`, of the part
/// `parts[part]` ([`NO_PART`] for a piece of the unit's own file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub file: u32,
    pub start: u32,
    pub end: u32,
    pub part: u32,
}

/// The [`Piece::part`] of a piece of the unit's own file, which is always read.
pub const NO_PART: u32 = u32::MAX;

impl Unit {
    /// The source of the unit with all its parts: the pieces joined.
    pub fn source(&self) -> Cow<'static, str> {
        self.source_with(|_| true)
    }

    /// The source of the unit with the parts for which `read` is true: the pieces of the unit's
    /// own file and of those parts joined.
    pub fn source_with(&self, read: impl Fn(u32) -> bool) -> Cow<'static, str> {
        if self.pieces.is_empty() {
            return Cow::Borrowed(self.files[0].text);
        }
        Cow::Owned(self.pieces_with(read).map(|p| self.text(p)).collect())
    }

    /// The pieces of the unit's own file and of the parts for which `read` is true, in order.
    pub fn pieces_with(&self, read: impl Fn(u32) -> bool) -> impl Iterator<Item = &'static Piece> {
        self.pieces
            .iter()
            .filter(move |p| p.part == NO_PART || read(p.part))
    }

    /// The text of a piece.
    pub fn text(&self, p: &Piece) -> &'static str {
        &self.files[p.file as usize].text[p.start as usize..p.end as usize]
    }
}

/// A name that a unit of Delphi has and the same unit of Puy does not provide (requirements
/// §8.6, `docs/spec/stdlib/README.md`): using it is E3022, which names the unit and says what to
/// use instead. Each unit lists its names in `units/<Name>.unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unavailable {
    /// The unit name (`System.Contnrs`).
    pub unit: &'static str,
    /// The name as Delphi writes it (`TList`).
    pub name: &'static str,
    /// What to use instead, in the Markdown of a note (`` `TList<T>` or `TObjectList` ``).
    pub instead: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/units.rs"));

/// Whether the part of `System` written in Pascal declares the member `member` (of any
/// visibility) in the helper `helper` (both compared ignoring ASCII case). The compiler reads the
/// files of that helper only when a file asks for such a member
/// (`docs/spec/stdlib/System/README.md`).
pub fn system_helper_declares(helper: &str, member: &str) -> bool {
    system_helper_members(helper).any(|m| m.eq_ignore_ascii_case(member))
}

/// The members the part of `System` written in Pascal declares in the helper `helper`.
pub fn system_helper_members(helper: &str) -> impl Iterator<Item = &'static str> {
    SYSTEM_HELPER_MEMBERS
        .iter()
        .filter(move |h| h.helper.eq_ignore_ascii_case(helper))
        .flat_map(|h| h.members.iter().copied())
}

/// The part of the unit `System` that declares the members of the helper `helper` (compared
/// ignoring ASCII case).
pub fn system_helper_part(helper: &str) -> Option<u32> {
    SYSTEM_HELPER_MEMBERS
        .iter()
        .find(|h| h.helper.eq_ignore_ascii_case(helper))
        .map(|h| h.part)
}

/// The part of the unit `System` that exports `name` (lowercase), visible in every file
/// without a `uses` (`docs/spec/stdlib/System/README.md`).
pub fn system_export_part(name: &str) -> Option<u32> {
    SYSTEM_EXPORTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, part)| part)
}

/// The part of the unit `System` that declares the type `name` (lowercase), which every file
/// sees without a `uses` (`TInstant`; `docs/spec/stdlib/System/README.md`).
pub fn system_type_part(name: &str) -> Option<u32> {
    SYSTEM_TYPES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, part)| part)
}

/// The part of the unit `System` that a use of `name` (lowercase; a built-in helper or a name in
/// [`SYSTEM_EXPORTS`]) reads.
pub fn system_part(name: &str) -> Option<u32> {
    system_helper_part(name).or_else(|| system_export_part(name))
}

/// The unit of that name (compared ignoring ASCII case, as unit names are).
pub fn find(name: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.name.eq_ignore_ascii_case(name))
}

/// The names not provided that are written `name` (compared ignoring ASCII case, as identifiers
/// are), in the order of the units.
pub fn unavailable(name: &str) -> impl Iterator<Item = &'static Unavailable> + '_ {
    UNAVAILABLE
        .iter()
        .filter(move |u| u.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each unit's heading names the unit, so the table and the sources cannot drift apart.
    #[test]
    fn headings_match_names() {
        for u in UNITS {
            let heading = format!("unit {};", u.name);
            assert!(u.source().contains(&heading), "{}", u.name);
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

    // A unit split into included files is joined in place of its `{$I}` lines: no `{$I}` is
    // left, every line of an included file is in the source, and the pieces follow one another.
    #[test]
    fn included_files_are_joined_in_place() {
        let u = find("System.Generics.Collections").expect("the unit");
        assert!(u.files.len() > 1, "the unit is split");
        let src = u.source();
        assert!(!src.contains("{$I "), "a {{$I}} is left");
        for f in &u.files[1..] {
            assert!(src.contains(f.text), "{}", f.path);
        }
        assert!(src.starts_with("unit System.Generics.Collections;"));
        assert!(src.trim_end().ends_with("end."));
    }

    // A name not provided is found ignoring case, with its units in their order (requirements
    // §8.6); a member is listed with its class.
    #[test]
    fn unavailable_ignores_case() {
        let found: Vec<&str> = unavailable("tlist").map(|u| u.unit).collect();
        assert_eq!(found, ["System.Classes", "System.Contnrs"]);
        assert_eq!(unavailable("TObjectList").count(), 0);
        let found: Vec<&str> = unavailable("tmemorystream.MEMORY")
            .map(|u| u.unit)
            .collect();
        assert_eq!(found, ["System.Classes"]);
        assert_eq!(unavailable("Memory").count(), 0);
    }

    // Each unit's list of names not provided is the table in its specification, so that the
    // compiler and the documentation cannot drift apart (`docs/spec/stdlib/README.md`). Skipped
    // outside the repository (the published crate has no `docs/`).
    #[test]
    fn unavailable_names_are_in_the_spec() {
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/stdlib");
        if !docs.is_dir() {
            return;
        }
        for u in UNAVAILABLE {
            let path = docs.join(u.unit).join("README.md");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let row = format!("| `{}` | {} |", u.name, u.instead);
            assert!(
                text.lines().any(|l| l.trim() == row),
                "{}: no row {row:?}",
                path.display()
            );
        }
    }
}
