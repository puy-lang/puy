// `Exit(value)` in `initialization` / `finalization` is E0711, as in `<main>`, and `Result`
// cannot be used there (docs/spec/units.md 5.1).
unit UnitInitExit;

interface

implementation

initialization
  Exit(1);
finalization
  Result := 2;
end.
