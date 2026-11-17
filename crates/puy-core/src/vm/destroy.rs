//! The destruction of an instance (`docs/spec/objects.md` 3.3, 8.5; `docs/design/objects.md` 3.7):
//! the VM's built-in function that runs the steps in a frame of its own, so that `BeforeDestruction`
//! and the destructors are called like any method, without recursion on the Rust stack (§4.3).
//!
//! The frame of the procedure has [`DTOR_FN`] as its function. Its `pc` is the step it is at
//! (`STAGE_*`), and its registers are: the instance's handle, the exception recorded so far
//! (holding its reference), the flags, and the class whose parent's destructor was called last.
//! The callee of a step gets its frame right after them (`Self` in its first register).
//!
//! The release jobs (`docs/design/objects.md` 3.8) are run here too: an instruction whose
//! releases reach an instance of a reference counted class that lost its last reference leaves
//! an open job on the heap, which the VM stamps with where it came from and runs before going on.

use super::*;
use heap::{JobOrigin, JobStep};
use std::collections::VecDeque;

/// The function index of a frame of the destruction procedure (no function of a module has it).
pub(super) const DTOR_FN: u32 = u32::MAX;

/// The registers of the procedure's frame; the frame of a destructor it calls starts after them.
const DTOR_REGS: usize = 4;
const R_HANDLE: usize = 0;
const R_EXCEPTION: usize = 1;
const R_FLAGS: usize = 2;
const R_CHAINED: usize = 3;

/// [`R_FLAGS`]: `BeforeDestruction` is skipped (the clean-up after a constructor's exception; 3.1).
const FLAG_SKIP_BEFORE: i64 = 1;
/// [`R_FLAGS`]: a release job started the procedure (the instance lost its last reference); the
/// fields and the exception go back to that job, the last one, when it ends.
const FLAG_FROM_JOB: i64 = 2;

/// The steps (the frame's `pc`).
const STAGE_START: u32 = 0;
/// `BeforeDestruction` was called (8.5, 1: whatever it ends with, `Destroy` comes next).
const STAGE_IN_BEFORE: u32 = 1;
const STAGE_DESTROY: u32 = 2;
/// `Destroy` (or a parent's destructor the chain called) was called.
const STAGE_IN_DESTROY: u32 = 3;
/// `Destroy` ended with an exception: the parents' destructors it skipped (8.5, 2).
const STAGE_CHAIN: u32 = 4;
/// The managed fields and the memory (3.3, steps 4 and 5).
const STAGE_FIELDS: u32 = 5;

/// The message of `Free` / `Destroy` of an instance being destroyed (`docs/spec/objects.md` 3.2).
fn being_destroyed(class: &str) -> RtError {
    rt(
        exc::INVALID_OPERATION,
        format!("object of class `{class}` is being destroyed"),
    )
}

impl Isolate {
    /// Starts the destruction procedure of the instance `h` from the instruction at `pc` of the top
    /// frame (`Free`, `Destroy`, `DestroyFailed`; `next` is the instruction after it). The frame of
    /// the procedure goes on top; the instruction goes on after it (or raises its exception, 8.5).
    /// `skip_before`: the clean-up after a constructor's exception.
    pub(super) fn start_destroy(&mut self, h: Slot, next: usize, skip_before: bool) -> Step {
        if let Ok(ty) = self.heap.obj_class(h)
            && !self.heap.destruction_runs_code(ty)
        {
            return self.destroy_now(h, next);
        }
        // The procedure's registers first, so that a failure leaves the instance untouched
        if let Err(step) = self.dtor_regs() {
            return step;
        }
        if let Err(step) = self.begin_destroy_step(h) {
            return step;
        }
        // As after a `Call`: the caller's position is one word before its `pc` (`frame_pos`)
        self.frames.last_mut().expect("the caller").pc = next as u32;
        self.push_dtor(h, if skip_before { FLAG_SKIP_BEFORE } else { 0 });
        Step::Continue
    }

    /// [`Isolate::start_destroy`] of an instance whose class has neither a destructor nor a
    /// `BeforeDestruction` written in Pascal (`docs/spec/objects.md` 14.8): the steps run at once
    /// without the procedure's frame, as [`Isolate::dtor_end`] ends them (the fields go to a
    /// release job stamped with the instruction; none when no field holds a reference).
    fn destroy_now(&mut self, h: Slot, next: usize) -> Step {
        if let Err(step) = self.begin_destroy_step(h) {
            return step;
        }
        let remains = match self.heap.end_destroy(h) {
            Ok(v) => v,
            Err(e) => return RefError::from(e).into_step(),
        };
        let fr = self.frames.last_mut().expect("the caller");
        fr.pc = next as u32;
        // The instruction that started the destruction, as `dtor_end` finds it
        let pc = fr.pc.saturating_sub(1);
        let depth = self.frames.len() as u32;
        self.heap
            .push_fields_job(&remains, JobOrigin::Instr { pc }, depth);
        Step::Continue
    }

    /// Step 1 of the destruction (`docs/spec/objects.md` 3.2, 3.3): marks the instance `h` as
    /// being destroyed, or the step to return when it cannot start.
    fn begin_destroy_step(&mut self, h: Slot) -> Result<(), Step> {
        match self.heap.begin_destroy(h) {
            Ok(_) => Ok(()),
            Err(Some(e)) => Err(RefError::from(e).into_step()),
            Err(None) => {
                let name = self.instance_class_name(h);
                Err(Step::Error(being_destroyed(&name)))
            }
        }
    }

    /// The declared name of the class of the instance `h` (empty if it does not check).
    pub(super) fn instance_class_name(&self, h: Slot) -> String {
        match self
            .heap
            .obj_class(h)
            .ok()
            .and_then(|t| self.heap.type_entry(t))
        {
            Some(entry) => class_decl_name(&entry.name).to_string(),
            None => String::new(),
        }
    }

    /// The first register of a frame of the procedure pushed now: after the top frame's
    /// registers (0 without frames: the end of the program).
    fn dtor_base(&self) -> usize {
        self.frames.last().map_or(0, |fr| {
            let regs = if fr.func == DTOR_FN {
                DTOR_REGS
            } else {
                self.module.funcs[fr.func as usize].regs as usize
            };
            fr.base as usize + regs
        })
    }

    /// Makes room for the registers of a frame of the procedure (counted toward the usage limit).
    fn dtor_regs(&mut self) -> Result<(), Step> {
        // The procedure runs in `dtor_run`, not the fast path: no window
        let need = self.dtor_base() + DTOR_REGS;
        self.grow_regs(need).map_err(Step::Error)
    }

    /// Pushes the procedure's frame for the instance `h` (marked as being destroyed); its
    /// registers have room ([`Isolate::dtor_regs`]).
    fn push_dtor(&mut self, h: Slot, flags: i64) {
        let base = self.dtor_base();
        self.regs[base + R_HANDLE] = h;
        self.regs[base + R_EXCEPTION] = 0;
        self.regs[base + R_FLAGS] = flags;
        self.regs[base + R_CHAINED] = -1;
        // The procedure's frames are not counted toward the recursion limit (3.3): no check here
        self.frames.push(Frame {
            func: DTOR_FN,
            base: base as u32,
            pc: STAGE_START,
            ret_reg: base as u32,
        });
    }

    /// [`Isolate::dtor_step`] from the fast path, counted as one step of the budget. Kept out of
    /// `run_loop` so that the loop's code does not grow (`docs/design/vm.md` 3.14).
    #[cold]
    #[inline(never)]
    pub(super) fn dtor_run(&mut self, budget: &mut u32) -> Step {
        *budget = budget.saturating_sub(1);
        self.dtor_step()
    }

    /// Runs the destruction procedure in the top frame until it calls a method or ends.
    #[inline(never)]
    pub(super) fn dtor_step(&mut self) -> Step {
        let fr = *self.frames.last().expect("the procedure's frame");
        let base = fr.base as usize;
        let h = self.regs[base + R_HANDLE];
        let mut stage = fr.pc;
        loop {
            match stage {
                STAGE_START => {
                    stage = STAGE_DESTROY;
                    if self.regs[base + R_FLAGS] & FLAG_SKIP_BEFORE == 0
                        && let Some(f) = self.dtor_entry(h, bytecode::VMT_BEFORE_DESTRUCTION)
                    {
                        return self.dtor_call(f, STAGE_IN_BEFORE);
                    }
                }
                STAGE_IN_BEFORE => stage = STAGE_DESTROY,
                STAGE_DESTROY => {
                    stage = STAGE_FIELDS;
                    if let Some(f) = self.dtor_entry(h, bytecode::VMT_DESTROY) {
                        return self.dtor_call(f, STAGE_IN_DESTROY);
                    }
                }
                // The destructor returned: it called `inherited Destroy` itself, or chose not to
                STAGE_IN_DESTROY => stage = STAGE_FIELDS,
                STAGE_CHAIN => {
                    stage = STAGE_FIELDS;
                    // The parent of the most distant class whose destructor started (8.5, 2)
                    let recorded = match self.heap.dtor_state(h) {
                        Ok(r) => r,
                        Err(e) => return RefError::from(e).into_step(),
                    };
                    if let Some(r) = recorded
                        && i64::from(r) != self.regs[base + R_CHAINED]
                        && let Some(f) = self
                            .heap
                            .type_entry(r)
                            .and_then(|t| t.parent)
                            .and_then(|p| self.heap.type_entry(p))
                            .and_then(|p| p.vmt.get(bytecode::VMT_DESTROY as usize).copied())
                            .filter(|&f| f < bytecode::VMT_BUILTIN)
                    {
                        self.regs[base + R_CHAINED] = i64::from(r);
                        return self.dtor_call(f, STAGE_IN_DESTROY);
                    }
                }
                STAGE_FIELDS => return self.dtor_end(),
                _ => return Step::Fatal(format!("destruction step {stage} out of range")),
            }
        }
    }

    /// The function of the entry `k` of the instance's class's table, `None` for a built-in body
    /// (it does nothing).
    fn dtor_entry(&self, h: Slot, k: u32) -> Option<u32> {
        let ty = self.heap.obj_class(h).ok()?;
        let f = *self.heap.type_entry(ty)?.vmt.get(k as usize)?;
        (f < bytecode::VMT_BUILTIN).then_some(f)
    }

    /// Calls the method `f` on the instance from the procedure's frame, which goes on at `stage`
    /// when it returns (or ends with an exception; [`Isolate::dtor_caught`]).
    fn dtor_call(&mut self, f: u32, stage: u32) -> Step {
        let fr = self.frames.last_mut().expect("the procedure's frame");
        fr.pc = stage;
        let base = fr.base as usize;
        let module = Arc::clone(&self.module);
        let Some(func) = module.funcs.get(f as usize) else {
            return Step::Fatal(format!("function {f} out of range"));
        };
        if func.params != 1 {
            return Step::Fatal(format!(
                "destructor {} takes {} arguments",
                func.name, func.params
            ));
        }
        let new_base = base + DTOR_REGS;
        let need = new_base + func.regs as usize;
        // A method the procedure calls counts toward the recursion limit like any other (3.3)
        let bytes = need * std::mem::size_of::<Slot>()
            + (self.frames.len() + 1) * std::mem::size_of::<Frame>();
        if bytes > self.config.stack_limit {
            return Step::Error(rt(
                exc::STACK_OVERFLOW,
                format!("stack overflow (limit {} bytes)", self.config.stack_limit),
            ));
        }
        if let Err(e) = self.grow_regs(super::frame_end(new_base, func.regs as usize)) {
            return Step::Error(e);
        }
        self.regs[new_base] = self.regs[base + R_HANDLE];
        self.regs[new_base + 1..need].fill(0);
        self.frames.push(Frame {
            func: f,
            base: new_base as u32,
            pc: 0,
            ret_reg: new_base as u32,
        });
        Step::Continue
    }

    /// The end of the procedure (3.3, steps 4 and 5): the memory, then the fields and the recorded
    /// exception go to a release job: the one that started the procedure, or a new one stamped
    /// with the instruction that did (raised there; 8.5).
    fn dtor_end(&mut self) -> Step {
        let fr = self.frames.pop().expect("the procedure's frame");
        let base = fr.base as usize;
        let h = std::mem::take(&mut self.regs[base + R_HANDLE]);
        let x = std::mem::take(&mut self.regs[base + R_EXCEPTION]);
        let flags = self.regs[base + R_FLAGS];
        let remains = match self.heap.end_destroy(h) {
            Ok(v) => v,
            Err(e) => return RefError::from(e).into_step(),
        };
        if flags & FLAG_FROM_JOB != 0 {
            self.heap.job_push_fields(&remains);
            if x != 0 {
                let prev = self.heap.job_record(x);
                if let Err(e) = self.attach_suppressed(x, prev) {
                    return Step::Fatal(e.message);
                }
            }
            return Step::Continue;
        }
        // The instruction that started the procedure is the innermost position again
        let pc = self.frames.last().map_or(0, |c| c.pc.saturating_sub(1));
        self.heap.push_job(
            VecDeque::new(),
            x,
            JobOrigin::Instr { pc },
            self.frames.len() as u32,
        );
        self.heap.job_push_fields(&remains);
        Step::Continue
    }

    /// The unwinding of `x` reached the procedure's frame (`docs/spec/objects.md` 8.5, 14.8): the
    /// exception is recorded (the one before goes to its `Suppressed`) and the procedure goes on
    /// with the next step instead of leaving the frame.
    pub(super) fn dtor_caught(&mut self, x: Slot) -> Result<(), BadRef> {
        let fr = self.frames.last_mut().expect("the procedure's frame");
        fr.pc = match fr.pc {
            STAGE_IN_BEFORE => STAGE_DESTROY,
            STAGE_IN_DESTROY => STAGE_CHAIN,
            _ => STAGE_FIELDS,
        };
        let r = fr.base as usize + R_EXCEPTION;
        let old = std::mem::replace(&mut self.regs[r], x);
        self.attach_suppressed(x, old)
    }

    /// Leaves the procedure's frame on top without calling anything more (`Halt`, 8.5; and the
    /// frames left when the program ends): the memory goes, the fields go to a release job as in
    /// [`Isolate::dtor_end`], and the recorded exception is dropped.
    pub(super) fn dtor_abandon(&mut self) -> Result<(), BadRef> {
        let fr = self.frames.pop().expect("the procedure's frame");
        let base = fr.base as usize;
        let h = std::mem::take(&mut self.regs[base + R_HANDLE]);
        let x = std::mem::take(&mut self.regs[base + R_EXCEPTION]);
        let flags = self.regs[base + R_FLAGS];
        self.heap.release(x)?;
        // A handle that does not match any more was already destroyed: nothing is left to do
        let Ok(remains) = self.heap.end_destroy(h) else {
            return Ok(());
        };
        if flags & FLAG_FROM_JOB == 0 {
            let pc = self.frames.last().map_or(0, |c| c.pc.saturating_sub(1));
            self.heap.push_job(
                VecDeque::new(),
                0,
                JobOrigin::Instr { pc },
                self.frames.len() as u32,
            );
        }
        self.heap.job_push_fields(&remains);
        Ok(())
    }

    // ---- Release jobs ----

    /// Stamps the job the instruction at `pc` of the top frame opened (`docs/design/objects.md`
    /// 3.8): its exception is raised at that instruction.
    pub(super) fn stamp_instr(&mut self, pc: u32) {
        self.heap
            .stamp_job(JobOrigin::Instr { pc }, self.frames.len() as u32);
    }

    /// Runs the release jobs of the current frames (`docs/design/objects.md` 3.8), the last first,
    /// until one needs a frame for a destruction (pushed; `Continue`), or there is nothing to do
    /// at this depth (`Continue`), or the end of a job changes the course of execution (raises its
    /// exception, finishes the frames, …).
    pub(super) fn run_jobs(&mut self) -> Step {
        loop {
            let Some((origin, depth)) = self.heap.top_job() else {
                return Step::Continue;
            };
            let frames = self.frames.len() as u32;
            if origin == JobOrigin::Open {
                // Opened by the VM itself between instructions (an exception let go of, …)
                self.heap.stamp_job(JobOrigin::Nested, frames);
                continue;
            }
            if depth < frames {
                // A destructor's frames run above it
                return Step::Continue;
            }
            match self.heap.run_job() {
                Err(e) => return Step::Fatal(e.message),
                Ok(JobStep::Destroy(h)) => {
                    if let Err(step) = self.dtor_regs() {
                        // No room for the procedure: the job takes the exception and the
                        // instance is left as it is (destroyed as a leak would be)
                        let Step::Error(e) = step else { return step };
                        let x = match self.exception_of(e) {
                            Ok(x) => x,
                            Err(b) => return Step::Fatal(b.message),
                        };
                        let prev = self.heap.job_record(x);
                        if let Err(e) = self.attach_suppressed(x, prev) {
                            return Step::Fatal(e.message);
                        }
                        continue;
                    }
                    self.push_dtor(h, FLAG_FROM_JOB);
                    return Step::Continue;
                }
                Ok(JobStep::Done) => {
                    let (x, origin) = self.heap.pop_job();
                    if let Some(step) = self.job_done(x, origin) {
                        return step;
                    }
                }
            }
        }
    }

    /// The end of a job with the exception `x` (0 for none), by where it came from
    /// (`docs/spec/objects.md` 8.5). `None` when execution goes on as it was.
    fn job_done(&mut self, x: Slot, origin: JobOrigin) -> Option<Step> {
        if let JobOrigin::Globals { .. } = origin {
            // The end of the isolate: reported like an uncaught exception of a `<fini>`
            if x != 0 {
                match self.build_report(x, 0) {
                    Ok(report) => self.fini_exceptions.push(report),
                    Err(b) => return Some(Step::Fatal(b.message)),
                }
                if let Err(b) = self.heap.release(x) {
                    return Some(Step::Fatal(b.message));
                }
            }
            return Some(match self.advance_finishing() {
                Some(o) => Step::Done(Box::new(o)),
                None => Step::Continue,
            });
        }
        match origin {
            JobOrigin::Instr { pc } => {
                if x == 0 {
                    return None;
                }
                if let Some(fr) = self.frames.last_mut() {
                    fr.pc = pc;
                }
                Some(Step::Throw(x))
            }
            JobOrigin::Ret { pc, result, kind } => {
                if x == 0 {
                    return None;
                }
                // The result never reaches its destination
                let v = std::mem::take(&mut self.regs[result as usize]);
                let released = match kind {
                    bytecode::MANAGED_HEAP_REF => self.heap.release(v),
                    bytecode::MANAGED_OBJ => self.heap.release_obj(v),
                    _ => Ok(()),
                };
                if let Err(b) = released {
                    return Some(Step::Fatal(b.message));
                }
                if let Some(fr) = self.frames.last_mut() {
                    fr.pc = pc;
                }
                Some(Step::Throw(x))
            }
            JobOrigin::RetLast => Some(if x == 0 {
                self.frames_empty_after_return()
            } else {
                Step::Throw(x)
            }),
            JobOrigin::Unwind { pc } => {
                if let (Some(pc), Some(fr)) = (pc, self.frames.last_mut()) {
                    fr.pc = pc;
                }
                Some(Step::Throw(x))
            }
            JobOrigin::Halt => {
                // `Halt` goes on; an exception is reported like one leaving a `finally` block
                // during `Halt` (`docs/spec/exceptions.md` 5.3)
                if x != 0 {
                    match self.build_report(x, 0) {
                        Ok(report) => self.halt_exceptions.push(report),
                        Err(b) => return Some(Step::Fatal(b.message)),
                    }
                    if let Err(b) = self.heap.release(x) {
                        return Some(Step::Fatal(b.message));
                    }
                }
                Some(Step::Halt(None))
            }
            JobOrigin::Nested => {
                if x != 0 {
                    if self.heap.has_jobs() {
                        let prev = self.heap.job_record(x);
                        if let Err(b) = self.attach_suppressed(x, prev) {
                            return Some(Step::Fatal(b.message));
                        }
                    } else if let Err(b) = self.heap.release(x) {
                        return Some(Step::Fatal(b.message));
                    }
                }
                None
            }
            JobOrigin::Open | JobOrigin::Globals { .. } => {
                unreachable!("a stamped job; the end of the isolate is handled above")
            }
        }
    }
}
