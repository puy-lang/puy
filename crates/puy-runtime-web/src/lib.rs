//! Browser runtime (`wasm32-unknown-unknown` + `wasm-bindgen`; §14.1).
//!
//! For the Playground. Asynchronous host APIs are handled by suspending and resuming the stackless
//! VM.
//!
//! The `counting-allocator` feature supplies the global allocator for the standalone Wasm build.
//! Disable default features when linking this runtime into a Wasm application with its own
//! global allocator. The application can supply its own `HostSet::memory` in that case.

mod host;

pub use host::{BrowserOs, host_set};

#[cfg(all(
    target_arch = "wasm32",
    target_os = "unknown",
    feature = "counting-allocator"
))]
#[global_allocator]
static ALLOC: puy_host::alloc::CountingAlloc = puy_host::alloc::CountingAlloc;

/// The extensions of the runtime (`docs/design/extensions.md` 3.12): the standard library's, the
/// same as on the other targets.
pub fn extensions() -> puy_core::ext::ExtensionSet {
    puy_stdlib_ext::extensions()
}
