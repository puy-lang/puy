// Names of the units in `uses` resolve unqualified and qualified (docs/spec/units.md 3.1, 3.2).
program Main;

uses Greeting, MyApp.Models.User as U;

var
  P: TPoint;
begin
  WriteLn(Greet('unit'));
  WriteLn(Greeting.Greet('qualified'));
  WriteLn(U.Describe('nested'));
  P := Origin;
  WriteLn(P.X, ' ', P.Y);
  Count := Count + 1;
  WriteLn(Greeting.Count, Excl);
end.
