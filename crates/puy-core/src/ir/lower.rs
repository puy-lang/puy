//! Lowering (HIR → IR; `docs/design/lifetime.md` 3.10).
//!
//! A named variable is the virtual register `vN` with the number of its `VarId`. Temporaries of
//! unmanaged types (and call arguments) are created like a stack: a statement or expression
//! releases the ones it created when it ends, and the depth at creation becomes the temporary's
//! slot (codegen turns it into a register). A temporary of a managed type gets a register of its
//! own (one per position in the code) that keeps its reference until the routine ends; a dynamic
//! array holding no object is let go of earlier, when the statement that created it ends (the
//! condition of an `if` / `while` before the body; `docs/design/lifetime.md` 3.4).
//!
//! An expression is lowered into a destination register given by the caller, so an assignment to a
//! local variable computes straight into its register. For a managed type the destination either
//! owns its old value or is a zeroed call argument, and every instruction that writes a managed
//! value there releases the old one (`docs/design/lifetime.md` 3.1, 3.5).

use super::*;
use crate::bytecode::{self, op};
use crate::directives::DirectiveOutput;
use crate::sema::hir::*;
use crate::sema::types::{
    Const, EXCEPTION, KeyKind, OrderKind, TypeId, TypeTable, bound_text, setbits,
};
use crate::source::SourceFile;
use crate::syntax::ast::ParamMode;

/// The most operands one `ConcatStrN` joins (`FuncLower::concat_n`): `C` stays a narrow operand.
const CONCAT_N_MAX: usize = 255;

/// Lowers the program. Expects a program without errors. `files` are the directives and the
/// source of each unit of `p.units` (same order), `paths` the source paths of the unit section
/// (`docs/spec/units.md` 6.3).
pub fn lower(p: &Program, files: &[(&DirectiveOutput, &SourceFile)], paths: &[String]) -> Module {
    // Function table order (`docs/spec/units.md` 6.1): unit by unit in initialization order,
    // `<main>` (the program), `<init>`, `<fini>`, then the routines in declaration order
    let mut order: Vec<(&Routine, Option<RoutineId>)> = Vec::new();
    let mut units: Vec<UnitEntry> = Vec::new();
    let mut entry = 0;
    let mut func_index = vec![0u32; p.routines.len()];
    // The routines the analysis made after every unit's: the instantiations' and the generated
    // ones (`docs/spec/generics.md` 7, `docs/spec/equality.md` 8.3)
    let late: std::collections::HashSet<RoutineId> = p.instance_order.iter().copied().collect();
    for (k, u) in p.units.iter().enumerate() {
        if u.is_program {
            entry = order.len() as u32;
            order.push((&p.main, None));
        }
        let init = if u.is_program {
            p.init.as_ref()
        } else {
            u.init.as_ref()
        };
        let init_index = init.map(|r| {
            order.push((r, None));
            order.len() as u32 - 1
        });
        let fini_index = u.fini.as_ref().map(|r| {
            order.push((r, None));
            order.len() as u32 - 1
        });
        for (i, r) in p.routines.iter().enumerate() {
            // An abstract method has no function (`docs/spec/objects.md` 14.1), nor has a
            // generic body (`docs/spec/generics.md` 7)
            if r.unit as usize == k
                && !r.is_abstract
                && !r.generic
                && r.instance.is_none()
                && !late.contains(&RoutineId(i as u32))
            {
                func_index[i] = order.len() as u32;
                order.push((r, Some(RoutineId(i as u32))));
            }
        }
        units.push(UnitEntry {
            name: u.name.clone(),
            path: paths.get(k).cloned().unwrap_or_default(),
            init: init_index,
            fini: fini_index,
            first_global: u.first_global,
            global_count: u.global_count,
        });
    }
    // The routines of the instantiations and the generated ones, after every unit's
    // (`docs/spec/generics.md` 7)
    for id in &p.instance_order {
        let r = &p.routines[id.0 as usize];
        if r.is_abstract || r.generic {
            continue;
        }
        func_index[id.0 as usize] = order.len() as u32;
        order.push((r, Some(*id)));
    }
    // A type parameter left in the HIR is an error of the compiler (`docs/design/generics.md`
    // 3.1): checked over every routine in the debug builds
    #[cfg(debug_assertions)]
    for (r, _) in &order {
        if let Some(t) = crate::sema::hirwalk::has_param(r, &p.types) {
            panic!(
                "a type parameter is left in `{}` after instantiation: {}",
                r.name,
                p.types.name(t)
            );
        }
    }
    let func_names = p
        .routines
        .iter()
        .map(|r| {
            if let Some(full) = &r.instance {
                return full.clone();
            }
            // The methods of the built-in classes are `System`'s (`TObject.ToString`)
            if r.builtin {
                r.name.clone()
            } else {
                format!("{}.{}", p.units[r.unit as usize].name, r.name)
            }
        })
        .collect();
    let mut ml = ModuleLower {
        files,
        unit: 0,
        hosts: Vec::new(),
        unit_names: p.units.iter().map(|u| u.name.clone()).collect(),
        func_index,
        func_names,
        ctors: (0..p.types.defs.len() as u32)
            .flat_map(|t| p.types.class_methods(crate::sema::types::TypeId(t)))
            .filter(|m| m.kind == crate::sema::types::MethodKind::Constructor)
            .filter_map(|m| m.routine)
            .collect(),
    };
    // The local records expanded into registers (`docs/design/records.md` 3.9): a routine with
    // some is lowered from a copy of its HIR rewritten for them
    let expanded: Vec<Option<super::expand::Expanded>> = order
        .iter()
        .map(|(r, _)| super::expand::expand(r, &p.types, !cell_vars(p, r).is_empty()))
        .collect();
    let funcs = order
        .into_iter()
        .zip(&expanded)
        .map(|((r, id), ex)| {
            ml.unit = r.unit as usize;
            let (r, first) = match ex {
                Some(ex) => (&ex.routine, ex.first.clone()),
                None => (r, Vec::new()),
            };
            FuncLower::new(&mut ml, r, id, p, first).lower()
        })
        .collect();
    Module {
        program: p.name.clone(),
        types: p.types.clone(),
        globals: p
            .globals
            .iter()
            .enumerate()
            .map(|(i, g)| super::Global {
                // A unit's globals are named with the unit (`docs/spec/units.md` 6.2)
                name: match g.unit {
                    Some(u) if !p.units[u as usize].is_program => {
                        format!("{}.{}", p.units[u as usize].name, g.name)
                    }
                    _ => g.name.clone(),
                },
                ty: g.ty,
                exit_code: p.exit_code.is_some_and(|e| e.0 as usize == i),
            })
            .collect(),
        hosts: ml.hosts,
        funcs,
        units,
        entry,
        func_index: ml.func_index,
    }
}

struct ModuleLower<'a> {
    /// Directives and source of each unit.
    files: &'a [(&'a DirectiveOutput, &'a SourceFile)],
    /// The unit of the function being lowered.
    unit: usize,
    hosts: Vec<HostEntry>,
    unit_names: Vec<String>,
    /// The function index of each routine.
    func_index: Vec<u32>,
    /// The full name of each routine (`Hello.TShape.Area`; the `ir` dump of `CallVirtual`).
    func_names: Vec<String>,
    /// The constructors (never inlined; `docs/design/inlining.md` 3.3).
    ctors: std::collections::HashSet<RoutineId>,
}

impl ModuleLower<'_> {
    fn host(&mut self, name: &str, params: u16, ret: bool) -> u32 {
        if let Some(i) = self.hosts.iter().position(|h| h.name == name) {
            return i as u32;
        }
        self.hosts.push(HostEntry {
            name: name.to_string(),
            params,
            ret,
        });
        self.hosts.len() as u32 - 1
    }

    fn line_of(&self, span: TextRange) -> u32 {
        self.files[self.unit].1.line_col(span.start()).line
    }

    fn dir(&self) -> &DirectiveOutput {
        self.files[self.unit].0
    }
}

/// Where the instructions emitted from some point on start: the open block and its number of
/// instructions then, and the number of blocks then (the later blocks were all created since),
/// with the handler table entries added since.
struct Region {
    block: BlockId,
    insts: usize,
    blocks: usize,
    /// The number of handler table entries then.
    tries: usize,
}

/// A block while it is being built.
#[derive(Default)]
struct BlockBuild {
    insts: Vec<Inst>,
    term: Option<(Term, u32)>,
}

struct FuncLower<'m, 'a> {
    ml: &'m mut ModuleLower<'a>,
    r: &'m Routine,
    /// The routine's id (`None` for `<main>`, `<init>` and `<fini>`).
    id: Option<RoutineId>,
    program: &'m Program,
    /// Indexed by the id given when the block was created (renumbered in layout order at the end).
    blocks: Vec<BlockBuild>,
    /// Blocks in the order they were started (the layout order).
    order: Vec<BlockId>,
    /// The block instructions go to. `None` right after a terminator (the next instruction starts a new block).
    cur: Option<BlockId>,
    temps: Vec<Temp>,
    next_slot: u32,
    /// The owning temporaries let go of early ([`Self::alloc`]) of the statements being lowered,
    /// in creation order: a statement lets go of the ones it created when it ends
    /// (`docs/design/lifetime.md` 3.4).
    owned: Vec<Reg>,
    line: u32,
    /// (`Break` target, `Continue` target, `cleanup.len()` at the loop) of the enclosing loops.
    loops: Vec<(BlockId, BlockId, usize)>,
    /// The `except` blocks and `try..finally` bodies around the statement, outermost first.
    cleanup: Vec<Cleanup>,
    /// The handler table, inner entries first.
    tries: Vec<TryEntry>,
    /// The number of `try..except` and `try..finally` bodies written in the source around the
    /// statement (not the implicit `try..finally` of `for..in` and the like).
    guarded: u32,
    /// `{$Q}` at the position of the current statement.
    current_overflow: bool,
    /// The variables whose cell has a register of its own, after the named variables
    /// (`docs/spec/objects.md` 14.9; [`cell_vars`]).
    cells: Vec<VarId>,
    /// For each variable of the HIR before the expansion, the first of its field variables when
    /// it is a local record expanded into registers (`docs/design/records.md` 3.9; empty when
    /// none is).
    expand: Vec<Option<u32>>,
}

impl<'m, 'a> FuncLower<'m, 'a> {
    fn new(
        ml: &'m mut ModuleLower<'a>,
        r: &'m Routine,
        id: Option<RoutineId>,
        program: &'m Program,
        expand: Vec<Option<u32>>,
    ) -> Self {
        // The HIR after instantiation has no type parameter (`docs/design/generics.md` 3.1)
        for v in &r.vars {
            assert!(
                !matches!(v.ty, Ty::Param(_)),
                "a type parameter is left in `{}` after instantiation",
                r.name
            );
        }
        FuncLower {
            ml,
            r,
            id,
            program,
            blocks: Vec::new(),
            order: Vec::new(),
            cur: None,
            temps: Vec::new(),
            next_slot: 0,
            owned: Vec::new(),
            line: 1,
            loops: Vec::new(),
            cleanup: Vec::new(),
            tries: Vec::new(),
            guarded: 0,
            current_overflow: false,
            cells: cell_vars(program, r),
            expand,
        }
    }

    fn lower(mut self) -> Function {
        self.count_arguments_on_entry();
        if self.r.link {
            self.line = self.ml.line_of(self.r.name_span);
            self.outer_refs();
        }
        if !self.cells.is_empty() {
            self.line = self.ml.line_of(self.r.name_span);
            self.cells_on_entry();
        }
        if self.r.groups.own.is_some() || !self.r.groups.made.is_empty() {
            self.line = self.ml.line_of(self.r.name_span);
            self.group_blocks_on_entry();
        }
        if let Some(class) = self.r.dtor_of {
            // A destructor records that its body started (`docs/spec/objects.md` 8.5, 14.8)
            self.line = self.ml.line_of(self.r.name_span);
            let m = self.mark();
            let t = self.alloc_arg(Some(Ty::Integer));
            self.op_table(op::LOAD_INT, t, Operand::Type(Ty::Class(class)));
            self.intrinsic(op::intr::DTOR_ENTER, &[self.reg_of(VarId(0)), t]);
            self.release(m);
        }
        for s in &self.r.body {
            self.stmt(s);
        }
        // Final Ret / RetVoid
        self.line = self.ml.line_of(self.r.end_span);
        self.emit_return();
        let index: Vec<Option<BlockId>> = {
            let mut index = vec![None; self.blocks.len()];
            for (i, b) in self.order.iter().enumerate() {
                index[*b as usize] = Some(i as BlockId);
            }
            index
        };
        let place = |b: BlockId| index[b as usize].expect("jump to a block that was never started");
        let mut blocks = std::mem::take(&mut self.blocks);
        let laid_out: Vec<Block> = self
            .order
            .iter()
            .map(|&b| {
                let built = std::mem::take(&mut blocks[b as usize]);
                let (mut term, term_line) = built.term.expect("every started block is terminated");
                term.each_target_mut(|t| *t = place(*t));
                Block {
                    insts: built.insts,
                    term,
                    term_line,
                }
            })
            .collect();
        Function {
            name: match &self.r.instance {
                Some(full) => full.clone(),
                None => format!("{}.{}", self.ml.unit_names[self.ml.unit], self.r.name),
            },
            file: self.ml.unit as u32,
            origin: self
                .id
                .and_then(|id| self.program.origins.get(&id).cloned()),
            short_name: self.r.name.clone(),
            name_span: self.r.name_span,
            params: self.r.param_count + u32::from(hidden_first(self.r)),
            link: hidden_first(self.r),
            outer: self
                .r
                .parent
                .filter(|_| self.r.link)
                .map(|p| self.ml.func_index[p.0 as usize]),
            vars: self
                .r
                .vars
                .iter()
                .enumerate()
                .map(|(i, v)| VarReg {
                    name: v.name.clone(),
                    ty: v.ty,
                    kind: v.kind,
                    is_for_var: v.is_for_var,
                    shared: v.captured || v.cell,
                    expand: self.var_expand(i as u32),
                })
                .chain(self.cells.iter().map(|c| {
                    let v = &self.r.vars[c.0 as usize];
                    VarReg {
                        name: v.name.clone(),
                        ty: v.ty,
                        kind: VarKind::Cell {
                            var: *c,
                            owned: owns_cell(v),
                        },
                        is_for_var: false,
                        shared: true,
                        expand: VarExpand::No,
                    }
                }))
                .collect(),
            finally: self.finally_list(),
            tries: self.tries,
            temps: self.temps,
            blocks: laid_out,
            lifecycle: self.r.dtor_of.is_some()
                || self.id.is_some_and(|id| self.ml.ctors.contains(&id)),
            inlines: Vec::new(),
        }
    }

    /// The managed registers released at the end of the routine (`docs/design/lifetime.md` 3.8):
    /// value parameters, `Result` and locals in declaration order, then the owning temporaries.
    fn finally_list(&self) -> Vec<Reg> {
        // An expanded record's fields go where the record would, in the order of its slots
        // (`docs/design/records.md` 3.9); they follow the variables of the HIR
        let mut regs: Vec<Reg> = Vec::new();
        for (i, v) in self.r.vars.iter().enumerate() {
            if !self.expand.is_empty() && i >= self.expand.len() {
                break;
            }
            match self.expanded_first(VarId(i as u32)) {
                Some(first) => regs.extend(
                    self.field_regs(first, v.ty)
                        .into_iter()
                        .map(|(r, _)| r)
                        .filter(|r| self.owns(*r)),
                ),
                None if self.owns_var(v) => regs.push(Reg::Var(i as u32)),
                None => {}
            }
        }
        let temps = self
            .temps
            .iter()
            .enumerate()
            .filter(|(_, t)| t.slot.is_none() && t.kind != TempKind::State);
        let n = self.r.vars.len();
        let cells = self
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| owns_cell(&self.r.vars[c.0 as usize]))
            .map(|(k, _)| Reg::Var((n + k) as u32));
        regs.into_iter()
            .chain(cells)
            .chain(temps.map(|(i, _)| Reg::Temp(i as u32)))
            .collect()
    }

    // ---- Blocks ----

    /// Creates a block that is started later with `bind`.
    fn new_block(&mut self) -> BlockId {
        self.blocks.push(BlockBuild::default());
        self.blocks.len() as BlockId - 1
    }

    /// Starts `block`. The current block, if still open, falls through into it.
    fn bind(&mut self, block: BlockId) {
        if self.cur.is_some() {
            self.terminate(Term::Jmp(block));
        }
        self.cur = Some(block);
        self.order.push(block);
    }

    /// The open block (a new one if the previous block was terminated; such a block is unreachable).
    fn current(&mut self) -> BlockId {
        match self.cur {
            Some(b) => b,
            None => {
                let b = self.new_block();
                self.bind(b);
                b
            }
        }
    }

    fn terminate(&mut self, term: Term) {
        let b = self.current();
        self.blocks[b as usize].term = Some((term, self.line));
        self.cur = None;
    }

    fn jump(&mut self, target: BlockId) {
        self.terminate(Term::Jmp(target));
    }

    /// Points the jumps to `from`, a block that is never started, at `to` instead. Only the
    /// blocks from `since` on (in creation order) can jump to it.
    fn retarget(&mut self, since: BlockId, from: BlockId, to: BlockId) {
        for b in &mut self.blocks[since as usize..] {
            if let Some((term, _)) = &mut b.term {
                term.each_target_mut(|t| {
                    if *t == from {
                        *t = to;
                    }
                });
            }
        }
    }

    /// A conditional branch to `target`; otherwise execution continues in a new block.
    fn branch(&mut self, op: u16, a: Reg, b: Option<Reg>, target: BlockId) {
        let next = self.new_block();
        self.terminate(Term::Branch {
            op,
            a,
            b,
            then: target,
            other: next,
        });
        self.bind(next);
    }

    // ---- Instructions ----

    fn push(&mut self, op: u16, kind: InstKind) {
        let b = self.current();
        let line = self.line;
        self.blocks[b as usize].insts.push(Inst { op, kind, line });
    }

    fn plain(&mut self, op: u16, operands: Vec<Operand>) {
        self.push(op, InstKind::Plain(operands));
    }

    /// `Intrinsic` (`docs/spec/bytecode.md` 2.19): the intrinsic `id` with the registers in the
    /// order of its shape (`A`, then `B`; a range may list only its first register).
    fn intrinsic(&mut self, id: u8, regs: &[Reg]) {
        let mut operands: Vec<Operand> = regs.iter().map(|&r| Operand::Reg(r)).collect();
        operands.push(Operand::Imm(i64::from(id)));
        self.plain(op::INTRINSIC, operands);
    }

    /// An intrinsic `id` whose arguments are `B` and `B+1`: copies them (borrowed, `Move`) to two
    /// consecutive call-argument temporaries.
    fn intrinsic_pair(&mut self, id: u8, dst: Reg, args: [(Reg, Ty); 2]) {
        let b = self.alloc_arg(Some(args[0].1));
        let c = self.alloc_arg(Some(args[1].1));
        self.op2(op::MOVE, b, args[0].0);
        self.op2(op::MOVE, c, args[1].0);
        self.intrinsic(id, &[dst, b, c]);
    }

    fn op1(&mut self, op: u16, a: Reg) {
        self.plain(op, vec![Operand::Reg(a)]);
    }

    fn op2(&mut self, op: u16, a: Reg, b: Reg) {
        self.plain(op, vec![Operand::Reg(a), Operand::Reg(b)]);
    }

    fn op3(&mut self, op: u16, a: Reg, b: Reg, c: Reg) {
        self.plain(op, vec![Operand::Reg(a), Operand::Reg(b), Operand::Reg(c)]);
    }

    /// An instruction whose last operand is an immediate (`Narrow` and friends, `AddIntI`, `StrToIntIO`).
    fn op_imm(&mut self, op: u16, a: Reg, b: Reg, imm: i64) {
        self.plain(
            op,
            vec![Operand::Reg(a), Operand::Reg(b), Operand::Imm(imm)],
        );
    }

    /// An instruction with a register and a table operand (`LoadGlobal`, `MakeGlobalRef`, ...).
    fn op_table(&mut self, op: u16, a: Reg, x: Operand) {
        self.plain(op, vec![Operand::Reg(a), x]);
    }

    fn call(&mut self, op: u16, dst: Option<Reg>, callee: Operand, args: Vec<Reg>, frame: Reg) {
        self.push(
            op,
            InstKind::Call {
                dst,
                callee,
                args,
                frame,
            },
        );
    }

    // ---- Temporaries ----

    /// A temporary on the stack of temporaries (a call argument, a frame, or a value of an unmanaged type).
    fn alloc_kind(&mut self, ty: Option<Ty>, kind: TempKind) -> Reg {
        let ty = ty.filter(|t| !t.is_error());
        self.temps.push(Temp {
            ty,
            kind,
            slot: Some(self.next_slot),
        });
        self.next_slot += 1;
        Reg::Temp(self.temps.len() as u32 - 1)
    }

    /// A temporary for the value of an expression. A managed type gets a register of its own that
    /// owns the value until the routine ends, except that a dynamic array holding no object is
    /// let go of when the statement that created it ends: a reference held longer would make a
    /// later `SetLength` of the variable it was read from copy the elements (or raise
    /// `EAliasError` while an element is lent), and letting go of it runs no destructor
    /// (`docs/design/lifetime.md` 3.4).
    fn alloc(&mut self, ty: Ty) -> Reg {
        if self.managed(ty) {
            self.temps.push(Temp {
                ty: Some(ty),
                kind: TempKind::Value,
                slot: None,
            });
            let reg = Reg::Temp(self.temps.len() as u32 - 1);
            if self.released_early(ty) {
                self.owned.push(reg);
            }
            reg
        } else {
            self.alloc_kind(Some(ty), TempKind::Value)
        }
    }

    /// A temporary with a register of its own (`try`: the exception, and the state of a `finally`).
    fn alloc_kind_owning(&mut self, ty: Ty, kind: TempKind) -> Reg {
        self.temps.push(Temp {
            ty: (kind != TempKind::State).then_some(ty),
            kind,
            slot: None,
        });
        Reg::Temp(self.temps.len() as u32 - 1)
    }

    /// A call argument (or the frame of a call without arguments) on the stack of temporaries.
    fn alloc_arg(&mut self, ty: Option<Ty>) -> Reg {
        self.alloc_kind(ty, TempKind::Value)
    }

    /// Whether `reg` holds a reference of its managed value (or 0): a managed variable other than a
    /// `const` / `var` / `out` parameter, or an owning temporary. A call argument does not.
    fn owns(&self, reg: Reg) -> bool {
        match reg {
            Reg::Var(v) => self.owns_var(&self.r.vars[v as usize]),
            Reg::Temp(t) => self.temps[t as usize].slot.is_none(),
            Reg::Link => false,
        }
    }

    fn mark(&self) -> u32 {
        self.next_slot
    }

    // ---- How values are counted (`docs/spec/objects.md` 4.1, 14.3) ----

    /// Whether a variable owns its value ([`owns_value`]).
    fn owns_var(&self, v: &Var) -> bool {
        owns_value(v, &self.program.types)
    }

    /// Whether a value of `ty` is counted, directly or by its contents ([`TypeTable::managed`]).
    fn managed(&self, ty: Ty) -> bool {
        self.program.types.managed(ty)
    }

    /// How a value of `ty` written to `dst` is counted: by its contents when either the value or
    /// the register is exactly `TObject` (a class value goes into a `TObject` variable without a
    /// conversion; the old value there may be an instance of either kind), otherwise as `ty`.
    fn write_kind(&self, ty: Ty, dst: Reg) -> u8 {
        let types = &self.program.types;
        let by_reg = self
            .reg_type(dst)
            .is_some_and(|t| types.slot_kind(t) == bytecode::SLOT_OBJ);
        if by_reg {
            bytecode::SLOT_OBJ
        } else {
            types.slot_kind(ty)
        }
    }

    /// The type of a register's value (`None` for reference slots and frames).
    fn reg_type(&self, r: Reg) -> Option<Ty> {
        match r {
            Reg::Var(v) => self.r.vars.get(v as usize).map(|v| v.ty),
            Reg::Temp(t) => self.temps.get(t as usize).and_then(|t| t.ty),
            Reg::Link => None,
        }
    }

    /// `dst := src` of an existing value of `ty`: counted (`AssignRef`; `AssignObj` by its
    /// contents), or `Move`.
    fn copy_value(&mut self, ty: Ty, dst: Reg, src: Reg) {
        let opc = match self.write_kind(ty, dst) {
            bytecode::SLOT_REF => op::ASSIGN_REF,
            bytecode::SLOT_OBJ => op::ASSIGN_OBJ,
            _ => op::MOVE,
        };
        self.op2(opc, dst, src);
    }

    /// The reference `src` holds moves to `dst`, which lets go of its old value; `src` becomes 0
    /// (`MoveRef`; by its contents, counted into `dst` and let go of from `src`).
    fn move_value(&mut self, ty: Ty, dst: Reg, src: Reg) {
        match self.write_kind(ty, dst) {
            bytecode::SLOT_REF => self.op2(op::MOVE_REF, dst, src),
            bytecode::SLOT_OBJ => {
                self.op2(op::ASSIGN_OBJ, dst, src);
                self.op1(op::RELEASE_OBJ, src);
            }
            _ => self.op2(op::MOVE, dst, src),
        }
    }

    /// Lets go of the value of `r` and clears it (`Release` / `ReleaseObj`; `LoadZero` for a
    /// register that does not own its value).
    fn clear_value(&mut self, ty: Ty, r: Reg) {
        let opc = if !self.owns(r) {
            op::LOAD_ZERO
        } else if self.write_kind(ty, r) == bytecode::SLOT_OBJ {
            op::RELEASE_OBJ
        } else {
            op::RELEASE
        };
        self.op1(opc, r);
    }

    /// `ClearRef` of the reference `r` to an `out` argument of type `ty` before the call
    /// (`docs/design/lifetime.md` 3.3); by its contents, the old value is read, the place set to
    /// 0, then the old value let go of.
    fn clear_ref(&mut self, ty: Ty, r: Reg) {
        match self.program.types.slot_kind(ty) {
            bytecode::SLOT_REF => self.op1(op::CLEAR_REF, r),
            bytecode::SLOT_OBJ => {
                let z = self.alloc_borrow(Ty::Int64);
                self.op1(op::LOAD_ZERO, z);
                self.set_value(
                    ty,
                    true,
                    (op::STORE_REF, op::STORE_REF_REF),
                    vec![Operand::Reg(r), Operand::Reg(z)],
                    z,
                    (op::LOAD_REF, vec![Operand::Reg(r)]),
                );
            }
            _ => {}
        }
    }

    /// A read into `dst` (operands `dst` then `rest`): `plain` for a value not counted or when not
    /// `counted`, `counted_op` (`…Ref`) for a counted one; by its contents, `plain` into an
    /// uncounted temporary then `AssignObj` (`docs/spec/objects.md` 14.3).
    fn get_value(
        &mut self,
        ty: Ty,
        counted: bool,
        (plain, counted_op): (u16, u16),
        dst: Reg,
        rest: Vec<Operand>,
    ) {
        let kind = if counted {
            self.write_kind(ty, dst)
        } else {
            bytecode::SLOT_PLAIN
        };
        let mut operands = vec![Operand::Reg(dst)];
        match kind {
            bytecode::SLOT_REF => {
                operands.extend(rest);
                self.plain(counted_op, operands);
            }
            bytecode::SLOT_OBJ => {
                let t = self.alloc_borrow(Ty::Int64);
                operands[0] = Operand::Reg(t);
                operands.extend(rest);
                self.plain(plain, operands);
                self.op2(op::ASSIGN_OBJ, dst, t);
            }
            _ => {
                operands.extend(rest);
                self.plain(plain, operands);
            }
        }
    }

    /// A write of `src` (which keeps its reference) with the operands `set` (in the instruction's
    /// order): `plain` for a value not counted or when not `counted`, `counted_op` for a counted
    /// one. By its contents: the old value is read first with `get` (its opcode and the operands
    /// after the destination), `src` is counted, written uncounted, then the old value let go of
    /// (`docs/spec/objects.md` 14.3).
    fn set_value(
        &mut self,
        ty: Ty,
        counted: bool,
        (plain, counted_op): (u16, u16),
        set: Vec<Operand>,
        src: Reg,
        get: (u16, Vec<Operand>),
    ) {
        let kind = if counted {
            self.program.types.slot_kind(ty)
        } else {
            bytecode::SLOT_PLAIN
        };
        match kind {
            bytecode::SLOT_REF => self.plain(counted_op, set),
            bytecode::SLOT_OBJ => {
                let old = self.alloc_borrow(Ty::Int64);
                let mut operands = vec![Operand::Reg(old)];
                operands.extend(get.1);
                self.plain(get.0, operands);
                self.op1(op::ADD_REF_OBJ, src);
                self.plain(plain, set);
                self.op1(op::RELEASE_OBJ, old);
            }
            _ => self.plain(plain, set),
        }
    }

    /// Whether a hidden value of `ty` (a temporary, the collection of `for..in`) is let go of when
    /// its statement ends instead of when the routine ends: a dynamic array holding no object
    /// (`docs/design/lifetime.md` 3.4).
    fn released_early(&self, ty: Ty) -> bool {
        matches!(ty, Ty::DynArray(_)) && !self.program.types.holds_object(ty)
    }

    /// Lets go of the owning temporaries created since `self.owned` had `first` entries
    /// (`Release`, which also clears them), and returns them. On a path where one was not written,
    /// it holds 0 or a value of an earlier pass that it still owns, so releasing it is always
    /// right. Nothing is emitted where the code is unreachable (the end of the routine releases
    /// them there).
    fn release_owned(&mut self, first: usize) -> Vec<Reg> {
        let regs = self.owned.split_off(first);
        self.emit_releases(&regs);
        regs
    }

    /// `Release` of each of `regs`, newest first, where the code is reachable.
    fn emit_releases(&mut self, regs: &[Reg]) {
        if self.cur.is_some() {
            for &r in regs.iter().rev() {
                self.op1(op::RELEASE, r);
            }
        }
    }

    fn release(&mut self, mark: u32) {
        self.next_slot = mark;
    }

    fn set_line(&mut self, span: TextRange) -> u32 {
        let old = self.line;
        self.line = self.ml.line_of(span);
        old
    }

    fn reg_of(&self, v: VarId) -> Reg {
        assert!(
            self.expanded_first(v).is_none(),
            "an expanded record is used through its fields"
        );
        Reg::Var(v.0)
    }

    // ---- Local records expanded into registers (`docs/design/records.md` 3.9) ----

    /// The first field variable of a local record expanded into registers.
    fn expanded_first(&self, v: VarId) -> Option<u32> {
        self.expand.get(v.0 as usize).copied().flatten()
    }

    /// How the variable `v` takes part in the expansion (for the `ir` dump).
    fn var_expand(&self, v: u32) -> VarExpand {
        for (i, first) in self.expand.iter().enumerate() {
            let Some(first) = *first else { continue };
            let count = self.flat_width(self.r.vars[i].ty);
            if i as u32 == v {
                return VarExpand::Whole { first, count };
            }
            if (first..first + count).contains(&v) {
                return VarExpand::Field { of: i as u32 };
            }
        }
        VarExpand::No
    }

    /// The number of slots of a record laid out flat.
    fn flat_width(&self, ty: Ty) -> u32 {
        self.program
            .types
            .flat_width(ty)
            .expect("an expanded record is laid out flat")
    }

    /// The field registers (and their types) of the slots `first..` of an expanded record, as
    /// many as a value of `ty` takes.
    fn field_regs(&self, first: u32, ty: Ty) -> Vec<(Reg, Ty)> {
        (first..first + self.flat_width(ty))
            .map(|v| (Reg::Var(v), self.r.vars[v as usize].ty))
            .collect()
    }

    /// The first field register and the type of an expanded record, or of a record field of
    /// one, that a place names whole.
    fn expanded_place(&self, p: &Place) -> Option<(u32, Ty)> {
        match p {
            Place::Var(v, _) => Some((self.expanded_first(*v)?, self.r.vars[v.0 as usize].ty)),
            Place::Field {
                base, index, ty, ..
            } => {
                let (first, bty) = self.expanded_place(base)?;
                Some((first + self.program.types.field_offset(bty, *index), *ty))
            }
            _ => None,
        }
    }

    /// [`FuncLower::expanded_place`] for an expression.
    fn expanded_expr(&self, e: &Expr) -> Option<(u32, Ty)> {
        if e.value.is_some() {
            return None;
        }
        match &e.kind {
            ExprKind::Var(v) => Some((self.expanded_first(*v)?, self.r.vars[v.0 as usize].ty)),
            ExprKind::Field { base, index, .. } => {
                let (first, bty) = self.expanded_expr(base)?;
                Some((first + self.program.types.field_offset(bty, *index), e.ty))
            }
            _ => None,
        }
    }

    /// An assignment to an expanded record (or a record field of one) whole: field by field from
    /// another, cleared for `Default`, or read slot by slot from a flat place (`SlotGet`).
    fn assign_to_fields(&mut self, first: u32, ty: Ty, value: &Expr) {
        let dst = self.field_regs(first, ty);
        if let Some((src, _)) = self.expanded_expr(value) {
            if src != first {
                for (k, (r, t)) in dst.into_iter().enumerate() {
                    self.copy_value(t, r, Reg::Var(src + k as u32));
                }
            }
            return;
        }
        match &value.kind {
            ExprKind::Builtin {
                func: Builtin::Default,
                ..
            } => {
                for (r, t) in dst {
                    self.clear_value(t, r);
                }
            }
            _ => self.read_field_with(value, |this, h, c| this.hop_spread(h, c, &dst)),
        }
    }

    /// An assignment of an expanded record (or a record field of one) whole to a flat place:
    /// the target's indices, then its slots written one by one (`SlotSet`).
    fn assign_from_fields(&mut self, target: &Place, first: u32, ty: Ty) {
        let src = self.field_regs(first, ty);
        match target {
            Place::ObjField { base, slot, ty, .. } => {
                let b = self.obj_handle(base, false);
                let h = Hop::obj(base.ty, self.obj_slot(base.ty, *slot), *ty);
                self.hop_gather(&h, b, &src);
            }
            Place::Field { .. } | Place::Elem { .. } => {
                let part = self.eval_part(target);
                self.part_check(&part);
                self.part_access(&part, true, |this, h, c| this.hop_gather(h, c, &src));
            }
            _ => unreachable!("an expanded record is only assigned to a flat place"),
        }
    }

    /// Reads the flat place at the address of `h` in the container `c` into the registers `dst`,
    /// slot by slot (counted).
    fn hop_spread(&mut self, h: &Hop, c: Reg, dst: &[(Reg, Ty)]) {
        let (cur, _) = self.hop_cursor(h, c, true);
        for (k, (r, ty)) in dst.iter().enumerate() {
            self.cursor_get(*r, cur, k as u32, *ty, true);
        }
    }

    /// Writes the registers `src` to the flat place at the address of `h` in the opened
    /// container `c`, slot by slot (counted).
    fn hop_gather(&mut self, h: &Hop, c: Reg, src: &[(Reg, Ty)]) {
        let (cur, _) = self.hop_cursor(h, c, true);
        for (k, (r, ty)) in src.iter().enumerate() {
            self.cursor_set(cur, k as u32, *r, *ty, true);
        }
    }

    /// Whether the variable's register holds a reference slot: a `var` / `out` parameter, or a
    /// variable of an enclosing routine (`docs/spec/semantics.md` 4.9).
    fn is_ref_param(&self, v: VarId) -> bool {
        holds_ref(&self.r.vars[v.0 as usize])
    }

    /// The routine `up` levels out of the one being lowered.
    fn enclosing(&self, up: u32) -> &'m Routine {
        let mut r = self.r;
        for _ in 0..up {
            let p = r.parent.expect("a nested routine has an enclosing routine");
            r = &self.program.routines[p.0 as usize];
        }
        r
    }

    /// The variable a variable of an enclosing routine refers to (`docs/spec/semantics.md` 4.9),
    /// or the variable itself.
    fn origin(&self, v: VarId) -> &'m Var {
        let var = &self.r.vars[v.0 as usize];
        match var.kind {
            // A closure's variables of enclosing routines are cells it captured
            VarKind::Outer { up, var } if !self.r.closure => {
                &self.enclosing(up).vars[var.0 as usize]
            }
            _ => var,
        }
    }

    /// The references to the variables of enclosing routines, made when a nested routine is
    /// entered (`docs/spec/semantics.md` 4.9): `OuterRef vN, B, up` with `B` the variable's
    /// register in its routine's frame (after the static link, if it takes one). The register of
    /// a `var` / `out` parameter already holds a reference, which is read instead (`LoadRef`).
    fn outer_refs(&mut self) {
        for (i, v) in self.r.vars.iter().enumerate() {
            if let VarKind::Outer { up, var } = v.kind {
                let owner = self.enclosing(up);
                let b = var.0 + u32::from(hidden_first(owner));
                let reg = Reg::Var(i as u32);
                self.plain(
                    op::OUTER_REF,
                    vec![
                        Operand::Reg(reg),
                        Operand::Imm(b as i64),
                        Operand::Imm(up as i64),
                    ],
                );
                if holds_ref(&owner.vars[var.0 as usize]) {
                    self.op2(op::LOAD_REF, reg, reg);
                }
            }
        }
    }

    /// Counts the arguments of the managed value parameters that do not borrow them
    /// (`docs/design/lifetime.md` 3.3): the caller lends them, and these parameters are written
    /// (or used by a nested routine), so they hold a reference of their own, released with the
    /// managed registers. First, before anything can raise and release them. A parameter with a
    /// cell copies its value there instead ([`FuncLower::cells_on_entry`]).
    fn count_arguments_on_entry(&mut self) {
        for (i, v) in self.r.vars.iter().enumerate() {
            if v.kind != VarKind::Param(ParamMode::Value) || !self.owns_var(v) {
                continue;
            }
            self.line = self.ml.line_of(self.r.name_span);
            let opc = match self.program.types.slot_kind(v.ty) {
                bytecode::SLOT_OBJ => op::ADD_REF_OBJ,
                _ => op::ADD_REF,
            };
            self.op1(opc, self.reg_of(VarId(i as u32)));
        }
    }

    /// The register of the cell of the variable `v` (`docs/spec/objects.md` 14.9).
    fn cell_reg(&self, v: VarId) -> Reg {
        let k = self
            .cells
            .iter()
            .position(|c| *c == v)
            .expect("a captured variable has a cell register");
        Reg::Var((self.r.vars.len() + k) as u32)
    }

    /// The cells made or taken when the routine is entered (`docs/spec/objects.md` 14.9): a new
    /// cell for each captured parameter and local (a parameter's borrowed value is copied into
    /// it, counted; `Self` and a `const` parameter, which cannot change, keep their register),
    /// the cells a closure captured (`CapGet`), and the cells of the variables of enclosing
    /// routines (read through the static link). A variable declared inline gets its cell where it is declared.
    fn cells_on_entry(&mut self) {
        let n = self.r.vars.len();
        for (k, c) in self.cells.clone().into_iter().enumerate() {
            let v = &self.r.vars[c.0 as usize];
            let h = Reg::Var((n + k) as u32);
            let x = self.reg_of(c);
            match v.kind {
                VarKind::Outer { .. } if self.r.closure => {
                    let k = self
                        .r
                        .captures
                        .iter()
                        .position(|x| *x == c)
                        .expect("a closure's variable of an enclosing routine is captured");
                    // A closure-form nested routine takes it from its group's block
                    let i = self.r.groups.slots.get(k).map_or(k, |s| *s as usize);
                    self.plain(
                        op::CAP_GET,
                        vec![
                            Operand::Reg(h),
                            Operand::Reg(Reg::Link),
                            Operand::Imm(i as i64),
                        ],
                    );
                    self.intrinsic(op::intr::CELL_REF, &[x, h]);
                }
                VarKind::Outer { up, var } => {
                    let owner = self.enclosing(up);
                    let b = cell_index(self.program, owner, var)
                        .expect("the variable of an enclosing routine has a cell register")
                        + u32::from(hidden_first(owner));
                    self.plain(
                        op::OUTER_REF,
                        vec![
                            Operand::Reg(h),
                            Operand::Imm(b as i64),
                            Operand::Imm(up as i64),
                        ],
                    );
                    self.op2(op::LOAD_REF, h, h);
                }
                // `E` of `on E` gets its cell where the handler takes the exception
                VarKind::Exception => {}
                _ if declared_inline(v) => {}
                VarKind::SelfParam | VarKind::Param(ParamMode::Const) => self.copy_to_cell(c),
                // A value parameter borrows its argument: the cell gets a counted copy, and the
                // register a reference to the cell
                VarKind::Param(_) => {
                    self.copy_to_cell(c);
                    self.intrinsic(op::intr::CELL_REF, &[x, h]);
                }
                _ => {
                    self.new_cell(v.ty, h);
                    self.intrinsic(op::intr::CELL_REF, &[x, h]);
                }
            }
        }
    }

    /// A new cell holding a counted copy of the value of `v`: a borrowed one (`Self`, a `const`
    /// or value parameter, `E` of `on E`).
    fn copy_to_cell(&mut self, v: VarId) {
        let (h, x) = (self.cell_reg(v), self.reg_of(v));
        let kind = self.new_cell(self.r.vars[v.0 as usize].ty, h);
        match kind {
            bytecode::SLOT_REF => self.op2(op::CELL_SET_REF, h, x),
            bytecode::SLOT_OBJ => {
                self.op1(op::ADD_REF_OBJ, x);
                self.op2(op::CELL_SET, h, x);
            }
            _ => self.op2(op::CELL_SET, h, x),
        }
    }

    /// `NewCell` / `NewCellRef` / `NewCellObj` of a value of type `ty` into `h`: the kind of the
    /// cell's value (a `[weak]` value is not counted).
    fn new_cell(&mut self, ty: Ty, h: Reg) -> u8 {
        let kind = self.program.types.slot_kind(ty);
        let id = match kind {
            bytecode::SLOT_REF => op::intr::NEW_CELL_REF,
            bytecode::SLOT_OBJ => op::intr::NEW_CELL_OBJ,
            _ => op::intr::NEW_CELL,
        };
        self.intrinsic(id, &[h]);
        kind
    }

    /// A new cell for the variable `v` declared inline, where its declaration runs (or a `for`
    /// loop starts an iteration; `docs/spec/objects.md` 9.6): the old cell is let go of, and
    /// lives on in the closures holding it.
    fn renew_cell(&mut self, v: VarId) {
        let var = &self.r.vars[v.0 as usize];
        if !(var.cell && declared_inline(var)) {
            return;
        }
        let h = self.cell_reg(v);
        self.new_cell(var.ty, h);
        self.intrinsic(op::intr::CELL_REF, &[self.reg_of(v), h]);
    }

    /// `MakeClosure` of an anonymous method, or of a group block without a function (`func` is
    /// -1; `docs/spec/objects.md` 14.9): its function and the cells it captures, in consecutive
    /// registers.
    /// `func` is the closure's function, or `None` for a group block (-1).
    fn make_closure(&mut self, func: Option<u32>, captures: &[VarId], dst: Reg) {
        let m = self.mark();
        let f = self.alloc_arg(Some(Ty::Int64));
        let n = self.alloc_arg(Some(Ty::Int64));
        let cells: Vec<Reg> = captures.iter().map(|_| self.alloc_arg(None)).collect();
        match func {
            Some(func) => self.load_func(f, func, 0),
            None => self.load_int(f, -1),
        }
        self.load_int(n, captures.len() as i64);
        for (t, c) in cells.iter().zip(captures) {
            let h = self.cell_reg(*c);
            self.op2(op::MOVE, *t, h);
        }
        let mut regs = vec![dst, f, n];
        regs.extend(cells);
        self.intrinsic(op::intr::MAKE_CLOSURE, &regs);
        self.release(m);
    }

    /// The group blocks made on entry (`docs/spec/objects.md` 14.9): a closure-form nested
    /// routine's own block copied for the routines nested in it, and the blocks of the groups of
    /// the closure-form routines nested in this one (no function; they capture cells only, so
    /// their order does not matter).
    fn group_blocks_on_entry(&mut self) {
        let r = self.r;
        if let Some(v) = r.groups.own {
            self.store_place(&Place::Var(v, r.name_span), Reg::Link);
        }
        for (v, cells) in &r.groups.made {
            let x = self.reg_of(*v);
            if self.is_ref_param(*v) {
                let t = self.alloc(Ty::Callable(crate::sema::types::TPROC));
                self.make_closure(None, cells, t);
                self.store_place(&Place::Var(*v, r.name_span), t);
            } else {
                self.make_closure(None, cells, x);
            }
        }
    }

    /// The group block (`docs/spec/objects.md` 14.9) in `block` (`None`: the closure of the
    /// routine being lowered), uncounted into `dst`.
    fn load_block(&mut self, block: Option<VarId>, dst: Reg) {
        match block {
            None => self.op2(op::MOVE, dst, Reg::Link),
            Some(v) if self.is_ref_param(v) => self.op2(op::LOAD_REF, dst, self.reg_of(v)),
            Some(v) => self.op2(op::MOVE, dst, self.reg_of(v)),
        }
    }

    fn load_int(&mut self, dst: Reg, v: i64) {
        if v == 0 {
            self.op1(op::LOAD_ZERO, dst);
        } else if (i16::MIN as i64..=i16::MAX as i64).contains(&v) {
            self.plain(op::LOAD_INT, vec![Operand::Reg(dst), Operand::Imm(v)]);
        } else {
            self.op_table(op::LOAD_CONST, dst, Operand::Int(v));
        }
    }

    /// Loads a function index plus `add` as an integer (a routine value, a closure's function;
    /// `docs/spec/objects.md` 14.9): like [`Self::load_int`], with an operand that tells codegen
    /// the value names a function, so the function table can be renumbered after sharing
    /// (`docs/spec/generics.md` 7).
    fn load_func(&mut self, dst: Reg, func: u32, add: i64) {
        let value = Operand::FuncValue { func, add };
        if (i16::MIN as i64..=i16::MAX as i64).contains(&(i64::from(func) + add)) {
            self.plain(op::LOAD_INT, vec![Operand::Reg(dst), value]);
        } else {
            self.op_table(op::LOAD_CONST, dst, value);
        }
    }

    fn load_str(&mut self, dst: Reg, units: &[u16]) {
        if units.is_empty() {
            // The empty string is 0: an owning register lets go of its value
            let opc = if self.owns(dst) {
                op::RELEASE
            } else {
                op::LOAD_ZERO
            };
            self.op1(opc, dst);
        } else {
            self.op_table(op::LOAD_CONST, dst, Operand::Str(units.to_vec()));
        }
    }

    fn load_text(&mut self, dst: Reg, text: &str) {
        let units: Vec<u16> = text.encode_utf16().collect();
        self.load_str(dst, &units);
    }

    fn load_const(&mut self, dst: Reg, c: &Const, ty: Ty) {
        match c {
            Const::Int(v) => {
                let v = if ty == Ty::UInt64 {
                    *v as u64 as i64
                } else {
                    *v as i64
                };
                self.load_int(dst, v);
            }
            Const::Bool(b) => self.load_int(dst, *b as i64),
            // A `Currency` constant is the scaled integer
            Const::Curr(v) => self.load_int(dst, *v),
            Const::Real(bits) => {
                // 0.0 is 0 for both widths; a `Single` constant is the `f64` rounded when it is
                // loaded (`docs/spec/bytecode.md` 3.2)
                if *bits == 0 {
                    self.op1(op::LOAD_ZERO, dst);
                } else {
                    self.op_table(op::LOAD_CONST, dst, Operand::Float(*bits));
                    if ty == Ty::Single {
                        self.op2(op::F64_TO_F32, dst, dst);
                    }
                }
            }
            Const::Char(ch) if ty == Ty::String => self.load_str(dst, &[*ch]),
            Const::Char(ch) => self.load_int(dst, *ch as i64),
            Const::Str(s) => self.load_str(dst, s),
            Const::U8(s) if s.is_empty() => self.load_str(dst, &[]),
            Const::U8(s) => self.op_table(op::LOAD_CONST, dst, Operand::U8(s.clone())),
            Const::Agg(items) if matches!(ty, Ty::DynArray(_)) => {
                // A typed constant of a dynamic array is a new array each time it is read
                let elem = self.program.types.dyn_elem(ty).unwrap_or(Ty::Error);
                let elems: Vec<Expr> = items
                    .iter()
                    .map(|v| Expr {
                        kind: ExprKind::Const { explicit: false },
                        ty: elem,
                        value: Some(v.clone()),
                        span: TextRange::empty(0.into()),
                    })
                    .collect();
                let line = self.line;
                self.array_ctor(ty, &elems, dst);
                self.line = line;
            }
            Const::Agg(_) => self.op_table(op::LOAD_CONST, dst, Operand::Agg(ty, c.clone())),
            // `nil` is 0, the empty dynamic array
            Const::Nil => self.clear_value(ty, dst),
            Const::Set(words) => match ty {
                // The empty big set is 0
                Ty::BigSet(_) if words.is_empty() => self.load_str(dst, &[]),
                Ty::BigSet(_) => self.op_table(op::LOAD_CONST, dst, Operand::Agg(ty, c.clone())),
                _ => self.load_int(dst, setbits::to_small(words)),
            },
        }
    }

    fn emit_return(&mut self) {
        match self.r.result_var() {
            Some(rv) => self.terminate(Term::Ret(self.reg_of(rv))),
            None => self.terminate(Term::RetVoid),
        }
    }

    // ---- Statements ----

    fn stmt(&mut self, s: &Stmt) {
        let old = self.set_line(s.span);
        let mark = self.mark();
        let first_owned = self.owned.len();
        let saved_overflow = self.current_overflow;
        self.current_overflow = self.ml.dir().switches.at(s.span.start()).overflow_checks;
        match &s.kind {
            StmtKind::Assign {
                target,
                value,
                is_init,
                ..
            } => {
                if *is_init && let Place::Var(v, _) = target {
                    self.renew_cell(*v);
                }
                self.assign(target, value)
            }
            StmtKind::Declare(v) => self.renew_cell(*v),
            StmtKind::Block(list) => {
                for x in list {
                    self.stmt(x);
                }
            }
            StmtKind::Expr(e) => match &e.kind {
                ExprKind::InheritedBuiltin { msg: None, .. } => {}
                ExprKind::InheritedBuiltin { .. } => {
                    // `inherited Create(Msg)` of `Exception` writes `Message` (no value)
                    let t = self.alloc_borrow(Ty::Integer);
                    self.expr(e, t);
                }
                // A procedure call has no value to keep (`bytecode.md` 5.1: the destination is `-`)
                ExprKind::Call {
                    routine,
                    args,
                    dispatch,
                } if self.program.routines[routine.0 as usize].ret.is_none() => {
                    self.call_routine(*routine, args, *dispatch, None)
                }
                ExprKind::CallValue { callee, args } if e.ty.is_error() => {
                    self.call_value(callee, args, None, None)
                }
                _ => {
                    let t = self.alloc(e.ty);
                    self.expr(e, t);
                }
            },
            StmtKind::PropertySet { call, .. } => {
                let ExprKind::Call {
                    routine,
                    args,
                    dispatch,
                } = &call.kind
                else {
                    unreachable!("PropertySet always holds the call of the write method");
                };
                self.call_routine(*routine, args, *dispatch, None);
            }
            StmtKind::FreeAndNil { target } => self.free_and_nil(target),
            StmtKind::Free { target, destroy } => {
                // `docs/spec/objects.md` 3.2, 14.3
                let t = self.borrowed_value(target);
                let id = if *destroy {
                    op::intr::DESTROY
                } else {
                    op::intr::FREE
                };
                self.intrinsic(id, &[t]);
            }
            StmtKind::If {
                branches,
                else_body,
            } => {
                let end = self.new_block();
                let mut chosen = false;
                for (cond, body) in branches {
                    // A condition fixed by the type: only the branch it chooses
                    // (`docs/spec/bytecode.md` 2.23)
                    match self.fixed_cond(cond) {
                        Some(false) => continue,
                        Some(true) => {
                            self.stmt(body);
                            chosen = true;
                            break;
                        }
                        None => {}
                    }
                    let next = self.new_block();
                    let m = self.mark();
                    let first = self.owned.len();
                    self.cond(cond, next, false);
                    self.release(m);
                    // The condition's temporaries are let go on both paths before the bodies run
                    let cond_regs = self.release_owned(first);
                    self.stmt(body);
                    self.jump(end);
                    self.bind(next);
                    self.emit_releases(&cond_regs);
                }
                if let Some(e) = else_body
                    && !chosen
                {
                    self.stmt(e);
                }
                self.bind(end);
            }
            StmtKind::Case {
                selector,
                arms,
                else_body,
            } => self.case_stmt(selector, arms, else_body.as_deref()),
            StmtKind::While { cond, body } => {
                let top = self.new_block();
                let end = self.new_block();
                self.bind(top);
                let m = self.mark();
                let first = self.owned.len();
                self.cond(cond, end, false);
                self.release(m);
                // The condition's temporaries are let go before the body and after the loop
                let cond_regs = self.release_owned(first);
                self.loops.push((end, top, self.cleanup.len()));
                self.stmt(body);
                self.loops.pop();
                self.jump(top);
                self.bind(end);
                self.emit_releases(&cond_regs);
            }
            StmtKind::Repeat { body, cond } => {
                let top = self.new_block();
                let cont = self.new_block();
                let end = self.new_block();
                self.bind(top);
                self.loops.push((end, cont, self.cleanup.len()));
                for x in body {
                    self.stmt(x);
                }
                self.loops.pop();
                self.bind(cont);
                let old = self.set_line(cond.span);
                let m = self.mark();
                let first = self.owned.len();
                let back = self.new_block();
                self.cond(cond, back, false);
                self.release(m);
                // The condition's temporaries are let go after the loop and before the next
                // iteration (`docs/design/lifetime.md` 3.4)
                let cond_regs = self.release_owned(first);
                if cond_regs.is_empty() {
                    self.retarget(cont, back, top);
                } else {
                    self.jump(end);
                    self.bind(back);
                    self.emit_releases(&cond_regs);
                    self.jump(top);
                }
                self.line = old;
                self.bind(end);
            }
            StmtKind::For {
                var,
                from,
                to,
                downto,
                body,
                counter,
                ..
            } => self.for_stmt(*var, *counter, from, to, *downto, body),
            StmtKind::ForIn {
                var,
                coll,
                coll_var,
                idx_var,
                body,
                enumerator: Some(en),
                ..
            } => self.for_in_enum(*var, *coll_var, *idx_var, coll, en, body),
            StmtKind::ForIn {
                var,
                coll,
                coll_var,
                idx_var,
                elem,
                conv,
                body,
                ..
            } => self.for_in(
                &ForIn {
                    var: *var,
                    coll_var: *coll_var,
                    idx_var: *idx_var,
                    coll_ty: coll.ty,
                    elem: *elem,
                    conv: *conv,
                },
                coll,
                body,
            ),
            StmtKind::Break => {
                if let Some(i) = self.loops.len().checked_sub(1) {
                    self.leave(Leave::Break(i));
                }
            }
            StmtKind::Continue => {
                if let Some(i) = self.loops.len().checked_sub(1) {
                    self.leave(Leave::Continue(i));
                }
            }
            StmtKind::Exit(value) => {
                if let Some(v) = value
                    && let Some(rv) = self.r.result_var()
                {
                    let dst = self.reg_of(rv);
                    self.expr_into_var(v, dst);
                }
                self.leave(Leave::Exit);
            }
            StmtKind::Halt(code) => {
                let t = self.alloc(Ty::Integer);
                match code {
                    Some(c) => self.expr(c, t),
                    // `Halt` without a code keeps `ExitCode` (`docs/spec/semantics.md` 5.3)
                    None => {
                        let g = self
                            .program
                            .exit_code
                            .expect("sema adds `ExitCode` for `Halt` without a code");
                        self.op_table(op::LOAD_GLOBAL, t, Operand::Global(g.0));
                    }
                }
                self.terminate(Term::Halt(t));
            }
            StmtKind::SetElem {
                target,
                elem,
                exclude,
                check,
            } => self.set_elem(target, elem, *exclude, *check),
            StmtKind::IncDec {
                target,
                target_ty,
                delta,
                negative,
                calc,
                checks,
            } => self.inc_dec(
                target,
                *target_ty,
                delta.as_deref(),
                *negative,
                *calc,
                *checks,
            ),
            StmtKind::Read { targets, newline } => self.read(targets, *newline),
            StmtKind::Flush => self.intrinsic(op::intr::FLUSH_OUT, &[]),
            StmtKind::Write { args, newline, err } => self.write(args, *newline, *err),
            StmtKind::SetLength { target, lens } => self.set_length(target, lens),
            StmtKind::Insert {
                source,
                target,
                index,
                whole,
            } => self.insert(source, target, index, *whole),
            StmtKind::Delete {
                target,
                index,
                count,
            } => self.delete(target, index, count),
            StmtKind::Exchange {
                target,
                first,
                second,
                span,
            } => self.exchange(target, first, second, *span),
            StmtKind::ArrayMoveRange {
                target,
                from,
                to,
                count,
            } => self.array_move_range(target, [from, to, count]),
            StmtKind::ArraySortRange {
                target,
                index,
                count,
            } => self.array_sort_range(target, index, count),
            StmtKind::HashRelink {
                buckets,
                hashes,
                next,
                count,
            } => self.hash_relink(buckets, hashes, next, count),
            StmtKind::FreeIfClass { value } => {
                // `docs/spec/collections.md` 12.1: `Free` (3.2) once the type is a class; any
                // other type is only evaluated
                if let Ty::Class(_) = value.ty {
                    let t = self.borrowed_value(value);
                    self.intrinsic(op::intr::FREE, &[t]);
                } else {
                    self.expr_any(value);
                }
            }
            StmtKind::CheckDefaultOrder { value } => self.check_default_order(value),
            StmtKind::CheckClassType { value, what } => self.check_class_type(value, what),
            StmtKind::Assert {
                cond,
                message,
                text,
            } => {
                let skip = self.new_block();
                let m = self.mark();
                self.cond(cond, skip, true);
                self.release(m);
                let msg = self.alloc(Ty::String);
                self.load_text(msg, &format!("assertion failed: {text}"));
                if let Some(me) = message {
                    let t = self.alloc(Ty::String);
                    self.load_text(t, " (");
                    self.op3(op::CONCAT_STR, msg, msg, t);
                    self.expr(me, t);
                    self.op3(op::CONCAT_STR, msg, msg, t);
                    self.load_text(t, ")");
                    self.op3(op::CONCAT_STR, msg, msg, t);
                }
                // A new `EAssertionFailed` with the message (`docs/spec/objects.md` 14.8), which
                // `Raise` hands to the VM
                let class = Ty::Class(TypeId(bytecode::exc::ASSERTION_FAILED));
                let x = self.alloc(class);
                self.new_into(x, class);
                self.obj_set(x, bytecode::EXC_SLOT_MESSAGE, msg, Ty::String, true);
                self.terminate(Term::Raise(x));
                self.bind(skip);
            }
            StmtKind::Raise(Some(x)) => {
                // `Raise` takes the reference the temporary holds
                let t = self.alloc(x.ty);
                self.expr(x, t);
                self.terminate(Term::Raise(t));
            }
            StmtKind::Raise(None) => {
                // `raise;`: a new reference of the exception of the innermost `except` block
                let exc = self.current_exception();
                let t = self.alloc(Ty::Class(EXCEPTION));
                self.op2(op::ASSIGN_REF, t, exc);
                self.terminate(Term::Raise(t));
            }
            StmtKind::RaiseOuter(x) => {
                let exc = self.current_exception();
                let t = self.alloc(x.ty);
                self.expr(x, t);
                self.terminate(Term::RaiseOuter(t, exc));
            }
            StmtKind::Try { body, handler } => match handler {
                TryHandler::Except {
                    handlers,
                    else_body,
                } => self.try_except(body, handlers, else_body.as_deref()),
                TryHandler::Finally(fin) => self.try_finally(body, fin),
            },
            StmtKind::Nop => {}
        }
        self.current_overflow = saved_overflow;
        self.release_owned(first_owned);
        self.release(mark);
        self.line = old;
    }

    // ---- `try` (`docs/spec/exceptions.md` 6.2, 6.3) ----

    /// The exception of the lexically innermost `except` block.
    /// `case` (`docs/spec/semantics.md` 4.10): the selector is evaluated once, then compared with
    /// the labels in order; a match jumps to its arm, and no match goes on to the `else` part.
    fn case_stmt(&mut self, selector: &Expr, arms: &[CaseArm], else_body: Option<&[Stmt]>) {
        let end = self.new_block();
        let s = self.expr_any(selector);
        let ty = selector.ty;
        let unsigned = ty == Ty::UInt64;
        let mut targets = Vec::new();
        for arm in arms {
            let target = self.new_block();
            targets.push(target);
            for l in &arm.labels {
                let old = self.set_line(l.low.span);
                let mark = self.mark();
                let lo = self.alloc(ty);
                self.case_label_value(&l.low, ty, lo);
                match &l.high {
                    None if ty == Ty::String => {
                        self.branch(op::JMP_IF_EQ_STR, s, Some(lo), target);
                    }
                    None => self.fused_compare(BinOp::Eq, s, lo, unsigned, target, true),
                    Some(h) => {
                        // `lo <= s <= hi`
                        let skip = self.new_block();
                        self.fused_compare(BinOp::Lt, s, lo, unsigned, skip, true);
                        let hi = self.alloc(ty);
                        self.case_label_value(h, ty, hi);
                        self.fused_compare(BinOp::Le, s, hi, unsigned, target, true);
                        self.bind(skip);
                    }
                }
                self.release(mark);
                self.line = old;
            }
        }
        if let Some(list) = else_body {
            for x in list {
                self.stmt(x);
            }
        }
        self.jump(end);
        for (arm, target) in arms.iter().zip(targets) {
            self.bind(target);
            if let Some(b) = &arm.body {
                self.stmt(b);
            }
            self.jump(end);
        }
        self.bind(end);
    }

    /// Loads the constant of a `case` label as a value of the selector's type `ty`.
    fn case_label_value(&mut self, label: &Expr, ty: Ty, dst: Reg) {
        let c = label
            .value
            .as_ref()
            .expect("a case label is a constant (checked by sema)");
        self.load_const(dst, c, ty);
    }

    /// Whether the statement is in the body of a `try..except` or a `try..finally` written in
    /// the source (whose handler may read a variable after an exception; the implicit
    /// `finally` blocks, such as that of `for..in`, read none).
    fn in_try(&self) -> bool {
        self.guarded > 0
    }

    fn current_exception(&self) -> Reg {
        self.cleanup
            .iter()
            .rev()
            .find_map(|c| match c {
                Cleanup::Except(r) => Some(*r),
                Cleanup::Finally { .. } => None,
            })
            .expect("semantic analysis allows `raise;` only inside an `except` block")
    }

    /// The layout position the next bound block gets.
    fn next_position(&self) -> BlockId {
        self.order.len() as BlockId
    }

    fn try_except(&mut self, body: &[Stmt], handlers: &[Handler], else_body: Option<&[Stmt]>) {
        let exc = self.alloc_kind_owning(Ty::Class(EXCEPTION), TempKind::Exception);
        let after = self.new_block();
        let body_first = self.next_position();
        let start = self.new_block();
        self.bind(start);
        self.guarded += 1;
        for s in body {
            self.stmt(s);
        }
        self.guarded -= 1;
        self.jump(after);
        let handler_first = self.next_position();
        let dispatch = self.new_block();
        self.bind(dispatch);
        self.cleanup.push(Cleanup::Except(exc));
        for h in handlers {
            let old = self.set_line(h.span);
            let next = self.new_block();
            let m = self.mark();
            // `docs/spec/objects.md` 8.3: the `is` of the exception's class
            let t = self.alloc_borrow(Ty::Boolean);
            self.op2(op::MOVE, t, exc);
            self.op_table(op::IS_CLASS, t, Operand::Type(h.class));
            self.branch(op::JMP_IF_FALSE, t, None, next);
            self.release(m);
            if let Some(v) = h.var {
                // `E` borrows the exception the block holds
                self.op2(op::MOVE, self.reg_of(v), exc);
                if self.r.vars[v.0 as usize].cell {
                    self.copy_to_cell(v);
                }
            }
            self.line = old;
            if let Some(b) = &h.body {
                self.stmt(b);
            }
            self.op1(op::RELEASE, exc);
            self.jump(after);
            self.bind(next);
        }
        match else_body {
            Some(list) => {
                for s in list {
                    self.stmt(s);
                }
                self.op1(op::RELEASE, exc);
                self.jump(after);
            }
            // No handler matched: raise the same exception again
            None => self.terminate(Term::Raise(exc)),
        }
        self.cleanup.pop();
        let handler_last = self.next_position() - 1;
        self.tries.push(TryEntry {
            finally: false,
            body: (body_first, handler_first - 1),
            handler: (handler_first, handler_last),
            reg: exc,
            exc: None,
        });
        self.bind(after);
    }

    fn try_finally(&mut self, body: &[Stmt], fin: &[Stmt]) {
        let scope = self.finally_begin();
        let depth = self.cleanup.len();
        self.cleanup.push(Cleanup::Finally {
            state: scope.state,
            fin: scope.fin,
            conts: Vec::new(),
        });
        self.guarded += 1;
        for s in body {
            self.stmt(s);
        }
        self.guarded -= 1;
        let Some(Cleanup::Finally { conts, .. }) = self.cleanup.pop() else {
            unreachable!("pushed above")
        };
        let handler_first = self.finally_handler(&scope);
        for s in fin {
            self.stmt(s);
        }
        self.finally_end(&scope, handler_first, conts.iter().map(|c| c.0).collect());
        // The jumps that went through the block go on from here
        for (cont, leave) in conts {
            self.bind(cont);
            self.leave_from(depth, leave);
        }
        self.bind(scope.after);
    }

    /// The start of the body of a `try..finally` (its registers and blocks).
    fn finally_begin(&mut self) -> FinallyScope {
        // The state and the exception are adjacent registers (`EndFinally A`, `r(A+1)`)
        let state = self.alloc_kind_owning(Ty::Integer, TempKind::State);
        let exc = self.alloc_kind_owning(Ty::Class(EXCEPTION), TempKind::Exception);
        let fin = self.new_block();
        let after = self.new_block();
        let body_first = self.next_position();
        let start = self.new_block();
        self.bind(start);
        FinallyScope {
            state,
            exc,
            fin,
            after,
            body_first,
        }
    }

    /// The end of the body of a `try..finally` and the start of its `finally` block; returns the
    /// block's first position.
    fn finally_handler(&mut self, scope: &FinallyScope) -> BlockId {
        if self.cur.is_some() {
            self.op1(op::LOAD_ZERO, scope.state);
        }
        let handler_first = self.next_position();
        self.bind(scope.fin);
        handler_first
    }

    /// The end of the `finally` block (`EndFinally`, going on to `scope.after` or one of the
    /// jumps' `targets`) and the entry of the handler table.
    fn finally_end(&mut self, scope: &FinallyScope, handler_first: BlockId, targets: Vec<BlockId>) {
        // The block that ends with `EndFinally` (bound now if the last statement left none open)
        self.current();
        let handler_last_pos = self.next_position() - 1;
        self.terminate(Term::EndFinally {
            state: scope.state,
            next: scope.after,
            targets,
        });
        self.tries.push(TryEntry {
            finally: true,
            body: (scope.body_first, handler_first - 1),
            handler: (handler_first, handler_last_pos),
            reg: scope.state,
            exc: Some(scope.exc),
        });
    }

    /// `TFoo.Create(…)` (`docs/spec/objects.md` 3.1, 14.3): `New`, then the constructor and
    /// `AfterConstruction` in a `try` whose `finally` block destroys the instance after an
    /// exception (`DestroyFailed`). The arguments are evaluated before `New`, so an exception in
    /// them leaves no instance behind. Without a constructor or `AfterConstruction` of the
    /// class's own, `New` is all of it.
    #[allow(clippy::too_many_arguments)]
    fn new_instance(
        &mut self,
        class: Ty,
        ctor: Option<RoutineId>,
        args: &[CallArg],
        after: bool,
        of: Option<&Expr>,
        dispatch: Dispatch,
        dst: Reg,
    ) {
        // A built-in constructor with an argument is `Exception.Create(Msg)` / `CreateFmt`
        let msg = match (ctor, args) {
            (None, [CallArg::Value(m)]) => Some(m),
            _ => None,
        };
        // The message of `Exception.Create(Msg)` / `CreateFmt`, written to `Message` after `New`
        // (`docs/spec/objects.md` 8.2, 14.8)
        let write_msg = |s: &mut Self, msg: Option<Reg>, inst: Reg| {
            if let Some(m) = msg {
                s.obj_set(inst, bytecode::EXC_SLOT_MESSAGE, m, Ty::String, true);
            }
        };
        if ctor.is_none() && !after && of.is_none() {
            let m = msg.map(|m| self.expr_any(m));
            self.new_into(dst, class);
            write_msg(self, m, dst);
            return;
        }
        // The class reference of `C.Create(…)` is evaluated first (a class reference is not
        // counted), then the arguments (`docs/spec/objects.md` 3.1)
        let of = of.map(|c| {
            let r = self.alloc_arg(Some(c.ty));
            self.expr(c, r);
            r
        });
        let msg = msg.map(|m| self.expr_any(m));
        // The instance stays in a temporary below the call's frame until the sequence ends (a
        // handle of a class managed by hand is not counted)
        let inst = self.alloc_arg(Some(class));
        let new = NewIn { class, inst, of };
        let scope = match ctor {
            Some(r) => {
                let mut all = Vec::with_capacity(args.len() + 1);
                // `Self`: the call puts the new instance there (`NewInstance`)
                all.push(CallArg::Value(Expr {
                    kind: ExprKind::Error,
                    ty: class,
                    value: None,
                    span: TextRange::default(),
                }));
                all.extend(args.iter().cloned());
                self.call_routine_with(r, &all, dispatch, None, Some(new))
                    .0
                    .expect("a constructor call begins the `try`")
            }
            None => {
                self.new_in(&new);
                let scope = self.finally_begin();
                write_msg(self, msg, inst);
                scope
            }
        };
        if after || of.is_some() {
            let m = self.mark();
            // Through a class reference the class may be a descendant overriding it
            let name = match self
                .program
                .types
                .class_vmt(class.type_id().expect("a class"))
                [bytecode::VMT_AFTER_CONSTRUCTION as usize]
            {
                crate::sema::types::VmtEntry::Func(r) => self.ml.func_names[r.0 as usize].clone(),
                _ => "TObject.AfterConstruction".to_string(),
            };
            let a = self.alloc_arg(Some(class));
            self.op2(op::MOVE, a, inst);
            let callee = Operand::Virtual {
                index: bytecode::VMT_AFTER_CONSTRUCTION,
                name,
            };
            self.call(op::CALL_VIRTUAL, None, callee, vec![a], a);
            self.release(m);
        }
        let handler_first = self.finally_handler(&scope);
        // After an exception (state 1; `docs/spec/exceptions.md` 6.2) the instance is destroyed
        let m = self.mark();
        let one = self.alloc_arg(Some(Ty::Integer));
        self.load_int(one, 1);
        let skip = self.new_block();
        self.branch(op::JMP_IF_NE_INT, scope.state, Some(one), skip);
        self.intrinsic(op::intr::DESTROY_FAILED, &[inst]);
        self.jump(skip);
        self.bind(skip);
        self.release(m);
        self.finally_end(&scope, handler_first, Vec::new());
        self.bind(scope.after);
        // A reference counted instance holds the creation's reference, which moves to `dst`
        self.move_value(class, dst, inst);
    }

    /// The new instance of a creation (`New`, or `NewOf` through a class reference).
    fn new_in(&mut self, n: &NewIn) {
        match n.of {
            Some(c) => {
                self.op1(op::LOAD_ZERO, n.inst);
                self.intrinsic(op::intr::NEW_OF, &[n.inst, c]);
            }
            None => self.new_into(n.inst, n.class),
        }
    }

    /// `New dst, class`. A making instruction lets go of the old value of `dst` (by its
    /// contents), so a register that does not own its value (a call argument, which may hold a
    /// handle borrowed by an earlier call) is cleared first (`docs/spec/objects.md` 14.3,
    /// `docs/design/lifetime.md` 3.1).
    fn new_into(&mut self, dst: Reg, class: Ty) {
        if !self.owns(dst) {
            self.op1(op::LOAD_ZERO, dst);
        }
        self.op_table(op::NEW, dst, Operand::Type(class));
    }

    /// `Exit`, `Break` or `Continue`: releases the exceptions of the `except` blocks it leaves and
    /// goes through the `finally` blocks on its way (`docs/spec/exceptions.md` 6.2).
    fn leave(&mut self, l: Leave) {
        self.leave_from(self.cleanup.len(), l);
    }

    /// `leave` from inside the first `depth` entries of `cleanup`.
    fn leave_from(&mut self, depth: usize, l: Leave) {
        let target = match l {
            Leave::Exit => 0,
            Leave::Break(i) | Leave::Continue(i) => self.loops[i].2,
        };
        let mut d = depth;
        while d > target {
            d -= 1;
            match &mut self.cleanup[d] {
                Cleanup::Except(r) => {
                    let r = *r;
                    self.op1(op::RELEASE, r);
                }
                Cleanup::Finally { state, fin, conts } => {
                    let (state, fin) = (*state, *fin);
                    let k = conts.len() as i64 + 1;
                    let cont = self.blocks.len() as BlockId;
                    conts.push((cont, l));
                    self.new_block();
                    self.load_int(state, k + 2);
                    self.jump(fin);
                    return;
                }
            }
        }
        match l {
            Leave::Exit => self.emit_return(),
            Leave::Break(i) => self.jump(self.loops[i].0),
            Leave::Continue(i) => self.jump(self.loops[i].1),
        }
    }

    fn place_type(&self, p: &Place) -> Ty {
        match p {
            Place::Var(v, _) => self.r.vars[v.0 as usize].ty,
            Place::Global(g) => self.program.globals[g.0 as usize].ty,
            Place::StrIndex { .. } => Ty::Char,
            Place::Field { ty, .. } | Place::Elem { ty, .. } => *ty,
            Place::ObjField { ty, .. } => *ty,
        }
    }

    /// Whether the variable's register may be written directly (not for reference parameters).
    fn direct_var_reg(&self, p: &Place) -> Option<Reg> {
        match p {
            Place::Var(v, _) if !self.is_ref_param(*v) => Some(self.reg_of(*v)),
            _ => None,
        }
    }

    fn assign(&mut self, target: &Place, value: &Expr) {
        if let Some((first, ty)) = self.expanded_place(target) {
            return self.assign_to_fields(first, ty, value);
        }
        if let Some((first, ty)) = self.expanded_expr(value) {
            return self.assign_from_fields(target, first, ty);
        }
        if let Some((steps, kind)) = append_steps(target, self.place_type(target), value) {
            return self.append(target, steps, kind);
        }
        match target {
            Place::Var(..) | Place::Global(_) => {
                // A `[weak]` variable does not take the value's reference: the value is written
                // uncounted, from the register of a variable or a container read without
                // counting, or from a temporary that keeps a new value (`docs/spec/objects.md`
                // 4.4)
                let weak = matches!(self.place_type(target), Ty::Weak(_));
                if weak {
                    let v = self.borrowed_value(value);
                    self.store_place(target, v);
                } else if let Some(reg) = self.direct_var_reg(target) {
                    self.expr_into_var(value, reg);
                } else {
                    // A local variable on the right is stored from its own register (the store
                    // counts it): a counted copy in a temporary would hold the value until the
                    // routine ends (`docs/design/lifetime.md` 3.4)
                    let t = self.expr_any(value);
                    self.store_place(target, t);
                }
            }
            Place::StrIndex { base, index, span } => {
                // Index, right-hand side, then the target (`semantics.md` 3.1; left to right)
                let part = is_part(base).then(|| self.eval_part(base));
                let i = self.alloc(index.ty);
                self.expr(index, i);
                let v = self.alloc(value.ty);
                self.expr(value, v);
                // The string is changed in place through its place read without counting, so
                // that the place's own reference is the only one and the write needs no copy
                // (`docs/spec/types.md` 9.1)
                let old = self.set_line(*span);
                let f = |this: &mut Self, t: Reg| this.op3(op::STR_SET_INDEX, t, i, v);
                if let Some(part) = part {
                    self.part_check(&part);
                    self.modify_part(&part, f);
                } else {
                    self.modify_root(base, f);
                }
                self.line = old;
            }
            Place::Field { .. } | Place::Elem { .. } => {
                // The indices of the target, the right-hand side, then the range checks and the
                // target itself (`docs/spec/types.md` chapter 3)
                let part = self.eval_part(target);
                let v = self.weak_aware_value(target, value);
                self.part_check(&part);
                self.part_store(&part, v);
            }
            Place::ObjField { base, slot, ty, .. } => {
                // The instance, then the right-hand side (`docs/spec/objects.md` 2.6), then the
                // write. A call on the right could change the variable holding the handle, so
                // the handle is then copied first
                let b =
                    self.obj_handle(base, expr_has_call(value) && !self.stable_var(base, value));
                let v = self.weak_aware_value(target, value);
                let h = Hop::obj(base.ty, self.obj_slot(base.ty, *slot), *ty);
                self.hop_store(&h, b, v);
            }
        }
    }

    /// The register with the right-hand side of an assignment to `target`: for a `[weak]`
    /// place, which does not count it, a value read without counting where it can be
    /// (`borrowed_value`); otherwise as `expr_any`.
    fn weak_aware_value(&mut self, target: &Place, value: &Expr) -> Reg {
        if matches!(self.place_type(target), Ty::Weak(_)) {
            self.borrowed_value(value)
        } else {
            self.expr_any(value)
        }
    }

    /// `T := T + X1 + … + Xn` for a string or `UTF8String` place `T` (`append_steps`;
    /// `docs/spec/types.md` 9.1): the place's indices, a counted copy of its value, the operands
    /// joined into one value, then `AppendStr` / `AppendU8` on the place changed in place
    /// (`modify_root` / `modify_part`), which appends to the place's own string when nothing else
    /// refers to it. The operands run after the copy is taken: if they change the place, the copy
    /// is no longer its value and the instruction makes a new string from the copy, as the
    /// left-to-right order of `T + X1` asks (`docs/spec/semantics.md` 3.1).
    fn append(&mut self, target: &Place, steps: &[BinStep], kind: AppendKind) {
        let ty = self.place_type(target);
        let part = is_part(target).then(|| {
            let part = self.eval_part(target);
            self.part_check(&part);
            part
        });
        let snap = self.alloc(ty);
        match &part {
            Some(part) => self.part_load(part, snap),
            None => self.load_place(target, snap),
        }
        let (join, joined_ty) = match kind {
            AppendKind::U8 => (op::CONCAT_U8, Ty::Utf8String),
            AppendKind::Str | AppendKind::U8ViaStr => (op::CONCAT_STR, Ty::String),
        };
        // A single operand is only read by the append, so a variable's register serves as it
        // is; several are joined in a temporary first (three or more strings at once, as in
        // `binary_lazy`)
        let mut x = if let [step] = steps {
            let old = self.set_line(step.span);
            let x = self.expr_any(&step.rhs);
            self.line = old;
            x
        } else if join == op::CONCAT_STR && steps.len() >= 3 {
            let x = self.alloc(joined_ty);
            let operands: Vec<(&Expr, TextRange)> =
                steps.iter().map(|s| (&s.rhs, s.span)).collect();
            self.concat_n(x, &operands);
            x
        } else {
            let x = self.alloc(joined_ty);
            for (k, step) in steps.iter().enumerate() {
                let old = self.set_line(step.span);
                if k == 0 {
                    self.expr(&step.rhs, x);
                } else {
                    let r = self.expr_any(&step.rhs);
                    self.op3(join, x, x, r);
                }
                self.line = old;
            }
            x
        };
        if kind == AppendKind::U8ViaStr {
            // `UTF8String(string(T) + Y)` is `T` and `UTF8String(Y)` joined: `string(T)` has no
            // lone surrogate for `Y` to pair with
            let y = self.alloc(Ty::Utf8String);
            self.convert(Conversion::StrToU8, Ty::String, y, x);
            x = y;
        }
        let opc = match kind {
            AppendKind::Str => op::APPEND_STR,
            AppendKind::U8 | AppendKind::U8ViaStr => op::APPEND_U8,
        };
        let old = self.set_line(steps[0].span);
        let f = |this: &mut Self, t: Reg| this.op3(opc, t, snap, x);
        match &part {
            Some(part) => self.modify_part(part, f),
            None => {
                self.modify_root(target, f);
            }
        }
        // The copy is let go at once: held on, it would be a second reference to the place's
        // string at the next append in another statement (`S := S + 'a'; S := S + 'b'` in a
        // loop), which would then copy. A string has no destructor, so when is not observable
        self.op1(op::RELEASE, snap);
        self.line = old;
    }

    /// Puts an expression into a variable's register. The value is made in a temporary and then
    /// moved to the variable, so that if an instruction raises on the way, the variable keeps its
    /// old value (`docs/spec/semantics.md` 3.1; `Z := X + 1` under `{$Q+}`, `T := (X + Y) + Z`
    /// near the memory limit). Where that cannot be observed, the temporary is replaced by the
    /// variable and dropped ([`Self::write_in_place`]). Outside the bodies of `try` statements,
    /// the value is made in the variable: an exception then leaves the routine, and nothing
    /// reads the variable again.
    fn expr_into_var(&mut self, e: &Expr, reg: Reg) {
        let reads = self.expr_sees_var(e, reg);
        if !self.in_try() && !reads {
            self.expr(e, reg);
            return;
        }
        let ty = self.reg_type(reg).unwrap_or(e.ty);
        let t = self.alloc(ty);
        if reads || self.owns(t) != self.owns(reg) {
            self.expr(e, t);
            self.move_value(e.ty, reg, t);
            return;
        }
        let start = self.region_start();
        // `write_in_place` renumbers the temporaries made by the expression in its instructions,
        // its handler table entries (a constructor call's) and `owned`: an expression leaves no
        // cleanups or loops behind that could hold them
        let outer = (self.cleanup.len(), self.loops.len());
        self.expr(e, t);
        debug_assert_eq!(
            outer,
            (self.cleanup.len(), self.loops.len()),
            "an expression leaves the cleanups and the loops as they were"
        );
        if !self.write_in_place(&start, t, reg) {
            self.move_value(e.ty, reg, t);
        }
    }

    /// Whether evaluating the expression may read or write the variable `reg` before it ends, so
    /// that the value cannot be made in the variable's register: the expression reads it, or it
    /// is used by a nested routine (`docs/spec/semantics.md` 4.9) and the expression calls code,
    /// which may reach that routine (`Z := (X + 1) + Peek` with `Peek` reading `Z`).
    fn expr_sees_var(&self, e: &Expr, reg: Reg) -> bool {
        let nested = match reg {
            Reg::Var(v) => self.r.vars.get(v as usize).is_some_and(|v| v.captured),
            _ => false,
        };
        expr_reads_reg(e, reg) || (nested && expr_has_call(e))
    }

    /// Where the instructions about to be emitted start ([`Region`]).
    fn region_start(&mut self) -> Region {
        let block = self.current();
        Region {
            block,
            insts: self.blocks[block as usize].insts.len(),
            blocks: self.blocks.len(),
            tries: self.tries.len(),
        }
    }

    /// Calls `each` with every register operand of the instructions and terminators emitted since
    /// `start`, and every register of the handler table entries added since.
    fn region_regs(&mut self, start: &Region, mut each: impl FnMut(&mut Reg)) {
        for entry in &mut self.tries[start.tries..] {
            each(&mut entry.reg);
            if let Some(e) = &mut entry.exc {
                each(e);
            }
        }
        let first = &mut self.blocks[start.block as usize];
        for inst in &mut first.insts[start.insts..] {
            inst.each_reg_mut(&mut each);
        }
        if let Some((term, _)) = &mut first.term {
            term.each_reg_mut(&mut each);
        }
        for b in &mut self.blocks[start.blocks..] {
            for inst in &mut b.insts {
                inst.each_reg_mut(&mut each);
            }
            if let Some((term, _)) = &mut b.term {
                term.each_reg_mut(&mut each);
            }
        }
    }

    /// After the value of an assignment to the variable `reg` was made in the new temporary `t`
    /// by the instructions since `start`: whether writing `reg` in place of `t` there cannot be
    /// observed, and if so, does that and drops `t` (as if it had never been created). That holds
    /// when `t` is written only by the last instruction (no instruction runs after the variable
    /// is written), or when none of the instructions may raise (`opt::may_raise`; only plain
    /// branches end the blocks).
    fn write_in_place(&mut self, start: &Region, t: Reg, reg: Reg) -> bool {
        let Reg::Temp(n) = t else {
            unreachable!("`alloc` returns a temporary")
        };
        let mut uses = 0;
        self.region_regs(start, |r| uses += u32::from(*r == t));
        let first = &self.blocks[start.block as usize];
        let only_last = uses == 1
            && self.blocks.len() == start.blocks
            && self.cur == Some(start.block)
            && first.insts.len() > start.insts
            && first.insts.last().is_some_and(|i| {
                let mut found = false;
                i.each_reg(|r| found |= r == t);
                found
            });
        let safe = || {
            let plain = |b: &BlockBuild| {
                matches!(
                    b.term,
                    None | Some((
                        Term::Jmp(_) | Term::Branch { .. } | Term::BranchImm { .. },
                        _
                    ))
                )
            };
            first.insts[start.insts..]
                .iter()
                .all(|i| !opt::may_raise(i))
                && plain(first)
                && self.blocks[start.blocks..]
                    .iter()
                    .all(|b| b.insts.iter().all(|i| !opt::may_raise(i)) && plain(b))
        };
        if !(only_last || safe()) {
            return false;
        }
        // `t` is the newest temporary but for those made by the expression: they and their
        // stack slots move down by one
        let slot = self.temps[n as usize].slot;
        self.region_regs(start, |r| match *r {
            _ if *r == t => *r = reg,
            Reg::Temp(k) if k > n => *r = Reg::Temp(k - 1),
            _ => {}
        });
        self.owned.retain(|&r| r != t);
        for r in &mut self.owned {
            if let Reg::Temp(k) = r
                && *k > n
            {
                *k -= 1;
            }
        }
        self.temps.remove(n as usize);
        if let Some(s) = slot {
            for temp in &mut self.temps[n as usize..] {
                if let Some(x) = &mut temp.slot
                    && *x > s
                {
                    *x -= 1;
                }
            }
            self.next_slot -= 1;
        }
        true
    }

    /// `is` / `as` / a conversion by type name (`docs/spec/objects.md` 2.8, 2.9, 14.3). A variable
    /// `dst` is written only after the check passes, through a temporary, so that it never holds an
    /// instance of a class its type does not allow (nor a handle in a `Boolean`) after the
    /// exception; a temporary `dst` is not read after the exception and is checked in place,
    /// unless it does not own its value and the operand is counted (then a counted temporary is
    /// checked and moved into it).
    /// A target that is an interface uses `IsIntf` / `AsIntf` (7.6, 14.7).
    fn class_check(&mut self, operand: &Expr, class: TypeId, op: ClassCheckOp, ty: Ty, dst: Reg) {
        let intf = self.program.types.is_interface(class);
        let class = if intf {
            Ty::Intf(class)
        } else {
            Ty::Class(class)
        };
        let in_place = matches!(dst, Reg::Temp(_));
        if op == ClassCheckOp::Is {
            // `IsClass` / `IsIntf` turns an uncounted copy of the handle into the result
            let b = self.borrowed_value(operand);
            let t = if in_place {
                dst
            } else {
                self.alloc_borrow(Ty::Boolean)
            };
            self.op2(op::MOVE, t, b);
            if intf {
                self.intf_test(op::intr::IS_INTF, t, class);
            } else {
                self.op_table(op::IS_CLASS, t, Operand::Type(class));
            }
            if t != dst {
                self.op2(op::MOVE, dst, t);
            }
            return;
        }
        // a temporary of an uncounted class type on the stack of temporaries cannot hold the
        // counted value of a counted operand (`TCircle(Obj)` with `Obj: TObject`): nothing would
        // let go of it, and writing it would let go of whatever the shared slot held before
        let t = if in_place && (self.owns(dst) || !self.managed(operand.ty)) {
            dst
        } else {
            self.alloc(operand.ty)
        };
        self.expr(operand, t);
        if op == ClassCheckOp::RefCast {
            let m = self.mark();
            let k = self.alloc_arg(Some(Ty::Integer));
            self.op_table(op::LOAD_INT, k, Operand::Type(class));
            self.intrinsic(op::intr::AS_CLASS_REF, &[t, k]);
            self.release(m);
        } else if intf {
            self.intf_test(op::intr::AS_INTF, t, class);
        } else {
            self.op_table(op::AS_CLASS, t, Operand::Type(class));
        }
        if t != dst {
            self.move_value(ty, dst, t);
        }
    }

    /// `IsIntf` / `AsIntf` of the handle in `r` against the interface `intf`, whose type index is
    /// loaded into a temporary (`docs/spec/objects.md` 14.7).
    fn intf_test(&mut self, id: u8, r: Reg, intf: Ty) {
        let m = self.mark();
        let k = self.alloc(Ty::Integer);
        self.op_table(op::LOAD_INT, k, Operand::Type(intf));
        self.intrinsic(id, &[r, k]);
        self.release(m);
    }

    /// `Supports(X, IA[, V])` / `Supports(C, IA)` (`docs/spec/objects.md` 7.6, 14.7). With `V`,
    /// `X` is held counted in a temporary while `V` is written: with `X` as `IA` when the instance
    /// implements it, `nil` otherwise.
    fn supports(
        &mut self,
        operand: &Expr,
        intf: TypeId,
        target: Option<&Place>,
        class_ref: bool,
        dst: Reg,
    ) {
        let intf = Ty::Intf(intf);
        if class_ref {
            let c = self.borrowed_value(operand);
            let k = self.alloc(Ty::Integer);
            self.op_table(op::LOAD_INT, k, Operand::Type(intf));
            self.intrinsic_pair(
                op::intr::CLASS_SUPPORTS,
                dst,
                [(c, operand.ty), (k, Ty::Integer)],
            );
            return;
        }
        let Some(target) = target else {
            let b = self.borrowed_value(operand);
            let t = self.alloc_borrow(Ty::Boolean);
            self.op2(op::MOVE, t, b);
            self.intf_test(op::intr::IS_INTF, t, intf);
            self.op2(op::MOVE, dst, t);
            return;
        };
        let x = self.alloc(operand.ty);
        self.expr(operand, x);
        let t = self.alloc_borrow(Ty::Boolean);
        self.op2(op::MOVE, t, x);
        self.intf_test(op::intr::IS_INTF, t, intf);
        let other = self.new_block();
        let end = self.new_block();
        self.branch(op::JMP_IF_FALSE, t, None, other);
        self.store_place(target, x);
        self.jump(end);
        self.bind(other);
        let z = self.alloc(intf);
        self.op1(op::LOAD_ZERO, z);
        self.store_place(target, z);
        self.bind(end);
        self.op2(op::MOVE, dst, t);
    }

    /// Reads the value of a place into `dst` (`Move` for a variable). A managed value is counted
    /// (`AssignRef` and the `…Ref` loads; `docs/design/lifetime.md` 3.5).
    fn load_place(&mut self, p: &Place, dst: Reg) {
        let ty = self.place_type(p);
        match p {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    let ops = (op::LOAD_REF, op::LOAD_REF_REF);
                    self.get_value(ty, true, ops, dst, vec![Operand::Reg(r)]);
                } else if r != dst {
                    self.copy_value(ty, dst, r);
                }
            }
            Place::Global(g) => {
                let ops = (op::LOAD_GLOBAL, op::LOAD_GLOBAL_REF);
                self.get_value(ty, true, ops, dst, vec![Operand::Global(g.0)]);
            }
            Place::StrIndex { base, index, .. } => {
                // The copy of the string holds no object: it is let go of when the statement ends
                // (`docs/design/lifetime.md` 3.4)
                let b = self.alloc(Ty::String);
                self.load_place(base, b);
                self.owned.push(b);
                let i = self.alloc(index.ty);
                self.expr(index, i);
                self.op3(op::STR_INDEX, dst, b, i);
            }
            Place::Field { .. } | Place::Elem { .. } => {
                let part = self.eval_part(p);
                self.part_check(&part);
                self.part_load(&part, dst);
            }
            Place::ObjField { base, slot, ty, .. } => {
                let b = self.obj_handle(base, false);
                let h = Hop::obj(base.ty, self.obj_slot(base.ty, *slot), *ty);
                self.hop_load(&h, b, dst, true);
            }
        }
    }

    /// Writes the value of `src` to a place. A managed value is counted (`src` keeps its reference).
    fn store_place(&mut self, p: &Place, src: Reg) {
        let ty = self.place_type(p);
        match p {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    self.set_value(
                        ty,
                        true,
                        (op::STORE_REF, op::STORE_REF_REF),
                        vec![Operand::Reg(r), Operand::Reg(src)],
                        src,
                        (op::LOAD_REF, vec![Operand::Reg(r)]),
                    );
                } else if r != src {
                    self.copy_value(ty, r, src);
                }
            }
            Place::Global(g) => self.set_value(
                ty,
                true,
                (op::STORE_GLOBAL, op::STORE_GLOBAL_REF),
                vec![Operand::Reg(src), Operand::Global(g.0)],
                src,
                (op::LOAD_GLOBAL, vec![Operand::Global(g.0)]),
            ),
            Place::StrIndex { .. } => {
                unreachable!("string elements are not stored this way")
            }
            Place::ObjField { base, slot, ty, .. } => {
                let b = self.obj_handle(base, false);
                let h = Hop::obj(base.ty, self.obj_slot(base.ty, *slot), *ty);
                self.hop_store(&h, b, src);
            }
            Place::Field { .. } | Place::Elem { .. } => {
                let part = self.eval_part(p);
                self.part_check(&part);
                self.part_store(&part, src);
            }
        }
    }

    // ---- Fields and elements (`docs/spec/types.md` chapter 3, `docs/design/lifetime.md` 3.6,
    // `docs/design/records.md`) ----

    /// Evaluates the indices of a field or element place, outer first, into temporaries
    /// (0-based; the range checks come later with `part_check`).
    fn eval_part<'p>(&mut self, p: &'p Place) -> Part<'p> {
        match p {
            Place::Field {
                base, index, ty, ..
            } => {
                let mut part = self.eval_part(base);
                part.steps.push(PartStep {
                    slot: SlotRef::Field(*index),
                    ty: *ty,
                });
                part
            }
            Place::Elem {
                base,
                index,
                ty,
                span,
            } => {
                let container = self.place_type(base);
                let mut part = self.eval_part(base);
                let slot = if container.is_dyn_indexed() {
                    // The VM checks the index of a dynamic or open array when it uses it
                    let reg = self.alloc_borrow(Ty::Int64);
                    self.expr(index, reg);
                    SlotRef::Dyn(reg)
                } else {
                    let (index, check) = self.elem_idx(container, index, *span);
                    SlotRef::Elem { index, check }
                };
                part.steps.push(PartStep { slot, ty: *ty });
                part
            }
            Place::ObjField { base, .. } => {
                // The instance comes first, like the indices: the handle is read here, before
                // the right-hand side, so a call there that changes the variable holding it does
                // not move the write to another instance (`docs/spec/objects.md` 2.2). An
                // instance of a reference counted class is counted for it (an owning temporary)
                let h = self.alloc(base.ty);
                self.expr(base, h);
                Part {
                    root: p,
                    steps: Vec::new(),
                    handle: Some(h),
                    root_reg: None,
                }
            }
            _ => Part {
                root: p,
                steps: Vec::new(),
                handle: None,
                root_reg: None,
            },
        }
    }

    /// The 0-based index of an element of the static array `arr` in a new temporary, and the
    /// range check it still needs (`None` for a constant index, which sema checked).
    fn elem_index(&mut self, arr: Ty, index: &Expr, span: TextRange) -> (Reg, Option<IndexCheck>) {
        let (idx, check) = self.elem_idx(arr, index, span);
        (self.idx_reg(idx), check)
    }

    /// [`FuncLower::elem_index`], keeping a constant index a constant.
    fn elem_idx(&mut self, arr: Ty, index: &Expr, span: TextRange) -> (Idx, Option<IndexCheck>) {
        let (ity, lo, hi, _) = self
            .program
            .types
            .array(arr)
            .expect("an element place indexes a static array");
        if let Some(v) = index.value.as_ref().and_then(Const::as_int) {
            return (Idx::Const((v - lo) as i64), None);
        }
        let t = self.alloc_borrow(Ty::Int64);
        self.expr(index, t);
        if lo != 0 {
            if (i8::MIN as i128..=i8::MAX as i128).contains(&-lo) {
                self.op_imm(op::ADD_INT_I, t, t, -lo as i64);
            } else {
                let k = self.alloc_borrow(Ty::Int64);
                self.load_int(k, lo as i64);
                self.op3(op::SUB_INT, t, t, k);
            }
        }
        let msg = format!(
            "`{}` ({}..{})",
            self.program.types.name(arr),
            bound_text(ity, lo),
            bound_text(ity, hi)
        );
        let check = IndexCheck {
            len: (hi - lo + 1) as i64,
            msg,
            span,
        };
        (Idx::Reg(t), Some(check))
    }

    /// The register with an index (a constant loaded into a new temporary).
    fn idx_reg(&mut self, i: Idx) -> Reg {
        match i {
            Idx::Reg(r) => r,
            Idx::Const(k) => {
                let t = self.alloc_borrow(Ty::Int64);
                self.load_int(t, k);
                t
            }
        }
    }

    /// `ChkIndex` for each element index of the place that is not a constant.
    fn part_check(&mut self, part: &Part<'_>) {
        for step in &part.steps {
            if let SlotRef::Elem {
                index: Idx::Reg(reg),
                check: Some(c),
            } = &step.slot
            {
                self.chk_index(*reg, c);
            }
        }
    }

    fn chk_index(&mut self, reg: Reg, c: &IndexCheck) {
        let old = self.set_line(c.span);
        let len = self.alloc_borrow(Ty::Int64);
        self.load_int(len, c.len);
        let msg: Vec<u16> = c.msg.encode_utf16().collect();
        self.plain(
            op::CHK_INDEX,
            vec![Operand::Reg(reg), Operand::Reg(len), Operand::Str(msg)],
        );
        self.line = old;
    }

    /// A temporary on the stack of temporaries that never owns a value: a container read without
    /// counting (`docs/design/lifetime.md` 3.6), a slot number, a length.
    fn alloc_borrow(&mut self, ty: Ty) -> Reg {
        self.alloc_kind(Some(ty), TempKind::Value)
    }

    /// Whether a value of `ty` is laid out flat in its containers (`docs/design/records.md` 3.2).
    fn is_flat(&self, ty: Ty) -> bool {
        self.program.types.flat_width(ty).is_some()
    }

    /// Whether the variable's register holds a reference slot that may be of kind 4 (a lent flat
    /// place; `docs/spec/bytecode.md` 1.5): its value is a record laid out flat, read and written
    /// whole with `LoadRefRef` / `StoreRefRef` only. A static array is passed to a `var`
    /// parameter as a block (`PlaceBox`).
    fn place_ref(&self, v: VarId) -> bool {
        let ty = self.r.vars[v.0 as usize].ty;
        self.is_ref_param(v) && matches!(ty, Ty::Record(_)) && self.is_flat(ty)
    }

    /// The first slot of the field at `slot` of an instance of the class `class`
    /// (`docs/design/records.md` 3.2: records and static arrays among its fields are flat).
    fn obj_slot(&self, class: Ty, slot: u32) -> u32 {
        match class {
            Ty::Class(id) => self.program.types.class_field_offset(id, slot),
            _ => slot,
        }
    }

    /// The value of the root of a place, read without counting. A reference parameter whose value
    /// is laid out flat is read whole into an owning temporary (it may be a lent flat place).
    fn root_borrowed(&mut self, root: &Place) -> Reg {
        match root {
            Place::Var(v, _) if !self.is_ref_param(*v) => self.reg_of(*v),
            Place::Var(v, _) if self.place_ref(*v) => {
                let ty = self.r.vars[v.0 as usize].ty;
                let t = self.alloc(ty);
                self.op2(op::LOAD_REF_REF, t, self.reg_of(*v));
                if !self.program.types.holds_object(ty) {
                    self.owned.push(t);
                }
                t
            }
            Place::Var(v, _) => {
                let t = self.alloc_borrow(self.r.vars[v.0 as usize].ty);
                let r = self.reg_of(*v);
                self.op2(op::LOAD_REF, t, r);
                t
            }
            Place::Global(g) => {
                let t = self.alloc_borrow(self.program.globals[g.0 as usize].ty);
                self.op_table(op::LOAD_GLOBAL, t, Operand::Global(g.0));
                t
            }
            Place::ObjField { base, slot, ty, .. } => {
                let b = self.obj_handle(base, false);
                if self.is_flat(*ty) {
                    let t = self.alloc(*ty);
                    let h = Hop::obj(base.ty, self.obj_slot(base.ty, *slot), *ty);
                    self.hop_load(&h, b, t, true);
                    if !self.program.types.holds_object(*ty) {
                        self.owned.push(t);
                    }
                    return t;
                }
                let t = self.alloc_borrow(*ty);
                self.obj_get(t, b, self.obj_slot(base.ty, *slot), *ty, false);
                t
            }
            _ => unreachable!("the root of a place is a variable, a global or an object's field"),
        }
    }

    /// Whether `base` is a local variable (or `Self`) that evaluating `value` cannot write: it is
    /// not a reference parameter, no nested routine uses it, and `value` passes it to no `var` /
    /// `out` parameter. Its value can then be borrowed across `value` (`docs/design/lifetime.md`
    /// 3.3), without a counted copy that would outlive the statement.
    fn stable_var(&self, base: &Expr, value: &Expr) -> bool {
        base.value.is_none()
            && matches!(&base.kind, ExprKind::Var(v)
                if !self.is_ref_param(*v)
                    && !self.r.vars[v.0 as usize].captured
                    && !expr_writes_var(value, *v))
    }

    /// The handle of the instance whose field is used: borrowed (the variable's register, or a
    /// field read without counting) unless `copy`, when it goes to a temporary first. A handle of
    /// a class managed by hand is not counted (`docs/spec/objects.md` 4.1).
    fn obj_handle(&mut self, base: &Expr, copy: bool) -> Reg {
        if copy {
            let t = self.alloc(base.ty);
            self.expr(base, t);
            t
        } else {
            self.borrowed_value(base)
        }
    }

    /// `ObjGet` / `ObjGetRef` (`docs/spec/objects.md` 14.3): slot `slot` (the first slot of the
    /// field, [`FuncLower::obj_slot`]) of the instance `obj`, counted when `counted` and the
    /// value is managed.
    fn obj_get(&mut self, dst: Reg, obj: Reg, slot: u32, ty: Ty, counted: bool) {
        let rest = vec![Operand::Reg(obj), Operand::Imm(i64::from(slot))];
        self.get_value(ty, counted, (op::OBJ_GET, op::OBJ_GET_REF), dst, rest);
    }

    /// `ObjSet` / `ObjSetRef`: writes `src` to slot `slot` of the instance `obj` (shared, so not
    /// made unique), counted when `counted` and the value is managed (`src` keeps its reference).
    fn obj_set(&mut self, obj: Reg, slot: u32, src: Reg, ty: Ty, counted: bool) {
        let at = Operand::Imm(i64::from(slot));
        self.set_value(
            ty,
            counted,
            (op::OBJ_SET, op::OBJ_SET_REF),
            vec![Operand::Reg(obj), at.clone(), Operand::Reg(src)],
            src,
            (op::OBJ_GET, vec![Operand::Reg(obj), at]),
        );
    }

    /// A slot read: `FieldGet` / `ValGet` / `DynGet`, counted (`…Ref`) when `counted` and the
    /// value is managed.
    fn get_slot(&mut self, dst: Reg, container: Reg, slot: &At, ty: Ty, counted: bool) {
        let (ops, at) = slot_access(slot, true);
        self.get_value(ty, counted, ops, dst, vec![Operand::Reg(container), at]);
    }

    /// A slot write into a unique container (a dynamic array is written shared): `FieldSet` /
    /// `ValSet` / `DynSet`, counted (`…Ref`) when `counted` and the value is managed.
    fn set_slot(&mut self, container: Reg, slot: &At, src: Reg, ty: Ty, counted: bool) {
        let (ops, at) = slot_access(slot, false);
        let (get, _) = slot_access(slot, true);
        self.set_value(
            ty,
            counted,
            ops,
            vec![Operand::Reg(container), at.clone(), Operand::Reg(src)],
            src,
            (get.0, vec![Operand::Reg(container), at]),
        );
    }

    // ---- Hops: the steps of a place grouped by the container holding their slots ----

    /// The type of the root value of a place.
    fn part_root_type(&self, part: &Part<'_>) -> Ty {
        match part.root_reg {
            Some((_, ty)) => ty,
            None => self.place_type(part.root),
        }
    }

    /// The steps of a place grouped into hops (`docs/design/records.md` 3.4): a hop is one slot
    /// (or flat place) of one container, the steps into records and static arrays laid out flat
    /// adding to its address. The first hop's container is the root value (or the instance, for
    /// a flat field of an instance; or the reference slot, `HopKind::Ref`); each later hop's is
    /// the block in the slot of the hop before.
    fn hop_list(&self, part: &Part<'_>) -> Vec<Hop> {
        let types = &self.program.types;
        let mut hops: Vec<Hop> = Vec::new();
        let mut cur = match (part.root, part.handle, part.root_reg) {
            (Place::ObjField { base, slot, ty, .. }, Some(_), None) if self.is_flat(*ty) => {
                Some(Hop::obj(base.ty, self.obj_slot(base.ty, *slot), *ty))
            }
            _ => None,
        };
        let mut ty = self.part_root_type(part);
        for (k, step) in part.steps.iter().enumerate() {
            let inside = cur.is_some() && self.is_flat(ty);
            if !inside {
                hops.extend(cur.take());
                let kind = if ty.is_dyn_indexed() {
                    HopKind::Dyn
                } else if hops.is_empty() && self.ref_root(part).is_some() {
                    HopKind::Ref
                } else {
                    HopKind::Agg
                };
                let elem = types.elem_of(ty);
                let mut h = Hop {
                    kind,
                    ty,
                    elem: None,
                    width: elem.map_or(1, |e| types.slot_width(e)),
                    flat_elem: elem.is_some_and(|e| self.is_flat(e)),
                    off: 0,
                    terms: Vec::new(),
                    start: k,
                    val: ty,
                };
                match &step.slot {
                    SlotRef::Field(i) => h.off = types.field_offset(ty, *i),
                    SlotRef::Elem { index, .. } => h.elem = Some(*index),
                    SlotRef::Dyn(r) => h.elem = Some(Idx::Reg(*r)),
                }
                cur = Some(h);
            } else {
                let h = cur
                    .as_mut()
                    .expect("a step inside a flat place has its hop");
                match &step.slot {
                    SlotRef::Field(i) => h.off += types.field_offset(ty, *i),
                    SlotRef::Elem { index, .. } => {
                        let w = types.elem_of(ty).map_or(1, |e| types.slot_width(e));
                        match index {
                            Idx::Const(c) => h.off += *c as u32 * w,
                            Idx::Reg(r) => h.terms.push((*r, w)),
                        }
                    }
                    SlotRef::Dyn(_) => unreachable!("a dynamic array is not laid out flat"),
                }
            }
            cur.as_mut().expect("a step has its hop").val = step.ty;
            ty = step.ty;
        }
        hops.extend(cur);
        // `RefFieldGet` / `RefFieldSet` reach a slot at a number known when compiling, and
        // never a flat place whole
        let single = hops.len() == 1;
        if let Some(h) = hops.first_mut()
            && h.kind == HopKind::Ref
            && (!h.is_static() || (single && self.is_flat(h.val)))
        {
            h.kind = HopKind::Agg;
        }
        hops
    }

    /// Whether the root of a place is a reference parameter whose value is laid out flat (it may
    /// be a lent flat place) that its first hop cannot reach through the reference slot: the
    /// value is then copied into a register for the access ([`FuncLower::with_ref_copy`]).
    fn needs_ref_copy(&self, part: &Part<'_>, hops: &[Hop]) -> bool {
        part.root_reg.is_none()
            && matches!(part.root, Place::Var(v, _) if self.place_ref(*v))
            && hops.first().is_some_and(|h| h.kind != HopKind::Ref)
    }

    /// Runs `f` on the place with the value of its root, a reference parameter laid out flat,
    /// copied into an owning temporary (`LoadRefRef`; a block of its own for a lent flat place)
    /// and, when `write`, written back whole after (`StoreRefRef`). Nothing runs in between, so
    /// nothing else sees the copy. The copy is let go of at once.
    fn with_ref_copy(
        &mut self,
        part: &Part<'_>,
        write: bool,
        f: impl FnOnce(&mut Self, &Part<'_>),
    ) {
        let ty = self.part_root_type(part);
        let r = self
            .ref_root(part)
            .expect("the root is a reference parameter");
        let t = self.alloc(ty);
        self.op2(op::LOAD_REF_REF, t, r);
        let copy = Part {
            root: part.root,
            handle: None,
            steps: part.steps.clone(),
            root_reg: Some((t, ty)),
        };
        f(self, &copy);
        if write {
            self.op2(op::STORE_REF_REF, r, t);
        }
        self.op1(op::RELEASE, t);
    }

    /// The register of the reference slot the root of a place is read through (a `var` / `out`
    /// parameter, `Self` of a record method, a variable of an enclosing routine, a captured
    /// variable), if it is.
    fn ref_root(&self, part: &Part<'_>) -> Option<Reg> {
        match (part.root, part.root_reg) {
            (Place::Var(v, _), None) if self.is_ref_param(*v) => Some(self.reg_of(*v)),
            _ => None,
        }
    }

    /// The type of the record or static array the reference slot of a `Ref` hop leads to.
    fn ref_type(&self, h: &Hop) -> Ty {
        h.ty
    }

    /// The slot number of the address of a hop into a record, static array or instance: a
    /// constant, or a new temporary with `element × width + offset + Σ index × stride`.
    fn hop_slot(&mut self, h: &Hop) -> At {
        let mut base = h.off;
        let mut terms = Vec::new();
        match h.elem {
            Some(Idx::Const(k)) => base += k as u32 * h.width,
            Some(Idx::Reg(r)) => terms.push((r, h.width)),
            None => {}
        }
        terms.extend(h.terms.iter().copied());
        if terms.is_empty() {
            return At::Field(base);
        }
        let t = self.alloc_borrow(Ty::Int64);
        self.sum_into(t, &terms, base);
        At::Val(t)
    }

    /// `t := Σ index × stride + base` (`t` is written first).
    fn sum_into(&mut self, t: Reg, terms: &[(Reg, u32)], base: u32) {
        let ((r0, s0), rest) = terms.split_first().expect("a sum has a term");
        if *s0 == 1 {
            self.op2(op::MOVE, t, *r0);
        } else {
            self.op_imm(op::MUL_INT_I, t, *r0, i64::from(*s0));
        }
        self.add_terms(t, rest, base);
    }

    /// `t := t + Σ index × stride + base`.
    fn add_terms(&mut self, t: Reg, terms: &[(Reg, u32)], base: u32) {
        for &(r, s) in terms {
            if s == 1 {
                self.op3(op::ADD_INT, t, t, r);
            } else {
                let m = self.alloc_borrow(Ty::Int64);
                self.op_imm(op::MUL_INT_I, m, r, i64::from(s));
                self.op3(op::ADD_INT, t, t, m);
            }
        }
        if base == 0 {
            return;
        }
        if base <= i8::MAX as u32 {
            self.op_imm(op::ADD_INT_I, t, t, i64::from(base));
        } else {
            let k = self.alloc_borrow(Ty::Int64);
            self.load_int(k, i64::from(base));
            self.op3(op::ADD_INT, t, t, k);
        }
    }

    /// A cursor (`docs/spec/bytecode.md` 1.5) at the address of the hop `h` in the container `c`,
    /// and the slot number to add to it in `SlotGet` / `SlotSet` (with `fold`, it is added to the
    /// cursor, and 0 is returned): `ElemAt` for an element of an array whose elements are flat,
    /// otherwise the slot number and the container.
    fn hop_cursor(&mut self, h: &Hop, c: Reg, fold: bool) -> (Reg, u32) {
        let cur = self.alloc_borrow(Ty::Int64);
        let cur1 = self.alloc_borrow(h.ty);
        if h.kind == HopKind::Dyn {
            let Some(Idx::Reg(i)) = h.elem else {
                unreachable!("an element of a dynamic array has its index in a register")
            };
            self.plain(
                op::ELEM_AT,
                vec![
                    Operand::Reg(cur),
                    Operand::Reg(cur1),
                    Operand::Reg(c),
                    Operand::Reg(i),
                ],
            );
            let base = if fold { h.off } else { 0 };
            self.add_terms(cur, &h.terms, base);
            return (cur, if fold { 0 } else { h.off });
        }
        debug_assert!(
            h.kind != HopKind::Ref,
            "a cursor is not made through a reference slot"
        );
        match self.hop_slot(h) {
            At::Field(s) => self.load_int(cur, i64::from(s)),
            At::Val(s) => self.op2(op::MOVE, cur, s),
            At::Dyn(_) => unreachable!("hop_slot gives a slot number"),
        }
        self.op2(op::MOVE, cur1, c);
        (cur, 0)
    }

    /// Reads the slot at the address of `h` in the container `c` into `dst` (counted when
    /// `counted`): a value that is not laid out flat.
    fn hop_get(&mut self, h: &Hop, c: Reg, dst: Reg, counted: bool) {
        let ty = h.val;
        match h.kind {
            HopKind::Dyn if h.flat_elem => {
                let (cur, s) = self.hop_cursor(h, c, false);
                self.cursor_get(dst, cur, s, ty, counted);
            }
            HopKind::Dyn => {
                let Some(Idx::Reg(i)) = h.elem else {
                    unreachable!("an element of a dynamic array has its index in a register")
                };
                self.get_slot(dst, c, &At::Dyn(i), ty, counted);
            }
            HopKind::Ref => {
                let At::Field(s) = self.hop_slot(h) else {
                    unreachable!("a reference slot's hop has a constant address")
                };
                self.ref_field_get(dst, c, s, ty, counted);
            }
            HopKind::Obj => match self.hop_slot(h) {
                At::Field(s) => self.obj_get(dst, c, s, ty, counted),
                At::Val(s) => {
                    let (cur, k) = self.cursor_of(s, c, h.ty);
                    self.cursor_get(dst, cur, k, ty, counted);
                }
                At::Dyn(_) => unreachable!("hop_slot gives a slot number"),
            },
            HopKind::Agg => {
                let at = self.hop_slot(h);
                self.get_slot(dst, c, &at, ty, counted);
            }
        }
    }

    /// Writes `src` to the slot at the address of `h` in the container `c` (opened for writing;
    /// counted when `counted`): a value that is not laid out flat. Through a reference slot,
    /// `RefFieldSet` opens the record or static array itself.
    fn hop_set(&mut self, h: &Hop, c: Reg, src: Reg, counted: bool) {
        let ty = h.val;
        match h.kind {
            HopKind::Dyn if h.flat_elem => {
                let (cur, s) = self.hop_cursor(h, c, false);
                self.cursor_set(cur, s, src, ty, counted);
            }
            HopKind::Dyn => {
                let Some(Idx::Reg(i)) = h.elem else {
                    unreachable!("an element of a dynamic array has its index in a register")
                };
                self.set_slot(c, &At::Dyn(i), src, ty, counted);
            }
            HopKind::Ref => {
                let At::Field(s) = self.hop_slot(h) else {
                    unreachable!("a reference slot's hop has a constant address")
                };
                let rec = self.ref_type(h);
                self.ref_field_set(c, rec, s, src, ty, counted);
            }
            HopKind::Obj => match self.hop_slot(h) {
                At::Field(s) => self.obj_set(c, s, src, ty, counted),
                At::Val(s) => {
                    let (cur, k) = self.cursor_of(s, c, h.ty);
                    self.cursor_set(cur, k, src, ty, counted);
                }
                At::Dyn(_) => unreachable!("hop_slot gives a slot number"),
            },
            HopKind::Agg => {
                let at = self.hop_slot(h);
                self.set_slot(c, &at, src, ty, counted);
            }
        }
    }

    /// A cursor at slot `s` (a register) of the container `c` of type `ty`.
    fn cursor_of(&mut self, s: Reg, c: Reg, ty: Ty) -> (Reg, u32) {
        let cur = self.alloc_borrow(Ty::Int64);
        let cur1 = self.alloc_borrow(ty);
        self.op2(op::MOVE, cur, s);
        self.op2(op::MOVE, cur1, c);
        (cur, 0)
    }

    /// `SlotGet` / `SlotGetRef`: slot `k` from the cursor `cur`.
    fn cursor_get(&mut self, dst: Reg, cur: Reg, k: u32, ty: Ty, counted: bool) {
        let rest = vec![
            Operand::Reg(cur),
            Operand::Reg(cursor_next(cur)),
            Operand::Imm(i64::from(k)),
        ];
        self.get_value(ty, counted, (op::SLOT_GET, op::SLOT_GET_REF), dst, rest);
    }

    /// `SlotSet` / `SlotSetRef`: `src` to slot `k` from the cursor `cur`.
    fn cursor_set(&mut self, cur: Reg, k: u32, src: Reg, ty: Ty, counted: bool) {
        let at = vec![
            Operand::Reg(cur),
            Operand::Reg(cursor_next(cur)),
            Operand::Imm(i64::from(k)),
        ];
        let mut set = at.clone();
        set.push(Operand::Reg(src));
        self.set_value(
            ty,
            counted,
            (op::SLOT_SET, op::SLOT_SET_REF),
            set,
            src,
            (op::SLOT_GET, at),
        );
    }

    /// Reads the value at the address of `h` in the container `c` into `dst` (counted when
    /// `counted`): a flat place whole with `AggLoad` (always counted: it makes a block, so `dst`
    /// owns its value), any other value with [`FuncLower::hop_get`].
    fn hop_load(&mut self, h: &Hop, c: Reg, dst: Reg, counted: bool) {
        if !self.is_flat(h.val) {
            return self.hop_get(h, c, dst, counted);
        }
        let (cur, _) = self.hop_cursor(h, c, true);
        self.plain(
            op::AGG_LOAD,
            vec![
                Operand::Reg(dst),
                Operand::Reg(cur),
                Operand::Reg(cursor_next(cur)),
                Operand::Type(h.val),
            ],
        );
    }

    /// Writes `src` (counted) to the address of `h` in the opened container `c`: a flat place
    /// whole with `AggStore`, any other value with [`FuncLower::hop_set`].
    fn hop_store(&mut self, h: &Hop, c: Reg, src: Reg) {
        if !self.is_flat(h.val) {
            return self.hop_set(h, c, src, true);
        }
        let (cur, _) = self.hop_cursor(h, c, true);
        self.plain(
            op::AGG_STORE,
            vec![
                Operand::Reg(cur),
                Operand::Reg(cursor_next(cur)),
                Operand::Reg(src),
                Operand::Type(h.val),
            ],
        );
    }

    /// The container of the first hop read without counting: the instance, the reference
    /// slot, or the root value.
    fn hop0_borrowed(&mut self, part: &Part<'_>, h: &Hop) -> Reg {
        match h.kind {
            HopKind::Obj => part.handle.expect("an instance's field has its handle"),
            HopKind::Ref => self.ref_root(part).expect("a reference slot's hop"),
            _ => self.part_root_borrowed(part),
        }
    }

    /// The container of hop `k` read without counting from the root.
    fn hops_borrowed(&mut self, part: &Part<'_>, hops: &[Hop], k: usize) -> Reg {
        let mut c = self.hop0_borrowed(part, &hops[0]);
        for h in &hops[..k] {
            let t = self.alloc_borrow(h.val);
            self.hop_get(h, c, t, false);
            c = t;
        }
        c
    }

    /// Reads the value of a field or element place into `dst` (counted).
    fn part_load(&mut self, part: &Part<'_>, dst: Reg) {
        if part.steps.is_empty() && part.root_reg.is_none() {
            return self.load_place(part.root, dst);
        }
        self.part_access(part, false, |this, h, c| this.hop_load(h, c, dst, true));
    }

    /// Runs `f` on the last hop of a field or element place and its container: read without
    /// counting, or opened for writing when `write` ([`FuncLower::open_hops`]). A root that is a
    /// reference parameter laid out flat is copied first ([`FuncLower::with_ref_copy`]).
    fn part_access(&mut self, part: &Part<'_>, write: bool, f: impl FnOnce(&mut Self, &Hop, Reg)) {
        let hops = self.hop_list(part);
        if self.needs_ref_copy(part, &hops) {
            return self.with_ref_copy(part, write, |this, p| this.part_access(p, write, f));
        }
        let n = hops.len() - 1;
        let c = if write {
            self.open_hops(part, &hops).inner
        } else {
            self.hops_borrowed(part, &hops, n)
        };
        f(self, &hops[n], c);
    }

    /// A field read through the reference slot `r` (`RefFieldGet`, counted (`RefFieldGetRef`)
    /// when `counted` and the value is managed; `docs/spec/bytecode.md` 2.8): slot `slot` of the
    /// record or static array.
    fn ref_field_get(&mut self, dst: Reg, r: Reg, slot: u32, ty: Ty, counted: bool) {
        let ops = (op::REF_FIELD_GET, op::REF_FIELD_GET_REF);
        let rest = vec![Operand::Reg(r), Operand::Imm(i64::from(slot))];
        self.get_value(ty, counted, ops, dst, rest);
    }

    /// A field write through the reference slot `r` to a record or static array of type `rec`,
    /// which `RefFieldSet` makes unique first (counted (`RefFieldSetRef`) when `counted` and the
    /// value is managed; `src` keeps its reference).
    fn ref_field_set(&mut self, r: Reg, rec: Ty, slot: u32, src: Reg, ty: Ty, counted: bool) {
        let at = Operand::Imm(i64::from(slot));
        self.set_value(
            ty,
            counted,
            (op::REF_FIELD_SET, op::REF_FIELD_SET_REF),
            vec![
                Operand::Reg(r),
                at.clone(),
                Operand::Reg(src),
                Operand::Type(rec),
            ],
            src,
            (op::REF_FIELD_GET, vec![Operand::Reg(r), at]),
        );
    }

    /// Changes slot `slot` of the record or static array the reference slot `r` leads to in
    /// place: read without counting, written back without counting first to make the record
    /// unique (`RefFieldSet`; a lent flat place is written in place), so that `f` changes a
    /// value only the record holds, then written back again.
    fn modify_ref_slot(
        &mut self,
        r: Reg,
        rec: Ty,
        slot: u32,
        ty: Ty,
        f: impl FnOnce(&mut Self, Reg),
    ) -> Reg {
        let t = self.alloc_borrow(ty);
        self.ref_field_get(t, r, slot, ty, false);
        self.ref_field_set(r, rec, slot, t, ty, false);
        f(self, t);
        self.ref_field_set(r, rec, slot, t, ty, false);
        t
    }

    /// The root of a place made unique for writing (`Unique`; `docs/design/lifetime.md` 3.6). A
    /// global or a reference parameter is read without counting, made unique and written back at
    /// once, so the slot owns the copy.
    fn open_root(&mut self, root: &Place) -> Reg {
        let ty = self.place_type(root);
        self.modify_root(root, |this, t| {
            this.op_table(op::UNIQUE, t, Operand::Type(ty))
        })
    }

    /// The value of the root of a field or element place, read without counting: the copy of a
    /// reference parameter's value, an instance's field through the handle `eval_part` took
    /// first, any other root as `root_borrowed`.
    fn part_root_borrowed(&mut self, part: &Part<'_>) -> Reg {
        if let Some((t, _)) = part.root_reg {
            return t;
        }
        match (part.root, part.handle) {
            (Place::ObjField { base, slot, ty, .. }, Some(h)) => {
                let t = self.alloc_borrow(*ty);
                self.obj_get(t, h, self.obj_slot(base.ty, *slot), *ty, false);
                t
            }
            _ => self.root_borrowed(part.root),
        }
    }

    /// The root of a field or element place made unique for writing (`open_root`), an
    /// instance's field through the handle `eval_part` took first.
    fn part_open_root(&mut self, part: &Part<'_>) -> Reg {
        if let Some((t, ty)) = part.root_reg {
            self.op_table(op::UNIQUE, t, Operand::Type(ty));
            return t;
        }
        match (part.root, part.handle) {
            (Place::ObjField { base, slot, ty, .. }, Some(h)) => {
                let s = self.obj_slot(base.ty, *slot);
                let t = self.alloc_borrow(*ty);
                self.obj_get(t, h, s, *ty, false);
                self.op_table(op::UNIQUE, t, Operand::Type(*ty));
                self.obj_set(h, s, t, *ty, false);
                t
            }
            _ => self.open_root(part.root),
        }
    }

    /// Changes the value of a variable or global in place: `f` gets a register with the value read
    /// without counting, may replace it (as `Unique` and `SetLen` do: the old reference is let go
    /// only if a new value takes its place), and the register is written back without counting.
    /// Nothing else runs in between, so the counts stay right. A value laid out flat (an
    /// instance's field, or what a reference parameter may lend) is copied whole into an owning
    /// temporary and written back whole (`f` then gets the copy, and the register returned no
    /// longer holds it).
    fn modify_root(&mut self, root: &Place, f: impl FnOnce(&mut Self, Reg)) -> Reg {
        let ty = self.place_type(root);
        match root {
            Place::Var(v, _) if !self.is_ref_param(*v) => {
                let r = self.reg_of(*v);
                f(self, r);
                r
            }
            Place::Var(v, _) if self.place_ref(*v) => {
                let r = self.reg_of(*v);
                let t = self.alloc(ty);
                self.op2(op::LOAD_REF_REF, t, r);
                f(self, t);
                self.op2(op::STORE_REF_REF, r, t);
                self.op1(op::RELEASE, t);
                t
            }
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                let t = self.alloc_borrow(ty);
                self.op2(op::LOAD_REF, t, r);
                f(self, t);
                self.op2(op::STORE_REF, r, t);
                t
            }
            Place::Global(g) => {
                let t = self.alloc_borrow(ty);
                self.op_table(op::LOAD_GLOBAL, t, Operand::Global(g.0));
                f(self, t);
                self.op_table(op::STORE_GLOBAL, t, Operand::Global(g.0));
                t
            }
            Place::ObjField { base, slot, .. } => {
                // The instance's slot, like a global's
                let b = self.obj_handle(base, false);
                let s = self.obj_slot(base.ty, *slot);
                if self.is_flat(ty) {
                    let h = Hop::obj(base.ty, s, ty);
                    let t = self.alloc(ty);
                    self.hop_load(&h, b, t, true);
                    f(self, t);
                    self.hop_store(&h, b, t);
                    self.op1(op::RELEASE, t);
                    return t;
                }
                let t = self.alloc_borrow(ty);
                self.obj_get(t, b, s, ty, false);
                f(self, t);
                self.obj_set(b, s, t, ty, false);
                t
            }
            _ => unreachable!("the root of a place is a variable, a global or an object's field"),
        }
    }

    /// Opens the containers of a place for writing, from the root down (`docs/design/lifetime.md`
    /// 3.6, `docs/spec/types.md` chapter 3): records and static arrays are made unique and written
    /// back, while a dynamic array is written shared, so the containers before the innermost
    /// dynamic array are only read; an instance is shared, and a record or static array reached
    /// through a reference slot is opened by `RefFieldSet`. Returns the container of the last hop,
    /// and the number and container of the hop into the innermost dynamic array.
    fn open_hops(&mut self, part: &Part<'_>, hops: &[Hop]) -> Opened {
        let last_dyn = hops.iter().rposition(|h| h.kind == HopKind::Dyn);
        let (mut c, from, dyn_root) = match last_dyn {
            None => {
                let c = match hops[0].kind {
                    HopKind::Obj | HopKind::Ref => self.hop0_borrowed(part, &hops[0]),
                    _ => self.part_open_root(part),
                };
                (c, 0, None)
            }
            Some(0)
                if matches!(self.part_root_type(part), Ty::OpenArray(_))
                    && part.root_reg.is_none() =>
            {
                (self.open_open_array(part.root), 0, None)
            }
            Some(k) => {
                // The dynamic array is read without counting along the way
                let c = self.hops_borrowed(part, hops, k);
                (c, k, Some((k, c)))
            }
        };
        for h in &hops[from..hops.len() - 1] {
            let ty = h.val;
            c = if h.kind == HopKind::Ref {
                let At::Field(s) = self.hop_slot(h) else {
                    unreachable!("a reference slot's hop has a constant address")
                };
                let rec = self.ref_type(h);
                self.modify_ref_slot(c, rec, s, ty, |this, t| {
                    this.op_table(op::UNIQUE, t, Operand::Type(ty))
                })
            } else {
                let t = self.alloc_borrow(ty);
                self.hop_get(h, c, t, false);
                self.op_table(op::UNIQUE, t, Operand::Type(ty));
                // The container's slot takes over the reference of the copy (if one was made)
                self.hop_set(h, c, t, false);
                t
            };
        }
        Opened { inner: c, dyn_root }
    }

    /// An open array parameter opened for writing its elements (`docs/spec/types.md` 2.4): a value
    /// parameter is made unique as a dynamic array (`Unique` keeps a static array block a static
    /// array), a `var` parameter only when it holds a static array (`UniqueVal`; a dynamic array
    /// is shared), written back to the caller's variable.
    fn open_open_array(&mut self, root: &Place) -> Reg {
        let ty = self.place_type(root);
        match root {
            Place::Var(v, _) if !self.is_ref_param(*v) => {
                let r = self.reg_of(*v);
                let dyn_ty = match ty {
                    Ty::OpenArray(id) => Ty::DynArray(id),
                    t => t,
                };
                self.op_table(op::UNIQUE, r, Operand::Type(dyn_ty));
                r
            }
            // A value parameter of an enclosing routine is made unique as its own value is
            // (`docs/spec/semantics.md` 4.9)
            Place::Var(v, _) if self.origin(*v).kind == VarKind::Param(ParamMode::Value) => {
                let dyn_ty = match ty {
                    Ty::OpenArray(id) => Ty::DynArray(id),
                    t => t,
                };
                self.modify_root(root, |this, t| {
                    this.op_table(op::UNIQUE, t, Operand::Type(dyn_ty))
                })
            }
            _ => self.modify_root(root, |this, t| this.op1(op::UNIQUE_VAL, t)),
        }
    }

    /// Writes `src` to a field or element place (counted; `src` keeps its reference).
    fn part_store(&mut self, part: &Part<'_>, src: Reg) {
        if part.steps.is_empty() && part.root_reg.is_none() {
            return self.store_place(part.root, src);
        }
        self.part_access(part, true, |this, h, c| this.hop_store(h, c, src));
    }

    /// Changes the value of a field or element place in place: the slot of its opened container
    /// (like `modify_root` for a variable or global). A flat place is copied whole into an
    /// owning temporary and written back whole.
    fn modify_part(&mut self, part: &Part<'_>, f: impl FnOnce(&mut Self, Reg)) {
        let hops = self.hop_list(part);
        if self.needs_ref_copy(part, &hops) {
            return self.with_ref_copy(part, true, |this, p| this.modify_part(p, f));
        }
        let c = self.open_hops(part, &hops).inner;
        let last = hops.last().expect("a place has a hop");
        let ty = last.val;
        if self.is_flat(ty) {
            let t = self.alloc(ty);
            self.hop_load(last, c, t, true);
            f(self, t);
            self.hop_store(last, c, t);
            self.op1(op::RELEASE, t);
        } else if last.kind == HopKind::Ref {
            let At::Field(s) = self.hop_slot(last) else {
                unreachable!("a reference slot's hop has a constant address")
            };
            let rec = self.ref_type(last);
            self.modify_ref_slot(c, rec, s, ty, f);
        } else {
            let t = self.alloc_borrow(ty);
            self.hop_get(last, c, t, false);
            f(self, t);
            self.hop_set(last, c, t, false);
        }
    }

    // ---- Reading fields and elements in expressions ----

    /// Reads a field or element expression (or an instance's field) into `dst`, counted when
    /// `counted` (a flat value is always counted, into a register that owns it): the root
    /// first, then each hop's indices (a static array's checked at once) followed by the read of
    /// the slot holding the next hop's container. A container is read with counting when the
    /// indices after it may run code (a call), so the code cannot free it.
    fn read_chain(&mut self, e: &Expr, dst: Reg, counted: bool) {
        self.read_chain_with(e, |this, h, c| this.hop_load(h, c, dst, counted));
    }

    /// [`FuncLower::read_chain`] with `f` reading the last hop from its container.
    fn read_chain_with(&mut self, e: &Expr, f: impl FnOnce(&mut Self, &Hop, Reg)) {
        let mut f = Some(f);
        let mut steps: Vec<&Expr> = Vec::new();
        let mut x = e;
        while x.value.is_none() {
            match &x.kind {
                ExprKind::Field { base, .. } | ExprKind::Elem { base, .. } => {
                    steps.push(x);
                    x = base;
                }
                _ => break,
            }
        }
        steps.reverse();
        let root = x;
        let types = &self.program.types;
        // The hops, by the types (as `hop_list`): the steps of each, and its kind
        let mut groups: Vec<(HopKind, Ty, Vec<&Expr>)> = Vec::new();
        let obj_root = match &root.kind {
            ExprKind::ObjField { base, slot, .. }
                if root.value.is_none() && self.is_flat(root.ty) =>
            {
                Some((base, *slot))
            }
            _ => None,
        };
        if let Some((base, _)) = obj_root {
            groups.push((HopKind::Obj, base.ty, Vec::new()));
        }
        let mut ty = root.ty;
        for s in &steps {
            let inside = !groups.is_empty() && self.is_flat(ty);
            if !inside {
                let kind = if ty.is_dyn_indexed() {
                    HopKind::Dyn
                } else {
                    HopKind::Agg
                };
                groups.push((kind, ty, Vec::new()));
            }
            groups.last_mut().expect("a hop").2.push(s);
            ty = s.ty;
        }
        let ref_var = match &root.kind {
            ExprKind::Var(v) if root.value.is_none() && self.is_ref_param(*v) => Some(*v),
            _ => None,
        };
        // Through a reference slot, the first hop's slot is read with `RefFieldGet` when its
        // address is constant and it is not a flat place whole
        let first_static = groups.first().is_some_and(|(k, _, g)| {
            *k == HopKind::Agg
                && g.iter().all(|s| match &s.kind {
                    ExprKind::Elem { index, .. } => index.value.is_some(),
                    _ => true,
                })
        });
        let use_ref =
            ref_var.is_some() && first_static && !(groups.len() == 1 && self.is_flat(e.ty));
        let has_call = |g: &[&Expr]| {
            g.iter()
                .any(|s| matches!(&s.kind, ExprKind::Elem { index, .. } if expr_has_call(index)))
        };
        let calls_after = |k: usize| groups[k..].iter().any(|(_, _, g)| has_call(g));
        let mut c = if let Some((base, _)) = obj_root {
            self.obj_handle(base, calls_after(0))
        } else if use_ref {
            self.reg_of(ref_var.expect("a reference parameter"))
        } else if groups.is_empty() {
            unreachable!("read_chain reads a field, an element or an instance's field")
        } else if calls_after(0) {
            let t = self.alloc(root.ty);
            self.expr(root, t);
            t
        } else {
            self.borrowed_value(root)
        };
        let n = groups.len();
        for (k, (kind, cty, g)) in groups.iter().enumerate() {
            let elem = types.elem_of(*cty);
            let mut h = Hop {
                kind: if k == 0 && use_ref {
                    HopKind::Ref
                } else {
                    *kind
                },
                ty: *cty,
                elem: None,
                width: elem.map_or(1, |e| types.slot_width(e)),
                flat_elem: elem.is_some_and(|e| self.is_flat(e)),
                off: 0,
                terms: Vec::new(),
                start: 0,
                val: *cty,
            };
            let mut sty = *cty;
            if let Some((base, slot)) = obj_root.filter(|_| k == 0) {
                h = Hop::obj(base.ty, self.obj_slot(base.ty, slot), root.ty);
                sty = root.ty;
            }
            for (j, s) in g.iter().enumerate() {
                match &s.kind {
                    ExprKind::Field { index, .. } => h.off += types.field_offset(sty, *index),
                    ExprKind::Elem { index, .. } if sty.is_dyn_indexed() => {
                        let r = self.alloc_borrow(Ty::Int64);
                        self.expr(index, r);
                        h.elem = Some(Idx::Reg(r));
                        // A later index of the hop may run code: this index is checked first,
                        // as a read checks it right after it is evaluated
                        if has_call(&g[j + 1..]) {
                            let probe = Hop {
                                terms: Vec::new(),
                                ..h.clone()
                            };
                            self.hop_cursor(&probe, c, true);
                        }
                    }
                    ExprKind::Elem { index, .. } => {
                        let (i, check) = self.elem_idx(sty, index, s.span);
                        if let (Idx::Reg(r), Some(chk)) = (i, &check) {
                            self.chk_index(r, chk);
                        }
                        let w = types.elem_of(sty).map_or(1, |e| types.slot_width(e));
                        if j == 0 && h.kind != HopKind::Obj {
                            h.elem = Some(i);
                        } else {
                            match i {
                                Idx::Const(v) => h.off += v as u32 * w,
                                Idx::Reg(r) => h.terms.push((r, w)),
                            }
                        }
                    }
                    _ => unreachable!("a step is a field or an element"),
                }
                sty = s.ty;
            }
            h.val = sty;
            if k + 1 == n {
                let f = f.take().expect("the last hop is read once");
                f(self, &h, c);
            } else {
                let t = if calls_after(k + 1) {
                    let t = self.alloc(sty);
                    self.hop_get(&h, c, t, true);
                    t
                } else {
                    let t = self.alloc_borrow(sty);
                    self.hop_get(&h, c, t, false);
                    t
                };
                c = t;
            }
        }
    }

    /// A register with the value of an expression that is only read right away: a variable's
    /// register, or a container read without counting (`docs/design/lifetime.md` 3.6). Anything
    /// that makes a value goes to an owning temporary, a flat place read whole among them.
    fn borrowed_value(&mut self, e: &Expr) -> Reg {
        if e.value.is_none() {
            match &e.kind {
                ExprKind::Var(v) => {
                    return self.root_borrowed(&Place::Var(*v, e.span));
                }
                ExprKind::Global(g) => return self.root_borrowed(&Place::Global(*g)),
                ExprKind::Field { .. } | ExprKind::Elem { .. } | ExprKind::ObjField { .. }
                    if !self.is_flat(e.ty) =>
                {
                    let t = self.alloc_borrow(e.ty);
                    self.read_field(e, t, false);
                    return t;
                }
                ExprKind::Field { .. } | ExprKind::Elem { .. } | ExprKind::ObjField { .. } => {
                    let t = self.alloc(e.ty);
                    self.read_field(e, t, true);
                    if !self.program.types.holds_object(e.ty) {
                        self.owned.push(t);
                    }
                    return t;
                }
                _ => {}
            }
        }
        let t = self.alloc(e.ty);
        self.expr(e, t);
        t
    }

    /// Reads a field, element or instance's field expression into `dst` (counted when
    /// `counted`; a flat value always is).
    fn read_field(&mut self, e: &Expr, dst: Reg, counted: bool) {
        match &e.kind {
            ExprKind::ObjField { base, slot, .. } if !self.is_flat(e.ty) => {
                let b = self.borrowed_value(base);
                let s = self.obj_slot(base.ty, *slot);
                self.obj_get(dst, b, s, e.ty, counted);
            }
            ExprKind::ObjField { .. } => {
                self.read_field_with(e, |this, h, c| this.hop_load(h, c, dst, true))
            }
            _ => self.read_chain(e, dst, counted),
        }
    }

    /// Runs `f` on the last hop of a field, element or instance's field expression and its
    /// container read without counting ([`FuncLower::read_chain_with`]).
    fn read_field_with(&mut self, e: &Expr, f: impl FnOnce(&mut Self, &Hop, Reg)) {
        match &e.kind {
            ExprKind::ObjField { base, slot, .. } => {
                let b = self.borrowed_value(base);
                let h = Hop::obj(base.ty, self.obj_slot(base.ty, *slot), e.ty);
                f(self, &h, b);
            }
            _ => self.read_chain_with(e, f),
        }
    }

    fn for_stmt(
        &mut self,
        var: ForVar,
        counter: Option<VarId>,
        from: &Expr,
        to: &Expr,
        downto: bool,
        body: &Stmt,
    ) {
        // A captured loop variable is counted in a hidden register and written to its cell on
        // each entry to the body (a `for var` one to a new cell; `docs/spec/objects.md` 9.6)
        let cell = match var {
            ForVar::Local(v) if self.is_ref_param(v) => Some(v),
            _ => None,
        };
        let (counter, var_ty, global): (Reg, Ty, Option<GlobalId>) = match var {
            ForVar::Local(v) if cell.is_some() => (
                self.alloc_borrow(Ty::Int64),
                self.r.vars[v.0 as usize].ty,
                None,
            ),
            ForVar::Local(v) => (self.reg_of(v), self.r.vars[v.0 as usize].ty, None),
            ForVar::Global(id) => (
                self.reg_of(counter.expect("a global loop variable has a counter")),
                self.program.globals[id.0 as usize].ty,
                Some(id),
            ),
        };
        self.expr_into_var(from, counter);
        let limit = self.alloc(to.ty);
        self.expr(to, limit);
        let unsigned = self.program.types.base(var_ty) == Ty::UInt64;
        let end = self.new_block();
        let cont = self.new_block();
        let top = self.new_block();
        // Skip if the range is empty: limit < counter for `to`, counter < limit for `downto`
        let lt = if unsigned {
            op::JMP_IF_LT_UINT
        } else {
            op::JMP_IF_LT_INT
        };
        if downto {
            self.branch(lt, counter, Some(limit), end);
        } else {
            self.branch(lt, limit, Some(counter), end);
        }
        // When a global variable is the loop variable, write that iteration's value on each entry to the body,
        // and on exit (`Break` and the termination test) write back the value of the last entry
        // (even if a routine called from the body changes it, the value after the loop is the value of the last entry;
        // semantics.md 4.5). If the body is never entered, the original value is kept
        let brk = if global.is_some() {
            self.new_block()
        } else {
            end
        };
        self.bind(top);
        if let Some(g) = global {
            self.op_table(op::STORE_GLOBAL, counter, Operand::Global(g.0));
        }
        if let Some(v) = cell {
            self.renew_cell(v);
            self.op2(op::STORE_REF, self.reg_of(v), counter);
        }
        self.loops.push((brk, cont, self.cleanup.len()));
        self.stmt(body);
        self.loops.pop();
        self.bind(cont);
        self.branch(op::JMP_IF_EQ_INT, counter, Some(limit), brk);
        let delta = if downto { -1 } else { 1 };
        self.op_imm(op::ADD_INT_I, counter, counter, delta);
        self.jump(top);
        if let Some(g) = global {
            self.bind(brk);
            self.op_table(op::STORE_GLOBAL, counter, Operand::Global(g.0));
        }
        self.bind(end);
    }

    /// `for X in C` (`docs/spec/types.md` chapter 5): the collection's reference is held in a
    /// hidden variable for the whole loop, so the number of iterations is fixed on entry.
    fn for_in(&mut self, f: &ForIn, coll: &Expr, body: &Stmt) {
        if f.coll_ty.is_set() {
            return self.for_in_set(f, coll, body);
        }
        let c = self.reg_of(f.coll_var);
        let i = self.reg_of(f.idx_var);
        self.expr_into_var(coll, c);
        let len = self.alloc_borrow(Ty::Int64);
        let first = if f.coll_ty == Ty::String {
            self.op2(op::STR_LEN, len, c);
            1
        } else if f.coll_ty.is_dyn_indexed() {
            self.op2(op::ARR_LEN, len, c);
            0
        } else {
            let (_, lo, hi, _) = self
                .program
                .types
                .array(f.coll_ty)
                .expect("for..in iterates over an array or a string");
            self.load_int(len, (hi - lo + 1) as i64);
            0
        };
        self.load_int(i, first);
        let global = match f.var {
            ForVar::Global(g) => Some(g),
            ForVar::Local(_) => None,
        };
        let end = self.new_block();
        let cont = self.new_block();
        let top = self.new_block();
        // A global loop variable gets the element of the last entry again on exit (as `for..to`;
        // `docs/spec/semantics.md` 4.5): `done` steps back to it, `brk` stores it
        let (done, brk) = if global.is_some() {
            (self.new_block(), self.new_block())
        } else {
            (end, end)
        };
        self.bind(top);
        // Positions run first..len for a string (1-based) and 0..len-1 for an array
        let past_end = if first == 1 {
            op::JMP_IF_LT_INT
        } else {
            op::JMP_IF_LE_INT
        };
        self.branch(past_end, len, Some(i), done);
        self.load_loop_elem(f);
        self.loops.push((brk, cont, self.cleanup.len()));
        self.stmt(body);
        self.loops.pop();
        self.bind(cont);
        self.op_imm(op::ADD_INT_I, i, i, 1);
        self.jump(top);
        if global.is_some() {
            self.bind(done);
            let k = self.alloc_borrow(Ty::Int64);
            self.load_int(k, first);
            self.branch(op::JMP_IF_EQ_INT, i, Some(k), end);
            self.op_imm(op::ADD_INT_I, i, i, -1);
            self.bind(brk);
            self.load_loop_elem(f);
        }
        self.bind(end);
        // The collection is let go of once the loop is done (`Break` comes here too)
        if self.released_early(f.coll_ty) {
            self.emit_releases(&[c]);
        }
    }

    /// `for X in C` with `GetEnumerator` (`docs/spec/objects.md` 13.2, 14.10): the collection and
    /// the enumerator in hidden variables, then the loop in the body of a `try` whose `finally`
    /// block lets go of the enumerator.
    fn for_in_enum(
        &mut self,
        var: ForVar,
        coll_var: VarId,
        enum_var: VarId,
        coll: &Expr,
        en: &Enumerator,
        body: &Stmt,
    ) {
        let c = self.reg_of(coll_var);
        let e = self.reg_of(enum_var);
        self.expr_into_var(coll, c);
        self.expr_into_var(&en.get, e);
        let global = match var {
            ForVar::Global(g) => Some(g),
            ForVar::Local(_) => None,
        };
        // The element given last starts as the global's own value, so that the loop gives it
        // back unchanged if the body is never entered
        let last = en.last.map(|l| {
            let r = self.reg_of(l);
            let g = global.expect("only a global loop variable has `last`");
            let ty = self.program.globals[g.0 as usize].ty;
            let value = Expr {
                kind: ExprKind::Global(g),
                ty,
                value: None,
                span: coll.span,
            };
            self.expr_into_var(&value, r);
            (l, r)
        });
        let scope = self.finally_begin();
        let depth = self.cleanup.len();
        self.cleanup.push(Cleanup::Finally {
            state: scope.state,
            fin: scope.fin,
            conts: Vec::new(),
        });
        let top = self.new_block();
        let cont = self.new_block();
        let end = self.new_block();
        self.bind(top);
        self.cond(&en.move_next, end, false);
        // `X := E.Current`; as an initial value, it gives a captured `for var X` a new cell
        // (`objects.md` 9.6)
        let target = match (var, last) {
            (_, Some((l, _))) => Place::Var(l, TextRange::default()),
            (ForVar::Local(v), None) => Place::Var(v, TextRange::default()),
            (ForVar::Global(_), None) => {
                unreachable!("sema gives a global loop variable `last`")
            }
        };
        self.stmt(&Stmt {
            kind: StmtKind::Assign {
                target,
                value: en.current.clone(),
                check: en.checks,
                is_init: true,
            },
            span: en.current.span,
        });
        if let (Some(g), Some((_, r))) = (global, last) {
            self.store_place(&Place::Global(g), r);
        }
        self.loops.push((end, cont, self.cleanup.len()));
        self.stmt(body);
        self.loops.pop();
        self.bind(cont);
        self.jump(top);
        self.bind(end);
        if let (Some(g), Some((_, r))) = (global, last) {
            self.store_place(&Place::Global(g), r);
        }
        let Some(Cleanup::Finally { conts, .. }) = self.cleanup.pop() else {
            unreachable!("pushed above")
        };
        let handler_first = self.finally_handler(&scope);
        self.enum_cleanup(en, e);
        self.finally_end(&scope, handler_first, conts.iter().map(|c| c.0).collect());
        for (cont, leave) in conts {
            self.bind(cont);
            self.leave_from(depth, leave);
        }
        self.bind(scope.after);
    }

    /// The `finally` block of `for..in` with `GetEnumerator` (`docs/spec/objects.md` 13.2, 14.10):
    /// `Free` of an enumerator of a class managed by hand, letting go of a reference counted one
    /// or a record, and by its contents for an interface or `TObject`.
    fn enum_cleanup(&mut self, en: &Enumerator, e: Reg) {
        let ty = self
            .reg_type(e)
            .expect("the enumerator's hidden variable has a type");
        match en.cleanup {
            EnumCleanup::Free => self.intrinsic(op::intr::FREE, &[e]),
            EnumCleanup::Release => self.clear_value(ty, e),
            EnumCleanup::ByContents => {
                let m = self.mark();
                let t = self.alloc_borrow(Ty::Int64);
                let is_ref = self.alloc_borrow(Ty::Boolean);
                self.op2(op::MOVE, t, e);
                self.op2(op::MOVE, is_ref, e);
                self.op_table(
                    op::IS_CLASS,
                    is_ref,
                    Operand::Type(Ty::Class(crate::sema::types::TREFOBJECT)),
                );
                let manual = self.new_block();
                let done = self.new_block();
                self.branch(op::JMP_IF_FALSE, is_ref, None, manual);
                self.clear_value(ty, e);
                self.jump(done);
                self.bind(manual);
                // The variable does not own an instance managed by hand
                self.op1(op::LOAD_ZERO, e);
                self.intrinsic(op::intr::FREE, &[t]);
                self.bind(done);
                self.release(m);
            }
        }
    }

    /// `for X in S` over a set (`docs/spec/types.md` 8.3): the position holds the last element
    /// (-1 before the first), and `SetNext` finds the next one in ascending order.
    fn for_in_set(&mut self, f: &ForIn, coll: &Expr, body: &Stmt) {
        let c = self.reg_of(f.coll_var);
        let i = self.reg_of(f.idx_var);
        self.expr_into_var(coll, c);
        self.load_int(i, -1);
        let next = self.alloc_borrow(Ty::Int64);
        let global = matches!(f.var, ForVar::Global(_));
        let end = self.new_block();
        let cont = self.new_block();
        let top = self.new_block();
        let (done, brk) = if global {
            (self.new_block(), self.new_block())
        } else {
            (end, end)
        };
        self.bind(top);
        self.op_imm(op::ADD_INT_I, next, i, 1);
        let opc = if let Ty::BigSet(_) = f.coll_ty {
            op::SET_NEXT_BIG
        } else {
            op::SET_NEXT
        };
        self.op3(opc, next, c, next);
        let zero = self.alloc_borrow(Ty::Int64);
        self.op1(op::LOAD_ZERO, zero);
        self.branch(op::JMP_IF_LT_INT, next, Some(zero), done);
        self.op2(op::MOVE, i, next);
        self.load_loop_elem(f);
        self.loops.push((brk, cont, self.cleanup.len()));
        self.stmt(body);
        self.loops.pop();
        self.bind(cont);
        self.jump(top);
        if global {
            // The loop variable gets the last element again (nothing if there was none)
            self.bind(done);
            self.branch(op::JMP_IF_LT_INT, i, Some(zero), end);
            self.bind(brk);
            self.load_loop_elem(f);
        }
        self.bind(end);
    }

    /// Reads the element at the position of a `for..in` into its loop variable.
    fn load_loop_elem(&mut self, f: &ForIn) {
        let mark = self.mark();
        let (c, i) = (self.reg_of(f.coll_var), self.reg_of(f.idx_var));
        // A captured loop variable gets the element through its cell (a `for var` one in a new
        // cell; `docs/spec/objects.md` 9.6)
        let cell = match f.var {
            ForVar::Local(v) if self.is_ref_param(v) => Some(v),
            _ => None,
        };
        let dst = match f.var {
            ForVar::Local(v) if cell.is_some() => self.alloc(self.r.vars[v.0 as usize].ty),
            ForVar::Local(v) => self.reg_of(v),
            ForVar::Global(g) => self.alloc(self.program.globals[g.0 as usize].ty),
        };
        // The element goes straight to the variable unless it is converted to a string
        let raw = match f.conv {
            Some(Conversion::CharToStr) => self.alloc_borrow(f.elem),
            _ => dst,
        };
        if f.coll_ty.is_set() {
            // The position is the element
            self.op2(op::MOVE, raw, i);
        } else if f.coll_ty == Ty::String {
            self.op3(op::STR_INDEX, raw, c, i);
        } else {
            let types = &self.program.types;
            let h = Hop {
                kind: if f.coll_ty.is_dyn_indexed() {
                    HopKind::Dyn
                } else {
                    HopKind::Agg
                },
                ty: f.coll_ty,
                elem: Some(Idx::Reg(i)),
                width: types.slot_width(f.elem),
                flat_elem: self.is_flat(f.elem),
                off: 0,
                terms: Vec::new(),
                start: 0,
                val: f.elem,
            };
            // A `[weak]` loop variable does not count the element (`docs/spec/objects.md` 4.4)
            let counted = !matches!(self.reg_type(raw), Some(Ty::Weak(_)));
            self.hop_load(&h, c, raw, counted);
        }
        if let Some(kind) = f.conv {
            self.convert(kind, f.elem, dst, raw);
        }
        if let ForVar::Global(g) = f.var {
            self.store_place(&Place::Global(g), dst);
        }
        if let Some(v) = cell {
            self.renew_cell(v);
            self.store_place(&Place::Var(v, TextRange::default()), dst);
        }
        self.release(mark);
    }

    /// `Include` / `Exclude` (`docs/spec/types.md` 8.3): the element, then the set changed in
    /// place (a big set made unique first).
    fn set_elem(&mut self, target: &Place, elem: &Expr, exclude: bool, check: bool) {
        let ty = self.place_type(target);
        let x = self.expr_any(elem);
        if check {
            self.set_elem_check(x, ty);
        }
        let direct = self.direct_var_reg(target);
        let part = is_part(target).then(|| {
            let part = self.eval_part(target);
            self.part_check(&part);
            part
        });
        let t = match (direct, &part) {
            (Some(r), _) => r,
            (None, Some(part)) => {
                let t = self.alloc(ty);
                self.part_load(part, t);
                t
            }
            (None, None) => {
                let t = self.alloc(ty);
                self.load_place(target, t);
                t
            }
        };
        if let Ty::BigSet(_) = ty {
            self.op_table(op::UNIQUE, t, Operand::Type(ty));
            let opc = if exclude { op::SET_EXCL } else { op::SET_INCL };
            self.op3(opc, t, x, x);
        } else {
            let m = self.alloc_borrow(Ty::Int64);
            self.op3(op::SET_RANGE, m, x, x);
            if exclude {
                self.op2(op::NOT_INT, m, m);
                self.op3(op::AND_INT, t, t, m);
            } else {
                self.op3(op::OR_INT, t, t, m);
            }
        }
        match (direct, &part) {
            (Some(_), _) => {}
            (None, Some(part)) => self.part_store(part, t),
            (None, None) => self.store_place(target, t),
        }
    }

    /// `ChkRange` of an element against the range of a set's base type, with the set's name in
    /// the message (`docs/spec/types.md` 8.3).
    fn set_elem_check(&mut self, reg: Reg, set: Ty) {
        let types = &self.program.types;
        let (lo, hi) = types.set_range(set).expect("a set has a range");
        let tail: Vec<u16> = types.set_range_tail(set).encode_utf16().collect();
        self.range_check_with(reg, lo, hi, tail);
    }

    /// A set constructor (`docs/spec/types.md` 8.3): the elements are evaluated (and checked)
    /// first, then the set is built: the bits of each range or'ed into a small set, or included in
    /// a new block of a big set.
    fn set_ctor(&mut self, ty: Ty, elems: &[SetCtorElem], dst: Reg) {
        let bounds: Vec<(Reg, Reg)> = elems
            .iter()
            .map(|el| {
                let lo = self.alloc(el.low.ty);
                self.expr(&el.low, lo);
                let hi = match &el.high {
                    Some(h) => {
                        let r = self.alloc(h.ty);
                        self.expr(h, r);
                        r
                    }
                    None => lo,
                };
                (lo, hi)
            })
            .collect();
        for (el, &(lo, hi)) in elems.iter().zip(&bounds) {
            if !el.check {
                continue;
            }
            if lo == hi {
                self.set_elem_check(lo, ty);
            } else {
                // An empty range (`L > H`) is not checked
                let skip = self.new_block();
                self.branch(op::JMP_IF_LT_INT, hi, Some(lo), skip);
                self.set_elem_check(lo, ty);
                self.set_elem_check(hi, ty);
                self.bind(skip);
            }
        }
        let a = self.alloc_borrow(ty);
        self.op1(op::LOAD_ZERO, a);
        if let Ty::BigSet(_) = ty {
            self.op_table(op::UNIQUE, a, Operand::Type(ty));
            for (lo, hi) in bounds {
                self.op3(op::SET_INCL, a, lo, hi);
            }
            self.op2(op::MOVE_REF, dst, a);
        } else {
            let m = self.alloc_borrow(Ty::Int64);
            for (lo, hi) in bounds {
                self.op3(op::SET_RANGE, m, lo, hi);
                self.op3(op::OR_INT, a, a, m);
            }
            self.op2(op::MOVE, dst, a);
        }
    }

    /// A set value in `src` converted to the set type `to` in `dst` (`Conversion::Set`): checked
    /// first when `check` (`ChkSet` against the bits of the range of `to`), then a small set's
    /// bits are kept or taken from a big set's first word, and a big set is built as a new block
    /// of `to` with the elements that fit.
    fn convert_set(&mut self, from: Ty, to: Ty, check: bool, dst: Reg, src: Reg) {
        if check {
            let types = &self.program.types;
            let (lo, hi) = types.set_range(to).expect("a set has a range");
            let tail: Vec<u16> = types.set_range_tail(to).encode_utf16().collect();
            let cap = types.set_bits(from) as i128 - 1;
            let mut words = Vec::new();
            if lo <= hi.min(cap) {
                setbits::insert_range(&mut words, lo as u32, hi.min(cap) as u32);
            }
            let mask = self.alloc(from);
            self.load_const(mask, &Const::Set(setbits::trim(words)), from);
            let opc = if let Ty::BigSet(_) = from {
                op::CHK_SET_BIG
            } else {
                op::CHK_SET
            };
            self.plain(
                opc,
                vec![Operand::Reg(src), Operand::Reg(mask), Operand::Str(tail)],
            );
        }
        match (from, to) {
            (Ty::Set(_), Ty::Set(_)) => {
                if dst != src {
                    self.op2(op::MOVE, dst, src);
                }
            }
            (Ty::BigSet(_), Ty::Set(_)) => self.op2(op::SET_BITS, dst, src),
            _ => {
                let a = self.alloc_borrow(to);
                self.op1(op::LOAD_ZERO, a);
                self.op_table(op::UNIQUE, a, Operand::Type(to));
                let opc = if let Ty::BigSet(_) = from {
                    op::SET_UNION
                } else {
                    op::SET_UNION_BITS
                };
                self.op2(opc, a, src);
                self.op2(op::MOVE_REF, dst, a);
            }
        }
    }

    /// A set comparison into `dst` (`docs/spec/types.md` 8.3): both small (integer bits) or both
    /// big (blocks, possibly of different sizes).
    fn set_compare(&mut self, bop: BinOp, dst: Reg, a: Reg, b: Reg, big: bool) {
        let (x, y) = if bop == BinOp::Ge { (b, a) } else { (a, b) };
        if big {
            match bop {
                BinOp::Eq | BinOp::Ne => self.op3(op::SET_EQ, dst, x, y),
                _ => self.op3(op::SET_SUB, dst, x, y),
            }
        } else {
            match bop {
                BinOp::Eq | BinOp::Ne => self.op3(op::EQ_INT, dst, x, y),
                _ => {
                    // `x <= y`: no element of `x` outside `y`
                    let t = self.alloc_borrow(Ty::Int64);
                    self.op2(op::NOT_INT, t, y);
                    self.op3(op::AND_INT, t, x, t);
                    let z = self.alloc_borrow(Ty::Int64);
                    self.op1(op::LOAD_ZERO, z);
                    self.op3(op::EQ_INT, dst, t, z);
                }
            }
        }
        if bop == BinOp::Ne {
            self.op2(op::NOT_BOOL, dst, dst);
        }
    }

    fn inc_dec(
        &mut self,
        target: &Place,
        target_ty: Ty,
        delta: Option<&Expr>,
        negative: bool,
        calc: Ty,
        checks: Checks,
    ) {
        let direct = self.direct_var_reg(target);
        // A field or element: its indices are evaluated once for the read and the write
        let part = is_part(target).then(|| {
            let part = self.eval_part(target);
            self.part_check(&part);
            part
        });
        // Inside the body of a `try`, a variable's value is made in a temporary and then moved to
        // its register, so that a handler sees the value before the call if a check raises
        // (`docs/spec/semantics.md` 3.1; `Inc(X)` is `X := X + 1`, as in `expr_into_var`).
        // Elsewhere the value is made in the register
        let staged = direct.is_some() && self.in_try() && (checks.range || checks.overflow);
        let (src, t) = match (direct, &part) {
            (Some(r), _) if staged => (r, self.alloc(target_ty)),
            (Some(r), _) => (r, r),
            (None, Some(part)) => {
                let t = self.alloc(target_ty);
                self.part_load(part, t);
                (t, t)
            }
            (None, None) => {
                let t = self.alloc(target_ty);
                self.load_place(target, t);
                (t, t)
            }
        };
        let start = staged.then(|| self.region_start());
        let is64 = matches!(calc, Ty::Int64 | Ty::UInt64);
        // `{$Q-}` `Integer`: the 32-bit operations narrow the result themselves
        let wide32 = wraps32(calc, checks.overflow);
        let small_const = match delta {
            None => Some(1i64),
            Some(d) => match &d.value {
                Some(Const::Int(v)) if (-128..=127).contains(v) => Some(*v as i64),
                _ => None,
            },
        };
        match small_const {
            Some(v) if !(is64 && checks.overflow) => {
                let v = if negative { -v } else { v };
                if (i8::MIN as i64..=i8::MAX as i64).contains(&v) {
                    let opc = if wide32 {
                        op::ADD_INT32_I
                    } else {
                        op::ADD_INT_I
                    };
                    self.op_imm(opc, t, src, v);
                } else {
                    let d = self.alloc(calc);
                    self.load_int(d, v);
                    let opc = if wide32 { op::ADD_INT32 } else { op::ADD_INT };
                    self.op3(opc, t, src, d);
                }
            }
            _ => {
                let d = self.alloc(calc);
                match delta {
                    Some(e) => self.expr(e, d),
                    None => self.load_int(d, 1),
                }
                let opc = match (calc, checks.overflow, negative) {
                    (Ty::Int64, true, false) => op::ADD_INT_CHK,
                    (Ty::Int64, true, true) => op::SUB_INT_CHK,
                    (Ty::UInt64, true, false) => op::ADD_UINT_CHK,
                    (Ty::UInt64, true, true) => op::SUB_UINT_CHK,
                    (_, _, false) if wide32 => op::ADD_INT32,
                    (_, _, true) if wide32 => op::SUB_INT32,
                    (_, _, false) => op::ADD_INT,
                    (_, _, true) => op::SUB_INT,
                };
                self.op3(opc, t, src, d);
            }
        }
        // From the computation type to the target type (`{$R}` of assignment): to the base type,
        // then the range of an enumeration or subrange
        let base = match self.program.types.base(target_ty) {
            Ty::Enum(_) => calc,
            b => b,
        };
        // Under `{$Q-}` a narrowing conversion to the target does the job of the `Narrow`
        let to_target = Conversion::Int {
            to: base,
            check: checks.range,
        };
        if !wide32
            && (arith_pending(calc, checks.overflow).is_none()
                || !Self::conversion_narrows_low_bits(to_target, calc))
        {
            self.narrow_arith(t, calc, checks.overflow);
        }
        self.convert_int(t, t, calc, base, checks.range);
        if matches!(target_ty, Ty::Enum(_) | Ty::Subrange(_)) && checks.range {
            self.range_check(t, target_ty);
        }
        match (direct, &part) {
            (Some(r), _) => {
                if let Some(start) = start
                    && !self.write_in_place(&start, t, r)
                {
                    self.op2(op::MOVE, r, t);
                }
            }
            (None, Some(part)) => self.part_store(part, t),
            (None, None) => self.store_place(target, t),
        }
    }

    /// `SetLength` (`docs/spec/types.md` 2.3, chapter 7): the target's indices, then the lengths,
    /// then the target changed in place (`StrSetLen`, or `SetLen` with each element of the
    /// following dimensions resized in a loop).
    fn set_length(&mut self, target: &Place, lens: &[Expr]) {
        let part = is_part(target).then(|| self.eval_part(target));
        let regs: Vec<Reg> = lens
            .iter()
            .map(|l| {
                let r = self.alloc_borrow(Ty::Int64);
                self.expr(l, r);
                r
            })
            .collect();
        let ty = self.place_type(target);
        let f = |this: &mut Self, t: Reg| {
            if ty == Ty::String {
                this.intrinsic(op::intr::STR_SET_LEN, &[t, regs[0]]);
            } else if ty == Ty::Utf8String {
                this.intrinsic(op::intr::U8_SET_LEN, &[t, regs[0]]);
            } else {
                this.set_len_dims(t, ty, &regs);
            }
        };
        match &part {
            Some(part) => {
                self.part_check(part);
                self.modify_part(part, f);
            }
            None => {
                self.modify_root(target, f);
            }
        }
    }

    /// `Insert` (`docs/spec/semantics.md` 5.3, `docs/spec/types.md` 8.6): the source, the target's
    /// indices and the index in argument order, then the target changed in place (`StrInsert` /
    /// `U8Insert`, or `ArrInsert` / `ArrInsertArr` with the array, the source and the index in
    /// consecutive registers).
    fn insert(&mut self, source: &Expr, target: &Place, index: &Expr, whole: bool) {
        let src = self.alloc(source.ty);
        self.expr(source, src);
        let part = is_part(target).then(|| self.eval_part(target));
        let idx = self.alloc_borrow(Ty::Int64);
        self.expr(index, idx);
        let ty = self.place_type(target);
        let f = |this: &mut Self, t: Reg| {
            if let Ty::DynArray(_) = ty {
                let opc = if whole {
                    op::ARR_INSERT_ARR
                } else {
                    op::ARR_INSERT
                };
                this.array_edit(opc, ty, t, [(src, source.ty), (idx, Ty::Int64)]);
            } else {
                let id = if ty == Ty::Utf8String {
                    op::intr::U8_INSERT
                } else {
                    op::intr::STR_INSERT
                };
                this.string_edit(id, t, [(src, source.ty), (idx, Ty::Int64)]);
            }
        };
        self.edit_place(target, part, f);
    }

    /// `Delete`: the target's indices, the index and the count, then the target changed in place
    /// (`StrDelete` / `U8Delete` / `ArrDelete`).
    fn delete(&mut self, target: &Place, index: &Expr, count: &Expr) {
        let part = is_part(target).then(|| self.eval_part(target));
        let idx = self.alloc_borrow(Ty::Int64);
        self.expr(index, idx);
        let cnt = self.alloc_borrow(Ty::Int64);
        self.expr(count, cnt);
        let ty = self.place_type(target);
        let args = [(idx, Ty::Int64), (cnt, Ty::Int64)];
        let f = |this: &mut Self, t: Reg| match ty {
            Ty::DynArray(_) => this.array_edit(op::ARR_DELETE, ty, t, args),
            Ty::Utf8String => this.string_edit(op::intr::U8_DELETE, t, args),
            _ => this.string_edit(op::intr::STR_DELETE, t, args),
        };
        self.edit_place(target, part, f);
    }

    /// `Exchange` (`docs/spec/types.md` chapter 7): the target's indices and the two indices,
    /// then the two elements swapped without counting (the counts stay the same). A dynamic array
    /// is written shared; a static array is made unique first. `I = J` writes each element back.
    fn exchange(&mut self, target: &Place, first: &Expr, second: &Expr, span: TextRange) {
        let part = is_part(target).then(|| self.eval_part(target));
        let ty = self.place_type(target);
        let elem = match ty {
            Ty::DynArray(_) => self.program.types.dyn_elem(ty),
            _ => self.program.types.array(ty).map(|(_, _, _, e)| e),
        }
        .expect("`Exchange` takes an array");
        let dynamic = matches!(ty, Ty::DynArray(_));
        let (ri, rj) = if dynamic {
            let ri = self.alloc_borrow(Ty::Int64);
            self.expr(first, ri);
            let rj = self.alloc_borrow(Ty::Int64);
            self.expr(second, rj);
            (ri, rj)
        } else {
            let (ri, ci) = self.elem_index(ty, first, span);
            let (rj, cj) = self.elem_index(ty, second, span);
            for (r, c) in [(ri, ci), (rj, cj)] {
                if let Some(c) = c {
                    self.chk_index(r, &c);
                }
            }
            (ri, rj)
        };
        let flat = self.is_flat(elem);
        let width = self.program.types.slot_width(elem);
        let f = |this: &mut Self, t: Reg| {
            if let Ty::Array(_) = ty {
                this.op_table(op::UNIQUE, t, Operand::Type(ty));
            }
            let at = |i: Reg| Hop {
                kind: if dynamic { HopKind::Dyn } else { HopKind::Agg },
                ty,
                elem: Some(Idx::Reg(i)),
                width,
                flat_elem: flat,
                off: 0,
                terms: Vec::new(),
                start: 0,
                val: elem,
            };
            // A flat element is swapped whole, through blocks of its own
            let (x, y) = if flat {
                (this.alloc(elem), this.alloc(elem))
            } else {
                (this.alloc_borrow(elem), this.alloc_borrow(elem))
            };
            this.hop_load(&at(ri), t, x, false);
            this.hop_load(&at(rj), t, y, false);
            if flat {
                this.hop_store(&at(ri), t, y);
                this.hop_store(&at(rj), t, x);
                this.op1(op::RELEASE, x);
                this.op1(op::RELEASE, y);
            } else {
                this.hop_set(&at(ri), t, y, false);
                this.hop_set(&at(rj), t, x, false);
            }
        };
        self.edit_place(target, part, f);
    }

    /// `ArrayMoveRange` (`docs/spec/bytecode.md` 2.22): the target's indices, then the three
    /// `Int64` arguments, then the rotation of the dynamic array in place (shared, as `Exchange`
    /// writes it; no count changes).
    fn array_move_range(&mut self, target: &Place, args: [&Expr; 3]) {
        let part = is_part(target).then(|| self.eval_part(target));
        let values = args.map(|e| {
            let r = self.alloc_borrow(Ty::Int64);
            self.expr(e, r);
            r
        });
        let f = |this: &mut Self, t: Reg| {
            // `B` to `B+2` are consecutive call-argument temporaries
            let regs = values.map(|v| {
                let r = this.alloc_arg(Some(Ty::Int64));
                this.op2(op::MOVE, r, v);
                r
            });
            this.intrinsic(op::intr::ARRAY_MOVE_RANGE, &[t, regs[0], regs[1], regs[2]]);
        };
        self.edit_place(target, part, f);
    }

    /// `ArraySortRange` (`docs/spec/bytecode.md` 2.22): the target's indices, then the two `Int64`
    /// arguments, then, for an element type whose default order is a built-in comparison
    /// (`IsPlainOrder`), the comparison and `I96` on the dynamic array in place (shared, as
    /// `Exchange` writes it; no count changes). Any other type raises `EInvalidOperation` once
    /// the arguments are evaluated (only a mistake of the standard library gets there).
    fn array_sort_range(&mut self, target: &Place, index: &Expr, count: &Expr) {
        let ty = self.place_type(target);
        let elem = self
            .program
            .types
            .elem_of(ty)
            .expect("sema checked a dynamic array");
        let Some(kind) = self.sort_kind(elem) else {
            if is_part(target) {
                self.eval_part(target);
            }
            for e in [index, count] {
                let r = self.alloc_borrow(Ty::Int64);
                self.expr(e, r);
            }
            let name = self.program.types.name(elem);
            let msg = format!("`{name}` is not ordered by a built-in comparison");
            self.raise_invalid_operation(&msg);
            return;
        };
        let part = is_part(target).then(|| self.eval_part(target));
        let values = [index, count].map(|e| {
            let r = self.alloc_borrow(Ty::Int64);
            self.expr(e, r);
            r
        });
        let f = |this: &mut Self, t: Reg| {
            // `B` to `B+2` are consecutive call-argument temporaries
            let regs = values.map(|v| {
                let r = this.alloc_arg(Some(Ty::Int64));
                this.op2(op::MOVE, r, v);
                r
            });
            let k = this.alloc_arg(Some(Ty::Int64));
            this.load_int(k, kind);
            this.intrinsic(op::intr::ARRAY_SORT_RANGE, &[t, regs[0], regs[1], k]);
        };
        self.edit_place(target, part, f);
    }

    /// How `ArraySortRange` (`I96`) compares the elements of `ty` (`docs/spec/bytecode.md`
    /// 2.22), from the same kind as the expansion of `KeyCompare` (`docs/spec/collections.md`
    /// 10.2); `None` when the order is not a built-in comparison. `IsPlainOrder` is whether it is
    /// `Some`.
    fn sort_kind(&self, ty: Ty) -> Option<i64> {
        Some(match self.program.types.order_kind(ty) {
            OrderKind::Int => bytecode::SORT_INT,
            OrderKind::UInt => bytecode::SORT_UINT,
            OrderKind::Single => bytecode::SORT_SINGLE,
            OrderKind::Double => bytecode::SORT_DOUBLE,
            OrderKind::Str => bytecode::SORT_STR,
            OrderKind::Utf8 => bytecode::SORT_UTF8,
            OrderKind::Call | OrderKind::None => return None,
        })
    }

    /// `HashRelink` (`docs/spec/bytecode.md` 2.23): the buckets in `A`, then the hashes, the next
    /// entries and the count in consecutive registers. The arrays are read without counting (the
    /// intrinsic runs no Pascal code) and written in place, so the places are not written back.
    fn hash_relink(&mut self, buckets: &Place, hashes: &Expr, next: &Place, count: &Expr) {
        let b = self.place_borrowed(buckets);
        let h = self.borrowed_value(hashes);
        let x = self.place_borrowed(next);
        let n = self.alloc_borrow(Ty::Int64);
        self.expr(count, n);
        let types = [hashes.ty, self.place_type(next), Ty::Int64];
        let mut args = Vec::new();
        for (r, t) in [h, x, n].into_iter().zip(types) {
            let a = self.alloc_arg(Some(t));
            self.op2(op::MOVE, a, r);
            args.push(a);
        }
        self.intrinsic(op::intr::HASH_RELINK, &[b, args[0], args[1], args[2]]);
    }

    /// The value of a place read only right away: without counting when it is the root of the
    /// place (a variable, a global or an instance's field), else a counted copy let go of when
    /// the statement ends.
    fn place_borrowed(&mut self, p: &Place) -> Reg {
        match p {
            Place::Var(..) | Place::Global(_) | Place::ObjField { .. } => self.root_borrowed(p),
            _ => {
                let t = self.alloc(self.place_type(p));
                self.load_place(p, t);
                self.owned.push(t);
                t
            }
        }
    }

    /// Changes the value of a place in place with `f` (given the register holding the value).
    fn edit_place(
        &mut self,
        target: &Place,
        part: Option<Part<'_>>,
        f: impl FnOnce(&mut Self, Reg),
    ) {
        match &part {
            Some(part) => {
                self.part_check(part);
                self.modify_part(part, f);
            }
            None => {
                self.modify_root(target, f);
            }
        }
    }

    /// A string instruction `opc A B` that replaces the string in `t` (`B`, `B+1` are `args`).
    fn string_edit(&mut self, id: u8, t: Reg, args: [(Reg, Ty); 2]) {
        self.intrinsic_pair(id, t, args);
    }

    /// An array instruction `opc A Bx` on the array in `t` (an owning value read without counting;
    /// `A+1`, `A+2` are `args`), written back to `t`.
    fn array_edit(&mut self, opc: u16, ty: Ty, t: Reg, args: [(Reg, Ty); 2]) {
        let a = self.alloc_arg(Some(ty));
        let b = self.alloc_arg(Some(args[0].1));
        let c = self.alloc_arg(Some(args[1].1));
        self.op2(op::MOVE, a, t);
        self.op2(op::MOVE, b, args[0].0);
        self.op2(op::MOVE, c, args[1].0);
        self.op_table(opc, a, Operand::Type(ty));
        self.op2(op::MOVE, t, a);
    }

    /// Resizes the dynamic array in `arr` (an owning value read without counting) to `lens[0]`,
    /// and each of its elements to the following lengths.
    fn set_len_dims(&mut self, arr: Reg, ty: Ty, lens: &[Reg]) {
        let Some((&len, rest)) = lens.split_first() else {
            return;
        };
        // `SetLen` takes the array and the length in consecutive registers
        let a = self.alloc_borrow(ty);
        let n = self.alloc_borrow(Ty::Int64);
        self.op2(op::MOVE, a, arr);
        self.op2(op::MOVE, n, len);
        self.plain(
            op::SET_LEN,
            vec![Operand::Reg(a), Operand::Type(ty), Operand::Reg(n)],
        );
        self.op2(op::MOVE, arr, a);
        if rest.is_empty() {
            return;
        }
        let elem = self
            .program
            .types
            .dyn_elem(ty)
            .expect("SetLength with several lengths has nested dynamic arrays");
        let i = self.alloc_borrow(Ty::Int64);
        self.op1(op::LOAD_ZERO, i);
        let top = self.new_block();
        let end = self.new_block();
        self.bind(top);
        self.branch(op::JMP_IF_LE_INT, len, Some(i), end);
        let e = self.alloc_borrow(elem);
        self.op3(op::DYN_GET, e, arr, i);
        self.set_len_dims(e, elem, rest);
        self.op3(op::DYN_SET, arr, i, e);
        self.op_imm(op::ADD_INT_I, i, i, 1);
        self.jump(top);
        self.bind(end);
    }

    /// An array constructor (`docs/spec/types.md` chapter 4): the elements first, each in a
    /// register of its own, then a new dynamic array (`NewArr`) or a default static array
    /// (`Unique` of 0) filled with them, moved to `dst`. Nothing can fail between making the array
    /// and moving it, so an exception in an element leaves nothing behind.
    fn array_ctor(&mut self, ty: Ty, elems: &[Expr], dst: Reg) {
        if elems.is_empty() {
            return self.load_str(dst, &[]);
        }
        let vals: Vec<Reg> = elems
            .iter()
            .map(|x| {
                let t = self.alloc(x.ty);
                self.expr(x, t);
                t
            })
            .collect();
        let a = self.alloc_borrow(ty);
        let dynamic = matches!(ty, Ty::DynArray(_));
        if dynamic {
            self.load_int(a, elems.len() as i64);
            self.op_table(op::NEW_ARR, a, Operand::Type(ty));
        } else {
            self.op1(op::LOAD_ZERO, a);
            self.op_table(op::UNIQUE, a, Operand::Type(ty));
        }
        let k = self.alloc_borrow(Ty::Int64);
        let types = &self.program.types;
        let elem = types
            .elem_of(ty)
            .expect("an array constructor makes an array");
        let mut h = Hop {
            kind: if dynamic { HopKind::Dyn } else { HopKind::Agg },
            ty,
            elem: None,
            width: types.slot_width(elem),
            flat_elem: self.is_flat(elem),
            off: 0,
            terms: Vec::new(),
            start: 0,
            val: elem,
        };
        for (i, v) in vals.into_iter().enumerate() {
            h.elem = Some(if dynamic {
                self.load_int(k, i as i64);
                Idx::Reg(k)
            } else {
                Idx::Const(i as i64)
            });
            self.hop_store(&h, a, v);
        }
        self.op2(op::MOVE_REF, dst, a);
    }

    /// `Read` / `ReadLn` (`docs/spec/semantics.md` 6.2): each variable from the input line buffer
    /// (`docs/spec/bytecode.md` 2.17), then `ReadLn` drops the rest of the line.
    fn read(&mut self, targets: &[Place], newline: bool) {
        if newline {
            self.intrinsic(op::intr::READ_LN_BEGIN, &[]);
        }
        for (i, p) in targets.iter().enumerate() {
            let last = i + 1 == targets.len();
            // The indices of the target come first (the place to read into)
            let part = is_part(p).then(|| {
                let part = self.eval_part(p);
                self.part_check(&part);
                part
            });
            let ty = self.place_type(p);
            let v = if ty == Ty::Char {
                let t = self.alloc(ty);
                self.intrinsic(op::intr::READ_CHAR, &[t]);
                t
            } else if matches!(ty, Ty::String | Ty::Utf8String) {
                let a = self.alloc(Ty::String);
                self.intrinsic(op::intr::READ_REST, &[a]);
                if ty == Ty::Utf8String {
                    let t = self.alloc(ty);
                    self.intrinsic(op::intr::STR_TO_U8, &[t, a]);
                    t
                } else {
                    a
                }
            } else {
                // A number: a word, or the rest of the line for the last variable of `ReadLn`
                // (only blanks may follow it)
                let a = self.alloc(Ty::String);
                let id = if newline && last {
                    op::intr::READ_REST
                } else {
                    op::intr::READ_WORD
                };
                self.intrinsic(id, &[a]);
                let t = self.alloc(ty);
                if ty.is_integer() {
                    let w = ty.width_code().unwrap_or(6) as i64;
                    self.op_imm(op::STR_TO_INT_IO, t, a, w);
                } else {
                    let id = match ty {
                        Ty::Single => op::intr::STR_TO_F32_IO,
                        Ty::Currency => op::intr::STR_TO_CURR_IO,
                        _ => op::intr::STR_TO_F64_IO,
                    };
                    self.intrinsic(id, &[t, a]);
                }
                t
            };
            match &part {
                Some(part) => self.part_store(part, v),
                None => self.store_place(p, v),
            }
        }
        if newline {
            self.intrinsic(op::intr::READ_LN_END, &[]);
        }
    }

    fn write(&mut self, args: &[WriteArg], newline: bool, err: bool) {
        // Consecutive constant strings (without width) are concatenated at compile time
        let a = self.alloc(Ty::String);
        let mut started = false;
        let mut pending: Vec<u16> = Vec::new();
        let flush = |this: &mut Self, pending: &mut Vec<u16>, started: &mut bool| {
            if pending.is_empty() {
                return;
            }
            if *started {
                let t = this.alloc(Ty::String);
                this.load_str(t, pending);
                this.op3(op::CONCAT_STR, a, a, t);
            } else {
                this.load_str(a, pending);
                *started = true;
            }
            pending.clear();
        };
        for arg in args {
            if arg.width.is_none()
                && let Some(c) = &arg.value.value
                && let Some(units) = const_as_string(c, arg.value.ty, &self.program.types)
            {
                pending.extend(units);
                continue;
            }
            flush(self, &mut pending, &mut started);
            let s = if started { self.alloc(Ty::String) } else { a };
            let old = self.set_line(arg.value.span);
            match &arg.precision {
                Some(d) => self.fixed_value(&arg.value, d, s),
                None => {
                    self.string_value(&arg.value, s);
                    // `Write(G)`: the copy of `G` is let go of when the statement ends
                    self.let_go_at_statement_end(&arg.value, s);
                }
            }
            if let Some(w) = &arg.width {
                let wr = self.alloc(w.ty);
                self.expr(w, wr);
                self.op3(op::STR_PAD_LEFT, s, s, wr);
            }
            self.line = old;
            if started {
                self.op3(op::CONCAT_STR, a, a, s);
            }
            started = true;
        }
        if newline {
            pending.push(10);
        }
        flush(self, &mut pending, &mut started);
        let name = if err {
            "Console.WriteError"
        } else {
            "Console.Write"
        };
        let h = self.ml.host(name, 1, false);
        self.call(op::CALL_HOST, None, Operand::Host(h), vec![a], a);
    }

    /// Converts a `Write` argument to a string and puts it in `dst` (an owning register).
    fn string_value(&mut self, e: &Expr, dst: Reg) {
        if e.ty.is_managed() {
            self.expr(e, dst);
            return;
        }
        let x = self.expr_any(e);
        if let Ty::Enum(_) = e.ty {
            // The name of the value (`docs/spec/types.md` 8.1)
            self.plain(
                op::ENUM_TO_STR,
                vec![Operand::Reg(dst), Operand::Reg(x), Operand::Type(e.ty)],
            );
            return;
        }
        let id = match e.ty {
            Ty::Double => Some(op::intr::F64_TO_STR),
            Ty::Single => Some(op::intr::F32_TO_STR),
            Ty::Currency => Some(op::intr::CURR_TO_STR),
            _ => None,
        };
        if let Some(id) = id {
            self.intrinsic(id, &[dst, x]);
            return;
        }
        let opc = match e.ty {
            Ty::Char => op::STR_FROM_CHAR,
            Ty::Boolean => op::BOOL_TO_STR,
            Ty::UInt64 => op::UINT_TO_STR,
            _ => op::INT_TO_STR,
        };
        self.op2(opc, dst, x);
    }

    /// `X:W:D` of a real or `Currency` (`docs/spec/semantics.md` 6.1): the text with `D`
    /// decimals in `dst` (a `Single` is widened first; exact).
    fn fixed_value(&mut self, e: &Expr, d: &Expr, dst: Reg) {
        let x = self.alloc(if e.ty == Ty::Currency {
            Ty::Currency
        } else {
            Ty::Double
        });
        self.expr(e, x);
        if e.ty == Ty::Single {
            self.op2(op::F32_TO_F64, x, x);
        }
        let dr = self.expr_any(d);
        let (id, xty) = if e.ty == Ty::Currency {
            (op::intr::CURR_TO_FIXED, Ty::Currency)
        } else {
            (op::intr::F64_TO_FIXED, Ty::Double)
        };
        self.intrinsic_pair(id, dst, [(x, xty), (dr, d.ty)]);
    }

    // ---- Integer helpers ----

    /// Narrows the result when the computation type is narrow (`{$Q}`).
    fn narrow_arith(&mut self, reg: Reg, calc: Ty, overflow: bool) {
        let Some(w) = narrow_width(calc) else {
            return;
        };
        let opc = if overflow { op::NARROW_OVF } else { op::NARROW };
        self.op_imm(opc, reg, reg, w);
    }

    /// Emits the pending `Narrow` of `reg`, if any.
    fn settle(&mut self, reg: Reg, pending: Pending) {
        if let Some(w) = pending {
            self.op_imm(op::NARROW, reg, reg, w);
        }
    }

    /// Settles `pending` unless it is the width `want` (that of the operation about to read the
    /// low bits of `reg`).
    fn settle_unless(&mut self, reg: Reg, pending: Pending, want: Pending) {
        if pending.is_some() && pending != want {
            self.settle(reg, pending);
        }
    }

    /// The `Narrow` width of an integer conversion from `from` to `to`, or `None` when the value
    /// already fits (a wider type: nothing to do).
    fn int_conversion_width(from: Ty, to: Ty) -> Option<i64> {
        let (flo, fhi) = from.range().unwrap_or((i64::MIN as i128, i64::MAX as i128));
        let (tlo, thi) = to.range().unwrap_or((i64::MIN as i128, i64::MAX as i128));
        if from == to || (tlo <= flo && fhi <= thi) {
            return None;
        }
        Some(to.width_code().unwrap_or(6) as i64)
    }

    /// Integer type conversion (`{$R}` of assignment compatibility). Nothing to do for a wider type.
    fn convert_int(&mut self, dst: Reg, src: Reg, from: Ty, to: Ty, range: bool) {
        let Some(w) = Self::int_conversion_width(from, to) else {
            if dst != src {
                self.op2(op::MOVE, dst, src);
            }
            return;
        };
        let opc = if range { op::NARROW_RNG } else { op::NARROW };
        self.op_imm(opc, dst, src, w);
    }

    /// Whether the conversion is a plain `Narrow` that reads only the low 32 bits or fewer, and
    /// so also does the job of a pending `Narrow` of its operand.
    fn conversion_narrows_low_bits(kind: Conversion, from: Ty) -> bool {
        match kind {
            Conversion::Int { to, check: false } => {
                Self::int_conversion_width(from, to).is_some_and(low_bits_width)
            }
            Conversion::IntToChar { check: false } => true,
            _ => false,
        }
    }

    /// Converts the value of `src` (of type `from`) into `dst`. `dst` may be `src`, except for
    /// `Char` → `string`, whose result is a different register.
    fn convert(&mut self, kind: Conversion, from: Ty, dst: Reg, src: Reg) {
        match kind {
            Conversion::Int { to, check } => self.convert_int(dst, src, from, to, check),
            Conversion::CharToStr => self.op2(op::STR_FROM_CHAR, dst, src),
            Conversion::StrToU8 => self.intrinsic(op::intr::STR_TO_U8, &[dst, src]),
            Conversion::U8ToStr => self.intrinsic(op::intr::U8_TO_STR, &[dst, src]),
            // The checked form uses the `Char` width so that the `ERangeError` names `Char`
            // (`docs/spec/semantics.md` 2.5); both widths keep the low 16 bits
            Conversion::IntToChar { check: true } => self.op_imm(op::NARROW_RNG, dst, src, 8),
            Conversion::IntToChar { check: false } => self.op_imm(op::NARROW, dst, src, 3),
            Conversion::IntToBool => {
                let z = self.alloc(from);
                self.op1(op::LOAD_ZERO, z);
                self.op3(op::EQ_INT, dst, src, z);
                self.op2(op::NOT_BOOL, dst, dst);
            }
            Conversion::OrdToInt => {
                if dst != src {
                    self.op2(op::MOVE, dst, src);
                }
            }
            Conversion::Range { to, check } => {
                if dst != src {
                    self.op2(op::MOVE, dst, src);
                }
                if check {
                    self.range_check(dst, to);
                }
            }
            Conversion::Set { to, check } => self.convert_set(from, to, check, dst, src),
            Conversion::Num { to } => self.convert_num(from, to, dst, src),
        }
    }

    /// An integer, real or `Currency` value to the real or `Currency` type `to`
    /// (`docs/spec/types.md` 8.4): one rounding.
    fn convert_num(&mut self, from: Ty, to: Ty, dst: Reg, src: Reg) {
        // The `Currency` conversions are intrinsics (`docs/spec/bytecode.md` 2.19)
        let opc = match (from, to) {
            (Ty::UInt64, Ty::Double) => Ok(op::UINT_TO_F64),
            (Ty::UInt64, Ty::Single) => Ok(op::UINT_TO_F32),
            (Ty::UInt64, _) => Err(op::intr::UINT_TO_CURR),
            (f, Ty::Double) if f.is_integer() => Ok(op::INT_TO_F64),
            (f, Ty::Single) if f.is_integer() => Ok(op::INT_TO_F32),
            (f, _) if f.is_integer() => Err(op::intr::INT_TO_CURR),
            (Ty::Single, Ty::Double) => Ok(op::F32_TO_F64),
            (Ty::Double, Ty::Single) => Ok(op::F64_TO_F32),
            (Ty::Currency, Ty::Double) => Err(op::intr::CURR_TO_F64),
            (Ty::Currency, Ty::Single) => Err(op::intr::CURR_TO_F32),
            (Ty::Double, Ty::Currency) => Err(op::intr::F64_TO_CURR),
            (Ty::Single, Ty::Currency) => {
                // Exact, then one rounding to four decimals
                self.op2(op::F32_TO_F64, dst, src);
                self.intrinsic(op::intr::F64_TO_CURR, &[dst, dst]);
                return;
            }
            _ => unreachable!("sema converts only numbers to reals and `Currency`"),
        };
        match opc {
            Ok(opc) => self.op2(opc, dst, src),
            Err(id) => self.intrinsic(id, &[dst, src]),
        }
    }

    /// `ChkRange` of the value in `reg` against the range of the enumeration or subrange type `ty`
    /// (`docs/spec/types.md` 8.1, 8.2): like `ChkIndex`, it takes the value minus the lower bound
    /// and the number of values; the constant is the `` `T` (L..H) `` part of the message.
    fn range_check(&mut self, reg: Reg, ty: Ty) {
        let types = &self.program.types;
        let (lo, hi) = types
            .ord_range(ty)
            .expect("a range check is on an ordinal type");
        let tail: Vec<u16> = types.range_tail(ty).encode_utf16().collect();
        self.range_check_with(reg, lo, hi, tail);
    }

    /// `ChkRange` of `reg` against `lo..hi`, with `tail` as the message's type and range.
    fn range_check_with(&mut self, reg: Reg, lo: i128, hi: i128, tail: Vec<u16>) {
        let t = self.alloc_borrow(Ty::Int64);
        if lo == 0 {
            self.op2(op::MOVE, t, reg);
        } else if (i8::MIN as i128..=i8::MAX as i128).contains(&-lo) {
            self.op_imm(op::ADD_INT_I, t, reg, -lo as i64);
        } else {
            let k = self.alloc_borrow(Ty::Int64);
            self.load_int(k, lo as i64);
            self.op3(op::SUB_INT, t, reg, k);
        }
        let len = self.alloc_borrow(Ty::Int64);
        self.load_int(len, (hi - lo + 1) as u64 as i64);
        self.plain(
            op::CHK_RANGE,
            vec![Operand::Reg(t), Operand::Reg(len), Operand::Str(tail)],
        );
    }

    // ---- Expressions ----

    /// Puts the value of an expression into `dst`.
    fn expr(&mut self, e: &Expr, dst: Reg) {
        let pending = self.expr_lazy(e, dst);
        let old = self.set_line(e.span);
        self.settle(dst, pending);
        self.line = old;
    }

    /// Puts the value of an expression into `dst`, possibly leaving a `Narrow` pending (`Pending`):
    /// the caller settles it unless it only reads the low bits of `dst`.
    fn expr_lazy(&mut self, e: &Expr, dst: Reg) -> Pending {
        let old = self.set_line(e.span);
        let mark = self.mark();
        if let Some(c) = &e.value
            && !matches!(e.kind, ExprKind::Call { .. } | ExprKind::Builtin { .. })
        {
            self.load_const(dst, c, e.ty);
            self.release(mark);
            self.line = old;
            return None;
        }
        let mut pending = None;
        match &e.kind {
            ExprKind::Const { .. } => {
                // value has already been handled above
                self.op1(op::LOAD_ZERO, dst);
            }
            ExprKind::Var(v) => self.load_place(&Place::Var(*v, e.span), dst),
            ExprKind::Global(g) => self.load_place(&Place::Global(*g), dst),
            ExprKind::Binary { first, steps } => pending = self.binary_lazy(first, steps, dst),
            ExprKind::Unary {
                op: uop,
                operand,
                calc,
            } => {
                // `not` and a wrapping `-` read only the low bits of a narrow operand
                let wraps = match (uop, calc) {
                    (UnOp::Not, Calc::Int(t)) => arith_pending(*t, false),
                    (UnOp::Neg, Calc::Int(t)) => arith_pending(*t, self.current_overflow),
                    _ => None,
                };
                let p = self.expr_lazy(operand, dst);
                self.settle_unless(dst, p, wraps);
                match (uop, calc) {
                    (UnOp::Not, Calc::Bool) => self.op2(op::NOT_BOOL, dst, dst),
                    (UnOp::Not, Calc::Int(t)) => {
                        self.op2(op::NOT_INT, dst, dst);
                        // Not checked by `{$Q}`; left pending only under `{$Q-}`
                        let overflow = self.current_overflow;
                        pending = arith_pending(*t, overflow);
                        if overflow {
                            self.narrow_arith(dst, *t, false);
                        }
                    }
                    (UnOp::Neg, Calc::Real(Ty::Single)) => self.op2(op::NEG_F32, dst, dst),
                    (UnOp::Neg, Calc::Real(_)) => self.op2(op::NEG_F64, dst, dst),
                    (UnOp::Neg, Calc::Curr) => self.intrinsic(op::intr::NEG_CURR, &[dst, dst]),
                    (UnOp::Neg, Calc::Int(t)) => {
                        let overflow = self.current_overflow;
                        if *t == Ty::Int64 && overflow {
                            self.op2(op::NEG_INT_CHK, dst, dst);
                        } else {
                            self.op2(op::NEG_INT, dst, dst);
                            pending = arith_pending(*t, overflow);
                            if overflow {
                                self.narrow_arith(dst, *t, overflow);
                            }
                        }
                    }
                    _ => {}
                }
            }
            ExprKind::StrIndex { base, index } => {
                // When the index runs no code, nothing can change the string before `StrIndex`
                // reads it, so it is read without counting (as `Length`); a counted copy would
                // cost a `Release` in a loop over `G[I]` (`docs/design/lifetime.md` 3.5)
                let b = if expr_has_call(index) {
                    self.expr_any(base)
                } else {
                    self.borrowed_value(base)
                };
                let i = self.expr_any(index);
                let opc = if base.ty == Ty::Utf8String {
                    op::U8_INDEX
                } else {
                    op::STR_INDEX
                };
                self.op3(opc, dst, b, i);
            }
            ExprKind::Field { .. } | ExprKind::Elem { .. } => self.read_field(e, dst, true),
            ExprKind::ArrayCtor(elems) => self.array_ctor(e.ty, elems, dst),
            ExprKind::Slice { base, low, high } => {
                self.slice(base, low.as_deref(), high.as_deref(), e.ty, dst)
            }
            ExprKind::IndexRange { first, last } => {
                self.index_range(first.as_deref(), last.as_deref(), e.ty, dst)
            }
            ExprKind::Call {
                routine,
                args,
                dispatch,
            } => self.call_routine(*routine, args, *dispatch, Some(dst)),
            // `docs/spec/objects.md` 14.9
            ExprKind::RoutineRef(r) => {
                let f = self.ml.func_index[r.0 as usize];
                self.load_func(dst, f, 1);
            }
            ExprKind::ToClosure(x) => {
                let t = self.alloc_borrow(Ty::Int64);
                self.expr(x, t);
                self.intrinsic(op::intr::ROUTINE_REF, &[dst, t]);
            }
            ExprKind::Anon { func, captures } => {
                let f = self.ml.func_index[func.0 as usize];
                self.make_closure(Some(f), captures, dst)
            }
            ExprKind::NestedRef { func, block } => {
                let m = self.mark();
                let b = self.alloc_arg(None);
                let f = self.alloc_arg(Some(Ty::Int64));
                self.load_block(*block, b);
                self.load_func(f, self.ml.func_index[func.0 as usize], 0);
                self.intrinsic(op::intr::CLOSURE_WITH, &[dst, b]);
                self.release(m);
            }
            ExprKind::MethodRef { base, routine, how } => {
                self.method_ref(base, *routine, *how, dst)
            }
            ExprKind::CallValue { callee, args } => {
                let ret = (!e.ty.is_error()).then_some(e.ty);
                self.call_value(callee, args, ret, Some(dst));
            }
            ExprKind::RecordNew { call } => {
                // `docs/spec/objects.md` chapter 6: the constructor runs on a temporary holding
                // the default value, which becomes the value
                let ExprKind::Call {
                    routine,
                    args,
                    dispatch,
                } = &call.kind
                else {
                    unreachable!("RecordNew always holds the call of the constructor");
                };
                let (_, temps) = self.call_routine_with(*routine, args, *dispatch, None, None);
                let t = *temps
                    .first()
                    .expect("a record constructor's first argument is a temporary");
                self.move_value(e.ty, dst, t);
            }
            ExprKind::Builtin { func, args } => self.builtin(*func, args, e.ty, dst),
            ExprKind::Key { op, args, func } => self.key(*op, args, *func, dst),
            ExprKind::Convert {
                operand,
                kind: kind @ (Conversion::CharToStr | Conversion::StrToU8 | Conversion::U8ToStr),
            } => {
                // The result is a new value made from the operand's register
                let x = self.expr_any(operand);
                self.convert(*kind, operand.ty, dst, x);
            }
            ExprKind::Convert {
                operand,
                kind: Conversion::Set { to, check },
            } => {
                // The source keeps its own register (a small and a big set do not share one)
                let src = self.expr_any(operand);
                self.convert_set(operand.ty, *to, *check, dst, src);
            }
            ExprKind::Convert { operand, kind } => {
                // A narrowing conversion does the job of the operand's pending `Narrow`
                let p = self.expr_lazy(operand, dst);
                if !Self::conversion_narrows_low_bits(*kind, operand.ty) {
                    self.settle(dst, p);
                }
                self.convert(*kind, operand.ty, dst, dst);
            }
            ExprKind::SetCtor(elems) => self.set_ctor(e.ty, elems, dst),
            ExprKind::New {
                class,
                ctor,
                args,
                after,
                of,
                dispatch,
            } => self.new_instance(*class, *ctor, args, *after, of.as_deref(), *dispatch, dst),
            ExprKind::ClassRefValue(class) => {
                // `docs/spec/objects.md` 14.1: the type index + 1
                self.op_table(op::LOAD_INT, dst, Operand::ClassRef(Ty::Class(*class)));
            }
            ExprKind::ClassInfo { op: info, args } => {
                // `docs/spec/objects.md` 14.3: the intrinsics check their operands
                let id = match info {
                    ClassInfoOp::ClassType => op::intr::CLASS_TYPE,
                    ClassInfoOp::ClassRefName => op::intr::CLASS_REF_NAME,
                    ClassInfoOp::ClassParent => op::intr::CLASS_PARENT,
                    ClassInfoOp::InheritsFrom => op::intr::CLASS_INHERITS,
                    ClassInfoOp::RefCount => op::intr::REF_COUNT,
                };
                if let [a, b] = args.as_slice() {
                    let x = self.borrowed_value(a);
                    let y = self.borrowed_value(b);
                    self.intrinsic_pair(id, dst, [(x, a.ty), (y, b.ty)]);
                } else {
                    let b = self.borrowed_value(&args[0]);
                    self.intrinsic(id, &[dst, b]);
                }
            }
            ExprKind::InheritedBuiltin {
                base,
                msg: Some(msg),
                ..
            } => {
                // `inherited Create(Msg)` of `Exception` writes `Message` of `Self`
                // (`docs/spec/objects.md` 8.2, 14.3)
                let b = self.borrowed_value(base);
                let m = self.expr_any(msg);
                self.obj_set(b, bytecode::EXC_SLOT_MESSAGE, m, Ty::String, true);
            }
            // The built-in body does nothing (`docs/spec/objects.md` 14.3)
            ExprKind::InheritedBuiltin { .. } => {}
            ExprKind::ObjField { .. } => self.read_field(e, dst, true),
            ExprKind::Weak(inner) => {
                // `docs/spec/objects.md` 4.4, 14.3: the handle is read without counting, then
                // `LoadWeak` counts it into `dst` (or writes `nil`)
                let h = self.borrowed_value(inner);
                self.op2(op::LOAD_WEAK, dst, h);
            }
            ExprKind::ClassCheck { operand, class, op } => {
                self.class_check(operand, *class, *op, e.ty, dst)
            }
            ExprKind::Supports {
                operand,
                intf,
                target,
                class_ref,
            } => self.supports(operand, *intf, target.as_ref(), *class_ref, dst),
            ExprKind::ClassName { base } => {
                // `docs/spec/objects.md` 14.3: the intrinsic checks the handle
                let b = self.borrowed_value(base);
                self.intrinsic(op::intr::CLASS_NAME, &[dst, b]);
            }
            ExprKind::ExcMember { base, member } => {
                let b = self.borrowed_value(base);
                let n = match member {
                    ExcMember::StackTrace => bytecode::EXC_INFO_STACK_TRACE,
                    ExcMember::Suppressed => bytecode::EXC_INFO_SUPPRESSED,
                };
                self.plain(
                    op::EXC_INFO,
                    vec![Operand::Reg(dst), Operand::Reg(b), Operand::Imm(n as i64)],
                );
            }
            ExprKind::TryStrToInt { s, target } => {
                // The value goes to the `out` argument after the conversion (`docs/spec/semantics.md` 5.3)
                let x = self.expr_any(s);
                let b = self.alloc_arg(Some(s.ty));
                let v = self.alloc_arg(Some(Ty::Integer));
                self.op2(op::MOVE, b, x);
                self.intrinsic(op::intr::TRY_STR_TO_INT, &[dst, b, v]);
                let r = self.alloc(Ty::Integer);
                self.op2(op::MOVE, r, v);
                self.store_place(target, r);
            }
            ExprKind::Error => self.op1(op::LOAD_ZERO, dst),
        }
        self.release(mark);
        self.line = old;
        pending
    }

    /// A register holding the expression's value (the variable's register for a variable, otherwise a temporary).
    /// A counted copy of a variable holding no object is let go of when the statement ends
    /// ([`FuncLower::let_go_at_statement_end`]).
    fn expr_any(&mut self, e: &Expr) -> Reg {
        if e.value.is_none()
            && let ExprKind::Var(v) = &e.kind
            && !self.is_ref_param(*v)
        {
            return self.reg_of(*v);
        }
        let t = self.alloc(e.ty);
        self.expr(e, t);
        self.let_go_at_statement_end(e, t);
        t
    }

    /// A left fold. Under `{$Q-}` the `Narrow` of a narrow integer result may be left pending (`Pending`).
    fn binary_lazy(&mut self, first: &Expr, steps: &[BinStep], dst: Reg) -> Pending {
        // A string comparison: the strings on its left are concatenated in a register of their own,
        // and the comparison's `Boolean` goes on in the accumulator (a string never enters `dst`)
        // A comparison of big sets (or of a small set with a big one) likewise
        let big_cmp = |k: usize| {
            let s = &steps[k];
            let lt = if k == 0 { first.ty } else { steps[k - 1].ty };
            s.calc == Calc::Set
                && s.op.is_comparison()
                && (matches!(lt, Ty::BigSet(_)) || matches!(s.rhs.ty, Ty::BigSet(_)))
        };
        if let Some(k) = (0..steps.len()).find(|&k| {
            let s = &steps[k];
            (matches!(s.calc, Calc::Str | Calc::U8) && s.op.is_comparison())
                || matches!(s.calc, Calc::Ref | Calc::Callable)
                || big_cmp(k)
        }) {
            let rest = &steps[k + 1..];
            let acc = if rest.iter().any(|s| self.expr_sees_var(&s.rhs, dst)) {
                self.alloc(steps.last().map_or(first.ty, |s| s.ty))
            } else {
                dst
            };
            let lhs_ty = if k == 0 { first.ty } else { steps[k - 1].ty };
            let lhs = if k == 0 {
                self.expr_any(first)
            } else {
                let t = self.alloc(lhs_ty);
                let p = self.binary_lazy(first, &steps[..k], t);
                self.settle(t, p);
                t
            };
            let step = &steps[k];
            let old = self.set_line(step.span);
            let r = self.expr_any(&step.rhs);
            if step.calc == Calc::Set {
                // A small set on the left is widened to the big set on the right
                let lhs = if let Ty::Set(_) = lhs_ty {
                    let w = self.alloc(step.rhs.ty);
                    self.convert_set(lhs_ty, step.rhs.ty, false, w, lhs);
                    w
                } else {
                    lhs
                };
                self.set_compare(step.op, acc, lhs, r, true);
                self.line = old;
                let p = self.fold_steps(acc, rest, None);
                self.settle(acc, p);
                if acc != dst {
                    self.op2(op::MOVE, dst, acc);
                }
                return None;
            }
            if step.calc == Calc::Callable {
                // `docs/spec/objects.md` 9.5, 14.9
                let ty = step.rhs.ty;
                self.intrinsic_pair(op::intr::CALLABLE_EQUAL, acc, [(lhs, ty), (r, ty)]);
                if step.op == BinOp::Ne {
                    self.op2(op::NOT_BOOL, acc, acc);
                }
                self.line = old;
                let p = self.fold_steps(acc, rest, None);
                self.settle(acc, p);
                if acc != dst {
                    self.op2(op::MOVE, dst, acc);
                }
                return None;
            }
            // A dynamic array and `nil` compare the references (`docs/spec/types.md` 2.3)
            let (eq, cmp) = match step.calc {
                Calc::Ref => (op::EQ_INT, op::CMP_STR),
                Calc::U8 => (op::EQ_U8, op::CMP_U8),
                _ => (op::EQ_STR, op::CMP_STR),
            };
            match step.op {
                BinOp::Eq => self.op3(eq, acc, lhs, r),
                BinOp::Ne => {
                    self.op3(eq, acc, lhs, r);
                    self.op2(op::NOT_BOOL, acc, acc);
                }
                _ => {
                    self.op3(cmp, acc, lhs, r);
                    let z = self.alloc(Ty::Integer);
                    self.op1(op::LOAD_ZERO, z);
                    self.compare_int(step.op, acc, acc, z, false);
                }
            }
            self.line = old;
            let p = self.fold_steps(acc, rest, None);
            self.settle(acc, p);
            if acc != dst {
                self.op2(op::MOVE, dst, acc);
            }
            return None;
        }
        // A chain of three or more strings joined with `+`: made at once
        if steps.len() >= 2
            && steps
                .iter()
                .all(|s| s.calc == Calc::Str && s.op == BinOp::Add)
        {
            let operands: Vec<(&Expr, TextRange)> = std::iter::once((first, steps[0].span))
                .chain(steps.iter().map(|s| (&s.rhs, s.span)))
                .collect();
            self.concat_n(dst, &operands);
            return None;
        }
        // If a right operand reads the dst variable, accumulate in a temporary. A variable gets
        // here as `dst` only when `expr_into_var` found that the whole expression does not see
        // it, so this check (and the one in `binary_lazy`) is a guard that keeps the rule in one
        // place ([`Self::expr_sees_var`])
        let ty = steps.last().map_or(first.ty, |s| s.ty);
        let acc = if steps.iter().any(|s| self.expr_sees_var(&s.rhs, dst)) {
            self.alloc(ty)
        } else {
            dst
        };
        let p = self.expr_lazy(first, acc);
        let pending = self.fold_steps(acc, steps, p);
        if acc != dst {
            self.move_value(ty, dst, acc);
        }
        pending
    }

    /// `X1 + … + Xn` of strings (n ≥ 2) into `dst` with `ConcatStrN` (`docs/spec/bytecode.md`
    /// 2.4): each operand, evaluated left to right on the line of its operator, is borrowed into
    /// consecutive registers (a variable's register is moved unless an operand may write it), and
    /// they are joined at once. More operands than one instruction takes are joined in groups,
    /// each group's result the first operand of the next.
    fn concat_n(&mut self, dst: Reg, operands: &[(&Expr, TextRange)]) {
        let exprs: Vec<&Expr> = operands.iter().map(|(e, _)| *e).collect();
        let mut carried: Option<Reg> = None;
        let mut rest = operands;
        while !rest.is_empty() {
            let carry = usize::from(carried.is_some());
            let (group, tail) = rest.split_at((CONCAT_N_MAX - carry).min(rest.len()));
            let slots: Vec<Reg> = (0..carry + group.len())
                .map(|_| self.alloc_arg(Some(Ty::String)))
                .collect();
            if let Some(c) = carried {
                self.op2(op::MOVE, slots[0], c);
            }
            let mut copies = Vec::new();
            for (&(e, span), &slot) in group.iter().zip(&slots[carry..]) {
                let old = self.set_line(span);
                copies.extend(self.borrow_arg(e, slot, exprs.as_slice()));
                self.line = old;
            }
            let to = if tail.is_empty() {
                dst
            } else {
                self.alloc(Ty::String)
            };
            let old = self.set_line(group[0].1);
            let mut ops = vec![
                Operand::Reg(to),
                Operand::Reg(slots[0]),
                Operand::Imm(slots.len() as i64),
            ];
            ops.extend(slots.iter().map(|&r| Operand::Reg(r)));
            self.plain(op::CONCAT_STR_N, ops);
            self.let_go_copies(&copies);
            self.line = old;
            carried = Some(to);
            rest = tail;
        }
    }

    /// Applies the steps of a left fold to the accumulator `acc` (string comparisons are done by
    /// `binary_lazy`). `pending` is the accumulator's pending `Narrow` on entry; returns the one
    /// left on exit.
    fn fold_steps(&mut self, acc: Reg, steps: &[BinStep], mut pending: Pending) -> Pending {
        for step in steps {
            let old = self.set_line(step.span);
            // Only a wrapping integer operation reads an accumulator whose `Narrow` is pending
            let wraps = match step.calc {
                Calc::Int(calc) => int_op_wraps(step.op, calc, self.current_overflow),
                _ => None,
            };
            self.settle_unless(acc, pending, wraps);
            let acc_pending = pending.filter(|_| pending == wraps);
            pending = None;
            match step.calc {
                Calc::Bool => match step.op {
                    BinOp::And | BinOp::Or => {
                        let skip = self.new_block();
                        let jop = if step.op == BinOp::And {
                            op::JMP_IF_FALSE
                        } else {
                            op::JMP_IF_TRUE
                        };
                        self.branch(jop, acc, None, skip);
                        self.expr(&step.rhs, acc);
                        self.bind(skip);
                    }
                    BinOp::Xor => {
                        let r = self.expr_any(&step.rhs);
                        self.op3(op::XOR_INT, acc, acc, r);
                    }
                    _ => {
                        let r = self.expr_any(&step.rhs);
                        self.compare_int(step.op, acc, acc, r, false);
                    }
                },
                Calc::Char | Calc::Ord => {
                    let r = self.expr_any(&step.rhs);
                    self.compare_int(step.op, acc, acc, r, false);
                }
                Calc::Ref | Calc::Callable => unreachable!("`binary` compares references"),
                Calc::Set if step.op.is_comparison() => {
                    // Small sets (`binary` compares big ones)
                    let r = self.expr_any(&step.rhs);
                    self.set_compare(step.op, acc, acc, r, false);
                }
                Calc::Set => {
                    let r = self.expr_any(&step.rhs);
                    if let Ty::BigSet(_) = step.ty {
                        self.op_table(op::UNIQUE, acc, Operand::Type(step.ty));
                        let opc = match step.op {
                            BinOp::Add => op::SET_UNION,
                            BinOp::Sub => op::SET_DIFF,
                            _ => op::SET_INTER,
                        };
                        self.op2(opc, acc, r);
                    } else {
                        match step.op {
                            BinOp::Add => self.op3(op::OR_INT, acc, acc, r),
                            BinOp::Mul => self.op3(op::AND_INT, acc, acc, r),
                            _ => {
                                let t = self.alloc_borrow(Ty::Int64);
                                self.op2(op::NOT_INT, t, r);
                                self.op3(op::AND_INT, acc, acc, t);
                            }
                        }
                    }
                }
                Calc::SetIn => {
                    let r = self.expr_any(&step.rhs);
                    let opc = if let Ty::BigSet(_) = step.rhs.ty {
                        op::SET_IN_BIG
                    } else {
                        op::SET_IN
                    };
                    self.op3(opc, acc, acc, r);
                }
                Calc::Str => {
                    let r = self.expr_any(&step.rhs);
                    self.op3(op::CONCAT_STR, acc, acc, r);
                }
                Calc::U8 => {
                    let r = self.expr_any(&step.rhs);
                    self.op3(op::CONCAT_U8, acc, acc, r);
                }
                Calc::Arr => {
                    // `ArrConcat` takes the operands in consecutive registers and writes the new
                    // array over the borrowed left one; the accumulator lets go of its old value
                    let r = self.expr_any(&step.rhs);
                    let a = self.alloc_arg(Some(step.ty));
                    let b = self.alloc_arg(Some(step.rhs.ty));
                    self.op2(op::MOVE, a, acc);
                    self.op2(op::MOVE, b, r);
                    self.op_table(op::ARR_CONCAT, a, Operand::Type(step.ty));
                    self.op2(op::MOVE_REF, acc, a);
                }
                Calc::Int(calc) => {
                    let unsigned = calc == Ty::UInt64;
                    if step.op.is_comparison() {
                        let r = self.expr_any(&step.rhs);
                        self.compare_int(step.op, acc, acc, r, unsigned);
                    } else {
                        let (r, rp) = self.expr_any_lazy(&step.rhs);
                        let shift = matches!(step.op, BinOp::Shl | BinOp::Shr);
                        // The shift amount is read modulo the width: any pending `Narrow` will do
                        let r_pending = if shift && wraps.is_some() {
                            None
                        } else {
                            self.settle_unless(r, rp, wraps);
                            rp.filter(|_| rp == wraps)
                        };
                        let overflow = self.current_overflow;
                        let p = self.int_arith(step.op, calc, overflow, acc, acc, r);
                        pending = match step.op {
                            // Bitwise operations keep the operands' pending `Narrow`
                            BinOp::And | BinOp::Or | BinOp::Xor => acc_pending.or(r_pending),
                            _ => p,
                        };
                    }
                }
                Calc::Real(calc) => {
                    let r = self.expr_any(&step.rhs);
                    self.real_arith(step.op, calc, acc, acc, r);
                }
                Calc::Curr => {
                    let r = self.expr_any(&step.rhs);
                    if step.op.is_comparison() {
                        // The scaled integers compare like the values
                        self.compare_int(step.op, acc, acc, r, false);
                    } else {
                        let opc = match step.op {
                            BinOp::Add => op::ADD_CURR,
                            BinOp::Sub => op::SUB_CURR,
                            BinOp::Mul => op::MUL_CURR,
                            _ => op::DIV_CURR,
                        };
                        self.op3(opc, acc, acc, r);
                    }
                }
            }
            self.line = old;
        }
        pending
    }

    /// Puts the value of a comparison (`Boolean`) into `dst`.
    fn compare_int(&mut self, bop: BinOp, dst: Reg, a: Reg, b: Reg, unsigned: bool) {
        let (lt, le) = if unsigned {
            (op::LT_UINT, op::LE_UINT)
        } else {
            (op::LT_INT, op::LE_INT)
        };
        match bop {
            BinOp::Eq => self.op3(op::EQ_INT, dst, a, b),
            BinOp::Ne => {
                self.op3(op::EQ_INT, dst, a, b);
                self.op2(op::NOT_BOOL, dst, dst);
            }
            BinOp::Lt => self.op3(lt, dst, a, b),
            BinOp::Le => self.op3(le, dst, a, b),
            BinOp::Gt => self.op3(lt, dst, b, a),
            BinOp::Ge => self.op3(le, dst, b, a),
            _ => unreachable!("not a comparison"),
        }
    }

    /// A real operation or comparison in `Single` or `Double` (IEEE 754: a comparison with a NaN
    /// is false except `<>`, so `>` and `>=` swap the operands rather than negate).
    fn real_arith(&mut self, bop: BinOp, calc: Ty, dst: Reg, a: Reg, b: Reg) {
        let single = calc == Ty::Single;
        let pick = |d: u16, s: u16| if single { s } else { d };
        let (opc, a, b) = match bop {
            BinOp::Add => (pick(op::ADD_F64, op::ADD_F32), a, b),
            BinOp::Sub => (pick(op::SUB_F64, op::SUB_F32), a, b),
            BinOp::Mul => (pick(op::MUL_F64, op::MUL_F32), a, b),
            BinOp::Slash => (pick(op::DIV_F64, op::DIV_F32), a, b),
            BinOp::Eq | BinOp::Ne => (pick(op::EQ_F64, op::EQ_F32), a, b),
            BinOp::Lt => (pick(op::LT_F64, op::LT_F32), a, b),
            BinOp::Le => (pick(op::LE_F64, op::LE_F32), a, b),
            BinOp::Gt => (pick(op::LT_F64, op::LT_F32), b, a),
            BinOp::Ge => (pick(op::LE_F64, op::LE_F32), b, a),
            _ => unreachable!("not a real operation"),
        };
        self.op3(opc, dst, a, b);
        if bop == BinOp::Ne {
            self.op2(op::NOT_BOOL, dst, dst);
        }
    }

    /// An integer binary operation (the result is narrowed to the computation type). Under `{$Q-}`
    /// the `Narrow` of `+`, `-`, `*`, `div` and of a `Cardinal` shift is returned pending instead,
    /// except for `+`, `-` and `*` in `Integer`, whose 32-bit operations narrow the result
    /// themselves (`AddInt32` ...). `and`, `or` and `xor` of narrowed operands need no `Narrow`.
    fn int_arith(
        &mut self,
        bop: BinOp,
        calc: Ty,
        overflow: bool,
        dst: Reg,
        a: Reg,
        b: Reg,
    ) -> Pending {
        let is64 = matches!(calc, Ty::Int64 | Ty::UInt64);
        let unsigned = calc == Ty::UInt64;
        match bop {
            BinOp::Add | BinOp::Sub | BinOp::Mul if wraps32(calc, overflow) => {
                let opc = match bop {
                    BinOp::Add => op::ADD_INT32,
                    BinOp::Sub => op::SUB_INT32,
                    _ => op::MUL_INT32,
                };
                self.op3(opc, dst, a, b);
                None
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                let opc = match (bop, is64 && overflow, unsigned) {
                    (BinOp::Add, true, false) => op::ADD_INT_CHK,
                    (BinOp::Sub, true, false) => op::SUB_INT_CHK,
                    (BinOp::Mul, true, false) => op::MUL_INT_CHK,
                    (BinOp::Add, true, true) => op::ADD_UINT_CHK,
                    (BinOp::Sub, true, true) => op::SUB_UINT_CHK,
                    (BinOp::Mul, true, true) => op::MUL_UINT_CHK,
                    (BinOp::Add, false, _) => op::ADD_INT,
                    (BinOp::Sub, false, _) => op::SUB_INT,
                    _ => op::MUL_INT,
                };
                self.op3(opc, dst, a, b);
                if overflow {
                    self.narrow_arith(dst, calc, overflow);
                }
                arith_pending(calc, overflow)
            }
            BinOp::Div => {
                let opc = if unsigned {
                    op::DIV_UINT
                } else if calc == Ty::Int64 && overflow {
                    op::DIV_INT_CHK
                } else {
                    op::DIV_INT
                };
                self.op3(opc, dst, a, b);
                if overflow {
                    self.narrow_arith(dst, calc, overflow);
                }
                arith_pending(calc, overflow)
            }
            BinOp::Mod => {
                let opc = if unsigned { op::MOD_UINT } else { op::MOD_INT };
                self.op3(opc, dst, a, b);
                None
            }
            BinOp::And | BinOp::Or | BinOp::Xor => {
                let opc = match bop {
                    BinOp::And => op::AND_INT,
                    BinOp::Or => op::OR_INT,
                    _ => op::XOR_INT,
                };
                self.op3(opc, dst, a, b);
                None
            }
            BinOp::Shl | BinOp::Shr => {
                let left = bop == BinOp::Shl;
                if is64 {
                    self.op3(if left { op::SHL_INT } else { op::SHR_INT }, dst, a, b);
                    None
                } else {
                    // The result is sign-extended (`Integer`); a `Cardinal` one is zero-extended
                    self.op3(if left { op::SHL_INT32 } else { op::SHR_INT32 }, dst, a, b);
                    if calc != Ty::Cardinal {
                        return None;
                    }
                    let p = arith_pending(calc, overflow);
                    if p.is_none() {
                        self.op_imm(op::NARROW, dst, dst, 5);
                    }
                    p
                }
            }
            _ => unreachable!("not an integer operation"),
        }
    }

    /// A register holding the expression's value, with its pending `Narrow` (`expr_lazy`).
    fn expr_any_lazy(&mut self, e: &Expr) -> (Reg, Pending) {
        if e.value.is_none()
            && let ExprKind::Var(v) = &e.kind
            && !self.is_ref_param(*v)
        {
            return (self.reg_of(*v), None);
        }
        let t = self.alloc(e.ty);
        let p = self.expr_lazy(e, t);
        self.let_go_at_statement_end(e, t);
        (t, p)
    }

    /// Conditional branch: jumps to `target` when the expression equals `jump_if`.
    fn cond(&mut self, e: &Expr, target: BlockId, jump_if: bool) {
        let old = self.set_line(e.span);
        let mark = self.mark();
        if let Some(Const::Bool(b)) = &e.value {
            if *b == jump_if {
                self.jump(target);
            }
            self.release(mark);
            self.line = old;
            return;
        }
        match &e.kind {
            // A constant once the type is known: no instruction (`docs/spec/bytecode.md` 2.23)
            ExprKind::Builtin {
                func: func @ (Builtin::IsPlainKey | Builtin::IsPlainOrder),
                args,
            } => {
                if self.is_plain(*func, args[0].ty) == jump_if {
                    self.jump(target);
                }
                self.release(mark);
                self.line = old;
            }
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
                calc: Calc::Bool,
            } => self.cond(operand, target, !jump_if),
            ExprKind::Binary { first, steps }
                if steps.last().is_some_and(|s| {
                    s.op.is_comparison()
                        && !matches!(s.calc, Calc::Set | Calc::Real(_) | Calc::Callable)
                }) =>
            {
                // If the last step is a comparison, compute its left side (`X and 1` in `X and 1 = 0`) first
                // so that the comparison and branch become one instruction
                let step = steps.last().expect("checked by the guard");
                let a = if steps.len() == 1 {
                    self.expr_any(first)
                } else {
                    let prefix = &steps[..steps.len() - 1];
                    let last = prefix.last().expect("at least two steps");
                    let lhs = Expr {
                        kind: ExprKind::Binary {
                            first: first.clone(),
                            steps: prefix.to_vec(),
                        },
                        ty: last.ty,
                        value: None,
                        span: TextRange::new(first.span.start(), last.rhs.span.end()),
                    };
                    self.expr_any(&lhs)
                };
                let b = self.expr_any(&step.rhs);
                let old2 = self.set_line(step.span);
                match step.calc {
                    Calc::Str if matches!(step.op, BinOp::Eq | BinOp::Ne) => {
                        let jop = if (step.op == BinOp::Eq) == jump_if {
                            op::JMP_IF_EQ_STR
                        } else {
                            op::JMP_IF_NE_STR
                        };
                        self.branch(jop, a, Some(b), target);
                    }
                    Calc::Str | Calc::U8 => {
                        let cmp = if step.calc == Calc::U8 {
                            op::CMP_U8
                        } else {
                            op::CMP_STR
                        };
                        let t = self.alloc(Ty::Integer);
                        self.op3(cmp, t, a, b);
                        let z = self.alloc(Ty::Integer);
                        self.op1(op::LOAD_ZERO, z);
                        self.fused_compare(step.op, t, z, false, target, jump_if);
                    }
                    Calc::Int(c) => {
                        self.fused_compare(step.op, a, b, c == Ty::UInt64, target, jump_if)
                    }
                    Calc::Curr => self.fused_compare(step.op, a, b, false, target, jump_if),
                    Calc::Bool | Calc::Char | Calc::Ord | Calc::Ref => {
                        self.fused_compare(step.op, a, b, false, target, jump_if)
                    }
                    Calc::Set | Calc::SetIn | Calc::Real(_) | Calc::Callable => {
                        unreachable!("the guard leaves out sets, reals and callable values")
                    }
                    Calc::Arr => unreachable!("arrays are not compared"),
                }
                self.line = old2;
            }
            ExprKind::Binary { first, steps }
                if !steps.is_empty()
                    && steps
                        .iter()
                        .all(|s| s.calc == Calc::Bool && s.op == BinOp::And) =>
            {
                if !jump_if {
                    self.cond(first, target, false);
                    for s in steps {
                        self.cond(&s.rhs, target, false);
                    }
                } else {
                    let skip = self.new_block();
                    self.cond(first, skip, false);
                    for (i, s) in steps.iter().enumerate() {
                        if i + 1 < steps.len() {
                            self.cond(&s.rhs, skip, false);
                        } else {
                            self.cond(&s.rhs, target, true);
                        }
                    }
                    self.bind(skip);
                }
            }
            ExprKind::Binary { first, steps }
                if !steps.is_empty()
                    && steps
                        .iter()
                        .all(|s| s.calc == Calc::Bool && s.op == BinOp::Or) =>
            {
                if jump_if {
                    self.cond(first, target, true);
                    for s in steps {
                        self.cond(&s.rhs, target, true);
                    }
                } else {
                    let skip = self.new_block();
                    self.cond(first, skip, true);
                    for (i, s) in steps.iter().enumerate() {
                        if i + 1 < steps.len() {
                            self.cond(&s.rhs, skip, true);
                        } else {
                            self.cond(&s.rhs, target, false);
                        }
                    }
                    self.bind(skip);
                }
            }
            _ => {
                let t = self.expr_any(e);
                let jop = if jump_if {
                    op::JMP_IF_TRUE
                } else {
                    op::JMP_IF_FALSE
                };
                self.branch(jop, t, None, target);
            }
        }
        self.release(mark);
        self.line = old;
    }

    /// A fused compare-and-branch.
    fn fused_compare(
        &mut self,
        bop: BinOp,
        a: Reg,
        b: Reg,
        unsigned: bool,
        target: BlockId,
        jump_if: bool,
    ) {
        // If jump_if is false, branch on the negated condition
        let (bop, a, b) = if jump_if {
            (bop, a, b)
        } else {
            match bop {
                BinOp::Eq => (BinOp::Ne, a, b),
                BinOp::Ne => (BinOp::Eq, a, b),
                BinOp::Lt => (BinOp::Le, b, a),
                BinOp::Le => (BinOp::Lt, b, a),
                BinOp::Gt => (BinOp::Le, a, b),
                BinOp::Ge => (BinOp::Lt, a, b),
                _ => unreachable!("not a comparison"),
            }
        };
        let (lt, le) = if unsigned {
            (op::JMP_IF_LT_UINT, op::JMP_IF_LE_UINT)
        } else {
            (op::JMP_IF_LT_INT, op::JMP_IF_LE_INT)
        };
        let (opc, a, b) = match bop {
            BinOp::Eq => (op::JMP_IF_EQ_INT, a, b),
            BinOp::Ne => (op::JMP_IF_NE_INT, a, b),
            BinOp::Lt => (lt, a, b),
            BinOp::Le => (le, a, b),
            BinOp::Gt => (lt, b, a),
            BinOp::Ge => (le, b, a),
            _ => unreachable!("not a comparison"),
        };
        self.branch(opc, a, Some(b), target);
    }

    // ---- Calls ----

    /// A routine call. `dst` is `None` for a procedure called as a statement.
    ///
    /// Arguments go to consecutive temporaries at the top of the frame (`docs/design/lifetime.md` 3.3):
    /// a managed value or `const` argument is borrowed there with `Move` (a callee writing a value
    /// parameter counts it when entered), and an `out` argument of a managed type is cleared
    /// before the call.
    ///
    /// A method gets the receiver (`Self`) as its first argument; `dispatch` says how the call
    /// reaches it (`docs/spec/objects.md` 14.3).
    fn call_routine(
        &mut self,
        routine: RoutineId,
        args: &[CallArg],
        dispatch: Dispatch,
        dst: Option<Reg>,
    ) {
        self.call_routine_with(routine, args, dispatch, dst, None);
    }

    /// [`Lowerer::call_routine`]; with `new` (the class and the register of the new instance),
    /// the call of a constructor in `TFoo.Create(…)`: after the arguments, `New` puts the instance
    /// in the register and the `try` of the creation begins (returned), and the instance is
    /// moved to `Self` (the first argument, which is not evaluated). Also returns the
    /// temporaries of the `CallArg::Temp` arguments, in order (the value of a record constructor
    /// is the first; `docs/spec/objects.md` chapter 6).
    fn call_routine_with(
        &mut self,
        routine: RoutineId,
        args: &[CallArg],
        dispatch: Dispatch,
        dst: Option<Reg>,
        new: Option<NewIn>,
    ) -> (Option<FinallyScope>, Vec<Reg>) {
        let callee = &self.program.routines[routine.0 as usize];
        let modes: Vec<Option<ParamMode>> = (0..args.len())
            .map(|i| match callee.vars.get(i).map(|v| v.kind) {
                Some(VarKind::Param(m)) => Some(m),
                Some(VarKind::RecordSelf) => Some(ParamMode::Var),
                _ => None,
            })
            .collect();
        let param_tys: Vec<Ty> = (0..args.len())
            .map(|i| callee.vars.get(i).map_or(Ty::Error, |v| v.ty))
            .collect();
        let ret = callee.ret;
        // A nested routine taking the static link gets it first: a reference to the frame of the
        // routine it is nested in, this one or one `up` levels out (`docs/spec/semantics.md` 4.9)
        // A closure-form nested routine gets its group's block first (`docs/spec/objects.md`
        // 14.9). Either is the register `eval_args` puts before the arguments, so that the
        // registers of a lent argument do not come between it and them.
        let link = callee.link.then(|| {
            let parent = callee
                .parent
                .expect("a routine with a static link is nested");
            let mut chain =
                std::iter::successors(self.id, |r| self.program.routines[r.0 as usize].parent);
            chain
                .position(|r| r == parent)
                .expect("a nested routine is called from inside the routine it is nested in")
                as i64
        });
        let block = match dispatch {
            Dispatch::Group(b) => Some(b),
            _ => None,
        };
        let self_first = matches!(
            callee.vars.first().map(|v| v.kind),
            Some(VarKind::SelfParam)
        );
        // Both would be written to the one leading register
        debug_assert!(
            !(link.is_some() && block.is_some()),
            "a closure-form routine takes its block instead of the static link"
        );
        let lead = link.is_some() || block.is_some();
        let laid = self.eval_args(
            args,
            &modes,
            &param_tys,
            new.is_some(),
            self_first,
            ret,
            lead,
        );
        let (mut slots, temps, lent) = (laid.slots, laid.temps, laid.lent);
        let frame = slots.first().copied();
        if let Some(up) = link {
            let t = slots[0];
            self.plain(
                op::OUTER_REF,
                vec![Operand::Reg(t), Operand::Imm(0), Operand::Imm(up)],
            );
        }
        if let Some(b) = block {
            self.load_block(b, slots[0]);
        }
        slots.truncate(args.len() + usize::from(lead));
        let frame = frame.expect("a call has a frame");
        let scope = new.map(|n| {
            self.new_in(&n);
            let scope = self.finally_begin();
            self.op2(op::MOVE, frame, n.inst);
            scope
        });
        let (opc, callee) = match dispatch {
            Dispatch::Routine | Dispatch::Group(_) => (
                op::CALL,
                Operand::Func(self.ml.func_index[routine.0 as usize]),
            ),
            Dispatch::Method | Dispatch::Inherited => (
                op::CALL_METHOD,
                Operand::Func(self.ml.func_index[routine.0 as usize]),
            ),
            Dispatch::Virtual(index) => (
                op::CALL_VIRTUAL,
                Operand::Virtual {
                    index,
                    name: self.ml.func_names[routine.0 as usize].clone(),
                },
            ),
            Dispatch::VirtualClass(index) => (
                op::CALL_VIRTUAL_CLASS,
                Operand::Virtual {
                    index,
                    name: self.ml.func_names[routine.0 as usize].clone(),
                },
            ),
            Dispatch::Intf(intf, method) => {
                // `docs/spec/objects.md` 15.4: the interface the receiver's type names
                let full = &self.ml.func_names[routine.0 as usize];
                let m = full.rsplit('.').next().unwrap_or(full);
                let name = format!("{}.{m}", self.program.types.full_name(Ty::Intf(intf)));
                (
                    op::CALL_INTF,
                    Operand::Intf {
                        intf: Ty::Intf(intf),
                        method,
                        name,
                    },
                )
            }
        };
        match (dst, ret) {
            // The result comes back in the frame; move its reference to the destination
            (Some(d), Some(rt)) if self.managed(rt) => {
                self.call(opc, Some(frame), callee, slots, frame);
                self.move_value(rt, d, frame);
            }
            _ => self.call(opc, dst, callee, slots, frame),
        }
        if lent {
            self.plain(op::UNLEND, Vec::new());
        }
        (scope, temps)
    }

    /// Evaluates the arguments of a call into consecutive call-argument temporaries
    /// (`docs/design/lifetime.md` 3.3): managed value and `const` ones (and `Self` with
    /// `self_first`) borrowed, others made there, `var` / `out` ones as reference slots, a field or element
    /// lent (`Lend`) after every argument is evaluated. `modes` and `param_tys` are the
    /// parameters'; `skip_first` leaves the first argument for the caller (the new instance of a
    /// constructor); `lead` puts a register before the arguments (the frame of `CallClosure`,
    /// the static link of a nested routine, or the block of a closure-form one), after the
    /// registers of the lent arguments and the copies.
    #[allow(clippy::too_many_arguments)]
    fn eval_args(
        &mut self,
        args: &[CallArg],
        modes: &[Option<ParamMode>],
        param_tys: &[Ty],
        skip_first: bool,
        self_first: bool,
        ret: Option<Ty>,
        lead: bool,
    ) -> LaidArgs {
        // A field or element passed to a `var` / `out` parameter is lent to the call (`Lend`) after
        // every argument is evaluated, and given back after it (`Unlend`; `docs/spec/types.md`
        // chapter 3). Its registers come before the arguments, so the call does not overwrite them.
        let mut lends: Vec<Option<Lending>> = args
            .iter()
            .enumerate()
            .map(|(i, a)| match a {
                CallArg::Ref { place, .. } if is_lent(place) => {
                    // A static array is passed as a block to a `var` open array parameter, and
                    // to a `var` static array parameter, so that a static array parameter is never
                    // a lent flat place (`FuncLower::place_ref`)
                    let open = matches!(param_tys.get(i), Some(Ty::OpenArray(_) | Ty::Array(_)));
                    Some(self.alloc_lending(place, open))
                }
                _ => None,
            })
            .collect();
        // The copy a `CallArg::Temp` passes by reference: before the arguments too, so the callee's
        // frame (which starts at the arguments) does not overlap it
        let copies: Vec<Option<Reg>> = args
            .iter()
            .map(|a| match a {
                CallArg::Temp(e) => Some(self.alloc(e.ty)),
                _ => None,
            })
            .collect();
        // The arguments are laid out in consecutive temporaries; with no arguments the frame still gets one
        let mut slots = Vec::with_capacity(args.len().max(1) + 1);
        if lead {
            slots.push(self.alloc_kind(None, TempKind::Ref));
        }
        for a in args {
            let t = match a {
                CallArg::Value(e) => self.alloc_arg(Some(e.ty)),
                CallArg::Ref { .. } | CallArg::Temp(_) => self.alloc_kind(None, TempKind::Ref),
            };
            slots.push(t);
        }
        if slots.is_empty() {
            slots.push(self.alloc_arg(ret));
        }
        let mut temps = Vec::new();
        for (i, (a, &reg)) in args.iter().zip(&slots[usize::from(lead)..]).enumerate() {
            if i == 0 && skip_first {
                continue;
            }
            // A static array passed to an open array parameter is made a block first: the callee
            // cannot know the length of a default value, 0 (`docs/spec/types.md` chapter 9)
            let open_static = match (a, param_tys[i]) {
                (CallArg::Value(e), Ty::OpenArray(_)) if matches!(e.ty, Ty::Array(_)) => Some(e.ty),
                (CallArg::Ref { place, .. }, Ty::OpenArray(_))
                    if matches!(self.place_type(place), Ty::Array(_)) =>
                {
                    Some(self.place_type(place))
                }
                _ => None,
            };
            match a {
                CallArg::Value(e) if open_static.is_some() => {
                    let ty = e.ty;
                    if lends_argument(modes[i]) {
                        let t = self.alloc(ty);
                        self.expr(e, t);
                        self.op_table(op::MATERIALIZE, t, Operand::Type(ty));
                        self.op2(op::MOVE, reg, t);
                    } else {
                        self.value_arg(e, reg);
                        self.op_table(op::MATERIALIZE, reg, Operand::Type(ty));
                    }
                }
                // A value or `const` argument is lent (`docs/design/lifetime.md` 3.3), `Self` too
                // (`docs/spec/objects.md` 4.1)
                CallArg::Value(e)
                    if self.managed(e.ty)
                        && (lends_argument(modes[i]) || (i == 0 && self_first)) =>
                {
                    self.lend_arg(e, reg, args)
                }
                CallArg::Value(e) => self.value_arg(e, reg),
                CallArg::Ref { place, .. } if is_lent(place) => {
                    let lending = lends[i].as_mut().expect("a lent place has its registers");
                    self.eval_lending(place, lending);
                }
                CallArg::Ref { place, is_out } => {
                    self.make_ref(place, reg, open_static);
                    if *is_out {
                        self.clear_ref(self.place_type(place), reg);
                    }
                }
                CallArg::Temp(e) => {
                    // A copy the callee writes to (`docs/spec/objects.md` chapter 6)
                    let t = copies[i].expect("a `Temp` argument has its copy");
                    self.expr(e, t);
                    self.op2(op::MAKE_REF, reg, t);
                    temps.push(t);
                }
            }
        }
        // The containers are opened for writing before anything is taken out, so that taking a
        // location out twice is reported as such
        for (a, lending) in args.iter().zip(&lends) {
            if let (CallArg::Ref { place, .. }, Some(lending)) = (a, lending) {
                self.open_lending(place, lending);
            }
        }
        for (i, (a, &reg)) in args.iter().zip(&slots[usize::from(lead)..]).enumerate() {
            let (Some(lending), CallArg::Ref { place, is_out }) = (&lends[i], a) else {
                continue;
            };
            self.lend(lending);
            if let Some(b) = lending.boxed {
                // A flat static array passed to a `var` open array parameter, which reads its
                // argument as a block, is taken out into one (`docs/spec/bytecode.md` 2.8)
                self.intrinsic(op::intr::PLACE_BOX, &[b, lending.value]);
                self.op2(op::MAKE_REF, reg, b);
            } else if lending.place.is_some() {
                // The reference to the lent flat place is passed
                self.op2(op::MOVE, reg, lending.value);
            } else {
                // A static array passed to a `var` open array parameter becomes a block (the
                // callee cannot know the length of a default value, 0)
                if let Ty::OpenArray(_) = param_tys[i]
                    && let ty @ Ty::Array(_) = self.place_type(place)
                {
                    self.op_table(op::UNIQUE, lending.value, Operand::Type(ty));
                }
                self.op2(op::MAKE_REF, reg, lending.value);
            }
            if *is_out {
                self.clear_ref(self.place_type(place), reg);
            }
        }
        LaidArgs {
            slots,
            temps,
            lent: lends.iter().any(Option::is_some),
        }
    }

    /// A method reference into `dst` (`docs/spec/objects.md` 9.3, 14.9): `MethodRef` /
    /// `VirtualRef` / `ClassMethodRef` / `ClassVirtualRef` with the receiver (borrowed) and the
    /// function (its index, or the entry of the class's table) in consecutive registers.
    fn method_ref(&mut self, base: &Expr, routine: RoutineId, how: MethodRefHow, dst: Reg) {
        let recv = self.alloc_arg(Some(base.ty));
        let f = self.alloc_arg(Some(Ty::Int64));
        // The receiver is only read (the value holds its instance weakly): a variable or a field
        // is not counted, so making the value does not keep the instance alive
        let b = self.borrowed_value(base);
        self.op2(op::MOVE, recv, b);
        let (id, entry) = match how {
            MethodRefHow::Method => (op::intr::METHOD_REF, None),
            MethodRefHow::Virtual(k) => (op::intr::VIRTUAL_REF, Some(k)),
            MethodRefHow::Class => (op::intr::CLASS_METHOD_REF, None),
            MethodRefHow::ClassVirtual(k) => (op::intr::CLASS_VIRTUAL_REF, Some(k)),
        };
        match entry {
            // A virtual method's entry in the table
            Some(k) => self.load_int(f, i64::from(k)),
            None => self.load_func(f, self.ml.func_index[routine.0 as usize], 0),
        }
        self.intrinsic(id, &[dst, recv, f]);
    }

    /// A call of a procedural value (`docs/spec/objects.md` 9.4, 14.9): `CallIndirect` with the
    /// arguments from the frame, or `CallClosure` with the frame before them. The value is
    /// evaluated first; a counted one that the call could let go of (anything but a local
    /// variable nothing else writes) is held in a temporary of its own while the call runs.
    fn call_value(&mut self, callee: &Expr, args: &[CallArg], ret: Option<Ty>, dst: Option<Reg>) {
        let closure = matches!(callee.ty, Ty::Callable(_));
        let (modes, param_tys): (Vec<Option<ParamMode>>, Vec<Ty>) = self
            .program
            .types
            .proc_sig(callee.ty)
            .map(|(_, ps, _)| ps.iter().map(|p| (Some(p.mode), p.ty)).unzip())
            .unwrap_or_default();
        let value = match &callee.kind {
            ExprKind::Var(v)
                if closure
                    && callee.value.is_none()
                    && !self.is_ref_param(*v)
                    && !args.writes(*v)
                    && !self.r.vars[v.0 as usize].captured =>
            {
                self.reg_of(*v)
            }
            _ => {
                let t = self.alloc(callee.ty);
                self.expr(callee, t);
                t
            }
        };
        let laid = self.eval_args(args, &modes, &param_tys, false, false, ret, closure);
        let mut slots = laid.slots;
        let frame = *slots.first().expect("a call has a frame");
        if closure {
            slots.remove(0);
        }
        slots.truncate(args.len());
        let opc = if closure {
            op::CALL_CLOSURE
        } else {
            op::CALL_INDIRECT
        };
        let callee_op = Operand::Reg(value);
        match (dst, ret) {
            (Some(d), Some(rt)) if self.managed(rt) => {
                self.call(opc, Some(frame), callee_op, slots, frame);
                self.move_value(rt, d, frame);
            }
            _ => self.call(opc, dst, callee_op, slots, frame),
        }
        if laid.lent {
            self.plain(op::UNLEND, Vec::new());
        }
    }

    /// `FreeAndNil(X)` (`docs/spec/objects.md` 3.2): `X` is copied to a temporary and set to
    /// `nil`, then the copy is freed, so the destructor reads `nil` in `X`. A field or element
    /// is not lent: its path is evaluated once, read and written before `Free`.
    fn free_and_nil(&mut self, target: &Place) {
        let ty = self.place_type(target);
        let t = self.alloc_borrow(ty);
        let z = self.alloc_borrow(ty);
        self.op1(op::LOAD_ZERO, z);
        match target {
            Place::Var(..) | Place::Global(_) => {
                let r = self.root_borrowed(target);
                self.op2(op::MOVE, t, r);
                self.store_place(target, z);
            }
            Place::ObjField { base, slot, .. } => {
                let part = self.eval_part(target);
                let h = part.handle.expect("an instance's field has its handle");
                let s = self.obj_slot(base.ty, *slot);
                self.obj_get(t, h, s, ty, false);
                self.obj_set(h, s, z, ty, false);
            }
            _ => {
                let part = self.eval_part(target);
                self.part_check(&part);
                self.part_load(&part, t);
                self.part_store(&part, z);
            }
        }
        self.intrinsic(op::intr::FREE, &[t]);
    }

    /// `Lend` of a place whose registers `eval_lending` filled.
    fn lend(&mut self, lending: &Lending) {
        let mut operands = vec![
            Operand::Reg(lending.value),
            Operand::Reg(lending.root),
            Operand::Imm(lending.path.len() as i64),
        ];
        operands.extend(lending.path.iter().map(|p| Operand::Reg(*p)));
        match lending.place {
            // A flat place is lent where it is (`docs/design/records.md` 3.7)
            Some(ty) => {
                operands.push(Operand::Type(ty));
                self.plain(op::LEND_PLACE, operands);
            }
            None => self.plain(op::LEND, operands),
        }
    }

    /// A place with its steps' indices replaced by placeholders: the hops of a place by its
    /// types alone ([`FuncLower::hop_list`]), as the lending registers are made before the
    /// indices are evaluated.
    fn shape_part<'p>(&self, place: &'p Place) -> Part<'p> {
        match place {
            Place::Field {
                base, index, ty, ..
            } => {
                let mut part = self.shape_part(base);
                part.steps.push(PartStep {
                    slot: SlotRef::Field(*index),
                    ty: *ty,
                });
                part
            }
            Place::Elem { base, ty, .. } => {
                let mut part = self.shape_part(base);
                let slot = if self.place_type(base).is_dyn_indexed() {
                    SlotRef::Dyn(Reg::Link)
                } else {
                    SlotRef::Elem {
                        index: Idx::Const(0),
                        check: None,
                    }
                };
                part.steps.push(PartStep { slot, ty: *ty });
                part
            }
            Place::ObjField { .. } => Part {
                root: place,
                steps: Vec::new(),
                handle: Some(Reg::Link),
                root_reg: None,
            },
            _ => Part {
                root: place,
                steps: Vec::new(),
                handle: None,
                root_reg: None,
            },
        }
    }

    /// The number of steps of the lending path into each hop (`docs/spec/bytecode.md` 2.8): a slot
    /// number in a record or instance; an element index in an array, followed by the slot number
    /// in the element when the elements take more than one slot and the place is inside one.
    fn hop_path_len(&self, hops: &[Hop], k: usize, nsteps: usize) -> usize {
        let h = &hops[k];
        if h.elem.is_none() {
            return 1;
        }
        let end = hops.get(k + 1).map_or(nsteps, |n| n.start);
        1 + usize::from(h.width > 1 && end > h.start + 1)
    }

    /// The registers lending a field or element to a call (`Lend` / `LendPlace`): the value taken
    /// out (or the reference to the flat place) and the path after it, and the root (a reference
    /// slot, or an instance's handle whose slot number starts the path; `docs/spec/objects.md`
    /// 14.3), all before the call's arguments. `open` is for a `var` open array or static array
    /// parameter: a flat static array is taken out into a block (`PlaceBox`) in a register of its
    /// own.
    fn alloc_lending(&mut self, place: &Place, open: bool) -> Lending {
        let ty = self.place_type(place);
        let flat = self.is_flat(ty);
        let value = self.alloc_kind((!flat).then_some(ty), TempKind::Lent);
        let root_place = part_root(place);
        let shape = self.shape_part(place);
        let hops = self.hop_list(&shape);
        let extra = matches!(root_place, Place::ObjField { ty, .. } if !self.is_flat(*ty));
        let len = usize::from(extra)
            + (0..hops.len())
                .map(|k| self.hop_path_len(&hops, k, shape.steps.len()))
                .sum::<usize>();
        let path = (0..len).map(|_| self.alloc_borrow(Ty::Int64)).collect();
        let root = match root_place {
            Place::Var(v, _) if self.is_ref_param(*v) => self.reg_of(*v),
            Place::ObjField { base, .. } => self.alloc_borrow(base.ty),
            _ => self.alloc_kind(None, TempKind::Ref),
        };
        let boxed = (open && flat && matches!(ty, Ty::Array(_)))
            .then(|| self.alloc_kind(Some(ty), TempKind::Lent));
        Lending {
            value,
            root,
            path,
            hops: Vec::new(),
            place: flat.then_some(ty),
            boxed,
        }
    }

    /// Evaluates the indices of a lent place in its turn among the arguments: the path to it is
    /// computed into the lending path, which `Unlend` follows again after the call, and the static
    /// array indices are checked.
    fn eval_lending(&mut self, place: &Place, lending: &mut Lending) {
        let part = self.eval_part(place);
        self.part_check(&part);
        let hops = self.hop_list(&part);
        let mut p = 0;
        // An instance's field: the slot number comes first on the path
        if let (Place::ObjField { base, slot, ty, .. }, Some(h)) = (part.root, part.handle) {
            if !self.is_flat(*ty) {
                self.load_int(lending.path[0], i64::from(self.obj_slot(base.ty, *slot)));
                p = 1;
            }
            self.op2(op::MOVE, lending.root, h);
        }
        for (k, h) in hops.iter().enumerate() {
            let n = self.hop_path_len(&hops, k, part.steps.len());
            let mut out = h.clone();
            let at = lending.path[p];
            match h.elem {
                None => {
                    // The slot number: the hop's address
                    if h.is_static() {
                        self.load_int(at, i64::from(h.off));
                    } else {
                        self.sum_into(at, &h.terms, h.off);
                        out.off = 0;
                        out.terms = vec![(at, 1)];
                    }
                }
                Some(i) => {
                    match i {
                        Idx::Const(c) => self.load_int(at, c),
                        Idx::Reg(r) => {
                            self.op2(op::MOVE, at, r);
                            out.elem = Some(Idx::Reg(at));
                        }
                    }
                    if n == 2 {
                        let inner = lending.path[p + 1];
                        if h.terms.is_empty() {
                            self.load_int(inner, i64::from(h.off));
                        } else {
                            self.sum_into(inner, &h.terms, h.off);
                            out.off = 0;
                            out.terms = vec![(inner, 1)];
                        }
                    }
                }
            }
            p += n;
            lending.hops.push(out);
        }
        match part.root {
            Place::Var(v, _) if self.is_ref_param(*v) => {}
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                self.op2(op::MAKE_REF, lending.root, r);
            }
            Place::Global(g) => {
                self.op_table(op::MAKE_GLOBAL_REF, lending.root, Operand::Global(g.0))
            }
            Place::ObjField { .. } => {}
            _ => unreachable!("the root of a place is a variable, a global or an instance's field"),
        }
    }

    /// Opens the containers of a lent place for writing (`Lend` takes the value out of a unique
    /// record or static array, `LendPlace` lends a place in one). When a record or static array
    /// comes before the innermost dynamic array, that array's index is checked here, so `Lend`
    /// meets a default container only as an empty dynamic array.
    fn open_lending(&mut self, place: &Place, lending: &Lending) {
        let root = part_root(place);
        let hops = &lending.hops;
        let last_dyn = hops.iter().rposition(|h| h.kind == HopKind::Dyn);
        let check = last_dyn.is_some_and(|k| hops[..k].iter().any(|h| h.kind != HopKind::Dyn));
        // An element of a dynamic array reached through dynamic arrays only needs nothing here,
        // and an instance is shared
        if hops.is_empty()
            || !check && last_dyn == Some(hops.len() - 1)
            || hops.len() == 1 && hops[0].kind == HopKind::Obj
        {
            return;
        }
        let part = Part {
            root,
            handle: matches!(root, Place::ObjField { .. }).then_some(lending.root),
            steps: Vec::new(),
            root_reg: None,
        };
        let open = |this: &mut Self, part: &Part<'_>| {
            let opened = this.open_hops(part, hops);
            // A record or static array reached through a reference slot alone is opened by
            // writing a slot of it back (`RefFieldSet`)
            if hops.len() == 1 && hops[0].kind == HopKind::Ref {
                let At::Field(s) = this.hop_slot(&hops[0]) else {
                    unreachable!("a reference slot's hop has a constant address")
                };
                let (r, ty) = (opened.inner, hops[0].val);
                let t = this.alloc_borrow(ty);
                this.ref_field_get(t, r, s, ty, false);
                this.ref_field_set(r, hops[0].ty, s, t, ty, false);
            }
            if check && let Some((k, block)) = opened.dyn_root {
                let t = this.alloc_borrow(hops[k].val);
                this.hop_get(&hops[k], block, t, false);
            }
        };
        if self.needs_ref_copy(&part, hops) {
            self.with_ref_copy(&part, true, open);
        } else {
            open(self, &part);
        }
    }

    /// Puts the value of an argument into a call argument temporary. A managed value is made there
    /// from 0 (the temporary may hold an integer or a borrowed reference from an earlier call).
    fn value_arg(&mut self, e: &Expr, slot: Reg) {
        if self.managed(e.ty) {
            self.op1(op::LOAD_ZERO, slot);
            let empty = matches!(&e.value, Some(Const::Str(s)) if s.is_empty());
            if empty {
                return;
            }
        }
        self.expr(e, slot);
    }

    /// Puts a borrowed managed value into a register of a built-in instruction's range
    /// (`docs/design/lifetime.md` 3.3). A local variable that nothing in `siblings` can write is
    /// lent as it is; anything else is first copied (counted) into an owning temporary, which
    /// lends it. Returns that temporary when it holds a copy of a variable (a global, a field, an
    /// element) holding no object: the caller lets go of it right after the instruction
    /// ([`FuncLower::let_go_copies`]), unobserved. Held to the end of the routine, it would make
    /// the next `T := T + X` or element write of that variable copy the whole value.
    fn borrow_arg(
        &mut self,
        e: &Expr,
        slot: Reg,
        siblings: &(impl WritesVar + ?Sized),
    ) -> Option<Reg> {
        let t = self.borrow_arg_copy(e, slot, siblings)?;
        self.copy_of_place_let_go(e).then_some(t)
    }

    /// Lets go of the copies [`FuncLower::borrow_arg`] returned, once the instruction reading
    /// them has run (a dynamic array's is no longer left to the end of the statement).
    fn let_go_copies(&mut self, copies: &[Reg]) {
        self.owned.retain(|r| !copies.contains(r));
        self.emit_releases(copies);
    }

    /// Whether a counted copy of `e` lent to a call or an instruction may be let go of before the
    /// routine ends: `e` reads a variable (a global, a field, an element) and its type holds no
    /// object, so letting go of the copy runs no destructor (`docs/design/lifetime.md` 3.3).
    fn copy_of_place_let_go(&self, e: &Expr) -> bool {
        e.value.is_none()
            && matches!(
                e.kind,
                ExprKind::Var(_)
                    | ExprKind::Global(_)
                    | ExprKind::Field { .. }
                    | ExprKind::Elem { .. }
                    | ExprKind::ObjField { .. }
            )
            && !self.program.types.holds_object(e.ty)
    }

    /// [`FuncLower::borrow_arg`] of a call argument: a copy of a variable (a global, a field, an
    /// element) holding no object is let go of when the statement ends, unobserved. Held to the
    /// end of the routine, it would make the next `T := T + X` or element write of that variable
    /// copy the whole value (`docs/design/lifetime.md` 3.3, 3.4).
    fn lend_arg(&mut self, e: &Expr, slot: Reg, siblings: &(impl WritesVar + ?Sized)) {
        if let Some(t) = self.borrow_arg_copy(e, slot, siblings) {
            self.let_go_at_statement_end(e, t);
        }
    }

    /// Has the owning temporary `t`, which holds the value of `e`, let go of when the statement
    /// ends (or, in the condition of `if` / `while`, before the body runs) if it is a counted
    /// copy of a variable (a global, a field, an element) holding no object
    /// ([`FuncLower::copy_of_place_let_go`]), unobserved. Held to the end of the routine, it
    /// would make the next `T := T + X` or element write of that variable copy the whole value
    /// (`docs/design/lifetime.md` 3.3, 3.4). A dynamic array's is let go of then already
    /// ([`FuncLower::released_early`]).
    fn let_go_at_statement_end(&mut self, e: &Expr, t: Reg) {
        if self.managed(e.ty) && self.copy_of_place_let_go(e) && !self.released_early(e.ty) {
            self.owned.push(t);
        }
    }

    /// [`FuncLower::borrow_arg`], returning the owning temporary of a copy.
    fn borrow_arg_copy(
        &mut self,
        e: &Expr,
        slot: Reg,
        siblings: &(impl WritesVar + ?Sized),
    ) -> Option<Reg> {
        if e.value.is_none()
            && let ExprKind::Var(v) = &e.kind
            && !self.is_ref_param(*v)
            && !siblings.writes(*v)
            // A nested routine may write it during the call (`docs/spec/semantics.md` 4.9)
            && !self.r.vars[v.0 as usize].captured
        {
            let r = self.reg_of(*v);
            self.op2(op::MOVE, slot, r);
            return None;
        }
        let t = self.alloc(e.ty);
        self.expr(e, t);
        self.op2(op::MOVE, slot, t);
        Some(t)
    }

    /// Creates a reference slot for a variable passed to a `var` / `out` argument in `dst`.
    /// `materialize` is the static array type of a variable passed to a `var` open array
    /// parameter: its value is made a unique block first (the callee cannot know the length of a
    /// default value, 0).
    fn make_ref(&mut self, place: &Place, dst: Reg, materialize: Option<Ty>) {
        if let Some(ty) = materialize {
            self.modify_root(place, |this, t| {
                this.op_table(op::UNIQUE, t, Operand::Type(ty))
            });
        }
        match place {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    // Pass the reference on
                    self.op2(op::MOVE, dst, r);
                } else {
                    self.op2(op::MAKE_REF, dst, r);
                }
            }
            Place::Global(g) => self.op_table(op::MAKE_GLOBAL_REF, dst, Operand::Global(g.0)),
            Place::StrIndex { .. } | Place::ObjField { .. } => {
                unreachable!("string elements and object fields cannot be var arguments")
            }
            Place::Field { .. } | Place::Elem { .. } => {
                unreachable!("fields and elements are lent (`Lend`)")
            }
        }
    }

    /// `KeyEquals(A, B)` / `KeyHash(X)` into `dst` (`docs/spec/equality.md` 8.3): the one
    /// expansion of the key equality and hash, by the kind of the arguments' type, for every
    /// use (the generated bodies, and the built-in routines).
    fn key(&mut self, op: KeyOp, args: &[Expr], func: Option<RoutineId>, dst: Reg) {
        if op == KeyOp::Compare {
            return self.key_compare(&args[0], &args[1], func, dst);
        }
        let kind = self.program.types.key_kind(args[0].ty);
        match op {
            KeyOp::Equals => self.key_equals(kind, &args[0], &args[1], func, dst),
            KeyOp::Hash => self.key_hash(kind, &args[0], func, dst),
            KeyOp::Compare => unreachable!("`KeyCompare` is expanded above"),
        }
    }

    /// `KeyCompare(A, B)` into `dst` (`docs/spec/collections.md` 10.2): negative, 0 or positive
    /// by the default order of the arguments' type; `EInvalidOperation` once the arguments are
    /// evaluated for a type without one (an instantiation of a generic body; a call with a type
    /// written out is E0703).
    fn key_compare(&mut self, a: &Expr, b: &Expr, func: Option<RoutineId>, dst: Reg) {
        let kind = self.program.types.order_kind(a.ty);
        if kind == OrderKind::Call {
            let f = func.expect("the analysis finds the `<cmp:…>` of a type with `LessThan`");
            let args = [CallArg::Value(a.clone()), CallArg::Value(b.clone())];
            self.call_routine(f, &args, Dispatch::Routine, Some(dst));
            return;
        }
        let ra = self.expr_any(a);
        let rb = self.expr_any(b);
        match kind {
            OrderKind::Str => self.op3(op::CMP_STR, dst, ra, rb),
            OrderKind::Utf8 => self.op3(op::CMP_U8, dst, ra, rb),
            OrderKind::Int | OrderKind::UInt => {
                // (B < A) - (A < B)
                let lt = if kind == OrderKind::UInt {
                    op::LT_UINT
                } else {
                    op::LT_INT
                };
                let (gt, less) = (self.alloc(Ty::Boolean), self.alloc(Ty::Boolean));
                self.op3(lt, gt, rb, ra);
                self.op3(lt, less, ra, rb);
                self.op3(op::SUB_INT, dst, gt, less);
            }
            OrderKind::Single | OrderKind::Double => {
                // (B < A) - (A < B) - IsNaN(B) + IsNaN(A), with IsNaN(X) = 1 - (X = X): a NaN
                // after every number, two NaNs equal, and `-0.0` = `0.0` as `<` has them
                let (eq, lt) = if kind == OrderKind::Single {
                    (op::EQ_F32, op::LT_F32)
                } else {
                    (op::EQ_F64, op::LT_F64)
                };
                let [a_num, b_num, gt, less] = [(); 4].map(|_| self.alloc(Ty::Boolean));
                self.op3(eq, a_num, ra, ra);
                self.op3(eq, b_num, rb, rb);
                self.op3(lt, gt, rb, ra);
                self.op3(lt, less, ra, rb);
                let (up, down) = (self.alloc(Ty::Integer), self.alloc(Ty::Integer));
                self.op3(op::SUB_INT, up, gt, a_num);
                self.op3(op::SUB_INT, down, less, b_num);
                self.op3(op::SUB_INT, dst, up, down);
            }
            OrderKind::Call | OrderKind::None => self.raise_no_order(a.ty),
        }
    }

    /// `CheckDefaultOrder(X)` (`docs/spec/collections.md` 10.2, 12.1): `X` evaluated, then the
    /// exception of `KeyCompare` when its type has no default order.
    fn check_default_order(&mut self, value: &Expr) {
        self.expr_any(value);
        if self.program.types.order_kind(value.ty) == OrderKind::None {
            self.raise_no_order(value.ty);
        }
    }

    /// `CheckClassType(X, What)` (`docs/spec/collections.md` 9.3, 12.1): nothing once the type
    /// of `X` is a class; any other type raises `EInvalidOperation` with `What` and its name.
    fn check_class_type(&mut self, value: &Expr, what: &Expr) {
        self.expr_any(value);
        let msg = self.alloc(Ty::String);
        self.expr(what, msg);
        if let Ty::Class(_) = value.ty {
            return;
        }
        let t = self.alloc(Ty::String);
        let name = self.program.types.name(value.ty);
        self.load_text(t, &format!(", but `{name}` is not a class"));
        self.op3(op::CONCAT_STR, msg, msg, t);
        let class = Ty::Class(TypeId(bytecode::exc::INVALID_OPERATION));
        let x = self.alloc(class);
        self.new_into(x, class);
        self.obj_set(x, bytecode::EXC_SLOT_MESSAGE, msg, Ty::String, true);
        self.terminate(Term::Raise(x));
    }

    /// Raises the `EInvalidOperation` of a type without a default order (10.2).
    fn raise_no_order(&mut self, ty: Ty) {
        let name = self.program.types.name(ty);
        self.raise_invalid_operation(&format!("`{name}` has no default order; pass a comparer"));
    }

    /// Raises a new `EInvalidOperation` with the message `text`.
    fn raise_invalid_operation(&mut self, text: &str) {
        // A new `EInvalidOperation` with the message, which `Raise` hands to the VM
        let msg = self.alloc(Ty::String);
        self.load_text(msg, text);
        let class = Ty::Class(TypeId(bytecode::exc::INVALID_OPERATION));
        let x = self.alloc(class);
        self.new_into(x, class);
        self.obj_set(x, bytecode::EXC_SLOT_MESSAGE, msg, Ty::String, true);
        self.terminate(Term::Raise(x));
    }

    /// The value of a condition fixed by the type: `IsPlainKey(X)` / `IsPlainOrder(X)` or its
    /// `not` (`docs/spec/bytecode.md` 2.22, 2.23). `None` for any other condition.
    fn fixed_cond(&self, e: &Expr) -> Option<bool> {
        match &e.kind {
            ExprKind::Builtin {
                func: func @ (Builtin::IsPlainKey | Builtin::IsPlainOrder),
                args,
            } => Some(self.is_plain(*func, args[0].ty)),
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
                calc: Calc::Bool,
            } => self.fixed_cond(operand).map(|b| !b),
            _ => None,
        }
    }

    /// `IsPlainKey` / `IsPlainOrder` of the type `ty` (`docs/spec/bytecode.md` 2.22, 2.23).
    fn is_plain(&self, func: Builtin, ty: Ty) -> bool {
        match func {
            Builtin::IsPlainKey => self.is_plain_key(ty),
            _ => self.sort_kind(ty).is_some(),
        }
    }

    /// Whether `HashLookup` compares the keys of `ty` itself (`docs/spec/bytecode.md` 2.23): its
    /// key equality is `EqInt` or `EqStr` (`docs/spec/equality.md` 8.3). `IsPlainKey`.
    fn is_plain_key(&self, ty: Ty) -> bool {
        matches!(self.program.types.key_kind(ty), KeyKind::Int | KeyKind::Str)
    }

    /// `HashFind` (`docs/spec/bytecode.md` 2.23): the arguments in consecutive registers (the
    /// arrays read without counting: the instruction runs no Pascal code), then instruction 90.
    fn hash_find(&mut self, args: &[Expr], dst: Reg) {
        let regs: Vec<Reg> = args.iter().map(|e| self.alloc_arg(Some(e.ty))).collect();
        for (e, r) in args.iter().zip(&regs) {
            self.borrowed_into(e, *r);
        }
        let mut operands = vec![Operand::Reg(dst)];
        operands.extend(regs.iter().map(|r| Operand::Reg(*r)));
        self.plain(op::HASH_FIND, operands);
    }

    /// `HashLookup` (`docs/spec/bytecode.md` 2.23), whose arguments are `KeyHash(Key)` and the
    /// four fields of `Self`: for a key whose equality is `EqInt` / `EqStr`, `Self` (read without
    /// counting) and the key in consecutive registers, then instruction 96 / 98 with the slot of
    /// the first field (the instruction reads the fields and hashes the key). For another key,
    /// `HashFind` from the start with `KeyHash(Key)` and the fields read without counting.
    fn hash_lookup(&mut self, args: &[Expr], dst: Reg) {
        let ExprKind::Key { args: key, .. } = &args[0].kind else {
            unreachable!("sema passes the key of `HashLookup` as `KeyHash(Key)`")
        };
        let ExprKind::ObjField { base, slot, .. } = &args[1].kind else {
            unreachable!("sema checks the arrays of `HashLookup` are fields of `Self`")
        };
        let key = &key[0];
        let opc = match self.program.types.key_kind(key.ty) {
            KeyKind::Int => op::HASH_LOOKUP_INT,
            KeyKind::Str => op::HASH_LOOKUP_STR,
            _ => {
                // `HashFind` from the start of the chain with the hash and the first three
                // fields (the keys are not compared), the start right after them
                let args = &args[..4];
                let regs: Vec<Reg> = args.iter().map(|e| self.alloc_arg(Some(e.ty))).collect();
                let start = self.alloc_arg(Some(Ty::Int64));
                for (e, r) in args.iter().zip(&regs) {
                    self.borrowed_into(e, *r);
                }
                self.load_int(start, -1);
                let mut operands = vec![Operand::Reg(dst)];
                operands.extend(regs.iter().map(|r| Operand::Reg(*r)));
                operands.push(Operand::Reg(start));
                self.plain(op::HASH_FIND, operands);
                return;
            }
        };
        let this = self.alloc_arg(Some(base.ty));
        let k = self.alloc_arg(Some(key.ty));
        self.borrowed_into(base, this);
        self.borrowed_into(key, k);
        self.plain(
            opc,
            vec![
                Operand::Reg(dst),
                Operand::Reg(this),
                Operand::Reg(k),
                Operand::Imm(i64::from(*slot)),
            ],
        );
    }

    /// The value of `e`, read only right away, in `dst` (a call-argument temporary): a field
    /// read there without counting, a value that is not managed made there, anything else as
    /// [`FuncLower::borrowed_value`] and moved.
    fn borrowed_into(&mut self, e: &Expr, dst: Reg) {
        let field = matches!(
            e.kind,
            ExprKind::Field { .. } | ExprKind::Elem { .. } | ExprKind::ObjField { .. }
        );
        if e.value.is_none() && field && !self.is_flat(e.ty) {
            self.read_field(e, dst, false);
        } else if !self.program.types.managed(e.ty) && !matches!(e.kind, ExprKind::Var(_)) {
            self.expr(e, dst);
        } else {
            let r = self.borrowed_value(e);
            self.op2(op::MOVE, dst, r);
        }
    }

    /// The key equality of `a` and `b` (`docs/spec/equality.md` 2, 8.3).
    fn key_equals(&mut self, kind: KeyKind, a: &Expr, b: &Expr, func: Option<RoutineId>, dst: Reg) {
        match kind {
            KeyKind::Call => {
                let f = func.expect("the analysis finds the key equality of a record or an array");
                let args = [CallArg::Value(a.clone()), CallArg::Value(b.clone())];
                self.call_routine(f, &args, Dispatch::Routine, Some(dst));
            }
            KeyKind::Object => {
                // The same handle, else `A.Equals(B)` when neither is `nil` or destroyed (2.6)
                let ra = self.expr_any(a);
                let rb = self.expr_any(b);
                let done = self.new_block();
                self.op3(op::EQ_INT, dst, ra, rb);
                self.branch(op::JMP_IF_TRUE, dst, None, done);
                let alive = self.alloc(Ty::Boolean);
                for r in [ra, rb] {
                    self.intrinsic(op::intr::OBJ_ALIVE, &[alive, r]);
                    self.branch(op::JMP_IF_FALSE, alive, None, done);
                }
                let recv = self.alloc_arg(Some(a.ty));
                let obj = self.alloc_arg(Some(Ty::Class(crate::sema::types::TOBJECT)));
                self.op2(op::MOVE, recv, ra);
                self.op2(op::MOVE, obj, rb);
                let name = self.object_method_name(a.ty, bytecode::VMT_EQUALS, "TObject.Equals");
                let callee = Operand::Virtual {
                    index: bytecode::VMT_EQUALS,
                    name,
                };
                self.call(op::CALL_VIRTUAL, Some(dst), callee, vec![recv, obj], recv);
                self.bind(done);
            }
            _ => {
                let ra = self.expr_any(a);
                let rb = self.expr_any(b);
                match kind {
                    KeyKind::Single => {
                        let (x, y) = (self.alloc(Ty::Double), self.alloc(Ty::Double));
                        self.op2(op::F32_TO_F64, x, ra);
                        self.op2(op::F32_TO_F64, y, rb);
                        self.op3(op::KEY_EQ_F64, dst, x, y);
                    }
                    KeyKind::Double => self.op3(op::KEY_EQ_F64, dst, ra, rb),
                    KeyKind::Str => self.op3(op::EQ_STR, dst, ra, rb),
                    KeyKind::Utf8 => self.op3(op::EQ_U8, dst, ra, rb),
                    KeyKind::BigSet => self.op3(op::SET_EQ, dst, ra, rb),
                    KeyKind::Callable => self.intrinsic_pair(
                        op::intr::CALLABLE_KEY_EQ,
                        dst,
                        [(ra, a.ty), (rb, b.ty)],
                    ),
                    _ => self.op3(op::EQ_INT, dst, ra, rb),
                }
            }
        }
    }

    /// The key hash of `x` (`docs/spec/equality.md` 2, 8.3).
    fn key_hash(&mut self, kind: KeyKind, x: &Expr, func: Option<RoutineId>, dst: Reg) {
        match kind {
            KeyKind::Call => {
                let f = func.expect("the analysis finds the key hash of a record or an array");
                // A record's own `GetHashCode` takes `Self` by reference: a copy of the value
                // (3.2); a generated function takes it as `const`
                let by_ref = matches!(
                    self.program.routines[f.0 as usize]
                        .vars
                        .first()
                        .map(|v| v.kind),
                    Some(VarKind::RecordSelf)
                );
                let arg = if by_ref {
                    CallArg::Temp(x.clone())
                } else {
                    CallArg::Value(x.clone())
                };
                self.call_routine(f, &[arg], Dispatch::Routine, Some(dst));
            }
            KeyKind::Object => {
                // `X.GetHashCode` when not `nil` nor destroyed, else the handle's (2.6)
                let rx = self.expr_any(x);
                let alive = self.alloc(Ty::Boolean);
                let (dead, done) = (self.new_block(), self.new_block());
                self.intrinsic(op::intr::OBJ_ALIVE, &[alive, rx]);
                self.branch(op::JMP_IF_FALSE, alive, None, dead);
                let recv = self.alloc_arg(Some(x.ty));
                self.op2(op::MOVE, recv, rx);
                let name = self.object_method_name(
                    x.ty,
                    bytecode::VMT_GET_HASH_CODE,
                    "TObject.GetHashCode",
                );
                let callee = Operand::Virtual {
                    index: bytecode::VMT_GET_HASH_CODE,
                    name,
                };
                self.call(op::CALL_VIRTUAL, Some(dst), callee, vec![recv], recv);
                self.jump(done);
                self.bind(dead);
                self.op2(op::HASH_INT, dst, rx);
                self.bind(done);
            }
            _ => {
                let rx = self.expr_any(x);
                match kind {
                    KeyKind::Single => {
                        let d = self.alloc(Ty::Double);
                        self.op2(op::F32_TO_F64, d, rx);
                        self.op2(op::HASH_F64, dst, d);
                    }
                    KeyKind::Double => self.op2(op::HASH_F64, dst, rx),
                    KeyKind::Str => self.op2(op::HASH_STR, dst, rx),
                    KeyKind::Utf8 => self.intrinsic(op::intr::HASH_UTF8, &[dst, rx]),
                    KeyKind::BigSet => self.intrinsic(op::intr::HASH_SET_BIG, &[dst, rx]),
                    KeyKind::Callable => self.intrinsic(op::intr::CALLABLE_HASH, &[dst, rx]),
                    _ => self.op2(op::HASH_INT, dst, rx),
                }
            }
        }
    }

    /// The name a virtual call of the entry `index` of `TObject`'s table shows: the method of
    /// the static class's table, or `builtin` (an interface's value, or a class that does not
    /// override it).
    fn object_method_name(&self, ty: Ty, index: u32, builtin: &str) -> String {
        if let Ty::Class(c) = ty
            && let Some(crate::sema::types::VmtEntry::Func(r)) =
                self.program.types.class_vmt(c).get(index as usize)
        {
            return self.ml.func_names[r.0 as usize].clone();
        }
        builtin.to_string()
    }

    /// Evaluates the arguments into consecutive temporaries (an instruction reading a register range).
    /// The instruction only reads them, so managed values are borrowed.
    /// Returns the registers and the copies to let go of after the instruction
    /// ([`FuncLower::let_go_copies`]).
    fn args_range(&mut self, args: &[Expr]) -> (Vec<Reg>, Vec<Reg>) {
        let regs: Vec<Reg> = args.iter().map(|a| self.alloc_arg(Some(a.ty))).collect();
        let mut copies = Vec::new();
        for (a, &r) in args.iter().zip(&regs) {
            if self.managed(a.ty) {
                copies.extend(self.borrow_arg(a, r, args));
            } else {
                self.expr(a, r);
            }
        }
        (regs, copies)
    }

    /// A built-in function of result type `ty`.
    fn builtin(&mut self, func: Builtin, args: &[Expr], ty: Ty, dst: Reg) {
        match func {
            Builtin::Length | Builtin::High => {
                // Read without counting: a counted copy held to the end of the statement would
                // make `SetLength(A, Length(A) + 1)` copy the array (`docs/spec/types.md` 9.1)
                let s = self.borrowed_value(&args[0]);
                let opc = if args[0].ty.is_dyn_indexed() {
                    op::ARR_LEN
                } else if args[0].ty == Ty::Utf8String {
                    op::U8_LEN
                } else {
                    op::STR_LEN
                };
                self.op2(opc, dst, s);
                if func == Builtin::High {
                    self.op_imm(op::ADD_INT_I, dst, dst, -1);
                }
            }
            Builtin::Succ | Builtin::Pred => unreachable!("sema lowers `Succ` / `Pred`"),
            Builtin::Assigned if matches!(args[0].ty, Ty::Callable(_)) => {
                // `docs/spec/objects.md` 9.5, 14.9
                let s = self.expr_any(&args[0]);
                self.intrinsic(op::intr::CALLABLE_ASSIGNED, &[dst, s]);
            }
            Builtin::IsSameInstance => {
                // `docs/spec/collections.md` 9.3, 12.1: the identity of two instances; the other
                // types are only evaluated (their owners free nothing)
                let a = self.expr_any(&args[0]);
                let b = self.expr_any(&args[1]);
                if let Ty::Class(_) = args[0].ty {
                    self.op3(op::EQ_INT, dst, a, b);
                } else {
                    self.op1(op::LOAD_ZERO, dst);
                }
            }
            Builtin::IsOverridden => {
                // `docs/spec/collections.md` 12.1, `docs/spec/bytecode.md` 2.22: `Self` (read
                // without counting), the entry of the table and the method's function, in
                // consecutive registers
                let (ExprKind::RoutineRef(routine), Some(Const::Int(k))) =
                    (&args[2].kind, &args[1].value)
                else {
                    unreachable!("sema makes `IsOverridden` of `Self`, an entry and a method")
                };
                let recv = self.alloc_arg(Some(args[0].ty));
                let entry = self.alloc_arg(Some(Ty::Int64));
                let f = self.alloc_arg(Some(Ty::Int64));
                let b = self.borrowed_value(&args[0]);
                self.op2(op::MOVE, recv, b);
                self.load_int(entry, i64::try_from(*k).expect("an entry of the table"));
                self.load_func(f, self.ml.func_index[routine.0 as usize], 0);
                self.intrinsic(op::intr::IS_OVERRIDDEN, &[dst, recv, entry, f]);
            }
            Builtin::HashFind => self.hash_find(args, dst),
            Builtin::HashLookup => self.hash_lookup(args, dst),
            Builtin::IsPlainKey | Builtin::IsPlainOrder => {
                // `docs/spec/bytecode.md` 2.22, 2.23: a constant of the type (`X` is not
                // evaluated)
                let plain = self.is_plain(func, args[0].ty);
                self.load_int(dst, i64::from(plain));
            }
            Builtin::ObjectClassName => {
                // `docs/spec/collections.md` 9.3, 12.1: `ClassName` of an instance
                // (`docs/spec/objects.md` 14.3); the other types have none
                if let Ty::Class(_) = args[0].ty {
                    let b = self.borrowed_value(&args[0]);
                    self.intrinsic(op::intr::CLASS_NAME, &[dst, b]);
                } else {
                    self.expr_any(&args[0]);
                    self.load_text(dst, "");
                }
            }
            Builtin::IsRefCountedObject => {
                // `docs/spec/collections.md` 9.2, 12.1: `TObject` and the reference counted
                // classes look at the instance; a class managed by hand and the other types cannot
                // hold one (left out by the instantiation, `docs/spec/objects.md` 4.2)
                let s = self.expr_any(&args[0]);
                let looks = matches!(args[0].ty, Ty::Class(_))
                    && self.program.types.slot_kind(args[0].ty) != crate::bytecode::SLOT_PLAIN;
                if looks {
                    self.intrinsic(op::intr::IS_REF_OBJ, &[dst, s]);
                } else {
                    self.op1(op::LOAD_ZERO, dst);
                }
            }
            Builtin::Assigned => {
                let s = self.expr_any(&args[0]);
                let z = self.alloc_borrow(Ty::Int64);
                self.op1(op::LOAD_ZERO, z);
                self.op3(op::EQ_INT, dst, s, z);
                self.op2(op::NOT_BOOL, dst, dst);
            }
            Builtin::Copy if !matches!(args[0].ty, Ty::String | Ty::Utf8String) => {
                self.array_copy(args, dst)
            }
            Builtin::Copy | Builtin::Pos | Builtin::StrToIntDef => {
                let u8 = args.iter().any(|a| a.ty == Ty::Utf8String);
                // The `UTF8String` ones and `StrToIntDef` are intrinsics (`docs/spec/bytecode.md`
                // 2.19)
                let opc = match func {
                    Builtin::Copy if u8 => Err(op::intr::U8_COPY),
                    Builtin::Pos if u8 => Err(op::intr::U8_POS),
                    Builtin::Copy => Ok(op::STR_COPY),
                    Builtin::Pos => Ok(op::STR_POS),
                    _ => Err(op::intr::STR_TO_INT_DEF),
                };
                let (regs, copies) = self.args_range(args);
                let operands: Vec<Reg> = std::iter::once(dst).chain(regs).collect();
                match opc {
                    Ok(opc) => self.plain(opc, operands.into_iter().map(Operand::Reg).collect()),
                    Err(id) => self.intrinsic(id, &operands),
                }
                self.let_go_copies(&copies);
            }
            // sema turns `Chr` into Convert, so it only gets here for `Ord`
            Builtin::Ord | Builtin::Chr => self.expr(&args[0], dst),
            Builtin::Abs if x_is_real(args) => {
                // Clear the sign bit of the `f64` / `f32` bits
                self.expr(&args[0], dst);
                let mask = if args[0].ty == Ty::Single {
                    0x7fff_ffff
                } else {
                    i64::MAX
                };
                let m = self.alloc_borrow(Ty::Int64);
                self.load_int(m, mask);
                self.op3(op::AND_INT, dst, dst, m);
            }
            Builtin::Abs if args[0].ty == Ty::Currency => {
                self.expr(&args[0], dst);
                let z = self.alloc(Ty::Int64);
                self.op1(op::LOAD_ZERO, z);
                let skip = self.new_block();
                self.branch(op::JMP_IF_LE_INT, z, Some(dst), skip);
                self.intrinsic(op::intr::NEG_CURR, &[dst, dst]);
                self.bind(skip);
            }
            Builtin::Sqr => {
                let x = self.expr_any(&args[0]);
                match args[0].ty {
                    Ty::Single | Ty::Double => self.real_arith(BinOp::Mul, args[0].ty, dst, x, x),
                    Ty::Currency => self.op3(op::MUL_CURR, dst, x, x),
                    _ => {
                        // `X * X` in the computation type (the result's type)
                        let overflow = self.current_overflow;
                        let p = self.int_arith(BinOp::Mul, ty, overflow, dst, x, x);
                        self.settle(dst, p);
                    }
                }
            }
            Builtin::Sqrt => {
                self.expr(&args[0], dst);
                self.op2(op::SQRT_F64, dst, dst);
            }
            Builtin::HashCombine => {
                let h = self.expr_any(&args[0]);
                let x = self.expr_any(&args[1]);
                self.op3(op::HASH_MIX, dst, h, x);
            }
            Builtin::SameInstance => {
                let a = self.expr_any(&args[0]);
                let b = self.expr_any(&args[1]);
                self.op3(op::EQ_INT, dst, a, b);
            }
            Builtin::InstanceHash => {
                let x = self.expr_any(&args[0]);
                self.op2(op::HASH_INT, dst, x);
            }
            Builtin::Pi => unreachable!("sema folds `Pi`"),
            Builtin::Trunc | Builtin::Round => {
                let x = &args[0];
                self.expr(x, dst);
                let round = func == Builtin::Round;
                match x.ty {
                    Ty::Currency => {
                        let id = if round {
                            op::intr::ROUND_CURR
                        } else {
                            op::intr::TRUNC_CURR
                        };
                        self.intrinsic(id, &[dst, dst]);
                    }
                    t => {
                        if t == Ty::Single {
                            self.op2(op::F32_TO_F64, dst, dst);
                        }
                        let opc = if round { op::ROUND_F64 } else { op::TRUNC_F64 };
                        self.op2(opc, dst, dst);
                    }
                }
            }
            Builtin::FloatToStr => {
                let x = self.expr_any(&args[0]);
                let id = if args[0].ty == Ty::Single {
                    op::intr::F32_TO_STR
                } else {
                    op::intr::F64_TO_STR
                };
                self.intrinsic(id, &[dst, x]);
            }
            Builtin::StrToFloat => {
                let x = self.expr_any(&args[0]);
                self.intrinsic(op::intr::STR_TO_F64, &[dst, x]);
            }
            Builtin::Abs => {
                let x = &args[0];
                self.expr(x, dst);
                if x.ty == Ty::UInt64 {
                    return;
                }
                let z = self.alloc(x.ty);
                self.op1(op::LOAD_ZERO, z);
                let skip = self.new_block();
                self.branch(op::JMP_IF_LE_INT, z, Some(dst), skip);
                let overflow = self.current_overflow;
                let calc = if x.ty.fits_in_integer() {
                    Ty::Integer
                } else {
                    x.ty
                };
                if calc == Ty::Int64 && overflow {
                    self.op2(op::NEG_INT_CHK, dst, dst);
                } else {
                    self.op2(op::NEG_INT, dst, dst);
                    self.narrow_arith(dst, calc, overflow);
                }
                self.bind(skip);
            }
            Builtin::ParamCount => self.intrinsic(op::intr::ARG_COUNT, &[dst]),
            Builtin::ExceptObject => self.intrinsic(op::intr::EXCEPT_OBJECT, &[dst]),
            Builtin::Eof => self.intrinsic(op::intr::IN_EOF, &[dst]),
            Builtin::Eoln => self.intrinsic(op::intr::IN_EOLN, &[dst]),
            Builtin::ParamStr => {
                let i = self.expr_any(&args[0]);
                self.intrinsic(op::intr::ARG_GET, &[dst, i]);
            }
            Builtin::IntToStr
            | Builtin::UIntToStr
            | Builtin::StrToInt
            | Builtin::StrToInt64
            | Builtin::UpperCase
            | Builtin::LowerCase
            | Builtin::Trim => {
                let x = self.expr_any(&args[0]);
                // The rarer ones are intrinsics (`docs/spec/bytecode.md` 2.19)
                let opc = match func {
                    Builtin::IntToStr => Ok(op::INT_TO_STR),
                    Builtin::UIntToStr => Ok(op::UINT_TO_STR),
                    Builtin::UpperCase => Ok(op::STR_UPPER_ASCII),
                    Builtin::LowerCase => Ok(op::STR_LOWER_ASCII),
                    Builtin::StrToInt => Err(op::intr::STR_TO_INT),
                    Builtin::StrToInt64 => Err(op::intr::STR_TO_INT64),
                    _ => Err(op::intr::STR_TRIM),
                };
                match opc {
                    Ok(opc) => self.op2(opc, dst, x),
                    Err(id) => self.intrinsic(id, &[dst, x]),
                }
            }
            // The default value of every type is 0 (`docs/design/lifetime.md` 3.2)
            Builtin::Default => self.clear_value(ty, dst),
            Builtin::Low | Builtin::SizeOf => unreachable!("sema folds `Low` and `SizeOf`"),
            Builtin::Concat => unreachable!("sema lowers `Concat` to `+`"),
            Builtin::Odd => {
                // The lowest bit is the `Boolean` (two's complement keeps it for negatives)
                let x = self.expr_any(&args[0]);
                let one = self.alloc_borrow(Ty::Int64);
                self.load_int(one, 1);
                self.op3(op::AND_INT, dst, x, one);
            }
            Builtin::UpCase => {
                let x = self.expr_any(&args[0]);
                self.op2(op::UP_CASE_CHAR, dst, x);
            }
            Builtin::Format => self.format(args, dst),
            Builtin::TryStrToInt | Builtin::Supports => {
                unreachable!("sema makes `TryStrToInt` and `Supports` expressions of their own")
            }
            Builtin::IntToHex => {
                // The value as the unsigned integer of its type's width (two's complement)
                let x = self.expr_any(&args[0]);
                let digits = self.expr_any(&args[1]);
                let unsigned = match args[0].ty {
                    Ty::ShortInt => Some(Ty::Byte),
                    Ty::SmallInt => Some(Ty::Word),
                    Ty::Integer => Some(Ty::Cardinal),
                    _ => None,
                };
                let v = match unsigned {
                    Some(u) => {
                        let t = self.alloc_borrow(Ty::Int64);
                        let w = u.width_code().expect("an integer type") as i64;
                        self.op_imm(op::NARROW, t, x, w);
                        t
                    }
                    None => x,
                };
                self.intrinsic_pair(
                    op::intr::INT_TO_HEX,
                    dst,
                    [(v, Ty::Int64), (digits, Ty::Integer)],
                );
            }
            Builtin::StrToBool | Builtin::TrimLeft | Builtin::TrimRight | Builtin::QuotedStr => {
                let x = self.expr_any(&args[0]);
                let id = match func {
                    Builtin::StrToBool => op::intr::STR_TO_BOOL,
                    Builtin::TrimLeft => op::intr::STR_TRIM_LEFT,
                    Builtin::TrimRight => op::intr::STR_TRIM_RIGHT,
                    _ => op::intr::QUOTED_STR,
                };
                self.intrinsic(id, &[dst, x]);
            }
            Builtin::ToString if args[0].ty == Ty::Boolean => {
                // `BoolToStr(Self, True)` (`docs/spec/objects.md` 10.7, 14.10)
                let a = self.expr_any(&args[0]);
                let t = self.alloc_borrow(Ty::Boolean);
                self.load_int(t, 1);
                self.intrinsic_pair(
                    op::intr::BOOL_STR,
                    dst,
                    [(a, Ty::Boolean), (t, Ty::Boolean)],
                );
            }
            // The text `Write` gives (`docs/spec/objects.md` 10.7, 14.10)
            Builtin::ToString => self.string_value(&args[0], dst),
            Builtin::BoolToStr
            | Builtin::SameText
            | Builtin::CompareText
            | Builtin::CompareStr
            | Builtin::StringOfChar => {
                let a = self.expr_any(&args[0]);
                let b = self.expr_any(&args[1]);
                let id = match func {
                    Builtin::BoolToStr => op::intr::BOOL_STR,
                    Builtin::SameText => op::intr::SAME_TEXT,
                    Builtin::CompareText => op::intr::COMPARE_TEXT,
                    Builtin::CompareStr => {
                        self.op3(op::CMP_STR, dst, a, b);
                        return;
                    }
                    _ => op::intr::STR_OF_CHAR,
                };
                self.intrinsic_pair(id, dst, [(a, args[0].ty), (b, args[1].ty)]);
            }
            Builtin::AnsiUpperCase | Builtin::AnsiLowerCase => {
                let s = self.expr_any(&args[0]);
                let mapping = self.expr_any(&args[1]);
                let id = if func == Builtin::AnsiUpperCase {
                    op::intr::STR_UPPER
                } else {
                    op::intr::STR_LOWER
                };
                self.intrinsic_pair(id, dst, [(s, args[0].ty), (mapping, args[1].ty)]);
            }
            Builtin::StringReplace | Builtin::AnsiSameText | Builtin::AnsiCompareText => {
                let id = match func {
                    Builtin::StringReplace => op::intr::STR_REPLACE,
                    Builtin::AnsiSameText => op::intr::ANSI_SAME_TEXT,
                    _ => op::intr::ANSI_COMPARE_TEXT,
                };
                let (range, copies) = self.args_range(args);
                let mut regs = vec![dst];
                regs.extend(range);
                self.intrinsic(id, &regs);
                self.let_go_copies(&copies);
            }
            Builtin::GetEnvironmentVariable => {
                // The host function lets go of the name it read when it writes the value in its place
                let a = self.alloc(Ty::String);
                self.expr(&args[0], a);
                let h = self.ml.host("Env.Get", 1, true);
                self.call(op::CALL_HOST, Some(a), Operand::Host(h), vec![a], a);
                if a != dst {
                    self.op2(op::MOVE_REF, dst, a);
                }
            }
        }
    }
}

impl FuncLower<'_, '_> {
    /// `Format` (`docs/spec/bytecode.md` 2.16): the format, then a pair of the kind and the
    /// (borrowed) value of each element of the `array of const`, in consecutive registers.
    fn format(&mut self, args: &[Expr], dst: Reg) {
        let fmt = self.alloc_arg(Some(Ty::String));
        let pairs: Vec<(Reg, Reg)> = args[1..]
            .iter()
            .map(|a| (self.alloc_arg(Some(Ty::Int64)), self.alloc_arg(Some(a.ty))))
            .collect();
        let mut copies: Vec<Reg> = self.borrow_arg(&args[0], fmt, args).into_iter().collect();
        for (a, &(kind, value)) in args[1..].iter().zip(&pairs) {
            self.load_int(kind, format_kind(a.ty));
            if self.managed(a.ty) {
                copies.extend(self.borrow_arg(a, value, args));
            } else {
                self.expr(a, value);
            }
        }
        let mut operands = vec![
            Operand::Reg(dst),
            Operand::Reg(fmt),
            Operand::Imm(pairs.len() as i64),
        ];
        for (k, v) in pairs {
            operands.push(Operand::Reg(k));
            operands.push(Operand::Reg(v));
        }
        self.plain(op::FORMAT, operands);
        self.let_go_copies(&copies);
    }

    /// `Copy` of an array (`ArrCopy`; `docs/spec/types.md` chapter 7): the source and the bounds
    /// in consecutive registers (a missing index is 0, a missing count the source's length), the
    /// new dynamic array moved from the source's register to `dst`. A static array is made a
    /// block first (its default value, 0, has no length).
    fn array_copy(&mut self, args: &[Expr], dst: Reg) {
        let src = &args[0];
        let regs: Vec<Reg> = (0..3)
            .map(|i| self.alloc_arg(Some(if i == 0 { src.ty } else { Ty::Int64 })))
            .collect();
        let copy = if let Ty::Array(_) = src.ty {
            let t = self.alloc(src.ty);
            self.expr(src, t);
            self.op_table(op::MATERIALIZE, t, Operand::Type(src.ty));
            self.op2(op::MOVE, regs[0], t);
            None
        } else {
            self.borrow_arg(src, regs[0], args)
        };
        match args.get(1) {
            Some(i) => self.expr(i, regs[1]),
            None => self.op1(op::LOAD_ZERO, regs[1]),
        }
        match args.get(2) {
            Some(c) => self.expr(c, regs[2]),
            None => self.op2(op::ARR_LEN, regs[2], regs[0]),
        }
        let mut operands = vec![
            Operand::Reg(regs[0]),
            Operand::Type(self.ty_of_copy(src.ty)),
        ];
        operands.extend(regs[1..].iter().map(|r| Operand::Reg(*r)));
        self.plain(op::ARR_COPY, operands);
        self.op2(op::MOVE_REF, dst, regs[0]);
        self.let_go_copies(copy.as_slice());
    }

    /// A new `TIndexRange` (`docs/spec/objects.md` 12.3, 14.10): the bounds are evaluated in
    /// order, then written with `HasFirst` / `HasLast` into a default value made unique.
    fn index_range(&mut self, first: Option<&Expr>, last: Option<&Expr>, ty: Ty, dst: Reg) {
        let bounds: Vec<(u32, Reg)> = [(0, first), (1, last)]
            .into_iter()
            .filter_map(|(slot, b)| {
                let b = b?;
                let t = self.alloc(Ty::Integer);
                self.expr(b, t);
                Some((slot, t))
            })
            .collect();
        self.clear_value(ty, dst);
        self.op_table(op::UNIQUE, dst, Operand::Type(ty));
        let flag = self.alloc(Ty::Boolean);
        if !bounds.is_empty() {
            self.load_int(flag, 1);
        }
        for (slot, value) in bounds {
            // `First` / `Last` are slots 0 and 1, `HasFirst` / `HasLast` slots 2 and 3
            self.set_slot(dst, &At::Field(slot), value, Ty::Integer, false);
            self.set_slot(dst, &At::Field(slot + 2), flag, Ty::Boolean, false);
        }
    }

    /// `X[L..H]` (`docs/spec/types.md` 8.7): the source and the bounds in consecutive registers,
    /// with the first index after them; `SliceRange` turns the bounds into the 0-based start and
    /// the count cut to the source, which `StrCopy` / `U8Copy` / `ArrCopy` then copy (a string's
    /// start is made 1-based). A bound left out is the first index, or the last one (the length
    /// for a string or a dynamic array, whose first index is at most 1). A static array is made a
    /// block first, as for `Copy`.
    fn slice(&mut self, base: &Expr, low: Option<&Expr>, high: Option<&Expr>, ty: Ty, dst: Reg) {
        let is_str = matches!(base.ty, Ty::String | Ty::Utf8String);
        let fixed = self.program.types.array(base.ty).map(|a| (a.1, a.2));
        let first = match fixed {
            Some((lo, _)) => i64::try_from(lo).expect("a static array's bounds are `Int64`"),
            None if is_str => 1,
            None => 0,
        };
        let regs: Vec<Reg> = (0..4)
            .map(|i| self.alloc_arg(Some(if i == 0 { base.ty } else { Ty::Int64 })))
            .collect();
        let bounds: Vec<&Expr> = low.into_iter().chain(high).collect();
        let copy = if fixed.is_some() {
            let t = self.alloc(base.ty);
            self.expr(base, t);
            self.op_table(op::MATERIALIZE, t, Operand::Type(base.ty));
            self.op2(op::MOVE, regs[0], t);
            None
        } else {
            self.borrow_arg(base, regs[0], &bounds[..])
        };
        let len = self.alloc_borrow(Ty::Int64);
        match fixed {
            Some((lo, hi)) => {
                let n = i64::try_from(hi - lo + 1)
                    .expect("a static array has at most 2^31 - 1 elements");
                self.load_int(len, n);
            }
            None => {
                let opc = match base.ty {
                    Ty::String => op::STR_LEN,
                    Ty::Utf8String => op::U8_LEN,
                    _ => op::ARR_LEN,
                };
                self.op2(opc, len, regs[0]);
            }
        }
        match low {
            Some(l) => self.expr(l, regs[1]),
            None => self.load_int(regs[1], first),
        }
        match (high, fixed) {
            (Some(h), _) => self.expr(h, regs[2]),
            (None, Some((_, hi))) => {
                let hi = i64::try_from(hi).expect("a static array's bounds are `Int64`");
                self.load_int(regs[2], hi)
            }
            (None, None) => self.op2(op::MOVE, regs[2], len),
        }
        self.load_int(regs[3], first);
        self.intrinsic(op::intr::SLICE_RANGE, &[len, regs[1]]);
        if is_str {
            self.op_imm(op::ADD_INT_I, regs[1], regs[1], 1);
            let operands: Vec<Reg> = std::iter::once(dst)
                .chain(regs[..3].iter().copied())
                .collect();
            if base.ty == Ty::Utf8String {
                self.intrinsic(op::intr::U8_COPY, &operands);
            } else {
                self.plain(
                    op::STR_COPY,
                    operands.into_iter().map(Operand::Reg).collect(),
                );
            }
        } else {
            self.plain(
                op::ARR_COPY,
                vec![
                    Operand::Reg(regs[0]),
                    Operand::Type(ty),
                    Operand::Reg(regs[1]),
                    Operand::Reg(regs[2]),
                ],
            );
            self.op2(op::MOVE_REF, dst, regs[0]);
        }
        self.let_go_copies(copy.as_slice());
    }

    /// The dynamic array type `Copy` of an array makes (the element type's dynamic array).
    fn ty_of_copy(&self, src: Ty) -> Ty {
        let elem = self
            .program
            .types
            .elem_of(src)
            .expect("Copy of an array has an element type");
        let found = self
            .program
            .types
            .defs
            .iter()
            .position(|d| matches!(d.kind, crate::sema::types::TypeKind::DynArray { elem: e, .. } if e == elem))
            .expect("sema adds the dynamic array of Copy's result");
        Ty::DynArray(crate::sema::types::TypeId(found as u32))
    }
}

/// A `Narrow` (its width) not yet emitted for a register: the register's low bits hold the value of
/// a `{$Q-}` operation in a narrow computation type, and its upper bits are not yet the sign or zero
/// extension (`docs/spec/semantics.md` 3.2 narrows the result; wrapping makes it the same to narrow
/// later, as long as only `+`, `-`, `*`, `and`, `or`, `xor`, `not`, unary `-`, the 32-bit shifts and
/// narrowing conversions read the register before).
type Pending = Option<i64>;

/// The `Narrow` width of a narrow computation type (`Integer`, `Cardinal`); `None` for 64 bits.
fn narrow_width(calc: Ty) -> Option<i64> {
    match calc {
        Ty::Int64 | Ty::UInt64 => None,
        _ => Some(calc.width_code().unwrap_or(4) as i64),
    }
}

/// The `Narrow` a wrapping operation in `calc` leaves pending: under `{$Q-}` a narrow computation
/// type wraps modulo 2^width, so the result may stay unnarrowed while only operations that read its
/// low bits use it. Under `{$Q+}` the operation is checked at once (`NarrowOvf`).
fn arith_pending(calc: Ty, overflow: bool) -> Pending {
    if overflow { None } else { narrow_width(calc) }
}

/// Whether `+`, `-` and `*` in `calc` are the 32-bit operations that wrap and narrow at once
/// (`AddInt32` ...; `docs/spec/bytecode.md` 2.2): `Integer` under `{$Q-}`.
fn wraps32(calc: Ty, overflow: bool) -> bool {
    arith_pending(calc, overflow) == Some(4)
}

/// The pending `Narrow` an integer operation `op` in `calc` accepts on its operands: the
/// operations that wrap (and the 32-bit shifts, which read the low 32 bits) under `{$Q-}`.
/// Division, `mod` and comparisons read the whole value.
fn int_op_wraps(op: BinOp, calc: Ty, overflow: bool) -> Pending {
    match op {
        BinOp::Add
        | BinOp::Sub
        | BinOp::Mul
        | BinOp::And
        | BinOp::Or
        | BinOp::Xor
        | BinOp::Shl
        | BinOp::Shr => arith_pending(calc, overflow),
        _ => None,
    }
}

/// Whether a `Narrow` of width `w` reads only the low 32 bits or fewer (`docs/spec/bytecode.md` 2.2).
fn low_bits_width(w: i64) -> bool {
    matches!(w, 0..=5 | 8)
}

/// The kind of an element of `Format`'s `array of const` (`docs/spec/bytecode.md` 2.16).
fn format_kind(ty: Ty) -> i64 {
    match ty {
        Ty::Boolean => bytecode::FORMAT_KIND_BOOLEAN,
        Ty::String => bytecode::FORMAT_KIND_STRING,
        Ty::Utf8String => bytecode::FORMAT_KIND_UTF8,
        Ty::Single => bytecode::FORMAT_KIND_SINGLE,
        Ty::Double => bytecode::FORMAT_KIND_DOUBLE,
        Ty::Currency => bytecode::FORMAT_KIND_CURRENCY,
        // Integers and `Char` (8): the width code
        t => i64::from(
            t.width_code()
                .expect("sema allows only these types in an `array of const`"),
        ),
    }
}

/// Whether the argument of a built-in is a real.
fn x_is_real(args: &[Expr]) -> bool {
    args.first().is_some_and(|x| x.ty.is_real())
}

/// Converts a constant to a string for `Write` (integers, `Boolean`, `Char`, `string`).
fn const_as_string(c: &Const, ty: Ty, types: &TypeTable) -> Option<Vec<u16>> {
    Some(match c {
        // The name of an enumeration value (`docs/spec/types.md` 8.1)
        Const::Int(v) if matches!(ty, Ty::Enum(_)) => types
            .enum_values(ty)
            .iter()
            .find(|e| e.ord == *v)
            .map_or_else(|| v.to_string(), |e| e.name.clone())
            .encode_utf16()
            .collect(),
        Const::Str(s) => s.clone(),
        Const::U8(s) => s.encode_utf16().collect(),
        Const::Char(ch) => vec![*ch],
        Const::Bool(b) => if *b { "TRUE" } else { "FALSE" }.encode_utf16().collect(),
        Const::Int(v) => {
            let text = if ty == Ty::UInt64 {
                (*v as u64).to_string()
            } else {
                v.to_string()
            };
            text.encode_utf16().collect()
        }
        Const::Real(b) if ty == Ty::Single => crate::numfmt::f32_to_str(f64::from_bits(*b) as f32)
            .encode_utf16()
            .collect(),
        Const::Real(b) => crate::numfmt::f64_to_str(f64::from_bits(*b))
            .encode_utf16()
            .collect(),
        Const::Curr(c) => crate::numfmt::curr_to_str(*c).encode_utf16().collect(),
        Const::Agg(_) | Const::Nil | Const::Set(_) => return None,
    })
}

/// Whether a variable's register holds a reference of its managed value: a managed variable other
/// than a `const` parameter, a value parameter that is not written or the `E` of `on E`
/// (borrowed) or a `var` / `out` parameter (a reference slot).
fn owns_value(v: &Var, types: &TypeTable) -> bool {
    types.managed(v.ty)
        && !cell_slot(v)
        && !borrows_argument(v)
        && !matches!(
            v.kind,
            VarKind::Param(ParamMode::Const | ParamMode::Var | ParamMode::Out)
                | VarKind::Exception
                | VarKind::SelfParam
                | VarKind::RecordSelf
                | VarKind::Outer { .. }
        )
}

/// Whether a value parameter borrows its argument like a `const` one: nothing in its routine (or
/// a nested one) writes it, and no nested routine uses it (`docs/design/lifetime.md` 3.3). The
/// caller lends every managed value argument; a value parameter that does not borrow counts its
/// argument when the routine is entered.
fn borrows_argument(v: &Var) -> bool {
    v.kind == VarKind::Param(ParamMode::Value) && !v.written && !v.captured
}

/// Whether a managed argument for a parameter of mode `mode` is lent to the call (a value or
/// `const` parameter; `docs/design/lifetime.md` 3.3).
fn lends_argument(mode: Option<ParamMode>) -> bool {
    matches!(mode, Some(ParamMode::Value | ParamMode::Const))
}

/// Whether the variable's register holds a reference slot: a `var` / `out` parameter, a
/// variable of an enclosing routine (`docs/spec/semantics.md` 4.9), or a captured variable,
/// whose value is in its cell (`docs/spec/objects.md` 14.9).
fn holds_ref(v: &Var) -> bool {
    matches!(
        v.kind,
        VarKind::Param(ParamMode::Var | ParamMode::Out)
            | VarKind::RecordSelf
            | VarKind::Outer { .. }
    ) || cell_slot(v)
}

/// Whether a captured variable's register holds a reference slot to its cell: not `Self`, a
/// `const` parameter or `E` of `on E`, which cannot change and whose cell holds a copy.
fn cell_slot(v: &Var) -> bool {
    v.cell
        && !matches!(
            v.kind,
            VarKind::SelfParam | VarKind::Param(ParamMode::Const) | VarKind::Exception
        )
}

/// Whether a variable is declared inline (or by `for var`): it gets a new cell each time its
/// declaration runs (`docs/spec/objects.md` 9.6).
fn declared_inline(v: &Var) -> bool {
    v.is_for_var
        || v.decl
            .as_ref()
            .is_some_and(|d| d.kind() == crate::syntax::SyntaxKind::INLINE_VAR_DECL)
}

/// Whether the routine's function takes a hidden first argument: the static link, or a
/// closure's own block (`docs/spec/objects.md` 14.9).
fn hidden_first(r: &Routine) -> bool {
    r.link || r.closure
}

/// Whether the register of the cell of `v` owns the cell (it was made in this routine) rather
/// than borrowing it.
fn owns_cell(v: &Var) -> bool {
    !matches!(v.kind, VarKind::Outer { .. })
}

/// The routine `up` levels out of `r` (through `parent`).
fn enclosing_of<'p>(program: &'p Program, r: &'p Routine, up: u32) -> Option<&'p Routine> {
    let mut at = r;
    for _ in 0..up {
        at = &program.routines[at.parent?.0 as usize];
    }
    Some(at)
}

/// Whether the variable `v` of `r` has a register holding its cell (`docs/spec/objects.md`
/// 14.9): it is captured, or `r` is a closure and took it from its block, or `r` reaches it
/// through the static link and its routine has its cell in a register.
fn needs_cell(program: &Program, r: &Routine, v: &Var) -> bool {
    match v.kind {
        _ if v.cell => true,
        VarKind::Outer { .. } if r.closure => true,
        VarKind::Outer { up, var } => enclosing_of(program, r, up)
            .is_some_and(|o| needs_cell(program, o, &o.vars[var.0 as usize])),
        _ => false,
    }
}

/// The variables of `r` whose cells have registers, in variable order: the registers follow the
/// named variables.
fn cell_vars(program: &Program, r: &Routine) -> Vec<VarId> {
    r.vars
        .iter()
        .enumerate()
        .filter(|(_, v)| needs_cell(program, r, v))
        .map(|(i, _)| VarId(i as u32))
        .collect()
}

/// The register (without the hidden first argument) of the cell of the variable `v` of `r`.
fn cell_index(program: &Program, r: &Routine, v: VarId) -> Option<u32> {
    let k = cell_vars(program, r).iter().position(|c| *c == v)?;
    Some((r.vars.len() + k) as u32)
}

/// The (uncounted, counted) opcodes reading (`get`) or writing a slot, and the operand naming the
/// slot (a slot number, or the register of a slot number or an index).
fn slot_access(slot: &At, get: bool) -> ((u16, u16), Operand) {
    match (slot, get) {
        (At::Field(i), true) => ((op::FIELD_GET, op::FIELD_GET_REF), Operand::Imm(*i as i64)),
        (At::Field(i), false) => ((op::FIELD_SET, op::FIELD_SET_REF), Operand::Imm(*i as i64)),
        (At::Val(reg), true) => ((op::VAL_GET, op::VAL_GET_REF), Operand::Reg(*reg)),
        (At::Val(reg), false) => ((op::VAL_SET, op::VAL_SET_REF), Operand::Reg(*reg)),
        (At::Dyn(reg), true) => ((op::DYN_GET, op::DYN_GET_REF), Operand::Reg(*reg)),
        (At::Dyn(reg), false) => ((op::DYN_SET, op::DYN_SET_REF), Operand::Reg(*reg)),
    }
}

/// Where `Exit` / `Break` / `Continue` goes (`Break` / `Continue` name the loop by its index in
/// `FuncLower::loops`).
#[derive(Debug, Clone, Copy)]
enum Leave {
    Exit,
    Break(usize),
    Continue(usize),
}

/// What a jump out of a statement has to do on its way (`docs/spec/exceptions.md` 6.2).
enum Cleanup {
    /// Inside an `except` block: release its exception.
    Except(Reg),
    /// Inside the body of a `try..finally`: run the `finally` block, with `state` set to `k + 2`
    /// for the `k`-th continuation (1-based) in `conts`.
    Finally {
        state: Reg,
        fin: BlockId,
        conts: Vec<(BlockId, Leave)>,
    },
}

/// The arguments of a call or a built-in, asked whether evaluating them can write a local variable
/// (through a `var` / `out` argument, possibly of a nested call).
trait WritesVar {
    fn writes(&self, v: VarId) -> bool;
}

impl WritesVar for [CallArg] {
    fn writes(&self, v: VarId) -> bool {
        self.iter().any(|a| call_arg_writes_var(a, v))
    }
}

impl WritesVar for [Expr] {
    fn writes(&self, v: VarId) -> bool {
        self.iter().any(|e| expr_writes_var(e, v))
    }
}

impl WritesVar for [&Expr] {
    fn writes(&self, v: VarId) -> bool {
        self.iter().any(|e| expr_writes_var(e, v))
    }
}

fn call_arg_writes_var(a: &CallArg, v: VarId) -> bool {
    match a {
        CallArg::Value(e) | CallArg::Temp(e) => expr_writes_var(e, v),
        CallArg::Ref { place, .. } => place_root(place) == Some(v),
    }
}

/// The local variable a place starts from.
fn place_root(p: &Place) -> Option<VarId> {
    match p.root() {
        Place::Var(v, _) => Some(*v),
        _ => None,
    }
}

/// The registers and blocks of a `try..finally` being lowered ([`Lowerer::finally_begin`]).
struct FinallyScope {
    state: Reg,
    exc: Reg,
    /// The `finally` block, and the block after the statement.
    fin: BlockId,
    after: BlockId,
    /// The position of the body's first block.
    body_first: BlockId,
}

/// Whether evaluating the expression can write the local variable `v` (it is passed to a `var` /
/// `out` parameter somewhere inside).
fn expr_writes_var(e: &Expr, v: VarId) -> bool {
    match &e.kind {
        ExprKind::Var(_) | ExprKind::Const { .. } | ExprKind::Global(_) | ExprKind::Error => false,
        ExprKind::Binary { first, steps } => {
            expr_writes_var(first, v) || steps.iter().any(|s| expr_writes_var(&s.rhs, v))
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Convert { operand, .. }
        | ExprKind::ExcMember { base: operand, .. }
        | ExprKind::ObjField { base: operand, .. }
        | ExprKind::ClassName { base: operand }
        | ExprKind::ClassCheck { operand, .. }
        | ExprKind::Weak(operand) => expr_writes_var(operand, v),
        ExprKind::ClassInfo { args, .. } => args.iter().any(|a| expr_writes_var(a, v)),
        ExprKind::ClassRefValue(_)
        | ExprKind::RoutineRef(_)
        | ExprKind::Anon { .. }
        | ExprKind::NestedRef { .. } => false,
        ExprKind::MethodRef { base, .. } | ExprKind::ToClosure(base) => expr_writes_var(base, v),
        ExprKind::CallValue { callee, args } => {
            expr_writes_var(callee, v) || args.iter().any(|a| call_arg_writes_var(a, v))
        }
        ExprKind::New { args, of, .. } => {
            of.as_deref().is_some_and(|c| expr_writes_var(c, v))
                || args.iter().any(|a| call_arg_writes_var(a, v))
        }
        ExprKind::InheritedBuiltin { msg, .. } => {
            msg.as_deref().is_some_and(|m| expr_writes_var(m, v))
        }
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            expr_writes_var(base, v) || expr_writes_var(index, v)
        }
        ExprKind::Field { base, .. } => expr_writes_var(base, v),
        ExprKind::RecordNew { call } => expr_writes_var(call, v),
        ExprKind::Slice { base, low, high } => {
            expr_writes_var(base, v) || low.iter().chain(high.iter()).any(|b| expr_writes_var(b, v))
        }
        ExprKind::IndexRange { first, last } => first
            .iter()
            .chain(last.iter())
            .any(|b| expr_writes_var(b, v)),
        ExprKind::Call { args, .. } => args.iter().any(|a| call_arg_writes_var(a, v)),
        ExprKind::Builtin { args, .. } | ExprKind::Key { args, .. } | ExprKind::ArrayCtor(args) => {
            args.iter().any(|a| expr_writes_var(a, v))
        }
        ExprKind::SetCtor(elems) => set_ctor_exprs(elems).any(|a| expr_writes_var(a, v)),
        ExprKind::TryStrToInt { s, target } => {
            expr_writes_var(s, v) || place_root(target) == Some(v)
        }
        ExprKind::Supports {
            operand, target, ..
        } => {
            expr_writes_var(operand, v) || target.as_ref().is_some_and(|t| place_root(t) == Some(v))
        }
    }
}

/// The bounds of the elements of a set constructor, in order.
fn set_ctor_exprs(elems: &[SetCtorElem]) -> impl Iterator<Item = &Expr> {
    elems
        .iter()
        .flat_map(|e| std::iter::once(&e.low).chain(e.high.as_ref()))
}

/// The instance a constructor call creates (`docs/spec/objects.md` 3.1): its static class, the
/// register it is put in, and the class reference it is created from (`NewOf`), if any.
struct NewIn {
    class: Ty,
    inst: Reg,
    of: Option<Reg>,
}

/// Whether the expression reads the variable `reg`.
fn expr_reads_reg(e: &Expr, reg: Reg) -> bool {
    match &e.kind {
        ExprKind::Var(v) => Reg::Var(v.0) == reg,
        ExprKind::Const { .. } | ExprKind::Global(_) | ExprKind::Error => false,
        ExprKind::Binary { first, steps } => {
            expr_reads_reg(first, reg) || steps.iter().any(|s| expr_reads_reg(&s.rhs, reg))
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Convert { operand, .. }
        | ExprKind::ExcMember { base: operand, .. }
        | ExprKind::ObjField { base: operand, .. }
        | ExprKind::ClassName { base: operand }
        | ExprKind::ClassCheck { operand, .. }
        | ExprKind::Weak(operand) => expr_reads_reg(operand, reg),
        ExprKind::ClassInfo { args, .. } => args.iter().any(|a| expr_reads_reg(a, reg)),
        ExprKind::ClassRefValue(_) | ExprKind::RoutineRef(_) => false,
        ExprKind::Anon { captures, .. } => captures.iter().any(|c| Reg::Var(c.0) == reg),
        ExprKind::NestedRef { block, .. } => block.is_some_and(|b| Reg::Var(b.0) == reg),
        ExprKind::MethodRef { base, .. } | ExprKind::ToClosure(base) => expr_reads_reg(base, reg),
        ExprKind::CallValue { callee, args } => {
            expr_reads_reg(callee, reg)
                || args.iter().any(|a| match a {
                    CallArg::Value(v) | CallArg::Temp(v) => expr_reads_reg(v, reg),
                    CallArg::Ref { place, .. } => place_reads_reg(place, reg),
                })
        }
        ExprKind::New { args, of, .. } => {
            of.as_deref().is_some_and(|c| expr_reads_reg(c, reg))
                || args.iter().any(|a| match a {
                    CallArg::Value(v) | CallArg::Temp(v) => expr_reads_reg(v, reg),
                    CallArg::Ref { place, .. } => place_reads_reg(place, reg),
                })
        }
        ExprKind::InheritedBuiltin { base, msg, .. } => {
            msg.is_some()
                && (expr_reads_reg(base, reg)
                    || msg.as_deref().is_some_and(|m| expr_reads_reg(m, reg)))
        }
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            expr_reads_reg(base, reg) || expr_reads_reg(index, reg)
        }
        ExprKind::Field { base, .. } => expr_reads_reg(base, reg),
        ExprKind::RecordNew { call } => expr_reads_reg(call, reg),
        ExprKind::Slice { base, low, high } => {
            expr_reads_reg(base, reg)
                || low
                    .iter()
                    .chain(high.iter())
                    .any(|b| expr_reads_reg(b, reg))
        }
        ExprKind::IndexRange { first, last } => first
            .iter()
            .chain(last.iter())
            .any(|b| expr_reads_reg(b, reg)),
        ExprKind::Call { args, .. } => args.iter().any(|a| match a {
            CallArg::Value(v) | CallArg::Temp(v) => expr_reads_reg(v, reg),
            CallArg::Ref { place, .. } => place_reads_reg(place, reg),
        }),
        ExprKind::Builtin { args, .. } | ExprKind::Key { args, .. } | ExprKind::ArrayCtor(args) => {
            args.iter().any(|a| expr_reads_reg(a, reg))
        }
        ExprKind::SetCtor(elems) => set_ctor_exprs(elems).any(|a| expr_reads_reg(a, reg)),
        ExprKind::TryStrToInt { s, target } => {
            expr_reads_reg(s, reg) || place_reads_reg(target, reg)
        }
        ExprKind::Supports {
            operand, target, ..
        } => {
            expr_reads_reg(operand, reg) || target.as_ref().is_some_and(|t| place_reads_reg(t, reg))
        }
    }
}

fn place_reads_reg(p: &Place, reg: Reg) -> bool {
    match p {
        Place::Var(v, _) => Reg::Var(v.0) == reg,
        Place::Global(_) => false,
        Place::StrIndex { base, index, .. } | Place::Elem { base, index, .. } => {
            place_reads_reg(base, reg) || expr_reads_reg(index, reg)
        }
        Place::Field { base, .. } => place_reads_reg(base, reg),
        Place::ObjField { base, .. } => expr_reads_reg(base, reg),
    }
}

/// Whether evaluating the expression may run code (a routine or host call), which could change
/// any variable.
fn expr_has_call(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call { .. } | ExprKind::Key { .. } => true,
        ExprKind::Builtin { func, args } => {
            *func == Builtin::GetEnvironmentVariable || args.iter().any(expr_has_call)
        }
        ExprKind::Var(_) | ExprKind::Const { .. } | ExprKind::Global(_) | ExprKind::Error => false,
        ExprKind::Binary { first, steps } => {
            expr_has_call(first) || steps.iter().any(|s| expr_has_call(&s.rhs))
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Convert { operand, .. }
        | ExprKind::ExcMember { base: operand, .. }
        | ExprKind::ObjField { base: operand, .. }
        | ExprKind::ClassName { base: operand }
        | ExprKind::ClassCheck { operand, .. }
        | ExprKind::Weak(operand) => expr_has_call(operand),
        ExprKind::ClassInfo { args, .. } => args.iter().any(expr_has_call),
        ExprKind::ClassRefValue(_)
        | ExprKind::RoutineRef(_)
        | ExprKind::Anon { .. }
        | ExprKind::NestedRef { .. } => false,
        ExprKind::MethodRef { base, .. } | ExprKind::ToClosure(base) => expr_has_call(base),
        ExprKind::CallValue { .. } => true,
        // The constructor and `AfterConstruction` are code (`docs/spec/objects.md` 3.1)
        // (the argument of a built-in constructor is the message of `Exception.Create`)
        ExprKind::New {
            ctor,
            args,
            after,
            of,
            ..
        } => {
            ctor.is_some()
                || *after
                || of.is_some()
                || args.iter().any(|a| match a {
                    CallArg::Value(m) => expr_has_call(m),
                    _ => true,
                })
        }
        ExprKind::InheritedBuiltin { msg, .. } => msg.as_deref().is_some_and(expr_has_call),
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            expr_has_call(base) || expr_has_call(index)
        }
        ExprKind::Field { base, .. } => expr_has_call(base),
        ExprKind::RecordNew { .. } => true,
        ExprKind::Slice { base, low, high } => {
            expr_has_call(base) || low.iter().chain(high.iter()).any(|b| expr_has_call(b))
        }
        ExprKind::IndexRange { first, last } => {
            first.iter().chain(last.iter()).any(|b| expr_has_call(b))
        }
        ExprKind::ArrayCtor(args) => args.iter().any(expr_has_call),
        ExprKind::SetCtor(elems) => set_ctor_exprs(elems).any(expr_has_call),
        ExprKind::TryStrToInt { s, target } => expr_has_call(s) || place_has_call(target),
        ExprKind::Supports {
            operand, target, ..
        } => expr_has_call(operand) || target.as_ref().is_some_and(place_has_call),
    }
}

/// How `T := T + X1 + … + Xn` appends to its string place `T` (`Lowerer::append`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppendKind {
    /// A `string` (`AppendStr`).
    Str,
    /// A `UTF8String` with `UTF8String` operands (`AppendU8`).
    U8,
    /// A `UTF8String` with operands joined as a `string` (the `+` with a `string` or `Char`;
    /// `docs/spec/types.md` 8.5), converted to `UTF8String` once (`AppendU8`).
    U8ViaStr,
}

/// The steps `+ X1 … + Xn` when `value` is `target + X1 + … + Xn` for a string or `UTF8String`
/// place `target` of type `ty` (the appends of `docs/spec/types.md` 9.1): the first operand reads
/// the place itself ([`expr_is_place`]) and every step is a concatenation.
fn append_steps<'e>(
    target: &Place,
    ty: Ty,
    value: &'e Expr,
) -> Option<(&'e [BinStep], AppendKind)> {
    if value.value.is_some() {
        return None;
    }
    let (sum, kind) = match (&value.kind, ty) {
        (
            ExprKind::Convert {
                operand,
                kind: Conversion::StrToU8,
            },
            Ty::Utf8String,
        ) => (&**operand, AppendKind::U8ViaStr),
        (_, Ty::String) => (value, AppendKind::Str),
        (_, Ty::Utf8String) => (value, AppendKind::U8),
        _ => return None,
    };
    let ExprKind::Binary { first, steps } = &sum.kind else {
        return None;
    };
    let calc = match kind {
        AppendKind::U8 => Calc::U8,
        AppendKind::Str | AppendKind::U8ViaStr => Calc::Str,
    };
    if steps.iter().any(|s| s.op != BinOp::Add || s.calc != calc) {
        return None;
    }
    let first = match (kind, &first.kind) {
        (
            AppendKind::U8ViaStr,
            ExprKind::Convert {
                operand,
                kind: Conversion::U8ToStr,
            },
        ) => &**operand,
        (AppendKind::U8ViaStr, _) => return None,
        _ => &**first,
    };
    (!steps.is_empty() && expr_is_place(first, target)).then_some((steps, kind))
}

/// Whether reading `e` reads the place `p` itself: the same variable, global, field or element,
/// with the same index expressions, which run no code (so evaluating them once for the place gives
/// what evaluating them again would; [`same_pure_expr`]).
fn expr_is_place(e: &Expr, p: &Place) -> bool {
    if e.value.is_some() {
        return false;
    }
    match (&e.kind, p) {
        (ExprKind::Var(a), Place::Var(b, _)) => a == b,
        (ExprKind::Global(a), Place::Global(b)) => a == b,
        (
            ExprKind::Field { base, index, .. },
            Place::Field {
                base: pb,
                index: pi,
                ..
            },
        ) => index == pi && expr_is_place(base, pb),
        (
            ExprKind::Elem { base, index },
            Place::Elem {
                base: pb,
                index: pi,
                ..
            },
        ) => same_pure_expr(index, pi) && expr_is_place(base, pb),
        _ => false,
    }
}

/// Whether two expressions are written the same and run no code (constants, variables, globals,
/// fields, elements, operators and conversions of those): evaluated one after the other with
/// nothing in between, they give the same value.
fn same_pure_expr(a: &Expr, b: &Expr) -> bool {
    if a.ty != b.ty {
        return false;
    }
    match (&a.value, &b.value) {
        (Some(x), Some(y)) => return x == y,
        (None, None) => {}
        _ => return false,
    }
    match (&a.kind, &b.kind) {
        (ExprKind::Var(x), ExprKind::Var(y)) => x == y,
        (ExprKind::Global(x), ExprKind::Global(y)) => x == y,
        (
            ExprKind::Field {
                base: x, index: i, ..
            },
            ExprKind::Field {
                base: y, index: j, ..
            },
        ) => i == j && same_pure_expr(x, y),
        (ExprKind::Elem { base: x, index: i }, ExprKind::Elem { base: y, index: j }) => {
            same_pure_expr(x, y) && same_pure_expr(i, j)
        }
        (
            ExprKind::Binary {
                first: f1,
                steps: s1,
            },
            ExprKind::Binary {
                first: f2,
                steps: s2,
            },
        ) => {
            same_pure_expr(f1, f2)
                && s1.len() == s2.len()
                && s1.iter().zip(s2).all(|(x, y)| {
                    x.op == y.op
                        && x.calc == y.calc
                        && x.ty == y.ty
                        && same_pure_expr(&x.rhs, &y.rhs)
                })
        }
        (
            ExprKind::Convert {
                operand: x,
                kind: k,
            },
            ExprKind::Convert {
                operand: y,
                kind: l,
            },
        ) => k == l && same_pure_expr(x, y),
        _ => false,
    }
}

/// Whether evaluating a place (its indexes and exception values) may run code.
fn place_has_call(p: &Place) -> bool {
    match p {
        Place::Var(..) | Place::Global(_) => false,
        Place::StrIndex { base, index, .. } | Place::Elem { base, index, .. } => {
            place_has_call(base) || expr_has_call(index)
        }
        Place::Field { base, .. } => place_has_call(base),
        Place::ObjField { base, .. } => expr_has_call(base),
    }
}

/// Whether a place is a field or element (written through its containers).
fn is_part(p: &Place) -> bool {
    matches!(p, Place::Field { .. } | Place::Elem { .. })
}

/// Whether a `var` / `out` argument is taken out and back (`Lend`): a field or element, or a
/// field of an instance (`docs/spec/objects.md` 2.2).
fn is_lent(p: &Place) -> bool {
    is_part(p) || matches!(p, Place::ObjField { .. })
}

/// The variable or global a field or element place starts from.
fn part_root(p: &Place) -> &Place {
    match p {
        Place::Field { base, .. } | Place::Elem { base, .. } => part_root(base),
        _ => p,
    }
}

/// A field or element place with its indices evaluated (`FuncLower::eval_part`).
#[derive(Clone)]
struct Part<'p> {
    /// The variable, global or instance's field the place starts from.
    root: &'p Place,
    /// For an instance's field as the root, the handle of the instance, read before the
    /// right-hand side (`docs/spec/objects.md` 2.2).
    handle: Option<Reg>,
    /// From the root outward.
    steps: Vec<PartStep>,
    /// The register holding the root's value and its type, read and written as a local
    /// variable's, in place of the root: a copy of a reference parameter's value
    /// ([`FuncLower::with_ref_copy`]).
    root_reg: Option<(Reg, Ty)>,
}

#[derive(Clone)]
struct PartStep {
    slot: SlotRef,
    /// The type of the slot's value.
    ty: Ty,
}

/// An element index of a static array: a constant, or a register with it (0-based).
#[derive(Debug, Clone, Copy)]
enum Idx {
    Const(i64),
    Reg(Reg),
}

/// A step of a place: a field of a record (its number), an element of a static array, or an
/// element of a dynamic or open array (a register with the index, checked by the VM).
#[derive(Clone)]
enum SlotRef {
    Field(u32),
    Elem {
        index: Idx,
        check: Option<IndexCheck>,
    },
    Dyn(Reg),
}

/// One slot of a container, for the instructions reading and writing one: a slot number
/// (`FieldGet`), a register with a slot number (`ValGet`), a register with an element index
/// of a dynamic or open array (`DynGet`).
enum At {
    Field(u32),
    Val(Reg),
    Dyn(Reg),
}

/// What holds the slots of a hop ([`FuncLower::hop_list`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HopKind {
    /// A record or static array block.
    Agg,
    /// An instance (its handle).
    Obj,
    /// A dynamic or open array block.
    Dyn,
    /// The record or static array a reference slot leads to (`RefFieldGet` / `RefFieldSet`),
    /// at a constant address.
    Ref,
}

/// A slot or flat place in a container: the steps of a place into it, with those inside
/// records and static arrays laid out flat folded into its address (`docs/design/records.md`
/// 3.4).
#[derive(Debug, Clone)]
struct Hop {
    kind: HopKind,
    /// The container's type: a record, static array, dynamic or open array, or the class.
    ty: Ty,
    /// In an array: the element.
    elem: Option<Idx>,
    /// The slots of an element of an array (1 otherwise).
    width: u32,
    /// In a dynamic or open array: whether its elements are laid out flat.
    flat_elem: bool,
    /// The slot in the element (or in the record or instance): `off` plus each index times its
    /// stride.
    off: u32,
    terms: Vec<(Reg, u32)>,
    /// The number of the place's first step into the container.
    start: usize,
    /// The type of the value at the address.
    val: Ty,
}

impl Hop {
    /// The field of an instance whose first slot is `slot`, of type `val`.
    fn obj(class: Ty, slot: u32, val: Ty) -> Hop {
        Hop {
            kind: HopKind::Obj,
            ty: class,
            elem: None,
            width: 1,
            flat_elem: false,
            off: slot,
            terms: Vec::new(),
            start: 0,
            val,
        }
    }

    /// Whether the address is known when compiling.
    fn is_static(&self) -> bool {
        self.terms.is_empty() && !matches!(self.elem, Some(Idx::Reg(_)))
    }
}

/// The second register of the cursor whose first is `cur` (`docs/spec/bytecode.md` 1.5):
/// `FuncLower` makes the two one after the other on the stack of temporaries.
fn cursor_next(cur: Reg) -> Reg {
    match cur {
        Reg::Temp(t) => Reg::Temp(t + 1),
        _ => unreachable!("a cursor is a pair of temporaries"),
    }
}

/// The arguments of a call laid out by `eval_args`: the call-argument temporaries in order
/// (the frame of a call without arguments, or the lead register, first), the temporaries of the
/// `CallArg::Temp` arguments, and whether anything was lent (`Unlend` follows the call).
struct LaidArgs {
    slots: Vec<Reg>,
    temps: Vec<Reg>,
    lent: bool,
}

/// The registers of a field or element lent to a call (`Lend` / `LendPlace`;
/// `FuncLower::alloc_lending`).
struct Lending {
    /// The value taken out, or the reference to the flat place; the path follows it in
    /// consecutive registers.
    value: Reg,
    path: Vec<Reg>,
    /// The root: a reference slot (a `var` / `out` parameter's own register), or the handle of
    /// the instance whose field starts the path.
    root: Reg,
    /// The hops of the place over the path registers, once evaluated.
    hops: Vec<Hop>,
    /// The type of a flat place (lent with `LendPlace`).
    place: Option<Ty>,
    /// The register a flat static array passed to a `var` open array parameter is taken out
    /// into (`PlaceBox`).
    boxed: Option<Reg>,
}

/// The containers of a place opened for writing (`FuncLower::open_hops`).
struct Opened {
    /// The container of the last hop (holding the place's slot).
    inner: Reg,
    /// When the place is inside an element of a dynamic array: the number of the hop into the
    /// innermost such array, and the register with that array (read without counting).
    dyn_root: Option<(usize, Reg)>,
}

/// A range check still to emit for an element index (`ChkIndex`).
#[derive(Clone)]
struct IndexCheck {
    len: i64,
    /// The constant of `ChkIndex`: the type name and the bounds (`` `TArr` (0..3) ``).
    msg: String,
    span: TextRange,
}

/// The parts of a `for..in` statement that reading an element needs.
struct ForIn {
    var: ForVar,
    coll_var: VarId,
    idx_var: VarId,
    coll_ty: Ty,
    elem: Ty,
    conv: Option<Conversion>,
}
