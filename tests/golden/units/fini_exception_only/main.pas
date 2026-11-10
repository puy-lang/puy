// Only `finalization` fails: its uncaught exception alone turns exit code 0 into 1, and a later
// `Halt(0)` does not clear the failure. An exception leaving a `finally` while `Halt` unwinds in
// the finishing stage is reported after the `<fini>` exceptions (docs/spec/units.md 5.4).
program Main;

uses Outer;

begin
  WriteLn('main');
end.
