// The paired unit is seen only through `uses` (no implicit `uses`; units.md 2.4): without it,
// neither its public part nor its `implementation` section is visible (E0700).
unit Calc_test;

interface

implementation

[Test]
procedure Offsets;
begin
  Assert(Offset = 10);
end;

end.
