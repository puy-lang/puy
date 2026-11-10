program ExitCodeUncaught;

// An uncaught exception ends with `ExitCode` when it is not 0 (it is 1 only when `ExitCode` is
// 0; semantics.md 7.2).

uses System.SysUtils;

begin
  ExitCode := 9;
  raise Exception.Create('stop');
end.
