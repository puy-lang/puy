//! The `System.SysUtils` built-ins that do not take plain value arguments
//! (`docs/spec/semantics.md` 5.3): `Format` and `Exception.CreateFmt` with their `array of const`,
//! and `TryStrToInt` with its `out` argument.

use super::Sema;
use super::expr::{ctor_of, err_expr};
use super::hir::*;
use super::types::{Ty, TypeId};
use crate::diagnostics::Code;
use crate::source::TextRange;
use crate::syntax::ast::{self, AstNode};

impl Sema<'_, '_> {
    /// `Format(Fmt, […])`.
    pub(super) fn format_call(&mut self, args: &[ast::Arg], span: TextRange) -> Expr {
        match self.format_args(args, span) {
            Some(e) => e,
            None => err_expr(span),
        }
    }

    /// `T.CreateFmt(Fmt, […])` (`docs/spec/exceptions.md` 2.1): `T.Create(Format(Fmt, […]))`.
    pub(super) fn new_exception_fmt(
        &mut self,
        id: TypeId,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Expr {
        let Some(msg) = self.format_args(args, span) else {
            return err_expr(span);
        };
        Expr {
            kind: ExprKind::NewExc {
                class: Ty::Exception(id),
                msg: Box::new(msg),
            },
            ty: Ty::Exception(id),
            value: None,
            span,
        }
    }

    /// The `Format` call of `Format` / `CreateFmt`: the format and the elements of the `array of
    /// const` (each keeps its type; lower tells the VM the kind of each).
    fn format_args(&mut self, args: &[ast::Arg], span: TextRange) -> Option<Expr> {
        self.check_no_width_pub(args);
        if args.len() != 2 {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr_unchecked_ctor(&v);
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
        let fmt = args[0].value().map(|v| self.lower_expr(&v));
        let elems = args[1]
            .value()
            .and_then(|v| self.const_array(&v, (2, "Args")));
        let fmt = fmt?;
        if fmt.ty.is_error() {
            return None;
        }
        let fmt = self.coerce(fmt, Ty::String, false, Some((1, "Fmt")));
        if fmt.ty.is_error() {
            return None;
        }
        let mut all = vec![fmt];
        all.extend(elems?);
        Some(Expr {
            kind: ExprKind::Builtin {
                func: Builtin::Format,
                args: all,
            },
            ty: Ty::String,
            value: None,
            span,
        })
    }

    /// The elements of an `array of const` argument (`docs/spec/semantics.md` 5.3): an array
    /// constructor whose elements are integers, `Boolean`, `Char`, strings, reals or `Currency`.
    fn const_array(&mut self, v: &ast::Expr, param: (usize, &str)) -> Option<Vec<Expr>> {
        let Some(ac) = ctor_of(v) else {
            let e = self.lower_expr(v);
            if !e.ty.is_error() {
                let found = self.tn(e.ty);
                self.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected `array of const`, found `{found}` (argument {}, parameter `{}`)",
                        param.0, param.1
                    ),
                    e.span,
                );
            }
            return None;
        };
        let mut elems = Vec::new();
        let mut failed = false;
        for el in ac.elements() {
            match el {
                ast::CtorElem::Expr(x) => {
                    let e = self.lower_expr(&x);
                    if e.ty.is_error() {
                        failed = true;
                        continue;
                    }
                    if !const_array_accepts(e.ty) {
                        let t = self.tn(e.ty);
                        self.error(
                            Code::E(702),
                            format!("cannot pass a value of type `{t}` in an `array of const`"),
                            e.span,
                        );
                        failed = true;
                        continue;
                    }
                    elems.push(e);
                }
                ast::CtorElem::Range(r) => {
                    let (lo, hi) = r.bounds();
                    for b in [lo, hi].into_iter().flatten() {
                        self.lower_expr(&b);
                    }
                    self.error(
                        Code::E(702),
                        "a range is not allowed in an array constructor",
                        ast::trimmed_range(r.syntax()),
                    );
                    failed = true;
                }
            }
        }
        (!failed).then_some(elems)
    }

    /// `TryStrToInt(S, Value)`.
    pub(super) fn try_str_to_int(&mut self, args: &[ast::Arg], span: TextRange) -> Expr {
        self.check_no_width_pub(args);
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
            return err_expr(span);
        }
        let s = args[0].value().map(|v| self.lower_expr(&v));
        let target = args[1]
            .value()
            .and_then(|v| self.ref_arg(&v, (2, "Value"), Ty::Integer, true, None));
        let (Some(s), Some(target)) = (s, target) else {
            return err_expr(span);
        };
        if s.ty.is_error() {
            return err_expr(span);
        }
        let s = self.coerce(s, Ty::String, false, Some((1, "S")));
        if s.ty.is_error() {
            return err_expr(span);
        }
        Expr {
            kind: ExprKind::TryStrToInt {
                s: Box::new(s),
                target,
            },
            ty: Ty::Boolean,
            value: None,
            span,
        }
    }
}

/// The types an `array of const` takes (`docs/spec/semantics.md` 5.3).
fn const_array_accepts(t: Ty) -> bool {
    t.is_integer()
        || t.is_real()
        || matches!(
            t,
            Ty::Boolean | Ty::Char | Ty::String | Ty::Utf8String | Ty::Currency
        )
}
