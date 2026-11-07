program SyntaxEmptyStmt;
var X: Integer;
begin
  X := 0;
  if X > 0 then;
  if X > 0 then WriteLn('a') else;
  while X > 0 do;
  for X := 1 to 2 do;
  {$WARN empty-statement OFF}
  if X > 0 then;
end.
