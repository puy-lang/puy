// A test unit that uses its paired unit sees the declarations of its `implementation` section
// (a constant, a generic type, a class, a variable, a routine, and an overload declared there
// only) and the `private` and `protected` members, as code of `MyApp.Calc` does. The language
// attributes of the testing framework are accepted in a test unit (units.md 2.4, attributes.md 5).
unit MyApp.Calc_test;

interface

implementation

uses
  MyApp.Calc;

type
  TCalcTest = class
  public
    [Setup]
    procedure Prepare;
    [TearDown]
    procedure Done;
    [Test]
    procedure CountsBumps;
  end;

procedure TCalcTest.Prepare;
begin
end;

procedure TCalcTest.Done;
begin
end;

procedure TCalcTest.CountsBumps;
var
  C: TCounter;
begin
  C := TCounter.Create;
  try
    C.Bump;
    C.FCount := C.FCount + 1;
    Assert(C.Count = 2);
  finally
    C.Free;
  end;
end;

[Test]
procedure SeesImplementation;
var
  B: TBox<Integer>;
  S: TScratch;
  P: TPoint;
begin
  B := Default(TBox<Integer>);
  P := Default(TPoint);
  B.Value := Offset;
  Assert(Shifted(B.Value) = 21);
  Assert(Add('a', 'b') = 'ab');
  Assert(Add(1, 2) = 3);
  Assert(Calls = 1);
  S := TScratch.Create;
  S.FNote := 'x';
  S.Free;
  P.X := 3;
  Assert(P.GetX = 3);
end;

[TestCase(1, 2)]
[Skip('not yet')]
procedure Pairs(A, B: Integer);
begin
  Assert(A < B);
end;

end.
