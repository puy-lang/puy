//! Running and comparing cases (`docs/design/testing.md` 3.2-3.6). Failures are reported as `Err` (carrying the diff).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use puy_core::bytecode::Module;
use puy_core::diagnostics::{json, render};
use puy_core::directives::BuildConfig;
use puy_core::driver::{CompileOptions, compile};
use puy_core::source::MemorySourceProvider;
use puy_core::vm::{
    ExitKind, ExitStatus, HashSeed, Isolate, IsolateConfig, RunOutcome, StdoutBuffering,
};
use puy_host::{HostError, HostFuture, HostSet, OutStream, ready};
use puy_testsuite::{Case, CaseConfig, parse_config};

struct CaptureConsole {
    stdout: Rc<RefCell<Vec<u8>>>,
    stderr: Rc<RefCell<Vec<u8>>>,
    stdin: Rc<RefCell<Vec<u8>>>,
}

impl puy_host::Console for CaptureConsole {
    fn write(&mut self, stream: OutStream, bytes: &[u8]) -> HostFuture<()> {
        match stream {
            OutStream::Stdout => self.stdout.borrow_mut().extend_from_slice(bytes),
            OutStream::Stderr => self.stderr.borrow_mut().extend_from_slice(bytes),
        }
        ready(Ok(()))
    }

    fn read_line(&mut self) -> HostFuture<Option<String>> {
        let mut input = self.stdin.borrow_mut();
        if input.is_empty() {
            return ready(Ok(None));
        }
        let end = input
            .iter()
            .position(|b| *b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(input.len());
        let line: Vec<u8> = input.drain(..end).collect();
        let mut s = String::from_utf8_lossy(&line).into_owned();
        if s.ends_with('\n') {
            s.pop();
            if s.ends_with('\r') {
                s.pop();
            }
        }
        ready(Ok(Some(s)))
    }
}

struct FixedEnv(Vec<(String, String)>);

impl puy_host::Env for FixedEnv {
    fn get(&mut self, name: &str) -> Result<Option<String>, HostError> {
        Ok(self
            .0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone()))
    }
}

/// Runs a case and compares it against the expectations.
/// Runs one case. On native, it runs on a thread with a 16MB stack, same as the `puy` command
/// (a chain of `if`s at the nesting limit of 256 exceeds 2MB in debug builds;
/// `docs/design/compiler-pipeline.md` 3.11). On wasm32, the linker's `-zstack-size` enlarges it.
pub fn run_case(case: &'static Case) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(move || run_case_inner(case))
            .map_err(|e| format!("{}: cannot spawn thread: {e}", case.name))?
            .join()
            .map_err(|_| format!("{}: the case panicked", case.name))?
    }
    #[cfg(target_arch = "wasm32")]
    {
        run_case_inner(case)
    }
}

fn run_case_inner(case: &Case) -> Result<(), String> {
    let src = case.source();
    let cfg = parse_config(src).map_err(|e| format!("{}: {e}", case.name))?;
    if cfg.native_only && cfg!(target_arch = "wasm32") {
        eprintln!("skipped (native-only): {}", case.name);
        return Ok(());
    }
    let update = std::env::var_os("PUY_UPDATE_GOLDEN").is_some_and(|v| v == "1");
    let mut failures: Vec<String> = Vec::new();
    let path = format!("{}.pas", case.stem);
    let mut provider = MemorySourceProvider::new();
    for (name, bytes) in case.includes() {
        provider = provider.with_file(name, bytes.to_vec());
    }
    let opts = CompileOptions {
        build: BuildConfig {
            release: cfg.release,
            test: false,
            defines: cfg.defines.clone(),
            ..Default::default()
        },
        extra_diagnostics: Vec::new(),
        script_name: Some(script_name(case.stem)),
        extra_files: Vec::new(),
        package: case.package.then(|| puy_core::driver::PackageRoot {
            levels: 0,
            file: Some(path.clone()),
        }),
        ..Default::default()
    };
    let out = compile(&path, src.to_vec(), &mut provider, &opts);
    if case.group == "examples" {
        return run_example(case, &cfg, &out, &path);
    }
    let has_errors = out.diagnostics.iter().any(|d| d.is_error());
    let is_diff = case.group == "diff";

    // Stage dumps
    if let Some(tokens) = &out.stages.tokens {
        let actual = puy_core::lexer::dump_tokens(
            tokens,
            &out.sources.get(puy_core::source::FileId(0)).bytes,
        );
        compare_text(
            case,
            "tokens",
            actual.as_bytes(),
            update,
            false,
            &mut failures,
        );
    }
    if let Some(cst) = &out.stages.cst {
        let actual = puy_core::parser::dump_cst(cst);
        compare_text(case, "cst", actual.as_bytes(), update, false, &mut failures);
    }
    if let Some(p) = &out.stages.hir {
        let actual =
            puy_core::sema::dump::dump_hir(p, out.sources.get(puy_core::source::FileId(0)));
        compare_text(case, "hir", actual.as_bytes(), update, false, &mut failures);
    }
    if let Some(m) = &out.stages.ir {
        let actual = puy_core::ir::dump_ir(m);
        compare_text(case, "ir", actual.as_bytes(), update, false, &mut failures);
    } else if case.file("ir").is_some() {
        failures.push("expected an .ir dump but compilation produced no IR".into());
    }
    if let Some(m) = &out.stages.module {
        let actual = puy_core::bytecode::disassemble(m);
        compare_text(
            case,
            "bytecode",
            actual.as_bytes(),
            update,
            false,
            &mut failures,
        );
    } else if case.file("bytecode").is_some() {
        failures.push("expected a .bytecode dump but compilation produced no module".into());
    }

    // Diagnostics
    let diag_text = render::render_all(&out.diagnostics, &out.sources);
    if is_diff {
        if !out.diagnostics.is_empty() {
            failures.push(format!(
                "diff/ cases must compile without diagnostics:\n{diag_text}"
            ));
        }
    } else {
        compare_text_required(case, "diag", diag_text.as_bytes(), update, &mut failures);
    }
    // JSON
    let mut json_parts: Vec<String> = Vec::new();
    if !out.diagnostics.is_empty() {
        json_parts.push(json::diagnostics_json(&out.diagnostics, &out.sources));
    }
    // Applying fix suggestions: to the program, and in a package to the units too
    // (`docs/spec/units.md` 8.3). A package always runs it, so that a unit rewritten without an
    // expectation fails without `PUY_UPDATE_GOLDEN=1` too.
    let fixed_units = case.fixed_units();
    if case.file("fixed.pas").is_some() || case.package || update {
        let mut provider2 = MemorySourceProvider::new();
        for (name, bytes) in case.includes() {
            provider2 = provider2.with_file(name, bytes.to_vec());
        }
        let r = puy_core::fix::fix(&path, src.to_vec(), &mut provider2, &opts, false);
        let fixed = puy_core::fix::normalize_newlines(r.fixed(&path).unwrap_or(src));
        if case.file("fixed.pas").is_some() || (update && r.fixed(&path).is_some()) {
            compare_text(case, "fixed.pas", &fixed, update, false, &mut failures);
        }
        for f in r.files.iter().filter(|f| f.path != path) {
            let actual = puy_core::fix::normalize_newlines(&f.bytes);
            let expected = fixed_units
                .iter()
                .find(|(p, _)| *p == f.path)
                .map(|(_, b)| *b);
            compare_unit_fixed(case, &f.path, expected, &actual, update, &mut failures);
        }
        // An expectation for a unit that `puy fix` leaves as it is
        for (unit, expected) in &fixed_units {
            if r.fixed(unit).is_none() {
                let original = case
                    .files
                    .iter()
                    .find(|(n, _)| n == unit)
                    .map(|(_, b)| puy_core::fix::normalize_newlines(b))
                    .unwrap_or_default();
                compare_unit_fixed(case, unit, Some(expected), &original, update, &mut failures);
            }
        }
    }

    // Execution
    let wants_run = is_diff
        || case.file("out").is_some()
        || case.file("code").is_some()
        || case.file("err").is_some();
    if has_errors
        && (case.file("out").is_some() || case.file("code").is_some() || case.file("err").is_some())
    {
        failures.push(
            "the case has compile errors but also expects a run (.out / .code / .err)".into(),
        );
    }
    // A unit is checked, not run (`docs/spec/units.md` 1)
    if !has_errors && !out.is_unit && (wants_run || !is_diff) {
        let module_bytes = out
            .module
            .as_deref()
            .expect("a module when there are no errors");
        {
            let (stdout, stderr, status, last) = execute(case, &cfg, module_bytes, &path);
            let code = last.exit_code();
            if is_diff || case.file("out").is_some() || update {
                if is_diff {
                    compare_text_required(case, "out", &stdout, update, &mut failures);
                } else {
                    compare_text(case, "out", &stdout, update, true, &mut failures);
                }
            } else if !stdout.is_empty() && !is_diff {
                // Output with no .out in golden means the expectation was forgotten
                failures.push(format!(
                    "stdout is not empty but there is no .out file:\n{}",
                    String::from_utf8_lossy(&stdout)
                ));
            }
            let expected_code: u8 = case
                .file("code")
                .map(|b| String::from_utf8_lossy(b).trim().parse::<u8>().unwrap_or(0))
                .unwrap_or(0);
            if code != expected_code {
                if update && !is_diff {
                    write_expected(case, "code", format!("{code}\n").as_bytes());
                } else {
                    failures.push(format!("exit code: expected {expected_code}, got {code}"));
                }
            }
            if !is_diff {
                compare_text_required(case, "err", &stderr, update, &mut failures);
            }
            if let ExitKind::UncaughtException(rep) = &status.kind {
                json_parts.push(rep.to_json());
            }
            for rep in &status.halt_exceptions {
                json_parts.push(rep.to_halt_json());
            }
            for rep in &last.fini_exceptions {
                json_parts.push(rep.to_json());
            }
            for rep in &last.halt_exceptions {
                json_parts.push(rep.to_halt_json());
            }
            compare_leaks(case, &last.leaks, update, &mut failures);
            if !last.leaks.is_empty() {
                json_parts.push(puy_core::vm::leaks_json(&last.leaks));
            }
        }
    }
    if !is_diff {
        compare_json(case, &json_parts, update, &mut failures);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("case `{}`:\n{}", case.name, failures.join("\n")))
    }
}

/// An example in `examples/` (a program or a package directory; `docs/design/testing.md` 3.11):
/// compiles without diagnostics, runs to completion with exit code 0, prints something, writes
/// nothing to stderr and frees every heap block. There are no expectation files.
fn run_example(
    case: &Case,
    cfg: &CaseConfig,
    out: &puy_core::driver::CompileOutput,
    path: &str,
) -> Result<(), String> {
    let name = case.name.strip_prefix("example_").unwrap_or(case.name);
    let fail = |what: String| Err(format!("example `{name}`: {what}"));
    if !out.diagnostics.is_empty() {
        return fail(format!(
            "examples must compile without diagnostics:\n{}",
            render::render_all(&out.diagnostics, &out.sources)
        ));
    }
    let Some(module_bytes) = out.module.as_deref() else {
        return fail("compilation produced no module".into());
    };
    let (stdout, stderr, _, last) = execute(case, cfg, module_bytes, path);
    let mut failures = Vec::new();
    if last.exit_code() != 0 {
        failures.push(format!("exit code {}", last.exit_code()));
    }
    if stdout.is_empty() {
        failures.push("nothing was written to stdout".into());
    }
    if !stderr.is_empty() {
        failures.push(format!("stderr:\n{}", String::from_utf8_lossy(&stderr)));
    }
    compare_leaks(case, &last.leaks, false, &mut failures);
    if failures.is_empty() {
        Ok(())
    } else {
        fail(failures.join("\n"))
    }
}

fn script_name(name: &str) -> String {
    let b = name.as_bytes();
    let ident = !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        && puy_core::syntax::SyntaxKind::reserved_word(&name.to_ascii_lowercase()).is_none();
    if ident {
        name.to_string()
    } else {
        "Script".to_string()
    }
}

/// Compares the leak report with `.leak` (`docs/design/lifetime.md` 3.12). Without the file no block may be left.
/// Only `lifetime/` cases may expect leaks.
fn compare_leaks(case: &Case, leaks: &[(String, u64)], update: bool, failures: &mut Vec<String>) {
    let actual = puy_core::vm::render_leaks(leaks);
    if case.group != "lifetime" {
        if case.file("leak").is_some() {
            failures.push(".leak is only allowed in lifetime/ cases".into());
        }
        if !actual.is_empty() {
            failures.push(format!("heap blocks were not freed:\n{actual}"));
        }
        return;
    }
    compare_text_required(case, "leak", actual.as_bytes(), update, failures);
}

/// Runs with a host that captures the module, including the finishing stage (`Isolate::finish`).
/// Returns (stdout, the text the runtime writes to stderr, the program's exit status, the status after the finishing stage).
fn execute(
    case: &Case,
    cfg: &CaseConfig,
    module_bytes: &[u8],
    path: &str,
) -> (Vec<u8>, Vec<u8>, ExitStatus, ExitStatus) {
    let module =
        Arc::new(Module::decode(module_bytes, puy_core::VERSION).expect("module verifies"));
    let stdout = Rc::new(RefCell::new(Vec::new()));
    let stderr = Rc::new(RefCell::new(Vec::new()));
    let stdin = Rc::new(RefCell::new(
        case.file("in").map(|b| b.to_vec()).unwrap_or_default(),
    ));
    let host = HostSet {
        console: if cfg.no_console {
            None
        } else {
            Some(Box::new(CaptureConsole {
                stdout: stdout.clone(),
                // `merge-stderr`: one capture in the order of the writes
                stderr: if cfg.merge_stderr {
                    stdout.clone()
                } else {
                    stderr.clone()
                },
                stdin,
            }))
        },
        env: if cfg.no_env {
            None
        } else {
            Some(Box::new(FixedEnv(vec![
                ("PUY_TEST_HOME".into(), "/home/puy".into()),
                ("EMPTY".into(), String::new()),
            ])))
        },
        clock: None,
        random: None,
        memory: None,
    };
    let config = IsolateConfig {
        memory_limit: cfg.memory_limit,
        stack_limit: cfg.stack_limit.unwrap_or(256 * 1024 * 1024),
        hash_seed: HashSeed::Fixed(0),
        budget_default: 10_000,
        stdout_buffering: match cfg.stdout_buffering.as_deref() {
            Some("line") => StdoutBuffering::Line,
            Some("none") => StdoutBuffering::None,
            _ => StdoutBuffering::Full,
        },
        leak_check: true,
    };
    let mut iso = Isolate::new(module, host, config).expect("isolate");
    let mut args = vec![path.to_string()];
    if let Some(a) = case.file("args") {
        args.extend(String::from_utf8_lossy(a).lines().map(|l| l.to_string()));
    }
    iso.start_main(args).expect("start");
    let run = |iso: &mut Isolate| loop {
        match iso.run(10_000) {
            RunOutcome::Finished(st) => break st,
            RunOutcome::BudgetExhausted | RunOutcome::Waiting => continue,
            RunOutcome::Fatal(e) => panic!("fatal: {}", e.message),
        }
    };
    let status = run(&mut iso);
    iso.finish();
    let last = run(&mut iso);
    // What the program wrote to stderr (`ErrOutput`), then the reports, built the same way as the
    // runtime loop
    let mut err_text: Vec<u8> = stderr.borrow().clone();
    if let ExitKind::UncaughtException(rep) = &status.kind {
        err_text.extend_from_slice(rep.render().as_bytes());
    }
    for rep in &status.halt_exceptions {
        err_text.extend_from_slice(rep.render_halt().as_bytes());
    }
    for rep in &last.fini_exceptions {
        err_text.extend_from_slice(rep.render().as_bytes());
    }
    for rep in &last.halt_exceptions {
        err_text.extend_from_slice(rep.render_halt().as_bytes());
    }
    if let Some(e) = status.stdout_error.as_ref().or(last.stdout_error.as_ref()) {
        err_text.extend_from_slice(
            format!("error: failed to write to stdout: {}\n", e.message).as_bytes(),
        );
    }
    let out = stdout.borrow().clone();
    (out, err_text, status, last)
}

fn expected_path(case: &Case, ext: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(puy_testsuite::TESTS_DIR).join(case.group);
    if case.package {
        // `<group>/…/<name>/main.<ext>`: find the package directory by its `main.pas`
        let pkg = find_dir(&dir, case.name).unwrap_or_else(|| dir.join(case.name));
        return pkg.join(format!("main.{ext}"));
    }
    // Case in a subfolder: find where the .pas is
    let want = format!("{}.pas", case.name);
    let found = find_file(&dir, &want).unwrap_or_else(|| dir.join(&want));
    found.with_file_name(format!("{}.{ext}", case.name))
}

fn find_dir(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == name) && p.join("puy.toml").is_file() {
                return Some(p);
            }
            if let Some(f) = find_dir(&p, name) {
                return Some(f);
            }
        }
    }
    None
}

fn find_file(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
    }
    None
}

fn write_expected(case: &Case, ext: &str, bytes: &[u8]) {
    let p = expected_path(case, ext);
    if bytes.is_empty() && ext != "out" {
        let _ = std::fs::remove_file(&p);
        return;
    }
    std::fs::write(&p, bytes).unwrap_or_else(|e| panic!("cannot write {}: {e}", p.display()));
}

/// Compares byte-for-byte with the expectation file if it exists (otherwise does not compare).
/// With `create`, under `PUY_UPDATE_GOLDEN=1` the expectation file is created even if missing (unless empty).
fn compare_text(
    case: &Case,
    ext: &str,
    actual: &[u8],
    update: bool,
    create: bool,
    failures: &mut Vec<String>,
) {
    match case.file(ext) {
        Some(expected) => {
            if expected != actual {
                if update {
                    write_expected(case, ext, actual);
                } else {
                    failures.push(diff_message(ext, expected, actual));
                }
            }
        }
        None => {
            if update && create && !actual.is_empty() {
                write_expected(case, ext, actual);
            }
        }
    }
}

/// Compares the result of `puy fix` on a unit of a package with `<path without .pas>.fixed.pas`.
/// A unit rewritten without an expectation is a failure (written under `PUY_UPDATE_GOLDEN=1`), so
/// that a case cannot rewrite a unit unnoticed.
fn compare_unit_fixed(
    case: &Case,
    unit: &str,
    expected: Option<&[u8]>,
    actual: &[u8],
    update: bool,
    failures: &mut Vec<String>,
) {
    let rel = format!("{}.fixed.pas", unit.strip_suffix(".pas").unwrap_or(unit));
    if expected == Some(actual) {
        return;
    }
    if update {
        let pkg = std::path::Path::new(puy_testsuite::TESTS_DIR).join(case.group);
        let pkg = find_dir(&pkg, case.name).unwrap_or_else(|| pkg.join(case.name));
        let p = pkg.join(&rel);
        std::fs::write(&p, actual).unwrap_or_else(|e| panic!("cannot write {}: {e}", p.display()));
        return;
    }
    match expected {
        Some(e) => failures.push(diff_message_for(&rel, e, actual)),
        None => failures.push(format!(
            "`puy fix` rewrites `{unit}` but there is no `{rel}`:\n{}",
            String::from_utf8_lossy(actual)
        )),
    }
}

/// If there is no expectation file, requires the output to be empty (`.diag`, `.err`).
fn compare_text_required(
    case: &Case,
    ext: &str,
    actual: &[u8],
    update: bool,
    failures: &mut Vec<String>,
) {
    let expected: &[u8] = case.file(ext).unwrap_or(b"");
    if expected != actual {
        if update {
            write_expected(case, ext, actual);
        } else {
            failures.push(diff_message(ext, expected, actual));
        }
    }
}

fn compare_json(case: &Case, parts: &[String], update: bool, failures: &mut Vec<String>) {
    let actual: Vec<serde_json::Value> = parts
        .iter()
        .map(|p| serde_json::from_str(p).expect("valid JSON"))
        .collect();
    match case.file("json") {
        Some(expected_bytes) => {
            let expected: serde_json::Value = match serde_json::from_slice(expected_bytes) {
                Ok(v) => v,
                Err(e) => {
                    failures.push(format!(".json is not valid JSON: {e}"));
                    return;
                }
            };
            let actual_v = serde_json::Value::Array(actual.clone());
            if expected != actual_v {
                if update {
                    write_expected(case, "json", pretty_json(&actual).as_bytes());
                } else {
                    failures.push(diff_message(
                        "json",
                        serde_json::to_string_pretty(&expected).unwrap().as_bytes(),
                        pretty_json(&actual).as_bytes(),
                    ));
                }
            }
        }
        None => {
            // No .json means no comparison
        }
    }
}

/// A single JSON array in emission order, pretty-printed with 2-space indent (keys in the order of the chapter 7 table).
fn pretty_json(parts: &[serde_json::Value]) -> String {
    let mut s = serde_json::to_string_pretty(&serde_json::Value::Array(parts.to_vec())).unwrap();
    s.push('\n');
    s
}

fn diff_message(ext: &str, expected: &[u8], actual: &[u8]) -> String {
    diff_message_for(&format!(".{ext}"), expected, actual)
}

/// The difference between an expectation file (`what`) and the actual output.
fn diff_message_for(what: &str, expected: &[u8], actual: &[u8]) -> String {
    let e = String::from_utf8_lossy(expected);
    let a = String::from_utf8_lossy(actual);
    let mut msg =
        format!("{what} differs\n--- expected\n{e}\n--- actual\n{a}\n--- first difference:\n");
    let el: Vec<&str> = e.lines().collect();
    let al: Vec<&str> = a.lines().collect();
    for i in 0..el.len().max(al.len()) {
        let x = el.get(i).copied().unwrap_or("<missing>");
        let y = al.get(i).copied().unwrap_or("<missing>");
        if x != y {
            msg.push_str(&format!(
                "line {}:\n  expected: {x}\n  actual:   {y}\n",
                i + 1
            ));
            break;
        }
    }
    msg
}

#[cfg(test)]
mod leak_tests {
    use super::*;

    const LEAK_TEXT: &[u8] =
        b"warning: 3 heap blocks were not freed\n  Hello.TNode: 2\n  string: 1\n";

    fn leaks() -> Vec<(String, u64)> {
        vec![("Hello.TNode".into(), 2), ("string".into(), 1)]
    }

    fn check(case: &Case, leaks: &[(String, u64)]) -> Vec<String> {
        let mut failures = Vec::new();
        compare_leaks(case, leaks, false, &mut failures);
        failures
    }

    // A lifetime/ case passes when the report matches its .leak byte for byte.
    #[test]
    fn leak_matches_expectation() {
        let case = Case {
            group: "lifetime",
            name: "cycle",
            stem: "cycle",
            package: false,
            files: &[("cycle.leak", LEAK_TEXT)],
        };
        assert_eq!(check(&case, &leaks()), Vec::<String>::new());
    }

    // A different report (count or type) fails against .leak, and so does no leak at all.
    #[test]
    fn leak_differs_from_expectation() {
        let case = Case {
            group: "lifetime",
            name: "cycle",
            stem: "cycle",
            package: false,
            files: &[("cycle.leak", LEAK_TEXT)],
        };
        let fewer = vec![("Hello.TNode".into(), 1), ("string".into(), 1)];
        assert_eq!(check(&case, &fewer).len(), 1);
        assert_eq!(check(&case, &[]).len(), 1);
    }

    // Without .leak a lifetime/ case must leave no block.
    #[test]
    fn missing_leak_file_requires_no_leaks() {
        let case = Case {
            group: "lifetime",
            name: "plain",
            stem: "plain",
            package: false,
            files: &[],
        };
        assert_eq!(check(&case, &[]), Vec::<String>::new());
        assert_eq!(check(&case, &leaks()).len(), 1);
    }

    // golden/ and diff/ cases must leave no block and may not carry a .leak file.
    #[test]
    fn other_groups_require_no_leaks() {
        for group in ["golden", "diff"] {
            let plain = Case {
                group,
                name: "plain",
                stem: "plain",
                package: false,
                files: &[],
            };
            assert_eq!(check(&plain, &[]), Vec::<String>::new(), "{group}");
            assert_eq!(check(&plain, &leaks()).len(), 1, "{group}");
            let with_file = Case {
                group,
                name: "cycle",
                stem: "cycle",
                package: false,
                files: &[("cycle.leak", LEAK_TEXT)],
            };
            assert_eq!(check(&with_file, &leaks()).len(), 2, "{group}");
        }
    }
}
