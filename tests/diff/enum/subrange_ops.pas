program SubrangeOps;

// Subrange types (types.md 8.2): values take part in operations as their base type, an
// assignment in range keeps the value, `Low` / `High` are the bounds, `Succ` gives the base
// type, and subranges of `Char` and of an enumeration work the same way.

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;
  TLower = 'a'..'z';
  TWarm = TColor.Red..TColor.Green;
  TNeg = -5..-1;

var
  D: TDigit;
  L: TLower;
  W: TWarm;
  G: TNeg;
  N, Sum: Integer;
  Counts: array[TDigit] of Integer;

procedure Twice(X: TDigit);
begin
  WriteLn(X * 2);
end;

begin
  D := 7;
  N := D + 5;
  WriteLn(D, ' ', N, ' ', D * D, ' ', D div 2);
  WriteLn(Low(TDigit), ' ', High(TDigit), ' ', Low(D), ' ', High(D));
  D := 5;
  WriteLn(Succ(D), ' ', Pred(D));
  D := 3;
  Inc(D, 4);
  WriteLn(D);
  Sum := 0;
  for D := Low(TDigit) to High(TDigit) do
    Sum := Sum + D;
  WriteLn(Sum);
  for D := 0 to 9 do
    Counts[D] := D * D;
  WriteLn(Counts[3], ' ', Counts[9]);
  N := 4;
  D := N;
  Twice(D);
  Twice(5);
  L := 'q';
  WriteLn(L, ' ', Ord(L), ' ', Succ(L), ' ', L < 'r');
  W := TColor.Green;
  WriteLn(W, ' ', Ord(W), ' ', W = TColor.Green);
  G := -3;
  WriteLn(G, ' ', Low(TNeg), ' ', G + 10);
end.
