program EnvVars;

uses
  System.SysUtils;

begin
  WriteLn('[', GetEnvironmentVariable('PUY_TEST_HOME'), ']');
  WriteLn('[', GetEnvironmentVariable('EMPTY'), ']');
  WriteLn('[', GetEnvironmentVariable('PUY_TEST_MISSING'), ']');
end.
