program Main;

// The `record type declared here` of an anonymous record declared in a unit points at the
// `record` keyword in the unit's file (diagnostics.md 2.2, types.md 1.2).

uses Shared;

var
  N: Integer;
begin
  N := Point;
end.
