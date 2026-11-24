//! Builds the table of units from `units/` (`docs/spec/collections.md` 1.2).
//!
//! Each `units/<Name>.pas` is a unit. A line of it that holds only `{$I <path>}` names a file
//! under `units/` whose text takes the place of the line, so that a large unit is kept in one
//! file per type and a change to one file does not move the lines of the others. The table
//! records the files and the pieces of them that make up the source of the unit, in order; the
//! compiler joins the pieces and maps a position in the joined source back to its file.
//!
//! The included files that share a name before their section (`TEncoding.Interface.inc`,
//! `TEncoding.Implementation.inc`) make a part, which the compiler reads only when the program
//! writes a name the part declares; the table lists those names and the names of other parts
//! each part writes.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// A piece of the source of a unit: the bytes `start..end` of a file, of the part `part` (an
/// included file) or of the unit's own file (`None`).
struct Piece {
    file: usize,
    start: usize,
    end: usize,
    part: Option<usize>,
}

/// The files of a unit's directory that share a name before the section
/// (`System.SysUtils/TEncoding.Interface.inc`, `….Implementation.inc`, `….Finalization.inc`):
/// read or left out together (`docs/spec/collections.md` 1.2).
struct Part {
    /// `System.SysUtils/TEncoding`.
    name: String,
    /// The indices of its files in the unit's files.
    files: Vec<usize>,
    /// The names declared at the top level of its files, lowercase.
    names: Vec<String>,
    /// The names of other parts (of any unit) written in its files, lowercase.
    refs: Vec<String>,
}

struct Unit {
    name: String,
    /// The files, relative to `units/` with `/` between the parts; the unit's own file first.
    files: Vec<String>,
    /// Empty when the unit includes no file (its source is the whole of its own file).
    pieces: Vec<Piece>,
    parts: Vec<Part>,
}

fn main() {
    let dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo")).join("units");
    println!("cargo:rerun-if-changed=units");
    let mut units: Vec<Unit> = Vec::new();
    let mut included: Vec<String> = Vec::new();
    for name in entries(&dir) {
        let path = dir.join(&name);
        if path.is_dir() || name.ends_with(UNAVAILABLE_EXT) {
            continue;
        }
        let Some(unit) = name.strip_suffix(".pas") else {
            panic!(
                "units/{name}: only units (`.pas`), their lists of names not provided \
                 (`{UNAVAILABLE_EXT}`) and directories belong in `units/`"
            );
        };
        let unit = read_unit(&dir, unit, &mut included);
        for part in &unit.parts {
            for &f in &part.files {
                let text = read(&dir, &unit.files[f]);
                assert!(
                    unit.name == "System" || helper_members(&text).is_empty(),
                    "units/{}: a helper outside `System` goes in the unit's own file: its members \
                     are used without its name, so the file would not be read",
                    unit.files[f]
                );
            }
        }
        units.push(unit);
    }
    // Every file under a directory of `units/` is included once
    let mut parts: Vec<String> = Vec::new();
    for name in entries(&dir) {
        if dir.join(&name).is_dir() {
            collect(&dir, &name, &mut parts);
        }
    }
    for p in &parts {
        match included.iter().filter(|i| *i == p).count() {
            1 => {}
            0 => panic!("units/{p} is not included by any unit"),
            _ => panic!("units/{p} is included more than once"),
        }
    }
    find_refs(&dir, &mut units);
    let mut out = String::from(
        "/// The units, in the order of their names.\npub const UNITS: &[Unit] = &[\n",
    );
    for u in &units {
        writeln!(
            out,
            "    Unit {{\n        name: {:?},\n        files: &[",
            u.name
        )
        .unwrap();
        for f in &u.files {
            let abs = dir.join(f);
            writeln!(
                out,
                "            File {{\n                path: {f:?},\n                text: include_str!({:?}),\n            }},",
                abs.to_str().expect("a path in UTF-8")
            )
            .unwrap();
        }
        writeln!(out, "        ],\n        pieces: &[").unwrap();
        for p in &u.pieces {
            let part = match p.part {
                Some(k) => k.to_string(),
                None => "NO_PART".to_string(),
            };
            writeln!(
                out,
                "            Piece {{ file: {}, start: {}, end: {}, part: {part} }},",
                p.file, p.start, p.end
            )
            .unwrap();
        }
        writeln!(out, "        ],\n        parts: &[").unwrap();
        for p in &u.parts {
            writeln!(
                out,
                "            Part {{\n                name: {:?},\n                names: &{:?},\n                refs: &{:?},\n            }},",
                p.name, p.names, p.refs
            )
            .unwrap();
        }
        writeln!(out, "        ],\n    }},").unwrap();
    }
    out.push_str("];\n");
    out.push_str(&unavailable_table(&dir, &units));
    out.push_str(
        "/// The members the part of `System` written in Pascal declares in each helper, with the\n\
         /// part (of the unit `System`) that declares the helper, in the order of the declarations.\n\
         pub const SYSTEM_HELPER_MEMBERS: &[HelperMembers] = &[\n",
    );
    if let Some(system) = units.iter().find(|u| u.name == "System") {
        for (k, part) in system.parts.iter().enumerate() {
            for &f in &part.files {
                for (helper, members) in helper_members(&read(&dir, &system.files[f])) {
                    writeln!(
                        out,
                        "    HelperMembers {{\n        helper: {helper:?},\n        part: {k},\n        members: &{members:?},\n    }},"
                    )
                    .unwrap();
                }
            }
        }
    }
    out.push_str("];\n");
    out.push_str(
        "/// The names the part of `System` written in Pascal declares in the `interface` section\n\
         /// besides the helpers (lowercase), with their part: visible in every file without a\n\
         /// `uses` (`docs/spec/stdlib/System/README.md`).\n\
         pub const SYSTEM_EXPORTS: &[(&str, u32)] = &[\n",
    );
    let mut system_types = String::from(
        "/// The non-helper types declared by the Pascal part of `System` (lowercase), with their part.\n\
         pub const SYSTEM_TYPES: &[(&str, u32)] = &[\n",
    );
    if let Some(system) = units.iter().find(|u| u.name == "System") {
        for (k, part) in system.parts.iter().enumerate() {
            for &f in &part.files {
                let path = &system.files[f];
                if part_of(path).1 != "Interface" {
                    continue;
                }
                let text = read(&dir, path);
                let helpers: Vec<String> = helper_members(&text)
                    .into_iter()
                    .map(|(h, _)| h.to_ascii_lowercase())
                    .collect();
                let names = top_level_names(&text, false);
                for n in &names.keys {
                    if !n.ends_with('<') && !helpers.contains(n) {
                        writeln!(out, "    ({n:?}, {k}),").unwrap();
                    }
                }
                for n in &names.non_generic_types {
                    if !helpers.contains(n) {
                        writeln!(system_types, "    ({n:?}, {k}),").unwrap();
                    }
                }
            }
        }
    }
    out.push_str("];\n");
    out.push_str(&system_types);
    out.push_str("];\n");
    let target = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo")).join("units.rs");
    std::fs::write(target, out).expect("cannot write the table of units");
}

/// The extension of a unit's list of names not provided (`docs/spec/stdlib/README.md`).
const UNAVAILABLE_EXT: &str = ".unavailable";

/// The table of names not provided, from each `units/<Name>.unavailable` (`<Name>.pas` must be
/// a unit). A line is empty, a `//` comment, or `<identifier>: <what to use instead>` (the
/// identifier may be `<class>.<member>`).
fn unavailable_table(dir: &Path, units: &[Unit]) -> String {
    let mut out = String::from(
        "/// The names not provided, by unit (in the order of the units) and in the order of each list.\n\
         pub const UNAVAILABLE: &[Unavailable] = &[\n",
    );
    for name in entries(dir) {
        let Some(unit) = name.strip_suffix(UNAVAILABLE_EXT) else {
            continue;
        };
        assert!(
            units.iter().any(|u| u.name == unit),
            "units/{name}: there is no unit `{unit}.pas`"
        );
        let text = read(dir, &name);
        let mut seen: Vec<String> = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let at = format!("units/{name}:{}", i + 1);
            let (ident, instead) = line
                .split_once(':')
                .unwrap_or_else(|| panic!("{at}: expected `<name>: <what to use instead>`"));
            let (ident, instead) = (ident.trim(), instead.trim());
            // An identifier, or a member of a class of the unit (`TMemoryStream.Memory`)
            let parts: Vec<&str> = ident.split('.').collect();
            assert!(
                parts.len() <= 2
                    && parts.iter().all(|p| {
                        !p.is_empty()
                            && !p.starts_with(|c: char| c.is_ascii_digit())
                            && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    }),
                "{at}: `{ident}` is not an identifier or `<class>.<member>`"
            );
            assert!(!instead.is_empty(), "{at}: say what to use instead");
            let lower = ident.to_ascii_lowercase();
            assert!(!seen.contains(&lower), "{at}: `{ident}` is listed twice");
            seen.push(lower);
            writeln!(
                out,
                "    Unavailable {{\n        unit: {unit:?},\n        name: {ident:?},\n        instead: {instead:?},\n    }},"
            )
            .unwrap();
        }
    }
    out.push_str("];\n");
    out
}

/// The names in a directory, sorted.
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|e| {
            e.expect("a directory entry")
                .file_name()
                .into_string()
                .expect("a file name in UTF-8")
        })
        .collect();
    names.sort();
    names
}

/// The files under `units/<rel>`, relative to `units/`.
fn collect(dir: &Path, rel: &str, out: &mut Vec<String>) {
    for name in entries(&dir.join(rel)) {
        let r = format!("{rel}/{name}");
        if dir.join(&r).is_dir() {
            collect(dir, &r, out);
        } else {
            out.push(r);
        }
    }
}

/// The argument of a line that holds only `{$I <path>}` (spaces around it allowed).
fn include_arg(line: &str) -> Option<&str> {
    let arg = line.trim().strip_prefix("{$I ")?.strip_suffix('}')?.trim();
    (!arg.is_empty()).then_some(arg)
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel))
        .unwrap_or_else(|e| panic!("cannot read units/{rel}: {e}"))
}

fn read_unit(dir: &Path, name: &str, included: &mut Vec<String>) -> Unit {
    let own = format!("{name}.pas");
    let text = read(dir, &own);
    let mut files = vec![own.clone()];
    let mut pieces: Vec<Piece> = Vec::new();
    let mut parts: Vec<Part> = Vec::new();
    // The start of the piece of the unit's own file being extended
    let mut from = 0;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let end = at + line.len();
        if let Some(arg) = include_arg(line) {
            let path = arg.replace('\\', "/");
            let body = read(dir, &path);
            assert!(
                body.ends_with('\n'),
                "units/{path} must end with a line break"
            );
            assert!(
                body.lines().all(|l| include_arg(l).is_none()),
                "units/{path}: an included file cannot include another"
            );
            if from < at {
                pieces.push(Piece {
                    file: 0,
                    start: from,
                    end: at,
                    part: None,
                });
            }
            let (part_name, section) = part_of(&path);
            let file = files.len();
            files.push(path.clone());
            let k = match parts.iter().position(|p| p.name == part_name) {
                Some(k) => k,
                None => {
                    parts.push(Part {
                        name: part_name,
                        files: Vec::new(),
                        names: Vec::new(),
                        refs: Vec::new(),
                    });
                    parts.len() - 1
                }
            };
            parts[k].files.push(file);
            for n in top_level_names(&body, section == "Implementation").keys {
                if !parts[k].names.contains(&n) {
                    parts[k].names.push(n);
                }
            }
            pieces.push(Piece {
                file,
                start: 0,
                end: body.len(),
                part: Some(k),
            });
            included.push(path);
            from = end;
        }
        at = end;
    }
    if files.len() == 1 {
        pieces.clear();
    } else if from < text.len() {
        pieces.push(Piece {
            file: 0,
            start: from,
            end: text.len(),
            part: None,
        });
    }
    Unit {
        name: name.to_string(),
        files,
        pieces,
        parts,
    }
}

/// The sections an included file can be for (`<Name>.<Section>.inc`).
const SECTIONS: &[&str] = &[
    "Interface",
    "Implementation",
    "Initialization",
    "Finalization",
];

/// The part an included file belongs to (`System.SysUtils/TEncoding` for
/// `System.SysUtils/TEncoding.Interface.inc`) and its section.
fn part_of(path: &str) -> (String, &str) {
    let stem = path.strip_suffix(".inc").unwrap_or_else(|| {
        panic!("units/{path}: an included file is named `<Name>.<Section>.inc`")
    });
    let (part, section) = stem
        .rsplit_once('.')
        .filter(|(_, s)| SECTIONS.contains(s))
        .unwrap_or_else(|| {
            panic!("units/{path}: an included file is named `<Name>.<Section>.inc`, the section one of {SECTIONS:?}")
        });
    (part.to_string(), section)
}

/// For each part, the names of the other parts (of any unit) that its files write
/// ([`Part::refs`]), in the order of the parts that declare them.
fn find_refs(dir: &Path, units: &mut [Unit]) {
    let all: Vec<(usize, usize, String)> = units
        .iter()
        .enumerate()
        .flat_map(|(i, u)| {
            u.parts
                .iter()
                .enumerate()
                .flat_map(move |(k, p)| p.names.iter().map(move |n| (i, k, n.clone())))
        })
        .collect();
    for (i, unit) in units.iter_mut().enumerate() {
        for (k, part) in unit.parts.iter_mut().enumerate() {
            let mut written: std::collections::HashSet<String> = std::collections::HashSet::new();
            for &f in &part.files {
                identifiers(&read(dir, &unit.files[f]), &mut written);
            }
            let mut refs: Vec<String> = Vec::new();
            for (j, l, n) in &all {
                if (*j, *l) != (i, k) && written.contains(n) && !refs.contains(n) {
                    refs.push(n.clone());
                }
            }
            part.refs = refs;
        }
    }
}

/// Adds the identifiers of a Pascal source to `out` as the keys of [`top_level_names`]: lowercase,
/// with `<` after one followed by `<` (not `<=` or `<>`). Not those in comments, directives and
/// strings; words that are keywords are added too. `puy-core` reads the identifiers of the
/// program in the same way.
fn identifiers(text: &str, out: &mut std::collections::HashSet<String>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'{' {
            while i < b.len() && b[i] != b'}' {
                i += 1;
            }
            i += 1;
        } else if c == b'(' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b')') {
                i += 1;
            }
            i += 2;
        } else if c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != b'\'' {
                i += 1;
            }
            i += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let mut lower = text[start..i].to_ascii_lowercase();
            let j = skip_trivia(b, i);
            // `Name<` that is not `Name <=` or `Name <>`: a generic name with its type arguments
            if b.get(j) == Some(&b'<') && !matches!(b.get(j + 1), Some(b'=' | b'>')) {
                lower.push('<');
            }
            out.insert(lower);
        } else if c.is_ascii_digit() || c == b'$' || c == b'#' {
            // A number (`$FF`, `1e5`) or a character (`#$0A`): not an identifier
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'$') {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

/// The position of the first byte at or after `i` that is not in a space or a comment (the
/// trivia the parser skips).
fn skip_trivia(b: &[u8], mut i: usize) -> usize {
    loop {
        if i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        } else if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b.get(i) == Some(&b'{') {
            while i < b.len() && b[i] != b'}' {
                i += 1;
            }
            i = (i + 1).min(b.len());
        } else if b[i..].starts_with(b"(*") {
            i += 2;
            while i < b.len() && !b[i..].starts_with(b"*)") {
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else {
            return i;
        }
    }
}

struct TopLevelNames {
    keys: Vec<String>,
    non_generic_types: Vec<String>,
}

/// The names declared at the top level of an included file, as the keys a use of them is written
/// with ([`identifiers`]): the types, constants and variables of its `type` / `const` / `var`
/// sections, the values of its enumerated types and its routines that are not methods. A name is
/// its lowercase key and the key with `<` after it (`x < 1`, `Foo<T>(…)`), except that a type
/// without type parameters has only the first: `TArray<Byte>` names the dynamic array of `System`,
/// not the class `TArray` of `System.Generics.Collections` (`docs/spec/collections.md` 1.3). The
/// units are written so that this can be read line by line: a section keyword and a routine
/// heading start at the first column, a name of a section is indented by 2 spaces, and in the
/// `implementation` section the body of a routine ends at a line that starts with `end;`.
/// `puy-core` checks the keys against its parser. The non-generic type names are also collected
/// for lookup in a type position in the Pascal part of `System`.
fn top_level_names(text: &str, implementation: bool) -> TopLevelNames {
    #[derive(PartialEq)]
    enum Section {
        None,
        Type,
        Names,
    }
    let mut keys = Vec::new();
    let mut non_generic_types = Vec::new();
    let mut section = Section::None;
    let mut in_body = false;
    let add = |key: String, out: &mut Vec<String>| {
        if !out.contains(&key) {
            out.push(key);
        }
    };
    // A name used with or without `<` after it
    let push = |name: &str, out: &mut Vec<String>| {
        let lower = name.to_ascii_lowercase();
        if !lower.is_empty() {
            add(format!("{lower}<"), out);
            add(lower, out);
        }
    };
    for line in text.lines() {
        let code = line.split("//").next().expect("split yields one piece");
        let t = code.trim();
        if t.is_empty() || t.starts_with('{') {
            continue;
        }
        let indent = code.len() - code.trim_start().len();
        if indent == 0 {
            if in_body {
                if t.to_ascii_lowercase().starts_with("end;") {
                    in_body = false;
                }
                continue;
            }
            let lower = t.to_ascii_lowercase();
            let mut words = lower.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
            let first = words.next().unwrap_or("");
            if matches!(
                first,
                "type" | "const" | "var" | "threadvar" | "resourcestring"
            ) {
                section = if first == "type" {
                    Section::Type
                } else {
                    Section::Names
                };
                let rest = t[first.len()..].trim();
                if !rest.is_empty() {
                    push(&leading_ident(rest), &mut keys);
                }
                continue;
            }
            section = Section::None;
            let rest = lower.strip_prefix("class ").unwrap_or(&lower);
            let routine = [
                "function ",
                "procedure ",
                "constructor ",
                "destructor ",
                "operator ",
            ]
            .iter()
            .find_map(|k| rest.strip_prefix(k));
            if let Some(after) = routine {
                let after_orig = &t[t.len() - after.len()..];
                let name = leading_ident(after_orig);
                let tail = after_orig[name.len()..].trim_start();
                let tail = if tail.starts_with('<') {
                    tail.split_once('>').map_or("", |(_, r)| r)
                } else {
                    tail
                };
                if !tail.starts_with('.') {
                    push(&name, &mut keys);
                }
                if implementation && !lower.contains("forward;") && !lower.contains("external") {
                    in_body = true;
                }
            }
        } else if indent == 2 && !in_body && section != Section::None {
            // `A, B: Integer;` declares both; `private`, `end;` and the like declare nothing
            let head = t
                .split([':', '=', '<'])
                .next()
                .expect("split yields one piece");
            if head.len() == t.len() {
                continue;
            }
            let names: Vec<&str> = head.split(',').map(str::trim).collect();
            if names
                .iter()
                .any(|n| n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            {
                continue;
            }
            let generic = t[head.len()..].starts_with('<');
            for n in names {
                if section == Section::Type && !generic {
                    let name = n.to_ascii_lowercase();
                    add(name.clone(), &mut keys);
                    add(name, &mut non_generic_types);
                } else {
                    push(n, &mut keys);
                }
            }
            // The values of an enumerated type are names of the unit too
            if let Some((_, rhs)) = t.split_once('=')
                && let Some(values) = rhs.trim().strip_prefix('(')
            {
                let values = values
                    .split_once(')')
                    .unwrap_or_else(|| {
                        panic!("write the values of an enumerated type on one line: {t}")
                    })
                    .0;
                for v in values.split(',') {
                    push(&leading_ident(v.trim()), &mut keys);
                }
            }
        }
    }
    TopLevelNames {
        keys,
        non_generic_types,
    }
}

/// The identifier at the start of `s`.
fn leading_ident(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// The helpers declared in the `interface` section of a unit (`TName = record helper for …`,
/// one declaration per line as the units are written), with the names of their methods,
/// properties and constants (the private ones too: a use of one reads the unit, to report it as
/// private; a constant is written `const Name = …;`, one per line). A helper ends at a line that
/// holds only `end;`.
fn helper_members(source: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        let t = line.trim();
        let lower = t.to_ascii_lowercase();
        if lower == "implementation" {
            break;
        }
        if lower.contains("= record helper for ") || lower.contains("= class helper for ") {
            let name = t.split('=').next().expect("split yields one piece").trim();
            out.push((name.to_string(), Vec::new()));
            inside = true;
            continue;
        }
        if lower == "end;" {
            inside = false;
        }
        if !inside {
            continue;
        }
        let rest = lower.strip_prefix("class ").unwrap_or(&lower);
        let Some(after) = ["function ", "procedure ", "property ", "const "]
            .iter()
            .find_map(|k| rest.strip_prefix(k))
        else {
            continue;
        };
        let name: String = t[t.len() - after.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let members = &mut out.last_mut().expect("inside a helper").1;
        if !members.contains(&name) {
            members.push(name);
        }
    }
    out
}
