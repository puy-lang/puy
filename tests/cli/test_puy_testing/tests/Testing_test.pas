// The helpers of Puy.Testing under `puy test`: Skip makes a test SKIP while it runs (also from
// [Setup] and through a descendant of ESkipTest, but not inside AssertRaises), AssertRaises
// returns the exception or fails, and AssertNear compares within a tolerance
// (docs/spec/stdlib/Puy.Testing/README.md).
unit Testing_test;

interface

implementation

uses
  System.SysUtils, Puy.Testing;

type
  ENotHere = class(ESkipTest);

  TNeedsSetup = class
  public
    [Setup]
    procedure Prepare;
    [Test]
    procedure NeverRuns;
  end;

procedure TNeedsSetup.Prepare;
begin
  Skip('no database');
end;

procedure TNeedsSetup.NeverRuns;
begin
  WriteLn('not reached');
end;

[Test]
procedure SkipsWhileRunning;
begin
  WriteLn('before Skip');
  Skip('needs a network');
  Assert(False);
end;

[Test]
procedure SkipsWithoutReason;
begin
  Skip('');
end;

[Test]
procedure SkipsWithDescendant;
begin
  raise ENotHere.Create('not on this platform');
end;

[Test]
procedure CatchesSkip;
begin
  // Caught like any other exception: the test goes on
  try
    Skip('caught');
  except
    on ESkipTest do
      WriteLn('caught');
  end;
end;

[Test]
procedure SkipThenHalt;
begin
  // A Halt in the unwinding still makes the test an ERROR
  try
    Skip('halted');
  finally
    Halt(3);
  end;
end;

[Test]
procedure RaisesReturnsTheException;
var
  E: EConvertError;
begin
  E := AssertRaises<EConvertError>(
    procedure
    begin
      StrToInt('x');
    end);
  Assert(E.Message = '''x'' is not a valid integer');
end;

[Test]
procedure RaisesAcceptsDescendants;
var
  E: EMathError;
begin
  E := AssertRaises<EMathError>(
    procedure
    begin
      raise EZeroDivide.Create('zero');
    end);
  Assert(E.ClassName = 'EZeroDivide');
end;

[Test]
procedure RaisesNothing;
begin
  AssertRaises<EConvertError>(
    procedure
    begin
    end);
end;

[Test]
procedure RaisesAnotherClass;
begin
  AssertRaises<EConvertError>(
    procedure
    begin
      raise EArgumentException.Create('bad argument');
    end);
end;

[Test]
procedure SkipInsideRaisesFails;
begin
  // ESkipTest raised inside AssertRaises is another class: FAIL, not SKIP
  AssertRaises<EConvertError>(
    procedure
    begin
      Skip('too late');
    end);
end;

[Test]
procedure NearPasses;
var
  C: Currency;
begin
  AssertNear(0.1 + 0.2, 0.3, 1e-9);
  AssertNear(1, 2, 1);
  C := 1.5;
  AssertNear(C, 1.5001, 0.0001);
end;

[Test]
procedure NearFails;
begin
  AssertNear(1.25, 1.5, 0.1);
end;

[Test]
procedure NearNaNFails;
begin
  AssertNear(0.0 / 0.0, 0.0 / 0.0, 1);
end;

[Test]
procedure NearInfinityFails;
begin
  // An infinity is near only the same infinity, even within an infinite tolerance
  AssertNear(1.0 / 0.0, 1.0 / 0.0, 0);
  AssertNear(1.0 / 0.0, 1, 1.0 / 0.0);
end;

[Test]
procedure NearNegativeTolerance;
begin
  AssertNear(1, 1, -1);
end;

end.
