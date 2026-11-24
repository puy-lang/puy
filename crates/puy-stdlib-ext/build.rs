//! Builds the list of the standard library's extensions from `src/units/`
//! (`docs/design/stdlib.md` 3.3).
//!
//! Each `src/units/<name>.rs` holds the Rust parts of one standard unit: an inline module
//! `native` marked with `#[puy_ext::pascal_module(unit = "…")]`. The generated file declares a
//! module for each file and the table of their registrations, in the order of the file names, so
//! that adding a unit adds a file and touches no shared line.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
        .join("src")
        .join("units");
    println!("cargo:rerun-if-changed=src/units");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| {
            let e = e.expect("a readable entry of src/units");
            e.file_name()
                .into_string()
                .unwrap_or_else(|n| panic!("src/units/{n:?}: not a UTF-8 name"))
        })
        .collect();
    names.sort();
    let mut mods = String::new();
    let mut table = String::from(
        "/// The registrations of the units, in the order of their files' names.\n\
         pub static EXTENSIONS: &[&::puy_ext::Extension] = &[\n",
    );
    for name in &names {
        let Some(module) = name.strip_suffix(".rs") else {
            panic!("src/units/{name}: only Rust files (`.rs`) belong in `src/units/`");
        };
        assert!(
            !module.is_empty()
                && module
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "src/units/{name}: the name must be a Rust module name in snake_case"
        );
        let path = dir.join(name);
        let path = path.to_str().expect("a UTF-8 path");
        writeln!(mods, "#[path = {path:?}]\npub mod {module};").expect("writing to a String");
        writeln!(table, "    &{module}::native::EXTENSION,").expect("writing to a String");
    }
    table.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo")).join("units.rs");
    std::fs::write(&out, mods + &table).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
}
