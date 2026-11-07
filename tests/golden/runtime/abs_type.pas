program AbsType;

// The result type of Abs is the computation type of unary `-` (semantics.md 5.1).
// Narrow types are computed in Integer so they do not overflow; Low(Integer) raises EIntOverflow under {$Q+}

var
  S: ShortInt;
  C: Cardinal;
  L: Int64;
  I: Integer;
begin
  S := -128;
  WriteLn(Abs(S));
  C := 4000000000;
  WriteLn(Abs(C));
  L := -9223372036854775807;
  WriteLn(Abs(L));
  WriteLn(Abs(-2147483648) + 1);
  {$Q-}
  I := -2147483648;
  WriteLn(Abs(I));
  {$Q+}
  WriteLn(Abs(I));
  WriteLn('not reached');
end.
