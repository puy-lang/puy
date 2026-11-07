//! Text rendering of diagnostics (`docs/spec/diagnostics.md` chapter 6). No colors (the CLI adds them).

use super::{Applicability, Diagnostic, Label, Summary};
use crate::source::{FileId, SourceFile, SourceMap, TextRange, TextSize};

/// Renders one diagnostic (without a trailing newline).
pub fn render_diagnostic(d: &Diagnostic, sources: &SourceMap) -> String {
    let mut out = String::new();
    out.push_str(&headline(d));
    let Some(primary) = &d.primary else {
        return out;
    };
    // Width of line numbers: the maximum of all line numbers shown for this diagnostic
    let mut all_lines: Vec<u32> = Vec::new();
    for l in std::iter::once(primary).chain(d.secondary.iter()) {
        let f = sources.get(l.span.file);
        all_lines.push(f.line_col(l.span.range.start()).line);
        all_lines.push(f.line_col(end_pos(l.span.range)).line);
    }
    let w = all_lines
        .iter()
        .max()
        .copied()
        .unwrap_or(1)
        .to_string()
        .len();
    let pad = " ".repeat(w);
    let pfile = sources.get(primary.span.file);
    let plc = pfile.line_col(primary.span.range.start());
    out.push('\n');
    out.push_str(&format!(
        "{pad} --> {}:{}:{}",
        pfile.path, plc.line, plc.column
    ));
    out.push('\n');
    out.push_str(&format!("{pad}  |"));
    // Excerpt of the same file
    render_excerpt(
        &mut out,
        pfile,
        primary.span.file,
        Some(primary),
        &d.secondary,
        w,
    );
    // Secondary in another file
    let mut other_files: Vec<FileId> = Vec::new();
    for s in &d.secondary {
        if s.span.file != primary.span.file && !other_files.contains(&s.span.file) {
            other_files.push(s.span.file);
        }
    }
    for fid in other_files {
        let f = sources.get(fid);
        let first = d.secondary.iter().find(|s| s.span.file == fid).unwrap();
        let lc = f.line_col(first.span.range.start());
        out.push('\n');
        out.push_str(&format!("{pad} ::: {}:{}:{}", f.path, lc.line, lc.column));
        render_excerpt(&mut out, f, fid, None, &d.secondary, w);
    }
    if !d.notes.is_empty() || !d.fixes.is_empty() {
        out.push('\n');
        out.push_str(&format!("{pad}  |"));
        for n in &d.notes {
            out.push('\n');
            out.push_str(&format!("{pad}  = note: {n}"));
        }
        for f in &d.fixes {
            out.push('\n');
            let kind = match f.applicability {
                Applicability::Safe => "fix",
                Applicability::Suggestion => "suggestion",
            };
            out.push_str(&format!("{pad}  = {kind}: {}", f.title));
        }
    }
    out
}

/// The first line.
pub fn headline(d: &Diagnostic) -> String {
    let sev = d.severity.as_str();
    let mut s = match d.code {
        Some(c) => format!("{sev}[{c}]: {}", d.message),
        None => format!("{sev}: {}", d.message),
    };
    if let Some(name) = d.name() {
        s.push_str(&format!(" ({name})"));
    }
    s
}

fn end_pos(range: TextRange) -> TextSize {
    if range.is_empty() {
        range.start()
    } else {
        range.end() - TextSize::from(1)
    }
}

/// Excerpt of one file (`primary` only when it is in the same file).
fn render_excerpt(
    out: &mut String,
    f: &SourceFile,
    fid: FileId,
    primary: Option<&Label>,
    secondary: &[Label],
    w: usize,
) {
    // Split the ranges per line: (line, display position of the start column, underline length, label, is primary, byte offset of the range start)
    struct Mark {
        line: u32,
        col: usize,
        len: usize,
        label: String,
        primary: bool,
        start: u32,
    }
    let mut marks: Vec<Mark> = Vec::new();
    let labels: Vec<(&Label, bool)> = primary
        .map(|p| (p, true))
        .into_iter()
        .chain(
            secondary
                .iter()
                .filter(|s| s.span.file == fid)
                .map(|s| (s, false)),
        )
        .collect();
    for (l, is_primary) in labels {
        let range = l.span.range;
        let start_lc = f.line_col(range.start());
        let end_lc = f.line_col(end_pos(range));
        let sl = start_lc.line - 1;
        let el = end_lc.line - 1;
        let line_start_byte = usize::from(f.line_start(sl));
        let start_col = display_width(f, sl, &f.bytes[line_start_byte..usize::from(range.start())]);
        if sl == el {
            let len = if range.is_empty() {
                1
            } else {
                display_width(
                    f,
                    sl,
                    &f.bytes[usize::from(range.start())..usize::from(range.end())],
                )
                .max(1)
            };
            marks.push(Mark {
                line: sl,
                col: start_col,
                len,
                label: l.label.to_string(),
                primary: is_primary,
                start: range.start().into(),
            });
        } else {
            // The first line runs from the start to the end of the line, the last line from the beginning of the line to the end
            let first_text = &f.bytes[usize::from(range.start())..];
            let first_line_len = f
                .line_bytes(sl)
                .len()
                .saturating_sub(usize::from(range.start()) - line_start_byte);
            let len = display_width(f, sl, &first_text[..first_line_len]).max(1);
            marks.push(Mark {
                line: sl,
                col: start_col,
                len,
                label: String::new(),
                primary: is_primary,
                start: range.start().into(),
            });
            let last_start = usize::from(f.line_start(el));
            let len = display_width(f, el, &f.bytes[last_start..usize::from(range.end())]).max(1);
            marks.push(Mark {
                line: el,
                col: 0,
                len,
                label: l.label.to_string(),
                primary: is_primary,
                start: f.line_start(el).into(),
            });
        }
    }
    let mut lines: Vec<u32> = marks.iter().map(|m| m.line).collect();
    lines.sort();
    lines.dedup();
    let pad = " ".repeat(w);
    let mut prev: Option<u32> = None;
    let push_line = |out: &mut String, line: u32| {
        let text = display_line(f, line);
        out.push('\n');
        let num = format!("{:>w$}", line + 1, w = w);
        if text.is_empty() {
            out.push_str(&format!(" {num} |"));
        } else {
            out.push_str(&format!(" {num} | {text}"));
        }
    };
    for line in lines {
        if let Some(p) = prev {
            if line == p + 2 {
                // When only one line is skipped, show that line as is (same as rustc)
                push_line(out, p + 1);
            } else if line > p + 2 {
                out.push('\n');
                out.push_str("...");
            }
        }
        prev = Some(line);
        push_line(out, line);
        // Underlines: primary first, then secondaries in column order
        let mut line_marks: Vec<&Mark> = marks.iter().filter(|m| m.line == line).collect();
        line_marks.sort_by_key(|m| (!m.primary, m.col, m.start));
        for m in line_marks {
            out.push('\n');
            let ch = if m.primary { "^" } else { "-" };
            let mut s = format!("{pad}  | {}{}", " ".repeat(m.col), ch.repeat(m.len));
            if !m.label.is_empty() {
                s.push(' ');
                s.push_str(&m.label);
            }
            out.push_str(&s);
        }
    }
}

/// Text of an excerpt line (tab is 4 spaces, control characters are control pictures, invalid UTF-8 is U+FFFD; a leading BOM is removed).
fn display_line(f: &SourceFile, line: u32) -> String {
    let mut bytes = f.line_bytes(line);
    if line == 0 && f.has_bom() {
        bytes = &bytes[3.min(bytes.len())..];
    }
    expand(bytes)
}

fn expand(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\t' => out.push_str("    "),
            '\x00'..='\x1f' => out.push(char::from_u32(0x2400 + c as u32).unwrap()),
            '\x7f' => out.push('\u{2421}'),
            c => out.push(c),
        }
    }
    out
}

/// Display width (number of characters after expansion). A leading BOM on line 0 is not counted.
fn display_width(f: &SourceFile, line: u32, bytes: &[u8]) -> usize {
    let mut bytes = bytes;
    if line == 0 && f.has_bom() && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes = &bytes[3..];
    }
    expand(bytes).chars().count()
}

/// The summary line (`None` if there are no errors or warnings).
pub fn summary_line(s: Summary) -> Option<String> {
    let plural = |n: usize, word: &str| {
        if n == 1 {
            format!("1 {word}")
        } else {
            format!("{n} {word}s")
        }
    };
    if s.errors > 0 {
        let mut line = format!("error: aborting due to {}", plural(s.errors, "error"));
        if s.warnings > 0 {
            line.push_str(&format!("; {} emitted", plural(s.warnings, "warning")));
        }
        Some(line)
    } else if s.warnings > 0 {
        Some(format!(
            "warning: {} emitted",
            plural(s.warnings, "warning")
        ))
    } else {
        None
    }
}

/// All diagnostics plus the summary (standard error of `puy check`, `.diag` of golden tests).
pub fn render_all(diags: &[Diagnostic], sources: &SourceMap) -> String {
    let mut parts: Vec<String> = diags
        .iter()
        .map(|d| render_diagnostic(d, sources))
        .collect();
    if let Some(s) = summary_line(Summary::of(diags)) {
        parts.push(s);
    }
    if parts.is_empty() {
        return String::new();
    }
    let mut out = parts.join("\n\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{Code, Fix, Severity, Stage, TextEdit};
    use crate::source::{SourceFile, Span};

    fn sources(text: &str) -> SourceMap {
        let mut m = SourceMap::new();
        m.add(SourceFile::new("src/Hello.pas", text.as_bytes().to_vec()));
        m
    }

    fn span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), TextRange::new(start.into(), end.into()))
    }

    #[test]
    fn spec_example() {
        let text = "program Hello;\nprocedure P;\nvar\n  Count: Integer;\n  S: string;\nbegin\n  S := S + 'x';\nend;\nbegin end.\n";
        let m = sources(text);
        let s_decl = text.find("S: string").unwrap() as u32;
        let s_read = text.find("S + 'x'").unwrap() as u32;
        let d = Diagnostic::error(
            Code::E(2000),
            "variable `S` may be read before it is assigned",
            Stage::Flow,
        )
        .with_primary(span(s_read, s_read + 1), "read here")
        .with_secondary(span(s_decl, s_decl + 1), "declared here")
        .with_note("Puy requires every local variable to be assigned before it is read")
        .with_fix(Fix::safe(
            "add an initial value: `var S: string = '';`",
            vec![TextEdit::insert(FileId(0), (s_decl + 9).into(), " = ''")],
        ));
        let expected = "\
error[E2000]: variable `S` may be read before it is assigned
  --> src/Hello.pas:7:8
   |
 5 |   S: string;
   |   - declared here
 6 | begin
 7 |   S := S + 'x';
   |        ^ read here
   |
   = note: Puy requires every local variable to be assigned before it is read
   = fix: add an initial value: `var S: string = '';`";
        assert_eq!(render_diagnostic(&d, &m), expected);
        let count = text.find("Count").unwrap() as u32;
        let w = Diagnostic::warning(Code::W(1), "unused variable `Count`", Stage::Flow)
            .with_primary(span(count, count + 5), "declared here")
            .with_note("disable with `{$WARN unused-variable OFF}`");
        let expected = "\
warning[W0001]: unused variable `Count` (unused-variable)
  --> src/Hello.pas:4:3
   |
 4 |   Count: Integer;
   |   ^^^^^ declared here
   |
   = note: disable with `{$WARN unused-variable OFF}`";
        assert_eq!(render_diagnostic(&w, &m), expected);
        let all = render_all(&[d, w], &m);
        assert!(all.ends_with("= note: disable with `{$WARN unused-variable OFF}`\n\nerror: aborting due to 1 error; 1 warning emitted\n"));
    }

    #[test]
    fn gaps_and_width() {
        let mut text = String::new();
        for i in 1..=12 {
            text.push_str(&format!("line{i}\n"));
        }
        let m = sources(&text);
        let d = Diagnostic::error(Code::E(700), "x", Stage::Sema)
            .with_primary(span(68, 74), "")
            .with_secondary(span(0, 5), "declared here");
        let r = render_diagnostic(&d, &m);
        let expected = "\
error[E0700]: x
   --> src/Hello.pas:12:1
    |
  1 | line1
    | ----- declared here
...
 12 | line12
    | ^^^^^^";
        assert_eq!(r, expected);
    }

    #[test]
    fn eof_after_newline_and_tabs() {
        let m = sources("abc\n");
        let d = Diagnostic::error(
            Code::E(203),
            "unterminated conditional directive",
            Stage::Directives,
        )
        .with_primary(span(4, 4), "");
        let r = render_diagnostic(&d, &m);
        assert_eq!(
            r,
            "error[E0203]: unterminated conditional directive\n  --> src/Hello.pas:2:1\n   |\n 2 |\n   | ^"
        );
        let m = sources("\tx\0y");
        let d = Diagnostic::error(Code::E(3), "invalid character `\u{2400}`", Stage::Lexer)
            .with_primary(span(2, 3), "");
        let r = render_diagnostic(&d, &m);
        assert_eq!(
            r,
            "error[E0003]: invalid character `\u{2400}`\n  --> src/Hello.pas:1:3\n   |\n 1 |     x\u{2400}y\n   |      ^"
        );
    }

    #[test]
    fn multi_line_range_and_other_file() {
        let m = {
            let mut m = SourceMap::new();
            m.add(SourceFile::new("main.pas", b"a;\nb;\nc;\n".to_vec()));
            m.add(SourceFile::new("inc.inc", b"var X: Integer;\n".to_vec()));
            m
        };
        let d = Diagnostic::warning(Code::W(3), "unreachable code", Stage::Flow)
            .with_primary(span(3, 8), "");
        let r = render_diagnostic(&d, &m);
        assert_eq!(
            r,
            "warning[W0003]: unreachable code (unreachable-code)\n  --> main.pas:2:1\n   |\n 2 | b;\n   | ^^\n 3 | c;\n   | ^^"
        );
        let d = Diagnostic::error(
            Code::E(210),
            "include file must contain only directives and comments",
            Stage::Directives,
        )
        .with_primary(Span::new(FileId(1), TextRange::new(0.into(), 3.into())), "")
        .with_secondary(span(0, 1), "included from here");
        let r = render_diagnostic(&d, &m);
        assert_eq!(
            r,
            "error[E0210]: include file must contain only directives and comments\n  --> inc.inc:1:1\n   |\n 1 | var X: Integer;\n   | ^^^\n  ::: main.pas:1:1\n 1 | a;\n   | - included from here"
        );
    }

    #[test]
    fn positionless_and_summary() {
        let m = sources("");
        let d = Diagnostic::new(
            Code::F(2),
            Severity::Error,
            "cannot read `missing.pas`: file not found",
            Stage::Driver,
        );
        assert_eq!(
            render_diagnostic(&d, &m),
            "error[F0002]: cannot read `missing.pas`: file not found"
        );
        assert_eq!(
            summary_line(Summary {
                errors: 2,
                warnings: 1,
                notes: 0
            })
            .unwrap(),
            "error: aborting due to 2 errors; 1 warning emitted"
        );
        assert_eq!(
            summary_line(Summary {
                errors: 0,
                warnings: 3,
                notes: 1
            })
            .unwrap(),
            "warning: 3 warnings emitted"
        );
        assert_eq!(summary_line(Summary::default()), None);
        assert_eq!(render_all(&[], &m), "");
    }
}
