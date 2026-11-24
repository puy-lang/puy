//! The subexpressions of an `Assert` condition whose values a failure records
//! (`docs/spec/semantics.md` 5.4): the rewrite of the condition under `{$C+}` that wraps each in
//! an [`ExprKind::Probe`], and the right sides of `and` / `or` holding one in a mark.

use super::hir::*;
use super::types::Ty;
use crate::source::TextRange;
use std::collections::HashSet;

/// What the rewrite asks of the analysis about a subexpression.
pub(super) trait ProbeSource {
    /// The source text of `range` (each run of trivia as one space; empty when the range is not
    /// in the condition's text).
    fn text(&self, range: TextRange) -> String;
    /// Whether the variable `v` read as `text` is written in the source: not a hidden variable,
    /// and not the implicit `Self` of a member read without `Self.`.
    fn var_written(&self, v: VarId, text: &str) -> bool;
}

/// Wraps the subexpressions of `cond` whose values a failure records, and returns them in the
/// order they are shown (the probe numbers of the wrapped nodes) and the number of marks.
pub(super) fn rewrite(cond: &mut Expr, src: &impl ProbeSource) -> (Vec<AssertProbe>, u32) {
    let mut w = Rewrite {
        src,
        seen: HashSet::new(),
        found: Vec::new(),
        marks: 0,
    };
    // The condition itself is not shown: it is known to be false
    w.children(cond, None);
    // Shown from the left of the source, an outer expression before the ones inside it
    let mut order: Vec<usize> = (0..w.found.len()).collect();
    order.sort_by_key(|&i| {
        let r = w.found[i].range;
        (r.start(), std::cmp::Reverse(r.end()))
    });
    let mut number = vec![0u32; order.len()];
    for (k, &i) in order.iter().enumerate() {
        number[i] = k as u32;
    }
    // The marks the probes are under, numbered densely in order of first use
    let mut mark_number: Vec<Option<u32>> = vec![None; w.marks as usize];
    let mut marks = 0;
    let mut probes = Vec::with_capacity(order.len());
    for &i in &order {
        let f = &w.found[i];
        let mark = f.mark.map(|m| {
            *mark_number[m as usize].get_or_insert_with(|| {
                marks += 1;
                marks - 1
            })
        });
        probes.push(AssertProbe {
            text: f.text.clone(),
            ty: f.ty,
            mark,
        });
    }
    renumber(cond, &number, &mut mark_number, &mut marks);
    (probes, marks)
}

/// A subexpression found: its source range and text, its type, and the provisional number of
/// the innermost mark it is under.
struct Found {
    range: TextRange,
    text: String,
    ty: Ty,
    mark: Option<u32>,
}

struct Rewrite<'a, S> {
    src: &'a S,
    /// The texts already shown (a text is shown once).
    seen: HashSet<String>,
    found: Vec<Found>,
    /// The provisional marks handed out.
    marks: u32,
}

impl<S: ProbeSource> Rewrite<'_, S> {
    /// Wraps `e` if a failure records its value, then its parts. `whole` is false for an
    /// operand whose own value is not recorded (`Part`).
    fn expr(&mut self, e: &mut Expr, mark: Option<u32>) {
        self.part(e, mark, true);
    }

    fn part(&mut self, e: &mut Expr, mark: Option<u32>, whole: bool) {
        let shown = if whole { self.shown(e) } else { None };
        self.children(e, mark);
        if let Some(text) = shown {
            let k = self.found.len() as u32;
            self.found.push(Found {
                range: e.span,
                text,
                ty: e.ty,
                mark,
            });
            wrap(e, Probe::Value(k));
        }
    }

    /// The text of `e` when a failure records its value (and claims the text).
    fn shown(&mut self, e: &Expr) -> Option<String> {
        if e.value.is_some() || !shown_type(e.ty) {
            return None;
        }
        let text = self.src.text(e.span);
        let ok = match &e.kind {
            ExprKind::Var(v) => self.src.var_written(*v, &text),
            ExprKind::Global(_)
            | ExprKind::Field { .. }
            | ExprKind::ObjField { .. }
            | ExprKind::Elem { .. }
            | ExprKind::StrIndex { .. }
            | ExprKind::Slice { .. }
            | ExprKind::Call { .. }
            | ExprKind::CallValue { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Unary { .. }
            | ExprKind::ClassName { .. }
            | ExprKind::ExcMember { .. }
            | ExprKind::ClassCheck { .. }
            | ExprKind::ClassInfo { .. }
            | ExprKind::Supports { .. }
            | ExprKind::TryStrToInt { .. }
            | ExprKind::Weak(_) => true,
            // The built-in functions the code generator reads as a shape stay as they are
            ExprKind::Builtin { func, .. } => {
                !matches!(func, Builtin::IsPlainKey | Builtin::IsPlainOrder)
            }
            // An implicit conversion has the range of its operand, whose value is shown
            ExprKind::Convert { operand, .. } => operand.span != e.span,
            _ => false,
        };
        (ok && !text.is_empty() && self.seen.insert(text.clone())).then_some(text)
    }

    /// The parts of `e`, each under `mark` (the right sides of `and` / `or` under marks of
    /// their own).
    fn children(&mut self, e: &mut Expr, mark: Option<u32>) {
        if let ExprKind::Binary { .. } = e.kind {
            self.split_prefix(e);
        }
        match &mut e.kind {
            ExprKind::Binary { first, steps } => {
                // The operands of `and` / `or` that are themselves comparisons or logical
                // operations are not recorded (their operands tell their values), so that the
                // condition still branches on the comparison
                let first_lazy = steps.first().is_some_and(lazy);
                let whole = !(first_lazy && logical(first));
                self.part(first, mark, whole);
                for s in steps {
                    if lazy(s) {
                        let m = self.marks;
                        self.marks += 1;
                        let before = self.found.len();
                        let whole = !logical(&s.rhs);
                        self.part(&mut s.rhs, Some(m), whole);
                        if self.found.len() > before {
                            wrap(&mut s.rhs, Probe::Reached(m));
                        }
                    } else {
                        self.expr(&mut s.rhs, mark);
                    }
                }
            }
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
                calc: Calc::Bool,
            } => {
                let whole = !logical(operand);
                self.part(operand, mark, whole);
            }
            // The variable, field or element holding the record or static array a field or
            // element is read from is not recorded (the field or element is): reading it whole
            // would copy it
            ExprKind::Field { base, .. } => {
                let whole = !place(base);
                self.part(base, mark, whole);
            }
            ExprKind::Elem { base, index } => {
                let whole = !(matches!(base.ty, Ty::Array(_)) && place(base));
                self.part(base, mark, whole);
                self.expr(index, mark);
            }
            // A probe's operand is the expression (not met: the rewrite runs once)
            ExprKind::Probe { operand, .. } => self.children(operand, mark),
            kind => for_each_operand(kind, &mut |x| self.expr(x, mark)),
        }
    }

    /// Splits off the left operand of the last step of a fold when it is the result of an
    /// operation shown (`A + B` in `A + B = C`): the fold becomes that operand as a fold of its
    /// own, followed by the last step.
    fn split_prefix(&mut self, e: &mut Expr) {
        let ExprKind::Binary { first, steps } = &mut e.kind else {
            return;
        };
        let n = steps.len();
        if n < 2 || lazy(&steps[n - 2]) {
            return;
        }
        let constant =
            first.value.is_some() && steps[..n - 1].iter().all(|s| s.rhs.value.is_some());
        let range = TextRange::new(first.span.start(), steps[n - 2].rhs.span.end());
        if constant || self.src.text(range).is_empty() {
            return;
        }
        let last = steps.pop().expect("at least two steps");
        let prefix_ty = steps.last().expect("at least one step left").ty;
        let prefix = Expr {
            kind: ExprKind::Binary {
                first: first.clone(),
                steps: std::mem::take(steps),
            },
            ty: prefix_ty,
            value: None,
            span: range,
        };
        **first = prefix;
        steps.push(last);
    }
}

/// Whether `e` reads a variable, a field or an element (not a value made by a call).
fn place(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Var(_)
            | ExprKind::Global(_)
            | ExprKind::Field { .. }
            | ExprKind::Elem { .. }
            | ExprKind::ObjField { .. }
    )
}

/// Whether `e` is a comparison or a logical operation (`and`, `or`, `not`) of `Boolean`s.
fn logical(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Binary { steps, .. } => steps
            .last()
            .is_some_and(|s| s.op.is_comparison() || lazy(s)),
        ExprKind::Unary {
            op: UnOp::Not,
            calc: Calc::Bool,
            ..
        } => true,
        _ => false,
    }
}

/// Whether the step is the short-circuit `and` / `or`, whose right side may not be evaluated.
fn lazy(s: &BinStep) -> bool {
    s.calc == Calc::Bool && matches!(s.op, BinOp::And | BinOp::Or)
}

/// Whether a value of `ty` can be shown.
fn shown_type(ty: Ty) -> bool {
    !matches!(ty, Ty::Error | Ty::Helper(_) | Ty::Param(_) | Ty::TypeInfo)
}

/// Wraps `e` in a probe.
fn wrap(e: &mut Expr, probe: Probe) {
    let inner = std::mem::replace(
        e,
        Expr {
            kind: ExprKind::Error,
            ty: e.ty,
            value: None,
            span: e.span,
        },
    );
    *e = Expr {
        ty: inner.ty,
        span: inner.span,
        value: None,
        kind: ExprKind::Probe {
            operand: Box::new(inner),
            probe,
        },
    };
}

/// Renumbers the probes of `e` to the order they are shown and its marks to `mark_number` (a
/// mark no probe is under gets the next number).
fn renumber(e: &mut Expr, number: &[u32], mark_number: &mut [Option<u32>], marks: &mut u32) {
    if let ExprKind::Probe { probe, .. } = &mut e.kind {
        *probe = match *probe {
            Probe::Value(k) => Probe::Value(number[k as usize]),
            Probe::Reached(m) => {
                Probe::Reached(*mark_number[m as usize].get_or_insert_with(|| {
                    *marks += 1;
                    *marks - 1
                }))
            }
        };
    }
    match &mut e.kind {
        ExprKind::Binary { first, steps } => {
            renumber(first, number, mark_number, marks);
            for s in steps {
                renumber(&mut s.rhs, number, mark_number, marks);
            }
        }
        ExprKind::Probe { operand, .. } => renumber(operand, number, mark_number, marks),
        kind => for_each_operand(kind, &mut |x| renumber(x, number, mark_number, marks)),
    }
}

/// Calls `f` on each operand of an expression that is evaluated as a value (not the places of
/// `var` arguments, not the operands of a fold).
fn for_each_operand(kind: &mut ExprKind, f: &mut dyn FnMut(&mut Expr)) {
    let args = |args: &mut [CallArg], f: &mut dyn FnMut(&mut Expr)| {
        for a in args {
            if let CallArg::Value(e) | CallArg::Temp(e) = a {
                f(e);
            }
        }
    };
    match kind {
        ExprKind::Binary { first, steps } => {
            f(first);
            for s in steps {
                f(&mut s.rhs);
            }
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Convert { operand, .. }
        | ExprKind::ClassCheck { operand, .. }
        | ExprKind::Supports { operand, .. }
        | ExprKind::Probe { operand, .. } => f(operand),
        ExprKind::StrIndex { base, index } | ExprKind::Elem { base, index } => {
            f(base);
            f(index);
        }
        ExprKind::Field { base, .. }
        | ExprKind::ObjField { base, .. }
        | ExprKind::ClassName { base }
        | ExprKind::ExcMember { base, .. }
        | ExprKind::Weak(base) => f(base),
        ExprKind::Slice { base, low, high } => {
            f(base);
            for b in [low, high].into_iter().flatten() {
                f(b);
            }
        }
        ExprKind::Call { args: a, .. } => args(a, f),
        ExprKind::CallValue { callee, args: a } => {
            f(callee);
            args(a, f);
        }
        ExprKind::Builtin { args: a, .. } | ExprKind::ClassInfo { args: a, .. } => {
            for x in a {
                f(x);
            }
        }
        ExprKind::TryStrToInt { s, .. } => f(s),
        ExprKind::ArrayCtor(items) => {
            for x in items {
                f(x);
            }
        }
        ExprKind::SetCtor(items) => {
            for i in items {
                f(&mut i.low);
                if let Some(h) = &mut i.high {
                    f(h);
                }
            }
        }
        // Not shown inside: keys (the standard library's), constructions, values of routines
        _ => {}
    }
}
