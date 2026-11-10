program EnumRangeCheck;

// Under `{$R+}`, values out of the range of an enumeration or a subrange are an ERangeError
// (types.md 8.1, 8.2): `TColor(N)`, `Succ` / `Pred` / `Inc` / `Dec` of an enumeration, an
// assignment, `Inc` and `TDigit(N)` of a subrange. The message gives the value and the range
// as ordinals in decimal (`#N` for a subrange of `Char`). `Succ` of a subrange value gives
// its base type, so it is not checked against the subrange.

uses System.SysUtils;

{$R+}

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;
  TLower = 'a'..'z';
  TWarm = TColor.Red..TColor.Green;

var
  C: TColor;
  D: TDigit;
  L: TLower;
  W: TWarm;
  N: Integer;

procedure Show(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

begin
  N := 5;
  try C := TColor(N); except on E: ERangeError do Show(E); end;
  C := TColor.Blue;
  try C := Succ(C); except on E: ERangeError do Show(E); end;
  C := TColor.Red;
  try Dec(C); except on E: ERangeError do Show(E); end;
  N := 10;
  try D := N; except on E: ERangeError do Show(E); end;
  D := 9;
  try Inc(D); except on E: ERangeError do Show(E); end;
  N := -1;
  try D := TDigit(N); except on E: ERangeError do Show(E); end;
  N := Ord('A');
  try L := Chr(N); except on E: ERangeError do Show(E); end;
  C := TColor.Blue;
  try W := C; except on E: ERangeError do Show(E); end;
  D := 9;
  WriteLn(Succ(D));
  N := 12;
  D := TDigit(N);
  WriteLn('not reached');
end.
