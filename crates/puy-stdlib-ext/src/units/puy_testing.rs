//! The Rust part of `Puy.Testing`: the unit `Puy.Testing.Native`, which the Pascal unit uses in
//! its `implementation` part (specification: `docs/spec/stdlib/Puy.Testing/README.md`).
//!
//! - `AssertSnapshot` hands the value to the test runner (`HostSet::test`), which alone reads and
//!   writes the snapshot files (`docs/spec/test.md` 12). Outside `puy test` there is no runner:
//!   `ENotSupportedException`.
//! - The position written in the snapshot is the call of `AssertSnapshot`: the frame of the
//!   routine that called it (`CallCtx::frames`).

#[puy_ext::pascal_module(unit = "Puy.Testing.Native")]
pub mod native {
    use puy_ext::prelude::*;
    use puy_ext::{HostError, HostErrorKind};

    /// Compares `actual` with the snapshot of the running test, named `name` if `named`. Gives
    /// the message of the failure, or an empty string when the snapshot matches (or was
    /// written).
    #[pascal_function]
    pub fn snapshot(
        ctx: &mut CallCtx<'_>,
        actual: &str,
        name: &str,
        named: bool,
    ) -> Result<String, PasError> {
        if named {
            check_name(name)?;
        }
        // The innermost frame is the `AssertSnapshot` of `Puy.Testing` that called this (each
        // overload calls it directly); the next one made the call of `AssertSnapshot`
        let frames = ctx.frames();
        let caller = frames.get(1).or(frames.first());
        let (file, line) = caller.map_or(("", 0), |f| (f.file.as_str(), f.line));
        let Some(runner) = ctx.host().test.as_deref_mut() else {
            let e = HostError::new(HostErrorKind::Unsupported, "no test runner");
            return Err(PasError::host(
                &e,
                "AssertSnapshot is available only in a test that `puy test` runs",
            ));
        };
        let failure = runner
            .snapshot(named.then_some(name), actual, file, line)
            .map_err(|e| PasError::host(&e, e.message.clone()))?;
        Ok(failure.unwrap_or_default())
    }

    /// The name of a snapshot becomes part of a file name (`docs/spec/stdlib/Puy.Testing/README.md`
    /// 5): `EArgumentException` for an empty one, one with a character that a file name on some
    /// system cannot hold, or one that begins or ends with a space or a dot.
    fn check_name(name: &str) -> Result<(), PasError> {
        let bad_char = name
            .chars()
            .find(|&c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'));
        let reason = if name.is_empty() {
            Some("is empty".to_string())
        } else if let Some(c) = bad_char {
            Some(format!("contains #{}", c as u32))
        } else if name.starts_with([' ', '.']) || name.ends_with([' ', '.']) {
            Some("begins or ends with a space or a dot".to_string())
        } else {
            None
        };
        match reason {
            Some(r) => Err(PasError::argument(format!(
                "the name of a snapshot {r}: '{name}'"
            ))),
            None => Ok(()),
        }
    }
}
