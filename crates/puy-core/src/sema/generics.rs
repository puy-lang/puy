//! Generic declarations (`docs/spec/generics.md`): the generic types (classes and records), their
//! type parameters and constraints, the implementation headings of their methods, and the
//! operations a generic body cannot do with a value of a type parameter (E0762). The bodies are
//! checked once with the type parameters (`Ty::Param`) and have no function (7).

use super::Sema;
use super::types::{Constraint, Ty, TypeDef, TypeId, TypeKind, Visibility};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{Span, TextRange};
use crate::syntax::SyntaxKind;
use crate::syntax::ast::{self, AstNode};

/// A generic type declared in the file (`docs/spec/generics.md` 2.3): its names are found by the
/// name and the number of type parameters, apart from the names without type parameters.
#[derive(Debug, Clone)]
pub(super) struct GenericTypeName {
    pub lower: String,
    pub arity: usize,
    pub id: TypeId,
    pub span: TextRange,
    /// Declared in the `interface` section of a unit (seen by the files using it).
    pub interface: bool,
}

/// A generic type of the `type` section being declared whose type parameters are not entered
/// yet: the type parameters of its declaration, and of its forward declaration.
pub(super) struct PendingParams {
    id: TypeId,
    params: ast::TypeParams,
    forward: Option<ast::TypeParams>,
}

/// An instantiation whose constraints are checked later: the generic definition, where its name
/// is written, and the type arguments with where they are written.
pub(super) type DeferredCheck = (TypeId, TextRange, Vec<(Ty, TextRange)>);

/// An owning container of the standard library (`docs/spec/collections.md` chapter 9).
struct OwningContainer {
    /// The full name of the generic definition.
    name: &'static str,
    /// The container without ownership that replaces it in the fix of E0760
    /// (`docs/spec/generics.md` 3.6).
    plain: &'static str,
    /// The procedure that takes an item off and frees it, whose use as a value gets a note on
    /// E0725 (9.1).
    take: Option<&'static str>,
}

const OWNING_CONTAINERS: [OwningContainer; 4] = [
    OwningContainer {
        name: "System.Generics.Collections.TObjectList",
        plain: "TList",
        take: None,
    },
    OwningContainer {
        name: "System.Generics.Collections.TObjectStack",
        plain: "TStack",
        take: Some("Pop"),
    },
    OwningContainer {
        name: "System.Generics.Collections.TObjectQueue",
        plain: "TQueue",
        take: Some("Dequeue"),
    },
    OwningContainer {
        name: "System.Generics.Collections.TObjectDictionary",
        plain: "TDictionary",
        take: None,
    },
];

/// The deferral of constraint checks that [`Sema::begin_deferred_checks`] replaced.
#[must_use]
pub(super) struct DeferredChecks(Option<Vec<DeferredCheck>>);

/// An operation on a value of a type parameter that its constraints may not allow
/// (`docs/spec/generics.md` 3.3, E0762).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParamOp<'n> {
    Write,
    Compare,
    AssignNil,
    Convert,
    /// `is`, `Supports`.
    TypeTest,
    Free,
    /// `T.Create` without the `constructor` constraint.
    Create,
    /// An operator other than `=` / `<>`.
    Operator(&'n str),
    /// `Ord`, a `case` selector.
    Ordinal,
    SizeOf,
    /// An element of a set constructor, `Include` / `Exclude`.
    SetElement,
    Member(&'n str),
}

/// The names of the type parameters of a `TYPE_PARAMS` node, in order.
fn param_names(tp: &ast::TypeParams) -> Vec<(String, TextRange)> {
    tp.groups()
        .flat_map(|g| g.names())
        .map(|t| (t.text().to_string(), t.text_range()))
        .collect()
}

/// The constraints of each type parameter as written (lowercase, without spaces), to compare a
/// declaration with its forward declaration (`docs/spec/generics.md` 2.1).
fn written_constraints(tp: &ast::TypeParams) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for g in tp.groups() {
        let cs: Vec<String> = g
            .constraints()
            .map(|c| {
                c.syntax()
                    .text()
                    .to_string()
                    .split_whitespace()
                    .collect::<String>()
                    .to_ascii_lowercase()
            })
            .collect();
        for _ in g.names() {
            out.push(cs.clone());
        }
    }
    out
}

impl<'a, 's> Sema<'a, 's> {
    /// A generic type of a `type` section (`docs/spec/generics.md` 1.1, 2.1): a class or a record
    /// is declared (its type parameters are entered with its members), any other kind is E0757
    /// (or E3200 for the kinds M4-16 implements). Returns the type to define.
    pub(super) fn declare_generic_type(
        &mut self,
        decl: &ast::TypeDecl,
        tp: &ast::TypeParams,
        section: &ast::TypeSection,
        name: &str,
        range: TextRange,
    ) -> Option<Ty> {
        let tp_range = tp.syntax().text_range();
        if self.cur.as_ref().is_some_and(|c| !c.is_main) {
            self.error(
                Code::E(757),
                "a generic type cannot be declared inside a routine",
                tp_range,
            );
            return None;
        }
        // A generic type nested in a class or a record: its type parameters follow the outer
        // type's (2.5)
        let outer = self.class_scope();
        let spec = decl.type_spec()?;
        // The form of the definition (2.1); its id is made below
        let form: fn(TypeId) -> Ty = match &spec {
            ast::TypeSpec::Class(c) if c.is_forward() => {
                self.generic_forward(section, name, range, true);
                return None;
            }
            ast::TypeSpec::Interface(i) if i.is_forward() => {
                self.generic_forward(section, name, range, false);
                return None;
            }
            ast::TypeSpec::Class(_) => Ty::Class,
            ast::TypeSpec::Record(_) => Ty::Record,
            ast::TypeSpec::Interface(_) => Ty::Intf,
            ast::TypeSpec::Proc(p) if !p.of_object() => Ty::Proc,
            ast::TypeSpec::Proc(_) | ast::TypeSpec::Reference(_) => Ty::Callable,
            ast::TypeSpec::Array(a) if a.has_brackets() => Ty::Array,
            ast::TypeSpec::Array(_) => Ty::DynArray,
            ast::TypeSpec::Generic(g)
                if g.name()
                    .is_some_and(|n| n.text().eq_ignore_ascii_case("tarray")) =>
            {
                Ty::DynArray
            }
            _ => {
                self.error(
                    Code::E(757),
                    "a generic type must be a class, record, interface, procedural type or array type",
                    tp_range,
                );
                return None;
            }
        };
        // `<T, T>` (2.2)
        let mut params: Vec<(String, TextRange)> = Vec::new();
        for (n, r) in param_names(tp) {
            if let Some((_, prev)) = params.iter().find(|(p, _)| p.eq_ignore_ascii_case(&n)) {
                let d = Diagnostic::error(
                    Code::E(701),
                    format!("`{n}` is already declared"),
                    Stage::Sema,
                )
                .with_primary(self.span(r), "declared again here")
                .with_secondary(self.span(*prev), "declared here");
                self.push(d);
                continue;
            }
            params.push((n, r));
        }
        // The same name with the same number of type parameters (2.3)
        let lower = name.to_ascii_lowercase();
        let arity = param_names(tp).len();
        let prev = match outer {
            Some(o) => self
                .types
                .nested_generics
                .iter()
                .find(|g| g.owner == o && g.lower == lower && g.arity == arity)
                .map(|g| g.span),
            None => self
                .generic_types
                .iter()
                .find(|g| g.lower == lower && g.arity == arity)
                .map(|g| g.span),
        };
        if let Some(prev) = prev {
            let d = Diagnostic::error(
                Code::E(701),
                format!("`{name}` is already declared"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "declared again here")
            .with_secondary(self.span(prev), "declared here");
            self.push(d);
            return None;
        }
        // The forward declaration (2.1): the same type parameters
        let forward = section.decls().find_map(|d| {
            let n = d.name()?;
            let fp = d.type_params()?;
            (n.text().eq_ignore_ascii_case(name)
                && n.text_range().start() < range.start()
                && match d.type_spec() {
                    Some(ast::TypeSpec::Class(c)) => c.is_forward(),
                    Some(ast::TypeSpec::Interface(i)) => i.is_forward(),
                    _ => false,
                })
            .then_some(fp)
        });
        if let Some(fp) = &forward {
            let declared = param_names(fp);
            let same = declared.len() == arity
                && declared
                    .iter()
                    .zip(param_names(tp))
                    .all(|((a, _), (b, _))| a.eq_ignore_ascii_case(&b));
            if !same {
                let list: Vec<&str> = declared.iter().map(|(n, _)| n.as_str()).collect();
                self.error(
                    Code::E(757),
                    format!(
                        "type parameters of `{name}` must be written as in its declaration: `<{}>`",
                        list.join(", ")
                    ),
                    tp_range,
                );
            }
        }
        let display = format!(
            "{name}<{}>",
            params
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let id = self.add_type(Some(display), range);
        let ty = form(id);
        self.types.generic_tys.insert(id, ty);
        let mut ids: Vec<TypeId> = outer
            .map(|o| self.types.type_params(o).to_vec())
            .unwrap_or_default();
        let first = ids.len();
        ids.extend(params.into_iter().enumerate().map(|(i, (n, r))| {
            self.types.add(TypeDef {
                name: Some(n),
                full_name: None,
                kind: TypeKind::Param {
                    unit: self.unit_index,
                    index: (first + i) as u32,
                    constraints: Vec::new(),
                },
                span: r,
            })
        }));
        self.types.generics.push((id, ids));
        match outer {
            Some(o) => {
                self.types.generic_outer.insert(id, o);
                self.types
                    .nested_generics
                    .push(super::types::NestedGeneric {
                        owner: o,
                        lower,
                        arity,
                        id,
                        span: range,
                    });
            }
            None => self.generic_types.push(GenericTypeName {
                lower,
                arity,
                id,
                span: range,
                interface: self.in_interface_part,
            }),
        }
        self.generic_params.push(PendingParams {
            id,
            params: tp.clone(),
            forward,
        });
        Some(ty)
    }

    /// A forward declaration of a generic class or interface (`docs/spec/generics.md` 2.1): its
    /// full declaration comes later in the section (E0723 otherwise).
    fn generic_forward(
        &mut self,
        section: &ast::TypeSection,
        name: &str,
        range: TextRange,
        class: bool,
    ) {
        let later = section.decls().any(|d| {
            d.type_params().is_some()
                && d.name().is_some_and(|n| {
                    n.text().eq_ignore_ascii_case(name) && n.text_range().start() > range.start()
                })
                && match d.type_spec() {
                    Some(ast::TypeSpec::Class(c)) => class && !c.is_forward(),
                    Some(ast::TypeSpec::Interface(i)) => !class && !i.is_forward(),
                    _ => false,
                }
        });
        if !later {
            let what = if class { "class" } else { "interface" };
            self.error(
                Code::E(723),
                format!("forward declaration of {what} `{name}` has no body"),
                range,
            );
        }
    }

    /// Enters the type parameters of the generic type whose body is being declared (the class
    /// scope of `id` is the innermost): their names are seen in the body and the methods
    /// (`docs/spec/generics.md` 2.2), and their constraints are resolved (3.1, 3.2).
    pub(super) fn enter_type_params(&mut self, id: TypeId) {
        let Some(i) = self.generic_params.iter().position(|p| p.id == id) else {
            return;
        };
        let pending = self.generic_params.remove(i);
        let params = self.types.own_type_params(id).to_vec();
        // A class or a record sees them as its members; the other kinds through `param_scopes`
        let own_scope = self.class_scopes.last().is_some_and(|c| c.class == id)
            && matches!(
                self.types.generic_tys.get(&id),
                Some(Ty::Class(_) | Ty::Record(_))
            );
        // Seen only by the type itself (not through its name, nor by descendants)
        let vis = self.class_scopes.last().map(|c| c.vis);
        if let Some(c) = self.class_scopes.last_mut() {
            c.vis = Visibility::StrictPrivate;
        }
        for &p in params.iter().filter(|_| own_scope) {
            let def = self.types.get(p);
            let name = def.name.clone().unwrap_or_default();
            let span = def.span;
            if self.class_duplicate(&name.to_ascii_lowercase(), span) {
                continue;
            }
            self.bind_nested_type(name, Ty::Param(p), span);
        }
        if let (Some(c), Some(v)) = (self.class_scopes.last_mut(), vis) {
            c.vis = v;
        }
        // The constraints of the declaration, or of the forward declaration when only it has them
        let own = written_constraints(&pending.params);
        let source = match &pending.forward {
            Some(f) => {
                let fw = written_constraints(f);
                let has = |w: &[Vec<String>]| w.iter().any(|c| !c.is_empty());
                if has(&own) && has(&fw) && own != fw {
                    let name = self.types.name(Ty::Class(id));
                    let name = name.split('<').next().unwrap_or(&name).to_string();
                    self.error(
                        Code::E(758),
                        format!("constraints of `{name}` differ from its forward declaration"),
                        pending.params.syntax().text_range(),
                    );
                }
                if !has(&own) && has(&fw) {
                    f.clone()
                } else {
                    pending.params.clone()
                }
            }
            None => pending.params.clone(),
        };
        // The parameters left out of a duplicate (`<T, T>`) were not made
        let names: Vec<String> = params
            .iter()
            .map(|p| {
                self.types
                    .get(*p)
                    .name
                    .clone()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            })
            .collect();
        let mut names = names.into_iter();
        let mut next = names.next();
        // A constraint can have the type parameters of the list as its type arguments
        // (`T: IComparable<T>`; 1.1)
        self.param_scopes.push(id);
        self.with_deferred_checks(|s| {
            let mut assigned: Vec<(TypeId, Vec<Constraint>)> = Vec::new();
            let mut k = 0;
            for g in source.groups() {
                let cs = s.resolve_constraints(&g);
                for n in g.names() {
                    if next.as_deref() == Some(&n.text().to_ascii_lowercase()) && k < params.len() {
                        assigned.push((params[k], cs.clone()));
                        k += 1;
                        next = names.next();
                    }
                }
            }
            for (p, cs) in assigned {
                s.set_constraints(p, cs);
            }
        });
        self.param_scopes.pop();
    }

    fn set_constraints(&mut self, p: TypeId, cs: Vec<Constraint>) {
        if let TypeKind::Param { constraints, .. } = &mut self.types.get_mut(p).kind {
            *constraints = cs;
        }
    }

    /// Runs `f`, which resolves constraints, checking the constraints of the instantiations
    /// written in them only after it (`docs/spec/generics.md` 1.1, 3.5): `IFoo<T>` in the
    /// constraints of `T` needs those constraints of `T`, which are set by `f`.
    fn with_deferred_checks(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.begin_deferred_checks();
        f(self);
        self.end_deferred_checks(saved);
    }

    /// Starts deferring the constraint checks of the instantiations written from here on (until
    /// [`Sema::end_deferred_checks`] with the returned value): in a `type` section, they are
    /// checked when its classes have their parents, interfaces and methods
    /// (`docs/spec/generics.md` 4.2).
    pub(super) fn begin_deferred_checks(&mut self) -> DeferredChecks {
        DeferredChecks(self.deferred_checks.replace(Vec::new()))
    }

    /// Checks the constraints deferred since [`Sema::begin_deferred_checks`], or passes them on
    /// to the enclosing deferral.
    pub(super) fn end_deferred_checks(&mut self, saved: DeferredChecks) {
        let checks = std::mem::replace(&mut self.deferred_checks, saved.0).unwrap_or_default();
        if let Some(outer) = &mut self.deferred_checks {
            outer.extend(checks);
            return;
        }
        for (def, name_range, args) in checks {
            self.check_constraints(def, name_range, &args);
        }
    }

    /// A type declared in the body of a generic type (`docs/spec/generics.md` 2.5): a generic
    /// definition with the outer type's type parameters, so that each instantiation of the outer
    /// type has its own. Enumerations, subranges, sets and dynamic arrays (which have no members
    /// and are compared by structure) stay one type.
    pub(super) fn nest_in_generic(&mut self, ty: Ty) {
        let Some(outer) = self.class_scope().filter(|o| self.types.is_generic(*o)) else {
            return;
        };
        let (Ty::Class(id)
        | Ty::Record(id)
        | Ty::Intf(id)
        | Ty::Proc(id)
        | Ty::Callable(id)
        | Ty::Array(id)) = ty
        else {
            return;
        };
        if self.types.is_generic(id) {
            return;
        }
        let params = self.types.type_params(outer).to_vec();
        self.types.generics.push((id, params));
        self.types.generic_tys.insert(id, ty);
        self.types.generic_outer.insert(id, outer);
    }

    /// Runs `f` with the type parameters of the generic definition `id` in scope (an interface,
    /// a procedural type or an array type, which have no members to hold them;
    /// `docs/spec/generics.md` 2.2), entering them first.
    pub(super) fn with_type_params<R>(&mut self, id: TypeId, f: impl FnOnce(&mut Self) -> R) -> R {
        if !self.types.is_generic(id) {
            return f(self);
        }
        self.enter_type_params(id);
        self.param_scopes.push(id);
        let r = f(self);
        self.param_scopes.pop();
        r
    }

    /// Runs `f` with the type parameters of the generic class `id` in scope, before its body
    /// declares them as its members (its parents in parentheses; `docs/spec/generics.md` 2.4).
    pub(super) fn with_param_scope<R>(&mut self, id: TypeId, f: impl FnOnce(&mut Self) -> R) -> R {
        if !self.types.is_generic(id) {
            return f(self);
        }
        self.param_scopes.push(id);
        let r = f(self);
        self.param_scopes.pop();
        r
    }

    /// The type parameter `lower` of the definitions in `param_scopes`, innermost first, then of
    /// the generic types whose body is being declared or whose method is being analyzed (a type
    /// nested in a generic type sees the outer type's; 2.5).
    pub(super) fn scoped_param(&self, lower: &str) -> Option<Ty> {
        let starts = self
            .param_scopes
            .iter()
            .rev()
            .copied()
            .chain(self.method_class())
            .chain(self.class_scopes.iter().rev().map(|c| c.class));
        starts.into_iter().find_map(|d| {
            self.types.type_params(d).iter().find_map(|p| {
                self.types
                    .get(*p)
                    .name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(lower))
                    .then_some(Ty::Param(*p))
            })
        })
    }

    /// Whether a constraint names the built-in `NotRefCounted` (`docs/spec/generics.md` 3.6): a
    /// declaration of the name hides it.
    fn is_not_ref_counted(&mut self, r: &ast::TypeRef) -> bool {
        let Some(q) = r.name() else { return false };
        q.text().eq_ignore_ascii_case("notrefcounted")
            && self.lookup_type_symbol("notrefcounted").is_none()
            && self.find_visible("notrefcounted").is_none()
    }

    /// E0708 for `NotRefCounted` written as a type or a value (`docs/spec/generics.md` 3.6).
    pub(super) fn not_ref_counted_misplaced(&mut self, range: TextRange) {
        self.error(
            Code::E(708),
            "`NotRefCounted` can only be used as a constraint",
            range,
        );
    }

    /// The constraints of a group of type parameters (`docs/spec/generics.md` 1.1, 3.1, 3.2).
    fn resolve_constraints(&mut self, g: &ast::TypeParamGroup) -> Vec<Constraint> {
        let mut out: Vec<(Constraint, TextRange, String)> = Vec::new();
        let mut last_end = None;
        for c in g.constraints() {
            let range = ast::trimmed_range(c.syntax());
            let text = self.src(range);
            let prev_end = last_end.replace(range.end());
            let k = if let Some(kw) = c.keyword() {
                match kw.kind() {
                    SyntaxKind::CLASS_KW => Constraint::Class,
                    SyntaxKind::RECORD_KW => Constraint::Record,
                    _ => Constraint::Constructor,
                }
            } else {
                let ty = if let Some(r) = c.type_ref() {
                    if self.is_not_ref_counted(&r) {
                        None
                    } else {
                        Some(self.resolve_type_ref(&r))
                    }
                } else if let Some(gr) = c.generic_ref() {
                    // A type with type arguments (`IComparable<T>`; 1.1)
                    Some(self.resolve_type(Some(ast::TypeSpec::Generic(gr))))
                } else {
                    continue;
                };
                match ty {
                    None => Constraint::NotRefCounted,
                    Some(t @ (Ty::Class(_) | Ty::Intf(_))) => Constraint::Type(t),
                    Some(Ty::Error) => continue,
                    Some(_) => {
                        self.error(
                            Code::E(758),
                            format!("`{text}` cannot be used as a constraint"),
                            range,
                        );
                        continue;
                    }
                }
            };
            if out.iter().any(|(o, _, _)| *o == k) {
                // From the end of the constraint before it to its end (with the `,`)
                let del = TextRange::new(prev_end.unwrap_or(range.start()), range.end());
                let d = Diagnostic::error(
                    Code::E(758),
                    format!("constraint `{text}` is listed more than once"),
                    Stage::Sema,
                )
                .with_primary(self.span(range), "")
                .with_fix(Fix::safe(
                    format!("remove the second `{text}`"),
                    vec![TextEdit::delete(self.input.file, del)],
                ));
                self.push(d);
                continue;
            }
            out.push((k, range, text));
        }
        // The combinations (3.2): the later one of a pair is reported
        let is_class_type = |c: &Constraint| matches!(c, Constraint::Type(Ty::Class(_)));
        let conflicts = |a: &Constraint, b: &Constraint| {
            let record_with = |x: &Constraint| {
                matches!(
                    x,
                    Constraint::Class | Constraint::Constructor | Constraint::Type(_)
                )
            };
            (*a == Constraint::Record && record_with(b))
                || (*b == Constraint::Record && record_with(a))
        };
        let mut reported = false;
        'pairs: for j in 1..out.len() {
            for i in 0..j {
                if conflicts(&out[i].0, &out[j].0) {
                    let (a, b) = (out[i].2.clone(), out[j].2.clone());
                    self.error(
                        Code::E(758),
                        format!("constraints `{a}` and `{b}` cannot be combined"),
                        out[j].1,
                    );
                    reported = true;
                    break 'pairs;
                }
            }
        }
        if let Some((_, r, _)) = out.iter().filter(|(c, _, _)| is_class_type(c)).nth(1) {
            let r = *r;
            self.error(
                Code::E(758),
                "a type parameter can have only one class type constraint",
                r,
            );
            reported = true;
        }
        if !reported
            && let Some((_, r, _)) = out.iter().find(|(c, _, _)| *c == Constraint::Constructor)
            && !out
                .iter()
                .any(|(c, _, _)| *c == Constraint::Class || is_class_type(c))
        {
            let r = *r;
            let d = Diagnostic::error(
                Code::E(758),
                "`constructor` needs a `class` or class type constraint",
                Stage::Sema,
            )
            .with_primary(self.span(r), "")
            .with_fix(Fix::safe(
                "add `class`",
                vec![TextEdit::insert(self.input.file, r.start(), "class, ")],
            ));
            self.push(d);
        }
        out.into_iter().map(|(c, _, _)| c).collect()
    }

    /// The declaration of a generic type as the `hir` dump shows it
    /// (`docs/spec/generics.md` 8): `TOwnedList<T: class, NotRefCounted>`.
    pub(super) fn generic_decl_text(&self, id: TypeId, name: &str) -> String {
        let params = self.types.own_type_params(id);
        if params.is_empty() {
            // A type nested in a generic type (2.5)
            return name.to_string();
        }
        let mut groups: Vec<(Vec<String>, Vec<Constraint>)> = Vec::new();
        for &p in params {
            let n = self.types.get(p).name.clone().unwrap_or_default();
            let cs = self.types.constraints(p).to_vec();
            match groups.last_mut() {
                Some((names, c)) if *c == cs => names.push(n),
                _ => groups.push((vec![n], cs)),
            }
        }
        let text: Vec<String> = groups
            .iter()
            .map(|(names, cs)| {
                let mut s = names.join(", ");
                if !cs.is_empty() {
                    let c: Vec<String> = cs.iter().map(|c| self.constraint_text(*c)).collect();
                    s.push_str(": ");
                    s.push_str(&c.join(", "));
                }
                s
            })
            .collect();
        format!("{name}<{}>", text.join("; "))
    }

    fn constraint_text(&self, c: Constraint) -> String {
        match c {
            Constraint::Class => "class".into(),
            Constraint::Record => "record".into(),
            Constraint::Constructor => "constructor".into(),
            Constraint::NotRefCounted => "NotRefCounted".into(),
            Constraint::Type(t) => self.tn(t),
        }
    }

    /// The type parameters of a routine heading outside a class (`docs/spec/generics.md` 1.2):
    /// a generic routine (`function Max<T>`), the body of a generic type's method
    /// (`procedure TStack<T>.Push`), the body of a generic method (`function TUtil.Max<T>`,
    /// `function TList<T>.Map<U>`). E0757 when they are not written as in the declaration or
    /// have constraints, or for a nested routine; E3200 for the forms M4-16 implements (a nested
    /// type).
    pub(super) fn routine_heading_params(&mut self, header: &ast::RoutineHeader) -> HeadingParams {
        let Some(qname) = header.name() else {
            return HeadingParams::None;
        };
        let idents: Vec<_> = qname.idents().collect();
        // The type parameters, with the number of names before them
        let tps: Vec<(usize, ast::TypeParams)> = {
            let mut before = 0;
            let mut out = Vec::new();
            for e in qname.syntax().children_with_tokens() {
                if e.as_token().is_some_and(|t| t.kind() == SyntaxKind::IDENT) {
                    before += 1;
                } else if let Some(tp) = e.into_node().and_then(ast::TypeParams::cast) {
                    out.push((before, tp));
                }
            }
            out
        };
        match (idents.len(), tps.as_slice()) {
            (1, []) => HeadingParams::None,
            (1, [(1, tp)]) => self.generic_routine_heading(header, tp, idents[0].text()),
            (2, _) if tps.iter().all(|(k, _)| *k == 1 || *k == 2) => {
                let class_tp = tps.iter().find(|(k, _)| *k == 1).map(|(_, tp)| tp);
                let method_tp = tps.iter().find(|(k, _)| *k == 2).map(|(_, tp)| tp);
                let class = match class_tp {
                    Some(tp) => match self.generic_type_heading(tp, &idents) {
                        Some(c) => Some(c),
                        None => return HeadingParams::Failed,
                    },
                    None => None,
                };
                let owner = class.or_else(|| {
                    let lower = idents[0].text().to_ascii_lowercase();
                    match self.dry_run(|s| s.lookup_type_symbol(&lower)) {
                        Some(Some(Ty::Class(c) | Ty::Record(c) | Ty::Helper(c))) => Some(c),
                        _ => None,
                    }
                });
                let Some(owner) = owner else {
                    // Not a class: reported by the heading's own checks
                    return HeadingParams::None;
                };
                let own = match self.generic_method_heading(header, owner, &idents, method_tp) {
                    Some(own) => own,
                    None => return HeadingParams::Failed,
                };
                if class.is_none() && own.is_empty() {
                    return HeadingParams::None;
                }
                HeadingParams::Generic {
                    class,
                    own,
                    fresh: false,
                }
            }
            (_, []) => HeadingParams::None,
            (n, _) if n >= 3 => self.nested_heading(header, &idents, &tps),
            (_, [(_, tp), ..]) => {
                self.generics_not_supported(tp.syntax().text_range());
                HeadingParams::Failed
            }
        }
    }

    /// The heading of a method's body of a type nested in a type (`procedure
    /// TList<T>.TEnumerator.MoveNext`, `function THolder.TBox<T>.Show`,
    /// `function TOuter.TInner.Map<U>`; `docs/spec/generics.md` 1.2, 2.5): each part before the
    /// method's name is found in the one before it, with the type parameters of a generic type
    /// written as declared.
    fn nested_heading(
        &mut self,
        header: &ast::RoutineHeader,
        idents: &[crate::syntax::SyntaxToken],
        tps: &[(usize, ast::TypeParams)],
    ) -> HeadingParams {
        let n = idents.len();
        let after = |k: usize| {
            tps.iter()
                .find(|(b, _)| *b == k + 1)
                .map(|(_, tp)| tp.clone())
        };
        let mut class: Option<TypeId> = None;
        // A part's type parameters not written as declared (E0757): the rest is still found, so
        // that the method's declaration counts as having a body (no E0723)
        let mut failed = false;
        for (k, ident) in idents[..n - 1].iter().enumerate() {
            let name = ident.text().to_string();
            let lower = name.to_ascii_lowercase();
            let next = match after(k) {
                Some(tp) => {
                    let written = param_names(&tp).len();
                    let candidates: Vec<TypeId> = match class {
                        None => self
                            .generic_types
                            .iter()
                            .filter(|g| g.lower == lower)
                            .map(|g| g.id)
                            .collect(),
                        Some(c) => self
                            .types
                            .nested_generics
                            .iter()
                            .filter(|g| g.owner == c && g.lower == lower)
                            .map(|g| g.id)
                            .collect(),
                    };
                    let Some(&found) = candidates
                        .iter()
                        .find(|c| self.types.own_type_params(**c).len() == written)
                        .or(candidates.first())
                    else {
                        // Not a generic type: reported by the heading's own checks
                        return if failed {
                            HeadingParams::Failed
                        } else {
                            HeadingParams::None
                        };
                    };
                    let declared = self.types.own_type_params(found).to_vec();
                    if !failed && !self.check_heading_params(&tp, &declared, &name) {
                        failed = true;
                    }
                    found
                }
                None => {
                    let found = match class {
                        None => match self.dry_run(|s| s.lookup_type_symbol(&lower)) {
                            Some(Some(Ty::Class(c) | Ty::Record(c) | Ty::Helper(c))) => Some(c),
                            _ => None,
                        },
                        Some(c) => match self.find_static(c, &name).map(|(_, s)| s.kind) {
                            Some(super::types::StaticKind::Type(
                                Ty::Class(t) | Ty::Record(t) | Ty::Helper(t),
                            )) => Some(t),
                            _ => None,
                        },
                    };
                    let Some(found) = found else {
                        return if failed {
                            HeadingParams::Failed
                        } else {
                            HeadingParams::None
                        };
                    };
                    found
                }
            };
            class = Some(next);
        }
        let Some(owner) = class else {
            return HeadingParams::None;
        };
        if failed {
            // The body is not checked; its declaration counts as having one (no E0723)
            self.mark_has_body(owner, idents[n - 1].text());
            return HeadingParams::Failed;
        }
        let method_tp = after(n - 1);
        let Some(own) =
            self.generic_method_heading(header, owner, &idents[n - 2..], method_tp.as_ref())
        else {
            return HeadingParams::Failed;
        };
        let class = self.types.is_generic(owner).then_some(owner);
        if class.is_none() && own.is_empty() {
            return HeadingParams::None;
        }
        HeadingParams::Generic {
            class,
            own,
            fresh: false,
        }
    }

    /// The heading of a generic routine (`function Max<T>`; `docs/spec/generics.md` 1.2, 2.1):
    /// E0757 inside a routine; the type parameters of its first declaration (a heading in the
    /// `interface` section or a `forward` one), or new ones.
    fn generic_routine_heading(
        &mut self,
        header: &ast::RoutineHeader,
        tp: &ast::TypeParams,
        name: &str,
    ) -> HeadingParams {
        if self.cur.as_ref().is_some_and(|c| !c.is_main) {
            self.error(
                Code::E(757),
                "a nested routine cannot have type parameters",
                tp.syntax().text_range(),
            );
            return HeadingParams::Failed;
        }
        let arity = param_names(tp).len();
        let lower = name.to_ascii_lowercase();
        let unit = self.unit_index;
        let parent = self.cur.as_ref().and_then(|c| c.routine);
        // The first declaration without a body this heading completes: of the same name and
        // number of type parameters, and the same parameters with the type parameters read by
        // their positions (an overload is found by its parameters, not by its order)
        let earlier: Vec<super::RoutineId> = (0..self.routines.len())
            .map(|i| super::RoutineId(i as u32))
            .filter(|r| {
                let info = &self.routines[r.0 as usize];
                info.unit == unit
                    && info.parent == parent
                    && info.method_of.is_none()
                    && !info.has_body
                    && info.type_params.len() == arity
                    && info.name.eq_ignore_ascii_case(&lower)
            })
            .collect();
        let found = earlier
            .into_iter()
            .find(|r| self.heading_matches(header, tp, *r, None));
        if let Some(r) = found {
            let declared = self.routines[r.0 as usize].type_params.clone();
            let shown = self.routines[r.0 as usize].name.clone();
            if !self.check_heading_params(tp, &declared, &shown) {
                // The body is not checked; its declaration counts as having one (no E0723)
                self.routines[r.0 as usize].has_body = true;
                return HeadingParams::Failed;
            }
            return HeadingParams::Generic {
                class: None,
                own: declared,
                fresh: false,
            };
        }
        let own = self.declare_routine_type_params(tp, None);
        HeadingParams::Generic {
            class: None,
            own,
            fresh: true,
        }
    }

    /// Whether a heading's parameters and result are those of the routine `r`, with the type
    /// parameters written in `tp` read as `r`'s by their positions (`docs/spec/generics.md` 1.2):
    /// the declaration a body completes. `class`: the class whose scope the heading of a method's
    /// body sees.
    fn heading_matches(
        &mut self,
        header: &ast::RoutineHeader,
        tp: &ast::TypeParams,
        r: super::RoutineId,
        class: Option<TypeId>,
    ) -> bool {
        let written = param_names(tp);
        let declared = self.routines[r.0 as usize].type_params.clone();
        if written.len() != declared.len() {
            return false;
        }
        let mark = self.routine_type_params.len();
        for ((n, _), p) in written.iter().zip(&declared) {
            self.routine_type_params.push((n.to_ascii_lowercase(), *p));
        }
        if let Some(c) = class {
            self.class_scopes.push(super::statics::ClassScope {
                class: c,
                vis: Visibility::Public,
                names: Vec::new(),
            });
        }
        let (params, ret) = self.dry_run(|s| s.heading_signature(header));
        if class.is_some() {
            self.class_scopes.pop();
        }
        self.pop_routine_type_params(mark);
        let info = &self.routines[r.0 as usize];
        info.ret == ret
            && info.params.len() == params.len()
            && info
                .params
                .iter()
                .zip(&params)
                .all(|(a, b)| a.ty == b.ty && a.mode == b.mode)
    }

    /// The generic type of a method's body (`procedure TStack<T>.Push`;
    /// `docs/spec/generics.md` 1.2): E0757 when its type parameters are not written as in the
    /// declaration (the body is then not checked) or have constraints.
    fn generic_type_heading(
        &mut self,
        tp: &ast::TypeParams,
        idents: &[crate::syntax::SyntaxToken],
    ) -> Option<TypeId> {
        let name = idents[0].text().to_string();
        let lower = name.to_ascii_lowercase();
        let written = param_names(tp);
        let candidates: Vec<TypeId> = self
            .generic_types
            .iter()
            .filter(|g| g.lower == lower)
            .map(|g| g.id)
            .collect();
        let Some(&class) = candidates
            .iter()
            .find(|c| self.types.own_type_params(**c).len() == written.len())
            .or(candidates.first())
        else {
            // A type that is not generic (E0761, as for a method: `procedure TFoo.Plain<T>;`), a
            // name that is not a type (E0708) or an undeclared name (E0700), as in a type position
            self.not_generic_name(&name, idents[0].text_range(), tp.syntax().text_range());
            return None;
        };
        let declared = self.types.own_type_params(class).to_vec();
        if !self.check_heading_params(tp, &declared, &name) {
            // The body is not checked; its declaration counts as having one (no E0723)
            if idents.len() == 2 {
                self.mark_has_body(class, idents[1].text());
            }
            return None;
        }
        Some(class)
    }

    /// Counts the method named `method` that `owner` declares as having a body: the body of a
    /// heading reported with E0757 is not checked, and E0723 is not reported on top of it. Only
    /// when the name is not overloaded: otherwise the declaration the heading meant is not known,
    /// and marking every overload would make a later correct body E0701.
    fn mark_has_body(&mut self, owner: TypeId, method: &str) {
        let routines: Vec<super::RoutineId> = self
            .types
            .class_methods(owner)
            .iter()
            .filter(|m| m.name.eq_ignore_ascii_case(method))
            .filter_map(|m| m.routine)
            .collect();
        if let [r] = routines[..] {
            self.routines[r.0 as usize].has_body = true;
        }
    }

    /// The type parameters of the method a body's heading names (`function TUtil.Max<T>`;
    /// `docs/spec/generics.md` 1.2): those of its declaration. E0757 when they are not written
    /// as declared (or are left out), E0761 when the method is not generic. `None` after a
    /// diagnostic that stops the body; empty for a method that is not generic.
    fn generic_method_heading(
        &mut self,
        header: &ast::RoutineHeader,
        owner: TypeId,
        idents: &[crate::syntax::SyntaxToken],
        tp: Option<&ast::TypeParams>,
    ) -> Option<Vec<TypeId>> {
        let method = idents[1].text();
        let routines: Vec<super::RoutineId> = self
            .types
            .class_methods(owner)
            .iter()
            .filter(|m| m.name.eq_ignore_ascii_case(method))
            .filter_map(|m| m.routine)
            .collect();
        let generic: Vec<super::RoutineId> = routines
            .iter()
            .copied()
            .filter(|r| !self.routines[r.0 as usize].type_params.is_empty())
            .collect();
        let class_name =
            super::instances::base_name(&self.types.name(Ty::Class(owner))).to_string();
        let shown = format!("{class_name}.{method}");
        let Some(tp) = tp else {
            // Left out: only when no declaration of the name is without type parameters
            if let Some(&r) = generic.first()
                && routines.len() == generic.len()
            {
                let declared = self.routines[r.0 as usize].type_params.clone();
                let names: Vec<String> = declared
                    .iter()
                    .map(|p| self.types.get(*p).name.clone().unwrap_or_default())
                    .collect();
                let list = format!("<{}>", names.join(", "));
                let at = idents[1].text_range();
                let d = Diagnostic::error(
                    Code::E(757),
                    format!(
                        "type parameters of `{shown}` must be written as in its declaration: `{list}`"
                    ),
                    Stage::Sema,
                )
                .with_primary(self.span(at), "")
                .with_fix(Fix::safe(
                    format!("write `{list}`"),
                    vec![TextEdit::insert(self.input.file, at.end(), list.clone())],
                ));
                self.push(d);
                for r in generic {
                    self.routines[r.0 as usize].has_body = true;
                }
                return None;
            }
            return Some(Vec::new());
        };
        let arity = param_names(tp).len();
        // The declaration whose parameters the heading has (an overload is found by its
        // parameters, not by its order); else the first one of the number, which the heading's
        // own checks report
        let matching = generic.iter().copied().find(|r| {
            !self.routines[r.0 as usize].has_body
                && self.heading_matches(header, tp, *r, Some(owner))
        });
        let pick = matching
            .as_ref()
            .or_else(|| {
                generic
                    .iter()
                    .find(|r| self.routines[r.0 as usize].type_params.len() == arity)
            })
            .or(generic.first())
            .copied();
        let Some(r) = pick else {
            self.error(
                Code::E(761),
                format!("`{shown}` is not generic"),
                tp.syntax().text_range(),
            );
            return None;
        };
        let declared = self.routines[r.0 as usize].type_params.clone();
        if !self.check_heading_params(tp, &declared, &shown) {
            self.routines[r.0 as usize].has_body = true;
            return None;
        }
        Some(declared)
    }

    /// E0762 for an operation on a value of type parameter `param` that its constraints do not
    /// allow (`docs/spec/generics.md` 3.3).
    pub(super) fn param_misuse(&mut self, param: TypeId, op: ParamOp<'_>, range: TextRange) {
        let t = self.types.get(param).name.clone().unwrap_or_default();
        let decl = self.param_decl_span(param);
        let (msg, note) = match op {
            ParamOp::Member(m) => (
                format!("type parameter `{t}` has no member `{m}`"),
                format!("add a class or interface constraint that declares `{m}`"),
            ),
            ParamOp::SetElement => (
                format!("cannot use a value of type parameter `{t}` as a set element"),
                "a type parameter can only be used in ways that its constraints allow".to_string(),
            ),
            _ => {
                let what = match op {
                    ParamOp::Write => "write".to_string(),
                    ParamOp::Compare => "compare".to_string(),
                    ParamOp::AssignNil => "assign `nil` to".to_string(),
                    ParamOp::Convert => "convert".to_string(),
                    ParamOp::TypeTest => "test the type of".to_string(),
                    ParamOp::Free => "free".to_string(),
                    ParamOp::Create => "create".to_string(),
                    ParamOp::Operator(o) => format!("apply operator `{o}` to"),
                    ParamOp::Ordinal => "take the ordinal of".to_string(),
                    ParamOp::SizeOf => "take the size of".to_string(),
                    ParamOp::Member(_) | ParamOp::SetElement => unreachable!("handled above"),
                };
                let note = match op {
                    ParamOp::Free => format!(
                        "add the constraints `class, NotRefCounted` to free values of `{t}`"
                    ),
                    ParamOp::Compare | ParamOp::AssignNil => "add a `class` or interface constraint to compare references; to compare values, use `KeyEquals`".to_string(),
                    _ => "a type parameter can only be used in ways that its constraints allow"
                        .to_string(),
                };
                (
                    format!("cannot {what} a value of type parameter `{t}`"),
                    note,
                )
            }
        };
        let mut d =
            Diagnostic::error(Code::E(762), msg, Stage::Sema).with_primary(self.span(range), "");
        if let Some(s) = decl {
            d = d.with_secondary(s, "type parameter declared here");
        }
        d.notes.push(note);
        self.push(d);
    }

    /// The type parameter a type is, if any.
    pub(super) fn param_of(ty: Ty) -> Option<TypeId> {
        match ty {
            Ty::Param(p) => Some(p),
            _ => None,
        }
    }

    /// The class a value of type parameter `p` is an instance of by its constraints
    /// (`docs/spec/generics.md` 3.3): its class type constraint, or `TObject` for `class`.
    pub(super) fn param_class(&self, p: TypeId) -> Option<TypeId> {
        let cs = self.types.constraints(p);
        cs.iter()
            .find_map(|c| match c {
                Constraint::Type(Ty::Class(c)) => Some(*c),
                _ => None,
            })
            .or_else(|| {
                cs.contains(&Constraint::Class)
                    .then_some(super::types::TOBJECT)
            })
    }

    /// The interface type constraints of `p`, in the order written.
    pub(super) fn param_intfs(&self, p: TypeId) -> Vec<TypeId> {
        self.types
            .constraints(p)
            .iter()
            .filter_map(|c| match c {
                Constraint::Type(Ty::Intf(i)) => Some(*i),
                _ => None,
            })
            .collect()
    }

    /// The type a value of type parameter `ty` is seen as when its reference is used (`nil`,
    /// the identity `=`, `is` / `as`, `Supports`; `docs/spec/generics.md` 3.3): its class
    /// (`param_class`), or else its first interface type constraint. `None` for a type that is
    /// not a type parameter or whose constraints do not make it a reference.
    pub(super) fn param_reference(&self, ty: Ty) -> Option<Ty> {
        let p = Self::param_of(ty)?;
        self.param_class(p)
            .map(Ty::Class)
            .or_else(|| self.param_intfs(p).first().map(|i| Ty::Intf(*i)))
    }

    /// Whether a value of type parameter `p` can be freed (`docs/spec/generics.md` 3.3): `p`
    /// takes only manually managed classes (`class` with `NotRefCounted`, or a class type
    /// constraint that is a manually managed class other than `TObject`).
    pub(super) fn param_frees(&self, p: TypeId) -> bool {
        let cs = self.types.constraints(p);
        if cs.contains(&Constraint::Class) && cs.contains(&Constraint::NotRefCounted) {
            return true;
        }
        cs.iter().any(|c| match c {
            Constraint::Type(Ty::Class(c)) => self.is_manual_class(*c),
            _ => false,
        })
    }

    /// A manually managed class other than `TObject`.
    fn is_manual_class(&self, c: TypeId) -> bool {
        c != super::types::TOBJECT && !self.is_ref_counted(c)
    }

    /// The value `x` of a type parameter seen as the type of its constraints that declares the
    /// member `name` (`docs/spec/generics.md` 3.3, 4.3): the class type constraint (or
    /// `TObject` for `class`) first, then the interface type constraints in the order written.
    /// The member is found at the declaration, so a concrete type that hides it does not change
    /// what is called. E0762 when no constraint declares it, when two interface type
    /// constraints do (and the class does not), and for `Free` / `Destroy` when `T` may be a
    /// reference counted class (`None`).
    pub(super) fn param_member_view(
        &mut self,
        x: super::hir::Expr,
        name: &str,
        whole: TextRange,
    ) -> Option<super::hir::Expr> {
        let p = Self::param_of(x.ty)?;
        let class = self.param_class(p);
        let frees = ["free", "destroy"].contains(&name.to_ascii_lowercase().as_str());
        if frees && !self.param_frees(p) {
            self.param_misuse(p, ParamOp::Free, whole);
            return None;
        }
        let in_class = class.is_some_and(|c| self.class_declares(c, name));
        let view = if in_class {
            class.map(Ty::Class)
        } else {
            let mut found: Vec<TypeId> = Vec::new();
            for i in self.param_intfs(p) {
                if let Some(owner) = self.intf_declarer(i, name)
                    && !found.contains(&owner)
                {
                    found.push(owner);
                }
            }
            if found.len() > 1 {
                self.param_ambiguous(p, name, &found, whole);
                return None;
            }
            match found.first() {
                Some(&owner) => self
                    .param_intfs(p)
                    .into_iter()
                    .find(|i| self.intf_declarer(*i, name) == Some(owner))
                    .map(Ty::Intf),
                None => None,
            }
        };
        let Some(view) = view else {
            self.param_misuse(p, ParamOp::Member(name), whole);
            return None;
        };
        let mut x = x;
        x.ty = view;
        Some(x)
    }

    /// Whether a class or one of its ancestors declares a member `name` (a field, a method, a
    /// property or a class member), visible or not (E0743 is reported where it is found).
    fn class_declares(&self, class: TypeId, name: &str) -> bool {
        let has = |c: TypeId| {
            let fields = match &self.types.get(c).kind {
                TypeKind::Class { fields, .. } => {
                    fields.iter().any(|f| f.name.eq_ignore_ascii_case(name))
                }
                _ => false,
            };
            fields
                || self
                    .types
                    .class_methods(c)
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(name))
                || self
                    .types
                    .class_properties(c)
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(name))
                || self
                    .types
                    .class_statics(c)
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(name))
        };
        self.types.class_chain(class).into_iter().any(has) || super::methods::is_class_builtin(name)
    }

    /// The interface among `intf` and its ancestors that declares a method or a property
    /// `name`.
    fn intf_declarer(&self, intf: TypeId, name: &str) -> Option<TypeId> {
        self.types.class_chain(intf).into_iter().find(|&c| {
            self.types
                .class_methods(c)
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case(name))
                || self
                    .types
                    .class_properties(c)
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(name))
        })
    }

    /// E0762 for a member that two interface type constraints declare
    /// (`docs/spec/generics.md` 3.3).
    fn param_ambiguous(&mut self, p: TypeId, name: &str, owners: &[TypeId], whole: TextRange) {
        let a = self.tn(Ty::Intf(owners[0]));
        let b = self.tn(Ty::Intf(owners[1]));
        let msg = format!("`{name}` is ambiguous: it is declared in constraints `{a}` and `{b}`");
        let mut d =
            Diagnostic::error(Code::E(762), msg, Stage::Sema).with_primary(self.span(whole), "");
        if let Some(s) = self.param_decl_span(p) {
            d = d.with_secondary(s, "type parameter declared here");
        }
        d.notes.push(format!(
            "add a class or interface constraint that declares `{name}`"
        ));
        self.push(d);
    }

    /// Where a type parameter is declared.
    fn param_decl_span(&self, p: TypeId) -> Option<Span> {
        let def = self.types.get(p);
        match def.kind {
            TypeKind::Param { unit, .. } => Some(Span::new(self.unit_file(unit), def.span)),
            _ => None,
        }
    }

    /// `T.Create` of a type parameter with the `constructor` constraint
    /// (`docs/spec/generics.md` 3.3, 4.3): a new instance of `T`, whose constructor is chosen
    /// when the body is instantiated (`Inst::param_new`). It takes no argument (E0705).
    pub(super) fn param_new(
        &mut self,
        p: TypeId,
        args: &[ast::Arg],
        span: TextRange,
    ) -> super::hir::Expr {
        use super::hir::{Dispatch, Expr, ExprKind};
        let values: Vec<_> = args
            .iter()
            .filter_map(|a| a.value())
            .map(|v| self.lower_expr(&v))
            .collect();
        if !values.is_empty() {
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected 0, found {}",
                    values.len()
                ),
                span,
            );
            return super::expr::err_expr(span);
        }
        Expr {
            kind: ExprKind::New {
                class: Ty::Param(p),
                ctor: None,
                args: Vec::new(),
                after: false,
                of: None,
                dispatch: Dispatch::Method,
            },
            ty: Ty::Param(p),
            value: None,
            span,
        }
    }

    /// The implicit conversion of a value of a type parameter to a class or an interface its
    /// constraints guarantee (`docs/spec/generics.md` 3.3): `TObject` (for `class`), the class
    /// type constraint and its ancestors and interfaces, and the interface type constraints and
    /// their ancestors. The value keeps its handle (the same as a descendant to an ancestor).
    pub(super) fn param_upcast(&self, x: &super::hir::Expr, target: Ty) -> bool {
        let Some(p) = Self::param_of(x.ty) else {
            return false;
        };
        let class = self.param_class(p);
        let intfs = self.param_intfs(p);
        match target {
            Ty::Class(t) => class.is_some_and(|c| c == t || self.types.is_descendant(c, t)),
            Ty::Intf(t) => {
                class.is_some_and(|c| self.types.implements(c, t))
                    || intfs
                        .iter()
                        .any(|&i| i == t || self.types.is_descendant(i, t))
            }
            _ => false,
        }
    }
}

/// Where a generic type is looked up (`docs/spec/generics.md` 2.3, 4.2).
#[derive(Clone, Copy)]
pub(super) enum GenericScope {
    /// The file and the units in `uses`.
    Visible,
    /// One unit named before the type (`Unit.TBox<…>`).
    Unit(super::UnitRef),
    /// A class or record named before the type (`TFoo.TBox<…>`; 2.5).
    Type(TypeId),
}

impl<'a, 's> Sema<'a, 's> {
    /// Runs `f` with the parts of the analysis an instantiation changes; its diagnostics (E0764)
    /// are reported.
    pub(super) fn with_inst<R>(&mut self, f: impl FnOnce(&mut super::instances::Inst) -> R) -> R {
        let mut diags = Vec::new();
        let r = {
            let mut i = super::instances::Inst {
                types: &mut self.types,
                routines: &mut self.routines,
                inst: &mut self.instances,
                globals: &mut self.globals,
                diags: &mut diags,
                unit: self.unit_index,
                cur_origin: None,
            };
            f(&mut i)
        };
        for d in diags {
            self.push(d);
        }
        r
    }

    /// The generic types of a name (lowercase; `docs/spec/generics.md` 2.3), each with the unit
    /// in `uses` declaring it (`None` for the file and `System`). In the file and the units in
    /// `uses`, a number of type parameters is found first in the file, then in the units, then
    /// in `System` (`TProc<T>`, `TFunc<…>`, `TPredicate<T>`; 6.2): names with different numbers
    /// are different names. Two units declaring the same number are both returned (E2502).
    pub(super) fn generic_types_named(
        &mut self,
        lower: &str,
        scope: &GenericScope,
    ) -> Vec<(GenericTypeName, Option<usize>)> {
        let own: Vec<(GenericTypeName, Option<usize>)> = self
            .generic_types
            .iter()
            .filter(|g| g.lower == lower)
            .map(|g| (g.clone(), None))
            .collect();
        let of_unit = |s: &Self, i: usize, k: usize| {
            s.units[i]
                .generics
                .iter()
                .filter(|g| g.lower == lower)
                .map(|g| (g.clone(), Some(k)))
                .collect::<Vec<_>>()
        };
        let nested_in = |s: &Self, c: TypeId| {
            s.types
                .nested_generics
                .iter()
                .filter(|g| g.owner == c && g.lower == lower)
                .map(|g| {
                    (
                        GenericTypeName {
                            lower: g.lower.clone(),
                            arity: g.arity,
                            id: g.id,
                            span: g.span,
                            interface: true,
                        },
                        None,
                    )
                })
                .collect::<Vec<_>>()
        };
        match scope {
            GenericScope::Type(c) => nested_in(self, *c),
            GenericScope::Unit(super::UnitRef::Own) => own,
            GenericScope::Unit(super::UnitRef::Package(k)) => self.used_units[*k]
                .iface
                .map(|i| of_unit(self, i, *k))
                .unwrap_or_default(),
            GenericScope::Unit(super::UnitRef::System) => self
                .types
                .system_generics_named(lower)
                .into_iter()
                .map(|(arity, id)| {
                    (
                        GenericTypeName {
                            lower: lower.to_string(),
                            arity,
                            id,
                            span: TextRange::default(),
                            interface: true,
                        },
                        None,
                    )
                })
                .collect(),
            GenericScope::Unit(super::UnitRef::SysUtils) => self
                .sysutils_part()
                .and_then(|k| self.used_units[k].iface.map(|i| of_unit(self, i, k)))
                .unwrap_or_default(),
            GenericScope::Visible => {
                // The generic types nested in the classes being declared or whose method is
                // being analyzed, and in the classes they are nested in (2.5)
                let mut classes: Vec<TypeId> = Vec::new();
                let starts: Vec<TypeId> = self
                    .method_class()
                    .into_iter()
                    .chain(self.class_scopes.iter().rev().map(|c| c.class))
                    .collect();
                for c in starts {
                    let mut cur = Some(c);
                    while let Some(k) = cur {
                        if classes.contains(&k) {
                            break;
                        }
                        classes.push(k);
                        cur = self
                            .lexical_outer
                            .get(&k)
                            .copied()
                            .or_else(|| self.types.generic_outer.get(&k).copied());
                    }
                }
                let mut found: Vec<(GenericTypeName, Option<usize>)> = Vec::new();
                for c in classes {
                    for f in nested_in(self, c) {
                        if !found.iter().any(|(g, _)| g.arity == f.0.arity) {
                            found.push(f);
                        }
                    }
                }
                let arities: Vec<usize> = found.iter().map(|(g, _)| g.arity).collect();
                found.extend(own.into_iter().filter(|(g, _)| !arities.contains(&g.arity)));
                let own_arities: Vec<usize> = found.iter().map(|(g, _)| g.arity).collect();
                let units: Vec<(usize, usize)> = self
                    .visible_units()
                    .filter(|(_, u)| u.alias.is_none())
                    .filter_map(|(k, u)| u.iface.map(|i| (k, i)))
                    .collect();
                for (k, i) in units {
                    found.extend(
                        of_unit(self, i, k)
                            .into_iter()
                            .filter(|(g, _)| !own_arities.contains(&g.arity)),
                    );
                }
                let arities: Vec<usize> = found.iter().map(|(g, _)| g.arity).collect();
                found.extend(
                    self.generic_types_named(lower, &GenericScope::Unit(super::UnitRef::System))
                        .into_iter()
                        .filter(|(g, _)| !arities.contains(&g.arity)),
                );
                found
            }
        }
    }

    /// A generic type written with type arguments (`TBox<Integer>`; `docs/spec/generics.md` 1.3,
    /// 1.4, 4.2): the number of type arguments (E0761), whether they satisfy the constraints
    /// (E0759, E0760), and the instantiation. `None` when no generic type has the name.
    pub(super) fn generic_type_with_args(
        &mut self,
        name: &str,
        name_range: TextRange,
        args: Vec<ast::TypeSpec>,
        args_range: TextRange,
        scope: GenericScope,
    ) -> Option<Ty> {
        let lower = name.to_ascii_lowercase();
        let candidates = self.generic_types_named(&lower, &scope);
        if candidates.is_empty() {
            return None;
        }
        let tys: Vec<(Ty, TextRange)> = args
            .iter()
            .map(|a| {
                let r = ast::trimmed_range(a.syntax());
                (self.resolve_type(Some(a.clone())), r)
            })
            .collect();
        let same: Vec<&(GenericTypeName, Option<usize>)> = candidates
            .iter()
            .filter(|(g, _)| g.arity == tys.len())
            .collect();
        let Some((g, _)) = same.first().copied() else {
            let mut counts: Vec<usize> = candidates.iter().map(|(g, _)| g.arity).collect();
            counts.sort_unstable();
            counts.dedup();
            let n = counts
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" or ");
            let plural = if counts == [1] { "" } else { "s" };
            self.error(
                Code::E(761),
                format!(
                    "`{name}` expects {n} type argument{plural}, found {}",
                    tys.len()
                ),
                args_range,
            );
            return Some(Ty::Error);
        };
        if same.len() > 1 {
            // The same name with the same number of type parameters in two units (2.3)
            let found: Vec<(usize, Option<TextRange>)> = same
                .iter()
                .filter_map(|(g, k)| k.map(|k| (k, Some(g.span))))
                .collect();
            self.several_units(name, name_range, &found, false);
            return Some(Ty::Error);
        }
        if tys.iter().any(|(t, _)| t.is_error()) {
            return Some(Ty::Error);
        }
        let def = g.id;
        // The definition's own type parameters are the definition itself (4.1): they satisfy
        // its constraints
        let own: Vec<Ty> = self
            .types
            .own_type_params(def)
            .iter()
            .map(|p| Ty::Param(*p))
            .collect();
        let is_own = tys.iter().map(|(t, _)| *t).eq(own.iter().copied());
        // A type argument that does not satisfy a constraint is reported (E0759 / E0760) and the
        // type is still instantiated, so that the same type written where the check is deferred
        // and where it is not is the same type (a method's declaration in a `type` section and the
        // heading of its body)
        if !is_own {
            if let Some(d) = &mut self.deferred_checks {
                // Checked later (`begin_deferred_checks`), except in a dry run, whose diagnostics
                // are dropped
                if self.dry_run == 0 {
                    d.push((def, name_range, tys.clone()));
                }
            } else {
                self.check_constraints(def, name_range, &tys);
            }
        }
        let origin = Some(self.span(args_range));
        // A type nested in a generic type takes the outer type's type parameters first (2.5)
        let all = self.types.type_params(def).len();
        let mut args: Vec<Ty> = self.types.type_params(def)[..all - own.len()]
            .iter()
            .map(|p| Ty::Param(*p))
            .collect();
        args.extend(tys.into_iter().map(|(t, _)| t));
        Some(self.with_inst(|i| i.instantiate(def, args, origin, None)))
    }

    /// E0761 for a generic type written without type arguments (`docs/spec/generics.md` 2.3)
    /// when no other declaration has the name. Returns whether it was reported.
    pub(super) fn generic_needs_args(&mut self, name: &str, range: TextRange) -> bool {
        let lower = name.to_ascii_lowercase();
        if self
            .generic_types_named(&lower, &GenericScope::Visible)
            .is_empty()
        {
            return false;
        }
        self.error(
            Code::E(761),
            format!("generic type `{name}` needs type arguments"),
            range,
        );
        true
    }

    /// Whether the type arguments satisfy the constraints of `def`'s type parameters
    /// (`docs/spec/generics.md` 3.1, 3.6): E0759 / E0760 at each argument that does not. For
    /// an owning container of the standard library, E0760 suggests the container without
    /// ownership in place of the name written at `name_range`.
    fn check_constraints(
        &mut self,
        def: TypeId,
        name_range: TextRange,
        args: &[(Ty, TextRange)],
    ) -> bool {
        let params = self.types.own_type_params(def).to_vec();
        let def_name = self.types.get(def).name.clone().unwrap_or_default();
        let replace = self.owning_container(def).map(|c| (name_range, c.plain));
        self.check_constraints_with(&params, &def_name, args, None, replace)
    }

    /// Whether the type arguments satisfy the constraints of the type parameters `params` of
    /// the generic declaration `def_name` (a type or a routine; `docs/spec/generics.md` 3.1,
    /// 3.6): E0759 / E0760 at each argument that does not. The type parameters in the
    /// constraints (`T: IComparable<T>`; 1.1) are replaced with the type arguments, and those of
    /// the class of a method with the class's when `owner` (the class of the routine) is an
    /// instantiation.
    pub(super) fn check_param_constraints(
        &mut self,
        params: &[TypeId],
        def_name: &str,
        args: &[(Ty, TextRange)],
        owner: Option<TypeId>,
    ) -> bool {
        self.check_constraints_with(params, def_name, args, owner, None)
    }

    /// The full name of a generic definition without its type parameters
    /// (`System.Generics.Collections.TObjectList`).
    fn generic_def_name(&self, def: TypeId) -> &str {
        let full = self.types.get(def).full_name.as_deref().unwrap_or_default();
        full.split_once('<').map_or(full, |(n, _)| n)
    }

    /// The note of E0725 for a procedure used as a value: the procedures `Pop` / `Dequeue` of an
    /// owning stack or queue of the standard library free the item they take
    /// (`docs/spec/collections.md` 9.1, 12.4).
    pub(super) fn procedure_value_note(&self, r: super::RoutineId) -> Option<&'static str> {
        let info = &self.routines[r.0 as usize];
        let def = self.types.instance_of(info.method_of?)?.def;
        let take = self.owning_container(def)?.take?;
        let name = info.name.rsplit('.').next().unwrap_or_default();
        name.eq_ignore_ascii_case(take)
            .then_some("use `Extract` to take the object out without freeing it")
    }

    /// E0768 (`docs/spec/collections.md` 9.3, 12.4): a constructor of `TObjectDictionary<K, V>`
    /// whose `Ownerships` argument is a constant that owns a side whose type argument is not a
    /// class. The call is written at `span`; a type argument passed on (a type parameter) is
    /// checked when the dictionary is created.
    pub(super) fn check_dictionary_ownerships(
        &mut self,
        ctor: super::RoutineId,
        args: &[super::hir::CallArg],
        span: TextRange,
    ) {
        let Some(owner) = self.routines[ctor.0 as usize].method_of else {
            return;
        };
        let Some(inst) = self.types.instance_of(owner) else {
            return;
        };
        let (def, sides) = (inst.def, inst.args.clone());
        if self.generic_def_name(def) != "System.Generics.Collections.TObjectDictionary" {
            return;
        }
        // The first parameter of each constructor of `TObjectDictionary` is `Ownerships`
        let Some(super::hir::CallArg::Value(arg)) = args.first() else {
            return;
        };
        let Some(super::types::Const::Set(words)) = &arg.value else {
            return;
        };
        let dict = self.tn(Ty::Class(owner));
        // The ordinals of `doOwnsKeys` and `doOwnsValues`
        for (ordinal, (option, role)) in [("doOwnsKeys", "keys"), ("doOwnsValues", "values")]
            .into_iter()
            .enumerate()
        {
            let owned = words.first().is_some_and(|w| w & (1 << ordinal) != 0);
            let Some(&side) = sides.get(ordinal) else {
                continue;
            };
            if !owned || matches!(side, Ty::Class(_) | Ty::Param(_)) || side.is_error() {
                continue;
            }
            let msg = format!(
                "`{option}` needs a class type for the {role} of `{dict}`, but `{}` is not a class",
                self.tn(side)
            );
            let mut d = Diagnostic::error(Code::E(768), msg, Stage::Sema)
                .with_primary(self.span(arg.span), "");
            if let Some(at) = self.written_type_arg(span, arg.span, ordinal) {
                d = d.with_secondary(self.span(at), "type argument written here");
            }
            d.notes
                .push("only class instances can be freed by the dictionary".into());
            if let Some(edit) = self.set_element_removal(arg.span, option) {
                d = d.with_fix(Fix::suggestion(format!("remove `{option}`"), vec![edit]));
            }
            self.push(d);
        }
    }

    /// The range of the `index`th type argument written in the call at `span`
    /// (`TObjectDictionary<string, TFoo>.Create(…)`), whose first argument starts at `first`;
    /// `None` when the call names the class without type arguments (an alias). Only the class
    /// before the arguments is looked at: an argument may name another generic type
    /// (`TEqualityComparer<Integer>.Default`).
    fn written_type_arg(
        &self,
        span: TextRange,
        first: TextRange,
        index: usize,
    ) -> Option<TextRange> {
        let node = match self.input.root.covering_element(span) {
            rowan::NodeOrToken::Node(n) => n,
            rowan::NodeOrToken::Token(t) => t.parent()?,
        };
        let args = node
            .descendants()
            .filter_map(ast::TypeArgs::cast)
            .find(|a| ast::trimmed_range(a.syntax()).end() <= first.start())?;
        let arg = args.args().nth(index)?;
        Some(ast::trimmed_range(arg.syntax()))
    }

    /// The edit that removes the element naming `option` from the set constructor written at
    /// `at`, with the comma that separates it; `None` when `at` is not a constructor naming it.
    fn set_element_removal(&self, at: TextRange, option: &str) -> Option<TextEdit> {
        let node = match self.input.root.covering_element(at) {
            rowan::NodeOrToken::Node(n) => n,
            rowan::NodeOrToken::Token(t) => t.parent()?,
        };
        let ctor = node
            .ancestors()
            .filter_map(ast::ArrayCtor::cast)
            .find(|c| ast::trimmed_range(c.syntax()) == at)?;
        // A range keeps no text to match: it names more than the option, so removing it would
        // drop the others too
        let elems: Vec<(TextRange, Option<String>)> = ctor
            .elements()
            .iter()
            .map(|e| match e {
                ast::CtorElem::Expr(x) => (
                    ast::trimmed_range(x.syntax()),
                    Some(ast::source_text_normalized(x.syntax())),
                ),
                ast::CtorElem::Range(r) => (ast::trimmed_range(r.syntax()), None),
            })
            .collect();
        // `doOwnsKeys` written qualified with its type (`docs/spec/types.md` 8.1)
        let i = elems.iter().position(|(_, t)| {
            t.as_deref().is_some_and(|t| {
                let last = t.rsplit('.').next().unwrap_or_default();
                last.trim().eq_ignore_ascii_case(option)
            })
        })?;
        let elems: Vec<TextRange> = elems.into_iter().map(|(r, _)| r).collect();
        let range = if i + 1 < elems.len() {
            TextRange::new(elems[i].start(), elems[i + 1].start())
        } else if i > 0 {
            TextRange::new(elems[i - 1].end(), elems[i].end())
        } else {
            elems[i]
        };
        Some(TextEdit::delete(self.input.file, range))
    }

    /// The owning container of the standard library that the generic definition `def` is.
    fn owning_container(&self, def: TypeId) -> Option<&'static OwningContainer> {
        let name = self.generic_def_name(def);
        OWNING_CONTAINERS.iter().find(|c| c.name == name)
    }

    /// [`Sema::check_param_constraints`], with the name to write at a range in the fix of
    /// E0760 (`replace`).
    fn check_constraints_with(
        &mut self,
        params: &[TypeId],
        def_name: &str,
        args: &[(Ty, TextRange)],
        owner: Option<TypeId>,
        mut replace: Option<(TextRange, &str)>,
    ) -> bool {
        let mut env_params = params.to_vec();
        let mut env_args: Vec<Ty> = args.iter().map(|(t, _)| *t).collect();
        if let Some(i) = owner.and_then(|o| self.types.instance_of(o)) {
            let (def, inst_args) = (i.def, i.args.clone());
            env_params.extend(self.types.type_params(def).iter().copied());
            env_args.extend(inst_args);
        }
        let env = super::instances::Env::new(env_params, env_args, None);
        let mut ok = true;
        for (p, (arg, range)) in params.iter().zip(args) {
            let mut constraints = self.types.constraints(*p).to_vec();
            if constraints.is_empty() {
                continue;
            }
            for c in &mut constraints {
                if let Constraint::Type(t) = c
                    && self.types.has_param(*t)
                {
                    *t = self.with_inst(|i| i.subst(*t, &env));
                }
            }
            if constraints
                .iter()
                .any(|c| matches!(c, Constraint::Type(t) if t.is_error()))
            {
                continue;
            }
            let pname = self.types.get(*p).name.clone().unwrap_or_default();
            if let Ty::Param(q) = *arg {
                // A type parameter passed on: its constraints must guarantee these (3.5)
                ok &= self.check_passed_param(q, &constraints, &pname, def_name, *range);
                continue;
            }
            let decl = self.param_decl_span(*p);
            let arg_name = self.tn(*arg);
            // `NotRefCounted` first: a reference counted class or an interface gets its own error
            // (E0760) even when another constraint (`class`) does not hold either (3.6)
            constraints.sort_by_key(|c| *c != Constraint::NotRefCounted);
            for c in constraints {
                if let Some(msg) = self.refcount_misuse(c, *arg) {
                    let msg = format!(
                        "`{arg_name}` {msg} and cannot be used for `{pname}` of `{def_name}`"
                    );
                    let mut d = Diagnostic::error(Code::E(760), msg, Stage::Sema)
                        .with_primary(self.span(*range), "");
                    if let Some(s) = decl {
                        d = d.with_secondary(s, "type parameter declared here");
                    }
                    d.notes.push("reference counted objects are freed when the last reference goes away, so they do not need an owning container".into());
                    // Once: the type arguments of one written name share the edit
                    // (`TObjectDictionary<TRef, TRef>`)
                    if let Some((at, plain)) = replace.take() {
                        // The arguments of the constructor (`[doOwnsValues]`) would not fit the
                        // container without ownership, so it is only a suggestion
                        let edit = TextEdit::replace(self.input.file, at, plain);
                        d = d.with_fix(Fix::suggestion(format!("use `{plain}`"), vec![edit]));
                    }
                    self.push(d);
                    ok = false;
                    break;
                }
                if self.satisfies(c, *arg) {
                    continue;
                }
                let ctext = self.constraint_text(c);
                let mut d = Diagnostic::error(
                    Code::E(759),
                    format!(
                        "`{arg_name}` does not satisfy the constraint `{ctext}` of `{pname}` in `{def_name}`"
                    ),
                    Stage::Sema,
                )
                .with_primary(self.span(*range), "");
                if let Some(s) = decl {
                    d = d.with_secondary(s, "type parameter declared here");
                }
                if c == Constraint::Constructor && matches!(arg, Ty::Class(_)) {
                    d.notes.push("the type needs a public constructor `Create` without parameters, and must not be abstract".into());
                }
                self.push(d);
                ok = false;
                break;
            }
        }
        ok
    }

    /// Whether the constraints of the type parameter `q` passed as the type argument of `pname`
    /// guarantee `pname`'s constraints (`docs/spec/generics.md` 3.5): E0759 at the first one
    /// they do not, with `q`'s declaration and the constraint to add.
    fn check_passed_param(
        &mut self,
        q: TypeId,
        constraints: &[Constraint],
        pname: &str,
        def_name: &str,
        range: TextRange,
    ) -> bool {
        for &c in constraints {
            if self.guarantees(q, c) {
                continue;
            }
            let qname = self.types.get(q).name.clone().unwrap_or_default();
            let ctext = self.constraint_text(c);
            let mut d = Diagnostic::error(
                Code::E(759),
                format!(
                    "`{qname}` does not satisfy the constraint `{ctext}` of `{pname}` in `{def_name}`"
                ),
                Stage::Sema,
            )
            .with_primary(self.span(range), "");
            if let Some(s) = self.param_decl_span(q) {
                d = d.with_secondary(s, "type parameter declared here");
            }
            d.notes
                .push(format!("add the constraint `{ctext}` to `{qname}`"));
            self.push(d);
            return false;
        }
        true
    }

    /// Whether the constraints of the type parameter `q` guarantee the constraint `c`
    /// (`docs/spec/generics.md` 3.5).
    fn guarantees(&self, q: TypeId, c: Constraint) -> bool {
        let cs = self.types.constraints(q);
        let class = cs.iter().find_map(|c| match c {
            Constraint::Type(Ty::Class(c)) => Some(*c),
            _ => None,
        });
        match c {
            Constraint::Class => cs.contains(&Constraint::Class) || class.is_some(),
            Constraint::Record => cs.contains(&Constraint::Record),
            Constraint::Constructor => cs.contains(&Constraint::Constructor),
            Constraint::NotRefCounted => {
                cs.contains(&Constraint::NotRefCounted)
                    || cs.contains(&Constraint::Record)
                    || class.is_some_and(|c| self.is_manual_class(c))
            }
            Constraint::Type(Ty::Class(t)) => {
                class.is_some_and(|c| c == t || self.types.is_descendant(c, t))
            }
            Constraint::Type(Ty::Intf(t)) => {
                class.is_some_and(|c| self.types.implements(c, t))
                    || self
                        .param_intfs(q)
                        .iter()
                        .any(|&i| i == t || self.types.is_descendant(i, t))
            }
            Constraint::Type(_) => false,
        }
    }

    /// For `NotRefCounted` (`docs/spec/generics.md` 3.6), what makes a type fail it: `is a
    /// reference counted class` / `is an interface` (E0760).
    fn refcount_misuse(&self, c: Constraint, arg: Ty) -> Option<&'static str> {
        if c != Constraint::NotRefCounted {
            return None;
        }
        match arg {
            Ty::Class(id) if self.is_ref_counted(id) => Some("is a reference counted class"),
            Ty::Intf(_) => Some("is an interface"),
            _ => None,
        }
    }

    fn is_ref_counted(&self, id: TypeId) -> bool {
        matches!(
            self.types.get(id).kind,
            TypeKind::Class {
                ref_counted: true,
                ..
            }
        )
    }

    /// Whether a type satisfies a constraint (`docs/spec/generics.md` 3.1).
    fn satisfies(&mut self, c: Constraint, arg: Ty) -> bool {
        match c {
            Constraint::Class => matches!(arg, Ty::Class(_)),
            Constraint::Record => {
                // A distinct type by its root (`docs/spec/types.md` 1.3)
                let arg = self.types.root(arg);
                arg.is_number()
                    || matches!(
                        arg,
                        Ty::Boolean
                            | Ty::Char
                            | Ty::Enum(_)
                            | Ty::Subrange(_)
                            | Ty::Set(_)
                            | Ty::BigSet(_)
                            | Ty::Record(_)
                            | Ty::Array(_)
                    )
            }
            Constraint::Constructor => match arg {
                Ty::Class(id) => !self.types.class_is_abstract(id) && self.has_default_ctor(id),
                _ => false,
            },
            Constraint::NotRefCounted => {
                !matches!(arg, Ty::Intf(_))
                    && !matches!(arg, Ty::Class(id) if self.is_ref_counted(id))
            }
            Constraint::Type(Ty::Class(c)) => {
                matches!(arg, Ty::Class(a) if a == c || self.types.is_descendant(a, c))
            }
            Constraint::Type(Ty::Intf(i)) => match arg {
                Ty::Intf(j) => j == i || self.intf_descends(j, i),
                Ty::Class(a) => self.types.implements(a, i),
                _ => false,
            },
            Constraint::Type(_) => false,
        }
    }

    fn intf_descends(&self, mut j: TypeId, i: TypeId) -> bool {
        while let Some(p) = self.types.parent(j) {
            if p == i {
                return true;
            }
            j = p;
        }
        false
    }

    /// Whether a class has a public constructor `Create` that can be called without arguments,
    /// found as the name is found (`docs/spec/generics.md` 3.1).
    fn has_default_ctor(&self, class: TypeId) -> bool {
        default_ctor(&self.types, &self.routines, class).is_some()
    }

    /// Marks the generic types declared so far as complete and makes the members of their
    /// instantiations (`docs/spec/generics.md` 4.2).
    pub(super) fn generics_declared(&mut self) {
        // With the generic types nested in classes and the types nested in generic types
        let ids: Vec<TypeId> = self.types.generics.iter().map(|(g, _)| *g).collect();
        for id in ids {
            self.types.generic_done.insert(id);
        }
        self.with_inst(|i| i.fill_ready());
    }
}

/// The type parameters a routine heading writes (`docs/spec/generics.md` 1.2).
pub(super) enum HeadingParams {
    /// None (and none are missing).
    None,
    /// Reported: the declaration is not analyzed further.
    Failed,
    /// The generic type of a method's body (`TStack<T>.Push`), and the type parameters of the
    /// routine or method itself: those of its first declaration, or new ones when the heading
    /// is the first declaration (`fresh`).
    Generic {
        class: Option<TypeId>,
        own: Vec<TypeId>,
        fresh: bool,
    },
}

impl<'a, 's> Sema<'a, 's> {
    /// The type parameter of a generic routine whose heading or body is being analyzed, by its
    /// lowercase name (`docs/spec/generics.md` 2.2).
    pub(super) fn routine_type_param(&self, lower: &str) -> Option<TypeId> {
        self.routine_type_params
            .iter()
            .rev()
            .find(|(n, _)| n == lower)
            .map(|(_, p)| *p)
    }

    /// Makes the type parameters of a routine or method seen (until the returned mark is passed
    /// to [`Sema::pop_routine_type_params`]).
    pub(super) fn push_routine_type_params(&mut self, own: &[TypeId]) -> usize {
        let mark = self.routine_type_params.len();
        for &p in own {
            let n = self.types.get(p).name.clone().unwrap_or_default();
            self.routine_type_params.push((n.to_ascii_lowercase(), p));
        }
        mark
    }

    pub(super) fn pop_routine_type_params(&mut self, mark: usize) {
        self.routine_type_params.truncate(mark);
    }

    /// The type parameters of the first declaration of a generic routine or method
    /// (`docs/spec/generics.md` 1.2, 2.2): E0701 for a name written twice or a name of the
    /// class's type parameters; the constraints are resolved (3.1, 3.2).
    pub(super) fn declare_routine_type_params(
        &mut self,
        tp: &ast::TypeParams,
        class: Option<TypeId>,
    ) -> Vec<TypeId> {
        let class_params: Vec<(String, TextRange, u32)> = class
            .map(|c| {
                self.types
                    .type_params(c)
                    .iter()
                    .map(|p| {
                        let d = self.types.get(*p);
                        let unit = match d.kind {
                            TypeKind::Param { unit, .. } => unit,
                            _ => self.unit_index,
                        };
                        (d.name.clone().unwrap_or_default(), d.span, unit)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut ids: Vec<TypeId> = Vec::new();
        let mut names: Vec<(String, TextRange)> = Vec::new();
        // The type parameters of each group, made before the constraints, which can have them as
        // type arguments (`T: IComparable<T>`; 1.1)
        let mut groups: Vec<(ast::TypeParamGroup, Vec<TypeId>)> = Vec::new();
        for g in tp.groups() {
            let mut of_group = Vec::new();
            for t in g.names() {
                let (n, r) = (t.text().to_string(), t.text_range());
                let twice = names
                    .iter()
                    .find(|(p, _)| p.eq_ignore_ascii_case(&n))
                    .map(|(_, r)| Span::new(self.input.file, *r));
                // The name of a type parameter of the class: reported, and still declared (it
                // hides the class's in the method), so the method's body finds it
                let of_class = class_params
                    .iter()
                    .find(|(p, _, _)| p.eq_ignore_ascii_case(&n))
                    .map(|(_, r, u)| Span::new(self.unit_file(*u), *r));
                if let Some(prev) = twice.or(of_class) {
                    let d = Diagnostic::error(
                        Code::E(701),
                        format!("`{n}` is already declared"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(r), "declared again here")
                    .with_secondary(prev, "declared here");
                    self.push(d);
                    if twice.is_some() {
                        continue;
                    }
                }
                let id = self.types.add(TypeDef {
                    name: Some(n.clone()),
                    full_name: None,
                    kind: TypeKind::Param {
                        unit: self.unit_index,
                        index: ids.len() as u32,
                        constraints: Vec::new(),
                    },
                    span: r,
                });
                names.push((n, r));
                ids.push(id);
                of_group.push(id);
            }
            groups.push((g, of_group));
        }
        let mark = self.push_routine_type_params(&ids);
        self.with_deferred_checks(|s| {
            for (g, of_group) in groups {
                let cs = s.resolve_constraints(&g);
                for p in of_group {
                    s.set_constraints(p, cs.clone());
                }
            }
        });
        self.pop_routine_type_params(mark);
        ids
    }

    /// The type parameters of a heading written again for a declaration that has them
    /// (`docs/spec/generics.md` 1.2): the same names as `declared` (E0757 otherwise, with the
    /// fix writing them), and no constraints (E0757 with the fix removing them). `name` is the
    /// declaration as the message writes it. Returns whether the names are the same.
    pub(super) fn check_heading_params(
        &mut self,
        tp: &ast::TypeParams,
        declared: &[TypeId],
        name: &str,
    ) -> bool {
        let written = param_names(tp);
        let declared: Vec<String> = declared
            .iter()
            .map(|p| self.types.get(*p).name.clone().unwrap_or_default())
            .collect();
        let same = declared.len() == written.len()
            && declared
                .iter()
                .zip(&written)
                .all(|(a, (b, _))| a.eq_ignore_ascii_case(b));
        let tp_range = tp.syntax().text_range();
        if !same {
            let list = format!("<{}>", declared.join(", "));
            let d = Diagnostic::error(
                Code::E(757),
                format!(
                    "type parameters of `{name}` must be written as in its declaration: `{list}`"
                ),
                Stage::Sema,
            )
            .with_primary(self.span(tp_range), "")
            .with_fix(Fix::safe(
                format!("write `{list}`"),
                vec![TextEdit::replace(self.input.file, tp_range, list.clone())],
            ));
            self.push(d);
            return false;
        }
        for g in tp.groups() {
            let Some(colon) = g.colon_token() else {
                continue;
            };
            let range = TextRange::new(colon.text_range().start(), g.syntax().text_range().end());
            let d = Diagnostic::error(
                Code::E(757),
                format!("constraints are written only in the first declaration of `{name}`"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_fix(Fix::safe(
                "remove the constraints",
                vec![TextEdit::delete(self.input.file, range)],
            ));
            self.push(d);
        }
        true
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// E0763 for a generic routine made a value without type arguments
    /// (`docs/spec/generics.md` 5.1): they are not inferred there. Returns whether it was
    /// reported.
    pub(super) fn generic_value_without_args(
        &mut self,
        set: &[super::RoutineId],
        range: TextRange,
    ) -> bool {
        let Some(&r) = set
            .iter()
            .find(|r| !self.routines[r.0 as usize].type_params.is_empty())
        else {
            return false;
        };
        let info = &self.routines[r.0 as usize];
        let params: Vec<String> = info
            .type_params
            .iter()
            .map(|p| self.types.get(*p).name.clone().unwrap_or_default())
            .collect();
        let name = self.src(range);
        let first = params.first().cloned().unwrap_or_default();
        self.error(
            Code::E(763),
            format!("cannot infer the type argument `{first}` of `{name}`"),
            range,
        )
        .notes
        .push(format!(
            "write the type arguments, as in `{name}<{}>(…)`",
            params.join(", ")
        ));
        true
    }

    /// A name written with type arguments that is not a generic type
    /// (`docs/spec/generics.md` 1.4, 5.4): the instantiations of the generic routines or methods
    /// of the name with that number of type parameters, or E0761.
    pub(super) fn routine_type_args(
        &mut self,
        r: super::Resolved,
        name: &str,
        range: TextRange,
        args: &ast::TypeArgs,
    ) -> super::Resolved {
        use super::Resolved;
        let not_generic = |s: &mut Self| {
            s.error(Code::E(761), format!("`{name}` is not generic"), range)
                .notes
                .push("to compare, write `(A < B) > (C)`".into());
            Resolved::Error
        };
        if let Resolved::Key(op, None) = r {
            return self.key_type_arg(op, name, args);
        }
        let set = match &r {
            Resolved::Routine(id) => self.routine_overloads(*id),
            Resolved::Member {
                class,
                member:
                    super::methods::Member::Methods {
                        name: n,
                        inst: None,
                    },
                ..
            } => self.method_set(*class, n),
            Resolved::Error => return Resolved::Error,
            _ => return not_generic(self),
        };
        let list: Vec<ast::TypeSpec> = args.args().collect();
        let args_range = args.syntax().text_range();
        let generic: Vec<super::RoutineId> = set
            .iter()
            .copied()
            .filter(|c| !self.routines[c.0 as usize].type_params.is_empty())
            .collect();
        if generic.is_empty() {
            return not_generic(self);
        }
        let tys: Vec<(Ty, TextRange)> = list
            .iter()
            .map(|a| {
                let r = ast::trimmed_range(a.syntax());
                (self.resolve_type(Some(a.clone())), r)
            })
            .collect();
        let fitting: Vec<super::RoutineId> = generic
            .iter()
            .copied()
            .filter(|c| self.routines[c.0 as usize].type_params.len() == tys.len())
            .collect();
        if fitting.is_empty() {
            let mut counts: Vec<usize> = generic
                .iter()
                .map(|c| self.routines[c.0 as usize].type_params.len())
                .collect();
            counts.sort_unstable();
            counts.dedup();
            let n = counts
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" or ");
            let plural = if counts == [1] { "" } else { "s" };
            self.error(
                Code::E(761),
                format!(
                    "`{name}` expects {n} type argument{plural}, found {}",
                    tys.len()
                ),
                args_range,
            );
            return Resolved::Error;
        }
        if tys.iter().any(|(t, _)| t.is_error()) {
            return Resolved::Error;
        }
        // The candidates whose constraints the type arguments satisfy; with none, the first
        // one's diagnostics
        let check = |s: &mut Self, c: super::RoutineId| {
            let params = s.routines[c.0 as usize].type_params.clone();
            let def_name = s.routines[c.0 as usize].name.clone();
            let owner = s.routines[c.0 as usize].method_of;
            s.check_param_constraints(&params, &def_name, &tys, owner)
        };
        let mut ok: Vec<super::RoutineId> = if fitting.len() == 1 {
            fitting.clone()
        } else {
            fitting
                .iter()
                .copied()
                .filter(|c| self.dry_run(|s| check(s, *c)))
                .collect()
        };
        if ok.is_empty() {
            ok.push(fitting[0]);
        }
        if ok.len() == 1 && !check(self, ok[0]) {
            return Resolved::Error;
        }
        let origin = Some(self.span(args_range));
        let targs: Vec<Ty> = tys.iter().map(|(t, _)| *t).collect();
        let mut made = Vec::new();
        for c in ok {
            match self.with_inst(|i| i.instantiate_routine(c, targs.clone(), origin)) {
                Some(x) => made.push(x),
                None => return Resolved::Error,
            }
        }
        match r {
            Resolved::Routine(_) => {
                // The candidates of the call or value written here (5.4)
                self.explicit_sets
                    .insert(range.start(), (made[0], made.clone()));
                Resolved::Routine(made[0])
            }
            Resolved::Member {
                recv,
                class,
                member: super::methods::Member::Methods { name: n, .. },
                inherited,
            } => Resolved::Member {
                recv,
                class,
                member: super::methods::Member::Methods {
                    name: n,
                    inst: Some(made),
                },
                inherited,
            },
            _ => Resolved::Error,
        }
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// `KeyEquals<T>` / `KeyHash<T>` written with its type argument (`docs/spec/equality.md`
    /// chapter 4): E0761 for another number of type arguments.
    fn key_type_arg(
        &mut self,
        op: super::hir::KeyOp,
        name: &str,
        args: &ast::TypeArgs,
    ) -> super::Resolved {
        let list: Vec<ast::TypeSpec> = args.args().collect();
        if list.len() != 1 {
            for a in &list {
                self.resolve_type(Some(a.clone()));
            }
            self.error(
                Code::E(761),
                format!("`{name}` expects 1 type argument, found {}", list.len()),
                args.syntax().text_range(),
            );
            return super::Resolved::Error;
        }
        let t = self.resolve_type(list.into_iter().next());
        if t.is_error() {
            return super::Resolved::Error;
        }
        super::Resolved::Key(op, Some(t))
    }

    /// E0761 for `Zero<Integer> + 1`, read as the comparisons `(Zero < Integer) > (+1)`
    /// (`docs/spec/generics.md` 1.4): `A` names a generic routine and `B` a type, so the
    /// parentheses of the call were meant. The fix writes them after `>`. Returns whether it was
    /// reported.
    pub(super) fn generic_call_needs_parens(&mut self, b: &ast::BinaryExpr) -> bool {
        let Some(gt) = b.op().filter(|t| t.kind() == SyntaxKind::GT) else {
            return false;
        };
        let Some(ast::Expr::Binary(inner)) = b.lhs() else {
            return false;
        };
        if inner.op().is_none_or(|t| t.kind() != SyntaxKind::LT) {
            return false;
        }
        let (Some(ast::Expr::NameRef(a)), Some(ast::Expr::NameRef(t))) = (inner.lhs(), inner.rhs())
        else {
            return false;
        };
        let (Some(a_tok), Some(t_tok)) = (a.token(), t.token()) else {
            return false;
        };
        if a.type_args().is_some() || t.type_args().is_some() {
            return false;
        }
        let a_name = a_tok.text().to_string();
        let t_name = t_tok.text().to_string();
        let generic = match self.dry_run(|s| s.resolve_ident(&a_name, a_tok.text_range())) {
            super::Resolved::Routine(id) => self
                .routine_overloads(id)
                .iter()
                .any(|r| !self.routines[r.0 as usize].type_params.is_empty()),
            // `KeyEquals` / `KeyHash` are generic routines too (`docs/spec/equality.md` chapter 4)
            super::Resolved::Key(_, None) => true,
            _ => false,
        };
        if !generic {
            return false;
        }
        let is_type = t.is_string_keyword()
            || matches!(
                self.dry_run(|s| s.resolve_ident(&t_name, t_tok.text_range())),
                super::Resolved::Type(_)
            );
        if !is_type {
            return false;
        }
        let range = TextRange::new(a_tok.text_range().start(), gt.text_range().end());
        let text = self.src(range);
        let d = Diagnostic::error(
            Code::E(761),
            format!("`{text}` needs parentheses here"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "")
        .with_fix(Fix::safe(
            "add `()`",
            vec![TextEdit::insert(self.input.file, range.end(), "()")],
        ));
        self.push(d);
        true
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// The candidates of a routine name written with type arguments at `at`
    /// (`docs/spec/generics.md` 5.4), when `id` is the routine it resolved to.
    pub(super) fn explicit_set(
        &self,
        id: super::RoutineId,
        at: TextRange,
    ) -> Option<Vec<super::RoutineId>> {
        self.explicit_sets
            .get(&at.start())
            .filter(|(first, _)| *first == id)
            .map(|(_, set)| set.clone())
    }
}

/// The constructor `T.Create` calls for the class `class` (`docs/spec/generics.md` 3.1, 4.3): the
/// public `Create` found as the name is found (the nearest class declaring a method `Create`),
/// callable without arguments, with the default values of its parameters (the routine is `None`
/// for the built-in `TObject.Create`). `None` when the class has no such constructor (the
/// `constructor` constraint is not satisfied).
pub(super) fn default_ctor(
    types: &super::types::TypeTable,
    routines: &[super::RoutineInfo],
    class: TypeId,
) -> Option<(Option<super::hir::RoutineId>, Vec<super::hir::Expr>)> {
    let mut at = Some(class);
    while let Some(c) = at {
        let mut found = types
            .class_methods(c)
            .iter()
            .filter(|m| m.name.eq_ignore_ascii_case("create"))
            .peekable();
        if found.peek().is_some() {
            return found.find_map(|m| {
                if m.kind != super::types::MethodKind::Constructor || m.vis != Visibility::Public {
                    return None;
                }
                match m.routine {
                    Some(r) => {
                        let info = &routines[r.0 as usize];
                        (info.min_args() == 0).then(|| {
                            let defaults = info
                                .params
                                .iter()
                                .map(|p| p.default.clone().expect("every parameter has a default"))
                                .collect();
                            (Some(r), defaults)
                        })
                    }
                    None => m.params.is_empty().then(|| (None, Vec::new())),
                }
            });
        }
        at = types.parent(c);
    }
    None
}
