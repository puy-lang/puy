program ExitValueInProc;

procedure P(N: Integer);
begin
  if N = 0 then
    Exit(3);
  WriteLn(N);
end;

procedure Q;
begin
  Exit(1);
  WriteLn('never');
end;

begin
  P(1);
  Q;
end.
