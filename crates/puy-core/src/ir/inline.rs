//! Inlining small routines (`docs/design/inlining.md`).
//!
//! Codegen runs it on each function before [`super::opt::optimize`], callees before their
//! callers ([`call_order`]): a `Call` / `CallMethod` of a routine that [`may_inline`] allows and
//! that is small once optimized (codegen counts its instructions; [`MAX_SIZE`]) is replaced by a
//! copy of the callee's IR (with the calls inlined into it). The callee's registers go where its
//! frame would be (3.2): a named variable `vN` of the callee is the temporary at the depth of the
//! call's frame plus `N`, a temporary at depth `s` the one at the depth of the frame plus the
//! callee's named variables plus `s`, and the arguments are the call's argument temporaries (or
//! the caller's variables moved into them, [`substitute`]). The blocks of the copy follow the
//! block of the call, and [`Function::inlines`] records them for the stack traces (3.6).

use crate::bytecode::op;
use crate::ir::opt::{names_only_to_read, written};
use crate::ir::{
    Block, BlockId, Function, InlineSite, Inst, InstKind, Module, Operand, Reg, Temp, TempKind,
    Term, VarExpand,
};
use crate::sema::hir::VarKind;
use crate::sema::types::{Ty, TypeTable};
use crate::syntax::ast::ParamMode;

/// The largest callee inlined: instructions codegen lays out for it once optimized (3.5).
pub const MAX_SIZE: u32 = 32;

/// The registers that fit in 8 bits: a call is not inlined if the caller needs more registers
/// than that and than it had (3.5).
const NARROW_REGS: u32 = 256;

/// The functions in an order where a function comes after those it calls (`Call` /
/// `CallMethod`) unless they call each other, and per function whether it calls itself, directly
/// or through others (Tarjan's strongly connected components, without recursion).
pub fn call_order(m: &Module) -> (Vec<usize>, Vec<bool>) {
    let n = m.funcs.len();
    let edges: Vec<Vec<usize>> = m
        .funcs
        .iter()
        .map(|f| {
            let mut out = Vec::new();
            for b in &f.blocks {
                for i in &b.insts {
                    if let Some(g) = static_callee(i) {
                        out.push(g as usize);
                    }
                }
            }
            out.sort_unstable();
            out.dedup();
            out
        })
        .collect();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut order = Vec::with_capacity(n);
    let mut recursive = vec![false; n];
    let mut next = 0usize;
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        // (function, next edge to look at)
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (v, ref mut e)) = work.last_mut() {
            if let Some(&w) = edges[v].get(*e) {
                *e += 1;
                if w == v {
                    recursive[v] = true;
                }
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(u, _)) = work.last() {
                low[u] = low[u].min(low[v]);
            }
            if low[v] == index[v] {
                let mut comp = Vec::new();
                loop {
                    let w = stack.pop().expect("the component is on the stack");
                    on_stack[w] = false;
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                if comp.len() > 1 {
                    for &w in &comp {
                        recursive[w] = true;
                    }
                }
                order.extend(comp);
            }
        }
    }
    (order, recursive)
}

/// The function a `Call` / `CallMethod` calls.
fn static_callee(inst: &Inst) -> Option<u32> {
    match (&inst.kind, inst.op) {
        (
            InstKind::Call {
                callee: Operand::Func(g),
                ..
            },
            op::CALL | op::CALL_METHOD,
        ) => Some(*g),
        _ => None,
    }
}

/// Whether `f` (its IR with the calls inlined into it) may be inlined, whatever its size
/// (3.3, 3.4): not a constructor or destructor, no static link and no routine nested in it
/// (`is_outer`), no cell, no managed register released at its end, no handler table, no loan
/// ([`frame_bound`]), and only
/// variables whose registers hold plain values (the ones a call sets to 0, which the inlined
/// copy sets to 0 itself).
pub fn may_inline(f: &Function, types: &TypeTable, is_outer: bool) -> bool {
    !f.lifecycle
        && !f.link
        && !is_outer
        && f.finally.is_empty()
        && f.tries.is_empty()
        && f.temps.iter().all(|t| t.slot.is_some())
        && f.blocks
            .iter()
            .flat_map(|b| &b.insts)
            .all(|i| !frame_bound(i))
        && f.vars.iter().enumerate().all(|(i, v)| {
            !v.shared
                && !matches!(
                    v.kind,
                    VarKind::Cell { .. } | VarKind::Outer { .. } | VarKind::Exception
                )
                && (i < f.params as usize
                    || matches!(v.expand, VarExpand::Whole { .. })
                    || !types.managed(v.ty))
        })
}

/// Whether an instruction works on the loans of its frame (`docs/spec/bytecode.md` 2.8: a
/// frame's loans are those of one call, and `Unlend` gives back all of them), which an inlined
/// copy would mix with the caller's.
pub(crate) fn frame_bound(inst: &Inst) -> bool {
    match (inst.op, &inst.kind) {
        (op::LEND | op::LEND_PLACE | op::UNLEND, _) => true,
        (op::INTRINSIC, InstKind::Plain(ops)) => {
            ops.last() == Some(&Operand::Imm(i64::from(op::intr::PLACE_BOX)))
        }
        _ => false,
    }
}

/// The number of registers codegen gives a function: the static link, the named variables, the
/// owning temporaries, and the stack of the others.
pub(crate) fn registers(f: &Function) -> u32 {
    let owning = f.temps.iter().filter(|t| t.slot.is_none()).count() as u32;
    let stack = f
        .temps
        .iter()
        .filter_map(|t| t.slot)
        .map(|s| s + 1)
        .max()
        .unwrap_or(0);
    f.vars.len() as u32 + u32::from(f.link) + owning + stack
}

/// `f` with the calls of the functions `callee` gives inlined. `callee(g)` is the IR of `g` (with
/// the calls inlined into it) if it may be inlined and is small enough.
pub fn inline_calls<'a>(f: &Function, callee: &dyn Fn(u32) -> Option<&'a Function>) -> Function {
    let mut f = f.clone();
    let mut b = 0usize;
    'blocks: while b < f.blocks.len() {
        for k in 0..f.blocks[b].insts.len() {
            let Some(g) = static_callee(&f.blocks[b].insts[k]).and_then(callee) else {
                continue;
            };
            if let Some(cont) = splice(&mut f, b, k, g) {
                // The inlined blocks hold no call left to inline: go on after them
                b = cont;
                continue 'blocks;
            }
        }
        b += 1;
    }
    f
}

/// Inlines the call `k` of block `b` of `f`, calling `g`. Returns the block after the inlined
/// ones (holding the instructions after the call), or `None` if the call is not inlined.
fn splice(f: &mut Function, b: usize, k: usize, g: &Function) -> Option<usize> {
    let call = f.blocks[b].insts[k].clone();
    let InstKind::Call {
        dst,
        callee: Operand::Func(callee),
        args,
        frame,
    } = &call.kind
    else {
        return None;
    };
    let Reg::Temp(frame_t) = *frame else {
        return None;
    };
    let depth = f.temps[frame_t as usize].slot?;
    if args.len() != g.params as usize {
        return None;
    }
    for (i, a) in args.iter().enumerate() {
        match a {
            Reg::Temp(t) if f.temps[*t as usize].slot == Some(depth + i as u32) => {}
            _ => return None,
        }
    }
    let method = call.op == op::CALL_METHOD;
    if method && args.is_empty() {
        return None;
    }
    // The registers: the callee's frame where the call put it, and the check of `Self` above
    let callee_regs = registers(g);
    let named = f.vars.len() as u32 + u32::from(f.link);
    let owning = f.temps.iter().filter(|t| t.slot.is_none()).count() as u32;
    let need = named + owning + depth + callee_regs + u32::from(method);
    if need > NARROW_REGS.max(registers(f)) {
        return None;
    }
    let nblocks = g.blocks.len();
    let first = b + 1;
    let cont = first + nblocks;
    let line = call.line;
    let params = g.params as usize;
    let subst = substitute(f, b, k, g, args);

    // Registers of the callee in the caller
    let gvars = g.vars.len() as u32;
    let mut var_regs: Vec<Reg> = Vec::with_capacity(g.vars.len());
    for (n, v) in g.vars.iter().enumerate() {
        if n < params {
            var_regs.push(subst[n].map_or(args[n], |s| s.0));
        } else {
            f.temps.push(Temp {
                ty: Some(v.ty),
                kind: TempKind::Value,
                slot: Some(depth + n as u32),
            });
            var_regs.push(Reg::Temp(f.temps.len() as u32 - 1));
        }
    }
    let first_temp = f.temps.len() as u32;
    for t in &g.temps {
        f.temps.push(Temp {
            ty: t.ty,
            kind: t.kind,
            slot: Some(depth + gvars + t.slot.expect("an inlined routine has no owning temporary")),
        });
    }
    let map = |r: &mut Reg| {
        *r = match *r {
            Reg::Var(n) => var_regs[n as usize],
            Reg::Temp(t) => Reg::Temp(first_temp + t),
            Reg::Link => unreachable!("an inlined routine takes no static link"),
        }
    };

    // The caller's blocks after `b` move past the inlined ones and the continuation
    let shift = (nblocks + 1) as BlockId;
    let bb = b as BlockId;
    let moved = |t: &mut BlockId| {
        if *t > bb {
            *t += shift
        }
    };
    for block in &mut f.blocks {
        block.term.each_target_mut(moved);
    }
    let range = |(s, e): (BlockId, BlockId)| {
        (
            if s > bb { s + shift } else { s },
            if e >= bb { e + shift } else { e },
        )
    };
    for t in &mut f.tries {
        t.body = range(t.body);
        t.handler = range(t.handler);
    }
    for s in &mut f.inlines {
        s.blocks = range(s.blocks);
    }

    // Split the block of the call
    let tail: Vec<Inst> = f.blocks[b].insts.drain(k..).skip(1).collect();
    let tail_term = std::mem::replace(&mut f.blocks[b].term, Term::Jmp(first as BlockId));
    let tail_line = std::mem::replace(&mut f.blocks[b].term_line, line);
    // Drop the moves into the arguments replaced by the caller's variables (later ones first,
    // so that the positions of the others hold)
    let mut drops: Vec<(usize, usize)> = subst
        .iter()
        .flatten()
        .map(|&(_, blk, i)| (blk, i))
        .collect();
    drops.sort_unstable();
    for &(blk, i) in drops.iter().rev() {
        f.blocks[blk].insts.remove(i);
    }
    if method {
        // `CallMethod` checks `Self` before entering the routine (`docs/spec/objects.md` 14.4):
        // the same check, at the call (its result is not used)
        f.temps.push(Temp {
            ty: Some(Ty::Integer),
            kind: TempKind::Value,
            slot: Some(depth + callee_regs),
        });
        let check = Reg::Temp(f.temps.len() as u32 - 1);
        f.blocks[b].insts.push(Inst {
            op: op::INTRINSIC,
            kind: InstKind::Plain(vec![
                Operand::Reg(check),
                Operand::Reg(var_regs[0]),
                Operand::Imm(i64::from(op::intr::CLASS_TYPE)),
            ]),
            line,
        });
    }

    // The callee's blocks
    let zero = zero_on_entry(g);
    let mut inlined: Vec<Block> = Vec::with_capacity(nblocks);
    for (j, src) in g.blocks.iter().enumerate() {
        let mut insts = Vec::with_capacity(src.insts.len() + 1);
        if j == 0 {
            let at = src.insts.first().map_or(src.term_line, |i| i.line);
            for n in (params..g.vars.len()).filter(|&n| zero[n]) {
                insts.push(Inst {
                    op: op::LOAD_ZERO,
                    kind: InstKind::Plain(vec![Operand::Reg(var_regs[n])]),
                    line: at,
                });
            }
        }
        for inst in &src.insts {
            let mut inst = inst.clone();
            inst.each_reg_mut(map);
            insts.push(inst);
        }
        let term = match &src.term {
            Term::Ret(r) => {
                if let Some(d) = dst {
                    let mut r = *r;
                    map(&mut r);
                    insts.push(Inst {
                        op: op::MOVE,
                        kind: InstKind::Plain(vec![Operand::Reg(*d), Operand::Reg(r)]),
                        line: src.term_line,
                    });
                }
                Term::Jmp(cont as BlockId)
            }
            Term::RetVoid => Term::Jmp(cont as BlockId),
            t => {
                let mut t = t.clone();
                t.each_reg_mut(map);
                t.each_target_mut(|x| *x += first as BlockId);
                t
            }
        };
        inlined.push(Block {
            insts,
            term,
            term_line: src.term_line,
        });
    }
    inlined.push(Block {
        insts: tail,
        term: tail_term,
        term_line: tail_line,
    });
    f.blocks.splice(first..first, inlined);

    // The inline table: this call, then the calls inlined into the callee. The call is in no
    // other site (the inlined blocks are not looked at again)
    let site = f.inlines.len() as u32;
    let at = |x: BlockId| x + first as BlockId;
    // The sites come in block order; a site inlined later comes after the ones before it
    let pos = f
        .inlines
        .iter()
        .position(|s| s.blocks.0 > bb)
        .unwrap_or(f.inlines.len());
    debug_assert_eq!(pos as u32, site, "calls are inlined in block order");
    f.inlines.push(InlineSite {
        blocks: (first as BlockId, (cont - 1) as BlockId),
        callee: *callee,
        line,
        outer: None,
    });
    for s in &g.inlines {
        f.inlines.push(InlineSite {
            blocks: (at(s.blocks.0), at(s.blocks.1)),
            callee: s.callee,
            line: s.line,
            outer: Some(s.outer.map_or(site, |o| site + 1 + o)),
        });
    }
    Some(cont)
}

/// For each argument of the call `k` of block `b`, the caller's variable to use in its place,
/// and where the move into the argument is (block, instruction): the argument is moved from a
/// variable that only the instructions naming it write (as `opt`'s stable variables), nothing
/// between writes the variable, and the callee only reads the parameter (3.2; `L[I]` reads `L`
/// and `I`). The move may be in the block of the call or before calls inlined earlier on the
/// way to it (`L[I] := L[I] + 1`: the arguments of `SetItem` are moved before `GetItem` is
/// inlined).
fn substitute(
    f: &Function,
    b: usize,
    k: usize,
    g: &Function,
    args: &[Reg],
) -> Vec<Option<(Reg, usize, usize)>> {
    args.iter()
        .enumerate()
        .map(|(i, &a)| {
            if !only_read(g, Reg::Var(i as u32)) {
                return None;
            }
            // The instructions between the move and the call, block by block backwards
            let mut between: Vec<&Inst> = Vec::new();
            let (mut cb, mut end) = (b, k);
            let (x, at) = loop {
                let insts = &f.blocks[cb].insts[..end];
                let named = |inst: &Inst| {
                    let mut named = false;
                    inst.each_reg(|r| named |= r == a);
                    named
                };
                if let Some(pos) = insts.iter().rposition(named) {
                    let x = match (insts[pos].op, &insts[pos].kind) {
                        (op::MOVE, InstKind::Plain(ops)) => match ops[..] {
                            [Operand::Reg(d), Operand::Reg(x @ Reg::Var(_))] if d == a => x,
                            _ => return None,
                        },
                        _ => return None,
                    };
                    between.extend(&insts[pos + 1..]);
                    break (x, (cb, pos));
                }
                between.extend(insts);
                // The block right after a call inlined earlier: its only way in is through the
                // inlined blocks, and theirs through the block of that call
                let s = f
                    .inlines
                    .iter()
                    .find(|s| s.outer.is_none() && s.blocks.1 as usize + 1 == cb)?;
                for blk in s.blocks.0..=s.blocks.1 {
                    let insts = &f.blocks[blk as usize].insts;
                    if insts.iter().any(named) {
                        return None;
                    }
                    between.extend(insts);
                }
                cb = s.blocks.0 as usize - 1;
                end = f.blocks[cb].insts.len();
            };
            if !stable_var(f, x) {
                return None;
            }
            // Between the move and the call, `x` is only read (an argument of a call is read)
            let untouched = between.iter().all(|inst| match &inst.kind {
                InstKind::Plain(_) => names_only_to_read(inst, x) != Some(false),
                InstKind::Call { .. } => only_arg_or_none(inst, x),
            });
            untouched.then_some((x, at.0, at.1))
        })
        .collect()
}

/// Whether the call `inst` names `x` at most as an argument (which it reads).
fn only_arg_or_none(inst: &Inst, x: Reg) -> bool {
    match &inst.kind {
        InstKind::Call {
            dst, callee, frame, ..
        } => *dst != Some(x) && *frame != x && *callee != Operand::Reg(x),
        InstKind::Plain(_) => false,
    }
}

/// Whether the named variable `x` of `f` holds a plain value that only the instructions naming it
/// write: no reference to it is made, no other routine reaches it.
fn stable_var(f: &Function, x: Reg) -> bool {
    let Reg::Var(v) = x else {
        return false;
    };
    let var = &f.vars[v as usize];
    let plain = matches!(
        var.kind,
        VarKind::Local
            | VarKind::Result
            | VarKind::Hidden
            | VarKind::SelfParam
            | VarKind::Param(ParamMode::Value)
    );
    let referenced = |i: &Inst| {
        i.op == op::MAKE_REF
            && matches!(&i.kind, InstKind::Plain(ops) if ops.get(1) == Some(&Operand::Reg(x)))
    };
    plain
        && !var.shared
        && var.expand == VarExpand::No
        && !f.blocks.iter().flat_map(|b| &b.insts).any(referenced)
}

/// Whether every instruction of `g` naming `r` only reads it (the terminators only read).
fn only_read(g: &Function, r: Reg) -> bool {
    g.blocks
        .iter()
        .flat_map(|b| &b.insts)
        .all(|inst| names_only_to_read(inst, r) != Some(false))
}

/// Per named variable of `g`: whether the inlined copy sets it to 0 on entry, as the call does
/// (3.2): a variable other than a parameter that some path from the entry may read before it
/// writes it.
fn zero_on_entry(g: &Function) -> Vec<bool> {
    let params = g.params as usize;
    let nvars = g.vars.len();
    let n = g.blocks.len();
    let mut need = vec![false; nvars];
    if n == 0 {
        return need;
    }
    // Per block: the variables that may not be written yet when it starts
    let mut unset: Vec<Option<Vec<bool>>> = vec![None; n];
    unset[0] = Some((0..nvars).map(|v| v >= params).collect());
    let mut work = vec![0usize];
    while let Some(bi) = work.pop() {
        let mut state = unset[bi]
            .clone()
            .expect("a block on the work list has a state");
        let block = &g.blocks[bi];
        let reads = |state: &[bool], need: &mut Vec<bool>, r: Reg| {
            if let Reg::Var(v) = r
                && state[v as usize]
            {
                need[v as usize] = true;
            }
        };
        for inst in &block.insts {
            let w = written(inst);
            let mut occurrences: Vec<Reg> = Vec::new();
            inst.each_reg(|r| occurrences.push(r));
            for (j, &r) in occurrences.iter().enumerate() {
                // The written register is named once as the destination; any other naming reads
                let is_dest = Some(r) == w
                    && occurrences[..j].iter().all(|&o| o != r)
                    && is_destination(inst, r);
                if !is_dest {
                    reads(&state, &mut need, r);
                }
            }
            if let Some(Reg::Var(v)) = w {
                state[v as usize] = false;
            }
        }
        block.term.each_reg(|r| reads(&state, &mut need, r));
        block.term.each_target(|t| {
            let t = t as usize;
            match &mut unset[t] {
                None => {
                    unset[t] = Some(state.clone());
                    work.push(t);
                }
                Some(s) => {
                    let mut changed = false;
                    for (x, &y) in s.iter_mut().zip(&state) {
                        if y && !*x {
                            *x = true;
                            changed = true;
                        }
                    }
                    if changed {
                        work.push(t);
                    }
                }
            }
        });
    }
    need
}

/// Whether `r` is where `inst` writes its result: the first operand of a plain instruction, or a
/// call's destination.
fn is_destination(inst: &Inst, r: Reg) -> bool {
    match &inst.kind {
        InstKind::Plain(ops) => ops.first() == Some(&Operand::Reg(r)),
        InstKind::Call { dst, .. } => *dst == Some(r),
    }
}
