// `puy test` runs each test in an isolate of its own and reports ok / FAIL / ERROR / skip, the
// leaks and the output of the failed tests (docs/spec/test.md).
unit MyApp.Calc_test;

interface

implementation

uses MyApp.Calc;

type
  TNode = class(TInterfacedObject)
    Next: TNode;
  end;

var
  Count: Integer;

[Test]
procedure Adds;
begin
  // The paired unit's implementation section is visible
  Assert(Add(1, 2) = 3 + Offset);
end;

[TestCase(1, 2, 3)]
[TestCase(2, 2, 5)]
procedure AddsCase(A, B, Want: Integer);
begin
  WriteLn('adding ', A, ' and ', B);
  Assert(Add(A, B) = Want, 'sum');
end;

[Test]
procedure Raises;
var
  X: Integer;
begin
  X := 0;
  WriteLn(10 div X);
end;

[Test]
procedure CallsHalt;
begin
  WriteLn('before Halt');
  Halt(0);
end;

[Test]
procedure Leaks;
var
  N: TNode;
begin
  N := TNode.Create;
  N.Next := N;
end;

[Test]
procedure FreshGlobals;
begin
  // Each test starts with the unit's variables as initialized
  Inc(Count);
  Assert(Count = 1);
end;

[Test]
procedure FreshGlobalsAgain;
begin
  Inc(Count);
  Assert(Count = 1);
end;

[Test, Skip('not ready')]
procedure Later;
begin
end;

end.
