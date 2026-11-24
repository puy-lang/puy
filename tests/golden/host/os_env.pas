program OsEnv;

// Puy.OS writes environment variables into the isolate's own table (spec/stdlib/Puy.OS/README.md):
// GetEnvironmentVariable and the list see the writes, a removal hides the host's value, and a
// name written again keeps its place in the list. The runner's host has PUY_TEST_HOME and EMPTY;
// the list is filtered because the `puy` command runs this with the whole process environment.
// The process's variables are not changed: the host's `Env` has no way to write.

uses
  System.SysUtils,
  Puy.OS;

// The variables the isolate set, in the list's order, then how the list shows the host's two (the
// order of the host's variables is the OS's, so they are looked up rather than listed)
procedure List;
begin
  var Home := '(none)';
  var Empty := '(none)';
  for var V in GetEnvironmentVariables do
    if V.Name.StartsWith('PUY_OS_') then
      WriteLn('  ', V.Name, '=[', V.Value, ']')
    else if V.Name = 'PUY_TEST_HOME' then
      Home := '[' + V.Value + ']'
    else if V.Name = 'EMPTY' then
      Empty := '[' + V.Value + ']';
  WriteLn('  PUY_TEST_HOME=', Home, ' EMPTY=', Empty);
end;

procedure Attempt(const Name, Value: string; Remove: Boolean);
begin
  try
    if Remove then
      RemoveEnvironmentVariable(Name)
    else
      SetEnvironmentVariable(Name, Value);
    WriteLn('accepted');
  except
    on E: EArgumentException do
      WriteLn(E.ClassName, ': ', StringReplace(E.Message, #0, '#0', [TReplaceFlag.rfReplaceAll]));
  end;
end;

begin
  WriteLn('host:');
  List;
  SetEnvironmentVariable('PUY_OS_B', 'b');
  SetEnvironmentVariable('PUY_OS_A', 'a');
  SetEnvironmentVariable('PUY_TEST_HOME', '/home/other');
  RemoveEnvironmentVariable('EMPTY');
  RemoveEnvironmentVariable('PUY_OS_NEVER_SET');
  WriteLn('[', GetEnvironmentVariable('PUY_OS_A'), '] [', GetEnvironmentVariable('PUY_TEST_HOME'),
    '] [', GetEnvironmentVariable('EMPTY'), ']');
  WriteLn('changed:');
  List;
  // Written again: the value changes, the place in the list does not; an empty value is set
  SetEnvironmentVariable('PUY_OS_B', '');
  RemoveEnvironmentVariable('PUY_OS_A');
  SetEnvironmentVariable('EMPTY', 'back');
  WriteLn('again:');
  List;
  // Written again after its removal: back at the place it was first written (PUY_OS_B, removed
  // and written after PUY_OS_A, still comes first)
  SetEnvironmentVariable('PUY_OS_A', 'a2');
  RemoveEnvironmentVariable('PUY_OS_B');
  SetEnvironmentVariable('PUY_OS_B', 'b2');
  WriteLn('rewritten:');
  List;
  Attempt('', 'x', False);
  Attempt('A=B', 'x', False);
  Attempt('A'#0'B', 'x', False);
  Attempt('PUY_OS_C', 'x'#0'y', False);
  Attempt('=', '', True);
  Attempt('PUY_OS_C', 'é ü', False);
  WriteLn(GetEnvironmentVariable('PUY_OS_C'));
end.
