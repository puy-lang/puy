program Main;

// Units (units.md) with results FPC gives too: types, constants, variables and routines from
// the interface, fields of a unit's record variable, typed constant and parameterless
// function result read and written from the program, a qualified name that picks the unit's declaration over the program's, a
// unit variable changed through a routine, and the order of `initialization` (dependencies
// first, the interface `uses` before the implementation `uses`) and `finalization` (reverse).

uses Geometry, Counter;

function Area(const R: TRect): Integer;
begin
  Result := -1; // hides Geometry.Area here
end;

var
  R: TRect;
begin
  WriteLn('main');
  R := MakeRect(3, 4);
  WriteLn(Area(R), ' ', Geometry.Area(R), ' ', Perimeter(R), ' ', Unit_Size);
  WriteLn(Origin.W, ' ', Origin.H, ' ', Ord(DefaultKind), ' ', Kinds);
  Last := R;
  Last.H := 7;
  Geometry.Last.W := Last.W + 1;
  WriteLn(Last.W, ' ', Last.H, ' ', Area(Geometry.Last), ' ', R.H);
  WriteLn(UnitRect.H, ' ', Geometry.UnitRect.W);
  Bump;
  Bump;
  WriteLn(Count, ' ', Counter.Count);
  Count := 10;
  Bump;
  WriteLn(Count, ' ', Describe);
end.
