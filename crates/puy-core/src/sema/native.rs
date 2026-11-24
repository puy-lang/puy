//! `native` routines and extension classes (`docs/spec/extensions.md`): routines whose bodies are
//! Rust functions of an extension, declared in the extension's unit.

use super::RoutineId;
use super::Sema;
use super::types::{TREFOBJECT, Ty, TypeId, TypeKind, TypeTable};
use crate::bytecode::{ExtSig, FULL_RANGE, MAX_EXT_SHAPE_DEPTH, MAX_EXT_SHAPE_NODES, ext_kind};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::TextRange;
use crate::syntax::ast::ParamMode;

/// The kind a value of `ty` is passed to or returned from a Rust function in
/// (`docs/spec/extensions.md` 3; [`ext_kind`]), or `None` when a `native` routine cannot take it.
pub(crate) fn native_kind(types: &TypeTable, ty: Ty) -> Option<u8> {
    let ty = types.root(ty);
    if ty.is_ordinal() {
        return Some(ext_kind::INT);
    }
    Some(match ty {
        Ty::Single => ext_kind::SINGLE,
        Ty::Double => ext_kind::DOUBLE,
        Ty::String => ext_kind::STRING,
        Ty::Utf8String => ext_kind::UTF8,
        Ty::DynArray(_) if types.dyn_elem(ty) == Some(Ty::Byte) => ext_kind::BYTES,
        // An instance of a reference counted class: its handle is counted, so Rust can hold it
        Ty::Class(id) if types.is_descendant(id, TREFOBJECT) => ext_kind::OBJECT,
        Ty::Callable(_) if callback_sig(types, ty).is_some() => ext_kind::CALLABLE,
        Ty::Record(_) | Ty::DynArray(_) => native_shape(types, ty)?.kind,
        _ => return None,
    })
}

/// The shape of a record or dynamic array (other than `TBytes`) a `native` routine takes or
/// gives (`docs/spec/extensions.md` 3; [`crate::bytecode::ExtShape`]), with types as the
/// analysis knows them, or `None` when one of its fields or elements cannot be copied: only
/// ordinals, reals, strings, `UTF8String`, `TBytes`, and records and dynamic arrays of these, to
/// [`MAX_EXT_SHAPE_DEPTH`] levels and [`MAX_EXT_SHAPE_NODES`] fields and elements in all (a
/// record's fields are written again wherever the record is, so the shape of types nested in
/// each other twice at each level grows exponentially).
pub(crate) fn native_shape(types: &TypeTable, ty: Ty) -> Option<NativeShape> {
    let root = types.root(ty);
    match root {
        Ty::DynArray(_) if types.dyn_elem(root) == Some(Ty::Byte) => None,
        Ty::Record(_) | Ty::DynArray(_) => {
            item_shape(types, ty, 0, &mut MAX_EXT_SHAPE_NODES.clone())
        }
        _ => None,
    }
}

/// The shape of a record or dynamic array with its types as the analysis knows them
/// ([`native_shape`]); codegen gives them their type indexes.
#[derive(Debug, Clone)]
pub struct NativeShape {
    pub kind: u8,
    /// The record or dynamic array type of a record, a dynamic array or a `TBytes`.
    pub ty: Option<Ty>,
    pub range: (i64, i64),
    pub items: Vec<NativeShape>,
}

/// The shape of a value of `ty` as a field or an element at nesting `depth` ([`native_shape`]),
/// taking its nodes from the `budget` of nodes left.
fn item_shape(
    types: &TypeTable,
    declared: Ty,
    depth: usize,
    budget: &mut usize,
) -> Option<NativeShape> {
    *budget = budget.checked_sub(1)?;
    let ty = types.root(declared);
    let leaf = |kind, t: Option<Ty>| {
        Some(NativeShape {
            kind,
            ty: t,
            range: native_range(types, ty),
            items: Vec::new(),
        })
    };
    if ty.is_ordinal() {
        return leaf(ext_kind::INT, None);
    }
    match ty {
        Ty::Single => leaf(ext_kind::SINGLE, None),
        Ty::Double => leaf(ext_kind::DOUBLE, None),
        Ty::String => leaf(ext_kind::STRING, None),
        Ty::Utf8String => leaf(ext_kind::UTF8, None),
        Ty::DynArray(_) if types.dyn_elem(ty) == Some(Ty::Byte) => {
            leaf(ext_kind::BYTES, Some(declared))
        }
        Ty::Record(_) | Ty::DynArray(_) if depth + 1 < MAX_EXT_SHAPE_DEPTH => {
            let (kind, items) = match ty {
                Ty::Record(_) => (
                    ext_kind::RECORD,
                    types
                        .fields(ty)
                        .iter()
                        .map(|f| item_shape(types, f.ty, depth + 1, budget))
                        .collect::<Option<Vec<_>>>()?,
                ),
                _ => (
                    ext_kind::ARRAY,
                    vec![item_shape(types, types.dyn_elem(ty)?, depth + 1, budget)?],
                ),
            };
            Some(NativeShape {
                kind,
                ty: Some(declared),
                range: FULL_RANGE,
                items,
            })
        }
        _ => None,
    }
}

/// The range of a value of `ty` in a slot, which the VM checks what an extension gives against
/// (`docs/spec/extensions.md` 3): the declared range of an ordinal type, the whole of `i64` for
/// `UInt64` (its bits) and for any other type.
pub(crate) fn native_range(types: &TypeTable, ty: Ty) -> (i64, i64) {
    match types.ord_range(types.root(ty)) {
        Some((lo, hi)) if lo >= i128::from(i64::MIN) && hi <= i128::from(i64::MAX) => {
            (lo as i64, hi as i64)
        }
        _ => crate::bytecode::FULL_RANGE,
    }
}

/// The signature of a callable value a `native` routine takes (`docs/spec/extensions.md` 3.2):
/// value or `const` parameters and a result of the kinds a callback takes (neither `TBytes` nor
/// a callable value, nor a record or a dynamic array), or `None`.
pub(crate) fn callback_sig(types: &TypeTable, ty: Ty) -> Option<ExtSig> {
    let (_, sig_params, ret) = types.proc_sig(types.root(ty))?;
    let kind = |t: Ty| {
        native_kind(types, t).filter(|k| {
            !matches!(
                *k,
                ext_kind::BYTES | ext_kind::CALLABLE | ext_kind::RECORD | ext_kind::ARRAY
            )
        })
    };
    let params = sig_params
        .iter()
        .map(|p| match p.mode {
            ParamMode::Value | ParamMode::Const => kind(p.ty),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()?;
    let ret = match ret {
        Some(t) => kind(t)?,
        None => ext_kind::NONE,
    };
    let ranges = sig_params
        .iter()
        .map(|p| native_range(types, p.ty))
        .collect();
    Some(ExtSig {
        params,
        ret,
        ranges,
    })
}

impl Sema<'_, '_> {
    /// Whether `native` (at `at`) may be written on the routine being declared: only in the unit of
    /// an extension, and not on a nested routine (E0770) nor with `forward` (E0771).
    pub(super) fn native_allowed(&mut self, at: TextRange, has_forward: bool) -> bool {
        if self.input.extension.is_none() {
            self.error(
                Code::E(770),
                "`native` is only allowed in the unit of an extension",
                at,
            )
            .notes
            .push(
                "the body of a `native` routine is a Rust function its extension registers".into(),
            );
            return false;
        }
        if self.cur.is_some() {
            self.error(Code::E(770), "a nested routine cannot be `native`", at);
            return false;
        }
        if has_forward {
            self.error(Code::E(771), "a `native` routine cannot be `forward`", at);
            return false;
        }
        true
    }

    /// E0771: `native` written again on the declaration that completes a routine.
    pub(super) fn native_misplaced(&mut self, at: TextRange) {
        self.error(
            Code::E(771),
            "`native` is written only on the first declaration of a routine",
            at,
        );
    }

    /// E0771: a body written for the `native` routine `id`.
    pub(super) fn native_with_body(&mut self, name: &str, at: TextRange, id: RoutineId) {
        let declared = self.routines[id.0 as usize].name_span;
        let d = Diagnostic::error(
            Code::E(771),
            format!("`native` routine `{name}` cannot have a body"),
            Stage::Sema,
        )
        .with_primary(self.span(at), "")
        .with_secondary(self.span(declared), "declared here")
        .with_note("its body is a Rust function of the extension");
        self.push(d);
    }

    /// E0773: what the parameters and the result of the `native` routine `id` cannot be
    /// (`docs/spec/extensions.md` 3). `class` is the class of a constructor, whose result is the
    /// new instance.
    pub(super) fn check_native_signature(
        &mut self,
        id: RoutineId,
        class: Option<TypeId>,
        at: TextRange,
    ) {
        let r = &self.routines[id.0 as usize];
        let (overload, generic) = (r.overload, !r.type_params.is_empty());
        let params: Vec<(Ty, ParamMode, TextRange)> =
            r.params.iter().map(|p| (p.ty, p.mode, p.span)).collect();
        let ret = r.ret.or(class.map(Ty::Class));
        if overload {
            self.error(Code::E(773), "a `native` routine cannot be overloaded", at);
        }
        if generic {
            self.error(
                Code::E(773),
                "a `native` routine cannot have type parameters",
                at,
            );
        }
        for (ty, mode, span) in params {
            if matches!(mode, ParamMode::Var | ParamMode::Out) {
                self.error(
                    Code::E(773),
                    "a `native` routine cannot have `var` or `out` parameters",
                    span,
                );
            } else if !ty.is_error() && native_kind(&self.types, ty).is_none() {
                let name = self.types.name(ty);
                self.error(
                    Code::E(773),
                    format!("type `{name}` cannot be passed to a `native` routine"),
                    span,
                );
            }
        }
        if let Some(t) = ret
            && !t.is_error()
            && native_kind(&self.types, t).is_none()
        {
            let name = self.types.name(t);
            self.error(
                Code::E(773),
                format!("type `{name}` cannot be returned from a `native` routine"),
                at,
            );
        }
    }

    /// E0772: what an extension class (a class of an extension's unit with a `native` method)
    /// cannot be (`docs/spec/extensions.md` 2.2): it is `class sealed(TRefObject)` without
    /// fields, and all its methods are `native`. `at` is the class's name.
    pub(super) fn check_extension_class(&mut self, class: TypeId, at: TextRange) {
        let TypeKind::Class {
            parent,
            sealed,
            fields,
            methods,
            ..
        } = &self.types.get(class).kind
        else {
            return;
        };
        let natives = methods
            .iter()
            .filter_map(|m| m.routine)
            .filter(|r| self.routines[r.0 as usize].native)
            .count();
        if natives == 0 {
            return;
        }
        let shape_ok = *sealed && *parent == Some(TREFOBJECT);
        let has_fields = !fields.is_empty();
        let pascal: Vec<(String, TextRange)> = methods
            .iter()
            .filter_map(|m| m.routine)
            .map(|r| &self.routines[r.0 as usize])
            // One whose declaration has an error (a refused `native`) is reported already
            .filter(|r| !r.native && !r.implicit_forward)
            .map(|r| (r.name.clone(), r.name_span))
            .collect();
        if !shape_ok {
            self.error(
                Code::E(772),
                "a class with `native` methods must be declared as `class sealed(TRefObject)`",
                at,
            );
        }
        if has_fields {
            self.error(
                Code::E(772),
                "a class with `native` methods cannot declare fields",
                at,
            )
            .notes
            .push("its state is the Rust value its instances hold".into());
        }
        for (name, span) in pascal {
            self.error(
                Code::E(772),
                format!("method `{name}` of a class with `native` methods must be `native`"),
                span,
            );
        }
    }

    /// Whether `class` is an extension class: a class of an extension's unit with a `native`
    /// method (`docs/spec/extensions.md` 2.2).
    pub(super) fn is_extension_class(&self, class: TypeId) -> bool {
        match &self.types.get(class).kind {
            TypeKind::Class { methods, .. } => methods
                .iter()
                .filter_map(|m| m.routine)
                .any(|r| self.routines[r.0 as usize].native),
            _ => false,
        }
    }

    /// E0774: the `native` routine `routine` used as a value (it has no function to point to).
    /// Returns whether it was reported.
    pub(super) fn native_as_value(&mut self, routine: RoutineId, at: TextRange) -> bool {
        let r = &self.routines[routine.0 as usize];
        if !r.native {
            return false;
        }
        let name = r.name.clone();
        self.error(
            Code::E(774),
            format!("`native` routine `{name}` cannot be used as a value"),
            at,
        )
        .notes
        .push(
            "wrap it in an anonymous method, as in `function(…) begin Result := …(…) end`".into(),
        );
        true
    }
}
