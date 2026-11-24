//! The Rust part of `System.IOUtils`: the unit `System.IOUtils.Native`, which the Pascal unit
//! uses in its `implementation` part (`docs/design/stdlib.md` 3.1; specification:
//! `docs/spec/stdlib/System.IOUtils/README.md`).
//!
//! Each function is one call of the host's file system (`Fs`; suspendable, run to its end:
//! `cancel = "complete"`, requirements §7.1) or of its OS information (`Os`). A missing file
//! system is the `ENotSupportedException` of its absence; any other failure of the host is an
//! `EInOutError` with its kind and code, whose message names the operation and the path. The
//! Pascal unit checks the arguments first and turns the failures it names after Delphi's
//! exceptions (`EFileNotFoundException`, …) from these.

#[puy_ext::pascal_module(unit = "System.IOUtils.Native")]
pub mod native {
    use std::future::Future;

    use puy_ext::prelude::*;
    use puy_ext::{FileTimes, FileType, HostError, HostFuture};

    /// The exception of the host's failure `e` in `what`, the operation with its paths
    /// (`list directory 'd'`, `copy file 'a' to 'b'`).
    fn failed(e: &HostError, what: &str) -> PasError {
        PasError::host(e, format!("cannot {what}: {}", e.fixed_text()))
    }

    /// The operation `op` on `path` as `failed` names it.
    fn on(op: &str, path: &str) -> String {
        format!("{op} '{path}'")
    }

    /// The operation `op` from `source` to `dest` as `failed` names it: a failure may be on
    /// either side, so both paths are named.
    fn between(op: &str, source: &str, dest: &str) -> String {
        format!("{op} '{source}' to '{dest}'")
    }

    /// Waits for `call` (or raises the exception of a missing file system) and turns a failure
    /// of the host into the exception of `what`.
    async fn run<T: 'static>(
        call: Result<HostFuture<T>, HostError>,
        what: String,
    ) -> Result<T, PasError> {
        call.map_err(PasError::from)?.await.map_err(|e| failed(&e, &what))
    }

    /// The number of a kind of file, as `FileKind` of the Pascal unit: 1 a file, 2 a directory,
    /// 3 a symbolic link, 4 anything else.
    fn kind(t: FileType) -> i32 {
        match t {
            FileType::File => 1,
            FileType::Dir => 2,
            FileType::Symlink => 3,
            FileType::Other => 4,
        }
    }

    /// One entry of a listing.
    #[pascal_record(name = "TNativeDirEntry")]
    pub struct Entry {
        pub name: String,
        pub kind: i32,
    }

    /// The entries of the directory `Path` (`Fs.read_dir`), sorted by name in the order of the
    /// code points (the same on every target; `.` and `..` are never listed).
    #[pascal_function(cancel = "complete")]
    pub fn list_dir(
        ctx: &mut CallCtx<'_>,
        path: &str,
    ) -> impl Future<Output = Result<Vec<Entry>, PasError>> + 'static {
        let path = path.to_string();
        let call = ctx.host().fs().map(|fs| fs.read_dir(&path));
        async move {
            let what = on("list directory", &path);
            let mut reader = run(call, what.clone()).await?;
            let mut entries = Vec::new();
            loop {
                let batch = reader
                    .next_batch(256)
                    .await
                    .map_err(|e| failed(&e, &what))?;
                if batch.is_empty() {
                    break;
                }
                entries.extend(batch.into_iter().map(|e| Entry {
                    name: e.name,
                    kind: kind(e.file_type),
                }));
            }
            entries.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(entries)
        }
    }

    /// The attributes of a path. A time the file system does not keep is `Has… = False`.
    #[pascal_record(name = "TNativeFileInfo")]
    pub struct Info {
        pub kind: i32,
        pub size: i64,
        pub read_only: bool,
        pub has_modified: bool,
        pub modified: i64,
        pub has_accessed: bool,
        pub accessed: i64,
        pub has_created: bool,
        pub created: i64,
    }

    /// The attributes of `Path` (`Fs.metadata`); with `FollowLink`, of what a symbolic link
    /// points to.
    #[pascal_function(cancel = "complete")]
    pub fn file_info(
        ctx: &mut CallCtx<'_>,
        path: &str,
        follow_link: bool,
    ) -> impl Future<Output = Result<Info, PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.metadata(path, follow_link));
        let what = on("read the attributes of", path);
        async move {
            let m = run(call, what).await?;
            Ok(Info {
                kind: kind(m.file_type),
                size: i64::try_from(m.len).unwrap_or(i64::MAX),
                read_only: m.readonly,
                has_modified: m.modified.is_some(),
                modified: m.modified.unwrap_or(0),
                has_accessed: m.accessed.is_some(),
                accessed: m.accessed.unwrap_or(0),
                has_created: m.created.is_some(),
                created: m.created.unwrap_or(0),
            })
        }
    }

    /// Sets one time of `Path` (`Fs.set_times`): `Which` 0 the creation, 1 the last access, 2 the
    /// last write; `Nanos` since the UNIX epoch.
    #[pascal_function(cancel = "complete")]
    pub fn set_time(
        ctx: &mut CallCtx<'_>,
        path: &str,
        which: i32,
        nanos: i64,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        let mut times = FileTimes::default();
        match which {
            0 => times.created = Some(nanos),
            1 => times.accessed = Some(nanos),
            _ => times.modified = Some(nanos),
        }
        let call = ctx.host().fs().map(|fs| fs.set_times(path, times));
        run(call, on("set the time of", path))
    }

    /// Sets or clears the read-only attribute of `Path` (`Fs.set_readonly`).
    #[pascal_function(cancel = "complete")]
    pub fn set_read_only(
        ctx: &mut CallCtx<'_>,
        path: &str,
        read_only: bool,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.set_readonly(path, read_only));
        run(call, on("set the attributes of", path))
    }

    /// Copies the contents of the file `Source` to `Dest`, replacing it (`Fs.copy`).
    #[pascal_function(cancel = "complete")]
    pub fn copy_file(
        ctx: &mut CallCtx<'_>,
        source: &str,
        dest: &str,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.copy(source, dest));
        let what = between("copy file", source, dest);
        async move {
            run(call, what).await?;
            Ok(())
        }
    }

    /// Renames or moves a file or a directory (`Fs.rename`).
    #[pascal_function(cancel = "complete")]
    pub fn move_path(
        ctx: &mut CallCtx<'_>,
        source: &str,
        dest: &str,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.rename(source, dest));
        run(call, between("move", source, dest))
    }

    /// Removes a file or a symbolic link itself (`Fs.remove_file`).
    #[pascal_function(cancel = "complete")]
    pub fn erase_file(
        ctx: &mut CallCtx<'_>,
        path: &str,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.remove_file(path));
        run(call, on("delete file", path))
    }

    /// Removes an empty directory (`Fs.remove_dir`).
    #[pascal_function(cancel = "complete")]
    pub fn delete_dir(
        ctx: &mut CallCtx<'_>,
        path: &str,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.remove_dir(path));
        run(call, on("delete directory", path))
    }

    /// Creates a directory and its missing parents; an existing directory is not an error
    /// (`Fs.create_dir`).
    #[pascal_function(cancel = "complete")]
    pub fn make_dir(
        ctx: &mut CallCtx<'_>,
        path: &str,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        make_dir_with_parents(ctx, path, true)
    }

    pub(crate) fn make_dir_with_parents(
        ctx: &mut CallCtx<'_>,
        path: &str,
        recursive: bool,
    ) -> impl Future<Output = Result<(), PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.create_dir(path, recursive));
        run(call, on("create directory", path))
    }

    /// The directory for temporary files (`Os.temp_dir`); `''` without the OS information or
    /// where there is none (requirements §5.8).
    #[pascal_function]
    pub fn temp_dir(ctx: &mut CallCtx<'_>) -> Result<String, PasError> {
        match ctx.host().os.as_deref_mut() {
            Some(os) => os.temp_dir().map_err(|e| {
                PasError::host(&e, format!("cannot get the temporary directory: {}", e.fixed_text()))
            }),
            None => Ok(String::new()),
        }
    }

    /// The home directory of the user (`Os.home_dir`); `''` without the OS information or where
    /// there is none.
    #[pascal_function]
    pub fn home_dir(ctx: &mut CallCtx<'_>) -> Result<String, PasError> {
        match ctx.host().os.as_deref_mut() {
            Some(os) => os.home_dir().map_err(|e| {
                PasError::host(&e, format!("cannot get the home directory: {}", e.fixed_text()))
            }),
            None => Ok(String::new()),
        }
    }
}
