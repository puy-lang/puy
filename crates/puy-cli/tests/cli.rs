//! Tests through the `puy` command (`docs/design/testing.md` 3.11).
//!
//! 1. Runs the executable cases in `tests/diff/` and `tests/golden/` also as `puy run` child
//!    processes, and checks that stdout, stderr, and exit code match the runner (`puy-testsuite`).
//! 2. Runs the cases in `tests/cli/` (things only the CLI produces: inline metadata E2507,
//!    `puy fix` lines, usage error exit code 2, `--format json`, `-e`, stdin) with the arguments
//!    in `<name>.cmd`.
//!
//! `PUY_UPDATE_GOLDEN=1` rewrites `.out` / `.err` / `.code` / `.xml` / `.fixed.pas` in `tests/cli/`.
//! Native only (spawns child processes).

#![cfg(not(target_os = "wasi"))]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn puy() -> &'static str {
    env!("CARGO_BIN_EXE_puy")
}

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn update() -> bool {
    std::env::var_os("PUY_UPDATE_GOLDEN").is_some_and(|v| !v.is_empty() && v != "0")
}

struct Outcome {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: i32,
}

/// Runs `puy` with arguments in `cwd`, feeding `stdin` to standard input.
fn run(cwd: &Path, args: &[String], stdin: &[u8]) -> Outcome {
    let mut child = Command::new(puy())
        .args(args)
        .current_dir(cwd)
        .env("PUY_STDOUT_BUFFERING", "full")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        // Same environment variables as the runner's (puy-testsuite) `FixedEnv`
        .env("PUY_TEST_HOME", "/home/puy")
        .env("EMPTY", "")
        .env_remove("PUY_TEST_MISSING")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn puy");
    {
        let mut input = child.stdin.take().unwrap();
        // Writing may fail for a child process that exits without reading
        let _ = input.write_all(stdin);
    }
    let out = child.wait_with_output().expect("wait puy");
    Outcome {
        stdout: out.stdout,
        stderr: out.stderr,
        code: out.status.code().unwrap_or(-1),
    }
}

/// Reads the file with the same stem.
fn sibling(pas: &Path, ext: &str) -> Option<Vec<u8>> {
    let p = pas.with_extension(ext);
    fs::read(&p).ok()
}

fn parse_code(bytes: &[u8]) -> i32 {
    String::from_utf8_lossy(bytes).trim().parse().unwrap()
}

/// Arguments from `.args` (one per line; same as `puy-testsuite`).
fn parse_args(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect()
}

/// Arguments from `.cmd` (whitespace-separated; wrap in `'...'` / `"..."` to include whitespace).
fn parse_cmd(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in text.trim().chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                has = true;
            }
            None if c.is_whitespace() => {
                if has {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            None => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        args.push(cur);
    }
    args
}

/// Replaces the time of each test on the lines of `puy test` (`(12ms)` at the end of a line;
/// `docs/spec/test.md` 5) with `(Nms)`, so that the expected output does not depend on the
/// machine.
fn mask_durations(stdout: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(stdout);
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        let masked = body.strip_suffix("ms)").and_then(|rest| {
            let open = rest.rfind(" (")?;
            let digits = &rest[open + 2..];
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| format!("{} (Nms)", &rest[..open]))
        });
        match masked {
            Some(m) => {
                out.push_str(&m);
                out.push_str(&line[body.len()..]);
            }
            None => out.push_str(line),
        }
    }
    out.into_bytes()
}

/// Sets the elapsed times in JSON and JUnit to zero for comparison.
fn mask_report_times(report: &[u8], prefix: &str, replacement: &str) -> Vec<u8> {
    let mut text = String::from_utf8_lossy(report).into_owned();
    let mut start = 0;
    while let Some(offset) = text[start..].find(prefix) {
        start += offset + prefix.len();
        let len = text[start..]
            .bytes()
            .take_while(|b| b.is_ascii_digit() || *b == b'.')
            .count();
        if len > 0 {
            text.replace_range(start..start + len, replacement);
            start += replacement.len();
        }
    }
    text.into_bytes()
}

/// Copies a directory, leaving out the expected files after the command (`*.fixed.pas` of
/// `puy fix`, `*.after`).
fn copy_tree(from: &Path, to: &Path) {
    if from.is_dir() {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let p = entry.unwrap().path();
            copy_tree(&p, &to.join(p.file_name().unwrap()));
        }
    } else if !is_fixed_expectation(from) {
        fs::copy(from, to).unwrap();
    }
}

fn is_fixed_expectation(p: &Path) -> bool {
    p.file_name().is_some_and(|n| {
        let n = n.to_string_lossy();
        n.ends_with(".fixed.pas") || n.ends_with(".after")
    })
}

/// The file an expected file after the command stands for: `<path>.pas` for `<path>.fixed.pas`
/// (the result of `puy fix`), `<path>` for `<path>.after` (a file the command writes, such as a
/// snapshot of `puy test --update`).
fn expected_target(rel: &Path) -> String {
    let rel = rel.to_string_lossy();
    match rel.strip_suffix(".after") {
        Some(t) => t.to_string(),
        None => rel.replace(".fixed.pas", ".pas"),
    }
}

/// The expected files after the command under `dir`, as paths relative to it.
fn fixed_expectations(dir: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir.join(rel))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        let r = rel.join(p.file_name().unwrap());
        if p.is_dir() {
            fixed_expectations(dir, &r, out);
        } else if is_fixed_expectation(&p) {
            out.push(r);
        }
    }
}

/// Makes the files named in `<name>.readonly` (one path per line) read-only, so that `puy fix`
/// fails to write them. `false` if the files stay writable (running as root on Unix), when the
/// case cannot be run.
fn make_readonly(work: &Path, list: &[u8], readonly: bool) -> bool {
    for line in String::from_utf8_lossy(list)
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let p = work.join(line.trim());
        let mut perm = fs::metadata(&p).unwrap().permissions();
        perm.set_readonly(readonly);
        fs::set_permissions(&p, perm).unwrap();
        if readonly && fs::OpenOptions::new().write(true).open(&p).is_ok() {
            return false;
        }
    }
    true
}

fn diff_message(what: &str, expected: &[u8], actual: &[u8]) -> String {
    format!(
        "{what} differs\n--- expected ---\n{}\n--- actual ---\n{}\n",
        String::from_utf8_lossy(expected),
        String::from_utf8_lossy(actual)
    )
}

fn compare(
    what: &str,
    expected: &[u8],
    actual: &[u8],
    update_path: Option<&Path>,
    failures: &mut Vec<String>,
) {
    if expected == actual {
        return;
    }
    match update_path {
        Some(p) if update() => {
            if actual.is_empty() {
                let _ = fs::remove_file(p);
            } else {
                fs::write(p, actual).unwrap();
            }
        }
        _ => failures.push(diff_message(what, expected, actual)),
    }
}

/// Runner cases that should give the same result under `puy run`.
fn runnable_cases() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for group in ["diff", "golden"] {
        let mut stack = vec![tests_dir().join(group)];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "pas")
                    && !path.to_string_lossy().ends_with(".fixed.pas")
                {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

/// Strips the CLI-only `help:` lines (`spec/diagnostics.md` 10.4; the `puy fix` and `puy explain`
/// hints) from stderr. They are not part of `.diag` (`design/testing.md` 3.6), so re-runs of runner
/// cases don't compare them (the line format is pinned by the `tests/cli/` cases).
fn strip_cli_help_lines(stderr: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(stderr);
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let is_help = line.starts_with("help: ")
            && (line.contains("can be applied with `puy fix`")
                || line.contains("try `puy explain"));
        if !is_help {
            out.push_str(line);
        }
    }
    out.into_bytes()
}

#[test]
fn runner_cases_through_puy_run() {
    let mut failures = Vec::new();
    let mut count = 0;
    for pas in runnable_cases() {
        let src = fs::read(&pas).unwrap();
        let cfg = puy_testsuite::parse_config(&src).unwrap();
        // Skip cases with conditions the CLI cannot set (host presence, limits, release, merged stderr)
        if cfg.release
            || cfg.no_console
            || cfg.no_env
            || cfg.stack_limit.is_some()
            || cfg.memory_limit.is_some()
            || cfg.merge_stderr
            || cfg.test_ext
            || cfg.test_fs
            || cfg.no_fs
            || cfg.absent_unsupported
            || cfg.clock.is_some()
            || cfg.no_clock
            || cfg.random.is_some()
            || cfg.no_random
        {
            continue;
        }
        // Only cases that execute (have `.out`, `.code`, or `.err`)
        let out = sibling(&pas, "out");
        let code = sibling(&pas, "code");
        let err = sibling(&pas, "err");
        if out.is_none() && code.is_none() && err.is_none() {
            continue;
        }
        let name = pas.file_name().unwrap().to_string_lossy().to_string();
        let mut args = vec!["run".to_string()];
        if let Some(b) = &cfg.stdout_buffering {
            args.push("--stdout-buffering".into());
            args.push(b.clone());
        }
        for d in &cfg.defines {
            args.push("--define".into());
            args.push(d.clone());
        }
        args.push(name.clone());
        if let Some(a) = sibling(&pas, "args") {
            args.extend(parse_args(&a));
        }
        let stdin = sibling(&pas, "in").unwrap_or_default();
        let r = run(pas.parent().unwrap(), &args, &stdin);
        let mut expected_err = sibling(&pas, "diag").unwrap_or_default();
        expected_err.extend(err.unwrap_or_default());
        let mut f = Vec::new();
        compare("stdout", &out.unwrap_or_default(), &r.stdout, None, &mut f);
        compare(
            "stderr",
            &expected_err,
            &strip_cli_help_lines(&r.stderr),
            None,
            &mut f,
        );
        let want_code = code.map(|c| parse_code(&c)).unwrap_or(0);
        if r.code != want_code {
            f.push(format!("exit code: expected {want_code}, got {}", r.code));
        }
        if !f.is_empty() {
            failures.push(format!("[{name}]\n{}", f.join("")));
        }
        count += 1;
    }
    assert!(count > 0, "no runnable cases found");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every program in `examples/` runs to completion with exit code 0, prints something and writes
/// nothing to stderr (no diagnostics or warnings), so the samples keep working as the language
/// changes. A directory with `puy.toml` is a multi-file example, run as `puy run main.pas` inside
/// it. `puy run` does not enable leak detection, so leaks are checked by the `puy-testsuite`
/// runner, which also runs `examples/` (`docs/design/testing.md` 3.11).
#[test]
fn examples_run_cleanly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    // (directory to run in, file to pass to `puy run`)
    let mut programs: Vec<(PathBuf, String)> = Vec::new();
    for e in fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if p.is_dir() && p.join("puy.toml").is_file() {
            programs.push((p, "main.pas".into()));
        } else if name.ends_with(".pas") {
            programs.push((dir.clone(), name));
        }
    }
    programs.sort();
    assert!(
        programs.iter().any(|(d, _)| *d == dir),
        "no single-file examples found"
    );
    assert!(
        programs.iter().any(|(d, _)| *d != dir),
        "no multi-file examples found"
    );
    let mut failures = Vec::new();
    for (cwd, name) in &programs {
        let r = run(cwd, &["run".into(), name.clone()], b"");
        if r.code != 0 || !r.stderr.is_empty() || r.stdout.is_empty() {
            failures.push(format!(
                "[{}] exit code {}, stdout {} bytes\n{}",
                cwd.join(name).display(),
                r.code,
                r.stdout.len(),
                String::from_utf8_lossy(&r.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Run the report example's four tests, including its checked-in snapshot, through package discovery.
#[test]
fn examples_test_report_runs_cleanly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/test_report");
    let r = run(&dir, &["test".into(), "--seed".into(), "1".into()], b"");
    let stdout = String::from_utf8_lossy(&r.stdout);
    let stderr = String::from_utf8_lossy(&r.stderr);
    assert_eq!(r.code, 0, "{stdout}\n{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("test result: ok. 4 ok;")),
        "{stdout}"
    );
}

/// Nesting-limit cases also pass on the main thread with `puy check` (`design/testing.md` 3.9).
#[test]
fn nesting_limit_on_main_thread() {
    let dir = tests_dir().join("golden/syntax");
    let ok = run(&dir, &["check".into(), "nest_256.pas".into()], b"");
    assert_eq!(ok.code, 0, "{}", String::from_utf8_lossy(&ok.stderr));
    let ng = run(&dir, &["check".into(), "nest_257.pas".into()], b"");
    assert_eq!(ng.code, 1);
    let diag = fs::read(dir.join("nest_257.diag")).unwrap();
    assert!(
        ng.stderr.starts_with(&diag),
        "{}",
        String::from_utf8_lossy(&ng.stderr)
    );
}

/// A string the system refuses to allocate is `EOutOfMemory`, caught by the program, on every
/// path that takes the length of a new string from a value (`docs/spec/semantics.md` 3.5). The
/// refusal comes from `ulimit -v`, so this runs on Linux only (a case in `tests/` cannot limit
/// the process; `design/testing.md` 3.6).
#[cfg(target_os = "linux")]
#[test]
fn string_refused_by_the_system_is_out_of_memory() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("refused-string");
    fs::create_dir_all(&dir).unwrap();
    // Each length is within the string length limit and there is no isolate limit, so only the
    // system (1 GB of address space) refuses the gigabytes asked for
    let program = "program Refused;
uses System.SysUtils;
var
  N: Integer;
  S: string;
  U: UTF8String;

procedure Report(const What: string; E: Exception);
begin
  WriteLn(What, ': ', E.ClassName, ': ', E.Message);
end;

begin
  N := 2147483000;
  try S := StringOfChar('x', N); except on E: Exception do Report('StringOfChar', E); end;
  try SetLength(S, N); except on E: Exception do Report('SetLength', E); end;
  try SetLength(U, N); except on E: Exception do Report('SetLength UTF8String', E); end;
  try WriteLn('x':N); except on E: Exception do Report('width', E); end;
  try S := Format('%*d', [N, 1]); except on E: Exception do Report('Format width', E); end;
  try S := Format('%.*f', [N - 1000, 1.5]); except on E: Exception do Report('Format f', E); end;
  try S := Format('%.*e', [N - 1000, 1.5]); except on E: Exception do Report('Format e', E); end;
  try WriteLn(1.5:0:N - 1000); except on E: Exception do Report('X:W:D', E); end;
  S := StringOfChar('y', 3);
  WriteLn('still running: ', S);
end.
";
    fs::write(dir.join("refused.pas"), program).unwrap();
    let out = Command::new("sh")
        .arg("-c")
        .arg("ulimit -v 1000000 && exec \"$0\" run refused.pas")
        .arg(puy())
        .current_dir(&dir)
        .output()
        .expect("spawn sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        stdout,
        "StringOfChar: EOutOfMemory: out of memory
SetLength: EOutOfMemory: out of memory
SetLength UTF8String: EOutOfMemory: out of memory
width: EOutOfMemory: out of memory
Format width: EOutOfMemory: out of memory
Format f: EOutOfMemory: out of memory
Format e: EOutOfMemory: out of memory
X:W:D: EOutOfMemory: out of memory
still running: yyy
"
    );
    assert_eq!(stderr, "");
}

/// The copy of a written string into the standard output buffer is a copy of a Pascal value, so
/// a refusal of the system is `EOutOfMemory` the program catches, not F0003
/// (`docs/spec/semantics.md` 7.1). Linux only, for `ulimit -v` (`design/testing.md` 3.6).
#[cfg(target_os = "linux")]
#[test]
fn stdout_buffer_refused_by_the_system_is_out_of_memory() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("refused-stdout");
    fs::create_dir_all(&dir).unwrap();
    // The string (300 MB as UTF-16) and its UTF-8 text (450 MB) fit in the 1 GB of address
    // space, but a third copy of 450 MB into the buffer does not
    let program = "program Refused;
uses System.SysUtils;
var
  S: string;
begin
  S := StringOfChar('あ', 150000000);
  try Write(S); except on E: Exception do WriteLn('Write: ', E.ClassName, ': ', E.Message); end;
  S := '';
  WriteLn('still running');
end.
";
    fs::write(dir.join("refused.pas"), program).unwrap();
    let out = Command::new("sh")
        .arg("-c")
        .arg("ulimit -v 1000000 && exec \"$0\" run refused.pas")
        .arg(puy())
        .current_dir(&dir)
        .output()
        .expect("spawn sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{stderr}");
    assert_eq!(
        stdout,
        "Write: EOutOfMemory: out of memory
still running
"
    );
    assert_eq!(stderr, "");
}

/// `TFilePath.Resolve` follows symbolic links through the OS, while `Normalize` takes `..` by
/// name (`docs/spec/stdlib/Puy.Paths/README.md` 3). The runner's file system in memory has no
/// links, so this runs `puy` in a directory with one. Unix only, for making the link.
#[cfg(unix)]
#[test]
fn resolve_follows_symbolic_links() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("resolve-links");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("real/sub")).unwrap();
    fs::write(dir.join("real/f.txt"), "f").unwrap();
    std::os::unix::fs::symlink("real/sub", dir.join("link")).unwrap();
    let program = "program ResolveLinks;
uses Puy.Paths;
begin
  WriteLn(TFilePath('link/../f.txt').Resolve.ToString);
  WriteLn(TFilePath('link/../f.txt').Normalize.ToString);
end.
";
    fs::write(dir.join("resolve.pas"), program).unwrap();
    let out = run(&dir, &["run".into(), "resolve.pas".into()], b"");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.code, 0, "stderr:\n{stderr}");
    let canon = fs::canonicalize(&dir).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("{}/real/f.txt\nf.txt\n", canon.display())
    );
}

/// Cases in `tests/cli/<name>/`. The directory is copied to `target/tmp` before running (because
/// `puy fix` rewrites files).
#[test]
fn cli_cases() {
    let root = tests_dir().join("cli");
    let tmp_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-cases");
    let _ = fs::remove_dir_all(&tmp_root);
    let mut failures = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no cases under tests/cli");
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let stem = dir.join(&name);
        let cmd = fs::read(stem.with_extension("cmd"))
            .unwrap_or_else(|_| panic!("{name}: missing {name}.cmd"));
        let args = parse_cmd(&cmd);
        // Copy (excluding expected files; including subdirectories)
        let work = tmp_root.join(&name);
        fs::create_dir_all(&work).unwrap();
        for entry in fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            let fname = p.file_name().unwrap().to_string_lossy().to_string();
            let is_expectation = fname.starts_with(&format!("{name}."))
                && matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("cmd" | "in" | "out" | "err" | "code" | "readonly" | "xml")
                );
            if !is_expectation {
                copy_tree(&p, &work.join(&fname));
            }
        }
        let readonly = fs::read(stem.with_extension("readonly")).ok();
        if let Some(list) = &readonly
            && !make_readonly(&work, list, true)
        {
            make_readonly(&work, list, false);
            eprintln!("skipped (files stay writable for this user): {name}");
            continue;
        }
        let stdin = fs::read(stem.with_extension("in")).unwrap_or_default();
        let mut r = run(&work, &args, &stdin);
        if r.stdout.starts_with(b"{\"schema\":\"puy.test/1\"") {
            r.stdout = mask_report_times(&r.stdout, "\"duration_ms\":", "0");
        } else if args.first().is_some_and(|a| a == "test") {
            r.stdout = mask_durations(&r.stdout);
        }
        if let Some(list) = &readonly {
            make_readonly(&work, list, false);
        }
        let mut f = Vec::new();
        let out_path = stem.with_extension("out");
        let err_path = stem.with_extension("err");
        let code_path = stem.with_extension("code");
        compare(
            "stdout",
            &fs::read(&out_path).unwrap_or_default(),
            &r.stdout,
            Some(&out_path),
            &mut f,
        );
        compare(
            "stderr",
            &fs::read(&err_path).unwrap_or_default(),
            &r.stderr,
            Some(&err_path),
            &mut f,
        );
        let xml_path = stem.with_extension("xml");
        if let Ok(expected) = fs::read(&xml_path) {
            let target = args
                .windows(2)
                .rfind(|pair| pair[0] == "--junit")
                .expect("the JUnit case names the output file");
            match fs::read(work.join(&target[1])) {
                Ok(actual) => compare(
                    "JUnit report",
                    &expected,
                    &mask_report_times(&actual, " time=\"", "0.000"),
                    Some(&xml_path),
                    &mut f,
                ),
                Err(e) => f.push(format!("cannot read JUnit report: {e}\n")),
            }
        }
        let want_code = fs::read(&code_path).map(|c| parse_code(&c)).unwrap_or(0);
        if r.code != want_code {
            if update() {
                if r.code == 0 {
                    let _ = fs::remove_file(&code_path);
                } else {
                    fs::write(&code_path, format!("{}\n", r.code)).unwrap();
                }
            } else {
                f.push(format!("exit code: expected {want_code}, got {}\n", r.code));
            }
        }
        // Files after the command: each `<path>.fixed.pas` is compared with `<path>.pas`, each
        // `<path>.after` with `<path>`
        let mut fixed = Vec::new();
        fixed_expectations(&dir, Path::new(""), &mut fixed);
        for rel in fixed {
            let fixed_path = dir.join(&rel);
            let expected = fs::read(&fixed_path).unwrap();
            let target = expected_target(&rel);
            let actual = fs::read(work.join(&target)).unwrap_or_default();
            compare(
                &format!("file {target} after the command"),
                &expected,
                &actual,
                Some(&fixed_path),
                &mut f,
            );
        }
        if !f.is_empty() {
            failures.push(format!("[{name}]\n{}", f.join("")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
