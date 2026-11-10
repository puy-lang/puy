//! `type` sections, records and static arrays (`docs/spec/types.md` chapters 1, 2 and 4): resolving
//! type declarations (with forward references inside a section), anonymous types, cycle and bound
//! checks, and the constant shapes `(1, 2)` / `(X: 1; Y: 2)`.

use super::hir::*;
use super::types::{
    Const, DynWritten, EnumValueDef, Field, MAX_ARRAY_LEN, Ty, TypeDef, TypeId, TypeKind,
    bound_text,
};
use super::{GSym, LSym, Sema, builtins};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::TextRange;
use crate::syntax::ast::{self, AstNode};

/// An alias (`type X = Y`) of the section being declared, resolved on first use so that it may
/// name a type declared later in the same section.
pub(super) struct PendingAlias {
    lower: String,
    /// The type written after `=`: a name, or a dynamic array (`array of T`, `TArray<T>`), whose
    /// type is found by its element type.
    target: ast::TypeSpec,
    /// The declared name as written.
    name: String,
    span: TextRange,
    state: AliasState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AliasState {
    Unresolved,
    Resolving,
    Done(Ty),
}

/// One declaration of a `type` section between the two passes.
enum Pending {
    Record(TypeId, ast::RecordType),
    Array(TypeId, ast::ArrayType),
    /// `EFoo = class(Parent)` (`docs/spec/exceptions.md` chapter 3).
    Class(TypeId, ast::ClassType),
    /// `TDigit = 0..9`: defined in the second pass, after the enumerations of the section.
    Subrange(TypeId, ast::SubrangeType),
    Alias(usize),
    /// `type X = array of T` / `TArray<T>`: resolved like an alias (it names the dynamic array of
    /// its element type).
    DynArray(usize),
    Nothing,
}

impl<'a, 's> Sema<'a, 's> {
    /// The type name in diagnostics (`docs/spec/types.md` 1.2).
    pub(super) fn tn(&self, ty: Ty) -> String {
        self.types.name(ty)
    }

    /// A type written in a declaration: a name, or an anonymous `record` / `array` type (a new type
    /// for each position; `docs/spec/types.md` 1.2).
    pub(super) fn resolve_type(&mut self, t: Option<ast::TypeSpec>) -> Ty {
        match t {
            None => Ty::Error,
            Some(ast::TypeSpec::Ref(r)) => self.resolve_type_ref(&r),
            Some(ast::TypeSpec::Record(r)) => {
                let span = r
                    .record_token()
                    .map(|t| t.text_range())
                    .unwrap_or(r.syntax().text_range());
                let id = self.add_type(None, span);
                self.define_record(id, &r);
                Ty::Record(id)
            }
            Some(ast::TypeSpec::Array(a)) => {
                let span = a
                    .array_token()
                    .map(|t| t.text_range())
                    .unwrap_or(a.syntax().text_range());
                if !a.has_brackets() {
                    let elem = self.resolve_type(a.element());
                    return self.dyn_array_of(
                        elem,
                        DynWritten::Anonymous {
                            generic: false,
                            span,
                        },
                    );
                }
                let id = self.add_type(None, span);
                self.define_array(id, &a);
                Ty::Array(id)
            }
            Some(ast::TypeSpec::Class(_)) => Ty::Error,
            Some(ast::TypeSpec::Enum(e)) => {
                // An enumeration needs a name to qualify its values (`docs/spec/types.md` 8.1)
                let range = ast::trimmed_range(e.syntax());
                self.error(
                    Code::E(736),
                    "an enumeration type must be declared with a name",
                    range,
                )
                .notes
                .push("declare the enumeration as a named type, as in `type TColor = (Red, Green, Blue)`".into());
                Ty::Error
            }
            Some(ast::TypeSpec::Subrange(r)) => {
                let id = self.add_type(None, ast::trimmed_range(r.syntax()));
                match self.subrange_kind(&r) {
                    Some(kind) => {
                        self.types.get_mut(id).kind = kind;
                        Ty::Subrange(id)
                    }
                    None => Ty::Error,
                }
            }
            Some(ast::TypeSpec::Set(st)) => {
                let span = st
                    .set_token()
                    .map(|t| t.text_range())
                    .unwrap_or(st.syntax().text_range());
                let base = self.resolve_type(st.base());
                self.set_of(
                    base,
                    &st,
                    DynWritten::Anonymous {
                        generic: false,
                        span,
                    },
                )
            }
            Some(ast::TypeSpec::Generic(g)) => {
                let span = g.syntax().text_range();
                match self.generic_elem(&g) {
                    Some(elem) => self.dyn_array_of(
                        elem,
                        DynWritten::Anonymous {
                            generic: true,
                            span,
                        },
                    ),
                    None => Ty::Error,
                }
            }
        }
    }

    /// The set of a base type (`docs/spec/types.md` 8.3): an ordinal type whose ordinals are all
    /// within 0..65535 (E0737 otherwise).
    fn set_of(&mut self, base: Ty, st: &ast::SetType, written: DynWritten) -> Ty {
        if base.is_error() {
            return Ty::Error;
        }
        let ok = base.is_ordinal()
            && self
                .types
                .ord_range(base)
                .is_some_and(|(lo, hi)| lo >= 0 && hi <= 65535);
        if !ok {
            let range = st
                .base()
                .map_or(st.syntax().text_range(), |b| ast::trimmed_range(b.syntax()));
            let t = self.tn(base);
            self.error(
                Code::E(737),
                format!("set base type `{t}` must have ordinal values in 0..65535"),
                range,
            )
            .notes
            .push("use a subrange type such as `0..255` as the base type".into());
            return Ty::Error;
        }
        self.types.set_of(base, written)
    }

    /// The dynamic array of an element type (the error type stays the error type).
    fn dyn_array_of(&mut self, elem: Ty, written: DynWritten) -> Ty {
        if elem.is_error() {
            return Ty::Error;
        }
        self.types.dyn_array(elem, written)
    }

    /// The element type of `TArray<T>`. A `TArray` declared by the program hides the built-in one,
    /// so `<` cannot follow it (E0400; `docs/spec/types.md` chapter 1).
    fn generic_elem(&mut self, g: &ast::GenericTypeRef) -> Option<Ty> {
        if let Some(Some(_) | None) = self.lookup_type_symbol("tarray") {
            let lt = g
                .lt_token()
                .map_or(g.syntax().text_range(), |t| t.text_range());
            self.error(Code::E(400), "expected `;`, found `<`", lt);
            return None;
        }
        let elem = self.resolve_type(g.arg());
        (!elem.is_error()).then_some(elem)
    }

    /// An open array parameter `array of T` (`docs/spec/types.md` 2.4).
    pub(super) fn open_array_of(&mut self, a: &ast::ArrayType) -> Ty {
        let span = a
            .array_token()
            .map(|t| t.text_range())
            .unwrap_or(a.syntax().text_range());
        let elem = self.resolve_type(a.element());
        match self.dyn_array_of(
            elem,
            DynWritten::Anonymous {
                generic: false,
                span,
            },
        ) {
            Ty::DynArray(id) => Ty::OpenArray(id),
            t => t,
        }
    }

    fn add_type(&mut self, name: Option<String>, span: TextRange) -> TypeId {
        let full_name = name.as_ref().map(|n| self.qualified_type_name(n));
        self.types.add(TypeDef {
            name,
            full_name,
            kind: TypeKind::Pending,
            span,
        })
    }

    /// `Hello.TPoint` for a type of the program, `Hello.Routine.TPoint` for a local type.
    fn qualified_type_name(&self, name: &str) -> String {
        match &self.cur {
            Some(c) if !c.is_main => format!("{}.{}.{name}", self.program_name, c.qual_name),
            _ => format!("{}.{name}", self.program_name),
        }
    }

    /// Looks a name up as a type: `Some(Some(ty))` for a declared type, `Some(None)` for a name
    /// declared as something else, `None` if no declaration has the name.
    pub(super) fn lookup_type_symbol(&mut self, lower: &str) -> Option<Option<Ty>> {
        let mut found: Option<Option<Ty>> = self.lookup_scopes(lower).map(|(_, sym)| match sym {
            LSym::Type { ty, .. } => Some(ty),
            _ => None,
        });
        if found.is_none() {
            found = self
                .global_scope
                .iter()
                .rev()
                .find(|(n, _)| n == lower)
                .map(|(_, sym)| match sym {
                    GSym::Type { ty, .. } => Some(*ty),
                    _ => None,
                });
        }
        // An alias of the section being declared resolves on first use
        if let Some(Some(_)) = found
            && let Some(i) = self.pending_aliases.iter().position(|a| a.lower == lower)
        {
            return Some(Some(self.resolve_alias(i)));
        }
        found
    }

    fn resolve_alias(&mut self, i: usize) -> Ty {
        match self.pending_aliases[i].state {
            AliasState::Done(ty) => ty,
            AliasState::Resolving => {
                // `type A = B; B = A;`
                let span = self.pending_aliases[i].span;
                let name = self.src(span);
                self.error(Code::E(727), format!("type `{name}` contains itself"), span);
                self.pending_aliases[i].state = AliasState::Done(Ty::Error);
                Ty::Error
            }
            AliasState::Unresolved => {
                self.pending_aliases[i].state = AliasState::Resolving;
                let target = self.pending_aliases[i].target.clone();
                let ty = match &target {
                    ast::TypeSpec::Ref(r) => self.resolve_type_ref(r),
                    ast::TypeSpec::Array(a) => {
                        let elem = self.resolve_type(a.element());
                        self.named_dyn_array(i, elem)
                    }
                    ast::TypeSpec::Generic(g) => match self.generic_elem(g) {
                        Some(elem) => self.named_dyn_array(i, elem),
                        None => Ty::Error,
                    },
                    ast::TypeSpec::Set(st) => {
                        let base = self.resolve_type(st.base());
                        let name = self.pending_aliases[i].name.clone();
                        let full_name = self.qualified_type_name(&name);
                        let span = self.pending_aliases[i].span;
                        self.set_of(
                            base,
                            st,
                            DynWritten::Named {
                                name,
                                full_name,
                                span,
                            },
                        )
                    }
                    ast::TypeSpec::Record(_)
                    | ast::TypeSpec::Class(_)
                    | ast::TypeSpec::Enum(_)
                    | ast::TypeSpec::Subrange(_) => Ty::Error,
                };
                if let AliasState::Resolving = self.pending_aliases[i].state {
                    self.pending_aliases[i].state = AliasState::Done(ty);
                }
                match self.pending_aliases[i].state {
                    AliasState::Done(t) => t,
                    _ => ty,
                }
            }
        }
    }

    /// The dynamic array a `type` declaration names (`type TInts = array of Integer`).
    fn named_dyn_array(&mut self, i: usize, elem: Ty) -> Ty {
        let name = self.pending_aliases[i].name.clone();
        let full_name = self.qualified_type_name(&name);
        let span = self.pending_aliases[i].span;
        self.dyn_array_of(
            elem,
            DynWritten::Named {
                name,
                full_name,
                span,
            },
        )
    }

    /// A `type` section (`docs/spec/types.md` chapter 1). All names are declared first, so a type
    /// may refer to a type declared later in the same section.
    pub(super) fn declare_types(&mut self, section: &ast::TypeSection) {
        let mut items: Vec<(String, TextRange, Ty, Pending)> = Vec::new();
        for decl in section.decls() {
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            if decl.type_spec().is_none() {
                // A type the parser reported (a class, M3; `object`, removed): the name is known,
                // so uses of it do not pile on E0700, and a later full declaration of a
                // forward-declared class is not E0701
                if self.find_declared(&lower).is_none() {
                    self.bind_type(lower, Ty::Error, range);
                }
                continue;
            }
            if self.check_duplicate(&lower, range) {
                continue;
            }
            let (ty, pending) = match decl.type_spec() {
                Some(ast::TypeSpec::Record(r)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    (Ty::Record(id), Pending::Record(id, r))
                }
                Some(ast::TypeSpec::Array(a)) if a.has_brackets() => {
                    let id = self.add_type(Some(name.clone()), range);
                    (Ty::Array(id), Pending::Array(id, a))
                }
                Some(ast::TypeSpec::Class(c)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    (Ty::Exception(id), Pending::Class(id, c))
                }
                Some(ast::TypeSpec::Enum(e)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    self.define_enum(id, &e);
                    (Ty::Enum(id), Pending::Nothing)
                }
                Some(ast::TypeSpec::Subrange(r)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    (Ty::Subrange(id), Pending::Subrange(id, r))
                }
                Some(
                    t @ (ast::TypeSpec::Ref(_)
                    | ast::TypeSpec::Array(_)
                    | ast::TypeSpec::Generic(_)
                    | ast::TypeSpec::Set(_)),
                ) => {
                    // A name, or a dynamic array (whose type is found by its element type)
                    let defines = !matches!(t, ast::TypeSpec::Ref(_));
                    self.pending_aliases.push(PendingAlias {
                        lower: lower.clone(),
                        target: t,
                        name: name.clone(),
                        span: range,
                        state: AliasState::Unresolved,
                    });
                    let i = self.pending_aliases.len() - 1;
                    (
                        Ty::Error,
                        if defines {
                            Pending::DynArray(i)
                        } else {
                            Pending::Alias(i)
                        },
                    )
                }
                None => (Ty::Error, Pending::Nothing),
            };
            self.bind_type(lower, ty, range);
            items.push((name, range, ty, pending));
        }
        let mut defined = Vec::new();
        for (name, _range, ty, pending) in items {
            let is_dyn = matches!(pending, Pending::DynArray(_));
            let generic = match pending {
                Pending::DynArray(i) => {
                    matches!(self.pending_aliases[i].target, ast::TypeSpec::Generic(_))
                }
                _ => false,
            };
            let (ty, defines) = match pending {
                Pending::Record(id, r) => {
                    self.define_record(id, &r);
                    defined.push(id);
                    (ty, true)
                }
                Pending::Array(id, a) => {
                    self.define_array(id, &a);
                    defined.push(id);
                    (ty, true)
                }
                Pending::Class(id, c) => {
                    self.define_class(id, &c);
                    (ty, true)
                }
                Pending::Subrange(id, r) => match self.subrange_kind(&r) {
                    Some(kind) => {
                        self.types.get_mut(id).kind = kind;
                        (ty, true)
                    }
                    None => {
                        self.rebind_type(&name.to_ascii_lowercase(), Ty::Error);
                        (Ty::Error, true)
                    }
                },
                Pending::Alias(i) | Pending::DynArray(i) => {
                    let t = self.resolve_alias(i);
                    let lower = self.pending_aliases[i].lower.clone();
                    self.rebind_type(&lower, t);
                    (t, is_dyn)
                }
                Pending::Nothing => (ty, matches!(ty, Ty::Enum(_))),
            };
            let routine = self
                .cur
                .as_ref()
                .filter(|c| !c.is_main)
                .map(|c| c.qual_name.clone());
            self.type_decls.push(TypeDeclInfo {
                unit: self.unit_index,
                routine,
                name,
                ty,
                defines,
                generic,
            });
        }
        self.pending_aliases.clear();
        self.check_cycles(&defined);
    }

    fn bind_type(&mut self, lower: String, ty: Ty, span: TextRange) {
        match &mut self.cur {
            Some(c) if !c.is_main => {
                c.scopes
                    .last_mut()
                    .expect("a routine has a scope")
                    .push((lower, LSym::Type { ty, span }));
            }
            _ => self.global_scope.push((lower, GSym::Type { ty, span })),
        }
    }

    /// Sets the type an alias of the section denotes once it is resolved.
    fn rebind_type(&mut self, lower: &str, ty: Ty) {
        if let Some(c) = &mut self.cur
            && let Some(scope) = c.scopes.last_mut()
            && let Some((_, LSym::Type { ty: t, .. })) =
                scope.iter_mut().rev().find(|(n, _)| n == lower)
        {
            *t = ty;
            return;
        }
        if let Some((_, GSym::Type { ty: t, .. })) =
            self.global_scope.iter_mut().rev().find(|(n, _)| n == lower)
        {
            *t = ty;
        }
    }

    /// The parent of an exception class (`docs/spec/exceptions.md` chapter 3): an exception class
    /// (E0733 otherwise), without a cycle (the E0727 variant). On an error the parent is `Exception`,
    /// so the class stays usable.
    fn define_class(&mut self, id: TypeId, c: &ast::ClassType) {
        let root = Some(super::types::EXCEPTION);
        let parent = match c.parent() {
            Some(p) => {
                let ty = self.resolve_type_ref(&p);
                let range = p.syntax().text_range();
                match ty {
                    Ty::Exception(pid) if self.types.is_descendant(pid, id) => {
                        let name = self.types.name(Ty::Exception(id));
                        let span = self.types.get(id).span;
                        let d = Diagnostic::error(
                            Code::E(727),
                            format!("type `{name}` inherits from itself"),
                            Stage::Sema,
                        )
                        .with_primary(self.span(span), "")
                        .with_secondary(self.span(range), "part of the cycle");
                        self.push(d);
                        root
                    }
                    Ty::Exception(pid) => Some(pid),
                    Ty::Error => root,
                    other => {
                        let t = self.tn(other);
                        self.error(
                            Code::E(733),
                            format!("`{t}` is not an exception class"),
                            range,
                        );
                        root
                    }
                }
            }
            None => root,
        };
        self.types.get_mut(id).kind = TypeKind::Exception { parent };
    }

    /// The values of an enumeration (`docs/spec/types.md` 8.1): ordinals from 0, or as written
    /// (`= Expr`, an integer constant in the range of `Integer`), each value one more than the
    /// previous one otherwise. Overlapping ordinals are allowed.
    fn define_enum(&mut self, id: TypeId, e: &ast::EnumType) {
        let mut values: Vec<EnumValueDef> = Vec::new();
        let mut next: i128 = 0;
        for v in e.values() {
            let Some(name_tok) = v.name() else { continue };
            let name = self.text(&name_tok).to_string();
            let span = name_tok.text_range();
            let mut ord = next;
            if let Some(x) = v.value() {
                let x = self.lower_expr(&x);
                if !x.ty.is_error() {
                    if !x.ty.is_integer() {
                        let t = self.tn(x.ty);
                        self.error(
                            Code::E(702),
                            format!("type mismatch: expected an integer type, found `{t}`"),
                            x.span,
                        );
                    } else if let Some(Const::Int(n)) = x.value {
                        if Ty::Integer.contains(n) {
                            ord = n;
                        } else {
                            self.const_out_of_range(n, Ty::Integer, x.span);
                        }
                    } else {
                        self.error(Code::E(713), "constant expression required", x.span);
                    }
                }
            }
            if let Some(prev) = values
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
                .map(|p| p.span)
            {
                let d = Diagnostic::error(
                    Code::E(701),
                    format!("`{name}` is already declared"),
                    Stage::Sema,
                )
                .with_primary(self.span(span), "declared again here")
                .with_secondary(self.span(prev), "declared here");
                self.push(d);
            } else {
                values.push(EnumValueDef { name, ord, span });
            }
            next = ord + 1;
        }
        let low = values.iter().map(|v| v.ord).min().unwrap_or(0);
        let high = values.iter().map(|v| v.ord).max().unwrap_or(0);
        self.types.get_mut(id).kind = TypeKind::Enum { values, low, high };
    }

    /// The base type and bounds of `L..H` (`docs/spec/types.md` 8.2): constants of the same
    /// ordinal type; an integer range takes the narrowest of `Integer`, `Cardinal`, `Int64` and
    /// `UInt64` that holds it. `None` after a diagnostic.
    fn subrange_kind(&mut self, r: &ast::SubrangeType) -> Option<TypeKind> {
        let range = ast::trimmed_range(r.syntax());
        let (lo_e, hi_e) = r.bounds();
        let lo = lo_e.map(|e| self.lower_expr(&e));
        let hi = hi_e.map(|e| self.lower_expr(&e));
        let (lo, hi) = (lo?, hi?);
        if lo.ty.is_error() || hi.ty.is_error() {
            return None;
        }
        for b in [&lo, &hi] {
            if b.value.is_none() {
                self.error(Code::E(713), "constant expression required", b.span);
                return None;
            }
        }
        let l = lo.value.as_ref().and_then(Const::as_int)?;
        let h = hi.value.as_ref().and_then(Const::as_int)?;
        let base = match (lo.ty, hi.ty) {
            (Ty::Char, Ty::Char) => Ty::Char,
            (Ty::Boolean, Ty::Boolean) => Ty::Boolean,
            (Ty::Enum(a), Ty::Enum(b)) if a == b => Ty::Enum(a),
            (a, b) if a.is_integer() && b.is_integer() => {
                if Ty::Integer.contains(l) && Ty::Integer.contains(h) {
                    Ty::Integer
                } else if l >= 0 && Ty::Cardinal.contains(h) {
                    Ty::Cardinal
                } else if Ty::Int64.contains(l) && Ty::Int64.contains(h) {
                    Ty::Int64
                } else {
                    Ty::UInt64
                }
            }
            (a, b) => {
                let (a, b) = (self.tn(a), self.tn(b));
                let msg = if a == b {
                    format!("type mismatch: expected an ordinal type, found `{a}`")
                } else {
                    format!("type mismatch: the bounds have types `{a}` and `{b}`")
                };
                self.error(Code::E(702), msg, range);
                return None;
            }
        };
        if l > h {
            let text = format!("{}..{}", bound_text(base, l), bound_text(base, h));
            self.error(
                Code::E(728),
                format!("invalid subrange bounds `{text}`"),
                range,
            );
            return None;
        }
        Some(TypeKind::Subrange {
            base,
            low: l,
            high: h,
        })
    }

    fn define_record(&mut self, id: TypeId, r: &ast::RecordType) {
        let mut fields: Vec<Field> = Vec::new();
        let mut seen: std::collections::HashMap<String, TextRange> =
            std::collections::HashMap::new();
        for fd in r.fields() {
            let ty = self.resolve_type(fd.type_spec());
            for n in fd.names() {
                let name = self.text(&n).to_string();
                let span = n.text_range();
                if let Some(prev) = seen.get(&name.to_ascii_lowercase()) {
                    let d = Diagnostic::error(
                        Code::E(701),
                        format!("`{name}` is already declared"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(span), "declared again here")
                    .with_secondary(self.span(*prev), "declared here");
                    self.push(d);
                    continue;
                }
                seen.insert(name.to_ascii_lowercase(), span);
                fields.push(Field { name, ty, span });
            }
        }
        self.types.get_mut(id).kind = TypeKind::Record { fields };
    }

    fn define_array(&mut self, id: TypeId, a: &ast::ArrayType) {
        let ranges = a.ranges();
        let kind = self.array_kind(&ranges, a.element());
        self.types.get_mut(id).kind = kind;
    }

    /// `array[R1, R2, …] of T` is `array[R1] of array[R2, …] of T` (`docs/spec/types.md` 2.2).
    fn array_kind(&mut self, ranges: &[ast::IndexRange], elem: Option<ast::TypeSpec>) -> TypeKind {
        let Some((first, rest)) = ranges.split_first() else {
            return TypeKind::Pending;
        };
        let bounds = self.index_range(first);
        let elem = if rest.is_empty() {
            self.resolve_type(elem)
        } else {
            let span = rest[0].syntax().text_range();
            let inner = self.add_type(None, span);
            let kind = self.array_kind(rest, elem);
            self.types.get_mut(inner).kind = kind;
            Ty::Array(inner)
        };
        match bounds {
            Some((index, low, high)) => TypeKind::Array {
                index,
                low,
                high,
                elem,
            },
            None => TypeKind::Pending,
        }
    }

    /// The index type and bounds of `L..H` or of an ordinal type name.
    fn index_range(&mut self, r: &ast::IndexRange) -> Option<(Ty, i128, i128)> {
        let range = ast::trimmed_range(r.syntax());
        let (lo_e, hi_e) = r.bounds();
        if !r.is_range() {
            let e = lo_e?;
            let Some(ty) = self.type_name_arg(&e) else {
                let x = self.lower_expr(&e);
                if !x.ty.is_error() {
                    let text = self.src(x.span);
                    self.error(Code::E(708), format!("`{text}` is not a type"), x.span);
                }
                return None;
            };
            if ty.is_error() {
                return None;
            }
            let Some((lo, hi)) = self.types.ord_range(ty).filter(|_| ty.is_ordinal()) else {
                let t = self.tn(ty);
                self.error(
                    Code::E(702),
                    format!("type mismatch: expected an ordinal type, found `{t}`"),
                    range,
                );
                return None;
            };
            if hi - lo + 1 > MAX_ARRAY_LEN {
                self.error(Code::E(728), "array type is too large", range);
                return None;
            }
            return Some((self.types.base(ty), lo, hi));
        }
        let lo = lo_e.map(|e| self.lower_expr(&e))?;
        let hi = hi_e.map(|e| self.lower_expr(&e))?;
        if lo.ty.is_error() || hi.ty.is_error() {
            return None;
        }
        for b in [&lo, &hi] {
            if b.value.is_none() {
                self.error(Code::E(713), "constant expression required", b.span);
                return None;
            }
        }
        let index = match (lo.ty, hi.ty) {
            (Ty::Char, Ty::Char) => Ty::Char,
            (Ty::Boolean, Ty::Boolean) => Ty::Boolean,
            (Ty::Enum(a), Ty::Enum(b)) if a == b => Ty::Enum(a),
            (a, b) if a.is_integer() && b.is_integer() => Ty::Integer,
            (a, b) => {
                let (a, b) = (self.tn(a), self.tn(b));
                let msg = if a == b {
                    format!("type mismatch: expected an ordinal type, found `{a}`")
                } else {
                    format!("type mismatch: the bounds have types `{a}` and `{b}`")
                };
                self.error(Code::E(702), msg, range);
                return None;
            }
        };
        let l = lo.value.as_ref().and_then(Const::as_int)?;
        let h = hi.value.as_ref().and_then(Const::as_int)?;
        if l > h {
            let text = format!("{}..{}", bound_text(index, l), bound_text(index, h));
            self.error(
                Code::E(728),
                format!("invalid array bounds `{text}`"),
                range,
            );
            return None;
        }
        if h - l + 1 > MAX_ARRAY_LEN {
            self.error(Code::E(728), "array type is too large", range);
            return None;
        }
        Some((index, l, h))
    }

    /// E0727: a record field or static array element that leads back to its own type without
    /// going through a dynamic array (`docs/spec/types.md` 1.1). The first step of the cycle is
    /// cut (its type becomes the error type) so that nothing else walks around it.
    fn check_cycles(&mut self, defined: &[TypeId]) {
        for &id in defined {
            let mut path: Vec<(TypeId, usize, TextRange)> = Vec::new();
            let mut visited: Vec<TypeId> = Vec::new();
            if !self.reaches(id, id, &mut path, &mut visited) {
                continue;
            }
            let def = self.types.get(id);
            let name = self.types.name(match def.kind {
                TypeKind::Array { .. } => Ty::Array(id),
                _ => Ty::Record(id),
            });
            let mut d = Diagnostic::error(
                Code::E(727),
                format!("type `{name}` contains itself"),
                Stage::Sema,
            )
            .with_primary(self.span(def.span), "");
            for (_, _, span) in &path {
                d = d.with_secondary(self.span(*span), "part of the cycle");
            }
            self.push(d);
            let (owner, slot, _) = path[0];
            match &mut self.types.get_mut(owner).kind {
                TypeKind::Record { fields } => fields[slot].ty = Ty::Error,
                TypeKind::Array { elem, .. } => *elem = Ty::Error,
                TypeKind::DynArray { .. }
                | TypeKind::Exception { .. }
                | TypeKind::Enum { .. }
                | TypeKind::Subrange { .. }
                | TypeKind::Set { .. }
                | TypeKind::Pending => {}
            }
        }
    }

    /// Whether `from` contains `target` through record fields and static array elements; `path`
    /// collects the steps (owner, slot, position).
    fn reaches(
        &self,
        from: TypeId,
        target: TypeId,
        path: &mut Vec<(TypeId, usize, TextRange)>,
        visited: &mut Vec<TypeId>,
    ) -> bool {
        if visited.contains(&from) {
            return false;
        }
        visited.push(from);
        let def = self.types.get(from);
        let steps: Vec<(usize, Ty, TextRange)> = match &def.kind {
            TypeKind::Record { fields } => fields
                .iter()
                .enumerate()
                .map(|(i, f)| (i, f.ty, f.span))
                .collect(),
            TypeKind::Array { elem, .. } => vec![(0, *elem, def.span)],
            TypeKind::DynArray { .. }
            | TypeKind::Exception { .. }
            | TypeKind::Enum { .. }
            | TypeKind::Subrange { .. }
            | TypeKind::Set { .. }
            | TypeKind::Pending => Vec::new(),
        };
        for (slot, ty, span) in steps {
            // A dynamic array holds its elements by reference, so it ends the walk
            let (Ty::Record(next) | Ty::Array(next)) = ty else {
                continue;
            };
            path.push((from, slot, span));
            if next == target || self.reaches(next, target, path, visited) {
                return true;
            }
            path.pop();
        }
        false
    }

    /// The type an argument names (`Low(Integer)`, `Default(TPoint)`, `array[Char]`), without
    /// reporting anything. `None` if it is not a type name.
    pub(super) fn type_name_arg(&mut self, e: &ast::Expr) -> Option<Ty> {
        match e {
            ast::Expr::NameRef(n) => {
                if n.is_string_keyword() {
                    return Some(Ty::String);
                }
                let lower = n.text().to_ascii_lowercase();
                match self.lookup_type_symbol(&lower) {
                    Some(t) => t,
                    None => self.unit_type(&lower).or_else(|| Ty::from_name(&lower)),
                }
            }
            ast::Expr::Field(f) => {
                let base = f.base()?;
                let ast::Expr::NameRef(b) = base else {
                    return None;
                };
                let field = f.field()?;
                let unit = b.text().to_ascii_lowercase();
                if unit == "system" && self.lookup_type_symbol("system").is_none() {
                    Ty::from_name(&field.text().to_ascii_lowercase())
                } else {
                    None
                }
            }
            ast::Expr::Paren(p) => self.type_name_arg(&p.inner()?),
            _ => None,
        }
    }

    /// A type written as a name (`TypeRef`).
    pub(super) fn resolve_type_ref(&mut self, t: &ast::TypeRef) -> Ty {
        if t.string_token().is_some() {
            return Ty::String;
        }
        let Some(q) = t.name() else { return Ty::Error };
        let text = q.text();
        let lower = text.to_ascii_lowercase();
        let range = q.syntax().text_range();
        // A declared type comes first (it may hide a type of `System`; `docs/spec/types.md` 1.1)
        if !lower.contains('.') {
            match self.lookup_type_symbol(&lower) {
                Some(Some(ty)) => return ty,
                Some(None) => {
                    self.error(Code::E(708), format!("`{text}` is not a type"), range);
                    return Ty::Error;
                }
                None => {}
            }
            // A type of a unit in `uses` (`docs/spec/units.md` 3.1)
            match self.lookup_in_units(&lower, &text, range) {
                super::UnitLookup::Found(super::Resolved::Type(ty)) => return ty,
                super::UnitLookup::Found(_) => {
                    self.error(Code::E(708), format!("`{text}` is not a type"), range);
                    return Ty::Error;
                }
                super::UnitLookup::Error => return Ty::Error,
                super::UnitLookup::NotFound => {}
            }
        } else {
            // A name qualified with a package unit (`docs/spec/units.md` 3.2)
            let chain: Vec<(String, TextRange)> = q
                .idents()
                .map(|t| (t.text().to_string(), t.text_range()))
                .collect();
            if let Some((u, n)) = self.unit_prefix(&chain) {
                if n + 1 == chain.len() {
                    let (name, r) = chain[n].clone();
                    return match self.resolve_in_unit(u, &name, r) {
                        super::Resolved::Type(ty) => ty,
                        super::Resolved::Error => Ty::Error,
                        _ => {
                            self.error(Code::E(708), format!("`{text}` is not a type"), range);
                            Ty::Error
                        }
                    };
                }
                if n == chain.len() {
                    self.error(Code::E(708), format!("`{text}` is not a type"), range);
                    return Ty::Error;
                }
            }
        }
        // A qualified name such as `System.Integer`
        let unqualified = lower.strip_prefix("system.").unwrap_or(&lower).to_string();
        if let Some(ty) = Ty::from_name(&unqualified) {
            return ty;
        }
        if let Some(ty) =
            builtins::builtin_exception(&unqualified).or_else(|| self.sysutils_exception(&lower))
        {
            return ty;
        }
        // A type of `System.SysUtils` (`TReplaceFlags`, `TCaseMapping`, …; `docs/spec/semantics.md`
        // 5.3), resolved as in an expression: without `uses System.SysUtils`, E0700 with its fix
        let (unit, last) = match lower.rsplit_once('.') {
            Some((u, n)) => (Some(u), n),
            None => (None, lower.as_str()),
        };
        if matches!(
            builtins::lookup_sysutils(last),
            Some(n) if !matches!(n, builtins::SysUtilsName::Func(_))
        ) {
            let name = &text[text.len() - last.len()..];
            let resolved = match unit {
                None => Some(self.resolve_ident(name, range)),
                Some(u) if u != "system" && self.is_unit_name(u) => {
                    Some(self.resolve_in_unit(super::UnitRef::SysUtils, name, range))
                }
                Some(_) => None,
            };
            match resolved {
                Some(super::Resolved::Type(ty)) => return ty,
                Some(_) => return Ty::Error,
                None => {}
            }
        }
        // Names that are not types (variables, constants, routines, units) give E0708
        if self.unit_named(&lower).is_some() {
            self.error(Code::E(708), format!("`{text}` is not a type"), range);
            return Ty::Error;
        }
        if self.broken_unit_visible() {
            // A unit that could not be read may declare it (`docs/spec/units.md` 3.4)
            return Ty::Error;
        }
        if self.is_unit_name(&lower)
            || (!lower.contains('.') && self.find_visible(&lower).is_some())
        {
            self.error(Code::E(708), format!("`{text}` is not a type"), range);
            return Ty::Error;
        }
        if lower.contains('.') {
            self.error(
                Code::E(700),
                format!("undeclared identifier `{text}`"),
                range,
            );
            return Ty::Error;
        }
        match builtins::lookup_system(&lower) {
            Some(builtins::SystemName::Reserved(m)) => {
                self.error(
                    Code::E(3200),
                    format!("`{text}` is not supported yet (planned for {m})"),
                    range,
                );
            }
            Some(builtins::SystemName::Deleted(code)) => {
                let note = builtins::deleted_note(code);
                self.error(code, format!("`{text}` is not supported in Puy"), range)
                    .notes
                    .push(note.into());
            }
            Some(_) => {
                self.error(Code::E(708), format!("`{text}` is not a type"), range);
            }
            None => {
                if lower == "reference" {
                    self.error(
                        Code::E(3200),
                        "`reference to` is not supported yet (planned for M3)",
                        range,
                    );
                } else {
                    let d = Diagnostic::error(
                        Code::E(700),
                        format!("undeclared identifier `{text}`"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "");
                    let d = self.with_uses_unit_fix(d, &lower);
                    self.push(d);
                }
            }
        }
        Ty::Error
    }

    // ---- Constant shapes (`docs/spec/types.md` chapter 4) ----

    /// The value of a declaration whose type is written: a constant shape for a record or static
    /// array, otherwise an expression converted to the type.
    pub(super) fn lower_init(&mut self, e: &ast::Expr, ty: Ty, range_check: bool) -> Expr {
        let span = ast::trimmed_range(e.syntax());
        match e {
            ast::Expr::RecordConst(rc) => self.record_const(rc, ty, span),
            ast::Expr::ArrayConst(ac) => {
                let elems: Vec<ast::Expr> = ac.elements().collect();
                self.array_const(&elems, ty, span)
            }
            // A one-element array constant is written `(X)`
            ast::Expr::Paren(p) if self.types.slot_count(ty) == 1 && matches!(ty, Ty::Array(_)) => {
                let elems: Vec<ast::Expr> = p.inner().into_iter().collect();
                self.array_const(&elems, ty, span)
            }
            _ => {
                let x = self.lower_expected(e, ty);
                if ty.is_error() || x.ty.is_error() {
                    x
                } else {
                    self.coerce(x, ty, range_check, None)
                }
            }
        }
    }

    fn shape_mismatch(&mut self, ty: Ty, found: &str, span: TextRange) -> Expr {
        if !ty.is_error() {
            let t = self.tn(ty);
            self.error(
                Code::E(702),
                format!("type mismatch: expected `{t}`, found {found}"),
                span,
            );
        }
        error_expr(span)
    }

    /// One element of a shape: a constant of the element type.
    fn shape_element(&mut self, e: &ast::Expr, ty: Ty) -> Option<Const> {
        let x = self.lower_init(e, ty, true);
        if x.ty.is_error() {
            return None;
        }
        match x.value {
            Some(v) => Some(v),
            None => {
                self.error(Code::E(713), "constant expression required", x.span);
                None
            }
        }
    }

    fn array_const(&mut self, elems: &[ast::Expr], ty: Ty, span: TextRange) -> Expr {
        let Some((_, lo, hi, elem)) = self.types.array(ty) else {
            for e in elems {
                self.lower_expr(e);
            }
            return self.shape_mismatch(ty, "an array constant", span);
        };
        let n = (hi - lo + 1) as usize;
        if elems.len() != n {
            self.error(
                Code::E(702),
                format!("expected {n} elements, found {}", elems.len()),
                span,
            );
            return error_expr(span);
        }
        let mut items = Vec::with_capacity(n);
        let mut failed = false;
        for e in elems {
            match self.shape_element(e, elem) {
                Some(v) => items.push(v),
                None => failed = true,
            }
        }
        if failed {
            return error_expr(span);
        }
        const_shape(Const::Agg(items), ty, span)
    }

    fn record_const(&mut self, rc: &ast::RecordConst, ty: Ty, span: TextRange) -> Expr {
        let written: Vec<ast::FieldConst> = rc.fields().collect();
        if !matches!(ty, Ty::Record(_)) {
            for f in &written {
                if let Some(v) = f.value() {
                    self.lower_expr(&v);
                }
            }
            return self.shape_mismatch(ty, "a record constant", span);
        }
        let fields: Vec<Field> = self.types.fields(ty).to_vec();
        let tname = self.tn(ty);
        let mut items = Vec::with_capacity(fields.len());
        let mut failed = false;
        for (i, f) in written.iter().enumerate() {
            let Some(name_tok) = f.name() else {
                failed = true;
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let range = name_tok.text_range();
            let expected = fields.get(i);
            if !expected.is_some_and(|x| x.name.eq_ignore_ascii_case(&name)) {
                failed = true;
                if !fields.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
                    self.error(
                        Code::E(700),
                        format!("`{tname}` has no field `{name}`"),
                        range,
                    );
                } else if let Some(x) = expected {
                    let x = x.name.clone();
                    self.error(
                        Code::E(702),
                        format!("expected field `{x}`, found `{name}`"),
                        range,
                    )
                    .notes
                    .push("write every field of the record in declaration order".into());
                } else {
                    self.error(
                        Code::E(702),
                        format!("field `{name}` is written twice"),
                        range,
                    );
                }
                // One error per constant: the fields after it are out of step
                break;
            }
            let fty = expected.map_or(Ty::Error, |x| x.ty);
            match f.value().and_then(|v| self.shape_element(&v, fty)) {
                Some(v) => items.push(v),
                None => failed = true,
            }
        }
        if !failed && written.len() < fields.len() {
            let missing = fields[written.len()].name.clone();
            self.error(Code::E(702), format!("missing field `{missing}`"), span)
                .notes
                .push("write every field of the record in declaration order".into());
            failed = true;
        }
        if failed {
            return error_expr(span);
        }
        const_shape(Const::Agg(items), ty, span)
    }
}

fn error_expr(span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Error,
        ty: Ty::Error,
        value: None,
        span,
    }
}

fn const_shape(value: Const, ty: Ty, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Const,
        ty,
        value: Some(value),
        span,
    }
}
