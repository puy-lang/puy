//! Entry point that connects the stages (`driver` in `docs/design/compiler-pipeline.md` 2.1).
//!
//! Lexing → directive processing → parsing → semantic analysis → code generation. Diagnostics are collected without stopping at each stage.

use crate::diagnostics::{Code, Diagnostic, DiagnosticSink, Severity, Stage, codes};
use crate::directives::{self, BuildConfig, DirectiveOutput, WarnState};
use crate::lexer;
use crate::parser::{self, ParseInput};
use crate::sema::{self, hir};
use crate::source::{FileId, SourceFile, SourceMap, SourceProvider};
use crate::syntax::SyntaxNode;

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
}

/// Result of a compilation.
pub struct CompileOutput {
    /// Bytes of the module (`.puyc`). `None` if there are errors.
    pub module: Option<Vec<u8>>,
    /// Diagnostics sorted in ascending order of position.
    pub diagnostics: Vec<Diagnostic>,
    /// Files that entered the compilation unit (used to render diagnostics).
    pub sources: SourceMap,
    /// For dumping stages (`--dump`).
    pub stages: Stages,
}

/// Per-stage results (for dumps).
#[derive(Default)]
pub struct Stages {
    pub tokens: Option<Vec<lexer::Token>>,
    pub cst: Option<SyntaxNode>,
    pub hir: Option<hir::Program>,
    pub module: Option<crate::bytecode::Module>,
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

    // Lexing
    let lexed = lexer::lex(file, sources.get(file), &mut sink, false);
    let invalid_utf8 = lexed.is_invalid_utf8();

    // Directive processing (skipped for files stopped by E0001)
    let dir: DirectiveOutput = if invalid_utf8 {
        DirectiveOutput {
            tokens: lexed.tokens.clone(),
            switches: directives::SwitchTable::new(directives::Switches::initial(&opts.build)),
            placements: Vec::new(),
            blocks: Vec::new(),
            suppress_after: None,
            include_positions: Vec::new(),
        }
    } else {
        let mut cx = directives::ProcessContext {
            config: &opts.build,
            provider,
            sources: &mut sources,
            sink: &mut sink,
            include_positions: Vec::new(),
        };
        directives::process(file, lexed.tokens, &mut cx)
    };
    stages.tokens = Some(dir.tokens.clone());

    // Parsing
    let bytes = sources.get(file).bytes.clone();
    let parsed = parser::parse(
        &ParseInput {
            file,
            bytes: &bytes,
            tokens: &dir.tokens,
            directives: &dir,
            invalid_utf8,
        },
        &mut sink,
    );
    stages.cst = Some(parsed.root.clone());

    // Semantic analysis
    let script_name = opts
        .script_name
        .clone()
        .unwrap_or_else(|| "Script".to_string());
    let program = if invalid_utf8 {
        None
    } else {
        Some(sema::analyze(
            &sema::SemaInput {
                file,
                root: &parsed.root,
                bytes: &bytes,
                directives: &dir,
                script_name: &script_name,
            },
            &mut sink,
        ))
    };

    // Warning post-processing: `{$WARN}` state and the `disable with` hint
    apply_warn_states(&mut sink, &dir);
    let has_errors = sink.has_errors();

    // Code generation
    let module = match (&program, has_errors) {
        (Some(p), false) => {
            let module_path = sources.get(file).path.clone();
            crate::codegen::generate(p, &module_path, &dir, &sources.get(file).clone(), &mut sink)
        }
        _ => None,
    };
    let module = if sink.has_errors() { None } else { module };
    let bytes_out = module.as_ref().map(|m| m.encode(crate::VERSION));
    stages.module = module;
    stages.hir = program;
    CompileOutput {
        module: bytes_out,
        diagnostics: sink.into_sorted(),
        sources,
        stages,
    }
}

/// Applies the `{$WARN}` state (OFF removes, ERROR promotes) and adds the `disable with` `notes`.
fn apply_warn_states(sink: &mut DiagnosticSink, dir: &DirectiveOutput) {
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
        // The switch table is looked up by position in the file passed to `puy` (a warning inside an included file
        // uses the state at the including `{$I}`; `directives.md` chapter 5)
        let state = match &d.primary {
            Some(p) if p.span.file == FileId(0) => {
                dir.switches.at(p.span.range.start()).warn_state(code)
            }
            Some(p) => {
                let pos = dir
                    .include_positions
                    .iter()
                    .find(|(f, _)| *f == p.span.file)
                    .map(|(_, pos)| *pos)
                    .unwrap_or(0.into());
                dir.switches.at(pos).warn_state(code)
            }
            None => dir.switches.at(0.into()).warn_state(code),
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
