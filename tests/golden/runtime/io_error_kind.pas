program IOErrorKind;

// EInOutError.ErrorCode and ErrorKind, ENotSupportedException.ErrorKind
// (spec/exceptions.md 2.2): read and write, so that a program makes the exception of a failure
// of its own; ekNone in a new exception and in the errors that do not come from the host
// (a failed ReadLn).

uses
  System.SysUtils;

var
  E: EInOutError;
  N: ENotSupportedException;
  I: Integer;
begin
  E := EInOutError.Create('cannot open');
  WriteLn(E.ErrorCode, ' ', E.ErrorKind);
  E.ErrorCode := 13;
  E.ErrorKind := TIOErrorKind.ekPermissionDenied;
  try
    raise E;
  except
    on X: EInOutError do
      WriteLn(X.Message, ' ', X.ErrorCode, ' ', X.ErrorKind);
  end;
  N := ENotSupportedException.Create('no files here');
  N.ErrorKind := TIOErrorKind.ekUnsupported;
  try
    raise N;
  except
    on X: Exception do
      WriteLn(X.ClassName, ' ', ENotSupportedException(X).ErrorKind);
  end;
  for var K := TIOErrorKind.ekNone to TIOErrorKind.ekOther do
    Write(K, ' ');
  WriteLn;
  try
    ReadLn(I);
    WriteLn(I);
  except
    on X: EInOutError do
      WriteLn(X.Message, ' ', X.ErrorCode, ' ', X.ErrorKind);
  end;
end.
