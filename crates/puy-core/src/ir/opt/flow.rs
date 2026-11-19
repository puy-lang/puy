//! Rules 13 to 15: the copies and branches left where routines were inlined
//! (`docs/design/inlining.md` 7.2).
//!
//! An inlined routine takes its arguments and gives its result through temporaries: the copies
//! cross blocks (the routine's branches), which the rules of one block do not see, and a
//! `Boolean` result is tested once in the routine and again after the copy. The rules follow the
//! registers codegen gives (temporaries share them; [`Ctx::reg`]) across the blocks:
//!
//! 13. [`copies_across`]: `Move t, x`: the readers of `t` that see the move's value on every path,
//!     with `x`'s register still holding it, read `x`, and the moves go.
//! 14. [`known_tests`], [`compare_and_branch`]: a jump or a test going to an empty block that
//!     tests a `Boolean` whose value is known there (tested on every way in, nothing writing it
//!     since) goes where that test leads; a test only one block leads to moves up into it; a
//!     compare whose result only the test right after it reads becomes a compare-and-branch.
//! 15. [`coalesce`]: `Move t, x`, where `t` is an operand of an instruction that wants its
//!     registers in a row (`HashLookupInt`, a call's arguments): `x` is computed in `t`'s register,
//!     when nothing else uses that register while `x` lives.
//!
//! They only look at temporaries and stable variables ([`Ctx::stable`]) of types that are not
//! managed, never move an instruction that may raise, and leave the functions that lend
//! (`Lend`, whose `Unlend` writes registers it does not name) alone. Where they drop or move a
//! write of a register, they make sure that nothing else reads that register before it is
//! written again ([`read_later`]; rule 5 drops `Move y, t` of two temporaries of one register,
//! after which `y`'s readers read `t`'s value without naming `t`), and they keep the registers
//! that array instructions read without naming them ([`Flow::unnamed`]).
//!
//! Each candidate is checked by walking only the blocks around it (forward from a write up to the
//! next write of the register, back from the readers up to the writes), never the whole function,
//! so that a long function with many inlined calls stays about as fast to compile as before.

use super::{Ctx, Use, as_const, as_move, fuse_term, only_reads, operand_use, term_reads};
use crate::bytecode::op;
use crate::ir::inline::frame_bound;
use crate::ir::{BlockId, Function, Inst, InstKind, Operand, Reg, Term};

/// Rules 13 to 15 on `f` (after the rules of one block).
pub(super) fn copies_and_branches(f: &mut Function, ctx: &Ctx) {
    if f.blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|i| frame_bound(i) || i.op == op::UNLEND)
    {
        return;
    }
    let flow = Flow::of(f, ctx);
    // A copy taken away may leave another one whose readers it held (`Move d, r` after `Move r, x`)
    for _ in 0..3 {
        if !copies_across(f, ctx, &flow) {
            break;
        }
    }
    let before = reachable(f);
    if known_tests(f, ctx, &flow) {
        drop_dead(f, &before);
    }
    // The blocks may have moved (`drop_dead`)
    let flow = Flow::of(f, ctx);
    compare_and_branch(f, ctx, &flow);
    coalesce(f, ctx, &flow);
}

/// What the rules know of a function's temporaries and handler table.
struct Flow {
    /// Per temporary: whether it is on the stack of temporaries, where a call's frame may cover it.
    stack: Vec<bool>,
    /// Per temporary: whether a reference to it is made (`MakeRef`), through which a call or
    /// `StoreRef` may write it.
    referenced: Vec<bool>,
    /// Per temporary: whether the handler table or the registers released at the end name it.
    pinned: Vec<bool>,
    /// The registers the VM writes when it enters a handler, and the one after a `finally`'s
    /// state (`EndFinally` reads it).
    reserved: Vec<u32>,
    /// Per block: the first blocks of the handlers an exception raised in it may go to.
    handlers: Vec<Vec<BlockId>>,
    /// The registers some instruction may read without naming them: those after `rA` of the array
    /// instructions with a type operand (`ArrInsert` reads `r(A+1)`, `r(A+2)`; `lower`'s
    /// `array_edit`; `docs/spec/bytecode.md` 2.8).
    unnamed: Vec<u32>,
}

impl Flow {
    fn of(f: &Function, ctx: &Ctx) -> Flow {
        let n = f.temps.len();
        let mut referenced = vec![false; n];
        for b in &f.blocks {
            for i in &b.insts {
                if let (op::MAKE_REF, InstKind::Plain(ops)) = (i.op, &i.kind)
                    && let Some(Operand::Reg(Reg::Temp(t))) = ops.get(1)
                {
                    referenced[*t as usize] = true;
                }
            }
        }
        let mut pinned = vec![false; n];
        let mut reserved = Vec::new();
        let mut pin = |r: Reg| {
            if let Reg::Temp(t) = r {
                pinned[t as usize] = true;
            }
        };
        for t in &f.tries {
            pin(t.reg);
            reserved.push(ctx.reg(t.reg));
            reserved.push(ctx.reg(t.reg).wrapping_add(1));
            if let Some(e) = t.exc {
                pin(e);
                reserved.push(ctx.reg(e));
            }
        }
        for r in &f.finally {
            pin(*r);
        }
        let mut handlers = vec![Vec::new(); f.blocks.len()];
        for t in &f.tries {
            for b in t.body.0..=t.body.1 {
                handlers[b as usize].push(t.handler.0);
            }
        }
        let mut unnamed = Vec::new();
        for b in &f.blocks {
            for i in &b.insts {
                if let InstKind::Plain(ops) = &i.kind
                    && matches!(
                        i.op,
                        op::SET_LEN
                            | op::ARR_COPY
                            | op::ARR_CONCAT
                            | op::ARR_INSERT
                            | op::ARR_INSERT_ARR
                            | op::ARR_DELETE
                    )
                    && let Some(Operand::Reg(a)) = ops.first()
                {
                    let a = ctx.reg(*a);
                    unnamed.extend([a.wrapping_add(1), a.wrapping_add(2)]);
                }
            }
        }
        Flow {
            unnamed,
            stack: f.temps.iter().map(|t| t.slot.is_some()).collect(),
            referenced,
            pinned,
            reserved,
            handlers,
        }
    }

    /// Whether the rules may follow the value of `r` by its register: a stable variable or a
    /// constant loaded before a loop ([`Ctx::stable`]), or a temporary of a type that is not
    /// managed that the handler table does not name (when a reference to it is made, every call
    /// and every write through a reference may write it; [`Flow::clobbers`]).
    fn followed(&self, ctx: &Ctx, r: Reg) -> bool {
        match r {
            Reg::Temp(t) if t < ctx.first_hoisted => ctx.plain_temp(r) && !self.pinned[t as usize],
            _ => ctx.stable(r),
        }
    }

    /// Whether `r` is a temporary [`Flow::followed`] that the rules may rename or drop the writes
    /// of: not a constant loaded before a loop, no reference to it is made, and no instruction
    /// reads its register without naming it ([`Flow::unnamed`]).
    fn own_temp(&self, ctx: &Ctx, r: Reg) -> bool {
        matches!(r, Reg::Temp(t) if t < ctx.first_hoisted && !self.referenced[t as usize])
            && self.followed(ctx, r)
            && !self.unnamed.contains(&ctx.reg(r))
    }

    /// Whether `inst` may write the register of `r`: as its result, through a call's frame (which
    /// covers the stack of temporaries from its first register up), or as any register operand of
    /// an instruction [`operand_use`] does not know; for a temporary a reference reaches, also any
    /// call, `StoreRef` and instruction [`operand_use`] does not know.
    fn clobbers(&self, ctx: &Ctx, inst: &Inst, r: Reg) -> bool {
        let p = ctx.reg(r);
        if matches!(r, Reg::Temp(t) if t < ctx.first_hoisted && self.referenced[t as usize])
            && (matches!(inst.kind, InstKind::Call { .. })
                || inst.op == op::STORE_REF
                || operand_use(inst).is_none())
        {
            return true;
        }
        match &inst.kind {
            InstKind::Call { dst, frame, .. } => {
                let on_stack =
                    matches!(r, Reg::Temp(t) if t < ctx.first_hoisted && self.stack[t as usize]);
                dst.is_some_and(|d| ctx.reg(d) == p) || (on_stack && ctx.reg(*frame) <= p)
            }
            InstKind::Plain(ops) => match operand_use(inst) {
                Some(Use::WritesFirst) => matches!(ops[0], Operand::Reg(d) if ctx.reg(d) == p),
                Some(Use::Reads) => false,
                None => ctx.touches(inst, r),
            },
        }
    }
}

/// Whether a terminator may write the register of `r`: a `for` loop's counter, a `finally`'s
/// state (and the exception after it).
fn term_clobbers(ctx: &Ctx, term: &Term, r: Reg) -> bool {
    let p = ctx.reg(r);
    match term {
        Term::ForLoop { counter, .. } => ctx.reg(*counter) == p,
        Term::EndFinally { state, .. } => {
            let s = ctx.reg(*state);
            s == p || s.wrapping_add(1) == p
        }
        _ => false,
    }
}

/// Per block: whether a way from the entry (or into a handler) reaches it.
fn reachable(f: &Function) -> Vec<bool> {
    let mut seen = vec![false; f.blocks.len()];
    let mut work: Vec<BlockId> = vec![0];
    work.extend(f.tries.iter().map(|t| t.handler.0));
    while let Some(b) = work.pop() {
        if std::mem::replace(&mut seen[b as usize], true) {
            continue;
        }
        f.blocks[b as usize].term.each_target(|t| work.push(t));
    }
    seen
}

/// Per block: the reachable blocks whose terminators go to it (each once).
fn predecessors(f: &Function, reach: &[bool]) -> Vec<Vec<BlockId>> {
    let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); f.blocks.len()];
    for (b, block) in f.blocks.iter().enumerate() {
        if !reach[b] {
            continue;
        }
        block.term.each_target(|t| {
            let p = &mut preds[t as usize];
            if p.last() != Some(&(b as BlockId)) {
                p.push(b as BlockId);
            }
        });
    }
    preds
}

/// Whether the block is where the function or a handler starts (entered other than by a jump).
fn entered(f: &Function, b: BlockId) -> bool {
    b == 0 || f.tries.iter().any(|t| t.handler.0 == b)
}

/// Where an instruction or terminator is: block, and the instruction (`None` for the terminator).
type At = (usize, Option<usize>);

/// The places that name each temporary, in block order.
fn occurrences(f: &Function) -> Vec<Vec<At>> {
    let mut occ: Vec<Vec<At>> = vec![Vec::new(); f.temps.len()];
    for (b, block) in f.blocks.iter().enumerate() {
        for (i, inst) in block.insts.iter().enumerate() {
            inst.each_reg(|r| {
                if let Reg::Temp(t) = r {
                    let o = &mut occ[t as usize];
                    if o.last() != Some(&(b, Some(i))) {
                        o.push((b, Some(i)));
                    }
                }
            });
        }
        block.term.each_reg(|r| {
            if let Reg::Temp(t) = r {
                let o = &mut occ[t as usize];
                if o.last() != Some(&(b, None)) {
                    o.push((b, None));
                }
            }
        });
    }
    occ
}

/// Rule 13: a temporary `t` written by `Move t, x` (one `x` for all of them) and otherwise only by
/// instructions whose register operands may be any register ([`operand_use`]), read only by such
/// instructions and by terminators. A reader that, on every path to it, sees the value of one of
/// the moves while `x`'s register still holds it (a forward data flow; the entry and the handlers
/// start without) reads `x` instead; when every other reader sees no move's value on any path, the
/// moves go (`Move t, k` before the branches of an inlined routine, `Move d, r` from its result to
/// the call's destination). Returns whether something changed.
fn copies_across(f: &mut Function, ctx: &Ctx, flow: &Flow) -> bool {
    let mut changed = false;
    let reach = reachable(f);
    let preds = predecessors(f, &reach);
    let mut occ = occurrences(f);
    let mut marks = Marks::new(f.blocks.len());
    let mut states = vec![((false, false), (false, false)); f.blocks.len()];
    // The moves taken away, dropped at the end (so that the places stay where they are)
    let mut gone: Vec<Vec<bool>> = f
        .blocks
        .iter()
        .map(|b| vec![false; b.insts.len()])
        .collect();
    for n in 0..ctx.first_hoisted.min(f.temps.len() as u32) {
        let t = Reg::Temp(n);
        if !flow.own_temp(ctx, t) {
            continue;
        }
        let places: Vec<At> = occ[n as usize]
            .iter()
            .copied()
            .filter(|&(b, i)| i.is_none_or(|i| !gone[b][i]))
            .collect();
        let Some(web) = Web::of(f, t, &places) else {
            continue;
        };
        let x = web.x;
        if !flow.followed(ctx, x) || ctx.reg(x) == ctx.reg(t) {
            continue;
        }
        let Some(swapped) =
            web.readers_of_x(f, ctx, flow, &reach, &preds, &gone, &mut marks, &mut states)
        else {
            continue;
        };
        let after_moves: Vec<(usize, usize)> = web.moves.iter().map(|&(b, i)| (b, i + 1)).collect();
        if swapped.is_empty() || read_later(f, ctx, flow, &gone, &mut marks, &after_moves, t, &[t])
        {
            continue;
        }
        for &(b, i) in &swapped {
            match i {
                Some(i) => {
                    let inst = &mut f.blocks[b].insts[i];
                    // The result an instruction writes stays
                    let keep_first = operand_use(inst) == Some(Use::WritesFirst)
                        && matches!(&inst.kind, InstKind::Plain(ops) if ops[0] == Operand::Reg(t));
                    if let InstKind::Plain(ops) = &mut inst.kind {
                        for o in ops.iter_mut().skip(usize::from(keep_first)) {
                            if *o == Operand::Reg(t) {
                                *o = Operand::Reg(x);
                            }
                        }
                    }
                }
                None => f.blocks[b].term.each_reg_mut(|r| {
                    if *r == t {
                        *r = x;
                    }
                }),
            }
        }
        for &(b, i) in &web.moves {
            gone[b][i] = true;
        }
        changed = true;
        occ[n as usize].retain(|p| !web.moves.iter().any(|&(b, i)| *p == (b, Some(i))));
        if let Reg::Temp(u) = x {
            let o = &mut occ[u as usize];
            o.extend(swapped.iter().copied());
            o.sort_unstable();
            o.dedup();
        }
    }
    drop_gone(f, &gone);
    changed
}

/// The places naming a temporary `t` for rule 13.
struct Web {
    /// The source of the moves.
    x: Reg,
    /// `Move t, x`.
    moves: Vec<(usize, usize)>,
    /// The other instructions writing `t` (which may read it first).
    writers: Vec<(usize, usize)>,
    /// The places reading `t`.
    readers: Vec<At>,
}

impl Web {
    /// The places of `t`, if they are all of the kinds rule 13 deals with.
    fn of(f: &Function, t: Reg, places: &[At]) -> Option<Web> {
        let mut x = None;
        let mut moves = Vec::new();
        let mut writers = Vec::new();
        let mut readers = Vec::new();
        for &(b, i) in places {
            let Some(i) = i else {
                if !term_reads(&f.blocks[b].term, t).1 {
                    return None;
                }
                readers.push((b, None));
                continue;
            };
            let inst = &f.blocks[b].insts[i];
            if let Some((d, s)) = as_move(inst)
                && d == t
                && s != t
            {
                if *x.get_or_insert(s) != s {
                    return None;
                }
                moves.push((b, i));
                continue;
            }
            let InstKind::Plain(ops) = &inst.kind else {
                return None;
            };
            match operand_use(inst)? {
                Use::WritesFirst if ops[0] == Operand::Reg(t) => {
                    writers.push((b, i));
                    if ops[1..].contains(&Operand::Reg(t)) {
                        readers.push((b, Some(i)));
                    }
                }
                _ => readers.push((b, Some(i))),
            }
        }
        Some(Web {
            x: x?,
            moves,
            writers,
            readers,
        })
    }

    /// The readers that see a move's value with `x`'s register holding it on every path, if every
    /// other reader sees no move's value on any path. The flow runs over the blocks where `t` may
    /// live (back from the readers to the instructions writing it); `states` holds the states on
    /// entry and at the end of each block for it.
    #[allow(clippy::too_many_arguments)]
    fn readers_of_x(
        &self,
        f: &Function,
        ctx: &Ctx,
        flow: &Flow,
        reach: &[bool],
        preds: &[Vec<BlockId>],
        gone: &[Vec<bool>],
        marks: &mut Marks,
        states: &mut [(State, State)],
    ) -> Option<Vec<At>> {
        let kills = |b: usize, i: usize| {
            !gone[b][i] && (self.moves.contains(&(b, i)) || self.writers.contains(&(b, i)))
        };
        let step = |b: usize, i: usize, s: State| -> State {
            if self.moves.contains(&(b, i)) {
                return (true, true);
            }
            if self.writers.contains(&(b, i)) {
                return (false, false);
            }
            (
                s.0 && !flow.clobbers(ctx, &f.blocks[b].insts[i], self.x),
                s.1,
            )
        };
        let walk = |b: usize, upto: usize, mut s: State| -> State {
            for (i, &g) in gone[b][..upto].iter().enumerate() {
                if !g {
                    s = step(b, i, s);
                }
            }
            s
        };
        // The blocks `t` lives into (back from the readers up to the instructions writing it), in
        // the order met
        marks.next();
        let mut live_in: Vec<usize> = Vec::new();
        let mut work: Vec<(usize, usize)> = self
            .readers
            .iter()
            .map(|&(b, at)| (b, at.unwrap_or(f.blocks[b].insts.len())))
            .collect();
        while let Some((b, upto)) = work.pop() {
            if !reach[b] || (0..upto).rev().any(|i| kills(b, i)) || marks.seen(b) {
                continue;
            }
            live_in.push(b);
            if !entered(f, b as BlockId) {
                work.extend(
                    preds[b]
                        .iter()
                        .map(|&q| (q as usize, f.blocks[q as usize].insts.len())),
                );
            }
        }
        // The blocks whose end states the flow needs: those `t` lives into and the ways into them
        let mut blocks = live_in.clone();
        for &b in &live_in {
            if !entered(f, b as BlockId) {
                blocks.extend(preds[b].iter().map(|&q| q as usize));
            }
        }
        blocks.sort_unstable();
        blocks.dedup();
        live_in.sort_unstable();
        let entry_state = |b: usize| -> State {
            if b == 0 {
                (false, false)
            } else if entered(f, b as BlockId) {
                (false, true)
            } else if live_in.binary_search(&b).is_ok() {
                // Optimistic until the ways into it say otherwise
                (true, false)
            } else {
                // `t` is written in the block before it is read there
                (false, false)
            }
        };
        let end_of = |b: usize, s: State| -> State {
            let s = walk(b, f.blocks[b].insts.len(), s);
            (s.0 && !term_clobbers(ctx, &f.blocks[b].term, self.x), s.1)
        };
        for &b in &blocks {
            let s = entry_state(b);
            states[b] = (s, end_of(b, s));
        }
        loop {
            let mut changed = false;
            for &b in &blocks {
                if entered(f, b as BlockId) || live_in.binary_search(&b).is_err() {
                    continue;
                }
                let s = preds[b].iter().fold((true, false), |acc, &q| {
                    let e = states[q as usize].1;
                    (acc.0 && e.0, acc.1 || e.1)
                });
                if s != states[b].0 {
                    states[b] = (s, end_of(b, s));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut swapped = Vec::new();
        for &(b, at) in &self.readers {
            if !reach[b] {
                continue;
            }
            let upto = at.unwrap_or(f.blocks[b].insts.len());
            let start = if live_in.binary_search(&b).is_ok() {
                states[b].0
            } else {
                (false, false)
            };
            match walk(b, upto, start) {
                (true, _) => swapped.push((b, at)),
                (false, false) => {}
                (false, true) => return None,
            }
        }
        Some(swapped)
    }
}

/// The state of rule 13 at a point: (on every path a move came last and `x`'s register still
/// holds it; on some path a move came last).
type State = (bool, bool);

/// A test of a `Boolean`: `JmpIfTrue` / `JmpIfFalse` of `c`, as `(c, whether `then` is taken when
/// `c` is true, then, other)`.
fn test(term: &Term) -> Option<(Reg, bool, BlockId, BlockId)> {
    match *term {
        Term::Branch {
            op: o @ (op::JMP_IF_TRUE | op::JMP_IF_FALSE),
            a,
            b: None,
            then,
            other,
        } => Some((a, o == op::JMP_IF_TRUE, then, other)),
        _ => None,
    }
}

/// The block an empty block that only jumps on leads to (through a few of them).
fn through_jumps(f: &Function, mut b: BlockId) -> BlockId {
    for _ in 0..16 {
        let blk = &f.blocks[b as usize];
        match blk.term {
            Term::Jmp(next) if blk.insts.is_empty() && next != b => b = next,
            _ => break,
        }
    }
    b
}

/// Rule 14, the tests: a test going to an empty block that tests the same register goes where
/// that one leads (`(K >= Low) and (K <= High)` assigned and then tested).
///
/// A block ending with a jump (through empty blocks that only jump) to an empty block testing
/// `c`, where every way into it (back through blocks that jump on, writing nothing in `c`'s
/// register) comes from a test of `c` that found the same value ([`known_value`]): the jump goes
/// where the second test leads with that value (`Result := Index >= 0; if Result then … else …`
/// inlined in `if D.TryGetValue(K, V) then`). Where the test's value is not known and only that
/// block leads to the test, the test moves up into it. Returns whether a jump changed.
fn known_tests(f: &mut Function, ctx: &Ctx, flow: &Flow) -> bool {
    let mut changed = false;
    let reach = reachable(f);
    let mut preds = predecessors(f, &reach);
    for (p, &reached) in reach.iter().enumerate() {
        if !reached {
            continue;
        }
        // A test whose way goes to a test of the same register: that one's way for the value
        if let Some((c, on_true, then, other)) = test(&f.blocks[p].term) {
            let follow = |to: BlockId, value: bool| -> BlockId {
                let b = through_jumps(f, to);
                match test(&f.blocks[b as usize].term) {
                    Some((d, on, t, o)) if d == c && f.blocks[b as usize].insts.is_empty() => {
                        if value == on {
                            t
                        } else {
                            o
                        }
                    }
                    _ => to,
                }
            };
            let (t, o) = (follow(then, on_true), follow(other, !on_true));
            if (t, o) != (then, other) {
                let old = f.blocks[p].term.clone();
                if let Term::Branch { then, other, .. } = &mut f.blocks[p].term {
                    (*then, *other) = (t, o);
                }
                retarget(&mut preds, p, &old, &f.blocks[p].term);
                changed = true;
            }
            continue;
        }
        let Term::Jmp(mut b) = f.blocks[p].term else {
            continue;
        };
        // Whether only `p` leads to the block testing, through the empty ones
        let mut only = preds[b as usize].len() == 1;
        for _ in 0..16 {
            let blk = &f.blocks[b as usize];
            match blk.term {
                Term::Jmp(next) if blk.insts.is_empty() && next != b => {
                    b = next;
                    only &= preds[b as usize].len() == 1;
                }
                _ => break,
            }
        }
        let target = &f.blocks[b as usize];
        if !target.insts.is_empty() || b as usize == p || entered(f, b) {
            continue;
        }
        let Some((c, on_true, then, other)) = test(&target.term) else {
            continue;
        };
        let known = if flow.followed(ctx, c) {
            known_value(f, ctx, flow, &preds, p as BlockId, c)
        } else {
            None
        };
        let term = match known {
            Some(value) => Term::Jmp(if value == on_true { then } else { other }),
            // The test moves up into the only block leading to it (so that a compare before it
            // may become a compare-and-branch)
            None if only => target.term.clone(),
            None => continue,
        };
        let old = std::mem::replace(&mut f.blocks[p].term, term);
        retarget(&mut preds, p, &old, &f.blocks[p].term);
        changed = true;
    }
    changed
}

/// Updates the ways into the blocks for block `p`'s terminator going from `old` to `new`. A block
/// no way reaches any more keeps its ways out: they only count as more ways in, which makes the
/// rules give up, not go wrong.
fn retarget(preds: &mut [Vec<BlockId>], p: usize, old: &Term, new: &Term) {
    let p = p as BlockId;
    let mut before = Vec::new();
    old.each_target(|t| before.push(t));
    let mut after = Vec::new();
    new.each_target(|t| after.push(t));
    for t in before {
        if !after.contains(&t) {
            preds[t as usize].retain(|&q| q != p);
        }
    }
    for t in after {
        if !preds[t as usize].contains(&p) {
            preds[t as usize].push(p);
        }
    }
}

/// The value of `c` on entry to block `p`, when every way into it (back through blocks that jump
/// on) comes from a test of `c` that found the same value, and nothing on the way, nor in `p`,
/// writes `c`'s register.
fn known_value(
    f: &Function,
    ctx: &Ctx,
    flow: &Flow,
    preds: &[Vec<BlockId>],
    p: BlockId,
    c: Reg,
) -> Option<bool> {
    let mut value = None;
    let mut seen = vec![p];
    let mut work = vec![p];
    while let Some(cur) = work.pop() {
        let block = &f.blocks[cur as usize];
        if block.insts.iter().any(|i| flow.clobbers(ctx, i, c)) || entered(f, cur) {
            return None;
        }
        if preds[cur as usize].is_empty() {
            return None;
        }
        for &q in &preds[cur as usize] {
            let term = &f.blocks[q as usize].term;
            if let Some((tested, on_true, then, other)) = test(term) {
                if tested != c || then == other {
                    return None;
                }
                let v = (cur == then) == on_true;
                if *value.get_or_insert(v) != v {
                    return None;
                }
                continue;
            }
            if !matches!(term, Term::Jmp(_)) {
                return None;
            }
            if !seen.contains(&q) {
                if seen.len() >= 32 {
                    return None;
                }
                seen.push(q);
                work.push(q);
            }
        }
    }
    value
}

/// Takes out the empty blocks that were reachable (`before`) and are no longer: their jumps
/// would still read what they tested. The jumps, the handler table and the inlined calls'
/// ranges follow; a block that a range of the handler table starts or ends with stays, and so
/// does one a block that stays goes to.
fn drop_dead(f: &mut Function, before: &[bool]) {
    let after = reachable(f);
    let mut keep: Vec<bool> = (0..f.blocks.len())
        .map(|b| {
            let bid = b as BlockId;
            !(before[b] && !after[b] && f.blocks[b].insts.is_empty())
                || f.tries
                    .iter()
                    .any(|t| [t.body.0, t.body.1, t.handler.0, t.handler.1].contains(&bid))
        })
        .collect();
    // A block that stays (also one no longer reached) keeps the blocks it goes to, so that every
    // jump still has a block to go to
    loop {
        let mut changed = false;
        for b in 0..f.blocks.len() {
            if keep[b] {
                f.blocks[b].term.each_target(|t| {
                    if !std::mem::replace(&mut keep[t as usize], true) {
                        changed = true;
                    }
                });
            }
        }
        if !changed {
            break;
        }
    }
    if keep.iter().all(|&k| k) {
        return;
    }
    // The new number of each block (of the next kept one for a dropped one)
    let mut new = Vec::with_capacity(keep.len() + 1);
    let mut next = 0;
    for &k in &keep {
        new.push(next);
        if k {
            next += 1;
        }
    }
    new.push(next);
    let renumber = |b: &mut BlockId| *b = new[*b as usize];
    for block in &mut f.blocks {
        block.term.each_target_mut(renumber);
    }
    for t in &mut f.tries {
        renumber(&mut t.body.0);
        renumber(&mut t.body.1);
        renumber(&mut t.handler.0);
        renumber(&mut t.handler.1);
    }
    for s in &mut f.inlines {
        // The range is inclusive: it ends before the block after its last one
        s.blocks.1 = new[s.blocks.1 as usize + 1].wrapping_sub(1);
        renumber(&mut s.blocks.0);
    }
    let mut k = 0;
    f.blocks.retain(|_| {
        k += 1;
        keep[k - 1]
    });
}

/// The compare-and-branch for a compare whose result `JmpIfTrue` (`on_true`) / `JmpIfFalse` tests:
/// `(op, a, b)`.
fn fused(cmp: u16, on_true: bool, x: Reg, y: Reg) -> Option<(u16, Reg, Reg)> {
    Some(match (cmp, on_true) {
        (op::EQ_INT, true) => (op::JMP_IF_EQ_INT, x, y),
        (op::EQ_INT, false) => (op::JMP_IF_NE_INT, x, y),
        (op::LT_INT, true) => (op::JMP_IF_LT_INT, x, y),
        (op::LT_INT, false) => (op::JMP_IF_LE_INT, y, x),
        (op::LE_INT, true) => (op::JMP_IF_LE_INT, x, y),
        (op::LE_INT, false) => (op::JMP_IF_LT_INT, y, x),
        (op::LT_UINT, true) => (op::JMP_IF_LT_UINT, x, y),
        (op::LT_UINT, false) => (op::JMP_IF_LE_UINT, y, x),
        (op::LE_UINT, true) => (op::JMP_IF_LE_UINT, x, y),
        (op::LE_UINT, false) => (op::JMP_IF_LT_UINT, y, x),
        _ => return None,
    })
}

/// Rule 14, the compares: `LeInt c, x, y` right before `JmpIfFalse c`, where nothing reads `c`'s
/// register after the test, becomes `JmpIfLtInt y, x`, with a constant operand as an immediate where there
/// is one (rule 2; also a constant loaded before a loop).
fn compare_and_branch(f: &mut Function, ctx: &Ctx, flow: &Flow) {
    let counts = super::temp_counts(f);
    let mut marks = Marks::new(f.blocks.len());
    // Nothing is taken away beforehand (the blocks only get shorter)
    let gone: Vec<Vec<bool>> = f
        .blocks
        .iter()
        .map(|b| vec![false; b.insts.len()])
        .collect();
    for bi in 0..f.blocks.len() {
        let block = &f.blocks[bi];
        let Some((c @ Reg::Temp(_), on_true, then, other)) = test(&block.term) else {
            continue;
        };
        if !flow.own_temp(ctx, c) {
            continue;
        }
        let Some(last) = block.insts.last() else {
            continue;
        };
        let InstKind::Plain(ops) = &last.kind else {
            continue;
        };
        let [Operand::Reg(d), Operand::Reg(x), Operand::Reg(y)] = ops[..] else {
            continue;
        };
        if d != c {
            continue;
        }
        let Some((bop, a, b)) = fused(last.op, on_true, x, y) else {
            continue;
        };
        // Nothing reads the compare's register after the test
        let succ = [(then as usize, 0), (other as usize, 0)];
        if read_later(f, ctx, flow, &gone, &mut marks, &succ, c, &[]) {
            continue;
        }
        let cmp = block.insts.len() - 1;
        let mut term = Term::Branch {
            op: bop,
            a,
            b: Some(b),
            then,
            other,
        };
        // A constant operand
        let mut load = None;
        for r in [b, a] {
            let (v, at) = match r {
                Reg::Temp(h) if h >= ctx.first_hoisted => {
                    match ctx.hoisted[(h - ctx.first_hoisted) as usize] {
                        Operand::Imm(v) | Operand::Int(v) => (v, None),
                        _ => continue,
                    }
                }
                Reg::Temp(h) if counts[h as usize] == 2 && flow.own_temp(ctx, r) => {
                    let found =
                        block.insts[..cmp]
                            .iter()
                            .enumerate()
                            .rev()
                            .find_map(|(i, inst)| {
                                as_const(inst).filter(|l| l.0 == r).map(|l| (i, l.1))
                            });
                    match found {
                        Some((i, v))
                            if !read_later(
                                f,
                                ctx,
                                flow,
                                &gone,
                                &mut marks,
                                &[(bi, i + 1)],
                                r,
                                &[r],
                            ) =>
                        {
                            (v, Some(i))
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };
            if let Some(t) = fuse_term(&term, r, v) {
                term = t;
                load = at;
                break;
            }
        }
        let block = &mut f.blocks[bi];
        block.insts.pop();
        if let Some(i) = load {
            block.insts.remove(i);
        }
        block.term = term;
    }
}

/// The instructions [`operand_use`] does not know (their operands are in a row, or are not all
/// registers) that write their first operand, a register, and only read the others.
fn writes_only_first(opc: u16) -> bool {
    matches!(
        opc,
        op::MAKE_REF | op::HASH_FIND | op::HASH_LOOKUP_INT | op::HASH_LOOKUP_STR | op::HASH_STR
    )
}

/// Whether `t` is an operand of an instruction that wants its operands in a row of registers (one
/// [`operand_use`] does not know, or a call's argument), and only read there.
fn wants_place(inst: &Inst, t: Reg) -> Option<bool> {
    let mut named = false;
    inst.each_reg(|o| named |= o == t);
    if !named {
        return None;
    }
    Some(match &inst.kind {
        InstKind::Call {
            dst, callee, args, ..
        } => args.contains(&t) && *dst != Some(t) && *callee != Operand::Reg(t),
        InstKind::Plain(ops) => {
            operand_use(inst).is_none()
                && ops.first() != Some(&Operand::Reg(t))
                && !matches!(
                    op::info(inst.op).map(|i| i.shape),
                    Some(
                        op::Shape::ACursorBC
                            | op::Shape::ACursorSlotC
                            | op::Shape::ACursorBType
                            | op::Shape::Intrinsic
                    )
                )
        }
    })
}

/// Rule 15: `Move t, x` of two temporaries, where `t` is only written by the move and some
/// instruction wants it in its row of registers ([`wants_place`]), and `x` is named only by
/// instructions whose register operands may be any register ([`operand_use`]) and terminators that
/// read it: `x` becomes `t` everywhere (so it is computed in `t`'s register), and the move goes.
/// [`free_while_live`] makes sure that nothing writes `t`'s register while `x` lives, and that
/// nothing else reads it after `x` is written.
fn coalesce(f: &mut Function, ctx: &Ctx, flow: &Flow) {
    let reach = reachable(f);
    let preds = predecessors(f, &reach);
    let mut occ = occurrences(f);
    let mut marks = Marks::new(f.blocks.len());
    // The moves taken away, dropped at the end (so that the places stay where they are)
    let mut gone: Vec<Vec<bool>> = f
        .blocks
        .iter()
        .map(|b| vec![false; b.insts.len()])
        .collect();
    for b in 0..f.blocks.len() {
        if !reach[b] {
            continue;
        }
        for i in 0..f.blocks[b].insts.len() {
            let Some((t @ Reg::Temp(tn), x @ Reg::Temp(xn))) = as_move(&f.blocks[b].insts[i])
            else {
                continue;
            };
            if !(flow.own_temp(ctx, t)
                && flow.own_temp(ctx, x)
                && flow.stack[xn as usize]
                && ctx.reg(t) != ctx.reg(x)
                && !flow.reserved.contains(&ctx.reg(t)))
            {
                continue;
            }
            let live = |o: &Vec<At>| -> Vec<At> {
                o.iter()
                    .copied()
                    .filter(|&(pb, pi)| pi.is_none_or(|pi| !gone[pb][pi]))
                    .collect()
            };
            let (t_places, x_places) = (live(&occ[tn as usize]), live(&occ[xn as usize]));
            if !placed(f, &t_places, t, (b, i))
                || !renamable(f, &x_places, x)
                || !free_while_live(
                    f,
                    ctx,
                    flow,
                    &preds,
                    &gone,
                    &mut marks,
                    &x_places,
                    x,
                    t,
                    (b, i),
                )
            {
                continue;
            }
            for &(pb, pi) in &x_places {
                let swap = |r: &mut Reg| {
                    if *r == x {
                        *r = t;
                    }
                };
                match pi {
                    Some(pi) => f.blocks[pb].insts[pi].each_reg_mut(swap),
                    None => f.blocks[pb].term.each_reg_mut(swap),
                }
            }
            gone[b][i] = true;
            occ[xn as usize].clear();
            let o = &mut occ[tn as usize];
            o.extend(x_places);
            o.sort_unstable();
            o.dedup();
        }
    }
    drop_gone(f, &gone);
}

/// Drops the instructions marked in `gone`.
fn drop_gone(f: &mut Function, gone: &[Vec<bool>]) {
    for (block, gone) in f.blocks.iter_mut().zip(gone) {
        let mut k = 0;
        block.insts.retain(|_| {
            k += 1;
            !gone[k - 1]
        });
    }
}

/// Whether `t` is written only by the move at `at`, read by the others, and wanted in place by
/// one of them ([`wants_place`]).
fn placed(f: &Function, places: &[At], t: Reg, at: (usize, usize)) -> bool {
    let mut wanted = false;
    for &(b, i) in places {
        match i {
            Some(i) if (b, i) == at => {}
            Some(i) => {
                let inst = &f.blocks[b].insts[i];
                match wants_place(inst, t) {
                    Some(true) => wanted = true,
                    _ if operand_use(inst).is_some() && only_reads(inst, t) == Some(true) => {}
                    _ => return false,
                }
            }
            None => {
                if !term_reads(&f.blocks[b].term, t).1 {
                    return false;
                }
            }
        }
    }
    wanted
}

/// Whether every place naming `x` may name another register in its place: instructions
/// [`operand_use`] knows, and terminators that read it.
fn renamable(f: &Function, places: &[At], x: Reg) -> bool {
    places.iter().all(|&(b, i)| match i {
        Some(i) => operand_use(&f.blocks[b].insts[i]).is_some(),
        None => term_reads(&f.blocks[b].term, x).1,
    })
}

/// Whether `t`'s register may hold `x` wherever `x` lives (rule 15), with `gone` the moves
/// already taken away:
/// - back from each reader of `x` to the instructions writing it, no instruction but the move at
///   `at` may write `t`'s register, and `x` does not live where the function or a handler starts;
/// - after each instruction writing `x`, nothing reads `t`'s register (but `x`) before something
///   writes it (the move at `at` does), and nothing but `x` reads `x`'s register (a temporary of
///   that register may read `x`'s value without naming it: rule 5 drops a `Move` between two
///   temporaries of one register).
#[allow(clippy::too_many_arguments)]
fn free_while_live(
    f: &Function,
    ctx: &Ctx,
    flow: &Flow,
    preds: &[Vec<BlockId>],
    gone: &[Vec<bool>],
    marks: &mut Marks,
    places: &[At],
    x: Reg,
    t: Reg,
    at: (usize, usize),
) -> bool {
    let mut defs = Vec::new();
    let mut work: Vec<(usize, usize)> = Vec::new();
    for &(b, i) in places {
        match i {
            Some(i) => {
                let inst = &f.blocks[b].insts[i];
                if writes_reg(inst, x) {
                    defs.push((b, i + 1));
                }
                if reads_reg(ctx, inst, ctx.reg(x), &[]) {
                    work.push((b, i));
                }
            }
            None => work.push((b, f.blocks[b].insts.len())),
        }
    }
    // Back from the readers: `x` lives after each instruction met
    marks.next();
    while let Some((b, upto)) = work.pop() {
        let mut killed = false;
        for i in (0..upto).rev() {
            if gone[b][i] {
                continue;
            }
            let inst = &f.blocks[b].insts[i];
            if writes_reg(inst, x) && !reads_reg(ctx, inst, ctx.reg(x), &[]) {
                killed = true;
                break;
            }
            if (b, i) != at && flow.clobbers(ctx, inst, t) {
                return false;
            }
        }
        if killed || marks.seen(b) {
            continue;
        }
        if entered(f, b as BlockId) {
            return false;
        }
        for &q in &preds[b] {
            let q = q as usize;
            if term_clobbers(ctx, &f.blocks[q].term, t) {
                return false;
            }
            work.push((q, f.blocks[q].insts.len()));
        }
    }
    defs.iter().all(|&d| {
        !read_later(f, ctx, flow, gone, marks, &[d], t, &[x])
            && !read_later(f, ctx, flow, gone, marks, &[d], x, &[x])
    })
}

/// Marks on the blocks for one search at a time (cleared by starting the next one).
struct Marks {
    stamp: u32,
    seen: Vec<u32>,
}

impl Marks {
    fn new(n: usize) -> Marks {
        Marks {
            stamp: 0,
            seen: vec![0; n],
        }
    }

    fn next(&mut self) {
        self.stamp += 1;
    }

    /// Whether block `b` was marked in this search, marking it.
    fn seen(&mut self, b: usize) -> bool {
        std::mem::replace(&mut self.seen[b], self.stamp) == self.stamp
    }
}

/// Whether `inst` surely writes `r` when it completes: the first operand of an instruction
/// [`operand_use`] knows (or of one of [`writes_only_first`]), a call's result.
fn writes_reg(inst: &Inst, r: Reg) -> bool {
    match &inst.kind {
        InstKind::Call { dst, .. } => *dst == Some(r),
        InstKind::Plain(ops) => {
            (operand_use(inst) == Some(Use::WritesFirst) || writes_only_first(inst.op))
                && ops[0] == Operand::Reg(r)
        }
    }
}

/// Whether `inst` surely writes the register `p` when it completes ([`writes_reg`] by register).
fn writes_register(ctx: &Ctx, inst: &Inst, p: u32) -> bool {
    match &inst.kind {
        InstKind::Call { dst, .. } => dst.is_some_and(|d| ctx.reg(d) == p),
        InstKind::Plain(ops) => {
            (operand_use(inst) == Some(Use::WritesFirst) || writes_only_first(inst.op))
                && matches!(ops[0], Operand::Reg(d) if ctx.reg(d) == p)
        }
    }
}

/// Whether `inst` reads the register `p` through a register other than those of `ignore`: any
/// register operand but the result it surely writes ([`writes_reg`]).
fn reads_reg(ctx: &Ctx, inst: &Inst, p: u32, ignore: &[Reg]) -> bool {
    let hit = |r: &Reg| ctx.reg(*r) == p && !ignore.contains(r);
    match &inst.kind {
        InstKind::Call {
            callee,
            args,
            frame,
            ..
        } => matches!(callee, Operand::Reg(r) if hit(r)) || args.iter().any(hit) || hit(frame),
        InstKind::Plain(ops) => {
            let skip = usize::from(
                operand_use(inst) == Some(Use::WritesFirst) || writes_only_first(inst.op),
            );
            ops.iter()
                .skip(skip)
                .any(|o| matches!(o, Operand::Reg(r) if hit(r)))
        }
    }
}

/// Whether the register of `r` may be read, before something surely writes it, on a path from
/// one of `starts` (block, first instruction to look at), by an instruction or terminator naming
/// a register of it other than those of `ignore`; an exception raised in a block may take the
/// path to its handlers. The registers read without being named ([`Flow::unnamed`], the
/// handler table's) count as read.
#[allow(clippy::too_many_arguments)]
fn read_later(
    f: &Function,
    ctx: &Ctx,
    flow: &Flow,
    gone: &[Vec<bool>],
    marks: &mut Marks,
    starts: &[(usize, usize)],
    r: Reg,
    ignore: &[Reg],
) -> bool {
    let p = ctx.reg(r);
    if flow.unnamed.contains(&p) || flow.reserved.contains(&p) {
        return true;
    }
    marks.next();
    let mut work: Vec<(usize, usize)> = starts.to_vec();
    while let Some((b, from)) = work.pop() {
        if from == 0 && marks.seen(b) {
            continue;
        }
        let block = &f.blocks[b];
        work.extend(flow.handlers[b].iter().map(|&h| (h as usize, 0)));
        let mut killed = false;
        for (i, inst) in block.insts.iter().enumerate().skip(from) {
            if gone[b][i] {
                continue;
            }
            if reads_reg(ctx, inst, p, ignore) {
                return true;
            }
            if writes_register(ctx, inst, p) {
                killed = true;
                break;
            }
        }
        if killed {
            continue;
        }
        let mut read = false;
        block
            .term
            .each_reg(|o| read |= ctx.reg(o) == p && !ignore.contains(&o));
        if read {
            return true;
        }
        block.term.each_target(|t| work.push((t as usize, 0)));
    }
    false
}
