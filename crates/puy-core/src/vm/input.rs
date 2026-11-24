//! The input buffer of `Read` / `ReadLn` / `Eof` / `Eoln` and of the standard input stream
//! (`docs/spec/semantics.md` 6.2, `docs/spec/bytecode.md` 2.17, 2.22; `docs/design/stdlib.md`
//! 3.7).
//!
//! The VM keeps the bytes the host gave and not read yet; the lines of `ReadLn` are cut from them
//! here (the line break is `LF`, with a `CR` before it) and decoded from UTF-8 (an invalid
//! sequence becomes U+FFFD), and the stream reads the same bytes, so that the two read in turn.
//! The instructions are restartable: one that needs more bytes asks the host for them and runs
//! again from the start once they are in the buffer (the position it reached is kept here).

use super::heap::{OutOfMemory, try_extend, try_reserve};
use super::strconv::is_blank;
use super::{Extra, Isolate, PStr, RtError, Step, oom, rt};
use crate::bytecode::{exc, op};
use crate::sema::types::MAX_STRING_LEN;

/// The most bytes asked of the host at a time.
pub(super) const READ_CHUNK: usize = 64 * 1024;

/// What the VM holds of standard input.
#[derive(Debug, Default)]
pub(super) struct InputBuf {
    /// The bytes from the host, read from `head` on (the current line, if any, starts there).
    raw: Vec<u8>,
    head: usize,
    /// Where the search for the next line break goes on (none in `head..scanned`).
    scanned: usize,
    /// The current line (decoded, without its line break) and the position read up to in it.
    /// `None` once the line break has been read (or before the first line).
    line: Option<Line>,
    pos: usize,
    /// The host returned no bytes (the end of input; not asked again).
    eof: bool,
}

/// A line cut from the bytes at `head`.
#[derive(Debug)]
struct Line {
    units: Vec<u16>,
    /// Its bytes without the line break, and with it.
    text: usize,
    len: usize,
}

/// What an instruction needs before it can go on.
enum Need {
    /// More bytes from the host.
    Input,
    /// A line longer than a string can be.
    TooLong,
}

/// The `Char` of `Read` at the end of a line (the line break; `docs/spec/semantics.md` 6.2).
const LINE_BREAK: u16 = 10;

/// Decodes the UTF-8 `bytes` (each invalid sequence one U+FFFD, as `String::from_utf8_lossy`),
/// giving each UTF-16 code unit with the offset of the bytes it came from.
fn decode(bytes: &[u8], mut f: impl FnMut(u16, usize)) {
    let mut base = 0;
    for chunk in bytes.utf8_chunks() {
        let valid = chunk.valid();
        for (i, c) in valid.char_indices() {
            let mut buf = [0; 2];
            for u in c.encode_utf16(&mut buf) {
                f(*u, base + i);
            }
        }
        base += valid.len();
        if !chunk.invalid().is_empty() {
            f(0xFFFD, base);
            base += chunk.invalid().len();
        }
    }
}

/// The offset in the UTF-8 `bytes` of the code unit `pos` of their decoding ([`decode`]); in the
/// middle of a surrogate pair, the offset of the character after it. The length at the end.
fn byte_offset(bytes: &[u8], pos: usize) -> usize {
    let mut index = 0;
    let mut found = None;
    decode(bytes, |u, offset| {
        if found.is_none() && index >= pos && !(0xDC00..=0xDFFF).contains(&u) {
            found = Some(offset);
        }
        index += 1;
    });
    found.unwrap_or(bytes.len())
}

impl InputBuf {
    /// The current line, or what is needed first (`Ok(None)` at the end of input).
    fn current(&mut self) -> Result<Option<(&[u16], &mut usize)>, Need> {
        if self.line.is_none() && !self.cut_line()? {
            return Ok(None);
        }
        let line = self.line.as_ref().expect("cut_line made a line");
        Ok(Some((&line.units, &mut self.pos)))
    }

    /// Cuts the line at `head` from the bytes into `line`. `Ok(false)` at the end of input.
    fn cut_line(&mut self) -> Result<bool, Need> {
        let start = self.head;
        let from = self.scanned.max(start);
        let (text_end, end) = match self.raw[from..].iter().position(|b| *b == b'\n') {
            Some(i) => {
                let lf = from + i;
                let text_end = if lf > start && self.raw[lf - 1] == b'\r' {
                    lf - 1
                } else {
                    lf
                };
                (text_end, lf + 1)
            }
            None if !self.eof => {
                self.scanned = self.raw.len();
                return Err(Need::Input);
            }
            // The last line, without a line break
            None if start < self.raw.len() => (self.raw.len(), self.raw.len()),
            None => return Ok(false),
        };
        let bytes = &self.raw[start..text_end];
        // A UTF-16 code unit takes at least one byte, so the count is at most the bytes'
        let mut count = 0;
        decode(bytes, |_, _| count += 1);
        let mut units = Vec::new();
        if count > MAX_STRING_LEN || try_reserve(&mut units, count).is_err() {
            // The line is dropped, as it could not be read
            self.head = end;
            self.scanned = end;
            return Err(Need::TooLong);
        }
        decode(bytes, |u, _| units.push(u));
        self.line = Some(Line {
            units,
            text: text_end - start,
            len: end - start,
        });
        self.pos = 0;
        Ok(true)
    }

    /// Drops the current line with its line break (the next is read from the bytes after it).
    fn next_line(&mut self) {
        if let Some(line) = self.line.take() {
            self.head += line.len;
            self.scanned = self.head;
        }
    }

    /// A word for a number: blanks are skipped (line ends too when `cross`: in `Read`), then up
    /// to the next blank or line end. `''` when there is none (or at the end of input).
    fn word(&mut self, cross: bool) -> Result<Result<PStr, OutOfMemory>, Need> {
        loop {
            let Some((line, pos)) = self.current()? else {
                return Ok(PStr::from_units(&[]));
            };
            while *pos < line.len() && is_blank(line[*pos]) {
                *pos += 1;
            }
            if *pos == line.len() && cross {
                self.next_line();
                continue;
            }
            let start = *pos;
            while *pos < line.len() && !is_blank(line[*pos]) {
                *pos += 1;
            }
            return Ok(PStr::from_units(&line[start..*pos]));
        }
    }

    /// From the position to the end of the line (`''` at the end of input).
    fn rest(&mut self) -> Result<Result<PStr, OutOfMemory>, Need> {
        let Some((line, pos)) = self.current()? else {
            return Ok(PStr::from_units(&[]));
        };
        let start = *pos;
        *pos = line.len();
        Ok(PStr::from_units(&line[start..]))
    }

    /// One code unit; the line break at the end of a line (`Read` then goes on with the next
    /// line, when `consume`: in `Read`). `None` at the end of input.
    fn char(&mut self, consume: bool) -> Result<Option<u16>, Need> {
        let Some((line, pos)) = self.current()? else {
            return Ok(None);
        };
        if *pos < line.len() {
            let u = line[*pos];
            *pos += 1;
            return Ok(Some(u));
        }
        if consume {
            self.next_line();
        }
        Ok(Some(LINE_BREAK))
    }

    /// The end of `ReadLn`: the rest of the line is dropped (a line is read and dropped if none is
    /// in the buffer).
    fn end_line(&mut self) -> Result<(), Need> {
        if self.current()?.is_some() {
            self.next_line();
        }
        Ok(())
    }

    /// Drops the rest of the current line (an exception left a `ReadLn` statement; [`super::task::Task::readln_left`]).
    pub(super) fn drop_line(&mut self) {
        self.next_line();
    }

    /// `Eof`: nothing left to read in the buffer and no next line. In a line read from already,
    /// trailing blanks do not count (a last line `1 2 ` ends a `Read` loop after the `2`).
    fn at_eof(&mut self) -> Result<bool, Need> {
        let Some((line, pos)) = self.current()? else {
            return Ok(true);
        };
        let rest = &line[*pos..];
        let only_blanks = *pos > 0 && rest.iter().all(|u| is_blank(*u));
        if !rest.is_empty() && !only_blanks {
            return Ok(false);
        }
        // A byte after the line is the start of the next one
        let after = self.head + self.line.as_ref().map_or(0, |l| l.len);
        if after < self.raw.len() {
            return Ok(false);
        }
        if self.eof {
            return Ok(true);
        }
        Err(Need::Input)
    }

    /// `Eoln`: at the end of a line, or `Eof`.
    fn at_eoln(&mut self) -> Result<bool, Need> {
        Ok(match self.current()? {
            Some((line, pos)) => *pos == line.len(),
            None => true,
        })
    }

    /// How many bytes the standard input stream (`ConsoleRead`) can read now, up to `max`, from
    /// where `Read` / `ReadLn` stopped (the rest of a line read from, and its line break, come
    /// first); 0 at the end of input. Reads nothing: [`InputBuf::take`] does, once the bytes may
    /// be written.
    fn available(&mut self, max: usize) -> Result<usize, Need> {
        // The position in a line read from becomes the position in the bytes (the same bytes
        // are left to read, so doing it again changes nothing)
        if let Some(line) = self.line.take() {
            let text = &self.raw[self.head..self.head + line.text];
            self.head += byte_offset(text, self.pos);
            self.scanned = self.head;
        }
        let avail = self.raw.len() - self.head;
        if avail == 0 && max > 0 && !self.eof {
            return Err(Need::Input);
        }
        Ok(avail.min(max))
    }

    /// Reads `n` bytes [`InputBuf::available`] said there are.
    fn take(&mut self, n: usize) -> &[u8] {
        let start = self.head;
        self.head += n;
        self.scanned = self.scanned.max(self.head);
        &self.raw[start..self.head]
    }

    /// Stores bytes from the host (none: the end of input).
    pub(super) fn receive(&mut self, bytes: &[u8]) -> Result<(), RtError> {
        if bytes.is_empty() {
            self.eof = true;
            return Ok(());
        }
        // The bytes before `head` have been read
        self.raw.drain(..self.head);
        self.scanned -= self.head;
        self.head = 0;
        try_extend(&mut self.raw, bytes).map_err(|OutOfMemory| oom())
    }
}

/// `EInOutError` of a failed read (`ErrorCode` 0: not an OS error).
pub(super) fn input_error(message: String) -> RtError {
    RtError {
        class: exc::IN_OUT_ERROR,
        message,
        extra: Extra::Int(0),
    }
}

/// What an instruction writes to `rA`.
enum Out {
    Nothing,
    Str(Result<PStr, OutOfMemory>),
    Int(i64),
}

impl Isolate {
    /// The intrinsics of 2.17 (`id`; `pc` is this instruction, `next` the one after it).
    pub(super) fn input_op(&mut self, id: u8, ra: usize, pc: u32, next: u32) -> Step {
        if id == op::intr::FLUSH_OUT {
            return self.flush_out(next);
        }
        let depth = self.task.frames.len();
        let r: Result<Out, Need> = match id {
            op::intr::READ_WORD => self.input.word(!self.task.in_readln(depth)).map(Out::Str),
            op::intr::READ_REST => self.input.rest().map(Out::Str),
            op::intr::READ_LN_BEGIN => {
                self.task.readln.push(depth);
                Ok(Out::Nothing)
            }
            op::intr::READ_CHAR => match self.input.char(!self.task.in_readln(depth)) {
                Ok(Some(u)) => Ok(Out::Int(u as i64)),
                Ok(None) => return Step::Error(input_error("read past end of input".into())),
                Err(n) => Err(n),
            },
            op::intr::READ_LN_END => self.input.end_line().map(|()| {
                self.task.readln.pop();
                Out::Nothing
            }),
            op::intr::IN_EOF => self.input.at_eof().map(|b| Out::Int(b as i64)),
            op::intr::IN_EOLN => self.input.at_eoln().map(|b| Out::Int(b as i64)),
            _ => return Step::Fatal(format!("unknown input intrinsic {id}")),
        };
        match r {
            Ok(out) => {
                match out {
                    Out::Nothing => {}
                    Out::Int(v) => self.task.regs[ra] = v,
                    Out::Str(text) => {
                        let s = match text.map_err(|_| oom()).and_then(|p| self.alloc_string(p)) {
                            Ok(s) => s,
                            Err(e) => return Step::Error(e),
                        };
                        if let Err(e) = self.put(ra, s) {
                            return Step::Fatal(e.message);
                        }
                    }
                }
                self.task.frames.last_mut().expect("a frame runs").pc = next;
                Step::Continue
            }
            Err(need) => self.need(need, pc),
        }
    }

    /// The input stream's and output streams' intrinsics of 2.22 (`ConsoleRead`, `ConsoleWrite`,
    /// `ConsoleWriteError`): `rA :=` the bytes moved between the byte array `rB` (0 is empty) from
    /// `r(B+1)`, `r(B+2)` of them, and the VM's buffers. `pc` is this instruction, `next` the one
    /// after it.
    pub(super) fn console_op(&mut self, id: u8, ra: usize, rb: usize, pc: u32, next: u32) -> Step {
        let regs = &self.task.regs;
        let (buf, index, count) = (regs[rb], regs[rb + 1], regs[rb + 2]);
        let checked = self.heap.bytes_of(buf).map(<[u8]>::len).map_err(Into::into);
        let checked = checked.and_then(|len| super::check_array_range(index, count, len));
        if let Err(e) = checked {
            return e.into_step();
        }
        // In range, so they fit in `usize`
        let (index, count) = (index as usize, count as usize);
        let moved = match id {
            op::intr::CONSOLE_READ => {
                let n = match self.input.available(count) {
                    Ok(n) => n,
                    Err(need) => return self.need(need, pc),
                };
                // The array is written in place, as `BytesCopy` writes; checked before a byte is
                // read, so that a failure leaves the input as it was
                if let Err(e) = self.heap.check_slots(buf, index as i64, (index + n) as i64) {
                    return super::RefError::from(e).into_step();
                }
                let bytes = self.input.take(n);
                match self.heap.bytes_of_mut(buf) {
                    Ok(dst) => dst[index..index + n].copy_from_slice(bytes),
                    Err(e) => return Step::Fatal(e.message),
                }
                n
            }
            op::intr::CONSOLE_WRITE | op::intr::CONSOLE_WRITE_ERROR => {
                let mut data = Vec::new();
                match self.heap.bytes_of(buf) {
                    Ok(b) => {
                        if try_extend(&mut data, &b[index..index + count]).is_err() {
                            return Step::Error(oom());
                        }
                    }
                    Err(e) => return Step::Fatal(e.message),
                }
                self.task.regs[ra] = count as i64;
                return if id == op::intr::CONSOLE_WRITE {
                    self.write_out(&data, next)
                } else {
                    self.write_err(&data, pc, next)
                };
            }
            _ => return Step::Fatal(format!("unknown console intrinsic {id}")),
        };
        self.task.regs[ra] = moved as i64;
        self.task.frames.last_mut().expect("a frame runs").pc = next;
        Step::Continue
    }

    /// Goes on with what an instruction at `pc` needs: asks the host for more input (and runs the
    /// instruction again once it is in the buffer), or fails.
    fn need(&mut self, need: Need, pc: u32) -> Step {
        match need {
            Need::Input => self.request_input(pc),
            Need::TooLong => Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
        }
    }
}
