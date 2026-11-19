program Main;

// The outer type of a nested type named without the unit (`TC.TI`) is looked up in the units of
// `uses` as an unqualified name: declared in two of them, it is E2502 (units.md 3.1).

uses Left, Right;

var
  I: TC.TI;
  J: Left.TC.TI;
begin
end.
