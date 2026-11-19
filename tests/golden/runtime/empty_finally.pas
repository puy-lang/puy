program EmptyFinally;

// A `try..finally` whose `finally` block is empty (its handler range holds no instruction, only
// the `EndFinally` ending it) runs: the body runs, an exception still goes through to the outer
// handler, `Exit`, `Break` and `Continue` still leave through it, and a `finally` block whose
// only statement is a call of an empty (inlined) routine behaves the same.

uses System.SysUtils;

procedure Nothing;
begin
end;

function Leave(N: Integer): Integer;
begin
  Result := 0;
  try
    if N > 0 then
    begin
      Result := N;
      Exit;
    end;
    Result := -1;
  finally
  end;
end;

var
  N, I: Integer;
begin
  try
    N := 2;
  finally
  end;
  WriteLn(N);

  try
  finally
  end;

  try
    try
      raise Exception.Create('through an empty finally');
    finally
    end;
  except
    on E: Exception do
      WriteLn('caught: ', E.Message);
  end;

  WriteLn(Leave(5), ' ', Leave(0));

  for I := 1 to 5 do
  begin
    try
      if I = 2 then
        Continue;
      if I = 4 then
        Break;
      Write(I, ' ');
    finally
    end;
  end;
  WriteLn;

  try
    N := 3;
  finally
    Nothing;
  end;
  WriteLn(N);

  try
    try
      raise Exception.Create('through a finally calling an empty routine');
    finally
      Nothing;
    end;
  except
    on E: Exception do
      WriteLn('caught: ', E.Message);
  end;
end.
