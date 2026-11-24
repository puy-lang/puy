//! The Rust parts of `System.Classes`: the unit `System.Classes.Native`, which the Pascal unit
//! uses in its `implementation` part (`docs/design/stdlib.md` 3.1, 3.4.1; specification:
//! `docs/spec/stdlib/System.Classes/TStream.md`).
//!
//! `TFileHandle` owns a file the host's file system opened (`Fs::open`); `TFileStream` holds one
//! and does the rest in Pascal. Each operation is one call of the handle, suspendable and run to
//! its end (`cancel = "complete"`; requirements §7.1). A failure of the host is an `EInOutError`
//! (or the `ENotSupportedException` of a missing file system) with its kind and code, whose
//! message names the operation and the file; the Pascal unit raises its stream exceptions from
//! it. Closing drops the handle, which closes the file.

#[puy_ext::pascal_module(unit = "System.Classes.Native")]
pub mod native {
    use std::future::Future;

    use puy_ext::prelude::*;
    use puy_ext::{File, HostError, OpenOptions, SeekFrom};

    /// The ways `OpenFile` opens a file.
    const OPEN_READ: i32 = 0;
    const OPEN_WRITE: i32 = 1;
    const OPEN_READ_WRITE: i32 = 2;
    const CREATE: i32 = 3;

    /// An open file of the host (or a closed one).
    #[pascal_class(name = "TFileHandle")]
    pub struct FileHandle {
        file: Option<Box<dyn File>>,
        path: String,
    }

    /// The exception of the host's failure `e` in `what` (`read`, `write`, …) of the file `path`.
    fn failed(e: &HostError, what: &str, path: &str) -> PasError {
        PasError::host(e, format!("cannot {what} file '{path}': {}", e.fixed_text()))
    }

    /// Opens `Path`: `Mode` 0 to read, 1 to write, 2 to read and write (the file must exist), 3
    /// to create it (or cut it to 0 bytes) to read and write.
    #[pascal_function(cancel = "complete")]
    pub fn open_file(
        ctx: &mut CallCtx<'_>,
        path: &str,
        mode: i32,
    ) -> impl Future<Output = Result<FileHandle, PasError>> + 'static {
        let (read, write) = match mode {
            OPEN_READ => (true, false),
            OPEN_WRITE => (false, true),
            OPEN_READ_WRITE | CREATE => (true, true),
            _ => (true, false),
        };
        let options = OpenOptions {
            read,
            write,
            create: mode == CREATE,
            truncate: mode == CREATE,
            ..OpenOptions::default()
        };
        let what = if mode == CREATE { "create" } else { "open" };
        let path = path.to_string();
        let call = ctx.host().fs().map(|fs| fs.open(&path, options));
        async move {
            let file = call?.await.map_err(|e| failed(&e, what, &path))?;
            Ok(FileHandle {
                file: Some(file),
                path,
            })
        }
    }

    #[pascal_methods]
    impl FileHandle {
        /// The open file, or the error of a closed one (the Pascal unit checks first).
        fn file(&mut self) -> Result<&mut Box<dyn File>, PasError> {
            self.file.as_mut().ok_or_else(|| {
                PasError::new("EInvalidOperation", format!("the file '{}' is closed", self.path))
            })
        }

        /// Up to `Count` bytes from the position (fewer at the end of the file; none past it).
        #[pascal_function(cancel = "complete")]
        pub fn read(
            &mut self,
            count: i32,
        ) -> impl Future<Output = Result<Vec<u8>, PasError>> + 'static {
            let path = self.path.clone();
            let call = self
                .file()
                .map(|f| f.read(usize::try_from(count).unwrap_or(0)));
            async move { call?.await.map_err(|e| failed(&e, "read", &path)) }
        }

        /// Writes the `Count` bytes of `Bytes` at `Offset` (the Pascal unit checked the range).
        #[pascal_function(cancel = "complete")]
        pub fn write(
            &mut self,
            bytes: &[u8],
            offset: i32,
            count: i32,
        ) -> impl Future<Output = Result<(), PasError>> + 'static {
            let path = self.path.clone();
            let part = usize::try_from(offset)
                .ok()
                .zip(usize::try_from(count).ok())
                .and_then(|(o, c)| bytes.get(o..o.checked_add(c)?));
            let call = match part {
                Some(part) => self.file().map(|f| f.write(part)),
                None => Err(PasError::new(
                    "ERangeError",
                    format!("bytes {offset}..+{count} out of {} bytes", bytes.len()),
                )),
            };
            async move { call?.await.map_err(|e| failed(&e, "write", &path)) }
        }

        /// Moves the position: `Origin` 0 from the start, 1 from the position, 2 from the end.
        /// Returns the new position. (The Pascal unit refuses a negative offset from the start.)
        #[pascal_function(cancel = "complete")]
        pub fn seek(
            &mut self,
            offset: i64,
            origin: i32,
        ) -> impl Future<Output = Result<i64, PasError>> + 'static {
            let path = self.path.clone();
            let pos = match origin {
                0 => SeekFrom::Start(offset.max(0) as u64),
                1 => SeekFrom::Current(offset),
                _ => SeekFrom::End(offset),
            };
            let call = self.file().map(|f| f.seek(pos));
            async move {
                let at = call?.await.map_err(|e| failed(&e, "seek in", &path))?;
                Ok(i64::try_from(at).unwrap_or(i64::MAX))
            }
        }

        /// The size of the file in bytes.
        #[pascal_function(cancel = "complete")]
        pub fn size(&mut self) -> impl Future<Output = Result<i64, PasError>> + 'static {
            let path = self.path.clone();
            let call = self.file().map(|f| f.metadata());
            async move {
                let m = call?.await.map_err(|e| failed(&e, "read the size of", &path))?;
                Ok(i64::try_from(m.len).unwrap_or(i64::MAX))
            }
        }

        /// Cuts or extends the file to `Size` bytes (extended with zeros).
        #[pascal_function(cancel = "complete")]
        pub fn set_size(&mut self, size: i64) -> impl Future<Output = Result<(), PasError>> + 'static {
            let path = self.path.clone();
            let call = self.file().map(|f| f.set_len(size.max(0) as u64));
            async move { call?.await.map_err(|e| failed(&e, "resize", &path)) }
        }

        /// Writes out what the host buffers.
        #[pascal_function(cancel = "complete")]
        pub fn flush(&mut self) -> impl Future<Output = Result<(), PasError>> + 'static {
            let path = self.path.clone();
            let call = self.file().map(|f| f.flush());
            async move { call?.await.map_err(|e| failed(&e, "write", &path)) }
        }

        /// Closes the file (dropping the handle); nothing if it is closed.
        pub fn close(&mut self) {
            self.file = None;
        }
    }
}
