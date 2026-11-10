program StderrThenUncaught;

// What the program writes to `ErrOutput` comes before the report of the uncaught exception on
// standard error (semantics.md 6.1, 7.2).

var
  Z: Integer;
begin
  WriteLn('out');
  WriteLn(ErrOutput, 'err ', 1);
  Z := 0;
  WriteLn(1 div Z);
end.
