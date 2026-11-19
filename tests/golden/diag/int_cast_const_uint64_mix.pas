program IntCastConstUInt64Mix;

// `UInt64(1)` is a UInt64 constant, so mixing it with a signed operand is E0716 as for a
// UInt64 variable (semantics.md 3.2); it no longer adapts to the signed type. So is any signed
// constant whose type was written (Integer(1), High(Integer)); a constant folded from literals
// (K, the Integer 1) adapts to UInt64 like the literal (M4-136)

const
  K = 4000000000 - 3999999999;

var
  U: UInt64;
  V: Integer;
  B: Boolean;

begin
  V := 1;
  WriteLn(V + UInt64(1));
  B := V < UInt64(2);
  WriteLn(B);
  U := 1;
  WriteLn(U + SmallInt(1));
  WriteLn(U + High(ShortInt));
  WriteLn(U + Integer(1));
  WriteLn(U + High(Integer));
  WriteLn(U + K);
end.
