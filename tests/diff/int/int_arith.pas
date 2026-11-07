program IntArith;

var
  A, B: Integer;
  L: Int64;
  U: Cardinal;
begin
  A := 17;
  B := 5;
  WriteLn(A + B);
  WriteLn(A - B);
  WriteLn(A * B);
  WriteLn(A div B);
  WriteLn(A mod B);
  WriteLn(A and B);
  WriteLn(A or B);
  WriteLn(A xor B);
  WriteLn(A shl 2);
  WriteLn(A shr 1);
  WriteLn(Abs(B - A));
  WriteLn(A:5, '|');
  WriteLn(-A:5, '|');
  L := 1;
  L := L shl 40;
  WriteLn(L);
  WriteLn(L * 1000);
  U := 4000000000;
  WriteLn(U);
  WriteLn(U div 3);
  Inc(A);
  Dec(B, 2);
  WriteLn(A, ' ', B);
  if A > B then
    WriteLn('greater');
  if (A = 18) and (B <> 5) then
    WriteLn('both');
  if (A < 0) or (B = 3) then
    WriteLn('either');
  if not (A = B) then
    WriteLn('not equal');
end.
