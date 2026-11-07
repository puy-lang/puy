//! Native runtime. Implements the `puy-host` traits with OS facilities (requirements §4.4;
//! design: `docs/design/host-interface.md` 3.6, 3.7).
//!
//! For distribution, bytecode is appended to the end of a prebuilt runtime (`deno compile` style;
//! §4.5; M5).

use std::io::{BufRead, IsTerminal, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use puy_core::bytecode::Module;
use puy_core::source::{MAX_SOURCE_SIZE, SourceProvider, TOO_LARGE_MESSAGE};
use puy_core::vm::{ExitStatus, Isolate, IsolateConfig, RunOutcome, VmError};
use puy_host::{HostError, HostErrorKind, HostFuture, HostSet, OutStream, ready};

/// Standard I/O. Does not buffer (the VM owns the stdout buffer).
pub struct StdConsole;

impl puy_host::Console for StdConsole {
    fn write(&mut self, stream: OutStream, bytes: &[u8]) -> HostFuture<()> {
        let r = match stream {
            OutStream::Stdout => {
                let mut out = std::io::stdout().lock();
                out.write_all(bytes).and_then(|_| out.flush())
            }
            OutStream::Stderr => {
                let mut err = std::io::stderr().lock();
                err.write_all(bytes).and_then(|_| err.flush())
            }
        };
        ready(r.map_err(|e| HostError::from_io(&e)))
    }

    fn read_line(&mut self) -> HostFuture<Option<String>> {
        // Read one line as bytes; invalid UTF-8 sequences become U+FFFD
        // (`design/host-interface.md` 3.2, `spec/semantics.md` 6.2). On the Windows console std
        // converts from UTF-16, so a lone surrogate becomes an `InvalidData` error, which surfaces
        // as EInOutError as is
        let mut line = Vec::new();
        let r = std::io::stdin().lock().read_until(b'\n', &mut line);
        ready(match r {
            Ok(0) => Ok(None),
            Ok(_) => {
                if line.ends_with(b"\n") {
                    line.pop();
                    if line.ends_with(b"\r") {
                        line.pop();
                    }
                }
                Ok(Some(String::from_utf8_lossy(&line).into_owned()))
            }
            Err(e) => Err(HostError::from_io(&e)),
        })
    }
}

/// Environment variables.
pub struct StdEnv;

impl puy_host::Env for StdEnv {
    fn get(&mut self, name: &str) -> Result<Option<String>, HostError> {
        Ok(std::env::var_os(name).map(|v| v.to_string_lossy().into_owned()))
    }
}

/// Clock.
pub struct StdClock {
    start: std::time::Instant,
}

impl puy_host::Clock for StdClock {
    fn wall_nanos(&mut self) -> Result<i64, HostError> {
        let now = std::time::SystemTime::now();
        Ok(match now.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as i64,
            Err(e) => -(e.duration().as_nanos() as i64),
        })
    }

    fn monotonic_nanos(&mut self) -> Result<u64, HostError> {
        Ok(self.start.elapsed().as_nanos() as u64)
    }
}

/// Source of randomness (§8.9). On failure, returns a `HostError` instead of falling back to a
/// weaker substitute.
pub struct OsRandom;

impl puy_host::Random for OsRandom {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), HostError> {
        getrandom::fill(buf)
            .map_err(|e| HostError::new(HostErrorKind::Other, format!("random source failed: {e}")))
    }
}

/// Process-wide usage (counted by the allocator wrapper; §5.12).
pub struct ProcessMemory;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// Global allocator wrapper that counts usage. Use it as `#[global_allocator]` in the runtime
/// executable.
pub struct CountingAlloc;

// SAFETY: delegates to System; the accounting is just atomic add/subtract
unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let p = unsafe { std::alloc::System.alloc(layout) };
        if !p.is_null() {
            let now = ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

impl puy_host::MemoryStats for ProcessMemory {
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

/// The M1 `HostSet` (`Console`, `Env`, `Clock`, `Random`, `MemoryStats`).
pub fn host_set() -> HostSet {
    HostSet {
        console: Some(Box::new(StdConsole)),
        env: Some(Box::new(StdEnv)),
        clock: Some(Box::new(StdClock {
            start: std::time::Instant::now(),
        })),
        random: Some(Box::new(OsRandom)),
        memory: Some(Box::new(ProcessMemory)),
    }
}

/// Whether stdout is a terminal (`auto` for `stdout-buffering`).
pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

/// Whether stderr is a terminal (`--color auto`).
pub fn stderr_is_terminal() -> bool {
    std::io::stderr().is_terminal()
}

/// `SourceProvider` that reads from files. Directories are checked before opening and refused with
/// `IsDirectory`.
pub struct FileSourceProvider;

impl SourceProvider for FileSourceProvider {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, HostError> {
        read_source_file(path)
    }
}

/// Reads a source file (checks the size limit before reading).
pub fn read_source_file(path: &str) -> Result<Vec<u8>, HostError> {
    let meta = std::fs::metadata(path).map_err(|e| HostError::from_io(&e))?;
    if meta.is_dir() {
        return Err(HostError::new(HostErrorKind::IsDirectory, "is a directory"));
    }
    if meta.len() > MAX_SOURCE_SIZE {
        return Err(HostError::new(HostErrorKind::Other, TOO_LARGE_MESSAGE));
    }
    std::fs::read(path).map_err(|e| HostError::from_io(&e))
}

/// Reads stdin to the end.
pub fn read_stdin_all() -> Result<Vec<u8>, HostError> {
    let mut buf = Vec::new();
    std::io::stdin()
        .lock()
        .read_to_end(&mut buf)
        .map_err(|e| HostError::from_io(&e))?;
    if buf.len() as u64 > MAX_SOURCE_SIZE {
        return Err(HostError::new(HostErrorKind::Other, TOO_LARGE_MESSAGE));
    }
    Ok(buf)
}

/// Result of a run.
pub enum RunResult {
    Exit(ExitStatus),
    Fatal(VmError),
}

/// Runs a module to completion (the loop in `docs/design/host-interface.md` 3.6).
pub fn run_module(
    module: Arc<Module>,
    host: HostSet,
    config: IsolateConfig,
    args: Vec<String>,
) -> Result<RunResult, puy_core::vm::IsolateError> {
    let budget = config.budget_default;
    let mut iso = Isolate::new(module, host, config)?;
    iso.start_main(args)?;
    loop {
        match iso.run(budget) {
            RunOutcome::Finished(st) => return Ok(RunResult::Exit(st)),
            RunOutcome::BudgetExhausted => continue,
            RunOutcome::Waiting => continue, // does not happen in M1 (futures complete immediately)
            RunOutcome::Fatal(e) => return Ok(RunResult::Fatal(e)),
        }
    }
}
