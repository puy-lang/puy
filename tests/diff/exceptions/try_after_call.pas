program TryAfterCall;
// An exception propagating out of a call is not caught by a try statement that starts right after
// that call: the caller's position is the call, not its return address (docs/spec/exceptions.md 6.3).

uses System.SysUtils;

type
  EStep = class(Exception);

procedure Fail(N: Integer);
begin
  raise EStep.Create('fail ' + IntToStr(N));
end;

procedure Inner;
begin
  Fail(1);
  try
    WriteLn('not reached: try body');
    Fail(2);
  except
    on E: EStep do
      WriteLn('not reached: inner except ', E.Message);
  end;
end;

procedure InnerFinally;
begin
  Fail(3);
  try
    WriteLn('not reached: try body');
  finally
    WriteLn('not reached: inner finally');
  end;
end;

begin
  try
    Inner;
  except
    on E: EStep do
      WriteLn('outer caught ', E.Message);
  end;
  try
    InnerFinally;
  except
    on E: EStep do
      WriteLn('outer caught ', E.Message);
  end;
end.
