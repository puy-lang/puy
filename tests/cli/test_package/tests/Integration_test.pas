// A test unit under tests/ sees the public part of the units under src/ and the helper units
// under tests/ (docs/spec/units.md 2.5).
unit Integration_test;

interface

implementation

uses MyApp.Calc, Helpers.Util;

[Test]
procedure Works;
begin
  Assert(Twice(Add(1, 1)) = 4);
end;

end.
