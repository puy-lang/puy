//! Reals and `Currency` (`docs/spec/types.md` 8.4): the computation type, conversions to a real
//! or `Currency`, the operators and constant folding.

use super::Sema;
use super::hir::*;
use super::types::{Const, Ty};
use crate::diagnostics::Code;
use crate::numfmt;
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

fn const_expr(value: Const, ty: Ty, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Const,
        ty,
        value: Some(value),
        span,
    }
}

/// `notes` of E0702 / E0724 for a real to an integer type (`docs/spec/diagnostics.md` 4.10).
pub(super) const TRUNC_ROUND_NOTE: &str =
    "convert a floating-point value to an integer with `Trunc` or `Round`";

/// `notes` of E0703 for `div`, `mod` and the bitwise operators on a real.
const INTEGER_OPERATOR_NOTE: &str = "`div`, `mod` and bitwise operators need integer operands; convert with `Trunc` or `Round` first";

/// The text of the range of `Currency` in E0714.
const CURR_RANGE: &str = "(-922337203685477.5808..922337203685477.5807)";

/// Reals and `Currency`: the types whose operations are not integer operations.
pub(super) fn is_fractional(t: Ty) -> bool {
    t.is_real() || t == Ty::Currency
}

/// The computation type of a real or `Currency` operation (`docs/spec/types.md` 8.4), or `None`
/// when the operation is not one (an integer operation, or an operator reals do not have).
pub(super) fn real_calc_type(lt: Ty, op: BinOp, rt: Ty) -> Option<Ty> {
    let arith = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Slash);
    if !(arith || op.is_comparison()) || !lt.is_number() || !rt.is_number() {
        return None;
    }
    if !is_fractional(lt) && !is_fractional(rt) {
        // Integer division with `/` is a `Double` division
        return (op == BinOp::Slash).then_some(Ty::Double);
    }
    Some(if lt == Ty::Double || rt == Ty::Double {
        Ty::Double
    } else if lt == Ty::Single || rt == Ty::Single {
        // `Currency` and `Single` compute in `Double`
        if lt == Ty::Currency || rt == Ty::Currency {
            Ty::Double
        } else {
            Ty::Single
        }
    } else {
        Ty::Currency
    })
}

/// The value of a numeric constant as an `f64` (a `Single` constant holds a binary32 value).
fn real_value(c: &Const) -> f64 {
    c.as_f64()
        .expect("sema folds reals only from integer, real and Currency constants")
}

/// A real result as a constant of type `ty` (rounded to binary32 for `Single`).
fn real_const(v: f64, ty: Ty) -> Const {
    if ty == Ty::Single {
        Const::real(v as f32 as f64)
    } else {
        Const::real(v)
    }
}

/// IEEE 754 comparison (a NaN is unordered: only `<>` holds).
fn compare_f64(op: BinOp, a: f64, b: f64) -> bool {
    match op {
        BinOp::Eq => a == b,
        BinOp::Ne => a != b,
        BinOp::Lt => a < b,
        BinOp::Le => a <= b,
        BinOp::Gt => a > b,
        BinOp::Ge => a >= b,
        _ => unreachable!("not a comparison"),
    }
}

/// A comparison of ordered values.
fn compare_ord(op: BinOp, ord: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match op {
        BinOp::Eq => ord == Equal,
        BinOp::Ne => ord != Equal,
        BinOp::Lt => ord == Less,
        BinOp::Le => ord != Greater,
        BinOp::Gt => ord == Greater,
        BinOp::Ge => ord != Less,
        _ => unreachable!("not a comparison"),
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// E0714 for a constant outside `Currency` (`text` is the value as written in the message).
    fn curr_out_of_range(&mut self, text: &str, span: TextRange) {
        self.error(
            Code::E(714),
            format!("constant value {text} is out of range for `Currency` {CURR_RANGE}"),
            span,
        );
    }

    /// Converts an integer, real or `Currency` value to the real or `Currency` type `to`
    /// (`docs/spec/types.md` 8.4). A constant is converted at compile time (E0714 outside
    /// `Currency`); otherwise a `Num` conversion.
    pub(super) fn convert_num(&mut self, e: Expr, to: Ty) -> Expr {
        if e.ty == to || e.ty.is_error() {
            return e;
        }
        let span = e.span;
        if let Some(c) = &e.value {
            let value = match (to, c) {
                (Ty::Double, c) => Const::real(real_value(c)),
                (Ty::Single, Const::Int(v)) => Const::real(*v as f32 as f64),
                (Ty::Single, Const::Curr(v)) => Const::real(numfmt::curr_to_f32(*v) as f64),
                (Ty::Single, c) => real_const(real_value(c), Ty::Single),
                (_, Const::Int(v)) => match numfmt::int_to_curr(*v) {
                    Some(x) => Const::Curr(x),
                    None => {
                        self.curr_out_of_range(&v.to_string(), span);
                        return err_expr(span);
                    }
                },
                (_, c) => {
                    let v = real_value(c);
                    match numfmt::f64_to_curr(v) {
                        Some(x) => Const::Curr(x),
                        None => {
                            self.curr_out_of_range(&numfmt::f64_to_str(v), span);
                            return err_expr(span);
                        }
                    }
                }
            };
            return const_expr(value, to, span);
        }
        Expr {
            kind: ExprKind::Convert {
                operand: Box::new(e),
                kind: Conversion::Num { to },
            },
            ty: to,
            value: None,
            span,
        }
    }

    /// One step of a real or `Currency` operation (the left operand already has the computation
    /// type; `lower_binary` converts it). `None` after a diagnostic.
    pub(super) fn real_step(
        &mut self,
        lhs: &Expr,
        op: BinOp,
        rhs: Expr,
        op_range: TextRange,
        node: &ast::BinaryExpr,
    ) -> Option<(BinStep, Option<Const>)> {
        let Some(calc) = real_calc_type(lhs.ty, op, rhs.ty) else {
            self.operator_mismatch(op, lhs, &rhs, op_range, node);
            let integer_op = matches!(
                op,
                BinOp::Div
                    | BinOp::Mod
                    | BinOp::And
                    | BinOp::Or
                    | BinOp::Xor
                    | BinOp::Shl
                    | BinOp::Shr
            );
            if integer_op && lhs.ty.is_number() && rhs.ty.is_number() {
                self.last_mut().notes.push(INTEGER_OPERATOR_NOTE.into());
            }
            return None;
        };
        // `lower_binary` has converted the left operand; a trial type check may not have
        let lhs = self.convert_num(lhs.clone(), calc);
        if lhs.ty.is_error() {
            return None;
        }
        let rhs = self.convert_num(rhs, calc);
        if rhs.ty.is_error() {
            return None;
        }
        let ty = if op.is_comparison() {
            Ty::Boolean
        } else {
            calc
        };
        let value = match (&lhs.value, &rhs.value) {
            (Some(a), Some(b)) => Some(self.fold_real(op, a, b, calc, op_range)?),
            _ => None,
        };
        let calc = if calc == Ty::Currency {
            Calc::Curr
        } else {
            Calc::Real(calc)
        };
        Some((
            BinStep {
                op,
                rhs,
                calc,
                ty,
                span: op_range,
            },
            value,
        ))
    }

    /// Folds a real or `Currency` operation on constants of the computation type `calc` (the
    /// same result as at run time; `docs/spec/types.md` 8.4). `None` after E0714 / E0715.
    fn fold_real(
        &mut self,
        op: BinOp,
        a: &Const,
        b: &Const,
        calc: Ty,
        op_range: TextRange,
    ) -> Option<Const> {
        if calc == Ty::Currency {
            let (Const::Curr(x), Const::Curr(y)) = (a, b) else {
                return None;
            };
            let (x, y) = (*x, *y);
            if op.is_comparison() {
                // Scaled integers compare like the values
                return Some(Const::Bool(compare_ord(op, x.cmp(&y))));
            }
            let (r, exact) = match op {
                BinOp::Add => (numfmt::curr_add(x, y), x as i128 + y as i128),
                BinOp::Sub => (numfmt::curr_sub(x, y), x as i128 - y as i128),
                BinOp::Mul => (numfmt::curr_mul(x, y), x as i128 * y as i128 / 10000),
                _ => match numfmt::curr_div(x, y) {
                    Err(()) => {
                        self.error(
                            Code::E(715),
                            "division by zero in constant expression",
                            op_range,
                        );
                        return None;
                    }
                    Ok(r) => (r, x as i128 * 10000 / y as i128),
                },
            };
            return match r {
                Some(v) => Some(Const::Curr(v)),
                None => {
                    self.curr_out_of_range(&numfmt::curr_text_wide(exact), op_range);
                    None
                }
            };
        }
        let (x, y) = (real_value(a), real_value(b));
        if op.is_comparison() {
            return Some(Const::Bool(compare_f64(op, x, y)));
        }
        if calc == Ty::Single {
            let (x, y) = (x as f32, y as f32);
            let v = match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                _ => x / y,
            };
            return Some(Const::real(v as f64));
        }
        let v = match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            _ => x / y,
        };
        Some(Const::real(v))
    }

    /// Unary `-` of a real or `Currency` operand (a constant folds; `-` of the smallest
    /// `Currency` is E0714).
    pub(super) fn real_negate(
        &mut self,
        operand: Expr,
        op_range: TextRange,
        span: TextRange,
    ) -> Expr {
        let ty = operand.ty;
        match &operand.value {
            Some(Const::Curr(v)) => match v.checked_neg() {
                Some(n) => const_expr(Const::Curr(n), ty, span),
                None => {
                    self.curr_out_of_range(&numfmt::curr_text_wide(-(*v as i128)), op_range);
                    err_expr(span)
                }
            },
            Some(c) => const_expr(real_const(-real_value(c), ty), ty, span),
            None => Expr {
                kind: ExprKind::Unary {
                    op: UnOp::Neg,
                    operand: Box::new(operand),
                    calc: if ty == Ty::Currency {
                        Calc::Curr
                    } else {
                        Calc::Real(ty)
                    },
                },
                ty,
                value: None,
                span,
            },
        }
    }

    /// `Abs(X)` of a real or `Currency` (`docs/spec/semantics.md` 5.3): the same type. A constant
    /// folds (`Abs` of the smallest `Currency` is E0714).
    pub(super) fn real_abs(&mut self, x: Expr, span: TextRange) -> Expr {
        let ty = x.ty;
        match &x.value {
            Some(Const::Curr(v)) => match v.checked_abs() {
                Some(n) => const_expr(Const::Curr(n), ty, span),
                None => {
                    self.curr_out_of_range(&numfmt::curr_text_wide(-(*v as i128)), span);
                    err_expr(span)
                }
            },
            Some(c) => const_expr(real_const(real_value(c).abs(), ty), ty, span),
            None => Expr {
                kind: ExprKind::Builtin {
                    func: Builtin::Abs,
                    args: vec![x],
                },
                ty,
                value: None,
                span,
            },
        }
    }

    /// `Sqr(X)` of a real or `Currency`: `X * X` in the type of `X` (a constant folds).
    pub(super) fn real_sqr(&mut self, x: Expr, span: TextRange) -> Expr {
        let ty = x.ty;
        if let Some(c) = &x.value {
            return match self.fold_real(BinOp::Mul, c, c, ty, span) {
                Some(v) => const_expr(v, ty, span),
                None => err_expr(span),
            };
        }
        Expr {
            kind: ExprKind::Builtin {
                func: Builtin::Sqr,
                args: vec![x],
            },
            ty,
            value: None,
            span,
        }
    }
}
