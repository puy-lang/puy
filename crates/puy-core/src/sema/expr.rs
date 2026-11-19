//! Typing of expressions and constant folding (`docs/spec/semantics.md` chapters 2–3 and 5).

use super::builtins::{self, builtin_name, builtin_signature};
use super::hir::*;
use super::overload::{Pick, strip_parens};
use super::reals::{TRUNC_ROUND_NOTE, is_fractional, real_calc_type};
use super::types::{Const, MAX_STRING_LEN, SelfKind, Ty, bound_text};
use super::{Resolved, Sema};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::lexer;
use crate::source::{Span, TextRange};
use crate::syntax::SyntaxKind;
use crate::syntax::ast::{self, AstNode, ParamMode};

pub(super) fn err_expr(span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Error,
        ty: Ty::Error,
        value: None,
        span,
    }
}

/// The steps of a binary chain so far as one expression (`acc` when there are none), to be the
/// left operand of what follows.
fn fold_steps(acc: Expr, first_expr: &mut Option<Expr>, steps: &mut Vec<BinStep>) -> Expr {
    match first_expr.take() {
        Some(fe) => Expr {
            kind: ExprKind::Binary {
                first: Box::new(fe),
                steps: std::mem::take(steps),
            },
            ty: acc.ty,
            value: None,
            span: acc.span,
        },
        None => acc,
    }
}

fn const_expr(value: Const, ty: Ty, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Const { explicit: false },
        ty,
        value: Some(value),
        span,
    }
}

/// A constant whose type was written rather than taken from its value (`docs/spec/semantics.md`
/// 3.2): a conversion (`Integer(1)`), `High` / `Low`, a constant declared with a type.
pub(super) fn explicit_const(value: Const, ty: Ty, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Const { explicit: true },
        ty,
        value: Some(value),
        span,
    }
}

/// Whether an operand keeps a folded integer constant in its type: an `Int64` / `UInt64`
/// constant that keeps its own type (`Int64(1) + 1` is `Int64`; `docs/spec/semantics.md` 3.6).
fn keeps_wide_int_type(e: &Expr) -> bool {
    matches!(e.value, Some(Const::Int(_)))
        && matches!(e.ty, Ty::Int64 | Ty::UInt64)
        && keeps_int_type(e)
}

/// The constant folded from an operation (`docs/spec/semantics.md` 3.6). An integer arithmetic
/// result takes its type from its value (2.2), as a literal would, unless an operand is an
/// `Int64` / `UInt64` constant that keeps its type (`keeps_wide`): `Cardinal(5) + 1` is the
/// `Integer` 6 and `4000000000 - 3999999999` the `Integer` 1, but `Int64(1) + 1` the `Int64` 2.
fn folded_const(
    value: Const,
    ty: Ty,
    calc: &Calc,
    op: BinOp,
    keeps_wide: bool,
    span: TextRange,
) -> Expr {
    match (&value, calc) {
        (Const::Int(v), Calc::Int(_)) if !op.is_comparison() => {
            if keeps_wide {
                explicit_const(value, ty, span)
            } else {
                let ty = Ty::of_literal(*v);
                const_expr(value, ty, span)
            }
        }
        _ => const_expr(value, ty, span),
    }
}

/// Whether an integer constant keeps its own type in the computation-type table rather than
/// taking the other operand's (`docs/spec/semantics.md` 3.2): its type was written, or is not the
/// type its value would have as a literal (2.2).
pub(super) fn keeps_int_type(e: &Expr) -> bool {
    match &e.value {
        Some(Const::Int(v)) => {
            matches!(e.kind, ExprKind::Const { explicit: true }) || e.ty != Ty::of_literal(*v)
        }
        _ => true,
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// Types an expression. A value of a subrange type has its base type (`docs/spec/types.md`
    /// 8.2: operations, comparisons and conversions work on the base type).
    pub(super) fn lower_expr(&mut self, e: &ast::Expr) -> Expr {
        self.lower_expr_declared(e).0
    }

    /// [`Self::lower_expr`] and the subrange type the value had (otherwise its type), for the
    /// subrange type's own helpers (`docs/spec/objects.md` 10.4).
    pub(super) fn lower_expr_declared(&mut self, e: &ast::Expr) -> (Expr, Ty) {
        let (x, declared) = self.lower_value_declared(e);
        // A procedural value written without parentheses is called here
        // (`docs/spec/objects.md` 9.4)
        let x = self.call_bare_value(e, x);
        let declared = match declared {
            Ty::Subrange(_) if self.types.base(declared) == x.ty => declared,
            _ => x.ty,
        };
        (x, declared)
    }

    /// Types an expression, keeping a subrange type (`Low` / `High` of a variable).
    pub(super) fn lower_expr_raw(&mut self, e: &ast::Expr) -> Expr {
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
            ast::Expr::Field(f) if self.is_value_chain(e) => self.lower_field(f, span),
            ast::Expr::NameRef(_) | ast::Expr::Field(_) => self.lower_name_use(e, span),
            ast::Expr::RecordConst(_) | ast::Expr::ArrayConst(_) => {
                // The parser reads these only after a written type (`lower_init` takes them there)
                self.error(
                    Code::E(702),
                    "a constant written in parentheses needs a record or array type",
                    span,
                );
                err_expr(span)
            }
            ast::Expr::Unary(u) => self.lower_unary(u, span),
            ast::Expr::Binary(b) => self.lower_binary(b, span),
            ast::Expr::Call(c) => self.lower_call_expr(c, span),
            ast::Expr::Index(i) => self.lower_index(i, span),
            ast::Expr::Deref(_) | ast::Expr::Error(_) => err_expr(span),
            ast::Expr::Nil(_) => {
                // `nil` takes its type from the context (`lower_expected`, comparisons)
                self.nil_without_type(span);
                err_expr(span)
            }
            ast::Expr::ArrayCtor(ac) => {
                self.lower_ctor_elements_quietly(ac);
                self.error(
                    Code::E(729),
                    "cannot infer the type of an array constructor",
                    span,
                )
                .notes
                .push("give the array a type, as in `var A: TArray<Integer> := [1, 2]`".into());
                err_expr(span)
            }
            ast::Expr::Inherited(_) => self.lower_name_use(e, span),
            ast::Expr::Anon(_) => self.anon_without_type(span),
        }
    }

    /// Types an expression whose type the context gives (an assignment, an initial value, an
    /// argument, `Exit`): an array constructor and `nil` take their type from `expected`
    /// (`docs/spec/types.md` 2.3, chapter 4). Other expressions are typed as usual; the caller
    /// converts them (`coerce`).
    pub(super) fn lower_expected(&mut self, e: &ast::Expr, expected: Ty) -> Expr {
        let span = ast::trimmed_range(e.syntax());
        match e {
            ast::Expr::Paren(p) => match p.inner() {
                Some(inner @ (ast::Expr::ArrayCtor(_) | ast::Expr::Nil(_))) => {
                    let mut x = self.lower_expected(&inner, expected);
                    x.span = span;
                    x
                }
                _ => self.lower_expr(e),
            },
            // A routine or method name is a value, and a procedural value is not called
            // (`docs/spec/objects.md` 9.4)
            _ if expected.is_procedural() && !matches!(e, ast::Expr::Nil(_)) => {
                self.lower_proc_expected(e, expected)
            }
            ast::Expr::NameRef(_) | ast::Expr::Field(_)
                if matches!(expected, Ty::ClassRef(_)) && self.class_name_of(e).is_some() =>
            {
                // A class as a value (`docs/spec/objects.md` 2.8)
                let class = self.class_name_of(e).expect("checked above");
                self.class_ref_value(class, span)
            }
            ast::Expr::Nil(_) if !expected.is_error() => {
                let target = match expected {
                    Ty::OpenArray(id) => Ty::DynArray(id),
                    t => t,
                };
                if self.takes_nil_ty(target) {
                    return const_expr(Const::Nil, target, span);
                }
                self.nil_mismatch(expected, span);
                err_expr(span)
            }
            // The target has been diagnosed
            ast::Expr::Nil(_) => err_expr(span),
            ast::Expr::ArrayCtor(ac) if !expected.is_error() => self.lower_ctor(ac, expected, span),
            ast::Expr::ArrayCtor(ac) => {
                self.lower_ctor_elements_quietly(ac);
                err_expr(span)
            }
            _ => self.lower_expr(e),
        }
    }

    /// The class an expression names when it is just a class's name (`TCircle`, `Unit.TCircle`,
    /// `TOuter.TInner`).
    pub(super) fn class_name_of(&mut self, e: &ast::Expr) -> Option<super::types::TypeId> {
        if !matches!(e, ast::Expr::NameRef(_) | ast::Expr::Field(_)) {
            return None;
        }
        match self.dry_run(|s| s.resolve_name_expr(e)) {
            Some((super::Resolved::Type(Ty::Class(id)), ..)) => Some(id),
            _ => None,
        }
    }

    /// A class as a value of its class reference type (`classref TFoo`; `docs/spec/objects.md`
    /// 2.8).
    pub(super) fn class_ref_value(&mut self, class: super::types::TypeId, span: TextRange) -> Expr {
        let ty = self.types.class_ref(
            class,
            super::types::DynWritten::Anonymous {
                generic: false,
                span,
            },
        );
        Expr {
            kind: ExprKind::ClassRefValue(class),
            ty,
            value: None,
            span,
        }
    }

    /// Types an expression whose use has already been diagnosed: a constructor only has its
    /// elements typed (no E0729 on top), and `nil`, which has no type without its use, is not
    /// reported (no E0702 on top; `docs/spec/diagnostics.md` chapter 2).
    pub(super) fn lower_expr_unchecked_ctor(&mut self, e: &ast::Expr) {
        match e {
            ast::Expr::ArrayCtor(ac) => self.lower_ctor_elements_quietly(ac),
            _ if matches!(strip_parens(e), ast::Expr::Nil(_)) => {}
            _ => {
                self.lower_expr(e);
            }
        }
    }

    /// Types the elements of an array constructor that cannot be used (for their own diagnostics).
    pub(super) fn lower_ctor_elements_quietly(&mut self, ac: &ast::ArrayCtor) {
        for el in ac.elements() {
            match el {
                ast::CtorElem::Expr(x) => {
                    if let ast::Expr::ArrayCtor(inner) = &x {
                        self.lower_ctor_elements_quietly(inner);
                    } else {
                        self.lower_expr(&x);
                    }
                }
                ast::CtorElem::Range(r) => {
                    let (lo, hi) = r.bounds();
                    for b in [lo, hi].into_iter().flatten() {
                        self.lower_expr(&b);
                    }
                }
            }
        }
    }

    /// An array constructor of the array type `expected` (`docs/spec/types.md` chapter 4): a new
    /// dynamic array, or a static array with as many elements as the type (a constant when every
    /// element is). An open array parameter takes a dynamic array of its element type.
    fn lower_ctor(&mut self, ac: &ast::ArrayCtor, expected: Ty, span: TextRange) -> Expr {
        if expected.is_set() {
            return self.lower_set_ctor(ac, expected, span, false);
        }
        let target = match expected {
            Ty::OpenArray(id) => Ty::DynArray(id),
            t => t,
        };
        let Some(elem) = self.types.elem_of(target) else {
            self.lower_ctor_elements_quietly(ac);
            let t = self.tn(expected);
            self.error(
                Code::E(702),
                format!("type mismatch: expected `{t}`, found an array constructor"),
                span,
            );
            return err_expr(span);
        };
        let checks = self.checks_at(span.start());
        let written = ac.elements();
        let mut elems = Vec::with_capacity(written.len());
        let mut failed = false;
        for el in &written {
            match el {
                ast::CtorElem::Expr(x) => {
                    let v = self.lower_expected(x, elem);
                    if v.ty.is_error() || elem.is_error() {
                        failed = true;
                        continue;
                    }
                    let v = self.coerce(v, elem, checks.range, None);
                    if v.ty.is_error() {
                        failed = true;
                        continue;
                    }
                    elems.push(v);
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
        if failed {
            return err_expr(span);
        }
        if let Ty::Array(_) = target {
            let n = self.types.slot_count(target) as usize;
            if elems.len() != n {
                self.error(
                    Code::E(702),
                    format!("expected {n} elements, found {}", elems.len()),
                    span,
                );
                return err_expr(span);
            }
            // A static array of constants is a constant (`docs/spec/types.md` chapter 4)
            if elems.iter().all(|e| e.value.is_some()) {
                let items = elems.into_iter().map(|e| e.value.unwrap()).collect();
                return const_expr(Const::Agg(items), target, span);
            }
        }
        Expr {
            kind: ExprKind::ArrayCtor(elems),
            ty: target,
            value: None,
            span,
        }
    }

    /// The value of a string literal token, or `None` after E0009 (a line of a multi-line
    /// string shallower than its closing quotes).
    pub(super) fn string_literal(&mut self, tok: &crate::syntax::SyntaxToken) -> Option<Vec<u16>> {
        let base: u32 = tok.text_range().start().into();
        match lexer::string_literal_value(self.text(tok).as_bytes(), base) {
            Ok(units) => Some(units),
            Err(e) => {
                let d = Diagnostic::error(Code::E(9), "invalid multi-line string", Stage::Lexer)
                    .with_primary(self.span(e.range), "shallower than the closing quotes");
                self.push(d);
                None
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
            SyntaxKind::STRING_LIT => match self.string_literal(&tok) {
                Some(units) if units.len() == 1 => {
                    const_expr(Const::Char(units[0]), Ty::Char, span)
                }
                Some(units) => const_expr(Const::Str(units), Ty::String, span),
                None => err_expr(span),
            },
            SyntaxKind::FLOAT_LIT => {
                // A `Double` constant, rounded to nearest even (`docs/spec/types.md` 8.4)
                let digits: String = text
                    .iter()
                    .filter(|c| **c != b'_')
                    .map(|c| *c as char)
                    .collect();
                match digits.parse::<f64>() {
                    Ok(v) => const_expr(Const::real(v), Ty::Double, span),
                    Err(_) => err_expr(span),
                }
            }
            _ => err_expr(span),
        }
    }

    /// Uses a name as a value.
    fn lower_name_use(&mut self, e: &ast::Expr, span: TextRange) -> Expr {
        let Some((r, name, range)) = self.resolve_name_expr(e) else {
            return err_expr(span);
        };
        self.resolved_use(r, &name, range, span)
    }

    /// What a name resolved to, used as a value.
    pub(super) fn resolved_use(
        &mut self,
        r: Resolved,
        name: &str,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        match r {
            Resolved::Var(id) => {
                self.mark_used(id, span);
                self.weak_read(Expr {
                    kind: ExprKind::Var(id),
                    ty: self.var(id).ty,
                    value: None,
                    span,
                })
            }
            Resolved::Global(id) => self.weak_read(Expr {
                kind: ExprKind::Global(id),
                ty: self.globals[id.0 as usize].ty,
                value: None,
                span,
            }),
            Resolved::Const {
                value,
                ty,
                span: decl,
            } => {
                if ty.is_error() {
                    err_expr(span)
                } else if self.is_explicit_const(decl) {
                    explicit_const(value, ty, span)
                } else {
                    const_expr(value, ty, span)
                }
            }
            Resolved::Bool(b) => const_expr(Const::Bool(b), Ty::Boolean, span),
            Resolved::Routine(id) => self.lower_bare_routine_call(id, name, range, span),
            Resolved::Func(f) => self.lower_builtin_call(f, name, &[], range, span),
            Resolved::Key(op, t) => self.lower_key_call(op, t, &[], span),
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
                        format!("`{}` is a type and cannot be used as a value", self.tn(t)),
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
            Resolved::ClassCreate(id, ctor) => self.new_instance(id, &ctor, &[], span, range, None),
            Resolved::RaiseOuter => {
                self.error(
                    Code::E(725),
                    "procedure `RaiseOuterException` has no value",
                    range,
                );
                err_expr(span)
            }
            Resolved::StdFile(f) => {
                self.std_file_misuse(f, range);
                err_expr(span)
            }
            Resolved::Member {
                recv,
                class,
                member,
                inherited,
            } => self.member_expr(*recv, class, member, inherited, None, true, range, span),
            Resolved::Error => err_expr(span),
        }
    }

    /// `TFoo.Create(…)` (`docs/spec/objects.md` 3.1): a new instance on which the constructor
    /// `name` the class has (chosen among the overloads as for a method) runs. A class without a
    /// constructor of its own has the built-in `TObject.Create`, which takes no argument.
    /// `C.Create(…)` (`of`, a class reference whose static type's class is `id`) creates an
    /// instance of the class `C` points to, and a virtual constructor is called through its table.
    pub(super) fn new_instance(
        &mut self,
        id: super::types::TypeId,
        name: &str,
        args: &[ast::Arg],
        span: TextRange,
        name_range: TextRange,
        of: Option<Expr>,
    ) -> Expr {
        if self.types.is_param(id) {
            return self.param_new(id, args, span);
        }
        let after = of.is_some() || self.types.has_after_construction(id);
        // An abstract class is only refused when created from its name (3.1)
        let named = of.is_none();
        let of = of.map(Box::new);
        if self.types.is_exception_class(id) {
            self.exception_create_routine();
        }
        let set = self.method_set(id, name);
        if set.is_empty() && self.is_builtin_create_fmt(id, name) {
            // `T.CreateFmt(Fmt, […])` (`docs/spec/objects.md` 8.2): `Message` is the `Format`
            let Some(msg) = self.format_message(args, span) else {
                return err_expr(span);
            };
            if named && self.check_abstract_create(id, span) {
                return err_expr(span);
            }
            return Expr {
                kind: ExprKind::New {
                    class: Ty::Class(id),
                    ctor: None,
                    args: vec![CallArg::Value(msg)],
                    after,
                    of,
                    dispatch: Dispatch::Method,
                },
                ty: Ty::Class(id),
                value: None,
                span,
            };
        }
        let nearest = self
            .methods_named(id, name)
            .into_iter()
            .next()
            .filter(|(_, m)| m.routine.is_some());
        if set.is_empty()
            && let Some((owner, m)) = nearest
        {
            // Only constructors the code cannot see (E0743; `docs/spec/objects.md` 2.5)
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr_unchecked_ctor(&v);
                }
            }
            let owner_name = self.types.name(Ty::Class(owner));
            let msg = match m.vis {
                super::types::Visibility::StrictPrivate | super::types::Visibility::Private => {
                    format!("`{}` is private to `{owner_name}`", m.name)
                }
                _ => format!("`{}` is protected in `{owner_name}`", m.name),
            };
            let mut d =
                Diagnostic::error(Code::E(743), msg, Stage::Sema).with_primary(self.span(span), "");
            if let Some(s) = self.method_span(owner, &m) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            return err_expr(span);
        }
        if set.is_empty() {
            // The built-in `TObject.Create`
            self.check_no_width(args);
            let values: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
            for v in &values {
                self.lower_expr_unchecked_ctor(v);
            }
            if named && self.check_abstract_create(id, span) {
                return err_expr(span);
            }
            if !values.is_empty() {
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected 0, found {}",
                        values.len()
                    ),
                    span,
                );
                return err_expr(span);
            }
            return Expr {
                kind: ExprKind::New {
                    class: Ty::Class(id),
                    ctor: None,
                    args: Vec::new(),
                    after,
                    of,
                    dispatch: Dispatch::Method,
                },
                ty: Ty::Class(id),
                value: None,
                span,
            };
        }
        let ctor = match self.pick_method(set, args, span, false) {
            super::overload::Pick::Routine(r) => r,
            super::overload::Pick::Failed => return err_expr(span),
        };
        self.routine_hint_use(ctor, name_range);
        let call = self.lower_routine_call(ctor, args, span);
        if named && self.check_abstract_create(id, span) {
            return err_expr(span);
        }
        let ExprKind::Call { args, .. } = call.kind else {
            return err_expr(span);
        };
        self.check_dictionary_ownerships(ctor, &args, span);
        if let Some(msg) = self.builtin_create_message(ctor, &args) {
            // `T.Create(Msg)` of `Exception`, whose body is the compiler's (8.2)
            return Expr {
                kind: ExprKind::New {
                    class: Ty::Class(id),
                    ctor: None,
                    args: vec![CallArg::Value(msg)],
                    after,
                    of,
                    dispatch: Dispatch::Method,
                },
                ty: Ty::Class(id),
                value: None,
                span,
            };
        }
        // A virtual constructor is called through the table (`docs/spec/objects.md` 3.1, 15.1)
        let sealed = matches!(
            self.types.get(id).kind,
            super::types::TypeKind::Class { sealed: true, .. }
        );
        let dispatch = match self.method_of_routine(ctor) {
            Some((_, m)) if !sealed && !m.is_final => {
                m.vmt.map_or(Dispatch::Method, Dispatch::Virtual)
            }
            _ => Dispatch::Method,
        };
        Expr {
            kind: ExprKind::New {
                class: Ty::Class(id),
                ctor: Some(ctor),
                args,
                after,
                of,
                dispatch,
            },
            ty: Ty::Class(id),
            value: None,
            span,
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
        let id = match self.pick_overload(id, &[], range, true) {
            Pick::Routine(c) => c,
            Pick::Failed => return err_expr(span),
        };
        self.routine_hint_use(id, range);
        // The function being analyzed or one enclosing it (`docs/spec/semantics.md` 4.3, 4.9).
        // `Result` names its result unless a nested function in between has its own
        let self_up = self.enclosing_function(id);
        let result_is_self = self_up.is_some() && self_up == self.result_owner();
        let info = &self.routines[id.0 as usize];
        let is_self = self_up.is_some();
        if info.ret.is_none() {
            // A procedure used in an expression
            self.routine_no_value(id, range);
            return err_expr(span);
        }
        if is_self {
            let no_params = info.min_args() == 0;
            let n = info.name.clone();
            let name_span = self.routine_decl_span(id);
            let mut d = Diagnostic::error(
                Code::E(704),
                format!("recursive call to `{n}` needs parentheses"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(name_span, "declared here");
            if result_is_self {
                d = d.with_fix(Fix::suggestion(
                    "replace with `Result`",
                    vec![TextEdit::replace(self.input.file, range, "Result")],
                ));
            }
            if no_params {
                d = d.with_fix(Fix::suggestion(
                    format!("replace with `{n}()`"),
                    vec![TextEdit::replace(self.input.file, range, format!("{n}()"))],
                ));
            }
            self.push(d);
            return err_expr(span);
        }
        if info.min_args() > 0 {
            let n = info.name.clone();
            let d = Diagnostic::error(
                Code::E(704),
                format!("function `{n}` requires arguments"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.routine_decl_span(id), "declared here");
            self.push(d);
            return err_expr(span);
        }
        let _ = name;
        self.lower_routine_call(id, &[], span)
    }

    fn lower_index(&mut self, i: &ast::IndexExpr, span: TextRange) -> Expr {
        let Some(base) = i.base() else {
            return err_expr(span);
        };
        // `O.Items[I]` (`docs/spec/objects.md` 2.6)
        if let Some(e) = self.indexed_property_read(i, span) {
            return e;
        }
        // A distinct type of a string is indexed as its root (`docs/spec/types.md` 1.3)
        let base_e = self.lower_expr(&base);
        let base_e = self.rooted(base_e);
        let indices = i.indices();
        if base_e.ty.is_error() {
            for ix in &indices {
                self.lower_index_item_exprs(ix);
            }
            return err_expr(span);
        }
        if self.default_properties(base_e.ty).is_some() {
            // `X[…]` of a default array property (`docs/spec/objects.md` 12.1)
            return self.default_index_read(base_e, &indices, span);
        }
        if !self.check_index_count(base_e.ty, indices.len(), span) {
            return err_expr(span);
        }
        let mut cur = base_e;
        for (n, item) in indices.iter().enumerate() {
            let ix = match item {
                ast::IndexItem::Slice(sl) => {
                    if n + 1 < indices.len() {
                        // `docs/spec/types.md` 8.7: only the last index may be a slice
                        let r = ast::trimmed_range(sl.syntax());
                        self.error(
                            Code::E(720),
                            "a slice is only allowed as the last index".to_string(),
                            r,
                        );
                        return err_expr(span);
                    }
                    return self.lower_slice(cur, sl, span);
                }
                ast::IndexItem::Expr(e) => e,
            };
            if matches!(cur.ty, Ty::String | Ty::Utf8String) {
                // A byte of a `UTF8String` (`docs/spec/types.md` 8.5)
                let elem = if cur.ty == Ty::Utf8String {
                    Ty::Byte
                } else {
                    Ty::Char
                };
                let idx = self.lower_expr(ix);
                let idx = self.rooted(idx);
                if idx.ty.is_error() {
                    return err_expr(span);
                }
                if !idx.ty.is_integer() {
                    let r = idx.span;
                    self.error(
                        Code::E(719),
                        format!(
                            "index type mismatch: expected `Integer`, found `{}`",
                            self.tn(idx.ty)
                        ),
                        r,
                    );
                    return err_expr(span);
                }
                cur = Expr {
                    kind: ExprKind::StrIndex {
                        base: Box::new(cur),
                        index: Box::new(idx),
                    },
                    ty: elem,
                    value: None,
                    span,
                };
                continue;
            }
            if let Some(elem) = self.types.dyn_elem(cur.ty) {
                let Some(idx) = self.dyn_index(ix) else {
                    return err_expr(span);
                };
                cur = Expr {
                    kind: ExprKind::Elem {
                        base: Box::new(cur),
                        index: Box::new(idx),
                    },
                    ty: elem,
                    value: None,
                    span,
                };
                continue;
            }
            let Some((_, _, _, elem)) = self.types.array(cur.ty) else {
                self.error(
                    Code::E(720),
                    format!("cannot index a value of type `{}`", self.tn(cur.ty)),
                    span,
                );
                return err_expr(span);
            };
            let Some(idx) = self.array_index(cur.ty, ix) else {
                return err_expr(span);
            };
            cur = Expr {
                kind: ExprKind::Elem {
                    base: Box::new(cur),
                    index: Box::new(idx),
                },
                ty: elem,
                value: None,
                span,
            };
        }
        cur
    }

    /// Lowers the expressions of an index item for their diagnostics (the base is in error).
    fn lower_index_item_exprs(&mut self, item: &ast::IndexItem) {
        match item {
            ast::IndexItem::Expr(e) => {
                self.lower_expr(e);
            }
            ast::IndexItem::Slice(sl) => {
                let (lo, hi) = sl.bounds();
                for e in lo.iter().chain(hi.iter()) {
                    self.lower_expr(e);
                }
            }
        }
    }

    /// `X[L..H]` (`docs/spec/types.md` 8.7): a copy of the part of a string or an array. The bounds
    /// are integers (the index type for a static array; E0719), and either may be left out.
    fn lower_slice(&mut self, base: Expr, sl: &ast::Slice, span: TextRange) -> Expr {
        let (lo, hi) = sl.bounds();
        let ty = if matches!(base.ty, Ty::String | Ty::Utf8String) {
            base.ty
        } else if let Some(elem) = self.types.elem_of(base.ty) {
            self.types.dyn_array(
                elem,
                super::types::DynWritten::Anonymous {
                    generic: false,
                    span,
                },
            )
        } else {
            self.error(
                Code::E(720),
                format!("cannot index a value of type `{}`", self.tn(base.ty)),
                span,
            );
            return err_expr(span);
        };
        // A string's or a dynamic array's bounds are integers like its indices
        let index = self.types.array(base.ty).map_or(Ty::Integer, |a| a.0);
        let bound = |this: &mut Self, e: Option<ast::Expr>| -> Result<Option<Box<Expr>>, ()> {
            let Some(e) = e else {
                return Ok(None);
            };
            let x = this.index_operand(index, &e).ok_or(())?;
            let x = if index.is_integer() {
                this.coerce(x, Ty::Int64, false, None)
            } else {
                x
            };
            Ok(Some(Box::new(x)))
        };
        let low = bound(self, lo);
        let high = bound(self, hi);
        let (Ok(low), Ok(high)) = (low, high) else {
            return err_expr(span);
        };
        Expr {
            kind: ExprKind::Slice {
                base: Box::new(base),
                low,
                high,
            },
            ty,
            value: None,
            span,
        }
    }

    /// The number of indices `ty[I, J, …]` takes: one per nested static array, and one for a string
    /// at the end. E0705 when there are more (E0720 when the value takes none).
    pub(super) fn check_index_count(&mut self, ty: Ty, n: usize, span: TextRange) -> bool {
        let mut levels = 0usize;
        let mut t = ty;
        while let Some(elem) = self.types.elem_of(t) {
            levels += 1;
            t = elem;
        }
        if matches!(t, Ty::String | Ty::Utf8String) {
            levels += 1;
        }
        if levels == 0 {
            let d = self.error(
                Code::E(720),
                format!("cannot index a value of type `{}`", self.tn(ty)),
                span,
            );
            if Self::indexable_type(ty).is_some() {
                // `docs/spec/objects.md` 12.1
                d.notes.push(
                    "`X[…]` needs a default array property, as in `property Items[I: Integer]: T read GetItem; default;`"
                        .into(),
                );
            }
            return false;
        }
        if n > levels || n == 0 {
            self.error(
                Code::E(705),
                format!("wrong number of indices: expected {levels}, found {n}"),
                span,
            );
            return false;
        }
        true
    }

    /// The type check of an index or a slice bound (`docs/spec/types.md` 2.2, 2.3, 8.7): any
    /// integer type when `index` is an integer type, otherwise a value of `index` (a value of a
    /// subrange type has its base type, so this is assignment compatibility). E0719 names `index`.
    fn index_operand(&mut self, index: Ty, ix: &ast::Expr) -> Option<Expr> {
        let idx = self.lower_expr(ix);
        let idx = self.rooted(idx);
        if idx.ty.is_error() {
            return None;
        }
        let ok = if index.is_integer() {
            idx.ty.is_integer()
        } else {
            idx.ty == index
        };
        if !ok {
            let r = idx.span;
            self.error(
                Code::E(719),
                format!(
                    "index type mismatch: expected `{}`, found `{}`",
                    self.tn(index),
                    self.tn(idx.ty)
                ),
                r,
            );
            return None;
        }
        Some(idx)
    }

    /// The index of an element of the static array type `arr` (`docs/spec/types.md` 2.2): an
    /// integer for an integer range, otherwise the index type (E0719). A constant outside the
    /// bounds is E0714.
    pub(super) fn array_index(&mut self, arr: Ty, ix: &ast::Expr) -> Option<Expr> {
        let (index, lo, hi, _) = self.types.array(arr)?;
        let idx = self.index_operand(index, ix)?;
        if let Some(v) = idx.value.as_ref().and_then(Const::as_int)
            && (v < lo || v > hi)
        {
            let r = idx.span;
            self.error(
                Code::E(714),
                format!(
                    "index {} is out of range for `{}` ({}..{})",
                    bound_text(index, v),
                    self.tn(arr),
                    bound_text(index, lo),
                    bound_text(index, hi)
                ),
                r,
            );
            return None;
        }
        Some(idx)
    }

    /// The index of an element of a dynamic array or open array (`docs/spec/types.md` 2.3): an
    /// integer (E0719). The range is checked at run time.
    pub(super) fn dyn_index(&mut self, ix: &ast::Expr) -> Option<Expr> {
        let idx = self.index_operand(Ty::Integer, ix)?;
        Some(self.coerce(idx, Ty::Int64, false, None))
    }

    /// Whether a `.` chain starts from a value (a variable, constant, `Result` or call) rather than
    /// from a unit or type name, so that `X.F` is a field of `X` (`docs/spec/types.md` 2.1).
    pub(super) fn is_value_chain(&self, e: &ast::Expr) -> bool {
        // A member written with type arguments is resolved as a name (a generic routine or type;
        // `docs/spec/generics.md` 1.4)
        if let ast::Expr::Field(f) = e
            && f.type_args().is_some()
        {
            return false;
        }
        if self.class_value_chain(e) {
            return true;
        }
        // `U.R.F` where `R` is a value of the unit `U` (`docs/spec/units.md` 3.2)
        if let Some(chain) = super::name_chain(e)
            && let Some((u, n)) = self.unit_prefix(&chain)
        {
            return n + 1 < chain.len() && self.unit_names_value(u, &chain[n].0);
        }
        let mut cur = e.clone();
        loop {
            cur = match cur {
                ast::Expr::Field(f) => match f.base() {
                    Some(b) => b,
                    None => return false,
                },
                ast::Expr::NameRef(n) => {
                    if n.is_string_keyword() {
                        return false;
                    }
                    let lower = n.text().to_ascii_lowercase();
                    return self.names_value(&lower);
                }
                ast::Expr::Error(_) => return false,
                _ => return true,
            };
        }
    }

    /// Whether a name is declared as a value (a variable, a constant or a routine; `Result`).
    fn names_value(&self, lower: &str) -> bool {
        if let Some((_, sym)) = self.lookup_scopes(lower) {
            return !matches!(sym, super::LSym::Type { .. });
        }
        if lower == "result" && self.result_owner().is_some() {
            return true;
        }
        // `Self` and the members of `Self` in a method (`docs/spec/objects.md` 2.3)
        if let Some(class) = self.method_class()
            && (lower == "self"
                || self.types.class_chain(class).iter().any(|&c| {
                    // A class, or a record with methods (chapter 6)
                    let has_field = match &self.types.get(c).kind {
                        super::types::TypeKind::Class { fields, .. } => {
                            fields.iter().any(|f| f.name.eq_ignore_ascii_case(lower))
                        }
                        super::types::TypeKind::Record { fields, .. } => {
                            fields.iter().any(|f| f.name.eq_ignore_ascii_case(lower))
                        }
                        _ => false,
                    };
                    has_field
                        || self
                            .types
                            .class_methods(c)
                            .iter()
                            .any(|m| m.name.eq_ignore_ascii_case(lower))
                        || self
                            .types
                            .class_properties(c)
                            .iter()
                            .any(|p| p.name.eq_ignore_ascii_case(lower))
                        || self.types.class_statics(c).iter().any(|s| {
                            s.name.eq_ignore_ascii_case(lower)
                                && !matches!(s.kind, super::types::StaticKind::Type(_))
                        })
                }))
        {
            return true;
        }
        if let Some((_, sym)) = self.global_scope.iter().rev().find(|(n, _)| n == lower) {
            return !matches!(sym, super::GSym::Type { .. });
        }
        // `ExceptObject.Message` (`docs/spec/objects.md` 8.4): the only built-in function whose
        // value has members
        if lower == "exceptobject" {
            return true;
        }
        // A variable or constant from the public part of a used unit (`docs/spec/units.md` 3.1)
        self.unit_export_is_value(lower)
    }

    /// `R.F` where `R` is a value.
    fn lower_field(&mut self, f: &ast::FieldExpr, span: TextRange) -> Expr {
        let (Some(base), Some(field)) = (f.base(), f.field()) else {
            if let Some(b) = f.base() {
                self.lower_expr(&b);
            }
            return err_expr(span);
        };
        // A procedural value whose result has no such member is not called: it has no members
        // (`docs/spec/objects.md` 9.11)
        let (b, declared) = if self.member_of_procedural(&base, self.text(&field)) {
            self.lower_procedural_base(&base)
        } else {
            self.lower_expr_declared(&base)
        };
        if b.ty.is_error() {
            return err_expr(span);
        }
        let class = match b.ty {
            Ty::Class(c) | Ty::Intf(c) => Some(c),
            t @ Ty::ClassRef(_) => self.types.class_ref_target(t),
            _ => None,
        };
        if let Some(class) = class {
            let name = self.text(&field).to_string();
            let range = field.text_range();
            return match self.find_member(class, &name, range) {
                Ok(member) => self.member_expr(b, class, member, false, None, true, range, span),
                Err(true) => err_expr(span),
                Err(false) => {
                    // The helpers of the class (`docs/spec/objects.md` 10.4)
                    let found = if matches!(b.ty, Ty::Intf(_)) {
                        None
                    } else {
                        self.helper_resolved(b, &name, range)
                    };
                    match found {
                        Some(r) => self.resolved_use(r, &name, range, span),
                        None => {
                            self.member_missing(class, &name, range);
                            err_expr(span)
                        }
                    }
                }
            };
        }
        if let Ty::Record(rec) = b.ty {
            let name = self.text(&field).to_string();
            let range = field.text_range();
            if self.record_has_member(rec, &name, range) {
                // `docs/spec/objects.md` chapter 6
                return match self.find_member(rec, &name, range) {
                    Ok(member) => self.member_expr(b, rec, member, false, None, true, range, span),
                    Err(_) => err_expr(span),
                };
            }
        }
        let name = self.text(&field).to_string();
        let has_field = matches!(b.ty, Ty::Record(_))
            && self
                .types
                .fields(b.ty)
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&name));
        if !has_field {
            // The helpers of the value's type (`docs/spec/objects.md` 10.4)
            let range = field.text_range();
            let r = self.value_member(b, declared, &name, range, span);
            return self.resolved_use(r, &name, range, span);
        }
        match self.find_field(b.ty, &field, span) {
            Some((index, ty, name)) => self.weak_read(Expr {
                kind: ExprKind::Field {
                    base: Box::new(b),
                    index,
                    name,
                },
                ty,
                value: None,
                span,
            }),
            None => err_expr(span),
        }
    }

    /// Whether an argument of a call whose callee has been diagnosed is a routine or procedural
    /// value written as a bare name (`Addr(F)`): it is not typed, since it would be called there
    /// (E0704, E0725) though it was meant as a value (`docs/spec/objects.md` 9.11).
    pub(super) fn bare_routine_arg(&mut self, e: &ast::Expr) -> bool {
        matches!(e, ast::Expr::NameRef(_) | ast::Expr::Field(_))
            && self.dry_run(|s| {
                let n = s.sink.len();
                s.lower_expr(e);
                s.sink
                    .iter()
                    .skip(n)
                    .any(|d| matches!(d.code, Some(Code::E(704) | Code::E(725))))
            })
    }

    /// The type of an expression, without reporting anything (to choose how to read it).
    pub(super) fn trial_type(&mut self, e: &ast::Expr) -> Ty {
        self.dry_run(|s| s.lower_expr(e).ty)
    }

    /// The field `name_tok` of a record type: `(number, type, declared name)`. E0700 if the record
    /// has no such field. On a value that is not a record, a member of its helpers cannot be
    /// written as a variable (E0707), and a name no helper has is the E3200 of the built-in types'
    /// helpers (`docs/spec/objects.md` 10.4).
    pub(super) fn find_field(
        &mut self,
        ty: Ty,
        name_tok: &crate::syntax::SyntaxToken,
        whole: TextRange,
    ) -> Option<(u32, Ty, String)> {
        let name = self.text(name_tok).to_string();
        if !matches!(ty, Ty::Record(_)) {
            match self.find_helper_member(
                ty,
                &name,
                name_tok.text_range(),
                super::helpers::HelperRecv::Implicit,
            ) {
                Ok((_, super::helpers::HelperMember::Builtin(_))) => {
                    self.cannot_assign(whole, "a read-only property", None);
                }
                Ok(_) => self.cannot_assign(whole, "not a variable", None),
                Err(true) => {}
                Err(false) => {
                    self.error(
                        Code::E(3200),
                        "type helpers for built-in types are not supported yet (planned for M5)",
                        whole,
                    );
                }
            }
            return None;
        }
        let found = self
            .types
            .fields(ty)
            .iter()
            .enumerate()
            .find(|(_, f)| f.name.eq_ignore_ascii_case(&name))
            .map(|(i, f)| (i as u32, f.ty, f.name.clone(), f.vis, f.span));
        if let (Some((_, _, n, vis, decl)), Ty::Record(id)) = (&found, ty)
            && !self.can_see(*vis, id)
        {
            // `docs/spec/objects.md` chapter 6 (2.5)
            let t = self.tn(ty);
            let msg = match vis {
                super::types::Visibility::StrictPrivate | super::types::Visibility::Private => {
                    format!("`{n}` is private to `{t}`")
                }
                _ => format!("`{n}` is protected in `{t}`"),
            };
            let mut d = Diagnostic::error(Code::E(743), msg, Stage::Sema)
                .with_primary(self.span(name_tok.text_range()), "");
            if let Some(u) = self.types.class_unit(id) {
                d = d.with_secondary(Span::new(self.unit_file(u), *decl), "declared here");
            }
            self.push(d);
            return None;
        }
        let found = found.map(|(i, t, n, _, _)| (i, t, n));
        if found.is_none() {
            let t = self.tn(ty);
            self.error(
                Code::E(700),
                format!("`{t}` has no field `{name}`"),
                name_tok.text_range(),
            );
        }
        found
    }

    // ---- Unary ----

    fn lower_unary(&mut self, u: &ast::UnaryExpr, span: TextRange) -> Expr {
        let Some(op_tok) = u.op() else {
            return err_expr(span);
        };
        let op_range = op_tok.text_range();
        if op_tok.kind() == SyntaxKind::AT {
            // `@F` (E3004, reported by the parser): a routine's name there is not called, and
            // another operand is only checked for its own errors (`docs/spec/objects.md` 9.10)
            if let Some(o) = u.operand()
                && !self.is_code_name(&o)
            {
                self.lower_value(&o);
            }
            return err_expr(span);
        }
        let operand = match u.operand() {
            Some(o) => self.lower_expr(&o),
            None => return err_expr(span),
        };
        // A distinct type whose root is an integer type: its unary `-` / `+` / `not` is of the
        // distinct type when the root's would be of the root (`docs/spec/types.md` 1.3)
        let distinct = match operand.ty {
            t @ Ty::Distinct(_) if self.types.root(t).is_integer() => Some(t),
            _ => None,
        };
        let operand = self.rooted(operand);
        // The distinct type kept as the result: only when the computation type `calc` of the
        // root (`docs/spec/semantics.md` 3.2) is the root itself
        let root = operand.ty;
        let keep = |calc: Ty| distinct.filter(|_| calc == root);
        if operand.ty.is_error() {
            return err_expr(span);
        }
        // An operand whose type may declare operators (`docs/spec/objects.md` 11.2)
        let operand = if self.operator_owner(operand.ty).is_some() {
            let (name, text) = match op_tok.kind() {
                SyntaxKind::PLUS => ("Positive", "+"),
                SyntaxKind::MINUS => ("Negative", "-"),
                _ => ("LogicalNot", "not"),
            };
            match self.unary_operator(operand, name, text, op_range, span) {
                Ok(x) => return x,
                Err(o) => o,
            }
        } else {
            operand
        };
        if let Ty::Param(p) = operand.ty {
            // `docs/spec/generics.md` 3.3
            let text = self.text(&op_tok).to_ascii_lowercase();
            self.param_misuse(p, super::generics::ParamOp::Operator(&text), span);
            return err_expr(span);
        }
        match op_tok.kind() {
            SyntaxKind::PLUS => {
                if operand.ty.is_number() {
                    let mut e = operand;
                    e.span = span;
                    // `+` computes in the root itself
                    if let Some(t) = keep(root) {
                        e.ty = t;
                    }
                    e
                } else {
                    self.error(
                        Code::E(703),
                        format!(
                            "operator `+` cannot be applied to `{}`",
                            self.tn(operand.ty)
                        ),
                        op_range,
                    );
                    err_expr(span)
                }
            }
            SyntaxKind::MINUS => {
                if is_fractional(operand.ty) {
                    return self.real_negate(operand, op_range, span);
                }
                if !operand.ty.is_integer() {
                    self.error(
                        Code::E(703),
                        format!(
                            "operator `-` cannot be applied to `{}`",
                            self.tn(operand.ty)
                        ),
                        op_range,
                    );
                    return err_expr(span);
                }
                if let Some(Const::Int(v)) = operand.value {
                    let nv = -v;
                    if let Some(t) = neg_calc(operand.ty).and_then(keep) {
                        // A constant of a distinct type stays of it, so the value must fit
                        if !operand.ty.contains(nv) {
                            self.const_out_of_range(nv, t, op_range);
                            return err_expr(span);
                        }
                        return const_expr(Const::Int(nv), t, span);
                    }
                    // Negating a constant re-derives the type from the value
                    if !Ty::Int64.contains(nv) {
                        self.const_out_of_range(nv, Ty::Int64, op_range);
                        return err_expr(span);
                    }
                    return const_expr(Const::Int(nv), Ty::of_literal(nv), span);
                }
                let Some(calc) = neg_calc(operand.ty) else {
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
                };
                Expr {
                    kind: ExprKind::Unary {
                        op: UnOp::Neg,
                        operand: Box::new(operand),
                        calc: Calc::Int(calc),
                    },
                    ty: keep(calc).unwrap_or(calc),
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
                        ty: keep(calc).unwrap_or(calc),
                        value,
                        span,
                    }
                }
                t => {
                    self.error(
                        Code::E(703),
                        format!("operator `not` cannot be applied to `{}`", self.tn(t)),
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
        if self.generic_call_needs_parens(b) {
            if let Some(r) = b.rhs() {
                self.lower_expr(&r);
            }
            return err_expr(span);
        }
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
        // `F = G` / `F <> nil` of procedural values: compared without calling; an ordering of
        // values that cannot be called without arguments is E0703 (`docs/spec/objects.md` 9.5)
        if let [only] = chain.as_slice()
            && let (Some(op_tok), Some(r)) = (only.op(), only.rhs())
            && matches!(
                op_tok.kind(),
                SyntaxKind::EQ
                    | SyntaxKind::NE
                    | SyntaxKind::LT
                    | SyntaxKind::LE
                    | SyntaxKind::GT
                    | SyntaxKind::GE
            )
        {
            let op = match op_tok.kind() {
                SyntaxKind::EQ => BinOp::Eq,
                SyntaxKind::NE => BinOp::Ne,
                SyntaxKind::LT => BinOp::Lt,
                SyntaxKind::LE => BinOp::Le,
                SyntaxKind::GT => BinOp::Gt,
                _ => BinOp::Ge,
            };
            if let Some(x) = self.proc_comparison(&first, &r, op, op_tok.text_range(), span) {
                return x;
            }
        }
        // `nil = A` takes the type of `nil` from the right operand (`docs/spec/types.md` 2.3)
        let first_nil = is_nil(&first);
        // `[…] + S`: the constructor takes the type of the set on the right
        // `[…] + A`: likewise with the dynamic array on the right (`docs/spec/types.md` 8.6)
        let first_set = if ctor_of(&first).is_some()
            && let Some(n) = chain.last()
            && let Some(r) = n.rhs()
            && ctor_of(&r).is_none()
        {
            let is_add = n.op().is_some_and(|t| t.kind() == SyntaxKind::PLUS);
            match self.trial_type(&r) {
                t if t.is_set() => Some(t),
                Ty::DynArray(id) | Ty::OpenArray(id) if is_add => Some(Ty::DynArray(id)),
                _ => None,
            }
        } else {
            None
        };
        // `TCircle = C`: a class as a value next to a class reference (`docs/spec/objects.md` 2.8)
        let first_class = chain
            .last()
            .and_then(|n| n.op())
            .filter(|t| matches!(t.kind(), SyntaxKind::EQ | SyntaxKind::NE))
            .and_then(|_| self.class_name_of(&first))
            .filter(|_| {
                chain.last().and_then(|n| n.rhs()).is_some_and(|r| {
                    self.class_name_of(&r).is_some()
                        || matches!(self.trial_type(&r), Ty::ClassRef(_))
                })
            });
        let mut acc = if first_nil {
            const_expr(Const::Nil, Ty::Error, ast::trimmed_range(first.syntax()))
        } else if let Some(c) = first_class {
            self.class_ref_value(c, ast::trimmed_range(first.syntax()))
        } else if let Some(t) = first_set {
            self.lower_expected(&first, t)
        } else {
            self.lower_expr(&first)
        };
        let mut steps: Vec<BinStep> = Vec::new();
        let mut first_expr: Option<Expr> = None;
        let mut had_error = acc.ty.is_error() && !first_nil;
        for node in chain.iter().rev() {
            let Some(op_tok) = node.op() else {
                had_error = true;
                continue;
            };
            let op_range = op_tok.text_range();
            if matches!(op_tok.kind(), SyntaxKind::IS_KW | SyntaxKind::AS_KW) {
                // `X is T` / `X as T` (`docs/spec/objects.md` 2.9): the right operand is a class
                let target = match node.rhs() {
                    Some(r) => self.class_check_target(&r),
                    None => None,
                };
                if acc.value == Some(Const::Nil) && acc.ty.is_error() {
                    // The leading `nil` of `nil is T`
                    self.error(
                        Code::E(702),
                        "type mismatch: expected an instance of a class or an interface, found `nil`",
                        acc.span,
                    );
                    had_error = true;
                }
                let Some(target) = target.filter(|_| !had_error) else {
                    had_error = true;
                    continue;
                };
                let x = fold_steps(acc, &mut first_expr, &mut steps);
                let step_span = TextRange::new(x.span.start(), node.syntax().text_range().end());
                let op = if op_tok.kind() == SyntaxKind::IS_KW {
                    ClassCheckOp::Is
                } else {
                    ClassCheckOp::As
                };
                acc = self.class_check(x, target, op, step_span, node);
                had_error = acc.ty.is_error();
                continue;
            }
            let nil_cmp = matches!(op_tok.kind(), SyntaxKind::EQ | SyntaxKind::NE);
            let rhs = match node.rhs() {
                // A set constructor after `in` is a set of the left operand's type, and next to a
                // set it has the set's type (`docs/spec/types.md` 8.3)
                // A constructor next to an operand that failed (or another constructor, E0729)
                Some(r)
                    if had_error && op_tok.kind() != SyntaxKind::IN_KW && ctor_of(&r).is_some() =>
                {
                    self.lower_expr_unchecked_ctor(&r);
                    err_expr(ast::trimmed_range(r.syntax()))
                }
                // `X in […]` with `X` of a type parameter (`docs/spec/generics.md` 3.3)
                Some(r)
                    if op_tok.kind() == SyntaxKind::IN_KW
                        && !had_error
                        && matches!(acc.ty, Ty::Param(_))
                        && ctor_of(&r).is_some() =>
                {
                    let ac = ctor_of(&r).expect("checked above");
                    self.lower_ctor_elements_quietly(&ac);
                    let rr = ast::trimmed_range(r.syntax());
                    let Ty::Param(p) = acc.ty else {
                        unreachable!("checked in the guard")
                    };
                    let range = TextRange::new(acc.span.start(), rr.end());
                    self.param_misuse(p, super::generics::ParamOp::Operator("in"), range);
                    had_error = true;
                    err_expr(rr)
                }
                Some(r) if op_tok.kind() == SyntaxKind::IN_KW && ctor_of(&r).is_some() => {
                    let ac = ctor_of(&r).expect("checked above");
                    let lt = if had_error { Ty::Error } else { acc.ty };
                    self.lower_in_ctor(&ac, lt, ast::trimmed_range(r.syntax()))
                }
                Some(r) if acc.ty.is_set() && ctor_of(&r).is_some() => {
                    self.lower_expected(&r, acc.ty)
                }
                // `A + […]`: the constructor takes the type of the dynamic array
                Some(r)
                    if acc.ty.is_dyn_indexed()
                        && op_tok.kind() == SyntaxKind::PLUS
                        && ctor_of(&r).is_some() =>
                {
                    let t = match acc.ty {
                        Ty::OpenArray(id) => Ty::DynArray(id),
                        t => t,
                    };
                    self.lower_expected(&r, t)
                }
                // `C = TCircle`: a class as a value (`docs/spec/objects.md` 2.8)
                Some(r)
                    if nil_cmp
                        && matches!(acc.ty, Ty::ClassRef(_))
                        && self.class_name_of(&r).is_some() =>
                {
                    let c = self.class_name_of(&r).expect("checked above");
                    self.class_ref_value(c, ast::trimmed_range(r.syntax()))
                }
                // `A = nil` / `A <> nil`: `nil` takes the type of the dynamic array
                Some(r) if nil_cmp && self.takes_nil_ty(acc.ty) && is_nil(&r) => {
                    const_expr(Const::Nil, acc.ty, ast::trimmed_range(r.syntax()))
                }
                // `nil` next to an operand that cannot take it: the operand gives the expected
                // type; next to an operand that failed, nothing more is reported
                Some(n) if is_nil(&n) => {
                    let r = ast::trimmed_range(n.syntax());
                    if !had_error {
                        if acc.value == Some(Const::Nil) && acc.ty.is_error() {
                            // `nil = nil`: reported once, at the leading `nil`
                            self.nil_without_type(acc.span);
                            had_error = true;
                        } else if self.takes_nil_ty(acc.ty) {
                            self.nil_without_type(r);
                        } else {
                            self.nil_compare_mismatch(acc.ty, r);
                        }
                    }
                    err_expr(r)
                }
                Some(r) => self.lower_expr(&r),
                None => err_expr(TextRange::empty(op_range.end())),
            };
            if acc.value == Some(Const::Nil) && acc.ty.is_error() {
                // The leading `nil` of `nil = A`
                if nil_cmp && self.takes_nil_ty(rhs.ty) {
                    acc.ty = rhs.ty;
                } else {
                    // An operand that failed has been reported (and `nil = nil` above)
                    if self.takes_nil_ty(rhs.ty) {
                        self.nil_without_type(acc.span);
                    } else if !rhs.ty.is_error() {
                        self.nil_compare_mismatch(rhs.ty, acc.span);
                    }
                    had_error = true;
                }
            }
            let Some(op) = binop_of(op_tok.kind()) else {
                // The parser makes binary expressions only of the operators above
                had_error = true;
                continue;
            };
            if had_error || rhs.ty.is_error() {
                had_error = true;
                continue;
            }
            // An operand whose type may declare operators (`docs/spec/objects.md` 11.3)
            let rhs =
                if self.operator_owner(acc.ty).is_some() || self.operator_owner(rhs.ty).is_some() {
                    let lhs = fold_steps(acc, &mut first_expr, &mut steps);
                    match self.binary_operator(lhs, op, rhs, op_range) {
                        Ok(x) => {
                            had_error = x.ty.is_error();
                            acc = x;
                            continue;
                        }
                        Err(operands) => {
                            let (lhs, rhs) = *operands;
                            acc = lhs;
                            rhs
                        }
                    }
                } else {
                    rhs
                };
            // An operand of a distinct type is a value of its root type for the built-in
            // operations; a user's operator was chosen above with the distinct type
            // (`docs/spec/types.md` 1.3)
            let rhs = self.rooted(rhs);
            acc = self.rooted(acc);
            // A real or `Currency` operation converts its left operand to the computation type
            // (`docs/spec/types.md` 8.4); after earlier steps, they become the left operand
            if let Some(calc) = real_calc_type(acc.ty, op, rhs.ty)
                && acc.ty != calc
            {
                acc = fold_steps(acc, &mut first_expr, &mut steps);
                acc = self.convert_num(acc, calc);
                if acc.ty.is_error() {
                    had_error = true;
                    continue;
                }
            }
            // A `UTF8String` on the left of a `string` or `Char` is converted to `string`
            // (`docs/spec/types.md` 8.5), as a real operation converts its left operand
            if acc.ty == Ty::Utf8String
                && matches!(rhs.ty, Ty::String | Ty::Char)
                && (op == BinOp::Add || op.is_comparison())
            {
                acc = fold_steps(acc, &mut first_expr, &mut steps);
                acc = self.convert_u8(acc, Ty::String);
            }
            let step_span = TextRange::new(acc.span.start(), rhs.span.end());
            let keeps_wide = keeps_wide_int_type(&acc) || keeps_wide_int_type(&rhs);
            match self.binary_step(&acc, op, rhs, op_range, node) {
                Some((step, value)) => {
                    let ty = step.ty;
                    if let Some(v) = value {
                        // If everything so far is constant, fold it and use it as the left operand of the next step (3.6)
                        acc = folded_const(v, ty, &step.calc, op, keeps_wide, step_span);
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

    /// The class or interface after `is` / `as` (`docs/spec/objects.md` 2.9, 7.6). Anything else
    /// is E0702.
    fn class_check_target(&mut self, r: &ast::Expr) -> Option<super::types::TypeId> {
        let range = ast::trimmed_range(r.syntax());
        // A name that is a type is resolved again for its own diagnostics (as a value it would be
        // E0708); anything else is typed as a value below
        let is_type = matches!(r, ast::Expr::NameRef(_) | ast::Expr::Field(_))
            && matches!(
                self.dry_run(|s| s.resolve_name_expr(r)),
                Some((Resolved::Type(_), ..))
            );
        let resolved = if is_type {
            self.resolve_name_expr(r)
        } else {
            None
        };
        let found = match resolved {
            Some((Resolved::Type(Ty::Class(id) | Ty::Intf(id)), ..)) => return Some(id),
            Some((Resolved::Type(Ty::Error), ..)) => return None,
            Some((Resolved::Type(t), ..)) => format!("`{}`", self.tn(t)),
            _ => {
                let e = self.lower_expr(r);
                if e.ty.is_error() {
                    return None;
                }
                format!("a value of type `{}`", self.tn(e.ty))
            }
        };
        self.error(
            Code::E(702),
            format!("type mismatch: expected a class or an interface, found {found}"),
            range,
        );
        None
    }

    /// `x is T` / `x as T` of a class or an interface `T` (`docs/spec/objects.md` 2.9, 7.6).
    /// `span` is the whole expression; `node` gives the operator's place for `X <> nil`.
    fn class_check(
        &mut self,
        x: Expr,
        class: super::types::TypeId,
        op: ClassCheckOp,
        span: TextRange,
        node: &ast::BinaryExpr,
    ) -> Expr {
        let target = self.class_or_intf(class);
        // A value of a type parameter is seen as its constraint's class (any class or
        // interface constraint for an interface; `docs/spec/generics.md` 3.3)
        let x = match x.ty {
            Ty::Param(p) => {
                let view = if self.types.is_interface(class) {
                    self.param_reference(x.ty)
                } else {
                    self.param_class(p).map(Ty::Class)
                };
                match view {
                    Some(v) => Expr { ty: v, ..x },
                    None => {
                        let what = match op {
                            ClassCheckOp::Is => super::generics::ParamOp::TypeTest,
                            _ => super::generics::ParamOp::Convert,
                        };
                        self.param_misuse(p, what, span);
                        return err_expr(span);
                    }
                }
            }
            _ => x,
        };
        if !matches!(x.ty, Ty::Class(_) | Ty::Intf(_)) {
            let d = self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected an instance of a class or an interface, found `{}`",
                    self.tn(x.ty)
                ),
                x.span,
            );
            if matches!(x.ty, Ty::ClassRef(_)) {
                d.notes.push(
                    "check a class reference with `InheritsFrom`, and convert it with the name of a class reference type, as in `TShapeClass(C)`".into(),
                );
            }
            return err_expr(span);
        }
        let Some(holds) = self.check_holds_statically(x.ty, target, span) else {
            return err_expr(span);
        };
        if holds {
            // To an ancestor (or an implemented interface, or `TObject` from an interface): `is`
            // is `X <> nil`, `as` does nothing (2.9, 7.6)
            if op == ClassCheckOp::Is {
                let x_ty = x.ty;
                let nil = const_expr(Const::Nil, x_ty, TextRange::empty(span.end()));
                let op_range = node.op().map_or(span, |t| t.text_range());
                return match self.binary_step(&x, BinOp::Ne, nil, op_range, node) {
                    Some((step, _)) => Expr {
                        kind: ExprKind::Binary {
                            first: Box::new(x),
                            steps: vec![step],
                        },
                        ty: Ty::Boolean,
                        value: None,
                        span,
                    },
                    None => err_expr(span),
                };
            }
            let mut e = x;
            e.ty = target;
            e.span = span;
            return e;
        }
        Expr {
            kind: ExprKind::ClassCheck {
                operand: Box::new(x),
                class,
                op,
            },
            ty: if op == ClassCheckOp::Is {
                Ty::Boolean
            } else {
                target
            },
            value: None,
            span,
        }
    }

    /// The type of a class or an interface by its type id.
    pub(super) fn class_or_intf(&self, id: super::types::TypeId) -> Ty {
        if self.types.is_interface(id) {
            Ty::Intf(id)
        } else {
            Ty::Class(id)
        }
    }

    /// Whether a check (`is`, `as` or a conversion by type name) of a value of the class or
    /// interface type `from` against the class or interface `to` holds without looking at the
    /// instance (`Some(true)`), is decided at run time (`Some(false)`), or can never hold (E0702,
    /// `None`) (`docs/spec/objects.md` 2.9, 7.6).
    pub(super) fn check_holds_statically(
        &mut self,
        from: Ty,
        to: Ty,
        span: TextRange,
    ) -> Option<bool> {
        let sealed = |s: &Self, c| {
            matches!(
                s.types.get(c).kind,
                super::types::TypeKind::Class { sealed: true, .. }
            )
        };
        let never = match (from, to) {
            (Ty::Class(a), Ty::Class(b)) => {
                if self.types.is_descendant(a, b) {
                    return Some(true);
                }
                !self.types.is_descendant(b, a)
            }
            // A descendant of a class that does not implement the interface may (unless the class
            // is sealed)
            (Ty::Class(a), Ty::Intf(b)) => {
                if self.types.implements(a, b) {
                    return Some(true);
                }
                sealed(self, a)
            }
            (Ty::Intf(a), Ty::Intf(b)) => return Some(self.types.is_descendant(a, b)),
            // Every instance is a `TObject`; an instance of a sealed class held by the interface
            // implements it
            (Ty::Intf(a), Ty::Class(b)) => {
                if b == super::types::TOBJECT {
                    return Some(true);
                }
                sealed(self, b) && !self.types.implements(b, a)
            }
            _ => return Some(false),
        };
        if never {
            self.never_be(from, to, span);
            return None;
        }
        Some(false)
    }

    /// The E0702 of a check between classes (or class references) neither of which descends from
    /// the other (`docs/spec/objects.md` 2.8, 2.9).
    fn never_be(&mut self, from: Ty, to: Ty, span: TextRange) {
        self.error(
            Code::E(702),
            format!("`{}` can never be `{}`", self.tn(from), self.tn(to)),
            span,
        );
    }

    /// The conversion by type name `T(x)` of an instance, an interface value or a class reference
    /// (`docs/spec/objects.md` 2.8, 2.9, 7.6): to an ancestor nothing, otherwise checked at run
    /// time as `as`. Gives `x` back (`Err`) if neither type is a class, an interface or a class
    /// reference.
    fn class_cast(&mut self, x: Expr, t: Ty, span: TextRange) -> Result<Expr, Expr> {
        let (from, to) = match (x.ty, t) {
            (Ty::Class(_) | Ty::Intf(_), Ty::Class(to) | Ty::Intf(to)) => {
                let Some(holds) = self.check_holds_statically(x.ty, t, span) else {
                    return Ok(err_expr(span));
                };
                if holds {
                    let mut e = x;
                    e.ty = t;
                    e.span = span;
                    return Ok(e);
                }
                return Ok(Expr {
                    kind: ExprKind::ClassCheck {
                        operand: Box::new(x),
                        class: to,
                        op: ClassCheckOp::Cast,
                    },
                    ty: t,
                    value: None,
                    span,
                });
            }
            (Ty::ClassRef(_), Ty::ClassRef(_)) => {
                let bases = self
                    .types
                    .class_ref_target(x.ty)
                    .zip(self.types.class_ref_target(t));
                let Some((a, b)) = bases else {
                    return Ok(err_expr(span));
                };
                (a, b)
            }
            (Ty::ClassRef(_), _) | (_, Ty::ClassRef(_)) => {
                // Between a class reference and an instance or an integer (2.8)
                self.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected `{}`, found `{}`",
                        self.tn(t),
                        self.tn(x.ty)
                    ),
                    span,
                );
                return Ok(err_expr(span));
            }
            _ => return Err(x),
        };
        if self.types.is_descendant(from, to) {
            let mut e = x;
            e.ty = t;
            e.span = span;
            return Ok(e);
        }
        if !self.types.is_descendant(to, from) {
            self.never_be(x.ty, t, span);
            return Ok(err_expr(span));
        }
        Ok(Expr {
            kind: ExprKind::ClassCheck {
                operand: Box::new(x),
                class: to,
                op: ClassCheckOp::RefCast,
            },
            ty: t,
            value: None,
            span,
        })
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
        if is_fractional(lt) || is_fractional(rt) || op == BinOp::Slash {
            return self.real_step(lhs, op, rhs, op_range, node);
        }
        if lt.is_set() || rt.is_set() || op == BinOp::In {
            return match self.set_step(lhs, op, rhs.clone(), op_range) {
                Some(r) => r,
                None => {
                    self.operator_mismatch(op, lhs, &rhs, op_range, node);
                    None
                }
            };
        }
        let both_int = lt.is_integer() && rt.is_integer();
        let is_strlike = |t: Ty| matches!(t, Ty::String | Ty::Char);
        if lt == Ty::Utf8String && rt == Ty::Utf8String && (op == BinOp::Add || op.is_comparison())
        {
            return self.u8_step(lhs, op, rhs, op_range);
        }
        // A `UTF8String` with a `string` or `Char` works in `string` (`docs/spec/types.md` 8.5;
        // `lower_binary` has converted a `UTF8String` on the left)
        let (lt, rhs) = if lt == Ty::Utf8String && is_strlike(rt) {
            (Ty::String, rhs)
        } else if rt == Ty::Utf8String && is_strlike(lt) {
            (lt, self.convert_u8(rhs, Ty::String))
        } else {
            (lt, rhs)
        };
        let rt = rhs.ty;
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
            if let Ty::Enum(_) = lt
                && lt == rt
            {
                // By ordinal (`docs/spec/types.md` 8.1)
                let value = match (&lhs.value, &rhs.value) {
                    (Some(Const::Int(a)), Some(Const::Int(b))) => {
                        Some(Const::Bool(compare(op, a.cmp(b))))
                    }
                    _ => None,
                };
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Ord,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    value,
                ));
            }
            if matches!(lt, Ty::DynArray(_))
                && lt == rt
                && matches!(op, BinOp::Eq | BinOp::Ne)
                && (lhs.value == Some(Const::Nil) || rhs.value == Some(Const::Nil))
            {
                // A dynamic array compared with `nil` (`docs/spec/types.md` 2.3)
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Ref,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    None,
                ));
            }
            if matches!(op, BinOp::Eq | BinOp::Ne)
                && matches!(
                    (lt, rt),
                    (Ty::Intf(_), Ty::Intf(_) | Ty::Class(_)) | (Ty::Class(_), Ty::Intf(_))
                )
            {
                // The same instance or not, whatever the types (`docs/spec/objects.md` 7.4)
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Ref,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    None,
                ));
            }
            let class_refs = (
                self.types.class_ref_target(lt),
                self.types.class_ref_target(rt),
            );
            let objects = match (lt, rt) {
                (Ty::Class(a), Ty::Class(b)) => Some((a, b)),
                _ => class_refs.0.zip(class_refs.1),
            };
            if let Some((a, b)) = objects
                && matches!(op, BinOp::Eq | BinOp::Ne)
                && (lhs.value == Some(Const::Nil)
                    || rhs.value == Some(Const::Nil)
                    || self.types.is_descendant(a, b)
                    || self.types.is_descendant(b, a))
            {
                // The same object or not (`docs/spec/exceptions.md` chapter 1,
                // `docs/spec/objects.md` 4.5)
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Ref,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    None,
                ));
            }
            if matches!(op, BinOp::Eq | BinOp::Ne) && lt == rt && self.param_reference(lt).is_some()
            {
                // Values of a type parameter whose constraints make it a reference: the same
                // instance or not, whatever its type's operators (`docs/spec/generics.md` 3.3,
                // 4.3)
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Ref,
                        ty: Ty::Boolean,
                        span: op_range,
                    },
                    None,
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
            BinOp::Add if lt.is_dyn_indexed() && rt.is_dyn_indexed() => {
                // Dynamic arrays of one element type (`docs/spec/types.md` 8.6)
                let id = |t: Ty| match t {
                    Ty::DynArray(id) | Ty::OpenArray(id) => Some(id),
                    _ => None,
                };
                if id(lt) != id(rt) {
                    self.operator_mismatch(op, lhs, &rhs, op_range, node);
                    return None;
                }
                let ty = Ty::DynArray(id(lt).expect("a dynamic or open array"));
                return Some((
                    BinStep {
                        op,
                        rhs,
                        calc: Calc::Arr,
                        ty,
                        span: op_range,
                    },
                    None,
                ));
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

    /// `+` and the comparisons of two `UTF8String`s (`docs/spec/types.md` 8.5): bytes, whose order
    /// is the code point order.
    fn u8_step(
        &mut self,
        lhs: &Expr,
        op: BinOp,
        rhs: Expr,
        op_range: TextRange,
    ) -> Option<(BinStep, Option<Const>)> {
        let consts = match (&lhs.value, &rhs.value) {
            (Some(Const::U8(a)), Some(Const::U8(b))) => Some((a.clone(), b.clone())),
            _ => None,
        };
        let (ty, value) = if op == BinOp::Add {
            let value = match consts {
                Some((a, b)) => {
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
                    Some(Const::U8(a + &b))
                }
                None => None,
            };
            (Ty::Utf8String, value)
        } else {
            let value =
                consts.map(|(a, b)| Const::Bool(compare(op, a.as_bytes().cmp(b.as_bytes()))));
            (Ty::Boolean, value)
        };
        Some((
            BinStep {
                op,
                rhs,
                calc: Calc::U8,
                ty,
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
            // A constant whose type comes from its value (a literal, or a constant folded from
            // such constants) and fits the other operand's type is treated as that type; a
            // constant whose type was written (`Int64(1)`, `Integer(1)`) keeps it
            if let Some(Const::Int(v)) = &e.value
                && !keeps_int_type(e)
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
        let msg = match ty {
            // An enumeration or subrange: the bounds as in the range message (`docs/spec/types.md`
            // 8.1, 8.2; the ordinal of an enumeration in decimal)
            // A distinct type: the root's range, the distinct type's name (`docs/spec/types.md` 1.3)
            Ty::Enum(_) | Ty::Subrange(_) | Ty::Distinct(_) => format!(
                "constant value {} is out of range for {}",
                self.types.bound_text(ty, v),
                self.types.range_tail(ty)
            ),
            _ => {
                let (lo, hi) = ty.range().unwrap_or((0, 0));
                format!(
                    "constant value {v} is out of range for `{}` ({lo}..{hi})",
                    self.tn(ty)
                )
            }
        };
        self.error(Code::E(714), msg, range);
    }

    /// Converts an ordinal value whose type is already the base type of `target` (an enumeration
    /// or a subrange) to `target`: a constant is checked at compile time (E0714); otherwise, under
    /// `{$R+}`, a value whose type does not fit is checked at run time (`ChkRange`).
    /// `src` is the type the value had before it was converted to the base type (its range).
    pub(super) fn convert_to_ranged(
        &mut self,
        e: Expr,
        src: Ty,
        target: Ty,
        range_check: bool,
    ) -> Expr {
        let span = e.span;
        let (lo, hi) = self.types.ord_range(target).unwrap_or((0, 0));
        if let Some(v) = e.value.as_ref().and_then(Const::as_int) {
            if v < lo || v > hi {
                self.const_out_of_range(v, target, span);
                return err_expr(span);
            }
            let mut c = e;
            c.ty = target;
            return c;
        }
        let fits = self
            .types
            .ord_range(src)
            .is_some_and(|(slo, shi)| lo <= slo && shi <= hi);
        if fits || !range_check {
            let mut w = e;
            w.ty = target;
            return w;
        }
        Expr {
            kind: ExprKind::Convert {
                operand: Box::new(e),
                kind: Conversion::Range {
                    to: target,
                    check: true,
                },
            },
            ty: target,
            value: None,
            span,
        }
    }

    /// E0703 and its fixes.
    pub(super) fn operator_mismatch(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        op_range: TextRange,
        node: &ast::BinaryExpr,
    ) {
        if let Some(p) = super::Sema::param_of(lhs.ty).or(super::Sema::param_of(rhs.ty)) {
            // `docs/spec/generics.md` 3.3
            let what = match op {
                BinOp::Eq | BinOp::Ne => super::generics::ParamOp::Compare,
                _ => super::generics::ParamOp::Operator(op.text()),
            };
            let range = TextRange::new(lhs.span.start(), rhs.span.end());
            self.param_misuse(p, what, range);
            return;
        }
        let msg = format!(
            "operator `{}` cannot be applied to `{}` and `{}`",
            op.text(),
            self.tn(lhs.ty),
            self.tn(rhs.ty)
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
            if o.is_comparison() || o == BinOp::In {
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

    /// Converts between `string` (or `Char`) and `UTF8String` (`docs/spec/types.md` 8.5). A
    /// constant stays a constant: a `UTF8String` literal is its UTF-8 (lone surrogates become
    /// U+FFFD).
    pub(super) fn convert_u8(&mut self, e: Expr, to: Ty) -> Expr {
        let span = e.span;
        if to == Ty::Utf8String {
            if let Some(units) = e.value.as_ref().and_then(Const::as_str) {
                return const_expr(
                    Const::U8(String::from_utf16_lossy(&units)),
                    Ty::Utf8String,
                    span,
                );
            }
            let e = self.as_string_expr(e);
            return Expr {
                kind: ExprKind::Convert {
                    operand: Box::new(e),
                    kind: Conversion::StrToU8,
                },
                ty: Ty::Utf8String,
                value: None,
                span,
            };
        }
        if let Some(Const::U8(s)) = &e.value {
            return const_expr(Const::Str(s.encode_utf16().collect()), Ty::String, span);
        }
        Expr {
            kind: ExprKind::Convert {
                operand: Box::new(e),
                kind: Conversion::U8ToStr,
            },
            ty: Ty::String,
            value: None,
            span,
        }
    }

    // ---- Assignment compatibility ----

    /// The expression as a value of its root type when its type is a distinct type
    /// (`docs/spec/types.md` 1.3): the operand of an operator, a built-in routine or a statement
    /// that takes the root type. The representation is the same, so nothing is converted.
    pub(super) fn rooted(&self, mut e: Expr) -> Expr {
        if let Ty::Distinct(_) = e.ty {
            e.ty = self.types.root(e.ty);
        }
        e
    }

    /// Converts an expression to type `target` (2.4). E0702 if incompatible. `range_check` is `{$R}`.
    /// `param` is the argument position and parameter name (for the message).
    pub(super) fn coerce(
        &mut self,
        e: Expr,
        target: Ty,
        range_check: bool,
        param: Option<(usize, &str)>,
    ) -> Expr {
        self.coerce_with(e, target, range_check, param, true)
    }

    /// [`Self::coerce`], with `implicit` telling whether a user's `class operator Implicit` may
    /// convert the value (`docs/spec/objects.md` 11.4: not in a condition).
    pub(super) fn coerce_with(
        &mut self,
        e: Expr,
        target: Ty,
        range_check: bool,
        param: Option<(usize, &str)>,
        implicit: bool,
    ) -> Expr {
        // The names for this call's mismatch only (not for the conversions it makes)
        let shown = self.mismatch_shown.take();
        if e.ty.is_error() || target.is_error() || e.ty == target {
            return e;
        }
        if matches!(e.ty, Ty::Distinct(_)) || matches!(target, Ty::Distinct(_)) {
            return self.coerce_distinct(e, target, range_check, param, implicit);
        }
        let span = e.span;
        match (e.ty, target) {
            // Integers, reals and `Currency` to a real or `Currency` (`docs/spec/types.md` 8.4)
            (s, t) if is_fractional(t) && s.is_number() => self.convert_num(e, t),
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
            (Ty::String | Ty::Char, Ty::Utf8String) | (Ty::Utf8String, Ty::String) => {
                self.convert_u8(e, target)
            }
            // Sets of compatible base types (`docs/spec/types.md` 8.3)
            (s, t) if self.sets_compatible(s, t) => self.convert_set(e, t, range_check),
            // A subrange takes what its base type takes, then checks the range (`docs/spec/types.md` 8.2)
            (s, Ty::Subrange(_)) if self.base_accepts(self.types.base(target), s) => {
                let base = self.types.base(target);
                let src = e.ty;
                let inner = self.coerce_with(e, base, range_check, param, implicit);
                if inner.ty.is_error() {
                    return inner;
                }
                self.convert_to_ranged(inner, src, target, range_check)
            }
            // An open array parameter takes a dynamic array, a static array or an open array of
            // the same element type (`docs/spec/types.md` 2.4); the value keeps its own type
            (s, Ty::OpenArray(_)) if s.is_managed() && self.open_array_accepts(target, s) => e,
            // A descendant class to an ancestor (`docs/spec/objects.md` 4.3), a descendant
            // interface to an ancestor, and a class implementing an interface to the interface
            // (7.4): the same handle
            (Ty::Class(s), Ty::Class(t)) | (Ty::Intf(s), Ty::Intf(t))
                if self.types.is_descendant(s, t) =>
            {
                let mut w = e;
                w.ty = target;
                w
            }
            (Ty::Class(s), Ty::Intf(t)) if self.types.implements(s, t) => {
                let mut w = e;
                w.ty = target;
                w
            }
            // Procedural types of the same signature (`docs/spec/objects.md` 9.2)
            (s, t) if self.procedural_assignable(s, t) => self.coerce_procedural(e, target),
            // A descendant's class reference to an ancestor's (`docs/spec/objects.md` 4.3)
            (Ty::ClassRef(_), Ty::ClassRef(_))
                if self
                    .types
                    .class_ref_target(e.ty)
                    .zip(self.types.class_ref_target(target))
                    .is_some_and(|(s, t)| self.types.is_descendant(s, t)) =>
            {
                let mut w = e;
                w.ty = target;
                w
            }
            _ => {
                // A user's `Implicit` (`docs/spec/objects.md` 11.4)
                if implicit {
                    match self.convert_by_operator(&e, target, &["Implicit"]) {
                        Ok(Some(x)) => return x,
                        Ok(None) => {}
                        Err(()) => return err_expr(span),
                    }
                }
                if self.param_upcast(&e, target) {
                    // To a class or interface the constraints guarantee: the same handle
                    // (`docs/spec/generics.md` 3.3)
                    let mut w = e;
                    w.ty = target;
                    return w;
                }
                if let Some(p) = super::Sema::param_of(e.ty).or(super::Sema::param_of(target)) {
                    // `docs/spec/generics.md` 3.3
                    self.param_misuse(p, super::generics::ParamOp::Convert, span);
                    return err_expr(span);
                }
                let (shown_target, shown_from) = shown.unwrap_or((target, e.ty));
                let mut msg = format!(
                    "type mismatch: expected `{}`, found `{}`",
                    self.tn(shown_target),
                    self.tn(shown_from)
                );
                if let Some((n, pname)) = param {
                    msg = format!("{msg} (argument {n}, parameter `{pname}`)");
                }
                let to_int = self.types.base(target).is_integer();
                let from_real = is_fractional(e.ty);
                let downcast = match (e.ty, target) {
                    (Ty::Class(s), Ty::Class(t)) => self.types.is_descendant(t, s),
                    _ => false,
                };
                // `docs/spec/objects.md` 7.4: an interface to a class, and a class that does not
                // implement the interface (a descendant may, unless the class is sealed)
                let intf_note = match (e.ty, target) {
                    (Ty::Intf(_), Ty::Class(_)) => {
                        Some("use `as` to convert an interface to a class")
                    }
                    (Ty::Class(s), Ty::Intf(_))
                        if !matches!(
                            self.types.get(s).kind,
                            super::types::TypeKind::Class { sealed: true, .. }
                        ) =>
                    {
                        Some("use `as` to check at run time")
                    }
                    _ => None,
                };
                let d = self.error(Code::E(702), msg, span);
                if from_real && to_int {
                    d.notes.push(TRUNC_ROUND_NOTE.into());
                }
                if downcast {
                    // An ancestor to a descendant (`docs/spec/objects.md` 4.3)
                    d.notes
                        .push("use `as` to convert to a derived class".into());
                }
                if let Some(n) = intf_note {
                    d.notes.push(n.into());
                }
                err_expr(span)
            }
        }
    }

    /// [`Self::coerce_with`] when either type is a distinct type (`docs/spec/types.md` 1.3): the
    /// value as one of its root type, converted to the root type of `target`; a range checked
    /// there is reported with the name of `target`.
    fn coerce_distinct(
        &mut self,
        e: Expr,
        target: Ty,
        range_check: bool,
        param: Option<(usize, &str)>,
        implicit: bool,
    ) -> Expr {
        let from = e.ty;
        let e = self.rooted(e);
        // A type mismatch names the types as written, not their roots
        self.mismatch_shown = Some((target, from));
        let x = self.coerce_root(e, target, range_check, param, implicit);
        self.mismatch_shown = None;
        x
    }

    /// [`Self::coerce_distinct`] of a value of a root type `e` to `target`.
    fn coerce_root(
        &mut self,
        e: Expr,
        target: Ty,
        range_check: bool,
        param: Option<(usize, &str)>,
        implicit: bool,
    ) -> Expr {
        let Ty::Distinct(_) = target else {
            return self.coerce_with(e, target, range_check, param, implicit);
        };
        let root = self.types.root(target);
        let span = e.span;
        if root.is_integer() && e.ty.is_integer() {
            if let Some(Const::Int(v)) = &e.value {
                if !root.contains(*v) {
                    self.const_out_of_range(*v, target, span);
                    return err_expr(span);
                }
                let mut c = e;
                c.ty = target;
                return c;
            }
            // A check against the range of the root type, named by `target` (as a subrange's).
            // A 64-bit root, and a `UInt64` value, are checked by the conversion to the root,
            // named by the root (`ChkRange` takes a range of at most 2^63 values)
            let (lo, hi) = root.range().expect("an integer type has a range");
            let (slo, shi) = e.ty.range().expect("an integer type has a range");
            let wide = matches!(root, Ty::Int64 | Ty::UInt64) || e.ty == Ty::UInt64;
            if range_check && !(lo <= slo && shi <= hi) && !wide {
                return Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(e),
                        kind: Conversion::Range {
                            to: target,
                            check: true,
                        },
                    },
                    ty: target,
                    value: None,
                    span,
                };
            }
        }
        let mut x = self.coerce_with(e, root, range_check, param, implicit);
        if !x.ty.is_error() {
            x.ty = target;
        }
        x
    }

    /// Whether a value of type `s` is assignment compatible with the ordinal type `base` (the base
    /// type of a subrange; `docs/spec/types.md` 8.2).
    fn base_accepts(&self, base: Ty, s: Ty) -> bool {
        if base.is_integer() {
            s.is_integer()
        } else {
            s == base
        }
    }

    /// Whether an argument of type `arg` can be passed to the open array parameter type `param`:
    /// any array whose element type is the same type (`docs/spec/types.md` 2.4).
    pub(super) fn open_array_accepts(&self, param: Ty, arg: Ty) -> bool {
        let (Some(pe), Some(ae)) = (self.types.dyn_elem(param), self.types.elem_of(arg)) else {
            return false;
        };
        matches!(arg, Ty::DynArray(_) | Ty::Array(_) | Ty::OpenArray(_)) && pe == ae
    }

    // ---- Calls ----

    /// E0725 for a call of the procedure `id` used as a value; the name is written at `range`.
    pub(super) fn routine_no_value(&mut self, id: RoutineId, range: TextRange) {
        let n = self.routines[id.0 as usize].name.clone();
        let mut d = Diagnostic::error(
            Code::E(725),
            format!("procedure `{n}` has no value"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "")
        .with_secondary(self.routine_decl_span(id), "declared here");
        if let Some(note) = self.procedure_value_note(id) {
            d = d.with_note(note);
        }
        self.push(d);
    }

    /// A call written in an expression, whose value is used: a call of a procedure is E0725
    /// (`docs/spec/semantics.md` 4.3, `docs/spec/objects.md` 9.4).
    fn lower_call_expr(&mut self, c: &ast::CallExpr, span: TextRange) -> Expr {
        self.lower_call(c, span, true)
    }

    /// A call `c`; with `value`, its value is used and a call of a procedure is E0725. A statement
    /// (`(P)();`, `F()();`) passes `false`.
    pub(super) fn lower_call(&mut self, c: &ast::CallExpr, span: TextRange, value: bool) -> Expr {
        let Some(callee) = c.callee() else {
            return err_expr(span);
        };
        let args = c.args();
        if !c.has_error_arg()
            && let Some(v) = self.procedural_callee(&callee)
        {
            // A call of a procedural value (`docs/spec/objects.md` 9.4)
            let call = self.call_value(v, &args, span);
            if value
                && let ExprKind::CallValue { callee, .. } = &call.kind
                && self
                    .types
                    .proc_sig(callee.ty)
                    .is_some_and(|(_, _, ret)| ret.is_none())
            {
                let text = self.src(callee.span);
                self.error(
                    Code::E(725),
                    format!("procedure `{text}` has no value"),
                    span,
                );
                return err_expr(span);
            }
            return call;
        }
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
            Resolved::Routine(id) => match self.pick_overload(id, &args, span, false) {
                Pick::Routine(c) => {
                    self.routine_hint_use(c, range);
                    let call = self.lower_routine_call(c, &args, span);
                    if value
                        && matches!(call.kind, ExprKind::Call { .. })
                        && self.routines[c.0 as usize].ret.is_none()
                    {
                        self.routine_no_value(c, range);
                        return err_expr(span);
                    }
                    call
                }
                Pick::Failed => err_expr(span),
            },
            Resolved::Func(f) => self.lower_builtin_any(f, &name, &args, range, span),
            Resolved::Key(op, t) => self.lower_key_call(op, t, &args, span),
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
                    Resolved::Global(id) => match self.global_decl_span(*id) {
                        Some(s) => d.with_secondary(s, "declared here"),
                        None => d,
                    },
                    Resolved::Const { span: Some(s), .. } => d.with_secondary(*s, "declared here"),
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
            Resolved::ClassCreate(id, ctor) => {
                self.new_instance(id, &ctor, &args, span, range, None)
            }
            Resolved::Member {
                recv,
                class,
                member,
                inherited,
            } => self.member_expr(
                *recv,
                class,
                member,
                inherited,
                Some(&args),
                value,
                range,
                span,
            ),
            Resolved::RaiseOuter => {
                for a in &args {
                    for x in a.exprs() {
                        self.lower_expr(&x);
                    }
                }
                self.error(
                    Code::E(725),
                    "procedure `RaiseOuterException` has no value",
                    range,
                );
                err_expr(span)
            }
            Resolved::StdFile(f) => {
                for a in &args {
                    for x in a.exprs() {
                        self.lower_expr(&x);
                    }
                }
                self.std_file_misuse(f, range);
                err_expr(span)
            }
            Resolved::Error => {
                // The name has been diagnosed. Check the arguments, but allow type names (`Low(Integer)`)
                // and do not ask for the type of a constructor (`Format('%d', [X])`)
                self.allow_type_as_value = true;
                for a in &args {
                    for x in a.exprs() {
                        if !self.bare_routine_arg(&x) {
                            self.lower_expr_unchecked_ctor(&x);
                        }
                    }
                }
                self.allow_type_as_value = false;
                err_expr(span)
            }
        }
    }

    /// `Low(X)`, `High(X)` and `Default(T)`: the argument may be a type name (`docs/spec/types.md`
    /// chapter 7). `Low` / `High` fold into constants.
    fn lower_type_query(&mut self, f: Builtin, args: &[ast::Arg], span: TextRange) -> Expr {
        self.check_no_width(args);
        if args.len() != 1 {
            self.allow_type_as_value = true;
            for a in args {
                for x in a.exprs() {
                    self.lower_expr(&x);
                }
            }
            self.allow_type_as_value = false;
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
        let Some(a) = args[0].value() else {
            return err_expr(span);
        };
        let arg_span = ast::trimmed_range(a.syntax());
        let mut value_arg = None;
        let ty = match self.type_name_arg(&a) {
            Some(t) => t,
            None => {
                if f == Builtin::Default {
                    let x = self.lower_expr(&a);
                    if !x.ty.is_error() {
                        let text = self.src(arg_span);
                        self.error(Code::E(708), format!("`{text}` is not a type"), arg_span);
                    }
                    return err_expr(span);
                }
                // A variable of a subrange type gives the subrange's bounds
                let x = self.lower_expr_raw(&a);
                let t = x.ty;
                value_arg = Some(x);
                t
            }
        };
        if ty.is_error() {
            return err_expr(span);
        }
        // `Low` / `High` of a distinct type are of the distinct type, with the bounds of its
        // root; `SizeOf` is its root's (`docs/spec/types.md` 1.3)
        let distinct = matches!(ty, Ty::Distinct(_)).then_some(ty);
        let ty = if f == Builtin::Default {
            ty
        } else {
            self.types.root(ty)
        };
        let value_arg = value_arg.map(|x| self.rooted(x));
        if let (Builtin::SizeOf, Ty::Param(p)) = (f, ty) {
            // `docs/spec/generics.md` 6.1
            self.param_misuse(p, super::generics::ParamOp::SizeOf, span);
            return err_expr(span);
        }
        if f == Builtin::SizeOf {
            // A constant; the argument is not evaluated (`docs/spec/semantics.md` 5.3)
            return match self.size_of(ty) {
                Some(n) => const_expr(Const::Int(n), Ty::Integer, span),
                None => {
                    let t = self.tn(ty);
                    self.error(
                        Code::E(3200),
                        format!("`SizeOf({t})` is not supported yet (planned for M8)"),
                        span,
                    );
                    err_expr(span)
                }
            };
        }
        if f != Builtin::Default && matches!(ty, Ty::String | Ty::Utf8String) {
            // `Low` is 1 and `High` is `Length(S)` (`docs/spec/semantics.md` 5.3)
            let Some(x) = value_arg else {
                let t = self.tn(ty);
                self.error(
                    Code::E(702),
                    format!("type mismatch: expected an ordinal type or an array, found the type `{t}` (argument 1, parameter `X`)"),
                    arg_span,
                );
                return err_expr(span);
            };
            if f == Builtin::Low {
                return const_expr(Const::Int(1), Ty::Integer, span);
            }
            let n = match &x.value {
                Some(Const::U8(s)) => Some(s.len()),
                Some(c) => c.as_str().map(|s| s.len()),
                None => None,
            };
            if let Some(n) = n {
                return const_expr(Const::Int(n as i128), Ty::Integer, span);
            }
            return Expr {
                kind: ExprKind::Builtin {
                    func: Builtin::Length,
                    args: vec![x],
                },
                ty: Ty::Integer,
                value: None,
                span,
            };
        }
        if f != Builtin::Default && ty.is_dyn_indexed() {
            // A dynamic or open array: `Low` is 0, `High` is `Length - 1` (`docs/spec/types.md`
            // chapter 7)
            let Some(x) = value_arg else {
                let t = self.tn(ty);
                self.error(
                    Code::E(702),
                    format!("type mismatch: expected an ordinal type or an array, found the type `{t}` (argument 1, parameter `X`)"),
                    arg_span,
                );
                return err_expr(span);
            };
            if f == Builtin::Low {
                return const_expr(Const::Int(0), Ty::Integer, span);
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
        if f == Builtin::Default {
            return Expr {
                kind: ExprKind::Builtin {
                    func: f,
                    args: Vec::new(),
                },
                ty,
                value: None,
                span,
            };
        }
        let low = f == Builtin::Low;
        let (vty, lo, hi) = if let Some((index, lo, hi, _)) = self.types.array(ty) {
            (index, lo, hi)
        } else if let (true, Some((lo, hi))) = (ty.is_ordinal(), self.types.ord_range(ty)) {
            (self.types.base(ty), lo, hi)
        } else {
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected an ordinal type or an array, found `{}` (argument 1, parameter `X`)",
                    self.tn(ty)
                ),
                arg_span,
            );
            return err_expr(span);
        };
        let v = if low { lo } else { hi };
        let value = match vty {
            Ty::Char => Const::Char(v as u16),
            Ty::Boolean => Const::Bool(v != 0),
            _ => Const::Int(v),
        };
        explicit_const(value, distinct.unwrap_or(vty), span)
    }

    /// `SizeOf(T)` (`docs/spec/semantics.md` 5.3), or `None` for a type without a memory layout
    /// (strings, arrays, records, sets, exceptions; E3200).
    fn size_of(&self, ty: Ty) -> Option<i128> {
        // The smallest of 1 / 2 / 4 / 8 bytes holding the ordinals `lo..=hi`
        let fits = |lo: i128, hi: i128| -> i128 {
            [1u32, 2, 4, 8]
                .into_iter()
                .find(|&n| {
                    let bits = 8 * n;
                    if lo < 0 {
                        lo >= -(1i128 << (bits - 1)) && hi < (1i128 << (bits - 1))
                    } else {
                        hi < (1i128 << bits)
                    }
                })
                .unwrap_or(8) as i128
        };
        Some(match ty {
            Ty::ShortInt | Ty::Byte | Ty::Boolean => 1,
            Ty::SmallInt | Ty::Word | Ty::Char => 2,
            Ty::Integer | Ty::Cardinal | Ty::Single => 4,
            Ty::Int64 | Ty::UInt64 | Ty::Double | Ty::Currency => 8,
            // An enumeration by its largest ordinal: 1, 2 or 4 bytes
            Ty::Enum(_) => {
                let (_, hi) = self.types.ord_range(ty)?;
                fits(0, hi).min(4)
            }
            Ty::Subrange(_) => match self.types.base(ty) {
                Ty::Char => 2,
                Ty::Boolean => 1,
                base => {
                    let (lo, hi) = self.types.ord_range(ty)?;
                    if let Ty::Enum(_) = base {
                        fits(0, hi).min(4)
                    } else {
                        fits(lo, hi)
                    }
                }
            },
            _ => return None,
        })
    }

    /// `Concat(…)` (`docs/spec/semantics.md` 5.3, `docs/spec/types.md` 8.6): the same as `+` from
    /// the left. Strings (a `Char` becomes a `string`; all `UTF8String`s stay `UTF8String`), or
    /// dynamic arrays of one element type (an array constructor takes the type of the others).
    fn concat(&mut self, args: &[ast::Arg], span: TextRange) -> Expr {
        self.check_no_width(args);
        let exprs: Vec<ast::Expr> = args.iter().filter_map(|a| a.value()).collect();
        if exprs.len() != args.len() {
            for x in &exprs {
                self.lower_expr_unchecked_ctor(x);
            }
            return err_expr(span);
        }
        if exprs.is_empty() {
            self.error(
                Code::E(705),
                "wrong number of arguments: expected at least 1, found 0",
                span,
            );
            return err_expr(span);
        }
        // The array type comes from the first argument that is not a constructor
        let arr = exprs
            .iter()
            .filter(|e| !super::stmt::is_contextual(e))
            .map(|e| self.trial_type(e))
            .find(|t| t.is_dyn_indexed());
        let arr = arr.map(|t| match t {
            Ty::OpenArray(id) => Ty::DynArray(id),
            t => t,
        });
        let mut lowered = Vec::new();
        for e in &exprs {
            let x = match arr {
                Some(t) => self.lower_expected(e, t),
                None => self.lower_expr(e),
            };
            lowered.push(x);
        }
        if lowered.iter().any(|x| x.ty.is_error()) {
            return err_expr(span);
        }
        if let Some(arr) = arr {
            let mut out = Vec::new();
            for (i, x) in lowered.into_iter().enumerate() {
                let same = match x.ty {
                    Ty::DynArray(id) | Ty::OpenArray(id) => Ty::DynArray(id) == arr,
                    _ => false,
                };
                if !same {
                    let r = x.span;
                    self.error(
                        Code::E(702),
                        format!(
                            "type mismatch: expected `{}`, found `{}` (argument {}, parameter `S`)",
                            self.tn(arr),
                            self.tn(x.ty),
                            i + 1
                        ),
                        r,
                    );
                    return err_expr(span);
                }
                out.push(x);
            }
            if out.len() == 1 {
                // A copy, as `+` makes a new array
                let x = out.pop().expect("one argument");
                let elem = self.types.elem_of(x.ty).unwrap_or(Ty::Error);
                return self.array_copy(vec![x], elem, span);
            }
            return self.concat_chain(out, Calc::Arr, arr, span);
        }
        let all_u8 = lowered.iter().all(|x| x.ty == Ty::Utf8String);
        let target = if all_u8 { Ty::Utf8String } else { Ty::String };
        let mut out = Vec::new();
        for (i, x) in lowered.into_iter().enumerate() {
            let x = if matches!(x.ty, Ty::String | Ty::Char | Ty::Utf8String) {
                self.coerce(x, target, false, Some((i + 1, "S")))
            } else {
                let r = x.span;
                self.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected `string`, found `{}` (argument {}, parameter `S`)",
                        self.tn(x.ty),
                        i + 1
                    ),
                    r,
                );
                return err_expr(span);
            };
            if x.ty.is_error() {
                return err_expr(span);
            }
            out.push(x);
        }
        // Constants fold (`docs/spec/semantics.md` 3.6)
        if out.iter().all(|x| x.value.is_some()) {
            let total: usize = out
                .iter()
                .map(|x| match &x.value {
                    Some(Const::U8(s)) => s.len(),
                    Some(c) => c.as_str().map_or(0, |s| s.len()),
                    None => 0,
                })
                .sum();
            if total > MAX_STRING_LEN {
                self.error(
                    Code::E(726),
                    "string constant is too long (limit 2147483647 characters)",
                    span,
                )
                .notes
                .push("build the string at run time instead".into());
                return err_expr(span);
            }
            let value = if all_u8 {
                Const::U8(
                    out.iter()
                        .filter_map(|x| match &x.value {
                            Some(Const::U8(s)) => Some(s.as_str()),
                            _ => None,
                        })
                        .collect(),
                )
            } else {
                Const::Str(
                    out.iter()
                        .flat_map(|x| x.value.as_ref().and_then(Const::as_str).unwrap_or_default())
                        .collect(),
                )
            };
            return const_expr(value, target, span);
        }
        if out.len() == 1 {
            let mut x = out.pop().expect("one argument");
            x.span = span;
            return x;
        }
        let calc = if all_u8 { Calc::U8 } else { Calc::Str };
        self.concat_chain(out, calc, target, span)
    }

    /// `first + rest[0] + …` with every step in `calc`, of type `ty`.
    fn concat_chain(&mut self, mut args: Vec<Expr>, calc: Calc, ty: Ty, span: TextRange) -> Expr {
        let rest = args.split_off(1);
        let first = args.pop().expect("at least two arguments");
        let steps = rest
            .into_iter()
            .map(|rhs| BinStep {
                op: BinOp::Add,
                span: rhs.span,
                rhs,
                calc,
                ty,
            })
            .collect();
        Expr {
            kind: ExprKind::Binary {
                first: Box::new(first),
                steps,
            },
            ty,
            value: None,
            span,
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
        let min = self.routines[id.0 as usize].min_args();
        if args.len() < min || args.len() > params.len() {
            if self.lower_miscounted_args(args) {
                return err_expr(span);
            }
            let expected = if min == params.len() {
                min.to_string()
            } else {
                format!("{min} to {}", params.len())
            };
            let d = Diagnostic::error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {expected}, found {}",
                    args.len()
                ),
                Stage::Sema,
            )
            .with_primary(self.span(span), "");
            let d = self.with_routine_decl(d, id, "declared here");
            self.push(d);
            return err_expr(span);
        }
        let mut out: Vec<CallArg> = Vec::new();
        let mut failed = false;
        let checks = self.checks_at(span.start());
        // The `var` / `out` arguments: (argument number, place, written range)
        let mut refs: Vec<(usize, Place, TextRange)> = Vec::new();
        for (i, (a, p)) in args.iter().zip(&params).enumerate() {
            let Some(v) = a.value() else {
                failed = true;
                continue;
            };
            match p.mode {
                ParamMode::Var | ParamMode::Out => {
                    let is_out = p.mode == ParamMode::Out;
                    match self.ref_arg(&v, (i + 1, &p.name), p.ty, is_out, Some(p.span)) {
                        Some(place) => {
                            refs.push((i + 1, place.clone(), ast::trimmed_range(v.syntax())));
                            out.push(CallArg::Ref { place, is_out });
                        }
                        None => failed = true,
                    }
                }
                _ => {
                    let e = self.lower_expected(&v, p.ty);
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
        // Omitted arguments take the default values (`docs/spec/semantics.md` 4.8)
        for p in &params[args.len()..] {
            let mut d = p
                .default
                .clone()
                .expect("only parameters with defaults are omitted");
            if d.ty.is_error() {
                failed = true;
            }
            d.span = span;
            out.push(CallArg::Value(d));
        }
        if failed {
            return err_expr(span);
        }
        if self.same_location_args(&refs) {
            return err_expr(span);
        }
        let _ = name;
        // A closure-form nested routine gets its group's block (`docs/spec/objects.md` 14.9)
        let dispatch = match self.routines[id.0 as usize].closure_info.group {
            Some(_) => Dispatch::Group(self.group_block(id)),
            None => Dispatch::Routine,
        };
        // For the static link (`docs/spec/semantics.md` 4.9)
        if self.dry_run == 0
            && let Some(c) = &mut self.cur
            && !c.calls.contains(&id)
        {
            c.calls.push(id);
        }
        Expr {
            kind: ExprKind::Call {
                routine: id,
                args: out,
                dispatch,
            },
            ty: ret.unwrap_or(Ty::Error),
            value: None,
            span,
        }
    }

    /// E0741 for the first `var` / `out` argument known to be the same location as an earlier one
    /// (`docs/spec/types.md` chapter 3): the same variable, or the same path whose indices are equal
    /// constants or the same variable. Returns whether it was reported.
    fn same_location_args(&mut self, refs: &[(usize, Place, TextRange)]) -> bool {
        for (k, (m, b, rb)) in refs.iter().enumerate() {
            let Some((n, _, ra)) = refs[..k].iter().find(|(_, a, _)| same_location(a, b)) else {
                continue;
            };
            let mut d = Diagnostic::error(
                Code::E(741),
                format!(
                    "the same location is passed to two `var` or `out` parameters (arguments {n} and {m})"
                ),
                Stage::Sema,
            )
            .with_primary(self.span(*rb), "")
            .with_secondary(self.span(*ra), "also passed here")
            .with_note("Puy does not let two `var` or `out` parameters refer to the same location");
            d = if matches!(b, Place::Elem { .. }) {
                d.with_note(
                    "copy the value to a local variable and pass that, or use `Exchange` to swap \
                     two array elements",
                )
            } else {
                d.with_note("copy the value to a local variable and pass that")
            };
            self.push(d);
            return true;
        }
        false
    }

    /// The place of an argument for a `var` / `out` parameter `param` (its number and name) of
    /// type `ty`: E0702 unless the variable has exactly that type (an open array parameter takes
    /// a matching array), E0706 if the argument is not a variable (`decl`: the parameter's
    /// declaration, for the secondary label). `None` on an error.
    pub(super) fn ref_arg(
        &mut self,
        v: &ast::Expr,
        param: (usize, &str),
        ty: Ty,
        is_out: bool,
        decl: Option<TextRange>,
    ) -> Option<Place> {
        let (place, found) = self.ref_place(v, param, is_out, decl)?;
        let open_ok = matches!(ty, Ty::OpenArray(_)) && self.open_array_accepts(ty, found);
        if !found.is_error() && !ty.is_error() && found != ty && !open_ok {
            let mode = if is_out { "an `out`" } else { "a `var`" };
            let r = ast::trimmed_range(v.syntax());
            let d = Diagnostic::error(
                Code::E(702),
                format!(
                    "type mismatch: expected `{}`, found `{}` (argument {}, parameter `{}`)",
                    self.tn(ty),
                    self.tn(found),
                    param.0,
                    param.1
                ),
                Stage::Sema,
            )
            .with_primary(self.span(r), "")
            .with_note(format!(
                "{mode} parameter needs a variable of exactly the same type"
            ));
            self.push(d);
            return None;
        }
        Some(place)
    }

    /// The place and type of an argument for a `var` / `out` parameter `param` (its number and
    /// name), without checking the type: E0706 if the argument is not a variable (`decl`: the
    /// parameter's declaration, for the secondary label). `None` on an error.
    pub(super) fn ref_place(
        &mut self,
        v: &ast::Expr,
        param: (usize, &str),
        is_out: bool,
        decl: Option<TextRange>,
    ) -> Option<(Place, Ty)> {
        let result = self.lower_place(v, PlaceUse::RefArg { is_out });
        if let PlaceResult::NotVariable(r) =
            self.last_place_result.take().unwrap_or(PlaceResult::Ok)
        {
            let mode = if is_out { "out" } else { "var" };
            let mut d = Diagnostic::error(
                Code::E(706),
                format!(
                    "argument {} must be a variable (parameter `{}` is `{mode}`)",
                    param.0, param.1
                ),
                Stage::Sema,
            )
            .with_primary(self.span(r), "");
            if let Some(s) = decl {
                d = d.with_secondary(self.span(s), "declared here");
            }
            self.push(d);
        }
        result
    }

    /// A call to a built-in function, in an expression or as a statement: the built-ins whose
    /// arguments are not plain values have their own lowering.
    pub(super) fn lower_builtin_any(
        &mut self,
        f: Builtin,
        name: &str,
        args: &[ast::Arg],
        name_range: TextRange,
        span: TextRange,
    ) -> Expr {
        match f {
            Builtin::Low | Builtin::High | Builtin::Default | Builtin::SizeOf => {
                self.lower_type_query(f, args, span)
            }
            Builtin::Concat => self.concat(args, span),
            Builtin::Format => self.format_call(args, span),
            Builtin::TryStrToInt => self.try_str_to_int(args, span),
            Builtin::Supports => self.supports(args, span),
            Builtin::Eof | Builtin::Eoln => self.eof_call(f, args, name_range, span),
            _ => self.lower_builtin_call(f, name, args, name_range, span),
        }
    }

    /// A built-in function call without arguments (after `eof_call` removed `Input`).
    pub(super) fn lower_builtin_any_plain(
        &mut self,
        f: Builtin,
        name: &str,
        name_range: TextRange,
        span: TextRange,
    ) -> Expr {
        self.lower_builtin_call(f, name, &[], name_range, span)
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

    /// `IsOverridden(M)` (`docs/spec/collections.md` 12.1, `docs/spec/bytecode.md` 2.22): `M` is
    /// the name of a virtual instance method of the class whose method is being analyzed. Only
    /// the units of the standard library see the name.
    fn is_overridden(&mut self, args: &[ast::Arg], span: TextRange) -> Expr {
        let name = match args {
            [a] => match a.value() {
                Some(ast::Expr::NameRef(n)) => Some(n.text().to_string()),
                _ => None,
            },
            _ => None,
        };
        // The one method of the name, which must be a virtual instance method (an overloaded
        // name would leave open which entry is meant)
        let found = name.as_deref().and_then(|name| {
            // `Self` is an instance (not the class reference of a class method)
            if self.method_self_kind() != Some(SelfKind::Instance) {
                return None;
            }
            let class = self.method_class()?;
            match self.methods_named(class, name).as_slice() {
                [(_, m)] => match (m.vmt, m.routine, m.self_kind) {
                    (Some(k), Some(r), SelfKind::Instance) => Some((k, r)),
                    _ => None,
                },
                _ => None,
            }
        });
        let recv = found.and_then(|_| self.self_expr(span));
        let (Some((k, routine)), Some(recv)) = (found, recv) else {
            self.error(
                Code::E(702),
                "type mismatch: expected the name of a virtual method of `Self`".to_string(),
                span,
            );
            return err_expr(span);
        };
        let method = Expr {
            kind: ExprKind::RoutineRef(routine),
            ty: Ty::Int64,
            value: None,
            span,
        };
        Expr {
            kind: ExprKind::Builtin {
                func: Builtin::IsOverridden,
                args: vec![
                    recv,
                    const_expr(Const::Int(i128::from(k)), Ty::Int64, span),
                    method,
                ],
            },
            ty: Ty::Boolean,
            value: None,
            span,
        }
    }

    /// `HashFind` / `HashLookup` / `IsPlainKey` (`docs/spec/bytecode.md` 2.23; the units of the
    /// standard library only): the integers converted to their parameter types, the arrays
    /// checked against theirs, and a key of any type (the keys of `HashLookup` are of its type,
    /// and its arrays four consecutive fields of `Self`); lower expands the call by the type of
    /// the key.
    fn hash_table_call(&mut self, f: Builtin, args: &[ast::Arg], span: TextRange) -> Expr {
        let (sig, ret) = builtin_signature(f);
        if args.len() != sig.len() {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {}, found {}",
                    sig.len(),
                    args.len()
                ),
                span,
            );
            return err_expr(span);
        }
        let checks = self.checks_at(span.start());
        let mut lowered = Vec::new();
        for (i, (a, (name, ty))) in args.iter().zip(sig).enumerate() {
            let Some(v) = a.value() else {
                return err_expr(span);
            };
            let e = if ty.is_error() {
                self.lower_expr(&v)
            } else {
                let e = self.lower_expected(&v, *ty);
                if e.ty.is_error() {
                    return err_expr(span);
                }
                self.coerce(e, *ty, checks.range, Some((i + 1, name)))
            };
            if e.ty.is_error() {
                return err_expr(span);
            }
            lowered.push(e);
        }
        // The arrays: the buckets and the next entries of `Integer`, the hashes of `Int64`, the
        // keys of the type of the key
        let arrays: &[(usize, Option<Ty>)] = match f {
            Builtin::HashFind => &[
                (1, Some(Ty::Integer)),
                (2, Some(Ty::Int64)),
                (3, Some(Ty::Integer)),
            ],
            Builtin::HashLookup => &[
                (1, Some(Ty::Integer)),
                (2, Some(Ty::Int64)),
                (3, Some(Ty::Integer)),
                (4, Some(lowered[0].ty)),
            ],
            _ => &[],
        };
        for &(i, elem) in arrays {
            let e = &lowered[i];
            let fits = matches!(e.ty, Ty::DynArray(_))
                && elem.is_none_or(|t| self.types.elem_of(e.ty) == Some(t));
            if !fits {
                let expected = match elem {
                    Some(t) => format!("a dynamic array of `{}`", self.tn(t)),
                    None => "a dynamic array".into(),
                };
                self.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected {expected}, found `{}` (argument {}, parameter `{}`)",
                        self.tn(e.ty),
                        i + 1,
                        sig[i].0
                    ),
                    e.span,
                );
                return err_expr(span);
            }
        }
        if f == Builtin::HashLookup {
            return self.hash_lookup(args, lowered, span);
        }
        Expr {
            kind: ExprKind::Builtin {
                func: f,
                args: lowered,
            },
            ty: ret,
            value: None,
            span,
        }
    }

    /// `HashLookup(Key, Buckets, Hashes, Next, Keys)` once its arguments have their types
    /// (`docs/spec/bytecode.md` 2.23): the arrays must be fields of `Self` named without `Self.`,
    /// declared one after another in this order (E0702), which the instruction reads from the
    /// slot of the first. The key is passed as `KeyHash(Key)`, which lower evaluates for a key
    /// the instruction does not hash itself.
    fn hash_lookup(&mut self, args: &[ast::Arg], mut lowered: Vec<Expr>, span: TextRange) -> Expr {
        let this = self.self_var().map(|(v, _)| v);
        let mut first = None;
        for (i, (a, e)) in args.iter().zip(&lowered).enumerate().skip(1) {
            let named = matches!(a.value(), Some(ast::Expr::NameRef(_)));
            let slot = match &e.kind {
                ExprKind::ObjField { base, slot, .. }
                    if named
                        && matches!((&base.kind, this), (ExprKind::Var(b), Some(v)) if *b == v) =>
                {
                    Some(*slot)
                }
                _ => None,
            };
            let first = *first.get_or_insert(slot);
            let fits = matches!((first, slot), (Some(f), Some(s)) if s == f + i as u32 - 1);
            if !fits {
                self.error(
                    Code::E(702),
                    "type mismatch: expected four consecutive fields of `Self`".to_string(),
                    e.span,
                );
                return err_expr(span);
            }
        }
        let key = lowered.remove(0);
        self.instances.key_used = true;
        let hash = Expr {
            kind: ExprKind::Key {
                op: KeyOp::Hash,
                args: vec![key],
                func: None,
            },
            ty: Ty::Int64,
            value: None,
            span,
        };
        lowered.insert(0, hash);
        Expr {
            kind: ExprKind::Builtin {
                func: Builtin::HashLookup,
                args: lowered,
            },
            ty: Ty::Integer,
            value: None,
            span,
        }
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
        if f == Builtin::IsOverridden {
            return self.is_overridden(args, span);
        }
        if matches!(
            f,
            Builtin::HashFind | Builtin::HashLookup | Builtin::IsPlainKey | Builtin::IsPlainOrder
        ) {
            return self.hash_table_call(f, args, span);
        }
        let (sig, ret) = builtin_signature(f);
        let shown = builtin_name(f);
        let _ = name;
        // The parameter types (the flags of `StringReplace` are a set, so a constructor takes it)
        let mut params: Vec<Ty> = sig.iter().map(|p| p.1).collect();
        // The optional last argument of the `Ansi*` case routines (`docs/spec/semantics.md` 5.3)
        let option = match f {
            Builtin::StringReplace => {
                params[3] = self.replace_flag_types().1;
                None
            }
            Builtin::AnsiUpperCase | Builtin::AnsiLowerCase => Some(self.case_mapping_type()),
            Builtin::AnsiSameText | Builtin::AnsiCompareText => Some(self.case_fold_type()),
            _ => None,
        };
        if let Some(t) = option {
            *params.last_mut().expect("the option parameter") = t;
        }
        let lowered: Vec<Option<Expr>> = args
            .iter()
            .enumerate()
            .map(|(i, a)| {
                a.value().map(|v| match params.get(i) {
                    Some(t) if t.is_set() => self.lower_expected(&v, *t),
                    // `Assigned(F)` looks at the value (`docs/spec/objects.md` 9.5)
                    _ if f == Builtin::Assigned => self.lower_value(&v),
                    _ => self.lower_expr(&v),
                })
            })
            .collect();
        if lowered
            .iter()
            .any(|e| e.as_ref().is_none_or(|x| x.ty.is_error()))
        {
            return err_expr(span);
        }
        // A distinct type is passed as its root type (`docs/spec/types.md` 1.3)
        let mut lowered: Vec<Expr> = lowered
            .into_iter()
            .map(|e| {
                let e = e.expect("checked above");
                // `Succ` / `Pred` return the distinct type
                if matches!(f, Builtin::Succ | Builtin::Pred) {
                    e
                } else {
                    self.rooted(e)
                }
            })
            .collect();
        if f == Builtin::Copy
            && let Some(elem) = lowered.first().and_then(|a| self.types.elem_of(a.ty))
        {
            return self.array_copy(lowered, elem, span);
        }
        if f == Builtin::Assigned {
            return self.assigned(lowered, span);
        }
        if matches!(
            f,
            Builtin::IsRefCountedObject | Builtin::IsSameInstance | Builtin::ObjectClassName
        ) {
            // `docs/spec/collections.md` 12.1: values of any type; lower expands them by the type
            let expected = if f == Builtin::IsSameInstance { 2 } else { 1 };
            if lowered.len() != expected {
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected {expected}, found {}",
                        lowered.len()
                    ),
                    span,
                );
                return err_expr(span);
            }
            if f == Builtin::IsSameInstance && lowered[0].ty != lowered[1].ty {
                // Only the bodies of the standard library call it, with two values of one type
                self.error(
                    Code::E(702),
                    format!(
                        "type mismatch: expected `{}`, found `{}` (argument 2, parameter `Y`)",
                        self.tn(lowered[0].ty),
                        self.tn(lowered[1].ty)
                    ),
                    lowered[1].span,
                );
                return err_expr(span);
            }
            return Expr {
                kind: ExprKind::Builtin {
                    func: f,
                    args: lowered,
                },
                ty: ret,
                value: None,
                span,
            };
        }
        // `BoolToStr(B)` is `BoolToStr(B, False)`, and the `Ansi*` case routines take the first
        // value of their option type (`docs/spec/semantics.md` 5.3)
        let min = if f == Builtin::BoolToStr || option.is_some() {
            sig.len() - 1
        } else {
            sig.len()
        };
        if lowered.len() < min || lowered.len() > sig.len() {
            let expected = if min == sig.len() {
                min.to_string()
            } else {
                format!("{min} to {}", sig.len())
            };
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {expected}, found {}",
                    lowered.len()
                ),
                span,
            );
            return err_expr(span);
        }
        if lowered.len() < sig.len() {
            lowered.push(match option {
                Some(t) => const_expr(Const::Int(0), t, span),
                None => const_expr(Const::Bool(false), Ty::Boolean, span),
            });
        }
        let checks = self.checks_at(span.start());
        match f {
            Builtin::Ord => {
                let x = lowered.pop().unwrap();
                match x.ty {
                    Ty::Char | Ty::Boolean | Ty::Enum(_) => {
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
                    Ty::Param(p) => {
                        // `docs/spec/generics.md` 3.3
                        self.param_misuse(p, super::generics::ParamOp::Ordinal, span);
                        return err_expr(span);
                    }
                    t => {
                        let r = x.span;
                        self.error(Code::E(702), format!("type mismatch: expected an ordinal type, found `{}` (argument 1, parameter `X`)", self.tn(t)), r);
                        return err_expr(span);
                    }
                }
            }
            Builtin::Succ | Builtin::Pred => {
                let x = lowered.pop().unwrap();
                return self.succ_pred(x, f == Builtin::Pred, span, checks);
            }
            Builtin::Abs => {
                let x = lowered.pop().unwrap();
                if is_fractional(x.ty) {
                    return self.real_abs(x, span);
                }
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected a number type, found `{}` (argument 1, parameter `X`)", self.tn(x.ty)), r);
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
            Builtin::Pi => {
                return const_expr(Const::real(std::f64::consts::PI), Ty::Double, span);
            }
            Builtin::Odd => {
                let x = lowered.pop().unwrap();
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected an integer type, found `{}` (argument 1, parameter `X`)", self.tn(x.ty)), r);
                    return err_expr(span);
                }
                if let Some(Const::Int(v)) = &x.value {
                    return const_expr(Const::Bool(v.rem_euclid(2) == 1), Ty::Boolean, span);
                }
                return Expr {
                    kind: ExprKind::Builtin {
                        func: f,
                        args: vec![x],
                    },
                    ty: Ty::Boolean,
                    value: None,
                    span,
                };
            }
            Builtin::UpCase => {
                let x = lowered.pop().unwrap();
                let x = self.coerce(x, Ty::Char, false, Some((1, "Ch")));
                if x.ty.is_error() {
                    return err_expr(span);
                }
                if let Some(Const::Char(c)) = &x.value {
                    let u = if (97..=122).contains(c) { c - 32 } else { *c };
                    return const_expr(Const::Char(u), Ty::Char, span);
                }
                return Expr {
                    kind: ExprKind::Builtin {
                        func: f,
                        args: vec![x],
                    },
                    ty: Ty::Char,
                    value: None,
                    span,
                };
            }
            Builtin::Sqr => {
                let x = lowered.pop().unwrap();
                if is_fractional(x.ty) {
                    return self.real_sqr(x, span);
                }
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected a number type, found `{}` (argument 1, parameter `X`)", self.tn(x.ty)), r);
                    return err_expr(span);
                }
                // `X * X` in the computation type of `*` (`docs/spec/semantics.md` 5.3)
                let calc = self.int_calc_type(&x, &x, span).unwrap_or(Ty::Error);
                if let Some(Const::Int(v)) = &x.value {
                    return match self.fold_int(BinOp::Mul, *v, *v, calc, span) {
                        Some(c) => const_expr(c, calc, span),
                        None => err_expr(span),
                    };
                }
                return Expr {
                    kind: ExprKind::Builtin {
                        func: f,
                        args: vec![x],
                    },
                    ty: calc,
                    value: None,
                    span,
                };
            }
            Builtin::Trunc | Builtin::Round | Builtin::FloatToStr => {
                // A real or `Currency` keeps its type; an integer becomes a `Double`
                let x = lowered.pop().unwrap();
                // `class operator Trunc` / `Round` (`docs/spec/objects.md` 11.2)
                let x = if f != Builtin::FloatToStr && self.operator_owner(x.ty).is_some() {
                    let name = if f == Builtin::Trunc {
                        "Trunc"
                    } else {
                        "Round"
                    };
                    let at = x.span;
                    match self.unary_operator(x, name, name, at, span) {
                        Ok(e) => return e,
                        Err(x) => x,
                    }
                } else {
                    x
                };
                let x = if x.ty.is_integer() || (f == Builtin::FloatToStr && x.ty == Ty::Currency) {
                    self.convert_num(x, Ty::Double)
                } else {
                    x
                };
                if !is_fractional(x.ty) {
                    let r = x.span;
                    let (what, pname) = if f == Builtin::FloatToStr {
                        ("`Double`", "Value")
                    } else {
                        ("a number type", "X")
                    };
                    self.error(Code::E(702), format!("type mismatch: expected {what}, found `{}` (argument 1, parameter `{pname}`)", self.tn(x.ty)), r);
                    return err_expr(span);
                }
                return Expr {
                    kind: ExprKind::Builtin {
                        func: f,
                        args: vec![x],
                    },
                    ty: ret,
                    value: None,
                    span,
                };
            }
            Builtin::Chr => {
                let x = lowered.pop().unwrap();
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected `Word`, found `{}` (argument 1, parameter `X`)", self.tn(x.ty)), r);
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
                    self.error(Code::E(702), format!("type mismatch: expected `Int64`, found `{}` (argument 1, parameter `X`)", self.tn(x.ty)), r);
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
            Builtin::IntToHex => {
                // `X` keeps its type: its width decides the digits of a negative value
                let digits = lowered.pop().unwrap();
                let x = lowered.pop().unwrap();
                if !x.ty.is_integer() {
                    let r = x.span;
                    self.error(Code::E(702), format!("type mismatch: expected an integer type, found `{}` (argument 1, parameter `Value`)", self.tn(x.ty)), r);
                    return err_expr(span);
                }
                let digits = self.coerce(digits, Ty::Integer, checks.range, Some((2, "Digits")));
                if digits.ty.is_error() {
                    return err_expr(span);
                }
                return Expr {
                    kind: ExprKind::Builtin {
                        func: f,
                        args: vec![x, digits],
                    },
                    ty: Ty::String,
                    value: None,
                    span,
                };
            }
            Builtin::Length => {
                let x = lowered.pop().unwrap();
                if matches!(x.ty, Ty::Array(_)) {
                    // A static array's length is a constant (`docs/spec/types.md` chapter 7)
                    let n = self.types.slot_count(x.ty);
                    return const_expr(Const::Int(n as i128), Ty::Integer, span);
                }
                if let Some(Const::U8(s)) = &x.value {
                    // The number of bytes of a `UTF8String` (`docs/spec/types.md` 8.5)
                    return const_expr(Const::Int(s.len() as i128), Ty::Integer, span);
                }
                if x.ty.is_dyn_indexed() || x.ty == Ty::Utf8String {
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
            // In bytes on a `UTF8String` (`docs/spec/types.md` 8.5): `Copy(S, …)` returns a
            // `UTF8String`, and `Pos(Sub, S)` takes `Sub` as a `UTF8String`
            Builtin::Copy | Builtin::Pos => {
                let s_index = if f == Builtin::Copy { 0 } else { 1 };
                if lowered[s_index].ty == Ty::Utf8String {
                    let mut out = Vec::new();
                    for (i, (e, (pname, pty))) in lowered.into_iter().zip(sig.iter()).enumerate() {
                        let pty = if *pty == Ty::String {
                            Ty::Utf8String
                        } else {
                            *pty
                        };
                        let e = self.coerce(e, pty, checks.range, Some((i + 1, pname)));
                        if e.ty.is_error() {
                            return err_expr(span);
                        }
                        out.push(e);
                    }
                    let ty = if f == Builtin::Copy {
                        Ty::Utf8String
                    } else {
                        ret
                    };
                    return Expr {
                        kind: ExprKind::Builtin { func: f, args: out },
                        ty,
                        value: None,
                        span,
                    };
                }
            }
            _ => {}
        }
        let mut out = Vec::new();
        for (i, (e, ((pname, _), pty))) in
            lowered.into_iter().zip(sig.iter().zip(&params)).enumerate()
        {
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

    /// `Succ(X)` / `Pred(X)` (`docs/spec/semantics.md` 5.3): `X + 1` / `X - 1` in the computation
    /// type (`{$Q}`), converted back to the type of `X` (`{$R}`; a subrange is already its base
    /// type). A constant folds, and is E0714 outside the type.
    fn succ_pred(&mut self, x: Expr, pred: bool, span: TextRange, checks: Checks) -> Expr {
        // A distinct type: computed in its root type, the result of the distinct type, whose
        // name the range check reports (`docs/spec/types.md` 1.3)
        let target = x.ty;
        let x = self.rooted(x);
        let ty = x.ty;
        if !ty.is_ordinal() {
            let r = x.span;
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected an ordinal type, found `{}` (argument 1, parameter `X`)",
                    self.tn(ty)
                ),
                r,
            );
            return err_expr(span);
        }
        let delta: i128 = if pred { -1 } else { 1 };
        if let Some(v) = x.value.as_ref().and_then(Const::as_int) {
            let n = v + delta;
            let (lo, hi) = self.types.ord_range(ty).unwrap_or((0, 0));
            if n < lo || n > hi {
                self.const_out_of_range(n, target, span);
                return err_expr(span);
            }
            let value = match ty {
                Ty::Char => Const::Char(n as u16),
                Ty::Boolean => Const::Bool(n != 0),
                _ => Const::Int(n),
            };
            return const_expr(value, target, span);
        }
        let (operand, calc) = if ty.is_integer() {
            let calc = if ty.fits_in_integer() {
                Ty::Integer
            } else {
                ty
            };
            (x, calc)
        } else {
            let xs = x.span;
            (
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(x),
                        kind: Conversion::OrdToInt,
                    },
                    ty: Ty::Integer,
                    value: None,
                    span: xs,
                },
                Ty::Integer,
            )
        };
        let sum = Expr {
            kind: ExprKind::Binary {
                first: Box::new(operand),
                steps: vec![BinStep {
                    op: if pred { BinOp::Sub } else { BinOp::Add },
                    rhs: const_expr(Const::Int(1), calc, span),
                    calc: Calc::Int(calc),
                    ty: calc,
                    span,
                }],
            },
            ty: calc,
            value: None,
            span,
        };
        let mut out = match ty {
            t if t.is_integer() => self.coerce(sum, target, checks.range, None),
            // Checked against the range of `Char`, named by the distinct type
            Ty::Char if matches!(target, Ty::Distinct(_)) => {
                self.convert_to_ranged(sum, Ty::Integer, target, checks.range)
            }
            Ty::Char => Expr {
                kind: ExprKind::Convert {
                    operand: Box::new(sum),
                    kind: Conversion::IntToChar {
                        check: checks.range,
                    },
                },
                ty: Ty::Char,
                value: None,
                span,
            },
            t => self.convert_to_ranged(sum, Ty::Integer, t, checks.range),
        };
        out.span = span;
        if !out.ty.is_error() {
            out.ty = target;
        }
        out
    }

    /// `Copy(A[, Index[, Count]])` of an array (`docs/spec/types.md` chapter 7): a new dynamic array
    /// of the element type.
    fn array_copy(&mut self, mut args: Vec<Expr>, elem: Ty, span: TextRange) -> Expr {
        if args.len() > 3 {
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 1 to 3, found {}",
                    args.len()
                ),
                span,
            );
            return err_expr(span);
        }
        let checks = self.checks_at(span.start());
        let names = ["Index", "Count"];
        for i in 1..args.len() {
            let a = std::mem::replace(&mut args[i], err_expr(span));
            let a = self.coerce(a, Ty::Int64, checks.range, Some((i + 1, names[i - 1])));
            if a.ty.is_error() {
                return err_expr(span);
            }
            args[i] = a;
        }
        let ty = self.types.dyn_array(
            elem,
            super::types::DynWritten::Anonymous {
                generic: false,
                span,
            },
        );
        Expr {
            kind: ExprKind::Builtin {
                func: Builtin::Copy,
                args,
            },
            ty,
            value: None,
            span,
        }
    }

    /// `Assigned(X)` of a dynamic array: `X <> nil` (`docs/spec/types.md` chapter 7).
    fn assigned(&mut self, mut args: Vec<Expr>, span: TextRange) -> Expr {
        if args.len() != 1 {
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
        let x = args.pop().unwrap();
        if let Ty::Param(p) = x.ty
            && self.param_reference(x.ty).is_none()
        {
            // `docs/spec/generics.md` 3.3
            self.param_misuse(p, super::generics::ParamOp::Compare, span);
            return err_expr(span);
        }
        if !self.takes_nil_ty(x.ty) {
            let r = x.span;
            self.error(
                Code::E(702),
                format!(
                    "type mismatch: expected a dynamic array, found `{}` (argument 1, parameter `X`)",
                    self.tn(x.ty)
                ),
                r,
            );
            return err_expr(span);
        }
        Expr {
            kind: ExprKind::Builtin {
                func: Builtin::Assigned,
                args: vec![x],
            },
            ty: Ty::Boolean,
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
        // A procedural value converted to a procedural type is not called
        // (`docs/spec/objects.md` 9.4)
        let x = if t.is_procedural() {
            self.lower_value(&v)
        } else {
            self.lower_expr(&v)
        };
        if x.ty.is_error() || t.is_error() {
            return err_expr(span);
        }
        let _ = name_range;
        if x.ty != t && (matches!(x.ty, Ty::Distinct(_)) || matches!(t, Ty::Distinct(_))) {
            // A distinct type converts as its root type; the result is of the type written
            // (`docs/spec/types.md` 1.3)
            let x = self.rooted(x);
            let root = self.types.root(t);
            let mut e = self.convert_to(x, root, &v, span);
            if !e.ty.is_error() {
                e.ty = t;
            }
            return e;
        }
        self.convert_to(x, t, &v, span)
    }

    /// `T(x)` of the value `x` (2.5); `v` is the argument as written.
    fn convert_to(&mut self, x: Expr, t: Ty, v: &ast::Expr, span: TextRange) -> Expr {
        let checks = self.checks_at(span.start());
        if matches!(t, Ty::Enum(_) | Ty::Subrange(_)) && x.ty != t {
            // To an enumeration (from an integer) or a subrange (as to its base type), with the
            // range checked by `{$R}` (`docs/spec/types.md` 8.1, 8.2)
            let src = x.ty;
            let arg_span = x.span;
            let base = self.types.base(t);
            let inner = match base {
                Ty::Enum(_) if src == base => x,
                Ty::Enum(_) if src.is_integer() => {
                    let mut w = x;
                    w.ty = base;
                    w
                }
                Ty::Enum(_) => {
                    self.error(
                        Code::E(724),
                        format!(
                            "a value of type `{}` cannot be converted to `{}`",
                            self.tn(src),
                            self.tn(t)
                        ),
                        span,
                    );
                    return err_expr(span);
                }
                _ => self.convert_explicit(x, base, v, span, checks),
            };
            if inner.ty.is_error() {
                return inner;
            }
            // A constant out of range is reported at the argument, as for `Byte(300)`
            let mut inner = inner;
            inner.span = arg_span;
            let mut e = self.convert_to_ranged(inner, src, t, checks.range);
            e.span = span;
            return e;
        }
        self.convert_explicit(x, t, v, span, checks)
    }

    /// `T(x)` for a type that is not an enumeration or a subrange (2.5). `v` is the argument as
    /// written (for the fix of a constant out of range).
    fn convert_explicit(
        &mut self,
        x: Expr,
        t: Ty,
        v: &ast::Expr,
        span: TextRange,
        checks: Checks,
    ) -> Expr {
        if x.ty == t {
            let mut e = x;
            e.span = span;
            // `Integer(1)` is an `Integer` constant by its conversion, not by its value (3.2)
            if let (ExprKind::Const { explicit }, Some(Const::Int(_))) = (&mut e.kind, &e.value) {
                *explicit = true;
            }
            return e;
        }
        if let Some(p) = super::Sema::param_of(x.ty).or(super::Sema::param_of(t)) {
            // `docs/spec/generics.md` 3.3
            self.param_misuse(p, super::generics::ParamOp::Convert, span);
            return err_expr(span);
        }
        let x = match self.class_cast(x, t, span) {
            Ok(e) => return e,
            Err(x) => x,
        };
        match (x.ty, t) {
            // To a real or `Currency`, as assignment compatibility (`docs/spec/types.md` 8.4)
            (s, t) if is_fractional(t) && s.is_number() => {
                let mut e = self.convert_num(x, t);
                e.span = span;
                e
            }
            (s, t) if s.is_integer() && t.is_integer() => {
                if let Some(Const::Int(val)) = &x.value {
                    if !t.contains(*val) {
                        let val = *val;
                        let literal = is_literal_arg(v);
                        self.const_out_of_range(val, t, x.span);
                        if literal {
                            let wrapped = t.wrap(val);
                            let text = if val < 0 || is_hex_literal(v, self.input.bytes) {
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
                    return explicit_const(Const::Int(*val), t, span);
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
            (s @ (Ty::Char | Ty::Boolean | Ty::Enum(_)), t) if t.is_integer() => {
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
                    return explicit_const(Const::Int(*v), t, span);
                }
                let (lo, hi) = t.range().unwrap();
                let (slo, shi) = self.types.ord_range(s).unwrap_or((0, 0));
                if lo <= slo && shi <= hi {
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
            (Ty::String | Ty::Char, Ty::Utf8String) | (Ty::Utf8String, Ty::String) => {
                let mut e = self.convert_u8(x, t);
                e.span = span;
                e
            }
            (s, t) => {
                // A user's `Explicit`, or else `Implicit` (`docs/spec/objects.md` 11.4)
                for names in [["Explicit"], ["Implicit"]] {
                    match self.convert_by_operator(&x, t, &names) {
                        Ok(Some(mut e)) => {
                            e.span = span;
                            return e;
                        }
                        Ok(None) => {}
                        Err(()) => return err_expr(span),
                    }
                }
                let d = self.error(
                    Code::E(724),
                    format!(
                        "a value of type `{}` cannot be converted to `{}`",
                        self.tn(s),
                        self.tn(t)
                    ),
                    span,
                );
                if is_fractional(s) && t.is_integer() {
                    d.notes.push(TRUNC_ROUND_NOTE.into());
                }
                err_expr(span)
            }
        }
    }

    // ---- Assignment targets ----

    /// Resolves an expression as an assignment target. Returns `(place, type)`. On error, reports a diagnostic and returns `None`.
    /// The type of a `[weak]` place is the class it takes (`docs/spec/objects.md` 4.4); the
    /// place is not passed to a `var` / `out` parameter (E0706), whose writes would count.
    pub(super) fn lower_place(&mut self, e: &ast::Expr, usage: PlaceUse) -> Option<(Place, Ty)> {
        let (place, ty) = self.lower_place_storage(e, usage)?;
        if matches!(ty, Ty::Weak(_)) && usage.wants_variable() {
            let diag = match usage {
                PlaceUse::Container { whole, .. } => whole,
                _ => ast::trimmed_range(e.syntax()),
            };
            let kind = match usage {
                PlaceUse::RefArg { is_out: true } => "an `out` parameter",
                _ => "a `var` parameter",
            };
            self.error(
                Code::E(706),
                format!("a `[weak]` reference cannot be passed to {kind}"),
                diag,
            )
            .notes
            .push(
                "copy it to a variable without `[weak]`, pass that variable, and assign it back"
                    .into(),
            );
            return None;
        }
        Some((place, self.types.strong(ty)))
    }

    /// [`Sema::lower_place`] with the type of the place's storage (`Ty::Weak` for a `[weak]`
    /// variable or field).
    fn lower_place_storage(&mut self, e: &ast::Expr, usage: PlaceUse) -> Option<(Place, Ty)> {
        self.last_place_result = None;
        let span = ast::trimmed_range(e.syntax());
        // Diagnostics about a container (`R` in `R.F := 1`) point at the whole target
        let diag = match usage {
            PlaceUse::Container { whole, .. } => whole,
            _ => span,
        };
        match e {
            ast::Expr::Field(f) if self.is_value_chain(e) => {
                let base = f.base()?;
                let Some(field) = f.field() else {
                    self.lower_expr(&base);
                    return None;
                };
                if matches!(self.trial_type(&base), Ty::Class(_) | Ty::ClassRef(_)) {
                    // A field of a class instance: the handle is read, its slot written. A
                    // `class var` through an instance or a class reference is the class's global
                    // (`docs/spec/objects.md` 2.2)
                    let b = self.lower_expr(&base);
                    let class = match b.ty {
                        Ty::Class(c) => c,
                        t => self.types.class_ref_target(t)?,
                    };
                    let fname = self.text(&field).to_string();
                    let frange = field.text_range();
                    return match self.find_member(class, &fname, frange) {
                        Ok(super::methods::Member::Static(_, st)) => match st.kind {
                            super::types::StaticKind::Var(g, ty) => Some((Place::Global(g), ty)),
                            _ => {
                                if usage.wants_variable() {
                                    self.last_place_result = Some(PlaceResult::NotVariable(diag));
                                } else {
                                    self.cannot_assign(diag, "not a variable", None);
                                }
                                None
                            }
                        },
                        Ok(super::methods::Member::Field { name, .. })
                            if matches!(b.ty, Ty::ClassRef(_)) =>
                        {
                            self.instance_member_error(&name, None, frange);
                            None
                        }
                        Ok(super::methods::Member::Field { slot, ty, name }) => Some((
                            Place::ObjField {
                                base: Box::new(b),
                                slot,
                                ty,
                                name,
                            },
                            ty,
                        )),
                        Ok(super::methods::Member::Property(..)) => {
                            self.property_place_error(usage, diag);
                            None
                        }
                        Ok(_) => {
                            if usage.wants_variable() {
                                self.last_place_result = Some(PlaceResult::NotVariable(diag));
                            } else {
                                self.cannot_assign(diag, "not a variable", None);
                            }
                            None
                        }
                        Err(true) => None,
                        Err(false) => {
                            let t = self.tn(Ty::Class(class));
                            self.class_member_missing(&t, &fname, frange);
                            None
                        }
                    };
                }
                let (bp, bty) = self.lower_place(&base, usage.container(diag))?;
                self.mark_container_written(&bp, bty);
                if bty.is_error() {
                    return None;
                }
                let (index, ty, name) = self.find_field(bty, &field, span)?;
                Some((
                    Place::Field {
                        base: Box::new(bp),
                        index,
                        ty,
                        name,
                    },
                    ty,
                ))
            }
            // `inherited P` names the parent's property (`docs/spec/objects.md` 2.4)
            ast::Expr::NameRef(_) | ast::Expr::Field(_) | ast::Expr::Inherited(_) => {
                let (r, name, range) = self.resolve_name_expr(e)?;
                match r {
                    Resolved::Var(id) => {
                        let v = self.var(id).clone();
                        // A `const` parameter of an enclosing routine cannot be written either
                        // (`docs/spec/semantics.md` 4.9)
                        if let VarKind::Param(ParamMode::Const) = self.var_decl(id).kind
                            && usage.is_write()
                        {
                            self.cannot_assign(diag, "a `const` parameter", Some(v.span));
                            return None;
                        }
                        if matches!(v.ty, Ty::OpenArray(_))
                            && matches!(usage, PlaceUse::Assign | PlaceUse::SetLength)
                        {
                            // An open array parameter is not assigned as a whole, nor resized
                            // (`docs/spec/types.md` 2.4)
                            self.cannot_assign(diag, "an open array parameter", Some(v.span));
                            return None;
                        }
                        if self.cur.as_ref().is_some_and(|c| c.loop_vars.contains(&id))
                            && usage.is_write()
                        {
                            self.cannot_assign(diag, "the loop variable", Some(v.span));
                            return None;
                        }
                        if v.kind == VarKind::Exception && usage.is_write() {
                            self.cannot_assign(diag, "the exception variable", Some(v.span));
                            return None;
                        }
                        // `docs/spec/objects.md` 2.3
                        if self.var_decl(id).kind == VarKind::SelfParam && usage.is_write() {
                            self.cannot_assign(diag, "Self", None);
                            return None;
                        }
                        if usage.reads() {
                            self.mark_used(id, span);
                        }
                        self.mark_written(id, span);
                        // A container is marked by its field or element once its type is known
                        if !matches!(usage, PlaceUse::Container { .. }) {
                            self.mark_value_written(id);
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
                            let s = self.global_decl_span(id);
                            self.cannot_assign_decl(diag, "the loop variable", s);
                            return None;
                        }
                        Some((Place::Global(id), self.globals[id.0 as usize].ty))
                    }
                    Resolved::Const { span: decl, .. } => {
                        if usage.wants_variable() {
                            // A constant is not a variable (E0706; `var` arguments, `Inc` / `Dec`, `ReadLn`)
                            self.last_place_result = Some(PlaceResult::NotVariable(diag));
                            return None;
                        }
                        self.cannot_assign_decl(diag, "a constant", decl);
                        None
                    }
                    Resolved::Error => None,
                    Resolved::Member {
                        recv,
                        member: super::methods::Member::Field { slot, ty, name },
                        ..
                    } => Some((
                        Place::ObjField {
                            base: recv,
                            slot,
                            ty,
                            name,
                        },
                        ty,
                    )),
                    Resolved::Member {
                        recv,
                        member: super::methods::Member::RecField { index, ty, name },
                        ..
                    } => {
                        // A field of `Self` in a record method (`docs/spec/objects.md` chapter 6)
                        let Some(base) = self.receiver_place(&recv) else {
                            self.cannot_assign(diag, "not a variable", None);
                            return None;
                        };
                        Some((
                            Place::Field {
                                base: Box::new(base),
                                index,
                                ty,
                                name,
                            },
                            ty,
                        ))
                    }
                    Resolved::Member {
                        member: super::methods::Member::Property(..),
                        ..
                    } => {
                        self.property_place_error(usage, diag);
                        None
                    }
                    Resolved::Routine(id)
                        if usage == PlaceUse::Assign
                            && let Some(up) = self.enclosing_function(id) =>
                    {
                        // Assigning to the function name (`Square := X * X`) assigns to `Result`
                        // (4.3), also from a nested routine (4.9)
                        let rid = self.result_id(up);
                        let vid = self.var_at(up, rid);
                        let ty = self.var(vid).ty;
                        Some((Place::Var(vid, span), ty))
                    }
                    _ => {
                        if usage.wants_variable() {
                            self.last_place_result = Some(PlaceResult::NotVariable(diag));
                            return None;
                        }
                        let _ = (name, range);
                        self.cannot_assign(diag, "not a variable", None);
                        None
                    }
                }
            }
            ast::Expr::Paren(p) => {
                let inner = p.inner()?;
                self.lower_place(&inner, usage)
            }
            ast::Expr::Index(i) => {
                let base = i.base()?;
                let indices = i.indices();
                if self.indexes_default_property(&base) {
                    // A default array property is not a variable (`docs/spec/objects.md` 12.1)
                    self.lower_expr(&base);
                    for ix in &indices {
                        self.lower_index_item_exprs(ix);
                    }
                    self.property_place_error(usage, diag);
                    return None;
                }
                if indices
                    .iter()
                    .any(|ix| matches!(ix, ast::IndexItem::Slice(_)))
                {
                    // A slice is a copy, not a place (`docs/spec/types.md` 8.7): E0707, or E0706
                    // for a `var` / `out` argument
                    let x = self.lower_expr(e);
                    if x.ty.is_error() {
                        return None;
                    }
                    if usage.wants_variable() {
                        self.last_place_result = Some(PlaceResult::NotVariable(diag));
                    } else {
                        self.cannot_assign(diag, "a slice", None);
                    }
                    return None;
                }
                let indices: Vec<ast::Expr> = indices
                    .into_iter()
                    .filter_map(|ix| match ix {
                        ast::IndexItem::Expr(e) => Some(e),
                        ast::IndexItem::Slice(_) => None,
                    })
                    .collect();
                let Some((place, ty)) = self.lower_place(&base, usage.container(diag)) else {
                    for ix in &indices {
                        self.lower_expr(ix);
                    }
                    return None;
                };
                self.mark_container_written(&place, ty);
                if ty.is_error() {
                    return None;
                }
                if !self.check_index_count(ty, indices.len(), span) {
                    return None;
                }
                let bracket = i.l_brack().map(|t| t.text_range()).unwrap_or(span);
                let mut cur = (place, ty);
                for ix in &indices {
                    let (p, t) = cur;
                    if t == Ty::Utf8String && !usage.wants_variable() {
                        // The bytes of a `UTF8String` are read only (`docs/spec/types.md` 8.5)
                        self.lower_expr(ix);
                        self.cannot_assign(diag, "a byte of a `UTF8String`", None);
                        self.last_mut().notes.push(
                            "convert the value to `string` to change its characters, then convert it back"
                                .into(),
                        );
                        return None;
                    }
                    if matches!(t, Ty::String | Ty::Utf8String) {
                        if usage.wants_variable() {
                            // A string element cannot be passed to a var / out parameter, nor be the
                            // target of `Inc` / `Dec` / `ReadLn` (E0706; `docs/spec/types.md` chapter 3)
                            self.lower_expr(ix);
                            self.last_place_result = Some(PlaceResult::NotVariable(diag));
                            return None;
                        }
                        let idx = self.lower_expr(ix);
                        if idx.ty.is_error() {
                            return None;
                        }
                        if !idx.ty.is_integer() {
                            let r = idx.span;
                            self.error(
                                Code::E(719),
                                format!(
                                    "index type mismatch: expected `Integer`, found `{}`",
                                    self.tn(idx.ty)
                                ),
                                r,
                            );
                            return None;
                        }
                        cur = (
                            Place::StrIndex {
                                base: Box::new(p),
                                index: Box::new(idx),
                                span: bracket,
                            },
                            Ty::Char,
                        );
                        continue;
                    }
                    if let Some(elem) = self.types.dyn_elem(t) {
                        let idx = self.dyn_index(ix)?;
                        cur = (
                            Place::Elem {
                                base: Box::new(p),
                                index: Box::new(idx),
                                ty: elem,
                                span: bracket,
                            },
                            elem,
                        );
                        continue;
                    }
                    let (_, _, _, elem) = self.types.array(t)?;
                    let idx = self.array_index(t, ix)?;
                    cur = (
                        Place::Elem {
                            base: Box::new(p),
                            index: Box::new(idx),
                            ty: elem,
                            span: bracket,
                        },
                        elem,
                    );
                }
                Some(cur)
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
        let decl = decl.map(|s| self.span(s));
        self.cannot_assign_decl(span, label, decl);
    }

    /// E0707 with the declaration in any file.
    pub(super) fn cannot_assign_decl(
        &mut self,
        span: TextRange,
        label: &'static str,
        decl: Option<Span>,
    ) {
        let text = self.src(span);
        let mut d = Diagnostic::error(
            Code::E(707),
            format!("cannot assign to `{text}`"),
            Stage::Sema,
        )
        .with_primary(self.span(span), label);
        if let Some(s) = decl {
            d = d.with_secondary(s, "declared here");
        }
        self.push(d);
    }
}

impl Sema<'_, '_> {
    /// A field or element of the variable `place` (of type `ty`) is written: its value changes
    /// ([`Sema::mark_value_written`]), unless it is a dynamic array, whose elements are shared.
    fn mark_container_written(&mut self, place: &Place, ty: Ty) {
        if let Place::Var(id, _) = place
            && !matches!(ty, Ty::DynArray(_))
        {
            self.mark_value_written(*id);
        }
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
    /// The first argument of `SetLength` (written; an assignment for definite assignment,
    /// `docs/spec/types.md` chapter 6).
    SetLength,
    /// The record or array a field or element belongs to (`R` in `R.F := …`; read and written).
    /// `whole` is the whole target (for diagnostics); `var_arg` is set when the whole target goes
    /// to a `var` / `out` parameter, `Inc` / `Dec` or `ReadLn` (a constant is then E0706).
    Container { whole: TextRange, var_arg: bool },
}

impl PlaceUse {
    fn is_write(self) -> bool {
        true
    }

    /// A non-variable is E0706 (reported by the caller).
    fn wants_variable(self) -> bool {
        match self {
            PlaceUse::RefArg { .. } | PlaceUse::IncDec | PlaceUse::ReadLn | PlaceUse::SetLength => {
                true
            }
            PlaceUse::Container { var_arg, .. } => var_arg,
            _ => false,
        }
    }

    fn reads(self) -> bool {
        matches!(
            self,
            PlaceUse::RefArg { is_out: false } | PlaceUse::IncDec | PlaceUse::Container { .. }
        )
    }

    /// The use of the container of a field or element written with this use.
    fn container(self, whole: TextRange) -> PlaceUse {
        match self {
            PlaceUse::Container { .. } => self,
            _ => PlaceUse::Container {
                whole,
                var_arg: self.wants_variable(),
            },
        }
    }
}

/// When `lower_place` finds a non-variable for `RefArg`.
pub(super) enum PlaceResult {
    Ok,
    NotVariable(TextRange),
}

/// The array constructor an expression is, through parentheses.
pub(super) fn ctor_of(e: &ast::Expr) -> Option<ast::ArrayCtor> {
    match e {
        ast::Expr::ArrayCtor(ac) => Some(ac.clone()),
        ast::Expr::Paren(p) => p.inner().as_ref().and_then(ctor_of),
        _ => None,
    }
}

fn binop_of(kind: SyntaxKind) -> Option<BinOp> {
    Some(match kind {
        SyntaxKind::PLUS => BinOp::Add,
        SyntaxKind::MINUS => BinOp::Sub,
        SyntaxKind::STAR => BinOp::Mul,
        SyntaxKind::DIV_KW => BinOp::Div,
        SyntaxKind::SLASH => BinOp::Slash,
        SyntaxKind::MOD_KW => BinOp::Mod,
        SyntaxKind::AND_KW => BinOp::And,
        SyntaxKind::OR_KW => BinOp::Or,
        SyntaxKind::XOR_KW => BinOp::Xor,
        SyntaxKind::SHL_KW => BinOp::Shl,
        SyntaxKind::SHR_KW => BinOp::Shr,
        SyntaxKind::IN_KW => BinOp::In,
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
    compare_units(a.iter().copied(), b.iter().copied())
}

/// [`compare_strings`] of code units read from iterators (the VM reads either representation of
/// a string without copying it).
pub fn compare_units(
    a: impl IntoIterator<Item = u16>,
    b: impl IntoIterator<Item = u16>,
) -> std::cmp::Ordering {
    let code = |r: Result<char, std::char::DecodeUtf16Error>| {
        r.map(|c| c as u32)
            .unwrap_or_else(|e| e.unpaired_surrogate() as u32)
    };
    char::decode_utf16(a)
        .map(code)
        .cmp(char::decode_utf16(b).map(code))
}

fn is_paren(e: &ast::BinaryExpr) -> bool {
    // BINARY_EXPR contains no parentheses (parentheses are PAREN_EXPR)
    let _ = e;
    false
}

/// The computation type of unary `-` on an integer type (`docs/spec/semantics.md` 3.2), or
/// `None` for `UInt64` (E0716).
fn neg_calc(t: Ty) -> Option<Ty> {
    match t {
        t if t.fits_in_integer() => Some(Ty::Integer),
        Ty::Cardinal | Ty::Int64 => Some(Ty::Int64),
        _ => None,
    }
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

/// Whether two `var` / `out` arguments are known at compile time to be the same location
/// (E0741): the same variable, or the same fields and elements of it with indices that are equal
/// constants or the same variable.
fn same_location(a: &Place, b: &Place) -> bool {
    match (a, b) {
        (Place::Var(x, _), Place::Var(y, _)) => x == y,
        (Place::Global(x), Place::Global(y)) => x == y,
        (
            Place::Field {
                base: x, index: i, ..
            },
            Place::Field {
                base: y, index: j, ..
            },
        ) => i == j && same_location(x, y),
        (
            Place::Elem {
                base: x, index: i, ..
            },
            Place::Elem {
                base: y, index: j, ..
            },
        ) => same_index(i, j) && same_location(x, y),
        (
            Place::ObjField {
                base: x, slot: i, ..
            },
            Place::ObjField {
                base: y, slot: j, ..
            },
        ) => i == j && same_instance(x, y),
        _ => false,
    }
}

/// Whether two instance expressions are known to be the same instance: the same variable, or
/// `Self` (`docs/spec/objects.md` 2.2).
fn same_instance(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Var(x), ExprKind::Var(y)) => x == y,
        (ExprKind::Global(x), ExprKind::Global(y)) => x == y,
        _ => false,
    }
}

/// Whether two indices are known to be equal: equal constants, or the same variable.
fn same_index(a: &Expr, b: &Expr) -> bool {
    match (&a.value, &b.value) {
        (Some(x), Some(y)) => x.as_int().is_some() && x.as_int() == y.as_int(),
        (None, None) => match (&a.kind, &b.kind) {
            (ExprKind::Var(x), ExprKind::Var(y)) => x == y,
            (ExprKind::Global(x), ExprKind::Global(y)) => x == y,
            _ => false,
        },
        _ => false,
    }
}

impl Sema<'_, '_> {
    /// E0702 for a `nil` whose type no context gives (`docs/spec/semantics.md` chapter 8).
    fn nil_without_type(&mut self, span: TextRange) {
        self.error(
            Code::E(702),
            "type mismatch: expected a dynamic array, a class, a class reference or an interface, found `nil`",
            span,
        )
        .notes
        .push("`nil` takes its type from the variable or the operand it is used with".into());
    }

    /// Whether `nil` can be a value of the type: [`takes_nil`], or a type parameter whose
    /// constraints make it a reference (`docs/spec/generics.md` 3.3).
    pub(super) fn takes_nil_ty(&self, ty: Ty) -> bool {
        takes_nil(ty) || self.param_reference(ty).is_some()
    }

    /// [`Self::nil_mismatch`] for a `nil` compared with an operand of a type that cannot be
    /// `nil` (E0762 `compare` for a type parameter; `docs/spec/generics.md` 3.3).
    fn nil_compare_mismatch(&mut self, other: Ty, span: TextRange) {
        match other {
            Ty::Param(p) => self.param_misuse(p, super::generics::ParamOp::Compare, span),
            _ => self.nil_mismatch(other, span),
        }
    }

    /// E0702 for a `nil` where a type that cannot be `nil` is expected.
    fn nil_mismatch(&mut self, expected: Ty, span: TextRange) {
        if let Ty::Param(p) = expected {
            // `docs/spec/generics.md` 3.3
            self.param_misuse(p, super::generics::ParamOp::AssignNil, span);
            return;
        }
        let t = self.tn(expected);
        self.error(
            Code::E(702),
            format!("type mismatch: expected `{t}`, found `nil`"),
            span,
        );
    }
}

/// Whether the expression is `nil`, possibly in parentheses.
pub(super) fn is_nil(e: &ast::Expr) -> bool {
    matches!(super::overload::strip_parens(e), ast::Expr::Nil(_))
}

/// Whether `nil` can be a value of the type: a dynamic array or a class
/// (`docs/spec/types.md` 2.3, `docs/spec/objects.md` 4.5).
pub(super) fn takes_nil(ty: Ty) -> bool {
    matches!(
        ty,
        Ty::DynArray(_)
            | Ty::Class(_)
            | Ty::ClassRef(_)
            | Ty::Intf(_)
            | Ty::Proc(_)
            | Ty::Callable(_)
    )
}
