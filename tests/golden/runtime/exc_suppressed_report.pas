// An exception raised in a `finally` block while another propagates carries the first in
// `Suppressed`, and `RaiseOuterException` attaches `InnerException`; the uncaught report shows
// both, nested (docs/spec/exceptions.md 5.4, 7.1).
uses System.SysUtils;

type
  EParseError = class(Exception);
  EIOError = class(Exception);

procedure ReadPort;
begin
  WriteLn(StrToInt('x'));
end;

procedure Parse;
begin
  try
    ReadPort;
  except
    Exception.RaiseOuterException(EParseError.Create('bad input'));
  end;
end;

procedure Close;
begin
  try
    Parse;
  finally
    raise EIOError.Create('connection reset');
  end;
end;

WriteLn('start');
Close;
