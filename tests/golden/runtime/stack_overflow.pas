// puy-test: stack-limit=64k
program StackOverflow;

function Down(N: Integer): Integer;
begin
  Result := Down(N + 1) + 1;
end;

begin
  WriteLn(Down(0));
end.
