//! Tests through the `puy` command (`docs/design/testing.md` 3.11).
//!
//! 1. Runs the executable cases in `tests/diff/` and `tests/golden/` also as `puy run` child
//!    processes, and checks that stdout, stderr, and exit code match the runner (`puy-testsuite`).
//! 2. Runs the cases in `tests/cli/` (things only the CLI produces: inline metadata E2507,
//!    `puy fix` lines, usage error exit code 2, `--format json`, `-e`, stdin) with the arguments
//!    in `<name>.cmd`.
//!
//! `PUY_UPDATE_GOLDEN=1` rewrites `.out` / `.err` / `.code` / `.fixed.pas` in `tests/cli/`.
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

/// Copies a directory, leaving out the expected results of `puy fix` (`*.fixed.pas`).
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
    p.file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".fixed.pas"))
}

/// The expected results of `puy fix` under `dir`, as paths relative to it.
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
                    Some("cmd" | "in" | "out" | "err" | "code" | "readonly")
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
        let r = run(&work, &args, &stdin);
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
        // Result of `puy fix`: each `<path>.fixed.pas` is compared with `<path>.pas`
        let mut fixed = Vec::new();
        fixed_expectations(&dir, Path::new(""), &mut fixed);
        for rel in fixed {
            let fixed_path = dir.join(&rel);
            let expected = fs::read(&fixed_path).unwrap();
            let target = rel.to_string_lossy().replace(".fixed.pas", ".pas");
            let actual = fs::read(work.join(&target)).unwrap_or_default();
            compare(
                &format!("fixed file {target}"),
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
