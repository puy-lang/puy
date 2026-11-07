//! Runs cases with FPC and records `tests/diff/<name>.out` and `.code` (`docs/design/testing.md` 3.3, 3.4).
//!
//! Usage: `cargo run -p puy-testsuite --bin record-fpc -- <case name>…` (`--check` compares against the recording).
//! The FPC version is pinned in this program; other versions are not recorded. Built for native targets only.

#[cfg(not(target_arch = "wasm32"))]
mod native {

    use std::path::PathBuf;
    use std::process::{Command, ExitCode};

    use puy_testsuite::{CASES, Case, TESTS_DIR, find_case, parse_config};

    /// The FPC version used for recording (the apt version on `ubuntu-latest`).
    const FPC_VERSION: &str = "3.2.2";

    pub fn main() -> ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let check = args.iter().any(|a| a == "--check");
        let names: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
        let version = match Command::new("fpc").arg("-iV").output() {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(e) => {
                eprintln!("record-fpc: cannot run fpc: {e}");
                return ExitCode::from(2);
            }
        };
        if version != FPC_VERSION {
            eprintln!(
                "record-fpc: fpc {version} found, but only {FPC_VERSION} is used for recording"
            );
            return ExitCode::from(2);
        }
        let cases: Vec<&Case> = if names.is_empty() {
            CASES.iter().filter(|c| c.group == "diff").collect()
        } else {
            names
                .iter()
                .filter_map(|n| match find_case(n) {
                    Some(c) if c.group == "diff" => Some(c),
                    _ => {
                        eprintln!("record-fpc: no diff case named `{n}`");
                        None
                    }
                })
                .collect()
        };
        let mut failed = 0;
        for case in cases {
            match record(case, check) {
                Ok(()) => println!("ok: {}", case.name),
                Err(e) => {
                    failed += 1;
                    eprintln!("FAILED: {}: {e}", case.name);
                }
            }
        }
        if failed > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        }
    }

    fn record(case: &Case, check: bool) -> Result<(), String> {
        let src = case.source();
        let cfg = parse_config(src).map_err(|e| e.to_string())?;
        let work = std::env::temp_dir().join(format!("puy-record-{}", case.name));
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        // Leading directives (3.3)
        let switches = if cfg.release {
            "{$R-,Q-,C-}"
        } else {
            "{$R+,Q+,C+}"
        };
        let mut text =
            format!("{{$mode delphiunicode}}{{$codepage utf8}}{{$SCOPEDENUMS ON}}{switches}\n");
        // Fully qualified unit names do not work with FPC 3.2.2's `-FN`, so rewrite them to short names
        // (`design/testing.md` 3.3; decided 2026-11-03). In M1 only `System.SysUtils` can be written
        text.push_str(&super::replace_ascii_case_insensitive(
            &String::from_utf8_lossy(src),
            "System.SysUtils",
            "SysUtils",
        ));
        let pas = work.join(format!("{}.pas", case.name));
        std::fs::write(&pas, text).map_err(|e| e.to_string())?;
        let exe = work.join(case.name);
        let mut fpc = Command::new("fpc");
        // FPC options take their value attached (with `-Fa cwstring`, `cwstring` is taken as the source name)
        fpc.arg("-Facwstring")
            .arg("-v0")
            .arg(format!("-o{}", exe.display()));
        for d in &cfg.defines {
            fpc.arg(format!("-d{d}"));
        }
        fpc.arg(&pas);
        let out = fpc.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "fpc failed:\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let mut run = Command::new(&exe);
        // cwstring picks the output encoding from the locale, so pin it to UTF-8
        run.env("LANG", "C.UTF-8").env("LC_ALL", "C.UTF-8");
        if let Some(a) = case.file("args") {
            for line in String::from_utf8_lossy(a).lines() {
                run.arg(line);
            }
        }
        run.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = run.spawn().map_err(|e| e.to_string())?;
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().unwrap();
            let _ = stdin.write_all(case.file("in").unwrap_or(b""));
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let code = out.status.code().unwrap_or(1);
        // Normalize: FPC's 217 (unhandled exception) and runtime errors without `uses SysUtils` (200 etc.;
        // `Runtime error N` on stderr) become 1 (Puy's exit code for an unhandled exception; verified 2026-11-03)
        let stderr_text = String::from_utf8_lossy(&out.stderr);
        let runtime_error = stderr_text.contains("Runtime error ")
            || stderr_text.contains("An unhandled exception occurred");
        let code = if code == 217 || (code != 0 && runtime_error) {
            1
        } else {
            code
        };
        let stdout = out.stdout;
        let dir = case_dir(case);
        let out_path = dir.join(format!("{}.out", case.name));
        let code_path = dir.join(format!("{}.code", case.name));
        let expected_code: i32 = std::fs::read_to_string(&code_path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        if check {
            let expected_out = std::fs::read(&out_path).unwrap_or_default();
            if expected_out != stdout {
                return Err(format!(
                    "stdout differs from .out:\n{}",
                    String::from_utf8_lossy(&stdout)
                ));
            }
            if expected_code != code {
                return Err(format!(
                    "exit code {code} differs from .code ({expected_code})"
                ));
            }
            return Ok(());
        }
        std::fs::write(&out_path, &stdout).map_err(|e| e.to_string())?;
        if code != 0 {
            std::fs::write(&code_path, format!("{code}\n")).map_err(|e| e.to_string())?;
        } else {
            let _ = std::fs::remove_file(&code_path);
        }
        Ok(())
    }

    fn case_dir(case: &Case) -> PathBuf {
        let dir = PathBuf::from(TESTS_DIR).join(case.group);
        let want = format!("{}.pas", case.name);
        find_file(&dir, &want)
            .map(|p| p.parent().unwrap().to_path_buf())
            .unwrap_or(dir)
    }

    fn find_file(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
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
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    native::main()
}

/// On wasm32, only make it build (`cargo test --target wasm32-wasip2` also builds bins).
#[cfg(target_arch = "wasm32")]
fn main() -> std::process::ExitCode {
    eprintln!("record-fpc runs only on native targets");
    std::process::ExitCode::from(2)
}

/// Replaces `pat` in `text` (ASCII case-insensitive) with `rep`.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn replace_ascii_case_insensitive(text: &str, pat: &str, rep: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let pat_lower = pat.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(off) = lower[i..].find(&pat_lower) {
        out.push_str(&text[i..i + off]);
        out.push_str(rep);
        i += off + pat.len();
    }
    out.push_str(&text[i..]);
    out
}
