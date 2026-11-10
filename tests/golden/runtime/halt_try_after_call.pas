// `Halt` called inside a callee does not run a finally block that starts right after the call in
// the caller: the caller's position is the call, not its return address (docs/spec/exceptions.md 5.3).
procedure Stop;
begin
  WriteLn('stopping');
  Halt(2);
end;

procedure Run;
begin
  try
    Stop;
  finally
    WriteLn('Run finally');
  end;
end;

procedure Skip;
begin
  Run;
  try
    WriteLn('not reached: try body');
  finally
    WriteLn('not reached: Skip finally');
  end;
end;

Skip;
WriteLn('not reached');
