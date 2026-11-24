//! Rules 11 and 12: fewer checks of `Self` before the inlined methods
//! (`docs/design/inlining.md` 7.1).
//!
//! The inlining puts `ClassType t, x` (its result unused) where a `CallMethod` would have checked
//! its `Self` `x` (`super::super::inline`). Such a check raises nothing once an earlier check of
//! the same register passed on every path to it and nothing between could have destroyed the
//! instance or changed the register: the rules drop those checks ([`drop_checked`]), and put one
//! check before a loop that holds nothing of the kind in place of the one in its first block
//! ([`before_loops`]). Only the named variables that only the instructions naming them write
//! ([`Ctx::stable`]) are followed.

use super::{Ctx, Pure, loops, only_reads, pure, term_reads};
use crate::bytecode::op;
use crate::ir::inline::registers;
use crate::ir::{BlockId, Function, Inst, InstKind, Operand, Reg, Temp, TempKind, Term};
use crate::sema::hir::VarKind;
use crate::sema::types::Ty;

/// `ClassType t, x`: `(t, x)`.
fn class_type(inst: &Inst) -> Option<(Reg, Reg)> {
    let (op::INTRINSIC, InstKind::Plain(ops)) = (inst.op, &inst.kind) else {
        return None;
    };
    match ops[..] {
        [Operand::Reg(t), Operand::Reg(x), Operand::Imm(id)]
            if id == i64::from(op::intr::CLASS_TYPE) =>
        {
            Some((t, x))
        }
        _ => None,
    }
}

/// Per temporary: whether something reads it (names it other than as the result of a
/// `ClassType`).
fn read_temps(f: &Function) -> Vec<bool> {
    let mut read = vec![false; f.temps.len()];
    let mut mark = |r: Reg| {
        if let Reg::Temp(t) = r {
            read[t as usize] = true;
        }
    };
    for b in &f.blocks {
        for i in &b.insts {
            match class_type(i) {
                Some((_, x)) => mark(x),
                None => i.each_reg(&mut mark),
            }
        }
        b.term.each_reg(&mut mark);
    }
    for t in &f.tries {
        mark(t.reg);
        if let Some(e) = t.exc {
            mark(e);
        }
    }
    for r in &f.finally {
        mark(*r);
    }
    read
}

/// The register `ClassType t, x` checks, if `inst` is that with a result nothing reads ([`read_temps`]).
fn unused_check(inst: &Inst, read: &[bool]) -> Option<Reg> {
    match class_type(inst)? {
        (Reg::Temp(t), x) if !read[t as usize] => Some(x),
        _ => None,
    }
}

/// The register whose instance an instruction checks (`docs/spec/objects.md` 14.4) when it
/// completes: `ClassType`, the field reads and writes that count nothing, and the hash table
/// lookups of an instance (`docs/spec/bytecode.md` 2.23).
fn checked(inst: &Inst) -> Option<Reg> {
    let InstKind::Plain(ops) = &inst.kind else {
        return None;
    };
    if let Some((_, x)) = class_type(inst) {
        return Some(x);
    }
    let at = match inst.op {
        op::OBJ_GET | op::HASH_LOOKUP_INT | op::HASH_LOOKUP_STR => 1,
        op::OBJ_SET => 0,
        _ => return None,
    };
    match ops.get(at) {
        Some(Operand::Reg(r)) => Some(*r),
        _ => None,
    }
}

/// Whether an instruction cannot destroy an instance (`Free`, a call, releasing a reference that
/// may be the last): a pure one, and the reads and writes that count nothing.
fn keeps_instances(inst: &Inst) -> bool {
    if matches!(inst.kind, InstKind::Call { .. }) {
        return false;
    }
    pure(inst).is_some()
        || checked(inst).is_some()
        || matches!(
            inst.op,
            op::MAKE_REF
                | op::LOAD_REF
                | op::STORE_REF
                | op::DYN_GET
                | op::DYN_SET
                | op::BYTES_GET
                | op::BYTES_SET
                | op::VAL_GET
                | op::VAL_SET
                | op::FIELD_GET
                | op::FIELD_SET
                | op::REF_FIELD_GET
                | op::REF_FIELD_SET
                | op::ARR_LEN
                | op::STR_LEN
                | op::HASH_FIND
                | op::HASH_STR
                | op::IS_CLASS
                | op::AS_CLASS
        )
}

/// Whether a terminator only goes on (a jump, a branch, a `for` loop's step): the others may
/// release what the routine holds (`EndFinally`) or leave it.
fn plain_term(term: &Term) -> bool {
    matches!(
        term,
        Term::Jmp(_) | Term::Branch { .. } | Term::BranchImm { .. } | Term::ForLoop { .. }
    )
}

/// Whether an instruction that keeps instances ([`keeps_instances`]) may write the stable
/// variable `x`: only those naming it do, and only where they do not only read it.
fn writes(inst: &Inst, x: Reg) -> bool {
    only_reads(inst, x) == Some(false)
}

/// The checks the rules follow: per stable variable, its index in a state (`None` if not
/// followed).
struct Followed {
    index: Vec<Option<usize>>,
    vars: Vec<u32>,
}

impl Followed {
    fn of(&self, r: Reg) -> Option<usize> {
        match r {
            Reg::Var(v) => self.index[v as usize],
            _ => None,
        }
    }

    /// The state after `inst`: which followed variables hold an instance checked since.
    fn step(&self, inst: &Inst, state: &mut [bool]) {
        if !keeps_instances(inst) {
            state.fill(false);
            return;
        }
        if let Some(k) = checked(inst).and_then(|r| self.of(r)) {
            state[k] = true;
        }
        for (k, &v) in self.vars.iter().enumerate() {
            if state[k] && writes(inst, Reg::Var(v)) {
                state[k] = false;
            }
        }
    }

    /// The state after a block's terminator.
    fn step_term(&self, term: &Term, state: &mut [bool]) {
        if !plain_term(term) {
            state.fill(false);
            return;
        }
        for (k, &v) in self.vars.iter().enumerate() {
            let (names, reads) = term_reads(term, Reg::Var(v));
            if names && !reads {
                state[k] = false;
            }
        }
    }
}

/// Rules 11 and 12 on `f` (after the other rules).
pub(super) fn self_checks(f: &mut Function, ctx: &Ctx) {
    let followed = followed(f, ctx);
    if followed.vars.is_empty() {
        return;
    }
    before_loops(f, &followed);
    drop_checked(f, ctx, &followed);
}

/// The stable variables some unused `ClassType` checks.
fn followed(f: &Function, ctx: &Ctx) -> Followed {
    let read = read_temps(f);
    let mut index = vec![None; f.vars.len()];
    let mut vars = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(Reg::Var(v)) = unused_check(i, &read)
                && ctx.stable(Reg::Var(v))
                && index[v as usize].is_none()
            {
                index[v as usize] = Some(vars.len());
                vars.push(v);
            }
        }
    }
    Followed { index, vars }
}

/// Rule 11: an unused `ClassType` check of a followed variable goes where, on every path from the
/// entry, the variable was checked (by a check or a field read or write) and since then nothing
/// may have destroyed an instance or written the variable. On entry, `Self` of a method of a class
/// counts as checked: every call of one checks it (`CallMethod`, `CallVirtual`, `CallIntf`, a method
/// reference; `docs/spec/objects.md` 14.3, 14.9), and the destructor's instance lives until it
/// ends. An exception's way into a handler counts as unchecked.
fn drop_checked(f: &mut Function, ctx: &Ctx, followed: &Followed) {
    let n = f.blocks.len();
    let width = followed.vars.len();
    let mut entry = vec![false; width];
    if let Some(v) = f.vars.first()
        && v.kind == VarKind::SelfParam
        && matches!(v.ty, Ty::Class(_))
        && ctx.stable(Reg::Var(0))
        && let Some(k) = followed.of(Reg::Var(0))
    {
        entry[k] = true;
    }
    // Per block: the state on entry (`None` while no way into it is known)
    let mut start: Vec<Option<Vec<bool>>> = vec![None; n];
    let mut work = Vec::new();
    if n > 0 {
        start[0] = Some(entry);
        work.push(0usize);
    }
    for t in &f.tries {
        let h = t.handler.0 as usize;
        start[h] = Some(vec![false; width]);
        work.push(h);
    }
    while let Some(b) = work.pop() {
        let mut state = start[b]
            .clone()
            .expect("a block on the work list has a state");
        let block = &f.blocks[b];
        for i in &block.insts {
            followed.step(i, &mut state);
        }
        followed.step_term(&block.term, &mut state);
        block.term.each_target(|t| {
            let t = t as usize;
            match &mut start[t] {
                None => {
                    start[t] = Some(state.clone());
                    work.push(t);
                }
                Some(s) => {
                    let mut changed = false;
                    for (x, &y) in s.iter_mut().zip(&state) {
                        if *x && !y {
                            *x = false;
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
    let read = read_temps(f);
    for (b, s) in f.blocks.iter_mut().zip(start) {
        let Some(mut state) = s else {
            continue;
        };
        b.insts.retain(|i| {
            let redundant = unused_check(i, &read)
                .and_then(|r| followed.of(r))
                .is_some_and(|k| state[k]);
            if !redundant {
                followed.step(i, &mut state);
            }
            !redundant
        });
    }
}

/// Rule 12: a loop ([`loops`]) where no instruction may destroy an instance or write a followed
/// variable, whose first block begins with an unused `ClassType` check of the variable after only
/// instructions that cannot raise and write temporaries alone, gets the check at the end of each
/// block jumping into it too, so that rule 11 drops the one in the loop. Every way into the loop
/// goes through its first block and so through the check, also when the loop runs no iteration
/// (a `for` loop over no values does not jump into it); a check raising at the end of a block
/// jumping in raises what the one in the loop would have raised the first time. The check keeps
/// its line, and the blocks jumping in must be in the same inlined calls and handler table ranges
/// as the first block, so that the trace and the handler are the same. The checks' results go to
/// one register of their own (none if that would take the registers past 8 bits).
fn before_loops(f: &mut Function, followed: &Followed) {
    let read = read_temps(f);
    let mut added: Option<Reg> = None;
    for (top, last) in loops(f) {
        let range = top..=last;
        let blocks = &f.blocks[top as usize..=last as usize];
        let keeps = blocks
            .iter()
            .all(|b| b.insts.iter().all(keeps_instances) && plain_term(&b.term));
        if !keeps {
            continue;
        }
        // The checks at the beginning of the first block
        let mut checks: Vec<(Reg, u32)> = Vec::new();
        for i in &f.blocks[top as usize].insts {
            if let Some(x) = unused_check(i, &read) {
                if followed.of(x).is_some() {
                    checks.push((x, i.line));
                }
                continue;
            }
            let quiet = match (&i.kind, pure(i)) {
                (InstKind::Plain(ops), Some(Pure::Safe)) => {
                    matches!(ops.first(), Some(Operand::Reg(Reg::Temp(_))))
                }
                (InstKind::Plain(ops), None) if i.op == op::MAKE_REF => {
                    matches!(ops.first(), Some(Operand::Reg(Reg::Temp(_))))
                }
                _ => false,
            };
            if !quiet {
                break;
            }
        }
        checks.retain(|&(x, _)| {
            blocks.iter().all(|b| {
                !b.insts.iter().any(|i| writes(i, x)) && {
                    let (names, reads) = term_reads(&b.term, x);
                    !names || reads
                }
            })
        });
        if checks.is_empty() {
            continue;
        }
        let ins: Vec<usize> = (0..f.blocks.len())
            .filter(|&y| {
                !range.contains(&(y as BlockId))
                    && matches!(f.blocks[y].term, Term::Jmp(t) if t == top)
            })
            .collect();
        let place = |b: BlockId| {
            let sites: Vec<bool> = f
                .inlines
                .iter()
                .map(|s| (s.blocks.0..=s.blocks.1).contains(&b))
                .collect();
            let tries: Vec<(bool, bool)> = f
                .tries
                .iter()
                .map(|t| {
                    (
                        (t.body.0..=t.body.1).contains(&b),
                        (t.handler.0..=t.handler.1).contains(&b),
                    )
                })
                .collect();
            (sites, tries)
        };
        let at = place(top);
        if ins.is_empty() || ins.iter().any(|&y| place(y as BlockId) != at) {
            continue;
        }
        let dst = match added {
            Some(r) => r,
            None => {
                if registers(f) >= 256 {
                    return;
                }
                f.temps.push(Temp {
                    ty: Some(Ty::Integer),
                    kind: TempKind::Value,
                    slot: None,
                });
                let r = Reg::Temp(f.temps.len() as u32 - 1);
                added = Some(r);
                r
            }
        };
        for &y in &ins {
            for &(x, line) in &checks {
                f.blocks[y].insts.push(Inst {
                    op: op::INTRINSIC,
                    kind: InstKind::Plain(vec![
                        Operand::Reg(dst),
                        Operand::Reg(x),
                        Operand::Imm(i64::from(op::intr::CLASS_TYPE)),
                    ]),
                    line,
                });
            }
        }
    }
}
