//! Runs cases with FPC and records `tests/diff/<name>.out` and `.code` (`docs/design/testing.md` 3.3, 3.4).
//!
//! Usage: `cargo run -p puy-testsuite --bin record-fpc -- <case name>…` (`--check` compares against the recording).
//! The FPC version is pinned in this program; other versions are not recorded. Built for native targets only.

#[cfg(not(target_arch = "wasm32"))]
mod native {

    use std::path::PathBuf;
    use std::process::{Command, ExitCode};

    use puy_testsuite::{
        CASES, Case, TESTS_DIR, diff_switches, find_case, normalize_exit_code, parse_config,
        replace_ascii_case_insensitive,
    };

    /// The FPC version used for recording (the apt version on `ubuntu-latest`).
    const FPC_VERSION: &str = "3.2.2";

    /// Cases FPC 3.2.2 cannot compile, and why (`design/testing.md` 3.4): their `.out` is written
    /// by hand and checked with Delphi (`check-dcc`). Such a case is compiled without writing
    /// anything, and one that compiles (the list is out of date) makes the run fail.
    const UNSUPPORTED: &[(&str, &str)] = &[
        // FPC's helpers of `string` are for `AnsiString`; `delphiunicode` strings have none
        ("string_helper", "no helper for UnicodeString"),
        // FPC 3.2.2 has no anonymous methods
        ("overload_proc_values", "no anonymous methods"),
        // FPC 3.2.2 has no `System.RegularExpressions`
        ("regex_basics", "no System.RegularExpressions"),
        ("regex_replace", "no System.RegularExpressions"),
        // FPC 3.2.2's `TStream` has no `TBytes` forms of `Read` / `Write`
        ("memory_stream", "no TStream.Read / Write of TBytes"),
        // FPC 3.2.2 has no `TStreamReader` / `TStreamWriter` / `TBinaryReader` / `TBinaryWriter`
        ("stream_reader_writer", "no TStreamReader / TStreamWriter"),
        ("binary_reader_writer", "no TBinaryReader / TBinaryWriter"),
        // FPC 3.2.2 has no `System.IOUtils`
        ("ioutils_errors", "no System.IOUtils"),
        // FPC 3.2.2's `TTypeKind` has other values (no `tkProcedure`)
        ("rtti_type_kinds", "TTypeKind has other values"),
        // FPC 3.2.2 declares `TValue` in `Rtti` and its `TTypeKind` has other values
        ("tvalue_basic", "TValue is in the unit Rtti"),
        // FPC 3.2.2's `Rtti` lists the members in another way
        ("rtti_members", "Rtti has other members"),
        // FPC 3.2.2's `Rtti` has no `Invoke` of the same forms
        ("rtti_invoke", "Rtti has another Invoke"),
        // FPC 3.2.2 does not resolve System.Rtti directly
        ("rtti_published_no_m", "no System.Rtti"),
    ];

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
            if let Some((_, reason)) = UNSUPPORTED.iter().find(|(n, _)| *n == case.name) {
                match record(case, true) {
                    Err(e) if e.starts_with(FPC_FAILED) => {
                        println!("unsupported: {} ({reason})", case.name);
                    }
                    _ => {
                        failed += 1;
                        eprintln!(
                            "FAILED: {}: listed as unsupported, but FPC compiles it",
                            case.name
                        );
                    }
                }
                continue;
            }
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

    /// The start of the error of a case FPC cannot compile.
    const FPC_FAILED: &str = "fpc failed:";

    fn record(case: &Case, check: bool) -> Result<(), String> {
        let src = case.source();
        let cfg = parse_config(src).map_err(|e| e.to_string())?;
        let work = std::env::temp_dir().join(format!("puy-record-{}", case.name));
        // Start clean so that no stale unit (`.ppu`) from an earlier run is used
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        // Leading directives (3.3)
        let switches = diff_switches(&cfg);
        let pas = work.join(format!("{}.pas", case.stem));
        std::fs::write(&pas, fpc_source(src, switches)).map_err(|e| e.to_string())?;
        // A package (`design/testing.md` 3.6): the units are flat under `src/`, which FPC searches
        // with `-Fu`. Folders under `src/` (dotted names from the path) are not supported here.
        let unit_dir = work.join("src");
        if case.package {
            std::fs::create_dir_all(&unit_dir).map_err(|e| e.to_string())?;
            for (name, bytes) in case.includes() {
                let Some(file) = name.strip_prefix("src/") else {
                    continue;
                };
                if !file.ends_with(".pas") {
                    continue;
                }
                if file.contains('/') {
                    return Err(format!(
                        "units in folders under src/ are not supported: {name}"
                    ));
                }
                std::fs::write(unit_dir.join(file), fpc_source(bytes, switches))
                    .map_err(|e| e.to_string())?;
            }
        }
        let exe = work.join(case.name);
        let mut fpc = Command::new("fpc");
        // FPC options take their value attached (with `-Fa cwstring`, `cwstring` is taken as the source name)
        fpc.arg("-Facwstring")
            .arg("-v0")
            .arg(format!("-o{}", exe.display()));
        for d in &cfg.defines {
            fpc.arg(format!("-d{d}"));
        }
        if case.package {
            fpc.arg(format!("-Fu{}", unit_dir.display()));
        }
        fpc.arg(&pas);
        let out = fpc.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "{FPC_FAILED}\n{}{}",
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
        let code = normalize_exit_code(
            code,
            &String::from_utf8_lossy(&out.stderr),
            &["Runtime error ", "An unhandled exception occurred"],
        );
        let stdout = out.stdout;
        let dir = case_dir(case);
        let out_path = dir.join(format!("{}.out", case.stem));
        let code_path = dir.join(format!("{}.code", case.stem));
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

    /// The source given to FPC: the leading directives (3.3) and the rewrites of names FPC 3.2.2
    /// does not accept. `{$MINFPCONSTPREC 64}` makes FPC type real constants such as `3.0` as
    /// Double, as Puy does; otherwise it computes expressions with them in single precision.
    fn fpc_source(src: &[u8], switches: &str) -> String {
        let mut text = format!(
            "{{$mode delphiunicode}}{{$codepage utf8}}{{$SCOPEDENUMS ON}}{{$MINFPCONSTPREC 64}}{switches}\n"
        );
        // Fully qualified unit names do not work with FPC 3.2.2's `-FN`, so rewrite them to short
        // names (`design/testing.md` 3.3; decided 2026-11-03): `System.SysUtils` and the units of
        // the containers (M4-40, `System.Contnrs` M5-14), `System.Classes` (M5-16), `System.Math` (M5-36),
        // `System.Character` (M5-38)
        let mut body = String::from_utf8_lossy(src).into_owned();
        for (full, short) in [
            ("System.Character", "Character"),
            ("System.Classes", "Classes"),
            ("System.Contnrs", "Contnrs"),
            ("System.DateUtils", "DateUtils"),
            ("System.Math", "Math"),
            ("System.Generics.Collections", "Generics.Collections"),
            ("System.Generics.Defaults", "Generics.Defaults"),
            ("System.StrUtils", "StrUtils"),
            ("System.SysUtils", "SysUtils"),
        ] {
            body = replace_ascii_case_insensitive(&body, full, short);
        }
        // FPC's `TReplaceFlags` is a set of an anonymous enumeration, so the values cannot be
        // qualified with `TReplaceFlag.` as Puy requires (`design/testing.md` 3.3)
        text.push_str(&replace_ascii_case_insensitive(&body, "TReplaceFlag.", ""));
        text
    }

    fn case_dir(case: &Case) -> PathBuf {
        let dir = PathBuf::from(TESTS_DIR).join(case.group);
        if case.package {
            // The directory named after the case that holds `puy.toml`
            return find_package(&dir, case.name).unwrap_or(dir);
        }
        let want = format!("{}.pas", case.name);
        find_file(&dir, &want)
            .map(|p| p.parent().unwrap().to_path_buf())
            .unwrap_or(dir)
    }

    fn find_package(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
        for e in std::fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if p.file_name().is_some_and(|n| n == name) && p.join("puy.toml").is_file() {
                return Some(p);
            }
            if let Some(f) = find_package(&p, name) {
                return Some(f);
            }
        }
        None
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
