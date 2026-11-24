//! The offset of the local time from UTC (`Clock::local_offset`; requirements §8.8): asked of the
//! OS for each instant, so that daylight saving time and `TZ` are the OS's own. The WASI runtime
//! re-exports this crate's `StdClock`, so WASI comes here too; it has no time zone and answers
//! `None` (UTC).

/// The offset in seconds (east positive) of the local time at `utc_secs` seconds since the UNIX
/// epoch, or `None` when the OS cannot tell.
pub fn local_offset(utc_secs: i64) -> Option<i32> {
    imp::local_offset(utc_secs)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod imp {
    use std::ffi::{c_char, c_int, c_long};

    /// `struct tm` of glibc, musl and macOS: nine `int`s, then `tm_gmtoff` and `tm_zone`.
    #[repr(C)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
        tm_gmtoff: c_long,
        tm_zone: *const c_char,
    }

    unsafe extern "C" {
        /// Reads `TZ` (or the OS's setting when it is unset) again.
        fn tzset();
        /// The local time of `*time` (a `time_t`, which is a `long` on these targets) into
        /// `*result`; null when it cannot be represented.
        fn localtime_r(time: *const c_long, result: *mut Tm) -> *mut Tm;
    }

    pub fn local_offset(utc_secs: i64) -> Option<i32> {
        let time = c_long::try_from(utc_secs).ok()?;
        let mut tm = Tm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        };
        // SAFETY: `tzset` takes nothing; `localtime_r` reads `time` and writes only `tm`, both
        // valid for the call. `TZ` is read by the C library; nothing in the runtime changes the
        // environment while it runs
        let r = unsafe {
            tzset();
            localtime_r(&time, &mut tm)
        };
        if r.is_null() {
            return None;
        }
        i32::try_from(tm.tm_gmtoff).ok()
    }
}

#[cfg(windows)]
mod imp {
    /// `FILETIME`: 100 ns since 1601-01-01 UTC.
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    /// `SYSTEMTIME`.
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn FileTimeToSystemTime(file_time: *const FileTime, system_time: *mut SystemTime) -> i32;
        fn SystemTimeToFileTime(system_time: *const SystemTime, file_time: *mut FileTime) -> i32;
        /// The local time of a UTC time in the time zone `zone` (null: the current one, with the
        /// daylight saving rules of that year).
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const std::ffi::c_void,
            utc: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
    }

    /// Seconds from 1601-01-01 to 1970-01-01.
    const EPOCH_1601: i64 = 11_644_473_600;

    pub fn local_offset(utc_secs: i64) -> Option<i32> {
        let ticks = u64::try_from(utc_secs.checked_add(EPOCH_1601)?)
            .ok()?
            .checked_mul(10_000_000)?;
        let utc_file = FileTime {
            low: ticks as u32,
            high: (ticks >> 32) as u32,
        };
        let mut utc = SystemTime::default();
        let mut local = SystemTime::default();
        let mut local_file = FileTime::default();
        // SAFETY: each call reads and writes only the structures given, all valid for the call
        let ok = unsafe {
            FileTimeToSystemTime(&utc_file, &mut utc) != 0
                && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0
                && SystemTimeToFileTime(&local, &mut local_file) != 0
        };
        if !ok {
            return None;
        }
        let local_ticks = (u64::from(local_file.high) << 32) | u64::from(local_file.low);
        let diff = (local_ticks as i128 - ticks as i128) / 10_000_000;
        i32::try_from(diff).ok()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    /// Any other OS (WASI among them): no time zone, so UTC.
    pub fn local_offset(_utc_secs: i64) -> Option<i32> {
        None
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    // `TZ` decides the offset, with daylight saving time at the instant asked: a fixed zone
    // written in the POSIX form needs no time zone database on the machine.
    #[test]
    fn offset_follows_tz() {
        // SAFETY: the only test of this crate that touches the environment
        unsafe { std::env::set_var("TZ", "XST-9") };
        assert_eq!(local_offset(1_700_000_000), Some(9 * 3600));
        unsafe { std::env::set_var("TZ", "EST5EDT,M3.2.0,M11.1.0") };
        // 2026-01-15 (standard time) and 2026-07-15 (daylight saving time)
        assert_eq!(local_offset(1_768_478_400), Some(-5 * 3600));
        assert_eq!(local_offset(1_784_116_800), Some(-4 * 3600));
        unsafe { std::env::set_var("TZ", "UTC0") };
        assert_eq!(local_offset(0), Some(0));
    }
}
