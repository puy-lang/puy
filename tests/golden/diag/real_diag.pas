program RealDiag;

// Diagnostics of reals and Currency (types.md 8.4): no implicit conversion to an integer
// (E0702 with `Trunc` / `Round` in the notes), no explicit one (E0724), `div` / `mod` / bitwise
// operators (E0703 with a note), `X:W:D` on other types (E0717) and its `D` (E0702), Currency
// constants outside the range (E0714, also from folding), a Currency constant divided by 0
// (E0715), operators reals do not have, and `ReadLn` into a real (planned for M2).

const
  Big: Currency = 1E20;
  Max = 922337203685477;
  Over: Currency = Max + 1;
  Twice = Currency(Max) * 2;
  Zero = Currency(1) / 0;
  NotErr = 1 / 0;

var
  D: Double;
  S: Single;
  C: Currency;
  I: Integer;
  B: Byte;

begin
  I := D;
  I := 1 + C;
  B := Integer(1.5);
  I := Byte(S);
  I := Trunc(D) div 2;
  D := D div 2;
  C := C mod 2;
  I := I and D;
  D := D shl 1;
  D := not D;
  WriteLn(I:4:2);
  WriteLn('a':1:1);
  WriteLn(D:4:1.5);
  WriteLn('x' + D);
  WriteLn(D = True);
  WriteLn(Abs('a'), Trunc(True), Sqr('b'));
  C := -Currency(Max) - Currency(0.5808);
  C := -(-Currency(Max) - Currency(0.5808));
  ReadLn(D);
  for D := 1 to 2 do
    WriteLn(D);
end.
