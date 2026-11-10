program OverloadTrace;

// The stack trace names the second overload `Fail#2` (semantics.md 4.7).

uses System.SysUtils;

procedure Fail(N: Integer); overload;
begin
  raise Exception.Create('integer');
end;

procedure Fail(const S: string; Code: Integer = 3); overload;
begin
  raise Exception.Create(S + IntToStr(Code));
end;

begin
  Fail('text');
end.
