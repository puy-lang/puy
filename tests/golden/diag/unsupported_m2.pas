program UnsupportedM2;

procedure Def(X: Integer = 1); begin end;
procedure Over(X: Integer); overload; deprecated;
begin
end;

var
  Z, I: Integer;
  S: string;
begin
  Z := Low(Integer) + SizeOf(1);
  for var E in S do WriteLn(E);
  Z := I.ToString;
  WriteLn(Z);
  raise;
end.
