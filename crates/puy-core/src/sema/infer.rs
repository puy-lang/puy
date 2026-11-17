//! Type argument inference (`docs/spec/generics.md` 5.2) and the choice between generic and
//! non-generic candidates of a call written without type arguments (5.4).

use super::Resolved;
use super::Sema;
use super::hir::RoutineId;
use super::overload::strip_parens;
use super::types::Ty;
use crate::diagnostics::Code;
use crate::source::TextRange;
use crate::syntax::ast::{self, ParamMode};

/// The note of E0763 (`docs/spec/diagnostics.md` 4.10): the call of `name` written with the type
/// arguments `shown`.
fn type_args_note(name: &str, shown: &[String]) -> String {
    format!(
        "write the type arguments, as in `{name}<{}>(…)`",
        shown.join(", ")
    )
}

/// The type arguments of one generic candidate, found from the arguments of a call.
enum Inferred {
    /// Every type argument was found.
    Args(Vec<Ty>),
    /// The type argument at the index has no candidate, or two candidates of different types;
    /// the others as far as they were found (for the notes of E0763).
    Failed {
        param: usize,
        conflict: Option<(Ty, Ty)>,
        found: Vec<Option<Ty>>,
    },
    /// An argument has an error (reported when the arguments are typed).
    ArgError,
}

/// The candidates of one type parameter (`docs/spec/generics.md` 5.2): from typed values
/// (strong) and from constants written without a type (weak), in the order of the arguments.
#[derive(Default, Clone)]
struct Found {
    strong: Vec<Ty>,
    weak: Vec<Ty>,
}

/// The generic candidates left out of a call without type arguments
/// (`docs/spec/generics.md` 5.4): the declarations to show in E0735, in their order, and why
/// each generic one was left out.
#[derive(Default)]
pub(super) struct Excluded {
    pub shown: Vec<RoutineId>,
    pub notes: Vec<String>,
}

impl<'a, 's> Sema<'a, 's> {
    /// The candidates of a call without type arguments of a name with generic declarations
    /// (`docs/spec/generics.md` 5.2, 5.4): each generic one is replaced by its instantiation
    /// with the type arguments inferred from `args`, or left out (with the reason, for E0735).
    /// With a single generic declaration, E0763 / E0759 are reported here. `None` when a
    /// diagnostic was reported.
    pub(super) fn infer_candidates(
        &mut self,
        set: &mut Vec<RoutineId>,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<Excluded> {
        let generic = |s: &Self, r: &RoutineId| !s.routines[r.0 as usize].type_params.is_empty();
        if !set.iter().any(|r| generic(self, r)) {
            return Some(Excluded::default());
        }
        if let [only] = set.as_slice() {
            let c = *only;
            let made = self.infer_single(c, args, span)?;
            *set = vec![made];
            return Some(Excluded::default());
        }
        let n = args.len();
        let mut ex = Excluded {
            shown: set.clone(),
            notes: Vec::new(),
        };
        let mut out = Vec::new();
        for &c in set.iter() {
            let r = &self.routines[c.0 as usize];
            // A candidate the count of the arguments rules out stays as it is (4.7)
            if !generic(self, &c) || !(r.min_args() <= n && n <= r.params.len()) {
                out.push(c);
                continue;
            }
            match self.infer_type_args(c, args) {
                Inferred::ArgError => {
                    self.lower_args_for_reads(args);
                    return None;
                }
                Inferred::Failed {
                    param, conflict, ..
                } => {
                    let msg = self.infer_failure_message(c, param, conflict);
                    ex.notes.push(msg);
                }
                Inferred::Args(targs) => {
                    let params = self.routines[c.0 as usize].type_params.clone();
                    let def_name = self.routines[c.0 as usize].name.clone();
                    let owner = self.routines[c.0 as usize].method_of;
                    let ranges: Vec<(Ty, TextRange)> = targs.iter().map(|t| (*t, span)).collect();
                    let (ok, msgs) = self.dry_run(|s| {
                        let at = s.sink.len();
                        let ok = s.check_param_constraints(&params, &def_name, &ranges, owner);
                        let msgs: Vec<String> =
                            s.sink.iter().skip(at).map(|d| d.message.clone()).collect();
                        (ok, msgs)
                    });
                    if !ok {
                        ex.notes.extend(msgs);
                        continue;
                    }
                    let origin = Some(self.span(span));
                    let made = self.with_inst(|i| i.instantiate_routine(c, targs, origin))?;
                    out.push(made);
                }
            }
        }
        *set = out;
        Some(ex)
    }

    /// The instantiation a call of the single generic declaration `c` without type arguments
    /// resolves to (`docs/spec/generics.md` 5.2): E0763 when a type argument is not found, and
    /// E0759 / E0760 when the inferred ones do not satisfy the constraints (with them in the
    /// notes). `None` when a diagnostic was reported.
    fn infer_single(
        &mut self,
        c: RoutineId,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<RoutineId> {
        let targs = match self.infer_type_args(c, args) {
            Inferred::Args(t) => t,
            Inferred::ArgError => {
                self.lower_args_for_reads(args);
                return None;
            }
            Inferred::Failed {
                param,
                conflict,
                found,
            } => {
                self.lower_args_for_reads(args);
                let msg = self.infer_failure_message(c, param, conflict);
                let note = self.write_type_args_note(c, &found);
                self.error(Code::E(763), msg, span).notes.push(note);
                return None;
            }
        };
        let params = self.routines[c.0 as usize].type_params.clone();
        let def_name = self.routines[c.0 as usize].name.clone();
        let owner = self.routines[c.0 as usize].method_of;
        let ranges: Vec<(Ty, TextRange)> = targs.iter().map(|t| (*t, span)).collect();
        let at = self.sink.len();
        if !self.check_param_constraints(&params, &def_name, &ranges, owner) {
            self.lower_args_for_reads(args);
            let shown: Vec<String> = targs.iter().map(|t| self.tn(*t)).collect();
            let note = format!(
                "the type arguments were inferred from the arguments as `{def_name}<{}>`",
                shown.join(", ")
            );
            for d in self.sink.iter_mut().skip(at) {
                if matches!(d.code, Some(Code::E(759 | 760))) {
                    d.notes.push(note.clone());
                }
            }
            return None;
        }
        let origin = Some(self.span(span));
        self.with_inst(|i| i.instantiate_routine(c, targs, origin))
    }

    /// The message of E0763 for the type parameter `param` of `c` (`docs/spec/generics.md` 9).
    fn infer_failure_message(
        &self,
        c: RoutineId,
        param: usize,
        conflict: Option<(Ty, Ty)>,
    ) -> String {
        let info = &self.routines[c.0 as usize];
        let p = info.type_params[param];
        let pname = self.types.get(p).name.clone().unwrap_or_default();
        self.failure_text(&pname, &info.name, conflict)
    }

    /// The message of E0763 for the type parameter `pname` of the routine `name`.
    fn failure_text(&self, pname: &str, name: &str, conflict: Option<(Ty, Ty)>) -> String {
        match conflict {
            Some((a, b)) => format!(
                "conflicting types `{}` and `{}` for the type argument `{pname}` of `{name}`",
                self.tn(a),
                self.tn(b)
            ),
            None => format!("cannot infer the type argument `{pname}` of `{name}`"),
        }
    }

    /// The type argument of `KeyEquals` / `KeyHash` (`name`) called without it
    /// (`docs/spec/equality.md` chapter 4; `docs/spec/generics.md` 5.2): every argument is a
    /// value of the type parameter `T`. E0763 when it is not found. `None` when a diagnostic was
    /// reported.
    pub(super) fn infer_key_arg(
        &mut self,
        name: &str,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<Ty> {
        let t = self.key_param();
        let params = vec![Ty::Param(t); args.len()];
        match self.infer_from(&[t], &params, args) {
            Inferred::Args(found) => Some(found[0]),
            Inferred::ArgError => {
                self.lower_args_for_reads(args);
                None
            }
            Inferred::Failed { conflict, .. } => {
                self.lower_args_for_reads(args);
                let msg = self.failure_text("T", name, conflict);
                self.error(Code::E(763), msg, span)
                    .notes
                    .push(type_args_note(name, &["T".to_string()]));
                None
            }
        }
    }

    /// The type parameter `T` of `KeyEquals` / `KeyHash`, which only inference sees.
    fn key_param(&mut self) -> super::types::TypeId {
        if let Some(t) = self.key_param {
            return t;
        }
        let t = self.types.add(super::types::TypeDef {
            name: Some("T".to_string()),
            full_name: None,
            kind: super::types::TypeKind::Param {
                unit: 0,
                index: 0,
                constraints: Vec::new(),
            },
            span: TextRange::default(),
        });
        self.key_param = Some(t);
        t
    }

    /// The note of E0763 (`docs/spec/diagnostics.md` 4.10): the call written with the type
    /// arguments, those found as types and the others as their names.
    fn write_type_args_note(&self, c: RoutineId, found: &[Option<Ty>]) -> String {
        let info = &self.routines[c.0 as usize];
        let shown: Vec<String> = info
            .type_params
            .iter()
            .zip(found)
            .map(|(p, t)| match t {
                Some(t) => self.tn(*t),
                None => self.types.get(*p).name.clone().unwrap_or_default(),
            })
            .collect();
        type_args_note(&info.name, &shown)
    }

    /// Types the arguments of a call that does not resolve, for their own diagnostics and so
    /// their variables count as read (W0001, E2000). An anonymous method has no type here: only
    /// its parameters and return type are resolved.
    fn lower_args_for_reads(&mut self, args: &[ast::Arg]) {
        for a in args {
            let Some(v) = a.value() else {
                continue;
            };
            match strip_parens(&v) {
                ast::Expr::Nil(_) => {}
                ast::Expr::Anon(an) => {
                    self.signature_of(an.params(), an.is_function(), an.return_type());
                }
                _ => {
                    self.lower_expr_unchecked_ctor(&v);
                }
            }
        }
    }

    /// The type arguments of the generic routine `c` from the arguments of a call
    /// (`docs/spec/generics.md` 5.2, steps 1 to 3).
    fn infer_type_args(&mut self, c: RoutineId, args: &[ast::Arg]) -> Inferred {
        let own = self.routines[c.0 as usize].type_params.clone();
        let params: Vec<Ty> = self.routines[c.0 as usize]
            .params
            .iter()
            .map(|p| p.ty)
            .collect();
        self.infer_from(&own, &params, args)
    }

    /// The type arguments of the type parameters `own` from the arguments of a call whose
    /// parameter types are `params` (`docs/spec/generics.md` 5.2, steps 1 to 3).
    fn infer_from(
        &mut self,
        own: &[super::types::TypeId],
        params: &[Ty],
        args: &[ast::Arg],
    ) -> Inferred {
        let mut found = vec![Found::default(); own.len()];
        for (a, &p) in args.iter().zip(params) {
            if !self.types.has_param(p) {
                continue;
            }
            let Some(v) = a.value() else {
                return Inferred::ArgError;
            };
            if !self.collect_arg(p, &v, own, &mut found) {
                return Inferred::ArgError;
            }
        }
        let mut out: Vec<Option<Ty>> = Vec::new();
        let mut failed: Option<(usize, Option<(Ty, Ty)>)> = None;
        for (i, f) in found.iter().enumerate() {
            let list = if f.strong.is_empty() {
                &f.weak
            } else {
                &f.strong
            };
            match list.first() {
                None => {
                    out.push(None);
                    failed.get_or_insert((i, None));
                }
                Some(&first) => match list.iter().find(|t| **t != first) {
                    Some(&other) => {
                        out.push(None);
                        failed.get_or_insert((i, Some((first, other))));
                    }
                    None => out.push(Some(first)),
                },
            }
        }
        match failed {
            Some((param, conflict)) => Inferred::Failed {
                param,
                conflict,
                found: out,
            },
            None => Inferred::Args(out.into_iter().flatten().collect()),
        }
    }

    /// Collects the candidates an argument gives for the parameter type `pty` (step 1). False
    /// when the argument has an error.
    fn collect_arg(
        &mut self,
        pty: Ty,
        v: &ast::Expr,
        own: &[super::types::TypeId],
        found: &mut [Found],
    ) -> bool {
        match strip_parens(v) {
            ast::Expr::Nil(_) => true,
            ast::Expr::ArrayCtor(ac) => {
                let Some(elem) = self.elem_of(pty) else {
                    return true;
                };
                for el in ac.elements() {
                    if let ast::CtorElem::Expr(x) = el
                        && !self.collect_value(elem, &x, own, found)
                    {
                        return false;
                    }
                }
                true
            }
            ast::Expr::Anon(an) => {
                let (ps, ret) = self
                    .dry_run(|s| {
                        let n = s.sink.len();
                        let r = s.signature_of(an.params(), an.is_function(), an.return_type());
                        let failed = s.sink.iter().skip(n).any(|d| d.is_error());
                        (!failed).then_some(r)
                    })
                    .unwrap_or_default();
                if ps.iter().any(|p| p.ty.is_error()) || ret.is_some_and(|r| r.is_error()) {
                    return false;
                }
                let sig: Vec<(Ty, ParamMode)> = ps.iter().map(|p| (p.ty, p.mode)).collect();
                self.unify_signature(pty, &sig, ret, own, found);
                true
            }
            // A routine name is a value only for a procedural parameter type; elsewhere it is a
            // call, typed by its result (5.2, step 1)
            e if self.is_code_name(&e) && self.types.proc_sig(pty).is_none() => {
                let t = self.dry_run(|s| s.lower_expr(&e)).ty;
                if t.is_error() {
                    return false;
                }
                self.unify(pty, t, false, own, found);
                true
            }
            e if self.is_code_name(&e) => {
                let r = self.dry_run(|s| s.resolve_name_expr(&e));
                if let Some((Resolved::Routine(id), _, _)) = r
                    && let [only] = self.routine_overloads(id).as_slice()
                    && self.routines[only.0 as usize].type_params.is_empty()
                {
                    let info = &self.routines[only.0 as usize];
                    let sig: Vec<(Ty, ParamMode)> =
                        info.params.iter().map(|p| (p.ty, p.mode)).collect();
                    let ret = info.ret;
                    self.unify_signature(pty, &sig, ret, own, found);
                }
                true
            }
            e => self.collect_value(pty, &e, own, found),
        }
    }

    /// Collects the candidate of a value written for the parameter type `pty`: strong, or weak
    /// for a constant written without a type (step 2). False when the value has an error.
    fn collect_value(
        &mut self,
        pty: Ty,
        x: &ast::Expr,
        own: &[super::types::TypeId],
        found: &mut [Found],
    ) -> bool {
        let t = self.dry_run(|s| s.lower_value(x)).ty;
        if t.is_error() {
            return false;
        }
        let weak = self.is_untyped_const(x);
        self.unify(pty, t, weak, own, found);
        true
    }

    /// Whether an expression is a constant written without a type (`docs/spec/generics.md`
    /// 5.2): a literal, the name of a constant declared without a type, and the operators on
    /// them.
    fn is_untyped_const(&mut self, x: &ast::Expr) -> bool {
        match strip_parens(x) {
            ast::Expr::Literal(_) => true,
            ast::Expr::Unary(u) => u.operand().is_some_and(|o| self.is_untyped_const(&o)),
            ast::Expr::Binary(b) => match (b.lhs(), b.rhs()) {
                (Some(l), Some(r)) => self.is_untyped_const(&l) && self.is_untyped_const(&r),
                _ => false,
            },
            e @ (ast::Expr::NameRef(_) | ast::Expr::Field(_)) => {
                match self.dry_run(|s| s.resolve_name_expr(&e)) {
                    Some((Resolved::Const { ty, span, .. }, _, _)) => {
                        let typed = span.is_some_and(|s| self.typed_consts.contains(&s));
                        let ty = self.types.base(ty);
                        !typed
                            && (ty.is_integer()
                                || matches!(
                                    ty,
                                    Ty::Double
                                        | Ty::Single
                                        | Ty::Currency
                                        | Ty::Char
                                        | Ty::String
                                        | Ty::Boolean
                                ))
                    }
                    Some((Resolved::Bool(_), _, _)) => true,
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The element type of a dynamic array or open array parameter type.
    fn elem_of(&self, ty: Ty) -> Option<Ty> {
        match ty {
            Ty::DynArray(id) | Ty::OpenArray(id) | Ty::Array(id) => match self.types.get(id).kind {
                super::types::TypeKind::DynArray { elem, .. }
                | super::types::TypeKind::Array { elem, .. } => Some(elem),
                _ => None,
            },
            _ => None,
        }
    }

    /// Matches the shape of the parameter type `p` with the argument type `a`, collecting the
    /// candidates of the type parameters `own` in `p` (step 1). Shapes that do not match give
    /// nothing (the argument is then checked against the replaced heading).
    fn unify(
        &mut self,
        p: Ty,
        a: Ty,
        weak: bool,
        own: &[super::types::TypeId],
        found: &mut [Found],
    ) {
        if !self.types.has_param(p) {
            return;
        }
        if let Ty::Param(q) = p {
            if let Some(i) = own.iter().position(|o| *o == q) {
                let f = &mut found[i];
                if weak {
                    f.weak.push(a);
                } else {
                    f.strong.push(a);
                }
            }
            return;
        }
        if let Ty::DynArray(_) | Ty::OpenArray(_) = p {
            if let (Some(pe), Some(ae)) = (self.elem_of(p), self.elem_of(a)) {
                self.unify(pe, ae, false, own, found);
            }
            return;
        }
        let (Some(pid), Some(aid)) = (p.type_id(), a.type_id()) else {
            return;
        };
        // An instantiation: only one of the same definition, by position
        if let Some((pdef, pargs)) = self.types.generic_key(pid)
            && pdef != pid
            && let Some(i) = self.types.instance_of(aid)
            && i.def == pdef
            && std::mem::discriminant(&p) == std::mem::discriminant(&a)
        {
            let aargs = i.args.clone();
            for (x, y) in pargs.into_iter().zip(aargs) {
                self.unify(x, y, false, own, found);
            }
        }
    }

    /// Matches a procedural parameter type with the parameters and return type of a routine
    /// or an anonymous method passed for it (step 1): the same number, each with the same mode.
    fn unify_signature(
        &mut self,
        p: Ty,
        sig: &[(Ty, ParamMode)],
        ret: Option<Ty>,
        own: &[super::types::TypeId],
        found: &mut [Found],
    ) {
        let Some((_, pp, pr)) = self.types.proc_sig(p) else {
            return;
        };
        let pp: Vec<(Ty, ParamMode)> = pp.iter().map(|x| (x.ty, x.mode)).collect();
        if pp.len() != sig.len()
            || pp.iter().zip(sig).any(|(x, y)| x.1 != y.1)
            || pr.is_some() != ret.is_some()
        {
            return;
        }
        for ((x, _), (y, _)) in pp.iter().zip(sig) {
            self.unify(*x, *y, false, own, found);
        }
        if let (Some(x), Some(y)) = (pr, ret) {
            self.unify(x, y, false, own, found);
        }
    }
}
