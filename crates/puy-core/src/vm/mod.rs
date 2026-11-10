//! Stackless VM (`docs/design/vm.md`, `docs/design/host-interface.md` 3.5, `docs/spec/bytecode.md` chapter 2).
//!
//! - Pascal calls do not recurse on the Rust stack. Frames and registers live on the heap.
//! - Execution budget (decremented by 1 on every backward jump and `Call`).
//! - Rust panics are not converted into Pascal exceptions (§5.10).

mod format;
pub mod heap;
mod input;
pub mod report;
mod strconv;
mod text;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::bytecode::{self, ConstEntry, Module, exc, op};
use crate::numfmt;
use crate::sema::types::MAX_STRING_LEN;
use heap::{BadRef, Heap, OutOfMemory, PStr, Piece, Trace};
use puy_host::{HostError, HostErrorKind, HostFuture, HostSet, OutStream};
pub use report::{ExceptionReport, PropValue, StackFrame, leaks_json, render_leaks};

/// A register / global variable slot (8 bytes).
pub type Slot = i64;

/// The message of `EOverflow` (`docs/spec/semantics.md` 7.1).
const CURRENCY_OVERFLOW: &str = "Currency overflow";

/// A `Double` slot holds the `f64` bits; a `Single` slot the `f32` bits (`docs/design/vm.md` 3.2).
fn slot_f64(v: Slot) -> f64 {
    f64::from_bits(v as u64)
}

fn f64_slot(v: f64) -> Slot {
    v.to_bits() as Slot
}

fn slot_f32(v: Slot) -> f32 {
    f32::from_bits(v as u32)
}

fn f32_slot(v: f32) -> Slot {
    v.to_bits() as Slot
}

/// The binary real arithmetic and comparisons `AddF64` .. `LeF32` (shape `A B C`; the negations
/// in the range are separate arms, since their `C` is not verified). IEEE 754; a `Single`
/// operation is done in binary32.
fn real_op(opc: u8, x: Slot, y: Slot) -> Slot {
    let (a, b) = (slot_f64(x), slot_f64(y));
    let (fa, fb) = (slot_f32(x), slot_f32(y));
    match opc {
        op::ADD_F64 => f64_slot(a + b),
        op::SUB_F64 => f64_slot(a - b),
        op::MUL_F64 => f64_slot(a * b),
        op::DIV_F64 => f64_slot(a / b),
        op::EQ_F64 => (a == b) as Slot,
        op::LT_F64 => (a < b) as Slot,
        op::LE_F64 => (a <= b) as Slot,
        op::ADD_F32 => f32_slot(fa + fb),
        op::SUB_F32 => f32_slot(fa - fb),
        op::MUL_F32 => f32_slot(fa * fb),
        op::DIV_F32 => f32_slot(fa / fb),
        op::EQ_F32 => (fa == fb) as Slot,
        op::LT_F32 => (fa < fb) as Slot,
        op::LE_F32 => (fa <= fb) as Slot,
        _ => unreachable!("not a real operation"),
    }
}

/// Standard output buffering mode (`docs/spec/semantics.md` 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdoutBuffering {
    Line,
    Full,
    None,
}

/// Hash seed (§8.9). M1 has no hash tables, so it is only used to check whether `Random` is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashSeed {
    FromHost,
    Fixed(u64),
}

#[derive(Debug, Clone)]
pub struct IsolateConfig {
    /// Memory limit per isolate (§5.12).
    pub memory_limit: Option<usize>,
    /// Recursion limit (total bytes of `regs` and `frames`; 256MB by default).
    pub stack_limit: usize,
    pub hash_seed: HashSeed,
    pub budget_default: u32,
    pub stdout_buffering: StdoutBuffering,
    /// Count the blocks left on the heap when the isolate finishes, and never reuse the indexes of freed
    /// blocks so that releasing too often is an internal error (`docs/design/lifetime.md` 3.12).
    pub leak_check: bool,
}

impl Default for IsolateConfig {
    fn default() -> Self {
        IsolateConfig {
            memory_limit: None,
            stack_limit: 256 * 1024 * 1024,
            hash_seed: HashSeed::FromHost,
            budget_default: 100_000,
            stdout_buffering: StdoutBuffering::Full,
            leak_check: false,
        }
    }
}

/// Failure of `Isolate::new` / `start_main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsolateError {
    /// A host function cannot be resolved (F0005).
    HostFunction(String),
    /// Neither a random source nor a seed (§8.9).
    NoRandom,
    /// `start_main` was called twice.
    AlreadyStarted,
}

impl std::fmt::Display for IsolateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsolateError::HostFunction(n) => {
                write!(f, "host function `{n}` is not available in this runtime")
            }
            IsolateError::NoRandom => write!(f, "no random source and no hash seed"),
            IsolateError::AlreadyStarted => write!(f, "isolate already started"),
        }
    }
}

/// A fatal error of the implementation (variants of F0001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmError {
    pub message: String,
}

/// How execution ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitKind {
    Code(u8),
    UncaughtException(Box<ExceptionReport>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitStatus {
    pub kind: ExitKind,
    /// The low 8 bits of `ExitCode` when the program ended (`docs/spec/semantics.md` 7.2; the same
    /// as `ExitKind::Code`).
    pub code: u8,
    /// Failure to flush the standard output buffer before exiting (`semantics.md` 7.2).
    pub stdout_error: Option<HostError>,
    /// Blocks left on the heap after the finishing stage, as (type name, count) in byte order of the name
    /// (`docs/spec/diagnostics.md` 8.1). Only with `leak_check`, and only in the status after [`Isolate::finish`].
    pub leaks: Vec<(String, u64)>,
    /// Exceptions that left a `finally` block while `Halt` was unwinding, in the order they
    /// occurred (`docs/spec/exceptions.md` 5.3, 7.1). The runtime reports them as warnings.
    pub halt_exceptions: Vec<ExceptionReport>,
    /// Uncaught exceptions of the units' `<fini>`, in the order they occurred
    /// (`docs/spec/units.md` 5.4). Only in the status after [`Isolate::finish`].
    pub fini_exceptions: Vec<ExceptionReport>,
}

impl ExitStatus {
    /// Whether the isolate failed (`docs/spec/units.md` 5.4): an uncaught exception (of
    /// `<main>`, `<init>` or `<fini>`) or a flush failure.
    pub fn failed(&self) -> bool {
        matches!(self.kind, ExitKind::UncaughtException(_))
            || self.stdout_error.is_some()
            || !self.fini_exceptions.is_empty()
    }

    /// Process exit code (§9.3): `ExitCode`, except that a failure turns 0 into 1
    /// (`docs/spec/semantics.md` 7.2).
    pub fn exit_code(&self) -> u8 {
        if self.failed() && self.code == 0 {
            1
        } else {
            self.code
        }
    }
}

#[derive(Debug)]
pub enum RunOutcome {
    Finished(ExitStatus),
    BudgetExhausted,
    Waiting,
    Fatal(VmError),
}

/// Host function kind (`docs/spec/bytecode.md` 2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostFn {
    ConsoleWrite,
    ConsoleWriteError,
    EnvGet,
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    func: u32,
    base: u32,
    pc: u32,
    /// The caller's register to write the return value to (absolute position).
    ret_reg: u32,
}

/// A runtime error the VM detects (`semantics.md` 7.1): the class (a built-in type index), the
/// message and the class's property. It becomes an exception object when raised
/// (`docs/spec/exceptions.md` 5.5).
struct RtError {
    class: u32,
    message: String,
    extra: Extra,
}

/// The value of the slot a built-in class adds (`docs/spec/exceptions.md` 2).
enum Extra {
    None,
    /// `EConvertError.Value`.
    Str(String),
    /// `EInOutError.ErrorCode`.
    Int(i64),
}

fn rt(class: u32, message: impl Into<String>) -> RtError {
    RtError {
        class,
        message: message.into(),
        extra: Extra::None,
    }
}

/// A pending call to a suspendable host function.
struct PendingHost {
    kind: PendingKind,
    /// The `pc` to go on at when done (the instruction after `CallHost`, or the input instruction
    /// itself, which runs again). On failure it is not advanced, so the report points at the instruction.
    resume_pc: u32,
}

enum PendingKind {
    Write(HostFuture<()>),
    /// A line for the input buffer (`ahead`: the line after the current one; `input.rs`).
    ReadLine {
        fut: HostFuture<Option<String>>,
        ahead: bool,
    },
}

enum Phase {
    NotStarted,
    /// Running the `<init>` of the unit with this index (`docs/spec/units.md` 5.3).
    Init(usize),
    Main,
    /// The program has finished (the first `Finished`).
    Done,
    /// [`Isolate::finish`] was called; the next `run` starts the finishing stage.
    Finishing,
    /// Running the `<fini>` of the unit with this index (`docs/spec/units.md` 5.4).
    Fini(usize),
    /// The finishing stage is over (the final `Finished`).
    Ended,
}

pub struct Isolate {
    module: Arc<Module>,
    host: HostSet,
    config: IsolateConfig,
    heap: Heap,
    globals: Vec<Slot>,
    regs: Vec<Slot>,
    frames: Vec<Frame>,
    args: Vec<String>,
    const_cache: Vec<Option<Slot>>,
    /// Whether each constant is a managed value (`LoadConst` counts it).
    managed_consts: Vec<bool>,
    host_table: Vec<HostFn>,
    stdout_buf: Vec<u8>,
    pending: Option<PendingHost>,
    phase: Phase,
    finished: Option<ExitStatus>,
    /// `ExitCode` when the module has no `ExitCode` global (`docs/spec/semantics.md` 5.3, 7.2).
    exit_code: i64,
    /// The `ExitCode` global (`bytecode::GLOBAL_EXIT_CODE`), if the program uses it.
    exit_code_global: Option<usize>,
    /// See [`ExitStatus::halt_exceptions`].
    halt_exceptions: Vec<ExceptionReport>,
    /// The input line buffer (`docs/spec/semantics.md` 6.2).
    input: input::InputBuf,
    /// The number of units, in initialization order, whose `<init>` has run to its end
    /// (`docs/spec/units.md` 5.4: only these run their `<fini>`).
    inited: usize,
    /// The finishing stage goes on with the unit before this index (units in reverse order).
    fini_next: usize,
    /// See [`ExitStatus::fini_exceptions`].
    fini_exceptions: Vec<ExceptionReport>,
}

const FULL_BUFFER: usize = 8192;

fn noop_waker() -> Waker {
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    fn noop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    // SAFETY: a Waker that does nothing
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
}

/// Ends the step with an internal error when a reference operation hits a bad reference (F0001).
macro_rules! try_ref {
    ($e:expr) => {
        if let Err(e) = $e {
            return Step::Fatal(e.message);
        }
    };
}

/// Reads the bytes of a `UTF8String` a reference points to, or ends the step with an internal
/// error (a freed block; F0001).
macro_rules! get_u8 {
    ($heap:expr, $r:expr) => {
        match $heap.get_u8($r) {
            Ok(s) => s,
            Err(e) => return Step::Fatal(e.message),
        }
    };
}

/// The 0-based half-open range `Copy(S, Index, Count)` takes from a sequence of `len` units
/// (`docs/spec/semantics.md` 5.1; the bytes of a `UTF8String`), or `None` for an empty result.
/// `Index < 1` counts as 1, and the end is computed in 64 bits, so it cannot overflow.
fn copy_range(index: i64, count: i64, len: usize) -> Option<(usize, usize)> {
    let index = index.max(1);
    let len = len as i64;
    if index > len || count <= 0 {
        return None;
    }
    Some(((index - 1) as usize, (index - 1 + count).min(len) as usize))
}

/// The part `X[low..high]` of a sequence of `len` items whose first index is `first`, cut to the
/// sequence (`docs/spec/types.md` 8.7): the 0-based start and the count (0 when empty).
fn slice_range(len: i64, low: i64, high: i64, first: i64) -> (i64, i64) {
    let len = i128::from(len);
    let start = (i128::from(low) - i128::from(first)).clamp(0, len);
    let end = (i128::from(high) - i128::from(first) + 1).clamp(0, len);
    // Both are within 0..=len, which fits an `i64`
    (start as i64, (end - start).max(0) as i64)
}

/// The 0-based position `Insert` of a string puts the source at (`docs/spec/semantics.md` 5.3):
/// an `index` below 1 is the start, one past the end is the end.
fn insert_pos(index: i64, len: usize) -> usize {
    (index.clamp(1, len as i64 + 1) - 1) as usize
}

/// The 0-based range `Delete` of a string removes (`docs/spec/semantics.md` 5.3), or `None` when it
/// does nothing (`index` outside `1..len`, `count <= 0`).
fn delete_range(index: i64, count: i64, len: usize) -> Option<(usize, usize)> {
    if index < 1 || index > len as i64 || count <= 0 {
        return None;
    }
    let start = index as usize - 1;
    Some((
        start,
        (start as i64).saturating_add(count).min(len as i64) as usize,
    ))
}

/// `EEncodingError` when a piece of a `UTF8String` would start at the 0-based byte `i` inside a
/// multibyte character (`docs/spec/types.md` 8.5). The message names the 1-based byte.
fn check_u8_boundary(b: &[u8], i: usize) -> Result<(), RtError> {
    if b.get(i).is_some_and(|x| x & 0xC0 == 0x80) {
        Err(rt(
            exc::ENCODING_ERROR,
            format!(
                "UTF8String cut inside a multibyte character at byte {}",
                i + 1
            ),
        ))
    } else {
        Ok(())
    }
}

/// Reads the string a reference points to, or ends the step with an internal error (a freed block; F0001).
macro_rules! get_str {
    ($heap:expr, $r:expr) => {
        match $heap.get($r) {
            Ok(s) => s,
            Err(e) => return Step::Fatal(e.message),
        }
    };
}

fn width_type(c: u32) -> (&'static str, i128, i128) {
    match c {
        0 => ("ShortInt", i8::MIN as i128, i8::MAX as i128),
        1 => ("Byte", 0, u8::MAX as i128),
        2 => ("SmallInt", i16::MIN as i128, i16::MAX as i128),
        3 => ("Word", 0, u16::MAX as i128),
        4 => ("Integer", i32::MIN as i128, i32::MAX as i128),
        5 => ("Cardinal", 0, u32::MAX as i128),
        6 => ("Int64", i64::MIN as i128, i64::MAX as i128),
        7 => ("UInt64", 0, u64::MAX as i128),
        _ => ("Char", 0, u16::MAX as i128),
    }
}

impl Isolate {
    pub fn new(
        module: Arc<Module>,
        host: HostSet,
        config: IsolateConfig,
    ) -> Result<Isolate, IsolateError> {
        if config.hash_seed == HashSeed::FromHost && host.random.is_none() {
            return Err(IsolateError::NoRandom);
        }
        let mut host_table = Vec::new();
        for h in &module.hosts {
            let kind = match h.name.as_str() {
                "Console.Write" => HostFn::ConsoleWrite,
                "Console.WriteError" => HostFn::ConsoleWriteError,
                "Env.Get" => HostFn::EnvGet,
                _ => return Err(IsolateError::HostFunction(h.name.clone())),
            };
            host_table.push(kind);
        }
        let globals = vec![0; module.globals.len()];
        let const_cache = vec![None; module.consts.len()];
        let managed_consts = module.consts.iter().map(is_managed_const).collect();
        Ok(Isolate {
            heap: Heap::new(config.memory_limit, !config.leak_check, &module.types),
            globals,
            regs: Vec::with_capacity(256),
            frames: Vec::with_capacity(16),
            args: Vec::new(),
            const_cache,
            managed_consts,
            host_table,
            stdout_buf: Vec::new(),
            pending: None,
            phase: Phase::NotStarted,
            finished: None,
            exit_code: 0,
            exit_code_global: module
                .globals
                .iter()
                .position(|g| g.kind == bytecode::GLOBAL_EXIT_CODE),
            halt_exceptions: Vec::new(),
            input: input::InputBuf::default(),
            inited: 0,
            fini_next: 0,
            fini_exceptions: Vec::new(),
            module,
            host,
            config,
        })
    }

    /// Starts the program. `args[0]` is `ParamStr(0)`.
    pub fn start_main(&mut self, args: Vec<String>) -> Result<(), IsolateError> {
        if !matches!(self.phase, Phase::NotStarted) {
            return Err(IsolateError::AlreadyStarted);
        }
        self.args = args;
        self.start_init_from(0);
        Ok(())
    }

    /// Starts the finishing stage (`docs/spec/exceptions.md` 7.2, `docs/spec/units.md` 5.4): the following
    /// `run` calls run the `<fini>` of the units that finished their `<init>`, in reverse initialization order,
    /// releasing each unit's global variables after its `<fini>`, then release the constant cache, count leaks
    /// (`leak_check`), flush standard output, and return the final `Finished`. Only takes effect after the first
    /// `Finished`; otherwise it does nothing.
    pub fn finish(&mut self) {
        if matches!(self.phase, Phase::Done) {
            self.phase = Phase::Finishing;
        }
    }

    pub fn finished(&self) -> Option<&ExitStatus> {
        self.finished.as_ref()
    }

    /// Heap usage (bytes).
    pub fn memory_used(&self) -> usize {
        self.heap.used()
    }

    fn push_frame(&mut self, func: u32, base: u32, ret_reg: u32) {
        let regs = self.module.funcs[func as usize].regs as usize;
        let need = base as usize + regs;
        if self.regs.len() < need {
            self.regs.resize(need, 0);
        }
        // A frame starts zeroed (`<main>` reuses the registers `<init>` left behind)
        self.regs[base as usize..need].fill(0);
        self.frames.push(Frame {
            func,
            base,
            pc: 0,
            ret_reg,
        });
    }

    /// Runs runnable tasks within the budget.
    pub fn run(&mut self, budget: u32) -> RunOutcome {
        if matches!(self.phase, Phase::Finishing) {
            self.fini_next = self.module.units.len();
            if let Some(o) = self.advance_finishing() {
                return o;
            }
        }
        if matches!(self.phase, Phase::Done | Phase::Ended)
            && let Some(st) = &self.finished
        {
            return RunOutcome::Finished(st.clone());
        }
        if matches!(self.phase, Phase::NotStarted) {
            return RunOutcome::Fatal(VmError {
                message: "run before start_main".into(),
            });
        }
        // Pending Future
        if self.pending.is_some() {
            match self.poll_pending() {
                Ok(true) => {}
                Ok(false) => return RunOutcome::Waiting,
                Err(PollError::Rt(e)) => {
                    if let Some(o) = self.raise(e) {
                        return o;
                    }
                }
                Err(PollError::Fatal(e)) => {
                    self.flush_stdout().ok();
                    return RunOutcome::Fatal(VmError { message: e.message });
                }
            }
        }
        let mut budget = budget.max(1);
        loop {
            match self.run_loop(&mut budget) {
                Step::Continue => {
                    if budget == 0 {
                        return RunOutcome::BudgetExhausted;
                    }
                }
                Step::Waiting => return RunOutcome::Waiting,
                Step::Finished(kind) => {
                    // `Halt` leaves frames behind; their managed registers are released first
                    if let Err(e) = self.release_frames() {
                        return RunOutcome::Fatal(VmError { message: e.message });
                    }
                    if let Some(o) = self.frames_done(kind) {
                        return o;
                    }
                    if budget == 0 {
                        return RunOutcome::BudgetExhausted;
                    }
                }
                Step::Error(e) => {
                    if let Some(o) = self.raise(e) {
                        return o;
                    }
                }
                Step::Throw(x) => {
                    if let Some(o) = self.throw(x) {
                        return o;
                    }
                }
                Step::Halt(from) => {
                    if let Some(o) = self.halt(from) {
                        return o;
                    }
                }
                Step::Fatal(m) => {
                    self.flush_stdout().ok();
                    return RunOutcome::Fatal(VmError { message: m });
                }
            }
        }
    }

    /// `ExitCode` (`docs/spec/semantics.md` 5.3): the global if the program uses it.
    fn exit_code_value(&self) -> i64 {
        match self.exit_code_global {
            Some(g) => self.globals[g],
            None => self.exit_code,
        }
    }

    fn set_exit_code(&mut self, v: i64) {
        match self.exit_code_global {
            Some(g) => self.globals[g] = v,
            None => self.exit_code = v,
        }
    }

    /// The low 8 bits of `ExitCode` (§9.3).
    fn exit_code_byte(&self) -> u8 {
        (self.exit_code_value() & 0xFF) as u8
    }

    /// The outermost frame has ended (`kind` says how). While the program runs, the program has
    /// finished; in the finishing stage, a unit's `<fini>` has: its uncaught exception is recorded,
    /// and the stage goes on with the next unit (`docs/spec/units.md` 5.4). `None` when another
    /// `<fini>` was started.
    fn frames_done(&mut self, kind: ExitKind) -> Option<RunOutcome> {
        let Phase::Fini(unit) = self.phase else {
            return Some(self.end_program(kind));
        };
        if let ExitKind::UncaughtException(report) = kind {
            self.fini_exceptions.push(*report);
        }
        if let Err(e) = self.release_unit_globals(unit) {
            return Some(self.fatal(e));
        }
        self.fini_next = unit;
        self.advance_finishing()
    }

    /// The program has finished (the first `Finished`). The finishing stage waits for [`Isolate::finish`].
    fn end_program(&mut self, kind: ExitKind) -> RunOutcome {
        let stdout_error = self.flush_stdout().err();
        self.phase = Phase::Done;
        let st = ExitStatus {
            kind,
            code: self.exit_code_byte(),
            stdout_error,
            leaks: Vec::new(),
            halt_exceptions: std::mem::take(&mut self.halt_exceptions),
            fini_exceptions: Vec::new(),
        };
        self.finished = Some(st.clone());
        RunOutcome::Finished(st)
    }

    /// Goes on with the finishing stage (`docs/spec/units.md` 5.4) from the unit before
    /// `fini_next`, in reverse initialization order: starts the `<fini>` of a unit that finished its
    /// `<init>` (`None`; the unit's global variables are released when it ends), and releases the
    /// global variables of a unit without one. After the first unit, completes the stage.
    fn advance_finishing(&mut self) -> Option<RunOutcome> {
        while let Some(unit) = self.fini_next.checked_sub(1) {
            if unit < self.inited
                && let Some(fini) = self.module.units[unit].fini
            {
                self.phase = Phase::Fini(unit);
                self.push_frame(fini, 0, 0);
                return None;
            }
            if let Err(e) = self.release_unit_globals(unit) {
                return Some(self.fatal(e));
            }
            self.fini_next = unit;
        }
        Some(self.complete_finishing())
    }

    /// Releases the managed global variables of a unit in reverse declaration order
    /// (`docs/design/lifetime.md` 3.8, `docs/spec/units.md` 5.4).
    fn release_unit_globals(&mut self, unit: usize) -> Result<(), BadRef> {
        let module = Arc::clone(&self.module);
        let u = &module.units[unit];
        let first = u.first_global as usize;
        for i in (first..first + u.global_count as usize).rev() {
            if module.globals[i].kind == bytecode::GLOBAL_REF {
                let r = std::mem::take(&mut self.globals[i]);
                self.heap.release(r)?;
            }
        }
        Ok(())
    }

    /// The end of the finishing stage: releases the constant cache, counts the blocks left on the
    /// heap, and flushes standard output. The exit code is `ExitCode` now, and 1 instead of 0 when
    /// the program or a `<fini>` failed (`docs/spec/units.md` 5.4).
    fn complete_finishing(&mut self) -> RunOutcome {
        let program_failed = self
            .finished
            .as_ref()
            .expect("the finishing stage starts after the first Finished")
            .failed();
        let module = Arc::clone(&self.module);
        for (k, c) in module.consts.iter().enumerate() {
            if let (true, Some(r)) = (is_managed_const(c), self.const_cache[k].take())
                && let Err(e) = self.heap.release(r)
            {
                return RunOutcome::Fatal(VmError { message: e.message });
            }
        }
        let leaks = if self.config.leak_check {
            count_leaks(self.heap.live_type_names())
        } else {
            Vec::new()
        };
        let stdout_error = self.flush_stdout().err();
        self.phase = Phase::Ended;
        let mut st = ExitStatus {
            kind: ExitKind::Code(0),
            code: self.exit_code_byte(),
            stdout_error,
            leaks,
            halt_exceptions: std::mem::take(&mut self.halt_exceptions),
            fini_exceptions: std::mem::take(&mut self.fini_exceptions),
        };
        // The program's failure carries over; the stage's own failures are `ExitStatus::failed`
        if program_failed && st.code == 0 {
            st.code = 1;
        }
        st.code = st.exit_code();
        st.kind = ExitKind::Code(st.code);
        self.finished = Some(st.clone());
        RunOutcome::Finished(st)
    }

    /// A runtime error the VM detected: raises it as an exception object (`docs/spec/exceptions.md` 5.5).
    /// `None` when a handler caught it.
    fn raise(&mut self, e: RtError) -> Option<RunOutcome> {
        match self.exception_of(e) {
            Ok(x) => self.throw(x),
            Err(b) => Some(self.fatal(b)),
        }
    }

    fn fatal(&mut self, e: BadRef) -> RunOutcome {
        self.flush_stdout().ok();
        RunOutcome::Fatal(VmError { message: e.message })
    }

    /// The exception object of a runtime error. When the object cannot be made within the usage
    /// limit, an `EOutOfMemory` takes its place; `EOutOfMemory` and `EStackOverflow` are made without
    /// the limit (`docs/spec/exceptions.md` 5.5).
    fn exception_of(&mut self, e: RtError) -> Result<Slot, BadRef> {
        let force = matches!(e.class, exc::OUT_OF_MEMORY | exc::STACK_OVERFLOW);
        if !force && let Ok(x) = self.try_exception_of(&e) {
            return Ok(x);
        }
        if force {
            let msg = self.heap.alloc_str_force(PStr::from_text(&e.message));
            let x = self.heap.alloc_exc(e.class, msg, true).expect("forced");
            return Ok(x);
        }
        let msg = self.heap.alloc_str_force(PStr::from_text("out of memory"));
        Ok(self
            .heap
            .alloc_exc(exc::OUT_OF_MEMORY, msg, true)
            .expect("forced allocation does not fail"))
    }

    fn try_exception_of(&mut self, e: &RtError) -> Result<Slot, OutOfMemory> {
        let msg = self.heap.alloc_str(PStr::from_text(&e.message))?;
        let x = match self.heap.alloc_exc(e.class, msg, false) {
            Ok(x) => x,
            Err(o) => {
                self.heap.release(msg).expect("a new string");
                return Err(o);
            }
        };
        let extra = match &e.extra {
            Extra::None => return Ok(x),
            Extra::Int(n) => *n,
            Extra::Str(t) => match self.heap.alloc_str(PStr::from_text(t)) {
                Ok(v) => v,
                Err(o) => {
                    self.heap.release(x).expect("a new exception");
                    return Err(o);
                }
            },
        };
        self.heap
            .exc_replace(x, bytecode::EXC_SLOT_EXTRA, extra)
            .expect("a new exception has the slot of its class");
        Ok(x)
    }

    /// Raises an exception object, taking its reference (`Raise`; `docs/spec/exceptions.md` 6.2,
    /// 6.3): records the stack trace if the object has none, then unwinds. `None` when a handler
    /// caught it (execution goes on there); otherwise the program ends with the uncaught exception
    /// (7.1).
    fn throw(&mut self, x: Slot) -> Option<RunOutcome> {
        let unwound = self.unwind(x);
        // A `ReadLn` the exception left drops the rest of its line (`docs/spec/semantics.md` 6.2)
        let caught_at = matches!(unwound, Ok(Unwound::Caught)).then_some(self.frames.len());
        self.input.exception_left(caught_at);
        match unwound {
            Ok(Unwound::Caught) => None,
            Ok(Unwound::Halt(from)) => self.halt(Some(from)),
            Ok(Unwound::Uncaught(report)) => {
                self.frames_done(ExitKind::UncaughtException(Box::new(report)))
            }
            Err(b) => Some(self.fatal(b)),
        }
    }

    /// The position to look up in the handler table for the last frame while unwinding: the
    /// current instruction if it is the frame the unwinding started in (`innermost`), otherwise
    /// the `Call` (one word before the return address) of a caller whose callee was popped.
    fn unwind_pc(&self, innermost: bool) -> u32 {
        let fr = self.frames.last().expect("a frame being unwound");
        if innermost {
            fr.pc
        } else {
            fr.pc.saturating_sub(1)
        }
    }

    /// The unwinding of an exception (`docs/spec/exceptions.md` 6.3).
    fn unwind(&mut self, x: Slot) -> Result<Unwound, BadRef> {
        self.record_trace(x)?;
        let module = Arc::clone(&self.module);
        let mut innermost = true;
        while let Some(&fr) = self.frames.last() {
            let pc = self.unwind_pc(innermost);
            innermost = false;
            let base = fr.base as usize;
            for h in &module.funcs[fr.func as usize].handlers {
                let r = base + h.reg as usize;
                if h.kind == bytecode::HANDLER_FINALLY && h.in_handler(pc) {
                    let state = std::mem::take(&mut self.regs[r]);
                    if state == FINALLY_HALT {
                        // An exception leaving a `finally` block during `Halt`: report it and go on
                        // with `Halt` after the block (5.3)
                        let report = self.build_report(x, 0)?;
                        self.halt_exceptions.push(report);
                        self.heap.release(x)?;
                        return Ok(Unwound::Halt(h.handler_end));
                    }
                    if state == FINALLY_EXCEPTION {
                        let y = std::mem::take(&mut self.regs[r + 1]);
                        self.attach_suppressed(x, y)?;
                    }
                }
                if h.kind == bytecode::HANDLER_EXCEPT && h.in_handler(pc) {
                    // Leaving an `except` block (5.2)
                    let old = std::mem::take(&mut self.regs[r]);
                    self.heap.release(old)?;
                }
                if h.in_body(pc) {
                    if h.kind == bytecode::HANDLER_EXCEPT {
                        self.put(r, x)?;
                    } else {
                        self.regs[r] = FINALLY_EXCEPTION;
                        self.put(r + 1, x)?;
                    }
                    self.frames.last_mut().expect("the frame being unwound").pc = h.handler_start;
                    return Ok(Unwound::Caught);
                }
            }
            self.release_managed(fr.func, fr.base, |_| true)?;
            self.frames.pop();
        }
        let report = self.build_report(x, 0)?;
        self.heap.release(x)?;
        Ok(Unwound::Uncaught(report))
    }

    /// Attaches `y` (whose reference it takes) to `Suppressed` of `x`, unless that makes a cycle
    /// (`docs/spec/exceptions.md` 5.4; then `y` is let go).
    fn attach_suppressed(&mut self, x: Slot, y: Slot) -> Result<(), BadRef> {
        if y == 0 {
            return Ok(());
        }
        if self.heap.exc_reaches(y, x)? {
            return self.heap.release(y);
        }
        self.heap.exc_push_suppressed(x, y)
    }

    /// The unwinding of `Halt` (`docs/spec/exceptions.md` 5.3, 6.3): runs the `finally` blocks from
    /// the innermost, skipping `except`. `from` is the position to resume from in the innermost
    /// frame (after a `finally` block), or `None` at the `Halt` instruction. `None` when a
    /// `finally` block runs (execution goes on there).
    fn halt(&mut self, mut from: Option<u32>) -> Option<RunOutcome> {
        let module = Arc::clone(&self.module);
        let result: Result<bool, BadRef> = (|| {
            let mut innermost = true;
            while let Some(&fr) = self.frames.last() {
                let pc = from.take().unwrap_or_else(|| self.unwind_pc(innermost));
                innermost = false;
                let base = fr.base as usize;
                for h in &module.funcs[fr.func as usize].handlers {
                    let r = base + h.reg as usize;
                    if h.kind == bytecode::HANDLER_FINALLY && h.in_handler(pc) {
                        let state = std::mem::take(&mut self.regs[r]);
                        if state == FINALLY_EXCEPTION {
                            // The exception being propagated is dropped (not reported)
                            let y = std::mem::take(&mut self.regs[r + 1]);
                            self.heap.release(y)?;
                        }
                    }
                    if h.kind == bytecode::HANDLER_EXCEPT && h.in_handler(pc) {
                        let old = std::mem::take(&mut self.regs[r]);
                        self.heap.release(old)?;
                    }
                    if h.kind == bytecode::HANDLER_FINALLY && h.in_body(pc) {
                        self.regs[r] = FINALLY_HALT;
                        let old = std::mem::take(&mut self.regs[r + 1]);
                        self.heap.release(old)?;
                        self.frames.last_mut().expect("the frame being unwound").pc =
                            h.handler_start;
                        return Ok(true);
                    }
                }
                self.release_managed(fr.func, fr.base, |_| true)?;
                self.frames.pop();
            }
            Ok(false)
        })();
        match result {
            Ok(true) => None,
            Ok(false) => self.frames_done(ExitKind::Code(self.exit_code_byte())),
            Err(b) => Some(self.fatal(b)),
        }
    }

    /// Records the current frames as the stack trace of an exception that does not have one yet
    /// (`docs/spec/exceptions.md` 5.1).
    fn record_trace(&mut self, x: Slot) -> Result<(), BadRef> {
        if self.heap.exc_trace(x)?.is_some() {
            return Ok(());
        }
        let n = self.frames.len();
        let pos = |i: usize, fr: &Frame| -> (u32, u32) {
            // The innermost is the raising instruction; callers are their Call (one word before the return address)
            if i == n - 1 {
                (fr.func, fr.pc)
            } else {
                (fr.func, fr.pc.saturating_sub(1))
            }
        };
        let all = self.frames.iter().enumerate().rev();
        let (frames, omitted) = if n > 32 {
            let mut v: Vec<(u32, u32)> = all.clone().take(16).map(|(i, f)| pos(i, f)).collect();
            v.extend(
                self.frames[..16]
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(i, f)| pos(i, f)),
            );
            (v, n - 32)
        } else {
            (all.map(|(i, f)| pos(i, f)).collect(), 0)
        };
        self.heap.set_exc_trace(x, Trace { frames, omitted })
    }

    /// The stack trace of an exception as report frames.
    fn trace_frames(&self, x: Slot) -> Result<(Vec<StackFrame>, usize), BadRef> {
        let Some(t) = self.heap.exc_trace(x)? else {
            return Ok((Vec::new(), 0));
        };
        let frames = t
            .frames
            .iter()
            .map(|&(func, pc)| {
                let f = &self.module.funcs[func as usize];
                StackFrame {
                    function: f.name.clone(),
                    file: self
                        .module
                        .units
                        .get(f.file as usize)
                        .map(|u| u.path.clone())
                        .unwrap_or_default(),
                    line: f.line_at(pc),
                }
            })
            .collect();
        Ok((frames, t.omitted))
    }

    /// The `StackTrace` string: the `  at …` lines joined by `#10` (`docs/spec/exceptions.md` 2.1).
    fn stack_trace_text(&self, x: Slot) -> Result<String, BadRef> {
        let (frames, omitted) = self.trace_frames(x)?;
        Ok(report::trace_lines(&frames, omitted).join("\n"))
    }

    /// The class name of an exception: the declared name, without the qualification
    /// (`docs/spec/exceptions.md` 2.1).
    fn class_name(&self, x: Slot) -> Result<String, BadRef> {
        let name = self.heap.type_name(self.heap.exc_type(x)?);
        Ok(name.rsplit('.').next().unwrap_or(name).to_string())
    }

    /// The report of an exception and the exceptions attached to it (`docs/spec/exceptions.md` 7.1).
    /// `depth` is the nesting level (0 for the uncaught exception); deeper than
    /// [`report::MAX_NESTING`] levels are left out.
    fn build_report(&self, x: Slot, depth: usize) -> Result<ExceptionReport, BadRef> {
        let ty = self.heap.exc_type(x)?;
        let message = self
            .heap
            .get(
                self.heap
                    .exc_slot(x, bytecode::EXC_SLOT_MESSAGE)?
                    .unwrap_or(0),
            )?
            .to_utf8();
        let (stack, omitted) = self.trace_frames(x)?;
        let mut properties = Vec::new();
        let extra = self
            .heap
            .exc_slot(x, bytecode::EXC_SLOT_EXTRA)?
            .unwrap_or(0);
        if self.heap.class_is(ty, exc::CONVERT_ERROR) {
            properties.push((
                "Value".to_string(),
                PropValue::Str(self.heap.get(extra)?.to_utf8()),
            ));
        } else if self.heap.class_is(ty, exc::IN_OUT_ERROR) {
            properties.push(("ErrorCode".to_string(), PropValue::Int(extra)));
        }
        let mut report = ExceptionReport {
            class: self.class_name(x)?,
            message,
            stack,
            omitted,
            properties,
            suppressed: Vec::new(),
            inner: None,
            more_nested: false,
        };
        let suppressed = self.heap.exc_suppressed(x)?.to_vec();
        let inner = self
            .heap
            .exc_slot(x, bytecode::EXC_SLOT_INNER)?
            .unwrap_or(0);
        if depth >= report::MAX_NESTING {
            report.more_nested = !suppressed.is_empty() || inner != 0;
            return Ok(report);
        }
        for s in suppressed {
            report.suppressed.push(self.build_report(s, depth + 1)?);
        }
        if inner != 0 {
            report.inner = Some(Box::new(self.build_report(inner, depth + 1)?));
        }
        Ok(report)
    }

    /// Discards the frames from the innermost, releasing the managed registers of each
    /// (the implicit `finally`; `docs/design/lifetime.md` 3.8).
    fn release_frames(&mut self) -> Result<(), BadRef> {
        while let Some(fr) = self.frames.pop() {
            self.release_managed(fr.func, fr.base, |_| true)?;
        }
        Ok(())
    }

    /// Releases the managed registers of a function's frame at `base`, in list order, that `which`
    /// selects (by register number), and sets them to 0.
    fn release_managed(
        &mut self,
        func: u32,
        base: u32,
        which: impl Fn(u16) -> bool,
    ) -> Result<(), BadRef> {
        let module = Arc::clone(&self.module);
        for &r in &module.funcs[func as usize].managed {
            if which(r) {
                let old = std::mem::take(&mut self.regs[base as usize + r as usize]);
                self.heap.release(old)?;
            }
        }
        Ok(())
    }

    /// Writes a value that holds its own reference into a register, letting go of the register's old value
    /// (every instruction that makes a managed value; `docs/design/lifetime.md` 3.1).
    fn put(&mut self, i: usize, v: Slot) -> Result<(), BadRef> {
        let old = std::mem::replace(&mut self.regs[i], v);
        self.heap.release(old)
    }

    /// Writes an existing value into a register: counts it, then lets go of the old value.
    fn put_shared(&mut self, i: usize, v: Slot) -> Result<(), BadRef> {
        self.heap.add_ref(v)?;
        self.put(i, v)
    }

    // ---- Standard output buffer ----

    fn write_stdout(&mut self, bytes: &[u8]) -> Result<(), HostError> {
        match self.config.stdout_buffering {
            StdoutBuffering::None => {
                self.stdout_buf.extend_from_slice(bytes);
                self.flush_stdout()
            }
            StdoutBuffering::Line => {
                self.stdout_buf.extend_from_slice(bytes);
                if bytes.contains(&b'\n') {
                    self.flush_stdout()
                } else {
                    Ok(())
                }
            }
            StdoutBuffering::Full => {
                self.stdout_buf.extend_from_slice(bytes);
                if self.stdout_buf.len() >= FULL_BUFFER {
                    self.flush_stdout()
                } else {
                    Ok(())
                }
            }
        }
    }

    fn flush_stdout(&mut self) -> Result<(), HostError> {
        if self.stdout_buf.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.stdout_buf);
        let Some(console) = self.host.console.as_mut() else {
            return Ok(()); // discard when there is no Console (§9.5)
        };
        let mut fut = console.write(OutStream::Stdout, &data);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(r) => return r,
                Poll::Pending => {
                    // M1 writes complete immediately. For an implementation that does not complete, wait until everything is written
                    continue;
                }
            }
        }
    }

    // ---- Host functions ----

    fn poll_pending(&mut self) -> Result<bool, PollError> {
        let Some(p) = self.pending.as_mut() else {
            return Ok(true);
        };
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let result: Result<Option<(Option<String>, bool)>, HostError> = match &mut p.kind {
            PendingKind::Write(f) => match Pin::new(f).poll(&mut cx) {
                Poll::Pending => return Ok(false),
                Poll::Ready(Ok(())) => Ok(None),
                Poll::Ready(Err(e)) => Err(e),
            },
            PendingKind::ReadLine { fut, ahead } => match Pin::new(fut).poll(&mut cx) {
                Poll::Pending => return Ok(false),
                Poll::Ready(Ok(line)) => Ok(Some((line, *ahead))),
                Poll::Ready(Err(e)) => Err(e),
            },
        };
        let resume_pc = p.resume_pc;
        self.pending = None;
        match result {
            Ok(None) => {}
            // The line goes into the buffer; the input instruction runs again (`input.rs`)
            Ok(Some((line, ahead))) => self.input.receive(line, ahead)?,
            Err(e) => return Err(PollError::Rt(host_error_to_rt(e))),
        }
        if let Some(fr) = self.frames.last_mut() {
            fr.pc = resume_pc;
        }
        Ok(true)
    }

    fn alloc_string(&mut self, s: PStr) -> Result<Slot, RtError> {
        self.heap
            .alloc_str(s)
            .map_err(|_| rt(exc::OUT_OF_MEMORY, "out of memory"))
    }

    fn alloc_u8(&mut self, b: Vec<u8>) -> Result<Slot, RtError> {
        self.heap
            .alloc_u8(b)
            .map_err(|_| rt(exc::OUT_OF_MEMORY, "out of memory"))
    }

    /// The number of bits of a big set block's type.
    fn set_bits(&self, set: Slot) -> Result<i64, String> {
        let ty = self.heap.agg_type(set).map_err(|e| e.message)?;
        Ok(self.module.type_entry(ty).map_or(0, |t| t.slots as i64))
    }

    /// The value of a constant, made on first use and kept in the constant cache, which holds one
    /// reference of a managed value (`docs/design/vm.md` 3.11).
    fn load_const(&mut self, k: usize) -> Result<Slot, PollError> {
        if let Some(v) = self.const_cache[k] {
            return Ok(v);
        }
        let module = Arc::clone(&self.module);
        let v = match &module.consts[k] {
            ConstEntry::Int(i) => *i,
            ConstEntry::Float(b) => *b as i64,
            ConstEntry::Str(s) => {
                let p = PStr::from_text(s);
                self.alloc_string(p)?
            }
            ConstEntry::Utf16(u) => {
                let p = PStr::from_units(u);
                self.alloc_string(p)?
            }
            ConstEntry::U8(s) => self.alloc_u8(s.as_bytes().to_vec())?,
            ConstEntry::Agg { ty, items } => {
                // The elements come before the aggregate (`docs/spec/bytecode.md` 3.3); the block
                // holds a reference of each managed element besides the cache's
                let mut slots = Vec::with_capacity(items.len());
                for &item in items {
                    let v = self.load_const(item as usize)?;
                    if self.managed_consts[item as usize] {
                        self.heap.add_ref(v).map_err(PollError::Fatal)?;
                    }
                    slots.push(v);
                }
                match self.heap.alloc_agg(*ty, slots.clone()) {
                    Ok(r) => r,
                    Err(_) => {
                        for (i, &item) in items.iter().enumerate() {
                            if self.managed_consts[item as usize] {
                                self.heap.release(slots[i]).map_err(PollError::Fatal)?;
                            }
                        }
                        return Err(rt(exc::OUT_OF_MEMORY, "out of memory").into());
                    }
                }
            }
            ConstEntry::Set { ty, words } => match self.heap.alloc_set(*ty, words) {
                Ok(Ok(r)) => r,
                Ok(Err(_)) => return Err(rt(exc::OUT_OF_MEMORY, "out of memory").into()),
                Err(e) => return Err(PollError::Fatal(e)),
            },
        };
        self.const_cache[k] = Some(v);
        Ok(v)
    }

    // ---- Execution loop ----

    /// Fast path of the execution loop. Common instructions (register moves, integer arithmetic and comparisons, branches, globals,
    /// constant loads) keep looping inside this function without writing back the frame's `pc`. Other instructions
    /// (strings, calls and returns, host functions, wide forms) write back `pc` and are handed to `step`;
    /// since the frame may change, the function's information is reloaded. Returns `Step::Continue` only when the budget runs out.
    fn run_loop(&mut self, budget: &mut u32) -> Step {
        let module = Arc::clone(&self.module);
        loop {
            let Some(frame) = self.frames.last().copied() else {
                return self.step(budget);
            };
            let func = &module.funcs[frame.func as usize];
            let code: &[u32] = &func.code;
            let base = frame.base as usize;
            let mut pc = frame.pc as usize;
            // Instruction not on the fast path: write back pc and leave it to step
            macro_rules! slow {
                () => {{
                    self.frames.last_mut().unwrap().pc = pc as u32;
                    match self.step(budget) {
                        Step::Continue if *budget == 0 => return Step::Continue,
                        Step::Continue => break,
                        other => return other,
                    }
                }};
            }
            loop {
                let Some(&word) = code.get(pc) else { slow!() };
                let opc = (word & 0xFF) as u8;
                if opc == op::WIDE {
                    slow!();
                }
                let a = ((word >> 8) & 0xFF) as usize;
                let b = ((word >> 16) & 0xFF) as usize;
                let c = ((word >> 24) & 0xFF) as usize;
                let (ra, rb, rc) = (base + a, base + b, base + c);
                let bx = (word >> 16) as usize;
                let sbx = (bx as u16 as i16) as i64;
                let sc = (c as u8 as i8) as i64;
                let next = pc + 1;
                let regs = &mut self.regs;
                match opc {
                    op::NOP => {}
                    op::MOVE => regs[ra] = regs[rb],
                    op::LOAD_ZERO => regs[ra] = 0,
                    op::LOAD_INT => regs[ra] = sbx,
                    op::LOAD_CONST => match self.const_cache[bx] {
                        Some(v) if !self.managed_consts[bx] => regs[ra] = v,
                        _ => slow!(),
                    },
                    op::LOAD_GLOBAL => regs[ra] = self.globals[bx],
                    op::STORE_GLOBAL => self.globals[bx] = regs[ra],
                    op::ADD_INT => regs[ra] = regs[rb].wrapping_add(regs[rc]),
                    op::SUB_INT => regs[ra] = regs[rb].wrapping_sub(regs[rc]),
                    op::MUL_INT => regs[ra] = regs[rb].wrapping_mul(regs[rc]),
                    op::NEG_INT => regs[ra] = regs[rb].wrapping_neg(),
                    op::AND_INT => regs[ra] = regs[rb] & regs[rc],
                    op::OR_INT => regs[ra] = regs[rb] | regs[rc],
                    op::XOR_INT => regs[ra] = regs[rb] ^ regs[rc],
                    op::NOT_INT => regs[ra] = !regs[rb],
                    op::SHL_INT => regs[ra] = regs[rb].wrapping_shl((regs[rc] & 63) as u32),
                    op::SHR_INT => {
                        regs[ra] = ((regs[rb] as u64).wrapping_shr((regs[rc] & 63) as u32)) as i64
                    }
                    op::SHL_INT32 => {
                        regs[ra] = ((regs[rb] as i32).wrapping_shl((regs[rc] & 31) as u32)) as i64
                    }
                    op::SHR_INT32 => {
                        regs[ra] =
                            (((regs[rb] as u32).wrapping_shr((regs[rc] & 31) as u32)) as i32) as i64
                    }
                    op::ADD_INT_I => regs[ra] = regs[rb].wrapping_add(sc),
                    op::NEG_F64 => regs[ra] = f64_slot(-slot_f64(regs[rb])),
                    op::NEG_F32 => regs[ra] = f32_slot(-slot_f32(regs[rb])),
                    op::ADD_F64..=op::LE_F32 => regs[ra] = real_op(opc, regs[rb], regs[rc]),
                    op::NARROW => regs[ra] = narrow(regs[rb], c as u32),
                    op::EQ_INT => regs[ra] = (regs[rb] == regs[rc]) as i64,
                    op::LT_INT => regs[ra] = (regs[rb] < regs[rc]) as i64,
                    op::LE_INT => regs[ra] = (regs[rb] <= regs[rc]) as i64,
                    op::LT_UINT => regs[ra] = ((regs[rb] as u64) < (regs[rc] as u64)) as i64,
                    op::LE_UINT => regs[ra] = ((regs[rb] as u64) <= (regs[rc] as u64)) as i64,
                    op::NOT_BOOL => regs[ra] = regs[rb] ^ 1,
                    op::JMP
                    | op::JMP_IF_TRUE
                    | op::JMP_IF_FALSE
                    | op::JMP_IF_EQ_INT
                    | op::JMP_IF_NE_INT
                    | op::JMP_IF_LT_INT
                    | op::JMP_IF_LE_INT
                    | op::JMP_IF_LT_UINT
                    | op::JMP_IF_LE_UINT => {
                        let (taken, off) = match opc {
                            op::JMP => (true, sbx),
                            op::JMP_IF_TRUE => (regs[ra] != 0, sbx),
                            op::JMP_IF_FALSE => (regs[ra] == 0, sbx),
                            op::JMP_IF_EQ_INT => (regs[ra] == regs[rb], sc),
                            op::JMP_IF_NE_INT => (regs[ra] != regs[rb], sc),
                            op::JMP_IF_LT_INT => (regs[ra] < regs[rb], sc),
                            op::JMP_IF_LE_INT => (regs[ra] <= regs[rb], sc),
                            op::JMP_IF_LT_UINT => ((regs[ra] as u64) < (regs[rb] as u64), sc),
                            _ => ((regs[ra] as u64) <= (regs[rb] as u64), sc),
                        };
                        if taken {
                            pc = (next as i64 + off) as usize;
                            if off < 0 {
                                // Decrement the budget on a backward jump (3.9). When exhausted, perform the jump and then return
                                *budget = budget.saturating_sub(1);
                                if *budget == 0 {
                                    self.frames.last_mut().unwrap().pc = pc as u32;
                                    return Step::Continue;
                                }
                            }
                            continue;
                        }
                    }
                    op::CALL => {
                        // Same as `Call` in `step`. Growing the registers and exceeding the limit (exception) are left to step
                        let f = &module.funcs[bx];
                        let new_base = ra;
                        let need = new_base + f.regs as usize;
                        let bytes = need * std::mem::size_of::<Slot>()
                            + (self.frames.len() + 1) * std::mem::size_of::<Frame>();
                        if bytes > self.config.stack_limit || regs.len() < need {
                            slow!();
                        }
                        *budget = budget.saturating_sub(1);
                        for slot in &mut regs[new_base + f.params as usize..need] {
                            *slot = 0;
                        }
                        self.frames.last_mut().unwrap().pc = next as u32;
                        self.frames.push(Frame {
                            func: bx as u32,
                            base: new_base as u32,
                            pc: 0,
                            ret_reg: new_base as u32,
                        });
                        if *budget == 0 {
                            return Step::Continue;
                        }
                        break;
                    }
                    op::RET | op::RET_VOID => {
                        if self.frames.len() <= 1 || !func.managed.is_empty() {
                            // A return from the first frame (<init> → <main>, exit) and the implicit
                            // `finally` are left to step
                            slow!();
                        }
                        let value = if opc == op::RET { regs[ra] } else { 0 };
                        let fr = self.frames.pop().unwrap();
                        if opc == op::RET {
                            regs[fr.ret_reg as usize] = value;
                        }
                        break;
                    }
                    _ => slow!(),
                }
                pc = next;
            }
        }
    }

    fn step(&mut self, budget: &mut u32) -> Step {
        let Some(frame) = self.frames.last().copied() else {
            return Step::Fatal("no frame".into());
        };
        let func = &self.module.funcs[frame.func as usize];
        let code = &func.code;
        let pc = frame.pc as usize;
        if pc >= code.len() {
            return Step::Fatal(format!("pc {pc} out of range in {}", func.name));
        }
        let (op0, _, _, _) = bytecode::decode(code[pc]);
        let wide = op0 == op::WIDE;
        let body = if wide { pc + 1 } else { pc };
        let (opc, a8, b8, c8) = bytecode::decode(code[body]);
        let (a, b, c) = if wide {
            let (_, ea, eb, ec) = bytecode::decode(code[body + 1]);
            (a8 | (ea << 8), b8 | (eb << 8), c8 | (ec << 8))
        } else {
            (a8, b8, c8)
        };
        let next = if wide { pc + 3 } else { pc + 1 };
        let bx = bytecode::decode_bx(code[body], wide.then(|| code[body + 1]));
        let sbx: i64 = if wide {
            bx as i32 as i64
        } else {
            (b8 | (c8 << 8)) as u16 as i16 as i64
        };
        let sc: i64 = if wide {
            (c as u16 as i16) as i64
        } else {
            (c8 as u8 as i8) as i64
        };
        let base = frame.base as usize;
        let ra = base + a as usize;
        let rb = base + b as usize;
        let rc = base + c as usize;
        let module = Arc::clone(&self.module);
        let regs = &mut self.regs;
        macro_rules! r {
            ($i:expr) => {
                regs[$i]
            };
        }
        let mut new_pc = next as u32;
        let mut jump = |off: i64, budget: &mut u32| {
            let t = next as i64 + off;
            if off < 0 {
                *budget = budget.saturating_sub(1);
            }
            new_pc = t as u32;
        };
        let overflow = |t: &str| {
            rt(
                exc::INT_OVERFLOW,
                format!("integer overflow in {t} arithmetic"),
            )
        };
        match opc {
            op::NOP => {}
            op::MOVE => r!(ra) = r!(rb),
            op::LOAD_ZERO => r!(ra) = 0,
            op::LOAD_INT => r!(ra) = sbx,
            op::LOAD_CONST => {
                let v = match self.load_const(bx as usize) {
                    Ok(v) => v,
                    Err(PollError::Rt(e)) => return Step::Error(e),
                    Err(PollError::Fatal(e)) => return Step::Fatal(e.message),
                };
                if self.managed_consts[bx as usize] {
                    // The constant cache keeps its reference; the register gets one of its own
                    try_ref!(self.put_shared(ra, v));
                } else {
                    self.regs[ra] = v;
                }
            }
            op::LOAD_GLOBAL => r!(ra) = self.globals[bx as usize],
            op::STORE_GLOBAL => self.globals[bx as usize] = r!(ra),
            op::MAKE_REF => r!(ra) = make_ref(1, rb as u64),
            op::MAKE_GLOBAL_REF => r!(ra) = make_ref(2, bx as u64),
            op::OUTER_REF => {
                // Follow `C` static links (each in register 0 of its frame; `docs/spec/semantics.md` 4.9)
                let mut frame = base;
                for _ in 0..c {
                    let (kind, at) = deref(r!(frame));
                    if kind != 1 || at >= regs.len() {
                        return Step::Fatal("bad static link".into());
                    }
                    frame = at;
                }
                r!(ra) = make_ref(1, (frame + b as usize) as u64);
            }
            op::LOAD_REF => {
                let slot = r!(rb);
                match self.ref_read(slot) {
                    Ok(v) => self.regs[ra] = v,
                    Err(e) => return e.into_step(),
                }
            }
            op::STORE_REF => {
                let (slot, v) = (r!(ra), r!(rb));
                let loc = match self.ref_loc(slot) {
                    Ok(l) => l,
                    Err(e) => return e.into_step(),
                };
                if let Err(e) = self.loc_replace(loc, v) {
                    return e.into_step();
                }
            }
            op::ADD_INT => r!(ra) = r!(rb).wrapping_add(r!(rc)),
            op::SUB_INT => r!(ra) = r!(rb).wrapping_sub(r!(rc)),
            op::MUL_INT => r!(ra) = r!(rb).wrapping_mul(r!(rc)),
            op::DIV_INT | op::DIV_INT_CHK => {
                let d = r!(rc);
                if d == 0 {
                    return Step::Error(rt(exc::DIV_BY_ZERO, "division by zero"));
                }
                let n = r!(rb);
                if opc == op::DIV_INT_CHK && n == i64::MIN && d == -1 {
                    return Step::Error(overflow("Int64"));
                }
                r!(ra) = n.wrapping_div(d);
            }
            op::MOD_INT => {
                let d = r!(rc);
                if d == 0 {
                    return Step::Error(rt(exc::DIV_BY_ZERO, "division by zero"));
                }
                r!(ra) = r!(rb).wrapping_rem(d);
            }
            op::DIV_UINT => {
                let d = r!(rc) as u64;
                if d == 0 {
                    return Step::Error(rt(exc::DIV_BY_ZERO, "division by zero"));
                }
                r!(ra) = ((r!(rb) as u64) / d) as i64;
            }
            op::MOD_UINT => {
                let d = r!(rc) as u64;
                if d == 0 {
                    return Step::Error(rt(exc::DIV_BY_ZERO, "division by zero"));
                }
                r!(ra) = ((r!(rb) as u64) % d) as i64;
            }
            op::NEG_INT => r!(ra) = r!(rb).wrapping_neg(),
            op::AND_INT => r!(ra) = r!(rb) & r!(rc),
            op::OR_INT => r!(ra) = r!(rb) | r!(rc),
            op::XOR_INT => r!(ra) = r!(rb) ^ r!(rc),
            op::NOT_INT => r!(ra) = !r!(rb),
            op::SHL_INT => r!(ra) = r!(rb).wrapping_shl((r!(rc) & 63) as u32),
            op::SHR_INT => r!(ra) = ((r!(rb) as u64).wrapping_shr((r!(rc) & 63) as u32)) as i64,
            op::SHL_INT32 => r!(ra) = ((r!(rb) as i32).wrapping_shl((r!(rc) & 31) as u32)) as i64,
            op::SHR_INT32 => {
                r!(ra) = (((r!(rb) as u32).wrapping_shr((r!(rc) & 31) as u32)) as i32) as i64
            }
            op::ADD_INT_I => r!(ra) = r!(rb).wrapping_add(sc),
            op::SLICE_RANGE => {
                let (start, count) = slice_range(r!(ra), r!(rb), r!(rb + 1), r!(rb + 2));
                r!(rb) = start;
                r!(rb + 1) = count;
            }
            op::ADD_INT_CHK => match r!(rb).checked_add(r!(rc)) {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::SUB_INT_CHK => match r!(rb).checked_sub(r!(rc)) {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::MUL_INT_CHK => match r!(rb).checked_mul(r!(rc)) {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::NEG_INT_CHK => match r!(rb).checked_neg() {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::ADD_UINT_CHK => match (r!(rb) as u64).checked_add(r!(rc) as u64) {
                Some(v) => r!(ra) = v as i64,
                None => return Step::Error(overflow("UInt64")),
            },
            op::SUB_UINT_CHK => match (r!(rb) as u64).checked_sub(r!(rc) as u64) {
                Some(v) => r!(ra) = v as i64,
                None => return Step::Error(overflow("UInt64")),
            },
            op::MUL_UINT_CHK => match (r!(rb) as u64).checked_mul(r!(rc) as u64) {
                Some(v) => r!(ra) = v as i64,
                None => return Step::Error(overflow("UInt64")),
            },
            op::NARROW => r!(ra) = narrow(r!(rb), c),
            op::NARROW_OVF | op::NARROW_RNG => {
                let v = r!(rb);
                let (name, lo, hi) = width_type(c);
                // Width 6 (Int64) checks UInt64 → Int64 (u64 bits above i64::MAX look negative),
                // width 7 (UInt64) checks signed → UInt64 (negative is out of range). Both are decided by the sign bit
                let ok = if matches!(c, 6 | 7) {
                    v >= 0
                } else {
                    (v as i128) >= lo && (v as i128) <= hi
                };
                if !ok {
                    if opc == op::NARROW_OVF {
                        return Step::Error(overflow(name));
                    }
                    // An out-of-range value at width 6 is a UInt64 value, so display it unsigned
                    let shown = if c == 6 {
                        (v as u64).to_string()
                    } else {
                        v.to_string()
                    };
                    return Step::Error(rt(
                        exc::RANGE_ERROR,
                        format!("value {shown} is out of range for {name} ({lo}..{hi})"),
                    ));
                }
                r!(ra) = v;
            }
            op::EQ_INT => r!(ra) = (r!(rb) == r!(rc)) as i64,
            op::LT_INT => r!(ra) = (r!(rb) < r!(rc)) as i64,
            op::LE_INT => r!(ra) = (r!(rb) <= r!(rc)) as i64,
            op::LT_UINT => r!(ra) = ((r!(rb) as u64) < (r!(rc) as u64)) as i64,
            op::LE_UINT => r!(ra) = ((r!(rb) as u64) <= (r!(rc) as u64)) as i64,
            op::NOT_BOOL => r!(ra) = r!(rb) ^ 1,
            op::JMP => jump(sbx, budget),
            op::JMP_IF_TRUE => {
                if r!(ra) != 0 {
                    jump(sbx, budget);
                }
            }
            op::JMP_IF_FALSE => {
                if r!(ra) == 0 {
                    jump(sbx, budget);
                }
            }
            op::JMP_IF_EQ_INT => {
                if r!(ra) == r!(rb) {
                    jump(sc, budget);
                }
            }
            op::JMP_IF_NE_INT => {
                if r!(ra) != r!(rb) {
                    jump(sc, budget);
                }
            }
            op::JMP_IF_LT_INT => {
                if r!(ra) < r!(rb) {
                    jump(sc, budget);
                }
            }
            op::JMP_IF_LE_INT => {
                if r!(ra) <= r!(rb) {
                    jump(sc, budget);
                }
            }
            op::JMP_IF_LT_UINT => {
                if (r!(ra) as u64) < (r!(rb) as u64) {
                    jump(sc, budget);
                }
            }
            op::JMP_IF_LE_UINT => {
                if (r!(ra) as u64) <= (r!(rb) as u64) {
                    jump(sc, budget);
                }
            }
            op::JMP_IF_EQ_STR | op::JMP_IF_NE_STR => {
                let eq = get_str!(self.heap, regs[ra]).equals(get_str!(self.heap, regs[rb]));
                if eq == (opc == op::JMP_IF_EQ_STR) {
                    jump(sc, budget);
                }
            }
            op::CONCAT_STR => {
                let x = get_str!(self.heap, regs[rb]);
                let y = get_str!(self.heap, regs[rc]);
                if x.len() + y.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                if x.is_empty() || y.is_empty() {
                    let s = if x.is_empty() { regs[rc] } else { regs[rb] };
                    try_ref!(self.put_shared(ra, s));
                } else {
                    let n = x.concat(y);
                    match self.alloc_string(n) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::STR_LEN => r!(ra) = get_str!(self.heap, regs[rb]).len() as i64,
            op::STR_INDEX => {
                let s = get_str!(self.heap, regs[rb]);
                let i = regs[rc];
                if i < 1 || i > s.len() as i64 {
                    return Step::Error(rt(
                        exc::RANGE_ERROR,
                        format!("string index {i} is out of range 1..{}", s.len()),
                    ));
                }
                regs[ra] = s.unit(i as usize - 1) as i64;
            }
            op::STR_SET_INDEX => {
                let s = get_str!(self.heap, regs[ra]);
                let i = regs[rb];
                if i < 1 || i > s.len() as i64 {
                    return Step::Error(rt(
                        exc::RANGE_ERROR,
                        format!("string index {i} is out of range 1..{}", s.len()),
                    ));
                }
                let (index, unit) = (i as usize - 1, regs[rc] as u16);
                // The only reference writes in place; a shared string (the constant cache holds
                // one of every literal) is copied (`docs/spec/bytecode.md` 2.4)
                let in_place = match self.heap.unique_str_mut(regs[ra]) {
                    Ok(Some(s)) => s.set_unit_in_place(index, unit),
                    Ok(None) => false,
                    Err(e) => return Step::Fatal(e.message),
                };
                if !in_place {
                    let n = get_str!(self.heap, self.regs[ra]).with_unit(index, unit);
                    match self.alloc_string(n) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::CMP_STR => {
                let o = get_str!(self.heap, regs[rb]).compare(get_str!(self.heap, regs[rc]));
                regs[ra] = o as i64;
            }
            op::EQ_STR => {
                regs[ra] =
                    get_str!(self.heap, regs[rb]).equals(get_str!(self.heap, regs[rc])) as i64
            }
            op::STR_FROM_CHAR => {
                let u = regs[rb] as u16;
                match self.alloc_string(PStr::from_units(&[u])) {
                    Ok(v) => try_ref!(self.put(ra, v)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::INT_TO_STR | op::UINT_TO_STR | op::BOOL_TO_STR => {
                let v = regs[rb];
                let text = match opc {
                    op::INT_TO_STR => v.to_string(),
                    op::UINT_TO_STR => (v as u64).to_string(),
                    _ => if v != 0 { "TRUE" } else { "FALSE" }.to_string(),
                };
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::ENUM_TO_STR => {
                let text = match module.type_entry(c) {
                    Some(t) => t.enum_name(regs[rb]),
                    None => return Step::Fatal(format!("type {c} is not an enumeration")),
                };
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::STR_TO_INT | op::STR_TO_INT64 => {
                let s = get_str!(self.heap, regs[rb]);
                let units = s.units();
                let (lo, hi) = if opc == op::STR_TO_INT {
                    (i32::MIN as i128, i32::MAX as i128)
                } else {
                    (i64::MIN as i128, i64::MAX as i128)
                };
                match strconv::parse_int(&units, false) {
                    Some(v) if v >= lo && v <= hi => regs[ra] = v as i64,
                    _ => {
                        let text = s.to_utf8();
                        return Step::Error(RtError {
                            class: exc::CONVERT_ERROR,
                            message: format!("'{text}' is not a valid integer"),
                            extra: Extra::Str(text),
                        });
                    }
                }
            }
            op::STR_TO_INT_DEF => {
                let s = get_str!(self.heap, regs[rb]);
                let units = s.units();
                let def = regs[rb + 1];
                regs[ra] = match strconv::parse_int(&units, false) {
                    Some(v) if v >= i32::MIN as i128 && v <= i32::MAX as i128 => v as i64,
                    _ => def,
                };
            }
            op::STR_TO_INT_IO => {
                let s = get_str!(self.heap, regs[rb]);
                let units = s.units();
                let (_, lo, hi) = width_type(c);
                match strconv::parse_int(&units, true) {
                    Some(v) if v >= lo && v <= hi => regs[ra] = v as i64,
                    _ => {
                        let text = s.to_utf8();
                        return Step::Error(input::input_error(format!(
                            "invalid integer input: '{text}'"
                        )));
                    }
                }
            }
            op::STR_TO_FLOAT_IO => {
                // `Read` / `ReadLn` of a real or a `Currency` (`docs/spec/semantics.md` 6.2): the
                // `StrToFloat` syntax, trailing blanks allowed
                let s = get_str!(self.heap, regs[rb]);
                let mut units = s.units();
                while units.last().is_some_and(|u| strconv::is_blank(*u)) {
                    units.pop();
                }
                let v = numfmt::parse_f64(&units).and_then(|v| match c as i64 {
                    bytecode::FLOAT_IO_SINGLE => Some(f32_slot(v as f32)),
                    bytecode::FLOAT_IO_CURRENCY => numfmt::f64_to_curr(v),
                    _ => Some(f64_slot(v)),
                });
                match v {
                    Some(v) => regs[ra] = v,
                    None => {
                        let text = s.to_utf8();
                        return Step::Error(input::input_error(format!(
                            "invalid floating-point input: '{text}'"
                        )));
                    }
                }
            }
            op::STR_COPY => {
                let s = get_str!(self.heap, regs[rb]);
                match copy_range(regs[rb + 1], regs[rb + 2], s.len()) {
                    None => try_ref!(self.put(ra, 0)),
                    Some((start, end)) if start == 0 && end == s.len() => {
                        let whole = regs[rb];
                        try_ref!(self.put_shared(ra, whole));
                    }
                    Some((start, end)) => match self.alloc_string(s.slice(start, end)) {
                        Ok(x) => try_ref!(self.put(ra, x)),
                        Err(e) => return Step::Error(e),
                    },
                }
            }
            op::STR_POS => {
                let sub = get_str!(self.heap, regs[rb]).units();
                let s = get_str!(self.heap, regs[rb + 1]).units();
                let pos = if sub.is_empty() || sub.len() > s.len() {
                    0
                } else {
                    s.windows(sub.len())
                        .position(|w| w == &sub[..])
                        .map(|p| p as i64 + 1)
                        .unwrap_or(0)
                };
                regs[ra] = pos;
            }
            op::STR_UPPER_ASCII | op::STR_LOWER_ASCII => {
                let s = get_str!(self.heap, regs[rb]);
                let units: Vec<u16> = s
                    .units()
                    .into_iter()
                    .map(|u| {
                        if opc == op::STR_UPPER_ASCII && (97..=122).contains(&u) {
                            u - 32
                        } else if opc == op::STR_LOWER_ASCII && (65..=90).contains(&u) {
                            u + 32
                        } else {
                            u
                        }
                    })
                    .collect();
                match self.alloc_string(PStr::from_units(&units)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::STR_TRIM | op::STR_TRIM_LEFT | op::STR_TRIM_RIGHT => {
                let s = get_str!(self.heap, regs[rb]);
                let units = s.units();
                let (start, end) =
                    text::trimmed(&units, opc != op::STR_TRIM_RIGHT, opc != op::STR_TRIM_LEFT);
                if start == 0 && end == units.len() {
                    let same = regs[rb];
                    try_ref!(self.put_shared(ra, same));
                } else {
                    match self.alloc_string(PStr::from_units(&units[start..end])) {
                        Ok(x) => try_ref!(self.put(ra, x)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::INT_TO_HEX => {
                let digits = regs[rc];
                let text = format!("{:X}", regs[rb] as u64);
                let pad = (digits.max(0) as u64).saturating_sub(text.len() as u64);
                if pad > (self.heap.str_room().saturating_sub(text.len())) as u64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                let mut units = vec![u16::from(b'0'); pad as usize];
                units.extend(text.encode_utf16());
                match self.alloc_string(PStr::from_units(&units)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::STR_TO_BOOL => {
                let s = get_str!(self.heap, regs[rb]);
                match text::parse_bool(&s.units()) {
                    Some(b) => regs[ra] = b as i64,
                    None => {
                        let text = s.to_utf8();
                        return Step::Error(RtError {
                            class: exc::CONVERT_ERROR,
                            message: format!("'{text}' is not a valid boolean value"),
                            extra: Extra::Str(text),
                        });
                    }
                }
            }
            op::BOOL_STR => {
                let text = match (regs[rb] != 0, regs[rc] != 0) {
                    (true, false) => "-1",
                    (false, false) => "0",
                    (true, true) => "True",
                    (false, true) => "False",
                };
                match self.alloc_string(PStr::from_text(text)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::SAME_TEXT | op::COMPARE_TEXT => {
                let a = get_str!(self.heap, regs[rb]).units();
                let b = get_str!(self.heap, regs[rc]).units();
                regs[ra] = if opc == op::SAME_TEXT {
                    text::same_text(&a, &b) as i64
                } else {
                    text::compare_text(&a, &b) as i64
                };
            }
            op::STR_UPPER | op::STR_LOWER => {
                let s = get_str!(self.heap, regs[rb]).units();
                let full = regs[rc] == bytecode::CASE_MAPPING_FULL;
                let units = text::convert_case(&s, opc == op::STR_UPPER, full);
                if units.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(PStr::from_units(&units)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::ANSI_SAME_TEXT | op::ANSI_COMPARE_TEXT => {
                let a = get_str!(self.heap, regs[rb]).units();
                let b = get_str!(self.heap, regs[rb + 1]).units();
                let upper = regs[rb + 2] == bytecode::CASE_FOLD_UPPER;
                let order = text::compare_folded(&a, &b, upper);
                regs[ra] = if opc == op::ANSI_SAME_TEXT {
                    order.is_eq() as i64
                } else {
                    order as i64
                };
            }
            op::STR_REPLACE => {
                let s = get_str!(self.heap, regs[rb]).units();
                let old = get_str!(self.heap, regs[rb + 1]).units();
                let new = get_str!(self.heap, regs[rb + 2]).units();
                let flags = regs[rb + 3];
                let all = flags & (1 << bytecode::REPLACE_FLAG_ALL) != 0;
                let ignore_case = flags & (1 << bytecode::REPLACE_FLAG_IGNORE_CASE) != 0;
                let units = text::replace(&s, &old, &new, all, ignore_case);
                if units.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(PStr::from_units(&units)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::STR_OF_CHAR => {
                let count = regs[rc];
                if count > self.heap.str_room() as i64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                let units = vec![regs[rb] as u16; count.max(0) as usize];
                match self.alloc_string(PStr::from_units(&units)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::QUOTED_STR => {
                let units = text::quoted(&get_str!(self.heap, regs[rb]).units());
                if units.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(PStr::from_units(&units)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::TRY_STR_TO_INT => {
                let s = get_str!(self.heap, regs[rb]);
                match strconv::parse_int(&s.units(), false) {
                    Some(v) if v >= i32::MIN as i128 && v <= i32::MAX as i128 => {
                        regs[ra] = 1;
                        regs[rc] = v as i64;
                    }
                    _ => {
                        regs[ra] = 0;
                        regs[rc] = 0;
                    }
                }
            }
            op::FORMAT => {
                let fmt = get_str!(self.heap, regs[rb]).units();
                let mut args = Vec::with_capacity(c as usize);
                for i in 0..c as usize {
                    let kind = regs[rb + 1 + 2 * i];
                    let v = regs[rb + 2 + 2 * i];
                    args.push(match kind {
                        0..=bytecode::FORMAT_KIND_INT_LAST => {
                            let (_, lo, _) = width_type(kind as u32);
                            let value = if lo == 0 { v as u64 as i128 } else { v as i128 };
                            let bits = match kind {
                                0 | 1 => 8,
                                2 | 3 => 16,
                                4 | 5 => 32,
                                _ => 64,
                            };
                            format::FmtArg::Int { value, bits }
                        }
                        bytecode::FORMAT_KIND_CHAR => format::FmtArg::Char(v as u16),
                        bytecode::FORMAT_KIND_BOOLEAN => format::FmtArg::Bool,
                        bytecode::FORMAT_KIND_STRING => {
                            format::FmtArg::Str(get_str!(self.heap, v).units())
                        }
                        bytecode::FORMAT_KIND_UTF8 => format::FmtArg::Str(
                            String::from_utf8_lossy(get_u8!(self.heap, v))
                                .encode_utf16()
                                .collect(),
                        ),
                        bytecode::FORMAT_KIND_SINGLE => format::FmtArg::Single(slot_f32(v)),
                        bytecode::FORMAT_KIND_DOUBLE => format::FmtArg::Double(slot_f64(v)),
                        bytecode::FORMAT_KIND_CURRENCY => format::FmtArg::Curr(v),
                        k => return Step::Fatal(format!("unknown `Format` argument kind {k}")),
                    });
                }
                match format::format(&fmt, &args, self.heap.str_room()) {
                    Ok(units) => match self.alloc_string(PStr::from_units(&units)) {
                        Ok(x) => try_ref!(self.put(ra, x)),
                        Err(e) => return Step::Error(e),
                    },
                    Err(format::FormatError::TooLong) => {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    Err(format::FormatError::Invalid) => {
                        let text = String::from_utf16_lossy(&fmt);
                        return Step::Error(RtError {
                            class: exc::CONVERT_ERROR,
                            message: format!(
                                "format '{text}' invalid or incompatible with argument"
                            ),
                            extra: Extra::Str(text),
                        });
                    }
                }
            }
            // `NegF64` / `NegF32` have the shape `A B`: `C` is not verified, so it is not read
            op::NEG_F64 => regs[ra] = f64_slot(-slot_f64(regs[rb])),
            op::NEG_F32 => regs[ra] = f32_slot(-slot_f32(regs[rb])),
            op::ADD_F64..=op::LE_F32 => regs[ra] = real_op(opc, regs[rb], regs[rc]),
            op::INT_TO_F64 => regs[ra] = f64_slot(regs[rb] as f64),
            op::UINT_TO_F64 => regs[ra] = f64_slot(regs[rb] as u64 as f64),
            op::INT_TO_F32 => regs[ra] = f32_slot(regs[rb] as f32),
            op::UINT_TO_F32 => regs[ra] = f32_slot(regs[rb] as u64 as f32),
            op::F64_TO_F32 => regs[ra] = f32_slot(slot_f64(regs[rb]) as f32),
            op::F32_TO_F64 => regs[ra] = f64_slot(slot_f32(regs[rb]) as f64),
            op::SQRT_F64 => regs[ra] = f64_slot(slot_f64(regs[rb]).sqrt()),
            op::TRUNC_F64 | op::ROUND_F64 => {
                let v = slot_f64(regs[rb]);
                let round = opc == op::ROUND_F64;
                match numfmt::f64_to_int(v, round) {
                    Some(n) => regs[ra] = n,
                    None => {
                        let name = if round { "Round" } else { "Trunc" };
                        return Step::Error(rt(
                            exc::INVALID_OP,
                            format!(
                                "invalid floating-point operation: {name}({})",
                                numfmt::f64_to_str(v)
                            ),
                        ));
                    }
                }
            }
            op::ADD_CURR | op::SUB_CURR | op::MUL_CURR | op::DIV_CURR => {
                let (x, y) = (regs[rb], regs[rc]);
                let v = match opc {
                    op::ADD_CURR => numfmt::curr_add(x, y),
                    op::SUB_CURR => numfmt::curr_sub(x, y),
                    op::MUL_CURR => numfmt::curr_mul(x, y),
                    _ => match numfmt::curr_div(x, y) {
                        Ok(v) => v,
                        Err(()) => {
                            return Step::Error(rt(exc::ZERO_DIVIDE, "division by zero"));
                        }
                    },
                };
                match v {
                    Some(v) => regs[ra] = v,
                    None => return Step::Error(rt(exc::OVERFLOW, CURRENCY_OVERFLOW)),
                }
            }
            op::NEG_CURR => match regs[rb].checked_neg() {
                Some(v) => regs[ra] = v,
                None => return Step::Error(rt(exc::OVERFLOW, CURRENCY_OVERFLOW)),
            },
            op::INT_TO_CURR | op::UINT_TO_CURR | op::F64_TO_CURR => {
                let x = regs[rb];
                let v = match opc {
                    op::INT_TO_CURR => numfmt::int_to_curr(x as i128),
                    op::UINT_TO_CURR => numfmt::int_to_curr(x as u64 as i128),
                    _ => numfmt::f64_to_curr(slot_f64(x)),
                };
                match v {
                    Some(v) => regs[ra] = v,
                    None => return Step::Error(rt(exc::OVERFLOW, CURRENCY_OVERFLOW)),
                }
            }
            op::CURR_TO_F64 => regs[ra] = f64_slot(numfmt::curr_to_f64(regs[rb])),
            op::CURR_TO_F32 => regs[ra] = f32_slot(numfmt::curr_to_f32(regs[rb])),
            op::TRUNC_CURR => regs[ra] = numfmt::curr_to_int(regs[rb], false),
            op::ROUND_CURR => regs[ra] = numfmt::curr_to_int(regs[rb], true),
            op::F64_TO_STR | op::F32_TO_STR | op::CURR_TO_STR => {
                let v = regs[rb];
                let text = match opc {
                    op::F64_TO_STR => numfmt::f64_to_str(slot_f64(v)),
                    op::F32_TO_STR => numfmt::f32_to_str(slot_f32(v)),
                    _ => numfmt::curr_to_str(v),
                };
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::F64_TO_FIXED | op::CURR_TO_FIXED => {
                let (v, d) = (regs[rb], regs[rc]);
                // A finite value has at least `d` digits after the point
                let finite = opc == op::CURR_TO_FIXED || slot_f64(v).is_finite();
                if finite && d > self.heap.str_room() as i64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                let text = if opc == op::F64_TO_FIXED {
                    numfmt::f64_fixed(slot_f64(v), d)
                } else {
                    numfmt::curr_fixed(v, d)
                };
                let Some(text) = text else {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                };
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::STR_TO_F64 => {
                let s = get_str!(self.heap, regs[rb]);
                match numfmt::parse_f64(&s.units()) {
                    Some(v) => regs[ra] = f64_slot(v),
                    None => {
                        let text = s.to_utf8();
                        return Step::Error(RtError {
                            class: exc::CONVERT_ERROR,
                            message: format!("'{text}' is not a valid floating-point value"),
                            extra: Extra::Str(text),
                        });
                    }
                }
            }
            op::STR_PAD_LEFT => {
                let w = regs[rc];
                let s = get_str!(self.heap, regs[rb]);
                if w <= s.len() as i64 {
                    let same = regs[rb];
                    try_ref!(self.put_shared(ra, same));
                } else {
                    if w > self.heap.str_room() as i64 {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    let pad = w as usize - s.len();
                    let mut units = vec![32u16; pad];
                    units.extend(s.units());
                    match self.alloc_string(PStr::from_units(&units)) {
                        Ok(x) => try_ref!(self.put(ra, x)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::CALL => {
                *budget = budget.saturating_sub(1);
                let callee = bx;
                let f = &module.funcs[callee as usize];
                let new_base = base + a as usize;
                let need = new_base + f.regs as usize;
                // Recursion limit
                let bytes = need * std::mem::size_of::<Slot>()
                    + (self.frames.len() + 1) * std::mem::size_of::<Frame>();
                let params = f.params;
                // The callee owns its managed value arguments; when the frame cannot be made they
                // are released here, as if the callee had been entered and left
                let failed = |this: &mut Self, e: RtError| match this.release_managed(
                    callee,
                    new_base as u32,
                    |r| r < params,
                ) {
                    Ok(()) => Step::Error(e),
                    Err(b) => Step::Fatal(b.message),
                };
                if bytes > self.config.stack_limit {
                    let e = rt(
                        exc::STACK_OVERFLOW,
                        format!("stack overflow (limit {} bytes)", self.config.stack_limit),
                    );
                    return failed(self, e);
                }
                if self.regs.len() < need {
                    let grow = (need - self.regs.len()) * std::mem::size_of::<Slot>();
                    if self.heap.account(grow).is_err()
                        || self.regs.try_reserve(need - self.regs.len()).is_err()
                    {
                        return failed(self, rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    self.regs.resize(need, 0);
                }
                // Zero the registers other than the arguments
                for slot in &mut self.regs[new_base + f.params as usize..need] {
                    *slot = 0;
                }
                self.frames.last_mut().unwrap().pc = next as u32;
                self.frames.push(Frame {
                    func: callee,
                    base: new_base as u32,
                    pc: 0,
                    ret_reg: new_base as u32,
                });
                return Step::Continue;
            }
            op::READ_WORD
            | op::READ_REST
            | op::READ_CHAR
            | op::READ_LN_BEGIN
            | op::READ_LN_END
            | op::IN_EOF
            | op::IN_EOLN
            | op::FLUSH_OUT => return self.input_op(opc, ra, pc as u32, next as u32),
            op::CALL_HOST => {
                // Advance `pc` only on success (so that a failure is reported at the `CallHost` line)
                let step = self.call_host(bx as usize, ra as u32, next as u32);
                if matches!(step, Step::Continue) {
                    self.frames.last_mut().unwrap().pc = next as u32;
                }
                return step;
            }
            op::RET | op::RET_VOID => {
                // The result's reference is set aside so the implicit `finally` does not release it,
                // and written to the caller after the frame's managed registers are released
                // (the destination is the callee's base, the first argument)
                let value = if opc == op::RET {
                    std::mem::take(&mut regs[ra])
                } else {
                    0
                };
                try_ref!(self.release_managed(frame.func, frame.base, |_| true));
                let fr = self.frames.pop().unwrap();
                if self.frames.is_empty() {
                    return self.frames_empty_after_return();
                }
                if opc == op::RET {
                    self.regs[fr.ret_reg as usize] = value;
                }
                return Step::Continue;
            }
            op::HALT => {
                let code = regs[ra];
                self.set_exit_code(code);
                return Step::Halt(None);
            }
            op::RAISE => {
                let x = std::mem::take(&mut regs[ra]);
                if x == 0 {
                    return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
                }
                return Step::Throw(x);
            }
            op::RAISE_OUTER => {
                let x = std::mem::take(&mut regs[ra]);
                if x == 0 {
                    return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
                }
                // `rB` becomes the `InnerException` of `rA`, unless that makes a cycle (5.4)
                let inner = regs[rb];
                let cycle = match self.heap.exc_reaches(inner, x) {
                    Ok(c) => c,
                    Err(e) => return Step::Fatal(e.message),
                };
                if inner != 0 && !cycle {
                    try_ref!(self.heap.add_ref(inner));
                    let old = match self.heap.exc_replace(x, bytecode::EXC_SLOT_INNER, inner) {
                        Ok(Some(o)) => o,
                        Ok(None) => return Step::Fatal("exception without an inner slot".into()),
                        Err(e) => return Step::Fatal(e.message),
                    };
                    try_ref!(self.heap.release(old));
                }
                return Step::Throw(x);
            }
            op::END_FINALLY => {
                let state = regs[ra];
                match state {
                    FINALLY_NORMAL => {
                        // Skip the `Jmp`s that follow
                        let mut p = next;
                        for _ in 0..b {
                            p += instruction_len(code, p);
                        }
                        new_pc = p as u32;
                    }
                    FINALLY_EXCEPTION => {
                        regs[ra] = 0;
                        let x = std::mem::take(&mut regs[ra + 1]);
                        return Step::Throw(x);
                    }
                    FINALLY_HALT => {
                        regs[ra] = 0;
                        return Step::Halt(Some(pc as u32));
                    }
                    k if k >= 3 && k - 2 <= b as i64 => {
                        regs[ra] = 0;
                        // The `k`-th `Jmp` after the instruction
                        let mut p = next;
                        for _ in 1..k - 2 {
                            p += instruction_len(code, p);
                        }
                        match jump_target(code, p) {
                            Some(t) => new_pc = t as u32,
                            None => return Step::Fatal("`EndFinally` without its `Jmp`".into()),
                        }
                    }
                    _ => return Step::Fatal(format!("bad `finally` state {state}")),
                }
            }
            op::EXC_CLASS => {
                let x = regs[rb];
                if x == 0 {
                    return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
                }
                match self.heap.exc_type(x) {
                    Ok(t) => self.regs[ra] = t as i64,
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::CLASS_IS => {
                let ty = regs[ra];
                regs[ra] = (ty >= 0 && self.heap.class_is(ty as u32, bx)) as i64;
            }
            op::NEW_EXC => {
                let msg = std::mem::take(&mut regs[ra]);
                match self.heap.alloc_exc(bx, msg, false) {
                    Ok(x) => regs[ra] = x,
                    Err(_) => {
                        try_ref!(self.heap.release(msg));
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                }
            }
            op::EXC_GET => {
                let x = regs[rb];
                if x == 0 {
                    return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
                }
                let v = match self.heap.exc_slot(x, c) {
                    Ok(Some(v)) => v,
                    Ok(None) => return Step::Fatal(format!("exception slot {c} out of range")),
                    Err(e) => return Step::Fatal(e.message),
                };
                let ty = match self.heap.exc_type(x) {
                    Ok(t) => t,
                    Err(e) => return Step::Fatal(e.message),
                };
                let managed = self
                    .heap
                    .type_entry(ty)
                    .is_some_and(|t| t.slot_is_ref(c as usize));
                if managed {
                    try_ref!(self.put_shared(ra, v));
                } else {
                    self.regs[ra] = v;
                }
            }
            op::EXC_SET => {
                let x = regs[ra];
                if x == 0 {
                    return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
                }
                let v = regs[rc];
                let ty = match self.heap.exc_type(x) {
                    Ok(t) => t,
                    Err(e) => return Step::Fatal(e.message),
                };
                let managed = self
                    .heap
                    .type_entry(ty)
                    .is_some_and(|t| t.slot_is_ref(b as usize));
                if managed {
                    try_ref!(self.heap.add_ref(v));
                }
                let old = match self.heap.exc_replace(x, b, v) {
                    Ok(Some(o)) => o,
                    Ok(None) => return Step::Fatal(format!("exception slot {b} out of range")),
                    Err(e) => return Step::Fatal(e.message),
                };
                if managed {
                    try_ref!(self.heap.release(old));
                }
            }
            op::EXC_INFO => {
                let x = regs[rb];
                if x == 0 {
                    return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
                }
                let v = match c {
                    bytecode::EXC_INFO_SUPPRESSED => {
                        let items = match self.heap.exc_suppressed(x) {
                            Ok(s) => s.to_vec(),
                            Err(e) => return Step::Fatal(e.message),
                        };
                        if items.is_empty() {
                            0
                        } else {
                            for &i in &items {
                                try_ref!(self.heap.add_ref(i));
                            }
                            match self
                                .heap
                                .alloc_agg(bytecode::EXCEPTION_ARRAY_TYPE, items.clone())
                            {
                                Ok(a) => a,
                                Err(_) => {
                                    for i in items {
                                        try_ref!(self.heap.release(i));
                                    }
                                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                                }
                            }
                        }
                    }
                    _ => {
                        let text = if c == bytecode::EXC_INFO_STACK_TRACE {
                            self.stack_trace_text(x)
                        } else {
                            self.class_name(x)
                        };
                        let text = match text {
                            Ok(t) => t,
                            Err(e) => return Step::Fatal(e.message),
                        };
                        match self.alloc_string(PStr::from_text(&text)) {
                            Ok(v) => v,
                            Err(e) => return Step::Error(e),
                        }
                    }
                };
                try_ref!(self.put(ra, v));
            }
            op::ARG_COUNT => regs[ra] = self.args.len().saturating_sub(1) as i64,
            op::ARG_GET => {
                let i = regs[rb];
                let text = if i >= 0 && (i as usize) < self.args.len() {
                    self.args[i as usize].clone()
                } else {
                    String::new()
                };
                if text.encode_utf16().count() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::ASSIGN_REF => {
                let v = regs[rb];
                try_ref!(self.put_shared(ra, v));
            }
            op::MOVE_REF => {
                let v = std::mem::take(&mut regs[rb]);
                try_ref!(self.put(ra, v));
            }
            op::ADD_REF => try_ref!(self.heap.add_ref(regs[ra])),
            op::RELEASE => {
                let old = std::mem::take(&mut regs[ra]);
                try_ref!(self.heap.release(old));
            }
            op::LOAD_GLOBAL_REF => {
                let v = self.globals[bx as usize];
                try_ref!(self.put_shared(ra, v));
            }
            op::STORE_GLOBAL_REF => {
                let v = regs[ra];
                try_ref!(self.heap.add_ref(v));
                let old = std::mem::replace(&mut self.globals[bx as usize], v);
                try_ref!(self.heap.release(old));
            }
            op::LOAD_REF_REF => {
                let slot = regs[rb];
                let v = match self.ref_read(slot) {
                    Ok(v) => v,
                    Err(e) => return e.into_step(),
                };
                try_ref!(self.put_shared(ra, v));
            }
            op::STORE_REF_REF | op::CLEAR_REF => {
                let (slot, v) = if opc == op::CLEAR_REF {
                    (regs[ra], 0)
                } else {
                    (regs[ra], regs[rb])
                };
                try_ref!(self.heap.add_ref(v));
                let loc = match self.ref_loc(slot) {
                    Ok(l) => l,
                    Err(e) => {
                        try_ref!(self.heap.release(v));
                        return e.into_step();
                    }
                };
                match self.loc_replace(loc, v) {
                    Ok(old) => try_ref!(self.heap.release(old)),
                    Err(e) => return e.into_step(),
                }
            }
            op::UNIQUE => {
                let ty = bx;
                match self.heap.unique(regs[ra], ty) {
                    Ok(Ok(v)) => self.regs[ra] = v,
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::VAL_GET | op::VAL_GET_REF | op::FIELD_GET | op::FIELD_GET_REF => {
                let container = regs[rb];
                let i = if matches!(opc, op::FIELD_GET | op::FIELD_GET_REF) {
                    c as i64
                } else {
                    regs[rc]
                };
                // A default record or array (0) has default slots
                let v = if container == 0 {
                    0
                } else {
                    match self.heap.slot(container, i) {
                        Ok(Some(v)) => v,
                        Ok(None) => {
                            return Step::Fatal(format!("slot {i} out of range in a read"));
                        }
                        Err(e) => return Step::Fatal(e.message),
                    }
                };
                if matches!(opc, op::VAL_GET_REF | op::FIELD_GET_REF) {
                    try_ref!(self.put_shared(ra, v));
                } else {
                    self.regs[ra] = v;
                }
            }
            op::VAL_SET | op::VAL_SET_REF | op::FIELD_SET | op::FIELD_SET_REF => {
                let container = regs[ra];
                let i = if matches!(opc, op::FIELD_SET | op::FIELD_SET_REF) {
                    b as i64
                } else {
                    regs[rb]
                };
                let v = regs[rc];
                if matches!(opc, op::VAL_SET_REF | op::FIELD_SET_REF) {
                    try_ref!(self.heap.add_ref(v));
                    let old = match self.heap.slot_mut(container, i) {
                        Ok(p) => std::mem::replace(p, v),
                        Err(e) => return Step::Fatal(e.message),
                    };
                    try_ref!(self.heap.release(old));
                } else {
                    match self.heap.slot_mut(container, i) {
                        Ok(p) => *p = v,
                        Err(e) => return Step::Fatal(e.message),
                    }
                }
            }
            // Sets (`docs/spec/types.md` 8.3, `docs/spec/bytecode.md` 6.3): a small set is the
            // bits of an integer, a big set a block of bit words (0 is the empty set)
            op::SET_IN => {
                let x = regs[rb];
                regs[ra] = ((0..64).contains(&x) && regs[rc] & (1 << x) != 0) as i64;
            }
            op::SET_RANGE => {
                let (lo, hi) = (regs[rb].max(0), regs[rc].min(63));
                regs[ra] = if lo > hi {
                    0
                } else {
                    let width = (hi - lo + 1) as u32;
                    let ones = if width == 64 {
                        -1i64
                    } else {
                        (1i64 << width) - 1
                    };
                    ones << lo
                };
            }
            op::SET_NEXT => {
                let (bits, from) = (regs[rb] as u64, regs[rc].max(0));
                regs[ra] = if from > 63 {
                    -1
                } else {
                    let rest = bits >> from;
                    if rest == 0 {
                        -1
                    } else {
                        from + rest.trailing_zeros() as i64
                    }
                };
            }
            op::CHK_SET => {
                let bad = regs[ra] & !regs[rb];
                if bad != 0 {
                    let tail = match &module.consts[c as usize] {
                        ConstEntry::Str(t) => t.as_str(),
                        _ => "",
                    };
                    let e = bad.trailing_zeros() as i64;
                    return Step::Error(rt(exc::RANGE_ERROR, value_message(tail, e)));
                }
            }
            op::SET_IN_BIG => {
                let (x, set) = (regs[rb], regs[rc]);
                let words = match self.heap.set_slots(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                self.regs[ra] = set_word_bit(words, x) as i64;
            }
            op::SET_INCL | op::SET_EXCL => {
                let (set, lo, hi) = (regs[ra], regs[rb], regs[rc]);
                let bits = match self.set_bits(set) {
                    Ok(b) => b,
                    Err(e) => return Step::Fatal(e),
                };
                let (lo, hi) = (lo.max(0), hi.min(bits - 1));
                let words = match self.heap.set_slots_mut(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                for i in lo..=hi {
                    let (k, m) = ((i / 64) as usize, 1i64 << (i % 64));
                    if opc == op::SET_INCL {
                        words[k] |= m;
                    } else {
                        words[k] &= !m;
                    }
                }
            }
            op::SET_UNION | op::SET_DIFF | op::SET_INTER | op::SET_UNION_BITS => {
                let (set, other) = (regs[ra], regs[rb]);
                let theirs: Vec<i64> = if opc == op::SET_UNION_BITS {
                    vec![other]
                } else {
                    match self.heap.set_slots(other) {
                        Ok(w) => w.to_vec(),
                        Err(e) => return Step::Fatal(e.message),
                    }
                };
                let bits = match self.set_bits(set) {
                    Ok(b) => b,
                    Err(e) => return Step::Fatal(e),
                };
                let words = match self.heap.set_slots_mut(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                for (k, w) in words.iter_mut().enumerate() {
                    let t = theirs.get(k).copied().unwrap_or(0);
                    *w = match opc {
                        op::SET_DIFF => *w & !t,
                        op::SET_INTER => *w & t,
                        _ => *w | t,
                    };
                }
                // Elements beyond the set's own type are not kept
                let extra = words.len() as i64 * 64 - bits;
                if let Some(last) = words.last_mut()
                    && extra > 0
                {
                    *last &= ((1u64 << (64 - extra)) - 1) as i64;
                }
            }
            op::SET_EQ | op::SET_SUB => {
                let (x, y) = (regs[rb], regs[rc]);
                let (xs, ys) = match (self.heap.set_slots(x), self.heap.set_slots(y)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(e), _) | (_, Err(e)) => return Step::Fatal(e.message),
                };
                let n = xs.len().max(ys.len());
                let at = |v: &[i64], k: usize| v.get(k).copied().unwrap_or(0);
                let holds = (0..n).all(|k| {
                    let (a, b) = (at(xs, k), at(ys, k));
                    if opc == op::SET_EQ {
                        a == b
                    } else {
                        a & !b == 0
                    }
                });
                self.regs[ra] = holds as i64;
            }
            op::SET_NEXT_BIG => {
                let (set, from) = (regs[rb], regs[rc].max(0));
                let words = match self.heap.set_slots(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                self.regs[ra] = set_next_in(words, from);
            }
            op::CHK_SET_BIG => {
                let (set, mask) = (regs[ra], regs[rb]);
                let (xs, ms) = match (self.heap.set_slots(set), self.heap.set_slots(mask)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(e), _) | (_, Err(e)) => return Step::Fatal(e.message),
                };
                let bad = xs.iter().enumerate().find_map(|(k, w)| {
                    let b = w & !ms.get(k).copied().unwrap_or(0);
                    (b != 0).then(|| k as i64 * 64 + b.trailing_zeros() as i64)
                });
                if let Some(e) = bad {
                    let tail = match &module.consts[c as usize] {
                        ConstEntry::Str(t) => t.as_str(),
                        _ => "",
                    };
                    return Step::Error(rt(exc::RANGE_ERROR, value_message(tail, e)));
                }
            }
            op::SET_BITS => {
                let set = regs[rb];
                let words = match self.heap.set_slots(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                self.regs[ra] = words.first().copied().unwrap_or(0);
            }
            op::CHK_INDEX => {
                let (i, len) = (regs[ra], regs[rb]);
                if !(0..len).contains(&i) {
                    let tail = match &module.consts[c as usize] {
                        ConstEntry::Str(t) => t.as_str(),
                        _ => "",
                    };
                    return Step::Error(rt(exc::RANGE_ERROR, index_message(tail, i)));
                }
            }
            op::CHK_RANGE => {
                // The value minus the lower bound, and the number of values (as unsigned, so a
                // range of `UInt64` compares right)
                let (i, len) = (regs[ra], regs[rb]);
                if (i as u64) >= (len as u64) {
                    let tail = match &module.consts[c as usize] {
                        ConstEntry::Str(t) => t.as_str(),
                        _ => "",
                    };
                    return Step::Error(rt(exc::RANGE_ERROR, range_message(tail, i)));
                }
            }
            op::DYN_GET | op::DYN_GET_REF => {
                let (container, i) = (regs[rb], regs[rc]);
                let v = match self.dyn_slot(container, i) {
                    Ok(v) => v,
                    Err(e) => return e.into_step(),
                };
                if opc == op::DYN_GET_REF {
                    try_ref!(self.put_shared(ra, v));
                } else {
                    self.regs[ra] = v;
                }
            }
            op::DYN_SET | op::DYN_SET_REF => {
                let (container, i, v) = (regs[ra], regs[rb], regs[rc]);
                if let Err(e) = self.dyn_slot(container, i) {
                    return e.into_step();
                }
                if opc == op::DYN_SET_REF {
                    try_ref!(self.heap.add_ref(v));
                    let old = match self.heap.slot_mut(container, i) {
                        Ok(p) => std::mem::replace(p, v),
                        Err(e) => return Step::Fatal(e.message),
                    };
                    try_ref!(self.heap.release(old));
                } else {
                    match self.heap.slot_mut(container, i) {
                        Ok(p) => *p = v,
                        Err(e) => return Step::Fatal(e.message),
                    }
                }
            }
            op::ARR_LEN => match self.heap.len(regs[rb]) {
                Ok(n) => self.regs[ra] = n as i64,
                Err(e) => return Step::Fatal(e.message),
            },
            op::SET_LEN => {
                let ty = bx;
                let n = regs[ra + 1];
                if n < 0 {
                    return Step::Error(rt(exc::RANGE_ERROR, format!("invalid length {n}")));
                }
                match self.heap.set_len(regs[ra], ty, n as usize) {
                    Ok(Ok(v)) => self.regs[ra] = v,
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::NEW_ARR => {
                // `rA` holds the length (an integer), so nothing is let go
                let ty = bx;
                let n = regs[ra].max(0) as usize;
                match self.heap.alloc_dyn(ty, n) {
                    Ok(v) => self.regs[ra] = v,
                    Err(_) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                }
            }
            op::ARR_COPY => {
                // `rA` holds the borrowed source, so nothing is let go
                let ty = bx;
                let (src, index, count) = (regs[ra], regs[ra + 1], regs[ra + 2]);
                match self.heap.copy_range(src, ty, index, count) {
                    Ok(Ok(v)) => self.regs[ra] = v,
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::STR_SET_LEN => {
                let n = regs[rb];
                if n < 0 {
                    return Step::Error(rt(exc::RANGE_ERROR, format!("invalid length {n}")));
                }
                if n as u64 > MAX_STRING_LEN as u64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                let s = get_str!(self.heap, regs[ra]);
                let n = n as usize;
                if n != s.len() {
                    // Only a new value is checked against the room left; the same length
                    // allocates nothing
                    if n > self.heap.str_room() {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    let mut units = s.units();
                    units.resize(n, 0);
                    match self.alloc_string(PStr::from_units(&units)) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::STR_TO_U8 => {
                let text = get_str!(self.heap, regs[rb]).to_utf8();
                if text.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_u8(text.into_bytes()) {
                    Ok(v) => try_ref!(self.put(ra, v)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::U8_TO_STR => {
                // Valid UTF-8 has at most as many UTF-16 code units as bytes, so the length fits
                let p = PStr::from_text(&String::from_utf8_lossy(get_u8!(self.heap, regs[rb])));
                match self.alloc_string(p) {
                    Ok(v) => try_ref!(self.put(ra, v)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::U8_LEN => r!(ra) = get_u8!(self.heap, regs[rb]).len() as i64,
            op::U8_INDEX => {
                let s = get_u8!(self.heap, regs[rb]);
                let i = regs[rc];
                if i < 1 || i > s.len() as i64 {
                    return Step::Error(rt(
                        exc::RANGE_ERROR,
                        format!("string index {i} is out of range 1..{}", s.len()),
                    ));
                }
                regs[ra] = s[i as usize - 1] as i64;
            }
            op::CONCAT_U8 => {
                let x = get_u8!(self.heap, regs[rb]);
                let y = get_u8!(self.heap, regs[rc]);
                if x.len() + y.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                if x.is_empty() || y.is_empty() {
                    let s = if x.is_empty() { regs[rc] } else { regs[rb] };
                    try_ref!(self.put_shared(ra, s));
                } else {
                    let n = [x, y].concat();
                    match self.alloc_u8(n) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::CMP_U8 => {
                // The byte order of UTF-8 is the code point order (`docs/spec/types.md` 8.5)
                let o = get_u8!(self.heap, regs[rb]).cmp(get_u8!(self.heap, regs[rc]));
                regs[ra] = o as i64;
            }
            op::EQ_U8 => {
                regs[ra] = (get_u8!(self.heap, regs[rb]) == get_u8!(self.heap, regs[rc])) as i64
            }
            op::U8_COPY => {
                let s = get_u8!(self.heap, regs[rb]);
                match copy_range(regs[rb + 1], regs[rb + 2], s.len()) {
                    None => try_ref!(self.put(ra, 0)),
                    Some((start, end)) => {
                        if let Err(e) =
                            check_u8_boundary(s, start).and_then(|_| check_u8_boundary(s, end))
                        {
                            return Step::Error(e);
                        }
                        if start == 0 && end == s.len() {
                            let whole = regs[rb];
                            try_ref!(self.put_shared(ra, whole));
                        } else {
                            let piece = s[start..end].to_vec();
                            match self.alloc_u8(piece) {
                                Ok(x) => try_ref!(self.put(ra, x)),
                                Err(e) => return Step::Error(e),
                            }
                        }
                    }
                }
            }
            op::U8_POS => {
                let sub = get_u8!(self.heap, regs[rb]);
                let s = get_u8!(self.heap, regs[rb + 1]);
                let pos = if sub.is_empty() || sub.len() > s.len() {
                    0
                } else {
                    s.windows(sub.len())
                        .position(|w| w == sub)
                        .map(|p| p as i64 + 1)
                        .unwrap_or(0)
                };
                regs[ra] = pos;
            }
            op::U8_SET_LEN => {
                let n = regs[rb];
                if n < 0 {
                    return Step::Error(rt(exc::RANGE_ERROR, format!("invalid length {n}")));
                }
                if n as u64 > MAX_STRING_LEN as u64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                let s = get_u8!(self.heap, regs[ra]);
                let n = n as usize;
                if n != s.len() {
                    // Only a new value is checked against the room left; the same length
                    // allocates nothing
                    if n > self.heap.str_room() {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    if let Err(e) = check_u8_boundary(s, n) {
                        return Step::Error(e);
                    }
                    let mut bytes = s.to_vec();
                    bytes.resize(n, 0);
                    match self.alloc_u8(bytes) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::UP_CASE_CHAR => {
                let u = regs[rb];
                regs[ra] = if (97..=122).contains(&u) { u - 32 } else { u };
            }
            op::STR_INSERT => {
                let src = get_str!(self.heap, regs[rb]).units();
                let s = get_str!(self.heap, regs[ra]).units();
                if !src.is_empty() {
                    if s.len() + src.len() > MAX_STRING_LEN {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    let pos = insert_pos(regs[rb + 1], s.len());
                    let units = [&s[..pos], &src[..], &s[pos..]].concat();
                    match self.alloc_string(PStr::from_units(&units)) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::STR_DELETE => {
                let s = get_str!(self.heap, regs[ra]).units();
                if let Some((start, end)) = delete_range(regs[rb], regs[rb + 1], s.len()) {
                    let units = [&s[..start], &s[end..]].concat();
                    match self.alloc_string(PStr::from_units(&units)) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::U8_INSERT => {
                let src = get_u8!(self.heap, regs[rb]).to_vec();
                let s = get_u8!(self.heap, regs[ra]);
                if !src.is_empty() {
                    if s.len() + src.len() > MAX_STRING_LEN {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    let pos = insert_pos(regs[rb + 1], s.len());
                    if let Err(e) = check_u8_boundary(s, pos) {
                        return Step::Error(e);
                    }
                    let bytes = [&s[..pos], &src[..], &s[pos..]].concat();
                    match self.alloc_u8(bytes) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::U8_DELETE => {
                let s = get_u8!(self.heap, regs[ra]);
                if let Some((start, end)) = delete_range(regs[rb], regs[rb + 1], s.len()) {
                    if let Err(e) =
                        check_u8_boundary(s, start).and_then(|_| check_u8_boundary(s, end))
                    {
                        return Step::Error(e);
                    }
                    let bytes = [&s[..start], &s[end..]].concat();
                    match self.alloc_u8(bytes) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::ARR_CONCAT => {
                // `rA` holds the borrowed left operand, so nothing is let go
                let (x, y) = (regs[ra], regs[ra + 1]);
                let lens = self.heap.len(x).and_then(|a| Ok((a, self.heap.len(y)?)));
                let (lx, ly) = match lens {
                    Ok(l) => l,
                    Err(e) => return Step::Fatal(e.message),
                };
                let pieces = [Piece::Range(x, 0, lx), Piece::Range(y, 0, ly)];
                match self.heap.build_dyn(bx, &pieces) {
                    Ok(Ok(v)) => self.regs[ra] = v,
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::ARR_INSERT | op::ARR_INSERT_ARR | op::ARR_DELETE => {
                // `rA` is the owned array read without counting; the compiler writes it back
                let arr = regs[ra];
                let (b, c) = (regs[ra + 1], regs[ra + 2]);
                let len = match self.heap.len(arr) {
                    Ok(n) => n,
                    Err(e) => return Step::Fatal(e.message),
                };
                let range_error =
                    |i: i64, n: usize| Step::Error(rt(exc::RANGE_ERROR, dyn_index_message(i, n)));
                let pieces = if opc == op::ARR_DELETE {
                    // `Count <= 0` does nothing without looking at `Index` (`docs/spec/types.md` 8.6)
                    if c <= 0 {
                        None
                    } else if b < 0 || b >= len as i64 {
                        return range_error(b, len);
                    } else {
                        let start = b as usize;
                        let end = b.saturating_add(c).min(len as i64) as usize;
                        Some(vec![
                            Piece::Range(arr, 0, start),
                            Piece::Range(arr, end, len),
                        ])
                    }
                } else if c < 0 || c > len as i64 {
                    // `Insert` may also put the source at the end (`0..Length`)
                    return range_error(c, len + 1);
                } else {
                    let at = c as usize;
                    let mid = if opc == op::ARR_INSERT {
                        Some(Piece::Elem(b))
                    } else {
                        match self.heap.len(b) {
                            Ok(0) => None,
                            Ok(n) => Some(Piece::Range(b, 0, n)),
                            Err(e) => return Step::Fatal(e.message),
                        }
                    };
                    mid.map(|m| vec![Piece::Range(arr, 0, at), m, Piece::Range(arr, at, len)])
                };
                if let Some(pieces) = pieces {
                    match self.heap.build_dyn(bx, &pieces) {
                        Ok(Ok(v)) => {
                            try_ref!(self.heap.release(arr));
                            self.regs[ra] = v;
                        }
                        Ok(Err(_)) => {
                            return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                        }
                        Err(e) => return Step::Fatal(e.message),
                    }
                }
            }
            op::UNIQUE_VAL => match self.heap.unique_val(regs[ra]) {
                Ok(Ok(v)) => self.regs[ra] = v,
                Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                Err(e) => return Step::Fatal(e.message),
            },
            op::MATERIALIZE => {
                if regs[ra] == 0 {
                    let ty = bx;
                    match self.heap.alloc_default(ty) {
                        Ok(Ok(v)) => self.regs[ra] = v,
                        Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                        Err(e) => return Step::Fatal(e.message),
                    }
                }
            }
            op::MAKE_CELL => {
                let root = regs[rb];
                let path: Vec<i64> = regs[ra + 1..=ra + c as usize].to_vec();
                // The root is a reference slot, or a dynamic array block (`docs/spec/bytecode.md` 2.8)
                let block_root = heap::is_block_ref(root) || root == 0;
                let mut v = if block_root {
                    root
                } else {
                    match self.ref_read(root) {
                        Ok(v) => v,
                        Err(e) => return e.into_step(),
                    }
                };
                if root == 0 {
                    let i = path[0];
                    return Step::Error(rt(exc::RANGE_ERROR, dyn_index_message(i, 0)));
                }
                // The compiler makes the containers unique first, so each one is a block whose
                // type the cell records
                let mut steps = Vec::with_capacity(path.len());
                for (k, &slot) in path.iter().enumerate() {
                    let ty = match self.heap.agg_type(v) {
                        Ok(t) => t,
                        Err(e) => return Step::Fatal(e.message),
                    };
                    match self.heap.slot_count(v) {
                        Ok(n) if (0..n as i64).contains(&slot) => {}
                        Ok(n) => {
                            return Step::Error(rt(exc::RANGE_ERROR, dyn_index_message(slot, n)));
                        }
                        Err(e) => return Step::Fatal(e.message),
                    }
                    steps.push((slot as u32, ty));
                    if k + 1 < path.len() {
                        v = match self.heap.slot(v, slot) {
                            Ok(Some(x)) => x,
                            Ok(None) => return Step::Fatal("cell path out of range".into()),
                            Err(e) => return Step::Fatal(e.message),
                        };
                    }
                }
                if block_root {
                    // The cell holds a reference of its root block (released with the cell)
                    try_ref!(self.heap.add_ref(root));
                }
                match self.heap.alloc_cell(root, steps) {
                    Ok(cell) => try_ref!(self.put(ra, cell)),
                    Err(_) => {
                        if block_root {
                            try_ref!(self.heap.release(root));
                        }
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                }
            }
            _ => return Step::Fatal(format!("unknown opcode {opc}")),
        }
        self.frames.last_mut().unwrap().pc = new_pc;
        Step::Continue
    }

    /// Pushes the `<init>` of the first unit from `k` on that has one, or `<main>` after the last
    /// (units in initialization order; `docs/spec/units.md` 5.3).
    fn start_init_from(&mut self, k: usize) {
        let next = self.module.units[k..]
            .iter()
            .enumerate()
            .find_map(|(i, u)| u.init.map(|f| (k + i, f)));
        match next {
            Some((unit, init)) => {
                // The units before it have finished their `<init>` (or have none; 5.4)
                self.inited = unit;
                self.phase = Phase::Init(unit);
                self.push_frame(init, 0, 0);
            }
            None => {
                self.inited = self.module.units.len();
                self.phase = Phase::Main;
                let entry = self.module.entry;
                self.push_frame(entry, 0, 0);
            }
        }
    }

    /// The first frame returned: start the next `<init>` or `<main>` if it was `<init>`; otherwise
    /// (`<main>` or a `<fini>`) the frames are done.
    fn frames_empty_after_return(&mut self) -> Step {
        match self.phase {
            Phase::Init(k) => {
                self.start_init_from(k + 1);
                Step::Continue
            }
            _ => Step::Finished(ExitKind::Code(self.exit_code_byte())),
        }
    }

    fn call_host(&mut self, index: usize, dst: u32, resume_pc: u32) -> Step {
        let kind = self.host_table[index];
        match kind {
            HostFn::ConsoleWrite | HostFn::ConsoleWriteError => {
                let text = get_str!(self.heap, self.regs[dst as usize]).to_utf8();
                if kind == HostFn::ConsoleWrite {
                    if let Err(e) = self.write_stdout(text.as_bytes()) {
                        return Step::Error(host_error_to_rt(e));
                    }
                    return Step::Continue;
                }
                if let Err(e) = self.flush_stdout() {
                    return Step::Error(host_error_to_rt(e));
                }
                let Some(console) = self.host.console.as_mut() else {
                    return Step::Continue;
                };
                let fut = console.write(OutStream::Stderr, text.as_bytes());
                self.pending = Some(PendingHost {
                    kind: PendingKind::Write(fut),
                    resume_pc,
                });
            }
            HostFn::EnvGet => {
                let name = get_str!(self.heap, self.regs[dst as usize]).to_utf8();
                let value = if name.is_empty() || name.contains('=') || name.contains('\0') {
                    None
                } else {
                    match self.host.env.as_mut() {
                        Some(env) => match env.get(&name) {
                            Ok(v) => v,
                            Err(e) => return Step::Error(host_error_to_rt(e)),
                        },
                        None => None,
                    }
                };
                let text = value.unwrap_or_default();
                if text.encode_utf16().count() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                // The value takes the place of the name, which is let go
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => try_ref!(self.put(dst as usize, x)),
                    Err(e) => return Step::Error(e),
                }
                return Step::Continue;
            }
        }
        match self.poll_pending() {
            Ok(true) => Step::Continue,
            Ok(false) => Step::Waiting,
            Err(PollError::Rt(e)) => Step::Error(e),
            Err(PollError::Fatal(e)) => Step::Fatal(e.message),
        }
    }
}

/// Where a reference slot leads (`docs/spec/bytecode.md` 1.5, 6.1).
#[derive(Debug, Clone, Copy)]
enum Loc {
    Reg(usize),
    Global(usize),
    /// Slot `1` of the record or array block `0` (unique, unless it is a dynamic array).
    Slot(Slot, i64),
    /// The dynamic array block at the root of a cell (read only: a dynamic array is not made unique).
    Block(Slot),
}

/// A failure while following a reference slot.
enum RefError {
    /// A range error on the path of a cell (an array shrank during the call).
    Rt(RtError),
    Fatal(String),
}

impl RefError {
    fn into_step(self) -> Step {
        match self {
            RefError::Rt(e) => Step::Error(e),
            RefError::Fatal(m) => Step::Fatal(m),
        }
    }
}

impl From<BadRef> for RefError {
    fn from(e: BadRef) -> Self {
        RefError::Fatal(e.message)
    }
}

impl Isolate {
    /// Element `i` of a dynamic array or open array block (0 is empty), range-checked.
    fn dyn_slot(&self, container: Slot, i: i64) -> Result<Slot, RefError> {
        let n = self.heap.len(container)?;
        if !(0..n as i64).contains(&i) {
            return Err(RefError::Rt(rt(exc::RANGE_ERROR, dyn_index_message(i, n))));
        }
        self.heap
            .slot(container, i)?
            .ok_or_else(|| RefError::Fatal("array slot out of range".into()))
    }

    /// Splits a reference slot into the register or global variable it starts from and the paths
    /// of the cells on the way, innermost cell first (`docs/design/lifetime.md` 3.7).
    fn ref_chain(&self, slot: Slot) -> Result<(Loc, Vec<heap::CellPath>), RefError> {
        let mut chain = Vec::new();
        let mut s = slot;
        loop {
            if heap::is_block_ref(s) {
                return Ok((Loc::Block(s), chain));
            }
            match deref(s) {
                (1, i) if i < self.regs.len() => return Ok((Loc::Reg(i), chain)),
                (2, i) if i < self.globals.len() => return Ok((Loc::Global(i), chain)),
                (k, _) if k as u64 == heap::CELL_TAG => {
                    let (root, path) = self.heap.cell(s)?;
                    chain.push(path);
                    s = root;
                }
                _ => return Err(RefError::Fatal("bad reference slot".into())),
            }
        }
    }

    fn loc_get(&self, loc: Loc) -> Result<Slot, RefError> {
        Ok(match loc {
            Loc::Reg(i) => self.regs[i],
            Loc::Global(i) => self.globals[i],
            Loc::Slot(b, i) => self
                .heap
                .slot(b, i)?
                .ok_or_else(|| RefError::Fatal("cell path out of range".into()))?,
            Loc::Block(b) => b,
        })
    }

    /// Writes a value to a location without counting; returns the old value.
    fn loc_replace(&mut self, loc: Loc, v: Slot) -> Result<Slot, RefError> {
        Ok(match loc {
            Loc::Reg(i) => std::mem::replace(&mut self.regs[i], v),
            Loc::Global(i) => std::mem::replace(&mut self.globals[i], v),
            Loc::Slot(b, i) => std::mem::replace(self.heap.slot_mut(b, i)?, v),
            Loc::Block(_) => {
                return Err(RefError::Fatal("write to the root block of a cell".into()));
            }
        })
    }

    /// The value a reference slot leads to, following the paths of cells (`LoadRef`). A default
    /// container (0) has default slots.
    fn ref_read(&self, slot: Slot) -> Result<Slot, RefError> {
        let (loc, chain) = self.ref_chain(slot)?;
        let mut v = self.loc_get(loc)?;
        for path in chain.iter().rev() {
            for &(s, _) in path {
                if v == 0 {
                    return Ok(0);
                }
                let n = self.heap.slot_count(v)?;
                v = self.heap.slot(v, s as i64)?.ok_or_else(|| {
                    RefError::Rt(rt(exc::RANGE_ERROR, dyn_index_message(s as i64, n)))
                })?;
            }
        }
        Ok(v)
    }

    /// The location a reference slot leads to, for writing: on the path of a cell each container
    /// is made unique first (a default container becomes a block of the recorded type), and the
    /// container's slot takes over the copy (`docs/spec/bytecode.md` 6.1, `MakeCell`).
    fn ref_loc(&mut self, slot: Slot) -> Result<Loc, RefError> {
        let (mut loc, chain) = self.ref_chain(slot)?;
        for path in chain.iter().rev() {
            for &(s, ty) in path {
                let v = self.loc_get(loc)?;
                // A dynamic array is shared, not made unique (`docs/spec/types.md` chapter 3)
                let n = if self.heap.is_dyn_type(ty) {
                    v
                } else {
                    match self.heap.unique(v, ty)? {
                        Ok(n) => n,
                        Err(_) => {
                            return Err(RefError::Rt(rt(exc::OUT_OF_MEMORY, "out of memory")));
                        }
                    }
                };
                if n != v {
                    self.loc_replace(loc, n)?;
                }
                let count = self.heap.len(n)?;
                if s as usize >= count {
                    return Err(RefError::Rt(rt(
                        exc::RANGE_ERROR,
                        dyn_index_message(s as i64, count),
                    )));
                }
                loc = Loc::Slot(n, s as i64);
            }
        }
        Ok(loc)
    }
}

/// The `ERangeError` message of a dynamic array or open array index (`docs/spec/semantics.md`
/// 7.1): `array index 5 is out of range 0..2`.
fn dyn_index_message(i: i64, len: usize) -> String {
    format!("array index {i} is out of range 0..{}", len as i64 - 1)
}

/// The `ERangeError` message of a static array index (`docs/spec/semantics.md` 7.1): `tail` is
/// the `ChkIndex` constant (`` `TArr` (0..3) ``), whose lower bound gives back the index as written.
fn index_message(tail: &str, i: i64) -> String {
    format!(
        "index {} is out of range for {tail}",
        shown_from_low(tail, i)
    )
}

/// The `ERangeError` message of an enumeration or subrange value (`docs/spec/types.md` 8.1,
/// 8.2): `tail` is the `ChkRange` constant (`` `TDigit` (0..9) ``).
/// Whether the ordinal `x` is in the bit words of a big set.
fn set_word_bit(words: &[i64], x: i64) -> bool {
    x >= 0
        && words
            .get((x / 64) as usize)
            .is_some_and(|w| w & (1 << (x % 64)) != 0)
}

/// The smallest element of a big set at least `from` (not negative), or -1.
fn set_next_in(words: &[i64], from: i64) -> i64 {
    let mut k = (from / 64) as usize;
    let mut w = match words.get(k) {
        Some(w) => (*w as u64) >> (from % 64) << (from % 64),
        None => return -1,
    };
    loop {
        if w != 0 {
            return k as i64 * 64 + w.trailing_zeros() as i64;
        }
        k += 1;
        match words.get(k) {
            Some(x) => w = *x as u64,
            None => return -1,
        }
    }
}

/// `ERangeError` of an element `v` of a set (`ChkSet`): `value 300 is out of range for tail`.
fn value_message(tail: &str, v: i64) -> String {
    format!(
        "value {} is out of range for {tail}",
        shown_from_low(tail, (v as i128 - tail_low(tail)) as i64)
    )
}

/// The lower bound written in a range tail (`(0..3)`, `(#97..#99)`, `(False..True)`).
fn tail_low(tail: &str) -> i128 {
    let low = tail
        .rsplit_once(" (")
        .and_then(|(_, r)| r.split_once(".."))
        .map_or("0", |(l, _)| l);
    if let Some(code) = low.strip_prefix('#') {
        code.parse().unwrap_or(0)
    } else if low == "False" || low == "True" {
        i128::from(low == "True")
    } else {
        low.parse().unwrap_or(0)
    }
}

fn range_message(tail: &str, i: i64) -> String {
    format!(
        "value {} is out of range for {tail}",
        shown_from_low(tail, i)
    )
}

/// The value `low + i` written like the lower bound of `tail` (`(0..3)`, `(#97..#99)`,
/// `(False..True)`).
fn shown_from_low(tail: &str, i: i64) -> String {
    let low = tail
        .rsplit_once(" (")
        .and_then(|(_, r)| r.split_once(".."))
        .map_or("0", |(l, _)| l);
    let v = tail_low(tail) + i as i128;
    if low.starts_with('#') {
        format!("#{v}")
    } else if low == "False" || low == "True" {
        if v != 0 { "True" } else { "False" }.to_string()
    } else {
        v.to_string()
    }
}

/// Whether `LoadConst` of the constant makes a managed value (counted; `docs/design/vm.md` 3.11).
fn is_managed_const(c: &ConstEntry) -> bool {
    matches!(
        c,
        ConstEntry::Str(_)
            | ConstEntry::U8(_)
            | ConstEntry::Utf16(_)
            | ConstEntry::Agg { .. }
            | ConstEntry::Set { .. }
    )
}

/// Counts type names into (type name, count) sorted by the bytes of the name (`docs/spec/diagnostics.md` 8.1).
fn count_leaks<'a>(names: impl Iterator<Item = &'a str>) -> Vec<(String, u64)> {
    let mut counts: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for n in names {
        *counts.entry(n).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(n, c)| (n.to_string(), c))
        .collect()
}

/// Failure of a pending host call: a runtime error, or a bad reference when storing its result (F0001).
enum PollError {
    Rt(RtError),
    Fatal(BadRef),
}

impl From<RtError> for PollError {
    fn from(e: RtError) -> Self {
        PollError::Rt(e)
    }
}

enum Step {
    Continue,
    Waiting,
    Finished(ExitKind),
    Error(RtError),
    /// `Raise` of an exception object (its reference moves to the unwinding).
    Throw(Slot),
    /// `Halt`, or `EndFinally` going on with it (the position of the `EndFinally`).
    Halt(Option<u32>),
    Fatal(String),
}

/// How the unwinding of an exception ended.
enum Unwound {
    /// A handler caught it; execution goes on there.
    Caught,
    /// It left a `finally` block run by `Halt`; `Halt` goes on from this position.
    Halt(u32),
    /// No handler: the report of the uncaught exception (the frames are gone).
    Uncaught(ExceptionReport),
}

/// The states of a `finally` block (`docs/spec/exceptions.md` 6.2): the body ended normally, an
/// exception is being propagated, `Halt` is unwinding. `k + 2` is the `k`-th jump.
const FINALLY_NORMAL: i64 = 0;
const FINALLY_EXCEPTION: i64 = 1;
const FINALLY_HALT: i64 = 2;

/// The number of words of the instruction at `pc` (3 with `Wide`).
fn instruction_len(code: &[u32], pc: usize) -> usize {
    match code.get(pc) {
        Some(&w) if (w & 0xFF) as u8 == op::WIDE => 3,
        _ => 1,
    }
}

/// The target of the `Jmp` at `pc` (`None` if it is not a `Jmp`).
fn jump_target(code: &[u32], pc: usize) -> Option<usize> {
    let wide = (code.get(pc)? & 0xFF) as u8 == op::WIDE;
    let body = if wide { pc + 1 } else { pc };
    let word = *code.get(body)?;
    if (word & 0xFF) as u8 != op::JMP {
        return None;
    }
    let off = if wide {
        bytecode::decode_bx(word, Some(*code.get(body + 1)?)) as i32 as i64
    } else {
        ((word >> 16) as u16 as i16) as i64
    };
    let next = pc + instruction_len(code, pc);
    usize::try_from(next as i64 + off).ok()
}

/// The message of `EAccessViolation` (`semantics.md` 7.1).
const NIL_ACCESS: &str = "access to a nil reference";

fn host_error_to_rt(e: HostError) -> RtError {
    match e.kind {
        HostErrorKind::Unsupported | HostErrorKind::NoPermission => rt(
            exc::NOT_SUPPORTED,
            format!("not supported on this target: {}", e.message),
        ),
        _ => RtError {
            class: exc::IN_OUT_ERROR,
            message: e.message,
            extra: Extra::Int(e.os_code.unwrap_or(0) as i64),
        },
    }
}

/// Reference slot: the upper 8 bits are the kind (1: register, 2: global variable), the lower 56 bits the index.
fn make_ref(kind: u8, index: u64) -> Slot {
    (((kind as u64) << 56) | (index & 0x00FF_FFFF_FFFF_FFFF)) as i64
}

fn deref(slot: Slot) -> (u8, usize) {
    let u = slot as u64;
    ((u >> 56) as u8, (u & 0x00FF_FFFF_FFFF_FFFF) as usize)
}

fn narrow(v: i64, width: u32) -> i64 {
    match width {
        0 => v as i8 as i64,
        1 => v as u8 as i64,
        2 => v as i16 as i64,
        3 | 8 => v as u16 as i64,
        4 => v as i32 as i64,
        5 => v as u32 as i64,
        _ => v,
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Test host (captures output).
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    pub struct CaptureConsole {
        pub stdout: Rc<RefCell<Vec<u8>>>,
        pub stderr: Rc<RefCell<Vec<u8>>>,
        pub stdin: Rc<RefCell<Vec<u8>>>,
    }

    impl puy_host::Console for CaptureConsole {
        fn write(&mut self, stream: OutStream, bytes: &[u8]) -> HostFuture<()> {
            match stream {
                OutStream::Stdout => self.stdout.borrow_mut().extend_from_slice(bytes),
                OutStream::Stderr => self.stderr.borrow_mut().extend_from_slice(bytes),
            }
            puy_host::ready(Ok(()))
        }
        fn read_line(&mut self) -> HostFuture<Option<String>> {
            let mut input = self.stdin.borrow_mut();
            if input.is_empty() {
                return puy_host::ready(Ok(None));
            }
            let end = input
                .iter()
                .position(|b| *b == b'\n')
                .map(|p| p + 1)
                .unwrap_or(input.len());
            let line: Vec<u8> = input.drain(..end).collect();
            let mut s = String::from_utf8_lossy(&line).into_owned();
            if s.ends_with('\n') {
                s.pop();
                if s.ends_with('\r') {
                    s.pop();
                }
            }
            puy_host::ready(Ok(Some(s)))
        }
    }

    pub struct MapEnv(pub Vec<(String, String)>);

    impl puy_host::Env for MapEnv {
        fn get(&mut self, name: &str) -> Result<Option<String>, HostError> {
            Ok(self
                .0
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::driver::{CompileOptions, compile};
    use crate::source::MemorySourceProvider;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Run {
        stdout: String,
        status: ExitStatus,
    }

    fn run_with(src: &str, stdin: &str, args: &[&str], config: IsolateConfig) -> Run {
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        assert!(
            !out.diagnostics.iter().any(|d| d.is_error()),
            "{src}\n{:#?}",
            out.diagnostics
        );
        let bytes = out.module.expect("module");
        let module = Arc::new(Module::decode(&bytes, crate::VERSION).unwrap());
        let stdout = Rc::new(RefCell::new(Vec::new()));
        let host = HostSet {
            console: Some(Box::new(CaptureConsole {
                stdout: stdout.clone(),
                stderr: Rc::new(RefCell::new(Vec::new())),
                stdin: Rc::new(RefCell::new(stdin.as_bytes().to_vec())),
            })),
            env: Some(Box::new(MapEnv(vec![("HOME".into(), "/home/x".into())]))),
            ..HostSet::empty()
        };
        let mut iso = Isolate::new(module, host, config).unwrap();
        let mut argv = vec!["t.pas".to_string()];
        argv.extend(args.iter().map(|s| s.to_string()));
        iso.start_main(argv).unwrap();
        let status = loop {
            match iso.run(1000) {
                RunOutcome::Finished(st) => break st,
                RunOutcome::BudgetExhausted => continue,
                RunOutcome::Waiting => panic!("waiting"),
                RunOutcome::Fatal(e) => panic!("fatal: {}", e.message),
            }
        };
        let s = String::from_utf8(stdout.borrow().clone()).unwrap();
        Run { stdout: s, status }
    }

    fn run(src: &str) -> Run {
        run_with(
            src,
            "",
            &[],
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            },
        )
    }

    fn out(src: &str) -> String {
        let r = run(src);
        assert_eq!(r.status.kind, ExitKind::Code(0), "{src}\n{:?}", r.status);
        r.stdout
    }

    fn error(src: &str) -> (String, String, String) {
        let r = run(src);
        match r.status.kind {
            ExitKind::UncaughtException(rep) => (rep.class.clone(), rep.message.clone(), r.stdout),
            other => panic!("expected an exception, got {other:?}\n{}", r.stdout),
        }
    }

    #[test]
    fn dynamic_array_length_limit_is_checked_before_reading() {
        // A result longer than 2^31 - 1 elements (`+`, `Insert`) is `EOutOfMemory`, decided
        // from the lengths alone: the pieces are never read (types.md 8.6). Building such arrays
        // for real would take gigabytes, so the pieces here name no block.
        let mut heap = Heap::new(None, true, &[]);
        let pieces = [Piece::Range(0, 0, MAX_STRING_LEN), Piece::Elem(7)];
        assert!(matches!(heap.build_dyn(64, &pieces), Ok(Err(OutOfMemory))));
    }

    #[test]
    fn wide_negative_immediates() {
        // Make the temporary registers after the locals not fit in 8 bits.
        let mut src = String::new();
        for i in 0..256 {
            src.push_str(&format!("var V{i}: Int64 := 0;\n"));
        }
        src.push_str(
            "{$Q-}\nvar N: Int64 := -1; WriteLn(N);\n\
             N := -32768; WriteLn(N); N := 32767; WriteLn(N);\n\
             N := 3; Dec(N); WriteLn(N); Inc(N, -2); WriteLn(N);\n\
             for var I := 3 downto 1 do begin Write(I); if I > 3 then Break; end; WriteLn;\n",
        );
        assert_eq!(out(&src), "-1\n-32768\n32767\n2\n0\n321\n");
    }

    /// Runs `<main>` made of `code` (with `regs` registers and the constant pool `consts`) after
    /// verifying it, and returns how it ended.
    fn run_code(consts: Vec<ConstEntry>, regs: u16, code: Vec<u32>) -> RunOutcome {
        use crate::bytecode::{Function, UnitEntry};

        let module = Module {
            types: Vec::new(),
            consts,
            funcs: vec![Function {
                name: "Test.<main>".into(),
                params: 0,
                regs,
                code,
                lines: vec![(0, 1)],
                locals: Vec::new(),
                managed: Vec::new(),
                file: 0,
                handlers: Vec::new(),
                outer: None,
            }],
            globals: Vec::new(),
            entry: 0,
            units: vec![UnitEntry {
                name: "Test".into(),
                path: "t.pas".into(),
                init: None,
                fini: None,
                first_global: 0,
                global_count: 0,
            }],
            hosts: Vec::new(),
        };
        let module = Module::decode(&module.encode(crate::VERSION), crate::VERSION).unwrap();
        module.verify().expect("valid module");
        let mut iso = Isolate::new(
            Arc::new(module),
            HostSet::empty(),
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            },
        )
        .unwrap();
        iso.start_main(vec!["t.pas".into()]).unwrap();
        iso.run(1000)
    }

    #[test]
    fn wide_constant_index() {
        use crate::bytecode::{encode, encode_ext};

        let mut consts = vec![ConstEntry::Int(0); 65537];
        consts[65536] = ConstEntry::Int(42);
        let code = vec![
            encode(op::WIDE, 0, 0, 0),
            encode(op::LOAD_CONST, 0, 0, 0),
            encode_ext(0, 256, 0),
            encode(op::HALT, 0, 0, 0),
        ];
        match run_code(consts, 1, code) {
            RunOutcome::Finished(status) => assert_eq!(status.kind, ExitKind::Code(42)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    // `NegF64` / `NegF32` have the shape `A B`, so the verifier does not check `C`: a verified
    // module with garbage in `C` must run (the VM must not read the register `C` names).
    fn real_negation_ignores_the_c_field() {
        use crate::bytecode::encode;

        for opc in [op::NEG_F64, op::NEG_F32] {
            let code = vec![
                encode(op::LOAD_ZERO, 0, 0, 0),
                encode(opc, 0, 0, 255),
                encode(op::HALT, 0, 0, 0),
            ];
            match run_code(Vec::new(), 1, code) {
                // The bits of -0.0 have the sign bit set; `Halt` keeps the low 8 bits (0)
                RunOutcome::Finished(status) => assert_eq!(status.kind, ExitKind::Code(0), "{opc}"),
                other => panic!("{opc}: {other:?}"),
            }
        }
    }

    #[test]
    fn hello() {
        assert_eq!(
            out("program Hello;\nbegin\n  WriteLn('Hello, Puy!');\nend."),
            "Hello, Puy!\n"
        );
        assert_eq!(
            out("WriteLn('a', 1, True, 'c', -5:4, 'x':3)"),
            "a1TRUEc  -5  x\n"
        );
        assert_eq!(out("Write('a'); Write; WriteLn; WriteLn"), "a\n\n");
    }

    #[test]
    fn arithmetic() {
        assert_eq!(
            out(
                "var A := 7; var B := 2;\nWriteLn(A + B, ' ', A - B, ' ', A * B, ' ', A div B, ' ', A mod B, ' ', -A, ' ', not A, ' ', A and B, ' ', A or B, ' ', A xor B, ' ', A shl 2, ' ', A shr 1);"
            ),
            "9 5 14 3 1 -7 -8 2 7 5 28 3\n"
        );
        assert_eq!(
            out("var A := -7; WriteLn(A div 2, ' ', A mod 2, ' ', A shr 1, ' ', A shl 31);"),
            "-3 -1 2147483644 -2147483648\n"
        );
        assert_eq!(
            out("var C: Cardinal := 4294967295; WriteLn(C, ' ', C shr 1, ' ', C - 1 + 1);"),
            "4294967295 2147483647 4294967295\n"
        );
        assert_eq!(
            error("var C: Cardinal := 4294967295; C := C + 1;").1,
            "integer overflow in Cardinal arithmetic"
        );
        assert_eq!(
            out("var U: UInt64 := 18446744073709551615; WriteLn(U, ' ', U div 2, ' ', U shr 60);"),
            "18446744073709551615 9223372036854775807 15\n"
        );
        assert_eq!(
            out(
                "var I: Int64 := 9223372036854775807; WriteLn(I, ' ', I div -1, ' ', (I - 1) + 1);"
            ),
            "9223372036854775807 -9223372036854775807 9223372036854775807\n"
        );
        assert_eq!(
            out("var B: Byte := 250; Inc(B, 5); WriteLn(B); {$R-} Inc(B); WriteLn(B);"),
            "255\n0\n"
        );
        assert_eq!(
            out("{$Q-}\nvar I := 2147483647; I := I + 1; WriteLn(I);"),
            "-2147483648\n"
        );
        assert_eq!(
            out("var X := 5; X := X * X + X; WriteLn(X, ' ', Abs(-3), ' ', Abs(X));"),
            "30 3 30\n"
        );
        assert_eq!(
            out("WriteLn(1 = 1, ' ', 1 <> 1, ' ', 1 < 2, ' ', 2 <= 1, ' ', 3 > 2, ' ', 3 >= 4);"),
            "TRUE FALSE TRUE FALSE TRUE FALSE\n"
        );
        assert_eq!(
            out(
                "var A := 3; var B := 4; WriteLn(A = B, ' ', A <> B, ' ', A < B, ' ', A <= B, ' ', A > B, ' ', A >= B);"
            ),
            "FALSE TRUE TRUE TRUE FALSE FALSE\n"
        );
    }

    #[test]
    fn runtime_errors() {
        assert_eq!(error("var I := 2147483647; I := I + 1;").0, "EIntOverflow");
        assert_eq!(
            error("var I := 2147483647; I := I + 1;").1,
            "integer overflow in Integer arithmetic"
        );
        assert_eq!(
            error("var B: Byte := 255; Inc(B);"),
            (
                "ERangeError".into(),
                "value 256 is out of range for Byte (0..255)".into(),
                String::new()
            )
        );
        assert_eq!(error("var Z := 0; WriteLn(1 div Z);").0, "EDivByZero");
        assert_eq!(
            error("var S := 'abc'; WriteLn(S[5]);").1,
            "string index 5 is out of range 1..3"
        );
        assert_eq!(
            error("uses System.SysUtils; WriteLn(StrToInt('abc'));").1,
            "'abc' is not a valid integer"
        );
        assert_eq!(
            error("Assert(1 > 2, 'oops');").1,
            "assertion failed: 1 > 2 (oops)"
        );
        assert_eq!(
            error("var N := 0; WriteLn('before'); Assert(N >\n 0);"),
            (
                "EAssertionFailed".into(),
                "assertion failed: N > 0".into(),
                "before\n".into()
            )
        );
        assert_eq!(
            error("var I: Int64 := -9223372036854775807 - 1; I := -I;").0,
            "EIntOverflow"
        );
        // Range checks for UInt64 → Int64 and signed → UInt64 (2.4)
        assert_eq!(
            error("var U: UInt64 := 18446744073709551615; var L: Int64 := U;").1,
            "value 18446744073709551615 is out of range for Int64 (-9223372036854775808..9223372036854775807)"
        );
        assert_eq!(
            error("var L: Int64 := -1; var U: UInt64 := L;").1,
            "value -1 is out of range for UInt64 (0..18446744073709551615)"
        );
        assert_eq!(
            out("{$R-}\nvar U: UInt64 := 18446744073709551615; var L: Int64 := U; WriteLn(L);"),
            "-1\n"
        );
        assert_eq!(error("WriteLn('x':3000000000);").0, "EOutOfMemory");
    }

    #[test]
    fn global_loop_variable() {
        // Iterations are counted in a hidden register; after the loop the value is that of the last entry to the body (4.5)
        assert_eq!(
            out(
                "program P;\nvar G: Integer;\nprocedure Bump; begin G := 100; end;\nbegin\n  for G := 1 to 3 do begin Write(G, ' '); Bump; end;\n  WriteLn(G);\n  for G := 5 downto 1 do begin if G = 4 then Break; Bump; end;\n  WriteLn(G);\n  G := 7; for G := 3 to 1 do Bump;\n  WriteLn(G);\nend."
            ),
            "1 2 3 3\n4\n7\n"
        );
        assert_eq!(
            error("var I: Int64 := -9223372036854775807 - 1; I := I div -1;").0,
            "EIntOverflow"
        );
        assert_eq!(
            error("var N: Integer := 0; ReadLn(N);").1,
            "invalid integer input: ''"
        );
    }

    #[test]
    fn strings() {
        assert_eq!(
            out(
                "uses System.SysUtils;\nvar S := 'Hello';\nWriteLn(S + ', ' + 'World', ' ', Length(S), ' ', S[1], ' ', Copy(S, 2, 3), ' ', Pos('l', S), ' ', UpperCase(S), ' ', LowerCase(S), ' ', Trim('  x '), '|', IntToStr(42), ' ', StrToInt('-$10'), ' ', StrToIntDef('zz', 7), ' ', StrToInt64('9223372036854775807'));"
            ),
            "Hello, World 5 H ell 3 HELLO hello x|42 -16 7 9223372036854775807\n"
        );
        assert_eq!(
            out(
                "var S := 'abc'; S[2] := 'X'; var T := S; T[1] := 'y'; WriteLn(S, ' ', T, ' ', S = 'aXc', ' ', 'a' < 'b', ' ', 'b' > 'a', ' ', 'a' = 'a');"
            ),
            "aXc yXc TRUE TRUE TRUE TRUE\n"
        );
        assert_eq!(
            out(
                "WriteLn('日本語', ' ', Length('日本語'), ' ', Ord('あ'), ' ', Chr(12354), ' ', #$D83D#$DE00, ' ', Length(#$D83D#$DE00));"
            ),
            "日本語 3 12354 あ 😀 2\n"
        );
        assert_eq!(out("WriteLn(#$D800)"), "\u{FFFD}\n");
        assert_eq!(
            out(
                "var C := 'a'; var S: string := C; S := S + C; WriteLn(S, Length(S), Ord(C), ' ', Chr(Ord(C) + 1));"
            ),
            "aa297 b\n"
        );
        assert_eq!(
            out(
                "WriteLn(Copy('abc', 0, 2), '|', Copy('abc', 4, 1), '|', Copy('abc', 2, 100), '|', Copy('abc', 1, -1), '|', Pos('', 'a'), '|', Pos('x', 'abc'));"
            ),
            "ab||bc||0|0\n"
        );
        assert_eq!(
            out("WriteLn('ab' < 'abc', ' ', 'B' < 'a', ' ', 'é' > 'z');"),
            "TRUE TRUE TRUE\n"
        );
    }

    #[test]
    fn control_flow_and_routines() {
        assert_eq!(
            out(
                "var S := 0;\nfor var I := 1 to 10 do if (I mod 2 = 0) and (I > 4) then S := S + I;\nWriteLn(S);"
            ),
            "24\n"
        );
        assert_eq!(
            out(
                "for var I := 3 downto 1 do Write(I); WriteLn; for var I := 5 to 1 do Write(I); WriteLn('x');"
            ),
            "321\nx\n"
        );
        assert_eq!(
            out(
                "var I := 0; while I < 3 do begin Inc(I); if I = 2 then Continue; Write(I); end; WriteLn; repeat Dec(I); if I = 1 then Break; until I = 0; WriteLn(I);"
            ),
            "13\n1\n"
        );
        assert_eq!(
            out(
                "program P;\nfunction Fact(N: Int64): Int64;\nbegin\n  if N <= 1 then Exit(1);\n  Result := N * Fact(N - 1);\nend;\nprocedure Swap(var A, B: Integer);\nvar T: Integer;\nbegin T := A; A := B; B := T; end;\nvar X, Y: Integer;\nbegin\n  X := 1; Y := 2; Swap(X, Y);\n  WriteLn(Fact(20), ' ', X, ' ', Y);\nend."
            ),
            "2432902008176640000 2 1\n"
        );
        assert_eq!(
            out(
                "program P;\nvar G: Integer = 5;\nprocedure Bump(out R: Integer; D: Byte);\nbegin R := G + D; end;\nvar V: Integer;\nbegin\n  Bump(V, 3); WriteLn(V, ' ', G);\n  for G := 1 to 3 do Write(G); WriteLn(' ', G);\nend."
            ),
            "8 5\n123 3\n"
        );
        assert_eq!(
            out(
                "var B: Boolean := False; if B or (1 = 1) then WriteLn('t'); if B and (1 = 1) then WriteLn('f'); if not B then WriteLn('n'); var C := B xor True; WriteLn(C);"
            ),
            "t\nn\nTRUE\n"
        );
        assert_eq!(
            out("if False then WriteLn(1) else if True then WriteLn(2) else WriteLn(3);"),
            "2\n"
        );
        assert_eq!(
            out(
                "var N := 0; procedure Inc3(var X: Integer); begin Inc(X, 3); end; Inc3(N); Inc3(N); WriteLn(N);"
            ),
            "6\n"
        );
        let r = run("WriteLn('x'); Halt(3); WriteLn('y');");
        assert_eq!(r.status.kind, ExitKind::Code(3));
        assert_eq!(r.stdout, "x\n");
        let r = run("Halt(-1);");
        assert_eq!(r.status.kind, ExitKind::Code(255));
        assert_eq!(
            out("procedure P; begin WriteLn('a'); Exit; WriteLn('b'); end; P;"),
            "a\n"
        );
        assert_eq!(
            out(
                "var X := 1; var S := 'a'; procedure Q(S: string; X: Integer); begin WriteLn(S, X); end; Q(S + 'b', X + 1); WriteLn(S, X);"
            ),
            "ab2\na1\n"
        );
    }

    #[test]
    fn input_and_env() {
        let r = run_with(
            "uses System.SysUtils;\nvar S := ''; var N := 0;\nReadLn(S); ReadLn(N); ReadLn; var T := ''; ReadLn(T);\nWriteLn(S, '|', N, '|', T, '|', ParamCount, '|', ParamStr(0), '|', ParamStr(1), '|', ParamStr(9), '|', GetEnvironmentVariable('HOME'), '|', GetEnvironmentVariable('NOPE'));",
            "hello\r\n 42 \nskip\nlast",
            &["a", "b"],
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            },
        );
        assert_eq!(r.stdout, "hello|42|last|2|t.pas|a||/home/x|\n");
    }

    #[test]
    fn stack_overflow_and_trace() {
        let r = run_with(
            "program P;\nprocedure Rec(N: Integer);\nbegin\n  Rec(N + 1);\nend;\nbegin\n  Rec(0);\nend.",
            "",
            &[],
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                stack_limit: 64 * 1024,
                ..Default::default()
            },
        );
        match r.status.kind {
            ExitKind::UncaughtException(rep) => {
                assert_eq!(rep.class, "EStackOverflow");
                assert_eq!(rep.message, "stack overflow (limit 65536 bytes)");
                assert_eq!(rep.stack.len(), 32);
                assert!(rep.omitted > 0);
                assert_eq!(rep.stack[0].function, "P.Rec");
                assert_eq!(rep.stack[0].line, 4);
                assert_eq!(rep.stack[31].function, "P.<main>");
                assert_eq!(rep.stack[31].line, 7);
                assert_eq!(rep.stack[31].file, "t.pas");
            }
            other => panic!("{other:?}"),
        }
        let r = run(
            "program P;\nfunction Second(S: string): Char;\nbegin\n  Result := S[5];\nend;\nbegin\n  WriteLn(Second('abc'));\nend.",
        );
        match r.status.kind {
            ExitKind::UncaughtException(rep) => {
                assert_eq!(
                    rep.render(),
                    "error: uncaught exception ERangeError: string index 5 is out of range 1..3\n  at P.Second (t.pas:4)\n  at P.<main> (t.pas:7)\n"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn budget_and_memory() {
        // The budget returns control even from an infinite loop
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            b"while True do ;".to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        let module = Arc::new(Module::decode(&out.module.unwrap(), crate::VERSION).unwrap());
        let mut iso = Isolate::new(
            module,
            HostSet::empty(),
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            },
        )
        .unwrap();
        iso.start_main(vec![]).unwrap();
        assert!(matches!(iso.run(10), RunOutcome::BudgetExhausted));
        assert!(matches!(iso.run(10), RunOutcome::BudgetExhausted));
        // Memory limit
        let r = run_with(
            "var S := 'xy'; while True do S := S + S;",
            "",
            &[],
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                memory_limit: Some(1024 * 1024),
                ..Default::default()
            },
        );
        match r.status.kind {
            ExitKind::UncaughtException(rep) => assert_eq!(rep.class, "EOutOfMemory"),
            other => panic!("{other:?}"),
        }
        // No Console: output is discarded, ReadLn raises
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            b"WriteLn('x'); var S := ''; ReadLn(S);".to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        let module = Arc::new(Module::decode(&out.module.unwrap(), crate::VERSION).unwrap());
        let mut iso = Isolate::new(
            module,
            HostSet::empty(),
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            },
        )
        .unwrap();
        iso.start_main(vec![]).unwrap();
        match iso.run(1000) {
            RunOutcome::Finished(st) => match st.kind {
                ExitKind::UncaughtException(rep) => assert_eq!(rep.class, "ENotSupportedException"),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn global_init_and_halt_in_init() {
        assert_eq!(
            out(
                "program P;\nconst N = 3;\nvar A: Integer = N * 2; B: string = 'x' + 'y'; C: Boolean;\nbegin WriteLn(A, B, C); end."
            ),
            "6xyFALSE\n"
        );
    }

    /// Runs `src` with leak checking to its first `Finished`, lets `prepare` touch the isolate (e.g. build heap
    /// blocks directly, which Pascal cannot leave behind yet), then runs the finishing stage and returns its outcome.
    fn finish_with(src: &str, leak_check: bool, prepare: impl FnOnce(&mut Isolate)) -> RunOutcome {
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        let bytes = out
            .module
            .unwrap_or_else(|| panic!("{src}\n{:#?}", out.diagnostics));
        let module = Arc::new(Module::decode(&bytes, crate::VERSION).unwrap());
        let config = IsolateConfig {
            hash_seed: HashSeed::Fixed(0),
            leak_check,
            ..Default::default()
        };
        let mut iso = Isolate::new(module, HostSet::empty(), config).unwrap();
        iso.start_main(vec!["t.pas".into()]).unwrap();
        match iso.run(1000) {
            RunOutcome::Finished(_) => {}
            other => panic!("{other:?}"),
        }
        prepare(&mut iso);
        iso.finish();
        iso.run(1000)
    }

    fn final_status(outcome: RunOutcome) -> ExitStatus {
        match outcome {
            RunOutcome::Finished(st) => st,
            other => panic!("expected the final Finished, got {other:?}"),
        }
    }

    // Strings the program made (pinned until M2-11) and the global variables and constants it released
    // leave no leak; the exit code is the program's.
    #[test]
    fn finishing_stage_reports_no_leak_for_a_program() {
        let st = final_status(finish_with(
            "program T;\nvar S: string;\nbegin S := 'a'; S := S + 'b'; Halt(3) end.",
            true,
            |_| {},
        ));
        assert_eq!(st.kind, ExitKind::Code(3));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
        assert_eq!(render_leaks(&st.leaks), "");
    }

    // An uncaught exception still ends with 1 after the finishing stage (`spec/units.md` 5.4).
    #[test]
    fn finishing_stage_keeps_the_failure_exit_code() {
        let st = final_status(finish_with("var S := 'ab'; WriteLn(S[5]);", true, |_| {}));
        assert_eq!(st.kind, ExitKind::Code(1));
    }

    // One block left on the heap is reported in the singular form, as text and JSON.
    #[test]
    fn leak_report_one_block() {
        let st = final_status(finish_with("program T;\nbegin end.", true, |iso| {
            iso.heap.alloc_test_str("left");
        }));
        assert_eq!(st.leaks, vec![("string".to_string(), 1)]);
        assert_eq!(
            render_leaks(&st.leaks),
            "warning: 1 heap block was not freed\n  string: 1\n"
        );
        assert_eq!(
            leaks_json(&st.leaks),
            r#"{"schema":"puy.leaks/1","blocks":1,"types":[{"type":"string","count":1}]}"#
        );
    }

    // Several blocks are counted per type; blocks released before the end are not counted.
    #[test]
    fn leak_report_several_blocks() {
        let st = final_status(finish_with("program T;\nbegin end.", true, |iso| {
            iso.heap.alloc_test_str("a");
            let freed = iso.heap.alloc_test_str("b");
            iso.heap.alloc_test_str("c");
            iso.heap.release(freed).unwrap();
        }));
        assert_eq!(
            render_leaks(&st.leaks),
            "warning: 2 heap blocks were not freed\n  string: 2\n"
        );
    }

    // Leak checking is reported only when enabled.
    #[test]
    fn leak_report_needs_leak_check() {
        let st = final_status(finish_with("program T;\nbegin end.", false, |iso| {
            iso.heap.alloc_test_str("left");
        }));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    // Type names are sorted by their bytes (upper case before lower case) with a total over all types.
    #[test]
    fn leak_report_orders_type_names_by_bytes() {
        let names = [
            "string",
            "Hello.TNode",
            "array of Hello.TNode",
            "TArray<Exception>",
            "Hello.TNode",
            "<cell>",
        ];
        let leaks = count_leaks(names.into_iter());
        assert_eq!(
            render_leaks(&leaks),
            "warning: 6 heap blocks were not freed\n  <cell>: 1\n  Hello.TNode: 2\n  TArray<Exception>: 1\n  array of Hello.TNode: 1\n  string: 1\n"
        );
        assert_eq!(
            leaks_json(&leaks),
            r#"{"schema":"puy.leaks/1","blocks":6,"types":[{"type":"<cell>","count":1},{"type":"Hello.TNode","count":2},{"type":"TArray<Exception>","count":1},{"type":"array of Hello.TNode","count":1},{"type":"string","count":1}]}"#
        );
    }

    // Releasing a global variable's block a second time is an internal error (F0001), not a panic.
    #[test]
    fn over_release_is_fatal() {
        let outcome = finish_with("program T;\nvar S: string;\nbegin end.", true, |iso| {
            let r = iso.heap.alloc_test_str("x");
            iso.globals[0] = r;
            iso.heap.release(r).unwrap();
        });
        match outcome {
            RunOutcome::Fatal(e) => assert_eq!(e.message, "release of a freed heap block 1"),
            other => panic!("expected Fatal, got {other:?}"),
        }
    }

    // With leak checking a freed block's index is not reused, so a stale reference keeps hitting the freed block.
    #[test]
    fn leak_check_does_not_reuse_indexes() {
        for (leak_check, expected) in [(true, 2), (false, 1)] {
            finish_with("program T;\nbegin end.", leak_check, |iso| {
                let first = iso.heap.alloc_test_str("a");
                iso.heap.release(first).unwrap();
                let second = iso.heap.alloc_test_str("b");
                assert_eq!(second, expected, "leak_check = {leak_check}");
                if leak_check {
                    assert_eq!(
                        iso.heap.get(first).unwrap_err().message,
                        "read of a freed heap block 1"
                    );
                }
            });
        }
    }

    /// A console whose reads are not ready on the first poll (a host that waits for input).
    struct SlowConsole {
        lines: Vec<String>,
        stdout: Rc<RefCell<Vec<u8>>>,
    }

    struct PendingOnce(Option<Option<String>>, bool);

    impl std::future::Future for PendingOnce {
        type Output = Result<Option<String>, HostError>;
        fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            if !self.1 {
                self.1 = true;
                return Poll::Pending;
            }
            Poll::Ready(Ok(self.0.take().expect("polled after completion")))
        }
    }

    impl puy_host::Console for SlowConsole {
        fn write(&mut self, _: OutStream, bytes: &[u8]) -> HostFuture<()> {
            self.stdout.borrow_mut().extend_from_slice(bytes);
            puy_host::ready(Ok(()))
        }
        fn read_line(&mut self) -> HostFuture<Option<String>> {
            let line = (!self.lines.is_empty()).then(|| self.lines.remove(0));
            Box::pin(PendingOnce(Some(line), false))
        }
    }

    #[test]
    fn input_instructions_resume_after_waiting() {
        // An input instruction that waits for a line runs again once the line is in the buffer,
        // keeping what it already read (`input.rs`); `Eof` waits for the line it reads ahead.
        let src = "var A, B, C: Integer;\nReadLn(A, B);\nRead(C);\nWriteLn(A + B + C, ' ', Eof);\nRead(C);\nWriteLn(C, ' ', Eof);";
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        let module =
            Arc::new(Module::decode(&out.module.expect("module"), crate::VERSION).unwrap());
        let stdout = Rc::new(RefCell::new(Vec::new()));
        let host = HostSet {
            console: Some(Box::new(SlowConsole {
                lines: vec!["1 2".into(), "".into(), "  3".into(), "4".into()],
                stdout: stdout.clone(),
            })),
            ..HostSet::empty()
        };
        let config = IsolateConfig {
            hash_seed: HashSeed::Fixed(0),
            ..IsolateConfig::default()
        };
        let mut iso = Isolate::new(module, host, config).unwrap();
        iso.start_main(vec!["t.pas".into()]).unwrap();
        let mut waits = 0;
        let status = loop {
            match iso.run(1000) {
                RunOutcome::Finished(st) => break st,
                RunOutcome::BudgetExhausted => continue,
                RunOutcome::Waiting => waits += 1,
                RunOutcome::Fatal(e) => panic!("fatal: {}", e.message),
            }
        };
        assert_eq!(status.kind, ExitKind::Code(0));
        assert_eq!(
            String::from_utf8(stdout.borrow().clone()).unwrap(),
            "6 FALSE\n4 TRUE\n"
        );
        // One wait per line, and one for the end of input read ahead by the last `Eof`
        assert_eq!(waits, 5);
    }
}
