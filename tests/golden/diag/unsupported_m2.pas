program UnsupportedM2;

procedure Def(X: Integer = 1); begin end;
procedure Over(X: Integer); overload; deprecated;
begin
end;

var
  Z: Integer;
  C: Currency;
  S: string;
begin
  Z := Low(Integer) + SizeOf(1);
  for var E in S do WriteLn(E);
  Z := C.ToHexString;
  WriteLn(Z);
  raise;
end.
