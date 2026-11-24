//! Runner for the differential, golden, and lifetime tests (`docs/design/testing.md`).
//!
//! Cases are embedded at build time (`build.rs`). The comparison is done by this crate's integration test (`tests/cases.rs`),
//! which calls `puy-core`'s `compile` and `Isolate` directly (no process is spawned).

pub mod ext;
pub mod memfs;

/// One case (a set of files sharing the same stem).
#[derive(Debug)]
pub struct Case {
    /// `diff` / `golden` / `lifetime`
    pub group: &'static str,
    pub name: &'static str,
    /// The stem of the source and expectation files: the case name, or for a package `main`
    /// (or the path of its one test unit, `src/Calc_test`).
    pub stem: &'static str,
    /// A directory with `puy.toml` (`docs/design/testing.md` 3.6): `files` are named by their
    /// path inside it, and the file compiled is `main.pas` (or the test unit).
    pub package: bool,
    /// (file name, contents)
    pub files: &'static [(&'static str, &'static [u8])],
}

include!(concat!(env!("OUT_DIR"), "/cases.rs"));

impl Case {
    pub fn file(&self, ext: &str) -> Option<&'static [u8]> {
        let want = format!("{}.{ext}", self.stem);
        self.files.iter().find(|(n, _)| *n == want).map(|(_, b)| *b)
    }

    pub fn source(&self) -> &'static [u8] {
        self.file("pas").expect("case has a .pas file")
    }

    /// The files the compiler can read besides the program: the `.inc` files (`<stem>.inc`,
    /// `<stem>.*.inc`), and for a package every file inside it except the expected results of
    /// `puy fix` (`*.fixed.pas`).
    pub fn includes(&self) -> Vec<(&'static str, &'static [u8])> {
        self.files
            .iter()
            .filter(|(n, _)| {
                if self.package {
                    !n.ends_with(".fixed.pas")
                } else {
                    n.ends_with(".inc")
                }
            })
            .copied()
            .collect()
    }
}

impl Case {
    /// The units of a package with an expected result of `puy fix`: (the unit's path, the
    /// expectation `<path without .pas>.fixed.pas`). The program's is `file("fixed.pas")`.
    pub fn fixed_units(&self) -> Vec<(String, &'static [u8])> {
        if !self.package {
            return Vec::new();
        }
        let main = format!("{}.fixed.pas", self.stem);
        self.files
            .iter()
            .filter(|(n, _)| *n != main)
            .filter_map(|(n, b)| {
                n.strip_suffix(".fixed.pas")
                    .map(|stem| (format!("{stem}.pas"), *b))
            })
            .collect()
    }
}

pub fn find_case(name: &str) -> Option<&'static Case> {
    CASES.iter().find(|c| c.name == name)
}

/// Case settings (`// puy-test: item, …` at the top of the `.pas`; 3.2).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CaseConfig {
    pub native_only: bool,
    pub release: bool,
    pub defines: Vec<String>,
    pub no_console: bool,
    pub no_env: bool,
    pub stack_limit: Option<usize>,
    pub memory_limit: Option<usize>,
    /// `stdout-buffering=line|full|none` (`IsolateConfig.stdout_buffering`; `full` if not given).
    pub stdout_buffering: Option<String>,
    /// `merge-stderr`: standard error goes into the same capture as standard output, in the
    /// order of the host writes (`.out` shows the interleaving, like `2>&1`).
    pub merge_stderr: bool,
    /// `test-ext`: the program is compiled and run with the test extensions ([`ext`];
    /// `docs/design/testing.md` 3.2). The `puy` command has none, so it does not run the case.
    pub test_ext: bool,
    /// `test-fs`: the `Fs` and `Os` of the `HostSet` are a file system in memory and a fixed OS
    /// ([`memfs`]); without it both are `None`.
    pub test_fs: bool,
    /// `no-fs`: the case depends on `Fs` and `Os` being `None`, as the runner leaves them without
    /// `test-fs` (the `puy` command, which has them, does not run it).
    pub no_fs: bool,
    /// `os=NAME`: the kind of OS the fixed `Os` of `test-fs` reports (`linux` if not given).
    pub os: Option<String>,
    /// `absent=unsupported`: a missing capability is one the target does not have
    /// (`Absent::Unsupported`) rather than one not granted.
    pub absent_unsupported: bool,
    /// `clock=NANOS`: the `HostSet` has a clock stopped at `NANOS` nanoseconds since the UNIX
    /// epoch ([`FixedClock`]); without it the clock is `None`.
    pub clock: Option<i64>,
    /// `tz-offset=SECONDS`: the offset of the local time from UTC the fixed clock reports
    /// (east positive); without it the clock cannot tell (`None`, so UTC).
    pub tz_offset: Option<i32>,
    /// `tz-change=NANOS:SECONDS` (may be repeated): from the instant `NANOS` (nanoseconds since
    /// the UNIX epoch) on, the fixed clock reports the offset `SECONDS` instead, so that a case
    /// can cross a change of daylight saving time. Written in the order of time (an error
    /// otherwise); the latest change at or before an instant wins.
    pub tz_changes: Vec<(i64, i32)>,
    /// `mono=NANOS`: the first reading of the monotonic clock of the fixed clock (0 if not
    /// given).
    pub mono: u64,
    /// `mono-step=NANOS`: how far the monotonic clock of the fixed clock moves after each
    /// reading (0 if not given: it stays stopped), so that a case can see time pass.
    pub mono_step: u64,
    /// `no-clock`: the case depends on the clock being `None`, as the runner leaves it without
    /// `clock=` (the `puy` command, which has one, does not run it).
    pub no_clock: bool,
    /// `random=SEED`: the `HostSet` has a source of random numbers that gives the 8 bytes of
    /// `SEED` (little-endian) over and over ([`FixedRandom`]); without it the source is `None`.
    pub random: Option<i64>,
    /// `no-random`: the case depends on the source of random numbers being `None`, as the runner
    /// leaves it without `random=` (the `puy` command, which has one, does not run it).
    pub no_random: bool,
}

/// Parses the settings. Unknown items are an `Err`.
pub fn parse_config(src: &[u8]) -> Result<CaseConfig, String> {
    let mut cfg = CaseConfig::default();
    let mut i = 0;
    if src.starts_with(&[0xEF, 0xBB, 0xBF]) {
        i = 3;
    }
    if src[i..].starts_with(b"#!") {
        while i < src.len() && !matches!(src[i], b'\n' | b'\r') {
            i += 1;
        }
        if src.get(i) == Some(&b'\r') {
            i += 1;
        }
        if src.get(i) == Some(&b'\n') {
            i += 1;
        }
    }
    let rest = &src[i..];
    let Some(line_end) = rest
        .iter()
        .position(|b| matches!(b, b'\n' | b'\r'))
        .or(Some(rest.len()))
    else {
        return Ok(cfg);
    };
    let line = String::from_utf8_lossy(&rest[..line_end]);
    let Some(items) = line.trim().strip_prefix("// puy-test:") else {
        return Ok(cfg);
    };
    for item in items.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        match item {
            "native-only" => cfg.native_only = true,
            "release" => cfg.release = true,
            "no-console" => cfg.no_console = true,
            "no-env" => cfg.no_env = true,
            "merge-stderr" => cfg.merge_stderr = true,
            "test-ext" => cfg.test_ext = true,
            "test-fs" => cfg.test_fs = true,
            "no-fs" => cfg.no_fs = true,
            "no-clock" => cfg.no_clock = true,
            "no-random" => cfg.no_random = true,
            "absent=unsupported" => cfg.absent_unsupported = true,
            _ => {
                if let Some(name) = item.strip_prefix("define=") {
                    cfg.defines.push(name.to_string());
                } else if let Some(v) = item.strip_prefix("stack-limit=") {
                    cfg.stack_limit = Some(parse_size(v)?);
                } else if let Some(v) = item.strip_prefix("clock=") {
                    cfg.clock = Some(v.parse().map_err(|_| format!("invalid clock `{v}`"))?);
                } else if let Some(v) = item.strip_prefix("mono=") {
                    cfg.mono = v.parse().map_err(|_| format!("invalid mono `{v}`"))?;
                } else if let Some(v) = item.strip_prefix("mono-step=") {
                    cfg.mono_step = v.parse().map_err(|_| format!("invalid mono-step `{v}`"))?;
                } else if let Some(v) = item.strip_prefix("tz-offset=") {
                    cfg.tz_offset =
                        Some(v.parse().map_err(|_| format!("invalid tz-offset `{v}`"))?);
                } else if let Some(v) = item.strip_prefix("tz-change=") {
                    let change = v
                        .split_once(':')
                        .and_then(|(at, off)| Some((at.parse().ok()?, off.parse().ok()?)))
                        .ok_or_else(|| format!("invalid tz-change `{v}`"))?;
                    // In the order of time, so that the offset at an instant is the latest
                    // change at or before it
                    if cfg.tz_changes.last().is_some_and(|(at, _)| *at >= change.0) {
                        return Err(format!("tz-change `{v}` is not after the one before it"));
                    }
                    cfg.tz_changes.push(change);
                } else if let Some(v) = item.strip_prefix("os=") {
                    cfg.os = Some(v.to_string());
                } else if let Some(v) = item.strip_prefix("random=") {
                    cfg.random = Some(v.parse().map_err(|_| format!("invalid random `{v}`"))?);
                } else if let Some(v) = item.strip_prefix("memory-limit=") {
                    cfg.memory_limit = Some(parse_size(v)?);
                } else if let Some(v) = item.strip_prefix("stdout-buffering=") {
                    if !matches!(v, "line" | "full" | "none") {
                        return Err(format!("invalid stdout-buffering `{v}`"));
                    }
                    cfg.stdout_buffering = Some(v.to_string());
                } else {
                    return Err(format!("unknown puy-test item `{item}`"));
                }
            }
        }
    }
    // The settings of the fixed clock mean nothing without it: a slip of the case, not a choice
    let clock_settings = cfg.tz_offset.is_some()
        || !cfg.tz_changes.is_empty()
        || cfg.mono != 0
        || cfg.mono_step != 0;
    if clock_settings && cfg.clock.is_none() {
        return Err("tz-offset, tz-change, mono and mono-step need clock=".into());
    }
    Ok(cfg)
}

/// A clock stopped at one instant (`clock=NANOS`, `tz-offset=SECONDS`, `tz-change=NANOS:SECONDS`,
/// `mono=NANOS`, `mono-step=NANOS`; `docs/design/testing.md` 3.2): every read of the wall clock
/// gives the same time, and the monotonic clock moves by a fixed step at each read, so that a case
/// that prints the time is deterministic.
pub struct FixedClock {
    pub nanos: i64,
    /// The offset before the first change (`None`: the clock cannot tell)
    pub offset: Option<i32>,
    /// The instants (UNIX nanoseconds) from which the offset is another
    pub changes: Vec<(i64, i32)>,
    /// The next reading of the monotonic clock
    pub mono: u64,
    /// How far the monotonic clock moves after each reading
    pub mono_step: u64,
}

impl puy_host::Clock for FixedClock {
    fn wall_nanos(&mut self) -> Result<i64, puy_host::HostError> {
        Ok(self.nanos)
    }

    fn monotonic_nanos(&mut self) -> Result<u64, puy_host::HostError> {
        let now = self.mono;
        self.mono = now.wrapping_add(self.mono_step);
        Ok(now)
    }

    fn local_offset(&mut self, utc_nanos: i64) -> Result<Option<i32>, puy_host::HostError> {
        Ok(self
            .changes
            .iter()
            .rev()
            .find(|(at, _)| *at <= utc_nanos)
            .map_or(self.offset, |(_, offset)| Some(*offset)))
    }

    // Does not wait: moves both clocks by the time asked, so that a case that sleeps on the
    // host's clock is quick and deterministic
    fn sleep(&mut self, nanos: u64) -> puy_host::HostFuture<()> {
        self.nanos = self.nanos.wrapping_add(nanos as i64);
        self.mono = self.mono.wrapping_add(nanos);
        puy_host::ready(Ok(()))
    }
}

/// A source of random numbers that repeats one value (`random=SEED`; `docs/design/testing.md`
/// 3.2): `fill` writes the 8 bytes of the value, little-endian, over and over, so that a seed
/// taken from it is the value itself.
pub struct FixedRandom(pub i64);

impl puy_host::Random for FixedRandom {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), puy_host::HostError> {
        let bytes = self.0.to_le_bytes();
        for (b, v) in buf.iter_mut().zip(bytes.iter().cycle()) {
            *b = *v;
        }
        Ok(())
    }
}

/// A size such as `64k` or `1m`.
pub fn parse_size(s: &str) -> Result<usize, String> {
    let (num, mul) = if let Some(n) = s.strip_suffix('k') {
        (n, 1024)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 1024 * 1024)
    } else {
        (s, 1)
    };
    num.parse::<usize>()
        .map(|n| n * mul)
        .map_err(|_| format!("invalid size `{s}`"))
}

/// The switches put before a `diff/` case when it is compiled by FPC or Delphi
/// (`docs/design/testing.md` 3.3): Puy's debug defaults, or all off for `release`.
pub fn diff_switches(cfg: &CaseConfig) -> &'static str {
    if cfg.release {
        "{$R-,Q-,C-}"
    } else {
        "{$R+,Q+,C+}"
    }
}

/// Normalizes the exit code of a `diff/` case built by FPC or Delphi to Puy's
/// (`docs/design/testing.md` 3.3): 217 (unhandled exception), and any other non-zero code with a
/// runtime error reported on standard error (one of `error_markers`), become 1.
pub fn normalize_exit_code(code: i32, stderr: &str, error_markers: &[&str]) -> i32 {
    let runtime_error = error_markers.iter().any(|m| stderr.contains(m));
    if code == 217 || (code != 0 && runtime_error) {
        1
    } else {
        code
    }
}

/// Replaces `pat` in `text` (ASCII case-insensitive) with `rep`.
pub fn replace_ascii_case_insensitive(text: &str, pat: &str, rep: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    // `tz-change=` may be repeated in the order of time; a malformed one, or one not after the
    // one before it, is an error.
    #[test]
    fn tz_change_parsing() {
        let c = parse_config(
            b"// puy-test: clock=1, tz-offset=3600, tz-change=-5:7200, tz-change=10:0\n",
        )
        .unwrap();
        assert_eq!(c.tz_changes, [(-5, 7200), (10, 0)]);
        for bad in [
            "tz-change=10",
            "tz-change=x:1",
            "tz-change=1:y",
            "tz-change=10:7200, tz-change=-5:0",
            "tz-change=10:7200, tz-change=10:0",
        ] {
            let src = format!("// puy-test: {bad}\n");
            assert!(parse_config(src.as_bytes()).is_err(), "{bad}");
        }
    }

    // The settings of the fixed clock are an error without `clock=`, which they configure.
    #[test]
    fn clock_settings_need_a_clock() {
        let c = parse_config(b"// puy-test: clock=0, mono=5, mono-step=7\n").unwrap();
        assert_eq!((c.mono, c.mono_step), (5, 7));
        for bad in ["tz-offset=3600", "tz-change=1:0", "mono=5", "mono-step=7"] {
            let src = format!("// puy-test: {bad}\n");
            assert!(parse_config(src.as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn config_parsing() {
        let c = parse_config(b"// puy-test: native-only, release, define=FOO, stack-limit=64k\nx")
            .unwrap();
        assert!(c.native_only && c.release);
        assert_eq!(c.defines, ["FOO"]);
        assert_eq!(c.stack_limit, Some(65536));
        assert!(parse_config(b"// puy-test: bogus\n").is_err());
        assert!(
            parse_config(b"#!/usr/bin/env puy\n// puy-test: no-env\n")
                .unwrap()
                .no_env
        );
        assert_eq!(parse_config(b"program X;").unwrap(), CaseConfig::default());
    }

    #[test]
    fn stdio_config_items() {
        // `stdout-buffering=` takes the three fixed modes only; `merge-stderr` is a flag
        let c = parse_config(b"// puy-test: merge-stderr, stdout-buffering=line\n").unwrap();
        assert!(c.merge_stderr);
        assert_eq!(c.stdout_buffering.as_deref(), Some("line"));
        assert!(parse_config(b"// puy-test: stdout-buffering=auto\n").is_err());
    }

    #[test]
    fn cases_embedded() {
        assert!(find_case("hello").is_some());
    }
}
