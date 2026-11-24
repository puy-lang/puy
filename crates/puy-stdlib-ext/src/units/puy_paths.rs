//! The Rust part of `Puy.Paths`: the unit `Puy.Paths.Native`, which the Pascal unit uses in its
//! `implementation` part (specification: `docs/spec/stdlib/Puy.Paths/README.md`).
//!
//! Only `TFilePath.Resolve` needs it: resolving symbolic links is a question to the file system
//! (`Fs.canonicalize`) that `System.IOUtils` does not ask. The other operations of `TFilePath`
//! call `System.IOUtils`.

#[puy_ext::pascal_module(unit = "Puy.Paths.Native")]
pub mod native {
    use std::future::Future;

    use puy_ext::prelude::*;

    /// The absolute path of `Path` with every symbolic link, `.` and `..` resolved
    /// (`Fs.canonicalize`; suspendable, run to its end: requirements §7.1). A missing file system
    /// is the `ENotSupportedException` of its absence; a failure of the host (the path does not
    /// exist, …) is an `EInOutError` with its kind and code.
    #[pascal_function(cancel = "complete")]
    pub fn canonical_path(
        ctx: &mut CallCtx<'_>,
        path: &str,
    ) -> impl Future<Output = Result<String, PasError>> + 'static {
        crate::system_file_system::native::canonical_path(ctx, path)
    }
}
