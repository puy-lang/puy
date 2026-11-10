//! Sets (`docs/spec/types.md` 8.3): constructors, conversions between set types, the operators
//! and `in`, `Include` / `Exclude`, and constant folding.

use super::Sema;
use super::expr::PlaceUse;
use super::hir::*;
use super::types::{Const, DynWritten, Ty, setbits};
use crate::diagnostics::Code;
use crate::source::TextRange;
use crate::syntax::ast;

fn err_expr(span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Error,
        ty: Ty::Error,
        value: None,
        span,
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// The base type of a set's base type (`docs/spec/types.md` 8.2): `Integer` for `set of 0..9`.
    fn set_root(&self, set: Ty) -> Ty {
        self.types
            .base(self.types.set_base(set).unwrap_or(Ty::Error))
    }

    /// Whether an ordinal of type `t` can be an element of `set` (assignment compatible with the
    /// base type's base).
    pub(super) fn set_accepts(&self, set: Ty, t: Ty) -> bool {
        let root = self.set_root(set);
        let t = self.types.base(t);
        if root.is_integer() {
            t.is_integer()
        } else {
            t == root
        }
    }

    /// Whether two set types can be assigned and operated on together (`docs/spec/types.md` 8.3:
    /// the bases of their base types are assignment compatible).
    pub(super) fn sets_compatible(&self, a: Ty, b: Ty) -> bool {
        a.is_set() && b.is_set() && self.set_accepts(a, self.set_root(b))
    }

    /// E0714 for a constant element outside the range of a set's base type, with the set's name
    /// (`constant value 12 is out of range for `TDigits` (0..9)`).
    fn set_elem_out_of_range(&mut self, v: i128, set: Ty, span: TextRange) {
        let base = self.types.set_base(set).unwrap_or(Ty::Error);
        let msg = format!(
            "constant value {} is out of range for {}",
            self.types.bound_text(base, v),
            self.types.set_range_tail(set)
        );
        self.error(Code::E(714), msg, span);
    }

    /// Converts a set value to the compatible set type `target` (`docs/spec/types.md` 8.3). A
    /// constant is checked at compile time (E0714); otherwise, when `range_check` is set and the
    /// source's range does not fit, the conversion checks the elements at run time.
    pub(super) fn convert_set(&mut self, e: Expr, target: Ty, range_check: bool) -> Expr {
        if e.ty == target || e.ty.is_error() {
            return e;
        }
        let span = e.span;
        let (lo, hi) = self.types.set_range(target).unwrap_or((0, 0));
        if let Some(Const::Set(words)) = &e.value {
            if let Some(v) = setbits::elements(words).find(|v| *v < lo || *v > hi) {
                self.set_elem_out_of_range(v, target, span);
                return err_expr(span);
            }
            let mut c = e;
            c.ty = target;
            return c;
        }
        let fits = self
            .types
            .set_range(e.ty)
            .is_some_and(|(slo, shi)| lo <= slo && shi <= hi);
        let check = range_check && !fits;
        if !check && matches!((e.ty, target), (Ty::Set(_), Ty::Set(_))) {
            // The same bits
            let mut w = e;
            w.ty = target;
            return w;
        }
        Expr {
            kind: ExprKind::Convert {
                operand: Box::new(e),
                kind: Conversion::Set { to: target, check },
            },
            ty: target,
            value: None,
            span,
        }
    }

    /// A set constructor of the set type `set` (`docs/spec/types.md` 8.3): each element an ordinal
    /// assignment compatible with the base type, or a range `L..H` (empty when `L > H`). A
    /// constructor of constants is a constant. `inferred` is set when the type was not written
    /// (`in`, a constant without a type): an integer element outside 0..65535 is then the E0737
    /// variant.
    pub(super) fn lower_set_ctor(
        &mut self,
        ac: &ast::ArrayCtor,
        set: Ty,
        span: TextRange,
        inferred: bool,
    ) -> Expr {
        let checks = self.checks_at(span.start());
        let (lo, hi) = self.types.set_range(set).unwrap_or((0, 0));
        let root = self.set_root(set);
        let mut elems: Vec<SetCtorElem> = Vec::new();
        let mut failed = false;
        for el in ac.elements() {
            let bounds: Vec<ast::Expr> = match &el {
                ast::CtorElem::Expr(x) => vec![x.clone()],
                ast::CtorElem::Range(r) => {
                    let (a, b) = r.bounds();
                    match (a, b) {
                        (Some(a), Some(b)) => vec![a, b],
                        (a, b) => {
                            for x in [a, b].into_iter().flatten() {
                                self.lower_expr(&x);
                            }
                            failed = true;
                            continue;
                        }
                    }
                }
            };
            let mut parts: Vec<Expr> = Vec::new();
            let mut fits = true;
            for b in &bounds {
                let x = self.lower_expr_raw(b);
                if x.ty.is_error() {
                    failed = true;
                    continue;
                }
                if !self.set_accepts(set, x.ty) {
                    let msg = format!(
                        "type mismatch: expected `{}`, found `{}`",
                        self.tn(root),
                        self.tn(x.ty)
                    );
                    self.error(Code::E(702), msg, x.span);
                    failed = true;
                    continue;
                }
                if let Some(v) = x.value.as_ref().and_then(Const::as_int) {
                    if v < lo || v > hi {
                        if inferred {
                            self.error(
                                Code::E(737),
                                format!("set element {v} is not in 0..65535"),
                                x.span,
                            )
                            .notes
                            .push(
                                "a set without a written type of integers is `set of Word`".into(),
                            );
                        } else {
                            self.set_elem_out_of_range(v, set, x.span);
                        }
                        failed = true;
                        continue;
                    }
                } else if !self
                    .types
                    .ord_range(x.ty)
                    .is_some_and(|(slo, shi)| lo <= slo && shi <= hi)
                {
                    fits = false;
                }
                let mut x = x;
                x.ty = self.types.base(x.ty);
                parts.push(x);
            }
            if failed || parts.len() != bounds.len() {
                continue;
            }
            let mut it = parts.into_iter();
            let low = it.next().expect("an element has a bound");
            elems.push(SetCtorElem {
                low,
                high: it.next(),
                check: checks.range && !fits,
            });
        }
        if failed {
            return err_expr(span);
        }
        let all_const = elems
            .iter()
            .all(|e| e.low.value.is_some() && e.high.as_ref().is_none_or(|h| h.value.is_some()));
        if all_const {
            let mut words = Vec::new();
            for e in &elems {
                let l = e.low.value.as_ref().and_then(Const::as_int).unwrap_or(0);
                let h = e
                    .high
                    .as_ref()
                    .and_then(|h| h.value.as_ref())
                    .and_then(Const::as_int)
                    .unwrap_or(l);
                if l <= h {
                    setbits::insert_range(&mut words, l as u32, h as u32);
                }
            }
            return Expr {
                kind: ExprKind::Const,
                ty: set,
                value: Some(Const::Set(setbits::trim(words))),
                span,
            };
        }
        Expr {
            kind: ExprKind::SetCtor(elems),
            ty: set,
            value: None,
            span,
        }
    }

    /// The set type of a constructor whose type is not written, from the type of an ordinal
    /// (`docs/spec/types.md` 8.3): `set of Char`, `set of Boolean`, the set of an enumeration, and
    /// `set of Word` for an integer. `None` for any other type.
    fn inferred_set_type(&mut self, elem: Ty, span: TextRange) -> Option<Ty> {
        let base = match self.types.base(elem) {
            Ty::Char => Ty::Char,
            Ty::Boolean => Ty::Boolean,
            t @ Ty::Enum(_) => t,
            t if t.is_integer() => Ty::Word,
            _ => return None,
        };
        if let Ty::Enum(_) = base
            && !self
                .types
                .ord_range(base)
                .is_some_and(|(lo, hi)| lo >= 0 && hi <= 65535)
        {
            let t = self.tn(base);
            self.error(
                Code::E(737),
                format!("set base type `{t}` must have ordinal values in 0..65535"),
                span,
            )
            .notes
            .push("use a subrange type such as `0..255` as the base type".into());
            return Some(Ty::Error);
        }
        Some(self.types.set_of(
            base,
            DynWritten::Anonymous {
                generic: false,
                span,
            },
        ))
    }

    /// The set after `in` when it is a constructor: the set of the left operand's type.
    pub(super) fn lower_in_ctor(&mut self, ac: &ast::ArrayCtor, lhs: Ty, span: TextRange) -> Expr {
        if lhs.is_error() {
            self.lower_ctor_elements_quietly(ac);
            return err_expr(span);
        }
        match self.inferred_set_type(lhs, span) {
            Some(Ty::Error) => {
                self.lower_ctor_elements_quietly(ac);
                err_expr(span)
            }
            Some(set) => self.lower_set_ctor(ac, set, span, true),
            None => {
                self.lower_ctor_elements_quietly(ac);
                let t = self.tn(lhs);
                self.error(
                    Code::E(703),
                    format!("operator `in` cannot be applied to `{t}` and a set constructor"),
                    span,
                );
                err_expr(span)
            }
        }
    }

    /// A constant `[…]` without a written type (`const Digits = ['0'..'9'];`): a set whose base
    /// type comes from the first element. `None` when no set type fits (E0729 by the caller).
    pub(super) fn lower_untyped_set_const(
        &mut self,
        ac: &ast::ArrayCtor,
        span: TextRange,
    ) -> Option<Expr> {
        let first = match ac.elements().into_iter().next()? {
            ast::CtorElem::Expr(x) => x,
            ast::CtorElem::Range(r) => r.bounds().0?,
        };
        let t = self.dry_run(|s| s.lower_expr_raw(&first).ty);
        if t.is_error() {
            self.lower_ctor_elements_quietly(ac);
            return Some(err_expr(span));
        }
        match self.inferred_set_type(t, span)? {
            Ty::Error => {
                self.lower_ctor_elements_quietly(ac);
                Some(err_expr(span))
            }
            set => Some(self.lower_set_ctor(ac, set, span, true)),
        }
    }

    /// One step of a binary operation with a set operand (`docs/spec/types.md` 8.3). `None` when
    /// the operator does not apply (the caller reports E0703).
    pub(super) fn set_step(
        &mut self,
        lhs: &Expr,
        op: BinOp,
        rhs: Expr,
        op_range: TextRange,
    ) -> Option<Option<(BinStep, Option<Const>)>> {
        let (lt, rt) = (lhs.ty, rhs.ty);
        if op == BinOp::In {
            if !rt.is_set() || !lt.is_ordinal() || !self.set_accepts(rt, lt) {
                return None;
            }
            let value = match (lhs.value.as_ref().and_then(Const::as_int), &rhs.value) {
                (Some(x), Some(Const::Set(w))) => Some(Const::Bool(setbits::contains(w, x))),
                _ => None,
            };
            let step = BinStep {
                op,
                rhs,
                calc: Calc::SetIn,
                ty: Ty::Boolean,
                span: op_range,
            };
            return Some(Some((step, value)));
        }
        if !self.sets_compatible(lt, rt) {
            return None;
        }
        let checks = self.checks_at(op_range.start());
        let (rhs, ty) = match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                // The result has the left operand's type: the right operand is converted to it,
                // checked like an assignment for `+` (the elements `-` and `*` drop are not in the
                // result anyway)
                let check = op == BinOp::Add && checks.range;
                let r = self.convert_set(rhs, lt, check);
                if r.ty.is_error() {
                    return Some(None);
                }
                (r, lt)
            }
            BinOp::Eq | BinOp::Ne | BinOp::Le | BinOp::Ge => {
                // A small set compared with a big one is widened (`Calc::Set` on a big set
                // compares blocks of different sizes)
                let r = match (lt, rt) {
                    (Ty::BigSet(_), Ty::Set(_)) => self.convert_set(rhs, lt, false),
                    _ => rhs,
                };
                (r, Ty::Boolean)
            }
            _ => return None,
        };
        let value = match (&lhs.value, &rhs.value) {
            (Some(Const::Set(a)), Some(Const::Set(b))) => Some(match op {
                BinOp::Add => Const::Set(setbits::zip(a, b, |x, y| x | y)),
                BinOp::Sub => Const::Set(setbits::zip(a, b, |x, y| x & !y)),
                BinOp::Mul => Const::Set(setbits::zip(a, b, |x, y| x & y)),
                BinOp::Eq => Const::Bool(a == b),
                BinOp::Ne => Const::Bool(a != b),
                BinOp::Le => Const::Bool(setbits::is_subset(a, b)),
                _ => Const::Bool(setbits::is_subset(b, a)),
            }),
            _ => None,
        };
        let step = BinStep {
            op,
            rhs,
            calc: Calc::Set,
            ty,
            span: op_range,
        };
        Some(Some((step, value)))
    }

    /// `Include(S, X)` / `Exclude(S, X)` (`docs/spec/types.md` 8.3): `S` a set variable (E0706
    /// otherwise), `X` an element of its base type.
    pub(super) fn lower_include(
        &mut self,
        args: &[ast::Arg],
        exclude: bool,
        span: TextRange,
    ) -> Option<StmtKind> {
        if args.len() != 2 {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 2, found {}",
                    args.len()
                ),
                span,
            );
            return None;
        }
        let s = args[0].value()?;
        let x = args[1].value();
        let place = self.lower_place(&s, PlaceUse::IncDec);
        let Some((place, ty)) = place else {
            if let Some(super::expr::PlaceResult::NotVariable(r)) = self.last_place_result.take() {
                self.error(
                    Code::E(706),
                    "argument 1 must be a variable (parameter `S` is `var`)",
                    r,
                );
            }
            if let Some(x) = x {
                self.lower_expr(&x);
            }
            return None;
        };
        let x = x?;
        if ty.is_error() {
            self.lower_expr(&x);
            return None;
        }
        if !ty.is_set() {
            self.lower_expr(&x);
            let r = ast::trimmed_range(s.syntax());
            let msg = format!(
                "type mismatch: expected a set, found `{}` (argument 1, parameter `S`)",
                self.tn(ty)
            );
            self.error(Code::E(702), msg, r);
            return None;
        }
        let e = self.lower_expr_raw(&x);
        if e.ty.is_error() {
            return None;
        }
        if !self.set_accepts(ty, e.ty) {
            let msg = format!(
                "type mismatch: expected `{}`, found `{}` (argument 2, parameter `X`)",
                self.tn(self.set_root(ty)),
                self.tn(e.ty)
            );
            self.error(Code::E(702), msg, e.span);
            return None;
        }
        let (lo, hi) = self.types.set_range(ty).unwrap_or((0, 0));
        if let Some(v) = e.value.as_ref().and_then(Const::as_int)
            && (v < lo || v > hi)
        {
            self.set_elem_out_of_range(v, ty, e.span);
            return None;
        }
        let fits = e.value.is_some()
            || self
                .types
                .ord_range(e.ty)
                .is_some_and(|(slo, shi)| lo <= slo && shi <= hi);
        let check = self.checks_at(span.start()).range && !fits;
        let mut e = e;
        e.ty = self.types.base(e.ty);
        Some(StmtKind::SetElem {
            target: place,
            elem: e,
            exclude,
            check,
        })
    }
}
