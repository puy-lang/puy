program ExceptRelease;

// The exception of an `except` block is released when the block is left: at its end, by `Exit`,
// `Break` and `Continue`, and when another exception leaves it; a saved `E` stays alive, and
// `raise;` raises a new reference (docs/spec/exceptions.md 5.2, 6.3, docs/design/lifetime.md 3.9).
// No block leaks.

type
  EStep = class(Exception);

var
  Kept: Exception;

procedure Leave(N: Integer);
var
  I: Integer;
begin
  for I := 1 to 3 do
    try
      raise EStep.Create('step');
    except
      on E: EStep do
      begin
        if I = 1 then
          Continue;
        if (I = 2) and (N = 0) then
          Break;
        if I = 3 then
          Exit;
        Kept := E;
      end;
    end;
end;

procedure Replace;
begin
  try
    try
      raise EStep.Create('first');
    except
      on E: EStep do
        raise EStep.Create('second ' + E.Message);
    end;
  except
    on E: EStep do
      WriteLn(E.Message);
  end;
end;

procedure Again;
begin
  try
    try
      raise EStep.Create('again');
    except
      raise;
    end;
  except
    on E: Exception do
      WriteLn(E.Message);
  end;
end;

begin
  Leave(0);
  Leave(1);
  WriteLn(Kept.Message);
  Replace;
  Again;
end.
