//! Reporting uncaught exceptions (`docs/spec/diagnostics.md` chapter 8).

use crate::diagnostics::json::JsonWriter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrame {
    pub function: String,
    pub file: String,
    pub line: u32,
}

/// An uncaught exception.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionReport {
    pub class: String,
    pub message: String,
    /// Innermost first. At most 32 (16 innermost + 16 outermost).
    pub stack: Vec<StackFrame>,
    /// Number of omitted frames (between the 16 innermost and the 16 outermost).
    pub omitted: usize,
}

impl ExceptionReport {
    /// Text (including the trailing newline).
    pub fn render(&self) -> String {
        let mut out = format!(
            "error: uncaught exception {}: {}\n",
            self.class, self.message
        );
        let inner = if self.omitted > 0 {
            16
        } else {
            self.stack.len()
        };
        for (i, f) in self.stack.iter().enumerate() {
            if self.omitted > 0 && i == inner {
                let n = self.omitted;
                out.push_str(&format!(
                    "  ... {n} more frame{}\n",
                    if n == 1 { "" } else { "s" }
                ));
            }
            out.push_str(&format!("  at {} ({}:{})\n", f.function, f.file, f.line));
        }
        out
    }

    /// JSON (`puy.exception/1`; one line, no trailing newline).
    pub fn to_json(&self) -> String {
        let mut w = JsonWriter::new();
        w.begin_object();
        w.field_string("schema", "puy.exception/1");
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
        w.end_object();
        w.finish()
    }
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
        };
        assert_eq!(
            r.render(),
            "error: uncaught exception ERangeError: string index 5 is out of range 1..3\n  at Hello.Second (src/Hello.pas:9)\n  at Hello.<main> (src/Hello.pas:14)\n"
        );
        assert_eq!(
            r.to_json(),
            "{\"schema\":\"puy.exception/1\",\"class\":\"ERangeError\",\"message\":\"string index 5 is out of range 1..3\",\"stack\":[{\"function\":\"Hello.Second\",\"file\":\"src/Hello.pas\",\"line\":9},{\"function\":\"Hello.<main>\",\"file\":\"src/Hello.pas\",\"line\":14}],\"omitted\":0}"
        );
    }
}
