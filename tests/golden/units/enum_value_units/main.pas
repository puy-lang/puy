program Main;

// A value of an enumeration type declared in a unit of `uses`, written without its type, is
// E0736 with the fix that qualifies it with the type (types.md 8.1). The scope of 8.1 includes
// the types of the units of `uses` (units.md 3.1); `puy fix` applies the fix in the program and
// in a unit that names the unit only in the `uses` of its `implementation` section. A unit used
// with an alias is reached through the alias.

uses Kinds, Impl, Colors as C;

begin
  WriteLn(Ord(kTwo));
  WriteLn(Ord(cBlue));
  WriteLn(ImplKind)
end.
