//! The members of a class that are not part of its instances (`docs/spec/objects.md` 2.2):
//! `class var`, class constants and nested types. Their names are found in the class body after
//! their declaration, in the methods of the class and its descendants, and through the class's
//! name (`TFoo.Count`, `TFoo.TItem`).

use super::hir::*;
use super::types::{ClassStatic, StaticKind, Ty, TypeId, Visibility};
use super::{Resolved, Sema};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::{Span, TextRange};
use crate::syntax::ast::{self, AstNode};

/// The class whose body is being declared (its nested types and constants are seen by the rest
/// of the body).
pub(super) struct ClassScope {
    pub class: TypeId,
    /// The visibility of the section being declared.
    pub vis: Visibility,
    /// The names the class has declared so far (fields and the members here), lowercase, with
    /// their positions: a second declaration of a name is E0701.
    pub names: Vec<(String, TextRange)>,
}

impl<'a, 's> Sema<'a, 's> {
    /// The class whose body is being declared, if any.
    pub(super) fn class_scope(&self) -> Option<TypeId> {
        self.class_scopes.last().map(|c| c.class)
    }

    /// E0701 when the class being declared already has a member named `lower`.
    pub(super) fn class_duplicate(&mut self, lower: &str, range: TextRange) -> bool {
        if self.check_reserved_name(lower, range) {
            return true;
        }
        let Some(prev) = self
            .class_scopes
            .last()
            .and_then(|c| c.names.iter().find(|(n, _)| n == lower))
            .map(|(_, s)| *s)
        else {
            if let Some(c) = self.class_scopes.last_mut() {
                c.names.push((lower.to_string(), range));
            }
            return false;
        };
        let name = self.src(range);
        let d = Diagnostic::error(
            Code::E(701),
            format!("`{name}` is already declared"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "declared again here")
        .with_secondary(self.span(prev), "declared here");
        self.push(d);
        true
    }

    /// Adds a member to the class being declared.
    fn add_static(&mut self, name: String, span: TextRange, kind: StaticKind) {
        let Some(scope) = self.class_scopes.last() else {
            return;
        };
        let (class, vis) = (scope.class, scope.vis);
        if let super::types::TypeKind::Class { statics, .. }
        | super::types::TypeKind::Record { statics, .. }
        | super::types::TypeKind::Helper { statics, .. } = &mut self.types.get_mut(class).kind
        {
            statics.push(ClassStatic {
                name,
                span,
                vis,
                kind,
            });
        }
    }

    /// A nested type of the class being declared (`bind_type` inside a class body).
    pub(super) fn bind_nested_type(&mut self, name: String, ty: Ty, span: TextRange) {
        self.add_static(name, span, StaticKind::Type(ty));
    }

    /// Sets the type a nested alias of the class being declared denotes once it is resolved.
    pub(super) fn rebind_nested_type(&mut self, lower: &str, ty: Ty) -> bool {
        let Some(class) = self.class_scope() else {
            return false;
        };
        if let super::types::TypeKind::Class { statics, .. }
        | super::types::TypeKind::Record { statics, .. }
        | super::types::TypeKind::Helper { statics, .. } = &mut self.types.get_mut(class).kind
            && let Some(s) = statics
                .iter_mut()
                .rev()
                .find(|s| s.name.eq_ignore_ascii_case(lower))
        {
            s.kind = StaticKind::Type(ty);
            return true;
        }
        false
    }

    /// `class var` (`docs/spec/objects.md` 2.2): a global variable of the class, named
    /// `TFoo.Count` in the bytecode.
    pub(super) fn declare_class_vars(&mut self, section: &ast::ClassVarSection) {
        let Some(class) = self.class_scope() else {
            return;
        };
        let class_name = self.types.name(Ty::Class(class));
        for fd in section.fields() {
            let scope = self.declare_member_hints(fd.syntax(), &fd.names());
            self.begin_names_hints(scope);
            let ty = self.resolve_type(fd.type_spec());
            self.end_names_hints(scope);
            let ty = self.decl_attributes(fd.syntax(), super::attrs::AttrSite::Field, ty);
            for n in fd.names() {
                let name = self.text(&n).to_string();
                let lower = name.to_ascii_lowercase();
                let span = n.text_range();
                if self.class_duplicate(&lower, span) {
                    continue;
                }
                let id = GlobalId(self.globals.len() as u32);
                self.globals.push(Global {
                    name: format!("{class_name}.{name}"),
                    ty,
                    span: Some(span),
                    unit: Some(self.unit_index),
                });
                if self.types.is_generic(class) {
                    // Each instantiation has its own (`docs/spec/generics.md` 2.5)
                    let k = self.types.class_statics(class).len();
                    self.instances.global_owner.insert(id, (class, k));
                }
                self.add_static(name, span, StaticKind::Var(id, ty));
            }
        }
    }

    /// The class constants of a `const` section in a class body (`docs/spec/objects.md` 2.2).
    pub(super) fn declare_class_consts(&mut self, section: &ast::ConstSection) {
        for decl in section.decls() {
            // `docs/spec/attributes.md` 1.2
            self.attributes_not_allowed(decl.syntax());
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let scope = self.declare_member_hints(decl.syntax(), &[name_tok]);
            self.begin_names_hints(scope);
            let (value, ty, explicit) = self.eval_const_decl(decl.type_ref(), decl.value(), false);
            self.end_names_hints(scope);
            if self.class_duplicate(&lower, range) {
                continue;
            }
            self.note_typed_const(decl.type_ref().is_some(), range);
            self.note_explicit_const(explicit, range);
            let kind = match value {
                Some(v) => StaticKind::Const(v, ty),
                None => StaticKind::Const(super::types::Const::Int(0), Ty::Error),
            };
            self.add_static(name, range, kind);
        }
    }

    /// The member `name` of the classes whose body is being declared, or of the class of the
    /// method being analyzed and its ancestors (`docs/spec/objects.md` 2.2): the nearest visible
    /// one.
    pub(super) fn scoped_static(&mut self, name: &str) -> Option<(TypeId, ClassStatic)> {
        let mut classes: Vec<TypeId> = Vec::new();
        // The class of the method and the classes it is nested in, then the class bodies being
        // declared (innermost first) and the classes they are nested in
        let starts: Vec<TypeId> = self
            .method_class()
            .into_iter()
            .chain(self.class_scopes.iter().rev().map(|c| c.class))
            .collect();
        for c in starts {
            let mut cur = Some(c);
            while let Some(k) = cur {
                if classes.contains(&k) {
                    break;
                }
                classes.push(k);
                cur = self.lexical_outer.get(&k).copied();
            }
        }
        for c in classes {
            if let Some(found) = self.find_static(c, name) {
                return Some(found);
            }
        }
        None
    }

    /// The name declaring a `class var`, class constant or nested type of `owner`.
    pub(super) fn static_decl_span(&self, owner: TypeId, st: &ClassStatic) -> Option<Span> {
        self.types
            .class_unit(owner)
            .map(|u| Span::new(self.unit_file(u), st.span))
    }

    /// The visible member `name` of a class or its ancestors, the nearest first.
    pub(super) fn find_static(&self, class: TypeId, name: &str) -> Option<(TypeId, ClassStatic)> {
        for c in self.types.class_chain(class) {
            if let Some(s) = self
                .types
                .class_statics(c)
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(name))
                && self.can_see(s.vis, c)
            {
                return Some((c, s.clone()));
            }
        }
        None
    }

    /// Whether a dotted name reaches a `class var` or class constant through a class's name before
    /// its last part (`TFoo.Pos.X`, `TOuter.TInner.Count.Y`): the part after it is read from that
    /// value, as from a variable (`docs/spec/objects.md` 2.2).
    pub(super) fn class_value_chain(&self, e: &ast::Expr) -> bool {
        let Some(chain) = super::name_chain(e) else {
            return false;
        };
        let [(first, _), rest @ .., _] = chain.as_slice() else {
            return false;
        };
        let lower = first.to_ascii_lowercase();
        let root = match self.lookup_scopes(&lower) {
            Some((_, super::LSym::Type { ty, .. })) => Some(ty),
            Some(_) => None,
            None => self
                .global_scope
                .iter()
                .rev()
                .find(|(n, _)| *n == lower)
                .and_then(|(_, sym)| match sym {
                    super::GSym::Type { ty, .. } => Some(*ty),
                    _ => None,
                }),
        };
        let Some(Ty::Class(mut c)) = root else {
            return false;
        };
        for (name, _) in rest {
            match self.find_static(c, name).map(|(_, s)| s.kind) {
                Some(StaticKind::Type(Ty::Class(n))) => c = n,
                Some(StaticKind::Var(..) | StaticKind::Const(..)) => return true,
                _ => return false,
            }
        }
        false
    }

    /// The class a dotted name such as `TOuter.TInner` names (no diagnostics).
    pub(super) fn class_named(&mut self, text: &str) -> Option<TypeId> {
        self.owner_named(text).filter(|&c| !self.types.is_record(c))
    }

    /// The class or record a dotted name such as `TOuter.TInner` or `TRec.TInner` names (no
    /// diagnostics): a type whose nested types and constants are named through it.
    pub(super) fn owner_named(&mut self, text: &str) -> Option<TypeId> {
        let mut parts = text.split('.');
        let first = parts.next()?;
        let Some(Some(mut ty)) = self.lookup_type_symbol(&first.to_ascii_lowercase()) else {
            return None;
        };
        for p in parts {
            let c = nested_owner(ty)?;
            match self.find_static(c, p) {
                Some((
                    _,
                    ClassStatic {
                        kind: StaticKind::Type(t),
                        ..
                    },
                )) => ty = t,
                _ => return None,
            }
        }
        nested_owner(ty)
    }

    /// The resolution of a class member that is not part of the instances.
    pub(super) fn static_resolved(&self, owner: TypeId, s: &ClassStatic) -> Resolved {
        match &s.kind {
            StaticKind::Var(g, _) => Resolved::Global(*g),
            StaticKind::Const(value, ty) => Resolved::Const {
                value: value.clone(),
                ty: *ty,
                span: self
                    .types
                    .class_unit(owner)
                    .map(|u| Span::new(self.unit_file(u), s.span)),
            },
            StaticKind::Type(t) => Resolved::Type(*t),
        }
    }

    /// A class member that is not part of the instances used as a value: the `class var`'s
    /// global, or the constant (a nested type is E0708).
    pub(super) fn static_expr(
        &mut self,
        owner: TypeId,
        st: &ClassStatic,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        match self.static_resolved(owner, st) {
            Resolved::Global(g) => self.weak_read(Expr {
                kind: ExprKind::Global(g),
                ty: self.globals[g.0 as usize].ty,
                value: None,
                span,
            }),
            Resolved::Const {
                value,
                ty,
                span: decl,
            } if !ty.is_error() => Expr {
                kind: ExprKind::Const {
                    explicit: self.is_explicit_const(decl),
                },
                ty,
                value: Some(value),
                span,
            },
            Resolved::Type(t) => {
                let n = self.tn(t);
                self.error(
                    Code::E(708),
                    format!("`{n}` is a type and cannot be used as a value"),
                    range,
                );
                super::expr::err_expr(span)
            }
            _ => super::expr::err_expr(span),
        }
    }

    /// `TOuter.TInner.TItem` after `TOuter`: the nested types named by the rest of a qualified
    /// type name, through classes and records (E0700 for a name the type does not have, E0708
    /// for one that is not a type).
    pub(super) fn nested_type(
        &mut self,
        mut ty: Ty,
        rest: &[(String, TextRange)],
        whole: TextRange,
    ) -> Ty {
        for (name, range) in rest {
            let Some(c) = nested_owner(ty) else {
                let text = self.src(whole);
                self.error(Code::E(708), format!("`{text}` is not a type"), whole);
                return Ty::Error;
            };
            match self.static_of(c, name) {
                Some((
                    _,
                    ClassStatic {
                        kind: StaticKind::Type(t),
                        ..
                    },
                )) => ty = t,
                Some(_) => {
                    let text = self.src(whole);
                    self.error(Code::E(708), format!("`{text}` is not a type"), whole);
                    return Ty::Error;
                }
                None => {
                    let t = self.tn(ty);
                    self.error(
                        Code::E(700),
                        format!("`{t}` has no member `{name}`"),
                        *range,
                    );
                    return Ty::Error;
                }
            }
        }
        ty
    }

    /// The `class var`, constant or nested type `name` of a class, as [`Self::find_static`].
    /// An instantiation asked for in the `type` section that declares its definition has no
    /// members until the section's generic types are complete (`docs/spec/generics.md` 4.2):
    /// its nested type is then the definition's, or else one its parent declares (the parent
    /// may be such an instantiation too), with the type parameters replaced: the same type the
    /// instantiation gets when its members are made (`TF = TBox<T>; TE = TF.TInner;`).
    pub(super) fn static_of(&mut self, c: TypeId, name: &str) -> Option<(TypeId, ClassStatic)> {
        if let Some(found) = self.find_static(c, name) {
            return Some(found);
        }
        let def = match self.types.instance_of(c) {
            Some(i) if !i.filled => i.def,
            _ => return None,
        };
        let own = self
            .types
            .class_statics(def)
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
            .filter(|s| self.can_see(s.vis, def))
            .cloned();
        let (owner, mut s) = match own {
            Some(s) => (def, s),
            None => {
                let parent = self.types.parent(def)?;
                self.static_of(parent, name)?
            }
        };
        if let StaticKind::Type(t) = s.kind {
            let env = super::instances::instance_env(&self.types, c)
                .expect("an unfilled instantiation has an entry");
            s.kind = StaticKind::Type(self.with_inst(|i| i.subst(t, &env)));
        }
        Some((owner, s))
    }
}

/// The class or record whose nested types a name after it reaches (`TOuter.TInner`,
/// `TRec.TInner`; `docs/spec/objects.md` 2.2, `docs/spec/generics.md` 2.5).
pub(super) fn nested_owner(ty: Ty) -> Option<TypeId> {
    match ty {
        Ty::Class(c) | Ty::Record(c) => Some(c),
        _ => None,
    }
}
