// puy-test: no-env, test-fs
program OsNoEnv;

// Without the host's `Env` (spec/stdlib/Puy.OS/README.md), the isolate's table still works: a
// written variable is read back and listed, while the host's variables stay empty. The process
// ID and the number of processors come from the fixed `Os` of `test-fs`.

uses
  System.SysUtils,
  Puy.OS;

begin
  WriteLn(GetProcessId, ' ', GetProcessorCount);
  WriteLn(Length(GetEnvironmentVariables), ' [', GetEnvironmentVariable('PUY_TEST_HOME'), ']');
  SetEnvironmentVariable('PUY_TEST_HOME', '/set');
  SetEnvironmentVariable('X', '1');
  WriteLn('[', GetEnvironmentVariable('PUY_TEST_HOME'), ']');
  for var V in GetEnvironmentVariables do
    WriteLn(V.Name, '=', V.Value);
  RemoveEnvironmentVariable('X');
  WriteLn(Length(GetEnvironmentVariables), ' [', GetEnvironmentVariable('X'), ']');
end.
