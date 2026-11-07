program SemaDeclared;
uses Windows, Vcl.Forms, System.SysUtils, Foo;
const
  C = 1;
  C = 2;
  Result = 3;
  True = 4;
var
  X: Integer;
  X: string;
  T: Foo;
  U: C;
  V: SysUtils;
procedure P;
var
  L: Integer;
begin
  L := 1;
  var L := 2;
  WriteLn(L);
end;
procedure P;
begin
end;
function P2(A: Integer; A: string): Integer;
begin
  Result := A;
end;
begin
  X := 1;
  WriteLn(X, T, U, V);
end.
