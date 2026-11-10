//! Unused values (W0002, `docs/spec/semantics.md` 4.6): an assignment statement whose value no
//! path reads before the variable is assigned again or the routine ends.
//!
//! A forward analysis of the assignments that reach each point. Reading a variable marks every
//! assignment of it that reaches the read as used; an assignment statement to the whole variable
//! replaces them.
//!
//! Each statement is walked once. A `finally` block starts from placeholders the same way, which
//! each way out of it replaces by the values that came in along it. A loop body starts from a placeholder per variable standing for
//! the values at the head of the loop; after the body, the head is the entry plus what reaches the
//! end of the body, and the placeholders are replaced by it (a read of a placeholder reads every
//! value at the head). Variables do not affect one another, so this is the fixed point.

use super::hir::*;
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::{FileId, Span, TextRange};
use crate::syntax::ast::ParamMode;

/// For each variable, the values (indices into `Values::defs`) that may reach this point unread.
/// Empty for variables that are not candidates.
type State = Vec<Vec<usize>>;

#[derive(Clone, Copy)]
enum Def {
    /// An assignment statement: the target identifier.
    Assign(TextRange),
    /// The values at the head of a loop being walked (a placeholder).
    Head,
}

#[derive(Clone)]
struct Cursor {
    state: State,
    reachable: bool,
}

struct LoopCtx {
    breaks: Option<State>,
    continues: Option<State>,
}

#[derive(Clone, Copy)]
enum Jump {
    Exit,
    Break(usize),
    Continue(usize),
}

/// The body of a `try..finally`: jumps out of it run the `finally` block first.
struct FinallyCtx {
    loops_at_entry: usize,
    pending: Vec<(Jump, State)>,
}

struct Values<'a> {
    /// Candidates for W0002, by variable.
    candidate: Vec<bool>,
    sema_reads: &'a [(VarId, TextRange)],
    defs: Vec<(VarId, Def)>,
    used: Vec<bool>,
    loops: Vec<LoopCtx>,
    finallies: Vec<FinallyCtx>,
    /// For each enclosing `try` body, the assignments reaching any point of it (where an exception
    /// may leave it for the handlers, the `finally` block or the caller).
    trys: Vec<State>,
}

/// W0002 for one routine. `excluded` marks variables the analysis must not report: those that get
/// W0001 and those read where the tree does not show (`docs/spec/semantics.md` 4.6).
pub fn analyze(
    r: &Routine,
    file: FileId,
    sema_reads: &[(VarId, TextRange)],
    excluded: &[bool],
) -> Vec<Diagnostic> {
    let candidate: Vec<bool> = r
        .vars
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let kind_ok = match v.kind {
                VarKind::Local => !v.is_loop_var && !v.is_for_var,
                VarKind::Param(ParamMode::Value) => true,
                _ => false,
            };
            kind_ok && !excluded[i]
        })
        .collect();
    if !candidate.iter().any(|&c| c) {
        return Vec::new();
    }
    let mut a = Values {
        candidate,
        sema_reads,
        defs: Vec::new(),
        used: Vec::new(),
        loops: Vec::new(),
        finallies: Vec::new(),
        trys: Vec::new(),
    };
    let mut cur = Cursor {
        state: vec![Vec::new(); r.vars.len()],
        reachable: true,
    };
    a.stmts(&r.body, &mut cur);
    let mut diags = Vec::new();
    for (d, &(id, def)) in a.defs.iter().enumerate() {
        let Def::Assign(at) = def else { continue };
        if a.used[d] {
            continue;
        }
        let name = &r.vars[id.0 as usize].name;
        diags.push(
            Diagnostic::warning(
                Code::W(2),
                format!("value assigned to `{name}` is never read"),
                Stage::Flow,
            )
            .with_primary(Span::new(file, at), "")
            .with_note("remove the assignment if the value is not needed"),
        );
    }
    diags
}

fn union(a: &mut State, b: &State) {
    for (x, y) in a.iter_mut().zip(b) {
        for d in y {
            if !x.contains(d) {
                x.push(*d);
            }
        }
    }
}

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
            union(&mut a.state, &c.state);
        }
    }
}

fn add_to(acc: &mut Option<State>, s: &State) {
    match acc {
        None => *acc = Some(s.clone()),
        Some(a) => union(a, s),
    }
}

fn empty_like(s: &State) -> State {
    vec![Vec::new(); s.len()]
}

/// Replaces the placeholders of a loop (`heads`, by variable) by the values at its head.
fn substitute(state: &mut State, heads: &[Option<usize>], head: &State) {
    for (v, t) in heads.iter().enumerate() {
        if let Some(t) = t
            && let Some(pos) = state[v].iter().position(|d| d == t)
        {
            state[v].swap_remove(pos);
            for d in &head[v] {
                if !state[v].contains(d) {
                    state[v].push(*d);
                }
            }
        }
    }
}

impl Values<'_> {
    fn stmts(&mut self, list: &[Stmt], cur: &mut Cursor) {
        for s in list {
            if !cur.reachable {
                return;
            }
            self.stmt(s, cur);
        }
    }

    /// Records the state where an exception may leave the innermost `try` body.
    fn may_raise(&mut self, cur: &Cursor) {
        if cur.reachable
            && let Some(t) = self.trys.last_mut()
        {
            union(t, &cur.state);
        }
    }

    fn stmt(&mut self, s: &Stmt, cur: &mut Cursor) {
        self.may_raise(cur);
        match &s.kind {
            StmtKind::Assign {
                target,
                value,
                is_init,
                ..
            } => {
                self.expr(value, cur);
                match target {
                    Place::Var(id, at) if !*is_init && self.candidate[id.0 as usize] => {
                        let d = self.def(*id, Def::Assign(*at));
                        cur.state[id.0 as usize] = vec![d];
                        self.may_raise(cur);
                    }
                    Place::Var(..) => {}
                    _ => self.place_write(target, cur),
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
                *cur = merged.expect("at least the path past the branches is merged");
            }
            StmtKind::While { cond, body } => {
                let entry = cur.state.clone();
                let (mut c, heads) = self.placeholders(cur);
                self.expr(cond, &mut c);
                let mut after = c.state.clone();
                let ctx = self.in_loop(|a| a.stmt(body, &mut c));
                let mut back = empty_like(&entry);
                if c.reachable {
                    union(&mut back, &c.state);
                }
                if let Some(st) = &ctx.continues {
                    union(&mut back, st);
                }
                let head = self.loop_end(&heads, &entry, &back);
                substitute(&mut after, &heads, &head);
                if let Some(mut st) = ctx.breaks {
                    substitute(&mut st, &heads, &head);
                    union(&mut after, &st);
                }
                *cur = Cursor {
                    state: after,
                    reachable: true,
                };
            }
            StmtKind::Repeat { body, cond } => {
                let entry = cur.state.clone();
                let (mut c, heads) = self.placeholders(cur);
                let ctx = self.in_loop(|a| a.stmts(body, &mut c));
                // `Continue` jumps to the condition
                if let Some(st) = &ctx.continues {
                    if c.reachable {
                        union(&mut c.state, st);
                    } else {
                        c = Cursor {
                            state: st.clone(),
                            reachable: true,
                        };
                    }
                }
                if c.reachable {
                    self.expr(cond, &mut c);
                }
                let mut back = empty_like(&entry);
                if c.reachable {
                    union(&mut back, &c.state);
                }
                let head = self.loop_end(&heads, &entry, &back);
                substitute(&mut c.state, &heads, &head);
                if let Some(mut st) = ctx.breaks {
                    substitute(&mut st, &heads, &head);
                    if c.reachable {
                        union(&mut c.state, &st);
                    } else {
                        c = Cursor {
                            state: st,
                            reachable: true,
                        };
                    }
                }
                *cur = c;
            }
            StmtKind::For { from, to, body, .. } => {
                self.expr(from, cur);
                self.expr(to, cur);
                self.for_body(body, cur);
            }
            StmtKind::ForIn { coll, body, .. } => {
                self.expr(coll, cur);
                self.for_body(body, cur);
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
                }
                self.jump(Jump::Exit, cur.state.clone());
                cur.reachable = false;
            }
            StmtKind::Halt(code) => {
                if let Some(c) = code {
                    self.expr(c, cur);
                }
                cur.reachable = false;
            }
            StmtKind::Raise(x) => {
                if let Some(x) = x {
                    self.expr(x, cur);
                }
                self.may_raise(cur);
                cur.reachable = false;
            }
            StmtKind::RaiseOuter(x) => {
                self.expr(x, cur);
                self.may_raise(cur);
                cur.reachable = false;
            }
            StmtKind::Try { body, handler } => self.try_stmt(body, handler, cur),
            StmtKind::Case {
                selector,
                arms,
                else_body,
            } => {
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
                self.place_read(target, cur);
            }
            StmtKind::Insert {
                source,
                target,
                index,
                ..
            } => {
                self.expr(source, cur);
                self.place_read(target, cur);
                self.expr(index, cur);
            }
            StmtKind::Delete {
                target,
                index,
                count,
            } => {
                self.place_read(target, cur);
                self.expr(index, cur);
                self.expr(count, cur);
            }
            StmtKind::SetElem { target, elem, .. } => {
                self.expr(elem, cur);
                self.place_read(target, cur);
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

    fn def(&mut self, id: VarId, def: Def) -> usize {
        self.defs.push((id, def));
        self.used.push(false);
        self.defs.len() - 1
    }

    /// A cursor holding a placeholder for each candidate's values (the placeholders are returned
    /// by variable): the head of a loop body, or the start of a `finally` block.
    fn placeholders(&mut self, cur: &Cursor) -> (Cursor, Vec<Option<usize>>) {
        let mut state = empty_like(&cur.state);
        let mut heads = vec![None; state.len()];
        for v in 0..state.len() {
            if self.candidate[v] {
                let t = self.def(VarId(v as u32), Def::Head);
                state[v].push(t);
                heads[v] = Some(t);
            }
        }
        let c = Cursor {
            state,
            reachable: true,
        };
        (c, heads)
    }

    /// The values at the head of a loop, from the entry and what comes back from the end of the
    /// body. Replaces the placeholders by them (`resolve`).
    fn loop_end(&mut self, heads: &[Option<usize>], entry: &State, back: &State) -> State {
        let mut head = entry.clone();
        for (v, t) in heads.iter().enumerate() {
            for d in &back[v] {
                if Some(*d) != *t && !head[v].contains(d) {
                    head[v].push(*d);
                }
            }
        }
        self.resolve(heads, &head);
        head
    }

    /// Replaces the placeholders by the values they stand for (`head`) in the states kept for
    /// later, and passes a read of a placeholder on to those values.
    fn resolve(&mut self, heads: &[Option<usize>], head: &State) {
        for t in self.trys.iter_mut() {
            substitute(t, heads, head);
        }
        for f in self.finallies.iter_mut() {
            for (_, st) in f.pending.iter_mut() {
                substitute(st, heads, head);
            }
        }
        for l in self.loops.iter_mut() {
            for st in l.breaks.iter_mut().chain(l.continues.iter_mut()) {
                substitute(st, heads, head);
            }
        }
        for (v, t) in heads.iter().enumerate() {
            if let Some(t) = t
                && self.used[*t]
            {
                for &d in &head[v] {
                    self.used[d] = true;
                }
            }
        }
    }

    /// Runs `f` inside a new loop context and returns the context.
    fn in_loop(&mut self, f: impl FnOnce(&mut Self)) -> LoopCtx {
        self.loops.push(LoopCtx {
            breaks: None,
            continues: None,
        });
        f(self);
        self.loops.pop().expect("pushed above")
    }

    /// The body of a `for` / `for..in` (after the bounds or the collection), which may run any
    /// number of times.
    fn for_body(&mut self, body: &Stmt, cur: &mut Cursor) {
        let entry = cur.state.clone();
        let (mut c, heads) = self.placeholders(cur);
        let ctx = self.in_loop(|a| a.stmt(body, &mut c));
        let mut back = empty_like(&entry);
        if c.reachable {
            union(&mut back, &c.state);
        }
        if let Some(st) = &ctx.continues {
            union(&mut back, st);
        }
        let head = self.loop_end(&heads, &entry, &back);
        // The body may run no times
        let mut after = head.clone();
        if let Some(mut st) = ctx.breaks {
            substitute(&mut st, &heads, &head);
            union(&mut after, &st);
        }
        *cur = Cursor {
            state: after,
            reachable: true,
        };
    }

    fn jump(&mut self, j: Jump, state: State) {
        if let Some(f) = self.finallies.last_mut() {
            let outside = match j {
                Jump::Exit => true,
                Jump::Break(i) | Jump::Continue(i) => i < f.loops_at_entry,
            };
            if outside {
                f.pending.push((j, state));
                return;
            }
        }
        match j {
            // Nothing is read after the routine ends
            Jump::Exit => {}
            Jump::Break(i) => add_to(&mut self.loops[i].breaks, &state),
            Jump::Continue(i) => add_to(&mut self.loops[i].continues, &state),
        }
    }

    /// `try..except` / `try..finally`. An exception may leave the body at any statement, so the
    /// handlers and the `finally` block start from every state of the body (4.6).
    fn try_stmt(&mut self, body: &[Stmt], handler: &TryHandler, cur: &mut Cursor) {
        let entry = cur.clone();
        match handler {
            TryHandler::Except {
                handlers,
                else_body,
            } => {
                self.trys.push(entry.state.clone());
                let mut b = entry.clone();
                self.stmts(body, &mut b);
                let raised = self.trys.pop().expect("pushed above");
                let start = Cursor {
                    state: raised.clone(),
                    reachable: true,
                };
                let mut merged = Some(b);
                for h in handlers {
                    let mut c = start.clone();
                    if let Some(s) = &h.body {
                        self.stmt(s, &mut c);
                    }
                    merge_into(&mut merged, c);
                }
                match else_body {
                    Some(list) => {
                        let mut c = start.clone();
                        self.stmts(list, &mut c);
                        merge_into(&mut merged, c);
                    }
                    // An exception no handler matches is raised again
                    None => self.may_raise(&start),
                }
                *cur = merged.expect("the body's cursor is merged first");
            }
            TryHandler::Finally(fin) => {
                let first_in_body = self.defs.len();
                self.trys.push(entry.state.clone());
                self.finallies.push(FinallyCtx {
                    loops_at_entry: self.loops.len(),
                    pending: Vec::new(),
                });
                let mut b = entry.clone();
                self.stmts(body, &mut b);
                let ctx = self.finallies.pop().expect("pushed above");
                let raised = self.trys.pop().expect("pushed above");
                let mut all = raised.clone();
                if b.reachable {
                    union(&mut all, &b.state);
                }
                for (_, st) in &ctx.pending {
                    union(&mut all, st);
                }
                // One walk of the `finally` block from placeholders; what leaves it goes on along
                // each way into it (the normal path, the jumps, the exception) with that way's
                // values in place of the placeholders. A read inside it reads them all
                let (mut f, heads) = self.placeholders(&entry);
                self.stmts(fin, &mut f);
                self.resolve(&heads, &all);
                if f.reachable {
                    for (j, st) in ctx.pending {
                        let mut out = f.state.clone();
                        substitute(&mut out, &heads, &st);
                        self.jump(j, out);
                    }
                    let mut out = f.state.clone();
                    substitute(&mut out, &heads, &raised);
                    self.may_raise(&Cursor {
                        state: out,
                        reachable: true,
                    });
                }
                // The normal path also carries the assignments of the body that an exception may
                // have cut short (they count as read if read after the `try`; 4.6), but not the
                // values from before the `try`, which reach the `finally` block past them only by
                // an exception
                let mut normal = all;
                if b.reachable {
                    normal = b.state.clone();
                    for (v, ds) in raised.iter().enumerate() {
                        for &d in ds {
                            if d >= first_in_body && !normal[v].contains(&d) {
                                normal[v].push(d);
                            }
                        }
                    }
                }
                let mut out = f.state;
                substitute(&mut out, &heads, &normal);
                *cur = Cursor {
                    state: out,
                    reachable: b.reachable && f.reachable,
                };
            }
        }
    }

    fn expr(&mut self, e: &Expr, cur: &mut Cursor) {
        match &e.kind {
            ExprKind::Const | ExprKind::Global(_) => {}
            ExprKind::Error => {
                for i in 0..self.sema_reads.len() {
                    let (id, at) = self.sema_reads[i];
                    if e.span.contains_range(at) {
                        self.read(id, cur);
                    }
                }
            }
            ExprKind::Var(id) => self.read(*id, cur),
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
            ExprKind::Call { args, .. } => {
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

    fn place_read(&mut self, p: &Place, cur: &mut Cursor) {
        match p {
            Place::Var(id, _) => self.read(*id, cur),
            Place::Global(_) => {}
            Place::StrIndex { base, index, .. } | Place::Elem { base, index, .. } => {
                self.expr(index, cur);
                self.place_read(base, cur);
            }
            Place::Field { base, .. } => self.place_read(base, cur),
            Place::ExcMember { base, .. } => self.expr(base, cur),
        }
    }

    /// A write that is not an assignment statement (`ReadLn`, an `out` argument, `SetLength`):
    /// it neither reads nor replaces the value of a whole variable; writing a part reads the whole.
    fn place_write(&mut self, p: &Place, cur: &mut Cursor) {
        match p {
            Place::Var(..) | Place::Global(_) => {}
            _ => self.place_read(p, cur),
        }
    }

    fn read(&mut self, id: VarId, cur: &mut Cursor) {
        for &d in &cur.state[id.0 as usize] {
            self.used[d] = true;
        }
    }
}
