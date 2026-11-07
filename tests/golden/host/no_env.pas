// puy-test: no-env
program NoEnv;

uses
  System.SysUtils;

begin
  WriteLn('[', GetEnvironmentVariable('PUY_TEST_HOME'), ']');
end.
