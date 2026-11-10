// Semantic errors of try..except / try..finally (docs/spec/exceptions.md 5.1-5.4, chapter 8):
// E0731, E0732, E0733, W0019, E0701 and E0707 for the exception variable.
program TryErrors;

type
  TRec = record X: Integer; end;
  EParse = class(Exception);

function Leave(N: Integer): Integer;
var
  I: Integer;
begin
  Result := 0;
  for I := 1 to N do
    try
      Result := I;
    finally
      if I = 1 then
        Continue;
      if I = 2 then
        Break;
      Exit;
    end;
end;

procedure InsideFinally;
var
  I: Integer;
begin
  try
    WriteLn('body');
  finally
    for I := 1 to 3 do
    begin
      if I = 2 then
        Continue;
      Break;
    end;
    try
      raise EParse.Create('x');
    except
      on E: EParse do
        raise;
    end;
  end;
end;

procedure Handlers;
var
  Local: Integer;
begin
  Local := 0;
  try
    WriteLn(Local);
  except
    on Exception do
      WriteLn('all');
    on EParse do
      WriteLn('parse');
    on E: TRec do
      WriteLn('record');
    on Local: Exception do
      WriteLn('shadow');
    on E: Exception do
    begin
      E := nil;
      try
        WriteLn('inner');
      except
        on E: EParse do
          WriteLn('nested');
      end;
    end;
  end;
end;

procedure Siblings;
begin
  try
    WriteLn('x');
  except
    on E: EParse do
      WriteLn(E.Message);
    on E: Exception do
      WriteLn(E.Message);
  end;
end;

begin
  raise;
  Exception.RaiseOuterException(EParse.Create('outer'));
  try
    WriteLn('x');
  except
    Exception.RaiseOuterException(EParse.Create('a'), 1);
    Exception.RaiseOuterException(42);
  end;
end.
