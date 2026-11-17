//! Attributes on declarations (`docs/spec/attributes.md`): the language attributes `[weak]`
//! (`docs/spec/objects.md` 4.4), `[unsafe]` (E3018), `[Ref]` and `[Volatile]` (W0010), and the
//! attribute classes, checked where they are written (M3 creates no instance of them).

use super::Sema;
use super::hir::{AttrUse, CallArg, Expr, ExprKind, RoutineId};
use super::types::{MethodKind, TCUSTOMATTRIBUTE, TOBJECT, Ty, TypeId, TypeKind, TypeTable};
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{Span, TextRange};
use crate::syntax::ast::{self, AstNode};
use crate::syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

/// The attributes of the testing framework (requirements §11), E3200 until M5 unless an attribute
/// class of the name is found (`docs/spec/attributes.md` 5).
const TEST_ATTRIBUTES: [&str; 6] = ["test", "testcase", "setup", "teardown", "skip", "benchmark"];

/// An attribute naming an attribute class (or nothing found), read where it is written and
/// checked once the classes and their constructors are known ([`Sema::flush_attribute_checks`]).
#[derive(Debug)]
pub(super) struct UserAttr {
    attr: ast::Attribute,
    /// The names of the declaration it is written on.
    names: Vec<TextRange>,
    /// The classes whose bodies enclose the declaration (their nested types and constants are
    /// seen by the arguments).
    classes: Vec<TypeId>,
    /// The name with `Attribute` added, and the name as written (`docs/spec/attributes.md` 2).
    suffixed: Option<Found>,
    plain: Option<Found>,
}

/// What an attribute's name was found to be.
#[derive(Debug, Clone, Copy)]
enum Found {
    Class(TypeId),
    /// Something other than a class, with its declaration if known.
    Other(Option<Span>),
    /// A name whose use is itself an error (a name reserved for a later milestone).
    Broken,
}

/// The names declared by a declaration's node: those of its identifier list, its routine
/// heading's name, or its own name.
fn decl_names(decl: &SyntaxNode) -> Vec<TextRange> {
    if let Some(h) = decl
        .children()
        .find(|n| n.kind() == SyntaxKind::ROUTINE_HEADER)
    {
        return h
            .children()
            .find(|n| n.kind() == SyntaxKind::QUALIFIED_NAME)
            .map(|n| n.text_range())
            .into_iter()
            .collect();
    }
    if let Some(l) = decl.children().find(|n| n.kind() == SyntaxKind::IDENT_LIST) {
        return l
            .children_with_tokens()
            .filter(|e| e.kind() == SyntaxKind::IDENT)
            .map(|e| e.text_range())
            .collect();
    }
    decl.children_with_tokens()
        .find(|e| e.kind() == SyntaxKind::IDENT)
        .map(|e| e.text_range())
        .into_iter()
        .collect()
}

/// The attributes written on a declaration, to compare a parameter's in a declaration and in its
/// body (`docs/spec/attributes.md` 1.2, 11.4): one entry per attribute, so that `[A, B]` and
/// `[A][B]` give the same key, with the identifiers in lowercase (literals keep their spelling)
/// and empty parentheses left out (`[A]` and `[A()]` are the same); `None` when there are none.
pub(super) fn attributes_key(decl: &SyntaxNode) -> Option<String> {
    let text = |n: &SyntaxNode| {
        let mut out = String::new();
        for t in n.descendants_with_tokens().filter_map(|e| e.into_token()) {
            match t.kind() {
                k if k.is_trivia() => {}
                SyntaxKind::IDENT => out.push_str(&t.text().to_ascii_lowercase()),
                _ => out.push_str(t.text()),
            }
            out.push(' ');
        }
        out
    };
    let entries: Vec<String> = decl
        .children()
        .filter(|n| n.kind() == SyntaxKind::ATTRIBUTE_LIST)
        .flat_map(|l| l.children().filter(|n| n.kind() == SyntaxKind::ATTRIBUTE))
        .map(|a| {
            let a = ast::Attribute::cast(a).expect("an ATTRIBUTE node");
            let mut key = a
                .name()
                .map(|q| q.text().to_ascii_lowercase())
                .unwrap_or_default();
            if let Some(args) = a.args().filter(|l| l.args().next().is_some()) {
                key.push_str(&text(args.syntax()));
            }
            key
        })
        .collect();
    (!entries.is_empty()).then(|| entries.join(","))
}

/// The range of the attribute lists written on a declaration.
fn lists_range(decl: &SyntaxNode) -> Option<TextRange> {
    let mut lists = decl
        .children()
        .filter(|n| n.kind() == SyntaxKind::ATTRIBUTE_LIST);
    let first = lists.next()?.text_range();
    let last = lists.last().map_or(first, |l| l.text_range());
    Some(TextRange::new(first.start(), last.end()))
}

/// Where an attribute is written (`docs/spec/attributes.md` 1.2): `[weak]` can be applied to the
/// fields and variables only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttrSite {
    /// A field of a class or a record, or a `class var`.
    Field,
    /// A global variable, a variable of a `var` section or an inline variable.
    Var,
    Param,
    /// A type, a method or a property.
    Other,
}

/// The language attributes of a declaration, checked once the types they apply to are known
/// ([`Sema::flush_attribute_checks`]).
#[derive(Debug)]
pub(super) struct AttrCheck {
    attrs: Vec<ast::Attribute>,
    site: AttrSite,
    /// The declared type (the storage type before `[weak]`).
    ty: Ty,
}

/// Why `[weak]` cannot be applied to a type (`docs/spec/objects.md` 4.4, the notes of E0748).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotWeak {
    /// A class managed by hand: its references are not counted.
    Manual,
    /// Exactly `TObject`.
    TObject,
    /// Any other type (a string, an array, a procedural type, …).
    Other,
}

/// A language attribute (`docs/spec/attributes.md` 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Weak,
    Unsafe,
    /// `[Ref]` and `[Volatile]`: ignored with W0010.
    Ignored,
    /// `[ValueEquality]` and `[EqualityMember]` (`docs/spec/equality.md` 3.4).
    ValueEquality,
    EqualityMember,
}

/// The equality attributes of the file (`docs/spec/equality.md` 3.4, 3.5), read where they are
/// written and checked with the methods of the classes (the class's own `Equals`,
/// `GetHashCode` and `Equal` decide whether it is treated as `[ValueEquality]`).
#[derive(Debug, Default)]
pub(super) struct EqualityAttrs {
    /// The classes `[ValueEquality]` is written on: (the class's name, the attribute's name).
    pub value_equality: Vec<(TextRange, TextRange)>,
    /// `[EqualityMember]` on the fields of classes.
    pub members: Vec<EqualityMemberMark>,
    /// The type declarations (by name) and the classes and records (by id) that have an E0767:
    /// their checks of E0765 are not made (3.5).
    pub errored_decls: Vec<TextRange>,
    pub errored_classes: Vec<TypeId>,
}

/// `[EqualityMember]` written on the fields of a class.
#[derive(Debug)]
pub(super) struct EqualityMemberMark {
    pub class: TypeId,
    /// The names of the fields of the declaration.
    pub names: Vec<TextRange>,
    /// The attribute's name, and what removing the attribute removes.
    pub attr: TextRange,
    pub removal: TextRange,
}

fn language_attribute(a: &ast::Attribute) -> Option<Lang> {
    let name = a.name()?;
    let mut idents = name.idents();
    let first = idents.next()?;
    if idents.next().is_some() {
        // A qualified name is never a language attribute (2)
        return None;
    }
    match first.text().to_ascii_lowercase().as_str() {
        "weak" => Some(Lang::Weak),
        "unsafe" => Some(Lang::Unsafe),
        "ref" | "volatile" => Some(Lang::Ignored),
        "valueequality" => Some(Lang::ValueEquality),
        "equalitymember" => Some(Lang::EqualityMember),
        _ => None,
    }
}

/// Whether `[weak]` can be applied to a value of `ty`.
fn weak_applies(types: &TypeTable, ty: Ty) -> Result<(), NotWeak> {
    match ty {
        Ty::Error => Ok(()),
        Ty::Class(TOBJECT) => Err(NotWeak::TObject),
        Ty::Class(c) => match types.get(c).kind {
            TypeKind::Class {
                ref_counted: false, ..
            } => Err(NotWeak::Manual),
            _ => Ok(()),
        },
        // `docs/spec/objects.md` 4.4, 7.4
        Ty::Intf(_) => Ok(()),
        _ => Err(NotWeak::Other),
    }
}

/// The edit removing one attribute (`docs/spec/attributes.md` 3, the fixes of W0010): the whole
/// list with the space after it when it is the only one in its brackets; otherwise the attribute
/// with the `,` after it and the space after that, or, for the last one, with the `,` before it
/// and the space before that.
fn removal_range(attr: &SyntaxNode) -> TextRange {
    let Some(list) = attr.parent() else {
        return attr.text_range();
    };
    let is_trivia =
        |e: &SyntaxElement| matches!(e.kind(), SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE);
    let attrs = list
        .children()
        .filter(|n| n.kind() == SyntaxKind::ATTRIBUTE)
        .count();
    if attrs <= 1 {
        let mut end = list.text_range().end();
        let mut next = list.next_sibling_or_token();
        while let Some(e) = next.filter(|e| e.kind() == SyntaxKind::WHITESPACE) {
            end = e.text_range().end();
            next = e.next_sibling_or_token();
        }
        return TextRange::new(list.text_range().start(), end);
    }
    let range = attr.text_range();
    // A `,` after it: up to the start of what follows the spaces after the `,`
    let mut next = attr.next_sibling_or_token();
    while let Some(e) = next.clone().filter(is_trivia) {
        next = e.next_sibling_or_token();
    }
    if let Some(comma) = next.filter(|e| e.kind() == SyntaxKind::COMMA) {
        let mut end = comma.text_range().end();
        let mut after = comma.next_sibling_or_token();
        while let Some(e) = after.filter(is_trivia) {
            end = e.text_range().end();
            after = e.next_sibling_or_token();
        }
        return TextRange::new(range.start(), end);
    }
    // The last one: from the `,` before it, with the spaces before that
    let mut prev = attr.prev_sibling_or_token();
    while let Some(e) = prev.clone().filter(is_trivia) {
        prev = e.prev_sibling_or_token();
    }
    let mut start = range.start();
    if let Some(comma) = prev.filter(|e| e.kind() == SyntaxKind::COMMA) {
        start = comma.text_range().start();
        let mut before = comma.prev_sibling_or_token();
        while let Some(e) = before.filter(is_trivia) {
            start = e.text_range().start();
            before = e.prev_sibling_or_token();
        }
    }
    TextRange::new(start, range.end())
}

impl Sema<'_, '_> {
    /// The value an expression reads: the read of a `[weak]` variable or field
    /// (`ExprKind::Weak`) for one of type `Ty::Weak`, otherwise the expression itself.
    pub(super) fn weak_read(&self, e: Expr) -> Expr {
        match e.ty {
            Ty::Weak(_) => {
                let span = e.span;
                Expr {
                    ty: self.types.strong(e.ty),
                    kind: ExprKind::Weak(Box::new(e)),
                    value: None,
                    span,
                }
            }
            _ => e,
        }
    }

    /// Reads the attributes written on a declaration (`decl` is its node) and returns the type
    /// of its storage: `Ty::Weak` for a field or variable of a class marked `[weak]`, otherwise
    /// `ty`. The checks of the language attributes wait until the types are known (a class
    /// declared later in the same `type` section), and those of the attribute classes until
    /// their constructors are known; names are looked up here, where they are written.
    pub(super) fn decl_attributes(&mut self, decl: &SyntaxNode, site: AttrSite, ty: Ty) -> Ty {
        let attrs = ast::attributes(decl);
        if attrs.is_empty() {
            return ty;
        }
        let mut lang = Vec::new();
        let mut weak = false;
        let mut seen: Vec<String> = Vec::new();
        let mut equality = Vec::new();
        for a in attrs {
            let l = language_attribute(&a);
            if l.is_some() {
                // A language attribute written twice is the same as once, with no diagnostics
                // of its own (`docs/spec/attributes.md` 3)
                let key = a
                    .name()
                    .map(|n| n.text().to_ascii_lowercase())
                    .unwrap_or_default();
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
            }
            match l {
                Some(Lang::Ignored) => self.ignored_attribute(&a),
                Some(l @ (Lang::ValueEquality | Lang::EqualityMember)) => {
                    self.language_attribute_args(&a);
                    equality.push((l, a));
                }
                Some(Lang::Weak)
                    if site == AttrSite::Field
                        && self.class_scope().is_some_and(|c| self.types.is_helper(c)) =>
                {
                    // A helper has no fields, so no `[weak]` (`docs/spec/objects.md` 10.2)
                    let range = a
                        .name()
                        .map_or_else(|| a.syntax().text_range(), |n| n.syntax().text_range());
                    self.not_in_helper("[weak]", range);
                }
                Some(l) => {
                    weak |= l == Lang::Weak;
                    lang.push(a);
                }
                None => self.read_user_attribute(a, decl),
            }
        }
        for (l, a) in equality {
            let weak = weak && weak_applies(&self.types, ty).is_ok();
            self.equality_attribute(l, &a, decl, site, weak);
        }
        if lang.is_empty() {
            return ty;
        }
        self.attr_checks.push(AttrCheck {
            attrs: lang,
            site,
            ty,
        });
        match ty {
            Ty::Class(c) | Ty::Intf(c)
                if weak && c != TOBJECT && matches!(site, AttrSite::Field | AttrSite::Var) =>
            {
                Ty::Weak(c)
            }
            t => t,
        }
    }

    /// E3401 for the attributes of a declaration where none can be written (a constant, a nested
    /// routine; `docs/spec/attributes.md` 1.2). They are not read further.
    pub(super) fn attributes_not_allowed(&mut self, decl: &SyntaxNode) {
        let Some(range) = lists_range(decl) else {
            return;
        };
        self.error(Code::E(3401), "attributes are not allowed here", range)
            .notes
            .push(crate::parser::ATTRIBUTE_PLACES_NOTE.into());
    }

    /// E3401 for the attributes of a declaration that completes an earlier one (the body of a
    /// forward routine or a method, the full declaration of a class; `docs/spec/attributes.md`
    /// 1.2): `full` names the variant for a forward declaration of a class or an interface, and
    /// `declared` is the other declaration.
    pub(super) fn attributes_on_forward(
        &mut self,
        decl: &SyntaxNode,
        name: &str,
        declared: Option<Span>,
        full: bool,
    ) {
        let Some(range) = lists_range(decl) else {
            return;
        };
        let what = if full {
            "full declaration"
        } else {
            "declaration"
        };
        let mut d = Diagnostic::error(
            Code::E(3401),
            format!("attributes must be written on the {what} of `{name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        if let Some(s) = declared {
            d = d.with_secondary(s, "declared here");
        }
        self.push(d);
    }

    /// E0705 when a language attribute is given arguments (`[weak()]` has none).
    fn language_attribute_args(&mut self, a: &ast::Attribute) {
        if let Some(args) = a.args()
            && !args.has_error()
        {
            let n = args.args().count();
            if n > 0 {
                let r = args.syntax().text_range();
                self.error(
                    Code::E(705),
                    format!("wrong number of arguments: expected 0, found {n}"),
                    r,
                );
            }
        }
    }

    /// `[Ref]` / `[Volatile]` (`docs/spec/attributes.md` 3): W0010, with the attribute removed.
    fn ignored_attribute(&mut self, a: &ast::Attribute) {
        self.language_attribute_args(a);
        let name = a.name().map(|n| n.text()).unwrap_or_default();
        let range = a
            .name()
            .map(|n| n.syntax().text_range())
            .unwrap_or_else(|| a.syntax().text_range());
        let edit = TextEdit::delete(self.input.file, removal_range(a.syntax()));
        let d = Diagnostic::warning(
            Code::W(10),
            format!("attribute `[{name}]` is ignored"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "")
        .with_fix(Fix::safe(format!("remove `[{name}]`"), vec![edit]));
        self.push(d);
    }

    /// Looks the name of an attribute up where it is written (`docs/spec/attributes.md` 2) and
    /// queues its checks.
    fn read_user_attribute(&mut self, attr: ast::Attribute, decl: &SyntaxNode) {
        let Some(q) = attr.name() else {
            return;
        };
        let suffixed = self.find_attribute_name(&q, true);
        let plain = self.find_attribute_name(&q, false);
        let classes = self.class_scopes.iter().map(|c| c.class).collect();
        self.user_attrs.push(UserAttr {
            attr,
            names: decl_names(decl),
            classes,
            suffixed,
            plain,
        });
    }

    /// What the name of an attribute (with `Attribute` added to its last part when `suffix`)
    /// denotes, without reporting anything; `None` when nothing is declared with the name.
    fn find_attribute_name(&mut self, q: &ast::QualifiedName, suffix: bool) -> Option<Found> {
        let mut chain: Vec<(String, TextRange)> = q
            .idents()
            .map(|t| (t.text().to_string(), t.text_range()))
            .collect();
        if suffix {
            chain.last_mut()?.0.push_str("Attribute");
        }
        let range = q.syntax().text_range();
        if chain.len() == 1 {
            let name = chain[0].0.clone();
            let (r, undeclared) = self.dry_run(|s| {
                let n = s.sink.len();
                let r = s.resolve_ident(&name, range);
                let undeclared = s.sink.iter().skip(n).any(|d| d.code == Some(Code::E(700)));
                (r, undeclared)
            });
            return match r {
                super::Resolved::Type(Ty::Class(c)) => Some(Found::Class(c)),
                super::Resolved::Error if undeclared => None,
                super::Resolved::Error => Some(Found::Broken),
                _ => {
                    let lower = name.to_ascii_lowercase();
                    Some(Found::Other(
                        self.find_visible(&lower).map(|r| self.span(r)),
                    ))
                }
            };
        }
        // A name qualified with a unit, or a nested class (`TOuter.TInnerAttribute`)
        if let Some((u, n)) = self.unit_prefix(&chain)
            && n + 1 == chain.len()
        {
            let (name, r) = chain[n].clone();
            return match self.dry_run(|s| s.resolve_in_unit(u, &name, r)) {
                super::Resolved::Type(Ty::Class(c)) => Some(Found::Class(c)),
                super::Resolved::Error => None,
                _ => Some(Found::Other(None)),
            };
        }
        let text: Vec<&str> = chain.iter().map(|(t, _)| t.as_str()).collect();
        self.dry_run(|s| s.class_named(&text.join(".")))
            .map(Found::Class)
    }

    /// Reports the diagnostics of the attributes read so far ([`Sema::decl_attributes`]): those
    /// of the language attributes, and those of the attribute classes unless a `type` section
    /// is being declared.
    pub(super) fn flush_attribute_checks(&mut self) {
        for check in std::mem::take(&mut self.attr_checks) {
            for a in &check.attrs {
                self.check_language_attribute(a, check.site, check.ty);
            }
        }
        if self.attr_defer > 0 {
            return;
        }
        for u in std::mem::take(&mut self.user_attrs) {
            self.check_user_attribute(u);
        }
    }

    /// Whether `f` is an attribute class (`docs/spec/attributes.md` 4.1).
    fn attribute_class(&self, f: Option<Found>) -> Option<TypeId> {
        match f {
            Some(Found::Class(c)) if self.types.is_descendant(c, TCUSTOMATTRIBUTE) => Some(c),
            _ => None,
        }
    }

    /// The checks of an attribute that is not a language attribute (`docs/spec/attributes.md` 2,
    /// 4.2, 5).
    fn check_user_attribute(&mut self, u: UserAttr) {
        let Some(q) = u.attr.name() else {
            return;
        };
        let text = q.text();
        let name_range = q.syntax().text_range();
        let class = self
            .attribute_class(u.suffixed)
            .or_else(|| self.attribute_class(u.plain));
        let Some(class) = class else {
            let lower = text.to_ascii_lowercase();
            if TEST_ATTRIBUTES.contains(&lower.as_str()) {
                self.error(
                    Code::E(3200),
                    format!("`[{text}]` is not supported yet (planned for M5)"),
                    name_range,
                );
                return;
            }
            let found = match (u.suffixed, u.plain) {
                (Some(f @ (Found::Class(_) | Found::Other(_))), _) => {
                    Some((f, format!("{text}Attribute")))
                }
                (_, Some(f @ (Found::Class(_) | Found::Other(_)))) => Some((f, text.clone())),
                _ => None,
            };
            if let Some((f, name)) = found {
                let decl = match f {
                    Found::Class(c) => {
                        let span = self.types.get(c).span;
                        (span != TextRange::default())
                            .then(|| self.type_decl_span(Ty::Class(c), span))
                    }
                    Found::Other(s) => s,
                    Found::Broken => None,
                };
                let mut d = Diagnostic::error(
                    Code::E(3402),
                    format!("`{name}` is not an attribute class"),
                    Stage::Sema,
                )
                .with_primary(self.span(name_range), "")
                .with_note("an attribute class must inherit from `TCustomAttribute`");
                if let Some(s) = decl {
                    d = d.with_secondary(s, "declared here");
                }
                self.push(d);
                return;
            }
            if matches!(u.suffixed, Some(Found::Broken)) || matches!(u.plain, Some(Found::Broken)) {
                return;
            }
            let d = Diagnostic::warning(
                Code::W(25),
                format!("unknown attribute `{text}`"),
                Stage::Sema,
            )
            .with_primary(self.span(name_range), "")
            .with_note("an attribute is a class that inherits from `TCustomAttribute`")
            .with_note("this attribute is ignored");
            self.push(d);
            return;
        };
        // The attribute class is used here (`docs/spec/attributes.md` 6.2)
        if let Some(decl) = self.class_decl_span(class) {
            self.hint_use(decl, name_range);
        }
        // The arguments see what the declaration sees
        for c in &u.classes {
            self.class_scopes.push(super::statics::ClassScope {
                class: *c,
                vis: super::types::Visibility::Public,
                names: Vec::new(),
            });
        }
        let calls = self.cur.as_ref().map(|c| c.calls.len());
        let instance = self.attribute_instance(class, &u.attr, name_range);
        if let (Some(n), Some(c)) = (calls, self.cur.as_mut()) {
            // No call is made (M3 creates no instance)
            c.calls.truncate(n);
        }
        for _ in &u.classes {
            self.class_scopes.pop();
        }
        let Some((ctor, args)) = instance else {
            return;
        };
        let class_name = self
            .types
            .get(class)
            .full_name
            .clone()
            .unwrap_or_else(|| self.types.name(Ty::Class(class)));
        for decl in u.names {
            self.attr_uses.push(AttrUse {
                unit: self.unit_index,
                decl,
                class: class_name.clone(),
                ctor,
                args: args.clone(),
            });
        }
    }

    /// The constructor an attribute class's instance would be created with, and the arguments
    /// (`docs/spec/attributes.md` 4.2): chosen among all the constructors of the class by the
    /// arguments, as `TFoo.Create(…)` chooses among the overloads. `None` when an error was
    /// reported.
    fn attribute_instance(
        &mut self,
        class: TypeId,
        attr: &ast::Attribute,
        name_range: TextRange,
    ) -> Option<(Option<RoutineId>, Vec<Expr>)> {
        let range = ast::trimmed_range(attr.syntax());
        let args: Vec<ast::Arg> = attr.args().map(|l| l.args().collect()).unwrap_or_default();
        let lower_all = |s: &mut Self| {
            for a in &args {
                if let Some(v) = a.value() {
                    s.lower_expr(&v);
                }
            }
        };
        if self.check_abstract_create(class, name_range) {
            lower_all(self);
            return None;
        }
        let mut names: Vec<String> = Vec::new();
        for c in self.types.class_chain(class) {
            for m in self.types.class_methods(c) {
                let lower = m.name.to_ascii_lowercase();
                if m.kind == MethodKind::Constructor && !names.contains(&lower) {
                    names.push(lower);
                }
            }
        }
        let mut set: Vec<RoutineId> = Vec::new();
        for n in &names {
            for r in self.method_set(class, n) {
                if !set.contains(&r) {
                    set.push(r);
                }
            }
        }
        if set.is_empty() {
            let hidden = names.iter().find_map(|n| {
                self.methods_named(class, n)
                    .into_iter()
                    .find(|(_, m)| m.routine.is_some())
            });
            lower_all(self);
            if let Some((owner, m)) = hidden {
                // Only constructors the declaration cannot see (`docs/spec/objects.md` 2.5)
                let owner_name = self.types.name(Ty::Class(owner));
                let msg = match m.vis {
                    super::types::Visibility::StrictPrivate | super::types::Visibility::Private => {
                        format!("`{}` is private to `{owner_name}`", m.name)
                    }
                    _ => format!("`{}` is protected in `{owner_name}`", m.name),
                };
                let mut d = Diagnostic::error(Code::E(743), msg, Stage::Sema)
                    .with_primary(self.span(range), "");
                if let Some(s) = self.method_span(owner, &m) {
                    d = d.with_secondary(s, "declared here");
                }
                self.push(d);
                return None;
            }
            // The built-in `TObject.Create`
            if !args.is_empty() {
                self.error(
                    Code::E(705),
                    format!(
                        "wrong number of arguments: expected 0, found {}",
                        args.len()
                    ),
                    range,
                );
                return None;
            }
            return Some((None, Vec::new()));
        }
        let ctor = match self.pick_method(set, &args, range, false) {
            super::overload::Pick::Routine(r) => r,
            super::overload::Pick::Failed => return None,
        };
        let call = self.lower_routine_call(ctor, &args, range);
        let ExprKind::Call { args: values, .. } = call.kind else {
            return None;
        };
        let mut ok = true;
        let mut out = Vec::new();
        for (a, v) in args.iter().zip(values) {
            let e = match v {
                CallArg::Value(e) => e,
                CallArg::Ref { .. } | CallArg::Temp(_) => {
                    ok = false;
                    let r = a.value().map_or_else(
                        || a.syntax().text_range(),
                        |v| ast::trimmed_range(v.syntax()),
                    );
                    self.error(Code::E(713), "constant expression required", r);
                    continue;
                }
            };
            if e.ty.is_error() {
                ok = false;
                continue;
            }
            if e.value.is_none() && !matches!(e.kind, ExprKind::ClassRefValue(_)) {
                ok = false;
                self.error(Code::E(713), "constant expression required", e.span);
                continue;
            }
            out.push(e);
        }
        ok.then_some((Some(ctor), out))
    }

    /// `[ValueEquality]` / `[EqualityMember]` (`docs/spec/equality.md` 3.4): E0767 where they
    /// cannot be written; on a class, or on a field of a class, they are kept for the checks
    /// with the class's methods (`weak`: the field is `[weak]`).
    fn equality_attribute(
        &mut self,
        l: Lang,
        a: &ast::Attribute,
        decl: &SyntaxNode,
        site: AttrSite,
        weak: bool,
    ) {
        let attr = a
            .name()
            .map_or_else(|| a.syntax().text_range(), |n| n.syntax().text_range());
        let removal = self.whole_line(removal_range(a.syntax()));
        let type_decl = ast::TypeDecl::cast(decl.clone());
        let class = self.class_scope();
        let mut record_note = false;
        if l == Lang::ValueEquality {
            let spec = type_decl.as_ref().and_then(|d| d.type_spec());
            if let (Some(ast::TypeSpec::Class(c)), Some(name)) =
                (&spec, type_decl.as_ref().and_then(|d| d.name()))
                && !c.is_forward()
            {
                self.equality_attrs
                    .value_equality
                    .push((name.text_range(), attr));
                return;
            }
            record_note = matches!(spec, Some(ast::TypeSpec::Record(_)));
        } else if site == AttrSite::Field
            && let Some(c) = class
            && matches!(self.types.get(c).kind, TypeKind::Class { .. })
            && decl
                .parent()
                .is_none_or(|p| p.kind() != SyntaxKind::CLASS_VAR_SECTION)
        {
            if weak {
                // `docs/spec/equality.md` 3.4: its value changes when the instance is destroyed
                for n in decl_names(decl) {
                    let name = self.src(n).to_string();
                    self.error(
                        Code::E(767),
                        format!("`[weak]` field `{name}` cannot be an equality member"),
                        n,
                    );
                }
                self.equality_attrs.errored_classes.push(c);
                return;
            }
            self.equality_attrs.members.push(EqualityMemberMark {
                class: c,
                names: decl_names(decl),
                attr,
                removal,
            });
            return;
        }
        // Where it cannot be written: the declaration is not checked further (3.5)
        match (&type_decl, class) {
            (Some(d), _) => {
                if let Some(n) = d.name() {
                    self.equality_attrs.errored_decls.push(n.text_range());
                }
            }
            (None, Some(c)) => self.equality_attrs.errored_classes.push(c),
            _ => {}
        }
        let name = if l == Lang::ValueEquality {
            "ValueEquality"
        } else {
            "EqualityMember"
        };
        self.misplaced_equality_attribute(name, attr, removal, record_note);
    }

    /// `range` with its line when nothing else is on the line (an attribute list written on a
    /// line of its own): removing it leaves no empty line.
    fn whole_line(&self, range: TextRange) -> TextRange {
        let bytes = self.input.bytes;
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        let line_start = bytes[..start]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        if !bytes[line_start..start]
            .iter()
            .all(|&b| b == b' ' || b == b'\t')
        {
            return range;
        }
        let after = match bytes.get(end..) {
            Some([b'\r', b'\n', ..]) => end + 2,
            Some([b'\n', ..]) => end + 1,
            _ => return range,
        };
        TextRange::new(
            crate::source::TextSize::from(line_start as u32),
            crate::source::TextSize::from(after as u32),
        )
    }

    /// E0767 for `[ValueEquality]` / `[EqualityMember]` written where it cannot be
    /// (`docs/spec/equality.md` 3.4), with the attribute removed (`removal`).
    pub(super) fn misplaced_equality_attribute(
        &mut self,
        name: &str,
        attr: TextRange,
        removal: TextRange,
        record_note: bool,
    ) {
        let message = if name == "ValueEquality" {
            "`[ValueEquality]` can only be applied to a class"
        } else {
            "`[EqualityMember]` can only be applied to a field of a class with `[ValueEquality]`"
        };
        let mut d =
            Diagnostic::error(Code::E(767), message, Stage::Sema).with_primary(self.span(attr), "");
        if record_note {
            d = d.with_note("records are already compared field by field");
        }
        d = d.with_fix(Fix::safe(
            format!("remove `[{name}]`"),
            vec![TextEdit::delete(self.input.file, removal)],
        ));
        self.push(d);
    }

    fn check_language_attribute(&mut self, a: &ast::Attribute, site: AttrSite, ty: Ty) {
        let Some(lang) = language_attribute(a) else {
            return;
        };
        // The primary of E0748 / E3018 is the attribute's name (`docs/spec/diagnostics.md`)
        let range = a
            .name()
            .map(|n| n.syntax().text_range())
            .unwrap_or_else(|| a.syntax().text_range());
        // A language attribute takes no arguments (`[weak()]` has none)
        self.language_attribute_args(a);
        let fits = match site {
            AttrSite::Field | AttrSite::Var => weak_applies(&self.types, ty),
            AttrSite::Param | AttrSite::Other => Err(NotWeak::Other),
        };
        match lang {
            Lang::Ignored | Lang::ValueEquality | Lang::EqualityMember => {}
            Lang::Weak => {
                if matches!(site, AttrSite::Param | AttrSite::Other) {
                    self.error(Code::E(748), "`[weak]` is not allowed here", range);
                    return;
                }
                let Err(why) = fits else {
                    return;
                };
                let note = match why {
                    NotWeak::Manual => {
                        "references to a manually managed class are not counted, so `[weak]` is not needed"
                    }
                    NotWeak::TObject => {
                        "`[weak]` needs a reference counted class type; declare the variable with that class type instead of `TObject`"
                    }
                    NotWeak::Other => {
                        "`[weak]` in Puy can only be applied to reference counted classes and interfaces"
                    }
                };
                self.error(
                    Code::E(748),
                    "`[weak]` can only be applied to a reference counted class or an interface",
                    range,
                )
                .notes
                .push(note.into());
            }
            Lang::Unsafe => {
                let file = self.input.file;
                let mut d = Diagnostic::error(
                    Code::E(3018),
                    "`[unsafe]` is not supported in Puy",
                    Stage::Sema,
                )
                .with_primary(self.span(range), "");
                match fits {
                    Ok(()) => {
                        // `[weak]` fits here: the name is replaced
                        let name = a
                            .name()
                            .and_then(|n| n.last_ident())
                            .map(|t| t.text_range())
                            .unwrap_or(range);
                        d = d.with_note("use `[weak]` instead").with_fix(Fix::safe(
                            "use `[weak]`",
                            vec![TextEdit::replace(file, name, "weak")],
                        ));
                    }
                    Err(why) => {
                        let note = match (site, why) {
                            (AttrSite::Param, _) => {
                                "`[weak]` and `[unsafe]` cannot be applied to parameters; use a `const` parameter to avoid counting"
                            }
                            (AttrSite::Other, _) => {
                                // `[weak]` cannot be written here either (`docs/spec/attributes.md` 3)
                                "`[weak]` and `[unsafe]` can only be applied to fields and variables"
                            }
                            (_, NotWeak::Manual) => {
                                "references to a manually managed class are not counted, so `[unsafe]` is not needed"
                            }
                            _ => {
                                "`[weak]` in Puy can only be applied to reference counted classes and interfaces"
                            }
                        };
                        let edit = TextEdit::delete(file, removal_range(a.syntax()));
                        d = d
                            .with_note(note)
                            .with_fix(Fix::suggestion("remove `[unsafe]`", vec![edit]));
                    }
                }
                self.push(d);
            }
        }
    }
}
