program EnumRangeOff;

// Under `{$R-}` nothing is checked and the ordinal is kept as is (types.md 8.1, 8.2): `Ord`
// shows it and `WriteLn` writes it in decimal when no value has it. A value whose ordinal
// several values share is written with the name declared first; an ordinal between the
// declared values is in the type's range but has no name.

{$R-}

type
  TColor = (Red, Green, Blue);
  TCode = (First = 1, Second = 5, Third, Again = 5);
  TDigit = 0..9;

var
  C: TColor;
  K: TCode;
  D: TDigit;
  N: Integer;

begin
  C := TColor.Blue;
  C := Succ(C);
  WriteLn(Ord(C), ' ', C);
  C := TColor.Red;
  Dec(C);
  WriteLn(Ord(C), ' ', C);
  N := 7;
  C := TColor(N);
  WriteLn(C);
  K := TCode.Again;
  WriteLn(K, ' ', TCode.Third, ' ', Ord(TCode.Third));
  N := 3;
  K := TCode(N);
  WriteLn(K, ' ', Ord(Low(TCode)), ' ', Ord(High(TCode)));
  WriteLn(K:4, '|', TCode.First:8, '|');
  N := 42;
  D := N;
  WriteLn(D);
  D := 9;
  Inc(D);
  WriteLn(D);
end.
