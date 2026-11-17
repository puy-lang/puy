program ExceptionCtorErrors;

// Exception classes (objects.md 8.2, 8.6): a child's constructor that is not `overload` hides
// `Exception.Create(Msg)`, so `EApp.Create('x')` does not match it; `InnerException` cannot be
// assigned; a constructor call written as a statement is E0402.

uses
  System.SysUtils;

type
  EApp = class(Exception)
    Code: Integer;
    constructor Create(ACode: Integer);
  end;

constructor EApp.Create(ACode: Integer);
begin
  inherited Create('app');
  Code := ACode;
end;

var
  E: Exception;
begin
  E := EApp.Create('xy');
  E.InnerException := nil;
  Exception.Create('x');
end.
