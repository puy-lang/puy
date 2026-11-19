//! Peephole rules on a function's IR before codegen (M3-58, M4-55; `docs/design/vm.md` 3.15).
//!
//! Codegen runs [`optimize`] on a copy of each function, so the `ir` dump still shows what `lower`
//! made. Rules 1 to 9 work inside one block, mostly on "pure" instructions (those
//! that read and write nothing but their register operands; [`pure`]); a few look past calls and
//! the instructions whose register operands are known ([`operand_use`]) when a variable only the
//! instructions naming it write is involved ([`Ctx::stable`]). The rules keep what a program can
//! observe, also when an instruction raises: a named variable is never written earlier than
//! before, and never left unwritten where it was written before, at an instruction that may raise,
//! unless the block is outside every range of the handler table (an exception there leaves the
//! routine, and nothing reads the variable again; [`Ctx::outside_try`]).
//!
//! 1. [`stored_globals`]: a `LoadGlobal` right after the `StoreGlobal` of the same global (of a
//!    type that is not managed) becomes a `Move` from the stored register.
//! 2. [`immediates`]: a constant loaded into a temporary used once by an integer operation or a
//!    compare-and-branch goes into that instruction as an immediate (`MulIntI`, `JmpIfLtIntI`, ...;
//!    `docs/spec/bytecode.md` 2.2, 2.3).
//! 3. [`sources`]: `Move t, x` followed by the next write of `t` that reads it (`AddInt t, t, y`)
//!    reads `x` there instead (`AddInt t, x, y`), and the `Move` goes.
//! 4. [`destinations`]: a temporary computed and then moved into a variable (`AddIntI t, v, 1`,
//!    `Narrow t, t, 4`, `Move v, t`) is computed in the variable itself.
//! 5. A `Move` of a register to itself goes.
//! 6. [`for_loops`]: the end of a `for` loop's iteration (`JmpIfEqInt counter, limit` to the exit,
//!    then a block of `AddIntI counter, counter, ±1` jumping back) becomes one `ForLoopUp` /
//!    `ForLoopDown` (`Term::ForLoop`).
//! 7. [`copies`]: `Move t, x` where nothing else writes `t`: its readers read `x` (`Move t, I` /
//!    `DynGet e, A, t`: `DynGet e, A, I`).
//! 8. [`call_results`]: a call's result read once is read where the call leaves it, the first
//!    register of its frame (no `Move` after the call).
//! 9. [`returns`]: `Move v, x` / `Ret v` becomes `Ret x`.
//! 10. [`hoist`]: a constant loaded in a loop is loaded once before it, into a register of its own.
//! 11. [`checks`]: a check of `Self` before an inlined method goes where the same variable is
//!     already checked on every path to it, and nothing since may have destroyed an instance.
//! 12. [`checks`]: the check in the first block of a loop where nothing may destroy an instance
//!     goes before the loop.
//! 13. [`flow`]: `Move t, x` across blocks (an inlined routine's argument or result): the readers
//!     of `t` that see the move's value with `x`'s register holding it on every path read `x`.
//! 14. [`flow`]: a jump or a test going to a test of a `Boolean` whose value is known there goes
//!     where that test leads; a compare whose result only the test after it reads becomes a
//!     compare-and-branch.
//! 15. [`flow`]: `Move t, x` into an operand that must be in its place (`HashLookupInt`'s row of
//!     registers, a call's argument): `x` is computed in `t`'s register.

mod checks;
mod flow;

use crate::bytecode::op;
use crate::ir::{BlockId, Function, Global, Inst, InstKind, Operand, Reg, TempKind, Term};
use crate::sema::hir::VarKind;
use crate::sema::types::TypeTable;
use crate::syntax::ast::ParamMode;

/// Applies the rules to a copy of `f`. `phys` gives the register codegen puts a virtual register
/// in: temporaries share registers (one whose value is dead leaves its register to the next), so a
/// rule that makes a register's value live longer checks the registers, not the temporaries.
pub fn optimize(
    f: &Function,
    types: &TypeTable,
    globals: &[Global],
    phys: &dyn Fn(Reg) -> u32,
) -> Function {
    let mut f = f.clone();
    let vars: Vec<bool> = f
        .vars
        .iter()
        .map(|v| {
            matches!(
                v.kind,
                VarKind::Local
                    | VarKind::Result
                    | VarKind::Hidden
                    | VarKind::Param(ParamMode::Value)
            ) && !types.managed(v.ty)
        })
        .collect();
    // A variable whose register only the instructions naming it write: no reference to it is
    // made (`MakeRef`, for a `var` argument) and no other routine reaches it
    let mut stable: Vec<bool> = vars
        .iter()
        .zip(&f.vars)
        .map(|(&plain, v)| {
            // `Self` of a class method is a value the instructions naming it alone write
            (plain || v.kind == VarKind::SelfParam && !types.managed(v.ty)) && !v.shared
        })
        .collect();
    for b in &f.blocks {
        for i in &b.insts {
            if let (op::MAKE_REF, InstKind::Plain(ops)) = (i.op, &i.kind)
                && let Some(Operand::Reg(Reg::Var(v))) = ops.get(1)
            {
                stable[*v as usize] = false;
            }
        }
    }
    let mut ctx = Ctx {
        types,
        globals,
        phys,
        vars,
        stable,
        temps: f
            .temps
            .iter()
            .map(|t| {
                t.kind == TempKind::Value
                    && t.slot.is_some()
                    && t.ty.is_some_and(|ty| !types.managed(ty))
            })
            .collect(),
        first_hoisted: f.temps.len() as u32,
        hoisted: Vec::new(),
        outside_try: (0..f.blocks.len() as BlockId)
            .map(|b| {
                f.tries.iter().all(|t| {
                    !(t.body.0..=t.body.1).contains(&b) && !(t.handler.0..=t.handler.1).contains(&b)
                })
            })
            .collect(),
    };
    for b in &mut f.blocks {
        stored_globals(&ctx, &mut b.insts);
    }
    let counts = temp_counts(&f);
    for (bi, b) in f.blocks.iter_mut().enumerate() {
        immediates(&counts, &mut b.insts, &mut b.term);
        sources(&ctx, ctx.outside_try[bi], &mut b.insts);
    }
    let counts = temp_counts(&f);
    for b in &mut f.blocks {
        call_results(&ctx, &counts, &f.temps, &mut b.insts, &mut b.term);
    }
    hoist(&mut f, &mut ctx);
    let counts = temp_counts(&f);
    for b in &mut f.blocks {
        copies(&ctx, &counts, &mut b.insts, &mut b.term);
    }
    let counts = temp_counts(&f);
    for (bi, b) in f.blocks.iter_mut().enumerate() {
        destinations(&ctx, ctx.outside_try[bi], &counts, &mut b.insts);
        b.insts
            .retain(|i| !matches!(as_move(i), Some((a, b)) if ctx.reg(a) == ctx.reg(b)));
        returns(&ctx, &mut b.insts, &mut b.term);
    }
    for_loops(&mut f);
    flow::copies_and_branches(&mut f, &ctx);
    drop_unread_constants(&mut f, &mut ctx);
    // `ctx.outside_try` is per block: rule 14 may have taken blocks out, and rules 11 and 12 do
    // not look at it
    checks::self_checks(&mut f, &ctx);
    f
}

struct Ctx<'a> {
    types: &'a TypeTable,
    globals: &'a [Global],
    phys: &'a dyn Fn(Reg) -> u32,
    /// Per named variable: whether its register holds its value (not a reference slot nor a cell)
    /// of a type that is not managed.
    vars: Vec<bool>,
    /// Per named variable: whether it is a plain variable (`vars`) that only the instructions
    /// naming it write, so that its value stays across a call ([`Ctx::stable`]).
    stable: Vec<bool>,
    /// Per temporary: whether it holds the value of an expression (a stack temporary) of a type
    /// that is not managed.
    temps: Vec<bool>,
    /// The temporaries from this one on hold constants loaded before a loop ([`hoist`]). They get
    /// registers of their own below the stack temporaries, which `phys` does not know of.
    first_hoisted: u32,
    /// The constant of each of those temporaries.
    hoisted: Vec<Operand>,
    /// Per block: whether it is outside every range of the handler table, so that an exception
    /// raised there leaves the routine and nothing reads its variables again (as `lower`'s
    /// `expr_into_var` has it): a [`Ctx::stable`] variable may then be written earlier or later
    /// than before around instructions that may raise.
    outside_try: Vec<bool>,
}

impl Ctx<'_> {
    /// The register of `r`, or for a constant loaded before a loop, a number no other register
    /// has (only compared for equality).
    fn reg(&self, r: Reg) -> u32 {
        match r {
            Reg::Temp(t) if t >= self.first_hoisted => u32::MAX - (t - self.first_hoisted),
            _ => (self.phys)(r),
        }
    }

    /// [`pure`], where a division by a constant loaded before a loop that is not 0 does not raise
    /// (as `DivIntI` does not).
    fn purity(&self, inst: &Inst) -> Option<Pure> {
        let p = pure(inst)?;
        if let (op::DIV_INT..=op::MOD_UINT, InstKind::Plain(ops)) = (inst.op, &inst.kind)
            && let Some(Operand::Reg(Reg::Temp(h))) = ops.get(2)
            && *h >= self.first_hoisted
            && matches!(
                self.hoisted[(*h - self.first_hoisted) as usize],
                Operand::Imm(v) | Operand::Int(v) if v != 0
            )
        {
            return Some(Pure::Safe);
        }
        Some(p)
    }

    /// Whether an instruction has an operand in the register of `r`.
    fn touches(&self, inst: &Inst, r: Reg) -> bool {
        let p = self.reg(r);
        let mut found = false;
        inst.each_reg(|o| found |= self.reg(o) == p);
        found
    }

    fn plain_var(&self, r: Reg) -> bool {
        matches!(r, Reg::Var(v) if self.vars[v as usize])
    }

    fn plain_temp(&self, r: Reg) -> bool {
        matches!(r, Reg::Temp(t) if self.temps[t as usize])
    }

    /// Whether only the instructions naming the register of `r` write it, also across a call: a
    /// [`Ctx::stable`] variable, or a constant loaded before a loop (whose register is below
    /// every call's frame).
    fn stable(&self, r: Reg) -> bool {
        match r {
            Reg::Var(v) => self.stable[v as usize],
            Reg::Temp(t) => t >= self.first_hoisted,
            Reg::Link => false,
        }
    }

    /// Whether an instruction may write the register of `r`, which is [`Ctx::stable`]. `None` if
    /// that is not known ([`operand_use`]).
    fn writes(&self, inst: &Inst, r: Reg) -> Option<bool> {
        let p = self.reg(r);
        match &inst.kind {
            InstKind::Call { dst, .. } => Some(dst.is_some_and(|d| self.reg(d) == p)),
            InstKind::Plain(ops) => match operand_use(inst)? {
                Use::WritesFirst => Some(matches!(ops[0], Operand::Reg(d) if self.reg(d) == p)),
                Use::Reads => Some(false),
            },
        }
    }
}

/// How an instruction other than a call uses its register operands ([`operand_use`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    /// It may write its first operand (a register), and only reads the others.
    WritesFirst,
    /// It only reads its register operands.
    Reads,
}

/// How a plain instruction uses its register operands, for the rules that look past
/// instructions that are not pure: those of a pure instruction ([`pure`]), of the element,
/// field and slot reads and writes, and of a few reference counting instructions, whose register
/// operands are all listed (no register range) and which write no register but the first.
/// `None` for the others.
fn operand_use(inst: &Inst) -> Option<Use> {
    let InstKind::Plain(ops) = &inst.kind else {
        return None;
    };
    if pure(inst).is_some() {
        return Some(Use::WritesFirst);
    }
    let first_reg = matches!(ops.first(), Some(Operand::Reg(_)));
    match inst.op {
        op::DYN_GET
        | op::DYN_GET_REF
        | op::VAL_GET
        | op::VAL_GET_REF
        | op::FIELD_GET
        | op::FIELD_GET_REF
        | op::REF_FIELD_GET
        | op::REF_FIELD_GET_REF
        | op::OBJ_GET
        | op::OBJ_GET_REF
        | op::STR_LEN
        | op::ARR_LEN
        | op::LOAD_CONST
        | op::LOAD_GLOBAL_REF
        | op::ASSIGN_REF
        | op::ASSIGN_OBJ
        | op::UNIQUE
        | op::RELEASE
        | op::RELEASE_OBJ
        | op::ADD_REF
        | op::IS_CLASS
        | op::LOAD_REF
        | op::CAP_GET
            if first_reg =>
        {
            Some(Use::WritesFirst)
        }
        op::DYN_SET
        | op::DYN_SET_REF
        | op::VAL_SET
        | op::VAL_SET_REF
        | op::FIELD_SET
        | op::FIELD_SET_REF
        | op::REF_FIELD_SET
        | op::REF_FIELD_SET_REF
        | op::OBJ_SET
        | op::OBJ_SET_REF
        | op::STORE_GLOBAL
        | op::STORE_GLOBAL_REF
        | op::ADD_REF_OBJ
        | op::AS_CLASS
        | op::STORE_REF => Some(Use::Reads),
        // `ClassType a, b` (the check of `Self` before an inlined method; `super::inline`)
        op::INTRINSIC if first_reg && is_class_type(ops) => Some(Use::WritesFirst),
        _ => None,
    }
}

/// [`only_reads`], for the inlining (`super::inline`).
pub(crate) fn names_only_to_read(inst: &Inst, r: Reg) -> Option<bool> {
    only_reads(inst, r)
}

/// The register an instruction writes whenever it completes: the first operand of a pure
/// instruction or of one of [`operand_use`] that writes it, or the destination of a call. `None`
/// if it writes none, or if that is not known.
pub(crate) fn written(inst: &Inst) -> Option<Reg> {
    match (&inst.kind, operand_use(inst)) {
        (InstKind::Call { dst, .. }, _) => *dst,
        (InstKind::Plain(ops), Some(Use::WritesFirst)) => match ops[0] {
            Operand::Reg(r) => Some(r),
            _ => None,
        },
        _ => None,
    }
}

/// Whether the operands of an `Intrinsic` are those of `ClassType a, b`.
fn is_class_type(ops: &[Operand]) -> bool {
    let class_type = i64::from(op::intr::CLASS_TYPE);
    matches!(ops, [_, Operand::Reg(_), Operand::Imm(id)] if *id == class_type)
}

/// Whether the instruction names `r` (some register operand is `r`) only where it reads it: not
/// as a call's operand, nor as the first operand of an instruction that writes it. `None` if it
/// does not name `r`.
fn only_reads(inst: &Inst, r: Reg) -> Option<bool> {
    let mut named = false;
    inst.each_reg(|o| named |= o == r);
    if !named {
        return None;
    }
    Some(match (&inst.kind, operand_use(inst)) {
        (InstKind::Plain(ops), Some(Use::WritesFirst)) => ops[0] != Operand::Reg(r),
        (InstKind::Plain(_), Some(Use::Reads)) => true,
        _ => false,
    })
}

/// Whether the terminator names `r` only where it reads it (a condition, a returned value, a
/// `for` loop's limit), and whether it names it at all: `(names, only reads)`.
fn term_reads(term: &Term, r: Reg) -> (bool, bool) {
    let mut named = false;
    term.each_reg(|o| named |= o == r);
    let reads = match term {
        Term::Branch { .. } | Term::BranchImm { .. } | Term::Ret(_) => true,
        Term::ForLoop { counter, .. } => *counter != r,
        _ => false,
    };
    (named, reads)
}

/// Whether a pure instruction may raise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pure {
    Safe,
    Raises,
}

/// Whether `inst` is pure: it writes its first operand (a register), reads its other register
/// operands, and touches nothing else (no reference counts, no memory through references, no
/// calls). `None` if it is not.
fn pure(inst: &Inst) -> Option<Pure> {
    let InstKind::Plain(ops) = &inst.kind else {
        return None;
    };
    if !matches!(ops.first(), Some(Operand::Reg(_))) {
        return None;
    }
    use Pure::*;
    Some(match inst.op {
        op::MOVE
        | op::LOAD_ZERO
        | op::LOAD_INT
        | op::LOAD_GLOBAL
        | op::ADD_INT..=op::MUL_INT
        | op::NEG_INT..=op::SHR_INT32
        | op::ADD_INT_I
        | op::NARROW
        | op::EQ_INT..=op::NOT_BOOL
        | op::MUL_INT_I
        | op::AND_INT_I..=op::SHR_INT32_I
        | op::ADD_INT32..=op::MUL_INT32_I
        | op::ADD_F64..=op::SQRT_F64
        | op::HASH_INT
        | op::HASH_F64
        | op::HASH_MIX
        | op::KEY_EQ_F64 => {
            // Not `TruncF64` / `RoundF64`, which raise (below)
            if matches!(inst.op, op::TRUNC_F64 | op::ROUND_F64) {
                Raises
            } else {
                Safe
            }
        }
        // A string or an aggregate counts a reference
        op::LOAD_CONST => match ops.get(1) {
            Some(Operand::Int(_) | Operand::Float(_) | Operand::FuncValue { .. }) => Safe,
            _ => return None,
        },
        op::DIV_INT_I | op::MOD_INT_I => match ops.get(2) {
            Some(Operand::Imm(v)) if *v != 0 => Safe,
            _ => Raises,
        },
        op::DIV_INT..=op::MOD_UINT
        | op::ADD_INT_CHK..=op::MUL_UINT_CHK
        | op::NARROW_OVF
        | op::NARROW_RNG
        | op::ADD_CURR..=op::DIV_CURR => Raises,
        // The `Currency` conversions moved to the intrinsics (`docs/spec/bytecode.md` 2.19; M4-109)
        // keep their classes: the number is the last operand
        op::INTRINSIC => match ops.last() {
            Some(&Operand::Imm(id)) => match u8::try_from(id) {
                Ok(
                    op::intr::CURR_TO_F64
                    | op::intr::CURR_TO_F32
                    | op::intr::TRUNC_CURR
                    | op::intr::ROUND_CURR,
                ) => Safe,
                Ok(
                    op::intr::NEG_CURR
                    | op::intr::INT_TO_CURR
                    | op::intr::UINT_TO_CURR
                    | op::intr::F64_TO_CURR,
                ) => Raises,
                _ => return None,
            },
            _ => return None,
        },
        _ => return None,
    })
}

/// Whether an instruction may raise or do anything besides writing its first operand from its
/// other register operands: everything but a pure instruction that does not raise ([`pure`]).
pub(crate) fn may_raise(inst: &Inst) -> bool {
    pure(inst) != Some(Pure::Safe)
}

/// The register a pure instruction writes.
fn def(inst: &Inst) -> Reg {
    match &inst.kind {
        InstKind::Plain(ops) => match ops[0] {
            Operand::Reg(r) => r,
            _ => unreachable!("a pure instruction writes a register"),
        },
        InstKind::Call { .. } => unreachable!("a call is not pure"),
    }
}

/// Whether a pure instruction reads `r`.
fn reads(inst: &Inst, r: Reg) -> bool {
    match &inst.kind {
        InstKind::Plain(ops) => ops[1..].contains(&Operand::Reg(r)),
        InstKind::Call { .. } => unreachable!("a call is not pure"),
    }
}

/// `Move a, b`.
fn as_move(inst: &Inst) -> Option<(Reg, Reg)> {
    match (&inst.kind, inst.op) {
        (InstKind::Plain(ops), op::MOVE) => match ops[..] {
            [Operand::Reg(a), Operand::Reg(b)] => Some((a, b)),
            _ => None,
        },
        _ => None,
    }
}

/// A load of an integer constant into `t`: `LoadZero`, `LoadInt` of an immediate, `LoadConst` of an
/// integer.
fn as_const(inst: &Inst) -> Option<(Reg, i64)> {
    let InstKind::Plain(ops) = &inst.kind else {
        return None;
    };
    match (inst.op, &ops[..]) {
        (op::LOAD_ZERO, [Operand::Reg(t)]) => Some((*t, 0)),
        (op::LOAD_INT, [Operand::Reg(t), Operand::Imm(v)])
        | (op::LOAD_CONST, [Operand::Reg(t), Operand::Int(v)]) => Some((*t, *v)),
        _ => None,
    }
}

/// How many times each temporary occurs in the function (as an operand anywhere, in the handler
/// table or in the registers released when it ends).
fn temp_counts(f: &Function) -> Vec<u32> {
    let mut counts = vec![0u32; f.temps.len()];
    let mut count = |r: Reg| {
        if let Reg::Temp(t) = r {
            counts[t as usize] += 1;
        }
    };
    for b in &f.blocks {
        for i in &b.insts {
            i.each_reg(&mut count);
        }
        b.term.each_reg(&mut count);
    }
    for t in &f.tries {
        count(t.reg);
        if let Some(e) = t.exc {
            count(e);
        }
    }
    for r in &f.finally {
        count(*r);
    }
    counts
}

/// Rule 6: a block ending with `JmpIfEqInt c, l` to `exit`, followed by a block that only `lower`'s
/// `for` reaches from it, holding `AddIntI c, c, ±1` and jumping back to `top`: the first ends with
/// `Term::ForLoop` instead, and the second is left empty.
fn for_loops(f: &mut Function) {
    let mut refs = vec![0u32; f.blocks.len() + 1];
    for b in &f.blocks {
        b.term.each_target(|t| refs[t as usize] += 1);
    }
    for x in 0..f.blocks.len().saturating_sub(1) {
        let y = x + 1;
        let Term::Branch {
            op: op::JMP_IF_EQ_INT,
            a: counter,
            b: Some(limit),
            then: exit,
            other,
        } = f.blocks[x].term
        else {
            continue;
        };
        let step = &f.blocks[y];
        let Term::Jmp(top) = step.term else {
            continue;
        };
        if other as usize != y || refs[y] != 1 || top as usize > x || step.insts.len() != 1 {
            continue;
        }
        let inst = &step.insts[0];
        let delta = match (&inst.kind, inst.op) {
            (InstKind::Plain(ops), op::ADD_INT_I) => match ops[..] {
                [Operand::Reg(a), Operand::Reg(b), Operand::Imm(d)]
                    if a == counter && b == counter && (d == 1 || d == -1) =>
                {
                    d
                }
                _ => continue,
            },
            _ => continue,
        };
        f.blocks[x].term = Term::ForLoop {
            counter,
            limit,
            delta,
            top,
            exit,
        };
        f.blocks[y].insts.clear();
        f.blocks[y].term = Term::Jmp(y as u32 + 1);
    }
}

/// Rule 1: `StoreGlobal t, g` then `LoadGlobal u, g` → `StoreGlobal t, g`, `Move u, t`.
fn stored_globals(ctx: &Ctx, insts: &mut [Inst]) {
    for i in 1..insts.len() {
        let (InstKind::Plain(stored), InstKind::Plain(loaded)) =
            (&insts[i - 1].kind, &insts[i].kind)
        else {
            continue;
        };
        if insts[i - 1].op != op::STORE_GLOBAL || insts[i].op != op::LOAD_GLOBAL {
            continue;
        }
        let ([Operand::Reg(t), Operand::Global(g)], [Operand::Reg(u), Operand::Global(h)]) =
            (&stored[..], &loaded[..])
        else {
            continue;
        };
        if g != h || ctx.types.managed(ctx.globals[*g as usize].ty) {
            continue;
        }
        let (t, u) = (*t, *u);
        insts[i].op = op::MOVE;
        insts[i].kind = InstKind::Plain(vec![Operand::Reg(u), Operand::Reg(t)]);
    }
}

/// Whether an immediate fits in `sB` / `sC` (8 bits; `docs/spec/bytecode.md` 1.2).
fn fits(v: i64) -> bool {
    (i8::MIN as i64..=i8::MAX as i64).contains(&v)
}

/// The integer operation with the immediate `v` in place of its register `C` (`None` if there is
/// none or `v` does not fit). `swapped`: the constant is `B` (the operation must be commutative).
fn with_immediate(opc: u16, v: i64, swapped: bool) -> Option<(u16, i64)> {
    let commutative = matches!(
        opc,
        op::ADD_INT
            | op::MUL_INT
            | op::AND_INT
            | op::OR_INT
            | op::XOR_INT
            | op::ADD_INT32
            | op::MUL_INT32
    );
    if swapped && !commutative {
        return None;
    }
    let (opc, v) = match opc {
        op::ADD_INT => (op::ADD_INT_I, v),
        op::SUB_INT => (op::ADD_INT_I, v.checked_neg()?),
        op::MUL_INT => (op::MUL_INT_I, v),
        op::ADD_INT32 => (op::ADD_INT32_I, v),
        op::SUB_INT32 => (op::ADD_INT32_I, v.checked_neg()?),
        op::MUL_INT32 => (op::MUL_INT32_I, v),
        op::DIV_INT if v != 0 => (op::DIV_INT_I, v),
        op::MOD_INT if v != 0 => (op::MOD_INT_I, v),
        op::AND_INT => (op::AND_INT_I, v),
        op::OR_INT => (op::OR_INT_I, v),
        op::XOR_INT => (op::XOR_INT_I, v),
        op::SHL_INT => (op::SHL_INT_I, v),
        op::SHR_INT => (op::SHR_INT_I, v),
        op::SHL_INT32 => (op::SHL_INT32_I, v),
        op::SHR_INT32 => (op::SHR_INT32_I, v),
        _ => return None,
    };
    fits(v).then_some((opc, v))
}

/// Rule 2 on an instruction: `OP a, b, t` with `t` holding `v` → `OPI a, b, v`.
fn fuse_inst(inst: &Inst, t: Reg, v: i64) -> Option<Inst> {
    let InstKind::Plain(ops) = &inst.kind else {
        return None;
    };
    let [Operand::Reg(a), Operand::Reg(b), Operand::Reg(c)] = ops[..] else {
        return None;
    };
    let (other, swapped) = match (b == t, c == t) {
        (false, true) => (b, false),
        (true, false) => (c, true),
        _ => return None,
    };
    let (opc, v) = with_immediate(inst.op, v, swapped)?;
    Some(Inst {
        op: opc,
        kind: InstKind::Plain(vec![Operand::Reg(a), Operand::Reg(other), Operand::Imm(v)]),
        line: inst.line,
    })
}

/// Rule 2 on a terminator: a compare-and-branch of `t` holding `v` with a register → the
/// compare-and-branch with the immediate, or `JmpIfTrue` / `JmpIfFalse` for `= 0` / `<> 0`.
fn fuse_term(term: &Term, t: Reg, v: i64) -> Option<Term> {
    let Term::Branch {
        op: opc,
        a,
        b: Some(b),
        then,
        other,
    } = *term
    else {
        return None;
    };
    // `x ○ v`, with the operands of a constant on the left swapped
    let (x, opc) = match (a == t, b == t) {
        (false, true) => (a, opc),
        (true, false) => (
            b,
            match opc {
                op::JMP_IF_LT_INT => op::JMP_IF_GT_INT_I,
                op::JMP_IF_LE_INT => op::JMP_IF_GE_INT_I,
                o => o,
            },
        ),
        _ => return None,
    };
    let opc = match opc {
        op::JMP_IF_EQ_INT if v == 0 => {
            return Some(Term::Branch {
                op: op::JMP_IF_FALSE,
                a: x,
                b: None,
                then,
                other,
            });
        }
        op::JMP_IF_NE_INT if v == 0 => {
            return Some(Term::Branch {
                op: op::JMP_IF_TRUE,
                a: x,
                b: None,
                then,
                other,
            });
        }
        op::JMP_IF_EQ_INT => op::JMP_IF_EQ_INT_I,
        op::JMP_IF_NE_INT => op::JMP_IF_NE_INT_I,
        op::JMP_IF_LT_INT => op::JMP_IF_LT_INT_I,
        op::JMP_IF_LE_INT => op::JMP_IF_LE_INT_I,
        o @ (op::JMP_IF_GT_INT_I | op::JMP_IF_GE_INT_I) => o,
        _ => return None,
    };
    fits(v).then_some(Term::BranchImm {
        op: opc,
        a: x,
        imm: v,
        then,
        other,
    })
}

/// Rule 2 on a constant that starts a commutative operation: `LoadInt t, v`, `OP t, t, y` →
/// `OPI t, y, v` (`5 * K`).
fn fuse_acc(inst: &Inst, t: Reg, v: i64) -> Option<Inst> {
    let InstKind::Plain(ops) = &inst.kind else {
        return None;
    };
    let [Operand::Reg(a), Operand::Reg(b), Operand::Reg(c)] = ops[..] else {
        return None;
    };
    if a != t || b != t || c == t {
        return None;
    }
    let (opc, v) = with_immediate(inst.op, v, true)?;
    Some(Inst {
        op: opc,
        kind: InstKind::Plain(vec![Operand::Reg(t), Operand::Reg(c), Operand::Imm(v)]),
        line: inst.line,
    })
}

/// Rule 2: a constant in a temporary that occurs only in its load and in one later instruction (or
/// the terminator) of the block goes into that instruction. A constant that starts a commutative
/// operation computed in its temporary goes into that operation ([`fuse_acc`]).
fn immediates(counts: &[u32], insts: &mut Vec<Inst>, term: &mut Term) {
    let mut i = 0;
    while i < insts.len() {
        if let Some((t @ Reg::Temp(n), v)) = as_const(&insts[i]) {
            let mut used = None;
            for (j, inst) in insts.iter().enumerate().skip(i + 1) {
                let mut found = false;
                inst.each_reg(|r| found |= r == t);
                if found {
                    used = Some(j);
                    break;
                }
            }
            if let Some(j) = used
                && let Some(fused) = fuse_acc(&insts[j], t, v)
            {
                insts[j] = fused;
                insts.remove(i);
                continue;
            }
            if counts[n as usize] != 2 {
                i += 1;
                continue;
            }
            match used {
                Some(j) => {
                    if let Some(fused) = fuse_inst(&insts[j], t, v) {
                        insts[j] = fused;
                        insts.remove(i);
                        continue;
                    }
                }
                None => {
                    if let Some(fused) = fuse_term(term, t, v) {
                        *term = fused;
                        insts.remove(i);
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
}

/// Rule 3: `Move t, x` and the next instruction writing `t`, with only pure instructions that
/// neither use the register of `t` nor write that of `x` between. That instruction reads `x`
/// where it read `t`, and the `Move` goes. A variable `t` may be observed by an exception handler,
/// so then none of them may raise, unless the block is outside the handler table's ranges
/// (`outside_try`; `Ctx::outside_try`) and `t` is [`Ctx::stable`]. For an `x` that only the
/// instructions naming it write ([`Ctx::stable`]), calls and the instructions of [`operand_use`]
/// may be between too, when `t` is a temporary or such a variable (`Sum := (Sum + F(I)) and M`:
/// `Sum` is read after the call).
fn sources(ctx: &Ctx, outside_try: bool, insts: &mut Vec<Inst>) {
    let mut i = 0;
    while i < insts.len() {
        if let Some((t, x)) = as_move(&insts[i])
            && ctx.reg(t) != ctx.reg(x)
            && (ctx.plain_temp(t) || ctx.plain_var(t))
        {
            // Whether `t` may be observed by a handler with the value of `x` written early
            let var = matches!(t, Reg::Var(_)) && !(outside_try && ctx.stable(t));
            let past_calls = !var && ctx.stable(x);
            let mut target = None;
            for (j, inst) in insts.iter().enumerate().skip(i + 1) {
                let Some(p) = ctx.purity(inst) else {
                    // `t` holds its value across the instruction (it is read after it), so a
                    // call's frame is above its register
                    if past_calls && !ctx.touches(inst, t) && ctx.writes(inst, x) == Some(false) {
                        continue;
                    }
                    break;
                };
                if var && p == Pure::Raises {
                    break;
                }
                let d = def(inst);
                if d == t {
                    target = Some(j);
                    break;
                }
                // `x` must keep its value (no temporary taking its register either) up to there
                if ctx.touches(inst, t) || ctx.reg(d) == ctx.reg(x) {
                    break;
                }
            }
            if let Some(j) = target {
                if let InstKind::Plain(ops) = &mut insts[j].kind {
                    for o in &mut ops[1..] {
                        if *o == Operand::Reg(t) {
                            *o = Operand::Reg(x);
                        }
                    }
                }
                insts.remove(i);
                continue;
            }
        }
        i += 1;
    }
}

/// Rule 7: `Move t, x`, where `t` is a temporary that no other instruction writes, and `x` keeps
/// its value up to the last instruction reading `t` in the block ([`Ctx::stable`]: also across
/// calls). Those read `x` instead, and the `Move` goes (`Move t, I` / `DynGet e, A, t`:
/// `DynGet e, A, I`).
fn copies(ctx: &Ctx, counts: &[u32], insts: &mut Vec<Inst>, term: &mut Term) {
    let mut i = 0;
    while i < insts.len() {
        let Some((t @ Reg::Temp(n), x)) = as_move(&insts[i]) else {
            i += 1;
            continue;
        };
        if !ctx.plain_temp(t) || !ctx.stable(x) || ctx.reg(t) == ctx.reg(x) {
            i += 1;
            continue;
        }
        let total = counts[n as usize];
        let mut found = 1; // the `Move`
        let mut readers = Vec::new();
        let mut in_term = false;
        let mut ok = false;
        let mut failed = false;
        for (j, inst) in insts.iter().enumerate().skip(i + 1) {
            match only_reads(inst, t) {
                Some(true) => {
                    inst.each_reg(|r| found += u32::from(r == t));
                    readers.push(j);
                }
                Some(false) => failed = true,
                None => {}
            }
            if failed || found == total {
                ok = !failed;
                break;
            }
            // The instruction reads before it writes: `x` must keep its value for the later ones
            if ctx.writes(inst, x) != Some(false) {
                failed = true;
                break;
            }
        }
        if !ok && !failed {
            let (named, reads) = term_reads(term, t);
            if named && reads {
                term.each_reg(|r| found += u32::from(r == t));
                in_term = true;
                ok = found == total;
            }
        }
        if !ok {
            i += 1;
            continue;
        }
        for &j in &readers {
            insts[j].each_reg_mut(|r| {
                if *r == t {
                    *r = x;
                }
            });
        }
        if in_term {
            term.each_reg_mut(|r| {
                if *r == t {
                    *r = x;
                }
            });
        }
        insts.remove(i);
    }
}

/// Rule 8: a call's result in a temporary read once, by a later instruction of the block with only
/// pure instructions between that leave the register of the call's frame alone (`Call t, F` /
/// `AddInt s, s, t`): it is read where the call leaves it, the frame's first register (codegen then
/// puts no `Move` after the call). The frame's temporary then also holds the result, so its type
/// (`Temp::ty`, the first argument's, which may be managed) no longer tells what it holds after the
/// call: only codegen's registers and [`Ctx::plain_temp`] (which then keeps it out of the rules) look
/// at it.
fn call_results(
    ctx: &Ctx,
    counts: &[u32],
    temps: &[crate::ir::Temp],
    insts: &mut [Inst],
    term: &mut Term,
) {
    for i in 0..insts.len() {
        let InstKind::Call {
            dst: Some(t @ Reg::Temp(n)),
            frame: frame @ Reg::Temp(m),
            ..
        } = insts[i].kind
        else {
            continue;
        };
        if !ctx.plain_temp(t) || temps[m as usize].slot.is_none() || counts[n as usize] != 2 {
            continue;
        }
        let mut reader = None;
        let mut in_term = false;
        for (j, inst) in insts.iter().enumerate().skip(i + 1) {
            match only_reads(inst, t) {
                Some(true) => {
                    reader = Some(j);
                    break;
                }
                Some(false) => break,
                None => {}
            }
            if pure(inst).is_none() || ctx.touches(inst, frame) {
                break;
            }
        }
        if reader.is_none() {
            let between_ok = insts[i + 1..]
                .iter()
                .all(|inst| pure(inst).is_some() && !ctx.touches(inst, frame));
            let (named, reads) = term_reads(term, t);
            if between_ok && named && reads {
                in_term = true;
            } else {
                continue;
            }
        }
        if let InstKind::Call { dst, .. } = &mut insts[i].kind {
            *dst = Some(frame);
        }
        let swap = |r: &mut Reg| {
            if *r == t {
                *r = frame;
            }
        };
        match reader {
            Some(j) => insts[j].each_reg_mut(swap),
            None if in_term => term.each_reg_mut(swap),
            None => {}
        }
    }
}

/// Rule 9: `Move v, x` right before `Ret v` (`Exit(N)` of a function): `Ret x`.
fn returns(ctx: &Ctx, insts: &mut Vec<Inst>, term: &mut Term) {
    let Term::Ret(v) = *term else { return };
    if !ctx.plain_var(v) {
        return;
    }
    if let Some((a, x)) = insts.last().and_then(as_move)
        && a == v
    {
        *term = Term::Ret(x);
        insts.pop();
    }
}

/// Rule 4: `Move v, t`, where the temporary `t` is written by a pure instruction that does not
/// read it and then used only by pure instructions up to the `Move`. These write `v` instead.
/// `v` is written earlier than before, so the instructions after the first must neither raise
/// nor use `v`. Outside the handler table's ranges (`outside_try`), for a [`Ctx::stable`] `v`,
/// they may raise, and calls and the instructions of [`operand_use`] that do not name `t` may be
/// among them (`Sum := (Sum + O.V + Length(O.S)) and M`).
fn destinations(ctx: &Ctx, outside_try: bool, counts: &[u32], insts: &mut Vec<Inst>) {
    let mut m = 0;
    while m < insts.len() {
        let Some((v, t @ Reg::Temp(n))) = as_move(&insts[m]) else {
            m += 1;
            continue;
        };
        if !ctx.plain_var(v) || !ctx.plain_temp(t) {
            m += 1;
            continue;
        }
        // Nothing reads `v` after an exception here, and only the instructions naming it write it
        let free = outside_try && ctx.stable(v);
        // The first instruction of the computation
        let mut start = None;
        let mut uses = 1; // the `Move`
        for s in (0..m).rev() {
            let inst = &insts[s];
            if ctx.purity(inst).is_none() {
                // An instruction that does not name `t` (its register holds the value across it)
                let mut names = false;
                inst.each_reg(|r| names |= r == t);
                let known =
                    matches!(inst.kind, InstKind::Call { .. }) || operand_use(inst).is_some();
                if free && known && !names {
                    continue;
                }
                break;
            }
            let mut here = 0;
            inst.each_reg(|r| here += u32::from(r == t));
            uses += here;
            if def(inst) == t && !reads(inst, t) {
                start = Some(s);
                break;
            }
        }
        let ok = start.is_some_and(|s| {
            counts[n as usize] == uses
                && insts[s + 1..m]
                    .iter()
                    .all(|i| (free || ctx.purity(i) == Some(Pure::Safe)) && !ctx.touches(i, v))
        });
        let Some(s) = start.filter(|_| ok) else {
            m += 1;
            continue;
        };
        for inst in &mut insts[s..m] {
            if let InstKind::Plain(ops) = &mut inst.kind {
                for o in ops.iter_mut() {
                    if *o == Operand::Reg(t) {
                        *o = Operand::Reg(v);
                    }
                }
            }
        }
        insts.remove(m);
    }
}

/// The blocks the terminator may go to.
fn successors(term: &Term) -> Vec<BlockId> {
    let mut out = Vec::new();
    term.each_target(|t| out.push(t));
    out
}

/// The loops of a function whose constants [`hoist`] may load before them, as the inclusive range
/// of their blocks in layout order: a jump back from a block to an earlier one (or the same one)
/// makes the blocks from the earlier one up to it a loop (`lower` lays a loop out in one piece),
/// and every way into the range from outside it is a `Jmp` to its first block (so the blocks
/// ending with those jumps can load the constants), also through the handler table.
fn loops(f: &Function) -> Vec<(BlockId, BlockId)> {
    let mut ranges: Vec<(BlockId, BlockId)> = Vec::new();
    for (x, b) in f.blocks.iter().enumerate() {
        let x = x as BlockId;
        for s in successors(&b.term) {
            if s > x {
                continue;
            }
            match ranges.iter_mut().find(|r| r.0 == s) {
                Some(r) => r.1 = r.1.max(x),
                None => ranges.push((s, x)),
            }
        }
    }
    ranges.retain(|&(top, last)| {
        let inside = |b: BlockId| (top..=last).contains(&b);
        top != 0
            && f.blocks.iter().enumerate().all(|(y, b)| {
                inside(y as BlockId)
                    || successors(&b.term)
                        .into_iter()
                        .all(|s| !inside(s) || matches!(b.term, Term::Jmp(t) if t == top))
            })
            && f.tries.iter().all(|t| {
                // An exception enters the range only from a body inside it
                let (b, h) = (t.body, t.handler);
                (!inside(h.0) && !inside(h.1))
                    || (inside(b.0) && inside(b.1) && inside(h.0) && inside(h.1) && h.0 != top)
            })
    });
    ranges
}

/// A constant load [`hoist`] may move: `LoadZero`, `LoadInt`, or `LoadConst` of a number.
fn hoistable(inst: &Inst) -> Option<(Reg, Operand)> {
    let InstKind::Plain(ops) = &inst.kind else {
        return None;
    };
    match (inst.op, &ops[..]) {
        (op::LOAD_ZERO, [Operand::Reg(t)]) => Some((*t, Operand::Imm(0))),
        (op::LOAD_INT, [Operand::Reg(t), c @ Operand::Imm(_)])
        | (op::LOAD_CONST, [Operand::Reg(t), c @ (Operand::Int(_) | Operand::Float(_))]) => {
            Some((*t, c.clone()))
        }
        _ => None,
    }
}

/// A constant load of `c` (as [`hoistable`] gives it) into `r`.
fn load(r: Reg, c: &Operand, line: u32) -> Inst {
    let (opc, ops) = match c {
        Operand::Imm(0) => (op::LOAD_ZERO, vec![Operand::Reg(r)]),
        Operand::Imm(_) => (op::LOAD_INT, vec![Operand::Reg(r), c.clone()]),
        _ => (op::LOAD_CONST, vec![Operand::Reg(r), c.clone()]),
    };
    Inst {
        op: opc,
        kind: InstKind::Plain(ops),
        line,
    }
}

/// Rule 10: a constant loaded inside a loop ([`loops`]; the outermost one) into a register that
/// then only gives it to other instructions is loaded once, before the loop, into a register of
/// its own (`LoadConst t, 16777215` / `AndInt s, s, t` in the loop: `AndInt s, s, h`, with
/// `LoadConst h, 16777215` before it). The load becomes `Move t, h`, and rules 3 and 7 take the
/// `Move` away; where they do not, the load stays as it was. One register per constant and loop;
/// the registers go below the stack temporaries, so a function whose registers would no longer
/// fit in 8 bits (`Wide`) loads fewer constants before its loops.
fn hoist(f: &mut Function, ctx: &mut Ctx) {
    let loops = loops(f);
    if loops.is_empty() {
        return;
    }
    // The registers of the function, without the frames of its calls
    let mut top_reg = 0;
    for b in &f.blocks {
        for i in &b.insts {
            i.each_reg(|r| top_reg = top_reg.max(ctx.reg(r)));
        }
        b.term.each_reg(|r| top_reg = top_reg.max(ctx.reg(r)));
    }
    let room = 255u32.saturating_sub(top_reg) as usize;
    let first = f.temps.len() as u32;
    debug_assert_eq!(first, ctx.first_hoisted, "constants are hoisted once");
    // (loop, constant) → its register
    let mut hoisted: Vec<((BlockId, BlockId), Operand)> = Vec::new();
    for (bi, b) in f.blocks.iter_mut().enumerate() {
        let bi = bi as BlockId;
        let Some(&range) = loops
            .iter()
            .filter(|(top, last)| (*top..=*last).contains(&bi))
            .min_by_key(|(top, _)| *top)
        else {
            continue;
        };
        for inst in &mut b.insts {
            let Some((t, c)) = hoistable(inst) else {
                continue;
            };
            if !(ctx.plain_temp(t) || ctx.plain_var(t)) {
                continue;
            }
            let k = match hoisted.iter().position(|(r, k)| *r == range && *k == c) {
                Some(k) => k,
                None if hoisted.len() < room => {
                    hoisted.push((range, c));
                    hoisted.len() - 1
                }
                None => continue,
            };
            let h = Reg::Temp(first + k as u32);
            *inst = Inst {
                op: op::MOVE,
                kind: InstKind::Plain(vec![Operand::Reg(t), Operand::Reg(h)]),
                line: inst.line,
            };
        }
    }
    if hoisted.is_empty() {
        return;
    }
    ctx.hoisted = hoisted.iter().map(|(_, c)| c.clone()).collect();
    for (k, ((top, last), c)) in hoisted.iter().enumerate() {
        let ty = match c {
            Operand::Float(_) => crate::sema::types::Ty::Double,
            _ => crate::sema::types::Ty::Int64,
        };
        f.temps.push(crate::ir::Temp {
            ty: Some(ty),
            kind: TempKind::Value,
            slot: None,
        });
        ctx.temps.push(true);
        let h = Reg::Temp(first + k as u32);
        for y in 0..f.blocks.len() {
            // The ways into the loop (`loops`)
            if !(*top..=*last).contains(&(y as BlockId))
                && matches!(f.blocks[y].term, Term::Jmp(t) if t == *top)
            {
                let line = f.blocks[y].term_line;
                f.blocks[y].insts.push(load(h, c, line));
            }
        }
    }
    let counts = temp_counts(f);
    // `copies` first: a division by a constant that is not 0 no longer raises once it reads the
    // constant's register (`Ctx::purity`), which `sources` may then need
    for (bi, b) in f.blocks.iter_mut().enumerate() {
        copies(ctx, &counts, &mut b.insts, &mut b.term);
        sources(ctx, ctx.outside_try[bi], &mut b.insts);
    }
    // A `Move` from a constant's register left: the load goes back
    for b in &mut f.blocks {
        for inst in &mut b.insts {
            if let Some((t, Reg::Temp(h))) = as_move(inst)
                && h >= first
            {
                *inst = load(t, &hoisted[(h - first) as usize].1, inst.line);
            }
        }
    }
    drop_unread_constants(f, ctx);
}

/// The registers of constants loaded before a loop ([`hoist`]) that no instruction reads any more
/// go, with their loads (after [`hoist`] and after rule 14, which may put a constant in a
/// compare-and-branch).
fn drop_unread_constants(f: &mut Function, ctx: &mut Ctx) {
    let first = ctx.first_hoisted;
    let count = ctx.hoisted.len();
    if count == 0 {
        return;
    }
    let mut read = vec![false; count];
    for b in &f.blocks {
        for i in &b.insts {
            let def = hoistable(i).map(|(r, _)| r);
            i.each_reg(|r| {
                if let Reg::Temp(h) = r
                    && h >= first
                    && def != Some(r)
                {
                    read[(h - first) as usize] = true;
                }
            });
        }
        b.term.each_reg(|r| {
            if let Reg::Temp(h) = r
                && h >= first
            {
                read[(h - first) as usize] = true;
            }
        });
    }
    let mut renumber = vec![None; count];
    let mut next = first;
    for (k, &r) in read.iter().enumerate() {
        if r {
            renumber[k] = Some(next);
            next += 1;
        }
    }
    for b in &mut f.blocks {
        b.insts.retain(|i| {
            !matches!(hoistable(i), Some((Reg::Temp(h), _)) if h >= first && !read[(h - first) as usize])
        });
        let swap = |r: &mut Reg| {
            if let Reg::Temp(h) = r
                && *h >= first
            {
                *h = renumber[(*h - first) as usize].expect("a kept constant's register");
            }
        };
        for i in &mut b.insts {
            i.each_reg_mut(swap);
        }
        b.term.each_reg_mut(swap);
    }
    f.temps.truncate(next as usize);
    ctx.temps.truncate(next as usize);
    let mut k = 0;
    ctx.hoisted.retain(|_| {
        k += 1;
        read[k - 1]
    });
}
