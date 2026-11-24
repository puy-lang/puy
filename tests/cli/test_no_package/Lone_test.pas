// Without a package, `puy test` takes test files: the paths of stack traces are the paths given
// (docs/spec/test.md 1).
unit Lone_test;

interface

implementation

[Test]
procedure Works;
begin
end;

[Test]
procedure Fails;
begin
  Assert(False);
end;

end.
