//! `TValue` (`docs/spec/rtti.md` chapters 3 and 4): the built-ins the part of `System` written in
//! Pascal makes it of (`ValueWrap`, `ValueIs`, `ValueCast`), and the elements of an `array of
//! const` argument, each made a `TValue` of its own type.

use super::Sema;
use super::expr::{err_expr, is_nil};
use super::hir::*;
use super::types::Ty;
use crate::diagnostics::Code;
use crate::source::TextRange;
use crate::syntax::ast;

impl Sema<'_, '_> {
    /// `ValueWrap(X, TypeInfo(T))`, `ValueIs(T, V, Raise)` and `ValueCast(T, V)` (the part of
    /// `System` written in Pascal; `docs/spec/bytecode.md` 2.24), and the built-ins of the
    /// standard library taking a `TValue` as it is (`ValueArrayValue`, `ValueArraySetValue`,
    /// `RttiFieldGet`, `RttiFieldSet`).
    pub(super) fn value_builtin(&mut self, f: Builtin, args: &[ast::Arg], span: TextRange) -> Expr {
        // What `sema::finish` makes for `System.Rtti` (`docs/spec/rtti.md` 1.5)
        match f {
            Builtin::RttiMethodRef => self.instances.rtti_wants.invoke = true,
            Builtin::RttiPropRef => self.instances.rtti_wants.props = true,
            Builtin::RttiAttrFunc => self.instances.rtti_wants.attrs = true,
            _ => {}
        }
        let expected = match f {
            Builtin::ValueIs | Builtin::ValueArrayValue | Builtin::RttiFieldGet => 3,
            Builtin::ValueArraySetValue | Builtin::RttiFieldSet | Builtin::RttiBridge => 4,
            Builtin::RttiMethodRef | Builtin::RttiAttrFunc => 2,
            Builtin::RttiPropRef | Builtin::RttiArgSet => 3,
            _ => 2,
        };
        if args.len() != expected {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {expected}, found {}",
                    args.len()
                ),
                span,
            );
            return err_expr(span);
        }
        let values: Option<Vec<ast::Expr>> = args.iter().map(|a| a.value()).collect();
        let Some(values) = values else {
            return err_expr(span);
        };
        let tvalue = self.tvalue_type();
        // The built-ins whose arguments are plain values: their parameters' types and their type
        let plain = match f {
            // `AType`, `ARef`, `Index` (and the `TValue` to write)
            Builtin::ValueArrayValue => {
                Some(([Ty::Integer, Ty::ValueRef, Ty::Integer, tvalue], tvalue))
            }
            Builtin::ValueArraySetValue => Some((
                [Ty::Integer, Ty::ValueRef, Ty::Integer, tvalue],
                Ty::ValueRef,
            )),
            // `Table`, `Index`, the instance's `TValue` (and the `TValue` to write)
            Builtin::RttiFieldGet | Builtin::RttiFieldSet => {
                Some(([Ty::Integer, Ty::Integer, tvalue, tvalue], tvalue))
            }
            // `Index` and the instance's `TValue`; `Index`, `Which` and the instance's `TValue`
            Builtin::RttiMethodRef => Some((
                [Ty::Integer, tvalue, Ty::Error, Ty::Error],
                Ty::Callable(super::types::TPROC),
            )),
            Builtin::RttiPropRef => Some((
                [Ty::Integer, Ty::Integer, tvalue, Ty::Error],
                Ty::Callable(super::types::TPROC),
            )),
            _ => None,
        };
        if let Some((params, ty)) = plain {
            let mut out = Vec::with_capacity(values.len());
            for (i, (v, p)) in values.iter().zip(params).enumerate() {
                let e = self.lower_expr(v);
                if e.ty.is_error() || p.is_error() {
                    return err_expr(span);
                }
                out.push(self.coerce(e, p, false, Some((i + 1, ""))));
            }
            if out.iter().any(|e| e.ty.is_error()) {
                return err_expr(span);
            }
            return Expr {
                kind: ExprKind::Builtin { func: f, args: out },
                ty,
                value: None,
                span,
            };
        }
        if matches!(f, Builtin::RttiBridge | Builtin::RttiAttrFunc) {
            return self.rtti_func_builtin(f, &values, span);
        }
        if f == Builtin::RttiArgSet {
            return self.rtti_arg_set(&values, tvalue, span);
        }
        if f == Builtin::ValueWrap {
            let x = self.lower_expr(&values[0]);
            let info = self.lower_expr(&values[1]);
            if x.ty.is_error() || info.ty.is_error() || tvalue.is_error() {
                return err_expr(span);
            }
            if !matches!(info.kind, ExprKind::TypeInfo(_)) {
                self.error(Code::E(776), "`TypeInfo` expects a type", info.span);
                return err_expr(span);
            }
            return Expr {
                kind: ExprKind::Builtin {
                    func: f,
                    args: vec![x, info],
                },
                ty: tvalue,
                value: None,
                span,
            };
        }
        let arg_span = ast::trimmed_range(values[0].syntax());
        let Some(t) = self.type_name_arg(&values[0]) else {
            self.error(Code::E(776), "`TypeInfo` expects a type", arg_span);
            return err_expr(span);
        };
        let v = self.lower_expr(&values[1]);
        if t.is_error() || v.ty.is_error() || tvalue.is_error() {
            return err_expr(span);
        }
        if v.ty != tvalue {
            let found = self.tn(v.ty);
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected `TValue`, found `{found}` (argument 2, parameter `V`)"
                ),
                v.span,
            );
            return err_expr(span);
        }
        let t = match t {
            Ty::OpenArray(id) => Ty::DynArray(id),
            Ty::Weak(id) => Ty::Class(id),
            t => t,
        };
        let info = Expr {
            kind: ExprKind::TypeInfo(t),
            ty: Ty::TypeInfo,
            value: None,
            span: arg_span,
        };
        let mut out = vec![info, v];
        let ty = if f == Builtin::ValueIs {
            let raise = self.lower_expr(&values[2]);
            if raise.ty.is_error() {
                return err_expr(span);
            }
            out.push(self.coerce(raise, Ty::Boolean, false, Some((3, "Raise"))));
            Ty::Boolean
        } else {
            t
        };
        Expr {
            kind: ExprKind::Builtin { func: f, args: out },
            ty,
            value: None,
            span,
        }
    }

    /// `RttiBridge(T, Table, Index, Which)` / `RttiAttrFunc(T, Index)`: a function of the type
    /// information section as a value of the procedural type `T` (the standard library's
    /// declaration of the function's parameters).
    fn rtti_func_builtin(&mut self, f: Builtin, values: &[ast::Expr], span: TextRange) -> Expr {
        let arg_span = ast::trimmed_range(values[0].syntax());
        let t = self.type_name_arg(&values[0]);
        let Some(t @ Ty::Proc(_)) = t else {
            self.error(Code::E(702), "expected a procedural type", arg_span);
            return err_expr(span);
        };
        let mut out = Vec::new();
        for (i, v) in values[1..].iter().enumerate() {
            let e = self.lower_expr(v);
            if e.ty.is_error() {
                return err_expr(span);
            }
            out.push(self.coerce(e, Ty::Integer, false, Some((i + 2, ""))));
        }
        Expr {
            kind: ExprKind::Builtin { func: f, args: out },
            ty: t,
            value: None,
            span,
        }
    }

    /// `RttiArgSet(Args, Index, V)`: `Args` an open or dynamic array of `TValue`.
    fn rtti_arg_set(&mut self, values: &[ast::Expr], tvalue: Ty, span: TextRange) -> Expr {
        let arr = self.lower_expr(&values[0]);
        let index = self.lower_expr(&values[1]);
        let v = self.lower_expr(&values[2]);
        if [&arr, &index, &v].iter().any(|e| e.ty.is_error()) || tvalue.is_error() {
            return err_expr(span);
        }
        let elem = match arr.ty {
            Ty::OpenArray(_) | Ty::DynArray(_) => self.types.elem_of(arr.ty),
            _ => None,
        };
        if elem != Some(tvalue) || v.ty != tvalue {
            self.error(
                Code::E(702),
                "expected an array of `TValue` and a `TValue`",
                span,
            );
            return err_expr(span);
        }
        let index = self.coerce(index, Ty::Integer, false, Some((2, "Index")));
        Expr {
            kind: ExprKind::Builtin {
                func: Builtin::RttiArgSet,
                args: vec![arr, index, v],
            },
            ty: Ty::Integer,
            value: None,
            span,
        }
    }

    /// An element of an array constructor passed to an `array of const` parameter (an open array
    /// of `TValue`; `docs/spec/rtti.md` 4.1): a `TValue` of the element's own type, a `TValue` as
    /// it is, and `nil` the empty `TValue`. A routine name written without a call is E0777 (there
    /// is no procedural type to take it as). `None` after a diagnostic.
    pub(super) fn const_array_elem(&mut self, x: &ast::Expr, tvalue: Ty) -> Option<Expr> {
        let span = ast::trimmed_range(x.syntax());
        if is_nil(x) {
            return Some(Expr {
                kind: ExprKind::Builtin {
                    func: Builtin::Default,
                    args: Vec::new(),
                },
                ty: tvalue,
                value: None,
                span,
            });
        }
        let outer = std::mem::replace(&mut self.const_elem, true);
        let v = self.lower_expr(x);
        self.const_elem = outer;
        if v.ty.is_error() {
            return None;
        }
        if v.ty == tvalue {
            return Some(v);
        }
        // A distinct type keeps its own type information (`docs/spec/rtti.md` 2.2); a one-character
        // string literal is a `string` (4.1)
        let ty = match v.ty {
            Ty::Char if is_string_literal(x) => Ty::String,
            Ty::OpenArray(id) => Ty::DynArray(id),
            t => t,
        };
        let v = if ty == v.ty {
            v
        } else {
            self.coerce(v, ty, false, None)
        };
        let info = Expr {
            kind: ExprKind::TypeInfo(ty),
            ty: Ty::TypeInfo,
            value: None,
            span,
        };
        Some(Expr {
            kind: ExprKind::Builtin {
                func: Builtin::ValueWrap,
                args: vec![v, info],
            },
            ty: tvalue,
            value: None,
            span,
        })
    }

    /// E0777: a routine name as an element of an `array of const` (`docs/spec/rtti.md` 4.1, 9).
    pub(super) fn routine_in_const_array(&mut self, range: TextRange) {
        let name = self.src(range);
        self.error(
            Code::E(777),
            format!("routine `{name}` cannot be an element of an `array of const`"),
            range,
        )
        .notes
        .push("call it, or pass a value of a procedural type".into());
    }
}

/// Whether the expression is a string literal, possibly in parentheses.
fn is_string_literal(e: &ast::Expr) -> bool {
    matches!(
        super::overload::strip_parens(e),
        ast::Expr::Literal(l)
            if l.token().is_some_and(|t| t.kind() == crate::syntax::SyntaxKind::STRING_LIT)
    )
}
