program IntImmediates;
{$Q-}{$R-}

// Integer operations and comparisons with a constant operand, which codegen turns into the
// instructions with an immediate (MulIntI, JmpIfLtIntI, ...): constants at the edges of the
// 8-bit immediate (-128, 127) and just outside it (-129, 128), on either side, with negative
// operands, and `for` loops counting up and down to the edges of their ranges

var
  A, B, R: Integer;
  L: Int64;
  W: Cardinal;
  I, N: Integer;

procedure Line(V: Int64);
begin
  Write(V, ' ');
end;

begin
  A := -1000;
  B := 999;
  Line(A + 127); Line(A + 128); Line(A - 128); Line(A - 127); Line(A - 129);
  Line(A * -128); Line(A * 127); Line(-128 * A); Line(127 * B);
  Line(A div 7); Line(A mod 7); Line(A div -7); Line(A mod -7); Line(B div -128);
  Line(B mod 127); Line(A mod -128);
  WriteLn;
  Line(A and 127); Line(A and -128); Line(-128 and A); Line(A or 15); Line(15 or A);
  Line(A xor -1); Line(-1 xor B); Line(A xor 127);
  Line(A shl 3); Line(A shr 3); Line(B shl 31); Line(A shr 31); Line(A shl 0);
  WriteLn;
  L := -5000000000;
  Line(L * 3); Line(L div 9); Line(L mod 9); Line(L and 255); Line(L shl 7); Line(L shr 60);
  W := 4000000000;
  Line(W shr 4); Line(W div 7); Line(W mod 7); Line(W and 127);
  WriteLn;
  R := 0;
  for I := -130 to 130 do
  begin
    if I < -128 then Inc(R);
    if I <= -128 then Inc(R, 10);
    if I > 127 then Inc(R, 100);
    if I >= 127 then Inc(R, 1000);
    if I = 0 then Inc(R, 10000);
    if I <> 5 then Inc(R, 100000);
    if -1 < I then Inc(R, 7);
    if 3 >= I then Inc(R, 11);
    if 128 = I then Inc(R, 13);
  end;
  WriteLn(R);
  N := 0;
  for I := 5 downto -3 do
    N := N * 3 + I;
  WriteLn(N);
  N := 0;
  for I := MaxInt - 2 to MaxInt do
    N := N + 1;
  for I := -MaxInt - 1 + 2 downto -MaxInt - 1 do
    N := N + 10;
  WriteLn(N);
end.
