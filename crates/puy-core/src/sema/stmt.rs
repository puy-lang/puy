//! Typing of statements (`docs/spec/semantics.md` chapters 4–6).

use super::builtins::{self, BuiltinProc};
use super::expr::PlaceUse;
use super::hir::*;
use super::overload::Pick;
use super::types::{Const, Ty};
use super::{LSym, Resolved, Sema};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::TextRange;
use crate::syntax::ast::{self, AstNode, ParamMode};

impl<'a, 's> Sema<'a, 's> {
    /// Statements inside `begin..end`. Creates a block scope.
    pub(super) fn lower_block_stmts(&mut self, block: &ast::CompoundStmt) -> Vec<Stmt> {
        self.cur_mut().scopes.push(Vec::new());
        let mut out = Vec::new();
        for st in block.stmts() {
            if let Some(s) = self.lower_stmt(&st) {
                out.push(s);
            }
        }
        self.cur_mut().scopes.pop();
        out
    }

    pub(super) fn lower_stmt(&mut self, st: &ast::Stmt) -> Option<Stmt> {
        let span = ast::trimmed_range(st.syntax());
        let kind = match st {
            ast::Stmt::Compound(b) => StmtKind::Block(self.lower_block_stmts(b)),
            ast::Stmt::Assign(a) => self.lower_assign(a, span)?,
            ast::Stmt::Expr(e) => self.lower_expr_stmt(e, span)?,
            ast::Stmt::InlineVar(v) => return self.lower_inline_var(v, span),
            ast::Stmt::InlineConst(c) => {
                let name_tok = c.name()?;
                let name = self.text(&name_tok).to_string();
                let lower = name.to_ascii_lowercase();
                let range = name_tok.text_range();
                let (value, ty) = self.eval_const_decl(c.type_ref(), c.value(), false);
                if self.check_duplicate(&lower, range) {
                    return None;
                }
                let sym = LSym::Const {
                    ty: if value.is_some() { ty } else { Ty::Error },
                    value: value.unwrap_or(Const::Int(0)),
                    span: range,
                };
                self.cur_mut().scopes.last_mut().unwrap().push((lower, sym));
                return None;
            }
            ast::Stmt::If(i) => self.lower_if(i)?,
            ast::Stmt::While(w) => {
                let cond = self.lower_condition(w.condition());
                let body = self.lower_loop_body(w.body());
                StmtKind::While {
                    cond,
                    body: Box::new(body),
                }
            }
            ast::Stmt::Repeat(r) => {
                self.cur_mut().scopes.push(Vec::new());
                self.cur_mut().loop_depth += 1;
                let mut body = Vec::new();
                for s in r.stmts() {
                    if let Some(x) = self.lower_stmt(&s) {
                        body.push(x);
                    }
                }
                self.cur_mut().loop_depth -= 1;
                let cond = self.lower_condition(r.condition());
                self.cur_mut().scopes.pop();
                StmtKind::Repeat { body, cond }
            }
            ast::Stmt::For(f) => self.lower_for(f, span)?,
            ast::Stmt::Raise(r) => self.lower_raise(r, span),
            ast::Stmt::Try(t) => self.lower_try(t),
            ast::Stmt::Case(c) => self.lower_case(c),
            ast::Stmt::Error(_) => return None,
        };
        Some(Stmt { kind, span })
    }

    fn empty_stmt(span: TextRange) -> Stmt {
        Stmt {
            kind: StmtKind::Nop,
            span,
        }
    }

    /// The body of `then` / `do` (a no-op statement if absent).
    fn lower_body(&mut self, body: Option<ast::Stmt>, at: TextRange) -> Stmt {
        match body {
            Some(b) => {
                self.cur_mut().scopes.push(Vec::new());
                let s = self.lower_stmt(&b);
                self.cur_mut().scopes.pop();
                s.unwrap_or_else(|| Self::empty_stmt(at))
            }
            None => Self::empty_stmt(at),
        }
    }

    fn lower_loop_body(&mut self, body: Option<ast::Stmt>) -> Stmt {
        self.cur_mut().loop_depth += 1;
        let at = body
            .as_ref()
            .map(|b| ast::trimmed_range(b.syntax()))
            .unwrap_or(TextRange::empty(0.into()));
        let s = self.lower_body(body, at);
        self.cur_mut().loop_depth -= 1;
        s
    }

    fn lower_condition(&mut self, cond: Option<ast::Expr>) -> Expr {
        let Some(c) = cond else {
            return Expr {
                kind: ExprKind::Error,
                ty: Ty::Error,
                value: None,
                span: TextRange::empty(0.into()),
            };
        };
        let e = self.lower_expr(&c);
        self.coerce(e, Ty::Boolean, false, None)
    }

    fn lower_if(&mut self, i: &ast::IfStmt) -> Option<StmtKind> {
        let mut branches: Vec<(Expr, Stmt)> = Vec::new();
        let mut cur = i.clone();
        let else_body: Option<Box<Stmt>>;
        loop {
            let cond = self.lower_condition(cur.condition());
            let then_at = cur
                .then_token()
                .map(|t| t.text_range())
                .unwrap_or(cond.span);
            let body = self.lower_body(cur.then_branch(), then_at);
            branches.push((cond, body));
            match cur.else_branch() {
                Some(eb) => match eb.stmt() {
                    Some(ast::Stmt::If(next)) => {
                        cur = next;
                        continue;
                    }
                    other => {
                        let at = eb.syntax().text_range();
                        else_body = Some(Box::new(self.lower_body(other, at)));
                        break;
                    }
                },
                None => {
                    else_body = None;
                    break;
                }
            }
        }
        Some(StmtKind::If {
            branches,
            else_body,
        })
    }

    fn lower_assign(&mut self, a: &ast::AssignStmt, span: TextRange) -> Option<StmtKind> {
        let target = a.target()?;
        let value_ast = a.value();
        // Type the right-hand side first (the target variable is read after the right-hand side),
        // except an array constructor and `nil`, which take their type from the target
        let contextual = value_ast.as_ref().is_some_and(is_contextual);
        let (value, place) = if contextual {
            let place = self.lower_place(&target, PlaceUse::Assign);
            let expected = place.as_ref().map_or(Ty::Error, |(_, t)| *t);
            let value = value_ast.as_ref().map(|v| self.lower_expected(v, expected));
            (value, place)
        } else {
            let value = value_ast.as_ref().map(|v| self.lower_expr(v));
            (value, self.lower_place(&target, PlaceUse::Assign))
        };
        let value = value?;
        let (place, ty) = place?;
        let checks = self.checks_at(span.start());
        // Record the assignment even if the type is unknown (for definite-assignment analysis; no code is generated)
        let value = if ty.is_error() || value.ty.is_error() {
            value
        } else {
            self.coerce(value, ty, checks.range, None)
        };
        Some(StmtKind::Assign {
            target: place,
            value,
            check: checks,
            is_init: false,
        })
    }

    fn lower_inline_var(&mut self, v: &ast::InlineVarDecl, span: TextRange) -> Option<Stmt> {
        let name_tok = v.name()?;
        let name = self.text(&name_tok).to_string();
        let lower = name.to_ascii_lowercase();
        let range = name_tok.text_range();
        let declared = v.type_ref().map(|t| self.resolve_type(Some(t)));
        let checks = self.checks_at(span.start());
        let init = v.initializer().map(|e| match declared {
            Some(t) => self.lower_init(&e, t, checks.range),
            None => self.lower_expr(&e),
        });
        let ty = match (declared, &init) {
            (Some(t), _) => t,
            (None, Some(e)) => e.ty,
            (None, None) => Ty::Error,
        };
        if self.check_duplicate(&lower, range) {
            return None;
        }
        // Inline variables at the script's top level (not visible from routines; for E0700 notes)
        if self.is_script
            && self
                .cur
                .as_ref()
                .is_some_and(|c| c.is_main && c.scopes.len() == 1)
        {
            self.script_top_vars.push((lower.clone(), range));
        }
        let id = self.add_var(name, ty, VarKind::Local, range);
        self.cur_mut().vars[id.0 as usize].decl = Some(v.syntax().clone());
        self.cur_mut()
            .scopes
            .last_mut()
            .unwrap()
            .push((lower, LSym::Var(id)));
        let value = init?;
        Some(Stmt {
            kind: StmtKind::Assign {
                target: Place::Var(id, span),
                value,
                check: checks,
                is_init: true,
            },
            span,
        })
    }

    fn lower_for(&mut self, f: &ast::ForStmt, span: TextRange) -> Option<StmtKind> {
        if f.is_in() {
            return self.lower_for_in(f, span);
        }
        let fv = f.var()?;
        let checks = self.checks_at(span.start());
        let from_ast = f.from();
        let to_ast = f.to();
        let from = from_ast.as_ref().map(|e| self.lower_expr(e));
        self.cur_mut().scopes.push(Vec::new());
        let inferred = from.as_ref().map_or(Ty::Error, |e| e.ty);
        let (var, var_ty) = match self.lower_loop_var(f, &fv, inferred, true) {
            LoopVar::Resolved(var, ty) => (var, ty),
            LoopVar::Undeclared => {
                // Keep type-checking the body
                if let Some(t) = &to_ast {
                    self.lower_expr(t);
                }
                self.lower_loop_body(f.body());
                return None;
            }
            LoopVar::Failed => return None,
        };
        // A global loop variable is counted in a hidden variable (`docs/spec/semantics.md` 4.5)
        let counter = matches!(var, ForVar::Global(_)).then(|| {
            let range = fv
                .name()
                .map_or(fv.syntax().text_range(), |t| t.text_range());
            self.add_var(String::new(), Ty::Int64, VarKind::Hidden, range)
        });
        let to = to_ast.as_ref().map(|e| self.lower_expr(e));
        let (Some(from), Some(to)) = (from, to) else {
            self.cur_mut().scopes.pop();
            return None;
        };
        let ok = !var_ty.is_error() && !from.ty.is_error() && !to.ty.is_error();
        let (from, to) = if ok {
            (
                self.coerce(from, var_ty, checks.range, None),
                self.coerce(to, var_ty, checks.range, None),
            )
        } else {
            (from, to)
        };
        let body = self.lower_loop_var_body(var, f.body());
        if !ok || from.ty.is_error() || to.ty.is_error() {
            return None;
        }
        Some(StmtKind::For {
            var,
            var_span: fv.syntax().text_range(),
            from,
            to,
            downto: f.is_downto(),
            body: Box::new(body),
            checks,
            counter,
        })
    }

    /// `for X in C` (`docs/spec/types.md` chapter 5): the collection is evaluated once into a
    /// hidden variable, and a second hidden variable holds the position.
    fn lower_for_in(&mut self, f: &ast::ForStmt, span: TextRange) -> Option<StmtKind> {
        let fv = f.var()?;
        let checks = self.checks_at(span.start());
        let coll = f.from().map(|e| self.lower_expr(&e));
        let elem = coll.as_ref().map_or(Ty::Error, |c| self.iteration_elem(c));
        self.cur_mut().scopes.push(Vec::new());
        let (var, var_ty) = match self.lower_loop_var(f, &fv, elem, false) {
            LoopVar::Resolved(var, ty) => (var, ty),
            LoopVar::Undeclared => {
                self.lower_loop_body(f.body());
                return None;
            }
            LoopVar::Failed => return None,
        };
        let conv = match &coll {
            Some(c) if !elem.is_error() && !var_ty.is_error() => {
                // The element is assigned to the loop variable (`docs/spec/semantics.md` 2.4). An
                // element of a set read into another type is a value of the base type's base
                let ty = if c.ty.is_set() && var_ty != elem {
                    self.types.base(elem)
                } else {
                    elem
                };
                let probe = Expr {
                    kind: ExprKind::Error,
                    ty,
                    value: None,
                    span: c.span,
                };
                match self.coerce(probe, var_ty, checks.range, None) {
                    Expr {
                        kind: ExprKind::Convert { kind, .. },
                        ..
                    } => Ok(Some(kind)),
                    e if e.ty.is_error() => Err(()),
                    _ => Ok(None),
                }
            }
            _ => Err(()),
        };
        let range = fv
            .name()
            .map_or(fv.syntax().text_range(), |t| t.text_range());
        let hidden = coll.as_ref().filter(|_| conv.is_ok()).map(|c| {
            (
                self.add_var(String::new(), c.ty, VarKind::Hidden, range),
                self.add_var(String::new(), Ty::Integer, VarKind::Hidden, range),
            )
        });
        let body = self.lower_loop_var_body(var, f.body());
        let (Some(coll), Some((coll_var, idx_var)), Ok(conv)) = (coll, hidden, conv) else {
            return None;
        };
        Some(StmtKind::ForIn {
            var,
            var_span: fv.syntax().text_range(),
            coll,
            coll_var,
            idx_var,
            elem,
            conv,
            body: Box::new(body),
        })
    }

    /// The element type of a `for..in` collection: an array's element type, or `Char` for a
    /// string. Anything else is E0730 (`docs/spec/types.md` chapter 5).
    fn iteration_elem(&mut self, coll: &Expr) -> Ty {
        match coll.ty {
            Ty::Error => Ty::Error,
            Ty::String => Ty::Char,
            // A set's elements have its base type (`docs/spec/types.md` 8.3)
            t if t.is_set() => self.types.set_base(t).unwrap_or(Ty::Error),
            t => match self.types.elem_of(t) {
                Some(e) => e,
                None => {
                    let msg = format!("cannot iterate over a value of type `{}`", self.tn(t));
                    self.error(Code::E(730), msg, coll.span);
                    Ty::Error
                }
            },
        }
    }

    /// The body of a `for` loop: assigning to the loop variable there is E0707. Ends the scope
    /// of `for var X`, which `lower_loop_var` opened.
    fn lower_loop_var_body(&mut self, var: ForVar, body: Option<ast::Stmt>) -> Stmt {
        match var {
            ForVar::Local(id) => self.cur_mut().loop_vars.push(id),
            ForVar::Global(id) => self.cur_mut().loop_globals.push(id),
        }
        let body = self.lower_loop_body(body);
        match var {
            ForVar::Local(_) => {
                self.cur_mut().loop_vars.pop();
            }
            ForVar::Global(_) => {
                self.cur_mut().loop_globals.pop();
            }
        }
        // The scope of `for var I` ends here
        let ended = self.cur_mut().scopes.pop().unwrap_or_default();
        for (n, sym) in ended {
            if let LSym::Var(id) = sym {
                let span = self.var(id).span;
                self.cur_mut().ended_for_vars.push((n, span));
            }
        }
        body
    }

    /// E0722 for a loop variable that is not a local variable (or not of an ordinal type, for
    /// `for..to`; `docs/spec/semantics.md` 4.5, `docs/spec/types.md` chapter 5).
    fn loop_var_error(&self, ordinal: bool) -> Diagnostic {
        let msg = if ordinal {
            "loop variable must be a local variable of an ordinal type"
        } else {
            "loop variable must be a local variable"
        };
        Diagnostic::error(Code::E(722), msg, Stage::Sema)
    }

    /// Resolves the loop variable of a `for` statement (`docs/spec/semantics.md` 4.5) inside the
    /// scope the caller opened for `for var X`. `inferred` is the type of `for var X` without a
    /// written type; `ordinal` requires an ordinal type (`for..to`). On failure the scope is closed.
    fn lower_loop_var(
        &mut self,
        f: &ast::ForStmt,
        fv: &ast::ForVar,
        inferred: Ty,
        ordinal: bool,
    ) -> LoopVar {
        let res = self.resolve_loop_var(f, fv, inferred, ordinal);
        if !matches!(res, LoopVar::Resolved(..)) {
            self.cur_mut().scopes.pop();
        }
        res
    }

    fn resolve_loop_var(
        &mut self,
        f: &ast::ForStmt,
        fv: &ast::ForVar,
        inferred: Ty,
        ordinal: bool,
    ) -> LoopVar {
        let Some(name_tok) = fv.name() else {
            return LoopVar::Failed;
        };
        let name = self.text(&name_tok).to_string();
        let lower = name.to_ascii_lowercase();
        let range = name_tok.text_range();
        let wrong_type = |ty: Ty| ordinal && !ty.is_error() && !ty.is_ordinal();
        if fv.is_inline() {
            let ty = match fv.type_ref() {
                Some(t) => self.resolve_type(Some(t)),
                None => inferred,
            };
            if wrong_type(ty) {
                let d = self
                    .loop_var_error(ordinal)
                    .with_primary(self.span(range), "");
                self.push(d);
            }
            if self.check_duplicate(&lower, range) {
                return LoopVar::Failed;
            }
            let id = self.add_var(name, ty, VarKind::Local, range);
            self.cur_mut().vars[id.0 as usize].decl = Some(fv.syntax().clone());
            self.cur_mut().vars[id.0 as usize].is_for_var = true;
            self.cur_mut()
                .scopes
                .last_mut()
                .unwrap()
                .push((lower, LSym::Var(id)));
            return LoopVar::Resolved(ForVar::Local(id), ty);
        }
        let is_main = self.cur.as_ref().is_some_and(|c| c.is_main);
        match self.lookup_scopes(&lower) {
            Some((up, LSym::Var(id))) if up > 0 => {
                // A variable of an enclosing routine (`docs/spec/semantics.md` 4.9). It counts as
                // used, so no W0001 piles on
                let decl = self.ctx_at(up).vars[id.0 as usize].span;
                let alias = self.var_at(up, id);
                self.mark_used(alias, range);
                let d = self
                    .loop_var_error(ordinal)
                    .with_primary(self.span(range), "")
                    .with_secondary(self.span(decl), "declared here")
                    .with_note("a variable of an enclosing routine cannot be a loop variable; use a local variable of this routine");
                self.push(d);
                LoopVar::Failed
            }
            Some((_, LSym::Var(id))) => {
                let v = self.var(id).clone();
                // A value parameter can be a loop variable, like a local variable (semantics.md 4.5).
                // A `const` parameter cannot be modified, so E0707; a `var` / `out` parameter gives E0722
                if matches!(v.kind, VarKind::Param(ast::ParamMode::Const)) {
                    self.cannot_assign(range, "a `const` parameter", Some(v.span));
                    return LoopVar::Failed;
                }
                if matches!(
                    v.kind,
                    VarKind::Param(ast::ParamMode::Var | ast::ParamMode::Out)
                ) {
                    let d = self
                        .loop_var_error(ordinal)
                        .with_primary(self.span(range), "")
                        .with_secondary(self.span(v.span), "declared here")
                        .with_note("a `var` or `out` parameter refers to the caller's variable; copy it to a local variable first");
                    self.push(d);
                    return LoopVar::Failed;
                }
                if wrong_type(v.ty) {
                    let d = self
                        .loop_var_error(ordinal)
                        .with_primary(self.span(range), "");
                    self.push(d);
                    return LoopVar::Failed;
                }
                if self.cur.as_ref().is_some_and(|c| c.loop_vars.contains(&id)) {
                    self.cannot_assign_loop_var(range, Some(v.span));
                    return LoopVar::Failed;
                }
                // A nested routine writing it could make the loop run on (`docs/spec/semantics.md` 4.9)
                if let Some(at) = v.written_by_nested {
                    let text = self.src(at);
                    let d = Diagnostic::error(
                        Code::E(707),
                        format!("cannot assign to `{text}`"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(at), "the loop variable of the enclosing routine")
                    .with_secondary(self.span(range), "used as the loop variable here")
                    .with_note("a nested routine cannot write a variable its enclosing routine uses as a `for` loop variable");
                    self.push(d);
                    // No W0001 piles on
                    self.cur_mut().vars[id.0 as usize].is_loop_var = true;
                    return LoopVar::Failed;
                }
                self.cur_mut().vars[id.0 as usize].is_loop_var = true;
                LoopVar::Resolved(ForVar::Local(id), v.ty)
            }
            Some((_, LSym::Const { span: decl, .. })) => {
                self.cannot_assign_const(range, decl);
                LoopVar::Failed
            }
            Some((_, LSym::Routine(_) | LSym::Type { .. })) => {
                self.cannot_assign(range, "not a variable", None);
                LoopVar::Failed
            }
            None => {
                // A global variable, or undeclared
                let global = self
                    .global_scope
                    .iter()
                    .rev()
                    .find(|(n, _)| *n == lower)
                    .map(|(_, s)| s.clone());
                // `ExitCode` is a global of `System` (5.3)
                let global = match global {
                    None if matches!(
                        super::builtins::lookup_system(&lower),
                        Some(super::builtins::SystemName::ExitCode)
                    ) =>
                    {
                        Some(super::GSym::Global(self.exit_code_global()))
                    }
                    g => g,
                };
                match global {
                    Some(super::GSym::Global(gid)) => {
                        let g = self.globals[gid.0 as usize].clone();
                        if !is_main {
                            let mut d = self
                                .loop_var_error(ordinal)
                                .with_primary(self.span(range), "");
                            if let Some(decl) = g.span {
                                d = d.with_secondary(self.span(decl), "declared here");
                            }
                            self.push(d);
                            return LoopVar::Failed;
                        }
                        if wrong_type(g.ty) {
                            let d = self
                                .loop_var_error(ordinal)
                                .with_primary(self.span(range), "");
                            self.push(d);
                            return LoopVar::Failed;
                        }
                        if self
                            .cur
                            .as_ref()
                            .is_some_and(|c| c.loop_globals.contains(&gid))
                        {
                            self.cannot_assign_loop_var(range, g.span);
                            return LoopVar::Failed;
                        }
                        LoopVar::Resolved(ForVar::Global(gid), g.ty)
                    }
                    Some(super::GSym::Const { span: decl, .. }) => {
                        self.cannot_assign_const(range, decl);
                        LoopVar::Failed
                    }
                    Some(super::GSym::Routine(_)) | Some(super::GSym::Type { .. }) => {
                        self.cannot_assign(range, "not a variable", None);
                        LoopVar::Failed
                    }
                    None => {
                        // Undeclared: fix inserting `var ` (omitted if the body declares the same name)
                        let body_declares = f
                            .body()
                            .map(|b| body_declares_name(b.syntax(), &lower, self.input.bytes))
                            .unwrap_or(false);
                        let mut d = Diagnostic::error(
                            Code::E(700),
                            format!("undeclared identifier `{name}`"),
                            Stage::Sema,
                        )
                        .with_primary(self.span(range), "");
                        // A routine wrote the name of a variable at the script's top level (1.2)
                        if let Some(decl) = self.script_top_var_outside_main(&lower) {
                            d = d
                                .with_secondary(self.span(decl), "declared here")
                                .with_note("a top-level variable of a script is a local variable of the main block and is not visible inside routines")
                                .with_note("pass the value as a parameter instead");
                        }
                        if !body_declares {
                            d = d.with_fix(Fix::safe(
                                "insert `var`",
                                vec![TextEdit::insert(self.input.file, range.start(), "var ")],
                            ));
                        }
                        self.push(d);
                        LoopVar::Undeclared
                    }
                }
            }
        }
    }

    /// E0707 for a constant written as a loop variable.
    fn cannot_assign_const(&mut self, range: TextRange, decl: TextRange) {
        let text = self.src(range);
        let d = Diagnostic::error(
            Code::E(707),
            format!("cannot assign to `{text}`"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "a constant")
        .with_secondary(self.span(decl), "declared here");
        self.push(d);
    }

    fn cannot_assign_loop_var(&mut self, range: TextRange, decl: Option<TextRange>) {
        let text = self.src(range);
        let mut d = Diagnostic::error(
            Code::E(707),
            format!("cannot assign to `{text}`"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "the loop variable");
        if let Some(decl) = decl {
            d = d.with_secondary(self.span(decl), "declared here");
        }
        self.push(d);
    }

    // ---- Expression statements ----

    /// `raise X` / `raise;` (`docs/spec/exceptions.md` 5.1).
    fn lower_raise(&mut self, r: &ast::RaiseStmt, span: TextRange) -> StmtKind {
        let Some(e) = r.expr() else {
            if self.cur.as_ref().is_none_or(|c| c.except_depth == 0) {
                let at = r.raise_token().map_or(span, |t| t.text_range());
                self.error(
                    Code::E(732),
                    "`raise` without an exception is only allowed in an `except` block",
                    at,
                );
            }
            return StmtKind::Raise(None);
        };
        StmtKind::Raise(Some(self.exception_value(&e)))
    }

    /// The statements of a `try` body, an `except` / `finally` block or a handler `else`, in a scope
    /// of their own.
    fn lower_stmt_list(&mut self, list: impl Iterator<Item = ast::Stmt>) -> Vec<Stmt> {
        self.cur_mut().scopes.push(Vec::new());
        let mut out = Vec::new();
        for st in list {
            if let Some(s) = self.lower_stmt(&st) {
                out.push(s);
            }
        }
        self.cur_mut().scopes.pop();
        out
    }

    /// `case` (`docs/spec/semantics.md` 4.10).
    fn lower_case(&mut self, c: &ast::CaseStmt) -> StmtKind {
        let mut selector = match c.selector() {
            Some(e) => self.lower_expr_raw(&e),
            None => Expr {
                kind: ExprKind::Error,
                ty: Ty::Error,
                value: None,
                span: c
                    .case_token()
                    .map_or(TextRange::empty(0.into()), |t| t.text_range()),
            },
        };
        // The labels are checked against the selector's own type (a subrange keeps its bounds);
        // the selector's value has the base type, as any value of a subrange
        // A `UTF8String` selector is compared as a `string` (`docs/spec/types.md` 8.5)
        if selector.ty == Ty::Utf8String {
            selector = self.convert_u8(selector, Ty::String);
        }
        let declared = selector.ty;
        selector.ty = self.types.base(declared);
        let is_string = selector.ty == Ty::String;
        let label_ty = if declared.is_error() || declared.is_ordinal() || is_string {
            declared
        } else {
            let t = self.tn(declared);
            self.error(
                Code::E(739),
                format!("case selector must be an ordinal type or `string`, found `{t}`"),
                selector.span,
            );
            Ty::Error
        };
        // The labels so far, with their position (E0709)
        let mut seen: Vec<(CaseKey, TextRange)> = Vec::new();
        let mut arms = Vec::new();
        for arm in c.arms() {
            let mut labels = Vec::new();
            for lab in arm.labels() {
                let span = ast::trimmed_range(lab.syntax());
                let low = self.lower_case_label(lab.low(), label_ty, span);
                let high = if lab.is_range() {
                    let high = self.lower_case_label(lab.high(), label_ty, span);
                    if is_string {
                        self.error(
                            Code::E(709),
                            "a range is not allowed in a `string` case",
                            span,
                        );
                        continue;
                    }
                    Some(high)
                } else {
                    None
                };
                let key = match (&low.value, high.as_ref().map(|h| &h.value)) {
                    (Some(l), None) if is_string => l.as_str().map(CaseKey::Str),
                    (Some(l), None) => l.as_int().map(|v| CaseKey::Ord(v, v)),
                    (Some(l), Some(Some(h))) => match (l.as_int(), h.as_int()) {
                        (Some(lo), Some(hi)) => Some(CaseKey::Ord(lo, hi)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(key) = key {
                    let text = self.src(span).to_string();
                    if let CaseKey::Ord(lo, hi) = key
                        && lo > hi
                    {
                        self.error(
                            Code::E(709),
                            format!("empty case label range `{text}`"),
                            span,
                        );
                        continue;
                    }
                    if let Some((_, prev)) = seen.iter().find(|(k, _)| k.overlaps(&key)) {
                        let d = Diagnostic::error(
                            Code::E(709),
                            format!("duplicate case label `{text}`"),
                            Stage::Sema,
                        )
                        .with_primary(self.span(span), "")
                        .with_secondary(self.span(*prev), "previous label here");
                        self.push(d);
                    } else {
                        seen.push((key, span));
                    }
                }
                labels.push(CaseLabel { low, high });
            }
            let at = arm
                .colon_token()
                .map_or(ast::trimmed_range(arm.syntax()), |t| t.text_range());
            let body = arm.body().map(|b| Box::new(self.lower_body(Some(b), at)));
            arms.push(CaseArm { labels, body });
        }
        let else_body = c.else_branch().map(|e| self.lower_stmt_list(e.stmts()));
        StmtKind::Case {
            selector,
            arms,
            else_body,
        }
    }

    /// A label of a `case`: a constant (E0713) assignment compatible with the selector's type
    /// (E0702, E0714 out of its range). The error type after a diagnostic.
    fn lower_case_label(&mut self, e: Option<ast::Expr>, ty: Ty, at: TextRange) -> Expr {
        let Some(e) = e else {
            return Expr {
                kind: ExprKind::Error,
                ty: Ty::Error,
                value: None,
                span: at,
            };
        };
        let x = self.lower_expr(&e);
        if x.ty.is_error() || ty.is_error() {
            return x;
        }
        let x = self.coerce(x, ty, false, None);
        if x.ty.is_error() || x.value.is_some() {
            return x;
        }
        let span = x.span;
        self.error(Code::E(713), "constant expression required", span);
        Expr {
            kind: ExprKind::Error,
            ty: Ty::Error,
            value: None,
            span,
        }
    }

    /// `try..except` / `try..finally` (`docs/spec/exceptions.md` 5.2, 5.3).
    fn lower_try(&mut self, t: &ast::TryStmt) -> StmtKind {
        let body = self.lower_stmt_list(t.body());
        if let Some(f) = t.finally_block() {
            let depth = self.cur_mut().loop_depth;
            let saved = self.cur_mut().finally_loop_depth.replace(depth);
            let fin = self.lower_stmt_list(f.stmts());
            self.cur_mut().finally_loop_depth = saved;
            return StmtKind::Try {
                body,
                handler: TryHandler::Finally(fin),
            };
        }
        let Some(ex) = t.except_block() else {
            // A syntax error (no `except` / `finally`); the routine is not compiled
            return StmtKind::Block(body);
        };
        self.cur_mut().except_depth += 1;
        let mut handlers: Vec<Handler> = Vec::new();
        // The classes of the handlers so far, with the position of their type (W0019)
        let mut caught: Vec<(super::types::TypeId, TextRange)> = Vec::new();
        for h in ex.handlers() {
            let span = h
                .on_token()
                .map_or(ast::trimmed_range(h.syntax()), |t| t.text_range());
            let class = match h.type_ref() {
                Some(tr) => {
                    let range = tr.syntax().text_range();
                    match self.resolve_type_ref(&tr) {
                        ty @ Ty::Exception(id) => {
                            if let Some((_, prev)) = caught
                                .iter()
                                .find(|(c, _)| self.types.is_descendant(id, *c))
                            {
                                let name = self.src(range);
                                let d = Diagnostic::warning(
                                    Code::W(19),
                                    format!("handler for `{name}` is unreachable"),
                                    Stage::Sema,
                                )
                                .with_primary(self.span(range), "")
                                .with_secondary(self.span(*prev), "caught here");
                                self.push(d);
                            }
                            caught.push((id, range));
                            ty
                        }
                        Ty::Error => Ty::Error,
                        other => {
                            let t = self.tn(other);
                            self.error(
                                Code::E(733),
                                format!("`{t}` is not an exception class"),
                                range,
                            );
                            Ty::Error
                        }
                    }
                }
                None => Ty::Error,
            };
            self.cur_mut().scopes.push(Vec::new());
            let mut var = None;
            if let Some(name_tok) = h.name() {
                let name = self.text(&name_tok).to_string();
                let lower = name.to_ascii_lowercase();
                let range = name_tok.text_range();
                if !self.check_duplicate(&lower, range) {
                    let id = self.add_var(name, class, VarKind::Exception, range);
                    self.cur_mut()
                        .scopes
                        .last_mut()
                        .unwrap()
                        .push((lower, LSym::Var(id)));
                    var = Some(id);
                }
            }
            let body = h.body().and_then(|b| self.lower_stmt(&b)).map(Box::new);
            self.cur_mut().scopes.pop();
            handlers.push(Handler {
                var,
                class,
                body,
                span,
            });
        }
        let else_body = if ex.handlers().next().is_none() {
            Some(self.lower_stmt_list(ex.stmts()))
        } else {
            ex.else_branch().map(|e| self.lower_stmt_list(e.stmts()))
        };
        self.cur_mut().except_depth -= 1;
        StmtKind::Try {
            body,
            handler: TryHandler::Except {
                handlers,
                else_body,
            },
        }
    }

    /// `Exit`, `Break` or `Continue` that would leave a `finally` block (E0731;
    /// `docs/spec/exceptions.md` 5.3). `leaves_loop` is false for `Exit`.
    fn leaves_finally(&mut self, name: &str, range: TextRange, loop_jump: bool) -> bool {
        let Some(c) = self.cur.as_ref() else {
            return false;
        };
        let Some(depth) = c.finally_loop_depth else {
            return false;
        };
        if loop_jump && c.loop_depth > depth {
            return false;
        }
        let d = Diagnostic::error(
            Code::E(731),
            format!("`{name}` cannot leave a `finally` block"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "")
        .with_note("leaving a `finally` block would discard an exception that is being raised")
        .with_note("set a flag variable instead, and leave after the `try` statement");
        self.push(d);
        true
    }

    /// The exception of `raise X` / `RaiseOuterException(X)`: an exception class value (E0702).
    fn exception_value(&mut self, e: &ast::Expr) -> Expr {
        let root = Ty::Exception(super::types::EXCEPTION);
        let x = self.lower_expected(e, root);
        if x.ty.is_error() || matches!(x.ty, Ty::Exception(_)) {
            return x;
        }
        let t = self.tn(x.ty);
        let at = x.span;
        self.error(
            Code::E(702),
            format!("type mismatch: expected `Exception`, found `{t}`"),
            at,
        );
        Expr {
            kind: ExprKind::Error,
            ty: Ty::Error,
            value: None,
            span: at,
        }
    }

    /// `E.Free`, `E.Destroy` and `FreeAndNil(E)` of an exception (E3019; `docs/spec/exceptions.md`
    /// chapter 1), and other members of an exception written as a statement. `None` if the statement
    /// is not one of these.
    fn exception_member_stmt(
        &mut self,
        callee: &ast::Expr,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<()> {
        match callee {
            ast::Expr::Field(f) if self.is_value_chain(callee) => {
                let base = f.base()?;
                if !matches!(self.trial_type(&base), Ty::Exception(_)) {
                    return None;
                }
                let x = self.lower_expr(callee);
                for a in args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                if !x.ty.is_error() {
                    self.error(Code::E(402), "expected a statement", span);
                }
                Some(())
            }
            ast::Expr::NameRef(n)
                if n.text().eq_ignore_ascii_case("freeandnil")
                    && self.find_visible("freeandnil").is_none()
                    && args.len() == 1 =>
            {
                let v = args[0].value()?;
                if !matches!(self.trial_type(&v), Ty::Exception(_)) {
                    return None;
                }
                self.lower_expr(&v);
                let range = ast::trimmed_range(callee.syntax());
                let name = self.src(range);
                self.error(
                    Code::E(3019),
                    format!("`{name}` is not supported in Puy"),
                    range,
                )
                .notes
                .push(builtins::deleted_note(Code::E(3019)).into());
                Some(())
            }
            _ => None,
        }
    }

    fn lower_expr_stmt(&mut self, e: &ast::ExprStmt, span: TextRange) -> Option<StmtKind> {
        let expr = e.expr()?;
        let (callee, args): (ast::Expr, Vec<ast::Arg>) = match &expr {
            ast::Expr::Call(c) => (c.callee()?, c.args()),
            ast::Expr::NameRef(_) | ast::Expr::Field(_) => (expr.clone(), Vec::new()),
            _ => {
                // A form for which the parser has reported E0402
                self.lower_expr(&expr);
                return None;
            }
        };
        if self.exception_member_stmt(&callee, &args, span).is_some() {
            return None;
        }
        let (r, name, range) = self.resolve_name_expr(&callee)?;
        if matches!(&expr, ast::Expr::Call(c) if c.has_error_arg()) {
            // The argument list has a skipped range. No E0705 for the argument count (same as for expressions)
            for a in &args {
                for x in a.exprs() {
                    self.lower_expr(&x);
                }
            }
            return None;
        }
        match r {
            Resolved::Routine(id) => {
                let bare = !matches!(expr, ast::Expr::Call(_));
                let at = if bare { range } else { span };
                let id = match self.pick_overload(id, &args, at, bare) {
                    Pick::Routine(c) => c,
                    Pick::Failed => return None,
                };
                if bare && !self.check_bare_routine_stmt(id, range) {
                    return None;
                }
                let call = self.lower_routine_call(id, &args, span);
                if call.ty.is_error() && !matches!(call.kind, ExprKind::Call { .. }) {
                    return None;
                }
                Some(StmtKind::Expr(call))
            }
            Resolved::Func(f) => {
                let call = self.lower_builtin_call_stmt(f, &name, &args, range, span);
                call.map(StmtKind::Expr)
            }
            Resolved::Proc(p) => self.lower_builtin_proc(p, &args, range, span),
            Resolved::Type(t) => {
                // A type conversion written as a statement
                for a in &args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                if args.is_empty() {
                    self.error(
                        Code::E(708),
                        format!("`{}` is a type and cannot be used as a value", self.tn(t)),
                        range,
                    );
                } else {
                    self.error(Code::E(402), "expected a statement", span);
                }
                None
            }
            Resolved::Var(_) | Resolved::Global(_) | Resolved::Const { .. } | Resolved::Bool(_) => {
                if !matches!(expr, ast::Expr::Call(_)) {
                    self.error(Code::E(402), "expected a statement", span);
                } else {
                    for a in &args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    let d = Diagnostic::error(
                        Code::E(704),
                        format!("`{name}` is not a procedure or function"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "");
                    self.push(d);
                }
                None
            }
            Resolved::Unit(_) => {
                self.error(
                    Code::E(708),
                    format!("`{name}` is a unit and cannot be used as a value"),
                    range,
                );
                None
            }
            Resolved::ExcCreate(_) | Resolved::ExcCreateFmt(_) => {
                // A new exception object that nothing holds
                for a in &args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                self.error(Code::E(402), "expected a statement", span);
                None
            }
            Resolved::RaiseOuter => {
                // `docs/spec/exceptions.md` 5.4
                let in_except = self.cur.as_ref().is_some_and(|c| c.except_depth > 0);
                if !in_except {
                    for a in &args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(732),
                        "`RaiseOuterException` is only allowed in an `except` block",
                        range,
                    );
                    return None;
                }
                if args.len() != 1 {
                    for a in &args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of arguments: expected 1, found {}",
                            args.len()
                        ),
                        span,
                    );
                    return None;
                }
                let v = args[0].value()?;
                let x = self.exception_value(&v);
                Some(StmtKind::RaiseOuter(x))
            }
            Resolved::StdFile(f) => {
                for a in &args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                self.std_file_misuse(f, range);
                None
            }
            Resolved::Error => {
                for a in &args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                None
            }
        }
    }

    /// A statement consisting only of a routine name without parentheses (`F;`). E0704 if the routine takes arguments
    /// (`function `F` requires arguments`), or the recursive-call E0704 if it is the current function's name
    /// (`semantics.md` 4.3; a procedure may call itself without parentheses).
    /// Returns `true` if it may continue as a call.
    fn check_bare_routine_stmt(&mut self, id: RoutineId, range: TextRange) -> bool {
        let info = &self.routines[id.0 as usize];
        // The function being analyzed or one enclosing it (`docs/spec/semantics.md` 4.3, 4.9)
        let is_self = self.enclosing_function(id).is_some();
        let n = info.name.clone();
        let has_params = info.min_args() > 0;
        if is_self && info.ret.is_some() {
            let mut d = Diagnostic::error(
                Code::E(704),
                format!("recursive call to `{n}` needs parentheses"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.routine_decl_span(id), "declared here");
            if !has_params {
                d = d.with_fix(Fix::suggestion(
                    format!("replace with `{n}()`"),
                    vec![TextEdit::replace(self.input.file, range, format!("{n}()"))],
                ));
            }
            self.push(d);
            return false;
        }
        if has_params {
            let kind = if info.ret.is_some() {
                "function"
            } else {
                "procedure"
            };
            let d = Diagnostic::error(
                Code::E(704),
                format!("{kind} `{n}` requires arguments"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.routine_decl_span(id), "declared here");
            self.push(d);
            return false;
        }
        true
    }

    fn lower_builtin_call_stmt(
        &mut self,
        f: Builtin,
        name: &str,
        args: &[ast::Arg],
        range: TextRange,
        span: TextRange,
    ) -> Option<Expr> {
        // Same as a call in an expression (the return value is discarded)
        let e = self.lower_builtin_any(f, name, args, range, span);
        if e.ty.is_error() {
            return None;
        }
        Some(e)
    }

    /// Built-in procedures.
    fn lower_builtin_proc(
        &mut self,
        p: BuiltinProc,
        args: &[ast::Arg],
        range: TextRange,
        span: TextRange,
    ) -> Option<StmtKind> {
        let checks = self.checks_at(span.start());
        match p {
            BuiltinProc::Write | BuiltinProc::WriteLn => {
                self.lower_write(p == BuiltinProc::WriteLn, args, checks)
            }
            BuiltinProc::ReadLn | BuiltinProc::Read => {
                self.lower_read(p == BuiltinProc::ReadLn, args, span)
            }
            BuiltinProc::Flush => self.lower_flush(args, span),
            BuiltinProc::Inc | BuiltinProc::Dec => {
                self.check_no_width_pub(args);
                if args.is_empty() || args.len() > 2 {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of arguments: expected 1, found {}",
                            args.len()
                        ),
                        span,
                    );
                    return None;
                }
                let v = args[0].value()?;
                let (place, ty) = match self.lower_place(&v, PlaceUse::IncDec) {
                    Some(x) => x,
                    None => {
                        if let Some(super::expr::PlaceResult::NotVariable(r)) =
                            self.last_place_result.take()
                        {
                            self.error(
                                Code::E(706),
                                "argument 1 must be a variable (parameter `X` is `var`)",
                                r,
                            );
                        }
                        if let Some(d) = args.get(1).and_then(|a| a.value()) {
                            self.lower_expr(&d);
                        }
                        return None;
                    }
                };
                if ty.is_error() {
                    return None;
                }
                // An enumeration or subrange is also allowed (`docs/spec/semantics.md` 5.3)
                let base = self.types.base(ty);
                if !(base.is_integer() || matches!(base, Ty::Char | Ty::Enum(_))) {
                    let r = ast::trimmed_range(v.syntax());
                    self.error(Code::E(702), format!("type mismatch: expected an integer type or `Char`, found `{}` (argument 1, parameter `X`)", self.tn(ty)), r);
                    return None;
                }
                let delta = match args.get(1).and_then(|a| a.value()) {
                    Some(d) => {
                        let e = self.lower_expr(&d);
                        if e.ty.is_error() {
                            return None;
                        }
                        if !e.ty.is_integer() {
                            let r = e.span;
                            self.error(Code::E(702), format!("type mismatch: expected an integer type, found `{}` (argument 2, parameter `N`)", self.tn(e.ty)), r);
                            return None;
                        }
                        Some(e)
                    }
                    None => None,
                };
                // Computation type: same as X := X + N (`Char` is computed as `Word` and narrowed back to `Char`)
                let target_ty = match base {
                    Ty::Char => Ty::Word,
                    Ty::Enum(_) => Ty::Integer,
                    b => b,
                };
                let calc = match &delta {
                    Some(d) => {
                        let lhs = Expr {
                            kind: ExprKind::Error,
                            ty: target_ty,
                            value: None,
                            span: v.syntax().text_range(),
                        };
                        self.int_calc_type_pub(&lhs, d, span)?
                    }
                    None => {
                        if target_ty.fits_in_integer() {
                            Ty::Integer
                        } else {
                            target_ty
                        }
                    }
                };
                Some(StmtKind::IncDec {
                    target: place,
                    target_ty: ty,
                    delta,
                    negative: p == BuiltinProc::Dec,
                    calc,
                    checks,
                })
            }
            BuiltinProc::Assert => {
                self.check_no_width_pub(args);
                if args.is_empty() || args.len() > 2 {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of arguments: expected 1, found {}",
                            args.len()
                        ),
                        span,
                    );
                    return None;
                }
                let c = args[0].value()?;
                let cond = self.lower_expr(&c);
                let cond = self.coerce(cond, Ty::Boolean, false, Some((1, "Condition")));
                let message = match args.get(1).and_then(|a| a.value()) {
                    Some(m) => {
                        let e = self.lower_expr(&m);
                        let e = self.coerce(e, Ty::String, false, Some((2, "Message")));
                        if e.ty.is_error() {
                            return None;
                        }
                        Some(e)
                    }
                    None => None,
                };
                if cond.ty.is_error() {
                    return None;
                }
                if !self.assertions_at(span.start()) {
                    return Some(StmtKind::Nop);
                }
                let text = ast::source_text_normalized(c.syntax());
                Some(StmtKind::Assert {
                    cond,
                    message,
                    text,
                })
            }
            BuiltinProc::Halt => {
                self.check_no_width_pub(args);
                if args.len() > 1 {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of arguments: expected 1, found {}",
                            args.len()
                        ),
                        span,
                    );
                    return None;
                }
                let code = match args.first().and_then(|a| a.value()) {
                    Some(c) => {
                        let e = self.lower_expr(&c);
                        let e = self.coerce(e, Ty::Integer, checks.range, Some((1, "Code")));
                        if e.ty.is_error() {
                            return None;
                        }
                        Some(e)
                    }
                    None => {
                        // `Halt` without a code ends with `ExitCode` as it is (5.3)
                        self.exit_code_global();
                        None
                    }
                };
                Some(StmtKind::Halt(code))
            }
            BuiltinProc::Exit => {
                self.check_no_width_pub(args);
                if args.len() > 1 {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of arguments: expected 1, found {}",
                            args.len()
                        ),
                        span,
                    );
                    return None;
                }
                let ret = self.cur.as_ref().and_then(|c| c.ret);
                if self.leaves_finally("Exit", range, false) {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    return None;
                }
                let value = match args.first().and_then(|a| a.value()) {
                    Some(v) => {
                        let e = match ret {
                            Some(rt) => self.lower_expected(&v, rt),
                            None => self.lower_expr(&v),
                        };
                        let Some(rt) = ret else {
                            let d = Diagnostic::error(
                                Code::E(711),
                                "`Exit` with a value is not allowed in a procedure",
                                Stage::Sema,
                            )
                            .with_primary(self.span(range), "");
                            let d = match self
                                .cur
                                .as_ref()
                                .filter(|c| !c.is_main)
                                .map(|c| c.name_span)
                            {
                                Some(s) => d.with_secondary(self.span(s), "declared here"),
                                None => d,
                            };
                            self.push(d);
                            // Control-flow analysis treats this as `Exit` (statements after it are unreachable)
                            return Some(StmtKind::Exit(None));
                        };
                        // Keep it as an assignment to `Result` even if the expression is error-typed (avoids a duplicate E2001; 4.4)
                        Some(self.coerce(e, rt, checks.range, None))
                    }
                    None => None,
                };
                Some(StmtKind::Exit(value))
            }
            BuiltinProc::SetLength => self.lower_set_length(args, span),
            BuiltinProc::Insert | BuiltinProc::Delete => {
                self.lower_insert_delete(args, p == BuiltinProc::Delete, span)
            }
            BuiltinProc::Include | BuiltinProc::Exclude => {
                self.check_no_width_pub(args);
                self.lower_include(args, p == BuiltinProc::Exclude, span)
            }
            BuiltinProc::Break | BuiltinProc::Continue => {
                if !args.is_empty() {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of arguments: expected 0, found {}",
                            args.len()
                        ),
                        span,
                    );
                    return None;
                }
                if self.cur.as_ref().is_none_or(|c| c.loop_depth == 0) {
                    let name = builtins::proc_name(p);
                    self.error(Code::E(712), format!("`{name}` outside a loop"), range);
                    return None;
                }
                if self.leaves_finally(builtins::proc_name(p), range, true) {
                    return None;
                }
                Some(if p == BuiltinProc::Break {
                    StmtKind::Break
                } else {
                    StmtKind::Continue
                })
            }
        }
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// `SetLength(S, N)` of a string, `SetLength(A, N[, M…])` of a dynamic array with one length
    /// per level of nested dynamic arrays (`docs/spec/types.md` 2.3, chapter 7).
    fn lower_set_length(&mut self, args: &[ast::Arg], span: TextRange) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let lower_rest = |this: &mut Self| {
            for a in args.iter().skip(1) {
                if let Some(v) = a.value() {
                    this.lower_expr(&v);
                }
            }
        };
        let Some(v) = args.first().and_then(|a| a.value()) else {
            self.error(
                Code::E(705),
                "wrong number of arguments: expected 2, found 0",
                span,
            );
            return None;
        };
        let (place, ty) = match self.lower_place(&v, PlaceUse::SetLength) {
            Some(x) => x,
            None => {
                if let Some(super::expr::PlaceResult::NotVariable(r)) =
                    self.last_place_result.take()
                {
                    self.error(
                        Code::E(706),
                        "argument 1 must be a variable (parameter `S` is `var`)",
                        r,
                    );
                }
                lower_rest(self);
                return None;
            }
        };
        if ty.is_error() {
            lower_rest(self);
            return None;
        }
        // The number of lengths the type takes
        let mut dims = 0usize;
        let mut t = ty;
        if matches!(ty, Ty::String | Ty::Utf8String) {
            dims = 1;
        } else {
            while let Ty::DynArray(_) = t {
                dims += 1;
                t = self.types.dyn_elem(t).unwrap_or(Ty::Error);
            }
        }
        if dims == 0 {
            lower_rest(self);
            let r = ast::trimmed_range(v.syntax());
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected a string or a dynamic array, found `{}` (argument 1, parameter `S`)",
                    self.tn(ty)
                ),
                r,
            );
            return None;
        }
        if args.len() < 2 || args.len() > dims + 1 {
            lower_rest(self);
            let expected = if dims == 1 {
                "2".to_string()
            } else {
                format!("2 to {}", dims + 1)
            };
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {expected}, found {}",
                    args.len()
                ),
                span,
            );
            return None;
        }
        let checks = self.checks_at(span.start());
        let mut lens = Vec::new();
        let mut failed = false;
        for (i, a) in args.iter().enumerate().skip(1) {
            let Some(x) = a.value() else {
                failed = true;
                continue;
            };
            let e = self.lower_expr(&x);
            if e.ty.is_error() {
                failed = true;
                continue;
            }
            if !e.ty.is_integer() {
                let r = e.span;
                self.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected an integer type, found `{}` (argument {}, parameter `NewLength`)",
                        self.tn(e.ty),
                        i + 1
                    ),
                    r,
                );
                failed = true;
                continue;
            }
            lens.push(self.coerce(e, Ty::Int64, checks.range, None));
        }
        if failed {
            return None;
        }
        Some(StmtKind::SetLength {
            target: place,
            lens,
        })
    }
}

impl Sema<'_, '_> {
    /// `Insert(Source, var S, Index)` / `Delete(var S, Index, Count)` (`docs/spec/semantics.md`
    /// 5.3, `docs/spec/types.md` 8.6): `S` is a string, a `UTF8String` or a dynamic array.
    fn lower_insert_delete(
        &mut self,
        args: &[ast::Arg],
        delete: bool,
        span: TextRange,
    ) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let target_at = if delete { 0 } else { 1 };
        let lower_all = |this: &mut Self, skip: Option<usize>| {
            for (i, a) in args.iter().enumerate() {
                if Some(i) != skip
                    && let Some(v) = a.value()
                {
                    this.lower_expr_unchecked_ctor(&v);
                }
            }
        };
        if args.len() != 3 {
            lower_all(self, None);
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 3, found {}",
                    args.len()
                ),
                span,
            );
            return None;
        }
        let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        if values.len() != 3 {
            lower_all(self, None);
            return None;
        }
        let s = &values[target_at];
        let Some((place, ty)) = self.lower_place(s, PlaceUse::IncDec) else {
            if let Some(super::expr::PlaceResult::NotVariable(r)) = self.last_place_result.take() {
                self.error(
                    Code::E(706),
                    format!(
                        "argument {} must be a variable (parameter `S` is `var`)",
                        target_at + 1
                    ),
                    r,
                );
            }
            lower_all(self, Some(target_at));
            return None;
        };
        if ty.is_error() {
            lower_all(self, Some(target_at));
            return None;
        }
        if !matches!(ty, Ty::String | Ty::Utf8String | Ty::DynArray(_)) {
            lower_all(self, Some(target_at));
            let r = ast::trimmed_range(s.syntax());
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected a string or a dynamic array, found `{}` (argument {}, parameter `S`)",
                    self.tn(ty),
                    target_at + 1
                ),
                r,
            );
            return None;
        }
        let checks = self.checks_at(span.start());
        let int_arg = |this: &mut Self, e: &ast::Expr, n: usize, name: &'static str| {
            let x = this.lower_expr(e);
            if x.ty.is_error() {
                return None;
            }
            if !x.ty.is_integer() {
                let r = x.span;
                this.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected an integer type, found `{}` (argument {n}, parameter `{name}`)",
                        this.tn(x.ty)
                    ),
                    r,
                );
                return None;
            }
            Some(this.coerce(x, Ty::Int64, checks.range, None))
        };
        if delete {
            let index = int_arg(self, &values[1], 2, "Index");
            let count = int_arg(self, &values[2], 3, "Count");
            return Some(StmtKind::Delete {
                target: place,
                index: index?,
                count: count?,
            });
        }
        let source = if let Ty::DynArray(_) = ty {
            let elem = self.types.dyn_elem(ty).unwrap_or(Ty::Error);
            let x = self.lower_expected(&values[0], ty);
            let whole = x.ty != elem
                && matches!(x.ty, Ty::DynArray(id) | Ty::OpenArray(id) if Ty::DynArray(id) == ty);
            if whole {
                Some((x, true))
            } else {
                let x = self.coerce(x, elem, checks.range, Some((1, "Source")));
                (!x.ty.is_error()).then_some((x, false))
            }
        } else {
            let x = self.lower_expr(&values[0]);
            let x = self.coerce(x, ty, checks.range, Some((1, "Source")));
            (!x.ty.is_error()).then_some((x, false))
        };
        let index = int_arg(self, &values[2], 3, "Index");
        let (source, whole) = source?;
        Some(StmtKind::Insert {
            source,
            target: place,
            index: index?,
            whole,
        })
    }
}

/// Whether the body of a `for` declares an inline variable or constant with the same name.
fn body_declares_name(body: &crate::syntax::SyntaxNode, lower: &str, bytes: &[u8]) -> bool {
    use crate::syntax::SyntaxKind::*;
    body.descendants().any(|n| {
        if !matches!(n.kind(), INLINE_VAR_DECL | INLINE_CONST_DECL | FOR_VAR) {
            return false;
        }
        n.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == IDENT)
            .is_some_and(|t| {
                let r = t.text_range();
                bytes[usize::from(r.start())..usize::from(r.end())]
                    .eq_ignore_ascii_case(lower.as_bytes())
            })
    })
}

/// An expression that takes its type from the context: an array constructor or `nil` (possibly
/// in parentheses).
pub(super) fn is_contextual(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::ArrayCtor(_) | ast::Expr::Nil(_) => true,
        ast::Expr::Paren(p) => p.inner().as_ref().is_some_and(is_contextual),
        _ => false,
    }
}

#[allow(dead_code)]
fn _unused(_: ParamMode) {}

/// The outcome of resolving a `for` loop variable.
enum LoopVar {
    Resolved(ForVar, Ty),
    /// E0700 was reported; the caller keeps type-checking the rest of the loop.
    Undeclared,
    /// A diagnostic was reported (or there is no name).
    Failed,
}

/// The values a `case` label covers (E0709).
enum CaseKey {
    /// An ordinal value or range (`low..high`, ordinals).
    Ord(i128, i128),
    Str(Vec<u16>),
}

impl CaseKey {
    fn overlaps(&self, other: &CaseKey) -> bool {
        match (self, other) {
            (CaseKey::Ord(a, b), CaseKey::Ord(c, d)) => a <= d && c <= b,
            (CaseKey::Str(a), CaseKey::Str(b)) => a == b,
            _ => false,
        }
    }
}
