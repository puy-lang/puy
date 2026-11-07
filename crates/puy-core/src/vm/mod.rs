//! Stackless VM (`docs/design/vm.md`, `docs/design/host-interface.md` 3.5, `docs/spec/bytecode.md` chapter 2).
//!
//! - Pascal calls do not recurse on the Rust stack. Frames and registers live on the heap.
//! - Execution budget (decremented by 1 on every backward jump and `Call`).
//! - Rust panics are not converted into Pascal exceptions (§5.10).

pub mod heap;
pub mod report;
mod strconv;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::bytecode::{self, ConstEntry, Module, op};
use crate::sema::types::MAX_STRING_LEN;
use heap::{Heap, PStr};
use puy_host::{HostError, HostErrorKind, HostFuture, HostSet, OutStream};
pub use report::{ExceptionReport, StackFrame};

/// A register / global variable slot (8 bytes).
pub type Slot = i64;

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
}

impl Default for IsolateConfig {
    fn default() -> Self {
        IsolateConfig {
            memory_limit: None,
            stack_limit: 256 * 1024 * 1024,
            hash_seed: HashSeed::FromHost,
            budget_default: 100_000,
            stdout_buffering: StdoutBuffering::Full,
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
    UncaughtException(ExceptionReport),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitStatus {
    pub kind: ExitKind,
    /// Failure to flush the standard output buffer before exiting (`semantics.md` 7.2).
    pub stdout_error: Option<HostError>,
}

impl ExitStatus {
    /// Process exit code (§9.3). A flush failure turns 0 into 1.
    pub fn exit_code(&self) -> u8 {
        let code = match &self.kind {
            ExitKind::Code(c) => *c,
            ExitKind::UncaughtException(_) => 1,
        };
        if code == 0 && self.stdout_error.is_some() {
            1
        } else {
            code
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
    ConsoleReadLine,
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

/// A runtime error (the M1 exception; `semantics.md` 7.1).
struct RtError {
    class: &'static str,
    message: String,
}

fn rt(class: &'static str, message: impl Into<String>) -> RtError {
    RtError {
        class,
        message: message.into(),
    }
}

/// A pending call to a suspendable host function.
struct PendingHost {
    kind: PendingKind,
    /// Register to write the result to (absolute position).
    dst: u32,
    /// The `pc` to advance to when done (the instruction after `CallHost`). On failure it is not advanced, so the report points at `CallHost`.
    resume_pc: u32,
}

enum PendingKind {
    Write(HostFuture<()>),
    ReadLine(HostFuture<Option<String>>),
}

enum Phase {
    NotStarted,
    Init,
    Main,
    Done,
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
    host_table: Vec<HostFn>,
    stdout_buf: Vec<u8>,
    pending: Option<PendingHost>,
    phase: Phase,
    finished: Option<ExitStatus>,
    /// `ReadLine` returned `None` (`Eof` in M2).
    pub eof_seen: bool,
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
                "Console.ReadLine" => HostFn::ConsoleReadLine,
                "Env.Get" => HostFn::EnvGet,
                _ => return Err(IsolateError::HostFunction(h.name.clone())),
            };
            host_table.push(kind);
        }
        let globals = vec![0; module.globals.len()];
        let const_cache = vec![None; module.consts.len()];
        Ok(Isolate {
            heap: Heap::new(config.memory_limit),
            globals,
            regs: Vec::with_capacity(256),
            frames: Vec::with_capacity(16),
            args: Vec::new(),
            const_cache,
            host_table,
            stdout_buf: Vec::new(),
            pending: None,
            phase: Phase::NotStarted,
            finished: None,
            eof_seen: false,
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
        let unit = &self.module.units[0];
        match unit.init {
            Some(init) => {
                self.phase = Phase::Init;
                self.push_frame(init, 0, 0);
            }
            None => {
                self.phase = Phase::Main;
                let entry = self.module.entry;
                self.push_frame(entry, 0, 0);
            }
        }
        Ok(())
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
        self.frames.push(Frame {
            func,
            base,
            pc: 0,
            ret_reg,
        });
    }

    /// Runs runnable tasks within the budget.
    pub fn run(&mut self, budget: u32) -> RunOutcome {
        if let Some(st) = &self.finished {
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
                Err(e) => return self.raise(e),
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
                Step::Finished(kind) => return self.finish(kind),
                Step::Error(e) => return self.raise(e),
                Step::Fatal(m) => {
                    self.flush_stdout().ok();
                    return RunOutcome::Fatal(VmError { message: m });
                }
            }
        }
    }

    fn finish(&mut self, kind: ExitKind) -> RunOutcome {
        let stdout_error = self.flush_stdout().err();
        self.phase = Phase::Done;
        let st = ExitStatus { kind, stdout_error };
        self.finished = Some(st.clone());
        RunOutcome::Finished(st)
    }

    /// Runtime error: build the report and finish.
    fn raise(&mut self, e: RtError) -> RunOutcome {
        let report = self.build_report(e);
        self.finish(ExitKind::UncaughtException(report))
    }

    fn build_report(&self, e: RtError) -> ExceptionReport {
        let n = self.frames.len();
        let mut all: Vec<StackFrame> = Vec::new();
        for (i, fr) in self.frames.iter().enumerate().rev() {
            let f = &self.module.funcs[fr.func as usize];
            // The innermost is the position of the raising instruction; callers are the position of the Call (one word before the return address)
            let pos = if i == n - 1 {
                fr.pc
            } else {
                fr.pc.saturating_sub(1)
            };
            let line = f.line_at(pos);
            let file = self
                .module
                .units
                .get(f.file as usize)
                .map(|u| u.path.clone())
                .unwrap_or_default();
            all.push(StackFrame {
                function: f.name.clone(),
                file,
                line,
            });
        }
        let (stack, omitted) = if all.len() > 32 {
            let omitted = all.len() - 32;
            let mut s: Vec<StackFrame> = all[..16].to_vec();
            s.extend_from_slice(&all[all.len() - 16..]);
            (s, omitted)
        } else {
            (all, 0)
        };
        ExceptionReport {
            class: e.class.to_string(),
            message: e.message,
            stack,
            omitted,
        }
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

    fn poll_pending(&mut self) -> Result<bool, RtError> {
        let Some(p) = self.pending.as_mut() else {
            return Ok(true);
        };
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let result: Result<Option<Option<String>>, HostError> = match &mut p.kind {
            PendingKind::Write(f) => match Pin::new(f).poll(&mut cx) {
                Poll::Pending => return Ok(false),
                Poll::Ready(Ok(())) => Ok(None),
                Poll::Ready(Err(e)) => Err(e),
            },
            PendingKind::ReadLine(f) => match Pin::new(f).poll(&mut cx) {
                Poll::Pending => return Ok(false),
                Poll::Ready(Ok(line)) => Ok(Some(line)),
                Poll::Ready(Err(e)) => Err(e),
            },
        };
        let dst = p.dst;
        let resume_pc = p.resume_pc;
        self.pending = None;
        if result.is_ok()
            && let Some(fr) = self.frames.last_mut()
        {
            fr.pc = resume_pc;
        }
        match result {
            Ok(None) => Ok(true),
            Ok(Some(line)) => {
                let s = match line {
                    Some(l) => l,
                    None => {
                        self.eof_seen = true;
                        String::new()
                    }
                };
                let r = self.alloc_string(PStr::from_text(&s))?;
                self.regs[dst as usize] = r;
                Ok(true)
            }
            Err(e) => Err(host_error_to_rt(e)),
        }
    }

    fn alloc_string(&mut self, s: PStr) -> Result<Slot, RtError> {
        self.heap
            .alloc_str(s)
            .map_err(|_| rt("EOutOfMemory", "out of memory"))
    }

    fn load_const(&mut self, k: usize) -> Result<Slot, RtError> {
        if let Some(v) = self.const_cache[k] {
            return Ok(v);
        }
        let v = match &self.module.consts[k] {
            ConstEntry::Int(i) => *i,
            ConstEntry::Str(s) => {
                let p = PStr::from_text(s);
                self.alloc_string(p)?
            }
            ConstEntry::Utf16(u) => {
                let p = PStr::from_units(u);
                self.alloc_string(p)?
            }
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
                        Some(v) => regs[ra] = v,
                        None => slow!(),
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
                        if self.frames.len() <= 1 {
                            // A return from the first frame (<init> → <main>, exit) is left to step
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
                "EIntOverflow",
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
                    Err(e) => return Step::Error(e),
                };
                self.regs[ra] = v;
            }
            op::LOAD_GLOBAL => r!(ra) = self.globals[bx as usize],
            op::STORE_GLOBAL => self.globals[bx as usize] = r!(ra),
            op::MAKE_REF => r!(ra) = make_ref(1, rb as u64),
            op::MAKE_GLOBAL_REF => r!(ra) = make_ref(2, bx as u64),
            op::LOAD_REF => {
                let slot = r!(rb);
                let v = match deref(slot) {
                    (1, i) => regs.get(i).copied(),
                    (2, i) => self.globals.get(i).copied(),
                    _ => None,
                };
                let Some(v) = v else {
                    return Step::Fatal("bad reference slot".into());
                };
                r!(ra) = v;
            }
            op::STORE_REF => {
                let slot = r!(ra);
                let v = r!(rb);
                match deref(slot) {
                    (1, i) if i < regs.len() => regs[i] = v,
                    (2, i) if i < self.globals.len() => self.globals[i] = v,
                    _ => return Step::Fatal("bad reference slot".into()),
                }
            }
            op::ADD_INT => r!(ra) = r!(rb).wrapping_add(r!(rc)),
            op::SUB_INT => r!(ra) = r!(rb).wrapping_sub(r!(rc)),
            op::MUL_INT => r!(ra) = r!(rb).wrapping_mul(r!(rc)),
            op::DIV_INT | op::DIV_INT_CHK => {
                let d = r!(rc);
                if d == 0 {
                    return Step::Error(rt("EDivByZero", "division by zero"));
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
                    return Step::Error(rt("EDivByZero", "division by zero"));
                }
                r!(ra) = r!(rb).wrapping_rem(d);
            }
            op::DIV_UINT => {
                let d = r!(rc) as u64;
                if d == 0 {
                    return Step::Error(rt("EDivByZero", "division by zero"));
                }
                r!(ra) = ((r!(rb) as u64) / d) as i64;
            }
            op::MOD_UINT => {
                let d = r!(rc) as u64;
                if d == 0 {
                    return Step::Error(rt("EDivByZero", "division by zero"));
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
                        "ERangeError",
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
                let eq = self.heap.get(regs[ra]).equals(self.heap.get(regs[rb]));
                if eq == (opc == op::JMP_IF_EQ_STR) {
                    jump(sc, budget);
                }
            }
            op::CONCAT_STR => {
                let x = self.heap.get(regs[rb]);
                let y = self.heap.get(regs[rc]);
                if x.len() + y.len() > MAX_STRING_LEN {
                    return Step::Error(rt("EOutOfMemory", "out of memory"));
                }
                let s = if x.is_empty() {
                    regs[rc]
                } else if y.is_empty() {
                    regs[rb]
                } else {
                    let n = x.concat(y);
                    match self.alloc_string(n) {
                        Ok(v) => v,
                        Err(e) => return Step::Error(e),
                    }
                };
                self.regs[ra] = s;
            }
            op::STR_LEN => r!(ra) = self.heap.get(regs[rb]).len() as i64,
            op::STR_INDEX => {
                let s = self.heap.get(regs[rb]);
                let i = regs[rc];
                if i < 1 || i > s.len() as i64 {
                    return Step::Error(rt(
                        "ERangeError",
                        format!("string index {i} is out of range 1..{}", s.len()),
                    ));
                }
                regs[ra] = s.unit(i as usize - 1) as i64;
            }
            op::STR_SET_INDEX => {
                let s = self.heap.get(regs[ra]);
                let i = regs[rb];
                if i < 1 || i > s.len() as i64 {
                    return Step::Error(rt(
                        "ERangeError",
                        format!("string index {i} is out of range 1..{}", s.len()),
                    ));
                }
                let n = s.with_unit(i as usize - 1, regs[rc] as u16);
                match self.alloc_string(n) {
                    Ok(v) => self.regs[ra] = v,
                    Err(e) => return Step::Error(e),
                }
            }
            op::CMP_STR => {
                let o = self.heap.get(regs[rb]).compare(self.heap.get(regs[rc]));
                regs[ra] = o as i64;
            }
            op::EQ_STR => regs[ra] = self.heap.get(regs[rb]).equals(self.heap.get(regs[rc])) as i64,
            op::STR_FROM_CHAR => {
                let u = regs[rb] as u16;
                match self.alloc_string(PStr::from_units(&[u])) {
                    Ok(v) => self.regs[ra] = v,
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
                    Ok(x) => self.regs[ra] = x,
                    Err(e) => return Step::Error(e),
                }
            }
            op::STR_TO_INT | op::STR_TO_INT64 => {
                let s = self.heap.get(regs[rb]);
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
                        return Step::Error(rt(
                            "EConvertError",
                            format!("'{text}' is not a valid integer"),
                        ));
                    }
                }
            }
            op::STR_TO_INT_DEF => {
                let s = self.heap.get(regs[rb]);
                let units = s.units();
                let def = regs[rb + 1];
                regs[ra] = match strconv::parse_int(&units, false) {
                    Some(v) if v >= i32::MIN as i128 && v <= i32::MAX as i128 => v as i64,
                    _ => def,
                };
            }
            op::STR_TO_INT_IO => {
                let s = self.heap.get(regs[rb]);
                let units = s.units();
                let (_, lo, hi) = width_type(c);
                match strconv::parse_int(&units, true) {
                    Some(v) if v >= lo && v <= hi => regs[ra] = v as i64,
                    _ => {
                        let text = s.to_utf8();
                        return Step::Error(rt(
                            "EInOutError",
                            format!("invalid integer input: '{text}'"),
                        ));
                    }
                }
            }
            op::STR_COPY => {
                let s = self.heap.get(regs[rb]);
                let index = regs[rb + 1].max(1);
                let count = regs[rb + 2];
                let len = s.len() as i64;
                let v = if index > len || count <= 0 {
                    0
                } else {
                    let start = (index - 1) as usize;
                    let end = (index - 1 + count).min(len) as usize;
                    if start == 0 && end == len as usize {
                        regs[rb]
                    } else {
                        match self.alloc_string(s.slice(start, end)) {
                            Ok(x) => x,
                            Err(e) => return Step::Error(e),
                        }
                    }
                };
                self.regs[ra] = v;
            }
            op::STR_POS => {
                let sub = self.heap.get(regs[rb]).units();
                let s = self.heap.get(regs[rb + 1]).units();
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
                let s = self.heap.get(regs[rb]);
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
                    Ok(x) => self.regs[ra] = x,
                    Err(e) => return Step::Error(e),
                }
            }
            op::STR_TRIM => {
                let s = self.heap.get(regs[rb]);
                let units = s.units();
                let start = units.iter().position(|u| *u > 32).unwrap_or(units.len());
                let end = units
                    .iter()
                    .rposition(|u| *u > 32)
                    .map(|p| p + 1)
                    .unwrap_or(start);
                let v = if start == 0 && end == units.len() {
                    regs[rb]
                } else {
                    match self.alloc_string(PStr::from_units(&units[start..end])) {
                        Ok(x) => x,
                        Err(e) => return Step::Error(e),
                    }
                };
                self.regs[ra] = v;
            }
            op::STR_PAD_LEFT => {
                let w = regs[rc];
                let s = self.heap.get(regs[rb]);
                if w <= s.len() as i64 {
                    regs[ra] = regs[rb];
                } else {
                    if w > MAX_STRING_LEN as i64 {
                        return Step::Error(rt("EOutOfMemory", "out of memory"));
                    }
                    let pad = w as usize - s.len();
                    let mut units = vec![32u16; pad];
                    units.extend(s.units());
                    match self.alloc_string(PStr::from_units(&units)) {
                        Ok(x) => self.regs[ra] = x,
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
                if bytes > self.config.stack_limit {
                    return Step::Error(rt(
                        "EStackOverflow",
                        format!("stack overflow (limit {} bytes)", self.config.stack_limit),
                    ));
                }
                if self.regs.len() < need {
                    let grow = (need - self.regs.len()) * std::mem::size_of::<Slot>();
                    if self.heap.account(grow).is_err()
                        || self.regs.try_reserve(need - self.regs.len()).is_err()
                    {
                        return Step::Error(rt("EOutOfMemory", "out of memory"));
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
            op::CALL_HOST => {
                // Advance `pc` only on success (so that a failure is reported at the `CallHost` line)
                let step = self.call_host(bx as usize, ra as u32, next as u32);
                if matches!(step, Step::Continue) {
                    self.frames.last_mut().unwrap().pc = next as u32;
                }
                return step;
            }
            op::RET | op::RET_VOID => {
                let value = if opc == op::RET { regs[ra] } else { 0 };
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
                let code = (regs[ra] & 0xFF) as u8;
                return Step::Finished(ExitKind::Code(code));
            }
            op::RAISE_ERROR => {
                let msg = self.heap.get(regs[ra]).to_utf8();
                return Step::Error(rt("EAssertionFailed", msg));
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
                    return Step::Error(rt("EOutOfMemory", "out of memory"));
                }
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => self.regs[ra] = x,
                    Err(e) => return Step::Error(e),
                }
            }
            _ => return Step::Fatal(format!("unknown opcode {opc}")),
        }
        self.frames.last_mut().unwrap().pc = new_pc;
        Step::Continue
    }

    /// The first frame returned: start `<main>` if it was `<init>`, exit code 0 if it was `<main>`.
    fn frames_empty_after_return(&mut self) -> Step {
        match self.phase {
            Phase::Init => {
                self.phase = Phase::Main;
                let entry = self.module.entry;
                self.push_frame(entry, 0, 0);
                Step::Continue
            }
            _ => Step::Finished(ExitKind::Code(0)),
        }
    }

    fn call_host(&mut self, index: usize, dst: u32, resume_pc: u32) -> Step {
        let kind = self.host_table[index];
        match kind {
            HostFn::ConsoleWrite | HostFn::ConsoleWriteError => {
                let text = self.heap.get(self.regs[dst as usize]).to_utf8();
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
                    dst,
                    resume_pc,
                });
            }
            HostFn::ConsoleReadLine => {
                if let Err(e) = self.flush_stdout() {
                    return Step::Error(host_error_to_rt(e));
                }
                let Some(console) = self.host.console.as_mut() else {
                    return Step::Error(rt(
                        "ENotSupportedException",
                        "not supported on this target: Console.ReadLine",
                    ));
                };
                let fut = console.read_line();
                self.pending = Some(PendingHost {
                    kind: PendingKind::ReadLine(fut),
                    dst,
                    resume_pc,
                });
            }
            HostFn::EnvGet => {
                let name = self.heap.get(self.regs[dst as usize]).to_utf8();
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
                    return Step::Error(rt("EOutOfMemory", "out of memory"));
                }
                match self.alloc_string(PStr::from_text(&text)) {
                    Ok(x) => self.regs[dst as usize] = x,
                    Err(e) => return Step::Error(e),
                }
                return Step::Continue;
            }
        }
        match self.poll_pending() {
            Ok(true) => Step::Continue,
            Ok(false) => Step::Waiting,
            Err(e) => Step::Error(e),
        }
    }
}

enum Step {
    Continue,
    Waiting,
    Finished(ExitKind),
    Error(RtError),
    Fatal(String),
}

fn host_error_to_rt(e: HostError) -> RtError {
    match e.kind {
        HostErrorKind::Unsupported | HostErrorKind::NoPermission => rt(
            "ENotSupportedException",
            format!("not supported on this target: {}", e.message),
        ),
        _ => rt("EInOutError", e.message),
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
            ExitKind::UncaughtException(rep) => (rep.class, rep.message, r.stdout),
            other => panic!("expected an exception, got {other:?}\n{}", r.stdout),
        }
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

    #[test]
    fn wide_constant_index() {
        use crate::bytecode::{Function, UnitEntry, encode, encode_ext};

        let mut consts = vec![ConstEntry::Int(0); 65537];
        consts[65536] = ConstEntry::Int(42);
        let module = Module {
            consts,
            funcs: vec![Function {
                name: "Test.<main>".into(),
                params: 0,
                regs: 1,
                code: vec![
                    encode(op::WIDE, 0, 0, 0),
                    encode(op::LOAD_CONST, 0, 0, 0),
                    encode_ext(0, 256, 0),
                    encode(op::HALT, 0, 0, 0),
                ],
                lines: vec![(0, 1)],
                locals: Vec::new(),
                file: 0,
            }],
            globals: Vec::new(),
            entry: 0,
            units: vec![UnitEntry {
                name: "Test".into(),
                path: "t.pas".into(),
                init: None,
                fini: None,
            }],
            hosts: Vec::new(),
        };
        let module = Module::decode(&module.encode(crate::VERSION), crate::VERSION).unwrap();
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
        match iso.run(1000) {
            RunOutcome::Finished(status) => assert_eq!(status.kind, ExitKind::Code(42)),
            other => panic!("{other:?}"),
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
}
