# Puy

[![crates.io](https://img.shields.io/crates/v/puy.svg)](https://crates.io/crates/puy)
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

## Installation

Install the `puy` command from crates.io with Cargo (Rust 1.99 or later):

```sh
cargo install puy
```

To build it from a checkout of this repository instead, see [Building](#building).

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

More samples are in [`examples/`](examples/):

| File | Shows |
|---|---|
| `fizzbuzz.pas` | Script mode (no `program` header), `for var` |
| `primes.pas` | Dynamic arrays, `for..in`, `Write` with a width |
| `records.pas` | Records, value semantics, `var` and open-array parameters, `Format` |
| `enums_sets.pas` | Enumerated types, sets, `case` |
| `exceptions.pas` | Custom exception classes, `try..except`, `try..finally` |
| `temperature.pas` | Floating point, `overload`, default parameters |
| `strings.pas` | String routines from `System.SysUtils`, UTF-16 strings |
| `nested.pas` | Nested routines that use the outer routine's variables and `Result` |
| `arrays.pas` | Static arrays with subrange and multi-dimensional indexes, typed array constants, slices, `Insert` / `Delete` |
| `invoice.pas` | `Currency`, `case` on strings, `UTF8String`, `AnsiUpperCase` / `AnsiSameText` |
| `todo/` | A multi-file package: `puy.toml`, units under `src/` (`interface` / `implementation`, `initialization`), `uses … as`. Run `puy run examples/todo/main.pas` |

## Building

Puy requires the Rust toolchain pinned in `rust-toolchain.toml` (rustup installs it
automatically).

```sh
cargo build --workspace
cargo run -p puy -- run examples/hello.pas
cargo run -p puy -- --help
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
cargo build -p puy --target wasm32-wasip2

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
  puy-cli/             The puy command (crate `puy`)
  puy-testsuite/       Test runner for the cases under tests/ (not published)
tests/                 Differential, golden and CLI test cases
examples/              Sample programs
```

## Contributing

Puy is developed in a separate repository; this repository receives a snapshot for each
release. Issues and pull requests are not accepted for now.

## License

[MIT License](LICENSE)
