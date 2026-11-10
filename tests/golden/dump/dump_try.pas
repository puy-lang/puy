program DumpTry;

// The dumps of try statements (semantics.md 9, bytecode.md 5.1, 5.2): `try` with `body`, `on`,
// `else`, `except`, `finally` and `raise-outer` in the hir dump; the `.try` lines, `ExcClass`,
// `ClassIs`, `RaiseOuter` and `EndFinally` with the jumps that leave through the `finally` block
// in the ir and bytecode dumps.

type
  EAppError = class(Exception);

procedure Handle(N: Integer);
begin
  try
    if N > 0 then
      raise EAppError.Create('app');
  except
    on E: EAppError do
      Exception.RaiseOuterException(Exception.Create('wrapped ' + E.Message));
    on EConvertError do
      raise;
  else
    WriteLn('other');
  end;
end;

function Count(N: Integer): Integer;
var
  I: Integer;
begin
  Result := 0;
  for I := 1 to N do
    try
      if I = 3 then
        Break;
      if I = 5 then
        Exit;
      Inc(Result);
    finally
      WriteLn(I);
    end;
end;

begin
  try
    Handle(0);
  except
    WriteLn('caught');
  end;
  WriteLn(Count(4));
end.
