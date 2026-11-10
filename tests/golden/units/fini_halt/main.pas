// `Halt(n)` in `finalization` runs its `finally` blocks, ends that unit's `<fini>` and goes on
// with the remaining units; the exit code is `n` (docs/spec/units.md 5.4).
program Main;

uses Low, High;

begin
  WriteLn('main');
end.
