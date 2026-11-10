// uses cycles, through implementation sections too, and a unit using itself (E2505;
// docs/spec/units.md 3.3).
program Main;

uses A, Loop.Back, C;

begin
  WriteLn(FromC);
end.
