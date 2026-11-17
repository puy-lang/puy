//! Procedural types and method references (`docs/spec/objects.md` 9.1–9.5): routines and
//! methods as values, calls of procedural values, the calls of values written without
//! parentheses, and the comparisons that do not call.

use super::expr::err_expr;
use super::hir::*;
use super::methods::Member;
use super::types::{MethodKind, ProcFamily, SelfKind, Ty, TypeId};
use super::{Resolved, Sema};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::TextRange;
use crate::syntax::ast::{self, ParamMode};

/// The note of E0752: how to make a value of what cannot be one.
const WRAP_NOTE: &str = "wrap it in an anonymous method, as in `procedure(…) begin …(…) end`";

/// Whether a name resolves to something that runs code when written without parentheses (a
/// routine, a method, a built-in routine, a constructor): in a value position it is made a value
/// (or reported), not read.
fn names_code(r: &Resolved) -> bool {
    match r {
        Resolved::Routine(_)
        | Resolved::Func(_)
        | Resolved::Key(..)
        | Resolved::Proc(_)
        | Resolved::ClassCreate(..)
        | Resolved::RaiseOuter => true,
        Resolved::Member { member, .. } => {
            matches!(
                member,
                Member::Methods { .. }
                    | Member::Builtin(_)
                    | Member::HelperBuiltin(super::types::BuiltinHelper::ToString)
            )
        }
        _ => false,
    }
}

/// Whether an expression is written as a name, a member or an element: a procedural value
/// written so is called outside a value position (`docs/spec/objects.md` 9.4).
fn bare_syntax(e: &ast::Expr) -> bool {
    matches!(
        e,
        ast::Expr::NameRef(_) | ast::Expr::Field(_) | ast::Expr::Index(_)
    )
}

impl<'a, 's> Sema<'a, 's> {
    /// Whether the expression is a name of code (see [`names_code`]), without reporting anything.
    pub(super) fn is_code_name(&mut self, e: &ast::Expr) -> bool {
        matches!(
            e,
            ast::Expr::NameRef(_) | ast::Expr::Field(_) | ast::Expr::Inherited(_)
        ) && self
            .dry_run(|s| s.resolve_name_expr(e))
            .is_some_and(|(r, _, _)| names_code(&r))
    }

    /// Types an expression as `lower_expr` does, but a procedural value written without
    /// parentheses stays a value (`docs/spec/objects.md` 9.4: a value position).
    pub(super) fn lower_value(&mut self, e: &ast::Expr) -> Expr {
        self.lower_value_declared(e).0
    }

    /// [`Self::lower_value`] and the type the expression has before a subrange type is taken to
    /// its base (for the subrange type's own helpers; `docs/spec/objects.md` 10.4).
    pub(super) fn lower_value_declared(&mut self, e: &ast::Expr) -> (Expr, Ty) {
        // `(S)` keeps the subrange type of `S` for its helpers
        if let ast::Expr::Paren(p) = e
            && let Some(inner) = p.inner()
            && !matches!(inner, ast::Expr::ArrayCtor(_) | ast::Expr::Nil(_))
        {
            let (mut x, declared) = self.lower_expr_declared(&inner);
            x.span = ast::trimmed_range(e.syntax());
            return (x, declared);
        }
        let mut x = self.lower_expr_raw(e);
        let declared = x.ty;
        if let Ty::Subrange(_) = x.ty {
            x.ty = self.types.base(x.ty);
        }
        (x, declared)
    }

    /// A procedural value written without parentheses outside a value position is called
    /// (`docs/spec/objects.md` 9.4): `X := F;`, `WriteLn(F)`. A function's result (`MakeCounter`)
    /// is not called again.
    pub(super) fn call_bare_value(&mut self, e: &ast::Expr, x: Expr) -> Expr {
        if !x.ty.is_procedural() || !bare_syntax(e) {
            return x;
        }
        if matches!(x.kind, ExprKind::Call { .. })
            && !matches!(
                self.dry_run(|s| s.resolve_name_expr(e)),
                Some((
                    Resolved::Member {
                        member: Member::Property(..),
                        ..
                    },
                    _,
                    _
                ))
            )
        {
            return x;
        }
        let span = x.span;
        let Some((_, params, ret)) = self.types.proc_sig(x.ty) else {
            return x;
        };
        if !params.is_empty() {
            let n = params.len();
            self.error(
                Code::E(705),
                format!("wrong number of arguments: expected {n}, found 0"),
                span,
            );
            return err_expr(span);
        }
        if ret.is_none() {
            let text = self.src(span);
            self.error(
                Code::E(725),
                format!("procedure `{text}` has no value"),
                span,
            );
            return err_expr(span);
        }
        self.call_value(x, &[], span)
    }

    /// When `callee` (the callee of a call, or a statement without parentheses) is a value of a
    /// procedural type rather than the name of a routine or method: the value
    /// (`docs/spec/objects.md` 9.4).
    pub(super) fn procedural_callee(&mut self, callee: &ast::Expr) -> Option<Expr> {
        if !matches!(
            callee,
            ast::Expr::NameRef(_)
                | ast::Expr::Field(_)
                | ast::Expr::Index(_)
                | ast::Expr::Call(_)
                | ast::Expr::Paren(_)
        ) || self.is_code_name(callee)
        {
            return None;
        }
        let ty = self.dry_run(|s| s.lower_value(callee).ty);
        if !ty.is_procedural() {
            return None;
        }
        Some(self.lower_value(callee))
    }

    /// A call of a procedural value (`docs/spec/objects.md` 9.4): the arguments are checked
    /// against the type's parameters (no default values).
    pub(super) fn call_value(&mut self, callee: Expr, args: &[ast::Arg], span: TextRange) -> Expr {
        self.check_no_width_pub(args);
        let Some((_, params, ret)) = self.types.proc_sig(callee.ty) else {
            return err_expr(span);
        };
        let params: Vec<(String, Ty, ParamMode)> = params
            .iter()
            .map(|p| (p.name.clone(), p.ty, p.mode))
            .collect();
        if args.len() != params.len() {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {}, found {}",
                    params.len(),
                    args.len()
                ),
                span,
            );
            return err_expr(span);
        }
        let checks = self.checks_at(span.start());
        let mut out = Vec::new();
        let mut failed = false;
        for (i, (a, (name, ty, mode))) in args.iter().zip(&params).enumerate() {
            let Some(v) = a.value() else {
                failed = true;
                continue;
            };
            match mode {
                ParamMode::Var | ParamMode::Out => {
                    let is_out = *mode == ParamMode::Out;
                    match self.ref_arg(&v, (i + 1, name), *ty, is_out, None) {
                        Some(place) => out.push(CallArg::Ref { place, is_out }),
                        None => failed = true,
                    }
                }
                _ => {
                    let e = self.lower_expected(&v, *ty);
                    if e.ty.is_error() {
                        failed = true;
                        continue;
                    }
                    let e = self.coerce(e, *ty, checks.range, Some((i + 1, name)));
                    if e.ty.is_error() {
                        failed = true;
                    }
                    out.push(CallArg::Value(e));
                }
            }
        }
        if failed || callee.ty.is_error() {
            return err_expr(span);
        }
        Expr {
            kind: ExprKind::CallValue {
                callee: Box::new(callee),
                args: out,
            },
            ty: ret.unwrap_or(Ty::Error),
            value: None,
            span,
        }
    }

    /// An expression in a value position whose expected type is procedural
    /// (`docs/spec/objects.md` 9.4): the name of a routine or a method is made a value of type
    /// `expected` (9.2, 9.3), and any other expression is typed without calling it. The caller
    /// converts the result (`coerce`).
    pub(super) fn lower_proc_expected(&mut self, e: &ast::Expr, expected: Ty) -> Expr {
        let span = ast::trimmed_range(e.syntax());
        if let Some(a) = super::anon::anon_of(e) {
            let mut x = self.lower_anon(&a, expected);
            x.span = span;
            return x;
        }
        if !self.is_code_name(e) {
            return self.lower_value(e);
        }
        let Some((r, name, range)) = self.resolve_name_expr(e) else {
            return err_expr(span);
        };
        match r {
            Resolved::Routine(id) => self.routine_value(id, expected, range, span),
            Resolved::Member {
                recv,
                class,
                member: Member::Methods { name: n, inst },
                inherited,
            } => self.method_value(*recv, class, &n, inst, inherited, expected, range, span),
            Resolved::Member {
                member: Member::Builtin(_) | Member::HelperBuiltin(_),
                ..
            }
            | Resolved::Func(_)
            | Resolved::Key(..)
            | Resolved::Proc(_)
            | Resolved::RaiseOuter => {
                // Built-in routines and members, whose bodies are instructions (9.2)
                self.not_a_value(&name, expected, range);
                err_expr(span)
            }
            Resolved::ClassCreate(..) => {
                let text = self.src(span);
                self.not_a_value(&text, expected, range);
                err_expr(span)
            }
            _ => self.lower_value(e),
        }
    }

    /// E0752 `` `…` cannot be used as a procedural value `` (`docs/spec/objects.md` 9.2, 9.3),
    /// with the note on wrapping it in an anonymous method when the expected type is an
    /// anonymous method type.
    fn not_a_value(&mut self, name: &str, expected: Ty, range: TextRange) {
        let wrap = self.types.proc_family(expected) == Some(ProcFamily::Reference);
        let d = self.error(
            Code::E(752),
            format!("`{name}` cannot be used as a procedural value"),
            range,
        );
        if wrap {
            d.notes.push(WRAP_NOTE.into());
        }
    }

    /// E0702 for a routine or method whose parameters or family do not fit the expected
    /// procedural type: the routine's type is written as the procedural type it would have.
    pub(super) fn value_mismatch(
        &mut self,
        expected: Ty,
        family: ProcFamily,
        params: &[(Ty, ParamMode)],
        ret: Option<Ty>,
        span: TextRange,
    ) {
        let list: Vec<(Option<&str>, Ty, ParamMode)> =
            params.iter().map(|(t, m)| (None, *t, *m)).collect();
        let found = self.types.signature_text(family, &list, ret, false);
        let msg = format!(
            "type mismatch: expected `{}`, found `{found}`",
            self.tn(expected)
        );
        self.error(Code::E(702), msg, span);
    }

    /// A value of the expected procedural type made from a routine (`docs/spec/objects.md` 9.2):
    /// the overload whose parameters and return type match.
    fn routine_value(
        &mut self,
        id: RoutineId,
        expected: Ty,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let Some(family) = self.types.proc_family(expected) else {
            return err_expr(span);
        };
        let nested = self.routines[id.0 as usize].parent.is_some();
        if nested && family != ProcFamily::Reference {
            // A nested routine (9.7)
            self.error(
                Code::E(752),
                "a nested routine can only be assigned to a `reference to` type",
                range,
            );
            return err_expr(span);
        }
        let sig = |s: &Self, c: RoutineId| {
            let r = &s.routines[c.0 as usize];
            let params: Vec<(Ty, ParamMode)> = r.params.iter().map(|p| (p.ty, p.mode)).collect();
            (params, r.ret)
        };
        let (set, given) = match self.explicit_set(id, range) {
            Some(set) => (set, true),
            None => (self.routine_overloads(id), false),
        };
        // A generic routine is a value only with its type arguments written
        // (`docs/spec/generics.md` 5.1)
        let picked = set.iter().copied().find(|c| {
            let (p, r) = sig(self, *c);
            (given || self.routines[c.0 as usize].type_params.is_empty())
                && self.types.has_signature(expected, &p, r)
        });
        if picked.is_none() && !given && self.generic_value_without_args(&set, range) {
            return err_expr(span);
        }
        let Some(c) = picked.filter(|_| family != ProcFamily::Method) else {
            let (p, r) = sig(self, id);
            self.value_mismatch(expected, ProcFamily::Plain, &p, r, span);
            return err_expr(span);
        };
        self.routine_hint_use(c, range);
        if nested {
            return self.nested_value(c, expected, span);
        }
        self.plain_value(c, expected, family, span)
    }

    /// A nested routine as a value of the anonymous method type `expected`
    /// (`docs/spec/objects.md` 9.7): it is compiled as a closure, and the value is made from its
    /// group's block (14.9).
    fn nested_value(&mut self, routine: RoutineId, expected: Ty, span: TextRange) -> Expr {
        if self.dry_run == 0 {
            self.routines[routine.0 as usize].closure_info.taken = true;
        }
        let block = self.group_block(routine);
        Expr {
            kind: ExprKind::NestedRef {
                func: routine,
                block,
            },
            ty: expected,
            value: None,
            span,
        }
    }

    /// `RoutineRef` of the function of `routine`, as a value of `expected` (a procedural type, or
    /// an anonymous method type through `ToClosure`).
    fn plain_value(
        &mut self,
        routine: RoutineId,
        expected: Ty,
        family: ProcFamily,
        span: TextRange,
    ) -> Expr {
        let x = Expr {
            kind: ExprKind::RoutineRef(routine),
            ty: expected,
            value: None,
            span,
        };
        if family != ProcFamily::Reference {
            return x;
        }
        Expr {
            kind: ExprKind::ToClosure(Box::new(x)),
            ty: expected,
            value: None,
            span,
        }
    }

    /// A method reference of the expected type (`docs/spec/objects.md` 9.3): `O.M`, `M` and
    /// `Self.M` in a method, `TFoo.M` / `C.M` of a class method. A static method is a routine
    /// value (9.2).
    #[allow(clippy::too_many_arguments)]
    fn method_value(
        &mut self,
        recv: Expr,
        class: TypeId,
        name: &str,
        inst: Option<Vec<RoutineId>>,
        inherited: bool,
        expected: Ty,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let Some(family) = self.types.proc_family(expected) else {
            return err_expr(span);
        };
        if recv.ty.is_error() && !Self::no_instance(&recv) {
            return err_expr(span);
        }
        if inherited {
            self.not_a_value(&format!("inherited {name}"), expected, range);
            return err_expr(span);
        }
        if self.types.self_by_ref(class) || self.types.is_interface(class) {
            // `Self` of a record method (and of a record helper's; 10.8) is a `var` parameter
            // (chapter 6); an interface's methods are reached through the interface's value
            // (7.5)
            self.not_a_value(name, expected, range);
            return err_expr(span);
        }
        let explicit = inst.is_some();
        let set = match inst {
            Some(s) => s,
            None => self.method_set(class, name),
        };
        if !explicit
            && !set.iter().any(|c| {
                self.routines[c.0 as usize].type_params.is_empty()
                    && self
                        .method_of_routine(*c)
                        .is_some_and(|(_, m)| self.types.has_signature(expected, &m.params, m.ret))
            })
            && self.generic_value_without_args(&set, range)
        {
            return err_expr(span);
        }
        if set.is_empty() {
            return self.builtin_method_value(recv, class, name, family, expected, range, span);
        }
        let methods: Vec<(RoutineId, super::types::ClassMethod)> = set
            .iter()
            .filter_map(|c| self.method_of_routine(*c).map(|(_, m)| (*c, m)))
            .collect();
        if methods
            .iter()
            .all(|(_, m)| matches!(m.kind, MethodKind::Constructor | MethodKind::Destructor))
        {
            self.not_a_value(name, expected, range);
            return err_expr(span);
        }
        let picked = methods.iter().find(|(_, m)| {
            !matches!(m.kind, MethodKind::Constructor | MethodKind::Destructor)
                && self.types.has_signature(expected, &m.params, m.ret)
        });
        let Some((id, m)) = picked.cloned() else {
            let (_, m) = &methods[0];
            let f = if m.self_kind == SelfKind::Static {
                ProcFamily::Plain
            } else {
                ProcFamily::Method
            };
            let (p, r) = (m.params.clone(), m.ret);
            self.value_mismatch(expected, f, &p, r, span);
            return err_expr(span);
        };
        if m.self_kind == SelfKind::Static {
            if family == ProcFamily::Method {
                self.value_mismatch(expected, ProcFamily::Plain, &m.params, m.ret, span);
                return err_expr(span);
            }
            return self.plain_value(id, expected, family, span);
        }
        if family == ProcFamily::Plain {
            self.value_mismatch(expected, ProcFamily::Method, &m.params, m.ret, span);
            return err_expr(span);
        }
        if m.self_kind == SelfKind::Instance && Self::no_instance(&recv) {
            let decl = self
                .method_of_routine(id)
                .and_then(|(o, m)| self.method_span(o, &m));
            self.instance_member_error(&m.name, decl, range);
            return err_expr(span);
        }
        let how = match self.method_dispatch(class, &m, false) {
            Dispatch::Virtual(k) => MethodRefHow::Virtual(k),
            Dispatch::VirtualClass(k) => MethodRefHow::ClassVirtual(k),
            _ if m.self_kind == SelfKind::Class => MethodRefHow::Class,
            _ => MethodRefHow::Method,
        };
        if m.self_kind == SelfKind::Instance {
            self.temporary_receiver(&recv, class);
        }
        let base = if m.self_kind == SelfKind::Class {
            self.receiver_class_ref(recv)
        } else {
            recv
        };
        Expr {
            kind: ExprKind::MethodRef {
                base: Box::new(base),
                routine: id,
                how,
            },
            ty: expected,
            value: None,
            span,
        }
    }

    /// A method reference to a built-in method of `TObject` (`docs/spec/objects.md` 9.3): a
    /// virtual one (`ToString`, `AfterConstruction`, `BeforeDestruction`) through the table; the
    /// others (`Free`, …) are E0752.
    #[allow(clippy::too_many_arguments)]
    fn builtin_method_value(
        &mut self,
        recv: Expr,
        class: TypeId,
        name: &str,
        family: ProcFamily,
        expected: Ty,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let builtin = self
            .methods_named(class, name)
            .into_iter()
            .find(|(_, m)| m.routine.is_none());
        let Some((_, m)) = builtin else {
            self.member_missing(class, name, range);
            return err_expr(span);
        };
        // A destructor (`Destroy`) is not a value, though it is virtual (9.2)
        let Some(k) = m.vmt.filter(|_| m.kind != MethodKind::Destructor) else {
            self.not_a_value(&m.name, expected, range);
            return err_expr(span);
        };
        if family == ProcFamily::Plain || !self.types.has_signature(expected, &m.params, m.ret) {
            self.value_mismatch(expected, ProcFamily::Method, &m.params, m.ret, span);
            return err_expr(span);
        }
        if Self::no_instance(&recv) {
            self.instance_member_error(&m.name, None, range);
            return err_expr(span);
        }
        self.temporary_receiver(&recv, class);
        let routine = self.builtin_routine(&m.name, m.ret);
        Expr {
            kind: ExprKind::MethodRef {
                base: Box::new(recv),
                routine,
                how: MethodRefHow::Virtual(k),
            },
            ty: expected,
            value: None,
            span,
        }
    }

    /// W0023 (`docs/spec/objects.md` 9.3): a method reference whose receiver is a new instance
    /// nothing else holds.
    fn temporary_receiver(&mut self, recv: &Expr, class: TypeId) {
        if !matches!(recv.kind, ExprKind::New { .. }) {
            return;
        }
        let t = self.tn(Ty::Class(class));
        let counted = self.types.slot_kind(Ty::Class(class)) == crate::bytecode::SLOT_REF;
        let note = if counted {
            "the instance is released at the end of the routine, and the method reference \
             is no longer assigned after that"
        } else {
            "nothing can free the instance, so it leaks"
        };
        let d = Diagnostic::warning(
            Code::W(23),
            format!("method reference to a temporary instance of `{t}`"),
            Stage::Sema,
        )
        .with_primary(self.span(recv.span), "")
        .with_note(note)
        .with_note(
            "store the instance in a variable or a field, and make the method reference from it",
        );
        self.push(d);
    }

    /// `=` / `<>` with a procedural value on one side (`docs/spec/objects.md` 9.5): compared as
    /// values, not called. An ordering of two procedural values that cannot be called without
    /// arguments is E0703. `None` when neither applies (the comparison is typed as usual, and a
    /// value written without parentheses is called).
    pub(super) fn proc_comparison(
        &mut self,
        lhs: &ast::Expr,
        rhs: &ast::Expr,
        op: BinOp,
        op_range: TextRange,
        span: TextRange,
    ) -> Option<Expr> {
        let side_type = |s: &mut Self, e: &ast::Expr| -> Ty {
            if super::expr::is_nil(e) || s.is_code_name(e) {
                return Ty::Error;
            }
            s.dry_run(|s| s.lower_value(e).ty)
        };
        let lt = side_type(self, lhs);
        let rt = side_type(self, rhs);
        if !matches!(op, BinOp::Eq | BinOp::Ne) {
            // An ordering calls the values that can be called without arguments (9.4); values
            // that cannot are not ordered
            let callable = |t: Ty| {
                self.types
                    .proc_sig(t)
                    .is_some_and(|(_, params, ret)| params.is_empty() && ret.is_some())
            };
            if !(lt.is_procedural() && rt.is_procedural()) || callable(lt) || callable(rt) {
                return None;
            }
            let msg = format!(
                "operator `{}` cannot be applied to `{}` and `{}`",
                op.text(),
                self.tn(lt),
                self.tn(rt)
            );
            // Typed for their reads (W0001, E2000)
            self.lower_value(lhs);
            self.lower_value(rhs);
            self.error(Code::E(703), msg, op_range);
            return Some(err_expr(span));
        }
        let target = if lt.is_procedural() {
            lt
        } else if rt.is_procedural() {
            rt
        } else {
            return None;
        };
        let side = |s: &mut Self, e: &ast::Expr| -> Expr {
            if super::expr::is_nil(e) {
                return Expr {
                    kind: ExprKind::Const,
                    ty: target,
                    value: Some(super::types::Const::Nil),
                    span: ast::trimmed_range(e.syntax()),
                };
            }
            // A side that is not a procedural value is reported below with its type
            s.lower_proc_expected(e, target)
        };
        let a = side(self, lhs);
        let b = side(self, rhs);
        if a.ty.is_error() || b.ty.is_error() {
            return Some(err_expr(span));
        }
        let fa = self.types.proc_family(a.ty);
        let fb = self.types.proc_family(b.ty);
        if fa.is_none() || fb.is_none() || fa != fb || !self.types.same_signature(a.ty, b.ty) {
            let msg = format!(
                "operator `{}` cannot be applied to `{}` and `{}`",
                op.text(),
                self.tn(a.ty),
                self.tn(b.ty)
            );
            self.error(Code::E(703), msg, op_range);
            return Some(err_expr(span));
        }
        let calc = if matches!(a.ty, Ty::Proc(_)) {
            Calc::Ref
        } else {
            Calc::Callable
        };
        Some(Expr {
            kind: ExprKind::Binary {
                first: Box::new(a),
                steps: vec![BinStep {
                    op,
                    rhs: b,
                    calc,
                    ty: Ty::Boolean,
                    span,
                }],
            },
            ty: Ty::Boolean,
            value: None,
            span,
        })
    }

    /// Whether a value of the procedural type `s` can be assigned to the procedural type `t`
    /// (`docs/spec/objects.md` 9.2): the same signature, and the same family or a procedural
    /// type or method reference type to an anonymous method type.
    pub(super) fn procedural_assignable(&self, s: Ty, t: Ty) -> bool {
        let (Some(fs), Some(ft)) = (self.types.proc_family(s), self.types.proc_family(t)) else {
            return false;
        };
        (fs == ft || ft == ProcFamily::Reference) && self.types.same_signature(s, t)
    }

    /// The conversion of a procedural value to the procedural type `target`
    /// ([`Sema::procedural_assignable`]): a procedural type's value becomes a callable value
    /// block (`ToClosure`); a method reference is already one (14.9).
    pub(super) fn coerce_procedural(&mut self, e: Expr, target: Ty) -> Expr {
        let span = e.span;
        if matches!(e.ty, Ty::Proc(_)) && matches!(target, Ty::Callable(_)) {
            return Expr {
                kind: ExprKind::ToClosure(Box::new(e)),
                ty: target,
                value: None,
                span,
            };
        }
        let mut w = e;
        w.ty = target;
        w
    }
}
