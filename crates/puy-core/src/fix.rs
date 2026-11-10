//! The `puy fix` procedure (`docs/spec/diagnostics.md` 2.1, 10.7): apply the `safe` fixes and recompile (up to 10 times).
//!
//! The fixes are applied to every source file checked: the passed file and the units it uses
//! (`docs/spec/units.md` 8.3). The included files are not rewritten.

use std::collections::HashMap;

use crate::diagnostics::{Applicability, Diagnostic, TextEdit};
use crate::driver::{CompileOptions, compile};
use crate::source::{FileId, SourceMap, SourceProvider};

/// An applied fix.
#[derive(Debug, Clone)]
pub struct AppliedFix {
    pub code: String,
    pub title: String,
    /// The file the fix rewrites (the path of the passed file, or a unit's path in the form of
    /// `docs/spec/units.md` 8.2).
    pub file: String,
    /// 1-based.
    pub round: usize,
    /// The edits, in terms of the source before this round was applied.
    pub edits: Vec<TextEdit>,
    /// The source of `file` before this round was applied (to look up lines and columns for
    /// `original`).
    pub source_before: Vec<u8>,
}

/// A `safe` fix inside an included file that was not applied.
#[derive(Debug, Clone)]
pub struct SkippedFix {
    pub code: String,
    pub title: String,
    /// Location of `primary` (from the last compilation after applying).
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub edits: Vec<TextEdit>,
}

/// A file rewritten by the fixes.
#[derive(Debug, Clone)]
pub struct FixedFile {
    pub path: String,
    /// The source after applying (line breaks not yet normalized; `normalize_newlines`).
    pub bytes: Vec<u8>,
}

pub struct FixResult {
    /// The files with at least one applied fix, in file order (`docs/spec/units.md` 8.1): the
    /// order in which `puy fix` writes them.
    pub files: Vec<FixedFile>,
    pub applied: Vec<AppliedFix>,
    pub skipped: Vec<SkippedFix>,
    /// Number of compilations.
    pub rounds: usize,
    pub converged: bool,
    /// Diagnostics from the last compilation.
    pub diagnostics: Vec<Diagnostic>,
    pub sources: SourceMap,
}

impl FixResult {
    /// The source of `path` after applying, if a fix was applied to it.
    pub fn fixed(&self, path: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.bytes.as_slice())
    }
}

pub const MAX_ROUNDS: usize = 10;

/// Whether two edits overlap (touching at an endpoint also counts as overlapping).
fn overlaps(a: &TextEdit, b: &TextEdit) -> bool {
    a.file == b.file && a.range.start() <= b.range.end() && b.range.start() <= a.range.end()
}

/// Selects the fixes to apply in one round (in diagnostic order, only non-overlapping ones),
/// among those whose `primary` is in `files` (the files `puy fix` rewrites) and whose edits are
/// all in the primary's file.
/// Returns (diagnostic index, the fix's edits) for each.
pub fn select_round(diags: &[Diagnostic], files: &[FileId]) -> Vec<(usize, Vec<TextEdit>)> {
    let mut chosen: Vec<TextEdit> = Vec::new();
    let mut out: Vec<(usize, Vec<TextEdit>)> = Vec::new();
    for (i, d) in diags.iter().enumerate() {
        let Some(p) = &d.primary else { continue };
        if !files.contains(&p.span.file) {
            continue;
        }
        let Some(fix) = d
            .fixes
            .iter()
            .find(|f| f.applicability == Applicability::Safe)
        else {
            continue;
        };
        // The edits of a fix are in its primary's file (`docs/spec/diagnostics.md` 2.1), which
        // `AppliedFix` and the output of `puy fix` rely on
        if fix.edits.iter().any(|e| e.file != p.span.file) {
            continue;
        }
        // An identical edit counts as already selected
        let edits: Vec<TextEdit> = fix
            .edits
            .iter()
            .filter(|e| {
                !chosen.iter().any(|c| {
                    c.file == e.file && c.range == e.range && c.replacement == e.replacement
                })
            })
            .cloned()
            .collect();
        if edits.is_empty() {
            continue;
        }
        if edits.iter().any(|e| chosen.iter().any(|c| overlaps(c, e))) {
            continue;
        }
        chosen.extend(edits.iter().cloned());
        out.push((i, fix.edits.clone()));
    }
    out
}

/// Applies the edits (all in one file) in descending order of position. If `newline` is given,
/// inserted `LF`s are replaced with it.
pub fn apply_edits(bytes: &[u8], edits: &[TextEdit], newline: Option<&str>) -> Vec<u8> {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.range.start()));
    let mut out = bytes.to_vec();
    for e in sorted {
        let start = usize::from(e.range.start()).min(out.len());
        let end = usize::from(e.range.end()).min(out.len());
        let replacement = match newline {
            Some(nl) if nl != "\n" => e.replacement.replace('\n', nl),
            _ => e.replacement.clone(),
        };
        out.splice(start..end, replacement.bytes());
    }
    out
}

/// The kind of the file's first line break (`LF` if there is none).
pub fn detect_newline(bytes: &[u8]) -> &'static str {
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'\n' => return "\n",
            b'\r' => {
                return if bytes.get(i + 1) == Some(&b'\n') {
                    "\r\n"
                } else {
                    "\r"
                };
            }
            _ => {}
        }
    }
    "\n"
}

/// Normalizes `CR LF` and `CR` to `LF`.
pub fn normalize_newlines(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(b'\n');
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    out
}

/// Serves the rewritten units in place of the files on disk, so that the next round compiles
/// the fixed sources.
struct Overlay<'a> {
    inner: &'a mut dyn SourceProvider,
    files: &'a HashMap<String, Vec<u8>>,
}

impl SourceProvider for Overlay<'_> {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, puy_host::HostError> {
        match self.files.get(path) {
            Some(b) => Ok(b.clone()),
            None => self.inner.read(path),
        }
    }

    fn list(&mut self, dir: &str) -> Result<Vec<String>, puy_host::HostError> {
        self.inner.list(dir)
    }
}

/// Applies fixes until none remain. With `keep_line_endings`, inserted line breaks follow each
/// file's line breaks.
pub fn fix(
    path: &str,
    bytes: Vec<u8>,
    provider: &mut dyn SourceProvider,
    opts: &CompileOptions,
    keep_line_endings: bool,
) -> FixResult {
    let mut main = bytes;
    // The rewritten units by path, and the paths of all rewritten files in the order first
    // rewritten
    let mut units: HashMap<String, Vec<u8>> = HashMap::new();
    let mut touched: Vec<String> = Vec::new();
    // The first line break of each file, from its source before any fix
    let mut newlines: HashMap<String, &'static str> = HashMap::new();
    let mut applied: Vec<AppliedFix> = Vec::new();
    let mut rounds = 0;
    loop {
        rounds += 1;
        let out = {
            let mut overlay = Overlay {
                inner: &mut *provider,
                files: &units,
            };
            compile(path, main.clone(), &mut overlay, opts)
        };
        let selected = select_round(&out.diagnostics, &out.files);
        if selected.is_empty() || rounds > MAX_ROUNDS {
            let converged = selected.is_empty();
            let skipped = collect_skipped(&out.diagnostics, &out.sources, &out.files);
            // File order: the files of the last compilation, then any rewritten file it no
            // longer reads
            let mut order: Vec<String> = out
                .files
                .iter()
                .map(|f| out.sources.get(*f).path.clone())
                .filter(|p| touched.contains(p))
                .collect();
            for p in &touched {
                if !order.contains(p) {
                    order.push(p.clone());
                }
            }
            let files = order
                .into_iter()
                .map(|p| {
                    let bytes = if p == path {
                        main.clone()
                    } else {
                        units.remove(&p).expect("a rewritten unit has its source")
                    };
                    FixedFile { path: p, bytes }
                })
                .collect();
            return FixResult {
                files,
                applied,
                skipped,
                rounds,
                converged,
                diagnostics: out.diagnostics,
                sources: out.sources,
            };
        }
        // The edits of this round by file, without duplicates
        let mut by_file: Vec<(FileId, Vec<TextEdit>)> = Vec::new();
        for (i, edits) in &selected {
            let d = &out.diagnostics[*i];
            let fix = d
                .safe_fix()
                .expect("select_round picks diagnostics with a safe fix");
            let file = d
                .primary
                .as_ref()
                .expect("select_round picks diagnostics with a primary")
                .span
                .file;
            let src = out.sources.get(file);
            applied.push(AppliedFix {
                code: d.code.map(|c| c.to_string()).unwrap_or_default(),
                title: fix.title.clone(),
                file: src.path.clone(),
                round: rounds,
                edits: edits.clone(),
                source_before: src.bytes.clone(),
            });
            for e in edits {
                let slot = match by_file.iter().position(|(f, _)| *f == e.file) {
                    Some(k) => k,
                    None => {
                        by_file.push((e.file, Vec::new()));
                        by_file.len() - 1
                    }
                };
                let list = &mut by_file[slot].1;
                if !list
                    .iter()
                    .any(|c| c.range == e.range && c.replacement == e.replacement)
                {
                    list.push(e.clone());
                }
            }
        }
        for (file, edits) in by_file {
            let src = out.sources.get(file);
            let newline = if keep_line_endings {
                Some(
                    *newlines
                        .entry(src.path.clone())
                        .or_insert_with(|| detect_newline(&src.bytes)),
                )
            } else {
                None
            };
            let fixed = apply_edits(&src.bytes, &edits, newline);
            if !touched.contains(&src.path) {
                touched.push(src.path.clone());
            }
            if file == FileId(0) {
                main = fixed;
            } else {
                units.insert(src.path.clone(), fixed);
            }
        }
    }
}

/// The `safe` fixes in included files. `checked` are the source files of the compilation (the
/// passed file and the units it uses), whose fixes are applied.
fn collect_skipped(
    diags: &[Diagnostic],
    sources: &SourceMap,
    checked: &[FileId],
) -> Vec<SkippedFix> {
    let mut out = Vec::new();
    for d in diags {
        let Some(p) = &d.primary else { continue };
        if checked.contains(&p.span.file) {
            continue;
        }
        let Some(fix) = d.safe_fix() else { continue };
        let f = sources.get(p.span.file);
        let lc = f.line_col(p.span.range.start());
        out.push(SkippedFix {
            code: d.code.map(|c| c.to_string()).unwrap_or_default(),
            title: fix.title.clone(),
            file: f.path.clone(),
            line: lc.line,
            column: lc.column,
            edits: fix.edits.clone(),
        });
    }
    out
}

/// Number of `safe` fixes `puy fix` would apply (for the `puy fix` hint line;
/// `docs/spec/diagnostics.md` 10.4): those whose `primary` is in `files`, the source files of the
/// compilation (`CompileOutput::files`). Fixes inside included files are not counted.
pub fn applicable_fix_count(diags: &[Diagnostic], files: &[FileId]) -> usize {
    diags
        .iter()
        .filter(|d| {
            d.primary
                .as_ref()
                .is_some_and(|p| files.contains(&p.span.file))
                && d.safe_fix().is_some()
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySourceProvider;

    fn run_fix(src: &str) -> FixResult {
        let mut p = MemorySourceProvider::new();
        fix(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut p,
            &CompileOptions::default(),
            false,
        )
    }

    #[test]
    fn inserts_semicolon_and_uses() {
        let r = run_fix(
            "program Hello\nvar X: Integer\nbegin\n  X := StrToInt('1')\n  WriteLn(X)\nend.\n",
        );
        let text = String::from_utf8(r.fixed("t.pas").unwrap().to_vec()).unwrap();
        assert_eq!(
            text,
            "program Hello;\nuses System.SysUtils;\nvar X: Integer;\nbegin\n  X := StrToInt('1');\n  WriteLn(X)\nend.\n"
        );
        assert!(r.converged);
        assert!(r.rounds >= 2, "{}", r.rounds);
        assert!(
            r.diagnostics.iter().all(|d| !d.is_error()),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn removes_ignored_directive_and_fixes_initial_value() {
        let r = run_fix(
            "{$APPTYPE CONSOLE}\nprocedure P;\nvar S: string;\nbegin\n  S := S + 'x';\n  WriteLn(S);\nend;\n",
        );
        let text = String::from_utf8(r.fixed("t.pas").unwrap().to_vec()).unwrap();
        assert_eq!(
            text,
            "procedure P;\nvar S: string = '';\nbegin\n  S := S + 'x';\n  WriteLn(S);\nend;\n"
        );
        assert_eq!(r.applied.len(), 2);
    }

    #[test]
    fn keep_line_endings() {
        let mut p = MemorySourceProvider::new();
        let r = fix(
            "t.pas",
            b"program Hello\r\nbegin\r\n  X := StrToInt('1');\r\nend.\r\n".to_vec(),
            &mut p,
            &CompileOptions::default(),
            true,
        );
        let text = String::from_utf8(r.fixed("t.pas").unwrap().to_vec()).unwrap();
        assert_eq!(
            text,
            "program Hello;\r\nuses System.SysUtils;\r\nbegin\r\n  X := StrToInt('1');\r\nend.\r\n"
        );
        assert_eq!(normalize_newlines(b"a\r\nb\rc\n"), b"a\nb\nc\n");
        assert_eq!(detect_newline(b"ab\rc"), "\r");
    }

    #[test]
    fn nothing_to_fix() {
        let r = run_fix("WriteLn('x');\n");
        assert!(r.applied.is_empty());
        assert_eq!(r.rounds, 1);
        assert!(r.converged);
    }
}
