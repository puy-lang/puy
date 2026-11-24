//! Calls of extension functions (`CallExt`; `docs/spec/extensions.md` 4, 5,
//! `docs/design/extensions.md` 3.3–3.9).
//!
//! The isolate binds each function of the module's extension section to the Rust function of the
//! same name when it is made ([`bind`]; F0007 when one is missing). `CallExt` lets the function
//! read its arguments through [`ExtCall`] (the VM's side of [`puy_ext::Vm`]), and makes the
//! [`Value`] it gives back the Pascal value of the declared kind. A suspendable function's future
//! waits in the task like a host call's (`host.rs`).
//!
//! A function may call a Pascal callable value back ([`Isolate::call_back`]): the VM pushes a
//! boundary frame ([`CALLBACK_FN`]) and runs its execution loop, nested on the Rust stack, until
//! the callback returns to the boundary. That is the one place Pascal runs inside a Rust call
//! (`docs/design/stackless-vm.md` 3.6): it cannot wait, and its depth is limited.

use std::any::TypeId;
use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};

use puy_ext::{
    Body, CallCtx, ErrorExtra, ExtensionSet, HostSet, PasError, PasRef, PasStr, RefKind,
    ReleaseQueue, Value,
};

use super::heap::{self, PStr};
use super::host::{Deliver, WAIT_IN_CALLBACK};
use super::task::{Frame, Pending};
use super::{
    Extra, IsolateError, RtError, Slot, Step, Unwound, exc, noop_waker, oom, rt, slot_f32, slot_f64,
};
use super::{Isolate, f32_slot, f64_slot};
use crate::bytecode::{self, ExtEntry, ExtShape, ExtSig, Module, ext_kind};
use crate::sema::types::MAX_STRING_LEN;

/// The function of the boundary frame a callback from an extension runs above
/// (`docs/design/extensions.md` 3.9). Its `ret_reg` holds the number of its registers.
pub(super) const CALLBACK_FN: u32 = u32::MAX - 1;

/// The deepest nesting of callbacks (an extension calling Pascal calling an extension …):
/// each level takes Rust stack (`docs/spec/extensions.md` 5.2).
pub(super) const MAX_CALLBACK_DEPTH: u32 = 64;

/// The extension functions and classes of an isolate, and the state of its callbacks.
pub(super) struct ExtState {
    /// The body of each function of the module's extension section.
    fns: Vec<Body>,
    /// Each extension class: its type index, and the type of the Rust value its instances hold.
    classes: Vec<(u32, TypeId)>,
    /// The references Rust let go of, let go of by the VM when it can.
    pub(super) queue: Rc<ReleaseQueue>,
    /// The number of callbacks running.
    depth: u32,
    /// How the innermost callback ended when it did not return: its uncaught exception (whose
    /// reference this holds), or `Halt`.
    exit: Option<Exit>,
    /// A callback ran `Halt`: the extension function's call goes on with it once it returns.
    halting: bool,
    /// An internal error inside a call (a bad reference; F0001): the call ends with it. Set
    /// from the argument accessors that only borrow the isolate.
    fatal: RefCell<Option<String>>,
}

enum Exit {
    Exception(Slot),
    Halt,
}

/// The failure of turning a value of an extension into a Pascal value.
pub(super) enum ValueFail {
    Rt(RtError),
    Fatal(String),
}

/// Binds the module's extension units and functions to the runtime's extensions
/// (`docs/spec/extensions.md` 4): each unit is in `set` with the fingerprint the module was
/// compiled with, and each function and class is in its unit's table (F0007).
pub(super) fn bind(module: &Module, set: &ExtensionSet) -> Result<ExtState, IsolateError> {
    let mut exts = Vec::with_capacity(module.ext_units.len());
    let mut classes = Vec::new();
    for u in &module.ext_units {
        let Some(ext) = set.find(&u.name) else {
            return Err(IsolateError::Extension(format!(
                "extension unit `{}` is not in this runtime",
                u.name
            )));
        };
        if ext.fingerprint != u.fingerprint {
            return Err(IsolateError::Extension(format!(
                "extension unit `{}` differs from the one the program was compiled with",
                u.name
            )));
        }
        for (name, ty) in &u.classes {
            let Some(c) = ext
                .classes
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
            else {
                return Err(IsolateError::Extension(format!(
                    "extension class `{}.{name}` is not in this runtime",
                    u.name
                )));
            };
            classes.push((*ty, (c.type_id)()));
        }
        exts.push(ext);
    }
    let mut fns = Vec::with_capacity(module.exts.len());
    for e in &module.exts {
        let ext = exts[e.unit as usize];
        let Some(f) = ext
            .functions
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&e.name))
        else {
            return Err(IsolateError::Extension(format!(
                "extension function `{}.{}` is not in this runtime",
                ext.unit, e.name
            )));
        };
        fns.push(f.body);
    }
    Ok(ExtState {
        fns,
        classes,
        queue: Rc::new(ReleaseQueue::default()),
        depth: 0,
        exit: None,
        halting: false,
        fatal: RefCell::new(None),
    })
}

/// What an extension function sees of the VM while it runs: its arguments in the registers from
/// `base`, of the kinds of its entry.
struct ExtCall<'a> {
    iso: &'a mut Isolate,
    base: usize,
    /// The index of the function in the extension section.
    index: usize,
    entry: &'a ExtEntry,
}

impl ExtCall<'_> {
    /// The register of argument `i` if it is of one of the kinds `kinds`.
    fn arg(&self, i: usize, kinds: &[u8], what: &str) -> Result<Slot, PasError> {
        match self.entry.params.get(i) {
            Some(k) if kinds.contains(k) => Ok(self.iso.task.regs[self.base + i]),
            Some(_) => Err(PasError::new(
                "EInvalidCast",
                format!(
                    "argument {i} of `{}` is not {what} (the declaration and the extension disagree)",
                    self.entry.name
                ),
            )),
            None => Err(PasError::new(
                "EInvalidCast",
                format!("`{}` has no argument {i}", self.entry.name),
            )),
        }
    }

    /// An internal error: the call ends with it.
    fn fatal(&self, message: String) -> PasError {
        self.iso.ext.fatal.borrow_mut().get_or_insert(message);
        PasError::Halt
    }

    /// A counted reference to the managed value `v` for Rust (`None` for `nil`).
    fn counted(&mut self, v: Slot, kind: RefKind, tag: u32) -> Result<Option<PasRef>, PasError> {
        if v == 0 {
            return Ok(None);
        }
        if let Err(e) = self.iso.heap.add_ref(v) {
            return Err(self.fatal(e.message));
        }
        Ok(Some(PasRef::from_raw(
            v,
            kind,
            tag,
            Rc::clone(&self.iso.ext.queue),
        )))
    }
}

impl puy_ext::Vm for ExtCall<'_> {
    fn arg_count(&self) -> usize {
        self.entry.params.len()
    }

    fn arg_int(&self, i: usize) -> Result<i64, PasError> {
        self.arg(i, &[ext_kind::INT], "an ordinal")
    }

    fn arg_float(&self, i: usize) -> Result<f64, PasError> {
        let v = self.arg(i, &[ext_kind::SINGLE, ext_kind::DOUBLE], "a real")?;
        Ok(if self.entry.params[i] == ext_kind::SINGLE {
            f64::from(slot_f32(v))
        } else {
            slot_f64(v)
        })
    }

    fn arg_str(&self, i: usize) -> Result<PasStr<'_>, PasError> {
        let v = self.arg(i, &[ext_kind::STRING], "a string")?;
        match self.iso.heap.get(v) {
            Ok(s) => Ok(s.ext_view()),
            Err(e) => Err(self.fatal(e.message)),
        }
    }

    fn arg_utf8(&self, i: usize) -> Result<Cow<'_, str>, PasError> {
        let v = self.arg(i, &[ext_kind::STRING, ext_kind::UTF8], "a string")?;
        if self.entry.params[i] == ext_kind::UTF8 {
            return match self.iso.heap.get_u8(v) {
                // A `UTF8String` holds valid UTF-8 (`docs/spec/types.md` 8.5)
                Ok(b) => Ok(Cow::Borrowed(
                    std::str::from_utf8(b).expect("a UTF8String is valid UTF-8"),
                )),
                Err(e) => Err(self.fatal(e.message)),
            };
        }
        match self.iso.heap.get(v) {
            Ok(s) => Ok(match s.ext_view() {
                PasStr::Ascii(a) => Cow::Borrowed(a),
                PasStr::Utf16(_) => Cow::Owned(s.text()),
            }),
            Err(e) => Err(self.fatal(e.message)),
        }
    }

    fn arg_bytes(&self, i: usize) -> Result<Cow<'_, [u8]>, PasError> {
        let v = self.arg(i, &[ext_kind::BYTES], "a TBytes")?;
        // One byte per element (`docs/spec/types.md` 9.1): borrowed as it is
        match self.iso.heap.bytes_of(v) {
            Ok(b) => Ok(Cow::Borrowed(b)),
            Err(e) => Err(self.fatal(e.message)),
        }
    }

    fn arg_value(&self, i: usize) -> Result<Value, PasError> {
        let v = self.arg(
            i,
            &[
                ext_kind::INT,
                ext_kind::SINGLE,
                ext_kind::DOUBLE,
                ext_kind::STRING,
                ext_kind::UTF8,
                ext_kind::BYTES,
                ext_kind::RECORD,
                ext_kind::ARRAY,
            ],
            "a value that is copied",
        )?;
        let k = self.entry.params[i];
        let leaf;
        let shape = match self.entry.param_shape(i).filter(|_| ext_kind::has_shape(k)) {
            Some(s) => s,
            None => {
                leaf = ExtShape::leaf(k, bytecode::NONE_INDEX, bytecode::FULL_RANGE);
                &leaf
            }
        };
        self.iso
            .ext_read(v, shape)
            .map_err(|e| self.fatal(e.message))
    }

    fn arg_object(&mut self, i: usize) -> Result<Option<puy_ext::ObjectArg>, PasError> {
        let v = self.arg(i, &[ext_kind::OBJECT], "an instance")?;
        if v == 0 {
            return Ok(None);
        }
        let value = match self.iso.heap.obj_ext(v) {
            Ok(Some(x)) => x,
            Ok(None) => {
                return Err(PasError::new(
                    "EInvalidCast",
                    "the instance holds no value of the extension",
                ));
            }
            Err(heap::BadHandle::Nil) => return Ok(None),
            Err(heap::BadHandle::Freed) => {
                return Err(PasError::new(
                    "EAccessViolation",
                    super::FREED_ACCESS.to_string(),
                ));
            }
            Err(heap::BadHandle::Fatal(m)) => return Err(self.fatal(m)),
        };
        let r = self.counted(v, RefKind::Object, 0)?;
        Ok(r.map(|r| (value, r)))
    }

    fn arg_ref(&mut self, i: usize) -> Result<Option<PasRef>, PasError> {
        let v = self.arg(
            i,
            &[
                ext_kind::STRING,
                ext_kind::UTF8,
                ext_kind::BYTES,
                ext_kind::OBJECT,
                ext_kind::CALLABLE,
            ],
            "a managed value",
        )?;
        let (kind, tag) = match self.entry.params[i] {
            ext_kind::OBJECT => (RefKind::Object, 0),
            ext_kind::CALLABLE => {
                // The signature: which of the function's callable arguments this is
                let k = self.entry.params[..i]
                    .iter()
                    .filter(|k| **k == ext_kind::CALLABLE)
                    .count();
                (RefKind::Block, callable_tag(self.index, k))
            }
            _ => (RefKind::Block, 0),
        };
        self.counted(v, kind, tag)
    }

    fn host(&mut self) -> &mut HostSet {
        &mut self.iso.host
    }

    fn isolate_memory(&self) -> puy_ext::IsolateMemory {
        puy_ext::IsolateMemory {
            used: self.iso.memory_used(),
            peak: self.iso.heap.peak(),
            limit: self.iso.heap.limit(),
        }
    }

    fn reset_isolate_peak(&mut self) {
        self.iso.heap.reset_peak();
    }

    fn isolate_live_types(&self) -> Vec<puy_ext::IsolateTypeUsage> {
        self.iso.live_type_usage()
    }

    fn isolate_live_block_count(&self) -> usize {
        self.iso.heap.live_block_count()
    }

    fn isolate_live_block_bytes(&self) -> usize {
        self.iso.heap.live_block_bytes()
    }

    fn env(&mut self) -> (&mut puy_ext::EnvTable, &mut HostSet) {
        (&mut self.iso.env, &mut self.iso.host)
    }

    fn call(&mut self, f: &PasRef, args: Vec<Value>) -> Result<Value, PasError> {
        self.iso.call_back(f, args)
    }

    fn frames(&self) -> Vec<puy_ext::Frame> {
        self.iso.ext_frames()
    }
}

/// A short text of a value an extension gave, for the message of a value that does not fit its
/// declaration: its first characters, without writing out the whole of a large value.
fn value_text(v: &Value) -> String {
    use std::fmt::Write;
    /// Takes characters until it has `left` more, then stops the writing.
    struct Short {
        text: String,
        left: usize,
    }
    impl Write for Short {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            for c in s.chars() {
                if self.left == 0 {
                    self.text.push('…');
                    return Err(std::fmt::Error);
                }
                self.text.push(c);
                self.left -= 1;
            }
            Ok(())
        }
    }
    let mut out = Short {
        text: String::new(),
        left: 60,
    };
    // An error only stops the writing at the limit
    let _ = write!(out, "{v:?}");
    out.text
}

/// The tag of a callable value Rust holds: the function whose argument it was, and which of its
/// callable arguments, which name the signature it is called with.
fn callable_tag(index: usize, k: usize) -> u32 {
    ((index as u32) << 8) | k as u32
}

impl Isolate {
    /// `CallExt A Bx` (`docs/spec/extensions.md` 5.1): the extension function `index` with its
    /// arguments from register `ra` (absolute position). `pc` is the instruction, `next` the one
    /// after it; the frame goes on at `next` when the call is done (now, or when the task waited
    /// for it).
    pub(super) fn call_ext(&mut self, index: usize, ra: u32, pc: u32, next: u32) -> Step {
        let module = Arc::clone(&self.module);
        let entry = &module.exts[index];
        let body = self.ext.fns[index];
        // While the function runs, the frame is at its return address, as a caller's frame is
        // during a call: a stack trace taken in a callback shows the line of the call
        // (`docs/spec/diagnostics.md` chapter 8). A failure is raised at the instruction.
        self.set_top_pc(next);
        let result = {
            let mut call = ExtCall {
                iso: self,
                base: ra as usize,
                index,
                entry,
            };
            let mut ctx = CallCtx::new(&mut call);
            match body {
                Body::Sync(f) => f(&mut ctx).map(Ok),
                Body::Async(f) => f(&mut ctx).map(Err),
            }
        };
        self.set_top_pc(pc);
        let step = self.ext_returned(result, index, ra, next);
        // The references Rust let go of, the values it gave back included
        if let Err(e) = self.drain_ext_queue() {
            return Step::Fatal(e.message);
        }
        step
    }

    fn set_top_pc(&mut self, pc: u32) {
        if let Some(fr) = self.task.frames.last_mut() {
            fr.pc = pc;
        }
    }

    /// The step after the extension function `index` returned `result` (a value, or the future
    /// of a suspendable function).
    fn ext_returned(
        &mut self,
        result: Result<Result<Value, puy_ext::ExtFuture>, PasError>,
        index: usize,
        ra: u32,
        next: u32,
    ) -> Step {
        if let Some(m) = self.ext.fatal.take() {
            return Step::Fatal(m);
        }
        if std::mem::take(&mut self.ext.halting) {
            // A callback ran `Halt`: it goes on from this call, whatever the function returned
            return Step::Halt(None);
        }
        let unit = self.module.exts[index].unit;
        match result {
            Err(e) => self.ext_failure(e, Some(unit)),
            Ok(Ok(v)) => self.ext_done(v, index, ra, next),
            Ok(Err(mut fut)) => {
                let waker = noop_waker();
                let mut cx = Context::from_waker(&waker);
                match fut.as_mut().poll(&mut cx) {
                    Poll::Ready(Ok(v)) => self.ext_done(v, index, ra, next),
                    Poll::Ready(Err(e)) => self.ext_failure(e, Some(unit)),
                    Poll::Pending if self.ext.depth > 0 => {
                        Step::Error(rt(exc::INVALID_OPERATION, WAIT_IN_CALLBACK))
                    }
                    Poll::Pending => {
                        self.task.pending = Some(Pending {
                            fut,
                            deliver: Deliver::Ext {
                                reg: ra,
                                index: index as u32,
                            },
                            resume_pc: next,
                        });
                        Step::Waiting
                    }
                }
            }
        }
    }

    /// An extension function `index` gave back `v`: it goes to register `ra` and the frame goes
    /// on at `next`.
    fn ext_done(&mut self, v: Value, index: usize, ra: u32, next: u32) -> Step {
        match self.deliver_ext(v, index, ra) {
            Ok(()) => {
                self.set_top_pc(next);
                Step::Continue
            }
            Err(ValueFail::Rt(e)) => Step::Error(e),
            Err(ValueFail::Fatal(m)) => Step::Fatal(m),
        }
    }

    /// Writes the result `v` of the extension function `index` to register `ra`, as `Ret` writes
    /// a result: the register's old value is the borrowed first argument, which is not let go
    /// of (`docs/spec/extensions.md` 5.1).
    pub(super) fn deliver_ext(&mut self, v: Value, index: usize, ra: u32) -> Result<(), ValueFail> {
        let module = Arc::clone(&self.module);
        let e = &module.exts[index];
        if e.ret == ext_kind::NONE {
            return Ok(());
        }
        let slot = match e.ret_shape() {
            Some(shape) => self.ext_shape_slot(v, shape, &e.name)?,
            None => self.ext_value_slot(v, e.ret, e.ret_type, e.ret_range, &e.name)?,
        };
        self.task.regs[ra as usize] = slot;
        Ok(())
    }

    /// The Pascal value of the kind `kind` (of the type `ty` for a new object or a `TBytes`) of a
    /// value an extension gave, holding a reference of its own when managed.
    fn ext_value_slot(
        &mut self,
        v: Value,
        kind: u8,
        ty: u32,
        range: (i64, i64),
        name: &str,
    ) -> Result<Slot, ValueFail> {
        let mismatch = |v: &Value| {
            ValueFail::Fatal(format!(
                "extension function `{name}` gave {} for a value of kind `{}`",
                value_text(v),
                ext_kind::name(kind)
            ))
        };
        let oom = || ValueFail::Rt(oom());
        match (kind, v) {
            // A value outside the declared type cannot be made in Pascal (chapter 3)
            (ext_kind::INT, Value::Int(i)) if i < range.0 || i > range.1 => {
                Err(ValueFail::Fatal(format!(
                    "extension function `{name}` gave {i}, outside the range {}..{} of its type",
                    range.0, range.1
                )))
            }
            (ext_kind::INT, Value::Int(i)) => Ok(i),
            (ext_kind::SINGLE, Value::Float(f)) => Ok(f32_slot(f as f32)),
            (ext_kind::SINGLE, Value::Int(i)) => Ok(f32_slot(i as f32)),
            (ext_kind::DOUBLE, Value::Float(f)) => Ok(f64_slot(f)),
            (ext_kind::DOUBLE, Value::Int(i)) => Ok(f64_slot(i as f64)),
            (ext_kind::STRING | ext_kind::UTF8 | ext_kind::BYTES, Value::Nil) => Ok(0),
            (ext_kind::OBJECT | ext_kind::CALLABLE, Value::Nil) => Ok(0),
            (ext_kind::STRING, Value::Str(s)) => {
                if s.encode_utf16().count() > MAX_STRING_LEN {
                    return Err(oom());
                }
                let p = PStr::from_text(&s).map_err(|_| oom())?;
                self.heap.alloc_str(p).map_err(|_| oom())
            }
            (ext_kind::STRING, Value::PasString(s)) => {
                let p = match s.into_parts() {
                    Ok(ascii) => PStr::from_text(&ascii),
                    Err(units) => PStr::from_units(&units),
                }
                .map_err(|_| oom())?;
                self.heap.alloc_str(p).map_err(|_| oom())
            }
            (ext_kind::UTF8, Value::Str(s)) => {
                self.heap.alloc_u8(s.into_bytes()).map_err(|_| oom())
            }
            (ext_kind::UTF8, Value::PasString(s)) => self
                .heap
                .alloc_u8(s.as_pas_str().to_string().into_bytes())
                .map_err(|_| oom()),
            // The buffer becomes the array's, without a copy
            (ext_kind::BYTES, Value::Bytes(b)) => {
                self.heap.alloc_bytes_owned(ty, b).map_err(|_| oom())
            }
            (ext_kind::OBJECT, Value::Object(value)) => {
                let Some((_, want)) = self.ext.classes.iter().find(|(t, _)| *t == ty) else {
                    return Err(ValueFail::Fatal(format!(
                        "extension function `{name}` gave a new object for a result that is not an extension class"
                    )));
                };
                if (*value).type_id() != *want {
                    return Err(ValueFail::Fatal(format!(
                        "extension function `{name}` gave an object of another Rust type than its class holds"
                    )));
                }
                let h = match self.heap.alloc_obj(ty) {
                    Ok(Ok(h)) => h,
                    Ok(Err(_)) => return Err(oom()),
                    Err(e) => return Err(ValueFail::Fatal(e.message)),
                };
                self.heap
                    .set_obj_ext(h, value)
                    .map_err(|e| ValueFail::Fatal(format!("{e:?}")))?;
                Ok(h)
            }
            (ext_kind::STRING | ext_kind::UTF8 | ext_kind::BYTES, Value::Ref(r))
            | (ext_kind::OBJECT | ext_kind::CALLABLE, Value::Ref(r)) => {
                let (raw, rk, _) = r.raw();
                let want = if kind == ext_kind::OBJECT {
                    RefKind::Object
                } else {
                    RefKind::Block
                };
                // An instance of the declared class (or a descendant), and a block for any other
                // kind (chapter 3)
                let fits = rk == want
                    && (kind != ext_kind::OBJECT
                        || ty == bytecode::NONE_INDEX
                        || self
                            .heap
                            .obj_class(raw)
                            .is_ok_and(|c| self.heap.class_is(c, ty)));
                if !fits {
                    return Err(ValueFail::Fatal(format!(
                        "extension function `{name}` gave a value that is not of its declared type"
                    )));
                }
                self.heap
                    .add_ref(raw)
                    .map_err(|e| ValueFail::Fatal(e.message))?;
                Ok(raw)
            }
            (_, v) => Err(mismatch(&v)),
        }
    }

    /// The value of shape `s` in the slot `v`, copied for an extension (`docs/spec/extensions.md`
    /// 3): a record or a dynamic array with the values of its fields or elements, an ordinal, a
    /// real, a string (in the VM's form) or a `TBytes`. 0 reads as the default value.
    fn ext_read(&self, v: Slot, s: &ExtShape) -> Result<Value, heap::BadRef> {
        Ok(match s.kind {
            ext_kind::INT => Value::Int(v),
            ext_kind::SINGLE => Value::Float(f64::from(slot_f32(v))),
            ext_kind::DOUBLE => Value::Float(slot_f64(v)),
            ext_kind::STRING => Value::PasString(self.heap.get(v)?.ext_view().to_pas_string()),
            ext_kind::UTF8 if v == 0 => Value::Str(String::new()),
            // A `UTF8String` holds valid UTF-8 (`docs/spec/types.md` 8.5)
            ext_kind::UTF8 => {
                Value::Str(String::from_utf8_lossy(self.heap.get_u8(v)?).into_owned())
            }
            ext_kind::BYTES => Value::Bytes(self.heap.bytes_of(v)?.to_vec()),
            ext_kind::RECORD if v == 0 => {
                let n = self.heap.type_entry(s.ty).map_or(0, |t| t.slots as usize);
                self.ext_read_fields(&vec![0; n], 0, s)?
            }
            ext_kind::RECORD => {
                let (_, slots) = self.heap.any_slots(v)?;
                self.ext_read_fields(slots, 0, s)?
            }
            ext_kind::ARRAY => {
                let slots = self.heap.elems(v)?;
                let item = &s.items[0];
                let w = item.width(&self.module.types);
                let items = slots
                    .chunks_exact(w)
                    .map(|e| self.ext_read_in(e, 0, item))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::Array(items)
            }
            _ => {
                return Err(heap::BadRef {
                    message: format!("a value of kind `{}` is not copied", ext_kind::name(s.kind)),
                });
            }
        })
    }

    /// The fields of the record of shape `s` laid out from slot `at` of `slots`.
    fn ext_read_fields(
        &self,
        slots: &[Slot],
        at: usize,
        s: &ExtShape,
    ) -> Result<Value, heap::BadRef> {
        let mut k = at;
        let mut fields = Vec::with_capacity(s.items.len());
        for item in &s.items {
            fields.push(self.ext_read_in(slots, k, item)?);
            k += item.width(&self.module.types);
        }
        Ok(Value::Record(fields))
    }

    /// The value of shape `s` at slot `at` of `slots` (a record laid out flat there, or the value
    /// of the slot).
    fn ext_read_in(&self, slots: &[Slot], at: usize, s: &ExtShape) -> Result<Value, heap::BadRef> {
        if s.flat_slots(&self.module.types).is_some() {
            return self.ext_read_fields(slots, at, s);
        }
        let v = *slots.get(at).ok_or_else(|| heap::BadRef {
            message: format!("slot {at} of a value for an extension is out of range"),
        })?;
        self.ext_read(v, s)
    }

    /// The Pascal value of shape `s` of a value an extension gave (a record or a dynamic array
    /// made of its fields or elements, or a value of another kind), holding a reference of its
    /// own when managed (`docs/spec/extensions.md` 3).
    fn ext_shape_slot(&mut self, v: Value, s: &ExtShape, name: &str) -> Result<Slot, ValueFail> {
        let oom = || ValueFail::Rt(oom());
        let mismatch = |what: &str| {
            ValueFail::Fatal(format!(
                "extension function `{name}` gave {what} for a value of kind `{}`",
                ext_kind::name(s.kind)
            ))
        };
        match (s.kind, v) {
            (ext_kind::RECORD, Value::Record(fields)) => {
                Self::ext_field_count(&fields, s, name)?;
                let b = match self.heap.alloc_default(s.ty) {
                    Ok(Ok(b)) => b,
                    Ok(Err(_)) => return Err(oom()),
                    Err(e) => return Err(ValueFail::Fatal(e.message)),
                };
                let r = self.ext_fill_fields(b, 0, fields, s, name);
                self.release_on_failure(b, r)?;
                Ok(b)
            }
            (ext_kind::ARRAY, Value::Nil) => Ok(0),
            (ext_kind::ARRAY, Value::Array(items)) => {
                if items.is_empty() {
                    return Ok(0);
                }
                let b = self.heap.alloc_dyn(s.ty, items.len()).map_err(|_| oom())?;
                let item = &s.items[0];
                let w = item.width(&self.module.types);
                for (i, v) in items.into_iter().enumerate() {
                    let r = self.ext_fill(b, i * w, v, item, name);
                    self.release_on_failure(b, r)?;
                }
                Ok(b)
            }
            // A `Vec<u8>` inside another value may come element by element
            (ext_kind::BYTES, Value::Array(items)) => {
                let bytes = items
                    .iter()
                    .map(|v| v.as_int().and_then(|i| u8::try_from(i).ok()))
                    .collect::<Option<Vec<u8>>>()
                    .ok_or_else(|| mismatch("a value that is not a byte"))?;
                self.ext_value_slot(Value::Bytes(bytes), s.kind, s.ty, s.range, name)
            }
            (ext_kind::RECORD | ext_kind::ARRAY, v) => Err(mismatch(&value_text(&v))),
            (k, v) => self.ext_value_slot(v, k, s.ty, s.range, name),
        }
    }

    /// `r`, the result of filling the new block `b`: on a failure, lets go of `b` with what was
    /// written in it.
    fn release_on_failure(&mut self, b: Slot, r: Result<(), ValueFail>) -> Result<(), ValueFail> {
        if r.is_err()
            && let Err(e) = self.heap.release(b)
        {
            return Err(ValueFail::Fatal(e.message));
        }
        r
    }

    /// Whether an extension gave a record of as many fields as the shape `s` has.
    fn ext_field_count(fields: &[Value], s: &ExtShape, name: &str) -> Result<(), ValueFail> {
        if fields.len() == s.items.len() {
            return Ok(());
        }
        Err(ValueFail::Fatal(format!(
            "extension function `{name}` gave a record of {} fields for one of {}",
            fields.len(),
            s.items.len()
        )))
    }

    /// Writes the fields of a record of shape `s` from slot `at` of the new block `b`.
    fn ext_fill_fields(
        &mut self,
        b: Slot,
        at: usize,
        fields: Vec<Value>,
        s: &ExtShape,
        name: &str,
    ) -> Result<(), ValueFail> {
        let mut k = at;
        for (f, item) in fields.into_iter().zip(&s.items) {
            self.ext_fill(b, k, f, item, name)?;
            k += item.width(&self.module.types);
        }
        Ok(())
    }

    /// Writes the value `v` of shape `s` at slot `at` of the new block `b`: the fields of a
    /// record laid out flat, or one slot holding a value of its own.
    fn ext_fill(
        &mut self,
        b: Slot,
        at: usize,
        v: Value,
        s: &ExtShape,
        name: &str,
    ) -> Result<(), ValueFail> {
        if s.flat_slots(&self.module.types).is_some() {
            let Value::Record(fields) = v else {
                return Err(ValueFail::Fatal(format!(
                    "extension function `{name}` gave {} for a record",
                    value_text(&v)
                )));
            };
            Self::ext_field_count(&fields, s, name)?;
            return self.ext_fill_fields(b, at, fields, s, name);
        }
        let slot = self.ext_shape_slot(v, s, name)?;
        // The block is new: the slot is in it, and holds 0
        let p = self
            .heap
            .any_slot_mut(b, at)
            .map_err(|e| ValueFail::Fatal(e.message))?;
        *p = slot;
        Ok(())
    }

    /// The failure `e` of an extension function of the extension unit `unit` (`None`: a host
    /// call) as the step that raises it (`docs/spec/extensions.md` 5.3).
    pub(super) fn ext_failure(&mut self, e: PasError, unit: Option<u32>) -> Step {
        match self.pas_error(e, unit) {
            Ok(Ok(rt)) => Step::Error(rt),
            Ok(Err(x)) => Step::Throw(x),
            Err(None) => Step::Halt(None),
            Err(Some(m)) => Step::Fatal(m),
        }
    }

    /// The exception of a failure: a runtime error to raise (`Ok(Ok)`), an exception object to
    /// raise again with its reference (`Ok(Err)`), `Halt` (`Err(None)`), or an internal error.
    pub(super) fn pas_error(
        &mut self,
        e: PasError,
        unit: Option<u32>,
    ) -> Result<Result<RtError, Slot>, Option<String>> {
        match e {
            PasError::Raise {
                class,
                message,
                extra,
            } => {
                let Some(ty) = self.exception_class(&class, unit) else {
                    return Err(Some(format!(
                        "an extension raised `{class}`, which is not an exception class it can see"
                    )));
                };
                let extra = match extra {
                    ErrorExtra::Int(n) if self.heap.class_is(ty, exc::IN_OUT_ERROR) => {
                        Extra::Int(n)
                    }
                    ErrorExtra::Str(s) if self.heap.class_is(ty, exc::CONVERT_ERROR) => {
                        Extra::Str(s)
                    }
                    ErrorExtra::Host { code, kind }
                        if self.heap.class_is(ty, exc::IN_OUT_ERROR)
                            || self.heap.class_is(ty, exc::NOT_SUPPORTED) =>
                    {
                        Extra::Host { code, kind }
                    }
                    _ => Extra::None,
                };
                Ok(Ok(RtError {
                    class: ty,
                    message,
                    extra,
                }))
            }
            PasError::Pascal(r) => {
                // Only an exception object is raised again (5.3)
                let (x, rk, _) = r.raw();
                let is_exception = rk == RefKind::Object
                    && self
                        .heap
                        .obj_class(x)
                        .is_ok_and(|c| self.heap.class_is(c, exc::EXCEPTION));
                if !is_exception {
                    return Err(Some(
                        "an extension raised a value that is not an exception object".into(),
                    ));
                }
                self.heap.add_ref(x).map_err(|e| Some(e.message))?;
                Ok(Err(x))
            }
            PasError::Halt => Err(None),
        }
    }

    /// The exception class an extension names: a class of its unit, or a built-in exception
    /// class (`docs/spec/extensions.md` 5.3).
    fn exception_class(&self, name: &str, unit: Option<u32>) -> Option<u32> {
        let builtin = bytecode::builtin_types()
            .iter()
            .position(|t| t.kind == bytecode::TYPE_CLASS && t.name.eq_ignore_ascii_case(name))
            .map(|i| i as u32);
        let own = unit.and_then(|u| {
            let full = format!("{}.{name}", self.module.ext_units.get(u as usize)?.name);
            self.module
                .types
                .iter()
                .position(|t| t.kind == bytecode::TYPE_CLASS && t.name.eq_ignore_ascii_case(&full))
                .map(|i| bytecode::FIRST_TYPE_INDEX + i as u32)
        });
        own.or(builtin)
            .filter(|ty| self.heap.class_is(*ty, exc::EXCEPTION))
    }

    /// Lets go of the references Rust let go of (`docs/design/extensions.md` 3.9).
    pub(super) fn drain_ext_queue(&mut self) -> Result<(), heap::BadRef> {
        while !self.ext.queue.is_empty() {
            for (r, _) in self.ext.queue.take() {
                self.heap.release(r)?;
            }
        }
        Ok(())
    }

    /// Calls the Pascal callable value `f` with `args` from inside an extension function
    /// (`docs/spec/extensions.md` 5.2): pushes a boundary frame, enters the callable above it,
    /// and runs the execution loop until it returns to the boundary.
    fn call_back(&mut self, f: &PasRef, args: Vec<Value>) -> Result<Value, PasError> {
        if self.ext.halting {
            // A callback ran `Halt`: no more Pascal runs before the call returns (5.2)
            return Err(PasError::Halt);
        }
        let (v, _, tag) = f.raw();
        let module = Arc::clone(&self.module);
        let Some(sig) = module
            .exts
            .get((tag >> 8) as usize)
            .and_then(|e| e.sigs.get((tag & 0xFF) as usize))
        else {
            return Err(PasError::invalid_operation(
                "the value is not a callable argument of an extension function",
            ));
        };
        if self.ext.depth >= MAX_CALLBACK_DEPTH {
            return Err(PasError::new(
                "EStackOverflow",
                format!("callbacks from extensions nested more than {MAX_CALLBACK_DEPTH} deep"),
            ));
        }
        if args.len() != sig.params.len() {
            return Err(PasError::argument(format!(
                "the callback takes {} arguments, {} given",
                sig.params.len(),
                args.len()
            )));
        }
        // The boundary frame after the top frame's registers: its first register takes the
        // result (and the closure or `Self`), the arguments follow
        let n = args.len();
        let base = self.frame_top();
        if let Err(e) = self.grow_regs(base + 1 + n) {
            return Err(self.rt_to_pas(e));
        }
        let mut owned = Vec::with_capacity(n);
        for (i, ((a, k), r)) in args
            .into_iter()
            .zip(&sig.params)
            .zip(&sig.ranges)
            .enumerate()
        {
            match self.ext_value_slot(a, *k, bytecode::NONE_INDEX, *r, "a callback") {
                Ok(s) => {
                    self.task.regs[base + 1 + i] = s;
                    if matches!(*k, ext_kind::STRING | ext_kind::UTF8 | ext_kind::OBJECT) {
                        owned.push(s);
                    }
                }
                Err(e) => {
                    self.release_slots(&owned);
                    return Err(self.value_fail_to_pas(e));
                }
            }
        }
        self.task.regs[base] = 0;
        let depth = self.task.frames.len();
        self.task.frames.push(Frame {
            func: CALLBACK_FN,
            base: base as u32,
            pc: 0,
            ret_reg: (1 + n) as u32,
        });
        self.ext.depth += 1;
        let entered = self.enter_callable(v, base, n);
        let ended = match entered {
            Step::Continue => self.run_nested(depth),
            step => self.nested_step(step, depth),
        };
        self.ext.depth -= 1;
        let result = std::mem::take(&mut self.task.regs[base]);
        // The boundary frame is on top again (or the run ended with an internal error)
        if self.task.frames.len() > depth {
            self.task.frames.truncate(depth);
        }
        self.release_slots(&owned);
        if let Err(m) = ended {
            self.ext.fatal.borrow_mut().get_or_insert(m);
            return Err(PasError::Halt);
        }
        match self.ext.exit.take() {
            Some(Exit::Exception(x)) => Err(PasError::Pascal(PasRef::from_raw(
                x,
                RefKind::Object,
                0,
                Rc::clone(&self.ext.queue),
            ))),
            Some(Exit::Halt) => {
                self.ext.halting = true;
                Err(PasError::Halt)
            }
            None => self.callback_result(result, sig),
        }
    }

    /// The number of callbacks from extensions running.
    pub(super) fn ext_depth(&self) -> u32 {
        self.ext.depth
    }

    /// The first register after the top frame's (`docs/design/vm.md` 3.14: past its window).
    fn frame_top(&self) -> usize {
        self.task.frames.last().map_or(0, |fr| {
            let regs = self.frame_regs(fr);
            super::frame_end(fr.base as usize, regs)
        })
    }

    /// The number of registers of a frame: its function's, or those of the destruction
    /// procedure or of a callback's boundary.
    pub(super) fn frame_regs(&self, fr: &Frame) -> usize {
        match fr.func {
            super::DTOR_FN => super::destroy::DTOR_REGS,
            CALLBACK_FN => fr.ret_reg as usize,
            f => self.module.funcs[f as usize].regs as usize,
        }
    }

    /// Lets go of the values a callback's boundary frame held.
    fn release_slots(&mut self, slots: &[Slot]) {
        for s in slots {
            if let Err(e) = self.heap.release(*s) {
                self.ext.fatal.borrow_mut().get_or_insert(e.message);
            }
        }
    }

    /// Enters the callable value `v` above the boundary frame at `base` with the `n` arguments
    /// from `base + 1` (as `CallClosure` does; `docs/spec/objects.md` 14.9).
    fn enter_callable(&mut self, v: Slot, base: usize, n: usize) -> Step {
        if v == 0 {
            return Step::Error(rt(exc::ACCESS_VIOLATION, super::NIL_ACCESS));
        }
        let c = match self.heap.callable(v) {
            Ok(c) => c,
            Err(e) => return Step::Fatal(e.message),
        };
        let module = Arc::clone(&self.module);
        let params = |f: u32| module.funcs.get(f as usize).map(|f| f.params as usize);
        let mut budget = u32::MAX;
        let (args, counted) = match c.kind {
            heap::CallableKind::Closure => {
                self.task.regs[base] = v;
                (base, false)
            }
            heap::CallableKind::Routine => (base + 1, false),
            heap::CallableKind::Method | heap::CallableKind::ClassMethod => {
                if c.kind == heap::CallableKind::ClassMethod {
                    self.task.regs[base] = c.target;
                    (base, false)
                } else {
                    let h = match self.heap.weak_target(c.target) {
                        Ok(h) => h,
                        Err(e) => return super::RefError::from(e).into_step(),
                    };
                    if h == 0 {
                        return Step::Error(rt(exc::ACCESS_VIOLATION, super::FREED_ACCESS));
                    }
                    let counted = match self.heap.obj_counted(h) {
                        Ok(k) => k,
                        Err(e) => return super::RefError::from(e).into_step(),
                    };
                    if counted && let Err(e) = self.heap.add_ref(h) {
                        return Step::Fatal(e.message);
                    }
                    self.task.regs[base] = h;
                    (base, counted)
                }
            }
        };
        // The function takes the arguments laid out, after the closure or `Self` if it takes one
        let lead = usize::from(args == base);
        if c.func >= bytecode::VMT_BUILTIN || params(c.func) != Some(lead + n) {
            if counted {
                let s = std::mem::take(&mut self.task.regs[base]);
                if let Err(e) = self.heap.release(s) {
                    return Step::Fatal(e.message);
                }
            }
            return Step::Error(rt(
                exc::INVALID_OPERATION,
                "the callback does not take the arguments of its signature",
            ));
        }
        self.enter_call(c.func, args, base as u32, counted, 0, &mut budget)
    }

    /// Runs the execution loop of a callback until it returns to the boundary frame at frame
    /// depth `depth` (`Err`: an internal error).
    fn run_nested(&mut self, depth: usize) -> Result<(), String> {
        loop {
            // The budget does not apply inside a callback: it cannot give control back
            let mut budget = u32::MAX;
            let step = self.run_loop(&mut budget);
            match self.nested_step(step, depth) {
                Ok(()) if self.at_boundary(depth) => return Ok(()),
                Ok(()) => {}
                Err(m) => return Err(m),
            }
        }
    }

    /// Whether the boundary frame of the callback at `depth` is on top, with no job left for it.
    fn at_boundary(&self, depth: usize) -> bool {
        self.task.frames.len() == depth + 1
            && self
                .heap
                .top_job()
                .is_none_or(|(_, d)| (d as usize) < depth + 1)
    }

    /// Handles a step of a callback's loop as `run` does, the boundary taking the place of the
    /// end of the program.
    fn nested_step(&mut self, step: Step, depth: usize) -> Result<(), String> {
        let done = match step {
            Step::Continue | Step::CallbackDone => None,
            Step::Error(e) => match self.exception_of(e) {
                Ok(x) => self.throw_after_releases(x),
                Err(b) => return Err(b.message),
            },
            Step::Throw(x) => self.throw_after_releases(x),
            Step::Halt(from) => {
                if self.heap.job_open() {
                    let pc = self.task.frames.last().map_or(0, |f| f.pc);
                    self.stamp_instr(pc);
                }
                self.halt(from)
            }
            Step::Fatal(m) => return Err(m),
            Step::Waiting => return Err(WAIT_IN_CALLBACK.to_string()),
            Step::Finished(_) | Step::Done(_) => {
                return Err("the program ended inside a callback".to_string());
            }
        };
        match done {
            None => {}
            Some(super::RunOutcome::Fatal(e)) => return Err(e.message),
            Some(_) => return Err("the program ended inside a callback".to_string()),
        }
        if self.task.frames.len() <= depth {
            return Err("a callback left its boundary frame".to_string());
        }
        Ok(())
    }

    /// The boundary frame of a callback is reached by the unwinding of the exception `x`
    /// (`Unwound::Caught`: the callback's call fails with it).
    pub(super) fn callback_unwound(&mut self, x: Slot) -> Unwound {
        self.ext.exit = Some(Exit::Exception(x));
        Unwound::Caught
    }

    /// The boundary frame of a callback is reached by `Halt`: the extension function's call goes
    /// on with it once it returns.
    pub(super) fn callback_halted(&mut self) {
        self.ext.exit = Some(Exit::Halt);
    }

    /// The result of a callback in its register, as the value of its signature's kind (a
    /// managed value's reference moves to the value, or is let go of once copied).
    fn callback_result(&mut self, r: Slot, sig: &ExtSig) -> Result<Value, PasError> {
        let v = match sig.ret {
            ext_kind::NONE => Value::Nothing,
            ext_kind::INT => Value::Int(r),
            ext_kind::SINGLE => Value::Float(f64::from(slot_f32(r))),
            ext_kind::DOUBLE => Value::Float(slot_f64(r)),
            ext_kind::STRING => {
                let units = match self.heap.get(r).and_then(|s| {
                    s.units().map_err(|_| heap::BadRef {
                        message: "out of memory".into(),
                    })
                }) {
                    Ok(u) => u,
                    Err(e) => return Err(self.fatal_pas(e.message)),
                };
                self.release_slots(&[r]);
                Value::PasString(puy_ext::PasString::from_utf16(units))
            }
            ext_kind::UTF8 => {
                let text = match self.heap.get_u8(r) {
                    Ok(b) => String::from_utf8_lossy(b).into_owned(),
                    Err(e) => return Err(self.fatal_pas(e.message)),
                };
                self.release_slots(&[r]);
                Value::Str(text)
            }
            ext_kind::OBJECT if r == 0 => Value::Nil,
            ext_kind::OBJECT => Value::Ref(PasRef::from_raw(
                r,
                RefKind::Object,
                0,
                Rc::clone(&self.ext.queue),
            )),
            _ => return Err(self.fatal_pas("a callback of an unknown result kind".into())),
        };
        Ok(v)
    }

    /// An internal error inside an extension's call: the call ends with it.
    fn fatal_pas(&mut self, message: String) -> PasError {
        self.ext.fatal.borrow_mut().get_or_insert(message);
        PasError::Halt
    }

    /// A runtime error as the failure of a call back into Pascal: the exception object, made as
    /// the VM makes those it raises.
    fn rt_to_pas(&mut self, e: RtError) -> PasError {
        match self.exception_of(e) {
            Ok(x) => PasError::Pascal(PasRef::from_raw(
                x,
                RefKind::Object,
                0,
                Rc::clone(&self.ext.queue),
            )),
            Err(b) => self.fatal_pas(b.message),
        }
    }

    fn value_fail_to_pas(&mut self, e: ValueFail) -> PasError {
        match e {
            ValueFail::Rt(e) => self.rt_to_pas(e),
            ValueFail::Fatal(m) => self.fatal_pas(m),
        }
    }
}
