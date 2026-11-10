program OnDispatch;
// The choice of the `on` handler, `else`, a bare `except`, raising again, the built-in
// exceptions and the members of the caught exception (docs/spec/exceptions.md 5.1, 5.2).

uses System.SysUtils;

type
  EParse = class(Exception);
  EEmpty = class(EParse);
  EOther = class(Exception) end;

procedure Fail(N: Integer);
var
  Z: Integer;
begin
  Z := 0;
  if N = 1 then
    raise EParse.Create('parse')
  else if N = 2 then
    raise EEmpty.Create('empty')
  else if N = 3 then
    raise EOther.Create('other')
  else if N = 4 then
    WriteLn(10 div Z)
  else if N = 5 then
    WriteLn(StrToInt('abc'))
  else if N = 6 then
    raise Exception.Create('plain');
end;

procedure Dispatch(N: Integer);
begin
  try
    Fail(N);
    WriteLn('no exception');
  except
    on E: EEmpty do
      WriteLn('EEmpty: ', E.Message);
    on E: EParse do
      WriteLn('EParse: ', E.Message, ' (', E.ClassName, ')');
    on E: EIntError do
      WriteLn('integer error: ', E.ClassName);
    on EConvertError do
      WriteLn('conversion failed');
  else
    WriteLn('else branch');
  end;
end;

procedure Reraise(N: Integer);
begin
  try
    try
      Fail(N);
    except
      on E: EParse do
      begin
        WriteLn('  first handler: ', E.Message);
        raise;
      end;
    end;
  except
    on E: Exception do
      WriteLn('  outer handler: ', E.ClassName, ' ', E.Message);
  end;
end;

procedure NotMatched;
begin
  try
    try
      Fail(3);
    except
      on EParse do
        WriteLn('  never');
    end;
  except
    on E: EOther do
      WriteLn('  passed through: ', E.Message);
  end;
end;

procedure Bare;
var
  S: string;
begin
  S := 'start';
  try
    Fail(6);
    S := 'never';
  except
    S := S + ' caught';
  end;
  WriteLn('  ', S);
end;

procedure Replace;
begin
  try
    try
      Fail(1);
    except
      on E: EParse do
        raise EOther.Create('replaced ' + E.Message);
    end;
  except
    on E: Exception do
      WriteLn('  ', E.ClassName, ': ', E.Message);
  end;
end;

procedure Saved;
var
  Kept: Exception;
begin
  Kept := nil;
  try
    Fail(2);
  except
    on E: EEmpty do
      Kept := E;
  end;
  WriteLn('  ', Assigned(Kept));
end;

var
  I: Integer;
begin
  for I := 0 to 6 do
    Dispatch(I);
  WriteLn('Reraise');
  Reraise(1);
  Reraise(2);
  Reraise(3);
  WriteLn('NotMatched');
  NotMatched;
  WriteLn('Bare');
  Bare;
  WriteLn('Replace');
  Replace;
  WriteLn('Saved');
  Saved;
end.
