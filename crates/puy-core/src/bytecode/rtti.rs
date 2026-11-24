//! The type information section (section kind 12; `docs/spec/bytecode.md` 3.2,
//! `docs/spec/rtti.md` chapter 7): its entries, how they are written and read, their verification
//! (3.3) and their lines in the dump (5.2).

use super::{LoadError, Module, NONE_INDEX, Reader, TYPE_CLASS, TYPE_RECORD, Writer};

/// The `TTypeKind` ordinals the type entries use (`docs/spec/rtti.md` 2.3).
pub const TK_INTEGER: u8 = 1;
pub const TK_ENUMERATION: u8 = 3;
pub const TK_FLOAT: u8 = 4;
pub const TK_SET: u8 = 6;
pub const TK_CLASS: u8 = 7;
pub const TK_METHOD: u8 = 8;
pub const TK_WCHAR: u8 = 9;
pub const TK_LSTRING: u8 = 10;
pub const TK_ARRAY: u8 = 13;
pub const TK_RECORD: u8 = 14;
pub const TK_INTERFACE: u8 = 15;
pub const TK_INT64: u8 = 16;
pub const TK_DYNARRAY: u8 = 17;
pub const TK_USTRING: u8 = 18;
pub const TK_CLASSREF: u8 = 19;
pub const TK_POINTER: u8 = 20;
pub const TK_PROCEDURE: u8 = 21;

/// The flags of a type entry: the member stage, an instantiation, a reference counted class, a
/// GUID written.
pub const RTTI_MEMBERS: u8 = 1;
pub const RTTI_INSTANCE: u8 = 2;
pub const RTTI_REF_COUNTED: u8 = 4;
pub const RTTI_GUID: u8 = 8;

/// The shape of an ordinal type entry: 0 to 7 the integer widths (`ShortInt` to `UInt64`,
/// `docs/spec/bytecode.md` 2.2), then `Char` and the enumerations (`Boolean` included).
pub const SHAPE_CHAR: u8 = 8;
pub const SHAPE_ENUM: u8 = 9;

/// The `TFloatType` ordinals of `Single`, `Double` and `Currency`.
pub const FT_SINGLE: u8 = 0;
pub const FT_DOUBLE: u8 = 1;
pub const FT_CURR: u8 = 4;

/// The `TMemberVisibility` ordinals used by fields, properties and methods.
pub const VIS_PUBLIC: u8 = 2;
pub const VIS_PUBLISHED: u8 = 3;

/// The kinds of a call target or accessor (`docs/spec/bytecode.md` 3.2): none, a field, a
/// function, an entry of the virtual method table.
pub const TARGET_NONE: u8 = 0;
pub const TARGET_FIELD: u8 = 1;
pub const TARGET_FUNC: u8 = 2;
pub const TARGET_VIRTUAL: u8 = 3;

/// The flags of a property entry: its read and write accessors are declared.
pub const PROP_READABLE: u8 = 1;
pub const PROP_WRITABLE: u8 = 2;

/// The member tables of the section, as `RttiMemberField` (`I130`) names them.
pub const TABLE_FIELDS: u8 = 0;
pub const TABLE_PROPS: u8 = 1;
pub const TABLE_METHODS: u8 = 2;
pub const TABLE_PARAMS: u8 = 3;
pub const TABLE_ATTRS: u8 = 4;

/// A range of a member table: { first, count }.
pub type Range = (u32, u32);

/// The type information section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RttiSection {
    /// The names, each once, in order of first use.
    pub names: Vec<String>,
    /// The type entries; type information number `n` is `types[n - 1]`.
    pub types: Vec<RttiTypeEntry>,
    pub fields: Vec<RttiFieldEntry>,
    pub props: Vec<RttiPropEntry>,
    pub methods: Vec<RttiMethodEntry>,
    pub params: Vec<RttiParamEntry>,
    pub attrs: Vec<RttiAttrEntry>,
}

/// A type entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RttiTypeEntry {
    /// The `TTypeKind` ordinal.
    pub kind: u8,
    pub flags: u8,
    pub name: u32,
    pub full_name: u32,
    /// The type index of the type section, or [`NONE_INDEX`].
    pub type_index: u32,
    pub ext: RttiExt,
    /// An instantiation's definition (its fully qualified name) and type arguments.
    pub generic: Option<(u32, Vec<u32>)>,
}

/// The part of a type entry that depends on its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RttiExt {
    /// `tkInteger`, `tkInt64`, `tkWChar`, `tkEnumeration`: the shape, the range, the original
    /// type of a subrange or distinct type (0 if none), and an enumeration's values.
    Ordinal {
        shape: u8,
        min: i64,
        max: i64,
        base: u32,
        values: Vec<(i64, u32)>,
    },
    /// `tkFloat`: the `TFloatType` ordinal.
    Float(u8),
    /// `tkSet`: the base type and the number of bits.
    Set { base: u32, bits: u32 },
    /// `tkArray`: the element type, its width in slots, the number of elements and the index
    /// type.
    Array {
        elem: u32,
        width: u32,
        count: u32,
        index: u32,
    },
    /// `tkDynArray`: the element type.
    DynArray { elem: u32 },
    /// `tkRecord`, `tkClass`: the parent (a class's; 0 for `TObject` and the records) and the
    /// member ranges.
    Struct {
        parent: u32,
        fields: Range,
        props: Range,
        methods: Range,
        attrs: Range,
    },
    /// `tkInterface`: the parent, the GUID and the member ranges.
    Intf {
        parent: u32,
        guid: [u8; 16],
        methods: Range,
        attrs: Range,
    },
    /// `tkClassRef`: the class.
    ClassRef { class: u32 },
    /// `tkProcedure`, `tkMethod`: the parameters and the result type (0 if none).
    Proc { params: Range, ret: u32 },
    /// `tkUString`, `tkLString`, `tkPointer`.
    None,
}

/// A call target or property accessor: its kind ([`TARGET_NONE`] …), the width of a field, the
/// slot, function index or table entry, and the bridge function ([`NONE_INDEX`] if none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RttiTarget {
    pub kind: u8,
    pub width: u16,
    pub value: u32,
    pub bridge: u32,
}

impl RttiTarget {
    pub const NONE: RttiTarget = RttiTarget {
        kind: TARGET_NONE,
        width: 0,
        value: 0,
        bridge: NONE_INDEX,
    };
}

const BRIDGE_PREFIX: &str = "<invoke:";
const BRIDGE_SUFFIX: char = '>';

/// Wraps the shape in the reserved delimiters of a bridge name (`docs/spec/rtti.md` 7.3).
pub(crate) fn format_bridge_name(shape: std::fmt::Arguments<'_>) -> String {
    format!("{BRIDGE_PREFIX}{shape}{BRIDGE_SUFFIX}")
}

fn is_bridge_name(name: &str) -> bool {
    name.starts_with(BRIDGE_PREFIX) && name.ends_with(BRIDGE_SUFFIX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RttiFieldEntry {
    pub name: u32,
    pub ty: u32,
    pub vis: u8,
    pub width: u16,
    pub pos: u32,
    pub attrs: Range,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RttiPropEntry {
    pub name: u32,
    pub ty: u32,
    pub vis: u8,
    pub flags: u8,
    pub read: RttiTarget,
    pub write: RttiTarget,
    pub attrs: Range,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RttiMethodEntry {
    pub name: u32,
    /// The `TMethodKind` ordinal.
    pub kind: u8,
    pub vis: u8,
    pub flags: u8,
    pub params: Range,
    /// The result type (0 if none).
    pub ret: u32,
    pub call: RttiTarget,
    pub attrs: Range,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RttiParamEntry {
    pub name: u32,
    pub ty: u32,
    /// The `TParamFlags` bits.
    pub flags: u8,
    pub attrs: Range,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RttiAttrEntry {
    pub class: u32,
    /// The function making the instance ([`NONE_INDEX`] if none; `docs/spec/rtti.md` 7.4).
    pub make: u32,
}

/// The `TTypeKind` names, without `tk`, in lowercase (the dump).
const KIND_NAMES: [&str; 22] = [
    "unknown",
    "integer",
    "char",
    "enumeration",
    "float",
    "string",
    "set",
    "class",
    "method",
    "wchar",
    "lstring",
    "wstring",
    "variant",
    "array",
    "record",
    "interface",
    "int64",
    "dynarray",
    "ustring",
    "classref",
    "pointer",
    "procedure",
];

impl Writer {
    fn range(&mut self, r: Range) {
        self.u32(r.0);
        self.u32(r.1);
    }
    fn target(&mut self, t: &RttiTarget) {
        self.u8(t.kind);
        self.u8(0);
        self.u16(t.width);
        self.u32(t.value);
        self.u32(t.bridge);
    }
}

impl Reader<'_> {
    fn range(&mut self) -> Result<Range, LoadError> {
        Ok((self.u32()?, self.u32()?))
    }
    fn target(&mut self) -> Result<RttiTarget, LoadError> {
        let kind = self.u8()?;
        self.u8()?;
        Ok(RttiTarget {
            kind,
            width: self.u16()?,
            value: self.u32()?,
            bridge: self.u32()?,
        })
    }
}

impl RttiSection {
    /// The bytes of the section (`docs/spec/bytecode.md` 3.2).
    pub(super) fn encode(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.names.len() as u32);
        for n in &self.names {
            w.lp_utf8(n);
        }
        w.u32(self.types.len() as u32);
        for t in &self.types {
            w.u8(t.kind);
            w.u8(t.flags);
            w.u16(0);
            w.u32(t.name);
            w.u32(t.full_name);
            w.u32(t.type_index);
            match &t.ext {
                RttiExt::Ordinal {
                    shape,
                    min,
                    max,
                    base,
                    values,
                } => {
                    w.u8(*shape);
                    w.u8(0);
                    w.u16(0);
                    w.i64(*min);
                    w.i64(*max);
                    w.u32(*base);
                    if *shape == SHAPE_ENUM {
                        w.u32(values.len() as u32);
                        for (ord, name) in values {
                            w.i64(*ord);
                            w.u32(*name);
                        }
                    }
                }
                RttiExt::Float(f) => {
                    w.u8(*f);
                    w.u8(0);
                    w.u16(0);
                }
                RttiExt::Set { base, bits } => {
                    w.u32(*base);
                    w.u32(*bits);
                }
                RttiExt::Array {
                    elem,
                    width,
                    count,
                    index,
                } => {
                    w.u32(*elem);
                    w.u32(*width);
                    w.u32(*count);
                    w.u32(*index);
                }
                RttiExt::DynArray { elem } => w.u32(*elem),
                RttiExt::Struct {
                    parent,
                    fields,
                    props,
                    methods,
                    attrs,
                } => {
                    w.u32(*parent);
                    w.range(*fields);
                    w.range(*props);
                    w.range(*methods);
                    w.range(*attrs);
                }
                RttiExt::Intf {
                    parent,
                    guid,
                    methods,
                    attrs,
                } => {
                    w.u32(*parent);
                    w.buf.extend_from_slice(guid);
                    w.range(*methods);
                    w.range(*attrs);
                }
                RttiExt::ClassRef { class } => w.u32(*class),
                RttiExt::Proc { params, ret } => {
                    w.range(*params);
                    w.u32(*ret);
                }
                RttiExt::None => {}
            }
            if let Some((def, args)) = &t.generic {
                w.u32(*def);
                w.u32(args.len() as u32);
                for a in args {
                    w.u32(*a);
                }
            }
        }
        w.u32(self.fields.len() as u32);
        for f in &self.fields {
            w.u32(f.name);
            w.u32(f.ty);
            w.u8(f.vis);
            w.u8(0);
            w.u16(f.width);
            w.u32(f.pos);
            w.range(f.attrs);
        }
        w.u32(self.props.len() as u32);
        for p in &self.props {
            w.u32(p.name);
            w.u32(p.ty);
            w.u8(p.vis);
            w.u8(p.flags);
            w.u16(0);
            w.target(&p.read);
            w.target(&p.write);
            w.range(p.attrs);
        }
        w.u32(self.methods.len() as u32);
        for m in &self.methods {
            w.u32(m.name);
            w.u8(m.kind);
            w.u8(m.vis);
            w.u8(m.flags);
            w.u8(0);
            w.range(m.params);
            w.u32(m.ret);
            w.target(&m.call);
            w.range(m.attrs);
        }
        w.u32(self.params.len() as u32);
        for p in &self.params {
            w.u32(p.name);
            w.u32(p.ty);
            w.u8(p.flags);
            w.u8(0);
            w.u16(0);
            w.range(p.attrs);
        }
        w.u32(self.attrs.len() as u32);
        for a in &self.attrs {
            w.u32(a.class);
            w.u32(a.make);
        }
        w.buf
    }

    /// Reads the section. The values are checked by [`Module::verify`]; only the kinds that
    /// decide the layout are checked here.
    pub(super) fn decode(buf: &[u8]) -> Result<RttiSection, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let mut s = RttiSection::default();
        let n = r.count("type information name")?;
        for _ in 0..n {
            s.names.push(r.lp_utf8()?);
        }
        let n = r.count("type information type")?;
        for _ in 0..n {
            let kind = r.u8()?;
            let flags = r.u8()?;
            r.u16()?;
            let name = r.u32()?;
            let full_name = r.u32()?;
            let type_index = r.u32()?;
            let ext = match kind {
                TK_INTEGER | TK_INT64 | TK_WCHAR | TK_ENUMERATION => {
                    let shape = r.u8()?;
                    r.u8()?;
                    r.u16()?;
                    let min = r.i64()?;
                    let max = r.i64()?;
                    let base = r.u32()?;
                    let mut values = Vec::new();
                    if shape == SHAPE_ENUM {
                        let k = r.count("enumeration value")?;
                        for _ in 0..k {
                            values.push((r.i64()?, r.u32()?));
                        }
                    }
                    RttiExt::Ordinal {
                        shape,
                        min,
                        max,
                        base,
                        values,
                    }
                }
                TK_FLOAT => {
                    let f = r.u8()?;
                    r.u8()?;
                    r.u16()?;
                    RttiExt::Float(f)
                }
                TK_SET => RttiExt::Set {
                    base: r.u32()?,
                    bits: r.u32()?,
                },
                TK_ARRAY => RttiExt::Array {
                    elem: r.u32()?,
                    width: r.u32()?,
                    count: r.u32()?,
                    index: r.u32()?,
                },
                TK_DYNARRAY => RttiExt::DynArray { elem: r.u32()? },
                TK_RECORD | TK_CLASS => RttiExt::Struct {
                    parent: r.u32()?,
                    fields: r.range()?,
                    props: r.range()?,
                    methods: r.range()?,
                    attrs: r.range()?,
                },
                TK_INTERFACE => {
                    let parent = r.u32()?;
                    let guid: [u8; 16] = r.bytes(16)?.try_into().expect("16 bytes were read");
                    RttiExt::Intf {
                        parent,
                        guid,
                        methods: r.range()?,
                        attrs: r.range()?,
                    }
                }
                TK_CLASSREF => RttiExt::ClassRef { class: r.u32()? },
                TK_PROCEDURE | TK_METHOD => RttiExt::Proc {
                    params: r.range()?,
                    ret: r.u32()?,
                },
                TK_USTRING | TK_LSTRING | TK_POINTER => RttiExt::None,
                k => {
                    return Err(LoadError::Invalid(format!(
                        "unknown type information kind {k}"
                    )));
                }
            };
            let generic = if flags & RTTI_INSTANCE != 0 {
                let def = r.u32()?;
                let k = r.count("type argument")?;
                let mut args = Vec::with_capacity(k);
                for _ in 0..k {
                    args.push(r.u32()?);
                }
                Some((def, args))
            } else {
                None
            };
            s.types.push(RttiTypeEntry {
                kind,
                flags,
                name,
                full_name,
                type_index,
                ext,
                generic,
            });
        }
        let n = r.count("type information field")?;
        for _ in 0..n {
            let name = r.u32()?;
            let ty = r.u32()?;
            let vis = r.u8()?;
            r.u8()?;
            s.fields.push(RttiFieldEntry {
                name,
                ty,
                vis,
                width: r.u16()?,
                pos: r.u32()?,
                attrs: r.range()?,
            });
        }
        let n = r.count("type information property")?;
        for _ in 0..n {
            let name = r.u32()?;
            let ty = r.u32()?;
            let vis = r.u8()?;
            let flags = r.u8()?;
            r.u16()?;
            s.props.push(RttiPropEntry {
                name,
                ty,
                vis,
                flags,
                read: r.target()?,
                write: r.target()?,
                attrs: r.range()?,
            });
        }
        let n = r.count("type information method")?;
        for _ in 0..n {
            let name = r.u32()?;
            let kind = r.u8()?;
            let vis = r.u8()?;
            let flags = r.u8()?;
            r.u8()?;
            s.methods.push(RttiMethodEntry {
                name,
                kind,
                vis,
                flags,
                params: r.range()?,
                ret: r.u32()?,
                call: r.target()?,
                attrs: r.range()?,
            });
        }
        let n = r.count("type information parameter")?;
        for _ in 0..n {
            let name = r.u32()?;
            let ty = r.u32()?;
            let flags = r.u8()?;
            r.u8()?;
            r.u16()?;
            s.params.push(RttiParamEntry {
                name,
                ty,
                flags,
                attrs: r.range()?,
            });
        }
        let n = r.count("type information attribute")?;
        for _ in 0..n {
            s.attrs.push(RttiAttrEntry {
                class: r.u32()?,
                make: r.u32()?,
            });
        }
        if r.pos != buf.len() {
            return Err(LoadError::Invalid(
                "type information section has trailing bytes".into(),
            ));
        }
        Ok(s)
    }

    /// The name at an index of the name table (`?` past its end, for the dump of a module that
    /// failed to verify).
    pub fn name(&self, i: u32) -> &str {
        self.names.get(i as usize).map_or("?", String::as_str)
    }

    /// The entry of a type information number (1 and up).
    pub fn entry(&self, number: u32) -> Option<&RttiTypeEntry> {
        number
            .checked_sub(1)
            .and_then(|i| self.types.get(i as usize))
    }
}

impl Module {
    /// The checks of the type information section (`docs/spec/bytecode.md` 3.3).
    pub(super) fn verify_rtti(&self) -> Result<(), LoadError> {
        let Some(s) = &self.rtti else {
            return Ok(());
        };
        let bad = |m: String| Err(LoadError::Invalid(format!("type information: {m}")));
        let name = |i: u32| (i as usize) < s.names.len();
        let ty = |i: u32| i as usize <= s.types.len();
        let some_ty = |i: u32| i != 0 && ty(i);
        let range = |r: Range, len: usize| u64::from(r.0) + u64::from(r.1) <= len as u64;
        let func = |f: u32, params: u32| {
            f == NONE_INDEX
                || self
                    .funcs
                    .get(f as usize)
                    .is_some_and(|x| u32::from(x.params) == params)
        };
        let mut seen_index = std::collections::HashSet::new();
        for (k, t) in s.types.iter().enumerate() {
            let what = format!("type {}", k + 1);
            if !name(t.name) || !name(t.full_name) {
                return bad(format!("{what} has a name out of range"));
            }
            if t.type_index != NONE_INDEX {
                if self.type_entry(t.type_index).is_none() {
                    return bad(format!("{what} has a type index out of range"));
                }
                if !seen_index.insert(t.type_index) {
                    return bad(format!("{what} repeats type index {}", t.type_index));
                }
            }
            // The slots of the record or instance the fields are in
            let slots = (t.type_index != NONE_INDEX)
                .then(|| self.type_entry(t.type_index))
                .flatten()
                .filter(|e| e.kind == TYPE_RECORD || e.kind == TYPE_CLASS)
                .map(|e| e.slots);
            let ok = match &t.ext {
                RttiExt::Ordinal {
                    shape,
                    min,
                    max,
                    base,
                    values,
                } => {
                    let kind_ok = match *shape {
                        0..=5 => t.kind == TK_INTEGER,
                        6 | 7 => t.kind == TK_INT64,
                        SHAPE_CHAR => t.kind == TK_WCHAR,
                        SHAPE_ENUM => t.kind == TK_ENUMERATION,
                        _ => false,
                    };
                    kind_ok
                        && (*shape == 7 || min <= max)
                        && ty(*base)
                        && values.iter().all(|(_, n)| name(*n))
                }
                RttiExt::Float(f) => matches!(*f, FT_SINGLE | FT_DOUBLE | FT_CURR),
                RttiExt::Set { base, bits } => some_ty(*base) && *bits <= 65536,
                RttiExt::Array {
                    elem, width, index, ..
                } => some_ty(*elem) && some_ty(*index) && (1..=16).contains(width),
                RttiExt::DynArray { elem } => some_ty(*elem),
                RttiExt::Struct {
                    parent,
                    fields,
                    props,
                    methods,
                    attrs,
                } => {
                    let class_ok = if t.kind == TK_CLASS {
                        // A class is in the type section (`docs/spec/rtti.md` 1.2)
                        slots.is_some()
                            && self
                                .type_entry(t.type_index)
                                .is_some_and(|e| e.kind == TYPE_CLASS)
                    } else {
                        *parent == 0
                    };
                    let fields_ok = range(*fields, s.fields.len())
                        && s.fields[fields.0 as usize..][..fields.1 as usize]
                            .iter()
                            .all(|f| {
                                slots.is_some_and(|n| {
                                    u64::from(f.pos) + u64::from(f.width) <= u64::from(n)
                                })
                            });
                    let props_ok = range(*props, s.props.len())
                        && s.props[props.0 as usize..][..props.1 as usize]
                            .iter()
                            .all(|p| {
                                [p.read, p.write].iter().all(|a| {
                                    a.kind != TARGET_FIELD
                                        || slots.is_some_and(|n| {
                                            u64::from(a.value) + u64::from(a.width) <= u64::from(n)
                                        })
                                })
                            });
                    class_ok
                        && ty(*parent)
                        && fields_ok
                        && props_ok
                        && range(*methods, s.methods.len())
                        && range(*attrs, s.attrs.len())
                }
                RttiExt::Intf {
                    parent,
                    methods,
                    attrs,
                    ..
                } => {
                    ty(*parent) && range(*methods, s.methods.len()) && range(*attrs, s.attrs.len())
                }
                RttiExt::ClassRef { class } => some_ty(*class),
                RttiExt::Proc { params, ret } => ty(*ret) && range(*params, s.params.len()),
                RttiExt::None => true,
            };
            if !ok {
                return bad(format!("{what} is malformed"));
            }
            if matches!(t.kind, TK_CLASS | TK_RECORD) && t.type_index == NONE_INDEX {
                return bad(format!("{what} is a record or class without a type index"));
            }
            if let Some((def, args)) = &t.generic
                && (!name(*def) || !args.iter().all(|a| some_ty(*a)))
            {
                return bad(format!("{what} has a malformed instantiation"));
            }
        }
        // The size of the virtual method table of the class whose methods and properties the
        // entries of the methods and properties tables are
        let mut vmt_of = vec![None; s.methods.len()];
        let mut prop_vmt_of = vec![None; s.props.len()];
        for t in &s.types {
            if let RttiExt::Struct { methods, props, .. } = t.ext
                && t.kind == TK_CLASS
                && let Some(e) = self.type_entry(t.type_index)
            {
                for m in &mut vmt_of[methods.0 as usize..][..methods.1 as usize] {
                    *m = Some(e.vmt.len());
                }
                for p in &mut prop_vmt_of[props.0 as usize..][..props.1 as usize] {
                    *p = Some(e.vmt.len());
                }
            }
        }
        let bridge = |f: u32| {
            f == NONE_INDEX
                || self
                    .funcs
                    .get(f as usize)
                    .is_some_and(|x| x.params == 3 && is_bridge_name(&x.name))
        };
        let target = |t: &RttiTarget, vmt: Option<usize>| match t.kind {
            TARGET_NONE => true,
            TARGET_FIELD => true,
            TARGET_FUNC => (t.value as usize) < self.funcs.len(),
            TARGET_VIRTUAL => vmt.is_some_and(|n| (t.value as usize) < n),
            _ => false,
        } && bridge(t.bridge);
        for (k, f) in s.fields.iter().enumerate() {
            if !name(f.name)
                || !some_ty(f.ty)
                || !matches!(f.vis, VIS_PUBLIC | VIS_PUBLISHED)
                || !range(f.attrs, s.attrs.len())
            {
                return bad(format!("field {k} is malformed"));
            }
        }
        for (k, p) in s.props.iter().enumerate() {
            if !name(p.name)
                || !some_ty(p.ty)
                || !matches!(p.vis, VIS_PUBLIC | VIS_PUBLISHED)
                || !target(&p.read, prop_vmt_of[k])
                || !target(&p.write, prop_vmt_of[k])
                || !range(p.attrs, s.attrs.len())
            {
                return bad(format!("property {k} is malformed"));
            }
        }
        for (k, m) in s.methods.iter().enumerate() {
            if !name(m.name)
                || m.kind > 5
                || !matches!(m.vis, VIS_PUBLIC | VIS_PUBLISHED)
                || !range(m.params, s.params.len())
                || !ty(m.ret)
                || m.call.kind == TARGET_FIELD
                || !target(&m.call, vmt_of[k])
                || !range(m.attrs, s.attrs.len())
            {
                return bad(format!("method {k} is malformed"));
            }
        }
        for (k, p) in s.params.iter().enumerate() {
            if !name(p.name) || !some_ty(p.ty) || !range(p.attrs, s.attrs.len()) {
                return bad(format!("parameter {k} is malformed"));
            }
        }
        for (k, a) in s.attrs.iter().enumerate() {
            if !some_ty(a.class) || !func(a.make, 0) {
                return bad(format!("attribute {k} is malformed"));
            }
        }
        Ok(())
    }
}

/// The `.rtti` lines of the dump (`docs/spec/bytecode.md` 5.2).
pub(super) fn disassemble(s: &RttiSection, out: &mut String) {
    let ti = |n: u32| {
        if n == 0 {
            "-".to_string()
        } else {
            format!("ti{n}")
        }
    };
    let opt = |n: u32| {
        if n == NONE_INDEX {
            "-".to_string()
        } else {
            format!("f{n}")
        }
    };
    let target = |t: &RttiTarget| match t.kind {
        TARGET_FIELD => format!("@{}/{}", t.value, t.width),
        TARGET_FUNC => format!("f{}", t.value),
        TARGET_VIRTUAL => format!("v{}", t.value),
        _ => "-".to_string(),
    };
    let slice = |r: Range, len: usize| {
        let start = (r.0 as usize).min(len);
        start..(start + r.1 as usize).min(len)
    };
    let attr_lines = |r: Range, indent: &str, out: &mut String| {
        for a in &s.attrs[slice(r, s.attrs.len())] {
            out.push_str(&format!(
                "{indent}attr {} make={}\n",
                ti(a.class),
                opt(a.make)
            ));
        }
    };
    let params_text = |r: Range| {
        s.params[slice(r, s.params.len())]
            .iter()
            .map(|p| {
                let mut flags = String::new();
                for (bit, word) in [(1, "var "), (2, "const "), (32, "out ")] {
                    if p.flags & bit != 0 {
                        flags.push_str(word);
                    }
                }
                let array = if p.flags & 4 != 0 { " array" } else { "" };
                format!("{flags}{}: {}{array}", s.name(p.name), ti(p.ty))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    out.push_str(".rtti\n");
    for (k, t) in s.types.iter().enumerate() {
        let kind = KIND_NAMES.get(t.kind as usize).copied().unwrap_or("?");
        let mut line = format!("  {}: {kind} {}", k + 1, s.name(t.full_name));
        if s.name(t.name) != s.name(t.full_name) {
            line.push_str(&format!(" name={}", s.name(t.name)));
        }
        if t.type_index != NONE_INDEX {
            line.push_str(&format!(" ty=ty{}", t.type_index));
        }
        if t.flags & RTTI_MEMBERS != 0 {
            line.push_str(" members");
        }
        if t.flags & RTTI_REF_COUNTED != 0 {
            line.push_str(" refcounted");
        }
        match &t.ext {
            RttiExt::Ordinal {
                shape,
                min,
                max,
                base,
                values,
            } => {
                let (min, max) = if *shape == 7 {
                    ((*min as u64).to_string(), (*max as u64).to_string())
                } else {
                    (min.to_string(), max.to_string())
                };
                line.push_str(&format!(" shape={shape} min={min} max={max}"));
                if *base != 0 {
                    line.push_str(&format!(" base={}", ti(*base)));
                }
                if !values.is_empty() {
                    let v: Vec<String> = values
                        .iter()
                        .map(|(o, n)| format!("{}={o}", s.name(*n)))
                        .collect();
                    line.push_str(&format!(" ({})", v.join(", ")));
                }
            }
            RttiExt::Float(f) => line.push_str(&format!(" float={f}")),
            RttiExt::Set { base, bits } => {
                line.push_str(&format!(" base={} bits={bits}", ti(*base)));
            }
            RttiExt::Array {
                elem,
                width,
                count,
                index,
            } => line.push_str(&format!(
                " elem={} width={width} count={count} index={}",
                ti(*elem),
                ti(*index)
            )),
            RttiExt::DynArray { elem } => line.push_str(&format!(" elem={}", ti(*elem))),
            RttiExt::Struct { parent, .. } if *parent != 0 => {
                line.push_str(&format!(" parent={}", ti(*parent)));
            }
            RttiExt::Intf { parent, guid, .. } => {
                if *parent != 0 {
                    line.push_str(&format!(" parent={}", ti(*parent)));
                }
                if t.flags & RTTI_GUID != 0 {
                    line.push_str(&format!(" guid={}", super::guid_text(guid)));
                }
            }
            RttiExt::ClassRef { class } => line.push_str(&format!(" class={}", ti(*class))),
            RttiExt::Proc { params, ret } => {
                line.push_str(&format!(" ({}): {}", params_text(*params), ti(*ret)));
            }
            _ => {}
        }
        if let Some((def, args)) = &t.generic {
            let a: Vec<String> = args.iter().map(|a| ti(*a)).collect();
            line.push_str(&format!(" generic={}<{}>", s.name(*def), a.join(", ")));
        }
        out.push_str(&line);
        out.push('\n');
        let (fields, props, methods, attrs) = match &t.ext {
            RttiExt::Struct {
                fields,
                props,
                methods,
                attrs,
                ..
            } => (*fields, *props, *methods, *attrs),
            RttiExt::Intf { methods, attrs, .. } => ((0, 0), (0, 0), *methods, *attrs),
            _ => continue,
        };
        attr_lines(attrs, "    ", out);
        for f in &s.fields[slice(fields, s.fields.len())] {
            out.push_str(&format!(
                "    field {}: {} @{}/{}\n",
                s.name(f.name),
                ti(f.ty),
                f.pos,
                f.width
            ));
            attr_lines(f.attrs, "      ", out);
        }
        for p in &s.props[slice(props, s.props.len())] {
            let acc = |declared: bool, a: &RttiTarget| {
                if declared {
                    target(a)
                } else {
                    "none".to_string()
                }
            };
            out.push_str(&format!(
                "    property {}: {} read={} write={}\n",
                s.name(p.name),
                ti(p.ty),
                acc(p.flags & 1 != 0, &p.read),
                acc(p.flags & 2 != 0, &p.write)
            ));
            attr_lines(p.attrs, "      ", out);
        }
        for m in &s.methods[slice(methods, s.methods.len())] {
            let kind = [
                "procedure",
                "function",
                "constructor",
                "destructor",
                "class procedure",
                "class function",
            ]
            .get(m.kind as usize)
            .copied()
            .unwrap_or("?");
            let mut flags = String::new();
            if m.flags & 1 != 0 {
                flags.push_str(" static");
            }
            if m.flags & 2 != 0 {
                flags.push_str(" virtual");
            }
            out.push_str(&format!(
                "    method {} {kind}{flags} ({}): {} call={} bridge={}\n",
                s.name(m.name),
                params_text(m.params),
                ti(m.ret),
                target(&m.call),
                opt(m.call.bridge)
            ));
            attr_lines(m.attrs, "      ", out);
            for p in &s.params[slice(m.params, s.params.len())] {
                if p.attrs.1 != 0 {
                    out.push_str(&format!("      param {}\n", s.name(p.name)));
                    attr_lines(p.attrs, "        ", out);
                }
            }
        }
    }
}

/// The fields of a type entry `RttiTypeField` (`I129`) reads (`docs/spec/bytecode.md` 2.24).
/// A field the entry's kind has not reads 0; a name reads its index in the name table, a type
/// its type information number, a missing name or function -1.
pub mod type_field {
    pub const KIND: i64 = 0;
    pub const FLAGS: i64 = 1;
    pub const NAME: i64 = 2;
    pub const FULL_NAME: i64 = 3;
    /// The type index of the type section, -1 if none.
    pub const TYPE_INDEX: i64 = 4;
    pub const SHAPE: i64 = 5;
    pub const MIN: i64 = 6;
    pub const MAX: i64 = 7;
    /// The original type of an ordinal type, the base type of a set.
    pub const BASE: i64 = 8;
    pub const VALUE_COUNT: i64 = 9;
    /// The ordinal and name of the value `Index` of an enumeration.
    pub const VALUE_ORD: i64 = 10;
    pub const VALUE_NAME: i64 = 11;
    pub const FLOAT_TYPE: i64 = 12;
    pub const SET_BITS: i64 = 13;
    /// The element type of a static or dynamic array, its width, the number of elements and the
    /// index type of a static array.
    pub const ELEM: i64 = 14;
    pub const ELEM_WIDTH: i64 = 15;
    pub const ELEM_COUNT: i64 = 16;
    pub const INDEX_TYPE: i64 = 17;
    /// The parent of a class or interface, the class of a class reference.
    pub const PARENT: i64 = 18;
    /// The GUID of an interface as two halves (`Index` 0 the first 8 bytes, 1 the last 8, each
    /// read little endian).
    pub const GUID: i64 = 19;
    pub const CLASS: i64 = 20;
    /// The first entry and the number of entries of each member table, and of the parameters of
    /// a procedural type.
    pub const FIELDS_FIRST: i64 = 21;
    pub const FIELDS_COUNT: i64 = 22;
    pub const PROPS_FIRST: i64 = 23;
    pub const PROPS_COUNT: i64 = 24;
    pub const METHODS_FIRST: i64 = 25;
    pub const METHODS_COUNT: i64 = 26;
    pub const ATTRS_FIRST: i64 = 27;
    pub const ATTRS_COUNT: i64 = 28;
    pub const PARAMS_FIRST: i64 = 29;
    pub const PARAMS_COUNT: i64 = 30;
    /// The result type of a procedural type.
    pub const RESULT: i64 = 31;
    /// The fully qualified name of an instantiation's definition (-1 if not one), the number of
    /// its type arguments and the type argument `Index`.
    pub const GENERIC_DEF: i64 = 32;
    pub const GENERIC_ARG_COUNT: i64 = 33;
    pub const GENERIC_ARG: i64 = 34;
}

/// The fields of the member table entries `RttiMemberField` (`I130`) reads, by table.
pub mod member_field {
    /// Every table but the attributes': the name, the type.
    pub const NAME: i64 = 0;
    pub const TYPE: i64 = 1;
    /// Fields: the visibility, the width and first slot, the attributes.
    pub const FIELD_VIS: i64 = 2;
    pub const FIELD_WIDTH: i64 = 3;
    pub const FIELD_POS: i64 = 4;
    pub const FIELD_ATTRS_FIRST: i64 = 5;
    pub const FIELD_ATTRS_COUNT: i64 = 6;
    /// Properties: the visibility, the flags, the read and write accessors (kind, width, value,
    /// bridge), the attributes.
    pub const PROP_VIS: i64 = 2;
    pub const PROP_FLAGS: i64 = 3;
    pub const PROP_READ_KIND: i64 = 4;
    pub const PROP_READ_WIDTH: i64 = 5;
    pub const PROP_READ_VALUE: i64 = 6;
    pub const PROP_READ_BRIDGE: i64 = 7;
    pub const PROP_WRITE_KIND: i64 = 8;
    pub const PROP_WRITE_WIDTH: i64 = 9;
    pub const PROP_WRITE_VALUE: i64 = 10;
    pub const PROP_WRITE_BRIDGE: i64 = 11;
    pub const PROP_ATTRS_FIRST: i64 = 12;
    pub const PROP_ATTRS_COUNT: i64 = 13;
    /// Methods: the name, the `TMethodKind`, the visibility, the flags, the parameters, the
    /// result type, the call target (kind, value, bridge), the attributes.
    pub const METHOD_KIND: i64 = 1;
    pub const METHOD_VIS: i64 = 2;
    pub const METHOD_FLAGS: i64 = 3;
    pub const METHOD_PARAMS_FIRST: i64 = 4;
    pub const METHOD_PARAMS_COUNT: i64 = 5;
    pub const METHOD_RESULT: i64 = 6;
    pub const METHOD_CALL_KIND: i64 = 7;
    pub const METHOD_CALL_VALUE: i64 = 8;
    pub const METHOD_CALL_BRIDGE: i64 = 9;
    pub const METHOD_ATTRS_FIRST: i64 = 10;
    pub const METHOD_ATTRS_COUNT: i64 = 11;
    /// Parameters: the name, the type, the `TParamFlags`, the attributes.
    pub const PARAM_FLAGS: i64 = 2;
    pub const PARAM_ATTRS_FIRST: i64 = 3;
    pub const PARAM_ATTRS_COUNT: i64 = 4;
    /// Attributes: the class, the function making the instance.
    pub const ATTR_CLASS: i64 = 0;
    pub const ATTR_MAKE: i64 = 1;
}

impl RttiSection {
    /// `RttiTypeField` (`I129`): the field `field` (index `index` of a list) of the entry of
    /// number `info`; 0 for a number out of range or a field the entry has not.
    pub fn type_field(&self, info: i64, field: i64, index: i64) -> i64 {
        use type_field as f;
        let Some(t) = u32::try_from(info).ok().and_then(|n| self.entry(n)) else {
            return 0;
        };
        let opt = |v: u32| if v == NONE_INDEX { -1 } else { i64::from(v) };
        let at = |len: usize| usize::try_from(index).ok().filter(|i| *i < len);
        let first = |r: Range| i64::from(r.0);
        let count = |r: Range| i64::from(r.1);
        let none: Range = (0, 0);
        let (fields, props, methods, attrs, params) = match &t.ext {
            RttiExt::Struct {
                fields,
                props,
                methods,
                attrs,
                ..
            } => (*fields, *props, *methods, *attrs, none),
            RttiExt::Intf { methods, attrs, .. } => (none, none, *methods, *attrs, none),
            RttiExt::Proc { params, .. } => (none, none, none, none, *params),
            _ => (none, none, none, none, none),
        };
        match (field, &t.ext) {
            (f::KIND, _) => i64::from(t.kind),
            (f::FLAGS, _) => i64::from(t.flags),
            (f::NAME, _) => i64::from(t.name),
            (f::FULL_NAME, _) => i64::from(t.full_name),
            (f::TYPE_INDEX, _) => opt(t.type_index),
            (f::SHAPE, RttiExt::Ordinal { shape, .. }) => i64::from(*shape),
            (f::MIN, RttiExt::Ordinal { min, .. }) => *min,
            (f::MAX, RttiExt::Ordinal { max, .. }) => *max,
            (f::BASE, RttiExt::Ordinal { base, .. }) | (f::BASE, RttiExt::Set { base, .. }) => {
                i64::from(*base)
            }
            (f::VALUE_COUNT, RttiExt::Ordinal { values, .. }) => values.len() as i64,
            (f::VALUE_ORD, RttiExt::Ordinal { values, .. }) => {
                at(values.len()).map_or(0, |i| values[i].0)
            }
            (f::VALUE_NAME, RttiExt::Ordinal { values, .. }) => {
                at(values.len()).map_or(-1, |i| i64::from(values[i].1))
            }
            (f::FLOAT_TYPE, RttiExt::Float(k)) => i64::from(*k),
            (f::SET_BITS, RttiExt::Set { bits, .. }) => i64::from(*bits),
            (f::ELEM, RttiExt::Array { elem, .. }) | (f::ELEM, RttiExt::DynArray { elem }) => {
                i64::from(*elem)
            }
            (f::ELEM_WIDTH, RttiExt::Array { width, .. }) => i64::from(*width),
            (f::ELEM_COUNT, RttiExt::Array { count, .. }) => i64::from(*count),
            (f::INDEX_TYPE, RttiExt::Array { index, .. }) => i64::from(*index),
            (f::PARENT, RttiExt::Struct { parent, .. })
            | (f::PARENT, RttiExt::Intf { parent, .. }) => i64::from(*parent),
            (f::GUID, RttiExt::Intf { guid, .. }) => match index {
                0 | 1 => {
                    let k = index as usize * 8;
                    i64::from_le_bytes(guid[k..k + 8].try_into().expect("8 bytes"))
                }
                _ => 0,
            },
            (f::CLASS, RttiExt::ClassRef { class }) => i64::from(*class),
            (f::FIELDS_FIRST, _) => first(fields),
            (f::FIELDS_COUNT, _) => count(fields),
            (f::PROPS_FIRST, _) => first(props),
            (f::PROPS_COUNT, _) => count(props),
            (f::METHODS_FIRST, _) => first(methods),
            (f::METHODS_COUNT, _) => count(methods),
            (f::ATTRS_FIRST, _) => first(attrs),
            (f::ATTRS_COUNT, _) => count(attrs),
            (f::PARAMS_FIRST, _) => first(params),
            (f::PARAMS_COUNT, _) => count(params),
            (f::RESULT, RttiExt::Proc { ret, .. }) => i64::from(*ret),
            (f::GENERIC_DEF, _) => t.generic.as_ref().map_or(-1, |(d, _)| i64::from(*d)),
            (f::GENERIC_ARG_COUNT, _) => t.generic.as_ref().map_or(0, |(_, a)| a.len() as i64),
            (f::GENERIC_ARG, _) => t
                .generic
                .as_ref()
                .and_then(|(_, a)| at(a.len()).map(|i| i64::from(a[i])))
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// `RttiMemberField` (`I130`): the field `field` of the entry `index` of the member table
    /// `table` ([`TABLE_FIELDS`] …); 0 for an entry out of range or a field it has not.
    pub fn member_field(&self, table: i64, index: i64, field: i64) -> i64 {
        use member_field as f;
        let Ok(i) = usize::try_from(index) else {
            return 0;
        };
        let opt = |v: u32| if v == NONE_INDEX { -1 } else { i64::from(v) };
        let target = |t: &RttiTarget, k: i64| match k {
            0 => i64::from(t.kind),
            1 => i64::from(t.width),
            2 => i64::from(t.value),
            _ => opt(t.bridge),
        };
        match table {
            t if t == i64::from(TABLE_FIELDS) => self.fields.get(i).map_or(0, |e| match field {
                f::NAME => i64::from(e.name),
                f::TYPE => i64::from(e.ty),
                f::FIELD_VIS => i64::from(e.vis),
                f::FIELD_WIDTH => i64::from(e.width),
                f::FIELD_POS => i64::from(e.pos),
                f::FIELD_ATTRS_FIRST => i64::from(e.attrs.0),
                f::FIELD_ATTRS_COUNT => i64::from(e.attrs.1),
                _ => 0,
            }),
            t if t == i64::from(TABLE_PROPS) => self.props.get(i).map_or(0, |e| match field {
                f::NAME => i64::from(e.name),
                f::TYPE => i64::from(e.ty),
                f::PROP_VIS => i64::from(e.vis),
                f::PROP_FLAGS => i64::from(e.flags),
                f::PROP_READ_KIND..=f::PROP_READ_BRIDGE => {
                    target(&e.read, field - f::PROP_READ_KIND)
                }
                f::PROP_WRITE_KIND..=f::PROP_WRITE_BRIDGE => {
                    target(&e.write, field - f::PROP_WRITE_KIND)
                }
                f::PROP_ATTRS_FIRST => i64::from(e.attrs.0),
                f::PROP_ATTRS_COUNT => i64::from(e.attrs.1),
                _ => 0,
            }),
            t if t == i64::from(TABLE_METHODS) => self.methods.get(i).map_or(0, |e| match field {
                f::NAME => i64::from(e.name),
                f::METHOD_KIND => i64::from(e.kind),
                f::METHOD_VIS => i64::from(e.vis),
                f::METHOD_FLAGS => i64::from(e.flags),
                f::METHOD_PARAMS_FIRST => i64::from(e.params.0),
                f::METHOD_PARAMS_COUNT => i64::from(e.params.1),
                f::METHOD_RESULT => i64::from(e.ret),
                f::METHOD_CALL_KIND => i64::from(e.call.kind),
                f::METHOD_CALL_VALUE => i64::from(e.call.value),
                f::METHOD_CALL_BRIDGE => opt(e.call.bridge),
                f::METHOD_ATTRS_FIRST => i64::from(e.attrs.0),
                f::METHOD_ATTRS_COUNT => i64::from(e.attrs.1),
                _ => 0,
            }),
            t if t == i64::from(TABLE_PARAMS) => self.params.get(i).map_or(0, |e| match field {
                f::NAME => i64::from(e.name),
                f::TYPE => i64::from(e.ty),
                f::PARAM_FLAGS => i64::from(e.flags),
                f::PARAM_ATTRS_FIRST => i64::from(e.attrs.0),
                f::PARAM_ATTRS_COUNT => i64::from(e.attrs.1),
                _ => 0,
            }),
            t if t == i64::from(TABLE_ATTRS) => self.attrs.get(i).map_or(0, |e| match field {
                f::ATTR_CLASS => i64::from(e.class),
                f::ATTR_MAKE => opt(e.make),
                _ => 0,
            }),
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySourceProvider;

    /// The module of a program naming a record, a class and an enumeration with `TypeInfo`.
    fn module() -> Module {
        let src = "program T;
                   type
                     TColor = (Red, Green);
                     TPoint = record X, Y: Integer; end;
                     TShape = class Name: string; end;
                   begin
                     WriteLn(TypeInfo(TColor) <> nil, TypeInfo(TPoint) <> nil,
                       TypeInfo(TShape) <> nil);
                   end.";
        compile(src)
    }

    fn compile(src: &str) -> Module {
        let mut provider = MemorySourceProvider::new();
        let out = crate::driver::compile(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &crate::driver::test_options(),
        );
        out.stages.module.expect("a module")
    }

    fn rtti(m: &mut Module) -> &mut RttiSection {
        m.rtti.as_mut().expect("a type information section")
    }

    // The section is written and read back as it was (bytecode.md 3.2).
    #[test]
    fn section_round_trips() {
        let m = module();
        let back = Module::decode(&m.encode(crate::VERSION), crate::VERSION).expect("valid");
        assert_eq!(back.rtti, m.rtti);
        assert_eq!(m.rtti.as_ref().map(|s| s.types.len()), Some(4));
    }

    // An ordinary three-argument function passes the range and arity checks but is not a
    // bridge (bytecode.md 3.3).
    #[test]
    fn verify_rejects_a_three_argument_non_bridge() {
        let mut m = compile(
            "program T;
             uses System.Rtti;
             type TShape = class procedure Draw; end;
             procedure TShape.Draw; begin end;
             procedure Other(A, B, C: Integer);
             begin WriteLn(A, B, C); end;
             begin
               Other(1, 2, 3);
               var Ctx := TRttiContext.Create;
               var Shape := TShape.Create;
               Ctx.GetType(TShape).GetMethod('Draw').Invoke(Shape, []);
               Shape.Free;
             end.",
        );
        assert_eq!(m.verify(), Ok(()));
        let other = m.funcs.iter().position(|f| f.name == "T.Other").unwrap();
        assert_eq!(m.funcs[other].params, 3);
        let s = rtti(&mut m);
        let method = s
            .methods
            .iter()
            .position(|entry| s.name(entry.name) == "Draw")
            .unwrap();
        assert_ne!(s.methods[method].call.bridge, NONE_INDEX);
        s.methods[method].call.bridge = other as u32;
        assert_eq!(
            m.verify(),
            Err(LoadError::Invalid(format!(
                "type information: method {method} is malformed"
            )))
        );
    }

    // The loading rejects a section whose indexes leave their tables, a class without its entry
    // of the type section, and a field outside the slots of its record (bytecode.md 3.3).
    #[test]
    fn verify_rejects_malformed_sections() {
        type Break = fn(&mut Module);
        let cases: [(&str, Break); 7] = [
            ("type 1 has a name out of range", |m| {
                rtti(m).types[0].name = 1000;
            }),
            ("type 2 is malformed", |m| {
                if let RttiExt::Ordinal { base, .. } = &mut rtti(m).types[1].ext {
                    *base = 99;
                }
            }),
            ("type 4 is malformed", |m| {
                rtti(m).types[3].type_index = NONE_INDEX;
            }),
            ("type 3 is a record or class without a type index", |m| {
                // A record without fields has no slot to check its fields against
                rtti(m).types[2].type_index = NONE_INDEX;
                if let RttiExt::Struct { fields, .. } = &mut rtti(m).types[2].ext {
                    *fields = (0, 0);
                }
            }),
            ("type 3 is malformed", |m| {
                rtti(m).fields.push(RttiFieldEntry {
                    name: 0,
                    ty: 1,
                    vis: 2,
                    width: 1,
                    pos: 2,
                    attrs: (0, 0),
                });
                if let RttiExt::Struct { fields, .. } = &mut rtti(m).types[2].ext {
                    *fields = (0, 1);
                }
            }),
            ("method 0 is malformed", |m| {
                rtti(m).methods.push(RttiMethodEntry {
                    name: 0,
                    kind: 0,
                    vis: 2,
                    flags: 0,
                    params: (0, 0),
                    ret: 0,
                    call: RttiTarget {
                        kind: TARGET_FUNC,
                        width: 0,
                        value: 0,
                        // `<main>` takes no argument, a bridge function three
                        bridge: 0,
                    },
                    attrs: (0, 0),
                });
            }),
            ("attribute 0 is malformed", |m| {
                rtti(m).attrs.push(RttiAttrEntry { class: 0, make: 0 });
            }),
        ];
        let base = module();
        // The entries in number order (by their fully qualified names; `TObject` is the parent
        // of `TShape`)
        let r = base.rtti.as_ref().expect("a type information section");
        let names: Vec<&str> = r.types.iter().map(|t| r.name(t.full_name)).collect();
        assert_eq!(
            names,
            ["System.TObject", "T.TColor", "T.TPoint", "T.TShape"]
        );
        assert_eq!(base.verify(), Ok(()));
        for (what, break_it) in cases {
            let mut m = base.clone();
            break_it(&mut m);
            let err = m.verify().expect_err(what);
            assert_eq!(
                err,
                LoadError::Invalid(format!("type information: {what}")),
                "{what}"
            );
        }
    }
}
