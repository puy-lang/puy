# tests

End-to-end test cases. Unit tests live inside each crate. The runner is
`crates/puy-testsuite`: it embeds the cases at build time and calls `puy-core` directly,
without starting a process.

| Path | Contents |
|---|---|
| `diff/` | Programs whose expected output was recorded with Free Pascal (`{$mode delphiunicode}`) |
| `lifetime/` | Lifetime management: where the compiler inserts lifetime instructions (`.ir`), leak reports (`.leak`; none expected when absent), memory limits that check freeing, and the order of destructor calls |
| `golden/` | Output fixed by Puy itself: stage dumps (`.tokens` / `.cst` / `.hir` / `.ir` / `.bytecode`), diagnostics (`.diag`), and behavior that intentionally differs from Delphi (`.out` / `.code`) |
| `cli/` | Cases run through the `puy` command (`crates/puy-cli/tests/cli.rs`). One directory per case; `<name>.cmd` holds the arguments |

## Running

```sh
cargo test -p puy-testsuite                       # diff/, golden/, lifetime/ and examples/ (also part of cargo test --workspace)
cargo test -p puy                             # cli/, and the runnable cases again through `puy run` (native only)
cargo test -p puy-testsuite -- golden_nest        # filter by name (<group>_<case>)
PUY_UPDATE_GOLDEN=1 cargo test -p puy-testsuite   # rewrite the expectations (review the diff; cli/ uses -p puy)
cargo run -p puy-testsuite --bin record-fpc -- [--check] [case…]   # record diff/ outputs with Free Pascal (--check only compares)
```

- A case is a set of files sharing a stem: `<name>.pas` plus `.out` / `.in` / `.code` /
  `.args` / `.inc` / `.diag` / `.json` / `.err` / `.fixed.pas` / `.tokens` / `.cst` /
  `.hir` / `.ir` / `.bytecode` / `.leak`. Case names are unique across groups.
- Per-case conditions go on the first line of the `.pas` file, for example
  `// puy-test: native-only, release, define=NAME, no-console, no-env, stack-limit=64k, memory-limit=1m`.
- The `.out` files under `diff/` are recorded with Free Pascal 3.2.2 (`record-fpc`).

The Free Pascal test suite is not included.
