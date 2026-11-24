// `--exact` chooses the test whose whole name is the text (ignoring case; docs/spec/test.md 6).
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
