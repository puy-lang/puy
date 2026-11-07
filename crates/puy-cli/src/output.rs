//! Diagnostic output (text is colored, JSON is passed through; `docs/spec/diagnostics.md`
//! chapter 10).

use std::io::Write;

use crate::args::{Color, Format, Options};
use puy_core::diagnostics::{Diagnostic, Severity, json, render};
use puy_core::fix::FixResult;
use puy_core::source::SourceMap;
use puy_core::vm::ExceptionReport;

fn use_color(opts: &Options) -> bool {
    match opts.color {
        Color::Never => false,
        Color::Always => true,
        Color::Auto => {
            std::env::var_os("NO_COLOR").is_none()
                && crate::runtime::stderr_is_terminal()
                && enable_vt()
        }
    }
}

#[cfg(windows)]
fn enable_vt() -> bool {
    // Enable virtual terminal processing on legacy Windows consoles (10.1). On failure, no color
    use std::os::windows::io::AsRawHandle;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    unsafe extern "system" {
        fn GetConsoleMode(handle: *mut std::ffi::c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: *mut std::ffi::c_void, mode: u32) -> i32;
    }
    let handle = std::io::stderr().as_raw_handle();
    let mut mode = 0u32;
    unsafe {
        if GetConsoleMode(handle, &mut mode) == 0 {
            return true; // Not a console (e.g. a pseudo-terminal): leave as is
        }
        SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}

#[cfg(not(windows))]
fn enable_vt() -> bool {
    true
}

/// Applies color (the severity on the first line, `= fix:`, and so on).
fn colorize(text: &str) -> String {
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let colored = if let Some(rest) = body.strip_prefix("error") {
            format!("\x1b[1;31merror\x1b[0m{}", bold_message(rest))
        } else if let Some(rest) = body.strip_prefix("warning") {
            format!("\x1b[1;33mwarning\x1b[0m{}", bold_message(rest))
        } else if let Some(rest) = body.strip_prefix("fatal") {
            format!("\x1b[1;31mfatal\x1b[0m{}", bold_message(rest))
        } else if let Some(rest) = body.strip_prefix("note") {
            format!("\x1b[1;36mnote\x1b[0m{}", bold_message(rest))
        } else if body.trim_start().starts_with("--> ") || body.trim_start().starts_with("::: ") {
            let idx = body.find("-->").or_else(|| body.find(":::")).unwrap();
            format!(
                "{}\x1b[1;34m{}\x1b[0m{}",
                &body[..idx],
                &body[idx..idx + 3],
                &body[idx + 3..]
            )
        } else if let Some(rest) = body.strip_prefix("help:") {
            format!("\x1b[1;36mhelp:\x1b[0m{rest}")
        } else {
            body.to_string()
        };
        out.push_str(&colored);
        if line.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn bold_message(rest: &str) -> String {
    // Bold the message in `[E0001]: message`
    match rest.find(": ") {
        Some(i) => format!("{}\x1b[1m{}\x1b[0m", &rest[..i + 2], &rest[i + 2..]),
        None => rest.to_string(),
    }
}

/// Emits compile diagnostics. `to_stdout` is for `puy check` JSON (stdout). `fix_hint` is set when
/// a file was given.
pub fn emit_diagnostics(
    diags: &[Diagnostic],
    sources: &SourceMap,
    opts: &Options,
    to_stdout: bool,
    fix_hint: bool,
) {
    match opts.format {
        Format::Json => {
            let line = json::diagnostics_json(diags, sources);
            if to_stdout {
                println!("{line}");
            } else {
                eprintln!("{line}");
            }
        }
        Format::Text => {
            let mut text = render::render_all(diags, sources);
            if fix_hint {
                let n = puy_core::fix::applicable_fix_count(diags);
                if n > 0 {
                    text.push_str(&format!(
                        "help: {n} fix{} can be applied with `puy fix`\n",
                        if n == 1 { "" } else { "es" }
                    ));
                }
            }
            text.push_str(&explain_hint(diags));
            if text.is_empty() {
                return;
            }
            let text = if use_color(opts) {
                colorize(&text)
            } else {
                text
            };
            let mut err = std::io::stderr().lock();
            let _ = err.write_all(text.as_bytes());
            let _ = err.flush();
        }
    }
}

/// Emits a diagnostic without a location (F0002, etc.) on one line.
pub fn emit_standalone(d: &Diagnostic, opts: &Options, to_stdout: bool) {
    let sources = SourceMap::new();
    match opts.format {
        Format::Json => {
            let line = json::diagnostics_json(std::slice::from_ref(d), &sources);
            if to_stdout {
                println!("{line}");
            } else {
                eprintln!("{line}");
            }
        }
        Format::Text => {
            let text = format!("{}\n", render::render_diagnostic(d, &sources));
            let text = if use_color(opts) {
                colorize(&text)
            } else {
                text
            };
            let _ = std::io::stderr().write_all(text.as_bytes());
        }
    }
}

/// Fatal error (exit code 101).
pub fn emit_fatal(code: &str, message: &str, opts: &Options) {
    let d = Diagnostic::new(
        puy_core::diagnostics::Code::parse(code).unwrap_or(puy_core::diagnostics::Code::F(1)),
        Severity::Fatal,
        message,
        puy_core::diagnostics::Stage::Driver,
    );
    emit_standalone(&d, opts, false);
}

/// The `puy explain` hint line (`diagnostics.md` 10.4): codes of emitted diagnostics that have an
/// explanation, in location order without duplicates. Empty if no code has an explanation.
fn explain_hint(diags: &[Diagnostic]) -> String {
    let mut codes: Vec<String> = Vec::new();
    for d in diags {
        if let Some(c) = d.code
            && puy_core::diagnostics::explain::has(c)
        {
            let s = c.to_string();
            if !codes.contains(&s) {
                codes.push(s);
            }
        }
    }
    match codes.len() {
        0 => String::new(),
        1 => format!(
            "help: for more information about {0}, try `puy explain {0}`\n",
            codes[0]
        ),
        n => {
            let head = codes[..n - 1].join(", ");
            format!(
                "help: for more information about {head} or {}, try `puy explain <code>`\n",
                codes[n - 1]
            )
        }
    }
}

/// Reports an uncaught exception (chapter 8).
pub fn emit_exception(report: &ExceptionReport, opts: &Options) {
    match opts.format {
        Format::Json => eprintln!("{}", report.to_json()),
        Format::Text => {
            let text = report.render();
            let text = if use_color(opts) {
                colorize(&text)
            } else {
                text
            };
            let _ = std::io::stderr().write_all(text.as_bytes());
        }
    }
}

/// Failure to flush stdout at exit (`semantics.md` 7.2).
pub fn emit_stdout_error(e: &puy_host::HostError, opts: &Options) {
    let d = Diagnostic {
        code: None,
        severity: Severity::Error,
        message: format!("failed to write to stdout: {}", e.message),
        primary: None,
        secondary: Vec::new(),
        notes: Vec::new(),
        fixes: Vec::new(),
        stage: puy_core::diagnostics::Stage::Driver,
    };
    emit_standalone(&d, opts, false);
}

/// Output of `puy fix` (10.7).
pub fn emit_fix_result(r: &FixResult, path: &str, written: bool, opts: &Options) {
    match opts.format {
        Format::Json => {
            let mut w = json::JsonWriter::new();
            w.begin_object();
            w.field_string("schema", "puy.fix/1");
            w.field_array("applied");
            for a in &r.applied {
                let mut sources = SourceMap::new();
                sources.add(puy_core::source::SourceFile::new(
                    path,
                    a.source_before.clone(),
                ));
                w.begin_object();
                w.field_string("code", &a.code);
                w.field_string("file", path);
                w.field_number("round", a.round);
                w.field_string("title", &a.title);
                w.field_array("edits");
                for e in &a.edits {
                    json::write_edit(&mut w, &sources, e);
                }
                w.end_array().end_field();
                w.end_object();
            }
            w.end_array().end_field();
            w.field_array("skipped");
            for s in &r.skipped {
                w.begin_object();
                w.field_string("code", &s.code);
                w.field_string("file", &s.file);
                w.field_number("line", s.line);
                w.field_number("column", s.column);
                w.field_string("title", &s.title);
                w.field_string("reason", "included-file");
                w.field_array("edits");
                for e in &s.edits {
                    json::write_edit(&mut w, &r.sources, e);
                }
                w.end_array().end_field();
                w.end_object();
            }
            w.end_array().end_field();
            w.field_number("rounds", r.rounds);
            w.field_bool("converged", r.converged);
            w.key("diagnostics")
                .raw(&json::diagnostics_json(&r.diagnostics, &r.sources))
                .end_field();
            w.end_object();
            println!("{}", w.finish());
        }
        Format::Text => {
            let mut text = render::render_all(&r.diagnostics, &r.sources);
            if written {
                let n = r.applied.len();
                text.push_str(&format!(
                    "fixed {path} ({n} fix{})\n",
                    if n == 1 { "" } else { "es" }
                ));
            } else {
                text.push_str("nothing to fix\n");
            }
            if !r.skipped.is_empty() {
                let n = r.skipped.len();
                let note = if n == 1 {
                    "note: 1 safe fix is in an included file and was not applied:\n".to_string()
                } else {
                    format!("note: {n} safe fixes are in included files and were not applied:\n")
                };
                text.push_str(&note);
                for s in &r.skipped {
                    text.push_str(&format!(
                        "  {}:{}:{}: {}\n",
                        s.file, s.line, s.column, s.title
                    ));
                }
            }
            if !r.converged {
                text.push_str("note: stopped after 10 rounds; run `puy fix` again to continue\n");
            }
            text.push_str(&explain_hint(&r.diagnostics));
            let text = if use_color(opts) {
                colorize(&text)
            } else {
                text
            };
            let _ = std::io::stderr().write_all(text.as_bytes());
        }
    }
}
