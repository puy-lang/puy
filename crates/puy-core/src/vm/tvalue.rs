//! `TValue` (`docs/spec/rtti.md` chapter 3, `docs/spec/bytecode.md` 2.24): the intrinsics that
//! read a `TValue` by the entry of its type in the type information section, and `Format` of an
//! array of `TValue` (4.2).

use super::*;
use crate::bytecode::rtti::{
    FT_CURR, FT_DOUBLE, FT_SINGLE, RttiExt, RttiTypeEntry, SHAPE_CHAR, SHAPE_ENUM, TK_ARRAY,
    TK_CLASS, TK_CLASSREF, TK_ENUMERATION, TK_FLOAT, TK_INT64, TK_INTEGER, TK_INTERFACE,
    TK_LSTRING, TK_METHOD, TK_POINTER, TK_PROCEDURE, TK_SET, TK_USTRING, TK_WCHAR,
};

/// The modes of `ValueRead` (`I123`).
const READ_INTEGER: i64 = 0;
const READ_INT64: i64 = 1;
const READ_ORDINAL: i64 = 2;
const READ_BOOLEAN: i64 = 3;
const READ_DOUBLE: i64 = 4;
const READ_CURRENCY: i64 = 5;
const READ_EMPTY: i64 = 6;

/// The modes of `ValueArray` (`I128`).
const ARRAY_LENGTH: i64 = 0;
const ARRAY_TYPE: i64 = 1;
const ARRAY_BITS: i64 = 2;
const ARRAY_REF: i64 = 3;
const ARRAY_ELEM_TYPE: i64 = 4;
const ARRAY_SET: i64 = 5;
const ARRAY_SET_VALUE: i64 = 6;

/// The `UInt64` width code of an ordinal type entry (`docs/spec/bytecode.md` 2.2).
const SHAPE_UINT64: u8 = 7;

/// The fully qualified name of `Boolean`'s type entry.
const BOOLEAN_NAME: &str = "System.Boolean";

/// A value of a `TValue` as its three slots (`docs/spec/rtti.md` 3.1).
#[derive(Clone, Copy)]
pub(super) struct Value {
    ty: i64,
    bits: i64,
    r: i64,
}

impl Value {
    /// The registers `rB`, `r(B+1)`, `r(B+2)`.
    pub(super) fn at(regs: &[Slot], rb: usize) -> Value {
        Value {
            ty: regs[rb],
            bits: regs[rb + 1],
            r: regs[rb + 2],
        }
    }
}

/// A value of a slot (an element of an array, a field) as a `TValue` would hold it: its type
/// information number, and its bits or its reference (not counted).
pub(super) struct Elem {
    pub(super) ty: i64,
    pub(super) bits: i64,
    pub(super) r: i64,
}

impl Isolate {
    /// The entry of the type information number `n` (`None` for 0 and outside the section).
    pub(super) fn rtti_entry(&self, n: i64) -> Option<&RttiTypeEntry> {
        let k = usize::try_from(n).ok()?.checked_sub(1)?;
        self.module.rtti.as_ref()?.types.get(k)
    }

    /// The name of the type information number `n` (`TRttiType.Name`; `docs/spec/rtti.md` 2.4).
    pub(super) fn rtti_short_name(&self, n: i64) -> String {
        match (self.module.rtti.as_ref(), self.rtti_entry(n)) {
            (Some(s), Some(e)) => s.name(e.name).to_string(),
            _ => String::new(),
        }
    }

    /// The fully qualified name of the type information number `n`.
    pub(super) fn rtti_full_name(&self, n: i64) -> &str {
        match (self.module.rtti.as_ref(), self.rtti_entry(n)) {
            (Some(s), Some(e)) => s.name(e.full_name),
            _ => "",
        }
    }

    /// The `TTypeKind` ordinal of the type information number `n` (`TypeKindOf`; 0 for 0).
    pub(super) fn type_kind_of(&self, n: i64) -> i64 {
        self.rtti_entry(n).map_or(0, |e| i64::from(e.kind))
    }

    /// The type information number of the class `ty` of the type section, or of its nearest
    /// ancestor with one (`ValueOfObject`; `docs/spec/rtti.md` 7.1): the table is made when first
    /// used.
    pub(super) fn class_info_number(&mut self, ty: u32) -> Option<i64> {
        let module = &self.module;
        let table = self.rtti_classes.get_or_insert_with(|| {
            let mut t = std::collections::HashMap::new();
            if let Some(s) = &module.rtti {
                for (k, e) in s.types.iter().enumerate() {
                    if e.kind == TK_CLASS && e.type_index != bytecode::NONE_INDEX {
                        t.insert(e.type_index, k as i64 + 1);
                    }
                }
            }
            t
        });
        let mut cur = Some(ty);
        while let Some(c) = cur {
            if let Some(n) = table.get(&c) {
                return Some(*n);
            }
            cur = self.heap.type_entry(c).and_then(|e| e.parent);
        }
        None
    }

    /// `ValueOfObject` (`I127`): the type information number of the class of the instance `h`,
    /// `fallback` (the static type's) for `nil`.
    pub(super) fn value_of_object(&mut self, h: Slot, fallback: i64) -> Result<i64, RefError> {
        if h == 0 {
            return Ok(fallback);
        }
        let class = self.heap.obj_class(h)?;
        Ok(self.class_info_number(class).unwrap_or(fallback))
    }

    /// The `EInvalidCast` of a `TValue` of the type `from` taken as `to` (`docs/spec/rtti.md`
    /// chapter 6).
    pub(super) fn value_cast_error(&self, from: i64, to: &str) -> RefError {
        let message = if from == 0 {
            format!("invalid type cast: empty `TValue` to `{to}`")
        } else {
            format!(
                "invalid type cast: `TValue` of `{}` to `{to}`",
                self.rtti_short_name(from)
            )
        };
        RefError::Rt(rt(exc::INVALID_CAST, message))
    }

    /// Whether the entry `n` is `Boolean` or a type made from it (a subrange, a distinct type).
    fn is_boolean_entry(&self, mut n: i64) -> bool {
        for _ in 0..8 {
            if self.rtti_full_name(n) == BOOLEAN_NAME {
                return true;
            }
            match self.rtti_entry(n).map(|e| &e.ext) {
                Some(RttiExt::Ordinal { base, .. }) if *base != 0 => n = i64::from(*base),
                _ => return false,
            }
        }
        false
    }

    /// `ValueMatch` (`I124`): whether `AsType<T>` of a `TValue` of the type `ty` whose instance or
    /// class reference is `h` holds for the type `target` (`docs/spec/rtti.md` 3.5).
    pub(super) fn value_match(&mut self, ty: i64, h: Slot, target: i64) -> Result<bool, RefError> {
        if ty == target && ty != 0 {
            return Ok(true);
        }
        let Some(t) = self.rtti_entry(target) else {
            return Ok(false);
        };
        let (t_kind, t_index, t_ext) = (t.kind, t.type_index, t.ext.clone());
        if ty == 0 {
            // The empty value is `nil` of the reference types (6)
            return Ok(matches!(
                t_kind,
                TK_CLASS | TK_INTERFACE | TK_CLASSREF | TK_PROCEDURE | TK_METHOD
            ));
        }
        let v_kind = self.type_kind_of(ty) as u8;
        Ok(match t_kind {
            TK_CLASS if v_kind == TK_CLASS => {
                h == 0 || {
                    let class = self.heap.obj_class(h)?;
                    self.heap.class_inherits(class, t_index)
                }
            }
            TK_INTERFACE if v_kind == TK_INTERFACE => {
                h == 0 || {
                    let class = self.heap.obj_class(h)?;
                    self.heap.class_implements(class, t_index)
                }
            }
            TK_CLASSREF if v_kind == TK_CLASSREF => {
                let RttiExt::ClassRef { class } = t_ext else {
                    return Ok(false);
                };
                let target_class = self
                    .rtti_entry(i64::from(class))
                    .map_or(bytecode::NONE_INDEX, |e| e.type_index);
                h == 0 || {
                    let c = match u32::try_from(h - 1) {
                        Ok(c) => c,
                        Err(_) => return Err(RefError::Fatal(format!("bad class reference {h}"))),
                    };
                    self.heap.class_inherits(c, target_class)
                }
            }
            _ => false,
        })
    }

    /// `ValueRead` (`I123`): the named reads of a `TValue` (`docs/spec/rtti.md` 3.5), and whether
    /// it is empty (3.2).
    pub(super) fn value_read(&self, v: Value, mode: i64) -> Result<Slot, RefError> {
        if mode == READ_EMPTY {
            let empty = v.ty == 0
                || (matches!(
                    self.type_kind_of(v.ty) as u8,
                    TK_CLASS | TK_CLASSREF | TK_INTERFACE | TK_POINTER
                ) && v.bits == 0
                    && v.r == 0);
            return Ok(Slot::from(empty));
        }
        let target = match mode {
            READ_INTEGER => "Integer",
            READ_INT64 | READ_ORDINAL => "Int64",
            READ_BOOLEAN => "Boolean",
            READ_DOUBLE => "Extended",
            READ_CURRENCY => "Currency",
            m => return Err(RefError::Fatal(format!("bad `ValueRead` mode {m}"))),
        };
        let fail = || self.value_cast_error(v.ty, target);
        let Some(e) = self.rtti_entry(v.ty) else {
            return Err(fail());
        };
        // An integer as an `i128` (`UInt64` by its bits)
        let int = match (&e.ext, e.kind) {
            (RttiExt::Ordinal { shape, .. }, TK_INTEGER | TK_INT64) => {
                Some(if *shape == SHAPE_UINT64 {
                    i128::from(v.bits as u64)
                } else {
                    i128::from(v.bits)
                })
            }
            _ => None,
        };
        match mode {
            READ_INTEGER => match int {
                Some(n) if i32::try_from(n).is_ok() => Ok(n as i64),
                _ => Err(fail()),
            },
            READ_INT64 => match int {
                Some(n) if i64::try_from(n).is_ok() => Ok(n as i64),
                _ => Err(fail()),
            },
            READ_ORDINAL => match e.kind {
                TK_INTEGER | TK_INT64 | TK_WCHAR | TK_ENUMERATION => Ok(v.bits),
                _ => Err(fail()),
            },
            READ_BOOLEAN if self.is_boolean_entry(v.ty) => Ok(v.bits),
            READ_DOUBLE => {
                let x = match (&e.ext, int) {
                    (_, Some(n)) => numfmt::int_to_f64(n),
                    (RttiExt::Float(FT_SINGLE), _) => f64::from(slot_f32(v.bits)),
                    (RttiExt::Float(FT_DOUBLE), _) => slot_f64(v.bits),
                    (RttiExt::Float(FT_CURR), _) => numfmt::curr_to_f64(v.bits),
                    _ => return Err(fail()),
                };
                Ok(f64_slot(x))
            }
            READ_CURRENCY => match (&e.ext, int) {
                (_, Some(n)) => numfmt::int_to_curr(n).ok_or_else(fail),
                (RttiExt::Float(FT_CURR), _) => Ok(v.bits),
                _ => Err(fail()),
            },
            _ => Err(fail()),
        }
    }

    /// `ValueReadString` (`I125`): `AsString` (`docs/spec/rtti.md` 3.5) into `rA`.
    pub(super) fn value_read_string(&mut self, ra: usize, v: Value) -> Result<(), RefError> {
        match self.type_kind_of(v.ty) as u8 {
            TK_USTRING => Ok(self.put_shared(ra, v.r)?),
            TK_LSTRING => {
                let text = String::from_utf8_lossy(self.heap.get_u8(v.r)?).into_owned();
                self.put_text(ra, &text)
            }
            TK_WCHAR => {
                let s = PStr::from_units(&[v.bits as u16]).map_err(|_| RefError::Rt(oom()))?;
                let x = self.alloc_string(s).map_err(RefError::Rt)?;
                Ok(self.put(ra, x)?)
            }
            _ => Err(self.value_cast_error(v.ty, "string")),
        }
    }

    /// Writes a new string of `text` into `rA`.
    fn put_text(&mut self, ra: usize, text: &str) -> Result<(), RefError> {
        let s = PStr::from_text(text).map_err(|_| RefError::Rt(oom()))?;
        let x = self.alloc_string(s).map_err(RefError::Rt)?;
        Ok(self.put(ra, x)?)
    }

    /// The name of the value `ord` of the enumeration entry `n` (or of the type it is made from).
    fn enum_value_name(&self, mut n: i64, ord: i64) -> Option<String> {
        let s = self.module.rtti.as_ref()?;
        for _ in 0..8 {
            match &self.rtti_entry(n)?.ext {
                RttiExt::Ordinal { values, base, .. } => {
                    if let Some((_, name)) = values.iter().find(|(o, _)| *o == ord) {
                        return Some(s.name(*name).to_string());
                    }
                    if *base == 0 {
                        return None;
                    }
                    n = i64::from(*base);
                }
                _ => return None,
            }
        }
        None
    }

    /// The text of the value `bits` of the ordinal entry `n` (`ToString`, the elements of a set).
    fn ordinal_text(&self, n: i64, bits: i64) -> String {
        match self.rtti_entry(n).map(|e| (&e.ext, e.kind)) {
            Some((RttiExt::Ordinal { shape, .. }, _)) if *shape == SHAPE_CHAR => {
                String::from_utf16_lossy(&[bits as u16])
            }
            Some((RttiExt::Ordinal { shape, .. }, _)) if *shape == SHAPE_ENUM => self
                .enum_value_name(n, bits)
                .unwrap_or_else(|| bits.to_string()),
            Some((RttiExt::Ordinal { shape, .. }, _)) if *shape == SHAPE_UINT64 => {
                (bits as u64).to_string()
            }
            _ => bits.to_string(),
        }
    }

    /// `ValueToString` (`I122`): `TValue.ToString` (`docs/spec/rtti.md` 3.7) into `rA`.
    pub(super) fn value_to_string(&mut self, ra: usize, v: Value) -> Result<(), RefError> {
        const EMPTY: &str = "(empty)";
        let Some(e) = self.rtti_entry(v.ty) else {
            return self.put_text(ra, EMPTY);
        };
        let (kind, ext) = (e.kind, e.ext.clone());
        let text = match (kind, &ext) {
            (TK_USTRING, _) => return Ok(self.put_shared(ra, v.r)?),
            (TK_INTEGER | TK_INT64 | TK_WCHAR | TK_ENUMERATION, _) => {
                self.ordinal_text(v.ty, v.bits)
            }
            (TK_LSTRING, _) => String::from_utf8_lossy(self.heap.get_u8(v.r)?).into_owned(),
            (TK_FLOAT, RttiExt::Float(FT_SINGLE)) => numfmt::f32_to_str(slot_f32(v.bits)),
            (TK_FLOAT, RttiExt::Float(FT_CURR)) => numfmt::curr_to_str(v.bits),
            (TK_FLOAT, _) => numfmt::f64_to_str(slot_f64(v.bits)),
            (TK_SET, RttiExt::Set { base, bits }) => {
                let base = i64::from(*base);
                let words: Vec<i64> = if *bits <= 64 {
                    vec![v.bits]
                } else {
                    self.heap.set_slots(v.r)?.to_vec()
                };
                let mut items = Vec::new();
                for (w, word) in words.iter().enumerate() {
                    for b in 0..64 {
                        if (*word as u64) >> b & 1 != 0 {
                            items.push(self.ordinal_text(base, (w * 64 + b) as i64));
                        }
                    }
                }
                format!("[{}]", items.join(","))
            }
            (TK_CLASS, _) if v.bits == 0 && v.r == 0 => EMPTY.to_string(),
            (TK_CLASS, _) => format!("({})", self.rtti_short_name(v.ty)),
            (TK_CLASSREF, _) if v.bits == 0 => EMPTY.to_string(),
            (TK_CLASSREF, _) => {
                let ty = u32::try_from(v.bits - 1)
                    .map_err(|_| RefError::Fatal(format!("bad class reference {}", v.bits)))?;
                class_decl_name(self.heap.type_name(ty)).to_string()
            }
            _ => format!("({})", self.rtti_short_name(v.ty)),
        };
        self.put_text(ra, &text)
    }

    /// The `Format` kind of a `TValue` of the type `n` (`docs/spec/rtti.md` 4.2, the kinds of
    /// `docs/spec/bytecode.md` 2.16), and the slot holding its value; `None` for a type `Format`
    /// does not take.
    fn format_kind_of(&self, v: Value) -> Option<(i64, Slot)> {
        let e = self.rtti_entry(v.ty)?;
        Some(match (e.kind, &e.ext) {
            (TK_INTEGER | TK_INT64, RttiExt::Ordinal { shape, .. }) => (i64::from(*shape), v.bits),
            (TK_WCHAR, _) => (bytecode::FORMAT_KIND_CHAR, v.bits),
            (TK_ENUMERATION, _) if self.is_boolean_entry(v.ty) => {
                (bytecode::FORMAT_KIND_BOOLEAN, v.bits)
            }
            (TK_USTRING, _) => (bytecode::FORMAT_KIND_STRING, v.r),
            (TK_LSTRING, _) => (bytecode::FORMAT_KIND_UTF8, v.r),
            (TK_FLOAT, RttiExt::Float(FT_SINGLE)) => (bytecode::FORMAT_KIND_SINGLE, v.bits),
            (TK_FLOAT, RttiExt::Float(FT_DOUBLE)) => (bytecode::FORMAT_KIND_DOUBLE, v.bits),
            (TK_FLOAT, RttiExt::Float(FT_CURR)) => (bytecode::FORMAT_KIND_CURRENCY, v.bits),
            _ => return None,
        })
    }

    /// `FormatValues` (`I121`): `Format(rB, r(B+1))` of an array of `TValue` into `rA`
    /// (`docs/spec/rtti.md` 4.2): each element as its `Format` kind and value, then the same
    /// formatting as `Format` (248).
    pub(super) fn format_values(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let (fmt, arr) = (self.task.regs[rb], self.task.regs[rb + 1]);
        let elems: Vec<Value> = if arr == 0 {
            Vec::new()
        } else {
            let (_, slots) = self.heap.any_slots(arr)?;
            let (chunks, _) = slots.as_chunks::<3>();
            chunks
                .iter()
                .map(|c| Value {
                    ty: c[0],
                    bits: c[1],
                    r: c[2],
                })
                .collect()
        };
        let mut pairs = Vec::with_capacity(elems.len());
        for v in elems {
            match self.format_kind_of(v) {
                Some(p) => pairs.push(p),
                None => {
                    let text = String::from_utf16_lossy(
                        &self
                            .heap
                            .get(fmt)?
                            .units()
                            .map_err(|_| RefError::Rt(oom()))?,
                    );
                    return Err(RefError::Rt(format_error(text)));
                }
            }
        }
        self.format_into(ra, fmt, FormatArgs::Pairs(&pairs))
    }

    /// The type entry, length, element width and element type of the array a `TValue` of the
    /// type `ty` holds in `r` (`EInvalidCast` when it is not an array).
    fn value_array_shape(&self, ty: i64, r: Slot) -> Result<(usize, usize, i64), RefError> {
        let Some(e) = self.rtti_entry(ty) else {
            return Err(self.value_cast_error(ty, "array"));
        };
        match e.ext {
            RttiExt::Array {
                elem, width, count, ..
            } => Ok((count as usize, width.max(1) as usize, i64::from(elem))),
            RttiExt::DynArray { elem } => {
                let (len, w) = if r == 0 {
                    (0, 1)
                } else {
                    let t = self.heap.agg_type(r)?;
                    (self.heap.len(r)?, self.heap.width_of(t))
                };
                Ok((len, w, i64::from(elem)))
            }
            _ => Err(self.value_cast_error(ty, "array")),
        }
    }

    /// The type index of a record or static array element laid out flat in its array (`None`
    /// for any other element).
    pub(super) fn flat_elem_type(&self, elem: i64) -> Option<u32> {
        let e = self.rtti_entry(elem)?;
        let index = e.type_index;
        let t = self.heap.type_entry(index)?;
        match t.flat_width() {
            Some(w) if (1..=u64::from(bytecode::MAX_FLAT_WIDTH)).contains(&w) => Some(index),
            _ => None,
        }
    }

    /// The value `v` of a slot of the kind `kind` holding a value of the type `elem` (not laid
    /// out flat) as a `TValue` holds it (`docs/spec/rtti.md` 3.1): a counted value in `FRef`, any
    /// other in `FBits`; an instance with the type of its class (3.3).
    pub(super) fn slot_value(&mut self, v: Slot, kind: u8, elem: i64) -> Result<Elem, RefError> {
        let counted = match kind {
            bytecode::SLOT_REF => true,
            bytecode::SLOT_OBJ => self.heap.obj_counted(v).unwrap_or(false),
            _ => false,
        };
        Ok(Elem {
            ty: if self.type_kind_of(elem) as u8 == TK_CLASS {
                self.value_of_object(v, elem)?
            } else {
                elem
            },
            bits: if counted { 0 } else { v },
            r: if counted { v } else { 0 },
        })
    }

    /// Element `i` of the array `r` (the value of a `TValue` of an array type, elements of the type
    /// `elem` and the width `w`) as a `TValue` holds it (`docs/spec/rtti.md` 3.6), into `rA` by
    /// `mode`.
    fn value_array_get(
        &mut self,
        ra: usize,
        r: Slot,
        i: usize,
        w: usize,
        elem: i64,
        mode: i64,
    ) -> Result<(), RefError> {
        let flat = self.flat_elem_type(elem);
        if let Some(index) = flat {
            // A box of the element's slots (`AggLoad`)
            return match mode {
                ARRAY_TYPE => {
                    self.task.regs[ra] = elem;
                    Ok(())
                }
                ARRAY_BITS => {
                    self.task.regs[ra] = 0;
                    Ok(())
                }
                _ => self.agg_load(ra, (r != 0).then_some(r), i * w, index, None),
            };
        }
        let (v, kind) = if r == 0 {
            (0, bytecode::SLOT_PLAIN)
        } else {
            let t = self.heap.agg_type(r)?;
            (self.heap.any_slot(r, i * w)?, self.heap.slot_kind(t, 0))
        };
        let e = self.slot_value(v, kind, elem)?;
        match mode {
            ARRAY_TYPE => self.task.regs[ra] = e.ty,
            ARRAY_BITS => self.task.regs[ra] = e.bits,
            _ => self.put_shared(ra, e.r)?,
        }
        Ok(())
    }

    /// `ValueArray` (`I128`): the array members of `TValue` (`docs/spec/rtti.md` 3.6).
    pub(super) fn value_array(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let regs = &self.task.regs;
        let (ty, r, index, mode, value) = (
            regs[rb],
            regs[rb + 1],
            regs[rb + 2],
            regs[rb + 3],
            regs[rb + 4],
        );
        let (len, w, elem) = self.value_array_shape(ty, r)?;
        if mode == ARRAY_LENGTH {
            self.task.regs[ra] = len as i64;
            return Ok(());
        }
        let i = match usize::try_from(index) {
            Ok(i) if i < len => i,
            _ => {
                return Err(RefError::Rt(rt(
                    exc::RANGE_ERROR,
                    dyn_index_message(index, len),
                )));
            }
        };
        match mode {
            ARRAY_ELEM_TYPE => {
                self.task.regs[ra] = elem;
                Ok(())
            }
            ARRAY_TYPE | ARRAY_BITS | ARRAY_REF => self.value_array_get(ra, r, i, w, elem, mode),
            // The value is a box of the element's type (a `TValue` element)
            ARRAY_SET | ARRAY_SET_VALUE => self.value_array_set(ra, ty, r, i, w, elem, value),
            m => Err(RefError::Fatal(format!("bad `ValueArray` mode {m}"))),
        }
    }

    /// `SetArrayElement` (`docs/spec/rtti.md` 3.6): writes `value` (the bits or the reference of
    /// a `TValue` whose type was matched) at element `i`, and puts the array into `rA`: a static
    /// array is copied first (the `TValue` it was read from keeps its value until it is written
    /// back), a dynamic array is written while shared.
    #[allow(clippy::too_many_arguments)]
    fn value_array_set(
        &mut self,
        ra: usize,
        ty: i64,
        r: Slot,
        i: usize,
        w: usize,
        elem: i64,
        value: Slot,
    ) -> Result<(), RefError> {
        let is_static = self.type_kind_of(ty) as u8 == TK_ARRAY;
        let target = if is_static {
            let index = self
                .rtti_entry(ty)
                .map_or(bytecode::NONE_INDEX, |e| e.type_index);
            if index == bytecode::NONE_INDEX {
                return Err(RefError::Fatal(format!(
                    "the static array type {ty} is not in the type section"
                )));
            }
            if r != 0 {
                // A copy: counted once more, so that `Unique` copies it whoever else holds it
                self.heap.add_ref(r)?;
            }
            match self.heap.unique(r, index)? {
                Ok(n) => n,
                Err(OutOfMemory) => {
                    // The reference counted for the copy goes back
                    if r != 0 {
                        self.heap.release(r)?;
                    }
                    return Err(RefError::Rt(oom()));
                }
            }
        } else {
            self.heap.add_ref(r)?;
            r
        };
        let written = (|| -> Result<(), RefError> {
            if let Some(index) = self.flat_elem_type(elem) {
                return self.agg_store(target, i * w, index, value, None);
            }
            let t = self.heap.agg_type(target)?;
            let kind = self.heap.slot_kind(t, 0);
            self.heap.add_ref_kind(value, kind)?;
            let old = self.heap.any_replace(target, i * w, value)?;
            self.heap.release_kind(old, kind)?;
            Ok(())
        })();
        if let Err(e) = written {
            self.heap.release(target)?;
            return Err(e);
        }
        Ok(self.put(ra, target)?)
    }
}

/// The `EConvertError` of a format the arguments do not match (`Format`, `FormatValues`).
pub(super) fn format_error(text: String) -> RtError {
    convert_error(text, |t| {
        format!("format '{t}' invalid or incompatible with argument")
    })
}
