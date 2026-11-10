//! Entry point that connects the stages (`driver` in `docs/design/compiler-pipeline.md` 2.1).
//!
//! Lexing → directive processing → parsing → semantic analysis → lowering to IR → code generation. Diagnostics are collected without stopping at each stage.
//!
//! With a package root (`puy.toml`), the units named in `uses` are found under `src/`, read in
//! depth-first order and analyzed dependencies first (`docs/spec/units.md`,
//! `docs/design/compiler-pipeline.md` 3.8).

use std::collections::HashMap;

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Fix, Severity, Stage, TextEdit, codes};
use crate::directives::{self, BuildConfig, DirectiveOutput, WarnState};
use crate::lexer;
use crate::parser::{self, ParseInput};
use crate::sema::{self, hir};
use crate::source::{
    FileId, MAX_SOURCE_SIZE, SourceFile, SourceMap, SourceProvider, Span, TOO_LARGE_MESSAGE,
    TextRange, TextSize,
};
use crate::syntax::{SyntaxKind, SyntaxNode};

/// Compilation options.
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    pub build: BuildConfig,
    /// Diagnostics created by the CLI (E2507, F0002). Rendered in the same order as the other diagnostics.
    pub extra_diagnostics: Vec<Diagnostic>,
    /// Name of the script's program (derived from the file name; `Script` if `None`).
    pub script_name: Option<String>,
    /// Additional files referenced by `extra_diagnostics` (`puy.toml` etc.). Numbered in order starting at `FileId(1)`.
    pub extra_files: Vec<(String, Vec<u8>)>,
    /// The package root, if `puy.toml` was found (`docs/spec/units.md` 2.1).
    pub package: Option<PackageRoot>,
    /// Compiling to run the file (`puy run`): a unit is E0408.
    pub run: bool,
}

/// Where the package root is, as seen from the file passed to `puy`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageRoot {
    /// Number of directories to go up from the directory of the passed file (the working
    /// directory for `puy -e` / `puy run -`) to reach the root.
    pub levels: u32,
    /// The passed file relative to the root, with `/` between the parts (`None` for `-e` / stdin).
    pub file: Option<String>,
}

/// Result of a compilation.
pub struct CompileOutput {
    /// Bytes of the module (`.puyc`). `None` if there are errors or the file is a unit.
    pub module: Option<Vec<u8>>,
    /// Diagnostics sorted in ascending order of position.
    pub diagnostics: Vec<Diagnostic>,
    /// Files that entered the compilation unit (used to render diagnostics).
    pub sources: SourceMap,
    /// For dumping stages (`--dump`).
    pub stages: Stages,
    /// The passed file is a unit (no module is built).
    pub is_unit: bool,
    /// The source files checked, in file order (`docs/spec/units.md` 8.1): the passed file, then
    /// the units (not the included files).
    pub files: Vec<FileId>,
}

/// Per-stage results (for dumps).
#[derive(Default)]
pub struct Stages {
    pub tokens: Option<Vec<lexer::Token>>,
    pub cst: Option<SyntaxNode>,
    pub hir: Option<hir::Program>,
    /// Built only when there are no errors up to semantic analysis.
    pub ir: Option<crate::ir::Module>,
    pub module: Option<crate::bytecode::Module>,
}

/// One file after lexing, directives and parsing.
struct Front {
    file: FileId,
    dir: DirectiveOutput,
    root: SyntaxNode,
    invalid_utf8: bool,
}

fn front(
    file: FileId,
    sources: &mut SourceMap,
    provider: &mut dyn SourceProvider,
    build: &BuildConfig,
    sink: &mut DiagnosticSink,
) -> Front {
    let lexed = lexer::lex(file, sources.get(file), sink, false);
    let invalid_utf8 = lexed.is_invalid_utf8();
    // Directive processing (skipped for files stopped by E0001)
    let dir: DirectiveOutput = if invalid_utf8 {
        DirectiveOutput {
            tokens: lexed.tokens.clone(),
            switches: directives::SwitchTable::new(directives::Switches::initial(build)),
            placements: Vec::new(),
            blocks: Vec::new(),
            suppress_after: None,
            include_positions: Vec::new(),
        }
    } else {
        let mut cx = directives::ProcessContext {
            config: build,
            provider,
            sources,
            sink,
            include_positions: Vec::new(),
        };
        directives::process(file, lexed.tokens, &mut cx)
    };
    let bytes = sources.get(file).bytes.clone();
    let parsed = parser::parse(
        &ParseInput {
            file,
            bytes: &bytes,
            tokens: &dir.tokens,
            directives: &dir,
            invalid_utf8,
        },
        sink,
    );
    Front {
        file,
        dir,
        root: parsed.root,
        invalid_utf8,
    }
}

/// The unit name of a file under `src/` (`docs/spec/units.md` 2.2): the folders joined with `.`
/// and the file name without `.pas`. `None` for a file that is not a unit: not `.pas`, a part
/// that is not an identifier, or a test (`*_test.pas`).
pub fn unit_name_of(rel: &str) -> Option<String> {
    let stem = rel.strip_suffix(".pas")?;
    if stem.to_ascii_lowercase().ends_with("_test") {
        return None;
    }
    let parts: Vec<&str> = stem.split(['/', '.']).collect();
    parts
        .iter()
        .all(|p| is_identifier(p))
        .then(|| parts.join("."))
}

fn is_identifier(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        && SyntaxKind::reserved_word(&s.to_ascii_lowercase()).is_none()
}

/// The path that reaches the package root from the passed file (`docs/spec/units.md` 8.2): the
/// directory part of `path` with `levels` trailing parts dropped, then `../` for the levels left.
/// Empty, or ending with `/`.
pub fn root_prefix(path: &str, levels: u32) -> String {
    let dir = match path.rfind(['/', '\\']) {
        Some(i) => &path[..i],
        None => "",
    };
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split(['/', '\\']).collect()
    };
    let mut up = 0u32;
    for _ in 0..levels {
        match parts.last() {
            Some(p) if !matches!(*p, ".." | "." | "") => {
                parts.pop();
            }
            _ => up += 1,
        }
    }
    let mut out = String::new();
    for p in &parts {
        out.push_str(p);
        out.push('/');
    }
    for _ in 0..up {
        out.push_str("../");
    }
    out
}

/// A path normalized as a string (`docs/spec/units.md` 2.3): `/` between the parts, no `.`
/// parts, and `..` cancelling the part before it.
pub fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with(['/', '\\']);
    let mut out: Vec<&str> = Vec::new();
    for p in path.split(['/', '\\']) {
        match p {
            "" | "." => {}
            ".." if out.last().is_some_and(|l| *l != "..") => {
                out.pop();
            }
            _ => out.push(p),
        }
    }
    let joined = out.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// State of a unit found under `src/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visit {
    NotRead,
    Visiting,
    Done,
}

struct UnitEntry {
    /// The name from the path (`MyApp.Models.User`).
    name: String,
    /// The path from the root (`src/MyApp/Models.User.pas`).
    rel: String,
    /// The files mapping to the name, from the root (more than one is E2506).
    files: Vec<String>,
    front: Option<Front>,
    visit: Visit,
    /// Its names are the error type (`docs/spec/units.md` 3.4).
    broken: bool,
    /// Index into `ModuleState::units` once analyzed.
    iface: Option<usize>,
}

/// What an item of `uses` was found to be.
#[derive(Debug, Clone, Copy)]
enum Found {
    /// A unit of the table (its names are the error type if it is broken).
    Unit(usize),
    Broken,
}

struct Units<'p> {
    provider: &'p mut dyn SourceProvider,
    build: BuildConfig,
    /// The prefix that reaches the root from the passed file (`None` without a root).
    prefix: Option<String>,
    entries: Vec<UnitEntry>,
    by_name: HashMap<String, usize>,
    /// What each item of `uses` names, per file.
    found: HashMap<FileId, Vec<(TextRange, Found)>>,
    /// Units being read (for cycles), innermost last.
    stack: Vec<usize>,
    /// Units in initialization order (dependencies first).
    post: Vec<usize>,
    /// Source files in file order.
    files: Vec<FileId>,
    /// Units that cannot be used, with the E2500 to report at their first item in file order
    /// (`docs/spec/units.md` 3.4, 8.1): a file that cannot be read or is not a unit.
    unusable: HashMap<usize, String>,
}

impl Units<'_> {
    fn list(&mut self) {
        let Some(prefix) = self.prefix.clone() else {
            return;
        };
        let files = self
            .provider
            .list(&format!("{prefix}src"))
            .unwrap_or_default();
        let mut files: Vec<String> = files.into_iter().map(|f| f.replace('\\', "/")).collect();
        files.sort();
        for f in files {
            let Some(name) = unit_name_of(&f) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            let rel = format!("src/{f}");
            match self.by_name.get(&lower) {
                Some(&i) => self.entries[i].files.push(rel),
                None => {
                    self.by_name.insert(lower, self.entries.len());
                    self.entries.push(UnitEntry {
                        name,
                        rel: rel.clone(),
                        files: vec![rel],
                        front: None,
                        visit: Visit::NotRead,
                        broken: false,
                        iface: None,
                    });
                }
            }
        }
    }

    /// The display path of a file under the root (`docs/spec/units.md` 8.2).
    fn display(&self, rel: &str) -> String {
        format!("{}{rel}", self.prefix.as_deref().unwrap_or(""))
    }

    /// Reads the units named in the `uses` of a file, depth first (`docs/spec/units.md` 8.1).
    fn visit(
        &mut self,
        file: FileId,
        root: &SyntaxNode,
        sources: &mut SourceMap,
        sink: &mut DiagnosticSink,
    ) {
        let mut found: Vec<(TextRange, Found)> = Vec::new();
        for item in uses_items(root) {
            if self.prefix.is_none() {
                continue;
            }
            let lower = item.name.to_ascii_lowercase();
            if matches!(lower.as_str(), "system" | "system.sysutils" | "sysutils") {
                continue;
            }
            let Some(&idx) = self.by_name.get(&lower) else {
                continue;
            };
            let span = Span::new(file, item.range);
            if self.entries[idx].files.len() > 1 {
                let mut d = Diagnostic::error(
                    Code::E(2506),
                    format!("two files map to the unit name `{}`", item.name),
                    Stage::Driver,
                )
                .with_primary(span, "");
                for f in &self.entries[idx].files {
                    d = d.with_note(format!("`{f}`"));
                }
                sink.push(d);
                found.push((item.range, Found::Broken));
                continue;
            }
            match self.entries[idx].visit {
                Visit::Visiting => {
                    self.cycle(idx, span, sink);
                    found.push((item.range, Found::Broken));
                    continue;
                }
                Visit::Done => {}
                Visit::NotRead => self.read(idx, sources, sink),
            }
            if let Some(in_path) = &item.in_path {
                self.check_in_path(file, idx, in_path, sources, sink);
            }
            found.push((item.range, Found::Unit(idx)));
        }
        self.found.insert(file, found);
    }

    /// E2505: the item closes a cycle (`docs/spec/units.md` 3.3).
    fn cycle(&self, idx: usize, span: Span, sink: &mut DiagnosticSink) {
        let at = self
            .stack
            .iter()
            .position(|&i| i == idx)
            .expect("a unit being visited is on the stack");
        let mut path: Vec<&str> = self.stack[at..]
            .iter()
            .map(|&i| self.entries[i].name.as_str())
            .collect();
        path.push(&self.entries[idx].name);
        let first = path[0];
        let second = path[1];
        let route: Vec<String> = path.iter().map(|n| format!("`{n}`")).collect();
        let d = Diagnostic::error(
            Code::E(2505),
            format!("units `{first}` and `{second}` depend on each other"),
            Stage::Driver,
        )
        .with_primary(span, "")
        .with_note(route.join(" -> "))
        .with_note("move the shared declarations to a separate unit");
        sink.push(d);
    }

    /// Reads, parses and visits a unit first named at `at`.
    fn read(&mut self, idx: usize, sources: &mut SourceMap, sink: &mut DiagnosticSink) {
        let path = self.display(&self.entries[idx].rel);
        let name = self.entries[idx].name.clone();
        let bytes = match self.provider.read(&path) {
            Ok(b) if b.len() as u64 > MAX_SOURCE_SIZE => Err(TOO_LARGE_MESSAGE.to_string()),
            Ok(b) => Ok(b),
            Err(e) => Err(e.fixed_text().to_string()),
        };
        let bytes = match bytes {
            Ok(b) => b,
            Err(why) => {
                self.unusable
                    .insert(idx, format!("cannot read unit `{name}`: {why}"));
                self.entries[idx].visit = Visit::Done;
                self.entries[idx].broken = true;
                return;
            }
        };
        let file = sources.add(SourceFile::new(path, bytes));
        self.files.push(file);
        let fr = front(file, sources, self.provider, &self.build, sink);
        if fr.invalid_utf8 {
            // E0001 in the file; no E2500 on the item
            self.entries[idx].visit = Visit::Done;
            self.entries[idx].broken = true;
            self.entries[idx].front = Some(fr);
            return;
        }
        let Some(unit) = fr.root.children().find(|n| n.kind() == SyntaxKind::UNIT) else {
            self.unusable.insert(idx, format!("`{name}` is not a unit"));
            self.entries[idx].visit = Visit::Done;
            self.entries[idx].broken = true;
            self.entries[idx].front = Some(fr);
            return;
        };
        check_unit_name(file, &unit, &name, sources, sink);
        let root = fr.root.clone();
        self.entries[idx].front = Some(fr);
        self.entries[idx].visit = Visit::Visiting;
        self.stack.push(idx);
        self.visit(file, &root, sources, sink);
        self.stack.pop();
        self.entries[idx].visit = Visit::Done;
        self.post.push(idx);
    }

    /// Reports each unit that cannot be used once, at its first item in file order (the order
    /// of `files`; `docs/spec/units.md` 3.4, 8.1).
    fn report_unusable(&mut self, sink: &mut DiagnosticSink) {
        for f in &self.files {
            let Some(items) = self.found.get(f) else {
                continue;
            };
            for (range, found) in items {
                if let Found::Unit(idx) = found
                    && let Some(msg) = self.unusable.remove(idx)
                {
                    sink.push(
                        Diagnostic::error(Code::E(2500), msg, Stage::Driver)
                            .with_primary(Span::new(*f, *range), ""),
                    );
                }
            }
        }
    }

    /// E2504: `uses … in '…'` names another file than the one found by name
    /// (`docs/spec/units.md` 2.3).
    fn check_in_path(
        &mut self,
        file: FileId,
        idx: usize,
        in_path: &(String, TextRange),
        sources: &SourceMap,
        sink: &mut DiagnosticSink,
    ) {
        let (written, range) = in_path;
        let using = sources.get(file).path.clone();
        let joined = directives::resolve_include_path(&using, written);
        let found = self.display(&self.entries[idx].rel);
        if normalize_path(&joined) == normalize_path(&found) {
            return;
        }
        // A path that no longer exists is only W0006 (the old path of a project)
        if self.provider.read(&joined).is_err() {
            return;
        }
        let d = Diagnostic::error(
            Code::E(2504),
            "file in `uses … in '…'` is not the unit found by name",
            Stage::Driver,
        )
        .with_primary(Span::new(file, *range), "")
        .with_note(format!(
            "units are always found by name; the file found is `{found}`"
        ));
        sink.push(d);
    }
}

/// E2503: the name in the heading of a unit differs from the name of its path, or the name is
/// reserved for the standard library (`docs/spec/units.md` 2.2).
fn check_unit_name(
    file: FileId,
    unit: &SyntaxNode,
    path_name: &str,
    sources: &SourceMap,
    sink: &mut DiagnosticSink,
) {
    let Some(q) = unit
        .children()
        .find(|n| n.kind() == SyntaxKind::UNIT_HEADER)
        .and_then(|h| {
            h.children()
                .find(|n| n.kind() == SyntaxKind::QUALIFIED_NAME)
        })
    else {
        return;
    };
    let range = q.text_range();
    let written = String::from_utf8_lossy(sources.get(file).text(range)).into_owned();
    let path_lower = path_name.to_ascii_lowercase();
    for prefix in ["System.", "Puy.", "Xml."] {
        if path_lower.starts_with(&prefix.to_ascii_lowercase()) {
            let d = Diagnostic::error(
                Code::E(2503),
                format!(
                    "unit names starting with `{prefix}` are reserved for the standard library"
                ),
                Stage::Driver,
            )
            .with_primary(Span::new(file, range), "")
            .with_note("the unit name is the path under `src/` with folders joined by `.`");
            sink.push(d);
            return;
        }
    }
    let written_name: String = written.split_whitespace().collect();
    if written_name.eq_ignore_ascii_case(path_name) {
        return;
    }
    let parts: Vec<&str> = written_name.split('.').collect();
    let place = match parts.as_slice() {
        [one] => format!("src/{one}.pas"),
        [first, rest @ ..] => format!("src/{first}/{}.pas", rest.join(".")),
        [] => String::new(),
    };
    let d = Diagnostic::error(
        Code::E(2503),
        format!("unit name `{written_name}` does not match its path"),
        Stage::Driver,
    )
    .with_primary(Span::new(file, range), "")
    .with_fix(Fix::suggestion(
        format!("replace with `{path_name}`"),
        vec![TextEdit::replace(file, range, path_name)],
    ))
    .with_note("the unit name is the path under `src/` with folders joined by `.`")
    .with_note(format!("move the file to `{place}`"));
    sink.push(d);
}

/// An item of `uses`.
struct UsesItem {
    name: String,
    /// The `QUALIFIED_NAME`.
    range: TextRange,
    /// The string of `in '…'` and its range.
    in_path: Option<(String, TextRange)>,
}

/// The items of the `uses` clauses of a file in order (a unit's `interface` section first).
fn uses_items(root: &SyntaxNode) -> Vec<UsesItem> {
    use SyntaxKind::*;
    let mut out = Vec::new();
    for clause in root.descendants().filter(|n| n.kind() == USES_CLAUSE) {
        let ok = clause.parent().is_some_and(|p| {
            matches!(
                p.kind(),
                PROGRAM | SCRIPT | INTERFACE_SECTION | IMPLEMENTATION_SECTION
            )
        });
        if !ok {
            continue;
        }
        for item in clause.children().filter(|n| n.kind() == USES_ITEM) {
            let Some(q) = item.children().find(|n| n.kind() == QUALIFIED_NAME) else {
                continue;
            };
            let name: Vec<String> = q
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| t.kind() == IDENT)
                .map(|t| t.text().to_string())
                .collect();
            let in_path = item
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| t.kind() == STRING_LIT)
                .and_then(|t| {
                    let text = t.text();
                    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
                    Some((inner.replace("''", "'"), t.text_range()))
                });
            out.push(UsesItem {
                name: name.join("."),
                range: q.text_range(),
                in_path,
            });
        }
    }
    out
}

/// Compiles the source of one file. `path` is the path shown in diagnostics (the string passed to `puy`, as is).
pub fn compile(
    path: &str,
    bytes: Vec<u8>,
    provider: &mut dyn SourceProvider,
    opts: &CompileOptions,
) -> CompileOutput {
    let mut sources = SourceMap::new();
    let file = sources.add(SourceFile::new(path, bytes));
    for (p, b) in &opts.extra_files {
        sources.add(SourceFile::new(p.clone(), b.clone()));
    }
    let mut sink = DiagnosticSink::new();
    for d in &opts.extra_diagnostics {
        sink.push(d.clone());
    }
    let mut stages = Stages::default();

    let main = front(file, &mut sources, provider, &opts.build, &mut sink);
    stages.tokens = Some(main.dir.tokens.clone());
    stages.cst = Some(main.root.clone());
    let main_unit = main.root.children().find(|n| n.kind() == SyntaxKind::UNIT);
    let is_unit = main_unit.is_some();

    // The units of the package (`docs/spec/units.md` 2)
    let mut units = Units {
        provider,
        build: opts.build.clone(),
        prefix: opts.package.as_ref().map(|p| root_prefix(path, p.levels)),
        entries: Vec::new(),
        by_name: HashMap::new(),
        found: HashMap::new(),
        stack: Vec::new(),
        post: Vec::new(),
        files: vec![file],
        unusable: HashMap::new(),
    };
    units.list();
    // A passed unit under `src/` is the unit of that name (a `uses` of it is a cycle)
    let main_entry = opts
        .package
        .as_ref()
        .and_then(|p| p.file.as_deref())
        .and_then(|f| f.strip_prefix("src/"))
        .and_then(unit_name_of)
        .filter(|_| is_unit)
        .and_then(|n| units.by_name.get(&n.to_ascii_lowercase()).copied());
    if let (Some(idx), Some(unit)) = (main_entry, &main_unit) {
        let name = units.entries[idx].name.clone();
        check_unit_name(file, unit, &name, &sources, &mut sink);
        units.entries[idx].visit = Visit::Visiting;
        units.stack.push(idx);
    }
    if !main.invalid_utf8 {
        let root = main.root.clone();
        units.visit(file, &root, &mut sources, &mut sink);
    }
    if let Some(idx) = main_entry {
        units.entries[idx].visit = Visit::Done;
    }
    units.report_unusable(&mut sink);
    if opts.run
        && let Some(unit) = &main_unit
    {
        let kw = unit
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::UNIT_KW)
            .map(|t| t.text_range())
            .unwrap_or_default();
        sink.push(
            Diagnostic::error(
                Code::E(408),
                "a unit cannot be run as a program",
                Stage::Driver,
            )
            .with_primary(Span::new(file, kw), "")
            .with_note("run a program that uses this unit, or check it with `puy check`"),
        );
    }

    // Semantic analysis, dependencies first
    let script_name = opts
        .script_name
        .clone()
        .unwrap_or_else(|| "Script".to_string());
    let has_root = opts.package.is_some();
    let mut state = sema::ModuleState::new();
    let post = std::mem::take(&mut units.post);
    for &idx in &post {
        let e = &units.entries[idx];
        let fr = e.front.as_ref().expect("a visited unit was parsed");
        let resolutions = resolutions_for(&units, fr.file);
        sema::analyze(
            &sema::SemaInput {
                file: fr.file,
                root: &fr.root,
                bytes: &sources.get(fr.file).bytes,
                directives: &fr.dir,
                script_name: &script_name,
                uses: &resolutions,
                has_root,
            },
            &mut state,
            &mut sink,
        );
        units.entries[idx].iface = Some(state.units.len() - 1);
    }
    let program = if main.invalid_utf8 {
        None
    } else {
        let resolutions = resolutions_for(&units, file);
        sema::analyze(
            &sema::SemaInput {
                file,
                root: &main.root,
                bytes: &sources.get(file).bytes,
                directives: &main.dir,
                script_name: &script_name,
                uses: &resolutions,
                has_root,
            },
            &mut state,
            &mut sink,
        );
        Some(sema::finish(state))
    };

    // Warning post-processing: `{$WARN}` state and the `disable with` hint
    let mut dirs: Vec<&Front> = vec![&main];
    dirs.extend(post.iter().filter_map(|&i| units.entries[i].front.as_ref()));
    dirs.extend(
        units
            .entries
            .iter()
            .filter(|e| e.broken)
            .filter_map(|e| e.front.as_ref()),
    );
    apply_warn_states(&mut sink, &dirs);
    let has_errors = sink.has_errors();

    // Lowering and code generation
    let ir = match (&program, has_errors || is_unit) {
        (Some(p), false) => {
            let files: Vec<(&DirectiveOutput, &SourceFile)> = p
                .units
                .iter()
                .map(|u| {
                    let fr = dirs
                        .iter()
                        .find(|f| f.file == u.file)
                        .expect("each unit of the program was parsed");
                    (&fr.dir, sources.get(u.file))
                })
                .collect();
            let paths: Vec<String> = p
                .units
                .iter()
                .map(|u| {
                    if u.file == file {
                        path.to_string()
                    } else {
                        units
                            .entries
                            .iter()
                            .find(|e| e.front.as_ref().is_some_and(|f| f.file == u.file))
                            .map(|e| e.rel.clone())
                            .unwrap_or_default()
                    }
                })
                .collect();
            Some(crate::ir::lower::lower(p, &files, &paths))
        }
        _ => None,
    };
    let module = ir
        .as_ref()
        .and_then(|ir| crate::codegen::generate(ir, &mut sink));
    let module = if sink.has_errors() { None } else { module };
    let bytes_out = module.as_ref().map(|m| m.encode(crate::VERSION));
    stages.module = module;
    stages.hir = program;
    stages.ir = ir;
    let order = file_order(&units.files, &dirs);
    let mut diagnostics = sink.into_vec();
    diagnostics.sort_by_key(|d| sort_key(d, &order));
    CompileOutput {
        module: bytes_out,
        diagnostics,
        sources,
        stages,
        is_unit,
        files: units.files.clone(),
    }
}

fn resolutions_for(units: &Units<'_>, file: FileId) -> Vec<sema::UsesResolution> {
    units
        .found
        .get(&file)
        .map(|v| {
            v.iter()
                .map(|(range, f)| sema::UsesResolution {
                    range: *range,
                    target: match f {
                        Found::Unit(i) => match units.entries[*i].iface {
                            Some(k) => sema::UsesTarget::Unit(k),
                            None => sema::UsesTarget::Broken,
                        },
                        Found::Broken => sema::UsesTarget::Broken,
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}

/// For each file, its rank in file order and, for an included file, the position of the `{$I}`
/// in the file it belongs to (`docs/spec/units.md` 8.1).
fn file_order(files: &[FileId], fronts: &[&Front]) -> HashMap<FileId, (u32, Option<TextSize>)> {
    let mut out = HashMap::new();
    for (rank, f) in files.iter().enumerate() {
        out.insert(*f, (rank as u32, None));
        if let Some(fr) = fronts.iter().find(|fr| fr.file == *f) {
            for (inc, pos) in &fr.dir.include_positions {
                out.insert(*inc, (rank as u32, Some(*pos)));
            }
        }
    }
    out
}

/// The ordering key of a diagnostic (`docs/spec/diagnostics.md` chapter 3, `docs/spec/units.md`
/// 8.1): file order, the position (an included file's diagnostics at its `{$I}`), stage, code.
#[allow(clippy::type_complexity)]
fn sort_key(
    d: &Diagnostic,
    order: &HashMap<FileId, (u32, Option<TextSize>)>,
) -> (u32, u32, u32, u32, Stage, Option<Code>) {
    match &d.primary {
        Some(l) => {
            let pos: u32 = l.span.range.start().into();
            match order.get(&l.span.file) {
                Some((rank, None)) => (rank + 1, pos, 0, 0, d.stage, d.code),
                Some((rank, Some(at))) => (rank + 1, (*at).into(), 1, pos, d.stage, d.code),
                // Files outside the order (`puy.toml`) come last
                None => (u32::MAX, l.span.file.0, 0, pos, d.stage, d.code),
            }
        }
        None => (0, 0, 0, 0, d.stage, d.code),
    }
}

/// Applies the `{$WARN}` state (OFF removes, ERROR promotes) and adds the `disable with` `notes`.
fn apply_warn_states(sink: &mut DiagnosticSink, fronts: &[&Front]) {
    let mut kept: Vec<Diagnostic> = Vec::new();
    for mut d in std::mem::take(sink).into_vec() {
        let Some(code @ Code::W(_)) = d.code else {
            kept.push(d);
            continue;
        };
        if !matches!(d.severity, Severity::Warning | Severity::Note) {
            kept.push(d);
            continue;
        }
        // The switch table is looked up by position in the file the warning belongs to (a warning
        // inside an included file uses the state at the including `{$I}`; `directives.md` chapter 5)
        let state = match &d.primary {
            Some(p) => {
                let f = p.span.file;
                if let Some(fr) = fronts.iter().find(|fr| fr.file == f) {
                    fr.dir.switches.at(p.span.range.start()).warn_state(code)
                } else if let Some((fr, pos)) = fronts.iter().find_map(|fr| {
                    fr.dir
                        .include_positions
                        .iter()
                        .find(|(i, _)| *i == f)
                        .map(|(_, pos)| (fr, *pos))
                }) {
                    fr.dir.switches.at(pos).warn_state(code)
                } else {
                    fronts[0].dir.switches.at(0.into()).warn_state(code)
                }
            }
            None => fronts[0].dir.switches.at(0.into()).warn_state(code),
        };
        match state {
            WarnState::Off => continue,
            WarnState::Error => {
                d.severity = Severity::Error;
            }
            WarnState::On => {
                if let Some(name) = codes::warning_name(code) {
                    d.notes.push(format!("disable with `{{$WARN {name} OFF}}`"));
                }
            }
        }
        kept.push(d);
    }
    let _ = Stage::Driver;
    sink.extend_raw(kept);
}

#[cfg(test)]
mod tests {
    use super::*;

    // The path to the root keeps the passed directory and goes up past it with `../`
    // (`docs/spec/units.md` 8.2).
    #[test]
    fn root_prefix_examples() {
        assert_eq!(root_prefix("tools/gen.pas", 1), "");
        assert_eq!(root_prefix("../app/bin/main.pas", 1), "../app/");
        assert_eq!(root_prefix("gen.pas", 1), "../");
        assert_eq!(root_prefix("main.pas", 0), "");
        assert_eq!(root_prefix("a/b/c.pas", 3), "../");
    }

    // Unit names come from the path under `src/`; tests and non-identifiers are not units
    // (`docs/spec/units.md` 2.2).
    #[test]
    fn unit_names_from_paths() {
        assert_eq!(
            unit_name_of("MyApp/Models.User.pas").as_deref(),
            Some("MyApp.Models.User")
        );
        assert_eq!(unit_name_of("my-notes.pas"), None);
        assert_eq!(unit_name_of("Calc_Test.pas"), None);
        assert_eq!(unit_name_of("begin.pas"), None);
        assert_eq!(unit_name_of("A.pp"), None);
    }

    /// Lists `src/Bad.pas` but cannot read it (a case file cannot be unreadable).
    struct Unreadable;

    impl SourceProvider for Unreadable {
        fn read(&mut self, _path: &str) -> Result<Vec<u8>, puy_host::HostError> {
            Err(puy_host::HostError::new(
                puy_host::HostErrorKind::PermissionDenied,
                "denied",
            ))
        }
        fn list(&mut self, _dir: &str) -> Result<Vec<String>, puy_host::HostError> {
            Ok(vec!["Bad.pas".into()])
        }
    }

    // A unit that cannot be read is E2500 once, at its first item, and its names are the error
    // type (no E0700; `docs/spec/units.md` 3.4).
    #[test]
    fn unreadable_unit_reported_once() {
        let src = "program P;\nuses Bad;\nbegin\n  WriteLn(FromBad);\nend.\n";
        let opts = CompileOptions {
            package: Some(PackageRoot {
                levels: 0,
                file: Some("main.pas".into()),
            }),
            ..Default::default()
        };
        let out = compile("main.pas", src.as_bytes().to_vec(), &mut Unreadable, &opts);
        let texts: Vec<String> = out
            .diagnostics
            .iter()
            .map(|d| format!("{} {}", d.code.unwrap(), d.message))
            .collect();
        assert_eq!(
            texts,
            ["E2500 cannot read unit `Bad`: permission denied"],
            "{texts:?}"
        );
    }

    /// Serves `src/A.pas` and lists `src/Bad.pas`, which cannot be read.
    struct BadAfterA;

    impl SourceProvider for BadAfterA {
        fn read(&mut self, path: &str) -> Result<Vec<u8>, puy_host::HostError> {
            match path {
                "src/A.pas" => {
                    Ok(b"unit A;\ninterface\nuses Bad;\nimplementation\nend.\n".to_vec())
                }
                _ => Err(puy_host::HostError::new(
                    puy_host::HostErrorKind::PermissionDenied,
                    "denied",
                )),
            }
        }
        fn list(&mut self, _dir: &str) -> Result<Vec<String>, puy_host::HostError> {
            Ok(vec!["A.pas".into(), "Bad.pas".into()])
        }
    }

    // The E2500 of an unreadable unit goes to its first item in file order (the passed file
    // first), not to the item that was read first (`docs/spec/units.md` 3.4, 8.1).
    #[test]
    fn unreadable_unit_reported_at_first_item_in_file_order() {
        let src = "program P;\nuses A, Bad;\nbegin\nend.\n";
        let opts = CompileOptions {
            package: Some(PackageRoot {
                levels: 0,
                file: Some("main.pas".into()),
            }),
            ..Default::default()
        };
        let out = compile("main.pas", src.as_bytes().to_vec(), &mut BadAfterA, &opts);
        let at: Vec<(String, String)> = out
            .diagnostics
            .iter()
            .map(|d| {
                let p = d.primary.as_ref().expect("a primary");
                (
                    out.sources.get(p.span.file).path.clone(),
                    String::from_utf8_lossy(out.sources.get(p.span.file).text(p.span.range))
                        .into_owned(),
                )
            })
            .collect();
        assert_eq!(at, [("main.pas".to_string(), "Bad".to_string())], "{at:?}");
    }

    // `.` parts are dropped and `..` cancels the part before it (`docs/spec/units.md` 2.3).
    #[test]
    fn normalized_paths() {
        assert_eq!(normalize_path("./src/../src/A.pas"), "src/A.pas");
        assert_eq!(normalize_path("..\\x/./A.pas"), "../x/A.pas");
    }
}
