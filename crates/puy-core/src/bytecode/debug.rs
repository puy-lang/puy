//! The debug information section (section kind 14; `docs/spec/bytecode.md` 3.2): how the values
//! `Assert` records are shown (`docs/spec/semantics.md` 5.4). Its entries, how they are written
//! and read, their verification (3.3) and their lines in the dump (5.2).

use super::{LoadError, Module, Reader, Writer};

/// The kinds of a debug type entry (`docs/spec/bytecode.md` 3.2).
pub const DT_INT: u8 = 1;
pub const DT_UINT: u8 = 2;
pub const DT_BOOL: u8 = 3;
pub const DT_CHAR: u8 = 4;
pub const DT_SINGLE: u8 = 5;
pub const DT_DOUBLE: u8 = 6;
pub const DT_CURRENCY: u8 = 7;
pub const DT_STRING: u8 = 8;
pub const DT_UTF8: u8 = 9;
pub const DT_ENUM: u8 = 10;
pub const DT_SET: u8 = 11;
pub const DT_RECORD: u8 = 12;
pub const DT_ARRAY: u8 = 13;
pub const DT_DYNARRAY: u8 = 14;
pub const DT_CLASS: u8 = 15;
pub const DT_INTF: u8 = 16;
pub const DT_CLASSREF: u8 = 17;
pub const DT_PROC: u8 = 18;

/// The largest number of slots a value laid out flat takes (`docs/spec/bytecode.md` 1.5).
const MAX_FLAT: u32 = super::MAX_FLAT_WIDTH;

/// The debug information section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugSection {
    /// The debug type entries; debug type number `n` is `types[n - 1]` (0 is "not evaluated").
    pub types: Vec<DebugType>,
}

/// A debug type entry: how a value of the type is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugType {
    /// The kinds without anything more ([`DT_INT`], [`DT_UINT`], … [`DT_UTF8`], [`DT_CLASS`],
    /// [`DT_INTF`], [`DT_CLASSREF`], [`DT_PROC`]).
    Plain(u8),
    /// An enumeration: its values (ordinal, name) in declaration order.
    Enum(Vec<(i64, String)>),
    /// A set: the debug type of its base type (an ordinal kind) and whether it is a big set
    /// (a block of bit words) rather than the bits of the slot.
    Set { base: u32, big: bool },
    /// A record: its fields in declaration order, and the number of slots it takes when it is
    /// laid out flat in a container (0 when it is not: a field of this type holds a reference).
    Record { fields: Vec<DebugField>, flat: u32 },
    /// A static array: the element type, the number of elements, the slots of an element, and
    /// the slots the array takes when laid out flat (0 when it is not).
    Array {
        elem: u32,
        count: u32,
        width: u32,
        flat: u32,
    },
    /// A dynamic array (or the block an open array receives): the element type and the slots of
    /// an element.
    DynArray { elem: u32, width: u32 },
}

/// A field of a record entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugField {
    pub name: String,
    /// The field's debug type number.
    pub ty: u32,
    /// The first slot of the field in the record's slots.
    pub offset: u32,
}

impl DebugType {
    /// The kind of the entry.
    pub fn kind(&self) -> u8 {
        match self {
            DebugType::Plain(k) => *k,
            DebugType::Enum(_) => DT_ENUM,
            DebugType::Set { .. } => DT_SET,
            DebugType::Record { .. } => DT_RECORD,
            DebugType::Array { .. } => DT_ARRAY,
            DebugType::DynArray { .. } => DT_DYNARRAY,
        }
    }

    /// The number of slots a value of this type takes laid out flat in a container, `None` when
    /// it takes one slot holding the value (or a reference to it).
    pub fn flat(&self) -> Option<u32> {
        match self {
            DebugType::Record { flat, .. } | DebugType::Array { flat, .. } if *flat > 0 => {
                Some(*flat)
            }
            _ => None,
        }
    }

    /// Whether the kind is an ordinal one (the base type of a set).
    fn is_ordinal(&self) -> bool {
        matches!(self.kind(), DT_INT | DT_UINT | DT_BOOL | DT_CHAR | DT_ENUM)
    }
}

impl DebugSection {
    /// The entry of a debug type number (1 and up).
    pub fn entry(&self, number: u32) -> Option<&DebugType> {
        number
            .checked_sub(1)
            .and_then(|i| self.types.get(i as usize))
    }

    /// The bytes of the section (`docs/spec/bytecode.md` 3.2).
    pub(super) fn encode(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.types.len() as u32);
        for t in &self.types {
            w.u8(t.kind());
            w.u8(0);
            w.u16(0);
            match t {
                DebugType::Plain(_) => {}
                DebugType::Enum(values) => {
                    w.u32(values.len() as u32);
                    for (ord, name) in values {
                        w.i64(*ord);
                        w.lp_utf8(name);
                    }
                }
                DebugType::Set { base, big } => {
                    w.u32(*base);
                    w.u32(u32::from(*big));
                }
                DebugType::Record { fields, flat } => {
                    w.u32(*flat);
                    w.u32(fields.len() as u32);
                    for f in fields {
                        w.lp_utf8(&f.name);
                        w.u32(f.ty);
                        w.u32(f.offset);
                    }
                }
                DebugType::Array {
                    elem,
                    count,
                    width,
                    flat,
                } => {
                    w.u32(*elem);
                    w.u32(*count);
                    w.u32(*width);
                    w.u32(*flat);
                }
                DebugType::DynArray { elem, width } => {
                    w.u32(*elem);
                    w.u32(*width);
                }
            }
        }
        w.buf
    }

    /// Reads the section (`docs/spec/bytecode.md` 3.2).
    pub(super) fn decode(buf: &[u8]) -> Result<DebugSection, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("debug type")?;
        let mut types = Vec::with_capacity(n);
        for k in 0..n {
            let kind = r.u8()?;
            r.u8()?;
            r.u16()?;
            let t = match kind {
                DT_INT | DT_UINT | DT_BOOL | DT_CHAR | DT_SINGLE | DT_DOUBLE | DT_CURRENCY
                | DT_STRING | DT_UTF8 | DT_CLASS | DT_INTF | DT_CLASSREF | DT_PROC => {
                    DebugType::Plain(kind)
                }
                DT_ENUM => {
                    let m = r.count("enumeration value")?;
                    let mut values = Vec::with_capacity(m);
                    for _ in 0..m {
                        values.push((r.i64()?, r.lp_utf8()?));
                    }
                    DebugType::Enum(values)
                }
                DT_SET => {
                    let base = r.u32()?;
                    let big = match r.u32()? {
                        0 => false,
                        1 => true,
                        _ => return Err(bad(k + 1)),
                    };
                    DebugType::Set { base, big }
                }
                DT_RECORD => {
                    let flat = r.u32()?;
                    let m = r.count("debug field")?;
                    let mut fields = Vec::with_capacity(m);
                    for _ in 0..m {
                        fields.push(DebugField {
                            name: r.lp_utf8()?,
                            ty: r.u32()?,
                            offset: r.u32()?,
                        });
                    }
                    DebugType::Record { fields, flat }
                }
                DT_ARRAY => DebugType::Array {
                    elem: r.u32()?,
                    count: r.u32()?,
                    width: r.u32()?,
                    flat: r.u32()?,
                },
                DT_DYNARRAY => DebugType::DynArray {
                    elem: r.u32()?,
                    width: r.u32()?,
                },
                _ => return Err(bad(k + 1)),
            };
            types.push(t);
        }
        Ok(DebugSection { types })
    }
}

/// The error of a malformed entry.
fn bad(number: usize) -> LoadError {
    LoadError::Invalid(format!("debug type {number} is malformed"))
}

impl Module {
    /// The checks of the debug information section (`docs/spec/bytecode.md` 3.3): the numbers
    /// an entry names are entries of the section, a set's base type is ordinal, the widths are
    /// those of slots laid out flat, and a record's fields lie within its slots.
    pub(super) fn verify_debug(&self) -> Result<(), LoadError> {
        let Some(s) = &self.debug else {
            return Ok(());
        };
        let entry = |n: u32| s.entry(n);
        // The slots an element or field of type `n` takes in its container
        let slots = |n: u32| entry(n).map(|t| t.flat().unwrap_or(1));
        let width_ok = |w: u32| (1..=MAX_FLAT).contains(&w);
        let flat_ok = |f: u32| f == 0 || width_ok(f);
        for (k, t) in s.types.iter().enumerate() {
            let ok = match t {
                DebugType::Plain(_) | DebugType::Enum(_) => true,
                DebugType::Set { base, .. } => entry(*base).is_some_and(DebugType::is_ordinal),
                DebugType::Record { fields, flat } => {
                    let total = fields.iter().try_fold(0u64, |end, f| {
                        let w = slots(f.ty)?;
                        (u64::from(f.offset) >= end).then_some(u64::from(f.offset) + u64::from(w))
                    });
                    flat_ok(*flat) && total.is_some_and(|end| *flat == 0 || end <= u64::from(*flat))
                }
                DebugType::Array {
                    elem,
                    count,
                    width,
                    flat,
                } => {
                    slots(*elem) == Some(*width)
                        && flat_ok(*flat)
                        && (*flat == 0 || u64::from(*count) * u64::from(*width) == u64::from(*flat))
                }
                DebugType::DynArray { elem, width } => slots(*elem) == Some(*width),
            };
            if !ok {
                return Err(bad(k + 1));
            }
        }
        Ok(())
    }
}

/// The names of the plain kinds in the dump.
fn plain_name(kind: u8) -> &'static str {
    match kind {
        DT_INT => "int",
        DT_UINT => "uint",
        DT_BOOL => "bool",
        DT_CHAR => "char",
        DT_SINGLE => "single",
        DT_DOUBLE => "double",
        DT_CURRENCY => "currency",
        DT_STRING => "string",
        DT_UTF8 => "utf8",
        DT_CLASS => "class",
        DT_INTF => "interface",
        DT_CLASSREF => "classref",
        _ => "procedure",
    }
}

/// The `.debug` lines of the dump (`docs/spec/bytecode.md` 5.2).
pub(super) fn disassemble(s: &DebugSection, out: &mut String) {
    out.push_str(".debug\n");
    for (k, t) in s.types.iter().enumerate() {
        let line = match t {
            DebugType::Plain(kind) => plain_name(*kind).to_string(),
            DebugType::Enum(values) => {
                let v: Vec<String> = values.iter().map(|(o, n)| format!("{n}={o}")).collect();
                format!("enum {}", v.join(" "))
            }
            DebugType::Set { base, big } => {
                format!("set dt{base}{}", if *big { " big" } else { "" })
            }
            DebugType::Record { fields, flat } => {
                let f: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}:dt{}@{}", f.name, f.ty, f.offset))
                    .collect();
                format!("record flat={flat} {}", f.join(" "))
            }
            DebugType::Array {
                elem,
                count,
                width,
                flat,
            } => format!("array dt{elem} count={count} width={width} flat={flat}"),
            DebugType::DynArray { elem, width } => format!("dynarray dt{elem} width={width}"),
        };
        out.push_str(&format!("  dt{}: {}\n", k + 1, line.trim_end()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySourceProvider;

    /// The module of a program whose `Assert` records a record, an enumeration and a set.
    fn module() -> Module {
        let src = "program T;
                   type
                     TColor = (Red, Green);
                     TPoint = record X, Y: Integer; C: TColor; end;
                   function Same(const Q: TPoint): Boolean;
                   begin
                     Result := Q.X = 1;
                   end;
                   var P: TPoint; S: set of TColor;
                   begin
                     Assert(Same(P) or (S = []));
                   end.";
        let mut provider = MemorySourceProvider::new();
        let out = crate::driver::compile(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &crate::driver::test_options(),
        );
        out.stages.module.expect("a module")
    }

    fn debug(m: &mut Module) -> &mut DebugSection {
        m.debug.as_mut().expect("a debug information section")
    }

    // The section is written and read back as it was (bytecode.md 3.2).
    #[test]
    fn section_round_trips() {
        let m = module();
        let back = Module::decode(&m.encode(crate::VERSION), crate::VERSION).expect("valid");
        assert_eq!(back.debug, m.debug);
        assert!(m.debug.as_ref().is_some_and(|s| s.types.len() >= 4));
    }

    // The loading rejects entries naming numbers outside the section, a set over a type that is
    // not ordinal, and fields that overlap or leave the record's slots (bytecode.md 3.3).
    #[test]
    fn verify_rejects_malformed_entries() {
        type Break = fn(&mut DebugSection);
        let cases: [(&str, Break); 5] = [
            ("a set's base out of range", |s| {
                s.types.push(DebugType::Set {
                    base: 1000,
                    big: false,
                })
            }),
            ("a set over a string", |s| {
                s.types.push(DebugType::Plain(DT_STRING));
                let n = s.types.len() as u32;
                s.types.push(DebugType::Set { base: n, big: true });
            }),
            ("overlapping fields", |s| {
                s.types.push(DebugType::Plain(DT_INT));
                let n = s.types.len() as u32;
                let f = |offset| DebugField {
                    name: "X".into(),
                    ty: n,
                    offset,
                };
                s.types.push(DebugType::Record {
                    fields: vec![f(0), f(0)],
                    flat: 2,
                });
            }),
            ("a field past the flat slots", |s| {
                s.types.push(DebugType::Plain(DT_INT));
                let n = s.types.len() as u32;
                s.types.push(DebugType::Record {
                    fields: vec![DebugField {
                        name: "X".into(),
                        ty: n,
                        offset: 3,
                    }],
                    flat: 2,
                });
            }),
            ("an element width that is not the element's", |s| {
                s.types.push(DebugType::Plain(DT_INT));
                let n = s.types.len() as u32;
                s.types.push(DebugType::DynArray { elem: n, width: 2 });
            }),
        ];
        for (what, break_it) in cases {
            let mut m = module();
            break_it(debug(&mut m));
            assert!(
                matches!(m.verify(), Err(LoadError::Invalid(ref e)) if e.starts_with("debug type")),
                "{what}"
            );
        }
    }
}
