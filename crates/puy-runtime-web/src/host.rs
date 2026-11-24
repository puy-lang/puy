//! The `HostSet` of the browser (`docs/design/stdlib.md` 3.4.1).
//!
//! The browser has no file system (`Fs` is `None` as `Unsupported`; the Playground's in-memory
//! file system is M6). Its `Os` gives the fixed values of requirements §5.8 (Q62). The
//! capabilities that reach JavaScript (`Console`, `Clock`, `Random`, …) come with the Playground
//! and are `Unsupported` until then.

use puy_host::{AbsentSet, HostError, HostSet, Os, OsVersion};

/// The `HostSet` of the browser runtime.
pub fn host_set() -> HostSet {
    HostSet {
        os: Some(Box::new(BrowserOs)),
        #[cfg(all(
            target_arch = "wasm32",
            target_os = "unknown",
            feature = "counting-allocator"
        ))]
        memory: Some(Box::new(puy_host::alloc::ProcessMemory)),
        absent: AbsentSet::unsupported(),
        ..HostSet::empty()
    }
}

/// The OS information of the browser: no process, one processor, no home or temporary
/// directory, `/` as the working directory, which cannot be changed.
pub struct BrowserOs;

impl Os for BrowserOs {
    fn process_id(&mut self) -> Result<u32, HostError> {
        Ok(0)
    }

    fn cpu_count(&mut self) -> Result<u32, HostError> {
        Ok(1)
    }

    fn temp_dir(&mut self) -> Result<String, HostError> {
        Ok(String::new())
    }

    fn home_dir(&mut self) -> Result<String, HostError> {
        Ok(String::new())
    }

    fn current_dir(&mut self) -> Result<String, HostError> {
        Ok("/".to_string())
    }

    fn set_current_dir(&mut self, _path: &str) -> Result<(), HostError> {
        Err(HostError::unsupported(
            "changing the working directory is not available in the browser",
        ))
    }

    fn os_version(&mut self) -> Result<OsVersion, HostError> {
        Ok(OsVersion {
            platform: "browser".to_string(),
            arch: std::env::consts::ARCH.to_string(),
            name: "Browser".to_string(),
            major: 0,
            minor: 0,
            build: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use puy_host::{Absent, Capability, HostErrorKind};

    // The browser has no file system, as a capability the target lacks; its `Os` gives the fixed
    // values and refuses to change the working directory.
    #[test]
    fn browser_has_no_fs_and_a_fixed_os() {
        let mut host = host_set();
        assert_eq!(host.absent(Capability::Fs), Some(Absent::Unsupported));
        assert_eq!(
            host.fs().err().map(|e| e.kind),
            Some(HostErrorKind::Unsupported)
        );
        assert_eq!(host.absent(Capability::Os), None);
        let os = host.os().unwrap();
        assert_eq!(os.process_id().unwrap(), 0);
        assert_eq!(os.cpu_count().unwrap(), 1);
        assert_eq!(os.temp_dir().unwrap(), "");
        assert_eq!(os.home_dir().unwrap(), "");
        assert_eq!(os.current_dir().unwrap(), "/");
        assert_eq!(
            os.set_current_dir("/tmp").unwrap_err().kind,
            HostErrorKind::Unsupported
        );
        let v = os.os_version().unwrap();
        assert_eq!((v.platform.as_str(), v.major), ("browser", 0));
    }
}
