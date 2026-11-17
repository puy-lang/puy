//! Operator overloading (`docs/spec/objects.md` chapter 11): the checks of `class operator`
//! declarations, the operators an expression's form looks for, and the choice among them.

use super::hir::*;
use super::types::{
    ClassMethod, MethodKind, SelfKind, TOBJECT, TREFOBJECT, Ty, TypeId, VMT_EQUALS, Visibility,
    VmtEntry,
};
use super::{ParamInfo, RoutineInfo, Sema};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::TextRange;
use crate::syntax::ast::ParamMode;

/// The note of E0700 for an operator named in a call (`docs/spec/objects.md` 11.1).
pub(super) const BY_NAME_NOTE: &str = "operators are used only through their expressions, as in `A + B`, and cannot be called by name";

/// The note of E0755 for an operator of a manually managed class (`docs/spec/objects.md` 11.1).
const MANUAL_CLASS_NOTE: &str = "the instances that operators return could not be freed; derive the class from `TRefObject` to use operators";

/// The note of E0765 for a record that declares its own `Equal` but not `GetHashCode`
/// (`docs/spec/equality.md` 3.6).
const RECORD_HASH_NOTE: &str = "declare `function GetHashCode: Int64;` that returns the same value for equal records, for example `HashCombine(KeyHash(A), KeyHash(B))`";

/// The note of E0765 for a reference counted class that declares its own `Equal` but does not
/// override `GetHashCode` (`docs/spec/equality.md` 3.6).
const CLASS_HASH_NOTE: &str = "override `GetHashCode` so that equal objects return the same value";

/// How many operands an operator takes and what it returns (`docs/spec/objects.md` 11.2).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Two operands; any result.
    Binary,
    /// Two operands; `Boolean`.
    Compare,
    /// One operand; any result.
    Unary,
    /// `Inc` / `Dec`: one operand; the declaring type.
    Step,
    /// `Implicit` / `Explicit`: one operand; the declaring type as the operand or the result.
    Conversion,
}

/// The operator names (`docs/spec/objects.md` 11.2).
const OPERATORS: &[(&str, Shape)] = &[
    ("Add", Shape::Binary),
    ("Subtract", Shape::Binary),
    ("Multiply", Shape::Binary),
    ("Divide", Shape::Binary),
    ("IntDivide", Shape::Binary),
    ("Modulus", Shape::Binary),
    ("LeftShift", Shape::Binary),
    ("RightShift", Shape::Binary),
    ("LogicalAnd", Shape::Binary),
    ("BitwiseAnd", Shape::Binary),
    ("LogicalOr", Shape::Binary),
    ("BitwiseOr", Shape::Binary),
    ("LogicalXor", Shape::Binary),
    ("BitwiseXor", Shape::Binary),
    ("Equal", Shape::Compare),
    ("NotEqual", Shape::Compare),
    ("LessThan", Shape::Compare),
    ("LessThanOrEqual", Shape::Compare),
    ("GreaterThan", Shape::Compare),
    ("GreaterThanOrEqual", Shape::Compare),
    ("In", Shape::Compare),
    ("Negative", Shape::Unary),
    ("Positive", Shape::Unary),
    ("LogicalNot", Shape::Unary),
    ("Inc", Shape::Step),
    ("Dec", Shape::Step),
    ("Trunc", Shape::Unary),
    ("Round", Shape::Unary),
    ("Implicit", Shape::Conversion),
    ("Explicit", Shape::Conversion),
];

/// The names of the custom managed records (E3014; `docs/spec/objects.md` 11.1).
const MANAGED_RECORD: &[&str] = &["Initialize", "Finalize", "Assign"];

fn shape_of(name: &str) -> Option<Shape> {
    OPERATORS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, s)| *s)
}

/// The name an operator is known by: `LogicalAnd` and `BitwiseAnd` (and `Or`, `Xor`) are the same
/// operator (`docs/spec/objects.md` 11.2).
fn group(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower
        .strip_prefix("logical")
        .or(lower.strip_prefix("bitwise"))
    {
        Some(rest @ ("and" | "or" | "xor")) => rest.to_string(),
        _ => lower,
    }
}

/// Whether two operator names name the same operator.
pub(super) fn same_operator(a: &str, b: &str) -> bool {
    group(a) == group(b)
}

/// Whether an operator is `Implicit` / `Explicit` (declared side by side by their results too).
pub(super) fn is_conversion(name: &str) -> bool {
    shape_of(name) == Some(Shape::Conversion)
}

/// Whether two operators' parameters are the same: value and `const` count as the same
/// (`docs/spec/semantics.md` 4.7).
pub(super) fn same_operands(a: &[(Ty, ParamMode)], b: &[(Ty, ParamMode)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|((x, _), (y, _))| x == y)
}

/// The operator names a binary expression looks for (`docs/spec/objects.md` 11.2).
pub(super) fn binary_names(op: BinOp) -> &'static [&'static str] {
    match op {
        BinOp::Add => &["Add"],
        BinOp::Sub => &["Subtract"],
        BinOp::Mul => &["Multiply"],
        BinOp::Slash => &["Divide"],
        BinOp::Div => &["IntDivide"],
        BinOp::Mod => &["Modulus"],
        BinOp::Shl => &["LeftShift"],
        BinOp::Shr => &["RightShift"],
        BinOp::And => &["LogicalAnd", "BitwiseAnd"],
        BinOp::Or => &["LogicalOr", "BitwiseOr"],
        BinOp::Xor => &["LogicalXor", "BitwiseXor"],
        BinOp::In => &["In"],
        BinOp::Eq => &["Equal"],
        BinOp::Ne => &["NotEqual"],
        BinOp::Lt => &["LessThan"],
        BinOp::Le => &["LessThanOrEqual"],
        BinOp::Gt => &["GreaterThan"],
        BinOp::Ge => &["GreaterThanOrEqual"],
    }
}

/// What converting a value with `Implicit` / `Explicit` finds (`docs/spec/objects.md` 11.4).
pub(super) enum ConvPick {
    None,
    One(RoutineId),
    Ambiguous(Vec<RoutineId>),
}

/// What choosing among the operators of an expression found (`docs/spec/objects.md` 11.3).
pub(super) enum OpPick {
    /// The operator to call.
    One(RoutineId),
    /// No operator of the name is declared by the operands' types: the built-in rules apply.
    NoCandidates,
    /// Operators are declared, but none takes the operands.
    NoMatch(Vec<RoutineId>),
    /// More than one takes them equally well.
    Ambiguous(Vec<RoutineId>),
}

impl<'a, 's> Sema<'a, 's> {
    /// The type whose operators a value of type `t` offers: a record, or a reference counted
    /// class (`docs/spec/objects.md` 11.3).
    pub(super) fn operator_owner(&self, t: Ty) -> Option<TypeId> {
        match t {
            Ty::Record(id) => Some(id),
            Ty::Class(id) if self.types.is_descendant(id, TREFOBJECT) => Some(id),
            _ => None,
        }
    }

    /// Whether `class` or an ancestor declares an operator named `name`.
    pub(super) fn has_operator_named(&self, class: TypeId, name: &str) -> bool {
        self.types.class_chain(class).into_iter().any(|c| {
            self.types
                .class_methods(c)
                .iter()
                .any(|m| m.kind == MethodKind::Operator && m.name.eq_ignore_ascii_case(name))
        })
    }

    /// The operators named one of `names` that the types `tys` (and the ancestors of a class,
    /// nearest first) declare, each once.
    fn operators_of(&self, tys: &[Ty], names: &[&str]) -> Vec<RoutineId> {
        let mut out = Vec::new();
        for t in tys {
            let Some(id) = self.operator_owner(*t) else {
                continue;
            };
            for c in self.types.class_chain(id) {
                for m in self.types.class_methods(c) {
                    if m.kind == MethodKind::Operator
                        && names.iter().any(|n| n.eq_ignore_ascii_case(&m.name))
                        && let Some(r) = m.routine
                        && !out.contains(&r)
                    {
                        out.push(r);
                    }
                }
            }
        }
        out
    }

    /// The checks of a record's `Equal` and `GetHashCode` (`docs/spec/equality.md` 3.2, the
    /// record's part of 3.5): a record declaring its own `Equal` must declare
    /// `function GetHashCode: Int64;`, and one declaring `GetHashCode` must declare its own
    /// `Equal` (E0765). `decls` maps each method's name to the range of its declaration, for the
    /// fix inserting `GetHashCode` after `Equal`. Only the first error is reported.
    pub(super) fn check_record_hash(
        &mut self,
        id: TypeId,
        methods: &[ClassMethod],
        decls: &[(TextRange, TextRange)],
    ) {
        let own = Ty::Record(id);
        // `Equal(const A, B: TFoo)`: both operands of the record's type (2.5). Operator operands
        // are value or `const` parameters (11.1)
        let equal = super::types::own_equal(methods, own);
        let hashes: Vec<&ClassMethod> = methods
            .iter()
            .filter(|m| {
                m.kind != MethodKind::Operator && m.name.eq_ignore_ascii_case("GetHashCode")
            })
            .collect();
        let name = self.tn(own);
        match (equal, hashes.first()) {
            (None, None) => {}
            (Some(eq), None) => {
                let mut d = Diagnostic::error(
                    Code::E(765),
                    format!("record `{name}` declares operator `Equal` but not `GetHashCode`"),
                    Stage::Sema,
                )
                .with_primary(self.span(eq.span), "")
                .with_note(RECORD_HASH_NOTE);
                if let Some(&(_, decl)) = decls.iter().find(|(n, _)| *n == eq.span) {
                    d = d.with_fix(Fix::suggestion(
                        "declare `function GetHashCode: Int64;`",
                        vec![self.insert_line_after(decl, "function GetHashCode: Int64;")],
                    ));
                }
                self.push(d);
            }
            (None, Some(h)) => {
                self.error(
                    Code::E(765),
                    format!("record `{name}` declares `GetHashCode` but not operator `Equal`"),
                    h.span,
                );
            }
            (Some(eq), Some(h)) => {
                let shaped = |m: &&ClassMethod| {
                    m.kind == MethodKind::Function
                        && m.self_kind == SelfKind::Instance
                        && m.params.is_empty()
                        && m.ret == Some(Ty::Int64)
                };
                // A result type that did not resolve was reported already (E0700): no E0765 on
                // top of it
                let errored = |m: &&ClassMethod| m.ret.is_some_and(|t| t.is_error());
                if !hashes.iter().any(shaped) && !hashes.iter().any(errored) {
                    let d = Diagnostic::error(
                        Code::E(765),
                        "`GetHashCode` of a record must be declared as `function GetHashCode: Int64`",
                        Stage::Sema,
                    )
                    .with_primary(self.span(h.span), "")
                    .with_secondary(self.span(eq.span), "operator `Equal` declared here");
                    self.push(d);
                }
            }
        }
    }

    /// The checks of a reference counted class's own `Equal` (`docs/spec/equality.md` 3.3, the
    /// class's part of 3.5) and the `Equals` override generated for it. A class declaring its own
    /// `Equal` (both operands of the class's type) must override `GetHashCode` and cannot override
    /// `Equals` (E0765); `Equals` is then a method of the class whose body `sema::finish` makes,
    /// at the entry of `Equals` in `vmt`. `eq_hash` is the class's `override` declarations of
    /// `Equals` and `GetHashCode` (as `equals_without_hash` takes them). Returns whether the class
    /// declares its own `Equal` (the check of overriding only one of the two does not apply).
    pub(super) fn check_class_equal(
        &mut self,
        id: TypeId,
        methods: &mut Vec<ClassMethod>,
        vmt: &mut [VmtEntry],
        eq_hash: [Option<(bool, TextRange, TextRange)>; 2],
        decls: &[(TextRange, TextRange)],
    ) -> bool {
        let own = Ty::Class(id);
        let Some(eq) = super::types::own_equal(methods, own) else {
            return false;
        };
        let (eq_span, eq_routine) = (eq.span, eq.routine);
        let name = self.tn(own);
        // 2. `Equal` with the user's `Equals` (3.5)
        if let [Some((true, equals, _)), _] = eq_hash {
            let d = Diagnostic::error(
                Code::E(765),
                format!(
                    "`{name}` declares operator `Equal`, so `Equals` is generated and cannot be overridden"
                ),
                Stage::Sema,
            )
            .with_primary(self.span(equals), "")
            .with_secondary(self.span(eq_span), "operator `Equal` declared here");
            self.push(d);
            return true;
        }
        // 3. `Equal` without `GetHashCode` (3.6). A declaration of `GetHashCode` that failed
        // (E0744) counts as written
        if eq_hash[1].is_none() {
            let mut d = Diagnostic::error(
                Code::E(765),
                format!(
                    "class `{name}` declares operator `Equal` but does not override `GetHashCode`"
                ),
                Stage::Sema,
            )
            .with_primary(self.span(eq_span), "")
            .with_note(CLASS_HASH_NOTE);
            if let Some(&(_, decl)) = decls.iter().find(|(n, _)| *n == eq_span) {
                d = d.with_fix(Fix::suggestion(
                    "override `GetHashCode`",
                    vec![self.insert_line_after(decl, "function GetHashCode: Int64; override;")],
                ));
            }
            self.push(d);
        }
        let Some(op) = eq_routine else {
            return true;
        };
        // The generated `Equals(Obj: TObject): Boolean`, overriding the entry of `TObject`
        let class_name = self.types.name(own);
        let routine = RoutineId(self.routines.len() as u32);
        self.routines.push(RoutineInfo {
            name: format!("{class_name}.Equals"),
            params: vec![ParamInfo {
                name: "Obj".into(),
                ty: Ty::Class(TOBJECT),
                mode: ParamMode::Value,
                span: eq_span,
                default: None,
                attrs: None,
            }],
            ret: Some(Ty::Boolean),
            has_body: false,
            forward_span: None,
            // Its body is made, not written: no E0723
            implicit_forward: true,
            name_span: eq_span,
            hir: None,
            overload: false,
            overload_fix_at: None,
            unit: self.unit_index,
            file: self.input.file,
            interface: self.in_interface_part,
            parent: None,
            calls: vec![op],
            method_of: Some(id),
            self_kind: SelfKind::Instance,
            is_abstract: false,
            body_span: None,
            closure: false,
            type_params: Vec::new(),
            closure_info: super::ClosureInfo::default(),
        });
        if let Some(e) = vmt.get_mut(VMT_EQUALS as usize) {
            *e = VmtEntry::Func(routine);
        }
        methods.push(ClassMethod {
            name: "Equals".into(),
            routine: Some(routine),
            span: eq_span,
            vis: Visibility::Public,
            kind: MethodKind::Function,
            params: vec![(Ty::Class(TOBJECT), ParamMode::Value)],
            ret: Some(Ty::Boolean),
            vmt: Some(VMT_EQUALS),
            is_override: true,
            is_abstract: false,
            is_final: false,
            self_kind: SelfKind::Instance,
        });
        self.instances.generated_equals.push((id, routine, op));
        true
    }

    /// Whether an operator may be declared in the type `id` (`docs/spec/objects.md` 11.1): a
    /// record or a reference counted class. E0755 otherwise (`at` is `class operator`).
    pub(super) fn operator_allowed(&mut self, id: TypeId, at: TextRange) -> bool {
        if self.types.is_record(id)
            || (!self.types.is_helper(id) && self.types.is_descendant(id, TREFOBJECT))
        {
            return true;
        }
        let manual = !self.types.is_helper(id) && !self.types.is_interface(id);
        let d = self.error(
            Code::E(755),
            "operators can only be declared in records and reference counted classes",
            at,
        );
        if manual {
            d.notes.push(MANUAL_CLASS_NOTE.into());
        }
        false
    }

    /// The checks of an operator's declaration in the type `id` (`docs/spec/objects.md` 11.1,
    /// 11.2, 11.4). Returns whether it is declared.
    pub(super) fn check_operator_decl(
        &mut self,
        id: TypeId,
        name: &str,
        name_range: TextRange,
        params: &[ParamInfo],
        ret: Option<Ty>,
    ) -> bool {
        let is_record = self.types.is_record(id);
        let Some(shape) = shape_of(name) else {
            if is_record && MANAGED_RECORD.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                self.error(
                    Code::E(3014),
                    format!("`class operator {name}` is not supported in Puy"),
                    name_range,
                )
                .notes
                .push(super::builtins::deleted_note(Code::E(3014)).into());
                return false;
            }
            let names: Vec<&str> = OPERATORS.iter().map(|(n, _)| *n).collect();
            let mut d = Diagnostic::error(
                Code::E(755),
                format!("`{name}` is not an operator name"),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "")
            .with_note(format!(
                "the operator names are {}",
                names
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if let Some(s) = crate::diagnostics::codes::similar_name(name, names.iter().copied()) {
                d = d.with_fix(Fix::suggestion(
                    format!("replace with `{s}`"),
                    vec![TextEdit::replace(
                        self.input.file,
                        name_range,
                        s.to_string(),
                    )],
                ));
            }
            self.push(d);
            return false;
        };
        let canonical = OPERATORS
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(n, _)| *n)
            .expect("the shape was found");
        let mut ok = true;
        for p in params {
            if matches!(p.mode, ParamMode::Var | ParamMode::Out) {
                self.error(
                    Code::E(755),
                    "operator parameters must be value or `const` parameters",
                    p.span,
                );
                ok = false;
            } else if p.default.is_some() {
                self.error(
                    Code::E(755),
                    "operator parameters cannot have default values",
                    p.span,
                );
                ok = false;
            }
        }
        if !ok {
            return false;
        }
        let want = match shape {
            Shape::Binary | Shape::Compare => 2,
            _ => 1,
        };
        if params.len() != want {
            let noun = if want == 1 { "parameter" } else { "parameters" };
            self.error(
                Code::E(755),
                format!("operator `{canonical}` must have {want} {noun}"),
                name_range,
            );
            return false;
        }
        let Some(ret) = ret else {
            // The heading has no result type (a syntax error was reported)
            return false;
        };
        if ret.is_error() || params.iter().any(|p| p.ty.is_error()) {
            return false;
        }
        let own = if is_record {
            Ty::Record(id)
        } else {
            Ty::Class(id)
        };
        let own_name = self.tn(own);
        if shape == Shape::Compare && ret != Ty::Boolean {
            self.error(
                Code::E(755),
                format!("operator `{canonical}` must return `Boolean`"),
                name_range,
            );
            return false;
        }
        if shape == Shape::Step && (ret != own || params[0].ty != own) {
            self.error(
                Code::E(755),
                format!("operator `{canonical}` must take and return `{own_name}`"),
                name_range,
            );
            return false;
        }
        let in_params = params.iter().any(|p| p.ty == own);
        if !(in_params || (shape == Shape::Conversion && ret == own)) {
            self.error(
                Code::E(755),
                format!(
                    "operator `{canonical}` must have a parameter or result of type `{own_name}`"
                ),
                name_range,
            );
            return false;
        }
        if shape == Shape::Conversion {
            let from = params[0].ty;
            let explicit = canonical == "Explicit";
            if from == ret || self.builtin_conversion(from, ret, explicit) {
                let (f, t) = (self.tn(from), self.tn(ret));
                self.error(
                    Code::E(755),
                    format!("`{canonical}` from `{f}` to `{t}` is already a built-in conversion"),
                    name_range,
                );
                return false;
            }
        }
        true
    }

    /// Whether a value of type `from` converts to `to` by the built-in rules: assignment
    /// compatibility, or for `explicit` also a conversion by a type's name
    /// (`docs/spec/semantics.md` 2.4, 2.5).
    fn builtin_conversion(&mut self, from: Ty, to: Ty, explicit: bool) -> bool {
        let probe = Expr {
            kind: ExprKind::Error,
            ty: from,
            value: None,
            span: TextRange::default(),
        };
        let assignable =
            self.dry_run(|s| !s.coerce_with(probe, to, false, None, false).ty.is_error());
        if assignable || !explicit {
            return assignable;
        }
        // The classes of one line convert by name (`docs/spec/objects.md` 2.9)
        match (from, to) {
            (Ty::Class(a), Ty::Class(b)) => {
                self.types.is_descendant(a, b) || self.types.is_descendant(b, a)
            }
            (Ty::Class(_) | Ty::Intf(_), Ty::Intf(_)) | (Ty::Intf(_), Ty::Class(_)) => true,
            _ => false,
        }
    }

    /// The virtual directives an operator cannot have (`docs/spec/objects.md` 11.1): E0744.
    pub(super) fn operator_directive(&mut self, at: TextRange) {
        let what = self.src(at).to_ascii_lowercase();
        self.error(Code::E(744), format!("an operator cannot be `{what}`"), at);
    }

    /// The stage of passing the operand `e` to a parameter of type `p` (`docs/spec/semantics.md`
    /// 4.7) on the scale of [`Self::conversion_stage`]: 0–4 by the built-in rules, 6 through
    /// `Implicit` (`docs/spec/objects.md` 11.4).
    pub(super) fn operand_stage(&mut self, e: &Expr, p: Ty) -> Option<u8> {
        if e.ty == p {
            return Some(0);
        }
        let probe = e.clone();
        let fits = self.dry_run(|s| !s.coerce_with(probe, p, false, None, false).ty.is_error());
        if fits {
            return Some(self.conversion_stage(e.ty, e.value.is_some(), p));
        }
        match self.conversion_for(e, p, &["Implicit"]) {
            ConvPick::One(_) => Some(6),
            _ => None,
        }
    }

    /// The operator among `cands` that takes `operands` (`docs/spec/objects.md` 11.3, by the
    /// stages of `docs/spec/semantics.md` 4.7).
    fn pick_operator(&mut self, cands: Vec<RoutineId>, operands: &[&Expr]) -> OpPick {
        if cands.is_empty() {
            return OpPick::NoCandidates;
        }
        let mut viable: Vec<(RoutineId, Vec<u8>)> = Vec::new();
        for &c in &cands {
            let params: Vec<Ty> = self.routines[c.0 as usize]
                .params
                .iter()
                .map(|p| p.ty)
                .collect();
            if params.len() != operands.len() {
                continue;
            }
            let stages: Option<Vec<u8>> = operands
                .iter()
                .zip(&params)
                .map(|(e, p)| self.operand_stage(e, *p))
                .collect();
            if let Some(s) = stages {
                viable.push((c, s));
            }
        }
        if viable.is_empty() {
            return OpPick::NoMatch(cands);
        }
        let dominates = |a: &[u8], b: &[u8]| {
            a.iter().zip(b).all(|(x, y)| x <= y) && a.iter().zip(b).any(|(x, y)| x < y)
        };
        let best: Vec<RoutineId> = viable
            .iter()
            .filter(|(_, s)| !viable.iter().any(|(_, o)| dominates(o, s)))
            .map(|(c, _)| *c)
            .collect();
        match best.as_slice() {
            [only] => OpPick::One(*only),
            _ => OpPick::Ambiguous(best),
        }
    }

    /// The call of the operator `id` on `operands`, each converted to its parameter's type.
    pub(super) fn operator_call(
        &mut self,
        id: RoutineId,
        operands: Vec<Expr>,
        span: TextRange,
    ) -> Expr {
        let params: Vec<Ty> = self.routines[id.0 as usize]
            .params
            .iter()
            .map(|p| p.ty)
            .collect();
        let ret = self.routines[id.0 as usize].ret.unwrap_or(Ty::Error);
        let checks = self.checks_at(span.start());
        let mut args = Vec::with_capacity(operands.len());
        for (e, p) in operands.into_iter().zip(params) {
            let e = self.coerce(e, p, checks.range, None);
            if e.ty.is_error() {
                return super::expr::err_expr(span);
            }
            args.push(CallArg::Value(e));
        }
        // For the static link (`docs/spec/semantics.md` 4.9)
        if self.dry_run == 0
            && let Some(c) = &mut self.cur
            && !c.calls.contains(&id)
        {
            c.calls.push(id);
        }
        Expr {
            kind: ExprKind::Call {
                routine: id,
                args,
                dispatch: Dispatch::Routine,
            },
            ty: ret,
            value: None,
            span,
        }
    }

    /// `A op B` when an operand's type may declare operators (`docs/spec/objects.md` 11.3): the
    /// call of the chosen operator, or an error expression after a diagnostic. `Err` gives the
    /// operands back when the built-in rules apply (no operator is declared, or a reference
    /// counted class's `=` / `<>` falls back to identity).
    pub(super) fn binary_operator(
        &mut self,
        lhs: Expr,
        op: BinOp,
        rhs: Expr,
        op_range: TextRange,
    ) -> Result<Expr, Box<(Expr, Expr)>> {
        let span = TextRange::new(lhs.span.start(), rhs.span.end());
        let is_nil = |e: &Expr| e.value == Some(super::types::Const::Nil);
        if matches!(op, BinOp::Eq | BinOp::Ne) && (is_nil(&lhs) || is_nil(&rhs)) {
            // `X = nil` compares the references (`docs/spec/objects.md` 11.3)
            return Err(Box::new((lhs, rhs)));
        }
        let tys = if op == BinOp::In {
            [rhs.ty, lhs.ty]
        } else {
            [lhs.ty, rhs.ty]
        };
        let cands = self.operators_of(&tys, binary_names(op));
        let identity = matches!(op, BinOp::Eq | BinOp::Ne)
            && matches!((lhs.ty, rhs.ty), (Ty::Class(_), Ty::Class(_)));
        // `<>` is not made from `=` (`docs/spec/objects.md` 11.3): E0703 when an `Equal` would take
        // the operands. One that would not leaves `=` to the built-in rules, and `<>` too (11.6)
        let equal_takes = cands.is_empty()
            && op == BinOp::Ne
            && !matches!(
                self.pick_operator(self.operators_of(&tys, &["Equal"]), &[&lhs, &rhs]),
                OpPick::NoCandidates | OpPick::NoMatch(_)
            );
        if equal_takes {
            let owner = tys
                .iter()
                .find(|t| !self.operators_of(&[**t], &["Equal"]).is_empty())
                .copied()
                .expect("one of them declares `Equal`");
            let (l, r, o) = (self.tn(lhs.ty), self.tn(rhs.ty), self.tn(owner));
            self.error(
                Code::E(703),
                format!("operator `<>` cannot be applied to `{l}` and `{r}`"),
                op_range,
            )
            .notes
            .push(format!(
                "`{o}` declares `Equal` but not `NotEqual`; declare `class operator NotEqual` too"
            ));
            return Ok(super::expr::err_expr(span));
        }
        match self.pick_operator(cands, &[&lhs, &rhs]) {
            OpPick::One(id) => Ok(self.operator_call(id, vec![lhs, rhs], span)),
            OpPick::NoCandidates => Err(Box::new((lhs, rhs))),
            OpPick::NoMatch(_) if identity => Err(Box::new((lhs, rhs))),
            OpPick::NoMatch(cands) => {
                let (l, r) = (self.tn(lhs.ty), self.tn(rhs.ty));
                let mut d = Diagnostic::error(
                    Code::E(735),
                    format!(
                        "no operator `{}` matches operands of type `{l}` and `{r}`",
                        op.text()
                    ),
                    Stage::Sema,
                )
                .with_primary(self.span(op_range), "");
                for c in cands {
                    d = self.with_routine_decl(d, c, "candidate");
                }
                self.push(d);
                Ok(super::expr::err_expr(span))
            }
            OpPick::Ambiguous(best) => {
                self.ambiguous_operator(op.text(), best, op_range);
                Ok(super::expr::err_expr(span))
            }
        }
    }

    /// The unary operator `text` (`-`, `+`, `not`, `Trunc`, `Round`) named `name` on an operand
    /// whose type may declare it (`docs/spec/objects.md` 11.2): `None` when it declares no
    /// operator of the name (the built-in rules apply).
    pub(super) fn unary_operator(
        &mut self,
        operand: Expr,
        name: &str,
        text: &str,
        op_range: TextRange,
        span: TextRange,
    ) -> Result<Expr, Expr> {
        let cands = self.operators_of(&[operand.ty], &[name]);
        match self.pick_operator(cands, &[&operand]) {
            OpPick::One(id) => Ok(self.operator_call(id, vec![operand], span)),
            OpPick::NoCandidates => Err(operand),
            OpPick::NoMatch(cands) => {
                let t = self.tn(operand.ty);
                let mut d = Diagnostic::error(
                    Code::E(735),
                    format!("no operator `{text}` matches an operand of type `{t}`"),
                    Stage::Sema,
                )
                .with_primary(self.span(op_range), "");
                for c in cands {
                    d = self.with_routine_decl(d, c, "candidate");
                }
                self.push(d);
                Ok(super::expr::err_expr(span))
            }
            OpPick::Ambiguous(best) => {
                self.ambiguous_operator(text, best, op_range);
                Ok(super::expr::err_expr(span))
            }
        }
    }

    /// E0734 for an operator that more than one declaration takes equally well.
    fn ambiguous_operator(&mut self, text: &str, best: Vec<RoutineId>, at: TextRange) {
        let mut d = Diagnostic::error(
            Code::E(734),
            format!("ambiguous call to overloaded operator `{text}`"),
            Stage::Sema,
        )
        .with_primary(self.span(at), "");
        for c in best {
            d = self.with_routine_decl(d, c, "candidate");
        }
        self.push(d);
    }

    /// The `Implicit` / `Explicit` operator (`names`) converting the value `e` to `target`
    /// (`docs/spec/objects.md` 11.4): one taking the value's type, or an integer constant's
    /// value through the built-in conversion to an integer parameter.
    pub(super) fn conversion_for(&self, e: &Expr, target: Ty, names: &[&str]) -> ConvPick {
        if e.ty.is_error() || target.is_error() {
            return ConvPick::None;
        }
        let cands = self.operators_of(&[e.ty, target], names);
        let takes = |c: &RoutineId, exact: bool| {
            let r = &self.routines[c.0 as usize];
            let Some(p) = r.params.first() else {
                return false;
            };
            if r.ret != Some(target) {
                return false;
            }
            if exact {
                return p.ty == e.ty;
            }
            match &e.value {
                Some(super::types::Const::Int(v)) => {
                    e.ty.is_integer() && p.ty.is_integer() && p.ty.contains(*v)
                }
                _ => false,
            }
        };
        let mut found: Vec<RoutineId> = cands.iter().copied().filter(|c| takes(c, true)).collect();
        if found.is_empty() {
            found = cands.iter().copied().filter(|c| takes(c, false)).collect();
        }
        match found.as_slice() {
            [] => ConvPick::None,
            [one] => ConvPick::One(*one),
            _ => ConvPick::Ambiguous(found),
        }
    }

    /// `e` converted to `target` by `Implicit` / `Explicit` (`names`; `docs/spec/objects.md`
    /// 11.4): `Ok(None)` when no operator converts it, `Err` after E0734.
    pub(super) fn convert_by_operator(
        &mut self,
        e: &Expr,
        target: Ty,
        names: &[&str],
    ) -> Result<Option<Expr>, ()> {
        match self.conversion_for(e, target, names) {
            ConvPick::None => Ok(None),
            ConvPick::One(id) => Ok(Some(self.operator_call(id, vec![e.clone()], e.span))),
            ConvPick::Ambiguous(best) => {
                self.ambiguous_operator(names[0], best, e.span);
                Err(())
            }
        }
    }
}
