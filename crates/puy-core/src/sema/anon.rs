//! Anonymous methods (`docs/spec/objects.md` 9.6, 9.8): their bodies, the variables they
//! capture, and the warning on an anonymous method holding `Self` stored where `Self` reaches.

use super::expr::err_expr;
use super::hir::*;
use super::types::{ProcFamily, Ty};
use super::{
    LSym, ParamInfo, Resolved, RoutineInfo, Sema, has_syntax_error, methods::Member, types,
};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::TextRange;
use crate::syntax::ast::{self, AstNode, ParamMode};

/// The note of E0729 for an anonymous method (`docs/spec/objects.md` 16.2).
const INFER_NOTE: &str =
    "give the anonymous method a type, as in `var P: TProc := procedure begin … end`";

/// The anonymous method an expression is, inside any parentheses.
pub(super) fn anon_of(e: &ast::Expr) -> Option<ast::AnonRoutine> {
    match e {
        ast::Expr::Anon(a) => Some(a.clone()),
        ast::Expr::Paren(p) => p.inner().as_ref().and_then(anon_of),
        _ => None,
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// An anonymous method where no type is expected (E0729; `docs/spec/objects.md` 9.6).
    pub(super) fn anon_without_type(&mut self, span: TextRange) -> Expr {
        self.error(
            Code::E(729),
            "cannot infer the type of an anonymous method",
            span,
        )
        .notes
        .push(INFER_NOTE.into());
        err_expr(span)
    }

    /// An anonymous method as a value of the expected type (`docs/spec/objects.md` 9.6): an
    /// anonymous method type whose parameters and return type match.
    pub(super) fn lower_anon(&mut self, a: &ast::AnonRoutine, expected: Ty) -> Expr {
        let span = ast::trimmed_range(a.syntax());
        let at = a.keyword().map_or(span, |k| k.text_range());
        match self.types.proc_family(expected) {
            Some(ProcFamily::Reference) => {}
            Some(_) => {
                self.error(
                    Code::E(752),
                    "an anonymous method can only be assigned to a `reference to` type",
                    at,
                );
                return err_expr(span);
            }
            None if expected.is_error() => return err_expr(span),
            None => return self.anon_without_type(span),
        }
        if self.cur.is_none() {
            // The initial value of a global variable (a constant expression)
            self.error(Code::E(713), "constant expression required", span);
            return err_expr(span);
        }
        let (params, ret) = self.signature_of(a.params(), a.is_function(), a.return_type());
        let sig: Vec<(Ty, ParamMode)> = params.iter().map(|p| (p.ty, p.mode)).collect();
        if sig.iter().any(|(t, _)| t.is_error()) || ret.is_some_and(|r| r.is_error()) {
            return err_expr(span);
        }
        if !self.types.has_signature(expected, &sig, ret) {
            self.value_mismatch(expected, ProcFamily::Reference, &sig, ret, span);
            return err_expr(span);
        }
        let Some((func, captures)) = self.anon_body(a, params, ret, at) else {
            return err_expr(span);
        };
        Expr {
            kind: ExprKind::Anon { func, captures },
            ty: expected,
            value: None,
            span,
        }
    }

    /// Analyzes the body of an anonymous method as a routine nested in the one being analyzed
    /// whose function takes the closure (`docs/spec/objects.md` 9.6, 14.9): its id, and the
    /// variables of the routine being analyzed whose cells it captures.
    fn anon_body(
        &mut self,
        a: &ast::AnonRoutine,
        params: Vec<ParamInfo>,
        ret: Option<Ty>,
        at: TextRange,
    ) -> Option<(RoutineId, Vec<VarId>)> {
        let body = a.body()?;
        let (n, parent, outer_name) = {
            let c = self.cur.as_mut()?;
            c.anon_count += 1;
            (c.anon_count, c.routine, c.qual_name.clone())
        };
        let short = format!("<anon{n}>");
        // Its name is that of the routine it is written in with `<anonN>` added (9.6)
        let name = match parent {
            Some(_) => short.clone(),
            None => format!("{outer_name}.{short}"),
        };
        let id = RoutineId(self.routines.len() as u32);
        if parent.is_none() {
            self.cur_mut().top_anons.push(id);
        }
        self.routines.push(RoutineInfo {
            name: name.clone(),
            params: params.clone(),
            ret,
            has_body: true,
            forward_span: None,
            implicit_forward: false,
            name_span: at,
            hir: None,
            overload: false,
            overload_fix_at: None,
            unit: self.unit_index,
            file: self.input.file,
            interface: false,
            parent,
            calls: Vec::new(),
            method_of: None,
            self_kind: types::SelfKind::Instance,
            is_abstract: false,
            body_span: None,
            closure: true,
            type_params: Vec::new(),
            closure_info: super::ClosureInfo {
                anon: true,
                ..Default::default()
            },
        });
        let syntax_errors = has_syntax_error(a.syntax(), self.sink, self.input.file);
        let end_span = body
            .block()
            .and_then(|b| b.end_token())
            .map(|t| t.text_range())
            .unwrap_or(a.syntax().text_range());
        let enclosing = self
            .cur
            .take()
            .expect("an anonymous method is in a routine");
        let qual_name = format!("{}.{short}", enclosing.qual_name);
        self.outer.push(enclosing);
        self.begin_routine(Some(id), ret, short, end_span, false, syntax_errors);
        {
            let begin = body
                .block()
                .and_then(|b| b.begin_token())
                .map(|t| t.text_range().end());
            let ctx = self.cur_mut();
            ctx.name_span = at;
            ctx.body_begin = begin;
            ctx.qual_name = qual_name;
            ctx.closure = true;
        }
        for p in &params {
            let lower = p.name.to_ascii_lowercase();
            let duplicate = self.check_duplicate(&lower, p.span);
            let vid = self.add_var(p.name.clone(), p.ty, VarKind::Param(p.mode), p.span);
            if !duplicate {
                self.cur_mut().scopes[0].push((lower, LSym::Var(vid)));
            }
        }
        if let Some(rt) = ret {
            self.add_var("Result".into(), rt, VarKind::Result, at);
        }
        let mut stmts: Vec<Stmt> = Vec::new();
        for node in body.syntax().children() {
            match ast::Item::cast(node) {
                Some(ast::Item::Const(c)) => self.declare_local_consts(c.decls(), false),
                Some(ast::Item::ResString(c)) => self.declare_local_consts(c.decls(), true),
                Some(ast::Item::Var(v)) => self.declare_local_vars(&v, &mut stmts),
                Some(ast::Item::Type(t)) => self.declare_types(&t),
                Some(ast::Item::Routine(r)) => self.declare_routine(&r, false),
                _ => {}
            }
        }
        if let Some(block) = body.block() {
            let inner = self.lower_block_stmts(&block);
            stmts.extend(inner);
        }
        let calls = std::mem::take(&mut self.cur_mut().calls);
        let srcs: Vec<VarId> = self.cur_mut().captures.iter().map(|(_, s)| *s).collect();
        let self_at = self.cur_mut().self_captured_at;
        let routine = self.end_routine(stmts, end_span);
        let failed = routine.has_errors;
        self.cur = self.outer.pop();
        if failed {
            self.cur_mut().has_errors = true;
        }
        let info = &mut self.routines[id.0 as usize];
        info.hir = Some(routine);
        info.calls = calls;
        if let Some(s) = self_at {
            self.anon_self.insert(id, s);
        }
        Some((id, srcs))
    }

    /// W0024 (`docs/spec/objects.md` 9.8): in a method of a reference counted class, an anonymous
    /// method that captured `Self` assigned to a place reached from `Self` (a member of `Self`
    /// written with or without `Self.`).
    pub(super) fn check_self_cycle(&mut self, target: &ast::Expr, value: &Expr) {
        if self.dry_run > 0 {
            return;
        }
        let (ExprKind::Anon { func, .. } | ExprKind::NestedRef { func, .. }) = &value.kind else {
            return;
        };
        let Some(captured) = self.self_captured(*func) else {
            return;
        };
        let Some(mut class) = self.method_class() else {
            return;
        };
        // A class helper's `Self` is an instance of its class (`docs/spec/objects.md` 10.8)
        if let Some(Ty::Class(t)) = self.types.helper_target(class) {
            class = t;
        }
        if self.method_self_kind() != Some(types::SelfKind::Instance)
            || self.types.is_record(class)
            || self.types.slot_kind(Ty::Class(class)) != crate::bytecode::SLOT_REF
        {
            return;
        }
        let mut root = target.clone();
        loop {
            root = match &root {
                ast::Expr::Field(f) => match f.base() {
                    Some(b) => b,
                    None => return,
                },
                ast::Expr::Index(i) => match i.base() {
                    Some(b) => b,
                    None => return,
                },
                ast::Expr::Call(c) => match c.callee() {
                    Some(b) => b,
                    None => return,
                },
                ast::Expr::Paren(p) => match p.inner() {
                    Some(b) => b,
                    None => return,
                },
                _ => break,
            };
        }
        let ast::Expr::NameRef(n) = &root else { return };
        let Some(tok) = n.token() else { return };
        let name = tok.text().to_string();
        let range = tok.text_range();
        let from_self = name.eq_ignore_ascii_case("self")
            || matches!(
                self.dry_run(|s| Some(s.resolve_ident(&name, range))),
                Some(Resolved::Member {
                    member: Member::Field { .. } | Member::Property(..),
                    ..
                })
            );
        if !from_self {
            return;
        }
        let d = Diagnostic::warning(
            Code::W(24),
            "anonymous method captures `Self` and is stored in a field reachable from `Self`",
            Stage::Sema,
        )
        .with_primary(self.span(value.span), "")
        .with_secondary(self.span(captured), "`Self` captured here")
        .with_secondary(self.span(ast::trimmed_range(target.syntax())), "stored here")
        .with_note(
            "move the body to a method and store a reference to that method (a method reference does not count `Self`)",
        )
        .with_note(
            "or copy `Self` to a `[weak]` local variable and use that in the anonymous method",
        );
        self.push(d);
    }

    /// Where the closure of `func` (an anonymous method, or a closure-form nested routine) holds
    /// `Self` (W0024): where it first captured `Self`, or, for a nested routine, where a routine
    /// whose block its value holds did (one of its group, or of a group they call; a call from
    /// an anonymous method or a routine nested in a routine counts as that routine's, as for the
    /// groups, `docs/spec/objects.md` 9.13).
    fn self_captured(&self, func: RoutineId) -> Option<TextRange> {
        let mut seen = vec![func];
        let mut k = 0;
        while k < seen.len() {
            let r = seen[k];
            if let Some(&at) = self.anon_self.get(&r) {
                return Some(at);
            }
            let info = &self.routines[r.0 as usize];
            let group = info.closure_info.group;
            let mut next: Vec<RoutineId> = (0..self.routines.len())
                .map(|i| RoutineId(i as u32))
                .filter(|x| self.encloses(r, *x))
                .flat_map(|x| self.routines[x.0 as usize].calls.iter().copied())
                .filter(|c| self.routines[c.0 as usize].closure_info.group.is_some())
                .collect();
            if group.is_some() {
                next.extend(
                    (0..self.routines.len())
                        .map(|i| RoutineId(i as u32))
                        .filter(|m| self.routines[m.0 as usize].closure_info.group == group),
                );
            }
            for m in next {
                if !seen.contains(&m) {
                    seen.push(m);
                }
            }
            k += 1;
        }
        None
    }

    /// Whether `x` is `r` or a routine nested in it (an anonymous method included).
    fn encloses(&self, r: RoutineId, x: RoutineId) -> bool {
        let mut at = Some(x);
        while let Some(y) = at {
            if y == r {
                return true;
            }
            at = self.routines[y.0 as usize].parent;
        }
        false
    }
}
