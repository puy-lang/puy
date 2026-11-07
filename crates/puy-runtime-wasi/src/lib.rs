//! Runtime for WASI (`wasm32-wasip2`) (§14.2; `docs/design/host-interface.md` 3.6).
//!
//! The M1 WASI runtime is `puy.wasm`, i.e. the CLI crate (`puy`, in `crates/puy-cli`) built for
//! `wasm32-wasip2`. Its `HostSet` is the same five M1 capabilities as native, all usable from
//! the `wasm32-wasip2` `std` (`Console` is stdio, `Env` is `std::env`, `Clock` is
//! `wasi:clocks`, `Random` is `wasi:random` (`getrandom`), `MemoryStats` is the allocator
//! wrapper). So the implementation is shared with the native runtime, and this crate just
//! re-exports it. WASI-specific differences (how exit codes propagate, etc.) go in this crate.
//!
//! The `.wasm` produced by `puy build --target wasi` (M6) is a separate entry point with neither
//! the compiler nor the CLI (`wasm.md`).

pub use puy_runtime_native::*;
