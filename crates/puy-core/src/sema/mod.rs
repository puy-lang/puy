//! Semantic analysis (`docs/spec/semantics.md`): name resolution, type checking, constant folding, definite assignment. Produces HIR.

mod anon;
mod attrs;
pub mod builtins;
mod defaults;
pub mod dump;
mod enumerators;
pub mod erase;
mod expr;
mod flow;
mod generics;
mod helpers;
mod hints;
pub mod hir;
pub(crate) mod hirwalk;
mod infer;
mod instances;
mod interfaces;
mod keys;
mod methods;
mod operators;
mod overload;
mod procs;
mod properties;
mod reals;
mod sets;
mod statics;
mod stdio;
mod stmt;
mod sysutils;
mod typedecl;
pub mod types;
mod values;

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Fix, Stage, TextEdit};
use crate::directives::DirectiveOutput;
use crate::source::{FileId, Span, TextRange, TextSize};
use crate::syntax::ast::{self, AstNode, ParamMode};
use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
pub use expr::{compare_strings, compare_units};
use hir::*;
use types::{Const, Ty, TypeTable};

pub struct SemaInput<'a> {
    pub file: FileId,
    pub root: &'a SyntaxNode,
    pub bytes: &'a [u8],
    pub directives: &'a DirectiveOutput,
    /// Program name of a script (derived from the file name; `semantics.md` 1.1).
    pub script_name: &'a str,
    /// What the driver found for the items of `uses` (`docs/spec/units.md` 2.2, 3.3, 3.4). An item
    /// without an entry is a name of the standard library or a unit that was not found.
    pub uses: &'a [UsesResolution],
    /// A package root (`puy.toml`) was found (`docs/spec/units.md` 2.1; the notes of E2500).
    pub has_root: bool,
    /// The file is a unit of the standard library (`docs/spec/collections.md` 1.2): it also sees
    /// the built-ins only the standard library uses (12.1).
    pub stdlib: bool,
}

/// What an item of `uses` names (keyed by the range of its `QualifiedName`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsesResolution {
    pub range: TextRange,
    pub target: UsesTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsesTarget {
    /// A unit of the package, already analyzed (index into [`ModuleState::units`]).
    Unit(usize),
    /// A unit whose names all resolve to the error type: it could not be read, has no tree,
    /// is not a unit, maps to several files, or closes a cycle. The driver has reported it.
    Broken,
}

/// A unit's public part (`UnitInterface` in `docs/spec/units.md` chapter 10): the names declared
/// in its `interface` section.
#[derive(Debug, Clone)]
pub struct UnitInterface {
    /// The name as written in the unit's heading (or the name from its path).
    pub name: String,
    pub file: FileId,
    /// The name in the unit's heading (the declaration its hint directives are kept under).
    name_span: Option<TextRange>,
    exports: Vec<(String, GSym)>,
    /// The generic types declared in the `interface` section (`docs/spec/generics.md` 2.3).
    generics: Vec<generics::GenericTypeName>,
}

/// State shared by the files of one compilation: the module's globals, routines and types are
/// numbered across all units (`docs/spec/units.md` 6.1, 6.2).
#[derive(Default)]
pub struct ModuleState {
    globals: Vec<Global>,
    exit_code: Option<GlobalId>,
    replace_flags: Option<(Ty, Ty)>,
    case_mapping: Option<Ty>,
    case_fold: Option<Ty>,
    routines: Vec<RoutineInfo>,
    types: TypeTable,
    type_decls: Vec<TypeDeclInfo>,
    attrs: Vec<AttrUse>,
    /// The declarations marked `deprecated` / `experimental` (`hints.rs`).
    hints: hints::HintTable,
    /// The analyzed units, in analysis order (dependencies first).
    pub units: Vec<UnitInterface>,
    /// The `hir` of each analyzed file, in the same order as `units` (the program last).
    parts: Vec<UnitHir>,
    main: Option<Routine>,
    uses_sysutils: bool,
    /// The closure-form nested routines the analysis compiles as closures.
    plan: ClosurePlan,
    /// The routines of the instantiations of generic types.
    instances: instances::InstanceRoutines,
    /// The declarations of the typed constants (`docs/spec/generics.md` 5.2).
    typed_consts: std::collections::HashSet<Span>,
    /// The type parameter of `KeyEquals` / `KeyHash` for inference, shared by the units.
    key_param: Option<types::TypeId>,
}

impl ModuleState {
    pub fn new() -> Self {
        Self::default()
    }

    /// A state whose analysis compiles the nested routines of `plan` as closures.
    pub fn with_plan(plan: ClosurePlan) -> Self {
        Self {
            plan,
            ..Self::default()
        }
    }

    /// The plan the analysis was given.
    pub fn plan(&self) -> &ClosurePlan {
        &self.plan
    }

    /// The nested routines to compile as closures (`docs/spec/objects.md` 9.7), found from the
    /// analyzed files, and their groups (14.9). A nested routine is one when it is made a value,
    /// or when a routine calls it from inside a closure nested in the routine it is nested in
    /// (an anonymous method, or another such nested routine): such a caller cannot reach the
    /// frame of that routine through the static link. A group is a set of these routines nested
    /// in the same routine that call each other, directly or through the routines nested in them
    /// (a strongly connected component of the calls).
    pub fn closure_plan(&self) -> ClosurePlan {
        closure_plan(&self.routines)
    }
}

/// Runs semantic analysis of one file (a unit, a program or a script) and adds it to `state`.
/// Units are analyzed before the files that use them (`docs/design/compiler-pipeline.md` 3.8).
pub fn analyze(input: &SemaInput<'_>, state: &mut ModuleState, sink: &mut DiagnosticSink) {
    let unit_index = state.units.len() as u32;
    let s = Sema {
        input,
        sink,
        globals: std::mem::take(&mut state.globals),
        exit_code: state.exit_code,
        replace_flags: state.replace_flags,
        case_mapping: state.case_mapping,
        case_fold: state.case_fold,
        routines: std::mem::take(&mut state.routines),
        global_scope: Vec::new(),
        sysutils: SysUtils::NotUsed,
        uses_fix: UsesFix::compute(input.root, input.bytes),
        script_top_vars: Vec::new(),
        script_all_top_vars: Vec::new(),
        is_script: false,
        cur: None,
        outer: Vec::new(),
        allow_type_as_value: false,
        mismatch_shown: None,
        use_span: None,
        anon_self: Default::default(),
        plan: std::mem::take(&mut state.plan),
        last_place_result: None,
        dry_run: 0,
        types: std::mem::take(&mut state.types),
        type_decls: std::mem::take(&mut state.type_decls),
        attr_uses: std::mem::take(&mut state.attrs),
        hints: std::mem::take(&mut state.hints),
        hint_state: Default::default(),
        unit_name_span: None,
        program_name: String::new(),
        pending_aliases: Vec::new(),
        pending_classes: Vec::new(),
        incomplete_classes: std::collections::HashSet::new(),
        pending_impls: Vec::new(),
        attr_checks: Vec::new(),
        user_attrs: Vec::new(),
        attr_defer: 0,
        equality_attrs: Default::default(),
        class_scopes: Vec::new(),
        lexical_outer: std::collections::HashMap::new(),
        overload_picks: std::collections::HashMap::new(),
        unit_index,
        units: std::mem::take(&mut state.units),
        used_units: Vec::new(),
        in_interface_part: false,
        is_unit: false,
        sysutils_item: None,
        generic_types: Vec::new(),
        generic_params: Vec::new(),
        routine_type_params: Vec::new(),
        explicit_sets: std::collections::HashMap::new(),
        param_scopes: Vec::new(),
        deferred_checks: None,
        instances: std::mem::take(&mut state.instances),
        typed_consts: std::mem::take(&mut state.typed_consts),
        key_param: state.key_param,
    };
    s.run(state);
}

/// Builds the program from the analyzed files (`analyze` was called for each, the file passed to
/// `puy` last).
pub fn finish(mut state: ModuleState, sink: &mut DiagnosticSink) -> Program {
    // The bodies of the methods of the instantiations that are used (`docs/design/generics.md`
    // 3.1), after every routine they may be used from
    let mut diags = Vec::new();
    {
        let program_unit = state.parts.len().saturating_sub(1) as u32;
        state.instances.unit_files = state.parts.iter().map(|u| u.file).collect();
        let mut i = instances::Inst {
            types: &mut state.types,
            routines: &mut state.routines,
            inst: &mut state.instances,
            globals: &mut state.globals,
            diags: &mut diags,
            unit: program_unit,
            cur_origin: None,
        };
        // Before the work list, which copies a generic type's into its instantiations
        i.generate_equals();
        i.generate_value_equality();
        // The routines of the key equalities and hashes (`docs/spec/equality.md` 8.3)
        let main_file = state.parts.last().map_or(FileId(0), |u| u.file);
        let mut key_roots: Vec<(&mut Routine, u32, FileId)> = Vec::new();
        for (k, u) in state.parts.iter_mut().enumerate() {
            let file = u.file;
            for r in u.init.iter_mut().chain(u.fini.iter_mut()) {
                key_roots.push((r, k as u32, file));
            }
        }
        key_roots.extend(state.main.iter_mut().map(|r| (r, program_unit, main_file)));
        i.fill_all_keys(key_roots);
        let mut roots: Vec<&Routine> = Vec::new();
        for u in &state.parts {
            roots.extend(u.init.iter());
            roots.extend(u.fini.iter());
        }
        roots.extend(state.main.iter());
        i.instantiate_bodies(&roots);
    }
    for d in diags {
        sink.push(d);
    }
    let generic_globals = place_class_vars(&mut state);
    // Overloads are `Name`, `Name#2`, `Name#3` in declaration order within a scope
    // (`semantics.md` 4.7), and a nested routine is named after the routines enclosing it
    // (`Outer.Inner`; 4.9). A routine comes after the routine it is nested in
    // A generic routine is named with its type parameters (`Max<T>`), an instantiation of one
    // after the generic routine's name with the number of its overload (`Max#2<Integer>`;
    // `docs/spec/generics.md` 4.1)
    let mut seen: Vec<(u32, Option<RoutineId>, String)> = Vec::new();
    let mut names: Vec<String> = Vec::with_capacity(state.routines.len());
    let mut bases: Vec<String> = Vec::with_capacity(state.routines.len());
    for (i, r) in state.routines.iter().enumerate() {
        if let Some((base, args)) = state.instances.rinst.get(&RoutineId(i as u32)) {
            let shown: Vec<String> = args.iter().map(|a| state.types.name(*a)).collect();
            let name = format!("{}<{}>", bases[base.0 as usize], shown.join(", "));
            bases.push(name.clone());
            names.push(name);
            continue;
        }
        let key = (r.unit, r.parent, r.name.to_ascii_lowercase());
        let k = seen.iter().filter(|n| **n == key).count();
        seen.push(key);
        let mut name = match r.parent {
            Some(p) => format!("{}.{}", names[p.0 as usize], r.name),
            None => r.name.clone(),
        };
        if k > 0 {
            name = format!("{name}#{}", k + 1);
        }
        bases.push(name.clone());
        if !r.type_params.is_empty() {
            let params: Vec<String> = r
                .type_params
                .iter()
                .map(|p| state.types.get(*p).name.clone().unwrap_or_default())
                .collect();
            name = format!("{name}<{}>", params.join(", "));
        }
        names.push(name);
    }
    let links = static_links(&state.routines);
    // The methods of a generic type and the routines nested in them have no function
    // (`docs/spec/generics.md` 7). A routine comes after the routine it is nested in
    let mut generic: Vec<bool> = Vec::with_capacity(state.routines.len());
    for (i, r) in state.routines.iter().enumerate() {
        let id = RoutineId(i as u32);
        // A method of an instantiation has a function only once its body is made
        let unused = (state.instances.stubs.contains_key(&id)
            && !state.instances.owner.contains_key(&id))
            || (state.instances.rinst.contains_key(&id)
                && !state.instances.rowner.contains_key(&id));
        let g = r.method_of.is_some_and(|c| state.types.is_generic(c))
            || !r.type_params.is_empty()
            || unused
            || r.parent.is_some_and(|p| generic[p.0 as usize]);
        generic.push(g);
    }
    // The full name of each routine of an instantiation (`docs/spec/generics.md` 4.1, 7):
    // `Hello.TBox<Hello.TFoo>.Get`
    let stub_names: Vec<Option<String>> = (0..state.routines.len())
        .map(|i| {
            let id = RoutineId(i as u32);
            let mut at = id;
            loop {
                if state.instances.rinst.contains_key(&at) {
                    return None;
                }
                if let Some((_, inst)) = state.instances.stubs.get(&at) {
                    let def = state.types.get(*inst);
                    let short = def.name.clone().unwrap_or_default();
                    let full = def.full_name.clone().unwrap_or_default();
                    let name = &bases[i];
                    return Some(match name.strip_prefix(&short) {
                        Some(rest) => format!("{full}{rest}"),
                        None => name.clone(),
                    });
                }
                match state.routines[at.0 as usize].parent {
                    Some(p) => at = p,
                    None => return None,
                }
            }
        })
        .collect();
    // `Hello.Max#2<Integer>`, `Hello.TBox<Integer>.Map<string>`
    let mut instance_names: Vec<Option<String>> = Vec::with_capacity(state.routines.len());
    for i in 0..state.routines.len() {
        let id = RoutineId(i as u32);
        let mut at = id;
        let full = loop {
            if let Some((base, args)) = state.instances.rinst.get(&at) {
                let b = base.0 as usize;
                let base_full = match instance_names[b].as_ref().or(stub_names[b].as_ref()) {
                    Some(f) => f.clone(),
                    None => format!(
                        "{}.{}",
                        state.parts[state.routines[b].unit as usize].name, bases[b]
                    ),
                };
                let full_args: Vec<String> = args
                    .iter()
                    .map(|a| state.types.canonical_name(*a))
                    .collect();
                let root = format!("{base_full}<{}>", full_args.join(","));
                break Some(match names[i].strip_prefix(&names[at.0 as usize]) {
                    Some(rest) => format!("{root}{rest}"),
                    None => root,
                });
            }
            if state.instances.stubs.contains_key(&at) {
                break None;
            }
            match state.routines[at.0 as usize].parent {
                Some(p) => at = p,
                None => break None,
            }
        };
        instance_names.push(
            full.or_else(|| stub_names[i].clone())
                .or_else(|| state.instances.generated_names.get(&id).cloned()),
        );
    }
    // Where each copied routine comes from (`docs/spec/generics.md` 7): the generic name is the
    // declared one with the type parameters joined by `,` as in a full name (4.1)
    let mut origins: std::collections::HashMap<RoutineId, hir::GenericOrigin> = Default::default();
    for (&dst, &src) in &state.instances.source {
        let instance = if let Some(t) = state.instances.owner.get(&dst) {
            state.types.get(*t).full_name.clone()
        } else {
            state
                .instances
                .rowner
                .get(&dst)
                .and_then(|x| instance_names[x.0 as usize].clone())
        };
        let Some(instance) = instance else { continue };
        // A method's name starts with its type's short name; the type's full name holds the
        // types it is nested in (`Hello.TStore<T>.TCell.Reset`)
        let mut top = src;
        while let Some(p) = state.routines[top.0 as usize].parent {
            top = p;
        }
        let name = &names[src.0 as usize];
        let class = state.routines[top.0 as usize]
            .method_of
            .map(|c| state.types.get(c))
            .and_then(|d| Some((d.name.as_deref()?, d.full_name.as_deref()?)));
        let generic_name =
            match class.and_then(|(short, full)| Some((name.strip_prefix(short)?, full))) {
                Some((rest, full)) => format!("{full}{rest}"),
                None => format!(
                    "{}.{name}",
                    state.parts[state.routines[src.0 as usize].unit as usize].name
                ),
            };
        origins.insert(
            dst,
            hir::GenericOrigin {
                def: src,
                generic_name: generic_name.replace(", ", ","),
                instance,
            },
        );
    }
    // The class of each destructor (`DtorEnter`; `docs/spec/objects.md` 14.8)
    let dtor_of: Vec<Option<types::TypeId>> = (0..state.routines.len())
        .map(|i| {
            let c = state.routines[i].method_of?;
            state
                .types
                .class_methods(c)
                .iter()
                .any(|m| {
                    m.routine == Some(RoutineId(i as u32))
                        && m.kind == types::MethodKind::Destructor
                })
                .then_some(c)
        })
        .collect();
    // The operators (`docs/spec/objects.md` 15.6)
    let operators: Vec<bool> = (0..state.routines.len())
        .map(|i| {
            state.routines[i].method_of.is_some_and(|c| {
                state.types.class_methods(c).iter().any(|m| {
                    m.routine == Some(RoutineId(i as u32)) && m.kind == types::MethodKind::Operator
                })
            })
        })
        .collect();
    let mut routines: Vec<Routine> = state
        .routines
        .drain(..)
        .zip(names)
        .zip(links)
        .zip(dtor_of)
        .zip(generic)
        .zip(instance_names)
        .map(|(((((r, full), link), dtor_of), generic), instance)| {
            let self_ty = |c| match r.self_kind {
                types::SelfKind::Class => state
                    .types
                    .find_class_ref(c)
                    .expect("a class method's class reference is made with it"),
                _ if state.types.is_record(c) => Ty::Record(c),
                // A helper's method: `Self` is a value of its type (`docs/spec/objects.md` 10.8)
                _ if state.types.is_helper(c) => state.types.helper_target(c).unwrap_or(Ty::Error),
                // An interface's method: `Self` is the interface's value (`docs/spec/objects.md`
                // 7.5)
                _ if state.types.is_interface(c) => Ty::Intf(c),
                _ => Ty::Class(c),
            };
            let self_var = r
                .method_of
                .filter(|_| r.self_kind != types::SelfKind::Static)
                .map(|c| Var {
                    name: "Self".into(),
                    ty: self_ty(c),
                    kind: if state.types.self_by_ref(c) {
                        VarKind::RecordSelf
                    } else {
                        VarKind::SelfParam
                    },
                    span: r.name_span,
                    decl: None,
                    is_for_var: false,
                    is_loop_var: false,
                    captured: false,
                    read_by_nested: false,
                    written_by_nested: None,
                    written: false,
                    cell: false,
                });
            let mut routine = r.hir.unwrap_or_else(|| Routine {
                name: r.name.clone(),
                vars: self_var
                    .into_iter()
                    .chain(r.params.iter().map(|p| Var {
                        name: p.name.clone(),
                        ty: p.ty,
                        kind: VarKind::Param(p.mode),
                        span: p.span,
                        decl: None,
                        is_for_var: false,
                        is_loop_var: false,
                        captured: false,
                        read_by_nested: false,
                        written_by_nested: None,
                        written: false,
                        cell: false,
                    }))
                    .collect(),
                param_count: r.params.len() as u32
                    + u32::from(r.method_of.is_some() && r.self_kind != types::SelfKind::Static),
                ret: r.ret,
                body: Vec::new(),
                end_span: r.name_span,
                body_begin: None,
                name_span: r.name_span,
                has_syntax_errors: false,
                // An abstract method has no body and no function: not an error
                has_errors: !r.is_abstract,
                unit: r.unit,
                parent: r.parent,
                link: false,
                is_abstract: false,
                builtin: false,
                generic: false,
                dtor_of: None,
                helper: false,
                operator: false,
                closure: false,
                captures: Vec::new(),
                groups: Default::default(),
                instance: None,
            });
            routine.name = full;
            routine.instance = instance;
            routine.is_abstract = r.is_abstract;
            routine.builtin = r.method_of == Some(types::TOBJECT);
            routine.generic = generic;
            routine.dtor_of = dtor_of;
            routine.helper = r.method_of.is_some_and(|c| state.types.is_helper(c));
            routine.parent = r.parent;
            routine.link = link && !r.closure;
            routine.closure = r.closure;
            routine
        })
        .collect();
    for (r, op) in routines.iter_mut().zip(operators) {
        r.operator = op;
    }
    // The instantiations without type parameters, with their routines made, in the order they
    // were asked for (`docs/spec/generics.md` 8)
    let mut instances: Vec<hir::InstanceInfo> = Vec::new();
    for a in &state.instances.asked {
        match *a {
            instances::Asked::Type(id) => {
                let Some(i) = state.types.instance_of(id) else {
                    continue;
                };
                if i.args.iter().any(|a| state.types.has_param(*a)) {
                    continue;
                }
                instances.push(hir::InstanceInfo {
                    name: state.types.get(i.id).name.clone().unwrap_or_default(),
                    unit: i.unit,
                    routines: state
                        .instances
                        .order
                        .iter()
                        .filter(|r| state.instances.owner.get(r) == Some(&i.id))
                        .copied()
                        .collect(),
                });
            }
            instances::Asked::Routine(id) => {
                if !state.instances.rowner.contains_key(&id) {
                    continue;
                }
                instances.push(hir::InstanceInfo {
                    name: routines[id.0 as usize].name.clone(),
                    unit: state.instances.runit.get(&id).copied().unwrap_or(0),
                    routines: state
                        .instances
                        .order
                        .iter()
                        .filter(|r| state.instances.rowner.get(r) == Some(&id))
                        .copied()
                        .collect(),
                });
            }
        }
    }
    let mut units = std::mem::take(&mut state.parts);
    let last = units.len().saturating_sub(1);
    let (name, init) = match units.last_mut() {
        Some(u) => (
            u.name.clone(),
            if u.is_program { u.init.take() } else { None },
        ),
        None => (String::new(), None),
    };
    let main = state.main.take().unwrap_or_else(|| Routine {
        name: "<main>".into(),
        vars: Vec::new(),
        param_count: 0,
        ret: None,
        body: Vec::new(),
        end_span: TextRange::default(),
        body_begin: None,
        name_span: TextRange::default(),
        has_syntax_errors: true,
        has_errors: false,
        unit: last as u32,
        parent: None,
        link: false,
        is_abstract: false,
        builtin: false,
        generic: false,
        dtor_of: None,
        helper: false,
        operator: false,
        closure: false,
        captures: Vec::new(),
        groups: Default::default(),
        instance: None,
    });
    Program {
        name,
        globals: state.globals,
        generic_globals,
        exit_code: state.exit_code,
        routines,
        main,
        init,
        uses_sysutils: state.uses_sysutils,
        types: state.types,
        type_decls: state.type_decls,
        attrs: state.attrs,
        hints: hints::marks(&state.hints),
        units,
        instances,
        instance_order: state.instances.order,
        generated: state.instances.generated,
        origins,
    }
}

/// Places the `class var`s of the instantiations at the end of the global variables of the unit
/// that asked for each instantiation first (`docs/spec/generics.md` 2.5, `docs/design/generics.md`
/// 3.2), and moves the `class var`s of the generic types (and of the instantiations with type
/// parameters) out of the global variables, after them: only generic bodies use those. Returns
/// the moved ones; the global variables, the units' ranges and every reference are renumbered.
fn place_class_vars(state: &mut ModuleState) -> Vec<Global> {
    let inst = &state.instances;
    let types = &state.types;
    let is_generic = |owner: types::TypeId| {
        types.is_generic(owner)
            || types
                .instance_of(owner)
                .is_some_and(|i| i.args.iter().any(|a| types.has_param(*a)))
    };
    let generic: std::collections::HashSet<GlobalId> = inst
        .global_owner
        .iter()
        .filter(|(_, (owner, _))| is_generic(*owner))
        .map(|(g, _)| *g)
        .collect();
    if generic.is_empty() && inst.inst_globals.is_empty() {
        return Vec::new();
    }
    let placed: std::collections::HashSet<GlobalId> =
        inst.inst_globals.iter().map(|(g, _)| *g).collect();
    let mut order: Vec<GlobalId> = Vec::with_capacity(state.globals.len());
    for (k, part) in state.parts.iter_mut().enumerate() {
        let first = order.len() as u32;
        for g in part.first_global..part.first_global + part.global_count {
            let g = GlobalId(g);
            if !generic.contains(&g) && !placed.contains(&g) {
                order.push(g);
            }
        }
        for (g, unit) in &inst.inst_globals {
            if *unit as usize == k && !generic.contains(g) {
                order.push(*g);
            }
        }
        part.first_global = first;
        part.global_count = order.len() as u32 - first;
    }
    let mut moved: Vec<GlobalId> = generic.into_iter().collect();
    moved.sort_by_key(|g| g.0);
    let mut map: Vec<u32> = vec![u32::MAX; state.globals.len()];
    for (i, g) in order.iter().chain(moved.iter()).enumerate() {
        map[g.0 as usize] = i as u32;
    }
    debug_assert!(
        map.iter().all(|m| *m != u32::MAX),
        "every global variable is in a unit's range or moved"
    );
    let old = std::mem::take(&mut state.globals);
    state.globals = order.iter().map(|g| old[g.0 as usize].clone()).collect();
    let generic_globals: Vec<Global> = moved.iter().map(|g| old[g.0 as usize].clone()).collect();
    let remap = |g: GlobalId| GlobalId(map[g.0 as usize]);
    struct Remap<'m>(&'m dyn Fn(GlobalId) -> GlobalId);
    impl hirwalk::HirVisit for Remap<'_> {
        fn ty(&mut self, _t: &mut Ty) {}
        fn global(&mut self, g: &mut GlobalId) {
            *g = (self.0)(*g);
        }
    }
    let mut v = Remap(&remap);
    for r in state.routines.iter_mut().filter_map(|r| r.hir.as_mut()) {
        hirwalk::walk_routine(r, &mut v);
    }
    for part in &mut state.parts {
        for r in [&mut part.init, &mut part.fini].into_iter().flatten() {
            hirwalk::walk_routine(r, &mut v);
        }
    }
    if let Some(m) = &mut state.main {
        hirwalk::walk_routine(m, &mut v);
    }
    state.exit_code = state.exit_code.map(remap);
    state.types.remap_globals(&remap);
    generic_globals
}

/// Which routines take the static link (`docs/spec/semantics.md` 4.9): a nested routine that
/// reaches a frame outside its own. `reach[r]` is how many routines out `r` has to go: to a
/// variable it uses, to the routine a callee taking the link is nested in, and one less than a
/// nested routine of its own goes. Computed until nothing changes (calls can be mutual).
fn static_links(routines: &[RoutineInfo]) -> Vec<bool> {
    let n = routines.len();
    let parent = |r: usize| routines[r].parent.map(|p| p.0 as usize);
    let mut reach: Vec<u32> = routines
        .iter()
        .map(|r| {
            // An anonymous method takes the closure instead of the static link
            if r.closure {
                return 0;
            }
            r.hir.as_ref().map_or(0, |h| {
                h.vars
                    .iter()
                    .filter_map(|v| match v.kind {
                        VarKind::Outer { up, .. } => Some(up),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0)
            })
        })
        .collect();
    loop {
        let mut changed = false;
        for r in 0..n {
            if parent(r).is_none() || routines[r].closure {
                continue;
            }
            let mut need = reach[r];
            for c in &routines[r].calls {
                let c = c.0 as usize;
                if reach[c] > 0
                    && let Some(p) = routines[c].parent
                {
                    let d = nesting_distance(routines, RoutineId(r as u32), p);
                    need = need.max(d.expect("a callee is declared in the caller or around it"));
                }
            }
            for (ch, k) in reach.iter().enumerate() {
                if parent(ch) == Some(r) && !routines[ch].closure {
                    need = need.max(k.saturating_sub(1));
                }
            }
            if need > reach[r] {
                reach[r] = need;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..n)
        .map(|r| parent(r).is_some() && reach[r] > 0)
        .collect()
}

/// [`ModuleState::closure_plan`].
fn closure_plan(routines: &[RoutineInfo]) -> ClosurePlan {
    let n = routines.len();
    let parent = |r: usize| routines[r].parent.map(|p| p.0 as usize);
    let nested = |r: usize| parent(r).is_some() && !routines[r].closure_info.anon;
    let mut form: Vec<bool> = (0..n)
        .map(|r| nested(r) && routines[r].closure_info.taken)
        .collect();
    // A routine called from a closure nested in its enclosing routine (until nothing changes:
    // a closure-form routine is such a closure for the routines it calls)
    loop {
        let mut changed = false;
        for x in 0..n {
            for c in &routines[x].calls {
                let r = c.0 as usize;
                if !nested(r) || form[r] {
                    continue;
                }
                let mut at = Some(x);
                while let Some(y) = at.filter(|y| Some(*y) != parent(r)) {
                    if routines[y].closure_info.anon || form[y] {
                        form[r] = true;
                        changed = true;
                        break;
                    }
                    at = parent(y);
                }
            }
        }
        if !changed {
            break;
        }
    }
    // `calls[r]`: the closure-form routines of the same enclosing routine that `r` or a routine
    // nested in it calls
    let mut calls: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (x, info) in routines.iter().enumerate() {
        for c in &info.calls {
            let m = c.0 as usize;
            if !form[m] {
                continue;
            }
            let mut at = Some(x);
            while let Some(y) = at {
                if form[y] && parent(y) == parent(m) && !calls[y].contains(&m) {
                    calls[y].push(m);
                }
                at = parent(y);
            }
        }
    }
    let reaches = |from: usize| {
        let mut seen = vec![from];
        let mut k = 0;
        while k < seen.len() {
            for &m in &calls[seen[k]] {
                if !seen.contains(&m) {
                    seen.push(m);
                }
            }
            k += 1;
        }
        seen
    };
    let reach: Vec<Vec<usize>> = (0..n)
        .map(|r| if form[r] { reaches(r) } else { Vec::new() })
        .collect();
    let mut group: Vec<Option<u32>> = vec![None; n];
    let mut next = 0;
    for r in 0..n {
        if !form[r] || group[r].is_some() {
            continue;
        }
        for &m in &reach[r] {
            if reach[m].contains(&r) {
                group[m] = Some(next);
            }
        }
        next += 1;
    }
    (0..n)
        .filter_map(|r| {
            let g = group[r]?;
            Some(((routines[r].file, routines[r].name_span), g))
        })
        .collect()
}

/// How many routines out `anc` is from `r` (`Some(0)` when it is `r`), if it encloses `r`.
fn nesting_distance(routines: &[RoutineInfo], r: RoutineId, anc: RoutineId) -> Option<u32> {
    let mut d = 0;
    let mut at = Some(r);
    while let Some(x) = at {
        if x == anc {
            return Some(d);
        }
        d += 1;
        at = routines[x.0 as usize].parent;
    }
    None
}

/// The files a unit name is looked up as (`docs/spec/units.md` 2.2), for the notes of E2500:
/// every way of turning the dots between the parts into folders.
fn unit_lookup_paths(name: &str) -> String {
    let parts: Vec<&str> = name.split('.').collect();
    let seps = parts.len() - 1;
    let paths: Vec<String> = (0..1u64 << seps)
        .map(|k| {
            let mut p = String::from("src/");
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    p.push(if k >> (i - 1) & 1 == 1 { '/' } else { '.' });
                }
                p.push_str(part);
            }
            format!("`{p}.pas`")
        })
        .collect();
    match paths.as_slice() {
        [one] => one.clone(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
        [] => String::new(),
    }
}

/// The names of a chain `A.B.C` of `NAME_REF` / `FIELD_EXPR` (with their positions), or `None`
/// if it contains anything else.
pub(super) fn name_chain(e: &ast::Expr) -> Option<Vec<(String, TextRange)>> {
    match e {
        ast::Expr::NameRef(n) => {
            if n.is_string_keyword() {
                return None;
            }
            if n.type_args().is_some() {
                return None;
            }
            let tok = n.token()?;
            Some(vec![(tok.text().to_string(), tok.text_range())])
        }
        ast::Expr::Field(f) => {
            if f.type_args().is_some() {
                return None;
            }
            let mut v = name_chain(&f.base()?)?;
            let field = f.field()?;
            v.push((field.text().to_string(), field.text_range()));
            Some(v)
        }
        _ => None,
    }
}

/// The result of analyzing one file.
struct FilePart {
    name: String,
    /// `<main>` (a program or a script).
    main: Option<Routine>,
    init: Option<Routine>,
    /// `<fini>` (a unit with `finalization`).
    fini: Option<Routine>,
    /// The public part (a unit).
    exports: Vec<(String, GSym)>,
}

/// A unit visible from the file being analyzed.
#[derive(Debug, Clone)]
struct UsedUnit {
    /// The full name, lowercase.
    lower: String,
    /// The alias (`uses … as M`), lowercase.
    alias: Option<String>,
    /// The name or alias to qualify with in fixes, as written.
    qualifier: String,
    /// Index into `units`; `None` for a unit whose names resolve to the error type.
    iface: Option<usize>,
    /// Named in the `uses` of the `interface` section (visible from it).
    in_interface: bool,
    /// The item in `uses`.
    item: TextRange,
    /// The part of `System.SysUtils` written in Pascal (`docs/spec/collections.md` 1.1): its
    /// names are found like those of a package unit, but the unit name (and its alias) names the
    /// built-in unit, which falls back to this part ([`UnitRef::SysUtils`]).
    sysutils: bool,
}

/// State of `uses System.SysUtils`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SysUtils {
    NotUsed,
    Used,
    Alias(String),
}

/// Insertion points for the E0700 fix that adds a `uses` (chapter 5; per section in a unit,
/// `docs/spec/units.md` 3.1).
struct UsesFix {
    /// The program or script, or the `interface` section of a unit.
    main: UsesSite,
    /// The `implementation` section of a unit.
    implementation: Option<UsesSite>,
}

#[derive(Default)]
struct UsesSite {
    /// Position of the `;` of the first `uses` (or the end of the last item if the `;` was supplied by recovery).
    uses_semicolon: Option<TextSize>,
    /// Insertion point when there is no `uses`, and whether the clause goes on a new line after
    /// it (`\nuses …;`) rather than before the following text (`uses …;\n`).
    insert: Option<(TextSize, bool)>,
}

impl UsesSite {
    /// A container's leading `uses` (its first child node other than `skip`).
    fn leading_uses(container: &SyntaxNode, skip: SyntaxKind) -> Option<TextSize> {
        use SyntaxKind::*;
        let first = container.children().find(|n| n.kind() != skip)?;
        if first.kind() != USES_CLAUSE {
            return None;
        }
        let semi = first
            .last_token()
            .filter(|t| t.kind() == SEMICOLON)
            .map(|t| t.text_range().start());
        Some(semi.unwrap_or_else(|| {
            first
                .children()
                .filter(|c| c.kind() == USES_ITEM)
                .last()
                .map(|i| i.text_range().end())
                .unwrap_or(first.text_range().end())
        }))
    }

    /// A section of a unit: after the section's reserved word.
    fn section(sec: &SyntaxNode) -> Self {
        if let Some(pos) = Self::leading_uses(sec, SyntaxKind::PROGRAM_HEADER) {
            return UsesSite {
                uses_semicolon: Some(pos),
                insert: None,
            };
        }
        let pos = sec
            .first_token()
            .map(|t| t.text_range().end())
            .unwrap_or(sec.text_range().start());
        UsesSite {
            uses_semicolon: None,
            insert: Some((pos, true)),
        }
    }
}

impl UsesFix {
    fn compute(root: &SyntaxNode, bytes: &[u8]) -> Self {
        use SyntaxKind::*;
        if let Some(unit) = root.children().find(|n| n.kind() == UNIT) {
            let sec = |k| unit.children().find(|n| n.kind() == k);
            return UsesFix {
                main: sec(INTERFACE_SECTION)
                    .map(|s| UsesSite::section(&s))
                    .unwrap_or_default(),
                implementation: sec(IMPLEMENTATION_SECTION).map(|s| UsesSite::section(&s)),
            };
        }
        let Some(container) = root
            .children()
            .find(|n| matches!(n.kind(), PROGRAM | SCRIPT))
        else {
            return UsesFix {
                main: UsesSite::default(),
                implementation: None,
            };
        };
        if let Some(pos) = UsesSite::leading_uses(&container, PROGRAM_HEADER) {
            return UsesFix {
                main: UsesSite {
                    uses_semicolon: Some(pos),
                    insert: None,
                },
                implementation: None,
            };
        }
        let insert = if container.kind() == PROGRAM {
            let header = container.children().find(|n| n.kind() == PROGRAM_HEADER);
            let pos = header
                .as_ref()
                .map(|h| {
                    // If the `;` was supplied by recovery, the end of the name
                    h.last_token()
                        .filter(|t| t.kind() == SEMICOLON)
                        .map(|t| t.text_range().end())
                        .unwrap_or(h.text_range().end())
                })
                .unwrap_or(container.text_range().start());
            (pos, true)
        } else {
            // Script: the start of the file (after the BOM, shebang, inline metadata and their newlines)
            let mut pos = 0usize;
            let mut i = 0usize;
            let tokens: Vec<_> = root
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .collect();
            // Also skip the newline right after each of the BOM, shebang and inline metadata
            while i < tokens.len() {
                let t = &tokens[i];
                let is_header = match t.kind() {
                    BOM | SHEBANG => true,
                    BLOCK_COMMENT => {
                        bytes[usize::from(t.text_range().start())..].starts_with(b"(*! puy")
                    }
                    _ => false,
                };
                if !is_header {
                    break;
                }
                pos = usize::from(t.text_range().end());
                i += 1;
                if let Some(n) = tokens.get(i)
                    && n.kind() == NEWLINE
                {
                    pos = usize::from(n.text_range().end());
                    i += 1;
                }
            }
            ((pos as u32).into(), false)
        };
        UsesFix {
            main: UsesSite {
                uses_semicolon: None,
                insert: Some(insert),
            },
            implementation: None,
        }
    }
}

/// An entry in the global scope.
#[derive(Debug, Clone)]
enum GSym {
    Global(GlobalId),
    Const {
        value: Const,
        ty: Ty,
        span: TextRange,
    },
    Routine(RoutineId),
    /// A declared type (`docs/spec/types.md` 1.1).
    Type {
        ty: Ty,
        span: TextRange,
    },
}

/// An entry in a routine scope.
#[derive(Debug, Clone)]
enum LSym {
    Var(VarId),
    Const {
        value: Const,
        ty: Ty,
        span: TextRange,
    },
    /// A nested routine (`docs/spec/semantics.md` 4.9).
    Routine(RoutineId),
    /// A local type (`docs/spec/types.md` 1.1).
    Type {
        ty: Ty,
        span: TextRange,
    },
}

/// Information about a declared routine.
#[derive(Clone)]
struct RoutineInfo {
    name: String,
    params: Vec<ParamInfo>,
    ret: Option<Ty>,
    /// Has a body (`false` if only `forward`).
    has_body: bool,
    /// Position of the name in the forward declaration. A body-less declaration without `forward` (for which the parser
    /// reported E0411) is also treated as a forward declaration and tied to a later body (so E0701 does not cascade).
    forward_span: Option<TextRange>,
    /// A declaration with neither `forward` nor a body (E0411), or a method declaration with an
    /// error. Even without a body, E0723 is not reported on top of it.
    implicit_forward: bool,
    name_span: TextRange,
    hir: Option<Routine>,
    /// Declared with `overload` (`docs/spec/semantics.md` 4.7).
    overload: bool,
    /// Where ` overload;` goes (just after the `;` of the heading) when the declaration has no
    /// `overload` (the fix of E0701).
    overload_fix_at: Option<TextSize>,
    /// The unit declaring it (index into `ModuleState::parts`) and its file.
    unit: u32,
    file: FileId,
    /// Declared by a heading in the `interface` section (`docs/spec/units.md` 4.2).
    interface: bool,
    /// The routine it is nested in (`docs/spec/semantics.md` 4.9).
    parent: Option<RoutineId>,
    /// The routines its body calls (for the static link; `docs/spec/semantics.md` 4.9).
    calls: Vec<RoutineId>,
    /// The class of a method (`docs/spec/objects.md` 2.3): the routine takes `Self` first,
    /// unless it is a static method.
    method_of: Option<types::TypeId>,
    /// What `Self` of a method is (`Instance` for a routine that is not a method).
    self_kind: types::SelfKind,
    /// An abstract method (no body, no function).
    is_abstract: bool,
    /// The name in the implementation heading of a method's body.
    body_span: Option<TextRange>,
    /// An anonymous method (`docs/spec/objects.md` 9.6), or a closure-form nested routine (9.7):
    /// its function takes the closure instead of the static link.
    closure: bool,
    /// The type parameters of a generic routine or method (`docs/spec/generics.md` 1.2, 2.2):
    /// its own, not those of its class.
    type_params: Vec<types::TypeId>,
    closure_info: ClosureInfo,
}

/// What decides and describes the closure form of a routine (`docs/spec/objects.md` 9.7, 14.9).
#[derive(Debug, Clone, Default)]
struct ClosureInfo {
    /// An anonymous method.
    anon: bool,
    /// A nested routine made a value of an anonymous method type (it is compiled as a closure).
    taken: bool,
    /// A closure-form nested routine: its group (`ClosurePlan`).
    group: Option<u32>,
    /// A closure (an anonymous method or a closure-form nested routine): for each of its
    /// captured variables, the variable of the routine around it the cell comes from.
    srcs: Vec<VarId>,
}

/// The nested routines compiled as closures (`docs/spec/objects.md` 9.7), by file and the position
/// of the name in the heading, with their groups (14.9). Found by [`ModuleState::closure_plan`]
/// after the files are analyzed; the analysis is run again with it when it differs from the one
/// the analysis was given, as whether a nested routine is a closure is known only after the
/// routine it is nested in is analyzed.
pub type ClosurePlan = std::collections::HashMap<(FileId, TextRange), u32>;

impl RoutineInfo {
    /// The fewest arguments a call can pass: the parameters up to the last one without a default
    /// value (`docs/spec/semantics.md` 4.8).
    fn min_args(&self) -> usize {
        self.params
            .iter()
            .rposition(|p| p.default.is_none())
            .map_or(0, |i| i + 1)
    }
}

#[derive(Debug, Clone)]
struct ParamInfo {
    name: String,
    ty: Ty,
    mode: ParamMode,
    span: TextRange,
    /// The default value converted to the parameter type (`docs/spec/semantics.md` 4.8).
    default: Option<Expr>,
    /// The attributes written on the parameter, as compared between a declaration and its body
    /// (`docs/spec/attributes.md` 1.2): their text without spaces, in lowercase.
    attrs: Option<String>,
}

/// Context of the routine being analyzed.
struct RoutineCtx {
    vars: Vec<Var>,
    scopes: Vec<Vec<(String, LSym)>>,
    loop_depth: u32,
    /// Number of `except` blocks lexically around the statement (`raise;` and
    /// `RaiseOuterException` need one; `docs/spec/exceptions.md` 5.1, 5.4).
    except_depth: u32,
    /// Inside a `finally` block: `loop_depth` at its start (`Exit`, and `Break` / `Continue` of a loop
    /// outside it, are E0731; `docs/spec/exceptions.md` 5.3).
    finally_loop_depth: Option<u32>,
    /// Loop variables of the `for` loops currently active (assigning to them is E0707).
    loop_vars: Vec<VarId>,
    /// `for` loops using a global variable as the loop variable (only in `<main>`).
    loop_globals: Vec<GlobalId>,
    routine: Option<RoutineId>,
    ret: Option<Ty>,
    is_main: bool,
    /// The class of a method being analyzed (`Self` is its first variable, unless the method is
    /// static).
    self_class: Option<types::TypeId>,
    /// What `Self` of the method being analyzed is.
    self_kind: types::SelfKind,
    has_errors: bool,
    has_syntax_errors: bool,
    /// Name (lowercase; for detecting recursive calls).
    lower: String,
    display_name: String,
    /// The name with the enclosing routines (`Outer.Inner`; `docs/spec/semantics.md` 4.9).
    qual_name: String,
    /// The routines the body calls.
    calls: Vec<RoutineId>,
    name_span: TextRange,
    body_begin: Option<TextSize>,
    /// Variables read (with positions; also records uses inside error-typed expressions and discarded statements, for W0001 / E2000).
    used_vars: Vec<(VarId, TextRange)>,
    /// `for var` variables whose scope has ended (name and declaration position; for E0700 `notes`).
    ended_for_vars: Vec<(String, TextRange)>,
    /// An anonymous method (`docs/spec/objects.md` 9.6): the variables of enclosing routines it
    /// uses are captured rather than reached through the static link.
    closure: bool,
    /// The variables a closure captures: (its own variable, the variable of the enclosing
    /// routine the cell comes from), in capture order.
    captures: Vec<(VarId, VarId)>,
    /// The anonymous methods written in the body so far (their names are `<anonN>`).
    anon_count: u32,
    /// For a routine without an id (`<main>`, `<init>`, `<fini>`), the anonymous methods
    /// written directly in its body: the routines nested in it, for the effects of
    /// `docs/spec/semantics.md` 4.9 (a routine with an id finds them by `RoutineInfo::parent`).
    top_anons: Vec<RoutineId>,
    /// Where `Self` was first captured, for an anonymous method (W0024).
    self_captured_at: Option<TextRange>,
    /// The variables (how many routines out, id) an anonymous method was reported for not being
    /// able to capture (E0751, once each).
    uncapturable: Vec<(u32, VarId)>,
    /// The hidden variables holding the group blocks of the closure-form routines nested in this
    /// one (group, variable; `docs/spec/objects.md` 14.9), made on first use.
    group_vars: Vec<(u32, VarId)>,
    /// A closure-form nested routine: the hidden variable copying its own block, made on first
    /// use by a routine nested in it.
    own_block: Option<VarId>,
    /// The closure-form routines nested in this one.
    members: Vec<RoutineId>,
}

struct Sema<'a, 's> {
    input: &'a SemaInput<'a>,
    sink: &'s mut DiagnosticSink,
    globals: Vec<Global>,
    /// The `ExitCode` global, once used.
    exit_code: Option<GlobalId>,
    /// `TReplaceFlag` and `TReplaceFlags`, once used.
    replace_flags: Option<(Ty, Ty)>,
    /// `TCaseMapping` and `TCaseFold`, once used.
    case_mapping: Option<Ty>,
    case_fold: Option<Ty>,
    routines: Vec<RoutineInfo>,
    global_scope: Vec<(String, GSym)>,
    sysutils: SysUtils,
    uses_fix: UsesFix,
    /// Variables at the script's top level (name and declaration position; for E0700 `notes`).
    script_top_vars: Vec<(String, TextRange)>,
    /// All variables at the script's top level (regardless of declaration position; for E0700 notes inside routines).
    script_all_top_vars: Vec<(String, TextRange)>,
    is_script: bool,
    cur: Option<RoutineCtx>,
    /// The routines enclosing `cur` while a nested routine is analyzed, outermost first
    /// (`docs/spec/semantics.md` 4.9).
    outer: Vec<RoutineCtx>,
    /// Do not diagnose a type name written as a value (inside the name arguments of E3200).
    allow_type_as_value: bool,
    /// The types a type mismatch (E0702) of an assignment names instead of the ones compared:
    /// a distinct type is compared as its root, but named as written (`docs/spec/types.md` 1.3).
    mismatch_shown: Option<(Ty, Ty)>,
    /// The position of the name being resolved: where E0751 is reported when a variable that
    /// cannot be captured is used in an anonymous method.
    use_span: Option<TextRange>,
    /// Where each anonymous method that captured `Self` first used it (W0024).
    anon_self: std::collections::HashMap<RoutineId, TextRange>,
    /// The nested routines compiled as closures ([`ModuleState::with_plan`]).
    plan: ClosurePlan,
    /// Set when `lower_place` finds a non-variable for `RefArg` (the caller reports E0706).
    last_place_result: Option<expr::PlaceResult>,
    /// Inside a trial type check for a fix (diagnostics are discarded; no fixes are built).
    dry_run: u32,
    /// Records and static arrays (`docs/spec/types.md` 1.2).
    types: TypeTable,
    /// Declared types for the `hir` dump.
    type_decls: Vec<TypeDeclInfo>,
    /// The program name (the prefix of the fully qualified type names).
    program_name: String,
    /// Aliases of the `type` section being declared that are not resolved yet (forward references
    /// inside the section; `docs/spec/types.md` 1.1).
    pending_aliases: Vec<typedecl::PendingAlias>,
    /// The classes of the `type` sections being declared whose fields are not numbered yet: a
    /// class cannot inherit from one (`docs/spec/objects.md` chapter 1).
    incomplete_classes: std::collections::HashSet<types::TypeId>,
    /// The classes of the `type` section being declared, with their method declarations: their
    /// methods and virtual method tables are made after every type of the section is known,
    /// the parent first (`docs/spec/objects.md` 2.3, 2.4).
    /// The language attributes read whose checks wait for the types (`attrs.rs`).
    attr_checks: Vec<attrs::AttrCheck>,
    /// The attribute classes read whose checks wait for the types and their constructors
    /// (`attrs.rs`).
    user_attrs: Vec<attrs::UserAttr>,
    /// Above 0 while a `type` section is declared: the checks of `user_attrs` wait for its
    /// classes' methods.
    attr_defer: u32,
    /// The equality attributes read (`docs/spec/equality.md` 3.4), checked with the classes'
    /// methods (`keys.rs`).
    equality_attrs: attrs::EqualityAttrs,
    /// The attribute classes written on declarations (`hir::Program::attrs`).
    attr_uses: Vec<AttrUse>,
    /// The declarations marked `deprecated` / `experimental`, of this file and those before it.
    hints: hints::HintTable,
    /// The uses of names recorded for W0011 / W0012 (`hints.rs`).
    hint_state: hints::HintState,
    /// The name in the heading of the unit being analyzed.
    unit_name_span: Option<TextRange>,
    #[allow(clippy::type_complexity)]
    pending_classes: Vec<(
        types::TypeId,
        Vec<(ast::MethodDecl, types::Visibility)>,
        Vec<(ast::PropertyDecl, types::Visibility)>,
    )>,
    /// The interfaces the classes of the `type` section list, and their method resolution
    /// clauses: their interface tables are made after every method of the section is known
    /// (`docs/spec/objects.md` 7.3).
    pending_impls: Vec<interfaces::PendingImpl>,
    /// The classes whose bodies are being declared, innermost last (nested types;
    /// `docs/spec/objects.md` 2.2).
    class_scopes: Vec<statics::ClassScope>,
    /// The class a nested class is declared in (its members see the enclosing class's).
    lexical_outer: std::collections::HashMap<types::TypeId, types::TypeId>,
    /// Resolved overloaded calls by the range of the call (`Some`), and calls that failed to
    /// resolve while trying candidates (`None`, only kept in a dry run). Resolution types each
    /// argument against every candidate, so without this a nested overloaded call would be
    /// resolved again for every candidate of each enclosing call (exponential in the nesting).
    overload_picks: std::collections::HashMap<TextRange, Option<RoutineId>>,
    /// Index of this file among the analyzed units (`ModuleState::units`).
    unit_index: u32,
    /// The units analyzed before this file.
    units: Vec<UnitInterface>,
    /// The package units named in `uses` (`docs/spec/units.md` 3.1).
    used_units: Vec<UsedUnit>,
    /// Analyzing the `interface` section of a unit (only the `uses` of that section are visible).
    in_interface_part: bool,
    is_unit: bool,
    /// The item of `uses` naming `System.SysUtils` (the order of E2502's fixes).
    sysutils_item: Option<TextRange>,
    /// The generic types declared in the file (`docs/spec/generics.md` 2.3).
    generic_types: Vec<generics::GenericTypeName>,
    /// The generic types of the `type` section being declared whose type parameters are not
    /// entered yet.
    generic_params: Vec<generics::PendingParams>,
    /// The type parameters of the generic routines whose heading or body is being analyzed,
    /// innermost last (`docs/spec/generics.md` 2.2): (lowercase name, type parameter).
    routine_type_params: Vec<(String, types::TypeId)>,
    /// The candidates of the routine names written with type arguments, by the start of the
    /// name (`docs/spec/generics.md` 5.4): the first candidate (the routine resolved), and all.
    explicit_sets: std::collections::HashMap<TextSize, (RoutineId, Vec<RoutineId>)>,
    /// The generic interfaces, procedural types and array types whose type parameters are in
    /// scope (their declarations being resolved; `docs/spec/generics.md` 2.2), innermost last.
    param_scopes: Vec<types::TypeId>,
    /// The instantiations written in the constraints being resolved, whose constraints are
    /// checked after them (`docs/spec/generics.md` 1.1).
    deferred_checks: Option<Vec<generics::DeferredCheck>>,
    /// The routines of the instantiations of generic types.
    instances: instances::InstanceRoutines,
    /// The declarations of the typed constants, of this file and those before it: a constant
    /// written with a type is a strong candidate of type argument inference
    /// (`docs/spec/generics.md` 5.2).
    typed_consts: std::collections::HashSet<Span>,
    /// The type parameter of `KeyEquals` / `KeyHash` for inference, made the first time a unit
    /// of the compilation needs it (`docs/spec/equality.md` chapter 4).
    key_param: Option<types::TypeId>,
}

/// Result of name resolution.
#[derive(Debug, Clone)]
enum Resolved {
    Var(VarId),
    Global(GlobalId),
    Const {
        value: Const,
        ty: Ty,
        /// The declaration (`None` for a built-in constant such as `MaxInt`).
        span: Option<Span>,
    },
    Routine(RoutineId),
    Type(Ty),
    Bool(bool),
    Func(Builtin),
    Proc(builtins::BuiltinProc),
    /// `KeyEquals` / `KeyHash` (`docs/spec/equality.md` chapter 4), with the type argument when
    /// it is written (`KeyEquals<TPoint>`).
    Key(KeyOp, Option<Ty>),
    Unit(UnitRef),
    /// `TFoo.Create` of a class (`docs/spec/objects.md` 3.1): the class and the constructor's
    /// name as written (any constructor the class has).
    ClassCreate(types::TypeId, String),
    /// `Exception.RaiseOuterException` (`docs/spec/exceptions.md` 5.4).
    RaiseOuter,
    /// `Input` / `Output` / `ErrOutput` (`docs/spec/semantics.md` 6.1).
    StdFile(builtins::StdFile),
    /// A member of an instance (`docs/spec/objects.md` 2.3): `O.M`, a name of a member of `Self`
    /// written without `Self.`, or `inherited M` (`class` is then the parent of the method's
    /// class, called statically). `class` is the static type of `recv`.
    Member {
        recv: Box<Expr>,
        class: types::TypeId,
        member: methods::Member,
        inherited: bool,
    },
    /// A diagnostic was reported (error type).
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitRef {
    System,
    SysUtils,
    /// A unit of the package named in `uses` (index into `Sema::used_units`).
    Package(usize),
    /// The unit being analyzed, named in its own code (`docs/spec/units.md` 3.2).
    Own,
}

/// Result of looking a name up in the units named in `uses`.
enum UnitLookup {
    NotFound,
    Found(Resolved),
    /// Reported (E2502), or a unit whose names are the error type.
    Error,
}

/// The values of the enumerated types of `System.SysUtils` (`docs/spec/semantics.md` 5.3), in
/// ordinal order. The ordinals are the operands of `StrReplace`, `StrUpper` and the others
/// (`bytecode::REPLACE_FLAG_ALL`, `bytecode::CASE_MAPPING_FULL`, `bytecode::CASE_FOLD_UPPER`).
const REPLACE_FLAG_VALUES: [&str; 2] = ["rfReplaceAll", "rfIgnoreCase"];
const CASE_MAPPING_VALUES: [&str; 2] = ["cmSimple", "cmFull"];
const CASE_FOLD_VALUES: [&str; 2] = ["cfLower", "cfUpper"];

/// `MaxInt` (`docs/spec/semantics.md` 5.3).
fn max_int() -> Resolved {
    Resolved::Const {
        value: Const::Int(i32::MAX as i128),
        ty: Ty::Integer,
        span: None,
    }
}

/// `sLineBreak` (`docs/spec/semantics.md` 5.3): `#10` on every target.
fn line_break() -> Resolved {
    Resolved::Const {
        value: Const::Str(vec![10]),
        ty: Ty::String,
        span: None,
    }
}

impl<'a, 's> Sema<'a, 's> {
    /// What a name of `System.SysUtils` resolves to.
    fn sysutils_name(&mut self, n: builtins::SysUtilsName) -> Resolved {
        match n {
            builtins::SysUtilsName::Func(f) => Resolved::Func(f),
            builtins::SysUtilsName::Proc(p) => Resolved::Proc(p),
            builtins::SysUtilsName::ReplaceFlag => Resolved::Type(self.replace_flag_types().0),
            builtins::SysUtilsName::ReplaceFlags => Resolved::Type(self.replace_flag_types().1),
            builtins::SysUtilsName::CaseMapping => Resolved::Type(self.case_mapping_type()),
            builtins::SysUtilsName::CaseFold => Resolved::Type(self.case_fold_type()),
        }
    }

    /// `TReplaceFlag = (rfReplaceAll, rfIgnoreCase)` and `TReplaceFlags = set of TReplaceFlag`
    /// (`docs/spec/semantics.md` 5.3), added to the type table on first use.
    pub(super) fn replace_flag_types(&mut self) -> (Ty, Ty) {
        if let Some(t) = self.replace_flags {
            return t;
        }
        let flag = self.sysutils_enum("TReplaceFlag", &REPLACE_FLAG_VALUES);
        let flags = self.types.set_of(
            flag,
            types::DynWritten::Named {
                name: "TReplaceFlags".into(),
                full_name: "System.SysUtils.TReplaceFlags".into(),
                span: TextRange::default(),
            },
        );
        self.replace_flags = Some((flag, flags));
        (flag, flags)
    }

    /// `TCaseMapping = (cmSimple, cmFull)` of `AnsiUpperCase` / `AnsiLowerCase`
    /// (`docs/spec/semantics.md` 5.3), added to the type table on first use.
    pub(super) fn case_mapping_type(&mut self) -> Ty {
        if let Some(t) = self.case_mapping {
            return t;
        }
        let t = self.sysutils_enum("TCaseMapping", &CASE_MAPPING_VALUES);
        self.case_mapping = Some(t);
        t
    }

    /// `TCaseFold = (cfLower, cfUpper)` of `AnsiSameText` / `AnsiCompareText`
    /// (`docs/spec/semantics.md` 5.3), added to the type table on first use.
    pub(super) fn case_fold_type(&mut self) -> Ty {
        if let Some(t) = self.case_fold {
            return t;
        }
        let t = self.sysutils_enum("TCaseFold", &CASE_FOLD_VALUES);
        self.case_fold = Some(t);
        t
    }

    /// An enumerated type of `System.SysUtils` with the values `0..values.len()`.
    fn sysutils_enum(&mut self, name: &str, values: &[&str]) -> Ty {
        let defs = values
            .iter()
            .enumerate()
            .map(|(i, n)| types::EnumValueDef {
                name: (*n).to_string(),
                ord: i as i128,
                span: TextRange::default(),
            })
            .collect();
        Ty::Enum(self.types.add(types::TypeDef {
            name: Some(name.into()),
            full_name: Some(format!("System.SysUtils.{name}")),
            kind: types::TypeKind::Enum {
                values: defs,
                low: 0,
                high: values.len() as i128 - 1,
            },
            span: TextRange::default(),
        }))
    }

    /// The `System` global variable `ExitCode` (`docs/spec/semantics.md` 5.3), added to the
    /// globals the first time it is used.
    pub(super) fn exit_code_global(&mut self) -> GlobalId {
        if let Some(id) = self.exit_code {
            return id;
        }
        let id = GlobalId(self.globals.len() as u32);
        self.globals.push(Global {
            name: "ExitCode".into(),
            ty: Ty::Integer,
            span: None,
            unit: None,
        });
        self.exit_code = Some(id);
        id
    }

    fn span(&self, range: TextRange) -> Span {
        Span::new(self.input.file, range)
    }

    /// The file of a unit analyzed so far or being analyzed.
    pub(super) fn unit_file(&self, unit: u32) -> FileId {
        if unit == self.unit_index {
            self.input.file
        } else {
            self.units[unit as usize].file
        }
    }

    /// The name in the declaration of a class, record, interface or helper, in its file.
    pub(super) fn class_decl_span(&self, id: types::TypeId) -> Option<Span> {
        self.types
            .class_unit(id)
            .map(|u| Span::new(self.unit_file(u), self.types.get(id).span))
    }

    /// The name in a routine's (first) heading, in the routine's file.
    pub(super) fn routine_decl_span(&self, id: RoutineId) -> Span {
        let r = &self.routines[id.0 as usize];
        Span::new(r.file, r.name_span)
    }

    /// Adds the declaration of a routine as a secondary label, unless it is a method of a
    /// built-in class (`Exception.Create`), which is declared nowhere in the source.
    pub(super) fn with_routine_decl(
        &self,
        d: Diagnostic,
        id: RoutineId,
        label: &'static str,
    ) -> Diagnostic {
        match self.routines[id.0 as usize].method_of {
            Some(c) if types::TypeTable::is_builtin(c) => d,
            _ => d.with_secondary(self.routine_decl_span(id), label),
        }
    }

    /// The declaring identifier of a global variable, in its unit's file.
    pub(super) fn global_decl_span(&self, id: GlobalId) -> Option<Span> {
        let g = &self.globals[id.0 as usize];
        Some(Span::new(self.unit_file(g.unit?), g.span?))
    }

    /// A position inside the declaration of a type (the value of an enumeration), in the file
    /// that declares it.
    fn type_decl_span(&self, ty: Ty, range: TextRange) -> Span {
        let unit = ty
            .type_id()
            .and_then(|id| self.type_unit(id))
            .unwrap_or(self.unit_index);
        Span::new(self.unit_file(unit), range)
    }

    /// The unit declaring a type of the type table.
    fn type_unit(&self, id: types::TypeId) -> Option<u32> {
        self.type_decls
            .iter()
            .find(|d| d.ty.type_id() == Some(id))
            .map(|d| d.unit)
    }

    fn error(&mut self, code: Code, msg: impl Into<String>, range: TextRange) -> &mut Diagnostic {
        if let Some(c) = &mut self.cur {
            c.has_errors = true;
        }
        let d = Diagnostic::error(code, msg, Stage::Sema).with_primary(self.span(range), "");
        self.sink.push(d);
        self.last_mut()
    }

    fn push(&mut self, d: Diagnostic) {
        if d.is_error()
            && let Some(c) = &mut self.cur
        {
            c.has_errors = true;
        }
        self.sink.push(d);
    }

    /// Tries `f` without reporting diagnostics (only checks whether the fix's form passes type checking).
    /// Diagnostics produced are discarded, and the routine's error flag is restored.
    pub(super) fn dry_run<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let n = self.sink.len();
        let had_errors = self.cur.as_ref().map(|c| c.has_errors);
        self.dry_run += 1;
        let r = f(self);
        self.dry_run -= 1;
        self.sink.truncate(n);
        if let (Some(h), Some(c)) = (had_errors, self.cur.as_mut()) {
            c.has_errors = h;
        }
        r
    }

    fn last_mut(&mut self) -> &mut Diagnostic {
        self.sink.iter_mut().last().unwrap()
    }

    fn text(&self, tok: &SyntaxToken) -> &'a str {
        let r = tok.text_range();
        std::str::from_utf8(&self.input.bytes[usize::from(r.start())..usize::from(r.end())])
            .unwrap_or("")
    }

    fn run(mut self, state: &mut ModuleState) {
        let first_global = self.globals.len() as u32;
        let root = self.input.root.clone();
        let file = ast::SourceFile::cast(root).expect("SOURCE_FILE");
        let part = if let Some(program) = file.program() {
            self.run_program(program)
        } else if let Some(script) = file.script() {
            self.is_script = true;
            self.run_script(script)
        } else if let Some(unit) = file.unit() {
            self.is_unit = true;
            self.run_unit(unit)
        } else {
            // E0001 file: an empty program
            let main = self.empty_main(TextRange::empty(0.into()), true);
            FilePart {
                name: self.input.script_name.to_string(),
                main: Some(main),
                init: None,
                fini: None,
                exports: Vec::new(),
            }
        };
        let uses: Vec<String> = {
            let mut v: Vec<String> = Vec::new();
            if self.sysutils != SysUtils::NotUsed {
                v.push("System.SysUtils".into());
            }
            for u in self.used_units.iter().filter(|u| !u.sysutils) {
                if let Some(i) = u.iface {
                    v.push(self.units[i].name.clone());
                }
            }
            v
        };
        self.report_hint_uses();
        let is_program = part.main.is_some();
        // Put the shared state back
        state.globals = std::mem::take(&mut self.globals);
        state.exit_code = self.exit_code;
        state.replace_flags = self.replace_flags;
        state.case_mapping = self.case_mapping;
        state.case_fold = self.case_fold;
        state.routines = std::mem::take(&mut self.routines);
        state.plan = std::mem::take(&mut self.plan);
        state.types = std::mem::take(&mut self.types);
        state.type_decls = std::mem::take(&mut self.type_decls);
        state.attrs = std::mem::take(&mut self.attr_uses);
        state.hints = std::mem::take(&mut self.hints);
        state.units = std::mem::take(&mut self.units);
        state.instances = std::mem::take(&mut self.instances);
        state.typed_consts = std::mem::take(&mut self.typed_consts);
        state.key_param = self.key_param;
        if is_program {
            state.uses_sysutils = self.sysutils != SysUtils::NotUsed;
            state.main = part.main;
        }
        state.units.push(UnitInterface {
            name: part.name.clone(),
            file: self.input.file,
            name_span: self.unit_name_span,
            exports: part.exports,
            generics: self
                .generic_types
                .iter()
                .filter(|g| g.interface)
                .cloned()
                .collect(),
        });
        state.parts.push(UnitHir {
            name: part.name,
            file: self.input.file,
            is_program,
            init: part.init,
            fini: part.fini,
            first_global,
            global_count: state.globals.len() as u32 - first_global,
            uses,
        });
    }

    fn empty_main(&mut self, end: TextRange, syntax_errors: bool) -> Routine {
        Routine {
            name: "<main>".into(),
            vars: Vec::new(),
            param_count: 0,
            ret: None,
            body: Vec::new(),
            end_span: end,
            body_begin: None,
            name_span: end,
            has_syntax_errors: syntax_errors,
            has_errors: false,
            unit: self.unit_index,
            parent: None,
            link: false,
            is_abstract: false,
            builtin: false,
            generic: false,
            dtor_of: None,
            helper: false,
            operator: false,
            closure: false,
            captures: Vec::new(),
            groups: Default::default(),
            instance: None,
        }
    }

    // ---- program ----

    fn run_program(&mut self, program: ast::Program) -> FilePart {
        let name_tok = program.header().and_then(|h| h.name());
        let name = name_tok
            .as_ref()
            .map(|t| self.text(t).to_string())
            .unwrap_or_else(|| "Program".to_string());
        self.program_name = name.clone();
        let mut init_body: Vec<Stmt> = Vec::new();
        let mut init_last: Option<TextRange> = None;
        let mut checked_name = false;
        for item in program.items() {
            if !checked_name && !matches!(item, ast::Item::Uses(_)) {
                checked_name = true;
                if let Some(t) = &name_tok {
                    self.check_program_name(&name, t.text_range());
                }
            }
            self.declare_item(item, &mut init_body, &mut init_last);
        }
        if !checked_name && let Some(t) = &name_tok {
            self.check_program_name(&name, t.text_range());
        }
        self.check_forward_bodies();
        self.check_method_bodies();
        // Main processing
        let main_block = program.main_block();
        let (end_span, syntax_errors) = match &main_block {
            Some(b) => (
                b.end_token()
                    .map(|t| t.text_range())
                    .unwrap_or(b.syntax().text_range()),
                has_syntax_error(b.syntax(), self.sink, self.input.file),
            ),
            None => (TextRange::empty(self.input.root.text_range().end()), true),
        };
        self.begin_routine(None, None, "<main>".into(), end_span, true, syntax_errors);
        self.cur_mut().body_begin = main_block
            .as_ref()
            .and_then(|b| b.begin_token())
            .map(|t| t.text_range().end());
        let body = match &main_block {
            Some(b) => self.lower_block_stmts(b),
            None => Vec::new(),
        };
        let main = self.end_routine(body, end_span);
        let init = if init_body.is_empty() {
            None
        } else {
            let end = init_last.unwrap();
            self.begin_routine(None, None, "<init>".into(), end, false, false);
            Some(self.end_routine(init_body, end))
        };
        self.finish_program(name, main, init)
    }

    fn finish_program(&mut self, name: String, main: Routine, init: Option<Routine>) -> FilePart {
        FilePart {
            name,
            main: Some(main),
            init,
            fini: None,
            exports: Vec::new(),
        }
    }

    /// Whether a unit of this name is in the compilation.
    fn is_unit_compiled(&self, name: &str) -> bool {
        self.units.iter().any(|u| u.name.eq_ignore_ascii_case(name))
    }

    /// E0701: the program has the name of a unit of the compilation (`docs/spec/units.md` 4.3).
    fn check_program_name(&mut self, name: &str, range: TextRange) {
        if !self.is_unit_compiled(name) {
            return;
        }
        let lower = name.to_ascii_lowercase();
        let item = self
            .used_units
            .iter()
            .find(|u| u.lower == lower)
            .map(|u| u.item);
        let mut d = Diagnostic::error(
            Code::E(701),
            format!("`{name}` is already the name of a used unit"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        if let Some(i) = item {
            d = d.with_secondary(self.span(i), "unit used here");
        }
        self.push(d);
    }

    // ---- unit (`docs/spec/units.md`) ----

    fn run_unit(&mut self, unit: ast::Unit) -> FilePart {
        let name = unit
            .header()
            .and_then(|h| h.name())
            .map(|q| q.text())
            .unwrap_or_else(|| "Unit".to_string());
        self.program_name = name.clone();
        if let Some(h) = unit.header() {
            self.check_unit_directives(&h);
            // The unit's marks (`docs/spec/attributes.md` 6.2): its own uses are inside it
            if let Some(q) = h.name() {
                let range = q.syntax().text_range();
                self.unit_name_span = Some(range);
                let hints = self.read_hints(h.syntax());
                self.keep_hints(self.span(range), name.clone(), true, hints);
                self.push_hint_scope(self.span(range));
            }
        }
        let mut init_body: Vec<Stmt> = Vec::new();
        let mut init_last: Option<TextRange> = None;
        if let Some(intf) = unit.interface() {
            self.in_interface_part = true;
            for item in intf.items() {
                self.declare_item(item, &mut init_body, &mut init_last);
            }
            self.in_interface_part = false;
        }
        // The public part: the declarations of the `interface` section (4.1)
        let exports = self.global_scope.clone();
        if let Some(imp) = unit.implementation() {
            for item in imp.items() {
                self.declare_item(item, &mut init_body, &mut init_last);
            }
        }
        self.check_forward_bodies();
        self.check_method_bodies();
        // `<init>`: the initial values, then the `initialization` statements; `<fini>`: the
        // `finalization` statements (5.1). Both follow the rules of `<main>`.
        let end_span = unit
            .end_token()
            .map(|t| t.text_range())
            .unwrap_or_else(|| TextRange::empty(self.input.root.text_range().end()));
        let init = match unit.initialization() {
            Some(sec) => {
                let stmts: Vec<ast::Stmt> = sec.stmts().collect();
                Some(self.unit_section_routine(
                    "<init>",
                    sec.syntax(),
                    sec.keyword(),
                    &stmts,
                    init_body,
                    end_span,
                ))
            }
            None if init_body.is_empty() => None,
            None => {
                let end = init_last.expect("set with init_body");
                self.begin_routine(None, None, "<init>".into(), end, false, false);
                Some(self.end_routine(init_body, end))
            }
        };
        let fini = unit.finalization().map(|sec| {
            let stmts: Vec<ast::Stmt> = sec.stmts().collect();
            self.unit_section_routine(
                "<fini>",
                sec.syntax(),
                sec.keyword(),
                &stmts,
                Vec::new(),
                end_span,
            )
        });
        FilePart {
            name,
            main: None,
            init,
            fini,
            exports,
        }
    }

    /// `<init>` / `<fini>` of a unit with an `initialization` / `finalization` section: `prefix`
    /// (the initial values of the global variables) followed by the section's statements, in a
    /// routine that follows the rules of `<main>` (`docs/spec/units.md` 5.1).
    fn unit_section_routine(
        &mut self,
        name: &str,
        section: &SyntaxNode,
        keyword: Option<SyntaxToken>,
        stmts: &[ast::Stmt],
        prefix: Vec<Stmt>,
        end_span: TextRange,
    ) -> Routine {
        let syntax_errors = has_syntax_error(section, self.sink, self.input.file);
        let name_span = keyword.as_ref().map(|t| t.text_range()).unwrap_or(end_span);
        self.begin_routine(None, None, name.into(), name_span, true, syntax_errors);
        self.cur_mut().body_begin = keyword.map(|t| t.text_range().end());
        self.cur_mut().scopes.push(Vec::new());
        let mut body = prefix;
        for st in stmts {
            if let Some(s) = self.lower_stmt(st) {
                body.push(s);
            }
        }
        self.cur_mut().scopes.pop();
        self.end_routine(body, end_span)
    }

    /// An item of a declaration part (of a program or a unit section).
    fn declare_item(
        &mut self,
        item: ast::Item,
        init_body: &mut Vec<Stmt>,
        init_last: &mut Option<TextRange>,
    ) {
        match item {
            ast::Item::Uses(u) => self.check_uses(&u),
            ast::Item::Const(c) => self.declare_global_consts(c.decls(), false),
            ast::Item::ResString(c) => self.declare_global_consts(c.decls(), true),
            ast::Item::Var(v) => self.declare_global_vars(&v, init_body, init_last),
            ast::Item::Type(t) => self.declare_types(&t),
            ast::Item::Routine(r) => self.declare_routine(&r, false),
            ast::Item::Error(_) => {}
        }
    }

    /// The hint directives of a unit heading (`docs/spec/units.md` chapter 1): `platform` and
    /// `library` are W0010; `deprecated` and `experimental` are read as the unit's marks.
    fn check_unit_directives(&mut self, h: &ast::UnitHeader) {
        for d in h.directives() {
            let Some(kw) = d.keyword() else { continue };
            let text = self.text(&kw).to_string();
            let range = kw.text_range();
            match d.kind() {
                Some(SyntaxKind::PLATFORM_KW) | Some(SyntaxKind::LIBRARY_KW) => {
                    let del = crate::diagnostics::deletion_range(
                        self.input.bytes,
                        d.syntax().text_range(),
                    );
                    let dg = Diagnostic::warning(
                        Code::W(10),
                        format!("hint directive `{text}` is ignored"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "")
                    .with_fix(Fix::safe(
                        format!("remove `{text}`"),
                        vec![TextEdit::delete(self.input.file, del)],
                    ));
                    self.push(dg);
                }
                _ => {}
            }
        }
    }

    // ---- script ----

    fn run_script(&mut self, script: ast::Script) -> FilePart {
        let mut name = self.input.script_name.to_string();
        // A script named like a unit is `Script` (`docs/spec/units.md` 4.3)
        if self.is_unit_compiled(&name) {
            name = "Script".to_string();
            if self.is_unit_compiled(&name) {
                let at = self
                    .input
                    .root
                    .descendants_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| !t.kind().is_trivia() && t.kind() != SyntaxKind::EOF)
                    .map(|t| t.text_range())
                    .unwrap_or_default();
                self.error(
                    Code::E(701),
                    "`Script` is already the name of a used unit",
                    at,
                );
            }
        }
        self.program_name = name.clone();
        let eof = TextRange::empty(self.input.root.text_range().end());
        // Whether the <main> part of the script contains syntax errors
        let syntax_errors = script_main_has_syntax_error(&script, self.sink, self.input.file);
        self.begin_routine(None, None, "<main>".into(), eof, true, syntax_errors);
        // Collect the names of top-level variables first (E0700 notes in routines are reported even when the variable
        // is declared after the routine; `semantics.md` 1.2)
        for item in script.items() {
            match item {
                ast::ScriptItem::Var(v) => {
                    for decl in v.decls() {
                        for name_tok in decl.names() {
                            let lower = self.text(&name_tok).to_ascii_lowercase();
                            self.script_all_top_vars
                                .push((lower, name_tok.text_range()));
                        }
                    }
                }
                ast::ScriptItem::Stmt(ast::Stmt::InlineVar(iv)) => {
                    if let Some(name_tok) = iv.name() {
                        let lower = self.text(&name_tok).to_ascii_lowercase();
                        self.script_all_top_vars
                            .push((lower, name_tok.text_range()));
                    }
                }
                _ => {}
            }
        }
        let mut body: Vec<Stmt> = Vec::new();
        for item in script.items() {
            match item {
                ast::ScriptItem::Uses(u) => self.check_uses(&u),
                ast::ScriptItem::Const(c) => self.declare_global_consts(c.decls(), false),
                ast::ScriptItem::ResString(c) => self.declare_global_consts(c.decls(), true),
                ast::ScriptItem::Type(t) => self.declare_types(&t),
                ast::ScriptItem::Var(v) => {
                    // A top-level var block holds inline variables of <main>
                    for decl in v.decls() {
                        self.declare_script_var_decl(&decl, &mut body);
                    }
                }
                ast::ScriptItem::Routine(r) => {
                    // Save the <main> context while analyzing the routine
                    let saved = self.cur.take();
                    self.declare_routine(&r, true);
                    self.cur = saved;
                }
                ast::ScriptItem::Stmt(st) => {
                    if let Some(s) = self.lower_stmt(&st) {
                        body.push(s);
                    }
                }
                ast::ScriptItem::Error(_) => {}
            }
        }
        self.check_forward_bodies();
        self.check_method_bodies();
        let main = self.end_routine(body, eof);
        self.finish_program(name, main, None)
    }

    // ---- uses ----

    fn check_uses(&mut self, u: &ast::UsesClause) {
        for item in u.items() {
            let Some(q) = item.name() else { continue };
            let text = q.text();
            let lower = text.to_ascii_lowercase();
            let range = q.syntax().text_range();
            let alias = item.alias().map(|t| self.text(&t).to_string());
            match lower.as_str() {
                "system.sysutils" | "sysutils" => {
                    if lower == "sysutils" {
                        let d = Diagnostic::error(
                            Code::E(2501),
                            "use the full unit name `System.SysUtils`",
                            Stage::Sema,
                        )
                        .with_primary(self.span(range), "")
                        .with_fix(Fix::safe(
                            "replace with `System.SysUtils`",
                            vec![TextEdit::replace(self.input.file, range, "System.SysUtils")],
                        ));
                        self.push(d);
                    }
                    if self.sysutils != SysUtils::NotUsed {
                        self.error(
                            Code::E(2500),
                            "unit `System.SysUtils` is already used",
                            range,
                        );
                        continue;
                    }
                    // The part written in Pascal, found by the driver in the standard library
                    let iface = self
                        .input
                        .uses
                        .iter()
                        .find(|r| r.range == range)
                        .and_then(|r| match r.target {
                            UsesTarget::Unit(i) => Some(i),
                            UsesTarget::Broken => None,
                        });
                    if let Some(i) = iface {
                        self.used_units.push(UsedUnit {
                            lower,
                            alias: alias.as_ref().map(|a| a.to_ascii_lowercase()),
                            qualifier: alias.clone().unwrap_or_else(|| "System.SysUtils".into()),
                            iface: Some(i),
                            in_interface: self.in_interface_part,
                            item: range,
                            sysutils: true,
                        });
                    }
                    self.sysutils = match alias {
                        Some(a) => SysUtils::Alias(a),
                        None => SysUtils::Used,
                    };
                    self.sysutils_item = Some(range);
                }
                "system" => {
                    self.error(Code::E(2500), "unit `System` is already used", range);
                }
                _ => {
                    let target = self
                        .input
                        .uses
                        .iter()
                        .find(|r| r.range == range)
                        .map(|r| r.target);
                    let in_interface = self.in_interface_part;
                    match target {
                        Some(UsesTarget::Unit(i)) => {
                            if self.used_units.iter().any(|u| u.iface == Some(i)) {
                                let name = self.units[i].name.clone();
                                self.error(
                                    Code::E(2500),
                                    format!("unit `{name}` is already used"),
                                    range,
                                );
                                continue;
                            }
                            if let Some(n) = self.units[i].name_span {
                                self.hint_use(Span::new(self.units[i].file, n), range);
                            }
                            self.used_units.push(UsedUnit {
                                lower: self.units[i].name.to_ascii_lowercase(),
                                alias: alias.as_ref().map(|a| a.to_ascii_lowercase()),
                                qualifier: alias.clone().unwrap_or_else(|| text.clone()),
                                iface: Some(i),
                                in_interface,
                                item: range,
                                sysutils: false,
                            });
                        }
                        // Reported by the driver; its names resolve to the error type (3.4)
                        Some(UsesTarget::Broken) => self.used_units.push(UsedUnit {
                            lower,
                            alias: alias.as_ref().map(|a| a.to_ascii_lowercase()),
                            qualifier: alias.clone().unwrap_or_else(|| text.clone()),
                            iface: None,
                            in_interface,
                            item: range,
                            sysutils: false,
                        }),
                        None => self.unit_not_found(&text, &lower, range),
                    }
                }
            }
        }
    }

    /// E2500 for a unit that is neither in the standard library nor in the package, with the
    /// notes of `docs/spec/diagnostics.md` 4.10.
    fn unit_not_found(&mut self, text: &str, lower: &str, range: TextRange) {
        let has_root = self.input.has_root;
        let d = self.error(Code::E(2500), format!("unit `{text}` not found"), range);
        let std_prefix = ["system.", "puy.", "xml."]
            .iter()
            .any(|p| lower.starts_with(p));
        if builtins::is_delphi_os_unit(lower) {
            d.notes
                .push("Puy has no OS-specific units, VCL or FMX".into());
            d.notes.push(
                "check the platform at run time with `TOSVersion.Platform`, and use `Puy.UI` for a GUI".into(),
            );
        } else if std_prefix {
            let names: Vec<String> = puy_stdlib::UNITS
                .iter()
                .map(|u| format!("`{}`", u.name))
                .collect();
            d.notes.push(format!(
                "the standard library of this release has only {}; more units come with later releases",
                names.join(", ")
            ));
        } else if has_root {
            d.notes.push(format!(
                "unit `{text}` is looked up as {}",
                unit_lookup_paths(text)
            ));
        } else {
            d.notes
                .push("a script without `puy.toml` can only use standard library units".into());
        }
    }

    // ---- Declarations ----

    /// Whether the name is already declared (E0701). `range` is the identifier of the new declaration.
    fn check_duplicate(&mut self, lower: &str, range: TextRange) -> bool {
        if !self.class_scopes.is_empty() {
            // A member of the class being declared (`docs/spec/objects.md` 2.2)
            return self.class_duplicate(lower, range);
        }
        if let Some(prev) = self.find_declared(lower) {
            let name = self.src(range);
            let d = Diagnostic::error(
                Code::E(701),
                format!("`{name}` is already declared"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "declared again here")
            .with_secondary(self.span(prev), "declared here");
            self.push(d);
            return true;
        }
        self.check_reserved_name(lower, range)
    }

    /// E0701 for a declaration of a reserved name (`docs/spec/semantics.md` 2.3, `docs/spec/objects.md` 2.3).
    pub(super) fn check_reserved_name(&mut self, lower: &str, range: TextRange) -> bool {
        if !matches!(
            lower,
            "result" | "exit" | "break" | "continue" | "self" | "true" | "false"
        ) {
            return false;
        }
        let name = self.src(range);
        let d = Diagnostic::error(
            Code::E(701),
            format!("`{name}` is a reserved name"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        self.push(d);
        true
    }

    /// Position of a declaration in the same scope (for locals, the names visible in the whole routine; for globals,
    /// the globals and the script's top level).
    fn find_declared(&self, lower: &str) -> Option<TextRange> {
        if let Some(c) = &self.cur {
            // The names visible from the enclosing routines too: a nested routine cannot declare
            // them again (`docs/spec/semantics.md` 4.9)
            if let Some((up, sym)) = self.lookup_scopes(lower) {
                return Some(self.lsym_span(up, &sym));
            }
            if !c.is_main {
                return None;
            }
            // <main> (the script's top level) shares the global scope
            if !self.is_script {
                return None;
            }
        } else if self.is_script {
            // Declaring a routine in a script (the <main> context is saved): top-level variables share the scope too
            if let Some((_, span)) = self.script_top_vars.iter().find(|(n, _)| n == lower) {
                return Some(*span);
            }
        }
        for (n, sym) in self.global_scope.iter().rev() {
            if n == lower {
                return Some(match sym {
                    GSym::Global(id) => self.globals[id.0 as usize]
                        .span
                        .expect("a global in the scope is declared"),
                    GSym::Const { span, .. } | GSym::Type { span, .. } => *span,
                    GSym::Routine(id) => self.routines[id.0 as usize].name_span,
                });
            }
        }
        None
    }

    /// Position of the declaration of a name visible from that position (local scope, globals). Used for E0708 in type
    /// positions and for detecting a user-defined `IntToStr` (`find_declared` checks duplicates and does not see globals in `<main>`).
    fn find_visible(&self, lower: &str) -> Option<TextRange> {
        if let Some((up, sym)) = self.lookup_scopes(lower) {
            return Some(self.lsym_span(up, &sym));
        }
        for (n, sym) in self.global_scope.iter().rev() {
            if n == lower {
                return Some(match sym {
                    GSym::Global(id) => self.globals[id.0 as usize]
                        .span
                        .expect("a global in the scope is declared"),
                    GSym::Const { span, .. } | GSym::Type { span, .. } => *span,
                    GSym::Routine(id) => self.routines[id.0 as usize].name_span,
                });
            }
        }
        None
    }

    /// The position of the declaration of a local symbol of the routine `up` levels out.
    fn lsym_span(&self, up: u32, sym: &LSym) -> TextRange {
        match sym {
            LSym::Var(id) => self.ctx_at(up).vars[id.0 as usize].span,
            LSym::Routine(id) => self.routines[id.0 as usize].name_span,
            LSym::Const { span, .. } | LSym::Type { span, .. } => *span,
        }
    }

    /// Whether this is a unit name (`System`, `System.SysUtils`, or an alias).
    fn is_unit_name(&self, lower: &str) -> bool {
        if lower == "system" {
            return true;
        }
        match &self.sysutils {
            SysUtils::Used => lower == "system.sysutils",
            SysUtils::Alias(a) => a.eq_ignore_ascii_case(lower),
            SysUtils::NotUsed => false,
        }
    }

    /// A built-in exception class named through `System.SysUtils` (or its alias), which exports
    /// the same classes (`docs/spec/exceptions.md` chapter 2).
    fn sysutils_exception(&self, lower: &str) -> Option<Ty> {
        let (unit, name) = lower.rsplit_once('.')?;
        if unit == "system" || !self.is_unit_name(unit) {
            return None;
        }
        builtins::builtin_exception(name)
    }

    fn src(&self, range: TextRange) -> String {
        String::from_utf8_lossy(
            &self.input.bytes[usize::from(range.start())..usize::from(range.end())],
        )
        .into_owned()
    }

    /// Constants of a `const` section, or of a `resourcestring` section when `resource` is set
    /// (always `string`; `docs/spec/semantics.md` 3.6).
    fn declare_global_consts(
        &mut self,
        decls: impl Iterator<Item = ast::ConstDecl>,
        resource: bool,
    ) {
        for decl in decls {
            // `docs/spec/attributes.md` 1.2
            self.attributes_not_allowed(decl.syntax());
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let scope = self.declare_names_hints(decl.syntax(), &[name_tok], None);
            self.begin_names_hints(scope);
            let typed = resource || decl.type_ref().is_some();
            let (value, ty) = self.eval_const_decl(decl.type_ref(), decl.value(), resource);
            self.end_names_hints(scope);
            if self.check_duplicate(&lower, range) {
                continue;
            }
            self.note_typed_const(typed, range);
            if self.cur.is_some() {
                // Script top level: placed in the global scope
            }
            self.global_scope.push((
                lower,
                GSym::Const {
                    ty: if value.is_some() { ty } else { Ty::Error },
                    value: value.unwrap_or(Const::Int(0)),
                    span: range,
                },
            ));
        }
    }

    /// Records the declaration of a constant written with a type (`docs/spec/generics.md` 5.2).
    pub(super) fn note_typed_const(&mut self, typed: bool, range: TextRange) {
        if typed {
            let at = self.span(range);
            self.typed_consts.insert(at);
        }
    }

    /// Value and type of a constant declaration (a typed constant is converted to the written type). Error type if there is no value.
    /// A `resourcestring` (`resource`) is checked as a typed constant of type `string`; a written type (E0417,
    /// reported by the parser) is not used.
    fn eval_const_decl(
        &mut self,
        type_ref: Option<ast::TypeSpec>,
        value: Option<ast::Expr>,
        resource: bool,
    ) -> (Option<Const>, Ty) {
        let declared = if resource {
            Some(Ty::String)
        } else {
            type_ref.map(|t| self.resolve_type(Some(t)))
        };
        let Some(value) = value else {
            return (None, Ty::Error);
        };
        let e = match declared {
            Some(t) if !t.is_error() => self.lower_init(&value, t, true),
            Some(_) => {
                self.lower_expr(&value);
                return (None, Ty::Error);
            }
            // `const Digits = ['0'..'9'];` is a set (`docs/spec/types.md` 8.3)
            None => match &value {
                ast::Expr::ArrayCtor(ac) => {
                    let span = ast::trimmed_range(value.syntax());
                    match self.lower_untyped_set_const(ac, span) {
                        Some(e) => e,
                        None => self.lower_expr(&value),
                    }
                }
                _ => self.lower_expr(&value),
            },
        };
        if e.ty.is_error() {
            return (None, Ty::Error);
        }
        let range = e.span;
        match const_init_value(&e) {
            Some(v) => (Some(v), e.ty),
            None => {
                if !e.ty.is_error() {
                    self.error(Code::E(713), "constant expression required", range);
                }
                (None, Ty::Error)
            }
        }
    }

    fn declare_global_vars(
        &mut self,
        section: &ast::VarSection,
        init: &mut Vec<Stmt>,
        init_last: &mut Option<TextRange>,
    ) {
        for decl in section.decls() {
            let names = decl.names();
            let scope = self.declare_names_hints(decl.syntax(), &names, None);
            self.begin_names_hints(scope);
            let ty = self.resolve_type(decl.type_ref());
            let storage = self.decl_attributes(decl.syntax(), attrs::AttrSite::Var, ty);
            self.flush_attribute_checks();
            let mut ids = Vec::new();
            for name_tok in &names {
                let name = self.text(name_tok).to_string();
                let lower = name.to_ascii_lowercase();
                let range = name_tok.text_range();
                if self.check_duplicate(&lower, range) {
                    continue;
                }
                let id = GlobalId(self.globals.len() as u32);
                self.globals.push(Global {
                    name,
                    ty: storage,
                    span: Some(range),
                    unit: Some(self.unit_index),
                });
                self.global_scope.push((lower, GSym::Global(id)));
                ids.push(id);
            }
            if let Some(initializer) = decl.initializer() {
                // Initial values of globals are constant expressions
                let e = self.lower_init(&initializer, ty, true);
                if const_init_value(&e).is_none() && !e.ty.is_error() {
                    let r = e.span;
                    self.error(Code::E(713), "constant expression required", r);
                }
                if names.len() == 1
                    && let Some(&id) = ids.first()
                {
                    let span = decl.syntax().text_range();
                    init.push(Stmt {
                        kind: StmtKind::Assign {
                            target: Place::Global(id),
                            value: e,
                            check: self.checks_at(span.start()),
                            is_init: true,
                        },
                        span,
                    });
                    *init_last = Some(span);
                }
            }
            self.end_names_hints(scope);
        }
    }

    fn declare_script_var_decl(&mut self, decl: &ast::VarDecl, body: &mut Vec<Stmt>) {
        let names = decl.names();
        let scope = self.declare_names_hints(decl.syntax(), &names, None);
        self.begin_names_hints(scope);
        let ty = self.resolve_type(decl.type_ref());
        let storage = self.decl_attributes(decl.syntax(), attrs::AttrSite::Var, ty);
        self.flush_attribute_checks();
        let span = decl.syntax().text_range();
        let checks = self.checks_at(span.start());
        let init = decl
            .initializer()
            .map(|e| self.lower_init(&e, ty, checks.range));
        self.end_names_hints(scope);
        for name_tok in &names {
            let name = self.text(name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            if self.check_duplicate(&lower, range) {
                continue;
            }
            self.script_top_vars.push((lower.clone(), range));
            let id = self.add_var(name, storage, VarKind::Local, range);
            self.cur_mut().vars[id.0 as usize].decl = Some(decl.syntax().clone());
            self.cur_mut()
                .scopes
                .last_mut()
                .unwrap()
                .push((lower, LSym::Var(id)));
            if let Some(e) = &init
                && names.len() == 1
            {
                let value = e.clone();
                body.push(Stmt {
                    kind: StmtKind::Assign {
                        target: Place::Var(id, span),
                        value,
                        check: checks,
                        is_init: true,
                    },
                    span,
                });
            }
        }
    }

    fn checks_at(&self, pos: TextSize) -> Checks {
        let s = self.input.directives.switches.at(pos);
        Checks {
            overflow: s.overflow_checks,
            range: s.range_checks,
        }
    }

    fn assertions_at(&self, pos: TextSize) -> bool {
        self.input.directives.switches.at(pos).assertions
    }

    // ---- Routines ----

    fn declare_routine(&mut self, decl: &ast::RoutineDecl, in_script: bool) {
        let Some(header) = decl.header() else { return };
        // The type parameters of a generic routine or method and the generic type of a
        // method's body (`docs/spec/generics.md` 1.2), seen by the heading and the body (2.2)
        let (generic_class, own, fresh) = match self.routine_heading_params(&header) {
            generics::HeadingParams::None => (None, Vec::new(), false),
            generics::HeadingParams::Failed => return,
            generics::HeadingParams::Generic { class, own, fresh } => (class, own, fresh),
        };
        let mark = self.push_routine_type_params(&own);
        self.declare_routine_in(
            decl,
            &header,
            generic_class,
            fresh.then_some(own),
            in_script,
        );
        self.pop_routine_type_params(mark);
    }

    /// [`Sema::declare_routine`] with the type parameters seen. `fresh`: the type parameters of
    /// a generic routine declared by this heading.
    fn declare_routine_in(
        &mut self,
        decl: &ast::RoutineDecl,
        header: &ast::RoutineHeader,
        generic_class: Option<types::TypeId>,
        fresh: Option<Vec<types::TypeId>>,
        in_script: bool,
    ) {
        let header = header.clone();
        let Some(qname) = header.name() else { return };
        let name_text = qname.text();
        let name_range = qname.syntax().text_range();
        // Parameter and return types are resolved at the point of declaration; those of a
        // method's body see the class's nested types and constants (`docs/spec/objects.md` 2.2)
        let heading_class = generic_class.or_else(|| {
            name_text
                .rsplit_once('.')
                .and_then(|(c, _)| self.dry_run(|s| s.class_named(c)))
        });
        if let Some(c) = heading_class {
            self.class_scopes.push(statics::ClassScope {
                class: c,
                vis: types::Visibility::Public,
                names: Vec::new(),
            });
        }
        let uses_mark = self.hint_uses_mark();
        let (params, ret) = self.heading_signature(&header);
        if heading_class.is_some() {
            self.class_scopes.pop();
        }
        let lower = name_text.to_ascii_lowercase();
        if !name_text.contains('.')
            && let Some(kw) = header.first_token().filter(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::CONSTRUCTOR_KW | SyntaxKind::DESTRUCTOR_KW
                )
            })
        {
            // A constructor or destructor is a method: only its body is written outside the
            // class (`docs/spec/objects.md` chapter 1)
            let what = self.text(&kw).to_ascii_lowercase();
            self.error(
                Code::E(747),
                format!("a {what} must be declared in a class"),
                name_range,
            );
            return;
        }
        if name_text.contains('.') {
            // The heading of a method's body is not a use (`docs/spec/attributes.md` 6.2)
            self.drop_hint_uses(uses_mark);
            // The body of a method (`docs/spec/objects.md` chapter 1). The parser has reported
            // the method directives (E0401)
            for d in decl.directives() {
                let (Some(k), Some(kw)) = (d.kind(), d.keyword()) else {
                    continue;
                };
                match k {
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
            let Some(id) = self.method_heading(&header, &params, ret, generic_class) else {
                return;
            };
            // The attributes of a method are written on its declaration in the class
            // (`docs/spec/attributes.md` 1.2)
            let declared = self.routine_decl_span(id);
            self.attributes_on_forward(decl.syntax(), &name_text, Some(declared), false);
            self.hints_on_forward(decl.syntax(), &name_text, Some(declared), false);
            if decl.body().is_none() {
                return;
            }
            let class = self.routines[id.0 as usize].method_of;
            self.routine_body(decl, id, params, ret, name_text, name_range, lower, class);
            return;
        }
        // Directives
        let mut has_forward = false;
        for d in decl.directives() {
            let Some(kw) = d.keyword() else { continue };
            let range = kw.text_range();
            match d.kind() {
                Some(SyntaxKind::FORWARD_KW) if self.in_interface_part => {
                    // `docs/spec/units.md` 4.2: a heading in the `interface` section is already
                    // completed by a body in the `implementation` section
                    let del = crate::diagnostics::deletion_range(
                        self.input.bytes,
                        directive_with_semicolon(d.syntax()),
                    );
                    let dg = Diagnostic::error(
                        Code::E(723),
                        "`forward` is not allowed in the interface section",
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "")
                    .with_fix(Fix::safe(
                        "remove `forward`",
                        vec![TextEdit::delete(self.input.file, del)],
                    ));
                    self.push(dg);
                }
                Some(SyntaxKind::FORWARD_KW) => has_forward = true,
                Some(k) => self.routine_directive(k, &kw, d.syntax()),
                None => {}
            }
        }
        let body = decl.body();
        let has_overload = decl
            .directives()
            .any(|d| d.kind() == Some(SyntaxKind::OVERLOAD_KW));
        let overload_fix_at = if has_overload {
            None
        } else {
            decl.syntax()
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| t.kind() == SyntaxKind::SEMICOLON)
                .map(|t| t.text_range().end())
        };
        let id = match self.forward_to_complete(&lower, &params, has_forward, has_overload) {
            Some(id) => {
                // The heading of the body is not a use (`docs/spec/attributes.md` 6.2)
                self.drop_hint_uses(uses_mark);
                // Body of a forward routine: does the heading match?
                self.check_body_matches_forward(id, &name_text, name_range, &params, ret);
                // Its attributes are written on the first declaration (`docs/spec/attributes.md`
                // 1.2)
                let declared = self.routines[id.0 as usize]
                    .forward_span
                    .map(|r| Span::new(self.routines[id.0 as usize].file, r));
                self.attributes_on_forward(decl.syntax(), &name_text, declared, false);
                self.hints_on_forward(decl.syntax(), &name_text, declared, false);
                if body.is_none() {
                    // Another body-less declaration after the forward declaration. The parser has reported E0411,
                    // so report nothing here (E0723 at the end if it never gets a body)
                    return;
                }
                id
            }
            None => {
                if self.check_routine_redeclared(
                    &lower,
                    name_range,
                    &params,
                    has_overload,
                    overload_fix_at,
                ) {
                    return;
                }
                let id = RoutineId(self.routines.len() as u32);
                self.routines.push(RoutineInfo {
                    name: name_text.clone(),
                    params: params.clone(),
                    ret,
                    has_body: false,
                    forward_span: (has_forward || body.is_none()).then_some(name_range),
                    // A heading in the `interface` section needs a body, unless E0415 already
                    // reported the one written after it
                    implicit_forward: !has_forward
                        && body.is_none()
                        && (!self.in_interface_part
                            || decl
                                .syntax()
                                .children()
                                .any(|n| n.kind() == SyntaxKind::ERROR_NODE)),
                    name_span: name_range,
                    hir: None,
                    overload: has_overload,
                    overload_fix_at,
                    unit: self.unit_index,
                    file: self.input.file,
                    interface: self.in_interface_part,
                    parent: self.cur.as_ref().and_then(|c| c.routine),
                    calls: Vec::new(),
                    method_of: None,
                    self_kind: types::SelfKind::Instance,
                    is_abstract: false,
                    body_span: None,
                    closure: false,
                    type_params: fresh.clone().unwrap_or_default(),
                    closure_info: ClosureInfo::default(),
                });
                // A nested routine compiled as a closure (`docs/spec/objects.md` 9.7)
                if self.cur.is_some()
                    && let Some(&g) = self.plan.get(&(self.input.file, name_range))
                {
                    let info = &mut self.routines[id.0 as usize];
                    info.closure = true;
                    info.closure_info.group = Some(g);
                    self.cur_mut().members.push(id);
                }
                self.declare_hints(decl.syntax(), name_range, name_text.clone());
                if self.cur.as_ref().is_some_and(|c| !c.is_main) {
                    // A nested routine takes no attributes (`docs/spec/attributes.md` 1.2)
                    self.attributes_not_allowed(decl.syntax());
                } else {
                    self.decl_attributes(decl.syntax(), attrs::AttrSite::Other, Ty::Error);
                    self.flush_attribute_checks();
                }
                match &mut self.cur {
                    // A nested routine (`docs/spec/semantics.md` 4.9)
                    Some(c) => c
                        .scopes
                        .last_mut()
                        .expect("a routine has a scope")
                        .push((lower.clone(), LSym::Routine(id))),
                    None => self.global_scope.push((lower.clone(), GSym::Routine(id))),
                }
                id
            }
        };
        if has_forward || body.is_none() {
            // No body (the parser has reported E0411)
            return;
        }
        let _ = in_script;
        self.routine_body(decl, id, params, ret, name_text, name_range, lower, None);
    }

    /// The body of a routine, or of a method of `self_class` (whose `Self` is its first
    /// variable). `heading` is the parameters as the body's heading writes them.
    #[allow(clippy::too_many_arguments)]
    fn routine_body(
        &mut self,
        decl: &ast::RoutineDecl,
        id: RoutineId,
        heading: Vec<ParamInfo>,
        ret: Option<Ty>,
        name_text: String,
        name_range: TextRange,
        lower: String,
        self_class: Option<types::TypeId>,
    ) {
        // The uses inside the routine, and inside its class and the classes that class is
        // nested in, are inside their declarations (`docs/spec/attributes.md` 6.2)
        let mut classes = Vec::new();
        let mut cur = self_class;
        while let Some(c) = cur.filter(|c| !classes.contains(c)) {
            classes.push(c);
            cur = self.lexical_outer.get(&c).copied();
        }
        let scopes: Vec<Span> = classes
            .iter()
            .filter_map(|c| self.class_decl_span(*c))
            .collect();
        for s in &scopes {
            self.push_hint_scope(*s);
        }
        let decl_span = self.routine_decl_span(id);
        self.in_hint_scope(decl_span, |s| {
            s.routine_body_in(
                decl, id, heading, ret, name_text, name_range, lower, self_class,
            )
        });
        for _ in &scopes {
            self.pop_hint_scope();
        }
    }

    /// [`Sema::routine_body`] inside the scopes of its declarations.
    #[allow(clippy::too_many_arguments)]
    fn routine_body_in(
        &mut self,
        decl: &ast::RoutineDecl,
        id: RoutineId,
        heading: Vec<ParamInfo>,
        ret: Option<Ty>,
        name_text: String,
        name_range: TextRange,
        lower: String,
        self_class: Option<types::TypeId>,
    ) {
        let Some(body) = decl.body() else { return };
        self.routines[id.0 as usize].has_body = true;
        // Analyze the body
        let syntax_errors = has_syntax_error(decl.syntax(), self.sink, self.input.file);
        let end_span = body
            .block()
            .and_then(|b| b.end_token())
            .map(|t| t.text_range())
            .unwrap_or(decl.syntax().text_range());
        // The body of a nested routine is analyzed inside the enclosing routine's context
        let enclosing = self.cur.take();
        let qual_name = match &enclosing {
            Some(c) => format!("{}.{name_text}", c.qual_name),
            None => name_text.clone(),
        };
        if let Some(c) = enclosing {
            self.outer.push(c);
        }
        self.begin_routine(
            Some(id),
            ret,
            name_text.clone(),
            end_span,
            false,
            syntax_errors,
        );
        {
            let begin = body
                .block()
                .and_then(|b| b.begin_token())
                .map(|t| t.text_range().end());
            let ctx = self.cur_mut();
            ctx.lower = lower;
            ctx.name_span = name_range;
            ctx.body_begin = begin;
            ctx.qual_name = qual_name;
            ctx.self_class = self_class;
        }
        // A closure-form nested routine captures what it uses (`docs/spec/objects.md` 9.7)
        if self.routines[id.0 as usize].closure_info.group.is_some() {
            self.cur_mut().closure = true;
        }
        let self_kind = self.routines[id.0 as usize].self_kind;
        self.cur_mut().self_kind = self_kind;
        // `Self` (`docs/spec/objects.md` 2.3): found by `resolve_ident`, not by the scopes. A
        // class method's is the class reference; a static method has none
        let helper_target = self_class.and_then(|c| self.types.helper_target(c));
        if let (Some(c), Some(target)) = (self_class, helper_target) {
            // A helper's method (`docs/spec/objects.md` 10.8): `Self` is a value of its type
            match self_kind {
                types::SelfKind::Instance if self.types.self_by_ref(c) => {
                    self.add_var("Self".into(), target, VarKind::RecordSelf, name_range);
                }
                types::SelfKind::Instance => {
                    self.add_var("Self".into(), target, VarKind::SelfParam, name_range);
                }
                types::SelfKind::Class => {
                    let ty = match target {
                        Ty::Class(t) => self.types.class_ref(
                            t,
                            types::DynWritten::Anonymous {
                                generic: false,
                                span: name_range,
                            },
                        ),
                        _ => Ty::Error,
                    };
                    self.add_var("Self".into(), ty, VarKind::SelfParam, name_range);
                }
                types::SelfKind::Static => {}
            }
        } else if let Some(c) = self_class {
            match self_kind {
                types::SelfKind::Instance if self.types.is_record(c) => {
                    // `docs/spec/objects.md` chapter 6: like a `var` parameter
                    self.add_var(
                        "Self".into(),
                        Ty::Record(c),
                        VarKind::RecordSelf,
                        name_range,
                    );
                }
                types::SelfKind::Instance => {
                    self.add_var("Self".into(), Ty::Class(c), VarKind::SelfParam, name_range);
                }
                types::SelfKind::Class => {
                    let ty = self.types.class_ref(
                        c,
                        types::DynWritten::Anonymous {
                            generic: false,
                            span: name_range,
                        },
                    );
                    self.add_var("Self".into(), ty, VarKind::SelfParam, name_range);
                }
                types::SelfKind::Static => {}
            }
        }
        let first_param = self.cur_mut().vars.len();
        // Parameters. Their positions are those of the body's heading (W0017 is reported there,
        // not at a `forward` declaration; `docs/spec/semantics.md` 4.6)
        let params = self.routines[id.0 as usize].params.clone();
        let same_count = heading.len() == params.len();
        for (i, p) in params.iter().enumerate() {
            let at = if same_count { heading[i].span } else { p.span };
            let plower = p.name.to_ascii_lowercase();
            // A duplicate parameter name (E0701) still takes a slot (keeps the parameter list in step with the registers).
            // Only the first declaration of the name is visible
            let duplicate = self.check_duplicate(&plower, at);
            let vid = self.add_var(p.name.clone(), p.ty, VarKind::Param(p.mode), at);
            if !duplicate {
                self.cur_mut().scopes[0].push((plower, LSym::Var(vid)));
            }
        }
        self.cur_mut().vars.truncate(first_param + params.len());
        // Result
        if let Some(rt) = ret {
            let vid = self.add_var("Result".into(), rt, VarKind::Result, name_range);
            debug_assert_eq!(vid.0 as usize, first_param + params.len());
        }
        // Sections and body
        let mut stmts: Vec<Stmt> = Vec::new();
        for node in body.syntax().children() {
            match ast::Item::cast(node) {
                Some(ast::Item::Const(c)) => self.declare_local_consts(c.decls(), false),
                // E0416 was reported by the parser; the names are still declared so uses do not cascade
                Some(ast::Item::ResString(c)) => self.declare_local_consts(c.decls(), true),
                Some(ast::Item::Var(v)) => self.declare_local_vars(&v, &mut stmts),
                Some(ast::Item::Type(t)) => self.declare_types(&t),
                Some(ast::Item::Routine(r)) => self.declare_routine(&r, false),
                _ => {}
            }
        }
        if let Some(block) = body.block() {
            let inner = self.lower_block_stmts(&block);
            stmts.extend(inner);
        }
        let calls = std::mem::take(&mut self.cur_mut().calls);
        let srcs: Vec<VarId> = self.cur_mut().captures.iter().map(|(_, s)| *s).collect();
        let self_at = self.cur_mut().self_captured_at;
        let routine = self.end_routine(stmts, end_span);
        self.cur = self.outer.pop();
        let info = &mut self.routines[id.0 as usize];
        info.hir = Some(routine);
        info.calls = calls;
        info.closure_info.srcs = srcs;
        if let Some(s) = self_at {
            self.anon_self.insert(id, s);
        }
    }

    /// The parameters and the return type of a routine or method heading, resolved at the point
    /// of the declaration (with the checks of `docs/spec/semantics.md` 4.8).
    fn heading_signature(&mut self, header: &ast::RoutineHeader) -> (Vec<ParamInfo>, Option<Ty>) {
        self.signature_of(header.params(), header.is_function(), header.return_type())
    }

    /// The parameters and the return type written in a heading (of a routine, a method or an
    /// anonymous method).
    pub(super) fn signature_of(
        &mut self,
        groups: Vec<ast::ParamGroup>,
        is_function: bool,
        return_type: Option<ast::TypeSpec>,
    ) -> (Vec<ParamInfo>, Option<Ty>) {
        let mut params: Vec<ParamInfo> = Vec::new();
        let mut seen_default_error = false;
        for group in groups {
            let ty = match group.type_ref() {
                // `array of T` in a parameter is an open array (`docs/spec/types.md` 2.4)
                Some(ast::TypeSpec::Array(a)) if !a.has_brackets() => {
                    let t = self.open_array_of(&a);
                    if group.mode() == ast::ParamMode::Out && !t.is_error() {
                        let r = ast::trimmed_range(a.syntax());
                        self.error(
                            Code::E(3200),
                            "`out` open array parameters are not supported yet (planned for a later release)",
                            r,
                        )
                        .notes
                        .push("use a `var` parameter".into());
                        Ty::Error
                    } else {
                        t
                    }
                }
                t => self.resolve_type(t),
            };
            self.decl_attributes(group.syntax(), attrs::AttrSite::Param, ty);
            self.flush_attribute_checks();
            let attrs = attrs::attributes_key(group.syntax());
            let names = group.names();
            // An erroneous default value still makes the parameter optional (as an error-typed
            // value), so calls that omit it do not pile on E0705
            let default = group
                .default_node()
                .map(|_| self.param_default(&group, ty, names.len()));
            if default.is_none()
                && !seen_default_error
                && let Some(first) = names.first()
                && params.iter().any(|p: &ParamInfo| p.default.is_some())
            {
                // `docs/spec/semantics.md` 4.8
                seen_default_error = true;
                self.error(
                    Code::E(738),
                    "parameters with default values must come last",
                    first.text_range(),
                );
            }
            for n in names {
                params.push(ParamInfo {
                    name: self.text(&n).to_string(),
                    ty,
                    mode: group.mode(),
                    span: n.text_range(),
                    default: default.clone(),
                    attrs: attrs.clone(),
                });
            }
        }
        let ret = if is_function {
            Some(match return_type {
                Some(t) => self.resolve_type(Some(t)),
                None => Ty::Error,
            })
        } else {
            None
        };
        (params, ret)
    }

    /// A directive of a routine or method heading other than `forward` (`docs/spec/semantics.md`
    /// chapter 8): `inline` and `overload` are accepted as they are, the others are diagnosed.
    fn routine_directive(&mut self, kind: SyntaxKind, kw: &SyntaxToken, node: &SyntaxNode) {
        self.routine_directive_at(kind, kw, directive_with_semicolon(node));
    }

    /// [`Sema::routine_directive`] whose fixes delete `del` (the directive and its `;`).
    fn routine_directive_at(&mut self, kind: SyntaxKind, kw: &SyntaxToken, del: TextRange) {
        let text = self.text(kw).to_string();
        let range = kw.text_range();
        match kind {
            SyntaxKind::INLINE_KW => {}
            SyntaxKind::PLATFORM_KW | SyntaxKind::LIBRARY_KW => {
                let del = crate::diagnostics::deletion_range(self.input.bytes, del);
                let dg = Diagnostic::warning(
                    Code::W(10),
                    format!("hint directive `{text}` is ignored"),
                    Stage::Sema,
                )
                .with_primary(self.span(range), "")
                .with_fix(Fix::safe(
                    format!("remove `{text}`"),
                    vec![TextEdit::delete(self.input.file, del)],
                ));
                self.push(dg);
            }
            SyntaxKind::ASM_KW => {
                self.error(Code::E(3008), "`assembler` is not supported in Puy", range)
                    .notes
                    .push("Puy runs on a virtual machine and cannot execute assembly".into());
            }
            SyntaxKind::SAFECALL_KW => {
                self.error(Code::E(3010), "`safecall` is not supported in Puy", range)
                    .notes
                    .push("Puy does not support COM".into());
            }
            SyntaxKind::EXTERNAL_KW => {
                self.error(Code::E(3011), "`external` is not supported in Puy", range)
                    .notes
                    .push(
                        "Puy has no DLLs; extend it with the Rust extension mechanism instead"
                            .into(),
                    );
            }
            // `docs/spec/semantics.md` 4.7 (checked with the redeclaration)
            SyntaxKind::OVERLOAD_KW => {}
            // Read with the declaration (`hints.rs`)
            SyntaxKind::DEPRECATED_KW | SyntaxKind::EXPERIMENTAL_KW => {}
            SyntaxKind::VARARGS_KW => {
                let del = crate::diagnostics::deletion_range(self.input.bytes, del);
                let dg = Diagnostic::warning(Code::W(9), "`varargs` is ignored", Stage::Sema)
                    .with_primary(self.span(range), "")
                    .with_fix(Fix::safe(
                        "remove `varargs`",
                        vec![TextEdit::delete(self.input.file, del)],
                    ));
                self.push(dg);
            }
            _ => {
                let del = crate::diagnostics::deletion_range(self.input.bytes, del);
                let dg = Diagnostic::warning(
                    Code::W(9),
                    format!("calling convention `{text}` is ignored"),
                    Stage::Sema,
                )
                .with_primary(self.span(range), "")
                .with_fix(Fix::safe(
                    format!("remove `{text}`"),
                    vec![TextEdit::delete(self.input.file, del)],
                ));
                self.push(dg);
            }
        }
    }

    /// Routines with only `forward` and no body (E0723).
    fn check_forward_bodies(&mut self) {
        let unit = self.unit_index;
        let missing: Vec<(String, TextRange, bool)> = self
            .routines
            .iter()
            .filter(|r| {
                r.unit == unit && !r.has_body && r.forward_span.is_some() && !r.implicit_forward
            })
            .map(|r| (r.name.clone(), r.forward_span.unwrap(), r.interface))
            .collect();
        for (name, span, interface) in missing {
            let msg = if interface {
                // `docs/spec/units.md` 4.2
                format!("`{name}` is declared in the interface section but has no body")
            } else {
                format!("forward declaration of `{name}` has no body")
            };
            let d =
                Diagnostic::error(Code::E(723), msg, Stage::Sema).with_primary(self.span(span), "");
            self.sink.push(d);
        }
    }

    fn declare_local_consts(
        &mut self,
        decls: impl Iterator<Item = ast::ConstDecl>,
        resource: bool,
    ) {
        for decl in decls {
            // `docs/spec/attributes.md` 1.2
            self.attributes_not_allowed(decl.syntax());
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let scope = self.declare_names_hints(decl.syntax(), &[name_tok], None);
            self.begin_names_hints(scope);
            let typed = resource || decl.type_ref().is_some();
            let (value, ty) = self.eval_const_decl(decl.type_ref(), decl.value(), resource);
            self.end_names_hints(scope);
            if self.check_duplicate(&lower, range) {
                continue;
            }
            self.note_typed_const(typed, range);
            let sym = LSym::Const {
                ty: if value.is_some() { ty } else { Ty::Error },
                value: value.unwrap_or(Const::Int(0)),
                span: range,
            };
            self.cur_mut().scopes.last_mut().unwrap().push((lower, sym));
        }
    }

    /// The routine's `var` section. Initial values may be any expression and are assigned in declaration order before the body's first statement.
    fn declare_local_vars(&mut self, section: &ast::VarSection, stmts: &mut Vec<Stmt>) {
        for decl in section.decls() {
            let names = decl.names();
            let scope = self.declare_names_hints(decl.syntax(), &names, None);
            self.begin_names_hints(scope);
            let ty = self.resolve_type(decl.type_ref());
            let storage = self.decl_attributes(decl.syntax(), attrs::AttrSite::Var, ty);
            self.flush_attribute_checks();
            let span = decl.syntax().text_range();
            let checks = self.checks_at(span.start());
            // An initial value sees only names declared before this declaration: analyze the expression before adding the variable
            let init = decl
                .initializer()
                .map(|e| self.lower_init(&e, ty, checks.range));
            for name_tok in &names {
                let name = self.text(name_tok).to_string();
                let lower = name.to_ascii_lowercase();
                let range = name_tok.text_range();
                if self.check_duplicate(&lower, range) {
                    continue;
                }
                let id = self.add_var(name, storage, VarKind::Local, range);
                self.cur_mut().vars[id.0 as usize].decl = Some(decl.syntax().clone());
                self.cur_mut()
                    .scopes
                    .last_mut()
                    .unwrap()
                    .push((lower, LSym::Var(id)));
                if let Some(e) = &init
                    && names.len() == 1
                {
                    let value = e.clone();
                    stmts.push(Stmt {
                        kind: StmtKind::Assign {
                            target: Place::Var(id, span),
                            value,
                            check: checks,
                            is_init: true,
                        },
                        span,
                    });
                }
            }
            self.end_names_hints(scope);
        }
    }

    fn begin_routine(
        &mut self,
        id: Option<RoutineId>,
        ret: Option<Ty>,
        name: String,
        name_span: TextRange,
        is_main: bool,
        syntax_errors: bool,
    ) {
        self.cur = Some(RoutineCtx {
            vars: Vec::new(),
            scopes: vec![Vec::new()],
            loop_depth: 0,
            except_depth: 0,
            finally_loop_depth: None,
            loop_vars: Vec::new(),
            loop_globals: Vec::new(),
            routine: id,
            ret,
            is_main,
            self_class: None,
            self_kind: types::SelfKind::Instance,
            has_errors: false,
            has_syntax_errors: syntax_errors,
            lower: name.to_ascii_lowercase(),
            qual_name: name.clone(),
            display_name: name,
            calls: Vec::new(),
            name_span,
            body_begin: None,
            used_vars: Vec::new(),
            ended_for_vars: Vec::new(),
            closure: false,
            captures: Vec::new(),
            anon_count: 0,
            top_anons: Vec::new(),
            self_captured_at: None,
            uncapturable: Vec::new(),
            group_vars: Vec::new(),
            own_block: None,
            members: Vec::new(),
        });
    }

    fn end_routine(&mut self, body: Vec<Stmt>, end_span: TextRange) -> Routine {
        let ctx = self.cur.take().unwrap();
        let param_count = ctx
            .vars
            .iter()
            .take_while(|v| {
                matches!(
                    v.kind,
                    VarKind::Param(_) | VarKind::SelfParam | VarKind::RecordSelf
                )
            })
            .count() as u32;
        let mut routine = Routine {
            name: ctx.display_name.clone(),
            vars: ctx.vars,
            param_count,
            ret: ctx.ret,
            body,
            end_span,
            body_begin: ctx.body_begin,
            name_span: ctx.name_span,
            has_syntax_errors: ctx.has_syntax_errors,
            has_errors: ctx.has_errors,
            unit: self.unit_index,
            parent: ctx.routine.and_then(|r| self.routines[r.0 as usize].parent),
            link: false,
            is_abstract: false,
            builtin: false,
            generic: false,
            dtor_of: None,
            helper: false,
            operator: false,
            closure: ctx.closure,
            captures: ctx.captures.iter().map(|(v, _)| *v).collect(),
            groups: GroupBlocks {
                own: ctx.own_block,
                ..Default::default()
            },
            instance: None,
        };
        routine.groups.made = self.group_blocks(&ctx.members, &ctx.group_vars);
        // Control-flow analysis (skipped for routines containing syntax errors)
        if !routine.has_syntax_errors {
            // Where W0017 is enabled, an unread parameter gets W0017 alone (`semantics.md` 4.6)
            let unused_param_on: Vec<bool> = routine
                .vars
                .iter()
                .map(|v| {
                    matches!(v.kind, VarKind::Param(_))
                        && self
                            .input
                            .directives
                            .switches
                            .at(v.span.start())
                            .warn_state(Code::W(17))
                            != crate::directives::WarnState::Off
                })
                .collect();
            // The calls of the nested routines read and assign this routine's variables
            // (`docs/spec/semantics.md` 4.9)
            // (for `<main>`, the anonymous methods written in it and the routines in them)
            let group: Vec<(RoutineId, Option<&Routine>, u32)> = (0..self.routines.len())
                .filter_map(|i| {
                    let r = RoutineId(i as u32);
                    let up = match ctx.routine {
                        Some(id) => nesting_distance(&self.routines, r, id)?,
                        None => {
                            1 + ctx
                                .top_anons
                                .iter()
                                .find_map(|&a| nesting_distance(&self.routines, r, a))?
                        }
                    };
                    (up > 0).then(|| (r, self.routines[i].hir.as_ref(), up))
                })
                .collect();
            let effects = flow::nested_effects(
                &group,
                routine.vars.len(),
                &self.types,
                self.input.file,
                self.input.bytes,
            );
            let diags = flow::analyze(
                &routine,
                &self.types,
                self.input.file,
                self.input.bytes,
                &ctx.used_vars,
                &unused_param_on,
                &effects,
            );
            for d in diags {
                if d.is_error() {
                    routine.has_errors = true;
                }
                self.sink.push(d);
            }
        }
        routine
    }

    /// The group blocks a routine makes for its hidden variables `vars` (group, variable;
    /// `docs/spec/objects.md` 14.9): each captures the cells its routines (`members`, the
    /// closure-form routines nested in it) take from the routine. Sets the index in the block of
    /// each variable the members capture.
    fn group_blocks(
        &mut self,
        members: &[RoutineId],
        vars: &[(u32, VarId)],
    ) -> Vec<(VarId, Vec<VarId>)> {
        let mut blocks: Vec<(u32, Vec<VarId>)> = Vec::new();
        for m in members {
            let info = &self.routines[m.0 as usize];
            let g = info
                .closure_info
                .group
                .expect("a member of a group has a group");
            let srcs = info.closure_info.srcs.clone();
            let k = match blocks.iter().position(|(x, _)| *x == g) {
                Some(k) => k,
                None => {
                    blocks.push((g, Vec::new()));
                    blocks.len() - 1
                }
            };
            let list = &mut blocks[k].1;
            let slots: Vec<u32> = srcs
                .iter()
                .map(|src| match list.iter().position(|v| v == src) {
                    Some(j) => j as u32,
                    None => {
                        list.push(*src);
                        list.len() as u32 - 1
                    }
                })
                .collect();
            if let Some(h) = &mut self.routines[m.0 as usize].hir {
                h.groups.slots = slots;
            }
        }
        vars.iter()
            .map(|(g, v)| {
                let cells = blocks
                    .iter()
                    .find(|(x, _)| x == g)
                    .map(|(_, l)| l.clone())
                    .unwrap_or_default();
                (*v, cells)
            })
            .collect()
    }

    /// The variable of the routine being analyzed holding the group block of the closure-form
    /// nested routine `r` (`docs/spec/objects.md` 14.9), for calling it or making it a value:
    /// `None` when the routine being analyzed is of `r`'s group (its own closure). Otherwise the
    /// hidden variable of the routine `r` is nested in, or of a routine of the group the routine
    /// being analyzed is nested in, reached like any variable of an enclosing routine.
    fn group_block(&mut self, r: RoutineId) -> Option<VarId> {
        let info = &self.routines[r.0 as usize];
        let (g, q) = (info.closure_info.group?, info.parent);
        for level in 0..=self.outer.len() as u32 {
            let x = self.ctx_at(level).routine;
            let member =
                x.is_some_and(|x| self.routines[x.0 as usize].closure_info.group == Some(g));
            if member {
                if level == 0 {
                    return None;
                }
                let v = match self.ctx_at(level).own_block {
                    Some(v) => v,
                    None => {
                        let v = self.hidden_block(level);
                        self.ctx_at_mut(level).own_block = Some(v);
                        v
                    }
                };
                return Some(self.var_rel(0, level, v));
            }
            if x == q {
                let found = self
                    .ctx_at(level)
                    .group_vars
                    .iter()
                    .find(|(x, _)| *x == g)
                    .map(|(_, v)| *v);
                let v = match found {
                    Some(v) => v,
                    None => {
                        let v = self.hidden_block(level);
                        self.ctx_at_mut(level).group_vars.push((g, v));
                        v
                    }
                };
                return Some(self.var_rel(0, level, v));
            }
        }
        None
    }

    /// A new hidden variable holding a group block, of the routine `level` routines out.
    fn hidden_block(&mut self, level: u32) -> VarId {
        let span = self.ctx_at(level).name_span;
        let ctx = self.ctx_at_mut(level);
        let id = VarId(ctx.vars.len() as u32);
        ctx.vars.push(Var {
            name: String::new(),
            ty: Ty::Callable(types::TPROC),
            kind: VarKind::Hidden,
            span,
            decl: None,
            is_for_var: false,
            is_loop_var: false,
            captured: false,
            read_by_nested: false,
            written_by_nested: None,
            written: false,
            cell: false,
        });
        id
    }

    fn cur_mut(&mut self) -> &mut RoutineCtx {
        self.cur.as_mut().expect("routine context")
    }

    pub(super) fn add_var(
        &mut self,
        name: String,
        ty: Ty,
        kind: VarKind,
        span: TextRange,
    ) -> VarId {
        let ctx = self.cur_mut();
        let id = VarId(ctx.vars.len() as u32);
        ctx.vars.push(Var {
            name,
            ty,
            kind,
            span,
            decl: None,
            is_for_var: false,
            is_loop_var: false,
            captured: false,
            read_by_nested: false,
            written_by_nested: None,
            written: false,
            cell: false,
        });
        id
    }

    /// Looks a name up in the scopes of the routine being analyzed and then in those of the
    /// routines enclosing it (`docs/spec/semantics.md` 4.9): the symbol and how many routines out
    /// it is declared (0: the routine being analyzed).
    fn lookup_scopes(&self, lower: &str) -> Option<(u32, LSym)> {
        for (up, c) in self.cur.iter().chain(self.outer.iter().rev()).enumerate() {
            for scope in c.scopes.iter().rev() {
                for (n, sym) in scope.iter().rev() {
                    if n == lower {
                        return Some((up as u32, sym.clone()));
                    }
                }
            }
        }
        None
    }

    /// The context of the routine `up` levels out (0: the routine being analyzed).
    fn ctx_at(&self, up: u32) -> &RoutineCtx {
        match up {
            0 => self.cur.as_ref().expect("routine context"),
            _ => &self.outer[self.outer.len() - up as usize],
        }
    }

    /// The routine being analyzed and the routines enclosing it, innermost first.
    fn enclosing_routines(&self) -> impl Iterator<Item = RoutineId> + '_ {
        self.cur
            .iter()
            .chain(self.outer.iter().rev())
            .filter_map(|c| c.routine)
    }

    /// How many routines out the `Result` a name `Result` refers to is: the routine being
    /// analyzed if it is a function, otherwise the innermost enclosing function
    /// (`docs/spec/semantics.md` 4.9).
    fn result_owner(&self) -> Option<u32> {
        self.cur
            .iter()
            .chain(self.outer.iter().rev())
            .position(|c| c.ret.is_some())
            .map(|up| up as u32)
    }

    /// How many routines out the function `id` is, if it is the routine being analyzed or one
    /// enclosing it.
    fn enclosing_function(&self, id: RoutineId) -> Option<u32> {
        self.cur
            .iter()
            .chain(self.outer.iter().rev())
            .position(|c| c.routine == Some(id) && c.ret.is_some())
            .map(|up| up as u32)
    }

    /// The `Result` variable of the routine `up` levels out (it follows the parameters).
    fn result_id(&self, up: u32) -> VarId {
        VarId(
            self.ctx_at(up)
                .vars
                .iter()
                .take_while(|v| {
                    matches!(
                        v.kind,
                        VarKind::Param(_) | VarKind::SelfParam | VarKind::RecordSelf
                    )
                })
                .count() as u32,
        )
    }

    /// The variable `id` of the routine `up` levels out, as a variable of the routine being
    /// analyzed: itself for `up == 0`, otherwise the variable referring to it (made on first use;
    /// `docs/spec/semantics.md` 4.9, `docs/spec/objects.md` 9.6).
    fn var_at(&mut self, up: u32, id: VarId) -> VarId {
        self.var_rel(0, up, id)
    }

    /// The context `level` routines out (0: the routine being analyzed).
    fn ctx_at_mut(&mut self, level: u32) -> &mut RoutineCtx {
        match level {
            0 => self.cur.as_mut().expect("routine context"),
            _ => {
                let n = self.outer.len();
                &mut self.outer[n - level as usize]
            }
        }
    }

    /// [`Sema::var_at`] for the routine `level` routines out: the variable of that routine
    /// referring to the variable `id` of the routine `up` routines further out. An anonymous
    /// method captures it (its cell comes from the variable of the routine around it, made first);
    /// a nested routine reaches it through the static link, or, past an anonymous method, the
    /// variable that anonymous method captured.
    fn var_rel(&mut self, level: u32, up: u32, id: VarId) -> VarId {
        if up == 0 {
            return id;
        }
        let closure = self.ctx_at(level).closure;
        if !closure && let Some(k) = (1..up).find(|k| self.ctx_at(level + k).closure) {
            let inner = self.var_rel(level + k, up - k, id);
            return self.add_outer(level, k, inner);
        }
        if !closure {
            return self.add_outer(level, up, id);
        }
        let kind = VarKind::Outer { up, var: id };
        if let Some(i) = self.ctx_at(level).vars.iter().position(|v| v.kind == kind) {
            self.check_capturable(level, up, id);
            return VarId(i as u32);
        }
        self.check_capturable(level, up, id);
        let src = self.var_rel(level + 1, up - 1, id);
        let owner = &mut self.ctx_at_mut(level + up).vars[id.0 as usize];
        owner.cell = true;
        let is_self = matches!(owner.kind, VarKind::SelfParam);
        let v = self.add_outer(level, up, id);
        let at = self.use_span;
        let ctx = self.ctx_at_mut(level);
        ctx.captures.push((v, src));
        if is_self && ctx.self_captured_at.is_none() {
            ctx.self_captured_at = at;
        }
        v
    }

    /// The variable of the routine `level` routines out referring to the variable `id` of the
    /// routine `up` routines further out (made on first use).
    fn add_outer(&mut self, level: u32, up: u32, id: VarId) -> VarId {
        let kind = VarKind::Outer { up, var: id };
        if let Some(i) = self.ctx_at(level).vars.iter().position(|v| v.kind == kind) {
            return VarId(i as u32);
        }
        let owner = &mut self.ctx_at_mut(level + up).vars[id.0 as usize];
        owner.captured = true;
        let (name, ty, span) = (owner.name.clone(), owner.ty, owner.span);
        let ctx = self.ctx_at_mut(level);
        let v = VarId(ctx.vars.len() as u32);
        ctx.vars.push(Var {
            name,
            ty,
            kind,
            span,
            decl: None,
            is_for_var: false,
            is_loop_var: false,
            captured: false,
            read_by_nested: false,
            written_by_nested: None,
            written: false,
            cell: false,
        });
        v
    }

    /// E0751 when the variable `id` of the routine `up` routines out of the anonymous method
    /// `level` routines out cannot be captured (`docs/spec/objects.md` 9.6): it points into that
    /// routine's frame. Reported once for each anonymous method and variable, at the first use
    /// outside a trial.
    fn check_capturable(&mut self, level: u32, up: u32, id: VarId) {
        if self.dry_run > 0 {
            return;
        }
        let v = &self.ctx_at(level + up).vars[id.0 as usize];
        let note = match v.kind {
            VarKind::Param(ParamMode::Var | ParamMode::Out) => {
                "copy the parameter to a local variable and capture that"
            }
            VarKind::Param(_) if matches!(v.ty, Ty::OpenArray(_)) => {
                "copy the open array to a dynamic array and capture that"
            }
            VarKind::Result => {
                "assign the value to a local variable, and that to `Result` at the end"
            }
            VarKind::RecordSelf => {
                "`Self` of a record method is passed like a `var` parameter; copy the fields needed to local variables"
            }
            _ => return,
        };
        let (name, decl) = (v.name.clone(), v.span);
        let Some(at) = self.use_span else { return };
        let key = (up, id);
        if self.ctx_at(level).uncapturable.contains(&key) {
            return;
        }
        self.ctx_at_mut(level).uncapturable.push(key);
        let d = Diagnostic::error(
            Code::E(751),
            format!("cannot capture `{name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(at), "")
        .with_secondary(self.span(decl), "declared here")
        .with_note(note);
        self.push(d);
        self.cur_mut().has_errors = true;
    }

    /// Records a write of the value of a variable ([`Var::written`]; on the variable of the
    /// enclosing routine for an `Outer` one): what would be E0707 for a `const` parameter,
    /// except an element of a dynamic array written (the array is shared, not changed). Trials
    /// record too: a write recorded once too often only counts an argument.
    pub(super) fn mark_value_written(&mut self, id: VarId) {
        let (up, var) = self.var_origin(id);
        self.ctx_at_mut(up).vars[var.0 as usize].written = true;
    }

    /// Records a write of a variable of an enclosing routine by a nested routine (it cannot be
    /// the enclosing routine's `for` loop variable; `docs/spec/semantics.md` 4.9).
    pub(super) fn mark_written(&mut self, id: VarId, at: TextRange) {
        let (up, var) = self.var_origin(id);
        if up == 0 || self.dry_run > 0 {
            return;
        }
        let n = self.outer.len();
        let v = &mut self.outer[n - up as usize].vars[var.0 as usize];
        let first = v.written_by_nested.is_none();
        v.written_by_nested.get_or_insert(at);
        // An anonymous method in the body of the loop (`docs/spec/objects.md` 9.6)
        if first && self.ctx_at(up).loop_vars.contains(&var) {
            let text = self.src(at);
            let d = Diagnostic::error(
                Code::E(707),
                format!("cannot assign to `{text}`"),
                Stage::Sema,
            )
            .with_primary(self.span(at), "the loop variable of the enclosing routine")
            .with_note(
                "an anonymous method cannot write a variable its enclosing routine uses as a `for` loop variable",
            );
            self.push(d);
            self.cur_mut().has_errors = true;
        }
    }

    /// The variable an `Outer` variable refers to: (how many routines out, its id), or `(0, id)`.
    /// A variable reached past an anonymous method is followed to the one that method captured.
    fn var_origin(&self, id: VarId) -> (u32, VarId) {
        let (mut up, mut var) = match self.var(id).kind {
            VarKind::Outer { up, var } => (up, var),
            _ => return (0, id),
        };
        while let VarKind::Outer { up: u, var: v } = self.ctx_at(up).vars[var.0 as usize].kind {
            up += u;
            var = v;
        }
        (up, var)
    }

    /// The declaration of a variable as written: for an `Outer` variable, the variable of the
    /// enclosing routine (its kind decides what may be done with it).
    fn var_decl(&self, id: VarId) -> &Var {
        let (up, var) = self.var_origin(id);
        &self.ctx_at(up).vars[var.0 as usize]
    }

    fn var(&self, id: VarId) -> &Var {
        &self.cur.as_ref().unwrap().vars[id.0 as usize]
    }

    // ---- Name resolution ----

    /// Resolves an unqualified name. `range` is the position of the identifier (for diagnostics).
    fn resolve_ident(&mut self, name: &str, range: TextRange) -> Resolved {
        self.use_span = Some(range);
        let lower = name.to_ascii_lowercase();
        // A type parameter of a generic routine hides the outer names (`docs/spec/generics.md`
        // 2.2)
        if let Some(p) = self.routine_type_param(&lower) {
            return Resolved::Type(Ty::Param(p));
        }
        if let Some((up, sym)) = self.lookup_scopes(&lower) {
            // `docs/spec/attributes.md` 6.2 (a routine is used once its overload is picked)
            if !matches!(sym, LSym::Routine(_)) {
                let decl = self.lsym_span(up, &sym);
                self.hint_use(self.span(decl), range);
            }
            return match sym {
                LSym::Var(id) => Resolved::Var(self.var_at(up, id)),
                LSym::Const { value, ty, span } => Resolved::Const {
                    value,
                    ty,
                    span: Some(self.span(span)),
                },
                // Inside an overload, its own name is the routine itself (as for globals below)
                LSym::Routine(id) => Resolved::Routine(
                    self.enclosing_routines()
                        .find(|c| self.routines[c.0 as usize].name.eq_ignore_ascii_case(name))
                        .filter(|c| {
                            self.routines[c.0 as usize].parent
                                == self.routines[id.0 as usize].parent
                        })
                        .unwrap_or(id),
                ),
                LSym::Type { ty, .. } => Resolved::Type(ty),
            };
        }
        // `Self` and the members of `Self` (`docs/spec/objects.md` 2.3)
        if let Some(class) = self.method_class() {
            if lower == "self" {
                if let Some((v, _)) = self.self_var() {
                    return Resolved::Var(v);
                }
                self.error(
                    Code::E(700),
                    "`Self` is not available in a static method",
                    range,
                );
                return Resolved::Error;
            }
            // In a helper's method, the members of its type come first, then the helper's own and
            // its parents' (`docs/spec/objects.md` 10.4)
            let mut class = class;
            if let Some(Ty::Class(t) | Ty::Record(t)) = self.types.helper_target(class)
                && !matches!(self.dry_run(|s| s.find_member(t, name, range)), Err(false))
            {
                class = t;
                // A member of the type the helper cannot use (E0743; 10.8)
                if let Err(true) = self.dry_run(|s| s.find_member(t, name, range)) {
                    let _ = self.find_member(t, name, range);
                    return Resolved::Error;
                }
            }
            match self.dry_run(|s| s.find_member(class, name, range)) {
                Ok(methods::Member::Static(owner, st)) => {
                    return self.static_resolved(owner, &st);
                }
                Ok(
                    methods::Member::Field { name: n, .. }
                    | methods::Member::RecField { name: n, .. },
                ) if self.method_self_kind() != Some(types::SelfKind::Instance) => {
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
                    return Resolved::Error;
                }
                Ok(member) => {
                    if let Some(recv) = self.implicit_receiver(range) {
                        return Resolved::Member {
                            recv: Box::new(recv),
                            class,
                            member,
                            inherited: false,
                        };
                    }
                }
                Err(_) => {}
            }
        }
        if let Some(c) = &self.cur
            && lower == "result"
        {
            {
                let decl = (!c.is_main).then_some(c.name_span);
                // In a nested procedure, `Result` is that of the innermost enclosing function
                // (`docs/spec/semantics.md` 4.9)
                if let Some(up) = self.result_owner() {
                    let id = self.result_id(up);
                    return Resolved::Var(self.var_at(up, id));
                }
                let mut d = Diagnostic::error(
                    Code::E(710),
                    "`Result` can only be used in a function",
                    Stage::Sema,
                )
                .with_primary(self.span(range), "");
                if let Some(s) = decl {
                    d = d.with_secondary(self.span(s), "declared here");
                }
                self.push(d);
                return Resolved::Error;
            }
        }
        // The members of the classes whose bodies are being declared (`docs/spec/objects.md` 2.2)
        // and the classes the method's class is nested in
        if (!self.class_scopes.is_empty() || self.method_class().is_some())
            && let Some((owner, st)) = self.scoped_static(name)
        {
            if let Some(d) = self.static_decl_span(owner, &st) {
                self.hint_use(d, range);
            }
            return self.static_resolved(owner, &st);
        }
        if let Some(decl) = self
            .global_scope
            .iter()
            .rev()
            .find(|(n, _)| *n == lower)
            .and_then(|(_, sym)| self.gsym_span(self.input.file, sym))
        {
            self.hint_use(decl, range);
        }
        for (n, sym) in self.global_scope.iter().rev() {
            if *n == lower {
                return match sym {
                    GSym::Global(id) => Resolved::Global(*id),
                    GSym::Const { value, ty, span } => Resolved::Const {
                        value: value.clone(),
                        ty: *ty,
                        span: Some(self.span(*span)),
                    },
                    // Inside an overload, its own name is the routine itself (assigning to
                    // the function name, recursive calls without parentheses; 4.3)
                    GSym::Routine(id) => Resolved::Routine(
                        self.enclosing_routines()
                            .find(|c| self.routines[c.0 as usize].name.eq_ignore_ascii_case(name))
                            .filter(|c| self.routines[c.0 as usize].parent.is_none())
                            .unwrap_or(*id),
                    ),
                    GSym::Type { ty, .. } => Resolved::Type(*ty),
                };
            }
        }
        match self.lookup_in_units(&lower, name, range) {
            UnitLookup::Found(r) => return r,
            UnitLookup::Error => return Resolved::Error,
            UnitLookup::NotFound => {}
        }
        if let Some(u) = self.unit_named(&lower) {
            return Resolved::Unit(u);
        }
        if lower == "system" {
            return Resolved::Unit(UnitRef::System);
        }
        if let SysUtils::Alias(a) = &self.sysutils
            && a.eq_ignore_ascii_case(&lower)
        {
            return Resolved::Unit(UnitRef::SysUtils);
        }
        match self.system_name(&lower) {
            Some(builtins::SystemName::Type(t)) => return Resolved::Type(t),
            Some(builtins::SystemName::True) => return Resolved::Bool(true),
            Some(builtins::SystemName::False) => return Resolved::Bool(false),
            Some(builtins::SystemName::Func(f)) => return Resolved::Func(f),
            Some(builtins::SystemName::Key(op)) => return Resolved::Key(op, None),
            Some(builtins::SystemName::Proc(p)) => return Resolved::Proc(p),
            Some(builtins::SystemName::ExitCode) => {
                return Resolved::Global(self.exit_code_global());
            }
            Some(builtins::SystemName::MaxInt) => return max_int(),
            Some(builtins::SystemName::LineBreak) => return line_break(),
            Some(builtins::SystemName::StdFile(f)) => return Resolved::StdFile(f),
            Some(builtins::SystemName::Reserved(m)) => {
                self.error(
                    Code::E(3200),
                    format!("`{name}` is not supported yet (planned for {m})"),
                    range,
                );
                return Resolved::Error;
            }
            Some(builtins::SystemName::Deleted(code)) => {
                let note = builtins::deleted_note(code);
                self.error(code, format!("`{name}` is not supported in Puy"), range)
                    .notes
                    .push(note.into());
                return Resolved::Error;
            }
            None => {}
        }
        if let Some(f) = builtins::lookup_sysutils(&lower) {
            match &self.sysutils {
                SysUtils::Used => return self.sysutils_name(f),
                SysUtils::Alias(a) => {
                    let alias = a.clone();
                    let d = Diagnostic::error(
                        Code::E(700),
                        format!("undeclared identifier `{name}`"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "")
                    .with_fix(Fix::safe(
                        format!("replace with `{alias}.{name}`"),
                        vec![TextEdit::replace(
                            self.input.file,
                            range,
                            format!("{alias}.{name}"),
                        )],
                    ));
                    self.push(d);
                    return Resolved::Error;
                }
                SysUtils::NotUsed => {
                    let mut d = Diagnostic::error(
                        Code::E(700),
                        format!("undeclared identifier `{name}`"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(range), "");
                    if let Some(fix) = self.uses_sysutils_fix() {
                        d = d.with_fix(fix);
                    }
                    self.push(d);
                    return Resolved::Error;
                }
            }
        }
        // A value of an enumeration written without its type (`docs/spec/types.md` 8.1)
        if self.unqualified_enum_value(name, range) {
            return Resolved::Error;
        }
        // A `for var` variable read after the loop
        let ended = self.cur.as_ref().and_then(|c| {
            c.ended_for_vars
                .iter()
                .rev()
                .find(|(n, _)| *n == lower)
                .map(|(_, s)| *s)
        });
        if let Some(decl) = ended {
            let d = Diagnostic::error(
                Code::E(700),
                format!("undeclared identifier `{name}`"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "")
            .with_secondary(self.span(decl), "declared here")
            .with_note("a variable declared with `for var` is only in scope inside the loop")
            .with_note("copy the value to another variable inside the loop");
            self.push(d);
            return Resolved::Error;
        }
        // A unit that could not be read: its names are the error type (`docs/spec/units.md` 3.4)
        if self.broken_unit_visible() {
            return Resolved::Error;
        }
        if lower == "notrefcounted" {
            self.not_ref_counted_misplaced(range);
            return Resolved::Error;
        }
        // A generic type without type arguments (`docs/spec/generics.md` 2.3)
        if self.generic_needs_args(name, range) {
            return Resolved::Error;
        }
        // A variable at the script's top level (not visible from routines)
        let top_var = self.script_top_var_outside_main(&lower);
        let mut d = Diagnostic::error(
            Code::E(700),
            format!("undeclared identifier `{name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        if top_var.is_none() {
            d = self.with_uses_unit_fix(d, &lower);
        }
        if let Some(decl) = top_var {
            d = d
                .with_secondary(self.span(decl), "declared here")
                .with_note("a top-level variable of a script is a local variable of the main block and is not visible inside routines")
                .with_note("pass the value as a parameter instead");
        }
        self.push(d);
        Resolved::Error
    }

    /// The package units visible here: in the `interface` section, only those of its own `uses`
    /// (`docs/spec/units.md` 3.1).
    fn visible_units(&self) -> impl Iterator<Item = (usize, &UsedUnit)> {
        let intf = self.in_interface_part;
        self.used_units
            .iter()
            .enumerate()
            .filter(move |(_, u)| u.in_interface || !intf)
    }

    /// The part of `System.SysUtils` written in Pascal, if visible here (index into
    /// `used_units`).
    pub(super) fn sysutils_part(&self) -> Option<usize> {
        self.visible_units()
            .find(|(_, u)| u.sysutils)
            .map(|(k, _)| k)
    }

    /// Whether the part of `System.SysUtils` written in Pascal declares `lower` publicly.
    pub(super) fn sysutils_part_declares(&self, lower: &str) -> bool {
        self.sysutils_part()
            .and_then(|k| self.used_units[k].iface)
            .is_some_and(|i| self.units[i].exports.iter().any(|(n, _)| n == lower))
    }

    fn broken_unit_visible(&self) -> bool {
        self.visible_units().any(|(_, u)| u.iface.is_none())
    }

    /// A name of `System`, or in a unit of the standard library also one of the built-ins only
    /// it sees (`docs/spec/collections.md` 12.1). Elsewhere those are undeclared (E0700).
    fn system_name(&self, lower: &str) -> Option<builtins::SystemName> {
        builtins::lookup_system(lower).or_else(|| {
            self.input
                .stdlib
                .then(|| builtins::lookup_stdlib_only(lower))
                .flatten()
        })
    }

    /// A unit name (or alias) written as a name: a visible package unit, or this unit itself.
    fn unit_named(&self, lower: &str) -> Option<UnitRef> {
        for (k, u) in self.visible_units().filter(|(_, u)| !u.sysutils) {
            let matches = match &u.alias {
                Some(a) => a == lower,
                None => u.lower == lower,
            };
            if matches {
                return Some(UnitRef::Package(k));
            }
        }
        (self.is_unit && self.program_name.eq_ignore_ascii_case(lower)).then_some(UnitRef::Own)
    }

    /// A built-in unit (`System`, `System.SysUtils` or its alias) named before a name, unless a
    /// declared name hides its first name (`docs/spec/units.md` 3.2).
    pub(super) fn builtin_unit_named(&self, lower: &str) -> Option<UnitRef> {
        let first = lower.split('.').next().unwrap_or(lower);
        if self.name_is_visible(first) {
            return None;
        }
        if lower == "system" {
            Some(UnitRef::System)
        } else if self.is_unit_name(lower) {
            Some(UnitRef::SysUtils)
        } else {
            None
        }
    }

    /// The longest leading part of a chain of names that names a visible package unit (or this
    /// unit), and how many names it takes (`docs/spec/units.md` 3.2). `None` when the first name
    /// is a visible name (it hides the unit).
    pub(super) fn unit_prefix(&self, chain: &[(String, TextRange)]) -> Option<(UnitRef, usize)> {
        let first = chain.first()?.0.to_ascii_lowercase();
        if self.name_is_visible(&first) {
            return None;
        }
        let mut joined: Vec<String> = Vec::new();
        let mut best = None;
        for (k, (n, _)) in chain.iter().enumerate() {
            joined.push(n.to_ascii_lowercase());
            if let Some(u) = self.unit_named(&joined.join(".")) {
                best = Some((u, k + 1));
            }
        }
        best
    }

    /// The full name of a unit used with an alias at the start of a chain: the alias and how
    /// many names the full name takes.
    fn aliased_full_name(&self, chain: &[(String, TextRange)]) -> Option<(String, usize)> {
        let first = chain.first()?.0.to_ascii_lowercase();
        if self.name_is_visible(&first) {
            return None;
        }
        let mut joined: Vec<String> = Vec::new();
        for (k, (n, _)) in chain.iter().enumerate().take(chain.len() - 1) {
            joined.push(n.to_ascii_lowercase());
            let full = joined.join(".");
            if let Some((_, u)) = self
                .visible_units()
                .find(|(_, u)| !u.sysutils && u.alias.is_some() && u.lower == full)
            {
                return Some((u.qualifier.clone(), k + 1));
            }
        }
        None
    }

    /// Whether a name is declared locally, in this file or in a visible unit (a qualifier with
    /// such a name is not a unit name).
    fn name_is_visible(&self, lower: &str) -> bool {
        if self.lookup_scopes(lower).is_some() {
            return true;
        }
        if self.global_scope.iter().any(|(n, _)| n == lower) {
            return true;
        }
        self.visible_units().any(|(_, u)| {
            u.iface
                .is_some_and(|i| self.units[i].exports.iter().any(|(n, _)| n == lower))
        })
    }

    /// Whether `name` names a value (a variable, a constant or a routine; not a type) in the unit `u`.
    pub(super) fn unit_names_value(&self, u: UnitRef, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        let sym = match u {
            UnitRef::Package(k) => self.used_units[k].iface.and_then(|i| {
                self.units[i]
                    .exports
                    .iter()
                    .find(|(n, _)| *n == lower)
                    .map(|(_, s)| s.clone())
            }),
            UnitRef::Own => self
                .global_scope
                .iter()
                .rev()
                .find(|(n, _)| *n == lower)
                .map(|(_, s)| s.clone()),
            _ => None,
        };
        // A routine is a value too (`U.F.X` reads a field of what `F` returns), as in `names_value`
        matches!(
            sym,
            Some(GSym::Global(_) | GSym::Const { .. } | GSym::Routine(_))
        )
    }

    /// Whether a visible unit (reached without an alias) declares `lower` in its public part as a
    /// value rather than a type. A name in several units is still E2502 when it is resolved.
    pub(super) fn unit_export_is_value(&self, lower: &str) -> bool {
        self.visible_units()
            .filter(|(_, u)| u.alias.is_none())
            .filter_map(|(_, u)| u.iface)
            .filter_map(|i| self.units[i].exports.iter().find(|(n, _)| n == lower))
            .any(|(_, s)| !matches!(s, GSym::Type { .. }))
    }

    /// A type declared by exactly one visible unit (no diagnostics).
    pub(super) fn unit_type(&self, lower: &str) -> Option<Ty> {
        let mut found = self.visible_units().filter_map(|(_, u)| {
            let i = u.iface?;
            self.units[i]
                .exports
                .iter()
                .find(|(n, _)| n == lower)
                .map(|(_, s)| s.clone())
        });
        let first = found.next()?;
        if found.next().is_some() {
            return None;
        }
        match first {
            GSym::Type { ty, .. } => Some(ty),
            _ => None,
        }
    }

    /// The name declaring a symbol of the file `file` (`None` for a routine: its use is
    /// recorded once its overload is picked).
    fn gsym_span(&self, file: FileId, sym: &GSym) -> Option<Span> {
        match sym {
            GSym::Global(id) => self.global_decl_span(*id),
            GSym::Const { span, .. } | GSym::Type { span, .. } => Some(Span::new(file, *span)),
            GSym::Routine(_) => None,
        }
    }

    /// What a public name of the unit `iface` resolves to, used at `range`.
    fn export_used(&mut self, iface: usize, sym: &GSym, range: TextRange) -> Resolved {
        if let Some(decl) = self.gsym_span(self.units[iface].file, sym) {
            self.hint_use(decl, range);
        }
        self.export_resolved(iface, sym)
    }

    /// What a public name of the unit `iface` resolves to.
    fn export_resolved(&self, iface: usize, sym: &GSym) -> Resolved {
        let file = self.units[iface].file;
        match sym {
            GSym::Global(id) => Resolved::Global(*id),
            GSym::Const { value, ty, span } => Resolved::Const {
                value: value.clone(),
                ty: *ty,
                span: Some(Span::new(file, *span)),
            },
            GSym::Routine(id) => Resolved::Routine(*id),
            GSym::Type { ty, .. } => Resolved::Type(*ty),
        }
    }

    /// Looks a name up in the public parts of the visible units (`docs/spec/units.md` 3.1). A
    /// name in two or more of them (or in one and in `System.SysUtils`) is E2502.
    fn lookup_in_units(&mut self, lower: &str, name: &str, range: TextRange) -> UnitLookup {
        let mut found: Vec<(usize, GSym)> = Vec::new();
        for (k, u) in self.visible_units() {
            // A unit with an alias is only reached through the alias (M1 1.3)
            if u.alias.is_some() {
                continue;
            }
            let Some(i) = u.iface else { continue };
            if let Some((_, sym)) = self.units[i].exports.iter().find(|(n, _)| n == lower) {
                found.push((k, sym.clone()));
            }
        }
        let in_sysutils =
            self.sysutils == SysUtils::Used && builtins::lookup_sysutils(lower).is_some();
        match found.as_slice() {
            [] => UnitLookup::NotFound,
            [(k, sym)] if !in_sysutils => {
                let i = self.used_units[*k]
                    .iface
                    .expect("found in a unit with a public part");
                let sym = sym.clone();
                UnitLookup::Found(self.export_used(i, &sym, range))
            }
            _ => {
                let found: Vec<(usize, Option<TextRange>)> = found
                    .iter()
                    .map(|(k, sym)| {
                        let decl = match sym {
                            GSym::Global(id) => self.globals[id.0 as usize].span,
                            GSym::Const { span, .. } | GSym::Type { span, .. } => Some(*span),
                            GSym::Routine(id) => Some(self.routines[id.0 as usize].name_span),
                        };
                        (*k, decl)
                    })
                    .collect();
                self.several_units(name, range, &found, in_sysutils);
                UnitLookup::Error
            }
        }
    }

    /// E2502: the name is declared in several units (each with the range of its declaration in
    /// its unit's file).
    pub(super) fn several_units(
        &mut self,
        name: &str,
        range: TextRange,
        found: &[(usize, Option<TextRange>)],
        sysutils: bool,
    ) {
        let mut d = Diagnostic::error(
            Code::E(2502),
            format!("`{name}` is declared in several units; qualify it"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        // In the order of `uses`
        let mut quals: Vec<(TextRange, String, Option<Span>)> = Vec::new();
        for (k, decl) in found {
            let u = &self.used_units[*k];
            let i = u.iface.expect("found in a unit with a public part");
            quals.push((
                u.item,
                u.qualifier.clone(),
                decl.map(|r| Span::new(self.units[i].file, r)),
            ));
        }
        if sysutils {
            let at = self.sysutils_item.unwrap_or_default();
            let q = match &self.sysutils {
                SysUtils::Alias(a) => a.clone(),
                _ => "System.SysUtils".into(),
            };
            quals.push((at, q, None));
        }
        quals.sort_by_key(|(at, _, _)| at.start());
        for (_, q, decl) in &quals {
            if let Some(s) = decl {
                d = d.with_secondary(*s, "declared here");
            }
            let text = format!("{q}.{name}");
            d = d.with_fix(Fix::suggestion(
                format!("replace with `{text}`"),
                vec![TextEdit::replace(self.input.file, range, text)],
            ));
        }
        d = d.with_note("Puy does not let a later unit in `uses` hide an earlier one");
        self.push(d);
    }

    /// The E0700 fix that adds a unit of the package to `uses` (`docs/spec/units.md` 3.1): the
    /// units already compiled whose public part declares the name.
    fn with_uses_unit_fix(&self, mut d: Diagnostic, lower: &str) -> Diagnostic {
        // A unit used with an alias: qualify with the alias (M1 1.3)
        let aliased = self.visible_units().find_map(|(_, u)| {
            let i = u.iface?;
            u.alias.as_ref()?;
            self.units[i]
                .exports
                .iter()
                .any(|(n, _)| n == lower)
                .then(|| u.qualifier.clone())
        });
        if let (Some(q), Some(p)) = (aliased, d.primary.clone()) {
            let name = self.src(p.span.range);
            return d.with_fix(Fix::safe(
                format!("replace with `{q}.{name}`"),
                vec![TextEdit::replace(
                    self.input.file,
                    p.span.range,
                    format!("{q}.{name}"),
                )],
            ));
        }
        let mut names: Vec<String> = Vec::new();
        for (i, u) in self.units.iter().enumerate() {
            // Named in some `uses` of this file (a later section included): adding it again
            // would be E2500
            let used = self
                .input
                .uses
                .iter()
                .any(|r| r.target == UsesTarget::Unit(i));
            if !used && u.exports.iter().any(|(n, _)| n == lower) {
                names.push(u.name.clone());
            }
        }
        match names.as_slice() {
            [] => d,
            [one] => {
                if let Some(fix) = self.uses_fix_for(one) {
                    d = d.with_fix(fix);
                }
                d
            }
            many => {
                let list: Vec<String> = many.iter().map(|n| format!("`{n}`")).collect();
                d.with_note(format!("declared in the units {}", list.join(", ")))
            }
        }
    }

    /// E0736 when `name` is the name of a value of an enumeration type in scope: the fix qualifies
    /// it with the type (`safe` when one type has such a value; one `suggestion` per type
    /// otherwise). Returns whether it reported.
    fn unqualified_enum_value(&mut self, name: &str, range: TextRange) -> bool {
        let mut types: Vec<Ty> = Vec::new();
        let local = self
            .cur
            .iter()
            .chain(self.outer.iter().rev())
            .flat_map(|c| c.scopes.iter().rev().flat_map(|s| s.iter().rev()))
            .filter_map(|(_, sym)| match sym {
                LSym::Type { ty, .. } => Some(*ty),
                _ => None,
            });
        let global = self
            .global_scope
            .iter()
            .rev()
            .filter_map(|(_, sym)| match sym {
                GSym::Type { ty, .. } => Some(*ty),
                _ => None,
            });
        for ty in local.chain(global) {
            if matches!(ty, Ty::Enum(_)) && !types.contains(&ty) {
                types.push(ty);
            }
        }
        // The enumerated types of `System.SysUtils` (`[rfReplaceAll]` written as in Delphi)
        if self.sysutils == SysUtils::Used {
            let has = |values: &[&str]| values.iter().any(|v| v.eq_ignore_ascii_case(name));
            if has(&REPLACE_FLAG_VALUES) {
                types.push(self.replace_flag_types().0);
            }
            if has(&CASE_MAPPING_VALUES) {
                types.push(self.case_mapping_type());
            }
            if has(&CASE_FOLD_VALUES) {
                types.push(self.case_fold_type());
            }
        }
        let mut fixes: Vec<(String, String)> = Vec::new();
        // Declaration order of the types
        types.sort_by_key(|t| t.type_id().map(|id| id.0));
        for ty in types {
            if let Some(v) = self
                .types
                .enum_values(ty)
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(name))
            {
                fixes.push((self.types.name(ty), v.name.clone()));
            }
        }
        if fixes.is_empty() {
            return false;
        }
        let mut d = Diagnostic::error(
            Code::E(736),
            format!("enumeration value `{name}` must be qualified with its type"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        let single = fixes.len() == 1;
        for (t, v) in fixes {
            let text = format!("{t}.{v}");
            let title = format!("replace with `{text}`");
            let edits = vec![TextEdit::replace(self.input.file, range, text)];
            d = d.with_fix(if single {
                Fix::safe(title, edits)
            } else {
                Fix::suggestion(title, edits)
            });
        }
        self.push(d);
        true
    }

    /// If a name written in a routine (outside `<main>`) is the name of a variable at the script's top level, the position of its declaration (for the 1.2 `notes`).
    pub(super) fn script_top_var_outside_main(&self, lower: &str) -> Option<TextRange> {
        if self.cur.as_ref().is_some_and(|c| !c.is_main) {
            self.script_all_top_vars
                .iter()
                .find(|(n, _)| *n == lower)
                .map(|(_, s)| *s)
        } else {
            None
        }
    }

    /// Fix that adds `uses System.SysUtils` (chapter 5).
    fn uses_sysutils_fix(&self) -> Option<Fix> {
        self.uses_fix_for("System.SysUtils")
    }

    /// Fix that adds a unit to the `uses` of the file (of the section being analyzed in a unit;
    /// `docs/spec/units.md` 3.1).
    fn uses_fix_for(&self, unit: &str) -> Option<Fix> {
        let site = match &self.uses_fix.implementation {
            Some(imp) if !self.in_interface_part => imp,
            _ => &self.uses_fix.main,
        };
        let title = format!("add `uses {unit}`");
        if let Some(pos) = site.uses_semicolon {
            return Some(Fix::safe(
                title,
                vec![TextEdit::insert(self.input.file, pos, format!(", {unit}"))],
            ));
        }
        let (pos, newline_before) = site.insert?;
        let text = if newline_before {
            format!("\nuses {unit};")
        } else {
            format!("uses {unit};\n")
        };
        Some(Fix::safe(
            title,
            vec![TextEdit::insert(self.input.file, pos, text)],
        ))
    }

    /// Resolves a unit-qualified name (`System.Length`, `SU.IntToStr`).
    fn resolve_in_unit(&mut self, unit: UnitRef, name: &str, range: TextRange) -> Resolved {
        let lower = name.to_ascii_lowercase();
        match unit {
            UnitRef::Package(k) => {
                let Some(i) = self.used_units[k].iface else {
                    return Resolved::Error;
                };
                let found = self.units[i]
                    .exports
                    .iter()
                    .find(|(n, _)| *n == lower)
                    .map(|(_, s)| s.clone());
                match found {
                    Some(sym) => self.export_used(i, &sym, range),
                    None => {
                        self.error(
                            Code::E(700),
                            format!("undeclared identifier `{name}`"),
                            range,
                        );
                        Resolved::Error
                    }
                }
            }
            UnitRef::Own => {
                let found = self
                    .global_scope
                    .iter()
                    .rev()
                    .find(|(n, _)| *n == lower)
                    .map(|(_, s)| s.clone());
                if let Some(decl) = found
                    .as_ref()
                    .and_then(|s| self.gsym_span(self.input.file, s))
                {
                    self.hint_use(decl, range);
                }
                match found {
                    Some(GSym::Global(id)) => Resolved::Global(id),
                    Some(GSym::Const { value, ty, span }) => Resolved::Const {
                        value,
                        ty,
                        span: Some(self.span(span)),
                    },
                    Some(GSym::Routine(id)) => Resolved::Routine(id),
                    Some(GSym::Type { ty, .. }) => Resolved::Type(ty),
                    None => {
                        self.error(
                            Code::E(700),
                            format!("undeclared identifier `{name}`"),
                            range,
                        );
                        Resolved::Error
                    }
                }
            }
            UnitRef::System => match self.system_name(&lower) {
                Some(builtins::SystemName::Type(t)) => Resolved::Type(t),
                Some(builtins::SystemName::True) => Resolved::Bool(true),
                Some(builtins::SystemName::False) => Resolved::Bool(false),
                Some(builtins::SystemName::Func(f)) => Resolved::Func(f),
                Some(builtins::SystemName::Key(op)) => Resolved::Key(op, None),
                Some(builtins::SystemName::Proc(p)) => Resolved::Proc(p),
                Some(builtins::SystemName::ExitCode) => Resolved::Global(self.exit_code_global()),
                Some(builtins::SystemName::MaxInt) => max_int(),
                Some(builtins::SystemName::LineBreak) => line_break(),
                Some(builtins::SystemName::StdFile(f)) => Resolved::StdFile(f),
                Some(builtins::SystemName::Reserved(m)) => {
                    self.error(
                        Code::E(3200),
                        format!("`{name}` is not supported yet (planned for {m})"),
                        range,
                    );
                    Resolved::Error
                }
                Some(builtins::SystemName::Deleted(code)) => {
                    let note = builtins::deleted_note(code);
                    self.error(code, format!("`{name}` is not supported in Puy"), range)
                        .notes
                        .push(note.into());
                    Resolved::Error
                }
                None => {
                    self.error(
                        Code::E(700),
                        format!("undeclared identifier `{name}`"),
                        range,
                    );
                    Resolved::Error
                }
            },
            UnitRef::SysUtils => match builtins::lookup_sysutils(&lower) {
                Some(f) => self.sysutils_name(f),
                None if builtins::builtin_exception(&lower).is_some() => Resolved::Type(
                    builtins::builtin_exception(&lower).expect("checked by the guard"),
                ),
                None if self.sysutils_part_declares(&lower) => {
                    let k = self.sysutils_part().expect("checked by the guard");
                    self.resolve_in_unit(UnitRef::Package(k), name, range)
                }
                None => {
                    if let Some(builtins::SystemName::Reserved(m)) = builtins::lookup_system(&lower)
                    {
                        self.error(
                            Code::E(3200),
                            format!("`{name}` is not supported yet (planned for {m})"),
                            range,
                        );
                    } else {
                        self.error(
                            Code::E(700),
                            format!("undeclared identifier `{name}`"),
                            range,
                        );
                    }
                    Resolved::Error
                }
            },
        }
    }

    /// A name written with type arguments in an expression (`docs/spec/generics.md` 1.4): a
    /// generic type (`TBox<Integer>.Create`), `TArray<T>`, a generic routine or method
    /// (`Max<Integer>`), or E0761 for a name that is not generic.
    pub(super) fn type_args_in_expr(
        &mut self,
        name: &SyntaxToken,
        args: &ast::TypeArgs,
        scope: generics::GenericScope,
    ) -> (Resolved, String, TextRange) {
        let text = self.text(name).to_string();
        let name_range = name.text_range();
        let range = TextRange::new(name_range.start(), args.syntax().text_range().end());
        let list: Vec<ast::TypeSpec> = args.args().collect();
        if let Some(t) = self.generic_type_with_args(
            &text,
            name_range,
            list.clone(),
            args.syntax().text_range(),
            scope,
        ) {
            let r = if t.is_error() {
                Resolved::Error
            } else {
                Resolved::Type(t)
            };
            return (r, text, range);
        }
        let lower = text.to_ascii_lowercase();
        // The built-in `TArray<T>`: a `TArray` without type parameters declared by the program
        // is another name (`docs/spec/generics.md` 2.3)
        let builtin_tarray = matches!(
            scope,
            generics::GenericScope::Unit(UnitRef::System) | generics::GenericScope::Visible
        );
        if lower == "tarray"
            && list.len() != 1
            && matches!(scope, generics::GenericScope::Unit(UnitRef::System))
        {
            // The parser takes any number of type arguments after a qualified name
            self.tarray_arity_error(list.len(), args.syntax().text_range());
            return (Resolved::Error, text, range);
        }
        if lower == "tarray" && list.len() == 1 && builtin_tarray {
            let elem = self.resolve_type(list.into_iter().next());
            if elem.is_error() {
                return (Resolved::Error, text, range);
            }
            let t = self.types.dyn_array(
                elem,
                types::DynWritten::Anonymous {
                    generic: true,
                    span: range,
                },
            );
            return (Resolved::Type(t), text, range);
        }
        if let generics::GenericScope::Unit(u) = scope {
            // `Unit.Max<Integer>` (`docs/spec/generics.md` 1.4)
            let r = self.resolve_in_unit(u, &text, name_range);
            let r = self.routine_type_args(r, &text, range, args);
            return (r, text, range);
        }
        // A generic routine or method (`Max<Integer>`; `docs/spec/generics.md` 1.4)
        let r = self.resolve_ident(&text, name_range);
        let r = self.routine_type_args(r, &text, range, args);
        (r, text, range)
    }

    /// Resolves an expression (a chain of names), reading the chain of `FIELD_EXPR`s as unit qualification.
    /// Returns `(resolution result, name as written, name position)`.
    pub(super) fn resolve_name_expr(
        &mut self,
        e: &ast::Expr,
    ) -> Option<(Resolved, String, TextRange)> {
        self.resolve_name_expr_with(e, false)
    }

    /// [`Sema::resolve_name_expr`]; `skip_type_args`: the type arguments of a member are left to
    /// the caller.
    fn resolve_name_expr_with(
        &mut self,
        e: &ast::Expr,
        skip_type_args: bool,
    ) -> Option<(Resolved, String, TextRange)> {
        match e {
            ast::Expr::Inherited(i) => {
                // `inherited Name` (`docs/spec/objects.md` 2.4): the parent's method, statically
                let whole = ast::trimmed_range(i.syntax());
                let Some(tok) = i.name() else {
                    self.error(
                        Code::E(704),
                        "`inherited` without a method name is only allowed as a statement",
                        whole,
                    );
                    return Some((Resolved::Error, "inherited".into(), whole));
                };
                let name = self.text(&tok).to_string();
                let range = tok.text_range();
                let Some(class) = self.method_class() else {
                    self.error(
                        Code::E(700),
                        "`inherited` is only allowed in a method",
                        whole,
                    );
                    return Some((Resolved::Error, name, range));
                };
                if self.types.is_helper(class) {
                    // `docs/spec/objects.md` 10.6
                    self.error(
                        Code::E(753),
                        "`inherited` is not allowed in a helper",
                        whole,
                    );
                    return Some((Resolved::Error, name, range));
                }
                if self.types.is_record(class) {
                    // A record has no parent (`docs/spec/objects.md` chapter 6)
                    self.error(
                        Code::E(700),
                        "`inherited` is only allowed in a method of a class",
                        whole,
                    );
                    return Some((Resolved::Error, name, range));
                }
                let parent = self.types.parent(class).unwrap_or(types::TOBJECT);
                let recv = self.self_expr(range)?;
                let r = match self.find_member(parent, &name, range) {
                    Ok(
                        member @ (methods::Member::Methods { .. } | methods::Member::Builtin(_)),
                    ) => Resolved::Member {
                        recv: Box::new(recv),
                        class: parent,
                        member,
                        inherited: true,
                    },
                    Ok(
                        methods::Member::Field { .. }
                        | methods::Member::RecField { .. }
                        | methods::Member::Static(..)
                        | methods::Member::Property(..)
                        | methods::Member::ExcInfo(_)
                        | methods::Member::HelperBuiltin(_),
                    )
                    | Err(false) => {
                        let t = self.types.name(Ty::Class(parent));
                        let builtin = self.dry_run(|s| {
                            let n = s.sink.len();
                            s.class_member_missing(&t, &name, range);
                            s.sink.iter().skip(n).any(|d| d.code == Some(Code::E(3200)))
                        });
                        if builtin {
                            self.class_member_missing(&t, &name, range);
                        } else {
                            self.error(
                                Code::E(700),
                                format!("`{t}` has no method `{name}`"),
                                range,
                            );
                        }
                        Resolved::Error
                    }
                    Err(true) => Resolved::Error,
                };
                Some((r, name, range))
            }
            ast::Expr::NameRef(n) => {
                let tok = n.token()?;
                if let Some(args) = n.type_args() {
                    return Some(self.type_args_in_expr(
                        &tok,
                        &args,
                        generics::GenericScope::Visible,
                    ));
                }
                let range = tok.text_range();
                if n.is_string_keyword() {
                    return Some((Resolved::Type(Ty::String), "string".into(), range));
                }
                let name = self.text(&tok).to_string();
                let r = self.resolve_ident(&name, range);
                Some((r, name, range))
            }
            ast::Expr::Field(f) => {
                if let (Some(field), Some(args)) = (f.field(), f.type_args())
                    && let Some(chain) = f.base().as_ref().and_then(name_chain)
                    && let Some((u, n)) = self.unit_prefix(&chain)
                    && n == chain.len()
                {
                    // `Unit.TBox<Integer>`, `Unit.Max<Integer>` (`docs/spec/generics.md` 1.4)
                    let found =
                        self.type_args_in_expr(&field, &args, generics::GenericScope::Unit(u));
                    // A call is picked at the start of the whole name, the unit's name
                    if let Some(set) = self.explicit_sets.get(&field.text_range().start()).cloned()
                    {
                        let start = ast::trimmed_range(f.syntax()).start();
                        self.explicit_sets.insert(start, set);
                    }
                    return Some(found);
                }
                if let (Some(field), Some(args)) = (f.field(), f.type_args())
                    && let Some(chain) = f.base().as_ref().and_then(name_chain)
                    && let Some(u) = {
                        let text: Vec<String> =
                            chain.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
                        self.builtin_unit_named(&text.join("."))
                    }
                {
                    // `System.TFunc<Integer>`, `System.TArray<Integer>` (`docs/spec/generics.md`
                    // 6.2)
                    return Some(self.type_args_in_expr(
                        &field,
                        &args,
                        generics::GenericScope::Unit(u),
                    ));
                }
                if let (Some(field), Some(args)) = (f.field(), f.type_args())
                    && let Some(chain) = f.base().as_ref().and_then(name_chain)
                    && let Some(c) = {
                        let text: Vec<&str> = chain.iter().map(|(n, _)| n.as_str()).collect();
                        let text = text.join(".");
                        self.dry_run(|s| s.class_named(&text))
                    }
                    && !self
                        .generic_types_named(
                            &field.text().to_ascii_lowercase(),
                            &generics::GenericScope::Type(c),
                        )
                        .is_empty()
                {
                    // A generic type nested in a class (`THolder.TBox<Integer>`;
                    // `docs/spec/generics.md` 2.5)
                    return Some(self.type_args_in_expr(
                        &field,
                        &args,
                        generics::GenericScope::Type(c),
                    ));
                }
                if !skip_type_args && let (Some(_), Some(args)) = (f.field(), f.type_args()) {
                    // A generic method (`TUtil.Max<Double>`, `B.Map<string>`;
                    // `docs/spec/generics.md` 1.4): the member found without its type arguments
                    let (r, name, range) = self.resolve_name_expr_with(e, true)?;
                    let whole = TextRange::new(range.start(), args.syntax().text_range().end());
                    let r = self.routine_type_args(r, &name, whole, &args);
                    return Some((r, name, range));
                }
                // A name qualified with a package unit (`docs/spec/units.md` 3.2)
                if let Some(chain) = name_chain(e)
                    && let Some((u, n)) = self.unit_prefix(&chain)
                {
                    if n == chain.len() {
                        let whole = ast::trimmed_range(f.syntax());
                        return Some((Resolved::Unit(u), self.src(whole), whole));
                    }
                    if n == chain.len() - 1 {
                        let (name, range) = chain[n].clone();
                        let r = self.resolve_in_unit(u, &name, range);
                        return Some((r, name, range));
                    }
                }
                // The full name of a unit used with an alias (M1 1.3): E0700 with the alias
                if let Some(chain) = name_chain(e)
                    && let Some((alias, n)) = self.aliased_full_name(&chain)
                {
                    let whole = TextRange::new(chain[0].1.start(), chain[n - 1].1.end());
                    let text = self.src(whole);
                    let d = Diagnostic::error(
                        Code::E(700),
                        format!("undeclared identifier `{text}`"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(whole), "")
                    .with_fix(Fix::safe(
                        format!("replace with `{alias}`"),
                        vec![TextEdit::replace(self.input.file, whole, alias)],
                    ));
                    self.push(d);
                    let (name, range) = chain.last().cloned().expect("a chain has names");
                    return Some((Resolved::Error, name, range));
                }
                let base = f.base()?;
                let field = f.field()?;
                let field_name = self.text(&field).to_string();
                let field_range = field.text_range();
                // `TFoo.ClassType.ClassName`: a value made from a class's name is a receiver too
                let value_chain = self.is_value_chain(&base);
                let base_ty = if value_chain || matches!(base, ast::Expr::Field(_)) {
                    self.trial_type(&base)
                } else {
                    Ty::Error
                };
                if let Ty::Param(_) = base_ty {
                    // A member of a value of a type parameter, such as `T.Create.ClassName`
                    // (`docs/spec/generics.md` 3.3)
                    let whole = ast::trimmed_range(f.syntax());
                    let (b, declared) = self.lower_value_declared(&base);
                    let r = self.value_member(b, declared, &field_name, field_range, whole);
                    return Some((r, field_name, field_range));
                }
                if matches!(base_ty, Ty::Class(_) | Ty::Intf(_) | Ty::ClassRef(_)) {
                    // A member of an instance (`O.M(…)`), of an interface's value (`I.M(…)`;
                    // `docs/spec/objects.md` 7.5) or of a class reference (`C.M(…)`; 2.3, 2.8)
                    let b = self.lower_expr(&base);
                    let class = match b.ty {
                        Ty::Class(c) | Ty::Intf(c) => c,
                        t @ Ty::ClassRef(_) => match self.types.class_ref_target(t) {
                            Some(c) => c,
                            None => return Some((Resolved::Error, field_name, field_range)),
                        },
                        _ => return Some((Resolved::Error, field_name, field_range)),
                    };
                    if let Ty::ClassRef(_) = b.ty
                        && let Ok(methods::Member::Static(owner, st)) =
                            self.dry_run(|s| s.find_member(class, &field_name, field_range))
                    {
                        return Some((self.static_resolved(owner, &st), field_name, field_range));
                    }
                    let r = match self.find_member(class, &field_name, field_range) {
                        Ok(member) => Resolved::Member {
                            recv: Box::new(b),
                            class,
                            member,
                            inherited: false,
                        },
                        Err(true) => Resolved::Error,
                        Err(false) => {
                            // The helpers of the class (`docs/spec/objects.md` 10.4)
                            let found = if matches!(b.ty, Ty::Intf(_)) {
                                None
                            } else {
                                self.helper_resolved(b, &field_name, field_range)
                            };
                            match found {
                                Some(r) => r,
                                None => {
                                    self.member_missing(class, &field_name, field_range);
                                    Resolved::Error
                                }
                            }
                        }
                    };
                    return Some((r, field_name, field_range));
                }
                if let Ty::Record(rec) = base_ty
                    && (!value_chain || self.record_has_member(rec, &field_name, field_range))
                {
                    // A method, property or class member of a record value (`P.Offset(1, 2)`;
                    // `docs/spec/objects.md` chapter 6)
                    let b = self.lower_expr(&base);
                    let r = match self.dry_run(|s| s.find_member(rec, &field_name, field_range)) {
                        Err(false) => {
                            // The helpers of the record (`docs/spec/objects.md` 10.4)
                            let whole = ast::trimmed_range(f.syntax());
                            let ty = b.ty;
                            self.value_member(b, ty, &field_name, field_range, whole)
                        }
                        _ => match self.find_member(rec, &field_name, field_range) {
                            Ok(methods::Member::Static(owner, st)) => {
                                self.static_resolved(owner, &st)
                            }
                            Ok(member) => Resolved::Member {
                                recv: Box::new(b),
                                class: rec,
                                member,
                                inherited: false,
                            },
                            Err(_) => Resolved::Error,
                        },
                    };
                    return Some((r, field_name, field_range));
                }
                if !matches!(base, ast::Expr::NameRef(_) | ast::Expr::Field(_)) {
                    // `.` on a value that is not a name (literal, parentheses, call): its type's
                    // helpers (`docs/spec/objects.md` 10.4)
                    let whole = ast::trimmed_range(f.syntax());
                    let (b, declared) = self.lower_value_declared(&base);
                    let r = self.value_member(b, declared, &field_name, field_range, whole);
                    return Some((r, field_name, field_range));
                }
                let (base_r, base_name, base_range) = self.resolve_name_expr(&base)?;
                match base_r {
                    Resolved::Unit(UnitRef::System)
                        if field_name.eq_ignore_ascii_case("sysutils") =>
                    {
                        // System.SysUtils.X
                        match &self.sysutils {
                            SysUtils::Used => Some((
                                Resolved::Unit(UnitRef::SysUtils),
                                "System.SysUtils".into(),
                                ast::trimmed_range(f.syntax()),
                            )),
                            SysUtils::Alias(a) => {
                                let alias = a.clone();
                                let whole = ast::trimmed_range(f.syntax());
                                let d = Diagnostic::error(
                                    Code::E(700),
                                    "undeclared identifier `System.SysUtils`",
                                    Stage::Sema,
                                )
                                .with_primary(self.span(whole), "")
                                .with_fix(Fix::safe(
                                    format!("replace with `{alias}`"),
                                    vec![TextEdit::replace(self.input.file, whole, alias.clone())],
                                ));
                                self.push(d);
                                Some((Resolved::Error, base_name, whole))
                            }
                            SysUtils::NotUsed => {
                                let whole = ast::trimmed_range(f.syntax());
                                let mut d = Diagnostic::error(
                                    Code::E(700),
                                    "undeclared identifier `System.SysUtils`",
                                    Stage::Sema,
                                )
                                .with_primary(self.span(whole), "");
                                if let Some(fix) = self.uses_sysutils_fix() {
                                    d = d.with_fix(fix);
                                }
                                self.push(d);
                                Some((Resolved::Error, base_name, whole))
                            }
                        }
                    }
                    Resolved::Unit(u) => {
                        let r = self.resolve_in_unit(u, &field_name, field_range);
                        Some((r, field_name, field_range))
                    }
                    Resolved::Error => Some((Resolved::Error, field_name, field_range)),
                    // `TFoo.ClassType.ClassName` where `TFoo.ClassType` failed: reported there
                    Resolved::Member {
                        recv,
                        class,
                        member,
                        inherited,
                    } => {
                        let mut v = self.member_expr(
                            *recv, class, member, inherited, None, true, base_range, base_range,
                        );
                        let declared = v.ty;
                        if let Ty::Subrange(_) = v.ty {
                            v.ty = self.types.base(v.ty);
                        }
                        // `.` on a value that is not a class: its type's helpers
                        // (`docs/spec/objects.md` 10.4)
                        let whole = TextRange::new(base_range.start(), field_range.end());
                        let r = self.value_member(v, declared, &field_name, field_range, whole);
                        Some((r, field_name, field_range))
                    }
                    Resolved::Type(ty @ Ty::Enum(_)) => {
                        // `TColor.Red` (`docs/spec/types.md` 8.1)
                        let found = self
                            .types
                            .enum_values(ty)
                            .iter()
                            .find(|v| v.name.eq_ignore_ascii_case(&field_name))
                            .map(|v| (v.ord, v.span));
                        let r = match found {
                            Some((ord, span)) => Resolved::Const {
                                value: Const::Int(ord),
                                ty,
                                span: Some(self.type_decl_span(ty, span)),
                            },
                            None => {
                                let t = self.types.name(ty);
                                self.error(
                                    Code::E(700),
                                    format!("`{t}` has no member `{field_name}`"),
                                    field_range,
                                );
                                Resolved::Error
                            }
                        };
                        Some((r, field_name, field_range))
                    }
                    Resolved::Type(Ty::Class(id))
                        if field_name.eq_ignore_ascii_case("RaiseOuterException")
                            && self.types.is_exception_class(id) =>
                    {
                        // `docs/spec/exceptions.md` 5.4
                        Some((Resolved::RaiseOuter, field_name, field_range))
                    }
                    Resolved::Type(Ty::Class(id)) => {
                        // `TFoo.Create` (`docs/spec/objects.md` 3.1): the nearest method of the
                        // name is a constructor
                        let ctor = self
                            .methods_named(id, &field_name)
                            .first()
                            .is_some_and(|(_, m)| m.kind == types::MethodKind::Constructor);
                        if ctor {
                            return Some((
                                Resolved::ClassCreate(id, field_name.clone()),
                                field_name,
                                field_range,
                            ));
                        }
                        // `TFoo.Count`, `TFoo.Make`, `TFoo.ClassName` (2.2, 2.3, chapter 5): a
                        // member through the class as a value
                        let r = match self.find_member(id, &field_name, field_range) {
                            Ok(methods::Member::Static(owner, st)) => {
                                self.static_resolved(owner, &st)
                            }
                            Ok(member) => {
                                let recv = self.class_ref_value(id, base_range);
                                Resolved::Member {
                                    recv: Box::new(recv),
                                    class: id,
                                    member,
                                    inherited: false,
                                }
                            }
                            Err(true) => Resolved::Error,
                            Err(false) => {
                                // A class member of the class's helpers (`docs/spec/objects.md`
                                // 10.4)
                                let recv = self.class_ref_value(id, base_range);
                                match self.helper_resolved(recv, &field_name, field_range) {
                                    Some(r) => r,
                                    None => {
                                        let t = self.types.name(Ty::Class(id));
                                        self.class_member_missing(&t, &field_name, field_range);
                                        Resolved::Error
                                    }
                                }
                            }
                        };
                        Some((r, field_name, field_range))
                    }
                    Resolved::Type(Ty::Param(p)) if field_name.eq_ignore_ascii_case("create") => {
                        // `T.Create` (`docs/spec/generics.md` 3.3, 4.3)
                        if self
                            .types
                            .constraints(p)
                            .contains(&types::Constraint::Constructor)
                        {
                            return Some((
                                Resolved::ClassCreate(p, field_name.clone()),
                                field_name,
                                field_range,
                            ));
                        }
                        let whole = TextRange::new(base_range.start(), field_range.end());
                        self.param_misuse(p, generics::ParamOp::Create, whole);
                        Some((Resolved::Error, field_name, field_range))
                    }
                    Resolved::Type(Ty::Helper(h)) => {
                        // `H.M(X)`, `H.Count` (`docs/spec/objects.md` 10.2, 10.5)
                        let r = self.helper_type_member(h, &field_name, base_range, field_range);
                        Some((r, field_name, field_range))
                    }
                    Resolved::Type(Ty::Record(id)) if self.types.get(id).name.is_some() => {
                        // `TPoint.Create(…)`, `TPoint.Make(…)`, `TPoint.Count`
                        // (`docs/spec/objects.md` chapter 6)
                        let r = match self.find_member(id, &field_name, field_range) {
                            Ok(methods::Member::Static(owner, st)) => {
                                self.static_resolved(owner, &st)
                            }
                            Ok(member) => Resolved::Member {
                                recv: Box::new(Self::record_type_receiver(id, base_range)),
                                class: id,
                                member,
                                inherited: false,
                            },
                            Err(true) => Resolved::Error,
                            Err(false) => {
                                // A class member of the record's helpers (`docs/spec/objects.md`
                                // 10.4)
                                let recv = Self::record_type_receiver(id, base_range);
                                match self.helper_resolved(recv, &field_name, field_range) {
                                    Some(r) => r,
                                    None => {
                                        let t = self.types.name(Ty::Record(id));
                                        self.error(
                                            Code::E(700),
                                            format!("`{t}` has no member `{field_name}`"),
                                            field_range,
                                        );
                                        Resolved::Error
                                    }
                                }
                            }
                        };
                        Some((r, field_name, field_range))
                    }
                    Resolved::Var(_) | Resolved::Global(_)
                        if {
                            let t = match base_r {
                                Resolved::Var(id) => self.var(id).ty,
                                Resolved::Global(id) => self.globals[id.0 as usize].ty,
                                _ => Ty::Error,
                            };
                            t.is_procedural()
                        } =>
                    {
                        // A procedural value has no members; `Invoke` of an anonymous method
                        // type is not a method in Puy (`docs/spec/objects.md` 9.10)
                        let t = match base_r {
                            Resolved::Var(id) => self.var(id).ty,
                            Resolved::Global(id) => self.globals[id.0 as usize].ty,
                            _ => Ty::Error,
                        };
                        let tn = self.tn(t);
                        let d = self.error(
                            Code::E(700),
                            format!("`{tn}` has no member `{field_name}`"),
                            field_range,
                        );
                        if field_name.eq_ignore_ascii_case("invoke") {
                            d.notes.push(format!("call the value as `{base_name}()`"));
                        }
                        Some((Resolved::Error, field_name, field_range))
                    }
                    Resolved::Var(_)
                    | Resolved::Global(_)
                    | Resolved::Const { .. }
                    | Resolved::Bool(_)
                    | Resolved::Func(_)
                    | Resolved::Key(..) => {
                        // `.` on a value (a built-in constant such as `True`, or a built-in
                        // function called without parentheses such as `Pi`): its type's helpers
                        // (`docs/spec/objects.md` 10.4)
                        let whole = TextRange::new(base_range.start(), field_range.end());
                        let (b, declared) = self.lower_value_declared(&base);
                        let r = self.value_member(b, declared, &field_name, field_range, whole);
                        Some((r, field_name, field_range))
                    }
                    Resolved::Type(t @ Ty::DynArray(_))
                        if field_name.eq_ignore_ascii_case("create")
                            && matches!(&base, ast::Expr::NameRef(n) if n.type_args().is_some()) =>
                    {
                        // `TArray<Integer>.Create(…)` is not in the language
                        // (`docs/spec/generics.md` 1.4)
                        let tn = self.tn(t);
                        self.error(
                            Code::E(700),
                            format!("`{tn}` has no member `{field_name}`"),
                            field_range,
                        )
                        .notes
                        .push("use an array constructor, as in `[1, 2]`".into());
                        Some((Resolved::Error, field_name, field_range))
                    }
                    _ => {
                        // `.` on a type of the language or another name that has no members
                        let whole = TextRange::new(base_range.start(), field_range.end());
                        let d = Diagnostic::error(
                            Code::E(3200),
                            "type helpers for built-in types are not supported yet (planned for M5)",
                            Stage::Sema,
                        )
                        .with_primary(self.span(whole), "");
                        self.push(d);
                        Some((Resolved::Error, field_name, field_range))
                    }
                }
            }
            _ => None,
        }
    }

    /// A name a class does not have as a field (`docs/spec/objects.md` 2.10, chapter 5): a member
    /// of `TObject` that a later item implements is E3200, one Puy does not have is the E0700
    /// variant `` `TObject` has no member `…` ``, anything else `` `TFoo` has no member `…` ``.
    /// E0700 for a member a class or an interface does not have (E3017 for `_AddRef`,
    /// `_Release` and `QueryInterface`; `docs/spec/objects.md` 7.2).
    pub(super) fn member_missing(&mut self, class: types::TypeId, name: &str, range: TextRange) {
        if !self.types.is_interface(class) || interfaces::is_ref_count_method(name) {
            let t = self.types.name(Ty::Class(class));
            self.class_member_missing(&t, name, range);
            return;
        }
        // `docs/spec/objects.md` 7.4: only the interface's methods and properties
        let t = self.types.name(Ty::Intf(class));
        let tobject = methods::is_class_builtin(name)
            || self
                .types
                .class_methods(types::TOBJECT)
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case(name));
        let d = self.error(Code::E(700), format!("`{t}` has no member `{name}`"), range);
        if tobject {
            d.notes
                .push("convert the interface to a class with `as TObject` to use it".into());
        }
    }

    /// The E3009 of `message` methods and of `TObject`'s `Dispatch` / `DefaultHandler`
    /// (`docs/spec/objects.md` 2.10).
    pub(super) fn message_unsupported(&mut self, name: &str, range: TextRange) {
        self.error(
            Code::E(3009),
            format!("`{name}` is not supported in Puy"),
            range,
        )
        .notes
        .push("use events or channels instead".into());
    }

    pub(super) fn class_member_missing(&mut self, class: &str, name: &str, range: TextRange) {
        // Only a class has `TObject`'s `Dispatch` / `DefaultHandler` (`docs/spec/objects.md` 2.10)
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "dispatch" | "defaulthandler"
        ) {
            self.message_unsupported(name, range);
            return;
        }
        self.type_member_missing(class, name, range);
    }

    /// [`Self::class_member_missing`] without the members of `TObject` that Puy does not support
    /// (a record's missing member; `docs/spec/objects.md` chapter 6).
    pub(super) fn type_member_missing(&mut self, class: &str, name: &str, range: TextRange) {
        if interfaces::is_ref_count_method(name) {
            // `docs/spec/objects.md` 7.2
            self.error(
                Code::E(3017),
                format!("`{name}` is not supported in Puy"),
                range,
            )
            .notes
            .push(interfaces::REF_COUNT_NOTE.into());
            return;
        }
        let lower = name.to_ascii_lowercase();
        let planned = match lower.as_str() {
            "unitname" | "qualifiedclassname" => Some("M5"),
            _ => None,
        };
        if let Some(m) = planned {
            self.error(
                Code::E(3200),
                format!("`{class}.{name}` is not supported yet (planned for {m})"),
                range,
            );
            return;
        }
        let removed = matches!(
            lower.as_str(),
            "newinstance"
                | "freeinstance"
                | "initinstance"
                | "cleanupinstance"
                | "instancesize"
                | "fieldaddress"
                | "methodaddress"
                | "methodname"
                | "classinfo"
                | "getinterfacetable"
                | "safecallexception"
                | "disposeof"
        );
        let owner = if removed { "TObject" } else { class };
        self.error(
            Code::E(700),
            format!("`{owner}` has no member `{name}`"),
            range,
        );
    }

    /// Records a use of a variable (counts as a read even inside an error-typed expression; W0001).
    fn mark_used(&mut self, id: VarId, at: TextRange) {
        if self.dry_run > 0 {
            return;
        }
        if let Some(c) = &mut self.cur
            && !c.used_vars.contains(&(id, at))
        {
            c.used_vars.push((id, at));
        }
        // A read by a nested routine is a read of the enclosing routine's variable (W0001, W0017)
        let (up, var) = self.var_origin(id);
        if up > 0 {
            let n = self.outer.len();
            self.outer[n - up as usize].vars[var.0 as usize].read_by_nested = true;
        }
    }
}

/// Range of a directive and the `;` after it (for the removal fix).
/// The range of a calling convention of a procedural type with the `;` before it
/// (`procedure; cdecl`; `docs/spec/objects.md` 9.1).
fn semicolon_with_directive(d: &SyntaxNode) -> TextRange {
    let end = d.text_range().end();
    let mut start = d.text_range().start();
    let mut prev = d.prev_sibling_or_token();
    while let Some(p) = prev {
        match p.kind() {
            k if k.is_trivia() => {}
            SyntaxKind::SEMICOLON => {
                start = p.text_range().start();
                break;
            }
            _ => break,
        }
        prev = p.prev_sibling_or_token();
    }
    TextRange::new(start, end)
}

fn directive_with_semicolon(d: &SyntaxNode) -> TextRange {
    let start = d.text_range().start();
    let mut end = d.text_range().end();
    let mut next = d.next_sibling_or_token();
    while let Some(n) = next {
        match n.kind() {
            k if k.is_trivia() => {}
            SyntaxKind::SEMICOLON => {
                end = n.text_range().end();
                break;
            }
            _ => break,
        }
        next = n.next_sibling_or_token();
    }
    TextRange::new(start, end)
}

/// Whether the node's range contains a syntax error (an `ERROR` node or a diagnostic from the parser).
fn has_syntax_error(node: &SyntaxNode, sink: &DiagnosticSink, file: FileId) -> bool {
    if node
        .descendants()
        .any(|n| n.kind() == SyntaxKind::ERROR_NODE)
    {
        return true;
    }
    let range = node.text_range();
    sink.iter().any(|d| {
        d.stage == Stage::Parser
            && d.is_error()
            && d.primary.as_ref().is_some_and(|p| {
                p.span.file == file && range.contains_inclusive(p.span.range.start())
            })
    })
}

/// Whether the script's `<main>` range (statements, `VAR_SECTION`, `INLINE_VAR_DECL`, `ERROR`, item separators)
/// contains a syntax error (4.4).
fn script_main_has_syntax_error(script: &ast::Script, sink: &DiagnosticSink, file: FileId) -> bool {
    use SyntaxKind::*;
    let mut ranges: Vec<TextRange> = Vec::new();
    for child in script.syntax().children() {
        match child.kind() {
            USES_CLAUSE | CONST_SECTION | RESOURCESTRING_SECTION | ROUTINE_DECL => {}
            ERROR_NODE => return true,
            _ => {
                if child.descendants().any(|n| n.kind() == ERROR_NODE) {
                    return true;
                }
                ranges.push(child.text_range());
            }
        }
    }
    // Diagnostics at item separators (a supplied `;`) are also in the <main> range
    let excluded: Vec<TextRange> = script
        .syntax()
        .children()
        .filter(|c| {
            matches!(
                c.kind(),
                USES_CLAUSE | CONST_SECTION | RESOURCESTRING_SECTION | ROUTINE_DECL
            )
        })
        .map(|c| c.text_range())
        .collect();
    sink.iter().any(|d| {
        d.stage == Stage::Parser
            && d.is_error()
            && d.primary.as_ref().is_some_and(|p| {
                p.span.file == file
                    && !excluded.iter().any(|r| {
                        r.contains_inclusive(p.span.range.start())
                            && r.start() != p.span.range.start()
                    })
                    && (ranges
                        .iter()
                        .any(|r| r.contains_inclusive(p.span.range.start()))
                        || !excluded
                            .iter()
                            .any(|r| r.contains_inclusive(p.span.range.start())))
            })
    })
}

#[cfg(test)]
mod tests;

/// The constant value of an initial value or a typed constant: a constant expression, or a
/// dynamic array constructor of constants (`docs/spec/types.md` chapter 4), whose value is the
/// list of its elements (`Const::Agg` with a dynamic array type; a new array each time it is read).
pub(crate) fn const_init_value(e: &hir::Expr) -> Option<Const> {
    if let Some(v) = &e.value {
        return Some(v.clone());
    }
    match (&e.kind, e.ty) {
        (hir::ExprKind::ArrayCtor(elems), Ty::DynArray(_)) => elems
            .iter()
            .map(const_init_value)
            .collect::<Option<Vec<_>>>()
            .map(Const::Agg),
        _ => None,
    }
}
