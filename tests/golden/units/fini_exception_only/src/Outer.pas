unit Outer;

interface

uses Inner;

implementation

uses System.SysUtils;

finalization
  WriteLn('fini Outer');
  raise EInvalidOp.Create('from finalization');
end.
