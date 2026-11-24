//! Typing of statements (`docs/spec/semantics.md` chapters 4–6).

use super::builtins::{self, BuiltinProc};
use super::expr::PlaceUse;
use super::hir::*;
use super::overload::Pick;
use super::types::{Const, MethodKind, Ty};
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
                let (value, ty, explicit) = self.eval_const_decl(c.type_ref(), c.value(), false);
                if self.check_duplicate(&lower, range) {
                    return None;
                }
                self.note_typed_const(c.type_ref().is_some(), range);
                self.note_explicit_const(explicit, range);
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
        // Not converted by `Implicit` (`docs/spec/objects.md` 11.4)
        self.coerce_with(e, Ty::Boolean, false, None, false)
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
        // `O.P := X` (`docs/spec/objects.md` 2.6)
        if let Some(s) = self.assign_property(&target, value_ast.clone(), span) {
            return s;
        }
        // Type the right-hand side first (the target variable is read after the right-hand side),
        // except an array constructor and `nil`, which take their type from the target
        // A class's name too, which is a value where a class reference is expected
        // (`docs/spec/objects.md` 2.8)
        let contextual = value_ast.as_ref().is_some_and(is_contextual)
            || value_ast
                .as_ref()
                .is_some_and(|v| self.class_name_of(v).is_some())
            // A routine or method name, or a procedural value, assigned to a procedural
            // variable is a value (`docs/spec/objects.md` 9.4)
            || (value_ast.as_ref().is_some_and(|v| {
                matches!(
                    v,
                    ast::Expr::NameRef(_)
                        | ast::Expr::Field(_)
                        | ast::Expr::Index(_)
                        | ast::Expr::Inherited(_)
                )
            }) && self
                .dry_run(|s| s.lower_place(&target, PlaceUse::Assign))
                .is_some_and(|(_, t)| t.is_procedural()));
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
        self.check_self_cycle(&target, &value);
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
        let ty = self.decl_attributes(v.syntax(), super::attrs::AttrSite::Var, ty);
        self.flush_attribute_checks();
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
        let Some(value) = init else {
            // A captured variable gets a new cell where it is declared (`docs/spec/objects.md` 9.6)
            return Some(Stmt {
                kind: StmtKind::Declare(id),
                span,
            });
        };
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
            from: Box::new(from),
            to: Box::new(to),
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
        let coll = f.from().map(|e| match &e {
            // An array constructor after a written loop variable type that is in error: the
            // type's error is enough (no E0729 on top)
            ast::Expr::ArrayCtor(ac) if self.loop_var_type_failed(&fv) => {
                self.lower_ctor_elements_quietly(ac);
                super::expr::err_expr(ast::trimmed_range(e.syntax()))
            }
            _ => self.lower_expr(&e),
        });
        if let Some(c) = coll.as_ref().filter(|c| !self.iterates_itself(c.ty)) {
            let c = c.clone();
            return self.lower_for_in_enum(f, &fv, checks, c);
        }
        let elem = coll.as_ref().map_or(Ty::Error, |c| self.iteration_elem(c));
        self.cur_mut().scopes.push(Vec::new());
        let (var, var_ty) = match self.lower_loop_var(f, &fv, elem, false) {
            // A `[weak]` loop variable takes the element without counting it
            // (`docs/spec/objects.md` 4.4)
            LoopVar::Resolved(var, ty) => (var, self.types.strong(ty)),
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
            coll: Box::new(coll),
            coll_var,
            idx_var,
            elem,
            conv,
            body: Box::new(body),
            enumerator: None,
        })
    }

    /// Whether an inline loop variable's written type is in error (the parser skipped it, or it
    /// does not resolve); its diagnostics come when the variable is declared.
    fn loop_var_type_failed(&mut self, fv: &ast::ForVar) -> bool {
        if !fv.is_inline() || fv.colon_token().is_none() {
            return false;
        }
        match fv.type_ref() {
            Some(t) => self.dry_run(|s| s.resolve_type(Some(t)).is_error()),
            None => true,
        }
    }

    /// Whether `for..in` iterates over a value of the type by itself (`docs/spec/types.md`
    /// chapter 5): an array, a string or a set (and the error type, already reported).
    fn iterates_itself(&self, ty: Ty) -> bool {
        ty.is_error() || ty == Ty::String || ty.is_set() || self.types.elem_of(ty).is_some()
    }

    /// The element type of a `for..in` collection that iterates by itself: an array's element
    /// type, `Char` for a string, a set's base type.
    fn iteration_elem(&mut self, coll: &Expr) -> Ty {
        match coll.ty {
            Ty::String => Ty::Char,
            // A set's elements have its base type (`docs/spec/types.md` 8.3)
            t if t.is_set() => self.types.set_base(t).unwrap_or(Ty::Error),
            t => self.types.elem_of(t).unwrap_or(Ty::Error),
        }
    }

    /// `for X in C` with `GetEnumerator` (`docs/spec/objects.md` chapter 13): hidden variables
    /// hold the collection and the enumerator, and the element is `E.Current` converted to the
    /// loop variable's type.
    fn lower_for_in_enum(
        &mut self,
        f: &ast::ForStmt,
        fv: &ast::ForVar,
        checks: Checks,
        coll: Expr,
    ) -> Option<StmtKind> {
        let range = fv
            .name()
            .map_or(fv.syntax().text_range(), |t| t.text_range());
        let coll_var = self.add_var(String::new(), coll.ty, VarKind::Hidden, range);
        let calls = self.enumerator_calls(&coll, coll_var);
        let elem = calls.as_ref().map_or(Ty::Error, |c| c.current.ty);
        self.cur_mut().scopes.push(Vec::new());
        let (var, var_ty) = match self.lower_loop_var(f, fv, elem, false) {
            // A `[weak]` loop variable takes the element without counting it
            // (`docs/spec/objects.md` 4.4)
            LoopVar::Resolved(var, ty) => (var, ty),
            LoopVar::Undeclared => {
                self.lower_loop_body(f.body());
                return None;
            }
            LoopVar::Failed => return None,
        };
        let current = calls.as_ref().and_then(|c| {
            if var_ty.is_error() {
                return None;
            }
            let strong = self.types.strong(var_ty);
            let e = self.coerce(c.current.clone(), strong, checks.range, None);
            (!e.ty.is_error()).then_some(e)
        });
        // A global loop variable gets the element of the last entry again on exit
        let last = match var {
            ForVar::Global(g) if current.is_some() => Some(self.add_var(
                String::new(),
                self.globals[g.0 as usize].ty,
                VarKind::Hidden,
                range,
            )),
            _ => None,
        };
        let body = self.lower_loop_var_body(var, f.body());
        let (Some(calls), Some(current)) = (calls, current) else {
            return None;
        };
        Some(StmtKind::ForIn {
            var,
            var_span: fv.syntax().text_range(),
            coll: Box::new(coll),
            coll_var,
            idx_var: calls.var,
            elem,
            conv: None,
            body: Box::new(body),
            enumerator: Some(Box::new(Enumerator {
                get: calls.get,
                move_next: calls.move_next,
                current,
                last,
                checks,
                cleanup: calls.cleanup,
            })),
        })
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

    /// E0722 for a loop variable of a type that is not ordinal, or E0762 for one of a type
    /// parameter (`docs/spec/generics.md` 3.3).
    fn loop_var_type_error(&mut self, ordinal: bool, ty: Ty, range: TextRange) {
        if let Ty::Param(p) = ty {
            self.param_misuse(p, super::generics::ParamOp::Ordinal, range);
            return;
        }
        let d = self
            .loop_var_error(ordinal)
            .with_primary(self.span(range), "");
        self.push(d);
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
        // A distinct type as its root (`docs/spec/types.md` 1.3)
        let wrong_type = |types: &super::types::TypeTable, ty: Ty| {
            ordinal && !ty.is_error() && !types.root(ty).is_ordinal()
        };
        if fv.is_inline() {
            let ty = match fv.type_ref() {
                Some(t) => self.resolve_type(Some(t)),
                // A written type the parser reported and skipped (`docs/spec/grammar.md` 1.3):
                // the variable has the error type, not the inferred one, so nothing piles on
                None if fv.colon_token().is_some() => Ty::Error,
                None => inferred,
            };
            if wrong_type(&self.types, ty) {
                self.loop_var_type_error(ordinal, ty, range);
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
                    VarKind::Param(ast::ParamMode::Var | ast::ParamMode::Out) | VarKind::RecordSelf
                ) {
                    let d = self
                        .loop_var_error(ordinal)
                        .with_primary(self.span(range), "")
                        .with_secondary(self.span(v.span), "declared here")
                        .with_note("a `var` or `out` parameter refers to the caller's variable; copy it to a local variable first");
                    self.push(d);
                    return LoopVar::Failed;
                }
                if wrong_type(&self.types, v.ty) {
                    self.loop_var_type_error(ordinal, v.ty, range);
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
                    .with_note(super::LOOP_VAR_WRITE_NOTE);
                    self.push(d);
                    // No W0001 piles on
                    self.cur_mut().vars[id.0 as usize].is_loop_var = true;
                    return LoopVar::Failed;
                }
                self.cur_mut().vars[id.0 as usize].is_loop_var = true;
                self.cur_mut().vars[id.0 as usize].written = true;
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
                        if wrong_type(&self.types, g.ty) {
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
        let selector = match c.selector() {
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
        // A distinct type as its root (`docs/spec/types.md` 1.3)
        let mut selector = self.rooted(selector);
        // A `UTF8String` selector is compared as a `string` (`docs/spec/types.md` 8.5)
        if selector.ty == Ty::Utf8String {
            selector = self.convert_u8(selector, Ty::String);
        }
        let declared = selector.ty;
        selector.ty = self.types.base(declared);
        let is_string = selector.ty == Ty::String;
        let label_ty = if declared.is_error() || declared.is_ordinal() || is_string {
            declared
        } else if let Ty::Param(p) = declared {
            // `docs/spec/generics.md` 3.3
            self.param_misuse(p, super::generics::ParamOp::Ordinal, selector.span);
            Ty::Error
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

    /// W0019 of a handler (its type at `range`) that a handler before it (at `prev`) catches.
    fn unreachable_handler(&mut self, range: TextRange, prev: TextRange) {
        let name = self.src(range);
        let d = Diagnostic::warning(
            Code::W(19),
            format!("handler for `{name}` is unreachable"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "")
        .with_secondary(self.span(prev), "caught here");
        self.push(d);
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
            // A name, or a name with type arguments (`docs/spec/generics.md` 1.3)
            let class = match h.syntax().children().find_map(ast::TypeSpec::cast) {
                Some(tr) => {
                    let range = tr.syntax().text_range();
                    match self.resolve_type(Some(tr.clone())) {
                        ty @ Ty::Class(id) if self.types.is_exception_class(id) => {
                            if let Some((_, prev)) = caught
                                .iter()
                                .find(|(c, _)| self.types.is_descendant(id, *c))
                            {
                                self.unreachable_handler(range, *prev);
                            }
                            caught.push((id, range));
                            ty
                        }
                        // A type parameter whose class constraint is an exception class: the
                        // instantiation catches its type argument (`docs/spec/generics.md` 3.3).
                        // It is unreachable after a handler of an ancestor of the constraint;
                        // it does not make a later handler unreachable, since the argument may
                        // be a descendant of the constraint
                        ty @ Ty::Param(p)
                            if self
                                .param_class(p)
                                .is_some_and(|c| self.types.is_exception_class(c)) =>
                        {
                            let c = self.param_class(p).expect("checked above");
                            if let Some((_, prev)) =
                                caught.iter().find(|(k, _)| self.types.is_descendant(c, *k))
                            {
                                self.unreachable_handler(range, *prev);
                            }
                            ty
                        }
                        Ty::Error => Ty::Error,
                        other => {
                            let t = self.tn(other);
                            let d = self.error(
                                Code::E(733),
                                format!("`{t}` is not an exception class"),
                                range,
                            );
                            if matches!(other, Ty::Class(_)) {
                                // Only exceptions are raised (`docs/spec/objects.md` 8.3)
                                d.notes
                                    .push("every exception derives from `Exception`".into());
                            } else if matches!(other, Ty::Param(_)) {
                                d.notes.push(format!(
                                    "add a class constraint that derives from `Exception`, as in `<{t}: Exception>`"
                                ));
                            }
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

    /// The exception of `raise X` / `RaiseOuterException(X)`: a value of an exception class
    /// (`docs/spec/objects.md` 8.3). Another class (`TObject` too) and an interface are E3021,
    /// any other type E0702.
    fn exception_value(&mut self, e: &ast::Expr) -> Expr {
        let root = Ty::Class(super::types::EXCEPTION);
        let x = self.lower_expected(e, root);
        if x.ty.is_error() {
            return x;
        }
        if let Ty::Class(id) = x.ty
            && self.types.is_exception_class(id)
        {
            return x;
        }
        let t = self.tn(x.ty);
        let at = x.span;
        if let Ty::Class(_) | Ty::Intf(_) = x.ty {
            self.error(
                Code::E(3021),
                format!("raising `{t}` is not supported in Puy"),
                at,
            )
            .notes
            .push(builtins::deleted_note(Code::E(3021)).into());
        } else {
            self.error(
                Code::E(702),
                format!("type mismatch: expected `Exception`, found `{t}`"),
                at,
            );
        }
        Expr {
            kind: ExprKind::Error,
            ty: Ty::Error,
            value: None,
            span: at,
        }
    }

    /// `FreeAndNil(E)` of an exception without `System.SysUtils` (E3019, as in M2;
    /// `docs/spec/exceptions.md` chapter 1, `docs/spec/objects.md` 8.6): the class rule needs
    /// the unit for the name. `None` if the statement is not this one.
    fn exception_free_and_nil(&mut self, callee: &ast::Expr, args: &[ast::Arg]) -> Option<()> {
        let ast::Expr::NameRef(n) = callee else {
            return None;
        };
        if !(n.text().eq_ignore_ascii_case("freeandnil")
            && self.find_visible("freeandnil").is_none()
            && args.len() == 1)
        {
            return None;
        }
        let v = args[0].value()?;
        match self.trial_type(&v) {
            Ty::Class(id) if self.types.is_exception_class(id) => {}
            _ => return None,
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

    /// A statement `O.Name` / `O.Name(…)` on a class instance (`docs/spec/objects.md` 3.2):
    /// `Free` / `Destroy` (when the class has no field of that name) destroy the instance;
    /// anything else is not a statement. `None` if `callee` is not a member of an instance.
    fn class_member_stmt(
        &mut self,
        callee: &ast::Expr,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<Option<StmtKind>> {
        let ast::Expr::Field(f) = callee else {
            return None;
        };
        if !self.is_value_chain(callee) {
            return None;
        }
        let base = f.base()?;
        let Ty::Class(id) = self.trial_type(&base) else {
            return None;
        };
        let field = f.field()?;
        let lower = self.text(&field).to_ascii_lowercase();
        let has_field = self
            .types
            .class_fields(id)
            .iter()
            .any(|(_, x)| x.name.eq_ignore_ascii_case(&lower));
        if matches!(lower.as_str(), "free" | "destroy") && !has_field {
            let target = self.lower_expr(&base);
            let values: Vec<Expr> = args
                .iter()
                .filter_map(|a| a.value())
                .map(|v| self.lower_expr(&v))
                .collect();
            if !values.is_empty() {
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected 0, found {}",
                        values.len()
                    ),
                    span,
                );
                return Some(None);
            }
            if target.ty.is_error() {
                return Some(None);
            }
            let name_range = field.text_range();
            return Some(self.free_stmt(target, lower == "destroy", name_range, Some(span)));
        }
        if has_field {
            let x = self.lower_expr(callee);
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            if !x.ty.is_error() {
                self.error(Code::E(402), "expected a statement", span);
            }
            return Some(None);
        }
        // A method call (`docs/spec/objects.md` 2.3): resolved as a name
        None
    }

    fn lower_expr_stmt(&mut self, e: &ast::ExprStmt, span: TextRange) -> Option<StmtKind> {
        let expr = e.expr()?;
        let (callee, args): (ast::Expr, Vec<ast::Arg>) = match &expr {
            ast::Expr::Call(c) => (c.callee()?, c.args()),
            ast::Expr::NameRef(_) | ast::Expr::Field(_) | ast::Expr::Inherited(_) => {
                (expr.clone(), Vec::new())
            }
            _ => {
                // A form for which the parser has reported E0402: typed as a value, so a
                // procedural value (`A[0];`, `(A[0]);`) is not called and gets no E0725 on top
                // of it
                let mut inner = expr.clone();
                while let ast::Expr::Paren(p) = &inner
                    && let Some(e) = p.inner()
                {
                    inner = e;
                }
                if let ast::Expr::Call(c) = &inner {
                    // `(P());`: a call is not used as a value either
                    self.lower_call(c, ast::trimmed_range(inner.syntax()), false);
                } else {
                    self.lower_value(&inner);
                }
                return None;
            }
        };
        if let ast::Expr::Inherited(i) = &expr
            && i.name().is_none()
        {
            return self.bare_inherited(span);
        }
        if !matches!(&expr, ast::Expr::Call(c) if c.has_error_arg())
            && let Some(v) = self.procedural_callee(&callee)
        {
            // A call of a procedural value, with or without parentheses
            // (`docs/spec/objects.md` 9.4)
            let call = self.call_value(v, &args, span);
            return matches!(call.kind, ExprKind::CallValue { .. }).then_some(StmtKind::Expr(call));
        }
        if let Some(k) = self.class_member_stmt(&callee, &args, span) {
            return k;
        }
        if self.exception_free_and_nil(&callee, &args).is_some() {
            return None;
        }
        if let ast::Expr::Call(c) = &expr
            && matches!(callee, ast::Expr::Paren(_) | ast::Expr::Call(_))
        {
            // `(F)()`, `(procedure begin … end)()`, `O.H()()`: typed as the call it is (no name to
            // resolve), so its errors are reported (E0725 for calling the result of a procedure)
            // and a call of a value is kept
            let call = self.lower_call(c, span, false);
            return matches!(call.kind, ExprKind::CallValue { .. }).then_some(StmtKind::Expr(call));
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
                self.routine_hint_use(id, range);
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
            Resolved::Key(op, t) => {
                let call = self.lower_key_call(op, t, &args, span);
                (!call.ty.is_error()).then_some(StmtKind::Expr(call))
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
            Resolved::Member {
                recv,
                class,
                member,
                inherited,
            } => {
                let bare = !matches!(expr, ast::Expr::Call(_));
                if bare
                    && matches!(
                        member,
                        super::methods::Member::Field { .. } | super::methods::Member::Property(..)
                    )
                {
                    self.error(Code::E(402), "expected a statement", span);
                    return None;
                }
                if let super::methods::Member::Methods { name: n, .. } = &member
                    && class == super::types::TINDEXRANGE
                    && n.eq_ignore_ascii_case(super::types::INDEX_RANGE_CLAMP)
                {
                    // Written out here (`docs/spec/objects.md` 12.3, 14.10)
                    return self.index_range_clamp(*recv, &args, span);
                }
                if let super::methods::Member::Methods { name: n, .. } = &member
                    && args.is_empty()
                    && matches!(recv.ty, Ty::Class(_))
                    && let Some((_, m)) = self.methods_named(class, n).first()
                {
                    // `Free` / `Destroy` inside a method destroy `Self` (`docs/spec/objects.md`
                    // 3.2); `inherited Destroy` calls the parent's destructor
                    let free = m.routine.is_none() && m.name == "Free";
                    if free || (m.kind == MethodKind::Destructor && !inherited) {
                        return self.free_stmt(*recv, !free, range, None);
                    }
                }
                let args = (!bare).then_some(&args[..]);
                let x = self.member_expr(*recv, class, member, inherited, args, false, range, span);
                (!x.ty.is_error()
                    || matches!(
                        x.kind,
                        ExprKind::Call { .. } | ExprKind::InheritedBuiltin { .. }
                    ))
                .then_some(StmtKind::Expr(x))
            }
            Resolved::ClassCreate(id, ctor) if !self.types.is_exception_class(id) => {
                // A new instance that nothing holds: one of a class managed by hand leaks
                // (`docs/spec/objects.md` 4.1)
                let x = self.new_instance(id, &ctor, &args, span, range, None);
                (!x.ty.is_error()).then_some(StmtKind::Expr(x))
            }
            Resolved::ClassCreate(..) => {
                // A new exception object that nothing holds: not a statement, as in M2
                // (`docs/spec/objects.md` 8.6)
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
                // The name has been diagnosed: the arguments have no parameters to take a type
                // from (no E0702 for `nil`, no E0729 for a constructor)
                for a in &args {
                    if let Some(v) = a.value()
                        && !self.bare_routine_arg(&v)
                    {
                        self.lower_expr_unchecked_ctor(&v);
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
            BuiltinProc::RequestLeakReport => {
                if !args.is_empty() {
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
                Some(StmtKind::RequestLeakReport)
            }
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
                // `class operator Inc` / `Dec`: `X := Inc(X)` (`docs/spec/objects.md` 11.2)
                if self.operator_owner(ty).is_some() {
                    let name = if p == BuiltinProc::Dec { "Dec" } else { "Inc" };
                    let value = self.lower_expr(&v);
                    if value.ty.is_error() {
                        return None;
                    }
                    let call = self.unary_operator(value, name, name, range, span);
                    if let Ok(x) = call {
                        if let Some(d) = args.get(1).and_then(|a| a.value()) {
                            self.lower_expr(&d);
                            self.error(
                                Code::E(705),
                                "wrong number of arguments: expected 1, found 2",
                                span,
                            );
                            return None;
                        }
                        if x.ty.is_error() {
                            return None;
                        }
                        let value = self.coerce(x, ty, checks.range, None);
                        if value.ty.is_error() {
                            return None;
                        }
                        return Some(StmtKind::Assign {
                            target: place,
                            value,
                            check: checks,
                            is_init: false,
                        });
                    }
                }
                // An enumeration or subrange is also allowed (`docs/spec/semantics.md` 5.3), and a
                // distinct type as its root (`docs/spec/types.md` 1.3)
                let base = self.types.base(self.types.root(ty));
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
                    delta: delta.map(Box::new),
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
                let cond =
                    self.coerce_with(cond, Ty::Boolean, false, Some((1, "Condition")), false);
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
                // `docs/spec/semantics.md` 5.4: the subexpressions a failure records
                let mut cond = cond;
                let (probes, marks) = super::probes::rewrite(
                    &mut cond,
                    &AssertSource {
                        sema: self,
                        node: c.syntax(),
                    },
                );
                Some(StmtKind::Assert {
                    cond: Box::new(cond),
                    message: message.map(Box::new),
                    text,
                    probes,
                    marks,
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
                            // A constructor has no result either (`docs/spec/objects.md` 3.1)
                            let what = match self
                                .cur
                                .as_ref()
                                .and_then(|c| c.routine)
                                .and_then(|r| self.method_of_routine(r))
                                .map(|(_, m)| m.kind)
                            {
                                Some(MethodKind::Constructor) => "constructor",
                                Some(MethodKind::Destructor) => "destructor",
                                _ => "procedure",
                            };
                            let d = Diagnostic::error(
                                Code::E(711),
                                format!("`Exit` with a value is not allowed in a {what}"),
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
            BuiltinProc::Exchange => self.lower_exchange(args, span),
            BuiltinProc::ArrayMoveRange => self.lower_array_move_range(args, span),
            BuiltinProc::ArraySortRange => self.lower_array_sort_range(args, span),
            BuiltinProc::HashRelink => self.lower_hash_relink(args, span),
            BuiltinProc::FreeIfClass => self.lower_stdlib_value_proc(args, false, span),
            BuiltinProc::CheckDefaultOrder => self.lower_stdlib_value_proc(args, true, span),
            BuiltinProc::CheckClassType => self.lower_check_class_type(args, span),
            BuiltinProc::FreeAndNil => self.lower_free_and_nil(args, range, span),
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
            // A distinct type of a string as its root (`docs/spec/types.md` 1.3)
            Some((p, t)) => (p, self.types.root(t)),
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
    /// `FreeAndNil(var Obj)` (`docs/spec/objects.md` 3.2): `Obj` is a variable of any class type
    /// (the exact type rule of `var` parameters does not apply).
    fn lower_free_and_nil(
        &mut self,
        args: &[ast::Arg],
        name_range: TextRange,
        span: TextRange,
    ) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        if args.len() != 1 {
            for v in &values {
                self.lower_expr_unchecked_ctor(v);
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
        let v = values.first()?;
        let (place, ty) = self.ref_place(v, (1, "Obj"), false, None)?;
        if ty.is_error() {
            return None;
        }
        if matches!(ty, Ty::Class(_)) && self.types.slot_kind(ty) == crate::bytecode::SLOT_REF {
            // A reference counted class (`docs/spec/objects.md` 3.2)
            let callee = ast::trimmed_range(v.syntax());
            let text = self.src(callee);
            self.ref_counted_free("FreeAndNil", name_range, Some((span, text)));
            return None;
        }
        if let Ty::Param(p) = ty {
            // A type parameter that takes only manually managed classes
            // (`docs/spec/generics.md` 3.3)
            if self.param_frees(p) {
                return Some(StmtKind::FreeAndNil { target: place });
            }
            let r = ast::trimmed_range(v.syntax());
            self.param_misuse(p, super::generics::ParamOp::Free, r);
            return None;
        }
        if !matches!(ty, Ty::Class(_)) {
            let r = ast::trimmed_range(v.syntax());
            let d = Diagnostic::error(
                Code::E(702),
                format!(
                    "type mismatch: expected a class type, found `{}` (argument 1, parameter `Obj`)",
                    self.tn(ty)
                ),
                Stage::Sema,
            )
            .with_primary(self.span(r), "")
            .with_note("`FreeAndNil` takes a variable of a class type");
            self.push(d);
            return None;
        }
        Some(StmtKind::FreeAndNil { target: place })
    }

    /// `O.Free` / `O.Destroy` (`docs/spec/objects.md` 3.2): the destruction of an instance, or
    /// E3019 when the static type of `target` is a reference counted class (`name` is the
    /// member as written; `stmt`, the whole statement, which the fix replaces with `O := nil`
    /// when `target` can be assigned).
    fn free_stmt(
        &mut self,
        target: Expr,
        destroy: bool,
        name: TextRange,
        stmt: Option<TextRange>,
    ) -> Option<StmtKind> {
        if self.types.slot_kind(target.ty) == crate::bytecode::SLOT_REF {
            let assignable =
                matches!(
                    target.kind,
                    // `Self` and `const` parameters cannot be assigned
                    ExprKind::Var(v) if self.cur.as_ref().is_some_and(|c| !matches!(
                        c.vars[v.0 as usize].kind,
                        VarKind::SelfParam | VarKind::Param(ParamMode::Const)
                    ))
                ) || matches!(target.kind, ExprKind::Global(_) | ExprKind::ObjField { .. });
            let fix = stmt
                .filter(|_| assignable)
                .map(|s| (s, self.src(target.span)));
            let member = if destroy { "Destroy" } else { "Free" };
            self.ref_counted_free(member, name, fix);
            return None;
        }
        Some(StmtKind::Free { target, destroy })
    }

    /// E3019 for `Free` / `Destroy` / `FreeAndNil` of a reference counted class
    /// (`docs/spec/objects.md` 3.2): `fix` is the statement and the text of the variable to
    /// assign `nil` to instead.
    fn ref_counted_free(
        &mut self,
        member: &str,
        range: TextRange,
        fix: Option<(TextRange, String)>,
    ) {
        let mut d = Diagnostic::error(
            Code::E(3019),
            format!("`{member}` is not supported in Puy"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "")
        .with_note(builtins::deleted_note(Code::E(3019)));
        if let Some((stmt, var)) = fix {
            d = d.with_fix(Fix::suggestion(
                format!("replace with `{var} := nil`"),
                vec![TextEdit::replace(
                    self.input.file,
                    stmt,
                    format!("{var} := nil"),
                )],
            ));
        }
        self.push(d);
    }

    /// `Exchange(var A, I, J)` (`docs/spec/types.md` chapter 7): `A` is a dynamic or static array,
    /// `I` and `J` its indices.
    fn lower_exchange(&mut self, args: &[ast::Arg], span: TextRange) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        let lower_rest = |this: &mut Self, from: usize| {
            for v in &values[from..] {
                this.lower_expr_unchecked_ctor(v);
            }
        };
        if args.len() != 3 {
            lower_rest(self, 0);
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
        if values.len() != 3 {
            lower_rest(self, 0);
            return None;
        }
        let a = &values[0];
        let Some((place, ty)) = self.lower_place(a, PlaceUse::IncDec) else {
            if let Some(super::expr::PlaceResult::NotVariable(r)) = self.last_place_result.take() {
                self.error(
                    Code::E(706),
                    "argument 1 must be a variable (parameter `A` is `var`)",
                    r,
                );
            }
            lower_rest(self, 1);
            return None;
        };
        if ty.is_error() {
            lower_rest(self, 1);
            return None;
        }
        let index = |this: &mut Self, e: &ast::Expr| {
            if let Ty::DynArray(_) = ty {
                this.dyn_index(e)
            } else {
                this.array_index(ty, e)
            }
        };
        if !matches!(ty, Ty::DynArray(_)) && self.types.array(ty).is_none() {
            lower_rest(self, 1);
            let r = ast::trimmed_range(a.syntax());
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected an array, found `{}` (argument 1, parameter `A`)",
                    self.tn(ty)
                ),
                r,
            );
            return None;
        }
        let first = index(self, &values[1]);
        let second = index(self, &values[2]);
        Some(StmtKind::Exchange {
            target: place,
            first: Box::new(first?),
            second: Box::new(second?),
            span,
        })
    }

    /// `ArrayMoveRange(var A, FromIndex, ToIndex, Count)` (`docs/spec/bytecode.md` 2.22; the
    /// units of the standard library only): `A` is a dynamic array, the others `Integer`s.
    fn lower_array_move_range(&mut self, args: &[ast::Arg], span: TextRange) -> Option<StmtKind> {
        let (target, [from, to, count]) = self.lower_array_range_args(args, span)?;
        Some(StmtKind::ArrayMoveRange {
            target,
            from: Box::new(from),
            to: Box::new(to),
            count: Box::new(count),
        })
    }

    /// `ArraySortRange(var A, Index, Count)` (`docs/spec/bytecode.md` 2.22; the units of the
    /// standard library only): `A` is a dynamic array, the others `Integer`s.
    fn lower_array_sort_range(&mut self, args: &[ast::Arg], span: TextRange) -> Option<StmtKind> {
        let (target, [index, count]) = self.lower_array_range_args(args, span)?;
        Some(StmtKind::ArraySortRange {
            target,
            index: Box::new(index),
            count: Box::new(count),
        })
    }

    /// The arguments of `ArrayMoveRange` / `ArraySortRange`: the variable of a dynamic array `A`,
    /// then `N` `Integer`s.
    fn lower_array_range_args<const N: usize>(
        &mut self,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<(Place, [Expr; N])> {
        self.check_no_width_pub(args);
        let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        let lower_rest = |this: &mut Self, from: usize| {
            for v in &values[from..] {
                this.lower_expr_unchecked_ctor(v);
            }
        };
        if args.len() != N + 1 {
            lower_rest(self, 0);
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {}, found {}",
                    N + 1,
                    args.len()
                ),
                span,
            );
            return None;
        }
        if values.len() != N + 1 {
            lower_rest(self, 0);
            return None;
        }
        let a = &values[0];
        let Some((place, ty)) = self.lower_place(a, PlaceUse::IncDec) else {
            if let Some(super::expr::PlaceResult::NotVariable(r)) = self.last_place_result.take() {
                self.error(
                    Code::E(706),
                    "argument 1 must be a variable (parameter `A` is `var`)",
                    r,
                );
            }
            lower_rest(self, 1);
            return None;
        };
        if ty.is_error() {
            lower_rest(self, 1);
            return None;
        }
        if !matches!(ty, Ty::DynArray(_)) {
            lower_rest(self, 1);
            let r = ast::trimmed_range(a.syntax());
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected a dynamic array, found `{}` (argument 1, parameter `A`)",
                    self.tn(ty)
                ),
                r,
            );
            return None;
        }
        // Every index is lowered (and reports its errors) before a failure is returned
        let ints: Vec<Option<Expr>> = values[1..].iter().map(|v| self.dyn_index(v)).collect();
        let ints: Vec<Expr> = ints.into_iter().collect::<Option<_>>()?;
        Some((place, ints.try_into().ok()?))
    }

    /// `HashRelink(var Buckets, const Hashes, var Next, Count)` (`docs/spec/bytecode.md` 2.23; the
    /// units of the standard library only): `Buckets` and `Next` are variables of dynamic arrays
    /// of `Integer`, `Hashes` a dynamic array of `Int64`, `Count` an `Integer`.
    fn lower_hash_relink(&mut self, args: &[ast::Arg], span: TextRange) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        if args.len() != 4 || values.len() != 4 {
            for v in &values {
                self.lower_expr_unchecked_ctor(v);
            }
            if args.len() != 4 {
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected 4, found {}",
                        args.len()
                    ),
                    span,
                );
            }
            return None;
        }
        let mismatch = |this: &mut Self, ty: Ty, elem: Ty, i: usize, name: &str, r: TextRange| {
            let (found, elem) = (this.tn(ty), this.tn(elem));
            this.error(
                Code::E(702),
                format!(
                    "type mismatch: expected a dynamic array of `{elem}`, found `{found}` (argument {i}, parameter `{name}`)"
                ),
                r,
            );
        };
        let mut places = Vec::new();
        for (i, name) in [(0, "Buckets"), (2, "Next")] {
            let a = &values[i];
            let Some((place, ty)) = self.lower_place(a, PlaceUse::IncDec) else {
                if let Some(super::expr::PlaceResult::NotVariable(r)) =
                    self.last_place_result.take()
                {
                    self.error(
                        Code::E(706),
                        format!(
                            "argument {} must be a variable (parameter `{name}` is `var`)",
                            i + 1
                        ),
                        r,
                    );
                }
                return None;
            };
            if ty.is_error() {
                return None;
            }
            if !matches!(ty, Ty::DynArray(_)) || self.types.elem_of(ty) != Some(Ty::Integer) {
                mismatch(
                    self,
                    ty,
                    Ty::Integer,
                    i + 1,
                    name,
                    ast::trimmed_range(a.syntax()),
                );
                return None;
            }
            places.push(place);
        }
        let hashes = self.lower_expr(&values[1]);
        if hashes.ty.is_error() {
            return None;
        }
        if !matches!(hashes.ty, Ty::DynArray(_)) || self.types.elem_of(hashes.ty) != Some(Ty::Int64)
        {
            mismatch(self, hashes.ty, Ty::Int64, 2, "Hashes", hashes.span);
            return None;
        }
        let count = self.dyn_index(&values[3])?;
        let next = places.pop().expect("two places");
        let buckets = places.pop().expect("two places");
        Some(StmtKind::HashRelink {
            buckets,
            hashes: Box::new(hashes),
            next,
            count: Box::new(count),
        })
    }

    /// `FreeIfClass(X)` / `CheckDefaultOrder(X)` (`check`) (`docs/spec/collections.md` 12.1,
    /// 10.2; the units of the standard library only): a value of any type, whose type decides what
    /// the statement does once it is known (lower): `X.Free` for a class, `EInvalidOperation`
    /// without a default order.
    fn lower_stdlib_value_proc(
        &mut self,
        args: &[ast::Arg],
        check: bool,
        span: TextRange,
    ) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        if args.len() != 1 {
            for v in &values {
                self.lower_expr_unchecked_ctor(v);
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
        let value = self.lower_value(values.first()?);
        if value.ty.is_error() {
            return None;
        }
        Some(if check {
            StmtKind::CheckDefaultOrder { value }
        } else {
            StmtKind::FreeIfClass { value }
        })
    }

    /// `CheckClassType(X, What)` (`docs/spec/collections.md` 9.3, 12.1; the units of the standard
    /// library only): a value of any type, whose type decides once it is known (lower) whether
    /// the statement raises `EInvalidOperation` with the string `What`.
    fn lower_check_class_type(&mut self, args: &[ast::Arg], span: TextRange) -> Option<StmtKind> {
        self.check_no_width_pub(args);
        let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        if args.len() != 2 || values.len() != 2 {
            for v in &values {
                self.lower_expr_unchecked_ctor(v);
            }
            if args.len() != 2 {
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected 2, found {}",
                        args.len()
                    ),
                    span,
                );
            }
            return None;
        }
        let value = self.lower_value(&values[0]);
        let what = self.lower_expr(&values[1]);
        let what = self.coerce(what, Ty::String, false, Some((2, "What")));
        if value.ty.is_error() || what.ty.is_error() {
            return None;
        }
        Some(StmtKind::CheckClassType {
            value: Box::new(value),
            what: Box::new(what),
        })
    }

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
        // A distinct type of a string as its root (`docs/spec/types.md` 1.3)
        let ty = self.types.root(ty);
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
                index: Box::new(index?),
                count: Box::new(count?),
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
            source: Box::new(source),
            target: place,
            index: Box::new(index?),
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
        ast::Expr::ArrayCtor(_) | ast::Expr::Nil(_) | ast::Expr::Anon(_) => true,
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

/// The source of the subexpressions of an `Assert` condition (`docs/spec/semantics.md` 5.4).
struct AssertSource<'a, 'b, 's> {
    sema: &'a Sema<'b, 's>,
    node: &'a crate::syntax::SyntaxNode,
}

impl super::probes::ProbeSource for AssertSource<'_, '_, '_> {
    fn text(&self, range: TextRange) -> String {
        if range.is_empty() {
            return String::new();
        }
        ast::source_text_in(self.node, range)
    }

    fn var_written(&self, v: VarId, text: &str) -> bool {
        let var = self.sema.var_decl(v);
        match var.kind {
            VarKind::SelfParam | VarKind::RecordSelf => text.eq_ignore_ascii_case("self"),
            _ => !var.name.is_empty(),
        }
    }
}
