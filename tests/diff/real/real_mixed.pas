program RealMixed;

// Reals in more places (types.md 8.4) with results FPC gives too: `Pi`, reals in records,
// static and dynamic arrays and `var` parameters, integer arguments to real parameters,
// `Inc`-style accumulation in a loop, `Trunc` / `Round` of negative values, comparisons of
// mixed integer and real operands, and `X:W:D` with widths and zero decimals.

type
  TPoint = record
    X, Y: Double;
  end;

var
  P: TPoint;
  A: array[1..4] of Double;
  D: array of Single;
  I: Integer;
  Sum, M: Double;

procedure Scale(var Q: TPoint; F: Double);
begin
  Q.X := Q.X * F;
  Q.Y := Q.Y * F;
end;

function Dist(const Q: TPoint): Double;
begin
  Result := Sqrt(Q.X * Q.X + Q.Y * Q.Y);
end;

procedure Halve(var V: Double);
begin
  V := V / 2;
end;

begin
  WriteLn(Pi:0:6, ' ', 2 * Pi:10:4, '|', Pi * 1000:0:0);
  P.X := 3;
  P.Y := 4;
  WriteLn(Dist(P):0:3);
  Scale(P, 2.5);
  WriteLn(P.X:0:2, ' ', P.Y:0:2, ' ', Dist(P):0:2);
  Scale(P, 2); // an integer argument for a real parameter
  WriteLn(P.X:0:1);
  for I := 1 to 4 do
    A[I] := I / 4;
  Sum := 0;
  for I := Low(A) to High(A) do
    Sum := Sum + A[I];
  WriteLn(Sum:0:2, ' ', A[3]:6:3, '|');
  SetLength(D, 3);
  for I := 0 to High(D) do
    D[I] := I * 0.5;
  WriteLn(D[0]:0:1, ' ', D[1]:0:1, ' ', D[2] + 1:0:1, ' ', Length(D));
  M := 10;
  Halve(M);
  Halve(M);
  WriteLn(M:0:3);
  WriteLn(Trunc(-2.75), ' ', Round(-2.25), ' ', Round(-3.5), ' ', Round(-4.5), ' ', Trunc(-0.5));
  WriteLn(3 < 3.5, ' ', 4 > 3.99, ' ', 7 = 7.0, ' ', 1 / 3 = 0.3, ' ', -1 < -0.5);
  WriteLn(12.5:8:1, '|', -12.5:8:1, '|', 12.74:0:1, ' ', 0.126:0:2, ' ', 99.996:0:2);
end.
