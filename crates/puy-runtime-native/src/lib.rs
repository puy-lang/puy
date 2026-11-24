//! Native runtime. Implements the `puy-host` traits with OS facilities (requirements §4.4;
//! design: `docs/design/host-interface.md` 3.6, 3.7).
//!
//! For distribution, bytecode is appended to the end of a prebuilt runtime (`deno compile` style;
//! §4.5; M5).

use std::io::{IsTerminal, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use puy_core::bytecode::Module;
use puy_core::source::{MAX_SOURCE_SIZE, SourceProvider, TOO_LARGE_MESSAGE};
use puy_core::vm::{ExitStatus, Isolate, IsolateConfig, RunOutcome, VmError};
use puy_host::{AbsentSet, HostError, HostErrorKind, HostFuture, HostSet, OutStream, ready};

mod fs;
mod os;
mod tz;

pub use fs::StdFs;
pub use os::StdOs;
pub use puy_host::alloc::ProcessMemory;

/// Standard I/O. Does not buffer (the VM owns the stdout buffer).
#[derive(Default)]
pub struct StdConsole {
    /// What `read` reads into, kept between reads (a read gives a line at a terminal, far less
    /// than `max`; only the bytes read are copied out).
    buf: Vec<u8>,
}

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

    fn read(&mut self, max: usize) -> HostFuture<Vec<u8>> {
        // The bytes as they come (a terminal gives a line at a time); the VM cuts lines and
        // decodes UTF-8 (`design/host-interface.md` 3.2, `spec/semantics.md` 6.2). On the Windows
        // console std converts from UTF-16, so a lone surrogate becomes an `InvalidData` error,
        // which surfaces as EInOutError as is
        if self.buf.len() < max {
            self.buf.resize(max, 0);
        }
        let buf = &mut self.buf[..max];
        let r = loop {
            match std::io::stdin().lock().read(buf) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                r => break r,
            }
        };
        ready(match r {
            Ok(n) => Ok(buf[..n].to_vec()),
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

    fn vars(&mut self) -> Result<Vec<(String, String)>, HostError> {
        Ok(std::env::vars_os()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
            .collect())
    }

    fn ignores_case(&self) -> bool {
        cfg!(windows)
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

    fn local_offset(&mut self, utc_nanos: i64) -> Result<Option<i32>, HostError> {
        Ok(tz::local_offset(utc_nanos.div_euclid(1_000_000_000)))
    }

    // Blocks the thread of the isolate: one task per isolate until M5c, where a wait lets the
    // other tasks run (requirements §7.1)
    fn sleep(&mut self, nanos: u64) -> puy_host::HostFuture<()> {
        std::thread::sleep(std::time::Duration::from_nanos(nanos));
        puy_host::ready(Ok(()))
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

/// Global allocator wrapper that counts usage. Use it as `#[global_allocator]` in the runtime
/// executable.
pub struct CountingAlloc;

// SAFETY: delegates allocation and deallocation to the same shared counting allocator.
unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let p = unsafe { puy_host::alloc::CountingAlloc.alloc(layout) };
        if p.is_null() && !puy_host::alloc::in_fallible() {
            // Rust would abort on this refusal (SIGABRT, not 101), so the wrapper ends the
            // process itself (`docs/design/host-interface.md` 3.8). A refusal under the mark
            // returns null to the VM, which raises `EOutOfMemory`
            out_of_memory(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { puy_host::alloc::CountingAlloc.dealloc(ptr, layout) }
    }
}

/// Whether fatal diagnostics are written as JSON (`--format json`).
static FATAL_JSON: AtomicBool = AtomicBool::new(false);
/// Whether a program has started running (the F0003 message says which stage ran out).
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Sets how [`write_fatal`] writes (the CLI calls it before compiling).
pub fn set_fatal_json(json: bool) {
    FATAL_JSON.store(json, Ordering::Relaxed);
}

/// Marks that a program has started running ([`run_module`] calls it): from here on F0003 says
/// how large the refused allocation was instead of `while compiling`.
pub fn set_running() {
    RUNNING.store(true, Ordering::Relaxed);
}

/// Writes a fatal diagnostic without a source position (F0001 of the panic hook, F0003 of the
/// allocator wrapper; `docs/spec/diagnostics.md` 4.9) as one line in the text or JSON form,
/// without allocating.
pub fn write_fatal(w: &mut impl std::fmt::Write, code: &str, message: &str) -> std::fmt::Result {
    use puy_core::diagnostics::json::{write_json_escaped, write_json_string};
    if !FATAL_JSON.load(Ordering::Relaxed) {
        return writeln!(w, "fatal[{code}]: {message}");
    }
    write!(
        w,
        "{{\"schema\":\"puy.diagnostics/1\",\"diagnostics\":[{{\"code\":\"{code}\",\"severity\":\"fatal\",\"name\":null,\"message\":"
    )?;
    write_json_string(w, message)?;
    write!(
        w,
        ",\"primary\":null,\"secondary\":[],\"notes\":[],\"fixes\":[],\"rendered\":\"fatal[{code}]: "
    )?;
    write_json_escaped(w, message)?;
    w.write_str("\"}],\"summary\":{\"errors\":1,\"warnings\":0,\"notes\":0}}\n")
}

/// A fixed buffer on the stack (the allocator cannot allocate to report its own failure). Text
/// past its end is cut.
struct StackText {
    buf: [u8; 512],
    len: usize,
}

impl std::fmt::Write for StackText {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// F0003 for an allocation inside Rust that the system refused: written to standard error, then
/// exit code 101 like the panic hook (`docs/spec/diagnostics.md` 4.9).
fn out_of_memory(size: usize) -> ! {
    let mut text = StackText {
        buf: [0; 512],
        len: 0,
    };
    let mut message = StackText {
        buf: [0; 512],
        len: 0,
    };
    let _ = if RUNNING.load(Ordering::Relaxed) {
        std::fmt::Write::write_fmt(
            &mut message,
            format_args!("out of memory: allocation of {size} bytes failed"),
        )
    } else {
        std::fmt::Write::write_str(&mut message, "out of memory while compiling")
    };
    let message = std::str::from_utf8(&message.buf[..message.len]).unwrap_or("out of memory");
    let _ = write_fatal(&mut text, "F0003", message);
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(&text.buf[..text.len]);
    let _ = err.flush();
    std::process::exit(101)
}

/// The `HostSet` of the runtime: every capability (`Console`, `Env`, `Clock`, `Random`,
/// `MemoryStats`, `Fs`, `Os`).
pub fn host_set() -> HostSet {
    HostSet {
        console: Some(Box::new(StdConsole::default())),
        env: Some(Box::new(StdEnv)),
        clock: Some(Box::new(StdClock {
            start: std::time::Instant::now(),
        })),
        random: Some(Box::new(OsRandom)),
        memory: Some(Box::new(ProcessMemory)),
        fs: Some(Box::new(StdFs)),
        os: Some(Box::new(StdOs)),
        absent: AbsentSet::unsupported(),
        test: None,
    }
}

/// The extensions of the runtime (`docs/design/extensions.md` 3.12): the standard library's,
/// which every target registers alike, so that a module runs on all of them. The compiler (the
/// units it finds) and the isolate (the functions it binds) take the same set.
pub fn extensions() -> puy_core::ext::ExtensionSet {
    puy_stdlib_ext::extensions()
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

    fn list(&mut self, dir: &str) -> Result<Vec<String>, HostError> {
        fn walk(base: &std::path::Path, rel: &str, out: &mut Vec<String>) -> std::io::Result<()> {
            for e in std::fs::read_dir(base)? {
                let e = e?;
                let Some(name) = e.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                let path = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}/{name}")
                };
                if e.file_type()?.is_dir() {
                    walk(&e.path(), &path, out)?;
                } else {
                    out.push(path);
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        let dir = if dir.is_empty() { "." } else { dir };
        match walk(std::path::Path::new(dir), "", &mut out) {
            Ok(()) => Ok(out),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(HostError::from_io(&e)),
        }
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
    Exit {
        /// The first `Finished`: how the program ended (its uncaught exception is reported first).
        program: Box<ExitStatus>,
        /// The `Finished` after the finishing stage: the final exit code, leaks, and flush failure.
        last: Box<ExitStatus>,
    },
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
    set_running();
    let mut iso = Isolate::new(module, host, config)?;
    iso.start_main(args)?;
    let mut program = None;
    loop {
        match iso.run(budget) {
            RunOutcome::Finished(st) if program.is_none() => {
                program = Some(st);
                iso.finish();
            }
            RunOutcome::Finished(last) => {
                let program = program.expect("checked by the arm above");
                return Ok(RunResult::Exit {
                    program: Box::new(program),
                    last: Box::new(last),
                });
            }
            RunOutcome::BudgetExhausted => continue,
            // The native console's futures complete at once, so the VM does not wait; were one
            // pending, running again polls it again
            RunOutcome::Waiting => continue,
            RunOutcome::Fatal(e) => return Ok(RunResult::Fatal(e)),
        }
    }
}
