//! The file system (`docs/design/stdlib.md` 3.4.1): `Fs`, the `File` handle it opens and the
//! `DirReader` it lists with.
//!
//! - Every operation touches the host, so every one is suspendable (`HostFuture`; requirements
//!   §7.1). None can be stopped half way, so their cancel mode is [`crate::CancelMode::Complete`].
//!   The M5a implementations do the work at once and return a completed `Future`.
//! - Paths are UTF-8 (`&str`): the VM turns a Pascal `string` into UTF-8 (a lone surrogate becomes
//!   U+FFFD). A name the OS returns that is not valid text (a listed name, a link target) has its
//!   invalid parts replaced with U+FFFD, as with environment variables; such a name cannot be
//!   opened again.
//! - Handles are trait objects owned by whoever opened them: dropping a `File` closes it. `Drop`
//!   only closes the OS handle; a caller that wants written data on disk calls `flush` / `sync`
//!   first.
//! - Times are nanoseconds since the UNIX epoch (as `Clock::wall_nanos`); a time the OS does not
//!   keep is `None`.

use crate::HostFuture;

/// How [`Fs::open`] opens a file. The fields have the meaning of `std::fs::OpenOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    /// Every write goes to the end (implies `write`).
    pub append: bool,
    /// Cuts an existing file to 0 bytes (needs `write`).
    pub truncate: bool,
    /// Creates the file if it does not exist (needs `write` or `append`).
    pub create: bool,
    /// Creates the file and fails with `AlreadyExists` if it exists (needs `write` or `append`).
    pub create_new: bool,
}

impl OpenOptions {
    /// Opens an existing file for reading.
    pub fn read() -> Self {
        OpenOptions {
            read: true,
            ..Self::default()
        }
    }

    /// Creates the file, or cuts an existing one to 0 bytes, for writing.
    pub fn create() -> Self {
        OpenOptions {
            write: true,
            create: true,
            truncate: true,
            ..Self::default()
        }
    }
}

/// Where [`File::seek`] counts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

/// What a path names. A symbolic link is `Symlink` only where the link itself is asked about
/// (`Fs::metadata` with `follow_links = false`, and the entries of a listing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    File,
    Dir,
    Symlink,
    /// A device, a pipe, a socket, …
    Other,
}

/// The attributes of a file or a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub file_type: FileType,
    /// Size in bytes.
    pub len: u64,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub created: Option<i64>,
    pub readonly: bool,
}

/// One entry of a directory listing (`.` and `..` are never listed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The name inside the directory (not the whole path).
    pub name: String,
    /// The type of the entry itself: a symbolic link is `Symlink`, not followed.
    pub file_type: FileType,
}

/// The times [`Fs::set_times`] sets; `None` leaves that time as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FileTimes {
    pub accessed: Option<i64>,
    pub modified: Option<i64>,
    /// Only some OSes (Windows) let the creation time be set; elsewhere `Some` fails with
    /// `Unsupported`.
    pub created: Option<i64>,
}

/// The file system.
pub trait Fs {
    /// Opens a file. Opening a directory fails with `IsDirectory` on every OS.
    fn open(&mut self, path: &str, options: OpenOptions) -> HostFuture<Box<dyn File>>;
    /// The attributes of `path`. With `follow_links`, a symbolic link is followed to its target.
    fn metadata(&mut self, path: &str, follow_links: bool) -> HostFuture<Metadata>;
    /// Starts listing a directory.
    fn read_dir(&mut self, path: &str) -> HostFuture<Box<dyn DirReader>>;
    /// Creates a directory; with `recursive`, its missing parents too, and an existing directory
    /// is not an error.
    fn create_dir(&mut self, path: &str, recursive: bool) -> HostFuture<()>;
    /// Removes an empty directory.
    fn remove_dir(&mut self, path: &str) -> HostFuture<()>;
    /// Removes a file (or a symbolic link itself).
    fn remove_file(&mut self, path: &str) -> HostFuture<()>;
    /// Renames or moves a file or a directory. An existing file at `to` is replaced where the OS
    /// does so.
    fn rename(&mut self, from: &str, to: &str) -> HostFuture<()>;
    /// Copies the contents of a file (replacing `to` if it exists) and returns the bytes copied.
    fn copy(&mut self, from: &str, to: &str) -> HostFuture<u64>;
    /// Sets the times of a file or a directory.
    fn set_times(&mut self, path: &str, times: FileTimes) -> HostFuture<()>;
    /// Sets or clears the read-only attribute (on Unix, clearing gives the owner the write
    /// permission back).
    fn set_readonly(&mut self, path: &str, readonly: bool) -> HostFuture<()>;
    /// The absolute path with every symbolic link, `.` and `..` resolved. The file must exist.
    fn canonicalize(&mut self, path: &str) -> HostFuture<String>;
    /// The target a symbolic link points to, as stored in the link.
    fn read_link(&mut self, path: &str) -> HostFuture<String>;
}

/// An open file (returned by [`Fs::open`]). Dropping it closes the OS handle.
pub trait File {
    /// Reads up to `max` bytes from the current position. Fewer come back only at the end of the
    /// file; none means the position is at the end.
    fn read(&mut self, max: usize) -> HostFuture<Vec<u8>>;
    /// Writes all of `bytes` at the current position (at the end when opened with `append`).
    fn write(&mut self, bytes: &[u8]) -> HostFuture<()>;
    /// Moves the position and returns it, counted from the start.
    fn seek(&mut self, pos: SeekFrom) -> HostFuture<u64>;
    /// Cuts or extends (with zeros) the file to `len` bytes. The position does not move.
    fn set_len(&mut self, len: u64) -> HostFuture<()>;
    /// Hands what the implementation buffers to the OS.
    fn flush(&mut self) -> HostFuture<()>;
    /// Waits until the contents and the attributes are on the storage.
    fn sync(&mut self) -> HostFuture<()>;
    /// The attributes of the open file.
    fn metadata(&mut self) -> HostFuture<Metadata>;
}

/// A directory being listed (returned by [`Fs::read_dir`]).
pub trait DirReader {
    /// The next entries, at most `max` of them (`max` is at least 1), so that a large directory
    /// is not read at once. An empty batch means the listing is over. The order is the OS's.
    fn next_batch(&mut self, max: usize) -> HostFuture<Vec<DirEntry>>;
}
