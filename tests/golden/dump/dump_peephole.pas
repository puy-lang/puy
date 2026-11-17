program DumpPeephole;
{$Q-}{$R-}

// The rules codegen applies to the IR before laying it out (design/vm.md 3.15): immediates in
// integer operations and compare-and-branch, the moves around an operation on a variable, a
// global loaded right after it is stored, and the end of a `for` iteration

var
  G: Integer;

function Calc(N: Integer): Integer;
var
  I, K: Integer;
  D: Double;
begin
  Result := 0;
  K := N * 3;
  K := K + 1;
  K := K - 2;
  K := (K shr 2) and 255;
  K := K mod 7 + K div 3;
  K := 5 * K;
  D := N;
  D := D * D;
  if K < 10 then
    Inc(Result);
  if 10 < K then
    Inc(Result, 2);
  if K = 0 then
    Inc(Result, 3);
  if K <> 1000 then
    Inc(Result, 4);
  for I := 1 to N do
    Result := Result + I;
  for I := N downto 1 do
    Result := Result - 1;
  Result := Result + Trunc(D);
end;

begin
  G := Calc(20);
  G := G + 1;
  WriteLn(G, ' ', G and 1);
end.
