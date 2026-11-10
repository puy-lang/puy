// A unit without a tree (E0001) gives the error type to its names; a unit with errors keeps its
// interface section (docs/spec/units.md 3.4).
program Main;

uses Bad, Rough;

begin
  WriteLn(Name, Missing);
  WriteLn(Good, Broken);
  WriteLn(Other);
end.
