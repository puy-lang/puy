//! Control-flow analysis (`docs/spec/semantics.md` 4.4, 4.6): definite assignment (E2000–E2004),
//! unused variables (W0001), unreachable code (W0003).

use super::hir::*;
use super::types::Ty;
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

struct Flow<'a> {
    r: &'a Routine,
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
    /// Ranges of unreachable code (W0003).
    unreachable: Vec<TextRange>,
    loops: Vec<LoopCtx>,
    result_return_reported: bool,
    out_return_reported: Vec<bool>,
}

/// `sema_reads` are the variable reads recorded by semantic analysis (with positions). Reads inside expressions that did not
/// make it into the tree (error-typed expressions, discarded statements) also count as reads if they are in reachable code (4.6).
pub fn analyze(
    r: &Routine,
    file: FileId,
    bytes: &[u8],
    sema_reads: &[(VarId, TextRange)],
) -> Vec<Diagnostic> {
    let n = r.vars.len();
    let mut f = Flow {
        r,
        file,
        bytes,
        diags: Vec::new(),
        reads: vec![false; n],
        refs: vec![0; n],
        tainted: vec![None; n],
        sema_reads,
        seen: vec![false; sema_reads.len()],
        unreachable: Vec::new(),
        loops: Vec::new(),
        result_return_reported: false,
        out_return_reported: vec![false; n],
    };
    let mut cur = Cursor {
        state: (0..n)
            .map(|i| match r.vars[i].kind {
                VarKind::Param(ast::ParamMode::Out) => false,
                VarKind::Param(_) => true,
                VarKind::Result => false,
                VarKind::Local => false,
                VarKind::Hidden => true,
            })
            .collect(),
        reachable: true,
    };
    f.stmts(&r.body, &mut cur);
    if cur.reachable {
        f.check_return(&cur.state, r.end_span);
    }
    f.count_unseen_reads();
    f.unused_variables();
    f.diags
}

fn span(file: FileId, range: TextRange) -> Span {
    Span::new(file, range)
}

fn zero_text(ty: Ty) -> &'static str {
    match ty {
        Ty::Boolean => "False",
        Ty::Char => "#0",
        Ty::String => "''",
        _ => "0",
    }
}

impl<'a> Flow<'a> {
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
                    self.tainted[i] = Some(*var_span);
                }
                cur.reachable = true;
            }
            StmtKind::Break => {
                if let Some(l) = self.loops.last_mut() {
                    l.breaks.push(cur.state.clone());
                }
                cur.reachable = false;
            }
            StmtKind::Continue => {
                if let Some(l) = self.loops.last_mut() {
                    l.continues.push(cur.state.clone());
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
                self.check_return(&cur.state, s.span);
                cur.reachable = false;
            }
            StmtKind::Halt(code) => {
                if let Some(c) = code {
                    self.expr(c, cur);
                }
                cur.reachable = false;
            }
            StmtKind::IncDec { target, delta, .. } => {
                if let Some(d) = delta {
                    self.expr(d, cur);
                }
                // Same as `X := X + N`: read, then assign (4.4)
                self.place_read(target, cur);
                if let Place::Var(id, _) = target {
                    cur.state[id.0 as usize] = true;
                }
            }
            StmtKind::ReadLn(place) => {
                if let Some(p) = place {
                    self.place_write(p, cur);
                }
            }
            StmtKind::Write { args, .. } => {
                for a in args {
                    self.expr(&a.value, cur);
                    if let Some(w) = &a.width {
                        self.expr(w, cur);
                    }
                }
            }
            StmtKind::Assert { cond, message, .. } => {
                self.expr(cond, cur);
                if let Some(m) = message {
                    self.expr(m, cur);
                }
            }
            StmtKind::Nop => {}
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
            ExprKind::StrIndex { base, index } => {
                self.expr(base, cur);
                self.expr(index, cur);
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
            ExprKind::Builtin { args, .. } => {
                for a in args {
                    self.expr(a, cur);
                }
            }
            ExprKind::Convert { operand, .. } => self.expr(operand, cur),
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
        }
    }

    fn place_read(&mut self, p: &Place, cur: &mut Cursor) {
        match p {
            Place::Var(id, span) => {
                self.mark_seen(*id, *span);
                self.read(*id, *span, cur);
            }
            Place::Global(_) => {}
            Place::StrIndex { base, index, .. } => {
                self.expr(index, cur);
                self.place_read(base, cur);
            }
        }
    }

    fn place_write(&mut self, p: &Place, cur: &mut Cursor) {
        match p {
            Place::Var(id, _) => {
                let i = id.0 as usize;
                cur.state[i] = true;
                self.tainted[i] = None;
                self.refs[i] += 1;
            }
            Place::Global(_) => {}
            Place::StrIndex { base, index, .. } => {
                // The `S` in `S[I] := C` is read
                self.expr(index, cur);
                self.place_read(base, cur);
            }
        }
    }

    fn read(&mut self, id: VarId, at: TextRange, cur: &mut Cursor) {
        let i = id.0 as usize;
        self.refs[i] += 1;
        self.reads[i] = true;
        if cur.state[i] {
            return;
        }
        let v = &self.r.vars[i];
        let name = v.name.clone();
        if let Some(for_span) = self.tainted[i] {
            let d = Diagnostic::error(Code::E(2004), format!("loop variable `{name}` cannot be read after the loop"), Stage::Flow)
                .with_primary(span(self.file, at), "read here")
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
                .with_primary(span(self.file, at), "read here")
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
                .with_primary(span(self.file, at), "read here")
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
                .with_primary(span(self.file, at), "read here")
                .with_secondary(span(self.file, v.span), "declared here")
                .with_note("Puy requires every local variable to be assigned before it is read");
                match self.initial_value_fix(v) {
                    Ok(f) => d = d.with_fix(f),
                    Err(true) => {
                        d = d.with_note(format!(
                            "declare `{name}` on its own to give it an initial value"
                        ))
                    }
                    Err(false) => {}
                }
                self.diags.push(d);
            }
        }
    }

    /// An edit inserting `name := zero;` right after the body's `begin`. If a newline follows `begin`,
    /// adds a line with the same indentation as the next line (continuing on the `begin` line reads poorly).
    fn begin_insert(&self, name: &str, ty: Ty) -> Option<Fix> {
        let at = self.r.body_begin?;
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
                format!("{newline}{indent}{name} := {};", zero_text(ty))
            }
            _ => format!(" {name} := {};", zero_text(ty)),
        };
        Some(Fix::safe(
            format!(
                "add `{name} := {};` at the start of the body",
                zero_text(ty)
            ),
            vec![TextEdit::insert(self.file, at, text)],
        ))
    }

    /// An edit adding an initial value to a declaration (E2000). `Err(true)` means a declaration with two or more identifiers.
    fn initial_value_fix(&self, v: &Var) -> Result<Fix, bool> {
        let decl = v.decl.as_ref().ok_or(false)?;
        let zero = zero_text(v.ty);
        match decl.kind() {
            SyntaxKind::VAR_DECL => {
                let d = ast::VarDecl::cast(decl.clone()).ok_or(false)?;
                if d.names().len() != 1 {
                    return Err(true);
                }
                let t = d.type_ref().ok_or(false)?;
                let at = t.syntax().text_range().end();
                let text = format!(" = {zero}");
                let shown = format!("var {}: {} = {zero};", v.name, t.text());
                Ok(Fix::safe(
                    format!("add an initial value: `{shown}`"),
                    vec![TextEdit::insert(self.file, at, text)],
                ))
            }
            SyntaxKind::INLINE_VAR_DECL => {
                let d = ast::InlineVarDecl::cast(decl.clone()).ok_or(false)?;
                let t = d.type_ref().ok_or(false)?;
                let at = t.syntax().text_range().end();
                let text = format!(" := {zero}");
                let shown = format!("var {}: {} := {zero};", v.name, t.text());
                Ok(Fix::safe(
                    format!("add an initial value: `{shown}`"),
                    vec![TextEdit::insert(self.file, at, text)],
                ))
            }
            _ => Err(false),
        }
    }

    /// Whether `Result` and `out` parameters are assigned at a return point (the return variants of E2001, E2002).
    fn check_return(&mut self, state: &State, at: TextRange) {
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
            if v.kind == VarKind::Param(ast::ParamMode::Out)
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
            if v.kind != VarKind::Local || v.is_loop_var || v.is_for_var || self.reads[i] {
                continue;
            }
            let name = v.name.clone();
            let mut d =
                Diagnostic::warning(Code::W(1), format!("unused variable `{name}`"), Stage::Flow)
                    .with_primary(span(self.file, v.span), "declared here");
            if self.refs[i] == 0 {
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
