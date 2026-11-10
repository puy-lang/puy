//! Source files and locations (`docs/spec/lexical.md` chapters 1–2, `docs/design/compiler-pipeline.md` 3.7).
//!
//! A location is held as a byte offset within the file (`TextSize`); line and column are looked up from the line table when displayed.
//! Lines are 1-based. Columns are 1-based and count Unicode scalar values (a leading BOM is not counted).

pub use rowan::{TextRange, TextSize};

/// Number of a file in the compilation unit. 0 is the file passed to `puy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);

/// A range within a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub range: TextRange,
}

impl Span {
    pub fn new(file: FileId, range: TextRange) -> Self {
        Span { file, range }
    }
}

/// Line and column (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub column: u32,
}

/// Upper limit on source size (`docs/spec/lexical.md` chapter 1).
pub const MAX_SOURCE_SIZE: u64 = u32::MAX as u64 - 1;

/// One source file. Kept as raw bytes (to handle files with invalid UTF-8 too).
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Path shown in diagnostics (the string passed to `puy`, as is).
    pub path: String,
    pub bytes: Vec<u8>,
    /// Byte offset of the start of each line. The first is 0.
    line_starts: Vec<u32>,
    has_bom: bool,
}

impl SourceFile {
    pub fn new(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        let line_starts = compute_line_starts(&bytes);
        let has_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
        SourceFile {
            path: path.into(),
            bytes,
            line_starts,
            has_bom,
        }
    }

    pub fn len(&self) -> TextSize {
        TextSize::from(self.bytes.len() as u32)
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn has_bom(&self) -> bool {
        self.has_bom
    }

    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// The line (0-based) containing the byte offset.
    pub fn line_index(&self, pos: TextSize) -> u32 {
        let pos: u32 = pos.into();
        match self.line_starts.binary_search(&pos) {
            Ok(i) => i as u32,
            Err(i) => (i - 1) as u32,
        }
    }

    /// Byte offset of the start of a line (0-based).
    pub fn line_start(&self, line: u32) -> TextSize {
        TextSize::from(self.line_starts[line as usize])
    }

    /// Bytes of a line (0-based), excluding the line break.
    pub fn line_bytes(&self, line: u32) -> &[u8] {
        let start = self.line_starts[line as usize] as usize;
        let end = if (line as usize) + 1 < self.line_starts.len() {
            self.line_starts[line as usize + 1] as usize
        } else {
            self.bytes.len()
        };
        let mut slice = &self.bytes[start..end];
        if slice.ends_with(b"\n") {
            slice = &slice[..slice.len() - 1];
        }
        if slice.ends_with(b"\r") {
            slice = &slice[..slice.len() - 1];
        }
        slice
    }

    /// Line and column (1-based). The column counts scalar values; a leading BOM is not counted.
    pub fn line_col(&self, pos: TextSize) -> LineCol {
        let line = self.line_index(pos);
        let mut start = self.line_starts[line as usize] as usize;
        if line == 0 && self.has_bom {
            start = start.max(3);
        }
        let pos = usize::from(pos).max(start);
        let column = count_scalars(&self.bytes[start..pos]) as u32 + 1;
        LineCol {
            line: line + 1,
            column,
        }
    }

    pub fn text(&self, range: TextRange) -> &[u8] {
        &self.bytes[usize::from(range.start())..usize::from(range.end())]
    }
}

/// Number of scalar values in a byte string (an invalid sequence counts as one U+FFFD, by the same rule as `from_utf8_lossy`).
pub fn count_scalars(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.chars().count(),
        Err(_) => String::from_utf8_lossy(bytes).chars().count(),
    }
}

fn compute_line_starts(bytes: &[u8]) -> Vec<u32> {
    let mut starts = vec![0u32];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                starts.push((i + 1) as u32);
            }
            b'\r' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                    i += 1;
                }
                starts.push((i + 1) as u32);
            }
            _ => {}
        }
        i += 1;
    }
    starts
}

/// Table of the files in the compilation unit.
#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, file: SourceFile) -> FileId {
        self.files.push(file);
        FileId(self.files.len() as u32 - 1)
    }

    pub fn get(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn line_col(&self, span: Span) -> LineCol {
        self.get(span.file).line_col(span.range.start())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_basic() {
        let f = SourceFile::new("a.pas", b"abc\nd\xc3\xa9f\r\nx".to_vec());
        assert_eq!(f.line_col(0.into()), LineCol { line: 1, column: 1 });
        assert_eq!(f.line_col(3.into()), LineCol { line: 1, column: 4 });
        assert_eq!(f.line_col(4.into()), LineCol { line: 2, column: 1 });
        // 'é' is 2 bytes and 1 column
        assert_eq!(f.line_col(7.into()), LineCol { line: 2, column: 3 });
        assert_eq!(f.line_col(10.into()), LineCol { line: 3, column: 1 });
        assert_eq!(f.line_count(), 3);
        assert_eq!(f.line_bytes(1), b"d\xc3\xa9f");
    }

    #[test]
    fn eof_after_newline_is_next_line() {
        let f = SourceFile::new("a.pas", b"abc\n".to_vec());
        assert_eq!(f.line_col(4.into()), LineCol { line: 2, column: 1 });
        let f = SourceFile::new("a.pas", b"abc".to_vec());
        assert_eq!(f.line_col(3.into()), LineCol { line: 1, column: 4 });
        let f = SourceFile::new("a.pas", Vec::new());
        assert_eq!(f.line_col(0.into()), LineCol { line: 1, column: 1 });
    }

    #[test]
    fn bom_not_counted() {
        let f = SourceFile::new("a.pas", b"\xEF\xBB\xBFprogram".to_vec());
        assert_eq!(f.line_col(3.into()), LineCol { line: 1, column: 1 });
        assert_eq!(f.line_col(0.into()), LineCol { line: 1, column: 1 });
    }

    #[test]
    fn cr_only_newlines() {
        let f = SourceFile::new("a.pas", b"a\rb\rc".to_vec());
        assert_eq!(f.line_col(2.into()), LineCol { line: 2, column: 1 });
        assert_eq!(f.line_col(4.into()), LineCol { line: 3, column: 1 });
    }
}

/// Interface for receiving source text from the caller (`docs/design/host-interface.md` 3.5).
///
/// `read` returns raw bytes (the lexer checks UTF-8). A file over the size limit may be
/// refused without reading it, with `HostError` (`Other`, `message` is `file is too large (limit 4294967294 bytes)`).
pub trait SourceProvider {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, puy_host::HostError>;

    /// The files under the directory `dir` and its subdirectories, as paths relative to `dir`
    /// with `/` between the parts (the `src/` of a package; `docs/spec/units.md` 2.2). A provider
    /// that serves no directories returns none.
    fn list(&mut self, dir: &str) -> Result<Vec<String>, puy_host::HostError> {
        let _ = dir;
        Ok(Vec::new())
    }
}

/// A `SourceProvider` that serves in-memory files (for tests and the Playground).
#[derive(Debug, Default, Clone)]
pub struct MemorySourceProvider {
    pub files: Vec<(String, Vec<u8>)>,
}

impl MemorySourceProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file(mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.push((path.into(), bytes.into()));
        self
    }
}

impl SourceProvider for MemorySourceProvider {
    fn list(&mut self, dir: &str) -> Result<Vec<String>, puy_host::HostError> {
        let prefix = if dir.is_empty() || dir.ends_with('/') {
            dir.to_string()
        } else {
            format!("{dir}/")
        };
        Ok(self
            .files
            .iter()
            .filter_map(|(p, _)| p.strip_prefix(&prefix).map(str::to_string))
            .collect())
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, puy_host::HostError> {
        match self.files.iter().find(|(p, _)| p == path) {
            Some((_, b)) => Ok(b.clone()),
            None => Err(puy_host::HostError::new(
                puy_host::HostErrorKind::NotFound,
                format!("{path}: file not found"),
            )),
        }
    }
}

/// Fixed text for a file over the size limit (`docs/spec/diagnostics.md` 4.11).
pub const TOO_LARGE_MESSAGE: &str = "file is too large (limit 4294967294 bytes)";
