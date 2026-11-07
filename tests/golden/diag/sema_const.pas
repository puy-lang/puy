program SemaConst;
const
  A = 1 div 0;
  B = 1 mod 0;
  C = 300;
  D: Byte = C;
  E = 'x' + 'y';
  F = High(Integer) + 1;
  G = Byte(300);
  H = Byte(-1);
var
  N: Integer;
  By: Byte;
  Sh: ShortInt;
begin
  N := 1;
  const K = N;
  By := N;
  By := 300;
  Sh := 200;
  By := Byte(N);
  By := Byte(256);
  N := N div 0;
  N := N mod 0;
  N := 1 div (N - N);
  WriteLn(A, B, C, D, E, F, G, H, K, By, Sh);
end.
