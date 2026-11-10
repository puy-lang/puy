program SystemBuiltins;

// The System built-ins of M2b (semantics.md 5.3): Odd, Sqr, MaxInt, Low / High of a string,
// SizeOf of the types with a memory layout (enumerations are
// in golden/: FPC sizes them differently), UpCase, and the aliases WideChar, UCS4Char,
// ByteBool, WordBool and LongBool.

type
  TDigit = 0..9;

var
  I: Integer;
  N: Int64;
  S: string;
  C: WideChar;
  U: UCS4Char;
  B: ByteBool;
  W: WordBool;
  L: LongBool;
begin
  for I := -3 to 3 do
    Write(Odd(I), ' ');
  WriteLn;
  N := -7;
  WriteLn(Odd(N), ' ', Odd(Int64(10)));
  WriteLn(Sqr(12), ' ', Sqr(-5));
  WriteLn(MaxInt, ' ', MaxInt = High(Integer));
  S := 'hello';
  WriteLn(Low(S), ' ', High(S));
  S := '';
  WriteLn(Low(S), ' ', High(S));
  WriteLn(SizeOf(ShortInt), ' ', SizeOf(Byte), ' ', SizeOf(SmallInt), ' ', SizeOf(Word));
  WriteLn(SizeOf(Integer), ' ', SizeOf(Cardinal), ' ', SizeOf(Int64), ' ', SizeOf(UInt64));
  WriteLn(SizeOf(Boolean), ' ', SizeOf(Char), ' ', SizeOf(Single), ' ', SizeOf(Double));
  WriteLn(SizeOf(Currency), ' ', SizeOf(TDigit), ' ', SizeOf(I));
  WriteLn(UpCase('a'), UpCase('z'), UpCase('A'), UpCase('1'), UpCase('{'));
  C := 'q';
  WriteLn(UpCase(C));
  U := 65;
  WriteLn(U, ' ', SizeOf(U), ' ', SizeOf(WideChar));
  B := True;
  W := False;
  L := B and not W;
  WriteLn(B, ' ', W, ' ', L);
end.
