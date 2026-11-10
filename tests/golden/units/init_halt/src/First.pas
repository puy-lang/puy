unit First;

interface

implementation

initialization
  WriteLn('init First');
  ExitCode := 3;
finalization
  WriteLn('fini First ', ExitCode);
end.
