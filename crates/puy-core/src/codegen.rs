//! Code generation (IR → bytecode; `docs/spec/bytecode.md`, `docs/design/lifetime.md` 3.10).
//!
//! Register allocation: a named variable `vN` lives in register `rN` (parameters, `Result`, then
//! locals; one register per variable, the M1 rule). Owning temporaries of managed types follow, one
//! register each in creation order (`docs/design/lifetime.md` 3.4), and the other temporaries come
//! last, reused like a stack by their slot (`ir::Temp::slot`), so a call only overwrites them.
//! Blocks are laid out in order; a jump to the next block is dropped.

use crate::bytecode::{
    self, ConstEntry, FIRST_TYPE_INDEX, Function, GlobalEntry, Module, TYPE_ARRAY, TYPE_DYNARRAY,
    TYPE_ENUM, TYPE_EXCEPTION, TYPE_RECORD, TYPE_SET, TypeEntry, UnitEntry, op,
};
use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Stage};
use crate::ir::{self, BlockId, InstKind, Operand, Reg, Term};
use crate::sema::hir::VarKind;
use crate::sema::types::{Const, Ty, TypeTable};
use crate::source::{FileId, Span};

/// Maximum number of registers (1.2).
const MAX_REGS: u32 = 65535;
/// Maximum length of the instruction stream (in words; 1.3).
const MAX_WORDS: usize = i32::MAX as usize;

/// Generates the module. Returns `None` on an error (E3400).
pub fn generate(m: &ir::Module, sink: &mut DiagnosticSink) -> Option<Module> {
    let mut consts = ConstPool {
        entries: Vec::new(),
        types: Vec::new(),
        type_ids: Vec::new(),
        table: &m.types,
    };
    let mut failed = false;
    let mut funcs = Vec::with_capacity(m.funcs.len());
    for f in &m.funcs {
        let mut g = FuncGen::new(f, &mut consts);
        match g.generate() {
            Ok(func) => funcs.push(func),
            Err(d) => {
                sink.push(*d);
                failed = true;
            }
        }
    }
    if failed {
        return None;
    }
    Some(Module {
        consts: consts.entries,
        types: consts.types,
        funcs,
        globals: m
            .globals
            .iter()
            .map(|g| GlobalEntry {
                name: g.name.clone(),
                kind: if g.exit_code {
                    crate::bytecode::GLOBAL_EXIT_CODE
                } else if g.ty.is_managed() {
                    crate::bytecode::GLOBAL_REF
                } else {
                    crate::bytecode::GLOBAL_INT
                },
            })
            .collect(),
        entry: m.entry,
        units: m
            .units
            .iter()
            .map(|u| UnitEntry {
                name: u.name.clone(),
                path: u.path.clone(),
                init: u.init,
                fini: u.fini,
                first_global: u.first_global,
                global_count: u.global_count,
            })
            .collect(),
        hosts: m.hosts.clone(),
    })
}

/// The constant pool and the type section (entries are shared in order of first use).
struct ConstPool<'t> {
    entries: Vec<ConstEntry>,
    types: Vec<TypeEntry>,
    /// The type of each entry of `types`.
    type_ids: Vec<Ty>,
    table: &'t TypeTable,
}

impl ConstPool<'_> {
    /// The type index of a type: a built-in type's own index (below 64), or the type section
    /// entry (64 and up), adding it (and an exception class's parent first) to the section.
    fn ty(&mut self, ty: Ty) -> u32 {
        if let Some(id) = ty.type_id()
            && TypeTable::is_builtin(id)
        {
            return id.0;
        }
        if let Some(i) = self.type_ids.iter().position(|t| *t == ty) {
            return FIRST_TYPE_INDEX + i as u32;
        }
        let mut parent = None;
        let mut values = Vec::new();
        let (kind, slots, refs) = match ty {
            Ty::Enum(_) => {
                values = self
                    .table
                    .enum_values(ty)
                    .iter()
                    .map(|v| (v.ord as i64, v.name.clone()))
                    .collect();
                (TYPE_ENUM, 0, Vec::new())
            }
            Ty::Exception(id) => {
                let p = self
                    .table
                    .parent(id)
                    .expect("a declared exception class has a parent");
                parent = Some(self.ty(Ty::Exception(p)));
                let refs = self.table.exception_slots(id);
                (TYPE_EXCEPTION, refs.len() as u32, refs)
            }
            Ty::Record(_) => {
                let fields = self.table.fields(ty);
                let refs = fields.iter().map(|f| f.ty.is_managed()).collect();
                (TYPE_RECORD, fields.len() as u32, refs)
            }
            Ty::BigSet(_) => (TYPE_SET, self.table.set_bits(ty), Vec::new()),
            Ty::DynArray(_) | Ty::OpenArray(_) => {
                let elem = self
                    .table
                    .dyn_elem(ty)
                    .expect("a dynamic array type has an element type");
                (TYPE_DYNARRAY, 0, vec![elem.is_managed()])
            }
            _ => {
                let (_, lo, hi, elem) = self
                    .table
                    .array(ty)
                    .expect("a type operand is a record or a static array");
                (TYPE_ARRAY, (hi - lo + 1) as u32, vec![elem.is_managed()])
            }
        };
        let ty = match ty {
            Ty::OpenArray(id) => Ty::DynArray(id),
            t => t,
        };
        if let Some(i) = self.type_ids.iter().position(|t| *t == ty) {
            return FIRST_TYPE_INDEX + i as u32;
        }
        self.types.push(TypeEntry {
            kind,
            slots,
            refs,
            name: self.table.full_name(ty),
            parent,
            values,
        });
        self.type_ids.push(ty);
        FIRST_TYPE_INDEX + self.types.len() as u32 - 1
    }

    /// A constant of a slot of an aggregate, by its type: the slot's representation (`Char` in a
    /// `string` slot is a string).
    fn slot_const(&mut self, c: &Const, ty: Ty) -> u32 {
        match c {
            Const::Int(v) => {
                let v = if ty == Ty::UInt64 {
                    *v as u64 as i64
                } else {
                    *v as i64
                };
                self.int(v)
            }
            Const::Bool(b) => self.int(*b as i64),
            Const::Curr(v) => self.int(*v),
            // A `Single` slot holds the `f32` bits; the slot of a constant aggregate is its value
            Const::Real(b) if ty == Ty::Single => {
                self.int((f64::from_bits(*b) as f32).to_bits() as i64)
            }
            Const::Real(b) => self.add(ConstEntry::Float(*b)),
            Const::Char(ch) if ty == Ty::String => self.str(&[*ch]),
            Const::Char(ch) => self.int(*ch as i64),
            Const::Str(units) => self.str(units),
            Const::U8(s) => self.add(ConstEntry::U8(s.clone())),
            Const::Agg(_) => self.agg(ty, c),
            Const::Nil => self.int(0),
            Const::Set(words) if matches!(ty, Ty::BigSet(_)) => self.set(ty, words),
            Const::Set(words) => self.int(crate::sema::types::setbits::to_small(words)),
        }
    }

    /// A big set constant (kind 6).
    fn set(&mut self, ty: Ty, words: &[u64]) -> u32 {
        let t = self.ty(ty);
        self.add(ConstEntry::Set {
            ty: t,
            words: words.to_vec(),
        })
    }

    /// A record or static array constant: its elements first, then the aggregate (3.3).
    fn agg(&mut self, ty: Ty, c: &Const) -> u32 {
        let Const::Agg(items) = c else {
            unreachable!("an aggregate constant")
        };
        let slot_types: Vec<Ty> = match ty {
            Ty::Record(_) => self.table.fields(ty).iter().map(|f| f.ty).collect(),
            _ => {
                let elem = self.table.array(ty).map_or(Ty::Error, |a| a.3);
                vec![elem; items.len()]
            }
        };
        let ks: Vec<u32> = items
            .iter()
            .zip(slot_types)
            .map(|(v, t)| self.slot_const(v, t))
            .collect();
        let t = self.ty(ty);
        self.add(ConstEntry::Agg { ty: t, items: ks })
    }

    fn add(&mut self, entry: ConstEntry) -> u32 {
        if let Some(i) = self.entries.iter().position(|c| *c == entry) {
            return i as u32;
        }
        self.entries.push(entry);
        self.entries.len() as u32 - 1
    }

    fn int(&mut self, v: i64) -> u32 {
        self.add(ConstEntry::Int(v))
    }

    fn str(&mut self, units: &[u16]) -> u32 {
        self.add(match String::from_utf16(units) {
            Ok(s) => ConstEntry::Str(s),
            Err(_) => ConstEntry::Utf16(units.to_vec()),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum Instr {
    Plain {
        op: u8,
        a: u32,
        b: u32,
        c: u32,
    },
    /// A jump to the start of a block (`b` is the second register of a compare-and-branch).
    Jump {
        op: u8,
        a: u32,
        b: u32,
        label: BlockId,
    },
}

struct FuncGen<'a, 't> {
    f: &'a ir::Function,
    consts: &'a mut ConstPool<'t>,
    nvars: u32,
    /// Register of each temporary.
    temp_regs: Vec<u32>,
    instrs: Vec<(Instr, u32)>,
    /// Index into `instrs` of the start of each block.
    labels: Vec<Option<usize>>,
    /// Index into `instrs` of the `EndFinally` ending each block (if it does).
    end_finally: Vec<Option<usize>>,
    /// Line of the last instruction.
    line: u32,
    /// An operand that does not fit in 16 bits even with `Wide` (a field number or a constant
    /// index in `C`); reported as E3400.
    oversize: Option<&'static str>,
}

impl<'a, 't> FuncGen<'a, 't> {
    fn new(f: &'a ir::Function, consts: &'a mut ConstPool<'t>) -> Self {
        // The static link comes before the named variables (`docs/spec/semantics.md` 4.9)
        let nvars = f.vars.len() as u32 + u32::from(f.link);
        let owning = f.temps.iter().filter(|t| t.slot.is_none()).count() as u32;
        let mut next_owning = nvars;
        let temp_regs = f
            .temps
            .iter()
            .map(|t| match t.slot {
                Some(slot) => nvars + owning + slot,
                None => {
                    next_owning += 1;
                    next_owning - 1
                }
            })
            .collect();
        FuncGen {
            f,
            consts,
            nvars,
            temp_regs,
            instrs: Vec::new(),
            labels: vec![None; f.blocks.len()],
            end_finally: vec![None; f.blocks.len()],
            line: 1,
            oversize: None,
        }
    }

    /// The register of a virtual register.
    fn reg(&self, r: Reg) -> u32 {
        match r {
            Reg::Var(n) => n + u32::from(self.f.link),
            Reg::Temp(t) => self.temp_regs[t as usize],
            Reg::Link => 0,
        }
    }

    fn emit(&mut self, op: u8, a: u32, b: u32, c: u32) {
        self.instrs.push((Instr::Plain { op, a, b, c }, self.line));
    }

    fn emit_bx(&mut self, op: u8, a: u32, bx: u32) {
        self.emit(op, a, bx & 0xFF, bx >> 8);
    }

    fn emit_jump(&mut self, op: u8, a: u32, b: u32, label: BlockId) {
        self.instrs
            .push((Instr::Jump { op, a, b, label }, self.line));
    }

    /// An operand that `Wide` widens to 16 bits; a larger one is reported as E3400 at the end.
    fn small_operand(&mut self, v: u32, what: &'static str) -> u32 {
        if v > 0xFFFF {
            self.oversize.get_or_insert(what);
            0
        } else {
            v
        }
    }

    fn too_large(&self, what: &str) -> Box<Diagnostic> {
        let d = Diagnostic::error(
            Code::E(3400),
            format!("routine `{}` is too large: {what}", self.f.short_name),
            Stage::Codegen,
        )
        .with_primary(Span::new(FileId(0), self.f.name_span), "")
        .with_note("split the routine into smaller ones");
        Box::new(d)
    }

    fn generate(&mut self) -> Result<Function, Box<Diagnostic>> {
        let regs = self
            .temp_regs
            .iter()
            .map(|r| r + 1)
            .max()
            .unwrap_or(0)
            .max(self.nvars)
            .max(1);
        if regs > MAX_REGS + 1 {
            return Err(self.too_large("it needs more than 65535 registers"));
        }
        for (i, block) in self.f.blocks.iter().enumerate() {
            self.labels[i] = Some(self.instrs.len());
            self.block(i as BlockId, block);
        }
        if let Some(what) = self.oversize {
            return Err(self.too_large(what));
        }
        let (code, lines, positions) = self.layout();
        let handlers = self.handler_table(&positions);
        if code.len() > MAX_WORDS {
            return Err(self.too_large("its code exceeds 2147483647 words"));
        }
        let locals: Vec<(u16, String)> = self
            .f
            .vars
            .iter()
            .enumerate()
            .filter(|(_, v)| v.kind != VarKind::Hidden)
            .map(|(i, v)| (self.reg(Reg::Var(i as u32)) as u16, v.name.clone()))
            .collect();
        Ok(Function {
            name: self.f.name.clone(),
            params: self.f.params as u16,
            outer: self.f.outer,
            regs: regs as u16,
            code,
            lines,
            locals,
            managed: self.f.finally.iter().map(|r| self.reg(*r) as u16).collect(),
            file: self.f.file,
            handlers,
        })
    }

    /// The handler table (`docs/spec/exceptions.md` 6.3) from the IR's block ranges. `positions`
    /// holds the word position of each instruction and, last, the length of the code.
    fn handler_table(&self, positions: &[usize]) -> Vec<bytecode::HandlerEntry> {
        let at_block = |b: BlockId| -> u32 {
            let i = match self.labels.get(b as usize) {
                Some(l) => l.expect("every block is laid out"),
                None => self.instrs.len(),
            };
            positions[i] as u32
        };
        self.f
            .tries
            .iter()
            .map(|t| {
                let handler_end = if t.finally {
                    let i = self.end_finally[t.handler.1 as usize]
                        .expect("a `finally` block ends with `EndFinally`");
                    positions[i] as u32
                } else {
                    at_block(t.handler.1 + 1)
                };
                bytecode::HandlerEntry {
                    kind: if t.finally {
                        bytecode::HANDLER_FINALLY
                    } else {
                        bytecode::HANDLER_EXCEPT
                    },
                    body_start: at_block(t.body.0),
                    body_end: at_block(t.handler.0),
                    handler_start: at_block(t.handler.0),
                    handler_end,
                    reg: self.reg(t.reg) as u16,
                }
            })
            .collect()
    }

    fn block(&mut self, index: BlockId, block: &ir::Block) {
        for inst in &block.insts {
            self.line = inst.line;
            match &inst.kind {
                InstKind::Plain(operands) => self.plain(inst.op, operands),
                InstKind::Call {
                    dst,
                    callee,
                    args,
                    frame,
                } => {
                    let base = self.reg(*frame);
                    for (i, a) in args.iter().enumerate() {
                        assert_eq!(
                            self.reg(*a),
                            base + i as u32,
                            "lower lays out call arguments contiguously"
                        );
                    }
                    let x = match callee {
                        Operand::Func(f) | Operand::Host(f) => *f,
                        _ => unreachable!("a call names a function or a host function"),
                    };
                    self.emit_bx(inst.op, base, x);
                    if let Some(d) = dst {
                        let d = self.reg(*d);
                        if d != base {
                            self.emit(op::MOVE, d, base, 0);
                        }
                    }
                }
            }
        }
        self.line = block.term_line;
        let next = index + 1;
        match &block.term {
            Term::Jmp(t) => {
                if *t != next {
                    self.emit_jump(op::JMP, 0, 0, *t);
                }
            }
            Term::Branch {
                op: opc,
                a,
                b,
                then,
                other,
            } => {
                let a = self.reg(*a);
                let b = b.map_or(0, |b| self.reg(b));
                self.emit_jump(*opc, a, b, *then);
                if *other != next {
                    self.emit_jump(op::JMP, 0, 0, *other);
                }
            }
            Term::Ret(r) => {
                let r = self.reg(*r);
                self.emit(op::RET, r, 0, 0);
            }
            Term::RetVoid => self.emit(op::RET_VOID, 0, 0, 0),
            Term::Halt(r) => {
                let r = self.reg(*r);
                self.emit(op::HALT, r, 0, 0);
            }
            Term::Raise(r) => {
                let r = self.reg(*r);
                self.emit(op::RAISE, r, 0, 0);
            }
            Term::RaiseOuter(a, b) => {
                let (a, b) = (self.reg(*a), self.reg(*b));
                self.emit(op::RAISE_OUTER, a, b, 0);
            }
            Term::EndFinally {
                state,
                next: after,
                targets,
            } => {
                let a = self.reg(*state);
                self.end_finally[index as usize] = Some(self.instrs.len());
                self.emit(op::END_FINALLY, a, targets.len() as u32, 0);
                for t in targets {
                    self.emit_jump(op::JMP, 0, 0, *t);
                }
                if *after != next {
                    self.emit_jump(op::JMP, 0, 0, *after);
                }
            }
        }
    }

    /// Encodes a plain instruction's operands into its fields (by the instruction's shape).
    fn plain(&mut self, opc: u8, operands: &[Operand]) {
        let shape = op::info(opc)
            .expect("IR instructions use bytecode opcodes")
            .shape;
        let reg = |this: &Self, i: usize| match &operands[i] {
            Operand::Reg(r) => this.reg(*r),
            o => unreachable!("operand {i} of {opc} is a register, not {o:?}"),
        };
        let imm = |i: usize| match &operands[i] {
            Operand::Imm(v) => *v,
            o => unreachable!("operand {i} of {opc} is an immediate, not {o:?}"),
        };
        match shape {
            // `ReadLnEnd`, `FlushOut` (`docs/spec/bytecode.md` 2.17)
            op::Shape::None => self.emit(opc, 0, 0, 0),
            op::Shape::A => self.emit(opc, reg(self, 0), 0, 0),
            op::Shape::AB => self.emit(opc, reg(self, 0), reg(self, 1), 0),
            op::Shape::ABC => self.emit(opc, reg(self, 0), reg(self, 1), reg(self, 2)),
            op::Shape::ABCImm => self.emit(opc, reg(self, 0), reg(self, 1), imm(2) as u32),
            op::Shape::ABsCImm => {
                self.emit(opc, reg(self, 0), reg(self, 1), imm(2) as i8 as u8 as u32)
            }
            op::Shape::AOuter => self.emit(opc, reg(self, 0), imm(1) as u32, imm(2) as u32),
            op::Shape::ABRange2 | op::Shape::ABRange3 | op::Shape::ABRange4 => {
                let b = reg(self, 1);
                for i in 2..operands.len() {
                    assert_eq!(
                        reg(self, i),
                        b + i as u32 - 1,
                        "lower lays out a register range contiguously"
                    );
                }
                self.emit(opc, reg(self, 0), b, 0);
            }
            op::Shape::ASBxImm => {
                let v = imm(1) as i16 as u16 as u32;
                self.emit(opc, reg(self, 0), v & 0xFF, v >> 8);
            }
            op::Shape::ABSlot => {
                let slot = self.small_operand(imm(2) as u32, "a record has more than 65536 fields");
                self.emit(opc, reg(self, 0), reg(self, 1), slot)
            }
            op::Shape::ASlotC => {
                let slot = self.small_operand(imm(1) as u32, "a record has more than 65536 fields");
                self.emit(opc, reg(self, 0), slot, reg(self, 2))
            }
            op::Shape::ABk => {
                let k = match &operands[2] {
                    Operand::Str(units) => self.consts.str(units),
                    o => unreachable!("operand 2 of {opc} is a constant, not {o:?}"),
                };
                let k = self.small_operand(k, "the module has more than 65536 constants");
                self.emit(opc, reg(self, 0), reg(self, 1), k);
            }
            op::Shape::ABType => {
                let t = match &operands[2] {
                    Operand::Type(t) => self.consts.ty(*t),
                    o => unreachable!("operand 2 of {opc} is a type, not {o:?}"),
                };
                let t = self.small_operand(t, "the module has more than 65536 types");
                self.emit(opc, reg(self, 0), reg(self, 1), t);
            }
            op::Shape::ABPairs => {
                let b = reg(self, 1);
                let n = imm(2) as u32;
                assert_eq!(operands.len(), 3 + 2 * n as usize, "{opc} has {n} pairs");
                for i in 0..2 * n as usize {
                    assert_eq!(
                        reg(self, 3 + i),
                        b + 1 + i as u32,
                        "lower lays out the pairs right after the format"
                    );
                }
                let n = self.small_operand(n, "a `Format` call has more than 65536 arguments");
                self.emit(opc, reg(self, 0), b, n);
            }
            op::Shape::ABPath => {
                let a = reg(self, 0);
                let n = imm(2) as u32;
                for i in 0..n as usize {
                    assert_eq!(
                        reg(self, 3 + i),
                        a + 1 + i as u32,
                        "lower lays out a cell path right after the cell"
                    );
                }
                self.emit(opc, a, reg(self, 1), n);
            }
            op::Shape::ABx => {
                let x = match &operands[1] {
                    Operand::Int(v) => self.consts.int(*v),
                    Operand::Float(b) => self.consts.add(ConstEntry::Float(*b)),
                    Operand::Str(units) => self.consts.str(units),
                    Operand::U8(s) => self.consts.add(ConstEntry::U8(s.clone())),
                    Operand::Type(t) => self.consts.ty(*t),
                    Operand::Agg(t, c) => self.consts.slot_const(c, *t),
                    Operand::Global(i) | Operand::Func(i) | Operand::Host(i) => *i,
                    o => unreachable!("operand 1 of {opc} is a table index, not {o:?}"),
                };
                let a = reg(self, 0);
                // `SetLen` / `ArrCopy` read the registers after `rA` (listed after the type)
                for i in 2..operands.len() {
                    assert_eq!(
                        reg(self, i),
                        a + i as u32 - 1,
                        "lower lays out the operands of {opc} in consecutive registers"
                    );
                }
                self.emit_bx(opc, a, x);
            }
            op::Shape::ASBx | op::Shape::SBx | op::Shape::ABsC | op::Shape::AJumps => {
                unreachable!("{opc} is not a plain instruction")
            }
        }
    }

    // ---- Layout and encoding ----

    /// Decides the size of each jump and turns the instructions into words.
    fn layout(&self) -> (Vec<u32>, Vec<(u32, u32)>, Vec<usize>) {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Form {
            Short,
            Wide,
            /// The compare-and-branch cannot reach: skip over a `Jmp` with the inverted condition (inverted branch + Jmp)
            Expanded,
        }
        let n = self.instrs.len();
        let mut forms: Vec<Form> = self
            .instrs
            .iter()
            .map(|(i, _)| {
                if plain_needs_wide(i) {
                    Form::Wide
                } else {
                    Form::Short
                }
            })
            .collect();
        let size = |f: Form, i: &Instr| -> usize {
            match (f, i) {
                (Form::Short, _) => 1,
                (Form::Wide, _) => 3,
                (Form::Expanded, _) => 6,
            }
        };
        let mut positions = vec![0usize; n + 1];
        loop {
            let mut changed = false;
            let mut pos = 0usize;
            for (i, (ins, _)) in self.instrs.iter().enumerate() {
                positions[i] = pos;
                pos += size(forms[i], ins);
            }
            positions[n] = pos;
            for (i, (ins, _)) in self.instrs.iter().enumerate() {
                let Instr::Jump { op, a, b, label } = ins else {
                    continue;
                };
                let target = positions[self.labels[*label as usize].expect("unbound label")];
                let fused = op::info(*op).is_some_and(|i| i.shape == op::Shape::ABsC);
                let regs_wide = *a > 255 || *b > 255;
                let needed = match forms[i] {
                    Form::Expanded => Form::Expanded,
                    _ => {
                        let next = positions[i] + 1;
                        let next_wide = positions[i] + 3;
                        let off_short = target as i64 - next as i64;
                        let off_wide = target as i64 - next_wide as i64;
                        if fused {
                            if !regs_wide && (i8::MIN as i64..=i8::MAX as i64).contains(&off_short)
                            {
                                Form::Short
                            } else if (i16::MIN as i64..=i16::MAX as i64).contains(&off_wide) {
                                Form::Wide
                            } else {
                                Form::Expanded
                            }
                        } else if !regs_wide
                            && (i16::MIN as i64..=i16::MAX as i64).contains(&off_short)
                        {
                            Form::Short
                        } else {
                            Form::Wide
                        }
                    }
                };
                // Only ever grow (so that it converges)
                let rank = |f: Form| match f {
                    Form::Short => 0,
                    Form::Wide => 1,
                    Form::Expanded => 2,
                };
                if rank(needed) > rank(forms[i]) {
                    forms[i] = needed;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Encoding
        let mut code: Vec<u32> = Vec::with_capacity(positions[n]);
        let mut lines: Vec<(u32, u32)> = Vec::new();
        for (i, (ins, line)) in self.instrs.iter().enumerate() {
            if lines.last().is_none_or(|l| l.1 != *line) {
                lines.push((code.len() as u32, *line));
            }
            match ins {
                Instr::Plain { op, a, b, c } => {
                    if forms[i] == Form::Wide {
                        code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                        code.push(bytecode::encode(*op, *a, *b, *c));
                        // A table index continues with the upper 16 bits; a signed immediate is sign-extended from its original width.
                        let (ext_b, ext_c) = match op::info(*op).unwrap().shape {
                            op::Shape::ABx => {
                                let bx = b | (c << 8);
                                (bx >> 8, bx >> 16)
                            }
                            op::Shape::ASBxImm => {
                                let bx = (b | (c << 8)) as u16 as i16 as i32 as u32;
                                (bx >> 8, bx >> 16)
                            }
                            op::Shape::ABsCImm => (*b, *c as u8 as i8 as i16 as u16 as u32),
                            _ => (*b, *c),
                        };
                        code.push(bytecode::encode_ext(*a, ext_b, ext_c));
                    } else {
                        code.push(bytecode::encode(*op, *a, *b, *c));
                    }
                }
                Instr::Jump { op, a, b, label } => {
                    let target = positions[self.labels[*label as usize].unwrap()] as i64;
                    let fused = op::info(*op).is_some_and(|i| i.shape == op::Shape::ABsC);
                    match forms[i] {
                        Form::Short => {
                            let next = code.len() as i64 + 1;
                            let off = target - next;
                            if fused {
                                code.push(bytecode::encode(*op, *a, *b, (off as i8 as u8) as u32));
                            } else {
                                let o = off as i16 as u16 as u32;
                                code.push(bytecode::encode(*op, *a, o & 0xFF, o >> 8));
                            }
                        }
                        Form::Wide => {
                            let next = code.len() as i64 + 3;
                            let off = target - next;
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            if fused {
                                let o = off as i16 as u16 as u32;
                                code.push(bytecode::encode(*op, *a, *b, o & 0xFF));
                                code.push(bytecode::encode_ext(*a, *b, o));
                            } else {
                                let o = off as i32 as u32;
                                code.push(bytecode::encode(*op, *a, o & 0xFF, (o >> 8) & 0xFF));
                                code.push(bytecode::encode_ext(
                                    *a,
                                    (o >> 8) & 0xFFFF,
                                    (o >> 16) & 0xFFFF,
                                ));
                            }
                        }
                        Form::Expanded => {
                            // Skip over a 3-word (Wide) Jmp with the inverted condition; the inverted branch is also Wide, 3 words
                            let (inv, ia, ib) = invert_fused(*op, *a, *b);
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            let o = 3u32; // skip the next Jmp (3 words)
                            code.push(bytecode::encode(inv, ia, ib, o & 0xFF));
                            code.push(bytecode::encode_ext(ia, ib, o));
                            let next = code.len() as i64 + 3;
                            let off = (target - next) as i32 as u32;
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            code.push(bytecode::encode(op::JMP, 0, off & 0xFF, (off >> 8) & 0xFF));
                            code.push(bytecode::encode_ext(
                                0,
                                (off >> 8) & 0xFFFF,
                                (off >> 16) & 0xFFFF,
                            ));
                        }
                    }
                }
            }
        }
        if lines.is_empty() {
            lines.push((0, self.line));
        }
        (code, lines, positions)
    }
}

/// Whether an instruction's operand does not fit in 8 bits.
fn plain_needs_wide(i: &Instr) -> bool {
    match i {
        Instr::Plain { op: opc, a, b, c } => {
            let shape = op::info(*opc).map(|i| i.shape).unwrap_or(op::Shape::None);
            match shape {
                op::Shape::ABx => *a > 255 || (b | (c << 8)) > 0xFFFF,
                op::Shape::ASBxImm => *a > 255,
                op::Shape::ABsC | op::Shape::ABsCImm => *a > 255 || *b > 255,
                _ => *a > 255 || *b > 255 || *c > 255,
            }
        }
        Instr::Jump { .. } => false,
    }
}

/// The inverted condition of a compare-and-branch.
fn invert_fused(opc: u8, a: u32, b: u32) -> (u8, u32, u32) {
    match opc {
        op::JMP_IF_EQ_INT => (op::JMP_IF_NE_INT, a, b),
        op::JMP_IF_NE_INT => (op::JMP_IF_EQ_INT, a, b),
        op::JMP_IF_LT_INT => (op::JMP_IF_LE_INT, b, a),
        op::JMP_IF_LE_INT => (op::JMP_IF_LT_INT, b, a),
        op::JMP_IF_LT_UINT => (op::JMP_IF_LE_UINT, b, a),
        op::JMP_IF_LE_UINT => (op::JMP_IF_LT_UINT, b, a),
        op::JMP_IF_EQ_STR => (op::JMP_IF_NE_STR, a, b),
        op::JMP_IF_NE_STR => (op::JMP_IF_EQ_STR, a, b),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{CompileOptions, compile};
    use crate::source::MemorySourceProvider;
    use crate::source::TextRange;

    fn dump(src: &str) -> String {
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "Hello.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &CompileOptions::default(),
        );
        assert!(
            out.diagnostics.iter().all(|d| !d.is_error()),
            "{:#?}",
            out.diagnostics
        );
        crate::bytecode::disassemble(out.stages.module.as_ref().expect("module"))
    }

    #[test]
    fn hello() {
        let d = dump("program Hello;\n\nbegin\n  WriteLn('Hello, world');\nend.\n");
        let expected = "\
; puyc format 2
.const
  0: str \"Hello, world\\n\"
.global
.host
  0: Console.Write params=1 ret=0
.unit Hello file=\"Hello.pas\" init=- fini=- entry=0
.func 0 Hello.<main> params=0 regs=1
  .line 4
  0000  LoadConst    r0, k0
  0001  CallHost     r0, h0
  .line 5
  0002  RetVoid
.finally r0
";
        assert_eq!(d, expected);
    }

    #[test]
    fn arithmetic_and_calls() {
        let d = dump(
            "program P;\nvar Total: Integer = 1;\nfunction Sq(X: Integer): Integer;\nbegin\n  Result := X * X;\nend;\nbegin\n  Total := Sq(Total) + 2;\n  WriteLn(Total);\nend.\n",
        );
        eprintln!("{d}");
        assert!(d.contains(".func 1 P.<init>"));
        assert!(d.contains("MulInt       r1, r1, r0"));
        assert!(d.contains("NarrowOvf    r1, r1, 4"));
        assert!(d.contains("Call         r3, f2"));
        assert!(d.contains("StoreGlobal  r0, g0") || d.contains("StoreGlobal  r1, g0"));
    }

    #[test]
    fn loops_and_conditions() {
        let d = dump(
            "var S := 0;\nfor var I := 1 to 10 do\n  if (I mod 2 = 0) and (I > 4) then S := S + I;\nwhile S > 0 do Dec(S);\nrepeat Inc(S) until S >= 3;\nWriteLn(S);\n",
        );
        eprintln!("{d}");
        assert!(d.contains("JmpIfLtInt"));
        assert!(d.contains("JmpIfEqInt"));
        assert!(d.contains("AddIntI"));
        assert!(d.contains("ModInt"));
        assert!(d.ends_with("RetVoid\n.finally r2, r3\n"));
    }

    #[test]
    fn verifies() {
        for src in [
            "program P;\nprocedure Swap(var A, B: Integer);\nvar T: Integer;\nbegin T := A; A := B; B := T; end;\nvar X, Y: Integer;\nbegin X := 1; Y := 2; Swap(X, Y); WriteLn(X, ' ', Y); end.",
            "uses System.SysUtils;\nvar S := 'abc';\nS[1] := 'x';\nWriteLn(Copy(S, 1, 2), Pos('b', S), UpperCase(S), IntToStr(Length(S)), StrToIntDef('1', 0), ParamStr(0), ParamCount, GetEnvironmentVariable('HOME'));\nvar N := 0; ReadLn(N); ReadLn; var L := ''; ReadLn(L); Assert(N >= 0, 'neg'); if N = 1 then Halt(3);",
            "var U: UInt64 := 1; var I: Int64 := -1; U := U * 2; I := I - Abs(I) + (I shl 3); var C: Cardinal := 7; C := C shr 1; var B: Byte := 200; Inc(B, 5); var Ch := 'a'; Inc(Ch); WriteLn(U, I, C, B, Ch, Chr(66), Ord(Ch), Boolean(1), Char(65), string(Ch));",
            "{$Q-}{$R-}\nvar B: Byte := 200; Inc(B, 100); var I := 2147483647; I := I + 1; WriteLn(B, I);",
        ] {
            let mut provider = MemorySourceProvider::new();
            let out = compile(
                "t.pas",
                src.as_bytes().to_vec(),
                &mut provider,
                &CompileOptions::default(),
            );
            assert!(
                out.diagnostics.iter().all(|d| !d.is_error()),
                "{src}\n{:#?}",
                out.diagnostics
            );
            let bytes = out.module.expect("module");
            let m = match crate::bytecode::Module::decode(&bytes, crate::VERSION) {
                Ok(m) => m,
                Err(e) => panic!(
                    "{e}\n{}",
                    crate::bytecode::disassemble(out.stages.module.as_ref().unwrap())
                ),
            };
            assert_eq!(&m, out.stages.module.as_ref().unwrap());
        }
    }

    /// Encodes a function whose only block holds `inst` followed by `RetVoid`.
    fn encode_one(inst: ir::Inst) -> Vec<u32> {
        let f = ir::Function {
            name: "P.<main>".into(),
            file: 0,
            short_name: "<main>".into(),
            name_span: TextRange::default(),
            params: 0,
            link: false,
            outer: None,
            vars: Vec::new(),
            temps: Vec::new(),
            blocks: vec![ir::Block {
                insts: vec![inst],
                term: Term::RetVoid,
                term_line: 1,
            }],
            finally: Vec::new(),
            tries: Vec::new(),
        };
        let table = TypeTable::default();
        let mut consts = ConstPool {
            entries: Vec::new(),
            types: Vec::new(),
            type_ids: Vec::new(),
            table: &table,
        };
        let mut g = FuncGen::new(&f, &mut consts);
        g.generate().expect("small function").code
    }

    #[test]
    fn wide_table_operands() {
        // Table indices above 16 bits are carried by the Wide extension word
        let operand = |opc: u8, idx: u32| match opc {
            op::LOAD_GLOBAL | op::STORE_GLOBAL | op::MAKE_GLOBAL_REF => Operand::Global(idx),
            op::CALL => Operand::Func(idx),
            _ => Operand::Host(idx),
        };
        for opc in [
            op::LOAD_GLOBAL,
            op::STORE_GLOBAL,
            op::MAKE_GLOBAL_REF,
            op::CALL,
            op::CALL_HOST,
        ] {
            for idx in [65535, 65536, 0x1234_5678, u32::MAX] {
                let kind = if matches!(opc, op::CALL | op::CALL_HOST) {
                    InstKind::Call {
                        dst: None,
                        callee: operand(opc, idx),
                        args: Vec::new(),
                        frame: Reg::Var(300),
                    }
                } else {
                    InstKind::Plain(vec![Operand::Reg(Reg::Var(300)), operand(opc, idx)])
                };
                let code = encode_one(ir::Inst {
                    op: opc,
                    kind,
                    line: 1,
                });
                assert_eq!(code.len(), 4, "{opc} {idx}");
                assert_eq!(code[0], op::WIDE as u32, "{opc} {idx}");
                assert_eq!(
                    code[1],
                    opc as u32 | (44 << 8) | ((idx & 0xFFFF) << 16),
                    "{opc} {idx}"
                );
                assert_eq!(code[2], (1 << 8) | (idx & 0xFFFF_0000), "{opc} {idx}");
            }
        }
    }

    #[test]
    // A field number is at most 16 bits even with `Wide`: a record whose 65537th field is used
    // is E3400 rather than a wrong field (bytecode.md 1.2).
    fn too_many_fields() {
        let mut src = String::from("type\n  TBig = record\n");
        for i in 0..65537 {
            src.push_str(&format!("    F{i}: Integer;\n"));
        }
        src.push_str("  end;\nvar R := Default(TBig);\nR.F65536 := 1;\nR.F65535 := 2;\n");
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "Big.pas",
            src.into_bytes(),
            &mut provider,
            &CompileOptions::default(),
        );
        let codes: Vec<String> = out
            .diagnostics
            .iter()
            .filter_map(|d| d.code.map(|c| c.to_string()))
            .collect();
        assert_eq!(codes, ["E3400"]);
        assert!(
            out.diagnostics[0]
                .message
                .contains("a record has more than 65536 fields"),
            "{}",
            out.diagnostics[0].message
        );
    }

    #[test]
    fn long_chain_and_wide_jumps() {
        // A 300-term concatenation and a branch that jumps over a long body
        let mut src = String::from("var S := 'x'");
        for _ in 0..300 {
            src.push_str(" + 'y'");
        }
        src.push_str(";\nvar N := 0;\nif N = 0 then begin\n");
        for i in 0..200 {
            src.push_str(&format!("  S := S + '{i}';\n"));
        }
        src.push_str("end;\nWriteLn(S);\n");
        let d = dump(&src);
        assert!(d.contains("w.JmpIfNeInt") || d.contains("JmpIfNeInt"));
    }
}
