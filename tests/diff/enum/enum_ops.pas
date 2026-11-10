program EnumOps;

// Enumeration types (types.md 8.1): qualified values, comparison by ordinal, `Ord`,
// `Succ` / `Pred`, `Low` / `High`, `Inc` / `Dec`, a `for` loop variable, a static array
// indexed by the type, the conversions to and from integers, and `WriteLn` of the names.

type
  TColor = (Red, Green, Blue);
  TLevel = (Low1 = 1, Mid = 5, High1 = 10);
  TNames = array[TColor] of string;

var
  C: TColor;
  L: TLevel;
  N: Integer;
  Names: TNames;

function Next(X: TColor): TColor;
begin
  Result := X;
  if X < High(TColor) then
    Result := Succ(X);
end;

begin
  C := TColor.Green;
  WriteLn(C);
  WriteLn(Ord(C));
  WriteLn(C > TColor.Red, ' ', C = TColor.Blue, ' ', C <> TColor.Blue, ' ', C <= TColor.Green);
  WriteLn(Succ(C), ' ', Pred(C));
  WriteLn(Low(TColor), ' ', High(TColor), ' ', Ord(High(TColor)));
  Inc(C);
  WriteLn(C);
  Dec(C, 2);
  WriteLn(C);
  for C := TColor.Red to TColor.Blue do
    WriteLn(Ord(C), ' ', C);
  for C := TColor.Blue downto TColor.Green do
    WriteLn(C);
  Names[TColor.Red] := 'rot';
  Names[TColor.Green] := 'gruen';
  Names[TColor.Blue] := 'blau';
  C := TColor.Blue;
  WriteLn(Names[C], ' ', Length(Names), ' ', Names[TColor.Red]);
  N := 1;
  C := TColor(N);
  WriteLn(C, ' ', Integer(C) + 10, ' ', TColor(2));
  WriteLn(Next(TColor.Red), ' ', Next(TColor.Blue));
  L := TLevel.Mid;
  WriteLn(Ord(L), ' ', Ord(Low(TLevel)), ' ', Ord(High(TLevel)));
  WriteLn(L > TLevel.Low1);
end.
