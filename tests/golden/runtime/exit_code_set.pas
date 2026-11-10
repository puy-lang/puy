program ExitCodeSet;

// `ExitCode` (semantics.md 5.3, 7.2): the program ends normally with the code it was given;
// only its low 8 bits are the exit code.

begin
  WriteLn(ExitCode);
  ExitCode := 256 + 3;
  Inc(ExitCode, 1);
  WriteLn(ExitCode);
end.
