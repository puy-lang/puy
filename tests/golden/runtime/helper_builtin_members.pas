program HelperBuiltinMembers;

// The members of the built-in helpers of `System` (objects.md 10.7): `ToString` of the integer
// types (as `IntToStr`), the real types and `Currency` (as `FloatToStr`), `Boolean` (`True` /
// `False`, not the `TRUE` / `FALSE` of `WriteLn`), `Char` and an enumeration (the value's name);
// `Length` of `string` (UTF-16 code units), `UTF8String` (bytes) and dynamic arrays. A subrange
// uses its base type's helper. `TIntegerHelper.ToString(X)` names the helper; the values are
// not folded, so a constant receiver works too.

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;
  TNames = array of string;

const
  Answer = 42;

var
  B: Byte;
  I: Integer;
  U: UInt64;
  D: Double;
  Sg: Single;
  Cu: Currency;
  Flag: Boolean;
  Ch: Char;
  Col: TColor;
  Dg: TDigit;
  S: string;
  U8: UTF8String;
  Names: TNames;
  Nums: TArray<Integer>;
begin
  B := 255;
  I := -12;
  U := 18446744073709551615;
  WriteLn(B.ToString, ' ', I.ToString, ' ', U.ToString, ' ', Answer.ToString);
  WriteLn(TIntegerHelper.ToString(I * 2), ' ', TByteHelper.ToString(B));
  D := 2.5;
  Sg := 0.25;
  Cu := 12.34;
  WriteLn(D.ToString, ' ', Sg.ToString, ' ', Cu.ToString);
  Flag := True;
  WriteLn(Flag.ToString, ' ', (not Flag).ToString, ' ', Flag);
  Ch := 'x';
  Col := TColor.Green;
  Dg := 7;
  WriteLn(Ch.ToString, ' ', Col.ToString, ' ', Dg.ToString);
  S := 'héllo😀';
  U8 := 'héllo';
  WriteLn(S.Length, ' ', U8.Length, ' ', 'abc'.Length);
  SetLength(Names, 4);
  Nums := [1, 2, 3];
  WriteLn(Names.Length, ' ', Nums.Length);
end.
