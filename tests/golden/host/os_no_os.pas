// puy-test: no-fs
program OsNoOs;

// Without the host's `Os` (an embedding that did not grant it), the process ID and the number of
// processors are the fixed values of requirements §5.8, not an exception
// (spec/stdlib/Puy.OS/README.md).

uses
  Puy.OS;

begin
  WriteLn(GetProcessId, ' ', GetProcessorCount);
end.
