program IntWrap32;
{$Q-}{$R-}

// Integer +, - and * under {$Q-} are the 32-bit operations that wrap and narrow at once
// (AddInt32, SubInt32, MulInt32 and the immediate forms AddInt32I, MulInt32I): results across
// MaxInt / MinInt, operands whose Narrow is still pending (unary -, not), Inc / Dec, and
// results stored into narrower types. The results are passed as Integer: FPC computes an
// Integer expression read as Int64 (or compared) in 64 bits on 64-bit targets, unlike Puy

var
  A, B, X, I: Integer;
  By: Byte;
  Sm: SmallInt;

procedure Line(V: Integer);
begin
  Write(V, ' ');
end;

begin
  A := MaxInt;
  B := -MaxInt - 1;
  Line(A + 1); Line(A + A); Line(B - 1); Line(B + B); Line(A - B); Line(B - A);
  Line(A * 2); Line(A * A); Line(B * B); Line(B * -1); Line(A * -3); Line(A * 127);
  Line(1 + A); Line(-1 + B); Line(3 * A); Line(A + 127); Line(B - 128); Line(A - (-128));
  WriteLn;
  // Operands whose Narrow is pending: -MinInt, not (no MinInt div -1: it traps in native code)
  Line(-B + 0); Line(-B + 1); Line(-B - 1); Line(-B * 3); Line(not A + 1); Line(not B * 2);
  Line((A + 1) * (A + 1)); Line((B - 1) * 3 + 5);
  WriteLn;
  X := A;
  Inc(X); Line(X);
  Dec(X); Line(X);
  Inc(X, 1000); Line(X);
  Dec(X, 2000000000); Line(X);
  X := B;
  Inc(X, -1); Line(X);
  Dec(X, -200); Line(X);
  Inc(X, A); Line(X);
  WriteLn;
  // Stored into narrower types
  By := A + 2; Line(By);
  Sm := A * 3; Line(Sm);
  By := B - 1 + A * 2; Line(By);
  WriteLn;
  // A chain in a loop (the benchmark intloop's shape)
  X := 1;
  for I := 1 to 1000 do
    X := (X * 1103515245 + 12345) and $7FFFFFFF;
  WriteLn(X);
  X := 7;
  for I := 1 to 100 do
    X := X * 31 + I - 5;
  WriteLn(X);
end.
