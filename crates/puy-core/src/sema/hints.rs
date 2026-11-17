//! Hint directives (`docs/spec/attributes.md` chapter 6): the `deprecated` / `experimental`
//! marks of declarations and the warnings at their uses (W0011, W0012), and `platform` /
//! `library` in the positions other than routines (W0010).
//!
//! A declaration's marks are kept under the span of its name (the `declared here` of the
//! warnings). Uses are recorded where names are resolved, with the declarations enclosing them,
//! and reported once the file has been analyzed: a use can come before the declaration it names
//! is read (a later type of the same `type` section), and uses found in a trial analysis
//! ([`Sema::dry_run`]) are uses too.

use super::Sema;
use crate::diagnostics::{Code, Diagnostic, Fix, Stage, TextEdit};
use crate::source::{Span, TextRange};
use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
use std::collections::HashMap;

/// The `deprecated` / `experimental` marks of a declaration: `Some` with the description, if
/// one is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Hints {
    pub deprecated: Option<Option<String>>,
    pub experimental: Option<Option<String>>,
}

impl Hints {
    pub fn is_empty(&self) -> bool {
        self.deprecated.is_none() && self.experimental.is_none()
    }
}

/// A marked declaration: the name the warnings show (members qualified with their type, a unit
/// by its full name) and its marks.
#[derive(Debug, Clone)]
pub(super) struct HintInfo {
    name: String,
    unit: bool,
    hints: Hints,
}

/// The marked declarations of a compilation, by the span of their names.
pub(super) type HintTable = HashMap<Span, HintInfo>;

/// A use of a name: where it is written, the declaration it resolved to, and the declarations
/// enclosing it (an index into [`HintState::snaps`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct HintUse {
    range: TextRange,
    decl: Span,
    ctx: usize,
}

/// The state of one file's analysis.
#[derive(Debug)]
pub(super) struct HintState {
    /// The declarations being analyzed, outermost first (their uses inside them are not
    /// reported).
    stack: Vec<Span>,
    /// The stacks the uses were recorded under (the first is the empty one).
    snaps: Vec<Vec<Span>>,
    /// The current stack in `snaps`.
    cur: usize,
    uses: Vec<HintUse>,
    seen: std::collections::HashSet<(TextRange, Span)>,
}

impl Default for HintState {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            snaps: vec![Vec::new()],
            cur: 0,
            uses: Vec::new(),
            seen: Default::default(),
        }
    }
}

/// The hint directive nodes of a declaration: its own directives, and for a type those written
/// after `class` / `interface` and their parents.
fn directive_nodes(decl: &SyntaxNode) -> Vec<SyntaxNode> {
    let is_directive = |n: &SyntaxNode| {
        matches!(
            n.kind(),
            SyntaxKind::HINT_DIRECTIVE
                | SyntaxKind::ROUTINE_DIRECTIVE
                | SyntaxKind::METHOD_DIRECTIVE
        )
    };
    let mut out: Vec<SyntaxNode> = decl.children().filter(is_directive).collect();
    for t in decl.children().filter(|n| {
        matches!(
            n.kind(),
            SyntaxKind::CLASS_TYPE | SyntaxKind::INTERFACE_TYPE
        )
    }) {
        out.extend(t.children().filter(is_directive));
    }
    out.sort_by_key(|n| n.text_range().start());
    out
}

/// The keyword of a hint directive node.
fn keyword(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.first_token()
}

/// The `deprecated` / `experimental` directives written on a declaration's node, by keyword.
pub(super) fn mark_tokens(decl: &SyntaxNode) -> Vec<SyntaxToken> {
    directive_nodes(decl)
        .iter()
        .filter_map(keyword)
        .filter(|k| {
            matches!(
                k.kind(),
                SyntaxKind::DEPRECATED_KW | SyntaxKind::EXPERIMENTAL_KW
            )
        })
        .collect()
}

impl Sema<'_, '_> {
    /// Reads the hint directives of a declaration's node: the `deprecated` / `experimental`
    /// marks, and W0010 for `platform` / `library` written in the positions of
    /// `docs/spec/attributes.md` 6.1 other than routines and units (whose directives are
    /// diagnosed with the others of their heading).
    pub(super) fn read_hints(&mut self, decl: &SyntaxNode) -> Hints {
        let mut hints = Hints::default();
        for node in directive_nodes(decl) {
            let Some(kw) = keyword(&node) else { continue };
            match kw.kind() {
                SyntaxKind::DEPRECATED_KW | SyntaxKind::EXPERIMENTAL_KW => {
                    let text = node
                        .children_with_tokens()
                        .filter_map(|e| e.into_token())
                        .find(|t| t.kind() == SyntaxKind::STRING_LIT)
                        .and_then(|t| self.string_literal(&t))
                        .map(|units| description(&units));
                    if kw.kind() == SyntaxKind::DEPRECATED_KW {
                        hints.deprecated = Some(text);
                    } else {
                        hints.experimental = Some(text);
                    }
                }
                SyntaxKind::PLATFORM_KW | SyntaxKind::LIBRARY_KW
                    if node.kind() == SyntaxKind::HINT_DIRECTIVE =>
                {
                    let text = self.text(&kw).to_string();
                    let del =
                        crate::diagnostics::deletion_range(self.input.bytes, node.text_range());
                    let d = Diagnostic::warning(
                        Code::W(10),
                        format!("hint directive `{text}` is ignored"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(kw.text_range()), "")
                    .with_fix(Fix::safe(
                        format!("remove `{text}`"),
                        vec![TextEdit::delete(self.input.file, del)],
                    ));
                    self.push(d);
                }
                _ => {}
            }
        }
        hints
    }

    /// Reads the marks of a declaration's node and keeps them under `name_span` (in this file),
    /// shown as `name`. Returns the marks.
    pub(super) fn declare_hints(
        &mut self,
        decl: &SyntaxNode,
        name_span: TextRange,
        name: String,
    ) -> Hints {
        let hints = self.read_hints(decl);
        self.keep_hints(self.span(name_span), name, false, hints.clone());
        hints
    }

    /// Reads the marks of a declaration of the names `names` (a variable or field declaration
    /// with an identifier list, a constant) and keeps them under each name, shown qualified with
    /// `owner` if given. Returns the name of the first one if marked: the declaration's scope
    /// ([`Sema::push_hint_scope`]) for its type and initial value.
    pub(super) fn declare_names_hints(
        &mut self,
        decl: &SyntaxNode,
        names: &[SyntaxToken],
        owner: Option<&str>,
    ) -> Option<Span> {
        let hints = self.read_hints(decl);
        if hints.is_empty() {
            return None;
        }
        for n in names {
            let text = self.text(n);
            let name = match owner {
                Some(o) => format!("{o}.{text}"),
                None => text.to_string(),
            };
            self.keep_hints(self.span(n.text_range()), name, false, hints.clone());
        }
        names.first().map(|n| self.span(n.text_range()))
    }

    /// [`Sema::declare_names_hints`] for a member of the class or record body being declared.
    pub(super) fn declare_member_hints(
        &mut self,
        decl: &SyntaxNode,
        names: &[SyntaxToken],
    ) -> Option<Span> {
        let owner = self
            .class_scope()
            .and_then(|c| self.types.get(c).name.clone());
        self.declare_names_hints(decl, names, owner.as_deref())
    }

    /// Ends the scope [`Sema::declare_names_hints`] returned, if any.
    pub(super) fn end_names_hints(&mut self, scope: Option<Span>) {
        if scope.is_some() {
            self.pop_hint_scope();
        }
    }

    /// Starts the scope [`Sema::declare_names_hints`] returned, if any.
    pub(super) fn begin_names_hints(&mut self, scope: Option<Span>) {
        if let Some(s) = scope {
            self.push_hint_scope(s);
        }
    }

    /// Keeps the marks of the declaration named at `decl`.
    pub(super) fn keep_hints(&mut self, decl: Span, name: String, unit: bool, hints: Hints) {
        if hints.is_empty() {
            return;
        }
        self.hints.insert(decl, HintInfo { name, unit, hints });
    }

    /// E3401 for the `deprecated` / `experimental` directives of a declaration completing an
    /// earlier one that does not have them (the body of a forward routine or a method, the full
    /// declaration of a class; `docs/spec/attributes.md` 6.1): `full` names the variant for a
    /// forward declaration of a class or an interface, and `declared` is the other declaration
    /// (whose marks are those kept under it). The same directive written again is accepted.
    pub(super) fn hints_on_forward(
        &mut self,
        decl: &SyntaxNode,
        name: &str,
        declared: Option<Span>,
        full: bool,
    ) {
        let kept = declared
            .and_then(|s| self.hints.get(&s))
            .map(|h| h.hints.clone())
            .unwrap_or_default();
        for kw in mark_tokens(decl) {
            let again = match kw.kind() {
                SyntaxKind::DEPRECATED_KW => kept.deprecated.is_some(),
                _ => kept.experimental.is_some(),
            };
            if again && !full {
                continue;
            }
            let text = self.text(&kw).to_ascii_lowercase();
            let what = if full {
                "full declaration"
            } else {
                "declaration"
            };
            let mut d = Diagnostic::error(
                Code::E(3401),
                format!("`{text}` must be written on the {what} of `{name}`"),
                Stage::Sema,
            )
            .with_primary(self.span(kw.text_range()), "");
            if let Some(s) = declared {
                d = d.with_secondary(s, "declared here");
            }
            self.push(d);
        }
    }

    /// Analyzes `f` inside the declaration named at `decl`: uses of the declaration itself, and
    /// of declarations with the same marks as one enclosing them, are not reported
    /// (`docs/spec/attributes.md` 6.2).
    pub(super) fn in_hint_scope<T>(&mut self, decl: Span, f: impl FnOnce(&mut Self) -> T) -> T {
        self.push_hint_scope(decl);
        let r = f(self);
        self.pop_hint_scope();
        r
    }

    pub(super) fn push_hint_scope(&mut self, decl: Span) {
        let h = &mut self.hint_state;
        h.stack.push(decl);
        h.snaps.push(h.stack.clone());
        h.cur = h.snaps.len() - 1;
    }

    pub(super) fn pop_hint_scope(&mut self) {
        let h = &mut self.hint_state;
        h.stack.pop();
        h.snaps.push(h.stack.clone());
        h.cur = h.snaps.len() - 1;
    }

    /// Records a use, at `range` in this file, of the declaration named at `decl`.
    pub(super) fn hint_use(&mut self, decl: Span, range: TextRange) {
        let h = &mut self.hint_state;
        if h.seen.insert((range, decl)) {
            h.uses.push(HintUse {
                range,
                decl,
                ctx: h.cur,
            });
        }
    }

    /// The number of uses recorded so far, for [`Sema::drop_hint_uses`].
    pub(super) fn hint_uses_mark(&self) -> usize {
        self.hint_state.uses.len()
    }

    /// Forgets the uses recorded since `mark`: those in the heading of a body completing an
    /// earlier declaration, which is not a use (`docs/spec/attributes.md` 6.2).
    pub(super) fn drop_hint_uses(&mut self, mark: usize) {
        self.hint_state.uses.truncate(mark);
    }

    /// [`Sema::hint_use`] for a routine (or a method).
    pub(super) fn routine_hint_use(&mut self, id: super::hir::RoutineId, range: TextRange) {
        let decl = self.routine_decl_span(id);
        self.hint_use(decl, range);
    }

    /// W0011 / W0012 for the recorded uses of marked declarations (`docs/spec/attributes.md`
    /// 6.2), once the file has been analyzed.
    pub(super) fn report_hint_uses(&mut self) {
        let state = std::mem::take(&mut self.hint_state);
        for u in &state.uses {
            let Some(info) = self.hints.get(&u.decl).cloned() else {
                continue;
            };
            let ctx = &state.snaps[u.ctx];
            if ctx.contains(&u.decl) {
                continue;
            }
            let marked = |pick: fn(&Hints) -> bool| {
                ctx.iter()
                    .any(|s| self.hints.get(s).is_some_and(|i| pick(&i.hints)))
            };
            let in_deprecated = marked(|h| h.deprecated.is_some());
            let in_experimental = marked(|h| h.experimental.is_some());
            let unit = if info.unit { "unit " } else { "" };
            let name = &info.name;
            if let Some(text) = &info.hints.deprecated
                && !in_deprecated
            {
                let message = match text {
                    Some(t) => format!("{unit}`{name}` is deprecated: {t}"),
                    None => format!("{unit}`{name}` is deprecated"),
                };
                let d = Diagnostic::warning(Code::W(11), message, Stage::Sema)
                    .with_primary(self.span(u.range), "")
                    .with_secondary(u.decl, "declared here");
                self.push(d);
            }
            if let Some(text) = &info.hints.experimental
                && !in_experimental
            {
                let message = match text {
                    Some(t) => format!("{unit}`{name}` is experimental: {t}"),
                    None => format!("{unit}`{name}` is experimental"),
                };
                let d = Diagnostic::warning(Code::W(12), message, Stage::Sema)
                    .with_primary(self.span(u.range), "")
                    .with_secondary(u.decl, "declared here")
                    .with_note(
                        "an experimental declaration may change or be removed without being \
                         deprecated first",
                    );
                self.push(d);
            }
        }
    }
}

/// The marked declarations as the `hir` dump shows them.
pub(super) fn marks(table: &HintTable) -> Vec<super::hir::HintMark> {
    table
        .iter()
        .map(|(decl, info)| super::hir::HintMark {
            decl: *decl,
            deprecated: info.hints.deprecated.is_some(),
            experimental: info.hints.experimental.is_some(),
        })
        .collect()
}

/// The description of a mark as the warnings show it: line breaks become a single space.
fn description(units: &[u16]) -> String {
    let text = String::from_utf16_lossy(units);
    text.replace("\r\n", " ").replace(['\r', '\n'], " ")
}
