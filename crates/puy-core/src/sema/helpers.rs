//! Class helpers and record helpers (`docs/spec/objects.md` chapter 10): their declarations, the
//! rules on what they may declare, and finding a member through the helpers of a type.

use super::expr::err_expr;
use super::hir::{Expr, ExprKind};
use super::methods::Member;
use super::types::{BuiltinHelper, SelfKind, Ty, TypeDef, TypeId, TypeKind, Visibility};
use super::{GSym, Resolved, Sema};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{Span, TextRange};
use crate::syntax::ast::{self, AstNode};

/// How the code reaches a member found through the helpers (the note and fixes of E0754).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HelperRecv {
    /// `X.Name`: the receiver `X` is written at the range.
    Written(TextRange),
    /// No fixes to offer: a member written to as a variable, or a broken receiver.
    Implicit,
    /// `for..in` calls `GetEnumerator`, `MoveNext` or `Current` itself (`docs/spec/objects.md` 13).
    ForIn,
}

/// A member found in the helpers of a type (`docs/spec/objects.md` 10.4).
#[derive(Debug, Clone)]
pub(super) enum HelperMember {
    /// A member a helper declares (or its parent helper), found as by `find_member` on it.
    Declared,
    /// A member of a built-in helper (10.7).
    Builtin(BuiltinHelper),
}

/// The built-in helpers of `System` (`docs/spec/objects.md` 10.7): name (`None` for the unnamed
/// ones), and the member.
const BUILTIN_HELPERS: &[(Option<&str>, BuiltinHelper)] = &[
    (Some("TByteHelper"), BuiltinHelper::ToString),
    (Some("TShortIntHelper"), BuiltinHelper::ToString),
    (Some("TWordHelper"), BuiltinHelper::ToString),
    (Some("TSmallIntHelper"), BuiltinHelper::ToString),
    (Some("TCardinalHelper"), BuiltinHelper::ToString),
    (Some("TIntegerHelper"), BuiltinHelper::ToString),
    (Some("TUInt64Helper"), BuiltinHelper::ToString),
    (Some("TInt64Helper"), BuiltinHelper::ToString),
    (Some("TSingleHelper"), BuiltinHelper::ToString),
    (Some("TDoubleHelper"), BuiltinHelper::ToString),
    (Some("TCurrencyHelper"), BuiltinHelper::ToString),
    (Some("TBooleanHelper"), BuiltinHelper::ToString),
    (Some("TCharHelper"), BuiltinHelper::ToString),
    (Some("TStringHelper"), BuiltinHelper::Length),
    (Some("TUTF8StringHelper"), BuiltinHelper::Length),
];

/// The id of the named built-in helper called `lower` (lowercase): they follow `TProc` in the
/// type table ([`super::types::TypeTable::new`]).
pub(super) fn builtin_helper_id(lower: &str) -> Option<TypeId> {
    BUILTIN_HELPERS
        .iter()
        .position(|(n, _)| n.is_some_and(|n| n.eq_ignore_ascii_case(lower)))
        .map(|i| TypeId(super::types::TPROC.0 + 1 + i as u32))
}

/// The type a named built-in helper is for.
fn builtin_target(name: &str) -> Ty {
    match name {
        "TByteHelper" => Ty::Byte,
        "TShortIntHelper" => Ty::ShortInt,
        "TWordHelper" => Ty::Word,
        "TSmallIntHelper" => Ty::SmallInt,
        "TCardinalHelper" => Ty::Cardinal,
        "TIntegerHelper" => Ty::Integer,
        "TUInt64Helper" => Ty::UInt64,
        "TInt64Helper" => Ty::Int64,
        "TSingleHelper" => Ty::Single,
        "TDoubleHelper" => Ty::Double,
        "TCurrencyHelper" => Ty::Currency,
        "TBooleanHelper" => Ty::Boolean,
        "TCharHelper" => Ty::Char,
        "TStringHelper" => Ty::String,
        _ => Ty::Utf8String,
    }
}

/// The member of the built-in helper of a type (`docs/spec/objects.md` 10.7), if it has one:
/// `ToString` for the integer, real, `Boolean`, `Char` and enumeration types, `Length` for the
/// strings and the dynamic arrays.
pub(super) fn builtin_member(ty: Ty) -> Option<BuiltinHelper> {
    match ty {
        t if t.is_integer() || t.is_real() => Some(BuiltinHelper::ToString),
        Ty::Boolean | Ty::Char | Ty::Enum(_) => Some(BuiltinHelper::ToString),
        Ty::String | Ty::Utf8String | Ty::DynArray(_) => Some(BuiltinHelper::Length),
        _ => None,
    }
}

impl super::types::TypeTable {
    /// Adds the named built-in helpers (`docs/spec/objects.md` 10.7); returns the id of each, in
    /// the order of [`BUILTIN_HELPERS`].
    pub(super) fn add_builtin_helpers(&mut self) -> Vec<TypeId> {
        BUILTIN_HELPERS
            .iter()
            .map(|(name, member)| {
                let name = name.expect("the listed built-in helpers have names");
                self.add(TypeDef {
                    name: Some(name.to_string()),
                    full_name: Some(name.to_string()),
                    kind: TypeKind::Helper {
                        is_class: false,
                        target: builtin_target(name),
                        parent: None,
                        unit: None,
                        methods: Vec::new(),
                        statics: Vec::new(),
                        properties: Vec::new(),
                        builtin: Some(*member),
                    },
                    span: TextRange::default(),
                })
            })
            .collect()
    }

    /// The member of a built-in helper (`None` for a helper the user declared).
    pub fn helper_builtin(&self, id: TypeId) -> Option<BuiltinHelper> {
        match self.get(id).kind {
            TypeKind::Helper { builtin, .. } => builtin,
            _ => None,
        }
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// `THelper = class helper(TParent) for T … end` (`docs/spec/objects.md` 10.1, 10.2, 10.6):
    /// the type, the parent helper, and the members (the methods and properties are defined
    /// with the classes' in `define_pending_methods`).
    pub(super) fn define_helper(&mut self, id: TypeId, h: &ast::HelperType) {
        let is_class = h.is_class();
        let target = self.helper_target_type(h, is_class);
        let parent = self.helper_parent(id, h, target);
        self.types.get_mut(id).kind = TypeKind::Helper {
            is_class,
            target,
            parent,
            unit: Some(self.unit_index),
            methods: Vec::new(),
            statics: Vec::new(),
            properties: Vec::new(),
            builtin: None,
        };
        let mut methods = Vec::new();
        let mut properties = Vec::new();
        let mut vis = Visibility::Public;
        let members: Vec<ast::ClassMember> =
            h.body().map(|b| b.members().collect()).unwrap_or_default();
        if let Some(outer) = self.class_scope() {
            self.lexical_outer.insert(id, outer);
        }
        self.class_scopes.push(super::statics::ClassScope {
            class: id,
            vis: Visibility::Public,
            names: Vec::new(),
        });
        for m in members {
            match m {
                ast::ClassMember::Visibility(v) => {
                    vis = match v.keyword().map(|k| k.kind()) {
                        Some(crate::syntax::SyntaxKind::PRIVATE_KW) if v.is_strict() => {
                            Visibility::StrictPrivate
                        }
                        Some(crate::syntax::SyntaxKind::PRIVATE_KW) => Visibility::Private,
                        Some(crate::syntax::SyntaxKind::PROTECTED_KW) if v.is_strict() => {
                            Visibility::StrictProtected
                        }
                        Some(crate::syntax::SyntaxKind::PROTECTED_KW) => Visibility::Protected,
                        _ => Visibility::Public,
                    };
                    if let Some(scope) = self.class_scopes.last_mut() {
                        scope.vis = vis;
                    }
                }
                ast::ClassMember::Method(m) => methods.push((m, vis)),
                ast::ClassMember::Property(p) => properties.push((p, vis)),
                ast::ClassMember::Var(v) if v.is_class() => self.declare_class_vars(&v),
                ast::ClassMember::Const(cs) => self.declare_class_consts(&cs),
                ast::ClassMember::Type(ts) => self.declare_types(&ts),
                ast::ClassMember::Field(f) => self.helper_field(f.syntax()),
                ast::ClassMember::Var(v) => self.helper_field(v.syntax()),
                ast::ClassMember::Resolution(r) => {
                    let range = ast::trimmed_range(r.syntax());
                    let text = self.src(range);
                    self.not_in_helper(&text, range);
                }
            }
        }
        self.class_scopes.pop();
        self.pending_classes.push((id, methods, properties));
    }

    /// Whether the members of `owner` are limited to the public ones in the helper `h`
    /// (`docs/spec/objects.md` 10.8): `owner` is the type `h` is for (a class's ancestors
    /// included). Other types' members are seen as from any code of the unit.
    pub(super) fn helper_restricts(&self, h: TypeId, owner: TypeId) -> bool {
        match self.types.helper_target(h) {
            Some(Ty::Class(t)) => self.types.class_chain(t).contains(&owner),
            Some(Ty::Record(r)) => owner == r,
            _ => false,
        }
    }

    /// E0753 for a member a helper cannot have (`docs/spec/objects.md` 10.2): `what` is the word
    /// as written.
    pub(super) fn not_in_helper(&mut self, what: &str, range: TextRange) {
        self.error(
            Code::E(753),
            format!("`{what}` is not allowed in a helper"),
            range,
        );
    }

    /// E0753 for a field in a helper (`docs/spec/objects.md` 10.2).
    fn helper_field(&mut self, node: &crate::syntax::SyntaxNode) {
        let range = ast::trimmed_range(node);
        self.error(Code::E(753), "helpers cannot have fields", range)
            .notes
            .push("declare a `class var`, or add the field to the type itself".into());
    }

    /// The type after `for` (`docs/spec/objects.md` 10.2): E0753 for a type that cannot have a
    /// helper of this kind (`Ty::Error` then).
    fn helper_target_type(&mut self, h: &ast::HelperType, is_class: bool) -> Ty {
        let Some(spec) = h.target() else {
            return Ty::Error;
        };
        let range = ast::trimmed_range(spec.syntax());
        let ty = self.resolve_type(Some(spec));
        let for_record = matches!(
            ty,
            Ty::Record(_)
                | Ty::Array(_)
                | Ty::DynArray(_)
                | Ty::Enum(_)
                | Ty::Subrange(_)
                | Ty::Set(_)
                | Ty::BigSet(_)
                | Ty::String
                | Ty::Utf8String
                | Ty::Boolean
                | Ty::Char
                | Ty::Currency
                // `docs/spec/types.md` 1.3
                | Ty::Distinct(_)
        ) || ty.is_integer()
            || ty.is_real();
        let for_class = matches!(ty, Ty::Class(_));
        if ty.is_error() || (is_class && for_class) || (!is_class && for_record) {
            return ty;
        }
        let t = self.tn(ty);
        let kw = h.kind_token();
        let d = self.error(Code::E(753), format!("`{t}` cannot have a helper"), range);
        d.notes.push(
            "a class has a `class helper`, and the other types that can have a helper have a `record helper`"
                .into(),
        );
        if (is_class && for_record) || (!is_class && for_class) {
            let (with, title) = if is_class {
                ("record", "use `record helper`")
            } else {
                ("class", "use `class helper`")
            };
            if let Some(kw) = kw {
                let file = self.input.file;
                let d = self.last_mut();
                d.fixes.push(Fix::safe(
                    title,
                    vec![TextEdit::replace(file, kw.text_range(), with)],
                ));
            }
        }
        Ty::Error
    }

    /// The parent helper in parentheses (`docs/spec/objects.md` 10.6): a helper for the same type
    /// (or, for a class helper, for an ancestor of the class). E0753 otherwise (`None` then).
    fn helper_parent(&mut self, id: TypeId, h: &ast::HelperType, target: Ty) -> Option<TypeId> {
        // A name with type arguments (`docs/spec/generics.md` 1.3): no helper is generic, so it
        // is a generic type that is not a helper, or E0761 for a helper's name
        if let Some(g) = h.parent_list().and_then(|l| {
            l.children()
                .find(|n| n.kind() == crate::syntax::SyntaxKind::GENERIC_TYPE_REF)
        }) {
            let range = g.text_range();
            let t = self.resolve_type(ast::TypeSpec::cast(g));
            if !t.is_error() {
                let t = self.tn(t);
                self.error(Code::E(753), format!("`{t}` is not a helper"), range);
            }
            return None;
        }
        let p = h.parent()?;
        let range = p.syntax().text_range();
        let pid = match self.resolve_type_name(&p) {
            Ty::Helper(pid) => pid,
            Ty::Error => return None,
            other => {
                let t = self.tn(other);
                self.error(Code::E(753), format!("`{t}` is not a helper"), range);
                return None;
            }
        };
        if pid == id || self.types.class_chain(pid).contains(&id) {
            let name = self.tn(Ty::Helper(id));
            let span = self.types.get(id).span;
            let d = Diagnostic::error(
                Code::E(727),
                format!("type `{name}` inherits from itself"),
                Stage::Sema,
            )
            .with_primary(self.span(span), "")
            .with_secondary(self.span(range), "part of the cycle");
            self.push(d);
            return None;
        }
        let ptarget = self.types.helper_target(pid).unwrap_or(Ty::Error);
        let fits = match (ptarget, target) {
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Class(a), Ty::Class(b)) => self.types.is_descendant(b, a),
            (a, b) => a == b,
        };
        if !fits {
            let pn = self.tn(Ty::Helper(pid));
            let pt = self.tn(ptarget);
            let t = self.tn(target);
            self.error(
                Code::E(753),
                format!("helper `{pn}` is for `{pt}`, not `{t}`"),
                range,
            );
            return None;
        }
        Some(pid)
    }

    /// After the methods and properties of a helper are defined (`docs/spec/objects.md` 10.3,
    /// 10.6, 10.9): E0753 for a member with the name of a member of the type, E0701 for one with
    /// the name of a member of the parent helper, and E0753 for a `GetEnumerator` `for..in`
    /// never calls.
    pub(super) fn check_helper_members(&mut self, id: TypeId) {
        let Some(target) = self.types.helper_target(id) else {
            return;
        };
        let parent = self.types.parent(id);
        let own: Vec<(String, TextRange)> = {
            let mut v: Vec<(String, TextRange)> = self
                .types
                .class_methods(id)
                .iter()
                .map(|m| (m.name.clone(), m.span))
                .collect();
            v.extend(
                self.types
                    .class_properties(id)
                    .iter()
                    .map(|p| (p.name.clone(), p.span)),
            );
            v.extend(
                self.types
                    .class_statics(id)
                    .iter()
                    .map(|s| (s.name.clone(), s.span)),
            );
            v.sort_by_key(|(_, r)| r.start());
            v
        };
        let mut reported: Vec<String> = Vec::new();
        let tname = self.tn(target);
        for (name, span) in own {
            let lower = name.to_ascii_lowercase();
            if reported.contains(&lower) {
                continue;
            }
            // A method reported here does not also need a body (E0723)
            let first_diag = self.sink.len();
            self.check_helper_member(parent, target, &tname, &name, span, &mut reported);
            if self.sink.len() > first_diag
                && let Some(r) = self
                    .types
                    .class_methods(id)
                    .iter()
                    .find(|m| m.span == span)
                    .and_then(|m| m.routine)
            {
                self.routines[r.0 as usize].implicit_forward = true;
            }
        }
    }

    /// One member of [`Sema::check_helper_members`].
    #[allow(clippy::too_many_arguments)]
    fn check_helper_member(
        &mut self,
        parent: Option<TypeId>,
        target: Ty,
        tname: &str,
        name: &str,
        span: TextRange,
        reported: &mut Vec<String>,
    ) {
        let lower = name.to_ascii_lowercase();
        if let Some(p) = parent
            && let Some(decl) = self.helper_member_decl(p, name)
        {
            reported.push(lower.clone());
            let pn = self.tn(Ty::Helper(decl.0));
            let mut d = Diagnostic::error(
                Code::E(701),
                format!("`{name}` is already declared in helper `{pn}`"),
                Stage::Sema,
            )
            .with_primary(self.span(span), "declared again here");
            if let Some(s) = decl.1 {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            return;
        }
        if let Some(found) = self.type_member_decl(target, name) {
            reported.push(lower.clone());
            let mut d = Diagnostic::error(
                Code::E(753),
                format!("helper member `{name}` has the same name as a member of `{tname}`"),
                Stage::Sema,
            )
            .with_primary(self.span(span), "");
            match found {
                Some(s) => d = d.with_secondary(s, "declared here"),
                None => d
                    .notes
                    .push(format!("`{name}` is a built-in member of `TObject`")),
            }
            d.notes.push("give the helper member another name".into());
            if matches!(target, Ty::Class(_)) {
                d.notes.push(
                    "or derive a class from it and declare the member in the derived class".into(),
                );
            }
            self.push(d);
            return;
        }
        if lower == "getenumerator"
            && matches!(
                target,
                Ty::Array(_) | Ty::DynArray(_) | Ty::String | Ty::Set(_) | Ty::BigSet(_)
            )
        {
            reported.push(lower);
            let d = self.error(
                Code::E(753),
                format!("`GetEnumerator` in a helper for `{tname}` is never used by `for..in`"),
                span,
            );
            d.notes.push(
                "`for..in` iterates over arrays, strings and sets by itself, without `GetEnumerator`"
                    .into(),
            );
        }
    }

    /// The declaration of the member `name` of a helper or its parents: the helper declaring it,
    /// and the position (`None` for a built-in helper's member).
    fn helper_member_decl(&self, id: TypeId, name: &str) -> Option<(TypeId, Option<Span>)> {
        for h in self.types.class_chain(id) {
            if let Some(b) = self.types.helper_builtin(h) {
                if b.name().eq_ignore_ascii_case(name) {
                    return Some((h, None));
                }
                continue;
            }
            let file = self.unit_file(self.types.class_unit(h)?);
            if let Some(span) = self.declared_member_span(h, name) {
                return Some((h, Some(Span::new(file, span))));
            }
        }
        None
    }

    /// The position of a member a class, record, interface or helper declares itself.
    fn declared_member_span(&self, id: TypeId, name: &str) -> Option<TextRange> {
        let eq = |n: &str| n.eq_ignore_ascii_case(name);
        let t = &self.types;
        let field = match &t.get(id).kind {
            TypeKind::Class { fields, .. } => fields.iter().find(|f| eq(&f.name)).map(|f| f.span),
            TypeKind::Record { fields, .. } => fields.iter().find(|f| eq(&f.name)).map(|f| f.span),
            _ => None,
        };
        field
            .or_else(|| {
                t.class_methods(id)
                    .iter()
                    .find(|m| eq(&m.name))
                    .map(|m| m.span)
            })
            .or_else(|| {
                t.class_properties(id)
                    .iter()
                    .find(|p| eq(&p.name))
                    .map(|p| p.span)
            })
            .or_else(|| {
                t.class_statics(id)
                    .iter()
                    .find(|s| eq(&s.name))
                    .map(|s| s.span)
            })
    }

    /// Whether the type a helper is for has a member named `name`, whatever its visibility
    /// (`docs/spec/objects.md` 10.3): `Some(Some(position))`, or `Some(None)` for a built-in
    /// member of `TObject` (chapter 5).
    fn type_member_decl(&self, target: Ty, name: &str) -> Option<Option<Span>> {
        let id = match target {
            Ty::Class(id) | Ty::Record(id) => id,
            _ => return None,
        };
        for c in self.types.class_chain(id) {
            if let Some(span) = self.declared_member_span(c, name) {
                return Some(
                    self.types
                        .class_unit(c)
                        .map(|u| Span::new(self.unit_file(u), span)),
                );
            }
        }
        let builtin = matches!(target, Ty::Class(_))
            && (super::methods::is_class_builtin(name)
                || self
                    .types
                    .class_methods(super::types::TOBJECT)
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(name)));
        builtin.then_some(None)
    }

    /// The helpers visible where the code being analyzed is (`docs/spec/objects.md` 10.4): those
    /// declared earlier in this file, those declared in the `interface` section of a unit in
    /// `uses`, and the built-in helpers of `System` (10.7; listed first).
    pub(super) fn visible_helpers(&self, at: TextRange) -> Vec<TypeId> {
        let mut out: Vec<TypeId> = Vec::new();
        for (i, d) in self.types.defs.iter().enumerate() {
            if let TypeKind::Helper {
                builtin: Some(_), ..
            } = d.kind
                && !d
                    .name
                    .as_ref()
                    .is_some_and(|n| self.declares_name(&n.to_ascii_lowercase()))
            {
                out.push(TypeId(i as u32));
            }
        }
        for (_, sym) in &self.global_scope {
            if let GSym::Type {
                ty: Ty::Helper(h),
                span,
            } = sym
                && span.start() < at.start()
                && !out.contains(h)
            {
                out.push(*h);
            }
        }
        if let Some(c) = &self.cur {
            for scope in &c.scopes {
                for (_, sym) in scope {
                    if let super::LSym::Type {
                        ty: Ty::Helper(h), ..
                    } = sym
                        && !out.contains(h)
                    {
                        out.push(*h);
                    }
                }
            }
        }
        for u in &self.used_units {
            let Some(i) = u.iface else { continue };
            if self.in_interface_part && !u.in_interface {
                continue;
            }
            for (_, sym) in &self.units[i].exports {
                if let GSym::Type {
                    ty: Ty::Helper(h), ..
                } = sym
                    && !out.contains(h)
                {
                    out.push(*h);
                }
            }
        }
        out
    }

    /// Whether a declaration of the program or of a used unit has the name `lower` where the code
    /// being analyzed is: it hides the built-in helper of that name, members included
    /// (`docs/spec/objects.md` 10.7).
    fn declares_name(&self, lower: &str) -> bool {
        self.lookup_scopes(lower).is_some()
            || self.global_scope.iter().any(|(n, _)| n == lower)
            || self.used_units.iter().any(|u| {
                u.iface.is_some_and(|i| {
                    !(self.in_interface_part && !u.in_interface)
                        && self.units[i].exports.iter().any(|(n, _)| n == lower)
                })
            })
    }

    /// The helpers of one step of `docs/spec/objects.md` 10.4 that have a member named `name`:
    /// the visible helpers for exactly the type `ty`, and the unnamed built-in helper of `ty`.
    fn helpers_with(&self, ty: Ty, name: &str, visible: &[TypeId]) -> Vec<(TypeId, HelperMember)> {
        let mut out: Vec<(TypeId, HelperMember)> = Vec::new();
        let mut named_builtin = false;
        for &h in visible {
            if self.types.helper_target(h) != Some(ty) {
                continue;
            }
            let Some((owner, _)) = self.helper_member_decl(h, name) else {
                continue;
            };
            // A parent and its child declare the same member (10.6)
            if out.iter().any(|(o, _)| {
                self.types.class_chain(*o).contains(&owner)
                    || self.types.class_chain(owner).contains(o)
            }) {
                continue;
            }
            match self.types.helper_builtin(owner) {
                Some(b) => {
                    named_builtin = true;
                    out.push((h, HelperMember::Builtin(b)));
                }
                None => out.push((h, HelperMember::Declared)),
            }
        }
        if !named_builtin
            && let Some(b) = builtin_member(ty)
            && b.name().eq_ignore_ascii_case(name)
            && !self.has_named_builtin_helper(ty)
        {
            out.push((super::types::TOBJECT, HelperMember::Builtin(b)));
        }
        out
    }

    /// Whether the built-in helper of `ty` has a name (then it is among the visible helpers).
    fn has_named_builtin_helper(&self, ty: Ty) -> bool {
        BUILTIN_HELPERS
            .iter()
            .any(|(n, _)| n.is_some_and(|n| builtin_target(n) == ty))
    }

    /// The member `name` of a value of type `ty` found through the helpers
    /// (`docs/spec/objects.md` 10.4, steps 2 and 3): the helper (`TOBJECT` for an unnamed
    /// built-in helper) and the member. `Err(true)` after E0754 (ambiguous), `Err(false)` when no
    /// helper has it. `recv` is how the code reaches the member, for the note and fixes of E0754.
    pub(super) fn find_helper_member(
        &mut self,
        ty: Ty,
        name: &str,
        range: TextRange,
        recv: HelperRecv,
    ) -> Result<(TypeId, HelperMember), bool> {
        let visible = self.visible_helpers(range);
        let mut steps: Vec<Ty> = vec![ty];
        match ty {
            Ty::Class(c) => {
                steps.extend(self.types.class_chain(c).into_iter().skip(1).map(Ty::Class));
            }
            Ty::Subrange(_) => steps.push(self.types.base(ty)),
            // The original type, its original type, …, the root (`docs/spec/types.md` 1.3)
            Ty::Distinct(_) => {
                let mut t = ty;
                while let Some(o) = self.types.origin(t) {
                    steps.push(o);
                    t = o;
                }
            }
            _ => {}
        }
        for step in steps {
            let found = self.helpers_with(step, name, &visible);
            match found.len() {
                0 => continue,
                1 => {
                    let (h, m) = found.into_iter().next().expect("one helper");
                    return Ok((h, m));
                }
                _ => {
                    self.ambiguous_helpers(name, &found, range, recv);
                    return Err(true);
                }
            }
        }
        Err(false)
    }

    /// E0754 (`docs/spec/objects.md` 10.4), with a fix per candidate that calls it as
    /// `H.Name(X, …)` (10.5) when the receiver `X` is written, no candidate is a property,
    /// and the candidate's helper has a name.
    fn ambiguous_helpers(
        &mut self,
        name: &str,
        found: &[(TypeId, HelperMember)],
        range: TextRange,
        recv: HelperRecv,
    ) {
        let names: Vec<String> = found
            .iter()
            .map(|(h, m)| match m {
                HelperMember::Builtin(_) if *h == super::types::TOBJECT => "System".to_string(),
                _ => self.tn(Ty::Helper(*h)),
            })
            .collect();
        let list = match names.as_slice() {
            [a, b] => format!("`{a}` and `{b}`"),
            _ => {
                let (last, rest) = names.split_last().expect("two or more helpers");
                let rest: Vec<String> = rest.iter().map(|n| format!("`{n}`")).collect();
                format!("{} and `{last}`", rest.join(", "))
            }
        };
        let mut d = Diagnostic::error(
            Code::E(754),
            format!("`{name}` is ambiguous: it is declared in helpers {list}"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        let mut property = false;
        for (h, m) in found {
            match m {
                HelperMember::Builtin(b) => {
                    property |= *b == BuiltinHelper::Length;
                }
                HelperMember::Declared => {
                    if let Some((owner, Some(s))) = self.helper_member_decl(*h, name) {
                        d = d.with_secondary(s, "candidate");
                        property |= self
                            .types
                            .class_properties(owner)
                            .iter()
                            .any(|p| p.name.eq_ignore_ascii_case(name));
                    }
                }
            }
        }
        let named: Vec<&String> = found
            .iter()
            .zip(&names)
            .filter(
                |((h, m), _)| !matches!(m, HelperMember::Builtin(_) if *h == super::types::TOBJECT),
            )
            .map(|(_, n)| n)
            .collect();
        if recv == HelperRecv::ForIn {
            // `for..in` calls the member itself, so it cannot be written as `H.Name(X)`
            d.notes.push(
                "`for..in` cannot select a helper; leave only one of the helpers visible here"
                    .into(),
            );
        } else if property {
            d.notes.push(
                "rename the property in one of the helpers; a property cannot be selected with `Helper.Name(X)`"
                    .into(),
            );
        } else {
            let first = named.first().map_or("THelper", |n| n.as_str());
            d.notes.push(format!(
                "name the helper to call, as in `{first}.{name}(X)`"
            ));
            if let HelperRecv::Written(recv) = recv {
                for h in &named {
                    let edits = self.explicit_call_edits(h, name, recv, range);
                    d = d.with_fix(Fix::suggestion(format!("call `{h}.{name}`"), edits));
                }
            }
        }
        self.push(d);
    }

    /// The edits that turn `X.Name`, `X.Name()` or `X.Name(A, …)` into `H.Name(X)` /
    /// `H.Name(X, A, …)` (`docs/spec/objects.md` 10.5).
    fn explicit_call_edits(
        &self,
        h: &str,
        name: &str,
        recv: TextRange,
        range: TextRange,
    ) -> Vec<TextEdit> {
        let file = self.input.file;
        let x = self.src(recv);
        let bytes = self.input.bytes;
        let skip = |mut i: usize| {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            i
        };
        let end = usize::from(range.end());
        let open = skip(end);
        let head = TextRange::new(recv.start(), range.end());
        if bytes.get(open) != Some(&b'(') {
            return vec![TextEdit::replace(file, head, format!("{h}.{name}({x})"))];
        }
        let paren = TextRange::new(
            crate::source::TextSize::from(open as u32),
            crate::source::TextSize::from(open as u32 + 1),
        );
        let empty = bytes.get(skip(open + 1)) == Some(&b')');
        vec![
            TextEdit::replace(file, head, format!("{h}.{name}({x}")),
            TextEdit::replace(file, paren, if empty { "" } else { ", " }),
        ]
    }

    /// `X.Name` found through the helpers of the type of `recv` (`docs/spec/objects.md` 10.4):
    /// `None` when no helper has it (the caller reports the missing member).
    pub(super) fn helper_resolved(
        &mut self,
        recv: Expr,
        name: &str,
        range: TextRange,
    ) -> Option<Resolved> {
        let ty = recv.ty;
        self.helper_resolved_as(recv, ty, name, range)
    }

    /// [`Self::helper_resolved`] looking up the helpers of `declared`: the subrange type of a
    /// value whose expression has the base type (10.4, step 3).
    fn helper_resolved_as(
        &mut self,
        recv: Expr,
        declared: Ty,
        name: &str,
        range: TextRange,
    ) -> Option<Resolved> {
        // A class reference finds the class's helpers (their class members; 10.4)
        let ty = match declared {
            t @ Ty::ClassRef(_) => Ty::Class(self.types.class_ref_target(t)?),
            t => self.types.strong(t),
        };
        if ty.is_error() {
            return None;
        }
        let written = if matches!(recv.kind, ExprKind::Error) {
            HelperRecv::Implicit
        } else {
            HelperRecv::Written(recv.span)
        };
        match self.find_helper_member(ty, name, range, written) {
            Ok((h, HelperMember::Declared)) => Some(match self.find_member(h, name, range) {
                Ok(Member::Static(owner, st)) => self.static_resolved(owner, &st),
                Ok(member) => Resolved::Member {
                    recv: Box::new(recv),
                    class: h,
                    member,
                    inherited: false,
                },
                Err(_) => Resolved::Error,
            }),
            Ok((h, HelperMember::Builtin(b))) => Some(Resolved::Member {
                recv: Box::new(recv),
                class: h,
                member: Member::HelperBuiltin(b),
                inherited: false,
            }),
            Err(true) => Some(Resolved::Error),
            Err(false) => None,
        }
    }

    /// `X.Name` on a value that is neither an instance nor a class reference, when the type has no
    /// such member (`docs/spec/objects.md` 10.4): the helpers' member, or E0700 for a record (the
    /// record has no such member), or the E3200 of the built-in types' helpers (M5).
    /// `declared` is the subrange type of the value when `recv` has its base type (a subrange
    /// type's own helpers come first; 10.4, step 3), otherwise `recv`'s type.
    pub(super) fn value_member(
        &mut self,
        recv: Expr,
        declared: Ty,
        name: &str,
        range: TextRange,
        whole: TextRange,
    ) -> Resolved {
        let ty = recv.ty;
        if let Ty::Param(_) = ty {
            // A member of a constraint (`docs/spec/generics.md` 3.3, 4.3)
            let Some(x) = self.param_member_view(recv, name, whole) else {
                return Resolved::Error;
            };
            let class =
                x.ty.type_id()
                    .expect("a constraint is a class or interface");
            return match self.find_member(class, name, range) {
                Ok(member) => Resolved::Member {
                    recv: Box::new(x),
                    class,
                    member,
                    inherited: false,
                },
                Err(true) => Resolved::Error,
                Err(false) => {
                    self.member_missing(class, name, range);
                    Resolved::Error
                }
            };
        }
        if let Some(r) = self.helper_resolved_as(recv, declared, name, range) {
            return r;
        }
        match ty {
            Ty::Error => {}
            Ty::Record(_) => {
                let t = self.tn(ty);
                self.error(Code::E(700), format!("`{t}` has no field `{name}`"), range);
            }
            Ty::Class(_) => {
                let t = self.tn(ty);
                self.error(Code::E(700), format!("`{t}` has no member `{name}`"), range);
            }
            _ => {
                self.error(
                    Code::E(3200),
                    "type helpers for built-in types are not supported yet (planned for M5)",
                    whole,
                );
            }
        }
        Resolved::Error
    }

    /// `X.Name` written with the name of a helper `H` (`docs/spec/objects.md` 10.5): a class
    /// member, or an instance method called with its receiver as the first argument (the receiver
    /// is then `H` itself as a marker, which `method_call` replaces with that argument).
    pub(super) fn helper_type_member(
        &mut self,
        h: TypeId,
        name: &str,
        base_range: TextRange,
        range: TextRange,
    ) -> Resolved {
        let marker = Self::helper_marker(h, base_range);
        if let Some(b) = self.types.helper_builtin(h) {
            if b == BuiltinHelper::ToString && b.name().eq_ignore_ascii_case(name) {
                return Resolved::Member {
                    recv: Box::new(marker),
                    class: h,
                    member: Member::HelperBuiltin(b),
                    inherited: false,
                };
            }
            if b.name().eq_ignore_ascii_case(name) {
                self.instance_member_error(b.name(), None, range);
                return Resolved::Error;
            }
            let t = self.tn(Ty::Helper(h));
            self.error(Code::E(700), format!("`{t}` has no member `{name}`"), range);
            return Resolved::Error;
        }
        match self.find_member(h, name, range) {
            Ok(Member::Static(owner, st)) => self.static_resolved(owner, &st),
            Ok(Member::Property(owner, p)) if !p.is_class => {
                // A property cannot be selected (10.5)
                let decl = self
                    .types
                    .class_unit(owner)
                    .map(|u| Span::new(self.unit_file(u), p.span));
                self.instance_member_error(&p.name, decl, range);
                Resolved::Error
            }
            // `method_call` decides the receiver of `H.M(…)` from the candidates
            Ok(member) => Resolved::Member {
                recv: Box::new(marker),
                class: h,
                member,
                inherited: false,
            },
            Err(true) => Resolved::Error,
            Err(false) => {
                let t = self.tn(Ty::Helper(h));
                self.error(Code::E(700), format!("`{t}` has no member `{name}`"), range);
                Resolved::Error
            }
        }
    }

    /// Whether `H.M(…)` calls an instance method with its receiver first (`true`, with the number
    /// of the method's own parameters) or a class method (`false`; `docs/spec/objects.md` 10.5).
    /// When `M` has both, the forms `(X, …)` and `(…)` both go through the choice of overloads
    /// (`semantics.md` 4.7): the one that fits is taken, E0734 if both do (`None`), and the
    /// first declared candidate's form when neither does (its call reports why).
    pub(super) fn explicit_kind(
        &mut self,
        h: TypeId,
        class: TypeId,
        name: &str,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<(bool, usize)> {
        let set = self.method_set(class, name);
        let methods: Vec<(super::hir::RoutineId, super::types::ClassMethod)> = set
            .iter()
            .filter_map(|r| self.method_of_routine(*r).map(|(_, m)| (*r, m)))
            .collect();
        let first = self.methods_named(class, name).first().cloned();
        let first_instance = first
            .as_ref()
            .is_some_and(|(_, m)| m.self_kind == SelfKind::Instance);
        let expected = first.as_ref().map_or(0, |(_, m)| m.params.len());
        let inst: Vec<super::hir::RoutineId> = methods
            .iter()
            .filter(|(_, m)| m.self_kind == SelfKind::Instance)
            .map(|(r, _)| *r)
            .collect();
        let cls: Vec<super::hir::RoutineId> = methods
            .iter()
            .filter(|(_, m)| m.self_kind != SelfKind::Instance)
            .map(|(r, _)| *r)
            .collect();
        if inst.is_empty() || cls.is_empty() {
            return Some((first_instance, expected));
        }
        let receiver_fits = self.dry_run(|s| s.explicit_receiver(h, args, 0, span).is_some());
        let inst_fit = if receiver_fits {
            self.viable_candidates(&inst, &args[1..])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let cls_fit = self.viable_candidates(&cls, args).unwrap_or_default();
        match (inst_fit.is_empty(), cls_fit.is_empty()) {
            (false, true) => Some((true, expected)),
            (true, false) => Some((false, 0)),
            (true, true) | (false, false) => {
                // Neither form fits (E0735, every candidate), or both do (E0734)
                let none = inst_fit.is_empty();
                // The arguments' own errors come first, and then nothing more is said
                let before = self.sink.len();
                for a in args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                if self.sink.iter().skip(before).any(|d| d.is_error()) {
                    return None;
                }
                let n = self.tn(Ty::Helper(h));
                let (code, msg, cands) = if none {
                    (
                        735,
                        format!("no overload of `{n}.{name}` matches these arguments"),
                        set,
                    )
                } else {
                    (
                        734,
                        format!("ambiguous call to overloaded `{n}.{name}`"),
                        inst_fit.into_iter().chain(cls_fit).collect(),
                    )
                };
                let mut d = Diagnostic::error(Code::E(code), msg, Stage::Sema)
                    .with_primary(self.span(span), "");
                for c in cands {
                    d = self.with_routine_decl(d, c, "candidate");
                }
                self.push(d);
                None
            }
        }
    }

    /// The receiver standing for a helper's name in `H.M(X, …)` (`docs/spec/objects.md` 10.5).
    fn helper_marker(h: TypeId, span: TextRange) -> Expr {
        Expr {
            kind: ExprKind::Error,
            ty: Ty::Helper(h),
            value: None,
            span,
        }
    }

    /// `H.M(X, …)` (`docs/spec/objects.md` 10.5): the receiver `X`, lowered and checked against
    /// the type `H` is for, and the arguments after it. `None` after a diagnostic (the arguments
    /// are lowered).
    pub(super) fn explicit_receiver<'x>(
        &mut self,
        h: TypeId,
        args: &'x [ast::Arg],
        expected: usize,
        span: TextRange,
    ) -> Option<(Expr, &'x [ast::Arg])> {
        let Some((first, rest)) = args.split_first() else {
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {}, found 0",
                    expected + 1
                ),
                span,
            );
            return None;
        };
        let Some(v) = first.value() else {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            return None;
        };
        let x = self.lower_expr(&v);
        let target = self.types.helper_target(h).unwrap_or(Ty::Error);
        let ok = match (target, x.ty) {
            (_, Ty::Error) | (Ty::Error, _) => true,
            (Ty::Class(t), Ty::Class(c)) => self.types.is_descendant(c, t),
            (Ty::Subrange(_), t) => t == target || t == self.types.base(target),
            // A distinct type for a helper of a type it was made from (`docs/spec/types.md` 1.3)
            (t, u @ Ty::Distinct(_)) => {
                let mut cur = Some(u);
                let mut found = false;
                while let Some(c) = cur {
                    found |= c == t;
                    cur = self.types.origin(c);
                }
                found
            }
            (t, u) => t == u,
        };
        if !ok {
            for a in rest {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            let (t, u) = (self.tn(target), self.tn(x.ty));
            self.error(
                Code::E(702),
                format!("type mismatch: expected `{t}`, found `{u}`"),
                x.span,
            );
            return None;
        }
        Some((x, rest))
    }

    /// A member of a built-in helper (`docs/spec/objects.md` 10.7, 14.10): `ToString` as the
    /// text `Write` gives (`FloatToStr` for a `Currency`, `'True'` / `'False'` for a `Boolean`),
    /// `Length` as `System.Length`. Values are not folded.
    pub(super) fn helper_builtin_expr(
        &mut self,
        recv: Expr,
        b: BuiltinHelper,
        args: Option<&[ast::Arg]>,
        value: bool,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let lower_args = |s: &mut Self, args: &[ast::Arg]| {
            for a in args {
                if let Some(v) = a.value() {
                    s.lower_expr(&v);
                }
            }
        };
        let (recv, args) = match recv.ty {
            Ty::Helper(h) => match self.explicit_receiver(h, args.unwrap_or(&[]), 0, span) {
                Some((x, rest)) => (x, Some(rest)),
                None => return err_expr(span),
            },
            _ => (recv, args),
        };
        // A distinct type as its root (`docs/spec/types.md` 1.3)
        let recv = self.rooted(recv);
        match b {
            BuiltinHelper::ToString => {
                if let Some(a) = args
                    && !a.is_empty()
                {
                    lower_args(self, a);
                    self.error(
                        Code::E(705),
                        format!("wrong number of arguments: expected 0, found {}", a.len()),
                        span,
                    );
                    return err_expr(span);
                }
                let x = match recv.ty {
                    Ty::Currency => self.convert_num(recv, Ty::Double),
                    Ty::Subrange(_) => Expr {
                        ty: self.types.base(recv.ty),
                        ..recv
                    },
                    _ => recv,
                };
                Expr {
                    kind: ExprKind::Builtin {
                        func: super::hir::Builtin::ToString,
                        args: vec![x],
                    },
                    ty: Ty::String,
                    value: None,
                    span,
                }
            }
            BuiltinHelper::Length => {
                if args.is_some() || !value {
                    lower_args(self, args.unwrap_or(&[]));
                    let whole = TextRange::new(recv.span.start(), range.end());
                    let text = self.src(whole);
                    self.error(
                        Code::E(704),
                        format!("`{text}` is not a procedure or function"),
                        whole,
                    );
                    return err_expr(span);
                }
                Expr {
                    kind: ExprKind::Builtin {
                        func: super::hir::Builtin::Length,
                        args: vec![recv],
                    },
                    ty: Ty::Integer,
                    value: None,
                    span,
                }
            }
        }
    }
}
