var I: Integer;

// A script top-level variable named as the loop variable inside a routine (1.2, 4.5)
procedure Count;
begin
  for I := 1 to 2 do
    WriteLn(I);
end;

Count;
