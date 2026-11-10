//! The Puy core: lexer, parser (lossless CST), compiler, bytecode, and stackless VM.
//!
//! Constraints (requirements §4.4):
//! - No dependency on I/O, threads, or `tokio`. All interaction with the outside goes through `puy-host`.
//! - Must build for `wasm32-wasip2` and `wasm32-unknown-unknown`.
//! - When adding a dependency crate, check its WASM support and binary size.
//!
//! The stages are described in `docs/design/compiler-pipeline.md` 2.1.

pub mod bytecode;
pub mod codegen;
pub mod diagnostics;
pub mod directives;
pub mod driver;
pub mod fix;
pub mod ir;
pub mod lexer;
pub mod numfmt;
pub mod parser;
pub mod sema;
pub mod source;
pub mod syntax;
pub mod vm;

/// Version of the implementation (`puy --version`, the `.puyc` header, `PuyVersionString`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `PuyVersion` (`major * 10000 + minor * 100 + patch`; §5.8).
pub fn version_number() -> i64 {
    let mut parts = VERSION.split('.').map(|p| p.parse::<i64>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    major * 10000 + minor * 100 + patch
}
