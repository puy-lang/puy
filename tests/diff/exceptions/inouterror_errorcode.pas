program InOutErrorErrorCode;

// EInOutError.ErrorCode can be set (it is a field in Delphi; a read and write property in Puy,
// spec/exceptions.md 2.2) before the exception is raised, and a handler reads it.

uses
  System.SysUtils;

var
  E: EInOutError;
begin
  E := EInOutError.Create('disk full');
  WriteLn(E.ErrorCode);
  E.ErrorCode := 28;
  try
    raise E;
  except
    on X: EInOutError do
      WriteLn(X.Message, ' ', X.ErrorCode);
  end;
end.
