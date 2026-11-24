// `--filter` chooses the tests whose names contain the text (ignoring case), `--exact` the
// whole name, a path the test units under it; `--show-output` prints the output of the passed
// tests too (docs/spec/test.md 1, 5, 6).
unit Shapes_test;

interface

implementation

[Test]
procedure Square;
begin
  WriteLn('square');
end;

[Test]
procedure SquareRoot;
begin
  WriteLn('root');
end;

[Test]
procedure Circle;
begin
  WriteLn('circle');
end;

end.
