program DumpIncDecTry;

// `Inc` / `Dec` of a local variable inside a `try` body (semantics.md 3.1, 5.1; M4-49): when a
// check after the addition may raise, the value is made in a temporary and moved to the
// variable; when nothing may raise, it is made in the variable as outside `try`.

procedure P;
var
  I: Integer;
  B: Byte;
begin
  I := 0;
  B := 5;
  try
    {$Q-}
    // No check is emitted for `Integer`: made in I
    Inc(I);
    {$Q+}
    // NarrowOvf may raise after the addition: a temporary, then Move
    Inc(I);
    // The range check of `Byte` may raise: a temporary, then Move
    Dec(B, 2);
  finally
    WriteLn(I, ' ', B);
  end;
end;

begin
  P;
end.
