// A test skipped while it runs is still an ERROR when a `finalization` raises afterwards; the
// report of the ESkipTest comes first (docs/spec/test.md 4).
unit Closer_test;

interface

implementation

uses
  Puy.Testing, Closer;

[Test]
procedure SkipsThenFinalizationFails;
begin
  Touch;
  Skip('not ready');
end;

end.
