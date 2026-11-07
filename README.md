# Puy

[![crates.io](https://img.shields.io/crates/v/puy-cli.svg)](https://crates.io/crates/puy-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Puy is an Object Pascal implementation modeled on the feel of writing Delphi code.
It compiles Pascal source to bytecode and runs it on a stackless VM written in Rust,
aiming at single native binaries, WASI and the browser from the same source.

Puy keeps the syntax of Delphi and its deterministic lifetime management, and takes
newer data-type operations (dynamic arrays, slices, collections) from modern languages.
It does not promise compatibility with Delphi or Free Pascal, and does not follow the
ISO Pascal standards. Where Puy differs, it tries not to silently change the meaning of
ported code.

> **Status: early development.** The current release covers integers, strings,
> `if` / `while`, procedures and functions, and `WriteLn`. Records, arrays, exceptions,
> units, classes and the standard library are not implemented yet; using them reports
> a "not supported yet" error (E3200).

## Example

```pascal
program Hello;

begin
  WriteLn('Hello, Puy!');
end.
```

```sh
$ puy run hello.pas
Hello, Puy!
```

## Building

Puy requires the Rust toolchain pinned in `rust-toolchain.toml` (rustup installs it
automatically).

```sh
cargo build --workspace
cargo run -p puy-cli -- run examples/hello.pas
cargo run -p puy-cli -- --help
```

The `puy` command:

| Command | Description |
|---|---|
| `puy run <file>` | Compile and run a program (`puy <file>` is the same) |
| `puy check <file>` | Type-check only; `--dump tokens\|cst\|hir\|bytecode` prints a compiler stage |
| `puy fix <file>` | Apply the safe fixes suggested by diagnostics |
| `puy -e '<statements>'` | Run statements given on the command line |

### WebAssembly

```sh
# WASI (run with a WASI runtime such as wasmtime)
cargo build -p puy-cli --target wasm32-wasip2

# Browser (build only for now)
cargo build -p puy-core -p puy-runtime-web --target wasm32-unknown-unknown
```

## Testing

```sh
cargo test --workspace
```

The end-to-end cases live under `tests/` (see [tests/README.md](tests/README.md)).
Each release is built and tested on Linux, Windows and macOS (x86_64 and aarch64 each)
and on `wasm32-wasip2` before it is published here.

## Repository layout

```
crates/
  puy-core/            Lexer, parser (CST), semantic analysis, code generation, bytecode, VM
                       (no I/O; builds for WASM)
  puy-host/            Host interface traits
  puy-fmt/             Formatter
  puy-runtime-native/  Native runtime
  puy-runtime-wasi/    WASI runtime
  puy-runtime-web/     Browser runtime
  puy-cli/             The puy command
  puy-testsuite/       Test runner for the cases under tests/ (not published)
tests/                 Differential, golden and CLI test cases
examples/              Sample programs
```

## Contributing

Puy is developed in a separate repository; this repository receives a snapshot for each
release. Issues and pull requests are not accepted for now.

## License

[MIT License](LICENSE)
