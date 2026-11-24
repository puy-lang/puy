//! The names a unit of the standard library does not provide (requirements §8.6,
//! `docs/spec/stdlib/README.md`): E3022 in place of E0700, naming the unit and what to use
//! instead. The lists are kept by `puy-stdlib`, one file per unit.

use super::Sema;
use super::generics::GenericScope;
use crate::diagnostics::Code;
use crate::source::TextRange;
use crate::syntax::{SyntaxElement, SyntaxKind};

impl Sema<'_, '_> {
    /// E3022 when `name`, found nowhere, is a name a unit of the standard library does not
    /// provide (whether or not that unit is used). When a generic type of the name is visible
    /// (E0761 otherwise: `TList` with `System.Generics.Collections`), only when the unit that
    /// lists it is used. Not when type arguments follow the name (`TList<Integer>` without the
    /// unit of the generic type stays E0700). Returns whether it reported.
    pub(super) fn unavailable_name(&mut self, name: &str, range: TextRange) -> bool {
        let Some(entry) = self.unavailable_entry(name) else {
            return false;
        };
        if self.type_args_follow(range) {
            return false;
        }
        let generic = !self
            .generic_types_named(&name.to_ascii_lowercase(), &GenericScope::Visible)
            .is_empty();
        if generic && !self.unit_is_used(entry.unit) {
            return false;
        }
        self.report_unavailable(entry, range);
        true
    }

    /// E3022 for `name` qualified with the unit `unit` (`System.Contnrs.TList`) when that unit
    /// lists it. Returns whether it reported.
    pub(super) fn unavailable_in_unit(&mut self, unit: &str, name: &str, range: TextRange) -> bool {
        let Some(entry) = puy_stdlib::unavailable(name).find(|u| u.unit.eq_ignore_ascii_case(unit))
        else {
            return false;
        };
        self.report_unavailable(entry, range);
        true
    }

    /// E3022 when `name`, not a member of the class or record `class`, is a member that the unit
    /// declaring that type (or one of its ancestors) lists as not provided
    /// (`TMemoryStream.Memory`, listed as `TMemoryStream.Memory` by `System.Classes`;
    /// `TFile.GetLastWriteTime` by `System.IOUtils`). Returns whether it reported.
    pub(super) fn unavailable_member(
        &mut self,
        class: super::types::TypeId,
        name: &str,
        range: TextRange,
    ) -> bool {
        let mut at = Some(class);
        while let Some(id) = at {
            let unit = self
                .types
                .class_unit(id)
                .filter(|u| *u != self.unit_index)
                .and_then(|u| self.units.get(u as usize))
                .map(|u| u.name.clone());
            if let (Some(unit), Some(cname)) = (unit, self.types.get(id).name.clone()) {
                let full = format!("{cname}.{name}");
                let entry =
                    puy_stdlib::unavailable(&full).find(|u| u.unit.eq_ignore_ascii_case(&unit));
                if let Some(entry) = entry {
                    self.report_unavailable(entry, range);
                    return true;
                }
            }
            at = self.types.parent(id);
        }
        false
    }

    /// E3022 at `range`: `` `<unit>.<name>` is not supported in Puy `` (the name as the list
    /// writes it) with the note `` use … instead `` (`docs/spec/diagnostics.md` 4.7, 4.10).
    fn report_unavailable(&mut self, entry: &puy_stdlib::Unavailable, range: TextRange) {
        let note = format!("use {} instead", entry.instead);
        self.error(
            Code::E(3022),
            format!("`{}.{}` is not supported in Puy", entry.unit, entry.name),
            range,
        )
        .notes
        .push(note);
    }

    /// The entry for `name`: that of a unit used here if there is one, else the first.
    fn unavailable_entry(&self, name: &str) -> Option<&'static puy_stdlib::Unavailable> {
        let mut first = None;
        for u in puy_stdlib::unavailable(name) {
            if self.unit_is_used(u.unit) {
                return Some(u);
            }
            first.get_or_insert(u);
        }
        first
    }

    /// Whether the unit named `unit` is visible here through `uses`.
    fn unit_is_used(&self, unit: &str) -> bool {
        let lower = unit.to_ascii_lowercase();
        self.visible_units().any(|(_, u)| u.lower == lower)
    }

    /// Whether type arguments follow the name at `range` in the source: `TList<Integer>` in a
    /// type (`GENERIC_TYPE_REF`) or in an expression (`TYPE_ARGS` after the name).
    fn type_args_follow(&self, range: TextRange) -> bool {
        let root = self.input.root;
        if !root.text_range().contains_range(range) {
            return false;
        }
        let mut el: SyntaxElement = root.covering_element(range);
        while let Some(parent) = el.parent() {
            let mut next = el.next_sibling_or_token();
            while next.as_ref().is_some_and(|n| n.kind().is_trivia()) {
                next = next.and_then(|n| n.next_sibling_or_token());
            }
            match next.map(|n| n.kind()) {
                Some(SyntaxKind::TYPE_ARGS) => return true,
                Some(SyntaxKind::LT) if parent.kind() == SyntaxKind::GENERIC_TYPE_REF => {
                    return true;
                }
                Some(_) => return false,
                None => {}
            }
            // The name is the last part of the parent (`QUALIFIED_NAME`): look after the parent
            el = parent.into();
        }
        false
    }
}
