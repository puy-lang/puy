//! JSON output of diagnostics (`docs/spec/diagnostics.md` chapter 7). Each object is written as a single line without line breaks.

use super::{Diagnostic, Label, Summary, TextEdit};
use crate::source::{SourceMap, TextRange};

/// Escapes a JSON string (including the surrounding `"`).
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A small JSON builder.
pub struct JsonWriter {
    buf: String,
    /// For each nesting level, whether the next item is the first.
    first: Vec<bool>,
}

impl Default for JsonWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonWriter {
    pub fn new() -> Self {
        JsonWriter {
            buf: String::new(),
            first: Vec::new(),
        }
    }

    fn sep(&mut self) {
        if let Some(f) = self.first.last_mut() {
            if !*f {
                self.buf.push(',');
            }
            *f = false;
        }
    }

    pub fn begin_object(&mut self) -> &mut Self {
        self.sep();
        self.buf.push('{');
        self.first.push(true);
        self
    }

    pub fn end_object(&mut self) -> &mut Self {
        self.first.pop();
        self.buf.push('}');
        self
    }

    pub fn begin_array(&mut self) -> &mut Self {
        self.sep();
        self.buf.push('[');
        self.first.push(true);
        self
    }

    pub fn end_array(&mut self) -> &mut Self {
        self.first.pop();
        self.buf.push(']');
        self
    }

    pub fn key(&mut self, k: &str) -> &mut Self {
        self.sep();
        self.buf.push_str(&json_string(k));
        self.buf.push(':');
        // Temporarily mark as "first" so no separator is written before the value
        self.first.push(true);
        self
    }

    fn end_value(&mut self) {
        self.first.pop();
    }

    pub fn string(&mut self, s: &str) -> &mut Self {
        self.sep();
        self.buf.push_str(&json_string(s));
        self
    }

    pub fn raw(&mut self, s: &str) -> &mut Self {
        self.sep();
        self.buf.push_str(s);
        self
    }

    pub fn null(&mut self) -> &mut Self {
        self.raw("null")
    }

    pub fn number(&mut self, n: impl std::fmt::Display) -> &mut Self {
        let s = n.to_string();
        self.raw(&s)
    }

    pub fn bool(&mut self, b: bool) -> &mut Self {
        self.raw(if b { "true" } else { "false" })
    }

    pub fn field_string(&mut self, k: &str, v: &str) -> &mut Self {
        self.key(k).string(v);
        self.end_value();
        self
    }

    pub fn field_opt_string(&mut self, k: &str, v: Option<&str>) -> &mut Self {
        self.key(k);
        match v {
            Some(v) => self.string(v),
            None => self.null(),
        };
        self.end_value();
        self
    }

    pub fn field_number(&mut self, k: &str, n: impl std::fmt::Display) -> &mut Self {
        self.key(k).number(n);
        self.end_value();
        self
    }

    pub fn field_bool(&mut self, k: &str, b: bool) -> &mut Self {
        self.key(k).bool(b);
        self.end_value();
        self
    }

    pub fn field_null(&mut self, k: &str) -> &mut Self {
        self.key(k).null();
        self.end_value();
        self
    }

    /// Starts `key: {`. Call `end_field` after `end_object`.
    pub fn field_object(&mut self, k: &str) -> &mut Self {
        self.key(k).begin_object();
        self
    }

    pub fn field_array(&mut self, k: &str) -> &mut Self {
        self.key(k).begin_array();
        self
    }

    pub fn end_field(&mut self) -> &mut Self {
        self.end_value();
        self
    }

    pub fn finish(self) -> String {
        self.buf
    }
}

/// Location fields (`file`, `start` / `end`, `line` / `column` / `end_line` / `end_column`).
fn write_position(
    w: &mut JsonWriter,
    sources: &SourceMap,
    file: crate::source::FileId,
    range: TextRange,
) {
    let f = sources.get(file);
    let start = f.line_col(range.start());
    let end = f.line_col(range.end());
    w.field_string("file", &f.path);
    w.field_number("start", u32::from(range.start()));
    w.field_number("end", u32::from(range.end()));
    w.field_number("line", start.line);
    w.field_number("column", start.column);
    w.field_number("end_line", end.line);
    w.field_number("end_column", end.column);
}

fn write_label(w: &mut JsonWriter, sources: &SourceMap, l: &Label) {
    w.begin_object();
    write_position(w, sources, l.span.file, l.span.range);
    w.field_string("label", l.label);
    w.end_object();
}

/// JSON of an edit (`original` is looked up from the source).
pub fn write_edit(w: &mut JsonWriter, sources: &SourceMap, e: &TextEdit) {
    w.begin_object();
    write_position(w, sources, e.file, e.range);
    let f = sources.get(e.file);
    let original = String::from_utf8_lossy(f.text(e.range)).into_owned();
    w.field_string("original", &original);
    w.field_string("replacement", &e.replacement);
    w.end_object();
}

/// The object for one diagnostic (including `rendered`).
pub fn write_diagnostic(w: &mut JsonWriter, d: &Diagnostic, sources: &SourceMap) {
    w.begin_object();
    w.field_opt_string("code", d.code.map(|c| c.to_string()).as_deref());
    w.field_string("severity", d.severity.as_str());
    w.field_opt_string("name", d.name());
    w.field_string("message", &d.message);
    match &d.primary {
        Some(p) => {
            w.key("primary");
            write_label(w, sources, p);
            w.end_field();
        }
        None => {
            w.field_null("primary");
        }
    }
    w.field_array("secondary");
    for s in &d.secondary {
        write_label(w, sources, s);
    }
    w.end_array().end_field();
    w.field_array("notes");
    for n in &d.notes {
        w.string(n);
    }
    w.end_array().end_field();
    w.field_array("fixes");
    for f in &d.fixes {
        w.begin_object();
        w.field_string("title", &f.title);
        w.field_string("applicability", f.applicability.as_str());
        w.field_array("edits");
        for e in &f.edits {
            write_edit(w, sources, e);
        }
        w.end_array().end_field();
        w.end_object();
    }
    w.end_array().end_field();
    w.field_string("rendered", &super::render::render_diagnostic(d, sources));
    w.end_object();
}

/// The `puy.diagnostics/1` object (one line, without a trailing newline).
pub fn diagnostics_json(diags: &[Diagnostic], sources: &SourceMap) -> String {
    let mut w = JsonWriter::new();
    w.begin_object();
    w.field_string("schema", "puy.diagnostics/1");
    w.field_array("diagnostics");
    for d in diags {
        write_diagnostic(&mut w, d, sources);
    }
    w.end_array().end_field();
    let s = Summary::of(diags);
    w.field_object("summary");
    w.field_number("errors", s.errors);
    w.field_number("warnings", s.warnings);
    w.field_number("notes", s.notes);
    w.end_object().end_field();
    w.end_object();
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{Code, Fix, Stage};
    use crate::source::{FileId, SourceFile, Span};

    #[test]
    fn escape() {
        assert_eq!(json_string("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
    }

    #[test]
    fn diagnostics_object() {
        let mut m = SourceMap::new();
        m.add(SourceFile::new("t.pas", b"var S: string;\n".to_vec()));
        let d = Diagnostic::error(Code::E(2000), "m", Stage::Flow)
            .with_primary(
                Span::new(FileId(0), TextRange::new(4.into(), 5.into())),
                "read here",
            )
            .with_fix(Fix::safe(
                "t",
                vec![TextEdit::replace(
                    FileId(0),
                    TextRange::new(4.into(), 13.into()),
                    "S: string = ''",
                )],
            ));
        let j = diagnostics_json(&[d], &m);
        assert!(!j.contains('\n'));
        assert!(j.starts_with("{\"schema\":\"puy.diagnostics/1\",\"diagnostics\":[{\"code\":\"E2000\",\"severity\":\"error\",\"name\":null,\"message\":\"m\",\"primary\":{\"file\":\"t.pas\",\"start\":4,\"end\":5,\"line\":1,\"column\":5,\"end_line\":1,\"end_column\":6,\"label\":\"read here\"},\"secondary\":[],\"notes\":[],\"fixes\":[{\"title\":\"t\",\"applicability\":\"safe\",\"edits\":[{\"file\":\"t.pas\",\"start\":4,\"end\":13,\"line\":1,\"column\":5,\"end_line\":1,\"end_column\":14,\"original\":\"S: string\",\"replacement\":\"S: string = ''\"}]}],\"rendered\":\""), "{j}");
        assert!(j.ends_with("\"summary\":{\"errors\":1,\"warnings\":0,\"notes\":0}}"));
        let empty = diagnostics_json(&[], &m);
        assert_eq!(
            empty,
            "{\"schema\":\"puy.diagnostics/1\",\"diagnostics\":[],\"summary\":{\"errors\":0,\"warnings\":0,\"notes\":0}}"
        );
    }
}
