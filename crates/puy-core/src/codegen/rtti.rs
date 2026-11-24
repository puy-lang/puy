//! The type information section (`docs/spec/rtti.md` chapter 7, `docs/spec/bytecode.md` 3.2)
//! from the types `ir::reach` found: their numbers, and their entries.

use std::collections::HashMap;

use super::ConstPool;
use crate::bytecode::NONE_INDEX;
use crate::bytecode::rtti::*;
use crate::ir::{self, RttiType};
use crate::sema::types::{Ty, TypeKind, TypeTable};
use crate::syntax::ast::ParamMode;

/// The type information numbers (`docs/spec/rtti.md` 7.1): the entries in the order of their
/// fully qualified names (ignoring case, then with it), from 1. Returns the entries' indexes
/// in that order and the number of each type.
pub(super) fn numbers(m: &ir::Module) -> (Vec<usize>, HashMap<Ty, u32>) {
    let names: Vec<String> = m.rtti.iter().map(|t| m.types.rtti_name(t.ty)).collect();
    let lower: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let mut order: Vec<usize> = (0..m.rtti.len()).collect();
    order.sort_by(|&a, &b| {
        lower[a]
            .cmp(&lower[b])
            .then_with(|| names[a].cmp(&names[b]))
            .then(a.cmp(&b))
    });
    let numbers = order
        .iter()
        .enumerate()
        .map(|(n, &i)| (m.rtti[i].ty, n as u32 + 1))
        .collect();
    (order, numbers)
}

/// The name of a type in the type information (`docs/spec/rtti.md` 2.4): its fully qualified
/// name without the unit and the routines and types it is declared in. An anonymous type's is
/// its fully qualified name.
fn short_name(types: &TypeTable, ty: Ty, full: &str) -> String {
    let named = ty.type_id().is_none_or(|id| types.get(id).name.is_some());
    if !named {
        return full.to_string();
    }
    let head = full.find('<').unwrap_or(full.len());
    match full[..head].rfind('.') {
        Some(dot) => full[dot + 1..].to_string(),
        None => full.to_string(),
    }
}

/// Builds the section.
struct Build<'a, 't> {
    types: &'a TypeTable,
    numbers: &'a HashMap<Ty, u32>,
    names: HashMap<String, u32>,
    out: RttiSection,
    consts: &'a mut ConstPool<'t>,
}

impl Build<'_, '_> {
    fn name(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.names.get(s) {
            return i;
        }
        let i = self.out.names.len() as u32;
        self.out.names.push(s.to_string());
        self.names.insert(s.to_string(), i);
        i
    }

    /// The number of a type (0 for none). Every type an entry names has an entry
    /// (`ir::rtti` closes the set).
    fn info(&self, ty: Ty) -> u32 {
        let t = ir::rtti::normalize(self.types, ty).expect("a type of a value");
        *self
            .numbers
            .get(&t)
            .unwrap_or_else(|| panic!("`{}` has type information", self.types.full_name(t)))
    }

    fn attrs(&mut self, attrs: &[ir::RttiAttr]) -> Range {
        let first = self.out.attrs.len() as u32;
        for a in attrs {
            let class = self.info(a.class);
            self.out.attrs.push(RttiAttrEntry {
                class,
                make: a.make.unwrap_or(NONE_INDEX),
            });
        }
        (first, attrs.len() as u32)
    }

    fn params(&mut self, params: &[ir::RttiParam]) -> Range {
        // The attributes of each parameter go first, so that the parameters are consecutive
        let attrs: Vec<Range> = params.iter().map(|p| self.attrs(&p.attrs)).collect();
        let first = self.out.params.len() as u32;
        for (p, attrs) in params.iter().zip(attrs) {
            let name = self.name(&p.name);
            let ty = self.info(p.ty);
            self.out.params.push(RttiParamEntry {
                name,
                ty,
                flags: p.flags,
                attrs,
            });
        }
        (first, params.len() as u32)
    }

    /// The kind and the part that depends on it of a type that is not a record, class or
    /// interface, by its root type for a distinct type (`base` is then the distinct type's
    /// original type).
    fn scalar(&mut self, ty: Ty) -> (u8, RttiExt) {
        let types = self.types;
        let ordinal = |shape: u8, min: i128, max: i128, base: u32, values| RttiExt::Ordinal {
            shape,
            min: min as i64,
            max: max as i64,
            base,
            values,
        };
        if let Some(id) = ty.type_id()
            && let Some(&root) = types.distinct_roots.get(&id)
        {
            // A distinct type: its root's kind and range under its own name
            let (kind, ext) = self.scalar(root);
            let base = self.info(root);
            return match ext {
                RttiExt::Ordinal {
                    shape, min, max, ..
                } => {
                    let (min, max) = match types.get(id).kind {
                        TypeKind::Subrange { low, high, .. } => (low as i64, high as i64),
                        _ => (min, max),
                    };
                    (
                        kind,
                        RttiExt::Ordinal {
                            shape,
                            min,
                            max,
                            base,
                            values: Vec::new(),
                        },
                    )
                }
                ext => (kind, ext),
            };
        }
        match ty {
            Ty::ShortInt
            | Ty::Byte
            | Ty::SmallInt
            | Ty::Word
            | Ty::Integer
            | Ty::Cardinal
            | Ty::Int64
            | Ty::UInt64 => {
                let shape = ty.width_code().expect("an integer type has a width");
                let (lo, hi) = ty.range().expect("an integer type has a range");
                let kind = if matches!(ty, Ty::Int64 | Ty::UInt64) {
                    TK_INT64
                } else {
                    TK_INTEGER
                };
                (kind, ordinal(shape, lo, hi, 0, Vec::new()))
            }
            Ty::Char => (TK_WCHAR, ordinal(SHAPE_CHAR, 0, 0xFFFF, 0, Vec::new())),
            Ty::Boolean => {
                let values = vec![(0, self.name("False")), (1, self.name("True"))];
                (TK_ENUMERATION, ordinal(SHAPE_ENUM, 0, 1, 0, values))
            }
            Ty::Enum(_) => {
                let (lo, hi) = types.ord_range(ty).unwrap_or((0, 0));
                let values = types
                    .enum_values(ty)
                    .iter()
                    .map(|v| (v.ord as i64, v.name.clone()))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|(o, n)| (o, self.name(&n)))
                    .collect();
                (TK_ENUMERATION, ordinal(SHAPE_ENUM, lo, hi, 0, values))
            }
            Ty::Subrange(id) => {
                let TypeKind::Subrange { base, low, high } = types.get(id).kind else {
                    unreachable!("a subrange type");
                };
                let (kind, ext) = self.scalar(base);
                let RttiExt::Ordinal { shape, .. } = ext else {
                    unreachable!("the base of a subrange is ordinal");
                };
                let base = self.info(base);
                (kind, ordinal(shape, low, high, base, Vec::new()))
            }
            Ty::Single => (TK_FLOAT, RttiExt::Float(FT_SINGLE)),
            Ty::Double => (TK_FLOAT, RttiExt::Float(FT_DOUBLE)),
            Ty::Currency => (TK_FLOAT, RttiExt::Float(FT_CURR)),
            Ty::String => (TK_USTRING, RttiExt::None),
            Ty::Utf8String => (TK_LSTRING, RttiExt::None),
            Ty::TypeInfo => (TK_POINTER, RttiExt::None),
            Ty::Set(_) | Ty::BigSet(_) => {
                let base = self.info(types.set_base(ty).expect("a set has a base type"));
                (
                    TK_SET,
                    RttiExt::Set {
                        base,
                        bits: types.set_bits(ty),
                    },
                )
            }
            Ty::Array(_) => {
                let (index, lo, hi, elem) = types.array(ty).expect("a static array");
                (
                    TK_ARRAY,
                    RttiExt::Array {
                        elem: self.info(elem),
                        width: types.slot_width(elem),
                        count: (hi - lo + 1) as u32,
                        index: self.info(index),
                    },
                )
            }
            Ty::DynArray(_) => {
                let elem = types.dyn_elem(ty).expect("a dynamic array");
                (
                    TK_DYNARRAY,
                    RttiExt::DynArray {
                        elem: self.info(elem),
                    },
                )
            }
            Ty::ClassRef(_) => {
                let class = types.class_ref_target(ty).expect("a class reference");
                (
                    TK_CLASSREF,
                    RttiExt::ClassRef {
                        class: self.info(Ty::Class(class)),
                    },
                )
            }
            Ty::Proc(id) | Ty::Callable(id) => {
                let TypeKind::Proc { params, ret, .. } = &types.get(id).kind else {
                    unreachable!("a procedural type");
                };
                let params: Vec<ir::RttiParam> = params
                    .iter()
                    .map(|p| ir::RttiParam {
                        name: p.name.clone(),
                        ty: p.ty,
                        flags: param_flags(p.mode, p.ty),
                        attrs: Vec::new(),
                    })
                    .collect();
                let ret = ret.map_or(0, |r| self.info(r));
                let params = self.params(&params);
                let kind =
                    if matches!(ty, Ty::Callable(_)) && ir::rtti::callable_is_method(types, id) {
                        TK_METHOD
                    } else {
                        TK_PROCEDURE
                    };
                (kind, RttiExt::Proc { params, ret })
            }
            t => unreachable!("`{}` has no type information", types.full_name(t)),
        }
    }

    fn entry(&mut self, t: &RttiType) -> RttiTypeEntry {
        let types = self.types;
        let full = types.rtti_name(t.ty);
        let short = short_name(types, t.ty, &full);
        let name = self.name(&short);
        let full_name = self.name(&full);
        let mut flags = 0;
        if t.members {
            flags |= RTTI_MEMBERS;
        }
        // Records and classes are in the type section (the fields' slots); any other type is
        // when the code uses it
        let type_index = match t.ty {
            Ty::Class(_) | Ty::Record(_) => self.consts.ty(t.ty),
            ty => self.consts.existing(ty).unwrap_or(NONE_INDEX),
        };
        let (kind, ext) = match t.ty {
            Ty::Record(_) | Ty::Class(_) | Ty::Intf(_) => {
                let attrs = self.attrs(&t.attrs);
                let fields = self.fields(t);
                let props = self.props(t);
                let methods = self.methods(t);
                match t.ty {
                    Ty::Record(_) => (
                        TK_RECORD,
                        RttiExt::Struct {
                            parent: 0,
                            fields,
                            props,
                            methods,
                            attrs,
                        },
                    ),
                    Ty::Class(id) => {
                        if let TypeKind::Class {
                            ref_counted: true, ..
                        } = types.get(id).kind
                        {
                            flags |= RTTI_REF_COUNTED;
                        }
                        let parent = types.parent(id).map_or(0, |p| self.info(Ty::Class(p)));
                        (
                            TK_CLASS,
                            RttiExt::Struct {
                                parent,
                                fields,
                                props,
                                methods,
                                attrs,
                            },
                        )
                    }
                    Ty::Intf(id) => {
                        let parent = types.parent(id).map_or(0, |p| self.info(Ty::Intf(p)));
                        let guid = match types.get(id).kind {
                            TypeKind::Interface { guid: Some(g), .. } => {
                                flags |= RTTI_GUID;
                                g
                            }
                            _ => [0; 16],
                        };
                        (
                            TK_INTERFACE,
                            RttiExt::Intf {
                                parent,
                                guid,
                                methods,
                                attrs,
                            },
                        )
                    }
                    _ => unreachable!(),
                }
            }
            ty => self.scalar(ty),
        };
        let generic =
            t.ty.type_id()
                .and_then(|id| types.instance_of(id))
                .map(|inst| {
                    // The definition's own fully qualified name (`TList<T>`)
                    let def = types.get(inst.def).full_name.clone().unwrap_or_default();
                    let args = inst.args.iter().map(|a| self.info(*a)).collect::<Vec<_>>();
                    (self.name(&def), args)
                });
        if generic.is_some() {
            flags |= RTTI_INSTANCE;
        }
        RttiTypeEntry {
            kind,
            flags,
            name,
            full_name,
            type_index,
            ext,
            generic,
        }
    }

    fn fields(&mut self, t: &RttiType) -> Range {
        let attrs: Vec<Range> = t.fields.iter().map(|f| self.attrs(&f.attrs)).collect();
        let first = self.out.fields.len() as u32;
        for (f, attrs) in t.fields.iter().zip(attrs) {
            let name = self.name(&f.name);
            let ty = self.info(f.ty);
            self.out.fields.push(RttiFieldEntry {
                name,
                ty,
                vis: f.vis,
                width: f.width as u16,
                pos: f.pos,
                attrs,
            });
        }
        (first, t.fields.len() as u32)
    }

    fn props(&mut self, t: &RttiType) -> Range {
        let attrs: Vec<Range> = t.props.iter().map(|p| self.attrs(&p.attrs)).collect();
        let first = self.out.props.len() as u32;

        for (p, attrs) in t.props.iter().zip(attrs) {
            let name = self.name(&p.name);
            let ty = self.info(p.ty);
            self.out.props.push(RttiPropEntry {
                name,
                ty,
                vis: p.vis,
                flags: p.flags,
                read: target(p.read),
                write: target(p.write),
                attrs,
            });
        }
        (first, t.props.len() as u32)
    }

    fn methods(&mut self, t: &RttiType) -> Range {
        let parts: Vec<(Range, Range)> = t
            .methods
            .iter()
            .map(|m| (self.attrs(&m.attrs), self.params(&m.params)))
            .collect();
        let first = self.out.methods.len() as u32;
        for (m, (attrs, params)) in t.methods.iter().zip(parts) {
            let name = self.name(&m.name);
            let ret = m.ret.map_or(0, |r| self.info(r));
            self.out.methods.push(RttiMethodEntry {
                name,
                kind: m.kind,
                vis: m.vis,
                flags: m.flags,
                params,
                ret,
                call: target((m.call, m.bridge)),
                attrs,
            });
        }
        (first, t.methods.len() as u32)
    }
}

/// The entry of what an accessor or a method calls (`docs/spec/bytecode.md` 3.2).
fn target((call, bridge): (ir::RttiCall, Option<u32>)) -> RttiTarget {
    // A constructor of `TObject` calls nothing but has a bridge
    let (kind, width, value) = match call {
        ir::RttiCall::None => (TARGET_NONE, 0, 0),
        ir::RttiCall::Field(pos, width) => (TARGET_FIELD, width as u16, pos),
        ir::RttiCall::Func(f) => (TARGET_FUNC, 0, f),
        ir::RttiCall::Virtual(k) => (TARGET_VIRTUAL, 0, k),
    };
    RttiTarget {
        kind,
        width,
        value,
        bridge: bridge.unwrap_or(NONE_INDEX),
    }
}

/// The `TParamFlags` of a parameter of a procedural type.
fn param_flags(mode: ParamMode, ty: Ty) -> u8 {
    let mut f = match mode {
        ParamMode::Value => 0,
        ParamMode::Const => 2,
        ParamMode::Var => 1,
        ParamMode::Out => 32,
    };
    if matches!(ty, Ty::OpenArray(_)) {
        f |= 4;
    }
    f
}

/// The section of the module, entries in number order (`None` when no type has type
/// information).
pub(super) fn section(
    m: &ir::Module,
    order: &[usize],
    numbers: &HashMap<Ty, u32>,
    consts: &mut ConstPool<'_>,
) -> Option<RttiSection> {
    if m.rtti.is_empty() {
        return None;
    }
    let mut b = Build {
        types: &m.types,
        numbers,
        names: HashMap::new(),
        out: RttiSection::default(),
        consts,
    };
    for &i in order {
        let e = b.entry(&m.rtti[i]);
        b.out.types.push(e);
    }
    Some(b.out)
}
