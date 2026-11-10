program NestedTrace;

// The stack trace names a nested routine with the routines enclosing it (semantics.md 4.9)

procedure Outer(N: Integer);

  procedure Inner;
  begin
    WriteLn(100 div N);
  end;

begin
  Inner;
end;

begin
  Outer(5);
  Outer(0);
end.
