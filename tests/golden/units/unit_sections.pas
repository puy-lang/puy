// The `hir` dump of a unit shows `<init>` (the initial values, then the `initialization`
// statements) and `<fini>` (docs/spec/semantics.md 9.1, docs/spec/units.md 5.1).
unit UnitSections;

interface

var
  Count: Integer = 1;

implementation

initialization
  Count := Count + 1;
finalization
  Count := 0;
end.
