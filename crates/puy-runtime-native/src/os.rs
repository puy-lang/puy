//! The OS and the process (`docs/design/stdlib.md` 3.4.1, 3.10). Shared by the native and the
//! WASI runtimes; WASI has no process ID, home or temporary directory, and gives the values of
//! requirements §5.8 (Q62) for them.

use puy_host::{HostError, Os, OsVersion};

/// The OS of the process.
pub struct StdOs;

fn io<T>(r: std::io::Result<T>) -> Result<T, HostError> {
    r.map_err(|e| HostError::from_io(&e))
}

fn path_text(p: std::path::PathBuf) -> String {
    p.to_string_lossy().into_owned()
}

impl Os for StdOs {
    fn process_id(&mut self) -> Result<u32, HostError> {
        Ok(if cfg!(target_os = "wasi") {
            0
        } else {
            std::process::id()
        })
    }

    fn cpu_count(&mut self) -> Result<u32, HostError> {
        // A target that cannot tell (WASI) counts as one
        Ok(std::thread::available_parallelism()
            .map_or(1, |n| u32::try_from(n.get()).unwrap_or(u32::MAX)))
    }

    fn temp_dir(&mut self) -> Result<String, HostError> {
        Ok(if cfg!(target_os = "wasi") {
            String::new()
        } else {
            path_text(std::env::temp_dir())
        })
    }

    fn home_dir(&mut self) -> Result<String, HostError> {
        Ok(if cfg!(target_os = "wasi") {
            String::new()
        } else {
            std::env::home_dir().map(path_text).unwrap_or_default()
        })
    }

    fn current_dir(&mut self) -> Result<String, HostError> {
        io(std::env::current_dir()).map(path_text)
    }

    fn set_current_dir(&mut self, path: &str) -> Result<(), HostError> {
        io(std::env::set_current_dir(path))
    }

    fn os_version(&mut self) -> Result<OsVersion, HostError> {
        let (name, [major, minor, build]) = version::read();
        Ok(OsVersion {
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            name,
            major,
            minor,
            build,
        })
    }
}

/// The first three numbers of a version text (`6.18.44-generic` → `[6, 18, 44]`); a missing or
/// unreadable part is 0.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn version_numbers(text: &str) -> [u32; 3] {
    let mut out = [0; 3];
    for (slot, part) in out.iter_mut().zip(text.trim().split('.')) {
        let digits: &str = &part[..part.bytes().take_while(u8::is_ascii_digit).count()];
        *slot = digits.parse().unwrap_or(0);
        if digits.len() != part.len() {
            break;
        }
    }
    out
}

#[cfg(target_os = "linux")]
mod version {
    /// The distribution's name (`PRETTY_NAME` of `os-release`) and the kernel's version.
    pub fn read() -> (String, [u32; 3]) {
        let name = ["/etc/os-release", "/usr/lib/os-release"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| pretty_name(&text))
            .unwrap_or_else(|| "Linux".to_string());
        let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        (name, super::version_numbers(&release))
    }

    /// `PRETTY_NAME=…` of an `os-release` file, without its quotes.
    pub fn pretty_name(text: &str) -> Option<String> {
        let value = text.lines().find_map(|l| l.strip_prefix("PRETTY_NAME="))?;
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        (!value.is_empty()).then(|| value.to_string())
    }
}

#[cfg(target_os = "macos")]
mod version {
    /// `ProductVersion` of the system's version file.
    pub fn read() -> (String, [u32; 3]) {
        let text = std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
            .unwrap_or_default();
        let version = plist_string(&text, "ProductVersion").unwrap_or_default();
        ("macOS".to_string(), super::version_numbers(version))
    }

    /// The `<string>` after `<key>{key}</key>` of a property list.
    fn plist_string<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        let rest = &text[text.find(&format!("<key>{key}</key>"))?..];
        let start = rest.find("<string>")? + "<string>".len();
        let end = rest[start..].find("</string>")?;
        Some(rest[start..start + end].trim())
    }
}

#[cfg(windows)]
mod version {
    /// `OSVERSIONINFOW`.
    #[repr(C)]
    struct OsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        csd_version: [u16; 128],
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        /// Unlike `GetVersionExW`, tells the real version whatever the manifest of the program
        /// says. Returns `STATUS_SUCCESS` (0).
        fn RtlGetVersion(info: *mut OsVersionInfo) -> i32;
    }

    pub fn read() -> (String, [u32; 3]) {
        let mut info = OsVersionInfo {
            size: std::mem::size_of::<OsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform_id: 0,
            csd_version: [0; 128],
        };
        // SAFETY: `info` is a valid `OSVERSIONINFOW` with its size set, which is all the call
        // writes to
        let status = unsafe { RtlGetVersion(&mut info) };
        let numbers = if status == 0 {
            [info.major, info.minor, info.build]
        } else {
            [0; 3]
        };
        ("Windows".to_string(), numbers)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod version {
    /// Any other OS (WASI among them): its name and no version.
    pub fn read() -> (String, [u32; 3]) {
        let name = match std::env::consts::OS {
            "wasi" => "WASI",
            other => other,
        };
        (name.to_string(), [0; 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The leading numbers of a version text are read up to the first part that is not all
    // digits; what is missing is 0.
    #[test]
    fn version_numbers_of_texts() {
        for (text, want) in [
            ("6.18.44-fc-v653\n", [6, 18, 44]),
            ("5.15.0-105-generic", [5, 15, 0]),
            ("14.5", [14, 5, 0]),
            ("10.0.22631", [10, 0, 22631]),
            ("6.1-rc2.7", [6, 1, 0]),
            ("", [0, 0, 0]),
            ("x.1", [0, 0, 0]),
        ] {
            assert_eq!(version_numbers(text), want, "{text:?}");
        }
    }

    // `PRETTY_NAME` is read with or without quotes, and a file without it has no name.
    #[cfg(target_os = "linux")]
    #[test]
    fn pretty_name_of_os_release() {
        let text = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\n";
        assert_eq!(
            version::pretty_name(text).as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
        assert_eq!(
            version::pretty_name("PRETTY_NAME=Alpine\n").as_deref(),
            Some("Alpine")
        );
        assert_eq!(
            version::pretty_name("PRETTY_NAME='Arch Linux'").as_deref(),
            Some("Arch Linux")
        );
        assert_eq!(version::pretty_name("PRETTY_NAME=\"\"\n"), None);
        assert_eq!(version::pretty_name("ID=debian\n"), None);
    }
}
