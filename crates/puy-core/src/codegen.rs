//! Code generation (IR → bytecode; `docs/spec/bytecode.md`, `docs/design/lifetime.md` 3.10).
//!
//! Register allocation: a named variable `vN` lives in register `rN` (parameters, `Result`, then
//! locals; one register per variable, the M1 rule). Owning temporaries of managed types follow, one
//! register each in creation order (`docs/design/lifetime.md` 3.4), and the other temporaries come
//! last, reused like a stack by their slot (`ir::Temp::slot`), so a call only overwrites them.
//! Blocks are laid out in order; a jump to the next block is dropped.

use crate::bytecode::{
    self, ConstEntry, FIRST_TYPE_INDEX, Function, GlobalEntry, Module, TYPE_ARRAY, TYPE_DYNARRAY,
    TYPE_ENUM, TYPE_RECORD, TYPE_SET, TypeEntry, UnitEntry, op,
};
use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Stage};
use crate::ir::{self, BlockId, InstKind, Operand, Reg, Term};
use crate::sema::hir::GenericOrigin;
use crate::sema::hir::VarKind;
use crate::sema::types::{Const, IntfEntry, Ty, TypeTable, VmtEntry};
use crate::source::{FileId, Span};

mod debug;
mod rtti;
mod share;

/// Where a function loads function indexes as values (`ir::Operand::FuncValue`): (word position
/// of the instruction, the amount added to the index). `share` renumbers them.
type FuncValues = Vec<(u32, i64)>;

/// Maximum number of registers (1.2).
const MAX_REGS: u32 = 65535;
/// Maximum length of the instruction stream (in words; 1.3).
const MAX_WORDS: usize = i32::MAX as usize;

/// Generates the module. Returns `None` on an error (E3400).
pub fn generate(m: &ir::Module, sink: &mut DiagnosticSink) -> Option<Module> {
    // The type information numbers (`docs/spec/rtti.md` 7.1), which `TypeInfo(T)` loads
    let (rtti_order, rtti_numbers) = rtti::numbers(m);
    let mut consts = ConstPool {
        entries: Vec::new(),
        types: Vec::new(),
        type_ids: Vec::new(),
        table: &m.types,
        func_index: &m.func_index,
        intf_calls: Vec::new(),
        rtti_numbers: &rtti_numbers,
        debug: debug::DebugTypes::default(),
    };
    let mut failed = false;
    let mut funcs = Vec::with_capacity(m.funcs.len());
    let mut func_values = Vec::with_capacity(m.funcs.len());
    for f in inline_and_optimize(m, &rtti_numbers) {
        let mut g = FuncGen::new(&f, &mut consts);
        match g.generate() {
            Ok((func, values)) => {
                funcs.push(func);
                func_values.push(values);
            }
            Err(d) => {
                sink.push(*d);
                failed = true;
            }
        }
    }
    if failed {
        return None;
    }
    let mut module = Module {
        consts: std::mem::take(&mut consts.entries),
        types: Vec::new(),
        intf_calls: Vec::new(),
        funcs,
        globals: m
            .globals
            .iter()
            .map(|g| GlobalEntry {
                name: g.name.clone(),
                kind: if g.exit_code {
                    crate::bytecode::GLOBAL_EXIT_CODE
                } else {
                    match m.types.slot_kind(g.ty) {
                        crate::bytecode::SLOT_REF => crate::bytecode::GLOBAL_REF,
                        crate::bytecode::SLOT_OBJ => crate::bytecode::GLOBAL_OBJ,
                        _ => crate::bytecode::GLOBAL_INT,
                    }
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
        included: m.included.clone(),
        hosts: m.hosts.clone(),
        exts: Vec::new(),
        ext_units: Vec::new(),
        rtti: None,
        tests: m.tests.clone(),
        debug: None,
    };
    // The extension sections (`docs/spec/extensions.md` 4): the classes take their type indexes
    // (an extension unit's exception classes are put in the type section, so that the VM finds
    // one an extension raises by name)
    for u in &m.ext_units {
        let classes = u
            .classes
            .iter()
            .map(|(n, t)| (n.clone(), consts.ty(*t)))
            .collect();
        for t in &u.exceptions {
            consts.ty(*t);
        }
        module.ext_units.push(bytecode::ExtUnitEntry {
            name: u.name.clone(),
            fingerprint: u.fingerprint,
            classes,
        });
    }
    for e in &m.exts {
        module.exts.push(bytecode::ExtEntry {
            name: e.name.clone(),
            unit: e.unit,
            params: e.params.clone(),
            ret: e.ret,
            ret_type: e.ret_type.map_or(bytecode::NONE_INDEX, |t| consts.ty(t)),
            ret_range: e.ret_range,
            sigs: e.sigs.clone(),
            shapes: e
                .shapes
                .iter()
                .map(|sh| ext_shape(&mut consts, sh))
                .collect(),
        });
    }
    module.rtti = rtti::section(m, &rtti_order, &rtti_numbers, &mut consts);
    module.types = consts.types;
    module.intf_calls = consts.intf_calls;
    module.debug = consts.debug.section();
    let origins: Vec<Option<&GenericOrigin>> = m.funcs.iter().map(|f| f.origin.as_ref()).collect();
    share::share(&mut module, &origins, &func_values);
    Some(module)
}

/// The functions of `m` with the calls of small routines inlined (`ir::inline`) and optimized
/// (`ir::opt`), in function table order. Callees go first, so that a caller inlines a callee
/// with the calls inlined into it. `rtti_numbers` are the module's type information numbers
/// ([`rtti::numbers`], made once).
fn inline_and_optimize(
    m: &ir::Module,
    rtti_numbers: &std::collections::HashMap<Ty, u32>,
) -> Vec<ir::Function> {
    let n = m.funcs.len();
    let (order, recursive) = ir::inline::call_order(m);
    let mut is_outer = vec![false; n];
    for f in &m.funcs {
        if let Some(o) = f.outer {
            is_outer[o as usize] = true;
        }
    }
    // Per function: its IR with the calls inlined, kept while it may be inlined itself
    let mut inlinable: Vec<Option<ir::Function>> = vec![None; n];
    let mut optimized: Vec<Option<ir::Function>> = vec![None; n];
    for i in order {
        let optimize = |f: &ir::Function| {
            let temp_regs = temp_registers(f);
            let phys = |r: Reg| register(f, &temp_regs, r);
            ir::opt::optimize(f, &m.types, &m.globals, &phys)
        };
        let mut f = ir::inline::inline_calls(&m.funcs[i], &|g| inlinable[g as usize].as_ref());
        let mut o = optimize(&f);
        let candidate = !recursive[i] && ir::inline::may_inline(&f, &m.types, is_outer[i]);
        let size = |o: &ir::Function| instruction_count(m, rtti_numbers, o);
        if candidate && size(&o) > ir::inline::MAX_SIZE && !f.inlines.is_empty() {
            // Small without the calls inlined into it: it keeps its calls and stays small
            // enough to be inlined (`docs/design/inlining.md` 3.5)
            let own = optimize(&m.funcs[i]);
            if size(&own) <= ir::inline::MAX_SIZE {
                f = m.funcs[i].clone();
                o = own;
            }
        }
        if candidate && size(&o) <= ir::inline::MAX_SIZE {
            inlinable[i] = Some(f);
        }
        optimized[i] = Some(o);
    }
    optimized
        .into_iter()
        .map(|f| f.expect("every function is optimized"))
        .collect()
}

/// The size of an optimized function for the inlining (`docs/design/inlining.md` 3.5): the
/// number of instructions codegen lays out for it (not counting `Wide`, `Ext` and type words).
/// `u32::MAX` if it cannot be generated.
fn instruction_count(
    m: &ir::Module,
    rtti_numbers: &std::collections::HashMap<Ty, u32>,
    f: &ir::Function,
) -> u32 {
    let mut scratch = ConstPool {
        entries: Vec::new(),
        types: Vec::new(),
        type_ids: Vec::new(),
        table: &m.types,
        func_index: &m.func_index,
        intf_calls: Vec::new(),
        rtti_numbers,
        debug: debug::DebugTypes::default(),
    };
    let mut g = FuncGen::new(f, &mut scratch);
    if g.generate().is_err() {
        return u32::MAX;
    }
    g.instrs
        .iter()
        .filter(|(i, _)| !matches!(i, Instr::TypeWord(_)))
        .count() as u32
}

/// The shape of a record or dynamic array of an extension function with its types' indexes
/// (adding them to the type section).
fn ext_shape(
    consts: &mut ConstPool<'_>,
    s: &crate::sema::native::NativeShape,
) -> bytecode::ExtShape {
    bytecode::ExtShape {
        kind: s.kind,
        ty: s.ty.map_or(bytecode::NONE_INDEX, |t| consts.ty(t)),
        range: s.range,
        items: s.items.iter().map(|i| ext_shape(consts, i)).collect(),
    }
}

/// The constant pool and the type section (entries are shared in order of first use).
struct ConstPool<'t> {
    entries: Vec<ConstEntry>,
    types: Vec<TypeEntry>,
    /// The type of each entry of `types`.
    type_ids: Vec<Ty>,
    table: &'t TypeTable,
    /// The function index of each HIR routine (for the virtual method tables).
    func_index: &'t [u32],
    /// The interface call targets (`docs/spec/objects.md` 14.7), in order of first use.
    intf_calls: Vec<(u32, u32)>,
    /// The type information number of each type with type information (`docs/spec/rtti.md`
    /// 7.1).
    rtti_numbers: &'t std::collections::HashMap<Ty, u32>,
    /// The entries of the debug information section (`docs/spec/semantics.md` 5.4).
    debug: debug::DebugTypes,
}

impl ConstPool<'_> {
    /// The type index of a type already in the type section (or a built-in type's), without
    /// adding it.
    fn existing(&self, ty: Ty) -> Option<u32> {
        if let Some(id) = ty.type_id()
            && TypeTable::is_builtin(id)
        {
            return Some(id.0);
        }
        self.type_ids
            .iter()
            .position(|t| *t == ty)
            .map(|i| FIRST_TYPE_INDEX + i as u32)
    }

    /// The type information number of a type (`TypeInfo(T)`; `docs/spec/rtti.md` 7.1).
    fn type_info(&self, ty: Ty) -> u32 {
        let t = ir::rtti::normalize(self.table, ty).expect("`TypeInfo` of a type of a value");
        *self
            .rtti_numbers
            .get(&t)
            .expect("`ir::reach` gives every `TypeInfo` operand an entry")
    }

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
        let mut flags = 0;
        let mut vmt = Vec::new();
        let mut abstract_names = Vec::new();
        let mut intfs = Vec::new();
        let mut methods = 0;
        let mut guid = [0; 16];
        let (kind, slots, refs) = match ty {
            Ty::Intf(id) => {
                // `docs/spec/objects.md` 14.7: the parent first
                let p = self
                    .table
                    .parent(id)
                    .expect("a declared interface has a parent");
                parent = Some(self.ty(Ty::Intf(p)));
                methods = self.table.intf_method_count(id);
                if let crate::sema::types::TypeKind::Interface { guid: Some(g), .. } =
                    self.table.get(id).kind
                {
                    guid = g;
                }
                (crate::bytecode::TYPE_INTERFACE, 0, Vec::new())
            }
            Ty::Class(id) => {
                // `docs/spec/objects.md` 14.1: the parent first, then this class's entry
                let p = self
                    .table
                    .parent(id)
                    .expect("a declared class has a parent");
                parent = Some(self.ty(Ty::Class(p)));
                if let crate::sema::types::TypeKind::Class {
                    ref_counted: true, ..
                } = self.table.get(id).kind
                {
                    flags |= crate::bytecode::CLASS_REF_COUNTED;
                }
                if self.table.class_is_abstract(id) {
                    flags |= crate::bytecode::CLASS_ABSTRACT;
                }
                if let crate::sema::types::TypeKind::Class { sealed: true, .. } =
                    self.table.get(id).kind
                {
                    flags |= crate::bytecode::CLASS_SEALED;
                }
                // `docs/spec/objects.md` 14.1
                vmt = self
                    .table
                    .class_vmt(id)
                    .iter()
                    .map(|e| match e {
                        VmtEntry::Func(r) => self.method(*r),
                        VmtEntry::Builtin(k) => crate::bytecode::VMT_BUILTIN + k,
                        VmtEntry::Abstract => crate::bytecode::VMT_ABSTRACT,
                    })
                    .collect();
                abstract_names = self
                    .table
                    .class_vmt(id)
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| **e == VmtEntry::Abstract)
                    .map(|(k, _)| (k as u32, self.table.vmt_method_name(id, k as u32)))
                    .collect();
                // The interface table (`docs/spec/objects.md` 14.7): a virtual method's entry is
                // this class's table entry (its override)
                for e in self.table.class_intfs(id) {
                    let funcs = e
                        .entries
                        .iter()
                        .map(|x| match x {
                            IntfEntry::Func(r) => self.method(*r),
                            IntfEntry::Virtual(k) => vmt[*k as usize],
                            IntfEntry::Missing => {
                                unreachable!("a class missing an interface method has an error")
                            }
                        })
                        .collect();
                    intfs.push(crate::bytecode::IntfEntry {
                        intf: self.ty(Ty::Intf(e.intf)),
                        funcs,
                    });
                }
                // Records and static arrays in fields are laid out flat (`docs/design/records.md`
                // 3.2)
                let mut refs = Vec::new();
                for (_, f) in self.table.class_fields(id) {
                    self.table.flat_kinds(f.ty, &mut refs);
                }
                (crate::bytecode::TYPE_CLASS, refs.len() as u32, refs)
            }
            Ty::Enum(_) => {
                values = self
                    .table
                    .enum_values(ty)
                    .iter()
                    .map(|v| (v.ord as i64, v.name.clone()))
                    .collect();
                (TYPE_ENUM, 0, Vec::new())
            }
            Ty::Record(_) => {
                let mut refs = Vec::new();
                for f in self.table.fields(ty) {
                    self.table.flat_kinds(f.ty, &mut refs);
                }
                (TYPE_RECORD, refs.len() as u32, refs)
            }
            Ty::BigSet(_) => (TYPE_SET, self.table.set_bits(ty), Vec::new()),
            Ty::DynArray(_) | Ty::OpenArray(_) => {
                let elem = self
                    .table
                    .dyn_elem(ty)
                    .expect("a dynamic array type has an element type");
                let mut refs = Vec::new();
                self.table.flat_kinds(elem, &mut refs);
                if self.table.is_bytes_array(ty) {
                    flags = crate::bytecode::DYN_BYTES;
                }
                (TYPE_DYNARRAY, 0, refs)
            }
            _ => {
                let (_, lo, hi, elem) = self
                    .table
                    .array(ty)
                    .expect("a type operand is a record or a static array");
                let mut refs = Vec::new();
                self.table.flat_kinds(elem, &mut refs);
                (TYPE_ARRAY, (hi - lo + 1) as u32, refs)
            }
        };
        let ty = match ty {
            Ty::OpenArray(id) => Ty::DynArray(id),
            t => t,
        };
        if let Some(i) = self.type_ids.iter().position(|t| *t == ty) {
            return FIRST_TYPE_INDEX + i as u32;
        }
        // An array's element takes the slots of its kinds (`docs/design/records.md` 3.3)
        let width = match kind {
            TYPE_ARRAY | TYPE_DYNARRAY => refs.len() as u32,
            _ => 0,
        };
        self.types.push(TypeEntry {
            kind,
            slots,
            refs,
            width,
            name: self.table.full_name(ty),
            parent,
            values,
            flags,
            vmt,
            abstract_names,
            intfs,
            methods,
            guid,
        });
        self.type_ids.push(ty);
        FIRST_TYPE_INDEX + self.types.len() as u32 - 1
    }

    /// The function index of a method in a table of a class in the type section: `ir::reach`
    /// keeps the methods of these classes.
    fn method(&self, r: crate::sema::hir::RoutineId) -> u32 {
        let f = self.func_index[r.0 as usize];
        assert_ne!(
            f,
            ir::reach::DROPPED,
            "a method of a class in the type section is kept"
        );
        f
    }

    /// The index of an interface call target (`docs/spec/objects.md` 14.7), adding it on first
    /// use.
    fn intf_call(&mut self, intf: Ty, method: u32) -> u32 {
        let t = (self.ty(intf), method);
        match self.intf_calls.iter().position(|c| *c == t) {
            Some(i) => i as u32,
            None => {
                self.intf_calls.push(t);
                self.intf_calls.len() as u32 - 1
            }
        }
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
        let mut ks = Vec::new();
        self.agg_items(ty, c, &mut ks);
        let t = self.ty(ty);
        self.add(ConstEntry::Agg { ty: t, items: ks })
    }

    /// The constants of the slots of a record or static array constant: those of its fields or
    /// elements, a record or static array laid out flat among them by its own slots
    /// (`docs/design/records.md` 3.3).
    fn agg_items(&mut self, ty: Ty, c: &Const, out: &mut Vec<u32>) {
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
        for (v, t) in items.iter().zip(slot_types) {
            if self.table.flat_width(t).is_some() && matches!(v, Const::Agg(_)) {
                self.agg_items(t, v, out);
            } else {
                out.push(self.slot_const(v, t));
            }
        }
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
        op: u16,
        a: u32,
        b: u32,
        c: u32,
    },
    /// A jump to the start of a block (`b` is the second register of a compare-and-branch).
    Jump {
        op: u16,
        a: u32,
        b: u32,
        label: BlockId,
    },
    /// The type word that ends the instruction before it (`docs/spec/bytecode.md` 1.2): one word,
    /// after the extension word when that instruction is `Wide`.
    TypeWord(u32),
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
    /// The instructions loading a function index as a value (`ir::Operand::FuncValue`): (index
    /// into `instrs`, the amount added to the index).
    func_values: Vec<(usize, i64)>,
}

impl<'a, 't> FuncGen<'a, 't> {
    fn new(f: &'a ir::Function, consts: &'a mut ConstPool<'t>) -> Self {
        FuncGen {
            f,
            consts,
            nvars: named_registers(f),
            temp_regs: temp_registers(f),
            instrs: Vec::new(),
            labels: vec![None; f.blocks.len()],
            end_finally: vec![None; f.blocks.len()],
            line: 1,
            oversize: None,
            func_values: Vec::new(),
        }
    }

    /// Whether control reaches the start of `target` by falling through from the start of `next`:
    /// they are the same block, or the blocks between are empty and each just goes on to the
    /// next (no instruction is laid out for them; the step `ForLoop` replaced, the join of an
    /// `if` without `else`).
    fn falls_to(&self, next: BlockId, target: BlockId) -> bool {
        let mut b = next;
        while b < target {
            match self.f.blocks.get(b as usize) {
                Some(block)
                    if block.insts.is_empty()
                        && matches!(block.term, Term::Jmp(t) if t == b + 1) =>
                {
                    b += 1
                }
                _ => return false,
            }
        }
        b == target
    }

    /// The register of a virtual register.
    fn reg(&self, r: Reg) -> u32 {
        register(self.f, &self.temp_regs, r)
    }

    fn emit(&mut self, op: u16, a: u32, b: u32, c: u32) {
        self.instrs.push((Instr::Plain { op, a, b, c }, self.line));
    }

    fn emit_bx(&mut self, op: u16, a: u32, bx: u32) {
        self.emit(op, a, bx & 0xFF, bx >> 8);
    }

    fn emit_jump(&mut self, op: u16, a: u32, b: u32, label: BlockId) {
        self.instrs
            .push((Instr::Jump { op, a, b, label }, self.line));
    }

    /// The type index of a type operand (the type word of `RefFieldSet`, `AggLoad`, ...).
    fn type_operand(&mut self, o: &Operand) -> u32 {
        match o {
            Operand::Type(t) => self.consts.ty(*t),
            o => unreachable!("the operand is a type, not {o:?}"),
        }
    }

    /// The first register of a cursor (`docs/spec/bytecode.md` 1.5), whose two registers lower
    /// lists one after the other.
    fn cursor(&self, regs: &[Operand]) -> u32 {
        let r: Vec<u32> = regs.iter().map(|o| self.reg_operand(o)).collect();
        assert_eq!(
            r[1],
            r[0] + 1,
            "lower lays out a cursor in consecutive registers"
        );
        r[0]
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

    /// The function, and where it loads function indexes as values: (word position of the
    /// instruction, the amount added to the index; `share`).
    fn generate(&mut self) -> Result<(Function, FuncValues), Box<Diagnostic>> {
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
        let inlines = self.inline_table(&positions);
        let func_values = self
            .func_values
            .iter()
            .map(|&(i, add)| (positions[i] as u32, add))
            .collect();
        if code.len() > MAX_WORDS {
            return Err(self.too_large("its code exceeds 2147483647 words"));
        }
        let locals: Vec<(u16, String)> = self
            .f
            .vars
            .iter()
            .enumerate()
            // An expanded record has no value of its own: its fields are named variables
            // (`docs/design/records.md` 3.9)
            .filter(|(_, v)| {
                v.kind != VarKind::Hidden && !matches!(v.expand, ir::VarExpand::Whole { .. })
            })
            .map(|(i, v)| (self.reg(Reg::Var(i as u32)) as u16, v.name.clone()))
            .collect();
        Ok((
            Function {
                name: self.f.name.clone(),
                params: self.f.params as u16,
                outer: self.f.outer,
                regs: regs as u16,
                code,
                lines,
                locals,
                managed: self
                    .f
                    .finally
                    .iter()
                    .map(|r| bytecode::ManagedReg {
                        reg: self.reg(*r) as u16,
                        kind: self
                            .f
                            .reg_type(*r)
                            .map_or(bytecode::MANAGED_HEAP_REF, |t| self.managed_kind(t)),
                    })
                    .collect(),
                file: self.f.file,
                handlers,
                shared_by: Vec::new(),
                inlines,
            },
            func_values,
        ))
    }

    /// The kind of a managed register holding a value of the managed type `ty` (3.2 of
    /// `docs/spec/bytecode.md`): counted by its contents when `ty` is.
    fn managed_kind(&self, ty: Ty) -> u16 {
        if self.consts.table.slot_kind(ty) == bytecode::SLOT_OBJ {
            bytecode::MANAGED_OBJ
        } else {
            bytecode::MANAGED_HEAP_REF
        }
    }

    /// The word position of the start of a block (the end of the code past the last block).
    fn block_position(&self, positions: &[usize], b: BlockId) -> u32 {
        let i = match self.labels.get(b as usize) {
            Some(l) => l.expect("every block is laid out"),
            None => self.instrs.len(),
        };
        positions[i] as u32
    }

    /// The inline table (`docs/spec/bytecode.md` 3.2) from the IR's inlined blocks. A site whose
    /// code came out empty is left out (and so are the sites inside it).
    fn inline_table(&self, positions: &[usize]) -> Vec<bytecode::InlineEntry> {
        let mut index: Vec<Option<u32>> = Vec::with_capacity(self.f.inlines.len());
        let mut out = Vec::new();
        for s in &self.f.inlines {
            let start = self.block_position(positions, s.blocks.0);
            let end = self.block_position(positions, s.blocks.1 + 1);
            if start == end {
                index.push(None);
                continue;
            }
            let outer = s
                .outer
                .map(|o| index[o as usize].expect("the site around a site with code has code"));
            index.push(Some(out.len() as u32));
            out.push(bytecode::InlineEntry {
                start,
                end,
                func: s.callee,
                line: s.line,
                outer,
            });
        }
        out
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
                    // `CallClosure` has its arguments after the frame (`docs/spec/objects.md`
                    // 14.9)
                    let first = base + u32::from(inst.op == op::CALL_CLOSURE);
                    for (i, a) in args.iter().enumerate() {
                        assert_eq!(
                            self.reg(*a),
                            first + i as u32,
                            "lower lays out call arguments contiguously"
                        );
                    }
                    if let Operand::Reg(value) = callee {
                        // `CallIndirect` / `CallClosure A B`: the value is a register
                        let v = self.reg(*value);
                        self.emit(inst.op, base, v, 0);
                        if let Some(d) = dst {
                            let d = self.reg(*d);
                            if d != base {
                                self.emit(op::MOVE, d, base, 0);
                            }
                        }
                        continue;
                    }
                    let x = match callee {
                        Operand::Func(f) | Operand::Host(f) | Operand::Ext(f) => *f,
                        Operand::Virtual { index, .. } => *index,
                        Operand::Intf { intf, method, .. } => self.consts.intf_call(*intf, *method),
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
                if !self.falls_to(next, *t) {
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
                if !self.falls_to(next, *other) {
                    self.emit_jump(op::JMP, 0, 0, *other);
                }
            }
            Term::BranchImm {
                op: opc,
                a,
                imm,
                then,
                other,
            } => {
                let a = self.reg(*a);
                // `sB` as 16 bits, for the extension word of a `Wide` form (1.2)
                let b = *imm as i16 as u16 as u32;
                self.emit_jump(*opc, a, b, *then);
                if !self.falls_to(next, *other) {
                    self.emit_jump(op::JMP, 0, 0, *other);
                }
            }
            Term::ForLoop {
                counter,
                limit,
                delta,
                top,
                exit,
            } => {
                let opc = if *delta > 0 {
                    op::FOR_LOOP_UP
                } else {
                    op::FOR_LOOP_DOWN
                };
                let (a, b) = (self.reg(*counter), self.reg(*limit));
                self.emit_jump(opc, a, b, *top);
                if !self.falls_to(next, *exit) {
                    self.emit_jump(op::JMP, 0, 0, *exit);
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
                if !self.falls_to(next, *after) {
                    self.emit_jump(op::JMP, 0, 0, *after);
                }
            }
        }
    }

    /// The register of a register operand.
    fn reg_operand(&self, o: &Operand) -> u32 {
        match o {
            Operand::Reg(r) => self.reg(*r),
            o => unreachable!("expected a register operand, not {o:?}"),
        }
    }

    /// Encodes a plain instruction's operands into its fields (by the instruction's shape).
    fn plain(&mut self, opc: u16, operands: &[Operand]) {
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
            // The intrinsic's number is the last operand; the others follow the intrinsic's shape,
            // and the number goes to `C` (`docs/spec/bytecode.md` 2.19)
            op::Shape::Intrinsic => {
                let (last, rest) = operands
                    .split_last()
                    .expect("an intrinsic has its number as the last operand");
                let id = match last {
                    Operand::Imm(v) => *v as u32,
                    o => unreachable!("the last operand of `Intrinsic` is its number, not {o:?}"),
                };
                let info = op::intr::info(id).expect("lower uses the intrinsics of the table");
                let a = match info.shape {
                    op::Shape::None => 0,
                    _ => self.reg_operand(&rest[0]),
                };
                let b = match info.shape {
                    op::Shape::None | op::Shape::A => 0,
                    op::Shape::AB => self.reg_operand(&rest[1]),
                    op::Shape::ABRange2
                    | op::Shape::ABRange3
                    | op::Shape::ABRange4
                    | op::Shape::ABRange5 => {
                        let b = self.reg_operand(&rest[1]);
                        for (i, o) in rest.iter().enumerate().skip(2) {
                            assert_eq!(
                                self.reg_operand(o),
                                b + i as u32 - 1,
                                "lower lays out a register range contiguously"
                            );
                        }
                        b
                    }
                    s => unreachable!("intrinsic {} has shape {s:?}", info.name),
                };
                self.emit(opc, a, b, id);
            }
            // `ReadLnEnd`, `FlushOut` (`docs/spec/bytecode.md` 2.17)
            op::Shape::None => self.emit(opc, 0, 0, 0),
            op::Shape::A => self.emit(opc, reg(self, 0), 0, 0),
            op::Shape::AB => {
                let (a, b) = (reg(self, 0), reg(self, 1));
                // Two temporaries can share a register (M3-58)
                if !(opc == op::MOVE && a == b) {
                    self.emit(opc, a, b, 0);
                }
            }
            op::Shape::ABC => self.emit(opc, reg(self, 0), reg(self, 1), reg(self, 2)),
            op::Shape::ABCImm => self.emit(opc, reg(self, 0), reg(self, 1), imm(2) as u32),
            op::Shape::ABsCImm => {
                self.emit(opc, reg(self, 0), reg(self, 1), imm(2) as i8 as u8 as u32)
            }
            op::Shape::AOuter => self.emit(opc, reg(self, 0), imm(1) as u32, imm(2) as u32),
            op::Shape::ABRange2
            | op::Shape::ABRange3
            | op::Shape::ABRange4
            | op::Shape::ABRange5 => {
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
            // `HashLookupInt` / `HashLookupStr` (`docs/spec/bytecode.md` 2.23): the instance and
            // the key in consecutive registers, then the slot of the first field
            op::Shape::ABRange2Slot => {
                let b = reg(self, 1);
                assert_eq!(
                    reg(self, 2),
                    b + 1,
                    "lower lays out a register range contiguously"
                );
                let slot = self.small_operand(imm(3) as u32, "a record has more than 65536 fields");
                self.emit(opc, reg(self, 0), b, slot);
            }
            op::Shape::ASBxImm => {
                // A type index loaded as an integer (`DtorEnter`; `docs/spec/objects.md` 14.8)
                // or a class reference (its type index + 1; `docs/spec/objects.md` 14.1)
                let v = match &operands[1] {
                    Operand::FuncValue { func, add } => {
                        self.func_values.push((self.instrs.len(), *add));
                        i64::from(*func) + add
                    }
                    o @ (Operand::Type(t)
                    | Operand::ClassRef(t)
                    | Operand::TypeInfo(t)
                    | Operand::DebugType(t)) => {
                        let t = match o {
                            Operand::TypeInfo(_) => self.consts.type_info(*t),
                            Operand::DebugType(_) => {
                                let table = self.consts.table;
                                self.consts.debug.number(table, *t)
                            }
                            _ => self.consts.ty(*t) + u32::from(matches!(o, Operand::ClassRef(_))),
                        };
                        if t > i16::MAX as u32 {
                            self.oversize
                                .get_or_insert("the module has more than 32768 types");
                        }
                        i64::from(t)
                    }
                    _ => imm(1),
                };
                let v = v as i16 as u16 as u32;
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
            op::Shape::ASlotCType => {
                let slot = self.small_operand(imm(1) as u32, "a record has more than 65536 fields");
                let t = self.type_operand(&operands[3]);
                self.emit(opc, reg(self, 0), slot, reg(self, 2));
                self.instrs.push((Instr::TypeWord(t), self.line));
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
            op::Shape::ABList => {
                let b = reg(self, 1);
                let n = imm(2) as u32;
                assert_eq!(operands.len(), 3 + n as usize, "{opc} has {n} operands");
                for i in 0..n as usize {
                    assert_eq!(
                        reg(self, 3 + i),
                        b + i as u32,
                        "lower lays out the operands in consecutive registers"
                    );
                }
                let n = self.small_operand(n, "a concatenation has more than 65536 operands");
                self.emit(opc, reg(self, 0), b, n);
            }
            // The instructions of flat places (`docs/spec/bytecode.md` 2.8): a cursor is two
            // registers, both listed (the second one after the first)
            op::Shape::ACursorBC => {
                let a = self.cursor(&operands[..2]);
                self.emit(opc, a, reg(self, 2), reg(self, 3));
            }
            op::Shape::ABCursorSlot => {
                let b = self.cursor(&operands[1..3]);
                self.emit(opc, reg(self, 0), b, imm(3) as u32);
            }
            op::Shape::ACursorSlotC => {
                let a = self.cursor(&operands[..2]);
                self.emit(opc, a, imm(2) as u32, reg(self, 3));
            }
            op::Shape::ABCursorType => {
                let b = self.cursor(&operands[1..3]);
                let t = self.type_operand(&operands[3]);
                self.emit(opc, reg(self, 0), b, 0);
                self.instrs.push((Instr::TypeWord(t), self.line));
            }
            op::Shape::ACursorBType => {
                let a = self.cursor(&operands[..2]);
                let t = self.type_operand(&operands[3]);
                self.emit(opc, a, reg(self, 2), 0);
                self.instrs.push((Instr::TypeWord(t), self.line));
            }
            op::Shape::ABPath | op::Shape::ABPathType => {
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
                if shape == op::Shape::ABPathType {
                    let t = self.type_operand(&operands[3 + n as usize]);
                    self.instrs.push((Instr::TypeWord(t), self.line));
                }
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
                    Operand::FuncValue { func, add } => {
                        self.func_values.push((self.instrs.len(), *add));
                        self.consts.int(i64::from(*func) + add)
                    }
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
            op::Shape::ASBx
            | op::Shape::SBx
            | op::Shape::ABsC
            | op::Shape::AsBsC
            | op::Shape::AJumps => {
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
            /// `ForLoopUp` / `ForLoopDown` that cannot reach `top`: the three instructions it
            /// stands for (`JmpIfEqInt` over the next two, `AddIntI`, `Jmp`), without `Wide`
            Loop3,
            /// The same with `Wide` on each (registers over 255, or a `Jmp` beyond 16 bits)
            Loop9,
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
        // `Ext` adds a word to an instruction numbered past 255 (`docs/spec/bytecode.md` 1.2)
        let size = |f: Form, i: &Instr| -> usize {
            let ext = match i {
                Instr::Plain { op, .. } => usize::from(bytecode::needs_ext(*op)),
                _ => 0,
            };
            match (f, i) {
                (Form::Short, _) => 1 + ext,
                (Form::Wide, _) => 3 + ext,
                (Form::Expanded, _) => 6,
                (Form::Loop3, _) => 3,
                (Form::Loop9, _) => 9,
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
                let shape = op::info(*op).map(|i| i.shape);
                let fused = matches!(shape, Some(op::Shape::ABsC | op::Shape::AsBsC));
                // `B` of `AsBsC` is an immediate that fits in 8 bits (`Term::BranchImm`)
                let regs_wide = *a > 255 || (shape == Some(op::Shape::ABsC) && *b > 255);
                let for_loop = matches!(*op, op::FOR_LOOP_UP | op::FOR_LOOP_DOWN);
                let needed = match forms[i] {
                    Form::Expanded => Form::Expanded,
                    Form::Loop9 => Form::Loop9,
                    _ if for_loop => {
                        let off_short = target as i64 - (positions[i] + 1) as i64;
                        let off_jmp = target as i64 - (positions[i] + 3) as i64;
                        if regs_wide {
                            Form::Loop9
                        } else if (i8::MIN as i64..=i8::MAX as i64).contains(&off_short) {
                            Form::Short
                        } else if (i16::MIN as i64..=i16::MAX as i64).contains(&off_jmp) {
                            Form::Loop3
                        } else {
                            Form::Loop9
                        }
                    }
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
                    Form::Wide | Form::Loop3 => 1,
                    Form::Expanded => 2,
                    Form::Loop9 => 3,
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
                    // The prefixes in their order: `Wide`, then `Ext` (1.2)
                    let ext = bytecode::needs_ext(*op).then(|| bytecode::encode(op::EXT, 0, 0, 0));
                    if forms[i] == Form::Wide {
                        code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                        code.extend(ext);
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
                        code.extend(ext);
                        code.push(bytecode::encode(*op, *a, *b, *c));
                    }
                }
                Instr::TypeWord(t) => code.push(*t),
                Instr::Jump { op, a, b, label } => {
                    debug_assert!(!bytecode::needs_ext(*op), "a jump has no `Ext` form");
                    let target = positions[self.labels[*label as usize].unwrap()] as i64;
                    let fused = op::info(*op)
                        .is_some_and(|i| matches!(i.shape, op::Shape::ABsC | op::Shape::AsBsC));
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
                        Form::Loop3 => {
                            let delta = if *op == op::FOR_LOOP_UP { 1i32 } else { -1 };
                            code.push(bytecode::encode(op::JMP_IF_EQ_INT, *a, *b, 2));
                            code.push(bytecode::encode(
                                op::ADD_INT_I,
                                *a,
                                *a,
                                delta as i8 as u8 as u32,
                            ));
                            let off = (target - (code.len() as i64 + 1)) as i16 as u16 as u32;
                            code.push(bytecode::encode(op::JMP, 0, off & 0xFF, off >> 8));
                        }
                        Form::Loop9 => {
                            let delta = if *op == op::FOR_LOOP_UP { 1i32 } else { -1 };
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            code.push(bytecode::encode(op::JMP_IF_EQ_INT, *a, *b, 6));
                            code.push(bytecode::encode_ext(*a, *b, 6));
                            let d = delta as i16 as u16 as u32;
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            code.push(bytecode::encode(op::ADD_INT_I, *a, *a, d & 0xFF));
                            code.push(bytecode::encode_ext(*a, *a, d));
                            let off = (target - (code.len() as i64 + 3)) as i32 as u32;
                            code.push(bytecode::encode(op::WIDE, 0, 0, 0));
                            code.push(bytecode::encode(op::JMP, 0, off & 0xFF, (off >> 8) & 0xFF));
                            code.push(bytecode::encode_ext(
                                0,
                                (off >> 8) & 0xFFFF,
                                (off >> 16) & 0xFFFF,
                            ));
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

/// The number of registers before the temporaries: the static link (`docs/spec/semantics.md` 4.9)
/// and the named variables.
fn named_registers(f: &ir::Function) -> u32 {
    f.vars.len() as u32 + u32::from(f.link)
}

/// The register of each temporary (the module documentation).
fn temp_registers(f: &ir::Function) -> Vec<u32> {
    let nvars = named_registers(f);
    let owning = f.temps.iter().filter(|t| t.slot.is_none()).count() as u32;
    let mut next_owning = nvars;
    f.temps
        .iter()
        .map(|t| match t.slot {
            Some(slot) => nvars + owning + slot,
            None => {
                next_owning += 1;
                next_owning - 1
            }
        })
        .collect()
}

/// The register of a virtual register, given the registers of the temporaries.
fn register(f: &ir::Function, temp_regs: &[u32], r: Reg) -> u32 {
    match r {
        Reg::Var(n) => n + u32::from(f.link),
        Reg::Temp(t) => temp_regs[t as usize],
        Reg::Link => 0,
    }
}

/// Whether an instruction's operand does not fit in 8 bits. A cursor fits when its second
/// register does (`docs/spec/bytecode.md` 1.2).
fn plain_needs_wide(i: &Instr) -> bool {
    match i {
        Instr::Plain { op: opc, a, b, c } => {
            let shape = op::info(*opc).map(|i| i.shape).unwrap_or(op::Shape::None);
            if shape.cursor_end(*a, *b).is_some_and(|r| r > 255) {
                return true;
            }
            match shape {
                op::Shape::ABx => *a > 255 || (b | (c << 8)) > 0xFFFF,
                op::Shape::ASBxImm => *a > 255,
                op::Shape::ABsC | op::Shape::ABsCImm => *a > 255 || *b > 255,
                _ => *a > 255 || *b > 255 || *c > 255,
            }
        }
        Instr::Jump { .. } | Instr::TypeWord(_) => false,
    }
}

/// The inverted condition of a compare-and-branch.
fn invert_fused(opc: u16, a: u32, b: u32) -> (u16, u32, u32) {
    match opc {
        op::JMP_IF_EQ_INT => (op::JMP_IF_NE_INT, a, b),
        op::JMP_IF_NE_INT => (op::JMP_IF_EQ_INT, a, b),
        op::JMP_IF_LT_INT => (op::JMP_IF_LE_INT, b, a),
        op::JMP_IF_LE_INT => (op::JMP_IF_LT_INT, b, a),
        op::JMP_IF_LT_UINT => (op::JMP_IF_LE_UINT, b, a),
        op::JMP_IF_LE_UINT => (op::JMP_IF_LT_UINT, b, a),
        op::JMP_IF_EQ_STR => (op::JMP_IF_NE_STR, a, b),
        op::JMP_IF_NE_STR => (op::JMP_IF_EQ_STR, a, b),
        op::JMP_IF_EQ_INT_I => (op::JMP_IF_NE_INT_I, a, b),
        op::JMP_IF_NE_INT_I => (op::JMP_IF_EQ_INT_I, a, b),
        op::JMP_IF_LT_INT_I => (op::JMP_IF_GE_INT_I, a, b),
        op::JMP_IF_GE_INT_I => (op::JMP_IF_LT_INT_I, a, b),
        op::JMP_IF_LE_INT_I => (op::JMP_IF_GT_INT_I, a, b),
        op::JMP_IF_GT_INT_I => (op::JMP_IF_LE_INT_I, a, b),
        _ => unreachable!(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::driver::compile;
    use crate::source::MemorySourceProvider;
    use crate::source::TextRange;

    fn dump(src: &str) -> String {
        let mut provider = MemorySourceProvider::new();
        let out = compile(
            "Hello.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &crate::driver::test_options(),
        );
        assert!(
            out.diagnostics.iter().all(|d| !d.is_error()),
            "{:#?}",
            out.diagnostics
        );
        crate::bytecode::disassemble(out.stages.module.as_ref().expect("module"))
    }

    /// A program whose routine `Bump` declares `pad` integer variables before its own, so that
    /// with `pad` past 255 the values it writes to the fields of its `var` parameter are in
    /// registers that do not fit in 8 bits (`RefFieldSet` / `RefFieldSetRef` in the `Wide` form;
    /// `vm::tests` runs it).
    pub(crate) fn padded_ref_fields(pad: usize) -> String {
        let mut src = String::from(
            "program WideRef;
             type
  TItem = record
    Count: Integer;
    Name: string;
  end;
             procedure Bump(var It: TItem);
var
",
        );
        for i in 0..pad {
            src.push_str(&format!(
                "  V{i}: Integer;
"
            ));
        }
        src.push_str(
            "  K: Integer;
  S: string;
             begin
  K := It.Count + 1;
  S := It.Name + 'x';
  It.Count := K;
  It.Name := S;
end;
             var
  R: TItem;
             begin
  Bump(R);
  Bump(R);
  WriteLn(R.Count, ' ', R.Name);
end.
",
        );
        src
    }

    #[test]
    // A value in a register past 255 makes `RefFieldSet` / `RefFieldSetRef` `Wide`: the `Wide`
    // word, the instruction, the extension word and then the type word (spec/bytecode.md 1.2).
    fn wide_ref_field_writes_end_with_the_type_word() {
        let d = dump(&padded_ref_fields(300));
        let wide = "  0012  w.RefFieldSet r0, 0, r301, ty64\n  .line 315\n  \
                    0016  w.RefFieldSetRef r0, 1, r302, ty64\n  .line 316\n  001a  RetVoid\n";
        assert!(d.contains(wide), "{d}");
        let d = dump(&padded_ref_fields(0));
        let short = "  0006  RefFieldSet  r0, 0, r1, ty64\n  .line 15\n  \
                     0008  RefFieldSetRef r0, 1, r2, ty64\n  .line 16\n  000a  RetVoid\n";
        assert!(d.contains(short), "{d}");
    }

    /// A program whose routine `Run` declares `pad` integer variables before its own, so that with
    /// `pad` past 255 its registers do not fit in 8 bits and its instructions take the `Wide` form:
    /// integer operations and compare-and-branch with immediates, a concatenation of several
    /// strings, string lengths, global managed variables, elements, fields and instance fields
    /// lent to `var` parameters, cells, closures and routine values, `TObject` values and
    /// elements of a managed type. The anonymous method declares `pad` variables too, so that
    /// reading what it captures is `Wide` as well (`vm::tests` runs it).
    pub(crate) fn padded_program(pad: usize) -> String {
        let mut src = String::from(
            "program Wide;\n\
             uses System.SysUtils;\n\
             type\n  TBox = class\n    Value: Integer;\n    Name: string;\n    Ref: TObject;\n  end;\n\
             \x20 TPair = record\n    N: Integer;\n    S: string;\n  end;\n\
             \x20 TIntFn = reference to function(X: Integer): Integer;\n\
             \x20 TIntProc = function(X: Integer): Integer;\n\
             \x20 TTag = class(TRefObject)\n    Name: string;\n  end;\n\
             var\n  GS: string;\n\
             function Twice(X: Integer): Integer;\nbegin\n  Result := X * 2;\nend;\n\
             procedure Bump(var X: Integer);\nbegin\n  Inc(X);\nend;\n\
             procedure Run;\nvar\n",
        );
        for i in 0..pad {
            src.push_str(&format!("  V{i}: Integer;\n"));
        }
        src.push_str(
            "  A, B, C, Sum: Integer;\n  L: Int64;\n  S, T: string;\n  Arr: TArray<Integer>;\n\
             \x20 P: TPair;\n  Fn: TIntFn;\n  Pf: TIntProc;\n  Box: TBox;\n\
             \x20 O, O2: TObject;\n  Tag: TTag;\n  Strs: TArray<string>;\n  Bs: TArray<Byte>;\n\
             begin\n\
             \x20 A := 1000;\n  B := -77;\n  L := -5000000000;\n\
             \x20 WriteLn(A * 3, ' ', A div 7, ' ', A mod -7, ' ', B and 127, ' ', B or 15, ' ', B xor -1);\n\
             \x20 WriteLn(L shl 3, ' ', L shr 60, ' ', B shl 2, ' ', B shr 28);\n\
             \x20 Sum := 0;\n\
             \x20 if B = -77 then Inc(Sum, 1);\n\
             \x20 if B <> 5 then Inc(Sum, 2);\n\
             \x20 if B < -10 then Inc(Sum, 4);\n\
             \x20 if B <= -77 then Inc(Sum, 8);\n\
             \x20 if B > -100 then Inc(Sum, 16);\n\
             \x20 if B >= -76 then Inc(Sum, 32);\n\
             \x20 while B < -70 do Inc(B, 2);\n\
             \x20 WriteLn(Sum, ' ', B);\n\
             \x20 S := '<' + IntToStr(A) + '|' + IntToStr(B) + '>';\n\
             \x20 GS := S;\n  T := GS + GS;\n\
             \x20 WriteLn(S, ' ', Length(T), ' ', T);\n\
             \x20 Arr := [1, 2, 3];\n  Arr[1] := Arr[0] + Arr[2];\n  Bump(Arr[1]);\n\
             \x20 WriteLn(Arr[0], Arr[1], Arr[2], ' ', Length(Arr));\n\
             \x20 P := Default(TPair);\n  P.N := 5;\n  P.S := S;\n  Bump(P.N);\n\
             \x20 WriteLn(P.N, ' ', P.S);\n\
             \x20 C := 10;\n\
             \x20 Fn := function(X: Integer): Integer\n    var\n      Z: Integer;\n",
        );
        for i in 0..pad {
            src.push_str(&format!("      W{i}: Integer;\n"));
        }
        src.push_str(
            "    begin\n      Z := C;\n      Inc(C);\n      Result := X + Z + 1;\n    end;\n\
             \x20 WriteLn(Fn(1), ' ', Fn(2), ' ', C);\n\
             \x20 Pf := Twice;\n  WriteLn(Pf(21));\n\
             \x20 Box := TBox.Create;\n  Box.Value := 3;\n  Box.Name := T;\n  Bump(Box.Value);\n\
             \x20 WriteLn(Box.Value, ' ', Box.Name);\n\
             \x20 Tag := TTag.Create;\n  Tag.Name := S;\n  O := Tag;\n  O2 := O;\n  Tag := nil;\n  O := nil;\n\
             \x20 Box.Ref := O2;\n  O2 := nil;\n  WriteLn(Box.Ref.ClassName, ' ', O = nil);\n  Box.Ref := nil;\n  Box.Free;\n\
             \x20 SetLength(Strs, 2);\n  Strs[0] := S;\n  Strs[1] := Strs[0] + '!';\n  WriteLn(Strs[1]);\n\
             \x20 SetLength(Bs, 2);\n  Bs[1] := 7;\n  Bs[0] := Bs[1] + 1;\n  WriteLn(Bs[0], Bs[1]);\n\
             end;\n\
             begin\n  Run;\nend.\n",
        );
        src
    }

    // The instructions M3c added or moved to the fast path of the run loop take their `Wide` form
    // when the registers do not fit in 8 bits (`docs/spec/bytecode.md` 1.2), and none of them does
    // while they fit; `vm::tests::wide_forms_run_like_short_forms` runs both (M3-72)
    #[test]
    fn padded_program_takes_wide_forms() {
        let short = dump(&padded_program(0));
        let run = &short[short.find(".func 3 ").expect("Run")..];
        assert!(!run.contains("w."), "{run}");
        let d = dump(&padded_program(300));
        for name in [
            "MulIntI",
            "DivIntI",
            "ModIntI",
            "AndIntI",
            "OrIntI",
            "XorIntI",
            "ShlIntI",
            "ShrIntI",
            "ShlInt32I",
            "ShrInt32I",
            "JmpIfEqIntI",
            "JmpIfNeIntI",
            "JmpIfLtIntI",
            "JmpIfLeIntI",
            "JmpIfGtIntI",
            "JmpIfGeIntI",
            "ConcatStrN",
            "StrLen",
            "LoadGlobalRef",
            "StoreGlobalRef",
            "DynGet",
            "DynSet",
            "FieldSet",
            "ObjGet",
            "ObjSet",
            "Lend",
            "NewCell",
            "LoadRef",
            "StoreRef",
            "MakeClosure",
            "CallClosure",
            "CallIndirect",
            "AssignRef",
            "AssignObj",
            "AddRefObj",
            "ReleaseObj",
            "CapGet",
            "DynGetRef",
            "DynSetRef",
            "BytesGet",
            "BytesSet",
        ] {
            assert!(d.contains(&format!("w.{name} ")), "{name}\n{d}");
        }
    }

    #[test]
    fn hello() {
        let d = dump("program Hello;\n\nbegin\n  WriteLn('Hello, world');\nend.\n");
        let expected = "\
; puyc format 0
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
        assert!(d.contains("MulInt       r1, r0, r0"));
        assert!(d.contains("NarrowOvf    r1, r1, 4"));
        // `Sq` is small: its call is inlined (`docs/design/inlining.md`)
        assert!(d.contains("  .inline f2 P.Sq line=8\n"), "{d}");
        assert!(d.contains("StoreGlobal  r0, g0") || d.contains("StoreGlobal  r1, g0"));
    }

    #[test]
    fn loops_and_conditions() {
        let d = dump(
            "var S := 0;\nfor var I := 1 to 10 do\n  if (I mod 2 = 0) and (I > 4) then S := S + I;\nwhile S > 0 do Dec(S);\nrepeat Inc(S) until S >= 3;\nWriteLn(S);\n",
        );
        eprintln!("{d}");
        assert!(d.contains("JmpIfLtInt"));
        assert!(d.contains("ForLoopUp"));
        assert!(d.contains("AddIntI"));
        assert!(d.contains("ModIntI"));
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
                &crate::driver::test_options(),
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
            origin: None,
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
            lifecycle: false,
            inlines: Vec::new(),
        };
        let table = TypeTable::default();
        let mut consts = ConstPool {
            entries: Vec::new(),
            types: Vec::new(),
            type_ids: Vec::new(),
            table: &table,
            func_index: &[],
            intf_calls: Vec::new(),
            rtti_numbers: &std::collections::HashMap::new(),
            debug: debug::DebugTypes::default(),
        };
        let mut g = FuncGen::new(&f, &mut consts);
        g.generate().expect("small function").0.code
    }

    #[test]
    fn wide_table_operands() {
        // Table indices above 16 bits are carried by the Wide extension word
        let operand = |opc: u16, idx: u32| match opc {
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

    // An instruction numbered past 255 is written after `Ext`, and with `Wide` the prefixes come
    // in the order `Wide`, `Ext` (`docs/spec/bytecode.md` 1.2).
    #[test]
    fn ext_instructions_get_their_prefix() {
        let test_ext = |a: u32| ir::Inst {
            op: op::TEST_EXT,
            kind: InstKind::Plain(vec![
                Operand::Reg(Reg::Var(a)),
                Operand::Reg(Reg::Var(1)),
                Operand::Reg(Reg::Var(2)),
            ]),
            line: 1,
        };
        // `Ext`, `TestExt r0, r1, r2` (the `op` of its word is 16), `RetVoid`
        assert_eq!(
            encode_one(test_ext(0)),
            [0x0000_00FE, 0x0201_0010, 0x0000_0073]
        );
        // `Wide`, `Ext`, `TestExt r300, r1, r2`, the extension word, `RetVoid`
        assert_eq!(
            encode_one(test_ext(300)),
            [
                0x0000_0001,
                0x0000_00FE,
                0x0201_2C10,
                0x0000_0100,
                0x0000_0073
            ]
        );
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
            &crate::driver::test_options(),
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

    /// A `for` loop whose body is `n` statements long, inside a routine with `vars` variables
    /// declared before the loop variable (so that it gets a register past them).
    fn long_for_loop(n: usize, vars: usize) -> String {
        let mut src = String::from("procedure Run;\nvar\n");
        for i in 0..vars {
            src.push_str(&format!("  V{i}: Integer;\n"));
        }
        src.push_str(
            "  A, S, I: Integer;\nbegin\n  A := 0;\n  S := 0;\n  for I := 1 to 10 do\n  begin\n",
        );
        for _ in 0..n {
            src.push_str("    A := A + 1;\n");
        }
        src.push_str("    S := S + I;\n  end;\n  WriteLn(S, ' ', A);\nend;\nRun;\n");
        src
    }

    // A `for` loop whose `ForLoopUp` cannot reach the top of a long body falls back to the three
    // instructions it stands for, without `Wide` while the registers fit in 8 bits (M3-58)
    #[test]
    fn for_loop_out_of_reach_expands() {
        let d = dump(&long_for_loop(100, 0));
        assert!(!d.contains("ForLoopUp"), "{d}");
        assert!(
            d.contains(
                "  00d1  JmpIfEqInt   r2, r7, -> 00d4\n  00d2  AddIntI      r2, r2, 1\n  00d3  Jmp          -> 0007\n"
            ),
            "{d}"
        );
        let d = dump(&long_for_loop(1, 300));
        assert!(!d.contains("ForLoopUp"), "{d}");
        assert!(d.contains("w.JmpIfEqInt r302, r307, -> "), "{d}");
        assert!(d.contains("w.AddIntI    r302, r302, 1\n"), "{d}");
        assert!(d.contains("w.Jmp        -> "), "{d}");
    }

    // The end of a `for` iteration whose body is short is one `ForLoopUp` (M3-58)
    #[test]
    fn for_loop_in_reach_fuses() {
        let d = dump(&long_for_loop(1, 0));
        assert!(d.contains("ForLoopUp    r2, r7, -> 0005\n"), "{d}");
    }

    /// A script with `if N > -5` around `n` statements (`vm::tests` runs the same shape).
    fn long_if_negative(n: usize) -> String {
        let mut src = String::from("var N := 1;\nvar S := 0;\nif N > -5 then begin\n");
        for _ in 0..n {
            src.push_str("  S := S + 1;\n");
        }
        src.push_str("end;\nWriteLn(S);\n");
        src
    }

    // A compare-and-branch with a negative immediate over a long body takes the `Wide` form,
    // whose extension word carries the sign of `sB` (`docs/spec/bytecode.md` 1.2; M3-58)
    #[test]
    fn wide_compare_with_negative_immediate() {
        let d = dump(&long_if_negative(150));
        assert!(d.contains("w.JmpIfLeIntI r0, -5, -> "), "{d}");
        let d = dump(&long_if_negative(1));
        assert!(d.contains("  JmpIfLeIntI  r0, -5, -> "), "{d}");
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
        assert!(d.contains("JmpIfTrue"));
    }

    /// A routine with `pad` integer variables before its own, whose loop uses four constants that
    /// do not fit in an immediate.
    fn loop_constants(pad: usize) -> String {
        let mut src = String::from("procedure Run;\nvar\n");
        for i in 0..pad {
            src.push_str(&format!("  V{i}: Integer;\n"));
        }
        src.push_str(
            "  S, I: Integer;\nbegin\n  S := 0;\n  for I := 1 to 3 do\n    \
             S := ((((S + 1001) xor 1002) + 1003) xor 1004) and $FFFFFF;\n  WriteLn(S);\nend;\nRun;\n",
        );
        src
    }

    /// The instructions of the body (line 7 after the padding) of [`loop_constants`]'s loop.
    fn loop_body(d: &str, pad: usize) -> String {
        let start = d.find(&format!("  .line {}\n", pad + 7)).expect("the body");
        let end = start + d[start..].find("ForLoop").expect("the loop's end");
        d[start..end].to_string()
    }

    // Constants that do not fit in an immediate are loaded once before the loop, and the loop's
    // body reads their registers (M4-55)
    #[test]
    fn loop_constants_load_before_the_loop() {
        let d = dump(&loop_constants(0));
        let b = loop_body(&d, 0);
        assert!(!b.contains("Load"), "{d}");
        assert!(b.contains("  XorInt       r0, r0, r"), "{d}");
    }

    // Constants loaded before a loop take registers of their own below the temporaries, only as
    // many as keep every register within 8 bits (no `Wide` form; M4-55)
    #[test]
    fn hoisted_constants_keep_registers_narrow() {
        let d = dump(&loop_constants(248));
        assert!(!d.contains("w."), "{d}");
        let b = loop_body(&d, 248);
        assert!(b.contains("  LoadInt      r"), "{d}");
        assert!(b.contains("  XorInt       r248, r248, r"), "{d}");
    }
}
