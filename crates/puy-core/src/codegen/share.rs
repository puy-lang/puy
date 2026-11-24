//! Sharing the functions of instantiations (`docs/spec/generics.md` 7, `docs/design/generics.md`
//! 3.3 B).
//!
//! Functions made from the same generic routine that come out the same are merged into one,
//! named with the generic form (`Hello.TList<T>.Add`). Two functions are the same when their
//! instructions, handler tables, inline tables, managed registers, line tables, local names, file,
//! enclosing function and numbers of arguments and registers are, comparing
//! - the type operands that only give a shape to the VM (`Unique`, `SetLen`, `NewArr`, `ArrCopy`,
//!   `ArrConcat`, `ArrInsert`, `ArrInsertArr`, `ArrDelete`, `Materialize`, the type word of
//!   `RefFieldSet` / `RefFieldSetRef`, the type of an aggregate or big set constant) by the
//!   contents of the type entry without its name;
//! - the other type operands (`New`, `IsClass`, `AsClass`, `EnumToStr`, a class reference or a
//!   type index loaded as an integer), the globals, the host functions and the interface call
//!   targets by their index;
//! - the constants by their contents;
//! - the functions called, loaded as values or inlined by the class they are in.
//!
//! The classes are found by refining a partition (the functions that call each other are
//! compared together): every function of an instantiation starts in the class of its generic
//! routine, and the classes are split by their contents until no class splits. A merged function
//! takes the place of the first one of its class; the others are removed and every function index
//! after them is renumbered. The type section and the constants are kept as they are (each
//! instantiation keeps its own type entries; 7).

use std::collections::HashMap;

use crate::bytecode::{
    self, ConstEntry, FIRST_TYPE_INDEX, Module, TypeEntry, bx_at, decode_at, op, set_bx_at,
    signed_bx, type_word_at,
};
use crate::sema::hir::GenericOrigin;

/// Shares the functions of `m` (`origins` and `func_values` are per function: where a function
/// of an instantiation comes from, and where it loads function indexes as values, as (word
/// position, amount added)).
pub(super) fn share(
    m: &mut Module,
    origins: &[Option<&GenericOrigin>],
    func_values: &[super::FuncValues],
) {
    if origins.iter().flatten().count() < 2 {
        return;
    }
    let classes = partition(m, origins, func_values);
    // The first function of each class stands for it
    let n = m.funcs.len();
    let mut first: HashMap<usize, usize> = HashMap::new();
    let mut rep = vec![0usize; n];
    for (i, c) in classes.iter().enumerate() {
        rep[i] = *first.entry(*c).or_insert(i);
    }
    if (0..n).all(|i| rep[i] == i) {
        return;
    }
    let mut new_index = vec![0u32; n];
    let mut next = 0u32;
    for i in 0..n {
        if rep[i] == i {
            new_index[i] = next;
            next += 1;
        } else {
            new_index[i] = new_index[rep[i]];
        }
    }
    let Some(new_consts) = value_constants(m, func_values, &new_index) else {
        return;
    };
    m.consts.extend(new_consts.added);
    // The names of the merged functions and the instantiations they stand for
    let mut shared_by: Vec<Vec<String>> = vec![Vec::new(); n];
    for i in 0..n {
        if rep[i] != i
            && let Some(o) = origins[i]
        {
            if shared_by[rep[i]].is_empty() {
                let first_origin = origins[rep[i]].expect("a merged function has an origin");
                shared_by[rep[i]].push(first_origin.instance.clone());
            }
            shared_by[rep[i]].push(o.instance.clone());
        }
    }
    let funcs = std::mem::take(&mut m.funcs);
    for (i, mut f) in funcs.into_iter().enumerate() {
        if rep[i] != i {
            continue;
        }
        renumber_code(
            &mut f.code,
            &func_values[i],
            &new_index,
            &m.consts,
            &new_consts.index,
        );
        f.outer = f.outer.map(|o| new_index[o as usize]);
        for e in &mut f.inlines {
            e.func = new_index[e.func as usize];
        }
        if !shared_by[i].is_empty() {
            f.name = origins[i]
                .expect("a merged function has an origin")
                .generic_name
                .clone();
            f.shared_by = std::mem::take(&mut shared_by[i]);
        }
        m.funcs.push(f);
    }
    let func = |e: u32| -> u32 {
        if (e as usize) < n {
            new_index[e as usize]
        } else {
            e
        }
    };
    for t in &mut m.types {
        for e in &mut t.vmt {
            *e = func(*e);
        }
        for intf in &mut t.intfs {
            for e in &mut intf.funcs {
                *e = func(*e);
            }
        }
    }
    if let Some(rtti) = &mut m.rtti {
        let renumber_target = |target: &mut bytecode::rtti::RttiTarget| {
            if target.kind == bytecode::rtti::TARGET_FUNC {
                target.value = func(target.value);
            }
            target.bridge = func(target.bridge);
        };
        for method in &mut rtti.methods {
            renumber_target(&mut method.call);
        }
        for prop in &mut rtti.props {
            renumber_target(&mut prop.read);
            renumber_target(&mut prop.write);
        }
        for attr in &mut rtti.attrs {
            attr.make = func(attr.make);
        }
    }
    m.entry = func(m.entry);
    for u in &mut m.units {
        u.init = u.init.map(func);
        u.fini = u.fini.map(func);
    }
    for t in &mut m.tests {
        t.func = func(t.func);
    }
}

/// The generic routine and the parts of a function compared as they are ([`fixed_key`]).
type FixedKey = (Option<u32>, Vec<u64>, Vec<String>);

/// The class of each function: a function that is not an instantiation's is alone in its class.
fn partition(
    m: &Module,
    origins: &[Option<&GenericOrigin>],
    func_values: &[super::FuncValues],
) -> Vec<usize> {
    let shapes = TypeShapes::new(&m.types);
    let consts = const_classes(&m.consts, &shapes);
    let n = m.funcs.len();
    // The parts that do not name functions, and the generic routine
    let mut interned: HashMap<FixedKey, usize> = HashMap::new();
    let mut classes: Vec<usize> = (0..n)
        .map(|i| match origins[i] {
            Some(o) => {
                let (words, names) = fixed_key(&m.funcs[i]);
                let len = interned.len();
                *interned
                    .entry((Some(o.def.0), words, names))
                    .or_insert(n + len)
            }
            None => i,
        })
        .collect();
    let mut count = count_classes(&classes);
    loop {
        let mut split: HashMap<(usize, Vec<u64>), usize> = HashMap::new();
        let mut next = classes.clone();
        for i in 0..n {
            if origins[i].is_none() {
                continue;
            }
            let key = (
                classes[i],
                code_key(m, i, &func_values[i], &classes, &shapes, &consts),
            );
            let len = split.len();
            next[i] = *split.entry(key).or_insert(n + len);
        }
        let c = count_classes(&next);
        classes = next;
        if c == count {
            return classes;
        }
        count = c;
    }
}

fn count_classes(classes: &[usize]) -> usize {
    let mut v = classes.to_vec();
    v.sort_unstable();
    v.dedup();
    v.len()
}

/// The parts of a function compared as they are: numbers of arguments and registers, file, line
/// table, local names, managed registers and handler table.
fn fixed_key(f: &bytecode::Function) -> (Vec<u64>, Vec<String>) {
    let mut w: Vec<u64> = vec![
        u64::from(f.params),
        u64::from(f.regs),
        u64::from(f.file),
        f.code.len() as u64,
    ];
    w.push(f.lines.len() as u64);
    for &(p, l) in &f.lines {
        w.extend([u64::from(p), u64::from(l)]);
    }
    w.push(f.managed.len() as u64);
    for r in &f.managed {
        w.extend([u64::from(r.reg), u64::from(r.kind)]);
    }
    w.push(f.handlers.len() as u64);
    for h in &f.handlers {
        w.extend([
            u64::from(h.kind),
            u64::from(h.body_start),
            u64::from(h.body_end),
            u64::from(h.handler_start),
            u64::from(h.handler_end),
            u64::from(h.reg),
        ]);
    }
    // The inline table without the functions inlined (compared by their classes, `code_key`)
    w.push(f.inlines.len() as u64);
    for e in &f.inlines {
        w.extend([
            u64::from(e.start),
            u64::from(e.end),
            u64::from(e.line),
            e.outer.map_or(u64::MAX, u64::from),
        ]);
    }
    let mut names = Vec::with_capacity(f.locals.len());
    for (r, name) in &f.locals {
        w.push(u64::from(*r));
        names.push(name.clone());
    }
    (w, names)
}

/// Tags of the instructions in [`code_key`].
const TAG_RAW: u64 = 0;
const TAG_FUNC: u64 = 1;
const TAG_CONST: u64 = 2;
const TAG_SHAPE: u64 = 3;
const TAG_VALUE: u64 = 4;
const TAG_OUTER: u64 = 5;
const TAG_INLINE: u64 = 6;

/// The instructions of function `i` and its enclosing function, with the operands that name
/// functions, shapes and constants replaced by their classes.
fn code_key(
    m: &Module,
    i: usize,
    values: &[(u32, i64)],
    classes: &[usize],
    shapes: &TypeShapes,
    consts: &[u64],
) -> Vec<u64> {
    let f = &m.funcs[i];
    let mut w: Vec<u64> = Vec::with_capacity(f.code.len() * 2 + 2);
    w.push(TAG_OUTER);
    w.push(f.outer.map_or(u64::MAX, |o| classes[o as usize] as u64));
    w.push(TAG_INLINE);
    w.extend(f.inlines.iter().map(|e| classes[e.func as usize] as u64));
    let mut next_value = 0usize;
    let mut pc = 0usize;
    while pc < f.code.len() {
        let (opc, a, b, c, len) = decode_at(&f.code, pc);
        let head = [len as u64, u64::from(opc), u64::from(a)];
        if let Some(add) = value_at(values, &mut next_value, pc) {
            let func = function_value(&m.consts, &f.code, pc, opc, len) - add;
            w.push(TAG_VALUE);
            w.extend(head);
            w.extend([classes[func as usize] as u64, add as u64]);
        } else if op::resolve(opc, c).is_some_and(|i| i.shape == op::Shape::ABk) {
            w.push(TAG_CONST);
            w.extend(head);
            w.extend([u64::from(b), consts[c as usize]]);
        } else {
            match opc {
                op::CALL | op::CALL_METHOD => {
                    w.push(TAG_FUNC);
                    w.extend(head);
                    w.push(classes[bx_at(&f.code, pc) as usize] as u64);
                }
                op::LOAD_CONST => {
                    w.push(TAG_CONST);
                    w.extend(head);
                    w.push(consts[bx_at(&f.code, pc) as usize]);
                }
                op::UNIQUE
                | op::SET_LEN
                | op::NEW_ARR
                | op::ARR_COPY
                | op::ARR_CONCAT
                | op::ARR_INSERT
                | op::ARR_INSERT_ARR
                | op::ARR_DELETE
                | op::MATERIALIZE => {
                    w.push(TAG_SHAPE);
                    w.extend(head);
                    w.push(shapes.of(bx_at(&f.code, pc)));
                }
                // The words before the type word as they are, the type word by its shape
                _ if bytecode::has_type_word(opc) => {
                    w.extend([TAG_SHAPE, len as u64]);
                    w.extend(f.code[pc..pc + len - 1].iter().map(|x| u64::from(*x)));
                    w.push(shapes.of(type_word_at(&f.code, pc, len)));
                }
                _ => {
                    w.push(TAG_RAW);
                    w.extend(f.code[pc..pc + len].iter().map(|x| u64::from(*x)));
                }
            }
        }
        pc += len;
    }
    w
}

/// The amount added to the function index loaded at `pc`, if the instruction there loads one.
/// `values` is in increasing position order (codegen notes them as it emits), and `next` is the
/// first one not reached yet, so a walk over the code reads each once.
fn value_at(values: &[(u32, i64)], next: &mut usize, pc: usize) -> Option<i64> {
    let &(_, add) = values.get(*next).filter(|(p, _)| *p as usize == pc)?;
    *next += 1;
    Some(add)
}

/// The value a `LoadInt` / `LoadConst` at `pc` loads (a function index plus the amount added).
/// Both comparing and renumbering read it here.
fn function_value(consts: &[ConstEntry], code: &[u32], pc: usize, opc: u16, len: usize) -> i64 {
    let bx = bx_at(code, pc);
    match opc {
        op::LOAD_INT => signed_bx(bx, len == 3),
        op::LOAD_CONST => match consts.get(bx as usize) {
            Some(ConstEntry::Int(v)) => *v,
            c => unreachable!("a function value is an integer constant, not {c:?}"),
        },
        _ => unreachable!("a function value is loaded by `LoadInt` or `LoadConst`"),
    }
}

/// The classes of the type entries by their shape: the entry without its name
/// (`docs/design/generics.md` 3.3 B). A built-in type is its own class.
struct TypeShapes {
    classes: Vec<u64>,
}

impl TypeShapes {
    fn new(types: &[TypeEntry]) -> Self {
        let mut interned: HashMap<TypeEntry, u64> = HashMap::new();
        let classes = types
            .iter()
            .map(|t| {
                let shape = TypeEntry {
                    name: String::new(),
                    ..t.clone()
                };
                let len = interned.len() as u64;
                *interned
                    .entry(shape)
                    .or_insert(u64::from(FIRST_TYPE_INDEX) + len)
            })
            .collect();
        TypeShapes { classes }
    }

    fn of(&self, index: u32) -> u64 {
        match index.checked_sub(FIRST_TYPE_INDEX) {
            Some(i) => self.classes[i as usize],
            None => u64::from(index),
        }
    }
}

/// A constant by its contents, the type of an aggregate or a big set by its shape.
#[derive(PartialEq, Eq, Hash)]
enum ConstKey {
    Int(i64),
    Float(u64),
    Str(String),
    U8(String),
    Utf16(Vec<u16>),
    Agg(u64, Vec<u64>),
    Set(u64, Vec<u64>),
}

/// The class of each constant (an aggregate's elements come before it; `docs/spec/bytecode.md`
/// 3.3).
fn const_classes(consts: &[ConstEntry], shapes: &TypeShapes) -> Vec<u64> {
    let mut interned: HashMap<ConstKey, u64> = HashMap::new();
    let mut out: Vec<u64> = Vec::with_capacity(consts.len());
    for c in consts {
        let key = match c {
            ConstEntry::Int(v) => ConstKey::Int(*v),
            ConstEntry::Float(b) => ConstKey::Float(*b),
            ConstEntry::Str(s) => ConstKey::Str(s.clone()),
            ConstEntry::U8(s) => ConstKey::U8(s.clone()),
            ConstEntry::Utf16(u) => ConstKey::Utf16(u.clone()),
            ConstEntry::Agg { ty, items } => ConstKey::Agg(
                shapes.of(*ty),
                items.iter().map(|k| out[*k as usize]).collect(),
            ),
            ConstEntry::Set { ty, words } => ConstKey::Set(shapes.of(*ty), words.clone()),
        };
        let len = interned.len() as u64;
        out.push(*interned.entry(key).or_insert(len));
    }
    out
}

/// The integer constants the renumbered function values need.
struct NewConsts {
    /// The constants to append to the pool.
    added: Vec<ConstEntry>,
    /// The constant of each value.
    index: HashMap<i64, u32>,
}

/// The constants of the function values loaded by `LoadConst` once renumbered, or `None` if one
/// would not fit its instruction (then nothing is shared: a module that large keeps its
/// functions).
fn value_constants(
    m: &Module,
    func_values: &[super::FuncValues],
    new_index: &[u32],
) -> Option<NewConsts> {
    let mut out = NewConsts {
        added: Vec::new(),
        index: HashMap::new(),
    };
    for (f, values) in m.funcs.iter().zip(func_values) {
        for &(pos, add) in values {
            let (opc, _, _, _, len) = decode_at(&f.code, pos as usize);
            if opc != op::LOAD_CONST {
                continue;
            }
            let old = function_value(&m.consts, &f.code, pos as usize, opc, len);
            let value = i64::from(new_index[(old - add) as usize]) + add;
            if out.index.contains_key(&value) {
                continue;
            }
            let k = match m.consts.iter().position(|c| *c == ConstEntry::Int(value)) {
                Some(k) => k,
                None => {
                    out.added.push(ConstEntry::Int(value));
                    m.consts.len() + out.added.len() - 1
                }
            };
            if len == 1 && k > 0xFFFF {
                return None;
            }
            out.index.insert(value, k as u32);
        }
    }
    Some(out)
}

/// Renumbers the functions named by the instructions of `code`: calls and function values
/// (`consts` is the pool, whose old entries keep their indexes).
fn renumber_code(
    code: &mut [u32],
    values: &[(u32, i64)],
    new_index: &[u32],
    consts: &[ConstEntry],
    new_consts: &HashMap<i64, u32>,
) {
    let mut next_value = 0usize;
    let mut pc = 0usize;
    while pc < code.len() {
        let (opc, _, _, _, len) = decode_at(code, pc);
        if let Some(add) = value_at(values, &mut next_value, pc) {
            let old = function_value(consts, code, pc, opc, len);
            let value = i64::from(new_index[(old - add) as usize]) + add;
            let bx = match opc {
                // A function index only decreases, so the value still fits
                op::LOAD_INT if len == 3 => value as i32 as u32,
                op::LOAD_INT => u32::from(value as i16 as u16),
                _ => *new_consts
                    .get(&value)
                    .expect("`value_constants` gave each renumbered value a constant"),
            };
            set_bx_at(code, pc, bx);
        } else if matches!(opc, op::CALL | op::CALL_METHOD) {
            let f = bx_at(code, pc);
            set_bx_at(code, pc, new_index[f as usize]);
        }
        pc += len;
    }
}
