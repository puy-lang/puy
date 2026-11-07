//! The `puy fix` procedure (`docs/spec/diagnostics.md` 2.1, 10.7): apply the `safe` fixes and recompile (up to 10 times).

use crate::diagnostics::{Applicability, Diagnostic, TextEdit};
use crate::driver::{CompileOptions, compile};
use crate::source::{FileId, SourceMap, SourceProvider, TextRange};

/// An applied fix.
#[derive(Debug, Clone)]
pub struct AppliedFix {
    pub code: String,
    pub title: String,
    /// 1-based.
    pub round: usize,
    /// The edits, in terms of the source before this round was applied.
    pub edits: Vec<TextEdit>,
    /// The source before this round was applied (to look up lines and columns for `original`).
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

pub struct FixResult {
    /// The source after applying.
    pub bytes: Vec<u8>,
    pub applied: Vec<AppliedFix>,
    pub skipped: Vec<SkippedFix>,
    /// Number of compilations.
    pub rounds: usize,
    pub converged: bool,
    /// Diagnostics from the last compilation.
    pub diagnostics: Vec<Diagnostic>,
    pub sources: SourceMap,
}

pub const MAX_ROUNDS: usize = 10;

/// Whether two edits overlap (touching at an endpoint also counts as overlapping).
fn overlaps(a: &TextEdit, b: &TextEdit) -> bool {
    a.file == b.file && a.range.start() <= b.range.end() && b.range.start() <= a.range.end()
}

/// Selects the edits to apply in one round (in diagnostic order, only non-overlapping ones).
/// Returns (the fixes to apply as (diagnostic index, fix), all of the edits).
pub fn select_round(diags: &[Diagnostic], file: FileId) -> Vec<(usize, Vec<TextEdit>)> {
    let mut chosen: Vec<TextEdit> = Vec::new();
    let mut out: Vec<(usize, Vec<TextEdit>)> = Vec::new();
    for (i, d) in diags.iter().enumerate() {
        let Some(p) = &d.primary else { continue };
        if p.span.file != file {
            continue;
        }
        let Some(fix) = d
            .fixes
            .iter()
            .find(|f| f.applicability == Applicability::Safe)
        else {
            continue;
        };
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

/// Applies the edits in descending order of position. If `newline` is given, inserted `LF`s are replaced with it.
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

/// Applies fixes until none remain. With `keep_line_endings`, inserted line breaks follow the file's line breaks.
pub fn fix(
    path: &str,
    bytes: Vec<u8>,
    provider: &mut dyn SourceProvider,
    opts: &CompileOptions,
    keep_line_endings: bool,
) -> FixResult {
    let newline = if keep_line_endings {
        Some(detect_newline(&bytes))
    } else {
        None
    };
    let mut current = bytes;
    let mut applied: Vec<AppliedFix> = Vec::new();
    let mut rounds = 0;
    loop {
        rounds += 1;
        let out = compile(path, current.clone(), provider, opts);
        let selected = select_round(&out.diagnostics, FileId(0));
        if selected.is_empty() || rounds > MAX_ROUNDS {
            let converged = selected.is_empty();
            let skipped = collect_skipped(&out.diagnostics, &out.sources);
            return FixResult {
                bytes: current,
                applied,
                skipped,
                rounds,
                converged,
                diagnostics: out.diagnostics,
                sources: out.sources,
            };
        }
        let mut all_edits: Vec<TextEdit> = Vec::new();
        for (i, edits) in &selected {
            let d = &out.diagnostics[*i];
            let fix = d.safe_fix().unwrap();
            applied.push(AppliedFix {
                code: d.code.map(|c| c.to_string()).unwrap_or_default(),
                title: fix.title.clone(),
                round: rounds,
                edits: edits.clone(),
                source_before: current.clone(),
            });
            for e in edits {
                if !all_edits
                    .iter()
                    .any(|c| c.range == e.range && c.replacement == e.replacement)
                {
                    all_edits.push(e.clone());
                }
            }
        }
        current = apply_edits(&current, &all_edits, newline);
    }
}

fn collect_skipped(diags: &[Diagnostic], sources: &SourceMap) -> Vec<SkippedFix> {
    let mut out = Vec::new();
    for d in diags {
        let Some(p) = &d.primary else { continue };
        if p.span.file == FileId(0) {
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

/// Number of applicable `safe` fixes (for the `puy fix` hint line). Fixes inside included files are not counted.
pub fn applicable_fix_count(diags: &[Diagnostic]) -> usize {
    diags
        .iter()
        .filter(|d| {
            d.primary.as_ref().is_some_and(|p| p.span.file == FileId(0)) && d.safe_fix().is_some()
        })
        .count()
}

#[allow(dead_code)]
fn _unused(_: TextRange) {}

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
        let text = String::from_utf8(r.bytes.clone()).unwrap();
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
        let text = String::from_utf8(r.bytes.clone()).unwrap();
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
        let text = String::from_utf8(r.bytes.clone()).unwrap();
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
