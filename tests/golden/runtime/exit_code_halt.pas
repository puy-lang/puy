program ExitCodeHalt;

// `Halt` without a code keeps `ExitCode`, and the `finally` blocks it runs see it
// (semantics.md 5.3).

begin
  ExitCode := 6;
  try
    Halt;
  finally
    WriteLn('finally: ', ExitCode);
  end;
end.
