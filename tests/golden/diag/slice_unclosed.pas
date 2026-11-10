program SliceUnclosed;

// A slice whose upper bound is left out and whose `]` is missing reports the missing `]`, not
// a missing expression (grammar.md 1.5).

var
  S: string;
begin
  S := 'abc';
  WriteLn(S[1..);
end.
