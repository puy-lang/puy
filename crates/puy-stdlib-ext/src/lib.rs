//! The Rust parts of the Puy standard library (`docs/design/stdlib.md` 3.1, 3.3): the units that
//! the Pascal units of `puy-stdlib` use for what is written in Rust, each an extension written
//! with `puy-ext` (requirements §4.6, §6).
//!
//! The units live one per file in `src/units/` (the Rust parts of `System.SysUtils` in
//! `system_sysutils.rs`); `build.rs` declares their modules and the table [`EXTENSIONS`]. Every
//! runtime registers all of them ([`extensions`]; `docs/design/extensions.md` 3.12), so that a
//! module runs on every target. Like `puy-core`, this crate does no I/O: what needs the OS goes
//! through the isolate's `HostSet`.

include!(concat!(env!("OUT_DIR"), "/units.rs"));

/// The standard library's extensions, for the compiler and the isolate alike.
pub fn extensions() -> puy_ext::ExtensionSet {
    let mut set = puy_ext::ExtensionSet::new();
    for ext in EXTENSIONS {
        set.add(ext);
    }
    set
}
