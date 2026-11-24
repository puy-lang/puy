//! The Rust part of `Puy.OS`: the unit `Puy.OS.Native`, which the Pascal unit uses in its
//! `implementation` part (specification: `docs/spec/stdlib/Puy.OS/README.md`).
//!
//! - The process ID and the number of processors come from the host's `Os`; without it they are
//!   the fixed values of requirements §5.8 (0 and 1), not an exception.
//! - Writing an environment variable changes the isolate's own table, never the process's
//!   variables (`docs/design/stdlib.md` 3.4.3; `CallCtx::set_env_var`). Reading sees the table
//!   first, as `GetEnvironmentVariable` does.

#[puy_ext::pascal_module(unit = "Puy.OS.Native")]
pub mod native {
    use puy_ext::prelude::*;

    /// The process ID (`Os.process_id`); 0 without the OS information.
    #[pascal_function]
    pub fn process_id(ctx: &mut CallCtx<'_>) -> Result<u32, PasError> {
        match ctx.host().os.as_deref_mut() {
            Some(os) => os.process_id().map_err(PasError::from),
            None => Ok(0),
        }
    }

    /// The number of logical processors the process may use (`Os.cpu_count`); 1 without the OS
    /// information. Kept within `Integer`.
    #[pascal_function]
    pub fn processor_count(ctx: &mut CallCtx<'_>) -> Result<i32, PasError> {
        let n = match ctx.host().os.as_deref_mut() {
            Some(os) => os.cpu_count().map_err(PasError::from)?,
            None => 1,
        };
        Ok(i32::try_from(n.max(1)).unwrap_or(i32::MAX))
    }

    /// Sets an environment variable for this isolate.
    #[pascal_function]
    pub fn set_env(ctx: &mut CallCtx<'_>, name: &str, value: &str) -> Result<(), PasError> {
        ctx.set_env_var(name, Some(value))
    }

    /// Removes an environment variable for this isolate.
    #[pascal_function]
    pub fn remove_env(ctx: &mut CallCtx<'_>, name: &str) -> Result<(), PasError> {
        ctx.set_env_var(name, None)
    }

    /// One variable of the list.
    #[pascal_record(name = "TNativeEnvVar")]
    pub struct EnvVar {
        pub name: String,
        pub value: String,
    }

    /// Every environment variable as the isolate sees it.
    #[pascal_function]
    pub fn env_vars(ctx: &mut CallCtx<'_>) -> Result<Vec<EnvVar>, PasError> {
        Ok(ctx
            .env_vars()?
            .into_iter()
            .map(|(name, value)| EnvVar { name, value })
            .collect())
    }
}
