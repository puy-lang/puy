//! Traits that form the boundary between the VM and the host environment (requirements §4.4;
//! design: `docs/design/host-interface.md`).
//!
//! - Both `puy-core` (no I/O) and `puy-runtime-*` (touches the OS) depend on this crate.
//! - `HostSet` bundles the per-capability traits (`Console`, `Env`, `Clock`, `Random`,
//!   `MemoryStats`, `Fs`, `Os`) as `Option<Box<dyn Xxx>>`. `None` means "no permission / not
//!   available on this target"; which of the two, `HostSet::absent` records per capability.
//! - Host functions are split into synchronous ones (plain methods) and suspendable ones (returning
//!   `HostFuture`) (§4.3).
//! - `Send` is not required. An isolate runs on the thread that created it.
//!
//! This crate itself may depend on `std`, but does no I/O.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

mod fs;
mod os;

pub use fs::{DirEntry, DirReader, File, FileTimes, FileType, Fs, Metadata, OpenOptions, SeekFrom};
pub use os::{Os, OsVersion};

/// Kinds of host errors (`docs/design/host-interface.md` 3.3).
///
/// The mapping to Pascal exception classes lives in `puy-core` (`docs/spec/bytecode.md` chapter 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostErrorKind {
    /// A file or similar was not found.
    NotFound,
    /// OS permission denied.
    PermissionDenied,
    /// Tried to open a directory.
    IsDirectory,
    /// Already exists.
    AlreadyExists,
    /// Capability not available on this target.
    Unsupported,
    /// Refused by the embedder's permissions (§6.1).
    NoPermission,
    /// Interrupted.
    Interrupted,
    /// Anything else. `message` holds the OS text.
    Other,
}

/// A host error. It only carries OS information and knows nothing of Pascal exception classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub kind: HostErrorKind,
    /// OS error code (§8.3).
    pub os_code: Option<i32>,
    /// English message (§13.1).
    pub message: String,
}

impl HostError {
    pub fn new(kind: HostErrorKind, message: impl Into<String>) -> Self {
        HostError {
            kind,
            os_code: None,
            message: message.into(),
        }
    }

    pub fn with_os_code(mut self, code: Option<i32>) -> Self {
        self.os_code = code;
        self
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(HostErrorKind::Unsupported, message)
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(HostErrorKind::Other, message)
    }

    /// Builds from a `std::io::Error`. The kind is mapped from `ErrorKind`; the OS text goes into
    /// `message`.
    pub fn from_io(err: &std::io::Error) -> Self {
        use std::io::ErrorKind as K;
        let kind = match err.kind() {
            K::NotFound => HostErrorKind::NotFound,
            K::PermissionDenied => HostErrorKind::PermissionDenied,
            K::IsADirectory => HostErrorKind::IsDirectory,
            K::AlreadyExists => HostErrorKind::AlreadyExists,
            K::Unsupported => HostErrorKind::Unsupported,
            K::Interrupted => HostErrorKind::Interrupted,
            _ => HostErrorKind::Other,
        };
        HostError {
            kind,
            os_code: err.raw_os_error(),
            message: err.to_string(),
        }
    }

    /// Fixed English text (`docs/spec/diagnostics.md` 4.11). Only `Other` returns the OS text as
    /// is.
    pub fn fixed_text(&self) -> &str {
        match self.kind {
            HostErrorKind::NotFound => "file not found",
            HostErrorKind::PermissionDenied => "permission denied",
            HostErrorKind::IsDirectory => "is a directory",
            HostErrorKind::Interrupted => "interrupted",
            HostErrorKind::AlreadyExists => "already exists",
            HostErrorKind::Unsupported => "not supported",
            HostErrorKind::NoPermission => "permission not granted",
            HostErrorKind::Other => &self.message,
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HostError {}

/// The `Future` returned by suspendable host functions (`docs/design/host-interface.md` 3.2).
///
/// Requires `'static` (it is stored in a task). Does not require `Send`.
pub type HostFuture<T> = Pin<Box<dyn Future<Output = Result<T, HostError>> + 'static>>;

/// Turns an already-completed result into a `HostFuture` (all M1 implementations complete
/// immediately).
pub fn ready<T: 'static>(result: Result<T, HostError>) -> HostFuture<T> {
    Box::pin(Ready(Some(result)))
}

struct Ready<T>(Option<Result<T, HostError>>);

impl<T> Future for Ready<T> {
    type Output = Result<T, HostError>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(
            self.get_mut()
                .0
                .take()
                .expect("HostFuture polled after completion"),
        )
    }
}

impl<T> Unpin for Ready<T> {}

/// What a deadline or a cancellation does to a suspendable call that has not completed
/// (`docs/design/extensions.md` 3.7; requirements §7.1). Recorded with each suspendable function;
/// the scheduler reads it once deadlines and cancellation exist (M5c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CancelMode {
    /// The `Future` is dropped and the exception raised at once (sockets, pipes, timers).
    #[default]
    Drop,
    /// The exception is raised at once; the `Future` runs to completion behind and its result is
    /// dropped (operations without side effects, such as name resolution).
    Detach,
    /// The exception is raised once the `Future` completes (regular files), unless the scope
    /// allows abandoning blocking operations.
    Complete,
}

/// Standard output or standard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutStream {
    Stdout,
    Stderr,
}

/// Standard I/O (suspendable).
///
/// - `write` does not buffer; it writes out the given bytes immediately (the VM owns the stdout
///   buffer; `docs/spec/semantics.md` 6.1). `bytes` has already been converted to UTF-8 by the VM.
/// - `read` reads up to `max` bytes of stdin as they come (fewer than `max` is fine, such as one
///   line typed at a terminal) and returns them as is; an empty result is the end of input. The
///   VM owns the input buffer: it cuts lines and decodes UTF-8 (`docs/spec/semantics.md` 6.2,
///   `docs/design/stdlib.md` 3.7). `max` is at least 1.
pub trait Console {
    fn write(&mut self, stream: OutStream, bytes: &[u8]) -> HostFuture<()>;
    fn read(&mut self, max: usize) -> HostFuture<Vec<u8>>;
}

/// Reading environment variables (synchronous). Writing is not the host's: an isolate keeps its
/// own table of changes and leaves the process's variables alone (`docs/design/stdlib.md` 3.4.3).
///
/// If a name or a value is not a valid OS string, invalid parts become U+FFFD as with
/// `to_string_lossy`.
pub trait Env {
    /// The value of `name`; `Ok(None)` if unset. If the name is empty or contains `=` / `#0`, the
    /// VM does not call the host (`docs/spec/bytecode.md` 2.6).
    fn get(&mut self, name: &str) -> Result<Option<String>, HostError>;
    /// Every variable as `(name, value)`, in the OS's order.
    fn vars(&mut self) -> Result<Vec<(String, String)>, HostError>;
    /// Whether names that differ only in case name the same variable (Windows). The isolate's
    /// own changes follow the same rule.
    fn ignores_case(&self) -> bool {
        false
    }
}

/// Clock (synchronous; requirements §8.8).
pub trait Clock {
    /// Wall clock. Nanoseconds since the UNIX epoch.
    fn wall_nanos(&mut self) -> Result<i64, HostError>;
    /// Monotonic clock. Nanoseconds since an arbitrary origin.
    fn monotonic_nanos(&mut self) -> Result<u64, HostError>;
    /// The offset of the local time from UTC, in seconds (east positive), at the instant
    /// `utc_nanos` (nanoseconds since the UNIX epoch): asked for each instant, since daylight
    /// saving time changes it. The current time zone is `TZ` if it is set, else the OS's
    /// setting. `None` when the target cannot tell (WASI, an embedding that gives no time zone):
    /// the local time is then UTC (`docs/spec/stdlib/System.SysUtils/DateTime.md`).
    fn local_offset(&mut self, utc_nanos: i64) -> Result<Option<i32>, HostError> {
        let _ = utc_nanos;
        Ok(None)
    }
    /// Waits `nanos` nanoseconds of the monotonic clock (`Sleep`; requirements §7.1, §8.8). A
    /// clock that cannot wait (the default) answers `Unsupported`. The isolate's virtual clock
    /// does not call this (`docs/spec/stdlib/Puy.Testing/README.md` chapter 6).
    fn sleep(&mut self, nanos: u64) -> HostFuture<()> {
        let _ = nanos;
        ready(Err(HostError::unsupported(
            "sleeping is not available on this target",
        )))
    }
}

/// Source of randomness (synchronous; §8.9). Failure is a `HostError`, which the VM treats as a
/// fatal error.
pub trait Random {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), HostError>;

    /// The hash seed of an isolate made with `HashSeed::FromHost` (`docs/spec/equality.md` 7.1).
    /// The seed is one per process: by default, 8 bytes taken with `fill` the first time any
    /// `Random` is asked, and the same value every time after, so every isolate of the process
    /// hashes alike. A failure is returned and leaves the seed to be taken by the next call.
    fn hash_seed(&mut self) -> Result<u64, HostError> {
        static SEED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        if let Some(s) = SEED.get() {
            return Ok(*s);
        }
        let mut buf = [0; 8];
        self.fill(&mut buf)?;
        // Another thread may have set it meanwhile: its value wins, so all agree
        Ok(*SEED.get_or_init(|| u64::from_le_bytes(buf)))
    }
}

/// Process-wide memory usage (§5.12). Implemented by the runtime's allocator wrapper.
/// Values that cannot be obtained are `None`.
pub trait MemoryStats {
    fn current(&self) -> Option<usize>;
    fn peak(&self) -> Option<usize>;
    fn limit(&self) -> Option<usize>;
    /// Bytes kept below the process limit for runtime allocations. `None` means unspecified.
    fn reserve(&self) -> Option<usize> {
        None
    }
}

/// The mark of an allocation whose failure the caller handles (`docs/design/host-interface.md`
/// 3.8). The VM makes its fallible allocations (`try_reserve` of the buffers of Pascal values,
/// which fail with `EOutOfMemory`) inside [`fallible`]; a runtime's global allocator wrapper
/// reads [`in_fallible`] when the system allocator refuses, and treats a refusal outside the mark
/// as the fatal failure of an allocation inside Rust (F0003). An embedder's allocator may ignore
/// the mark.
pub mod alloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
    static PEAK: AtomicUsize = AtomicUsize::new(0);

    /// Shared allocator accounting for the runtimes. An executable installs this as its global
    /// allocator or delegates to it from a wrapper that handles allocation failures.
    pub struct CountingAlloc;

    // SAFETY: delegates to System; the accounting only records successful allocations.
    unsafe impl GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { System.alloc(layout) };
            if !ptr.is_null() {
                let now = ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
                PEAK.fetch_max(now, Ordering::Relaxed);
            }
            ptr
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    /// Process-wide usage of an executable using [`CountingAlloc`] as its global allocator.
    pub struct ProcessMemory;

    impl super::MemoryStats for ProcessMemory {
        fn current(&self) -> Option<usize> {
            Some(ALLOCATED.load(Ordering::Relaxed))
        }

        fn peak(&self) -> Option<usize> {
            Some(PEAK.load(Ordering::Relaxed))
        }

        fn limit(&self) -> Option<usize> {
            None
        }
    }

    thread_local! {
        // A const initializer with no destructor: reading it from inside the global allocator
        // allocates nothing
        static FALLIBLE: Cell<bool> = const { Cell::new(false) };
    }

    /// Runs `f`, whose allocations may fail without ending the process, under the mark.
    pub fn fallible<R>(f: impl FnOnce() -> R) -> R {
        let outer = FALLIBLE.with(|m| m.replace(true));
        let r = f();
        FALLIBLE.with(|m| m.set(outer));
        r
    }

    /// Whether the current thread is inside [`fallible`].
    pub fn in_fallible() -> bool {
        FALLIBLE.with(Cell::get)
    }
}

/// Why a capability of a `HostSet` is `None` (`docs/design/stdlib.md` 3.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Absent {
    /// The target does not have it (the file system in the browser).
    Unsupported,
    /// The embedder did not grant it (§6.1). The default: a capability nobody passed is one not
    /// granted.
    #[default]
    NoPermission,
}

impl Absent {
    /// The error an operation of the missing capability fails with.
    pub fn error(self, what: &str) -> HostError {
        match self {
            Absent::Unsupported => {
                HostError::unsupported(format!("{what} is not available on this target"))
            }
            Absent::NoPermission => HostError::new(
                HostErrorKind::NoPermission,
                format!("{what} is not permitted by the host"),
            ),
        }
    }
}

/// A capability of a `HostSet` (one per field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Console,
    Env,
    Clock,
    Random,
    Memory,
    Fs,
    Os,
}

impl Capability {
    /// Every capability, in the order of the fields of `HostSet`.
    pub const ALL: [Capability; 7] = [
        Capability::Console,
        Capability::Env,
        Capability::Clock,
        Capability::Random,
        Capability::Memory,
        Capability::Fs,
        Capability::Os,
    ];

    /// The name of the field (`fs`), also used in error messages.
    pub fn name(self) -> &'static str {
        match self {
            Capability::Console => "console",
            Capability::Env => "env",
            Capability::Clock => "clock",
            Capability::Random => "random",
            Capability::Memory => "memory",
            Capability::Fs => "fs",
            Capability::Os => "os",
        }
    }
}

/// Why each capability would be missing (read only for those that are `None`). The runtime that
/// builds a `HostSet` marks what its target lacks as `Unsupported`; the rest stay `NoPermission`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AbsentSet {
    pub console: Absent,
    pub env: Absent,
    pub clock: Absent,
    pub random: Absent,
    pub memory: Absent,
    pub fs: Absent,
    pub os: Absent,
}

impl AbsentSet {
    /// Every capability `Unsupported` (a runtime then lists what it has).
    pub fn unsupported() -> Self {
        let u = Absent::Unsupported;
        AbsentSet {
            console: u,
            env: u,
            clock: u,
            random: u,
            memory: u,
            fs: u,
            os: u,
        }
    }

    pub fn get(&self, cap: Capability) -> Absent {
        match cap {
            Capability::Console => self.console,
            Capability::Env => self.env,
            Capability::Clock => self.clock,
            Capability::Random => self.random,
            Capability::Memory => self.memory,
            Capability::Fs => self.fs,
            Capability::Os => self.os,
        }
    }
}

/// Bundle of host capabilities (`docs/design/host-interface.md` 3.1). `None` means "no permission /
/// not available on this target"; `absent` says which.
#[derive(Default)]
pub struct HostSet {
    pub console: Option<Box<dyn Console>>,
    pub env: Option<Box<dyn Env>>,
    pub clock: Option<Box<dyn Clock>>,
    pub random: Option<Box<dyn Random>>,
    pub memory: Option<Box<dyn MemoryStats>>,
    pub fs: Option<Box<dyn Fs>>,
    pub os: Option<Box<dyn Os>>,
    pub absent: AbsentSet,
    /// The test runner's side of the isolate of a test (`puy test`); `None` elsewhere. Not a
    /// capability the embedder grants: what `Puy.Testing` asks the runner.
    pub test: Option<Box<dyn TestRunner>>,
}

/// What `puy test` does for the isolate of a test (`docs/spec/test.md` 12): the snapshots of
/// `Puy.Testing.AssertSnapshot`, whose files only the runner reads and writes.
pub trait TestRunner {
    /// Compares `actual` with the stored snapshot of the running test named by `name` (`None`:
    /// the next of the unnamed ones), made by the call at `file`:`line`. `Ok(None)` when they are
    /// equal or the snapshot was written (`--update`), `Ok(Some(message))` when the test fails
    /// with `message` (a different or missing snapshot), `Err` when its file cannot be read or
    /// written.
    fn snapshot(
        &mut self,
        name: Option<&str>,
        actual: &str,
        file: &str,
        line: u32,
    ) -> Result<Option<String>, HostError>;
}

impl HostSet {
    /// A `HostSet` that can do nothing (the embedding default; §6.1): every capability is missing
    /// as not granted.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether `cap` is there.
    pub fn has(&self, cap: Capability) -> bool {
        match cap {
            Capability::Console => self.console.is_some(),
            Capability::Env => self.env.is_some(),
            Capability::Clock => self.clock.is_some(),
            Capability::Random => self.random.is_some(),
            Capability::Memory => self.memory.is_some(),
            Capability::Fs => self.fs.is_some(),
            Capability::Os => self.os.is_some(),
        }
    }

    /// Why `cap` is missing; `None` when it is there.
    pub fn absent(&self, cap: Capability) -> Option<Absent> {
        (!self.has(cap)).then(|| self.absent.get(cap))
    }

    /// The file system, or the error of its absence.
    pub fn fs(&mut self) -> Result<&mut dyn Fs, HostError> {
        let absent = self.absent.fs;
        match self.fs.as_deref_mut() {
            Some(fs) => Ok(fs),
            None => Err(absent.error("the file system")),
        }
    }

    /// The OS information, or the error of its absence.
    pub fn os(&mut self) -> Result<&mut dyn Os, HostError> {
        let absent = self.absent.os;
        match self.os.as_deref_mut() {
            Some(os) => Ok(os),
            None => Err(absent.error("the OS information")),
        }
    }
}

impl fmt::Debug for HostSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("HostSet");
        for cap in Capability::ALL {
            match self.absent(cap) {
                None => d.field(cap.name(), &"present"),
                Some(a) => d.field(cap.name(), &a),
            };
        }
        if self.test.is_some() {
            d.field("test", &"present");
        }
        d.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{RawWaker, RawWakerVTable, Waker};

    fn noop_waker() -> Waker {
        fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        fn noop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    /// A source that gives different bytes on every call.
    struct Counter(u8);

    impl Random for Counter {
        fn fill(&mut self, buf: &mut [u8]) -> Result<(), HostError> {
            self.0 = self.0.wrapping_add(1);
            buf.fill(self.0);
            Ok(())
        }
    }

    // The default `hash_seed` takes the seed once per process (`docs/spec/equality.md` 7.1): asked
    // again, or through another source whose bytes differ, it gives the same value.
    #[test]
    fn default_hash_seed_is_one_per_process() {
        let mut a = Counter(0);
        let mut b = Counter(100);
        let first = a.hash_seed().unwrap();
        assert_eq!(a.hash_seed().unwrap(), first);
        assert_eq!(b.hash_seed().unwrap(), first);
    }

    #[test]
    fn ready_future_completes_immediately() {
        let mut fut = ready(Ok::<i32, HostError>(42));
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(Ok(42)));
    }

    #[test]
    fn fixed_text_by_kind() {
        let e = HostError::new(HostErrorKind::NotFound, "No such file (os error 2)");
        assert_eq!(e.fixed_text(), "file not found");
        let e = HostError::other("something odd");
        assert_eq!(e.fixed_text(), "something odd");
    }

    // The mark is set only inside `fallible`, nests, and is restored afterwards.
    #[test]
    fn fallible_mark_nests() {
        assert!(!alloc::in_fallible());
        let inner = alloc::fallible(|| {
            let nested = alloc::fallible(alloc::in_fallible);
            (nested, alloc::in_fallible())
        });
        assert_eq!(inner, (true, true));
        assert!(!alloc::in_fallible());
    }

    // A `None` capability fails with the kind its `absent` says; `empty()` marks every one as
    // not granted.
    #[test]
    fn absent_capability_errors_by_reason() {
        let mut host = HostSet::empty();
        for cap in Capability::ALL {
            assert_eq!(host.absent(cap), Some(Absent::NoPermission), "{cap:?}");
        }
        assert_eq!(
            host.fs().err().map(|e| e.kind),
            Some(HostErrorKind::NoPermission)
        );
        host.absent = AbsentSet::unsupported();
        assert_eq!(
            host.fs().err().map(|e| e.kind),
            Some(HostErrorKind::Unsupported)
        );
        assert_eq!(
            host.os().err().map(|e| e.kind),
            Some(HostErrorKind::Unsupported)
        );
        assert_eq!(
            format!("{host:?}"),
            "HostSet { console: Unsupported, env: Unsupported, clock: Unsupported, \
             random: Unsupported, memory: Unsupported, fs: Unsupported, os: Unsupported }"
        );
    }

    // A capability that is there has no `absent`, whatever the mark says.
    #[test]
    fn present_capability_has_no_absent() {
        let host = HostSet {
            random: Some(Box::new(Counter(0))),
            absent: AbsentSet::unsupported(),
            ..HostSet::empty()
        };
        assert_eq!(host.absent(Capability::Random), None);
        assert_eq!(host.absent(Capability::Clock), Some(Absent::Unsupported));
        assert!(format!("{host:?}").contains("random: \"present\""));
    }

    #[test]
    fn io_error_mapping() {
        let e = HostError::from_io(&std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert_eq!(e.kind, HostErrorKind::PermissionDenied);
    }
}
