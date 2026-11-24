//! `System.Rtti` (`docs/spec/rtti.md` chapter 5, `docs/spec/bytecode.md` 2.24): the fields and
//! the properties read through their fields, read and written by the entries of the type
//! information section (`RttiFieldGet`, `RttiFieldSet`), and the class of a class reference
//! (`RttiClassType`).

use super::tvalue::Elem;
use super::*;
use crate::bytecode::rtti::{
    PROP_READABLE, PROP_WRITABLE, RTTI_REF_COUNTED, RttiExt, RttiTarget, TABLE_FIELDS,
    TABLE_METHODS, TABLE_PROPS, TARGET_FIELD, TARGET_FUNC, TARGET_VIRTUAL, TK_CLASS, TK_CLASSREF,
    TK_INTERFACE, TK_RECORD,
};

/// The slot of a `TValue` holding a managed value (`FRef`; `docs/spec/rtti.md` 3.1).
const VALUE_REF: usize = 2;

/// The fully qualified name of `TValue`'s type entry: a field of this type is read and written as
/// the `TValue` it holds, not nested (`docs/spec/rtti.md` 3.3).
const TVALUE_NAME: &str = "System.TValue";

/// The modes of `RttiClassType` (`I135`).
const CLASS_TO_INFO: i64 = 0;
const INFO_TO_CLASS: i64 = 1;

/// The place a field or a property reads or writes: the type information numbers of the type
/// declaring it and of its value, its first slot and its width.
struct Place {
    owner: i64,
    ty: i64,
    pos: usize,
    width: usize,
}

/// A `TValue` read from its block: its three slots.
#[derive(Clone, Copy)]
struct Parts {
    ty: i64,
    bits: Slot,
    r: Slot,
}

impl Isolate {
    /// The three slots of the `TValue` block `v` (0 is the empty value).
    fn value_parts(&self, v: Slot) -> Result<Parts, RefError> {
        if v == 0 {
            return Ok(Parts {
                ty: 0,
                bits: 0,
                r: 0,
            });
        }
        let (_, slots) = self.heap.any_slots(v)?;
        match slots {
            [ty, bits, r] => Ok(Parts {
                ty: *ty,
                bits: *bits,
                r: *r,
            }),
            _ => Err(RefError::Fatal(format!(
                "heap block {v} is not a `TValue` ({} slots)",
                slots.len()
            ))),
        }
    }

    /// The type information number of the record or class declaring each field and property,
    /// made when first used.
    fn rtti_owner(&mut self, table: u8, index: usize) -> i64 {
        let module = &self.module;
        let owners = self.rtti_owners.get_or_insert_with(|| {
            let mut fields = Vec::new();
            let mut props = Vec::new();
            let mut methods = Vec::new();
            if let Some(s) = &module.rtti {
                fields.resize(s.fields.len(), 0);
                props.resize(s.props.len(), 0);
                methods.resize(s.methods.len(), 0);
                for (k, e) in s.types.iter().enumerate() {
                    let n = k as i64 + 1;
                    if let RttiExt::Intf { methods: m, .. } = &e.ext {
                        let (first, count) = (m.0 as usize, m.1 as usize);
                        if let Some(slots) = methods.get_mut(first..first + count) {
                            slots.fill(n);
                        }
                    }
                    if let RttiExt::Struct {
                        fields: f,
                        props: p,
                        methods: m,
                        ..
                    } = &e.ext
                    {
                        for (table, (first, count)) in
                            [(&mut fields, f), (&mut props, p), (&mut methods, m)]
                        {
                            let (first, count) = (*first as usize, *count as usize);
                            if let Some(slots) = table.get_mut(first..first + count) {
                                slots.fill(n);
                            }
                        }
                    }
                }
            }
            [fields, props, methods]
        });
        owners
            .get(usize::from(table))
            .and_then(|t| t.get(index))
            .copied()
            .unwrap_or(0)
    }

    /// The place the field (`table` 0) or property (`table` 1) `index` reads (`write` false) or
    /// writes: a property must have the accessor, and the accessor must be a field
    /// (`EInvalidOperation`; `docs/spec/rtti.md` 5.6).
    fn rtti_place(&mut self, table: i64, index: i64, write: bool) -> Result<Place, RefError> {
        let bad = || RefError::Fatal(format!("bad member {index} of the table {table}"));
        let s = self.module.rtti.as_ref().ok_or_else(bad)?;
        let i = usize::try_from(index).map_err(|_| bad())?;
        let (table, place) = match u8::try_from(table) {
            Ok(TABLE_FIELDS) => {
                let f = s.fields.get(i).ok_or_else(bad)?;
                let place = (i64::from(f.ty), f.pos as usize, usize::from(f.width));
                (TABLE_FIELDS, place)
            }
            Ok(TABLE_PROPS) => {
                let p = s.props.get(i).ok_or_else(bad)?;
                let name = s.name(p.name);
                let (flag, target, what) = if write {
                    (PROP_WRITABLE, &p.write, "writable")
                } else {
                    (PROP_READABLE, &p.read, "readable")
                };
                if p.flags & flag == 0 {
                    return Err(RefError::Rt(rt(
                        exc::INVALID_OPERATION,
                        format!("property `{name}` is not {what}"),
                    )));
                }
                if target.kind != TARGET_FIELD {
                    // `System.Rtti` calls the method of such a property through its bridge
                    // (`RttiPropRef`; `docs/spec/rtti.md` 5.6)
                    return Err(RefError::Fatal(format!(
                        "property `{name}` is not read or written through a field"
                    )));
                }
                let place = (
                    i64::from(p.ty),
                    target.value as usize,
                    usize::from(target.width),
                );
                (TABLE_PROPS, place)
            }
            _ => return Err(bad()),
        };
        let (ty, pos, width) = place;
        Ok(Place {
            owner: self.rtti_owner(table, i),
            ty,
            pos,
            width,
        })
    }

    /// The block holding the fields of the instance the `TValue` `inst` holds, checked against
    /// the type `owner` declaring the member (`docs/spec/rtti.md` 5.6): an instance of `owner` or
    /// of a descendant, or a record of the type `owner` (`None` for one at its default value).
    /// The empty value, `nil` and a destroyed instance are `EAccessViolation`, another type
    /// `EInvalidCast`.
    fn rtti_instance(&self, inst: Parts, owner: i64) -> Result<Option<heap::StrRef>, RefError> {
        let Some(o) = self.rtti_entry(owner) else {
            return Err(RefError::Fatal(format!("bad declaring type {owner}")));
        };
        let (o_kind, o_index) = (o.kind, o.type_index);
        if inst.ty == 0 {
            return Err(heap::BadHandle::Nil.into());
        }
        let mismatch = || self.value_cast_error(inst.ty, &self.rtti_short_name(owner));
        match o_kind {
            TK_CLASS if self.type_kind_of(inst.ty) as u8 == TK_CLASS => {
                let index = self.heap.obj_index(inst.bits | inst.r)?;
                let class = self.heap.obj_class(inst.bits | inst.r)?;
                if !self.heap.class_inherits(class, o_index) {
                    return Err(mismatch());
                }
                Ok(Some(index as heap::StrRef + 1))
            }
            TK_RECORD if inst.ty == owner => Ok((inst.r != 0).then_some(inst.r)),
            _ => Err(mismatch()),
        }
    }

    /// Whether the slot kind `kind` of a field of the type `ty` is that of a `[weak]` field: an
    /// instance of a reference counted class or an interface kept without counting it
    /// (`docs/spec/objects.md` 4.4).
    fn is_weak_slot(&self, kind: u8, ty: i64) -> bool {
        kind == bytecode::SLOT_PLAIN
            && self.rtti_entry(ty).is_some_and(|e| {
                e.kind == TK_INTERFACE || (e.kind == TK_CLASS && e.flags & RTTI_REF_COUNTED != 0)
            })
    }

    /// `RttiFieldGet` (`I133`): `rA :=` a new `TValue` holding the value of the field or property
    /// `r(B+1)` of the table `rB` of the instance the `TValue` `r(B+2)` holds
    /// (`docs/spec/rtti.md` 5.6). A record or static array laid out flat is copied into a box; a
    /// field of the type `TValue` gives that `TValue` (not nested).
    pub(super) fn rtti_field_get(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let (table, index, v) = (
            self.task.regs[rb],
            self.task.regs[rb + 1],
            self.task.regs[rb + 2],
        );
        let place = self.rtti_place(table, index, false)?;
        let inst = self.value_parts(v)?;
        let block = self.rtti_instance(inst, place.owner)?;
        // The type of the `TValue` blocks (the instance's is one, not empty here)
        let tvalue = self.heap.agg_type(v)?;
        if let Some(ty) = self.flat_elem_type(place.ty) {
            // A box of the field's slots (`AggLoad`): the `TValue` itself, or its `FRef`
            self.agg_load(ra, block, place.pos, ty, None)?;
            if self.rtti_full_name(place.ty) == TVALUE_NAME {
                return Ok(());
            }
            let r = std::mem::take(&mut self.task.regs[ra]);
            let e = Elem {
                ty: place.ty,
                bits: 0,
                r,
            };
            return self.put_value(ra, tvalue, e, false);
        }
        if place.width != 1 {
            return Err(RefError::Fatal(format!(
                "field of {} slots not laid out flat",
                place.width
            )));
        }
        let (v, kind) = match block {
            None => (0, bytecode::SLOT_PLAIN),
            Some(b) => {
                let t = self.heap.any_slots(b)?.0;
                self.heap.check_slot(b, place.pos as i64)?;
                (
                    self.heap.any_slot(b, place.pos)?,
                    self.heap.slot_kind(t, place.pos),
                )
            }
        };
        let e = if self.is_weak_slot(kind, place.ty) {
            // `LoadWeak`: `nil` once the instance is gone
            let h = self.heap.weak_target(v)?;
            self.slot_value(h, bytecode::SLOT_OBJ, place.ty)?
        } else {
            self.slot_value(v, kind, place.ty)?
        };
        if kind == bytecode::SLOT_REF {
            // Sharing a record or static array holding a lent location (`ObjGetRef`)
            self.heap.check_share(e.r)?;
        }
        self.put_value(ra, tvalue, e, true)
    }

    /// `rA :=` a new `TValue` block (of the type `tvalue`) holding `e`; its reference is counted
    /// first when `count` (otherwise it already holds one, which the block takes).
    fn put_value(&mut self, ra: usize, tvalue: u32, e: Elem, count: bool) -> Result<(), RefError> {
        let kind = self.heap.slot_kind(tvalue, VALUE_REF);
        if count {
            self.heap.add_ref_kind(e.r, kind)?;
        }
        let mut slots = Vec::new();
        let made = heap::try_extend(&mut slots, &[e.ty, e.bits, e.r])
            .and_then(|()| self.heap.alloc_agg(tvalue, slots));
        match made {
            Ok(b) => Ok(self.put(ra, b)?),
            Err(OutOfMemory) => {
                self.heap.release_kind(e.r, kind)?;
                Err(OutOfMemory.into())
            }
        }
    }

    /// `RttiFieldSet` (`I134`): writes the `TValue` `r(B+3)` into the field or property `r(B+1)`
    /// of the table `rB` of the instance the `TValue` `r(B+2)` holds (`docs/spec/rtti.md` 5.6),
    /// and `rA :=` the instance's `TValue` after the write: the same block for an instance, which
    /// is written in place; a new one for a record, whose box is copied first (the `TValue` given
    /// keeps its value). A value whose type does not match the field's is `EInvalidCast`.
    pub(super) fn rtti_field_set(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let (table, index, v, x) = (
            self.task.regs[rb],
            self.task.regs[rb + 1],
            self.task.regs[rb + 2],
            self.task.regs[rb + 3],
        );
        let place = self.rtti_place(table, index, true)?;
        let inst = self.value_parts(v)?;
        let block = self.rtti_instance(inst, place.owner)?;
        let value = self.value_parts(x)?;
        let tvalue = self.heap.agg_type(v)?;
        let is_tvalue = self.rtti_full_name(place.ty) == TVALUE_NAME;
        if !is_tvalue && !self.value_match(value.ty, value.bits | value.r, place.ty)? {
            let to = self.rtti_short_name(place.ty);
            return Err(self.value_cast_error(value.ty, &to));
        }
        let record = self.type_kind_of(inst.ty) as u8 == TK_RECORD;
        let target = if record {
            // A copy of the record's box: counted once more, so that `Unique` copies it
            let index = self
                .rtti_entry(inst.ty)
                .map_or(bytecode::NONE_INDEX, |e| e.type_index);
            let r = block.unwrap_or(0);
            if r != 0 {
                self.heap.add_ref(r)?;
            }
            match self.heap.unique(r, index)? {
                Ok(n) => n,
                Err(OutOfMemory) => {
                    if r != 0 {
                        self.heap.release(r)?;
                    }
                    return Err(OutOfMemory.into());
                }
            }
        } else {
            block.ok_or_else(|| RefError::Fatal("an instance without a block".into()))?
        };
        let written = if is_tvalue {
            self.agg_store(target, place.pos, tvalue, x, None)
        } else {
            self.rtti_store(target, &place, value.bits | value.r)
        };
        if !record {
            written?;
            return Ok(self.put_shared(ra, v)?);
        }
        if let Err(e) = written {
            self.heap.release(target)?;
            return Err(e);
        }
        let e = Elem {
            ty: inst.ty,
            bits: 0,
            r: target,
        };
        self.put_value(ra, tvalue, e, false)
    }

    /// Writes the value `x` (the bits or the reference of a `TValue` whose type matched) into the
    /// place of the block `r`: a record or static array laid out flat as `AggStore` does, a
    /// managed slot counting the new value and letting go of the old one, a `[weak]` slot
    /// without counting.
    fn rtti_store(&mut self, r: heap::StrRef, place: &Place, x: Slot) -> Result<(), RefError> {
        if let Some(ty) = self.flat_elem_type(place.ty) {
            return self.agg_store(r, place.pos, ty, x, None);
        }
        let t = self.heap.any_slots(r)?.0;
        let kind = self.heap.slot_kind(t, place.pos);
        self.heap.check_slot(r, place.pos as i64)?;
        // Only a counted value can be a record or static array holding a lent location (as
        // `ObjSetRef`); the bits of any other value may look like a block's reference
        if kind == bytecode::SLOT_REF {
            self.heap.check_share(x)?;
        }
        self.heap.add_ref_kind(x, kind)?;
        let old = self.heap.any_replace(r, place.pos, x);
        match old {
            Ok(old) => Ok(self.heap.release_kind(old, kind)?),
            Err(e) => {
                self.heap.release_kind(x, kind)?;
                Err(e.into())
            }
        }
    }

    /// `RttiClassType` (`I135`): with the mode `r(B+1)` 0, the type information number of the
    /// class reference `rB` (0 for `nil`); with 1, the class reference of the class whose type
    /// information number is `rB` (`nil` for any other type).
    pub(super) fn rtti_class_type(&mut self, v: Slot, mode: i64) -> Result<Slot, RefError> {
        match mode {
            CLASS_TO_INFO if v == 0 => Ok(0),
            CLASS_TO_INFO => {
                let ty = u32::try_from(v - 1)
                    .map_err(|_| RefError::Fatal(format!("bad class reference {v}")))?;
                Ok(self.class_info_number(ty).unwrap_or(0))
            }
            INFO_TO_CLASS => Ok(self
                .rtti_entry(v)
                .filter(|e| e.kind == TK_CLASS && e.type_index != bytecode::NONE_INDEX)
                .map_or(0, |e| i64::from(e.type_index) + 1)),
            m => Err(RefError::Fatal(format!("bad `RttiClassType` mode {m}"))),
        }
    }
}

/// The `TMethodKind` values of a method entry (`docs/spec/rtti.md` 5.1).
const MK_CONSTRUCTOR: u8 = 2;
const MK_CLASS_PROCEDURE: u8 = 4;
const MK_CLASS_FUNCTION: u8 = 5;

/// The flag of a static method entry (`docs/spec/bytecode.md` 3.2).
const METHOD_STATIC: u8 = 1;

/// The tables `RttiBridge` reads (`I142`).
const BRIDGE_PROPS: i64 = TABLE_PROPS as i64;
const BRIDGE_METHODS: i64 = TABLE_METHODS as i64;

impl Isolate {
    /// The method value `TRttiMethod.Invoke` calls the method `index` through, for the instance,
    /// class reference or record the `TValue` `v` holds (`RttiMethodRef`, `I141`;
    /// `docs/spec/rtti.md` 5.7): a method of an instance holds the instance, a class method its
    /// class reference, a record's or a static method none (a record's `Self` is passed by the
    /// bridge). A virtual method is the one of the table of the actual class. A constructor has
    /// none (`nil`): its bridge creates the instance of the class reference.
    pub(super) fn rtti_method_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let (index, v) = (self.task.regs[rb], self.task.regs[rb + 1]);
        let bad = || RefError::Fatal(format!("bad method {index}"));
        let s = self.module.rtti.as_ref().ok_or_else(bad)?;
        let i = usize::try_from(index).map_err(|_| bad())?;
        let m = s.methods.get(i).ok_or_else(bad)?;
        let (kind, flags, call, name) = (m.kind, m.flags, m.call, s.name(m.name).to_string());
        let owner = self.rtti_owner(TABLE_METHODS, i);
        let inst = self.value_parts(v)?;
        let static_ = flags & METHOD_STATIC != 0;
        let class_level = matches!(kind, MK_CLASS_PROCEDURE | MK_CLASS_FUNCTION);
        let c = self.rtti_callable(owner, &name, kind, static_, class_level, call, inst)?;
        Ok(self.put(ra, c)?)
    }

    /// The method value of the read (`which` 0) or write (1) method of the property `index`
    /// (`RttiPropRef`, `I144`; `docs/spec/rtti.md` 5.6), as [`Isolate::rtti_method_ref`] for an
    /// instance's method.
    pub(super) fn rtti_prop_ref(&mut self, ra: usize, rb: usize) -> Result<(), RefError> {
        let (index, which, v) = (
            self.task.regs[rb],
            self.task.regs[rb + 1],
            self.task.regs[rb + 2],
        );
        let bad = || RefError::Fatal(format!("bad property {index}"));
        let s = self.module.rtti.as_ref().ok_or_else(bad)?;
        let i = usize::try_from(index).map_err(|_| bad())?;
        let p = s.props.get(i).ok_or_else(bad)?;
        let name = s.name(p.name).to_string();
        let (flag, target, what) = if which != 0 {
            (PROP_WRITABLE, p.write, "writable")
        } else {
            (PROP_READABLE, p.read, "readable")
        };
        if p.flags & flag == 0 {
            return Err(RefError::Rt(rt(
                exc::INVALID_OPERATION,
                format!("property `{name}` is not {what}"),
            )));
        }
        let owner = self.rtti_owner(TABLE_PROPS, i);
        let inst = self.value_parts(v)?;
        let c = self.rtti_callable(owner, &name, 0, false, false, target, inst)?;
        Ok(self.put(ra, c)?)
    }

    /// The method value calling `call` (a method `name` of the kind `kind` of the type `owner`)
    /// for the `TValue` `inst`, checked against `owner` (`docs/spec/rtti.md` 5.7).
    #[allow(clippy::too_many_arguments)]
    fn rtti_callable(
        &mut self,
        owner: i64,
        name: &str,
        kind: u8,
        static_: bool,
        class_level: bool,
        call: RttiTarget,
        inst: Parts,
    ) -> Result<Slot, RefError> {
        let Some(o) = self.rtti_entry(owner) else {
            return Err(RefError::Fatal(format!("bad declaring type {owner}")));
        };
        let (o_kind, o_index) = (o.kind, o.type_index);
        let mismatch = || self.value_cast_error(inst.ty, &self.rtti_short_name(owner));
        let inst_kind = self.type_kind_of(inst.ty) as u8;
        if kind == MK_CONSTRUCTOR {
            // The bridge creates the instance of the class reference (or the record)
            return match (o_kind, inst_kind) {
                (TK_RECORD, _) => Ok(0),
                (_, TK_CLASSREF) => {
                    let class = self.rtti_class_of_ref(inst.bits)?;
                    if !self.heap.class_inherits(class, o_index) {
                        return Err(mismatch());
                    }
                    Ok(0)
                }
                (_, TK_CLASS) => Err(RefError::Rt(rt(
                    exc::INVALID_OPERATION,
                    format!(
                        "constructor `{name}` is invoked on an instance; pass a class reference"
                    ),
                ))),
                _ => Err(mismatch()),
            };
        }
        let not_callable = || {
            RefError::Rt(rt(
                exc::INVALID_OPERATION,
                format!("method `{name}` cannot be invoked"),
            ))
        };
        let func = |this: &Self, class: Option<u32>| -> Result<u32, RefError> {
            match call.kind {
                TARGET_FUNC => Ok(call.value),
                TARGET_VIRTUAL => {
                    let class = class.ok_or_else(not_callable)?;
                    this.rtti_table_entry(class, call.value)
                }
                _ => Err(not_callable()),
            }
        };
        let (kind, func, target) = if static_ {
            (heap::CallableKind::Routine, func(self, None)?, 0)
        } else if o_kind == TK_RECORD {
            // The bridge passes `Self`, the record `Instance` holds
            if inst.ty != owner {
                return Err(if inst.ty == 0 {
                    heap::BadHandle::Nil.into()
                } else {
                    mismatch()
                });
            }
            (heap::CallableKind::Routine, func(self, None)?, 0)
        } else if o_kind != TK_CLASS {
            return Err(not_callable());
        } else if class_level {
            // A class reference only, as Delphi (an instance is `EInvalidCast`)
            let to_ref = || {
                let to = format!("class of {}", self.rtti_short_name(owner));
                self.value_cast_error(inst.ty, &to)
            };
            let class = match inst_kind {
                TK_CLASSREF => self.rtti_class_of_ref(inst.bits)?,
                _ if inst.ty == 0 => return Err(heap::BadHandle::Nil.into()),
                _ => return Err(to_ref()),
            };
            if !self.heap.class_inherits(class, o_index) {
                return Err(to_ref());
            }
            (
                heap::CallableKind::ClassMethod,
                func(self, Some(class))?,
                i64::from(class) + 1,
            )
        } else {
            self.rtti_instance(inst, owner)?;
            let h = inst.bits | inst.r;
            let class = self.heap.obj_class(h)?;
            (heap::CallableKind::Method, func(self, Some(class))?, h)
        };
        if (func as usize) >= self.module.funcs.len() && func < bytecode::VMT_BUILTIN {
            return Err(RefError::Fatal(format!("bad function index {func}")));
        }
        let c = heap::Callable { kind, func, target };
        Ok(self.heap.alloc_callable(c)?)
    }

    /// The class of the class reference `r` (`nil` is `EAccessViolation`).
    fn rtti_class_of_ref(&self, r: Slot) -> Result<u32, RefError> {
        if r == 0 {
            return Err(heap::BadHandle::Nil.into());
        }
        u32::try_from(r - 1).map_err(|_| RefError::Fatal(format!("bad class reference {r}")))
    }

    /// Entry `k` of the virtual method table of the class `ty`: an abstract one is
    /// `EAbstractError`.
    fn rtti_table_entry(&self, ty: u32, k: u32) -> Result<u32, RefError> {
        let entry = self
            .heap
            .type_entry(ty)
            .ok_or_else(|| RefError::Fatal(format!("type {ty} out of range")))?;
        match entry.vmt.get(k as usize) {
            Some(&bytecode::VMT_ABSTRACT) => Err(RefError::Rt(abstract_called(entry, k))),
            Some(&f) => Ok(f),
            None => Err(RefError::Fatal(format!(
                "virtual method {k} out of range in {}",
                entry.name
            ))),
        }
    }

    /// `RttiBridge` (`I142`; `docs/spec/rtti.md` 7.3): the bridge of the method `index` (`table`
    /// 2) or of the read (`which` 0) or write accessor of the property `index` (`table` 1), as
    /// a procedural value (the function index + 1; 0 if none).
    pub(super) fn rtti_bridge(&self, table: i64, index: i64, which: i64) -> Result<Slot, RefError> {
        let bad = || RefError::Fatal(format!("bad member {index} of the table {table}"));
        let s = self.module.rtti.as_ref().ok_or_else(bad)?;
        let i = usize::try_from(index).map_err(|_| bad())?;
        let bridge = match table {
            BRIDGE_METHODS => s.methods.get(i).ok_or_else(bad)?.call.bridge,
            BRIDGE_PROPS => {
                let p = s.props.get(i).ok_or_else(bad)?;
                if which != 0 { p.write } else { p.read }.bridge
            }
            _ => return Err(bad()),
        };
        Ok(if bridge == bytecode::NONE_INDEX {
            0
        } else {
            i64::from(bridge) + 1
        })
    }

    /// `RttiAttrFunc` (`I143`; `docs/spec/rtti.md` 7.4): the function making the instance of the
    /// attribute `index`, as a procedural value (0 if none).
    pub(super) fn rtti_attr_func(&self, index: i64) -> Result<Slot, RefError> {
        let bad = || RefError::Fatal(format!("bad attribute {index}"));
        let s = self.module.rtti.as_ref().ok_or_else(bad)?;
        let a = usize::try_from(index)
            .ok()
            .and_then(|i| s.attrs.get(i))
            .ok_or_else(bad)?;
        Ok(if a.make == bytecode::NONE_INDEX {
            0
        } else {
            i64::from(a.make) + 1
        })
    }

    /// `RttiArgSet` (`I153`; `docs/spec/rtti.md` 5.7): writes the `TValue` `v` into the element
    /// `index` of the open array `arr` where it is, when it is a dynamic array, shared with the
    /// caller. A static array is a value: it is not written (its block may be shared by other
    /// variables, and the call holds a reference to it too). Returns whether it wrote.
    pub(super) fn rtti_arg_set(
        &mut self,
        arr: Slot,
        index: i64,
        v: Slot,
    ) -> Result<Slot, RefError> {
        if arr == 0 {
            return Err(RefError::Rt(rt(
                exc::RANGE_ERROR,
                dyn_index_message(index, 0),
            )));
        }
        let ty = self.heap.agg_type(arr)?;
        if !self.heap.is_dyn_type(ty) {
            return Ok(0);
        }
        let width = self.heap.type_entry(ty).map_or(1, |t| t.width.max(1)) as usize;
        let count = self.heap.any_slots(arr)?.1.len() / width;
        let i = usize::try_from(index)
            .ok()
            .filter(|i| *i < count)
            .ok_or_else(|| RefError::Rt(rt(exc::RANGE_ERROR, dyn_index_message(index, count))))?;
        if width == 1 {
            self.heap.check_slot(arr, i as i64)?;
            self.heap.check_share(v)?;
            self.heap.add_ref(v)?;
            let old = self.heap.any_replace(arr, i, v)?;
            self.heap.release(old)?;
            return Ok(1);
        }
        // A `TValue` laid out flat in the array (`AggStore`)
        let tvalue = if v != 0 {
            self.heap.agg_type(v)?
        } else {
            self.tvalue_type_index()
                .ok_or_else(|| RefError::Fatal("no `TValue` type".into()))?
        };
        self.agg_store(arr, i * width, tvalue, v, None)?;
        Ok(1)
    }

    /// The type index of `TValue` in the type section.
    fn tvalue_type_index(&self) -> Option<u32> {
        (0..self.module.types.len() as u32).find(|&t| {
            self.heap
                .type_entry(t)
                .is_some_and(|e| e.name == TVALUE_NAME)
        })
    }
}
