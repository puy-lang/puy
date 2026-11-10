program ExitCodeFor;

// `ExitCode` is a global variable of `System`, so the main block can use it as a `for` loop
// variable (semantics.md 4.5) and it keeps the last value; `Halt` without a code ends with it.

begin
  for ExitCode := 1 to 3 do
    Write(ExitCode, ' ');
  WriteLn;
  Halt;
end.
