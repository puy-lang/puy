program DumpMath;

// The hir and bytecode dumps of the floating-point functions of System (semantics.md 5.3,
// bytecode.md 2.22): Sin and the others are `MathF64` with the function's number in the
// register after the argument; an Integer argument is converted to Double first.

var
  X: Double;
  N: Integer;
begin
  X := 0.5;
  N := 2;
  WriteLn(Sin(X), Cos(N), ArcTan(X), Ln(X), Exp(X), Int(X), Frac(X));
end.
