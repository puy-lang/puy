program SuppressedRelease;

// Exceptions attached to `Suppressed` and `InnerException` are released with the exception that
// holds them; the exception propagating through a `finally` block lives in the block's register
// until it goes on (docs/spec/exceptions.md 5.4, 6.1, docs/design/lifetime.md 3.9). No block leaks.

type
  EA = class(Exception);

procedure Layers;
begin
  try
    try
      raise EA.Create('a');
    finally
      raise EA.Create('b');
    end;
  finally
    WriteLn('outer finally');
  end;
end;

procedure Wrap;
begin
  try
    Layers;
  except
    on E: EA do
      Exception.RaiseOuterException(EA.Create('wrapped ' + E.Message));
  end;
end;

begin
  try
    Wrap;
  except
    on E: EA do
      WriteLn(E.Message, ' ', E.InnerException.Message, ' ', E.InnerException.Suppressed[0].Message);
  end;
end.
