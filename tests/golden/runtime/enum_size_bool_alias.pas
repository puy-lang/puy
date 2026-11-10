program EnumSizeBoolAlias;

// SizeOf of an enumeration is 1, 2 or 4 bytes by its largest ordinal, and of a subrange the
// smallest size holding its range (semantics.md 5.3). ByteBool / WordBool / LongBool are
// aliases of Boolean: a conversion keeps only True / False, so the ordinal is 0 or 1 (Delphi
// keeps the integer; delphi-diff.md).

type
  TSmall = (A, B, C);
  TWide = (W0, W1 = 300);
  TSub = TSmall.B..TSmall.C;
  TNeg = -200..100;
  TBig = 0..70000;

var
  L: LongBool;
  W: WordBool;
begin
  WriteLn(SizeOf(TSmall), ' ', SizeOf(TWide), ' ', SizeOf(TSub), ' ', SizeOf(TNeg), ' ', SizeOf(TBig));
  L := LongBool(2);
  W := WordBool(0);
  WriteLn(Ord(L), ' ', Ord(W), ' ', Ord(ByteBool(-1)), ' ', L = True);
end.
