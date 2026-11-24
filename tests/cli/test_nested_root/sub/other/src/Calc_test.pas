// A test file under another package's root is taken with the root found from the working
// directory, outside its `src/`: it has no paired unit, so `Secret` is not visible
// (docs/spec/test.md 1).
unit Calc_test;

interface

implementation

uses Calc;

[Test]
procedure SeesSecret;
begin
  Assert(Add(Secret, 1) = 2);
end;

end.
