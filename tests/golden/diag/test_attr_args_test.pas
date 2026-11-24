// The arguments of the attributes of the testing framework and the parameters of the tests
// (attributes.md 5.1, 5.2): E0705 / E0702 / E0713 / E0706 as for a call, E0786 for parameters
// without `[TestCase]`, E0787 for the `[Setup]` / `[TearDown]` of a class, E0788 for a test class
// that cannot be created, W0027 for a test case written twice. A test with defaults only is
// called with them.
unit Test_attr_args_test;

interface

implementation

const
  Big = 10;

type
  TTwoSetups = class
    [Setup]
    procedure A;
    [Setup]
    procedure B;
    [TearDown]
    procedure C(X: Integer);
    [Test]
    procedure T;
  end;

  TNoTests = class
    [TearDown]
    procedure Done;
  end;

  TNeedsArgs = class
  public
    constructor Create(X: Integer);
    [Test]
    procedure T;
  end;

  TAbstract = class
    procedure Hook; virtual; abstract;
    [Test]
    procedure T;
  end;

  // A class whose only test has an error still has tests: no E0787 for its `[Setup]`
  TBadTest = class
    [Setup]
    procedure S;
    [Test]
    procedure T(X: Integer);
  end;

procedure TBadTest.S; begin end;
procedure TBadTest.T(X: Integer); begin end;
procedure TTwoSetups.A; begin end;
procedure TTwoSetups.B; begin end;
procedure TTwoSetups.C(X: Integer); begin end;
procedure TTwoSetups.T; begin end;
procedure TNoTests.Done; begin end;
constructor TNeedsArgs.Create(X: Integer); begin end;
procedure TNeedsArgs.T; begin end;
procedure TAbstract.T; begin end;

[Test]
procedure Params(X: Integer);
begin
end;

[Test]
procedure Defaults(X: Integer = 1);
begin
end;

[TestCase(1, 2)]
[TestCase('a')]
[TestCase(1)]
[TestCase(Big)]
[TestCase(1)]
procedure Cases(X: Integer);
begin
end;

var
  G: Integer;
  Why: string;

[TestCase(G)]
procedure NotConst(X: Integer);
begin
end;

[Test(1)]
[Skip('a', 'b')]
procedure WithArgs;
begin
end;

[Test]
[Skip(1)]
procedure SkipInt;
begin
end;

[Test]
[Skip(Why)]
procedure SkipVar;
begin
end;

// Overloads whose tests get the same name: W0027, the second one ` #2`
[Test]
procedure Over; overload;
begin
end;

[Test]
procedure Over(X: Integer = 1); overload;
begin
end;

[TestCase(1)]
procedure Out(var X: Integer);
begin
end;

end.
