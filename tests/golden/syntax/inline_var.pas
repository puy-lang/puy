program InlineVar;

var
  Total: Integer = 100;

begin
  var Step := 7;
  const Limit = 3;
  for var I := 1 to Limit do
    Total := Total + Step * I;
  WriteLn(Total);
  var S := 'x';
  var T: string := S + 'y';
  WriteLn(T);
end.
