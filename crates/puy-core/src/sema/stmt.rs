//! Typing of statements (`docs/spec/semantics.md` chapters 4–6).

use super::builtins::{self, BuiltinProc};
use super::expr::PlaceUse;
use super::hir::*;
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
                let (value, ty) = self.eval_const_decl(c.type_ref(), c.value());
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
        // Type the right-hand side first (the target variable is read after the right-hand side)
        let value = value_ast.as_ref().map(|v| self.lower_expr(v));
        let place = self.lower_place(&target, PlaceUse::Assign);
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
        let init = v.initializer().map(|e| self.lower_expr(&e));
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
        let init = init?;
        let checks = self.checks_at(span.start());
        let value = if ty.is_error() || init.ty.is_error() {
            init
        } else {
            self.coerce(init, ty, checks.range, None)
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
        let fv = f.var()?;
        let checks = self.checks_at(span.start());
        let from_ast = f.from();
        let to_ast = f.to();
        let from = from_ast.as_ref().map(|e| self.lower_expr(e));
        let var: ForVar;
        let var_ty: Ty;
        self.cur_mut().scopes.push(Vec::new());
        if fv.is_inline() {
            let name_tok = fv.name()?;
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let declared = fv.type_ref().map(|t| self.resolve_type(Some(t)));
            let ty = match (declared, &from) {
                (Some(t), _) => t,
                (None, Some(e)) => e.ty,
                (None, None) => Ty::Error,
            };
            if !ty.is_error() && !ty.is_ordinal() {
                self.error(
                    Code::E(722),
                    "loop variable must be a local variable of an ordinal type",
                    range,
                );
            }
            if self.check_duplicate(&lower, range) {
                self.cur_mut().scopes.pop();
                return None;
            }
            let id = self.add_var(name, ty, VarKind::Local, range);
            self.cur_mut().vars[id.0 as usize].decl = Some(fv.syntax().clone());
            self.cur_mut().vars[id.0 as usize].is_for_var = true;
            self.cur_mut()
                .scopes
                .last_mut()
                .unwrap()
                .push((lower, LSym::Var(id)));
            var = ForVar::Local(id);
            var_ty = ty;
        } else {
            let name_tok = fv.name()?;
            let name = self.text(&name_tok).to_string();
            let range = name_tok.text_range();
            let lower = name.to_ascii_lowercase();
            // Fix inserting `var` if the variable is not declared
            let resolved_local = self.lookup_local(&lower);
            let is_main = self.cur.as_ref().is_some_and(|c| c.is_main);
            match resolved_local {
                Some(LSym::Var(id)) => {
                    let v = self.var(id).clone();
                    // A value parameter can be a loop variable, like a local variable (semantics.md 4.5).
                    // A `const` parameter cannot be modified, so E0707; a `var` / `out` parameter gives E0722
                    if matches!(v.kind, VarKind::Param(ast::ParamMode::Const)) {
                        self.cannot_assign(range, "a `const` parameter", Some(v.span));
                        self.cur_mut().scopes.pop();
                        return None;
                    }
                    if matches!(
                        v.kind,
                        VarKind::Param(ast::ParamMode::Var | ast::ParamMode::Out)
                    ) {
                        let d = Diagnostic::error(
                            Code::E(722),
                            "loop variable must be a local variable of an ordinal type",
                            Stage::Sema,
                        )
                        .with_primary(self.span(range), "")
                        .with_secondary(self.span(v.span), "declared here")
                        .with_note("a `var` or `out` parameter refers to the caller's variable; copy it to a local variable first");
                        self.push(d);
                        self.cur_mut().scopes.pop();
                        return None;
                    }
                    if !v.ty.is_error() && !v.ty.is_ordinal() {
                        self.error(
                            Code::E(722),
                            "loop variable must be a local variable of an ordinal type",
                            range,
                        );
                        self.cur_mut().scopes.pop();
                        return None;
                    }
                    if self.cur.as_ref().is_some_and(|c| c.loop_vars.contains(&id)) {
                        self.cannot_assign_loop_var(range, v.span);
                        self.cur_mut().scopes.pop();
                        return None;
                    }
                    self.cur_mut().vars[id.0 as usize].is_loop_var = true;
                    var = ForVar::Local(id);
                    var_ty = v.ty;
                }
                Some(LSym::Const { span: decl, .. }) => {
                    let text = self.src(range);
                    let d = Diagnostic::error(
                        Code::E(707),
                        format!("cannot assign to `{text}`"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "a constant")
                    .with_secondary(self.span(decl), "declared here");
                    self.push(d);
                    self.cur_mut().scopes.pop();
                    return None;
                }
                Some(LSym::Unsupported { .. }) => {
                    // Name of a nested routine (the parser has reported E3200). Do not pile on diagnostics
                    self.cur_mut().scopes.pop();
                    return None;
                }
                None => {
                    // A global variable, or undeclared
                    let global = self
                        .global_scope
                        .iter()
                        .rev()
                        .find(|(n, _)| *n == lower)
                        .map(|(_, s)| s.clone());
                    match global {
                        Some(super::GSym::Global(gid)) => {
                            let g = self.globals[gid.0 as usize].clone();
                            if !is_main {
                                let d = Diagnostic::error(
                                    Code::E(722),
                                    "loop variable must be a local variable of an ordinal type",
                                    Stage::Sema,
                                )
                                .with_primary(self.span(range), "")
                                .with_secondary(self.span(g.span), "declared here");
                                self.push(d);
                                self.cur_mut().scopes.pop();
                                return None;
                            }
                            if !g.ty.is_error() && !g.ty.is_ordinal() {
                                self.error(
                                    Code::E(722),
                                    "loop variable must be a local variable of an ordinal type",
                                    range,
                                );
                                self.cur_mut().scopes.pop();
                                return None;
                            }
                            if self
                                .cur
                                .as_ref()
                                .is_some_and(|c| c.loop_globals.contains(&gid))
                            {
                                self.cannot_assign_loop_var(range, g.span);
                                self.cur_mut().scopes.pop();
                                return None;
                            }
                            let counter =
                                self.add_var(String::new(), Ty::Int64, VarKind::Hidden, range);
                            var = ForVar::Global { id: gid, counter };
                            var_ty = g.ty;
                        }
                        Some(super::GSym::Const { span: decl, .. }) => {
                            let text = self.src(range);
                            let d = Diagnostic::error(
                                Code::E(707),
                                format!("cannot assign to `{text}`"),
                                Stage::Sema,
                            )
                            .with_primary(self.span(range), "a constant")
                            .with_secondary(self.span(decl), "declared here");
                            self.push(d);
                            self.cur_mut().scopes.pop();
                            return None;
                        }
                        Some(super::GSym::Routine(_)) => {
                            let text = self.src(range);
                            self.cannot_assign(range, "not a variable", None);
                            let _ = text;
                            self.cur_mut().scopes.pop();
                            return None;
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
                            self.cur_mut().scopes.pop();
                            // Keep type-checking the body
                            if let Some(t) = &to_ast {
                                self.lower_expr(t);
                            }
                            self.lower_loop_body(f.body());
                            return None;
                        }
                    }
                }
            }
        }
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
        // Body (assigning to the loop variable is E0707)
        match &var {
            ForVar::Local(id) => self.cur_mut().loop_vars.push(*id),
            ForVar::Global { id, .. } => self.cur_mut().loop_globals.push(*id),
        }
        let body = self.lower_loop_body(f.body());
        match &var {
            ForVar::Local(_) => {
                self.cur_mut().loop_vars.pop();
            }
            ForVar::Global { .. } => {
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
        })
    }

    fn cannot_assign_loop_var(&mut self, range: TextRange, decl: TextRange) {
        let text = self.src(range);
        let d = Diagnostic::error(
            Code::E(707),
            format!("cannot assign to `{text}`"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "the loop variable")
        .with_secondary(self.span(decl), "declared here");
        self.push(d);
    }

    /// Looks up a name in the current routine's scope (globals are not consulted).
    fn lookup_local(&self, lower: &str) -> Option<LSym> {
        let c = self.cur.as_ref()?;
        for scope in c.scopes.iter().rev() {
            for (n, sym) in scope.iter().rev() {
                if n == lower {
                    return Some(sym.clone());
                }
            }
        }
        None
    }

    // ---- Expression statements ----

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
                if !matches!(expr, ast::Expr::Call(_)) && !self.check_bare_routine_stmt(id, range) {
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
                        format!("`{}` is a type and cannot be used as a value", t.name()),
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
        let is_self = self.cur.as_ref().is_some_and(|c| c.routine == Some(id));
        let n = info.name.clone();
        let name_span = info.name_span;
        let has_params = !info.params.is_empty();
        if is_self && info.ret.is_some() {
            let mut d = Diagnostic::error(
                Code::E(704),
                format!("recursive call to `{n}` needs parentheses"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.span(name_span), "declared here");
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
            .with_secondary(self.span(name_span), "declared here");
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
        let e = self.lower_builtin_call_pub(f, name, args, range, span);
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
                let newline = p == BuiltinProc::WriteLn;
                let mut out: Vec<WriteArg> = Vec::new();
                let mut failed = false;
                for a in args {
                    let exprs = a.exprs();
                    let Some(v) = exprs.first() else {
                        failed = true;
                        continue;
                    };
                    let value = self.lower_expr(v);
                    if value.ty.is_error() {
                        failed = true;
                        continue;
                    }
                    if !(value.ty.is_integer()
                        || matches!(value.ty, Ty::Boolean | Ty::Char | Ty::String))
                    {
                        let r = value.span;
                        self.error(
                            Code::E(718),
                            format!("cannot write a value of type `{}`", value.ty),
                            r,
                        );
                        failed = true;
                        continue;
                    }
                    let mut width = None;
                    if let Some(w) = exprs.get(1) {
                        let we = self.lower_expr(w);
                        if we.ty.is_error() {
                            failed = true;
                        } else if !we.ty.is_integer() {
                            let r = we.span;
                            self.error(
                                Code::E(702),
                                format!("type mismatch: expected `Integer`, found `{}`", we.ty),
                                r,
                            );
                            failed = true;
                        } else {
                            width = Some(self.coerce(we, Ty::Int64, checks.range, None));
                        }
                    }
                    if a.colon_count() >= 2 {
                        if let Some(d) = exprs.get(2) {
                            self.lower_expr(d);
                        }
                        let r = a.syntax().text_range();
                        self.error(
                            Code::E(717),
                            "precision specifier requires a floating-point value",
                            r,
                        );
                        failed = true;
                    }
                    out.push(WriteArg { value, width });
                }
                if failed {
                    return None;
                }
                if out.is_empty() && !newline {
                    return Some(StmtKind::Nop);
                }
                Some(StmtKind::Write { args: out, newline })
            }
            BuiltinProc::ReadLn => {
                self.check_no_width_pub(args);
                if args.len() > 1 {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(3200),
                        "`ReadLn` with several arguments is not supported yet (planned for M2)",
                        span,
                    );
                    return None;
                }
                let Some(a) = args.first() else {
                    return Some(StmtKind::ReadLn(None));
                };
                let v = a.value()?;
                let (place, ty) = match self.lower_place(&v, PlaceUse::ReadLn) {
                    Some(x) => x,
                    None => {
                        if let Some(super::expr::PlaceResult::NotVariable(r)) =
                            self.last_place_result.take()
                        {
                            self.error(
                                Code::E(706),
                                "argument 1 must be a variable (parameter `X` is `out`)",
                                r,
                            );
                        }
                        return None;
                    }
                };
                if ty.is_error() {
                    return None;
                }
                if ty == Ty::Char || ty == Ty::Boolean {
                    self.error(
                        Code::E(3200),
                        format!("`ReadLn` into a `{ty}` is not supported yet (planned for M2)"),
                        span,
                    );
                    return None;
                }
                if !(ty == Ty::String || ty.is_integer()) {
                    self.error(
                        Code::E(702),
                        format!("type mismatch: expected `string`, found `{ty}`"),
                        span,
                    );
                    return None;
                }
                Some(StmtKind::ReadLn(Some(place)))
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
                if !(ty.is_integer() || ty == Ty::Char) {
                    let r = ast::trimmed_range(v.syntax());
                    self.error(Code::E(702), format!("type mismatch: expected an integer type or `Char`, found `{ty}` (argument 1, parameter `X`)"), r);
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
                            self.error(Code::E(702), format!("type mismatch: expected an integer type, found `{}` (argument 2, parameter `N`)", e.ty), r);
                            return None;
                        }
                        Some(e)
                    }
                    None => None,
                };
                // Computation type: same as X := X + N (`Char` is computed as `Word` and narrowed back to `Char`)
                let target_ty = if ty == Ty::Char { Ty::Word } else { ty };
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
                    None => None,
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
                let value = match args.first().and_then(|a| a.value()) {
                    Some(v) => {
                        let e = self.lower_expr(&v);
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
                Some(if p == BuiltinProc::Break {
                    StmtKind::Break
                } else {
                    StmtKind::Continue
                })
            }
        }
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

#[allow(dead_code)]
fn _unused(_: ParamMode) {}
