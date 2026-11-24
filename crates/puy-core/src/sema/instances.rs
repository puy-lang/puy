//! The instantiation of generic types (`docs/spec/generics.md` chapter 4,
//! `docs/design/generics.md` 3.1): the table of instantiations by (definition, type arguments),
//! the members of an instantiation with the type parameters replaced, and the work list that
//! copies the bodies of the generic methods into the instantiations whose methods are used.

use std::collections::{HashMap, VecDeque};

use super::RoutineInfo;
use super::hir::{Global, GlobalId, Routine, RoutineId};
use super::hirwalk::{HirVisit, walk_expr, walk_routine};
use super::types::{
    Accessor, ClassStatic, Field, Instance, IntfEntry, ProcParam, StaticKind, Ty, TypeDef, TypeId,
    TypeKind, TypeTable, VmtEntry,
};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::Span;

/// The deepest type arguments' tree an instantiation may have (`docs/spec/generics.md` 4.4).
pub(super) const MAX_NESTING: u32 = 32;

/// The routines of the instantiations: the method each instantiated method is a copy of
/// (`stubs`), and the copies made with a body, in the order they were made (the function table
/// order; `docs/spec/generics.md` 7).
#[derive(Default)]
pub(super) struct InstanceRoutines {
    /// The routine of each method of an instantiation: (the definition's method, the
    /// instantiation).
    pub stubs: HashMap<RoutineId, (RoutineId, TypeId)>,
    /// The routines made with a body (a method of an instantiation, then the routines nested in
    /// it), in order.
    pub order: Vec<RoutineId>,
    /// The instantiation each routine of `order` belongs to.
    pub owner: HashMap<RoutineId, TypeId>,
    /// The generic routine each routine of `order` copied by the work list was copied from: the
    /// routines made from the same one may be shared (`docs/spec/generics.md` 7).
    pub source: HashMap<RoutineId, RoutineId>,
    /// The definitions E0764 was reported for (once each).
    pub deep_reported: Vec<TypeId>,
    /// The `Equals` overrides generated for the reference counted classes that declare their own
    /// `Equal` (`docs/spec/equality.md` 3.3): (the class, the `Equals` routine, the operator).
    /// Their bodies are made by `sema::finish`.
    pub generated_equals: Vec<(TypeId, RoutineId, RoutineId)>,
    /// The `Equals` / `GetHashCode` overrides generated for the `[ValueEquality]` classes
    /// (`docs/spec/equality.md` 3.4): (the class, `Equals`, `GetHashCode`). Their bodies are made
    /// by `sema::finish`.
    pub generated_value_eq: Vec<(TypeId, RoutineId, RoutineId)>,
    /// Whether some body has a key equality or hash (`ExprKind::Key`) whose routine is to be
    /// filled in (`docs/spec/equality.md` 8.3).
    pub key_used: bool,
    /// The generated `<eq:…>` / `<hash:…>` function of each type (8.3).
    pub key_funcs: HashMap<(super::hir::KeyOp, Ty), RoutineId>,
    /// The full name of each generated function (`<eq:Hello.TPoint>`).
    pub generated_names: HashMap<RoutineId, String>,
    /// The generated functions in the order they were made, for the `hir` dump (9).
    pub generated: Vec<super::hir::GeneratedInfo>,
    /// The bodies whose key equalities and hashes are yet to be filled in.
    pub key_pending: Vec<RoutineId>,
    /// Where E0778 was reported (`docs/spec/rtti.md` 3.8; once each).
    pub tvalue_keys: Vec<Span>,
    /// The file of each unit (a record's generated functions are placed at its declaration).
    pub unit_files: Vec<crate::source::FileId>,
    /// The built-ins of `System.Rtti` the analyzed bodies use (`docs/spec/rtti.md` 1.5): the
    /// bridges and attribute functions `sema::finish` makes.
    pub rtti_wants: super::invoke::RttiWants,
    /// The bridges made, by name (`docs/spec/rtti.md` 7.3).
    pub bridges: HashMap<String, RoutineId>,
    /// The bridge of each method (by the type declaring it and its index among its methods).
    pub method_bridges: HashMap<(TypeId, u32), RoutineId>,
    /// The function made for each attribute of the program, by its index (7.4).
    pub attr_funcs: HashMap<usize, RoutineId>,
    /// The instantiations of generic routines and methods (`docs/spec/generics.md` 4.1): by
    /// (the generic routine, the type arguments), and back. The generic routine of a method of
    /// an instantiated type is that type's routine of the method (`stubs`).
    pub rkeys: HashMap<(RoutineId, Vec<Ty>), RoutineId>,
    pub rinst: HashMap<RoutineId, (RoutineId, Vec<Ty>)>,
    /// The routine instantiation each routine of `order` made for one belongs to.
    pub rowner: HashMap<RoutineId, RoutineId>,
    /// The unit whose analysis asked for each routine instantiation first.
    pub runit: HashMap<RoutineId, u32>,
    /// Where each routine instantiation was asked for (E0764).
    pub rorigin: HashMap<RoutineId, Span>,
    /// The instantiation whose copied body asked for each routine instantiation (the path of
    /// E0764).
    pub rfrom: HashMap<RoutineId, Asked>,
    /// The routine instantiation whose copied body asked for each type instantiation that no
    /// type instantiation asked for (the path of E0764; the others have `Instance::from`).
    pub tfrom: HashMap<TypeId, RoutineId>,
    /// The generic routines E0764 was reported for (once each).
    pub deep_routines: Vec<RoutineId>,
    /// The instantiations of types and routines in the order they were asked for (the `hir`
    /// dump; `docs/spec/generics.md` 8).
    pub asked: Vec<Asked>,
    /// The routine of each (definition's method, instantiation), made once (an instantiation's
    /// members are made again once its definition's interface table is known).
    pub stub_of: HashMap<(RoutineId, TypeId), RoutineId>,
    /// The type declaring each `class var` of a generic definition or an instantiation, and its
    /// number among the type's statics (`docs/spec/generics.md` 2.5).
    pub global_owner: HashMap<GlobalId, (TypeId, usize)>,
    /// The `class var` of each (`class var` of the definition, instantiation), made once.
    pub class_vars: HashMap<(GlobalId, TypeId), GlobalId>,
    /// The `class var`s of the instantiations, with the unit whose global variables they are
    /// placed after (the unit that asked for the instantiation first; `docs/design/generics.md`
    /// 3.2), in the order they were made.
    pub inst_globals: Vec<(GlobalId, u32)>,
}

/// An instantiation in the order of [`InstanceRoutines::asked`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Asked {
    Type(TypeId),
    Routine(RoutineId),
}

/// The parts of the analysis the instantiation changes.
pub(super) struct Inst<'x> {
    pub types: &'x mut TypeTable,
    pub routines: &'x mut Vec<RoutineInfo>,
    pub inst: &'x mut InstanceRoutines,
    pub globals: &'x mut Vec<Global>,
    pub diags: &'x mut Vec<Diagnostic>,
    /// The unit whose analysis asks.
    pub unit: u32,
    /// Where the body being copied was asked for (E0764 of the instantiations it asks for).
    pub cur_origin: Option<Span>,
    /// The instantiation whose body is being copied (the path of E0764 of the routine
    /// instantiations it asks for).
    pub cur_from: Option<Asked>,
}

/// What the type parameters are replaced with.
#[derive(Clone)]
pub(super) struct Env {
    params: Vec<TypeId>,
    args: Vec<Ty>,
    /// The instantiation being made (the path of E0764).
    from: Option<TypeId>,
}

/// What the type parameters of an instantiation's definition are replaced with (`None` for a type
/// that is not an instantiation). The members of the instantiation are made with it, and a nested
/// type looked up before they are made is replaced the same way (`Sema::static_of`), so that
/// both give the same type.
pub(super) fn instance_env(types: &TypeTable, id: TypeId) -> Option<Env> {
    let i = types.instance_of(id)?;
    Some(Env::new(
        types.type_params(i.def).to_vec(),
        i.args.clone(),
        Some(id),
    ))
}

impl Env {
    pub fn new(params: Vec<TypeId>, args: Vec<Ty>, from: Option<TypeId>) -> Env {
        Env { params, args, from }
    }

    fn get(&self, p: TypeId) -> Option<Ty> {
        self.params
            .iter()
            .position(|q| *q == p)
            .map(|i| self.args[i])
    }
}

/// The name of a type in the path of E0764: shortened inside depth 3
/// (`docs/spec/generics.md` 4.4).
fn short_name(types: &TypeTable, ty: Ty, depth: u32) -> String {
    let Some(id) = ty.type_id() else {
        return types.name(ty);
    };
    if let Some(i) = types.instance_of(id) {
        let base = base_name(types.get(i.def).name.as_deref().unwrap_or(""));
        if depth >= 3 {
            return format!("{base}<…>");
        }
        let args: Vec<String> = i
            .args
            .iter()
            .map(|a| short_name(types, *a, depth + 1))
            .collect();
        return format!("{base}<{}>", args.join(", "));
    }
    match &types.get(id).kind {
        TypeKind::DynArray { .. } if depth >= 3 => "TArray<…>".to_string(),
        TypeKind::DynArray { elem, generic } => {
            let e = short_name(types, *elem, depth + 1);
            if *generic || types.get(id).name.is_none() {
                format!("TArray<{e}>")
            } else {
                types.name(ty)
            }
        }
        _ => types.name(ty),
    }
}

/// The name of a generic definition without its type parameters (`TBox` of `TBox<T>`).
pub(super) fn base_name(name: &str) -> &str {
    name.split('<').next().unwrap_or(name)
}

impl Inst<'_> {
    /// The instantiation of `def` with `args` (`docs/spec/generics.md` 4.1): the same key is the
    /// same type. A definition's own type parameters are the definition itself. `Ty::Error`
    /// past the nesting limit (E0764, at `origin`).
    pub fn instantiate(
        &mut self,
        def: TypeId,
        args: Vec<Ty>,
        origin: Option<Span>,
        from: Option<TypeId>,
    ) -> Ty {
        let def_ty = self.def_ty(def);
        let wrap = |id| rewrap(def_ty, id);
        let own: Vec<Ty> = self
            .types
            .type_params(def)
            .iter()
            .map(|p| Ty::Param(*p))
            .collect();
        if args == own {
            return wrap(def);
        }
        if args.iter().any(|a| a.is_error()) {
            return Ty::Error;
        }
        if let Ty::DynArray(_) = def_ty {
            return self.dyn_instance(def, args);
        }
        if let Some(id) = self.types.instance_keys.get(&(def, args.clone())) {
            return wrap(*id);
        }
        let depth = 1 + args
            .iter()
            .map(|a| self.types.nesting_depth(*a))
            .max()
            .unwrap_or(0);
        if depth > MAX_NESTING {
            let last = from.map(Asked::Type).or(self.cur_from);
            self.too_deep(def, &args, origin, last);
            return Ty::Error;
        }
        let Some((name, full_name)) = self.instance_names(def, &args, origin, from) else {
            return Ty::Error;
        };
        let span = self.types.get(def).span;
        let id = self.types.add(TypeDef {
            name: Some(name),
            full_name: Some(full_name),
            kind: TypeKind::Pending,
            span,
        });
        let origin = origin
            .or_else(|| {
                from.and_then(|f| self.types.instance_of(f))
                    .and_then(|i| i.origin)
            })
            .or(self.cur_origin);
        if from.is_none()
            && let Some(Asked::Routine(r)) = self.cur_from
        {
            self.inst.tfrom.insert(id, r);
        }
        self.types
            .instance_index
            .insert(id, self.types.instances.len());
        self.types.instances.push(Instance {
            id,
            def,
            args: args.clone(),
            filled: false,
            unit: from
                .and_then(|f| self.types.instance_of(f))
                .map_or(self.unit, |i| i.unit),
            origin,
            from,
        });
        self.types.instance_keys.insert((def, args), id);
        self.inst.asked.push(Asked::Type(id));
        if self.types.generic_done.contains(&def) {
            self.fill(id);
        }
        wrap(id)
    }

    /// The name in diagnostics and the full name of an instantiation (`docs/spec/generics.md`
    /// 4.1): `TBox<Integer>` / `Hello.TBox<Integer>`, and for a type nested in a generic type, the
    /// outer type's instantiation before it (`TList<Integer>.TEnumerator`). `None` when the
    /// outer type's instantiation fails.
    fn instance_names(
        &mut self,
        def: TypeId,
        args: &[Ty],
        origin: Option<Span>,
        from: Option<TypeId>,
    ) -> Option<(String, String)> {
        if let Some(local) = self.types.generic_local.get(&def).cloned() {
            return self.local_instance_names(&local, args, origin, from);
        }
        let def_name = self.types.get(def).name.clone().unwrap_or_default();
        let base = base_name(&def_name).to_string();
        let (own, prefix, full_prefix) = match self.types.generic_outer.get(&def).copied() {
            Some(outer) => {
                let n = self.types.type_params(outer).len().min(args.len());
                let o = self.instantiate(outer, args[..n].to_vec(), origin, from);
                if o.is_error() {
                    return None;
                }
                (
                    &args[n..],
                    format!("{}.", self.types.name(o)),
                    format!("{}.", self.types.full_name(o)),
                )
            }
            None => {
                let def_full = self.types.get(def).full_name.clone().unwrap_or_default();
                let full_base = base_name(&def_full).to_string();
                let short = full_base.rsplit_once('.').map_or("", |(q, _)| q);
                (
                    args,
                    String::new(),
                    if short.is_empty() {
                        String::new()
                    } else {
                        format!("{short}.")
                    },
                )
            }
        };
        let names: Vec<String> = own.iter().map(|a| self.types.name(*a)).collect();
        let full_args: Vec<String> = own.iter().map(|a| self.types.canonical_name(*a)).collect();
        let (name, full) = if own.is_empty() {
            (base.clone(), base)
        } else {
            (
                format!("{base}<{}>", names.join(", ")),
                format!("{base}<{}>", full_args.join(",")),
            )
        };
        Some((format!("{prefix}{name}"), format!("{full_prefix}{full}")))
    }

    /// The names of an instantiation of a type declared in a generic body
    /// (`docs/spec/generics.md` 2.6): the routine's instantiation, then the enclosing routines
    /// and the type's name (`Find<Integer>.TLoc` / `Hello.Find<Integer>.TLoc`,
    /// `TBox<Integer>.Get.TLoc` / `Hello.TBox<Integer>.Get.TLoc`).
    fn local_instance_names(
        &mut self,
        local: &super::types::LocalGeneric,
        args: &[Ty],
        origin: Option<Span>,
        from: Option<TypeId>,
    ) -> Option<(String, String)> {
        let n = args.len() - local.own.min(args.len());
        let (prefix, full_prefix) = match local.class {
            Some(c) => {
                let o = self.instantiate(c, args[..n].to_vec(), origin, from);
                if o.is_error() {
                    return None;
                }
                (
                    format!("{}.", self.types.name(o)),
                    format!("{}.", self.types.full_name(o)),
                )
            }
            None => (String::new(), format!("{}.", local.unit)),
        };
        let own = &args[n..];
        let (names, full_args) = if own.is_empty() {
            (String::new(), String::new())
        } else {
            let names: Vec<String> = own.iter().map(|a| self.types.name(*a)).collect();
            let full: Vec<String> = own.iter().map(|a| self.types.canonical_name(*a)).collect();
            (
                format!("<{}>", names.join(", ")),
                format!("<{}>", full.join(",")),
            )
        };
        let (head, tail) = (&local.head, &local.tail);
        Some((
            format!("{prefix}{head}{names}{tail}"),
            format!("{full_prefix}{head}{full_args}{tail}"),
        ))
    }

    /// The type of a generic definition (the form of its instantiations).
    fn def_ty(&self, def: TypeId) -> Ty {
        self.types
            .generic_tys
            .get(&def)
            .copied()
            .unwrap_or(Ty::Class(def))
    }

    /// The type of a generic definition or an instantiation, by its id.
    fn ty_of(&self, id: TypeId) -> Ty {
        match self.types.generic_key(id) {
            Some((def, _)) => rewrap(self.def_ty(def), id),
            None => Ty::Class(id),
        }
    }

    /// An instantiation of a generic dynamic array type (`TRow<T> = array of T`;
    /// `docs/spec/generics.md` 2.1): the dynamic array of the element type with the type
    /// parameters replaced, the same type as any other dynamic array of that element type
    /// (`docs/spec/types.md` 1.2). It is named after the instantiation when it is the first.
    fn dyn_instance(&mut self, def: TypeId, args: Vec<Ty>) -> Ty {
        let TypeKind::DynArray { elem, .. } = self.types.get(def).kind else {
            return Ty::Error;
        };
        let env = Env::new(self.types.type_params(def).to_vec(), args.clone(), None);
        let elem = self.subst(elem, &env);
        if elem.is_error() {
            return Ty::Error;
        }
        let def_name = self.types.get(def).name.clone().unwrap_or_default();
        let def_full = self.types.get(def).full_name.clone().unwrap_or_default();
        let names: Vec<String> = args.iter().map(|a| self.types.name(*a)).collect();
        let full_args: Vec<String> = args.iter().map(|a| self.types.canonical_name(*a)).collect();
        let span = self.types.get(def).span;
        self.types.dyn_array(
            elem,
            super::types::DynWritten::Named {
                name: format!("{}<{}>", base_name(&def_name), names.join(", ")),
                full_name: format!("{}<{}>", base_name(&def_full), full_args.join(",")),
                span,
            },
        )
    }

    /// E0764 (`docs/spec/generics.md` 4.4): once per definition, at the type arguments that
    /// started the path, with the instantiations of the path (ending at `last`) in the notes.
    fn too_deep(&mut self, def: TypeId, args: &[Ty], origin: Option<Span>, last: Option<Asked>) {
        if self.inst.deep_reported.contains(&def) {
            return;
        }
        self.inst.deep_reported.push(def);
        let path = self.path(last);
        let origin = origin.or_else(|| match path.first() {
            Some(Asked::Type(t)) => self.types.instance_of(*t).and_then(|i| i.origin),
            Some(Asked::Routine(r)) => self.inst.rorigin.get(r).copied(),
            None => None,
        });
        let base = base_name(self.types.get(def).name.as_deref().unwrap_or("")).to_string();
        let shown: Vec<String> = args.iter().map(|a| short_name(self.types, *a, 1)).collect();
        let name = format!("{base}<{}>", shown.join(", "));
        let mut d = Diagnostic::error(
            Code::E(764),
            format!("generic instantiation `{name}` is nested too deeply (limit {MAX_NESTING})"),
            Stage::Sema,
        );
        if let Some(o) = origin {
            d = d.with_primary(o, "");
        }
        if let Some(n) = self.path_note(&path) {
            d.notes.push(n);
        }
        self.diags.push(d);
    }

    /// The path of E0764 that ends at `last`, from its start: each step goes back to the
    /// instantiation whose body asked for it (a type's `from` or `tfrom`, a routine's `rfrom`).
    fn path(&self, last: Option<Asked>) -> Vec<Asked> {
        let mut path: Vec<Asked> = Vec::new();
        let mut at = last;
        while let Some(a) = at {
            path.push(a);
            at = match a {
                Asked::Type(t) => self
                    .types
                    .instance_of(t)
                    .and_then(|i| i.from)
                    .map(Asked::Type)
                    .or_else(|| self.inst.tfrom.get(&t).map(|r| Asked::Routine(*r))),
                Asked::Routine(r) => self.inst.rfrom.get(&r).copied(),
            };
        }
        path.reverse();
        path
    }

    /// The `notes` of E0764 (`docs/spec/generics.md` 4.4): the instantiations of the path, at
    /// most 4 of them.
    fn path_note(&self, path: &[Asked]) -> Option<String> {
        if path.is_empty() {
            return None;
        }
        let mut steps: Vec<String> = path
            .iter()
            .take(4)
            .map(|a| format!("`{}`", self.path_name(*a)))
            .collect();
        if path.len() > 4 {
            steps.push("…".to_string());
        }
        Some(format!("instantiated through {}", steps.join(", ")))
    }

    /// The name of an instantiation in the path of E0764: shortened inside depth 3.
    fn path_name(&self, a: Asked) -> String {
        match a {
            Asked::Type(t) => short_name(self.types, Ty::Class(t), 0),
            Asked::Routine(r) => {
                let Some((base, args)) = self.inst.rinst.get(&r) else {
                    return self.routines[r.0 as usize].name.clone();
                };
                let shown: Vec<String> =
                    args.iter().map(|a| short_name(self.types, *a, 1)).collect();
                format!(
                    "{}<{}>",
                    self.routines[base.0 as usize].name,
                    shown.join(", ")
                )
            }
        }
    }

    /// The type with the type parameters replaced.
    pub fn subst(&mut self, ty: Ty, env: &Env) -> Ty {
        if !self.types.has_param(ty) {
            return ty;
        }
        match ty {
            Ty::Param(p) => env.get(p).unwrap_or(ty),
            Ty::Class(id)
            | Ty::Record(id)
            | Ty::Intf(id)
            | Ty::Array(id)
            | Ty::Proc(id)
            | Ty::Callable(id)
                if self.types.generic_key(id).is_some() =>
            {
                let (def, args) = self.types.generic_key(id).expect("checked above");
                let args: Vec<Ty> = args.into_iter().map(|a| self.subst(a, env)).collect();
                self.instantiate(def, args, None, env.from)
            }
            Ty::Weak(id) => match self.subst(Ty::Class(id), env) {
                Ty::Class(c) => Ty::Weak(c),
                t => t,
            },
            Ty::DynArray(id) | Ty::OpenArray(id) => {
                let TypeKind::DynArray { elem, generic } = self.types.get(id).kind else {
                    return ty;
                };
                let span = self.types.get(id).span;
                let elem = self.subst(elem, env);
                if elem.is_error() {
                    return Ty::Error;
                }
                let d = self
                    .types
                    .dyn_array(elem, super::types::DynWritten::Anonymous { generic, span });
                match (ty, d) {
                    (Ty::OpenArray(_), Ty::DynArray(n)) => Ty::OpenArray(n),
                    _ => d,
                }
            }
            Ty::ClassRef(id) => {
                let TypeKind::ClassRef { class } = self.types.get(id).kind else {
                    return ty;
                };
                let span = self.types.get(id).span;
                match self.subst(Ty::Class(class), env) {
                    Ty::Class(c) => self.types.class_ref(
                        c,
                        super::types::DynWritten::Anonymous {
                            generic: false,
                            span,
                        },
                    ),
                    t => t,
                }
            }
            Ty::Array(id) | Ty::Proc(id) | Ty::Callable(id) | Ty::Record(id) => {
                self.subst_anonymous(ty, id, env)
            }
            _ => ty,
        }
    }

    /// An anonymous type with a type parameter in it (`array[0..1] of T`, `procedure(X: T)`): a
    /// new type for each replacement, made once.
    fn subst_anonymous(&mut self, ty: Ty, id: TypeId, env: &Env) -> Ty {
        let key_args: Vec<Ty> = env.args.clone();
        if let Some(n) = self.types.subst_cache.get(&(id, key_args.clone())) {
            return rewrap(ty, *n);
        }
        let mut def = self.types.get(id).clone();
        def.kind = match def.kind {
            TypeKind::Array {
                index,
                low,
                high,
                elem,
            } => TypeKind::Array {
                index,
                low,
                high,
                elem: self.subst(elem, env),
            },
            TypeKind::Proc {
                family,
                params,
                ret,
            } => TypeKind::Proc {
                family,
                params: params
                    .into_iter()
                    .map(|p| ProcParam {
                        ty: self.subst(p.ty, env),
                        ..p
                    })
                    .collect(),
                ret: ret.map(|r| self.subst(r, env)),
            },
            TypeKind::Record {
                fields,
                unit,
                methods,
                statics,
                properties,
            } => TypeKind::Record {
                fields: fields
                    .into_iter()
                    .map(|f| Field {
                        ty: self.subst(f.ty, env),
                        ..f
                    })
                    .collect(),
                unit,
                methods,
                statics,
                properties,
            },
            k => k,
        };
        let n = self.types.add(def);
        self.types.subst_cache.insert((id, key_args), n);
        rewrap(ty, n)
    }

    /// Makes the members of the instantiations whose definitions are now declared.
    pub fn fill_ready(&mut self) {
        loop {
            let ready: Vec<TypeId> = self
                .types
                .instances
                .iter()
                .filter(|i| !i.filled && self.types.generic_done.contains(&i.def))
                .map(|i| i.id)
                .collect();
            if ready.is_empty() {
                break;
            }
            for id in ready {
                self.fill(id);
            }
        }
    }

    /// Makes the members of the instantiations of `def` again (whether or not its declaration is
    /// complete): the parts of the definition declared since (its methods, then its interface
    /// table) reach the instantiations a class of the same `type` section inherits from
    /// (`docs/spec/generics.md` 2.4).
    pub fn refill(&mut self, def: TypeId) {
        // The instantiations asked for from now on are made at once
        self.types.generic_done.insert(def);
        let ids: Vec<TypeId> = self
            .types
            .instances
            .iter()
            .filter(|i| i.def == def)
            .map(|i| i.id)
            .collect();
        for id in ids {
            self.fill_with(id, true);
        }
    }

    /// The members of an instantiation (`docs/spec/generics.md` 4.2): the definition's, with the
    /// type parameters replaced, and a routine for each method (its body is made only when the
    /// method is used; `instantiate_bodies`).
    fn fill(&mut self, id: TypeId) {
        self.fill_with(id, false);
    }

    fn fill_with(&mut self, id: TypeId, again: bool) {
        let Some(&k) = self.types.instance_index.get(&id) else {
            return;
        };
        let Some(i) = self.types.instances.get_mut(k) else {
            return;
        };
        if i.filled && !again {
            return;
        }
        i.filled = true;
        let def = i.def;
        let env = instance_env(self.types, id).expect("an instantiation has an entry");
        let name = self.types.get(id).name.clone().unwrap_or_default();
        let kind = self.types.get(def).kind.clone();
        // The routine of each method of the definition
        let mut map: HashMap<RoutineId, RoutineId> = HashMap::new();
        let methods = match &kind {
            TypeKind::Class { methods, .. }
            | TypeKind::Record { methods, .. }
            | TypeKind::Interface { methods, .. } => methods.clone(),
            _ => Vec::new(),
        };
        let mut new_methods = Vec::with_capacity(methods.len());
        for m in methods {
            let mut m = m;
            m.params = m
                .params
                .into_iter()
                .map(|(t, mode)| (self.subst(t, &env), mode))
                .collect();
            m.ret = m.ret.map(|t| self.subst(t, &env));
            if let Some(r) = m.routine {
                let stub = self.stub(r, id, &name, &m.name, &env);
                map.insert(r, stub);
                m.routine = Some(stub);
            }
            new_methods.push(m);
        }
        // A routine of the definition, or one it inherits from an instantiation with its type
        // parameters (`TSorted<T> = class(TBase<T>)`)
        let map_routine = |s: &mut Self, r: RoutineId| match map.get(&r) {
            Some(n) => *n,
            None => s.map_member(r, &env),
        };
        let new_kind = match kind {
            TypeKind::Class {
                parent,
                fields,
                unit,
                ref_counted,
                methods: _,
                vmt,
                is_abstract,
                sealed,
                statics,
                properties,
                intfs,
            } => {
                let parent = parent.map(|p| match self.subst(Ty::Class(p), &env) {
                    Ty::Class(c) => c,
                    _ => p,
                });
                let vmt = vmt
                    .into_iter()
                    .map(|e| match e {
                        VmtEntry::Func(r) => VmtEntry::Func(map_routine(self, r)),
                        e => e,
                    })
                    .collect();
                let intfs = intfs
                    .into_iter()
                    .map(|mut i| {
                        if let Ty::Intf(n) = self.subst(Ty::Intf(i.intf), &env) {
                            i.intf = n;
                        }
                        for e in &mut i.entries {
                            if let IntfEntry::Func(r) = e {
                                *r = map_routine(self, *r);
                            }
                        }
                        i
                    })
                    .collect();
                TypeKind::Class {
                    parent,
                    fields: fields
                        .into_iter()
                        .map(|f| super::types::ClassField {
                            ty: self.subst(f.ty, &env),
                            ..f
                        })
                        .collect(),
                    unit,
                    ref_counted,
                    methods: new_methods,
                    vmt,
                    is_abstract,
                    sealed,
                    statics: self.subst_statics(def, id, statics, &env),
                    properties: self.subst_properties(properties, &env, &map),
                    intfs,
                }
            }
            TypeKind::Record {
                fields,
                unit,
                methods: _,
                statics,
                properties,
            } => TypeKind::Record {
                fields: fields
                    .into_iter()
                    .map(|f| Field {
                        ty: self.subst(f.ty, &env),
                        ..f
                    })
                    .collect(),
                unit,
                methods: new_methods,
                statics: self.subst_statics(def, id, statics, &env),
                properties: self.subst_properties(properties, &env, &map),
            },
            TypeKind::Interface {
                parent,
                unit,
                methods: _,
                properties,
                count,
                guid,
            } => TypeKind::Interface {
                parent: parent.map(|p| match self.subst(Ty::Intf(p), &env) {
                    Ty::Intf(c) => c,
                    _ => p,
                }),
                unit,
                methods: new_methods,
                properties: self.subst_properties(properties, &env, &map),
                count,
                guid,
            },
            TypeKind::Proc {
                family,
                params,
                ret,
            } => TypeKind::Proc {
                family,
                params: params
                    .into_iter()
                    .map(|p| ProcParam {
                        ty: self.subst(p.ty, &env),
                        ..p
                    })
                    .collect(),
                ret: ret.map(|r| self.subst(r, &env)),
            },
            TypeKind::Array {
                index,
                low,
                high,
                elem,
            } => TypeKind::Array {
                index,
                low,
                high,
                elem: self.subst(elem, &env),
            },
            k => k,
        };
        let class_methods = matches!(new_kind, TypeKind::Class { .. })
            && map
                .keys()
                .any(|r| self.routines[r.0 as usize].self_kind == super::types::SelfKind::Class);
        self.types.get_mut(id).kind = new_kind;
        if class_methods {
            // `Self` of a class method is the class reference
            let span = self.types.get(id).span;
            self.types.class_ref(
                id,
                super::types::DynWritten::Anonymous {
                    generic: false,
                    span,
                },
            );
        }
    }

    /// The `class var`s, constants and nested types of an instantiation (`docs/spec/generics.md`
    /// 2.5): a `class var` of its own for each of the definition's, made once.
    fn subst_statics(
        &mut self,
        def: TypeId,
        id: TypeId,
        statics: Vec<ClassStatic>,
        env: &Env,
    ) -> Vec<ClassStatic> {
        let inst_name = self.types.get(id).name.clone().unwrap_or_default();
        statics
            .into_iter()
            .enumerate()
            .map(|(k, mut s)| {
                s.kind = match s.kind {
                    StaticKind::Const(c, t) => StaticKind::Const(c, self.subst(t, env)),
                    StaticKind::Type(t) => StaticKind::Type(self.subst(t, env)),
                    StaticKind::Var(g, t) => {
                        self.inst.global_owner.entry(g).or_insert((def, k));
                        let t = self.subst(t, env);
                        StaticKind::Var(self.class_var(g, id, &inst_name, &s.name, t, k), t)
                    }
                };
                s
            })
            .collect()
    }

    /// The `class var` of an instantiation for the definition's `class var` `g`.
    fn class_var(
        &mut self,
        g: GlobalId,
        id: TypeId,
        inst_name: &str,
        name: &str,
        ty: Ty,
        k: usize,
    ) -> GlobalId {
        if let Some(n) = self.inst.class_vars.get(&(g, id)) {
            return *n;
        }
        let src = &self.globals[g.0 as usize];
        let n = GlobalId(self.globals.len() as u32);
        let global = Global {
            name: format!("{inst_name}.{name}"),
            ty,
            span: src.span,
            unit: src.unit,
            class_var: true,
        };
        self.globals.push(global);
        let unit = self.types.instance_of(id).map_or(self.unit, |i| i.unit);
        self.inst.class_vars.insert((g, id), n);
        self.inst.global_owner.insert(n, (id, k));
        self.inst.inst_globals.push((n, unit));
        n
    }

    /// The `class var` of the instantiation that a `class var` of a generic type (a definition
    /// or an open instantiation) is, under `env`; the variable itself otherwise.
    fn map_global(&mut self, g: GlobalId, env: &Env) -> GlobalId {
        let Some(&(owner, k)) = self.inst.global_owner.get(&g) else {
            return g;
        };
        if !self.types.has_param(self.ty_of(owner)) {
            return g;
        }
        let target = match self.subst(self.ty_of(owner), env).type_id() {
            Some(t) => t,
            None => return g,
        };
        match self.types.class_statics(target).get(k).map(|s| &s.kind) {
            Some(StaticKind::Var(n, _)) => *n,
            _ => g,
        }
    }

    fn subst_properties(
        &mut self,
        props: Vec<super::types::ClassProperty>,
        env: &Env,
        map: &HashMap<RoutineId, RoutineId>,
    ) -> Vec<super::types::ClassProperty> {
        let id = env.from;
        let acc = |s: &mut Self, a: Option<Accessor>| {
            a.map(|a| match a {
                Accessor::Field { slot, name, ty } => Accessor::Field {
                    slot,
                    name,
                    ty: s.subst(ty, env),
                },
                Accessor::Method(r) => Accessor::Method(map.get(&r).copied().unwrap_or(r)),
                Accessor::ClassVar(g) => Accessor::ClassVar(
                    id.and_then(|i| s.inst.class_vars.get(&(g, i)).copied())
                        .unwrap_or_else(|| s.map_global(g, env)),
                ),
            })
        };
        props
            .into_iter()
            .map(|mut p| {
                p.ty = self.subst(p.ty, env);
                p.params = p.params.iter().map(|t| self.subst(*t, env)).collect();
                p.read = acc(self, p.read.take());
                p.write = acc(self, p.write.take());
                p
            })
            .collect()
    }

    /// The routine of a method of an instantiation: the definition's heading with the type
    /// parameters replaced, without a body yet.
    fn stub(
        &mut self,
        r: RoutineId,
        inst: TypeId,
        inst_name: &str,
        method: &str,
        env: &Env,
    ) -> RoutineId {
        if let Some(n) = self.inst.stub_of.get(&(r, inst)) {
            return *n;
        }
        let src = &self.routines[r.0 as usize];
        let mut info = RoutineInfo {
            name: format!("{inst_name}.{method}"),
            params: src.params.clone(),
            ret: src.ret,
            has_body: true,
            forward_span: None,
            implicit_forward: src.implicit_forward,
            name_span: src.name_span,
            hir: None,
            overload: src.overload,
            overload_fix_at: None,
            unit: src.unit,
            file: src.file,
            interface: src.interface,
            parent: None,
            scope: None,
            calls: Vec::new(),
            method_of: Some(inst),
            self_kind: src.self_kind,
            is_abstract: src.is_abstract,
            native: false,
            body_span: src.body_span,
            closure: false,
            type_params: src.type_params.clone(),
            closure_info: Default::default(),
        };
        for p in &mut info.params {
            p.ty = self.subst(p.ty, env);
            if let Some(d) = &mut p.default {
                let mut s = Subst {
                    inst: self,
                    env,
                    local: &HashMap::new(),
                };
                walk_expr(d, &mut s);
            }
        }
        info.ret = info.ret.map(|t| self.subst(t, env));
        let id = RoutineId(self.routines.len() as u32);
        self.routines.push(info);
        self.inst.stubs.insert(id, (r, inst));
        self.inst.stub_of.insert((r, inst), id);
        id
    }

    /// The routine of the instantiation that a routine of a generic type (a definition or an
    /// open instantiation) is, under `env`; the routine itself otherwise.
    fn map_member(&mut self, r: RoutineId, env: &Env) -> RoutineId {
        let Some(owner) = self.routines.get(r.0 as usize).and_then(|i| i.method_of) else {
            return r;
        };
        if self.types.generic_key(owner).is_none() || !self.types.has_param(Ty::Class(owner)) {
            return r;
        }
        let Some(k) = self
            .types
            .class_methods(owner)
            .iter()
            .position(|m| m.routine == Some(r))
        else {
            return r;
        };
        let target = match self.subst(Ty::Class(owner), env) {
            Ty::Class(c) | Ty::Record(c) => c,
            _ => return r,
        };
        self.types
            .class_methods(target)
            .get(k)
            .and_then(|m| m.routine)
            .unwrap_or(r)
    }

    /// The instantiation of the generic routine or method `base` with `args`
    /// (`docs/spec/generics.md` 4.1): its heading with the type parameters replaced, and a body
    /// made when it is used (`instantiate_bodies`). The same key is the same routine; a routine's
    /// own type parameters are the routine itself. `None` past the nesting limit (E0764, at
    /// `origin`).
    pub fn instantiate_routine(
        &mut self,
        base: RoutineId,
        args: Vec<Ty>,
        origin: Option<Span>,
    ) -> Option<RoutineId> {
        let own = self.routines[base.0 as usize].type_params.clone();
        if own.len() != args.len() || args.iter().any(|a| a.is_error()) {
            return None;
        }
        if own.iter().map(|p| Ty::Param(*p)).eq(args.iter().copied()) {
            return Some(base);
        }
        if let Some(r) = self.inst.rkeys.get(&(base, args.clone())) {
            return Some(*r);
        }
        let origin = origin.or(self.cur_origin);
        let depth = 1 + args
            .iter()
            .map(|a| self.types.nesting_depth(*a))
            .max()
            .unwrap_or(0);
        let base_name = self.routines[base.0 as usize].name.clone();
        if depth > MAX_NESTING {
            // The generic routine the instantiations come from (once each)
            let root = self.inst.rinst.get(&base).map_or(base, |(b, _)| *b);
            if !self.inst.deep_routines.contains(&root) {
                self.inst.deep_routines.push(root);
                let shown: Vec<String> =
                    args.iter().map(|a| short_name(self.types, *a, 1)).collect();
                let mut d = Diagnostic::error(
                    Code::E(764),
                    format!(
                        "generic instantiation `{base_name}<{}>` is nested too deeply (limit {MAX_NESTING})",
                        shown.join(", ")
                    ),
                    Stage::Sema,
                );
                if let Some(o) = origin {
                    d = d.with_primary(o, "");
                }
                let path = self.path(self.cur_from);
                if let Some(n) = self.path_note(&path) {
                    d.notes.push(n);
                }
                self.diags.push(d);
            }
            return None;
        }
        let env = Env::new(own, args.clone(), None);
        let names: Vec<String> = args.iter().map(|a| self.types.name(*a)).collect();
        let src = self.routines[base.0 as usize].clone();
        let mut info = super::RoutineInfo {
            name: format!("{base_name}<{}>", names.join(", ")),
            has_body: true,
            forward_span: None,
            hir: None,
            overload_fix_at: None,
            calls: Vec::new(),
            closure: false,
            type_params: Vec::new(),
            closure_info: Default::default(),
            ..src
        };
        for p in &mut info.params {
            p.ty = self.subst(p.ty, &env);
            if let Some(d) = &mut p.default {
                let mut s = Subst {
                    inst: self,
                    env: &env,
                    local: &HashMap::new(),
                };
                walk_expr(d, &mut s);
            }
        }
        info.ret = info.ret.map(|t| self.subst(t, &env));
        let id = RoutineId(self.routines.len() as u32);
        self.routines.push(info);
        self.inst.rkeys.insert((base, args.clone()), id);
        self.inst.rinst.insert(id, (base, args));
        self.inst.runit.insert(id, self.unit);
        if let Some(o) = origin {
            self.inst.rorigin.insert(id, o);
        }
        if let Some(f) = self.cur_from {
            self.inst.rfrom.insert(id, f);
        }
        self.inst.asked.push(Asked::Routine(id));
        Some(id)
    }

    /// The routine a copied body refers to in place of `r` under `env`: a routine instantiation
    /// with its type arguments replaced, a generic routine referring to itself instantiated, a
    /// method of an open instantiation mapped to the instantiation's (`map_member`).
    fn map_routine(&mut self, r: RoutineId, env: &Env) -> RoutineId {
        if let Some((base, args)) = self.inst.rinst.get(&r).cloned() {
            let nb = self.map_member(base, env);
            let na: Vec<Ty> = args.iter().map(|a| self.subst(*a, env)).collect();
            if nb == base && na == args {
                return r;
            }
            return self.instantiate_routine(nb, na, None).unwrap_or(r);
        }
        let own = self.routines[r.0 as usize].type_params.clone();
        if !own.is_empty() && own.iter().any(|p| env.get(*p).is_some()) {
            let nb = self.map_member(r, env);
            let args: Vec<Ty> = own
                .iter()
                .map(|p| env.get(*p).unwrap_or(Ty::Param(*p)))
                .collect();
            return self.instantiate_routine(nb, args, None).unwrap_or(r);
        }
        self.map_member(r, env)
    }

    /// The instantiations without type parameters in their arguments, from `from` on.
    fn concrete_instances(&self, from: usize) -> Vec<TypeId> {
        self.types.instances[from..]
            .iter()
            .filter(|i| !i.args.iter().any(|a| self.types.has_param(*a)))
            .map(|i| i.id)
            .collect()
    }

    /// The methods of an instantiation that have a body whenever it exists: those of its virtual
    /// method table and of its interface table (`docs/design/generics.md` 3.1).
    fn always_used(&self, id: TypeId) -> Vec<RoutineId> {
        let mut out = Vec::new();
        // The methods the type information calls by name (`docs/spec/rtti.md` 1.5)
        for (_, _, m) in super::invoke::invoked_methods(self.types, id, self.inst.rtti_wants) {
            if let Some(r) = m.routine
                && self.inst.stubs.get(&r).is_some_and(|(_, o)| *o == id)
                && !out.contains(&r)
            {
                out.push(r);
            }
        }
        if let TypeKind::Class { vmt, intfs, .. } = &self.types.get(id).kind {
            for e in vmt {
                if let VmtEntry::Func(r) = e
                    && self.inst.stubs.get(r).is_some_and(|(_, o)| *o == id)
                {
                    out.push(*r);
                }
            }
            for i in intfs {
                for e in &i.entries {
                    if let IntfEntry::Func(r) = e
                        && self.inst.stubs.get(r).is_some_and(|(_, o)| *o == id)
                        && !out.contains(r)
                    {
                        out.push(*r);
                    }
                }
            }
        }
        out
    }

    /// The routines of instantiations a body refers to.
    fn referenced(&self, r: &Routine) -> Vec<RoutineId> {
        struct Refs<'s> {
            stubs: &'s HashMap<RoutineId, (RoutineId, TypeId)>,
            rinst: &'s HashMap<RoutineId, (RoutineId, Vec<Ty>)>,
            out: Vec<RoutineId>,
        }
        impl HirVisit for Refs<'_> {
            fn ty(&mut self, _t: &mut super::types::Ty) {}
            fn routine(&mut self, r: &mut RoutineId) {
                if (self.stubs.contains_key(r) || self.rinst.contains_key(r))
                    && !self.out.contains(r)
                {
                    self.out.push(*r);
                }
            }
        }
        let mut v = Refs {
            stubs: &self.inst.stubs,
            rinst: &self.inst.rinst,
            out: Vec::new(),
        };
        let mut copy = r.clone();
        walk_routine(&mut copy, &mut v);
        v.out
    }

    /// The work list (`docs/design/generics.md` 3.1): the bodies of the methods of the
    /// instantiations that are used (by the routines given, by the virtual method tables, and by
    /// the bodies made), copied from the generic bodies with the type parameters replaced, until
    /// none is left. A loop, not a recursion.
    pub fn instantiate_bodies(&mut self, roots: &[&Routine]) {
        // No instantiation has a method: nothing to copy
        if self.inst.stubs.is_empty() && self.inst.rinst.is_empty() {
            return;
        }
        let mut queue: VecDeque<RoutineId> = VecDeque::new();
        let mut queued: std::collections::HashSet<RoutineId> = Default::default();
        let mut push = |queue: &mut VecDeque<RoutineId>, r: RoutineId| {
            if queued.insert(r) {
                queue.push_back(r);
            }
        };
        for id in self.concrete_instances(0) {
            for r in self.always_used(id) {
                push(&mut queue, r);
            }
        }
        let mut seen_instances = self.types.instances.len();
        let mut found = Vec::new();
        for (i, info) in self.routines.iter().enumerate() {
            let Some(h) = &info.hir else { continue };
            if self.is_generic_routine(RoutineId(i as u32)) {
                continue;
            }
            found.extend(self.referenced(h));
        }
        for r in roots {
            found.extend(self.referenced(r));
        }
        for r in found {
            push(&mut queue, r);
        }
        while let Some(stub) = queue.pop_front() {
            let made = self.body(stub);
            // The routines of the key equalities and hashes the copies have
            // (`docs/spec/equality.md` 8.3)
            for &r in &made {
                self.fill_keys(r);
            }
            // The instantiations the copies made: their tables' methods
            for id in self.concrete_instances(seen_instances) {
                for r in self.always_used(id) {
                    push(&mut queue, r);
                }
            }
            seen_instances = self.types.instances.len();
            for r in made {
                let refs = match &self.routines[r.0 as usize].hir {
                    Some(h) => self.referenced(h),
                    None => Vec::new(),
                };
                for x in refs {
                    push(&mut queue, x);
                }
            }
        }
    }

    /// The bodies of the generated `Equals` overrides (`docs/spec/equality.md` 3.3, 8.3), once
    /// every class of the program is known: `Result := (Obj is TFoo) and not (Obj is TBar) and
    /// … and (Self = TFoo(Obj))`, where `TBar`, … are the nearest descendants of `TFoo` that
    /// declare their own `Equal` (an instance of one of them has another nearest class declaring
    /// `Equal`, so it is not equal). A class's own generated `Equals` comes after every unit's
    /// functions, in the order generated, before the bodies of the instantiations; a generic
    /// type's is copied into each instantiation by the work list, as its other methods.
    pub fn generate_equals(&mut self) {
        use super::hir::*;
        let generated = self.inst.generated_equals.clone();
        let declares = |c: TypeId| generated.iter().any(|(g, _, _)| *g == c);
        for &(class, equals, op) in &generated {
            // The nearest descendants declaring their own `Equal`. A descendant of an
            // instantiation counts for the generic class (`TSub = class(TBox<Integer>)`;
            // `docs/spec/generics.md` 2.4): in the other instantiations' `Equals` its check is
            // false anyway, as their `is` check is
            let def_of = |p: TypeId| self.types.instance_of(p).map_or(p, |i| i.def);
            let below: Vec<TypeId> = generated
                .iter()
                .map(|(d, _, _)| *d)
                .filter(|&d| {
                    let mut at = self.types.parent(d);
                    while let Some(p) = at {
                        let p = def_of(p);
                        if declares(p) {
                            return p == class;
                        }
                        at = self.types.parent(p);
                    }
                    false
                })
                .collect();
            let span = self.routines[equals.0 as usize].name_span;
            let expr = |kind: ExprKind, ty: Ty| Expr {
                kind,
                ty,
                value: None,
                span,
            };
            let obj = || expr(ExprKind::Var(VarId(1)), Ty::Class(super::types::TOBJECT));
            let is = |c: TypeId| {
                expr(
                    ExprKind::ClassCheck {
                        operand: Box::new(obj()),
                        class: c,
                        op: ClassCheckOp::Is,
                    },
                    Ty::Boolean,
                )
            };
            let and = |rhs: Expr| BinStep {
                op: BinOp::And,
                rhs,
                calc: Calc::Bool,
                ty: Ty::Boolean,
                span,
            };
            let mut steps: Vec<BinStep> = below
                .into_iter()
                .map(|d| {
                    and(expr(
                        ExprKind::Unary {
                            op: UnOp::Not,
                            operand: Box::new(is(d)),
                            calc: Calc::Bool,
                        },
                        Ty::Boolean,
                    ))
                })
                .collect();
            let cast = expr(
                ExprKind::ClassCheck {
                    operand: Box::new(obj()),
                    class,
                    op: ClassCheckOp::Cast,
                },
                Ty::Class(class),
            );
            steps.push(and(expr(
                ExprKind::Call {
                    routine: op,
                    args: vec![
                        CallArg::Value(expr(ExprKind::Var(VarId(0)), Ty::Class(class))),
                        CallArg::Value(cast),
                    ],
                    dispatch: Dispatch::Routine,
                },
                Ty::Boolean,
            )));
            let value = expr(
                ExprKind::Binary {
                    first: Box::new(is(class)),
                    steps,
                },
                Ty::Boolean,
            );
            let var = |name: &str, ty: Ty, kind: VarKind| Var {
                name: name.into(),
                ty,
                kind,
                span,
                decl: None,
                is_for_var: false,
                is_loop_var: false,
                captured: false,
                read_by_nested: false,
                written_by_nested: None,
                written: false,
                cell: false,
            };
            let info = &self.routines[equals.0 as usize];
            let routine = Routine {
                name: info.name.clone(),
                vars: vec![
                    var("Self", Ty::Class(class), VarKind::SelfParam),
                    var(
                        "Obj",
                        Ty::Class(super::types::TOBJECT),
                        VarKind::Param(crate::syntax::ast::ParamMode::Value),
                    ),
                    var("Result", Ty::Boolean, VarKind::Result),
                ],
                param_count: 2,
                ret: Some(Ty::Boolean),
                body: vec![Stmt {
                    kind: StmtKind::Assign {
                        target: Place::Var(VarId(2), span),
                        value,
                        check: Checks {
                            overflow: false,
                            range: false,
                        },
                        is_init: false,
                    },
                    span,
                }],
                end_span: span,
                body_begin: None,
                name_span: span,
                has_syntax_errors: false,
                has_errors: false,
                unit: info.unit,
                parent: None,
                link: false,
                is_abstract: false,
                native: false,
                builtin: false,
                generic: false,
                unreached: false,
                dtor_of: None,
                helper: false,
                operator: false,
                closure: false,
                captures: Vec::new(),
                groups: Default::default(),
                instance: None,
            };
            self.routines[equals.0 as usize].hir = Some(Box::new(routine));
            self.routines[equals.0 as usize].has_body = true;
            if !self.types.is_generic(class) {
                self.inst.order.push(equals);
            }
        }
    }

    /// Whether a routine is generic (a method of a generic type or an open instantiation, a
    /// method of an instantiation without a body, or nested in one of them).
    pub(super) fn is_generic_routine(&self, r: RoutineId) -> bool {
        let mut at = Some(r);
        while let Some(x) = at {
            let info = &self.routines[x.0 as usize];
            if self.inst.rinst.contains_key(&x) {
                return !self.inst.rowner.contains_key(&x);
            }
            if !info.type_params.is_empty() {
                return true;
            }
            if let Some(c) = info.method_of
                && (self.types.is_generic(c) || self.inst.stubs.contains_key(&x))
            {
                return !self.inst.owner.contains_key(&x);
            }
            at = info.parent;
        }
        false
    }

    /// The body of a method of an instantiation, or of a routine instantiation, and of the
    /// routines nested in it: copies of the definition's with the type parameters replaced.
    /// Returns the routines made (none for an instantiation with type parameters left in it).
    fn body(&mut self, stub: RoutineId) -> Vec<RoutineId> {
        if let Some((base, args)) = self.inst.rinst.get(&stub).cloned() {
            if args.iter().any(|a| self.types.has_param(*a)) {
                return Vec::new();
            }
            let (def_r, mut env) = match self.inst.stubs.get(&base).copied() {
                Some((d, cls)) => {
                    let Some((def, cargs)) = self.types.generic_key(cls) else {
                        return Vec::new();
                    };
                    if cargs.iter().any(|a| self.types.has_param(*a)) {
                        return Vec::new();
                    }
                    (
                        d,
                        Env::new(self.types.type_params(def).to_vec(), cargs, Some(cls)),
                    )
                }
                None => {
                    // A method of a generic type itself is open
                    if self.routines[base.0 as usize]
                        .method_of
                        .is_some_and(|c| self.types.is_generic(c))
                    {
                        return Vec::new();
                    }
                    (base, Env::new(Vec::new(), Vec::new(), None))
                }
            };
            if self.routines[def_r.0 as usize].hir.is_none() {
                return Vec::new();
            }
            env.params
                .extend(self.routines[base.0 as usize].type_params.iter().copied());
            env.args.extend(args);
            let unit = self.inst.runit.get(&stub).copied().unwrap_or(self.unit);
            let saved = (self.cur_origin.take(), self.cur_from.take());
            self.cur_origin = self.inst.rorigin.get(&stub).copied();
            self.cur_from = Some(Asked::Routine(stub));
            let made = self.copy_body(def_r, stub, &env, Owner::Routine(stub), unit);
            (self.cur_origin, self.cur_from) = saved;
            return made;
        }
        let Some(&(def_r, inst)) = self.inst.stubs.get(&stub) else {
            return Vec::new();
        };
        // A generic method of an instantiated type is instantiated with its own type arguments
        if !self.routines[stub.0 as usize].type_params.is_empty() {
            return Vec::new();
        }
        if self
            .types
            .instance_of(inst)
            .is_some_and(|i| i.args.iter().any(|a| self.types.has_param(*a)))
        {
            return Vec::new();
        }
        if self.routines[def_r.0 as usize].hir.is_none() {
            return Vec::new();
        }
        let (def, args) = self
            .types
            .generic_key(inst)
            .expect("a stub belongs to an instantiation");
        let env = Env::new(self.types.type_params(def).to_vec(), args, Some(inst));
        let unit = self.types.instance_of(inst).map_or(self.unit, |i| i.unit);
        let saved = (self.cur_origin.take(), self.cur_from.take());
        self.cur_origin = self.types.instance_of(inst).and_then(|i| i.origin);
        self.cur_from = Some(Asked::Type(inst));
        let made = self.copy_body(def_r, stub, &env, Owner::Type(inst), unit);
        (self.cur_origin, self.cur_from) = saved;
        made
    }

    /// Copies the body of `def_r` and of the routines nested in it into `target` (and new
    /// routines for the nested ones), with the type parameters replaced under `env`.
    fn copy_body(
        &mut self,
        def_r: RoutineId,
        target: RoutineId,
        env: &Env,
        owner: Owner,
        unit: u32,
    ) -> Vec<RoutineId> {
        let saved_unit = std::mem::replace(&mut self.unit, unit);
        let env = env.clone();
        // The routines nested in the method (after it, in declaration order)
        let mut local: HashMap<RoutineId, RoutineId> = HashMap::new();
        local.insert(def_r, target);
        let mut nested: Vec<RoutineId> = Vec::new();
        for i in (def_r.0 as usize + 1)..self.routines.len() {
            let mut at = self.routines[i].parent;
            while let Some(p) = at {
                if p == def_r {
                    nested.push(RoutineId(i as u32));
                    break;
                }
                at = self.routines[p.0 as usize].parent;
            }
        }
        for &n in &nested {
            let mut info = self.routines[n.0 as usize].clone();
            info.hir = None;
            let id = RoutineId(self.routines.len() as u32);
            self.routines.push(info);
            local.insert(n, id);
        }
        let mut made = vec![target];
        for &n in &nested {
            made.push(local[&n]);
        }
        for (&src, &dst) in
            std::iter::once((&def_r, &target)).chain(nested.iter().map(|n| (n, &local[n])))
        {
            let mut h = self.routines[src.0 as usize]
                .hir
                .clone()
                .expect("a nested routine of a body has a body");
            {
                let mut s = Subst {
                    inst: self,
                    env: &env,
                    local: &local,
                };
                walk_routine(&mut h, &mut s);
            }
            h.parent = h.parent.map(|p| local.get(&p).copied().unwrap_or(p));
            let calls: Vec<RoutineId> = self.routines[src.0 as usize].calls.clone();
            let calls: Vec<RoutineId> = calls
                .into_iter()
                .map(|c| {
                    local
                        .get(&c)
                        .copied()
                        .unwrap_or_else(|| self.map_routine(c, &env))
                })
                .collect();
            let parent = self.routines[src.0 as usize]
                .parent
                .map(|p| local.get(&p).copied().unwrap_or(p));
            self.inst.source.insert(dst, src);
            let info = &mut self.routines[dst.0 as usize];
            info.parent = parent;
            info.hir = Some(h);
            info.calls = calls;
            if dst != target {
                // A nested routine's parameters
                let params = info.params.clone();
                let mut params = params;
                for p in &mut params {
                    p.ty = self.subst(p.ty, &env);
                }
                let ret = self.routines[dst.0 as usize]
                    .ret
                    .map(|t| self.subst(t, &env));
                let info = &mut self.routines[dst.0 as usize];
                info.params = params;
                info.ret = ret;
            }
        }
        for &r in &made {
            self.inst.order.push(r);
            match owner {
                Owner::Type(t) => {
                    self.inst.owner.insert(r, t);
                }
                Owner::Routine(x) => {
                    self.inst.rowner.insert(r, x);
                }
            }
        }
        self.unit = saved_unit;
        made
    }
}

fn rewrap(ty: Ty, id: TypeId) -> Ty {
    match ty {
        Ty::Class(_) => Ty::Class(id),
        Ty::Intf(_) => Ty::Intf(id),
        Ty::DynArray(_) => Ty::DynArray(id),
        Ty::Array(_) => Ty::Array(id),
        Ty::Proc(_) => Ty::Proc(id),
        Ty::Callable(_) => Ty::Callable(id),
        Ty::Record(_) => Ty::Record(id),
        t => t,
    }
}

/// What a copied body belongs to.
#[derive(Clone, Copy)]
enum Owner {
    Type(TypeId),
    Routine(RoutineId),
}

/// The replacement of the types and routines of a copied body.
struct Subst<'s, 'x> {
    inst: &'s mut Inst<'x>,
    env: &'s Env,
    /// The routines nested in the method copied, and their copies.
    local: &'s HashMap<RoutineId, RoutineId>,
}

impl HirVisit for Subst<'_, '_> {
    fn ty(&mut self, t: &mut Ty) {
        *t = self.inst.subst(*t, self.env);
    }

    fn type_id(&mut self, id: &mut TypeId) {
        // A type parameter (the class of `X is T`, `T.ClassName`; `docs/spec/generics.md` 3.3)
        let ty = if self.inst.types.is_param(*id) {
            Ty::Param(*id)
        } else if self.inst.types.is_interface(*id) {
            Ty::Intf(*id)
        } else if self.inst.types.is_record(*id) {
            Ty::Record(*id)
        } else {
            Ty::Class(*id)
        };
        if let Some(n) = self.inst.subst(ty, self.env).type_id() {
            *id = n;
        }
    }

    fn global(&mut self, g: &mut GlobalId) {
        *g = self.inst.map_global(*g, self.env);
    }

    fn routine(&mut self, r: &mut RoutineId) {
        *r = match self.local.get(r) {
            Some(n) => *n,
            None => self.inst.map_routine(*r, self.env),
        };
    }

    fn expr(&mut self, e: &mut super::hir::Expr) {
        use super::hir::ExprKind;
        // `T.Create` (`docs/spec/generics.md` 4.3): the concrete class's constructor
        if let ExprKind::New {
            class: class @ Ty::Param(_),
            ctor,
            args,
            after,
            ..
        } = &mut e.kind
        {
            let Ty::Class(c) = self.inst.subst(*class, self.env) else {
                return;
            };
            // A class that does not satisfy the `constructor` constraint was reported (E0759)
            // where the type argument was given, and the instantiation is still made (4.2)
            let Some((r, defaults)) =
                super::generics::default_ctor(self.inst.types, self.inst.routines, c)
            else {
                return;
            };
            *class = Ty::Class(c);
            *ctor = r;
            *args = defaults
                .into_iter()
                .map(super::hir::CallArg::Value)
                .collect();
            *after = self.inst.types.has_after_construction(c);
        }
    }
}
