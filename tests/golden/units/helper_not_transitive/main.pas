program Main;

// `uses` is not transitive for helpers (objects.md 10.4): `HelpB` uses `HelpA`, but the program
// only uses `HelpB`, so `HelpA`'s helper does not apply here (the E3200 of a built-in type's
// missing helper member), while `HelpB`'s function using it works.

uses HelpB;

var
  S: string;
begin
  S := 'hey';
  WriteLn(Loud(S));
  WriteLn(S.Bang);
end.
