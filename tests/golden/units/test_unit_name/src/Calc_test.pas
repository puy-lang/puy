// A test unit's name is the name of its path, as a unit's (E2503 with its fix; units.md 2.2, 2.4).
unit CalcTests;

interface

implementation

uses
  Calc;

[Test]
procedure Offsets;
begin
  Assert(Offset = 10);
end;

end.
