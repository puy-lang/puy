//! Local records expanded into one register per field (`docs/design/records.md` 3.9 B).
//!
//! A local record that needs no block of its own gets a hidden local variable for each slot of
//! its flat layout (a field, or a field of a record field laid out flat), and its field places
//! and reads become those variables before the routine is lowered: every rule of a local variable
//! (counting, `var` arguments, appending in place) then holds for them as it is. The record's
//! own variable is left only in the assignments of the whole value, which `lower` turns into
//! one copy per field: from or to a flat place (a field or an element in a container), another
//! expanded record, or `Default`.
//!
//! A record is expanded when it is a local whose type is laid out flat with no static array in
//! it (a static array's elements are reached by a computed slot number), no `[weak]` field and
//! no value that may hold an instance (letting go of fields one by one, among the other
//! registers, could run destructors in another order than letting go of the record's block),
//! no nested routine or anonymous method uses it, it is no `for..in` loop variable, and each of
//! its uses is a field (read, written, passed to a parameter, …) or one of those assignments.
//! Passing it whole, returning it or calling its methods would need a block made for the
//! purpose, so a record used that way keeps its block.

use crate::sema::hir::*;
use crate::sema::hirwalk::{HirVisit, walk_routine};
use crate::sema::types::{Ty, TypeTable};

/// A routine whose local records are expanded: its HIR with the hidden variables of the fields
/// added at the end of its variables, and for each variable, the first of its field variables
/// when it is expanded.
pub(super) struct Expanded {
    pub routine: Routine,
    pub first: Vec<Option<u32>>,
}

/// Expands the local records of `r` that can be ([`Expanded`]), or `None` when there is none. A
/// routine with variables in cells is left as it is: the registers of the cells follow the named
/// variables, and a nested routine finds them by the number of its enclosing routine's variables.
pub(super) fn expand(r: &Routine, types: &TypeTable, has_cells: bool) -> Option<Expanded> {
    if has_cells {
        return None;
    }
    let mut leaves: Vec<Option<Vec<(String, Ty)>>> =
        r.vars.iter().map(|v| candidate(v, types)).collect();
    if leaves.iter().all(Option::is_none) {
        return None;
    }
    let mut routine = r.clone();
    let mut count = Count {
        types,
        vars: &r.vars,
        cand: leaves.iter().map(Option::is_some).collect(),
        total: vec![0; r.vars.len()],
        fields: vec![0; r.vars.len()],
        assigns: Vec::new(),
    };
    walk_routine(&mut routine, &mut count);
    let Count {
        mut cand,
        total,
        fields,
        assigns,
        ..
    } = count;
    // Drop the records with a use that is not allowed until none is left: an assignment of a
    // whole record to or from another is allowed only while that one is expanded too
    loop {
        let mut allowed = fields.clone();
        for (target, value) in &assigns {
            let t = target.resolve(&cand);
            let v = value.resolve(&cand);
            if !matches!(t, Side::Expanded(_) | Side::Flat)
                || !matches!(v, Side::Expanded(_) | Side::Flat | Side::Default)
            {
                continue;
            }
            for s in [t, v] {
                if let Side::Expanded(x) = s {
                    allowed[x as usize] += 1;
                }
            }
        }
        let mut changed = false;
        for (i, c) in cand.iter_mut().enumerate() {
            if *c && allowed[i] != total[i] {
                *c = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if !cand.iter().any(|c| *c) {
        return None;
    }
    let mut first = vec![None; r.vars.len()];
    for (i, c) in cand.iter().enumerate() {
        let Some(fields) = leaves[i].take().filter(|_| *c) else {
            continue;
        };
        first[i] = Some(routine.vars.len() as u32);
        let var = r.vars[i].clone();
        for (name, ty) in fields {
            routine.vars.push(Var {
                name: format!("{}.{name}", var.name),
                ty,
                kind: VarKind::Local,
                span: var.span,
                decl: None,
                is_for_var: false,
                is_loop_var: false,
                captured: false,
                read_by_nested: false,
                written_by_nested: None,
                written: var.written,
                cell: false,
            });
        }
    }
    let mut rewrite = Rewrite {
        types,
        vars: &r.vars,
        first: &first,
    };
    walk_routine(&mut routine, &mut rewrite);
    Some(Expanded { routine, first })
}

/// The fields (names from the record, types) of a local that may be expanded, slot by slot of
/// the flat layout.
fn candidate(v: &Var, types: &TypeTable) -> Option<Vec<(String, Ty)>> {
    if v.kind != VarKind::Local
        || v.captured
        || v.cell
        || v.read_by_nested
        || v.written_by_nested.is_some()
        || !matches!(v.ty, Ty::Record(_))
        || types.flat_width(v.ty).is_none()
        || types.holds_object(v.ty)
    {
        return None;
    }
    let mut out = Vec::new();
    if !leaves(types, v.ty, "", &mut out) {
        return None;
    }
    debug_assert_eq!(Some(out.len() as u32), types.flat_width(v.ty));
    Some(out)
}

/// Adds the slots of the flat record `ty` to `out` (a field laid out flat is a record whose
/// slots follow in turn). False when one is a static array laid out flat, or `[weak]`.
fn leaves(types: &TypeTable, ty: Ty, prefix: &str, out: &mut Vec<(String, Ty)>) -> bool {
    for f in types.fields(ty) {
        let name = format!("{prefix}{}", f.name);
        match f.ty {
            Ty::Record(_) if types.flat_width(f.ty).is_some() => {
                if !leaves(types, f.ty, &format!("{name}."), out) {
                    return false;
                }
            }
            Ty::Weak(_) => return false,
            t if types.flat_width(t).is_some() => return false,
            t => out.push((name, t)),
        }
    }
    true
}

/// Whether `ty` is a record whose fields are slots of the flat layout of its expanded variable.
fn flat_record(types: &TypeTable, ty: Ty) -> bool {
    matches!(ty, Ty::Record(_)) && types.flat_width(ty).is_some()
}

/// A chain of field steps from a variable: the variable, the slot the chain reaches in its flat
/// layout, and whether every step stays inside it (each step is from a flat record).
struct Chain {
    var: u32,
    slot: u32,
    inside: bool,
    steps: usize,
}

/// Follows `steps` (field numbers, outermost last) from `var` of type `ty`.
fn chain(types: &TypeTable, var: u32, ty: Ty, steps: &[u32]) -> Chain {
    let mut slot = 0;
    let mut cur = ty;
    let mut inside = true;
    for &index in steps.iter().rev() {
        if !flat_record(types, cur) {
            inside = false;
            break;
        }
        slot += types.field_offset(cur, index);
        cur = types.fields(cur)[index as usize].ty;
    }
    Chain {
        var,
        slot,
        inside,
        steps: steps.len(),
    }
}

/// The chain of a place made of field steps from a variable.
fn place_chain(types: &TypeTable, vars: &[Var], p: &Place) -> Option<Chain> {
    let mut steps = Vec::new();
    let mut x = p;
    loop {
        match x {
            Place::Field { base, index, .. } => {
                steps.push(*index);
                x = base;
            }
            Place::Var(v, _) => {
                return Some(chain(types, v.0, vars.get(v.0 as usize)?.ty, &steps));
            }
            _ => return None,
        }
    }
}

/// The chain of an expression made of field reads from a variable.
fn expr_chain(types: &TypeTable, vars: &[Var], e: &Expr) -> Option<Chain> {
    let mut steps = Vec::new();
    let mut x = e;
    loop {
        if x.value.is_some() {
            return None;
        }
        match &x.kind {
            ExprKind::Field { base, index, .. } => {
                steps.push(*index);
                x = base;
            }
            ExprKind::Var(v) => {
                return Some(chain(types, v.0, vars.get(v.0 as usize)?.ty, &steps));
            }
            _ => return None,
        }
    }
}

/// One side of an assignment of a whole record.
#[derive(Clone, Copy)]
enum Side {
    /// A record variable that may be expanded, or a record field of it (`whole` is false) when
    /// each step stays inside its flat layout.
    Var {
        var: u32,
        whole: bool,
    },
    /// The variable or field of a record variable that is expanded.
    Expanded(u32),
    /// A flat place: a field or an element in a container.
    Flat,
    /// `Default(T)`.
    Default,
    Other,
}

impl Side {
    /// The side once the candidates are known: a variable's field stays a flat place (in the
    /// variable's block) when the variable is not expanded.
    fn resolve(self, cand: &[bool]) -> Side {
        match self {
            Side::Var { var, .. } if cand[var as usize] => Side::Expanded(var),
            Side::Var { whole: false, .. } => Side::Flat,
            Side::Var { whole: true, .. } => Side::Other,
            s => s,
        }
    }

    fn of_place(types: &TypeTable, vars: &[Var], p: &Place, ty: Ty) -> Side {
        if let Some(c) = place_chain(types, vars, p)
            && c.inside
        {
            return Side::Var {
                var: c.var,
                whole: c.steps == 0,
            };
        }
        match p {
            Place::Field { .. } | Place::Elem { .. } | Place::ObjField { .. }
                if flat_record(types, ty) =>
            {
                Side::Flat
            }
            _ => Side::Other,
        }
    }

    fn of_expr(types: &TypeTable, vars: &[Var], e: &Expr) -> Side {
        if e.value.is_none()
            && let Some(c) = expr_chain(types, vars, e)
            && c.inside
        {
            return Side::Var {
                var: c.var,
                whole: c.steps == 0,
            };
        }
        match &e.kind {
            ExprKind::Builtin {
                func: Builtin::Default,
                ..
            } => Side::Default,
            ExprKind::Field { .. } | ExprKind::Elem { .. } | ExprKind::ObjField { .. }
                if e.value.is_none() && flat_record(types, e.ty) =>
            {
                Side::Flat
            }
            _ => Side::Other,
        }
    }
}

/// Counts the uses of the candidates: every use (`total`), the uses as a field (`fields`), and
/// the assignments of a whole record.
struct Count<'t> {
    types: &'t TypeTable,
    vars: &'t [Var],
    cand: Vec<bool>,
    total: Vec<u32>,
    fields: Vec<u32>,
    assigns: Vec<(Side, Side)>,
}

impl Count<'_> {
    /// Counts a chain that reaches a field slot (not a record laid out flat) of a candidate.
    fn field(&mut self, c: Option<Chain>, ty: Ty) {
        if let Some(c) = c
            && self.cand[c.var as usize]
            && c.inside
            && c.steps > 0
            && !flat_record(self.types, ty)
        {
            self.fields[c.var as usize] += 1;
        }
    }
}

impl HirVisit for Count<'_> {
    fn ty(&mut self, _t: &mut Ty) {}

    fn stmt(&mut self, s: &mut Stmt) {
        match &s.kind {
            StmtKind::Assign { target, value, .. } if flat_record(self.types, value.ty) => {
                let t = Side::of_place(self.types, self.vars, target, value.ty);
                let v = Side::of_expr(self.types, self.vars, value);
                if matches!(t, Side::Var { .. }) || matches!(v, Side::Var { .. }) {
                    self.assigns.push((t, v));
                }
            }
            StmtKind::ForIn {
                var: ForVar::Local(v),
                ..
            }
            | StmtKind::For {
                var: ForVar::Local(v),
                ..
            } => {
                // Not a use the walk counts: never allowed
                self.total[v.0 as usize] += 1;
            }
            _ => {}
        }
    }

    fn place(&mut self, p: &mut Place) {
        if let Place::Var(v, _) = p {
            self.total[v.0 as usize] += 1;
        }
        if let Place::Field { ty, .. } = p {
            let ty = *ty;
            let c = place_chain(self.types, self.vars, p);
            self.field(c, ty);
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        if let ExprKind::Var(v) = e.kind {
            self.total[v.0 as usize] += 1;
        }
        if matches!(e.kind, ExprKind::Field { .. }) && e.value.is_none() {
            let c = expr_chain(self.types, self.vars, e);
            self.field(c, e.ty);
        }
    }
}

/// Replaces the field places and reads of the expanded records with their field variables.
struct Rewrite<'t> {
    types: &'t TypeTable,
    vars: &'t [Var],
    first: &'t [Option<u32>],
}

impl Rewrite<'_> {
    /// The field variable a chain reaches, if it is one of an expanded record.
    fn field_var(&self, c: Option<Chain>, ty: Ty) -> Option<VarId> {
        let c = c?;
        let first = self.first.get(c.var as usize).copied().flatten()?;
        (c.inside && c.steps > 0 && !flat_record(self.types, ty)).then(|| VarId(first + c.slot))
    }
}

impl HirVisit for Rewrite<'_> {
    fn ty(&mut self, _t: &mut Ty) {}

    fn place(&mut self, p: &mut Place) {
        let Place::Field { ty, .. } = p else {
            return;
        };
        let ty = *ty;
        let c = place_chain(self.types, self.vars, p);
        if let Some(v) = self.field_var(c, ty) {
            *p = Place::Var(v, root_span(p));
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        if !matches!(e.kind, ExprKind::Field { .. }) || e.value.is_some() {
            return;
        }
        let c = expr_chain(self.types, self.vars, e);
        if let Some(v) = self.field_var(c, e.ty) {
            e.kind = ExprKind::Var(v);
        }
    }
}

/// Where the variable at the root of a place was written.
fn root_span(p: &Place) -> crate::source::TextRange {
    match p.root() {
        Place::Var(_, span) => *span,
        _ => unreachable!("a chain of fields starts from a variable"),
    }
}
