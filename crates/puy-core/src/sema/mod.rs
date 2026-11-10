//! Semantic analysis (`docs/spec/semantics.md`): name resolution, type checking, constant folding, definite assignment. Produces HIR.

pub mod builtins;
pub mod dump;
mod expr;
mod flow;
pub mod hir;
mod overload;
mod reals;
mod sets;
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
pub use expr::compare_strings;
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
    exports: Vec<(String, GSym)>,
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
    /// The analyzed units, in analysis order (dependencies first).
    pub units: Vec<UnitInterface>,
    /// The `hir` of each analyzed file, in the same order as `units` (the program last).
    parts: Vec<UnitHir>,
    main: Option<Routine>,
    uses_sysutils: bool,
}

impl ModuleState {
    pub fn new() -> Self {
        Self::default()
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
        last_place_result: None,
        dry_run: 0,
        types: std::mem::take(&mut state.types),
        type_decls: std::mem::take(&mut state.type_decls),
        program_name: String::new(),
        pending_aliases: Vec::new(),
        overload_picks: std::collections::HashMap::new(),
        unit_index,
        units: std::mem::take(&mut state.units),
        used_units: Vec::new(),
        in_interface_part: false,
        is_unit: false,
        sysutils_item: None,
    };
    s.run(state);
}

/// Builds the program from the analyzed files (`analyze` was called for each, the file passed to
/// `puy` last).
pub fn finish(mut state: ModuleState) -> Program {
    // Overloads are `Name`, `Name#2`, `Name#3` in declaration order within a scope
    // (`semantics.md` 4.7), and a nested routine is named after the routines enclosing it
    // (`Outer.Inner`; 4.9). A routine comes after the routine it is nested in
    let mut seen: Vec<(u32, Option<RoutineId>, String)> = Vec::new();
    let mut names: Vec<String> = Vec::with_capacity(state.routines.len());
    for r in &state.routines {
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
        names.push(name);
    }
    let links = static_links(&state.routines);
    let routines: Vec<Routine> = state
        .routines
        .drain(..)
        .zip(names)
        .zip(links)
        .map(|((r, full), link)| {
            let mut routine = r.hir.unwrap_or_else(|| Routine {
                name: r.name.clone(),
                vars: r
                    .params
                    .iter()
                    .map(|p| Var {
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
                    })
                    .collect(),
                param_count: r.params.len() as u32,
                ret: r.ret,
                body: Vec::new(),
                end_span: r.name_span,
                body_begin: None,
                name_span: r.name_span,
                has_syntax_errors: false,
                has_errors: true,
                unit: r.unit,
                parent: r.parent,
                link: false,
            });
            routine.name = full;
            routine.parent = r.parent;
            routine.link = link;
            routine
        })
        .collect();
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
    });
    Program {
        name,
        globals: state.globals,
        exit_code: state.exit_code,
        routines,
        main,
        init,
        uses_sysutils: state.uses_sysutils,
        types: state.types,
        type_decls: state.type_decls,
        units,
    }
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
            if parent(r).is_none() {
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
                if parent(ch) == Some(r) {
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
            let tok = n.token()?;
            Some(vec![(tok.text().to_string(), tok.text_range())])
        }
        ast::Expr::Field(f) => {
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
struct RoutineInfo {
    name: String,
    params: Vec<ParamInfo>,
    ret: Option<Ty>,
    /// Has a body (`false` if only `forward`).
    has_body: bool,
    /// Position of the name in the forward declaration. A body-less declaration without `forward` (for which the parser
    /// reported E0411) is also treated as a forward declaration and tied to a later body (so E0701 does not cascade).
    forward_span: Option<TextRange>,
    /// A declaration with neither `forward` nor a body (E0411). Even without a body, E0723 is not reported on top of it.
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
}

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
    Unit(UnitRef),
    /// `T.Create` of an exception class (`docs/spec/exceptions.md` 5.6).
    ExcCreate(types::TypeId),
    /// `T.CreateFmt` (`docs/spec/exceptions.md` 2.1).
    ExcCreateFmt(types::TypeId),
    /// `Exception.RaiseOuterException` (`docs/spec/exceptions.md` 5.4).
    RaiseOuter,
    /// `Input` / `Output` / `ErrOutput` (`docs/spec/semantics.md` 6.1).
    StdFile(builtins::StdFile),
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
    fn unit_file(&self, unit: u32) -> FileId {
        if unit == self.unit_index {
            self.input.file
        } else {
            self.units[unit as usize].file
        }
    }

    /// The name in a routine's (first) heading, in the routine's file.
    pub(super) fn routine_decl_span(&self, id: RoutineId) -> Span {
        let r = &self.routines[id.0 as usize];
        Span::new(r.file, r.name_span)
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
            for u in &self.used_units {
                if let Some(i) = u.iface {
                    v.push(self.units[i].name.clone());
                }
            }
            v
        };
        let is_program = part.main.is_some();
        // Put the shared state back
        state.globals = std::mem::take(&mut self.globals);
        state.exit_code = self.exit_code;
        state.replace_flags = self.replace_flags;
        state.case_mapping = self.case_mapping;
        state.case_fold = self.case_fold;
        state.routines = std::mem::take(&mut self.routines);
        state.types = std::mem::take(&mut self.types);
        state.type_decls = std::mem::take(&mut self.type_decls);
        state.units = std::mem::take(&mut self.units);
        if is_program {
            state.uses_sysutils = self.sysutils != SysUtils::NotUsed;
            state.main = part.main;
        }
        state.units.push(UnitInterface {
            name: part.name.clone(),
            file: self.input.file,
            exports: part.exports,
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
    /// `library` are W0010, `deprecated` and `experimental` E3200 until M3.
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
                _ => {
                    self.error(
                        Code::E(3200),
                        format!("`{text}` is not supported yet (planned for M3)"),
                        range,
                    );
                }
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
                            self.used_units.push(UsedUnit {
                                lower: self.units[i].name.to_ascii_lowercase(),
                                alias: alias.as_ref().map(|a| a.to_ascii_lowercase()),
                                qualifier: alias.clone().unwrap_or_else(|| text.clone()),
                                iface: Some(i),
                                in_interface,
                                item: range,
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
            d.notes.push(
                "only `System.SysUtils` can be used in this release; more units of the standard library come with later releases".into(),
            );
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
        if matches!(
            lower,
            "result" | "exit" | "break" | "continue" | "self" | "true" | "false"
        ) {
            let name = self.src(range);
            let d = Diagnostic::error(
                Code::E(701),
                format!("`{name}` is a reserved name"),
                Stage::Sema,
            )
            .with_primary(self.span(range), "");
            self.push(d);
            return true;
        }
        false
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
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let (value, ty) = self.eval_const_decl(decl.type_ref(), decl.value(), resource);
            if self.check_duplicate(&lower, range) {
                continue;
            }
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
            let ty = self.resolve_type(decl.type_ref());
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
                    ty,
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
        }
    }

    fn declare_script_var_decl(&mut self, decl: &ast::VarDecl, body: &mut Vec<Stmt>) {
        let names = decl.names();
        let ty = self.resolve_type(decl.type_ref());
        let span = decl.syntax().text_range();
        let checks = self.checks_at(span.start());
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
            self.script_top_vars.push((lower.clone(), range));
            let id = self.add_var(name, ty, VarKind::Local, range);
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
        let Some(qname) = header.name() else { return };
        let name_text = qname.text();
        let name_range = qname.syntax().text_range();
        let is_function = header.is_function();
        // Parameter and return types are resolved at the point of declaration
        let mut params: Vec<ParamInfo> = Vec::new();
        let mut seen_default_error = false;
        for group in header.params() {
            let ty = match group.type_ref() {
                // `array of T` in a parameter is an open array (`docs/spec/types.md` 2.4)
                Some(ast::TypeSpec::Array(a)) if !a.has_brackets() => {
                    let t = self.open_array_of(&a);
                    if group.mode() == ast::ParamMode::Out && !t.is_error() {
                        let r = ast::trimmed_range(a.syntax());
                        self.error(
                            Code::E(3200),
                            "`out` open array parameters are not supported yet",
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
                });
            }
        }
        let ret = if is_function {
            Some(match header.return_type() {
                Some(t) => self.resolve_type(Some(t)),
                None => Ty::Error,
            })
        } else {
            None
        };
        if name_text.contains('.') {
            self.error(
                Code::E(3200),
                "methods are not supported yet (planned for M3)",
                name_range,
            );
            return;
        }
        let lower = name_text.to_ascii_lowercase();
        // Directives
        let mut has_forward = false;
        for d in decl.directives() {
            let Some(kw) = d.keyword() else { continue };
            let text = self.text(&kw).to_string();
            let range = kw.text_range();
            match d.kind() {
                Some(SyntaxKind::FORWARD_KW) if self.in_interface_part => {
                    // `docs/spec/units.md` 4.2: a heading in the `interface` section is already
                    // completed by a body in the `implementation` section
                    let del = crate::diagnostics::deletion_range(
                        self.input.bytes,
                        directive_with_semicolon(&d),
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
                Some(SyntaxKind::INLINE_KW) => {}
                Some(SyntaxKind::PLATFORM_KW) | Some(SyntaxKind::LIBRARY_KW) => {
                    let del = crate::diagnostics::deletion_range(
                        self.input.bytes,
                        directive_with_semicolon(&d),
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
                Some(SyntaxKind::ASM_KW) => {
                    self.error(Code::E(3008), "`assembler` is not supported in Puy", range)
                        .notes
                        .push("Puy runs on a virtual machine and cannot execute assembly".into());
                }
                Some(SyntaxKind::SAFECALL_KW) => {
                    self.error(Code::E(3010), "`safecall` is not supported in Puy", range)
                        .notes
                        .push("Puy does not support COM".into());
                }
                Some(SyntaxKind::EXTERNAL_KW) => {
                    self.error(Code::E(3011), "`external` is not supported in Puy", range)
                        .notes
                        .push(
                            "Puy has no DLLs; extend it with the Rust extension mechanism instead"
                                .into(),
                        );
                }
                // `docs/spec/semantics.md` 4.7 (checked with the redeclaration)
                Some(SyntaxKind::OVERLOAD_KW) => {}
                Some(SyntaxKind::DEPRECATED_KW) | Some(SyntaxKind::EXPERIMENTAL_KW) => {
                    self.error(
                        Code::E(3200),
                        format!("`{text}` is not supported yet (planned for M2)"),
                        range,
                    );
                }
                Some(SyntaxKind::VARARGS_KW) => {
                    let del = crate::diagnostics::deletion_range(
                        self.input.bytes,
                        directive_with_semicolon(&d),
                    );
                    let dg = Diagnostic::warning(Code::W(9), "`varargs` is ignored", Stage::Sema)
                        .with_primary(self.span(range), "")
                        .with_fix(Fix::safe(
                            "remove `varargs`",
                            vec![TextEdit::delete(self.input.file, del)],
                        ));
                    self.push(dg);
                }
                Some(_) => {
                    let del = crate::diagnostics::deletion_range(
                        self.input.bytes,
                        directive_with_semicolon(&d),
                    );
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
                // Body of a forward routine: does the heading match?
                self.check_body_matches_forward(id, &name_text, name_range, &params, ret);
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
                });
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
        if has_forward {
            return;
        }
        let Some(body) = body else {
            // No body (the parser has reported E0411)
            return;
        };
        self.routines[id.0 as usize].has_body = true;
        let _ = in_script;
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
        }
        // Parameters. Their positions are those of the body's heading (W0017 is reported there,
        // not at a `forward` declaration; `docs/spec/semantics.md` 4.6)
        let heading = params;
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
        self.cur_mut().vars.truncate(params.len());
        // Result
        if let Some(rt) = ret {
            let vid = self.add_var("Result".into(), rt, VarKind::Result, name_range);
            debug_assert_eq!(vid.0 as usize, params.len());
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
        let routine = self.end_routine(stmts, end_span);
        self.cur = self.outer.pop();
        let info = &mut self.routines[id.0 as usize];
        info.hir = Some(routine);
        info.calls = calls;
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
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let (value, ty) = self.eval_const_decl(decl.type_ref(), decl.value(), resource);
            if self.check_duplicate(&lower, range) {
                continue;
            }
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
            let ty = self.resolve_type(decl.type_ref());
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
                let id = self.add_var(name, ty, VarKind::Local, range);
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
        });
    }

    fn end_routine(&mut self, body: Vec<Stmt>, end_span: TextRange) -> Routine {
        let ctx = self.cur.take().unwrap();
        let param_count = ctx
            .vars
            .iter()
            .take_while(|v| matches!(v.kind, VarKind::Param(_)))
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
        };
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
            let effects = match ctx.routine {
                Some(id) => {
                    let group: Vec<(RoutineId, Option<&Routine>, u32)> = (0..self.routines.len())
                        .filter_map(|i| {
                            let up = nesting_distance(&self.routines, RoutineId(i as u32), id)?;
                            (up > 0)
                                .then(|| (RoutineId(i as u32), self.routines[i].hir.as_ref(), up))
                        })
                        .collect();
                    flow::nested_effects(
                        &group,
                        routine.vars.len(),
                        &self.types,
                        self.input.file,
                        self.input.bytes,
                    )
                }
                None => flow::Effects::new(),
            };
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

    fn cur_mut(&mut self) -> &mut RoutineCtx {
        self.cur.as_mut().expect("routine context")
    }

    fn add_var(&mut self, name: String, ty: Ty, kind: VarKind, span: TextRange) -> VarId {
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
                .take_while(|v| matches!(v.kind, VarKind::Param(_)))
                .count() as u32,
        )
    }

    /// The variable `id` of the routine `up` levels out, as a variable of the routine being
    /// analyzed: itself for `up == 0`, otherwise the variable referring to it (made on first use;
    /// `docs/spec/semantics.md` 4.9).
    fn var_at(&mut self, up: u32, id: VarId) -> VarId {
        if up == 0 {
            return id;
        }
        let kind = VarKind::Outer { up, var: id };
        if let Some(i) = self.cur_mut().vars.iter().position(|v| v.kind == kind) {
            return VarId(i as u32);
        }
        let n = self.outer.len();
        let owner = &mut self.outer[n - up as usize].vars[id.0 as usize];
        owner.captured = true;
        let (name, ty, span) = (owner.name.clone(), owner.ty, owner.span);
        self.add_var(name, ty, kind, span)
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
        v.written_by_nested.get_or_insert(at);
    }

    /// The variable an `Outer` variable refers to: (how many routines out, its id), or `(0, id)`.
    fn var_origin(&self, id: VarId) -> (u32, VarId) {
        match self.var(id).kind {
            VarKind::Outer { up, var } => (up, var),
            _ => (0, id),
        }
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
        let lower = name.to_ascii_lowercase();
        if let Some((up, sym)) = self.lookup_scopes(&lower) {
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
        match builtins::lookup_system(&lower) {
            Some(builtins::SystemName::Type(t)) => return Resolved::Type(t),
            Some(builtins::SystemName::True) => return Resolved::Bool(true),
            Some(builtins::SystemName::False) => return Resolved::Bool(false),
            Some(builtins::SystemName::Func(f)) => return Resolved::Func(f),
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

    fn broken_unit_visible(&self) -> bool {
        self.visible_units().any(|(_, u)| u.iface.is_none())
    }

    /// A unit name (or alias) written as a name: a visible package unit, or this unit itself.
    fn unit_named(&self, lower: &str) -> Option<UnitRef> {
        for (k, u) in self.visible_units() {
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
                .find(|(_, u)| u.alias.is_some() && u.lower == full)
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

    /// Whether `chain[at]` names a value (not a type) in the unit `u`.
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
        matches!(sym, Some(GSym::Global(_) | GSym::Const { .. }))
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
                UnitLookup::Found(self.export_resolved(i, sym))
            }
            _ => {
                self.several_units(name, range, &found, in_sysutils);
                UnitLookup::Error
            }
        }
    }

    /// E2502: the name is declared in several units.
    fn several_units(
        &mut self,
        name: &str,
        range: TextRange,
        found: &[(usize, GSym)],
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
        for (k, sym) in found {
            let u = &self.used_units[*k];
            let i = u.iface.expect("found in a unit with a public part");
            let decl = match sym {
                GSym::Global(id) => self.globals[id.0 as usize].span,
                GSym::Const { span, .. } | GSym::Type { span, .. } => Some(*span),
                GSym::Routine(id) => Some(self.routines[id.0 as usize].name_span),
            };
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
                    Some(sym) => self.export_resolved(i, &sym),
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
            UnitRef::System => match builtins::lookup_system(&lower) {
                Some(builtins::SystemName::Type(t)) => Resolved::Type(t),
                Some(builtins::SystemName::True) => Resolved::Bool(true),
                Some(builtins::SystemName::False) => Resolved::Bool(false),
                Some(builtins::SystemName::Func(f)) => Resolved::Func(f),
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

    /// Resolves an expression (a chain of names), reading the chain of `FIELD_EXPR`s as unit qualification.
    /// Returns `(resolution result, name as written, name position)`.
    fn resolve_name_expr(&mut self, e: &ast::Expr) -> Option<(Resolved, String, TextRange)> {
        match e {
            ast::Expr::NameRef(n) => {
                let tok = n.token()?;
                let range = tok.text_range();
                if n.is_string_keyword() {
                    return Some((Resolved::Type(Ty::String), "string".into(), range));
                }
                let name = self.text(&tok).to_string();
                let r = self.resolve_ident(&name, range);
                Some((r, name, range))
            }
            ast::Expr::Field(f) => {
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
                if !matches!(base, ast::Expr::NameRef(_) | ast::Expr::Field(_)) {
                    // `.` on a value that is not a name (literal, parentheses, call) (type helpers come in M3)
                    let whole = ast::trimmed_range(f.syntax());
                    let text = self.src(whole);
                    self.lower_expr(&base);
                    let d = Diagnostic::error(
                        Code::E(3200),
                        format!("`{text}` is not supported yet (planned for M3)"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(whole), "");
                    self.push(d);
                    return Some((Resolved::Error, field_name, field_range));
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
                    Resolved::Type(Ty::Exception(id)) => {
                        // `T.Create`, `Exception.RaiseOuterException` (`docs/spec/exceptions.md` 5.6)
                        let r = match field_name.to_ascii_lowercase().as_str() {
                            "create" => Resolved::ExcCreate(id),
                            "createfmt" => Resolved::ExcCreateFmt(id),
                            "raiseouterexception" => Resolved::RaiseOuter,
                            _ => {
                                let t = self.types.name(Ty::Exception(id));
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
                    _ => {
                        // `.` on a value that is not a unit name (type helpers come in M3)
                        let d = Diagnostic::error(
                            Code::E(3200),
                            format!(
                                "`{base_name}.{field_name}` is not supported yet (planned for M3)"
                            ),
                            Stage::Sema,
                        )
                        .with_primary(
                            self.span(TextRange::new(base_range.start(), field_range.end())),
                            "",
                        );
                        self.push(d);
                        Some((Resolved::Error, field_name, field_range))
                    }
                }
            }
            _ => None,
        }
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
fn directive_with_semicolon(d: &ast::RoutineDirective) -> TextRange {
    let start = d.syntax().text_range().start();
    let mut end = d.syntax().text_range().end();
    let mut next = d.syntax().next_sibling_or_token();
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
