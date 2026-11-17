//! Runs the `diff/` cases with Delphi's `dcc64` and compares the result with the recorded
//! `.out` / `.code` (`docs/design/testing.md` 3.4). The recording itself is FPC's (`record-fpc`);
//! this is a second check that the expectations hold for Delphi too. It never writes files under
//! `tests/`.
//!
//! Usage: `cargo run -p puy-testsuite --bin check-dcc -- [<case name>…]`.
//! The compiler version is pinned in this program; other versions stop with exit code 2.
//! Built for native targets only (it is run on Windows).

#[cfg(not(target_arch = "wasm32"))]
mod native {

    use std::process::{Command, ExitCode, Stdio};

    use puy_testsuite::{
        CASES, Case, diff_switches, find_case, normalize_exit_code, parse_config,
        replace_ascii_case_insensitive,
    };

    /// The banner of the compiler used for the check (Delphi XE4, Win64).
    const DCC_VERSION: &str = "compiler version 25.0";

    /// A unit put first in the program's `uses` so that `Output` writes UTF-8 (the text files
    /// otherwise convert strings to the system's ANSI code page).
    const OUTPUT_UNIT: &str = "PuyCheckOutput";
    const OUTPUT_UNIT_SOURCE: &str = "unit PuyCheckOutput;\r\n\
        // Added by check-dcc: standard input and output in UTF-8\r\n\
        interface\r\n\
        implementation\r\n\
        initialization\r\n\
        \x20 SetTextCodePage(Input, 65001);\r\n\
        \x20 SetTextCodePage(Output, 65001);\r\n\
        end.\r\n";

    /// Cases known to fail with Delphi XE4, and why (`design/testing.md` 3.4). Other failures, and
    /// a case on this list that passes (the list is out of date), make the check fail.
    pub(super) const KNOWN: &[(&str, &str)] = &[
        // Not in XE4: dynamic array constructors `[…]` and `+` (later versions)
        ("array_concat", "dynamic array constructor / `+`"),
        ("constructors", "dynamic array constructor"),
        ("dyn_copy", "dynamic array constructor"),
        ("dyn_set_length", "dynamic array constructor"),
        ("open_array_args", "dynamic array constructor"),
        // Delphi keeps two dynamic array types with the same element type apart
        (
            "dyn_sharing",
            "dynamic array constructor; `TArray<Integer>` vs `TInts`",
        ),
        // Delphi's case labels are 32-bit
        ("case_labels", "Int64 case labels"),
        ("case_stmt", "Int64 case labels"),
        // `as` from an object to an interface without a GUID
        ("interface_supports", "`as` to an interface without a GUID"),
        // Writing an enumeration value is a compile error in Delphi (types.md 8.1)
        ("enum_ops", "`Write` of an enumeration value"),
        ("subrange_ops", "`Write` of an enumeration value"),
        ("set_ops", "`Write` of an enumeration value"),
        // Results that differ from FPC and Puy; Puy keeps its rules (docs/delphi-diff.md)
        (
            "sysutils_builtins",
            "`TryStrToInt` leaves the parsed prefix in `Value`",
        ),
        (
            "subrange_range",
            "`Inc` stores the value before the range error",
        ),
        (
            "strings",
            "`'x':3` writes `?` first (UTF-8 output; not looked into)",
        ),
    ];

    pub fn main() -> ExitCode {
        let names: Vec<String> = std::env::args().skip(1).collect();
        let banner = match Command::new("dcc64").output() {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(e) => {
                eprintln!("check-dcc: cannot run dcc64: {e}");
                return ExitCode::from(2);
            }
        };
        let first_line = banner.lines().next().unwrap_or("").trim();
        if !first_line.contains(DCC_VERSION) {
            eprintln!("check-dcc: `{first_line}` found, but only {DCC_VERSION} is used");
            return ExitCode::from(2);
        }
        println!("{first_line}");
        let cases: Vec<&Case> = if names.is_empty() {
            CASES.iter().filter(|c| c.group == "diff").collect()
        } else {
            names
                .iter()
                .filter_map(|n| match find_case(n) {
                    Some(c) if c.group == "diff" => Some(c),
                    _ => {
                        eprintln!("check-dcc: no diff case named `{n}`");
                        None
                    }
                })
                .collect()
        };
        let mut compile_failed = Vec::new();
        let mut differs = Vec::new();
        let mut stale = Vec::new();
        for case in cases {
            let known = KNOWN.iter().find(|(n, _)| *n == case.name).map(|(_, r)| *r);
            let result = check(case);
            match (result, known) {
                (Ok(()), None) => println!("ok: {}", case.name),
                (Ok(()), Some(_)) => {
                    eprintln!("PASSES BUT LISTED AS KNOWN: {}", case.name);
                    stale.push(case.name);
                }
                (Err(Failure::Compile(e)), known) => {
                    eprintln!("COMPILE FAILED: {}: {e}", case.name);
                    if known.is_none() {
                        compile_failed.push(case.name);
                    }
                }
                (Err(Failure::Differs(e)), known) => {
                    eprintln!("DIFFERS: {}: {e}", case.name);
                    if known.is_none() {
                        differs.push(case.name);
                    }
                }
            }
            if let Some(reason) = known {
                println!("known: {} ({reason})", case.name);
            }
        }
        println!(
            "\nnew compile failures ({}): {}\nnew differences ({}): {}\nlisted as known but passing ({}): {}",
            compile_failed.len(),
            compile_failed.join(", "),
            differs.len(),
            differs.join(", "),
            stale.len(),
            stale.join(", ")
        );
        if compile_failed.is_empty() && differs.is_empty() && stale.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        }
    }

    enum Failure {
        Compile(String),
        Differs(String),
    }

    impl From<std::io::Error> for Failure {
        fn from(e: std::io::Error) -> Self {
            Failure::Compile(e.to_string())
        }
    }

    fn check(case: &Case) -> Result<(), Failure> {
        let src = case.source();
        let cfg = parse_config(src).map_err(Failure::Compile)?;
        let work = std::env::temp_dir().join(format!("puy-delphi-{}", case.name));
        // Start clean so that no stale `.dcu` from an earlier run is used
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work)?;
        let switches = diff_switches(&cfg);
        let (program, name) =
            insert_unit(&String::from_utf8_lossy(src), OUTPUT_UNIT).map_err(Failure::Compile)?;
        // Named after the program (the case's file name may differ from it)
        let pas = work.join(format!("{name}.dpr"));
        std::fs::write(&pas, delphi_source(&program, switches))?;
        std::fs::write(work.join(format!("{OUTPUT_UNIT}.pas")), OUTPUT_UNIT_SOURCE)?;
        // A package (`design/testing.md` 3.6): the units are flat under `src/`
        let unit_dir = work.join("src");
        if case.package {
            std::fs::create_dir_all(&unit_dir)?;
            for (name, bytes) in case.includes() {
                let Some(file) = name.strip_prefix("src/") else {
                    continue;
                };
                if !file.ends_with(".pas") {
                    continue;
                }
                if file.contains('/') {
                    return Err(Failure::Compile(format!(
                        "units in folders under src/ are not supported: {name}"
                    )));
                }
                std::fs::write(
                    unit_dir.join(file),
                    delphi_source(&String::from_utf8_lossy(bytes), switches),
                )?;
            }
        }
        let mut dcc = Command::new("dcc64");
        dcc.current_dir(&work).arg("-Q").arg("-B").arg("-CC");
        if !cfg.defines.is_empty() {
            dcc.arg(format!("-D{}", cfg.defines.join(";")));
        }
        if case.package {
            dcc.arg(format!("-U{}", unit_dir.display()));
        }
        dcc.arg(&pas);
        let out = dcc.output()?;
        if !out.status.success() {
            return Err(Failure::Compile(format!(
                "dcc64 failed:\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        let mut run = Command::new(work.join(format!("{name}.exe")));
        run.current_dir(&work);
        if let Some(a) = case.file("args") {
            for line in String::from_utf8_lossy(a).lines() {
                run.arg(line);
            }
        }
        run.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = run.spawn()?;
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("stdin is piped");
            let _ = stdin.write_all(case.file("in").unwrap_or(b""));
        }
        let out = child.wait_with_output()?;
        let code = out.status.code().unwrap_or(1);
        // A runtime error without `System.SysUtils` is reported as `Runtime error N at …`
        let stderr_text = String::from_utf8_lossy(&out.stderr);
        let code = normalize_exit_code(code, &stderr_text, &["Runtime error "]);
        // `WriteLn` ends lines with CR LF on Windows; the recording has LF
        let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        let expected_out = String::from_utf8_lossy(case.file("out").unwrap_or(b""));
        let expected_code: i32 = case
            .file("code")
            .and_then(|b| String::from_utf8_lossy(b).trim().parse().ok())
            .unwrap_or(0);
        if expected_out != stdout {
            return Err(Failure::Differs(format!(
                "stdout differs from .out:\n{stdout}\nstderr:\n{stderr_text}"
            )));
        }
        if expected_code != code {
            return Err(Failure::Differs(format!(
                "exit code {code} differs from .code ({expected_code})\nstderr:\n{stderr_text}"
            )));
        }
        Ok(())
    }

    /// The source given to dcc64: a UTF-8 byte order mark (dcc64 reads the source in the ANSI
    /// code page without one), the leading directives (3.3) and the same removal of the
    /// `TReplaceFlag.` qualifier as record-fpc's.
    fn delphi_source(src: &str, switches: &str) -> String {
        let mut text = format!("\u{FEFF}{{$SCOPEDENUMS ON}}{switches}\n");
        text.push_str(&replace_ascii_case_insensitive(src, "TReplaceFlag.", ""));
        text
    }

    /// Puts `unit` first in the `uses` clause after the program header, or adds a `uses` clause
    /// there, and returns the new source with the program's name. Nothing is added before the
    /// header's line ends, so the line numbers stay.
    pub(super) fn insert_unit(src: &str, unit: &str) -> Result<(String, String), String> {
        let b = src.as_bytes();
        let mut i = skip_trivia(b, 0);
        if !word_at(b, i, "program") {
            return Err("the case does not start with `program`".to_string());
        }
        let name_start = skip_trivia(b, i + "program".len());
        let name_len = b[name_start..]
            .iter()
            .position(|c| !(c.is_ascii_alphanumeric() || *c == b'_'))
            .unwrap_or(b.len() - name_start);
        let name = src[name_start..name_start + name_len].to_string();
        if name.is_empty() {
            return Err("no name after `program`".to_string());
        }
        while i < b.len() && b[i] != b';' {
            i = skip_trivia(b, i + 1);
        }
        if i >= b.len() {
            return Err("no `;` after the program header".to_string());
        }
        let after_header = i + 1;
        let next = skip_trivia(b, after_header);
        let mut out = String::with_capacity(src.len() + unit.len() + 8);
        if word_at(b, next, "uses") {
            let at = next + "uses".len();
            out.push_str(&src[..at]);
            out.push_str(&format!(" {unit},"));
            out.push_str(&src[at..]);
        } else {
            out.push_str(&src[..after_header]);
            out.push_str(&format!(" uses {unit};"));
            out.push_str(&src[after_header..]);
        }
        Ok((out, name))
    }

    /// Skips white space and comments (`//`, `{ }`, `(* *)`) from `i`.
    fn skip_trivia(b: &[u8], mut i: usize) -> usize {
        loop {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if b[i..].starts_with(b"//") {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            } else if b[i..].starts_with(b"{") {
                i = b[i..]
                    .iter()
                    .position(|&c| c == b'}')
                    .map_or(b.len(), |p| i + p + 1);
            } else if b[i..].starts_with(b"(*") {
                i = b[i..]
                    .windows(2)
                    .position(|w| w == b"*)")
                    .map_or(b.len(), |p| i + p + 2);
            } else {
                return i;
            }
        }
    }

    /// Whether the word `w` (ASCII case-insensitive) is at `i` and ends there.
    fn word_at(b: &[u8], i: usize, w: &str) -> bool {
        let end = i + w.len();
        end <= b.len()
            && b[i..end].eq_ignore_ascii_case(w.as_bytes())
            && b.get(end)
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    native::main()
}

/// On wasm32, only make it build (`cargo test --target wasm32-wasip2` also builds bins).
#[cfg(target_arch = "wasm32")]
fn main() -> std::process::ExitCode {
    eprintln!("check-dcc runs only on native targets");
    std::process::ExitCode::from(2)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::native::{KNOWN, insert_unit};

    // The output unit goes first in an existing `uses` clause, after comments between the
    // header and the clause, without adding a line.
    #[test]
    fn unit_goes_first_in_existing_uses() {
        let src = "program P;\n// note\nuses\n  System.SysUtils;\nbegin end.\n";
        assert_eq!(
            insert_unit(src, "U").unwrap().0,
            "program P;\n// note\nuses U,\n  System.SysUtils;\nbegin end.\n"
        );
    }

    // Without a `uses` clause, one is added on the header's line; a parameter list and a
    // leading settings comment are skipped.
    #[test]
    fn uses_clause_added_after_header() {
        let src = "// puy-test: release\nprogram P(input, output); { x }\nvar Uses_: Integer;\n";
        assert_eq!(
            insert_unit(src, "U").unwrap(),
            (
                "// puy-test: release\nprogram P(input, output); uses U; { x }\nvar Uses_: Integer;\n"
                    .to_string(),
                "P".to_string()
            )
        );
    }

    // A source that is not a program (a script) is an error, not a silent skip.
    #[test]
    fn non_program_is_error() {
        assert!(insert_unit("WriteLn('x');\n", "U").is_err());
    }

    // Every diff case is a program whose header the output unit can be inserted after.
    #[test]
    fn every_diff_case_takes_the_unit() {
        for case in puy_testsuite::CASES.iter().filter(|c| c.group == "diff") {
            let src = String::from_utf8_lossy(case.source());
            let result = insert_unit(&src, "U");
            assert!(result.is_ok(), "{}: {result:?}", case.name);
        }
    }

    // Every name on the known list is a diff case, so a typo does not hide a failure.
    #[test]
    fn known_cases_exist() {
        for (name, _) in KNOWN {
            assert!(
                puy_testsuite::find_case(name).is_some_and(|c| c.group == "diff"),
                "{name}"
            );
        }
    }
}
