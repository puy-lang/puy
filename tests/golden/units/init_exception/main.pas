// An uncaught exception in `initialization` ends the program: the failing unit and the units
// after it are not finalized, the units before it are (docs/spec/units.md 5.3, 5.4).
program Main;

uses Good, Bad;

begin
  WriteLn('main');
end.
