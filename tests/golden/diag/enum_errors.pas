program EnumErrors;

// Errors of enumeration and subrange types (types.md 8.1, 8.2): duplicate values, bounds,
// no implicit conversion with integers (E0702), no arithmetic (E0703), constants out of
// range (E0714, the ordinal in decimal), an unknown value, an enumeration or subrange
// written where only a type name is allowed, and a wrong index type.

type
  TColor = (Red, Green, Blue);
  TFruit = (Apple, Pear, Apple);
  TBig = (Huge = 2147483648);
  TBad = 5..1;
  TBadChar = 'z'..'a';
  TMix = 1..'z';
  TDigit = 0..9;
  TLower = 'a'..'z';
  TNames = array[TColor] of string;

var
  C: TColor;
  D: TDigit;
  L: TLower;
  N: Integer;
  Names: TNames;

procedure P(X: 0..9);
begin
end;

function F: (A, B);
begin
end;

procedure TakeInt(var X: Integer);
begin
end;

begin
  C := 1;
  N := C;
  N := C + 1;
  C := -C;
  C := TColor.Purple;
  D := TDigit(12);
  D := 10;
  L := 'A';
  C := TColor(3);
  N := Ord(Succ(TColor.Blue));
  N := Ord(Pred(Low(TColor)));
  Names[0] := 'x';
  Names[TColor(5)] := 'y';
  TakeInt(D);
  C := TColor('a');
end.
