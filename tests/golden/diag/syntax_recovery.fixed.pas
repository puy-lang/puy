program SyntaxRecovery;
var
  X: Integer;
  Y: Integer;
  A, B: Integer = 0;
  Z: ;
  : Integer;
  W: Integer;
const
  = 1;
procedure (P: Integer);
begin
end;
procedure Q(P: Integer); 42;
begin
end;
function F(): ;
begin
  Result := 0;
end;
begin
  X := 1;
  WriteLn(X);
  if X > 0 then WriteLn('a');
  if (X > 0) then WriteLn('a');
  if X := 1 then WriteLn(X);
  X := ;
  if then WriteLn('b');
  WriteLn(,);
  WriteLn 'a';
  while X > 0 do X := X - 1;
  for X := 1 to 3 do WriteLn(X);
  if X > 0 then
    WriteLn('a')
  else
    WriteLn('b');
  X := 1;
  else WriteLn('c');
  begin ) end;
  X := (1 + 2);
  Y := 1 ) + 2;
  X := 1 2;
  WriteLn(X, Y, A, B, W, F);
end.
