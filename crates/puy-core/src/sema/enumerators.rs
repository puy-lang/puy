//! `for..in` with `GetEnumerator` (`docs/spec/objects.md` chapter 13): finding the enumerator's
//! members and checking what they must be.

use super::Sema;
use super::hir::{EnumCleanup, Expr, ExprKind, VarId};
use super::methods::Member;
use super::types::{MethodKind, SelfKind, TOBJECT, TREFOBJECT, Ty, TypeId};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::{Span, TextRange};

/// The note of E0730 on what `for..in` iterates over (`docs/spec/objects.md` 13.1).
const ITERATE_NOTE: &str = "`for..in` iterates over arrays, strings, sets and values with a \
    `GetEnumerator` function; it returns an enumerator with a `MoveNext` function returning \
    `Boolean` and a `Current` property";

/// The note of E0730 on a `UTF8String` (`docs/spec/types.md` 8.5, `objects.md` 10.9).
const UTF8_NOTE: &str = "convert it with `string(S)` to iterate over its characters, or write a \
    helper with `GetEnumerator`";

/// The calls of an enumerator found for a collection.
pub(super) struct EnumCalls {
    pub get: Expr,
    /// The enumerator's hidden variable.
    pub var: VarId,
    pub move_next: Expr,
    /// `E.Current`, of the element type.
    pub current: Expr,
    pub cleanup: EnumCleanup,
}

/// What a member found for the enumerator is.
enum Found {
    Member(TypeId, Member),
    /// Not found; nothing reported.
    Missing,
    /// Reported already (E0743, E0754).
    Reported,
}

impl Sema<'_, '_> {
    /// E0730 for a collection with neither an iteration of its own nor `GetEnumerator`
    /// (`docs/spec/types.md` chapter 5, `objects.md` 13.1).
    pub(super) fn cannot_iterate(&mut self, ty: Ty, span: TextRange) {
        let msg = format!("cannot iterate over a value of type `{}`", self.tn(ty));
        let note = if ty == Ty::Utf8String {
            UTF8_NOTE
        } else {
            ITERATE_NOTE
        };
        self.error(Code::E(730), msg, span).notes.push(note.into());
    }

    /// The enumerator of `C` in `for X in C` (`docs/spec/objects.md` 13.1), whose static type has
    /// no iteration of its own: `C.GetEnumerator` on the collection's hidden variable `coll_var`,
    /// and `MoveNext` / `Current` on a new hidden variable of the enumerator's type. `None` after
    /// a diagnostic.
    pub(super) fn enumerator_calls(&mut self, coll: &Expr, coll_var: VarId) -> Option<EnumCalls> {
        let span = coll.span;
        let get = match self.enum_member(coll.ty, "GetEnumerator", span) {
            Found::Member(owner, member) => {
                let ok = match &member {
                    Member::Methods { .. } => self.method_without_params(owner, "GetEnumerator"),
                    _ => Err(None),
                };
                match ok {
                    Ok(ret)
                        if matches!(
                            self.types.strong(ret),
                            Ty::Class(_) | Ty::Record(_) | Ty::Intf(_)
                        ) => {}
                    Ok(_) | Err(_) => {
                        let decl = ok.err().flatten().or_else(|| {
                            self.methods_named(owner, "GetEnumerator")
                                .first()
                                .and_then(|(c, m)| self.method_span(*c, m))
                        });
                        let t = self.tn(coll.ty);
                        let mut d = Diagnostic::error(
                            Code::E(730),
                            format!("`GetEnumerator` of `{t}` must be a function without parameters returning a class, record or interface"),
                            Stage::Sema,
                        )
                        .with_primary(self.span(span), "");
                        if let Some(s) = decl {
                            d = d.with_secondary(s, "declared here");
                        }
                        self.push(d);
                        return None;
                    }
                }
                let recv = Self::hidden_receiver(coll_var, coll.ty, span);
                self.enum_call(recv, owner, member)
            }
            Found::Missing => {
                self.cannot_iterate(coll.ty, span);
                return None;
            }
            Found::Reported => return None,
        };
        if get.ty.is_error() {
            return None;
        }
        let ety = self.types.strong(get.ty);
        let var = self.add_var(String::new(), ety, super::hir::VarKind::Hidden, span);
        let move_next = match self.enum_member(ety, "MoveNext", span) {
            Found::Member(owner, member @ Member::Methods { .. })
                if self.method_without_params(owner, "MoveNext") == Ok(Ty::Boolean) =>
            {
                let recv = Self::hidden_receiver(var, ety, span);
                self.enum_call(recv, owner, member)
            }
            Found::Reported => return None,
            _ => {
                let t = self.tn(ety);
                self.error(
                    Code::E(730),
                    format!("enumerator `{t}` has no `MoveNext` function returning `Boolean`"),
                    span,
                );
                return None;
            }
        };
        let current = match self.enum_member(ety, "Current", span) {
            Found::Member(owner, member)
                if match &member {
                    Member::Property(_, p) => {
                        p.read.is_some() && p.params.is_empty() && !p.is_class
                    }
                    Member::Methods { .. } => self.method_without_params(owner, "Current").is_ok(),
                    _ => false,
                } =>
            {
                let recv = Self::hidden_receiver(var, ety, span);
                self.enum_call(recv, owner, member)
            }
            Found::Reported => return None,
            _ => {
                let t = self.tn(ety);
                self.error(
                    Code::E(730),
                    format!("enumerator `{t}` has no readable `Current` property"),
                    span,
                );
                return None;
            }
        };
        if move_next.ty.is_error() || current.ty.is_error() {
            return None;
        }
        Some(EnumCalls {
            get,
            var,
            move_next,
            current,
            cleanup: self.enum_cleanup(ety),
        })
    }

    /// How the enumerator of a static type is let go of (`docs/spec/objects.md` 13.2).
    fn enum_cleanup(&self, ety: Ty) -> EnumCleanup {
        match ety {
            Ty::Class(c) if c == TOBJECT => EnumCleanup::ByContents,
            Ty::Intf(_) => EnumCleanup::ByContents,
            Ty::Class(c) if self.types.is_descendant(c, TREFOBJECT) => EnumCleanup::Release,
            Ty::Class(_) => EnumCleanup::Free,
            _ => EnumCleanup::Release,
        }
    }

    /// The call of an enumerator's member on a hidden variable. The three calls share the
    /// collection's range, so the overload picked for another one is forgotten first.
    fn enum_call(&mut self, recv: Expr, owner: TypeId, member: Member) -> Expr {
        let span = recv.span;
        self.overload_picks.remove(&span);
        let e = self.member_expr(recv, owner, member, false, None, true, span, span);
        self.overload_picks.remove(&span);
        e
    }

    /// A hidden variable as the receiver of an enumerator's call.
    fn hidden_receiver(var: VarId, ty: Ty, span: TextRange) -> Expr {
        Expr {
            kind: ExprKind::Var(var),
            ty,
            value: None,
            span,
        }
    }

    /// The member `name` of a value of type `ty` as `X.name` finds it (`docs/spec/objects.md`
    /// 10.4): the type's own member, then its helpers' (an interface has none).
    fn enum_member(&mut self, ty: Ty, name: &str, range: TextRange) -> Found {
        let own = match ty {
            Ty::Class(c) | Ty::Intf(c) | Ty::Record(c) => Some(c),
            _ => None,
        };
        if let Some(c) = own {
            match self.find_member(c, name, range) {
                Ok(m) => return Found::Member(c, m),
                Err(true) => return Found::Reported,
                Err(false) if matches!(ty, Ty::Intf(_)) => return Found::Missing,
                Err(false) => {}
            }
        }
        match self.find_helper_member(ty, name, range, super::helpers::HelperRecv::ForIn) {
            Ok((h, super::helpers::HelperMember::Declared)) => {
                match self.find_member(h, name, range) {
                    Ok(m) => Found::Member(h, m),
                    Err(true) => Found::Reported,
                    Err(false) => Found::Missing,
                }
            }
            Ok(_) | Err(false) => Found::Missing,
            Err(true) => Found::Reported,
        }
    }

    /// The return type of the visible instance function `name` of `owner` (or its ancestors)
    /// without parameters, or the declaration of the method that is not one.
    fn method_without_params(&self, owner: TypeId, name: &str) -> Result<Ty, Option<Span>> {
        let methods = self.methods_named(owner, name);
        let visible: Vec<_> = methods
            .iter()
            .filter(|(c, m)| self.can_see(m.vis, *c))
            .collect();
        let Some((c, m)) = visible
            .iter()
            .find(|(_, m)| m.params.is_empty())
            .or(visible.first())
        else {
            return Err(None);
        };
        match m.ret {
            Some(ret)
                if m.params.is_empty()
                    && m.kind == MethodKind::Function
                    && m.self_kind == SelfKind::Instance =>
            {
                Ok(ret)
            }
            _ => Err(self.method_span(*c, m)),
        }
    }
}
