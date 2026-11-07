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
    let path = format!("{}.pas", case.name);
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
        script_name: Some(script_name(case.name)),
        extra_files: Vec::new(),
    };
    let out = compile(&path, src.to_vec(), &mut provider, &opts);
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
    // Applying fix suggestions
    if case.file("fixed.pas").is_some() || update {
        let mut provider2 = MemorySourceProvider::new();
        for (name, bytes) in case.includes() {
            provider2 = provider2.with_file(name, bytes.to_vec());
        }
        let r = puy_core::fix::fix(&path, src.to_vec(), &mut provider2, &opts, false);
        let fixed = puy_core::fix::normalize_newlines(&r.bytes);
        if case.file("fixed.pas").is_some() || (update && !r.applied.is_empty()) {
            compare_text(case, "fixed.pas", &fixed, update, false, &mut failures);
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
    if !has_errors && (wants_run || !is_diff) {
        let module_bytes = out
            .module
            .as_deref()
            .expect("a module when there are no errors");
        {
            let (stdout, stderr, status) = execute(case, &cfg, module_bytes, &path);
            let code = status.exit_code();
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

/// Runs with a host that captures the module. Returns (stdout, the text the runtime writes to stderr, exit status).
fn execute(
    case: &Case,
    cfg: &CaseConfig,
    module_bytes: &[u8],
    path: &str,
) -> (Vec<u8>, Vec<u8>, ExitStatus) {
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
                stderr: stderr.clone(),
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
        stdout_buffering: StdoutBuffering::Full,
    };
    let mut iso = Isolate::new(module, host, config).expect("isolate");
    let mut args = vec![path.to_string()];
    if let Some(a) = case.file("args") {
        args.extend(String::from_utf8_lossy(a).lines().map(|l| l.to_string()));
    }
    iso.start_main(args).expect("start");
    let status = loop {
        match iso.run(10_000) {
            RunOutcome::Finished(st) => break st,
            RunOutcome::BudgetExhausted | RunOutcome::Waiting => continue,
            RunOutcome::Fatal(e) => panic!("fatal: {}", e.message),
        }
    };
    // Build the stderr text the same way as the runtime loop
    let mut err_text: Vec<u8> = Vec::new();
    if let ExitKind::UncaughtException(rep) = &status.kind {
        err_text.extend_from_slice(rep.render().as_bytes());
    }
    if let Some(e) = &status.stdout_error {
        err_text.extend_from_slice(
            format!("error: failed to write to stdout: {}\n", e.message).as_bytes(),
        );
    }
    let out = stdout.borrow().clone();
    (out, err_text, status)
}

fn expected_path(case: &Case, ext: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(puy_testsuite::TESTS_DIR).join(case.group);
    // Case in a subfolder: find where the .pas is
    let want = format!("{}.pas", case.name);
    let found = find_file(&dir, &want).unwrap_or_else(|| dir.join(&want));
    found.with_file_name(format!("{}.{ext}", case.name))
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
    let e = String::from_utf8_lossy(expected);
    let a = String::from_utf8_lossy(actual);
    let mut msg =
        format!(".{ext} differs\n--- expected\n{e}\n--- actual\n{a}\n--- first difference:\n");
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
