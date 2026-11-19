//! Checks that every label and note in the expectations under `tests/` is in the registry of
//! `docs/spec/diagnostics.md`: labels in the table of 2.2, notes in the table of 4.10 (and the
//! `{$WARN}` note of chapter 5 for warnings). Only this direction is checked: a table entry
//! without a case is not an error.
//!
//! The spec is read at run time because the public repository does not ship `docs/`; there the
//! check is skipped.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The inline code spans of a Markdown line (CommonMark: a run of backticks is closed by a run of
/// the same length, and one space on each side is stripped): (byte range of the whole span,
/// contents).
fn spans(line: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let b = line.as_bytes();
    let run = |mut i: usize| {
        let start = i;
        while i < b.len() && b[i] == b'`' {
            i += 1;
        }
        i - start
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'`' {
            i += 1;
            continue;
        }
        let n = run(i);
        let open_end = i + n;
        let mut k = open_end;
        let mut close = None;
        while k < b.len() {
            if b[k] == b'`' {
                let m = run(k);
                if m == n {
                    close = Some(k);
                    break;
                }
                k += m;
            } else {
                k += 1;
            }
        }
        let Some(close) = close else {
            i = open_end;
            continue;
        };
        let mut c = &line[open_end..close];
        if c.len() >= 2 && c.starts_with(' ') && c.ends_with(' ') && !c.trim().is_empty() {
            c = &c[1..c.len() - 1];
        }
        out.push((i..close + n, c.to_string()));
        i = close + n;
    }
    out
}

/// The contents of the inline code spans of a Markdown line.
fn code_spans(line: &str) -> Vec<String> {
    spans(line).into_iter().map(|(_, c)| c).collect()
}

/// `cell` without its remarks: the text in full-width parentheses (U+FF08 … U+FF09) outside
/// code spans, counting nested ones.
fn strip_remarks(cell: &str) -> String {
    let spans = spans(cell);
    let mut out = String::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < cell.len() {
        if let Some((r, _)) = spans.iter().find(|(r, _)| r.start == i) {
            if depth == 0 {
                out.push_str(&cell[r.clone()]);
            }
            i = r.end;
            continue;
        }
        let ch = cell[i..].chars().next().expect("inside the cell");
        match ch {
            '（' => depth += 1,
            '）' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
        i += ch.len_utf8();
    }
    out
}

/// The cells of the table rows (excluding the header and the separator) in the section whose
/// heading starts with `heading`.
fn table_rows(spec: &str, heading: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut on = false;
    let mut header = true;
    for line in spec.lines() {
        if line.starts_with('#') {
            on = line.starts_with(heading);
            continue;
        }
        if !on || !line.starts_with("| ") {
            continue;
        }
        if line.starts_with("|---") {
            continue;
        }
        if header {
            header = false;
            continue;
        }
        let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
        rows.push(inner.split(" | ").map(|c| c.trim().to_string()).collect());
    }
    rows
}

/// Whether `text` matches `template`, where each `…` stands for any text.
fn matches(template: &str, text: &str) -> bool {
    let parts: Vec<&str> = template.split('…').collect();
    if parts.len() == 1 {
        return template == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text.ends_with(last) {
        return false;
    }
    let end = text.len() - last.len();
    let mut pos = first.len();
    if pos > end {
        return false;
    }
    for p in &parts[1..parts.len() - 1] {
        match text[pos..end].find(p) {
            Some(k) => pos += k + p.len(),
            None => return false,
        }
    }
    true
}

/// A label or a note found in an expectation, with the code of its diagnostic.
#[derive(Default)]
struct Found {
    /// label → (code, file)
    labels: BTreeMap<String, BTreeSet<(String, String)>>,
    /// (code, warning name, note) → file
    notes: BTreeMap<(String, String, String), String>,
}

impl Found {
    fn label(&mut self, code: &str, label: &str, file: &str) {
        self.labels
            .entry(label.to_string())
            .or_default()
            .insert((code.to_string(), file.to_string()));
    }

    fn note(&mut self, code: &str, name: &str, note: &str, file: &str) {
        self.notes
            .entry((code.to_string(), name.to_string(), note.to_string()))
            .or_insert_with(|| file.to_string());
    }

    /// The text rendering (`docs/spec/diagnostics.md` chapter 6).
    fn scan_text(&mut self, text: &str, file: &str) {
        let mut cur: Option<(String, String)> = None;
        for line in text.lines() {
            if let Some((sev, rest)) = line.split_once('[')
                && matches!(sev, "error" | "warning" | "note" | "fatal")
                && let Some((code, msg)) = rest.split_once("]: ")
            {
                let name = msg
                    .strip_suffix(')')
                    .and_then(|m| m.rsplit_once(" ("))
                    .map(|(_, n)| n.to_string())
                    .unwrap_or_default();
                cur = Some((code.to_string(), name));
                continue;
            }
            let Some((code, name)) = &cur else { continue };
            let trimmed = line.trim_start();
            if let Some(note) = trimmed.strip_prefix("= note: ") {
                self.note(code, name, note, file);
                continue;
            }
            // An underline line: an empty gutter, `|`, then a run of `^` or `-`
            let Some(rest) = trimmed.strip_prefix('|') else {
                continue;
            };
            if !line.starts_with(' ') {
                continue;
            }
            let rest = rest.trim_start();
            let Some(first) = rest.chars().next() else {
                continue;
            };
            if first != '^' && first != '-' {
                continue;
            }
            let after = rest.trim_start_matches(first);
            if after.is_empty() {
                self.label(code, "", file);
            } else if let Some(label) = after.strip_prefix(' ') {
                self.label(code, label, file);
            }
        }
    }

    /// The JSON output (`docs/spec/diagnostics.md` chapter 7): every object with a `code`.
    fn scan_json(&mut self, v: &serde_json::Value, file: &str) {
        match v {
            serde_json::Value::Object(o) => {
                if let Some(code) = o.get("code").and_then(|c| c.as_str()) {
                    let name = o.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let spans = o.get("primary").into_iter().chain(
                        o.get("secondary")
                            .and_then(|s| s.as_array())
                            .into_iter()
                            .flatten(),
                    );
                    for span in spans {
                        if let Some(label) = span.get("label").and_then(|l| l.as_str()) {
                            self.label(code, label, file);
                        }
                    }
                    for note in o
                        .get("notes")
                        .and_then(|n| n.as_array())
                        .into_iter()
                        .flatten()
                    {
                        if let Some(note) = note.as_str() {
                            self.note(code, name, note, file);
                        }
                    }
                }
                for v in o.values() {
                    self.scan_json(v, file);
                }
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    self.scan_json(v, file);
                }
            }
            _ => {}
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("tests/ is readable")
        .map(|e| e.expect("directory entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn found_in_tests() -> Found {
    let tests = root().join("tests");
    let mut files = Vec::new();
    walk(&tests, &mut files);
    let mut found = Found::default();
    for p in files {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "diag" | "err" | "json") {
            continue;
        }
        let name = p
            .strip_prefix(&tests)
            .expect("under tests/")
            .display()
            .to_string();
        let bytes = std::fs::read(&p).expect("expectation is readable");
        let text = String::from_utf8_lossy(&bytes);
        if ext == "json" {
            // A file holds one JSON value, or one per line
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => found.scan_json(&v, &name),
                Err(_) => {
                    for line in text.lines() {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                            found.scan_json(&v, &name);
                        }
                    }
                }
            }
        } else {
            found.scan_text(&text, &name);
        }
    }
    found
}

/// The spec, or `None` in the public repository, which ships no `docs/`.
fn read_spec() -> Option<String> {
    let docs = root().join("docs");
    if !docs.exists() {
        return None;
    }
    Some(
        std::fs::read_to_string(docs.join("spec/diagnostics.md"))
            .expect("docs/spec/diagnostics.md is readable"),
    )
}

#[test]
fn code_spans_follow_commonmark() {
    // Templates with backticks inside are written with a longer run of backticks.
    assert_eq!(
        code_spans("`a` x `` b `c` `` y ``d``"),
        vec!["a", "b `c`", "d"]
    );
    assert_eq!(
        code_spans("`` `Self` captured here ``"),
        vec!["`Self` captured here"]
    );
    assert_eq!(code_spans("`unclosed"), Vec::<String>::new());
}

#[test]
fn remarks_are_not_templates() {
    // Text in full-width parentheses outside code spans is a remark, nested or not.
    assert_eq!(
        strip_remarks("`a` (1)（`b`（`c`）d）`` x（y） ``"),
        "`a` (1)`` x（y） ``"
    );
}

#[test]
fn template_holes_match_any_text() {
    // `…` stands for any text; the rest must match exactly.
    assert!(matches("remove `…` and its", "remove `X` and its"));
    assert!(matches(
        "include chain: … -> …",
        "include chain: a -> b -> c"
    ));
    assert!(!matches("add `…` to `…`", "add `A` to `B` now"));
    assert!(!matches("exact", "exact "));
}

#[test]
fn labels_in_expectations_are_in_the_spec() {
    // Every label in the expectations is a label of the table in 2.2 (or empty).
    let Some(spec) = read_spec() else { return };
    let mut table: BTreeSet<String> = BTreeSet::from([String::new()]);
    for row in table_rows(&spec, "### 2.2") {
        table.extend(code_spans(&row[0]));
    }
    let found = found_in_tests();
    assert!(
        found.labels.contains_key("declared here"),
        "no labels found"
    );
    let missing: Vec<String> = found
        .labels
        .iter()
        .filter(|(label, _)| !table.contains(*label))
        .map(|(label, at)| format!("{label:?} {at:?}"))
        .collect();
    assert!(
        missing.is_empty(),
        "labels missing from 2.2:\n{}",
        missing.join("\n")
    );
}

#[test]
fn notes_in_expectations_are_in_the_spec() {
    // Every note in the expectations matches a template of its code in the table in 4.10, or is
    // the `{$WARN}` note of chapter 5 for a warning.
    let Some(spec) = read_spec() else { return };
    let mut table: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut code = String::new();
    for row in table_rows(&spec, "### 4.10") {
        if !row[0].is_empty() {
            code = row[0].clone();
        }
        // Only the notes column, without its remarks; a template must have text besides holes
        let cell = strip_remarks(row.last().expect("a row has cells"));
        table.entry(code.clone()).or_default().extend(
            code_spans(&cell)
                .into_iter()
                .filter(|t| !t.replace('…', "").trim().is_empty()),
        );
    }
    let found = found_in_tests();
    assert!(!found.notes.is_empty(), "no notes found");
    let mut missing = Vec::new();
    for ((code, name, note), file) in &found.notes {
        if code.starts_with('W') && *note == format!("disable with `{{$WARN {name} OFF}}`") {
            continue;
        }
        let ok = table
            .get(code)
            .is_some_and(|ts| ts.iter().any(|t| matches(t, note)));
        if !ok {
            missing.push(format!("{code} {note:?} ({file})"));
        }
    }
    assert!(
        missing.is_empty(),
        "notes missing from 4.10:\n{}",
        missing.join("\n")
    );
}
