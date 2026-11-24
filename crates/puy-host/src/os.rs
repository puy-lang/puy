//! Information about the OS and the process (`docs/design/stdlib.md` 3.4.1, 3.10).
//!
//! Every method only reads a value of the process (or changes the working directory) and never
//! blocks, so all are synchronous. A value the target cannot give is not an error: it is the
//! empty string or the fixed value each method documents (requirements §5.8, Q62).

use crate::HostError;

/// The name and the version of the OS (`TOSVersion`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OsVersion {
    /// The kind of OS, in the spelling of Rust's `std::env::consts::OS` (`linux`, `macos`,
    /// `windows`, `wasi`, …), or `browser`.
    pub platform: String,
    /// The CPU architecture, in the spelling of `std::env::consts::ARCH` (`x86_64`, `aarch64`,
    /// `wasm32`, …).
    pub arch: String,
    /// A name for people (`Windows`, `macOS`, the `PRETTY_NAME` of a Linux distribution, …).
    pub name: String,
    /// The version (on Linux, of the kernel). 0 where it cannot be read.
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

/// The OS and the process.
pub trait Os {
    /// The process ID (0 where there is none, as on WASI and in the browser).
    fn process_id(&mut self) -> Result<u32, HostError>;
    /// The number of logical processors the process may use (at least 1).
    fn cpu_count(&mut self) -> Result<u32, HostError>;
    /// The directory for temporary files (`''` where there is none).
    fn temp_dir(&mut self) -> Result<String, HostError>;
    /// The home directory of the user (`''` where there is none).
    fn home_dir(&mut self) -> Result<String, HostError>;
    /// The working directory. It is one per process: a change is seen by every isolate.
    fn current_dir(&mut self) -> Result<String, HostError>;
    /// Changes the working directory.
    fn set_current_dir(&mut self, path: &str) -> Result<(), HostError>;
    /// The name and the version of the OS.
    fn os_version(&mut self) -> Result<OsVersion, HostError>;
}
