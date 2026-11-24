//! Host operations shared by the Pascal file system adapter and the standard library.

#[puy_ext::pascal_module(unit = "System.FileSystem.Native")]
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
        let path = path.to_string();
        let call = ctx.host().fs().map(|fs| fs.canonicalize(&path));
        async move {
            call.map_err(PasError::from)?.await.map_err(|e| {
                PasError::host(
                    &e,
                    format!("cannot resolve path '{path}': {}", e.fixed_text()),
                )
            })
        }
    }

    /// Creates a directory, with missing parents only if `Recursive` is true.
    #[pascal_function(cancel = "complete")]
    pub fn create_directory(
        ctx: &mut CallCtx<'_>,
        path: &str,
        recursive: bool,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        crate::system_ioutils::native::make_dir_with_parents(ctx, path, recursive)
    }
}
