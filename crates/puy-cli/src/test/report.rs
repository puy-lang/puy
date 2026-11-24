//! Machine-readable test reports (`docs/spec/test.md` 10 and 11).

use std::fmt::Write;

use puy_core::diagnostics::{Code, Diagnostic, Severity, Stage, json::JsonWriter};
use puy_core::vm::{ExitKind, leaks_json};

use super::snapshot::SnapshotCounts;
use super::{Outcome, TestReport, repro_command};
use crate::{args::Options, output};

const XML_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

#[derive(Default)]
pub(super) struct Counts {
    pub(super) ok: usize,
    pub(super) failed: usize,
    pub(super) errors: usize,
    pub(super) skipped: usize,
}

impl Counts {
    pub(super) fn of(reports: &[TestReport]) -> Self {
        let mut counts = Self::default();
        for r in reports {
            match r.outcome {
                Outcome::Ok => counts.ok += 1,
                Outcome::Fail => counts.failed += 1,
                Outcome::Error => counts.errors += 1,
                Outcome::Skip => counts.skipped += 1,
            }
        }
        counts
    }
}

pub(super) fn print_json(
    reports: &[TestReport],
    filtered_out: usize,
    seed: i64,
    diagnostics: Option<&str>,
    snapshots: Option<&SnapshotCounts>,
) {
    let mut w = JsonWriter::new();
    let counts = Counts::of(reports);
    w.begin_object();
    w.field_string("schema", "puy.test/1");
    w.field_number("seed", seed);
    w.field_object("summary")
        .field_number("ok", counts.ok)
        .field_number("failed", counts.failed)
        .field_number("errors", counts.errors)
        .field_number("skipped", counts.skipped)
        .field_number("filtered_out", filtered_out)
        .end_object()
        .end_field();
    if let Some(c) = snapshots {
        w.field_object("snapshots")
            .field_number("written", c.written)
            .field_number("unused", c.unused)
            .end_object()
            .end_field();
    }
    w.field_array("tests");
    for r in reports {
        json_test(&mut w, r, seed);
    }
    w.end_array().end_field();
    if let Some(diagnostics) = diagnostics {
        w.key("diagnostics").raw(diagnostics).end_field();
    }
    w.end_object();
    println!("{}", w.finish());
}

fn json_test(w: &mut JsonWriter, r: &TestReport, seed: i64) {
    let result = match r.outcome {
        Outcome::Ok => "ok",
        Outcome::Fail => "fail",
        Outcome::Error => "error",
        Outcome::Skip => "skip",
    };
    w.begin_object();
    w.field_string("name", &r.name);
    w.field_string("file", &r.location.file);
    w.field_number("line", r.location.line);
    w.field_string("result", result);
    // `[Skip]` has no execution time. A test that calls `Puy.Testing.Skip` does (M5-58).
    if r.outcome == Outcome::Skip && r.program.is_none() {
        w.field_null("duration_ms");
    } else {
        w.field_number("duration_ms", r.elapsed.as_millis());
    }
    if !r.details.is_empty() {
        w.field_string(
            "message",
            r.details.strip_suffix('\n').unwrap_or(&r.details),
        );
    }
    // A runtime skip reports its reason instead of the exception used to stop the test or its
    // leaks. A finalization exception, Halt or a timeout takes precedence and reports ERROR.
    if r.outcome != Outcome::Skip {
        json_exit(w, r);
    }
    w.field_string("stdout", &String::from_utf8_lossy(&r.stdout));
    w.field_string("stderr", &String::from_utf8_lossy(&r.stderr));
    if r.outcome.failed() {
        w.field_string("repro", &repro_command(seed, &r.name));
    }
    w.end_object();
}

fn json_exit(w: &mut JsonWriter, r: &TestReport) {
    if let Some(program) = &r.program {
        if let ExitKind::UncaughtException(exception) = &program.kind {
            w.key("exception").raw(&exception.to_json()).end_field();
        }
        if program.halted {
            w.field_number("halt_exit_code", program.code);
        }
    }
    if let Some(last) = &r.last {
        if !last.fini_exceptions.is_empty() {
            w.field_array("fini_exceptions");
            for exception in &last.fini_exceptions {
                w.raw(&exception.to_json());
            }
            w.end_array().end_field();
        }
        if !last.leaks.is_empty() {
            w.key("leaks").raw(&leaks_json(&last.leaks)).end_field();
        }
    }
    let mut halt_exceptions = [&r.program, &r.last]
        .into_iter()
        .flatten()
        .flat_map(|status| &status.halt_exceptions)
        .peekable();
    if halt_exceptions.peek().is_some() {
        w.field_array("halt_exceptions");
        for exception in halt_exceptions {
            w.raw(&exception.to_halt_json());
        }
        w.end_array().end_field();
    }
}

pub(super) fn junit_xml(reports: &[TestReport]) -> String {
    let mut xml = String::from(XML_HEADER);
    writeln!(xml, "<testsuites {}>", junit_totals(reports)).unwrap();
    // The test table (and the filtered list) keeps each unit's tests together.
    for suite in reports.chunk_by(|a, b| a.location.unit == b.location.unit) {
        let unit = &suite[0].location.unit;
        writeln!(
            xml,
            "  <testsuite name=\"{}\" {}>",
            xml_escape(unit, true),
            junit_totals(suite),
        )
        .unwrap();
        for r in suite {
            junit_test(&mut xml, r, unit);
        }
        xml.push_str("  </testsuite>\n");
    }
    xml.push_str("</testsuites>\n");
    xml
}

/// A single error for a failed compilation, so CI can show its diagnostics as well.
pub(super) fn junit_compile_error(diagnostics: &str) -> String {
    let totals = "tests=\"1\" failures=\"0\" errors=\"1\" skipped=\"0\" time=\"0.000\"";
    let diagnostics = xml_escape(diagnostics, false);
    format!(
        "{XML_HEADER}<testsuites {totals}>\n  \
         <testsuite name=\"puy compile\" {totals}>\n    \
         <testcase classname=\"puy compile\" name=\"compile\" time=\"0.000\">\n      \
         <error message=\"compilation failed\">{diagnostics}</error>\n    \
         </testcase>\n  </testsuite>\n</testsuites>\n"
    )
}

fn junit_totals(reports: &[TestReport]) -> String {
    let counts = Counts::of(reports);
    let elapsed = reports.iter().map(|r| r.elapsed.as_millis()).sum();
    format!(
        "tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{}\"",
        reports.len(),
        counts.failed,
        counts.errors,
        counts.skipped,
        seconds(elapsed)
    )
}

fn junit_test(xml: &mut String, r: &TestReport, unit: &str) {
    let name = r
        .name
        .strip_prefix(unit)
        .and_then(|s| s.strip_prefix('.'))
        .unwrap_or(&r.name);
    writeln!(
        xml,
        "    <testcase classname=\"{}\" name=\"{}\" file=\"{}\" line=\"{}\" time=\"{}\">",
        xml_escape(unit, true),
        xml_escape(name, true),
        xml_escape(&r.location.file, true),
        r.location.line,
        seconds(r.elapsed.as_millis()),
    )
    .unwrap();
    match r.outcome {
        Outcome::Ok => {}
        Outcome::Fail | Outcome::Error => {
            let tag = if r.outcome == Outcome::Fail {
                "failure"
            } else {
                "error"
            };
            let message = r.details.lines().next().unwrap_or_default();
            writeln!(
                xml,
                "      <{tag} message=\"{}\">{}</{tag}>",
                xml_escape(message, true),
                xml_escape(&r.details, false),
            )
            .unwrap();
        }
        Outcome::Skip => {
            writeln!(
                xml,
                "      <skipped message=\"{}\"/>",
                xml_escape(&r.details, true)
            )
            .unwrap();
        }
    }
    for (tag, bytes) in [("system-out", &r.stdout), ("system-err", &r.stderr)] {
        if !bytes.is_empty() {
            let body = xml_escape(&String::from_utf8_lossy(bytes), false);
            writeln!(xml, "      <{tag}>{body}</{tag}>").unwrap();
        }
    }
    xml.push_str("    </testcase>\n");
}

fn seconds(ms: u128) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// Quotes markup and replaces scalars disallowed by XML 1.0. Numeric references preserve
/// whitespace that an XML parser would otherwise normalize.
fn xml_escape(text: &str, attribute: bool) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' if attribute => escaped.push_str("&quot;"),
            '\'' if attribute => escaped.push_str("&apos;"),
            '\r' => escaped.push_str("&#xD;"),
            '\n' if attribute => escaped.push_str("&#xA;"),
            '\t' if attribute => escaped.push_str("&#x9;"),
            '\n' | '\t' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'.. => {
                escaped.push(c);
            }
            _ => escaped.push('\u{fffd}'),
        }
    }
    escaped
}

pub(super) fn write_junit(path: &str, xml: &str, opts: &Options) -> bool {
    if let Err(e) = std::fs::write(path, xml) {
        let e = puy_host::HostError::from_io(&e);
        let diagnostic = Diagnostic::new(
            Code::F(2),
            Severity::Error,
            format!("cannot write `{path}`: {}", e.fixed_text()),
            Stage::Driver,
        );
        output::emit_standalone(&diagnostic, opts, false);
        return false;
    }
    true
}
