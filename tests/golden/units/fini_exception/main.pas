// An uncaught exception in `finalization` is reported and the remaining units are still
// finalized; the exit code is 1. The program's own uncaught exception is reported first
// (docs/spec/units.md 5.4).
program Main;

uses Outer;

begin
  WriteLn('main');
  raise Exception.Create('from main');
end.
