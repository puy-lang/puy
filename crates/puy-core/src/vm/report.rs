//! Reporting uncaught exceptions and leaks (`docs/spec/diagnostics.md` chapter 8).

use crate::diagnostics::json::JsonWriter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrame {
    pub function: String,
    pub file: String,
    pub line: u32,
}

/// A property of an exception class shown in the JSON report (`docs/spec/exceptions.md` 2, 7.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropValue {
    Str(String),
    Int(i64),
}

/// An uncaught exception, or one attached to it (`Suppressed`, `InnerException`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionReport {
    pub class: String,
    pub message: String,
    /// Innermost first. At most 32 (16 innermost + 16 outermost).
    pub stack: Vec<StackFrame>,
    /// Number of omitted frames (between the 16 innermost and the 16 outermost).
    pub omitted: usize,
    /// The class's properties (`EConvertError.Value`, `EInOutError.ErrorCode`).
    pub properties: Vec<(String, PropValue)>,
    /// The exceptions in `Suppressed`, in order.
    pub suppressed: Vec<ExceptionReport>,
    /// `InnerException`.
    pub inner: Option<Box<ExceptionReport>>,
    /// Attached exceptions deeper than the limit were left out (`docs/spec/exceptions.md` 7.1).
    pub more_nested: bool,
}

/// The number of levels of attached exceptions a report shows (`docs/spec/exceptions.md` 7.1).
pub const MAX_NESTING: usize = 8;

impl ExceptionReport {
    /// Text (including the trailing newline).
    pub fn render(&self) -> String {
        let mut out = String::new();
        push_message_line(&mut out, "", "error: uncaught exception ", self);
        self.render_body(&mut out, "");
        out
    }

    /// Text of an exception that left a `finally` block while `Halt` was unwinding (including the
    /// trailing newline; `docs/spec/exceptions.md` 7.1).
    pub fn render_halt(&self) -> String {
        let mut out = String::new();
        push_message_line(&mut out, "", "warning: exception during Halt: ", self);
        self.render_body(&mut out, "");
        out
    }

    /// The stack trace and the attached exceptions, each line starting with `indent` and two spaces.
    fn render_body(&self, out: &mut String, indent: &str) {
        for line in trace_lines(&self.stack, self.omitted) {
            out.push_str(indent);
            out.push_str(&line);
            out.push('\n');
        }
        let deeper = format!("{indent}  ");
        for s in &self.suppressed {
            push_message_line(out, &deeper, "suppressed: ", s);
            s.render_body(out, &deeper);
        }
        if let Some(i) = &self.inner {
            push_message_line(out, &deeper, "caused by: ", i);
            i.render_body(out, &deeper);
        }
        if self.more_nested {
            out.push_str(&deeper);
            out.push_str("... (more nested exceptions omitted)\n");
        }
    }

    /// JSON (`puy.exception/1`; one line, no trailing newline).
    pub fn to_json(&self) -> String {
        let mut w = JsonWriter::new();
        w.begin_object();
        w.field_string("schema", "puy.exception/1");
        self.json_fields(&mut w);
        w.end_object();
        w.finish()
    }

    /// JSON of an exception that left a `finally` block during `Halt`: `puy.exception/1` with
    /// `"halt": true` (one line, no trailing newline).
    pub fn to_halt_json(&self) -> String {
        let mut w = JsonWriter::new();
        w.begin_object();
        w.field_string("schema", "puy.exception/1");
        w.field_bool("halt", true);
        self.json_fields(&mut w);
        w.end_object();
        w.finish()
    }

    fn json_fields(&self, w: &mut JsonWriter) {
        w.field_string("class", &self.class);
        w.field_string("message", &self.message);
        w.field_array("stack");
        for f in &self.stack {
            w.begin_object();
            w.field_string("function", &f.function);
            w.field_string("file", &f.file);
            w.field_number("line", f.line);
            w.end_object();
        }
        w.end_array().end_field();
        w.field_number("omitted", self.omitted);
        w.field_object("properties");
        for (k, v) in &self.properties {
            match v {
                PropValue::Str(s) => w.field_string(k, s),
                PropValue::Int(n) => w.field_number(k, n),
            };
        }
        w.end_object().end_field();
        w.field_array("suppressed");
        for s in &self.suppressed {
            w.begin_object();
            s.json_fields(w);
            w.end_object();
        }
        w.end_array().end_field();
        match &self.inner {
            Some(i) => {
                w.field_object("inner");
                i.json_fields(w);
                w.end_object().end_field();
            }
            None => {
                w.field_null("inner");
            }
        }
    }
}

/// The `  at …` lines of a stack trace, with the `  ... N more frames` line between the 16
/// innermost and the 16 outermost when frames were omitted (`docs/spec/diagnostics.md` 8).
pub fn trace_lines(stack: &[StackFrame], omitted: usize) -> Vec<String> {
    let inner = if omitted > 0 { 16 } else { stack.len() };
    let mut lines = Vec::with_capacity(stack.len() + 1);
    for (i, f) in stack.iter().enumerate() {
        if omitted > 0 && i == inner {
            lines.push(format!(
                "  ... {omitted} more frame{}",
                if omitted == 1 { "" } else { "s" }
            ));
        }
        lines.push(format!("  at {} ({}:{})", f.function, f.file, f.line));
    }
    lines
}

/// `indent` + `head` + `Class: Message`, with the lines after the first of a multi-line message
/// indented like the line (`docs/spec/exceptions.md` 7.1).
fn push_message_line(out: &mut String, indent: &str, head: &str, e: &ExceptionReport) {
    out.push_str(indent);
    out.push_str(head);
    out.push_str(&e.class);
    out.push_str(": ");
    let mut lines = e.message.split('\n');
    out.push_str(lines.next().unwrap_or(""));
    for l in lines {
        out.push('\n');
        out.push_str(indent);
        out.push_str(l);
    }
    out.push('\n');
}

/// Total number of leaked blocks.
fn leaked_blocks(leaks: &[(String, u64)]) -> u64 {
    leaks.iter().map(|(_, n)| n).sum()
}

/// The leak report text (`docs/spec/diagnostics.md` 8.1; including the trailing newline). Empty when nothing leaked.
/// `leaks` is `ExitStatus::leaks`: (type name, count) in byte order of the name.
pub fn render_leaks(leaks: &[(String, u64)]) -> String {
    let total = leaked_blocks(leaks);
    if total == 0 {
        return String::new();
    }
    let mut out = if total == 1 {
        "warning: 1 heap block was not freed\n".to_string()
    } else {
        format!("warning: {total} heap blocks were not freed\n")
    };
    for (name, n) in leaks {
        out.push_str(&format!("  {name}: {n}\n"));
    }
    out
}

/// The leak report JSON (`puy.leaks/1`; one line, no trailing newline).
pub fn leaks_json(leaks: &[(String, u64)]) -> String {
    let mut w = JsonWriter::new();
    w.begin_object();
    w.field_string("schema", "puy.leaks/1");
    w.field_number("blocks", leaked_blocks(leaks));
    w.field_array("types");
    for (name, n) in leaks {
        w.begin_object();
        w.field_string("type", name);
        w.field_number("count", *n);
        w.end_object();
    }
    w.end_array().end_field();
    w.end_object();
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_text_and_json() {
        let r = ExceptionReport {
            class: "ERangeError".into(),
            message: "string index 5 is out of range 1..3".into(),
            stack: vec![
                StackFrame {
                    function: "Hello.Second".into(),
                    file: "src/Hello.pas".into(),
                    line: 9,
                },
                StackFrame {
                    function: "Hello.<main>".into(),
                    file: "src/Hello.pas".into(),
                    line: 14,
                },
            ],
            omitted: 0,
            properties: Vec::new(),
            suppressed: Vec::new(),
            inner: None,
            more_nested: false,
        };
        assert_eq!(
            r.render(),
            "error: uncaught exception ERangeError: string index 5 is out of range 1..3\n  at Hello.Second (src/Hello.pas:9)\n  at Hello.<main> (src/Hello.pas:14)\n"
        );
        assert_eq!(
            r.to_json(),
            "{\"schema\":\"puy.exception/1\",\"class\":\"ERangeError\",\"message\":\"string index 5 is out of range 1..3\",\"stack\":[{\"function\":\"Hello.Second\",\"file\":\"src/Hello.pas\",\"line\":9},{\"function\":\"Hello.<main>\",\"file\":\"src/Hello.pas\",\"line\":14}],\"omitted\":0,\"properties\":{},\"suppressed\":[],\"inner\":null}"
        );
    }

    fn frame(function: &str, line: u32) -> StackFrame {
        StackFrame {
            function: function.into(),
            file: "app.pas".into(),
            line,
        }
    }

    fn report(class: &str, message: &str, stack: Vec<StackFrame>) -> ExceptionReport {
        ExceptionReport {
            class: class.into(),
            message: message.into(),
            stack,
            omitted: 0,
            properties: Vec::new(),
            suppressed: Vec::new(),
            inner: None,
            more_nested: false,
        }
    }

    #[test]
    // Attached exceptions are listed under the trace, two columns deeper per level, suppressed ones
    // before the cause; a message's later lines take the line's indentation
    // (docs/spec/exceptions.md 7.1). Pascal cannot attach exceptions before `try` (M2-16).
    fn render_attached_exceptions() {
        let mut parse = report("EParseError", "bad input", vec![frame("App.Parse", 9)]);
        parse.more_nested = true;
        let mut top = report("EIOError", "connection reset", vec![frame("App.Close", 20)]);
        top.suppressed.push(parse);
        top.inner = Some(Box::new(report(
            "EConvertError",
            "line one\nline two",
            vec![frame("App.ReadPort", 4)],
        )));
        top.properties.push(("ErrorCode".into(), PropValue::Int(2)));
        assert_eq!(
            top.render(),
            "error: uncaught exception EIOError: connection reset
  at App.Close (app.pas:20)
  suppressed: EParseError: bad input
    at App.Parse (app.pas:9)
    ... (more nested exceptions omitted)
  caused by: EConvertError: line one
  line two
    at App.ReadPort (app.pas:4)
"
        );
        assert_eq!(
            top.to_json(),
            "{\"schema\":\"puy.exception/1\",\"class\":\"EIOError\",\"message\":\"connection reset\",\"stack\":[{\"function\":\"App.Close\",\"file\":\"app.pas\",\"line\":20}],\"omitted\":0,\"properties\":{\"ErrorCode\":2},\"suppressed\":[{\"class\":\"EParseError\",\"message\":\"bad input\",\"stack\":[{\"function\":\"App.Parse\",\"file\":\"app.pas\",\"line\":9}],\"omitted\":0,\"properties\":{},\"suppressed\":[],\"inner\":null}],\"inner\":{\"class\":\"EConvertError\",\"message\":\"line one\\nline two\",\"stack\":[{\"function\":\"App.ReadPort\",\"file\":\"app.pas\",\"line\":4}],\"omitted\":0,\"properties\":{},\"suppressed\":[],\"inner\":null}}"
        );
    }
}
