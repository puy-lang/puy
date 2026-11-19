program Main;

// The fix for an unqualified value of an enumeration type of a unit (types.md 8.1) names the
// type as it can be written here: with the unit when the type name alone does not name it
// (declared in two units, E2502 in units.md 3.1, or hidden by a declaration of the program), and
// one suggestion per type when several types have a value of that name.

uses Left, Right;

type
  THidden = Integer;

begin
  WriteLn(Ord(kTwo));
  WriteLn(Ord(hOne));
  WriteLn(Ord(sOnly))
end.
