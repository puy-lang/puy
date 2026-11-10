//! Argument parsing (`docs/spec/diagnostics.md` 10.1, 10.6; requirements §9.3).
//!
//! For `run`, shebang, and `-e`, `puy`'s own options go before the file, `-`, or `-e`; everything
//! after that is program arguments.
//! For `check` / `fix` / `explain`, options may also follow the operands.

use std::ffi::OsString;

pub const USAGE: &str = "\
Usage: puy <command> [options] [args]
       puy [options] <file> [args...]        (same as `puy run`)
       puy [options] -e '<statements>' [args...]

Commands:
  run <file|-> [args...]   Compile and run a program (`-` reads the source from stdin)
  check <file|->           Type-check only (`--dump tokens|cst|hir|ir|bytecode` prints a stage)
  fix <file>               Apply safe fixes to the file
  explain <code>           Explain an error code

Options:
  --format text|json       Output format for diagnostics (default: text)
  --color auto|always|never
  --define NAME            Define a conditional symbol (repeatable)
  --stdout-buffering auto|line|full|none
  --keep-line-endings      `fix`: keep the file's line endings
  -V, --version            Print the version
  -h, --help               Print this help
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Buffering {
    Auto,
    Line,
    Full,
    None,
}

impl Buffering {
    /// `auto` / `line` / `full` / `none` (same for `--stdout-buffering` and
    /// `PUY_STDOUT_BUFFERING`).
    pub fn parse(v: &str) -> Option<Buffering> {
        Some(match v {
            "auto" => Buffering::Auto,
            "line" => Buffering::Line,
            "full" => Buffering::Full,
            "none" => Buffering::None,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dump {
    Tokens,
    Cst,
    Hir,
    Ir,
    Bytecode,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub format: Format,
    pub color: Color,
    pub defines: Vec<String>,
    /// `--stdout-buffering`. `None` means unspecified (env var `PUY_STDOUT_BUFFERING`, else
    /// `auto`).
    pub stdout_buffering: Option<Buffering>,
    pub keep_line_endings: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            format: Format::Text,
            color: Color::Auto,
            defines: Vec::new(),
            stdout_buffering: None,
            keep_line_endings: false,
        }
    }
}

pub enum Command {
    Help,
    Version,
    Run {
        source: super::Source,
        args: Vec<String>,
    },
    Check {
        source: super::Source,
        dump: Option<Dump>,
    },
    Fix {
        file: String,
    },
    Explain {
        code: String,
    },
    /// Subcommands for M5 and later.
    Unavailable(String),
}

pub struct Parsed {
    pub command: Command,
    pub options: Options,
}

fn usage_error(msg: &str) -> String {
    format!("error: {msg}\n\n{USAGE}")
}

/// Program arguments (`ParamStr`). Invalid parts of OS strings become U+FFFD (`semantics.md` 6.2).
fn prog_arg(a: &OsString) -> String {
    a.to_string_lossy().into_owned()
}

fn to_str(a: &OsString) -> Result<String, String> {
    a.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| usage_error("argument is not valid Unicode"))
}

pub fn parse(argv: &[OsString]) -> Result<Parsed, String> {
    let mut opts = Options::default();
    let mut i = 0;
    let mut dump: Option<Dump> = None;
    let mut eval: Option<String> = None;
    let mut command: Option<String> = None;
    // Leading options and subcommand
    let mut positional: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    while i < argv.len() {
        let a = to_str(&argv[i])?;
        i += 1;
        match a.as_str() {
            "-h" | "--help" | "help" if command.is_none() && positional.is_empty() => {
                return Ok(Parsed {
                    command: Command::Help,
                    options: opts,
                });
            }
            "-V" | "--version" if command.is_none() && positional.is_empty() => {
                return Ok(Parsed {
                    command: Command::Version,
                    options: opts,
                });
            }
            "--format" => {
                let v = next_value(argv, &mut i, "--format")?;
                opts.format = match v.as_str() {
                    "text" => Format::Text,
                    "json" => Format::Json,
                    _ => {
                        return Err(usage_error(&format!(
                            "invalid value `{v}` for `--format`: expected `text` or `json`"
                        )));
                    }
                };
            }
            "--color" => {
                let v = next_value(argv, &mut i, "--color")?;
                opts.color = match v.as_str() {
                    "auto" => Color::Auto,
                    "always" => Color::Always,
                    "never" => Color::Never,
                    _ => {
                        return Err(usage_error(&format!(
                            "invalid value `{v}` for `--color`: expected `auto`, `always` or `never`"
                        )));
                    }
                };
            }
            "--define" => {
                let v = next_value(argv, &mut i, "--define")?;
                if !puy_core::directives::is_symbol_name(&v) {
                    return Err(usage_error(&format!(
                        "invalid value `{v}` for `--define`: expected a symbol name"
                    )));
                }
                opts.defines.push(v);
            }
            "--stdout-buffering" => {
                let v = next_value(argv, &mut i, "--stdout-buffering")?;
                opts.stdout_buffering = match Buffering::parse(&v) {
                    Some(b) => Some(b),
                    None => {
                        return Err(usage_error(&format!(
                            "invalid value `{v}` for `--stdout-buffering`: expected `auto`, `line`, `full` or `none`"
                        )));
                    }
                };
            }
            "--keep-line-endings" => opts.keep_line_endings = true,
            "--dump" => {
                let v = next_value(argv, &mut i, "--dump")?;
                dump = Some(match v.as_str() {
                    "tokens" => Dump::Tokens,
                    "cst" => Dump::Cst,
                    "hir" => Dump::Hir,
                    "ir" => Dump::Ir,
                    "bytecode" => Dump::Bytecode,
                    _ => {
                        return Err(usage_error(&format!(
                            "invalid value `{v}` for `--dump`: expected `tokens`, `cst`, `hir`, `ir` or `bytecode`"
                        )));
                    }
                });
            }
            "-e" => {
                let v = next_value(argv, &mut i, "-e")?;
                eval = Some(v);
                // The rest are program arguments
                while i < argv.len() {
                    rest.push(prog_arg(&argv[i]));
                    i += 1;
                }
                break;
            }
            "--" => {
                while i < argv.len() {
                    rest.push(prog_arg(&argv[i]));
                    i += 1;
                }
                break;
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(usage_error(&format!("unknown option `{s}`")));
            }
            s => {
                if command.is_none()
                    && matches!(
                        s,
                        "run"
                            | "check"
                            | "fix"
                            | "explain"
                            | "build"
                            | "test"
                            | "fmt"
                            | "init"
                            | "add"
                            | "remove"
                            | "sync"
                            | "toolchain"
                            | "bench"
                    )
                {
                    command = Some(s.to_string());
                    continue;
                }
                positional.push(s.to_string());
                // Everything after the file is program arguments (the `run` and shebang forms)
                if command.as_deref() == Some("run") || command.is_none() {
                    while i < argv.len() {
                        rest.push(prog_arg(&argv[i]));
                        i += 1;
                    }
                    break;
                }
            }
        }
    }
    if let Some(stmts) = eval {
        if command.is_some() || !positional.is_empty() {
            return Err(usage_error(
                "`-e` cannot be combined with a command or a file",
            ));
        }
        if dump.is_some() {
            return Err(usage_error("`--dump` cannot be used with `-e`"));
        }
        let mut args = rest;
        if args.first().is_some_and(|a| a == "--") {
            args.remove(0);
        }
        return Ok(Parsed {
            command: Command::Run {
                source: super::Source::Eval(stmts),
                args,
            },
            options: opts,
        });
    }
    let cmd = command.clone().unwrap_or_else(|| "run".to_string());
    match cmd.as_str() {
        "run" => {
            let Some(target) = positional.first().cloned() else {
                return Err(usage_error("`run` needs a file"));
            };
            if dump.is_some() {
                return Err(usage_error("`--dump` is only available with `check`"));
            }
            let mut args = rest;
            if args.first().is_some_and(|a| a == "--") {
                args.remove(0);
            }
            let source = if target == "-" {
                super::Source::Stdin
            } else {
                super::Source::File(target)
            };
            Ok(Parsed {
                command: Command::Run { source, args },
                options: opts,
            })
        }
        "check" => {
            if positional.len() + rest.len() != 1 {
                return Err(usage_error("`check` takes exactly one file"));
            }
            let target = positional
                .first()
                .cloned()
                .unwrap_or_else(|| rest[0].clone());
            if dump.is_some() && opts.format == Format::Json {
                return Err(usage_error("`--dump` cannot be used with `--format json`"));
            }
            let source = if target == "-" {
                super::Source::Stdin
            } else {
                super::Source::File(target)
            };
            Ok(Parsed {
                command: Command::Check { source, dump },
                options: opts,
            })
        }
        "fix" => {
            if positional.len() + rest.len() != 1 {
                return Err(usage_error("`fix` takes exactly one file"));
            }
            let target = positional
                .first()
                .cloned()
                .unwrap_or_else(|| rest[0].clone());
            if target == "-" {
                return Err(usage_error("`fix` cannot read from stdin"));
            }
            if dump.is_some() {
                return Err(usage_error("`--dump` cannot be used with `fix`"));
            }
            Ok(Parsed {
                command: Command::Fix { file: target },
                options: opts,
            })
        }
        "explain" => {
            if positional.len() + rest.len() != 1 {
                return Err(usage_error("`explain` takes exactly one error code"));
            }
            let code = positional
                .first()
                .cloned()
                .unwrap_or_else(|| rest[0].clone());
            Ok(Parsed {
                command: Command::Explain { code },
                options: opts,
            })
        }
        other => Ok(Parsed {
            command: Command::Unavailable(other.to_string()),
            options: opts,
        }),
    }
}

fn next_value(argv: &[OsString], i: &mut usize, name: &str) -> Result<String, String> {
    if *i >= argv.len() {
        return Err(usage_error(&format!("`{name}` needs a value")));
    }
    let v = to_str(&argv[*i])?;
    *i += 1;
    Ok(v)
}
