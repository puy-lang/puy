program RealOps;

// Reals and Currency (types.md 8.4) with results FPC gives too: arithmetic and comparisons
// of Double and Single, integer division with `/`, `Trunc` / `Round` (ties to even), `Abs`,
// `Sqrt`, `X:W:D` with values that are not ties, and Currency arithmetic shown as integers.

var
  D, E: Double;
  S: Single;
  C, K: Currency;
  I: Integer;
  N: Int64;

function Half(X: Double): Double;
begin
  Result := X / 2;
end;

begin
  D := 1.25;
  E := -3.5;
  S := 0.75;
  C := 12.5;
  K := 0.0004;
  I := 7;
  N := 0;
  WriteLn(D + E:0:3, ' ', D - E:0:3, ' ', D * E:0:4, ' ', D / E:0:5);
  WriteLn(S + S:0:3, ' ', S * 4:0:2, ' ', S / 3:0:4, ' ', -S:0:2);
  WriteLn(I / 2:0:2, ' ', 1 / 8:0:3, ' ', I / 3:10:6, '|');
  WriteLn(D + I:0:2, ' ', S + D:0:2, ' ', Half(I):0:1);
  WriteLn(D < E, ' ', D > E, ' ', D = 1.25, ' ', D <> 1.25, ' ', D <= 1.25, ' ', E >= D);
  WriteLn(S < D, ' ', I > D, ' ', C = 12.5, ' ', C < I);
  WriteLn(Trunc(D), ' ', Trunc(E), ' ', Round(D), ' ', Round(E), ' ', Round(2.5), ' ', Round(-0.5));
  WriteLn(Round(1.75), ' ', Trunc(1E10), ' ', Round(S), ' ', Trunc(-S));
  WriteLn(Abs(E):0:2, ' ', Abs(-S):0:2, ' ', Sqrt(16):0:1, ' ', Sqrt(2):0:6);
  WriteLn(3.14159:0:2, ' ', 2.71828:8:3, '|', -0.38:0:1, ' ', 1234.5678:0:0, ' ', 99.99:0:1);
  C := C * 3 + K;
  WriteLn(Trunc(C * 10000), ' ', Round(C), ' ', Trunc(C));
  C := C - 37;
  WriteLn(Trunc(C * 10000), ' ', Round(C * 1000));
  K := C / 4;
  WriteLn(Trunc(K * 10000));
  C := I;
  C := C * C;
  WriteLn(Trunc(C), ' ', Round(C / 4));
  D := C;
  WriteLn(D:0:1);
  if D > 10 then
    WriteLn('big')
  else
    WriteLn('small');
  while D > 1 do
    D := D / 2;
  WriteLn(D:0:4);
  for I := 1 to 3 do
    N := N + Round(I * 1.5);
  WriteLn(N);
end.
