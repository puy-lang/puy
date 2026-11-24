//! The Rust parts of `System.SysUtils`: the unit `System.SysUtils.Native`, which the Pascal unit
//! uses in its `implementation` part (`docs/design/stdlib.md` 3.1; specification:
//! `docs/spec/stdlib/System.SysUtils/TEncoding.md`).
//!
//! - The cores of `TEncoding`'s conversions between `string` and `TBytes`: the strings are
//!   borrowed as the VM holds them (`PasStr`) and the bytes are given back as the `TBytes`
//!   block itself (`Vec<u8>`). The checks of the arguments and the API's shape are Pascal's; a
//!   range that does not fit the array is an error here too rather than a panic.
//! - The calls of the host for the files, the working directory and `TOSVersion`
//!   (`docs/spec/stdlib/System.SysUtils/Files.md`, `TOSVersion.md`): each is one call of the
//!   isolate's `HostSet` (`docs/design/stdlib.md` 3.1). A missing capability is the exception of
//!   its absence (`ENotSupportedException` with its `ErrorKind`); the other failures are given
//!   back as `False` to the Pascal functions, which return them as Delphi's do.

#[puy_ext::pascal_module(unit = "System.SysUtils.Native")]
pub mod native {
    use std::future::Future;

    use puy_ext::prelude::*;
    use puy_ext::{HostError, HostFuture};

    /// The work of a call of the file system: `Ok(false)` if the host failed, the exception of
    /// the file system's absence at once.
    async fn succeeds<T: 'static>(call: Result<HostFuture<T>, HostError>) -> Result<bool, PasError> {
        match call {
            Ok(f) => Ok(f.await.is_ok()),
            Err(e) => Err(PasError::from(e)),
        }
    }

    /// What `Path` names (`Fs.metadata`): 0 nothing (or a path the host cannot read), 1 a file,
    /// 2 a directory, 3 a symbolic link (only without `FollowLink`), 4 anything else.
    #[pascal_function(cancel = "complete")]
    pub fn path_kind(
        ctx: &mut CallCtx<'_>,
        path: &str,
        follow_link: bool,
    ) -> impl Future<Output = Result<i32, PasError>> + 'static {
        let call = ctx.host().fs().map(|fs| fs.metadata(path, follow_link));
        async move {
            let m = match call {
                Ok(f) => f.await,
                Err(e) => return Err(PasError::from(e)),
            };
            Ok(match m.map(|m| m.file_type) {
                Err(_) => 0,
                Ok(puy_ext::FileType::File) => 1,
                Ok(puy_ext::FileType::Dir) => 2,
                Ok(puy_ext::FileType::Symlink) => 3,
                Ok(puy_ext::FileType::Other) => 4,
            })
        }
    }

    /// Removes a file (`Fs.remove_file`).
    #[pascal_function(cancel = "complete")]
    pub fn remove_file(
        ctx: &mut CallCtx<'_>,
        path: &str,
    ) -> impl Future<Output = Result<bool, PasError>> + 'static {
        succeeds(ctx.host().fs().map(|fs| fs.remove_file(path)))
    }

    /// Renames a file or a directory (`Fs.rename`).
    #[pascal_function(cancel = "complete")]
    pub fn rename(
        ctx: &mut CallCtx<'_>,
        old_name: &str,
        new_name: &str,
    ) -> impl Future<Output = Result<bool, PasError>> + 'static {
        succeeds(ctx.host().fs().map(|fs| fs.rename(old_name, new_name)))
    }

    /// Creates a directory, with `Recursive` its missing parents too (`Fs.create_dir`).
    #[pascal_function(cancel = "complete")]
    pub fn create_dir(
        ctx: &mut CallCtx<'_>,
        path: &str,
        recursive: bool,
    ) -> impl Future<Output = Result<bool, PasError>> + 'static {
        succeeds(ctx.host().fs().map(|fs| fs.create_dir(path, recursive)))
    }

    /// Removes an empty directory (`Fs.remove_dir`).
    #[pascal_function(cancel = "complete")]
    pub fn remove_dir(
        ctx: &mut CallCtx<'_>,
        path: &str,
    ) -> impl Future<Output = Result<bool, PasError>> + 'static {
        succeeds(ctx.host().fs().map(|fs| fs.remove_dir(path)))
    }

    /// The working directory (`Os.current_dir`); `/` without the OS information (requirements
    /// §5.8). A failure of the host is an `EInOutError` with its kind.
    #[pascal_function]
    pub fn current_dir(ctx: &mut CallCtx<'_>) -> Result<String, PasError> {
        let Some(os) = ctx.host().os.as_deref_mut() else {
            return Ok("/".into());
        };
        os.current_dir().map_err(|e| {
            PasError::host(
                &e,
                format!("cannot get the working directory: {}", e.fixed_text()),
            )
        })
    }

    /// Changes the working directory (`Os.set_current_dir`).
    #[pascal_function]
    pub fn set_current_dir(ctx: &mut CallCtx<'_>, path: &str) -> Result<bool, PasError> {
        let os = ctx.host().os()?;
        Ok(os.set_current_dir(path).is_ok())
    }

    /// The OS as the host reports it (`Os.os_version`); empty names and version 0 without the
    /// OS information or when the host cannot tell (requirements §5.8).
    #[pascal_record(name = "TNativeOSVersion")]
    pub struct OsVersion {
        pub platform: String,
        pub arch: String,
        pub name: String,
        pub major: i64,
        pub minor: i64,
        pub build: i64,
    }

    #[pascal_function]
    pub fn os_version(ctx: &mut CallCtx<'_>) -> OsVersion {
        let v = ctx.host().os.as_deref_mut().map(|os| os.os_version());
        match v {
            Some(Ok(v)) => OsVersion {
                platform: v.platform,
                arch: v.arch,
                name: v.name,
                major: i64::from(v.major),
                minor: i64::from(v.minor),
                build: i64::from(v.build),
            },
            _ => OsVersion {
                platform: String::new(),
                arch: String::new(),
                name: String::new(),
                major: 0,
                minor: 0,
                build: 0,
            },
        }
    }

    /// The bytes `index..index + count` of `bytes`, or the `EEncodingError` of a range out of
    /// them (`docs/spec/stdlib/System.SysUtils/TEncoding.md` 3). The only check of the range:
    /// `TEncoding.GetString` passes its arguments through.
    fn range(bytes: &[u8], index: i32, count: i32) -> Result<&[u8], PasError> {
        usize::try_from(index)
            .ok()
            .zip(usize::try_from(count).ok())
            .and_then(|(i, c)| bytes.get(i..i.checked_add(c)?))
            .ok_or_else(|| {
                PasError::new(
                    "EEncodingError",
                    format!(
                        "byte index {index} and count {count} are out of the bounds of {} bytes",
                        bytes.len()
                    ),
                )
            })
    }

    /// `S` as UTF-8; a lone surrogate becomes U+FFFD (requirements §5.5).
    #[pascal_function]
    pub fn encode_utf8(s: PasStr<'_>) -> Vec<u8> {
        match s {
            PasStr::Ascii(a) => a.as_bytes().to_vec(),
            PasStr::Utf16(_) => s.to_string().into_bytes(),
        }
    }

    /// `S` as UTF-8; a lone surrogate is an `EEncodingError` (`TEncoding.UTF8Strict`).
    #[pascal_function]
    pub fn encode_utf8_strict(s: PasStr<'_>) -> Result<Vec<u8>, PasError> {
        let units = match s {
            PasStr::Ascii(a) => return Ok(a.as_bytes().to_vec()),
            PasStr::Utf16(u) => u,
        };
        let mut out = String::with_capacity(units.len());
        // The 1-based position of the next code unit, as Pascal counts a string's characters
        let mut pos = 1;
        for c in char::decode_utf16(units.iter().copied()) {
            match c {
                Ok(c) => {
                    out.push(c);
                    pos += c.len_utf16();
                }
                Err(e) => {
                    return Err(PasError::new(
                        "EEncodingError",
                        format!(
                            "the unpaired surrogate ${:04X} at position {pos} cannot be encoded \
                             as UTF-8",
                            e.unpaired_surrogate()
                        ),
                    ));
                }
            }
        }
        Ok(out.into_bytes())
    }

    /// The bytes `Index..Index + Count` of `B` read as UTF-8; an invalid sequence becomes U+FFFD
    /// (each maximal part of one, as the Unicode standard recommends).
    #[pascal_function]
    pub fn decode_utf8(b: &[u8], index: i32, count: i32) -> Result<PasString, PasError> {
        let bytes = range(b, index, count)?;
        Ok(PasString::from(String::from_utf8_lossy(bytes).into_owned()))
    }

    /// The bytes `Index..Index + Count` of `B` read as UTF-8; an invalid sequence is an
    /// `EEncodingError` (`TEncoding.UTF8Strict`).
    #[pascal_function]
    pub fn decode_utf8_strict(b: &[u8], index: i32, count: i32) -> Result<PasString, PasError> {
        let bytes = range(b, index, count)?;
        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(PasString::from(s)),
            Err(e) => Err(PasError::new(
                "EEncodingError",
                format!(
                    "invalid UTF-8 at byte {}",
                    i64::from(index) + e.valid_up_to() as i64
                ),
            )),
        }
    }

    /// `S` as UTF-16, little-endian or big-endian; a lone surrogate is written as it is.
    #[pascal_function]
    pub fn encode_utf16(s: PasStr<'_>, big_endian: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(s.len() * 2);
        for u in s.units() {
            let pair = if big_endian {
                u.to_be_bytes()
            } else {
                u.to_le_bytes()
            };
            out.extend_from_slice(&pair);
        }
        out
    }

    /// The bytes `Index..Index + Count` of `B` read as UTF-16; lone surrogates are kept, and an
    /// odd last byte becomes U+FFFD.
    #[pascal_function]
    pub fn decode_utf16(
        b: &[u8],
        index: i32,
        count: i32,
        big_endian: bool,
    ) -> Result<PasString, PasError> {
        let bytes = range(b, index, count)?;
        let (pairs, rest) = bytes.as_chunks::<2>();
        let mut units: Vec<u16> = pairs
            .iter()
            .map(|&p| {
                if big_endian {
                    u16::from_be_bytes(p)
                } else {
                    u16::from_le_bytes(p)
                }
            })
            .collect();
        if !rest.is_empty() {
            units.push(0xFFFD);
        }
        Ok(PasString::from_utf16(units))
    }

    /// `S` as ASCII: each character past U+007F (a surrogate pair is one character, a lone
    /// surrogate too) becomes `?`.
    #[pascal_function]
    pub fn encode_ascii(s: PasStr<'_>) -> Vec<u8> {
        match s {
            PasStr::Ascii(a) => a.as_bytes().to_vec(),
            PasStr::Utf16(_) => s
                .chars()
                .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
                .collect(),
        }
    }

    /// The bytes `Index..Index + Count` of `B` read as ASCII: a byte past $7F becomes `?`.
    #[pascal_function]
    pub fn decode_ascii(b: &[u8], index: i32, count: i32) -> Result<PasString, PasError> {
        let bytes = range(b, index, count)?;
        let s: String = bytes
            .iter()
            .map(|&b| if b.is_ascii() { b as char } else { '?' })
            .collect();
        Ok(PasString::from(s))
    }
}
