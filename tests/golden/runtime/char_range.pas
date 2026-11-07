program CharRange;

// The type name for out-of-range `Chr` / `Inc(C)` is `Char` (2.5, 5.1)
var
  I: Integer;
  C: Char;
begin
  I := 65535;
  C := Chr(I);
  WriteLn(Ord(C));
  Inc(C);
  WriteLn(Ord(C));
end.
