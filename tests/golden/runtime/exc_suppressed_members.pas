// `Suppressed` and `InnerException` read in a handler, the order of several suppressed exceptions,
// `raise E` keeping the first trace, and no cycle when a `finally` block raises the exception that
// is propagating (docs/spec/exceptions.md 5.1, 5.4).
program ExcSuppressedMembers;

type
  EA = class(Exception);
  EB = class(Exception);
  EC = class(Exception);

procedure Layers;
begin
  try
    try
      raise EA.Create('a');
    finally
      raise EB.Create('b');
    end;
  finally
    raise EC.Create('c');
  end;
end;

procedure Twice;
begin
  try
    try
      raise EA.Create('first');
    finally
      raise EB.Create('second');
    end;
  except
    on E: EB do
      raise EC.Create('replaced');
  end;
end;

var Saved: Exception;

procedure Same;
begin
  Saved := EA.Create('same');
  try
    raise Saved;
  finally
    raise Saved;
  end;
end;

procedure TraceKept;
begin
  try
    raise EA.Create('trace');
  except
    on E: EA do
      raise E;
  end;
end;

begin
try
  Layers;
except
  on E: Exception do
  begin
    WriteLn(E.ClassName, ' ', Length(E.Suppressed));
    WriteLn('  ', E.Suppressed[0].ClassName, ' ', Length(E.Suppressed[0].Suppressed));
    WriteLn('  ', E.Suppressed[0].Suppressed[0].ClassName);
  end;
end;
try
  Twice;
except
  on E: EC do
    WriteLn(E.Message, ' ', Length(E.Suppressed), ' ', Assigned(E.InnerException));
end;
try
  Same;
except
  on E: EA do
    WriteLn(E.Message, ' ', Length(E.Suppressed), ' ', E = Saved);
end;
Saved := nil;
try
  TraceKept;
except
  on E: EA do
    Write(E.StackTrace, #10);
end;
end.
