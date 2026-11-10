// puy-test: memory-limit=64k
program LoopConcatMemory;

// Concatenation in a loop under a small memory limit: each iteration lets go of the previous
// string, so the heap holds a few short strings at a time instead of every intermediate one
// (about 2 MB for 2000 iterations, far over the 64 KB limit; lifetime.md 3.10).

procedure Grow;
var
  S: string;
  I: Integer;
begin
  S := '';
  for I := 1 to 2000 do
    S := S + 'x';
  WriteLn(Length(S));
end;

begin
  Grow;
end.
