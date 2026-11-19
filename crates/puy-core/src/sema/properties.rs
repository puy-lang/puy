//! Properties of classes (`docs/spec/objects.md` 2.6): the declarations in the class body and
//! their accessors, and the reads and writes of a property, which become reads and writes of a
//! field or a `class var`, or calls of the accessor methods.

use super::attrs::AttrSite;
use super::defaults::Indices;
use super::expr::{PlaceUse, err_expr};
use super::hir::*;
use super::methods::Member;
use super::types::{
    Accessor, ClassMethod, ClassProperty, Const, MethodKind, SelfKind, StaticKind, Ty, TypeId,
    Visibility,
};
use super::{Resolved, Sema};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{Span, TextRange};
use crate::syntax::SyntaxKind;
use crate::syntax::ast::{self, AstNode, ParamMode};

/// A property being declared: what its accessors are checked against.
struct Draft {
    ty: Ty,
    /// The index parameters: (name, type).
    params: Vec<(String, Ty)>,
    index: Option<i128>,
    is_class: bool,
    /// A property of an interface, whose accessors are methods only (`docs/spec/objects.md` 7.1).
    in_interface: bool,
}

/// The target of `O.P := X` or `O.Items[I] := X`: the expression naming the property and the
/// indices.
struct Target {
    name: ast::Expr,
    /// `O.Items[…]`; `None` for a property without indices.
    index: Option<ast::IndexExpr>,
    /// `X[…]` of a default array property: `name` is `X`.
    default: bool,
}

impl<'a, 's> Sema<'a, 's> {
    /// The properties of a class (`docs/spec/objects.md` 2.6), after its methods: each accessor
    /// is a field, a `class var` or a method of the class or an ancestor.
    pub(super) fn define_properties(
        &mut self,
        id: TypeId,
        decls: Vec<(ast::PropertyDecl, Visibility)>,
    ) {
        let mut props: Vec<ClassProperty> = Vec::new();
        // The accessors are inside the property's declaration (`docs/spec/attributes.md` 6.2)
        let mut scope = None;
        for (decl, vis) in decls {
            self.end_names_hints(scope.take());
            self.decl_attributes(decl.syntax(), AttrSite::Other, Ty::Error);
            let Some(name_tok) = decl.name() else {
                continue;
            };
            if decl.type_params().is_some() {
                // `docs/spec/generics.md` 2.1: the property is not declared
                let at = decl
                    .property_token()
                    .map_or(name_tok.text_range(), |t| t.text_range());
                self.error(Code::E(757), "a property cannot have type parameters", at);
                continue;
            }
            let name = self.text(&name_tok).to_string();
            let name_range = name_tok.text_range();
            let owner = self.types.get(id).name.clone().unwrap_or_default();
            self.declare_hints(decl.syntax(), name_range, format!("{owner}.{name}"));
            scope = Some(self.span(name_range));
            self.begin_names_hints(scope);
            let is_class = decl.is_class();
            let mut read = None;
            let mut write = None;
            let mut index = None;
            for spec in decl.specs() {
                let Some(kw) = spec.keyword() else { continue };
                let word = self.text(&kw).to_ascii_lowercase();
                let whole = ast::trimmed_range(spec.syntax());
                match kw.kind() {
                    SyntaxKind::READ_KW => read = spec.name(),
                    SyntaxKind::WRITE_KW => write = spec.name(),
                    SyntaxKind::INDEX_KW => index = Some(spec.expr()),
                    SyntaxKind::STORED_KW | SyntaxKind::DEFAULT_KW | SyntaxKind::NODEFAULT_KW => {
                        // RTTI and streaming have no meaning in M3
                        let del = crate::diagnostics::deletion_range(self.input.bytes, whole);
                        let d = Diagnostic::warning(
                            Code::W(22),
                            format!("property specifier `{word}` is ignored"),
                            Stage::Sema,
                        )
                        .with_primary(self.span(whole), "")
                        .with_fix(Fix::safe(
                            format!("remove `{word}`"),
                            vec![TextEdit::delete(self.input.file, del)],
                        ));
                        self.push(d);
                    }
                    SyntaxKind::IMPLEMENTS_KW => {
                        self.error(
                            Code::E(3200),
                            "`implements` is not supported yet (planned for a later release)",
                            whole,
                        );
                    }
                    _ => {
                        // `readonly`, `writeonly`, `dispid`
                        self.error(
                            Code::E(3010),
                            format!("`{word}` is not supported in Puy"),
                            kw.text_range(),
                        )
                        .notes
                        .push("Puy does not support COM".into());
                    }
                }
            }
            let mut is_default = false;
            if let Some(d) = decl.default_token() {
                let r = d.text_range();
                if self.types.is_helper(id) {
                    // `X[I]` is decided by the type itself (`docs/spec/objects.md` 10.2)
                    self.not_in_helper("default", r);
                } else if is_class {
                    self.error(
                        Code::E(756),
                        "a class property cannot be a default property",
                        r,
                    );
                } else if decl.params().is_empty() {
                    self.error(
                        Code::E(756),
                        "a default property must have index parameters",
                        r,
                    );
                } else {
                    is_default = true;
                }
            }
            let Some(ts) = decl.type_spec() else {
                // Redeclaring an ancestor's property to change its visibility (RTTI)
                let start = decl
                    .first_token()
                    .map_or(name_range.start(), |t| t.text_range().start());
                self.error(
                    Code::E(3200),
                    format!(
                        "`property {name};` is not supported yet (planned for a later release)"
                    ),
                    TextRange::new(start, name_range.end()),
                );
                continue;
            };
            let ty = self.resolve_type(Some(ts.clone()));
            let mut params = Vec::new();
            for g in decl.params() {
                let t = self.resolve_type(g.type_ref());
                self.decl_attributes(g.syntax(), AttrSite::Param, t);
                for n in g.names() {
                    params.push((self.text(&n).to_string(), t));
                }
            }
            let index_written = index.is_some();
            let index = index.and_then(|e| self.property_index(e));
            self.check_reserved_name(&name.to_ascii_lowercase(), name_range);
            let types: Vec<Ty> = params.iter().map(|(_, t)| *t).collect();
            if self.default_property_duplicate(&props, &name, name_range, &types, is_default) {
                continue;
            }
            if is_default {
                self.check_default_property(id, &props, &name, name_range, &types, ty, &ts);
            }
            if let Some(prev) = self.property_duplicate(id, &props, &name, is_default) {
                let d = Diagnostic::error(
                    Code::E(701),
                    format!("`{name}` is already declared"),
                    Stage::Sema,
                )
                .with_primary(self.span(name_range), "declared again here")
                .with_secondary(self.span(prev), "declared here");
                self.push(d);
                continue;
            }
            let draft = Draft {
                ty,
                params,
                index,
                is_class,
                in_interface: self.types.is_interface(id),
            };
            // After an error in `index N`, the accessors cannot be checked against it
            let index_failed = index_written && draft.index.is_none();
            let read = read
                .filter(|_| !index_failed)
                .and_then(|q| self.property_accessor(id, &draft, &q, false));
            let write = write
                .filter(|_| !index_failed)
                .and_then(|q| self.property_accessor(id, &draft, &q, true));
            props.push(ClassProperty {
                name,
                span: name_range,
                vis,
                ty,
                params: draft.params.iter().map(|(_, t)| *t).collect(),
                index: draft.index,
                read,
                write,
                is_class,
                is_default,
            });
        }
        self.end_names_hints(scope);
        if let super::types::TypeKind::Class { properties, .. }
        | super::types::TypeKind::Record { properties, .. }
        | super::types::TypeKind::Interface { properties, .. }
        | super::types::TypeKind::Helper { properties, .. } = &mut self.types.get_mut(id).kind
        {
            *properties = props;
        }
    }

    /// `index N`: an `Integer` constant. `None` after a diagnostic.
    fn property_index(&mut self, e: Option<ast::Expr>) -> Option<i128> {
        let x = self.lower_expr(&e?);
        if x.ty.is_error() {
            return None;
        }
        if !x.ty.is_integer() {
            let t = self.tn(x.ty);
            self.error(
                Code::E(702),
                format!("type mismatch: expected an integer type, found `{t}`"),
                x.span,
            );
            return None;
        }
        match x.value {
            Some(Const::Int(n)) if Ty::Integer.contains(n) => Some(n),
            Some(Const::Int(n)) => {
                self.const_out_of_range(n, Ty::Integer, x.span);
                None
            }
            _ => {
                self.error(Code::E(713), "constant expression required", x.span);
                None
            }
        }
    }

    /// A property named like an earlier property `props` declares, where default array
    /// properties may share a name (`docs/spec/objects.md` 12.2): E0701 for two default array
    /// properties with the same index types, and for a property without `default` sharing the
    /// name of one with it (at the one without). Whether a diagnostic was reported.
    fn default_property_duplicate(
        &mut self,
        props: &[ClassProperty],
        name: &str,
        name_range: TextRange,
        types: &[Ty],
        is_default: bool,
    ) -> bool {
        let same: Vec<&ClassProperty> = props
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .collect();
        if same.is_empty() || (!is_default && same.iter().all(|p| !p.is_default)) {
            return false;
        }
        let d = if let Some(prev) = same.iter().find(|p| p.is_default != is_default) {
            // The diagnostic goes to the one without `default`
            let (at, other) = if is_default {
                (prev.span, name_range)
            } else {
                (name_range, prev.span)
            };
            Diagnostic::error(
                Code::E(701),
                format!("`{name}` is already declared"),
                Stage::Sema,
            )
            .with_primary(self.span(at), "declared again here")
            .with_secondary(self.span(other), "declared here")
        } else if let Some(prev) = same.iter().find(|p| p.params == types) {
            Diagnostic::error(
                Code::E(701),
                format!("overloads of `{name}` have the same parameters"),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "declared again here")
            .with_secondary(self.span(prev.span), "declared here")
        } else {
            return false;
        };
        self.push(d);
        true
    }

    /// The checks of a default array property of type `id` (`docs/spec/objects.md` 12.2, 12.3):
    /// E0756 for a name other than that of the type's first default array property, and for one
    /// indexed by `TIndexRange` returning a class managed by hand.
    #[allow(clippy::too_many_arguments)]
    fn check_default_property(
        &mut self,
        id: TypeId,
        props: &[ClassProperty],
        name: &str,
        name_range: TextRange,
        types: &[Ty],
        ty: Ty,
        ts: &ast::TypeSpec,
    ) {
        if let Some(first) = props.iter().find(|p| p.is_default)
            && !first.name.eq_ignore_ascii_case(name)
        {
            let t = self.tn(Ty::Class(id));
            let d = Diagnostic::error(
                Code::E(756),
                format!("default properties of `{t}` must have the same name"),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "")
            .with_secondary(self.span(first.span), "the first default property");
            self.push(d);
        }
        let range = Ty::Record(super::types::TINDEXRANGE);
        if types.contains(&range)
            && let Ty::Class(c) = ty
            && c != super::types::TOBJECT
            && !self.types.is_descendant(c, super::types::TREFOBJECT)
        {
            let t = self.tn(ty);
            let d = Diagnostic::error(
                Code::E(756),
                format!("a default property indexed by `TIndexRange` cannot return a manual class `{t}`"),
                Stage::Sema,
            )
            .with_primary(self.span(ast::trimmed_range(ts.syntax())), "")
            .with_note(
                "the instance `X[L..H]` makes would leak; return a dynamic array or a reference counted class",
            );
            self.push(d);
        }
    }

    /// The declaration of a member of class `id` itself named `name`, other than an earlier
    /// property in `props` (E0701). Default array properties share their name
    /// (`default_property_duplicate`).
    fn property_duplicate(
        &self,
        id: TypeId,
        props: &[ClassProperty],
        name: &str,
        is_default: bool,
    ) -> Option<TextRange> {
        let same = |n: &str| n.eq_ignore_ascii_case(name);
        props
            .iter()
            .find(|p| same(&p.name) && !(is_default && p.is_default))
            .map(|p| p.span)
            .or_else(|| {
                self.types
                    .class_fields(id)
                    .iter()
                    .find(|(owner, f)| *owner == id && same(&f.name))
                    .map(|(_, f)| f.span)
            })
            .or_else(|| {
                self.types
                    .class_methods(id)
                    .iter()
                    .find(|m| same(&m.name))
                    .map(|m| m.span)
            })
            .or_else(|| {
                self.types
                    .class_statics(id)
                    .iter()
                    .find(|s| same(&s.name))
                    .map(|s| s.span)
            })
    }

    /// The accessor `q` of a property of class `id` (`docs/spec/objects.md` 2.6): `write` for
    /// the `write` specifier. `None` after a diagnostic (E0746, E0743).
    fn property_accessor(
        &mut self,
        id: TypeId,
        draft: &Draft,
        q: &ast::QualifiedName,
        write: bool,
    ) -> Option<Accessor> {
        let text = q.text();
        let range = ast::trimmed_range(q.syntax());
        if text.contains('.') {
            // A field of a record field (`read FPos.X`) has no heading to suggest
            self.error(
                Code::E(746),
                format!("invalid property accessor `{text}`"),
                range,
            );
            return None;
        }
        let simple = draft.params.is_empty() && draft.index.is_none();
        let member = match self.find_member(id, &text, range) {
            Ok(m) => m,
            Err(true) => return None,
            Err(false) => {
                self.invalid_accessor(draft, &text, range, write);
                return None;
            }
        };
        match member {
            Member::Field { .. } | Member::RecField { .. } if draft.is_class => {
                self.class_accessor_error(range);
                None
            }
            Member::Field { slot, ty, name } if simple && self.types.strong(ty) == draft.ty => {
                Some(Accessor::Field { slot, name, ty })
            }
            // A record's field: its number (`docs/spec/objects.md` chapter 6)
            Member::RecField {
                index: slot,
                ty,
                name,
            } if simple && self.types.strong(ty) == draft.ty => {
                Some(Accessor::Field { slot, name, ty })
            }
            Member::Static(_, st) => match st.kind {
                StaticKind::Var(g, ty)
                    if draft.is_class && simple && self.types.strong(ty) == draft.ty =>
                {
                    Some(Accessor::ClassVar(g))
                }
                _ => {
                    self.invalid_accessor(draft, &text, range, write);
                    None
                }
            },
            Member::Methods { name, .. } => {
                let candidates: Vec<ClassMethod> = self
                    .methods_named(id, &name)
                    .into_iter()
                    .filter(|(c, m)| m.routine.is_some() && self.can_see(m.vis, *c))
                    .map(|(_, m)| m)
                    .collect();
                let wanted = if draft.is_class {
                    SelfKind::Static
                } else {
                    SelfKind::Instance
                };
                if draft.is_class && candidates.iter().all(|m| m.self_kind != wanted) {
                    self.class_accessor_error(range);
                    return None;
                }
                let mut types: Vec<Ty> = Vec::new();
                if draft.index.is_some() {
                    types.push(Ty::Integer);
                }
                types.extend(draft.params.iter().map(|(_, t)| *t));
                if write {
                    types.push(draft.ty);
                }
                let found = candidates.iter().find(|m| {
                    m.self_kind == wanted
                        && m.params.len() == types.len()
                        && m.params.iter().zip(&types).all(|((t, mode), want)| {
                            t == want && matches!(mode, ParamMode::Value | ParamMode::Const)
                        })
                        && if write {
                            m.kind == MethodKind::Procedure
                        } else {
                            m.kind == MethodKind::Function && m.ret == Some(draft.ty)
                        }
                });
                match found {
                    Some(m) => m.routine.map(Accessor::Method),
                    None => {
                        self.invalid_accessor(draft, &text, range, write);
                        None
                    }
                }
            }
            _ => {
                self.invalid_accessor(draft, &text, range, write);
                None
            }
        }
    }

    /// E0746 for an accessor that does not fit the property, with the heading it should have
    /// (`docs/spec/objects.md` chapter 16, notes).
    fn invalid_accessor(&mut self, draft: &Draft, text: &str, range: TextRange, write: bool) {
        let ty = self.tn(draft.ty);
        let mut params: Vec<String> = Vec::new();
        if draft.index.is_some() {
            params.push("Index: Integer".into());
        }
        for (n, t) in &draft.params {
            params.push(format!("{n}: {}", self.tn(*t)));
        }
        if write {
            params.push(format!("Value: {ty}"));
        }
        let list = if params.is_empty() {
            String::new()
        } else {
            format!("({})", params.join("; "))
        };
        let heading = match (write, draft.is_class) {
            (false, false) => format!("function {text}{list}: {ty}"),
            (true, false) => format!("procedure {text}{list}"),
            (false, true) => format!("class function {text}{list}: {ty}; static"),
            (true, true) => format!("class procedure {text}{list}; static"),
        };
        let note = if draft.is_class {
            format!("expected a `class var` of type `{ty}` or `{heading}`")
        } else if draft.in_interface {
            format!("expected `{heading}` of the interface")
        } else {
            format!("expected a field of type `{ty}` or `{heading}`")
        };
        self.error(
            Code::E(746),
            format!("invalid property accessor `{text}`"),
            range,
        )
        .notes
        .push(note);
    }

    /// E0746 for an instance member as the accessor of a `class property` (2.3).
    fn class_accessor_error(&mut self, range: TextRange) {
        self.error(
            Code::E(746),
            "the accessor of a class property must be a `class var` or a static class method",
            range,
        );
    }

    /// Where a property is declared (`None` for a class of no unit).
    pub(super) fn property_decl_span(&self, owner: TypeId, p: &ClassProperty) -> Option<Span> {
        let unit = self.types.class_unit(owner)?;
        Some(Span::new(self.unit_file(unit), p.span))
    }

    /// A property used as a value (`docs/spec/objects.md` 2.6): `recv` is the receiver (unused
    /// for a `class property`), `class` its static class, `indices` the indices written in
    /// `[…]`. `range` is the property's name, `span` the whole expression.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn property_read(
        &mut self,
        recv: Expr,
        class: TypeId,
        owner: TypeId,
        p: &ClassProperty,
        indices: Indices,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let Some(args) = self.property_args(&recv, owner, p, indices, range, span) else {
            return err_expr(span);
        };
        match &p.read {
            None => {
                let text = self.src(span);
                let mut d =
                    Diagnostic::error(Code::E(707), format!("cannot read `{text}`"), Stage::Sema)
                        .with_primary(self.span(span), "a write-only property");
                if let Some(s) = self.property_decl_span(owner, p) {
                    d = d.with_secondary(s, "declared here");
                }
                self.push(d);
                err_expr(span)
            }
            Some(Accessor::Field { slot, name, ty }) if self.types.is_record(owner) => self
                .weak_read(Expr {
                    kind: ExprKind::Field {
                        base: Box::new(recv),
                        index: *slot,
                        name: name.clone(),
                    },
                    ty: *ty,
                    value: None,
                    span,
                }),
            Some(Accessor::Field { slot, name, ty }) => self.weak_read(Expr {
                kind: ExprKind::ObjField {
                    base: Box::new(recv),
                    slot: *slot,
                    name: name.clone(),
                },
                ty: *ty,
                value: None,
                span,
            }),
            Some(Accessor::ClassVar(g)) => self.weak_read(Expr {
                kind: ExprKind::Global(*g),
                ty: self.globals[g.0 as usize].ty,
                value: None,
                span,
            }),
            Some(Accessor::Method(r)) => self.accessor_call(*r, recv, class, args, span),
        }
    }

    /// The arguments of a property's accessor method other than the receiver and the value:
    /// the `index N` value and the indices. `None` after a diagnostic (an instance property
    /// without an instance, the wrong number of indices, an index of the wrong type).
    fn property_args(
        &mut self,
        recv: &Expr,
        owner: TypeId,
        p: &ClassProperty,
        indices: Indices,
        range: TextRange,
        span: TextRange,
    ) -> Option<Vec<Expr>> {
        let lower_all = |this: &mut Self, indices: &Indices| {
            if let Indices::Ast(v) = indices {
                for ix in v {
                    this.lower_expr(ix);
                }
            }
        };
        if !p.is_class && Self::no_instance(recv) {
            lower_all(self, &indices);
            let decl = self.property_decl_span(owner, p);
            self.instance_member_error(&p.name, decl, range);
            return None;
        }
        if indices.len() != p.params.len() {
            lower_all(self, &indices);
            self.error(
                Code::E(705),
                format!(
                    "wrong number of indices: expected {}, found {}",
                    p.params.len(),
                    indices.len()
                ),
                span,
            );
            return None;
        }
        let checks = self.checks_at(span.start());
        let mut out = Vec::new();
        if let Some(n) = p.index {
            out.push(Expr {
                kind: ExprKind::Const { explicit: false },
                ty: Ty::Integer,
                value: Some(Const::Int(n)),
                span,
            });
        }
        let mut failed = false;
        let typed: Vec<Expr> = match indices {
            Indices::Ast(v) => v
                .iter()
                .zip(&p.params)
                .map(|(ix, t)| self.lower_expected(ix, *t))
                .collect(),
            Indices::Lowered(v) => v,
        };
        for (e, t) in typed.into_iter().zip(&p.params) {
            let e = if e.ty.is_error() {
                e
            } else {
                self.coerce(e, *t, checks.range, None)
            };
            failed |= e.ty.is_error();
            out.push(e);
        }
        (!failed).then_some(out)
    }

    /// A call of the accessor method `r` on `recv` (dropped for a static method) with `args`.
    fn accessor_call(
        &mut self,
        r: RoutineId,
        recv: Expr,
        class: TypeId,
        args: Vec<Expr>,
        span: TextRange,
    ) -> Expr {
        let Some((owner, m)) = self.method_of_routine(r) else {
            return err_expr(span);
        };
        let dispatch = self.method_dispatch(class, &m, false);
        let mut call_args = Vec::new();
        if m.self_kind == SelfKind::Instance && self.types.self_by_ref(owner) {
            // `Self` of a record (`docs/spec/objects.md` chapter 6)
            let r = self.record_receiver(recv);
            call_args.push(r);
        } else if m.self_kind == SelfKind::Instance {
            call_args.push(CallArg::Value(recv));
        }
        call_args.extend(args.into_iter().map(CallArg::Value));
        self.record_call(r);
        Expr {
            kind: ExprKind::Call {
                routine: r,
                args: call_args,
                dispatch,
            },
            ty: m.ret.unwrap_or(Ty::Error),
            value: None,
            span,
        }
    }

    /// The property an expression names (`O.P`, `P` inside a method, `inherited P`), without
    /// reporting anything.
    fn named_property(&mut self, e: &ast::Expr) -> Option<ClassProperty> {
        if !matches!(
            e,
            ast::Expr::Field(_) | ast::Expr::NameRef(_) | ast::Expr::Inherited(_)
        ) {
            return None;
        }
        self.dry_run(|s| match s.resolve_name_expr(e) {
            Some((
                Resolved::Member {
                    member: Member::Property(_, p),
                    ..
                },
                _,
                _,
            )) => Some(*p),
            _ => None,
        })
    }

    /// `O.Items[I]` read through an indexed property (`docs/spec/objects.md` 2.6). `None` when
    /// the base is not an indexed property.
    pub(super) fn indexed_property_read(
        &mut self,
        i: &ast::IndexExpr,
        span: TextRange,
    ) -> Option<Expr> {
        let base = i.base()?;
        if self
            .named_property(&base)
            .is_none_or(|p| p.params.is_empty())
        {
            return None;
        }
        let Some(indices) = self.property_indices(i) else {
            return Some(err_expr(span));
        };
        let Some((r, _, range)) = self.resolve_name_expr(&base) else {
            return Some(err_expr(span));
        };
        let Resolved::Member {
            recv,
            class,
            member: Member::Property(owner, p),
            ..
        } = r
        else {
            return Some(err_expr(span));
        };
        let Some((p, indices)) = self.named_selection(owner, *p, indices, span) else {
            return Some(err_expr(span));
        };
        Some(self.property_read(*recv, class, owner, &p, indices, range, span))
    }

    /// `X.Cells[…]` of a default array property chooses among the default array properties of
    /// that name like `X[…]` (`docs/spec/objects.md` 12.2); any other indexed property takes the
    /// indices as written. `None` after a diagnostic.
    fn named_selection(
        &mut self,
        owner: TypeId,
        p: ClassProperty,
        indices: Vec<ast::Expr>,
        span: TextRange,
    ) -> Option<(ClassProperty, Indices)> {
        let cands = self.same_named_defaults(owner, &p);
        if cands.len() <= 1 {
            return Some((p, Indices::Ast(indices)));
        }
        let items: Vec<ast::IndexItem> = indices.into_iter().map(ast::IndexItem::Expr).collect();
        self.select_default(owner, cands, &items, span)
    }

    /// The indices of `O.Items[…]`: E0720 for a slice (an indexed property has no slices).
    fn property_indices(&mut self, i: &ast::IndexExpr) -> Option<Vec<ast::Expr>> {
        let mut out = Vec::new();
        for item in i.indices() {
            match item {
                ast::IndexItem::Expr(e) => out.push(e),
                ast::IndexItem::Slice(sl) => {
                    let r = ast::trimmed_range(sl.syntax());
                    self.error(
                        Code::E(720),
                        "a slice cannot be taken of an indexed property",
                        r,
                    );
                    return None;
                }
            }
        }
        Some(out)
    }

    /// The property an assignment target names (`O.P`, `O.Items[I]`, `X[I]` of a default array
    /// property), without reporting anything. `None` when the target is not a property.
    fn property_target(&mut self, target: &ast::Expr) -> Option<Target> {
        match target {
            ast::Expr::Index(i) => {
                let base = i.base()?;
                let Some(p) = self.named_property(&base) else {
                    // `X[I] := V` (`docs/spec/objects.md` 12.1)
                    return self.indexes_default_property(&base).then(|| Target {
                        name: base,
                        index: Some(i.clone()),
                        default: true,
                    });
                };
                if p.params.is_empty() {
                    // `O.List[1] := …` writes through the default array property of the value
                    // read from the property; `O.Name[1] := …` writes into that value (E0707)
                    return self.indexes_default_property(&base).then(|| Target {
                        name: base,
                        index: Some(i.clone()),
                        default: true,
                    });
                }
                Some(Target {
                    name: base,
                    index: Some(i.clone()),
                    default: false,
                })
            }
            _ => {
                self.named_property(target)?;
                Some(Target {
                    name: target.clone(),
                    index: None,
                    default: false,
                })
            }
        }
    }

    /// The receiver, its static class, the property's owner, the property and the indices of the
    /// target of `O.P := X`, `O.Items[I] := X` or `X[I] := X`. `None` after a diagnostic.
    fn target_property(
        &mut self,
        t: &Target,
    ) -> Option<(Expr, TypeId, TypeId, ClassProperty, Indices, TextRange)> {
        let whole = ast::trimmed_range(t.index.as_ref().map_or(t.name.syntax(), |i| i.syntax()));
        if t.default {
            let i = t.index.as_ref()?;
            let recv = self.lower_expr(&t.name);
            let items = i.indices();
            let (owner, cands) = self.default_properties(recv.ty)?;
            let class = Self::indexable_type(recv.ty).unwrap_or(owner);
            let (p, indices) = self.select_default(owner, cands, &items, whole)?;
            return Some((recv, class, owner, p, indices, whole));
        }
        let Some((
            Resolved::Member {
                recv,
                class,
                member: Member::Property(owner, p),
                ..
            },
            _,
            range,
        )) = self.resolve_name_expr(&t.name)
        else {
            return None;
        };
        // The same indices as a read: a slice is E0720
        let (p, indices) = match &t.index {
            Some(i) => {
                let v = self.property_indices(i)?;
                self.named_selection(owner, *p, v, whole)?
            }
            None => (*p, Indices::Ast(Vec::new())),
        };
        Some((*recv, class, owner, p, indices, range))
    }

    /// `O.P := X` (`docs/spec/objects.md` 2.6), `X[I] := V` (12.1): the write of the field or
    /// `class var`, or the call of the `write` method with the receiver, the indices and `X`,
    /// evaluated in that order. The outer `None` when the target is not a property.
    pub(super) fn assign_property(
        &mut self,
        target: &ast::Expr,
        value: Option<ast::Expr>,
        span: TextRange,
    ) -> Option<Option<StmtKind>> {
        let t = self.property_target(target)?;
        let whole = ast::trimmed_range(target.syntax());
        let Some((recv, class, owner, p, indices, range)) = self.target_property(&t) else {
            if let Some(v) = &value {
                self.lower_expr(v);
            }
            return Some(None);
        };
        let recv = Box::new(recv);
        let value = value.map(|v| self.lower_expected(&v, p.ty));
        let args = self.property_args(&recv, owner, &p, indices, range, whole);
        let (Some(value), Some(mut args)) = (value, args) else {
            return Some(None);
        };
        self.check_self_cycle(target, &value);
        let checks = self.checks_at(span.start());
        let value = if value.ty.is_error() || p.ty.is_error() {
            value
        } else {
            self.coerce(value, p.ty, checks.range, None)
        };
        let target = match &p.write {
            None => {
                let decl = self.property_decl_span(owner, &p);
                self.cannot_assign_decl(whole, "a read-only property", decl);
                return Some(None);
            }
            Some(Accessor::Field { slot, name, ty }) if self.types.is_record(owner) => {
                // A field of the record the receiver names (`docs/spec/objects.md` chapter 6)
                let Some(base) = self.receiver_place(&recv) else {
                    self.cannot_assign(whole, "not a variable", None);
                    return Some(None);
                };
                Place::Field {
                    base: Box::new(base),
                    index: *slot,
                    ty: *ty,
                    name: name.clone(),
                }
            }
            Some(Accessor::Field { slot, name, ty }) => Place::ObjField {
                base: recv,
                slot: *slot,
                ty: *ty,
                name: name.clone(),
            },
            Some(Accessor::ClassVar(g)) => Place::Global(*g),
            Some(Accessor::Method(_))
                if self.types.self_by_ref(owner) && self.receiver_place(&recv).is_none() =>
            {
                // The write would go to a copy (`docs/spec/objects.md` chapter 6)
                self.cannot_assign(whole, "not a variable", None);
                return Some(None);
            }
            Some(Accessor::Method(r)) => {
                args.push(value);
                let call = self.accessor_call(*r, *recv, class, args, whole);
                if !matches!(call.kind, ExprKind::Call { .. }) {
                    return Some(None);
                }
                return Some(Some(StmtKind::PropertySet { name: p.name, call }));
            }
        };
        Some(Some(StmtKind::Assign {
            target,
            value,
            check: checks,
            is_init: false,
        }))
    }

    /// A property as an assignment target that is not assigned as a whole
    /// (`docs/spec/objects.md` 2.6): passed to a `var` / `out` parameter, `Inc` / `Dec`,
    /// `ReadLn` (E0706), or a field or element of it written (E0707). `diag` is the target.
    pub(super) fn property_place_error(&mut self, usage: PlaceUse, diag: TextRange) {
        match usage {
            PlaceUse::RefArg { is_out: true } => {
                self.error(
                    Code::E(706),
                    "a property cannot be passed to an `out` parameter",
                    diag,
                );
            }
            PlaceUse::RefArg { .. }
            | PlaceUse::IncDec
            | PlaceUse::ReadLn
            | PlaceUse::SetLength
            | PlaceUse::Container { var_arg: true, .. } => {
                self.error(
                    Code::E(706),
                    "a property cannot be passed to a `var` parameter",
                    diag,
                );
            }
            PlaceUse::Container { .. } => {
                self.cannot_assign(diag, "part of a property", None);
                self.last_mut().notes.push(
                    "read the property into a variable, change it, and assign it back".into(),
                );
            }
            PlaceUse::Assign => self.cannot_assign(diag, "not a variable", None),
        }
    }

    /// Records a call for the static link (`docs/spec/semantics.md` 4.9).
    pub(super) fn record_call(&mut self, id: RoutineId) {
        if self.dry_run == 0
            && let Some(c) = &mut self.cur
            && !c.calls.contains(&id)
        {
            c.calls.push(id);
        }
    }
}
