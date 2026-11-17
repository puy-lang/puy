program SuppressedReleaseOrder;

// Freeing an exception whose class has no destructor of its own lets go of its `Suppressed`
// exceptions in their order after its fields (exceptions.md 5.4, objects.md 14.8): when one of
// them has a destructor, the rest are let go of after it runs, each once. No block leaks.

type
  EBad = class(Exception)
    destructor Destroy; override;
  end;

  EPlain = class(Exception);

destructor EBad.Destroy;
begin
  WriteLn('destroy ', Message);
  inherited;
end;

// Appends a new exception to the `Suppressed` of `Saved`: `Saved` is raised from a `finally`
// while the new one propagates. The new exception is made here, so that no temporary of the
// caller holds it.
procedure AddSuppressed(Saved: Exception; const Name: string; Bad: Boolean);
begin
  try
    try
      if Bad then
        raise EBad.Create(Name)
      else
        raise EPlain.Create(Name);
    finally
      raise Saved;
    end;
  except
    on E: Exception do
      WriteLn('caught ', E.Message);
  end;
end;

// Reads `Suppressed` here, so that the temporary array holding its exceptions is let go when this
// returns.
function SuppressedCount(E: Exception): Integer;
begin
  Result := Length(E.Suppressed);
end;

procedure Run;
var
  Saved: Exception;
begin
  Saved := EPlain.Create('outer');
  AddSuppressed(Saved, 'one', True);
  AddSuppressed(Saved, 'two', True);
  AddSuppressed(Saved, 'three', False);
  AddSuppressed(Saved, 'four', True);
  WriteLn('suppressed ', SuppressedCount(Saved));
  Saved := nil;
  WriteLn('outer cleared');
end;

begin
  Run;
  WriteLn('end');
end.
