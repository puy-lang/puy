//! Stackless VM (`docs/design/vm.md`, `docs/design/host-interface.md` 3.5, `docs/spec/bytecode.md` chapter 2).
//!
//! - Pascal calls do not recurse on the Rust stack. Frames and registers live on the heap.
//! - Execution budget (decremented by 1 on every backward jump and `Call`).
//! - Rust panics are not converted into Pascal exceptions (§5.10).

mod destroy;
mod exec;
mod format;
pub mod hash;
pub mod heap;
mod input;
mod lend;
pub mod report;
mod sort;
mod strconv;
mod task;
mod text;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::bytecode::{self, ConstEntry, Module, exc, op};
use crate::numfmt;
use crate::sema::types::MAX_STRING_LEN;
use destroy::DTOR_FN;
use heap::{BadRef, Heap, OutOfMemory, PStr, Piece, Trace, try_join};
use puy_host::{HostError, HostErrorKind, HostSet, OutStream};
pub use report::{ExceptionReport, PropValue, StackFrame, leaks_json, render_leaks};
use task::{FRAME_COUNTED_SELF, Frame, PendingHost, PendingKind, Task};

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

/// The registers an 8-bit operand reaches from a frame's base. the registers of the running task
/// ([`Task::regs`]) always have this many from the running frame's base, past the frame's own registers if it has fewer (`frame_end`), so
/// the fast path reads and writes them without a bounds check (`docs/design/vm.md` 3.14).
const REG_WINDOW: usize = 256;

/// The table of shared short strings (`docs/design/vm.md` 3.11): the one-character strings of the
/// ASCII codes, then the decimal text of the integers from 0 below [`SMALL_INTS`].
const SMALL_CHARS: usize = 128;
const SMALL_INTS: i64 = 100;
const SMALL_STRS: usize = SMALL_CHARS + SMALL_INTS as usize;

/// The end of the registers a frame of `regs` registers at `base` needs in [`Task::regs`]: its own
/// registers and its window (see [`REG_WINDOW`]). Every frame the fast path runs is pushed after
/// growing the registers to this; they never shrink, so a caller's window is still there
/// after a return.
fn frame_end(base: usize, regs: usize) -> usize {
    base + regs.max(REG_WINDOW)
}

/// Zeroes the registers a call does not pass (`docs/spec/bytecode.md` 1.4: the registers other
/// than the arguments start at zero). A routine has a few of them, so up to 8 are written with
/// overlapping stores of fixed length instead of a call of `memset` (`docs/design/vm.md` 3.14).
#[inline(always)]
fn zero_slots(s: &mut [Slot]) {
    let n = s.len();
    match n {
        0 => {}
        // Slots 0, n / 2 and n - 1 cover every slot of 1 to 3
        1..=3 => {
            s[0] = 0;
            s[n / 2] = 0;
            s[n - 1] = 0;
        }
        // The first 4 and the last 4 cover every slot of 4 to 8
        4..=8 => {
            s[..4].copy_from_slice(&[0; 4]);
            s[n - 4..].copy_from_slice(&[0; 4]);
        }
        _ => s.fill(0),
    }
}

/// The register window of the frame at `base` (see [`REG_WINDOW`]).
fn reg_window(regs: &mut [Slot], base: usize) -> &mut [Slot; REG_WINDOW] {
    (&mut regs[base..base + REG_WINDOW])
        .try_into()
        .expect("a frame is pushed after growing the registers to `frame_end`")
}

/// The binary real arithmetic and comparisons `AddF64` .. `LeF32` (shape `A B C`; the negations
/// in the range are separate arms, since their `C` is not verified). IEEE 754; a `Single`
/// operation is done in binary32.
fn real_op(opc: u8, x: Slot, y: Slot) -> Slot {
    let (a, b) = (slot_f64(x), slot_f64(y));
    let (fa, fb) = (slot_f32(x), slot_f32(y));
    match opc {
        op::word::ADD_F64 => f64_slot(a + b),
        op::word::SUB_F64 => f64_slot(a - b),
        op::word::MUL_F64 => f64_slot(a * b),
        op::word::DIV_F64 => f64_slot(a / b),
        op::word::EQ_F64 => (a == b) as Slot,
        op::word::LT_F64 => (a < b) as Slot,
        op::word::LE_F64 => (a <= b) as Slot,
        op::word::ADD_F32 => f32_slot(fa + fb),
        op::word::SUB_F32 => f32_slot(fa - fb),
        op::word::MUL_F32 => f32_slot(fa * fb),
        op::word::DIV_F32 => f32_slot(fa / fb),
        op::word::EQ_F32 => (fa == fb) as Slot,
        op::word::LT_F32 => (fa < fb) as Slot,
        op::word::LE_F32 => (fa <= fb) as Slot,
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

/// Hash seed (§8.9, `docs/spec/equality.md` 7.1): the key hash's keys are made from it
/// ([`hash::HashKeys`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashSeed {
    /// Taken from the host's random source (`Random::hash_seed`, one per process) when the isolate
    /// is made.
    FromHost,
    /// A fixed seed (embedding without a random source, or a fixed random seed).
    Fixed(u64),
}

#[derive(Debug, Clone)]
pub struct IsolateConfig {
    /// Memory limit per isolate (§5.12).
    pub memory_limit: Option<usize>,
    /// Recursion limit (total bytes of the running task's registers and frames; 256MB by default).
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
    /// The random source failed to give the hash seed (§8.9: never a weaker substitute).
    Random(String),
    /// `start_main` was called twice.
    AlreadyStarted,
    /// The module does not pass the verification (F0004; `docs/spec/bytecode.md` 3.3), which the
    /// fast path of the execution loop relies on (`docs/design/vm.md` 3.14).
    InvalidModule(bytecode::LoadError),
}

impl std::fmt::Display for IsolateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsolateError::HostFunction(n) => {
                write!(f, "host function `{n}` is not available in this runtime")
            }
            IsolateError::NoRandom => write!(f, "no random source and no hash seed"),
            IsolateError::Random(m) => write!(f, "{m}"),
            IsolateError::AlreadyStarted => write!(f, "isolate already started"),
            IsolateError::InvalidModule(e) => write!(f, "{e}"),
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

/// The position of a frame while unwinding or recording a stack trace
/// (`docs/spec/exceptions.md` 6.3): the current instruction if it is the frame the unwinding
/// started in (`innermost`), otherwise the `Call` (one word before the return address) of a
/// caller whose callee was entered.
fn frame_pos(fr: &Frame, innermost: bool) -> u32 {
    if innermost {
        fr.pc
    } else {
        fr.pc.saturating_sub(1)
    }
}

/// Where the line of a frame of a stack trace comes from (`docs/spec/diagnostics.md` chapter 8).
#[derive(Debug, Clone, Copy)]
enum TraceLine {
    /// The line table of a function (its index) at a word position.
    At(u32, u32),
    /// The line of an inlined call.
    Line(u32),
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
    /// The keys of the key hash, made from the seed (`docs/spec/equality.md` 7.2).
    hash_keys: hash::HashKeys,
    config: IsolateConfig,
    heap: Heap,
    globals: Vec<Slot>,
    /// The running task (`task.rs`; `docs/design/stackless-vm.md` 3.8).
    task: Task,
    args: Vec<String>,
    /// The constant cache (`docs/design/vm.md` 3.11), one entry per constant of the module.
    const_cache: Vec<CachedConst>,
    /// The shared short strings, each made on first use and holding one reference like the
    /// constant cache (0 until made; [`Isolate::small_str`]).
    small_strs: Vec<Slot>,
    /// Buffers reused by `Unlend` (`lend.rs`), so that giving values back does not allocate.
    loan_buf: Vec<heap::Loan>,
    loan_container_buf: Vec<heap::LoanContainer>,
    /// The values of a flat place being moved by `Unlend` (kept empty between uses and reused;
    /// `lend.rs`).
    place_buf: Vec<Slot>,
    host_table: Vec<HostFn>,
    stdout_buf: Vec<u8>,
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

/// Unwraps the result of an instruction body (`exec.rs`), or ends the step with its exception or
/// internal error.
macro_rules! try_op {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return e.into_step(),
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

/// The `EOutOfMemory` of a refused allocation (`docs/spec/semantics.md` 7.1).
fn oom() -> RtError {
    rt(exc::OUT_OF_MEMORY, "out of memory")
}

/// How many scalar values of the value an `EConvertError` message quotes
/// (`docs/spec/semantics.md` 7.1).
const MAX_QUOTED: usize = 100;

/// The `EConvertError` of `text`, which could not be converted: `message` is given `text` cut to
/// its first [`MAX_QUOTED`] scalar values (with `…` added when cut), so that a huge value does not
/// make a huge message; `Value` keeps all of `text`.
fn convert_error(text: String, message: impl FnOnce(&str) -> String) -> RtError {
    let message = match text.char_indices().nth(MAX_QUOTED) {
        Some((end, _)) => message(&format!("{}…", &text[..end])),
        None => message(&text),
    };
    RtError {
        class: exc::CONVERT_ERROR,
        message,
        extra: Extra::Str(text),
    }
}

/// The value of a fallible allocation ([`heap::try_reserve`]), or ends the step with
/// `EOutOfMemory`.
macro_rules! try_oom {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(OutOfMemory) => return Step::Error(oom()),
        }
    };
}

/// Whether `v` is in the range of the `NarrowOvf` / `NarrowRng` width `c`
/// (`docs/spec/bytecode.md` 2.2), that is, whether the check passes and the value stays as it is.
/// Width 6 (Int64) checks UInt64 → Int64 (u64 bits above i64::MAX look negative), width 7 (UInt64)
/// checks signed → UInt64 (negative is out of range): both are decided by the sign bit.
#[inline(always)]
fn narrow_fits(v: i64, c: u32) -> bool {
    // The bits each width keeps, and the signed widths (0, 2, 4) as a bit set. A table instead
    // of a `match` keeps the fast path free of a second jump table
    const BITS: [u32; 9] = [8, 8, 16, 16, 32, 32, 64, 64, 16];
    const SIGNED: u32 = 0b1_0101;
    // The verifier limits the width to 0..=8 (`docs/spec/bytecode.md` 3.3), so the fallback is
    // never taken
    let bits = BITS.get(c as usize).copied().unwrap_or(64);
    if bits == 64 {
        return v >= 0;
    }
    let shift = 64 - bits;
    let back = if (SIGNED >> c) & 1 == 1 {
        (v << shift) >> shift
    } else {
        ((v as u64) << shift >> shift) as i64
    };
    back == v
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
        mut host: HostSet,
        config: IsolateConfig,
    ) -> Result<Isolate, IsolateError> {
        // The fast path reads the instruction stream, the global variables and the constant cache
        // without bounds checks on what the verification guarantees (`docs/design/vm.md` 3.14)
        module.verify().map_err(IsolateError::InvalidModule)?;
        let seed = match config.hash_seed {
            HashSeed::Fixed(s) => s,
            HashSeed::FromHost => match host.random.as_mut() {
                None => return Err(IsolateError::NoRandom),
                Some(r) => r.hash_seed().map_err(|e| IsolateError::Random(e.message))?,
            },
        };
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
        let const_cache = module
            .consts
            .iter()
            .map(|c| CachedConst::new(is_managed_const(c)))
            .collect();
        Ok(Isolate {
            hash_keys: hash::HashKeys::new(seed),
            heap: Heap::new(config.memory_limit, !config.leak_check, &module.types),
            globals,
            task: Task::new(),
            args: Vec::new(),
            const_cache,
            small_strs: vec![0; SMALL_STRS],
            loan_buf: Vec::new(),
            loan_container_buf: Vec::new(),
            place_buf: Vec::new(),
            host_table,
            stdout_buf: Vec::new(),
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

    /// Grows the registers to `need` slots, counting the growth toward the usage limit
    /// (`EOutOfMemory` past it).
    fn grow_regs(&mut self, need: usize) -> Result<(), RtError> {
        if self.task.regs.len() < need {
            let grow = (need - self.task.regs.len()) * std::mem::size_of::<Slot>();
            if self.heap.account(grow).is_err()
                || self
                    .task
                    .regs
                    .try_reserve(need - self.task.regs.len())
                    .is_err()
            {
                return Err(rt(exc::OUT_OF_MEMORY, "out of memory"));
            }
            self.task.regs.resize(need, 0);
        }
        Ok(())
    }

    fn push_frame(&mut self, func: u32, base: u32, ret_reg: u32) {
        let regs = self.module.funcs[func as usize].regs as usize;
        let need = base as usize + regs;
        let end = frame_end(base as usize, regs);
        // Frames pushed here start the program (`<init>`, `<main>`, `<fini>`) at base 0; their
        // registers and window are not counted toward the usage limit
        if self.task.regs.len() < end {
            self.task.regs.resize(end, 0);
        }
        // A frame starts zeroed (`<main>` reuses the registers `<init>` left behind)
        self.task.regs[base as usize..need].fill(0);
        self.task.frames.push(Frame {
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
        if self.task.pending.is_some() {
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
                    let x = match self.exception_of(e) {
                        Ok(x) => x,
                        Err(b) => return self.fatal(b),
                    };
                    if let Some(o) = self.throw_after_releases(x) {
                        return o;
                    }
                }
                Step::Throw(x) => {
                    if let Some(o) = self.throw_after_releases(x) {
                        return o;
                    }
                }
                Step::Halt(from) => {
                    if self.heap.job_open() {
                        // Not made by the instructions that start `Halt`; run before it all the same
                        let pc = self.task.frames.last().map_or(0, |f| f.pc);
                        self.stamp_instr(pc);
                    }
                    if let Some(o) = self.halt(from) {
                        return o;
                    }
                }
                Step::Fatal(m) => {
                    self.flush_stdout().ok();
                    return RunOutcome::Fatal(VmError { message: m });
                }
                Step::Done(o) => return *o,
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
        self.fini_next = unit;
        match self.release_unit_globals(unit) {
            Ok(false) => self.advance_finishing(),
            Ok(true) => None,
            Err(e) => Some(self.fatal(e)),
        }
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
            self.fini_next = unit;
            match self.release_unit_globals(unit) {
                Ok(false) => {}
                // A destructor runs; the stage goes on when the job ends
                Ok(true) => return None,
                Err(e) => return Some(self.fatal(e)),
            }
        }
        Some(self.complete_finishing())
    }

    /// Releases the managed global variables of a unit in reverse declaration order
    /// (`docs/design/lifetime.md` 3.8, `docs/spec/units.md` 5.4). Returns whether that left a
    /// release job to run (a destructor; `docs/design/objects.md` 3.8), after which the finishing
    /// stage goes on.
    fn release_unit_globals(&mut self, unit: usize) -> Result<bool, BadRef> {
        let module = Arc::clone(&self.module);
        let u = &module.units[unit];
        let first = u.first_global as usize;
        for i in (first..first + u.global_count as usize).rev() {
            match module.globals[i].kind {
                bytecode::GLOBAL_REF => {
                    let r = std::mem::take(&mut self.globals[i]);
                    self.heap.release(r)?;
                }
                bytecode::GLOBAL_OBJ => {
                    let r = std::mem::take(&mut self.globals[i]);
                    self.heap.release_obj(r)?;
                }
                _ => {}
            }
        }
        if self.heap.job_open() {
            self.heap.stamp_job(
                heap::JobOrigin::Globals { unit: unit as u32 },
                self.task.frames.len() as u32,
            );
            return Ok(true);
        }
        Ok(false)
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
        // The table of short strings holds one reference of each it made, as the cache does
        for r in std::mem::take(&mut self.small_strs) {
            if let Err(e) = self.heap.release(r) {
                return RunOutcome::Fatal(VmError { message: e.message });
            }
        }
        let leaks = if self.config.leak_check {
            let funcs = &module.funcs;
            count_leaks(self.heap.live_type_names(
                |f| {
                    funcs
                        .get(f as usize)
                        .map_or_else(|| "<built-in>".to_string(), |f| f.name.clone())
                },
                |f, reg| {
                    funcs
                        .get(f as usize)
                        .and_then(|f| f.locals.iter().find(|(r, _)| *r == reg))
                        .map_or_else(|| "<unknown>".to_string(), |(_, n)| n.clone())
                },
            ))
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
        // The message of a VM exception is a short text; if even that is refused, the class
        // alone tells what happened
        let short = |m: &str| PStr::from_text(m).unwrap_or(PStr::Ascii(Vec::new()));
        if force {
            let msg = self.heap.alloc_str_force(short(&e.message));
            let x = self.heap.alloc_exc(e.class, msg, true).expect("forced");
            return Ok(x);
        }
        let msg = self.heap.alloc_str_force(short("out of memory"));
        Ok(self
            .heap
            .alloc_exc(exc::OUT_OF_MEMORY, msg, true)
            .expect("forced allocation does not fail"))
    }

    fn try_exception_of(&mut self, e: &RtError) -> Result<Slot, OutOfMemory> {
        let msg = self.heap.alloc_str(PStr::from_text(&e.message)?)?;
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
            Extra::Str(t) => match PStr::from_text(t).and_then(|p| self.heap.alloc_str(p)) {
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

    /// Raises `x` from the instruction that just failed. If its releases opened a job (an
    /// instance lost its last reference before the failure), the job runs first, holding `x`, and
    /// the unwinding starts when it ends (`docs/design/objects.md` 3.8).
    fn throw_after_releases(&mut self, x: Slot) -> Option<RunOutcome> {
        if !self.heap.job_open() {
            return self.throw(x);
        }
        if let Err(b) = self.record_trace(x) {
            return Some(self.fatal(b));
        }
        self.heap.stamp_job(
            heap::JobOrigin::Unwind { pc: None },
            self.task.frames.len() as u32,
        );
        self.heap.job_record(x);
        None
    }

    /// Raises an exception object, taking its reference (`Raise`; `docs/spec/exceptions.md` 6.2,
    /// 6.3): records the stack trace if the object has none, then unwinds. `None` when a handler
    /// caught it (execution goes on there); otherwise the program ends with the uncaught exception
    /// (7.1).
    fn throw(&mut self, x: Slot) -> Option<RunOutcome> {
        let unwound = self.unwind(x);
        if matches!(unwound, Ok(Unwound::Suspended)) {
            return None;
        }
        // A `ReadLn` the exception left drops the rest of its line (`docs/spec/semantics.md` 6.2)
        let caught_at = matches!(unwound, Ok(Unwound::Caught)).then_some(self.task.frames.len());
        if self.task.readln_left(caught_at) {
            self.input.drop_line();
        }
        match unwound {
            Ok(Unwound::Suspended) => unreachable!("returned above"),
            Ok(Unwound::Caught) => {
                if self.heap.job_open() {
                    // Released on the way (a value given back, …): run at the handler
                    let pc = self.task.frames.last().map_or(0, |f| f.pc);
                    self.stamp_instr(pc);
                }
                None
            }
            Ok(Unwound::Halt(from)) => self.halt(Some(from)),
            Ok(Unwound::Uncaught(report)) => {
                self.frames_done(ExitKind::UncaughtException(Box::new(report)))
            }
            Err(b) => Some(self.fatal(b)),
        }
    }

    /// The unwinding of an exception (`docs/spec/exceptions.md` 6.3).
    fn unwind(&mut self, x: Slot) -> Result<Unwound, BadRef> {
        self.record_trace(x)?;
        let module = Arc::clone(&self.module);
        let mut innermost = true;
        while let Some(&fr) = self.task.frames.last() {
            // The values lent to a call of this frame go back first (`docs/spec/bytecode.md` 2.8)
            self.give_back(false)?;
            if fr.func == DTOR_FN {
                // The destruction procedure records it and goes on (`docs/spec/objects.md` 14.8)
                self.dtor_caught(x)?;
                return Ok(Unwound::Caught);
            }
            let pc = frame_pos(&fr, innermost);
            innermost = false;
            let base = fr.base as usize;
            for h in &module.funcs[fr.func as usize].handlers {
                let r = base + h.reg as usize;
                if h.kind == bytecode::HANDLER_FINALLY && h.in_handler(pc) {
                    let state = std::mem::take(&mut self.task.regs[r]);
                    if state == FINALLY_HALT {
                        // An exception leaving a `finally` block during `Halt`: report it and go on
                        // with `Halt` after the block (5.3)
                        let report = self.build_report(x, 0)?;
                        self.halt_exceptions.push(report);
                        self.heap.release(x)?;
                        return Ok(Unwound::Halt(h.handler_end));
                    }
                    if state == FINALLY_EXCEPTION {
                        let y = std::mem::take(&mut self.task.regs[r + 1]);
                        self.attach_suppressed(x, y)?;
                    }
                }
                if h.kind == bytecode::HANDLER_EXCEPT && h.in_handler(pc) {
                    // Leaving an `except` block (5.2)
                    let old = std::mem::take(&mut self.task.regs[r]);
                    self.heap.release(old)?;
                }
                if h.in_body(pc) {
                    if h.kind == bytecode::HANDLER_EXCEPT {
                        self.put(r, x)?;
                    } else {
                        self.task.regs[r] = FINALLY_EXCEPTION;
                        self.put(r + 1, x)?;
                    }
                    self.task
                        .frames
                        .last_mut()
                        .expect("the frame being unwound")
                        .pc = h.handler_start;
                    return Ok(Unwound::Caught);
                }
            }
            self.release_frame(&fr)?;
            self.task.frames.pop();
            if self.heap.job_open() {
                // The destructors run before the unwinding goes on, from the caller's `Call`
                self.heap.stamp_job(
                    heap::JobOrigin::Unwind {
                        pc: self.task.frames.last().map(|c| frame_pos(c, false)),
                    },
                    self.task.frames.len() as u32,
                );
                self.heap.job_record(x);
                return Ok(Unwound::Suspended);
            }
        }
        let report = self.build_report(x, 0)?;
        self.heap.release(x)?;
        Ok(Unwound::Uncaught(report))
    }

    /// `ExceptObject` (`docs/spec/objects.md` 8.4): the exception of the innermost `except` block
    /// being run, from the top frame down (each frame's innermost entry first; the handler table
    /// lists inner entries first), or 0 outside every `except` block.
    fn except_object(&self) -> Slot {
        let last = self.task.frames.len().saturating_sub(1);
        for (i, fr) in self.task.frames.iter().enumerate().rev() {
            if fr.func == DTOR_FN {
                continue;
            }
            let pc = frame_pos(fr, i == last);
            let handler = self.module.funcs[fr.func as usize]
                .handlers
                .iter()
                .find(|h| h.kind == bytecode::HANDLER_EXCEPT && h.in_handler(pc));
            if let Some(h) = handler {
                return self.task.regs[fr.base as usize + h.reg as usize];
            }
        }
        0
    }

    /// The value `Raise` / `RaiseOuter` raise (`docs/spec/objects.md` 14.8): `nil` and a freed
    /// instance are `EAccessViolation` (the check of 14.4); an instance that is not an exception is
    /// only made by forged bytecode (the compiler checks the static type; E3021).
    fn check_raised(&self, x: Slot) -> Result<(), Step> {
        match self.heap.obj_class(x) {
            Ok(ty) if self.heap.class_is(ty, exc::EXCEPTION) => Ok(()),
            Ok(_) => Err(Step::Fatal(format!(
                "`Raise` of {x}, which is not an exception"
            ))),
            Err(e) => Err(RefError::from(e).into_step()),
        }
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
            while let Some(&fr) = self.task.frames.last() {
                if self
                    .heap
                    .top_job()
                    .is_some_and(|(_, d)| d as usize == self.task.frames.len())
                {
                    // A release job of this frame (left by a destruction `Halt` cut short, or by
                    // a dropped frame) runs first; `Halt` goes on from here when it ends
                    if fr.func != DTOR_FN {
                        let pos = from.take().unwrap_or_else(|| frame_pos(&fr, innermost));
                        self.task.frames.last_mut().expect("the frame").pc = pos;
                    }
                    self.halt_job()?;
                    return Ok(true);
                }
                self.give_back(false)?;
                if fr.func == DTOR_FN {
                    // The rest of the destruction without the destructors (`docs/spec/objects.md`
                    // 8.5)
                    self.dtor_abandon()?;
                    innermost = false;
                    continue;
                }
                let pc = from.take().unwrap_or_else(|| frame_pos(&fr, innermost));
                innermost = false;
                let base = fr.base as usize;
                for h in &module.funcs[fr.func as usize].handlers {
                    let r = base + h.reg as usize;
                    if h.kind == bytecode::HANDLER_FINALLY && h.in_handler(pc) {
                        let state = std::mem::take(&mut self.task.regs[r]);
                        if state == FINALLY_EXCEPTION {
                            // The exception being propagated is dropped (not reported)
                            let y = std::mem::take(&mut self.task.regs[r + 1]);
                            self.heap.release(y)?;
                        }
                    }
                    if h.kind == bytecode::HANDLER_EXCEPT && h.in_handler(pc) {
                        let old = std::mem::take(&mut self.task.regs[r]);
                        self.heap.release(old)?;
                    }
                    if h.kind == bytecode::HANDLER_FINALLY && h.in_body(pc) {
                        self.task.regs[r] = FINALLY_HALT;
                        let old = std::mem::take(&mut self.task.regs[r + 1]);
                        self.heap.release(old)?;
                        self.task
                            .frames
                            .last_mut()
                            .expect("the frame being unwound")
                            .pc = h.handler_start;
                        return Ok(true);
                    }
                }
                self.release_frame(&fr)?;
                self.task.frames.pop();
                if self.heap.job_open() {
                    // The destructors run before `Halt` goes on (from the caller's `Call`)
                    if let Some(c) = self.task.frames.last_mut() {
                        c.pc = frame_pos(c, false);
                    }
                    self.heap
                        .stamp_job(heap::JobOrigin::Halt, self.task.frames.len() as u32);
                    return Ok(true);
                }
            }
            Ok(false)
        })();
        match result {
            Ok(true) => None,
            Ok(false) => self.frames_done(ExitKind::Code(self.exit_code_byte())),
            Err(b) => Some(self.fatal(b)),
        }
    }

    /// Makes the last job, which belongs to the top frame, go on with `Halt` when it ends. A
    /// result it was to deliver is let go of (`Halt` drops the frame it was for).
    fn halt_job(&mut self) -> Result<(), BadRef> {
        let Some((origin, depth)) = self.heap.top_job() else {
            return Ok(());
        };
        if let heap::JobOrigin::Ret { result, kind, .. } = origin {
            let v = std::mem::take(&mut self.task.regs[result as usize]);
            match kind {
                bytecode::MANAGED_HEAP_REF => self.heap.release(v)?,
                bytecode::MANAGED_OBJ => self.heap.release_obj(v)?,
                _ => {}
            }
        }
        self.heap.restamp_job(heap::JobOrigin::Halt, depth);
        Ok(())
    }

    /// Records the current frames as the stack trace of an exception that does not have one yet
    /// (`docs/spec/exceptions.md` 5.1).
    fn record_trace(&mut self, x: Slot) -> Result<(), BadRef> {
        if self.heap.exc_trace(x)?.is_some() {
            return Ok(());
        }
        // The frames of the destruction procedure are not shown (`docs/spec/objects.md` 3.3).
        // A frame running inlined code stands for the frames of the calls inlined there
        // (`docs/spec/diagnostics.md` chapter 8): (function shown, where its line comes from)
        let last = self.task.frames.len().saturating_sub(1);
        let mut visible: Vec<(u32, TraceLine)> = Vec::with_capacity(self.task.frames.len());
        for (i, f) in self.task.frames.iter().enumerate() {
            if f.func == DTOR_FN {
                continue;
            }
            let pos = frame_pos(f, i == last);
            let func = &self.module.funcs[f.func as usize];
            let chain = func.inline_chain(pos);
            // Outermost first: the function itself at the line of the outermost call, then each
            // inlined callee at the line of the call inside it, the innermost at `pos`
            let mut shown = f.func;
            for &k in chain.iter().rev() {
                let e = &func.inlines[k];
                visible.push((shown, TraceLine::Line(e.line)));
                shown = e.func;
            }
            visible.push((shown, TraceLine::At(f.func, pos)));
        }
        let n = visible.len();
        let kept: Vec<(u32, TraceLine)> = if n > 32 {
            let mut v: Vec<_> = visible.iter().rev().take(16).copied().collect();
            v.extend(visible[..16].iter().rev().copied());
            v
        } else {
            visible.into_iter().rev().collect()
        };
        let omitted = n.saturating_sub(32);
        let frames = kept
            .into_iter()
            .map(|(func, line)| {
                let line = match line {
                    TraceLine::Line(l) => l,
                    TraceLine::At(f, pos) => self.module.funcs[f as usize].line_at(pos),
                };
                (func, line)
            })
            .collect();
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
            .map(|&(func, line)| {
                let f = &self.module.funcs[func as usize];
                StackFrame {
                    function: f.name.clone(),
                    file: self
                        .module
                        .units
                        .get(f.file as usize)
                        .map(|u| u.path.clone())
                        .unwrap_or_default(),
                    line,
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
        Ok(class_decl_name(self.heap.type_name(self.heap.exc_type(x)?)).to_string())
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
            .text();
        let (stack, omitted) = self.trace_frames(x)?;
        let mut properties = Vec::new();
        let extra = self
            .heap
            .exc_slot(x, bytecode::EXC_SLOT_EXTRA)?
            .unwrap_or(0);
        if self.heap.class_is(ty, exc::CONVERT_ERROR) {
            properties.push((
                "Value".to_string(),
                PropValue::Str(self.heap.get(extra)?.text()),
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
        while let Some(&fr) = self.task.frames.last() {
            self.give_back(false)?;
            if fr.func == DTOR_FN {
                self.dtor_abandon()?;
                continue;
            }
            self.release_frame(&fr)?;
            self.task.frames.pop();
        }
        Ok(())
    }

    /// Releases what a frame being left holds: its managed registers in list order, then a
    /// counted `Self` in `r0` that `CallClosure` took (`docs/spec/objects.md` 14.9). Each is set
    /// to 0.
    fn release_frame(&mut self, fr: &Frame) -> Result<(), BadRef> {
        let base = fr.base as usize;
        for m in &self.module.funcs[fr.func as usize].managed {
            release_slot(
                &mut self.heap,
                &mut self.task.regs[base + m.reg as usize],
                m,
            )?;
        }
        if fr.counted_self() {
            let s = std::mem::take(&mut self.task.regs[base]);
            self.heap.release(s)?;
        }
        Ok(())
    }

    /// Writes a value that holds its own reference into a register, letting go of the register's old value
    /// (every instruction that makes a managed value; `docs/design/lifetime.md` 3.1).
    fn put(&mut self, i: usize, v: Slot) -> Result<(), BadRef> {
        let old = std::mem::replace(&mut self.task.regs[i], v);
        self.heap.release(old)
    }

    /// Writes an existing value into a register: counts it, then lets go of the old value.
    fn put_shared(&mut self, i: usize, v: Slot) -> Result<(), BadRef> {
        self.heap.add_ref(v)?;
        self.put(i, v)
    }

    // ---- Standard output buffer ----

    /// Appends `bytes` to the standard output buffer and writes it out as `stdout_buffering` says.
    /// The copy into the buffer is a copy an instruction makes of a Pascal value, so a refusal of
    /// the system is `EOutOfMemory` (`docs/spec/semantics.md` 7.1); a failed write is the
    /// exception of the host error.
    fn write_stdout(&mut self, bytes: &[u8]) -> Result<(), RtError> {
        heap::try_extend(&mut self.stdout_buf, bytes).map_err(|OutOfMemory| oom())?;
        let flush = match self.config.stdout_buffering {
            StdoutBuffering::None => true,
            StdoutBuffering::Line => bytes.contains(&b'\n'),
            StdoutBuffering::Full => self.stdout_buf.len() >= FULL_BUFFER,
        };
        if flush {
            self.flush_stdout().map_err(host_error_to_rt)?;
        }
        Ok(())
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
        let Some(p) = self.task.pending.as_mut() else {
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
        self.task.pending = None;
        match result {
            Ok(None) => {}
            // The line goes into the buffer; the input instruction runs again (`input.rs`)
            Ok(Some((line, ahead))) => self.input.receive(line, ahead)?,
            Err(e) => return Err(PollError::Rt(host_error_to_rt(e))),
        }
        if let Some(fr) = self.task.frames.last_mut() {
            fr.pc = resume_pc;
        }
        Ok(true)
    }

    /// The shared string at `i` of the table of short strings, with a reference of its own: below
    /// [`SMALL_CHARS`] the one-character ASCII string of the code `i`, then the decimal text of
    /// `i - SMALL_CHARS`. Made on first use; the table keeps one reference until the end of the
    /// run, as the constant cache does (`docs/design/vm.md` 3.11).
    fn small_str(&mut self, i: usize) -> Result<Slot, RefError> {
        let v = self.small_strs[i];
        if v == 0 {
            let mut buf = [0u8; 21];
            let text: &[u8] = if i < SMALL_CHARS {
                buf[0] = i as u8;
                &buf[..1]
            } else {
                strconv::decimal(false, (i - SMALL_CHARS) as u64, &mut buf)
            };
            let s = PStr::ascii(text).map_err(|_| RefError::Rt(oom()))?;
            let v = self.alloc_string(s)?;
            self.small_strs[i] = v;
        }
        let v = self.small_strs[i];
        self.heap.add_ref(v)?;
        Ok(v)
    }

    fn alloc_string(&mut self, s: PStr) -> Result<Slot, RtError> {
        self.heap.alloc_str(s).map_err(|_| oom())
    }

    fn alloc_u8(&mut self, b: Vec<u8>) -> Result<Slot, RtError> {
        self.heap.alloc_u8(b).map_err(|_| oom())
    }

    /// The number of bits of a big set block's type.
    fn set_bits(&self, set: Slot) -> Result<i64, String> {
        let ty = self.heap.agg_type(set).map_err(|e| e.message)?;
        Ok(self.module.type_entry(ty).map_or(0, |t| t.slots as i64))
    }

    /// The value of a constant, made on first use and kept in the constant cache, which holds one
    /// reference of a managed value (`docs/design/vm.md` 3.11).
    fn load_const(&mut self, k: usize) -> Result<Slot, PollError> {
        if let Some(v) = self.const_cache[k].get() {
            return Ok(v);
        }
        let module = Arc::clone(&self.module);
        let v = match &module.consts[k] {
            ConstEntry::Int(i) => *i,
            ConstEntry::Float(b) => *b as i64,
            ConstEntry::Str(s) => {
                let p = PStr::from_text(s).map_err(|_| oom())?;
                self.alloc_string(p)?
            }
            ConstEntry::Utf16(u) => {
                let p = PStr::from_units(u).map_err(|_| oom())?;
                self.alloc_string(p)?
            }
            ConstEntry::U8(s) => {
                let b = try_join(&[s.as_bytes()]).map_err(|_| oom())?;
                self.alloc_u8(b)?
            }
            ConstEntry::Agg { ty, items } => {
                // The elements come before the aggregate (`docs/spec/bytecode.md` 3.3); the block
                // holds a reference of each managed element besides the cache's
                let mut slots = Vec::with_capacity(items.len());
                for &item in items {
                    let v = self.load_const(item as usize)?;
                    if self.const_cache[item as usize].managed() {
                        self.heap.add_ref(v).map_err(PollError::Fatal)?;
                    }
                    slots.push(v);
                }
                match self.heap.alloc_agg(*ty, slots.clone()) {
                    Ok(r) => r,
                    Err(_) => {
                        for (i, &item) in items.iter().enumerate() {
                            if self.const_cache[item as usize].managed() {
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
        self.const_cache[k].set(v);
        Ok(v)
    }

    // ---- Execution loop ----

    /// Fast path of the execution loop (`docs/design/vm.md` 3.14). Register-only instructions
    /// (moves, integer and real arithmetic, comparisons, branches, globals, constants, division),
    /// `Call` and `Ret`, and the managed-value instructions of `exec_op` keep looping inside this
    /// function without writing back the frame's `pc`. An instruction that fails writes back `pc`
    /// and returns its exception to `run`. Other instructions (host functions, exceptions, the
    /// string built-ins, wide forms) write back `pc` and are handed to `step`; since the frame may
    /// change, the function's information is reloaded. Returns `Step::Continue` when the
    /// budget runs out, or after a step of the destruction procedure (`destroy.rs`; `run` calls
    /// it again).
    fn run_loop(&mut self, budget: &mut u32) -> Step {
        let module = Arc::clone(&self.module);
        loop {
            if self.heap.has_jobs() {
                // Release jobs of the current frames first (`docs/design/objects.md` 3.8)
                match self.run_jobs() {
                    Step::Continue => {}
                    other => return other,
                }
            }
            let Some(frame) = self.task.frames.last().copied() else {
                return self.step(budget);
            };
            if frame.func == DTOR_FN {
                // The destruction procedure (`destroy.rs`), out of the hot loop: `run` calls
                // `run_loop` again after it
                return self.dtor_run(budget);
            }
            // The running function's information. A call or a return on the fast path replaces it
            // and goes on in the inner loop (`enter!`)
            let mut func = &module.funcs[frame.func as usize];
            let mut code: &[u32] = &func.code;
            let mut base = frame.base as usize;
            let mut pc = frame.pc as usize;
            // Instruction not on the fast path: write back pc and leave it to step
            macro_rules! slow {
                () => {{
                    match self.slow_at(pc, budget) {
                        Some(s) => return s,
                        None => break,
                    }
                }};
            }
            // An instruction body shared with step (`exec.rs`): on a failure, write back pc (the
            // exception is raised at this instruction) and leave it to `run`
            macro_rules! op {
                ($e:expr) => {
                    match $e {
                        Ok(v) => v,
                        Err(e) => return self.fail_at(pc, e),
                    }
                };
            }
            // The frame's registers as seen by an 8-bit operand: no bounds check on each access.
            // Taken again after anything that may grow `self.task.regs`
            let mut regs = reg_window(&mut self.task.regs, base);
            // Goes on in the function `$f` of the frame at `$base` from `$pc`, without leaving the
            // inner loop (the outer loop would read the same from the top frame). Only after a
            // call or a return that opens no release job and leaves no job to run in the frame
            // it goes to, and not into the destruction procedure (`DTOR_FN`)
            macro_rules! enter {
                ($f:expr, $base:expr, $pc:expr) => {{
                    func = $f;
                    code = &func.code;
                    base = $base;
                    pc = $pc;
                    regs = reg_window(&mut self.task.regs, base);
                    continue;
                }};
            }
            loop {
                debug_assert!(pc < code.len(), "pc {pc} past the instruction stream");
                // SAFETY: `pc` is an instruction boundary of the function, whose module was
                // verified (`Isolate::new`; `docs/spec/bytecode.md` 3.3): a frame starts at 0 (the
                // stream is not empty), and `pc` is set only to a verified jump target or handler,
                // to the boundary `step` or a return writes back, or to the word after an
                // instruction that is not the last one (the last one is `Ret`, `RetVoid`, `Halt`,
                // `Jmp` or `Raise`, none of which goes on to the next word). `Wide` and `Ext` go to
                // step below, so the fast path never stops inside an instruction with a prefix
                let word = unsafe { *code.get_unchecked(pc) };
                // Every instruction passes here once, also those left to `step`
                #[cfg(test)]
                executed::add_one();
                let opc = bytecode::word_op(word);
                let next = pc + 1;
                // The operands (`docs/spec/bytecode.md` 1.2), each read only in the arms that use
                // it, so that the dispatch does not decode the fields an instruction lacks
                macro_rules! a {
                    () => {
                        bytecode::word_a(word) as usize
                    };
                }
                macro_rules! b {
                    () => {
                        bytecode::word_b(word) as usize
                    };
                }
                macro_rules! c {
                    () => {
                        bytecode::word_c(word) as usize
                    };
                }
                macro_rules! bx {
                    () => {
                        bytecode::decode_bx(word, None) as usize
                    };
                }
                macro_rules! sbx {
                    () => {
                        bytecode::signed_bx(bytecode::decode_bx(word, None), false)
                    };
                }
                macro_rules! sb {
                    () => {
                        bytecode::signed_b(bytecode::word_b(word), false)
                    };
                }
                macro_rules! sc {
                    () => {
                        bytecode::signed_c(bytecode::word_c(word), false)
                    };
                }
                // The operands as indexes of `self.task.regs`, for the bodies shared with step
                macro_rules! abs {
                    () => {
                        (base + a!(), base + b!(), base + c!())
                    };
                }
                // The binary real arithmetic and comparisons (`real_op`)
                macro_rules! real {
                    ($op:expr) => {
                        regs[a!()] = real_op($op, regs[b!()], regs[c!()])
                    };
                }
                // A branch: when `$taken`, go to `next + $off`; a backward jump decrements the
                // budget (3.9), and when it runs out, the jump is done and then `run_loop` returns
                macro_rules! branch {
                    ($taken:expr, $off:expr) => {{
                        if $taken {
                            let off: i64 = $off;
                            pc = (next as i64 + off) as usize;
                            if off < 0 {
                                *budget = budget.saturating_sub(1);
                                if *budget == 0 {
                                    self.task.frames.last_mut().unwrap().pc = pc as u32;
                                    return Step::Continue;
                                }
                            }
                            continue;
                        }
                    }};
                }
                // An instruction body shared with step (`exec_op`), or step itself
                macro_rules! shared {
                    () => {{
                        // `bx!()` is not the type word of `RefFieldSet` / `RefFieldSetRef`
                        // (`docs/spec/bytecode.md` 1.2): their arms go to step instead. `AggLoad`,
                        // `AggStore` and `LendPlace` come here, but `exec_op` does not run them, so
                        // they go to step too (an arm of their own here slowed the loop down by
                        // moving its code; `docs/notes/benchmark-m4c.md`, M4-63, M4-64)
                        debug_assert!(
                            !bytecode::has_type_word(u16::from(opc))
                                || matches!(
                                    opc,
                                    op::word::AGG_LOAD | op::word::AGG_STORE | op::word::LEND_PLACE
                                )
                        );
                        let (ra, rb, rc) = abs!();
                        match self.exec_op(opc, ra, rb, rc, b!(), c!(), bx!()) {
                            Some(r) => {
                                op!(r);
                                if self.heap.job_open() {
                                    // The instruction let go of an instance's last reference: its
                                    // destruction runs before the next instruction
                                    self.stamp_instr(pc as u32);
                                    self.task.frames.last_mut().unwrap().pc = next as u32;
                                    break;
                                }
                            }
                            None => slow!(),
                        }
                        regs = reg_window(&mut self.task.regs, base);
                    }};
                }
                // Enters `$callee` with its frame at `r$args` of this one (the arguments are there);
                // the result comes back in `rA` (a routine's arguments start after it). A call that
                // needs more registers or passes the stack limit is left to step
                macro_rules! call {
                    ($callee:expr, $args:expr) => {{
                        let callee: usize = $callee;
                        let f = &module.funcs[callee];
                        let new_base = base + $args;
                        let need = new_base + f.regs as usize;
                        let bytes = need * std::mem::size_of::<Slot>()
                            + (self.task.frames.len() + 1) * std::mem::size_of::<Frame>();
                        if bytes > self.config.stack_limit
                            || self.task.regs.len() < frame_end(new_base, f.regs as usize)
                        {
                            slow!();
                        }
                        *budget = budget.saturating_sub(1);
                        zero_slots(&mut self.task.regs[new_base + f.params as usize..need]);
                        self.task.frames.last_mut().unwrap().pc = next as u32;
                        self.task.frames.push(Frame {
                            func: callee as u32,
                            base: new_base as u32,
                            pc: 0,
                            ret_reg: (base + a!()) as u32,
                        });
                        if *budget == 0 {
                            return Step::Continue;
                        }
                        // Entering a frame opens no job, and a job waiting below runs only when the
                        // frames come back down to it
                        enter!(f, new_base, 0)
                    }};
                }
                match opc {
                    op::word::NOP => {}
                    // A wide instruction is left to step (`docs/design/vm.md` 3.14). So is one
                    // numbered past 255 (`docs/spec/bytecode.md` 1.2): `Ext` goes through the last
                    // arm, as `exec_op` does not run it (an arm of its own here moved the code of
                    // the loop and slowed it down; `docs/notes/benchmark-m4c.md`, M4-109)
                    op::word::WIDE => slow!(),
                    op::word::MOVE => regs[a!()] = regs[b!()],
                    op::word::LOAD_ZERO => regs[a!()] = 0,
                    op::word::LOAD_INT => regs[a!()] = sbx!(),
                    // SAFETY: `Bx` of `LoadConst` is in the constant pool (verified;
                    // `docs/spec/bytecode.md` 3.3), and the cache has an entry for each constant
                    op::word::LOAD_CONST => {
                        match unsafe { self.const_cache.get_unchecked(bx!()) }.plain() {
                            Some(v) => regs[a!()] = v,
                            // A managed constant goes to `exec_op`, one not made yet to step (a
                            // constant holds no instance: no release job)
                            _ => {
                                let (ra, rb, rc) = abs!();
                                match self.exec_op(opc, ra, rb, rc, b!(), c!(), bx!()) {
                                    Some(r) => op!(r),
                                    None => slow!(),
                                }
                                regs = reg_window(&mut self.task.regs, base);
                            }
                        }
                    }
                    // SAFETY: `Bx` of `LoadGlobal` / `StoreGlobal` is in the global variable table
                    // (verified; `docs/spec/bytecode.md` 3.3), and `self.globals` has one slot for
                    // each entry and never shrinks
                    op::word::LOAD_GLOBAL => {
                        regs[a!()] = unsafe { *self.globals.get_unchecked(bx!()) }
                    }
                    op::word::STORE_GLOBAL => unsafe {
                        *self.globals.get_unchecked_mut(bx!()) = regs[a!()]
                    },
                    op::word::ADD_INT => regs[a!()] = regs[b!()].wrapping_add(regs[c!()]),
                    op::word::SUB_INT => regs[a!()] = regs[b!()].wrapping_sub(regs[c!()]),
                    op::word::MUL_INT => regs[a!()] = regs[b!()].wrapping_mul(regs[c!()]),
                    op::word::ADD_INT32 => {
                        regs[a!()] = exec::int32(op::word::ADD_INT32, regs[b!()], regs[c!()])
                    }
                    op::word::SUB_INT32 => {
                        regs[a!()] = exec::int32(op::word::SUB_INT32, regs[b!()], regs[c!()])
                    }
                    op::word::MUL_INT32 => {
                        regs[a!()] = exec::int32(op::word::MUL_INT32, regs[b!()], regs[c!()])
                    }
                    op::word::NEG_INT => regs[a!()] = regs[b!()].wrapping_neg(),
                    op::word::AND_INT => regs[a!()] = regs[b!()] & regs[c!()],
                    op::word::OR_INT => regs[a!()] = regs[b!()] | regs[c!()],
                    op::word::XOR_INT => regs[a!()] = regs[b!()] ^ regs[c!()],
                    op::word::NOT_INT => regs[a!()] = !regs[b!()],
                    op::word::SHL_INT => {
                        regs[a!()] = regs[b!()].wrapping_shl((regs[c!()] & 63) as u32)
                    }
                    op::word::SHR_INT => {
                        regs[a!()] =
                            ((regs[b!()] as u64).wrapping_shr((regs[c!()] & 63) as u32)) as i64
                    }
                    op::word::SHL_INT32 => {
                        regs[a!()] =
                            ((regs[b!()] as i32).wrapping_shl((regs[c!()] & 31) as u32)) as i64
                    }
                    op::word::SHR_INT32 => {
                        regs[a!()] = (((regs[b!()] as u32).wrapping_shr((regs[c!()] & 31) as u32))
                            as i32) as i64
                    }
                    op::word::ADD_INT_I => regs[a!()] = regs[b!()].wrapping_add(sc!()),
                    op::word::MUL_INT_I => {
                        regs[a!()] = exec::int_imm(op::word::MUL_INT_I, regs[b!()], sc!())
                    }
                    op::word::ADD_INT32_I => {
                        regs[a!()] = exec::int_imm(op::word::ADD_INT32_I, regs[b!()], sc!())
                    }
                    op::word::MUL_INT32_I => {
                        regs[a!()] = exec::int_imm(op::word::MUL_INT32_I, regs[b!()], sc!())
                    }
                    op::word::AND_INT_I => {
                        regs[a!()] = exec::int_imm(op::word::AND_INT_I, regs[b!()], sc!())
                    }
                    op::word::OR_INT_I => {
                        regs[a!()] = exec::int_imm(op::word::OR_INT_I, regs[b!()], sc!())
                    }
                    op::word::XOR_INT_I => {
                        regs[a!()] = exec::int_imm(op::word::XOR_INT_I, regs[b!()], sc!())
                    }
                    op::word::SHL_INT_I => {
                        regs[a!()] = exec::int_imm(op::word::SHL_INT_I, regs[b!()], sc!())
                    }
                    op::word::SHR_INT_I => {
                        regs[a!()] = exec::int_imm(op::word::SHR_INT_I, regs[b!()], sc!())
                    }
                    op::word::SHL_INT32_I => {
                        regs[a!()] = exec::int_imm(op::word::SHL_INT32_I, regs[b!()], sc!())
                    }
                    op::word::SHR_INT32_I => {
                        regs[a!()] = exec::int_imm(op::word::SHR_INT32_I, regs[b!()], sc!())
                    }
                    op::word::DIV_INT_I | op::word::MOD_INT_I => {
                        let div = if opc == op::word::DIV_INT_I {
                            op::word::DIV_INT
                        } else {
                            op::word::MOD_INT
                        };
                        match exec::int_div(div, regs[b!()], sc!()) {
                            Ok(v) => regs[a!()] = v,
                            Err(e) => op!(Err(RefError::Rt(e))),
                        }
                    }
                    op::word::NEG_F64 => regs[a!()] = f64_slot(-slot_f64(regs[b!()])),
                    op::word::NEG_F32 => regs[a!()] = f32_slot(-slot_f32(regs[b!()])),
                    // One arm each, not a range pattern: a range splits the dispatch in two (the
                    // arms after it get a second table)
                    op::word::ADD_F64 => real!(op::word::ADD_F64),
                    op::word::SUB_F64 => real!(op::word::SUB_F64),
                    op::word::MUL_F64 => real!(op::word::MUL_F64),
                    op::word::DIV_F64 => real!(op::word::DIV_F64),
                    op::word::EQ_F64 => real!(op::word::EQ_F64),
                    op::word::LT_F64 => real!(op::word::LT_F64),
                    op::word::LE_F64 => real!(op::word::LE_F64),
                    op::word::ADD_F32 => real!(op::word::ADD_F32),
                    op::word::SUB_F32 => real!(op::word::SUB_F32),
                    op::word::MUL_F32 => real!(op::word::MUL_F32),
                    op::word::DIV_F32 => real!(op::word::DIV_F32),
                    op::word::EQ_F32 => real!(op::word::EQ_F32),
                    op::word::LT_F32 => real!(op::word::LT_F32),
                    op::word::LE_F32 => real!(op::word::LE_F32),
                    op::word::NARROW => regs[a!()] = narrow(regs[b!()], c!() as u32),
                    // An out-of-range value goes to step, which raises the exception
                    op::word::NARROW_OVF | op::word::NARROW_RNG => {
                        let v = regs[b!()];
                        if !narrow_fits(v, c!() as u32) {
                            slow!();
                        }
                        regs[a!()] = v;
                    }
                    op::word::DIV_INT
                    | op::word::DIV_INT_CHK
                    | op::word::MOD_INT
                    | op::word::DIV_UINT
                    | op::word::MOD_UINT => match exec::int_div(opc, regs[b!()], regs[c!()]) {
                        Ok(v) => regs[a!()] = v,
                        Err(e) => op!(Err(RefError::Rt(e))),
                    },
                    op::word::EQ_INT => regs[a!()] = (regs[b!()] == regs[c!()]) as i64,
                    op::word::LT_INT => regs[a!()] = (regs[b!()] < regs[c!()]) as i64,
                    op::word::LE_INT => regs[a!()] = (regs[b!()] <= regs[c!()]) as i64,
                    op::word::LT_UINT => {
                        regs[a!()] = ((regs[b!()] as u64) < (regs[c!()] as u64)) as i64
                    }
                    op::word::LE_UINT => {
                        regs[a!()] = ((regs[b!()] as u64) <= (regs[c!()] as u64)) as i64
                    }
                    op::word::NOT_BOOL => regs[a!()] = regs[b!()] ^ 1,
                    // Each branch has an arm of its own, so the dispatch jumps straight to it
                    op::word::JMP => branch!(true, sbx!()),
                    op::word::JMP_IF_TRUE => branch!(regs[a!()] != 0, sbx!()),
                    op::word::JMP_IF_FALSE => branch!(regs[a!()] == 0, sbx!()),
                    op::word::JMP_IF_EQ_INT => branch!(regs[a!()] == regs[b!()], sc!()),
                    op::word::JMP_IF_NE_INT => branch!(regs[a!()] != regs[b!()], sc!()),
                    op::word::JMP_IF_LT_INT => branch!(regs[a!()] < regs[b!()], sc!()),
                    op::word::JMP_IF_LE_INT => branch!(regs[a!()] <= regs[b!()], sc!()),
                    op::word::JMP_IF_LT_UINT => {
                        branch!((regs[a!()] as u64) < (regs[b!()] as u64), sc!())
                    }
                    op::word::JMP_IF_LE_UINT => {
                        branch!((regs[a!()] as u64) <= (regs[b!()] as u64), sc!())
                    }
                    op::word::JMP_IF_EQ_INT_I => {
                        branch!(
                            exec::cmp_imm(op::word::JMP_IF_EQ_INT_I, regs[a!()], sb!()),
                            sc!()
                        )
                    }
                    op::word::JMP_IF_NE_INT_I => {
                        branch!(
                            exec::cmp_imm(op::word::JMP_IF_NE_INT_I, regs[a!()], sb!()),
                            sc!()
                        )
                    }
                    op::word::JMP_IF_LT_INT_I => {
                        branch!(
                            exec::cmp_imm(op::word::JMP_IF_LT_INT_I, regs[a!()], sb!()),
                            sc!()
                        )
                    }
                    op::word::JMP_IF_LE_INT_I => {
                        branch!(
                            exec::cmp_imm(op::word::JMP_IF_LE_INT_I, regs[a!()], sb!()),
                            sc!()
                        )
                    }
                    op::word::JMP_IF_GT_INT_I => {
                        branch!(
                            exec::cmp_imm(op::word::JMP_IF_GT_INT_I, regs[a!()], sb!()),
                            sc!()
                        )
                    }
                    op::word::JMP_IF_GE_INT_I => {
                        branch!(
                            exec::cmp_imm(op::word::JMP_IF_GE_INT_I, regs[a!()], sb!()),
                            sc!()
                        )
                    }
                    op::word::FOR_LOOP_UP | op::word::FOR_LOOP_DOWN => {
                        let a = a!();
                        let more = match exec::for_step(opc, regs[a], regs[b!()]) {
                            Some(v) => {
                                regs[a] = v;
                                true
                            }
                            None => false,
                        };
                        branch!(more, sc!())
                    }
                    op::word::JMP_IF_EQ_STR | op::word::JMP_IF_NE_STR => {
                        let (x, y) = (regs[a!()], regs[b!()]);
                        let eq = op!(self.str_equal(x, y));
                        regs = reg_window(&mut self.task.regs, base);
                        branch!(eq == (opc == op::word::JMP_IF_EQ_STR), sc!())
                    }
                    // Same as `Call` in `step`. Growing the registers and exceeding the limit
                    // (exception) are left to step (`call!`). A method call whose `Self` does not
                    // check, or whose table entry is not a function, is also left to step
                    // (`docs/spec/objects.md` 14.5), and so are the calls of a callable value that
                    // needs a `Self` (14.9)
                    op::word::CALL => call!(bx!(), a!()),
                    op::word::CALL_METHOD | op::word::CALL_VIRTUAL | op::word::CALL_INTF => {
                        match fast_method_callee(&self.heap, &module, opc, regs[a!()], bx!()) {
                            Some(f)
                                if a!() + module.funcs[f].params as usize <= func.regs as usize =>
                            {
                                call!(f, a!())
                            }
                            _ => slow!(),
                        }
                    }
                    op::word::CALL_INDIRECT | op::word::CALL_CLOSURE => {
                        // `closure`: `rA` takes the closure block `rB` as the hidden first argument
                        let Some((f, closure)) =
                            fast_value_callee(&self.heap, &module, opc, regs[b!()])
                        else {
                            slow!()
                        };
                        // A routine's arguments start after `rA`, which takes the result
                        let args = if opc == op::word::CALL_CLOSURE && !closure {
                            a!() + 1
                        } else {
                            a!()
                        };
                        if args + module.funcs[f].params as usize > func.regs as usize {
                            slow!();
                        }
                        if closure {
                            // Borrowed (`docs/spec/objects.md` 14.9). Step writes it the same way
                            // when it takes over the call in `call!`
                            regs[a!()] = regs[b!()];
                        }
                        call!(f, args)
                    }
                    op::word::RET | op::word::RET_VOID => {
                        if self.task.frames.len() <= 1 {
                            // A return from the first frame (<init> → <main>, exit) is left to step
                            slow!();
                        }
                        // Back in the caller (the frame now on top) after the frame is popped.
                        // A job waiting for the frames to come down (a destructor's caller) and
                        // the destruction procedure go through the outer loop
                        macro_rules! back {
                            () => {{
                                let caller = *self.task.frames.last().expect("not the first frame");
                                if caller.func == DTOR_FN || self.heap.has_jobs() {
                                    break;
                                }
                                enter!(
                                    &module.funcs[caller.func as usize],
                                    caller.base as usize,
                                    caller.pc as usize
                                )
                            }};
                        }
                        let counted_self = self.task.frames.last().unwrap().counted_self();
                        if func.managed.is_empty() && !counted_self {
                            // No implicit `finally` (`op_ret` without managed registers): only the
                            // result moves
                            let fr = self.task.frames.pop().unwrap();
                            if opc == op::word::RET {
                                let v = regs[a!()];
                                self.task.regs[fr.ret() as usize] = v;
                            }
                            back!()
                        }
                        if !counted_self {
                            // The implicit `finally` when each release only decrements a count
                            // (`op_ret` that frees nothing and opens no job): the result is set
                            // aside first, as `op_ret` does. A release that may free goes back to
                            // `op_ret` from there; the registers let go of so far are 0, which it
                            // releases as nothing. A managed register may lie beyond the register
                            // window, so they are reached through `self.task.regs`
                            let v = if opc == op::word::RET {
                                std::mem::take(&mut self.task.regs[base + a!()])
                            } else {
                                0
                            };
                            let released = func.managed.iter().all(|m| {
                                let slot = &mut self.task.regs[base + m.reg as usize];
                                let kept = if m.kind == bytecode::MANAGED_OBJ {
                                    self.heap.release_obj_fast(*slot)
                                } else {
                                    self.heap.release_fast(*slot)
                                };
                                if kept {
                                    *slot = 0;
                                }
                                kept
                            });
                            if released {
                                let fr = self.task.frames.pop().unwrap();
                                if opc == op::word::RET {
                                    self.task.regs[fr.ret() as usize] = v;
                                }
                                back!()
                            }
                            if opc == op::word::RET {
                                self.task.regs[base + a!()] = v;
                            }
                        }
                        // Not the last frame, so the caller goes on
                        op!(self.op_ret((opc == op::word::RET).then_some(base + a!())));
                        break;
                    }
                    // Reference slots to a register, a global variable or a cell, and the
                    // captured cells (`docs/spec/objects.md` 14.9). Anything else, and a check
                    // that fails, goes to `exec_op`
                    op::word::LOAD_REF => {
                        let place = regs[b!()];
                        match self.fast_ref_read(place) {
                            Some(v) => {
                                regs = reg_window(&mut self.task.regs, base);
                                regs[a!()] = v;
                            }
                            None => shared!(),
                        }
                    }
                    op::word::STORE_REF => {
                        let (place, v) = (regs[a!()], regs[b!()]);
                        match self.fast_ref_write(place, v) {
                            Some(()) => regs = reg_window(&mut self.task.regs, base),
                            None => shared!(),
                        }
                    }
                    op::word::CAP_GET => match self.heap.closure_cap(regs[b!()], c!()) {
                        Some(v) => regs[a!()] = v,
                        None => shared!(),
                    },
                    // The type checks of an instance (`docs/spec/objects.md` 14.3): a handle that
                    // does not check, or an `as` that fails, goes to `exec_op`
                    op::word::IS_CLASS => match self.heap.is_class_fast(regs[a!()], bx!() as u32) {
                        Some(yes) => regs[a!()] = Slot::from(yes),
                        None => shared!(),
                    },
                    // `nil` passes `AsClass` (also in `op_as_class`, `exec.rs`): kept here so
                    // that `nil` does not call `is_class_fast`
                    op::word::AS_CLASS => {
                        let h = regs[a!()];
                        if h != 0 && self.heap.is_class_fast(h, bx!() as u32) != Some(true) {
                            shared!()
                        }
                    }
                    // Unmanaged fields of an instance (`docs/spec/objects.md` 14.5): a handle that
                    // does not check, or a lent location, goes to `exec_op`
                    op::word::OBJ_GET => match self.heap.obj_get_fast(regs[b!()], c!()) {
                        Some(v) => regs[a!()] = v,
                        None => shared!(),
                    },
                    op::word::OBJ_SET => match self.heap.obj_set_fast(regs[a!()], b!(), regs[c!()])
                    {
                        Some(()) => {}
                        None => shared!(),
                    },
                    // Unmanaged elements of an array (`docs/design/vm.md` 3.14 (a′))
                    op::word::DYN_GET => match self.heap.dyn_get_fast(regs[b!()], regs[c!()]) {
                        Some(v) => regs[a!()] = v,
                        None => shared!(),
                    },
                    op::word::DYN_SET => {
                        match self.heap.dyn_set_fast(regs[a!()], regs[b!()], regs[c!()]) {
                            Some(()) => {}
                            None => shared!(),
                        }
                    }
                    // Counting by the contents (`docs/spec/objects.md` 14.3) when nothing is
                    // freed: a count that may reach 0 goes to `exec_op`
                    op::word::ASSIGN_OBJ => {
                        if self.heap.assign_obj_fast(regs[b!()], regs[a!()]) {
                            regs[a!()] = regs[b!()];
                        } else {
                            shared!()
                        }
                    }
                    op::word::ADD_REF_OBJ => self.heap.add_ref_obj_fast(regs[a!()]),
                    // Unmanaged fields of a record (`docs/design/vm.md` 3.14 (a′)): a write to a
                    // shared record, a lent location or a slot out of range goes to `exec_op`
                    op::word::FIELD_GET => match self.heap.dyn_get_fast(regs[b!()], c!() as i64) {
                        Some(v) => regs[a!()] = v,
                        None => shared!(),
                    },
                    op::word::FIELD_SET => {
                        match self.heap.dyn_set_fast(regs[a!()], b!() as i64, regs[c!()]) {
                            Some(()) => {}
                            None => shared!(),
                        }
                    }
                    // The cursor of an element of an array whose elements may be flat, and the
                    // unmanaged slots read and written through a cursor (`docs/design/records.md`
                    // 3.4): an index out of range, a lent slot or anything else goes to `exec_op`
                    op::word::ELEM_AT => match self.heap.elem_at_fast(regs[b!()], regs[c!()]) {
                        Some(pos) => {
                            regs[a!() + 1] = regs[b!()];
                            regs[a!()] = pos;
                        }
                        None => shared!(),
                    },
                    op::word::SLOT_GET => {
                        match self
                            .heap
                            .dyn_get_fast(regs[b!() + 1], regs[b!()].wrapping_add(c!() as i64))
                        {
                            Some(v) => regs[a!()] = v,
                            None => shared!(),
                        }
                    }
                    op::word::SLOT_SET => {
                        let (r, i) = (regs[a!() + 1], regs[a!()].wrapping_add(b!() as i64));
                        match self.heap.dyn_set_fast(r, i, regs[c!()]) {
                            Some(()) => {}
                            None => shared!(),
                        }
                    }
                    // Unmanaged fields of a record through a reference slot to a register, a
                    // global variable or a cell (`docs/design/records.md` 3.8). A write needs the
                    // record unique already; anything else goes to `exec_op`, and a write (whose
                    // type word `shared!` does not pass) to step
                    op::word::REF_FIELD_GET => {
                        let place = regs[b!()];
                        match self.fast_ref_read(place) {
                            Some(r) => match self.heap.dyn_get_fast(r, c!() as i64) {
                                Some(v) => {
                                    regs = reg_window(&mut self.task.regs, base);
                                    regs[a!()] = v;
                                }
                                None => shared!(),
                            },
                            // A lent flat place (`docs/design/records.md` 3.7 B)
                            None => match deref(place) {
                                (lend::PLACE_REF, j) => {
                                    match self.heap.place_slot_fast(j, c!() as i64) {
                                        Some(v) => {
                                            let v = *v;
                                            regs = reg_window(&mut self.task.regs, base);
                                            regs[a!()] = v;
                                        }
                                        None => shared!(),
                                    }
                                }
                                _ => shared!(),
                            },
                        }
                    }
                    op::word::REF_FIELD_SET => {
                        let (place, v) = (regs[a!()], regs[c!()]);
                        let done = match self.fast_ref_read(place) {
                            Some(r) => {
                                self.heap.unique_fast(r)
                                    && self.heap.dyn_set_fast(r, b!() as i64, v).is_some()
                            }
                            None => match deref(place) {
                                (lend::PLACE_REF, j) => {
                                    match self.heap.place_slot_fast(j, b!() as i64) {
                                        Some(slot) => {
                                            *slot = v;
                                            true
                                        }
                                        None => false,
                                    }
                                }
                                _ => false,
                            },
                        };
                        if done {
                            regs = reg_window(&mut self.task.regs, base);
                            // Over the type word
                            pc = next + 1;
                            continue;
                        }
                        slow!()
                    }
                    // Their type word is not `bx!()` (`docs/spec/bytecode.md` 1.2): step reads it
                    op::word::REF_FIELD_SET_REF => slow!(),
                    op::word::STR_LEN => match self.heap.str_len_fast(regs[b!()]) {
                        Some(n) => regs[a!()] = n,
                        None => shared!(),
                    },
                    op::word::ASSIGN_REF => {
                        if self.heap.assign_ref_fast(regs[b!()], regs[a!()]) {
                            regs[a!()] = regs[b!()];
                        } else {
                            shared!()
                        }
                    }
                    // The same counting as `AssignRef`, from and to a global variable
                    op::word::LOAD_GLOBAL_REF => {
                        let v = self.globals[bx!()];
                        if self.heap.assign_ref_fast(v, regs[a!()]) {
                            regs[a!()] = v;
                        } else {
                            shared!()
                        }
                    }
                    op::word::STORE_GLOBAL_REF => {
                        let v = regs[a!()];
                        if self.heap.assign_ref_fast(v, self.globals[bx!()]) {
                            self.globals[bx!()] = v;
                        } else {
                            shared!()
                        }
                    }
                    op::word::RELEASE_OBJ => {
                        if self.heap.release_obj_fast(regs[a!()]) {
                            regs[a!()] = 0;
                        } else {
                            shared!()
                        }
                    }
                    op::word::MAKE_REF => regs[a!()] = make_ref(1, (base + b!()) as u64),
                    op::word::MAKE_GLOBAL_REF => regs[a!()] = make_ref(2, bx!() as u64),
                    op::word::INTRINSIC
                        if c!() == usize::from(op::intr::CELL_REF)
                            && heap::is_block_ref(regs[b!()]) =>
                    {
                        regs[a!()] = make_ref(3, regs[b!()] as u64)
                    }
                    // Managed values, strings, records and arrays: the bodies shared with step
                    // (`exec_op`; `docs/design/vm.md` 3.14). The rest goes to step
                    _ => shared!(),
                }
                pc = next;
            }
        }
    }

    /// `run_loop`'s hand-off of the instruction at `pc` to `step` (kept out of the hot loop so its
    /// values do not take machine registers there; `docs/design/vm.md` 3.14). `None`: go on with
    /// the function's information reloaded; `Some`: return the step from `run_loop`.
    #[cold]
    #[inline(never)]
    fn slow_at(&mut self, pc: usize, budget: &mut u32) -> Option<Step> {
        self.task.frames.last_mut().unwrap().pc = pc as u32;
        match self.step(budget) {
            Step::Continue => {
                if self.heap.job_open() {
                    // The instruction let go of an instance's last reference
                    self.stamp_instr(pc as u32);
                }
                if *budget == 0 {
                    return Some(Step::Continue);
                }
                None
            }
            other => Some(other),
        }
    }

    /// `run_loop`'s failure of the instruction at `pc`: writes back `pc` (the exception is raised
    /// at this instruction) and leaves it to `run`.
    #[cold]
    #[inline(never)]
    fn fail_at(&mut self, pc: usize, e: RefError) -> Step {
        self.task.frames.last_mut().unwrap().pc = pc as u32;
        e.into_step()
    }

    fn step(&mut self, budget: &mut u32) -> Step {
        let Some(frame) = self.task.frames.last().copied() else {
            return Step::Fatal("no frame".into());
        };
        if frame.func == DTOR_FN {
            return self.dtor_step();
        }
        let module = Arc::clone(&self.module);
        let func = &module.funcs[frame.func as usize];
        let code = &func.code;
        let pc = frame.pc as usize;
        if pc >= code.len() {
            return Step::Fatal(format!("pc {pc} out of range in {}", func.name));
        }
        // The verifier keeps a `Wide` followed by two words
        let (number, a, b, c, len) = bytecode::decode_at(code, pc);
        let wide = bytecode::is_wide_at(code, pc);
        let next = pc + len;
        // The VM runs the instructions 0 to 255 by the `op` of their word (`op::word`); one
        // numbered past 255 (after `Ext`; 1.2) has its own arms below
        let opc = u8::try_from(number).unwrap_or(op::word::EXT);
        // The type word stands for `Bx` in the instructions that have one (1.2)
        let bx = if bytecode::has_type_word(number) {
            bytecode::type_word_at(code, pc, len)
        } else {
            bytecode::bx_at(code, pc)
        };
        let sbx = bytecode::signed_bx(bx, wide);
        let sc = bytecode::signed_c(c, wide);
        let base = frame.base as usize;
        let ra = base + a as usize;
        let rb = base + b as usize;
        let rc = base + c as usize;
        // The instructions the fast path also runs (`exec.rs`)
        if number < op::EXT_BASE
            && let Some(r) = self.exec_op(opc, ra, rb, rc, b as usize, c as usize, bx as usize)
        {
            try_op!(r);
            self.task.frames.last_mut().unwrap().pc = next as u32;
            return Step::Continue;
        }
        let regs = &mut self.task.regs;
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
        let overflow = exec::int_overflow;
        match opc {
            op::word::NOP => {}
            op::word::MOVE => r!(ra) = r!(rb),
            op::word::LOAD_ZERO => r!(ra) = 0,
            op::word::LOAD_INT => r!(ra) = sbx,
            op::word::LOAD_CONST => {
                // Make the constant on first use; then it is a load from the cache
                match self.load_const(bx as usize) {
                    Ok(_) => {}
                    Err(PollError::Rt(e)) => return Step::Error(e),
                    Err(PollError::Fatal(e)) => return Step::Fatal(e.message),
                };
                try_op!(self.op_load_cached_const(ra, bx as usize));
            }
            op::word::LOAD_GLOBAL => r!(ra) = self.globals[bx as usize],
            op::word::STORE_GLOBAL => self.globals[bx as usize] = r!(ra),
            op::word::MAKE_REF => r!(ra) = make_ref(1, rb as u64),
            op::word::MAKE_GLOBAL_REF => r!(ra) = make_ref(2, bx as u64),
            op::word::OUTER_REF => {
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
            op::word::ADD_INT => r!(ra) = r!(rb).wrapping_add(r!(rc)),
            op::word::SUB_INT => r!(ra) = r!(rb).wrapping_sub(r!(rc)),
            op::word::MUL_INT => r!(ra) = r!(rb).wrapping_mul(r!(rc)),
            op::word::ADD_INT32 | op::word::SUB_INT32 | op::word::MUL_INT32 => {
                r!(ra) = exec::int32(opc, r!(rb), r!(rc))
            }
            op::word::DIV_INT
            | op::word::DIV_INT_CHK
            | op::word::MOD_INT
            | op::word::DIV_UINT
            | op::word::MOD_UINT => match exec::int_div(opc, r!(rb), r!(rc)) {
                Ok(v) => r!(ra) = v,
                Err(e) => return Step::Error(e),
            },
            op::word::NEG_INT => r!(ra) = r!(rb).wrapping_neg(),
            op::word::AND_INT => r!(ra) = r!(rb) & r!(rc),
            op::word::OR_INT => r!(ra) = r!(rb) | r!(rc),
            op::word::XOR_INT => r!(ra) = r!(rb) ^ r!(rc),
            op::word::NOT_INT => r!(ra) = !r!(rb),
            op::word::SHL_INT => r!(ra) = r!(rb).wrapping_shl((r!(rc) & 63) as u32),
            op::word::SHR_INT => {
                r!(ra) = ((r!(rb) as u64).wrapping_shr((r!(rc) & 63) as u32)) as i64
            }
            op::word::SHL_INT32 => {
                r!(ra) = ((r!(rb) as i32).wrapping_shl((r!(rc) & 31) as u32)) as i64
            }
            op::word::SHR_INT32 => {
                r!(ra) = (((r!(rb) as u32).wrapping_shr((r!(rc) & 31) as u32)) as i32) as i64
            }
            op::word::ADD_INT_I => r!(ra) = r!(rb).wrapping_add(sc),
            op::word::MUL_INT_I
            | op::word::AND_INT_I
            | op::word::OR_INT_I
            | op::word::XOR_INT_I
            | op::word::SHL_INT_I
            | op::word::SHR_INT_I
            | op::word::SHL_INT32_I
            | op::word::SHR_INT32_I
            | op::word::ADD_INT32_I
            | op::word::MUL_INT32_I => r!(ra) = exec::int_imm(opc, r!(rb), sc),
            op::word::DIV_INT_I | op::word::MOD_INT_I => {
                let div = if opc == op::word::DIV_INT_I {
                    op::word::DIV_INT
                } else {
                    op::word::MOD_INT
                };
                match exec::int_div(div, r!(rb), sc) {
                    Ok(v) => r!(ra) = v,
                    Err(e) => return Step::Error(e),
                }
            }
            op::word::ADD_INT_CHK => match r!(rb).checked_add(r!(rc)) {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::word::SUB_INT_CHK => match r!(rb).checked_sub(r!(rc)) {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::word::MUL_INT_CHK => match r!(rb).checked_mul(r!(rc)) {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::word::NEG_INT_CHK => match r!(rb).checked_neg() {
                Some(v) => r!(ra) = v,
                None => return Step::Error(overflow("Int64")),
            },
            op::word::ADD_UINT_CHK => match (r!(rb) as u64).checked_add(r!(rc) as u64) {
                Some(v) => r!(ra) = v as i64,
                None => return Step::Error(overflow("UInt64")),
            },
            op::word::SUB_UINT_CHK => match (r!(rb) as u64).checked_sub(r!(rc) as u64) {
                Some(v) => r!(ra) = v as i64,
                None => return Step::Error(overflow("UInt64")),
            },
            op::word::MUL_UINT_CHK => match (r!(rb) as u64).checked_mul(r!(rc) as u64) {
                Some(v) => r!(ra) = v as i64,
                None => return Step::Error(overflow("UInt64")),
            },
            op::word::NARROW => r!(ra) = narrow(r!(rb), c),
            op::word::NARROW_OVF | op::word::NARROW_RNG => {
                let v = r!(rb);
                if !narrow_fits(v, c) {
                    let (name, lo, hi) = width_type(c);
                    if opc == op::word::NARROW_OVF {
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
            op::word::EQ_INT => r!(ra) = (r!(rb) == r!(rc)) as i64,
            op::word::LT_INT => r!(ra) = (r!(rb) < r!(rc)) as i64,
            op::word::LE_INT => r!(ra) = (r!(rb) <= r!(rc)) as i64,
            op::word::LT_UINT => r!(ra) = ((r!(rb) as u64) < (r!(rc) as u64)) as i64,
            op::word::LE_UINT => r!(ra) = ((r!(rb) as u64) <= (r!(rc) as u64)) as i64,
            op::word::NOT_BOOL => r!(ra) = r!(rb) ^ 1,
            op::word::JMP => jump(sbx, budget),
            op::word::JMP_IF_TRUE => {
                if r!(ra) != 0 {
                    jump(sbx, budget);
                }
            }
            op::word::JMP_IF_FALSE => {
                if r!(ra) == 0 {
                    jump(sbx, budget);
                }
            }
            op::word::JMP_IF_EQ_INT => {
                if r!(ra) == r!(rb) {
                    jump(sc, budget);
                }
            }
            op::word::JMP_IF_NE_INT => {
                if r!(ra) != r!(rb) {
                    jump(sc, budget);
                }
            }
            op::word::JMP_IF_LT_INT => {
                if r!(ra) < r!(rb) {
                    jump(sc, budget);
                }
            }
            op::word::JMP_IF_LE_INT => {
                if r!(ra) <= r!(rb) {
                    jump(sc, budget);
                }
            }
            op::word::JMP_IF_LT_UINT => {
                if (r!(ra) as u64) < (r!(rb) as u64) {
                    jump(sc, budget);
                }
            }
            op::word::JMP_IF_LE_UINT => {
                if (r!(ra) as u64) <= (r!(rb) as u64) {
                    jump(sc, budget);
                }
            }
            op::word::JMP_IF_EQ_INT_I
            | op::word::JMP_IF_NE_INT_I
            | op::word::JMP_IF_LT_INT_I
            | op::word::JMP_IF_LE_INT_I
            | op::word::JMP_IF_GT_INT_I
            | op::word::JMP_IF_GE_INT_I => {
                if exec::cmp_imm(opc, r!(ra), bytecode::signed_b(b, wide)) {
                    jump(sc, budget);
                }
            }
            op::word::FOR_LOOP_UP | op::word::FOR_LOOP_DOWN => {
                if let Some(v) = exec::for_step(opc, r!(ra), r!(rb)) {
                    r!(ra) = v;
                    jump(sc, budget);
                }
            }
            op::word::JMP_IF_EQ_STR | op::word::JMP_IF_NE_STR => {
                let eq = try_op!(self.str_equal(self.task.regs[ra], self.task.regs[rb]));
                if eq == (opc == op::word::JMP_IF_EQ_STR) {
                    jump(sc, budget);
                }
            }
            op::word::STR_FROM_CHAR => try_op!(self.op_str_from_char(ra, rb)),
            op::word::ENUM_TO_STR => {
                let text = match module.type_entry(c) {
                    Some(t) => t.enum_name(regs[rb]),
                    None => return Step::Fatal(format!("type {c} is not an enumeration")),
                };
                match self.alloc_string(try_oom!(PStr::from_text(&text))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::word::STR_TO_INT_IO => {
                let s = get_str!(self.heap, regs[rb]);
                let units = try_oom!(s.units());
                let (_, lo, hi) = width_type(c);
                match strconv::parse_int(&units, true) {
                    Some(v) if v >= lo && v <= hi => regs[ra] = v as i64,
                    _ => {
                        let text = try_oom!(s.to_utf8());
                        return Step::Error(input::input_error(format!(
                            "invalid integer input: '{text}'"
                        )));
                    }
                }
            }
            op::word::FORMAT => {
                let fmt = try_oom!(get_str!(self.heap, regs[rb]).units());
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
                            format::FmtArg::Str(try_oom!(get_str!(self.heap, v).units()))
                        }
                        bytecode::FORMAT_KIND_UTF8 => {
                            let b = get_u8!(self.heap, v);
                            let p = try_oom!(PStr::from_text(&String::from_utf8_lossy(b)));
                            format::FmtArg::Str(try_oom!(p.units()))
                        }
                        bytecode::FORMAT_KIND_SINGLE => format::FmtArg::Single(slot_f32(v)),
                        bytecode::FORMAT_KIND_DOUBLE => format::FmtArg::Double(slot_f64(v)),
                        bytecode::FORMAT_KIND_CURRENCY => format::FmtArg::Curr(v),
                        k => return Step::Fatal(format!("unknown `Format` argument kind {k}")),
                    });
                }
                match format::format(&fmt, &args, self.heap.str_room()) {
                    Ok(units) => match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                        Ok(x) => try_ref!(self.put(ra, x)),
                        Err(e) => return Step::Error(e),
                    },
                    Err(format::FormatError::OutOfMemory) => {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    Err(format::FormatError::Invalid) => {
                        let text = String::from_utf16_lossy(&fmt);
                        return Step::Error(convert_error(text, |t| {
                            format!("format '{t}' invalid or incompatible with argument")
                        }));
                    }
                }
            }
            // `NegF64` / `NegF32` have the shape `A B`: `C` is not verified, so it is not read
            op::word::NEG_F64 => regs[ra] = f64_slot(-slot_f64(regs[rb])),
            op::word::NEG_F32 => regs[ra] = f32_slot(-slot_f32(regs[rb])),
            op::word::ADD_F64..=op::word::LE_F32 => regs[ra] = real_op(opc, regs[rb], regs[rc]),
            op::word::INT_TO_F64 => regs[ra] = f64_slot(regs[rb] as f64),
            op::word::UINT_TO_F64 => regs[ra] = f64_slot(regs[rb] as u64 as f64),
            op::word::INT_TO_F32 => regs[ra] = f32_slot(regs[rb] as f32),
            op::word::UINT_TO_F32 => regs[ra] = f32_slot(regs[rb] as u64 as f32),
            op::word::F64_TO_F32 => regs[ra] = f32_slot(slot_f64(regs[rb]) as f32),
            op::word::F32_TO_F64 => regs[ra] = f64_slot(slot_f32(regs[rb]) as f64),
            op::word::SQRT_F64 => regs[ra] = f64_slot(slot_f64(regs[rb]).sqrt()),
            op::word::TRUNC_F64 | op::word::ROUND_F64 => {
                let v = slot_f64(regs[rb]);
                let round = opc == op::word::ROUND_F64;
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
            op::word::ADD_CURR | op::word::SUB_CURR | op::word::MUL_CURR | op::word::DIV_CURR => {
                let (x, y) = (regs[rb], regs[rc]);
                let v = match opc {
                    op::word::ADD_CURR => numfmt::curr_add(x, y),
                    op::word::SUB_CURR => numfmt::curr_sub(x, y),
                    op::word::MUL_CURR => numfmt::curr_mul(x, y),
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
            op::word::STR_PAD_LEFT => {
                let w = regs[rc];
                let s = get_str!(self.heap, regs[rb]);
                if w <= s.len() as i64 {
                    let same = regs[rb];
                    try_ref!(self.put_shared(ra, same));
                } else {
                    if w > self.heap.str_room() as i64 {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    let mut units = Vec::new();
                    try_oom!(heap::try_reserve(&mut units, w as usize));
                    units.extend(std::iter::repeat_n(32u16, w as usize - s.len()));
                    units.extend(s.iter_units());
                    match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                        Ok(x) => try_ref!(self.put(ra, x)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::word::CALL
            | op::word::CALL_METHOD
            | op::word::CALL_VIRTUAL
            | op::word::CALL_VIRTUAL_CLASS
            | op::word::CALL_INTF => {
                let callee = match opc {
                    op::word::CALL => bx,
                    _ => match self.method_callee(opc, ra, bx) {
                        Ok(Some(f)) => {
                            // A table entry is checked against the function table; its
                            // arguments must fit in the caller's frame
                            if a as usize + module.funcs[f as usize].params as usize
                                > func.regs as usize
                            {
                                return Step::Fatal(format!(
                                    "virtual call arguments exceed regs at {pc:04x}"
                                ));
                            }
                            f
                        }
                        // A built-in body ran in place (`docs/spec/objects.md` 14.3)
                        Ok(None) => {
                            self.task.frames.last_mut().unwrap().pc = next as u32;
                            return Step::Continue;
                        }
                        Err(step) => return step,
                    },
                };
                return self.enter_call(
                    callee,
                    base + a as usize,
                    (base + a as usize) as u32,
                    false,
                    next,
                    budget,
                );
            }
            // `docs/spec/objects.md` 14.9
            op::word::CALL_INDIRECT => {
                let v = self.task.regs[rb];
                if v == 0 {
                    return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
                }
                let Some(callee) = u32::try_from(v - 1)
                    .ok()
                    .filter(|f| (*f as usize) < module.funcs.len())
                else {
                    return Step::Fatal(format!("bad procedural value {v} at {pc:04x}"));
                };
                if a as usize + module.funcs[callee as usize].params as usize > func.regs as usize {
                    return Step::Fatal(format!("indirect call arguments exceed regs at {pc:04x}"));
                }
                return self.enter_call(callee, ra, ra as u32, false, next, budget);
            }
            op::word::CALL_CLOSURE => return self.call_closure(ra, rb, pc, next, budget),
            op::word::CALL_HOST => {
                // Advance `pc` only on success (so that a failure is reported at the `CallHost` line)
                let step = self.call_host(bx as usize, ra as u32, next as u32);
                if matches!(step, Step::Continue) {
                    self.task.frames.last_mut().unwrap().pc = next as u32;
                }
                return step;
            }
            op::word::RET | op::word::RET_VOID => {
                let value = (opc == op::word::RET).then_some(ra);
                if try_op!(self.op_ret(value)) {
                    if self
                        .heap
                        .top_job()
                        .is_some_and(|(o, _)| o == heap::JobOrigin::RetLast)
                    {
                        // The destructors of the last frame run first
                        return Step::Continue;
                    }
                    return self.frames_empty_after_return();
                }
                return Step::Continue;
            }
            op::word::HALT => {
                let code = regs[ra];
                self.set_exit_code(code);
                return Step::Halt(None);
            }
            op::word::RAISE => {
                if let Err(e) = self.check_raised(self.task.regs[ra]) {
                    return e;
                }
                let x = std::mem::take(&mut self.task.regs[ra]);
                return Step::Throw(x);
            }
            op::word::RAISE_OUTER => {
                if let Err(e) = self.check_raised(self.task.regs[ra]) {
                    return e;
                }
                let x = std::mem::take(&mut self.task.regs[ra]);
                // `rB` becomes the `InnerException` of `rA`, unless that makes a cycle (5.4)
                let inner = self.task.regs[rb];
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
            op::word::END_FINALLY => {
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
            op::word::EXC_INFO => {
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
                        let text = match self.stack_trace_text(x) {
                            Ok(t) => t,
                            Err(e) => return Step::Fatal(e.message),
                        };
                        match self.alloc_string(try_oom!(PStr::from_text(&text))) {
                            Ok(v) => v,
                            Err(e) => return Step::Error(e),
                        }
                    }
                };
                try_ref!(self.put(ra, v));
            }
            // Sets (`docs/spec/types.md` 8.3, `docs/spec/bytecode.md` 2.12): a small set is the
            // bits of an integer, a big set a block of bit words (0 is the empty set)
            op::word::SET_IN => {
                let x = regs[rb];
                regs[ra] = ((0..64).contains(&x) && regs[rc] & (1 << x) != 0) as i64;
            }
            op::word::SET_RANGE => {
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
            op::word::SET_NEXT => {
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
            op::word::CHK_SET => {
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
            op::word::SET_IN_BIG => {
                let (x, set) = (regs[rb], regs[rc]);
                let words = match self.heap.set_slots(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                self.task.regs[ra] = set_word_bit(words, x) as i64;
            }
            op::word::SET_INCL | op::word::SET_EXCL => {
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
                    if opc == op::word::SET_INCL {
                        words[k] |= m;
                    } else {
                        words[k] &= !m;
                    }
                }
            }
            op::word::SET_UNION
            | op::word::SET_DIFF
            | op::word::SET_INTER
            | op::word::SET_UNION_BITS => {
                let (set, other) = (regs[ra], regs[rb]);
                let theirs: Vec<i64> = if opc == op::word::SET_UNION_BITS {
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
                        op::word::SET_DIFF => *w & !t,
                        op::word::SET_INTER => *w & t,
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
            op::word::SET_EQ | op::word::SET_SUB => {
                let (x, y) = (regs[rb], regs[rc]);
                let (xs, ys) = match (self.heap.set_slots(x), self.heap.set_slots(y)) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(e), _) | (_, Err(e)) => return Step::Fatal(e.message),
                };
                let n = xs.len().max(ys.len());
                let at = |v: &[i64], k: usize| v.get(k).copied().unwrap_or(0);
                let holds = (0..n).all(|k| {
                    let (a, b) = (at(xs, k), at(ys, k));
                    if opc == op::word::SET_EQ {
                        a == b
                    } else {
                        a & !b == 0
                    }
                });
                self.task.regs[ra] = holds as i64;
            }
            op::word::SET_NEXT_BIG => {
                let (set, from) = (regs[rb], regs[rc].max(0));
                let words = match self.heap.set_slots(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                self.task.regs[ra] = set_next_in(words, from);
            }
            op::word::CHK_SET_BIG => {
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
            op::word::SET_BITS => {
                let set = regs[rb];
                let words = match self.heap.set_slots(set) {
                    Ok(w) => w,
                    Err(e) => return Step::Fatal(e.message),
                };
                self.task.regs[ra] = words.first().copied().unwrap_or(0);
            }
            op::word::CHK_INDEX => {
                let (i, len) = (regs[ra], regs[rb]);
                if !(0..len).contains(&i) {
                    let tail = match &module.consts[c as usize] {
                        ConstEntry::Str(t) => t.as_str(),
                        _ => "",
                    };
                    return Step::Error(rt(exc::RANGE_ERROR, index_message(tail, i)));
                }
            }
            op::word::CHK_RANGE => {
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
            op::word::SET_LEN => {
                let ty = bx;
                let n = regs[ra + 1];
                if n < 0 {
                    return Step::Error(rt(exc::RANGE_ERROR, format!("invalid length {n}")));
                }
                // Resizing a shared array copies its elements (`docs/spec/types.md` chapter 3)
                if self.heap.lending() {
                    let arr = regs[ra];
                    let shared = try_op!(self.heap.ref_count(arr).map_err(RefError::from)) > 1;
                    let len = try_op!(self.heap.len(arr).map_err(RefError::from));
                    if shared {
                        let kept = len.min(n as usize);
                        try_op!(self.heap.check_copy(arr, 0, kept).map_err(RefError::from));
                    }
                }
                match self.heap.set_len(regs[ra], ty, n as usize) {
                    Ok(Ok(v)) => self.task.regs[ra] = v,
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::word::NEW_ARR => {
                // `rA` holds the length (an integer), so nothing is let go
                let ty = bx;
                let n = regs[ra].max(0) as usize;
                match self.heap.alloc_dyn(ty, n) {
                    Ok(v) => self.task.regs[ra] = v,
                    Err(_) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                }
            }
            op::word::ARR_COPY => {
                // `rA` holds the borrowed source, so nothing is let go
                let ty = bx;
                let (src, index, count) = (regs[ra], regs[ra + 1], regs[ra + 2]);
                if self.heap.lending() {
                    let len = try_op!(self.heap.len(src).map_err(RefError::from)) as i64;
                    let start = index.clamp(0, len);
                    let end = start.saturating_add(count.max(0)).min(len);
                    try_op!(
                        self.heap
                            .check_copy(src, start as usize, end as usize)
                            .map_err(RefError::from)
                    );
                }
                match self.heap.copy_range(src, ty, index, count) {
                    Ok(Ok(v)) => self.task.regs[ra] = v,
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::word::U8_LEN => r!(ra) = get_u8!(self.heap, regs[rb]).len() as i64,
            op::word::U8_INDEX => {
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
            // The body is shared with `AppendU8` (`exec.rs`, inlined here). The arm stays in `step`:
            // moving it to `exec_op`, or calling the body without inlining, changed the layout of
            // the hot loop (`docs/notes/benchmark-m2.md`, M3-17)
            op::word::CONCAT_U8 => try_op!(self.op_concat_u8(ra, rb, rc)),
            op::word::CMP_U8 => {
                // The byte order of UTF-8 is the code point order (`docs/spec/types.md` 8.5)
                let o = get_u8!(self.heap, regs[rb]).cmp(get_u8!(self.heap, regs[rc]));
                regs[ra] = o as i64;
            }
            op::word::EQ_U8 => {
                regs[ra] = (get_u8!(self.heap, regs[rb]) == get_u8!(self.heap, regs[rc])) as i64
            }
            op::word::UP_CASE_CHAR => {
                let u = regs[rb];
                regs[ra] = if (97..=122).contains(&u) { u - 32 } else { u };
            }
            op::word::ARR_CONCAT => {
                // `rA` holds the borrowed left operand, so nothing is let go
                let (x, y) = (regs[ra], regs[ra + 1]);
                let lens = self.heap.len(x).and_then(|a| Ok((a, self.heap.len(y)?)));
                let (lx, ly) = match lens {
                    Ok(l) => l,
                    Err(e) => return Step::Fatal(e.message),
                };
                if let Err(a) = self
                    .heap
                    .check_copy(x, 0, lx)
                    .and_then(|()| self.heap.check_copy(y, 0, ly))
                {
                    return RefError::from(a).into_step();
                }
                let pieces = [Piece::Range(x, 0, lx), Piece::Range(y, 0, ly)];
                match self.heap.build_dyn(bx, &pieces) {
                    Ok(Ok(v)) => self.task.regs[ra] = v,
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::word::ARR_INSERT | op::word::ARR_INSERT_ARR | op::word::ARR_DELETE => {
                // `rA` is the owned array read without counting; the compiler writes it back
                let arr = regs[ra];
                let (b, c) = (regs[ra + 1], regs[ra + 2]);
                let len = match self.heap.len(arr) {
                    Ok(n) => n,
                    Err(e) => return Step::Fatal(e.message),
                };
                // Inserting and deleting move the elements (`docs/spec/types.md` chapter 3)
                if self.heap.lending() {
                    let mut check = self.heap.check_copy(arr, 0, len);
                    if opc == op::word::ARR_INSERT_ARR {
                        let n = match self.heap.len(b) {
                            Ok(n) => n,
                            Err(e) => return Step::Fatal(e.message),
                        };
                        check = check.and_then(|()| self.heap.check_copy(b, 0, n));
                    } else if opc == op::word::ARR_INSERT {
                        check = check.and_then(|()| self.heap.check_share(b));
                    }
                    if let Err(a) = check {
                        return RefError::from(a).into_step();
                    }
                }
                let range_error =
                    |i: i64, n: usize| Step::Error(rt(exc::RANGE_ERROR, dyn_index_message(i, n)));
                let pieces = if opc == op::word::ARR_DELETE {
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
                    let mid = if opc == op::word::ARR_INSERT {
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
                            self.task.regs[ra] = v;
                        }
                        Ok(Err(_)) => {
                            return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                        }
                        Err(e) => return Step::Fatal(e.message),
                    }
                }
            }
            // Flat places with a type word (`docs/spec/bytecode.md` 2.8): `bx` is the type word.
            // Not in `exec_op`, so the fast path leaves them to step without an arm of its own
            op::word::AGG_LOAD => try_op!(self.op_agg_load(ra, rb, bx)),
            op::word::AGG_STORE => try_op!(self.op_agg_store(ra, rb, bx)),
            op::word::LEND_PLACE => try_op!(self.op_lend_place(ra, rb, c as usize, bx)),
            op::word::UNIQUE_VAL => match self
                .check_copy_shared(self.task.regs[ra])
                .and_then(|()| Ok(self.heap.unique_val(self.task.regs[ra])?))
            {
                Ok(Ok(v)) => self.task.regs[ra] = v,
                Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                Err(e) => return e.into_step(),
            },
            op::word::MATERIALIZE => {
                if regs[ra] == 0 {
                    let ty = bx;
                    match self.heap.alloc_default(ty) {
                        Ok(Ok(v)) => self.task.regs[ra] = v,
                        Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                        Err(e) => return Step::Fatal(e.message),
                    }
                }
            }
            op::word::INTRINSIC => match u8::try_from(c) {
                // `docs/spec/bytecode.md` 1.3, 2.22: the budget goes down by the comparisons
                Ok(op::intr::ARRAY_SORT_RANGE) => match self.array_sort_range(ra, rb) {
                    Ok(compares) => {
                        let used = u32::try_from(1 + compares / SORT_COMPARES_PER_UNIT)
                            .unwrap_or(u32::MAX);
                        *budget = budget.saturating_sub(used);
                    }
                    Err(e) => return e.into_step(),
                },
                Ok(id) => return self.intrinsic(id, ra, rb, pc, next),
                Err(_) => return Step::Fatal(format!("unknown intrinsic {c}")),
            },
            // The instruction past 255 of the unit tests (`op::TEST_EXT`)
            #[cfg(test)]
            op::word::EXT if number == op::TEST_EXT => r!(ra) = r!(rb).wrapping_sub(r!(rc)),
            _ => return Step::Fatal(format!("unknown opcode {number}")),
        }
        self.task.frames.last_mut().unwrap().pc = new_pc;
        Step::Continue
    }

    /// The declared name of the class of the instance `h`, as a new string (`ClassName` and the
    /// built-in `ToString`; `docs/spec/objects.md` 14.3). Checks the handle (14.4).
    fn class_name_string(&mut self, h: Slot) -> Result<Slot, Step> {
        let ty = self
            .heap
            .obj_class(h)
            .map_err(|e| RefError::from(e).into_step())?;
        self.type_name_string(ty)
    }

    /// Makes the frame of a call of `callee` at `new_base` (`Call`; `docs/spec/bytecode.md` 2.11)
    /// whose result goes to `ret_reg`, after checking the stack limit and growing the registers.
    /// `counted_self`: `r0` holds a counted `Self` the VM lets go of when the frame is left
    /// (`CallClosure`; `docs/spec/objects.md` 14.9). The caller's `pc` moves to `next`.
    fn enter_call(
        &mut self,
        callee: u32,
        new_base: usize,
        ret_reg: u32,
        counted_self: bool,
        next: usize,
        budget: &mut u32,
    ) -> Step {
        *budget = budget.saturating_sub(1);
        let module = Arc::clone(&self.module);
        let f = &module.funcs[callee as usize];
        let need = new_base + f.regs as usize;
        // Recursion limit
        let bytes = need * std::mem::size_of::<Slot>()
            + (self.task.frames.len() + 1) * std::mem::size_of::<Frame>();
        // The arguments are lent by the caller (`docs/design/lifetime.md` 3.3); a counted `Self`
        // is let go of when the frame cannot be made, as if the callee had been entered and left
        let failed = |this: &mut Self, e: RtError| {
            if !counted_self {
                return Step::Error(e);
            }
            let s = std::mem::take(&mut this.task.regs[new_base]);
            match this.heap.release(s) {
                Ok(()) => Step::Error(e),
                Err(b) => Step::Fatal(b.message),
            }
        };
        if bytes > self.config.stack_limit {
            let e = rt(
                exc::STACK_OVERFLOW,
                format!("stack overflow (limit {} bytes)", self.config.stack_limit),
            );
            return failed(self, e);
        }
        if let Err(e) = self.grow_regs(frame_end(new_base, f.regs as usize)) {
            return failed(self, e);
        }
        // Zero the registers other than the arguments
        zero_slots(&mut self.task.regs[new_base + f.params as usize..need]);
        self.task.frames.last_mut().unwrap().pc = next as u32;
        let mark = if counted_self { FRAME_COUNTED_SELF } else { 0 };
        self.task.frames.push(Frame {
            func: callee,
            base: new_base as u32,
            pc: 0,
            ret_reg: ret_reg | mark,
        });
        Step::Continue
    }

    /// `CallClosure A B` (`docs/spec/objects.md` 14.9): calls the callable value `rB` with the
    /// arguments from `r(A+1)`; the result comes back in `rA`.
    fn call_closure(
        &mut self,
        ra: usize,
        rb: usize,
        pc: usize,
        next: usize,
        budget: &mut u32,
    ) -> Step {
        let v = self.task.regs[rb];
        if v == 0 {
            return Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS));
        }
        let c = match self.heap.callable(v) {
            Ok(c) => c,
            Err(e) => return Step::Fatal(e.message),
        };
        let module = Arc::clone(&self.module);
        let frame = *self.task.frames.last().expect("a call runs in a frame");
        let caller_regs = module.funcs[frame.func as usize].regs as usize;
        let a = ra - frame.base as usize;
        let fits = |f: u32, base: usize| {
            module
                .funcs
                .get(f as usize)
                .is_some_and(|f| base + f.params as usize <= caller_regs)
        };
        match c.kind {
            heap::CallableKind::Closure => {
                if !fits(c.func, a) {
                    return Step::Fatal(format!("bad closure at {pc:04x}"));
                }
                // The closure is the hidden first argument, borrowed (`docs/spec/objects.md` 14.9)
                self.task.regs[ra] = v;
                self.enter_call(c.func, ra, ra as u32, false, next, budget)
            }
            heap::CallableKind::Routine => {
                if !fits(c.func, a + 1) {
                    return Step::Fatal(format!("bad callable value at {pc:04x}"));
                }
                // The arguments are where `Call` would have them; the result comes back in `rA`
                self.enter_call(c.func, ra + 1, ra as u32, false, next, budget)
            }
            heap::CallableKind::Method | heap::CallableKind::ClassMethod => {
                let class = c.kind == heap::CallableKind::ClassMethod;
                let (this, counted) = if class {
                    (c.target, false)
                } else {
                    let h = match self.heap.weak_target(c.target) {
                        Ok(h) => h,
                        Err(e) => return RefError::from(e).into_step(),
                    };
                    if h == 0 {
                        return Step::Error(rt(exc::ACCESS_VIOLATION, FREED_ACCESS));
                    }
                    let counted = match self.heap.obj_counted(h) {
                        Ok(k) => k,
                        Err(e) => return RefError::from(e).into_step(),
                    };
                    if counted && let Err(e) = self.heap.add_ref(h) {
                        return Step::Fatal(e.message);
                    }
                    (h, counted)
                };
                self.task.regs[ra] = this;
                if c.func >= bytecode::VMT_BUILTIN {
                    // A built-in body runs here without a frame (`docs/spec/objects.md` 9.3); the
                    // instance taken out is let go of at once
                    let r = self.builtin_body(c.func, ra, class);
                    if counted {
                        // `rA` holds the result now, or still the instance
                        let ran = matches!(r, Ok(true));
                        let released = if ran {
                            self.heap.release(this)
                        } else {
                            let s = std::mem::take(&mut self.task.regs[ra]);
                            self.heap.release(s)
                        };
                        if let Err(e) = released {
                            return Step::Fatal(e.message);
                        }
                    }
                    return match r {
                        Ok(_) => {
                            self.task.frames.last_mut().unwrap().pc = next as u32;
                            Step::Continue
                        }
                        Err(step) => step,
                    };
                }
                if !fits(c.func, a) {
                    return Step::Fatal(format!("bad callable value at {pc:04x}"));
                }
                self.enter_call(c.func, ra, ra as u32, counted, next, budget)
            }
        }
    }

    /// Entry `k` of the virtual method table of the class `ty` as it is in the table, and the
    /// class's type entry; `Fatal` when `k` is out of the table (`docs/spec/objects.md` 14.9,
    /// `docs/spec/bytecode.md` 2.22).
    fn table_slot(&self, ty: u32, k: Slot) -> Result<(u32, &bytecode::TypeEntry), Step> {
        let Some(entry) = self.heap.type_entry(ty) else {
            return Err(Step::Fatal(format!("type {ty} out of range")));
        };
        match u32::try_from(k)
            .ok()
            .and_then(|k| entry.vmt.get(k as usize))
        {
            Some(&t) => Ok((t, entry)),
            None => Err(Step::Fatal(format!(
                "virtual method {k} out of range in {}",
                entry.name
            ))),
        }
    }

    /// Entry `k` of the virtual method table of the class `ty` (`VirtualRef` /
    /// `ClassVirtualRef`; `docs/spec/objects.md` 14.9): a function, or a built-in body kept as it
    /// is. An abstract method is `EAbstractError`.
    fn table_entry(&self, ty: u32, k: Slot) -> Result<u32, Step> {
        let (t, entry) = self.table_slot(ty, k)?;
        match t {
            bytecode::VMT_ABSTRACT => Err(Step::Error(abstract_called(entry, k as u32))),
            t if t < bytecode::VMT_BUILTIN && (t as usize) >= self.module.funcs.len() => {
                Err(Step::Fatal(format!(
                    "virtual method {k} of {} is not callable",
                    entry.name
                )))
            }
            t => Ok(t),
        }
    }

    /// `Assigned` of a callable value (`docs/spec/objects.md` 9.5): not `nil`, and a method's
    /// instance neither freed nor being destroyed.
    fn callable_assigned(&self, v: Slot) -> Result<bool, BadRef> {
        if v == 0 {
            return Ok(false);
        }
        let c = self.heap.callable(v)?;
        if c.kind != heap::CallableKind::Method {
            return Ok(true);
        }
        match self.heap.weak_target(c.target) {
            Ok(h) => Ok(h != 0),
            // `weak_target` reads `nil` and a freed instance as 0; what is left is forged
            Err(heap::BadHandle::Fatal(message)) => Err(BadRef { message }),
            Err(e) => Err(BadRef {
                message: format!("{e:?}"),
            }),
        }
    }

    /// `=` of two callable values (`docs/spec/objects.md` 9.5): with `nil` on a side, whether the
    /// other is not assigned; otherwise the same block, or the same variant, function and target
    /// (a method's handle compared as it is, not checked).
    fn callable_equal(&self, a: Slot, b: Slot) -> Result<bool, BadRef> {
        if a == 0 || b == 0 {
            let other = if a == 0 { b } else { a };
            return Ok(!self.callable_assigned(other)?);
        }
        self.callable_key_eq(a, b)
    }

    /// `CallableKeyEq`: the key equality of two callable values (`docs/spec/equality.md` 2.7).
    /// Like [`Isolate::callable_equal`], except that only `nil` equals `nil`: a method of a freed
    /// instance is compared by its handle as it is.
    fn callable_key_eq(&self, a: Slot, b: Slot) -> Result<bool, BadRef> {
        if a == b {
            return Ok(true);
        }
        if a == 0 || b == 0 {
            return Ok(false);
        }
        let (x, y) = (self.heap.callable(a)?, self.heap.callable(b)?);
        // Two closures are equal only as the same block
        Ok(x.kind != heap::CallableKind::Closure && x == y)
    }

    /// `CallableHash`: the key hash of a callable value (`docs/spec/equality.md` 2.7), alike for
    /// the values [`Isolate::callable_key_eq`] takes as equal. `nil` is `HashInt(0)`, a closure
    /// `HashInt` of its block; the other variants mix the function and the target into `HashInt`
    /// of the variant's number (`docs/spec/bytecode.md` 2.21).
    fn callable_hash(&self, v: Slot) -> Result<Slot, BadRef> {
        if v == 0 {
            return Ok(self.hash_keys.int(0));
        }
        let c = self.heap.callable(v)?;
        let variant: u64 = match c.kind {
            heap::CallableKind::Closure => return Ok(self.hash_keys.int(v as u64)),
            heap::CallableKind::Routine => 0,
            heap::CallableKind::Method => 1,
            heap::CallableKind::ClassMethod => 2,
        };
        let h = self.hash_keys.int(variant);
        let h = self.hash_keys.mix(h as u64, u64::from(c.func));
        Ok(self.hash_keys.mix(h as u64, c.target as u64))
    }

    /// Runs the built-in body `target` (`0xFFFFFF00 + k`; `docs/spec/objects.md` 14.3) over
    /// `Self` in `rA` (a class reference with `class`), without a frame. Returns whether it
    /// wrote a result to `rA` (replacing `Self`, which it borrowed).
    fn builtin_body(&mut self, target: u32, ra: usize, class: bool) -> Result<bool, Step> {
        match target {
            bytecode::VMT_BUILTIN_NOTHING => Ok(false),
            bytecode::VMT_BUILTIN_CLASS_NAME if !class => {
                // `ToString` of `TObject`: the class name
                let x = self.class_name_string(self.task.regs[ra])?;
                self.task.regs[ra] = x;
                Ok(true)
            }
            bytecode::VMT_BUILTIN_MESSAGE if !class => {
                // `ToString` of `Exception`: `Message` (`docs/spec/objects.md` 14.8). The
                // verifier gives it to exception classes only
                let msg = self
                    .heap
                    .exc_slot(self.task.regs[ra], bytecode::EXC_SLOT_MESSAGE)
                    .map_err(|e| Step::Fatal(e.message))?
                    .unwrap_or(0);
                self.heap.add_ref(msg).map_err(|e| Step::Fatal(e.message))?;
                self.task.regs[ra] = msg;
                Ok(true)
            }
            bytecode::VMT_BUILTIN_EQUALS if !class => {
                // `Equals` of `TObject`: the same instance (`docs/spec/equality.md` 3.1, 8.2). The
                // argument `Obj` follows `Self`; both are borrowed
                let obj = self
                    .task
                    .regs
                    .get(ra + 1)
                    .copied()
                    .ok_or_else(|| Step::Fatal("`Equals` has no argument register".into()))?;
                self.task.regs[ra] = Slot::from(self.task.regs[ra] == obj);
                Ok(true)
            }
            bytecode::VMT_BUILTIN_HASH if !class => {
                // `GetHashCode` of `TObject`: `HashInt` of the handle (`docs/spec/equality.md` 3.1)
                self.task.regs[ra] = self.hash_keys.int(self.task.regs[ra] as u64);
                Ok(true)
            }
            _ => Err(Step::Fatal(format!(
                "built-in body {target:#x} is not callable"
            ))),
        }
    }

    /// The function `CallMethod` / `CallVirtual A Bx` calls (`docs/spec/objects.md` 14.3), after
    /// checking `Self` (`rA`; 14.4): `Bx` itself, or entry `Bx` of the instance's class's table.
    /// `None` when the entry is a built-in body, run here without a frame (its result, if any,
    /// in `rA`).
    fn method_callee(&mut self, opc: u8, ra: usize, bx: u32) -> Result<Option<u32>, Step> {
        let ty = if opc == op::word::CALL_VIRTUAL_CLASS {
            // `Self` is a class reference (`docs/spec/objects.md` 14.3)
            self.class_of_ref(self.task.regs[ra])?
        } else {
            self.heap
                .obj_class(self.task.regs[ra])
                .map_err(|e| RefError::from(e).into_step())?
        };
        if opc == op::word::CALL_METHOD {
            return Ok(Some(bx));
        }
        let Some(entry) = self.heap.type_entry(ty) else {
            return Err(Step::Fatal(format!("type {ty} out of range")));
        };
        let target = if opc == op::word::CALL_INTF {
            // The entry of the call target's interface in the class's interface table
            // (`docs/spec/objects.md` 14.7); the verifier checked the target
            let (intf, k) = self.module.intf_calls[bx as usize];
            match entry.intf_entry(intf) {
                Some(e) => e.funcs[k as usize],
                None => {
                    return Err(Step::Fatal(format!(
                        "{} does not implement interface type {intf}",
                        entry.name
                    )));
                }
            }
        } else {
            match entry.vmt.get(bx as usize) {
                Some(&t) => t,
                None => {
                    return Err(Step::Fatal(format!(
                        "virtual method {bx} out of range in {}",
                        entry.name
                    )));
                }
            }
        };
        match target {
            t if t < bytecode::VMT_BUILTIN => Ok(Some(t)),
            bytecode::VMT_BUILTIN_NOTHING
            | bytecode::VMT_BUILTIN_CLASS_NAME
            | bytecode::VMT_BUILTIN_MESSAGE
            | bytecode::VMT_BUILTIN_EQUALS
            | bytecode::VMT_BUILTIN_HASH
                if target == bytecode::VMT_BUILTIN_NOTHING
                    || opc != op::word::CALL_VIRTUAL_CLASS =>
            {
                self.builtin_body(target, ra, opc == op::word::CALL_VIRTUAL_CLASS)?;
                Ok(None)
            }
            bytecode::VMT_ABSTRACT if opc == op::word::CALL_INTF => {
                // An abstract method implementing an interface's method (`docs/spec/objects.md`
                // 7.3): the interface table does not keep the method's name, so the interface
                // names it
                let (intf, _) = self.module.intf_calls[bx as usize];
                let name = self
                    .heap
                    .type_entry(intf)
                    .map_or("?", |t| class_decl_name(&t.name));
                Err(Step::Error(rt(
                    exc::ABSTRACT_ERROR,
                    format!("abstract method called through interface `{name}`"),
                )))
            }
            bytecode::VMT_ABSTRACT => {
                // An instance of an abstract class made through a class reference
                // (`docs/spec/objects.md` 3.1): the name of the abstract method
                Err(Step::Error(abstract_called(entry, bx)))
            }
            _ => Err(Step::Fatal(format!(
                "virtual method {bx} of {} is not callable",
                entry.name
            ))),
        }
    }

    /// The class (type index) a class reference points to (`docs/spec/objects.md` 14.1: the type
    /// index + 1). `nil` is `EAccessViolation`; a value that is not a class is `Fatal`.
    fn class_of_ref(&self, r: Slot) -> Result<u32, Step> {
        if r == 0 {
            return Err(Step::Error(rt(exc::ACCESS_VIOLATION, NIL_ACCESS)));
        }
        let ty =
            u32::try_from(r - 1).map_err(|_| Step::Fatal(format!("bad class reference {r}")))?;
        match self.heap.type_entry(ty) {
            Some(e) if e.kind == bytecode::TYPE_CLASS => Ok(ty),
            _ => Err(Step::Fatal(format!("bad class reference {r}"))),
        }
    }

    /// The interface whose type index the register value `v` is (the operand of `IsIntf` /
    /// `AsIntf` / `ClassSupports`; `docs/spec/objects.md` 14.7). The code generator only passes an
    /// interface's type index; anything else (which the verifier cannot see in a register) is
    /// `Fatal`.
    fn intf_operand(&self, v: Slot) -> Result<u32, Step> {
        match u32::try_from(v) {
            Ok(t)
                if self
                    .heap
                    .type_entry(t)
                    .is_some_and(|e| e.kind == bytecode::TYPE_INTERFACE) =>
            {
                Ok(t)
            }
            _ => Err(Step::Fatal(format!("bad interface type index {v}"))),
        }
    }

    /// The declared name of the class `ty` as a new string (`ClassRefName`).
    fn type_name_string(&mut self, ty: u32) -> Result<Slot, Step> {
        let Some(entry) = self.heap.type_entry(ty) else {
            return Err(Step::Fatal(format!("type {ty} out of range")));
        };
        let units: Vec<u16> = class_decl_name(&entry.name).encode_utf16().collect();
        let s = PStr::from_units(&units)
            .map_err(|_| Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")))?;
        self.alloc_string(s).map_err(Step::Error)
    }

    /// `Intrinsic A B C` (`docs/spec/bytecode.md` 2.19): the intrinsic `id` (`C`) with `rA` and the
    /// arguments from `rB` (`pc` is this instruction, `next` the one after it).
    fn intrinsic(&mut self, id: u8, ra: usize, rb: usize, pc: usize, next: usize) -> Step {
        // The second argument (`r(B+1)`; the `rC` of the instructions these were before)
        let rc = rb + 1;
        let regs = &mut self.task.regs;
        match id {
            op::intr::CLASS_NAME => {
                // `docs/spec/objects.md` 14.3: the declared name of the instance's class
                let h = regs[rb];
                match self.class_name_string(h) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(step) => return step,
                }
            }
            op::intr::NEW_OF => {
                // `docs/spec/objects.md` 3.1: an instance of the class `rB` points to (an abstract
                // class too). Like `New`, the old value of `rA` is let go of by its contents
                let ty = match self.class_of_ref(self.task.regs[rb]) {
                    Ok(t) => t,
                    Err(step) => return step,
                };
                match self.heap.alloc_obj(ty) {
                    Ok(Ok(h)) => {
                        let old = std::mem::replace(&mut self.task.regs[ra], h);
                        try_ref!(self.heap.release_obj(old));
                    }
                    Ok(Err(_)) => return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory")),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::intr::REF_COUNT => match self.heap.obj_ref_count(self.task.regs[rb]) {
                // `docs/spec/objects.md` chapter 5 (0 for an instance managed by hand)
                Ok(n) => self.task.regs[ra] = Slot::from(n),
                Err(e) => return RefError::from(e).into_step(),
            },
            op::intr::CLASS_TYPE => match self.heap.obj_class(self.task.regs[rb]) {
                Ok(ty) => self.task.regs[ra] = Slot::from(ty) + 1,
                Err(e) => return RefError::from(e).into_step(),
            },
            op::intr::CLASS_REF_NAME => {
                let x = match self
                    .class_of_ref(self.task.regs[rb])
                    .and_then(|ty| self.type_name_string(ty))
                {
                    Ok(x) => x,
                    Err(step) => return step,
                };
                try_ref!(self.put(ra, x));
            }
            op::intr::CLASS_PARENT => {
                let ty = match self.class_of_ref(self.task.regs[rb]) {
                    Ok(t) => t,
                    Err(step) => return step,
                };
                let parent = self.heap.type_entry(ty).and_then(|e| e.parent);
                self.task.regs[ra] = parent.map_or(0, |p| Slot::from(p) + 1);
            }
            op::intr::CLASS_INHERITS => {
                // `nil` on either side is `False` (`docs/spec/objects.md` chapter 5)
                let (a, b) = (self.task.regs[rb], self.task.regs[rc]);
                let mut yes = false;
                if a != 0 && b != 0 {
                    let (x, y) = match (self.class_of_ref(a), self.class_of_ref(b)) {
                        (Ok(x), Ok(y)) => (x, y),
                        (Err(step), _) | (_, Err(step)) => return step,
                    };
                    yes = self.heap.class_inherits(x, y);
                }
                self.task.regs[ra] = Slot::from(yes);
            }
            op::intr::AS_CLASS_REF => {
                // `docs/spec/objects.md` 2.8, 14.3: `nil` passes; `rB` is the target's type index
                let r = self.task.regs[ra];
                if r != 0 {
                    let class = match self.class_of_ref(r) {
                        Ok(t) => t,
                        Err(step) => return step,
                    };
                    // The code generator only passes a class's type index
                    let target = match u32::try_from(self.task.regs[rb]) {
                        Ok(t)
                            if self
                                .heap
                                .type_entry(t)
                                .is_some_and(|e| e.kind == bytecode::TYPE_CLASS) =>
                        {
                            t
                        }
                        _ => return Step::Fatal(format!("bad type index {}", self.task.regs[rb])),
                    };
                    if !self.heap.class_inherits(class, target) {
                        return Step::Error(self.invalid_cast(class, target));
                    }
                }
            }
            op::intr::IS_INTF => {
                // `docs/spec/objects.md` 7.6, 14.7: `rA` is an uncounted copy of the handle
                let ty = match self.intf_operand(self.task.regs[rb]) {
                    Ok(t) => t,
                    Err(step) => return step,
                };
                let h = self.task.regs[ra];
                let yes = h != 0
                    && match self.heap.obj_class(h) {
                        Ok(class) => self.heap.class_implements(class, ty),
                        Err(e) => return RefError::from(e).into_step(),
                    };
                self.task.regs[ra] = Slot::from(yes);
            }
            op::intr::AS_INTF => {
                // `docs/spec/objects.md` 7.6, 14.7: `nil` passes; `rA` is left as it is
                let ty = match self.intf_operand(self.task.regs[rb]) {
                    Ok(t) => t,
                    Err(step) => return step,
                };
                let h = self.task.regs[ra];
                if h != 0 {
                    let class = match self.heap.obj_class(h) {
                        Ok(c) => c,
                        Err(e) => return RefError::from(e).into_step(),
                    };
                    if !self.heap.class_implements(class, ty) {
                        return Step::Error(self.invalid_intf_cast(class, ty));
                    }
                }
            }
            op::intr::CLASS_SUPPORTS => {
                // `Supports(C, IA)` (`docs/spec/objects.md` 7.6, 14.7): a `nil` class reference
                // is `False`
                let ty = match self.intf_operand(self.task.regs[rc]) {
                    Ok(t) => t,
                    Err(step) => return step,
                };
                let r = self.task.regs[rb];
                let mut yes = false;
                if r != 0 {
                    let class = match self.class_of_ref(r) {
                        Ok(c) => c,
                        Err(step) => return step,
                    };
                    yes = self.heap.class_implements(class, ty);
                }
                self.task.regs[ra] = Slot::from(yes);
            }
            op::intr::FREE | op::intr::DESTROY => {
                // `docs/spec/objects.md` 3.2: `Free` of `nil` does nothing, `Destroy` of `nil` is
                // `EAccessViolation`. An instance of a reference counted class (through `TObject`)
                // is `EInvalidOperation`. The destruction procedure runs in a frame of its own (3.3)
                let h = regs[ra];
                if h != 0 || id == op::intr::DESTROY {
                    match self.heap.obj_counted(h) {
                        Ok(true) => {
                            let name = self.instance_class_name(h);
                            return Step::Error(rt(
                                exc::INVALID_OPERATION,
                                format!(
                                    "cannot free a reference counted object of class `{name}`; assign `nil` instead"
                                ),
                            ));
                        }
                        Ok(false) => {}
                        Err(e) => return RefError::from(e).into_step(),
                    }
                    return self.start_destroy(h, next, false);
                }
            }
            op::intr::DESTROY_FAILED => {
                // The clean-up after a constructor's exception (`docs/spec/objects.md` 3.1, 14.3):
                // the creation's reference of a reference counted instance is dropped without
                // counting down to its destruction, which comes now
                let h = std::mem::take(&mut regs[ra]);
                if let Err(e) = self.heap.obj_disown(h) {
                    return RefError::from(e).into_step();
                }
                return self.start_destroy(h, next, true);
            }
            // `docs/spec/objects.md` 14.9
            op::intr::METHOD_REF
            | op::intr::VIRTUAL_REF
            | op::intr::CLASS_METHOD_REF
            | op::intr::CLASS_VIRTUAL_REF => {
                let class_ref =
                    matches!(id, op::intr::CLASS_METHOD_REF | op::intr::CLASS_VIRTUAL_REF);
                let target = self.task.regs[rb];
                // The receiver is checked when the value is made (9.3)
                let ty = if class_ref {
                    self.class_of_ref(target)
                } else {
                    self.heap
                        .obj_class(target)
                        .map_err(|e| RefError::from(e).into_step())
                };
                let ty = match ty {
                    Ok(t) => t,
                    Err(step) => return step,
                };
                let k = self.task.regs[rc];
                let func = if matches!(id, op::intr::VIRTUAL_REF | op::intr::CLASS_VIRTUAL_REF) {
                    match self.table_entry(ty, k) {
                        Ok(f) => f,
                        Err(step) => return step,
                    }
                } else {
                    match u32::try_from(k)
                        .ok()
                        .filter(|f| (*f as usize) < self.module.funcs.len())
                    {
                        Some(f) => f,
                        None => return Step::Fatal(format!("bad function index {k}")),
                    }
                };
                let kind = if class_ref {
                    heap::CallableKind::ClassMethod
                } else {
                    heap::CallableKind::Method
                };
                let c = heap::Callable { kind, func, target };
                let r = try_oom!(self.heap.alloc_callable(c));
                try_ref!(self.put(ra, r));
            }
            op::intr::ROUTINE_REF => {
                let v = self.task.regs[rb];
                let r = if v == 0 {
                    0
                } else {
                    let Some(func) = u32::try_from(v - 1)
                        .ok()
                        .filter(|f| (*f as usize) < self.module.funcs.len())
                    else {
                        return Step::Fatal(format!("bad procedural value {v}"));
                    };
                    let c = heap::Callable {
                        kind: heap::CallableKind::Routine,
                        func,
                        target: 0,
                    };
                    try_oom!(self.heap.alloc_callable(c))
                };
                try_ref!(self.put(ra, r));
            }
            op::intr::CLOSURE_WITH => {
                let f = self.task.regs[rc];
                let Some(func) = u32::try_from(f)
                    .ok()
                    .filter(|f| (*f as usize) < self.module.funcs.len())
                else {
                    return Step::Fatal(format!("bad function index {f}"));
                };
                let caps = match self.heap.closure_caps(self.task.regs[rb]) {
                    Ok(c) => try_oom!(heap::Caps::new(c)),
                    Err(e) => return Step::Fatal(e.message),
                };
                for &cell in caps.iter() {
                    try_ref!(self.heap.add_ref(cell));
                }
                let r = match self.heap.alloc_closure(func, caps) {
                    Ok(r) => r,
                    Err(OutOfMemory) => {
                        // The cells counted for it are let go of again, read from the block
                        let caps: Vec<i64> = match self.heap.closure_caps(self.task.regs[rb]) {
                            Ok(c) => c.to_vec(),
                            Err(e) => return Step::Fatal(e.message),
                        };
                        for cell in caps {
                            try_ref!(self.heap.release(cell));
                        }
                        return Step::Error(oom());
                    }
                };
                try_ref!(self.put(ra, r));
            }
            op::intr::CALLABLE_ASSIGNED => match self.callable_assigned(self.task.regs[rb]) {
                Ok(yes) => self.task.regs[ra] = Slot::from(yes),
                Err(e) => return Step::Fatal(e.message),
            },
            op::intr::CALLABLE_EQUAL => {
                match self.callable_equal(self.task.regs[rb], self.task.regs[rc]) {
                    Ok(yes) => self.task.regs[ra] = Slot::from(yes),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            // `docs/spec/equality.md` 8.1
            op::intr::OBJ_ALIVE => {
                // A live instance, or one being destroyed; `nil`, a freed handle and a tombstone
                // are not alive
                regs[ra] = match self.heap.obj_index(regs[rb]) {
                    Ok(_) => 1,
                    Err(heap::BadHandle::Nil | heap::BadHandle::Freed) => 0,
                    Err(heap::BadHandle::Fatal(message)) => return Step::Fatal(message),
                };
            }
            op::intr::HASH_UTF8 => {
                let h = match self.heap.u8_hash(regs[rb], &self.hash_keys) {
                    Ok(h) => h,
                    Err(e) => return Step::Fatal(e.message),
                };
                self.task.regs[ra] = Slot::from(h);
            }
            op::intr::HASH_SET_BIG => {
                let h = match self.heap.set_slots(regs[rb]) {
                    Ok(words) => self.hash_keys.set_words(words),
                    Err(e) => return Step::Fatal(e.message),
                };
                self.task.regs[ra] = h;
            }
            op::intr::CALLABLE_KEY_EQ => {
                match self.callable_key_eq(self.task.regs[rb], self.task.regs[rc]) {
                    Ok(yes) => self.task.regs[ra] = Slot::from(yes),
                    Err(e) => return Step::Fatal(e.message),
                }
            }
            op::intr::CALLABLE_HASH => match self.callable_hash(self.task.regs[rb]) {
                Ok(h) => self.task.regs[ra] = h,
                Err(e) => return Step::Fatal(e.message),
            },
            op::intr::EXCEPT_OBJECT => {
                // `docs/spec/objects.md` 8.4, 14.7
                let x = self.except_object();
                try_ref!(self.put_shared(ra, x));
            }
            op::intr::DTOR_ENTER => {
                // `docs/spec/objects.md` 14.8: the destructor of the class `rB` started
                let h = regs[ra];
                let class = regs[rb];
                if let Err(e) = self.heap.dtor_enter(h, class as u32) {
                    return RefError::from(e).into_step();
                }
            }
            op::intr::STR_TO_F64_IO | op::intr::STR_TO_F32_IO | op::intr::STR_TO_CURR_IO => {
                // `Read` / `ReadLn` of a real or a `Currency` (`docs/spec/semantics.md` 6.2): the
                // `StrToFloat` syntax, trailing blanks allowed
                let s = get_str!(self.heap, regs[rb]);
                let mut units = try_oom!(s.units());
                while units.last().is_some_and(|u| strconv::is_blank(*u)) {
                    units.pop();
                }
                let v = numfmt::parse_f64(&units).and_then(|v| match id {
                    op::intr::STR_TO_F32_IO => Some(f32_slot(v as f32)),
                    op::intr::STR_TO_CURR_IO => numfmt::f64_to_curr(v),
                    _ => Some(f64_slot(v)),
                });
                match v {
                    Some(v) => regs[ra] = v,
                    None => {
                        let text = try_oom!(s.to_utf8());
                        return Step::Error(input::input_error(format!(
                            "invalid floating-point input: '{text}'"
                        )));
                    }
                }
            }
            op::intr::INT_TO_HEX => {
                let digits = regs[rc];
                let text = format!("{:X}", regs[rb] as u64);
                let pad = (digits.max(0) as u64).saturating_sub(text.len() as u64);
                if pad > (self.heap.str_room().saturating_sub(text.len())) as u64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                let mut units = Vec::new();
                try_oom!(heap::try_reserve(&mut units, pad as usize + text.len()));
                units.extend(std::iter::repeat_n(u16::from(b'0'), pad as usize));
                units.extend(text.encode_utf16());
                match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::STR_TO_BOOL => {
                let s = get_str!(self.heap, regs[rb]);
                match text::parse_bool(&try_oom!(s.units())) {
                    Some(b) => regs[ra] = b as i64,
                    None => {
                        let text = try_oom!(s.to_utf8());
                        return Step::Error(convert_error(text, |t| {
                            format!("'{t}' is not a valid boolean value")
                        }));
                    }
                }
            }
            op::intr::BOOL_STR => {
                let text = match (regs[rb] != 0, regs[rc] != 0) {
                    (true, false) => "-1",
                    (false, false) => "0",
                    (true, true) => "True",
                    (false, true) => "False",
                };
                match self.alloc_string(try_oom!(PStr::from_text(text))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::SAME_TEXT => {
                let a = get_str!(self.heap, regs[rb]);
                let b = get_str!(self.heap, regs[rc]);
                regs[ra] = try_oom!(text::same_text_str(a, b)) as i64;
            }
            op::intr::COMPARE_TEXT => {
                let a = try_oom!(get_str!(self.heap, regs[rb]).units());
                let b = try_oom!(get_str!(self.heap, regs[rc]).units());
                regs[ra] = text::compare_text(&a, &b) as i64;
            }
            op::intr::STR_UPPER | op::intr::STR_LOWER => {
                let s = try_oom!(get_str!(self.heap, regs[rb]).units());
                let full = regs[rc] == bytecode::CASE_MAPPING_FULL;
                let units = try_oom!(text::convert_case(&s, id == op::intr::STR_UPPER, full));
                if units.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::ANSI_SAME_TEXT | op::intr::ANSI_COMPARE_TEXT => {
                let a = try_oom!(get_str!(self.heap, regs[rb]).units());
                let b = try_oom!(get_str!(self.heap, regs[rb + 1]).units());
                let upper = regs[rb + 2] == bytecode::CASE_FOLD_UPPER;
                let order = text::compare_folded(&a, &b, upper);
                regs[ra] = if id == op::intr::ANSI_SAME_TEXT {
                    order.is_eq() as i64
                } else {
                    order as i64
                };
            }
            op::intr::STR_REPLACE => {
                let s = try_oom!(get_str!(self.heap, regs[rb]).units());
                let old = try_oom!(get_str!(self.heap, regs[rb + 1]).units());
                let new = try_oom!(get_str!(self.heap, regs[rb + 2]).units());
                let flags = regs[rb + 3];
                let all = flags & (1 << bytecode::REPLACE_FLAG_ALL) != 0;
                let ignore_case = flags & (1 << bytecode::REPLACE_FLAG_IGNORE_CASE) != 0;
                let units = try_oom!(text::replace(&s, &old, &new, all, ignore_case));
                if units.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::STR_OF_CHAR => {
                let count = regs[rc];
                if count > self.heap.str_room() as i64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                let mut units = Vec::new();
                try_oom!(heap::try_fill(
                    &mut units,
                    regs[rb] as u16,
                    count.max(0) as usize
                ));
                match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::QUOTED_STR => {
                let units = try_oom!(get_str!(self.heap, regs[rb]).units());
                let units = try_oom!(text::quoted(&units));
                if units.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::TRY_STR_TO_INT => {
                let s = get_str!(self.heap, regs[rb]);
                match strconv::parse_int(&try_oom!(s.units()), false) {
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
            op::intr::F64_TO_STR | op::intr::F32_TO_STR | op::intr::CURR_TO_STR => {
                let v = regs[rb];
                let text = match id {
                    op::intr::F64_TO_STR => numfmt::f64_to_str(slot_f64(v)),
                    op::intr::F32_TO_STR => numfmt::f32_to_str(slot_f32(v)),
                    _ => numfmt::curr_to_str(v),
                };
                match self.alloc_string(try_oom!(PStr::from_text(&text))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::F64_TO_FIXED | op::intr::CURR_TO_FIXED => {
                let (v, d) = (regs[rb], regs[rc]);
                // A finite value has at least `d` digits after the point
                let finite = id == op::intr::CURR_TO_FIXED || slot_f64(v).is_finite();
                if finite && d > self.heap.str_room() as i64 {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                // Written to a fallible buffer: a precision within the room can still ask for
                // gigabytes of digits
                let least = if finite { d.max(0) as usize } else { 0 };
                let room = if least > 0 {
                    least.saturating_add(numfmt::DIGITS_EXTRA)
                } else {
                    0
                };
                let mut text = try_oom!(heap::TextBuf::with_room(room));
                let written = if id == op::intr::F64_TO_FIXED {
                    numfmt::f64_fixed(&mut text, slot_f64(v), d)
                } else {
                    numfmt::curr_fixed(&mut text, v, d)
                };
                if written.is_err() {
                    return Step::Error(oom());
                }
                let units = try_oom!(text.finish());
                match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::STR_TO_F64 => {
                let s = get_str!(self.heap, regs[rb]);
                match numfmt::parse_f64(&try_oom!(s.units())) {
                    Some(v) => regs[ra] = f64_slot(v),
                    None => {
                        let text = try_oom!(s.to_utf8());
                        return Step::Error(convert_error(text, |t| {
                            format!("'{t}' is not a valid floating-point value")
                        }));
                    }
                }
            }
            op::intr::READ_WORD
            | op::intr::READ_REST
            | op::intr::READ_CHAR
            | op::intr::READ_LN_BEGIN
            | op::intr::READ_LN_END
            | op::intr::IN_EOF
            | op::intr::IN_EOLN
            | op::intr::FLUSH_OUT => return self.input_op(id, ra, pc as u32, next as u32),
            op::intr::ARG_COUNT => regs[ra] = self.args.len().saturating_sub(1) as i64,
            op::intr::ARG_GET => {
                let i = regs[rb];
                let text = if i >= 0 && (i as usize) < self.args.len() {
                    self.args[i as usize].clone()
                } else {
                    String::new()
                };
                if text.encode_utf16().count() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_string(try_oom!(PStr::from_text(&text))) {
                    Ok(x) => try_ref!(self.put(ra, x)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::U8_SET_LEN => {
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
                    let mut bytes = Vec::new();
                    try_oom!(heap::try_reserve(&mut bytes, n));
                    bytes.extend_from_slice(&s[..n.min(s.len())]);
                    bytes.resize(n, 0);
                    match self.alloc_u8(bytes) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::intr::STR_INSERT => {
                let src = try_oom!(get_str!(self.heap, regs[rb]).units());
                let s = try_oom!(get_str!(self.heap, regs[ra]).units());
                if !src.is_empty() {
                    if s.len() + src.len() > MAX_STRING_LEN {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    let pos = insert_pos(regs[rb + 1], s.len());
                    let units = try_oom!(try_join(&[&s[..pos], &src[..], &s[pos..]]));
                    match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::intr::STR_DELETE => {
                let s = try_oom!(get_str!(self.heap, regs[ra]).units());
                if let Some((start, end)) = delete_range(regs[rb], regs[rb + 1], s.len()) {
                    let units = try_oom!(try_join(&[&s[..start], &s[end..]]));
                    match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::intr::U8_INSERT => {
                let src = try_oom!(try_join(&[get_u8!(self.heap, regs[rb])]));
                let s = get_u8!(self.heap, regs[ra]);
                if !src.is_empty() {
                    if s.len() + src.len() > MAX_STRING_LEN {
                        return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                    }
                    let pos = insert_pos(regs[rb + 1], s.len());
                    if let Err(e) = check_u8_boundary(s, pos) {
                        return Step::Error(e);
                    }
                    let bytes = try_oom!(try_join(&[&s[..pos], &src[..], &s[pos..]]));
                    match self.alloc_u8(bytes) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::intr::U8_DELETE => {
                let s = get_u8!(self.heap, regs[ra]);
                if let Some((start, end)) = delete_range(regs[rb], regs[rb + 1], s.len()) {
                    if let Err(e) =
                        check_u8_boundary(s, start).and_then(|_| check_u8_boundary(s, end))
                    {
                        return Step::Error(e);
                    }
                    let bytes = try_oom!(try_join(&[&s[..start], &s[end..]]));
                    match self.alloc_u8(bytes) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            // `docs/spec/bytecode.md` 2.22
            op::intr::ARRAY_MOVE_RANGE => {
                let (arr, from, to, count) = (regs[ra], regs[rb], regs[rb + 1], regs[rb + 2]);
                if let Err(e) = self.array_move_range(arr, from, to, count) {
                    return e.into_step();
                }
            }
            // `docs/spec/bytecode.md` 2.23
            op::intr::HASH_RELINK => try_op!(self.op_hash_relink(ra, rb)),
            op::intr::IS_REF_OBJ => {
                // A live instance of a reference counted class (one being destroyed too); `nil`
                // and a freed handle are not, as an instance managed by hand (`docs/spec/objects.md`
                // 4.2)
                regs[ra] = match self.heap.obj_counted(regs[rb]) {
                    Ok(counted) => Slot::from(counted),
                    Err(heap::BadHandle::Nil | heap::BadHandle::Freed) => 0,
                    Err(heap::BadHandle::Fatal(message)) => return Step::Fatal(message),
                };
            }
            op::intr::IS_OVERRIDDEN => {
                // Whether the entry `r(B+1)` of the table of the instance's class is another
                // function than `r(B+2)` (an abstract entry or a built-in body is another)
                let (h, k, func) = (
                    self.task.regs[rb],
                    self.task.regs[rb + 1],
                    self.task.regs[rb + 2],
                );
                let ty = match self.heap.obj_class(h) {
                    Ok(t) => t,
                    Err(e) => return RefError::from(e).into_step(),
                };
                let t = match self.table_slot(ty, k) {
                    Ok((t, _)) => t,
                    Err(step) => return step,
                };
                self.task.regs[ra] = Slot::from(Slot::from(t) != func);
            }
            op::intr::PLACE_BOX => try_op!(self.op_place_box(ra, rb)),
            op::intr::STR_TRIM_LEFT => return self.trim(ra, rb, true, false, next),
            op::intr::STR_TRIM_RIGHT => return self.trim(ra, rb, false, true, next),
            // Instructions moved here to free their numbers (M4-109)
            op::intr::SLICE_RANGE => {
                let (start, count) = slice_range(regs[ra], regs[rb], regs[rb + 1], regs[rb + 2]);
                regs[rb] = start;
                regs[rb + 1] = count;
            }
            op::intr::STR_TO_INT | op::intr::STR_TO_INT64 => {
                let s = get_str!(self.heap, regs[rb]);
                let units = try_oom!(s.units());
                let (lo, hi) = if id == op::intr::STR_TO_INT {
                    (i32::MIN as i128, i32::MAX as i128)
                } else {
                    (i64::MIN as i128, i64::MAX as i128)
                };
                match strconv::parse_int(&units, false) {
                    Some(v) if v >= lo && v <= hi => regs[ra] = v as i64,
                    _ => {
                        let text = try_oom!(s.to_utf8());
                        return Step::Error(convert_error(text, |t| {
                            format!("'{t}' is not a valid integer")
                        }));
                    }
                }
            }
            op::intr::STR_TO_INT_DEF => {
                let s = get_str!(self.heap, regs[rb]);
                let units = try_oom!(s.units());
                let def = regs[rb + 1];
                regs[ra] = match strconv::parse_int(&units, false) {
                    Some(v) if v >= i32::MIN as i128 && v <= i32::MAX as i128 => v as i64,
                    _ => def,
                };
            }
            op::intr::STR_TRIM => return self.trim(ra, rb, true, true, next),
            op::intr::NEG_CURR => match regs[rb].checked_neg() {
                Some(v) => regs[ra] = v,
                None => return Step::Error(rt(exc::OVERFLOW, CURRENCY_OVERFLOW)),
            },
            op::intr::INT_TO_CURR | op::intr::UINT_TO_CURR | op::intr::F64_TO_CURR => {
                let x = regs[rb];
                let v = match id {
                    op::intr::INT_TO_CURR => numfmt::int_to_curr(x as i128),
                    op::intr::UINT_TO_CURR => numfmt::int_to_curr(x as u64 as i128),
                    _ => numfmt::f64_to_curr(slot_f64(x)),
                };
                match v {
                    Some(v) => regs[ra] = v,
                    None => return Step::Error(rt(exc::OVERFLOW, CURRENCY_OVERFLOW)),
                }
            }
            op::intr::CURR_TO_F64 => regs[ra] = f64_slot(numfmt::curr_to_f64(regs[rb])),
            op::intr::CURR_TO_F32 => regs[ra] = f32_slot(numfmt::curr_to_f32(regs[rb])),
            op::intr::TRUNC_CURR => regs[ra] = numfmt::curr_to_int(regs[rb], false),
            op::intr::ROUND_CURR => regs[ra] = numfmt::curr_to_int(regs[rb], true),
            op::intr::STR_SET_LEN => {
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
                    let mut units = Vec::new();
                    try_oom!(heap::try_reserve(&mut units, n));
                    units.extend(s.iter_units().take(n));
                    units.resize(n, 0);
                    match self.alloc_string(try_oom!(PStr::from_units(&units))) {
                        Ok(v) => try_ref!(self.put(ra, v)),
                        Err(e) => return Step::Error(e),
                    }
                }
            }
            op::intr::STR_TO_U8 => {
                let text = try_oom!(get_str!(self.heap, regs[rb]).to_utf8());
                if text.len() > MAX_STRING_LEN {
                    return Step::Error(rt(exc::OUT_OF_MEMORY, "out of memory"));
                }
                match self.alloc_u8(text.into_bytes()) {
                    Ok(v) => try_ref!(self.put(ra, v)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::U8_TO_STR => {
                // Valid UTF-8 has at most as many UTF-16 code units as bytes, so the length fits
                let b = get_u8!(self.heap, regs[rb]);
                let p = try_oom!(PStr::from_text(&String::from_utf8_lossy(b)));
                match self.alloc_string(p) {
                    Ok(v) => try_ref!(self.put(ra, v)),
                    Err(e) => return Step::Error(e),
                }
            }
            op::intr::U8_COPY => {
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
                            let piece = try_oom!(try_join(&[&s[start..end]]));
                            match self.alloc_u8(piece) {
                                Ok(x) => try_ref!(self.put(ra, x)),
                                Err(e) => return Step::Error(e),
                            }
                        }
                    }
                }
            }
            op::intr::U8_POS => {
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
            _ => return Step::Fatal(format!("unknown intrinsic {id}")),
        }
        self.task.frames.last_mut().expect("a frame runs").pc = next as u32;
        Step::Continue
    }

    /// `Trim` / `TrimLeft` / `TrimRight` of `rB` into `rA`: cut the blanks at the start (`left`)
    /// and at the end (`right`); the value itself when there is nothing to cut.
    fn trim(&mut self, ra: usize, rb: usize, left: bool, right: bool, next: usize) -> Step {
        let regs = &mut self.task.regs;
        let s = get_str!(self.heap, regs[rb]);
        let units = try_oom!(s.units());
        let (start, end) = text::trimmed(&units, left, right);
        if start == 0 && end == units.len() {
            let same = regs[rb];
            try_ref!(self.put_shared(ra, same));
        } else {
            match self.alloc_string(try_oom!(PStr::from_units(&units[start..end]))) {
                Ok(x) => try_ref!(self.put(ra, x)),
                Err(e) => return Step::Error(e),
            }
        }
        self.task.frames.last_mut().expect("a frame runs").pc = next as u32;
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
                let text = try_oom!(get_str!(self.heap, self.task.regs[dst as usize]).to_utf8());
                if kind == HostFn::ConsoleWrite {
                    if let Err(e) = self.write_stdout(text.as_bytes()) {
                        return Step::Error(e);
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
                self.task.pending = Some(PendingHost {
                    kind: PendingKind::Write(fut),
                    resume_pc,
                });
            }
            HostFn::EnvGet => {
                let name = try_oom!(get_str!(self.heap, self.task.regs[dst as usize]).to_utf8());
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
                match self.alloc_string(try_oom!(PStr::from_text(&text))) {
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

/// Where a reference slot leads (`docs/spec/bytecode.md` 1.5, 6.1), or a slot on the path of a
/// lent location.
#[derive(Debug, Clone, Copy)]
enum Loc {
    Reg(usize),
    Global(usize),
    /// Slot `1` of the record or array block `0`.
    Slot(Slot, i64),
    /// Slot `1` of the live instance at slab index `0` (`docs/spec/objects.md` 14.3).
    ObjSlot(usize, usize),
    /// The value of a cell (`docs/spec/objects.md` 14.9).
    Cell(Slot),
}

/// A failure of an instruction body: a Pascal exception or an internal error.
enum RefError {
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

/// How many comparisons of `ArraySortRange` (`I96`) take one unit of the budget, past the 1 of
/// the instruction (`docs/spec/bytecode.md` 1.3; `docs/design/vm.md` 3.17).
const SORT_COMPARES_PER_UNIT: u64 = 16;

/// The range of `count` elements at `start` of an array of `len` elements, as `ArrayMoveRange`
/// and `ArraySortRange` check it (`docs/spec/bytecode.md` 2.22): `ERangeError` for a negative
/// count or a range outside the array.
fn check_array_range(start: i64, count: i64, len: usize) -> Result<(), RefError> {
    if count < 0 {
        return Err(RefError::Rt(rt(
            exc::RANGE_ERROR,
            format!("array count {count} is out of range"),
        )));
    }
    let end = i128::from(start) + i128::from(count);
    if start < 0 || end > len as i128 {
        return Err(RefError::Rt(rt(
            exc::RANGE_ERROR,
            format!(
                "array range {start}..{} is out of range 0..{}",
                end - 1,
                len as i64 - 1
            ),
        )));
    }
    Ok(())
}

impl From<BadRef> for RefError {
    fn from(e: BadRef) -> Self {
        RefError::Fatal(e.message)
    }
}

impl Isolate {
    /// Element `i` of a dynamic array or open array block (0 is empty), range-checked.
    fn dyn_slot(&self, container: Slot, i: i64) -> Result<Slot, RefError> {
        let elems = self.heap.elems(container)?;
        match usize::try_from(i).ok().and_then(|i| elems.get(i)) {
            Some(v) => Ok(*v),
            None => Err(RefError::Rt(rt(
                exc::RANGE_ERROR,
                dyn_index_message(i, elems.len()),
            ))),
        }
    }

    /// `ArrayMoveRange` (`docs/spec/bytecode.md` 2.22): the `count` elements of the dynamic array
    /// `arr` (0 is empty) at `from` move to `to`, and the elements between shift into the room
    /// they leave. `ERangeError` for a negative count or a range outside the array; refused
    /// (`Aliased`) when an element that changes place is lent to a `var` parameter.
    fn array_move_range(
        &mut self,
        arr: Slot,
        from: i64,
        to: i64,
        count: i64,
    ) -> Result<(), RefError> {
        if arr != 0 && !self.heap.is_dyn_type(self.heap.agg_type(arr)?) {
            return Err(RefError::Fatal(format!(
                "`ArrayMoveRange` of the heap block {arr}, which is not a dynamic array"
            )));
        }
        let len = self.heap.len(arr)?;
        check_array_range(from, count, len)?;
        check_array_range(to, count, len)?;
        if count == 0 || from == to {
            return Ok(());
        }
        // In range, so they fit in `usize`; in slots, an element of a flat record taking several
        let w = self.heap.width_of(self.heap.agg_type(arr)?);
        let (from, to, count) = (from as usize * w, to as usize * w, count as usize * w);
        let (lo, hi) = (from.min(to), from.max(to) + count);
        for i in lo..hi {
            self.heap.check_slot(arr, i as i64)?;
        }
        self.heap.rotate_slots(arr, lo, hi, count, from < to)?;
        Ok(())
    }

    /// `ArraySortRange A B` (`I96`; `docs/spec/bytecode.md` 2.22): the `r(B+1)` elements of the
    /// dynamic array `rA` (0 is empty) at `rB` sorted stably in place by the comparison `r(B+2)`.
    /// Returns the number of comparisons. The range is checked as `ArrayMoveRange`'s, and every
    /// exception is raised before an element moves.
    fn array_sort_range(&mut self, ra: usize, rb: usize) -> Result<u64, RefError> {
        let (arr, index, count, kind) = (
            self.task.regs[ra],
            self.task.regs[rb],
            self.task.regs[rb + 1],
            self.task.regs[rb + 2],
        );
        if !(bytecode::SORT_INT..=bytecode::SORT_UTF8).contains(&kind) {
            return Err(RefError::Fatal(format!(
                "`ArraySortRange` with the unknown comparison {kind}"
            )));
        }
        if arr != 0 {
            let ty = self.heap.agg_type(arr)?;
            if !self.heap.is_dyn_type(ty) || self.heap.width_of(ty) != 1 {
                return Err(RefError::Fatal(format!(
                    "`ArraySortRange` of the heap block {arr}, which is not a dynamic array of \
                     one-slot elements"
                )));
            }
        }
        check_array_range(index, count, self.heap.len(arr)?)?;
        if count <= 1 {
            return Ok(0);
        }
        // In range, so they fit in `usize`
        let (lo, hi) = (index as usize, (index + count) as usize);
        for i in lo..hi {
            self.heap.check_slot(arr, i as i64)?;
        }
        let compares = self.heap.sort_slots(arr, lo, hi, kind)??;
        #[cfg(test)]
        executed::add(compares);
        Ok(compares)
    }

    /// The register or global variable a reference slot points to (kind 1 or 2;
    /// `docs/spec/bytecode.md` 1.5).
    fn ref_target(&self, slot: Slot) -> Result<Loc, RefError> {
        match deref(slot) {
            (1, i) if i < self.task.regs.len() => Ok(Loc::Reg(i)),
            (2, i) if i < self.globals.len() => Ok(Loc::Global(i)),
            // A cell (`docs/spec/objects.md` 14.9): its block is checked when it is used
            (3, i) => Ok(Loc::Cell(i as Slot)),
            _ => Err(RefError::Fatal("bad reference slot".into())),
        }
    }

    fn loc_get(&self, loc: Loc) -> Result<Slot, RefError> {
        Ok(match loc {
            Loc::Reg(i) => self.task.regs[i],
            Loc::Global(i) => self.globals[i],
            Loc::Slot(b, i) => self
                .heap
                .slot(b, i)?
                .ok_or_else(|| RefError::Fatal("slot out of range".into()))?,
            Loc::ObjSlot(index, i) => self.heap.obj_slot_at(index, i)?,
            Loc::Cell(r) => self.heap.cell_get(r)?,
        })
    }

    /// Writes a value to a location without counting; returns the old value.
    fn loc_replace(&mut self, loc: Loc, v: Slot) -> Result<Slot, RefError> {
        Ok(match loc {
            Loc::Reg(i) => std::mem::replace(&mut self.task.regs[i], v),
            Loc::Global(i) => std::mem::replace(&mut self.globals[i], v),
            Loc::Slot(b, i) => std::mem::replace(self.heap.slot_mut(b, i)?, v),
            Loc::ObjSlot(index, i) => self.heap.obj_replace_at(index, i, v)?,
            Loc::Cell(r) => std::mem::replace(self.heap.cell_mut(r)?, v),
        })
    }

    /// The value a reference slot points to (`LoadRef`).
    fn ref_read(&self, slot: Slot) -> Result<Slot, RefError> {
        self.loc_get(self.ref_target(slot)?)
    }

    /// `LoadRef` on the fast path: the value a reference slot to a register, a global variable or
    /// a cell points to, or `None` (another kind of location, or a bad one: `ref_read` reports
    /// it).
    #[inline(always)]
    fn fast_ref_read(&self, slot: Slot) -> Option<Slot> {
        match deref(slot) {
            (1, i) => self.task.regs.get(i).copied(),
            (2, i) => self.globals.get(i).copied(),
            (3, i) => self.heap.cell_value(i as Slot),
            _ => None,
        }
    }

    /// `StoreRef` on the fast path: writes `v` without counting to the register, global variable
    /// or cell a reference slot points to. `None` (nothing written) leaves it to `op_store_ref`.
    #[inline(always)]
    fn fast_ref_write(&mut self, slot: Slot, v: Slot) -> Option<()> {
        let place = match deref(slot) {
            (1, i) => self.task.regs.get_mut(i)?,
            (2, i) => self.globals.get_mut(i)?,
            (3, i) => self.heap.cell_value_mut(i as Slot)?,
            _ => return None,
        };
        *place = v;
        Some(())
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

/// What an entry of the constant cache holds: whether the constant is made yet, and whether it is
/// a managed value (`LoadConst` counts it; the cache holds one reference of it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConstState {
    Unmade,
    UnmadeManaged,
    Plain,
    Managed,
}

/// An entry of the constant cache (`docs/design/vm.md` 3.11). The fast path takes an unmanaged
/// constant already made with one look at the entry ([`CachedConst::plain`]).
#[derive(Debug, Clone, Copy)]
struct CachedConst {
    value: Slot,
    state: ConstState,
}

impl CachedConst {
    fn new(managed: bool) -> CachedConst {
        let state = if managed {
            ConstState::UnmadeManaged
        } else {
            ConstState::Unmade
        };
        CachedConst { value: 0, state }
    }

    fn managed(&self) -> bool {
        matches!(self.state, ConstState::UnmadeManaged | ConstState::Managed)
    }

    /// The value, once made.
    fn get(&self) -> Option<Slot> {
        matches!(self.state, ConstState::Plain | ConstState::Managed).then_some(self.value)
    }

    /// The value of an unmanaged constant already made (`None` for a managed one or one not made
    /// yet).
    #[inline(always)]
    fn plain(&self) -> Option<Slot> {
        (self.state == ConstState::Plain).then_some(self.value)
    }

    fn set(&mut self, v: Slot) {
        self.value = v;
        self.state = if self.managed() {
            ConstState::Managed
        } else {
            ConstState::Plain
        };
    }

    /// Takes the value out (the entry is not made any more).
    fn take(&mut self) -> Option<Slot> {
        let v = self.get();
        *self = CachedConst::new(self.managed());
        v
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
fn count_leaks<'a>(names: impl Iterator<Item = std::borrow::Cow<'a, str>>) -> Vec<(String, u64)> {
    let mut counts: std::collections::BTreeMap<std::borrow::Cow<'a, str>, u64> =
        std::collections::BTreeMap::new();
    for n in names {
        *counts.entry(n).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(n, c)| (n.into_owned(), c))
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
    /// The end of a release job ended the run (the finishing stage is over).
    Done(Box<RunOutcome>),
}

/// How the unwinding of an exception ended.
enum Unwound {
    /// A handler caught it; execution goes on there.
    Caught,
    /// It left a `finally` block run by `Halt`; `Halt` goes on from this position.
    Halt(u32),
    /// A dropped frame left a release job with a destruction to run: the job holds the
    /// exception and the unwinding goes on when it ends (`docs/design/objects.md` 3.8).
    Suspended,
    /// No handler: the report of the uncaught exception (the frames are gone).
    Uncaught(ExceptionReport),
}

/// The states of a `finally` block (`docs/spec/exceptions.md` 6.2): the body ended normally, an
/// exception is being propagated, `Halt` is unwinding. `k + 2` is the `k`-th jump.
const FINALLY_NORMAL: i64 = 0;
const FINALLY_EXCEPTION: i64 = 1;
const FINALLY_HALT: i64 = 2;

/// The number of words of the instruction at the boundary `pc` (`bytecode::decode_at`): one of
/// the `Jmp`s after an `EndFinally`, which the verifier keeps in the code (3.3).
fn instruction_len(code: &[u32], pc: usize) -> usize {
    bytecode::decode_at(code, pc).4
}

/// The target of the `Jmp` at `pc` (`None` if it is not a `Jmp`).
fn jump_target(code: &[u32], pc: usize) -> Option<usize> {
    let wide = (code.get(pc)? & 0xFF) as u8 == op::word::WIDE;
    if wide && code.len() < pc + 3 {
        return None;
    }
    let (opc, _, _, _, len) = bytecode::decode_at(code, pc);
    if opc != op::JMP {
        return None;
    }
    let off = bytecode::signed_bx(bytecode::bx_at(code, pc), wide);
    usize::try_from((pc + len) as i64 + off).ok()
}

/// `EAbstractError` for entry `k` of a class's table, an abstract method
/// (`docs/spec/objects.md` 3.1, 14.9): the name of the method.
fn abstract_called(entry: &bytecode::TypeEntry, k: u32) -> RtError {
    let name = entry
        .abstract_names
        .iter()
        .find(|(j, _)| *j == k)
        .map_or("?", |(_, n)| n.as_str());
    rt(
        exc::ABSTRACT_ERROR,
        format!("abstract method `{name}` called"),
    )
}

/// The message of `EAccessViolation` (`semantics.md` 7.1).
const NIL_ACCESS: &str = "access to a nil reference";
/// The message of an access through a handle to a destroyed instance (`docs/spec/objects.md` 14.4).
const FREED_ACCESS: &str = "access to a freed object";

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

/// Reference slot: the upper 8 bits are the kind (1: register, 2: global variable, 3: cell, 4: flat
/// place), the lower 56 bits the index (the cell's heap reference for a cell, the loan table entry
/// for a flat place).
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
    use puy_host::HostFuture;
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
    use puy_host::HostFuture;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Run {
        stdout: String,
        status: ExitStatus,
    }

    fn run_with(src: &str, stdin: &str, args: &[&str], config: IsolateConfig) -> Run {
        run_with_stdlib(src, stdin, args, config, &[])
    }

    /// [`run_with`], with `stdlib` read as units of the standard library
    /// (`driver::compile_with_stdlib`).
    fn run_with_stdlib(
        src: &str,
        stdin: &str,
        args: &[&str],
        config: IsolateConfig,
        stdlib: &[puy_stdlib::Unit],
    ) -> Run {
        let mut provider = MemorySourceProvider::new();
        let out = crate::driver::compile_with_stdlib(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
            stdlib,
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

    // `zero_slots` writes each length with stores of fixed positions: every slot of every length
    // across the boundaries of those forms (3, 4, 8, 9) ends up zero, as a frame's registers must
    #[test]
    fn zero_slots_clears_every_length() {
        for n in 0..=17 {
            let mut s = vec![Slot::MAX; n];
            zero_slots(&mut s);
            assert_eq!(s, vec![0; n], "{n}");
        }
    }

    // The three forms of the end of a `for` iteration (`ForLoopUp`, and the expanded ones for a
    // body out of its reach, with and without `Wide`; codegen.rs `for_loop_out_of_reach_expands`)
    // all run the loop the same number of times (M3-58)
    #[test]
    fn for_loop_forms_run_alike() {
        for (n, vars) in [(1, 0), (100, 0), (1, 300), (100, 300)] {
            let mut src = String::from("procedure Run;\nvar\n");
            for i in 0..vars {
                src.push_str(&format!("  V{i}: Integer;\n"));
            }
            src.push_str("  A, S, I: Integer;\nbegin\n  A := 0;\n  S := 0;\n");
            src.push_str("  for I := 1 to 10 do\n  begin\n");
            for _ in 0..n {
                src.push_str("    A := A + 1;\n");
            }
            src.push_str("    S := S + I;\n  end;\n  for I := 10 downto 1 do\n    S := S - I;\n");
            src.push_str("  WriteLn(S, ' ', A);\nend;\nRun;\n");
            assert_eq!(out(&src), format!("0 {}\n", 10 * n), "{n} {vars}");
        }
    }

    // A compare-and-branch with a negative immediate runs alike in its short and `Wide` forms
    // (codegen.rs `wide_compare_with_negative_immediate`; M3-58)
    #[test]
    fn compare_with_negative_immediate_runs_in_both_forms() {
        for (n, v) in [(1, 1), (150, 1), (1, -9), (150, -9)] {
            let mut src = format!("var N := {v};\nvar S := 0;\nif N > -5 then begin\n");
            for _ in 0..n {
                src.push_str("  S := S + 1;\n");
            }
            src.push_str("end;\nWriteLn(S);\n");
            let expected = if v > -5 { n } else { 0 };
            assert_eq!(out(&src), format!("{expected}\n"), "{n} {v}");
        }
    }

    // The `Wide` forms of the instructions M3c added or moved to the fast path of the run loop
    // (`codegen::tests::padded_program_takes_wide_forms`), which `step` runs, give the same results
    // as their short forms (M3-72)
    #[test]
    fn wide_forms_run_like_short_forms() {
        for pad in [0, 300] {
            assert_eq!(
                out(&crate::codegen::tests::padded_program(pad)),
                "3000 142 6 51 -65 76\n\
                 -40000000000 15 -308 15\n\
                 31 -69\n\
                 <1000|-69> 20 <1000|-69><1000|-69>\n\
                 153 3\n\
                 6 <1000|-69>\n\
                 12 14 12\n\
                 42\n\
                 4 <1000|-69><1000|-69>\n\
                 TTag TRUE\n\
                 <1000|-69>!\n",
                "{pad}"
            );
        }
    }

    #[test]
    // `RefFieldSet` / `RefFieldSetRef` in the `Wide` form (left to step) run as the short form
    // and go on after the type word (`docs/spec/bytecode.md` 1.2, 2.8).
    fn wide_ref_field_writes_run_like_short_forms() {
        for pad in [0, 300] {
            assert_eq!(
                out(&crate::codegen::tests::padded_ref_fields(pad)),
                "2 xx\n",
                "{pad}"
            );
        }
    }

    fn out(src: &str) -> String {
        let r = run(src);
        assert_eq!(r.status.kind, ExitKind::Code(0), "{src}\n{:?}", r.status);
        r.stdout
    }

    /// A unit read as a unit of the standard library, so that it sees the built-ins only the
    /// standard library uses (`docs/spec/collections.md` 12.1): wrappers the tests call.
    const TEST_STDLIB: puy_stdlib::Unit = puy_stdlib::Unit {
        name: "TestStd",
        source: "unit TestStd;
interface
procedure MoveRange<T>(var A: TArray<T>; FromIndex, ToIndex, Count: Integer);
procedure SortRange<T>(var A: TArray<T>; Index, Count: Integer);
function PlainOrder<T>(const X: T): Boolean;
procedure FreeValue<T>(const X: T);
function IsCounted<T>(const X: T): Boolean;
function Same<T>(const X, Y: T): Boolean;
function NameOf<T>(const X: T): string;
procedure NeedClass<T>(const X: T; const What: string);
type
  TProbe<T> = class
  protected
    procedure Hook(const X: T); virtual;
  public
    function Hooked: Boolean;
  end;
  TTable<T> = class
  private
    FBuckets: TArray<Integer>;
    FHashes: TArray<Int64>;
    FNext: TArray<Integer>;
    FKeys: TArray<T>;
  public
    procedure Put(const Key: T; Hash: Int64);
    function FindKey(const Key: T): Integer;
    function Plain: Boolean;
  end;
implementation
procedure TTable<T>.Put(const Key: T; Hash: Int64);
var
  N: Integer;
begin
  N := Length(FKeys);
  SetLength(FKeys, N + 1);
  SetLength(FHashes, N + 1);
  SetLength(FNext, N + 1);
  FKeys[N] := Key;
  FHashes[N] := Hash;
  SetLength(FBuckets, 4);
  HashRelink(FBuckets, FHashes, FNext, N + 1);
end;
function TTable<T>.FindKey(const Key: T): Integer;
begin
  Result := HashLookup(Key, FBuckets, FHashes, FNext, FKeys);
end;
function TTable<T>.Plain: Boolean;
begin
  Result := False;
  if IsPlainKey(Default(T)) then
    Result := True;
end;
procedure TProbe<T>.Hook(const X: T);
begin
end;
function TProbe<T>.Hooked: Boolean;
begin
  Result := IsOverridden(Hook);
end;
procedure MoveRange<T>(var A: TArray<T>; FromIndex, ToIndex, Count: Integer);
begin
  ArrayMoveRange(A, FromIndex, ToIndex, Count);
end;
procedure SortRange<T>(var A: TArray<T>; Index, Count: Integer);
begin
  ArraySortRange(A, Index, Count);
end;
function PlainOrder<T>(const X: T): Boolean;
begin
  Result := IsPlainOrder(X);
end;
procedure FreeValue<T>(const X: T);
begin
  FreeIfClass(X);
end;
function IsCounted<T>(const X: T): Boolean;
begin
  Result := IsRefCountedObject(X);
end;
function Same<T>(const X, Y: T): Boolean;
begin
  Result := SameInstance(X, Y);
end;
function NameOf<T>(const X: T): string;
begin
  Result := ObjectClassName(X);
end;
procedure NeedClass<T>(const X: T; const What: string);
begin
  CheckClassType(X, What);
end;
end.
",
    };

    /// The output of a program that uses [`TEST_STDLIB`].
    fn out_std(src: &str) -> String {
        let r = run_with_stdlib(
            src,
            "",
            &[],
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            },
            &[TEST_STDLIB],
        );
        assert!(
            matches!(r.status.kind, ExitKind::Code(0)),
            "{:?}\n{}",
            r.status.kind,
            r.stdout
        );
        r.stdout
    }

    /// The uncaught exception (class, message) and the output of a program that uses
    /// [`TEST_STDLIB`].
    fn error_std(src: &str) -> (String, String, String) {
        let r = run_with_stdlib(
            src,
            "",
            &[],
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            },
            &[TEST_STDLIB],
        );
        match r.status.kind {
            ExitKind::UncaughtException(rep) => (rep.class.clone(), rep.message.clone(), r.stdout),
            other => panic!("expected an exception, got {other:?}\n{}", r.stdout),
        }
    }

    /// A program that fills `A: TArray<string>` with `N` elements `'a'`, `'b'`, …, runs `call`
    /// and prints the elements joined.
    fn move_range_program(n: usize, call: &str) -> String {
        format!(
            "program T;
uses TestStd;
var
  A: TArray<string>;
  I: Integer;
  S: string;
begin
  SetLength(A, {n});
  for I := 0 to {n} - 1 do
    A[I] := Chr(Ord('a') + I);
  {call};
  S := '';
  for I := 0 to High(A) do
    S := S + A[I];
  WriteLn(S);
end.
"
        )
    }

    // `ArrayMoveRange` rotates: the moved range lands at `ToIndex` and the elements between fill
    // the room it leaves, in both directions and with overlapping ranges (bytecode.md 2.22).
    #[test]
    fn array_move_range_rotates_overlapping_ranges() {
        for (call, expected) in [
            ("MoveRange<string>(A, 1, 3, 2)", "adebcf"),
            ("MoveRange<string>(A, 3, 1, 2)", "adebcf"),
            ("MoveRange<string>(A, 3, 1, 1)", "adbcef"),
            ("MoveRange<string>(A, 0, 1, 5)", "fabcde"),
            ("MoveRange<string>(A, 1, 0, 5)", "bcdefa"),
            ("MoveRange<string>(A, 0, 4, 2)", "cdefab"),
            ("MoveRange<string>(A, 2, 2, 3)", "abcdef"),
        ] {
            assert_eq!(
                out_std(&move_range_program(6, call)),
                format!("{expected}\n"),
                "{call}"
            );
        }
    }

    // A count of 0 moves nothing, also at the end of the array and on an empty array; a range
    // ending at the last element moves (bytecode.md 2.22).
    #[test]
    fn array_move_range_empty_and_end() {
        for (n, call, expected) in [
            (4, "MoveRange<string>(A, 4, 0, 0)", "abcd"),
            (4, "MoveRange<string>(A, 0, 4, 0)", "abcd"),
            (0, "MoveRange<string>(A, 0, 0, 0)", ""),
            (4, "MoveRange<string>(A, 3, 0, 1)", "dabc"),
            (4, "MoveRange<string>(A, 0, 3, 1)", "bcda"),
        ] {
            assert_eq!(
                out_std(&move_range_program(n, call)),
                format!("{expected}\n"),
                "{call}"
            );
        }
    }

    // A negative count and a range outside the array are `ERangeError`, and nothing moves
    // (bytecode.md 2.22).
    #[test]
    fn array_move_range_out_of_range() {
        for (call, message) in [
            (
                "MoveRange<string>(A, 0, 1, -1)",
                "array count -1 is out of range",
            ),
            (
                "MoveRange<string>(A, 3, 0, 2)",
                "array range 3..4 is out of range 0..3",
            ),
            (
                "MoveRange<string>(A, 0, 3, 2)",
                "array range 3..4 is out of range 0..3",
            ),
            (
                "MoveRange<string>(A, -1, 0, 1)",
                "array range -1..-1 is out of range 0..3",
            ),
            (
                "MoveRange<string>(A, 5, 0, 0)",
                "array range 5..4 is out of range 0..3",
            ),
        ] {
            // The elements are printed after the caught exception: nothing has moved
            let src = move_range_program(
                4,
                &format!(
                    "try {call} except for I := 0 to High(A) do Write(A[I]); WriteLn end; {call}"
                ),
            );
            let (class, msg, stdout) = error_std(&src);
            assert_eq!(stdout, "abcd\n", "{call}");
            assert_eq!(
                (class.as_str(), msg.as_str()),
                ("ERangeError", message),
                "{call}"
            );
        }
    }

    // Moving an element that is lent to a `var` parameter is refused (`EAliasError`), as reading
    // or writing it would be (semantics.md 7.1); elements outside the moved span are not touched.
    #[test]
    fn array_move_range_refuses_lent_elements() {
        let src = "program T;
uses TestStd;
var
  A: TArray<Integer>;
procedure P(var X: Integer; F, T: Integer);
begin
  MoveRange<Integer>(A, F, T, 1);
  WriteLn(A[0], A[1], A[2]);
end;
begin
  A := [1, 2, 3, 4];
  P(A[3], 0, 1);
  P(A[3], 0, 3);
end.
";
        let (class, msg, stdout) = error_std(src);
        assert_eq!(stdout, "213\n");
        assert_eq!(
            (class.as_str(), msg.as_str()),
            (
                "EAliasError",
                "array element 3 is passed to a var parameter of a call in progress"
            )
        );
    }

    // `ArraySortRange` sorts only its range, stably and in place, and does nothing for a count
    // of 0 or 1, also at the end of the array and on an empty array (bytecode.md 2.22).
    #[test]
    fn array_sort_range_sorts_its_range() {
        for (n, call, expected) in [
            (
                6,
                "A[1] := 'z'; A[4] := 'a'; SortRange<string>(A, 1, 4)",
                "aacdzf",
            ),
            (6, "A[0] := 'z'; SortRange<string>(A, 0, 6)", "bcdefz"),
            (6, "A[2] := 'a'; SortRange<string>(A, 2, 0)", "abadef"),
            (6, "A[5] := 'a'; SortRange<string>(A, 5, 1)", "abcdea"),
            (6, "A[5] := 'a'; SortRange<string>(A, 6, 0)", "abcdea"),
            (0, "SortRange<string>(A, 0, 0)", ""),
        ] {
            assert_eq!(
                out_std(&move_range_program(n, call)),
                format!("{expected}\n"),
                "{call}"
            );
        }
    }

    // A negative count and a range outside the array are `ERangeError` with the messages of
    // `ArrayMoveRange`, and nothing moves (bytecode.md 2.22).
    #[test]
    fn array_sort_range_out_of_range() {
        for (call, message) in [
            (
                "SortRange<string>(A, 0, -1)",
                "array count -1 is out of range",
            ),
            (
                "SortRange<string>(A, 3, 2)",
                "array range 3..4 is out of range 0..3",
            ),
            (
                "SortRange<string>(A, -1, 2)",
                "array range -1..0 is out of range 0..3",
            ),
            (
                "SortRange<string>(A, 5, 0)",
                "array range 5..4 is out of range 0..3",
            ),
        ] {
            // Descending, so that a sort would show; printed after the caught exception
            let src = move_range_program(
                4,
                &format!(
                    "for I := 0 to 3 do A[I] := Chr(Ord('d') - I); \
                     try {call} except for I := 0 to High(A) do Write(A[I]); WriteLn end; {call}"
                ),
            );
            let (class, msg, stdout) = error_std(&src);
            assert_eq!(stdout, "dcba\n", "{call}");
            assert_eq!(
                (class.as_str(), msg.as_str()),
                ("ERangeError", message),
                "{call}"
            );
        }
    }

    // Sorting a range with an element lent to a `var` parameter is refused (`EAliasError`)
    // before anything moves; a range without it sorts (bytecode.md 2.22).
    #[test]
    fn array_sort_range_refuses_lent_elements() {
        let src = "program T;
uses TestStd;
var
  A: TArray<Integer>;
procedure P(var X: Integer; Count: Integer);
begin
  SortRange<Integer>(A, 0, Count);
  WriteLn(A[0], A[1], A[2]);
end;
begin
  A := [4, 3, 2, 1];
  P(A[3], 3);
  P(A[3], 4);
end.
";
        let (class, msg, stdout) = error_std(src);
        assert_eq!(stdout, "234\n");
        assert_eq!(
            (class.as_str(), msg.as_str()),
            (
                "EAliasError",
                "array element 3 is passed to a var parameter of a call in progress"
            )
        );
    }

    // `ArraySortRange` sorts the elements of each kind of type whose default order is a built-in
    // comparison by that order (bytecode.md 2.22; collections.md 10.2).
    #[test]
    fn array_sort_range_sorts_each_plain_order() {
        let src = "program T;
uses TestStd;
type
  TColor = (Red, Green, Blue);
  TSmall = 10..20;
var
  I: TArray<Integer>;
  U: TArray<UInt64>;
  B: TArray<Byte>;
  F: TArray<Single>;
  D: TArray<Double>;
  C: TArray<Currency>;
  S: TArray<string>;
  U8: TArray<UTF8String>;
  E: TArray<TColor>;
  R: TArray<TSmall>;
  L: TArray<Boolean>;
  H: TArray<Char>;
begin
  I := [3, -1, 2];
  SortRange<Integer>(I, 0, 3);
  WriteLn(I[0], ' ', I[1], ' ', I[2]);
  U := [High(UInt64), 1, UInt64(High(Int64)) + 1];
  SortRange<UInt64>(U, 0, 3);
  WriteLn(U[0], ' ', U[1], ' ', U[2]);
  B := [200, 7, 255];
  SortRange<Byte>(B, 0, 3);
  WriteLn(B[0], ' ', B[1], ' ', B[2]);
  F := [1.5, -2.5, 0.0 / 0.0, -0.0, 0.25];
  SortRange<Single>(F, 0, 5);
  WriteLn(F[0]:0:2, ' ', F[1]:0:2, ' ', F[2]:0:2, ' ', F[3]:0:2, ' ', F[4] <> F[4]);
  D := [1.5, 0.0 / 0.0, -1e3, 1e3];
  SortRange<Double>(D, 0, 4);
  WriteLn(D[0]:0:1, ' ', D[1]:0:1, ' ', D[2]:0:1, ' ', D[3] <> D[3]);
  C := [1.25, -3.5, 0.0001];
  SortRange<Currency>(C, 0, 3);
  WriteLn(C[0]:0:4, ' ', C[1]:0:4, ' ', C[2]:0:4);
  S := ['b', '', 'Z', 'a'];
  SortRange<string>(S, 0, 4);
  WriteLn('[', S[0], '] ', S[1], ' ', S[2], ' ', S[3]);
  U8 := [UTF8String('é'), UTF8String('z'), UTF8String('a')];
  SortRange<UTF8String>(U8, 0, 3);
  WriteLn(string(U8[0]), ' ', string(U8[1]), ' ', string(U8[2]));
  E := [TColor.Blue, TColor.Red, TColor.Green];
  SortRange<TColor>(E, 0, 3);
  WriteLn(Ord(E[0]), Ord(E[1]), Ord(E[2]));
  R := [20, 10, 15];
  SortRange<TSmall>(R, 0, 3);
  WriteLn(R[0], ' ', R[1], ' ', R[2]);
  L := [True, False];
  SortRange<Boolean>(L, 0, 2);
  WriteLn(L[0], ' ', L[1]);
  H := ['c', 'A', 'b'];
  SortRange<Char>(H, 0, 3);
  WriteLn(H[0], H[1], H[2]);
end.
";
        assert_eq!(
            out_std(src),
            "-1 2 3\n\
             1 9223372036854775808 18446744073709551615\n\
             7 200 255\n\
             -2.50 -0.00 0.25 1.50 TRUE\n\
             -1000.0 1.5 1000.0 TRUE\n\
             -3.5000 0.0001 1.2500\n\
             [] Z a b\n\
             a z é\n\
             012\n\
             10 15 20\n\
             FALSE TRUE\n\
             Abc\n"
        );
    }

    /// The types of [`is_plain_order_is_fixed_by_type`] and
    /// [`array_sort_range_refuses_other_orders`]: a record with its own `LessThan`, and one
    /// without a default order.
    const ORDER_TYPES: &str = "type
  TRec = record
    X: Integer;
    class operator LessThan(const A, B: TRec): Boolean;
  end;
  TPlainRec = record
    X: Integer;
  end;
class operator TRec.LessThan(const A, B: TRec): Boolean;
begin
  Result := A.X < B.X;
end;
";

    // `IsPlainOrder` is `True` for a type whose default order is a built-in comparison, and
    // `False` for a record with its own `LessThan`, a record and a class without a default order
    // (bytecode.md 2.22; collections.md 10.2).
    #[test]
    fn is_plain_order_is_fixed_by_type() {
        let src = format!(
            "program T;
uses TestStd;
{ORDER_TYPES}begin
  WriteLn(PlainOrder<Integer>(0), ' ', PlainOrder<string>(''), ' ', PlainOrder<TRec>(Default(TRec)),
    ' ', PlainOrder<TPlainRec>(Default(TPlainRec)), ' ', PlainOrder<TObject>(nil));
end.
"
        );
        assert_eq!(out_std(&src), "TRUE TRUE FALSE FALSE FALSE\n");
    }

    // `ArraySortRange` of a type whose default order is not a built-in comparison (here a record
    // with its own `LessThan`) raises `EInvalidOperation` (bytecode.md 2.22).
    #[test]
    fn array_sort_range_refuses_other_orders() {
        let src = format!(
            "program T;
uses TestStd;
{ORDER_TYPES}var
  X: TArray<TRec>;
begin
  SetLength(X, 2);
  SortRange<TRec>(X, 0, 2);
end.
"
        );
        let (class, msg, _) = error_std(&src);
        assert_eq!(
            (class.as_str(), msg.as_str()),
            (
                "EInvalidOperation",
                "`TRec` is not ordered by a built-in comparison"
            )
        );
    }

    // The room of the merges counts against the isolate's memory limit: a sort that would go
    // over it raises `EOutOfMemory` before anything moves (bytecode.md 2.22; requirements §5.12).
    #[test]
    fn array_sort_range_counts_its_room() {
        let src = "program T;
uses TestStd;
var
  A: TArray<Integer>;
  I: Integer;
begin
  SetLength(A, 100000);
  for I := 0 to High(A) do
    A[I] := -I;
  try
    SortRange<Integer>(A, 0, Length(A));
  except
    on E: EOutOfMemory do WriteLn('out of memory ', A[0], ' ', A[High(A)]);
  end;
  SortRange<Integer>(A, 0, 1000);
  WriteLn(A[0], ' ', A[999]);
end.
";
        // The array takes 800,000 bytes; the room of the whole sort as many again
        let r = run_with_stdlib(
            src,
            "",
            &[],
            IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                memory_limit: Some(1_200_000),
                ..Default::default()
            },
            &[TEST_STDLIB],
        );
        assert_eq!(r.status.kind, ExitKind::Code(0), "{}", r.stdout);
        assert_eq!(r.stdout, "out of memory 0 -99999\n-999 0\n");
    }

    // `FreeIfClass` frees an instance of a class (through `TObject` too) and does nothing for
    // `nil` and for a value of another type; `IsRefCountedObject` looks at the instance of
    // `TObject` and of a reference counted class only (collections.md 12.1, 9.2).
    #[test]
    fn free_if_class_and_is_ref_counted_object() {
        let src = "program T;
uses TestStd;
type
  TFoo = class
    destructor Destroy; override;
  end;
  TBar = class(TRefObject)
  end;
destructor TFoo.Destroy;
begin
  WriteLn('destroy');
  inherited;
end;
var
  F: TFoo;
  O: TObject;
  B: TBar;
begin
  F := TFoo.Create;
  FreeValue<TFoo>(F);
  O := TFoo.Create;
  WriteLn(IsCounted<TObject>(O));
  FreeValue<TObject>(O);
  WriteLn(IsCounted<TObject>(O));
  F := nil;
  FreeValue<TFoo>(F);
  FreeValue<Integer>(5);
  FreeValue<string>('x');
  B := TBar.Create;
  O := B;
  WriteLn(IsCounted<TObject>(O), ' ', IsCounted<TBar>(B), ' ', IsCounted<TObject>(F));
  F := nil;
  WriteLn(IsCounted<Integer>(1), ' ', IsCounted<TFoo>(F));
end.
";
        assert_eq!(
            out_std(src),
            "destroy\nFALSE\ndestroy\nFALSE\nTRUE TRUE FALSE\nFALSE FALSE\n"
        );
    }

    // `FreeIfClass` of an instance of a reference counted class through `TObject` raises as
    // `Free` does (objects.md 3.2).
    #[test]
    fn free_if_class_of_counted_instance_raises() {
        let src = "program T;
uses TestStd;
type
  TBar = class(TRefObject)
  end;
var
  O: TObject;
begin
  O := TBar.Create;
  FreeValue<TObject>(O);
end.
";
        let (class, msg, _) = error_std(src);
        assert_eq!(
            (class.as_str(), msg.as_str()),
            (
                "EInvalidOperation",
                "cannot free a reference counted object of class `TBar`; assign `nil` instead"
            )
        );
    }

    // `SameInstance` compares the identity of instances (not `Equals`) and is `False` for the
    // other types, even equal values; `ObjectClassName` is the class of the instance, `''` for
    // the other types (collections.md 9.3, 12.1).
    #[test]
    fn same_instance_and_object_class_name() {
        let src = "program T;
uses TestStd;
type
  TFoo = class
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;
  TBar = class(TFoo)
  end;
function TFoo.Equals(Obj: TObject): Boolean;
begin
  Result := True;
end;
function TFoo.GetHashCode: Int64;
begin
  Result := 0;
end;
var
  A, B: TFoo;
  O: TObject;
begin
  A := TFoo.Create;
  B := TBar.Create;
  O := B;
  WriteLn(Same<TFoo>(A, A), ' ', Same<TFoo>(A, B), ' ', Same<TObject>(O, B));
  WriteLn(Same<Integer>(1, 1), ' ', Same<string>('x', 'x'));
  WriteLn(NameOf<TFoo>(B), ' ', NameOf<TObject>(O), ' [', NameOf<Integer>(1), ']');
end.
";
        assert_eq!(out_std(src), "TRUE FALSE TRUE\nFALSE FALSE\nTBar TBar []\n");
    }

    // `HashLookup` hashes and compares the keys of `Integer` and `string` (`IsPlainKey`): an
    // entry whose remembered hash is not its key's is not found. For another key type
    // (`Double`) it is `HashFind` from the start of the chain of `KeyHash(Key)`: the first entry
    // with the hash, whatever its key (bytecode.md 2.23).
    #[test]
    fn hash_lookup_compares_plain_keys_only() {
        let src = "program T;
uses TestStd;
var
  I: TTable<Integer>;
  S: TTable<string>;
  D: TTable<Double>;
  A: string;
begin
  I := TTable<Integer>.Create;
  I.Put(10, KeyHash(10));
  I.Put(20, KeyHash(20));
  I.Put(30, KeyHash(30));
  I.Put(40, KeyHash(41));
  WriteLn(I.Plain, ' ', I.FindKey(30), ' ', I.FindKey(10), ' ', I.FindKey(40), ' ', I.FindKey(50));
  S := TTable<string>.Create;
  A := 'a';
  S.Put(A + 'b', KeyHash('ab'));
  S.Put('cd', KeyHash('cd'));
  WriteLn(S.Plain, ' ', S.FindKey('ab'), ' ', S.FindKey(A + 'b'), ' ', S.FindKey('cd'), ' ',
    S.FindKey('x'));
  D := TTable<Double>.Create;
  D.Put(1.5, KeyHash(1.5));
  D.Put(2.5, KeyHash(1.5));
  WriteLn(D.Plain, ' ', D.FindKey(1.5), ' ', D.FindKey(2.5));
end.
";
        assert_eq!(out_std(src), "TRUE 2 0 -1 -1\nTRUE 0 0 1 -1\nFALSE 1 -1\n");
    }

    // The arrays of `HashLookup` are fields of `Self` named without `Self.`, declared one after
    // another in the order of the parameters: a qualified field, fields out of that order and
    // local variables are E0702 (bytecode.md 2.23).
    #[test]
    fn hash_lookup_wants_consecutive_fields_of_self() {
        const BAD_STD: puy_stdlib::Unit = puy_stdlib::Unit {
            name: "BadStd",
            source: "unit BadStd;
interface
type
  TBad = class
  private
    FBuckets: TArray<Integer>;
    FHashes: TArray<Int64>;
    FNext: TArray<Integer>;
    FKeys: TArray<Integer>;
  public
    function Qualified(Key: Integer): Integer;
    function Swapped(Key: Integer): Integer;
    function Locals(Key: Integer): Integer;
    function Fine(Key: Integer): Integer;
  end;
implementation
function TBad.Qualified(Key: Integer): Integer;
begin
  Result := HashLookup(Key, Self.FBuckets, FHashes, FNext, FKeys);
end;
function TBad.Swapped(Key: Integer): Integer;
begin
  Result := HashLookup(Key, FBuckets, FHashes, FKeys, FNext);
end;
function TBad.Locals(Key: Integer): Integer;
var
  B, N: TArray<Integer>;
begin
  B := nil;
  N := nil;
  Result := HashLookup(Key, B, FHashes, N, FKeys);
end;
function TBad.Fine(Key: Integer): Integer;
begin
  Result := HashLookup(Key, FBuckets, FHashes, FNext, FKeys);
end;
end.
",
        };
        let mut provider = MemorySourceProvider::new();
        let out = crate::driver::compile_with_stdlib(
            "t.pas",
            b"program T;\nuses BadStd;\nbegin\nend.\n".to_vec(),
            &mut provider,
            &CompileOptions::default(),
            &[BAD_STD],
        );
        let errors: Vec<(Option<String>, String)> = out
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| (d.code.map(|c| c.to_string()), d.message.clone()))
            .collect();
        let one = (
            Some("E0702".to_string()),
            "type mismatch: expected four consecutive fields of `Self`".to_string(),
        );
        // `Qualified`, `Swapped` and `Locals`; `Fine` passes
        assert_eq!(errors, vec![one; 3]);
    }

    // `HashLookupInt` / `HashLookupStr` check their instance and destroy none, so the second
    // inlined lookup on the same table keeps no check of `Self` (`ClassType`) after the first
    // (design/inlining.md 7.1): one check and two lookups in `Twice` (bytecode.md 2.23). The
    // keys are parameters, so that nothing else between the lookups keeps the check.
    #[test]
    fn hash_lookup_keeps_the_check_of_its_instance() {
        for (idx, (ty, a, b)) in [("Integer", "1", "2"), ("string", "'ab'", "'cd'")]
            .into_iter()
            .enumerate()
        {
            let src = format!(
                "program T;
uses TestStd;
procedure Twice(T: TTable<{ty}>; const A, B: {ty});
begin
  WriteLn(T.FindKey(A) + T.FindKey(B));
end;
var
  X: TTable<{ty}>;
  K: {ty};
begin
  X := TTable<{ty}>.Create;
  K := {a};
  X.Put(K, KeyHash(K));
  Twice(X, K, {b});
end.
"
            );
            assert_eq!(out_std(&src), "-1\n", "{idx}");
            let mut provider = MemorySourceProvider::new();
            let out = crate::driver::compile_with_stdlib(
                "t.pas",
                src.into_bytes(),
                &mut provider,
                &CompileOptions::default(),
                &[TEST_STDLIB],
            );
            let module = out.stages.module.as_ref().expect("module");
            let text = crate::bytecode::disassemble(module);
            let twice: Vec<&str> = text
                .lines()
                .skip_while(|l| !(l.starts_with(".func") && l.contains("T.Twice ")))
                .skip(1)
                .take_while(|l| !l.starts_with(".func"))
                .collect();
            let count = |name: &str| twice.iter().filter(|l| l.contains(name)).count();
            let lookup = if ty == "string" {
                "HashLookupStr"
            } else {
                "HashLookupInt"
            };
            assert_eq!(
                (count("ClassType"), count(lookup)),
                (1, 2),
                "{idx}\n{}",
                twice.join("\n")
            );
        }
    }

    // `IsOverridden(M)` tells whether the class of the instance replaced the method `M` in its
    // table, whichever class the instance was made from (collections.md 12.1, bytecode.md 2.22).
    #[test]
    fn is_overridden_looks_at_the_class_of_the_instance() {
        let src = "program T;
uses TestStd;
type
  TPlain = class(TProbe<Integer>)
  end;
  THook = class(TProbe<Integer>)
  protected
    procedure Hook(const X: Integer); override;
  end;
  TBelow = class(THook)
  end;
  TText = class(TProbe<string>)
  protected
    procedure Hook(const X: string); override;
  end;
procedure THook.Hook(const X: Integer);
begin
end;
procedure TText.Hook(const X: string);
begin
end;
var
  P: TProbe<Integer>;
begin
  P := TProbe<Integer>.Create;
  Write(P.Hooked, ' ');
  P.Free;
  P := TPlain.Create;
  Write(P.Hooked, ' ');
  P.Free;
  P := THook.Create;
  Write(P.Hooked, ' ');
  P.Free;
  P := TBelow.Create;
  WriteLn(P.Hooked);
  P.Free;
  WriteLn(TProbe<string>.Create.Hooked, ' ', TText.Create.Hooked);
end.
";
        assert_eq!(out_std(src), "FALSE FALSE TRUE TRUE\nFALSE TRUE\n");
    }

    // `IsOverridden` takes the name of exactly one virtual instance method of `Self`: an
    // expression, a method that is not virtual, an overloaded name and a call in a class method
    // are E0702 (diagnostics.md 4.2, bytecode.md 2.22).
    #[test]
    fn is_overridden_wants_one_virtual_method_of_self() {
        const BAD_STD: puy_stdlib::Unit = puy_stdlib::Unit {
            name: "BadStd",
            source: "unit BadStd;
interface
type
  TBad = class
  protected
    procedure Plain;
    procedure Over(X: Integer); overload; virtual;
    procedure Over(const S: string); overload; virtual;
    procedure Hook; virtual;
  public
    function Expr: Boolean;
    function NotVirtual: Boolean;
    function Overloaded: Boolean;
    class function InClass: Boolean;
    function Fine: Boolean;
  end;
implementation
procedure TBad.Plain;
begin
end;
procedure TBad.Over(X: Integer);
begin
end;
procedure TBad.Over(const S: string);
begin
end;
procedure TBad.Hook;
begin
end;
function TBad.Expr: Boolean;
begin
  Result := IsOverridden(Self.Hook);
end;
function TBad.NotVirtual: Boolean;
begin
  Result := IsOverridden(Plain);
end;
function TBad.Overloaded: Boolean;
begin
  Result := IsOverridden(Over);
end;
class function TBad.InClass: Boolean;
begin
  Result := IsOverridden(Hook);
end;
function TBad.Fine: Boolean;
begin
  Result := IsOverridden(Hook);
end;
end.
",
        };
        let mut provider = MemorySourceProvider::new();
        let out = crate::driver::compile_with_stdlib(
            "t.pas",
            b"program T;\nuses BadStd;\nbegin\nend.\n".to_vec(),
            &mut provider,
            &CompileOptions::default(),
            &[BAD_STD],
        );
        let errors: Vec<(Option<String>, String)> = out
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| (d.code.map(|c| c.to_string()), d.message.clone()))
            .collect();
        let one = (
            Some("E0702".to_string()),
            "type mismatch: expected the name of a virtual method of `Self`".to_string(),
        );
        // `Expr`, `NotVirtual`, `Overloaded` and `InClass`; `Fine` passes
        assert_eq!(errors, vec![one; 4]);
    }

    // `CheckClassType` does nothing for a class type (even `nil`) and raises `EInvalidOperation`
    // with the text it is given and the name of any other type (collections.md 9.3, 12.1).
    #[test]
    fn check_class_type_raises_for_other_types() {
        let src = "program T;
uses TestStd;
type
  TFoo = class
  end;
  TRec = record
    X: Integer;
  end;
var
  F: TFoo;
  R: TRec;
begin
  F := nil;
  NeedClass<TFoo>(F, 'foo');
  NeedClass<TObject>(F, 'object');
  WriteLn('classes pass');
  R.X := 1;
  NeedClass<TRec>(R, 'records do not');
end.
";
        let (class, msg, out) = error_std(src);
        assert_eq!(
            (class.as_str(), msg.as_str(), out.as_str()),
            (
                "EInvalidOperation",
                "records do not, but `TRec` is not a class",
                "classes pass\n"
            )
        );
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

    // A refused reservation is `OutOfMemory` (the VM's one fallible allocation), and a text
    // known to be at least that long is refused before anything is written. The size is more
    // bytes than any target can address (on wasm32, half the address space can be granted).
    #[test]
    fn refused_reservation_is_out_of_memory() {
        let huge = usize::MAX / 2;
        let mut v: Vec<u16> = Vec::new();
        assert_eq!(heap::try_reserve(&mut v, huge), Err(OutOfMemory));
        assert_eq!(heap::try_fill(&mut v, 0, huge), Err(OutOfMemory));
        assert!(v.is_empty());
        assert!(matches!(heap::TextBuf::with_room(huge), Err(OutOfMemory)));
    }

    // The room asked for before a number with a precision is written (the digits plus
    // `DIGITS_EXTRA`) holds the whole text, so the buffer never doubles past it.
    #[test]
    fn number_text_fits_the_room_asked_for() {
        use std::fmt::Write as _;
        let p = 5000;
        let room = p as usize + numfmt::DIGITS_EXTRA;
        for (idx, v) in [-1.5, -f64::MAX, 1e-300].into_iter().enumerate() {
            let mut t = heap::TextBuf::with_room(room).unwrap();
            let cap = t.units_mut().capacity();
            numfmt::f64_fixed(&mut t, v, p).unwrap();
            assert_eq!(t.units_mut().capacity(), cap, "fixed {idx}");
            let mut t = heap::TextBuf::with_room(room).unwrap();
            write!(t, "{v:.*e}E-308", p as usize).unwrap();
            assert_eq!(t.units_mut().capacity(), cap, "exponential {idx}");
        }
    }

    // `X:W:D` with more decimals than Rust's formatting takes (65535) writes zeros after the
    // exact value instead of failing; the whole line is too long for a golden case.
    #[test]
    fn fixed_width_past_the_format_precision_limit() {
        let text = out("WriteLn(0.5:0:70000); WriteLn(-2.0:0:65536);");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 70002);
        assert!(lines[0].starts_with("0.5000"));
        assert!(lines[0][2..].bytes().skip(1).all(|b| b == b'0'));
        assert_eq!(lines[1].len(), 65539);
        assert!(lines[1].starts_with("-2.000"));
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
        run_code_with_types(Vec::new(), Vec::new(), consts, regs, code)
    }

    /// [`run_code`] with a type section and interface call targets.
    fn run_code_with_types(
        types: Vec<crate::bytecode::TypeEntry>,
        intf_calls: Vec<(u32, u32)>,
        consts: Vec<ConstEntry>,
        regs: u16,
        code: Vec<u32>,
    ) -> RunOutcome {
        let module = code_module(types, intf_calls, consts, regs, code);
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

    /// The verified module of [`run_code_with_types`].
    fn code_module(
        types: Vec<crate::bytecode::TypeEntry>,
        intf_calls: Vec<(u32, u32)>,
        consts: Vec<ConstEntry>,
        regs: u16,
        code: Vec<u32>,
    ) -> Module {
        use crate::bytecode::{Function, UnitEntry};

        let module = Module {
            types,
            intf_calls,
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
                shared_by: Vec::new(),
                inlines: Vec::new(),
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
        module
    }

    /// An isolate with the hash seed `seed`, not started, whose registers are `regs`: for running one
    /// instruction body on chosen values with [`Isolate::exec_op`].
    fn isolate_with_regs(seed: u64, regs: Vec<Slot>) -> Isolate {
        use crate::bytecode::encode;

        let module = code_module(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            1,
            vec![encode(op::RET_VOID, 0, 0, 0)],
        );
        let mut iso = Isolate::new(
            Arc::new(module),
            HostSet::empty(),
            IsolateConfig {
                hash_seed: HashSeed::Fixed(seed),
                ..Default::default()
            },
        )
        .unwrap();
        iso.task.regs = regs;
        iso
    }

    /// Runs the body of `opc` with `rB` = `b` and `rC` = `c` (registers 1 and 2) under the hash
    /// seed `seed`, and returns `rA` (register 0).
    fn exec_value(seed: u64, opc: u16, b: Slot, c: Slot) -> Slot {
        let mut iso = isolate_with_regs(seed, vec![0, b, c]);
        exec_one(&mut iso, opc, 1, 2);
        iso.task.regs[0]
    }

    /// Runs the body of `opc` into register 0 from registers `rb` and `rc`, which must succeed.
    fn exec_one(iso: &mut Isolate, opc: u16, rb: usize, rc: usize) {
        let r = iso.exec_op(
            u8::try_from(opc).expect("an instruction below 256"),
            0,
            rb,
            rc,
            rb,
            rc,
            0,
        );
        assert!(matches!(r, Some(Ok(()))), "{opc}");
    }

    // `HashInt` and `HashMix` give the values of the definitions in `docs/spec/equality.md` 7.2
    // under fixed seeds (worked out from the definitions outside this implementation), and
    // `HashMix` depends on the order of its operands.
    #[test]
    fn hash_int_and_mix_follow_the_definitions() {
        let cases: [(u64, [i64; 5]); 3] = [
            (
                0,
                [
                    0,
                    -5417735806833148549,
                    -6387817139659442654,
                    9187280898733096074,
                    -8146705761926482474,
                ],
            ),
            (
                1,
                [
                    8841707400507832957,
                    -903645775220811516,
                    -4420583484614546736,
                    2430264566066484953,
                    6102141007488897455,
                ],
            ),
            (
                0x1234,
                [
                    -594279027983241470,
                    -5920720206039136878,
                    6751245297235063491,
                    9145705504650706494,
                    246565457056481777,
                ],
            ),
        ];
        for (seed, [h0, h_minus1, h42, mix12, mix21]) in cases {
            assert_eq!(exec_value(seed, op::HASH_INT, 0, 0), h0, "{seed} HashInt 0");
            assert_eq!(
                exec_value(seed, op::HASH_INT, -1, 0),
                h_minus1,
                "{seed} HashInt -1"
            );
            assert_eq!(
                exec_value(seed, op::HASH_INT, 42, 0),
                h42,
                "{seed} HashInt 42"
            );
            assert_eq!(
                exec_value(seed, op::HASH_MIX, 1, 2),
                mix12,
                "{seed} HashMix 1 2"
            );
            assert_eq!(
                exec_value(seed, op::HASH_MIX, 2, 1),
                mix21,
                "{seed} HashMix 2 1"
            );
        }
    }

    // `HashStr` is the low 32 bits of SipHash-1-3 of the code units, two bytes each
    // (`docs/spec/equality.md` 2.3, 7.2; values worked out from the definitions), and the same for a one-byte and a UTF-16
    // string of the same content.
    #[test]
    fn hash_str_follows_the_definition_in_any_representation() {
        let cases: [(u64, &str, i64); 6] = [
            (0, "", 592699862),
            (0, "abc", 1887990610),
            (0, "a\u{e9}", 2217911862),
            (0x1234, "", 3647069930),
            (0x1234, "abc", 1552488154),
            (0x1234, "a\u{e9}", 2310918997),
        ];
        for (seed, text, expected) in cases {
            let units: Vec<u16> = text.encode_utf16().collect();
            let mut iso = isolate_with_regs(seed, vec![0; 3]);
            let utf16 = iso.heap.alloc_str(PStr::Utf16(units.clone())).unwrap();
            iso.task.regs[1] = utf16;
            if units.iter().all(|u| *u < 0x80) {
                let bytes = units.iter().map(|u| *u as u8).collect();
                iso.task.regs[2] = iso.heap.alloc_str(PStr::Ascii(bytes)).unwrap();
            } else {
                iso.task.regs[2] = utf16;
            }
            for r in [1, 2] {
                exec_one(&mut iso, op::HASH_STR, r, 0);
                assert_eq!(iso.task.regs[0], expected, "{seed} {text:?} register {r}");
            }
        }
        // The empty string is 0, and hashes like an empty string block
        assert_eq!(exec_value(0, op::HASH_STR, 0, 0), 592699862);
    }

    // `HashF64` makes `-0.0` hash as `0.0` and every NaN as one NaN; `KeyEqF64` makes two NaNs
    // equal and `0.0` equal to `-0.0` (`docs/spec/equality.md` 2.2).
    #[test]
    fn float_keys_unify_zeros_and_nans() {
        let bits = |x: f64| x.to_bits() as Slot;
        let seed = 0x1234;
        let nan_payload = f64::from_bits(0x7FF0_0000_0000_0001);
        let neg_nan = f64::from_bits(0xFFF8_0000_0000_0000);
        // `HashInt` of 0, of $7FF8000000000000 and of the bits of 1.5 under this seed
        let zero = -594279027983241470;
        let nan = -306512056521684778;
        let one_and_half = 6689913779726435811;
        for (x, expected) in [
            (0.0, zero),
            (-0.0, zero),
            (f64::NAN, nan),
            (nan_payload, nan),
            (neg_nan, nan),
            (1.5, one_and_half),
        ] {
            assert_eq!(
                exec_value(seed, op::HASH_F64, bits(x), 0),
                expected,
                "{x:?}"
            );
        }
        for (a, b, eq) in [
            (0.0, -0.0, true),
            (f64::NAN, f64::NAN, true),
            (f64::NAN, nan_payload, true),
            (neg_nan, f64::NAN, true),
            (f64::NAN, 0.0, false),
            (1.5, 1.5, true),
            (1.5, -1.5, false),
            (f64::INFINITY, f64::INFINITY, true),
            (f64::INFINITY, f64::NEG_INFINITY, false),
        ] {
            assert_eq!(
                exec_value(seed, op::KEY_EQ_F64, bits(a), bits(b)),
                eq as Slot,
                "{a:?} {b:?}"
            );
        }
    }

    /// A started isolate under the hash seed `seed` whose `<main>` is `code` and a `RetVoid` (with
    /// 6 registers and the type section `types`), stopped before its first instruction: registers
    /// can be set to values made on its heap, then the instructions run one at a time with
    /// [`step_through`].
    fn started_isolate(
        seed: u64,
        types: Vec<crate::bytecode::TypeEntry>,
        mut code: Vec<u32>,
    ) -> Isolate {
        code.push(crate::bytecode::encode(op::RET_VOID, 0, 0, 0));
        let module = code_module(types, Vec::new(), Vec::new(), 6, code);
        let mut iso = Isolate::new(
            Arc::new(module),
            HostSet::empty(),
            IsolateConfig {
                hash_seed: HashSeed::Fixed(seed),
                ..Default::default()
            },
        )
        .unwrap();
        iso.start_main(vec!["t.pas".into()]).unwrap();
        iso
    }

    /// Runs the first `n` instructions of `<main>`, each of which must continue.
    fn step_through(iso: &mut Isolate, n: usize) {
        for i in 0..n {
            let mut budget = 1;
            match iso.step(&mut budget) {
                Step::Continue => {}
                Step::Fatal(m) => panic!("instruction {i}: fatal: {m}"),
                _ => panic!("instruction {i} did not continue"),
            }
        }
    }

    /// `Intrinsic id A=0 B=1`: the intrinsic `id` into register 0 from register 1 (and 2).
    fn intrinsic_word(id: u8) -> u32 {
        crate::bytecode::encode(op::INTRINSIC, 0, 1, u32::from(id))
    }

    /// Runs the intrinsic `id` with `r1` = `b` and `r2` = `c` in an isolate made by
    /// [`started_isolate`] with that one instruction, and returns `r0`.
    fn intrinsic_value(iso: &mut Isolate, b: Slot, c: Slot) -> Slot {
        iso.task.frames.last_mut().expect("main runs").pc = 0;
        iso.task.regs[1] = b;
        iso.task.regs[2] = c;
        step_through(iso, 1);
        iso.task.regs[0]
    }

    // `ObjAlive` (`docs/spec/equality.md` 8.1) is 1 for a live instance and one being destroyed,
    // and 0 for `nil`, a freed handle and a tombstone (a resurrected instance's handle; objects.md
    // 3.3), without raising for any of them.
    #[test]
    fn obj_alive_tells_live_instances_from_freed_ones() {
        let mut iso = started_isolate(0, Vec::new(), vec![intrinsic_word(op::intr::OBJ_ALIVE)]);
        let manual = iso.heap.alloc_obj(bytecode::TOBJECT_TYPE).unwrap().unwrap();
        let counted = iso
            .heap
            .alloc_obj(bytecode::TREFOBJECT_TYPE)
            .unwrap()
            .unwrap();
        let dying = iso.heap.alloc_obj(bytecode::TOBJECT_TYPE).unwrap().unwrap();
        iso.heap.begin_destroy(dying).unwrap();
        let freed = iso.heap.alloc_obj(bytecode::TOBJECT_TYPE).unwrap().unwrap();
        iso.heap.begin_destroy(freed).unwrap();
        iso.heap.end_destroy(freed).unwrap();
        // A reference left when it is destroyed makes a tombstone
        let tomb = iso
            .heap
            .alloc_obj(bytecode::TREFOBJECT_TYPE)
            .unwrap()
            .unwrap();
        iso.heap.add_ref_obj(tomb).unwrap();
        iso.heap.begin_destroy(tomb).unwrap();
        iso.heap.end_destroy(tomb).unwrap();
        for (name, h, alive) in [
            ("nil", 0, 0),
            ("manual", manual, 1),
            ("counted", counted, 1),
            ("being destroyed", dying, 1),
            ("freed", freed, 0),
            ("tombstone", tomb, 0),
        ] {
            assert_eq!(intrinsic_value(&mut iso, h, 0), alive, "{name}");
        }
    }

    // `ObjAlive` of a handle that names a heap block that is not an instance is `Fatal`: only
    // forged bytecode makes one (`docs/spec/objects.md` 14.4).
    #[test]
    fn obj_alive_of_a_string_is_fatal() {
        let code = vec![
            crate::bytecode::encode(op::LOAD_CONST, 1, 0, 0),
            intrinsic_word(op::intr::OBJ_ALIVE),
            crate::bytecode::encode(op::RET_VOID, 0, 0, 0),
        ];
        match run_code(vec![ConstEntry::Str("abc".into())], 3, code) {
            RunOutcome::Fatal(e) => assert_eq!(e.message, "handle 1 does not refer to an instance"),
            other => panic!("{other:?}"),
        }
    }

    // `HashUtf8` is the low 32 bits of SipHash-1-3 of the bytes of a `UTF8String`
    // (`docs/spec/equality.md` 2.3, 7.2; values worked out from the definitions outside this implementation), and the empty
    // `UTF8String` (0) hashes like no bytes.
    #[test]
    fn hash_utf8_follows_the_definition() {
        let cases: [(u64, &str, i64); 8] = [
            (0, "", 592699862),
            (0, "abc", 3413298767),
            (0, "a\u{e9}", 1538883246),
            (0, "日本語の文字列テスト", 435506405),
            (0x1234, "", 3647069930),
            (0x1234, "abc", 3902593788),
            (0x1234, "a\u{e9}", 2268308500),
            (0x1234, "日本語の文字列テスト", 1695023260),
        ];
        for (seed, text, expected) in cases {
            let mut iso =
                started_isolate(seed, Vec::new(), vec![intrinsic_word(op::intr::HASH_UTF8)]);
            let s = iso.heap.alloc_u8(text.as_bytes().to_vec()).unwrap();
            assert_eq!(intrinsic_value(&mut iso, s, 0), expected, "{seed} {text:?}");
        }
    }

    // `HashSetBig` hashes the words of a big set without its trailing zero words, starting from
    // `HashInt` of their number (`docs/spec/bytecode.md` 2.21; values worked out from the
    // definitions), so sets of two sizes that `SetEq` takes as equal hash alike.
    #[test]
    fn hash_set_big_ignores_trailing_zero_words() {
        use crate::bytecode::{TYPE_SET, TypeEntry, encode};

        let set = |name: &str, bits: u32| TypeEntry {
            kind: TYPE_SET,
            name: name.into(),
            slots: bits,
            ..TypeEntry::default()
        };
        // r0 := SetEq(r1, r2); r3 := HashSetBig(r1); r4 := HashSetBig(r2)
        let code = vec![
            encode(op::SET_EQ, 0, 1, 2),
            encode(op::INTRINSIC, 3, 1, u32::from(op::intr::HASH_SET_BIG)),
            encode(op::INTRINSIC, 4, 2, u32::from(op::intr::HASH_SET_BIG)),
        ];
        let types = vec![set("Test.TSmall", 100), set("Test.TLarge", 200)];
        // The words of the small and the large set, `SetEq`, and their hashes
        type Case = (&'static [u64], &'static [u64], i64, i64, i64);
        let cases: [Case; 4] = [
            (&[], &[], 1, -594279027983241470, -594279027983241470),
            (&[5], &[5], 1, -2400061404566412363, -2400061404566412363),
            (
                &[0, 0],
                &[0, 0, 0, 0],
                1,
                -594279027983241470,
                -594279027983241470,
            ),
            (&[0, 5], &[5], 0, -739570611015246670, -2400061404566412363),
        ];
        for (idx, (small, large, eq, h_small, h_large)) in cases.into_iter().enumerate() {
            let mut iso = started_isolate(0x1234, types.clone(), code.clone());
            iso.task.regs[1] = iso.heap.alloc_set(64, small).unwrap().unwrap();
            iso.task.regs[2] = iso.heap.alloc_set(65, large).unwrap().unwrap();
            step_through(&mut iso, 3);
            assert_eq!(iso.task.regs[0], eq, "{idx} SetEq");
            assert_eq!(iso.task.regs[3], h_small, "{idx} small");
            assert_eq!(iso.task.regs[4], h_large, "{idx} large");
        }
        // The empty set as 0 hashes like an empty block
        let mut iso = started_isolate(0x1234, types, code);
        step_through(&mut iso, 3);
        assert_eq!(iso.task.regs[3], -594279027983241470);
    }

    /// A callable value block on `iso`'s heap.
    fn callable(iso: &mut Isolate, kind: heap::CallableKind, func: u32, target: Slot) -> Slot {
        iso.heap
            .alloc_callable(heap::Callable { kind, func, target })
            .unwrap()
    }

    // `CallableKeyEq` (`docs/spec/equality.md` 2.7) takes `nil` as equal to `nil` only: a method
    // of a freed instance is not, unlike with `CallableEqual` (`=`), and is equal to another
    // block of the same method and handle. Closures are equal only as the same block.
    #[test]
    fn callable_key_eq_keeps_freed_methods_apart_from_nil() {
        use heap::CallableKind::{ClassMethod, Closure, Method, Routine};

        let code = vec![intrinsic_word(op::intr::CALLABLE_KEY_EQ)];
        let mut iso = started_isolate(0, Vec::new(), code);
        let obj = iso.heap.alloc_obj(bytecode::TOBJECT_TYPE).unwrap().unwrap();
        let m = callable(&mut iso, Method, 0, obj);
        let m_again = callable(&mut iso, Method, 0, obj);
        let m_other = callable(&mut iso, Method, 1, obj);
        let r = callable(&mut iso, Routine, 0, 0);
        let r_again = callable(&mut iso, Routine, 0, 0);
        let cm = callable(&mut iso, ClassMethod, 0, 1);
        let closure = iso
            .heap
            .alloc_closure(0, heap::Caps::new(&[]).unwrap())
            .unwrap();
        let closure_again = iso
            .heap
            .alloc_closure(0, heap::Caps::new(&[]).unwrap())
            .unwrap();
        assert_eq!(iso.heap.callable(closure).unwrap().kind, Closure);
        iso.heap.begin_destroy(obj).unwrap();
        iso.heap.end_destroy(obj).unwrap();
        for (name, a, b, eq) in [
            ("nil nil", 0, 0, 1),
            ("freed method nil", m, 0, 0),
            ("nil freed method", 0, m, 0),
            ("freed method itself", m, m, 1),
            ("freed method same contents", m, m_again, 1),
            ("other function", m, m_other, 0),
            ("routine same function", r, r_again, 1),
            ("routine and class method", r, cm, 0),
            ("closure itself", closure, closure, 1),
            ("closures of one function", closure, closure_again, 0),
        ] {
            assert_eq!(intrinsic_value(&mut iso, a, b), eq, "{name}");
        }
        // `=` takes the method of the freed instance as equal to `nil`
        let mut iso_eq = started_isolate(
            0,
            Vec::new(),
            vec![intrinsic_word(op::intr::CALLABLE_EQUAL)],
        );
        let obj = iso_eq
            .heap
            .alloc_obj(bytecode::TOBJECT_TYPE)
            .unwrap()
            .unwrap();
        let m = callable(&mut iso_eq, Method, 0, obj);
        iso_eq.heap.begin_destroy(obj).unwrap();
        iso_eq.heap.end_destroy(obj).unwrap();
        assert_eq!(intrinsic_value(&mut iso_eq, m, 0), 1);
    }

    // `CallableHash` (`docs/spec/equality.md` 2.7, `bytecode.md` 2.21) is the same for values
    // `CallableKeyEq` takes as equal (a method of a live or freed instance, a routine), and has
    // the values of the definition for `nil`, a routine and a class method (worked out from the
    // definitions outside this implementation).
    #[test]
    fn callable_hash_agrees_with_key_equality() {
        use heap::CallableKind::{ClassMethod, Method, Routine};

        let mut iso = started_isolate(
            0x1234,
            Vec::new(),
            vec![intrinsic_word(op::intr::CALLABLE_HASH)],
        );
        let obj = iso.heap.alloc_obj(bytecode::TOBJECT_TYPE).unwrap().unwrap();
        let m = callable(&mut iso, Method, 0, obj);
        let m_again = callable(&mut iso, Method, 0, obj);
        let r = callable(&mut iso, Routine, 3, 0);
        let cm = callable(&mut iso, ClassMethod, 2, 66);
        let closure = iso
            .heap
            .alloc_closure(0, heap::Caps::new(&[]).unwrap())
            .unwrap();
        let h_m = intrinsic_value(&mut iso, m, 0);
        assert_eq!(intrinsic_value(&mut iso, m_again, 0), h_m, "live method");
        iso.heap.begin_destroy(obj).unwrap();
        iso.heap.end_destroy(obj).unwrap();
        assert_eq!(intrinsic_value(&mut iso, m, 0), h_m, "freed method");
        assert_eq!(
            intrinsic_value(&mut iso, m_again, 0),
            h_m,
            "freed method again"
        );
        assert_eq!(intrinsic_value(&mut iso, 0, 0), -594279027983241470, "nil");
        assert_eq!(
            intrinsic_value(&mut iso, r, 0),
            8128872522620973467,
            "routine"
        );
        assert_eq!(
            intrinsic_value(&mut iso, cm, 0),
            6690965250415484804,
            "class method"
        );
        // A closure hashes as its block: `HashInt` of the reference, the sixth block made here
        assert_eq!(closure, 6);
        assert_eq!(
            intrinsic_value(&mut iso, closure, 0),
            -7315096489595714179,
            "closure"
        );
    }

    // An isolate made with `HashSeed::FromHost` takes its seed from the host's
    // `Random::hash_seed` (`docs/spec/equality.md` 7.1).
    #[test]
    fn from_host_seed_comes_from_the_random_source() {
        struct OneSeed;
        impl puy_host::Random for OneSeed {
            fn fill(&mut self, _: &mut [u8]) -> Result<(), puy_host::HostError> {
                unreachable!("the seed comes from `hash_seed`")
            }
            fn hash_seed(&mut self) -> Result<u64, puy_host::HostError> {
                Ok(0x1234)
            }
        }
        let module = Arc::new(code_module(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            1,
            vec![crate::bytecode::encode(op::RET_VOID, 0, 0, 0)],
        ));
        let host = || HostSet {
            random: Some(Box::new(OneSeed)),
            ..HostSet::empty()
        };
        let mut iso = Isolate::new(module, host(), IsolateConfig::default()).unwrap();
        assert_eq!(iso.hash_keys, hash::HashKeys::new(0x1234));
        iso.task.regs = vec![0, 0];
        exec_one(&mut iso, op::HASH_INT, 1, 0);
        assert_eq!(iso.task.regs[0], -594279027983241470);
    }

    #[test]
    // `CallIntf` on an instance whose class does not implement the call target's interface is
    // `Fatal` (`docs/spec/objects.md` 14.7): only forged bytecode gets past the type check.
    fn call_intf_without_the_interface_is_fatal() {
        use crate::bytecode::{TypeEntry, encode};

        let types = vec![
            TypeEntry {
                kind: bytecode::TYPE_INTERFACE,
                name: "Test.IRun".into(),
                parent: Some(bytecode::IINTERFACE_TYPE),
                methods: 1,
                ..TypeEntry::default()
            },
            TypeEntry {
                kind: bytecode::TYPE_CLASS,
                name: "Test.TPlain".into(),
                parent: Some(bytecode::TOBJECT_TYPE),
                vmt: bytecode::TOBJECT_VMT.to_vec(),
                ..TypeEntry::default()
            },
        ];
        let code = vec![
            encode(op::NEW, 0, 65, 0),
            encode(op::CALL_INTF, 0, 0, 0),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        match run_code_with_types(types, vec![(64, 0)], Vec::new(), 1, code) {
            RunOutcome::Fatal(e) => assert_eq!(
                e.message,
                "Test.TPlain does not implement interface type 64"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    // A procedural value outside the function table is `Fatal` when it is called or made a
    // callable value (`docs/spec/objects.md` 14.9): only forged bytecode makes one.
    fn procedural_value_out_of_the_function_table_is_fatal() {
        use crate::bytecode::encode;

        let called = vec![
            encode(op::LOAD_INT, 0, 100, 0),
            encode(op::CALL_INDIRECT, 1, 0, 0),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        let made = vec![
            encode(op::LOAD_INT, 1, 50, 0),
            encode(op::INTRINSIC, 0, 1, u32::from(op::intr::ROUTINE_REF)),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        for (idx, (code, expected)) in [
            (called, "bad procedural value 100 at 0001"),
            (made, "bad procedural value 50"),
        ]
        .into_iter()
        .enumerate()
        {
            match run_code(Vec::new(), 2, code) {
                RunOutcome::Fatal(e) => assert_eq!(e.message, expected, "{idx}"),
                other => panic!("{idx}: {other:?}"),
            }
        }
    }

    #[test]
    // `CallClosure` on a heap block that is not a callable value is `Fatal`
    // (`docs/spec/objects.md` 14.9).
    fn call_closure_on_a_string_is_fatal() {
        use crate::bytecode::encode;

        let code = vec![
            encode(op::LOAD_CONST, 0, 0, 0),
            encode(op::CALL_CLOSURE, 1, 0, 0),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        match run_code(vec![ConstEntry::Str("abc".into())], 2, code) {
            RunOutcome::Fatal(e) => assert_eq!(e.message, "1 is not a callable value"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    // Closures and cells made by forged bytecode (`docs/spec/objects.md` 14.9): `CapGet` past the
    // captured cells or of a block that is not a closure, `MakeClosure` with more cells than the
    // frame holds, and `CellRef` of a value that is not a heap reference are `Fatal`.
    fn forged_closures_and_cells_are_fatal() {
        use crate::bytecode::encode;

        let closure = |count: u32| {
            vec![
                encode(op::LOAD_INT, 1, 0, 0),
                encode(op::LOAD_INT, 2, count, 0),
                encode(op::INTRINSIC, 0, 1, u32::from(op::intr::MAKE_CLOSURE)),
            ]
        };
        let mut past_end = closure(0);
        past_end.push(encode(op::CAP_GET, 3, 0, 0));
        let not_closure = vec![
            encode(op::LOAD_CONST, 0, 0, 0),
            encode(op::CAP_GET, 3, 0, 0),
        ];
        let too_many = closure(10);
        // Near the top of a 32-bit `usize` (wasm32), where adding the start would overflow
        let huge = vec![
            encode(op::LOAD_INT, 1, 0, 0),
            encode(op::LOAD_CONST, 2, 1, 0),
            encode(op::INTRINSIC, 0, 1, u32::from(op::intr::MAKE_CLOSURE)),
        ];
        let not_cell = vec![
            encode(op::LOAD_ZERO, 1, 0, 0),
            encode(op::INTRINSIC, 0, 1, u32::from(op::intr::CELL_REF)),
        ];
        for (idx, (mut code, expected)) in [
            (past_end, "capture 0 out of range"),
            (not_closure, "1 is not a closure"),
            (too_many, "bad capture count 10"),
            (huge, "bad capture count 4294967295"),
            (not_cell, "bad cell 0"),
        ]
        .into_iter()
        .enumerate()
        {
            code.push(encode(op::RET_VOID, 0, 0, 0));
            let consts = vec![ConstEntry::Str("abc".into()), ConstEntry::Int(0xFFFF_FFFF)];
            match run_code(consts, 4, code) {
                RunOutcome::Fatal(e) => assert_eq!(e.message, expected, "{idx}"),
                other => panic!("{idx}: {other:?}"),
            }
        }
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

    // An instruction after `Ext` runs as the one numbered past 255, not as the instruction its
    // `op` names (`TestExt` holds the `op` of `AddInt`), short and with `Wide`, and the code after
    // it goes on (`docs/spec/bytecode.md` 1.2).
    #[test]
    fn ext_instruction_runs() {
        use crate::bytecode::{encode, encode_ext};

        for wide in [false, true] {
            let mut code = vec![
                encode(op::LOAD_INT, 1, 50, 0),
                encode(op::LOAD_INT, 2, 8, 0),
            ];
            if wide {
                // r300 := r1 - r2, then r0 := r300
                code.extend([
                    encode(op::WIDE, 0, 0, 0),
                    encode(op::EXT, 0, 0, 0),
                    encode(op::TEST_EXT, 300, 1, 2),
                    encode_ext(300, 1, 2),
                    encode(op::WIDE, 0, 0, 0),
                    encode(op::MOVE, 0, 300, 0),
                    encode_ext(0, 300, 0),
                ]);
            } else {
                code.extend([encode(op::EXT, 0, 0, 0), encode(op::TEST_EXT, 0, 1, 2)]);
            }
            code.push(encode(op::HALT, 0, 0, 0));
            match run_code(Vec::new(), 301, code) {
                RunOutcome::Finished(status) => {
                    assert_eq!(status.kind, ExitKind::Code(42), "{wide}")
                }
                other => panic!("{wide}: {other:?}"),
            }
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

    // A zero immediate divisor of `DivIntI` / `ModIntI`, which the compiler never emits, raises
    // `EDivByZero` in both the short and the `Wide` form (`docs/spec/bytecode.md` 2.2; M3-72)
    #[test]
    fn zero_immediate_divisor_raises() {
        use crate::bytecode::{encode, encode_ext};

        for opc in [op::DIV_INT_I, op::MOD_INT_I] {
            for wide in [false, true] {
                let mut code = vec![encode(op::LOAD_INT, 0, 7, 0)];
                if wide {
                    code.extend([
                        encode(op::WIDE, 0, 0, 0),
                        encode(opc, 0, 0, 0),
                        encode_ext(0, 0, 0),
                    ]);
                } else {
                    code.push(encode(opc, 0, 0, 0));
                }
                code.push(encode(op::HALT, 0, 0, 0));
                match run_code(Vec::new(), 1, code) {
                    RunOutcome::Finished(status) => match status.kind {
                        ExitKind::UncaughtException(rep) => {
                            assert_eq!(rep.class, "EDivByZero", "{opc} {wide}");
                            assert_eq!(rep.message, "division by zero", "{opc} {wide}");
                        }
                        other => panic!("{opc} {wide}: {other:?}"),
                    },
                    other => panic!("{opc} {wide}: {other:?}"),
                }
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

    // `Isolate::new` verifies the module, since the fast path reads the instruction stream without
    // bounds checks (`docs/design/vm.md` 3.14): a stream that runs off its end is refused.
    #[test]
    fn isolate_refuses_an_unverified_module() {
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            b"program T;\nbegin end.".to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        let bytes = out.module.expect("module");
        let mut module = Module::decode(&bytes, crate::VERSION).unwrap();
        let code = &mut module.funcs[module.entry as usize].code;
        let last = code.len() - 1;
        assert_eq!(bytecode::decode(code[last]).0, op::word::RET_VOID);
        code[last] = bytecode::encode(op::NOP, 0, 0, 0);
        let config = IsolateConfig {
            hash_seed: HashSeed::Fixed(0),
            ..Default::default()
        };
        let Err(e) = Isolate::new(Arc::new(module), HostSet::empty(), config) else {
            panic!("an unverified module was accepted");
        };
        assert_eq!(
            e.to_string(),
            "invalid bytecode module: function 0 (T.<main>): last instruction must be Ret, RetVoid, Halt, Jmp or Raise"
        );
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
            "UTF8String",
        ];
        let leaks = count_leaks(names.into_iter().map(std::borrow::Cow::Borrowed));
        assert_eq!(
            render_leaks(&leaks),
            "warning: 6 heap blocks were not freed\n  Hello.TNode: 2\n  TArray<Exception>: 1\n  UTF8String: 1\n  array of Hello.TNode: 1\n  string: 1\n"
        );
        assert_eq!(
            leaks_json(&leaks),
            r#"{"schema":"puy.leaks/1","blocks":6,"types":[{"type":"Hello.TNode","count":2},{"type":"TArray<Exception>","count":1},{"type":"UTF8String","count":1},{"type":"array of Hello.TNode","count":1},{"type":"string","count":1}]}"#
        );
    }

    // `AsClassRef` with a target that is not a class (which the code generator never passes) is
    // an internal error, not an `EInvalidCast` a program could catch (objects.md 14.3).
    #[test]
    fn as_class_ref_of_a_non_class_target_is_fatal() {
        let src = "program T;\ntype TA = class end;\n  TAC = class of TA;\nvar K: TClass; C: TAC;\nbegin\n  K := TA;\n  try C := TAC(K); except end;\nend.";
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        let bytes = out.module.expect("module");
        let mut module = Module::decode(&bytes, crate::VERSION).unwrap();
        let code = &mut module.funcs[module.entry as usize].code;
        let at = code
            .iter()
            .position(|&w| {
                let (opc, _, _, c) = bytecode::decode(w);
                opc == op::word::INTRINSIC && c == u32::from(op::intr::AS_CLASS_REF)
            })
            .expect("AsClassRef");
        // The `LoadInt` of the target before it: type index 12 (`TArray<Exception>`)
        let (opc, a, _, _) = bytecode::decode(code[at - 1]);
        assert_eq!(opc, op::word::LOAD_INT);
        code[at - 1] = bytecode::encode(op::LOAD_INT, a, 12, 0);
        let config = IsolateConfig {
            hash_seed: HashSeed::Fixed(0),
            ..Default::default()
        };
        let mut iso = Isolate::new(Arc::new(module), HostSet::empty(), config).unwrap();
        iso.start_main(vec!["t.pas".into()]).unwrap();
        match iso.run(1000) {
            RunOutcome::Fatal(e) => assert_eq!(e.message, "bad type index 12"),
            other => panic!("expected Fatal, got {other:?}"),
        }
    }

    // `Raise` of an instance whose class is not an exception is only made by forged bytecode (the
    // compiler reports E3021), so it is an internal error, not an exception a program could catch
    // (`docs/spec/objects.md` 14.8).
    #[test]
    fn raise_of_a_non_exception_is_fatal() {
        let src = "program T;\ntype\n  EBox = class(Exception) end;\n  TBox = class S: string; end;\nvar B: TBox;\nbegin\n  B := TBox.Create;\n  B.Free;\n  raise EBox.Create('x');\nend.";
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        let bytes = out.module.expect("module");
        let mut module = Module::decode(&bytes, crate::VERSION).unwrap();
        let tbox = (bytecode::FIRST_TYPE_INDEX..)
            .find(|&t| module.type_entry(t).is_some_and(|e| e.name == "T.TBox"))
            .expect("TBox");
        let ebox = (bytecode::FIRST_TYPE_INDEX..)
            .find(|&t| module.type_entry(t).is_some_and(|e| e.name == "T.EBox"))
            .expect("EBox");
        // The `New` of the raised exception makes a `TBox` instead (its slot 0 is a string too)
        let code = &mut module.funcs[module.entry as usize].code;
        let at = code
            .iter()
            .position(|&w| {
                let (opc, ..) = bytecode::decode(w);
                opc == op::word::NEW && bytecode::decode_bx(w, None) == ebox
            })
            .expect("New of EBox");
        let (_, a, _, _) = bytecode::decode(code[at]);
        code[at] = bytecode::encode(op::NEW, a, tbox, 0);
        let config = IsolateConfig {
            hash_seed: HashSeed::Fixed(0),
            ..Default::default()
        };
        let mut iso = Isolate::new(Arc::new(module), HostSet::empty(), config).unwrap();
        iso.start_main(vec!["t.pas".into()]).unwrap();
        match iso.run(1000) {
            RunOutcome::Fatal(e) => {
                assert!(
                    e.message.ends_with("which is not an exception"),
                    "{}",
                    e.message
                )
            }
            other => panic!("expected Fatal, got {other:?}"),
        }
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

    // ---- Complexity guarantees (`docs/spec/types.md` 9.1) ----

    /// How many times more buffer elements `src` reserves (`heap::reserved`) with `{N}` at 4n than
    /// at n. Work linear in n gives about 4; copying the whole value at every step, about 16.
    /// Each program prints `{N}`'s result, checked here so that the loop really ran.
    fn reserved_ratio(src: &str, n: u32, expect: impl Fn(u32) -> String) -> f64 {
        let mut counts = [0u64; 2];
        for (count, m) in counts.iter_mut().zip([n, 4 * n]) {
            let program = src.replace("{N}", &m.to_string());
            let before = heap::reserved::total();
            let printed = out(&program);
            *count = heap::reserved::total() - before;
            assert_eq!(printed, expect(m), "{program}");
        }
        counts[1] as f64 / counts[0] as f64
    }

    /// How many times more instructions `src` runs (`executed`) with `{N}` at 4n than at n. Work
    /// linear in n gives about 4; a search through every entry at every step, about 16. Each
    /// program prints `{N}`'s result, checked here so that the loops really ran.
    fn executed_ratio(src: &str, n: u32, expect: impl Fn(u32) -> String) -> f64 {
        let mut counts = [0u64; 2];
        for (count, m) in counts.iter_mut().zip([n, 4 * n]) {
            let program = src.replace("{N}", &m.to_string());
            let before = executed::total();
            let printed = out(&program);
            *count = executed::total() - before;
            assert_eq!(printed, expect(m), "{program}");
        }
        counts[1] as f64 / counts[0] as f64
    }

    // Adding n keys to a dictionary, looking each up and removing each is O(n) on average for
    // keys of an ordinal type, a string and a record (collections.md 11.1, 11.3 (1)-(3))
    #[test]
    fn dictionary_operations_are_constant_on_average() {
        let keys = [
            ("Integer", "I * 7919"),
            ("string", "IntToStr(I)"),
            (
                "TPair<Integer, string>",
                "TPair<Integer, string>.Create(I, 'k')",
            ),
        ];
        for (idx, (ty, key)) in keys.iter().enumerate() {
            let src = format!(
                "program T;
                 uses System.SysUtils, System.Generics.Collections;
                 var D: TDictionary<{ty}, Integer>; I, V, Sum: Integer;
                 begin
                   D := TDictionary<{ty}, Integer>.Create;
                   for I := 1 to {{N}} do D.Add({key}, I);
                   Sum := 0;
                   for I := 1 to {{N}} do
                     if D.TryGetValue({key}, V) then Sum := Sum + 1;
                   for I := 1 to {{N}} do D.Remove({key});
                   WriteLn(Sum, ' ', D.Count);
                   D.Free;
                 end."
            );
            let ratio = executed_ratio(&src, 2000, |m| format!("{m} 0\n"));
            assert!(ratio < 6.0, "{idx} {ty}: {ratio}");
        }
    }

    // A set shares the dictionary's hash table: adding n items, looking each up and removing each
    // is O(n) on average, and so are the set operations over n items (collections.md 11.1, 11.3
    // (4))
    #[test]
    fn hash_set_operations_are_constant_on_average() {
        let items = [("Integer", "I * 7919"), ("string", "IntToStr(I)")];
        for (idx, (ty, item)) in items.iter().enumerate() {
            let src = format!(
                "program T;
                 uses System.SysUtils, System.Generics.Collections;
                 var S, O: THashSet<{ty}>; I, Sum: Integer;
                 begin
                   S := THashSet<{ty}>.Create;
                   O := THashSet<{ty}>.Create;
                   for I := 1 to {{N}} do S.Add({item});
                   for I := 1 to {{N}} do if I mod 2 = 0 then O.Add({item});
                   Sum := 0;
                   for I := 1 to {{N}} do
                     if S.Contains({item}) then Sum := Sum + 1;
                   S.IntersectWith(O);
                   S.UnionWith(O);
                   S.ExceptWith(O);
                   for I := 1 to {{N}} do S.Add({item});
                   for I := 1 to {{N}} do S.Remove({item});
                   WriteLn(Sum, ' ', S.Count);
                   S.Free;
                   O.Free;
                 end."
            );
            let ratio = executed_ratio(&src, 2000, |m| format!("{m} 0\n"));
            assert!(ratio < 6.0, "{idx} {ty}: {ratio}");
        }
    }

    // Adding and removing in turn leaves holes that are squeezed out from time to time; that
    // stays O(1) on average per operation, also when the capacity is far larger than the entries
    // (the capacity grows with n here, so a squeeze costing the capacity makes the ratio about 16)
    // (collections.md 7.3, 11.3 (5))
    #[test]
    fn dictionary_add_remove_in_turn_is_constant_on_average() {
        let cases = [
            ("Create", "D.Capacity <= 128"),
            ("Create(8 * {N})", "D.Capacity = 8 * {N}"),
        ];
        for (idx, (create, capacity)) in cases.iter().enumerate() {
            let src = format!(
                "program T;
                 uses System.SysUtils, System.Generics.Collections;
                 var D: TDictionary<Integer, Integer>; I: Integer;
                 begin
                   D := TDictionary<Integer, Integer>.{create};
                   for I := 1 to 50 do D.Add(-I, I);
                   for I := 1 to {{N}} do
                   begin
                     D.Add(I, I);
                     D.Remove(I);
                   end;
                   WriteLn(D.Count, ' ', {capacity});
                   D.Free;
                 end."
            );
            let ratio = executed_ratio(&src, 2000, |_| "50 TRUE\n".to_string());
            assert!(ratio < 6.0, "{idx} {create}: {ratio}");
        }
    }

    // TList<T>.Sort is O(n log n) (collections.md 10.3, 11.2): from n to 4n items the executed
    // instructions grow about 4.7 times, not 16, for scattered, descending and equal items
    #[test]
    fn list_sort_is_n_log_n() {
        let items = ["(I * 7919) mod 10007", "{N} - I", "I mod 2"];
        for (idx, item) in items.iter().enumerate() {
            let src = format!(
                "program T;
                 uses System.SysUtils, System.Generics.Collections;
                 var L: TList<Integer>; I: Integer; Ok: Boolean;
                 begin
                   L := TList<Integer>.Create;
                   for I := 1 to {{N}} do L.Add({item});
                   L.Sort;
                   Ok := True;
                   for I := 1 to L.Count - 1 do
                     if L[I - 1] > L[I] then Ok := False;
                   WriteLn(L.Count, ' ', Ok);
                   L.Free;
                 end."
            );
            let ratio = executed_ratio(&src, 2000, |m| format!("{m} TRUE\n"));
            assert!(ratio < 6.0, "{idx} {item}: {ratio}");
        }
    }

    // A change of the structure at change number MaxInt wraps the number round to 0 instead of
    // raising EIntOverflow under {$Q+}, and an enumerator made before it still sees the change
    // (collections.md 3.3). The number cannot reach MaxInt quickly from a program, so the test
    // reads a copy of the unit with its private sections made public and sets the number. The
    // capacity is set first, so that the change does not also grow the container.
    #[test]
    fn container_change_number_wraps_after_max_int() {
        let unit = puy_stdlib::find("System.Generics.Collections").expect("the unit");
        let open = puy_stdlib::Unit {
            name: unit.name,
            // Line by line, so that a checkout with CRLF line ends reads the same
            source: Box::leak(
                unit.source
                    .split_inclusive('\n')
                    .map(|line| match line.trim() {
                        "private" => line.replacen("private", "public", 1),
                        _ => line.to_string(),
                    })
                    .collect::<String>()
                    .into_boxed_str(),
            ),
        };
        let cases = [
            ("TList<Integer>", "TEnumerator", "C.Add(1)", "C.FVersion"),
            ("TStack<Integer>", "TEnumerator", "C.Push(1)", "C.FVersion"),
            (
                "TQueue<Integer>",
                "TEnumerator",
                "C.Enqueue(1)",
                "C.FVersion",
            ),
            // Written without the space, as ClassName writes it
            (
                "TDictionary<Integer,Integer>",
                "TPairEnumerator",
                "C.Add(1, 1)",
                "C.FVersion",
            ),
            (
                "THashSet<Integer>",
                "TEnumerator",
                "C.Add(1)",
                "C.FTable.FVersion",
            ),
        ];
        for (ty, enumerator, change, version) in cases {
            let src = format!(
                "program T;
                 {{$Q+}}
                 uses System.SysUtils, System.Generics.Collections;
                 type TC = {ty};
                 var C: TC; E: TC.{enumerator};
                 begin
                   C := TC.Create;
                   C.Capacity := 4;
                   {version} := MaxInt;
                   E := C.GetEnumerator;
                   {change};
                   WriteLn({version});
                   try
                     E.MoveNext;
                     WriteLn('not detected');
                   except
                     on X: EInvalidOperation do WriteLn(X.Message);
                   end;
                   C.Free;
                 end."
            );
            let config = IsolateConfig {
                hash_seed: HashSeed::Fixed(0),
                ..Default::default()
            };
            let r = run_with_stdlib(&src, "", &[], config, &[open]);
            assert!(
                matches!(r.status.kind, ExitKind::Code(0)),
                "{ty}: {:?}\n{}",
                r.status.kind,
                r.stdout
            );
            assert_eq!(
                r.stdout,
                format!("0\n`{ty}` was modified during enumeration\n"),
                "{ty}"
            );
        }
    }

    // Appending to a string place grows the place's own string with capacity to spare, so n
    // appends reserve O(n) in all, for every kind of place (types.md 9.1)
    #[test]
    fn string_append_is_amortized_constant() {
        let places = [
            ("var S: string = ''; I: Integer;", "S", ""),
            ("var I: Integer;", "G", ""),
            ("var I: Integer;", "V", ""),
            ("var R: TRec = Default(TRec); I: Integer;", "R.S", ""),
            ("var I: Integer;", "GR.S", ""),
            (
                "var A: array of string = nil; I: Integer;",
                "A[1]",
                "SetLength(A, 2);",
            ),
            (
                "var F: array[1..2] of string = ('', ''); I: Integer;",
                "F[2]",
                "",
            ),
        ];
        for (idx, (decl, place, setup)) in places.iter().enumerate() {
            let src = format!(
                "program T;
                 type TRec = record N: Integer; S: string; end;
                 var G: string; GR: TRec;
                 procedure P(var V: string);
                 {decl}
                 begin
                   {setup}
                   for I := 1 to {{N}} do {place} := {place} + 'ab' + 'c';
                   WriteLn(Length({place}));
                 end;
                 var T: string;
                 begin T := ''; P(T); end."
            );
            let ratio = reserved_ratio(&src, 2000, |m| format!("{}\n", 3 * m));
            assert!(ratio < 6.0, "{idx} {place}: {ratio}");
        }
    }

    // Appending to an `out` parameter and to a variable of the enclosing routine (both reached
    // through a reference slot, like a `var` parameter) is amortized O(1) too (types.md 9.1)
    #[test]
    fn append_through_reference_slots_is_amortized_constant() {
        let src = "program T;
                   procedure P(out O: string);
                   var W: string; I: Integer;
                     procedure Inner;
                     var J: Integer;
                     begin
                       for J := 1 to {N} do W := W + 'abc';
                     end;
                   begin
                     W := '';
                     Inner;
                     O := '';
                     for I := 1 to {N} do O := O + 'de';
                     WriteLn(Length(W) + Length(O));
                   end;
                   var T: string;
                   begin P(T); end.";
        let ratio = reserved_ratio(src, 2000, |m| format!("{}\n", 5 * m));
        assert!(ratio < 6.0, "{ratio}");
    }

    // Several appends to the same place in one loop body: the copy of the place each append takes
    // is let go at once, so the next append still finds the place's reference the only one
    #[test]
    fn appends_in_one_loop_body_are_amortized_constant() {
        let src = "program T;
                   var S: string; U: UTF8String; I: Integer;
                   begin
                     S := ''; U := '';
                     for I := 1 to {N} do
                     begin
                       S := S + 'a';
                       S := S + 'b';
                       U := U + 'c';
                       U := U + 'd';
                     end;
                     WriteLn(Length(S) + Length(U));
                   end.";
        let ratio = reserved_ratio(src, 2000, |m| format!("{}\n", 4 * m));
        assert!(ratio < 6.0, "{ratio}");
    }

    // Appending to a UTF8String place: UTF8String operands, and string operands converted once
    // (types.md 9.1)
    #[test]
    fn utf8_append_is_amortized_constant() {
        for (idx, operand) in ["X", "'é'", "'é' + 'x'"].iter().enumerate() {
            let src = format!(
                "program T;
                 var U: UTF8String; X: UTF8String; I: Integer;
                 begin
                   U := ''; X := 'é';
                   for I := 1 to {{N}} do U := U + {operand};
                   WriteLn(Length(U));
                 end."
            );
            let per = [2, 2, 3][idx];
            let ratio = reserved_ratio(&src, 2000, |m| format!("{}\n", per * m));
            assert!(ratio < 6.0, "{idx} {operand}: {ratio}");
        }
    }

    // A string that becomes UTF-16 while it grows is copied once, not once per append
    #[test]
    fn string_append_widening_is_amortized_constant() {
        let src = "program T;
                   var S: string; I: Integer;
                   begin
                     S := '';
                     for I := 1 to {N} do
                       if I = {N} div 2 then S := S + 'é' else S := S + 'x';
                     WriteLn(Length(S));
                   end.";
        let ratio = reserved_ratio(src, 2000, |m| format!("{m}\n"));
        assert!(ratio < 6.0, "{ratio}");
    }

    // Growing a dynamic array one element at a time with SetLength is amortized O(1) for every
    // kind of place, including `Length` of a global read in the same statement (types.md 9.1)
    #[test]
    fn set_length_growth_is_amortized_constant() {
        let places = [
            ("var A: array of Integer = nil; I: Integer;", "A"),
            ("var I: Integer;", "GA"),
            ("var I: Integer;", "V"),
            ("var R: TRec = Default(TRec); I: Integer;", "R.A"),
        ];
        for (idx, (decl, place)) in places.iter().enumerate() {
            let src = format!(
                "program T;
                 type TRec = record N: Integer; A: array of Integer; end;
                 var GA: array of Integer;
                 procedure P(var V: TArray<Integer>);
                 {decl}
                 begin
                   for I := 1 to {{N}} do SetLength({place}, Length({place}) + 1);
                   WriteLn(Length({place}));
                 end;
                 var T: TArray<Integer>;
                 begin T := nil; P(T); end."
            );
            let ratio = reserved_ratio(&src, 2000, |m| format!("{m}\n"));
            assert!(ratio < 6.0, "{idx} {place}: {ratio}");
        }
    }

    // Writing a character of a string whose place holds its only reference writes in place, for
    // every kind of place: only making the string reserves (types.md 9.1)
    #[test]
    fn string_index_write_does_not_copy() {
        let places = [
            ("var S: string = ''; I: Integer;", "S", ""),
            ("var I: Integer;", "G", ""),
            ("var I: Integer;", "V", ""),
            ("var R: TRec = Default(TRec); I: Integer;", "R.S", ""),
            (
                "var A: array of string = nil; I: Integer;",
                "A[0]",
                "SetLength(A, 1);",
            ),
        ];
        for (idx, (decl, place, setup)) in places.iter().enumerate() {
            let src = format!(
                "program T;
                 uses System.SysUtils;
                 type TRec = record N: Integer; S: string; end;
                 var G: string;
                 procedure P(var V: string);
                 {decl}
                 begin
                   {setup}
                   {place} := StringOfChar('a', {{N}});
                   for I := 1 to {{N}} do {place}[I] := 'b';
                   WriteLn({place}[{{N}}], Length({place}));
                 end;
                 var T: string;
                 begin T := ''; P(T); end."
            );
            let ratio = reserved_ratio(&src, 2000, |m| format!("b{m}\n"));
            assert!(ratio < 6.0, "{idx} {place}: {ratio}");
        }
    }

    // ---- Hash table instructions (`docs/spec/bytecode.md` 2.23; M4-113) ----

    /// The code of a `<main>` that makes `r0` a dynamic array (type 64) of the integers `values`,
    /// with `r1` and `r2` as scratch registers.
    fn int_array(values: &[i32]) -> Vec<u32> {
        use crate::bytecode::encode;
        let mut code = vec![
            word_bx(op::LOAD_INT, 0, values.len() as i32),
            word_bx(op::NEW_ARR, 0, 64),
        ];
        for (i, v) in values.iter().enumerate() {
            code.extend([
                word_bx(op::LOAD_INT, 1, i as i32),
                word_bx(op::LOAD_INT, 2, *v),
                encode(op::DYN_SET, 0, 1, 2),
            ]);
        }
        code
    }

    /// Runs `code` after making `r10` the buckets, `r11` the hashes and `r12` the next entries
    /// of a hash table (dynamic arrays of type 64; `r0` to `r2` are scratch registers), and `r19`
    /// an instance (type 66) whose slots 1 to 4 are the buckets, the hashes, the next entries
    /// and, as the keys, the hashes again (slot 0 is an integer, 0), with the managed registers
    /// `r10` to `r12` and `r20` to `r29` free for `code`. Type 65 is a dynamic array of two-slot
    /// records and constant 0 the string `'ab'`.
    fn hash_table_run(buckets: &[i32], hashes: &[i32], next: &[i32], code: &[u32]) -> RunOutcome {
        use crate::bytecode::{
            SLOT_PLAIN, SLOT_REF, TOBJECT_TYPE, TOBJECT_VMT, TYPE_CLASS, TYPE_DYNARRAY, TypeEntry,
            encode,
        };
        let mut all = Vec::new();
        for (r, values) in [(10, buckets), (11, hashes), (12, next)] {
            all.extend(int_array(values));
            all.push(encode(op::MOVE_REF, r, 0, 0));
        }
        all.push(word_bx(op::NEW, 19, 66));
        for (slot, r) in [(1, 10), (2, 11), (3, 12), (4, 11)] {
            all.push(encode(op::OBJ_SET_REF, 19, slot, r));
        }
        all.extend_from_slice(code);
        let types = vec![
            TypeEntry {
                kind: TYPE_DYNARRAY,
                refs: vec![SLOT_PLAIN],
                width: 1,
                name: "array of Int64".into(),
                ..TypeEntry::default()
            },
            flat_types().remove(1),
            TypeEntry {
                kind: TYPE_CLASS,
                slots: 5,
                refs: vec![SLOT_PLAIN, SLOT_REF, SLOT_REF, SLOT_REF, SLOT_REF],
                name: "Test.TTable".into(),
                parent: Some(TOBJECT_TYPE),
                vmt: TOBJECT_VMT.to_vec(),
                ..TypeEntry::default()
            },
        ];
        let mut iso = funcs_isolate(
            types,
            vec![ConstEntry::Str("ab".into())],
            vec![TestFunc {
                params: 0,
                regs: 30,
                managed: &[0, 10, 11, 12],
                code: all,
            }],
        );
        iso.run(10_000)
    }

    /// `rA := rB[i]` for the array in `rB` with `r1` as scratch.
    fn elem(a: u32, b: u32, i: i32) -> [u32; 2] {
        [
            word_bx(op::LOAD_INT, 1, i),
            crate::bytecode::encode(op::DYN_GET, a, b, 1),
        ]
    }

    #[test]
    // `HashRelink` links each entry that is not a hole at the front of the chain of its bucket
    // (the later entries first), and `HashFind` follows a chain from its bucket or after an
    // entry.
    fn hash_table_instructions_link_and_follow() {
        use crate::bytecode::encode;
        // Hashes 5, 9, 2, 13 (buckets 1, 1, 2, 1 of 4), the third entry a hole (-2)
        let mut code = vec![
            // HashRelink(Buckets, Hashes, Next, 4)
            encode(op::MOVE, 20, 11, 0),
            encode(op::MOVE, 21, 12, 0),
            word_bx(op::LOAD_INT, 22, 4),
            encode(op::INTRINSIC, 10, 20, u32::from(op::intr::HASH_RELINK)),
        ];
        // Buckets[1], Next[3], Next[1], Next[0], Next[2], Buckets[2]
        code.extend(elem(3, 10, 1));
        code.extend(elem(4, 12, 3));
        code.extend(elem(5, 12, 1));
        code.extend(elem(6, 12, 0));
        code.extend(elem(7, 12, 2));
        code.extend(elem(8, 10, 2));
        code.extend([
            // HashFind(9, ..., -1), HashFind(9, ..., 1), HashFind(2, ..., -1)
            word_bx(op::LOAD_INT, 20, 9),
            encode(op::MOVE, 21, 10, 0),
            encode(op::MOVE, 22, 11, 0),
            encode(op::MOVE, 23, 12, 0),
            word_bx(op::LOAD_INT, 24, -1),
            encode(op::HASH_FIND, 13, 20, 0),
            word_bx(op::LOAD_INT, 24, 1),
            encode(op::HASH_FIND, 14, 20, 0),
            word_bx(op::LOAD_INT, 20, 2),
            word_bx(op::LOAD_INT, 24, -1),
            encode(op::HASH_FIND, 15, 20, 0),
        ]);
        // Buckets[1] = 3 → Next[3] = 1 → Next[1] = 0 → Next[0] = -1; the hole keeps its -2, and
        // Buckets[2] stays empty; then the lookups
        let expected = [
            (3, 3),
            (4, 1),
            (5, 0),
            (6, -1),
            (7, -2),
            (8, -1),
            (13, 1),
            (14, -1),
            (15, -1),
        ];
        code.extend(halt_unless(&expected));
        match hash_table_run(&[0, 0, 0, 0], &[5, 9, 2, 13], &[0, 0, -2, 0], &code) {
            RunOutcome::Finished(st) => assert_eq!(st.kind, ExitKind::Code(0)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    // A table the standard library does not make stops the isolate (`Fatal`): buckets whose
    // length is not a power of 2, a chain that goes round, an entry out of the arrays, and a key
    // that is not a string looked for as a string.
    fn hash_table_instructions_refuse_bad_tables() {
        use crate::bytecode::encode;
        let find = [
            word_bx(op::LOAD_INT, 20, 9),
            encode(op::MOVE, 21, 10, 0),
            encode(op::MOVE, 22, 11, 0),
            encode(op::MOVE, 23, 12, 0),
            word_bx(op::LOAD_INT, 24, -1),
            encode(op::HASH_FIND, 13, 20, 0),
            encode(op::HALT, 13, 0, 0),
        ];
        let lookup_str = [
            encode(op::MOVE, 20, 19, 0),
            word_bx(op::LOAD_INT, 21, 9),
            encode(op::HASH_LOOKUP_STR, 13, 20, 1),
            encode(op::HALT, 13, 0, 0),
        ];
        let relink = [
            encode(op::MOVE, 20, 11, 0),
            encode(op::MOVE, 21, 12, 0),
            word_bx(op::LOAD_INT, 22, 2),
            encode(op::INTRINSIC, 10, 20, u32::from(op::intr::HASH_RELINK)),
            encode(op::HALT, 22, 0, 0),
        ];
        // The buckets, the hashes, the next entries, the code and a part of the message
        type Case<'a> = (&'a [i32], &'a [i32], &'a [i32], &'a [u32], &'a str);
        let cases: [Case; 6] = [
            (&[0, 0, 0], &[9], &[-1], &find, "not a power of 2"),
            (&[0], &[1, 1], &[1, 0], &find, "goes round"),
            (&[5], &[9], &[-1], &find, "out of range"),
            // The key 9 is no heap block
            (&[0], &[9], &[-1], &lookup_str, "heap reference 9"),
            (&[0, 0, 0], &[9, 9], &[0, 0], &relink, "not a power of 2"),
            (&[], &[9, 9], &[0, 0], &relink, "no bucket"),
        ];
        for (i, (buckets, hashes, next, code, what)) in cases.into_iter().enumerate() {
            match hash_table_run(buckets, hashes, next, code) {
                RunOutcome::Fatal(e) => assert!(e.message.contains(what), "{i}: {}", e.message),
                other => panic!("{i}: {other:?}"),
            }
        }
    }

    #[test]
    // `HashLookupInt` / `HashLookupStr` read the fields of their instance as `ObjGet` does: a
    // block that is no instance and a slot past the instance's (`C + 3` of 5 slots) stop the
    // isolate (`Fatal`), and `nil` and a destroyed instance raise `EAccessViolation`
    // (bytecode.md 2.23, objects.md 14.4).
    fn hash_lookup_checks_its_instance() {
        use crate::bytecode::encode;
        // The code that leaves the instance in `r20`, and the slot of the first field
        let run = |instance: &[u32], slot: u32, opc: u16| {
            let mut code = instance.to_vec();
            // The key 0 (also the empty string)
            code.extend([
                encode(op::LOAD_ZERO, 21, 0, 0),
                encode(opc, 13, 20, slot),
                encode(op::HALT, 13, 0, 0),
            ]);
            hash_table_run(&[0, 0], &[9], &[-1], &code)
        };
        let this = [encode(op::MOVE, 20, 19, 0)];
        let freed = [
            encode(op::MOVE, 20, 19, 0),
            encode(op::INTRINSIC, 19, 0, u32::from(op::intr::FREE)),
        ];
        for (idx, opc) in [op::HASH_LOOKUP_INT, op::HASH_LOOKUP_STR]
            .into_iter()
            .enumerate()
        {
            // The table read from its fields: nothing found, an exit code of 255 (-1)
            match run(&this, 1, opc) {
                RunOutcome::Finished(st) => assert_eq!(st.kind, ExitKind::Code(255), "{idx}"),
                other => panic!("{idx}: {other:?}"),
            }
            let fatal: [(&[u32], u32, &str); 2] = [
                // A dynamic array (the buckets)
                (
                    &[encode(op::MOVE, 20, 10, 0)],
                    1,
                    "does not refer to an instance",
                ),
                (&this, 2, "slot 5 out of range"),
            ];
            for (case, (instance, slot, what)) in fatal.into_iter().enumerate() {
                match run(instance, slot, opc) {
                    RunOutcome::Fatal(e) => {
                        assert!(e.message.contains(what), "{idx} {case}: {}", e.message)
                    }
                    other => panic!("{idx} {case}: {other:?}"),
                }
            }
            let raised: [(&[u32], &str); 2] = [
                (&[encode(op::LOAD_ZERO, 20, 0, 0)], NIL_ACCESS),
                (&freed, FREED_ACCESS),
            ];
            for (case, (instance, message)) in raised.into_iter().enumerate() {
                match run(instance, 1, opc) {
                    RunOutcome::Finished(st) => match st.kind {
                        ExitKind::UncaughtException(e) => assert_eq!(
                            (e.class.as_str(), e.message.as_str()),
                            ("EAccessViolation", message),
                            "{idx} {case}"
                        ),
                        other => panic!("{idx} {case}: {other:?}"),
                    },
                    other => panic!("{idx} {case}: {other:?}"),
                }
            }
        }
    }

    #[test]
    // Each array of a hash table that is not a dynamic array of one-slot elements stops the
    // isolate (`Fatal`) naming the array, in whichever operand of `HashFind` and `HashRelink` or
    // field of the instance of `HashLookupInt` it is: a string, an array of two-slot elements,
    // and no heap block.
    fn hash_table_instructions_refuse_arrays_of_wrong_shape() {
        use crate::bytecode::encode;
        // The code that leaves the wrong array in `r0`, and a part of the message
        let bad: [(&[u32], &str); 3] = [
            (&[encode(op::LOAD_CONST, 0, 0, 0)], "not a dynamic array"),
            (
                &[word_bx(op::LOAD_INT, 0, 1), word_bx(op::NEW_ARR, 0, 65)],
                "not a dynamic array",
            ),
            (&[word_bx(op::LOAD_INT, 0, 9)], "heap reference 9"),
        ];
        // The code before the wrong array is put in, the instruction that puts it (from `r0`),
        // the code after it and the name of the array in the message
        let find_head = [
            word_bx(op::LOAD_INT, 20, 9),
            encode(op::MOVE, 21, 10, 0),
            encode(op::MOVE, 22, 11, 0),
            encode(op::MOVE, 23, 12, 0),
            word_bx(op::LOAD_INT, 24, -1),
        ];

        let relink_head = [
            encode(op::MOVE, 26, 10, 0),
            encode(op::MOVE, 20, 11, 0),
            encode(op::MOVE, 21, 12, 0),
            word_bx(op::LOAD_INT, 22, 1),
        ];
        let find = [encode(op::HASH_FIND, 13, 20, 0), encode(op::HALT, 13, 0, 0)];
        // The field written without counting, so that a value that is no heap block goes in
        let lookup = [
            encode(op::MOVE, 20, 19, 0),
            word_bx(op::LOAD_INT, 21, 9),
            encode(op::HASH_LOOKUP_INT, 13, 20, 1),
            encode(op::HALT, 13, 0, 0),
        ];
        let reg = |r| encode(op::MOVE, r, 0, 0);
        let field = |slot| encode(op::OBJ_SET, 19, slot, 0);
        let relink = [
            encode(op::INTRINSIC, 26, 20, u32::from(op::intr::HASH_RELINK)),
            encode(op::HALT, 22, 0, 0),
        ];
        type Place<'a> = (&'a [u32], u32, &'a [u32], &'a str);
        let places: [Place; 10] = [
            (&find_head, reg(21), &find, "the buckets"),
            (&find_head, reg(22), &find, "the hashes"),
            (&find_head, reg(23), &find, "the next entries"),
            (&[], field(1), &lookup, "the buckets"),
            (&[], field(2), &lookup, "the hashes"),
            (&[], field(3), &lookup, "the next entries"),
            (&[], field(4), &lookup, "the keys"),
            (&relink_head, reg(26), &relink, "the buckets"),
            (&relink_head, reg(20), &relink, "the hashes"),
            (&relink_head, reg(21), &relink, "the next entries"),
        ];
        for (p, (head, put, tail, name)) in places.into_iter().enumerate() {
            for (b, (make, what)) in bad.iter().enumerate() {
                let mut code = make.to_vec();
                code.extend_from_slice(head);
                code.push(put);
                code.extend_from_slice(tail);
                match hash_table_run(&[0, 0], &[9], &[-1], &code) {
                    RunOutcome::Fatal(e) => {
                        assert!(e.message.contains(what), "{p} {b}: {}", e.message);
                        // An invalid reference is named as a read of it, as elsewhere
                        if b != 2 {
                            assert!(e.message.starts_with(name), "{p} {b}: {}", e.message);
                        }
                    }
                    other => panic!("{p} {b}: {other:?}"),
                }
            }
        }
    }

    /// An isolate whose `<main>` makes `r10` a dynamic array of `values` (type 64, `Int64`), or
    /// with `flat` of `values.len()` two-slot records (type 65), then runs `ArraySortRange` on all
    /// of it with the comparison `kind` (`I96`) and halts; stopped before its first instruction.
    /// Returns the isolate and the number of instructions before the sort.
    fn sort_isolate(values: &[i32], kind: i32, flat: bool) -> (Isolate, usize) {
        use crate::bytecode::{SLOT_PLAIN, TYPE_DYNARRAY, TypeEntry, encode};
        let mut code = if flat {
            vec![
                word_bx(op::LOAD_INT, 0, values.len() as i32),
                word_bx(op::NEW_ARR, 0, 65),
            ]
        } else {
            int_array(values)
        };
        code.extend([
            encode(op::MOVE_REF, 10, 0, 0),
            word_bx(op::LOAD_INT, 20, 0),
            word_bx(op::LOAD_INT, 21, values.len() as i32),
            word_bx(op::LOAD_INT, 22, kind),
        ]);
        let before = code.len();
        code.extend([
            encode(op::INTRINSIC, 10, 20, u32::from(op::intr::ARRAY_SORT_RANGE)),
            encode(op::LOAD_ZERO, 20, 0, 0),
            encode(op::HALT, 20, 0, 0),
        ]);
        let types = vec![
            TypeEntry {
                kind: TYPE_DYNARRAY,
                refs: vec![SLOT_PLAIN],
                width: 1,
                name: "array of Int64".into(),
                ..TypeEntry::default()
            },
            flat_types().remove(1),
        ];
        let iso = funcs_isolate(
            types,
            vec![ConstEntry::Str("ab".into())],
            vec![TestFunc {
                params: 0,
                regs: 30,
                managed: &[0, 10],
                code,
            }],
        );
        (iso, before)
    }

    #[test]
    // `ArraySortRange` (`I96`) takes 1 + (comparisons / 16) of the budget, and runs to the end
    // even when that is more than what is left (bytecode.md 1.3, 2.22; design/vm.md 3.17).
    fn array_sort_range_takes_budget_by_comparisons() {
        // 64 sorted elements take 63 comparisons (7 in each run of 8, then 4, 2, 1 merges that
        // see the runs in order): 1 + 63 / 16 = 4. One element compares nothing: 1.
        for (idx, (values, left)) in [((0..64).collect::<Vec<i32>>(), 96), (vec![7], 99)]
            .into_iter()
            .enumerate()
        {
            let (mut iso, before) = sort_isolate(&values, 0, false);
            step_through(&mut iso, before);
            let mut budget = 100;
            assert!(matches!(iso.step(&mut budget), Step::Continue), "{idx}");
            assert_eq!(budget, left, "{idx}");
        }
        // Descending, with 2 left: sorted all the same, and the budget is used up
        let values: Vec<i32> = (0..64).rev().collect();
        let (mut iso, before) = sort_isolate(&values, 0, false);
        step_through(&mut iso, before);
        let mut budget = 2;
        assert!(matches!(iso.step(&mut budget), Step::Continue));
        assert_eq!(budget, 0);
        let sorted: Vec<i64> = (0..64).collect();
        assert_eq!(iso.heap.elems(iso.task.regs[10]).unwrap(), &sorted[..]);
    }

    #[test]
    // An `ArraySortRange` the standard library does not make stops the isolate (`Fatal`): a
    // comparison outside 0..5, an array whose elements take two slots, and elements that are not
    // strings compared as strings (bytecode.md 2.22).
    fn array_sort_range_refuses_bad_operands() {
        let cases: [(&[i32], i32, bool, &str); 5] = [
            (&[2, 1], 6, false, "unknown comparison 6"),
            (&[2, 1], -1, false, "unknown comparison -1"),
            (&[2, 1], 0, true, "one-slot elements"),
            // 9 is no heap block
            (&[9, 0], 4, false, "heap reference 9"),
            (&[0, 9], 5, false, "heap reference 9"),
        ];
        for (i, (values, kind, flat, what)) in cases.into_iter().enumerate() {
            let (mut iso, _) = sort_isolate(values, kind, flat);
            match iso.run(10_000) {
                RunOutcome::Fatal(e) => assert!(e.message.contains(what), "{i}: {}", e.message),
                other => panic!("{i}: {other:?}"),
            }
        }
    }

    /// Code that halts with `k + 1` for the first `k` whose register `expected[k].0` does not
    /// hold `expected[k].1`, and with 0 when all do (`r18` and `r19` are scratch).
    fn halt_unless(expected: &[(u32, i32)]) -> Vec<u32> {
        use crate::bytecode::encode;
        let mut code = Vec::new();
        for (k, (r, v)) in expected.iter().enumerate() {
            code.extend([
                word_bx(op::LOAD_INT, 19, *v),
                // Over the `LoadInt` and the `Halt`
                encode(op::JMP_IF_EQ_INT, *r, 19, 2),
                word_bx(op::LOAD_INT, 18, k as i32 + 1),
                encode(op::HALT, 18, 0, 0),
            ]);
        }
        code.extend([encode(op::LOAD_ZERO, 18, 0, 0), encode(op::HALT, 18, 0, 0)]);
        code
    }

    // ---- Flat places (`docs/spec/bytecode.md` 2.8; M4-63) ----

    /// A function of a hand-made module for [`run_funcs`]: its parameters, registers, managed
    /// registers (heap references) and code.
    struct TestFunc {
        params: u16,
        regs: u16,
        managed: &'static [u16],
        code: Vec<u32>,
    }

    /// Runs a verified module of `funcs` (the first is `<main>`, the others `Test.F1`, `Test.F2`,
    /// …) with the type section `types`, the constants `consts` and one global variable `G` (a
    /// heap reference) to the end of its finishing stage, with leak checking: the status of the
    /// program (its exit code or uncaught exception) with the blocks left at the end.
    fn run_funcs(
        types: Vec<crate::bytecode::TypeEntry>,
        consts: Vec<ConstEntry>,
        funcs: Vec<TestFunc>,
    ) -> ExitStatus {
        let mut iso = funcs_isolate(types, consts, funcs);
        let program = match iso.run(10_000) {
            RunOutcome::Finished(st) => st,
            other => panic!("{other:?}"),
        };
        iso.finish();
        ExitStatus {
            leaks: final_status(iso.run(1000)).leaks,
            ..program
        }
    }

    /// The isolate of [`run_funcs`], with `<main>` started.
    fn funcs_isolate(
        types: Vec<crate::bytecode::TypeEntry>,
        consts: Vec<ConstEntry>,
        funcs: Vec<TestFunc>,
    ) -> Isolate {
        use crate::bytecode::{
            Function, GLOBAL_REF, GlobalEntry, MANAGED_HEAP_REF, ManagedReg, UnitEntry,
        };

        let funcs = funcs
            .into_iter()
            .enumerate()
            .map(|(i, f)| Function {
                name: if i == 0 {
                    "Test.<main>".into()
                } else {
                    format!("Test.F{i}")
                },
                params: f.params,
                regs: f.regs,
                code: f.code,
                lines: vec![(0, 1)],
                locals: Vec::new(),
                managed: f
                    .managed
                    .iter()
                    .map(|r| ManagedReg {
                        reg: *r,
                        kind: MANAGED_HEAP_REF,
                    })
                    .collect(),
                file: 0,
                handlers: Vec::new(),
                outer: None,
                shared_by: Vec::new(),
                inlines: Vec::new(),
            })
            .collect();
        let module = Module {
            types,
            intf_calls: Vec::new(),
            consts,
            funcs,
            globals: vec![GlobalEntry {
                name: "G".into(),
                kind: GLOBAL_REF,
            }],
            entry: 0,
            units: vec![UnitEntry {
                name: "Test".into(),
                path: "t.pas".into(),
                init: None,
                fini: None,
                first_global: 0,
                global_count: 1,
            }],
            hosts: Vec::new(),
        };
        let module = Module::decode(&module.encode(crate::VERSION), crate::VERSION).unwrap();
        module.verify().expect("valid module");
        let config = IsolateConfig {
            hash_seed: HashSeed::Fixed(0),
            leak_check: true,
            ..Default::default()
        };
        let mut iso = Isolate::new(Arc::new(module), HostSet::empty(), config).unwrap();
        iso.start_main(vec!["t.pas".into()]).unwrap();
        iso
    }

    /// The types of the tests of flat places: 64 `Test.TPair` (an `Integer` and a `string`), 65
    /// a dynamic array of it laid out flat (2 slots an element).
    fn flat_types() -> Vec<crate::bytecode::TypeEntry> {
        use crate::bytecode::{SLOT_PLAIN, SLOT_REF, TYPE_DYNARRAY, TYPE_RECORD, TypeEntry};

        vec![
            TypeEntry {
                kind: TYPE_RECORD,
                slots: 2,
                refs: vec![SLOT_PLAIN, SLOT_REF],
                name: "Test.TPair".into(),
                ..TypeEntry::default()
            },
            TypeEntry {
                kind: TYPE_DYNARRAY,
                refs: vec![SLOT_PLAIN, SLOT_REF],
                width: 2,
                name: "array of Test.TPair".into(),
                ..TypeEntry::default()
            },
        ]
    }

    /// The constants of the tests of flat places: `'ab'` and `'z'`.
    fn flat_consts() -> Vec<ConstEntry> {
        vec![ConstEntry::Str("ab".into()), ConstEntry::Str("z".into())]
    }

    /// `op A Bx` (or `op A sBx`) with the 16-bit `bx`.
    fn word_bx(opc: u16, a: u32, bx: i32) -> u32 {
        let bx = bx as u32 & 0xFFFF;
        crate::bytecode::encode(opc, a, bx & 0xFF, bx >> 8)
    }

    /// The code of a `<main>` that makes `G` an array of two `Test.TPair`, sets `G[1]` to
    /// `(x, 'ab')`, lends `G[1]` as a flat place to `F1` and ends the loan (`r5` holds `'ab'`,
    /// `r1` 1 and `r6` the array, uncounted, afterwards).
    fn lend_g1_to_f1(x: i32) -> Vec<u32> {
        use crate::bytecode::encode;

        vec![
            word_bx(op::LOAD_INT, 0, 2),
            word_bx(op::NEW_ARR, 0, 65),
            word_bx(op::STORE_GLOBAL_REF, 0, 0),
            encode(op::RELEASE, 0, 0, 0),
            word_bx(op::LOAD_GLOBAL, 6, 0),
            word_bx(op::LOAD_INT, 1, 1),
            encode(op::ELEM_AT, 2, 6, 1),
            word_bx(op::LOAD_INT, 14, x),
            encode(op::SLOT_SET, 2, 0, 14),
            word_bx(op::LOAD_CONST, 5, 0),
            encode(op::SLOT_SET_REF, 2, 1, 5),
            // G[1] as a flat place, passed to F1
            word_bx(op::MAKE_GLOBAL_REF, 7, 0),
            word_bx(op::LOAD_INT, 9, 1),
            encode(op::LEND_PLACE, 8, 7, 1),
            64,
            encode(op::MOVE, 12, 8, 0),
            word_bx(op::CALL, 12, 1),
            encode(op::UNLEND, 0, 0, 0),
            word_bx(op::LOAD_GLOBAL, 6, 0),
        ]
    }

    /// The end of a `<main>` after [`lend_g1_to_f1`]: exits with `G[1].X`, plus 100 when
    /// `G[1].S` is the constant `k` (`''` for `None`).
    fn exit_with_g1(k: Option<i32>) -> Vec<u32> {
        use crate::bytecode::encode;

        let mut code = vec![
            encode(op::ELEM_AT, 2, 6, 1),
            encode(op::SLOT_GET, 9, 2, 0),
            encode(op::SLOT_GET_REF, 4, 2, 1),
        ];
        match k {
            Some(k) => code.push(word_bx(op::LOAD_CONST, 10, k)),
            None => code.push(encode(op::LOAD_ZERO, 10, 0, 0)),
        }
        code.extend([
            encode(op::EQ_STR, 11, 4, 10),
            encode(op::MUL_INT_I, 11, 11, 100),
            encode(op::ADD_INT, 9, 9, 11),
            encode(op::HALT, 9, 0, 0),
        ]);
        code
    }

    /// Runs `<main>` = [`lend_g1_to_f1`] (`G[1] = (x, 'ab')`) and [`exit_with_g1`] (with the
    /// constant `k` for `S`) with `f1` as `F1` (a `var` parameter of `Test.TPair`) and the other
    /// functions `more`.
    fn run_lent_g1(x: i32, k: Option<i32>, f1: TestFunc, more: Vec<TestFunc>) -> ExitStatus {
        let mut main = lend_g1_to_f1(x);
        main.extend(exit_with_g1(k));
        let mut funcs = vec![
            TestFunc {
                params: 0,
                regs: 16,
                managed: &[4, 5, 10],
                code: main,
            },
            f1,
        ];
        funcs.extend(more);
        run_funcs(flat_types(), flat_consts(), funcs)
    }

    #[test]
    // `ElemAt` makes a cursor to an element of an array whose elements take two slots, and
    // `SlotGet` / `SlotSet` (and their counted forms) read and write the slots of that element;
    // `ArrLen` counts elements, not slots (`docs/spec/bytecode.md` 2.8, 2.9).
    fn cursors_read_and_write_the_slots_of_flat_elements() {
        use crate::bytecode::encode;

        let code = vec![
            word_bx(op::LOAD_INT, 0, 3),
            word_bx(op::NEW_ARR, 0, 65),
            word_bx(op::LOAD_INT, 1, 1),
            encode(op::ELEM_AT, 2, 0, 1),
            word_bx(op::LOAD_INT, 4, 7),
            encode(op::SLOT_SET, 2, 0, 4),
            word_bx(op::LOAD_CONST, 5, 0),
            encode(op::SLOT_SET_REF, 2, 1, 5),
            encode(op::SLOT_GET, 6, 2, 0),
            encode(op::ARR_LEN, 7, 0, 0),
            encode(op::SLOT_GET_REF, 8, 2, 1),
            encode(op::EQ_STR, 9, 8, 5),
            // 10 X + Length + 100 (S = 'ab')
            encode(op::MUL_INT_I, 6, 6, 10),
            encode(op::ADD_INT, 6, 6, 7),
            encode(op::MUL_INT_I, 9, 9, 100),
            encode(op::ADD_INT, 6, 6, 9),
            encode(op::HALT, 6, 0, 0),
        ];
        let st = run_funcs(
            flat_types(),
            flat_consts(),
            vec![TestFunc {
                params: 0,
                regs: 10,
                managed: &[0, 5, 8],
                code,
            }],
        );
        assert_eq!(st.kind, ExitKind::Code(173));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    #[test]
    // `ElemAt` checks the index against the number of elements (3 elements are 6 slots), with
    // the message of an index out of a dynamic array, and 0 is an empty array.
    fn elem_at_checks_the_number_of_elements() {
        use crate::bytecode::encode;

        for (idx, (len, index, message)) in [
            (3, 3, "array index 3 is out of range 0..2"),
            (3, -1, "array index -1 is out of range 0..2"),
            (0, 0, "array index 0 is out of range 0..-1"),
        ]
        .into_iter()
        .enumerate()
        {
            let code = vec![
                word_bx(op::LOAD_INT, 0, len),
                word_bx(op::NEW_ARR, 0, 65),
                word_bx(op::LOAD_INT, 1, index),
                encode(op::ELEM_AT, 2, 0, 1),
                encode(op::RET_VOID, 0, 0, 0),
            ];
            let st = run_funcs(
                flat_types(),
                Vec::new(),
                vec![TestFunc {
                    params: 0,
                    regs: 4,
                    managed: &[0],
                    code,
                }],
            );
            match st.kind {
                ExitKind::UncaughtException(e) => {
                    assert_eq!(
                        (e.class.as_str(), e.message.as_str()),
                        ("ERangeError", message),
                        "{idx}"
                    );
                }
                other => panic!("{idx}: {other:?}"),
            }
            assert_eq!(st.leaks, Vec::<(String, u64)>::new(), "{idx}");
        }
    }

    #[test]
    // `AggLoad` reads a flat element into a record value: 0 when every slot is 0 (letting go of
    // the old value), the block the register holds alone when it is of the type (no new block),
    // a new block otherwise; `AggStore` writes a record value into a flat element, counting its
    // string (`docs/design/records.md` 3.5).
    fn agg_load_and_store_copy_flat_values() {
        use crate::bytecode::encode;

        let code = vec![
            word_bx(op::LOAD_INT, 0, 2),
            word_bx(op::NEW_ARR, 0, 65),
            word_bx(op::UNIQUE, 10, 64),
            word_bx(op::LOAD_INT, 1, 0),
            encode(op::ELEM_AT, 2, 0, 1),
            // A[0] is default: r10 := 0
            encode(op::AGG_LOAD, 10, 2, 0),
            64,
            encode(op::LOAD_ZERO, 17, 0, 0),
            encode(op::EQ_INT, 16, 10, 17),
            word_bx(op::LOAD_INT, 4, 5),
            encode(op::SLOT_SET, 2, 0, 4),
            word_bx(op::LOAD_CONST, 5, 0),
            encode(op::SLOT_SET_REF, 2, 1, 5),
            // r10 := A[0], a new block; A[1] := r10
            encode(op::AGG_LOAD, 10, 2, 0),
            64,
            word_bx(op::LOAD_INT, 1, 1),
            encode(op::ELEM_AT, 6, 0, 1),
            encode(op::AGG_STORE, 6, 10, 0),
            64,
            // r10 := A[1] into the same block
            encode(op::MOVE, 11, 10, 0),
            encode(op::AGG_LOAD, 10, 6, 0),
            64,
            encode(op::EQ_INT, 12, 10, 11),
            encode(op::SLOT_GET, 13, 6, 0),
            encode(op::SLOT_GET_REF, 14, 6, 1),
            encode(op::EQ_STR, 15, 14, 5),
            encode(op::FIELD_GET, 9, 10, 0),
            // A[1].X + r10.X + 100 (same block) + 50 (A[1].S = 'ab') + 20 (0 at first)
            encode(op::ADD_INT, 13, 13, 9),
            encode(op::MUL_INT_I, 12, 12, 100),
            encode(op::ADD_INT, 13, 13, 12),
            encode(op::MUL_INT_I, 15, 15, 50),
            encode(op::ADD_INT, 13, 13, 15),
            encode(op::MUL_INT_I, 16, 16, 20),
            encode(op::ADD_INT, 13, 13, 16),
            encode(op::HALT, 13, 0, 0),
        ];
        let st = run_funcs(
            flat_types(),
            flat_consts(),
            vec![TestFunc {
                params: 0,
                regs: 18,
                managed: &[0, 5, 10, 14],
                code,
            }],
        );
        assert_eq!(st.kind, ExitKind::Code(180));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    #[test]
    // The array built-ins count elements of two slots: `Copy`, `+`, `Insert` (of a record
    // value, whose slots are copied), `Delete`, `ArrayMoveRange` and `SetLength` (growing and
    // shrinking in place, letting go of the strings of the dropped elements).
    fn array_built_ins_move_flat_elements() {
        use crate::bytecode::encode;

        // rX := A[i].X for the array in `arr`
        let x_of = |x: u32, arr: u32, i: i32| {
            vec![
                word_bx(op::LOAD_INT, 21, i),
                encode(op::ELEM_AT, 22, arr, 21),
                encode(op::SLOT_GET, x, 22, 0),
            ]
        };
        let mut code = vec![word_bx(op::LOAD_INT, 0, 3), word_bx(op::NEW_ARR, 0, 65)];
        // A := [1, 2, 3]
        for i in 0..3 {
            code.extend([
                word_bx(op::LOAD_INT, 1, i),
                encode(op::ELEM_AT, 2, 0, 1),
                word_bx(op::LOAD_INT, 4, i + 1),
                encode(op::SLOT_SET, 2, 0, 4),
            ]);
        }
        code.extend([
            // B := Copy(A, 1, 2) = [2, 3]
            encode(op::MOVE, 13, 0, 0),
            word_bx(op::LOAD_INT, 14, 1),
            word_bx(op::LOAD_INT, 15, 2),
            word_bx(op::ARR_COPY, 13, 65),
            // C := A + B = [1, 2, 3, 2, 3]
            encode(op::MOVE, 16, 0, 0),
            encode(op::MOVE, 17, 13, 0),
            word_bx(op::ARR_CONCAT, 16, 65),
            // Insert((9, 'ab'), A, 0): A = [9, 1, 2, 3]
            encode(op::LOAD_ZERO, 1, 0, 0),
            word_bx(op::UNIQUE, 1, 64),
            word_bx(op::LOAD_INT, 4, 9),
            encode(op::FIELD_SET, 1, 0, 4),
            word_bx(op::LOAD_CONST, 5, 0),
            encode(op::FIELD_SET_REF, 1, 1, 5),
            word_bx(op::LOAD_INT, 2, 0),
            word_bx(op::ARR_INSERT, 0, 65),
            encode(op::RELEASE, 1, 0, 0),
            // Delete(A, 1, 1): A = [9, 2, 3]
            word_bx(op::LOAD_INT, 1, 1),
            word_bx(op::LOAD_INT, 2, 1),
            word_bx(op::ARR_DELETE, 0, 65),
            // ArrayMoveRange(A, 0, 2, 1): A = [2, 3, 9]
            word_bx(op::LOAD_INT, 1, 0),
            word_bx(op::LOAD_INT, 2, 2),
            word_bx(op::LOAD_INT, 3, 1),
            encode(op::INTRINSIC, 0, 1, u32::from(op::intr::ARRAY_MOVE_RANGE)),
            // SetLength(A, 4), then SetLength(A, 2): A = [2, 3], dropping the 'ab' of 9
            word_bx(op::LOAD_INT, 1, 4),
            word_bx(op::SET_LEN, 0, 65),
            word_bx(op::LOAD_INT, 1, 2),
            word_bx(op::SET_LEN, 0, 65),
        ]);
        code.extend(x_of(6, 0, 0));
        code.extend(x_of(7, 0, 1));
        code.extend(x_of(8, 13, 0));
        code.extend(x_of(9, 16, 3));
        code.extend([
            encode(op::ARR_LEN, 10, 0, 0),
            encode(op::ARR_LEN, 11, 16, 0),
            // 10 A[0] + A[1] + 20 B[0] + 30 C[3] + 20 Length(C) + Length(A)
            encode(op::MUL_INT_I, 6, 6, 10),
            encode(op::ADD_INT, 6, 6, 7),
            encode(op::MUL_INT_I, 8, 8, 20),
            encode(op::ADD_INT, 6, 6, 8),
            encode(op::MUL_INT_I, 9, 9, 30),
            encode(op::ADD_INT, 6, 6, 9),
            encode(op::MUL_INT_I, 11, 11, 20),
            encode(op::ADD_INT, 6, 6, 11),
            encode(op::ADD_INT, 6, 6, 10),
            encode(op::HALT, 6, 0, 0),
        ]);
        let st = run_funcs(
            flat_types(),
            flat_consts(),
            vec![TestFunc {
                params: 0,
                regs: 24,
                managed: &[0, 5, 13, 16],
                code,
            }],
        );
        assert_eq!(st.kind, ExitKind::Code(225));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    #[test]
    // A cursor's container may be an instance's handle (a flat field of an instance): its slots
    // are read and written in place, and a destroyed instance is refused with
    // `EAccessViolation` (`docs/spec/bytecode.md` 2.8).
    fn cursors_into_an_instance() {
        use crate::bytecode::{
            SLOT_PLAIN, SLOT_REF, TOBJECT_TYPE, TOBJECT_VMT, TYPE_CLASS, TypeEntry, encode,
        };

        let mut types = flat_types();
        types.push(TypeEntry {
            kind: TYPE_CLASS,
            slots: 2,
            refs: vec![SLOT_PLAIN, SLOT_REF],
            name: "Test.TObj".into(),
            parent: Some(TOBJECT_TYPE),
            vmt: TOBJECT_VMT.to_vec(),
            ..TypeEntry::default()
        });
        let common = vec![
            word_bx(op::NEW, 0, 66),
            word_bx(op::LOAD_INT, 1, 0),
            encode(op::MOVE, 2, 0, 0),
            word_bx(op::LOAD_INT, 4, 7),
            encode(op::SLOT_SET, 1, 0, 4),
            word_bx(op::LOAD_CONST, 5, 0),
            encode(op::SLOT_SET_REF, 1, 1, 5),
            encode(op::AGG_LOAD, 6, 1, 0),
            64,
            encode(op::FIELD_GET, 7, 6, 0),
            encode(op::FIELD_GET_REF, 8, 6, 1),
            encode(op::EQ_STR, 9, 8, 5),
            encode(op::MUL_INT_I, 9, 9, 100),
            encode(op::ADD_INT, 7, 7, 9),
            encode(op::INTRINSIC, 0, 0, u32::from(op::intr::FREE)),
        ];
        let mut halt = common.clone();
        halt.push(encode(op::HALT, 7, 0, 0));
        let mut freed = common;
        freed.extend([encode(op::SLOT_GET, 9, 1, 0), encode(op::RET_VOID, 0, 0, 0)]);
        let run = |code| {
            run_funcs(
                types.clone(),
                flat_consts(),
                vec![TestFunc {
                    params: 0,
                    regs: 10,
                    managed: &[5, 6, 8],
                    code,
                }],
            )
        };
        let st = run(halt);
        assert_eq!(st.kind, ExitKind::Code(107));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
        let st = run(freed);
        match st.kind {
            ExitKind::UncaughtException(e) => assert_eq!(
                (e.class.as_str(), e.message.as_str()),
                ("EAccessViolation", FREED_ACCESS)
            ),
            other => panic!("{other:?}"),
        }
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    /// `F1(var It: Test.TPair)`: `It.X := It.X + 1; It.S := 'z'`.
    fn bump_f1() -> TestFunc {
        use crate::bytecode::encode;

        TestFunc {
            params: 1,
            regs: 3,
            managed: &[2],
            code: vec![
                encode(op::REF_FIELD_GET, 1, 0, 0),
                encode(op::ADD_INT_I, 1, 1, 1),
                encode(op::REF_FIELD_SET, 0, 0, 1),
                64,
                word_bx(op::LOAD_CONST, 2, 1),
                encode(op::REF_FIELD_SET_REF, 0, 1, 2),
                64,
                encode(op::RET_VOID, 0, 0, 0),
            ],
        }
    }

    #[test]
    // `LendPlace` lends a flat element to a `var` parameter as a reference slot of kind 4: the
    // callee's `RefFieldGet` / `RefFieldSet` read and write the element's slots in place, and
    // `Unlend` finds them where they were (`docs/design/records.md` 3.7 B, 3.8).
    fn lend_place_lets_the_callee_write_the_element() {
        let st = run_lent_g1(41, Some(1), bump_f1(), Vec::new());
        assert_eq!(st.kind, ExitKind::Code(142));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    #[test]
    // When the array holding a lent flat place goes (replaced by another while the callee runs),
    // the place's values move to a block of their own, the callee goes on writing there, and
    // `Unlend` puts them in the element the path leads to now (`docs/design/records.md` 3.7 B).
    fn lent_place_moves_out_when_its_array_goes() {
        use crate::bytecode::encode;

        let f1 = TestFunc {
            params: 1,
            regs: 4,
            managed: &[1],
            code: vec![
                word_bx(op::LOAD_INT, 2, 5),
                encode(op::REF_FIELD_SET, 0, 0, 2),
                64,
                word_bx(op::LOAD_INT, 1, 2),
                word_bx(op::NEW_ARR, 1, 65),
                word_bx(op::STORE_GLOBAL_REF, 1, 0),
                encode(op::REF_FIELD_GET, 2, 0, 0),
                encode(op::ADD_INT_I, 2, 2, 1),
                encode(op::REF_FIELD_SET, 0, 0, 2),
                64,
                encode(op::RET_VOID, 0, 0, 0),
            ],
        };
        let st = run_lent_g1(41, Some(0), f1, Vec::new());
        assert_eq!(st.kind, ExitKind::Code(106));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    #[test]
    // When the element holding a lent flat place is dropped (`SetLength` of the array, which
    // holds its only reference, to fewer elements or none), its values move out and `Unlend`,
    // not finding the element again, lets go of them and raises `ERangeError`.
    fn lent_place_of_a_dropped_element_is_let_go() {
        use crate::bytecode::encode;

        for (len, message) in [
            (1, "array index 1 is out of range 0..0"),
            (0, "array index 1 is out of range 0..-1"),
        ] {
            let f1 = TestFunc {
                params: 1,
                regs: 4,
                managed: &[],
                code: vec![
                    word_bx(op::LOAD_INT, 2, 5),
                    encode(op::REF_FIELD_SET, 0, 0, 2),
                    64,
                    word_bx(op::LOAD_GLOBAL, 1, 0),
                    word_bx(op::LOAD_INT, 2, len),
                    word_bx(op::SET_LEN, 1, 65),
                    word_bx(op::STORE_GLOBAL, 1, 0),
                    encode(op::RET_VOID, 0, 0, 0),
                ],
            };
            let st = run_lent_g1(41, Some(0), f1, Vec::new());
            match st.kind {
                ExitKind::UncaughtException(e) => {
                    assert_eq!(
                        (e.class.as_str(), e.message.as_str()),
                        ("ERangeError", message),
                        "{len}"
                    );
                }
                other => panic!("{len}: {other:?}"),
            }
            assert_eq!(st.leaks, Vec::<(String, u64)>::new(), "{len}");
        }
    }

    #[test]
    // While a flat place is lent, reading one of its slots through another path is refused, and
    // so is copying the place (`EAliasError`; `docs/spec/types.md` chapter 3).
    fn lent_place_refuses_other_paths() {
        use crate::bytecode::encode;

        for (idx, (last, ty_word, message)) in [
            (
                encode(op::SLOT_GET, 1, 3, 0),
                None,
                "array element 1 is passed to a var parameter of a call in progress",
            ),
            (
                encode(op::AGG_LOAD, 1, 3, 0),
                Some(64),
                "a value containing a location passed to a var parameter of a call in progress \
                 cannot be copied",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let mut code = vec![
                word_bx(op::LOAD_GLOBAL, 1, 0),
                word_bx(op::LOAD_INT, 2, 1),
                encode(op::ELEM_AT, 3, 1, 2),
                encode(op::LOAD_ZERO, 1, 0, 0),
                last,
            ];
            code.extend(ty_word);
            code.push(encode(op::RET_VOID, 0, 0, 0));
            let f1 = TestFunc {
                params: 1,
                regs: 5,
                managed: &[1],
                code,
            };
            let st = run_lent_g1(41, Some(0), f1, Vec::new());
            match st.kind {
                ExitKind::UncaughtException(e) => {
                    assert_eq!(
                        (e.class.as_str(), e.message.as_str()),
                        ("EAliasError", message),
                        "{idx}"
                    );
                }
                other => panic!("{idx}: {other:?}"),
            }
            assert_eq!(st.leaks, Vec::<(String, u64)>::new(), "{idx}");
        }
    }

    #[test]
    // A flat place lent through a reference to another one (a `LendPlace` whose root is of kind 4)
    // keeps its values when their array goes during the inner call: the inner place moves out
    // before the outer one, so the inner callee reads the values and its writes reach the
    // element the outer path leads to afterwards.
    fn place_lent_through_a_place_moves_out_first() {
        use crate::bytecode::encode;

        // F1(var It: TPair): F2(It)
        let f1 = TestFunc {
            params: 1,
            regs: 8,
            managed: &[],
            code: vec![
                word_bx(op::LOAD_INT, 3, 0),
                encode(op::LEND_PLACE, 2, 0, 1),
                64,
                encode(op::MOVE, 6, 2, 0),
                word_bx(op::CALL, 6, 2),
                encode(op::UNLEND, 0, 0, 0),
                encode(op::RET_VOID, 0, 0, 0),
            ],
        };
        // F2(var R: TPair): G := another array; R.X := R.X + 1
        let f2 = TestFunc {
            params: 1,
            regs: 4,
            managed: &[1],
            code: vec![
                word_bx(op::LOAD_INT, 1, 2),
                word_bx(op::NEW_ARR, 1, 65),
                word_bx(op::STORE_GLOBAL_REF, 1, 0),
                encode(op::REF_FIELD_GET, 2, 0, 0),
                encode(op::ADD_INT_I, 2, 2, 1),
                encode(op::REF_FIELD_SET, 0, 0, 2),
                64,
                encode(op::RET_VOID, 0, 0, 0),
            ],
        };
        let st = run_lent_g1(41, Some(0), f1, vec![f2]);
        assert_eq!(st.kind, ExitKind::Code(142));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    #[test]
    // A `Lend` whose root is a reference to a flat place (kind 4) takes a slot of the place out
    // (its path starts with the slot number in the place) and gives it back there.
    fn lend_through_a_place_reference() {
        use crate::bytecode::encode;

        let f1 = TestFunc {
            params: 1,
            regs: 8,
            managed: &[],
            code: vec![
                word_bx(op::LOAD_INT, 3, 0),
                encode(op::LEND, 2, 0, 1),
                encode(op::MAKE_REF, 6, 2, 0),
                word_bx(op::CALL, 6, 2),
                encode(op::UNLEND, 0, 0, 0),
                encode(op::RET_VOID, 0, 0, 0),
            ],
        };
        // F2(var N: Integer): N := N + 10
        let f2 = TestFunc {
            params: 1,
            regs: 2,
            managed: &[],
            code: vec![
                encode(op::LOAD_REF, 1, 0, 0),
                encode(op::ADD_INT_I, 1, 1, 10),
                encode(op::STORE_REF, 0, 1, 0),
                encode(op::RET_VOID, 0, 0, 0),
            ],
        };
        let st = run_lent_g1(1, Some(0), f1, vec![f2]);
        assert_eq!(st.kind, ExitKind::Code(111));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }

    #[test]
    // Through a reference to a flat place, `LoadRefRef` reads the whole place into a record value,
    // `StoreRefRef` writes a record value into it and `ClearRef` makes it default (counting the
    // strings in and out).
    fn whole_place_through_a_reference() {
        use crate::bytecode::encode;

        // It2 := It; It := (9, ''); It.X := It.X + It2.X
        let store = TestFunc {
            params: 1,
            regs: 8,
            managed: &[1, 3],
            code: vec![
                encode(op::LOAD_REF_REF, 1, 0, 0),
                encode(op::FIELD_GET, 2, 1, 0),
                word_bx(op::UNIQUE, 3, 64),
                word_bx(op::LOAD_INT, 4, 9),
                encode(op::FIELD_SET, 3, 0, 4),
                encode(op::STORE_REF_REF, 0, 3, 0),
                encode(op::REF_FIELD_GET, 5, 0, 0),
                encode(op::ADD_INT, 5, 5, 2),
                encode(op::REF_FIELD_SET, 0, 0, 5),
                64,
                encode(op::RET_VOID, 0, 0, 0),
            ],
        };
        let st = run_lent_g1(41, None, store, Vec::new());
        assert_eq!(st.kind, ExitKind::Code(150));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
        let clear = TestFunc {
            params: 1,
            regs: 2,
            managed: &[],
            code: vec![
                encode(op::CLEAR_REF, 0, 0, 0),
                encode(op::RET_VOID, 0, 0, 0),
            ],
        };
        let st = run_lent_g1(41, None, clear, Vec::new());
        assert_eq!(st.kind, ExitKind::Code(100));
        assert_eq!(st.leaks, Vec::<(String, u64)>::new());
    }
}

/// The number of instructions the VM ran on this thread, for the tests of the complexity
/// guarantees of the hash tables (`docs/spec/collections.md` 11.3): unlike the time taken, it does
/// not change from run to run.
#[cfg(test)]
pub(crate) mod executed {
    use std::cell::Cell;

    thread_local! {
        static TOTAL: Cell<u64> = const { Cell::new(0) };
    }

    pub(super) fn add_one() {
        TOTAL.with(|t| t.set(t.get() + 1));
    }

    /// Adds the work done inside one instruction: the entries the hash table instructions
    /// followed or linked (`docs/spec/collections.md` 11.3).
    pub(super) fn add(n: u64) {
        TOTAL.with(|t| t.set(t.get() + n));
    }

    /// The total so far.
    pub fn total() -> u64 {
        TOTAL.with(Cell::get)
    }
}

/// The declared name of a class from its full name (`Hello.TFoo.TItem` → `TItem`;
/// `docs/spec/objects.md` chapter 5). The name is cut at the last `.` outside the type arguments,
/// so an instantiation keeps its type arguments as written in the full name
/// (`Hello.TBox<Hello.TFoo>` → `TBox<Hello.TFoo>`; `docs/spec/generics.md` 4.1).
fn class_decl_name(full: &str) -> &str {
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in full.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            '.' if depth == 0 => start = i + 1,
            _ => {}
        }
    }
    &full[start..]
}

/// Releases the value of a register holding a managed value of the kind `m` says
/// (`docs/spec/bytecode.md` 3.2: by its contents for [`bytecode::MANAGED_OBJ`]) and sets it to 0.
fn release_slot(heap: &mut Heap, slot: &mut Slot, m: &bytecode::ManagedReg) -> Result<(), BadRef> {
    let old = std::mem::take(slot);
    if m.kind == bytecode::MANAGED_OBJ {
        heap.release_obj(old)
    } else {
        heap.release(old)
    }
}

/// The function `CallMethod` / `CallVirtual` calls when `Self` checks and the table entry is a
/// function (the fast path; `None` leaves the instruction to `step`).
fn fast_method_callee(
    heap: &heap::Heap,
    module: &Module,
    opc: u8,
    h: i64,
    bx: usize,
) -> Option<usize> {
    let ty = heap.obj_class_fast(h)?;
    if opc == op::word::CALL_METHOD {
        return Some(bx);
    }
    let entry = heap.type_entry(ty)?;
    let target = if opc == op::word::CALL_INTF {
        // `docs/spec/objects.md` 14.7: the interface's entry, found by its type index
        let (intf, k) = *module.intf_calls.get(bx)?;
        *entry.intf_entry(intf)?.funcs.get(k as usize)?
    } else {
        *entry.vmt.get(bx)?
    };
    (target < bytecode::VMT_BUILTIN).then_some(target as usize)
}

/// The function a `CallIndirect` / `CallClosure` of the value `v` enters on the fast path, and
/// whether it is a closure (whose block becomes the hidden first argument;
/// `docs/spec/objects.md` 14.9). `None` leaves the call to step: `nil`, a value outside the
/// function table, a callable value that needs a `Self` (a method or class method), or a function
/// not in the table.
#[inline(always)]
fn fast_value_callee(heap: &heap::Heap, module: &Module, opc: u8, v: i64) -> Option<(usize, bool)> {
    let (f, closure) = if opc == op::word::CALL_INDIRECT {
        (u32::try_from(v.checked_sub(1)?).ok()?, false)
    } else {
        heap.plain_callee(v)?
    };
    let f = f as usize;
    (f < module.funcs.len()).then_some((f, closure))
}
