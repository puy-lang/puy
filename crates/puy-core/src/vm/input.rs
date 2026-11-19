//! The input line buffer of `Read` / `ReadLn` / `Eof` / `Eoln` (`docs/spec/semantics.md` 6.2,
//! `docs/spec/bytecode.md` 2.17).
//!
//! The instructions are restartable: one that needs another line asks the host for it and runs
//! again from the start once the line is in the buffer (the position it reached is kept here).

use super::heap::{OutOfMemory, try_reserve};
use super::host_error_to_rt;
use super::strconv::is_blank;
use super::{Extra, Isolate, PStr, PendingHost, PendingKind, PollError, RtError, Step, oom, rt};
use crate::bytecode::{exc, op};
use crate::sema::types::MAX_STRING_LEN;

/// What the VM holds of standard input.
#[derive(Debug, Default)]
pub(super) struct InputBuf {
    /// The current line (without its line break) and the position read up to. `None` once the
    /// line break has been read (or before the first line).
    line: Option<Vec<u16>>,
    pos: usize,
    /// The next line, read ahead by `Eof` at the end of the current line.
    ahead: Option<Vec<u16>>,
    /// `Console.read_line` returned `None` (the end of input; not asked again).
    eof: bool,
}

/// What an instruction needs before it can go on.
enum Need {
    /// A line into the buffer (`ahead`: the next line, read ahead).
    Line { ahead: bool },
}

/// The `Char` of `Read` at the end of a line (the line break; `docs/spec/semantics.md` 6.2).
const LINE_BREAK: u16 = 10;

impl InputBuf {
    /// Moves a line read ahead into the buffer. Returns whether there is a line.
    fn take_ahead(&mut self) -> bool {
        if self.line.is_none()
            && let Some(a) = self.ahead.take()
        {
            self.line = Some(a);
            self.pos = 0;
        }
        self.line.is_some()
    }

    /// The current line, or what is needed first (`Ok(None)` at the end of input).
    fn current(&mut self) -> Result<Option<(&[u16], &mut usize)>, Need> {
        if !self.take_ahead() {
            if self.eof {
                return Ok(None);
            }
            return Err(Need::Line { ahead: false });
        }
        let line = self.line.as_deref().expect("take_ahead returned true");
        Ok(Some((line, &mut self.pos)))
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
                self.line = None;
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
            self.line = None;
        }
        Ok(Some(LINE_BREAK))
    }

    /// The end of `ReadLn`: the rest of the line is dropped (a line is read and dropped if none is
    /// in the buffer).
    fn end_line(&mut self) -> Result<(), Need> {
        if self.current()?.is_some() {
            self.line = None;
        }
        Ok(())
    }

    /// Drops the rest of the current line (an exception left a `ReadLn` statement; [`super::task::Task::readln_left`]).
    pub(super) fn drop_line(&mut self) {
        self.line = None;
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
        if self.ahead.is_some() {
            return Ok(false);
        }
        if self.eof {
            return Ok(true);
        }
        Err(Need::Line { ahead: true })
    }

    /// `Eoln`: at the end of a line, or `Eof`.
    fn at_eoln(&mut self) -> Result<bool, Need> {
        Ok(match self.current()? {
            Some((line, pos)) => *pos == line.len(),
            None => true,
        })
    }

    /// Stores a line from the host (`None`: the end of input).
    pub(super) fn receive(&mut self, line: Option<String>, ahead: bool) -> Result<(), RtError> {
        let Some(text) = line else {
            self.eof = true;
            return Ok(());
        };
        if text.encode_utf16().count() > MAX_STRING_LEN {
            return Err(rt(exc::OUT_OF_MEMORY, "out of memory"));
        }
        let mut units = Vec::new();
        try_reserve(&mut units, text.encode_utf16().count()).map_err(|_| oom())?;
        units.extend(text.encode_utf16());
        if ahead {
            self.ahead = Some(units);
        } else {
            self.line = Some(units);
            self.pos = 0;
        }
        Ok(())
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
            if let Err(e) = self.flush_stdout() {
                return Step::Error(host_error_to_rt(e));
            }
            self.task.frames.last_mut().expect("a frame runs").pc = next;
            return Step::Continue;
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
            Err(Need::Line { ahead }) => self.request_line(ahead, pc),
        }
    }

    /// Asks the host for a line; the instruction at `pc` runs again once it is in the buffer.
    fn request_line(&mut self, ahead: bool, pc: u32) -> Step {
        // Standard output is written out before reading (`docs/spec/semantics.md` 6.1)
        if let Err(e) = self.flush_stdout() {
            return Step::Error(host_error_to_rt(e));
        }
        let Some(console) = self.host.console.as_mut() else {
            return Step::Error(rt(
                exc::NOT_SUPPORTED,
                "not supported on this target: Console.ReadLine",
            ));
        };
        let fut = console.read_line();
        self.task.pending = Some(PendingHost {
            kind: PendingKind::ReadLine { fut, ahead },
            resume_pc: pc,
        });
        match self.poll_pending() {
            Ok(true) => Step::Continue,
            Ok(false) => Step::Waiting,
            Err(PollError::Rt(e)) => Step::Error(e),
            Err(PollError::Fatal(e)) => Step::Fatal(e.message),
        }
    }
}
