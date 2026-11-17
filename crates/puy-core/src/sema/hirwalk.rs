//! A walk over the HIR of a routine that reaches every type, type id and routine id in it: the
//! instantiation of a generic body replaces them (`docs/design/generics.md` 3.1), and the work
//! list collects the routines a body refers to.

use super::hir::*;
use super::types::{Ty, TypeId};

/// What the walk calls for each type, type id and routine id it reaches.
pub(crate) trait HirVisit {
    fn ty(&mut self, t: &mut Ty);
    /// The id of a class or an interface (`is`, `Supports`, a class reference value, …).
    fn type_id(&mut self, _id: &mut TypeId) {}
    fn routine(&mut self, _r: &mut RoutineId) {}
    /// A global variable (a `class var` of an instantiation is a global of its own;
    /// `docs/spec/generics.md` 2.5).
    fn global(&mut self, _g: &mut GlobalId) {}
    /// A key equality or hash (`ExprKind::Key`) on values of `ty`, after its arguments (their
    /// types replaced): its routine, which the analysis fills in once `ty` has no type
    /// parameter in it (`docs/spec/equality.md` 8.3). `span` is the expression's.
    fn key(
        &mut self,
        _op: KeyOp,
        _ty: Ty,
        _func: &mut Option<RoutineId>,
        _span: crate::source::TextRange,
    ) {
    }
    /// An expression, before its parts are walked.
    fn expr(&mut self, _e: &mut Expr) {}
}

/// Every type and id of a routine: its variables, result type and body (not `parent`).
pub(crate) fn walk_routine(r: &mut Routine, v: &mut impl HirVisit) {
    for var in &mut r.vars {
        v.ty(&mut var.ty);
    }
    if let Some(t) = &mut r.ret {
        v.ty(t);
    }
    if let Some(c) = &mut r.dtor_of {
        v.type_id(c);
    }
    for s in &mut r.body {
        walk_stmt(s, v);
    }
}

fn walk_stmts(stmts: &mut [Stmt], v: &mut impl HirVisit) {
    for s in stmts {
        walk_stmt(s, v);
    }
}

fn walk_calc(c: &mut Calc, v: &mut impl HirVisit) {
    match c {
        Calc::Int(t) | Calc::Real(t) => v.ty(t),
        _ => {}
    }
}

fn walk_conversion(c: &mut Conversion, v: &mut impl HirVisit) {
    match c {
        Conversion::Int { to, .. }
        | Conversion::Set { to, .. }
        | Conversion::Range { to, .. }
        | Conversion::Num { to } => v.ty(to),
        Conversion::CharToStr
        | Conversion::StrToU8
        | Conversion::U8ToStr
        | Conversion::IntToChar { .. }
        | Conversion::IntToBool
        | Conversion::OrdToInt => {}
    }
}

fn walk_stmt(s: &mut Stmt, v: &mut impl HirVisit) {
    match &mut s.kind {
        StmtKind::Assign { target, value, .. } => {
            walk_place(target, v);
            walk_expr(value, v);
        }
        StmtKind::Block(b) => walk_stmts(b, v),
        StmtKind::Expr(e) => walk_expr(e, v),
        StmtKind::If {
            branches,
            else_body,
        } => {
            for (c, b) in branches {
                walk_expr(c, v);
                walk_stmt(b, v);
            }
            if let Some(b) = else_body {
                walk_stmt(b, v);
            }
        }
        StmtKind::While { cond, body } => {
            walk_expr(cond, v);
            walk_stmt(body, v);
        }
        StmtKind::Repeat { body, cond } => {
            walk_stmts(body, v);
            walk_expr(cond, v);
        }
        StmtKind::For {
            var,
            from,
            to,
            body,
            ..
        } => {
            if let ForVar::Global(g) = var {
                v.global(g);
            }
            walk_expr(from, v);
            walk_expr(to, v);
            walk_stmt(body, v);
        }
        StmtKind::ForIn {
            var,
            coll,
            elem,
            conv,
            body,
            enumerator,
            ..
        } => {
            if let ForVar::Global(g) = var {
                v.global(g);
            }
            walk_expr(coll, v);
            v.ty(elem);
            if let Some(c) = conv {
                walk_conversion(c, v);
            }
            walk_stmt(body, v);
            if let Some(e) = enumerator {
                walk_expr(&mut e.get, v);
                walk_expr(&mut e.move_next, v);
                walk_expr(&mut e.current, v);
            }
        }
        StmtKind::Declare(_) | StmtKind::Break | StmtKind::Continue => {}
        StmtKind::Exit(e) | StmtKind::Halt(e) | StmtKind::Raise(e) => {
            if let Some(e) = e {
                walk_expr(e, v);
            }
        }
        StmtKind::IncDec {
            target,
            target_ty,
            delta,
            calc,
            ..
        } => {
            walk_place(target, v);
            v.ty(target_ty);
            if let Some(d) = delta {
                walk_expr(d, v);
            }
            v.ty(calc);
        }
        StmtKind::Read { targets, .. } => {
            for t in targets {
                walk_place(t, v);
            }
        }
        StmtKind::Write { args, .. } => {
            for a in args {
                walk_expr(&mut a.value, v);
                if let Some(w) = &mut a.width {
                    walk_expr(w, v);
                }
                if let Some(p) = &mut a.precision {
                    walk_expr(p, v);
                }
            }
        }
        StmtKind::Flush | StmtKind::Nop => {}
        StmtKind::Assert { cond, message, .. } => {
            walk_expr(cond, v);
            if let Some(m) = message {
                walk_expr(m, v);
            }
        }
        StmtKind::SetLength { target, lens } => {
            walk_place(target, v);
            for l in lens {
                walk_expr(l, v);
            }
        }
        StmtKind::RaiseOuter(e) => walk_expr(e, v),
        StmtKind::Try { body, handler } => {
            walk_stmts(body, v);
            match handler {
                TryHandler::Except {
                    handlers,
                    else_body,
                } => {
                    for h in handlers {
                        v.ty(&mut h.class);
                        if let Some(b) = &mut h.body {
                            walk_stmt(b, v);
                        }
                    }
                    if let Some(b) = else_body {
                        walk_stmts(b, v);
                    }
                }
                TryHandler::Finally(b) => walk_stmts(b, v),
            }
        }
        StmtKind::Case {
            selector,
            arms,
            else_body,
        } => {
            walk_expr(selector, v);
            for a in arms {
                for l in &mut a.labels {
                    walk_expr(&mut l.low, v);
                    if let Some(h) = &mut l.high {
                        walk_expr(h, v);
                    }
                }
                if let Some(b) = &mut a.body {
                    walk_stmt(b, v);
                }
            }
            if let Some(b) = else_body {
                walk_stmts(b, v);
            }
        }
        StmtKind::SetElem { target, elem, .. } => {
            walk_place(target, v);
            walk_expr(elem, v);
        }
        StmtKind::Insert {
            source,
            target,
            index,
            ..
        } => {
            walk_expr(source, v);
            walk_place(target, v);
            walk_expr(index, v);
        }
        StmtKind::Exchange {
            target,
            first,
            second,
            ..
        } => {
            walk_place(target, v);
            walk_expr(first, v);
            walk_expr(second, v);
        }
        StmtKind::Delete {
            target,
            index,
            count,
        } => {
            walk_place(target, v);
            walk_expr(index, v);
            walk_expr(count, v);
        }
        StmtKind::PropertySet { call, .. } => walk_expr(call, v),
        StmtKind::ArrayMoveRange {
            target,
            from,
            to,
            count,
        } => {
            walk_place(target, v);
            walk_expr(from, v);
            walk_expr(to, v);
            walk_expr(count, v);
        }
        StmtKind::Free { target, .. }
        | StmtKind::FreeIfClass { value: target }
        | StmtKind::CheckDefaultOrder { value: target } => walk_expr(target, v),
        StmtKind::CheckClassType { value, what } => {
            walk_expr(value, v);
            walk_expr(what, v);
        }
        StmtKind::FreeAndNil { target } => walk_place(target, v),
    }
}

fn walk_place(p: &mut Place, v: &mut impl HirVisit) {
    match p {
        Place::Var(..) => {}
        Place::Global(g) => v.global(g),
        Place::StrIndex { base, index, .. } => {
            walk_place(base, v);
            walk_expr(index, v);
        }
        Place::Field { base, ty, .. } => {
            walk_place(base, v);
            v.ty(ty);
        }
        Place::Elem {
            base, index, ty, ..
        } => {
            walk_place(base, v);
            walk_expr(index, v);
            v.ty(ty);
        }
        Place::ObjField { base, ty, .. } => {
            walk_expr(base, v);
            v.ty(ty);
        }
    }
}

fn walk_args(args: &mut [CallArg], v: &mut impl HirVisit) {
    for a in args {
        match a {
            CallArg::Value(e) | CallArg::Temp(e) => walk_expr(e, v),
            CallArg::Ref { place, .. } => walk_place(place, v),
        }
    }
}

fn walk_dispatch(d: &mut Dispatch, v: &mut impl HirVisit) {
    if let Dispatch::Intf(id, _) = d {
        v.type_id(id);
    }
}

/// Every type and id of an expression.
pub(crate) fn walk_expr(e: &mut Expr, v: &mut impl HirVisit) {
    v.expr(e);
    v.ty(&mut e.ty);
    match &mut e.kind {
        ExprKind::Const | ExprKind::Var(_) | ExprKind::Error => {}
        ExprKind::Global(g) => v.global(g),
        ExprKind::Binary { first, steps } => {
            walk_expr(first, v);
            for s in steps {
                walk_expr(&mut s.rhs, v);
                walk_calc(&mut s.calc, v);
                v.ty(&mut s.ty);
            }
        }
        ExprKind::Unary { operand, calc, .. } => {
            walk_expr(operand, v);
            walk_calc(calc, v);
        }
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            walk_expr(base, v);
            walk_expr(index, v);
        }
        ExprKind::Field { base, .. }
        | ExprKind::ClassName { base }
        | ExprKind::ExcMember { base, .. }
        | ExprKind::ObjField { base, .. }
        | ExprKind::Weak(base)
        | ExprKind::ToClosure(base) => walk_expr(base, v),
        ExprKind::Slice { base, low, high } => {
            walk_expr(base, v);
            if let Some(l) = low {
                walk_expr(l, v);
            }
            if let Some(h) = high {
                walk_expr(h, v);
            }
        }
        ExprKind::IndexRange { first, last } => {
            if let Some(f) = first {
                walk_expr(f, v);
            }
            if let Some(l) = last {
                walk_expr(l, v);
            }
        }
        ExprKind::ArrayCtor(items) => {
            for i in items {
                walk_expr(i, v);
            }
        }
        ExprKind::SetCtor(items) => {
            for i in items {
                walk_expr(&mut i.low, v);
                if let Some(h) = &mut i.high {
                    walk_expr(h, v);
                }
            }
        }
        ExprKind::Call {
            routine,
            args,
            dispatch,
        } => {
            v.routine(routine);
            walk_args(args, v);
            walk_dispatch(dispatch, v);
        }
        ExprKind::Builtin { args, .. } | ExprKind::ClassInfo { args, .. } => {
            for a in args {
                walk_expr(a, v);
            }
        }
        ExprKind::Key { op, args, func } => {
            for a in args.iter_mut() {
                walk_expr(a, v);
            }
            if let Some(f) = func {
                v.routine(f);
            }
            let ty = args.first().map_or(Ty::Error, |a| a.ty);
            v.key(*op, ty, func, e.span);
        }
        ExprKind::Convert { operand, kind } => {
            walk_expr(operand, v);
            walk_conversion(kind, v);
        }
        ExprKind::New {
            class,
            ctor,
            args,
            of,
            dispatch,
            ..
        } => {
            v.ty(class);
            if let Some(c) = ctor {
                v.routine(c);
            }
            walk_args(args, v);
            if let Some(o) = of {
                walk_expr(o, v);
            }
            walk_dispatch(dispatch, v);
        }
        ExprKind::RecordNew { call } => walk_expr(call, v),
        ExprKind::InheritedBuiltin { base, msg, .. } => {
            walk_expr(base, v);
            if let Some(m) = msg {
                walk_expr(m, v);
            }
        }
        ExprKind::ClassRefValue(id) => v.type_id(id),
        ExprKind::ClassCheck { operand, class, .. } => {
            walk_expr(operand, v);
            v.type_id(class);
        }
        ExprKind::TryStrToInt { s, target } => {
            walk_expr(s, v);
            walk_place(target, v);
        }
        ExprKind::Supports {
            operand,
            intf,
            target,
            ..
        } => {
            walk_expr(operand, v);
            v.type_id(intf);
            if let Some(t) = target {
                walk_place(t, v);
            }
        }
        ExprKind::RoutineRef(r) => v.routine(r),
        ExprKind::MethodRef { base, routine, .. } => {
            walk_expr(base, v);
            v.routine(routine);
        }
        ExprKind::Anon { func, .. } | ExprKind::NestedRef { func, .. } => v.routine(func),
        ExprKind::CallValue { callee, args } => {
            walk_expr(callee, v);
            walk_args(args, v);
        }
    }
}

/// Whether a routine's HIR has a type parameter (`Ty::Param`) in it, directly or inside a type.
/// Only the debug builds' check in `ir::lower` uses it, so it is compiled under the same condition.
#[cfg(debug_assertions)]
pub(crate) fn has_param(r: &Routine, types: &super::types::TypeTable) -> Option<Ty> {
    struct Find<'t> {
        types: &'t super::types::TypeTable,
        found: Option<Ty>,
    }
    impl HirVisit for Find<'_> {
        fn ty(&mut self, t: &mut Ty) {
            if self.found.is_none() && self.types.has_param(*t) {
                self.found = Some(*t);
            }
        }
    }
    let mut f = Find { types, found: None };
    let mut copy = r.clone();
    walk_routine(&mut copy, &mut f);
    f.found
}
