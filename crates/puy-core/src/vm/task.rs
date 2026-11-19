//! The execution state of a task (`docs/design/stackless-vm.md` 3.1, 3.8): what each lightweight
//! task (requirements §7.1) needs of its own, apart from what the isolate shares among its tasks.
//!
//! An isolate runs one task today (the program: the units' `<init>`, `<main>`, the `<fini>`). It
//! holds the running task by value ([`super::Isolate`]'s `task`), so that the fast path reaches the
//! registers and frames as directly as when they were fields of the isolate; a scheduler (M5c)
//! keeps the other tasks aside and swaps one in to run it.

use puy_host::HostFuture;

use super::Slot;

/// The state of one task.
pub(super) struct Task {
    /// The registers of all the task's frames (`docs/design/vm.md` 3.14: each frame's window
    /// follows its base; never shrinks).
    pub(super) regs: Vec<Slot>,
    /// The call frames, the running one last.
    pub(super) frames: Vec<Frame>,
    /// The suspendable host call the task waits for (`docs/design/stackless-vm.md` 3.3).
    pub(super) pending: Option<PendingHost>,
    /// The `ReadLn` statements being run (between `ReadLnBegin` and `ReadLnEnd`), as the frame
    /// depth each runs in (`docs/spec/semantics.md` 6.2; `input.rs`). Words and `Char`s read in
    /// the frame of the innermost one stay within the line; a `Read` in a routine called from its
    /// indices does not.
    pub(super) readln: Vec<usize>,
}

impl Task {
    pub(super) fn new() -> Task {
        Task {
            regs: Vec::with_capacity(256),
            frames: Vec::with_capacity(16),
            pending: None,
            readln: Vec::new(),
        }
    }

    /// Whether code at frame depth `depth` runs inside a `ReadLn` statement of its own frame.
    pub(super) fn in_readln(&self, depth: usize) -> bool {
        self.readln.last() == Some(&depth)
    }

    /// An exception was caught by a handler at frame depth `depth` (`None`: not caught): each
    /// `ReadLn` statement it left (run at that depth or deeper) ends. Returns whether one did (the
    /// rest of the line is then dropped; `input.rs`).
    pub(super) fn readln_left(&mut self, depth: Option<usize>) -> bool {
        let keep = match depth {
            Some(d) => self.readln.iter().take_while(|f| **f < d).count(),
            None => 0,
        };
        if keep < self.readln.len() {
            self.readln.truncate(keep);
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Frame {
    pub(super) func: u32,
    pub(super) base: u32,
    pub(super) pc: u32,
    /// The caller's register to write the return value to (absolute position; [`Frame::ret`]),
    /// and in the top bit the mark of a frame whose `r0` holds a counted `Self` that leaving the
    /// frame lets go of ([`FRAME_COUNTED_SELF`]).
    pub(super) ret_reg: u32,
}

/// The mark in [`Frame::ret_reg`] of a frame `CallClosure` made for a method of a reference
/// counted class (`docs/spec/objects.md` 14.9): `r0` holds `Self`, counted. The bit is above any
/// register number (the stack limit keeps them far smaller).
pub(super) const FRAME_COUNTED_SELF: u32 = 1 << 31;

impl Frame {
    /// The caller's register the return value goes to.
    pub(super) fn ret(&self) -> u32 {
        self.ret_reg & !FRAME_COUNTED_SELF
    }

    /// Whether `r0` holds a counted `Self` ([`FRAME_COUNTED_SELF`]).
    pub(super) fn counted_self(&self) -> bool {
        self.ret_reg & FRAME_COUNTED_SELF != 0
    }
}

/// A pending call to a suspendable host function.
pub(super) struct PendingHost {
    pub(super) kind: PendingKind,
    /// The `pc` to go on at when done (the instruction after `CallHost`, or the input instruction
    /// itself, which runs again). On failure it is not advanced, so the report points at the instruction.
    pub(super) resume_pc: u32,
}

pub(super) enum PendingKind {
    Write(HostFuture<()>),
    /// A line for the input buffer (`ahead`: the line after the current one; `input.rs`).
    ReadLine {
        fut: HostFuture<Option<String>>,
        ahead: bool,
    },
}
