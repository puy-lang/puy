// `Halt` runs the `finally` blocks from the innermost and is not caught by `except`; an exception
// leaving a `finally` block on the way is reported as a warning and `Halt` goes on; an exception
// propagating when `Halt` is called is dropped (docs/spec/exceptions.md 5.3, 7.1, 7.2).
type
  ECleanup = class(Exception);

procedure Stop;
begin
  try
    try
      WriteLn('stopping');
      Halt(3);
    except
      WriteLn('not reached: except');
    end;
  finally
    WriteLn('Stop finally');
    try
      raise ECleanup.Create('handled inside');
    except
      on E: ECleanup do
        WriteLn('caught in finally: ', E.Message);
    end;
    raise ECleanup.Create('first cleanup failed');
  end;
end;

procedure Drop;
begin
  try
    raise ECleanup.Create('dropped');
  finally
    WriteLn('Drop finally');
    Stop;
  end;
end;

try
  try
    Drop;
  finally
    WriteLn('outer finally');
    raise ECleanup.Create('second cleanup failed');
  end;
except
  WriteLn('not reached: outer except');
end;
WriteLn('not reached');
