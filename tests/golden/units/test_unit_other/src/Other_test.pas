// Only the unit named like the test unit without `_test` is paired with it (units.md 2.4): to
// `Other_test`, `Calc` shows its public part only, as to any other unit (E0700, E0743).
unit Other_test;

interface

implementation

uses
  Calc;

[Test]
procedure Offsets;
var
  C: TCounter;
begin
  C := TCounter.Create;
  Assert(C.FCount = 0);
  C.Free;
  Assert(Offset = 10);
end;

end.
