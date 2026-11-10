program LoopSubstring;

// Substrings and concatenation in a loop: the same temporaries are written on every iteration,
// and each write lets go of the previous iteration's value (lifetime.md 3.4).

var
  I: Integer;
  Acc, Part: string;
begin
  Acc := '';
  for I := 1 to 5 do
  begin
    Part := Copy('abcdef', I, 2);
    Acc := Acc + Part + '/';
  end;
  WriteLn(Acc);
end.
