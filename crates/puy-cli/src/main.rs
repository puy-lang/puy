//! The `puy` command (requirements §9; per-entry output and exit codes are in
//! `docs/spec/diagnostics.md` chapter 10).
//!
//! Uses `puy-runtime-native` natively and `puy-runtime-wasi` on `wasm32-wasip2`
//! (`docs/design/host-interface.md` 3.6). Argument parsing and the loop are shared.

mod args;
mod metadata;
mod output;

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use args::{Command, Format, Options};
use puy_core::bytecode::Module;
use puy_core::diagnostics::{self, Code, Diagnostic, Severity, Stage};
use puy_core::driver::{CompileOptions, compile};
use puy_core::vm::{ExitKind, IsolateConfig, StdoutBuffering};

#[cfg(not(target_os = "wasi"))]
use puy_runtime_native as runtime;
#[cfg(target_os = "wasi")]
use puy_runtime_wasi as runtime;

#[cfg(not(target_os = "wasi"))]
#[global_allocator]
static ALLOC: runtime::CountingAlloc = runtime::CountingAlloc;

fn main() -> ExitCode {
    let argv: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let parsed = match args::parse(&argv) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    install_panic_hook(parsed.options.format == Format::Json);
    // Run compilation and execution on a thread with a large stack (nesting limit 256 vs. the
    // Rust stack; `docs/design/compiler-pipeline.md` 3.11)
    let code = run_on_big_stack(move || dispatch(parsed));
    ExitCode::from(code)
}

#[cfg(not(target_os = "wasi"))]
fn run_on_big_stack(f: impl FnOnce() -> u8 + Send + 'static) -> u8 {
    match std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
    {
        Ok(h) => h.join().unwrap_or(101),
        Err(_) => f_fallback(),
    }
}

#[cfg(not(target_os = "wasi"))]
fn f_fallback() -> u8 {
    101
}

#[cfg(target_os = "wasi")]
fn run_on_big_stack(f: impl FnOnce() -> u8 + Send + 'static) -> u8 {
    f()
}

/// Panic hook: prints `fatal[F0001]: internal error: ...` and exits with 101
/// (§9.3, `diagnostics.md` 4.9).
fn install_panic_hook(json: bool) {
    std::panic::set_hook(Box::new(move |info| {
        let msg = match info.payload().downcast_ref::<&str>() {
            Some(s) => (*s).to_string(),
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => s.clone(),
                None => "panic".to_string(),
            },
        };
        let location = info
            .location()
            .map(|l| format!(" ({}:{})", l.file(), l.line()))
            .unwrap_or_default();
        let text = format!("internal error: {msg}{location}");
        let mut err = std::io::stderr().lock();
        if json {
            let _ = writeln!(
                err,
                "{{\"schema\":\"puy.diagnostics/1\",\"diagnostics\":[{{\"code\":\"F0001\",\"severity\":\"fatal\",\"name\":null,\"message\":{},\"primary\":null,\"secondary\":[],\"notes\":[],\"fixes\":[],\"rendered\":{}}}],\"summary\":{{\"errors\":1,\"warnings\":0,\"notes\":0}}}}",
                diagnostics::json::json_string(&text),
                diagnostics::json::json_string(&format!("fatal[F0001]: {text}"))
            );
        } else {
            let _ = writeln!(err, "fatal[F0001]: {text}");
        }
        let _ = err.flush();
        std::process::exit(101);
    }));
}

/// Source input.
enum Source {
    File(String),
    Stdin,
    Eval(String),
}

fn dispatch(parsed: args::Parsed) -> u8 {
    let opts = parsed.options;
    match parsed.command {
        Command::Help => {
            print!("{}", args::USAGE);
            0
        }
        Command::Version => {
            println!("puy {}", puy_core::VERSION);
            0
        }
        Command::Run {
            source,
            args: prog_args,
        } => run(source, prog_args, &opts),
        Command::Check { source, dump } => check(source, dump, &opts),
        Command::Fix { file } => fix(&file, &opts),
        Command::Explain { code } => explain(&code),
        Command::Unavailable(name) => {
            eprintln!("puy: `{name}` is not available in this release");
            2
        }
    }
}

/// Reads the source. Failure is F0002 (10.5).
fn read_source(
    source: &Source,
    opts: &Options,
    to_stdout: bool,
) -> Result<(String, Vec<u8>, Option<String>), u8> {
    let (path, bytes, script_name) = match source {
        Source::File(p) => match runtime::read_source_file(p) {
            Ok(b) => (p.clone(), b, script_name_of(p)),
            Err(e) => {
                let d = Diagnostic::new(
                    Code::F(2),
                    Severity::Error,
                    format!("cannot read `{p}`: {}", e.fixed_text()),
                    Stage::Driver,
                );
                output::emit_standalone(&d, opts, to_stdout);
                return Err(2);
            }
        },
        Source::Stdin => match runtime::read_stdin_all() {
            Ok(b) => ("<stdin>".to_string(), b, None),
            Err(e) => {
                let d = Diagnostic::new(
                    Code::F(2),
                    Severity::Error,
                    format!("cannot read `<stdin>`: {}", e.fixed_text()),
                    Stage::Driver,
                );
                output::emit_standalone(&d, opts, to_stdout);
                return Err(2);
            }
        },
        Source::Eval(text) => ("<eval>".to_string(), text.as_bytes().to_vec(), None),
    };
    Ok((path, bytes, script_name))
}

/// Program name of a script: the file name (without extension) if it is a valid identifier
/// (`semantics.md` 1.1).
fn script_name_of(path: &str) -> Option<String> {
    let stem = std::path::Path::new(path).file_stem()?.to_str()?;
    let b = stem.as_bytes();
    let ident = !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        && puy_core::syntax::SyntaxKind::reserved_word(&stem.to_ascii_lowercase()).is_none();
    ident.then(|| stem.to_string())
}

fn compile_options(
    path: &str,
    bytes: &[u8],
    script_name: Option<String>,
    opts: &Options,
) -> CompileOptions {
    let mut co = CompileOptions {
        build: puy_core::directives::BuildConfig {
            release: false,
            test: false,
            defines: opts.defines.clone(),
            ..Default::default()
        },
        extra_diagnostics: Vec::new(),
        script_name,
        extra_files: Vec::new(),
    };
    // The driver checks inline metadata (E2507)
    let (diags, files) = metadata::check(path, bytes);
    co.extra_diagnostics = diags;
    co.extra_files = files;
    co
}

fn run(source: Source, prog_args: Vec<String>, opts: &Options) -> u8 {
    let (path, bytes, script_name) = match read_source(&source, opts, false) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let co = compile_options(&path, &bytes, script_name, opts);
    let mut provider = runtime::FileSourceProvider;
    let out = compile(&path, bytes, &mut provider, &co);
    let has_errors = out.diagnostics.iter().any(|d| d.is_error());
    if !out.diagnostics.is_empty() {
        output::emit_diagnostics(
            &out.diagnostics,
            &out.sources,
            opts,
            false,
            matches!(source, Source::File(_)),
        );
    }
    if has_errors {
        return 1;
    }
    let Some(bytes) = out.module else { return 1 };
    let module = match Module::decode(&bytes, puy_core::VERSION) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            output::emit_fatal("F0004", &e.to_string(), opts);
            return 101;
        }
    };
    // Runtime settings: command arguments, else env var `PUY_STDOUT_BUFFERING` (errors are F0006;
    // `diagnostics.md` 10.1)
    let buffering = match opts.stdout_buffering {
        Some(b) => b,
        None => match std::env::var_os("PUY_STDOUT_BUFFERING") {
            None => args::Buffering::Auto,
            Some(v) => match v.to_str().and_then(args::Buffering::parse) {
                Some(b) => b,
                None => {
                    output::emit_fatal(
                        "F0006",
                        &format!(
                            "invalid runtime setting `PUY_STDOUT_BUFFERING`: expected `auto`, `line`, `full` or `none`, found `{}`",
                            v.to_string_lossy()
                        ),
                        opts,
                    );
                    return 101;
                }
            },
        },
    };
    let buffering = match buffering {
        args::Buffering::Auto => {
            if runtime::stdout_is_terminal() {
                StdoutBuffering::Line
            } else {
                StdoutBuffering::Full
            }
        }
        args::Buffering::Line => StdoutBuffering::Line,
        args::Buffering::Full => StdoutBuffering::Full,
        args::Buffering::None => StdoutBuffering::None,
    };
    let config = IsolateConfig {
        stdout_buffering: buffering,
        ..Default::default()
    };
    let mut argv = vec![match &source {
        Source::File(p) => p.clone(),
        _ => String::new(),
    }];
    argv.extend(prog_args);
    match runtime::run_module(module, runtime::host_set(), config, argv) {
        Ok(runtime::RunResult::Exit(status)) => {
            if let ExitKind::UncaughtException(report) = &status.kind {
                output::emit_exception(report, opts);
            }
            if let Some(e) = &status.stdout_error {
                output::emit_stdout_error(e, opts);
            }
            status.exit_code()
        }
        Ok(runtime::RunResult::Fatal(e)) => {
            output::emit_fatal("F0001", &format!("internal error: {}", e.message), opts);
            101
        }
        Err(e) => {
            let code = match e {
                puy_core::vm::IsolateError::HostFunction(_) => "F0005",
                _ => "F0001",
            };
            output::emit_fatal(code, &e.to_string(), opts);
            101
        }
    }
}

fn check(source: Source, dump: Option<args::Dump>, opts: &Options) -> u8 {
    let (path, bytes, script_name) = match read_source(&source, opts, true) {
        Ok(x) => x,
        Err(code) => return code,
    };
    let co = compile_options(&path, &bytes, script_name, opts);
    let mut provider = runtime::FileSourceProvider;
    let out = compile(&path, bytes, &mut provider, &co);
    let has_errors = out.diagnostics.iter().any(|d| d.is_error());
    match dump {
        Some(args::Dump::Tokens) => {
            if let Some(tokens) = &out.stages.tokens {
                let bytes = &out.sources.get(puy_core::source::FileId(0)).bytes;
                print!("{}", puy_core::lexer::dump_tokens(tokens, bytes));
            }
        }
        Some(args::Dump::Cst) => {
            if let Some(cst) = &out.stages.cst {
                print!("{}", puy_core::parser::dump_cst(cst));
            }
        }
        Some(args::Dump::Hir) => {
            if let Some(p) = &out.stages.hir {
                let src = out.sources.get(puy_core::source::FileId(0));
                print!("{}", puy_core::sema::dump::dump_hir(p, src));
            }
        }
        Some(args::Dump::Bytecode) => {
            if !has_errors && let Some(m) = &out.stages.module {
                print!("{}", puy_core::bytecode::disassemble(m));
            }
        }
        None => {}
    }
    let _ = std::io::stdout().flush();
    output::emit_diagnostics(
        &out.diagnostics,
        &out.sources,
        opts,
        true,
        matches!(source, Source::File(_)),
    );
    if has_errors { 1 } else { 0 }
}

fn fix(file: &str, opts: &Options) -> u8 {
    let (path, bytes, script_name) = match read_source(&Source::File(file.to_string()), opts, true)
    {
        Ok(x) => x,
        Err(code) => return code,
    };
    let co = compile_options(&path, &bytes, script_name, opts);
    let mut provider = runtime::FileSourceProvider;
    let result = puy_core::fix::fix(
        &path,
        bytes.clone(),
        &mut provider,
        &co,
        opts.keep_line_endings,
    );
    let mut written = false;
    if !result.applied.is_empty() {
        let to_write = if opts.keep_line_endings {
            result.bytes.clone()
        } else {
            puy_core::fix::normalize_newlines(&result.bytes)
        };
        if let Err(e) = std::fs::write(&path, &to_write) {
            let he = puy_host::HostError::from_io(&e);
            let d = Diagnostic::new(
                Code::F(2),
                Severity::Error,
                format!("cannot write `{path}`: {}", he.fixed_text()),
                Stage::Driver,
            );
            output::emit_standalone(&d, opts, true);
            return 2;
        }
        written = true;
    }
    let has_errors = result.diagnostics.iter().any(|d| d.is_error());
    output::emit_fix_result(&result, &path, written, opts);
    if has_errors { 1 } else { 0 }
}

fn explain(code: &str) -> u8 {
    // Codes not in the registry (including reserved numbers) are usage errors
    // (`diagnostics.md` 10.2)
    let Some(c) = Code::parse(code)
        .filter(|c| diagnostics::codes::lookup(*c).is_some_and(|i| i.milestone.is_some()))
    else {
        eprintln!("error: unknown error code `{code}`");
        return 2;
    };
    match diagnostics::explain::lookup(c) {
        Some(text) => print!("{text}"),
        None => println!("no explanation is available for {c} yet"),
    }
    0
}
