# tests

End-to-end test cases. Unit tests live inside each crate. The runner is
`crates/puy-testsuite`: it embeds the cases at build time and calls `puy-core` directly,
without starting a process.

| Path | Contents |
|---|---|
| `diff/` | Programs whose expected output was recorded with Free Pascal (`{$mode delphiunicode}`) |
| `golden/` | Output fixed by Puy itself: stage dumps (`.tokens` / `.cst` / `.hir` / `.bytecode`), diagnostics (`.diag`), and behavior that intentionally differs from Delphi (`.out` / `.code`) |
| `cli/` | Cases run through the `puy` command (`crates/puy-cli/tests/cli.rs`). One directory per case; `<name>.cmd` holds the arguments |

## Running

```sh
cargo test -p puy-testsuite                       # diff/ and golden/ (also part of cargo test --workspace)
cargo test -p puy                             # cli/, and the runnable cases again through `puy run` (native only)
cargo test -p puy-testsuite -- golden_nest        # filter by name (<group>_<case>)
PUY_UPDATE_GOLDEN=1 cargo test -p puy-testsuite   # rewrite the expectations (review the diff)
```

- A case is a set of files sharing a stem: `<name>.pas` plus `.out` / `.in` / `.code` /
  `.args` / `.inc` / `.diag` / `.json` / `.err` / `.fixed.pas` / `.tokens` / `.cst` /
  `.hir` / `.bytecode`. Case names are unique across groups.
- Per-case conditions go on the first line of the `.pas` file, for example
  `// puy-test: native-only, release, define=NAME, no-console, no-env, stack-limit=64k, memory-limit=1m`.

The Free Pascal test suite is not included.
