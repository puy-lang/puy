//! Semantic analysis (`docs/spec/semantics.md`): name resolution, type checking, constant folding, definite assignment. Produces HIR.

pub mod builtins;
pub mod dump;
mod expr;
mod flow;
pub mod hir;
mod stmt;
pub mod types;

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Fix, Stage, TextEdit};
use crate::directives::DirectiveOutput;
use crate::source::{FileId, Span, TextRange, TextSize};
use crate::syntax::ast::{self, AstNode, ParamMode};
use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};
pub use expr::compare_strings;
use hir::*;
use types::{Const, Ty};

pub struct SemaInput<'a> {
    pub file: FileId,
    pub root: &'a SyntaxNode,
    pub bytes: &'a [u8],
    pub directives: &'a DirectiveOutput,
    /// Program name of a script (derived from the file name; `semantics.md` 1.1).
    pub script_name: &'a str,
}

/// Runs semantic analysis.
pub fn analyze(input: &SemaInput<'_>, sink: &mut DiagnosticSink) -> Program {
    let s = Sema {
        input,
        sink,
        globals: Vec::new(),
        routines: Vec::new(),
        global_scope: Vec::new(),
        sysutils: SysUtils::NotUsed,
        uses_fix: UsesFix::compute(input.root, input.bytes),
        script_top_vars: Vec::new(),
        script_all_top_vars: Vec::new(),
        is_script: false,
        cur: None,
        allow_type_as_value: false,
        last_place_result: None,
        dry_run: 0,
    };
    s.run()
}

/// State of `uses System.SysUtils`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SysUtils {
    NotUsed,
    Used,
    Alias(String),
}

/// Insertion point for the E0700 fix that adds a `uses` (chapter 5).
struct UsesFix {
    /// Position of the `;` of the first `uses` (or the end of the last item if the `;` was supplied by recovery).
    uses_semicolon: Option<TextSize>,
    /// Insertion point and text to insert when there is no `uses`.
    insert: Option<(TextSize, String)>,
}

impl UsesFix {
    fn compute(root: &SyntaxNode, bytes: &[u8]) -> Self {
        use SyntaxKind::*;
        let Some(container) = root
            .children()
            .find(|n| matches!(n.kind(), PROGRAM | SCRIPT))
        else {
            return UsesFix {
                uses_semicolon: None,
                insert: None,
            };
        };
        let first_non_header = container.children().find(|n| n.kind() != PROGRAM_HEADER);
        if let Some(u) = first_non_header.filter(|n| n.kind() == USES_CLAUSE) {
            let semi = u
                .last_token()
                .filter(|t| t.kind() == SEMICOLON)
                .map(|t| t.text_range().start());
            let pos = semi.unwrap_or_else(|| {
                u.children()
                    .filter(|c| c.kind() == USES_ITEM)
                    .last()
                    .map(|i| i.text_range().end())
                    .unwrap_or(u.text_range().end())
            });
            return UsesFix {
                uses_semicolon: Some(pos),
                insert: None,
            };
        }
        if container.kind() == PROGRAM {
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
            UsesFix {
                uses_semicolon: None,
                insert: Some((pos, "\nuses System.SysUtils;".to_string())),
            }
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
            UsesFix {
                uses_semicolon: None,
                insert: Some(((pos as u32).into(), "uses System.SysUtils;\n".to_string())),
            }
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
    /// Name of a nested routine for which the parser reported E3200 (`grammar.md` 1.3). References to it do not
    /// pile on diagnostics (`semantics.md` chapter 8). `span` is the position of the name.
    Unsupported {
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
}

#[derive(Debug, Clone)]
struct ParamInfo {
    name: String,
    ty: Ty,
    mode: ParamMode,
    span: TextRange,
}

/// Context of the routine being analyzed.
struct RoutineCtx {
    vars: Vec<Var>,
    scopes: Vec<Vec<(String, LSym)>>,
    loop_depth: u32,
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
    /// Do not diagnose a type name written as a value (inside the name arguments of E3200).
    allow_type_as_value: bool,
    /// Set when `lower_place` finds a non-variable for `RefArg` (the caller reports E0706).
    last_place_result: Option<expr::PlaceResult>,
    /// Inside a trial type check for a fix (diagnostics are discarded; no fixes are built).
    dry_run: u32,
}

/// Result of name resolution.
#[derive(Debug, Clone)]
enum Resolved {
    Var(VarId),
    Global(GlobalId),
    Const {
        value: Const,
        ty: Ty,
        span: TextRange,
    },
    Routine(RoutineId),
    Type(Ty),
    Bool(bool),
    Func(Builtin),
    Proc(builtins::BuiltinProc),
    Unit(UnitRef),
    /// A diagnostic was reported (error type).
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitRef {
    System,
    SysUtils,
}

impl<'a, 's> Sema<'a, 's> {
    fn span(&self, range: TextRange) -> Span {
        Span::new(self.input.file, range)
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

    fn run(mut self) -> Program {
        let root = self.input.root.clone();
        let file = ast::SourceFile::cast(root).expect("SOURCE_FILE");
        if let Some(program) = file.program() {
            self.run_program(program)
        } else if let Some(script) = file.script() {
            self.is_script = true;
            self.run_script(script)
        } else {
            // E0001 file: an empty program
            let main = self.empty_main(TextRange::empty(0.into()), true);
            Program {
                name: self.input.script_name.to_string(),
                globals: Vec::new(),
                routines: Vec::new(),
                main,
                init: None,
                uses_sysutils: false,
            }
        }
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
        }
    }

    // ---- program ----

    fn run_program(&mut self, program: ast::Program) -> Program {
        let name = program
            .header()
            .and_then(|h| h.name())
            .map(|t| self.text(&t).to_string())
            .unwrap_or_else(|| "Program".to_string());
        let mut init_body: Vec<Stmt> = Vec::new();
        let mut init_last: Option<TextRange> = None;
        for item in program.items() {
            match item {
                ast::Item::Uses(u) => self.check_uses(&u),
                ast::Item::Const(c) => self.declare_global_consts(&c),
                ast::Item::Var(v) => self.declare_global_vars(&v, &mut init_body, &mut init_last),
                ast::Item::Routine(r) => self.declare_routine(&r, false),
                ast::Item::Error(_) => {}
            }
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

    fn finish_program(&mut self, name: String, main: Routine, init: Option<Routine>) -> Program {
        let routines: Vec<Routine> = self
            .routines
            .drain(..)
            .map(|r| {
                r.hir.unwrap_or_else(|| Routine {
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
                })
            })
            .collect();
        Program {
            name,
            globals: std::mem::take(&mut self.globals),
            routines,
            main,
            init,
            uses_sysutils: self.sysutils != SysUtils::NotUsed,
        }
    }

    // ---- script ----

    fn run_script(&mut self, script: ast::Script) -> Program {
        let name = self.input.script_name.to_string();
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
                ast::ScriptItem::Const(c) => self.declare_global_consts(&c),
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
                }
                "system" => {
                    self.error(Code::E(2500), "unit `System` is already used", range);
                }
                _ => {
                    let d = self.error(Code::E(2500), format!("unit `{text}` not found"), range);
                    if builtins::is_delphi_os_unit(&lower) {
                        d.notes
                            .push("Puy has no OS-specific units, VCL or FMX".into());
                        d.notes.push(
                            "check the platform at run time with `TOSVersion.Platform`, and use `Puy.UI` for a GUI".into(),
                        );
                    } else {
                        d.notes.push(
                            "only `System.SysUtils` can be used in this release; more units of the standard library come with later releases".into(),
                        );
                    }
                }
            }
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
            for scope in c.scopes.iter().rev() {
                for (n, sym) in scope {
                    if n == lower {
                        return Some(match sym {
                            LSym::Var(id) => c.vars[id.0 as usize].span,
                            LSym::Const { span, .. } | LSym::Unsupported { span } => *span,
                        });
                    }
                }
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
                    GSym::Global(id) => self.globals[id.0 as usize].span,
                    GSym::Const { span, .. } => *span,
                    GSym::Routine(id) => self.routines[id.0 as usize].name_span,
                });
            }
        }
        None
    }

    /// Position of the declaration of a name visible from that position (local scope, globals). Used for E0708 in type
    /// positions and for detecting a user-defined `IntToStr` (`find_declared` checks duplicates and does not see globals in `<main>`).
    fn find_visible(&self, lower: &str) -> Option<TextRange> {
        if let Some(c) = &self.cur {
            for scope in c.scopes.iter().rev() {
                for (n, sym) in scope {
                    if n == lower {
                        return Some(match sym {
                            LSym::Var(id) => c.vars[id.0 as usize].span,
                            LSym::Const { span, .. } | LSym::Unsupported { span } => *span,
                        });
                    }
                }
            }
        }
        for (n, sym) in self.global_scope.iter().rev() {
            if n == lower {
                return Some(match sym {
                    GSym::Global(id) => self.globals[id.0 as usize].span,
                    GSym::Const { span, .. } => *span,
                    GSym::Routine(id) => self.routines[id.0 as usize].name_span,
                });
            }
        }
        None
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

    fn src(&self, range: TextRange) -> String {
        String::from_utf8_lossy(
            &self.input.bytes[usize::from(range.start())..usize::from(range.end())],
        )
        .into_owned()
    }

    fn resolve_type(&mut self, t: Option<ast::TypeRef>) -> Ty {
        let Some(t) = t else { return Ty::Error };
        if t.string_token().is_some() {
            return Ty::String;
        }
        let Some(q) = t.name() else { return Ty::Error };
        let text = q.text();
        let lower = text.to_ascii_lowercase();
        let range = q.syntax().text_range();
        // A qualified name such as `System.Integer`
        let unqualified = lower.strip_prefix("system.").unwrap_or(&lower).to_string();
        if let Some(ty) = Ty::from_name(&unqualified) {
            // If the user declared the same name, E0708 (M1 has no type declarations, so it is a variable or the like)
            if !lower.contains('.') && self.find_visible(&lower).is_some() {
                self.error(Code::E(708), format!("`{text}` is not a type"), range);
                return Ty::Error;
            }
            return ty;
        }
        // Names that are not types (variables, constants, routines, units) give E0708
        if self.is_unit_name(&lower)
            || (!lower.contains('.') && self.find_visible(&lower).is_some())
        {
            self.error(Code::E(708), format!("`{text}` is not a type"), range);
            return Ty::Error;
        }
        if lower.contains('.') {
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
                let note = builtins::deleted_note(code);
                self.error(code, format!("`{text}` is not supported in Puy"), range)
                    .notes
                    .push(note.into());
            }
            Some(_) => {
                self.error(Code::E(708), format!("`{text}` is not a type"), range);
            }
            None => {
                if lower == "reference" {
                    self.error(
                        Code::E(3200),
                        "`reference to` is not supported yet (planned for M3)",
                        range,
                    );
                } else {
                    self.error(
                        Code::E(700),
                        format!("undeclared identifier `{text}`"),
                        range,
                    );
                }
            }
        }
        Ty::Error
    }

    fn declare_global_consts(&mut self, section: &ast::ConstSection) {
        for decl in section.decls() {
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let (value, ty) = self.eval_const_decl(decl.type_ref(), decl.value());
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
    fn eval_const_decl(
        &mut self,
        type_ref: Option<ast::TypeRef>,
        value: Option<ast::Expr>,
    ) -> (Option<Const>, Ty) {
        let declared = type_ref.map(|t| self.resolve_type(Some(t)));
        let Some(value) = value else {
            return (None, Ty::Error);
        };
        let e = self.lower_expr(&value);
        if e.ty.is_error() {
            return (None, Ty::Error);
        }
        let range = e.span;
        let e = match declared {
            Some(t) if !t.is_error() => self.coerce(e, t, true, None),
            Some(_) => return (None, Ty::Error),
            None => e,
        };
        match e.value {
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
                    span: range,
                });
                self.global_scope.push((lower, GSym::Global(id)));
                ids.push(id);
            }
            if let Some(initializer) = decl.initializer() {
                // Initial values of globals are constant expressions
                let e = self.lower_expr(&initializer);
                let e = if ty.is_error() || e.ty.is_error() {
                    e
                } else {
                    self.coerce(e, ty, true, None)
                };
                if e.value.is_none() && !e.ty.is_error() {
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
        let init = decl.initializer().map(|e| self.lower_expr(&e));
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
                let value = if ty.is_error() || e.ty.is_error() {
                    e.clone()
                } else {
                    self.coerce(e.clone(), ty, checks.range, None)
                };
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
        for group in header.params() {
            let ty = self.resolve_type(group.type_ref());
            for n in group.names() {
                params.push(ParamInfo {
                    name: self.text(&n).to_string(),
                    ty,
                    mode: group.mode(),
                    span: n.text_range(),
                });
            }
        }
        let ret = if is_function {
            Some(self.resolve_type(header.return_type()))
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
                Some(SyntaxKind::OVERLOAD_KW)
                | Some(SyntaxKind::DEPRECATED_KW)
                | Some(SyntaxKind::EXPERIMENTAL_KW) => {
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
        // Whether already declared
        let existing = self
            .global_scope
            .iter()
            .rev()
            .find(|(n, _)| *n == lower)
            .cloned();
        let id = match existing {
            Some((_, GSym::Routine(id)))
                if !self.routines[id.0 as usize].has_body
                    && self.routines[id.0 as usize].forward_span.is_some()
                    && !has_forward =>
            {
                // Body of a forward routine: does the heading match?
                let info = &self.routines[id.0 as usize];
                let same = info.params.len() == params.len()
                    && info.params.iter().zip(&params).all(|(a, b)| {
                        a.ty == b.ty && a.mode == b.mode && a.name.eq_ignore_ascii_case(&b.name)
                    })
                    && info.ret == ret;
                if !same {
                    let fwd = info.forward_span.unwrap();
                    let d = Diagnostic::error(
                        Code::E(723),
                        format!("body of `{name_text}` does not match its forward declaration"),
                        Stage::Sema,
                    )
                    .with_primary(self.span(name_range), "")
                    .with_secondary(self.span(fwd), "declared here");
                    self.push(d);
                }
                if body.is_none() {
                    // Another body-less declaration after the forward declaration. The parser has reported E0411,
                    // so report nothing here (E0723 at the end if it never gets a body)
                    return;
                }
                id
            }
            _ => {
                if self.check_duplicate(&lower, name_range) {
                    return;
                }
                let id = RoutineId(self.routines.len() as u32);
                self.routines.push(RoutineInfo {
                    name: name_text.clone(),
                    params: params.clone(),
                    ret,
                    has_body: false,
                    forward_span: (has_forward || body.is_none()).then_some(name_range),
                    implicit_forward: !has_forward && body.is_none(),
                    name_span: name_range,
                    hir: None,
                });
                self.global_scope.push((lower.clone(), GSym::Routine(id)));
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
        }
        // Parameters
        let params = self.routines[id.0 as usize].params.clone();
        for p in &params {
            let plower = p.name.to_ascii_lowercase();
            // A duplicate parameter name (E0701) still takes a slot (keeps the parameter list in step with the registers).
            // Only the first declaration of the name is visible
            let duplicate = self.check_duplicate(&plower, p.span);
            let vid = self.add_var(p.name.clone(), p.ty, VarKind::Param(p.mode), p.span);
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
            if node.kind() == SyntaxKind::ERROR_NODE {
                self.declare_unsupported_nested_routine(&node);
                continue;
            }
            match ast::Item::cast(node) {
                Some(ast::Item::Const(c)) => self.declare_local_consts(&c),
                Some(ast::Item::Var(v)) => self.declare_local_vars(&v, &mut stmts),
                _ => {}
            }
        }
        if let Some(block) = body.block() {
            let inner = self.lower_block_stmts(&block);
            stmts.extend(inner);
        }
        let routine = self.end_routine(stmts, end_span);
        self.routines[id.0 as usize].hir = Some(routine);
    }

    /// If an `ERROR` node in the body is a nested routine (for which the parser reported E3200), adds its name
    /// to the local scope, so calls in the enclosing body do not cascade into E0700 (`semantics.md` chapter 8).
    /// No duplicate check (nested routines are checked when M2 treats them as declarations).
    fn declare_unsupported_nested_routine(&mut self, node: &SyntaxNode) {
        let Some(name_tok) = node
            .children()
            .find_map(ast::RoutineDecl::cast)
            .and_then(|d| d.header())
            .and_then(|h| h.name())
            .and_then(|q| q.last_ident())
        else {
            return;
        };
        let lower = self.text(&name_tok).to_ascii_lowercase();
        let span = name_tok.text_range();
        self.cur_mut()
            .scopes
            .last_mut()
            .expect("the routine scope is created by begin_routine")
            .push((lower, LSym::Unsupported { span }));
    }

    /// Routines with only `forward` and no body (E0723).
    fn check_forward_bodies(&mut self) {
        let missing: Vec<(String, TextRange)> = self
            .routines
            .iter()
            .filter(|r| !r.has_body && r.forward_span.is_some() && !r.implicit_forward)
            .map(|r| (r.name.clone(), r.forward_span.unwrap()))
            .collect();
        for (name, span) in missing {
            let d = Diagnostic::error(
                Code::E(723),
                format!("forward declaration of `{name}` has no body"),
                Stage::Sema,
            )
            .with_primary(self.span(span), "");
            self.sink.push(d);
        }
    }

    fn declare_local_consts(&mut self, section: &ast::ConstSection) {
        for decl in section.decls() {
            let Some(name_tok) = decl.name() else {
                continue;
            };
            let name = self.text(&name_tok).to_string();
            let lower = name.to_ascii_lowercase();
            let range = name_tok.text_range();
            let (value, ty) = self.eval_const_decl(decl.type_ref(), decl.value());
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
            let init = decl.initializer().map(|e| self.lower_expr(&e));
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
                    let value = if ty.is_error() || e.ty.is_error() {
                        e.clone()
                    } else {
                        self.coerce(e.clone(), ty, checks.range, None)
                    };
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
            loop_vars: Vec::new(),
            loop_globals: Vec::new(),
            routine: id,
            ret,
            is_main,
            has_errors: false,
            has_syntax_errors: syntax_errors,
            lower: name.to_ascii_lowercase(),
            display_name: name,
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
        };
        // Control-flow analysis (skipped for routines containing syntax errors)
        if !routine.has_syntax_errors {
            let diags = flow::analyze(&routine, self.input.file, self.input.bytes, &ctx.used_vars);
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
        });
        id
    }

    fn var(&self, id: VarId) -> &Var {
        &self.cur.as_ref().unwrap().vars[id.0 as usize]
    }

    // ---- Name resolution ----

    /// Resolves an unqualified name. `range` is the position of the identifier (for diagnostics).
    fn resolve_ident(&mut self, name: &str, range: TextRange) -> Resolved {
        let lower = name.to_ascii_lowercase();
        if let Some(c) = &self.cur {
            for scope in c.scopes.iter().rev() {
                for (n, sym) in scope.iter().rev() {
                    if *n == lower {
                        return match sym {
                            LSym::Var(id) => Resolved::Var(*id),
                            LSym::Const { value, ty, span } => Resolved::Const {
                                value: value.clone(),
                                ty: *ty,
                                span: *span,
                            },
                            LSym::Unsupported { .. } => Resolved::Error,
                        };
                    }
                }
            }
            if lower == "result" {
                if c.ret.is_some() {
                    return Resolved::Var(VarId(
                        c.vars
                            .iter()
                            .take_while(|v| matches!(v.kind, VarKind::Param(_)))
                            .count() as u32,
                    ));
                }
                let decl = (!c.is_main).then_some(c.name_span);
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
                        span: *span,
                    },
                    GSym::Routine(id) => Resolved::Routine(*id),
                };
            }
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
                SysUtils::Used => return Resolved::Func(f),
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
        // A variable at the script's top level (not visible from routines)
        let top_var = self.script_top_var_outside_main(&lower);
        let mut d = Diagnostic::error(
            Code::E(700),
            format!("undeclared identifier `{name}`"),
            Stage::Sema,
        )
        .with_primary(self.span(range), "");
        if let Some(decl) = top_var {
            d = d
                .with_secondary(self.span(decl), "declared here")
                .with_note("a top-level variable of a script is a local variable of the main block and is not visible inside routines")
                .with_note("pass the value as a parameter instead");
        }
        self.push(d);
        Resolved::Error
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
        if let Some(pos) = self.uses_fix.uses_semicolon {
            return Some(Fix::safe(
                "add `uses System.SysUtils`",
                vec![TextEdit::insert(self.input.file, pos, ", System.SysUtils")],
            ));
        }
        let (pos, text) = self.uses_fix.insert.as_ref()?;
        Some(Fix::safe(
            "add `uses System.SysUtils`",
            vec![TextEdit::insert(self.input.file, *pos, text.clone())],
        ))
    }

    /// Resolves a unit-qualified name (`System.Length`, `SU.IntToStr`).
    fn resolve_in_unit(&mut self, unit: UnitRef, name: &str, range: TextRange) -> Resolved {
        let lower = name.to_ascii_lowercase();
        match unit {
            UnitRef::System => match builtins::lookup_system(&lower) {
                Some(builtins::SystemName::Type(t)) => Resolved::Type(t),
                Some(builtins::SystemName::True) => Resolved::Bool(true),
                Some(builtins::SystemName::False) => Resolved::Bool(false),
                Some(builtins::SystemName::Func(f)) => Resolved::Func(f),
                Some(builtins::SystemName::Proc(p)) => Resolved::Proc(p),
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
                Some(f) => Resolved::Func(f),
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
            USES_CLAUSE | CONST_SECTION | ROUTINE_DECL => {}
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
        .filter(|c| matches!(c.kind(), USES_CLAUSE | CONST_SECTION | ROUTINE_DECL))
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
