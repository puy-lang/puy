// In a script routine body too, if the body's end; remains after an end that closed a repeat,
// that end is read as a mistyped until (grammar.md chapter 3). What follows is top-level statements
procedure P;
begin
  repeat
    WriteLn('p');
  end;
end;
P;
WriteLn('done');
