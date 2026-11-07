//! Typing of expressions and constant folding (`docs/spec/semantics.md` chapters 2–3 and 5).

use super::builtins::{self, builtin_name, builtin_signature};
use super::hir::*;
use super::types::{Const, MAX_STRING_LEN, Ty};
use super::{Resolved, Sema};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::lexer;
use crate::source::TextRange;
use crate::syntax::SyntaxKind;
use crate::syntax::ast::{self, AstNode, ParamMode};

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

impl<'a, 's> Sema<'a, 's> {
    /// Types an expression.
    pub(super) fn lower_expr(&mut self, e: &ast::Expr) -> Expr {
        let span = ast::trimmed_range(e.syntax());
        match e {
            ast::Expr::Literal(l) => self.lower_literal(l, span),
            ast::Expr::Paren(p) => match p.inner() {
                Some(inner) => {
                    let mut x = self.lower_expr(&inner);
                    x.span = span;
                    x
                }
                None => err_expr(span),
            },
            ast::Expr::NameRef(_) | ast::Expr::Field(_) => self.lower_name_use(e, span),
            ast::Expr::Unary(u) => self.lower_unary(u, span),
            ast::Expr::Binary(b) => self.lower_binary(b, span),
            ast::Expr::Call(c) => self.lower_call_expr(c, span),
            ast::Expr::Index(i) => self.lower_index(i, span),
            ast::Expr::Deref(_) | ast::Expr::Error(_) => err_expr(span),
            ast::Expr::Nil(_) => {
                self.error(
                    Code::E(3200),
                    "`nil` is not supported yet (planned for M3)",
                    span,
                );
                err_expr(span)
            }
            ast::Expr::Inherited(_) => {
                self.error(
                    Code::E(3200),
                    "`inherited` is not supported yet (planned for M3)",
                    span,
                );
                err_expr(span)
            }
        }
    }

    fn lower_literal(&mut self, l: &ast::Literal, span: TextRange) -> Expr {
        let Some(tok) = l.token() else {
            return err_expr(span);
        };
        let text = self.text(&tok).as_bytes();
        match tok.kind() {
            SyntaxKind::INT_LIT => {
                let Some(v) = lexer::int_literal_value(text) else {
                    return err_expr(span);
                };
                let v = v as i128;
                const_expr(Const::Int(v), Ty::of_literal(v), span)
            }
            SyntaxKind::STRING_LIT => {
                let base: u32 = tok.text_range().start().into();
                match lexer::string_literal_value(text, base) {
                    Ok(units) => {
                        if units.len() == 1 {
                            const_expr(Const::Char(units[0]), Ty::Char, span)
                        } else {
                            const_expr(Const::Str(units), Ty::String, span)
                        }
                    }
                    Err(e) => {
                        let d = Diagnostic::error(
                            Code::E(9),
                            "invalid multi-line string",
                            Stage::Lexer,
                        )
                        .with_primary(self.span(e.range), "shallower than the closing quotes");
                        self.push(d);
                        err_expr(span)
                    }
                }
            }
            SyntaxKind::FLOAT_LIT => {
                self.error(
                    Code::E(3200),
                    "floating-point literals are not supported yet (planned for M2)",
                    span,
                );
                err_expr(span)
            }
            _ => err_expr(span),
        }
    }

    /// Uses a name as a value.
    fn lower_name_use(&mut self, e: &ast::Expr, span: TextRange) -> Expr {
        let Some((r, name, range)) = self.resolve_name_expr(e) else {
            return err_expr(span);
        };
        match r {
            Resolved::Var(id) => {
                self.mark_used(id, span);
                Expr {
                    kind: ExprKind::Var(id),
                    ty: self.var(id).ty,
                    value: None,
                    span,
                }
            }
            Resolved::Global(id) => Expr {
                kind: ExprKind::Global(id),
                ty: self.globals[id.0 as usize].ty,
                value: None,
                span,
            },
            Resolved::Const { value, ty, .. } => {
                if ty.is_error() {
                    err_expr(span)
                } else {
                    const_expr(value, ty, span)
                }
            }
            Resolved::Bool(b) => const_expr(Const::Bool(b), Ty::Boolean, span),
            Resolved::Routine(id) => self.lower_bare_routine_call(id, &name, range, span),
            Resolved::Func(f) => self.lower_builtin_call(f, &name, &[], range, span),
            Resolved::Proc(p) => {
                // A statement-only built-in (such as Break) written in an expression
                let pname = builtins::proc_name(p);
                self.error(
                    Code::E(725),
                    format!("procedure `{pname}` has no value"),
                    range,
                );
                err_expr(span)
            }
            Resolved::Type(t) => {
                if !self.allow_type_as_value {
                    self.error(
                        Code::E(708),
                        format!("`{}` is a type and cannot be used as a value", t.name()),
                        range,
                    );
                }
                err_expr(span)
            }
            Resolved::Unit(_) => {
                self.error(
                    Code::E(708),
                    format!("`{name}` is a unit and cannot be used as a value"),
                    range,
                );
                err_expr(span)
            }
            Resolved::Error => err_expr(span),
        }
    }

    /// A routine name without parentheses (`X := Next;`).
    fn lower_bare_routine_call(
        &mut self,
        id: RoutineId,
        name: &str,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let info = &self.routines[id.0 as usize];
        let is_self = self.cur.as_ref().is_some_and(|c| c.routine == Some(id));
        if info.ret.is_none() {
            // A procedure used in an expression
            let n = info.name.clone();
            let name_span = info.name_span;
            let d = Diagnostic::error(
                Code::E(725),
                format!("procedure `{n}` has no value"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.span(name_span), "declared here");
            self.push(d);
            return err_expr(span);
        }
        if is_self {
            let no_params = info.params.is_empty();
            let n = info.name.clone();
            let name_span = info.name_span;
            let mut d = Diagnostic::error(
                Code::E(704),
                format!("recursive call to `{n}` needs parentheses"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.span(name_span), "declared here")
            .with_fix(Fix::suggestion(
                "replace with `Result`",
                vec![TextEdit::replace(self.input.file, range, "Result")],
            ));
            if no_params {
                d = d.with_fix(Fix::suggestion(
                    format!("replace with `{n}()`"),
                    vec![TextEdit::replace(self.input.file, range, format!("{n}()"))],
                ));
            }
            self.push(d);
            return err_expr(span);
        }
        if !info.params.is_empty() {
            let n = info.name.clone();
            let d = Diagnostic::error(
                Code::E(704),
                format!("function `{n}` requires arguments"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.span(info.name_span), "declared here");
            self.push(d);
            return err_expr(span);
        }
        let _ = name;
        Expr {
            kind: ExprKind::Call {
                routine: id,
                args: Vec::new(),
            },
            ty: info.ret.unwrap(),
            value: None,
            span,
        }
    }

    fn lower_index(&mut self, i: &ast::IndexExpr, span: TextRange) -> Expr {
        let Some(base) = i.base() else {
            return err_expr(span);
        };
        let base_e = self.lower_expr(&base);
        let indices = i.indices();
        if base_e.ty.is_error() {
            for ix in &indices {
                self.lower_expr(ix);
            }
            return err_expr(span);
        }
        if indices.len() != 1 {
            let n = indices.len();
            self.error(
                Code::E(705),
                format!("wrong number of indices: expected 1, found {n}"),
                span,
            );
            return err_expr(span);
        }
        if base_e.ty != Ty::String {
            self.error(
                Code::E(720),
                format!("cannot index a value of type `{}`", base_e.ty),
                span,
            );
            return err_expr(span);
        }
        let idx = self.lower_expr(&indices[0]);
        if idx.ty.is_error() {
            return err_expr(span);
        }
        if !idx.ty.is_integer() {
            let r = idx.span;
            self.error(
                Code::E(719),
                format!(
                    "index type mismatch: expected `Integer`, found `{}`",
                    idx.ty
                ),
                r,
            );
            return err_expr(span);
        }
        Expr {
            kind: ExprKind::StrIndex {
                base: Box::new(base_e),
                index: Box::new(idx),
            },
            ty: Ty::Char,
            value: None,
            span,
        }
    }

    // ---- Unary ----

    fn lower_unary(&mut self, u: &ast::UnaryExpr, span: TextRange) -> Expr {
        let Some(op_tok) = u.op() else {
            return err_expr(span);
        };
        let op_range = op_tok.text_range();
        let operand = match u.operand() {
            Some(o) => self.lower_expr(&o),
            None => return err_expr(span),
        };
        if operand.ty.is_error() {
            return err_expr(span);
        }
        match op_tok.kind() {
            SyntaxKind::AT => err_expr(span),
            SyntaxKind::PLUS => {
                if operand.ty.is_integer() {
                    let mut e = operand;
                    e.span = span;
                    e
                } else {
                    self.error(
                        Code::E(703),
                        format!("operator `+` cannot be applied to `{}`", operand.ty),
                        op_range,
                    );
                    err_expr(span)
                }
            }
            SyntaxKind::MINUS => {
                if !operand.ty.is_integer() {
                    self.error(
                        Code::E(703),
                        format!("operator `-` cannot be applied to `{}`", operand.ty),
                        op_range,
                    );
                    return err_expr(span);
                }
                if let Some(Const::Int(v)) = operand.value {
                    // Negating a constant re-derives the type from the value
                    let nv = -v;
                    if !Ty::Int64.contains(nv) {
                        self.const_out_of_range(nv, Ty::Int64, op_range);
                        return err_expr(span);
                    }
                    return const_expr(Const::Int(nv), Ty::of_literal(nv), span);
                }
                let calc = match operand.ty {
                    t if t.fits_in_integer() => Ty::Integer,
                    Ty::Cardinal | Ty::Int64 => Ty::Int64,
                    Ty::UInt64 => {
                        let d = Diagnostic::error(
                            Code::E(716),
                            "cannot mix `UInt64` and signed integers",
                            Stage::Sema,
                        )
                        .with_primary(self.span(op_range), "")
                        .with_fix(Fix::suggestion(
                            "convert to `Int64`",
                            vec![
                                TextEdit::insert(self.input.file, operand.span.start(), "Int64("),
                                TextEdit::insert(self.input.file, operand.span.end(), ")"),
                            ],
                        ));
                        self.push(d);
                        return err_expr(span);
                    }
                    _ => unreachable!(),
                };
                Expr {
                    kind: ExprKind::Unary {
                        op: UnOp::Neg,
                        operand: Box::new(operand),
                        calc: Calc::Int(calc),
                    },
                    ty: calc,
                    value: None,
                    span,
                }
            }
            SyntaxKind::NOT_KW => match operand.ty {
                Ty::Boolean => {
                    let value = match operand.value {
                        Some(Const::Bool(b)) => Some(Const::Bool(!b)),
                        _ => None,
                    };
                    Expr {
                        kind: ExprKind::Unary {
                            op: UnOp::Not,
                            operand: Box::new(operand),
                            calc: Calc::Bool,
                        },
                        ty: Ty::Boolean,
                        value,
                        span,
                    }
                }
                t if t.is_integer() => {
                    let calc = if t.fits_in_integer() { Ty::Integer } else { t };
                    let value = match operand.value {
                        Some(Const::Int(v)) => Some(Const::Int(calc.wrap(!v))),
                        _ => None,
                    };
                    Expr {
                        kind: ExprKind::Unary {
                            op: UnOp::Not,
                            operand: Box::new(operand),
                            calc: Calc::Int(calc),
                        },
                        ty: calc,
                        value,
                        span,
                    }
                }
                t => {
                    self.error(
                        Code::E(703),
                        format!("operator `not` cannot be applied to `{t}`"),
                        op_range,
                    );
                    err_expr(span)
                }
            },
            _ => err_expr(span),
        }
    }

    // ---- Binary ----

    /// Flattens a left-leaning chain of `BINARY_EXPR` and types it (recursing only into right operands).
    fn lower_binary(&mut self, b: &ast::BinaryExpr, span: TextRange) -> Expr {
        // Collect the left-leaning chain
        let mut chain: Vec<ast::BinaryExpr> = vec![b.clone()];
        let mut first: Option<ast::Expr> = None;
        loop {
            let cur = chain.last().unwrap();
            match cur.lhs() {
                Some(ast::Expr::Binary(inner)) if !is_paren(&inner) => chain.push(inner),
                Some(other) => {
                    first = Some(other);
                    break;
                }
                None => break,
            }
        }
        let Some(first) = first else {
            return err_expr(span);
        };
        let mut acc = self.lower_expr(&first);
        let mut steps: Vec<BinStep> = Vec::new();
        let mut first_expr: Option<Expr> = None;
        let mut had_error = acc.ty.is_error();
        for node in chain.iter().rev() {
            let Some(op_tok) = node.op() else {
                had_error = true;
                continue;
            };
            let op_range = op_tok.text_range();
            let rhs = match node.rhs() {
                Some(r) => self.lower_expr(&r),
                None => err_expr(TextRange::empty(op_range.end())),
            };
            let Some(op) = binop_of(op_tok.kind()) else {
                // in / is / as / `/`
                match op_tok.kind() {
                    SyntaxKind::SLASH => {
                        self.error(
                            Code::E(3200),
                            "`/` is not supported yet (planned for M2)",
                            op_range,
                        )
                        .notes
                        .push("use `div` for the integer quotient".into());
                    }
                    SyntaxKind::IN_KW => {
                        self.error(
                            Code::E(3200),
                            "`in` is not supported yet (planned for M2)",
                            op_range,
                        );
                    }
                    _ => {
                        let t = self.text(&op_tok).to_string();
                        self.error(
                            Code::E(3200),
                            format!("`{t}` is not supported yet (planned for M3)"),
                            op_range,
                        );
                    }
                }
                had_error = true;
                continue;
            };
            if had_error || rhs.ty.is_error() {
                had_error = true;
                continue;
            }
            let step_span = TextRange::new(acc.span.start(), rhs.span.end());
            match self.binary_step(&acc, op, rhs, op_range, node) {
                Some((step, value)) => {
                    let ty = step.ty;
                    if let Some(v) = value {
                        // If everything so far is constant, fold it and use it as the left operand of the next step (3.6)
                        acc = const_expr(v, ty, step_span);
                        first_expr = None;
                        steps.clear();
                        continue;
                    }
                    if first_expr.is_none() {
                        // A leading `Char` is converted to `string` in string operations
                        let fe = if step.calc == Calc::Str && acc.ty == Ty::Char {
                            self.as_string_expr(acc)
                        } else {
                            acc
                        };
                        first_expr = Some(fe);
                    }
                    acc = Expr {
                        kind: ExprKind::Error,
                        ty,
                        value,
                        span: step_span,
                    };
                    steps.push(step);
                }
                None => {
                    had_error = true;
                }
            }
        }
        if had_error {
            return err_expr(span);
        }
        let Some(first_expr) = first_expr else {
            // No steps
            return acc;
        };
        // If everything is constant, it is a constant expression
        let ty = acc.ty;
        if let Some(v) = acc.value.clone() {
            return const_expr(v, ty, span);
        }
        Expr {
            kind: ExprKind::Binary {
                first: Box::new(first_expr),
                steps,
            },
            ty,
            value: None,
            span,
        }
    }

    /// Typing and folding of one step of a binary operation. Looks only at the type and value of `lhs`.
    fn binary_step(
        &mut self,
        lhs: &Expr,
        op: BinOp,
        rhs: Expr,
        op_range: TextRange,
        node: &ast::BinaryExpr,
    ) -> Option<(BinStep, Option<Const>)> {
        let lt = lhs.ty;
        let rt = rhs.ty;
        let both_int = lt.is_integer() && rt.is_integer();
        let is_strlike = |t: Ty| matches!(t, Ty::String | Ty::Char);
        // Comparisons
        if op.is_comparison() {
            if both_int {
                let calc = self.int_calc_type(lhs, &rhs, op_range)?;
                let value = match (&lhs.value, &rhs.value) {
                    (Some(Const::Int(a)), Some(Const::Int(b))) => {
                        Some(Const::Bool(compare(op, a.cmp(b))))
                    }
                    _ => None,
                };
                let rhs = self.adapt_const(rhs, calc);
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Int(calc),
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    value,
                ));
            }
            if lt == Ty::Boolean && rt == Ty::Boolean {
                let value = match (&lhs.value, &rhs.value) {
                    (Some(Const::Bool(a)), Some(Const::Bool(b))) => {
                        Some(Const::Bool(compare(op, a.cmp(b))))
                    }
                    _ => None,
                };
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Bool,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    value,
                ));
            }
            if lt == Ty::Char && rt == Ty::Char {
                let value = match (&lhs.value, &rhs.value) {
                    (Some(Const::Char(a)), Some(Const::Char(b))) => {
                        Some(Const::Bool(compare(op, a.cmp(b))))
                    }
                    _ => None,
                };
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Char,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    value,
                ));
            }
            if is_strlike(lt) && is_strlike(rt) {
                let value = match (
                    lhs.value.as_ref().and_then(Const::as_str),
                    rhs.value.as_ref().and_then(Const::as_str),
                ) {
                    (Some(a), Some(b)) => Some(Const::Bool(compare(op, compare_strings(&a, &b)))),
                    _ => None,
                };
                let rhs = self.as_string_expr(rhs);
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Str,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    value,
                ));
            }
            self.operator_mismatch(op, lhs, &rhs, op_range, node);
            return None;
        }
        match op {
            BinOp::And | BinOp::Or | BinOp::Xor => {
                if lt == Ty::Boolean && rt == Ty::Boolean {
                    let value = match (&lhs.value, &rhs.value) {
                        (Some(Const::Bool(a)), Some(Const::Bool(b))) => {
                            Some(Const::Bool(match op {
                                BinOp::And => *a && *b,
                                BinOp::Or => *a || *b,
                                _ => a ^ b,
                            }))
                        }
                        _ => None,
                    };
                    return Some((
                        BinStep {
                            op,
                            rhs,
                            calc: Calc::Bool,
                            ty: Ty::Boolean,
                            span: op_range,
                        },
                        value,
                    ));
                }
                if !both_int {
                    self.operator_mismatch(op, lhs, &rhs, op_range, node);
                    return None;
                }
            }
            BinOp::Add if is_strlike(lt) && is_strlike(rt) => {
                let value = match (
                    lhs.value.as_ref().and_then(Const::as_str),
                    rhs.value.as_ref().and_then(Const::as_str),
                ) {
                    (Some(a), Some(b)) => {
                        if a.len() + b.len() > MAX_STRING_LEN {
                            self.error(
                                Code::E(726),
                                "string constant is too long (limit 2147483647 characters)",
                                op_range,
                            )
                            .notes
                            .push("build the string at run time instead".into());
                            return None;
                        }
                        let mut s = a;
                        s.extend(b);
                        Some(Const::Str(s))
                    }
                    _ => None,
                };
                let rhs = self.as_string_expr(rhs);
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Str,
                        ty: Ty::String,
                        span: op_range,
                    },
                    value,
                ));
            }
            _ => {
                if !both_int {
                    self.operator_mismatch(op, lhs, &rhs, op_range, node);
                    return None;
                }
            }
        }
        // Integer operations
        let calc = if matches!(op, BinOp::Shl | BinOp::Shr) {
            // The computation type of a shift is determined by the left operand
            if lt.fits_in_integer() {
                Ty::Integer
            } else {
                lt
            }
        } else {
            self.int_calc_type(lhs, &rhs, op_range)?
        };
        let value = match (&lhs.value, &rhs.value) {
            (Some(Const::Int(a)), Some(Const::Int(b))) => {
                Some(self.fold_int(op, *a, *b, calc, op_range)?)
            }
            (_, Some(Const::Int(0))) if matches!(op, BinOp::Div | BinOp::Mod) => {
                self.error(
                    Code::E(715),
                    "division by zero in constant expression",
                    op_range,
                );
                return None;
            }
            _ => None,
        };
        let rhs = if matches!(op, BinOp::Shl | BinOp::Shr) {
            rhs
        } else {
            self.adapt_const(rhs, calc)
        };
        Some((
            BinStep {
                op,
                rhs,
                calc: Calc::Int(calc),
                ty: calc,
                span: op_range,
            },
            value,
        ))
    }

    /// Adjusts a constant operand to the computation type (changes only the type).
    fn adapt_const(&self, mut e: Expr, calc: Ty) -> Expr {
        if let Some(Const::Int(v)) = &e.value
            && calc.contains(*v)
        {
            e.ty = calc;
        }
        e
    }

    /// Computation type of an integer binary operation (the table in 3.2). Mixing `UInt64` with signed types is E0716.
    fn int_calc_type(&mut self, lhs: &Expr, rhs: &Expr, op_range: TextRange) -> Option<Ty> {
        let effective = |e: &Expr, other: Ty| -> Ty {
            // A constant whose value fits the other operand's type is treated as that type
            if let Some(Const::Int(v)) = &e.value
                && other.is_integer()
                && other.contains(*v)
            {
                return other;
            }
            e.ty
        };
        let lt = effective(lhs, rhs.ty);
        let rt = effective(rhs, lhs.ty);
        if lt.fits_in_integer() && rt.fits_in_integer() {
            return Some(Ty::Integer);
        }
        let unsigned = |t: Ty| matches!(t, Ty::Byte | Ty::Word | Ty::Cardinal | Ty::UInt64);
        if lt == Ty::UInt64 || rt == Ty::UInt64 {
            if unsigned(lt) && unsigned(rt) {
                return Some(Ty::UInt64);
            }
            let (u, s) = if lt == Ty::UInt64 {
                (lhs, rhs)
            } else {
                (rhs, lhs)
            };
            let d = Diagnostic::error(
                Code::E(716),
                "cannot mix `UInt64` and signed integers",
                Stage::Sema,
            )
            .with_primary(self.span(op_range), "")
            .with_fix(Fix::suggestion(
                "convert the `UInt64` operand to `Int64`",
                vec![
                    TextEdit::insert(self.input.file, u.span.start(), "Int64("),
                    TextEdit::insert(self.input.file, u.span.end(), ")"),
                ],
            ))
            .with_fix(Fix::suggestion(
                "convert the signed operand to `UInt64`",
                vec![
                    TextEdit::insert(self.input.file, s.span.start(), "UInt64("),
                    TextEdit::insert(self.input.file, s.span.end(), ")"),
                ],
            ));
            self.push(d);
            return None;
        }
        if lt == Ty::Int64 || rt == Ty::Int64 {
            return Some(Ty::Int64);
        }
        // One side is Cardinal
        if (lt == Ty::Cardinal && unsigned(rt)) || (rt == Ty::Cardinal && unsigned(lt)) {
            return Some(Ty::Cardinal);
        }
        Some(Ty::Int64)
    }

    fn fold_int(
        &mut self,
        op: BinOp,
        a: i128,
        b: i128,
        calc: Ty,
        op_range: TextRange,
    ) -> Option<Const> {
        let v = match op {
            BinOp::Add => a + b,
            BinOp::Sub => a - b,
            BinOp::Mul => a * b,
            BinOp::Div => {
                if b == 0 {
                    self.error(
                        Code::E(715),
                        "division by zero in constant expression",
                        op_range,
                    );
                    return None;
                }
                a / b
            }
            BinOp::Mod => {
                if b == 0 {
                    self.error(
                        Code::E(715),
                        "division by zero in constant expression",
                        op_range,
                    );
                    return None;
                }
                a % b
            }
            BinOp::And => a & b,
            BinOp::Or => a | b,
            BinOp::Xor => a ^ b,
            BinOp::Shl | BinOp::Shr => {
                let bits = if matches!(calc, Ty::Int64 | Ty::UInt64) {
                    64
                } else {
                    32
                };
                let n = (b & (bits - 1)) as u32;
                let wrapped = calc.wrap(a);
                let v = if op == BinOp::Shl {
                    calc.wrap(wrapped << n)
                } else {
                    // Logical shift: as an unsigned value of the width
                    let u = if bits == 64 {
                        wrapped as u64 as i128
                    } else {
                        wrapped as u32 as i128
                    };
                    calc.wrap(u >> n)
                };
                return Some(Const::Int(v));
            }
            _ => unreachable!(),
        };
        if !calc.contains(v) {
            self.const_out_of_range(v, calc, op_range);
            return None;
        }
        Some(Const::Int(v))
    }

    pub(super) fn const_out_of_range(&mut self, v: i128, ty: Ty, range: TextRange) {
        let (lo, hi) = ty.range().unwrap_or((0, 0));
        self.error(
            Code::E(714),
            format!("constant value {v} is out of range for `{ty}` ({lo}..{hi})"),
            range,
        );
    }

    /// E0703 and its fixes.
    fn operator_mismatch(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        op_range: TextRange,
        node: &ast::BinaryExpr,
    ) {
        let msg = format!(
            "operator `{}` cannot be applied to `{}` and `{}`",
            op.text(),
            lhs.ty,
            rhs.ty
        );
        let mut d =
            Diagnostic::error(Code::E(703), msg, Stage::Sema).with_primary(self.span(op_range), "");
        if self.dry_run > 0 {
            // No fixes inside a trial type check (the result is discarded)
            self.push(d);
            return;
        }
        // `not A = B` → `not (A = B)` (`grammar.md` 1.5)
        if op.is_comparison()
            && let Some(f) = self.not_paren_fix(op, rhs, op_range, node)
        {
            d = d.with_fix(f);
        }
        // `X = 1 and Y = 2` → `(X = 1) and (Y = 2)` (`semantics.md` chapter 3)
        if let Some(f) = self.paren_chain_fix(node) {
            d = d.with_fix(f);
        }
        // Concatenating / comparing a string and an integer: suggest IntToStr
        let strlike = |t: Ty| matches!(t, Ty::String | Ty::Char);
        let str_and_int = (op == BinOp::Add || op.is_comparison())
            && ((strlike(lhs.ty) && rhs.ty.is_integer())
                || (lhs.ty.is_integer() && strlike(rhs.ty)));
        // If the user declared `IntToStr`, do not offer the `IntToStr(…)` fix
        if str_and_int && self.find_visible("inttostr").is_none() {
            let int_side = if lhs.ty.is_integer() { lhs } else { rhs };
            let prefix = match &self.sysutils {
                super::SysUtils::Alias(a) => format!("{a}.IntToStr("),
                _ => "IntToStr(".to_string(),
            };
            let mut edits = vec![
                TextEdit::insert(self.input.file, int_side.span.start(), prefix.clone()),
                TextEdit::insert(self.input.file, int_side.span.end(), ")"),
            ];
            if self.sysutils == super::SysUtils::NotUsed
                && let Some(f) = self.uses_sysutils_fix()
            {
                edits.extend(f.edits);
            }
            edits.sort_by_key(|e| e.range.start());
            d = d.with_fix(Fix::suggestion(format!("convert with `{prefix}…)`"), edits));
        }
        if str_and_int && op == BinOp::Add && self.is_write_arg_top(node) {
            d = d.with_fix(Fix::suggestion(
                "pass as separate arguments",
                vec![TextEdit::replace(
                    self.input.file,
                    TextRange::new(lhs.span.end(), rhs.span.start()),
                    ", ",
                )],
            ));
        }
        self.push(d);
    }

    /// When the left operand of a comparison is `not A`, a fix adding parentheses if the form `not (A = B)` passes type checking.
    fn not_paren_fix(
        &mut self,
        op: BinOp,
        rhs: &Expr,
        op_range: TextRange,
        node: &ast::BinaryExpr,
    ) -> Option<Fix> {
        let Some(ast::Expr::Unary(u)) = node.lhs() else {
            return None;
        };
        if u.op()?.kind() != SyntaxKind::NOT_KW {
            return None;
        }
        let operand = u.operand()?;
        let rhs = rhs.clone();
        let (start, ok) = self.dry_run(|s| {
            let a = s.lower_expr(&operand);
            if a.ty.is_error() {
                return (a.span.start(), false);
            }
            let start = a.span.start();
            let ok = s
                .binary_step(&a, op, rhs, op_range, node)
                .is_some_and(|(step, _)| step.ty == Ty::Boolean);
            (start, ok)
        });
        if !ok {
            return None;
        }
        let end = node.rhs().map(|r| ast::trimmed_range(r.syntax()).end())?;
        Some(Fix::safe(
            "add parentheses: `not (…)`",
            vec![
                TextEdit::insert(self.input.file, start, "("),
                TextEdit::insert(self.input.file, end, ")"),
            ],
        ))
    }

    /// For an expression mixing comparisons with `and` / `or` / `xor` without parentheses (`X = 1 and Y = 2`), a fix
    /// that wraps each part separated by `and` / `or` / `xor` in parentheses, if that form passes type checking.
    fn paren_chain_fix(&mut self, node: &ast::BinaryExpr) -> Option<Fix> {
        // The outermost binary expression containing that operator (parentheses are PAREN_EXPR, so it does not cross them)
        let mut top = node.clone();
        while let Some(p) = top.syntax().parent().and_then(ast::BinaryExpr::cast) {
            top = p;
        }
        // Flatten left to right (no Rust recursion, even for deep trees)
        enum Work {
            Expr(ast::Expr),
            Op(ast::BinaryExpr),
        }
        let mut leaves: Vec<ast::Expr> = Vec::new();
        let mut ops: Vec<(BinOp, TextRange)> = Vec::new();
        let mut stack = vec![Work::Expr(ast::Expr::Binary(top))];
        while let Some(w) = stack.pop() {
            match w {
                Work::Expr(ast::Expr::Binary(b)) => {
                    stack.push(Work::Expr(b.rhs()?));
                    stack.push(Work::Op(b.clone()));
                    stack.push(Work::Expr(b.lhs()?));
                }
                Work::Expr(e) => leaves.push(e),
                Work::Op(b) => {
                    let t = b.op()?;
                    ops.push((binop_of(t.kind())?, t.text_range()));
                }
            }
        }
        let is_logic = |o: BinOp| matches!(o, BinOp::And | BinOp::Or | BinOp::Xor);
        if !ops.iter().any(|(o, _)| o.is_comparison()) || !ops.iter().any(|(o, _)| is_logic(*o)) {
            return None;
        }
        // Split at `and` / `or` / `xor` (ranges of leaf indices, inclusive at both ends)
        let mut parts: Vec<(usize, usize)> = Vec::new();
        let mut start = 0;
        for (i, (o, _)) in ops.iter().enumerate() {
            if is_logic(*o) {
                parts.push((start, i));
                start = i + 1;
            }
        }
        parts.push((start, leaves.len() - 1));
        // Whether each part types as Boolean (no diagnostics are reported)
        let spans: Option<Vec<(TextRange, bool)>> = self.dry_run(|s| {
            parts
                .iter()
                .map(|&(a, b)| {
                    let (ty, span) = s.type_of_part(&leaves[a..=b], &ops[a..b], node)?;
                    (ty == Ty::Boolean).then_some((span, a < b))
                })
                .collect()
        });
        let spans = spans?;
        let mut edits = Vec::new();
        for (span, wrap) in spans {
            if wrap {
                edits.push(TextEdit::insert(self.input.file, span.start(), "("));
                edits.push(TextEdit::insert(self.input.file, span.end(), ")"));
            }
        }
        if edits.is_empty() {
            return None;
        }
        Some(Fix::safe("add parentheses around each comparison", edits))
    }

    /// Types a sequence of leaves and operators without `and` / `or` / `xor` by precedence, returning the type and the span.
    fn type_of_part(
        &mut self,
        leaves: &[ast::Expr],
        ops: &[(BinOp, TextRange)],
        node: &ast::BinaryExpr,
    ) -> Option<(Ty, TextRange)> {
        fn prec(o: BinOp) -> u8 {
            if o.is_comparison() {
                0
            } else if matches!(o, BinOp::Add | BinOp::Sub) {
                1
            } else {
                2
            }
        }
        let mut vals: Vec<Expr> = Vec::new();
        let mut pending: Vec<(BinOp, TextRange)> = Vec::new();
        let first = self.lower_expr(&leaves[0]);
        if first.ty.is_error() {
            return None;
        }
        vals.push(first);
        for (i, &(op, r)) in ops.iter().enumerate() {
            while let Some(&(top, tr)) = pending.last()
                && prec(top) >= prec(op)
            {
                pending.pop();
                self.reduce_part(&mut vals, top, tr, node)?;
            }
            pending.push((op, r));
            let v = self.lower_expr(&leaves[i + 1]);
            if v.ty.is_error() {
                return None;
            }
            vals.push(v);
        }
        while let Some((top, tr)) = pending.pop() {
            self.reduce_part(&mut vals, top, tr, node)?;
        }
        let e = vals.pop()?;
        Some((e.ty, e.span))
    }

    fn reduce_part(
        &mut self,
        vals: &mut Vec<Expr>,
        op: BinOp,
        op_range: TextRange,
        node: &ast::BinaryExpr,
    ) -> Option<()> {
        let rhs = vals.pop()?;
        let lhs = vals.pop()?;
        let span = TextRange::new(lhs.span.start(), rhs.span.end());
        let (step, value) = self.binary_step(&lhs, op, rhs, op_range, node)?;
        vals.push(Expr {
            kind: ExprKind::Error,
            ty: step.ty,
            value,
            span,
        });
        Some(())
    }

    /// Whether that `+` is the top-level operator of a single argument to `Write` / `WriteLn`.
    fn is_write_arg_top(&self, node: &ast::BinaryExpr) -> bool {
        let Some(arg) = node.syntax().parent().and_then(ast::Arg::cast) else {
            return false;
        };
        let Some(list) = arg.syntax().parent() else {
            return false;
        };
        let Some(call) = list.parent().and_then(ast::CallExpr::cast) else {
            return false;
        };
        let Some(ast::Expr::NameRef(n)) = call.callee() else {
            return false;
        };
        let lower = n.text().to_ascii_lowercase();
        (lower == "write" || lower == "writeln")
            && arg
                .exprs()
                .first()
                .is_some_and(|e| e.syntax() == node.syntax())
    }

    /// Converts a `Char` to `string` (a `string` is left as is).
    pub(super) fn as_string_expr(&mut self, e: Expr) -> Expr {
        if e.ty != Ty::Char {
            return e;
        }
        let value = match &e.value {
            Some(Const::Char(c)) => Some(Const::Str(vec![*c])),
            _ => None,
        };
        let span = e.span;
        Expr {
            kind: ExprKind::Convert {
                operand: Box::new(e),
                kind: Conversion::CharToStr,
            },
            ty: Ty::String,
            value,
            span,
        }
    }

    // ---- Assignment compatibility ----

    /// Converts an expression to type `target` (2.4). E0702 if incompatible. `range_check` is `{$R}`.
    /// `param` is the argument position and parameter name (for the message).
    pub(super) fn coerce(
        &mut self,
        e: Expr,
        target: Ty,
        range_check: bool,
        param: Option<(usize, &str)>,
    ) -> Expr {
        if e.ty.is_error() || target.is_error() || e.ty == target {
            return e;
        }
        let span = e.span;
        match (e.ty, target) {
            (s, t) if s.is_integer() && t.is_integer() => {
                if let Some(Const::Int(v)) = &e.value {
                    if !t.contains(*v) {
                        self.const_out_of_range(*v, t, span);
                        return err_expr(span);
                    }
                    let mut c = e;
                    c.ty = t;
                    return c;
                }
                let (lo, hi) = t.range().unwrap();
                let (slo, shi) = s.range().unwrap();
                if lo <= slo && shi <= hi {
                    // Conversion to a wider type: nothing to do
                    let mut w = e;
                    w.ty = t;
                    return w;
                }
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(e),
                        kind: Conversion::Int {
                            to: t,
                            check: range_check,
                        },
                    },
                    ty: t,
                    value: None,
                    span,
                }
            }
            (Ty::Char, Ty::String) => self.as_string_expr(e),
            _ => {
                let mut msg = format!("type mismatch: expected `{target}`, found `{}`", e.ty);
                if let Some((n, pname)) = param {
                    msg = format!("{msg} (argument {n}, parameter `{pname}`)");
                }
                self.error(Code::E(702), msg, span);
                err_expr(span)
            }
        }
    }

    // ---- Calls ----

    fn lower_call_expr(&mut self, c: &ast::CallExpr, span: TextRange) -> Expr {
        let Some(callee) = c.callee() else {
            return err_expr(span);
        };
        let args = c.args();
        let Some((r, name, range)) = self.resolve_name_expr(&callee) else {
            // An expression that cannot be called
            let callee_e = self.lower_expr(&callee);
            for a in &args {
                for x in a.exprs() {
                    self.lower_expr(&x);
                }
            }
            if !callee_e.ty.is_error() {
                let r = callee_e.span;
                self.error(Code::E(704), "expression is not a procedure or function", r);
            }
            return err_expr(span);
        };
        if c.has_error_arg() {
            // The argument list has a skipped range (`Chr(0x3042)`). A syntax diagnostic has been reported, so
            // only check the remaining arguments and do not report E0705 for the argument count (error type)
            self.allow_type_as_value = true;
            for a in &args {
                for x in a.exprs() {
                    self.lower_expr(&x);
                }
            }
            self.allow_type_as_value = false;
            return err_expr(span);
        }
        match r {
            Resolved::Routine(id) => self.lower_routine_call(id, &args, span),
            Resolved::Func(f) => self.lower_builtin_call(f, &name, &args, range, span),
            Resolved::Type(t) => self.lower_conversion(t, &args, range, span),
            Resolved::Proc(p) => {
                for a in &args {
                    for x in a.exprs() {
                        self.lower_expr(&x);
                    }
                }
                let pname = builtins::proc_name(p);
                self.error(
                    Code::E(725),
                    format!("procedure `{pname}` has no value"),
                    range,
                );
                err_expr(span)
            }
            Resolved::Var(_) | Resolved::Global(_) | Resolved::Const { .. } | Resolved::Bool(_) => {
                for a in &args {
                    for x in a.exprs() {
                        self.lower_expr(&x);
                    }
                }
                let d = Diagnostic::error(
                    Code::E(704),
                    format!("`{name}` is not a procedure or function"),
                    Stage::Sema,
                )
                .with_primary(self.span(range), "");
                let d = match &r {
                    Resolved::Var(id) => {
                        let s = self.var(*id).span;
                        d.with_secondary(self.span(s), "declared here")
                    }
                    Resolved::Global(id) => {
                        let s = self.globals[id.0 as usize].span;
                        d.with_secondary(self.span(s), "declared here")
                    }
                    Resolved::Const { span: s, .. } => {
                        d.with_secondary(self.span(*s), "declared here")
                    }
                    _ => d,
                };
                self.push(d);
                err_expr(span)
            }
            Resolved::Unit(_) => {
                self.error(
                    Code::E(708),
                    format!("`{name}` is a unit and cannot be used as a value"),
                    range,
                );
                err_expr(span)
            }
            Resolved::Error => {
                // The name has been diagnosed. Check the arguments, but allow type names (`Low(Integer)`)
                self.allow_type_as_value = true;
                for a in &args {
                    for x in a.exprs() {
                        self.lower_expr(&x);
                    }
                }
                self.allow_type_as_value = false;
                err_expr(span)
            }
        }
    }

    /// E0717 if an argument has a width specifier (`:`).
    fn check_no_width(&mut self, args: &[ast::Arg]) {
        for a in args {
            if a.colon_count() > 0 {
                let r = a.syntax().text_range();
                self.error(
                    Code::E(717),
                    "width specifier is only allowed in `Write` and `WriteLn`",
                    r,
                );
            }
        }
    }

    /// A call to a user routine (in an expression or a statement).
    pub(super) fn lower_routine_call(
        &mut self,
        id: RoutineId,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Expr {
        self.check_no_width(args);
        let params = self.routines[id.0 as usize].params.clone();
        let ret = self.routines[id.0 as usize].ret;
        let name = self.routines[id.0 as usize].name.clone();
        let name_span = self.routines[id.0 as usize].name_span;
        if args.len() != params.len() {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            let d = Diagnostic::error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {}, found {}",
                    params.len(),
                    args.len()
                ),
                Stage::Sema,
            )
            .with_primary(self.span(span), "")
            .with_secondary(self.span(name_span), "declared here");
            self.push(d);
            return err_expr(span);
        }
        let mut out: Vec<CallArg> = Vec::new();
        let mut failed = false;
        let checks = self.checks_at(span.start());
        for (i, (a, p)) in args.iter().zip(&params).enumerate() {
            let Some(v) = a.value() else {
                failed = true;
                continue;
            };
            match p.mode {
                ParamMode::Var | ParamMode::Out => {
                    let is_out = p.mode == ParamMode::Out;
                    match self.lower_place(&v, PlaceUse::RefArg { is_out }) {
                        Some((place, ty)) => {
                            if !ty.is_error() && !p.ty.is_error() && ty != p.ty {
                                let r = ast::trimmed_range(v.syntax());
                                let mode = if is_out { "out" } else { "var" };
                                let d = Diagnostic::error(
                                    Code::E(702),
                                    format!("type mismatch: expected `{}`, found `{ty}` (argument {}, parameter `{}`)", p.ty, i + 1, p.name),
                                    Stage::Sema,
                                )
                                .with_primary(self.span(r), "")
                                .with_note(format!("a `{mode}` parameter needs a variable of exactly the same type"));
                                self.push(d);
                                failed = true;
                            } else {
                                out.push(CallArg::Ref { place, is_out });
                            }
                        }
                        None => {
                            failed = true;
                        }
                    }
                    if let PlaceResult::NotVariable(r) =
                        self.last_place_result.take().unwrap_or(PlaceResult::Ok)
                    {
                        let mode = if is_out { "out" } else { "var" };
                        let d = Diagnostic::error(
                            Code::E(706),
                            format!(
                                "argument {} must be a variable (parameter `{}` is `{mode}`)",
                                i + 1,
                                p.name
                            ),
                            Stage::Sema,
                        )
                        .with_primary(self.span(r), "")
                        .with_secondary(self.span(p.span), "declared here");
                        self.push(d);
                    }
                }
                _ => {
                    let e = self.lower_expr(&v);
                    if e.ty.is_error() {
                        failed = true;
                        continue;
                    }
                    let e = self.coerce(e, p.ty, checks.range, Some((i + 1, &p.name)));
                    if e.ty.is_error() {
                        failed = true;
                    }
                    out.push(CallArg::Value(e));
                }
            }
        }
        if failed {
            return err_expr(span);
        }
        let _ = name;
        Expr {
            kind: ExprKind::Call {
                routine: id,
                args: out,
            },
            ty: ret.unwrap_or(Ty::Error),
            value: None,
            span,
        }
    }

    pub(super) fn lower_builtin_call_pub(
        &mut self,
        f: Builtin,
        name: &str,
        args: &[ast::Arg],
        name_range: TextRange,
        span: TextRange,
    ) -> Expr {
        self.lower_builtin_call(f, name, args, name_range, span)
    }

    pub(super) fn check_no_width_pub(&mut self, args: &[ast::Arg]) {
        self.check_no_width(args)
    }

    pub(super) fn int_calc_type_pub(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        op_range: TextRange,
    ) -> Option<Ty> {
        self.int_calc_type(lhs, rhs, op_range)
    }

    /// A call to a built-in function.
    fn lower_builtin_call(
        &mut self,
        f: Builtin,
        name: &str,
        args: &[ast::Arg],
        name_range: TextRange,
        span: TextRange,
    ) -> Expr {
        self.check_no_width(args);
        let (sig, ret) = builtin_signature(f);
        let shown = builtin_name(f);
        let _ = name;
        let lowered: Vec<Option<Expr>> = args
            .iter()
            .map(|a| a.value().map(|v| self.lower_expr(&v)))
            .collect();
        if lowered
            .iter()
            .any(|e| e.as_ref().is_none_or(|x| x.ty.is_error()))
        {
            return err_expr(span);
        }
        let mut lowered: Vec<Expr> = lowered.into_iter().map(|e| e.unwrap()).collect();
        if lowered.len() != sig.len() {
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {}, found {}",
                    sig.len(),
                    lowered.len()
                ),
                span,
            );
            return err_expr(span);
        }
        let checks = self.checks_at(span.start());
        match f {
            Builtin::Ord => {
                let x = lowered.pop().unwrap();
                match x.ty {
                    Ty::Char | Ty::Boolean => {
                        let value = x.value.as_ref().and_then(Const::as_int).map(Const::Int);
                        return Expr {
                            kind: ExprKind::Convert {
                                operand: Box::new(x),
                                kind: Conversion::OrdToInt,
                            },
                            ty: Ty::Integer,
                            value,
                            span,
                        };
                    }
                    t if t.is_integer() => {
                        let mut e = x;
                        e.span = span;
                        return e;
                    }
                    t => {
                        let r = x.span;
                        self.error(Code::E(702), format!("type mismatch: expected an ordinal type, found `{t}` (argument 1, parameter `X`)"), r);
                        return err_expr(span);
                    }
                }
            }
            Builtin::Abs => {
                let x = lowered.pop().unwrap();
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected an integer type, found `{}` (argument 1, parameter `X`)", x.ty), r);
                    return err_expr(span);
                }
                if let Some(Const::Int(v)) = &x.value {
                    // Like unary `-`, a constant re-derives its type from the value (`Abs(-2147483648)` is `Int64`;
                    // E0714 if it does not fit in `Int64`)
                    let a = v.abs();
                    if !Ty::Int64.contains(a) {
                        self.const_out_of_range(a, Ty::Int64, span);
                        return err_expr(span);
                    }
                    return const_expr(Const::Int(a), Ty::of_literal(a), span);
                }
                // The result type is the computation type of unary `-` (`Integer` for narrow types; unsigned types unchanged)
                let ty = if x.ty.fits_in_integer() {
                    Ty::Integer
                } else {
                    x.ty
                };
                return Expr {
                    kind: ExprKind::Builtin {
                        func: f,
                        args: vec![x],
                    },
                    ty,
                    value: None,
                    span,
                };
            }
            Builtin::Chr => {
                let x = lowered.pop().unwrap();
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected `Word`, found `{}` (argument 1, parameter `X`)", x.ty), r);
                    return err_expr(span);
                }
                if let Some(Const::Int(v)) = &x.value {
                    if !Ty::Char.contains(*v) {
                        self.const_out_of_range(*v, Ty::Char, x.span);
                        return err_expr(span);
                    }
                    return const_expr(Const::Char(*v as u16), Ty::Char, span);
                }
                return Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(x),
                        kind: Conversion::IntToChar {
                            check: checks.range,
                        },
                    },
                    ty: Ty::Char,
                    value: None,
                    span,
                };
            }
            Builtin::IntToStr => {
                let x = lowered.pop().unwrap();
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected `Int64`, found `{}` (argument 1, parameter `X`)", x.ty), r);
                    return err_expr(span);
                }
                let func = if x.ty == Ty::UInt64 {
                    Builtin::UIntToStr
                } else {
                    Builtin::IntToStr
                };
                let x = if x.ty == Ty::UInt64 {
                    x
                } else {
                    self.coerce(x, Ty::Int64, false, None)
                };
                return Expr {
                    kind: ExprKind::Builtin {
                        func,
                        args: vec![x],
                    },
                    ty: Ty::String,
                    value: None,
                    span,
                };
            }
            Builtin::Length => {
                let x = lowered.pop().unwrap();
                let x = self.coerce(x, Ty::String, false, Some((1, "S")));
                if x.ty.is_error() {
                    return err_expr(span);
                }
                if let Some(Const::Str(s)) = &x.value {
                    return const_expr(Const::Int(s.len() as i128), Ty::Integer, span);
                }
                return Expr {
                    kind: ExprKind::Builtin {
                        func: f,
                        args: vec![x],
                    },
                    ty: Ty::Integer,
                    value: None,
                    span,
                };
            }
            _ => {}
        }
        let mut out = Vec::new();
        for (i, (e, (pname, pty))) in lowered.into_iter().zip(sig.iter()).enumerate() {
            let e = self.coerce(e, *pty, checks.range, Some((i + 1, pname)));
            if e.ty.is_error() {
                return err_expr(span);
            }
            out.push(e);
        }
        let _ = shown;
        let _ = name_range;
        Expr {
            kind: ExprKind::Builtin { func: f, args: out },
            ty: ret,
            value: None,
            span,
        }
    }

    /// Explicit conversion `T(expr)` (2.5).
    fn lower_conversion(
        &mut self,
        t: Ty,
        args: &[ast::Arg],
        name_range: TextRange,
        span: TextRange,
    ) -> Expr {
        self.check_no_width(args);
        if args.len() != 1 {
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
            return err_expr(span);
        }
        let Some(v) = args[0].value() else {
            return err_expr(span);
        };
        let x = self.lower_expr(&v);
        if x.ty.is_error() || t.is_error() {
            return err_expr(span);
        }
        let checks = self.checks_at(span.start());
        let _ = name_range;
        if x.ty == t {
            let mut e = x;
            e.span = span;
            return e;
        }
        match (x.ty, t) {
            (s, t) if s.is_integer() && t.is_integer() => {
                if let Some(Const::Int(val)) = &x.value {
                    if !t.contains(*val) {
                        let val = *val;
                        let literal = is_literal_arg(&v);
                        self.const_out_of_range(val, t, x.span);
                        if literal {
                            let wrapped = t.wrap(val);
                            let text = if val < 0 || is_hex_literal(&v, self.input.bytes) {
                                // Hex at the width of the target type (the -1 from `Integer($FFFFFFFF)` is shown as `$FFFFFFFF`)
                                let bits = t.bits().unwrap_or(64);
                                let width = (bits / 4) as usize;
                                let masked = if wrapped < 0 {
                                    (wrapped as i64 as u64) & (u64::MAX >> (64 - bits))
                                } else {
                                    wrapped as u64
                                };
                                format!("${masked:0width$X}")
                            } else {
                                wrapped.to_string()
                            };
                            // When the hex notation would be the same as the original literal (`ShortInt($80)`), show it in decimal
                            let text = if text.eq_ignore_ascii_case(&self.src(x.span)) {
                                wrapped.to_string()
                            } else {
                                text
                            };
                            let fix = Fix::suggestion(
                                format!("replace with `{text}`"),
                                vec![TextEdit::replace(self.input.file, x.span, text)],
                            );
                            self.last_mut().fixes.push(fix);
                        }
                        return err_expr(span);
                    }
                    return const_expr(Const::Int(*val), t, span);
                }
                let (lo, hi) = t.range().unwrap();
                let (slo, shi) = s.range().unwrap();
                if lo <= slo && shi <= hi {
                    let mut w = x;
                    w.ty = t;
                    w.span = span;
                    return w;
                }
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(x),
                        kind: Conversion::Int {
                            to: t,
                            check: checks.range,
                        },
                    },
                    ty: t,
                    value: None,
                    span,
                }
            }
            (s, Ty::Char) if s.is_integer() => {
                if let Some(Const::Int(val)) = &x.value {
                    if !Ty::Char.contains(*val) {
                        self.const_out_of_range(*val, Ty::Char, x.span);
                        return err_expr(span);
                    }
                    return const_expr(Const::Char(*val as u16), Ty::Char, span);
                }
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(x),
                        kind: Conversion::IntToChar {
                            check: checks.range,
                        },
                    },
                    ty: Ty::Char,
                    value: None,
                    span,
                }
            }
            (s, Ty::Boolean) if s.is_integer() => {
                let value = match &x.value {
                    Some(Const::Int(v)) => Some(Const::Bool(*v != 0)),
                    _ => None,
                };
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(x),
                        kind: Conversion::IntToBool,
                    },
                    ty: Ty::Boolean,
                    value,
                    span,
                }
            }
            (Ty::Char | Ty::Boolean, t) if t.is_integer() => {
                let value = x.value.as_ref().and_then(Const::as_int).map(Const::Int);
                let inner = Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(x),
                        kind: Conversion::OrdToInt,
                    },
                    ty: Ty::Integer,
                    value: value.clone(),
                    span,
                };
                if t == Ty::Integer {
                    return inner;
                }
                if let Some(Const::Int(v)) = &value {
                    if !t.contains(*v) {
                        self.const_out_of_range(*v, t, span);
                        return err_expr(span);
                    }
                    return const_expr(Const::Int(*v), t, span);
                }
                let (lo, hi) = t.range().unwrap();
                if lo <= 0 && 65535 <= hi {
                    let mut w = inner;
                    w.ty = t;
                    return w;
                }
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(inner),
                        kind: Conversion::Int {
                            to: t,
                            check: checks.range,
                        },
                    },
                    ty: t,
                    value: None,
                    span,
                }
            }
            (Ty::Char, Ty::String) => {
                let mut e = self.as_string_expr(x);
                e.span = span;
                e
            }
            (s, t) => {
                self.error(
                    Code::E(724),
                    format!("a value of type `{s}` cannot be converted to `{t}`"),
                    span,
                );
                err_expr(span)
            }
        }
    }

    // ---- Assignment targets ----

    /// Resolves an expression as an assignment target. Returns `(place, type)`. On error, reports a diagnostic and returns `None`.
    pub(super) fn lower_place(&mut self, e: &ast::Expr, usage: PlaceUse) -> Option<(Place, Ty)> {
        self.last_place_result = None;
        let span = ast::trimmed_range(e.syntax());
        match e {
            ast::Expr::NameRef(_) | ast::Expr::Field(_) => {
                let (r, name, range) = self.resolve_name_expr(e)?;
                match r {
                    Resolved::Var(id) => {
                        let v = self.var(id).clone();
                        if let VarKind::Param(ParamMode::Const) = v.kind
                            && usage.is_write()
                        {
                            self.cannot_assign(span, "a `const` parameter", Some(v.span));
                            return None;
                        }
                        if self.cur.as_ref().is_some_and(|c| c.loop_vars.contains(&id))
                            && usage.is_write()
                        {
                            self.cannot_assign(span, "the loop variable", Some(v.span));
                            return None;
                        }
                        if usage.reads() {
                            self.mark_used(id, span);
                        }
                        Some((Place::Var(id, span), v.ty))
                    }
                    Resolved::Global(id) => {
                        if self
                            .cur
                            .as_ref()
                            .is_some_and(|c| c.loop_globals.contains(&id))
                            && usage.is_write()
                        {
                            let s = self.globals[id.0 as usize].span;
                            self.cannot_assign(span, "the loop variable", Some(s));
                            return None;
                        }
                        Some((Place::Global(id), self.globals[id.0 as usize].ty))
                    }
                    Resolved::Const { span: decl, .. } => {
                        if usage.wants_variable() {
                            // A constant is not a variable (E0706; `var` arguments, `Inc` / `Dec`, `ReadLn`)
                            self.last_place_result = Some(PlaceResult::NotVariable(span));
                            return None;
                        }
                        self.cannot_assign(span, "a constant", Some(decl));
                        None
                    }
                    Resolved::Error => None,
                    Resolved::Routine(id)
                        if usage == PlaceUse::Assign
                            && self
                                .cur
                                .as_ref()
                                .is_some_and(|c| c.routine == Some(id) && c.ret.is_some()) =>
                    {
                        // Assigning to the function name (`Square := X * X`) assigns to `Result` (4.3)
                        let c = self.cur.as_ref().unwrap();
                        let vid = VarId(
                            c.vars
                                .iter()
                                .take_while(|v| matches!(v.kind, VarKind::Param(_)))
                                .count() as u32,
                        );
                        let ty = self.var(vid).ty;
                        Some((Place::Var(vid, span), ty))
                    }
                    _ => {
                        if usage.wants_variable() {
                            self.last_place_result = Some(PlaceResult::NotVariable(span));
                            return None;
                        }
                        let _ = (name, range);
                        self.cannot_assign(span, "not a variable", None);
                        None
                    }
                }
            }
            ast::Expr::Paren(p) => {
                let inner = p.inner()?;
                self.lower_place(&inner, usage)
            }
            ast::Expr::Index(i) => {
                if usage.wants_variable() {
                    // A string element cannot be passed to a var / out parameter, nor be the target of `Inc` / `Dec` / `ReadLn` (M1; E0706)
                    let base = i.base()?;
                    let _ = self.lower_expr(&base);
                    for ix in i.indices() {
                        self.lower_expr(&ix);
                    }
                    self.last_place_result = Some(PlaceResult::NotVariable(span));
                    return None;
                }
                let base = i.base()?;
                let indices = i.indices();
                let (place, ty) = self.lower_place(&base, PlaceUse::IndexBase)?;
                if ty.is_error() {
                    return None;
                }
                if indices.len() != 1 {
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of indices: expected 1, found {}",
                            indices.len()
                        ),
                        span,
                    );
                    return None;
                }
                if ty != Ty::String {
                    self.error(
                        Code::E(720),
                        format!("cannot index a value of type `{ty}`"),
                        span,
                    );
                    return None;
                }
                let idx = self.lower_expr(&indices[0]);
                if idx.ty.is_error() {
                    return None;
                }
                if !idx.ty.is_integer() {
                    let r = idx.span;
                    self.error(
                        Code::E(719),
                        format!(
                            "index type mismatch: expected `Integer`, found `{}`",
                            idx.ty
                        ),
                        r,
                    );
                    return None;
                }
                let bracket = i.l_brack().map(|t| t.text_range()).unwrap_or(span);
                Some((
                    Place::StrIndex {
                        base: Box::new(place),
                        index: Box::new(idx),
                        span: bracket,
                    },
                    Ty::Char,
                ))
            }
            ast::Expr::Error(_) => None,
            _ => {
                let x = self.lower_expr(e);
                if x.ty.is_error() {
                    return None;
                }
                if usage.wants_variable() {
                    self.last_place_result = Some(PlaceResult::NotVariable(span));
                    return None;
                }
                self.cannot_assign(span, "not a variable", None);
                None
            }
        }
    }

    pub(super) fn cannot_assign(
        &mut self,
        span: TextRange,
        label: &'static str,
        decl: Option<TextRange>,
    ) {
        let text = self.src(span);
        let mut d = Diagnostic::error(
            Code::E(707),
            format!("cannot assign to `{text}`"),
            Stage::Sema,
        )
        .with_primary(self.span(span), label);
        if let Some(s) = decl {
            d = d.with_secondary(self.span(s), "declared here");
        }
        self.push(d);
    }
}

/// How an assignment target is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlaceUse {
    /// Left-hand side of `:=` (the `S` in `S[I] := C` is read).
    Assign,
    /// Actual argument for a `var` / `out` parameter.
    RefArg { is_out: bool },
    /// Target of `Inc` / `Dec` (read and written).
    IncDec,
    /// Argument of `ReadLn` (written only).
    ReadLn,
    /// Target of indexing (the `S` in `S[I] := …`; read and written).
    IndexBase,
}

impl PlaceUse {
    fn is_write(self) -> bool {
        true
    }

    /// A non-variable is E0706 (reported by the caller).
    fn wants_variable(self) -> bool {
        matches!(
            self,
            PlaceUse::RefArg { .. } | PlaceUse::IncDec | PlaceUse::ReadLn
        )
    }

    fn reads(self) -> bool {
        matches!(
            self,
            PlaceUse::RefArg { is_out: false } | PlaceUse::IncDec | PlaceUse::IndexBase
        )
    }
}

/// When `lower_place` finds a non-variable for `RefArg`.
pub(super) enum PlaceResult {
    Ok,
    NotVariable(TextRange),
}

fn binop_of(kind: SyntaxKind) -> Option<BinOp> {
    Some(match kind {
        SyntaxKind::PLUS => BinOp::Add,
        SyntaxKind::MINUS => BinOp::Sub,
        SyntaxKind::STAR => BinOp::Mul,
        SyntaxKind::DIV_KW => BinOp::Div,
        SyntaxKind::MOD_KW => BinOp::Mod,
        SyntaxKind::AND_KW => BinOp::And,
        SyntaxKind::OR_KW => BinOp::Or,
        SyntaxKind::XOR_KW => BinOp::Xor,
        SyntaxKind::SHL_KW => BinOp::Shl,
        SyntaxKind::SHR_KW => BinOp::Shr,
        SyntaxKind::EQ => BinOp::Eq,
        SyntaxKind::NE => BinOp::Ne,
        SyntaxKind::LT => BinOp::Lt,
        SyntaxKind::LE => BinOp::Le,
        SyntaxKind::GT => BinOp::Gt,
        SyntaxKind::GE => BinOp::Ge,
        _ => return None,
    })
}

fn compare(op: BinOp, ord: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match op {
        BinOp::Eq => ord == Equal,
        BinOp::Ne => ord != Equal,
        BinOp::Lt => ord == Less,
        BinOp::Le => ord != Greater,
        BinOp::Gt => ord == Greater,
        BinOp::Ge => ord != Less,
        _ => false,
    }
}

/// String comparison (by Unicode code point order; surrogate pairs are combined).
pub fn compare_strings(a: &[u16], b: &[u16]) -> std::cmp::Ordering {
    let ia = char::decode_utf16(a.iter().copied()).map(|r| {
        r.map(|c| c as u32)
            .unwrap_or_else(|e| e.unpaired_surrogate() as u32)
    });
    let ib = char::decode_utf16(b.iter().copied()).map(|r| {
        r.map(|c| c as u32)
            .unwrap_or_else(|e| e.unpaired_surrogate() as u32)
    });
    ia.cmp(ib)
}

fn is_paren(e: &ast::BinaryExpr) -> bool {
    // BINARY_EXPR contains no parentheses (parentheses are PAREN_EXPR)
    let _ = e;
    false
}

/// Whether the argument is a literal (including with a unary `-`).
fn is_literal_arg(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Literal(_) => true,
        ast::Expr::Unary(u) => {
            u.op().is_some_and(|t| t.kind() == SyntaxKind::MINUS)
                && u.operand()
                    .is_some_and(|o| matches!(o, ast::Expr::Literal(_)))
        }
        ast::Expr::Paren(p) => p.inner().is_some_and(|i| is_literal_arg(&i)),
        _ => false,
    }
}

fn is_hex_literal(e: &ast::Expr, bytes: &[u8]) -> bool {
    let r = ast::trimmed_range(e.syntax());
    bytes[usize::from(r.start())..usize::from(r.end())].contains(&b'$')
}
