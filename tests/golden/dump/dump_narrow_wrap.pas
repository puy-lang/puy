program DumpNarrowWrap;
{$Q-}{$R-}
// Under {$Q-} a chain of wrapping operations in a narrow computation type narrows only where
// the whole value is read (semantics.md 3.2): before div / mod, comparisons, widening to Int64,
// conversion to a real, stores, arguments. The values below overflow so that a missing
// Narrow would change the output.
var
  A, B, X: Integer;
  C, D: Cardinal;
  By: Byte;
  Ch: Char;
  L: Int64;
  U: UInt64;
  R: Double;

procedure Show(N: Integer);
begin
  WriteLn(N);
end;

begin
  A := 2000000000;
  B := 1500000000;
  C := 4000000000;
  D := 3000000000;
  // A chain: one Narrow before the store
  X := A * 3 + B - 7;
  WriteLn(X);
  // and / or / xor keep the Narrow pending
  X := (A + B) xor 5 or 1;
  WriteLn(X);
  // div and mod read the narrowed value
  X := (A + B) div 7;
  WriteLn(X);
  X := (A * 3) mod 1000;
  WriteLn(X);
  X := 1000000000 div (A + B);
  WriteLn(X);
  // div wraps too (its result may stay pending)
  X := (A + B) div 2 + A;
  WriteLn(X);
  // Comparisons
  if A + B < 0 then
    WriteLn('negative');
  WriteLn(A * 2 = -294967296);
  // The 32-bit shifts read the low bits
  X := (A + B) shr 4;
  WriteLn(X);
  X := (A + B) shl 1 + 1;
  WriteLn(X);
  // The shift amount is read modulo 32, so its Narrow may stay pending
  X := 1 shl (A + B + 3);
  WriteLn(X);
  // Cardinal: zero extension, and a pending Narrow of a shift
  WriteLn(C + D);
  WriteLn((C + D) div 3);
  WriteLn((C shl 4) div 3);
  WriteLn((C shl 4) + D > C);
  // Unary operators
  X := -(A + B) div 3;
  WriteLn(X);
  X := not (A + B) div 3;
  WriteLn(X);
  // Unary - of a Cardinal is computed in Int64
  L := -(C + D);
  WriteLn(L);
  // Narrower stores: a narrowing conversion does the job of the Narrow
  By := A + B + 7;
  WriteLn(By);
  Ch := Char(A + B + 65);
  WriteLn(Ord(Ch));
  Inc(By, A);
  WriteLn(By);
  // Widening and reals read the narrowed value
  L := A + B;
  WriteLn(L);
  L := Int64(A + B) * 2;
  WriteLn(L);
  L := L + (A + B);
  WriteLn(L);
  U := A + B;
  WriteLn(U);
  R := A + B;
  WriteLn(R:0:1);
  // Arguments and builtins
  Show(A + B);
  WriteLn(Abs(A + B));
  WriteLn(Sqr(A + B) div 2);
  // {$Q+} is unchanged: each operation is checked at once
  {$Q+}
  X := A div 1000 * 3 + 1;
  WriteLn(X);
end.
