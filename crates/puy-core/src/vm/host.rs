//! Calls to host functions (`docs/design/vm.md` 3.10, `docs/design/extensions.md` 3.7,
//! `docs/spec/bytecode.md` 2.5, 2.6).
//!
//! Every suspendable call takes one path, whatever it calls: the host returns a
//! [`HostFuture`], which is polled once at once; a result that is ready goes to where the call
//! said ([`Deliver`]) as a VM value, and a call that is not done yet is kept by the task as a
//! [`Pending`] call (its result made into an owned [`Value`], its failure a [`PasError`]) and
//! polled again when the task runs again. Writing out standard output, `Console.WriteError`, the
//! bytes of the input buffer and the suspendable functions of extensions (`ext.rs`) go this way.
//! A synchronous call hands its result to the same delivery.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use puy_ext::{PasError, Value};
use puy_host::{HostFuture, HostSet, OutStream};

use super::heap::{OutOfMemory, PStr};
use super::task::Pending;
use super::{Extra, HostFn, Isolate, PollError, RtError, Slot, Step, noop_waker, oom, rt};
use crate::bytecode::exc;
use crate::sema::types::MAX_STRING_LEN;

/// The message of a suspendable call that would wait inside a callback from an extension
/// (`docs/spec/extensions.md` 5.2).
pub(super) const WAIT_IN_CALLBACK: &str = "cannot wait inside a callback from an extension";

/// Where the result of a host call goes once it is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Deliver {
    /// Nowhere (a write).
    Drop,
    /// Into a register (absolute position), letting go of what it held; [`HostValue::Nothing`]
    /// leaves the register as it is (`docs/spec/bytecode.md` 2.5).
    Reg(u32),
    /// Into the input buffer (`input.rs`).
    Input,
    /// The result of the extension function `index`, into register `reg` (`ext.rs`).
    Ext { reg: u32, index: u32 },
}

/// The future of a host call made into one of [`Value`], to be kept by a task.
struct Erased<T>(HostFuture<T>);

impl<T: Into<Value>> Future for Erased<T> {
    type Output = Result<Value, PasError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .as_mut()
            .poll(cx)
            .map(|r| r.map(Into::into).map_err(PasError::from))
    }
}

impl Isolate {
    /// `CallHost` (`docs/spec/bytecode.md` 2.5): the host function `index` with its argument in
    /// register `dst` (absolute position). `pc` is the instruction, `next` the one after it; the
    /// frame goes on at `next` when the call is done (now, or when the task waited for it).
    pub(super) fn call_host(&mut self, index: usize, dst: u32, pc: u32, next: u32) -> Step {
        let kind = self.host_table[index];
        match kind {
            HostFn::ConsoleWrite => {
                let text = try_oom!(get_str!(self.heap, self.task.regs[dst as usize]).to_utf8());
                self.write_out(text.as_bytes(), next)
            }
            HostFn::ConsoleWriteError => {
                let text = try_oom!(get_str!(self.heap, self.task.regs[dst as usize]).to_utf8());
                self.write_err(text.as_bytes(), pc, next)
            }
            HostFn::EnvGet => {
                let name = try_oom!(get_str!(self.heap, self.task.regs[dst as usize]).to_utf8());
                // The isolate's own changes first, then the host's `Env` (none: unset)
                let value = match self.env.get(&mut self.host, &name) {
                    Ok(v) => v,
                    Err(e) => return self.ext_failure(PasError::from(e), None),
                };
                // An unset variable is `''`; the value takes the place of the name
                let value = Value::Str(value.unwrap_or_default());
                self.host_done(Ok(value), Deliver::Reg(dst), next)
            }
        }
    }

    /// Writes out the standard output buffer through the host and goes on at `resume_pc` once it
    /// is written (at once when the buffer is empty or there is no `Console`; §9.5).
    pub(super) fn flush_out(&mut self, resume_pc: u32) -> Step {
        match self.take_stdout() {
            Some(fut) => self.await_host(fut, Deliver::Drop, resume_pc),
            None => {
                self.set_pc(resume_pc);
                Step::Continue
            }
        }
    }

    /// Appends `bytes` to the standard output buffer and writes it out if `stdout_buffering` says
    /// so (`Write` / `WriteLn` and the output stream); goes on at `next` once it is written.
    pub(super) fn write_out(&mut self, bytes: &[u8], next: u32) -> Step {
        if let Err(e) = self.write_stdout(bytes) {
            return Step::Error(e);
        }
        if self.stdout_due(bytes) {
            return self.flush_out(next);
        }
        self.set_pc(next);
        Step::Continue
    }

    /// Writes `bytes` to standard error at once, after writing out the standard output buffer
    /// (`ErrOutput` and the error stream; dropped without a `Console`). If writing out standard
    /// output waits, the instruction at `pc` runs again once it is written, and finds the buffer
    /// empty; the frame goes on at `next` once `bytes` is written.
    pub(super) fn write_err(&mut self, bytes: &[u8], pc: u32, next: u32) -> Step {
        match self.flush_out(pc) {
            Step::Continue => {}
            step => return step,
        }
        let Some(console) = self.host.console.as_mut() else {
            self.set_pc(next);
            return Step::Continue;
        };
        let fut = console.write(OutStream::Stderr, bytes);
        self.await_host(fut, Deliver::Drop, next)
    }

    /// Asks the host for more input; the instruction at `pc` runs again once it is in the buffer
    /// (`input.rs`). Standard output is written out before reading
    /// (`docs/spec/semantics.md` 6.1); if that waits, the instruction runs again once it is
    /// written, and asks then.
    pub(super) fn request_input(&mut self, pc: u32) -> Step {
        match self.flush_out(pc) {
            Step::Continue => {}
            step => return step,
        }
        let Some(console) = self.host.console.as_mut() else {
            let kind = puy_ext::io_error_kind(self.host.absent.console.error("Console.Read").kind);
            return Step::Error(RtError {
                class: exc::NOT_SUPPORTED,
                message: "not supported on this target: Console.Read".into(),
                extra: Extra::Host { code: 0, kind },
            });
        };
        let fut = console.read(super::input::READ_CHUNK);
        self.await_host(fut, Deliver::Input, pc)
    }

    /// The offset of the local time from UTC in seconds at the instant `nanos` (`LocalOffset`,
    /// `I118`): 0 when the host cannot tell (UTC). Without a clock, `ENotSupportedException`.
    pub(super) fn local_offset(host: &mut HostSet, nanos: i64) -> Result<i32, RtError> {
        let absent = host.absent.clock;
        match host.clock.as_mut() {
            Some(clock) => match clock.local_offset(nanos) {
                Ok(offset) => Ok(offset.unwrap_or(0)),
                Err(e) => Err(host_rt_error(&e)),
            },
            None => Err(host_rt_error(&absent.error("the clock"))),
        }
    }

    /// The path of every suspendable host call: polls `fut` once; if it is done, delivers the
    /// result and goes on at `resume_pc`; if not, the task keeps it and the run waits
    /// ([`Isolate::poll_pending`] polls it again). A failure leaves `pc` at the instruction, so
    /// that the exception is reported there.
    pub(super) fn await_host<T: Into<Value> + 'static>(
        &mut self,
        mut fut: HostFuture<T>,
        deliver: Deliver,
        resume_pc: u32,
    ) -> Step {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(r) => self.host_done(
                r.map(Into::into).map_err(PasError::from),
                deliver,
                resume_pc,
            ),
            // A callback from an extension cannot wait (`docs/spec/extensions.md` 5.2)
            Poll::Pending if self.ext_depth() > 0 => {
                Step::Error(rt(exc::INVALID_OPERATION, WAIT_IN_CALLBACK))
            }
            Poll::Pending => {
                self.task.pending = Some(Pending {
                    fut: Box::pin(Erased(fut)),
                    deliver,
                    resume_pc,
                });
                Step::Waiting
            }
        }
    }

    /// A host call is done: delivers its result and goes on at `resume_pc`.
    fn host_done(
        &mut self,
        result: Result<Value, PasError>,
        deliver: Deliver,
        resume_pc: u32,
    ) -> Step {
        match self.deliver(result, deliver, resume_pc) {
            Ok(()) => Step::Continue,
            Err(PollError::Rt(e)) => Step::Error(e),
            Err(PollError::Throw(x)) => Step::Throw(x),
            Err(PollError::Halt) => Step::Halt(None),
            Err(PollError::Fatal(e)) => Step::Fatal(e.message),
        }
    }

    fn deliver(
        &mut self,
        result: Result<Value, PasError>,
        deliver: Deliver,
        resume_pc: u32,
    ) -> Result<(), PollError> {
        let unit = match deliver {
            Deliver::Ext { index, .. } => Some(self.module.exts[index as usize].unit),
            _ => None,
        };
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                return Err(match self.pas_error(e, unit) {
                    Ok(Ok(rt)) => PollError::Rt(rt),
                    Ok(Err(x)) => PollError::Throw(x),
                    Err(None) => PollError::Halt,
                    Err(Some(m)) => PollError::Fatal(super::heap::BadRef { message: m }),
                });
            }
        };
        match deliver {
            Deliver::Drop => {}
            Deliver::Reg(r) => {
                if let Some(v) = self.host_value_slot(value)? {
                    self.put(r as usize, v).map_err(PollError::Fatal)?;
                }
            }
            Deliver::Input => {
                let bytes = match value {
                    Value::Bytes(b) => b,
                    _ => Vec::new(),
                };
                self.input.receive(&bytes)?;
            }
            Deliver::Ext { reg, index } => {
                match self.deliver_ext(value, index as usize, reg) {
                    Ok(()) => {}
                    Err(super::ext::ValueFail::Rt(e)) => return Err(PollError::Rt(e)),
                    Err(super::ext::ValueFail::Fatal(m)) => {
                        return Err(PollError::Fatal(super::heap::BadRef { message: m }));
                    }
                }
                self.drain_ext_queue().map_err(PollError::Fatal)?;
            }
        }
        self.set_pc(resume_pc);
        Ok(())
    }

    /// The Pascal value of a host value, with a reference of its own (`None`: no value).
    fn host_value_slot(&mut self, v: Value) -> Result<Option<Slot>, PollError> {
        match v {
            Value::Str(text) => {
                // Strings from the host are UTF-8 (`docs/spec/bytecode.md` 2.6)
                if text.encode_utf16().count() > MAX_STRING_LEN {
                    return Err(rt(exc::OUT_OF_MEMORY, "out of memory").into());
                }
                let s = PStr::from_text(&text).map_err(|OutOfMemory| oom())?;
                Ok(Some(self.alloc_string(s)?))
            }
            _ => Ok(None),
        }
    }

    /// Polls the host call the task waits for, if any. `Ok(true)` when there is none (any more):
    /// the result has been delivered and the frame goes on at the call's `resume_pc`.
    pub(super) fn poll_pending(&mut self) -> Result<bool, PollError> {
        let Some(p) = self.task.pending.as_mut() else {
            return Ok(true);
        };
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let Poll::Ready(result) = p.fut.as_mut().poll(&mut cx) else {
            return Ok(false);
        };
        let p = self.task.pending.take().expect("a call is pending");
        self.deliver(result, p.deliver, p.resume_pc)?;
        Ok(true)
    }

    pub(super) fn set_pc(&mut self, pc: u32) {
        if let Some(fr) = self.task.frames.last_mut() {
            fr.pc = pc;
        }
    }
}

/// The exception of a host error (`docs/spec/bytecode.md` chapter 4), for the VM's own
/// synchronous host calls: the mapping of [`puy_ext::host_exception`].
pub(super) fn host_rt_error(e: &puy_host::HostError) -> RtError {
    let (not_supported, code, kind) = puy_ext::host_exception(e);
    RtError {
        class: if not_supported {
            exc::NOT_SUPPORTED
        } else {
            exc::IN_OUT_ERROR
        },
        message: e.message.clone(),
        extra: Extra::Host { code, kind },
    }
}

#[cfg(test)]
mod tests {
    use puy_ext::Value;

    // The bytes of `Console::read` stay bytes as values, the empty result of the end of input
    // too (`Deliver::Input` reads only `Value::Bytes`).
    #[test]
    fn read_results_as_values() {
        assert!(matches!(Value::from(b"a\n".to_vec()), Value::Bytes(b) if b == b"a\n"));
        assert!(matches!(Value::from(Vec::<u8>::new()), Value::Bytes(b) if b.is_empty()));
    }
}
