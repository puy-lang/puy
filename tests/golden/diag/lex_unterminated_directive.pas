program LexUnterminatedDirective;
begin
  WriteLn('a');
  {$IFDEF X never closed
end.
