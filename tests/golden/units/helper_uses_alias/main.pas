program Main;

// A helper of a unit used with an alias applies (objects.md 10.4, 10.5): `S.Bang` works, and the
// helper can be named through the alias as `AA.TBangHelper.Bang(S)`.

uses HelpA as AA;

var
  S: string;
begin
  S := 'hey';
  WriteLn(S.Bang);
  WriteLn(AA.TBangHelper.Bang(S));
end.
