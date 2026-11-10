//! Control-flow analysis (`docs/spec/semantics.md` 4.4, 4.6): definite assignment (E2000–E2004),
//! unused variables (W0001), unused parameters (W0017), unreachable code (W0003). Unused values
//! (W0002) are in `values`.

use super::hir::*;
use super::types::{Ty, TypeTable};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{FileId, Span, TextRange, TextSize};
use crate::syntax::SyntaxKind;
use crate::syntax::ast::{self, AstNode};

/// The set of assigned variables.
type State = Vec<bool>;

#[derive(Clone)]
struct Cursor {
    state: State,
    reachable: bool,
}

struct LoopCtx {
    breaks: Vec<State>,
    continues: Vec<State>,
}

/// A jump that may pass through `finally` blocks on its way.
#[derive(Clone, Copy)]
enum Jump {
    /// `Exit` (the position, for `returns here`).
    Exit(TextRange),
    /// `Break` / `Continue` of the loop at this index of `Flow::loops`.
    Break(usize),
    Continue(usize),
}

/// The body of a `try..finally` being analyzed: jumps out of it run the `finally` block first, so
/// its assignments count on their way (`docs/spec/exceptions.md` 5.7).
struct FinallyCtx {
    /// `Flow::loops.len()` at the `try` (loops below it are outside the `finally`).
    loops_at_entry: usize,
    pending: Vec<(Jump, State)>,
}

/// What a call of a nested routine does to the variables of the routine whose definite
/// assignment is being checked (`docs/spec/semantics.md` 4.9): the variables it may read before
/// assigning them (directly or through the nested routines it calls), and those it assigns on
/// every path that returns (`None`: all of them, as it never returns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub reads: Vec<VarId>,
    pub writes: Option<Vec<VarId>>,
}

/// The effects of the nested routines, keyed by routine.
pub type Effects = std::collections::HashMap<RoutineId, Effect>;

/// Working out the effect of a nested routine (`summary`): the variables of the routine being
/// checked (the target) get state slots after the routine's own variables.
struct Summary {
    /// The number of the routine's own variables (the target's variable `v` is slot `own + v`).
    own: usize,
    /// For each variable of the routine, the slot of the target's variable it refers to.
    alias: Vec<Option<usize>>,
    /// The target's variables read before they are assigned.
    reads: Vec<bool>,
    /// The target's variables assigned at every return so far (`None`: no return yet).
    writes: Option<Vec<bool>>,
}

struct Flow<'a> {
    r: &'a Routine,
    effects: &'a Effects,
    summary: Option<Summary>,
    types: &'a TypeTable,
    file: FileId,
    bytes: &'a [u8],
    diags: Vec<Diagnostic>,
    /// Read in reachable code.
    reads: Vec<bool>,
    /// Number of references (reads and writes).
    refs: Vec<u32>,
    /// Unassigned after the `for` (the `FOR_VAR` position).
    tainted: Vec<Option<TextRange>>,
    /// Variable reads recorded by semantic analysis (with positions). Used to pick up reads inside error-typed expressions.
    sema_reads: &'a [(VarId, TextRange)],
    /// Those of `sema_reads` seen by this analysis.
    seen: Vec<bool>,
    /// Read where the tree does not show it (`sema_reads` not seen, in reachable code).
    unseen_read: Vec<bool>,
    /// Ranges of unreachable code (W0003).
    unreachable: Vec<TextRange>,
    loops: Vec<LoopCtx>,
    finallies: Vec<FinallyCtx>,
    result_return_reported: bool,
    out_return_reported: Vec<bool>,
}

/// `unused_param_on` marks the parameters where W0017 is enabled. `sema_reads` are the variable reads recorded by semantic analysis (with positions). Reads inside expressions that did not
/// make it into the tree (error-typed expressions, discarded statements) also count as reads if they are in reachable code (4.6).
pub fn analyze(
    r: &Routine,
    types: &TypeTable,
    file: FileId,
    bytes: &[u8],
    sema_reads: &[(VarId, TextRange)],
    unused_param_on: &[bool],
    effects: &Effects,
) -> Vec<Diagnostic> {
    let n = r.vars.len();
    let mut f = Flow {
        r,
        effects,
        summary: None,
        types,
        file,
        bytes,
        diags: Vec::new(),
        reads: vec![false; n],
        refs: vec![0; n],
        tainted: vec![None; n],
        sema_reads,
        seen: vec![false; sema_reads.len()],
        unseen_read: vec![false; n],
        unreachable: Vec::new(),
        loops: Vec::new(),
        finallies: Vec::new(),
        result_return_reported: false,
        out_return_reported: vec![false; n],
    };
    let mut cur = Cursor {
        state: initial_state(r),
        reachable: true,
    };
    f.stmts(&r.body, &mut cur);
    if cur.reachable {
        f.check_return(&cur.state, r.end_span);
    }
    f.count_unseen_reads();
    f.unused_variables();
    f.unused_parameters();
    // W0001 alone for a variable whose value is never read, and W0017 alone for such a
    // parameter where W0017 is enabled; no W0002 for a variable read where the tree does not show
    // it (the reads cannot be placed on a path)
    // A variable a nested routine uses is not followed by W0002 (its value may be read or
    // replaced by any call; `docs/spec/semantics.md` 4.9)
    let excluded: Vec<bool> = (0..n)
        .map(|i| {
            let v = &r.vars[i];
            let never_read = !(f.reads[i] || v.read_by_nested)
                && (v.kind == VarKind::Local || unused_param_on[i]);
            never_read || f.unseen_read[i] || v.captured
        })
        .collect();
    let mut diags = f.diags;
    diags.extend(super::values::analyze(r, file, sema_reads, &excluded));
    diags
}

/// Whether each variable is assigned on entry. A variable of an enclosing routine is: its reads
/// and assignments are checked at the calls of the nested routine (`docs/spec/semantics.md` 4.9).
fn initial_state(r: &Routine) -> State {
    r.vars
        .iter()
        .map(|v| match v.kind {
            VarKind::Param(ast::ParamMode::Out) => false,
            VarKind::Param(_) => true,
            VarKind::Result => false,
            VarKind::Local => false,
            VarKind::Hidden | VarKind::Exception | VarKind::Outer { .. } => true,
        })
        .collect()
}

/// The effects of the nested routines `group` on the variables of `target` (`target_vars` of
/// them; `docs/spec/semantics.md` 4.9). Each entry is a nested routine with how many routines out
/// `target` is from it. Calls can be mutual, so the effects are worked out again until they stop
/// changing, starting from reading nothing and assigning everything.
pub fn nested_effects(
    group: &[(RoutineId, Option<&Routine>, u32)],
    target_vars: usize,
    types: &TypeTable,
    file: FileId,
    bytes: &[u8],
) -> Effects {
    let mut effects: Effects = group
        .iter()
        .map(|&(id, _, _)| {
            (
                id,
                Effect {
                    reads: Vec::new(),
                    writes: None,
                },
            )
        })
        .collect();
    loop {
        let mut changed = false;
        for &(id, r, up) in group {
            // A routine without a body or with syntax errors (not analyzed) keeps the start
            let Some(r) = r.filter(|r| !r.has_syntax_errors) else {
                continue;
            };
            let e = summarize(r, up, target_vars, &effects, types, file, bytes);
            if effects.get(&id) != Some(&e) {
                effects.insert(id, e);
                changed = true;
            }
        }
        if !changed {
            return effects;
        }
    }
}

/// The effect of one nested routine `r` on the variables of the routine `up` levels out.
fn summarize(
    r: &Routine,
    up: u32,
    target_vars: usize,
    effects: &Effects,
    types: &TypeTable,
    file: FileId,
    bytes: &[u8],
) -> Effect {
    let n = r.vars.len();
    let alias = r
        .vars
        .iter()
        .map(|v| match v.kind {
            VarKind::Outer { up: u, var } if u == up => Some(n + var.0 as usize),
            _ => None,
        })
        .collect();
    let mut f = Flow {
        r,
        effects,
        summary: Some(Summary {
            own: n,
            alias,
            reads: vec![false; target_vars],
            writes: None,
        }),
        types,
        file,
        bytes,
        diags: Vec::new(),
        reads: vec![false; n],
        refs: vec![0; n],
        tainted: vec![None; n],
        sema_reads: &[],
        seen: Vec::new(),
        unseen_read: vec![false; n],
        unreachable: Vec::new(),
        loops: Vec::new(),
        finallies: Vec::new(),
        result_return_reported: false,
        out_return_reported: vec![false; n],
    };
    let mut state = initial_state(r);
    state.extend(std::iter::repeat_n(false, target_vars));
    let mut cur = Cursor {
        state,
        reachable: true,
    };
    f.stmts(&r.body, &mut cur);
    if cur.reachable {
        f.check_return(&cur.state, r.end_span);
    }
    let s = f.summary.expect("set above");
    let ids = |set: &[bool]| -> Vec<VarId> {
        set.iter()
            .enumerate()
            .filter(|(_, b)| **b)
            .map(|(i, _)| VarId(i as u32))
            .collect()
    };
    Effect {
        reads: ids(&s.reads),
        writes: s.writes.as_deref().map(ids),
    }
}

fn span(file: FileId, range: TextRange) -> Span {
    Span::new(file, range)
}

/// The text of a type's default value in a fix: `0`, `''`, `Default(TPoint)`
/// (`docs/spec/types.md` chapter 6). `None` for an anonymous record or static array, which has no
/// name to write.
fn zero_text(ty: Ty, types: &TypeTable) -> Option<String> {
    Some(match ty {
        Ty::Boolean => "False".into(),
        Ty::Char => "#0".into(),
        Ty::String | Ty::Utf8String => "''".into(),
        Ty::Record(id) | Ty::Array(id) => format!("Default({})", types.get(id).name.as_ref()?),
        Ty::DynArray(_) | Ty::Exception(_) => "nil".into(),
        _ => "0".into(),
    })
}

impl<'a> Flow<'a> {
    /// The state slot of a variable: for a variable of the target routine while working out an
    /// effect, the target's slot.
    fn slot(&self, id: VarId) -> usize {
        let i = id.0 as usize;
        match &self.summary {
            Some(s) => s.alias[i].unwrap_or(i),
            None => i,
        }
    }

    fn mark_assigned(&mut self, id: VarId, cur: &mut Cursor) {
        let s = self.slot(id);
        cur.state[s] = true;
    }

    /// The slot of the target's variable `v` (a variable of an enclosing routine while working
    /// out an effect; otherwise the routine's own).
    fn target_slot(&self, v: VarId) -> usize {
        self.summary.as_ref().map_or(0, |s| s.own) + v.0 as usize
    }

    /// A call of a nested routine: its reads and assignments of the target's variables
    /// (`docs/spec/semantics.md` 4.9).
    fn call_effect(&mut self, routine: RoutineId, at: TextRange, cur: &mut Cursor) {
        let Some(e) = self.effects.get(&routine) else {
            return;
        };
        for &v in &e.reads {
            if self.summary.is_some() {
                let slot = self.target_slot(v);
                if !cur.state[slot]
                    && let Some(s) = &mut self.summary
                {
                    s.reads[v.0 as usize] = true;
                }
            } else {
                self.read_as(v, at, "read by this call", cur);
            }
        }
        match &e.writes {
            Some(writes) => {
                for &v in writes {
                    let slot = self.target_slot(v);
                    cur.state[slot] = true;
                }
            }
            None => {
                let start = self.target_slot(VarId(0));
                for st in &mut cur.state[start..] {
                    *st = true;
                }
            }
        }
    }

    fn stmts(&mut self, list: &[Stmt], cur: &mut Cursor) {
        for (i, s) in list.iter().enumerate() {
            if !cur.reachable {
                let start = s.span.start();
                let end = list.last().unwrap().span.end();
                let range = TextRange::new(start, end);
                let d = Diagnostic::warning(Code::W(3), "unreachable code", Stage::Flow)
                    .with_primary(span(self.file, range), "");
                self.diags.push(d);
                self.unreachable.push(range);
                return;
            }
            self.stmt(s, cur);
            let _ = i;
        }
    }

    /// The loop variable and body of a `for` / `for..in` (after the bounds or the collection):
    /// the variable is assigned inside the body and unassigned after the loop (E2004).
    fn for_loop(&mut self, var: &ForVar, var_span: TextRange, body: &Stmt, cur: &mut Cursor) {
        if let ForVar::Local(id) = var {
            let i = id.0 as usize;
            cur.state[i] = true;
            self.tainted[i] = None;
            self.refs[i] += 1;
        }
        self.loops.push(LoopCtx {
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        let mut b = cur.clone();
        self.stmt(body, &mut b);
        let ctx = self.loops.pop().unwrap();
        for st in &ctx.breaks {
            and_state(&mut cur.state, st);
        }
        if let ForVar::Local(id) = var {
            let i = id.0 as usize;
            cur.state[i] = false;
            self.tainted[i] = Some(var_span);
        }
        cur.reachable = true;
    }

    fn stmt(&mut self, s: &Stmt, cur: &mut Cursor) {
        match &s.kind {
            StmtKind::Assign {
                target,
                value,
                is_init,
                ..
            } => {
                self.expr(value, cur);
                self.place_write(target, cur);
                if *is_init && let Place::Var(id, _) = target {
                    self.refs[id.0 as usize] -= 1;
                }
            }
            StmtKind::Block(list) => self.stmts(list, cur),
            StmtKind::Expr(e) => self.expr(e, cur),
            StmtKind::If {
                branches,
                else_body,
            } => {
                let mut c = cur.clone();
                let mut merged: Option<Cursor> = None;
                for (cond, body) in branches {
                    self.expr(cond, &mut c);
                    let mut b = c.clone();
                    self.stmt(body, &mut b);
                    merge_into(&mut merged, b);
                }
                match else_body {
                    Some(e) => {
                        let mut b = c.clone();
                        self.stmt(e, &mut b);
                        merge_into(&mut merged, b);
                    }
                    None => merge_into(&mut merged, c),
                }
                *cur = merged.unwrap();
            }
            StmtKind::While { cond, body } => {
                self.expr(cond, cur);
                self.loops.push(LoopCtx {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                });
                let mut b = cur.clone();
                self.stmt(body, &mut b);
                let ctx = self.loops.pop().unwrap();
                for st in &ctx.breaks {
                    and_state(&mut cur.state, st);
                }
                cur.reachable = true;
            }
            StmtKind::Repeat { body, cond } => {
                self.loops.push(LoopCtx {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                });
                let mut b = cur.clone();
                self.stmts(body, &mut b);
                let ctx = self.loops.pop().unwrap();
                // continue jumps to the condition
                for st in &ctx.continues {
                    if b.reachable {
                        and_state(&mut b.state, st);
                    } else {
                        b.state = st.clone();
                        b.reachable = true;
                    }
                }
                if b.reachable {
                    self.expr(cond, &mut b);
                }
                let mut after = b;
                for st in &ctx.breaks {
                    if after.reachable {
                        and_state(&mut after.state, st);
                    } else {
                        after.state = st.clone();
                        after.reachable = true;
                    }
                }
                *cur = after;
            }
            StmtKind::For {
                var,
                var_span,
                from,
                to,
                body,
                ..
            } => {
                self.expr(from, cur);
                self.expr(to, cur);
                self.for_loop(var, *var_span, body, cur);
            }
            StmtKind::ForIn {
                var,
                var_span,
                coll,
                body,
                ..
            } => {
                self.expr(coll, cur);
                self.for_loop(var, *var_span, body, cur);
            }
            StmtKind::Break => {
                if let Some(i) = self.loops.len().checked_sub(1) {
                    self.jump(Jump::Break(i), cur.state.clone());
                }
                cur.reachable = false;
            }
            StmtKind::Continue => {
                if let Some(i) = self.loops.len().checked_sub(1) {
                    self.jump(Jump::Continue(i), cur.state.clone());
                }
                cur.reachable = false;
            }
            StmtKind::Exit(value) => {
                if let Some(v) = value {
                    self.expr(v, cur);
                    if let Some(rid) = self.r.result_var() {
                        cur.state[rid.0 as usize] = true;
                        self.refs[rid.0 as usize] += 1;
                    }
                }
                self.jump(Jump::Exit(s.span), cur.state.clone());
                cur.reachable = false;
            }
            StmtKind::Halt(code) | StmtKind::Raise(code) => {
                if let Some(c) = code {
                    self.expr(c, cur);
                }
                cur.reachable = false;
            }
            StmtKind::RaiseOuter(x) => {
                self.expr(x, cur);
                cur.reachable = false;
            }
            StmtKind::Try { body, handler } => self.try_stmt(body, handler, cur),
            StmtKind::Case {
                selector,
                arms,
                else_body,
            } => {
                // Without `else`, the path past every arm counts even if the labels cover all the
                // selector's values (`docs/spec/semantics.md` 4.10)
                self.expr(selector, cur);
                let mut merged: Option<Cursor> = None;
                for arm in arms {
                    let mut b = cur.clone();
                    if let Some(body) = &arm.body {
                        self.stmt(body, &mut b);
                    }
                    merge_into(&mut merged, b);
                }
                match else_body {
                    Some(list) => {
                        let mut b = cur.clone();
                        self.stmts(list, &mut b);
                        merge_into(&mut merged, b);
                    }
                    None => merge_into(&mut merged, cur.clone()),
                }
                *cur = merged.expect("the path past the arms is merged");
            }
            StmtKind::IncDec { target, delta, .. } => {
                if let Some(d) = delta {
                    self.expr(d, cur);
                }
                // Same as `X := X + N`: read, then assign (4.4)
                self.place_read(target, cur);
                if let Place::Var(id, _) = target {
                    self.mark_assigned(*id, cur);
                }
            }
            StmtKind::Insert {
                source,
                target,
                index,
                ..
            } => {
                // Same as `S := …S…`: read, then assign
                self.expr(source, cur);
                self.place_read(target, cur);
                self.expr(index, cur);
                if let Place::Var(id, _) = target {
                    self.mark_assigned(*id, cur);
                }
            }
            StmtKind::Delete {
                target,
                index,
                count,
            } => {
                self.place_read(target, cur);
                self.expr(index, cur);
                self.expr(count, cur);
                if let Place::Var(id, _) = target {
                    self.mark_assigned(*id, cur);
                }
            }
            StmtKind::SetElem { target, elem, .. } => {
                // Same as `S := S + [X]`
                self.expr(elem, cur);
                self.place_read(target, cur);
                if let Place::Var(id, _) = target {
                    self.mark_assigned(*id, cur);
                }
            }
            StmtKind::Read { targets, .. } => {
                for p in targets {
                    self.place_write(p, cur);
                }
            }
            StmtKind::SetLength { target, lens } => {
                for l in lens {
                    self.expr(l, cur);
                }
                // An assignment (`docs/spec/types.md` chapter 6)
                self.place_write(target, cur);
            }
            StmtKind::Write { args, .. } => {
                for a in args {
                    self.expr(&a.value, cur);
                    if let Some(w) = &a.width {
                        self.expr(w, cur);
                    }
                    if let Some(d) = &a.precision {
                        self.expr(d, cur);
                    }
                }
            }
            StmtKind::Assert { cond, message, .. } => {
                self.expr(cond, cur);
                if let Some(m) = message {
                    self.expr(m, cur);
                }
            }
            StmtKind::Nop | StmtKind::Flush => {}
        }
    }

    /// A `Break` / `Continue` / `Exit` with the state at the jump. Through a `finally` block, the
    /// jump waits until the block has been analyzed.
    fn jump(&mut self, j: Jump, state: State) {
        if let Some(f) = self.finallies.last_mut() {
            let outside = match j {
                Jump::Exit(_) => true,
                Jump::Break(i) | Jump::Continue(i) => i < f.loops_at_entry,
            };
            if outside {
                f.pending.push((j, state));
                return;
            }
        }
        match j {
            Jump::Exit(at) => self.check_return(&state, at),
            Jump::Break(i) => self.loops[i].breaks.push(state),
            Jump::Continue(i) => self.loops[i].continues.push(state),
        }
    }

    /// `try..except` / `try..finally` (`docs/spec/exceptions.md` 5.7): handlers and the `finally`
    /// block start from the state at the `try`.
    fn try_stmt(&mut self, body: &[Stmt], handler: &TryHandler, cur: &mut Cursor) {
        let entry = cur.clone();
        match handler {
            TryHandler::Except {
                handlers,
                else_body,
            } => {
                let mut b = entry.clone();
                self.stmts(body, &mut b);
                let mut merged = Some(b);
                for h in handlers {
                    let mut c = entry.clone();
                    if let Some(s) = &h.body {
                        self.stmt(s, &mut c);
                    }
                    merge_into(&mut merged, c);
                }
                // Without `else`, an exception no handler matches is raised again
                if let Some(list) = else_body {
                    let mut c = entry.clone();
                    self.stmts(list, &mut c);
                    merge_into(&mut merged, c);
                }
                *cur = merged.expect("the body's cursor is merged first");
            }
            TryHandler::Finally(fin) => {
                self.finallies.push(FinallyCtx {
                    loops_at_entry: self.loops.len(),
                    pending: Vec::new(),
                });
                let mut b = entry.clone();
                self.stmts(body, &mut b);
                let ctx = self.finallies.pop().expect("pushed above");
                let mut f = entry;
                self.stmts(fin, &mut f);
                if f.reachable {
                    for (j, mut state) in ctx.pending {
                        or_state(&mut state, &f.state);
                        self.jump(j, state);
                    }
                }
                if b.reachable {
                    or_state(&mut b.state, &f.state);
                }
                b.reachable = b.reachable && f.reachable;
                *cur = b;
            }
        }
    }

    fn expr(&mut self, e: &Expr, cur: &mut Cursor) {
        match &e.kind {
            ExprKind::Const | ExprKind::Global(_) => {}
            // An error-typed expression has no children, so the reads recorded by semantic analysis within its range
            // are treated as read here (4.6; also reports E2000 / E2004)
            ExprKind::Error => self.reads_within(e.span, cur),
            ExprKind::Var(id) => {
                self.mark_seen(*id, e.span);
                self.read(*id, e.span, cur);
            }
            ExprKind::Binary { first, steps } => {
                self.expr(first, cur);
                for s in steps {
                    self.expr(&s.rhs, cur);
                }
            }
            ExprKind::Unary { operand, .. } => self.expr(operand, cur),
            ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
                self.expr(base, cur);
                self.expr(index, cur);
            }
            ExprKind::Field { base, .. } => self.expr(base, cur),
            ExprKind::Slice { base, low, high } => {
                self.expr(base, cur);
                for b in low.iter().chain(high.iter()) {
                    self.expr(b, cur);
                }
            }
            ExprKind::Call { routine, args } => {
                for a in args {
                    match a {
                        CallArg::Value(v) => self.expr(v, cur),
                        CallArg::Ref { place, is_out } => {
                            if *is_out {
                                self.place_write(place, cur);
                            } else {
                                self.place_read(place, cur);
                            }
                        }
                    }
                }
                self.call_effect(*routine, e.span, cur);
            }
            ExprKind::Builtin { args, .. } | ExprKind::ArrayCtor(args) => {
                for a in args {
                    self.expr(a, cur);
                }
            }
            ExprKind::Convert { operand, .. } | ExprKind::NewExc { msg: operand, .. } => {
                self.expr(operand, cur)
            }
            ExprKind::ExcMember { base, .. } => self.expr(base, cur),
            ExprKind::TryStrToInt { s, target } => {
                self.expr(s, cur);
                self.place_write(target, cur);
            }
            ExprKind::SetCtor(elems) => {
                for el in elems {
                    self.expr(&el.low, cur);
                    if let Some(h) = &el.high {
                        self.expr(h, cur);
                    }
                }
            }
        }
    }

    /// Reads the reads recorded by semantic analysis that lie within `range`.
    fn reads_within(&mut self, range: TextRange, cur: &mut Cursor) {
        for i in 0..self.sema_reads.len() {
            let (id, at) = self.sema_reads[i];
            if !self.seen[i] && range.contains_range(at) {
                self.seen[i] = true;
                self.read(id, at, cur);
            }
        }
    }

    fn mark_seen(&mut self, id: VarId, at: TextRange) {
        for i in 0..self.sema_reads.len() {
            if !self.seen[i] && self.sema_reads[i] == (id, at) {
                self.seen[i] = true;
                return;
            }
        }
    }

    /// Counts reads inside statements that did not make it into the tree (not seen by this analysis) as reads,
    /// except those in unreachable code (for W0001; definite assignment cannot be checked there, so no E2000).
    fn count_unseen_reads(&mut self) {
        for i in 0..self.sema_reads.len() {
            if self.seen[i] {
                continue;
            }
            let (id, at) = self.sema_reads[i];
            if self.unreachable.iter().any(|u| u.contains_range(at)) {
                continue;
            }
            self.reads[id.0 as usize] = true;
            self.refs[id.0 as usize] += 1;
            self.unseen_read[id.0 as usize] = true;
        }
    }

    fn place_read(&mut self, p: &Place, cur: &mut Cursor) {
        match p {
            Place::Var(id, span) => {
                self.mark_seen(*id, *span);
                self.read(*id, *span, cur);
            }
            Place::Global(_) => {}
            Place::StrIndex { base, index, .. } | Place::Elem { base, index, .. } => {
                self.expr(index, cur);
                self.place_read(base, cur);
            }
            Place::Field { base, .. } => self.place_read(base, cur),
            Place::ExcMember { base, .. } => self.expr(base, cur),
        }
    }

    fn place_write(&mut self, p: &Place, cur: &mut Cursor) {
        match p {
            Place::Var(id, _) => {
                let i = id.0 as usize;
                self.mark_assigned(*id, cur);
                self.tainted[i] = None;
                self.refs[i] += 1;
            }
            Place::Global(_) => {}
            // Writing a part reads the whole: the `S` in `S[I] := C`, `R` in `R.F := 1`
            // (`docs/spec/types.md` chapter 6)
            Place::StrIndex { base, index, .. } | Place::Elem { base, index, .. } => {
                self.expr(index, cur);
                self.place_read(base, cur);
            }
            Place::Field { base, .. } => self.place_read(base, cur),
            Place::ExcMember { base, .. } => self.expr(base, cur),
        }
    }

    fn read(&mut self, id: VarId, at: TextRange, cur: &mut Cursor) {
        self.read_as(id, at, "read here", cur);
    }

    /// A read at `at`, labeled `label` in E2000 / E2002 / E2004 (a call of a nested routine reads
    /// the variables it reads; `docs/spec/semantics.md` 4.9).
    fn read_as(&mut self, id: VarId, at: TextRange, label: &'static str, cur: &mut Cursor) {
        let i = id.0 as usize;
        self.refs[i] += 1;
        self.reads[i] = true;
        let slot = self.slot(id);
        if slot != i {
            // A variable of the target read by a nested routine (working out its effect)
            if !cur.state[slot]
                && let Some(s) = &mut self.summary
            {
                s.reads[slot - s.own] = true;
            }
            return;
        }
        if cur.state[i] {
            return;
        }
        let v = &self.r.vars[i];
        let name = v.name.clone();
        if let Some(for_span) = self.tainted[i] {
            let d = Diagnostic::error(Code::E(2004), format!("loop variable `{name}` cannot be read after the loop"), Stage::Flow)
                .with_primary(span(self.file, at), label)
                .with_secondary(span(self.file, for_span), "assigned by this loop")
                .with_secondary(span(self.file, v.span), "declared here")
                .with_note("the value of a loop variable after the loop is undefined in Delphi, so Puy does not allow reading it")
                .with_note("copy the value to another variable inside the loop");
            self.diags.push(d);
            return;
        }
        match v.kind {
            VarKind::Param(ast::ParamMode::Out) => {
                let d = Diagnostic::error(
                    Code::E(2002),
                    format!("out parameter `{name}` may be read before it is assigned"),
                    Stage::Flow,
                )
                .with_primary(span(self.file, at), label)
                .with_secondary(span(self.file, v.span), "declared here")
                .with_note("Puy requires an `out` parameter to be assigned before it is read");
                let d = match self.begin_insert(&name, v.ty) {
                    Some(f) => d.with_fix(f),
                    None => d,
                };
                self.diags.push(d);
            }
            VarKind::Result => {
                let d = Diagnostic::error(
                    Code::E(2000),
                    "variable `Result` may be read before it is assigned",
                    Stage::Flow,
                )
                .with_primary(span(self.file, at), label)
                .with_note("Puy requires `Result` to be assigned before it is read");
                let d = match self.begin_insert("Result", v.ty) {
                    Some(f) => d.with_fix(f),
                    None => d,
                };
                self.diags.push(d);
            }
            _ => {
                let mut d = Diagnostic::error(
                    Code::E(2000),
                    format!("variable `{name}` may be read before it is assigned"),
                    Stage::Flow,
                )
                .with_primary(span(self.file, at), label)
                .with_secondary(span(self.file, v.span), "declared here")
                .with_note("Puy requires every local variable to be assigned before it is read");
                match self.initial_value_fix(v) {
                    Ok(f) => d = d.with_fix(f),
                    Err(NoFix::SeveralNames) => {
                        d = d.with_note(format!(
                            "declare `{name}` on its own to give it an initial value"
                        ))
                    }
                    Err(NoFix::Anonymous) => {
                        d = d
                            .with_note(format!(
                                "declare a named type such as `TName` for `{name}` and initialize it with `Default(TName)`"
                            ))
                            .with_note(
                                "or give it an initial value that lists every element, as in `(0, 0, 0)`",
                            )
                    }
                    Err(NoFix::Other) => {}
                }
                self.diags.push(d);
            }
        }
    }

    /// An edit inserting `name := zero;` right after the body's `begin`. If a newline follows `begin`,
    /// adds a line with the same indentation as the next line (continuing on the `begin` line reads poorly).
    fn begin_insert(&self, name: &str, ty: Ty) -> Option<Fix> {
        let at = self.r.body_begin?;
        let zero = zero_text(ty, self.types)?;
        let rest = &self.bytes[u32::from(at) as usize..];
        let text = match rest.first() {
            Some(b'\r' | b'\n') => {
                // Newlines included in edits are always `LF` (`diagnostics.md` 2.1)
                let newline = "\n";
                let after = if rest.starts_with(b"\r\n") {
                    &rest[2..]
                } else {
                    &rest[1..]
                };
                let indent: String = after
                    .iter()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .map(|&b| b as char)
                    .collect();
                format!("{newline}{indent}{name} := {zero};")
            }
            _ => format!(" {name} := {zero};"),
        };
        Some(Fix::safe(
            format!("add `{name} := {zero};` at the start of the body"),
            vec![TextEdit::insert(self.file, at, text)],
        ))
    }

    /// An edit adding an initial value to a declaration (E2000).
    fn initial_value_fix(&self, v: &Var) -> Result<Fix, NoFix> {
        let decl = v.decl.as_ref().ok_or(NoFix::Other)?;
        let zero = zero_text(v.ty, self.types).ok_or(NoFix::Anonymous)?;
        match decl.kind() {
            SyntaxKind::VAR_DECL => {
                let d = ast::VarDecl::cast(decl.clone()).ok_or(NoFix::Other)?;
                if d.names().len() != 1 {
                    return Err(NoFix::SeveralNames);
                }
                let t = d.type_ref().ok_or(NoFix::Other)?;
                let at = t.syntax().text_range().end();
                let text = format!(" = {zero}");
                let shown = format!("var {}: {} = {zero};", v.name, t.text());
                Ok(Fix::safe(
                    format!("add an initial value: `{shown}`"),
                    vec![TextEdit::insert(self.file, at, text)],
                ))
            }
            SyntaxKind::INLINE_VAR_DECL => {
                let d = ast::InlineVarDecl::cast(decl.clone()).ok_or(NoFix::Other)?;
                let t = d.type_ref().ok_or(NoFix::Other)?;
                let at = t.syntax().text_range().end();
                let text = format!(" := {zero}");
                let shown = format!("var {}: {} := {zero};", v.name, t.text());
                Ok(Fix::safe(
                    format!("add an initial value: `{shown}`"),
                    vec![TextEdit::insert(self.file, at, text)],
                ))
            }
            _ => Err(NoFix::Other),
        }
    }

    /// Whether `Result` and `out` parameters are assigned at a return point (the return variants of E2001, E2002).
    fn check_return(&mut self, state: &State, at: TextRange) {
        if let Some(s) = &mut self.summary {
            let now = &state[s.own..];
            match &mut s.writes {
                Some(w) => and_state(w, &now.to_vec()),
                None => s.writes = Some(now.to_vec()),
            }
            return;
        }
        if let Some(rid) = self.r.result_var()
            && !state[rid.0 as usize]
            && !self.result_return_reported
        {
            self.result_return_reported = true;
            let name = self.r.name.clone();
            let ty = self.r.vars[rid.0 as usize].ty;
            let mut d = Diagnostic::error(
                Code::E(2001),
                format!("function `{name}` may return without assigning `Result`"),
                Stage::Flow,
            )
            .with_primary(span(self.file, self.r.name_span), "")
            .with_secondary(span(self.file, at), "returns here")
            .with_note("Puy requires `Result` to be assigned on every path that returns");
            if let Some(mut f) = self.begin_insert("Result", ty) {
                f.applicability = crate::diagnostics::Applicability::Suggestion;
                d = d.with_fix(f);
            }
            self.diags.push(d);
        }
        for (i, v) in self.r.vars.iter().enumerate() {
            // An `out` parameter whose type was rejected has its own diagnostic
            if v.kind == VarKind::Param(ast::ParamMode::Out)
                && !v.ty.is_error()
                && !state[i]
                && !self.out_return_reported[i]
            {
                self.out_return_reported[i] = true;
                let name = v.name.clone();
                let mut d = Diagnostic::error(
                    Code::E(2002),
                    format!("out parameter `{name}` may not be assigned on return"),
                    Stage::Flow,
                )
                .with_primary(span(self.file, self.r.name_span), "")
                .with_secondary(span(self.file, at), "returns here")
                .with_secondary(span(self.file, v.span), "declared here")
                .with_note(
                    "Puy requires an `out` parameter to be assigned on every path that returns",
                );
                if let Some(mut f) = self.begin_insert(&name, v.ty) {
                    f.applicability = crate::diagnostics::Applicability::Suggestion;
                    d = d.with_fix(f);
                }
                self.diags.push(d);
            }
        }
    }

    /// W0001.
    fn unused_variables(&mut self) {
        for (i, v) in self.r.vars.iter().enumerate() {
            if v.kind != VarKind::Local
                || v.is_loop_var
                || v.is_for_var
                || self.reads[i]
                || v.read_by_nested
            {
                continue;
            }
            let name = v.name.clone();
            let mut d =
                Diagnostic::warning(Code::W(1), format!("unused variable `{name}`"), Stage::Flow)
                    .with_primary(span(self.file, v.span), "declared here");
            // A variable a nested routine assigns is not removed by the fix
            if self.refs[i] == 0 && !v.captured {
                if let Some(edit) = self.deletion_edit(v) {
                    d = d.with_fix(Fix::suggestion(format!("remove `{name}`"), vec![edit]));
                }
            } else {
                d = d.with_note(format!(
                    "remove `{name}` and its assignments if the value is not needed"
                ));
            }
            self.diags.push(d);
        }
    }

    /// W0017 (disabled by default): a value or `const` parameter that is never read, or a `var`
    /// parameter that is neither read nor written.
    fn unused_parameters(&mut self) {
        for (i, v) in self.r.vars.iter().enumerate() {
            let unused = match v.kind {
                VarKind::Param(ast::ParamMode::Value | ast::ParamMode::Const) => {
                    !self.reads[i] && !v.read_by_nested
                }
                VarKind::Param(ast::ParamMode::Var) => self.refs[i] == 0 && !v.captured,
                _ => false,
            };
            if unused {
                let d = Diagnostic::warning(
                    Code::W(17),
                    format!("unused parameter `{}`", v.name),
                    Stage::Flow,
                )
                .with_primary(span(self.file, v.span), "declared here");
                self.diags.push(d);
            }
        }
    }

    /// An edit removing a declaration (W0001).
    fn deletion_edit(&self, v: &Var) -> Option<TextEdit> {
        let decl = v.decl.as_ref()?;
        let range = match decl.kind() {
            SyntaxKind::VAR_DECL => {
                let d = ast::VarDecl::cast(decl.clone())?;
                let names = d.names();
                if names.len() == 1 {
                    let section = decl.parent()?;
                    let decls = section
                        .children()
                        .filter(|c| c.kind() == SyntaxKind::VAR_DECL)
                        .count();
                    if decls == 1 && section.kind() == SyntaxKind::VAR_SECTION {
                        section.text_range()
                    } else {
                        decl.text_range()
                    }
                } else {
                    // One identifier in a list: remove only the identifier and the adjacent `,` (and whitespace between them).
                    // The rule that removes preceding whitespace (`diagnostics.md` 2.1) does not apply (`var A, B` would become `varB`)
                    let idx = names.iter().position(|t| t.text_range() == v.span)?;
                    let range = if idx + 1 < names.len() {
                        TextRange::new(
                            names[idx].text_range().start(),
                            names[idx + 1].text_range().start(),
                        )
                    } else {
                        TextRange::new(
                            names[idx - 1].text_range().end(),
                            names[idx].text_range().end(),
                        )
                    };
                    return Some(TextEdit::delete(self.file, range));
                }
            }
            SyntaxKind::INLINE_VAR_DECL => {
                let mut end = decl.text_range().end();
                let mut next = decl.next_sibling_or_token();
                while let Some(n) = next {
                    match n.kind() {
                        k if k.is_trivia() => {}
                        SyntaxKind::SEMICOLON => {
                            end = n.text_range().end();
                            break;
                        }
                        _ => break,
                    }
                    next = n.next_sibling_or_token();
                }
                TextRange::new(decl.text_range().start(), end)
            }
            _ => return None,
        };
        let range = crate::diagnostics::deletion_range(self.bytes, range);
        Some(TextEdit::delete(self.file, range))
    }
}

/// Why E2000 has no fix that adds an initial value.
enum NoFix {
    /// A declaration of two or more identifiers.
    SeveralNames,
    /// An anonymous record or static array type (`docs/spec/types.md` chapter 6).
    Anonymous,
    Other,
}

fn or_state(a: &mut State, b: &State) {
    for (x, y) in a.iter_mut().zip(b) {
        *x = *x || *y;
    }
}

fn and_state(a: &mut State, b: &State) {
    for (x, y) in a.iter_mut().zip(b) {
        *x = *x && *y;
    }
}

/// Merging branches. Unreachable branches impose no constraint.
fn merge_into(acc: &mut Option<Cursor>, c: Cursor) {
    match acc {
        None => *acc = Some(c),
        Some(a) => {
            if !c.reachable {
                return;
            }
            if !a.reachable {
                *a = c;
                return;
            }
            and_state(&mut a.state, &c.state);
        }
    }
}

#[allow(dead_code)]
fn _unused(_: TextSize) {}
