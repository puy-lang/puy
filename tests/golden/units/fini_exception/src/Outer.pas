unit Outer;

interface

uses Inner;

implementation

uses System.SysUtils;

procedure Fail;
begin
  raise EInvalidOp.Create('from finalization');
end;

initialization
  WriteLn('init Outer');
finalization
  WriteLn('fini Outer');
  Fail;
  WriteLn('not reached');
end.
