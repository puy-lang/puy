program RaiseFlow;

// `raise` ends a path (docs/spec/exceptions.md 5.7): the statement after it is unreachable
// (W0003), a function whose every path ends with `raise` or an assignment to `Result` is not
// E2001, and a variable assigned only on the path that continues is assigned after the `if`.

function Pick(N: Integer): Integer;
begin
  if N > 0 then
    Result := N
  else
    raise Exception.Create('not positive');
end;

function Never: Integer;
begin
  raise Exception.Create('never');
end;

var
  X: Integer;
begin
  if Pick(1) > 0 then
    X := 1
  else
    raise Exception.Create('x');
  WriteLn(X, Never);
  raise Exception.Create('stop');
  WriteLn('unreachable');
end.
