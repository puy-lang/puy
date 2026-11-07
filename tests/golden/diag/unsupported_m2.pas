program UnsupportedM2;

procedure Outer;
var X: Integer;
  procedure Inner; begin end;
begin
  X := 1;
  WriteLn(X);
end;

procedure Def(X: Integer = 1); begin end;
procedure Over(X: Integer); overload; deprecated;
begin
end;

var
  Z, I: Integer;
  S: string;
  D: Double;
begin
  case I of 1: WriteLn('one'); end;
  try
    WriteLn('x');
  except
  end;
  raise;
  Z := 7 / 2;
  Z := 1.5;
  if 1 in [1, 2] then WriteLn('in');
  Z := Low(Integer) + Succ(1);
  for var E in S do WriteLn(E);
  Z := I.ToString;
  WriteLn(ErrOutput, 'x');
  ReadLn(S, S);
  WriteLn(Format('%d', [1]));
  WriteLn(Z, D);
end.
