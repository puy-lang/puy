//! `type` sections, records and static arrays (`docs/spec/types.md` chapters 1, 2 and 4): resolving
//! type declarations (with forward references inside a section), anonymous types, cycle and bound
//! checks, and the constant shapes `(1, 2)` / `(X: 1; Y: 2)`.

use super::hir::*;
use super::types::{
    ClassField, Const, DynWritten, EnumValueDef, Field, MAX_ARRAY_LEN, ProcFamily, ProcParam, Ty,
    TypeDef, TypeId, TypeKind, Visibility, bound_text,
};
use super::{GSym, LSym, Sema, builtins};
use crate::diagnostics::{Code, Diagnostic, Stage};
use crate::source::{FileId, Span, TextRange};
use crate::syntax::SyntaxKind;
use crate::syntax::TokenName;
use crate::syntax::ast::{self, AstNode};

/// The records and static arrays of the outermost `type` section being declared (with the
/// sections nested in its classes), checked for cycles again at its end (E0727), and the types
/// reported so far.
#[derive(Default)]
pub(super) struct CycleCheck {
    pub types: Vec<TypeId>,
    pub reported: Vec<TypeId>,
}

/// An alias (`type X = Y`) of the section being declared, resolved on first use so that it may
/// name a type declared later in the same section.
pub(super) struct PendingAlias {
    lower: String,
    /// The type written after `=`: a name, or a dynamic array (`array of T`, `TArray<T>`), whose
    /// type is found by its element type.
    target: ast::TypeSpec,
    /// The declared name as written.
    name: String,
    span: TextRange,
    state: AliasState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AliasState {
    Unresolved,
    Resolving,
    Done(Ty),
}

/// One declaration of a `type` section between the two passes.
enum Pending {
    Record(TypeId, ast::RecordType),
    Array(TypeId, ast::ArrayType),
    /// `TFoo = class(…) … end` (`docs/spec/objects.md` chapter 1), with its parent when it was
    /// resolved in the first pass (a name not declared in the same section).
    Object(TypeId, ast::ClassType, Option<Ty>),
    /// `IFoo = interface(…) … end` (`docs/spec/objects.md` 7.1), with its parent when it was
    /// resolved in the first pass.
    Interface(TypeId, ast::InterfaceType, Option<Ty>),
    /// `THelper = class helper for T … end` (`docs/spec/objects.md` 10.1): defined in the second
    /// pass (its type and parent may be declared later in the section).
    Helper(TypeId, ast::HelperType),
    /// `TDigit = 0..9`: defined in the second pass, after the enumerations of the section.
    Subrange(TypeId, ast::SubrangeType),
    /// `TF = function(…): T`, `TE = procedure of object`, `TR = reference to procedure`
    /// (`docs/spec/objects.md` 9.1): defined in the second pass.
    Proc(TypeId, ast::TypeSpec),
    Alias(usize),
    /// `type X = array of T` / `TArray<T>` / `set of T` / `class of T`: resolved like an alias
    /// (it names the dynamic array, set or class reference of its element type, base type or
    /// class), and it defines that type.
    Defines(usize),
    Nothing,
}

/// What a name written with type arguments in a type position names.
enum GenericRef {
    /// A generic type of the program or of `System` (or `Ty::Error` once reported).
    Type(Ty),
    /// The built-in `TArray<T>`: its element type, `None` once reported.
    TArray(Option<Ty>),
}

impl<'a, 's> Sema<'a, 's> {
    /// The type name in diagnostics (`docs/spec/types.md` 1.2).
    /// `record type declared here` of an anonymous record in it comes from its mark
    /// ([`Sema::label_anon_records`]).
    pub(super) fn tn(&self, ty: Ty) -> String {
        self.types.marked_name(ty)
    }

    /// A type written in a declaration: a name, or an anonymous `record` / `array` type (a new type
    /// for each position; `docs/spec/types.md` 1.2).
    pub(super) fn resolve_type(&mut self, t: Option<ast::TypeSpec>) -> Ty {
        match t {
            None => Ty::Error,
            Some(ast::TypeSpec::Ref(r)) => self.resolve_type_ref(&r),
            Some(ast::TypeSpec::Record(r)) => {
                let span = r
                    .record_token()
                    .map(|t| t.text_range())
                    .unwrap_or(r.syntax().text_range());
                let id = self.add_type(None, span);
                self.types.anon_record_files.insert(id, self.input.file);
                self.define_record(id, &r);
                Ty::Record(id)
            }
            Some(ast::TypeSpec::Array(a)) => {
                let span = a
                    .array_token()
                    .map(|t| t.text_range())
                    .unwrap_or(a.syntax().text_range());
                if !a.has_brackets() {
                    let elem = self.resolve_type(a.element());
                    return self.dyn_array_of(
                        elem,
                        DynWritten::Anonymous {
                            generic: false,
                            span,
                        },
                    );
                }
                let id = self.add_type(None, span);
                self.define_array(id, &a);
                Ty::Array(id)
            }
            // Only a type declaration has these forms (the parser reads them there)
            Some(
                ast::TypeSpec::Class(_)
                | ast::TypeSpec::Interface(_)
                | ast::TypeSpec::Helper(_)
                | ast::TypeSpec::Distinct(_),
            ) => Ty::Error,
            Some(ast::TypeSpec::ClassRef(r)) => self.class_ref_type(
                &r,
                DynWritten::Anonymous {
                    generic: false,
                    span: ast::trimmed_range(r.syntax()),
                },
            ),
            Some(ast::TypeSpec::Enum(e)) => {
                // An enumeration needs a name to qualify its values (`docs/spec/types.md` 8.1)
                let range = ast::trimmed_range(e.syntax());
                self.error(
                    Code::E(736),
                    "an enumeration type must be declared with a name",
                    range,
                )
                .notes
                .push("declare the enumeration as a named type, as in `type TColor = (Red, Green, Blue)`".into());
                Ty::Error
            }
            Some(ast::TypeSpec::Subrange(r)) => {
                let id = self.add_type(None, ast::trimmed_range(r.syntax()));
                match self.subrange_kind(&r) {
                    Some(kind) => {
                        self.types.get_mut(id).kind = kind;
                        Ty::Subrange(id)
                    }
                    None => Ty::Error,
                }
            }
            Some(ast::TypeSpec::Set(st)) => {
                let span = st
                    .set_token()
                    .map(|t| t.text_range())
                    .unwrap_or(st.syntax().text_range());
                let base = self.resolve_type(st.base());
                self.set_of(
                    base,
                    &st,
                    DynWritten::Anonymous {
                        generic: false,
                        span,
                    },
                )
            }
            Some(t @ (ast::TypeSpec::Proc(_) | ast::TypeSpec::Reference(_))) => {
                let span = ast::trimmed_range(t.syntax());
                let (id, ty) = self.add_proc_type(None, &t, span);
                self.define_proc_type(id, &t);
                ty
            }
            Some(ast::TypeSpec::Nested(n)) => match self.nested_type_ref(&n) {
                Ty::Helper(_) => {
                    // As for a name (`resolve_type_ref`; `docs/spec/objects.md` 10.2)
                    let range = ast::trimmed_range(n.syntax());
                    self.error(Code::E(753), "a helper cannot be used as a type", range);
                    Ty::Error
                }
                ty => ty,
            },
            Some(ast::TypeSpec::Generic(g)) => {
                let span = g.syntax().text_range();
                match self.generic_type_ref(&g) {
                    GenericRef::Type(t) => t,
                    GenericRef::TArray(Some(elem)) => self.dyn_array_of(
                        elem,
                        DynWritten::Anonymous {
                            generic: true,
                            span,
                        },
                    ),
                    GenericRef::TArray(None) => Ty::Error,
                }
            }
        }
    }

    /// A generic dynamic array type (`TRow<T> = array of T`, `TRow<T> = TArray<T>`;
    /// `docs/spec/generics.md` 2.1): its element type, with the type parameters in scope.
    fn define_generic_dyn(&mut self, id: TypeId, t: &ast::TypeSpec) {
        let (elem, generic) = self.with_type_params(id, |s| match t {
            ast::TypeSpec::Array(a) => (s.resolve_type(a.element()), false),
            ast::TypeSpec::Generic(g) => (s.resolve_type(g.arg()), true),
            _ => (Ty::Error, false),
        });
        self.types.get_mut(id).kind = TypeKind::DynArray { elem, generic };
    }

    /// The set of a base type (`docs/spec/types.md` 8.3): an ordinal type whose ordinals are all
    /// within 0..65535 (E0737 otherwise).
    fn set_of(&mut self, base: Ty, st: &ast::SetType, written: DynWritten) -> Ty {
        if base.is_error() {
            return Ty::Error;
        }
        let ok = self.types.root(base).is_ordinal()
            && self
                .types
                .ord_range(base)
                .is_some_and(|(lo, hi)| lo >= 0 && hi <= 65535);
        if !ok {
            let range = st
                .base()
                .map_or(st.syntax().text_range(), |b| ast::trimmed_range(b.syntax()));
            let t = self.tn(base);
            self.error(
                Code::E(737),
                format!("set base type `{t}` must have ordinal values in 0..65535"),
                range,
            )
            .notes
            .push("use a subrange type such as `0..255` as the base type".into());
            return Ty::Error;
        }
        self.types.set_of(base, written)
    }

    /// The dynamic array of an element type (the error type stays the error type).
    fn dyn_array_of(&mut self, elem: Ty, written: DynWritten) -> Ty {
        if elem.is_error() {
            return Ty::Error;
        }
        self.types.dyn_array(elem, written)
    }

    /// A generic type written with type arguments in a type position (`TBox<Integer>`,
    /// `Unit.TBox<Integer>`; `docs/spec/generics.md` 1.3), or the built-in `TArray<T>` (also
    /// written `System.TArray<T>`; 6.2) when no generic type of the program has the name.
    fn generic_type_ref(&mut self, g: &ast::GenericTypeRef) -> GenericRef {
        let Some(q) = g.name() else {
            return GenericRef::TArray(self.generic_elem(g));
        };
        let idents: Vec<_> = q.idents().collect();
        let Some(last) = idents.last() else {
            return GenericRef::TArray(self.generic_elem(g));
        };
        let name = last.name().to_string();
        let name_range = last.text_range();
        let scope = if idents.len() > 1 {
            let unit: Vec<String> = idents[..idents.len() - 1]
                .iter()
                .map(|t| t.name().to_string())
                .collect();
            let lower = unit.join(".").to_ascii_lowercase();
            match self.unit_named(&lower) {
                Some(u) => super::generics::GenericScope::Unit(u),
                // `System.TProc<…>`, `System.TArray<…>` (`docs/spec/generics.md` 6.2)
                None if let Some(u) = self.builtin_unit_named(&lower) => {
                    super::generics::GenericScope::Unit(u)
                }
                // A generic type nested in a class or a record (`TOuter.TInner<…>`;
                // `docs/spec/generics.md` 2.5)
                None if let Some(c) = {
                    let chain: Vec<(String, TextRange)> = idents[..idents.len() - 1]
                        .iter()
                        .map(|t| t.name_and_range())
                        .collect();
                    match self.chain_owner(&chain) {
                        Ok(c) => c,
                        Err(()) => return GenericRef::Type(Ty::Error),
                    }
                } =>
                {
                    super::generics::GenericScope::Type(c)
                }
                None => {
                    let text = q.text();
                    self.error(
                        Code::E(700),
                        format!("undeclared identifier `{text}`"),
                        q.syntax().text_range(),
                    );
                    return GenericRef::Type(Ty::Error);
                }
            }
        } else {
            super::generics::GenericScope::Visible
        };
        let args_range = g.args_range().unwrap_or(g.syntax().text_range());
        if g.syntax()
            .children()
            .any(|n| n.kind() == SyntaxKind::ERROR_NODE)
        {
            // A type argument the parser rejected (an anonymous type: E0404): the arguments are
            // not counted and the name is not resolved either (the parser's error is enough)
            return GenericRef::Type(Ty::Error);
        }
        let args: Vec<ast::TypeSpec> = g.args().collect();
        // A qualified name that is not generic: a type of the unit or a nested type, or nothing
        let qualified_type = match &scope {
            super::generics::GenericScope::Type(c) => Some(matches!(
                self.find_static(*c, &name).map(|(_, s)| s.kind),
                Some(super::types::StaticKind::Type(_))
            )),
            super::generics::GenericScope::Unit(u) => {
                let u = *u;
                Some(matches!(
                    self.dry_run(|s| s.resolve_in_unit(u, &name, name_range)),
                    super::Resolved::Type(_)
                ))
            }
            super::generics::GenericScope::Visible => None,
        };
        let count = args.len();
        if let Some(t) = self.generic_type_with_args(&name, name_range, args, args_range, scope) {
            return GenericRef::Type(t);
        }
        if name.eq_ignore_ascii_case("tarray") {
            match scope {
                super::generics::GenericScope::Visible => {
                    return GenericRef::TArray(self.generic_elem(g));
                }
                super::generics::GenericScope::Unit(super::UnitRef::System) => {
                    // The parser takes any number of type arguments after a qualified name
                    if count != 1 {
                        self.tarray_arity_error(count, args_range);
                        return GenericRef::Type(Ty::Error);
                    }
                    return GenericRef::TArray(self.generic_elem(g));
                }
                _ => {}
            }
        }
        // A type that is not generic, or an undeclared name (`docs/spec/generics.md` 2.3)
        if let Some(is_type) = qualified_type {
            if is_type {
                let text = q.text();
                self.error(
                    Code::E(761),
                    format!("`{text}` is not generic"),
                    g.syntax().text_range(),
                );
            } else {
                let text = q.text();
                self.error(
                    Code::E(700),
                    format!("undeclared identifier `{text}`"),
                    q.syntax().text_range(),
                );
            }
            return GenericRef::Type(Ty::Error);
        }
        self.not_generic_name(&name, name_range, g.syntax().text_range());
        GenericRef::Type(Ty::Error)
    }

    /// A type nested in an instantiation, written in a type position
    /// (`TList<Integer>.TEnumerator`, `TOuter<Integer>.TInner<string>`; `docs/spec/generics.md`
    /// 2.5): the same type as the one named through an alias of the instantiation
    /// (`TIntList = TList<Integer>; TIntList.TEnumerator`).
    pub(super) fn nested_type_ref(&mut self, n: &ast::NestedTypeRef) -> Ty {
        let base = self.resolve_type(n.base());
        let Some(q) = n.name() else {
            // No name after `.` (reported by the parser)
            return Ty::Error;
        };
        if base.is_error()
            || n.syntax()
                .children()
                .any(|c| c.kind() == SyntaxKind::ERROR_NODE)
        {
            return Ty::Error;
        }
        let chain: Vec<(String, TextRange)> = q.idents().map(|t| t.name_and_range()).collect();
        let whole = ast::trimmed_range(n.syntax());
        let Some(((name, name_range), init)) = chain.split_last() else {
            return Ty::Error;
        };
        let init_end = init.last().map_or(whole.start(), |(_, r)| r.end());
        let Some(args_range) = n.args_range() else {
            // A generic type nested there written without type arguments (2.3)
            if let Some(c) =
                super::statics::nested_owner(self.dry_run(|s| {
                    s.nested_type(base, init, TextRange::new(whole.start(), init_end))
                }))
                && self.static_of(c, name).is_none()
                && !self
                    .generic_types_named(
                        &name.to_ascii_lowercase(),
                        &super::generics::GenericScope::Type(c),
                    )
                    .is_empty()
            {
                self.error(
                    Code::E(761),
                    format!("generic type `{name}` needs type arguments"),
                    *name_range,
                );
                return Ty::Error;
            }
            return self.nested_type(base, &chain, whole);
        };
        // The last name has type arguments: a generic type nested in the class the names before
        // it lead to
        let outer = self.nested_type(base, init, TextRange::new(whole.start(), init_end));
        let Some(c) = super::statics::nested_owner(outer) else {
            if !outer.is_error() {
                let text = self.src(TextRange::new(whole.start(), name_range.end()));
                self.error(Code::E(708), format!("`{text}` is not a type"), whole);
            }
            return Ty::Error;
        };
        let args: Vec<ast::TypeSpec> = n.args().collect();
        if let Some(t) = self.generic_type_with_args(
            name,
            *name_range,
            args,
            args_range,
            super::generics::GenericScope::Type(c),
        ) {
            return t;
        }
        // A nested type that is not generic, or no such member (`docs/spec/generics.md` 2.3)
        match self.static_of(c, name).map(|(_, s)| s.kind) {
            Some(super::types::StaticKind::Type(_)) => {
                self.error(
                    Code::E(761),
                    format!("`{name}` is not generic"),
                    TextRange::new(name_range.start(), whole.end()),
                );
            }
            _ => {
                let t = self.tn(outer);
                self.error(
                    Code::E(700),
                    format!("`{t}` has no member `{name}`"),
                    *name_range,
                );
            }
        }
        Ty::Error
    }

    /// E0761 for the built-in `TArray` written with other than one type argument
    /// (`System.TArray<A, B>`; `docs/spec/generics.md` 6.2), in a type and in an expression.
    pub(super) fn tarray_arity_error(&mut self, count: usize, args_range: TextRange) {
        self.error(
            Code::E(761),
            format!("`TArray` expects 1 type argument, found {count}"),
            args_range,
        );
    }

    /// A name written with type arguments that names no generic type (`docs/spec/generics.md`
    /// 2.3): E0761 `is not generic` at `args_range` for a type, E0708 for a name that is not a
    /// type, and the E0700 of `resolve_ident` for an undeclared name. Shared by the type position
    /// and the type of a body's heading (`procedure TPlain<T>.Run;`).
    pub(super) fn not_generic_name(
        &mut self,
        name: &str,
        name_range: TextRange,
        args_range: TextRange,
    ) {
        let lower = name.to_ascii_lowercase();
        let is_type = Ty::from_name(&lower).is_some()
            || matches!(self.lookup_type_symbol(&lower), Some(Some(_)))
            || matches!(
                self.dry_run(|s| s.resolve_ident(name, name_range)),
                super::Resolved::Type(_)
            );
        if is_type {
            self.error(Code::E(761), format!("`{name}` is not generic"), args_range);
        } else if !matches!(self.resolve_ident(name, name_range), super::Resolved::Error) {
            self.error(Code::E(708), format!("`{name}` is not a type"), name_range);
        }
    }

    /// The element type of the built-in `TArray<T>`, written `TArray<T>` or `System.TArray<T>`. A
    /// `TArray` without type parameters declared by the program is another name and does not
    /// hide it (`docs/spec/generics.md` 2.3, 6.2).
    fn generic_elem(&mut self, g: &ast::GenericTypeRef) -> Option<Ty> {
        let elem = self.resolve_type(g.arg());
        (!elem.is_error()).then_some(elem)
    }

    /// An interface's method declared with type parameters is E0757 and is not declared
    /// (`docs/spec/generics.md` 2.1: an interface's methods use a table like the virtual ones).
    pub(super) fn generic_interface_method(&mut self, m: &ast::MethodDecl) -> bool {
        let Some(tp) = m.header().and_then(|h| h.type_params()) else {
            return false;
        };
        self.error(
            Code::E(757),
            "a virtual method cannot have type parameters",
            tp.syntax().text_range(),
        );
        true
    }

    /// An open array parameter `array of T` (`docs/spec/types.md` 2.4).
    pub(super) fn open_array_of(&mut self, a: &ast::ArrayType) -> Ty {
        let span = a
            .array_token()
            .map(|t| t.text_range())
            .unwrap_or(a.syntax().text_range());
        let elem = if a.of_const() {
            // `array of const` is `array of TValue` (`docs/spec/rtti.md` 4.1)
            self.tvalue_type()
        } else {
            self.resolve_type(a.element())
        };
        if elem.is_error() {
            return Ty::Error;
        }
        match self.dyn_array_of(
            elem,
            DynWritten::Anonymous {
                generic: false,
                span,
            },
        ) {
            Ty::DynArray(id) => Ty::OpenArray(id),
            t => t,
        }
    }

    /// A new procedural type (`docs/spec/objects.md` 9.1) for a `ProcType` / `RefType`, not yet
    /// defined: its id and its type (the family is known from the syntax).
    fn add_proc_type(
        &mut self,
        name: Option<String>,
        t: &ast::TypeSpec,
        span: TextRange,
    ) -> (TypeId, Ty) {
        let id = self.add_type(name, span);
        let plain = matches!(t, ast::TypeSpec::Proc(p) if !p.of_object());
        (
            id,
            if plain {
                Ty::Proc(id)
            } else {
                Ty::Callable(id)
            },
        )
    }

    /// Defines a procedural type (`docs/spec/objects.md` 9.1): its parameters (no default values:
    /// E0752) and return type, and the calling conventions (W0009).
    fn define_proc_type(&mut self, id: TypeId, t: &ast::TypeSpec) {
        let (family, p) = match t {
            ast::TypeSpec::Proc(p) if p.of_object() => (ProcFamily::Method, Some(p.clone())),
            ast::TypeSpec::Proc(p) => (ProcFamily::Plain, Some(p.clone())),
            ast::TypeSpec::Reference(r) => (ProcFamily::Reference, r.proc_type()),
            _ => return,
        };
        let mut params = Vec::new();
        let mut ret = None;
        if let Some(p) = p {
            for group in p.params() {
                let ty = match group.type_ref() {
                    // `array of T` in a parameter is an open array (`docs/spec/types.md` 2.4)
                    Some(ast::TypeSpec::Array(a)) if !a.has_brackets() => self.open_array_of(&a),
                    t => self.resolve_type(t),
                };
                if let Some(d) = group.default_node() {
                    self.error(
                        Code::E(752),
                        "a procedural type cannot have default parameter values",
                        ast::trimmed_range(&d),
                    );
                }
                for n in group.names() {
                    params.push(ProcParam {
                        name: self.text(&n).to_string(),
                        ty,
                        mode: group.mode(),
                    });
                }
            }
            if p.is_function() {
                ret = Some(match p.return_type() {
                    Some(r) => self.resolve_type(Some(r)),
                    None => Ty::Error,
                });
            }
            for d in p.directives() {
                if let (Some(kind), Some(kw)) = (d.kind(), d.keyword()) {
                    let del = super::semicolon_with_directive(d.syntax());
                    self.routine_directive_at(kind, &kw, del);
                }
            }
        }
        self.types.get_mut(id).kind = TypeKind::Proc {
            family,
            params,
            ret,
        };
    }

    pub(super) fn add_type(&mut self, name: Option<String>, span: TextRange) -> TypeId {
        let full_name = name.as_ref().map(|n| self.qualified_type_name(n));
        self.types.add(TypeDef {
            name,
            full_name,
            kind: TypeKind::Pending,
            span,
        })
    }

    /// The built-in helper `lower` of `System` (`docs/spec/objects.md` 10.7) when its declaration
    /// at `span` is in the part of `System` written in Pascal: that declaration gives the helper
    /// its members besides the built-in one (`docs/spec/stdlib/System/README.md`).
    fn system_helper_id(&mut self, lower: &str, span: TextRange) -> Option<TypeId> {
        if !self.is_system_source() || self.class_scope().is_some() || self.cur.is_some() {
            return None;
        }
        let id = super::helpers::builtin_helper_id(lower)?;
        let full_name = self.qualified_type_name(self.types.get(id).name.as_deref()?);
        let def = self.types.get_mut(id);
        def.full_name = Some(full_name);
        def.span = span;
        Some(id)
    }

    /// `Hello.TPoint` for a type of the program, `Hello.Routine.TPoint` for a local type,
    /// `Hello.TFoo.TItem` for a nested type (`docs/spec/bytecode.md` 3.2).
    fn qualified_type_name(&self, name: &str) -> String {
        if let Some(c) = self.class_scope() {
            return format!("{}.{name}", self.types.full_name(Ty::Class(c)));
        }
        match &self.cur {
            Some(c) if !c.is_main => format!("{}.{}.{name}", self.program_name, c.qual_name),
            _ => format!("{}.{name}", self.program_name),
        }
    }

    /// Looks a name up as a type: `Some(Some(ty))` for a declared type, `Some(None)` for a name
    /// declared as something else, `None` if no declaration has the name.
    pub(super) fn lookup_type_symbol(&mut self, lower: &str) -> Option<Option<Ty>> {
        if let Some(p) = self.routine_type_param(lower) {
            return Some(Some(Ty::Param(p)));
        }
        let mut found: Option<Option<Ty>> = self.lookup_scopes(lower).map(|(_, sym)| match sym {
            LSym::Type { ty, .. } => Some(ty),
            _ => None,
        });
        if found.is_none()
            && let Some(t) = self.scoped_param(lower)
        {
            return Some(Some(t));
        }
        if found.is_none()
            && let Some((_, st)) = self.scoped_static(lower)
        {
            // A type parameter is not an alias of the section, even one of the same name
            // (`docs/spec/generics.md` 2.2)
            if let super::types::StaticKind::Type(t @ Ty::Param(_)) = st.kind {
                return Some(Some(t));
            }
            found = Some(match st.kind {
                super::types::StaticKind::Type(t) => Some(t),
                _ => None,
            });
        }
        if found.is_none() {
            found = self
                .global_scope
                .iter()
                .rev()
                .find(|(n, _)| n == lower)
                .map(|(_, sym)| match sym {
                    GSym::Type { ty, .. } => Some(*ty),
                    _ => None,
                });
        }
        // An alias of the section being declared resolves on first use
        if let Some(Some(_)) = found
            && let Some(i) = self.pending_aliases.iter().position(|a| a.lower == lower)
        {
            return Some(Some(self.resolve_alias(i)));
        }
        found
    }

    /// The name declaring the type [`Sema::lookup_type_symbol`] finds for `lower`, recorded as
    /// used at `range` (`docs/spec/attributes.md` 6.2).
    pub(super) fn type_symbol_use(&mut self, lower: &str, range: TextRange) {
        let decl = match self.lookup_scopes(lower) {
            Some((up, sym @ LSym::Type { .. })) => Some(self.span(self.lsym_span(up, &sym))),
            Some(_) => None,
            None => match self.scoped_static(lower) {
                Some((owner, st)) => self.static_decl_span(owner, &st),
                None => self
                    .global_scope
                    .iter()
                    .rev()
                    .find(|(n, _)| n == lower)
                    .and_then(|(_, sym)| match sym {
                        GSym::Type { span, .. } => Some(self.span(*span)),
                        _ => None,
                    }),
            },
        };
        if let Some(d) = decl {
            self.hint_use(d, range);
        }
    }

    fn resolve_alias(&mut self, i: usize) -> Ty {
        match self.pending_aliases[i].state {
            AliasState::Done(ty) => ty,
            AliasState::Resolving => {
                // `type A = B; B = A;`
                let span = self.pending_aliases[i].span;
                let name = self.src(span);
                self.error(Code::E(727), format!("type `{name}` contains itself"), span);
                self.pending_aliases[i].state = AliasState::Done(Ty::Error);
                Ty::Error
            }
            AliasState::Unresolved => {
                self.pending_aliases[i].state = AliasState::Resolving;
                let target = self.pending_aliases[i].target.clone();
                let ty = match &target {
                    ast::TypeSpec::Ref(r) => self.resolve_type_ref(r),
                    ast::TypeSpec::Nested(n) => self.nested_type_ref(n),
                    // Declared with `Pending::Proc`, never as an alias
                    ast::TypeSpec::Proc(_) | ast::TypeSpec::Reference(_) => Ty::Error,
                    ast::TypeSpec::Array(a) => {
                        let elem = self.resolve_type(a.element());
                        self.named_dyn_array(i, elem)
                    }
                    // An instantiation (`TIntBox = TBox<Integer>`; `docs/spec/generics.md` 1.3),
                    // or `TArray<T>`
                    ast::TypeSpec::Generic(g) => match self.generic_type_ref(g) {
                        GenericRef::Type(t) => t,
                        GenericRef::TArray(Some(elem)) => self.named_dyn_array(i, elem),
                        GenericRef::TArray(None) => Ty::Error,
                    },
                    ast::TypeSpec::Set(st) => {
                        let base = self.resolve_type(st.base());
                        let name = self.pending_aliases[i].name.clone();
                        let full_name = self.qualified_type_name(&name);
                        let span = self.pending_aliases[i].span;
                        self.set_of(
                            base,
                            st,
                            DynWritten::Named {
                                name,
                                full_name,
                                span,
                            },
                        )
                    }
                    ast::TypeSpec::ClassRef(r) => {
                        let name = self.pending_aliases[i].name.clone();
                        let full_name = self.qualified_type_name(&name);
                        let span = self.pending_aliases[i].span;
                        self.class_ref_type(
                            r,
                            DynWritten::Named {
                                name,
                                full_name,
                                span,
                            },
                        )
                    }
                    ast::TypeSpec::Distinct(d) => self.distinct_type(i, d),
                    ast::TypeSpec::Record(_)
                    | ast::TypeSpec::Class(_)
                    | ast::TypeSpec::Interface(_)
                    | ast::TypeSpec::Helper(_)
                    | ast::TypeSpec::Enum(_)
                    | ast::TypeSpec::Subrange(_) => Ty::Error,
                };
                if let AliasState::Resolving = self.pending_aliases[i].state {
                    self.pending_aliases[i].state = AliasState::Done(ty);
                }
                match self.pending_aliases[i].state {
                    AliasState::Done(t) => t,
                    _ => ty,
                }
            }
        }
    }

    /// The distinct type `type X = type Y` declares (`docs/spec/types.md` 1.3): E0769 unless the
    /// root of `Y` is one of the simple types.
    fn distinct_type(&mut self, i: usize, d: &ast::DistinctType) -> Ty {
        let base = match d.target() {
            Some(ast::TypeSpec::Ref(r)) => self.resolve_type_ref(&r),
            Some(ast::TypeSpec::Generic(g)) => match self.generic_type_ref(&g) {
                GenericRef::Type(t) => t,
                GenericRef::TArray(Some(elem)) => self.dyn_array_of(
                    elem,
                    DynWritten::Anonymous {
                        generic: true,
                        span: ast::trimmed_range(g.syntax()),
                    },
                ),
                GenericRef::TArray(None) => Ty::Error,
            },
            Some(ast::TypeSpec::Nested(n)) => self.nested_type_ref(&n),
            _ => Ty::Error,
        };
        if base.is_error() {
            return Ty::Error;
        }
        let root = self.types.root(base);
        if !Self::distinct_root(root) {
            let target = d
                .target()
                .map(|t| ast::trimmed_range(t.syntax()))
                .unwrap_or(d.syntax().text_range());
            let d_err = self.error(
                Code::E(769),
                format!("cannot declare a distinct type of `{}`", self.tn(base)),
                target,
            );
            d_err.notes.push(
                "a distinct type can be declared for integer, floating-point, `Currency`, `Char`, `Boolean`, `string` and `UTF8String` types"
                    .into(),
            );
            if let Some(kw) = d.type_token() {
                // `type X = type Y` → `type X = Y`
                let edit = TextRange::new(kw.text_range().start(), target.start());
                let fix = crate::diagnostics::Fix::suggestion(
                    "remove `type` to declare an alias".to_string(),
                    vec![crate::diagnostics::TextEdit::replace(
                        self.input.file,
                        edit,
                        String::new(),
                    )],
                );
                self.last_mut().fixes.push(fix);
            }
            return Ty::Error;
        }
        let name = self.pending_aliases[i].name.clone();
        let span = self.pending_aliases[i].span;
        let id = self.add_type(Some(name), span);
        self.types.get_mut(id).kind = TypeKind::Distinct { base };
        Ty::Distinct(id)
    }

    /// Whether a distinct type can have `root` as its root type (`docs/spec/types.md` 1.3).
    pub(super) fn distinct_root(root: Ty) -> bool {
        root.is_integer()
            || matches!(
                root,
                Ty::Single
                    | Ty::Double
                    | Ty::Currency
                    | Ty::Char
                    | Ty::Boolean
                    | Ty::String
                    | Ty::Utf8String
            )
    }

    /// The dynamic array a `type` declaration names (`type TInts = array of Integer`).
    fn named_dyn_array(&mut self, i: usize, elem: Ty) -> Ty {
        let name = self.pending_aliases[i].name.clone();
        let full_name = self.qualified_type_name(&name);
        let span = self.pending_aliases[i].span;
        self.dyn_array_of(
            elem,
            DynWritten::Named {
                name,
                full_name,
                span,
            },
        )
    }

    /// A `type` section (`docs/spec/types.md` chapter 1). All names are declared first, so a type
    /// may refer to a type declared later in the same section.
    pub(super) fn declare_types(&mut self, section: &ast::TypeSection) {
        // A section nested in a class body keeps the enclosing section's pending aliases, and
        // its classes get their methods with the enclosing section's (`docs/spec/objects.md` 2.2)
        let alias_base = self.pending_aliases.len();
        let nested = self.class_scope().is_some();
        let mut items: Vec<(String, TextRange, Ty, Pending)> = Vec::new();
        // The generic dynamic array types, defined once every name of the section is declared
        let mut generic_dyns: Vec<(TypeId, ast::TypeSpec)> = Vec::new();
        let classes = self.section_classes(section);
        // The constraints of the instantiations written in the section are checked once its
        // classes have their parents, interfaces and methods (`docs/spec/generics.md` 4.2)
        let deferred = (!nested).then(|| self.begin_deferred_checks());
        // The instantiations asked for from here on get their members at the end of the
        // section, and the cycles through them are checked then (E0727)
        let instance_base = self.types.instances.len();
        if !nested {
            // The attribute classes wait for the methods of the section's classes
            self.attr_defer += 1;
        }
        for decl in section.decls() {
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            if let Some(tp) = decl.type_params() {
                // A generic type (`docs/spec/generics.md` 1.1, 2.1). Its name, found by the
                // number of type parameters, is another name than one without them (2.3), so it
                // neither collides with nor hides a declaration without them
                let Some(ty) = self.declare_generic_type(&decl, &tp, section, &name, range) else {
                    continue;
                };
                let pending = match (ty, decl.type_spec()) {
                    (Ty::Class(id), Some(ast::TypeSpec::Class(c))) => Pending::Object(id, c, None),
                    (Ty::Record(id), Some(ast::TypeSpec::Record(r))) => Pending::Record(id, r),
                    (Ty::Intf(id), Some(ast::TypeSpec::Interface(it))) => {
                        Pending::Interface(id, it, None)
                    }
                    (Ty::Proc(id) | Ty::Callable(id), Some(t)) => Pending::Proc(id, t),
                    (Ty::Array(id), Some(ast::TypeSpec::Array(a))) => Pending::Array(id, a),
                    (Ty::DynArray(id), Some(t)) => {
                        generic_dyns.push((id, t));
                        Pending::Nothing
                    }
                    _ => unreachable!("a generic type is declared with the form of its type"),
                };
                items.push((name, range, ty, pending));
                continue;
            }
            if decl.type_spec().is_none() {
                // A type the parser reported (a class, M3; `object`, removed): the name is known,
                // so uses of it do not pile on E0700, and a later full declaration of a
                // forward-declared class is not E0701
                if self.find_declared(&lower).is_none() {
                    self.bind_type(lower, Ty::Error, range);
                }
                continue;
            }
            if let Some(ast::TypeSpec::Class(c)) = decl.type_spec()
                && c.is_forward()
            {
                // A forward declaration: the full declaration later in the section declares the
                // name (`docs/spec/objects.md` chapter 1)
                if !classes
                    .iter()
                    .any(|(n, c, r)| *n == lower && !c.is_forward() && r.start() > range.start())
                {
                    let d = Diagnostic::error(
                        Code::E(723),
                        format!("forward declaration of class `{name}` has no body"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "");
                    self.push(d);
                    if !self.check_duplicate(&lower, range) {
                        self.bind_type(lower, Ty::Error, range);
                    }
                }
                continue;
            }
            if let Some(ast::TypeSpec::Interface(it)) = decl.type_spec()
                && it.is_forward()
            {
                // `docs/spec/objects.md` 7.1, as for a class
                let later = section.decls().any(|d| {
                    d.name().is_some_and(|n| {
                        n.name().eq_ignore_ascii_case(&name) && n.text_range().start() > range.start()
                    }) && matches!(d.type_spec(), Some(ast::TypeSpec::Interface(i)) if !i.is_forward())
                });
                if !later {
                    let d = Diagnostic::error(
                        Code::E(723),
                        format!("forward declaration of interface `{name}` has no body"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "");
                    self.push(d);
                    if !self.check_duplicate(&lower, range) {
                        self.bind_type(lower, Ty::Error, range);
                    }
                }
                continue;
            }
            if self.check_duplicate(&lower, range) {
                continue;
            }
            let (ty, pending) = match decl.type_spec() {
                Some(ast::TypeSpec::Record(r)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    (Ty::Record(id), Pending::Record(id, r))
                }
                Some(ast::TypeSpec::Array(a)) if a.has_brackets() => {
                    let id = self.add_type(Some(name.clone()), range);
                    (Ty::Array(id), Pending::Array(id, a))
                }
                Some(ast::TypeSpec::Class(c)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    // A parent declared outside the section is resolved now, once
                    let generic_parent = c.parent_list().is_some_and(|l| {
                        l.children().any(|n| {
                            matches!(
                                n.kind(),
                                SyntaxKind::GENERIC_TYPE_REF | SyntaxKind::NESTED_TYPE_REF
                            )
                        })
                    });
                    let parent = c
                        .parent()
                        .filter(|p| !generic_parent && !Self::names_section_type(section, p))
                        .map(|p| self.resolve_type_ref(&p));
                    (Ty::Class(id), Pending::Object(id, c, parent))
                }
                Some(ast::TypeSpec::Interface(it)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    let parent = it
                        .parent()
                        .filter(|p| !Self::names_section_type(section, p))
                        .map(|p| self.resolve_type_ref(&p));
                    (Ty::Intf(id), Pending::Interface(id, it, parent))
                }
                Some(t @ ast::TypeSpec::ClassRef(_)) => {
                    // Resolved like an alias (it names the class reference of its class, which
                    // may be declared later in the section)
                    self.pending_aliases.push(PendingAlias {
                        lower: lower.clone(),
                        target: t,
                        name: name.clone(),
                        span: range,
                        state: AliasState::Unresolved,
                    });
                    (Ty::Error, Pending::Defines(self.pending_aliases.len() - 1))
                }
                Some(ast::TypeSpec::Helper(h)) => {
                    let id = match self.system_helper_id(&lower, range) {
                        Some(id) => id,
                        None => self.add_type(Some(name.clone()), range),
                    };
                    // Still declared, so its uses add no other error
                    self.helper_in_generic(id, &h);
                    (Ty::Helper(id), Pending::Helper(id, h))
                }
                Some(ast::TypeSpec::Enum(e)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    self.define_enum(id, &e);
                    (Ty::Enum(id), Pending::Nothing)
                }
                Some(ast::TypeSpec::Subrange(r)) => {
                    let id = self.add_type(Some(name.clone()), range);
                    (Ty::Subrange(id), Pending::Subrange(id, r))
                }
                Some(t @ (ast::TypeSpec::Proc(_) | ast::TypeSpec::Reference(_))) => {
                    // Its parameters may name types declared later in the section (an event
                    // type before its class), so it is defined in the second pass
                    let (id, ty) = self.add_proc_type(Some(name.clone()), &t, range);
                    (ty, Pending::Proc(id, t))
                }
                Some(
                    t @ (ast::TypeSpec::Ref(_)
                    | ast::TypeSpec::Nested(_)
                    | ast::TypeSpec::Array(_)
                    | ast::TypeSpec::Generic(_)
                    | ast::TypeSpec::Set(_)
                    | ast::TypeSpec::Distinct(_)),
                ) => {
                    // A name, or a dynamic array (whose type is found by its element type)
                    let defines = !matches!(t, ast::TypeSpec::Ref(_) | ast::TypeSpec::Nested(_));
                    self.pending_aliases.push(PendingAlias {
                        lower: lower.clone(),
                        target: t,
                        name: name.clone(),
                        span: range,
                        state: AliasState::Unresolved,
                    });
                    let i = self.pending_aliases.len() - 1;
                    (
                        Ty::Error,
                        if defines {
                            Pending::Defines(i)
                        } else {
                            Pending::Alias(i)
                        },
                    )
                }
                None => (Ty::Error, Pending::Nothing),
            };
            self.nest_in_generic(ty);
            self.local_in_generic(ty, &name);
            self.bind_type(lower, ty, range);
            items.push((name, range, ty, pending));
        }
        for (id, t) in generic_dyns {
            self.define_generic_dyn(id, &t);
        }
        // The attributes of the declarations see every name of the section (`docs/spec/attributes.md` 2)
        for decl in section.decls() {
            self.type_decl_attributes(&decl, section);
        }
        // A class's fields are numbered after its parent's, so the parent must be complete
        self.incomplete_classes
            .extend(items.iter().filter_map(|(_, _, _, p)| match p {
                Pending::Object(id, _, _) => Some(*id),
                _ => None,
            }));
        let mut defined = Vec::new();
        for (name, range, ty, pending) in items {
            let is_dyn = matches!(pending, Pending::Defines(_));
            let generic = match pending {
                Pending::Defines(i) => {
                    matches!(self.pending_aliases[i].target, ast::TypeSpec::Generic(_))
                }
                _ => false,
            };
            // The type's own declaration is inside it (`docs/spec/attributes.md` 6.2)
            self.push_hint_scope(self.span(range));
            let (ty, defines) = match pending {
                Pending::Record(id, r) => {
                    self.define_record(id, &r);
                    defined.push(id);
                    (ty, true)
                }
                Pending::Array(id, a) => {
                    self.with_type_params(id, |s| s.define_array(id, &a));
                    defined.push(id);
                    (ty, true)
                }
                Pending::Object(id, c, parent) => {
                    self.define_object(id, &c, parent);
                    (ty, true)
                }
                Pending::Interface(id, it, parent) => {
                    self.with_type_params(id, |s| s.define_interface(id, &it, parent));
                    (ty, true)
                }
                Pending::Helper(id, h) => {
                    self.define_helper(id, &h);
                    (ty, true)
                }
                Pending::Subrange(id, r) => match self.subrange_kind(&r) {
                    Some(kind) => {
                        self.types.get_mut(id).kind = kind;
                        (ty, true)
                    }
                    None => {
                        self.rebind_type(&name.to_ascii_lowercase(), Ty::Error);
                        (Ty::Error, true)
                    }
                },
                Pending::Proc(id, t) => {
                    self.with_type_params(id, |s| s.define_proc_type(id, &t));
                    (ty, true)
                }
                Pending::Alias(i) | Pending::Defines(i) => {
                    let t = self.resolve_alias(i);
                    let lower = self.pending_aliases[i].lower.clone();
                    self.rebind_type(&lower, t);
                    // `TIntBox = TBox<Integer>` names an instantiation (`docs/spec/generics.md` 1.3)
                    (t, is_dyn && (!generic || matches!(t, Ty::DynArray(_))))
                }
                Pending::Nothing => (ty, matches!(ty, Ty::Enum(_) | Ty::DynArray(_))),
            };
            self.pop_hint_scope();
            let routine = self
                .cur
                .as_ref()
                .filter(|c| !c.is_main)
                .and_then(|c| c.routine);
            // A generic type is shown with its type parameters (`docs/spec/generics.md` 8)
            let name = match ty.type_id().filter(|id| self.types.is_generic(*id)) {
                Some(id) => self.generic_decl_text(id, &name),
                None => name,
            };
            // A nested type is shown with its class (`TFoo.TItem`)
            let name = match self.class_scope() {
                Some(_) => {
                    let full = self.qualified_type_name(&name);
                    full.strip_prefix(&format!("{}.", self.program_name))
                        .unwrap_or(&full)
                        .to_string()
                }
                None => name,
            };
            self.type_decls.push(TypeDeclInfo {
                unit: self.unit_index,
                routine,
                name,
                ty,
                defines,
                generic: generic && defines,
                span: range,
            });
        }
        self.pending_aliases.truncate(alias_base);
        let mut reported = std::mem::take(&mut self.cycle_check.reported);
        self.check_cycles(&defined, &mut reported);
        self.cycle_check.reported = reported;
        self.cycle_check.types.extend(defined);
        if let Some(d) = deferred {
            self.define_pending_methods();
            self.end_deferred_checks(d);
            self.generics_declared();
            self.check_instance_cycles(instance_base);
            self.attr_defer -= 1;
            self.flush_attribute_checks();
        }
    }

    /// The attributes of a type declaration (`docs/spec/attributes.md` 1.2): those of a forward
    /// declaration of a class or an interface are E3401, pointing at the full declaration.
    fn type_decl_attributes(&mut self, decl: &ast::TypeDecl, section: &ast::TypeSection) {
        let forward = match decl.type_spec() {
            Some(ast::TypeSpec::Class(c)) => c.is_forward(),
            Some(ast::TypeSpec::Interface(i)) => i.is_forward(),
            _ => false,
        };
        let Some(name_tok) = decl.name().filter(|_| forward) else {
            self.decl_attributes(decl.syntax(), super::attrs::AttrSite::Other, Ty::Error);
            if let Some(n) = decl.name() {
                let name = self.member_display_name(n.name());
                self.declare_hints(decl.syntax(), n.text_range(), name);
            }
            return;
        };
        let full = super::generics::completing_decl(section, decl)
            .and_then(|d| d.name())
            .map(|n| n.text_range());
        let name = name_tok.name().to_string();
        self.attributes_on_forward(decl.syntax(), &name, full.map(|r| self.span(r)), true);
        self.read_hints(decl.syntax());
        self.hints_on_forward(decl.syntax(), &name, full.map(|r| self.span(r)), true);
    }

    /// The name a declaration in the class body being declared is shown with in the warnings
    /// of `docs/spec/attributes.md` 6.2 (`TFoo.Bar`), or `name` outside a class body.
    pub(super) fn member_display_name(&self, name: &str) -> String {
        match self.class_scope() {
            Some(c) => match &self.types.get(c).name {
                Some(owner) => format!("{owner}.{name}"),
                None => name.to_string(),
            },
            None => name.to_string(),
        }
    }

    fn bind_type(&mut self, lower: String, ty: Ty, span: TextRange) {
        if self.class_scope().is_some() {
            // A nested type (`docs/spec/objects.md` 2.2), under its name as written
            let name = self.name_src(span);
            self.bind_nested_type(name, ty, span);
            return;
        }
        match &mut self.cur {
            Some(c) if !c.is_main => {
                c.scopes
                    .last_mut()
                    .expect("a routine has a scope")
                    .push((lower, LSym::Type { ty, span }));
            }
            _ => self.global_scope.push((lower, GSym::Type { ty, span })),
        }
    }

    /// Sets the type an alias of the section denotes once it is resolved.
    fn rebind_type(&mut self, lower: &str, ty: Ty) {
        if self.rebind_nested_type(lower, ty) {
            return;
        }
        if let Some(c) = &mut self.cur
            && let Some(scope) = c.scopes.last_mut()
            && let Some((_, LSym::Type { ty: t, .. })) =
                scope.iter_mut().rev().find(|(n, _)| n == lower)
        {
            *t = ty;
            return;
        }
        if let Some((_, GSym::Type { ty: t, .. })) =
            self.global_scope.iter_mut().rev().find(|(n, _)| n == lower)
        {
            *t = ty;
        }
    }

    /// `class of T` (`docs/spec/objects.md` 2.8): the class reference of the class `T` (E0733
    /// when `T` is not a class).
    fn class_ref_type(&mut self, r: &ast::ClassRefType, written: DynWritten) -> Ty {
        // A name, or a name with type arguments (`docs/spec/generics.md` 1.3)
        let Some(target) = r.syntax().children().find_map(ast::TypeSpec::cast) else {
            return Ty::Error;
        };
        match self.resolve_type(Some(target.clone())) {
            Ty::Class(c) => self.types.class_ref(c, written),
            Ty::Error => Ty::Error,
            other => {
                let t = self.tn(other);
                self.error(
                    Code::E(733),
                    format!("`{t}` is not a class"),
                    target.syntax().text_range(),
                );
                Ty::Error
            }
        }
    }

    /// The class declarations of a section: (lowercase name, the type, name position).
    fn section_classes(
        &self,
        section: &ast::TypeSection,
    ) -> Vec<(String, ast::ClassType, TextRange)> {
        section
            .decls()
            .filter(|d| d.type_params().is_none())
            .filter_map(|d| {
                let n = d.name()?;
                match d.type_spec()? {
                    ast::TypeSpec::Class(c) => {
                        Some((self.text(&n).to_ascii_lowercase(), c, n.text_range()))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Whether a name (unqualified) is declared in the section.
    fn names_section_type(section: &ast::TypeSection, r: &ast::TypeRef) -> bool {
        let Some(q) = r.name() else { return false };
        let lower = q.text().to_ascii_lowercase();
        !lower.contains('.')
            && section.decls().any(|d| {
                d.name()
                    .is_some_and(|n| n.name().eq_ignore_ascii_case(&lower))
            })
    }

    /// A class (`docs/spec/objects.md` chapters 1 and 2): its parent (`TObject` when none is
    /// written or on an error), its fields after the parent's, and its members.
    fn define_object(&mut self, id: TypeId, c: &ast::ClassType, parent: Option<Ty>) {
        let parents = c.parent_specs();
        // `class(T)` (`docs/spec/generics.md` 2.4)
        let param_parent = parents.first().and_then(|p| {
            let n = p.as_ref()?.name()?;
            let text = n.text();
            self.types
                .type_params(id)
                .iter()
                .any(|t| {
                    self.types
                        .get(*t)
                        .name
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(&text))
                })
                .then(|| (text, p.syntax().text_range()))
        });
        if let Some((text, range)) = &param_parent {
            self.error(
                Code::E(757),
                format!("cannot inherit from type parameter `{text}`"),
                *range,
            );
        }
        let parent_ty = match (parent, parents.first()) {
            (_, Some(_)) if param_parent.is_some() => Some(Ty::Error),
            (Some(t), _) => Some(t),
            // The parent may use the class's type parameters (`TSorted<T> = class(TBase<T>)`)
            (None, Some(p)) => Some(self.with_param_scope(id, |s| s.resolve_type(Some(p.clone())))),
            (None, None) => None,
        };
        let root = super::types::TOBJECT;
        // A declaration without a body is the form of an exception class (`EA = class(EB);`), so
        // a parent that cannot be used falls back to `Exception`, as in M2
        // (`docs/spec/objects.md` 8.6)
        let fallback = if c.body().is_none() {
            super::types::EXCEPTION
        } else {
            root
        };
        let parent_id = match (parent_ty, parents.first()) {
            (Some(Ty::Class(pid)), Some(p)) if self.types.is_descendant(pid, id) => {
                let name = self.types.name(Ty::Class(id));
                let span = self.types.get(id).span;
                let range = p.syntax().text_range();
                let d = Diagnostic::error(
                    Code::E(727),
                    format!("type `{name}` inherits from itself"),
                    Stage::Sema,
                )
                .with_primary(self.span(span), "")
                .with_secondary(self.span(range), "part of the cycle");
                self.push(d);
                fallback
            }
            (Some(Ty::Class(pid)), Some(p))
                if self.incomplete_classes.contains(&self.types.layout(pid)) =>
            {
                // Declared later in the section, only forward-declared, or the class enclosing
                // this one (`docs/spec/objects.md` chapter 1)
                let pname = self.types.name(Ty::Class(pid));
                let pspan = self.types.get(self.types.layout(pid)).span;
                let range = p.syntax().text_range();
                let d = Diagnostic::error(
                    Code::E(723),
                    format!("cannot inherit from `{pname}` before its declaration is complete"),
                    Stage::Sema,
                )
                .with_primary(self.span(range), "")
                .with_secondary(self.span(pspan), "declared here");
                self.push(d);
                fallback
            }
            (Some(Ty::Class(pid)), _) => pid,
            // `class(IFoo)`: the parent is `TObject` and the name is an interface the class
            // implements (`docs/spec/objects.md` 7.1)
            (Some(Ty::Error | Ty::Intf(_)) | None, _) => root,
            (Some(other), Some(p)) => {
                let t = self.tn(other);
                let range = p.syntax().text_range();
                self.error(Code::E(733), format!("`{t}` is not a class"), range);
                root
            }
            (Some(_), None) => root,
        };
        // An instantiation whose definition is in the same section has the definition's layout
        let layout = self.types.layout(parent_id);
        let ref_counted = match &self.types.get(layout).kind {
            TypeKind::Class { ref_counted, .. } => *ref_counted,
            _ => false,
        };
        // The fields are filled in below; the kind is set first so that a field of the class's
        // own type sees a class
        // The modifiers (`docs/spec/objects.md` 2.4)
        let mut is_abstract = false;
        let mut sealed = false;
        for m in c.modifiers() {
            let both = match m.kind() {
                crate::syntax::SyntaxKind::ABSTRACT_KW => {
                    is_abstract = true;
                    sealed
                }
                _ => {
                    sealed = true;
                    is_abstract
                }
            };
            if both {
                self.error(
                    Code::E(745),
                    "a class cannot be both `sealed` and `abstract`",
                    m.text_range(),
                );
            }
        }
        if let (Some(p), TypeKind::Class { sealed: true, .. }) =
            (parents.first(), &self.types.get(layout).kind)
        {
            let pname = self.types.name(Ty::Class(parent_id));
            let pspan = self.types.get(parent_id).span;
            let punit = self.types.class_unit(parent_id);
            let mut d = Diagnostic::error(
                Code::E(745),
                format!("cannot inherit from sealed class `{pname}`"),
                Stage::Sema,
            )
            .with_primary(self.span(p.syntax().text_range()), "");
            if let Some(u) = punit {
                d = d.with_secondary(
                    crate::source::Span::new(self.unit_file(u), pspan),
                    "declared here",
                );
            }
            self.push(d);
        }
        self.types.get_mut(id).kind = TypeKind::Class {
            parent: Some(parent_id),
            fields: Vec::new(),
            unit: Some(self.unit_index),
            ref_counted,
            methods: Vec::new(),
            vmt: Vec::new(),
            is_abstract,
            sealed,
            statics: Vec::new(),
            properties: Vec::new(),
            intfs: Vec::new(),
        };
        // The interfaces in parentheses (`docs/spec/objects.md` 7.3)
        let first_intf = usize::from(!matches!(parent_ty, Some(Ty::Intf(_))));
        let listed = self.with_param_scope(id, |s| {
            s.listed_interfaces(&parents[first_intf.min(parents.len())..])
        });
        let mut next = self.types.class_slot_count(layout);
        let inherited: Vec<(String, crate::source::Span, Visibility, Option<u32>)> = self
            .types
            .class_fields(layout)
            .iter()
            .map(|(owner, f)| {
                let unit = match &self.types.get(*owner).kind {
                    TypeKind::Class { unit, .. } => *unit,
                    _ => None,
                };
                let file = self.unit_file(unit.unwrap_or(self.unit_index));
                (
                    f.name.to_ascii_lowercase(),
                    crate::source::Span::new(file, f.span),
                    f.vis,
                    unit,
                )
            })
            .collect();
        let mut fields: Vec<ClassField> = Vec::new();
        let mut methods = Vec::new();
        let mut properties = Vec::new();
        let mut resolutions = Vec::new();
        let mut vis = Visibility::Public;
        let members: Vec<ast::ClassMember> =
            c.body().map(|b| b.members().collect()).unwrap_or_default();
        // The members after the class's own constants and nested types see them (2.2)
        if let Some(outer) = self.class_scope() {
            self.lexical_outer.insert(id, outer);
        }
        self.class_scopes.push(super::statics::ClassScope {
            class: id,
            vis: Visibility::Public,
            names: Vec::new(),
        });
        self.enter_type_params(id);
        for m in members {
            let decls: Vec<ast::FieldDecl> = match m {
                ast::ClassMember::Visibility(v) => {
                    vis = match v.keyword().map(|k| k.kind()) {
                        Some(crate::syntax::SyntaxKind::PRIVATE_KW) if v.is_strict() => {
                            Visibility::StrictPrivate
                        }
                        Some(crate::syntax::SyntaxKind::PRIVATE_KW) => Visibility::Private,
                        Some(crate::syntax::SyntaxKind::PROTECTED_KW) if v.is_strict() => {
                            Visibility::StrictProtected
                        }
                        Some(crate::syntax::SyntaxKind::PROTECTED_KW) => Visibility::Protected,
                        Some(crate::syntax::SyntaxKind::PUBLISHED_KW) => Visibility::Published,
                        _ => Visibility::Public,
                    };
                    if let Some(scope) = self.class_scopes.last_mut() {
                        scope.vis = vis;
                    }
                    continue;
                }
                ast::ClassMember::Method(m) => {
                    methods.push((m, vis));
                    continue;
                }
                ast::ClassMember::Property(p) => {
                    properties.push((p, vis));
                    continue;
                }
                ast::ClassMember::Resolution(r) => {
                    resolutions.push(r);
                    continue;
                }
                ast::ClassMember::Var(v) if v.is_class() => {
                    self.declare_class_vars(&v);
                    continue;
                }
                ast::ClassMember::Const(cs) => {
                    self.declare_class_consts(&cs);
                    continue;
                }
                ast::ClassMember::Type(ts) => {
                    self.declare_types(&ts);
                    continue;
                }
                ast::ClassMember::Field(f) => vec![f],
                ast::ClassMember::Var(v) => v.fields().collect(),
            };
            for fd in decls {
                let scope = self.declare_member_hints(fd.syntax(), &fd.names());
                self.begin_names_hints(scope);
                let ty = self.resolve_type(fd.type_spec());
                self.end_names_hints(scope);
                let ty = self.decl_attributes(fd.syntax(), super::attrs::AttrSite::Field, ty);
                for n in fd.names() {
                    let name = self.text(&n).to_string();
                    let lower = name.to_ascii_lowercase();
                    let span = n.text_range();
                    let prev = fields
                        .iter()
                        .find(|f| f.name.eq_ignore_ascii_case(&name))
                        .map(|f| self.span(f.span))
                        .or_else(|| {
                            // An ancestor's field the class's declaration can see (2.2)
                            inherited
                                .iter()
                                .rev()
                                .find(|(n, _, v, u)| {
                                    *n == lower
                                        && match v {
                                            Visibility::StrictPrivate => false,
                                            Visibility::Private => {
                                                u.is_some_and(|u| self.is_own_unit(u))
                                            }
                                            _ => true,
                                        }
                                })
                                .map(|(_, s, _, _)| *s)
                        });
                    if let Some(prev) = prev {
                        let d = Diagnostic::error(
                            Code::E(701),
                            format!("`{name}` is already declared"),
                            Stage::Sema,
                        )
                        .with_primary(self.span(span), "declared again here")
                        .with_secondary(prev, "declared here");
                        self.push(d);
                        continue;
                    }
                    if self.class_duplicate(&lower, span) {
                        continue;
                    }
                    fields.push(ClassField {
                        name,
                        ty,
                        span,
                        vis,
                        slot: next,
                    });
                    next += 1;
                }
            }
        }
        self.class_scopes.pop();
        if let TypeKind::Class { fields: f, .. } = &mut self.types.get_mut(id).kind {
            *f = fields;
        }
        self.incomplete_classes.remove(&id);
        self.pending_classes.push((id, methods, properties));
        self.pending_impls.push(super::interfaces::PendingImpl {
            class: id,
            listed,
            resolutions,
        });
    }

    /// The values of an enumeration (`docs/spec/types.md` 8.1): ordinals from 0, or as written
    /// (`= Expr`, an integer constant in the range of `Integer`), each value one more than the
    /// previous one otherwise. Overlapping ordinals are allowed.
    fn define_enum(&mut self, id: TypeId, e: &ast::EnumType) {
        let mut values: Vec<EnumValueDef> = Vec::new();
        let mut next: i128 = 0;
        for v in e.values() {
            let Some(name_tok) = v.name() else { continue };
            let name = self.text(&name_tok).to_string();
            let span = name_tok.text_range();
            let mut ord = next;
            if let Some(x) = v.value() {
                let x = self.lower_expr(&x);
                if !x.ty.is_error() {
                    if !x.ty.is_integer() {
                        let t = self.tn(x.ty);
                        self.error(
                            Code::E(702),
                            format!("type mismatch: expected an integer type, found `{t}`"),
                            x.span,
                        );
                    } else if let Some(Const::Int(n)) = x.value {
                        if Ty::Integer.contains(n) {
                            ord = n;
                        } else {
                            self.const_out_of_range(n, Ty::Integer, x.span);
                        }
                    } else {
                        self.error(Code::E(713), "constant expression required", x.span);
                    }
                }
            }
            if let Some(prev) = values
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
                .map(|p| p.span)
            {
                let d = Diagnostic::error(
                    Code::E(701),
                    format!("`{name}` is already declared"),
                    Stage::Sema,
                )
                .with_primary(self.span(span), "declared again here")
                .with_secondary(self.span(prev), "declared here");
                self.push(d);
            } else {
                values.push(EnumValueDef { name, ord, span });
            }
            next = ord + 1;
        }
        let low = values.iter().map(|v| v.ord).min().unwrap_or(0);
        let high = values.iter().map(|v| v.ord).max().unwrap_or(0);
        self.types.get_mut(id).kind = TypeKind::Enum { values, low, high };
    }

    /// The base type and bounds of `L..H` (`docs/spec/types.md` 8.2): constants of the same
    /// ordinal type; an integer range takes the narrowest of `Integer`, `Cardinal`, `Int64` and
    /// `UInt64` that holds it. `None` after a diagnostic.
    fn subrange_kind(&mut self, r: &ast::SubrangeType) -> Option<TypeKind> {
        let range = ast::trimmed_range(r.syntax());
        let (lo_e, hi_e) = r.bounds();
        let lo = lo_e.map(|e| self.lower_expr(&e));
        let hi = hi_e.map(|e| self.lower_expr(&e));
        let (lo, hi) = (lo?, hi?);
        // A bound of a distinct type is a value of its root (`docs/spec/types.md` 1.3)
        let (lo, hi) = (self.rooted(lo), self.rooted(hi));
        if lo.ty.is_error() || hi.ty.is_error() {
            return None;
        }
        for b in [&lo, &hi] {
            if b.value.is_none() {
                self.error(Code::E(713), "constant expression required", b.span);
                return None;
            }
        }
        let l = lo.value.as_ref().and_then(Const::as_int)?;
        let h = hi.value.as_ref().and_then(Const::as_int)?;
        let base = match (lo.ty, hi.ty) {
            (Ty::Char, Ty::Char) => Ty::Char,
            (Ty::Boolean, Ty::Boolean) => Ty::Boolean,
            (Ty::Enum(a), Ty::Enum(b)) if a == b => Ty::Enum(a),
            (a, b) if a.is_integer() && b.is_integer() => {
                if Ty::Integer.contains(l) && Ty::Integer.contains(h) {
                    Ty::Integer
                } else if l >= 0 && Ty::Cardinal.contains(h) {
                    Ty::Cardinal
                } else if Ty::Int64.contains(l) && Ty::Int64.contains(h) {
                    Ty::Int64
                } else {
                    Ty::UInt64
                }
            }
            (a, b) => {
                let (a, b) = (self.tn(a), self.tn(b));
                let msg = if a == b {
                    format!("type mismatch: expected an ordinal type, found `{a}`")
                } else {
                    format!("type mismatch: the bounds have types `{a}` and `{b}`")
                };
                self.error(Code::E(702), msg, range);
                return None;
            }
        };
        if l > h {
            let text = format!("{}..{}", bound_text(base, l), bound_text(base, h));
            self.error(
                Code::E(728),
                format!("invalid subrange bounds `{text}`"),
                range,
            );
            return None;
        }
        Some(TypeKind::Subrange {
            base,
            low: l,
            high: h,
        })
    }

    /// A record (`docs/spec/types.md` 2.1) and its members (`docs/spec/objects.md` chapter 6):
    /// the fields in declaration order, the `class var`s, constants and nested types, and the
    /// methods and properties (defined with the classes of the section). What a record cannot
    /// have is E0744.
    fn define_record(&mut self, id: TypeId, r: &ast::RecordType) {
        self.types.get_mut(id).kind = TypeKind::Record {
            fields: Vec::new(),
            unit: Some(self.unit_index),
            methods: Vec::new(),
            statics: Vec::new(),
            properties: Vec::new(),
        };
        if let Some(p) = r.parents() {
            self.error(
                Code::E(744),
                "records cannot inherit from another type",
                ast::trimmed_range(&p),
            );
        }
        if let Some(m) = r.modifiers().next() {
            self.error(
                Code::E(744),
                "a record cannot be `abstract` or `sealed`",
                m.text_range(),
            );
        }
        let anonymous = self.types.get(id).name.is_none();
        let mut fields: Vec<Field> = Vec::new();
        let mut methods = Vec::new();
        let mut properties = Vec::new();
        let mut vis = Visibility::Public;
        if let Some(outer) = self.class_scope() {
            self.lexical_outer.insert(id, outer);
        }
        self.class_scopes.push(super::statics::ClassScope {
            class: id,
            vis: Visibility::Public,
            names: Vec::new(),
        });
        self.enter_type_params(id);
        for m in r.members() {
            let decls: Vec<ast::FieldDecl> = match m {
                ast::ClassMember::Visibility(v) => {
                    let kw = v.keyword();
                    vis = match kw.as_ref().map(|k| k.kind()) {
                        Some(crate::syntax::SyntaxKind::PRIVATE_KW) if v.is_strict() => {
                            Visibility::StrictPrivate
                        }
                        Some(crate::syntax::SyntaxKind::PRIVATE_KW) => Visibility::Private,
                        Some(
                            crate::syntax::SyntaxKind::PROTECTED_KW
                            | crate::syntax::SyntaxKind::PUBLISHED_KW,
                        ) => {
                            // No descendants: `protected` means nothing (chapter 6)
                            let range = ast::trimmed_range(v.syntax());
                            let src = self.src(range).to_ascii_lowercase();
                            let words: Vec<&str> = src.split_whitespace().collect();
                            self.error(
                                Code::E(744),
                                format!("`{}` is not allowed in a record", words.join(" ")),
                                range,
                            );
                            Visibility::Public
                        }
                        _ => Visibility::Public,
                    };
                    if let Some(scope) = self.class_scopes.last_mut() {
                        scope.vis = vis;
                    }
                    continue;
                }
                ast::ClassMember::Method(m) => {
                    methods.push((m, vis));
                    continue;
                }
                ast::ClassMember::Property(p) => {
                    properties.push((p, vis));
                    continue;
                }
                ast::ClassMember::Resolution(r) => {
                    // A record implements no interface (`docs/spec/objects.md` 7.1)
                    self.resolution_not_implemented(&r, id);
                    continue;
                }
                ast::ClassMember::Var(v) if v.is_class() => {
                    self.declare_class_vars(&v);
                    continue;
                }
                ast::ClassMember::Const(cs) => {
                    self.declare_class_consts(&cs);
                    continue;
                }
                ast::ClassMember::Type(ts) => {
                    self.declare_types(&ts);
                    continue;
                }
                ast::ClassMember::Field(f) => vec![f],
                ast::ClassMember::Var(v) => v.fields().collect(),
            };
            for fd in decls {
                let scope = self.declare_member_hints(fd.syntax(), &fd.names());
                self.begin_names_hints(scope);
                let ty = self.resolve_type(fd.type_spec());
                self.end_names_hints(scope);
                let ty = self.decl_attributes(fd.syntax(), super::attrs::AttrSite::Field, ty);
                for n in fd.names() {
                    let name = self.text(&n).to_string();
                    let span = n.text_range();
                    if self.class_duplicate(&name.to_ascii_lowercase(), span) {
                        continue;
                    }
                    fields.push(Field {
                        name,
                        ty,
                        span,
                        vis,
                    });
                }
            }
        }
        self.class_scopes.pop();
        if let TypeKind::Record { fields: f, .. } = &mut self.types.get_mut(id).kind {
            *f = fields;
        }
        if anonymous {
            // A method of a record without a name could not be given a body
            if let Some((m, _)) = methods.first() {
                let r = m
                    .header()
                    .and_then(|h| h.name())
                    .map(|n| n.syntax().text_range())
                    .unwrap_or_else(|| ast::trimmed_range(m.syntax()));
                self.error(
                    Code::E(744),
                    "a record type with methods must be declared in a `type` section",
                    r,
                );
            } else if let Some((p, _)) = properties.first() {
                let r = ast::trimmed_range(p.syntax());
                self.error(
                    Code::E(744),
                    "a record type with methods must be declared in a `type` section",
                    r,
                );
            }
            return;
        }
        if !methods.is_empty() || !properties.is_empty() {
            self.pending_classes.push((id, methods, properties));
        }
    }

    fn define_array(&mut self, id: TypeId, a: &ast::ArrayType) {
        let ranges = a.ranges();
        let kind = self.array_kind(&ranges, a.element());
        self.types.get_mut(id).kind = kind;
    }

    /// `array[R1, R2, …] of T` is `array[R1] of array[R2, …] of T` (`docs/spec/types.md` 2.2).
    fn array_kind(&mut self, ranges: &[ast::IndexRange], elem: Option<ast::TypeSpec>) -> TypeKind {
        let Some((first, rest)) = ranges.split_first() else {
            return TypeKind::Pending;
        };
        let bounds = self.index_range(first);
        let elem = if rest.is_empty() {
            self.resolve_type(elem)
        } else {
            let span = rest[0].syntax().text_range();
            let inner = self.add_type(None, span);
            let kind = self.array_kind(rest, elem);
            self.types.get_mut(inner).kind = kind;
            Ty::Array(inner)
        };
        match bounds {
            Some((index, low, high)) => TypeKind::Array {
                index,
                low,
                high,
                elem,
            },
            None => TypeKind::Pending,
        }
    }

    /// The index type and bounds of `L..H` or of an ordinal type name.
    fn index_range(&mut self, r: &ast::IndexRange) -> Option<(Ty, i128, i128)> {
        let range = ast::trimmed_range(r.syntax());
        let (lo_e, hi_e) = r.bounds();
        if !r.is_range() {
            let e = lo_e?;
            let Some(ty) = self.type_name_arg(&e) else {
                let x = self.lower_expr(&e);
                if !x.ty.is_error() {
                    let text = self.src(x.span);
                    self.error(Code::E(708), format!("`{text}` is not a type"), x.span);
                }
                return None;
            };
            if ty.is_error() {
                return None;
            }
            let Some((lo, hi)) = self
                .types
                .ord_range(ty)
                .filter(|_| self.types.root(ty).is_ordinal())
            else {
                let t = self.tn(ty);
                self.error(
                    Code::E(702),
                    format!("type mismatch: expected an ordinal type, found `{t}`"),
                    range,
                );
                return None;
            };
            if hi - lo + 1 > MAX_ARRAY_LEN {
                self.error(Code::E(728), "array type is too large", range);
                return None;
            }
            // A distinct type indexes as its root (`docs/spec/types.md` 1.3)
            return Some((self.types.base(self.types.root(ty)), lo, hi));
        }
        let lo = lo_e.map(|e| self.lower_expr(&e))?;
        let hi = hi_e.map(|e| self.lower_expr(&e))?;
        // A bound of a distinct type is a value of its root (`docs/spec/types.md` 1.3)
        let (lo, hi) = (self.rooted(lo), self.rooted(hi));
        if lo.ty.is_error() || hi.ty.is_error() {
            return None;
        }
        for b in [&lo, &hi] {
            if b.value.is_none() {
                self.error(Code::E(713), "constant expression required", b.span);
                return None;
            }
        }
        let index = match (lo.ty, hi.ty) {
            (Ty::Char, Ty::Char) => Ty::Char,
            (Ty::Boolean, Ty::Boolean) => Ty::Boolean,
            (Ty::Enum(a), Ty::Enum(b)) if a == b => Ty::Enum(a),
            (a, b) if a.is_integer() && b.is_integer() => Ty::Integer,
            (a, b) => {
                let (a, b) = (self.tn(a), self.tn(b));
                let msg = if a == b {
                    format!("type mismatch: expected an ordinal type, found `{a}`")
                } else {
                    format!("type mismatch: the bounds have types `{a}` and `{b}`")
                };
                self.error(Code::E(702), msg, range);
                return None;
            }
        };
        let l = lo.value.as_ref().and_then(Const::as_int)?;
        let h = hi.value.as_ref().and_then(Const::as_int)?;
        if l > h {
            let text = format!("{}..{}", bound_text(index, l), bound_text(index, h));
            self.error(
                Code::E(728),
                format!("invalid array bounds `{text}`"),
                range,
            );
            return None;
        }
        if h - l + 1 > MAX_ARRAY_LEN {
            self.error(Code::E(728), "array type is too large", range);
            return None;
        }
        Some((index, l, h))
    }

    /// E0727 again at the end of the outermost `type` section, once the instantiations asked for
    /// in it have their members (`docs/spec/generics.md` 4.2): a cycle through the fields of an
    /// instantiation (`TR = record A: TBox<TR>; end`; `docs/spec/types.md` 1.1). The types of the
    /// section are checked first, then the instantiations asked for since `instance_base`; an
    /// instantiation's cycle that goes through a type already reported is cut without reporting
    /// it again (`TSelf<Integer>` of a `TSelf<T>` that contains itself).
    fn check_instance_cycles(&mut self, instance_base: usize) {
        let check = std::mem::take(&mut self.cycle_check);
        let mut reported = check.reported;
        self.check_cycles(&check.types, &mut reported);
        let instances: Vec<TypeId> = self.types.instances[instance_base..]
            .iter()
            .map(|i| i.id)
            .collect();
        self.check_cycles(&instances, &mut reported);
    }

    /// E0727: a record field or static array element that leads back to its own type without
    /// going through a dynamic array (`docs/spec/types.md` 1.1). The first step of the cycle is
    /// cut (its type becomes the error type) so that nothing else walks around it. `reported`
    /// collects the types reported; a cycle through one of them (or through an instantiation of
    /// one) is cut without reporting it again.
    fn check_cycles(&mut self, ids: &[TypeId], reported: &mut Vec<TypeId>) {
        for &id in ids {
            let mut path: Vec<(TypeId, usize, TextRange)> = Vec::new();
            let mut visited: Vec<TypeId> = Vec::new();
            if !self.reaches(id, id, &mut path, &mut visited) {
                continue;
            }
            // Only an instantiation whose cycle goes through a reported type is not reported again
            let seen = self.types.instance_of(id).is_some()
                && path.iter().any(|(owner, _, _)| {
                    let def = self.types.instance_of(*owner).map(|i| i.def);
                    reported.contains(owner) || def.is_some_and(|d| reported.contains(&d))
                });
            if seen {
                self.cut_cycle(path[0]);
                continue;
            }
            reported.push(id);
            let def = self.types.get(id);
            let name = self.tn(match def.kind {
                TypeKind::Array { .. } => Ty::Array(id),
                _ => Ty::Record(id),
            });
            let mut d = Diagnostic::error(
                Code::E(727),
                format!("type `{name}` contains itself"),
                Stage::Sema,
            )
            .with_primary(Span::new(self.decl_file(id), def.span), "");
            for (owner, _, span) in &path {
                let file = self.decl_file(*owner);
                d = d.with_secondary(Span::new(file, *span), "part of the cycle");
            }
            self.push(d);
            self.cut_cycle(path[0]);
        }
    }

    /// The file declaring a record or a static array type (an instantiation is declared where its
    /// generic type is; `docs/spec/generics.md` 4.1): the steps of a cycle (E0727) may be in a
    /// unit.
    fn decl_file(&self, id: TypeId) -> FileId {
        let id = self.types.instance_of(id).map_or(id, |i| i.def);
        self.types
            .class_unit(id)
            .or_else(|| self.type_unit(id))
            .map_or(self.input.file, |u| self.unit_file(u))
    }

    /// Cuts a cycle at its first step (E0727): the field's or the element's type becomes the
    /// error type.
    fn cut_cycle(&mut self, (owner, slot, _): (TypeId, usize, TextRange)) {
        match &mut self.types.get_mut(owner).kind {
            TypeKind::Record { fields, .. } => fields[slot].ty = Ty::Error,
            TypeKind::Array { elem, .. } => *elem = Ty::Error,
            TypeKind::DynArray { .. }
            | TypeKind::Class { .. }
            | TypeKind::Interface { .. }
            | TypeKind::ClassRef { .. }
            | TypeKind::Enum { .. }
            | TypeKind::Subrange { .. }
            | TypeKind::Set { .. }
            | TypeKind::Proc { .. }
            | TypeKind::Helper { .. }
            | TypeKind::Param { .. }
            | TypeKind::Distinct { .. }
            | TypeKind::Pending => {}
        }
    }

    /// Whether `from` contains `target` through record fields and static array elements; `path`
    /// collects the steps (owner, slot, position).
    fn reaches(
        &self,
        from: TypeId,
        target: TypeId,
        path: &mut Vec<(TypeId, usize, TextRange)>,
        visited: &mut Vec<TypeId>,
    ) -> bool {
        if visited.contains(&from) {
            return false;
        }
        visited.push(from);
        let def = self.types.get(from);
        let steps: Vec<(usize, Ty, TextRange)> = match &def.kind {
            TypeKind::Record { fields, .. } => fields
                .iter()
                .enumerate()
                .map(|(i, f)| (i, f.ty, f.span))
                .collect(),
            TypeKind::Array { elem, .. } => vec![(0, *elem, def.span)],
            TypeKind::DynArray { .. }
            | TypeKind::Class { .. }
            | TypeKind::Interface { .. }
            | TypeKind::ClassRef { .. }
            | TypeKind::Enum { .. }
            | TypeKind::Subrange { .. }
            | TypeKind::Set { .. }
            | TypeKind::Proc { .. }
            | TypeKind::Helper { .. }
            | TypeKind::Param { .. }
            | TypeKind::Distinct { .. }
            | TypeKind::Pending => Vec::new(),
        };
        for (slot, ty, span) in steps {
            // A dynamic array holds its elements by reference, so it ends the walk
            let (Ty::Record(next) | Ty::Array(next)) = ty else {
                continue;
            };
            path.push((from, slot, span));
            if next == target || self.reaches(next, target, path, visited) {
                return true;
            }
            path.pop();
        }
        false
    }

    /// The type an argument names (`Low(Integer)`, `Default(TPoint)`, `array[Char]`), without
    /// reporting anything. `None` if it is not a type name.
    pub(super) fn type_name_arg(&mut self, e: &ast::Expr) -> Option<Ty> {
        match e {
            ast::Expr::NameRef(n) => {
                if n.is_string_keyword() {
                    return Some(Ty::String);
                }
                if let Some(args) = n.type_args() {
                    // A generic type with its type arguments (`Default(TBox<Integer>)`;
                    // `docs/spec/generics.md` 1.4): reported where it is written
                    let tok = n.token()?;
                    let lower = tok.name().to_ascii_lowercase();
                    let generic = !self
                        .generic_types_named(&lower, &super::generics::GenericScope::Visible)
                        .is_empty();
                    if !generic && lower != "tarray" {
                        return None;
                    }
                    return match self
                        .type_args_in_expr(&tok, &args, super::generics::GenericScope::Visible)
                        .0
                    {
                        super::Resolved::Type(t) => Some(t),
                        _ => Some(Ty::Error),
                    };
                }
                let lower = n.text().to_ascii_lowercase();
                match self.lookup_type_symbol(&lower) {
                    Some(t) => t,
                    None => self
                        .unit_type(&lower)
                        .or_else(|| Ty::from_name(&lower))
                        .or_else(|| self.system_pas_type(&lower))
                        .or_else(|| match builtins::lookup_system(&lower) {
                            // The built-in types of `System` (`TObject`, `TIndexRange`, `TProc`, …)
                            Some(builtins::SystemName::Type(t)) if !matches!(t, Ty::Helper(_)) => {
                                Some(t)
                            }
                            _ => None,
                        })
                        .or_else(|| self.sysutils_type_arg(n)),
                }
            }
            ast::Expr::Field(f) if f.type_args().is_some() => {
                // A qualified generic type (`Default(THolder.TBox<Integer>)`,
                // `Default(Unit.TBox<Integer>)`; `docs/spec/generics.md` 1.4, 2.5)
                match self.resolve_name_expr(e)?.0 {
                    super::Resolved::Type(t) => Some(t),
                    super::Resolved::Error => Some(Ty::Error),
                    _ => None,
                }
            }
            ast::Expr::Field(f)
                if matches!(
                    f.base(),
                    Some(ast::Expr::NameRef(b)) if b.text().eq_ignore_ascii_case("system")
                ) && self.lookup_type_symbol("system").is_none() =>
            {
                let lower = f.field()?.name().to_ascii_lowercase();
                Ty::from_name(&lower).or_else(|| self.system_pas_type(&lower))
            }
            ast::Expr::Field(f) => {
                // A type of a unit (`Default(Shapes.TRec)`; `docs/spec/units.md` 3.2), or a type
                // nested in a class or a record (`Default(TC.TR)`), also in an instantiation
                // (`High(TGen<Integer>.TE)`) or in a type of a unit (`Default(Shapes.TC.TR)`;
                // `docs/spec/objects.md` 2.2, `docs/spec/generics.md` 2.5). Anything else is left
                // to the caller as a value (`Low(R.Bounds)`).
                let base = f.base()?;
                // A unit of the package, or a built-in unit (`High(System.SysUtils.TIOErrorKind)`)
                let in_unit = super::name_chain(e).is_some_and(|chain| {
                    self.unit_prefix(&chain)
                        .is_some_and(|(_, n)| n + 1 == chain.len())
                        || chain.split_last().is_some_and(|(_, quals)| {
                            let q: Vec<String> =
                                quals.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
                            self.builtin_unit_named(&q.join(".")).is_some()
                        })
                });
                if !in_unit {
                    self.type_owner_in_expr(&base)?;
                }
                // Probe first without reporting: a value (`Low(R.Items)`) is left to the caller.
                // Then resolve again for real, so that the uses of the type are recorded.
                let probe = self.dry_run(|s| s.resolve_name_expr(e).map(|(r, ..)| r));
                if !matches!(probe, Some(super::Resolved::Type(_))) {
                    return None;
                }
                match self.resolve_name_expr(e)?.0 {
                    super::Resolved::Type(t) => Some(t),
                    _ => Some(Ty::Error),
                }
            }
            ast::Expr::Paren(p) => self.type_name_arg(&p.inner()?),
            _ => None,
        }
    }

    /// A built-in type of `System.SysUtils` named by an argument (`Low(TReplaceFlag)`,
    /// `High(TIOErrorKind)`; `docs/spec/semantics.md` 5.3), resolved as in an expression so that
    /// it is a type only where `System.SysUtils` is in `uses`. `None` for any other name, and
    /// where it does not resolve to a type (left to the caller as a value, which reports it).
    fn sysutils_type_arg(&mut self, n: &ast::NameRef) -> Option<Ty> {
        let lower = n.text().to_ascii_lowercase();
        if !matches!(
            builtins::lookup_sysutils(&lower),
            Some(k) if !matches!(k, builtins::SysUtilsName::Func(_) | builtins::SysUtilsName::Proc(_))
        ) {
            return None;
        }
        let range = n.syntax().text_range();
        let name = n.text();
        // Probe first without reporting, then resolve again for real so that the use is recorded
        let probe = self.dry_run(|s| s.resolve_ident(&name, range));
        if !matches!(probe, super::Resolved::Type(_)) {
            return None;
        }
        match self.resolve_ident(&name, range) {
            super::Resolved::Type(t) => Some(t),
            _ => Some(Ty::Error),
        }
    }

    /// A type written as a name (`TypeRef`).
    pub(super) fn resolve_type_ref(&mut self, t: &ast::TypeRef) -> Ty {
        match self.resolve_type_name(t) {
            Ty::Helper(_) => {
                // Only the name of a helper has this type (`docs/spec/objects.md` 10.2)
                let range = t.syntax().text_range();
                self.error(Code::E(753), "a helper cannot be used as a type", range);
                Ty::Error
            }
            ty => ty,
        }
    }

    /// The class or record the first name of a nested type's outer type names (`TOuter` in
    /// `TOuter.TInner`, `Unit.TOuter.TInner`; `docs/spec/objects.md` 2.2): a type of `unit`, or
    /// else of this file, or else of a unit in `uses` not qualified with the unit
    /// (`docs/spec/units.md` 3.1). `Ok(None)` when it is no such type; `Err` when looking it up
    /// reported a diagnostic (E2502 for a name in two units).
    pub(super) fn outer_type_base(
        &mut self,
        unit: Option<super::UnitRef>,
        first: &str,
        r: TextRange,
    ) -> Result<Option<Ty>, ()> {
        if let Some(u) = unit {
            return Ok(match self.dry_run(|s| s.resolve_in_unit(u, first, r)) {
                super::Resolved::Type(t @ (Ty::Class(_) | Ty::Record(_))) => Some(t),
                _ => None,
            });
        }
        let lower = first.to_ascii_lowercase();
        match self.lookup_type_symbol(&lower) {
            Some(Some(t @ (Ty::Class(_) | Ty::Record(_)))) => {
                self.type_symbol_use(&lower, r);
                Ok(Some(t))
            }
            Some(_) => Ok(None),
            None => match self.lookup_in_units(&lower, first, r) {
                super::UnitLookup::Found(super::Resolved::Type(
                    t @ (Ty::Class(_) | Ty::Record(_)),
                )) => Ok(Some(t)),
                super::UnitLookup::Error => Err(()),
                _ => Ok(None),
            },
        }
    }

    /// The class or record a dotted name before a nested generic type names (`THolder`,
    /// `TOuter.TMid`, `Unit.THolder`, `THolder` of a unit in `uses`; `docs/spec/generics.md`
    /// 2.5), looked up as the outer type of a nested type in a type position. `Ok(None)` when it
    /// names no class or record; `Err` when looking it up reported a diagnostic.
    pub(super) fn chain_owner(
        &mut self,
        chain: &[(String, TextRange)],
    ) -> Result<Option<TypeId>, ()> {
        let unit = self.unit_prefix(chain);
        let start = unit.map_or(0, |(_, n)| n);
        let Some((first, r)) = chain.get(start).cloned() else {
            return Ok(None);
        };
        let Some(base) = self.outer_type_base(unit.map(|(u, _)| u), &first, r)? else {
            return Ok(None);
        };
        let rest = &chain[start + 1..];
        let Some((_, last)) = rest.last() else {
            return Ok(super::statics::nested_owner(base));
        };
        let range = TextRange::new(chain[0].1.start(), last.end());
        Ok(super::statics::nested_owner(
            self.dry_run(|s| s.nested_type(base, rest, range)),
        ))
    }

    /// A type name, which may name a helper (`resolve_type_ref` reports that as E0753).
    pub(super) fn resolve_type_name(&mut self, t: &ast::TypeRef) -> Ty {
        if t.string_token().is_some() {
            return Ty::String;
        }
        let Some(q) = t.name() else { return Ty::Error };
        let text = q.text();
        let lower = text.to_ascii_lowercase();
        let range = q.syntax().text_range();
        // A declared type comes first (it may hide a type of `System`; `docs/spec/types.md` 1.1)
        if !lower.contains('.') {
            match self.lookup_type_symbol(&lower) {
                Some(Some(ty)) => {
                    self.type_symbol_use(&lower, range);
                    return ty;
                }
                Some(None) => {
                    self.error(Code::E(708), format!("`{text}` is not a type"), range);
                    return Ty::Error;
                }
                None => {}
            }
            // A type of a unit in `uses` (`docs/spec/units.md` 3.1)
            match self.lookup_in_units(&lower, &text, range) {
                super::UnitLookup::Found(super::Resolved::Type(ty)) => return ty,
                super::UnitLookup::Found(_) => {
                    self.error(Code::E(708), format!("`{text}` is not a type"), range);
                    return Ty::Error;
                }
                super::UnitLookup::Error => return Ty::Error,
                super::UnitLookup::NotFound => {}
            }
        } else {
            // A name qualified with a package unit (`docs/spec/units.md` 3.2)
            let chain: Vec<(String, TextRange)> = q.idents().map(|t| t.name_and_range()).collect();
            // A nested type (`TOuter.TInner`, `Unit.TOuter.TInner`; `docs/spec/objects.md` 2.2)
            let unit = self.unit_prefix(&chain);
            let start = unit.map_or(0, |(_, n)| n);
            if start + 1 < chain.len() {
                let (first, r) = chain[start].clone();
                let Ok(base) = self.outer_type_base(unit.map(|(u, _)| u), &first, r) else {
                    return Ty::Error;
                };
                if let Some(base) = base {
                    return self.nested_type(base, &chain[start + 1..], range);
                }
            }
            if let Some((u, n)) = unit {
                if n + 1 == chain.len() {
                    let (name, r) = chain[n].clone();
                    return match self.resolve_in_unit(u, &name, r) {
                        super::Resolved::Type(ty) => ty,
                        super::Resolved::Error => Ty::Error,
                        _ => {
                            self.error(Code::E(708), format!("`{text}` is not a type"), range);
                            Ty::Error
                        }
                    };
                }
                if n == chain.len() {
                    self.error(Code::E(708), format!("`{text}` is not a type"), range);
                    return Ty::Error;
                }
            }
        }
        // A type of the part of `System.SysUtils` written in Pascal (`System.SysUtils.EListError`;
        // `docs/spec/collections.md` 1.1)
        if let Some((u, last)) = lower.rsplit_once('.')
            && self.builtin_unit_named(u) == Some(super::UnitRef::SysUtils)
            && self.sysutils_part_declares(last)
        {
            let name = &text[text.len() - last.len()..];
            return match self.resolve_in_unit(super::UnitRef::SysUtils, name, range) {
                super::Resolved::Type(ty) => ty,
                super::Resolved::Error => Ty::Error,
                _ => {
                    self.error(Code::E(708), format!("`{text}` is not a type"), range);
                    Ty::Error
                }
            };
        }
        // A qualified name such as `System.Integer`
        let unqualified = lower.strip_prefix("system.").unwrap_or(&lower).to_string();
        if let Some(ty) = Ty::from_name(&unqualified) {
            return ty;
        }
        if self.input.stdlib && unqualified == "tvalueref" {
            // The managed part of `TValue`, which only the standard library names
            // (`docs/spec/rtti.md` 3.1)
            return Ty::ValueRef;
        }
        if let Some(ty) =
            builtins::builtin_exception(&unqualified).or_else(|| self.sysutils_exception(&lower))
        {
            return ty;
        }
        if let Some(builtins::SystemName::Type(ty)) = builtins::lookup_system(&unqualified)
            && matches!(
                ty,
                Ty::Class(_)
                    | Ty::ClassRef(_)
                    | Ty::Intf(_)
                    | Ty::Callable(_)
                    | Ty::Helper(_)
                    | Ty::Record(_)
                    | Ty::Enum(_)
            )
        {
            return ty;
        }
        // A type of the part of `System` written in Pascal (`TInstant`, `System.TInstant`)
        if (lower == unqualified || self.builtin_unit_named("system").is_some())
            && let Some(ty) = self.system_pas_type(&unqualified)
        {
            return ty;
        }
        // A type of `System.SysUtils` (`TReplaceFlags`, `TCaseMapping`, …; `docs/spec/semantics.md`
        // 5.3), resolved as in an expression: without `uses System.SysUtils`, E0700 with its fix
        let (unit, last) = match lower.rsplit_once('.') {
            Some((u, n)) => (Some(u), n),
            None => (None, lower.as_str()),
        };
        if matches!(
            builtins::lookup_sysutils(last),
            Some(n) if !matches!(n, builtins::SysUtilsName::Func(_) | builtins::SysUtilsName::Proc(_))
        ) {
            let name = &text[text.len() - last.len()..];
            let resolved = match unit {
                None => Some(self.resolve_ident(name, range)),
                Some(u) if u != "system" && self.is_unit_name(u) => {
                    Some(self.resolve_in_unit(super::UnitRef::SysUtils, name, range))
                }
                Some(_) => None,
            };
            match resolved {
                Some(super::Resolved::Type(ty)) => return ty,
                Some(_) => return Ty::Error,
                None => {}
            }
        }
        // Names that are not types (variables, constants, routines, units) give E0708
        if self.unit_named(&lower).is_some() {
            self.error(Code::E(708), format!("`{text}` is not a type"), range);
            return Ty::Error;
        }
        if self.broken_unit_visible() {
            // A unit that could not be read may declare it (`docs/spec/units.md` 3.4)
            return Ty::Error;
        }
        if self.is_unit_name(&lower)
            || (!lower.contains('.') && self.find_visible(&lower).is_some())
        {
            self.error(Code::E(708), format!("`{text}` is not a type"), range);
            return Ty::Error;
        }
        if lower.contains('.') {
            if let Some((unit, _)) = lower.rsplit_once('.')
                && self.builtin_unit_named(unit) == Some(super::UnitRef::System)
                && let Some((name, at)) = q.idents().last().map(|t| t.name_and_range())
                && self.unavailable_in_unit("System", &name, at)
            {
                return Ty::Error;
            }
            self.error(
                Code::E(700),
                format!("undeclared identifier `{text}`"),
                range,
            );
            return Ty::Error;
        }
        match builtins::lookup_system(&lower) {
            Some(builtins::SystemName::Reserved(m)) => {
                self.error(
                    Code::E(3200),
                    format!("`{text}` is not supported yet (planned for {m})"),
                    range,
                );
            }
            Some(builtins::SystemName::Deleted(code)) => {
                self.deleted_name(code, &text, range);
            }
            Some(_) => {
                self.error(Code::E(708), format!("`{text}` is not a type"), range);
            }
            None if lower == "notrefcounted" => self.not_ref_counted_misplaced(range),
            None if self.unavailable_name(&text, range) => {}
            None if self.generic_needs_args(&text, range) => {}
            None => {
                let d = Diagnostic::error(
                    Code::E(700),
                    format!("undeclared identifier `{text}`"),
                    Stage::Sema,
                )
                .with_primary(self.span(range), "");
                let d = self.with_uses_unit_fix(d, &lower);
                self.push(d);
            }
        }
        Ty::Error
    }

    // ---- Constant shapes (`docs/spec/types.md` chapter 4) ----

    /// The value of a declaration whose type is written: a constant shape for a record or static
    /// array, otherwise an expression converted to the type.
    pub(super) fn lower_init(&mut self, e: &ast::Expr, ty: Ty, range_check: bool) -> Expr {
        let span = ast::trimmed_range(e.syntax());
        match e {
            ast::Expr::RecordConst(rc) => self.record_const(rc, ty, span),
            ast::Expr::ArrayConst(ac) => {
                let elems: Vec<ast::Expr> = ac.elements().collect();
                self.array_const(&elems, ty, span)
            }
            // A one-element array constant is written `(X)`
            ast::Expr::Paren(p) if self.types.slot_count(ty) == 1 && matches!(ty, Ty::Array(_)) => {
                let elems: Vec<ast::Expr> = p.inner().into_iter().collect();
                self.array_const(&elems, ty, span)
            }
            _ => {
                let x = self.lower_expected(e, ty);
                if ty.is_error() || x.ty.is_error() {
                    x
                } else {
                    self.coerce(x, ty, range_check, None)
                }
            }
        }
    }

    fn shape_mismatch(&mut self, ty: Ty, found: &str, span: TextRange) -> Expr {
        if !ty.is_error() {
            let t = self.tn(ty);
            self.error(
                Code::E(702),
                format!("type mismatch: expected `{t}`, found {found}"),
                span,
            );
        }
        error_expr(span)
    }

    /// One element of a shape: a constant of the element type.
    fn shape_element(&mut self, e: &ast::Expr, ty: Ty) -> Option<Const> {
        let x = self.lower_init(e, ty, true);
        if x.ty.is_error() {
            return None;
        }
        match x.value {
            Some(v) => Some(v),
            None => {
                self.error(Code::E(713), "constant expression required", x.span);
                None
            }
        }
    }

    fn array_const(&mut self, elems: &[ast::Expr], ty: Ty, span: TextRange) -> Expr {
        let Some((_, lo, hi, elem)) = self.types.array(ty) else {
            for e in elems {
                self.lower_expr(e);
            }
            return self.shape_mismatch(ty, "an array constant", span);
        };
        let n = (hi - lo + 1) as usize;
        if elems.len() != n {
            self.error(
                Code::E(702),
                format!("expected {n} elements, found {}", elems.len()),
                span,
            );
            return error_expr(span);
        }
        let mut items = Vec::with_capacity(n);
        let mut failed = false;
        for e in elems {
            match self.shape_element(e, elem) {
                Some(v) => items.push(v),
                None => failed = true,
            }
        }
        if failed {
            return error_expr(span);
        }
        const_shape(Const::Agg(items), ty, span)
    }

    fn record_const(&mut self, rc: &ast::RecordConst, ty: Ty, span: TextRange) -> Expr {
        let written: Vec<ast::FieldConst> = rc.fields().collect();
        if !matches!(ty, Ty::Record(_)) {
            for f in &written {
                if let Some(v) = f.value() {
                    self.lower_expr(&v);
                }
            }
            return self.shape_mismatch(ty, "a record constant", span);
        }
        let fields: Vec<Field> = self.types.fields(ty).to_vec();
        let tname = self.tn(ty);
        let mut items = Vec::with_capacity(fields.len());
        let mut failed = false;
        for (i, f) in written.iter().enumerate() {
            let Some(name_tok) = f.name() else {
                failed = true;
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let range = name_tok.text_range();
            let expected = fields.get(i);
            if !expected.is_some_and(|x| x.name.eq_ignore_ascii_case(&name)) {
                failed = true;
                if !fields.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
                    self.error(
                        Code::E(700),
                        format!("`{tname}` has no field `{name}`"),
                        range,
                    );
                } else if let Some(x) = expected {
                    let x = x.name.clone();
                    self.error(
                        Code::E(702),
                        format!("expected field `{x}`, found `{name}`"),
                        range,
                    )
                    .notes
                    .push("write every field of the record in declaration order".into());
                } else {
                    self.error(
                        Code::E(702),
                        format!("field `{name}` is written twice"),
                        range,
                    );
                }
                // One error per constant: the fields after it are out of step
                break;
            }
            // A `[weak]` field takes the values of its class type (`docs/spec/objects.md` 4.4)
            let fty = expected.map_or(Ty::Error, |x| self.types.strong(x.ty));
            match f.value().and_then(|v| self.shape_element(&v, fty)) {
                Some(v) => items.push(v),
                None => failed = true,
            }
        }
        if !failed && written.len() < fields.len() {
            let missing = fields[written.len()].name.clone();
            self.error(Code::E(702), format!("missing field `{missing}`"), span)
                .notes
                .push("write every field of the record in declaration order".into());
            failed = true;
        }
        if failed {
            return error_expr(span);
        }
        const_shape(Const::Agg(items), ty, span)
    }
}

fn error_expr(span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Error,
        ty: Ty::Error,
        value: None,
        span,
    }
}

fn const_shape(value: Const, ty: Ty, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Const { explicit: false },
        ty,
        value: Some(value),
        span,
    }
}
