// The interface section sees only its own uses; implementation declarations and the units a
// unit uses are not visible to the program (docs/spec/units.md 3.1, 4.1).
program Main;

uses Mid;

begin
  WriteLn(Total);
  WriteLn(Hidden);
  WriteLn(BaseValue);
end.
