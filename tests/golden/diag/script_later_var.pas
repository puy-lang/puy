procedure Show;
begin
  WriteLn(Before, After);
end;

var After := 2;
var Before := 1;
Show;
WriteLn(Before, After);
