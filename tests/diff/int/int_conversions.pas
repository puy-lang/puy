program IntConversions;

// Integer types and conversions (semantics.md 2.4, 2.5, 3.2) with results FPC gives too:
// `div` / `mod` with negative operands, `Low` / `High` of each integer type, widening
// assignments, explicit narrowing casts of variables under `{$R-}` (which wrap;
// out of range constants are E0714 in Puy), `Ord` / `Chr` of ASCII, `Succ` / `Pred`,
// `Inc` / `Dec` with a step, mixed signed and unsigned operands, and IntToStr / StrToInt.

uses System.SysUtils;

var
  I, J: Integer;
  S8: ShortInt;
  U8: Byte;
  S16: SmallInt;
  U16: Word;
  U32: Cardinal;
  L: Int64;
  Q: UInt64;
  C: Char;
begin
  I := -17;
  J := 5;
  WriteLn(I div J, ' ', I mod J, ' ', -I div J, ' ', -I mod J, ' ', I div -J, ' ', I mod -J);
  WriteLn(Low(ShortInt), ' ', High(ShortInt), ' ', Low(Byte), ' ', High(Byte));
  WriteLn(Low(SmallInt), ' ', High(SmallInt), ' ', Low(Word), ' ', High(Word));
  WriteLn(Low(Integer), ' ', High(Integer), ' ', Low(Cardinal), ' ', High(Cardinal));
  WriteLn(Low(Int64), ' ', High(Int64), ' ', High(UInt64));

  // Widening keeps the value
  S8 := -100;
  U8 := 200;
  S16 := -30000;
  U16 := 60000;
  I := S8;
  L := U16;
  WriteLn(I, ' ', L, ' ', S8 + U8, ' ', S16 - U16, ' ', U8 * U16);
  U32 := 3000000000;
  L := U32;
  WriteLn(L + U32, ' ', U32 div 7, ' ', U32 mod 7);
  Q := 18000000000000000000;
  WriteLn(Q, ' ', Q div 1000, ' ', Q mod 1000);

  // Explicit casts to a narrower type wrap around under {$R-} (Puy checks them under {$R+};
  // FPC does not)
{$R-}
  I := 300;
  WriteLn(Byte(I), ' ', ShortInt(I), ' ', SmallInt(I * 200), ' ', Word(I * 300));
  J := -1;
  WriteLn(Word(J), ' ', Cardinal(J), ' ', Byte(J), ' ', Int64(J), ' ', UInt64(J));
  L := 4294967297;
  WriteLn(Integer(L), ' ', Cardinal(L), ' ', Word(L + 65535), ' ', ShortInt(L + 127));
{$R+}
  WriteLn(Integer(True), ' ', Ord(False), ' ', Ord('A'), ' ', Chr(97), ' ', Char(66));

  // Succ / Pred, Inc / Dec
  WriteLn(Succ(9), ' ', Pred(-9), ' ', Succ('a'), ' ', Pred('b'));
  I := 10;
  Inc(I, 5);
  Dec(I, 20);
  WriteLn(I);
  L := 0;
  Inc(L, 5000000000);
  Dec(L);
  WriteLn(L);
  C := 'x';
  Inc(C);
  Inc(C);
  Dec(C, 3);
  WriteLn(C);
  U8 := 10;
  Inc(U8, 245);
  WriteLn(U8);

  // IntToStr / StrToInt
  WriteLn(IntToStr(0), ' ', IntToStr(-42), ' ', IntToStr(Low(Int64)), ' ', IntToStr(High(Integer)));
  WriteLn(StrToInt('123') + 1, ' ', StrToInt('-7'), ' ', StrToInt('  42'), ' ', StrToInt('$FF'), ' ', StrToInt('+5'));
  WriteLn(StrToInt64('9000000000') * 2);

  // Operators on mixed types (`not` of a Byte or Word variable is in golden/: FPC
  // computes it in the width of the type)
  I := 7;
  U16 := 3;
  WriteLn(I - U16 * 4, ' ', (I shl 4) or U16, ' ', not 0, ' ', I xor -1, ' ', not U32);
end.
