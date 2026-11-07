program AssertFail;

var
  N: Integer;
begin
  N := 3;
  Assert(N > 0);
  WriteLn('first ok');
  Assert(N > 5, 'N must exceed 5');
  WriteLn('unreachable');
end.
