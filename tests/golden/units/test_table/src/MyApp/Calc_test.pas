// The module of a test unit (attributes.md 5.2): an empty `<main>`, the test table in the order
// of the declarations, and a routine per test. A test of a class managed by hand creates it, calls
// `[Setup]`, the test and `[TearDown]` (with the `[TearDown]` of a failed test in `AddSuppressed`)
// and frees it; one of a reference counted class without `[Setup]` / `[TearDown]` only calls the
// test. The arguments of a `[TestCase]` are converted as in a call and evaluated at their line.
unit MyApp.Calc_test;

interface

implementation

uses MyApp.Calc;

type
  TCalcTests = class
  private
    FBase: Integer;
  public
    [Setup]
    procedure Prepare;
    [TearDown]
    procedure CleanUp;
    [Test]
    procedure TwiceOfBase;
    [TestCase(4, 2)]
    [TestCase(9, 4)]
    procedure HalfOf(X: Int64; Expected: Integer);
  end;

  TCounted = class(TInterfacedObject)
    [Test]
    procedure Counts;
  end;

procedure TCalcTests.Prepare;
begin
  FBase := 21;
end;

procedure TCalcTests.CleanUp;
begin
  FBase := 0;
end;

procedure TCalcTests.TwiceOfBase;
begin
  Assert(Twice(FBase) = 42);
end;

procedure TCalcTests.HalfOf(X: Int64; Expected: Integer);
begin
  Assert(Half(X) = Expected);
end;

procedure TCounted.Counts;
begin
  Assert(RefCount = 1);
end;

[Test]
[Skip('not yet')]
procedure Later;
begin
end;

[TestCase('a')]
[TestCase( 'b' + 'c' )]
procedure Upper(const S: string);
begin
  Assert(Length(S) > 0);
end;

end.
