// ===== AssertSnapshot (docs/spec/stdlib/Puy.Testing/README.md chapter 5) =====

// Compares Actual with the snapshot of the running test stored in the snapshots folder next to
// the test unit, and raises EAssertionFailed (with a diff) when they differ or there is none yet;
// `puy test --update` writes it instead. The version without Name numbers the unnamed snapshots
// of a test in the order of the calls. Outside `puy test`, ENotSupportedException.
procedure AssertSnapshot(const Actual: string); overload;
procedure AssertSnapshot(const Actual: string; const Name: string); overload;
