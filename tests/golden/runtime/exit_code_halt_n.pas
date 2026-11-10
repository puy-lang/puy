program ExitCodeHaltN;

// `Halt(N)` sets `ExitCode` before it unwinds, so a `finally` block sees `N` and can change
// the final exit code (semantics.md 7.2).

procedure Stop;
begin
  Halt(5);
end;

begin
  try
    Stop;
  finally
    WriteLn('finally: ', ExitCode);
    ExitCode := ExitCode + 2;
  end;
end.
