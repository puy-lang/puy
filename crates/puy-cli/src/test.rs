//! `puy test` (`docs/spec/test.md`): finds the test units, compiles them into one module, runs
//! each test in an isolate of its own and reports the results.

mod report;
mod snapshot;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use puy_core::bytecode::{FIRST_TYPE_INDEX, Module, TYPE_CLASS, exc};
use puy_core::diagnostics::{Code, Diagnostic, Severity, Stage};
use puy_core::driver::{PackageRoot, TestSource, compile_tests, root_prefix, test_unit_name_of};
use puy_core::source::SourceProvider;
use puy_core::vm::{
    ExceptionReport, ExitKind, ExitStatus, HashSeed, Isolate, IsolateConfig, IsolateError,
    RunOutcome, StdoutBuffering, VerifiedModule, hash, render_leaks,
};
use puy_host::{HostFuture, OutStream};

use crate::args::{Format, Options, TestArgs};
use crate::{metadata, output, runtime};
use snapshot::{SnapshotCounts, Snapshots, TestSnapshots};

/// The result of a test (`docs/spec/test.md` 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Ok,
    Fail,
    Error,
    Skip,
}

impl Outcome {
    fn failed(self) -> bool {
        matches!(self, Self::Fail | Self::Error)
    }

    /// The word of the test's line, padded to one width.
    fn word(self) -> &'static str {
        match self {
            Outcome::Ok => "ok   ",
            Outcome::Fail => "FAIL ",
            Outcome::Error => "ERROR",
            Outcome::Skip => "skip ",
        }
    }
}

/// What a run test left to report.
struct TestReport {
    name: String,
    outcome: Outcome,
    elapsed: Duration,
    /// The lines that explain the result: the uncaught exceptions, `Halt`, the time limit and
    /// the leaks (`docs/spec/test.md` 5).
    details: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    location: TestLocation,
    program: Option<ExitStatus>,
    last: Option<ExitStatus>,
}

struct TestLocation {
    unit: String,
    file: String,
    line: u32,
}

impl TestLocation {
    fn new(module: &Module, entry: &puy_core::bytecode::TestEntry) -> Self {
        // The thunk is named `<unit>.<test:...>`, even when the declaration is in an include.
        let (unit, _) = module.funcs[entry.func as usize]
            .name
            .split_once(".<test:")
            .expect("the compiler names the test thunk after its unit");
        Self {
            unit: unit.to_owned(),
            file: module
                .file_path(entry.file)
                .expect("the verifier checks the test's source file")
                .to_owned(),
            line: entry.line,
        }
    }
}

/// A console that keeps what the test writes and gives it an empty standard input
/// (`docs/spec/test.md` 3).
struct CaptureConsole {
    stdout: Rc<RefCell<Vec<u8>>>,
    stderr: Rc<RefCell<Vec<u8>>>,
}

impl puy_host::Console for CaptureConsole {
    fn write(&mut self, stream: OutStream, bytes: &[u8]) -> HostFuture<()> {
        match stream {
            OutStream::Stdout => self.stdout.borrow_mut().extend_from_slice(bytes),
            OutStream::Stderr => self.stderr.borrow_mut().extend_from_slice(bytes),
        }
        puy_host::ready(Ok(()))
    }

    fn read(&mut self, _max: usize) -> HostFuture<Vec<u8>> {
        puy_host::ready(Ok(Vec::new()))
    }
}

pub fn test(args: &TestArgs, opts: &Options) -> u8 {
    let root = metadata::find_package_root("<eval>");
    let prefix = root.as_ref().map(|r| root_prefix("", r.levels));
    let files = match collect(args, root.as_ref(), prefix.as_deref(), opts) {
        Ok(f) => f,
        Err(code) => return code,
    };
    let mut provider = runtime::FileSourceProvider;
    let snapshots = Snapshots::new(args.update, &files);
    let mut sources = Vec::new();
    for (path, rel) in files {
        match provider.read(&path) {
            Ok(bytes) => sources.push(TestSource { path, rel, bytes }),
            Err(e) => return cannot_read(&path, &e, opts),
        }
    }
    let co = crate::compile_options_for_package(root.clone(), opts);
    let seed = match args.seed {
        Some(s) => s,
        None => match run_seed() {
            Ok(s) => s,
            Err(e) => {
                output::emit_fatal("F0001", &e, opts);
                return 101;
            }
        },
    };
    let Some(out) = compile_tests(sources, prefix, &mut provider, &co) else {
        // No test unit: the same report as when no test is chosen (`docs/spec/test.md` 1, 5)
        if opts.format == Format::Text {
            print_running(0, seed);
        }
        return emit_results(&[], 0, seed, None, None, args, opts);
    };
    let has_errors = out.diagnostics.iter().any(|d| d.is_error());
    let diagnostics = (opts.format == Format::Json && !out.diagnostics.is_empty())
        .then(|| puy_core::diagnostics::json::diagnostics_json(&out.diagnostics, &out.sources));
    if opts.format == Format::Text && !out.diagnostics.is_empty() {
        output::emit_diagnostics(
            &out.diagnostics,
            &out.sources,
            opts,
            false,
            Some(out.files.as_slice()),
        );
    }
    let Some(bytes) = out.module.filter(|_| !has_errors) else {
        if opts.format == Format::Json {
            report::print_json(&[], 0, seed, diagnostics.as_deref(), None);
        }
        if let Some(path) = &args.junit {
            let text = puy_core::diagnostics::render::render_all(&out.diagnostics, &out.sources);
            if !report::write_junit(path, &report::junit_compile_error(&text), opts) {
                return 2;
            }
        }
        return 1;
    };
    let module = match Module::decode(&bytes, puy_core::VERSION)
        .map_err(|e| e.to_string())
        .and_then(|m| VerifiedModule::new(Arc::new(m)).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            output::emit_fatal("F0004", &e, opts);
            return 101;
        }
    };
    runtime::set_running();
    let tests = &module.module().tests;
    let skip_class = skip_test_class(module.module());
    let chosen: Vec<usize> = (0..tests.len())
        .filter(|&i| selected(&tests[i].name, args))
        .collect();
    let filtered_out = tests.len() - chosen.len();
    if opts.format == Format::Text {
        print_running(chosen.len(), seed);
    }
    let mut reports = Vec::new();
    for i in chosen {
        let entry = &tests[i];
        let report = if let Some(reason) = &entry.skip {
            TestReport {
                name: entry.name.clone(),
                outcome: Outcome::Skip,
                elapsed: Duration::ZERO,
                details: reason.clone(),
                stdout: Vec::new(),
                stderr: Vec::new(),
                location: TestLocation::new(module.module(), entry),
                program: None,
                last: None,
            }
        } else {
            match run_one(&module, i, seed, args.timeout, skip_class, &snapshots) {
                Ok(r) => r,
                Err(Fatal::Vm(m)) => {
                    output::emit_fatal("F0001", &format!("internal error: {m}"), opts);
                    return 101;
                }
                Err(Fatal::Isolate(e)) => {
                    let code = match e {
                        IsolateError::HostFunction(_) => "F0005",
                        IsolateError::InvalidModule(_) => "F0004",
                        IsolateError::Extension(_) => "F0007",
                        _ => "F0001",
                    };
                    output::emit_fatal(code, &e.to_string(), opts);
                    return 101;
                }
            }
        };
        if opts.format == Format::Text {
            print_line(&report);
        }
        reports.push(report);
    }
    let counts = snapshots.borrow().counts();
    emit_results(
        &reports,
        filtered_out,
        seed,
        diagnostics.as_deref(),
        counts.as_ref(),
        args,
        opts,
    )
}

fn emit_results(
    reports: &[TestReport],
    filtered_out: usize,
    seed: i64,
    diagnostics: Option<&str>,
    snapshots: Option<&SnapshotCounts>,
    args: &TestArgs,
    opts: &Options,
) -> u8 {
    match opts.format {
        Format::Text => {
            print_details(reports, args.show_output);
            print_summary(reports, filtered_out, seed, snapshots);
        }
        Format::Json => report::print_json(reports, filtered_out, seed, diagnostics, snapshots),
    }
    if let Some(path) = &args.junit
        && !report::write_junit(path, &report::junit_xml(reports), opts)
    {
        return 2;
    }
    u8::from(reports.iter().any(|r| r.outcome.failed()))
}

/// The test files to compile, as (path from the working directory, path from the package root)
/// (`docs/spec/test.md` 1). A usage error or a path that cannot be read ends the command.
fn collect(
    args: &TestArgs,
    root: Option<&PackageRoot>,
    prefix: Option<&str>,
    opts: &Options,
) -> Result<Vec<(String, Option<String>)>, u8> {
    let mut provider = runtime::FileSourceProvider;
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    if args.paths.is_empty() {
        let (Some(_), Some(prefix)) = (root, prefix) else {
            eprintln!(
                "error: no package (`puy.toml`) was found; pass the test files to run\n\n{}",
                crate::args::USAGE
            );
            return Err(2);
        };
        for base in ["src", "tests"] {
            let dir = format!("{prefix}{base}");
            let mut listed = provider.list(&dir).unwrap_or_default();
            listed.sort();
            for f in listed {
                let f = f.replace('\\', "/");
                if test_unit_name_of(&f).is_some() {
                    out.push((format!("{dir}/{f}"), Some(format!("{base}/{f}"))));
                }
            }
        }
        return Ok(out);
    }
    for p in &args.paths {
        if std::path::Path::new(p).is_dir() {
            let mut listed = match provider.list(p) {
                Ok(l) => l,
                Err(e) => return Err(cannot_read(p, &e, opts)),
            };
            listed.sort();
            let dir = p.trim_end_matches(['/', '\\']);
            for f in listed {
                let f = f.replace('\\', "/");
                // Every part of the path under the directory is an identifier, as for the files
                // found without paths
                if test_unit_name_of(&f).is_some() {
                    let path = format!("{dir}/{f}");
                    let rel = root.and_then(|r| rel_from_root(&path, r.levels));
                    out.push((path, rel));
                }
            }
            continue;
        }
        let file = p.rsplit(['/', '\\']).next().unwrap_or(p);
        if test_unit_name_of(file).is_none() {
            if let Err(e) = provider.read(p) {
                return Err(cannot_read(p, &e, opts));
            }
            eprintln!(
                "error: `{p}` is not a test file (`*_test.pas`)\n\n{}",
                crate::args::USAGE
            );
            return Err(2);
        }
        let rel = root.and_then(|r| rel_from_root(p, r.levels));
        out.push((p.clone(), rel));
    }
    // A file named twice (directly and through its directory) is compiled once
    let mut seen = std::collections::HashSet::new();
    out.retain(|(path, rel)| {
        let key = rel
            .clone()
            .unwrap_or_else(|| puy_core::driver::normalize_path(path));
        seen.insert(key)
    });
    Ok(out)
}

/// The path of `path` (from the working directory) from the package root found from the working
/// directory, `levels` up from it; `None` for a file outside that root, which is compiled as a
/// test unit without a package (`docs/spec/test.md` 1). Paths are folded as text, without
/// resolving links (`docs/spec/units.md` 2.1).
fn rel_from_root(path: &str, levels: u32) -> Option<String> {
    fn fold(p: &std::path::Path) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in p.components() {
            match c {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    out.pop();
                }
                c => out.push(c.as_os_str().to_string_lossy().into_owned()),
            }
        }
        out
    }
    let cwd = std::env::current_dir().ok()?;
    let mut root = fold(&cwd);
    for _ in 0..levels {
        root.pop()?;
    }
    let file = fold(&cwd.join(path));
    let rel = file.strip_prefix(root.as_slice())?;
    (!rel.is_empty()).then(|| rel.join("/"))
}

/// F0002 for a path that cannot be read (`docs/spec/diagnostics.md` 10.5).
fn cannot_read(path: &str, e: &puy_host::HostError, opts: &Options) -> u8 {
    let d = Diagnostic::new(
        Code::F(2),
        Severity::Error,
        format!("cannot read `{path}`: {}", e.fixed_text()),
        Stage::Driver,
    );
    output::emit_standalone(&d, opts, false);
    2
}

/// The seed of the run when `--seed` is not given: from the random source (§8.9).
fn run_seed() -> Result<i64, String> {
    let mut host = runtime::host_set();
    let random = host
        .random
        .as_mut()
        .ok_or_else(|| "no random source".to_string())?;
    random
        .hash_seed()
        .map(|s| s as i64)
        .map_err(|e| e.message.clone())
}

/// Whether `--filter` / `--exact` choose the test (`docs/spec/test.md` 6): a name containing one
/// of the texts, or equal to one with `--exact`, ignoring ASCII case.
fn selected(name: &str, args: &TestArgs) -> bool {
    if args.filters.is_empty() {
        return true;
    }
    let name = name.to_ascii_lowercase();
    args.filters.iter().any(|f| {
        let f = f.to_ascii_lowercase();
        if args.exact {
            name == f
        } else {
            name.contains(&f)
        }
    })
}

/// The seed of a test: `mix64(r xor fnv1a64(name))` (`docs/spec/test.md` 7), so that it does not
/// depend on which other tests run.
fn test_seed(run: i64, name: &str) -> u64 {
    // The 64-bit FNV-1a of the name, as the fingerprint of an extension's source is made
    hash::mix64(run as u64 ^ puy_core::ext::fingerprint(name))
}

enum Fatal {
    Vm(String),
    Isolate(IsolateError),
}

/// Runs the test `index` in a new isolate to the end of its finishing stage, within the time
/// limit (`docs/spec/test.md` 3, 4).
fn run_one(
    module: &VerifiedModule,
    index: usize,
    run: i64,
    timeout: u64,
    skip_class: Option<u32>,
    snapshots: &Rc<RefCell<Snapshots>>,
) -> Result<TestReport, Fatal> {
    let entry = &module.module().tests[index];
    let location = TestLocation::new(module.module(), entry);
    let stdout = Rc::new(RefCell::new(Vec::new()));
    let stderr = Rc::new(RefCell::new(Vec::new()));
    let mut host = runtime::host_set();
    host.console = Some(Box::new(CaptureConsole {
        stdout: stdout.clone(),
        stderr: stderr.clone(),
    }));
    let unit_path = module
        .module()
        .units
        .iter()
        .find(|u| u.name == location.unit)
        .map_or(location.file.as_str(), |u| u.path.as_str());
    host.test = Some(Box::new(TestSnapshots::new(
        snapshots,
        &location.unit,
        unit_path,
        &entry.name,
        entry.case,
    )));
    let config = IsolateConfig {
        stdout_buffering: StdoutBuffering::Full,
        hash_seed: HashSeed::Fixed(hash::seed_from_fixed_random_seed(test_seed(
            run,
            &entry.name,
        ))),
        leak_check: true,
        extensions: runtime::extensions(),
        ..Default::default()
    };
    let budget = config.budget_default;
    let start = Instant::now();
    let limit = (timeout > 0).then(|| Duration::from_secs(timeout));
    let mut iso = Isolate::from_verified(module, host, config).map_err(Fatal::Isolate)?;
    iso.start_test(index, vec![String::new()])
        .map_err(Fatal::Isolate)?;
    let mut program: Option<ExitStatus> = None;
    let last = loop {
        match iso.run(budget) {
            RunOutcome::Finished(st) if program.is_none() => {
                program = Some(st);
                iso.finish();
            }
            RunOutcome::Finished(st) => break Some(st),
            RunOutcome::BudgetExhausted | RunOutcome::Waiting => {}
            RunOutcome::Fatal(e) => return Err(Fatal::Vm(e.message)),
        }
        if limit.is_some_and(|l| start.elapsed() > l) {
            break None;
        }
    };
    let elapsed = start.elapsed();
    drop(iso);
    let (outcome, details) = judge(program.as_ref(), last.as_ref(), timeout, skip_class);
    Ok(TestReport {
        name: entry.name.clone(),
        outcome,
        elapsed,
        details,
        stdout: stdout.take(),
        stderr: stderr.take(),
        location,
        program,
        last,
    })
}

/// The type index of `Puy.Testing.ESkipTest` in the module, if a test can raise it (the class
/// is in the type section only when it is used).
fn skip_test_class(module: &Module) -> Option<u32> {
    module
        .types
        .iter()
        .position(|t| t.kind == TYPE_CLASS && t.name == "Puy.Testing.ESkipTest")
        .map(|i| FIRST_TYPE_INDEX + i as u32)
}

/// The result of a test from how its routine ended (`program`) and how its finishing stage did
/// (`last`; `None` past the time limit), with the lines that explain it, or the reason of a SKIP
/// (`docs/spec/test.md` 4).
fn judge(
    program: Option<&ExitStatus>,
    last: Option<&ExitStatus>,
    timeout: u64,
    skip_class: Option<u32>,
) -> (Outcome, String) {
    let mut details = String::new();
    // The uncaught `ESkipTest` of a test skipped while it ran: its reason, and its report in case
    // the test ends as ERROR all the same
    let skipped = program.and_then(|p| match &p.kind {
        ExitKind::UncaughtException(r)
            if skip_class.is_some_and(|c| r.class_types.contains(&c)) =>
        {
            Some((r.message.clone(), r.render()))
        }
        _ => None,
    });
    let mut outcome = if skipped.is_some() {
        Outcome::Skip
    } else {
        Outcome::Ok
    };
    let mut worse = |o: Outcome| {
        if outcome == Outcome::Ok || o == Outcome::Error {
            outcome = o;
        }
    };
    if let Some(p) = program {
        match &p.kind {
            ExitKind::UncaughtException(report) if skipped.is_none() => {
                details.push_str(&report.render());
                worse(if is_assertion(report) {
                    Outcome::Fail
                } else {
                    Outcome::Error
                });
            }
            _ => {}
        }
        for r in &p.halt_exceptions {
            details.push_str(&r.render_halt());
        }
        if p.halted {
            details.push_str(&format!(
                "error: the test called `Halt` (exit code {})\n",
                p.code
            ));
            worse(Outcome::Error);
        }
    }
    let Some(last) = last else {
        details.push_str(&format!(
            "error: the test did not finish within {timeout} second{}\n",
            if timeout == 1 { "" } else { "s" }
        ));
        worse(Outcome::Error);
        return skip_result(outcome, details, skipped);
    };
    for r in &last.fini_exceptions {
        details.push_str(&r.render());
        worse(Outcome::Error);
    }
    for r in &last.halt_exceptions {
        details.push_str(&r.render_halt());
    }
    let leaks = render_leaks(&last.leaks);
    if !leaks.is_empty() {
        details.push_str(&leaks);
        // A test that failed otherwise keeps its result; the leaks are reported all the same
        if outcome == Outcome::Ok {
            outcome = Outcome::Fail;
        }
    }
    skip_result(outcome, details, skipped)
}

/// The result of `judge` for a test skipped while it ran (`skipped`): SKIP with its reason in
/// place of the details (the leaks of a test that stopped there are not judged), or, when the
/// test ends as ERROR all the same, the details after the report of the `ESkipTest`.
fn skip_result(
    outcome: Outcome,
    details: String,
    skipped: Option<(String, String)>,
) -> (Outcome, String) {
    match skipped {
        Some((reason, _)) if outcome == Outcome::Skip => (outcome, reason),
        Some((_, report)) => (outcome, report + &details),
        None => (outcome, details),
    }
}

/// Whether an uncaught exception is an `EAssertionFailed` or a descendant: a failed assertion
/// rather than an error (`docs/spec/test.md` 4).
fn is_assertion(report: &ExceptionReport) -> bool {
    report.class_types.contains(&exc::ASSERTION_FAILED)
}

fn print_running(count: usize, seed: i64) {
    println!(
        "running {count} test{} (seed {seed})",
        if count == 1 { "" } else { "s" }
    );
}

fn print_line(r: &TestReport) {
    match r.outcome {
        Outcome::Skip if r.details.is_empty() => println!("{} {}", r.outcome.word(), r.name),
        Outcome::Skip => println!("{} {}: {}", r.outcome.word(), r.name, r.details),
        _ => println!(
            "{} {} ({}ms)",
            r.outcome.word(),
            r.name,
            r.elapsed.as_millis()
        ),
    }
}

/// The details of the failed tests (and of every run test with `--show-output`), in the order
/// they ran (`docs/spec/test.md` 5).
fn print_details(reports: &[TestReport], show_output: bool) {
    for r in reports {
        let failed = r.outcome.failed();
        let shown = failed || (show_output && r.outcome == Outcome::Ok);
        if !shown {
            continue;
        }
        println!();
        println!("---- {} ----", r.name);
        if failed {
            print!("{}", r.details);
        }
        for (label, bytes) in [("stdout", &r.stdout), ("stderr", &r.stderr)] {
            if bytes.is_empty() {
                continue;
            }
            println!("{label}:");
            let text = String::from_utf8_lossy(bytes);
            print!("{text}");
            if !text.ends_with('\n') {
                println!();
            }
        }
    }
}

fn print_summary(
    reports: &[TestReport],
    filtered_out: usize,
    seed: i64,
    snapshots: Option<&SnapshotCounts>,
) {
    let counts = report::Counts::of(reports);
    let failed = counts.failed + counts.errors > 0;
    println!();
    println!(
        "test result: {}. {} ok; {} failed; {} errors; {} skipped; {} filtered out",
        if failed { "FAILED" } else { "ok" },
        counts.ok,
        counts.failed,
        counts.errors,
        counts.skipped,
        filtered_out
    );
    if let Some(c) = snapshots {
        println!(
            "snapshots: {} written; {} unused (kept)",
            c.written, c.unused
        );
    }
    if failed {
        println!();
        println!("to run a failed test again:");
        for r in reports.iter().filter(|r| r.outcome.failed()) {
            println!("  {}", repro_command(seed, &r.name));
        }
    }
}

fn repro_command(seed: i64, name: &str) -> String {
    format!(
        "puy test --seed {seed} --filter {} --exact",
        shell_quote(name)
    )
}

/// `s` in single quotes for a POSIX shell (`'` written as `'\''`).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The seed of a test depends on the run's seed and the test's name only, so filtering does
    // not change it (values worked out apart from the implementation, from the definition in
    // `docs/spec/test.md` 7).
    #[test]
    fn test_seed_mixes_the_run_seed_and_the_name() {
        for (run, name, seed) in [
            (0, "", 0xF52A_15E9_A9B5_E89B),
            (42, "a", 0x2C98_7FE0_B5E2_7E11),
            (-1, "T_test.Alone", 0x613F_48E2_B8CA_898A),
        ] {
            assert_eq!(test_seed(run, name), seed, "{run} {name}");
        }
    }

    // A name with a quote is quoted so that the shell gives it back as is.
    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(
            shell_quote("T_test.Echo('a', 1)"),
            "'T_test.Echo('\\''a'\\'', 1)'"
        );
        assert_eq!(shell_quote("T_test.Alone"), "'T_test.Alone'");
    }
}
