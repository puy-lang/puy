// With `--update` and `--format json`, the report counts the snapshots written and unused
// (docs/spec/test.md 10, 12); the folder of the snapshots is made when it is missing.
unit Json_test;

interface

implementation

uses
  Puy.Testing;

[Test]
procedure Writes;
begin
  AssertSnapshot('written');
end;

end.
