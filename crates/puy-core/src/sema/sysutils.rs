//! The `System.SysUtils` built-ins that do not take plain value arguments
//! (`docs/spec/semantics.md` 5.3): `Format` and `Exception.CreateFmt` with their `array of const`,
//! `TryStrToInt` with its `out` argument, and `Supports` with its interface name
//! (`docs/spec/objects.md` 7.6).

use super::expr::{ctor_of, err_expr};
use super::hir::*;
use super::types::{Ty, TypeId};
use super::{Resolved, Sema};
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

    /// The message of `T.CreateFmt(Fmt, […])` (`docs/spec/exceptions.md` 2.1,
    /// `docs/spec/objects.md` 8.2): `Format(Fmt, […])`.
    pub(super) fn format_message(&mut self, args: &[ast::Arg], span: TextRange) -> Option<Expr> {
        self.format_args(args, span)
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
        // An `array of const` variable or parameter: an array of `TValue`
        // (`docs/spec/rtti.md` 4.2)
        let values = args[1]
            .value()
            .filter(|v| ctor_of(v).is_none())
            .map(|v| self.lower_expr(&v));
        if let Some(values) = values {
            let fmt = fmt?;
            if fmt.ty.is_error() || values.ty.is_error() {
                return None;
            }
            let fmt = self.coerce(fmt, Ty::String, false, Some((1, "Fmt")));
            let of_values = self
                .types
                .elem_of(values.ty)
                .is_some_and(|e| values.ty.is_dyn_indexed() && self.types.is_tvalue(e));
            if !of_values {
                let found = self.tn(values.ty);
                self.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected `array of const`, found `{found}` (argument 2, parameter `Args`)"
                    ),
                    values.span,
                );
                return None;
            }
            if fmt.ty.is_error() {
                return None;
            }
            return Some(Expr {
                kind: ExprKind::Builtin {
                    func: Builtin::FormatValues,
                    args: vec![fmt, values],
                },
                ty: Ty::String,
                value: None,
                span,
            });
        }
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
                    // A distinct type as its root (`docs/spec/types.md` 1.3)
                    let e = self.lower_expr(&x);
                    let e = self.rooted(e);
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

impl Sema<'_, '_> {
    /// `Supports(X, IA)`, `Supports(X, IA, V)` and `Supports(C, IA)` (`docs/spec/objects.md`
    /// 7.6): `X` is a class, `TObject` or interface value, `C` a class reference (or a class
    /// name), `IA` the name of an interface and `V` a variable of `IA` or an ancestor of it.
    pub(super) fn supports(&mut self, args: &[ast::Arg], span: TextRange) -> Expr {
        self.check_no_width_pub(args);
        if !(2..=3).contains(&args.len()) {
            for (i, a) in args.iter().enumerate() {
                if let Some(v) = a.value() {
                    if i == 1 {
                        self.intf_name_arg(&v);
                    } else {
                        self.lower_expr(&v);
                    }
                }
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 2 to 3, found {}",
                    args.len()
                ),
                span,
            );
            return err_expr(span);
        }
        let x = args[0].value().map(|v| match self.class_name_of(&v) {
            Some(c) => self.class_ref_value(c, ast::trimmed_range(v.syntax())),
            None => self.lower_expr(&v),
        });
        let intf = args[1].value().and_then(|v| self.intf_name_arg(&v));
        let target = match args.get(2).map(|a| a.value()) {
            Some(Some(v)) => match self.ref_place(&v, (3, "Intf"), true, None) {
                Some((p, found)) => Some((p, found, ast::trimmed_range(v.syntax()))),
                None => return err_expr(span),
            },
            Some(None) => return err_expr(span),
            None => None,
        };
        let (Some(x), Some(intf)) = (x, intf) else {
            return err_expr(span);
        };
        if x.ty.is_error() {
            return err_expr(span);
        }
        // A value of a type parameter is seen as its constraint (`docs/spec/generics.md` 3.3)
        let x = match x.ty {
            Ty::Param(p) => match self.param_reference(x.ty) {
                Some(v) => Expr { ty: v, ..x },
                None => {
                    self.param_misuse(p, super::generics::ParamOp::TypeTest, x.span);
                    return err_expr(span);
                }
            },
            _ => x,
        };
        let class_ref = matches!(x.ty, Ty::ClassRef(_));
        if !(matches!(x.ty, Ty::Class(_) | Ty::Intf(_)) || class_ref && target.is_none()) {
            let expected = if target.is_none() {
                "an instance of a class, an interface or a class reference"
            } else {
                "an instance of a class or an interface"
            };
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected {expected}, found `{}` (argument 1, parameter `Instance`)",
                    self.tn(x.ty)
                ),
                x.span,
            );
            return err_expr(span);
        }
        if !class_ref
            && self
                .check_holds_statically(x.ty, Ty::Intf(intf), span)
                .is_none()
        {
            return err_expr(span);
        }
        let target = match target {
            Some((p, found, r)) => {
                let fits = match found {
                    Ty::Intf(j) => self.types.is_descendant(intf, j),
                    _ => found.is_error(),
                };
                if !fits {
                    let msg = format!(
                        "type mismatch: expected `{}`, found `{}` (argument 3, parameter `Intf`)",
                        self.tn(Ty::Intf(intf)),
                        self.tn(found)
                    );
                    self.error(Code::E(702), msg, r).notes.push(
                        "the variable must be of the interface or one of its ancestors".into(),
                    );
                    return err_expr(span);
                }
                Some(p)
            }
            None => None,
        };
        Expr {
            kind: ExprKind::Supports {
                operand: Box::new(x),
                intf,
                target,
                class_ref,
            },
            ty: Ty::Boolean,
            value: None,
            span,
        }
    }

    /// The interface named by the second argument of `Supports` (`docs/spec/objects.md` 7.6).
    /// Anything else is E0750.
    fn intf_name_arg(&mut self, r: &ast::Expr) -> Option<TypeId> {
        let range = ast::trimmed_range(r.syntax());
        let is_type = matches!(r, ast::Expr::NameRef(_) | ast::Expr::Field(_))
            && matches!(
                self.dry_run(|s| s.resolve_name_expr(r)),
                Some((Resolved::Type(_), ..))
            );
        if is_type {
            match self.resolve_name_expr(r) {
                Some((Resolved::Type(Ty::Intf(id)), ..)) => return Some(id),
                Some((Resolved::Type(Ty::Error), ..)) | None => return None,
                Some(_) => {}
            }
        } else {
            let ty = self.lower_expr(r).ty;
            if ty.is_error() {
                return None;
            }
            if ty == Ty::String {
                // Querying with a GUID (`docs/spec/objects.md` 7.1)
                self.error(
                    Code::E(3200),
                    "querying with a GUID is not supported yet (planned for M5)",
                    range,
                );
                return None;
            }
        }
        let text = self.src(range);
        self.error(Code::E(750), format!("`{text}` is not an interface"), range);
        None
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
