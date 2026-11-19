program SetFlowFix;

// The E2000 fix for a set local starts it at the empty set `[]`, not at `0`, which is not a
// set value (E0702 after the fix). Covers small and big sets, named and anonymous set types,
// an inline variable and `Result`.

type
  TColor = (Red, Green, Blue);
  TColors = set of TColor;

function Pick: TColors;
begin
  Result := [];
  Include(Result, TColor.Red);
end;

procedure Show;
var
  A: TColors;
  B: set of Byte;
  C: set of Char;
begin
  A := [];
  B := [];
  C := [];
  var D: set of Word := [];
  WriteLn(TColor.Red in A, 1 in B, 'a' in C, 2 in D, TColor.Green in Pick);
end;

begin
  Show;
end.
