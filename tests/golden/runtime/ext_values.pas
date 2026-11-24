// puy-test: test-ext
// Values passed to and returned from the functions of an extension (`docs/spec/extensions.md` 3):
// ordinals, reals, strings in either representation, UTF8String and TBytes.
program ExtValues;

uses TestExt;

var
  B: TArray<Byte>;
  R: TArray<Byte>;
  I: Integer;
  S: string;
begin
  WriteLn(Add(2, 40));
  WriteLn(AddInt64(High(Int64), 1));
  WriteLn(Half(5.0):0:2);
  WriteLn(HalfSingle(3.0):0:2);
  WriteLn(Negate(False));
  WriteLn(NextChar('a'));
  WriteLn(Inc1(-1));
  WriteLn(Upper('hello, world'));
  WriteLn(Upper('héllo'));
  WriteLn(Utf8Len('abc'), ' ', Utf8Len('日本語'));
  WriteLn(Utf16Len('日本語'), ' ', Utf16Len(''));
  WriteLn(Utf8Echo('ユニ'));
  SetLength(B, 4);
  for I := 0 to 3 do
    B[I] := I * 10 + 1;
  WriteLn(Checksum(B));
  R := Reverse(B);
  for I := 0 to High(R) do
    Write(R[I], ' ');
  WriteLn;
  WriteLn(Length(Reverse(nil)));
  // A UTF-16 string read as UTF-8 again after it changed
  S := '日本';
  S := S + '語';
  WriteLn(Utf8Len(S), ' ', Utf8Len(S), ' ', Utf8Echo(S));
  S[1] := 'x';
  WriteLn(Utf8Len(S), ' ', Utf8Echo(S));
  S := S + 'é';
  WriteLn(Utf8Len(S), ' ', Utf8Echo(S));
end.
