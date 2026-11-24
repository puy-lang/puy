// `puy test --update` writes the snapshots that differ or are missing (with the position of the
// call on the first line, and the line endings made LF) and passes those tests, leaves the
// matching ones as they are, fails a test that uses one file twice instead of overwriting the
// first value, and counts the snapshot files of the unit that no test used without deleting
// them, leaving out the files of a unit whose name begins with this one's (docs/spec/test.md 12).
unit Upd_test;

interface

implementation

uses
  Puy.Testing;

procedure Check(const S: string);
begin
  AssertSnapshot(S, 'helper');
end;

[Test]
procedure Same;
begin
  AssertSnapshot('same' + #10);
end;

[Test]
procedure Changed;
begin
  AssertSnapshot('new' + #13#10 + 'lines' + #13#10);
end;

[Test]
procedure Added;
begin
  Check('through a helper');
end;

[Test]
procedure Twice;
begin
  AssertSnapshot('named two', '2');
  AssertSnapshot('first');
  // The second unnamed snapshot is the file of the name '2'
  AssertSnapshot('second');
end;

end.
