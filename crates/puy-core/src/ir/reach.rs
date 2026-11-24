//! Dropping the functions no entry reaches and the `class var`s they leave unused
//! (`docs/spec/units.md` 6.1, 6.2, 8.3).
//!
//! The entries are `<main>`, the units' `<init>` and `<fini>`, the routines of the tests of a
//! test unit (`docs/spec/attributes.md` 5.2), and the virtual method tables and
//! interface tables of the classes the module names: those the kept functions name as a type
//! operand (`New`, `IsClass`, `AsClass`, a class reference), the extension classes, the exception
//! classes of the extension units, the classes of the extension functions' results, and the
//! parents of each. These are the classes codegen puts in the type section, so every function a
//! table there names is kept. From the entries, a function reaches the functions it calls, loads
//! as a value (a routine value, a closure, a method pointer) and the one it is nested in.
//!
//! The kept functions keep their order; the host functions and the extension functions are
//! renumbered in order of first use in the kept functions (function table order, then block and
//! instruction order), so that what a dropped function used does not show. The `class var`s no
//! kept function uses leave the global variables, and the others and the units' ranges are
//! renumbered (`docs/spec/units.md` 6.2, 6.3).

use super::{InstKind, Module, Operand, fits_load_int};
use crate::bytecode::op;
use crate::sema::hir::GlobalId;
use crate::sema::types::{IntfEntry, Ty, TypeId, TypeTable, VmtEntry};

/// The function index of a routine whose function was dropped (`Module::func_index`).
pub const DROPPED: u32 = u32::MAX;

/// Drops the functions of `m` no entry reaches, renumbering the function, host and extension
/// indexes, and then the `class var`s no kept function uses.
pub fn drop_unreachable(m: &mut Module, p: &crate::sema::hir::Program) {
    let (reached, mut rtti) = reachable(m, p);
    let mut new_index = vec![DROPPED; m.funcs.len()];
    let mut next = 0u32;
    for (i, r) in reached.iter().enumerate() {
        if *r {
            new_index[i] = next;
            next += 1;
        }
    }
    let funcs = std::mem::take(&mut m.funcs);
    m.funcs = funcs
        .into_iter()
        .zip(&reached)
        .filter(|(_, r)| **r)
        .map(|(f, _)| f)
        .collect();
    let func = |f: u32| new_index[f as usize];
    let mut hosts = Renumber::new(m.hosts.len());
    let mut exts = Renumber::new(m.exts.len());
    for f in &mut m.funcs {
        f.outer = f.outer.map(func);
        for b in &mut f.blocks {
            for inst in &mut b.insts {
                each_operand_mut(&mut inst.kind, |o| match o {
                    Operand::Func(g) => *g = func(*g),
                    Operand::FuncValue { func: g, .. } => *g = func(*g),
                    Operand::Host(h) => *h = hosts.index(*h),
                    Operand::Ext(x) => *x = exts.index(*x),
                    _ => {}
                });
                // A function value loaded from the constant pool may fit the instruction now
                if inst.op == op::LOAD_CONST
                    && let InstKind::Plain(ops) = &inst.kind
                    && let [_, Operand::FuncValue { func, add }] = ops.as_slice()
                    && fits_load_int(i64::from(*func) + add)
                {
                    inst.op = op::LOAD_INT;
                }
            }
        }
    }
    m.entry = func(m.entry);
    renumber_rtti(&mut rtti, &func);
    m.rtti = rtti;
    for t in &mut m.tests {
        t.func = func(t.func);
    }
    for u in &mut m.units {
        u.init = u.init.map(func);
        u.fini = u.fini.map(func);
    }
    for f in &mut m.func_index {
        // A routine without a function (abstract, generic, `native`) keeps `DROPPED`
        if *f != DROPPED {
            *f = new_index[*f as usize];
        }
    }
    m.hosts = hosts.apply(std::mem::take(&mut m.hosts));
    m.exts = exts.apply(std::mem::take(&mut m.exts));
    drop_unused_class_vars(m);
}

/// Renumbers the functions the entries of the type information call (all kept).
fn renumber_rtti(rtti: &mut [super::RttiType], func: &impl Fn(u32) -> u32) {
    use super::{RttiAttr, RttiCall};
    let call = |c: &mut RttiCall, b: &mut Option<u32>| {
        if let RttiCall::Func(f) = c {
            *f = func(*f);
        }
        if let Some(f) = b {
            *f = func(*f);
        }
    };
    let attrs = |a: &mut [RttiAttr]| {
        for x in a {
            if let Some(f) = &mut x.make {
                *f = func(*f);
            }
        }
    };
    for t in rtti {
        attrs(&mut t.attrs);
        for f in &mut t.fields {
            attrs(&mut f.attrs);
        }
        for p in &mut t.props {
            call(&mut p.read.0, &mut p.read.1);
            call(&mut p.write.0, &mut p.write.1);
            attrs(&mut p.attrs);
        }
        for m in &mut t.methods {
            call(&mut m.call, &mut m.bridge);
            attrs(&mut m.attrs);
            for p in &mut m.params {
                attrs(&mut p.attrs);
            }
        }
    }
}

/// Drops the `class var`s no kept function uses from the global variables (`docs/spec/units.md`
/// 6.2), keeping the order of the others and renumbering them and the units' ranges. The other
/// global variables (the units' `var`s and `ExitCode`) all stay.
fn drop_unused_class_vars(m: &mut Module) {
    let mut used = vec![false; m.globals.len()];
    for f in &m.funcs {
        for b in &f.blocks {
            for inst in &b.insts {
                each_operand(&inst.kind, |o| {
                    if let Operand::Global(g) = o {
                        used[*g as usize] = true;
                    }
                });
            }
        }
    }
    let kept = |i: usize| used[i] || !m.globals[i].class_var;
    if (0..m.globals.len()).all(kept) {
        return;
    }
    let mut new_index = vec![DROPPED; m.globals.len()];
    let mut next = 0u32;
    for (i, n) in new_index.iter_mut().enumerate() {
        if kept(i) {
            *n = next;
            next += 1;
        }
    }
    // A unit's range stays contiguous: its kept globals keep their order, after the kept globals
    // of the units before it (an empty range starts where the next kept global is)
    let kept_before = |end: u32| {
        new_index[..end as usize]
            .iter()
            .filter(|n| **n != DROPPED)
            .count() as u32
    };
    for u in &mut m.units {
        let end = u.first_global + u.global_count;
        u.first_global = kept_before(u.first_global);
        u.global_count = kept_before(end) - u.first_global;
    }
    let globals = std::mem::take(&mut m.globals);
    m.globals = globals
        .into_iter()
        .enumerate()
        .filter(|(i, _)| new_index[*i] != DROPPED)
        .map(|(_, g)| g)
        .collect();
    for f in &mut m.funcs {
        for b in &mut f.blocks {
            for inst in &mut b.insts {
                each_operand_mut(&mut inst.kind, |o| {
                    if let Operand::Global(g) = o {
                        *g = new_index[*g as usize];
                    }
                });
            }
        }
    }
    // The members of the types name the module's numbers (a dropped `class var` maps to
    // `DROPPED`: no global variable); the
    // `class var`s of the generic types stay numbered after the global variables
    // (`sema::hir::Program::global`)
    let old_len = new_index.len() as u32;
    m.types.remap_globals(&|g: GlobalId| {
        GlobalId(match new_index.get(g.0 as usize) {
            Some(n) => *n,
            None => next + (g.0 - old_len),
        })
    });
}

/// Whether an entry reaches each function of `m`, and the types with type information
/// (`docs/spec/rtti.md` chapter 1): the type information the reached functions ask for adds
/// classes to the type section, whose tables reach more functions.
fn reachable(m: &Module, p: &crate::sema::hir::Program) -> (Vec<bool>, Vec<super::RttiType>) {
    let mut demand = super::rtti::Demand::default();
    let mut rtti = super::rtti::Collect::new(p, &m.types, &m.func_index);
    let mut walk = Walk {
        types: &m.types,
        reached: vec![false; m.funcs.len()],
        funcs: Vec::new(),
        classes: vec![false; m.types.defs.len()],
        class_work: Vec::new(),
    };
    walk.func(m.entry);
    for u in &m.units {
        u.init.into_iter().chain(u.fini).for_each(|f| walk.func(f));
    }
    for t in &m.tests {
        walk.func(t.func);
    }
    for u in &m.ext_units {
        for t in u.classes.iter().map(|(_, t)| t).chain(&u.exceptions) {
            walk.ty(*t);
        }
    }
    let mut exts = vec![false; m.exts.len()];
    loop {
        if let Some(i) = walk.funcs.pop() {
            let f = &m.funcs[i as usize];
            if let Some(o) = f.outer {
                walk.func(o);
            }
            for b in &f.blocks {
                for inst in &b.insts {
                    demand.note(inst);
                    each_operand(&inst.kind, |o| match o {
                        Operand::Func(g) | Operand::FuncValue { func: g, .. } => walk.func(*g),
                        Operand::Type(t) | Operand::ClassRef(t) => walk.ty(*t),
                        Operand::TypeInfo(t) => rtti.add(*t),
                        Operand::Ext(x) if !exts[*x as usize] => {
                            exts[*x as usize] = true;
                            let e = &m.exts[*x as usize];
                            if let Some(t) = e.ret_type {
                                walk.ty(t);
                            }
                            walk.shapes(&e.shapes);
                        }
                        _ => {}
                    });
                }
            }
        } else if let Some(c) = walk.class_work.pop() {
            for e in m.types.class_vmt(c) {
                if let VmtEntry::Func(r) = e {
                    walk.func(m.func_index[r.0 as usize]);
                }
            }
            for i in m.types.class_intfs(c) {
                for e in &i.entries {
                    if let IntfEntry::Func(r) = e {
                        walk.func(m.func_index[r.0 as usize]);
                    }
                }
            }
        } else {
            // The type information of what was reached: its classes go into the type section
            // (`docs/spec/rtti.md` 1.2), and the walk goes on with their tables
            let classes: Vec<TypeId> = (0..walk.classes.len())
                .filter(|&k| walk.classes[k])
                .map(|k| TypeId(k as u32))
                .collect();
            let (fresh, funcs) = rtti.close(&demand, &classes);
            let mut more = false;
            for f in funcs {
                more |= !walk.reached[f as usize];
                walk.func(f);
            }
            for c in fresh {
                more |= !walk.classes[c.0 as usize];
                walk.ty(Ty::Class(c));
            }
            if !more {
                break;
            }
        }
    }
    let rtti = if rtti.is_empty() {
        Vec::new()
    } else {
        rtti.finish(&demand)
    };
    (walk.reached, rtti)
}

/// The state of [`reachable`].
struct Walk<'m> {
    types: &'m TypeTable,
    reached: Vec<bool>,
    /// The functions reached whose instructions are still to be read.
    funcs: Vec<u32>,
    /// The classes in the type section.
    classes: Vec<bool>,
    /// The classes whose tables are still to be read.
    class_work: Vec<TypeId>,
}

impl Walk<'_> {
    fn func(&mut self, f: u32) {
        // A routine the entries reach was analyzed and lowered (`sema::finish` marks the others)
        assert_ne!(f, DROPPED, "a reached routine has a function");
        if !self.reached[f as usize] {
            self.reached[f as usize] = true;
            self.funcs.push(f);
        }
    }

    /// A type codegen puts in the type section: a class and its parents.
    fn ty(&mut self, t: Ty) {
        let Ty::Class(mut id) = t else {
            return;
        };
        loop {
            if self.classes[id.0 as usize] {
                return;
            }
            self.classes[id.0 as usize] = true;
            self.class_work.push(id);
            match self.types.parent(id) {
                Some(p) => id = p,
                None => return,
            }
        }
    }

    fn shapes(&mut self, shapes: &[crate::sema::native::NativeShape]) {
        for s in shapes {
            if let Some(t) = s.ty {
                self.ty(t);
            }
            self.shapes(&s.items);
        }
    }
}

/// Numbers of a table renumbered in order of first use.
struct Renumber {
    new: Vec<Option<u32>>,
    /// The old indexes in their new order.
    order: Vec<usize>,
}

impl Renumber {
    fn new(len: usize) -> Self {
        Self {
            new: vec![None; len],
            order: Vec::new(),
        }
    }

    fn index(&mut self, old: u32) -> u32 {
        *self.new[old as usize].get_or_insert_with(|| {
            self.order.push(old as usize);
            self.order.len() as u32 - 1
        })
    }

    /// The entries used, in their new order.
    fn apply<T>(&self, entries: Vec<T>) -> Vec<T> {
        let mut entries: Vec<Option<T>> = entries.into_iter().map(Some).collect();
        self.order
            .iter()
            .map(|&i| {
                entries[i]
                    .take()
                    .expect("an entry is used once in the order")
            })
            .collect()
    }
}

/// Calls `each` with every operand of an instruction that is not a register (the operands of a
/// plain instruction and a call's callee).
fn each_operand(kind: &InstKind, mut each: impl FnMut(&Operand)) {
    match kind {
        InstKind::Plain(ops) => ops.iter().for_each(each),
        InstKind::Call { callee, .. } => each(callee),
    }
}

/// [`each_operand`], with operands it may change.
fn each_operand_mut(kind: &mut InstKind, mut each: impl FnMut(&mut Operand)) {
    match kind {
        InstKind::Plain(ops) => ops.iter_mut().for_each(each),
        InstKind::Call { callee, .. } => each(callee),
    }
}
