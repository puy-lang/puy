// Reading a member of a nil exception raises `EAccessViolation` (docs/spec/exceptions.md 1).
procedure Show(E: Exception);
begin
  WriteLn(E.Message);
end;

Show(Exception.Create('one'));
Show(nil);
