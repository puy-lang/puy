program ExceptionErrors;

// Diagnostics of exception classes and `raise` (docs/spec/exceptions.md 1-3, 5.1, 5.6, 8):
// E0727 (inherits from itself), E0733, E3200 for the other `class` forms, E0732 (no `except`
// block yet), E0702, E3004 (`raise ... at`), E0700 (no member), E0707 (read-only property),
// E3019 (`Free` / `Destroy` / `FreeAndNil`), E0718, E0705, E0703, E0725, and E3200 for a parent
// that is an M2b class.

type
  TRec = record X: Integer; end;
  EA = class(EB);
  EB = class(EA);
  EBad = class(TRec);
  EInt = class(Integer);
  EObj = class(TObject);
  EFwd = class;
  EMem = class(Exception) X: Integer; end;
  EOk = class(Exception);
  EFloat = class(EMathError);
var
  E: Exception;
  F: EOk;
  I: Integer;
  S: string;
begin
  E := nil;
  F := nil;
  I := 0;
  raise;
  raise 1;
  raise E at nil;
  F := E;
  S := E.Foo;
  E.ClassName := 'x';
  E.Free;
  E.Destroy;
  FreeAndNil(E);
  WriteLn(E);
  E := EOk.Create;
  E := EOk.Create('a', 'b');
  if E = I then
    WriteLn;
  if F = EA.Create('x') then
    WriteLn;
  Exception.RaiseOuterException(E);
  E := Exception.Make('x');
  I := Exception.RaiseOuterException(E);
  E.Message := S;
end.
