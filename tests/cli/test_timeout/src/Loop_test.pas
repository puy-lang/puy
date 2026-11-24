// A test past the time limit is an error, and the next test runs (docs/spec/test.md 4, 8).
unit Loop_test;

interface

implementation

[Test]
procedure Forever;
var
  N: Integer;
begin
  N := 0;
  while N >= 0 do
    N := (N + 1) mod 1000;
end;

[Test]
procedure Next;
begin
end;

end.
