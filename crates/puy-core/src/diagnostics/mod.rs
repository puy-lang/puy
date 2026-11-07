//! Diagnostics: source locations, error codes, fixes, rendering, JSON output (`docs/spec/diagnostics.md`).
//!
//! The structured data (`Diagnostic`) is authoritative; rendering (`render`) and JSON (`json`) are done afterwards
//! (`docs/design/compiler-pipeline.md` 3.6).

pub mod codes;
pub mod explain;
pub mod json;
pub mod render;

use std::fmt;

use crate::source::{FileId, Span, TextRange};

/// Error code (`E0001` / `W0001` / `F0001`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Code {
    E(u16),
    W(u16),
    F(u16),
}

impl Code {
    pub fn is_warning(self) -> bool {
        matches!(self, Code::W(_))
    }

    /// Creates one from a string such as `"E0700"`.
    pub fn parse(s: &str) -> Option<Code> {
        let (kind, num) = s.split_at_checked(1)?;
        if num.len() != 4 || !num.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let n: u16 = num.parse().ok()?;
        match kind {
            "E" => Some(Code::E(n)),
            "W" => Some(Code::W(n)),
            "F" => Some(Code::F(n)),
            _ => None,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Code::E(n) => write!(f, "E{n:04}"),
            Code::W(n) => write!(f, "W{n:04}"),
            Code::F(n) => write!(f, "F{n:04}"),
        }
    }
}

/// Severity (chapter 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Fatal,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Fatal => "fatal",
        }
    }

    /// Counted as `errors` in the summary.
    pub fn counts_as_error(self) -> bool {
        matches!(self, Severity::Error | Severity::Fatal)
    }
}

/// The stage that issued the diagnostic (an ordering key; chapter 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Diagnostics created by the CLI (F0002, E2507). Those without a location come first.
    Driver,
    Lexer,
    Directives,
    Parser,
    Sema,
    Flow,
    Codegen,
}

/// A location and a short label (vocabulary in 2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub label: &'static str,
}

impl Label {
    pub fn new(span: Span, label: &'static str) -> Self {
        Label { span, label }
    }
}

/// How readily a fix can be applied (2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Applicability {
    Safe,
    Suggestion,
}

impl Applicability {
    pub fn as_str(self) -> &'static str {
        match self {
            Applicability::Safe => "safe",
            Applicability::Suggestion => "suggestion",
        }
    }
}

/// A text edit (a byte range and the replacement string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub file: FileId,
    pub range: TextRange,
    pub replacement: String,
}

impl TextEdit {
    pub fn insert(file: FileId, at: crate::source::TextSize, text: impl Into<String>) -> Self {
        TextEdit {
            file,
            range: TextRange::empty(at),
            replacement: text.into(),
        }
    }

    pub fn replace(file: FileId, range: TextRange, text: impl Into<String>) -> Self {
        TextEdit {
            file,
            range,
            replacement: text.into(),
        }
    }

    pub fn delete(file: FileId, range: TextRange) -> Self {
        TextEdit {
            file,
            range,
            replacement: String::new(),
        }
    }
}

/// A fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub title: String,
    pub applicability: Applicability,
    pub edits: Vec<TextEdit>,
}

impl Fix {
    pub fn safe(title: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Fix {
            title: title.into(),
            applicability: Applicability::Safe,
            edits,
        }
    }

    pub fn suggestion(title: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Fix {
            title: title.into(),
            applicability: Applicability::Suggestion,
            edits,
        }
    }
}

/// One diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// `None` only for a failure to write standard output at exit (`code: null` in JSON).
    pub code: Option<Code>,
    pub severity: Severity,
    pub message: String,
    /// `None` for diagnostics without a location (`F` codes).
    pub primary: Option<Label>,
    pub secondary: Vec<Label>,
    pub notes: Vec<String>,
    pub fixes: Vec<Fix>,
    pub stage: Stage,
}

impl Diagnostic {
    pub fn new(code: Code, severity: Severity, message: impl Into<String>, stage: Stage) -> Self {
        Diagnostic {
            code: Some(code),
            severity,
            message: message.into(),
            primary: None,
            secondary: Vec::new(),
            notes: Vec::new(),
            fixes: Vec::new(),
            stage,
        }
    }

    pub fn error(code: Code, message: impl Into<String>, stage: Stage) -> Self {
        Self::new(code, Severity::Error, message, stage)
    }

    pub fn warning(code: Code, message: impl Into<String>, stage: Stage) -> Self {
        Self::new(code, Severity::Warning, message, stage)
    }

    pub fn with_primary(mut self, span: Span, label: &'static str) -> Self {
        self.primary = Some(Label::new(span, label));
        self
    }

    pub fn with_secondary(mut self, span: Span, label: &'static str) -> Self {
        self.secondary.push(Label::new(span, label));
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }

    /// Name of the warning (only for `W` codes).
    pub fn name(&self) -> Option<&'static str> {
        match self.code {
            Some(c @ Code::W(_)) => codes::warning_name(c),
            _ => None,
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity.counts_as_error()
    }

    /// The `safe` fix (at most one).
    pub fn safe_fix(&self) -> Option<&Fix> {
        self.fixes
            .iter()
            .find(|f| f.applicability == Applicability::Safe)
    }

    /// Ordering key (chapter 3): file, byte offset of the start of `primary`, stage, code.
    fn sort_key(&self) -> (u32, u32, Stage, Option<Code>) {
        match &self.primary {
            Some(l) => (
                l.span.file.0 + 1,
                l.span.range.start().into(),
                self.stage,
                self.code,
            ),
            None => (0, 0, self.stage, self.code),
        }
    }
}

/// Collects diagnostics. Within one stage, two `error`s at the same start position are not issued (chapter 3).
#[derive(Debug, Default)]
pub struct DiagnosticSink {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a diagnostic. In the lexing, directive, and parsing stages, it is dropped if an `error` already exists at the same start position.
    pub fn push(&mut self, diag: Diagnostic) {
        if diag.severity == Severity::Error
            && matches!(diag.stage, Stage::Lexer | Stage::Directives | Stage::Parser)
            && let Some(p) = &diag.primary
            && self.diagnostics.iter().any(|d| {
                d.severity == Severity::Error
                    && matches!(d.stage, Stage::Lexer | Stage::Directives | Stage::Parser)
                    && d.primary.as_ref().is_some_and(|q| {
                        q.span.file == p.span.file && q.span.range.start() == p.span.range.start()
                    })
            })
        {
            return;
        }
        self.diagnostics.push(diag);
    }

    /// Whether an `error` exists at the same start position (used by the parser to avoid a second error at the same position).
    pub fn has_error_at(&self, file: FileId, start: crate::source::TextSize) -> bool {
        self.diagnostics.iter().any(|d| {
            d.severity == Severity::Error
                && d.primary
                    .as_ref()
                    .is_some_and(|q| q.span.file == file && q.span.range.start() == start)
        })
    }

    /// Adds without the duplicate check (when re-sorting in post-processing).
    pub fn extend_raw(&mut self, diags: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diags);
    }

    pub fn extend(&mut self, diags: impl IntoIterator<Item = Diagnostic>) {
        for d in diags {
            self.push(d);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Diagnostic> {
        self.diagnostics.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Keeps only the first `n` diagnostics (to discard diagnostics from a trial type check).
    pub fn truncate(&mut self, n: usize) {
        self.diagnostics.truncate(n);
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }

    /// Takes the diagnostics sorted in ascending order of position (chapter 3). Ones with the same key keep their issue order.
    pub fn into_sorted(mut self) -> Vec<Diagnostic> {
        self.diagnostics.sort_by_key(|d| d.sort_key());
        self.diagnostics
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

/// Only sorts (when the list of diagnostics is already at hand).
pub fn sort_diagnostics(diags: &mut [Diagnostic]) {
    diags.sort_by_key(|d| d.sort_key());
}

/// Counts for the summary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
}

impl Summary {
    pub fn of(diags: &[Diagnostic]) -> Self {
        let mut s = Summary::default();
        for d in diags {
            match d.severity {
                Severity::Error | Severity::Fatal => s.errors += 1,
                Severity::Warning => s.warnings += 1,
                Severity::Note => s.notes += 1,
            }
        }
        s
    }
}

/// Range of a deletion edit (2.1): includes the whitespace on the same line just before the range, and if the line would contain only
/// whitespace after the deletion, deletes the whole line (including its line break).
pub fn deletion_range(bytes: &[u8], range: TextRange) -> TextRange {
    let mut start = usize::from(range.start());
    let mut end = usize::from(range.end());
    // Whitespace on the same line just before
    while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    // Whether the rest of the line is only whitespace
    let line_start = bytes[..start]
        .iter()
        .rposition(|b| matches!(b, b'\n' | b'\r'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut line_end = end;
    while line_end < bytes.len() && !matches!(bytes[line_end], b'\n' | b'\r') {
        line_end += 1;
    }
    let before_blank = bytes[line_start..start]
        .iter()
        .all(|b| matches!(b, b' ' | b'\t'));
    let after_blank = bytes[end..line_end]
        .iter()
        .all(|b| matches!(b, b' ' | b'\t'));
    if before_blank && after_blank {
        start = line_start;
        end = line_end;
        if end < bytes.len() {
            if bytes[end] == b'\r' && bytes.get(end + 1) == Some(&b'\n') {
                end += 2;
            } else {
                end += 1;
            }
        }
    }
    TextRange::new((start as u32).into(), (end as u32).into())
}

/// Wraps in `` `…` `` (identifiers and tokens in messages; chapter 3).
pub fn quote(s: &str) -> String {
    format!("`{s}`")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_rules() {
        let src = b"a\n  {$J+}\nb {$J+} c\n";
        // Whole line
        assert_eq!(
            deletion_range(src, TextRange::new(4.into(), 9.into())),
            TextRange::new(2.into(), 10.into())
        );
        // Only the preceding whitespace
        assert_eq!(
            deletion_range(src, TextRange::new(12.into(), 17.into())),
            TextRange::new(11.into(), 17.into())
        );
        // Last line (no line break)
        let src = b"x\n{$J+}";
        assert_eq!(
            deletion_range(src, TextRange::new(2.into(), 7.into())),
            TextRange::new(2.into(), 7.into())
        );
    }

    #[test]
    fn code_display_and_parse() {
        assert_eq!(Code::E(1).to_string(), "E0001");
        assert_eq!(Code::W(18).to_string(), "W0018");
        assert_eq!(Code::parse("F0002"), Some(Code::F(2)));
        assert_eq!(Code::parse("E12"), None);
        assert_eq!(Code::parse("X0001"), None);
    }

    #[test]
    fn same_position_error_is_dropped_in_early_stages() {
        let mut sink = DiagnosticSink::new();
        let span = Span::new(FileId(0), TextRange::new(5.into(), 6.into()));
        sink.push(Diagnostic::error(Code::E(3), "a", Stage::Lexer).with_primary(span, ""));
        sink.push(Diagnostic::error(Code::E(400), "b", Stage::Parser).with_primary(span, ""));
        sink.push(Diagnostic::warning(Code::W(5), "c", Stage::Parser).with_primary(span, ""));
        sink.push(Diagnostic::error(Code::E(700), "d", Stage::Sema).with_primary(span, ""));
        assert_eq!(sink.len(), 3);
    }

    #[test]
    fn sorted_by_position_then_stage_then_code() {
        let mut sink = DiagnosticSink::new();
        let at = |s: u32| Span::new(FileId(0), TextRange::new(s.into(), (s + 1).into()));
        sink.push(Diagnostic::error(Code::E(700), "x", Stage::Sema).with_primary(at(10), ""));
        sink.push(Diagnostic::warning(Code::W(1), "y", Stage::Flow).with_primary(at(2), ""));
        sink.push(Diagnostic::error(Code::E(2000), "z", Stage::Flow).with_primary(at(2), ""));
        sink.push(Diagnostic::error(Code::F(2), "f", Stage::Driver));
        let v = sink.into_sorted();
        let msgs: Vec<_> = v.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(msgs, ["f", "z", "y", "x"]);
    }
}
