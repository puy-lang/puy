//! Interfaces (`docs/spec/objects.md` chapter 7): the declarations of interfaces and their
//! methods, the interfaces a class implements, and the class's interface table (which method of
//! the class implements each method of each interface).

use super::attrs::AttrSite;
use super::hir::RoutineId;
use super::types::{
    ClassMethod, IntfEntry, IntfImpl, MethodKind, SelfKind, Ty, TypeId, TypeKind, Visibility,
};
use super::{RoutineInfo, Sema};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::TextRange;
use crate::syntax::SyntaxKind;
use crate::syntax::ast::{self, AstNode};

/// A class of the `type` section being declared whose interface table waits for the methods of
/// the section (`docs/spec/objects.md` 7.3): the interfaces written in its parentheses and its
/// method resolution clauses.
pub(super) struct PendingImpl {
    pub class: TypeId,
    pub listed: Vec<(TypeId, TextRange)>,
    pub resolutions: Vec<ast::MethodResolution>,
}

/// A method resolution clause applied to one interface method: (the clause's position, the
/// interface declaring the method, the method number).
type ClauseKey = (TextRange, TypeId, Option<u32>);

/// The names of the methods the VM implements for every interface in Delphi, which Puy does
/// not let a program declare or call (E3017; `docs/spec/objects.md` 7.1, 7.2).
pub(super) fn is_ref_count_method(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "_addref" | "_release" | "queryinterface"
    )
}

/// The `notes` of E3017 (`docs/spec/diagnostics.md` 4.10).
pub(super) const REF_COUNT_NOTE: &str =
    "remove the declaration; Puy manages reference counts itself";

impl<'a, 's> Sema<'a, 's> {
    /// The interfaces in the parentheses of a class after its parent (`docs/spec/objects.md`
    /// 7.3): E0750 for a name that is not an interface and for one written twice.
    pub(super) fn listed_interfaces(&mut self, refs: &[ast::TypeSpec]) -> Vec<(TypeId, TextRange)> {
        let mut out: Vec<(TypeId, TextRange)> = Vec::new();
        for r in refs {
            let range = r.syntax().text_range();
            let t = self.resolve_type(Some(r.clone()));
            match t {
                Ty::Intf(i) if out.iter().any(|(o, _)| *o == i) => {
                    let n = self.tn(Ty::Intf(i));
                    self.error(
                        Code::E(750),
                        format!("interface `{n}` is listed more than once"),
                        range,
                    );
                }
                Ty::Intf(i) => out.push((i, range)),
                Ty::Error => {}
                // A class implementing an anonymous method type with `Invoke`
                // (`docs/spec/objects.md` 9.10)
                Ty::Callable(_)
                    if self.types.proc_family(t) == Some(super::types::ProcFamily::Reference) =>
                {
                    let n = self.tn(t);
                    self.error(
                        Code::E(3200),
                        format!(
                            "a class implementing the anonymous method type `{n}` is not supported yet (planned for a later release)"
                        ),
                        range,
                    )
                    .notes
                    .push(
                        "make a method reference to a method of the class, or an anonymous method that calls it"
                            .into(),
                    );
                }
                other => {
                    let n = self.tn(other);
                    self.error(Code::E(750), format!("`{n}` is not an interface"), range);
                }
            }
        }
        out
    }

    /// An interface (`docs/spec/objects.md` 7.1): its parent (`IInterface` when none is written
    /// or on an error), its GUID, and its members (methods and properties; the rest is E0750).
    /// The methods and properties are defined with the classes of the section.
    pub(super) fn define_interface(
        &mut self,
        id: TypeId,
        it: &ast::InterfaceType,
        parent: Option<Ty>,
    ) {
        let root = super::types::IINTERFACE;
        let parent_ref = it.parent_spec();
        let parent_ty = match (parent, &parent_ref) {
            (Some(t), _) => Some(t),
            (None, Some(p)) => Some(self.resolve_type(Some(p.clone()))),
            (None, None) => None,
        };
        let parent_id = match (parent_ty, &parent_ref) {
            (Some(Ty::Intf(p)), Some(r)) if self.types.is_descendant(p, id) => {
                let name = self.types.name(Ty::Intf(id));
                let span = self.types.get(id).span;
                let d = Diagnostic::error(
                    Code::E(727),
                    format!("type `{name}` inherits from itself"),
                    Stage::Sema,
                )
                .with_primary(self.span(span), "")
                .with_secondary(self.span(r.syntax().text_range()), "part of the cycle");
                self.push(d);
                root
            }
            (Some(Ty::Intf(p)), _) => p,
            (Some(Ty::Error) | None, _) => root,
            (Some(other), Some(r)) => {
                let n = self.tn(other);
                let range = r.syntax().text_range();
                self.error(Code::E(750), format!("`{n}` is not an interface"), range);
                root
            }
            (Some(_), None) => root,
        };
        let guid = it.guid().and_then(|g| self.interface_guid(&g));
        self.types.get_mut(id).kind = TypeKind::Interface {
            parent: Some(parent_id),
            unit: Some(self.unit_index),
            methods: Vec::new(),
            properties: Vec::new(),
            count: self.types.intf_method_count(parent_id),
            guid,
        };
        let mut methods = Vec::new();
        let mut properties = Vec::new();
        let members: Vec<ast::ClassMember> =
            it.body().map(|b| b.members().collect()).unwrap_or_default();
        let not_allowed = |s: &mut Self, what: String, range: TextRange| {
            s.error(
                Code::E(750),
                format!("`{what}` is not allowed in an interface"),
                range,
            );
        };
        for m in members {
            match m {
                ast::ClassMember::Method(m) => {
                    if !self.generic_interface_method(&m) {
                        methods.push((m, Visibility::Public));
                    }
                }
                ast::ClassMember::Property(p) if p.is_class() => {
                    let range = ast::trimmed_range(p.syntax());
                    let start = range.start();
                    let end = p.name().map_or(range.end(), |n| n.text_range().start());
                    let what = self.src(TextRange::new(start, end));
                    not_allowed(self, what.trim().to_ascii_lowercase(), range);
                }
                ast::ClassMember::Property(p) => properties.push((p, Visibility::Public)),
                ast::ClassMember::Visibility(v) => {
                    let range = ast::trimmed_range(v.syntax());
                    let what = self.src(range).to_ascii_lowercase();
                    not_allowed(self, what, range);
                }
                ast::ClassMember::Field(f) => {
                    let range = ast::trimmed_range(f.syntax());
                    self.error(Code::E(750), "interfaces cannot have fields", range);
                }
                ast::ClassMember::Var(v) => {
                    let range = ast::trimmed_range(v.syntax());
                    if v.is_class() {
                        not_allowed(self, "class var".into(), range);
                    } else {
                        self.error(Code::E(750), "interfaces cannot have fields", range);
                    }
                }
                ast::ClassMember::Const(c) => {
                    not_allowed(self, "const".into(), ast::trimmed_range(c.syntax()));
                }
                ast::ClassMember::Type(t) => {
                    not_allowed(self, "type".into(), ast::trimmed_range(t.syntax()));
                }
                ast::ClassMember::Resolution(r) => {
                    let range = ast::trimmed_range(r.syntax());
                    let what = self.src(range);
                    let what = what.trim_end_matches(';').trim_end().to_string();
                    not_allowed(self, what, range);
                }
            }
        }
        self.pending_classes.push((id, methods, properties));
    }

    /// The GUID of an interface (`docs/spec/objects.md` 7.1): a string constant of the form
    /// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` (E0750 otherwise). All zeros is the same as no
    /// GUID.
    fn interface_guid(&mut self, g: &ast::GuidSpec) -> Option<[u8; 16]> {
        let e = self.lower_expr(&g.expr()?);
        if e.ty.is_error() {
            return None;
        }
        let text = match &e.value {
            Some(super::types::Const::Str(units)) => String::from_utf16_lossy(units),
            Some(super::types::Const::Char(c)) => String::from_utf16_lossy(&[*c]),
            Some(_) if !matches!(e.ty, Ty::String | Ty::Char) => {
                let t = self.tn(e.ty);
                self.error(
                    Code::E(702),
                    format!("type mismatch: expected `string`, found `{t}`"),
                    e.span,
                );
                return None;
            }
            _ => {
                self.error(Code::E(713), "constant expression required", e.span);
                return None;
            }
        };
        match parse_guid(&text) {
            Some(g) if g == [0; 16] => None,
            Some(g) => Some(g),
            None => {
                self.error(Code::E(750), format!("invalid GUID `{text}`"), e.span);
                None
            }
        }
    }

    /// The methods of an interface (`docs/spec/objects.md` 7.1): instance method headings, each
    /// an abstract routine (no body) numbered after the parent's methods. Directives other than
    /// `overload` and the calling conventions, constructors, destructors and class methods are
    /// E0750; `_AddRef` / `_Release` / `QueryInterface` are E3017.
    pub(super) fn define_intf_methods(
        &mut self,
        id: TypeId,
        decls: Vec<(ast::MethodDecl, Visibility)>,
    ) {
        let parent = self.types.parent(id);
        let mut next = parent.map_or(0, |p| self.types.intf_method_count(p));
        let intf_name = self.types.name(Ty::Intf(id));
        let mut methods: Vec<ClassMethod> = Vec::new();
        for (decl, _) in decls {
            self.decl_attributes(decl.syntax(), AttrSite::Other, Ty::Error);
            let Some(header) = decl.header() else {
                continue;
            };
            let Some(first) = header.first_token() else {
                continue;
            };
            let Some(qname) = header.name() else { continue };
            let kind_tok = header
                .syntax()
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| t.kind() != SyntaxKind::CLASS_KW && !t.kind().is_trivia());
            let Some(kind_tok) = kind_tok else { continue };
            if kind_tok.kind() == SyntaxKind::OPERATOR_KW {
                // `docs/spec/objects.md` 11.1
                let range = TextRange::new(first.text_range().start(), kind_tok.text_range().end());
                self.operator_allowed(id, range);
                continue;
            }
            if first.kind() == SyntaxKind::CLASS_KW
                || matches!(
                    kind_tok.kind(),
                    SyntaxKind::CONSTRUCTOR_KW | SyntaxKind::DESTRUCTOR_KW
                )
            {
                let range = TextRange::new(first.text_range().start(), kind_tok.text_range().end());
                let what = self.src(range).to_ascii_lowercase();
                self.error(
                    Code::E(750),
                    format!("`{what}` is not allowed in an interface"),
                    range,
                );
                continue;
            }
            let name = qname.text();
            let name_range = qname.syntax().text_range();
            if is_ref_count_method(&name) {
                self.ref_count_decl(&decl, &name, name_range, None);
                continue;
            }
            let (params, ret) = self.heading_signature(&header);
            let overload = self.intf_directives(&decl);
            let sig: Vec<(Ty, ast::ParamMode)> = params.iter().map(|p| (p.ty, p.mode)).collect();
            // The same name: overloads with different parameters only (`docs/spec/semantics.md`
            // 4.7); an ancestor's method of the same name and parameters is E0701 (an interface
            // overrides nothing)
            let earlier = methods
                .iter()
                .filter(|m| m.name.eq_ignore_ascii_case(&name))
                .find(|m| {
                    !(overload
                        && self.routines[m.routine.expect("an interface method").0 as usize]
                            .overload
                        && m.params != sig)
                })
                .map(|m| self.span(m.span))
                .or_else(|| {
                    parent.and_then(|p| {
                        self.types
                            .intf_methods(p)
                            .into_iter()
                            .find(|(_, m)| m.name.eq_ignore_ascii_case(&name) && m.params == sig)
                            .and_then(|(owner, m)| self.method_span(owner, &m))
                    })
                });
            if let Some(prev) = earlier {
                let d = Diagnostic::error(
                    Code::E(701),
                    format!("`{name}` is already declared"),
                    Stage::Sema,
                )
                .with_primary(self.span(name_range), "declared again here")
                .with_secondary(prev, "declared here");
                self.push(d);
                continue;
            }
            self.declare_hints(decl.syntax(), name_range, format!("{intf_name}.{name}"));
            let routine = RoutineId(self.routines.len() as u32);
            self.routines.push(RoutineInfo {
                name: format!("{intf_name}.{name}"),
                params,
                ret,
                has_body: false,
                forward_span: None,
                implicit_forward: true,
                name_span: name_range,
                hir: None,
                overload,
                overload_fix_at: None,
                unit: self.unit_index,
                file: self.input.file,
                interface: self.in_interface_part,
                parent: None,
                calls: Vec::new(),
                method_of: Some(id),
                self_kind: SelfKind::Instance,
                is_abstract: true,
                body_span: None,
                closure: false,
                type_params: Vec::new(),
                closure_info: super::ClosureInfo::default(),
            });
            methods.push(ClassMethod {
                name,
                routine: Some(routine),
                span: name_range,
                vis: Visibility::Public,
                kind: if ret.is_some() {
                    MethodKind::Function
                } else {
                    MethodKind::Procedure
                },
                params: sig,
                ret,
                vmt: Some(next),
                is_override: false,
                is_abstract: true,
                is_final: false,
                self_kind: SelfKind::Instance,
            });
            next += 1;
        }
        if let TypeKind::Interface {
            methods: m, count, ..
        } = &mut self.types.get_mut(id).kind
        {
            *m = methods;
            *count = next;
        }
    }

    /// The directives of an interface method's heading (`docs/spec/objects.md` 7.1): the method
    /// directives are E0750, the others as for a routine (the calling conventions are W0009).
    /// Whether it is `overload`.
    fn intf_directives(&mut self, decl: &ast::MethodDecl) -> bool {
        let mut overload = false;
        for d in decl.directives() {
            let (Some(kind), Some(kw)) = (d.kind(), d.keyword()) else {
                continue;
            };
            match kind {
                SyntaxKind::VIRTUAL_KW
                | SyntaxKind::DYNAMIC_KW
                | SyntaxKind::ABSTRACT_KW
                | SyntaxKind::OVERRIDE_KW
                | SyntaxKind::REINTRODUCE_KW
                | SyntaxKind::FINAL_KW
                | SyntaxKind::STATIC_KW
                | SyntaxKind::MESSAGE_KW
                | SyntaxKind::IDENT => {
                    let what = self.text(&kw).to_ascii_lowercase();
                    self.error(
                        Code::E(750),
                        format!("`{what}` is not allowed in an interface"),
                        kw.text_range(),
                    );
                }
                SyntaxKind::OVERLOAD_KW => overload = true,
                SyntaxKind::FORWARD_KW | SyntaxKind::EXTERNAL_KW => {
                    let text = self.text(&kw).to_string();
                    self.error(
                        Code::E(401),
                        format!("unexpected `{text}`"),
                        kw.text_range(),
                    );
                }
                k => self.routine_directive(k, &kw, d.syntax()),
            }
        }
        overload
    }

    /// E3017 for a declaration of `_AddRef` / `_Release` / `QueryInterface` (`docs/spec/objects.md`
    /// 7.1): the fix removes the declaration and, for a class's method (`class` is its name), the
    /// implementation in the same file.
    pub(super) fn ref_count_decl(
        &mut self,
        decl: &ast::MethodDecl,
        name: &str,
        name_range: TextRange,
        class: Option<&str>,
    ) {
        let mut edits = vec![TextEdit::delete(
            self.input.file,
            crate::diagnostics::deletion_range(self.input.bytes, ast::trimmed_range(decl.syntax())),
        )];
        if let Some(class) = class {
            let full = format!("{class}.{name}");
            let root = decl.syntax().ancestors().last().expect("a node has a root");
            let body = root
                .descendants()
                .filter_map(ast::RoutineDecl::cast)
                .find(|r| {
                    r.header()
                        .and_then(|h| h.name())
                        .is_some_and(|n| n.text().eq_ignore_ascii_case(&full))
                });
            if let Some(b) = body {
                edits.push(TextEdit::delete(
                    self.input.file,
                    crate::diagnostics::deletion_range(
                        self.input.bytes,
                        ast::trimmed_range(b.syntax()),
                    ),
                ));
            }
        }
        let d = Diagnostic::error(
            Code::E(3017),
            format!("`{name}` is not supported in Puy"),
            Stage::Sema,
        )
        .with_primary(self.span(name_range), "")
        .with_note(REF_COUNT_NOTE)
        .with_fix(Fix::safe("remove the declaration", edits));
        self.push(d);
    }

    /// E0750 for a method resolution clause naming an interface the type does not implement
    /// (`docs/spec/objects.md` 7.1).
    pub(super) fn resolution_not_implemented(&mut self, r: &ast::MethodResolution, class: TypeId) {
        let Some(target) = r.target() else { return };
        let text = target.text();
        let intf = text.rsplit_once('.').map_or(text.as_str(), |(i, _)| i);
        let cname = self.types.name(Ty::Class(class));
        self.error(
            Code::E(750),
            format!("`{intf}` is not an interface implemented by `{cname}`"),
            target.syntax().text_range(),
        );
    }

    /// The interface tables of the classes of the section (`docs/spec/objects.md` 7.3), each
    /// after its parent's.
    pub(super) fn define_pending_impls(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_impls);
        while !pending.is_empty() {
            let i = pending
                .iter()
                .position(|p| {
                    let parent = self.types.parent(p.class).map(|c| self.types.layout(c));
                    !pending.iter().any(|q| Some(q.class) == parent)
                })
                .unwrap_or(0);
            let p = pending.remove(i);
            let class = p.class;
            self.class_intf_table(p);
            if self.types.is_generic(class) {
                // The classes of the section inheriting from its instantiations see its table
                self.with_inst(|i| i.refill(class));
            }
        }
    }

    /// The interface table of a class (`docs/spec/objects.md` 7.3): the parent's entries, then for
    /// each interface written in parentheses the interface and its ancestors (the root first),
    /// each method implemented by a method of the class. An interface the parent implements is
    /// implemented again with the class's methods.
    fn class_intf_table(&mut self, p: PendingImpl) {
        let class = p.class;
        let mut table: Vec<IntfImpl> = self
            .types
            .parent(class)
            .map(|c| self.types.class_intfs(c).to_vec())
            .unwrap_or_default();
        // The interfaces whose entries this class makes
        let mut own: Vec<TypeId> = Vec::new();
        for (intf, _) in &p.listed {
            let mut chain = self.types.class_chain(*intf);
            chain.reverse();
            for i in chain {
                if !own.contains(&i) {
                    own.push(i);
                }
            }
        }
        // The method resolution clauses: (interface, method name, class method name, clause)
        let mut clauses: Vec<(TypeId, String, String, TextRange)> = Vec::new();
        for r in &p.resolutions {
            if let Some(c) = self.resolution_clause(r, class, &own, &table) {
                clauses.push(c);
            }
        }
        let mut missing: Vec<(TypeId, String)> = Vec::new();
        // The method a clause gives each interface method, by (clause, declaring interface,
        // method number): a method reached through several entries (the interface and its
        // descendants) is looked up and reported once
        let mut resolved: Vec<(ClauseKey, Option<IntfEntry>)> = Vec::new();
        for i in own {
            let mut entries = Vec::new();
            for (owner, m) in self.types.intf_methods(i) {
                let clause = self.clause_for(&clauses, i, owner, &m.name);
                let found = match clause {
                    Some((_, _, target, range)) => {
                        let key = (*range, owner, m.vmt);
                        match resolved.iter().find(|(k, _)| *k == key) {
                            Some((_, e)) => *e,
                            None => {
                                let (target, range) = (target.clone(), *range);
                                let e = self.resolution_target(class, &m, &target, range);
                                resolved.push((key, e));
                                e
                            }
                        }
                    }
                    None => self.implementing_method(class, &m),
                };
                let entry = match found {
                    Some(e) => e,
                    None => {
                        if clause.is_none() && !missing.contains(&(owner, m.name.clone())) {
                            missing.push((owner, m.name.clone()));
                            self.not_implemented(class, owner, &m);
                        }
                        IntfEntry::Missing
                    }
                };
                entries.push(entry);
            }
            let item = IntfImpl { intf: i, entries };
            match table.iter_mut().find(|e| e.intf == i) {
                Some(e) => *e = item,
                None => table.push(item),
            }
        }
        if let TypeKind::Class { intfs, .. } = &mut self.types.get_mut(class).kind {
            *intfs = table;
        }
    }

    /// The method resolution clause that applies to the method `name` (declared by `owner`) in the
    /// entry of interface `i` (`docs/spec/objects.md` 7.8): a clause naming an interface on the
    /// same line of descent as `i` whose ancestors include `owner`. A clause naming `i` itself
    /// wins, then one naming the nearest ancestor of `i`, then the nearest descendant of `i`.
    fn clause_for<'c>(
        &self,
        clauses: &'c [(TypeId, String, String, TextRange)],
        i: TypeId,
        owner: TypeId,
        name: &str,
    ) -> Option<&'c (TypeId, String, String, TextRange)> {
        let distance = |ci: TypeId| {
            // How far `ci` is from `i` along the parents: 0 for `i`, then the ancestors of `i`,
            // then the descendants of `i`
            let up = self.types.class_chain(i).iter().position(|c| *c == ci);
            let down = self.types.class_chain(ci).iter().position(|c| *c == i);
            match (up, down) {
                (Some(k), _) => Some(k),
                (None, Some(k)) => Some(usize::MAX / 2 + k),
                (None, None) => None,
            }
        };
        clauses
            .iter()
            .filter(|(ci, n, _, _)| {
                n.eq_ignore_ascii_case(name) && self.types.is_descendant(*ci, owner)
            })
            .filter_map(|c| distance(c.0).map(|d| (d, c)))
            .min_by_key(|(d, _)| *d)
            .map(|(_, c)| c)
    }

    /// The method of a class implementing the interface method `m` (`docs/spec/objects.md` 7.3):
    /// an instance method (any visibility) of the same name, parameters (types and modes) and
    /// return type, the nearest first.
    fn implementing_method(&self, class: TypeId, m: &ClassMethod) -> Option<IntfEntry> {
        self.methods_named(class, &m.name)
            .into_iter()
            .find(|(_, c)| implements_method(c, m))
            .and_then(|(_, c)| method_entry(&c))
    }

    /// The method named by a method resolution clause for the interface method `m`: E0700 when
    /// the class has no method of the name, E0749 when none of them matches.
    fn resolution_target(
        &mut self,
        class: TypeId,
        m: &ClassMethod,
        target: &str,
        range: TextRange,
    ) -> Option<IntfEntry> {
        let named = self.methods_named(class, target);
        if named.is_empty() {
            let cname = self.types.name(Ty::Class(class));
            self.error(
                Code::E(700),
                format!("`{cname}` has no member `{target}`"),
                range,
            );
            return None;
        }
        let found = named
            .iter()
            .find(|(_, c)| implements_method(c, m))
            .and_then(|(_, c)| method_entry(c));
        if found.is_none() {
            let cname = self.types.name(Ty::Class(class));
            let owner = self.method_owner(m);
            let iname = self.types.name(Ty::Intf(owner));
            let mut d = Diagnostic::error(
                Code::E(749),
                format!("`{cname}` does not implement `{iname}.{}`", m.name),
                Stage::Sema,
            )
            .with_primary(self.span(range), "");
            if let Some(s) = self.method_span(owner, m) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
        }
        found
    }

    /// The interface declaring an interface method.
    fn method_owner(&self, m: &ClassMethod) -> TypeId {
        m.routine
            .and_then(|r| self.routines[r.0 as usize].method_of)
            .unwrap_or(super::types::IINTERFACE)
    }

    /// A method resolution clause of a class (`procedure IFoo.Run = DoRun;`): (the interface, the
    /// interface's method name, the class's method name, the class method's position), after
    /// checking that the interface is one the class implements and has the method.
    fn resolution_clause(
        &mut self,
        r: &ast::MethodResolution,
        class: TypeId,
        own: &[TypeId],
        table: &[IntfImpl],
    ) -> Option<(TypeId, String, String, TextRange)> {
        let target = r.target()?;
        let method = r.method()?;
        let text = target.text();
        let (intf_text, name) = text.rsplit_once('.')?;
        let lower = intf_text.to_ascii_lowercase();
        let implemented = |s: &Self, i: TypeId| {
            own.contains(&i) || table.iter().any(|e| e.intf == i) || s.types.implements(class, i)
        };
        let found =
            self.lookup_type_symbol(&lower).or_else(|| {
                match super::builtins::lookup_system(&lower) {
                    Some(super::builtins::SystemName::Type(t)) => Some(Some(t)),
                    _ => None,
                }
            });
        let range = target.syntax().text_range();
        let intf = match found {
            Some(Some(Ty::Intf(i))) if implemented(self, i) => i,
            Some(Some(Ty::Error)) => return None,
            Some(Some(Ty::Intf(_))) => {
                self.resolution_not_implemented(r, class);
                return None;
            }
            Some(_) => {
                self.error(
                    Code::E(750),
                    format!("`{intf_text}` is not an interface"),
                    range,
                );
                return None;
            }
            None => {
                self.error(
                    Code::E(700),
                    format!("undeclared identifier `{intf_text}`"),
                    range,
                );
                return None;
            }
        };
        if !self
            .types
            .intf_methods(intf)
            .iter()
            .any(|(_, m)| m.name.eq_ignore_ascii_case(name))
        {
            let iname = self.types.name(Ty::Intf(intf));
            self.error(
                Code::E(700),
                format!("`{iname}` has no member `{name}`"),
                range,
            );
            return None;
        }
        let target_name = self.text(&method).to_string();
        Some((intf, name.to_string(), target_name, method.text_range()))
    }

    /// E0749 for an interface method no method of the class implements (`docs/spec/objects.md`
    /// 7.3): at the class's name, with the interface method's declaration.
    fn not_implemented(&mut self, class: TypeId, owner: TypeId, m: &ClassMethod) {
        let cname = self.types.name(Ty::Class(class));
        let iname = self.types.name(Ty::Intf(owner));
        let span = self.types.get(class).span;
        let mut d = Diagnostic::error(
            Code::E(749),
            format!("`{cname}` does not implement `{iname}.{}`", m.name),
            Stage::Sema,
        )
        .with_primary(self.span(span), "");
        if let Some(s) = self.method_span(owner, m) {
            d = d.with_secondary(s, "declared here");
        }
        self.push(d);
    }
}

/// Whether the class method `c` can implement the interface method `m` (`docs/spec/objects.md`
/// 7.3): an instance method (not a constructor or destructor) with the same parameters and return
/// type.
fn implements_method(c: &ClassMethod, m: &ClassMethod) -> bool {
    c.self_kind == SelfKind::Instance
        && matches!(c.kind, MethodKind::Procedure | MethodKind::Function)
        && c.params == m.params
        && c.ret == m.ret
}

/// The interface table entry of a class method: its table entry if it is virtual, its routine
/// otherwise. A built-in method that is not virtual (`Free`) cannot implement one.
fn method_entry(c: &ClassMethod) -> Option<IntfEntry> {
    match (c.vmt, c.routine) {
        (Some(k), _) => Some(IntfEntry::Virtual(k)),
        (None, Some(r)) => Some(IntfEntry::Func(r)),
        (None, None) => None,
    }
}

/// The 16 bytes of a GUID written as `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` (any case): the
/// hexadecimal digits in pairs, in written order (`docs/spec/objects.md` 14.7).
pub(super) fn parse_guid(text: &str) -> Option<[u8; 16]> {
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let groups: Vec<&str> = inner.split('-').collect();
    let lens: Vec<usize> = groups.iter().map(|g| g.len()).collect();
    if lens != [8, 4, 4, 4, 12] {
        return None;
    }
    let hex: String = groups.concat();
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}
