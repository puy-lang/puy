//! The bodies of the methods of the standard library's types that the analysis puts off until
//! some analyzed body uses them (`docs/spec/units.md` 6.1, `docs/spec/stdlib/System/README.md`
//! chapter 2).
//!
//! A unit of the standard library declares many methods a program does not use. Their headings
//! are checked where they are declared, as any other's; their bodies wait. When every file is
//! analyzed, a work list analyzes the bodies the analyzed ones refer to (a call, a routine value,
//! a method pointer, a key equality), those of every virtual method table and interface table,
//! and all those of the types an instantiation, a key equality or a generated equality takes as a
//! value (whose bodies the analysis makes later, in `finish`), in the context of their own file,
//! until no new one is referred to. A body none of them refers to is never analyzed and has no
//! function: no entry of the module reaches it (6.1).
//!
//! The user's units and programs are always analyzed whole: whether a method is used does not
//! change their diagnostics.

use std::collections::{HashMap, HashSet};

use super::hir::{KeyOp, RoutineId};
use super::hirwalk::{HirVisit, walk_routine};
use super::types::{IntfEntry, Ty, TypeId, TypeKind, VmtEntry};
use super::*;

/// A method body put off (the arguments of [`Sema::routine_body`]).
pub(super) struct Deferred {
    decl: ast::RoutineDecl,
    id: RoutineId,
    heading: Vec<ParamInfo>,
    ret: Option<Ty>,
    name_text: String,
    name_range: TextRange,
    lower: String,
    self_class: Option<TypeId>,
}

/// What a file's analysis knows that the analysis of its bodies needs: its names and the units
/// it uses (the state of [`Sema`] that is the file's own, as opposed to the module's).
pub(super) struct FileCtx {
    global_scope: Vec<(String, GSym)>,
    sysutils: SysUtils,
    uses_fix: UsesFix,
    script_top_vars: Vec<(String, TextRange)>,
    script_all_top_vars: Vec<(String, TextRange)>,
    is_script: bool,
    anon_self: HashMap<RoutineId, TextRange>,
    site_routines: HashMap<RoutineSite, (u32, RoutineId)>,
    unit_name_span: Option<TextRange>,
    program_name: String,
    lexical_outer: HashMap<TypeId, TypeId>,
    generic_helpers: HashSet<TypeId>,
    overload_picks: HashMap<TextRange, Option<RoutineId>>,
    used_units: Vec<UsedUnit>,
    is_unit: bool,
    sysutils_item: Option<TextRange>,
    generic_types: Vec<generics::GenericTypeName>,
    explicit_sets: HashMap<TextSize, (RoutineId, Vec<RoutineId>)>,
    comma_comparisons: Vec<TextRange>,
}

impl FileCtx {
    /// The state of a file whose analysis starts.
    pub(super) fn new(input: &SemaInput<'_>) -> Self {
        Self {
            global_scope: Vec::new(),
            sysutils: SysUtils::NotUsed,
            uses_fix: UsesFix::compute(input.root, input.bytes),
            script_top_vars: Vec::new(),
            script_all_top_vars: Vec::new(),
            is_script: false,
            anon_self: HashMap::new(),
            site_routines: HashMap::new(),
            unit_name_span: None,
            program_name: String::new(),
            lexical_outer: HashMap::new(),
            generic_helpers: HashSet::new(),
            overload_picks: HashMap::new(),
            used_units: Vec::new(),
            is_unit: false,
            sysutils_item: None,
            generic_types: Vec::new(),
            explicit_sets: HashMap::new(),
            comma_comparisons: Vec::new(),
        }
    }
}

/// A file whose analysis put off some bodies.
pub(super) struct Suspended {
    file: FileId,
    ctx: FileCtx,
    bodies: Vec<Option<Deferred>>,
}

/// The bodies put off by the files analyzed so far.
#[derive(Default)]
pub(super) struct DeferredBodies {
    /// By the unit (`ModuleState::units`) that declares them.
    files: HashMap<u32, Suspended>,
    /// Where each body put off and not yet analyzed is: its unit and its place in the unit's.
    pending: HashMap<RoutineId, (u32, usize)>,
    /// The bodies put off of each type.
    by_type: HashMap<TypeId, Vec<RoutineId>>,
    /// [`analyze_deferred`] has run.
    analyzed: bool,
}

impl DeferredBodies {
    /// Whether the body of `r` was put off and is not analyzed.
    pub(super) fn is_pending(&self, r: RoutineId) -> bool {
        self.pending.contains_key(&r)
    }

    /// Whether the work list has had its turn: `finish` comes after [`analyze_deferred`], or a
    /// body some routine uses is left without a function.
    pub(super) fn ready_to_finish(&self) -> bool {
        self.analyzed || self.pending.is_empty()
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// The analysis of a file: `ctx` is the file's own state, the rest is `state`'s.
    pub(super) fn new(
        input: &'a SemaInput<'a>,
        sink: &'s mut DiagnosticSink,
        state: &mut ModuleState,
        unit_index: u32,
        ctx: FileCtx,
    ) -> Self {
        Sema {
            input,
            sink,
            globals: std::mem::take(&mut state.globals),
            exit_code: state.exit_code,
            replace_flags: state.replace_flags,
            case_mapping: state.case_mapping,
            case_fold: state.case_fold,
            routines: std::mem::take(&mut state.routines),
            global_scope: ctx.global_scope,
            sysutils: ctx.sysutils,
            uses_fix: ctx.uses_fix,
            script_top_vars: ctx.script_top_vars,
            script_all_top_vars: ctx.script_all_top_vars,
            is_script: ctx.is_script,
            cur: None,
            outer: Vec::new(),
            allow_type_as_value: false,
            mismatch_shown: None,
            use_span: None,
            anon_self: ctx.anon_self,
            site_routines: ctx.site_routines,
            plan: std::mem::take(&mut state.plan),
            last_place_result: None,
            dry_run: 0,
            types: std::mem::take(&mut state.types),
            type_decls: std::mem::take(&mut state.type_decls),
            attr_uses: std::mem::take(&mut state.attrs),
            hints: std::mem::take(&mut state.hints),
            hint_state: Default::default(),
            unit_name_span: ctx.unit_name_span,
            program_name: ctx.program_name,
            pending_aliases: Vec::new(),
            pending_classes: Vec::new(),
            incomplete_classes: HashSet::new(),
            pending_impls: Vec::new(),
            attr_checks: Vec::new(),
            user_attrs: Vec::new(),
            test_attrs: Vec::new(),
            tests: Vec::new(),
            attr_defer: 0,
            equality_attrs: Default::default(),
            class_scopes: Vec::new(),
            lexical_outer: ctx.lexical_outer,
            generic_helpers: ctx.generic_helpers,
            overload_picks: ctx.overload_picks,
            unit_index,
            units: std::mem::take(&mut state.units),
            used_units: ctx.used_units,
            in_interface_part: false,
            is_unit: ctx.is_unit,
            sysutils_item: ctx.sysutils_item,
            generic_types: ctx.generic_types,
            generic_params: Vec::new(),
            cycle_check: Default::default(),
            routine_type_params: Vec::new(),
            explicit_sets: ctx.explicit_sets,
            comma_comparisons: ctx.comma_comparisons,
            param_scopes: Vec::new(),
            deferred_checks: None,
            instances: std::mem::take(&mut state.instances),
            typed_consts: std::mem::take(&mut state.typed_consts),
            explicit_consts: std::mem::take(&mut state.explicit_consts),
            key_param: state.key_param,
            system_helpers: state.system_helpers.clone(),
            wanted_helpers: Vec::new(),
            const_elem: false,
            deferred: Vec::new(),
        }
    }

    /// The file's own state, taken out of the analysis (the inverse of [`Sema::new`]).
    fn take_ctx(&mut self) -> FileCtx {
        FileCtx {
            global_scope: std::mem::take(&mut self.global_scope),
            sysutils: self.sysutils.clone(),
            uses_fix: std::mem::replace(&mut self.uses_fix, UsesFix::empty()),
            script_top_vars: std::mem::take(&mut self.script_top_vars),
            script_all_top_vars: std::mem::take(&mut self.script_all_top_vars),
            is_script: self.is_script,
            anon_self: std::mem::take(&mut self.anon_self),
            site_routines: std::mem::take(&mut self.site_routines),
            unit_name_span: self.unit_name_span,
            program_name: std::mem::take(&mut self.program_name),
            lexical_outer: std::mem::take(&mut self.lexical_outer),
            generic_helpers: std::mem::take(&mut self.generic_helpers),
            overload_picks: std::mem::take(&mut self.overload_picks),
            used_units: std::mem::take(&mut self.used_units),
            is_unit: self.is_unit,
            sysutils_item: self.sysutils_item,
            generic_types: std::mem::take(&mut self.generic_types),
            explicit_sets: std::mem::take(&mut self.explicit_sets),
            comma_comparisons: std::mem::take(&mut self.comma_comparisons),
        }
    }

    /// Whether the body of the method `id` of `class`, declared at the top level of this file,
    /// waits until some analyzed body uses it: a method of a type of the standard library that
    /// is not generic (a generic body is checked once and copied into the instantiations that
    /// use it; `docs/spec/generics.md` 3.3).
    pub(super) fn defers_body(&self, id: RoutineId, class: Option<TypeId>) -> bool {
        let Some(c) = class else { return false };
        self.input.stdlib
            && self.cur.is_none()
            && self.routine_type_params.is_empty()
            && self.routines[id.0 as usize].type_params.is_empty()
            && !self.types.is_generic(c)
            && !self.generic_helpers.contains(&c)
            && self.lexical_outer_chain_not_generic(c)
    }

    /// Whether none of the types `c` is nested in is generic.
    fn lexical_outer_chain_not_generic(&self, c: TypeId) -> bool {
        let mut seen = vec![c];
        let mut cur = self.lexical_outer.get(&c).copied();
        while let Some(o) = cur {
            // A cycle cannot be declared; were one there, the body is analyzed in place (the
            // side that needs no work list)
            if seen.contains(&o) || self.types.is_generic(o) {
                return false;
            }
            seen.push(o);
            cur = self.lexical_outer.get(&o).copied();
        }
        true
    }

    /// Puts off the body of a method (see [`Sema::defers_body`]).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn defer_body(
        &mut self,
        decl: &ast::RoutineDecl,
        id: RoutineId,
        heading: Vec<ParamInfo>,
        ret: Option<Ty>,
        name_text: String,
        name_range: TextRange,
        lower: String,
        self_class: Option<TypeId>,
    ) {
        if decl.body().is_none() {
            return;
        }
        self.routines[id.0 as usize].has_body = true;
        self.deferred.push(Deferred {
            decl: decl.clone(),
            id,
            heading,
            ret,
            name_text,
            name_range,
            lower,
            self_class,
        });
    }

    /// Keeps the bodies this file put off, with the file's state, in `state` (at the end of
    /// [`Sema::run`]).
    pub(super) fn suspend(&mut self, state: &mut ModuleState) {
        if self.deferred.is_empty() {
            return;
        }
        let unit = self.unit_index;
        let bodies = std::mem::take(&mut self.deferred);
        let d = &mut state.deferred;
        for (k, b) in bodies.iter().enumerate() {
            d.pending.insert(b.id, (unit, k));
            if let Some(c) = b.self_class {
                d.by_type.entry(c).or_default().push(b.id);
            }
        }
        let ctx = self.take_ctx();
        d.files.insert(
            unit,
            Suspended {
                file: self.input.file,
                ctx,
                bodies: bodies.into_iter().map(Some).collect(),
            },
        );
    }

    /// Analyzes `bodies`, put off by this (resumed) file.
    fn analyze_put_off(&mut self, bodies: Vec<Deferred>) {
        let first_diag = self.sink.len();
        for b in bodies {
            if let Some(span) = self.unit_name_span {
                self.push_hint_scope(self.span(span));
            }
            self.routine_body(
                &b.decl,
                b.id,
                b.heading,
                b.ret,
                b.name_text,
                b.name_range,
                b.lower,
                b.self_class,
            );
            if self.unit_name_span.is_some() {
                self.pop_hint_scope();
            }
        }
        self.report_hint_uses();
        self.label_anon_records(first_diag);
    }
}

/// Analyzes the bodies put off that the analyzed ones use, until none is left (see the module's
/// documentation). `inputs` are those of the analyzed files.
pub fn analyze_deferred(
    inputs: &[SemaInput<'_>],
    state: &mut ModuleState,
    sink: &mut DiagnosticSink,
) {
    state.deferred.analyzed = true;
    if state.deferred.pending.is_empty() {
        return;
    }
    let mut refs = Refs::default();
    // The bodies of the units and the program
    for u in &mut state.parts {
        for r in u.init.iter_mut().chain(u.fini.iter_mut()) {
            walk_routine(r, &mut refs);
        }
    }
    if let Some(m) = &mut state.main {
        walk_routine(m, &mut refs);
    }
    // The routines whose bodies were walked (a body put off is analyzed after routines made later)
    let mut scanned: Vec<bool> = Vec::new();
    let mut scanned_types = 0usize;
    let mut scanned_instances = 0usize;
    let mut whole: HashSet<TypeId> = HashSet::new();
    let mut rtti_seen = super::invoke::RttiWants::default();
    let mut rtti_types = 0usize;
    loop {
        // The routines analyzed since the last round
        scanned.resize(state.routines.len(), false);
        let hirs: Vec<usize> = (0..state.routines.len())
            .filter(|&i| !scanned[i] && state.routines[i].hir.is_some())
            .collect();
        for i in hirs {
            let mut h = state.routines[i].hir.take().expect("collected with a body");
            walk_routine(&mut h, &mut refs);
            state.routines[i].hir = Some(h);
            scanned[i] = true;
        }
        // The tables of the classes, the types of the instantiations, the classes whose
        // equality the analysis generates (`docs/spec/equality.md` 3.3, 3.4)
        for t in scanned_types..state.types.defs.len() {
            let id = TypeId(t as u32);
            if let TypeKind::Class { vmt, intfs, .. } = &state.types.get(id).kind {
                for e in vmt {
                    if let VmtEntry::Func(r) = e {
                        refs.routines.push(*r);
                    }
                }
                for i in intfs {
                    for e in &i.entries {
                        if let IntfEntry::Func(r) = e {
                            refs.routines.push(*r);
                        }
                    }
                }
            }
        }
        scanned_types = state.types.defs.len();
        // The methods `System.Rtti` calls by name and the attributes' constructors
        // (`docs/spec/rtti.md` 1.5), once a body analyzed uses the built-ins that call them
        let wants = state.instances.rtti_wants;
        if wants != rtti_seen {
            rtti_seen = wants;
            rtti_types = 0;
            if wants.attrs {
                refs.routines
                    .extend(state.attrs.iter().filter_map(|a| a.ctor));
            }
        }
        for t in rtti_types..state.types.defs.len() {
            for (_, _, m) in super::invoke::invoked_methods(&state.types, TypeId(t as u32), wants) {
                refs.routines.extend(m.routine);
            }
        }
        rtti_types = state.types.defs.len();
        for inst in &state.types.instances[scanned_instances..] {
            refs.values.extend(inst.args.iter().copied());
        }
        scanned_instances = state.types.instances.len();
        for (c, _, _) in &state.instances.generated_equals {
            refs.values.push(Ty::Class(*c));
        }
        for (c, _, _) in &state.instances.generated_value_eq {
            refs.values.push(Ty::Class(*c));
            for (_, f) in state.types.class_fields(*c) {
                refs.values.push(f.ty);
            }
        }
        for ty in std::mem::take(&mut refs.values) {
            value_types(
                &state.types,
                ty,
                &mut whole,
                &mut refs.routines,
                &state.deferred,
            );
        }
        // The bodies to analyze in this round, by unit and in the order they were put off
        let mut want: Vec<(u32, usize)> = std::mem::take(&mut refs.routines)
            .into_iter()
            .filter_map(|r| state.deferred.pending.remove(&r))
            .collect();
        if want.is_empty() {
            break;
        }
        want.sort_unstable();
        let mut k = 0;
        while k < want.len() {
            let unit = want[k].0;
            let mut bodies = Vec::new();
            while k < want.len() && want[k].0 == unit {
                let s = state
                    .deferred
                    .files
                    .get_mut(&unit)
                    .expect("a body put off is kept with its file");
                bodies.push(s.bodies[want[k].1].take().expect("analyzed once"));
                k += 1;
            }
            let mut s = state
                .deferred
                .files
                .remove(&unit)
                .expect("a body put off is kept with its file");
            let input = inputs
                .iter()
                .find(|i| i.file == s.file)
                .expect("the input of each analyzed file is given");
            let ctx = std::mem::replace(&mut s.ctx, FileCtx::new(input));
            let mut sema = Sema::new(input, sink, state, unit, ctx);
            sema.analyze_put_off(bodies);
            s.ctx = sema.take_ctx();
            sema.put_back(state);
            state.deferred.files.insert(unit, s);
        }
    }
}

/// The types whose bodies put off are all analyzed when a value of `ty` is a type argument or is
/// compared by a key equality or a generated equality: the records, classes (with their
/// ancestors) and interfaces in it, with their fields' (the analysis makes the bodies that use
/// their methods later).
fn value_types(
    types: &super::types::TypeTable,
    ty: Ty,
    seen: &mut HashSet<TypeId>,
    out: &mut Vec<RoutineId>,
    deferred: &DeferredBodies,
) {
    let mut work = vec![ty];
    while let Some(ty) = work.pop() {
        let id = match ty {
            Ty::Record(t) | Ty::Class(t) | Ty::Intf(t) => t,
            Ty::Array(_) | Ty::DynArray(_) | Ty::OpenArray(_) => {
                if let Some(e) = types.elem_of(ty) {
                    work.push(e);
                }
                continue;
            }
            _ => continue,
        };
        if !seen.insert(id) {
            continue;
        }
        // A class's ancestors too: an ancestor's own operators give its default order
        // (`LessThan`; `docs/spec/collections.md` 10.2), and its fields are compared
        let owners = match ty {
            Ty::Class(c) => types.class_chain(c),
            _ => vec![id],
        };
        for o in owners {
            if let Some(rs) = deferred.by_type.get(&o) {
                out.extend(rs.iter().copied());
            }
        }
        if matches!(ty, Ty::Record(_)) {
            work.extend(types.fields(ty).iter().map(|f| f.ty));
        }
    }
}

/// The routines and the types of key equalities the walked bodies refer to.
#[derive(Default)]
struct Refs {
    routines: Vec<RoutineId>,
    values: Vec<Ty>,
}

impl HirVisit for Refs {
    fn ty(&mut self, _t: &mut Ty) {}
    fn routine(&mut self, r: &mut RoutineId) {
        self.routines.push(*r);
    }
    fn key(
        &mut self,
        _op: KeyOp,
        ty: Ty,
        _func: &mut Option<RoutineId>,
        _span: crate::source::TextRange,
    ) {
        self.values.push(ty);
    }
}
