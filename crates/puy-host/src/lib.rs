//! Traits that form the boundary between the VM and the host environment (requirements §4.4;
//! design: `docs/design/host-interface.md`).
//!
//! - Both `puy-core` (no I/O) and `puy-runtime-*` (touches the OS) depend on this crate.
//! - `HostSet` bundles the per-capability traits (`Console`, `Env`, `Clock`, `Random`,
//!   `MemoryStats`) as `Option<Box<dyn Xxx>>`. `None` means "no permission / not available on
//!   this target".
//! - Host functions are split into synchronous ones (plain methods) and suspendable ones (returning
//!   `HostFuture`) (§4.3).
//! - `Send` is not required. An isolate runs on the thread that created it.
//!
//! This crate itself may depend on `std`, but does no I/O.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

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
/// - `read_line` reads stdin as UTF-8 (invalid sequences become U+FFFD) and returns the line with
///   the trailing newline (`LF` and a preceding `CR`) removed. An empty line is `Some("")`; end of
///   input is `None`. The last line is returned even without a newline.
pub trait Console {
    fn write(&mut self, stream: OutStream, bytes: &[u8]) -> HostFuture<()>;
    fn read_line(&mut self) -> HostFuture<Option<String>>;
}

/// Reading environment variables (synchronous).
///
/// Returns `Ok(None)` if unset. If the value is not a valid OS string, invalid parts become U+FFFD
/// as with `to_string_lossy`.
/// If the name is empty or contains `=` / `#0`, the VM does not call the host
/// (`docs/spec/bytecode.md` 2.6).
pub trait Env {
    fn get(&mut self, name: &str) -> Result<Option<String>, HostError>;
}

/// Clock (synchronous). In M1 it is used only inside the VM.
pub trait Clock {
    /// Wall clock. Nanoseconds since the UNIX epoch.
    fn wall_nanos(&mut self) -> Result<i64, HostError>;
    /// Monotonic clock. Nanoseconds since an arbitrary origin.
    fn monotonic_nanos(&mut self) -> Result<u64, HostError>;
}

/// Source of randomness (synchronous; §8.9). Failure is a `HostError`, which the VM treats as a
/// fatal error.
pub trait Random {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), HostError>;
}

/// Process-wide memory usage (§5.12). Implemented by the runtime's allocator wrapper.
/// Values that cannot be obtained are `None`.
pub trait MemoryStats {
    fn current(&self) -> Option<usize>;
    fn peak(&self) -> Option<usize>;
    fn limit(&self) -> Option<usize>;
}

/// Bundle of host capabilities (`docs/design/host-interface.md` 3.1). `None` means "no permission /
/// not available on this target".
#[derive(Default)]
pub struct HostSet {
    pub console: Option<Box<dyn Console>>,
    pub env: Option<Box<dyn Env>>,
    pub clock: Option<Box<dyn Clock>>,
    pub random: Option<Box<dyn Random>>,
    pub memory: Option<Box<dyn MemoryStats>>,
}

impl HostSet {
    /// A `HostSet` that can do nothing (the embedding default; §6.1).
    pub fn empty() -> Self {
        Self::default()
    }
}

impl fmt::Debug for HostSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostSet")
            .field("console", &self.console.is_some())
            .field("env", &self.env.is_some())
            .field("clock", &self.clock.is_some())
            .field("random", &self.random.is_some())
            .field("memory", &self.memory.is_some())
            .finish()
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

    #[test]
    fn io_error_mapping() {
        let e = HostError::from_io(&std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert_eq!(e.kind, HostErrorKind::PermissionDenied);
    }
}
