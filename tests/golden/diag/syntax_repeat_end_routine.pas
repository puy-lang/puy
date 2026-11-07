program SyntaxRepeatEndRoutine;

// A repeat in a routine body mistakenly closed by end (grammar.md chapter 3, "end inside repeat").
// If an end closing the outer begin remains after it, that end is consumed as a mistyped until,
// and no E0400 is chained onto the body's end. Each routine gets exactly one diagnostic

// The body's end; remains after it: the end is read as a mistyped until
procedure P;
var
  I: Integer;
begin
  I := 0;
  repeat
    I := I + 1;
  end;
end;

// Same inside a nested begin
procedure Q;
begin
  if True then
  begin
    repeat
      WriteLn('q');
    end;
  end;
end;

// Missing end: the end closes the outer begin (as before)
procedure R;
begin
  begin
    repeat
      WriteLn('r');
    end;
end;

begin
  P;
  Q;
  R;
end.
