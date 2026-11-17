//! The distinct types erased before lowering (`docs/spec/types.md` 1.3): a distinct type has the
//! representation and the operations of its root type, so the program lowered has no
//! `Ty::Distinct`. A value read is of the root type. A variable, field, element or parameter of
//! a distinct type whose root is an ordinal type narrower than 64 bits becomes a subrange of the
//! whole range of the root, named by the distinct type, so that the range checks on what is
//! stored there name it (as a subrange's); any other one becomes its root type.

use std::collections::HashMap;

use super::hir::{Expr, Program, Routine};
use super::hirwalk::{HirVisit, walk_expr, walk_routine};
use super::types::{Ty, TypeId, TypeKind, TypeTable};

/// The program with its distinct types erased, or `None` when it has none (the program is
/// lowered as it is).
pub fn erase(p: &Program) -> Option<Program> {
    if !p
        .types
        .defs
        .iter()
        .any(|d| matches!(d.kind, TypeKind::Distinct { .. }))
    {
        return None;
    }
    let mut q = p.clone();
    let storage = storage_types(&mut q.types);
    visit_table(&mut q.types, &mut |t: &mut Ty| {
        if let Ty::Distinct(id) = *t {
            *t = storage[&id].0;
        }
    });
    walk_program(&mut q, &mut Eraser { storage: &storage });
    #[cfg(debug_assertions)]
    {
        // Nothing the walk reaches is left a distinct type (the definitions of the distinct
        // types themselves stay in the table, unused)
        let mut c = Left(false);
        walk_program(&mut q, &mut c);
        let mut left = false;
        visit_table(&mut q.types, &mut |t: &mut Ty| {
            left |= matches!(t, Ty::Distinct(_));
        });
        debug_assert!(!c.0 && !left, "a distinct type is left after erasing");
    }
    Some(q)
}

/// Walks the global variables, every routine and the attributes' arguments of the program.
fn walk_program(q: &mut Program, v: &mut impl HirVisit) {
    for g in q.globals.iter_mut().chain(q.generic_globals.iter_mut()) {
        v.ty(&mut g.ty);
    }
    let mut routines: Vec<&mut Routine> = q.routines.iter_mut().collect();
    routines.push(&mut q.main);
    routines.extend(q.init.as_mut());
    for u in &mut q.units {
        routines.extend(u.init.as_mut());
        routines.extend(u.fini.as_mut());
    }
    for r in routines {
        walk_routine(r, v);
    }
    for a in &mut q.attrs {
        for e in &mut a.args {
            walk_expr(e, v);
        }
    }
}

/// Whether a walk reached a distinct type.
#[cfg(debug_assertions)]
struct Left(bool);

#[cfg(debug_assertions)]
impl HirVisit for Left {
    fn ty(&mut self, t: &mut Ty) {
        self.0 |= matches!(t, Ty::Distinct(_));
    }
}

/// The type in storage and the root of each distinct type, turning the definitions of those
/// that become subranges into the subranges.
fn storage_types(types: &mut TypeTable) -> HashMap<TypeId, (Ty, Ty)> {
    let mut out = HashMap::new();
    for k in 0..types.defs.len() {
        let id = TypeId(k as u32);
        if !matches!(types.get(id).kind, TypeKind::Distinct { .. }) {
            continue;
        }
        let root = types.root(Ty::Distinct(id));
        let narrow = root.is_ordinal() && !matches!(root, Ty::Int64 | Ty::UInt64);
        out.insert(id, (if narrow { Ty::Subrange(id) } else { root }, root));
    }
    for (&id, &(st, root)) in &out {
        if st != Ty::Subrange(id) {
            continue;
        }
        let (low, high) = root.range().expect("an ordinal root type has a range");
        // The computation type of the root (as `Inc` computes in it), so that only the
        // subrange's check, which names the distinct type, can fail
        let base = if root.fits_in_integer() {
            Ty::Integer
        } else if root == Ty::Cardinal {
            Ty::Int64
        } else {
            root
        };
        types.get_mut(id).kind = TypeKind::Subrange { base, low, high };
    }
    out
}

/// Calls `st` on each type in the members of the types (fields, elements, parameters, …), not on
/// the original type of a distinct type.
fn visit_table(types: &mut TypeTable, st: &mut impl FnMut(&mut Ty)) {
    let method = |m: &mut super::types::ClassMethod, st: &mut dyn FnMut(&mut Ty)| {
        for (t, _) in &mut m.params {
            st(t);
        }
        if let Some(t) = &mut m.ret {
            st(t);
        }
    };
    for d in &mut types.defs {
        match &mut d.kind {
            TypeKind::Record {
                fields,
                methods,
                statics,
                properties,
                ..
            } => {
                fields.iter_mut().for_each(|f| st(&mut f.ty));
                methods.iter_mut().for_each(|m| method(m, st));
                statics.iter_mut().for_each(|s| erase_static(s, st));
                properties.iter_mut().for_each(|p| erase_property(p, st));
            }
            TypeKind::Class {
                fields,
                methods,
                statics,
                properties,
                ..
            } => {
                fields.iter_mut().for_each(|f| st(&mut f.ty));
                methods.iter_mut().for_each(|m| method(m, st));
                statics.iter_mut().for_each(|s| erase_static(s, st));
                properties.iter_mut().for_each(|p| erase_property(p, st));
            }
            TypeKind::Interface {
                methods,
                properties,
                ..
            } => {
                methods.iter_mut().for_each(|m| method(m, st));
                properties.iter_mut().for_each(|p| erase_property(p, st));
            }
            TypeKind::Helper {
                target,
                methods,
                statics,
                properties,
                ..
            } => {
                st(target);
                methods.iter_mut().for_each(|m| method(m, st));
                statics.iter_mut().for_each(|s| erase_static(s, st));
                properties.iter_mut().for_each(|p| erase_property(p, st));
            }
            TypeKind::Array { index, elem, .. } => {
                st(index);
                st(elem);
            }
            TypeKind::DynArray { elem, .. } => st(elem),
            TypeKind::Set { base, .. } => st(base),
            TypeKind::Proc { params, ret, .. } => {
                params.iter_mut().for_each(|p| st(&mut p.ty));
                if let Some(t) = ret {
                    st(t);
                }
            }
            TypeKind::Subrange { base, .. } => st(base),
            TypeKind::ClassRef { .. }
            | TypeKind::Distinct { .. }
            | TypeKind::Enum { .. }
            | TypeKind::Param { .. }
            | TypeKind::Pending => {}
        }
    }
}

fn erase_static(s: &mut super::types::ClassStatic, st: &mut impl FnMut(&mut Ty)) {
    match &mut s.kind {
        super::types::StaticKind::Var(_, t)
        | super::types::StaticKind::Const(_, t)
        | super::types::StaticKind::Type(t) => st(t),
    }
}

fn erase_property(p: &mut super::types::ClassProperty, st: &mut impl FnMut(&mut Ty)) {
    st(&mut p.ty);
    p.params.iter_mut().for_each(st);
}

/// The walk over the routines: an expression's type becomes the root, any other type the type
/// in storage.
struct Eraser<'a> {
    storage: &'a HashMap<TypeId, (Ty, Ty)>,
}

impl HirVisit for Eraser<'_> {
    fn ty(&mut self, t: &mut Ty) {
        if let Ty::Distinct(id) = *t {
            *t = self.storage[&id].0;
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        if let Ty::Distinct(id) = e.ty {
            e.ty = self.storage[&id].1;
        }
    }
}
