//! Methods of classes (`docs/spec/objects.md` 2.3, 2.4): the declarations in the class body, the
//! virtual method table, the implementation headings (`procedure TFoo.Bar;`), and the lookup of
//! the members a method body and a call see.

use super::attrs::AttrSite;
use super::expr::err_expr;
use super::hir::*;
use super::overload::Pick;
use super::types::{ClassMethod, MethodKind, SelfKind, Ty, TypeId, TypeKind, Visibility, VmtEntry};
use super::{ParamInfo, RoutineInfo, Sema, directive_with_semicolon};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{Span, TextRange};
use crate::syntax::SyntaxKind;
use crate::syntax::ast::{self, AstNode, ParamMode};

/// The directives written after a method heading in a class body.
#[derive(Default)]
struct Directives {
    virtual_: Option<TextRange>,
    override_: Option<TextRange>,
    reintroduce: Option<(TextRange, crate::syntax::SyntaxNode)>,
    abstract_: Option<TextRange>,
    final_: Option<TextRange>,
    static_: Option<TextRange>,
    native: Option<TextRange>,
    overload: bool,
}

/// A member of a class found by name (`docs/spec/objects.md` 2.3, 2.5).
#[derive(Debug, Clone)]
pub(super) enum Member {
    /// A field: its slot, type and declared name.
    Field { slot: u32, ty: Ty, name: String },
    /// A field of a record (`docs/spec/objects.md` chapter 6): its number, type and declared
    /// name.
    RecField { index: u32, ty: Ty, name: String },
    /// Methods: the class declaring the nearest one, and the name as written in the call.
    /// `inst`: the instantiations of its generic methods for the type arguments written after
    /// the name (`docs/spec/generics.md` 1.4, 5.4), the candidates in place of the methods.
    Methods {
        name: String,
        inst: Option<Vec<RoutineId>>,
    },
    /// A member of `TObject` about classes whose body is the VM's (`docs/spec/objects.md`
    /// chapter 5).
    Builtin(ClassBuiltin),
    /// A `class var`, class constant or nested type (2.2), and the class declaring it.
    Static(TypeId, super::types::ClassStatic),
    /// A property (2.6), and the class declaring it.
    Property(TypeId, Box<super::types::ClassProperty>),
    /// `StackTrace` / `Suppressed` of an exception, which the VM keeps outside the slots
    /// (`docs/spec/objects.md` 8.2, 14.8).
    ExcInfo(ExcMember),
    /// `ToString` / `Length` of a built-in helper (`docs/spec/objects.md` 10.7).
    HelperBuiltin(super::types::BuiltinHelper),
}

/// The members of `TObject` about classes (`docs/spec/objects.md` chapter 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClassBuiltin {
    ClassName,
    ClassNameIs,
    ClassParent,
    InheritsFrom,
    ClassType,
    /// `TRefObject.RefCount` (only for the reference counted classes).
    RefCount,
}

impl ClassBuiltin {
    fn from_name(name: &str) -> Option<ClassBuiltin> {
        Some(match name.to_ascii_lowercase().as_str() {
            "classname" => ClassBuiltin::ClassName,
            "classnameis" => ClassBuiltin::ClassNameIs,
            "classparent" => ClassBuiltin::ClassParent,
            "inheritsfrom" => ClassBuiltin::InheritsFrom,
            "classtype" => ClassBuiltin::ClassType,
            "refcount" => ClassBuiltin::RefCount,
            _ => return None,
        })
    }
}

/// Whether a name is one of `TObject`'s members about classes (`docs/spec/objects.md` chapter 5).
pub(super) fn is_class_builtin(name: &str) -> bool {
    ClassBuiltin::from_name(name).is_some()
}

/// The methods named `name` that a class has, nearest declaration first: those of the class,
/// then those of its ancestors that are not overridden by a nearer one.
pub(super) fn methods_named(
    types: &super::types::TypeTable,
    class: TypeId,
    name: &str,
) -> Vec<(TypeId, ClassMethod)> {
    let mut out: Vec<(TypeId, ClassMethod)> = Vec::new();
    for c in types.class_chain(class) {
        for m in types.class_methods(c) {
            // An operator is not reached by its name (`docs/spec/objects.md` 11.1)
            if !m.name.eq_ignore_ascii_case(name) || m.kind == MethodKind::Operator {
                continue;
            }
            // An overridden method is reached through its override
            if m.vmt.is_some() && out.iter().any(|(_, n)| n.vmt == m.vmt && n.is_override) {
                continue;
            }
            out.push((c, m.clone()));
        }
    }
    out
}

/// The methods named `name` of a class that `visible` lets the code see (by the visibility and
/// the class declaring the method), for choosing among overloads: the routine of each, nearest
/// first. Built-in methods (`ToString` of `TObject`) have no routine and are left out.
///
/// A class's methods hide those of its ancestors with the same name unless they are all
/// `overload`, and a method hides an ancestor's with the same parameters (as Delphi does).
pub(super) fn method_set(
    types: &super::types::TypeTable,
    routines: &[RoutineInfo],
    class: TypeId,
    name: &str,
    visible: impl Fn(Visibility, TypeId) -> bool,
) -> Vec<RoutineId> {
    let mut out: Vec<(TypeId, ClassMethod)> = Vec::new();
    for (c, m) in methods_named(types, class, name) {
        if !visible(m.vis, c) {
            continue;
        }
        if let Some((last, _)) = out.last()
            && *last != c
            && !out
                .iter()
                .all(|(_, n)| n.routine.is_some_and(|r| routines[r.0 as usize].overload))
        {
            break;
        }
        if out
            .iter()
            .any(|(_, n)| same_signature(&n.params, &m.params))
        {
            continue;
        }
        out.push((c, m));
    }
    out.into_iter().filter_map(|(_, m)| m.routine).collect()
}

/// Whether two parameter lists are the same for overriding and for an implementation heading:
/// the count, the types and the modes.
fn same_signature(a: &[(Ty, ParamMode)], b: &[(Ty, ParamMode)]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}

fn signature(params: &[ParamInfo]) -> Vec<(Ty, ParamMode)> {
    params.iter().map(|p| (p.ty, p.mode)).collect()
}

impl<'a, 's> Sema<'a, 's> {
    /// The methods and virtual method tables of the classes of the section just declared, each
    /// after its parent.
    pub(super) fn define_pending_methods(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_classes);
        while !pending.is_empty() {
            // A class whose parent is not pending any more (and a helper whose type is not:
            // `docs/spec/objects.md` 10.3 compares with the type's members)
            let i = pending
                .iter()
                .position(|(id, _, _)| {
                    let parent = self.types.parent(*id);
                    let target = self.types.helper_target(*id).and_then(|t| match t {
                        Ty::Class(c) | Ty::Record(c) => Some(c),
                        _ => None,
                    });
                    // An instantiation waits for its definition (`docs/spec/generics.md` 2.4)
                    let parent = parent.map(|p| self.types.layout(p));
                    !pending
                        .iter()
                        .any(|(p, _, _)| Some(*p) == parent || Some(*p) == target)
                })
                .unwrap_or(0);
            let (id, decls, props) = pending.remove(i);
            // The members are inside the class's declaration (`docs/spec/attributes.md` 6.2)
            let scope = self.class_decl_span(id);
            self.begin_names_hints(scope);
            // The headings see the class's constants and nested types (2.2)
            self.class_scopes.push(super::statics::ClassScope {
                class: id,
                vis: Visibility::Public,
                names: Vec::new(),
            });
            if self.types.is_interface(id) {
                self.with_type_params(id, |s| {
                    s.define_intf_methods(id, decls);
                    s.define_properties(id, props);
                });
            } else {
                self.define_methods(id, decls);
                self.define_properties(id, props);
                if self.input.extension.is_some() {
                    let at = self.types.get(id).span;
                    self.check_extension_class(id, at);
                }
            }
            if self.types.is_helper(id) {
                self.check_helper_members(id);
            }
            self.class_scopes.pop();
            self.end_names_hints(scope);
            if self.types.is_generic(id) {
                // The classes of the section inheriting from its instantiations see its methods
                self.with_inst(|i| i.refill(id));
            }
        }
        self.define_pending_impls();
    }

    /// The methods of a class (`docs/spec/objects.md` 2.3, 2.4): a routine per method, and the
    /// virtual method table (the parent's, with the overridden entries replaced and the new
    /// virtual methods after it).
    fn define_methods(&mut self, id: TypeId, decls: Vec<(ast::MethodDecl, Visibility)>) {
        let parent = self.types.parent(id);
        let mut vmt: Vec<VmtEntry> = parent
            .map(|p| self.types.class_vmt(p).to_vec())
            .unwrap_or_default();
        let class_name = self.types.name(Ty::Class(id));
        let is_record = self.types.is_record(id);
        let is_helper = self.types.is_helper(id);
        // A record helper's class methods are static, as a record's (`docs/spec/objects.md` 10.2)
        let no_class_ref = is_record || (is_helper && self.types.self_by_ref(id));
        let mut methods: Vec<ClassMethod> = Vec::new();
        // The `override` declarations of `Equals` and `GetHashCode` (E0765): (whether it
        // overrides the method of `TObject`, the name, the declaration)
        let mut eq_hash: [Option<(bool, TextRange, TextRange)>; 2] = [None, None];
        // Each declared method's name and the range of its declaration (for the fixes of
        // `check_record_hash`)
        let mut decl_ranges: Vec<(TextRange, TextRange)> = Vec::new();
        for (decl, vis) in decls {
            self.decl_attributes(decl.syntax(), AttrSite::Other, Ty::Error);
            let Some(header) = decl.header() else {
                continue;
            };
            let Some(first) = header.first_token() else {
                continue;
            };
            let Some(qname) = header.name() else { continue };
            let is_class = first.kind() == SyntaxKind::CLASS_KW;
            let kind_tok = if is_class {
                header
                    .syntax()
                    .children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() != SyntaxKind::CLASS_KW && !t.kind().is_trivia())
            } else {
                Some(first.clone())
            };
            let Some(kind_tok) = kind_tok else { continue };
            let tparams = header.type_params();
            // A helper reports these as members it cannot have (`docs/spec/objects.md` 10.2,
            // 11.1) whether or not they have type parameters (`docs/spec/generics.md` 2.1)
            if tparams.is_some()
                && !is_helper
                && matches!(
                    kind_tok.kind(),
                    SyntaxKind::CONSTRUCTOR_KW
                        | SyntaxKind::DESTRUCTOR_KW
                        | SyntaxKind::OPERATOR_KW
                )
            {
                // `docs/spec/generics.md` 2.1
                let what = match kind_tok.kind() {
                    SyntaxKind::CONSTRUCTOR_KW => "a constructor",
                    SyntaxKind::DESTRUCTOR_KW => "a destructor",
                    _ => "an operator",
                };
                self.error(
                    Code::E(757),
                    format!("{what} cannot have type parameters"),
                    kind_tok.text_range(),
                );
                continue;
            }
            if is_class
                && matches!(
                    kind_tok.kind(),
                    SyntaxKind::CONSTRUCTOR_KW | SyntaxKind::DESTRUCTOR_KW
                )
            {
                // `docs/spec/objects.md` 2.10: `initialization` / `finalization` instead
                let what = self.text(&kind_tok).to_ascii_lowercase();
                self.error(
                    Code::E(3200),
                    format!("`class {what}` is not supported yet (planned for a later release)"),
                    TextRange::new(first.text_range().start(), kind_tok.text_range().end()),
                );
                continue;
            }
            if is_helper
                && matches!(
                    kind_tok.kind(),
                    SyntaxKind::CONSTRUCTOR_KW | SyntaxKind::DESTRUCTOR_KW
                )
            {
                // `docs/spec/objects.md` 10.2
                let what = self.text(&kind_tok).to_ascii_lowercase();
                self.not_in_helper(&what, kind_tok.text_range());
                continue;
            }
            let is_operator = is_class && kind_tok.kind() == SyntaxKind::OPERATOR_KW;
            if is_operator
                && !self.operator_allowed(
                    id,
                    TextRange::new(first.text_range().start(), kind_tok.text_range().end()),
                )
            {
                continue;
            }
            let name = qname.text();
            let name_range = qname.syntax().text_range();
            if !is_operator && super::interfaces::is_ref_count_method(&name) {
                // `docs/spec/objects.md` 7.1
                self.ref_count_decl(&decl, &name, name_range, Some(&class_name));
                continue;
            }
            let first_diag = self.sink.len();
            // The type parameters of a generic method (`docs/spec/generics.md` 1.2, 2.2)
            let own = match &tparams {
                Some(tp) => self.declare_routine_type_params(tp, Some(id)),
                None => Vec::new(),
            };
            let mark = self.push_routine_type_params(&own);
            let (params, ret) = self.heading_signature(&header);
            self.pop_routine_type_params(mark);
            let mut dirs = self.method_directives(&decl);
            if tparams.is_some()
                && !is_helper
                && let Some(v) = dirs.virtual_.or(dirs.override_).or(dirs.abstract_)
            {
                // A virtual method needs an entry of the table for each type argument
                // (`docs/spec/generics.md` 2.1)
                self.error(
                    Code::E(757),
                    "a virtual method cannot have type parameters",
                    v,
                );
                continue;
            }
            let kind = match kind_tok.kind() {
                SyntaxKind::CONSTRUCTOR_KW => MethodKind::Constructor,
                SyntaxKind::DESTRUCTOR_KW => MethodKind::Destructor,
                SyntaxKind::OPERATOR_KW => MethodKind::Operator,
                _ if header.is_function() => MethodKind::Function,
                _ => MethodKind::Procedure,
            };
            if is_operator {
                self.operator_directives(&mut dirs);
                if !self.check_operator_decl(id, &name, name_range, &params, ret) {
                    continue;
                }
            }
            if is_record
                && !self.record_method_allowed(kind, &params, &mut dirs, &kind_tok, name_range)
            {
                continue;
            }
            if is_helper {
                self.helper_method_directives(&mut dirs);
            }
            let self_kind = match (is_class, dirs.static_) {
                // An operator is static (`docs/spec/objects.md` 11.1)
                _ if is_operator => SelfKind::Static,
                (true, Some(_)) => SelfKind::Static,
                (true, None) if no_class_ref => {
                    // `docs/spec/objects.md` chapter 6: a record has no class reference
                    self.error(
                        Code::E(744),
                        "a class method of a record must be `static`",
                        name_range,
                    );
                    SelfKind::Static
                }
                (true, None) => SelfKind::Class,
                (false, _) => SelfKind::Instance,
            };
            if self_kind == SelfKind::Class {
                // `Self` of a class method is the class reference
                self.types.class_ref(
                    id,
                    super::types::DynWritten::Anonymous {
                        generic: false,
                        span: name_range,
                    },
                );
            }
            let bad_dtor = kind == MethodKind::Destructor
                && !(name.eq_ignore_ascii_case("Destroy")
                    && params.is_empty()
                    && dirs.override_.is_some()
                    && dirs.virtual_.is_none()
                    && dirs.reintroduce.is_none()
                    && dirs.abstract_.is_none());
            if bad_dtor {
                // The VM calls only `Destroy` (`docs/spec/objects.md` 3.2)
                self.error(
                    Code::E(747),
                    "a destructor must be declared as `destructor Destroy; override;`",
                    name_range,
                );
                dirs = Directives {
                    overload: dirs.overload,
                    ..Directives::default()
                };
            }
            // Reported but still declared, so the body finds its declaration
            self.check_reserved_member_name(&name.to_ascii_lowercase(), name_range);
            let sig = signature(&params);
            // The same name in the class: only overloads with different parameters
            // (`docs/spec/semantics.md` 4.7), and not a field's name
            let field = (!is_operator)
                .then(|| {
                    self.types
                        .class_fields(id)
                        .iter()
                        .find(|(owner, f)| *owner == id && f.name.eq_ignore_ascii_case(&name))
                        .map(|(_, f)| f.span)
                        .or_else(|| {
                            self.types
                                .fields(Ty::Record(id))
                                .iter()
                                .filter(|_| is_record)
                                .find(|f| f.name.eq_ignore_ascii_case(&name))
                                .map(|f| f.span)
                        })
                        .or_else(|| {
                            // A type parameter (`docs/spec/generics.md` 2.2), the innermost
                            // one of the name (a nested type's own follow the outer type's)
                            self.types
                                .type_params(id)
                                .iter()
                                .rev()
                                .map(|p| self.types.get(*p))
                                .find(|p| {
                                    p.name
                                        .as_deref()
                                        .is_some_and(|n| n.eq_ignore_ascii_case(&name))
                                })
                                .map(|p| p.span)
                        })
                })
                .flatten();
            // Operators of one name need no `overload`; the same parameters (and, for
            // `Implicit` / `Explicit`, the same result) are E0701 (`docs/spec/objects.md` 11.1)
            if is_operator
                && let Some(prev) = methods.iter().find(|m| {
                    m.kind == MethodKind::Operator
                        && super::operators::same_operator(&m.name, &name)
                        && super::operators::same_operands(&m.params, &sig)
                        && (m.ret == ret || !super::operators::is_conversion(&name))
                })
            {
                let d = Diagnostic::error(
                    Code::E(701),
                    format!("overloads of operator `{name}` have the same parameters"),
                    Stage::Sema,
                )
                .with_primary(self.span(name_range), "declared again here")
                .with_secondary(self.span(prev.span), "declared here");
                self.push(d);
                continue;
            }
            let earlier = methods
                .iter()
                .filter(|m| {
                    !is_operator
                        && m.kind != MethodKind::Operator
                        && m.name.eq_ignore_ascii_case(&name)
                })
                .find(|m| {
                    !(dirs.overload
                        && self.routines[m.routine.expect("a declared method").0 as usize].overload
                        && !same_signature(&m.params, &sig))
                })
                .map(|m| m.span);
            if let Some(prev) = field.or(earlier) {
                let d = Diagnostic::error(
                    Code::E(701),
                    format!("`{name}` is already declared"),
                    Stage::Sema,
                )
                .with_primary(self.span(name_range), "declared again here")
                .with_secondary(self.span(prev), "declared here");
                self.push(d);
                continue;
            }
            // The combinations of the directives (`docs/spec/objects.md` 2.3, 2.4)
            let later = |a: TextRange, b: TextRange| if a.start() > b.start() { a } else { b };
            let bad = |s: &mut Self, msg: &str, at: TextRange| {
                s.error(Code::E(744), msg.to_string(), at);
            };
            if let Some(r) = dirs.static_ {
                if self_kind != SelfKind::Static {
                    bad(self, "`static` can only be applied to class methods", r);
                } else if let Some(v) = dirs
                    .virtual_
                    .or(dirs.override_)
                    .or(dirs.abstract_)
                    .or(dirs.final_)
                {
                    // A static method has no `Self` to find a table with
                    bad(self, "a static method cannot be virtual", later(r, v));
                    dirs.virtual_ = None;
                    dirs.override_ = None;
                    dirs.abstract_ = None;
                    dirs.final_ = None;
                }
            }
            if let (Some(v), Some(o)) = (dirs.virtual_, dirs.override_) {
                let text = self.src(v).to_ascii_lowercase();
                bad(
                    self,
                    &format!("a method cannot be both `{text}` and `override`"),
                    later(v, o),
                );
            }
            if let (Some(o), Some((r, _))) = (dirs.override_, &dirs.reintroduce) {
                bad(
                    self,
                    "a method cannot be both `override` and `reintroduce`",
                    later(o, *r),
                );
            }
            let is_virtual = dirs.virtual_.is_some() || dirs.override_.is_some();
            if let Some(f) = dirs.final_
                && !is_virtual
            {
                bad(self, "`final` can only be applied to virtual methods", f);
            }
            let mut is_abstract = false;
            if let Some(a) = dirs.abstract_ {
                if let Some(o) = dirs.override_ {
                    bad(
                        self,
                        "an overriding method cannot be `abstract`",
                        later(a, o),
                    );
                } else if dirs.virtual_.is_none() {
                    bad(self, "an abstract method must be `virtual`", a);
                } else {
                    is_abstract = true;
                }
            }
            // A `native` method (`docs/spec/extensions.md` 2.2): not virtual, and of a class
            let native = match dirs.native {
                Some(at)
                    if is_virtual
                        || is_record
                        || is_helper
                        || is_operator
                        || kind == MethodKind::Destructor =>
                {
                    self.error(Code::E(773), "this method cannot be `native`", at)
                        .notes
                        .push("a `native` method is a static or class method, or a constructor, of a class".into());
                    false
                }
                Some(at) => self.native_allowed(at, false),
                None => false,
            };
            // A declaration with an error does not also need a body (E0723)
            let failed = self.sink.iter().skip(first_diag).any(|d| d.is_error());
            self.declare_hints(decl.syntax(), name_range, format!("{class_name}.{name}"));
            let info = RoutineInfo {
                name: format!("{class_name}.{name}"),
                params: params.clone(),
                ret,
                has_body: false,
                forward_span: None,
                implicit_forward: failed || native,
                name_span: name_range,
                hir: None,
                overload: dirs.overload,
                overload_fix_at: None,
                unit: self.unit_index,
                file: self.input.file,
                interface: self.in_interface_part,
                parent: None,
                scope: self.cur.as_ref().and_then(|c| c.routine),
                calls: Vec::new(),
                method_of: Some(id),
                self_kind,
                is_abstract,
                native,
                body_span: None,
                closure: false,
                type_params: own,
                closure_info: super::ClosureInfo::default(),
            };
            // A method of a local type declared again by a trial check keeps its routine
            let routine = self.routine_at(name_range, |_, _| (0, info)).1;
            if native {
                let ctor = (kind == MethodKind::Constructor).then_some(id);
                self.check_native_signature(routine, ctor, name_range);
            }
            let entry = if is_abstract {
                VmtEntry::Abstract
            } else {
                VmtEntry::Func(routine)
            };
            // The nearest ancestor's methods of the name
            let ancestors: Vec<(TypeId, ClassMethod)> = match parent {
                Some(p) if !is_operator => self.methods_named(p, &name),
                _ => Vec::new(),
            };
            let mut slot = None;
            if dirs.override_.is_some() {
                let parent_name = parent
                    .map(|p| self.types.name(Ty::Class(p)))
                    .unwrap_or_default();
                slot = self.check_override(
                    &name,
                    &parent_name,
                    &sig,
                    ret,
                    (kind, self_kind),
                    vis,
                    name_range,
                    header.return_type().map(|t| t.syntax().text_range()),
                    &ancestors,
                );
                let lower = name.to_ascii_lowercase();
                let which = match lower.as_str() {
                    "equals" => Some((0, super::types::VMT_EQUALS)),
                    "gethashcode" => Some((1, super::types::VMT_GET_HASH_CODE)),
                    _ => None,
                };
                if let Some((i, k)) = which {
                    // Prefer the declaration that overrides the method of `TObject` (another
                    // overload of the name may come first)
                    let hit = slot == Some(k);
                    if eq_hash[i].is_none_or(|(prev, _, _)| !prev && hit) {
                        eq_hash[i] = Some((hit, name_range, decl.syntax().text_range()));
                    }
                }
            } else if dirs.virtual_.is_some() {
                slot = Some(vmt.len() as u32);
                vmt.push(entry);
            }
            if dirs.override_.is_none() && !bad_dtor {
                // Hiding a virtual method of an ancestor (W0021), and a `reintroduce` that hides
                // nothing (W0026)
                let hidden = ancestors.iter().find(|(_, m)| m.vmt.is_some());
                match (&dirs.reintroduce, hidden) {
                    (None, Some((owner, m))) => {
                        let same = same_signature(&m.params, &sig) && m.ret == ret;
                        self.hidden_virtual(&decl, &name, *owner, name_range, same);
                    }
                    (Some((r, node)), _) if ancestors.is_empty() => {
                        let del = crate::diagnostics::deletion_range(
                            self.input.bytes,
                            directive_with_semicolon(node),
                        );
                        let d = Diagnostic::warning(
                            Code::W(26),
                            format!(
                                "`reintroduce` has no effect: no ancestor of `{class_name}` declares `{name}`"
                            ),
                            Stage::Sema,
                        )
                        .with_primary(self.span(*r), "")
                        .with_fix(Fix::safe(
                            "remove `reintroduce`",
                            vec![TextEdit::delete(self.input.file, del)],
                        ));
                        self.push(d);
                    }
                    _ => {}
                }
            }
            if self.sink.iter().skip(first_diag).any(|d| d.is_error()) {
                self.routines[routine.0 as usize].implicit_forward = true;
            }
            if let Some(k) = slot
                && dirs.override_.is_some()
                && let Some(e) = vmt.get_mut(k as usize)
            {
                *e = entry;
            }
            decl_ranges.push((name_range, crate::syntax::ast::trimmed_range(decl.syntax())));
            self.types.decl_params.insert(
                routine,
                params.iter().map(|p| (p.name.clone(), p.span)).collect(),
            );
            methods.push(ClassMethod {
                name,
                routine: Some(routine),
                span: name_range,
                vis,
                kind,
                params: sig,
                ret,
                vmt: slot,
                is_override: dirs.override_.is_some() && slot.is_some(),
                is_abstract,
                is_final: dirs.final_.is_some() && slot.is_some(),
                self_kind,
            });
        }
        let equality = if is_helper {
            super::keys::EqualityDecl::Plain
        } else {
            self.equality_decl(id, is_record, &methods, eq_hash)
        };
        match equality {
            super::keys::EqualityDecl::Errored => {}
            super::keys::EqualityDecl::Generate => {
                self.declare_value_equality(id, &mut methods, &mut vmt)
            }
            super::keys::EqualityDecl::Plain => {
                let own_equal = !is_record
                    && !is_helper
                    && self.check_class_equal(id, &mut methods, &mut vmt, eq_hash, &decl_ranges);
                if !own_equal {
                    self.equals_without_hash(&class_name, eq_hash);
                }
                if is_record {
                    self.check_record_hash(id, &methods, &decl_ranges);
                }
            }
        }
        match &mut self.types.get_mut(id).kind {
            TypeKind::Class {
                methods: m, vmt: v, ..
            } => {
                *m = methods;
                *v = vmt;
            }
            TypeKind::Record { methods: m, .. } | TypeKind::Helper { methods: m, .. } => {
                *m = methods
            }
            _ => {}
        }
    }

    /// The directives an operator cannot have (`docs/spec/objects.md` 11.1): the virtual ones
    /// (E0744; dropped). `static` and `overload` may be written.
    fn operator_directives(&mut self, dirs: &mut Directives) {
        let virtual_ = [
            dirs.virtual_,
            dirs.override_,
            dirs.reintroduce.as_ref().map(|(r, _)| *r),
            dirs.abstract_,
            dirs.final_,
        ];
        for r in virtual_.into_iter().flatten() {
            self.operator_directive(r);
        }
        *dirs = Directives {
            static_: dirs.static_,
            overload: dirs.overload,
            ..Directives::default()
        };
    }

    /// The virtual directives a helper's method cannot have (`docs/spec/objects.md` 10.2: a helper
    /// has no virtual method table): E0753, and the directives are dropped.
    fn helper_method_directives(&mut self, dirs: &mut Directives) {
        let virtual_ = [
            dirs.virtual_,
            dirs.override_,
            dirs.reintroduce.as_ref().map(|(r, _)| *r),
            dirs.abstract_,
            dirs.final_,
        ];
        for r in virtual_.into_iter().flatten() {
            let what = self.src(r).to_ascii_lowercase();
            self.not_in_helper(&what, r);
        }
        *dirs = Directives {
            static_: dirs.static_,
            overload: dirs.overload,
            ..Directives::default()
        };
    }

    /// What a record cannot declare (`docs/spec/objects.md` chapter 6): a destructor (the method
    /// is dropped; `false`), virtual methods (the directives are dropped), and a constructor
    /// without parameters (E0747; kept).
    fn record_method_allowed(
        &mut self,
        kind: MethodKind,
        params: &[ParamInfo],
        dirs: &mut Directives,
        kind_tok: &crate::syntax::SyntaxToken,
        name_range: TextRange,
    ) -> bool {
        if kind == MethodKind::Destructor {
            self.error(
                Code::E(744),
                "records cannot have destructors",
                kind_tok.text_range(),
            );
            return false;
        }
        let virtual_ = [
            dirs.virtual_,
            dirs.override_,
            dirs.reintroduce.as_ref().map(|(r, _)| *r),
            dirs.abstract_,
            dirs.final_,
        ]
        .into_iter()
        .flatten()
        .min_by_key(|r| r.start());
        if let Some(r) = virtual_ {
            self.error(Code::E(744), "records cannot have virtual methods", r);
            *dirs = Directives {
                static_: dirs.static_,
                overload: dirs.overload,
                ..Directives::default()
            };
        }
        if kind == MethodKind::Constructor && params.is_empty() {
            self.error(
                Code::E(747),
                "a record constructor must have parameters",
                name_range,
            );
        }
        true
    }

    /// The directives of a method declaration: the method directives are collected, the routine
    /// directives are checked as for a routine, and `message` / `dispid` / `forward` / `external`
    /// are diagnosed.
    fn method_directives(&mut self, decl: &ast::MethodDecl) -> Directives {
        let mut dirs = Directives::default();
        for d in decl.directives() {
            let (Some(kind), Some(kw)) = (d.kind(), d.keyword()) else {
                continue;
            };
            let range = kw.text_range();
            match kind {
                SyntaxKind::VIRTUAL_KW | SyntaxKind::DYNAMIC_KW => {
                    dirs.virtual_.get_or_insert(range);
                }
                SyntaxKind::OVERRIDE_KW => {
                    dirs.override_.get_or_insert(range);
                }
                SyntaxKind::REINTRODUCE_KW => {
                    dirs.reintroduce = Some((range, d.syntax().clone()));
                }
                SyntaxKind::ABSTRACT_KW => {
                    dirs.abstract_.get_or_insert(range);
                }
                SyntaxKind::FINAL_KW => {
                    dirs.final_.get_or_insert(range);
                }
                SyntaxKind::STATIC_KW => {
                    dirs.static_.get_or_insert(range);
                }
                SyntaxKind::OVERLOAD_KW => dirs.overload = true,
                SyntaxKind::NATIVE_KW => {
                    dirs.native.get_or_insert(range);
                }
                SyntaxKind::MESSAGE_KW => self.message_unsupported("message", range),
                // `dispid` (COM)
                SyntaxKind::IDENT => {
                    self.error(Code::E(3010), "`dispid` is not supported in Puy", range)
                        .notes
                        .push("Puy does not support COM".into());
                }
                SyntaxKind::FORWARD_KW | SyntaxKind::EXTERNAL_KW => {
                    let text = self.text(&kw).to_string();
                    self.error(Code::E(401), format!("unexpected `{text}`"), range);
                }
                k => self.routine_directive(k, &kw, d.syntax()),
            }
        }
        dirs
    }

    /// The table entry an `override` replaces (`docs/spec/objects.md` 2.4): an ancestor's virtual
    /// method of the same name, parameters, return type and kind, not `final`, and not more
    /// visible than the override. `None` (with E0744 / E0745 / E3200) otherwise.
    #[allow(clippy::too_many_arguments)]
    fn check_override(
        &mut self,
        name: &str,
        parent: &str,
        sig: &[(Ty, ParamMode)],
        ret: Option<Ty>,
        kind: (MethodKind, SelfKind),
        vis: Visibility,
        name_range: TextRange,
        ret_range: Option<TextRange>,
        ancestors: &[(TypeId, ClassMethod)],
    ) -> Option<u32> {
        let (kind, self_kind) = kind;
        let found = ancestors.iter().find(|(_, m)| {
            m.vmt.is_some()
                && same_signature(&m.params, sig)
                && m.ret == ret
                && m.kind == kind
                && m.self_kind == self_kind
        });
        let Some((owner, m)) = found else {
            let mut d = Diagnostic::error(
                Code::E(744),
                format!("`{name}` does not override a virtual method of `{parent}`"),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "");
            if let Some((owner, m)) = ancestors.first()
                && let Some(s) = self.method_span(*owner, m)
            {
                d = d.with_secondary(s, "declared here");
            }
            if name.eq_ignore_ascii_case("GetHashCode")
                && sig.is_empty()
                && ret.is_some_and(|t| t.is_integer() && t != Ty::Int64)
                && kind == MethodKind::Function
                && self_kind == SelfKind::Instance
                && let Some(r) = ret_range
            {
                // `function GetHashCode: Integer; override;` written for another dialect
                // (`docs/spec/equality.md` 1.1); other integer types too, but not a type whose
                // intent cannot be read (`string`, a record)
                d.notes.push("`GetHashCode` returns `Int64` in Puy".into());
                d = d.with_fix(Fix::safe(
                    "change the return type to `Int64`",
                    vec![TextEdit::replace(self.input.file, r, "Int64")],
                ));
            }
            self.push(d);
            return None;
        };
        let owner_name = self.types.name(Ty::Class(*owner));
        if m.is_final {
            let mut d = Diagnostic::error(
                Code::E(745),
                format!("cannot override final method `{owner_name}.{}`", m.name),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "");
            if let Some(s) = self.method_span(*owner, m) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            return None;
        }
        if vis.access_rank() < m.vis.access_rank() {
            let mut d = Diagnostic::error(
                Code::E(744),
                format!(
                    "override of `{}` cannot reduce its visibility from `{}` to `{}`",
                    m.name,
                    m.vis.text(),
                    vis.text()
                ),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "");
            if let Some(s) = self.method_span(*owner, m) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            return None;
        }
        m.vmt
    }

    /// E0765 (`docs/spec/equality.md` 3.1): a class that overrides only one of `Equals` and
    /// `GetHashCode`. `eq_hash` holds the `override` declarations of the two in the class, with
    /// whether each overrides the method of `TObject`; a declaration that failed (E0744) counts
    /// as written, so that it is not reported twice. The fix inserts the other declaration on the
    /// line after the one written.
    pub(super) fn equals_without_hash(
        &mut self,
        class: &str,
        eq_hash: [Option<(bool, TextRange, TextRange)>; 2],
    ) {
        let (written, missing, insert, name, decl) = match eq_hash {
            [Some((true, name, decl)), None] => (
                "Equals",
                "GetHashCode",
                "function GetHashCode: Int64; override;",
                name,
                decl,
            ),
            [None, Some((true, name, decl))] => (
                "GetHashCode",
                "Equals",
                "function Equals(Obj: TObject): Boolean; override;",
                name,
                decl,
            ),
            _ => return,
        };
        let d = Diagnostic::error(
            Code::E(765),
            format!("`{class}` overrides `{written}` but not `{missing}`"),
            Stage::Sema,
        )
        .with_primary(self.span(name), "")
        .with_note(
            "objects that are equal must have the same hash code, so override both `Equals` and \
             `GetHashCode`",
        )
        .with_fix(Fix::suggestion(
            format!("override `{missing}`"),
            vec![self.insert_line_after(decl, insert)],
        ));
        self.push(d);
    }

    /// An edit inserting `text` on a new line after the line where the declaration `decl` ends,
    /// with the indentation of the line where it starts and the file's line ending (the fixes
    /// that add a declaration next to another: E0765).
    pub(super) fn insert_line_after(&self, decl: TextRange, text: &str) -> TextEdit {
        let bytes = self.input.bytes;
        let line_start = bytes[..usize::from(decl.start())]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        let indent: String = bytes[line_start..]
            .iter()
            .take_while(|&&b| b == b' ' || b == b'\t')
            .map(|&b| b as char)
            .collect();
        let end = usize::from(decl.end());
        let line_end = bytes[end..]
            .iter()
            .position(|&b| b == b'\n' || b == b'\r')
            .map_or(bytes.len(), |i| end + i);
        let at = crate::source::TextSize::from(line_end as u32);
        // Keep the line ending of the file
        let eol = if bytes.get(line_end) == Some(&b'\r') {
            "\r\n"
        } else {
            "\n"
        };
        TextEdit::insert(self.input.file, at, format!("{eol}{indent}{text}"))
    }

    /// W0021 (`docs/spec/objects.md` 2.4): the method hides a virtual method of `owner`.
    fn hidden_virtual(
        &mut self,
        decl: &ast::MethodDecl,
        name: &str,
        owner: TypeId,
        name_range: TextRange,
        same: bool,
    ) {
        let owner_name = self.types.name(Ty::Class(owner));
        let end = decl.syntax().text_range().end();
        let mut d = Diagnostic::warning(
            Code::W(21),
            format!("method `{name}` hides virtual method of `{owner_name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(name_range), "");
        if same {
            d = d.with_fix(Fix::suggestion(
                "add `override`",
                vec![TextEdit::insert(self.input.file, end, " override;")],
            ));
        }
        d = d.with_fix(Fix::suggestion(
            "add `reintroduce`",
            vec![TextEdit::insert(self.input.file, end, " reintroduce;")],
        ));
        self.push(d);
    }

    /// Where a method is declared (`None` for a built-in method).
    pub(super) fn method_span(&self, owner: TypeId, m: &ClassMethod) -> Option<Span> {
        let unit = self.types.class_unit(owner)?;
        Some(Span::new(self.unit_file(unit), m.span))
    }

    /// The methods named `name` that a class has, nearest declaration first: those of the
    /// class, then those of its ancestors that are not overridden by a nearer one.
    pub(super) fn methods_named(&self, class: TypeId, name: &str) -> Vec<(TypeId, ClassMethod)> {
        methods_named(&self.types, class, name)
    }

    /// Whether the code being analyzed can see a member of `owner` with visibility `vis`
    /// (`docs/spec/objects.md` 2.5).
    pub(super) fn can_see(&self, vis: Visibility, owner: TypeId) -> bool {
        let unit = self.types.class_unit(owner);
        // Inside a class body, the class being declared
        let here = self.method_class().or(self.class_scope());
        if let Some(h) = here.filter(|h| self.types.is_helper(*h))
            && self.helper_restricts(h, owner)
        {
            // A helper uses only the public members of its type, even in the unit declaring the
            // type (`docs/spec/objects.md` 10.8)
            return vis.is_public();
        }
        let in_descendant = here.is_some_and(|c| self.types.is_descendant(c, owner));
        match vis {
            Visibility::Public | Visibility::Published => true,
            Visibility::Private => unit.is_none_or(|u| self.is_own_unit(u)),
            Visibility::Protected => unit.is_none_or(|u| self.is_own_unit(u)) || in_descendant,
            Visibility::StrictPrivate => here == Some(owner),
            Visibility::StrictProtected => in_descendant,
        }
    }

    /// The class of the method whose body (or a routine nested in it) is being analyzed.
    pub(super) fn method_class(&self) -> Option<TypeId> {
        self.cur
            .iter()
            .chain(self.outer.iter().rev())
            .find_map(|c| c.self_class)
    }

    /// `Self` of the method being analyzed, as a variable of the routine being analyzed.
    pub(super) fn self_var(&mut self) -> Option<(VarId, TypeId)> {
        let (up, class, kind) = self
            .cur
            .iter()
            .chain(self.outer.iter().rev())
            .enumerate()
            .find_map(|(up, c)| c.self_class.map(|k| (up as u32, k, c.self_kind)))?;
        if kind == SelfKind::Static {
            return None;
        }
        Some((self.var_at(up, VarId(0)), class))
    }

    /// What `Self` of the method being analyzed is (`None` outside a method).
    pub(super) fn method_self_kind(&self) -> Option<SelfKind> {
        self.cur
            .iter()
            .chain(self.outer.iter().rev())
            .find(|c| c.self_class.is_some())
            .map(|c| c.self_kind)
    }

    /// The expression `Self` (reading it marks it used): the instance, or the class reference of
    /// a class method.
    pub(super) fn self_expr(&mut self, span: TextRange) -> Option<Expr> {
        self.use_span = Some(span);
        let (v, _) = self.self_var()?;
        let ty = self.var(v).ty;
        Some(Expr {
            kind: ExprKind::Var(v),
            ty,
            value: None,
            span,
        })
    }

    /// The receiver of a member named without `Self.` inside a method: `Self`, or in a static
    /// method the class as a value (its class members need no instance).
    pub(super) fn implicit_receiver(&mut self, span: TextRange) -> Option<Expr> {
        let class = self.method_class()?;
        match self.self_expr(span) {
            Some(e) => Some(e),
            None => Some(self.class_ref_value(class, span)),
        }
    }

    /// E0700 for an instance member used where there is no instance (`docs/spec/objects.md`
    /// 2.3): in a class or static method, or through a class reference.
    pub(super) fn instance_member_error(
        &mut self,
        name: &str,
        decl: Option<Span>,
        range: TextRange,
    ) {
        let msg = match self.method_self_kind() {
            Some(SelfKind::Class) => {
                format!("`{name}` is an instance member and cannot be used in a class method")
            }
            Some(SelfKind::Static) => {
                format!("`{name}` is an instance member and cannot be used in a static method")
            }
            _ => format!("`{name}` is an instance member and cannot be used through a class"),
        };
        let mut d =
            Diagnostic::error(Code::E(700), msg, Stage::Sema).with_primary(self.span(range), "");
        if let Some(s) = decl {
            d = d.with_secondary(s, "declared here");
        }
        self.push(d);
    }

    /// The member `name` of a class as the code being analyzed sees it (`docs/spec/objects.md`
    /// 2.3, 2.5): the nearest visible field or method. `Err(true)` when only invisible members
    /// have the name (E0743 is reported), `Err(false)` when none has it (nothing is reported).
    pub(super) fn find_member(
        &mut self,
        class: TypeId,
        name: &str,
        range: TextRange,
    ) -> Result<Member, bool> {
        // The nearest declaration among the fields and the methods
        let mut hidden: Option<(String, Visibility, TypeId, Option<Span>)> = None;
        for c in self.types.class_chain(class) {
            let field = match &self.types.get(c).kind {
                TypeKind::Class { fields, .. } => fields
                    .iter()
                    .find(|f| f.name.eq_ignore_ascii_case(name))
                    .cloned(),
                TypeKind::Record { fields, .. } => fields
                    .iter()
                    .enumerate()
                    .find(|(_, f)| f.name.eq_ignore_ascii_case(name))
                    .map(|(i, f)| super::types::ClassField {
                        name: f.name.clone(),
                        ty: f.ty,
                        span: f.span,
                        vis: f.vis,
                        slot: i as u32,
                    }),
                _ => None,
            };
            let is_record = self.types.is_record(c);
            if field.is_none()
                && let Some(st) = self
                    .types
                    .class_statics(c)
                    .iter()
                    .find(|s| s.name.eq_ignore_ascii_case(name))
                    .cloned()
            {
                if self.can_see(st.vis, c) {
                    self.member_hint_use(c, st.span, range);
                    return Ok(Member::Static(c, st));
                }
                if hidden.is_none() {
                    let s = self
                        .types
                        .class_unit(c)
                        .map(|u| Span::new(self.unit_file(u), st.span));
                    hidden = Some((st.name, st.vis, c, s));
                }
                continue;
            }
            if field.is_none()
                && let Some(p) = self
                    .types
                    .class_properties(c)
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(name))
                    .cloned()
            {
                if self.can_see(p.vis, c) {
                    self.member_hint_use(c, p.span, range);
                    return Ok(Member::Property(c, Box::new(p)));
                }
                if hidden.is_none() {
                    let s = self
                        .types
                        .class_unit(c)
                        .map(|u| Span::new(self.unit_file(u), p.span));
                    hidden = Some((p.name, p.vis, c, s));
                }
                continue;
            }
            if let Some(f) = field {
                if self.can_see(f.vis, c) {
                    self.member_hint_use(c, f.span, range);
                }
                if self.can_see(f.vis, c) && is_record {
                    return Ok(Member::RecField {
                        index: f.slot,
                        ty: f.ty,
                        name: f.name,
                    });
                }
                if self.can_see(f.vis, c) {
                    return Ok(Member::Field {
                        slot: f.slot,
                        ty: f.ty,
                        name: f.name,
                    });
                }
                if hidden.is_none() {
                    let s = self
                        .types
                        .class_unit(c)
                        .map(|u| Span::new(self.unit_file(u), f.span));
                    hidden = Some((f.name, f.vis, c, s));
                }
                continue;
            }
            let methods: Vec<ClassMethod> = self
                .types
                .class_methods(c)
                .iter()
                .filter(|m| m.name.eq_ignore_ascii_case(name) && m.kind != MethodKind::Operator)
                .cloned()
                .collect();
            if let Some(m) = methods.iter().find(|m| self.can_see(m.vis, c)) {
                return Ok(Member::Methods {
                    name: m.name.clone(),
                    inst: None,
                });
            }
            if let Some(m) = methods.first()
                && hidden.is_none()
            {
                hidden = Some((m.name.clone(), m.vis, c, self.method_span(c, m)));
            }
        }
        if let Some(m) = ExcMember::from_name(&name.to_ascii_lowercase())
            && self.types.is_exception_class(class)
            && (m != ExcMember::Values
                || self
                    .types
                    .is_descendant(class, TypeId(crate::bytecode::exc::ASSERTION_FAILED)))
        {
            return Ok(Member::ExcInfo(m));
        }
        if let Some(b) = ClassBuiltin::from_name(name)
            && !self.types.is_record(class)
            && !self.types.is_interface(class)
            && (b != ClassBuiltin::RefCount
                || self.types.is_descendant(class, super::types::TREFOBJECT))
        {
            return Ok(Member::Builtin(b));
        }
        let Some((n, vis, owner, decl)) = hidden else {
            if self.has_operator_named(class, name) {
                // `TVec.Add(A, B)` (`docs/spec/objects.md` 11.1)
                let t = self.types.name(Ty::Class(class));
                self.error(Code::E(700), format!("`{t}` has no member `{name}`"), range)
                    .notes
                    .push(super::operators::BY_NAME_NOTE.into());
                return Err(true);
            }
            return Err(false);
        };
        let owner_name = self.types.name(Ty::Class(owner));
        let msg = match vis {
            Visibility::StrictPrivate | Visibility::Private => {
                format!("`{n}` is private to `{owner_name}`")
            }
            _ => format!("`{n}` is protected in `{owner_name}`"),
        };
        let mut d =
            Diagnostic::error(Code::E(743), msg, Stage::Sema).with_primary(self.span(range), "");
        if let Some(s) = decl {
            d = d.with_secondary(s, "declared here");
        }
        self.push(d);
        Err(true)
    }

    /// Records a use, at `range`, of the member of `owner` declared at `decl`
    /// (`docs/spec/attributes.md` 6.2).
    fn member_hint_use(&mut self, owner: TypeId, decl: TextRange, range: TextRange) {
        if let Some(u) = self.types.class_unit(owner) {
            let decl = Span::new(self.unit_file(u), decl);
            self.hint_use(decl, range);
        }
    }

    /// The visible methods named `name` of a class, for choosing among overloads: the routine of
    /// each, nearest first ([`method_set`]).
    pub(super) fn method_set(&self, class: TypeId, name: &str) -> Vec<RoutineId> {
        method_set(&self.types, &self.routines, class, name, |vis, owner| {
            self.can_see(vis, owner)
        })
    }

    /// The method of a class a routine is: (the class declaring it, the method).
    pub(super) fn method_of_routine(&self, id: RoutineId) -> Option<(TypeId, ClassMethod)> {
        // An instantiation of a generic method: the method, with its heading
        // (`docs/spec/generics.md` 4.2)
        if let Some((base, _)) = self.instances.rinst.get(&id) {
            let (class, mut m) = self.method_of_routine(*base)?;
            let info = &self.routines[id.0 as usize];
            m.routine = Some(id);
            m.params = info.params.iter().map(|p| (p.ty, p.mode)).collect();
            m.ret = info.ret;
            return Some((class, m));
        }
        let class = self.routines[id.0 as usize].method_of?;
        let m = self
            .types
            .class_methods(class)
            .iter()
            .find(|m| m.routine == Some(id))?
            .clone();
        Some((class, m))
    }

    /// The implementation heading of a method, `procedure TFoo.Bar(…);` (`docs/spec/objects.md`
    /// chapter 1): the method's routine, or `None` after a diagnostic.
    pub(super) fn method_heading(
        &mut self,
        header: &ast::RoutineHeader,
        params: &[ParamInfo],
        ret: Option<Ty>,
        generic: Option<TypeId>,
    ) -> Option<RoutineId> {
        let qname = header.name()?;
        let text = qname.text();
        let name_range = qname.syntax().text_range();
        let (class_text, name) = text.rsplit_once('.')?;
        let first = header.first_token()?;
        let is_class = first.kind() == SyntaxKind::CLASS_KW;
        let kind_tok = if is_class {
            header
                .syntax()
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| t.kind() != SyntaxKind::CLASS_KW && !t.kind().is_trivia())?
        } else {
            first.clone()
        };
        if is_class
            && matches!(
                kind_tok.kind(),
                SyntaxKind::CONSTRUCTOR_KW | SyntaxKind::DESTRUCTOR_KW
            )
        {
            let what = self.text(&kind_tok).to_ascii_lowercase();
            self.error(
                Code::E(3200),
                format!("`class {what}` is not supported yet (planned for a later release)"),
                TextRange::new(first.text_range().start(), kind_tok.text_range().end()),
            );
            return None;
        }
        let heading_kind = match kind_tok.kind() {
            SyntaxKind::CONSTRUCTOR_KW => MethodKind::Constructor,
            SyntaxKind::DESTRUCTOR_KW => MethodKind::Destructor,
            SyntaxKind::OPERATOR_KW => MethodKind::Operator,
            _ if header.is_function() => MethodKind::Function,
            _ => MethodKind::Procedure,
        };
        let class_range = TextRange::new(
            name_range.start(),
            name_range.start() + crate::source::TextSize::from(class_text.len() as u32),
        );
        // A nested class or record (`procedure TOuter.TInner.Run;`; `docs/spec/objects.md` 2.2)
        let looked_up = match class_text.split_once('.') {
            // A generic type (`docs/spec/generics.md` 1.2), found by its type parameters
            _ if generic.is_some() => generic.map(|c| {
                Some(if self.types.is_record(c) {
                    Ty::Record(c)
                } else {
                    Ty::Class(c)
                })
            }),
            Some((outer, _)) => {
                let parts: Vec<(String, TextRange)> = {
                    let mut at = name_range.start();
                    class_text
                        .split('.')
                        .map(|p| {
                            let r = TextRange::new(
                                at,
                                at + crate::source::TextSize::from(p.len() as u32),
                            );
                            at = r.end() + crate::source::TextSize::from(1);
                            (p.to_string(), r)
                        })
                        .collect()
                };
                match self.lookup_type_symbol(&outer.to_ascii_lowercase()) {
                    Some(Some(t @ (Ty::Class(_) | Ty::Record(_)))) => {
                        Some(Some(self.nested_type(t, &parts[1..], class_range)))
                    }
                    other => other,
                }
            }
            None => self.lookup_type_symbol(&class_text.to_ascii_lowercase()),
        };
        let class = match looked_up {
            Some(Some(Ty::Class(c) | Ty::Record(c) | Ty::Helper(c)))
                if self.types.class_unit(c) == Some(self.unit_index) =>
            {
                c
            }
            Some(Some(Ty::Error)) => return None,
            Some(Some(_)) | Some(None) => {
                self.error(
                    Code::E(700),
                    format!("`{class_text}` is not a class of this unit"),
                    class_range,
                );
                return None;
            }
            // A class of a unit in `uses`: its bodies are in that unit
            None if !class_text.contains('.')
                && matches!(
                    self.dry_run(|s| s.lookup_in_units(
                        &class_text.to_ascii_lowercase(),
                        class_text,
                        class_range
                    )),
                    super::UnitLookup::Found(super::Resolved::Type(_))
                ) =>
            {
                self.error(
                    Code::E(700),
                    format!("`{class_text}` is not a class of this unit"),
                    class_range,
                );
                return None;
            }
            None => {
                self.error(
                    Code::E(700),
                    format!("undeclared identifier `{class_text}`"),
                    class_range,
                );
                return None;
            }
        };
        let class_name = self.types.name(Ty::Class(class));
        let method_range = TextRange::new(
            name_range.end() - crate::source::TextSize::from(name.len() as u32),
            name_range.end(),
        );
        let candidates: Vec<ClassMethod> = self
            .types
            .class_methods(class)
            .iter()
            .filter(|m| m.name.eq_ignore_ascii_case(name) && m.routine.is_some())
            .cloned()
            .collect();
        if candidates.is_empty() {
            // The declaration of `_AddRef` and the like got E3017, whose fix removes this body
            // too (`docs/spec/objects.md` 7.1)
            if !super::interfaces::is_ref_count_method(name) {
                self.error(
                    Code::E(700),
                    format!("`{class_name}` has no method `{name}`"),
                    method_range,
                );
            }
            return None;
        }
        // The parameters may be left out when the declaration is not overloaded
        // (`docs/spec/objects.md` 1). A heading without them still matches an overloaded procedure
        // without parameters, since nothing is left out
        let omitted = header.param_list().is_none() && header.return_type().is_none();
        let sig = signature(params);
        let routines = &self.routines;
        let matches = |m: &ClassMethod| {
            let overloaded = m.routine.is_some_and(|r| routines[r.0 as usize].overload);
            m.kind == heading_kind
                && (m.self_kind != SelfKind::Instance) == is_class
                && if omitted {
                    if overloaded {
                        m.params.is_empty() && m.ret.is_none()
                    } else {
                        candidates.len() == 1 || m.params.is_empty()
                    }
                } else {
                    same_signature(&m.params, &sig) && m.ret == ret
                }
        };
        let Some(m) = candidates.iter().find(|m| matches(m)).cloned() else {
            let first = candidates[0].clone();
            self.body_mismatch(class, &first, &class_name, name, name_range);
            return None;
        };
        // The attributes of a parameter written in both places (`docs/spec/attributes.md` 1.2)
        if let Some(r) = m.routine
            && !super::overload::same_param_attrs(&self.routines[r.0 as usize].params, params)
        {
            self.body_mismatch(class, &m, &class_name, name, name_range);
        }
        let id = m.routine.expect("candidates have a routine");
        if m.is_abstract {
            let mut d = Diagnostic::error(
                Code::E(744),
                format!(
                    "abstract method `{class_name}.{}` cannot have a body",
                    m.name
                ),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "");
            if let Some(s) = self.method_span(class, &m) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            return None;
        }
        if self.routines[id.0 as usize].has_body {
            let prev = self.routines[id.0 as usize].body_span;
            let mut d = Diagnostic::error(
                Code::E(701),
                format!("`{class_name}.{}` is already declared", m.name),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "declared again here");
            if let Some(p) = prev {
                d = d.with_secondary(self.span(p), "declared here");
            }
            self.push(d);
            return None;
        }
        self.routines[id.0 as usize].body_span = Some(name_range);
        Some(id)
    }

    /// E0723 when the body of a method does not match its declaration `m` in `class`.
    fn body_mismatch(
        &mut self,
        class: TypeId,
        m: &ClassMethod,
        class_name: &str,
        name: &str,
        name_range: TextRange,
    ) {
        let mut d = Diagnostic::error(
            Code::E(723),
            format!("body of `{class_name}.{name}` does not match its declaration"),
            Stage::Sema,
        )
        .with_primary(self.span(name_range), "");
        if let Some(s) = self.method_span(class, m) {
            d = d.with_secondary(s, "declared here");
        }
        self.push(d);
    }

    /// The methods of the unit's classes without a body (`docs/spec/objects.md` 2.3, E0723).
    pub(super) fn check_method_bodies(&mut self) {
        let unit = self.unit_index;
        let missing: Vec<(String, TextRange)> = self
            .routines
            .iter()
            .filter(|r| {
                r.unit == unit
                    // A helper in a generic type is reported with E0757 alone
                    && r.method_of.is_some_and(|c| !self.generic_helpers.contains(&c))
                    && !r.is_abstract
                    && !r.native
                    && !r.has_body
                    && !r.implicit_forward
            })
            .map(|r| (r.name.clone(), r.name_span))
            .collect();
        for (name, span) in missing {
            self.error(Code::E(723), format!("method `{name}` has no body"), span);
        }
    }

    /// E0742 (`docs/spec/objects.md` 3.1): an instance of an abstract class created from the
    /// class's name. Returns whether it was reported.
    pub(super) fn check_abstract_create(&mut self, class: TypeId, range: TextRange) -> bool {
        if !self.types.class_is_abstract(class) {
            return false;
        }
        let name = self.types.name(Ty::Class(class));
        let mut d = Diagnostic::error(
            Code::E(742),
            format!("cannot create an instance of abstract class `{name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        // An abstract method still in the table: the nearest declaration of its entry
        let vmt = self.types.class_vmt(class).to_vec();
        'find: for (k, e) in vmt.iter().enumerate() {
            if *e != VmtEntry::Abstract {
                continue;
            }
            for c in self.types.class_chain(class) {
                if let Some(m) = self
                    .types
                    .class_methods(c)
                    .iter()
                    .find(|m| m.vmt == Some(k as u32))
                    .cloned()
                {
                    if let Some(s) = self.method_span(c, &m) {
                        d = d.with_secondary(s, "abstract method declared here");
                    }
                    break 'find;
                }
            }
        }
        self.push(d);
        true
    }

    /// A built-in body of a function of `TObject` called statically (`inherited ToString`,
    /// `inherited Equals(Obj)`, `inherited GetHashCode`): the instructions it stands for
    /// (`docs/spec/objects.md` 14.3, `docs/spec/equality.md` 8.2). `args` are `Self` and the
    /// arguments.
    fn inherited_builtin_function(
        &mut self,
        owner: TypeId,
        vmt: u32,
        args: Vec<CallArg>,
        ret: Ty,
        span: TextRange,
    ) -> Expr {
        let mut values = args.into_iter().map(|a| match a {
            CallArg::Value(e) => e,
            _ => unreachable!("the built-in functions take values"),
        });
        let recv = values.next().expect("`Self` comes first");
        let kind = match vmt {
            super::types::VMT_TO_STRING if self.types.is_exception_class(owner) => {
                // `Exception.ToString` called statically is `Message` (`docs/spec/objects.md`
                // 8.2, 14.8)
                ExprKind::ObjField {
                    base: Box::new(recv),
                    slot: crate::bytecode::EXC_SLOT_MESSAGE,
                    name: super::types::EXC_FIELD_MESSAGE.into(),
                }
            }
            // The class name (`docs/spec/objects.md` 14.3)
            super::types::VMT_TO_STRING => ExprKind::ClassName {
                base: Box::new(recv),
            },
            // The same instance: `EqInt` (`docs/spec/equality.md` 8.2)
            super::types::VMT_EQUALS => ExprKind::Builtin {
                func: Builtin::SameInstance,
                args: std::iter::once(recv).chain(values).collect(),
            },
            // `HashInt` of the handle
            super::types::VMT_GET_HASH_CODE => ExprKind::Builtin {
                func: Builtin::InstanceHash,
                args: vec![recv],
            },
            _ => unreachable!("not a built-in function of `TObject`"),
        };
        Expr {
            kind,
            ty: ret,
            value: None,
            span,
        }
    }

    /// The routine standing for a virtual method of `TObject` (`ToString`, `AfterConstruction`,
    /// `BeforeDestruction`, `Equals`, `GetHashCode`; `docs/spec/objects.md` chapter 5,
    /// `docs/spec/equality.md` 3.1): a method without a function (its body is the VM's), so that a
    /// call can go through the table.
    pub(super) fn builtin_routine(&mut self, name: &str, ret: Option<Ty>) -> RoutineId {
        let object = super::types::TOBJECT;
        let full = format!("TObject.{name}");
        if let Some(i) = self
            .routines
            .iter()
            .position(|r| r.method_of == Some(object) && r.is_abstract && r.name == full)
        {
            return RoutineId(i as u32);
        }
        // The parameters of the method (`Obj` of `Equals`)
        let params = self
            .types
            .class_methods(object)
            .iter()
            .find(|m| m.name == name)
            .map(|m| {
                m.params
                    .iter()
                    .map(|&(ty, mode)| ParamInfo {
                        name: "Obj".into(),
                        ty,
                        mode,
                        span: TextRange::default(),
                        default: None,
                        attrs: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let id = RoutineId(self.routines.len() as u32);
        self.routines.push(RoutineInfo {
            name: full,
            params,
            ret,
            has_body: false,
            forward_span: None,
            implicit_forward: false,
            name_span: TextRange::default(),
            hir: None,
            overload: false,
            overload_fix_at: None,
            unit: self.unit_index,
            file: self.input.file,
            interface: false,
            parent: None,
            scope: None,
            calls: Vec::new(),
            method_of: Some(object),
            self_kind: SelfKind::Instance,
            is_abstract: true,
            native: false,
            body_span: None,
            closure: false,
            type_params: Vec::new(),
            closure_info: super::ClosureInfo::default(),
        });
        id
    }

    /// The routine of the built-in constructor `Exception.Create(const Msg: string)`
    /// (`docs/spec/objects.md` 8.2), made the first time a constructor of an exception class is
    /// looked up so that it takes part in the choice among overloads. It has no function: a call
    /// becomes the instructions that write `Message` ([`Sema::builtin_create_message`]).
    pub(super) fn exception_create_routine(&mut self) -> RoutineId {
        let exception = super::types::EXCEPTION;
        let full = format!("Exception.{}", super::types::EXC_CREATE);
        if let Some(i) = self
            .routines
            .iter()
            .position(|r| r.method_of == Some(exception) && r.name == full)
        {
            return RoutineId(i as u32);
        }
        let id = RoutineId(self.routines.len() as u32);
        self.routines.push(RoutineInfo {
            name: full,
            params: vec![ParamInfo {
                name: "Msg".into(),
                ty: Ty::String,
                mode: ParamMode::Const,
                span: TextRange::default(),
                default: None,
                attrs: None,
            }],
            ret: None,
            has_body: false,
            forward_span: None,
            implicit_forward: false,
            name_span: TextRange::default(),
            hir: None,
            overload: false,
            overload_fix_at: None,
            unit: self.unit_index,
            file: self.input.file,
            interface: false,
            parent: None,
            scope: None,
            calls: Vec::new(),
            method_of: Some(exception),
            self_kind: SelfKind::Instance,
            is_abstract: true,
            native: false,
            body_span: None,
            closure: false,
            type_params: Vec::new(),
            closure_info: super::ClosureInfo::default(),
        });
        if let TypeKind::Class { methods, .. } = &mut self.types.get_mut(exception).kind
            && let Some(m) = methods
                .iter_mut()
                .find(|m| m.name == super::types::EXC_CREATE)
        {
            m.routine = Some(id);
        }
        id
    }

    /// The message of a call of the built-in `Exception.Create(Msg)` (`ctor` with its lowered
    /// arguments, `Self` not among them), or `None` for any other constructor.
    pub(super) fn builtin_create_message(&self, ctor: RoutineId, args: &[CallArg]) -> Option<Expr> {
        let info = &self.routines[ctor.0 as usize];
        if !(info.is_abstract && info.method_of == Some(super::types::EXCEPTION)) {
            return None;
        }
        match args.last() {
            Some(CallArg::Value(e)) => Some(e.clone()),
            _ => None,
        }
    }

    /// Whether `name` of the class `id` is the built-in `Exception.CreateFmt` (no visible method
    /// of the class or its ancestors below `Exception` declares the name; 8.2).
    pub(super) fn is_builtin_create_fmt(&self, id: TypeId, name: &str) -> bool {
        name.eq_ignore_ascii_case(super::types::EXC_CREATE_FMT)
            && self
                .methods_named(id, name)
                .first()
                .is_some_and(|(owner, m)| *owner == super::types::EXCEPTION && m.routine.is_none())
    }

    /// A call of the method `name` on `recv`, whose static type is `class`
    /// (`docs/spec/objects.md` 2.3, 2.4). `inherited` calls the method `class` (the parent of
    /// the method being analyzed) has, statically. `bare` is a call written without parentheses.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn method_call(
        &mut self,
        recv: Expr,
        class: TypeId,
        name: &str,
        inst: Option<Vec<RoutineId>>,
        inherited: bool,
        args: &[ast::Arg],
        span: TextRange,
        range: TextRange,
        bare: bool,
    ) -> Expr {
        // `H.M(X, …)`: `X` is the receiver of an instance method (`docs/spec/objects.md` 10.5)
        let mut explicit: Option<bool> = None;
        let (recv, args) = match recv.ty {
            Ty::Helper(h) => match self.explicit_kind(h, class, name, args, span) {
                None => return err_expr(span),
                Some((true, expected)) => {
                    explicit = Some(true);
                    match self.explicit_receiver(h, args, expected, span) {
                        Some((x, rest)) => (x, rest),
                        None => return err_expr(span),
                    }
                }
                Some((false, _)) => {
                    explicit = Some(false);
                    let recv = match self.types.helper_target(h) {
                        Some(Ty::Class(c)) => self.class_ref_value(c, recv.span),
                        _ => recv,
                    };
                    (recv, args)
                }
            },
            _ => (recv, args),
        };
        if self.types.is_exception_class(class) {
            self.exception_create_routine();
        }
        if inherited && self.is_builtin_create_fmt(class, name) {
            // `inherited CreateFmt(…)` (`docs/spec/objects.md` 8.2): `Message` of `Self`
            let Some(msg) = self.format_message(args, span) else {
                return err_expr(span);
            };
            return Expr {
                kind: ExprKind::InheritedBuiltin {
                    base: Box::new(recv),
                    name: format!("Exception.{}", super::types::EXC_CREATE_FMT),
                    msg: Some(Box::new(msg)),
                },
                ty: Ty::Error,
                value: None,
                span,
            };
        }
        let named = self.methods_named(class, name);
        let is_ctor = named
            .first()
            .is_some_and(|(_, m)| m.kind == MethodKind::Constructor);
        if !inherited && is_ctor && Self::no_instance(&recv) && self.types.is_record(class) {
            // `TPoint.Create(…)` (`docs/spec/objects.md` chapter 6)
            return self.record_new(class, name, args, span, range);
        }
        if !inherited && is_ctor && matches!(recv.ty, Ty::ClassRef(_)) {
            // `C.Create(…)`: an instance of the class `C` points to (`docs/spec/objects.md` 3.1)
            return self.new_instance(class, name, args, span, range, Some(recv));
        }
        if !inherited && is_ctor {
            // `O.Create` / `Create(…)` inside a method (`docs/spec/objects.md` 3.1)
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            let t = self.types.name(Ty::Class(class));
            let is_record = self.types.is_record(class);
            let d = self.error(
                Code::E(747),
                "a constructor must be called on a class",
                range,
            );
            d.notes.push(format!(
                "call the constructor on the class, as in `{t}.Create`"
            ));
            if !is_record {
                d.notes.push(
                    "inside a constructor, use `inherited Create` to run the parent constructor"
                        .into(),
                );
            }
            return err_expr(span);
        }
        let given = inst.is_some();
        let mut set = match inst {
            Some(s) => s,
            None => self.method_set(class, name),
        };
        let excluded = if given {
            super::infer::Excluded::default()
        } else {
            match self.infer_candidates(&mut set, args, span) {
                Some(x) => x,
                None => return err_expr(span),
            }
        };
        if let Some(instance) = explicit {
            // Only the candidates of the form chosen for `H.M(…)`
            set.retain(|r| {
                self.method_of_routine(*r)
                    .is_some_and(|(_, m)| (m.self_kind == SelfKind::Instance) == instance)
            });
        }
        if set.is_empty() && !excluded.shown.is_empty() {
            // Type argument inference left out every candidate (`docs/spec/generics.md` 5.4)
            self.no_candidate_left(&excluded, args, span);
            return err_expr(span);
        }
        if set.is_empty() {
            let builtin = named.into_iter().find(|(_, m)| m.routine.is_none());
            if let Some((_, m)) = &builtin
                && Self::no_instance(&recv)
            {
                // `Free`, `ToString`, … of `TObject` need an instance (2.3)
                for a in args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                let n = m.name.clone();
                self.instance_member_error(&n, None, range);
                return err_expr(span);
            }
            if let Some((owner, m)) = &builtin
                && m.kind != MethodKind::Function
            {
                if !args.is_empty() {
                    for a in args {
                        if let Some(v) = a.value() {
                            self.lower_expr(&v);
                        }
                    }
                    self.error(
                        Code::E(705),
                        format!(
                            "wrong number of arguments: expected 0, found {}",
                            args.len()
                        ),
                        span,
                    );
                    return err_expr(span);
                }
                let owner_name = self.types.name(Ty::Class(*owner));
                if inherited {
                    // A built-in body that does nothing (`docs/spec/objects.md` 14.3)
                    return Expr {
                        kind: ExprKind::InheritedBuiltin {
                            base: Box::new(recv),
                            name: format!("{owner_name}.{}", m.name),
                            msg: None,
                        },
                        ty: Ty::Error,
                        value: None,
                        span,
                    };
                }
                if let Some(k) = m.vmt.filter(|_| m.kind == MethodKind::Procedure) {
                    // `O.AfterConstruction` / `O.BeforeDestruction`: through the table
                    let routine = self.builtin_routine(&m.name, None);
                    return Expr {
                        kind: ExprKind::Call {
                            routine,
                            args: vec![CallArg::Value(recv)],
                            dispatch: Dispatch::Virtual(k),
                        },
                        ty: Ty::Error,
                        value: None,
                        span,
                    };
                }
            }
            let builtin = builtin.filter(|(_, m)| m.kind == MethodKind::Function);
            if builtin.is_none() {
                for a in args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                self.member_missing(class, name, range);
                return err_expr(span);
            }
            let Some((owner, m)) = builtin else {
                unreachable!("checked above")
            };
            // `ToString`, `Equals` and `GetHashCode`, whose bodies are the VM's
            if args.len() != m.params.len() {
                for a in args {
                    if let Some(v) = a.value() {
                        self.lower_expr(&v);
                    }
                }
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected {}, found {}",
                        m.params.len(),
                        args.len()
                    ),
                    span,
                );
                return err_expr(span);
            }
            let mut call_args = vec![CallArg::Value(recv)];
            for (i, (a, (ty, _))) in args.iter().zip(&m.params).enumerate() {
                let Some(v) = a.value() else {
                    return err_expr(span);
                };
                let x = self.lower_expected(&v, *ty);
                let x = self.coerce(x, *ty, false, Some((i + 1, "Obj")));
                if x.ty.is_error() {
                    return err_expr(span);
                }
                call_args.push(CallArg::Value(x));
            }
            let ret = m
                .ret
                .expect("the built-in methods taken here are functions");
            let vmt = m
                .vmt
                .expect("the built-in functions of `TObject` are virtual");
            if inherited {
                return self.inherited_builtin_function(owner, vmt, call_args, ret, span);
            }
            let routine = self.builtin_routine(&m.name, m.ret);
            return Expr {
                kind: ExprKind::Call {
                    routine,
                    args: call_args,
                    dispatch: Dispatch::Virtual(vmt),
                },
                ty: ret,
                value: None,
                span,
            };
        }
        let at = if bare { range } else { span };
        let id = match self.pick_method_with(set, args, at, bare, &excluded) {
            Pick::Routine(id) => id,
            Pick::Failed => return err_expr(span),
        };
        self.routine_hint_use(id, range);
        if bare && self.routines[id.0 as usize].min_args() > 0 {
            let info = &self.routines[id.0 as usize];
            let kind = if info.ret.is_some() {
                "function"
            } else {
                "procedure"
            };
            let d = Diagnostic::error(
                Code::E(704),
                format!("{kind} `{}` requires arguments", info.name),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.routine_decl_span(id), "declared here");
            self.push(d);
            return err_expr(span);
        }
        let Some((owner, m)) = self.method_of_routine(id) else {
            return err_expr(span);
        };
        if inherited {
            let mut call = self.lower_routine_call(id, args, span);
            if let ExprKind::Call { args: a, .. } = &mut call.kind
                && let Some(msg) = self.builtin_create_message(id, a)
            {
                // `inherited Create(Msg)` (`docs/spec/objects.md` 8.2): `Message` of `Self`
                return Expr {
                    kind: ExprKind::InheritedBuiltin {
                        base: Box::new(recv),
                        name: format!("Exception.{}", super::types::EXC_CREATE),
                        msg: Some(Box::new(msg)),
                    },
                    ty: Ty::Error,
                    value: None,
                    span,
                };
            }
        }
        if inherited && m.is_abstract {
            let owner_name = self.types.name(Ty::Class(owner));
            let mut d = Diagnostic::error(
                Code::E(744),
                format!("cannot call abstract method `{owner_name}.{}`", m.name),
                Stage::Sema,
            )
            .with_primary(self.span(range), "");
            if let Some(s) = self.method_span(owner, &m) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            return err_expr(span);
        }
        if m.self_kind == SelfKind::Instance && Self::no_instance(&recv) {
            // An instance method called where there is no instance (2.3)
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            let decl = self.method_span(owner, &m);
            self.instance_member_error(&m.name, decl, range);
            return err_expr(span);
        }
        let dispatch = self.method_dispatch(class, &m, inherited);
        let recv = match m.self_kind {
            // `Self` of a record is passed like a `var` argument (chapter 6)
            SelfKind::Instance if self.types.self_by_ref(owner) => Some(self.record_receiver(recv)),
            SelfKind::Instance => Some(CallArg::Value(recv)),
            SelfKind::Class => Some(CallArg::Value(self.receiver_class_ref(recv))),
            // A static method has no `Self`; the receiver only names the class
            SelfKind::Static => None,
        };
        let mut call = self.lower_routine_call(id, args, span);
        if let ExprKind::Call {
            args: a,
            dispatch: d,
            ..
        } = &mut call.kind
        {
            if let Some(r) = recv {
                a.insert(0, r);
            }
            *d = dispatch;
        }
        call
    }

    /// How a call of method `m` on a receiver whose static class is `class` reaches it
    /// (`docs/spec/objects.md` 2.4, 14.3).
    pub(super) fn method_dispatch(
        &self,
        class: TypeId,
        m: &ClassMethod,
        inherited: bool,
    ) -> Dispatch {
        if self.types.is_record(class) {
            // A record's methods are routines with `Self` first (`docs/spec/objects.md` chapter 6)
            return Dispatch::Routine;
        }
        if self.types.is_helper(class) {
            // A helper's methods are called statically; a class helper's instance methods check
            // `Self` (`docs/spec/objects.md` 10.8, 14.10)
            return match m.self_kind {
                SelfKind::Instance if !self.types.self_by_ref(class) => Dispatch::Method,
                _ => Dispatch::Routine,
            };
        }
        if self.types.is_interface(class) {
            // `docs/spec/objects.md` 7.5: the method number in the interface the receiver's type
            // names
            return Dispatch::Intf(class, m.vmt.expect("an interface method has a number"));
        }
        let sealed = matches!(
            self.types.get(class).kind,
            TypeKind::Class { sealed: true, .. }
        );
        // `docs/spec/objects.md` 2.4: no descendant can override it (Q118)
        let exact = (m.is_final || sealed) && !m.is_abstract;
        match (m.self_kind, m.vmt) {
            (SelfKind::Instance, _) if inherited => Dispatch::Inherited,
            (SelfKind::Instance, Some(_)) if exact => Dispatch::Method,
            (SelfKind::Instance, Some(k)) => Dispatch::Virtual(k),
            (SelfKind::Instance, None) => Dispatch::Method,
            // A class method takes the class reference as `Self` (2.3, 14.3)
            (SelfKind::Class, Some(k)) if !inherited && !exact => Dispatch::VirtualClass(k),
            (SelfKind::Class | SelfKind::Static, _) => Dispatch::Routine,
        }
    }

    /// A member of an instance used in an expression or as a statement
    /// (`docs/spec/objects.md` 2.2, 2.3, chapter 5): `args` is `None` for a name written without
    /// parentheses. `value`: the expression needs a value (a procedure is E0725).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn member_expr(
        &mut self,
        recv: Expr,
        class: TypeId,
        member: Member,
        inherited: bool,
        args: Option<&[ast::Arg]>,
        value: bool,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let lower_args = |s: &mut Self| {
            for a in args.unwrap_or(&[]) {
                if let Some(v) = a.value() {
                    s.lower_expr(&v);
                }
            }
        };
        if let Member::Methods { name, .. } = &member
            && class == super::types::TINDEXRANGE
            && name.eq_ignore_ascii_case(super::types::INDEX_RANGE_CLAMP)
        {
            // A procedure written out where it is called (`docs/spec/objects.md` 12.3)
            lower_args(self);
            self.error(
                Code::E(725),
                "procedure `TIndexRange.Clamp` has no value",
                range,
            );
            return err_expr(span);
        }
        match member {
            Member::Static(owner, st) => {
                // `O.Count`: the class's member; the instance is not read (2.2)
                if args.is_some() || !value {
                    lower_args(self);
                    let text = self.src(range);
                    self.error(
                        Code::E(704),
                        format!("`{text}` is not a procedure or function"),
                        range,
                    );
                    return err_expr(span);
                }
                self.static_expr(owner, &st, range, span)
            }
            Member::Field { name: n, .. } | Member::RecField { name: n, .. }
                if Self::no_instance(&recv) =>
            {
                // No instance (2.3)
                lower_args(self);
                let decl = self
                    .types
                    .class_fields(class)
                    .iter()
                    .find(|(_, f)| f.name == n)
                    .and_then(|(owner, f)| {
                        self.types
                            .class_unit(*owner)
                            .map(|u| Span::new(self.unit_file(u), f.span))
                    });
                self.instance_member_error(&n, decl, range);
                err_expr(span)
            }
            Member::Field { slot, ty, name: n } => {
                if args.is_some() || !value {
                    lower_args(self);
                    // `O.F(…)`, or a field of `Self` named alone (`recv` is at the name)
                    let whole = if recv.span == range {
                        range
                    } else {
                        TextRange::new(recv.span.start(), range.end())
                    };
                    let text = self.src(whole);
                    self.error(
                        Code::E(704),
                        format!("`{text}` is not a procedure or function"),
                        whole,
                    );
                    return err_expr(span);
                }
                self.weak_read(Expr {
                    kind: ExprKind::ObjField {
                        base: Box::new(recv),
                        slot,
                        name: n,
                    },
                    ty,
                    value: None,
                    span,
                })
            }
            Member::RecField { index, ty, name: n } => {
                if args.is_some() || !value {
                    lower_args(self);
                    let whole = if recv.span == range {
                        range
                    } else {
                        TextRange::new(recv.span.start(), range.end())
                    };
                    let text = self.src(whole);
                    self.error(
                        Code::E(704),
                        format!("`{text}` is not a procedure or function"),
                        whole,
                    );
                    return err_expr(span);
                }
                self.weak_read(Expr {
                    kind: ExprKind::Field {
                        base: Box::new(recv),
                        index,
                        name: n,
                    },
                    ty,
                    value: None,
                    span,
                })
            }
            Member::Builtin(b) => self.class_builtin(recv, class, b, args, range, span),
            Member::HelperBuiltin(b) => self.helper_builtin_expr(recv, b, args, value, range, span),
            Member::ExcInfo(m) => {
                // A read-only property of the instance (`docs/spec/objects.md` 8.2)
                if args.is_some() || !value {
                    lower_args(self);
                    let whole = if recv.span == range {
                        range
                    } else {
                        TextRange::new(recv.span.start(), range.end())
                    };
                    let text = self.src(whole);
                    self.error(
                        Code::E(704),
                        format!("`{text}` is not a procedure or function"),
                        whole,
                    );
                    return err_expr(span);
                }
                if Self::no_instance(&recv) {
                    self.instance_member_error(m.name(), None, range);
                    return err_expr(span);
                }
                Expr {
                    ty: super::dump::exc_member_type(m),
                    kind: ExprKind::ExcMember {
                        base: Box::new(recv),
                        member: m,
                    },
                    value: None,
                    span,
                }
            }
            Member::Property(owner, p) => {
                // `docs/spec/objects.md` 2.6
                if args.is_some() || !value {
                    lower_args(self);
                    let whole = if recv.span == range {
                        range
                    } else {
                        TextRange::new(recv.span.start(), range.end())
                    };
                    let text = self.src(whole);
                    self.error(
                        Code::E(704),
                        format!("`{text}` is not a procedure or function"),
                        whole,
                    );
                    return err_expr(span);
                }
                self.property_read(
                    recv,
                    class,
                    owner,
                    &p,
                    super::defaults::Indices::Ast(Vec::new()),
                    range,
                    span,
                )
            }
            Member::Methods { name: n, inst } => {
                let call = self.method_call(
                    recv,
                    class,
                    &n,
                    inst,
                    inherited,
                    args.unwrap_or(&[]),
                    span,
                    range,
                    args.is_none(),
                );
                if value
                    && let ExprKind::Call { routine, .. } = &call.kind
                    && self.routines[routine.0 as usize].ret.is_none()
                {
                    let r = *routine;
                    self.routine_no_value(r, range);
                    return err_expr(span);
                }
                call
            }
        }
    }

    /// The class reference of a receiver: itself, or the class of an instance (`ClassType`).
    pub(super) fn receiver_class_ref(&mut self, recv: Expr) -> Expr {
        if matches!(recv.ty, Ty::ClassRef(_)) {
            return recv;
        }
        let span = recv.span;
        Expr {
            kind: ExprKind::ClassInfo {
                op: ClassInfoOp::ClassType,
                args: vec![recv],
            },
            ty: Ty::ClassRef(super::types::TCLASS),
            value: None,
            span,
        }
    }

    /// A member of `TObject` about classes (`docs/spec/objects.md` chapter 5) on an instance or
    /// a class reference.
    fn class_builtin(
        &mut self,
        recv: Expr,
        class: TypeId,
        b: ClassBuiltin,
        args: Option<&[ast::Arg]>,
        range: TextRange,
        span: TextRange,
    ) -> Expr {
        let _ = class;
        let args = args.unwrap_or(&[]);
        let expected = match b {
            ClassBuiltin::ClassNameIs | ClassBuiltin::InheritsFrom => 1,
            _ => 0,
        };
        if args.len() != expected {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            self.error(
                Code::E(705),
                format!(
                    "wrong number of arguments: expected {expected}, found {}",
                    args.len()
                ),
                span,
            );
            return err_expr(span);
        }
        let info = |op, args, ty| Expr {
            kind: ExprKind::ClassInfo { op, args },
            ty,
            value: None,
            span,
        };
        let tclass = Ty::ClassRef(super::types::TCLASS);
        match b {
            ClassBuiltin::ClassType => {
                if matches!(recv.ty, Ty::ClassRef(_)) {
                    self.instance_member_error("ClassType", None, range);
                    return err_expr(span);
                }
                info(ClassInfoOp::ClassType, vec![recv], tclass)
            }
            ClassBuiltin::RefCount => {
                // `docs/spec/objects.md` chapter 5: a property of the instance
                if matches!(recv.ty, Ty::ClassRef(_)) {
                    self.instance_member_error("RefCount", None, range);
                    return err_expr(span);
                }
                info(ClassInfoOp::RefCount, vec![recv], Ty::Integer)
            }
            ClassBuiltin::ClassName => self.class_name_expr(recv, span),
            ClassBuiltin::ClassParent => {
                let r = self.receiver_class_ref(recv);
                info(ClassInfoOp::ClassParent, vec![r], tclass)
            }
            ClassBuiltin::InheritsFrom => {
                let r = self.receiver_class_ref(recv);
                let Some(v) = args[0].value() else {
                    return err_expr(span);
                };
                let a = self.lower_expected(&v, tclass);
                let a = self.coerce(a, tclass, false, Some((1, "AClass")));
                if a.ty.is_error() {
                    return err_expr(span);
                }
                info(ClassInfoOp::InheritsFrom, vec![r, a], Ty::Boolean)
            }
            ClassBuiltin::ClassNameIs => {
                // `ClassName` compared without case in ASCII (`SameText`)
                let n = self.class_name_expr(recv, span);
                let Some(v) = args[0].value() else {
                    return err_expr(span);
                };
                let a = self.lower_expr(&v);
                let a = self.coerce(a, Ty::String, false, Some((1, "Name")));
                if a.ty.is_error() {
                    return err_expr(span);
                }
                Expr {
                    kind: ExprKind::Builtin {
                        func: Builtin::SameText,
                        args: vec![n, a],
                    },
                    ty: Ty::Boolean,
                    value: None,
                    span,
                }
            }
        }
    }

    /// `ClassName` of an instance or a class reference.
    fn class_name_expr(&mut self, recv: Expr, span: TextRange) -> Expr {
        if matches!(recv.ty, Ty::ClassRef(_)) {
            return Expr {
                kind: ExprKind::ClassInfo {
                    op: ClassInfoOp::ClassRefName,
                    args: vec![recv],
                },
                ty: Ty::String,
                value: None,
                span,
            };
        }
        Expr {
            kind: ExprKind::ClassName {
                base: Box::new(recv),
            },
            ty: Ty::String,
            value: None,
            span,
        }
    }

    /// `inherited;` (`docs/spec/objects.md` 2.4): the parent's method of the same name and
    /// parameters, called statically with the parameters' values. Nothing when the parent has no
    /// such method (or only a built-in body).
    pub(super) fn bare_inherited(&mut self, span: TextRange) -> Option<StmtKind> {
        let method = self
            .cur
            .as_ref()
            .filter(|c| c.self_class.is_some())
            .and_then(|c| c.routine);
        let Some(id) = method else {
            self.error(
                Code::E(700),
                "`inherited` is only allowed in a method",
                span,
            );
            return None;
        };
        let (class, m) = self.method_of_routine(id)?;
        if self.types.is_helper(class) {
            // `docs/spec/objects.md` 10.6
            self.error(Code::E(753), "`inherited` is not allowed in a helper", span);
            return None;
        }
        if self.types.is_record(class) {
            self.error(
                Code::E(700),
                "`inherited` is only allowed in a method of a class",
                span,
            );
            return None;
        }
        let parent = self.types.parent(class)?;
        let (owner, pm) = self
            .methods_named(parent, &m.name)
            .into_iter()
            .find(|(c, x)| {
                self.can_see(x.vis, *c)
                    && x.kind == m.kind
                    && same_signature(&x.params, &m.params)
                    && x.ret == m.ret
            })?;
        let routine = pm.routine?;
        // `inherited` alone calls the parent's method (`docs/spec/attributes.md` 6.2)
        self.routine_hint_use(routine, span);
        if pm.is_abstract {
            let owner_name = self.types.name(Ty::Class(owner));
            let mut d = Diagnostic::error(
                Code::E(744),
                format!("cannot call abstract method `{owner_name}.{}`", pm.name),
                Stage::Sema,
            )
            .with_primary(self.span(span), "");
            if let Some(s) = self.method_span(owner, &pm) {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
            return None;
        }
        let recv = self.self_expr(span)?;
        let mut args = vec![CallArg::Value(recv)];
        let params = self.routines[id.0 as usize].params.clone();
        for (i, p) in params.iter().enumerate() {
            let v = VarId(i as u32 + 1);
            self.mark_used(v, span);
            args.push(match p.mode {
                ParamMode::Var | ParamMode::Out => CallArg::Ref {
                    place: Place::Var(v, span),
                    is_out: p.mode == ParamMode::Out,
                },
                _ => CallArg::Value(Expr {
                    kind: ExprKind::Var(v),
                    ty: p.ty,
                    value: None,
                    span,
                }),
            });
        }
        let ret = self.routines[routine.0 as usize].ret;
        Some(StmtKind::Expr(Expr {
            kind: ExprKind::Call {
                routine,
                args,
                dispatch: Dispatch::Inherited,
            },
            ty: ret.unwrap_or(Ty::Error),
            value: None,
            span,
        }))
    }

    /// The receiver standing for a record type's name (`TPoint.Make`, `TPoint.Create`): there is
    /// no instance (`docs/spec/objects.md` chapter 6). Only its type is looked at.
    pub(super) fn record_type_receiver(id: TypeId, span: TextRange) -> Expr {
        Expr {
            kind: ExprKind::Error,
            ty: Ty::Record(id),
            value: None,
            span,
        }
    }

    /// Whether a receiver has no instance: a class reference, a record type's name, or the name of
    /// a type of the language.
    pub(super) fn no_instance(recv: &Expr) -> bool {
        matches!(recv.ty, Ty::ClassRef(_))
            || matches!(recv.kind, ExprKind::TypeName)
            || (matches!(recv.kind, ExprKind::Error) && matches!(recv.ty, Ty::Record(_)))
    }

    /// `Self` of a record method called on `recv` (`docs/spec/objects.md` chapter 6): a
    /// variable is passed by reference, a field or an element is taken out and put back (as a
    /// `var` argument), and a value that cannot be written (a `const` parameter and its fields,
    /// a constant, a function's result, a property's value, a loop variable) is copied to a
    /// temporary whose reference is passed.
    pub(super) fn record_receiver(&mut self, recv: Expr) -> CallArg {
        match self.receiver_place(&recv) {
            Some(place) => CallArg::Ref {
                place,
                is_out: false,
            },
            None => CallArg::Temp(recv),
        }
    }

    /// The place an expression read, when it can be written (see [`Sema::record_receiver`]).
    pub(super) fn receiver_place(&mut self, e: &Expr) -> Option<Place> {
        if e.value.is_some() {
            return None;
        }
        match &e.kind {
            ExprKind::Var(v) => {
                let kind = self.var_decl(*v).kind;
                if matches!(
                    kind,
                    VarKind::Param(ParamMode::Const) | VarKind::Exception | VarKind::SelfParam
                ) || self.cur.as_ref().is_some_and(|c| c.loop_vars.contains(v))
                {
                    return None;
                }
                self.mark_written(*v, e.span);
                self.mark_value_written(*v);
                Some(Place::Var(*v, e.span))
            }
            ExprKind::Global(g) => {
                if self
                    .cur
                    .as_ref()
                    .is_some_and(|c| c.loop_globals.contains(g))
                {
                    return None;
                }
                Some(Place::Global(*g))
            }
            ExprKind::Field { base, index, name } => Some(Place::Field {
                base: Box::new(self.receiver_place(base)?),
                index: *index,
                ty: e.ty,
                name: name.clone(),
            }),
            ExprKind::Elem { base, index } => Some(Place::Elem {
                base: Box::new(self.receiver_place(base)?),
                index: index.clone(),
                ty: e.ty,
                span: e.span,
            }),
            ExprKind::ObjField { base, slot, name } => Some(Place::ObjField {
                base: base.clone(),
                slot: *slot,
                ty: e.ty,
                name: name.clone(),
            }),
            _ => None,
        }
    }

    /// `TPoint.Create(…)` (`docs/spec/objects.md` chapter 6): the constructor runs on a
    /// temporary with the default value, which is the value of the expression.
    fn record_new(
        &mut self,
        record: TypeId,
        name: &str,
        args: &[ast::Arg],
        span: TextRange,
        range: TextRange,
    ) -> Expr {
        let set: Vec<RoutineId> = self
            .method_set(record, name)
            .into_iter()
            .filter(|r| {
                self.method_of_routine(*r)
                    .is_some_and(|(_, m)| m.kind == MethodKind::Constructor)
            })
            .collect();
        if set.is_empty() {
            for a in args {
                if let Some(v) = a.value() {
                    self.lower_expr(&v);
                }
            }
            let t = self.tn(Ty::Record(record));
            self.type_member_missing(&t, name, range);
            return err_expr(span);
        }
        let id = match self.pick_method(set, args, span, false) {
            Pick::Routine(id) => id,
            Pick::Failed => return err_expr(span),
        };
        self.routine_hint_use(id, range);
        let ty = Ty::Record(record);
        let mut call = self.lower_routine_call(id, args, span);
        let ExprKind::Call { args: a, .. } = &mut call.kind else {
            return call;
        };
        a.insert(
            0,
            CallArg::Temp(Expr {
                kind: ExprKind::Builtin {
                    func: Builtin::Default,
                    args: Vec::new(),
                },
                ty,
                value: None,
                span,
            }),
        );
        call.ty = Ty::Error;
        Expr {
            kind: ExprKind::RecordNew {
                call: Box::new(call),
            },
            ty,
            value: None,
            span,
        }
    }

    /// Whether `name` is a member of a record other than a field (a method, a property, a class
    /// member), without reporting anything: such a name is read through [`Sema::find_member`]
    /// (`docs/spec/objects.md` chapter 6), a field as in M2.
    pub(super) fn record_has_member(&mut self, rec: TypeId, name: &str, range: TextRange) -> bool {
        if !self.types.is_record(rec) {
            return false;
        }
        let found = self.dry_run(|s| s.find_member(rec, name, range));
        match found {
            Ok(Member::RecField { .. }) | Err(false) => false,
            Ok(_) => true,
            // An invisible member: reported by `find_member`, unless it is a field
            Err(true) => !self
                .types
                .fields(Ty::Record(rec))
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(name)),
        }
    }
}
