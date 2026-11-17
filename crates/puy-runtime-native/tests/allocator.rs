//! The global allocator wrapper (`docs/design/host-interface.md` 3.8): an allocation the system
//! refuses outside the mark of `puy_host::alloc` ends the process with F0003 and exit code 101;
//! one under the mark returns its failure to the caller. Each case runs in a child process (this
//! test executable run again with `PUY_ALLOC_CASE` set), since the wrapper ends the process.

#![cfg(not(target_os = "wasi"))]

use std::alloc::Layout;
use std::process::Command;

#[global_allocator]
static ALLOC: puy_runtime_native::CountingAlloc = puy_runtime_native::CountingAlloc;

/// More than any system gives (half the address space), so the refusal does not depend on the
/// memory of the machine.
const HUGE: usize = isize::MAX as usize / 2;

/// Runs `case` in a child process (the test named `test`), returning its exit code and stderr.
fn child(test: &str, case: &str) -> (Option<i32>, String) {
    let out = Command::new(std::env::current_exe().expect("test executable"))
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env("PUY_ALLOC_CASE", case)
        .output()
        .expect("spawn the test executable");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The case this process is asked to run, if it is a child.
fn case() -> Option<String> {
    std::env::var("PUY_ALLOC_CASE").ok()
}

/// Asks the allocator for `HUGE` bytes outside the mark (as a Rust collection would).
fn refuse() {
    // SAFETY: the layout has a non-zero size; the pointer is null (the wrapper ends the process
    // before returning it) and is never used
    let p = unsafe { std::alloc::alloc(Layout::from_size_align(HUGE, 1).unwrap()) };
    assert!(p.is_null());
}

// Before a program runs, a refused allocation inside Rust is F0003 `while compiling`, exit 101
// (not the abort of Rust's default handler).
#[test]
fn refused_while_compiling() {
    if case().is_some() {
        refuse();
        return;
    }
    let (code, stderr) = child("refused_while_compiling", "compiling");
    assert_eq!(code, Some(101), "{stderr}");
    assert!(
        stderr.ends_with("fatal[F0003]: out of memory while compiling\n"),
        "{stderr}"
    );
}

// While a program runs, F0003 gives the size of the refused allocation, here in the JSON form
// of `--format json`.
#[test]
fn refused_while_running_as_json() {
    if case().is_some() {
        puy_runtime_native::set_fatal_json(true);
        puy_runtime_native::set_running();
        refuse();
        return;
    }
    let (code, stderr) = child("refused_while_running_as_json", "running");
    assert_eq!(code, Some(101), "{stderr}");
    let line = format!(
        "{{\"schema\":\"puy.diagnostics/1\",\"diagnostics\":[{{\"code\":\"F0003\",\"severity\":\"fatal\",\"name\":null,\"message\":\"out of memory: allocation of {HUGE} bytes failed\",\"primary\":null,\"secondary\":[],\"notes\":[],\"fixes\":[],\"rendered\":\"fatal[F0003]: out of memory: allocation of {HUGE} bytes failed\"}}],\"summary\":{{\"errors\":1,\"warnings\":0,\"notes\":0}}}}\n"
    );
    assert!(stderr.ends_with(&line), "{stderr}");
}

// Under the mark the refusal comes back to the caller (the VM turns it into `EOutOfMemory`),
// and the process goes on.
#[test]
fn refused_under_the_mark_returns_the_failure() {
    let mut v: Vec<u8> = Vec::new();
    let r = puy_host::alloc::fallible(|| v.try_reserve_exact(HUGE));
    assert!(r.is_err());
    assert_eq!(v.capacity(), 0);
}
