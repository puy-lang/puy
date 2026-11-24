//! `TRttiMethod.Invoke`, the properties read and written by their methods, and the attributes
//! of `System.Rtti` (`docs/spec/rtti.md` 5.6–5.8, 7.3, 7.4): the generated bridge functions
//! (`<invoke:…>`) and attribute functions (`<attr:N>`), and the rules the analysis and the type
//! information section share: which methods the section lists, and the bridge each one calls
//! through.
//!
//! A bridge function takes the callable value to call (`Target`), the `TValue` of the instance
//! (`Instance`) and the arguments (`Args`), and is a plain Pascal function: it unwraps the
//! arguments, calls `Target` (a method value made by `RttiMethodRef` / `RttiPropRef`, so a call
//! like any other: stackless, `docs/spec/rtti.md` 5.7), wraps the result, and writes the `var` /
//! `out` arguments back into `Args`. The methods of one shape share one bridge, whatever type
//! declares them; a record's method takes its `Self` from `Instance`, and a constructor creates
//! the instance of the class reference `Instance` holds, so theirs are of their own type.
//!
//! The analysis makes them only when a body it analyzes uses the built-ins of `System.Rtti` that
//! need them (the flags of [`RttiWants`]), so that a program reading no method through the type
//! information has none; the module keeps those the reachability walk reaches
//! (`docs/spec/rtti.md` 1.5).

use std::collections::HashMap;

use super::ParamInfo;
use super::hir::*;
use super::instances::Inst;
use super::keys::Gen;
use super::types::{
    Accessor, ClassMethod, DynWritten, MethodKind, ProcFamily, ProcParam, SelfKind, TOBJECT, Ty,
    TypeDef, TypeId, TypeKind, TypeTable,
};
use crate::bytecode::rtti::format_bridge_name;
use crate::source::TextRange;
use crate::syntax::ast::ParamMode;

/// The built-ins of `System.Rtti` some analyzed body uses: what the analysis generates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct RttiWants {
    /// `RttiMethodRef`: `TRttiMethod.Invoke` (every method the section lists is called by name).
    pub invoke: bool,
    /// `RttiPropRef`: a property read or written by its methods.
    pub props: bool,
    /// `RttiAttrFunc`: the attributes' instances.
    pub attrs: bool,
}

/// Whether the type information lists the method `m` of the record, class or interface `id`
/// (`docs/spec/rtti.md` 1.4): public and published methods are included except operators and
/// generic methods. Of a built-in class, only `TObject`'s `Create` and `Destroy` are included.
pub fn lists_method(types: &TypeTable, id: TypeId, m: &ClassMethod) -> bool {
    if TypeTable::is_builtin(id) {
        return id == TOBJECT
            && (m.name.eq_ignore_ascii_case("Create") || m.name.eq_ignore_ascii_case("Destroy"));
    }
    // A generic method has no entry: its parameters name its own type parameters; the methods
    // the compiler made have no place in the declaration
    m.vis.is_public()
        && m.kind != MethodKind::Operator
        && m.span != TextRange::default()
        && !m.params.iter().any(|(t, _)| types.has_param(*t))
        && !m.ret.is_some_and(|t| types.has_param(t))
}

/// Whether the type information lists the property `p` (`docs/spec/rtti.md` 1.4): an instance
/// property declared public or published, with neither an index parameter nor `index`.
pub fn lists_property(p: &super::types::ClassProperty) -> bool {
    p.vis.is_public() && !p.is_class && p.params.is_empty() && p.index.is_none()
}

/// The method an accessor of a property of the type `id` calls, found in the type or an
/// ancestor: the type declaring it, its index among that type's methods, the method.
pub fn accessor_method(
    types: &TypeTable,
    id: TypeId,
    a: &Option<Accessor>,
) -> Option<(TypeId, u32, ClassMethod)> {
    let Some(Accessor::Method(r)) = a else {
        return None;
    };
    let owners = match types.get(id).kind {
        TypeKind::Class { .. } => types.class_chain(id),
        _ => vec![id],
    };
    owners.into_iter().find_map(|o| {
        types
            .class_methods(o)
            .iter()
            .enumerate()
            .find(|(_, m)| m.routine == Some(*r))
            .map(|(k, m)| (o, k as u32, m.clone()))
    })
}

/// The shape of the bridge of a method (`docs/spec/rtti.md` 7.3).
enum Shape {
    /// A method called through its method value: an instance, class or static method of a
    /// class, a static method of a record, or (with `record`) a record's method, whose `Self`
    /// is passed first.
    Call {
        record: Option<TypeId>,
        params: Vec<(Ty, ParamMode)>,
        ret: Option<Ty>,
    },
    /// A constructor of a class (`ctor` `None` for the built-in `TObject.Create`) or a record.
    Ctor {
        owner: TypeId,
        ctor: Option<RoutineId>,
        params: Vec<(Ty, ParamMode)>,
    },
}

/// The shape of the bridge of the method `m` of `owner`: `None` for a destructor (`Free`, in
/// `System.Rtti`) and a method of an interface (no function to call).
fn shape(types: &TypeTable, owner: TypeId, m: &ClassMethod) -> Option<Shape> {
    let record = match types.get(owner).kind {
        TypeKind::Record { .. } => true,
        TypeKind::Class { .. } => false,
        _ => return None,
    };
    match m.kind {
        MethodKind::Destructor | MethodKind::Operator => None,
        MethodKind::Constructor => Some(Shape::Ctor {
            owner,
            ctor: m.routine,
            params: m.params.clone(),
        }),
        _ => {
            // A method without a function of its own (a built-in one) has nothing to call
            m.routine?;
            Some(Shape::Call {
                record: (record && m.self_kind != SelfKind::Static).then_some(owner),
                params: m.params.clone(),
                ret: m.ret,
            })
        }
    }
}

/// The parameters of a shape as written in a bridge's name.
fn params_text(types: &TypeTable, params: &[(Ty, ParamMode)]) -> String {
    params
        .iter()
        .map(|(t, mode)| {
            let m = match mode {
                ParamMode::Value => "",
                ParamMode::Const => "const ",
                ParamMode::Var => "var ",
                ParamMode::Out => "out ",
            };
            format!("{m}{}", types.canonical_name(*t))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The name of the bridge of the method `m` of `owner` (`docs/spec/rtti.md` 7.3), which tells
/// its shape: `<invoke:(const System.string; var System.Integer): System.Boolean>` for a method
/// called through its method value, `<invoke:record T.TPoint.(…)…>` for a record's method,
/// `<invoke:constructor T.TShape.Create(…)>` for a constructor. `None` if it has none.
pub fn bridge_name(types: &TypeTable, owner: TypeId, m: &ClassMethod) -> Option<String> {
    Some(match shape(types, owner, m)? {
        Shape::Call {
            record,
            params,
            ret,
        } => {
            let head = record.map_or_else(String::new, |r| {
                format!("record {}.", types.canonical_name(Ty::Record(r)))
            });
            let ret = ret.map_or_else(String::new, |r| format!(": {}", types.canonical_name(r)));
            format_bridge_name(format_args!("{head}({}){ret}", params_text(types, &params)))
        }
        Shape::Ctor { owner, params, .. } => {
            let ty = match types.get(owner).kind {
                TypeKind::Record { .. } => Ty::Record(owner),
                _ => Ty::Class(owner),
            };
            format_bridge_name(format_args!(
                "constructor {}.{}({})",
                types.canonical_name(ty),
                m.name,
                params_text(types, &params)
            ))
        }
    })
}

/// The name of the function making the instance of the attribute `n` of `Program::attrs`
/// (`docs/spec/rtti.md` 7.4).
pub fn attr_func_name(n: usize) -> String {
    format!("<attr:{n}>")
}

/// The methods whose bodies the bridges call for what `wants` asks of the type `id` (every
/// method listed for `Invoke`, the methods of the properties listed for the properties), with
/// the type declaring them and their index among its methods.
pub(super) fn invoked_methods(
    types: &TypeTable,
    id: TypeId,
    wants: RttiWants,
) -> Vec<(TypeId, u32, ClassMethod)> {
    let mut out = Vec::new();
    let (methods, props) = match &types.get(id).kind {
        TypeKind::Record {
            methods,
            properties,
            ..
        }
        | TypeKind::Class {
            methods,
            properties,
            ..
        } => (methods.as_slice(), properties.as_slice()),
        _ => return out,
    };
    if types.is_generic(id) || types.has_param(Ty::Record(id)) {
        return out;
    }
    if wants.invoke {
        for (k, m) in methods.iter().enumerate() {
            if lists_method(types, id, m) {
                out.push((id, k as u32, m.clone()));
            }
        }
    }
    if wants.props && !TypeTable::is_builtin(id) {
        for p in props.iter().filter(|p| lists_property(p)) {
            for a in [&p.read, &p.write] {
                if let Some((owner, k, m)) = accessor_method(types, id, a) {
                    out.push((owner, k, m));
                }
            }
        }
    }
    out
}

impl Inst<'_> {
    /// Makes the bridges of the methods `wants` asks for and the functions of the attributes
    /// (`docs/spec/rtti.md` 7.3, 7.4), once every type and every body is known (after the
    /// instantiations' bodies). Returns the routines made and the methods they call, which no
    /// body refers to: the analysis keeps them (`sema::mark_unreached`).
    pub(super) fn generate_rtti(&mut self, wants: RttiWants, attrs: &[AttrUse]) -> Vec<RoutineId> {
        let mut roots = Vec::new();
        let Some(tvalue) = self.types.tvalue() else {
            return roots;
        };
        if wants.invoke || wants.props {
            let args = self.types.dyn_array(
                tvalue,
                DynWritten::Anonymous {
                    generic: true,
                    span: TextRange::default(),
                },
            );
            let count = self.types.defs.len();
            for t in 0..count {
                let id = TypeId(t as u32);
                for (owner, k, m) in invoked_methods(self.types, id, wants) {
                    roots.extend(m.routine);
                    let Some(name) = bridge_name(self.types, owner, &m) else {
                        continue;
                    };
                    let r = match self.inst.bridges.get(&name) {
                        Some(&r) => r,
                        None => {
                            let Some(shape) = shape(self.types, owner, &m) else {
                                continue;
                            };
                            let r = self.bridge(&name, shape, tvalue, args, owner);
                            self.inst.bridges.insert(name, r);
                            roots.push(r);
                            r
                        }
                    };
                    self.inst.method_bridges.insert((owner, k), r);
                }
            }
        }
        if wants.attrs {
            for (n, a) in attrs.iter().enumerate() {
                if let Some(r) = self.attr_func(n, a) {
                    self.inst.attr_funcs.insert(n, r);
                    roots.push(r);
                    roots.extend(a.ctor);
                }
            }
        }
        roots
    }

    /// Adds a generated routine with its body (as `key_func` does).
    fn add_generated(
        &mut self,
        name: String,
        hir: Routine,
        params: Vec<ParamInfo>,
        ret: Ty,
        unit: u32,
    ) -> RoutineId {
        let r = RoutineId(self.routines.len() as u32);
        let file = self
            .inst
            .unit_files
            .get(unit as usize)
            .copied()
            .unwrap_or(crate::source::FileId(0));
        self.routines.push(super::RoutineInfo {
            name: name.clone(),
            params,
            ret: Some(ret),
            has_body: true,
            forward_span: None,
            implicit_forward: true,
            name_span: TextRange::default(),
            hir: Some(Box::new(hir)),
            overload: false,
            overload_fix_at: None,
            unit,
            file,
            interface: false,
            parent: None,
            scope: None,
            calls: Vec::new(),
            method_of: None,
            self_kind: SelfKind::Instance,
            is_abstract: false,
            native: false,
            body_span: None,
            closure: false,
            type_params: Vec::new(),
            closure_info: super::ClosureInfo::default(),
        });
        self.inst.generated_names.insert(r, name);
        self.inst.order.push(r);
        r
    }

    /// The unit of the type `id`'s declaration (the program's for a type without one).
    fn unit_of(&self, id: TypeId) -> u32 {
        match &self.types.get(id).kind {
            TypeKind::Record { unit: Some(u), .. } | TypeKind::Class { unit: Some(u), .. } => *u,
            _ => self.unit,
        }
    }

    /// The bridge `name` of `shape` (`docs/spec/rtti.md` 7.3): `function(const Target: <method
    /// value>; const Instance: TValue; const Args: TArray<TValue>): TValue`.
    fn bridge(
        &mut self,
        name: &str,
        shape: Shape,
        tvalue: Ty,
        args: Ty,
        owner: TypeId,
    ) -> RoutineId {
        let g = Gen {
            span: TextRange::default(),
        };
        let params = match &shape {
            Shape::Call { params, .. } | Shape::Ctor { params, .. } => params.clone(),
        };
        // `Target`'s type: a method reference of the shape (a record's `Self` first), so that
        // calling it passes the receiver the value holds (`CallClosure`)
        let (record, ret) = match &shape {
            Shape::Call { record, ret, .. } => (*record, *ret),
            Shape::Ctor { .. } => (None, None),
        };
        let mut proc_params: Vec<ProcParam> = Vec::new();
        if let Some(r) = record {
            proc_params.push(ProcParam {
                name: "Self".into(),
                ty: Ty::Record(r),
                mode: ParamMode::Var,
            });
        }
        proc_params.extend(params.iter().enumerate().map(|(i, (t, m))| ProcParam {
            name: format!("A{i}"),
            ty: *t,
            mode: *m,
        }));
        let target_ty = Ty::Callable(self.types.add(TypeDef {
            name: None,
            full_name: None,
            kind: TypeKind::Proc {
                family: ProcFamily::Reference,
                params: proc_params,
                ret,
            },
            span: TextRange::default(),
        }));
        let param = |n: &str, ty: Ty| {
            let mut v = g.decl(n, ty, VarKind::Param(ParamMode::Const));
            v.written = n == "Args";
            v
        };
        let mut vars = vec![
            param("Target", target_ty),
            param("Instance", tvalue),
            param("Args", args),
            g.decl("Result", tvalue, VarKind::Result),
        ];
        const TARGET: u32 = 0;
        const INSTANCE: u32 = 1;
        const ARGS: u32 = 2;
        const RESULT: u32 = 3;
        let mut body = Vec::new();
        let elem = |i: usize| {
            g.expr(
                ExprKind::Elem {
                    base: Box::new(g.var(ARGS, args)),
                    index: Box::new(g.constant(super::types::Const::Int(i as i128), Ty::Integer)),
                },
                tvalue,
            )
        };
        // `X := ValueCast(T, V)` after `ValueIs(T, V, True)` (`EInvalidCast` if it does not match)
        let unwrap = |v: u32, ty: Ty, from: Expr, body: &mut Vec<Stmt>| {
            let info = g.expr(ExprKind::TypeInfo(ty), Ty::TypeInfo);
            body.push(Stmt {
                kind: StmtKind::Expr(g.expr(
                    ExprKind::Builtin {
                        func: Builtin::ValueIs,
                        args: vec![info.clone(), from.clone(), g.boolean(true)],
                    },
                    Ty::Boolean,
                )),
                span: g.span,
            });
            body.push(g.assign(
                v,
                g.expr(
                    ExprKind::Builtin {
                        func: Builtin::ValueCast,
                        args: vec![info, from],
                    },
                    ty,
                ),
            ));
        };
        let wrap = |e: Expr, ty: Ty| {
            g.expr(
                ExprKind::Builtin {
                    func: Builtin::ValueWrap,
                    args: vec![e, g.expr(ExprKind::TypeInfo(ty), Ty::TypeInfo)],
                },
                tvalue,
            )
        };
        // The local of each argument (an open array's as a dynamic array), read from `Args`
        // unless `out`
        let mut locals = Vec::new();
        for (i, (t, mode)) in params.iter().enumerate() {
            let ty = match t {
                Ty::OpenArray(id) => Ty::DynArray(*id),
                t => *t,
            };
            let v = vars.len() as u32;
            let mut var = g.decl(&format!("A{i}"), ty, VarKind::Local);
            var.written = true;
            vars.push(var);
            if *mode != ParamMode::Out {
                unwrap(v, ty, elem(i), &mut body);
            }
            locals.push((v, ty, *mode));
        }
        let call_args = |locals: &[(u32, Ty, ParamMode)]| -> Vec<CallArg> {
            locals
                .iter()
                .map(|&(v, _, mode)| match mode {
                    ParamMode::Var | ParamMode::Out => CallArg::Ref {
                        place: Place::Var(VarId(v), g.span),
                        is_out: mode == ParamMode::Out,
                    },
                    _ => CallArg::Value(g.var(v, locals_ty(locals, v))),
                })
                .collect()
        };
        match shape {
            Shape::Call { record, ret, .. } => {
                let mut args = Vec::new();
                if let Some(r) = record {
                    let v = vars.len() as u32;
                    let mut var = g.decl("Self", Ty::Record(r), VarKind::Local);
                    var.written = true;
                    vars.push(var);
                    unwrap(v, Ty::Record(r), g.var(INSTANCE, tvalue), &mut body);
                    args.push(CallArg::Ref {
                        place: Place::Var(VarId(v), g.span),
                        is_out: false,
                    });
                }
                args.extend(call_args(&locals));
                let call = g.expr(
                    ExprKind::CallValue {
                        callee: Box::new(g.var(TARGET, target_ty)),
                        args,
                    },
                    ret.unwrap_or(Ty::Error),
                );
                match ret {
                    Some(rt) => body.push(g.assign(RESULT, wrap(call, rt))),
                    None => body.push(Stmt {
                        kind: StmtKind::Expr(call),
                        span: g.span,
                    }),
                }
            }
            Shape::Ctor { owner, ctor, .. } => {
                let made = match self.types.get(owner).kind {
                    TypeKind::Record { .. } => {
                        let ty = Ty::Record(owner);
                        let mut a = vec![CallArg::Temp(g.expr(
                            ExprKind::Builtin {
                                func: Builtin::Default,
                                args: Vec::new(),
                            },
                            ty,
                        ))];
                        a.extend(call_args(&locals));
                        let call = g.expr(
                            ExprKind::Call {
                                routine: ctor.expect("a record's constructor has a routine"),
                                args: a,
                                dispatch: Dispatch::Routine,
                            },
                            Ty::Error,
                        );
                        g.expr(
                            ExprKind::RecordNew {
                                call: Box::new(call),
                            },
                            ty,
                        )
                    }
                    _ => {
                        // `C.Create(…)` of the class reference `Instance` holds
                        // (`docs/spec/objects.md` 3.1)
                        let tclass = Ty::ClassRef(super::types::TCLASS);
                        let of = g.expr(
                            ExprKind::Builtin {
                                func: Builtin::ValueCast,
                                args: vec![
                                    g.expr(ExprKind::TypeInfo(tclass), Ty::TypeInfo),
                                    g.var(INSTANCE, tvalue),
                                ],
                            },
                            tclass,
                        );
                        let dispatch = ctor
                            .and_then(|r| {
                                self.types
                                    .class_methods(owner)
                                    .iter()
                                    .find(|m| m.routine == Some(r))
                                    .and_then(|m| m.vmt)
                            })
                            .map_or(Dispatch::Method, Dispatch::Virtual);
                        g.expr(
                            ExprKind::New {
                                class: Ty::Class(owner),
                                ctor,
                                args: call_args(&locals),
                                after: true,
                                of: Some(Box::new(of)),
                                dispatch,
                            },
                            Ty::Class(owner),
                        )
                    }
                };
                let ty = made.ty;
                body.push(g.assign(RESULT, wrap(made, ty)));
            }
        }
        // The `var` / `out` arguments written back (shared with the caller's `Args`)
        for (i, &(v, ty, mode)) in locals.iter().enumerate() {
            if matches!(mode, ParamMode::Var | ParamMode::Out) {
                body.push(Stmt {
                    kind: StmtKind::Assign {
                        target: Place::Elem {
                            base: Box::new(Place::Var(VarId(ARGS), g.span)),
                            index: Box::new(
                                g.constant(super::types::Const::Int(i as i128), Ty::Integer),
                            ),
                            ty: tvalue,
                            span: g.span,
                        },
                        value: wrap(g.var(v, ty), ty),
                        check: Checks {
                            overflow: false,
                            range: false,
                        },
                        is_init: false,
                    },
                    span: g.span,
                });
            }
        }
        let unit = self.unit_of(owner);
        let hir = g.routine(name.to_string(), vars, 3, tvalue, body, unit);
        let params = ["Target", "Instance", "Args"]
            .into_iter()
            .zip([target_ty, tvalue, args])
            .map(|(n, ty)| ParamInfo {
                name: n.into(),
                ty,
                mode: ParamMode::Const,
                span: TextRange::default(),
                default: None,
                attrs: None,
            })
            .collect();
        self.add_generated(name.to_string(), hir, params, tvalue, unit)
    }

    /// The function making the instance of the attribute `a` (`docs/spec/rtti.md` 7.4):
    /// `Result := TFooAttribute.Create(…)` with its constant arguments. `None` if its class is
    /// not known.
    fn attr_func(&mut self, n: usize, a: &AttrUse) -> Option<RoutineId> {
        let class = (0..self.types.defs.len())
            .map(|k| TypeId(k as u32))
            .find(|&id| {
                matches!(self.types.get(id).kind, TypeKind::Class { .. })
                    && self.types.get(id).full_name.as_deref() == Some(a.class.as_str())
            })?;
        let g = Gen {
            span: TextRange::default(),
        };
        let ty = Ty::Class(class);
        let dispatch = a
            .ctor
            .and_then(|r| {
                self.types
                    .class_methods(class)
                    .iter()
                    .find(|m| m.routine == Some(r))
                    .and_then(|m| m.vmt)
            })
            .map_or(Dispatch::Method, Dispatch::Virtual);
        let made = g.expr(
            ExprKind::New {
                class: ty,
                ctor: a.ctor,
                args: a.args.iter().cloned().map(CallArg::Value).collect(),
                after: self.types.has_after_construction(class),
                of: None,
                dispatch,
            },
            ty,
        );
        let vars = vec![g.decl("Result", ty, VarKind::Result)];
        let body = vec![g.assign(0, made)];
        let name = attr_func_name(n);
        let hir = g.routine(name.clone(), vars, 0, ty, body, a.unit);
        Some(self.add_generated(name, hir, Vec::new(), ty, a.unit))
    }
}

/// The type of the local `v` among the bridge's arguments.
fn locals_ty(locals: &[(u32, Ty, ParamMode)], v: u32) -> Ty {
    locals.iter().find(|l| l.0 == v).map_or(Ty::Error, |l| l.1)
}

/// The bridges and attribute functions of a program, for the type information section.
#[derive(Debug, Clone, Default)]
pub struct RttiFuncs {
    /// The bridge of each method: by the type declaring it and its index among the type's
    /// methods.
    pub bridges: HashMap<(TypeId, u32), RoutineId>,
    /// The function of each attribute of `Program::attrs`, by its index.
    pub attrs: HashMap<usize, RoutineId>,
}
