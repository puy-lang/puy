//! The debug information section (`docs/spec/bytecode.md` 3.2, section 14): the debug type
//! entries of the values `Assert` records (`docs/spec/semantics.md` 5.4), numbered in order of
//! first use, an entry's field and element types after it.

use crate::bytecode::debug::{self as dt, DebugField, DebugSection, DebugType};
use crate::sema::types::{Ty, TypeTable};
use std::collections::HashMap;

/// The debug type entries made so far.
#[derive(Default)]
pub(super) struct DebugTypes {
    types: Vec<DebugType>,
    numbers: HashMap<Ty, u32>,
    /// The entry of each plain kind.
    plain: HashMap<u8, u32>,
}

impl DebugTypes {
    /// The debug type number of `ty`, adding its entry (and those of its parts) on first use.
    pub(super) fn number(&mut self, table: &TypeTable, ty: Ty) -> u32 {
        let ty = plain(table, ty);
        if let Some(n) = self.numbers.get(&ty) {
            return *n;
        }
        if let Some(kind) = plain_kind(ty) {
            // One entry for each kind (the classes all show the same way)
            if let Some(n) = self.plain.get(&kind) {
                return *n;
            }
            self.types.push(DebugType::Plain(kind));
            let n = self.types.len() as u32;
            self.plain.insert(kind, n);
            self.numbers.insert(ty, n);
            return n;
        }
        // The number is taken first, so that a type reached again through its parts (a dynamic
        // array of records holding that array) names it
        self.types.push(DebugType::Plain(dt::DT_INT));
        let n = self.types.len() as u32;
        self.numbers.insert(ty, n);
        let flat = |t: Ty| table.flat_width(t).unwrap_or(0);
        let slots = |t: Ty| table.flat_width(t).unwrap_or(1);
        let entry = match ty {
            Ty::Enum(_) => DebugType::Enum(
                table
                    .enum_values(ty)
                    .iter()
                    .map(|v| (v.ord as i64, v.name.clone()))
                    .collect(),
            ),
            Ty::Set(_) | Ty::BigSet(_) => {
                let base = table.set_base(ty).unwrap_or(Ty::Integer);
                DebugType::Set {
                    base: self.number(table, base),
                    big: matches!(ty, Ty::BigSet(_)),
                }
            }
            Ty::Record(_) => {
                let mut offset = 0;
                let mut fields = Vec::new();
                for f in table.fields(ty) {
                    fields.push(DebugField {
                        name: f.name.clone(),
                        ty: self.number(table, f.ty),
                        offset,
                    });
                    offset += slots(f.ty);
                }
                DebugType::Record {
                    fields,
                    flat: flat(ty),
                }
            }
            Ty::Array(_) => {
                let (_, lo, hi, elem) = table.array(ty).expect("a static array type");
                DebugType::Array {
                    elem: self.number(table, elem),
                    count: (hi - lo + 1) as u32,
                    width: slots(elem),
                    flat: flat(ty),
                }
            }
            Ty::DynArray(_) | Ty::OpenArray(_) => {
                let elem = table.dyn_elem(ty).expect("a dynamic array type");
                DebugType::DynArray {
                    elem: self.number(table, elem),
                    width: slots(elem),
                }
            }
            _ => unreachable!("the other types are of a plain kind"),
        };
        self.types[n as usize - 1] = entry;
        n
    }

    /// The section, when some entry was made.
    pub(super) fn section(self) -> Option<DebugSection> {
        (!self.types.is_empty()).then_some(DebugSection { types: self.types })
    }
}

/// The plain kind a value of `ty` is shown by, `None` for the kinds with more to the entry.
fn plain_kind(ty: Ty) -> Option<u8> {
    Some(match ty {
        Ty::UInt64 => dt::DT_UINT,
        Ty::Boolean => dt::DT_BOOL,
        Ty::Char => dt::DT_CHAR,
        Ty::Single => dt::DT_SINGLE,
        Ty::Double => dt::DT_DOUBLE,
        Ty::Currency => dt::DT_CURRENCY,
        Ty::String => dt::DT_STRING,
        Ty::Utf8String => dt::DT_UTF8,
        Ty::Class(_) | Ty::Weak(_) => dt::DT_CLASS,
        Ty::Intf(_) => dt::DT_INTF,
        Ty::ClassRef(_) => dt::DT_CLASSREF,
        Ty::Proc(_) | Ty::Callable(_) => dt::DT_PROC,
        Ty::Enum(_)
        | Ty::Set(_)
        | Ty::BigSet(_)
        | Ty::Record(_)
        | Ty::Array(_)
        | Ty::DynArray(_)
        | Ty::OpenArray(_) => return None,
        // The integers (their slots hold the value sign- or zero-extended) and `PTypeInfo`
        _ => dt::DT_INT,
    })
}

/// The type whose values a value of `ty` is shown as: a subrange's base type, a distinct type's
/// root.
fn plain(table: &TypeTable, ty: Ty) -> Ty {
    let mut t = ty;
    loop {
        let next = table.base(table.root(t));
        if next == t {
            return t;
        }
        t = next;
    }
}
