//! The key equality and key hash the analysis generates (`docs/spec/equality.md`): the bodies of
//! the `Equals` / `GetHashCode` overrides of the `[ValueEquality]` classes (3.4), and the
//! `<eq:…>` / `<hash:…>` functions of the records and arrays whose key equality or hash a
//! [`ExprKind::Key`] calls (8.3), and the `<cmp:…>` functions of the default order
//! (`docs/spec/collections.md` 10.2). Lower expands each `ExprKind::Key` by the kind of its type
//! ([`super::types::KeyKind`]); the analysis only finds the routine a record or an array calls.

use super::hir::*;
use super::hirwalk::{HirVisit, walk_routine};
use super::instances::Inst;
use super::types::{
    ClassMethod, MethodKind, SelfKind, TCLASS, TOBJECT, Ty, TypeId, TypeKind, VMT_EQUALS,
    VMT_GET_HASH_CODE, Visibility, VmtEntry,
};
use super::{ParamInfo, RoutineInfo, Sema};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::{FileId, TextRange};
use crate::syntax::ast::ParamMode;

/// What the declaration checks of a class or record decide about its equality
/// (`docs/spec/equality.md` 3.4, 3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EqualityDecl {
    /// An E0767 or E0765 of `[ValueEquality]`: no other check of E0765 and nothing generated.
    Errored,
    /// Treated as `[ValueEquality]` without an error: `Equals` / `GetHashCode` are generated.
    Generate,
    /// Neither: the checks of `Equals`, `GetHashCode` and `Equal` (3.1, 3.2, 3.3).
    Plain,
}

impl Sema<'_, '_> {
    /// The checks of the equality attributes of a class or record (`docs/spec/equality.md` 3.4,
    /// the first two steps of 3.5), made with its methods (`methods`, and `eq_hash` its
    /// `override` declarations of `Equals` and `GetHashCode`). Decides whether a class is treated
    /// as `[ValueEquality]` (written on it, or inherited unless it overrides both or declares its
    /// own `Equal`), and checks the `[EqualityMember]` of its fields.
    pub(super) fn equality_decl(
        &mut self,
        id: TypeId,
        is_record: bool,
        methods: &[ClassMethod],
        eq_hash: [Option<(bool, TextRange, TextRange)>; 2],
    ) -> EqualityDecl {
        let name_span = self.types.get(id).span;
        let mut errored = self.equality_attrs.errored_decls.contains(&name_span)
            || self.equality_attrs.errored_classes.contains(&id);
        if is_record {
            return if errored {
                EqualityDecl::Errored
            } else {
                EqualityDecl::Plain
            };
        }
        let written = self
            .equality_attrs
            .value_equality
            .iter()
            .find(|(n, _)| *n == name_span)
            .map(|(_, a)| *a);
        let own = Ty::Class(id);
        let own_equal = super::types::own_equal(methods, own);
        let inherited = self
            .types
            .parent(id)
            .is_some_and(|p| self.types.is_value_equality(p));
        let both = eq_hash[0].is_some() && eq_hash[1].is_some();
        let treated = written.is_some() || (inherited && own_equal.is_none() && !both);
        if treated {
            self.types.value_equality.insert(id);
        }
        // 1. `[EqualityMember]` on the fields of a class not treated as `[ValueEquality]` (E0767)
        let fields: Vec<(TextRange, u32)> = match &self.types.get(id).kind {
            TypeKind::Class { fields, .. } => fields.iter().map(|f| (f.span, f.slot)).collect(),
            _ => Vec::new(),
        };
        let marks: Vec<(Vec<TextRange>, TextRange, TextRange)> = self
            .equality_attrs
            .members
            .iter()
            .filter(|m| m.class == id)
            .map(|m| (m.names.clone(), m.attr, m.removal))
            .collect();
        for (names, attr, removal) in marks {
            if treated {
                for (span, slot) in &fields {
                    if names.contains(span) {
                        self.types.equality_members.insert((id, *slot));
                    }
                }
            } else {
                self.misplaced_equality_attribute("EqualityMember", attr, removal, false);
                errored = true;
            }
        }
        if errored {
            return EqualityDecl::Errored;
        }
        let class_name = self.tn(own);
        // 2. `[ValueEquality]` written with the user's `Equals` / `GetHashCode` or own `Equal`
        if let Some(attr) = written {
            let (message, at) = if let Some(eq) = own_equal {
                (
                    "`[ValueEquality]` cannot be used with operator `Equal`".to_string(),
                    eq.span,
                )
            } else if let Some((_, at, _)) = eq_hash[0].or(eq_hash[1]) {
                (
                    format!(
                        "`{class_name}` has `[ValueEquality]`, so `Equals` and `GetHashCode` are generated and cannot be overridden"
                    ),
                    at,
                )
            } else {
                return EqualityDecl::Generate;
            };
            let d = Diagnostic::error(Code::E(765), message, Stage::Sema)
                .with_primary(self.span(at), "")
                .with_secondary(self.span(attr), "`[ValueEquality]` written here");
            self.push(d);
            return EqualityDecl::Errored;
        }
        if treated {
            // Inherited: overriding only one of the two is 3.1's E0765
            if eq_hash[0].is_some() || eq_hash[1].is_some() {
                self.equals_without_hash(&class_name, eq_hash);
                return EqualityDecl::Errored;
            }
            return EqualityDecl::Generate;
        }
        EqualityDecl::Plain
    }

    /// The `Equals` / `GetHashCode` overrides generated for a class treated as
    /// `[ValueEquality]` (`docs/spec/equality.md` 3.4, 8.3): methods of the class at the entries
    /// 4 and 5 of `vmt`, whose bodies `sema::finish` makes.
    pub(super) fn declare_value_equality(
        &mut self,
        id: TypeId,
        methods: &mut Vec<ClassMethod>,
        vmt: &mut [VmtEntry],
    ) {
        let span = self.types.get(id).span;
        let class_name = self.types.name(Ty::Class(id));
        let mut make = |s: &mut Self, name: &str, params: Vec<ParamInfo>, ret: Ty, k: u32| {
            let routine = RoutineId(s.routines.len() as u32);
            let sig = params.iter().map(|p| (p.ty, p.mode)).collect();
            s.routines.push(RoutineInfo {
                name: format!("{class_name}.{name}"),
                params,
                ret: Some(ret),
                has_body: false,
                forward_span: None,
                // Its body is made, not written: no E0723
                implicit_forward: true,
                name_span: span,
                hir: None,
                overload: false,
                overload_fix_at: None,
                unit: s.unit_index,
                file: s.input.file,
                interface: s.in_interface_part,
                parent: None,
                calls: Vec::new(),
                method_of: Some(id),
                self_kind: SelfKind::Instance,
                is_abstract: false,
                body_span: None,
                closure: false,
                type_params: Vec::new(),
                closure_info: super::ClosureInfo::default(),
            });
            if let Some(e) = vmt.get_mut(k as usize) {
                *e = VmtEntry::Func(routine);
            }
            methods.push(ClassMethod {
                name: name.into(),
                routine: Some(routine),
                span,
                vis: Visibility::Public,
                kind: MethodKind::Function,
                params: sig,
                ret: Some(ret),
                vmt: Some(k),
                is_override: true,
                is_abstract: false,
                is_final: false,
                self_kind: SelfKind::Instance,
            });
            routine
        };
        let obj = ParamInfo {
            name: "Obj".into(),
            ty: Ty::Class(TOBJECT),
            mode: ParamMode::Value,
            span,
            default: None,
            attrs: None,
        };
        let equals = make(self, "Equals", vec![obj], Ty::Boolean, VMT_EQUALS);
        let hash = make(
            self,
            "GetHashCode",
            Vec::new(),
            Ty::Int64,
            VMT_GET_HASH_CODE,
        );
        self.instances.generated_value_eq.push((id, equals, hash));
    }
}

/// Builds the HIR of a generated body, every node at one position.
struct Gen {
    span: TextRange,
}

impl Gen {
    fn expr(&self, kind: ExprKind, ty: Ty) -> Expr {
        Expr {
            kind,
            ty,
            value: None,
            span: self.span,
        }
    }

    fn var(&self, v: u32, ty: Ty) -> Expr {
        self.expr(ExprKind::Var(VarId(v)), ty)
    }

    fn constant(&self, value: super::types::Const, ty: Ty) -> Expr {
        Expr {
            kind: ExprKind::Const,
            ty,
            value: Some(value),
            span: self.span,
        }
    }

    /// An ordinal constant of the type `ty` (a bound of a static array's index).
    fn ordinal(&self, n: i128, ty: Ty) -> Expr {
        use super::types::Const;
        let value = match ty {
            Ty::Boolean => Const::Bool(n != 0),
            Ty::Char => Const::Char(u16::try_from(n).expect("a `Char` bound is a code unit")),
            _ => Const::Int(n),
        };
        self.constant(value, ty)
    }

    fn int64(&self, n: usize) -> Expr {
        self.constant(super::types::Const::Int(n as i128), Ty::Int64)
    }

    fn boolean(&self, b: bool) -> Expr {
        self.constant(super::types::Const::Bool(b), Ty::Boolean)
    }

    /// `KeyEquals(A, B)` / `KeyHash(X)`; its routine is found once the body is filled
    /// ([`Inst::fill_keys`]).
    fn key(&self, op: KeyOp, args: Vec<Expr>) -> Expr {
        let ty = match op {
            KeyOp::Equals => Ty::Boolean,
            KeyOp::Hash => Ty::Int64,
            KeyOp::Compare => Ty::Integer,
        };
        self.expr(
            ExprKind::Key {
                op,
                args,
                func: None,
            },
            ty,
        )
    }

    /// `First and R1 and R2 …` (short-circuit).
    fn and_all(&self, first: Expr, rest: Vec<Expr>) -> Expr {
        if rest.is_empty() {
            return first;
        }
        let steps = rest
            .into_iter()
            .map(|rhs| BinStep {
                op: BinOp::And,
                rhs,
                calc: Calc::Bool,
                ty: Ty::Boolean,
                span: self.span,
            })
            .collect();
        self.expr(
            ExprKind::Binary {
                first: Box::new(first),
                steps,
            },
            Ty::Boolean,
        )
    }

    /// `L op R` of two values compared or computed as `calc`.
    fn binary(&self, l: Expr, op: BinOp, r: Expr, calc: Calc, ty: Ty) -> Expr {
        self.expr(
            ExprKind::Binary {
                first: Box::new(l),
                steps: vec![BinStep {
                    op,
                    rhs: r,
                    calc,
                    ty,
                    span: self.span,
                }],
            },
            ty,
        )
    }

    /// `HashCombine(H, X)`.
    fn combine(&self, h: Expr, x: Expr) -> Expr {
        self.expr(
            ExprKind::Builtin {
                func: Builtin::HashCombine,
                args: vec![h, x],
            },
            Ty::Int64,
        )
    }

    /// `Length(A)` of a dynamic array, widened to `Int64` (the loop over its elements counts in
    /// `Int64`).
    fn length(&self, a: Expr) -> Expr {
        self.expr(
            ExprKind::Builtin {
                func: Builtin::Length,
                args: vec![a],
            },
            Ty::Int64,
        )
    }

    /// `High(A)` of a dynamic array, in `Int64`.
    fn high(&self, a: Expr) -> Expr {
        self.expr(
            ExprKind::Builtin {
                func: Builtin::High,
                args: vec![a],
            },
            Ty::Int64,
        )
    }

    fn assign(&self, v: u32, value: Expr) -> Stmt {
        Stmt {
            kind: StmtKind::Assign {
                target: Place::Var(VarId(v), self.span),
                value,
                check: Checks {
                    overflow: false,
                    range: false,
                },
                is_init: false,
            },
            span: self.span,
        }
    }

    fn if_then(&self, cond: Expr, then: Stmt) -> Stmt {
        Stmt {
            kind: StmtKind::If {
                branches: vec![(cond, then)],
                else_body: None,
            },
            span: self.span,
        }
    }

    fn not(&self, e: Expr) -> Expr {
        self.expr(
            ExprKind::Unary {
                op: UnOp::Not,
                operand: Box::new(e),
                calc: Calc::Bool,
            },
            Ty::Boolean,
        )
    }

    fn exit(&self) -> Stmt {
        Stmt {
            kind: StmtKind::Exit(None),
            span: self.span,
        }
    }

    /// `for V := From to To do Body` over the local variable `v`.
    fn for_to(&self, v: u32, from: Expr, to: Expr, body: Stmt) -> Stmt {
        Stmt {
            kind: StmtKind::For {
                var: ForVar::Local(VarId(v)),
                var_span: self.span,
                from,
                to,
                downto: false,
                body: Box::new(body),
                checks: Checks {
                    overflow: false,
                    range: false,
                },
                counter: None,
            },
            span: self.span,
        }
    }

    fn block(&self, stmts: Vec<Stmt>) -> Stmt {
        Stmt {
            kind: StmtKind::Block(stmts),
            span: self.span,
        }
    }

    fn decl(&self, name: &str, ty: Ty, kind: VarKind) -> Var {
        Var {
            name: name.into(),
            ty,
            kind,
            span: self.span,
            decl: None,
            is_for_var: false,
            is_loop_var: false,
            captured: false,
            read_by_nested: false,
            written_by_nested: None,
            written: false,
            cell: false,
        }
    }

    /// The loop variable of a generated body.
    fn loop_var(&self, ty: Ty) -> Var {
        Var {
            is_loop_var: true,
            written: true,
            ..self.decl("I", ty, VarKind::Local)
        }
    }

    /// The field of a record read: a `[weak]` one through `ExprKind::Weak`.
    fn field(&self, base: Expr, index: u32, name: &str, ty: Ty, strong: Ty) -> Expr {
        let read = self.expr(
            ExprKind::Field {
                base: Box::new(base),
                index,
                name: name.into(),
            },
            ty,
        );
        match ty {
            Ty::Weak(_) => self.expr(ExprKind::Weak(Box::new(read)), strong),
            _ => read,
        }
    }

    fn routine(
        &self,
        name: String,
        vars: Vec<Var>,
        param_count: u32,
        ret: Ty,
        body: Vec<Stmt>,
        unit: u32,
    ) -> Routine {
        Routine {
            name,
            vars,
            param_count,
            ret: Some(ret),
            body,
            end_span: self.span,
            body_begin: None,
            name_span: self.span,
            has_syntax_errors: false,
            has_errors: false,
            unit,
            parent: None,
            link: false,
            is_abstract: false,
            builtin: false,
            generic: false,
            dtor_of: None,
            helper: false,
            operator: false,
            closure: false,
            captures: Vec::new(),
            groups: Default::default(),
            instance: None,
        }
    }
}

/// The walk filling in the routines of the [`ExprKind::Key`]s of a body.
struct KeyFill<'i, 'x> {
    inst: &'i mut Inst<'x>,
    /// The unit and the file of the routine filled (the generated functions it asks for first
    /// are shown with its unit in the `hir` dump).
    unit: u32,
    file: FileId,
}

impl HirVisit for KeyFill<'_, '_> {
    fn ty(&mut self, _t: &mut Ty) {}

    fn key(&mut self, op: KeyOp, ty: Ty, func: &mut Option<RoutineId>, span: TextRange) {
        if func.is_none() {
            *func = self.inst.key_func(op, ty, (self.unit, self.file, span));
        }
    }
}

impl Inst<'_> {
    /// The bodies of the generated `Equals` / `GetHashCode` overrides of the `[ValueEquality]`
    /// classes (`docs/spec/equality.md` 3.4, 8.3), once every class of the program is known:
    /// `Result := (Obj is TFoo) and (Obj.ClassType = Self.ClassType); if Result then begin O :=
    /// TFoo(Obj); Result := KeyEquals(Self.F1, O.F1) and … end`, and `Result := KeyHash(N);
    /// Result := HashCombine(Result, KeyHash(Self.F1)); …` over the compared fields (`N` of
    /// them). A class's own come after
    /// every unit's functions; a generic type's are copied into each instantiation by the work
    /// list, as its other methods.
    pub fn generate_value_equality(&mut self) {
        let generated = self.inst.generated_value_eq.clone();
        for (class, equals, hash) in generated {
            let fields: Vec<(u32, String, Ty)> = self
                .types
                .equality_fields(class)
                .into_iter()
                .map(|f| (f.slot, f.name.clone(), f.ty))
                .collect();
            let info = &self.routines[equals.0 as usize];
            let (unit, g) = (
                info.unit,
                Gen {
                    span: info.name_span,
                },
            );
            let own = Ty::Class(class);
            let obj = || g.var(1, Ty::Class(TOBJECT));
            let field = |base: Expr, slot: u32, name: &str, ty: Ty| {
                g.expr(
                    ExprKind::ObjField {
                        base: Box::new(base),
                        slot,
                        name: name.into(),
                    },
                    ty,
                )
            };
            let class_type = |e: Expr| {
                g.expr(
                    ExprKind::ClassInfo {
                        op: ClassInfoOp::ClassType,
                        args: vec![e],
                    },
                    Ty::ClassRef(TCLASS),
                )
            };
            let is = g.expr(
                ExprKind::ClassCheck {
                    operand: Box::new(obj()),
                    class,
                    op: ClassCheckOp::Is,
                },
                Ty::Boolean,
            );
            let same_class = g.and_all(
                is,
                vec![g.binary(
                    class_type(obj()),
                    BinOp::Eq,
                    class_type(g.var(0, own)),
                    Calc::Ref,
                    Ty::Boolean,
                )],
            );
            // `O := TFoo(Obj)` once, then the fields of `Self` and `O` compared
            let mut stmts = vec![g.assign(2, same_class)];
            let mut keys = fields.iter().map(|(slot, name, ty)| {
                g.key(
                    KeyOp::Equals,
                    vec![
                        field(g.var(0, own), *slot, name, *ty),
                        field(g.var(3, own), *slot, name, *ty),
                    ],
                )
            });
            if let Some(first) = keys.next() {
                let cast = g.expr(
                    ExprKind::ClassCheck {
                        operand: Box::new(obj()),
                        class,
                        op: ClassCheckOp::Cast,
                    },
                    own,
                );
                let compare = g.and_all(first, keys.collect());
                stmts.push(g.if_then(
                    g.var(2, Ty::Boolean),
                    g.block(vec![g.assign(3, cast), g.assign(2, compare)]),
                ));
            }
            let name = self.routines[equals.0 as usize].name.clone();
            let body = g.routine(
                name,
                vec![
                    g.decl("Self", own, VarKind::SelfParam),
                    g.decl("Obj", Ty::Class(TOBJECT), VarKind::Param(ParamMode::Value)),
                    g.decl("Result", Ty::Boolean, VarKind::Result),
                    Var {
                        written: true,
                        ..g.decl("O", own, VarKind::Local)
                    },
                ],
                2,
                Ty::Boolean,
                stmts,
                unit,
            );
            self.set_generated_body(equals, body, class);
            let mut stmts = vec![g.assign(1, g.key(KeyOp::Hash, vec![g.int64(fields.len())]))];
            for (slot, name, ty) in &fields {
                let h = g.key(KeyOp::Hash, vec![field(g.var(0, own), *slot, name, *ty)]);
                stmts.push(g.assign(1, g.combine(g.var(1, Ty::Int64), h)));
            }
            let info = &self.routines[hash.0 as usize];
            let g = Gen {
                span: info.name_span,
            };
            let body = g.routine(
                info.name.clone(),
                vec![
                    g.decl("Self", own, VarKind::SelfParam),
                    g.decl("Result", Ty::Int64, VarKind::Result),
                ],
                1,
                Ty::Int64,
                stmts,
                info.unit,
            );
            self.set_generated_body(hash, body, class);
            self.inst.key_used = true;
        }
    }

    /// Gives a generated method of `class` its body; a class's own comes after every unit's
    /// functions (a generic type's is copied into its instantiations).
    fn set_generated_body(&mut self, r: RoutineId, body: Routine, class: TypeId) {
        self.routines[r.0 as usize].hir = Some(body);
        self.routines[r.0 as usize].has_body = true;
        if !self.types.is_generic(class) {
            self.inst.order.push(r);
        }
    }

    /// Fills in the routines of the key equalities and hashes of every routine with a body that
    /// is not generic, and of `roots` (`<main>`, the units' `<init>` / `<finalization>`, with
    /// their units and files), with the generated functions they ask for.
    pub fn fill_all_keys(&mut self, roots: Vec<(&mut Routine, u32, FileId)>) {
        if !self.inst.key_used {
            return;
        }
        let n = self.routines.len();
        for i in 0..n {
            let r = RoutineId(i as u32);
            if self.routines[i].hir.is_some() && !self.is_generic_routine(r) {
                self.fill_keys(r);
            }
        }
        for (h, unit, file) in roots {
            walk_routine(
                h,
                &mut KeyFill {
                    inst: self,
                    unit,
                    file,
                },
            );
            self.fill_pending_keys();
        }
    }

    /// Fills in the routines of the key equalities and hashes of a routine's body, and of the
    /// generated functions they ask for (`docs/spec/equality.md` 8.3).
    pub fn fill_keys(&mut self, r: RoutineId) {
        if !self.inst.key_used {
            return;
        }
        self.inst.key_pending.push(r);
        self.fill_pending_keys();
    }

    /// The bodies waiting to be filled: a loop, not a recursion.
    fn fill_pending_keys(&mut self) {
        while let Some(r) = self.inst.key_pending.pop() {
            let info = &mut self.routines[r.0 as usize];
            let Some(mut h) = info.hir.take() else {
                continue;
            };
            let (unit, file) = (info.unit, info.file);
            walk_routine(
                &mut h,
                &mut KeyFill {
                    inst: self,
                    unit,
                    file,
                },
            );
            self.routines[r.0 as usize].hir = Some(h);
        }
    }

    /// The routine the key equality or hash of `ty` calls (`docs/spec/equality.md` 8.3): a
    /// record's own `Equal` / `GetHashCode` (3.2), or the generated function of a record or an
    /// array (made once per type and routine, the first time asked); for the default order, the
    /// generated `<cmp:…>` of a record or a class with its own `LessThan`
    /// (`docs/spec/collections.md` 10.2). `None` for the other types and for a type with a type
    /// parameter in it. `at` is where it is asked: the unit and the
    /// file of the routine asking, and the expression (an array's function is placed there, a
    /// record's at the record's declaration).
    fn key_func(
        &mut self,
        op: KeyOp,
        ty: Ty,
        (unit, file, at): (u32, FileId, TextRange),
    ) -> Option<RoutineId> {
        if self.types.has_param(ty) {
            return None;
        }
        let ty = match ty {
            Ty::OpenArray(id) => Ty::DynArray(id),
            t => t,
        };
        let less_than = match (op, ty) {
            // The default order: only of a type with its own `LessThan`
            // (`docs/spec/collections.md` 10.2)
            (KeyOp::Compare, _) => Some(self.types.own_less_than(ty)?),
            (_, Ty::Record(id)) if self.types.record_declares_equal(id) => {
                let (equal, hash) = self.types.record_key_methods(id)?;
                return Some(match op {
                    KeyOp::Equals => equal,
                    KeyOp::Hash => hash,
                    KeyOp::Compare => unreachable!("the default order is matched first"),
                });
            }
            (_, Ty::Record(_) | Ty::Array(_) | Ty::DynArray(_)) => None,
            _ => return None,
        };
        if let Some(&r) = self.inst.key_funcs.get(&(op, ty)) {
            return Some(r);
        }
        let full = self.types.canonical_name(ty);
        let name = match op {
            KeyOp::Equals => format!("<eq:{full}>"),
            KeyOp::Hash => format!("<hash:{full}>"),
            KeyOp::Compare => format!("<cmp:{full}>"),
        };
        // A record's or a class's at its declaration
        let id = ty
            .type_id()
            .expect("a record, a class or an array has a type id");
        let (at_unit, at_file, span) = match &self.types.get(id).kind {
            TypeKind::Record { unit: Some(u), .. } | TypeKind::Class { unit: Some(u), .. } => (
                *u,
                self.inst
                    .unit_files
                    .get(*u as usize)
                    .copied()
                    .unwrap_or(file),
                self.types.get(id).span,
            ),
            _ => (unit, file, at),
        };
        let g = Gen { span };
        let (vars, param_count, ret, body) = match (op, less_than) {
            (KeyOp::Compare, Some((lt, operand))) => compare_body(&g, ty, lt, operand),
            (KeyOp::Equals, _) => self.equals_body(&g, ty),
            (KeyOp::Compare, None) => {
                unreachable!("the default order returns early without `LessThan`")
            }
            (KeyOp::Hash, _) => self.hash_body(&g, ty),
        };
        let params: Vec<ParamInfo> = vars
            .iter()
            .take(param_count as usize)
            .map(|v| ParamInfo {
                name: v.name.clone(),
                ty: v.ty,
                mode: ParamMode::Const,
                span,
                default: None,
                attrs: None,
            })
            .collect();
        let r = RoutineId(self.routines.len() as u32);
        let hir = g.routine(name.clone(), vars, param_count, ret, body, at_unit);
        self.routines.push(RoutineInfo {
            name: name.clone(),
            params,
            ret: Some(ret),
            has_body: true,
            forward_span: None,
            implicit_forward: true,
            name_span: span,
            hir: Some(hir),
            overload: false,
            overload_fix_at: None,
            unit: at_unit,
            file: at_file,
            interface: false,
            parent: None,
            calls: Vec::new(),
            method_of: None,
            self_kind: super::types::SelfKind::Instance,
            is_abstract: false,
            body_span: None,
            closure: false,
            type_params: Vec::new(),
            closure_info: super::ClosureInfo::default(),
        });
        self.inst.key_funcs.insert((op, ty), r);
        self.inst.generated_names.insert(r, name);
        self.inst.generated.push(GeneratedInfo { unit, routine: r });
        self.inst.order.push(r);
        self.inst.key_pending.push(r);
        Some(r)
    }

    /// The variables (`const A, B: T`, then `Result`, then the loop variable of an array) and
    /// the body of `<eq:T>` (`docs/spec/equality.md` 2.4, 2.5, 8.3): the fields in declaration
    /// order, or the elements in index order (a dynamic array's after its length), compared
    /// until one is not equal.
    fn equals_body(&self, g: &Gen, ty: Ty) -> (Vec<Var>, u32, Ty, Vec<Stmt>) {
        let mut vars = vec![
            g.decl("A", ty, VarKind::Param(ParamMode::Const)),
            g.decl("B", ty, VarKind::Param(ParamMode::Const)),
            g.decl("Result", Ty::Boolean, VarKind::Result),
        ];
        let body = match ty {
            Ty::Record(_) => {
                let fields: Vec<_> = self.types.fields(ty).to_vec();
                let mut keys = fields.iter().enumerate().map(|(i, f)| {
                    let strong = self.types.strong(f.ty);
                    g.key(
                        KeyOp::Equals,
                        vec![
                            g.field(g.var(0, ty), i as u32, &f.name, f.ty, strong),
                            g.field(g.var(1, ty), i as u32, &f.name, f.ty, strong),
                        ],
                    )
                });
                let value = match keys.next() {
                    Some(first) => g.and_all(first, keys.collect()),
                    None => g.boolean(true),
                };
                vec![g.assign(2, value)]
            }
            Ty::Array(_) => {
                let (index, low, high, elem) =
                    self.types.array(ty).expect("a static array's bounds");
                vars.push(g.loop_var(index));
                let elem_of = |a: u32| {
                    g.expr(
                        ExprKind::Elem {
                            base: Box::new(g.var(a, ty)),
                            index: Box::new(g.var(3, index)),
                        },
                        elem,
                    )
                };
                let differs = g.not(g.key(KeyOp::Equals, vec![elem_of(0), elem_of(1)]));
                vec![
                    g.assign(2, g.boolean(false)),
                    g.for_to(
                        3,
                        g.ordinal(low, index),
                        g.ordinal(high, index),
                        g.if_then(differs, g.exit()),
                    ),
                    g.assign(2, g.boolean(true)),
                ]
            }
            _ => {
                let elem = self
                    .types
                    .dyn_elem(ty)
                    .expect("a dynamic array's element type");
                vars.push(g.loop_var(Ty::Int64));
                let elem_of = |a: u32| {
                    g.expr(
                        ExprKind::Elem {
                            base: Box::new(g.var(a, ty)),
                            index: Box::new(g.var(3, Ty::Int64)),
                        },
                        elem,
                    )
                };
                let differs = g.not(g.key(KeyOp::Equals, vec![elem_of(0), elem_of(1)]));
                let same_length = g.binary(
                    g.length(g.var(0, ty)),
                    BinOp::Eq,
                    g.length(g.var(1, ty)),
                    Calc::Int(Ty::Int64),
                    Ty::Boolean,
                );
                let last = g.high(g.var(0, ty));
                vec![
                    g.assign(2, g.boolean(false)),
                    g.if_then(
                        same_length,
                        g.block(vec![
                            g.for_to(
                                3,
                                g.constant(super::types::Const::Int(0), Ty::Int64),
                                last,
                                g.if_then(differs, g.exit()),
                            ),
                            g.assign(2, g.boolean(true)),
                        ]),
                    ),
                ]
            }
        };
        (vars, 2, Ty::Boolean, body)
    }

    /// The variables (`const X: T`, `Result`, the loop variable of an array) and the body of
    /// `<hash:T>` (`docs/spec/equality.md` 8.3): `Result := KeyHash(N)` (`N` the number of
    /// fields or elements, a dynamic array's length), then `Result := HashCombine(Result,
    /// KeyHash(E))` for each field or element in order.
    fn hash_body(&self, g: &Gen, ty: Ty) -> (Vec<Var>, u32, Ty, Vec<Stmt>) {
        let mut vars = vec![
            g.decl("X", ty, VarKind::Param(ParamMode::Const)),
            g.decl("Result", Ty::Int64, VarKind::Result),
        ];
        let mix = |e: Expr| {
            g.assign(
                1,
                g.combine(g.var(1, Ty::Int64), g.key(KeyOp::Hash, vec![e])),
            )
        };
        let body = match ty {
            Ty::Record(_) => {
                let fields: Vec<_> = self.types.fields(ty).to_vec();
                let mut stmts = vec![g.assign(1, g.key(KeyOp::Hash, vec![g.int64(fields.len())]))];
                for (i, f) in fields.iter().enumerate() {
                    let strong = self.types.strong(f.ty);
                    stmts.push(mix(g.field(g.var(0, ty), i as u32, &f.name, f.ty, strong)));
                }
                stmts
            }
            Ty::Array(_) => {
                let (index, low, high, elem) =
                    self.types.array(ty).expect("a static array's bounds");
                vars.push(g.loop_var(index));
                let count = usize::try_from(high - low + 1).unwrap_or(0);
                let e = g.expr(
                    ExprKind::Elem {
                        base: Box::new(g.var(0, ty)),
                        index: Box::new(g.var(2, index)),
                    },
                    elem,
                );
                vec![
                    g.assign(1, g.key(KeyOp::Hash, vec![g.int64(count)])),
                    g.for_to(2, g.ordinal(low, index), g.ordinal(high, index), mix(e)),
                ]
            }
            _ => {
                let elem = self
                    .types
                    .dyn_elem(ty)
                    .expect("a dynamic array's element type");
                vars.push(g.loop_var(Ty::Int64));
                let e = g.expr(
                    ExprKind::Elem {
                        base: Box::new(g.var(0, ty)),
                        index: Box::new(g.var(2, Ty::Int64)),
                    },
                    elem,
                );
                let length = g.length(g.var(0, ty));
                let last = g.high(g.var(0, ty));
                vec![
                    g.assign(1, g.key(KeyOp::Hash, vec![length])),
                    g.for_to(
                        2,
                        g.constant(super::types::Const::Int(0), Ty::Int64),
                        last,
                        mix(e),
                    ),
                ]
            }
        };
        (vars, 1, Ty::Int64, body)
    }
}

/// The variables (`const A, B: T`, then `Result`) and the body of `<cmp:T>`
/// (`docs/spec/collections.md` 10.2): `if A < B then Result := -1 else if B < A then Result :=
/// 1 else Result := 0`, `<` the `LessThan` `lt` whose operands are of the type `operand` (`T`, or
/// the ancestor of a class declaring it).
fn compare_body(g: &Gen, ty: Ty, lt: RoutineId, operand: Ty) -> (Vec<Var>, u32, Ty, Vec<Stmt>) {
    use super::types::Const;
    let vars = vec![
        g.decl("A", ty, VarKind::Param(ParamMode::Const)),
        g.decl("B", ty, VarKind::Param(ParamMode::Const)),
        g.decl("Result", Ty::Integer, VarKind::Result),
    ];
    let less = |l: u32, r: u32| {
        g.expr(
            ExprKind::Call {
                routine: lt,
                args: vec![
                    CallArg::Value(g.var(l, operand)),
                    CallArg::Value(g.var(r, operand)),
                ],
                dispatch: Dispatch::Routine,
            },
            Ty::Boolean,
        )
    };
    let result = |n: i128| g.assign(2, g.constant(Const::Int(n), Ty::Integer));
    let body = vec![Stmt {
        kind: StmtKind::If {
            branches: vec![(less(0, 1), result(-1)), (less(1, 0), result(1))],
            else_body: Some(Box::new(result(0))),
        },
        span: g.span,
    }];
    (vars, 2, Ty::Integer, body)
}

impl Sema<'_, '_> {
    /// A call of `KeyEquals` / `KeyHash` (`docs/spec/equality.md` chapter 4) or `KeyCompare`
    /// (`docs/spec/collections.md` 10.2) written by the user: the type argument written (`targ`)
    /// or inferred from the arguments, which are taken as `const` parameters of that type. The
    /// expression is the one the generated bodies use (8.3); its routine is found once the type
    /// has no type parameter in it.
    pub(super) fn lower_key_call(
        &mut self,
        op: KeyOp,
        targ: Option<Ty>,
        args: &[crate::syntax::ast::Arg],
        span: TextRange,
    ) -> Expr {
        let (name, params, ty): (&str, &[&str], Ty) = match op {
            KeyOp::Equals => ("KeyEquals", &["A", "B"], Ty::Boolean),
            KeyOp::Hash => ("KeyHash", &["X"], Ty::Int64),
            KeyOp::Compare => ("KeyCompare", &["A", "B"], Ty::Integer),
        };
        self.check_no_width_pub(args);
        if args.len() != params.len() {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {}, found {}",
                    params.len(),
                    args.len()
                ),
                span,
            );
            return super::expr::err_expr(span);
        }
        let Some(t) = targ.or_else(|| self.infer_key_arg(name, args, span)) else {
            return super::expr::err_expr(span);
        };
        let checks = self.checks_at(span.start());
        let mut out = Vec::new();
        let mut failed = false;
        for (i, (a, p)) in args.iter().zip(params).enumerate() {
            let Some(v) = a.value() else {
                failed = true;
                continue;
            };
            let e = self.lower_expected(&v, t);
            if e.ty.is_error() {
                failed = true;
                continue;
            }
            let e = self.coerce(e, t, checks.range, Some((i + 1, p)));
            if e.ty.is_error() {
                failed = true;
            }
            out.push(e);
        }
        if failed || t.is_error() {
            return super::expr::err_expr(span);
        }
        // A type written out without a default order (`docs/spec/collections.md` 10.2); in a
        // generic body, the instantiation raises `EInvalidOperation` instead
        if op == KeyOp::Compare
            && !self.types.has_param(t)
            && self.types.order_kind(t) == super::types::OrderKind::None
        {
            let name = self.tn(t);
            let d = Diagnostic::error(
                Code::E(703),
                format!("`{name}` has no default order"),
                Stage::Sema,
            )
            .with_primary(self.span(span), "")
            .with_note(format!(
                "pass a comparer, as in `TComparer<{name}>.Construct(…)`"
            ));
            self.push(d);
            return super::expr::err_expr(span);
        }
        self.instances.key_used = true;
        Expr {
            kind: ExprKind::Key {
                op,
                args: out,
                func: None,
            },
            ty,
            value: None,
            span,
        }
    }
}
