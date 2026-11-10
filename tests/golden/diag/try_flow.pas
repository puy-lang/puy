// Definite assignment and reachability around try statements (docs/spec/exceptions.md 5.7):
// handlers and finally blocks start from the state at the `try`.
program TryFlow;

function InBody: Integer;
var
  X: Integer;
begin
  try
    X := 1;
  except
    WriteLn(X);
  end;
  Result := X;
end;

function AllHandlersAssign(S: string): Integer;
var
  X: Integer;
begin
  try
    X := Length(S);
  except
    on E: Exception do
      X := -1;
  end;
  Result := X;
end;

function FinallyAssigns: Integer;
var
  X: Integer;
begin
  try
    WriteLn('body');
  finally
    X := 2;
  end;
  Result := X;
end;

function ExitThroughFinally(N: Integer): Integer;
begin
  try
    if N > 0 then
      Exit;
  finally
    Result := N;
  end;
end;

function RaisesOnEveryPath: Integer;
begin
  try
    raise Exception.Create('x');
  finally
    WriteLn('cleanup');
  end;
  WriteLn('unreachable');
end;

function HandlerWithoutElse: Integer;
var
  X: Integer;
begin
  try
    X := 1;
  except
    on E: Exception do
      raise;
  end;
  Result := X;
end;

function Unassigned: Boolean;
var
  X: Exception;
begin
  // The fix of E2000 initializes an exception variable with `nil`
  Result := Assigned(X);
end;

begin
  WriteLn(Unassigned);
  WriteLn(InBody, AllHandlersAssign('a'), FinallyAssigns, ExitThroughFinally(1));
  WriteLn(RaisesOnEveryPath, HandlerWithoutElse);
end.
