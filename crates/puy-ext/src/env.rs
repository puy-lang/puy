//! An isolate's own changes to the environment variables (`docs/design/stdlib.md` 3.4.3,
//! `docs/spec/stdlib/Puy.OS/README.md`).
//!
//! Writing a variable never reaches the host: the isolate keeps a table of the names it set or
//! removed, and reading (`GetEnvironmentVariable`, the list) looks there before asking the host's
//! `Env`. The process's variables stay as they were, so isolates do not race on them nor see
//! each other's changes. The VM owns one table per isolate and reads it for
//! `GetEnvironmentVariable`; extensions reach it through `CallCtx::env_var` and its siblings.

use puy_host::{HostError, HostSet};

use crate::PasError;

/// The changes of one isolate, in the order each name was first changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EnvTable {
    /// The name as the first change spelled it, and its value (`None`: removed).
    changes: Vec<(String, Option<String>)>,
}

/// Whether `name` can name a variable: not empty, without `=` and `#0` (the names the host is
/// never asked about; `docs/spec/bytecode.md` 2.6). [`EnvTable::set`] refuses the others.
pub fn is_env_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0'])
}

/// `s` between single quotes, a quote in it doubled (`QuotedStr`).
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Whether the host's names ignore case (Windows); without an `Env`, they do not.
fn ignores_case(host: &HostSet) -> bool {
    host.env.as_deref().is_some_and(|e| e.ignores_case())
}

/// A character in upper case by a one-to-one mapping, as a host that ignores case compares; one
/// whose upper case is several characters (`ß`) stays as it is.
fn fold(c: char) -> char {
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// Whether two names name the same variable.
fn same_name(a: &str, b: &str, ignore_case: bool) -> bool {
    if ignore_case {
        a.chars().map(fold).eq(b.chars().map(fold))
    } else {
        a == b
    }
}

impl EnvTable {
    /// The change of `name`, if the isolate made one.
    fn change(&self, name: &str, ignore_case: bool) -> Option<&Option<String>> {
        self.changes
            .iter()
            .find(|(n, _)| same_name(n, name, ignore_case))
            .map(|(_, v)| v)
    }

    /// The value of `name` as the isolate sees it: its own change, else the host's value
    /// (`Ok(None)` when unset, when there is no `Env`, or when the name cannot name a variable).
    pub fn get(&self, host: &mut HostSet, name: &str) -> Result<Option<String>, HostError> {
        if !is_env_name(name) {
            return Ok(None);
        }
        if let Some(v) = self.change(name, ignores_case(host)) {
            return Ok(v.clone());
        }
        match host.env.as_deref_mut() {
            Some(env) => env.get(name),
            None => Ok(None),
        }
    }

    /// Sets `name` to `value`, or removes it (`None`), for this isolate only. A name that cannot
    /// name a variable ([`is_env_name`]), or a value with `#0`, is `PasError::argument`
    /// (`EArgumentException`; the name in the message is quoted as `QuotedStr` does).
    pub fn set(&mut self, host: &HostSet, name: &str, value: Option<&str>) -> Result<(), PasError> {
        if !is_env_name(name) {
            return Err(PasError::argument(format!(
                "invalid environment variable name {}",
                quoted(name)
            )));
        }
        if value.is_some_and(|v| v.contains('\0')) {
            return Err(PasError::argument(format!(
                "the value of the environment variable {} contains #0",
                quoted(name)
            )));
        }
        let ignore_case = ignores_case(host);
        let value = value.map(str::to_owned);
        match self
            .changes
            .iter_mut()
            .find(|(n, _)| same_name(n, name, ignore_case))
        {
            Some((_, v)) => *v = value,
            None => self.changes.push((name.to_owned(), value)),
        }
        Ok(())
    }

    /// Every variable as the isolate sees it: the host's in its order (a changed one with its new
    /// value, a removed one left out), then the names only the isolate set, in the order they
    /// were first set.
    pub fn vars(&self, host: &mut HostSet) -> Result<Vec<(String, String)>, HostError> {
        let ignore_case = ignores_case(host);
        let from_host = match host.env.as_deref_mut() {
            Some(env) => env.vars()?,
            None => Vec::new(),
        };
        let mut seen = vec![false; self.changes.len()];
        let mut out = Vec::with_capacity(from_host.len() + self.changes.len());
        for (name, value) in from_host {
            match self
                .changes
                .iter()
                .position(|(n, _)| same_name(n, &name, ignore_case))
            {
                Some(i) => {
                    seen[i] = true;
                    if let Some(v) = &self.changes[i].1 {
                        out.push((name, v.clone()));
                    }
                }
                None => out.push((name, value)),
            }
        }
        for ((name, value), seen) in self.changes.iter().zip(seen) {
            if let (Some(v), false) = (value, seen) {
                out.push((name.clone(), v.clone()));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MapEnv(Vec<(String, String)>, bool);

    impl puy_host::Env for MapEnv {
        fn get(&mut self, name: &str) -> Result<Option<String>, HostError> {
            let ic = self.1;
            Ok(self
                .0
                .iter()
                .find(|(k, _)| same_name(k, name, ic))
                .map(|(_, v)| v.clone()))
        }

        fn vars(&mut self) -> Result<Vec<(String, String)>, HostError> {
            Ok(self.0.clone())
        }

        fn ignores_case(&self) -> bool {
            self.1
        }
    }

    fn host(ignore_case: bool) -> HostSet {
        HostSet {
            env: Some(Box::new(MapEnv(
                vec![("Path".into(), "/bin".into()), ("HOME".into(), "/h".into())],
                ignore_case,
            ))),
            ..HostSet::empty()
        }
    }

    fn names(vars: &[(String, String)]) -> String {
        vars.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(";")
    }

    // On a host whose names ignore case (Windows), a change of `PATH` is a change of the host's
    // `Path`: it is read back under either spelling and listed under the host's spelling.
    #[test]
    fn changes_follow_the_hosts_case_rule() {
        let mut h = host(true);
        let mut t = EnvTable::default();
        t.set(&h, "PATH", Some("/usr/bin")).unwrap();
        t.set(&h, "home", None).unwrap();
        assert_eq!(t.get(&mut h, "path").unwrap().as_deref(), Some("/usr/bin"));
        assert_eq!(t.get(&mut h, "HOME").unwrap(), None);
        assert_eq!(names(&t.vars(&mut h).unwrap()), "Path=/usr/bin");
    }

    // On a host whose names keep case, `PATH` is another variable than `Path`.
    #[test]
    fn case_sensitive_names_are_distinct() {
        let mut h = host(false);
        let mut t = EnvTable::default();
        t.set(&h, "PATH", Some("/usr/bin")).unwrap();
        assert_eq!(t.get(&mut h, "Path").unwrap().as_deref(), Some("/bin"));
        // A character whose upper case is several characters is not folded
        assert!(!same_name("STRASSE", "straße", true));
        assert!(same_name("ÄRGER", "ärger", true));
        assert_eq!(
            names(&t.vars(&mut h).unwrap()),
            "Path=/bin;HOME=/h;PATH=/usr/bin"
        );
    }
}
