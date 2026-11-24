// The values a failed Assert records follow its message in the uncaught report, also for an
// attached exception, and go to `properties` of the JSON report (docs/spec/exceptions.md 7.1).
type
  EWrapped = class(Exception);

function Half(N: Integer): Integer;
begin
  Result := N div 2;
end;

procedure Check(N: Integer);
begin
  Assert(Half(N) * 2 = N, 'odd');
end;

procedure Run;
begin
  try
    Check(7);
  except
    Exception.RaiseOuterException(EWrapped.Create('check failed'));
  end;
end;

WriteLn('start');
Run;
