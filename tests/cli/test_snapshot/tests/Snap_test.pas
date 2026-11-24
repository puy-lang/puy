// AssertSnapshot under `puy test`: a matching snapshot passes; a different one fails with a
// unified diff; a missing one fails and names the file; the unnamed snapshots of a test are
// numbered in the order of the calls; a named one and a [TestCase] get their own files; the
// line endings are compared as LF; and a bad name is EArgumentException
// (docs/spec/test.md 12, docs/spec/stdlib/Puy.Testing/README.md 5).
unit Snap_test;

interface

implementation

uses
  System.SysUtils, Puy.Testing;

type
  TReport = class
  public
    [Test]
    procedure Matches;
  end;

procedure TReport.Matches;
begin
  AssertSnapshot('total: 3' + #10);
end;

[Test]
procedure Differs;
begin
  AssertSnapshot('one' + #10 + 'TWO' + #10 + 'three' + #10);
end;

[Test]
procedure Missing;
begin
  AssertSnapshot('new value');
end;

[Test]
procedure Numbered;
begin
  AssertSnapshot('first');
  AssertSnapshot('second');
  AssertSnapshot('named', 'extra');
end;

[TestCase(1)]
[TestCase(2)]
procedure Cases(N: Integer);
begin
  AssertSnapshot('case ' + IntToStr(N));
end;

[Test]
procedure LineEndings;
begin
  AssertSnapshot('a' + #13#10 + 'b' + #13 + 'c');
end;

[Test]
procedure BadName;
begin
  AssertSnapshot('x', 'a/b');
end;

end.
