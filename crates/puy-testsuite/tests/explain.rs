//! Checks the code examples in `docs/explain/<code>.md` (requirements §13.2, `docs/spec/diagnostics.md` chapter 1).
//!
//! - `pascal compile-fail`: lines annotated `// error: <code>` get an error with that code, and
//!   unannotated lines get no error. A block with no annotations is an error.
//! - `pascal`, `pascal no-run`: no diagnostics at all (including warnings).
//! - `pascal ignore`: not checked.

use puy_core::diagnostics::explain;
use puy_core::directives::BuildConfig;
use puy_core::driver::{CompileOptions, compile};
use puy_core::source::MemorySourceProvider;

struct Block {
    info: String,
    code: String,
    /// Line number of the code's first line within the document (1-based; for reporting).
    line: usize,
}

fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut cur: Option<Block> = None;
    for (i, line) in text.lines().enumerate() {
        match &mut cur {
            None => {
                if let Some(info) = line.strip_prefix("```")
                    && !info.is_empty()
                {
                    cur = Some(Block {
                        info: info.trim().to_string(),
                        code: String::new(),
                        line: i + 2,
                    });
                }
            }
            Some(b) => {
                if line.starts_with("```") {
                    out.push(cur.take().unwrap());
                } else {
                    b.code.push_str(line);
                    b.code.push('\n');
                }
            }
        }
    }
    out
}

fn compile_block(b: &Block) -> puy_core::driver::CompileOutput {
    let opts = CompileOptions {
        build: BuildConfig::default(),
        extra_diagnostics: Vec::new(),
        script_name: None,
        extra_files: Vec::new(),
        ..Default::default()
    };
    let mut provider = MemorySourceProvider::new();
    compile(
        "example.pas",
        b.code.clone().into_bytes(),
        &mut provider,
        &opts,
    )
}

#[test]
fn explain_examples() {
    let mut failures: Vec<String> = Vec::new();
    for (code, text) in explain::all() {
        let name = format!("docs/explain/{code}.md");
        let bs = blocks(text);
        assert!(!bs.is_empty(), "{name}: no code blocks");
        for b in &bs {
            let kind = b.info.split_whitespace().collect::<Vec<_>>();
            if kind.first() != Some(&"pascal") {
                failures.push(format!("{name}:{}: unknown block `{}`", b.line, b.info));
                continue;
            }
            let mode = kind.get(1).copied().unwrap_or("");
            if mode == "ignore" {
                continue;
            }
            let out = compile_block(b);
            let sources = &out.sources;
            if mode == "compile-fail" {
                // annotated line -> code
                let expected: Vec<(usize, String)> = b
                    .code
                    .lines()
                    .enumerate()
                    .filter_map(|(i, l)| {
                        l.split("// error: ")
                            .nth(1)
                            .map(|c| (i + 1, c.trim().to_string()))
                    })
                    .collect();
                if expected.is_empty() {
                    failures.push(format!(
                        "{name}:{}: compile-fail block without `// error:`",
                        b.line
                    ));
                    continue;
                }
                let mut actual: Vec<(usize, String)> = out
                    .diagnostics
                    .iter()
                    .filter(|d| d.is_error())
                    .filter_map(|d| {
                        let p = d.primary.as_ref()?;
                        let line = sources.line_col(p.span).line as usize;
                        Some((line, d.code.map(|c| c.to_string()).unwrap_or_default()))
                    })
                    .collect();
                actual.sort();
                actual.dedup();
                let mut expected = expected;
                expected.sort();
                if actual != expected {
                    failures.push(format!(
                        "{name}:{}: expected errors {expected:?}, found {actual:?}",
                        b.line
                    ));
                }
            } else {
                for d in &out.diagnostics {
                    let where_ = d
                        .primary
                        .as_ref()
                        .map(|p| sources.line_col(p.span).line.to_string())
                        .unwrap_or_default();
                    failures.push(format!(
                        "{name}:{}: unexpected diagnostic at line {where_}: {} {}",
                        b.line,
                        d.code.map(|c| c.to_string()).unwrap_or_default(),
                        d.message
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
