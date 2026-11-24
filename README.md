# Puy

[![crates.io](https://img.shields.io/crates/v/puy.svg)](https://crates.io/crates/puy)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Puy is an Object Pascal implementation modeled on the feel of writing Delphi code.
It compiles Pascal source to bytecode and runs it on a stackless VM written in Rust,
aiming at single native binaries, WASI and the browser from the same source.

Puy aims to keep the feel of the Object Pascal that people have written with Delphi and
Free Pascal, while adding modern features and making it hard to write unsafe code:

- **Familiar Pascal.** Delphi's syntax, deterministic lifetime management, and the names
  of commonly used units and routines.
- **Modern features.** Newer data-type operations (dynamic arrays, slices, collections)
  follow modern languages such as Python, Go and Rust.
- **Safe by default.** Mistakes are reported as compile errors or exceptions instead of
  passing silently.
- **Written by people and coding agents.** Puy is designed with AI coding agents in mind
  as well as people: knowledge of Delphi carries over, and diagnostics come with
  suggested fixes and machine-readable output.

It does not promise compatibility with Delphi or Free Pascal, and does not follow the
ISO Pascal standards (see [Compatibility](#compatibility)).

> **Status: early development.** The current release covers the core of the language:
> records and arrays, exceptions, units, classes and interfaces, anonymous methods,
> operator overloading, generics and `System.Generics.Collections`, together with the
> first part of the standard library: strings, files and streams, paths and OS basics,
> regular expressions, math and dates, runtime type information (`TValue` and
> `System.Rtti`) and the testing framework (`puy test`) (see [`examples/`](#example)).
> Lightweight tasks and isolates, networking, JSON, HTTP, `puy build`, package
> dependencies, the formatter and running in the browser are not implemented yet; using
> a feature that is not implemented reports a "not supported yet" error (E3200).

## Compatibility

Puy follows Delphi's syntax, so everyday Object Pascal (units, records, classes,
exceptions, `try..finally`, `for..in`, and so on) reads and is written the same way.
Keeping the syntax does not mean keeping every behavior, though:

- **Safer by default.** Some rules are stricter than Delphi's so that mistakes are caught
  instead of passing silently. For example, reading a variable before it is assigned is a
  compile error, using a freed object raises an exception, typecasts are checked, and
  overflow in constant expressions is a compile error.
- **Some features are left out.** Features tied to a particular OS, CPU or memory layout
  are not supported: inline assembler (`asm`), pointers (for now), variant records and `absolute`,
  `library` / `exports` / `external`, COM and `Variant`. A few legacy constructs
  (`with`, `goto`, the old `object` type) are also removed. Using any of these is a
  compile error with a hint on what to write instead.
- **No silent changes in meaning.** Where Puy behaves differently, it prefers an error to
  quietly running ported code with a different result.

Puy also keeps an eye on Free Pascal. Programs in the common core of the language are
tested against the output of Free Pascal (`{$mode delphiunicode}`), so code written for
that mode should behave the same where Puy supports it. Free Pascal's own dialects
(`{$mode objfpc}`, `specialize`, and so on) are not accepted for now.

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
| `exceptions.pas` | Exception classes with fields and constructors, `try..except`, `try..finally` |
| `temperature.pas` | Floating point, `overload`, default parameters |
| `strings.pas` | String routines from `System.SysUtils`, UTF-16 strings |
| `nested.pas` | Nested routines that use the outer routine's variables and `Result` |
| `arrays.pas` | Static arrays with subrange and multi-dimensional indexes, typed array constants, slices, `Insert` / `Delete` |
| `invoice.pas` | `Currency`, `case` on strings, `UTF8String`, `AnsiUpperCase` / `AnsiSameText` |
| `shapes.pas` | Classes: inheritance, virtual and abstract methods, constructors and destructors, properties, `class of`, `is` / `as`, `Free` |
| `interfaces.pas` | Interfaces, `TInterfacedObject`, automatic freeing by reference counting, `Supports` |
| `tree.pas` | Reference counted classes (`TRefObject`), `[weak]` fields |
| `vectors.pas` | Records with methods, constructors, static class functions and properties |
| `closures.pas` | Procedural types, method references (`of object`) as events, anonymous methods that capture variables, nested routines as values, `TProc` |
| `helpers.pas` | Record helpers for `string`, `Integer` and an enumerated type, class helpers and their inheritance, the built-in `ToString` / `Length` |
| `fractions.pas` | Operator overloading on a record: arithmetic, comparison, `Inc` / `Dec`, `Trunc`, `Implicit` / `Explicit` |
| `collections.pas` | Default array properties overloaded by index type, slices through `TIndexRange`, `for..in` with `GetEnumerator` (record and class enumerators) |
| `generics.pas` | Generic records, classes and routines, type arguments inferred from the arguments, `TFunc` / `TPredicate` / `TProc`, class, interface and `constructor` constraints |
| `containers.pas` | `System.Generics.Collections`: `TList<T>` (slices, stable `Sort` with a comparer, `BinarySearch`), `TArray.Sort`, `TDictionary<K, V>` in insertion order, a case-insensitive comparer, `THashSet<T>` set operations, `TStack<T>` / `TQueue<T>` |
| `equality.pas` | Keys in containers: records compared field by field, a record with `Equal` / `GetHashCode` / `LessThan`, classes overriding `Equals` / `GetHashCode`, `KeyEquals` / `KeyHash` / `KeyCompare` / `HashCombine` |
| `owning.pas` | Owning containers for manually managed objects: `TObjectList<T>`, `TObjectStack<T>` / `TObjectQueue<T>`, `TObjectDictionary<K, V>`, `Extract` |
| `distinct.pas` | Distinct types (`type X = type Y`): overloads and helpers of their own, assignment compatibility with the root type |
| `todo/` | A multi-file package: `puy.toml`, units under `src/` (`interface` / `implementation`, `initialization`), `uses … as`. Run `puy run examples/todo/main.pas` |
| `log_messages.pas` | String helpers, `System.StrUtils`, Unicode character classification, and regex named groups and replacement callbacks |
| `streams.pas` | UTF-8 settings with `TStringList`, text and binary streams, and `TConsole.Output` |
| `file_workspace.pas` | Paths and file I/O in a `TMemoryFileSystem`, host capability checks, and a virtual clock with `Sleep` and `TStopwatch` |
| `delivery_slots.pas` | `System.Math` and `System.DateUtils`, time windows with `TInstant` and `TTimeSpan`, lazy initialization and seeded shuffling |
| `reflection.pas` | `TValue`, `array of const`, published properties, attributes and method invocation with `System.Rtti` and `System.TypInfo` |
| `memory_usage.pas` | Isolate and process memory usage, live heap blocks by type, `GetMemoryManagerState` and shutdown leak reporting |
| `legacy_objects.pas` | The non-generic `System.Contnrs.TObjectList` with ownership and `Extract` |
| `test_report/` | A package with a paired test unit, parameterized tests, exception checks and a committed output snapshot |

Run the report and its tests from `examples/test_report/`:

```sh
puy run main.pas
puy test
puy test --format json
```

If you built Puy from this checkout, `puy test --junit ../../target/test-report.xml` writes JUnit XML to Cargo's ignored `target/` directory.

After an intentional change to the report format, use `puy test --update` to regenerate the snapshot and review the diff.

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
| `puy run <file\|->` | Compile and run a program (`puy <file>` is the same; `-` reads the source from stdin) |
| `puy check <file\|->` | Type-check only; `--dump tokens\|cst\|hir\|ir\|bytecode` prints a compiler stage |
| `puy fix <file>` | Apply the safe fixes suggested by diagnostics |
| `puy test [paths...]` | Run the tests of the package, or of the test files given (`--filter`, `--junit`, `--update` and more) |
| `puy explain <code>` | Explain an error code (explanations are not included in this release yet) |
| `puy -e '<statements>'` | Run statements given on the command line |

Diagnostics can also be printed as JSON with `--format json`. `puy --help` lists all
options.

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
  puy-ext/             API for extensions written in Rust (no I/O)
  puy-ext-macros/      Attribute macros that declare and register extensions
  puy-fmt/             Formatter
  puy-runtime-native/  Native runtime
  puy-runtime-wasi/    WASI runtime
  puy-runtime-web/     Browser runtime
  puy-cli/             The puy command (crate `puy`)
  puy-stdlib/          Pascal sources of the standard library units (embedded; no I/O)
  puy-stdlib-ext/      The Rust parts of the standard library, written as extensions (no I/O)
  puy-testsuite/       Test runner for the cases under tests/ (not published)
tests/                 Differential, lifetime, golden and CLI test cases
examples/              Sample programs
```

## Contributing

Puy is developed in a separate repository; this repository receives a snapshot for each
release. Issues and pull requests are not accepted for now.

## License

[MIT License](LICENSE)
