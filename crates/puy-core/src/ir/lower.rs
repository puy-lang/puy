//! Lowering (HIR → IR; `docs/design/lifetime.md` 3.10).
//!
//! A named variable is the virtual register `vN` with the number of its `VarId`. Temporaries of
//! unmanaged types (and call arguments) are created like a stack: a statement or expression
//! releases the ones it created when it ends, and the depth at creation becomes the temporary's
//! slot (codegen turns it into a register). A temporary of a managed type gets a register of its
//! own that keeps its reference until the routine ends (`docs/design/lifetime.md` 3.4).
//!
//! An expression is lowered into a destination register given by the caller, so an assignment to a
//! local variable computes straight into its register. For a managed type the destination either
//! owns its old value or is a zeroed call argument, and every instruction that writes a managed
//! value there releases the old one (`docs/design/lifetime.md` 3.1, 3.5).

use super::*;
use crate::bytecode::{self, op};
use crate::directives::DirectiveOutput;
use crate::sema::hir::*;
use crate::sema::types::{Const, EXCEPTION, TypeId, TypeTable, bound_text, setbits};
use crate::source::SourceFile;
use crate::syntax::ast::ParamMode;

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
            if r.unit as usize == k {
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
    let mut ml = ModuleLower {
        files,
        unit: 0,
        hosts: Vec::new(),
        unit_names: p.units.iter().map(|u| u.name.clone()).collect(),
        func_index,
    };
    let funcs = order
        .into_iter()
        .map(|(r, id)| {
            ml.unit = r.unit as usize;
            FuncLower::new(&mut ml, r, id, p).lower()
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
    line: u32,
    /// (`Break` target, `Continue` target, `cleanup.len()` at the loop) of the enclosing loops.
    loops: Vec<(BlockId, BlockId, usize)>,
    /// The `except` blocks and `try..finally` bodies around the statement, outermost first.
    cleanup: Vec<Cleanup>,
    /// The handler table, inner entries first.
    tries: Vec<TryEntry>,
    /// `{$Q}` at the position of the current statement.
    current_overflow: bool,
}

impl<'m, 'a> FuncLower<'m, 'a> {
    fn new(
        ml: &'m mut ModuleLower<'a>,
        r: &'m Routine,
        id: Option<RoutineId>,
        program: &'m Program,
    ) -> Self {
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
            line: 1,
            loops: Vec::new(),
            cleanup: Vec::new(),
            tries: Vec::new(),
            current_overflow: false,
        }
    }

    fn lower(mut self) -> Function {
        if self.r.link {
            self.line = self.ml.line_of(self.r.name_span);
            self.outer_refs();
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
                let (term, term_line) = built.term.expect("every started block is terminated");
                let term = match term {
                    Term::Jmp(t) => Term::Jmp(place(t)),
                    Term::Branch {
                        op,
                        a,
                        b,
                        then,
                        other,
                    } => Term::Branch {
                        op,
                        a,
                        b,
                        then: place(then),
                        other: place(other),
                    },
                    Term::EndFinally {
                        state,
                        next,
                        targets,
                    } => Term::EndFinally {
                        state,
                        next: place(next),
                        targets: targets.into_iter().map(place).collect(),
                    },
                    t => t,
                };
                Block {
                    insts: built.insts,
                    term,
                    term_line,
                }
            })
            .collect();
        Function {
            name: format!("{}.{}", self.ml.unit_names[self.ml.unit], self.r.name),
            file: self.ml.unit as u32,
            short_name: self.r.name.clone(),
            name_span: self.r.name_span,
            params: self.r.param_count + u32::from(self.r.link),
            link: self.r.link,
            outer: self
                .r
                .parent
                .filter(|_| self.r.link)
                .map(|p| self.ml.func_index[p.0 as usize]),
            vars: self
                .r
                .vars
                .iter()
                .map(|v| VarReg {
                    name: v.name.clone(),
                    ty: v.ty,
                    kind: v.kind,
                    is_for_var: v.is_for_var,
                })
                .collect(),
            finally: self.finally_list(),
            tries: self.tries,
            temps: self.temps,
            blocks: laid_out,
        }
    }

    /// The managed registers released at the end of the routine (`docs/design/lifetime.md` 3.8):
    /// value parameters, `Result` and locals in declaration order, then the owning temporaries.
    fn finally_list(&self) -> Vec<Reg> {
        let vars = self
            .r
            .vars
            .iter()
            .enumerate()
            .filter(|(_, v)| owns_value(v));
        let temps = self
            .temps
            .iter()
            .enumerate()
            .filter(|(_, t)| t.slot.is_none() && t.kind != TempKind::State);
        vars.map(|(i, _)| Reg::Var(i as u32))
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

    /// A conditional branch to `target`; otherwise execution continues in a new block.
    fn branch(&mut self, op: u8, a: Reg, b: Option<Reg>, target: BlockId) {
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

    fn push(&mut self, op: u8, kind: InstKind) {
        let b = self.current();
        let line = self.line;
        self.blocks[b as usize].insts.push(Inst { op, kind, line });
    }

    fn plain(&mut self, op: u8, operands: Vec<Operand>) {
        self.push(op, InstKind::Plain(operands));
    }

    fn op1(&mut self, op: u8, a: Reg) {
        self.plain(op, vec![Operand::Reg(a)]);
    }

    fn op2(&mut self, op: u8, a: Reg, b: Reg) {
        self.plain(op, vec![Operand::Reg(a), Operand::Reg(b)]);
    }

    fn op3(&mut self, op: u8, a: Reg, b: Reg, c: Reg) {
        self.plain(op, vec![Operand::Reg(a), Operand::Reg(b), Operand::Reg(c)]);
    }

    /// An instruction whose last operand is an immediate (`Narrow` and friends, `AddIntI`, `StrToIntIO`).
    fn op_imm(&mut self, op: u8, a: Reg, b: Reg, imm: i64) {
        self.plain(
            op,
            vec![Operand::Reg(a), Operand::Reg(b), Operand::Imm(imm)],
        );
    }

    /// An instruction with a register and a table operand (`LoadGlobal`, `MakeGlobalRef`, ...).
    fn op_table(&mut self, op: u8, a: Reg, x: Operand) {
        self.plain(op, vec![Operand::Reg(a), x]);
    }

    fn call(&mut self, op: u8, dst: Option<Reg>, callee: Operand, args: Vec<Reg>, frame: Reg) {
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
    /// owns the value until the routine ends (`docs/design/lifetime.md` 3.4).
    fn alloc(&mut self, ty: Ty) -> Reg {
        if ty.is_managed() {
            self.temps.push(Temp {
                ty: Some(ty),
                kind: TempKind::Value,
                slot: None,
            });
            Reg::Temp(self.temps.len() as u32 - 1)
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
            Reg::Var(v) => owns_value(&self.r.vars[v as usize]),
            Reg::Temp(t) => self.temps[t as usize].slot.is_none(),
            Reg::Link => false,
        }
    }

    fn mark(&self) -> u32 {
        self.next_slot
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
        Reg::Var(v.0)
    }

    /// Whether the variable's register holds a reference slot: a `var` / `out` parameter, or a
    /// variable of an enclosing routine (`docs/spec/semantics.md` 4.9).
    fn is_ref_param(&self, v: VarId) -> bool {
        matches!(
            self.r.vars[v.0 as usize].kind,
            VarKind::Param(ParamMode::Var | ParamMode::Out) | VarKind::Outer { .. }
        )
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
            VarKind::Outer { up, var } => &self.enclosing(up).vars[var.0 as usize],
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
                let b = var.0 + u32::from(owner.link);
                let reg = Reg::Var(i as u32);
                self.plain(
                    op::OUTER_REF,
                    vec![
                        Operand::Reg(reg),
                        Operand::Imm(b as i64),
                        Operand::Imm(up as i64),
                    ],
                );
                if matches!(
                    owner.vars[var.0 as usize].kind,
                    VarKind::Param(ParamMode::Var | ParamMode::Out)
                ) {
                    self.op2(op::LOAD_REF, reg, reg);
                }
            }
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
            Const::Char(ch) if ty.is_managed() => self.load_str(dst, &[*ch]),
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
                        kind: ExprKind::Const,
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
            Const::Nil => self.load_str(dst, &[]),
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
        let saved_overflow = self.current_overflow;
        self.current_overflow = self.ml.dir().switches.at(s.span.start()).overflow_checks;
        match &s.kind {
            StmtKind::Assign { target, value, .. } => self.assign(target, value),
            StmtKind::Block(list) => {
                for x in list {
                    self.stmt(x);
                }
            }
            StmtKind::Expr(e) => match &e.kind {
                // A procedure call has no value to keep (`bytecode.md` 5.1: the destination is `-`)
                ExprKind::Call { routine, args }
                    if self.program.routines[routine.0 as usize].ret.is_none() =>
                {
                    self.call_routine(*routine, args, None)
                }
                _ => {
                    let t = self.alloc(e.ty);
                    self.expr(e, t);
                }
            },
            StmtKind::If {
                branches,
                else_body,
            } => {
                let end = self.new_block();
                for (cond, body) in branches {
                    let next = self.new_block();
                    let m = self.mark();
                    self.cond(cond, next, false);
                    self.release(m);
                    self.stmt(body);
                    self.jump(end);
                    self.bind(next);
                }
                if let Some(e) = else_body {
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
                self.cond(cond, end, false);
                self.release(m);
                self.loops.push((end, top, self.cleanup.len()));
                self.stmt(body);
                self.loops.pop();
                self.jump(top);
                self.bind(end);
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
                self.cond(cond, top, false);
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
                delta.as_ref(),
                *negative,
                *calc,
                *checks,
            ),
            StmtKind::Read { targets, newline } => self.read(targets, *newline),
            StmtKind::Flush => self.plain(op::FLUSH_OUT, Vec::new()),
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
                // The message moves into the exception object, which `Raise` hands to the VM
                self.plain(
                    op::NEW_EXC,
                    vec![
                        Operand::Reg(msg),
                        Operand::Type(Ty::Exception(TypeId(bytecode::exc::ASSERTION_FAILED))),
                    ],
                );
                self.terminate(Term::Raise(msg));
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
                let t = self.alloc(Ty::Exception(crate::sema::types::EXCEPTION));
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
        let exc = self.alloc_kind_owning(Ty::Exception(EXCEPTION), TempKind::Exception);
        let after = self.new_block();
        let body_first = self.next_position();
        let start = self.new_block();
        self.bind(start);
        for s in body {
            self.stmt(s);
        }
        self.jump(after);
        let handler_first = self.next_position();
        let dispatch = self.new_block();
        self.bind(dispatch);
        self.cleanup.push(Cleanup::Except(exc));
        for h in handlers {
            let old = self.set_line(h.span);
            let next = self.new_block();
            let m = self.mark();
            let t = self.alloc(Ty::Integer);
            self.op2(op::EXC_CLASS, t, exc);
            self.op_table(op::CLASS_IS, t, Operand::Type(h.class));
            self.branch(op::JMP_IF_FALSE, t, None, next);
            self.release(m);
            if let Some(v) = h.var {
                // `E` borrows the exception the block holds
                self.op2(op::MOVE, self.reg_of(v), exc);
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
        // The state and the exception are adjacent registers (`EndFinally A`, `r(A+1)`)
        let state = self.alloc_kind_owning(Ty::Integer, TempKind::State);
        let exc = self.alloc_kind_owning(Ty::Exception(EXCEPTION), TempKind::Exception);
        let fin_block = self.new_block();
        let after = self.new_block();
        let body_first = self.next_position();
        let start = self.new_block();
        self.bind(start);
        let depth = self.cleanup.len();
        self.cleanup.push(Cleanup::Finally {
            state,
            fin: fin_block,
            conts: Vec::new(),
        });
        for s in body {
            self.stmt(s);
        }
        if self.cur.is_some() {
            self.op1(op::LOAD_ZERO, state);
        }
        let Some(Cleanup::Finally { conts, .. }) = self.cleanup.pop() else {
            unreachable!("pushed above")
        };
        let handler_first = self.next_position();
        self.bind(fin_block);
        for s in fin {
            self.stmt(s);
        }
        // The block that ends with `EndFinally` (bound now if the last statement left none open)
        self.current();
        let handler_last_pos = self.next_position() - 1;
        self.terminate(Term::EndFinally {
            state,
            next: after,
            targets: conts.iter().map(|c| c.0).collect(),
        });
        self.tries.push(TryEntry {
            finally: true,
            body: (body_first, handler_first - 1),
            handler: (handler_first, handler_last_pos),
            reg: state,
            exc: Some(exc),
        });
        // The jumps that went through the block go on from here
        for (cont, leave) in conts {
            self.bind(cont);
            self.leave_from(depth, leave);
        }
        self.bind(after);
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
            Place::ExcMember { .. } => Ty::String,
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
        match target {
            Place::Var(..) | Place::Global(_) => {
                if let Some(reg) = self.direct_var_reg(target) {
                    self.expr_into_var(value, reg);
                } else {
                    let t = self.alloc(self.place_type(target));
                    self.expr(value, t);
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
                let old = self.set_line(*span);
                if let Some(part) = part {
                    self.part_check(&part);
                    let t = self.alloc(Ty::String);
                    self.part_load(&part, t);
                    self.op3(op::STR_SET_INDEX, t, i, v);
                    self.part_store(&part, t);
                } else if let Some(reg) = self.direct_var_reg(base) {
                    self.op3(op::STR_SET_INDEX, reg, i, v);
                } else {
                    let t = self.alloc(Ty::String);
                    self.load_place(base, t);
                    self.op3(op::STR_SET_INDEX, t, i, v);
                    self.store_place(base, t);
                }
                self.line = old;
            }
            Place::Field { .. } | Place::Elem { .. } => {
                // The indices of the target, the right-hand side, then the range checks and the
                // target itself (`docs/spec/types.md` chapter 3)
                let part = self.eval_part(target);
                let v = self.alloc(value.ty);
                self.expr(value, v);
                self.part_check(&part);
                self.part_store(&part, v);
            }
            Place::ExcMember { base, member } => {
                // The object, then the right-hand side. A call on the right could drop the
                // object, so it is then held counted
                let b = if expr_has_call(value) {
                    let t = self.alloc(base.ty);
                    self.expr(base, t);
                    t
                } else {
                    self.borrowed_value(base)
                };
                let v = self.alloc(value.ty);
                self.expr(value, v);
                self.plain(
                    op::EXC_SET,
                    vec![
                        Operand::Reg(b),
                        Operand::Imm(exc_slot(*member) as i64),
                        Operand::Reg(v),
                    ],
                );
            }
        }
    }

    /// Puts an expression into a variable's register (through a temporary if the expression reads that variable).
    fn expr_into_var(&mut self, e: &Expr, reg: Reg) {
        if expr_reads_reg(e, reg) {
            let t = self.alloc(e.ty);
            self.expr(e, t);
            let opc = if e.ty.is_managed() {
                op::MOVE_REF
            } else {
                op::MOVE
            };
            self.op2(opc, reg, t);
        } else {
            self.expr(e, reg);
        }
    }

    /// Reads the value of a place into `dst` (`Move` for a variable). A managed value is counted
    /// (`AssignRef` and the `…Ref` loads; `docs/design/lifetime.md` 3.5).
    fn load_place(&mut self, p: &Place, dst: Reg) {
        let managed = self.place_type(p).is_managed();
        match p {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    let opc = if managed {
                        op::LOAD_REF_REF
                    } else {
                        op::LOAD_REF
                    };
                    self.op2(opc, dst, r);
                } else if r != dst {
                    let opc = if managed { op::ASSIGN_REF } else { op::MOVE };
                    self.op2(opc, dst, r);
                }
            }
            Place::Global(g) => {
                let opc = if managed {
                    op::LOAD_GLOBAL_REF
                } else {
                    op::LOAD_GLOBAL
                };
                self.op_table(opc, dst, Operand::Global(g.0))
            }
            Place::StrIndex { base, index, .. } => {
                let b = self.alloc(Ty::String);
                self.load_place(base, b);
                let i = self.alloc(index.ty);
                self.expr(index, i);
                self.op3(op::STR_INDEX, dst, b, i);
            }
            Place::Field { .. } | Place::Elem { .. } => {
                let part = self.eval_part(p);
                self.part_check(&part);
                self.part_load(&part, dst);
            }
            Place::ExcMember { .. } => unreachable!("a member of an exception is only assigned"),
        }
    }

    /// Writes the value of `src` to a place. A managed value is counted (`src` keeps its reference).
    fn store_place(&mut self, p: &Place, src: Reg) {
        let managed = self.place_type(p).is_managed();
        match p {
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                if self.is_ref_param(*v) {
                    let opc = if managed {
                        op::STORE_REF_REF
                    } else {
                        op::STORE_REF
                    };
                    self.op2(opc, r, src);
                } else if r != src {
                    let opc = if managed { op::ASSIGN_REF } else { op::MOVE };
                    self.op2(opc, r, src);
                }
            }
            Place::Global(g) => {
                let opc = if managed {
                    op::STORE_GLOBAL_REF
                } else {
                    op::STORE_GLOBAL
                };
                self.op_table(opc, src, Operand::Global(g.0))
            }
            Place::StrIndex { .. } | Place::ExcMember { .. } => {
                unreachable!("string elements and exception members are not stored this way")
            }
            Place::Field { .. } | Place::Elem { .. } => {
                let part = self.eval_part(p);
                self.part_check(&part);
                self.part_store(&part, src);
            }
        }
    }

    // ---- Fields and elements (`docs/spec/types.md` chapter 3, `docs/design/lifetime.md` 3.6) ----

    /// Evaluates the indices of a field or element place, outer first, into temporaries
    /// (0-based slot numbers; the range checks come later with `part_check`).
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
                    let (reg, check) = self.elem_index(container, index, *span);
                    SlotRef::Elem { reg, check }
                };
                part.steps.push(PartStep { slot, ty: *ty });
                part
            }
            _ => Part {
                root: p,
                steps: Vec::new(),
            },
        }
    }

    /// The 0-based slot number of an element of the static array `arr` in a new temporary, and the
    /// range check it still needs (`None` for a constant index, which sema checked).
    fn elem_index(&mut self, arr: Ty, index: &Expr, span: TextRange) -> (Reg, Option<IndexCheck>) {
        let (ity, lo, hi, _) = self
            .program
            .types
            .array(arr)
            .expect("an element place indexes a static array");
        let t = self.alloc_borrow(Ty::Int64);
        if let Some(v) = index.value.as_ref().and_then(Const::as_int) {
            self.load_int(t, (v - lo) as i64);
            return (t, None);
        }
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
        (t, Some(check))
    }

    /// `ChkIndex` for each element index of the place that is not a constant.
    fn part_check(&mut self, part: &Part<'_>) {
        for step in &part.steps {
            if let SlotRef::Elem {
                reg,
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

    /// The value of the root of a place, read without counting.
    fn root_borrowed(&mut self, root: &Place) -> Reg {
        match root {
            Place::Var(v, _) if !self.is_ref_param(*v) => self.reg_of(*v),
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
            _ => unreachable!("the root of a place is a variable or a global"),
        }
    }

    /// A slot read: `FieldGet` / `ValGet`, counted (`…Ref`) when `counted` and the value is managed.
    fn get_slot(&mut self, dst: Reg, container: Reg, slot: &SlotRef, ty: Ty, counted: bool) {
        let managed = counted && ty.is_managed();
        match slot {
            SlotRef::Field(i) => {
                let opc = if managed {
                    op::FIELD_GET_REF
                } else {
                    op::FIELD_GET
                };
                self.plain(
                    opc,
                    vec![
                        Operand::Reg(dst),
                        Operand::Reg(container),
                        Operand::Imm(*i as i64),
                    ],
                );
            }
            SlotRef::Elem { reg, .. } => {
                let opc = if managed {
                    op::VAL_GET_REF
                } else {
                    op::VAL_GET
                };
                self.op3(opc, dst, container, *reg);
            }
            SlotRef::Dyn(reg) => {
                let opc = if managed {
                    op::DYN_GET_REF
                } else {
                    op::DYN_GET
                };
                self.op3(opc, dst, container, *reg);
            }
        }
    }

    /// A slot write into a unique container: `FieldSet` / `ValSet`, counted (`…Ref`) when `counted`
    /// and the value is managed.
    fn set_slot(&mut self, container: Reg, slot: &SlotRef, src: Reg, ty: Ty, counted: bool) {
        let managed = counted && ty.is_managed();
        match slot {
            SlotRef::Field(i) => {
                let opc = if managed {
                    op::FIELD_SET_REF
                } else {
                    op::FIELD_SET
                };
                self.plain(
                    opc,
                    vec![
                        Operand::Reg(container),
                        Operand::Imm(*i as i64),
                        Operand::Reg(src),
                    ],
                );
            }
            SlotRef::Elem { reg, .. } => {
                let opc = if managed {
                    op::VAL_SET_REF
                } else {
                    op::VAL_SET
                };
                self.op3(opc, container, *reg, src);
            }
            SlotRef::Dyn(reg) => {
                let opc = if managed {
                    op::DYN_SET_REF
                } else {
                    op::DYN_SET
                };
                self.op3(opc, container, *reg, src);
            }
        }
    }

    /// Reads the value of a field or element place into `dst` (counted).
    fn part_load(&mut self, part: &Part<'_>, dst: Reg) {
        let Some((last, inner)) = part.steps.split_last() else {
            return self.load_place(part.root, dst);
        };
        let mut c = self.root_borrowed(part.root);
        for step in inner {
            let t = self.alloc_borrow(step.ty);
            self.get_slot(t, c, &step.slot, step.ty, false);
            c = t;
        }
        self.get_slot(dst, c, &last.slot, last.ty, true);
    }

    /// The root of a place made unique for writing (`Unique`; `docs/design/lifetime.md` 3.6). A
    /// global or a reference parameter is read without counting, made unique and written back at
    /// once, so the slot owns the copy. Writing a reference parameter back before `Unique` too
    /// makes the containers of a cell unique first (`docs/spec/bytecode.md` 2.8, `MakeCell`).
    fn open_root(&mut self, root: &Place) -> Reg {
        let ty = self.place_type(root);
        self.modify_root(root, |this, t| {
            this.op_table(op::UNIQUE, t, Operand::Type(ty))
        })
    }

    /// Changes the value of a variable or global in place: `f` gets a register with the value read
    /// without counting, may replace it (as `Unique` and `SetLen` do: the old reference is let go
    /// only if a new value takes its place), and the register is written back without counting.
    /// Nothing else runs in between, so the counts stay right.
    fn modify_root(&mut self, root: &Place, f: impl FnOnce(&mut Self, Reg)) -> Reg {
        let ty = self.place_type(root);
        match root {
            Place::Var(v, _) if !self.is_ref_param(*v) => {
                let r = self.reg_of(*v);
                f(self, r);
                r
            }
            Place::Var(v, _) => {
                let r = self.reg_of(*v);
                let t = self.alloc_borrow(ty);
                self.op2(op::LOAD_REF, t, r);
                self.op2(op::STORE_REF, r, t);
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
            _ => unreachable!("the root of a place is a variable or a global"),
        }
    }

    /// Opens the containers of a place for writing, from the root down (`docs/design/lifetime.md`
    /// 3.6, `docs/spec/types.md` chapter 3): records and static arrays are made unique and written
    /// back, while a dynamic array is written shared, so the containers before the innermost
    /// dynamic array are only read. `inner` is the register with the innermost container (the one
    /// holding the place's slot).
    fn open_containers(&mut self, part: &Part<'_>) -> Opened {
        let containers: Vec<Ty> = std::iter::once(self.place_type(part.root))
            .chain(part.steps.iter().map(|s| s.ty))
            .take(part.steps.len())
            .collect();
        let last_dyn = containers.iter().rposition(|t| t.is_dyn_indexed());
        let (mut c, from, dyn_root) = match last_dyn {
            None => (self.open_root(part.root), 0, None),
            Some(0) if matches!(containers[0], Ty::OpenArray(_)) => {
                (self.open_open_array(part.root), 0, None)
            }
            Some(k) => {
                // The dynamic array is read without counting along the way
                let mut c = self.root_borrowed(part.root);
                for step in &part.steps[..k] {
                    let t = self.alloc_borrow(step.ty);
                    self.get_slot(t, c, &step.slot, step.ty, false);
                    c = t;
                }
                (c, k, Some((k, c)))
            }
        };
        let inner = part.steps[from..].split_last().map_or(&[][..], |(_, i)| i);
        for step in inner {
            let t = self.alloc_borrow(step.ty);
            self.get_slot(t, c, &step.slot, step.ty, false);
            self.op_table(op::UNIQUE, t, Operand::Type(step.ty));
            // The container's slot takes over the reference of the copy (if one was made)
            self.set_slot(c, &step.slot, t, step.ty, false);
            c = t;
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
        let Some(last) = part.steps.last() else {
            return self.store_place(part.root, src);
        };
        let c = self.open_containers(part).inner;
        self.set_slot(c, &last.slot, src, last.ty, true);
    }

    /// Changes the value of a field or element place in place: the slot of its opened container
    /// (like `modify_root` for a variable or global).
    fn modify_part(&mut self, part: &Part<'_>, f: impl FnOnce(&mut Self, Reg)) {
        let last = part
            .steps
            .last()
            .expect("a field or element place has a step");
        let c = self.open_containers(part).inner;
        let t = self.alloc_borrow(last.ty);
        self.get_slot(t, c, &last.slot, last.ty, false);
        f(self, t);
        self.set_slot(c, &last.slot, t, last.ty, false);
    }

    /// A cell for a field or element passed to a `var` / `out` parameter (`MakeCell`;
    /// `docs/design/lifetime.md` 3.7): the root's reference slot and the path of slot numbers, or,
    /// for a place inside an element of a dynamic array, that array's block and the path from the
    /// element. The containers are made unique first, so the cell can record their types.
    /// `materialize` makes the place's own value unique first (a static array passed to a `var`
    /// open array parameter).
    fn make_cell(&mut self, place: &Place, dst: Reg, materialize: Option<Ty>) {
        let part = self.eval_part(place);
        self.part_check(&part);
        let opened = self.open_containers(&part);
        if let Some(ty) = materialize {
            let last = part
                .steps
                .last()
                .expect("a field or element place has a step");
            let t = self.alloc_borrow(last.ty);
            self.get_slot(t, opened.inner, &last.slot, last.ty, false);
            self.op_table(op::UNIQUE, t, Operand::Type(ty));
            self.set_slot(opened.inner, &last.slot, t, last.ty, false);
        }
        let (root, steps) = match opened.dyn_root {
            Some((k, block)) => (block, &part.steps[k..]),
            None => {
                let root = self.alloc_kind(None, TempKind::Ref);
                match part.root {
                    Place::Var(v, _) if self.is_ref_param(*v) => {
                        let r = self.reg_of(*v);
                        self.op2(op::MOVE, root, r);
                    }
                    Place::Var(v, _) => {
                        let r = self.reg_of(*v);
                        self.op2(op::MAKE_REF, root, r);
                    }
                    Place::Global(g) => {
                        self.op_table(op::MAKE_GLOBAL_REF, root, Operand::Global(g.0))
                    }
                    _ => unreachable!("the root of a place is a variable or a global"),
                }
                (root, &part.steps[..])
            }
        };
        // The cell and its path are consecutive temporaries
        let cell = self.alloc_kind(None, TempKind::Value);
        let path: Vec<Reg> = steps.iter().map(|_| self.alloc_borrow(Ty::Int64)).collect();
        for (step, &p) in steps.iter().zip(&path) {
            match &step.slot {
                SlotRef::Field(i) => self.load_int(p, *i as i64),
                SlotRef::Elem { reg, .. } | SlotRef::Dyn(reg) => self.op2(op::MOVE, p, *reg),
            }
        }
        self.op1(op::LOAD_ZERO, cell);
        let mut operands = vec![
            Operand::Reg(cell),
            Operand::Reg(root),
            Operand::Imm(path.len() as i64),
        ];
        operands.extend(path.iter().map(|p| Operand::Reg(*p)));
        self.plain(op::MAKE_CELL, operands);
        // The cell lives in a register of its own until the routine ends; the argument borrows it
        self.temps.push(Temp {
            ty: None,
            kind: TempKind::Cell,
            slot: None,
        });
        let owned = Reg::Temp(self.temps.len() as u32 - 1);
        self.op2(op::MOVE_REF, owned, cell);
        self.op2(op::MOVE, dst, owned);
    }

    /// A register with the value of an expression that is only read right away: a variable's
    /// register, or a container read without counting (`docs/design/lifetime.md` 3.6). Anything
    /// that makes a value goes to an owning temporary.
    fn borrowed_value(&mut self, e: &Expr) -> Reg {
        if e.value.is_none() {
            match &e.kind {
                ExprKind::Var(v) => {
                    return self.root_borrowed(&Place::Var(*v, e.span));
                }
                ExprKind::Global(g) => return self.root_borrowed(&Place::Global(*g)),
                ExprKind::Field { base, index, .. } => {
                    let b = self.borrowed_value(base);
                    let t = self.alloc_borrow(e.ty);
                    self.get_slot(t, b, &SlotRef::Field(*index), e.ty, false);
                    return t;
                }
                ExprKind::Elem { base, index } => {
                    let b = self.index_base(base, index);
                    let (i, get) = self.read_index(base.ty, index, e.span);
                    let t = self.alloc_borrow(e.ty);
                    self.op3(get, t, b, i);
                    return t;
                }
                _ => {}
            }
        }
        let t = self.alloc(e.ty);
        self.expr(e, t);
        t
    }

    /// The array an element is read from. When evaluating the index may run code (a call), the
    /// array is read with counting first, so the code cannot free it.
    fn index_base(&mut self, base: &Expr, index: &Expr) -> Reg {
        if expr_has_call(index) {
            let t = self.alloc(base.ty);
            self.expr(base, t);
            t
        } else {
            self.borrowed_value(base)
        }
    }

    /// An element index for a read: the register with the 0-based index (range-checked for a static
    /// array; the VM checks a dynamic or open array) and the read instruction without counting.
    fn read_index(&mut self, arr: Ty, index: &Expr, span: TextRange) -> (Reg, u8) {
        if arr.is_dyn_indexed() {
            let i = self.alloc_borrow(Ty::Int64);
            self.expr(index, i);
            return (i, op::DYN_GET);
        }
        let (i, check) = self.elem_index(arr, index, span);
        if let Some(c) = check {
            self.chk_index(i, &c);
        }
        (i, op::VAL_GET)
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
        let (counter, var_ty, global): (Reg, Ty, Option<GlobalId>) = match var {
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
        let (dst, var_ty) = match f.var {
            ForVar::Local(v) => (self.reg_of(v), self.r.vars[v.0 as usize].ty),
            ForVar::Global(g) => {
                let ty = self.program.globals[g.0 as usize].ty;
                (self.alloc(ty), ty)
            }
        };
        // The element goes straight to the variable unless it is converted to a string
        let raw = match f.conv {
            Some(Conversion::CharToStr) => self.alloc_borrow(f.elem),
            _ => dst,
        };
        let managed = f.elem.is_managed();
        if f.coll_ty.is_set() {
            // The position is the element
            self.op2(op::MOVE, raw, i);
        } else {
            let opc = if f.coll_ty == Ty::String {
                op::STR_INDEX
            } else if f.coll_ty.is_dyn_indexed() {
                if managed {
                    op::DYN_GET_REF
                } else {
                    op::DYN_GET
                }
            } else if managed {
                op::VAL_GET_REF
            } else {
                op::VAL_GET
            };
            self.op3(opc, raw, c, i);
        }
        if let Some(kind) = f.conv {
            self.convert(kind, f.elem, dst, raw);
        }
        if let ForVar::Global(g) = f.var {
            let opc = if var_ty.is_managed() {
                op::STORE_GLOBAL_REF
            } else {
                op::STORE_GLOBAL
            };
            self.op_table(opc, dst, Operand::Global(g.0));
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
        let t = match (direct, &part) {
            (Some(r), _) => r,
            (None, Some(part)) => {
                let t = self.alloc(target_ty);
                self.part_load(part, t);
                t
            }
            (None, None) => {
                let t = self.alloc(target_ty);
                self.load_place(target, t);
                t
            }
        };
        let is64 = matches!(calc, Ty::Int64 | Ty::UInt64);
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
                    self.op_imm(op::ADD_INT_I, t, t, v);
                } else {
                    let d = self.alloc(calc);
                    self.load_int(d, v);
                    self.op3(op::ADD_INT, t, t, d);
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
                    (_, _, false) => op::ADD_INT,
                    (_, _, true) => op::SUB_INT,
                };
                self.op3(opc, t, t, d);
            }
        }
        self.narrow_arith(t, calc, checks.overflow);
        // From the computation type to the target type (`{$R}` of assignment): to the base type,
        // then the range of an enumeration or subrange
        let base = match self.program.types.base(target_ty) {
            Ty::Enum(_) => calc,
            b => b,
        };
        self.convert_int(t, t, calc, base, checks.range);
        if matches!(target_ty, Ty::Enum(_) | Ty::Subrange(_)) && checks.range {
            self.range_check(t, target_ty);
        }
        match (direct, &part) {
            (Some(_), _) => {}
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
                this.op2(op::STR_SET_LEN, t, regs[0]);
            } else if ty == Ty::Utf8String {
                this.op2(op::U8_SET_LEN, t, regs[0]);
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
                let opc = if ty == Ty::Utf8String {
                    op::U8_INSERT
                } else {
                    op::STR_INSERT
                };
                this.string_edit(opc, t, [(src, source.ty), (idx, Ty::Int64)]);
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
            Ty::Utf8String => this.string_edit(op::U8_DELETE, t, args),
            _ => this.string_edit(op::STR_DELETE, t, args),
        };
        self.edit_place(target, part, f);
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
    fn string_edit(&mut self, opc: u8, t: Reg, args: [(Reg, Ty); 2]) {
        let b = self.alloc_arg(Some(args[0].1));
        let c = self.alloc_arg(Some(args[1].1));
        self.op2(op::MOVE, b, args[0].0);
        self.op2(op::MOVE, c, args[1].0);
        self.op2(opc, t, b);
    }

    /// An array instruction `opc A Bx` on the array in `t` (an owning value read without counting;
    /// `A+1`, `A+2` are `args`), written back to `t`.
    fn array_edit(&mut self, opc: u8, ty: Ty, t: Reg, args: [(Reg, Ty); 2]) {
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
        for (i, (x, v)) in elems.iter().zip(vals).enumerate() {
            self.load_int(k, i as i64);
            let opc = match (dynamic, x.ty.is_managed()) {
                (true, true) => op::DYN_SET_REF,
                (true, false) => op::DYN_SET,
                (false, true) => op::VAL_SET_REF,
                (false, false) => op::VAL_SET,
            };
            self.op3(opc, a, k, v);
        }
        self.op2(op::MOVE_REF, dst, a);
    }

    /// `Read` / `ReadLn` (`docs/spec/semantics.md` 6.2): each variable from the input line buffer
    /// (`docs/spec/bytecode.md` 2.17), then `ReadLn` drops the rest of the line.
    fn read(&mut self, targets: &[Place], newline: bool) {
        if newline {
            self.plain(op::READ_LN_BEGIN, Vec::new());
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
                self.op1(op::READ_CHAR, t);
                t
            } else if matches!(ty, Ty::String | Ty::Utf8String) {
                let a = self.alloc(Ty::String);
                self.op1(op::READ_REST, a);
                if ty == Ty::Utf8String {
                    let t = self.alloc(ty);
                    self.op2(op::STR_TO_U8, t, a);
                    t
                } else {
                    a
                }
            } else {
                // A number: a word, or the rest of the line for the last variable of `ReadLn`
                // (only blanks may follow it)
                let a = self.alloc(Ty::String);
                let opc = if newline && last {
                    op::READ_REST
                } else {
                    op::READ_WORD
                };
                self.op1(opc, a);
                let t = self.alloc(ty);
                if ty.is_integer() {
                    let w = ty.width_code().unwrap_or(6) as i64;
                    self.op_imm(op::STR_TO_INT_IO, t, a, w);
                } else {
                    let kind = match ty {
                        Ty::Single => bytecode::FLOAT_IO_SINGLE,
                        Ty::Currency => bytecode::FLOAT_IO_CURRENCY,
                        _ => bytecode::FLOAT_IO_DOUBLE,
                    };
                    self.op_imm(op::STR_TO_FLOAT_IO, t, a, kind);
                }
                t
            };
            match &part {
                Some(part) => self.part_store(part, v),
                None => self.store_place(p, v),
            }
        }
        if newline {
            self.plain(op::READ_LN_END, Vec::new());
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
                None => self.string_value(&arg.value, s),
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
        let opc = match e.ty {
            Ty::Char => op::STR_FROM_CHAR,
            Ty::Boolean => op::BOOL_TO_STR,
            Ty::UInt64 => op::UINT_TO_STR,
            Ty::Double => op::F64_TO_STR,
            Ty::Single => op::F32_TO_STR,
            Ty::Currency => op::CURR_TO_STR,
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
        let opc = if e.ty == Ty::Currency {
            op::CURR_TO_FIXED
        } else {
            op::F64_TO_FIXED
        };
        self.op3(opc, dst, x, dr);
    }

    // ---- Integer helpers ----

    /// Narrows the result when the computation type is narrow (`{$Q}`).
    fn narrow_arith(&mut self, reg: Reg, calc: Ty, overflow: bool) {
        if matches!(calc, Ty::Int64 | Ty::UInt64) {
            return;
        }
        let w = calc.width_code().unwrap_or(4) as i64;
        let opc = if overflow { op::NARROW_OVF } else { op::NARROW };
        self.op_imm(opc, reg, reg, w);
    }

    /// Integer type conversion (`{$R}` of assignment compatibility). Nothing to do for a wider type.
    fn convert_int(&mut self, dst: Reg, src: Reg, from: Ty, to: Ty, range: bool) {
        let widening = || {
            let (flo, fhi) = from.range().unwrap_or((i64::MIN as i128, i64::MAX as i128));
            let (tlo, thi) = to.range().unwrap_or((i64::MIN as i128, i64::MAX as i128));
            tlo <= flo && fhi <= thi
        };
        if from == to || widening() {
            if dst != src {
                self.op2(op::MOVE, dst, src);
            }
            return;
        }
        let w = to.width_code().unwrap_or(6) as i64;
        let opc = if range { op::NARROW_RNG } else { op::NARROW };
        self.op_imm(opc, dst, src, w);
    }

    /// Converts the value of `src` (of type `from`) into `dst`. `dst` may be `src`, except for
    /// `Char` → `string`, whose result is a different register.
    fn convert(&mut self, kind: Conversion, from: Ty, dst: Reg, src: Reg) {
        match kind {
            Conversion::Int { to, check } => self.convert_int(dst, src, from, to, check),
            Conversion::CharToStr => self.op2(op::STR_FROM_CHAR, dst, src),
            Conversion::StrToU8 => self.op2(op::STR_TO_U8, dst, src),
            Conversion::U8ToStr => self.op2(op::U8_TO_STR, dst, src),
            Conversion::IntToChar { check } => {
                let opc = if check { op::NARROW_RNG } else { op::NARROW };
                self.op_imm(opc, dst, src, 3);
            }
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
        let opc = match (from, to) {
            (Ty::UInt64, Ty::Double) => op::UINT_TO_F64,
            (Ty::UInt64, Ty::Single) => op::UINT_TO_F32,
            (Ty::UInt64, _) => op::UINT_TO_CURR,
            (f, Ty::Double) if f.is_integer() => op::INT_TO_F64,
            (f, Ty::Single) if f.is_integer() => op::INT_TO_F32,
            (f, _) if f.is_integer() => op::INT_TO_CURR,
            (Ty::Single, Ty::Double) => op::F32_TO_F64,
            (Ty::Double, Ty::Single) => op::F64_TO_F32,
            (Ty::Currency, Ty::Double) => op::CURR_TO_F64,
            (Ty::Currency, Ty::Single) => op::CURR_TO_F32,
            (Ty::Double, Ty::Currency) => op::F64_TO_CURR,
            (Ty::Single, Ty::Currency) => {
                // Exact, then one rounding to four decimals
                self.op2(op::F32_TO_F64, dst, src);
                self.op2(op::F64_TO_CURR, dst, dst);
                return;
            }
            _ => unreachable!("sema converts only numbers to reals and `Currency`"),
        };
        self.op2(opc, dst, src);
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
        let old = self.set_line(e.span);
        let mark = self.mark();
        if let Some(c) = &e.value
            && !matches!(e.kind, ExprKind::Call { .. } | ExprKind::Builtin { .. })
        {
            self.load_const(dst, c, e.ty);
            self.release(mark);
            self.line = old;
            return;
        }
        match &e.kind {
            ExprKind::Const => {
                // value has already been handled above
                self.op1(op::LOAD_ZERO, dst);
            }
            ExprKind::Var(v) => self.load_place(&Place::Var(*v, e.span), dst),
            ExprKind::Global(g) => self.load_place(&Place::Global(*g), dst),
            ExprKind::Binary { first, steps } => self.binary(first, steps, dst),
            ExprKind::Unary {
                op: uop,
                operand,
                calc,
            } => {
                self.expr(operand, dst);
                match (uop, calc) {
                    (UnOp::Not, Calc::Bool) => self.op2(op::NOT_BOOL, dst, dst),
                    (UnOp::Not, Calc::Int(t)) => {
                        self.op2(op::NOT_INT, dst, dst);
                        if !matches!(t, Ty::Int64 | Ty::UInt64) {
                            let w = t.width_code().expect("narrow integer type") as i64;
                            self.op_imm(op::NARROW, dst, dst, w);
                        }
                    }
                    (UnOp::Neg, Calc::Real(Ty::Single)) => self.op2(op::NEG_F32, dst, dst),
                    (UnOp::Neg, Calc::Real(_)) => self.op2(op::NEG_F64, dst, dst),
                    (UnOp::Neg, Calc::Curr) => self.op2(op::NEG_CURR, dst, dst),
                    (UnOp::Neg, Calc::Int(t)) => {
                        let overflow = self.current_overflow;
                        if *t == Ty::Int64 && overflow {
                            self.op2(op::NEG_INT_CHK, dst, dst);
                        } else {
                            self.op2(op::NEG_INT, dst, dst);
                            self.narrow_arith(dst, *t, overflow);
                        }
                    }
                    _ => {}
                }
            }
            ExprKind::StrIndex { base, index } => {
                let b = self.expr_any(base);
                let i = self.expr_any(index);
                let opc = if base.ty == Ty::Utf8String {
                    op::U8_INDEX
                } else {
                    op::STR_INDEX
                };
                self.op3(opc, dst, b, i);
            }
            ExprKind::Field { base, index, .. } => {
                let b = self.borrowed_value(base);
                self.get_slot(dst, b, &SlotRef::Field(*index), e.ty, true);
            }
            ExprKind::Elem { base, index } => {
                let b = self.index_base(base, index);
                let (i, get) = self.read_index(base.ty, index, e.span);
                let opc = match (get, e.ty.is_managed()) {
                    (op::DYN_GET, true) => op::DYN_GET_REF,
                    (_, true) => op::VAL_GET_REF,
                    _ => get,
                };
                self.op3(opc, dst, b, i);
            }
            ExprKind::ArrayCtor(elems) => self.array_ctor(e.ty, elems, dst),
            ExprKind::Slice { base, low, high } => {
                self.slice(base, low.as_deref(), high.as_deref(), e.ty, dst)
            }
            ExprKind::Call { routine, args } => self.call_routine(*routine, args, Some(dst)),
            ExprKind::Builtin { func, args } => self.builtin(*func, args, e.ty, dst),
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
                self.expr(operand, dst);
                self.convert(*kind, operand.ty, dst, dst);
            }
            ExprKind::SetCtor(elems) => self.set_ctor(e.ty, elems, dst),
            ExprKind::NewExc { class, msg } => {
                // The message moves into the new object (`docs/spec/exceptions.md` 6.2)
                self.expr(msg, dst);
                self.op_table(op::NEW_EXC, dst, Operand::Type(*class));
            }
            ExprKind::ExcMember { base, member } => {
                let b = self.borrowed_value(base);
                let (opc, n) = match member {
                    ExcMember::StackTrace => (op::EXC_INFO, bytecode::EXC_INFO_STACK_TRACE),
                    ExcMember::Suppressed => (op::EXC_INFO, bytecode::EXC_INFO_SUPPRESSED),
                    ExcMember::ClassName => (op::EXC_INFO, bytecode::EXC_INFO_CLASS_NAME),
                    m => (op::EXC_GET, exc_slot(*m)),
                };
                self.plain(
                    opc,
                    vec![Operand::Reg(dst), Operand::Reg(b), Operand::Imm(n as i64)],
                );
            }
            ExprKind::TryStrToInt { s, target } => {
                // The value goes to the `out` argument after the conversion (`docs/spec/semantics.md` 5.3)
                let x = self.expr_any(s);
                let v = self.alloc(Ty::Integer);
                self.op3(op::TRY_STR_TO_INT, dst, x, v);
                self.store_place(target, v);
            }
            ExprKind::Error => self.op1(op::LOAD_ZERO, dst),
        }
        self.release(mark);
        self.line = old;
    }

    /// A register holding the expression's value (the variable's register for a variable, otherwise a temporary).
    fn expr_any(&mut self, e: &Expr) -> Reg {
        if e.value.is_none()
            && let ExprKind::Var(v) = &e.kind
            && !self.is_ref_param(*v)
        {
            return self.reg_of(*v);
        }
        let t = self.alloc(e.ty);
        self.expr(e, t);
        t
    }

    /// A left fold.
    fn binary(&mut self, first: &Expr, steps: &[BinStep], dst: Reg) {
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
                || s.calc == Calc::Ref
                || big_cmp(k)
        }) {
            let rest = &steps[k + 1..];
            let acc = if rest.iter().any(|s| expr_reads_reg(&s.rhs, dst)) {
                self.alloc(steps.last().map_or(first.ty, |s| s.ty))
            } else {
                dst
            };
            let lhs_ty = if k == 0 { first.ty } else { steps[k - 1].ty };
            let lhs = if k == 0 {
                self.expr_any(first)
            } else {
                let t = self.alloc(lhs_ty);
                self.binary(first, &steps[..k], t);
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
                self.fold_steps(acc, rest);
                if acc != dst {
                    self.op2(op::MOVE, dst, acc);
                }
                return;
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
            self.fold_steps(acc, rest);
            if acc != dst {
                self.op2(op::MOVE, dst, acc);
            }
            return;
        }
        // If a right operand reads the dst variable, accumulate in a temporary
        let ty = steps.last().map_or(first.ty, |s| s.ty);
        let acc = if steps.iter().any(|s| expr_reads_reg(&s.rhs, dst)) {
            self.alloc(ty)
        } else {
            dst
        };
        self.expr(first, acc);
        self.fold_steps(acc, steps);
        if acc != dst {
            let opc = if ty.is_managed() {
                op::MOVE_REF
            } else {
                op::MOVE
            };
            self.op2(opc, dst, acc);
        }
    }

    /// Applies the steps of a left fold to the accumulator `acc` (string comparisons are done by `binary`).
    fn fold_steps(&mut self, acc: Reg, steps: &[BinStep]) {
        for step in steps {
            let old = self.set_line(step.span);
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
                Calc::Ref => unreachable!("`binary` compares references"),
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
                    let r = self.expr_any(&step.rhs);
                    let unsigned = calc == Ty::UInt64;
                    if step.op.is_comparison() {
                        self.compare_int(step.op, acc, acc, r, unsigned);
                    } else {
                        let overflow = self.current_overflow;
                        self.int_arith(step.op, calc, overflow, acc, acc, r);
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
        let pick = |d: u8, s: u8| if single { s } else { d };
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

    /// An integer binary operation (the result is narrowed to the computation type).
    fn int_arith(&mut self, bop: BinOp, calc: Ty, overflow: bool, dst: Reg, a: Reg, b: Reg) {
        let is64 = matches!(calc, Ty::Int64 | Ty::UInt64);
        let unsigned = calc == Ty::UInt64;
        match bop {
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
                self.narrow_arith(dst, calc, overflow);
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
                self.narrow_arith(dst, calc, overflow);
            }
            BinOp::Mod => {
                let opc = if unsigned { op::MOD_UINT } else { op::MOD_INT };
                self.op3(opc, dst, a, b);
            }
            BinOp::And => self.op3(op::AND_INT, dst, a, b),
            BinOp::Or => self.op3(op::OR_INT, dst, a, b),
            BinOp::Xor => self.op3(op::XOR_INT, dst, a, b),
            BinOp::Shl | BinOp::Shr => {
                let left = bop == BinOp::Shl;
                if is64 {
                    self.op3(if left { op::SHL_INT } else { op::SHR_INT }, dst, a, b);
                } else {
                    self.op3(if left { op::SHL_INT32 } else { op::SHR_INT32 }, dst, a, b);
                    if calc == Ty::Cardinal {
                        self.op_imm(op::NARROW, dst, dst, 5);
                    }
                }
            }
            _ => unreachable!("not an integer operation"),
        }
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
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
                calc: Calc::Bool,
            } => self.cond(operand, target, !jump_if),
            ExprKind::Binary { first, steps }
                if steps.last().is_some_and(|s| {
                    s.op.is_comparison() && !matches!(s.calc, Calc::Set | Calc::Real(_))
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
                    Calc::Set | Calc::SetIn | Calc::Real(_) => {
                        unreachable!("the guard leaves out sets and reals")
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
    /// a managed value argument is made there (the callee owns it), a managed `const` argument is
    /// borrowed there with `Move`, and an `out` argument of a managed type is cleared before the call.
    fn call_routine(&mut self, routine: RoutineId, args: &[CallArg], dst: Option<Reg>) {
        let callee = &self.program.routines[routine.0 as usize];
        let modes: Vec<Option<ParamMode>> = (0..args.len())
            .map(|i| match callee.vars.get(i).map(|v| v.kind) {
                Some(VarKind::Param(m)) => Some(m),
                _ => None,
            })
            .collect();
        let param_tys: Vec<Ty> = (0..args.len())
            .map(|i| callee.vars.get(i).map_or(Ty::Error, |v| v.ty))
            .collect();
        let ret = callee.ret;
        // A nested routine taking the static link gets it first: a reference to the frame of the
        // routine it is nested in, this one or one `up` levels out (`docs/spec/semantics.md` 4.9)
        let link = callee.link.then(|| {
            let parent = callee
                .parent
                .expect("a routine with a static link is nested");
            let mut chain =
                std::iter::successors(self.id, |r| self.program.routines[r.0 as usize].parent);
            let up = chain
                .position(|r| r == parent)
                .expect("a nested routine is called from inside the routine it is nested in");
            let t = self.alloc_kind(None, TempKind::Ref);
            (t, up as i64)
        });
        // The arguments are laid out in consecutive temporaries; with no arguments the frame still gets one
        let mut slots = Vec::with_capacity(args.len().max(1) + 1);
        for a in args {
            let t = match a {
                CallArg::Value(e) => self.alloc_arg(Some(e.ty)),
                CallArg::Ref { .. } => self.alloc_kind(None, TempKind::Ref),
            };
            slots.push(t);
        }
        if slots.is_empty() && link.is_none() {
            slots.push(self.alloc_arg(ret));
        }
        for (i, (a, &reg)) in args.iter().zip(&slots).enumerate() {
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
                    if modes[i] == Some(ParamMode::Const) {
                        let t = self.alloc(ty);
                        self.expr(e, t);
                        self.op_table(op::MATERIALIZE, t, Operand::Type(ty));
                        self.op2(op::MOVE, reg, t);
                    } else {
                        self.value_arg(e, reg);
                        self.op_table(op::MATERIALIZE, reg, Operand::Type(ty));
                    }
                }
                CallArg::Value(e) if e.ty.is_managed() && modes[i] == Some(ParamMode::Const) => {
                    self.borrow_arg(e, reg, args)
                }
                CallArg::Value(e) => self.value_arg(e, reg),
                CallArg::Ref { place, is_out } => {
                    self.make_ref(place, reg, open_static);
                    if *is_out && self.place_type(place).is_managed() {
                        self.op1(op::CLEAR_REF, reg);
                    }
                }
            }
        }
        let mut frame = slots.first().copied();
        slots.truncate(args.len());
        if let Some((t, up)) = link {
            self.plain(
                op::OUTER_REF,
                vec![Operand::Reg(t), Operand::Imm(0), Operand::Imm(up)],
            );
            slots.insert(0, t);
            frame = Some(t);
        }
        let frame = frame.expect("a call has a frame");
        let f = self.ml.func_index[routine.0 as usize];
        match dst {
            // The result comes back in the frame; move its reference to the destination
            Some(d) if ret.is_some_and(Ty::is_managed) => {
                self.call(op::CALL, Some(frame), Operand::Func(f), slots, frame);
                self.op2(op::MOVE_REF, d, frame);
            }
            _ => self.call(op::CALL, dst, Operand::Func(f), slots, frame),
        }
    }

    /// Puts the value of an argument into a call argument temporary. A managed value is made there
    /// from 0 (the temporary may hold an integer or a borrowed reference from an earlier call).
    fn value_arg(&mut self, e: &Expr, slot: Reg) {
        if e.ty.is_managed() {
            self.op1(op::LOAD_ZERO, slot);
            let empty = matches!(&e.value, Some(Const::Str(s)) if s.is_empty());
            if empty {
                return;
            }
        }
        self.expr(e, slot);
    }

    /// Puts a borrowed managed value into a call argument temporary (`const` arguments and the
    /// register ranges of built-in instructions; `docs/design/lifetime.md` 3.3). A local variable
    /// that nothing in `siblings` can write during the call is lent as it is; anything else is
    /// first copied (counted) into an owning temporary, which lends it.
    fn borrow_arg(&mut self, e: &Expr, slot: Reg, siblings: &(impl WritesVar + ?Sized)) {
        if e.value.is_none()
            && let ExprKind::Var(v) = &e.kind
            && !self.is_ref_param(*v)
            && !siblings.writes(*v)
            // A nested routine may write it during the call (`docs/spec/semantics.md` 4.9)
            && !self.r.vars[v.0 as usize].captured
        {
            let r = self.reg_of(*v);
            self.op2(op::MOVE, slot, r);
            return;
        }
        let t = self.alloc(e.ty);
        self.expr(e, t);
        self.op2(op::MOVE, slot, t);
    }

    /// Creates a reference slot for a `var` / `out` argument in `dst`. `materialize` is the static
    /// array type of a place passed to a `var` open array parameter: the place's value is made a
    /// unique block first (the callee cannot know the length of a default value, 0).
    fn make_ref(&mut self, place: &Place, dst: Reg, materialize: Option<Ty>) {
        if let Some(ty) = materialize {
            if is_part(place) {
                return self.make_cell(place, dst, Some(ty));
            }
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
            Place::StrIndex { .. } | Place::ExcMember { .. } => {
                unreachable!("string elements and exception members cannot be var arguments")
            }
            Place::Field { .. } | Place::Elem { .. } => self.make_cell(place, dst, None),
        }
    }

    /// Evaluates the arguments into consecutive temporaries (an instruction reading a register range).
    /// The instruction only reads them, so managed values are borrowed.
    fn args_range(&mut self, args: &[Expr]) -> Vec<Reg> {
        let regs: Vec<Reg> = args.iter().map(|a| self.alloc_arg(Some(a.ty))).collect();
        for (a, &r) in args.iter().zip(&regs) {
            if a.ty.is_managed() {
                self.borrow_arg(a, r, args);
            } else {
                self.expr(a, r);
            }
        }
        regs
    }

    /// A built-in function of result type `ty`.
    fn builtin(&mut self, func: Builtin, args: &[Expr], ty: Ty, dst: Reg) {
        match func {
            Builtin::Length | Builtin::High => {
                let s = self.expr_any(&args[0]);
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
                let opc = match func {
                    Builtin::Copy if u8 => op::U8_COPY,
                    Builtin::Pos if u8 => op::U8_POS,
                    Builtin::Copy => op::STR_COPY,
                    Builtin::Pos => op::STR_POS,
                    _ => op::STR_TO_INT_DEF,
                };
                let mut operands = vec![Operand::Reg(dst)];
                operands.extend(self.args_range(args).into_iter().map(Operand::Reg));
                self.plain(opc, operands);
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
                self.op2(op::NEG_CURR, dst, dst);
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
                        self.int_arith(BinOp::Mul, ty, overflow, dst, x, x);
                    }
                }
            }
            Builtin::Sqrt => {
                self.expr(&args[0], dst);
                self.op2(op::SQRT_F64, dst, dst);
            }
            Builtin::Pi => unreachable!("sema folds `Pi`"),
            Builtin::Trunc | Builtin::Round => {
                let x = &args[0];
                self.expr(x, dst);
                let round = func == Builtin::Round;
                match x.ty {
                    Ty::Currency => {
                        let opc = if round {
                            op::ROUND_CURR
                        } else {
                            op::TRUNC_CURR
                        };
                        self.op2(opc, dst, dst);
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
                let opc = if args[0].ty == Ty::Single {
                    op::F32_TO_STR
                } else {
                    op::F64_TO_STR
                };
                self.op2(opc, dst, x);
            }
            Builtin::StrToFloat => {
                let x = self.expr_any(&args[0]);
                self.op2(op::STR_TO_F64, dst, x);
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
            Builtin::ParamCount => self.op1(op::ARG_COUNT, dst),
            Builtin::Eof => self.op1(op::IN_EOF, dst),
            Builtin::Eoln => self.op1(op::IN_EOLN, dst),
            Builtin::ParamStr => {
                let i = self.expr_any(&args[0]);
                self.op2(op::ARG_GET, dst, i);
            }
            Builtin::IntToStr
            | Builtin::UIntToStr
            | Builtin::StrToInt
            | Builtin::StrToInt64
            | Builtin::UpperCase
            | Builtin::LowerCase
            | Builtin::Trim => {
                let x = self.expr_any(&args[0]);
                let opc = match func {
                    Builtin::IntToStr => op::INT_TO_STR,
                    Builtin::UIntToStr => op::UINT_TO_STR,
                    Builtin::StrToInt => op::STR_TO_INT,
                    Builtin::StrToInt64 => op::STR_TO_INT64,
                    Builtin::UpperCase => op::STR_UPPER_ASCII,
                    Builtin::LowerCase => op::STR_LOWER_ASCII,
                    _ => op::STR_TRIM,
                };
                self.op2(opc, dst, x);
            }
            Builtin::Default => {
                // The default value of every type is 0 (`docs/design/lifetime.md` 3.2)
                let opc = if self.owns(dst) {
                    op::RELEASE
                } else {
                    op::LOAD_ZERO
                };
                self.op1(opc, dst);
            }
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
            Builtin::TryStrToInt => {
                unreachable!("sema makes `TryStrToInt` an expression of its own")
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
                self.op3(op::INT_TO_HEX, dst, v, digits);
            }
            Builtin::StrToBool | Builtin::TrimLeft | Builtin::TrimRight | Builtin::QuotedStr => {
                let x = self.expr_any(&args[0]);
                let opc = match func {
                    Builtin::StrToBool => op::STR_TO_BOOL,
                    Builtin::TrimLeft => op::STR_TRIM_LEFT,
                    Builtin::TrimRight => op::STR_TRIM_RIGHT,
                    _ => op::QUOTED_STR,
                };
                self.op2(opc, dst, x);
            }
            Builtin::BoolToStr
            | Builtin::SameText
            | Builtin::CompareText
            | Builtin::CompareStr
            | Builtin::StringOfChar => {
                let a = self.expr_any(&args[0]);
                let b = self.expr_any(&args[1]);
                let opc = match func {
                    Builtin::BoolToStr => op::BOOL_STR,
                    Builtin::SameText => op::SAME_TEXT,
                    Builtin::CompareText => op::COMPARE_TEXT,
                    Builtin::CompareStr => op::CMP_STR,
                    _ => op::STR_OF_CHAR,
                };
                self.op3(opc, dst, a, b);
            }
            Builtin::AnsiUpperCase | Builtin::AnsiLowerCase => {
                let s = self.expr_any(&args[0]);
                let mapping = self.expr_any(&args[1]);
                let opc = if func == Builtin::AnsiUpperCase {
                    op::STR_UPPER
                } else {
                    op::STR_LOWER
                };
                self.op3(opc, dst, s, mapping);
            }
            Builtin::StringReplace | Builtin::AnsiSameText | Builtin::AnsiCompareText => {
                let opc = match func {
                    Builtin::StringReplace => op::STR_REPLACE,
                    Builtin::AnsiSameText => op::ANSI_SAME_TEXT,
                    _ => op::ANSI_COMPARE_TEXT,
                };
                let mut operands = vec![Operand::Reg(dst)];
                operands.extend(self.args_range(args).into_iter().map(Operand::Reg));
                self.plain(opc, operands);
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
        self.borrow_arg(&args[0], fmt, args);
        for (a, &(kind, value)) in args[1..].iter().zip(&pairs) {
            self.load_int(kind, format_kind(a.ty));
            if a.ty.is_managed() {
                self.borrow_arg(a, value, args);
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
        if let Ty::Array(_) = src.ty {
            let t = self.alloc(src.ty);
            self.expr(src, t);
            self.op_table(op::MATERIALIZE, t, Operand::Type(src.ty));
            self.op2(op::MOVE, regs[0], t);
        } else {
            self.borrow_arg(src, regs[0], args);
        }
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
        if fixed.is_some() {
            let t = self.alloc(base.ty);
            self.expr(base, t);
            self.op_table(op::MATERIALIZE, t, Operand::Type(base.ty));
            self.op2(op::MOVE, regs[0], t);
        } else {
            self.borrow_arg(base, regs[0], &bounds[..]);
        }
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
        self.op2(op::SLICE_RANGE, len, regs[1]);
        if is_str {
            self.op_imm(op::ADD_INT_I, regs[1], regs[1], 1);
            let opc = if base.ty == Ty::Utf8String {
                op::U8_COPY
            } else {
                op::STR_COPY
            };
            let operands = std::iter::once(dst).chain(regs[..3].iter().copied());
            self.plain(opc, operands.map(Operand::Reg).collect());
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
/// than a `const` parameter or the `E` of `on E` (borrowed) or a `var` / `out` parameter (a
/// reference slot).
fn owns_value(v: &Var) -> bool {
    v.ty.is_managed()
        && !matches!(
            v.kind,
            VarKind::Param(ParamMode::Const | ParamMode::Var | ParamMode::Out)
                | VarKind::Exception
                | VarKind::Outer { .. }
        )
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
        CallArg::Value(e) => expr_writes_var(e, v),
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

/// Whether evaluating the expression can write the local variable `v` (it is passed to a `var` /
/// `out` parameter somewhere inside).
fn expr_writes_var(e: &Expr, v: VarId) -> bool {
    match &e.kind {
        ExprKind::Var(_) | ExprKind::Const | ExprKind::Global(_) | ExprKind::Error => false,
        ExprKind::Binary { first, steps } => {
            expr_writes_var(first, v) || steps.iter().any(|s| expr_writes_var(&s.rhs, v))
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Convert { operand, .. }
        | ExprKind::NewExc { msg: operand, .. }
        | ExprKind::ExcMember { base: operand, .. } => expr_writes_var(operand, v),
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            expr_writes_var(base, v) || expr_writes_var(index, v)
        }
        ExprKind::Field { base, .. } => expr_writes_var(base, v),
        ExprKind::Slice { base, low, high } => {
            expr_writes_var(base, v) || low.iter().chain(high.iter()).any(|b| expr_writes_var(b, v))
        }
        ExprKind::Call { args, .. } => args.iter().any(|a| call_arg_writes_var(a, v)),
        ExprKind::Builtin { args, .. } | ExprKind::ArrayCtor(args) => {
            args.iter().any(|a| expr_writes_var(a, v))
        }
        ExprKind::SetCtor(elems) => set_ctor_exprs(elems).any(|a| expr_writes_var(a, v)),
        ExprKind::TryStrToInt { s, target } => {
            expr_writes_var(s, v) || place_root(target) == Some(v)
        }
    }
}

/// The bounds of the elements of a set constructor, in order.
fn set_ctor_exprs(elems: &[SetCtorElem]) -> impl Iterator<Item = &Expr> {
    elems
        .iter()
        .flat_map(|e| std::iter::once(&e.low).chain(e.high.as_ref()))
}

/// Whether the expression reads the variable `reg`.
fn expr_reads_reg(e: &Expr, reg: Reg) -> bool {
    match &e.kind {
        ExprKind::Var(v) => Reg::Var(v.0) == reg,
        ExprKind::Const | ExprKind::Global(_) | ExprKind::Error => false,
        ExprKind::Binary { first, steps } => {
            expr_reads_reg(first, reg) || steps.iter().any(|s| expr_reads_reg(&s.rhs, reg))
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Convert { operand, .. }
        | ExprKind::NewExc { msg: operand, .. }
        | ExprKind::ExcMember { base: operand, .. } => expr_reads_reg(operand, reg),
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            expr_reads_reg(base, reg) || expr_reads_reg(index, reg)
        }
        ExprKind::Field { base, .. } => expr_reads_reg(base, reg),
        ExprKind::Slice { base, low, high } => {
            expr_reads_reg(base, reg)
                || low
                    .iter()
                    .chain(high.iter())
                    .any(|b| expr_reads_reg(b, reg))
        }
        ExprKind::Call { args, .. } => args.iter().any(|a| match a {
            CallArg::Value(v) => expr_reads_reg(v, reg),
            CallArg::Ref { place, .. } => place_reads_reg(place, reg),
        }),
        ExprKind::Builtin { args, .. } | ExprKind::ArrayCtor(args) => {
            args.iter().any(|a| expr_reads_reg(a, reg))
        }
        ExprKind::SetCtor(elems) => set_ctor_exprs(elems).any(|a| expr_reads_reg(a, reg)),
        ExprKind::TryStrToInt { s, target } => {
            expr_reads_reg(s, reg) || place_reads_reg(target, reg)
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
        Place::ExcMember { base, .. } => expr_reads_reg(base, reg),
    }
}

/// Whether evaluating the expression may run code (a routine or host call), which could change
/// any variable.
fn expr_has_call(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Call { .. } => true,
        ExprKind::Builtin { func, args } => {
            *func == Builtin::GetEnvironmentVariable || args.iter().any(expr_has_call)
        }
        ExprKind::Var(_) | ExprKind::Const | ExprKind::Global(_) | ExprKind::Error => false,
        ExprKind::Binary { first, steps } => {
            expr_has_call(first) || steps.iter().any(|s| expr_has_call(&s.rhs))
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Convert { operand, .. }
        | ExprKind::NewExc { msg: operand, .. }
        | ExprKind::ExcMember { base: operand, .. } => expr_has_call(operand),
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            expr_has_call(base) || expr_has_call(index)
        }
        ExprKind::Field { base, .. } => expr_has_call(base),
        ExprKind::Slice { base, low, high } => {
            expr_has_call(base) || low.iter().chain(high.iter()).any(|b| expr_has_call(b))
        }
        ExprKind::ArrayCtor(args) => args.iter().any(expr_has_call),
        ExprKind::SetCtor(elems) => set_ctor_exprs(elems).any(expr_has_call),
        ExprKind::TryStrToInt { s, target } => expr_has_call(s) || place_has_call(target),
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
        Place::ExcMember { base, .. } => expr_has_call(base),
    }
}

/// The slot of an exception object a member reads (`docs/spec/exceptions.md` 6.1).
fn exc_slot(m: ExcMember) -> u32 {
    match m {
        ExcMember::Message => bytecode::EXC_SLOT_MESSAGE,
        ExcMember::InnerException => bytecode::EXC_SLOT_INNER,
        _ => bytecode::EXC_SLOT_EXTRA,
    }
}

/// Whether a place is a field or element (written through its containers).
fn is_part(p: &Place) -> bool {
    matches!(p, Place::Field { .. } | Place::Elem { .. })
}

/// A field or element place with its indices evaluated (`FuncLower::eval_part`).
struct Part<'p> {
    /// The variable or global the place starts from.
    root: &'p Place,
    /// From the root outward.
    steps: Vec<PartStep>,
}

struct PartStep {
    slot: SlotRef,
    /// The type of the slot's value.
    ty: Ty,
}

/// A slot of a record (a field number), of a static array (a register with the 0-based index) or
/// of a dynamic or open array (a register with the index, checked by the VM).
enum SlotRef {
    Field(u32),
    Elem { reg: Reg, check: Option<IndexCheck> },
    Dyn(Reg),
}

/// The containers of a place opened for writing (`FuncLower::open_containers`).
struct Opened {
    /// The innermost container (holding the place's slot).
    inner: Reg,
    /// When the place is inside an element of a dynamic array: the number of the step that indexes
    /// the innermost such array, and the register with that array (read without counting).
    dyn_root: Option<(usize, Reg)>,
}

/// A range check still to emit for an element index (`ChkIndex`).
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
