//! Default array properties (`docs/spec/objects.md` chapter 12): `X[…]` on a value of a class,
//! record or interface declaring them, the choice among them by the types of the indices, a
//! range written as an index (a `TIndexRange`), and `TIndexRange.Clamp`, whose body is written
//! out where it is called (14.10).

use super::Sema;
use super::expr::err_expr;
use super::hir::*;
use super::types::{ClassProperty, TINDEXRANGE, Ty, TypeId, Visibility};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::TextRange;
use crate::syntax::ast::{self, AstNode};

/// The indices of a property access: as written (each typed against its parameter), or already
/// typed when they chose among default array properties.
pub(super) enum Indices {
    Ast(Vec<ast::Expr>),
    Lowered(Vec<Expr>),
}

impl Indices {
    pub(super) fn len(&self) -> usize {
        match self {
            Indices::Ast(v) => v.len(),
            Indices::Lowered(v) => v.len(),
        }
    }
}

/// The example of a default array property taking a range, for the notes.
const RANGE_EXAMPLE: &str =
    "property Items[const R: TIndexRange]: TArray<Integer> read GetRange; default;";

impl<'a, 's> Sema<'a, 's> {
    /// The type id of the static type of a value whose type may declare default array
    /// properties (a class, a record or an interface).
    pub(super) fn indexable_type(ty: Ty) -> Option<TypeId> {
        match ty {
            Ty::Class(id) | Ty::Record(id) | Ty::Intf(id) => Some(id),
            _ => None,
        }
    }

    /// The default array properties `X[…]` chooses among for a value of type `ty`
    /// (`docs/spec/objects.md` 12.2): those of the nearest type in the chain of ancestors that
    /// declares any (a class declaring them hides its parent's), with that type.
    pub(super) fn default_properties(&self, ty: Ty) -> Option<(TypeId, Vec<ClassProperty>)> {
        let id = Self::indexable_type(ty)?;
        let chain = if self.types.is_record(id) {
            vec![id]
        } else {
            self.types.class_chain(id)
        };
        chain.into_iter().find_map(|c| {
            let ps: Vec<ClassProperty> = self
                .types
                .class_properties(c)
                .iter()
                .filter(|p| p.is_default)
                .cloned()
                .collect();
            (!ps.is_empty()).then_some((c, ps))
        })
    }

    /// Whether `X[…]` on the expression `base` is a default array property access, by the type of
    /// `base` (typed without reporting anything).
    pub(super) fn indexes_default_property(&mut self, base: &ast::Expr) -> bool {
        let ty = self.dry_run(|s| s.lower_expr(base).ty);
        self.default_properties(ty).is_some()
    }

    /// `X[…]` read through the default array properties of the type of `recv`
    /// (`docs/spec/objects.md` 12.1).
    pub(super) fn default_index_read(
        &mut self,
        recv: Expr,
        items: &[ast::IndexItem],
        span: TextRange,
    ) -> Expr {
        let Some((owner, cands)) = self.default_properties(recv.ty) else {
            return err_expr(span);
        };
        let class = Self::indexable_type(recv.ty).unwrap_or(owner);
        let Some((p, indices)) = self.select_default(owner, cands, items, span) else {
            return err_expr(span);
        };
        self.property_read(recv, class, owner, &p, indices, span, span)
    }

    /// The default array property among `cands` (declared by `owner`) that the indices `items`
    /// select, with the indices to pass (`docs/spec/objects.md` 12.2, 12.3). `None` after a
    /// diagnostic.
    pub(super) fn select_default(
        &mut self,
        owner: TypeId,
        cands: Vec<ClassProperty>,
        items: &[ast::IndexItem],
        span: TextRange,
    ) -> Option<(ClassProperty, Indices)> {
        let visible: Vec<ClassProperty> = cands
            .iter()
            .filter(|p| self.can_see(p.vis, owner))
            .cloned()
            .collect();
        if visible.is_empty() {
            self.default_index_items(items);
            let p = &cands[0];
            let owner_name = self.tn(Ty::Class(owner));
            let msg = match p.vis {
                Visibility::StrictPrivate | Visibility::Private => {
                    format!("`{}` is private to `{owner_name}`", p.name)
                }
                _ => format!("`{}` is protected in `{owner_name}`", p.name),
            };
            let mut d =
                Diagnostic::error(Code::E(743), msg, Stage::Sema).with_primary(self.span(span), "");
            if let Some(s) = self.property_decl_span(owner, p) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            return None;
        }
        let has_range = items.iter().any(|i| matches!(i, ast::IndexItem::Slice(_)));
        if let [only] = visible.as_slice()
            && !has_range
        {
            // One property: its indices are typed against its parameters (E0705, E0702)
            let exprs = items
                .iter()
                .filter_map(|i| match i {
                    ast::IndexItem::Expr(e) => Some(e.clone()),
                    ast::IndexItem::Slice(_) => None,
                })
                .collect();
            return Some((only.clone(), Indices::Ast(exprs)));
        }
        let xs = self.default_index_items(items)?;
        let p = self.pick_default(owner, &visible, &xs, has_range, span)?;
        Some((p, Indices::Lowered(xs)))
    }

    /// The indices of `X[…]` typed on their own: a range becomes a new `TIndexRange`
    /// (`docs/spec/objects.md` 12.3). `None` when one has an error.
    fn default_index_items(&mut self, items: &[ast::IndexItem]) -> Option<Vec<Expr>> {
        let mut out = Vec::with_capacity(items.len());
        let mut failed = false;
        for item in items {
            match item {
                ast::IndexItem::Expr(e) => {
                    let x = self.lower_expr(e);
                    failed |= x.ty.is_error();
                    out.push(x);
                }
                ast::IndexItem::Slice(sl) => match self.range_index(sl) {
                    Some(x) => out.push(x),
                    None => failed = true,
                },
            }
        }
        (!failed).then_some(out)
    }

    /// `L..H` as an index of a default array property: a new `TIndexRange` whose bounds are
    /// integers converted to `Integer` (E0719). `None` after a diagnostic.
    fn range_index(&mut self, sl: &ast::Slice) -> Option<Expr> {
        let span = ast::trimmed_range(sl.syntax());
        let checks = self.checks_at(span.start());
        let (lo, hi) = sl.bounds();
        let mut failed = false;
        let mut bound = |this: &mut Self, e: Option<ast::Expr>| -> Option<Box<Expr>> {
            let x = this.lower_expr(&e?);
            if x.ty.is_error() {
                failed = true;
                return None;
            }
            if !x.ty.is_integer() {
                this.error(Code::E(719), "slice bounds must be integers", x.span);
                failed = true;
                return None;
            }
            let x = this.coerce(x, Ty::Integer, checks.range, None);
            failed |= x.ty.is_error();
            Some(Box::new(x))
        };
        let first = bound(self, lo);
        let last = bound(self, hi);
        if failed {
            return None;
        }
        Some(Expr {
            kind: ExprKind::IndexRange { first, last },
            ty: Ty::Record(TINDEXRANGE),
            value: None,
            span,
        })
    }

    /// The property among `cands` whose index types take `xs` best, by the stages of
    /// `docs/spec/semantics.md` 4.7 (E0735 when none does, E0734 when no one is best).
    fn pick_default(
        &mut self,
        owner: TypeId,
        cands: &[ClassProperty],
        xs: &[Expr],
        has_range: bool,
        span: TextRange,
    ) -> Option<ClassProperty> {
        let mut viable: Vec<(usize, Vec<u8>)> = Vec::new();
        for (k, p) in cands.iter().enumerate() {
            if p.params.len() != xs.len() {
                continue;
            }
            let stages: Option<Vec<u8>> = xs
                .iter()
                .zip(&p.params)
                .map(|(x, t)| self.operand_stage(x, *t))
                .collect();
            if let Some(s) = stages {
                viable.push((k, s));
            }
        }
        let name = cands[0].name.clone();
        if viable.is_empty() {
            let types: Vec<String> = xs.iter().map(|x| format!("`{}`", self.tn(x.ty))).collect();
            let mut d = Diagnostic::error(
                Code::E(735),
                format!("no overload of `{name}` matches these indices"),
                Stage::Sema,
            )
            .with_primary(self.span(span), "");
            for p in cands {
                if let Some(s) = self.property_decl_span(owner, p) {
                    d = d.with_secondary(s, "candidate");
                }
            }
            d = d.with_note(format!("the indices have types {}", types.join(", ")));
            let range = Ty::Record(TINDEXRANGE);
            if has_range && !cands.iter().any(|p| p.params.contains(&range)) {
                d = d.with_note(format!(
                    "a range is passed as a `TIndexRange`; declare a default property taking one, as in `{RANGE_EXAMPLE}`"
                ));
            }
            self.push(d);
            return None;
        }
        let dominates = |a: &[u8], b: &[u8]| {
            a.iter().zip(b).all(|(x, y)| x <= y) && a.iter().zip(b).any(|(x, y)| x < y)
        };
        let best: Vec<usize> = viable
            .iter()
            .filter(|(_, s)| !viable.iter().any(|(_, o)| dominates(o, s)))
            .map(|(k, _)| *k)
            .collect();
        if let [only] = best.as_slice() {
            return Some(cands[*only].clone());
        }
        let mut d = Diagnostic::error(
            Code::E(734),
            format!("ambiguous call to overloaded `{name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(span), "");
        for k in best {
            if let Some(s) = self.property_decl_span(owner, &cands[k]) {
                d = d.with_secondary(s, "candidate");
            }
        }
        self.push(d);
        None
    }

    /// The other default array properties named like `p` of `owner`, with `p`, for `X.Cells[…]`
    /// (`docs/spec/objects.md` 12.2). Just `p` when it is not a default array property.
    pub(super) fn same_named_defaults(
        &self,
        owner: TypeId,
        p: &ClassProperty,
    ) -> Vec<ClassProperty> {
        if !p.is_default {
            return vec![p.clone()];
        }
        self.types
            .class_properties(owner)
            .iter()
            .filter(|q| q.is_default && q.name.eq_ignore_ascii_case(&p.name))
            .cloned()
            .collect()
    }

    /// `R.Clamp(ALow, AHigh, AFirst, ALast)` of a `TIndexRange` (`docs/spec/objects.md` 12.3,
    /// 14.10), written out as integer comparisons on copies:
    ///
    /// ```text
    /// r := R; lo := ALow; hi := AHigh;
    /// f := lo; if r.HasFirst then if r.First > lo then f := r.First;
    /// l := hi; if r.HasLast then if r.Last < hi then l := r.Last;
    /// if l < f then l := f - 1;
    /// AFirst := f; ALast := l;
    /// ```
    ///
    /// The places of `AFirst` and `ALast` are fixed after `AHigh`, as a call fixes its `var` /
    /// `out` arguments before it runs (`fix_place`), so writing `AFirst` does not move `ALast`.
    pub(super) fn index_range_clamp(
        &mut self,
        recv: Expr,
        args: &[ast::Arg],
        span: TextRange,
    ) -> Option<StmtKind> {
        let values: Vec<Option<ast::Expr>> = args.iter().map(|a| a.value()).collect();
        if values.len() != 4 {
            for v in values.iter().flatten() {
                self.lower_expr(v);
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 4, found {}",
                    values.len()
                ),
                span,
            );
            return None;
        }
        let checks = self.checks_at(span.start());
        let names = ["ALow", "AHigh", "AFirst", "ALast"];
        let mut bounds = Vec::new();
        let mut places = Vec::new();
        let mut failed = false;
        for (k, v) in values.iter().enumerate() {
            let Some(v) = v else {
                failed = true;
                continue;
            };
            if k < 2 {
                let x = self.lower_expected(v, Ty::Integer);
                let x = if x.ty.is_error() {
                    x
                } else {
                    self.coerce(x, Ty::Integer, checks.range, None)
                };
                failed |= x.ty.is_error();
                bounds.push(x);
            } else {
                match self.ref_arg(v, (k + 1, names[k]), Ty::Integer, true, None) {
                    Some(p) => places.push(p),
                    None => failed = true,
                }
            }
        }
        if failed || recv.ty.is_error() {
            return None;
        }
        let rty = Ty::Record(TINDEXRANGE);
        let r = self.add_var(String::new(), rty, VarKind::Hidden, span);
        let lo = self.add_var(String::new(), Ty::Integer, VarKind::Hidden, span);
        let hi = self.add_var(String::new(), Ty::Integer, VarKind::Hidden, span);
        let f = self.add_var(String::new(), Ty::Integer, VarKind::Hidden, span);
        let l = self.add_var(String::new(), Ty::Integer, VarKind::Hidden, span);
        let var = |v: VarId, ty: Ty| Expr {
            kind: ExprKind::Var(v),
            ty,
            value: None,
            span,
        };
        let field = |slot: u32, name: &str, ty: Ty| Expr {
            kind: ExprKind::Field {
                base: Box::new(var(r, rty)),
                index: slot,
                name: name.to_string(),
            },
            ty,
            value: None,
            span,
        };
        let int = |v: VarId| var(v, Ty::Integer);
        let binary = |lhs: Expr, op: BinOp, rhs: Expr, ty: Ty| Expr {
            kind: ExprKind::Binary {
                first: Box::new(lhs),
                steps: vec![BinStep {
                    op,
                    rhs,
                    calc: Calc::Int(Ty::Integer),
                    ty,
                    span,
                }],
            },
            ty,
            value: None,
            span,
        };
        let assign = |target: Place, value: Expr| Stmt {
            kind: StmtKind::Assign {
                target,
                value,
                check: Checks {
                    overflow: false,
                    range: false,
                },
                is_init: false,
            },
            span,
        };
        let to = |v: VarId| Place::Var(v, span);
        let when = |cond: Expr, body: Stmt| Stmt {
            kind: StmtKind::If {
                branches: vec![(cond, body)],
                else_body: None,
            },
            span,
        };
        let mut bounds = bounds.into_iter();
        let (Some(low), Some(high)) = (bounds.next(), bounds.next()) else {
            return None;
        };
        let mut places = places.into_iter();
        let (Some(first_place), Some(last_place)) = (places.next(), places.next()) else {
            return None;
        };
        let one = Expr {
            kind: ExprKind::Const,
            ty: Ty::Integer,
            value: Some(super::types::Const::Int(1)),
            span,
        };
        let mut stmts = vec![
            assign(to(r), recv),
            assign(to(lo), low),
            assign(to(hi), high),
        ];
        let first_place = self.fix_place(first_place, &mut stmts, span);
        let last_place = self.fix_place(last_place, &mut stmts, span);
        stmts.extend([
            assign(to(f), int(lo)),
            when(
                field(2, "HasFirst", Ty::Boolean),
                when(
                    binary(
                        field(0, "First", Ty::Integer),
                        BinOp::Gt,
                        int(lo),
                        Ty::Boolean,
                    ),
                    assign(to(f), field(0, "First", Ty::Integer)),
                ),
            ),
            assign(to(l), int(hi)),
            when(
                field(3, "HasLast", Ty::Boolean),
                when(
                    binary(
                        field(1, "Last", Ty::Integer),
                        BinOp::Lt,
                        int(hi),
                        Ty::Boolean,
                    ),
                    assign(to(l), field(1, "Last", Ty::Integer)),
                ),
            ),
            when(
                binary(int(l), BinOp::Lt, int(f), Ty::Boolean),
                assign(to(l), binary(int(f), BinOp::Sub, one, Ty::Integer)),
            ),
            assign(first_place, int(f)),
            assign(last_place, int(l)),
        ]);
        Some(StmtKind::Block(stmts))
    }

    /// `place` with the values it reads (the indices that are not constants and the instances of
    /// fields of classes) copied first to hidden variables by assignments pushed to `stmts`, outer
    /// first, so that the place is the same whenever it is written later.
    fn fix_place(&mut self, place: Place, stmts: &mut Vec<Stmt>, span: TextRange) -> Place {
        let copy = |this: &mut Self, e: Expr, stmts: &mut Vec<Stmt>| -> Box<Expr> {
            if e.value.is_some() || matches!(e.kind, ExprKind::Var(_)) {
                return Box::new(e);
            }
            let ty = e.ty;
            let v = this.add_var(String::new(), ty, VarKind::Hidden, span);
            stmts.push(Stmt {
                kind: StmtKind::Assign {
                    target: Place::Var(v, span),
                    value: e,
                    check: Checks {
                        overflow: false,
                        range: false,
                    },
                    is_init: false,
                },
                span,
            });
            Box::new(Expr {
                kind: ExprKind::Var(v),
                ty,
                value: None,
                span,
            })
        };
        match place {
            Place::Var(..) | Place::Global(_) => place,
            Place::Field {
                base,
                index,
                ty,
                name,
            } => Place::Field {
                base: Box::new(self.fix_place(*base, stmts, span)),
                index,
                ty,
                name,
            },
            Place::Elem {
                base,
                index,
                ty,
                span: at,
            } => {
                let base = Box::new(self.fix_place(*base, stmts, span));
                Place::Elem {
                    base,
                    index: copy(self, *index, stmts),
                    ty,
                    span: at,
                }
            }
            Place::StrIndex {
                base,
                index,
                span: at,
            } => {
                let base = Box::new(self.fix_place(*base, stmts, span));
                Place::StrIndex {
                    base,
                    index: copy(self, *index, stmts),
                    span: at,
                }
            }
            Place::ObjField {
                base,
                slot,
                ty,
                name,
            } => Place::ObjField {
                base: copy(self, *base, stmts),
                slot,
                ty,
                name,
            },
        }
    }
}
