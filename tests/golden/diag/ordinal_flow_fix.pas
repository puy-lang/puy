program OrdinalFlowFix;

// The E2000 fix for an enumeration or subrange local starts it at the type's lowest value
// (qualified by the type's name when it has one), not at `0`, which may not be a value of the
// type (E0702 / E0714 after the fix).

type
  TColor = (Red, Green, Blue);
  TLevel = (Low = 3, High = 7);
  TDigit = 5..9;
  TLetter = 'a'..'z';
  TWarm = TColor.Green..TColor.Blue;

procedure Show;
var
  C: TColor;
  L: TLevel;
  D: TDigit;
  T: TLetter;
  W: TWarm;
begin
  WriteLn(Ord(C), Ord(L), D, T, Ord(W));
end;

begin
  Show;
end.
