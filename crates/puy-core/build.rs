//! Embeds `docs/explain/<code>.md` for `puy explain` (`docs/spec/diagnostics.md` chapter 1).
//! Generates `OUT_DIR/explain_table.rs`: a table of `(code, body)` in ascending order of code.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dir = Path::new(&manifest).join("../../docs/explain");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "md")
                && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
            {
                println!("cargo:rerun-if-changed={}", p.display());
                let abs = p.canonicalize().unwrap_or(p.clone());
                entries.push((stem.to_string(), abs.display().to_string()));
            }
        }
    }
    entries.sort();
    let mut out = String::from(
        "/// Explanations from `docs/explain/` (in ascending order of code). Generated at build time.\npub static EXPLANATIONS: &[(&str, &str)] = &[\n",
    );
    for (code, path) in &entries {
        out.push_str(&format!("    ({code:?}, include_str!({path:?})),\n"));
    }
    out.push_str("];\n");
    let out_dir = env::var("OUT_DIR").unwrap();
    fs::write(Path::new(&out_dir).join("explain_table.rs"), out).unwrap();
}
