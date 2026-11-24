// Finalization errors and exceptions discarded by Halt are reported in different JSON fields.
unit Teardown_test;

interface

implementation

uses BadFini;

[Test]
procedure Returns;
begin
end;

[Test]
procedure Halts;
begin
  try
    Halt(3);
  finally
    raise EInvalidOp.Create('during Halt');
  end;
end;

end.
