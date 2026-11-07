program UInt64Range;

// Assignments between UInt64 and Int64 are range-checked under {$R+} (semantics.md 2.4)

var
  U: UInt64;
  L: Int64;
begin
  U := 9223372036854775807;
  L := U;
  WriteLn(L);
  {$R-}
  U := 18446744073709551615;
  L := U;
  WriteLn(L);
  {$R+}
  L := U;
  WriteLn('not reached');
end.
