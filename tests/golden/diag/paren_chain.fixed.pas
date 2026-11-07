program ParenChain;

var
  X, Y: Integer;
  S: string;
begin
  X := 1;
  Y := 2;
  S := 'a';
  if (X = 1) and (Y = 2) then
    WriteLn('both');
  if (X = 1) and (S = 'a') or (Y > 1) then
    WriteLn('mixed');
end.
