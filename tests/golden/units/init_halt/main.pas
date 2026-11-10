// `Halt` in `initialization` runs its `finally` blocks, skips the remaining `<init>` and
// `<main>`, and finalizes only the units whose `<init>` ran to its end; the exit code is
// `ExitCode` set in an earlier `initialization` (docs/spec/units.md 5.3, 5.4).
program Main;

uses First, Second, Third;

begin
  WriteLn('main');
end.
