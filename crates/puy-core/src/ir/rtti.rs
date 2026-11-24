//! The types that have type information (`docs/spec/rtti.md` chapter 1): which types the type
//! information section holds, at which stage, and their members.
//!
//! The reachability walk (`super::reach`) tells what the kept functions ask for: the types of
//! `TypeInfo(T)`, and which intrinsics reading the section they use ([`Demand`]). From these
//! [`Collect`] closes the set of types (1.2, 1.3): the types reached from those of the
//! identification stage, and, when the members are read, the types of the members of the
//! records, classes and interfaces. Each class it adds goes into the type section, so the walk
//! goes on with its tables; the walk and the closure take turns until neither adds anything.

use std::collections::HashMap;

use super::reach::DROPPED;
use super::{RttiAttr, RttiCall, RttiField, RttiMethod, RttiParam, RttiProp, RttiType};
use crate::bytecode::op;
use crate::bytecode::rtti::{PROP_READABLE, PROP_WRITABLE, VIS_PUBLIC, VIS_PUBLISHED};
use crate::sema::hir::{Program, RoutineId};
use crate::sema::invoke;
use crate::sema::types::{
    Accessor, ClassMethod, MethodKind, ProcFamily, SelfKind, Ty, TypeId, TypeKind, TypeTable,
    Visibility,
};
use crate::source::TextRange;
use crate::syntax::ast::ParamMode;

/// The `TMethodKind` values (`docs/spec/rtti.md` 5.1).
const MK_PROCEDURE: u8 = 0;
const MK_FUNCTION: u8 = 1;
const MK_CONSTRUCTOR: u8 = 2;
const MK_DESTRUCTOR: u8 = 3;
const MK_CLASS_PROCEDURE: u8 = 4;
const MK_CLASS_FUNCTION: u8 = 5;

/// The flags of a method entry (`docs/spec/bytecode.md` 3.2): static, virtual.
const METHOD_STATIC: u8 = 1;
const METHOD_VIRTUAL: u8 = 2;

/// The `TParamFlags` bits of a parameter entry (`docs/spec/bytecode.md` 3.2): bit `i` is the
/// value of ordinal `i` (`pfVar`, `pfConst`, `pfArray`, `pfAddress`, `pfReference`, `pfOut`).
const PF_VAR: u8 = 1;
const PF_CONST: u8 = 2;
const PF_ARRAY: u8 = 4;
const PF_OUT: u8 = 32;

/// What the kept functions ask of the type information (`docs/spec/rtti.md` 1.2, 1.3).
#[derive(Default)]
pub(super) struct Demand {
    /// A function reads the section (`I129`–`I135`, `System.Rtti`) or makes a `TValue` of an
    /// instance (`I127`, `I128`): every class in the type section has type information (1.2).
    pub any: bool,
    /// A function reads the member tables (`I130`): the records, classes and interfaces have
    /// their members (1.3).
    pub members: bool,
    /// A function finds types by name (`I132`): the types of the user's package are added
    /// (1.3).
    pub names: bool,
    /// `TRttiMethod.Invoke` is reached (`RttiMethodRef`, `I141`): the methods' functions and
    /// bridges are entries (1.5).
    pub invoke: bool,
    /// A property is read or written by its methods (`RttiPropRef`, `I144`).
    pub props: bool,
    /// The attributes are returned (`RttiAttrFunc`, `I143`): their functions are entries.
    pub attrs: bool,
}

impl Demand {
    /// Notes what an instruction of a kept function asks for: the intrinsics reading the
    /// section.
    pub(super) fn note(&mut self, inst: &super::Inst) {
        if inst.op != op::INTRINSIC {
            return;
        }
        let super::InstKind::Plain(ops) = &inst.kind else {
            return;
        };
        let Some(super::Operand::Imm(id)) = ops.last() else {
            return;
        };
        let id = *id as u8;
        // A `TValue` records the class of the instance it holds (`ValueOfObject`, and the
        // elements `ValueArray` reads; `docs/spec/rtti.md` 3.3)
        if (op::intr::RTTI_TYPE_FIELD..=op::intr::RTTI_CLASS_TYPE).contains(&id)
            || id == op::intr::VALUE_OF_OBJECT
            || id == op::intr::VALUE_ARRAY
        {
            self.any = true;
        }
        // The API returning the attributes is one returning members too (1.3)
        self.members |= id == op::intr::RTTI_MEMBER_FIELD || id == op::intr::RTTI_ATTR_FUNC;
        self.names |= id == op::intr::RTTI_FIND_TYPE;
        self.invoke |= id == op::intr::RTTI_METHOD_REF;
        self.props |= id == op::intr::RTTI_PROP_REF;
        self.attrs |= id == op::intr::RTTI_ATTR_FUNC;
    }

    /// What the functions of the members' entries depend on.
    fn calls(&self) -> (bool, bool, bool) {
        (self.invoke, self.props, self.attrs)
    }
}

/// The types with type information found so far.
pub(super) struct Collect<'p> {
    p: &'p Program,
    types: &'p TypeTable,
    /// In the order they were found.
    found: Vec<Ty>,
    index: HashMap<Ty, usize>,
    /// The entries whose identification stage has been closed over (a prefix of `found`).
    ident_done: usize,
    /// The entries whose members have been closed over.
    members_done: Vec<bool>,
    /// The entries whose classes were given to the type section (a prefix of `found`).
    classes_done: usize,
    /// The types of the user's package were added (1.3).
    package_added: bool,
    /// The attributes written on each declaration: (unit, declared name) to the classes and the
    /// attributes' indexes in `Program::attrs`.
    attrs: HashMap<(u32, TextRange), Vec<(Ty, usize)>>,
    /// The function of each routine (`Module::func_index`).
    func_index: &'p [u32],
    /// The calls (`Demand::calls`) whose functions each entry's members gave the walk.
    entries_done: Vec<Option<(bool, bool, bool)>>,
}

impl<'p> Collect<'p> {
    pub(super) fn new(p: &'p Program, types: &'p TypeTable, func_index: &'p [u32]) -> Self {
        // The attribute classes by their fully qualified name
        let mut classes: HashMap<&str, Ty> = HashMap::new();
        for (k, d) in types.defs.iter().enumerate() {
            if let (TypeKind::Class { .. }, Some(n)) = (&d.kind, &d.full_name) {
                classes
                    .entry(n.as_str())
                    .or_insert(Ty::Class(TypeId(k as u32)));
            }
        }
        let mut attrs: HashMap<(u32, TextRange), Vec<(Ty, usize)>> = HashMap::new();
        for (n, a) in p.attrs.iter().enumerate() {
            if let Some(c) = classes.get(a.class.as_str()) {
                attrs.entry((a.unit, a.decl)).or_default().push((*c, n));
            }
        }
        Collect {
            p,
            types,
            found: Vec::new(),
            index: HashMap::new(),
            ident_done: 0,
            members_done: Vec::new(),
            classes_done: 0,
            package_added: false,
            attrs,
            func_index,
            entries_done: Vec::new(),
        }
    }

    /// Whether any type has type information.
    pub(super) fn is_empty(&self) -> bool {
        self.found.is_empty()
    }

    /// Adds a type with (at least) the identification stage.
    pub(super) fn add(&mut self, t: Ty) {
        let Some(t) = normalize(self.types, t) else {
            return;
        };
        if !self.index.contains_key(&t) {
            self.index.insert(t, self.found.len());
            self.found.push(t);
            self.members_done.push(false);
            self.entries_done.push(None);
        }
    }

    /// Closes the set over what `demand` asks for, given the classes of the type section
    /// (`classes`; added when the section is read at all). Returns the classes added, which go
    /// into the type section, and the functions the members' entries call (the methods, the
    /// bridges, the attributes' functions; `docs/spec/rtti.md` 1.5), which are entries of the
    /// reachability walk.
    pub(super) fn close(&mut self, demand: &Demand, classes: &[TypeId]) -> (Vec<TypeId>, Vec<u32>) {
        let mut funcs = Vec::new();
        if demand.any {
            for c in classes {
                self.add(Ty::Class(*c));
            }
        }
        if demand.names && !self.package_added {
            self.package_added = true;
            for t in package_types(self.p, self.types) {
                self.add(t);
            }
        }
        loop {
            let mut progress = false;
            while self.ident_done < self.found.len() {
                let t = self.found[self.ident_done];
                self.ident_done += 1;
                for k in ident_kids(self.types, t) {
                    self.add(k);
                }
                progress = true;
            }
            if demand.members {
                for i in 0..self.found.len() {
                    if self.members_done[i] || !has_members(self.found[i]) {
                        continue;
                    }
                    self.members_done[i] = true;
                    let m = self.members(self.found[i], demand);
                    for k in member_types(&m) {
                        self.add(k);
                    }
                    progress = true;
                }
                for i in 0..self.found.len() {
                    if !has_members(self.found[i]) || self.entries_done[i] == Some(demand.calls()) {
                        continue;
                    }
                    self.entries_done[i] = Some(demand.calls());
                    funcs.extend(entry_funcs(&self.members(self.found[i], demand)));
                }
            }
            if !progress {
                break;
            }
        }
        // Classes found by any rule go into the type section (1.2)
        let before = std::mem::replace(&mut self.classes_done, self.found.len());
        let classes = self.found[before..]
            .iter()
            .filter_map(|t| match t {
                Ty::Class(c) => Some(*c),
                _ => None,
            })
            .collect();
        (classes, funcs)
    }

    /// The entries of the section, in the order they were found (codegen numbers them).
    pub(super) fn finish(self, demand: &Demand) -> Vec<RttiType> {
        self.found
            .iter()
            .map(|&t| {
                if demand.members && has_members(t) {
                    let mut m = self.members(t, demand);
                    m.members = true;
                    m
                } else {
                    RttiType {
                        ty: t,
                        members: false,
                        fields: Vec::new(),
                        props: Vec::new(),
                        methods: Vec::new(),
                        attrs: Vec::new(),
                    }
                }
            })
            .collect()
    }

    /// The attributes written on the declaration `at` of `unit`, with their functions when
    /// `demand` returns the attributes.
    fn attrs_of(&self, unit: Option<u32>, at: TextRange, demand: &Demand) -> Vec<RttiAttr> {
        let Some(list) = unit.and_then(|u| self.attrs.get(&(u, at))) else {
            return Vec::new();
        };
        list.iter()
            .map(|&(class, n)| RttiAttr {
                class,
                make: demand
                    .attrs
                    .then(|| self.p.rtti_funcs.attrs.get(&n))
                    .flatten()
                    .and_then(|r| self.func(*r)),
            })
            .collect()
    }

    /// The function of a routine (`None` if it has none).
    fn func(&self, r: RoutineId) -> Option<u32> {
        self.func_index
            .get(r.0 as usize)
            .copied()
            .filter(|f| *f != DROPPED)
    }

    /// What `Invoke` calls for the method `m` (index `k`) of `owner`, and its bridge, when
    /// `wanted` (`docs/spec/rtti.md` 1.5, 7.2).
    fn method_call(
        &self,
        owner: TypeId,
        k: u32,
        m: &ClassMethod,
        wanted: bool,
    ) -> (RttiCall, Option<u32>) {
        if !wanted {
            return (RttiCall::None, None);
        }
        let interface = matches!(self.types.get(owner).kind, TypeKind::Interface { .. });
        let call = match (m.vmt, m.routine) {
            _ if interface => RttiCall::None,
            (Some(slot), _) if matches!(self.types.get(owner).kind, TypeKind::Class { .. }) => {
                RttiCall::Virtual(slot)
            }
            (_, Some(r)) => self.func(r).map_or(RttiCall::None, RttiCall::Func),
            _ => RttiCall::None,
        };
        let bridge = self
            .p
            .rtti_funcs
            .bridges
            .get(&(owner, k))
            .and_then(|r| self.func(*r));
        (call, bridge)
    }

    /// The members a record, class or interface holds in the section (`docs/spec/rtti.md` 1.4):
    /// its own public fields, properties and methods, in declaration order, and its attributes;
    /// what the methods and properties call as `demand` asks (1.5).
    fn members(&self, t: Ty, demand: &Demand) -> RttiType {
        let types = self.types;
        let mut out = RttiType {
            ty: t,
            members: false,
            fields: Vec::new(),
            props: Vec::new(),
            methods: Vec::new(),
            attrs: Vec::new(),
        };
        let Some(id) = t.type_id() else {
            return out;
        };
        let def = types.get(id);
        let (fields, methods, props, unit) = match &def.kind {
            TypeKind::Record {
                fields,
                methods,
                properties,
                unit,
                ..
            } => {
                let fs = fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (f.name.clone(), f.ty, f.vis, f.span, i as u32))
                    .collect::<Vec<_>>();
                (fs, methods.as_slice(), properties.as_slice(), *unit)
            }
            TypeKind::Class {
                fields,
                methods,
                properties,
                unit,
                ..
            } => {
                let fs = fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty, f.vis, f.span, f.slot))
                    .collect::<Vec<_>>();
                (fs, methods.as_slice(), properties.as_slice(), *unit)
            }
            TypeKind::Interface { methods, unit, .. } => {
                (Vec::new(), methods.as_slice(), &[][..], *unit)
            }
            _ => return out,
        };
        let builtin = TypeTable::is_builtin(id);
        out.attrs = self.attrs_of(unit, def.span, demand);
        // The first slot of a field: its place among the flat slots of the record or instance
        let place = |slot: u32| match t {
            Ty::Class(c) => types.class_field_offset(c, slot),
            _ => types.field_offset(t, slot),
        };
        if !builtin {
            for (name, ty, vis, span, slot) in fields {
                if !vis.is_public() {
                    continue;
                }
                out.fields.push(RttiField {
                    name,
                    ty: strong(types, ty),
                    vis: member_visibility(vis),
                    pos: place(slot),
                    width: types.slot_width(ty),
                    attrs: self.attrs_of(unit, span, demand),
                });
            }
            for pr in props {
                if !invoke::lists_property(pr) {
                    continue;
                }
                let target = |a: &Option<Accessor>| match a {
                    Some(Accessor::Field { slot, ty, .. }) => {
                        (RttiCall::Field(place(*slot), types.slot_width(*ty)), None)
                    }
                    Some(Accessor::Method(_)) => match invoke::accessor_method(types, id, a) {
                        Some((owner, k, m)) => self.method_call(owner, k, &m, demand.props),
                        None => (RttiCall::None, None),
                    },
                    _ => (RttiCall::None, None),
                };
                let mut flags = 0;
                if pr.read.is_some() {
                    flags |= PROP_READABLE;
                }
                if pr.write.is_some() {
                    flags |= PROP_WRITABLE;
                }
                out.props.push(RttiProp {
                    name: pr.name.clone(),
                    ty: strong(types, pr.ty),
                    vis: member_visibility(pr.vis),
                    flags,
                    read: target(&pr.read),
                    write: target(&pr.write),
                    attrs: self.attrs_of(unit, pr.span, demand),
                });
            }
        }
        for (k, m) in methods.iter().enumerate() {
            if !invoke::lists_method(types, id, m) {
                continue;
            }
            let class_level = m.self_kind != SelfKind::Instance;
            let kind = match m.kind {
                MethodKind::Constructor => MK_CONSTRUCTOR,
                MethodKind::Destructor => MK_DESTRUCTOR,
                MethodKind::Function if class_level => MK_CLASS_FUNCTION,
                MethodKind::Function => MK_FUNCTION,
                _ if class_level => MK_CLASS_PROCEDURE,
                _ => MK_PROCEDURE,
            };
            let mut flags = 0;
            if m.self_kind == SelfKind::Static {
                flags |= METHOD_STATIC;
            }
            if m.vmt.is_some() && !matches!(def.kind, TypeKind::Interface { .. }) {
                flags |= METHOD_VIRTUAL;
            }
            // The parameters' names and attributes are those of the method's declaration
            let heading = m
                .routine
                .and_then(|r| types.decl_params.get(&r))
                .map_or(&[][..], Vec::as_slice);
            let params = m
                .params
                .iter()
                .enumerate()
                .map(|(i, (ty, mode))| {
                    let decl = heading.get(i);
                    let mut flags = match mode {
                        ParamMode::Value => 0,
                        ParamMode::Const => PF_CONST,
                        ParamMode::Var => PF_VAR,
                        ParamMode::Out => PF_OUT,
                    };
                    if matches!(ty, Ty::OpenArray(_)) {
                        flags |= PF_ARRAY;
                    }
                    RttiParam {
                        name: decl.map_or_else(String::new, |d| d.0.clone()),
                        ty: strong(types, *ty),
                        flags,
                        attrs: decl.map_or_else(Vec::new, |d| self.attrs_of(unit, d.1, demand)),
                    }
                })
                .collect();
            let (call, bridge) = self.method_call(id, k as u32, m, demand.invoke);
            out.methods.push(RttiMethod {
                name: m.name.clone(),
                kind,
                vis: member_visibility(m.vis),
                flags,
                params,
                ret: m.ret.map(|r| strong(types, r)),
                call,
                bridge,
                attrs: self.attrs_of(unit, m.span, demand),
            });
        }
        out
    }
}

/// The declared visibility of a member selected for RTTI (`docs/spec/rtti.md` 5.1).
fn member_visibility(vis: Visibility) -> u8 {
    match vis {
        Visibility::Public => VIS_PUBLIC,
        Visibility::Published => VIS_PUBLISHED,
        _ => unreachable!("only public and published members enter RTTI"),
    }
}

/// The type with type information a type stands for: an open array is its dynamic array, a
/// `[weak]` field's type its class, a distinct type erased into a subrange that subrange.
/// `None` for the types of no value.
pub fn normalize(types: &TypeTable, t: Ty) -> Option<Ty> {
    Some(match t {
        Ty::Error | Ty::Param(_) | Ty::Helper(_) => return None,
        Ty::OpenArray(id) => Ty::DynArray(id),
        Ty::Weak(id) => Ty::Class(id),
        Ty::Distinct(id) if matches!(types.get(id).kind, TypeKind::Subrange { .. }) => {
            Ty::Subrange(id)
        }
        t => t,
    })
}

/// The storage type of a member as a value: a `[weak]` field holds an instance of its class.
fn strong(types: &TypeTable, t: Ty) -> Ty {
    match t {
        Ty::Weak(_) => types.strong(t),
        t => t,
    }
}

/// Whether a type has members in the section when the members are read (1.3).
fn has_members(t: Ty) -> bool {
    matches!(t, Ty::Record(_) | Ty::Class(_) | Ty::Intf(_))
}

/// The types an entry of the identification stage names (1.2): the elements, the base and
/// original types, the parents, the class of a class reference, the type arguments, the
/// parameters and result of a procedural type.
fn ident_kids(types: &TypeTable, t: Ty) -> Vec<Ty> {
    let mut out = Vec::new();
    let Some(id) = t.type_id() else {
        return out;
    };
    if let Some(r) = types.distinct_roots.get(&id) {
        out.push(*r);
    }
    match &types.get(id).kind {
        TypeKind::Subrange { base, .. } if !types.distinct_roots.contains_key(&id) => {
            out.push(*base)
        }
        TypeKind::Distinct { base } => out.push(*base),
        TypeKind::Set { base, .. } => out.push(*base),
        TypeKind::Array { index, elem, .. } => {
            out.push(*index);
            out.push(*elem);
        }
        TypeKind::DynArray { elem, .. } => out.push(*elem),
        TypeKind::Class {
            parent: Some(p), ..
        } => out.push(Ty::Class(*p)),
        TypeKind::Interface {
            parent: Some(p), ..
        } => out.push(Ty::Intf(*p)),
        TypeKind::ClassRef { class } => out.push(Ty::Class(*class)),
        TypeKind::Proc { params, ret, .. } => {
            out.extend(params.iter().map(|p| p.ty));
            out.extend(*ret);
        }
        _ => {}
    }
    if let Some(i) = types.instance_of(id) {
        out.extend(i.args.iter().copied());
    }
    out
}

/// The types the members of an entry name: of the fields, properties, parameters and results,
/// and the attribute classes.
fn member_types(m: &RttiType) -> Vec<Ty> {
    let classes = |a: &[RttiAttr]| a.iter().map(|a| a.class).collect::<Vec<_>>();
    let mut out: Vec<Ty> = classes(&m.attrs);
    for f in &m.fields {
        out.push(f.ty);
        out.extend(classes(&f.attrs));
    }
    for p in &m.props {
        out.push(p.ty);
        out.extend(classes(&p.attrs));
    }
    for me in &m.methods {
        out.extend(me.ret);
        out.extend(classes(&me.attrs));
        for p in &me.params {
            out.push(p.ty);
            out.extend(classes(&p.attrs));
        }
    }
    out
}

/// The functions the members of an entry call (`docs/spec/rtti.md` 1.5): the methods' and the
/// accessors' functions, the bridges, the attributes' functions.
fn entry_funcs(m: &RttiType) -> Vec<u32> {
    let mut out = Vec::new();
    let mut call = |c: &RttiCall, b: &Option<u32>| {
        if let RttiCall::Func(f) = c {
            out.push(*f);
        }
        out.extend(*b);
    };
    for p in &m.props {
        call(&p.read.0, &p.read.1);
        call(&p.write.0, &p.write.1);
    }
    for me in &m.methods {
        call(&me.call, &me.bridge);
    }
    let mut attrs: Vec<&RttiAttr> = m.attrs.iter().collect();
    for f in &m.fields {
        attrs.extend(&f.attrs);
    }
    for p in &m.props {
        attrs.extend(&p.attrs);
    }
    for me in &m.methods {
        attrs.extend(&me.attrs);
        for p in &me.params {
            attrs.extend(&p.attrs);
        }
    }
    out.extend(attrs.iter().filter_map(|a| a.make));
    out
}

/// The types declared in the units of the user's package (`docs/spec/rtti.md` 1.3): not those of
/// the standard library and the extensions, nor the generic definitions.
fn package_types(p: &Program, types: &TypeTable) -> Vec<Ty> {
    p.type_decls
        .iter()
        .filter(|d| {
            let u = &p.units[d.unit as usize];
            !u.stdlib && u.extension.is_none()
        })
        .filter(|d| {
            // The declaration that makes the type, not an alias naming another one
            d.ty.type_id().is_some_and(|id| {
                let def = types.get(id);
                !types.is_generic(id)
                    && !types.has_param(d.ty)
                    && !matches!(def.kind, TypeKind::Helper { .. } | TypeKind::Pending)
                    && def
                        .name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&d.name))
            })
        })
        .map(|d| d.ty)
        .collect()
}

/// The procedural family of a `Ty::Callable` (for its kind): `tkMethod` or `tkProcedure`.
pub fn callable_is_method(types: &TypeTable, id: TypeId) -> bool {
    matches!(
        types.get(id).kind,
        TypeKind::Proc {
            family: ProcFamily::Method,
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use crate::source::MemorySourceProvider;

    /// A unit of the standard library whose functions read the section the way `System.Rtti`
    /// (M5-50) will: the members (`RttiMemberField`), the types by name (`RttiFindType`), and
    /// only the type entries (`RttiTypeField`).
    /// Only such a unit sees these built-ins, so the member stage is reached from the tests here.
    const MEMBERS_STD: puy_stdlib::Unit = puy_stdlib::Unit {
        name: "MembersStd",
        files: &[puy_stdlib::File {
            path: "MembersStd.pas",
            text: "unit MembersStd;
interface
function Member(I: Integer): Int64;
function Find(const Name: string): PTypeInfo;
function KindOf(P: PTypeInfo): Int64;
type
  TStdThing = class
    Hidden: Integer;
  end;
implementation
function Member(I: Integer): Int64;
begin
  Result := RttiMemberField(0, I, 0);
end;
function Find(const Name: string): PTypeInfo;
begin
  Result := RttiFindType(Name);
end;
function KindOf(P: PTypeInfo): Int64;
begin
  Result := RttiTypeField(P, 0, 0);
end;
end.
",
        }],
        pieces: &[],
        parts: &[],
    };

    /// A unit of the standard library whose functions call through the entries the way
    /// `System.Rtti`'s `Invoke`, `GetValue` and `GetAttributes` do (`RttiMethodRef`,
    /// `RttiPropRef`, `RttiAttrFunc`), besides reading the member tables.
    const INVOKE_STD: puy_stdlib::Unit = puy_stdlib::Unit {
        name: "InvokeStd",
        files: &[puy_stdlib::File {
            path: "InvokeStd.pas",
            text: "unit InvokeStd;
interface
function Member(I: Integer): Int64;
function Target(I: Integer; const V: TValue): TProc;
function Accessor(I: Integer; const V: TValue): TProc;
function Make(I: Integer): Boolean;
implementation
type
  TMake = function: TCustomAttribute;
function Member(I: Integer): Int64;
begin
  Result := RttiMemberField(0, I, 0);
end;
function Target(I: Integer; const V: TValue): TProc;
begin
  Result := RttiMethodRef(I, V);
end;
function Accessor(I: Integer; const V: TValue): TProc;
begin
  Result := RttiPropRef(I, 0, V);
end;
function Make(I: Integer): Boolean;
begin
  Result := Assigned(RttiAttrFunc(TMake, I));
end;
end.
",
        }],
        pieces: &[],
        parts: &[],
    };

    /// The `.rtti` lines of the bytecode dump of `src`, compiled with [`MEMBERS_STD`].
    fn rtti_dump(src: &str) -> String {
        rtti_dump_with(src, MEMBERS_STD)
    }

    /// The `.rtti` lines of the bytecode dump of `src`, compiled with the unit `std`.
    fn rtti_dump_with(src: &str, std: puy_stdlib::Unit) -> String {
        let mut provider = MemorySourceProvider::new();
        let out = crate::driver::compile_with_stdlib(
            "t.pas",
            src.as_bytes().to_vec(),
            &mut provider,
            &crate::driver::test_options(),
            &[std],
        );
        assert!(
            !out.diagnostics.iter().any(|d| d.is_error()),
            "{:#?}",
            out.diagnostics
        );
        let m = out.stages.module.expect("a module");
        let text = crate::bytecode::disassemble(&m);
        let start = text.find(".rtti\n").expect("a type information section");
        let end = text[start..]
            .find("\n.")
            .map_or(text.len(), |e| start + e + 1);
        text[start..end].to_string()
    }

    // Reading the member tables gives the records, classes and interfaces their public fields,
    // properties, methods with their parameters, and attributes, in declaration order, and not
    // the private ones nor the generic methods; the types the members name get entries too
    // (rtti.md 1.3, 1.4). Without a search by name, a type no entry reaches has none.
    #[test]
    fn member_stage_lists_the_public_members() {
        let got = rtti_dump(
            "program T;
             uses MembersStd;
             type
               TTagAttribute = class(TCustomAttribute)
               end;
               TUnused = record
                 A: Integer;
               end;
               [TTag]
               TShape = class
               private
                 FSecret: Integer;
                 FSize: Double;
               public
                 [TTag] Name: string;
                 constructor Create(const AName: string);
                 procedure Draw([TTag] Times: Integer; var Done: Boolean); virtual;
                 class function Count: Integer; static;
                 procedure Each<T>(const X: T);
                 property Size: Double read FSize write FSize;
               end;
             constructor TShape.Create(const AName: string); begin Name := AName; end;
             procedure TShape.Each<T>(const X: T); begin end;
             procedure TShape.Draw(Times: Integer; var Done: Boolean); begin end;
             class function TShape.Count: Integer; begin Result := 0; end;
             begin
               WriteLn(Member(0), TypeInfo(TShape) <> nil);
             end.",
        );
        assert_eq!(
            got,
            ".rtti
  1: enumeration System.Boolean name=Boolean shape=9 min=0 max=1 (False=0, True=1)
  2: float System.Double name=Double float=1
  3: integer System.Integer name=Integer shape=4 min=-2147483648 max=2147483647
  4: ustring System.string name=string
  5: class System.TCustomAttribute name=TCustomAttribute ty=ty25 members refcounted parent=ti7
  6: class System.TObject name=TObject ty=ty19 members
    method Create constructor (): - call=- bridge=-
    method Destroy destructor virtual (): - call=- bridge=-
  7: class System.TRefObject name=TRefObject ty=ty20 members refcounted parent=ti6
  8: class T.TShape name=TShape ty=ty64 members parent=ti6
    attr ti9 make=-
    field Name: ti4 @2/1
      attr ti9 make=-
    property Size: ti2 read=@1/1 write=@1/1
    method Create constructor (const AName: ti4): - call=- bridge=-
    method Draw procedure virtual (Times: ti3, var Done: ti1): - call=- bridge=-
      param Times
        attr ti9 make=-
    method Count class function static (): ti3 call=- bridge=-
  9: class T.TTagAttribute name=TTagAttribute ty=ty65 members refcounted parent=ti5
"
        );
    }

    // When `Invoke`, the reads through the properties' methods and the attributes are reached,
    // the entries name what they call: the function of a method (the table's entry of a virtual
    // one), the shared bridge of its shape, the method reading a property (private too), and
    // the function making each attribute (rtti.md 1.5, 7.2 to 7.4); a destructor has no bridge.
    #[test]
    fn reached_calls_fill_the_entries() {
        let got = rtti_dump_with(
            "program T;
             uses InvokeStd;
             type
               TTagAttribute = class(TCustomAttribute)
               end;
               [TTag]
               TShape = class
               private
                 function GetArea: Integer;
               public
                 function Grow(N: Integer): Integer;
                 function Scale(N: Integer): Integer; virtual;
                 destructor Destroy; override;
                 property Area: Integer read GetArea;
               end;
             function TShape.GetArea: Integer; begin Result := 1; end;
             function TShape.Grow(N: Integer): Integer; begin Result := N; end;
             function TShape.Scale(N: Integer): Integer; begin Result := N; end;
             destructor TShape.Destroy; begin inherited; end;
             begin
               WriteLn(Member(0), Make(0), TypeInfo(TShape) <> nil);
               Target(0, TValue.Empty);
               Accessor(0, TValue.Empty);
             end.",
            INVOKE_STD,
        );
        let shape = got
            .lines()
            .skip_while(|l| !l.contains("T.TShape"))
            .take_while(|l| !l.contains("T.TTagAttribute"))
            .collect::<Vec<_>>()
            .join("\n");
        // `Grow` and `Scale` share the bridge of `(Integer): Integer` (`f11`)
        assert_eq!(
            shape,
            "  5: class T.TShape name=TShape ty=ty64 members parent=ti3
    attr ti6 make=f13
    property Area: ti1 read=f6 write=none
    method Grow function (N: ti1): ti1 call=f7 bridge=f11
    method Scale function virtual (N: ti1): ti1 call=v6 bridge=f11
    method Destroy destructor virtual (): - call=v0 bridge=-"
        );
    }

    // A search by name gives the types of the user's package their entries, whether or not
    // anything else reaches them, and not the types of the standard library (rtti.md 1.3); with
    // no function reading the members, they stay at the identification stage.
    #[test]
    fn search_by_name_adds_the_package_types() {
        let got = rtti_dump(
            "program T;
             uses MembersStd;
             type
               TUnused = record
                 A: Integer;
               end;
               TKind = (kA, kB);
             begin
               WriteLn(Find('T.TUnused') <> nil, Find('MembersStd.TStdThing') = nil);
             end.",
        );
        assert_eq!(
            got,
            ".rtti
  1: enumeration T.TKind name=TKind shape=9 min=0 max=1 (kA=0, kB=1)
  2: record T.TUnused name=TUnused ty=ty64
"
        );
    }

    // When a function reads the section at all, every class in the type section gets an entry
    // (with its parents), though no `TypeInfo` names it (rtti.md 1.2).
    #[test]
    fn reading_the_section_gives_the_created_classes_entries() {
        let got = rtti_dump(
            "program T;
             uses MembersStd;
             type
               TFoo = class
               end;
             var F: TFoo;
             begin
               F := TFoo.Create;
               WriteLn(KindOf(nil));
               F.Free;
             end.",
        );
        assert_eq!(
            got,
            ".rtti
  1: class System.TObject name=TObject ty=ty19
  2: class T.TFoo name=TFoo ty=ty64 parent=ti1
"
        );
    }
}
