program UnusedValueTry;

uses System.SysUtils;

// W0002 and `try`: an exception may leave the body at any statement, so an assignment in the
// body counts as read if the handlers, the `finally` block or the code after the `try` read it.

procedure ReadInHandler(const S: string);
var
  N: Integer = 0;
begin
  try
    N := 1;
    N := StrToInt(S);
    WriteLn(N);
  except
    WriteLn(N);
  end;
end;

procedure ReadInFinally(const S: string);
var
  N: Integer = 0;
begin
  try
    N := 1;
    N := StrToInt(S);
  finally
    WriteLn(N);
  end;
end;

procedure ReadAfter(const S: string);
var
  N: Integer;
begin
  N := -1;
  try
    N := StrToInt(S);
  except
    on E: EConvertError do
      WriteLn('bad');
  end;
  WriteLn(N);
end;

// Reported: nothing reads the value after the body, even on an exception
procedure NotRead(const S: string);
var
  N: Integer;
begin
  try
    N := 1;
    N := StrToInt(S);
    WriteLn(N);
  except
    WriteLn('bad');
  end;
end;

begin
  ReadInHandler('1');
  ReadInFinally('2');
  ReadAfter('x');
  NotRead('3');
end.
