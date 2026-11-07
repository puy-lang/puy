program DirUnterminated;
begin
  WriteLn('x');
{$IFDEF NOPE}
  WriteLn(;
  this is never parsed
end.
