program CreateFmtUncaught;

// Exception.CreateFmt (exceptions.md 2.1) is Create(Format(Fmt, Args)); its uncaught report
// carries the formatted message.

procedure Check(N: Integer);
begin
  if N > 3 then
    raise Exception.CreateFmt('%d is more than %d (%s)', [N, 3, 'limit']);
end;

begin
  Check(1);
  WriteLn('one');
  Check(5);
  WriteLn('five');
end.
