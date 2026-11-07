program DumpFold;

// Constant subexpressions at the head of a left-associated chain are folded too (3.6)
var
  X: Integer;
begin
  X := 1;
  X := 2 * 3 + X;
  X := X + 2 * 3;
  X := 2 * 3 - 4 + X * 2;
  WriteLn(X);
end.
