program ControlFlow;

var
  I, N, Sum: Integer;
begin
  N := 10;
  Sum := 0;
  I := 1;
  while I <= N do
  begin
    Sum := Sum + I;
    I := I + 1;
  end;
  WriteLn('while: ', Sum);

  Sum := 0;
  for I := 1 to N do
    if I mod 2 = 0 then
      Sum := Sum + I;
  WriteLn('for: ', Sum);

  for I := 3 downto 1 do
    Write(I, ' ');
  WriteLn;

  I := 0;
  repeat
    I := I + 1;
    if I = 2 then
      Continue;
    Write(I, ' ');
    if I >= 4 then
      Break;
  until I > 100;
  WriteLn;

  if N > 5 then
    WriteLn('big')
  else if N > 2 then
    WriteLn('medium')
  else
    WriteLn('small');

  for I := 1 to 0 do
    WriteLn('never');
  WriteLn('done');
end.
