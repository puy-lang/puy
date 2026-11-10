// A unit used with an alias is only reached through the alias: the unqualified and the full
// names are E0700 with a fix to the alias (docs/spec/units.md 3.1, semantics.md 1.3).
program Main;

uses MyApp.Tools as T;

begin
  WriteLn(T.Twice(1));
  WriteLn(Twice(2));
  WriteLn(MyApp.Tools.Twice(3));
end.
