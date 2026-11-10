program DumpExceptions;

// The dumps of exception classes (semantics.md 9.1, bytecode.md 5.1, 5.2): `.type … = class(…)`,
// `new`, `member`, `raise`, the `exception` entries of `.type` with their parents, `NewExc`,
// `ExcGet`, `ExcSet`, `ExcInfo` and `Raise` (also for `Assert`).

type
  EAppError = class(Exception);
  EConfigError = class(EAppError);

procedure Fail(const Why: string);
var
  E: EAppError;
begin
  E := EConfigError.Create(Why);
  E.Message := E.Message + '!';
  WriteLn(E.ClassName, ': ', E.Message);
  raise E;
end;

var
  C: EConvertError;
begin
  C := EConvertError.Create('x');
  WriteLn(C.Value = '', ' ', Length(C.StackTrace));
  Assert(C <> nil);
  Fail('stop');
end.
