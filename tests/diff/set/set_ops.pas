program SetOps;

// Sets (types.md 8.3) with results FPC gives too: a small set of an enumeration and of a
// subrange of `Byte` (bits of an integer in Puy), and `set of Byte` (a heap block in Puy).
// Union, difference, intersection, comparisons, `in`, `Include` / `Exclude`, `for..in`,
// a set constructor with ranges and variables, `I in [1, 3, 5]`, and a set of 0..63.

type
  TColor = (Red, Green, Blue, Yellow);
  TColors = set of TColor;
  TDigit = 0..9;
  TDigits = set of TDigit;
  TBytes = set of Byte;
  T64 = set of 0..63;

procedure ShowColors(const S: TColors);
var
  C: TColor;
begin
  Write('[');
  for C in S do
    Write(' ', C);
  WriteLn(' ]');
end;

procedure ShowBytes(const S: TBytes);
var
  B: Byte;
begin
  Write('[');
  for B in S do
    Write(' ', B);
  WriteLn(' ]');
end;

var
  A, B: TColors;
  D, E: TDigits;
  X, Y: TBytes;
  I, Lo, Hi: Integer;
  C: TColor;
  F: T64;
begin
  A := [TColor.Red, TColor.Blue];
  B := [TColor.Green..TColor.Yellow];
  ShowColors(A + B);
  ShowColors(B - A);
  ShowColors(A * B);
  ShowColors([]);
  WriteLn(A = B, ' ', A <> B, ' ', A <= A + B, ' ', A >= B, ' ', [] <= A);
  WriteLn(TColor.Red in A, ' ', TColor.Green in A);
  Include(A, TColor.Green);
  Exclude(A, TColor.Red);
  Exclude(A, TColor.Yellow);
  ShowColors(A);
  C := TColor.Yellow;
  A := A + [C];
  ShowColors(A);

  D := [1, 3..5, 9];
  E := [2..4];
  Write(D + E = [1..5, 9], ' ', D - E = [1, 5, 9], ' ', D * E = [3, 4]);
  WriteLn(' ', 4 in D, ' ', 6 in D);
  for I in D * E do
    Write(I, ' ');
  WriteLn;

  Lo := 100;
  Hi := 103;
  X := [1, Lo..Hi, 255];
  Y := [0..101];
  ShowBytes(X);
  ShowBytes(X * Y);
  ShowBytes(X - Y);
  Include(X, 7);
  Exclude(X, 255);
  ShowBytes(X);
  WriteLn(X = Y, ' ', [1, 7] <= X, ' ', X >= [100..104], ' ', 255 in X, ' ', 7 in X);
  X := D;
  ShowBytes(X);

  I := 3;
  WriteLn(I in [1, 3, 5], ' ', I + 1 in [1, 3, 5]);
  I := -1;
  WriteLn(I in [1, 3, 5]);

  // All 64 bits of the largest small set
  Lo := 0;
  Hi := 63;
  F := [Lo..Hi];
  Hi := 0;
  for I in F do
    Inc(Hi);
  WriteLn(Hi, ' ', 63 in F, ' ', 0 in F);
  Exclude(F, 63);
  for I in F * [60..63] do
    Write(I, ' ');
  WriteLn;
end.
