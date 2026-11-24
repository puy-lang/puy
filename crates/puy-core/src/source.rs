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
    /// The files joined from pieces of other files (a unit of the standard library kept in one
    /// file per type; `docs/spec/collections.md` 1.2), with their pieces in order.
    joined: Vec<(FileId, Vec<Piece>)>,
}

/// A piece of a joined file: its bytes from `start` to `end` are those of `file` from `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub start: TextSize,
    pub end: TextSize,
    pub file: FileId,
    pub offset: TextSize,
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

    /// Replaces the file `id` (its text and the pieces it was joined from): a unit of the
    /// standard library read again with more of its parts (`docs/spec/collections.md` 1.2).
    pub fn replace(&mut self, id: FileId, file: SourceFile) {
        self.files[id.0 as usize] = file;
        self.joined.retain(|(f, _)| *f != id);
    }

    /// Records that `file` is joined from `pieces` (in order, each starting where the one before
    /// it ends), so that [`SourceMap::resolve`] maps its positions back to the files they came
    /// from.
    pub fn join(&mut self, file: FileId, pieces: Vec<Piece>) {
        debug_assert!(
            pieces.windows(2).all(|w| w[0].end == w[1].start),
            "the pieces follow one another"
        );
        self.joined.push((file, pieces));
    }

    /// The files a joined file was joined from other than `own` (the file whose `{$I}` lines
    /// the others replace), in the order of their first pieces. Empty for a file that is not
    /// joined.
    pub fn joined_files(&self, file: FileId, own: &str) -> Vec<FileId> {
        let mut out: Vec<FileId> = Vec::new();
        for (_, pieces) in self.joined.iter().filter(|(f, _)| *f == file) {
            for p in pieces {
                if self.get(p.file).path != own && !out.contains(&p.file) {
                    out.push(p.file);
                }
            }
        }
        out
    }

    /// The span in the file it came from: a span of a joined file is moved to the piece its
    /// start is in, its end cut at the end of that piece. Other spans are returned as they are.
    pub fn resolve(&self, span: Span) -> Span {
        let Some((_, pieces)) = self.joined.iter().find(|(f, _)| *f == span.file) else {
            return span;
        };
        let start = span.range.start();
        // The last piece that starts at or before the position (an empty span at the end of a
        // piece belongs to the next piece, the end of the file to the last one)
        let k = pieces
            .partition_point(|p| p.start <= start)
            .saturating_sub(1);
        let Some(p) = pieces.get(k) else {
            return span;
        };
        let end = span.range.end().min(p.end).max(start);
        Span::new(
            p.file,
            TextRange::new(p.offset + (start - p.start), p.offset + (end - p.start)),
        )
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

    // A span of a joined file is moved to the file of the piece its start is in, with its end
    // cut at the end of that piece; spans of other files are left as they are.
    #[test]
    fn joined_spans_resolve_to_their_pieces() {
        let mut m = SourceMap::new();
        let joined = m.add(SourceFile::new("u.pas", b"ab\nXY\ncd\n".to_vec()));
        let own = m.add(SourceFile::new("u.pas", b"ab\n{$I x}\ncd\n".to_vec()));
        let part = m.add(SourceFile::new("x", b"XY\n".to_vec()));
        let piece = |start: u32, end: u32, file, offset: u32| Piece {
            start: start.into(),
            end: end.into(),
            file,
            offset: offset.into(),
        };
        m.join(
            joined,
            vec![
                piece(0, 3, own, 0),
                piece(3, 6, part, 0),
                piece(6, 9, own, 10),
            ],
        );
        let at = |s: u32, e: u32| {
            let r = m.resolve(Span::new(joined, TextRange::new(s.into(), e.into())));
            (r.file, u32::from(r.range.start()), u32::from(r.range.end()))
        };
        assert_eq!(at(1, 2), (own, 1, 2));
        assert_eq!(at(3, 5), (part, 0, 2));
        assert_eq!(at(4, 8), (part, 1, 3));
        assert_eq!(at(6, 6), (own, 10, 10));
        assert_eq!(at(7, 9), (own, 11, 13));
        assert_eq!(at(9, 9), (own, 13, 13));
        let other = Span::new(own, TextRange::new(1.into(), 2.into()));
        assert_eq!(m.resolve(other), other);
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
