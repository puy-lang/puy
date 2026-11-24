//! Entry point that connects the stages (`driver` in `docs/design/compiler-pipeline.md` 2.1).
//!
//! Lexing → directive processing → parsing → semantic analysis → lowering to IR → code generation. Diagnostics are collected without stopping at each stage.
//!
//! With a package root (`puy.toml`), the units named in `uses` are found under `src/`, read in
//! depth-first order and analyzed dependencies first (`docs/spec/units.md`,
//! `docs/design/compiler-pipeline.md` 3.8).

use std::collections::{HashMap, HashSet};

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Fix, Severity, Stage, TextEdit, codes};
use crate::directives::{self, BuildConfig, DirectiveOutput, WarnState};
use crate::lexer;
use crate::parser::{self, ParseInput};
use crate::sema::{self, hir};
use crate::source::{
    FileId, MAX_SOURCE_SIZE, Piece, SourceFile, SourceMap, SourceProvider, Span, TOO_LARGE_MESSAGE,
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
    /// The extensions of the runtime (`docs/spec/extensions.md` 1): their units are found after
    /// the package's and the standard library's.
    pub extensions: crate::ext::ExtensionSet,
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
    /// The passed file is a unit (no module is built, unless it is a test unit).
    pub is_unit: bool,
    /// The passed file is a test unit (`docs/spec/units.md` 2.4): its module has an empty
    /// `<main>` and the test table (`docs/spec/attributes.md` 5.2).
    pub is_test: bool,
    /// The source files checked, in file order (`docs/spec/units.md` 8.1): the passed file, then
    /// the units (not the included files, nor the units of the standard library, which
    /// `puy fix` does not rewrite).
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
pub(crate) struct Front {
    file: FileId,
    pub(crate) dir: DirectiveOutput,
    pub(crate) root: SyntaxNode,
    invalid_utf8: bool,
}

pub(crate) fn front(
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
            unicode_guard_ranges: Vec::new(),
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
    if without_test_suffix(stem).is_some() {
        return None;
    }
    let parts: Vec<&str> = stem.split(['/', '.']).collect();
    parts
        .iter()
        .all(|p| is_identifier(p))
        .then(|| parts.join("."))
}

/// For a test unit under `src/` (`docs/spec/units.md` 2.4), its unit name and the name of the
/// unit it is paired with: `MyApp/Calc_test.pas` is `MyApp.Calc_test`, paired with `MyApp.Calc`.
/// `None` for a file that is not a test (or whose name is not a unit name).
pub fn test_unit_name_of(rel: &str) -> Option<(String, String)> {
    let stem = rel.strip_suffix(".pas")?;
    let paired = without_test_suffix(stem)?;
    // The last part of the paired name is an identifier, and so is it with `_test` added
    let pair = unit_name_of(&format!("{paired}.pas"))?;
    Some((format!("{pair}{}", &stem[paired.len()..]), pair))
}

/// Whether the file at `path` is named as a test (`*_test.pas`, case-insensitive;
/// `docs/spec/units.md` 2.4).
fn is_test_file(path: &str) -> bool {
    path.rsplit(['/', '\\'])
        .next()
        .and_then(|f| f.strip_suffix(".pas"))
        .and_then(without_test_suffix)
        .is_some()
}

/// The file name (without `.pas`) of a test without its `_test` (case-insensitive;
/// `docs/spec/units.md` 2.2, 2.4); `None` when it does not end with `_test`.
fn without_test_suffix(stem: &str) -> Option<&str> {
    const SUFFIX: &str = "_test";
    let at = stem.len().checked_sub(SUFFIX.len())?;
    let suffix = stem.get(at..)?;
    suffix.eq_ignore_ascii_case(SUFFIX).then(|| &stem[..at])
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
    /// A unit of the standard library (`docs/spec/collections.md` 1.2); `None` for a unit of the
    /// package, read through the provider.
    stdlib: Option<puy_stdlib::Unit>,
    /// For a unit of the standard library, which of its parts are read (`docs/spec/collections.md`
    /// 1.2).
    parts: Vec<bool>,
    /// The unit of an extension (`docs/spec/extensions.md` 1).
    ext: Option<&'static crate::ext::Extension>,
    /// A unit under `tests/` (`docs/spec/units.md` 2.5): only the units under `tests/` see it.
    in_tests: bool,
}

impl UnitEntry {
    /// The file of a visited unit.
    fn file(&self) -> FileId {
        self.front.as_ref().expect("a visited unit was parsed").file
    }
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
    /// Units read as units of the standard library before the table of `puy-stdlib` (only the
    /// tests pass any; [`compile_with_stdlib`]).
    extra_stdlib: &'p [puy_stdlib::Unit],
    extensions: &'p crate::ext::ExtensionSet,
    build: BuildConfig,
    /// The prefix that reaches the root from the passed file (`None` without a root).
    prefix: Option<String>,
    entries: Vec<UnitEntry>,
    /// The units of the package under `src/` by lowercase name.
    by_name: HashMap<String, usize>,
    /// The helper units of the package under `tests/` by lowercase name (`docs/spec/units.md`
    /// 2.5).
    tests_by_name: HashMap<String, usize>,
    /// The files under `tests/`, which see the helper units there.
    tests_files: HashSet<FileId>,
    /// The units of the standard library named so far, by lowercase name.
    stdlib_by_name: HashMap<String, usize>,
    /// What each item of `uses` names, per file.
    found: HashMap<FileId, Vec<(TextRange, Found)>>,
    /// Units being read (for cycles), innermost last.
    stack: Vec<usize>,
    /// Units in initialization order (dependencies first).
    post: Vec<usize>,
    /// Source files in file order.
    files: Vec<FileId>,
    /// The files of the units of the standard library.
    stdlib_files: Vec<FileId>,
    /// Units that cannot be used, with the E2500 to report at their first item in file order
    /// (`docs/spec/units.md` 3.4, 8.1): a file that cannot be read or is not a unit.
    unusable: HashMap<usize, String>,
    /// The parts of the units of the standard library are chosen ([`Units::read_parts`]): a unit
    /// read from then on is read once, with the parts it is given.
    parts_chosen: bool,
    /// The files included by the units of the standard library, by path, added to the sources
    /// once.
    included: HashMap<&'static str, FileId>,
}

impl Units<'_> {
    fn list(&mut self) {
        let Some(prefix) = self.prefix.clone() else {
            return;
        };
        for base in ["src", "tests"] {
            let files = self
                .provider
                .list(&format!("{prefix}{base}"))
                .unwrap_or_default();
            let mut files: Vec<String> = files.into_iter().map(|f| f.replace('\\', "/")).collect();
            files.sort();
            for f in files {
                self.list_unit(base, &f);
            }
        }
    }

    /// Adds the file `f` under the directory `base` (`src` or `tests`) to the table of its
    /// units, if it is a unit (`docs/spec/units.md` 2.2, 2.5).
    fn list_unit(&mut self, base: &str, f: &str) {
        let Some(name) = unit_name_of(f) else {
            return;
        };
        let in_tests = base == "tests";
        let lower = name.to_ascii_lowercase();
        let rel = format!("{base}/{f}");
        let table = if in_tests {
            &mut self.tests_by_name
        } else {
            &mut self.by_name
        };
        match table.get(&lower) {
            Some(&i) => self.entries[i].files.push(rel),
            None => {
                table.insert(lower, self.entries.len());
                self.entries.push(UnitEntry {
                    name,
                    rel: rel.clone(),
                    files: vec![rel],
                    front: None,
                    visit: Visit::NotRead,
                    broken: false,
                    iface: None,
                    stdlib: None,
                    parts: Vec::new(),
                    ext: None,
                    in_tests,
                });
            }
        }
    }

    /// The display path of a file under the root (`docs/spec/units.md` 8.2).
    fn display(&self, rel: &str) -> String {
        format!("{}{rel}", self.prefix.as_deref().unwrap_or(""))
    }

    /// The display path of the unit `idx`: a unit of the standard library is shown as
    /// `<stdlib>/Name.pas` (`docs/spec/collections.md` 1.2).
    fn unit_path(&self, idx: usize) -> String {
        let e = &self.entries[idx];
        if e.stdlib.is_some() || e.ext.is_some() {
            return e.rel.clone();
        }
        self.display(&e.rel)
    }

    /// The unit of the standard library named `lower`, added to the table the first time it is
    /// named. Found after the units of the package (`docs/spec/collections.md` 1.2).
    fn stdlib_unit(&mut self, lower: &str) -> Option<usize> {
        if let Some(&i) = self.stdlib_by_name.get(lower) {
            return Some(i);
        }
        let unit = self
            .extra_stdlib
            .iter()
            .find(|u| u.name.eq_ignore_ascii_case(lower))
            .or_else(|| puy_stdlib::find(lower));
        let Some(unit) = unit else {
            return self.ext_unit(lower);
        };
        let rel = stdlib_path(unit.files[0].path);
        let idx = self.entries.len();
        self.entries.push(UnitEntry {
            name: unit.name.to_string(),
            rel: rel.clone(),
            files: vec![rel],
            front: None,
            visit: Visit::NotRead,
            broken: false,
            iface: None,
            stdlib: Some(*unit),
            parts: vec![false; unit.parts.len()],
            ext: None,
            in_tests: false,
        });
        self.stdlib_by_name.insert(lower.to_string(), idx);
        Some(idx)
    }

    /// The unit of an extension named `lower`, after the standard library's
    /// (`docs/spec/extensions.md` 1), added to the table the first time it is named.
    fn ext_unit(&mut self, lower: &str) -> Option<usize> {
        let ext = self.extensions.find(lower)?;
        let rel = ext_path(ext.unit);
        let idx = self.entries.len();
        self.entries.push(UnitEntry {
            name: ext.unit.to_string(),
            rel: rel.clone(),
            files: vec![rel],
            front: None,
            visit: Visit::NotRead,
            broken: false,
            iface: None,
            stdlib: None,
            parts: Vec::new(),
            ext: Some(ext),
            in_tests: false,
        });
        self.stdlib_by_name.insert(lower.to_string(), idx);
        Some(idx)
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
        let from_tests = self.tests_files.contains(&file);
        for item in uses_items(root) {
            let lower = item.name.to_ascii_lowercase();
            // `System` is always used; `SysUtils` is E2501 in the analysis
            if matches!(lower.as_str(), "system" | "sysutils") {
                continue;
            }
            // The package first, then the standard library (`docs/spec/collections.md` 1.2); the
            // helper units under `tests/` only from the units there (`docs/spec/units.md` 2.5)
            let (src, tests) = match self.prefix {
                Some(_) => (
                    self.by_name.get(&lower).copied(),
                    from_tests
                        .then(|| self.tests_by_name.get(&lower).copied())
                        .flatten(),
                ),
                None => (None, None),
            };
            let span = Span::new(file, item.range);
            let files: Vec<&String> = src
                .iter()
                .chain(tests.iter())
                .flat_map(|&i| &self.entries[i].files)
                .collect();
            if files.len() > 1 {
                let mut d = Diagnostic::error(
                    Code::E(2506),
                    format!("two files map to the unit name `{}`", item.name),
                    Stage::Driver,
                )
                .with_primary(span, "");
                for f in files {
                    d = d.with_note(format!("`{f}`"));
                }
                sink.push(d);
                found.push((item.range, Found::Broken));
                continue;
            }
            let Some(idx) = tests.or(src).or_else(|| self.stdlib_unit(&lower)) else {
                continue;
            };
            match self.entries[idx].visit {
                Visit::Visiting => {
                    self.cycle(idx, span, sink);
                    found.push((item.range, Found::Broken));
                    continue;
                }
                Visit::Done => {}
                Visit::NotRead => self.read(idx, sources, sink),
            }
            if let Some(in_path) = &item.in_path
                && self.entries[idx].stdlib.is_none()
                && self.entries[idx].ext.is_none()
            {
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
        let path = self.unit_path(idx);
        let name = self.entries[idx].name.clone();
        let stdlib = self.entries[idx].stdlib;
        let ext = self.entries[idx].ext;
        let bytes = match (stdlib, ext) {
            (Some(unit), _) => Ok(self.stdlib_source(idx, &unit)),
            (None, Some(ext)) => Ok(ext.source.as_bytes().to_vec()),
            (None, None) => match self.provider.read(&path) {
                Ok(b) if b.len() as u64 > MAX_SOURCE_SIZE => Err(TOO_LARGE_MESSAGE.to_string()),
                Ok(b) => Ok(b),
                Err(e) => Err(e.fixed_text().to_string()),
            },
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
        if let Some(unit) = &stdlib {
            self.stdlib_files.push(file);
            self.join_included(file, idx, unit, sources);
        }
        if ext.is_some() {
            self.stdlib_files.push(file);
        }
        // Before its parts are chosen, a unit of the standard library that has parts is parsed
        // without them, for its `uses` and its identifiers only: it is parsed again with them
        // ([`Units::read_parts`]), and only the diagnostics of that parse are kept
        let provisional = stdlib.is_some_and(|u| !u.parts.is_empty()) && !self.parts_chosen;
        let mut fr = if provisional {
            front(
                file,
                sources,
                self.provider,
                &self.build,
                &mut DiagnosticSink::new(),
            )
        } else {
            front(file, sources, self.provider, &self.build, sink)
        };
        // Only the main file's tokens are kept (for the token dump); the parse is done with them
        fr.dir.tokens = Vec::new();
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
        let builtin = stdlib.is_some() || ext.is_some();
        let base = if self.entries[idx].in_tests {
            self.tests_files.insert(file);
            "tests"
        } else {
            "src"
        };
        check_unit_name(file, &unit, &name, base, builtin, false, sources, sink);
        let root = fr.root.clone();
        self.entries[idx].front = Some(fr);
        self.entries[idx].visit = Visit::Visiting;
        self.stack.push(idx);
        self.visit(file, &root, sources, sink);
        self.stack.pop();
        self.entries[idx].visit = Visit::Done;
        self.post.push(idx);
    }

    /// The source of the unit of the standard library `idx` with the parts it reads.
    fn stdlib_source(&self, idx: usize, unit: &puy_stdlib::Unit) -> Vec<u8> {
        let parts = &self.entries[idx].parts;
        unit.source_with(|k| parts[k as usize])
            .into_owned()
            .into_bytes()
    }

    /// For a unit of the standard library kept in several files, adds the files it reads to the
    /// table (once) and records the pieces of `file` (its joined source) they make up, so that
    /// diagnostics and the line tables show the files and their own lines
    /// (`docs/spec/collections.md` 1.2).
    fn join_included(
        &mut self,
        file: FileId,
        idx: usize,
        unit: &puy_stdlib::Unit,
        sources: &mut SourceMap,
    ) {
        if unit.pieces.is_empty() {
            return;
        }
        let parts = &self.entries[idx].parts;
        let mut at = 0u32;
        let mut pieces: Vec<Piece> = Vec::new();
        for p in unit.pieces_with(|k| parts[k as usize]) {
            let f = unit.files[p.file as usize];
            let id = *self.included.entry(f.path).or_insert_with(|| {
                sources.add(SourceFile::new(
                    stdlib_path(f.path),
                    f.text.as_bytes().to_vec(),
                ))
            });
            let start = at;
            at += p.end - p.start;
            pieces.push(Piece {
                start: start.into(),
                end: at.into(),
                file: id,
                offset: p.start.into(),
            });
        }
        sources.join(file, pieces);
    }

    /// Chooses the parts of the units of the standard library read so far (except `System`,
    /// whose parts the analysis asks for) and reads those units again with them
    /// (`docs/spec/collections.md` 1.2): a part is read when a file of the package, the unit
    /// itself, a unit of the standard library or of an extension that uses the unit, or a part
    /// read of one of those writes a name the part declares ([`puy_stdlib::Part`]). Called once
    /// every file of the package was read.
    fn read_parts(
        &mut self,
        roots: &[&SyntaxNode],
        sources: &mut SourceMap,
        sink: &mut DiagnosticSink,
    ) {
        self.parts_chosen = true;
        let lexical: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                e.front.is_some()
                    && e.stdlib.is_some_and(|u| {
                        !u.parts.is_empty() && !u.name.eq_ignore_ascii_case("System")
                    })
            })
            .collect();
        if lexical.is_empty() {
            return;
        }
        // The names a part may be used by: those written in the files of the package, in the
        // unit itself, and in the units of the standard library and of extensions that use it
        // (their names coinciding with a part's elsewhere would read it for nothing)
        let mut user: HashSet<String> = HashSet::new();
        for root in roots {
            add_identifiers(root, &mut user);
        }
        for e in &self.entries {
            if let Some(fr) = &e.front
                && e.stdlib.is_none()
                && e.ext.is_none()
            {
                add_identifiers(&fr.root, &mut user);
            }
        }
        let uses = |units: &Self, j: usize, idx: usize| {
            units.entries[j].front.as_ref().is_some_and(|fr| {
                units.found.get(&fr.file).is_some_and(|items| {
                    items
                        .iter()
                        .any(|(_, f)| matches!(f, Found::Unit(i) if *i == idx))
                })
            })
        };
        let mut written: Vec<HashSet<String>> = Vec::new();
        for &idx in &lexical {
            let mut set = user.clone();
            for (j, e) in self.entries.iter().enumerate() {
                if let Some(fr) = &e.front
                    && (e.stdlib.is_some() || e.ext.is_some())
                    && (j == idx || uses(self, j, idx))
                {
                    add_identifiers(&fr.root, &mut set);
                }
            }
            written.push(set);
        }
        loop {
            let mut changed = false;
            for (n, &idx) in lexical.iter().enumerate() {
                let unit = self.entries[idx]
                    .stdlib
                    .expect("a unit of the standard library");
                for (k, part) in unit.parts.iter().enumerate() {
                    if self.entries[idx].parts[k]
                        || !part.names.iter().any(|name| written[n].contains(*name))
                    {
                        continue;
                    }
                    self.entries[idx].parts[k] = true;
                    changed = true;
                    // The part writes names of its own unit and of the units it uses
                    for (m, &other) in lexical.iter().enumerate() {
                        if other == idx || uses(self, idx, other) {
                            written[m].extend(part.refs.iter().map(|r| r.to_string()));
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for idx in lexical {
            self.read_again(idx, sources, sink);
        }
    }

    /// Reads the unit of the standard library `idx` again with the parts it now reads, in place
    /// of its earlier reading (the units it uses were read then).
    fn read_again(&mut self, idx: usize, sources: &mut SourceMap, sink: &mut DiagnosticSink) {
        let unit = self.entries[idx]
            .stdlib
            .expect("a unit of the standard library");
        let file = self.entries[idx]
            .front
            .as_ref()
            .expect("the unit was read")
            .file;
        let path = sources.get(file).path.clone();
        let bytes = self.stdlib_source(idx, &unit);
        sources.replace(file, SourceFile::new(path, bytes));
        self.join_included(file, idx, &unit, sources);
        let mut fr = front(file, sources, self.provider, &self.build, sink);
        fr.dir.tokens = Vec::new();
        let root = fr.root.clone();
        self.entries[idx].front = Some(fr);
        self.visit(file, &root, sources, sink);
    }

    /// Places the implicit System unit before its users, with its own dependencies ahead of it.
    fn order_system(&mut self, system: usize, post: &mut Vec<usize>) {
        post.append(&mut self.post);
        let mut dependencies = HashSet::from([system]);
        let mut pending = vec![system];
        while let Some(unit) = pending.pop() {
            if let Some(front) = &self.entries[unit].front {
                for (_, found) in self.found.get(&front.file).into_iter().flatten() {
                    if let Found::Unit(dep) = *found
                        && dependencies.insert(dep)
                    {
                        pending.push(dep);
                    }
                }
            }
        }
        let mut first = Vec::new();
        post.retain(|&unit| {
            if !dependencies.contains(&unit) {
                return true;
            }
            if unit != system {
                first.push(unit);
            }
            false
        });
        first.push(system);
        post.splice(..0, first);
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
        let found = self.unit_path(idx);
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

/// E2503: the name in the heading of a unit differs from the name of its path, or the name of a
/// unit of the package is reserved for the standard library (`docs/spec/units.md` 2.2).
#[allow(clippy::too_many_arguments)]
fn check_unit_name(
    file: FileId,
    unit: &SyntaxNode,
    path_name: &str,
    base: &str,
    stdlib: bool,
    test: bool,
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
        if !stdlib && path_lower.starts_with(&prefix.to_ascii_lowercase()) {
            let d = Diagnostic::error(
                Code::E(2503),
                format!(
                    "unit names starting with `{prefix}` are reserved for the standard library"
                ),
                Stage::Driver,
            )
            .with_primary(Span::new(file, range), "")
            .with_note(format!(
                "the unit name is the path under `{base}/` with folders joined by `.`"
            ));
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
        [one] => format!("{base}/{one}.pas"),
        [first, rest @ ..] => format!("{base}/{first}/{}.pas", rest.join(".")),
        [] => String::new(),
    };
    let mut d = Diagnostic::error(
        Code::E(2503),
        format!("unit name `{written_name}` does not match its path"),
        Stage::Driver,
    )
    .with_primary(Span::new(file, range), "")
    .with_fix(Fix::suggestion(
        format!("replace with `{path_name}`"),
        vec![TextEdit::replace(file, range, path_name)],
    ))
    .with_note(format!(
        "the unit name is the path under `{base}/` with folders joined by `.`"
    ));
    // A test unit moved to a name without `_test` would no longer be a test unit
    // (`docs/spec/units.md` 2.4)
    let last = parts.last().copied().unwrap_or_default();
    if !test || without_test_suffix(last).is_some() {
        d = d.with_note(format!("move the file to `{place}`"));
    }
    sink.push(d);
}

/// E2506: two test units of `puy test` have the same name (`docs/spec/test.md` 2). Reported at
/// the heading of the later one, in name order.
fn check_test_name(
    file: FileId,
    unit: &SyntaxNode,
    path: &str,
    names: &mut HashMap<String, String>,
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
    let name: String = written.split_whitespace().collect();
    match names.get(&name.to_ascii_lowercase()) {
        Some(first) => sink.push(
            Diagnostic::error(
                Code::E(2506),
                format!("two files map to the unit name `{name}`"),
                Stage::Driver,
            )
            .with_primary(Span::new(file, range), "")
            .with_note(format!("`{first}`"))
            .with_note(format!("`{path}`")),
        ),
        None => {
            names.insert(name.to_ascii_lowercase(), path.to_string());
        }
    }
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
    for clause in crate::syntax::nodes_of_kind(root, USES_CLAUSE) {
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
                .map(|t| crate::syntax::ident_name(t.text()).to_string())
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
    compile_with_stdlib(path, bytes, provider, opts, &[])
}

/// A test unit for [`compile_tests`].
#[derive(Debug, Clone)]
pub struct TestSource {
    /// The path shown in diagnostics (from the working directory).
    pub path: String,
    /// The path from the package root, with `/` between the parts (`None` without a root).
    pub rel: Option<String>,
    pub bytes: Vec<u8>,
}

/// What [`compile_tests`] adds to the compilation of the passed file.
#[derive(Default)]
struct TestSet<'a> {
    /// The other test units, analyzed before the passed file (the last by name).
    others: &'a [TestSource],
    /// The path from the working directory to the package root, in place of the one made from
    /// the passed file.
    prefix: Option<String>,
    /// The path of the passed file from the root, for the unit section of the module.
    main_rel: Option<String>,
}

/// The name of a test unit at `rel` (from the package root), the name of its paired unit and
/// the directory it is under (`docs/spec/units.md` 2.4, 2.5): a test unit under `tests/` has no
/// paired unit.
fn test_place(rel: &str) -> Option<(String, Option<String>, &'static str)> {
    if let Some(f) = rel.strip_prefix("src/") {
        let (name, pair) = test_unit_name_of(f)?;
        Some((name, Some(pair), "src"))
    } else {
        let f = rel.strip_prefix("tests/")?;
        let (name, _) = test_unit_name_of(f)?;
        Some((name, None, "tests"))
    }
}

/// Compiles the test units of `puy test` into one module (`docs/spec/test.md` 2): the units
/// they use are compiled once, and the tests of the units are put in the test table in the order
/// of the units' names. `prefix` is the path from the working directory to the package root
/// (`opts.package` says whether there is one). A file that is not a unit (a program or a script
/// named `*_test.pas`) is left out; `None` when no file is a unit.
pub fn compile_tests(
    tests: Vec<TestSource>,
    prefix: Option<String>,
    provider: &mut dyn SourceProvider,
    opts: &CompileOptions,
) -> Option<CompileOutput> {
    let mut named: Vec<(String, TestSource)> = Vec::new();
    for t in tests {
        let mut scratch_sources = SourceMap::new();
        let f = scratch_sources.add(SourceFile::new(t.path.clone(), t.bytes.clone()));
        let fr = front(
            f,
            &mut scratch_sources,
            provider,
            &opts.build,
            &mut DiagnosticSink::new(),
        );
        // A file that cannot be decoded is kept, to report it
        if !fr.invalid_utf8 && !fr.root.children().any(|n| n.kind() == SyntaxKind::UNIT) {
            continue;
        }
        let name = t
            .rel
            .as_deref()
            .and_then(test_place)
            .map(|(n, _, _)| n)
            .unwrap_or_else(|| {
                let file = t.path.rsplit(['/', '\\']).next().unwrap_or(&t.path);
                file.strip_suffix(".pas").unwrap_or(file).to_string()
            });
        named.push((name.to_ascii_lowercase(), t));
    }
    named.sort_by(|a, b| (&a.0, &a.1.path).cmp(&(&b.0, &b.1.path)));
    let mut sources: Vec<TestSource> = named.into_iter().map(|(_, t)| t).collect();
    let main = sources.pop()?;
    let mut opts = opts.clone();
    if let Some(p) = &mut opts.package {
        p.file = main.rel.clone();
    }
    let set = TestSet {
        others: &sources,
        prefix,
        main_rel: main.rel.clone(),
    };
    Some(compile_inner(
        &main.path,
        main.bytes,
        provider,
        &opts,
        &[],
        &set,
    ))
}

/// [`compile`], with `extra_stdlib` read as units of the standard library ahead of the table of
/// `puy-stdlib`: the tests compile sources that see the built-ins only the standard library uses
/// (`docs/spec/collections.md` 12.1) without adding them to the table.
pub(crate) fn compile_with_stdlib(
    path: &str,
    bytes: Vec<u8>,
    provider: &mut dyn SourceProvider,
    opts: &CompileOptions,
    extra_stdlib: &[puy_stdlib::Unit],
) -> CompileOutput {
    compile_inner(
        path,
        bytes,
        provider,
        opts,
        extra_stdlib,
        &TestSet::default(),
    )
}

/// A test unit of [`compile_tests`] other than the passed file.
struct OtherTest {
    front: Front,
    pair: Option<usize>,
    /// The path in the unit section of the module.
    module_path: String,
}

fn compile_inner(
    path: &str,
    bytes: Vec<u8>,
    provider: &mut dyn SourceProvider,
    opts: &CompileOptions,
    extra_stdlib: &[puy_stdlib::Unit],
    test_set: &TestSet<'_>,
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

    let mut main = front(file, &mut sources, provider, &opts.build, &mut sink);
    stages.tokens = Some(std::mem::take(&mut main.dir.tokens));
    stages.cst = Some(main.root.clone());
    let main_unit = main.root.children().find(|n| n.kind() == SyntaxKind::UNIT);
    let is_unit = main_unit.is_some();

    // The units of the package (`docs/spec/units.md` 2)
    let mut units = Units {
        provider,
        extra_stdlib,
        extensions: &opts.extensions,
        build: opts.build.clone(),
        prefix: test_set
            .prefix
            .clone()
            .or_else(|| opts.package.as_ref().map(|p| root_prefix(path, p.levels))),
        entries: Vec::new(),
        by_name: HashMap::new(),
        tests_by_name: HashMap::new(),
        tests_files: HashSet::new(),
        stdlib_by_name: HashMap::new(),
        found: HashMap::new(),
        stack: Vec::new(),
        post: Vec::new(),
        // The other test units come first, in name order (`docs/spec/test.md` 2)
        files: if test_set.others.is_empty() {
            vec![file]
        } else {
            Vec::new()
        },
        stdlib_files: Vec::new(),
        unusable: HashMap::new(),
        parts_chosen: false,
        included: HashMap::new(),
    };
    units.list();
    let main_rel = opts.package.as_ref().and_then(|p| p.file.as_deref());
    // A passed unit under `src/` (or `tests/`) is the unit of that name (a `uses` of it is a
    // cycle)
    let main_entry = main_rel
        .and_then(|f| {
            f.strip_prefix("src/")
                .map(|f| (f, false))
                .or_else(|| f.strip_prefix("tests/").map(|f| (f, true)))
        })
        .and_then(|(f, in_tests)| Some((unit_name_of(f)?, in_tests)))
        .filter(|_| is_unit)
        .and_then(|(n, in_tests)| {
            let table = if in_tests {
                &units.tests_by_name
            } else {
                &units.by_name
            };
            table.get(&n.to_ascii_lowercase()).copied()
        });
    if main_rel.is_some_and(|f| f.starts_with("tests/")) {
        units.tests_files.insert(file);
    }
    if let (Some(idx), Some(unit)) = (main_entry, &main_unit) {
        let name = units.entries[idx].name.clone();
        let base = if units.entries[idx].in_tests {
            "tests"
        } else {
            "src"
        };
        check_unit_name(file, unit, &name, base, false, false, &sources, &mut sink);
        units.entries[idx].visit = Visit::Visiting;
        units.stack.push(idx);
    }
    // A passed unit named `*_test.pas` is a test unit; under `src/` or `tests/` it has the name
    // of its path, and under `src/` it is paired with the unit named without `_test`
    // (`docs/spec/units.md` 2.4)
    let is_test = is_unit && is_test_file(path);
    let mut pair_entry: Option<usize> = None;
    // The names of the test units (`puy test`), to report two of the same name
    let mut test_names: HashMap<String, String> = HashMap::new();
    let mut others: Vec<OtherTest> = Vec::new();
    for t in test_set.others {
        let f = sources.add(SourceFile::new(t.path.clone(), t.bytes.clone()));
        units.files.push(f);
        let mut fr = front(f, &mut sources, units.provider, &opts.build, &mut sink);
        fr.dir.tokens = Vec::new();
        let place = t.rel.as_deref().and_then(test_place);
        if place.as_ref().is_some_and(|p| p.2 == "tests") {
            units.tests_files.insert(f);
        }
        let mut pair = None;
        if let Some(unit) = fr.root.children().find(|n| n.kind() == SyntaxKind::UNIT) {
            if let Some((name, p, base)) = &place {
                check_unit_name(f, &unit, name, base, false, true, &sources, &mut sink);
                pair = p
                    .as_ref()
                    .and_then(|p| units.by_name.get(&p.to_ascii_lowercase()).copied());
            }
            check_test_name(f, &unit, &t.path, &mut test_names, &sources, &mut sink);
        }
        if !fr.invalid_utf8 {
            let root = fr.root.clone();
            units.visit(f, &root, &mut sources, &mut sink);
        }
        others.push(OtherTest {
            front: fr,
            pair,
            module_path: t.rel.clone().unwrap_or_else(|| t.path.clone()),
        });
    }
    if !test_set.others.is_empty() {
        units.files.push(file);
    }
    if is_test
        && let (Some((name, pair, base)), Some(unit)) = (main_rel.and_then(test_place), &main_unit)
    {
        check_unit_name(file, unit, &name, base, false, true, &sources, &mut sink);
        pair_entry = pair.and_then(|p| units.by_name.get(&p.to_ascii_lowercase()).copied());
    }
    if !test_set.others.is_empty()
        && let Some(unit) = &main_unit
    {
        check_test_name(file, unit, path, &mut test_names, &sources, &mut sink);
    }
    if !main.invalid_utf8 {
        let root = main.root.clone();
        units.visit(file, &root, &mut sources, &mut sink);
    }
    if let Some(idx) = main_entry {
        units.entries[idx].visit = Visit::Done;
    }
    units.report_unusable(&mut sink);
    let roots: Vec<&SyntaxNode> = others
        .iter()
        .map(|o| &o.front)
        .chain(std::iter::once(&main))
        .filter(|f| !f.invalid_utf8)
        .map(|f| &f.root)
        .collect();
    units.read_parts(&roots, &mut sources, &mut sink);
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
    let mut post = std::mem::take(&mut units.post);
    // The part of `System` written in Pascal is read only when the analysis asks for a member it
    // declares or for a type it declares (`TInstant`), with the files of the helpers and types
    // asked for (`docs/spec/stdlib/System/README.md`); it is analyzed before its implicit users
    let mut system_helpers: Vec<String> = Vec::new();
    // Whether a nested routine is compiled as a closure is known only once the routine it is
    // nested in is analyzed: when the analysis finds closure-form routines it was not given, it
    // runs again with them, its first diagnostics dropped (`docs/spec/objects.md` 9.13)
    let mut sema_from = sink.len();
    let mut plan = sema::ClosurePlan::default();
    let state = loop {
        let mut state = sema::ModuleState::with_plan(std::mem::take(&mut plan));
        state.set_system_helpers(system_helpers.clone());
        for &idx in &post {
            let e = &units.entries[idx];
            let resolutions = resolutions_for(&units, e.file());
            sema::analyze(
                &unit_input(e, &resolutions, &sources, &script_name, has_root),
                &mut state,
                &mut sink,
            );
            units.entries[idx].iface = Some(state.units.len() - 1);
        }
        for o in others.iter().filter(|o| !o.front.invalid_utf8) {
            let resolutions = resolutions_for(&units, o.front.file);
            sema::analyze(
                &sema::SemaInput {
                    file: o.front.file,
                    root: &o.front.root,
                    bytes: &sources.get(o.front.file).bytes,
                    directives: &o.front.dir,
                    script_name: &script_name,
                    uses: &resolutions,
                    has_root,
                    stdlib: false,
                    extension: None,
                    test: Some(sema::TestUnit {
                        pair: o.pair.and_then(|i| units.entries[i].iface),
                    }),
                },
                &mut state,
                &mut sink,
            );
        }
        if !main.invalid_utf8 {
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
                    stdlib: false,
                    extension: None,
                    test: is_test.then(|| sema::TestUnit {
                        pair: pair_entry.and_then(|i| units.entries[i].iface),
                    }),
                },
                &mut state,
                &mut sink,
            );
        }
        // The method bodies of the standard library's types the analyzed bodies use: only the
        // units of the standard library put bodies off (`docs/spec/units.md` 6.1)
        {
            let stdlib: Vec<(&UnitEntry, Vec<sema::UsesResolution>)> = post
                .iter()
                .map(|&idx| &units.entries[idx])
                .filter(|e| e.stdlib.is_some())
                .map(|e| (e, resolutions_for(&units, e.file())))
                .collect();
            let inputs: Vec<sema::SemaInput<'_>> = stdlib
                .iter()
                .map(|(e, uses)| unit_input(e, uses, &sources, &script_name, has_root))
                .collect();
            sema::analyze_deferred(&inputs, &mut state, &mut sink);
        }
        let wanted = with_referenced_system_parts(
            state
                .wanted_system_helpers()
                .filter_map(puy_stdlib::system_part)
                .collect(),
        );
        if !wanted.is_empty()
            && let Some(idx) = units.stdlib_unit("system")
            && wanted
                .iter()
                .any(|&k| !units.entries[idx].parts[k as usize])
        {
            sink.truncate(sema_from);
            for k in wanted {
                units.entries[idx].parts[k as usize] = true;
            }
            if matches!(units.entries[idx].visit, Visit::NotRead) {
                units.read(idx, &mut sources, &mut sink);
            } else {
                units.read_again(idx, &mut sources, &mut sink);
            }
            units.order_system(idx, &mut post);
            let parts = &units.entries[idx].parts;
            system_helpers = puy_stdlib::SYSTEM_HELPER_MEMBERS
                .iter()
                .filter(|h| parts[h.part as usize])
                .map(|h| h.helper.to_ascii_lowercase())
                .chain(
                    puy_stdlib::SYSTEM_EXPORTS
                        .iter()
                        .filter(|(_, k)| parts[*k as usize])
                        .map(|(n, _)| n.to_string()),
                )
                .collect();
            sema_from = sink.len();
            plan = sema::ClosurePlan::default();
            continue;
        }
        let found = state.closure_plan();
        if !state.plan().is_empty() {
            // The second run finds the plan it was given: the calls and the values that decide
            // it do not depend on it, and the routines (whose order numbers the groups) are
            // declared in the same order
            debug_assert_eq!(&found, state.plan(), "the closure plan is stable");
            break state;
        }
        if found.is_empty() {
            break state;
        }
        sink.truncate(sema_from);
        plan = found;
    };
    let program = if main.invalid_utf8 {
        None
    } else {
        Some(sema::finish(state, &mut sink))
    };

    // Warning post-processing: `{$WARN}` state and the `disable with` hint
    let mut dirs: Vec<&Front> = vec![&main];
    dirs.extend(others.iter().map(|o| &o.front));
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

    // Lowering and code generation: a unit has no module, but a test unit has one with its tests
    // (`docs/spec/attributes.md` 5.2)
    let ir = match (&program, has_errors || (is_unit && !is_test)) {
        (Some(p), false) => {
            let files: Vec<(&DirectiveOutput, FileId)> = p
                .units
                .iter()
                .map(|u| {
                    let fr = dirs
                        .iter()
                        .find(|f| f.file == u.file)
                        .expect("each unit of the program was parsed");
                    (&fr.dir, u.file)
                })
                .collect();
            let paths: Vec<String> = p
                .units
                .iter()
                .map(|u| {
                    if u.file == file {
                        test_set
                            .main_rel
                            .clone()
                            .unwrap_or_else(|| path.to_string())
                    } else if let Some(o) = others.iter().find(|o| o.front.file == u.file) {
                        o.module_path.clone()
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
            // The distinct types are erased first (`docs/spec/types.md` 1.3)
            let erased = sema::erase::erase(p);
            Some(crate::ir::lower::lower(
                erased.as_ref().unwrap_or(p),
                &files,
                &sources,
                &paths,
            ))
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
    for d in &mut diagnostics {
        resolve_spans(d, &sources);
    }
    CompileOutput {
        module: bytes_out,
        diagnostics,
        sources,
        stages,
        is_unit,
        is_test,
        files: units
            .files
            .iter()
            .copied()
            .filter(|f| !units.stdlib_files.contains(f))
            .collect(),
    }
}

/// The input of the analysis of a visited unit: the same for its declarations and for its bodies
/// analyzed later (`sema::analyze`, `sema::analyze_deferred`).
fn unit_input<'a>(
    e: &'a UnitEntry,
    uses: &'a [sema::UsesResolution],
    sources: &'a SourceMap,
    script_name: &'a str,
    has_root: bool,
) -> sema::SemaInput<'a> {
    let fr = e.front.as_ref().expect("a visited unit was parsed");
    sema::SemaInput {
        file: fr.file,
        root: &fr.root,
        bytes: &sources.get(fr.file).bytes,
        directives: &fr.dir,
        script_name,
        uses,
        has_root,
        stdlib: e.stdlib.is_some(),
        extension: e.ext.map(|x| x.fingerprint),
        test: None,
    }
}

/// The parts of `System` in `parts` and the parts whose names they write (`TInstant` writes
/// `TTimeSpan`): `System` is analysed as one file, so a part it reads must come with the parts
/// it names, as the parts of the other units do.
fn with_referenced_system_parts(mut parts: Vec<u32>) -> Vec<u32> {
    let Some(system) = puy_stdlib::find("System") else {
        return parts;
    };
    let mut i = 0;
    while i < parts.len() {
        let refs = system.parts[parts[i] as usize].refs;
        for (k, part) in system.parts.iter().enumerate() {
            let k = u32::try_from(k).expect("System has fewer than 2^32 parts");
            if !parts.contains(&k) && part.names.iter().any(|n| refs.contains(n)) {
                parts.push(k);
            }
        }
        i += 1;
    }
    parts
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

/// The path of a file of the standard library in diagnostics and stack traces
/// (`docs/spec/collections.md` 1.2): `rel` is relative to the directory of the units.
fn stdlib_path(rel: &str) -> String {
    format!("<stdlib>/{rel}")
}

/// The path of the unit of an extension in diagnostics and stack traces
/// (`docs/spec/extensions.md` 1).
fn ext_path(unit: &str) -> String {
    format!("<ext>/{unit}.pas")
}

/// Adds the identifiers written in a file to `out`, as [`puy_stdlib::Part::names`] writes them:
/// lowercase, with `<` after one followed by `<` (`TArray<Byte>`). Words that are keywords are
/// added too (`docs/spec/collections.md` 1.2).
fn add_identifiers(root: &SyntaxNode, out: &mut HashSet<String>) {
    each_identifier(root, |key, _| {
        out.insert(key);
    });
}

/// Calls `f` with the key of each identifier written in a file ([`add_identifiers`]) and the
/// position of the identifier, in order.
fn each_identifier(root: &SyntaxNode, mut f: impl FnMut(String, TextSize)) {
    let mut last: Option<(String, TextSize)> = None;
    for t in root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if t.kind().is_trivia() {
            continue;
        }
        if let Some((mut name, at)) = last.take() {
            if t.kind() == SyntaxKind::LT {
                name.push('<');
            }
            f(name, at);
        }
        let text = t.text();
        let text = text.strip_prefix('&').unwrap_or(text);
        if text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            last = Some((text.to_ascii_lowercase(), t.text_range().start()));
        }
    }
    if let Some((name, at)) = last {
        f(name, at);
    }
}

/// Moves the spans of the diagnostics in a joined file to the files they came from
/// ([`SourceMap::resolve`]).
fn resolve_spans(d: &mut Diagnostic, sources: &SourceMap) {
    for l in d.primary.iter_mut().chain(d.secondary.iter_mut()) {
        l.span = sources.resolve(l.span);
    }
    for e in d.fixes.iter_mut().flat_map(|f| f.edits.iter_mut()) {
        let s = sources.resolve(Span::new(e.file, e.range));
        (e.file, e.range) = (s.file, s.range);
    }
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

/// The options of the crate's tests: those of a runtime, which registers the standard library's
/// extensions (`docs/design/extensions.md` 3.12).
#[cfg(test)]
pub(crate) fn test_options() -> CompileOptions {
    CompileOptions {
        extensions: puy_stdlib_ext::extensions(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit of the standard library `S` kept in its own file and three included files, as
    /// `puy-stdlib` makes it, with `a` as the first included file (`TA`) and `TB.Create` in the
    /// section `vis`.
    fn split_unit(a: &'static str, vis: &str) -> puy_stdlib::Unit {
        const OWN: [&str; 3] = [
            "unit S;\ninterface\ntype\n",
            "{$I S/A.inc}\n{$I S/B.inc}\nimplementation\n",
            "{$I S/BI.inc}\nend.\n",
        ];
        let own: &'static str = Box::leak(OWN.concat().into_boxed_str());
        let files = Box::leak(Box::new([
            puy_stdlib::File {
                path: "S.pas",
                text: own,
            },
            puy_stdlib::File {
                path: "S/A.inc",
                text: a,
            },
            puy_stdlib::File {
                path: "S/B.inc",
                text: Box::leak(
                    format!("  TB = class\n  {vis}\n    constructor Create;\n  end;\n")
                        .into_boxed_str(),
                ),
            },
            puy_stdlib::File {
                path: "S/BI.inc",
                text: "constructor TB.Create;\nbegin\nend;\n",
            },
        ]));
        let whole = |file: u32| puy_stdlib::Piece {
            file,
            start: 0,
            end: files[file as usize].text.len() as u32,
            part: puy_stdlib::NO_PART,
        };
        // The own file without its `{$I}` lines
        let head = OWN[0].len() as u32;
        let a_line = "{$I S/A.inc}\n".len() as u32;
        let b_line = "{$I S/B.inc}\n".len() as u32;
        let impl_end = head + a_line + b_line + "implementation\n".len() as u32;
        let bi_line = "{$I S/BI.inc}\n".len() as u32;
        let pieces = Box::leak(Box::new([
            puy_stdlib::Piece {
                file: 0,
                start: 0,
                end: head,
                part: puy_stdlib::NO_PART,
            },
            whole(1),
            whole(2),
            puy_stdlib::Piece {
                file: 0,
                start: head + a_line + b_line,
                end: impl_end,
                part: puy_stdlib::NO_PART,
            },
            whole(3),
            puy_stdlib::Piece {
                file: 0,
                start: impl_end + bi_line,
                end: own.len() as u32,
                part: puy_stdlib::NO_PART,
            },
        ]));
        puy_stdlib::Unit {
            name: "S",
            files,
            pieces,
            parts: &[],
        }
    }

    // A unit of the standard library kept in several files shows each file's own path and lines
    // in diagnostics, so that a change to one file does not move the lines of another
    // (`docs/spec/collections.md` 1.2).
    #[test]
    fn included_stdlib_files_keep_their_own_lines_in_diagnostics() {
        let src = "program P;\nuses S;\nvar X: TB;\nbegin\n  X := TB.Create;\nend.\n";
        for (i, a) in [
            "  TA = class\n  end;\n",
            "  TA = class\n    // more\n  end;\n",
        ]
        .into_iter()
        .enumerate()
        {
            let out = compile_with_stdlib(
                "p.pas",
                src.as_bytes().to_vec(),
                &mut crate::source::MemorySourceProvider::new(),
                &CompileOptions::default(),
                &[split_unit(a, "private")],
            );
            let shown: Vec<String> = out
                .diagnostics
                .iter()
                .map(|d| crate::diagnostics::render::render_diagnostic(d, &out.sources))
                .collect();
            assert_eq!(
                shown,
                [
                    "error[E0743]: `Create` is private to `TB`\n  --> p.pas:5:8\n   |\n 5 |   X := TB.Create;\n   |        ^^^^^^^^^\n  ::: <stdlib>/S/B.inc:3:17\n 3 |     constructor Create;\n   |                 ------ declared here"
                ],
                "{i}"
            );
        }
    }

    // The line table of a function of an included file holds that file's lines, and the
    // function names the file past the units' (`docs/spec/bytecode.md` 3.2).
    #[test]
    fn included_stdlib_files_keep_their_own_lines_in_line_tables() {
        let src = "program P;\nuses S;\nbegin\n  TB.Create.Free;\nend.\n";
        let unit = split_unit("  TA = class\n  end;\n", "public");
        let out = compile_with_stdlib(
            "p.pas",
            src.as_bytes().to_vec(),
            &mut crate::source::MemorySourceProvider::new(),
            &CompileOptions::default(),
            &[unit],
        );
        let m = out.stages.module.expect("a module");
        let f = m
            .funcs
            .iter()
            .find(|f| f.name == "S.TB.Create")
            .expect("the constructor");
        assert_eq!(m.file_path(f.file), Some("<stdlib>/S/BI.inc"));
        assert_eq!(f.lines.first(), Some(&(0, 3)));
    }

    // A change to the `class var`s of a type of the standard library the program does not use
    // does not move the program's global variables (`docs/spec/units.md` 6.2, 8.3). A unit test,
    // as a dump case of a real unit of the standard library would change with every change to it.
    #[test]
    fn unused_stdlib_class_vars_do_not_move_globals() {
        let src = "program P;\nuses S;\nvar X: TB;\nbegin\n  X := TB.Create;\n  X.Free;\nend.\n";
        for (i, a) in [
            "  TA = class\n  end;\n",
            "  TA = class\n    class var F: Integer;\n  end;\n",
            "  TA = class\n    class var F: Integer;\n    class var G: string;\n  end;\n",
        ]
        .into_iter()
        .enumerate()
        {
            let out = compile_with_stdlib(
                "p.pas",
                src.as_bytes().to_vec(),
                &mut crate::source::MemorySourceProvider::new(),
                &CompileOptions::default(),
                &[split_unit(a, "public")],
            );
            let m = out.stages.module.expect("a module");
            let globals: Vec<&str> = m.globals.iter().map(|g| g.name.as_str()).collect();
            assert_eq!(globals, ["X"], "{i}");
        }
    }

    /// A unit of the standard library `D` whose record `TR` has methods whose bodies are wrong
    /// (`Bad*` give a string to an integer) or call each other (`Calls` calls `Chain`, which calls
    /// `BadLate`), and whose class `TC` has a virtual method with a wrong body.
    const PUT_OFF_STD: puy_stdlib::Unit = puy_stdlib::Unit {
        name: "D",
        files: &[puy_stdlib::File {
            path: "D.pas",
            text: "unit D;
interface
type
  TR = record
    X: Integer;
    function Good: Integer;
    function BadUnused: Integer;
    function Calls: Integer;
    function Chain: Integer;
    function BadLate: Integer;
  end;
  TC = class
    function Virt: Integer; virtual;
    function BadStatic: Integer;
  end;
implementation
function TR.Good: Integer;
begin
  Result := X;
end;
function TR.BadUnused: Integer;
begin
  Result := 'unused';
end;
function TR.Calls: Integer;
begin
  Result := Chain;
end;
function TR.Chain: Integer;
begin
  Result := BadLate;
end;
function TR.BadLate: Integer;
begin
  Result := 'late';
end;
function TC.Virt: Integer;
begin
  Result := 'virtual';
end;
function TC.BadStatic: Integer;
begin
  Result := 'static';
end;
end.
",
        }],
        pieces: &[],
        parts: &[],
    };

    /// The errors of compiling `src` with [`PUT_OFF_STD`], as `line: message`.
    fn put_off_errors(src: &str) -> Vec<String> {
        let out = compile_with_stdlib(
            "p.pas",
            src.as_bytes().to_vec(),
            &mut crate::source::MemorySourceProvider::new(),
            &test_options(),
            &[PUT_OFF_STD],
        );
        out.diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| {
                let line = d.primary.as_ref().map_or(0, |p| {
                    let f = out.sources.get(p.span.file);
                    f.bytes[..u32::from(p.span.range.start()) as usize]
                        .iter()
                        .filter(|b| **b == b'\n')
                        .count()
                        + 1
                });
                format!("{line}: {}", d.message)
            })
            .collect()
    }

    // The body of a method of a standard library type no analyzed body uses is not analyzed, nor
    // has it a function (`docs/spec/units.md` 6.1): its error does not show, while the virtual
    // method of a class is analyzed through the class's table.
    #[test]
    fn unused_stdlib_method_bodies_are_not_analyzed() {
        let src = "program P;\nuses D;\nvar R: TR;\nbegin\n  R.X := 1;\n  WriteLn(R.Good);\nend.\n";
        assert_eq!(
            put_off_errors(src),
            vec!["39: type mismatch: expected `Integer`, found `string`".to_string()]
        );
    }

    // A body analyzed late analyzes the bodies it uses in turn (`Calls` → `Chain` → `BadLate`);
    // the unused `BadUnused` and `BadStatic` stay unanalyzed (`docs/spec/units.md` 6.1).
    #[test]
    fn stdlib_method_bodies_used_by_late_bodies_are_analyzed() {
        let src = "program P;\nuses D;\nvar R: TR;\nbegin\n  WriteLn(R.Calls);\nend.\n";
        assert_eq!(
            put_off_errors(src),
            vec![
                "35: type mismatch: expected `Integer`, found `string`".to_string(),
                "39: type mismatch: expected `Integer`, found `string`".to_string(),
            ]
        );
    }

    // A standard library type given as a type argument has all its method bodies analyzed: the
    // instantiations' bodies that may use them are made after the analysis
    // (`docs/spec/units.md` 6.1).
    #[test]
    fn stdlib_type_as_type_argument_analyzes_all_its_bodies() {
        let src = "program P;\nuses D;\ntype\n  TBox<T> = record\n    V: T;\n  end;\n\
                   var B: TBox<TR>;\nbegin\n  B.V.X := 1;\nend.\n";
        assert_eq!(
            put_off_errors(src),
            vec![
                "23: type mismatch: expected `Integer`, found `string`".to_string(),
                "35: type mismatch: expected `Integer`, found `string`".to_string(),
                "39: type mismatch: expected `Integer`, found `string`".to_string(),
            ]
        );
    }

    // A standard library class whose `LessThan` gives the default order of its descendants
    // (`docs/spec/collections.md` 10.2): a user's descendant given as a type argument has the
    // ancestor's bodies analyzed, as `<cmp:…>`, made after the analysis, calls the ancestor's
    // operator (`docs/spec/units.md` 6.1).
    #[test]
    fn stdlib_ancestor_operators_of_type_arguments_are_analyzed() {
        const ORDER_STD: puy_stdlib::Unit = puy_stdlib::Unit {
            name: "O",
            files: &[puy_stdlib::File {
                path: "O.pas",
                text: "unit O;
interface
type
  TA = class(TRefObject)
    N: Integer;
    class operator LessThan(const A, B: TA): Boolean;
  end;
implementation
class operator TA.LessThan(const A, B: TA): Boolean;
begin
  Result := A.N < B.N;
end;
end.
",
            }],
            pieces: &[],
            parts: &[],
        };
        let src = "program P;\nuses O, System.Generics.Collections;\ntype\n  TMine = class(TA)\n  end;\n\
                   var\n  L: TList<TMine>;\n  M: TMine;\n  I: Integer;\nbegin\n  L := TList<TMine>.Create;\n\
                   for I := 3 downto 1 do\n  begin\n    M := TMine.Create;\n    M.N := I;\n    L.Add(M);\n  end;\n\
                   L.Sort;\n  for M in L do\n    Write(M.N);\n  WriteLn;\n  L.Free;\nend.\n";
        let out = compile_with_stdlib(
            "p.pas",
            src.as_bytes().to_vec(),
            &mut crate::source::MemorySourceProvider::new(),
            &test_options(),
            &[ORDER_STD],
        );
        let errors: Vec<&str> = out
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(errors, Vec::<&str>::new());
        let m = out.stages.module.expect("a module");
        assert!(
            m.funcs.iter().any(|f| f.name == "O.TA.LessThan"),
            "the ancestor's operator has a function"
        );
    }

    // The path to the root keeps the passed directory and goes up past it with `../`
    // (`docs/spec/units.md` 8.2).
    /// The parts of the units of the standard library read by a compilation, by their names
    /// (`System/TStringHelper`), sorted.
    fn parts_read(out: &CompileOutput) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for f in out.sources.files() {
            if let Some((dir, file)) = f
                .path
                .strip_prefix("<stdlib>/")
                .and_then(|rest| rest.rsplit_once('/'))
            {
                let part = format!("{dir}/{}", file.split('.').next().expect("a file name"));
                if !parts.contains(&part) {
                    parts.push(part);
                }
            }
        }
        parts.sort();
        parts
    }

    // The part of `System` written in Pascal is read only for the helper members and exports
    // a program uses (`docs/spec/stdlib/System/README.md`). The compiler's source list records
    // exactly which parts were read.
    #[test]
    fn system_part_read_only_when_asked_for() {
        let helper = "type TH = record helper for string\n  function Shout: string;\nend;\n\
                      function TH.Shout: string; begin Result := Self; end;\n";
        for (idx, (src, read)) in [
            ("var S := 'ab';\nWriteLn(S.Length);".to_string(), &[][..]),
            (format!("{helper}var S := 'ab';\nWriteLn(S.Shout);"), &[]),
            ("var I := 1;\nWriteLn(I.ToString);".to_string(), &[]),
            (
                "var S := 'ab';\nWriteLn(S.Trim);".to_string(),
                &["System/TStringHelper"],
            ),
            (
                "WriteLn(TStringHelper.Join(',', ['a']));".to_string(),
                &["System/TStringHelper"],
            ),
            (
                "var S := 'ab';\nWriteLn(S.GetChars(0));".to_string(),
                &["System/TStringHelper"],
            ),
            (
                "var I := 1;\nWriteLn(I.ToHexString);".to_string(),
                &["System/IntegerHelpers"],
            ),
            // A type of `System` reads its files where it is named, unless the program declares
            // its own, and with them the parts of the names they write (`TInstant`'s operators
            // write `TTimeSpan`; `docs/spec/stdlib/System/README.md` 2)
            (
                "WriteLn(TInstant.FromUnixSeconds(1).ToString);".to_string(),
                &["System/TInstant", "System/TTimeSpan"],
            ),
            (
                "WriteLn(TTimeSpan.FromSeconds(1).ToString);".to_string(),
                &["System/TTimeSpan"],
            ),
            (
                "type TInstant = Integer;\nvar T: TInstant := 1;\nWriteLn(T);".to_string(),
                &[],
            ),
            (
                "ReportMemoryLeaksOnShutdown := True;".to_string(),
                &["System/ReportMemoryLeaksOnShutdown"],
            ),
            (
                "var ReportMemoryLeaksOnShutdown := True;\nWriteLn(ReportMemoryLeaksOnShutdown);"
                    .to_string(),
                &[],
            ),
            (
                "var S := 'ab';\nvar C := 'a';\nWriteLn(S.Trim, C.IsDigit);".to_string(),
                &["System/TCharHelper", "System/TStringHelper"],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let out = compile(
                "p.pas",
                src.into_bytes(),
                &mut crate::source::MemorySourceProvider::new(),
                &test_options(),
            );
            let system = out
                .sources
                .files()
                .iter()
                .any(|f| f.path == "<stdlib>/System.pas");
            assert_eq!(system, !read.is_empty(), "{idx}");
            assert_eq!(parts_read(&out), read, "{idx}");
        }
    }

    // A unit of the standard library reads only the parts whose names the program writes, and
    // the parts those write (`docs/spec/collections.md` 1.2): not for a name of the built-in
    // table (`IntToStr`), a name the unit's extension declares too (`CreateDir` in
    // `System.SysUtils.Native`), or `TArray<T>` (the dynamic array, not the class `TArray`).
    #[test]
    fn stdlib_parts_read_only_for_names_used() {
        for (idx, (uses, body, read)) in [
            ("System.SysUtils", "WriteLn(IntToStr(1));", &[][..]),
            (
                "System.SysUtils",
                "WriteLn(Length(TEncoding.UTF8.GetBytes('a')));",
                &["System.SysUtils/TEncoding"],
            ),
            (
                "System.SysUtils",
                "WriteLn(ExtractFileName('a/b'));",
                &["System.SysUtils/Files", "System.SysUtils/TOSVersion"],
            ),
            ("System.Generics.Collections", "WriteLn(1);", &[]),
            (
                "System.Generics.Collections",
                "var A: TArray<Integer>; begin WriteLn(Length(A));",
                &[],
            ),
            (
                "System.Generics.Collections",
                "var D: TDictionary<Integer, Integer>; begin D := nil; WriteLn(D = nil);",
                &[
                    "System.Generics.Collections/Common",
                    "System.Generics.Collections/Dictionary",
                ],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let body = if body.starts_with("var ") {
                format!("{body}\nend.\n")
            } else {
                format!("begin\n  {body}\nend.\n")
            };
            let src = format!("program P;\nuses {uses};\n{body}");
            let out = compile(
                "p.pas",
                src.into_bytes(),
                &mut crate::source::MemorySourceProvider::new(),
                &test_options(),
            );
            let errors: Vec<&Diagnostic> = out
                .diagnostics
                .iter()
                .filter(|d| d.severity == crate::diagnostics::Severity::Error)
                .collect();
            assert!(errors.is_empty(), "{idx}: {errors:?}");
            assert_eq!(parts_read(&out), read, "{idx}");
        }
    }

    // The table of the members of the helpers of `System`, which the build of `puy-stdlib` makes
    // from the source line by line, lists every method, property and constant the analysis finds
    // in them: a member missing from the table could not be reached.
    #[test]
    fn system_helper_table_matches_the_analysis() {
        // A use of one member of each helper and of each other export reads every file of `System`
        let src: String = puy_stdlib::SYSTEM_HELPER_MEMBERS
            .iter()
            .map(|h| format!("WriteLn({}.{});\n", h.helper, h.members[0]))
            .chain(
                puy_stdlib::SYSTEM_EXPORTS
                    .iter()
                    .map(|(name, _)| format!("WriteLn(SizeOf({name}));\n")),
            )
            .collect();
        let out = compile(
            "p.pas",
            src.into_bytes(),
            &mut crate::source::MemorySourceProvider::new(),
            &test_options(),
        );
        let system = puy_stdlib::find("System").expect("the unit System");
        assert_eq!(parts_read(&out).len(), system.parts.len());
        let types = &out
            .stages
            .hir
            .as_ref()
            .expect("the program was analyzed")
            .types;
        for h in puy_stdlib::SYSTEM_HELPER_MEMBERS {
            let id = (0..types.defs.len() as u32)
                .map(crate::sema::types::TypeId)
                .find(|&id| types.get(id).name.as_deref() == Some(h.helper))
                .expect("a helper of the table is a built-in helper");
            let mut found: Vec<&str> = types
                .class_methods(id)
                .iter()
                .map(|m| m.name.as_str())
                .chain(types.class_properties(id).iter().map(|p| p.name.as_str()))
                .chain(
                    types
                        .class_statics(id)
                        .iter()
                        .filter(|s| matches!(s.kind, crate::sema::types::StaticKind::Const(..)))
                        .map(|s| s.name.as_str()),
                )
                .collect();
            found.sort_unstable();
            found.dedup();
            let mut table: Vec<&str> = h.members.to_vec();
            table.sort_unstable();
            assert_eq!(found, table, "{}", h.helper);
        }
    }

    // The names of the parts of the units of the standard library, which the build of
    // `puy-stdlib` reads from the sources line by line, are the names the parser finds declared
    // at the top level of each part's files: a name missing from the table would leave a part
    // unread where a program uses it (`docs/spec/collections.md` 1.2).
    #[test]
    fn part_names_match_the_parser() {
        use SyntaxKind::*;
        fn idents(n: &SyntaxNode) -> impl Iterator<Item = String> + '_ {
            n.children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| t.kind() == IDENT)
                .map(|t| t.text().to_ascii_lowercase())
        }
        for unit in puy_stdlib::UNITS.iter().filter(|u| !u.parts.is_empty()) {
            let mut sources = SourceMap::new();
            let file = sources.add(SourceFile::new(
                unit.name,
                unit.source().into_owned().into_bytes(),
            ));
            let mut sink = DiagnosticSink::new();
            let fr = front(
                file,
                &mut sources,
                &mut crate::source::MemorySourceProvider::new(),
                &BuildConfig::default(),
                &mut sink,
            );
            assert!(!sink.has_errors(), "{}", unit.name);
            // The part of each byte of the joined source
            let mut ends: Vec<(u32, u32)> = Vec::new();
            let mut at = 0;
            for p in unit.pieces {
                at += p.end - p.start;
                ends.push((at, p.part));
            }
            let part_at = |pos: u32| {
                ends.iter()
                    .find(|(end, _)| pos < *end)
                    .map(|(_, k)| *k)
                    .expect("a position in the source")
            };
            let mut found: Vec<Vec<String>> = vec![Vec::new(); unit.parts.len()];
            let sections = fr
                .root
                .descendants()
                .filter(|n| matches!(n.kind(), INTERFACE_SECTION | IMPLEMENTATION_SECTION));
            for decl in sections.flat_map(|s| s.children().collect::<Vec<_>>()) {
                let k = part_at(decl.text_range().start().into());
                let mut keys: Vec<String> = Vec::new();
                let both = |n: String, keys: &mut Vec<String>| {
                    keys.push(format!("{n}<"));
                    keys.push(n);
                };
                match decl.kind() {
                    TYPE_SECTION => {
                        for t in decl.children().filter(|n| n.kind() == TYPE_DECL) {
                            let name = idents(&t).next().expect("a type name");
                            if t.children().any(|n| n.kind() == TYPE_PARAMS) {
                                both(name, &mut keys);
                            } else {
                                keys.push(name);
                            }
                            for e in t.children().filter(|n| n.kind() == ENUM_TYPE) {
                                for v in e.children().filter(|n| n.kind() == ENUM_VALUE) {
                                    both(idents(&v).next().expect("a value"), &mut keys);
                                }
                            }
                        }
                    }
                    CONST_SECTION | VAR_SECTION => {
                        for d in decl.children() {
                            let names: Vec<String> = idents(&d)
                                .chain(
                                    d.children()
                                        .filter(|n| n.kind() == IDENT_LIST)
                                        .flat_map(|l| idents(&l).collect::<Vec<_>>()),
                                )
                                .collect();
                            for n in names {
                                both(n, &mut keys);
                            }
                        }
                    }
                    ROUTINE_HEADER_DECL | ROUTINE_DECL => {
                        let q = decl
                            .children()
                            .find(|n| n.kind() == ROUTINE_HEADER)
                            .and_then(|h| h.children().find(|n| n.kind() == QUALIFIED_NAME))
                            .expect("a routine name");
                        let names: Vec<String> = idents(&q).collect();
                        if let [name] = names.as_slice() {
                            both(name.clone(), &mut keys);
                        }
                    }
                    _ => {}
                }
                // The unit's own file is always read
                if k == puy_stdlib::NO_PART {
                    continue;
                }
                for key in keys {
                    if !found[k as usize].contains(&key) {
                        found[k as usize].push(key);
                    }
                }
            }
            for (part, mut found) in unit.parts.iter().zip(found) {
                let mut table: Vec<String> = part.names.iter().map(|n| n.to_string()).collect();
                found.sort();
                table.sort();
                assert_eq!(table, found, "{}", part.name);
            }
            // The refs are the names of the other parts (of any unit) that the identifiers of
            // each part's files write, read as the driver reads a program's
            let mut written: Vec<HashSet<String>> = vec![HashSet::new(); unit.parts.len()];
            each_identifier(&fr.root, |key, at| {
                let k = part_at(at.into());
                if k != puy_stdlib::NO_PART {
                    written[k as usize].insert(key);
                }
            });
            for (k, part) in unit.parts.iter().enumerate() {
                let mut expected: Vec<&str> = Vec::new();
                for other in puy_stdlib::UNITS.iter().flat_map(|u| u.parts) {
                    let own = std::ptr::eq(other, part);
                    for &n in other.names {
                        if !own && written[k].contains(n) && !expected.contains(&n) {
                            expected.push(n);
                        }
                    }
                }
                let mut table: Vec<&str> = part.refs.to_vec();
                expected.sort_unstable();
                table.sort_unstable();
                assert_eq!(table, expected, "refs of {}", part.name);
            }
        }
    }

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
