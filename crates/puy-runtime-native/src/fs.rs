//! The file system on `std::fs` (`docs/design/stdlib.md` 3.4.1). Shared by the native and the
//! WASI runtimes: on WASI only the preopened directories are reachable, and a path outside them
//! fails with `NotFound` or `PermissionDenied`.
//!
//! Every operation is done at once and returns a completed `Future` (M5a; M5c moves them to a
//! runtime thread).

use std::io::{Read, Seek, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use puy_host::{
    DirEntry, DirReader, File, FileTimes, FileType, Fs, HostError, HostErrorKind, HostFuture,
    Metadata, OpenOptions, SeekFrom, ready,
};

/// The file system of the OS.
pub struct StdFs;

fn io<T: 'static>(r: std::io::Result<T>) -> HostFuture<T> {
    ready(r.map_err(|e| HostError::from_io(&e)))
}

fn is_directory() -> HostError {
    HostError::new(HostErrorKind::IsDirectory, "is a directory")
}

fn path_text(p: std::path::PathBuf) -> String {
    p.to_string_lossy().into_owned()
}

/// Nanoseconds since the UNIX epoch (negative before it); `None` past the range of `i64`.
fn nanos(t: SystemTime) -> Option<i64> {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).ok(),
        Err(e) => i64::try_from(e.duration().as_nanos()).ok().map(|n| -n),
    }
}

/// The `SystemTime` of nanoseconds since the UNIX epoch; an error when the OS cannot represent it
/// (WASI has no time before the epoch), instead of the panic of `+` / `-`.
fn system_time(nanos: i64) -> Result<SystemTime, HostError> {
    let d = Duration::from_nanos(nanos.unsigned_abs());
    let t = if nanos >= 0 {
        UNIX_EPOCH.checked_add(d)
    } else {
        UNIX_EPOCH.checked_sub(d)
    };
    t.ok_or_else(|| HostError::unsupported("the time is out of the range of this system"))
}

fn file_type(t: std::fs::FileType) -> FileType {
    if t.is_symlink() {
        FileType::Symlink
    } else if t.is_dir() {
        FileType::Dir
    } else if t.is_file() {
        FileType::File
    } else {
        FileType::Other
    }
}

fn metadata(m: &std::fs::Metadata) -> Metadata {
    Metadata {
        file_type: file_type(m.file_type()),
        len: m.len(),
        modified: m.modified().ok().and_then(nanos),
        accessed: m.accessed().ok().and_then(nanos),
        created: m.created().ok().and_then(nanos),
        readonly: m.permissions().readonly(),
    }
}

fn open(path: &str, o: OpenOptions) -> Result<Box<dyn File>, HostError> {
    let opened = std::fs::OpenOptions::new()
        .read(o.read)
        .write(o.write)
        .append(o.append)
        .truncate(o.truncate)
        .create(o.create)
        .create_new(o.create_new)
        .open(path);
    let is_dir = || std::fs::metadata(path).is_ok_and(|m| m.is_dir());
    match opened {
        // Unix opens a directory for reading; the error comes only at the first read
        Ok(f) if f.metadata().is_ok_and(|m| m.is_dir()) => Err(is_directory()),
        Ok(f) => Ok(Box::new(StdFile(f))),
        // Windows refuses a directory with `PermissionDenied`, or with `NotFound` when the path
        // ends in a separator (`docs/design/host-interface.md` 3.3), so any failure on a
        // directory is `IsDirectory`
        Err(_) if is_dir() => Err(is_directory()),
        Err(e) => Err(HostError::from_io(&e)),
    }
}

/// Opens `path` (a file or a directory) to set its times. On Unix it is opened for reading, as
/// std has no stable way to set the times by path: a file the owner may not read cannot have its
/// times set, though the OS would allow it by path.
fn open_for_times(path: &str) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_WRITE_ATTRIBUTES only, which a read-only file allows too; backup semantics to
        // open a directory
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
    }
    #[cfg(not(windows))]
    {
        // Setting the times of an open file needs no write access on Unix
        std::fs::File::open(path)
    }
}

fn set_times(path: &str, t: FileTimes) -> Result<(), HostError> {
    let mut times = std::fs::FileTimes::new();
    if let Some(n) = t.accessed {
        times = times.set_accessed(system_time(n)?);
    }
    if let Some(n) = t.modified {
        times = times.set_modified(system_time(n)?);
    }
    if let Some(n) = t.created {
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTimesExt;
            times = times.set_created(system_time(n)?);
        }
        #[cfg(not(windows))]
        {
            let _ = n;
            return Err(HostError::unsupported(
                "setting the creation time is not supported on this system",
            ));
        }
    }
    // Opening a pipe would wait for its other end, and opening a device may act on it
    let meta = std::fs::metadata(path).map_err(|e| HostError::from_io(&e))?;
    if !meta.is_file() && !meta.is_dir() {
        return Err(HostError::unsupported(
            "setting the times is supported only for files and directories",
        ));
    }
    let f = open_for_times(path).map_err(|e| HostError::from_io(&e))?;
    f.set_times(times).map_err(|e| HostError::from_io(&e))
}

fn set_readonly(path: &str, readonly: bool) -> std::io::Result<()> {
    let mut perm = std::fs::metadata(path)?.permissions();
    #[cfg(unix)]
    {
        // `set_readonly(false)` would give everyone the write permission; give the owner's back
        use std::os::unix::fs::PermissionsExt;
        let mode = perm.mode();
        perm.set_mode(if readonly {
            mode & !0o222
        } else {
            mode | 0o200
        });
    }
    #[cfg(not(unix))]
    perm.set_readonly(readonly);
    std::fs::set_permissions(path, perm)
}

/// `canonicalize` gives a verbatim path on Windows (`\\?\C:\a`); hand back the ordinary form
/// (`C:\a`, `\\server\share\a`) that the rest of the program writes.
fn ordinary_path(p: String) -> String {
    if !cfg!(windows) {
        return p;
    }
    if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = p.strip_prefix(r"\\?\")
        && rest.as_bytes().get(1) == Some(&b':')
    {
        rest.to_string()
    } else {
        p
    }
}

impl Fs for StdFs {
    fn open(&mut self, path: &str, options: OpenOptions) -> HostFuture<Box<dyn File>> {
        ready(open(path, options))
    }

    fn metadata(&mut self, path: &str, follow_links: bool) -> HostFuture<Metadata> {
        let m = if follow_links {
            std::fs::metadata(path)
        } else {
            std::fs::symlink_metadata(path)
        };
        io(m.map(|m| metadata(&m)))
    }

    fn read_dir(&mut self, path: &str) -> HostFuture<Box<dyn DirReader>> {
        io(std::fs::read_dir(path).map(|r| Box::new(StdDirReader(r)) as Box<dyn DirReader>))
    }

    fn create_dir(&mut self, path: &str, recursive: bool) -> HostFuture<()> {
        io(if recursive {
            std::fs::create_dir_all(path)
        } else {
            std::fs::create_dir(path)
        })
    }

    fn remove_dir(&mut self, path: &str) -> HostFuture<()> {
        io(std::fs::remove_dir(path))
    }

    fn remove_file(&mut self, path: &str) -> HostFuture<()> {
        io(std::fs::remove_file(path))
    }

    fn rename(&mut self, from: &str, to: &str) -> HostFuture<()> {
        io(std::fs::rename(from, to))
    }

    fn copy(&mut self, from: &str, to: &str) -> HostFuture<u64> {
        io(std::fs::copy(from, to))
    }

    fn set_times(&mut self, path: &str, times: FileTimes) -> HostFuture<()> {
        ready(set_times(path, times))
    }

    fn set_readonly(&mut self, path: &str, readonly: bool) -> HostFuture<()> {
        io(set_readonly(path, readonly))
    }

    fn canonicalize(&mut self, path: &str) -> HostFuture<String> {
        io(std::fs::canonicalize(path).map(|p| ordinary_path(path_text(p))))
    }

    fn read_link(&mut self, path: &str) -> HostFuture<String> {
        io(std::fs::read_link(path).map(path_text))
    }
}

/// An open file of the OS.
struct StdFile(std::fs::File);

impl File for StdFile {
    fn read(&mut self, max: usize) -> HostFuture<Vec<u8>> {
        // The buffer grows with what is read, not with `max`; a refused allocation is an error
        // (`OutOfMemory`), not the end of the process (`docs/design/host-interface.md` 3.8)
        let mut buf = Vec::new();
        let r = puy_host::alloc::fallible(|| {
            (&mut self.0)
                .take(u64::try_from(max).unwrap_or(u64::MAX))
                .read_to_end(&mut buf)
        });
        io(r.map(|_| buf))
    }

    fn write(&mut self, bytes: &[u8]) -> HostFuture<()> {
        io(self.0.write_all(bytes))
    }

    fn seek(&mut self, pos: SeekFrom) -> HostFuture<u64> {
        io(self.0.seek(match pos {
            SeekFrom::Start(n) => std::io::SeekFrom::Start(n),
            SeekFrom::End(n) => std::io::SeekFrom::End(n),
            SeekFrom::Current(n) => std::io::SeekFrom::Current(n),
        }))
    }

    fn set_len(&mut self, len: u64) -> HostFuture<()> {
        io(self.0.set_len(len))
    }

    fn flush(&mut self) -> HostFuture<()> {
        io(self.0.flush())
    }

    fn sync(&mut self) -> HostFuture<()> {
        io(self.0.sync_all())
    }

    fn metadata(&mut self) -> HostFuture<Metadata> {
        io(self.0.metadata().map(|m| metadata(&m)))
    }
}

/// A directory of the OS being listed.
struct StdDirReader(std::fs::ReadDir);

impl DirReader for StdDirReader {
    fn next_batch(&mut self, max: usize) -> HostFuture<Vec<DirEntry>> {
        let mut out = Vec::new();
        while out.len() < max {
            let Some(e) = self.0.next() else { break };
            let entry = e.and_then(|e| {
                Ok(DirEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    file_type: file_type(e.file_type()?),
                })
            });
            match entry {
                Ok(entry) => out.push(entry),
                Err(e) => return io(Err(e)),
            }
        }
        ready(Ok(out))
    }
}
