//! `overload` and default parameter values (`docs/spec/semantics.md` 4.7, 4.8): the checks of the
//! declarations and the resolution of a call among the overloads of a name.

use super::expr::{PlaceResult, PlaceUse, err_expr};
use super::hir::{Expr, RoutineId};
use super::types::Ty;
use super::{GSym, LSym, ParamInfo, Sema, const_init_value};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{TextRange, TextSize};
use crate::syntax::ast::{self, ParamMode};

/// The routine a call resolves to.
pub(super) enum Pick {
    Routine(RoutineId),
    /// A diagnostic was reported (or an argument has an error type and the count does not decide).
    Failed,
}

/// What resolution knows of an argument before a candidate is chosen.
#[derive(Clone, Copy)]
enum ArgKind {
    /// The type of the expression by itself, and whether it is a constant.
    Typed(Ty, bool),
    /// `nil` or `[…]`: the type comes from the parameter (stage 0 when it fits).
    Contextual(&'static str),
    /// The argument has an error (no resolution diagnostic; `docs/spec/semantics.md` 4.7).
    Error,
}

/// Whether two parameter lists are the same for overloading: the count, the types and the modes,
/// with value and `const` counted as the same (`docs/spec/semantics.md` 4.7).
fn same_params(a: &[ParamInfo], b: &[ParamInfo]) -> bool {
    let by_ref = |m: ParamMode| match m {
        ParamMode::Value | ParamMode::Const => None,
        m => Some(m),
    };
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.ty == y.ty && by_ref(x.mode) == by_ref(y.mode))
}

/// The argument expression without parentheses around it.
fn strip_parens(e: &ast::Expr) -> ast::Expr {
    let mut cur = e.clone();
    while let ast::Expr::Paren(p) = &cur {
        match p.inner() {
            Some(inner) => cur = inner,
            None => break,
        }
    }
    cur
}

impl<'a, 's> Sema<'a, 's> {
    /// The routines declared with the name `lower` in the innermost scope declaring the name: a
    /// routine's own or an enclosing routine's local scope (nested routines;
    /// `docs/spec/semantics.md` 4.9), or the global scope. In declaration order.
    pub(super) fn overload_set(&self, lower: &str) -> Vec<RoutineId> {
        if let Some((up, _)) = self.lookup_scopes(lower) {
            return self
                .ctx_at(up)
                .scopes
                .iter()
                .flat_map(|s| s.iter())
                .filter(|(n, _)| n == lower)
                .filter_map(|(_, s)| match s {
                    LSym::Routine(id) => Some(*id),
                    _ => None,
                })
                .collect();
        }
        self.global_scope
            .iter()
            .filter(|(n, _)| n == lower)
            .filter_map(|(_, s)| match s {
                GSym::Routine(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    /// Whether the name is declared by a routine enclosing the one whose declarations are being
    /// analyzed: a nested routine of that name is E0701, not an overload or the body of a forward
    /// declaration (`docs/spec/semantics.md` 4.9).
    fn declared_outside(&self, lower: &str) -> bool {
        self.cur.is_some() && self.lookup_scopes(lower).is_some_and(|(up, _)| up > 0)
    }

    /// The default value of a parameter group (`docs/spec/semantics.md` 4.8): a constant
    /// expression converted to the parameter type. An erroneous one is an error-typed expression.
    pub(super) fn param_default(&mut self, group: &ast::ParamGroup, ty: Ty, names: usize) -> Expr {
        let node = group.default_node().expect("the caller checks the node");
        let value = group.default_value();
        let range = value.as_ref().map_or_else(
            || ast::trimmed_range(&node),
            |v| ast::trimmed_range(v.syntax()),
        );
        let fail = err_expr(range);
        if matches!(group.mode(), ParamMode::Var | ParamMode::Out) {
            self.error(
                Code::E(738),
                "a `var` or `out` parameter cannot have a default value",
                range,
            );
            return fail;
        }
        if names > 1 {
            self.error(
                Code::E(738),
                "only one parameter can be declared with a default value",
                range,
            );
            return fail;
        }
        if matches!(
            ty,
            Ty::Record(_) | Ty::Array(_) | Ty::DynArray(_) | Ty::OpenArray(_)
        ) {
            let t = self.tn(ty);
            self.error(
                Code::E(738),
                format!("a parameter of type `{t}` cannot have a default value"),
                range,
            );
            return fail;
        }
        let Some(value) = value else {
            // The parser reported E0403
            return fail;
        };
        if ty.is_error() {
            self.lower_expr_unchecked_ctor(&value);
            return fail;
        }
        let e = self.lower_init(&value, ty, true);
        if e.ty.is_error() {
            return fail;
        }
        if const_init_value(&e).is_none() {
            self.error(Code::E(713), "constant expression required", e.span);
            return fail;
        }
        e
    }

    /// The forward declaration (or body-less heading) a new heading completes: one with the same
    /// parameter list among the routines of the name. With a single routine of the name and no
    /// `overload` on both, the heading completes it whatever its parameters (the mismatch is
    /// E0723, as before `overload`).
    pub(super) fn forward_to_complete(
        &self,
        lower: &str,
        params: &[ParamInfo],
        has_forward: bool,
        has_overload: bool,
    ) -> Option<RoutineId> {
        if has_forward || self.declared_outside(lower) {
            return None;
        }
        let set = self.overload_set(lower);
        // A local declaration of the name hides the routines (only a forward routine of the same
        // scope can be completed)
        if self.find_declared(lower) != set.last().map(|id| self.routines[id.0 as usize].name_span)
        {
            return None;
        }
        let open = |id: &RoutineId| {
            let r = &self.routines[id.0 as usize];
            !r.has_body && r.forward_span.is_some()
        };
        if let Some(id) = set
            .iter()
            .copied()
            .filter(open)
            .find(|id| same_params(&self.routines[id.0 as usize].params, params))
        {
            return Some(id);
        }
        match set.as_slice() {
            [id] if open(id) && !(has_overload && self.routines[id.0 as usize].overload) => {
                Some(*id)
            }
            _ => None,
        }
    }

    /// E0723 when a body's heading differs from its forward declaration: the parameter names,
    /// types and modes and the return type must be the same, and a default value may be left out
    /// or repeated (`docs/spec/semantics.md` 4.1, 4.8).
    pub(super) fn check_body_matches_forward(
        &mut self,
        id: RoutineId,
        name_text: &str,
        name_range: TextRange,
        params: &[ParamInfo],
        ret: Option<Ty>,
    ) {
        let info = &self.routines[id.0 as usize];
        let fwd = info.forward_span.expect("a forward routine");
        let same = info.params.len() == params.len()
            && info.params.iter().zip(params).all(|(a, b)| {
                a.ty == b.ty && a.mode == b.mode && a.name.eq_ignore_ascii_case(&b.name)
            })
            && info.ret == ret;
        let msg = if !same {
            Some(if info.interface {
                format!(
                    "body of `{name_text}` does not match its declaration in the interface section"
                )
            } else {
                format!("body of `{name_text}` does not match its forward declaration")
            })
        } else {
            info.params
                .iter()
                .zip(params)
                .find(|(a, b)| {
                    b.default.as_ref().is_some_and(|bd| {
                        !bd.ty.is_error()
                            && a.default.as_ref().is_none_or(|ad| {
                                !ad.ty.is_error() && const_init_value(ad) != const_init_value(bd)
                            })
                    })
                })
                .map(|(_, b)| {
                    format!(
                        "default value of parameter `{}` does not match the forward declaration of `{name_text}`",
                        b.name
                    )
                })
        };
        if let Some(msg) = msg {
            let d = Diagnostic::error(Code::E(723), msg, Stage::Sema)
                .with_primary(self.span(name_range), "")
                .with_secondary(self.span(fwd), "declared here");
            self.push(d);
        }
    }

    /// Whether a new routine heading cannot be declared (reports E0701): the name is declared as
    /// something else, or as routines and not every declaration has `overload`, or an overload
    /// with the same parameter list exists (`docs/spec/semantics.md` 4.7).
    pub(super) fn check_routine_redeclared(
        &mut self,
        lower: &str,
        name_range: TextRange,
        params: &[ParamInfo],
        has_overload: bool,
        own_fix_at: Option<TextSize>,
    ) -> bool {
        if self.declared_outside(lower) {
            return self.check_duplicate(lower, name_range);
        }
        let set = self.overload_set(lower);
        let prev = self.find_declared(lower);
        let Some(last) = set.last() else {
            return self.check_duplicate(lower, name_range);
        };
        if prev != Some(self.routines[last.0 as usize].name_span) {
            return self.check_duplicate(lower, name_range);
        }
        let name = self.src(name_range);
        let same = set
            .iter()
            .copied()
            .find(|id| same_params(&self.routines[id.0 as usize].params, params));
        let all_overload =
            has_overload && set.iter().all(|id| self.routines[id.0 as usize].overload);
        if !all_overload {
            let first = self.routines[set[0].0 as usize].name_span;
            let mut d = Diagnostic::error(
                Code::E(701),
                format!("`{name}` is already declared"),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "declared again here")
            .with_secondary(self.span(first), "declared here");
            // Adding `overload` helps only when the parameter lists differ
            if same.is_none() {
                let mut at: Vec<_> = set
                    .iter()
                    .filter_map(|id| self.routines[id.0 as usize].overload_fix_at)
                    .collect();
                at.extend(own_fix_at);
                let edits = at
                    .into_iter()
                    .map(|p| TextEdit::insert(self.input.file, p, " overload;"))
                    .collect();
                d = d.with_fix(Fix::safe("add `overload`", edits));
            }
            self.push(d);
            return true;
        }
        if let Some(id) = same {
            let prev = self.routines[id.0 as usize].name_span;
            let d = Diagnostic::error(
                Code::E(701),
                format!("overloads of `{name}` have the same parameters"),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "declared again here")
            .with_secondary(self.span(prev), "declared here");
            self.push(d);
            return true;
        }
        false
    }

    /// Resolves a call of the routine `id` among the overloads of its name
    /// (`docs/spec/semantics.md` 4.7). A name with one declaration resolves to it (the call reports
    /// E0705 / E0702 as before). `bare` is a name without parentheses (no arguments); when no
    /// overload takes no arguments it resolves to `id` (the caller reports E0704). `span` is where
    /// E0734 / E0735 point.
    pub(super) fn pick_overload(
        &mut self,
        id: RoutineId,
        args: &[ast::Arg],
        span: TextRange,
        bare: bool,
    ) -> Pick {
        match self.overload_picks.get(&span) {
            Some(Some(c)) => return Pick::Routine(*c),
            // A failure seen in a dry run is reported again outside it
            Some(None) if self.dry_run > 0 => return Pick::Failed,
            _ => {}
        }
        let pick = self.pick_overload_uncached(id, args, span, bare);
        match pick {
            Pick::Routine(c) => {
                self.overload_picks.insert(span, Some(c));
            }
            Pick::Failed if self.dry_run > 0 => {
                self.overload_picks.insert(span, None);
            }
            Pick::Failed => {}
        }
        pick
    }

    fn pick_overload_uncached(
        &mut self,
        id: RoutineId,
        args: &[ast::Arg],
        span: TextRange,
        bare: bool,
    ) -> Pick {
        let lower = self.routines[id.0 as usize].name.to_ascii_lowercase();
        let unit = self.routines[id.0 as usize].unit;
        let set = if unit == self.unit_index {
            self.overload_set(&lower)
        } else {
            // A routine of another unit: the overloads its `interface` section declares
            self.units[unit as usize]
                .exports
                .iter()
                .filter(|(n, _)| *n == lower)
                .filter_map(|(_, s)| match s {
                    GSym::Routine(id) => Some(*id),
                    _ => None,
                })
                .collect()
        };
        if set.len() <= 1 {
            return Pick::Routine(id);
        }
        let n = args.len();
        let by_count: Vec<RoutineId> = set
            .iter()
            .copied()
            .filter(|c| {
                let r = &self.routines[c.0 as usize];
                r.min_args() <= n && n <= r.params.len()
            })
            .collect();
        if bare && by_count.is_empty() {
            return Pick::Routine(id);
        }
        let kinds: Vec<ArgKind> = args.iter().map(|a| self.arg_kind(a)).collect();
        if kinds.iter().any(|k| matches!(k, ArgKind::Error)) {
            if let [only] = by_count.as_slice() {
                return Pick::Routine(*only);
            }
            self.lower_args_quietly(args);
            return Pick::Failed;
        }
        let mut viable: Vec<(RoutineId, Vec<u8>)> = Vec::new();
        for c in by_count {
            if let Some(stages) = self.stages(c, args, &kinds) {
                viable.push((c, stages));
            }
        }
        if viable.is_empty() {
            // The arguments are typed once for real, so their variables count as read (W0001, E2000)
            self.lower_args_quietly(args);
            self.no_overload_matches(&set, &kinds, span);
            return Pick::Failed;
        }
        let dominates = |a: &[u8], b: &[u8]| {
            a.iter().zip(b).all(|(x, y)| x <= y) && a.iter().zip(b).any(|(x, y)| x < y)
        };
        let best: Vec<RoutineId> = viable
            .iter()
            .filter(|(_, s)| !viable.iter().any(|(_, o)| dominates(o, s)))
            .map(|(c, _)| *c)
            .collect();
        if let [only] = best.as_slice() {
            return Pick::Routine(*only);
        }
        self.lower_args_quietly(args);
        let name = self.routines[id.0 as usize].name.clone();
        let mut d = Diagnostic::error(
            Code::E(734),
            format!("ambiguous call to overloaded `{name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(span), "");
        for c in best {
            d = d.with_secondary(self.routine_decl_span(c), "candidate");
        }
        self.push(d);
        Pick::Failed
    }

    /// E0735: no overload takes the arguments.
    fn no_overload_matches(&mut self, set: &[RoutineId], kinds: &[ArgKind], span: TextRange) {
        let name = self.routines[set[0].0 as usize].name.clone();
        let mut d = Diagnostic::error(
            Code::E(735),
            format!("no overload of `{name}` matches these arguments"),
            Stage::Sema,
        )
        .with_primary(self.span(span), "");
        for c in set {
            d = d.with_secondary(self.routine_decl_span(*c), "candidate");
        }
        let note = if kinds.is_empty() {
            "the call has no arguments".to_string()
        } else {
            let types: Vec<String> = kinds
                .iter()
                .map(|k| match k {
                    ArgKind::Typed(t, _) => format!("`{}`", self.tn(*t)),
                    ArgKind::Contextual(s) => format!("`{s}`"),
                    ArgKind::Error => "`<error>`".to_string(),
                })
                .collect();
            format!("the arguments have types {}", types.join(", "))
        };
        self.push(d.with_note(note));
    }

    /// The kind of an argument, from typing it on its own without reporting anything.
    fn arg_kind(&mut self, a: &ast::Arg) -> ArgKind {
        let Some(v) = a.value() else {
            return ArgKind::Error;
        };
        match strip_parens(&v) {
            ast::Expr::Nil(_) => ArgKind::Contextual("nil"),
            ast::Expr::ArrayCtor(ac) => {
                let n = self.sink.len();
                let failed = self.dry_run(|s| {
                    s.lower_ctor_elements_quietly(&ac);
                    s.sink.iter().skip(n).any(|d| d.is_error())
                });
                if failed {
                    ArgKind::Error
                } else {
                    ArgKind::Contextual("[…]")
                }
            }
            _ => {
                let e = self.dry_run(|s| s.lower_expr(&v));
                if e.ty.is_error() {
                    ArgKind::Error
                } else {
                    ArgKind::Typed(e.ty, e.value.is_some())
                }
            }
        }
    }

    /// Types the arguments of a call that does not resolve, for their own diagnostics.
    fn lower_args_quietly(&mut self, args: &[ast::Arg]) {
        for a in args {
            if let Some(v) = a.value()
                && !matches!(strip_parens(&v), ast::Expr::Nil(_))
            {
                self.lower_expr_unchecked_ctor(&v);
            }
        }
    }

    /// The conversion stage of each argument passed to the candidate `c`, or `None` when an
    /// argument cannot be passed (`docs/spec/semantics.md` 4.7). Omitted arguments do not count.
    fn stages(&mut self, c: RoutineId, args: &[ast::Arg], kinds: &[ArgKind]) -> Option<Vec<u8>> {
        let params = self.routines[c.0 as usize].params.clone();
        let mut out = Vec::with_capacity(args.len());
        for ((a, k), p) in args.iter().zip(kinds).zip(&params) {
            let v = a.value()?;
            match p.mode {
                ParamMode::Var | ParamMode::Out => {
                    let is_out = p.mode == ParamMode::Out;
                    let found = self.dry_run(|s| {
                        let r = s.lower_place(&v, PlaceUse::RefArg { is_out });
                        let not_var = matches!(
                            s.last_place_result.take(),
                            Some(PlaceResult::NotVariable(_))
                        );
                        r.filter(|_| !not_var).map(|(_, t)| t)
                    });
                    let t = found?;
                    let open_ok =
                        matches!(p.ty, Ty::OpenArray(_)) && self.open_array_accepts(p.ty, t);
                    if t != p.ty && !open_ok {
                        return None;
                    }
                    out.push(0);
                }
                ParamMode::Value | ParamMode::Const => {
                    let fits = self.dry_run(|s| {
                        let e = s.lower_expected(&v, p.ty);
                        !e.ty.is_error() && !s.coerce(e, p.ty, true, None).ty.is_error()
                    });
                    if !fits {
                        return None;
                    }
                    out.push(match *k {
                        ArgKind::Typed(t, is_const) => self.conversion_stage(t, is_const, p.ty),
                        _ => 0,
                    });
                }
            }
        }
        Some(out)
    }

    /// The stage of passing a value of type `arg` to a parameter of type `param` that accepts it:
    /// 0 the same type, 1 a widening that keeps the value, 2 any other assignment compatibility
    /// (`docs/spec/semantics.md` 4.7).
    fn conversion_stage(&self, arg: Ty, is_const: bool, param: Ty) -> u8 {
        let a = self.types.base(arg);
        let p = self.types.base(param);
        if a == p
            || (matches!(param, Ty::OpenArray(_)) && self.open_array_accepts(param, arg))
            || (a == Ty::String && is_const && p == Ty::Utf8String)
        {
            return 0;
        }
        if a.is_integer() && p.is_integer() {
            let (alo, ahi) = a.range().expect("an integer type");
            let (plo, phi) = p.range().expect("an integer type");
            return if plo <= alo && ahi <= phi { 1 } else { 2 };
        }
        match (a, p) {
            (Ty::Single, Ty::Double) | (Ty::Char, Ty::String) | (Ty::Utf8String, Ty::String) => 1,
            _ => 2,
        }
    }
}
