//! Runner for the differential, golden, and lifetime tests (`docs/design/testing.md`).
//!
//! Cases are embedded at build time (`build.rs`). The comparison is done by this crate's integration test (`tests/cases.rs`),
//! which calls `puy-core`'s `compile` and `Isolate` directly (no process is spawned).

/// One case (a set of files sharing the same stem).
#[derive(Debug)]
pub struct Case {
    /// `diff` / `golden` / `lifetime`
    pub group: &'static str,
    pub name: &'static str,
    /// (file name, contents)
    pub files: &'static [(&'static str, &'static [u8])],
}

include!(concat!(env!("OUT_DIR"), "/cases.rs"));

impl Case {
    pub fn file(&self, ext: &str) -> Option<&'static [u8]> {
        let want = format!("{}.{ext}", self.name);
        self.files.iter().find(|(n, _)| *n == want).map(|(_, b)| *b)
    }

    pub fn source(&self) -> &'static [u8] {
        self.file("pas").expect("case has a .pas file")
    }

    /// The `.inc` files (`<stem>.inc`, `<stem>.*.inc`).
    pub fn includes(&self) -> Vec<(&'static str, &'static [u8])> {
        self.files
            .iter()
            .filter(|(n, _)| n.ends_with(".inc"))
            .copied()
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
            _ => {
                if let Some(name) = item.strip_prefix("define=") {
                    cfg.defines.push(name.to_string());
                } else if let Some(v) = item.strip_prefix("stack-limit=") {
                    cfg.stack_limit = Some(parse_size(v)?);
                } else if let Some(v) = item.strip_prefix("memory-limit=") {
                    cfg.memory_limit = Some(parse_size(v)?);
                } else {
                    return Err(format!("unknown puy-test item `{item}`"));
                }
            }
        }
    }
    Ok(cfg)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn cases_embedded() {
        assert!(find_case("hello").is_some());
    }
}
